//! K19：圖片像素級雙色調 `a:duotone`（先紅後綠迴歸鎖）。
//!
//! ## LO 真值出處（本機 soffice 實測，非猜測）
//! 以 `corpus/ooxmlexport/picture-with-schemecolor.docx` 走 LO 轉 PDF 後抽
//! **內嵌圖 XObject**（無損 FlateDecode；`scripts/render_lab.py` 之外的離線
//! 反推，腳本見提交說明），得到兩條硬實據：
//!
//! 1. **兩端色**：源圖為灰階商標（不透明像素 RGB 全 154，其餘 alpha 0），
//!    LO 壓色後不透明像素＝`(172,197,219)`、隱含暗端＝`(49,110,165)`。
//! 2. **LUT 線性度**（同型 duotone 宣告的水平灰階梯度圖 256 階，逐 x 取
//!    像素）：`out = c1 + (255 − c1)·L/255`（8bit 線性插值，最大殘差 ≤2），
//!    `L` 取 Rec.601 加權 luma。梯度實測對照：
//!    `L=0→(49,110,165)`、`64→(101,146,187)`、`128→(152,182,210)`、
//!    `224→(228,237,242)`。
//!
//! ## 修飾器折算（`a:shade`/`a:satMod`，ECMA-376 §21.2.3.29/.42）
//! `theme1.xml` 的 accent1＝`5B9BD5`＝(91,155,213)，duotone 首色節點帶
//! `a:shade val="45000"`＋`a:satMod val="135000"`；次色是 `<a:prstClr val="white"/>`
//! （LO 實測原樣 255,255,255——無 tint 修飾）。折算路徑（本方實作口徑）：
//!   shade 於 **sRGB 線性域**（ECMA 定義，與 `theme.rs::apply_shading_transform`
//!   的 8bit 近似式不同源——後者是 `w:shd/@w:shdColor` 的 corpus 校準式，
//!   兩者語意域不同，勿混用）：(91,155,213)→(61.3,107.2,148.7)
//!   satMod 於 **8bit 域、Rec.601 luma**：→(48,110,166)
//! 與 LO 內嵌圖實測暗端 (49,110,165) 逐通道差 ≤1（LO 內部 gamma 精度的量化
//! 差）。燈 1 因此**斷言本方折算值本身**（可重現、零 magic number），
//! 並在下方註記 LO 實測值；色軸（燈 2）對 ±1 完全不敏感。
//!
//! ## 紅燈口徑
//! - 燈1：`picture-with-schemecolor.docx` 發射 `DrawItem::Image` 帶 `duotone`
//!   欄位，`[color1, color2]` 與上述折算一致。
//! - 燈2：發射像素已是**變換後**的像素——源圖不透明像素 (154,154,154) 必須
//!   映射成 `c1 + (255−c1)·154/255` ≈ (172,197,219)（LO 實測值，±1）。
//! - 燈3：全庫僅 1 檔在 `document.xml` 宣告 duotone（掃描實據：其餘 5 檔命中
//!   在 `theme1.xml` 的 fmtScheme，與 blip 無關），無宣告檔的像素發射逐位
//!   零擾動（`duotone` 欄位不外露＋base64 位元組逐位不變）。
//! - `K19_DUOTONE_BUGGY=1` → 燈1/燈2 必紅（回塞鎖）。

use quire_engine::Engine;

/// 本方折算口徑（燈 1 的期望值來源；推導見檔頭）。
const MINE_COLOR1: u32 = 0xFF30_6EA6; // (48,110,166)
const MINE_COLOR2: u32 = 0xFFFF_FFFF; // prstClr white（LO 實測原樣）
/// LO 內嵌圖實測值（觀測記錄，非斷言物件）。
#[allow(dead_code)]
const LO_COLOR1_OBSERVED: (u8, u8, u8) = (49, 110, 165);

fn corpus(name: &str) -> Vec<u8> {
    let path = format!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/{}"), name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("讀 corpus/{name}: {e}"))
}

