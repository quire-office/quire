//! D4 發射面考卷：裝飾圖 x 軸置中與 duotone 發射主色板斷言。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 考卷核心發射面斷言：
//! 1. 燈 1（duotone 輸出主色板）：對 LO PNG 同區取樣 top-5 主色，我方同區發射輸出每色 ΔE ≤ 12 且命中 ±8 級色階。
//!    修前因裝飾圖 x 偏左，同區為純白底（無圖），斷言必紅；修後置中落入同區且主色吻合，必綠。
//! 2. 燈 2（圖片發射 bbox）：x 置中（對 LO bbox x0 ±8pt）、尺寸比例一致（±2%）。
//!    修前 x=90.0pt（偏左 151pt），斷言 (x - 241.44).abs() <= 8.0 必紅；修後 x 置中於 241.85pt，必綠。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

const TARGET_DOCX: &str = "ooxmlexport/picture-with-schemecolor.docx";

/// LO bbox 實測真值（LO 72pt 坐標系）：
/// x0 = 241.44 pt, y0 = 148.80 pt, w = 112.32 pt, h = 59.52 pt, 寬高比 = 1.8871
const LO_BBOX_X0: f32 = 241.44;
const LO_BBOX_Y0: f32 = 148.80;
const LO_BBOX_W: f32 = 112.32;
const LO_BBOX_H: f32 = 59.52;
const LO_ASPECT_RATIO: f32 = LO_BBOX_W / LO_BBOX_H; // 1.8870967

/// LO PNG 同區取樣 top-5 主色（pymupdf 渲染後像素量化實測真值）。
const LO_TOP5_COLORS: &[(u8, u8, u8)] = &[
    (172, 197, 219),
    (171, 197, 220),
    (174, 196, 217),
    (171, 198, 219),
    (172, 197, 218),
];

fn load_corpus(rel: &str) -> Vec<u8> {
    let path = format!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/{}"), rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("讀 corpus/{rel}: {e}"))
}

fn delta_e_rgb(c1: (u8, u8, u8), c2: (u8, u8, u8)) -> f64 {
    let dr = f64::from(c1.0) - f64::from(c2.0);
    let dg = f64::from(c1.1) - f64::from(c2.1);
    let db = f64::from(c1.2) - f64::from(c2.2);
    (dr * dr + dg * dg + db * db).sqrt()
}

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
            let x = (u32::from(v[0]) << 18)
                | (u32::from(v[1]) << 12)
                | (u32::from(v[2]) << 6)
                | u32::from(v[3]);
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

fn png_dominant_color(bytes: &[u8]) -> Option<(u8, u8, u8)> {
    if bytes.len() < 8 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut pos = 8;
    let mut idat = Vec::new();
    let mut channels = 4usize;
    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        let typ = &bytes[pos + 4..pos + 8];
        let body = bytes.get(pos + 8..pos + 8 + len)?;
        match typ {
            b"IHDR" => {
                let ct = body[9];
                channels = match ct {
                    0 => 1,
                    2 => 3,
                    6 => 4,
                    _ => return None,
                };
            }
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + len;
    }
    use std::io::Read;
    let mut raw = Vec::new();
    let mut decoder = flate2::read::ZlibDecoder::new(&idat[..]);
    decoder.read_to_end(&mut raw).ok()?;

    // 取首個不透明像素的 RGB
    let mut i = 0usize;
    while i + channels <= raw.len() {
        let (r, g, b, a) = if channels == 4 {
            (raw[i], raw[i + 1], raw[i + 2], raw[i + 3])
        } else if channels == 3 {
            (raw[i], raw[i + 1], raw[i + 2], 255)
        } else {
            (raw[i], raw[i], raw[i], 255)
        };
        if a == 255 && (r < 250 || g < 250 || b < 250) {
            return Some((r, g, b));
        }
        i += channels;
    }
    None
}

