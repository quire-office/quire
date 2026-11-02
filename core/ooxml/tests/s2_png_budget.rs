//! S2：PNG 像素預算與解碼安全回歸測試（紅燈與邊界加固驗證）。
//!
//! 1. 65536×65536 邊長合法但總量爆表假圖 → 返回 None 且不 panic（debug 組驗證）。
//! 2. 4096×4096 在預算內真圖 → 正常解出 RGBA（無誤殺正常檔案）。
//! 3. 預算調爆回塞（`S2_PNG_BUDGET_BUGGY=1`）→ 紅燈必紅。
//! 4. zlib bomb 輸出上限截斷與 chunk 走訪加法防溢位。

use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

#[path = "../src/png_duotone.rs"]
mod png_duotone;

use png_duotone::{decode_png_rgba8, duotone_png_bytes, MAX_TOTAL_PIXELS};

#[cfg(target_arch = "wasm32")]
const OVER_BUDGET_W: u32 = 4096;
#[cfg(target_arch = "wasm32")]
const OVER_BUDGET_H: u32 = 4097; // 4096 * 4097 = 16_781_312 px > 16M px

#[cfg(not(target_arch = "wasm32"))]
const OVER_BUDGET_W: u32 = 8192;
#[cfg(not(target_arch = "wasm32"))]
const OVER_BUDGET_H: u32 = 8193; // 8192 * 8193 = 67_117_056 px > 64M px

/// 寫入 PNG chunk（長度 + 標記 + 內容 + CRC32）。
fn write_chunk(out: &mut Vec<u8>, typ: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(typ);
    out.extend_from_slice(body);
    let mut crc = crc32fast::Hasher::new();
    crc.update(typ);
    crc.update(body);
    out.extend_from_slice(&crc.finalize().to_be_bytes());
}

/// 合成指定尺寸的假 PNG（僅 IHDR、微小合法 IDAT 與 IEND）。
fn make_fake_ihdr_png(w: u32, h: u32, interlace: u8) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&w.to_be_bytes());
    ihdr[4..8].copy_from_slice(&h.to_be_bytes());
    ihdr[8] = 8; // bit depth 8
    ihdr[9] = 6; // color type 6 (RGBA)
    ihdr[10] = 0; // compression 0
    ihdr[11] = 0; // filter 0
    ihdr[12] = interlace;
    write_chunk(&mut out, b"IHDR", &ihdr);

    let enc = ZlibEncoder::new(Vec::new(), Compression::fast());
    let idat_data = enc.finish().unwrap();
    write_chunk(&mut out, b"IDAT", &idat_data);
    write_chunk(&mut out, b"IEND", b"");
    out
}

/// 合成真實可解碼的灰階 PNG（使用 deflate 壓縮全零行，耗時 <50ms）。
fn make_real_png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&w.to_be_bytes());
    ihdr[4..8].copy_from_slice(&h.to_be_bytes());
    ihdr[8] = 8; // bit depth 8
    ihdr[9] = 0; // color type 0 (Grayscale, 1 byte/pixel)
    ihdr[10] = 0;
    ihdr[11] = 0;
    ihdr[12] = 0; // 非交錯
    write_chunk(&mut out, b"IHDR", &ihdr);

    let mut enc = ZlibEncoder::new(Vec::new(), Compression::fast());
    let row = vec![0u8; (w as usize) + 1]; // filter 0 (1 byte) + pixel data (w bytes)
    for _ in 0..h {
        enc.write_all(&row).unwrap();
    }
    let idat_data = enc.finish().unwrap();
    write_chunk(&mut out, b"IDAT", &idat_data);
    write_chunk(&mut out, b"IEND", b"");
    out
}

/// 燈 1：合成 IHDR 65536×65536 假 PNG → decode_png_rgba8 回 None 且不 panic。
#[test]
fn s2_huge_ihdr_65536_rejected_without_panic() {
    let png = make_fake_ihdr_png(65536, 65536, 0);
    let res = decode_png_rgba8(&png);
    assert_eq!(res, None, "65536x65536 邊長合法但總量爆表，必須回 None");
    assert_eq!(
        duotone_png_bytes(&png, 0xFF000000, 0xFFFFFFFF),
        None,
        "duotone 包裝函式對爆量圖亦必須安全回 None"
    );
}

/// 燈 1b：合成交錯 Adam7 65536×65536 假 PNG → 回 None 且不 panic。
#[test]
fn s2_huge_ihdr_65536_adam7_rejected_without_panic() {
    let png = make_fake_ihdr_png(65536, 65536, 1);
    let res = decode_png_rgba8(&png);
    assert_eq!(
        res, None,
        "Adam7 65536x65536 邊長合法但總量爆表，必須回 None"
    );
}