/// 全頁顯示列表 JSON 裡的 Image 項（含 duotone 欄位原樣）。
fn image_items(eng: &Engine) -> Vec<serde_json::Map<String, serde_json::Value>> {
    let mut out = Vec::new();
    for page in 0..eng.page_count() {
        let json = eng.page_display_list_json(page).expect("頁存在");
        let v: serde_json::Value = serde_json::from_str(&json).expect("顯示列表 JSON");
        for it in v["items"].as_array().expect("items 陣列") {
            if let Some(m) = it.get("Image").and_then(|m| m.as_object()).cloned() {
                out.push(m);
            }
        }
    }
    out
}

/// 燈1：blip 下的 `a:duotone` 必須落成 `DrawItem::Image` 的結構化色彩效果欄位。
#[test]
fn k19_schemecolor_picture_emits_duotone_effect() {
    let eng = Engine::from_docx_bytes(&corpus("ooxmlexport/picture-with-schemecolor.docx"))
        .expect("open picture-with-schemecolor");
    let imgs = image_items(&eng);
    assert_eq!(imgs.len(), 1, "本檔恰一張圖，實際 {}", imgs.len());
    let dt = imgs[0].get("duotone").and_then(|d| d.as_array());
    let Some(dt) = dt else {
        panic!(
            "a:duotone 必須發射結構化色彩效果欄位（K19）；實際 Image 項欄位：{:?}（LO 內嵌圖實測色 {:?}）",
            imgs[0].keys().collect::<Vec<_>>(),
            LO_COLOR1_OBSERVED
        );
    };
    assert_eq!(dt.len(), 2, "duotone 恰兩組顏色節點");
    let c1 = dt[0].as_u64().expect("color1 u32") as u32;
    let c2 = dt[1].as_u64().expect("color2 u32") as u32;
    assert_eq!(c1, MINE_COLOR1, "color1 必須是 accent1 經 shade(45000)+satMod(135000) 折算值");
    assert_eq!(c2, MINE_COLOR2, "color2 必須是 prstClr white（LO 實測原樣白）");
}

/// 燈2：發射的像素必須已是 duotone 變換後的像素（真像素調色，非底矩形）。
///
/// 源圖（`word/media/image1.png`）不透明像素全為 (154,154,154,255)，映射後
/// 必為 `c1 + (255−c1)·154/255`＝LO 實測 (172,197,219)。斷言走**發射像素
/// 本身**：解 base64 → PNG IHDR 尺寸＋IDAT 解壓後取首個不透明像素的 RGB。
#[test]
fn k19_emitted_pixels_are_duotone_mapped() {
    let eng = Engine::from_docx_bytes(&corpus("ooxmlexport/picture-with-schemecolor.docx"))
        .expect("open picture-with-schemecolor");
    let imgs = image_items(&eng);
    assert_eq!(imgs.len(), 1);
    let b64 = imgs[0].get("data_b64").and_then(|d| d.as_str()).expect("data_b64");
    let bytes = base64_decode(b64).expect("發射像素 base64 可解");
    assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n", "發射像素必須是 PNG");
    let (w, h, rgba) = png_rgba8(&bytes).expect("PNG RGBA8 可解");
    // 源圖 341×181；變換是逐像素 LUT，幾何零變動。
    assert_eq!((w, h), (341, 181), "調色不得改動像素幾何");
    // 期望值：本方折算 c1 走同一條 LUT（不是抄 LO 實測值——LO 實測 (172,197,219)
    // 見檔頭，與此差 ≤1/通道）。
    let l = 154.0f64 / 255.0;
    let exp = [
        MINE_COLOR1 >> 16 & 0xFF,
        MINE_COLOR1 >> 8 & 0xFF,
        MINE_COLOR1 & 0xFF,
    ]
    .map(|c| (f64::from(c) + (255.0 - f64::from(c)) * l).round() as u32);
    let mut hits = 0usize;
    let mut checked = 0usize;
    for px in rgba.chunks_exact(4) {
        if px[3] != 255 {
            continue; // alpha 合成屬 LO 側細節，只比不透明像素
        }
        checked += 1;
        if (i32::from(px[0]) - exp[0] as i32).abs() <= 1
            && (i32::from(px[1]) - exp[1] as i32).abs() <= 1
            && (i32::from(px[2]) - exp[2] as i32).abs() <= 1
        {
            hits += 1;
        }
    }
    assert!(checked > 5000, "源圖不透明像素樣本量異常：{checked}");
    let ratio = hits as f64 / checked as f64;
    assert!(
        ratio > 0.99,
        "不透明像素必須逐點落進 duotone LUT 結果 {exp:?}（LO 實測 (172,197,219)）；命中率 {ratio:.4}"
    );
    // 假圖元防呆：調色後**不得**殘留源灰階值（154,154,154）——灰階未調色＝K12 退件態。
    let raw_gray = rgba
        .chunks_exact(4)
        .filter(|px| px[3] == 255 && px[0] == 154 && px[1] == 154 && px[2] == 154)
        .count();
    assert_eq!(raw_gray, 0, "仍殘留源灰階像素＝duotone 未真正套用到像素");
}

