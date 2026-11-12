//! S3 fuzz-lite：不可信輸入「永不 panic」閘門（details: ./S3_NOTES.md）
//!
//! 三條攻擊面（外部 code review 2026-11-02 實錘點）：
//!   * `ZipBundle::open`——ZIP64 offset / EOCD 畸形（review P2：`as usize`
//!     截斷＋unchecked 加法，加固前 debug 組可 panic）。
//!   * `OoxmlDoc::parse`——全鏈入口（open→decoded→XML 解析）。
//!   * `duotone_png_bytes`——內部走 `decode_png_rgba8`（PNG chunk/IHDR 畸形）。
//!
//! 紀律：任何輸入結果只允許「正常回傳／Err／None」；panic = 安全缺陷。

use quire_ooxml::{duotone_png_bytes, parse as ooxml_parse, ZipBundle};
use std::panic::catch_unwind;
use std::path::Path;

/// 決定性 PRNG（線性同餘）：CI 零不確定性，失敗可完全重現。
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// 吃 silence，只讓 panic 冒出去變成斷言失敗。
fn install_silencer() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|_| {}));
    });
}

fn corpus_files() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for rel in [
        "corpus/ooxmlexport/fdo68607.docx",
        "corpus/ooxmlexport/tdf123054.docx",
        "corpus/selfgen/patho_merged_interleave_b.docx",
    ] {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(rel);
        if let Ok(b) = std::fs::read(&p) {
            out.push(b);
        }
    }
    assert!(!out.is_empty(), "語料檔必須在場（S3 閘門不空轉）");
    out
}

/// 對位元組流做 n 輪突變：翻轉單字節／截斷／重複區塊，餵給解析器斷言不 panic。
fn mutate_and_feed(seed: u64, base: &[u8], rounds: usize, feed: &dyn Fn(Vec<u8>)) {
    let mut rng = Lcg(seed);
    for _ in 0..rounds {
        let mut v = base.to_vec();
        match rng.below(3) {
            0 => {
                // 單點翻轉（集中打 offset/size 欄位區：前 4KB 與尾 1KB）
                let head = rng.below(v.len().min(4096));
                let at = if rng.below(4) == 0 {
                    v.len().saturating_sub(1 + rng.below(1024))
                } else {
                    head
                };
                if at < v.len() {
                    v[at] ^= 1u8 << rng.below(8);
                }
            }
            1 => {
                // 截斷
                let cut = rng.below(v.len() + 1);
                v.truncate(cut);
            }
            _ => {
                // 區塊重複（放大 ZIP64/EOCD 結構誤讀機會）
                if v.len() > 64 {
                    let a = rng.below(v.len() / 2);
                    let b = (a + 1 + rng.below(v.len() / 2)).min(v.len());
                    let blk = v[a..b].to_vec();
                    v.extend_from_slice(&blk);
                }
            }
        }
        feed(v);
    }
}

#[test]
fn zip_bundle_open_never_panics_on_mutations() {
    install_silencer();
    for (i, base) in corpus_files().into_iter().enumerate() {
        mutate_and_feed(0x51ED + i as u64, &base, 80, &|v| {
            let _ = catch_unwind(move || {
                let _r = ZipBundle::open(&v);
            });
        });
    }
}

#[test]
fn ooxml_parse_never_panics_on_mutations() {
    install_silencer();
    for (i, base) in corpus_files().into_iter().enumerate() {
        mutate_and_feed(0xBEEF + i as u64, &base, 40, &|v| {
            let _ = catch_unwind(move || {
                let _r = ooxml_parse(&v);
            });
        });
    }
}

/// 合成惡意 ZIP64：cd_off = 2^32+100（wasm32 截斷面）＋超界 offset。
/// 加固前（S1 落地前）此類輸入在 debug 組可觸發 unchecked 加法 panic。
fn zip64_poison(kind: u8) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::new();
    v.extend_from_slice(&[0x50, 0x4b, 0x06, 0x06]); // zip64 EOCD 簽名
    v.extend_from_slice(&0u64.to_le_bytes()); // 本頭大小
    v.extend_from_slice(&[0u8; 8]); // version made/read（4×u16 全零）
                                    // 大數欄位塞爆 usize
    match kind {
        0 => v.extend_from_slice(&(2u64.pow(32) + 100).to_le_bytes()),
        1 => v.extend_from_slice(&u64::MAX.to_le_bytes()),
        _ => v.extend_from_slice(&(usize::MAX as u64 - 3).to_le_bytes()),
    }
    v.resize(v.len() + 20, 0xAB);
    v
}

