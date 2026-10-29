//! K10 紅燈：表內浮動錨點（wp:anchor in_cell）發射鏈修復（IGCSE 封面框吞字根治）。
//!
//! 依據：`docs/wps-txbx-cover-diag.md` §4 全文。
//! 主根因 H2：`<wp:anchor floating>` 身處 `<w:tc>` 內被解析層標 `in_cell=true` →
//! 被正文流剔除、灌入表格儲存格串列 → 遭格寬鉗到 115pt → 發射端尺寸精確比對失敗 →
//! 文字發射從未被呼叫（實測殘留：p0 灰底佔位 Rect 115×33.75pt @y92.19，文字全吞）。
//!
//! 紅燈規格：
//! 1. fdo68607 p0 顯示列表含 "IGCSE Drama Glossary"（含 "IGCSE" 開頭 run），size 24pt，
//!    y<60pt（LO 23.7）、x≈210±15（LO 210.2）
//! 2. 灰底佔位 Rect（115×33.75 @y92.19）消失（文字框不再退化佔位）
//! 3. WC048-Text-Box-in-Cell（xopt，2 檔）：3 個文字框發射率 100%（報告 §5.3）
//! 4. buggy 鎖 K10_CELL_ANCHOR_BUGGY=1 → 回退 in_cell 舊標記路徑，紅燈必紅。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

/// 紅燈 1 & 2：fdo68607 p0 封面文字框發射，灰佔位框消失。
#[test]
fn fdo68607_cover_textbox_emitted_and_placeholder_disappears() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    let dl = eng.page_display_list(0).expect("p0 存在");

    // 斷言 1：p0 顯示列表含 "IGCSE Drama Glossary"（含 "IGCSE" 開頭 run），size 24pt，y<60pt，x≈210±15
    let title_item = dl.items.iter().find(|it| match it {
        DrawItem::Text { text, .. } => {
            text.contains("IGCSE Drama Glossary") || text.starts_with("IGCSE")
        }
        _ => false,
    });
    assert!(
        title_item.is_some(),
        "fdo68607 p0 必須包含「IGCSE Drama Glossary」文字項（修前文字全吞）"
    );
    if let Some(DrawItem::Text {
        x,
        y,
        size_pt,
        text,
        ..
    }) = title_item
    {
        assert!(
            (*size_pt - 24.0).abs() < 1.0,
            "標題字級應為 24pt，實測 size={size_pt} (text={text})"
        );
        assert!(
            *y < 60.0,
            "標題應浮動至頁首 (y < 60pt)，實測 y={y} (LO 23.7)"
        );
        assert!(
            (*x - 210.0).abs() <= 15.0,
            "標題 x 應在 210±15pt 區間，實測 x={x} (LO 210.2)"
        );
    }

    // 斷言 2：灰底佔位 Rect（115×33.75 @y92.19）消失
    let has_gray_placeholder = dl.items.iter().any(|it| match it {
        DrawItem::Rect {
            y,
            w,
            h,
            fill_argb,
            ..
        } => {
            *fill_argb == 0xFFF0_F0F0
                && (*w - 115.0).abs() < 1.0
                && (*h - 33.75).abs() < 1.0
                && (*y - 92.19).abs() < 5.0
        }
        _ => false,
    });
    assert!(
        !has_gray_placeholder,
        "灰底佔位 Rect（115×33.75 @y92.19）必須消失（文字框不再退化佔位）"
    );
}

/// 紅燈 3：WC048 兩檔（原版與 Mod）3 個儲存格文字框發射率 100%。
#[test]
fn wc048_text_box_in_cell_three_boxes_emitted() {
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
                "{file} 必須發射文字框文字「{target}」，實得 texts={all_texts:?}"
            );
        }
    }
}
