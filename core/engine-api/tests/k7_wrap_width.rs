//! K7 紅燈：表格換行可用寬（fdo68607 自然寬表不壓版心＋越界左右均分）。
//!
//! 口徑出自 `docs/k7-wrapwidth-diag.md` §1／§3：
//! - 無宣告 `w:tblW` 的自然寬表（fdo68607：tblGrid 550.8pt＞版心 540pt），
//!   LO **不壓寬**且越界量左右均分——格線中心 30.6／581.4（LO 30.1／580.9，
//!   容差 1.0pt），右欄文字起點 161.8（LO 161.4，容差 1.0pt）。
//! - 右欄格內可用寬＝425.0−2×5.4＝**414.2pt**（cellMargins 108dxa 兩側都扣，
//!   邊框寬**不扣**——LO 最長行文字越格線 2.5pt 實測鐵證）。
//!
//! 頁數紅燈**暫記 9**（LO 8）：殘差行數（491 vs 457）根因＝內嵌 Noto 子集
//! vs LO 替代 face 的字形 advance 寬差（+14.3%，報告 §3——引擎探針
//! scale=0.8742 直接 8 頁），屬「水平寬度表」獨立校準軸（K6-cal 同紀律，
//! K7-cal 卡），**嚴禁**在本檔用魔術常數湊 8 頁。K6 pitch 零觸碰。
//!
//! BUGGY 回塞鎖：`K7_WRAP_WIDTH_BUGGY=1` 回塞舊徑（壓版心＋貼左緣），
//! 格線錨點必紅。測試進程內不設 env（避免平行污染），由外部執行
//! `K7_WRAP_WIDTH_BUGGY=1 cargo test -p quire-engine --test k7_wrap_width` 驗證。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(name: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + name;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

/// p0 垂直格線 x（去重排序）。
fn vertical_rule_xs(eng: &Engine, page: usize) -> Vec<f32> {
    let dl = eng.page_display_list(page).expect("頁存在");
    let mut xs: Vec<f32> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Rule { x1, y1, x2, y2, .. }
                if (x1 - x2).abs() < 0.01 && (y1 - y2).abs() > 3.0 =>
            {
                Some(*x1)
            }
            _ => None,
        })
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    xs
}

/// 紅燈 1：fdo68607 p0 表格外框格線錨定 LO（30.1／580.9±1.0pt）。
/// 舊徑（壓版心＋貼左緣）＝36.0／586.8：右界 586.8 距 LO 580.9＝5.9pt 必紅。
#[test]
fn fdo68607_natural_width_table_gridlines_match_lo() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    let xs = vertical_rule_xs(&eng, 0);
    let left = xs.first().copied().unwrap_or(f32::NAN);
    let right = xs.last().copied().unwrap_or(f32::NAN);
    assert!(
        (left - 30.6).abs() < 1.0,
        "自然寬表左格線應 ≈30.6（LO 30.1±0.5），實得 {left}：xs={xs:?}"
    );
    assert!(
        (right - 581.4).abs() < 1.0,
        "自然寬表右格線應 ≈581.4（LO 580.9±0.5），實得 {right}：xs={xs:?}"
    );
}

/// 紅燈 2：右欄文字起點 x0＝格左界＋cellMargin(5.4)——LO 161.4±1.0pt。
/// 同時鎖「可用寬不扣邊框」：文字可延伸到越過格內側（LO 最長行越格線 2.5pt），
/// 若實作錯扣邊框寬（0.5pt 級）x0 不動、此斷言不咬——由頁數暫記紅燈兜住。
#[test]
fn fdo68607_right_column_text_x0_anchored() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    let dl = eng.page_display_list(0).expect("頁存在");
    let mut xs: Vec<f32> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { x, text, .. } if !text.trim().is_empty() && *x > 100.0 => Some(*x),
            _ => None,
        })
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mode = *xs.first().unwrap();
    assert!(
        (mode - 161.8).abs() < 1.0,
        "右欄文字 x0 應 ≈161.8（LO 161.4），實得 {mode}（n={})",
        xs.len()
    );
}

/// 紅燈 3：頁數暫記 9（LO 8）。差距根因＝字形 advance 寬差（報告 §3），
/// 由 K7-cal 水平寬度表卡收斂後，**該卡**負責把本斷言改 8 並留逐例註明。
/// 本檔不得用魔術常數湊 8（比照 k6_auto_pitch「頁數暫記」先例）。
#[test]
fn fdo68607_page_count_provisionally_9_until_width_table() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    let pages = eng.page_count();
    assert!(
        pages == 9 || pages == 8,
        "頁數應為 9（K7 幾何修正後暫記）或 8（K7-cal 寬表落地後），實得 {pages}"
    );
    if pages == 9 {
        // 暫記態：可用寬軸必須已閉——p0 格線錨點（紅燈 1）同時綠才允許。
        let xs = vertical_rule_xs(&eng, 0);
        assert!(
            xs.first().is_some_and(|l| (l - 30.6).abs() < 1.0),
            "9 頁暫記態僅在可用寬軸已閉（格線 30.6）時合法"
        );
    }
}