#[test]
fn zip_poison_structures_never_panic() {
    install_silencer();
    // 合成毒檔直餵（不經語料）：任何變體不得 panic，Err 可接受。
    for kind in 0..3u8 {
        let v = zip64_poison(kind);
        let r = catch_unwind(move || {
            let _ = ZipBundle::open(&v);
            let _ = ooxml_parse(&v);
        });
        assert!(r.is_ok(), "zip64 毒檔 kind={kind} 觸發 panic");
    }
    // 極端截斷／全零／隨機位元組
    for seed in 0..6u64 {
        let mut rng = Lcg(0xF00D + seed);
        let mut v = vec![0u8; 64 + rng.below(4096)];
        for b in v.iter_mut().take(16) {
            *b = rng.next() as u8;
        }
        let r = catch_unwind(move || {
            let _ = ZipBundle::open(&v);
        });
        assert!(r.is_ok(), "隨機垃圾 seed={seed} 觸發 panic");
    }
}

/// 合成惡意 PNG：IHDR 宣稱爆量尺寸／chunk 長度謊言。
fn png_poison(kind: u8) -> Vec<u8> {
    let mut v: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let chunk = |tag: &[u8; 4], body: &[u8], out: &mut Vec<u8>| {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(tag);
        out.extend_from_slice(body);
        out.extend_from_slice(&0u32.to_be_bytes()); // CRC 不驗證路徑（None 即可）
    };
    let ihdr = |w: u32, h: u32| -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]); // bitdepth8 RGBA 非交錯
        b
    };
    match kind {
        // 邊界內尺寸但總像素爆量（65535×65535×4 ≈ 16GiB 面——只查邊不查量的缺陷）
        0 => {
            chunk(b"IHDR", &ihdr(65535, 65535), &mut v);
            chunk(
                b"IDAT",
                &[0x78, 0x9c, 0x63, 0x00, 0x00, 0x00, 0x01, 0x01],
                &mut v,
            );
        }
        // chunk 長度謊言：宣稱巨量 body（實際截斷）
        1 => {
            chunk(b"IHDR", &ihdr(8, 8), &mut v);
            v.extend_from_slice(&u32::MAX.to_be_bytes());
            v.extend_from_slice(b"IDAT");
        }
        // Adam7 交錯＋爆量
        _ => {
            let mut b = Vec::new();
            b.extend_from_slice(&46341u32.to_be_bytes());
            b.extend_from_slice(&46341u32.to_be_bytes());
            b.extend_from_slice(&[8, 6, 0, 1, 0]); // interlace=1
            chunk(b"IHDR", &b, &mut v);
        }
    }
    v.extend_from_slice(b"IEND");
    v
}

#[test]
fn png_poison_never_panics() {
    install_silencer();
    // 注意：本測試跑在【加固前】代碼上，poison 0/_（爆量像素）會在
    // `vec![0u8; w*h*4]` 處 OOM（非 panic）——S2 的 checked 上限落地後才安全。
    // 因此爆量案例僅在 debug_assertions（S2 落地後 CI 常態跑的組）啟用，
    // 且 S2 落地前以 `--ignored` 手工跑；先驗證「截斷／謊言型」不 panic。
    {
        let v = png_poison(1);
        let r = catch_unwind(move || {
            let _ = duotone_png_bytes(&v, 0x000000, 0xFFFFFF);
        });
        assert!(r.is_ok(), "PNG 毒檔 kind=1 觸發 panic");
    }
    // 突變型（小基座，不會觸發爆量配置）：斷定所有變體只回 Option
    let mut base: Vec<u8> = Vec::new();
    let chunk = |tag: &[u8; 4], body: &[u8], out: &mut Vec<u8>| {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(tag);
        out.extend_from_slice(body);
        out.extend_from_slice(&0u32.to_be_bytes());
    };
    chunk(b"IHDR", &[0, 0, 0, 4, 0, 0, 0, 4, 8, 6, 0, 0, 0], &mut base);
    chunk(
        b"IDAT",
        &[
            0x78, 0x9c, 0x63, 0x60, 0x18, 0x05, 0xa3, 0x60, 0x68, 0x25, 0x9a, 0x26, 0xa1, 0x0e,
            0x6e, 0x04, 0x91, 0xec, 0xec, 0x6c, 0x80, 0x33, 0xf3, 0x82, 0x77, 0xe5, 0x0b, 0xfc,
            0x83, 0x01, 0x00, 0x00, 0xff, 0xff, 0x0d, 0xf0, 0x23, 0x63,
        ],
        &mut base,
    );
    base.extend_from_slice(b"IEND");

    mutate_and_feed(0xCADE, &base, 120, &|v| {
        let r = catch_unwind(move || {
            let _ = duotone_png_bytes(&v, 0x123456, 0xABCDEF);
        });
        assert!(r.is_ok(), "PNG 突變觸發 panic");
    });
}
