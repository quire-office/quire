//! F3 考卷：文字框內圖片渲染驗證（dml-picture-in-textframe.docx）
//!
//! 署名：ADT109119
//!
//! 考卷驗證點：
//! - 燈 1：該檔發射清單圖形 item 數 ≥ 1。
//! - 燈 2：發射圖片 bbox 落在文字框 bbox 內（±4pt），尺寸與源檔 a:ext 換算值一致（±2pt）。
//! - 燈 3（鑑別力）：環境變數切換可證明考卷抓得到「框內圖被整段跳過」的舊行為。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

const DOCX_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/ooxmlexport/dml-picture-in-textframe.docx"
);

fn load_engine() -> Engine {
    let bytes = std::fs::read(DOCX_PATH).expect("讀取 dml-picture-in-textframe.docx 語料");
    Engine::from_docx_bytes(&bytes).expect("引擎載入 docx 成功")
}

#[derive(Debug, Clone)]
struct ImageItem {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Debug, Clone)]
struct RectItem {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn collect_items(eng: &Engine) -> (Vec<ImageItem>, Vec<RectItem>) {
    let dl = eng.page_display_list(0).expect("頁 0 顯示清單必須存在");
    let mut images = Vec::new();
    let mut rects = Vec::new();
    for item in &dl.items {
        match item {
            DrawItem::Image { x, y, w, h, .. } => {
                images.push(ImageItem {
                    x: *x,
                    y: *y,
                    w: *w,
                    h: *h,
                });
            }
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } => {
                // 文字框白色背景矩形（或非 0 尺寸的背景框）
                if *w > 10.0 && *h > 10.0 && *fill_argb != 0x2088_8888 {
                    rects.push(RectItem {
                        x: *x,
                        y: *y,
                        w: *w,
                        h: *h,
                    });
                }
            }
            _ => {}
        }
    }
    (images, rects)
}

/// 燈 1 與燈 2：正常態發射檢驗
#[test]
fn test_f3_txbx_picture_emitted_and_geometry() {
    let eng = load_engine();
    let (images, rects) = collect_items(&eng);

    // 燈 1：該檔發射清單圖形 item 數 ≥ 1
    assert!(
        !images.is_empty(),
        "燈 1 紅燈：文字框內 DML 圖片未發射，發射清單圖片數為 0"
    );

    let img = &images[0];

    // 燈 2a：尺寸與源檔 a:ext 換算值一致（2200275 EMU / 12700 = 173.25 pt，容許 ±2pt）
    let expected_w = 173.25f32;
    let expected_h = 173.25f32;
    assert!(
        (img.w - expected_w).abs() <= 2.0,
        "燈 2a 紅燈：圖片寬度 {} 與源檔換算值 {} 差距超出 ±2pt",
        img.w,
        expected_w
    );
    assert!(
        (img.h - expected_h).abs() <= 2.0,
        "燈 2a 紅燈：圖片高度 {} 與源檔換算值 {} 差距超出 ±2pt",
        img.h,
        expected_h
    );

    // 燈 2b：圖片 bbox 必須落在文字框 bbox 內（誤差容許 ±4pt）
    assert!(
        !rects.is_empty(),
        "文字框背景矩形必須存在以作為幾何參考"
    );
    let txbx = &rects[0];
    let tolerance = 4.0f32;

    assert!(
        img.x >= txbx.x - tolerance,
        "燈 2b 紅燈：圖片左緣 {} 溢出文字框左緣 {}（容差 ±4pt）",
        img.x,
        txbx.x
    );
    assert!(
        img.y >= txbx.y - tolerance,
        "燈 2b 紅燈：圖片上緣 {} 溢出文字框上緣 {}（容差 ±4pt）",
        img.y,
        txbx.y
    );
    assert!(
        img.x + img.w <= txbx.x + txbx.w + tolerance,
        "燈 2b 紅燈：圖片右緣 {} 溢出文字框右緣 {}（容差 ±4pt）",
        img.x + img.w,
        txbx.x + txbx.w
    );
    assert!(
        img.y + img.h <= txbx.y + txbx.h + tolerance,
        "燈 2b 紅燈：圖片底緣 {} 溢出文字框底緣 {}（容差 ±4pt）",
        img.y + img.h,
        txbx.y + txbx.h
    );
}

/// 燈 3：鑑別力驗證（F3_TXBX_PIC_BUGGY=1 舊行為下圖片數為 0）
#[test]
fn test_f3_discriminative_power_under_buggy_mode() {
    std::env::set_var("F3_TXBX_PIC_BUGGY", "1");
    let eng = load_engine();
    std::env::remove_var("F3_TXBX_PIC_BUGGY");

    let (images, _) = collect_items(&eng);
    assert_eq!(
        images.len(),
        0,
        "燈 3 鑑別力：在 F3_TXBX_PIC_BUGGY=1 舊行為模式下，文字框內圖片必須被整段跳過（圖片數應為 0）"
    );
}