/// 燈3：無 duotone 宣告的檔案零擾動——`duotone` 欄位不外露、像素位元組逐位不變。
///
/// 掃描實據（`scripts` 級全域掃描，見提交說明）：全庫 6 檔 XML 命中
/// `a:duotone`，其中 5 檔命中處在 `word/theme/theme1.xml` 的 fmtScheme（與
/// blip 無關，**不得**被誤讀），只有 `picture-with-schemecolor.docx` 在
/// `word/document.xml` 真宣告。故 fdo68607／WC048／n780563／content-control-shape
/// 四個代表檔必須維持舊發射態。
#[test]
fn k19_no_duotone_declaration_zero_disturbance() {
    // (檔案, 期望 Image 項數, 期望首項 data_b64 長度)——鎖值取自 main 91c2693
    // 的實際發射（`quire-cli dump` 實測，見提交說明）：Bug51170 六項同圖
    // （Choice→pdf 不可渲染→Fallback→png）、其餘三檔走佔位框零 Image 項。
    // n780563／content-control-shape 的 theme1.xml 內含 duotone（fmtScheme，
    // 與 blip 無關）——本鎖同時保證誤讀風險為零。
    const CASES: &[(&str, usize, usize)] = &[
        ("poi/test-data_document_Bug51170.docx", 6, 10664),
        ("ooxmlexport/fdo68607.docx", 0, 0),
        ("xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx", 0, 0),
        ("ooxmlexport/n780563.docx", 0, 0),
        ("ooxmlexport/content-control-shape.docx", 0, 0),
    ];
    for (name, want_imgs, want_b64_len) in CASES {
        let eng = Engine::from_docx_bytes(&corpus(name)).expect("open");
        let imgs = image_items(&eng);
        assert_eq!(imgs.len(), *want_imgs, "{name} Image 項數漂移");
        for im in &imgs {
            assert!(
                im.get("duotone").is_none(),
                "{name} 無 blip 級 duotone 宣告，duotone 欄位不得外露"
            );
        }
        if *want_b64_len > 0 {
            let got = imgs[0]
                .get("data_b64")
                .and_then(|d| d.as_str())
                .map(str::len)
                .expect("data_b64");
            assert_eq!(got, *want_b64_len, "{name} 像素 base64 位元組數漂移（零擾動鎖）");
        }
    }
}