/// 燈 2：合成 4096×4096（在預算內）真 PNG → 正常解出 RGBA（無誤殺正常檔）。
#[test]
fn s2_4096_real_png_succeeds() {
    let png = make_real_png(4096, 4096);
    let res = decode_png_rgba8(&png);
    assert!(res.is_some(), "4096x4096 在預算內，必須正常解出");
    let (w, h, rgba) = res.unwrap();
    assert_eq!((w, h), (4096, 4096));
    assert_eq!(rgba.len(), 4096 * 4096 * 4);

    let duo = duotone_png_bytes(&png, 0xFF30_6EA6, 0xFFFF_FFFF);
    assert!(duo.is_some(), "4096x4096 duotone 變換必須成功產出 PNG");
}

/// 燈 3：總像素預算門禁。在正常情況下阻擋超額圖片（回 None）；
/// 當預算調爆（`S2_PNG_BUDGET_BUGGY=1`）時此斷言必破（放行回 Some）→ 紅燈必紅。
#[test]
fn s2_budget_guard_rejects_over_budget() {
    let over_budget_png = make_real_png(OVER_BUDGET_W, OVER_BUDGET_H);
    let res = decode_png_rgba8(&over_budget_png);
    assert!(
        res.is_none(),
        "超過總像素預算上限（{} px，當前 {} px）之圖片必須被門禁攔截回 None！若 S2_PNG_BUDGET_BUGGY=1 預算被調爆則放行導致紅燈！",
        MAX_TOTAL_PIXELS,
        OVER_BUDGET_W as u64 * OVER_BUDGET_H as u64
    );
}

/// 燈 4：BUGGY 回塞鎖。當設有 `S2_PNG_BUDGET_BUGGY=1` 時，實證超額圖片確實被放行，
/// 證明 `s2_budget_guard_rejects_over_budget` 之紅燈鑑別力真實有效。
#[test]
fn s2_buggy_regression_lock() {
    if std::env::var_os("S2_PNG_BUDGET_BUGGY").is_none() {
        eprintln!("未設 S2_PNG_BUDGET_BUGGY：跳過回塞檢驗（以 S2_PNG_BUDGET_BUGGY=1 跑）。");
        return;
    }
    let over_budget_png = make_real_png(OVER_BUDGET_W, OVER_BUDGET_H);
    let res = decode_png_rgba8(&over_budget_png);
    assert!(
        res.is_some(),
        "BUGGY 態（預算調爆）下超額圖片必須被放行（回 Some），實證門禁關閉"
    );
}

/// 燈 5：zlib bomb 防禦：解壓輸出改讀到 expected+1 即棄。
#[test]
fn s2_zlib_bomb_rejected() {
    // 構造 10x10 灰階圖片（預期 uncompressed 僅 (10+1)*10 = 110 位元組）
    // 但 IDAT 內塞入解壓後達 10,000 位元組的炸彈資料
    let mut out = Vec::new();
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&10u32.to_be_bytes());
    ihdr[4..8].copy_from_slice(&10u32.to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = 0;
    write_chunk(&mut out, b"IHDR", &ihdr);

    let mut enc = ZlibEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(&[0u8; 10000]).unwrap();
    let bomb_data = enc.finish().unwrap();
    write_chunk(&mut out, b"IDAT", &bomb_data);
    write_chunk(&mut out, b"IEND", b"");

    let res = decode_png_rgba8(&out);
    assert_eq!(
        res, None,
        "zlib 解壓超過 expected 的膨脹串流必須被即刻捨棄回 None"
    );
}

/// 燈 6：chunk 走訪加法防溢位：畸形 chunk 長度（0xFFFF_FFFF）不得 panic。
#[test]
fn s2_chunk_overflow_rejected() {
    let mut out = Vec::new();
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&10u32.to_be_bytes());
    ihdr[4..8].copy_from_slice(&10u32.to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = 0;
    write_chunk(&mut out, b"IHDR", &ihdr);

    // 塞入 length = 0xFFFF_FFFF 的異常 chunk
    out.extend_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    out.extend_from_slice(b"tEXt");
    out.extend_from_slice(b"dummy");

    let res = decode_png_rgba8(&out);
    assert_eq!(
        res, None,
        "chunk 加法溢位與長度異常必須安全回 None 且不 panic"
    );
}