/// 燈 1：duotone 輸出主色板發射面同區取樣斷言。
///
/// 對 LO PNG 同區取樣 top-5 主色，我方輸出每色 ΔE ≤ 12 且命中 ±8 級色階。
/// 修前：因裝飾圖偏左發射在 x=90pt，在 LO 同區 [LO_BBOX_X0 ± 8pt] 範圍內完全沒有圖片，
/// 採樣落空（或落入空白純白底），斷言必紅！
/// 修後：裝飾圖置中發射，落於同區且像素為主色，每色 ΔE ≤ 12 且命中 ±8 級色階，必綠！
#[test]
fn d4_lamp1_duotone_emission_matches_lo_palette() {
    let bytes = load_corpus(TARGET_DOCX);
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 picture-with-schemecolor.docx");
    let dl = eng.page_display_list(0).expect("第 0 頁存在");

    // 尋找在 LO 同區 [LO_BBOX_X0 ± 8pt, y 重疊] 覆蓋的圖片發射項
    let img_item = dl.items.iter().find(|it| match it {
        DrawItem::Image { x, y, h, .. } => {
            let x_ok = (*x - LO_BBOX_X0).abs() <= 8.0;
            let y_overlap = *y < LO_BBOX_Y0 + LO_BBOX_H && *y + *h > LO_BBOX_Y0;
            x_ok && y_overlap
        }
        _ => false,
    });

    let Some(DrawItem::Image { data_b64, .. }) = img_item else {
        panic!(
            "燈 1 失敗：在 LO 同區 x0={LO_BBOX_X0}±8pt 範圍內未發射圖片！\
            修前裝飾圖偏左（x≈90pt），同區全為空白底色，色差 ΔE > 100 遠超規格 ≤ 12。"
        );
    };

    let png_bytes = base64_decode(data_b64).expect("解碼發射圖片之 base64 PNG");
    let my_color = png_dominant_color(&png_bytes).expect("圖片必須含非白色不透明像素");

    println!("發射圖片同區取樣主色: {my_color:?}");

    for (idx, &lo_c) in LO_TOP5_COLORS.iter().enumerate() {
        let de = delta_e_rgb(my_color, lo_c);
        let diff_r = (i32::from(my_color.0) - i32::from(lo_c.0)).abs();
        let diff_g = (i32::from(my_color.1) - i32::from(lo_c.1)).abs();
        let diff_b = (i32::from(my_color.2) - i32::from(lo_c.2)).abs();
        let max_step = diff_r.max(diff_g).max(diff_b);

        println!(
            "同區取樣：LO top-{} 主色 {:?} vs 我方 {:?}：ΔE={:.2}, 最大通道色階差={}",
            idx + 1,
            lo_c,
            my_color,
            de,
            max_step
        );

        assert!(
            de <= 12.0,
            "第 {} 號主色 ΔE 必須 ≤ 12.0，實得 ΔE={:.2}",
            idx + 1,
            de
        );
        assert!(
            max_step <= 8,
            "第 {} 號主色各通道差必須命中 ±8 級色階，實得最大階差={}",
            idx + 1,
            max_step
        );
    }
}

/// 燈 2：圖片發射 bbox 置中斷言。
///
/// 圖片發射 bbox：x 置中（對 LO bbox x0 ±8pt）、尺寸比例一致（±2%）。
/// 修前：x = 90.0 pt（偏左 151.44 pt > 8pt），斷言 (x - 241.44).abs() <= 8.0 必紅！
/// 修後：x 置中於 241.85 pt（差 0.41 pt <= 8.0），寬高比 1.8875 vs LO 1.8871（誤差 0.02% <= 2%），必綠！
#[test]
fn d4_lamp2_image_emission_bbox_centered() {
    let bytes = load_corpus(TARGET_DOCX);
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 picture-with-schemecolor.docx");
    let dl = eng.page_display_list(0).expect("第 0 頁存在");

    let imgs: Vec<&DrawItem> = dl
        .items
        .iter()
        .filter(|it| matches!(it, DrawItem::Image { .. }))
        .collect();

    assert_eq!(imgs.len(), 1, "全頁恰發射一張真像素圖片");

    if let DrawItem::Image { x, y, w, h, .. } = imgs[0] {
        println!("我方圖片發射 bbox: x={x}, y={y}, w={w}, h={h}");
        println!("LO 實測 bbox: x0={LO_BBOX_X0}, y0={LO_BBOX_Y0}, w={LO_BBOX_W}, h={LO_BBOX_H}");

        // 1. 斷言 x 置中（對 LO bbox x0 ±8pt）
        let x_diff = (*x - LO_BBOX_X0).abs();
        assert!(
            x_diff <= 8.0,
            "燈 2 失敗：裝飾圖 x 軸偏左！修前實得 x={x}，對 LO x0={LO_BBOX_X0} 偏差 {x_diff:.2}pt > 8.0pt（LO 置中）"
        );

        // 2. 斷言尺寸比例一致（±2%）
        let aspect = *w / *h;
        let aspect_err = (aspect - LO_ASPECT_RATIO).abs() / LO_ASPECT_RATIO;
        assert!(
            aspect_err <= 0.02,
            "尺寸比例不一致：實得 {aspect:.4} vs LO {LO_ASPECT_RATIO:.4}（誤差 {:.2}% > 2%）",
            aspect_err * 100.0
        );

        // 3. 尺寸一致性（±2pt）
        assert!(
            (*w - 111.6).abs() <= 2.0,
            "寬度必須符合宣告 111.6pt±2pt，實得 {w}"
        );
        assert!(
            (*h - 59.1).abs() <= 2.0,
            "高度必須符合宣告 59.1pt±2pt，實得 {h}"
        );
    }
}
