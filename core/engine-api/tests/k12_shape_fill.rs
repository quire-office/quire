//! K12 紅燈：形狀填色鏈修復——無文字色塊不再被過濾丟棄（K11 Top1）。
//!
//! 背景：
//! `core/engine-api/src/lib.rs` 的 `shape_boxes` 構造以 `!im.txbx_texts.is_empty()`
//! 過濾，把「無文字但有填色/描邊」的色塊形狀全數丟棄，退化為 `0xFFF0F0F0` 佔位灰或缺發。
//! WC048 頁面上 LO 的藍色粗線（VML 線狀圖元）我方完全沒發，即此病灶（實測色分 0.072）。
//!
//! 紅燈規格：
//! 1. 燈1：`corpus/ooxmlexport/picture-with-schemecolor.docx` 發射清單中必須出現
//!    非佔位灰的填色 Rect（fill_argb 不是 0xFFF0F0F0 且來自該檔宣告色 0xFF5B9BD5）。
//! 2. 燈2：`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` 的藍色線狀圖元
//!    （LO 實渲有、我方現缺）發射且描邊/填色色值與 OOXML 宣告一致（0xFF4472C4）。
//! 3. 燈3：既有含文字文字框檔（WC048 原版與 Mod 檔）文字發射零回退（回歸鎖）。
//! 4. `K12_SHAPE_FILL_BUGGY=1` 環境變數把放寬還原成舊過濾 → 燈1/燈2 必須紅。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(name: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + name;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

/// 燈 1：picture-with-schemecolor.docx 發射清單中必須出現非佔位灰的填色 Rect
/// （fill_argb 不是 0xFFF0F0F0 且來自該檔宣告色 0xFF5B9BD5 accent1）。
#[test]
fn picture_with_schemecolor_emits_colored_rect() {
    let eng = load_corpus("ooxmlexport/picture-with-schemecolor.docx");
    let dl = eng.page_display_list(0).expect("p0 存在");

    let want_color: u32 = 0xFF5B_9BD5;
    let found = dl.items.iter().any(|it| match it {
        DrawItem::Rect { fill_argb, .. } => *fill_argb != 0xFFF0_F0F0 && *fill_argb == want_color,
        _ => false,
    });

    assert!(
        found,
        "picture-with-schemecolor.docx 必須發射非佔位灰的填色 Rect（宣告色 0xFF5B9BD5），實測項目清單為：{:?}",
        dl.items
    );
}

/// 燈 2：WC048 的藍色線狀圖元（LO 實渲有、我方現缺）發射且描邊/填色色值與 OOXML 宣告一致（0xFF4472C4）。
#[test]
fn wc048_blue_linear_primitive_emitted() {
    let eng = load_corpus("xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx");
    let dl = eng.page_display_list(0).expect("p0 存在");

    let want_blue: u32 = 0xFF44_72C4;
    // 藍色線狀圖元：寬度 > 100pt，高度 <= 20pt，色值為 0xFF4472C4
    let found = dl.items.iter().any(|it| match it {
        DrawItem::Rect {
            w, h, fill_argb, ..
        } => *fill_argb == want_blue && *w >= 100.0 && *h <= 20.0,
        DrawItem::Rule {
            x1,
            x2,
            y1,
            y2,
            color_argb,
            ..
        } => *color_argb == want_blue && (x2 - x1).abs() >= 100.0 && (y2 - y1).abs() <= 20.0,
        _ => false,
    });

    assert!(
        found,
        "WC048 必須發射藍色線狀圖元（0xFF4472C4, w>=100, h<=20），實測項目清單為：{:?}",
        dl.items
    );
}

/// 燈 3：既有含文字文字框檔（WC048 原版與 Mod 檔）文字發射零回退（回歸鎖）。
#[test]
fn wc048_text_box_text_zero_regression() {
    for file in [
        "xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx",
        "xopt/TestFiles_WC_WC048-Text-Box-in-Cell-Mod.docx",
    ] {
        let eng = load_corpus(file);
        let mut all_texts = Vec::new();
        for p in 0..eng.page_count() {
            let dl = eng.page_display_list(p).expect("頁存在");
            for it in &dl.items {
                if let DrawItem::Text { text, .. } = it {
                    all_texts.push(text.clone());
                }
            }
        }
        for target in ["Textbox1", "Textbox2", "Textbox3"] {
            assert!(
                all_texts.iter().any(|t| t.contains(target)),
                "{file} 必須發射 {target} 文字項（回歸鎖）"
            );
        }
    }
}
