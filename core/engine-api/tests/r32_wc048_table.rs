//! R32 紅燈（R34 重寫版）：WC048 表格首列文字位移、Textbox 根框佔位殘留、框內重複字。
//!
//! # R34 重寫緣由（PM 審查坐實）
//! 舊版燈 2 只查 `0xFFF0F0F0` 灰 Rect——R32 把填色改成 alpha-0 幽靈後照常放行；
//! 燈 3（文字總數）修前即綠（見 `docs/r32-redlight.md` 紅燈紀錄），零鑑別力。
//! 本版燈 2 加查 alpha-0 幽靈 Rect 與 0xFFCCCCCC 佔位框線，燈 3 加查幽靈
//! 吞噬文字；回塞鎖 `R34_PLACEHOLDER_GATE_BUGGY=1` 把引擎打回 R32 態即可
//! 復驗「修前必紅」。
//!
//! # LO 真值（pymupdf，`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` p0）
//!
//! | 項目 | LO 實測 |
//! |---|---|
//! | 首列格子文字 | cell1 x=77.8 **y=112.2**（被 Textbox1 wrap 推下）；cell2/3 x=228.0/378.2 y=72.7 |
//! | Textbox2（273.6×110.55）| 只有兩條藍線，**無底色 Rect、無框線** |
//! | Textbox3 群組根（387×185）| **無底色 Rect、無框線**；子件白框 387×98.5＋藍塊 281.4×2.3 |
//! | 全篇 "This is a test." | 9 個（首列 3＋第 2 列 3＋第 3 列 3）|
//!
//! 修前（R32 當時）：首列 cell1 y=72.0；Textbox2/3 根各發 0xFFF0F0F0 灰 Rect
//! （Textbox3 的 387×185 灰帶蓋到第 3 列文字，視覺上像重複字）。
//! R32 修後殘留（R34 標的）：兩根框改發 alpha-0 幽靈 Rect＋Textbox3 外圍
//! 4 條 0xFFCCCCCC 0.75pt 佔位框線（LO 全無）。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

const WC048: &str = "xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx";
const GRAY_PLACEHOLDER: u32 = 0xFFF0_F0F0;
const EDGE_PLACEHOLDER: u32 = 0xFFCC_CCCC;

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

/// 燈 2（R34 強化）：Textbox2／Textbox3 根框零墨跡——零佔位灰 Rect、零
/// alpha-0 幽靈 Rect、零 0xFFCCCCCC 佔位框線（LO 真值＝兩根框無底色無框線）。
///
/// 鑑別力：R32 修前（灰 Rect）紅；R32 修後 main@9d0407e（幽靈＋框線）紅；
/// R34 修後綠。以 `R34_PLACEHOLDER_GATE_BUGGY=1` 把引擎打回 R32 態可復驗紅。
#[test]
fn textbox_roots_have_zero_ink() {
    let eng = load_corpus(WC048);
    let dl = eng.page_display_list(0).expect("p0 存在");
    let (mut gray, mut ghost, mut edge): (Vec<String>, Vec<String>, Vec<String>) =
        (Vec::new(), Vec::new(), Vec::new());
    for it in &dl.items {
        match it {
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } => {
                let tag = format!("({x:.1},{y:.1},{w:.1},{h:.1})");
                if *fill_argb == GRAY_PLACEHOLDER {
                    gray.push(tag);
                } else if (*fill_argb >> 24) == 0 {
                    ghost.push(tag);
                }
            }
            DrawItem::Rule {
                x1,
                y1,
                x2,
                y2,
                color_argb,
                ..
            } if *color_argb == EDGE_PLACEHOLDER => {
                edge.push(format!("({x1:.1},{y1:.1})-({x2:.1},{y2:.1})"));
            }
            _ => {}
        }
    }
    assert!(
        gray.is_empty(),
        "頁面不得有佔位灰 Rect（LO Textbox2/3 皆無底色），實得 {gray:?}"
    );
    assert!(
        ghost.is_empty(),
        "頁面不得有 alpha-0 幽靈 Rect（R32 透明底口徑錯誤；SVG 端仍 hit-testable），實得 {ghost:?}"
    );
    assert!(
        edge.is_empty(),
        "頁面不得有 0xFFCCCCCC 佔位框線（LO Textbox3 無框線；R32 未攔截邊框臂），實得 {edge:?}"
    );
}

/// 燈 3（R34 強化）：全篇 "This is a test." 發射總數＝LO 真值 9；第 3 列 3 個
/// 留在 y≈345.2。總數段修前即綠（R32 紅燈紀錄坐實），R34 鑑別力在幽靈吞噬
/// 斷言：alpha-0 Rect 視覺不可見但 hit-testable，文字錨點嚴格落在任何幽靈
/// Rect 內部＝命中測試被幽靈攔截（LO 無此圖元）。修前基線幽靈
/// (160.6,93.6,273.6,110.5) 內部含第 2 列 cell2/cell3 文字 → 紅。
/// 注意：可見白框子件 (72.0,158.5,387.0,98.5) 內有 Textbox3 文字是 LO 合法
/// 形態（LO 真值如此），本斷言只吃 alpha-0，不得連坐可見 Rect。
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
    for (x, y) in &third_row {
        assert!(
            (341.0..=348.0).contains(y),
            "第 3 列 x={x} 應落 LO y=345.2±3，實得 {y}"
        );
    }
    // 幽靈吞噬斷言（R34 鑑別力）：任何文字錨點不得嚴格落在 alpha-0 Rect 內部。
    for (tx, ty) in &texts {
        for it in &dl.items {
            if let DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } = it
            {
                if (*fill_argb >> 24) != 0 {
                    continue; // 可見 Rect（含 LO 合法白框子件）不在此錘範圍
                }
                let inside =
                    *tx > x + 0.5 && *tx < x + w - 0.5 && *ty > y + 0.5 && *ty < y + h - 0.5;
                assert!(
                    !inside,
                    "文字 ({tx:.1},{ty:.1}) 落進幽靈 Rect ({x:.1},{y:.1},{w:.1},{h:.1}) 內部\
                     （alpha-0 仍 hit-testable，LO 無此圖元）"
                );
            }
        }
    }
}

/// 燈 4（防假綠回塞鎖）：`R34_PLACEHOLDER_GATE_BUGGY=1` 把發射層打回 R32 舊
/// 口徑（幽靈 Rect＋佔位框線照發）——燈 2 在本態必紅，證明其紅綠由 R34 修復
/// 驅動，不是斷言寫寬僥倖（照 K17 檔尾 BUGGY 鎖測先例；未設變數時跳過）。
#[test]
fn r34_buggy_regression_lock() {
    if std::env::var_os("R34_PLACEHOLDER_GATE_BUGGY").is_none() {
        eprintln!("未設 R34_PLACEHOLDER_GATE_BUGGY：跳過回塞檢驗（以其=1 跑）。");
        return;
    }
    let eng = load_corpus(WC048);
    let dl = eng.page_display_list(0).expect("p0 存在");
    let ghosts = dl
        .items
        .iter()
        .filter(|it| {
            matches!(it, DrawItem::Rect { fill_argb, .. } if (*fill_argb >> 24) == 0)
        })
        .count();
    let edges = dl
        .items
        .iter()
        .filter(|it| matches!(it, DrawItem::Rule { color_argb, .. } if *color_argb == EDGE_PLACEHOLDER))
        .count();
    assert!(
        ghosts > 0 && edges > 0,
        "BUGGY 態應復原 R32 殘留（幽靈 Rect＋佔位框線），實得 ghosts={ghosts} edges={edges}"
    );
}