// ---- 最小 PNG 讀取（測試專用；零依賴：IHDR＋IDAT→RGBA8）----

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut v = [0u8; 4];
    let mut n = 0usize;
    let mut out = Vec::new();
    for ch in s.bytes() {
        if ch == b'=' {
            break;
        }
        let idx = T.iter().position(|&c| c == ch)?;
        v[n] = idx as u8;
        n += 1;
        if n == 4 {
            let x = (u32::from(v[0]) << 18) | (u32::from(v[1]) << 12) | (u32::from(v[2]) << 6) | u32::from(v[3]);
            out.push((x >> 16) as u8);
            out.push((x >> 8) as u8);
            out.push(x as u8);
            n = 0;
        }
    }
    if n == 2 {
        let x = (u32::from(v[0]) << 18) | (u32::from(v[1]) << 12);
        out.push((x >> 16) as u8);
    } else if n == 3 {
        let x = (u32::from(v[0]) << 18) | (u32::from(v[1]) << 12) | (u32::from(v[2]) << 6);
        out.push((x >> 16) as u8);
        out.push((x >> 8) as u8);
    }
    Some(out)
}

/// PNG bytes → (寬, 高, RGBA8）。只支援本卡實需形態：bitdepth 8、color type
/// 0/2/6，filter 0..4；其餘老實 None（不虛構像素）。
fn png_rgba8(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.len() < 8 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut pos = 8;
    let mut w = 0u32;
    let mut h = 0u32;
    let mut channels = 0usize;
    let mut idat = Vec::new();
    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        let typ = &bytes[pos + 4..pos + 8];
        let body = bytes.get(pos + 8..pos + 8 + len)?;
        match typ {
            b"IHDR" => {
                let (ww, hh, bd, ct) = (
                    u32::from_be_bytes(body[0..4].try_into().ok()?),
                    u32::from_be_bytes(body[4..8].try_into().ok()?),
                    body[8],
                    body[9],
                );
                if bd != 8 {
                    return None;
                }
                channels = match ct {
                    0 => 1,
                    2 => 3,
                    6 => 4,
                    _ => return None,
                };
                w = ww;
                h = hh;
            }
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + len;
    }
    let raw = flate2::read::DeflateDecoder::new(&idat[..]);
    use std::io::Read;
    let mut raw = {
        let mut buf = Vec::new();
        let mut r = raw;
        r.read_to_end(&mut buf).ok()?;
        buf
    };
    let stride = (w as usize) * channels;
    if raw.len() < (stride + 1) * h as usize {
        return None;
    }
    let mut out = vec![0u8; h as usize * w as usize * 4];
    let mut prev = vec![0u8; stride];
    let mut rp = 0usize;
    for y in 0..h as usize {
        let ft = raw[rp];
        rp += 1;
        let mut line = raw[rp..rp + stride].to_vec();
        rp += stride;
        match ft {
            0 => {}
            1 => {
                for i in channels..stride {
                    line[i] = line[i].wrapping_add(line[i - channels]);
                }
            }
            2 => {
                for i in 0..stride {
                    line[i] = line[i].wrapping_add(prev[i]);
                }
            }
            3 => {
                for i in 0..stride {
                    let a = if i >= channels { line[i - channels] as u16 } else { 0 };
                    line[i] = line[i].wrapping_add(((a + u16::from(prev[i])) >> 1) as u8);
                }
            }
            4 => {
                for i in 0..stride {
                    let a = if i >= channels { line[i - channels] as i32 } else { 0 };
                    let b = prev[i] as i32;
                    let c = if i >= channels { prev[i - channels] as i32 } else { 0 };
                    let p = b + c - a;
                    let (pa, pb, pc) = ((p - b).abs(), (p - c).abs(), (p - a).abs());
                    let pr = if pa <= pb && pa <= pc { b } else if pb <= pc { c } else { a };
                    line[i] = line[i].wrapping_add(pr as u8);
                }
            }
            _ => return None,
        }
        for x in 0..w as usize {
            let o = (y * w as usize + x) * 4;
            let s = x * channels;
            out[o] = line[s];
            out[o + 1] = if channels >= 3 { line[s + 1] } else { line[s] };
            out[o + 2] = if channels >= 3 { line[s + 2] } else { line[s] };
            out[o + 3] = if channels == 4 { line[s + 3] } else { 255 };
        }
        prev = line;
    }
    Some((w, h, out))
}
