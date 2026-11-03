//! R32 紅燈：WC048 表格首列文字位移、Textbox3 灰帶、框內重複字。
//!
//! # LO 真值（pymupdf，`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` p0）
//!
//! | 項目 | LO 實測 |
//! |---|---|
//! | 首列格子文字 | cell1 x=77.8 **y=112.2**（被 Textbox1 wrap 推下）；cell2/3 x=228.0/378.2 y=72.7 |
//! | Textbox2（273.6×110.55）| 只有兩條藍線，**無底色 Rect** |
//! | Textbox3 群組根（387×185）| **無底色 Rect**；子件白框 387×98.5＋藍塊 281.4×2.3 |
//! | 全篇 "This is a test." | 9 個（首列 3＋第 2 列 3＋第 3 列 3） |
//!
//! 修前：首列 cell1 y=72.0（未 wrap 推下）；Textbox2/3 根各發 0xFFF0F0F0 灰佔位
//! Rect（Textbox3 的 387×185 灰帶蓋到第 3 列文字，視覺上像重複字）。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

const WC048: &str = "xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx";
const GRAY_PLACEHOLDER: u32 = 0xFFF0_F0F0;

/// 燈 1：首列 cell1 文字被 Textbox1 wrap 推下（LO y=112.2），cell2/3 留在列頂。
#[test]
fn first_row_cell1_text_pushed_below_textbox() {
    let eng = load_corpus(WC048);
    let dl = eng.page_display_list(0).expect("p0 存在");
    let texts: Vec<(f32, f32)> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { x, y, text, .. } if text == "This is a test." => Some((*x, *y)),
            _ => None,
        })
        .collect();
    let first_col = texts
        .iter()
        .find(|(x, y)| *x < 150.0 && *y < 130.0)
        .copied()
        .expect("首列 cell1 必須有 This is a test.");
    assert!(
        (108.0..=116.0).contains(&first_col.1),
        "首列 cell1 文字應被 Textbox1 wrap 推下到 LO y=112.2±4，實得 y={}（修前 72.0）",
        first_col.1
    );
    // 第一列列頂（y≈72）三格文字序列斷言 == LO pymupdf span 真值（首格空）：
    let row0_top_cell1 = texts
        .iter()
        .find(|(x, y)| *x < 150.0 && (69.0..=76.0).contains(y));
    let row0_top_cell2 = texts
        .iter()
        .find(|(x, y)| (150.0..=300.0).contains(x) && (69.0..=76.0).contains(y));
    let row0_top_cell3 = texts
        .iter()
        .find(|(x, y)| *x > 300.0 && (69.0..=76.0).contains(y));
    assert!(
        row0_top_cell1.is_none(),
        "第一列首格在列頂必須為空（LO pymupdf 真值 [空, This is a test., This is a test.]），實得 {row0_top_cell1:?}"
    );
    assert!(row0_top_cell2.is_some(), "第一列第 2 格在列頂必須有字");
    assert!(row0_top_cell3.is_some(), "第一列第 3 格在列頂必須有字");
}

/// 燈 2：Textbox2／Textbox3 根皆無底色（LO 無灰帶）；Textbox3 的 387×185 灰帶
/// 不得再蓋住第 3 列文字。
#[test]
fn textbox_roots_have_no_gray_fill() {
    let eng = load_corpus(WC048);
    let dl = eng.page_display_list(0).expect("p0 存在");
    let gray: Vec<(f32, f32, f32, f32)> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } if *fill_argb == GRAY_PLACEHOLDER => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect();
    assert!(
        gray.is_empty(),
        "頁面不得有佔位灰 Rect（LO Textbox2/3 皆無底色），實得 {gray:?}"
    );
}

/// 燈 3：全篇 "This is a test." 發射總數＝LO 真值 9；第 3 列 3 個留在 y≈345.2。
#[test]
fn test_text_total_matches_lo_truth() {
    let eng = load_corpus(WC048);
    let dl = eng.page_display_list(0).expect("p0 存在");
    let texts: Vec<(f32, f32)> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { x, y, text, .. } if text == "This is a test." => Some((*x, *y)),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts.len(),
        9,
        "全篇 This is a test. 應＝LO 9 個（不得重複發射），實得 {}: {texts:?}",
        texts.len()
    );
    let third_row: Vec<(f32, f32)> = texts.iter().copied().filter(|(_, y)| *y > 330.0).collect();
    assert_eq!(third_row.len(), 3, "第 3 列應 3 個，實得 {third_row:?}");
    for (x, y) in third_row {
        assert!(
            (341.0..=348.0).contains(&y),
            "第 3 列 x={x} 應落 LO y=345.2±3，實得 {y}"
        );
    }
}
