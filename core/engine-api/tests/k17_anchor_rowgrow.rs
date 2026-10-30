//! K17 紅燈：錨定框撐高宿主列（WC048 Textbox3 的 y 殘差 52.4pt）。
//!
//! # 根因與修復目標
//! K15 修好 WC048 Textbox3 的原點系（layoutInCell 壓制 relativeFrom），x 命中 LO
//! （我方 72.0 vs LO 72.25），但 y 我方 106.5／LO 158.9 差 52.4pt。
//! 根因：LO 把第 0 列被 Textbox2（wrapTopAndBottom＋spAutoFit）撐高到 65.9pt；
//! 我方未建模「錨定框撐高宿主列」，第 0 列仍按文字自然高（13.45pt），第 1 列頂
//! 跟著偏低，掛在第 1 列的 Textbox3 就差 52.4pt。
//!
//! # 撐列模型（規格出處＝`docs/k18-lo-rowgrow-calibration.md` §6，PM 復活令指定）
//! ```text
//! grow(anchor) = posV_pt + eff_h_pt
//! row_h        = max(natural_h, max{ grow(a) | a 錨在該列 })
//! eff_h        = noAutofit → 宣告 extent cy 原值
//!                spAutoFit  → tIns + bIns + Σ_par((before+after) + n_lines×pitch)
//!                             + Σ_pBdr_rules(sz/8 + space_pt)
//!                pitch      = size_pt × declared/240 × LH_face（框內 declared 恆 240）
//! ```
//! 撐列資格＝`layoutInCell=1` 且垂直 wrap ≠ none（topAndBottom 與 square 都撐、
//! none 不撐）。原任務書「AutoShape14 宣告高 185pt 撐列」前提已被 K18 §2 實測
//! 推翻（宣告值是**未排版**尺寸），本檔規格以 §6 為準。
//!
//! # LO 真值（pymupdf 實測，k18 卡 §1；單位＝PDF pt）
//! - 表格橫線 y 序列：72.25 / 138.15 / 344.25 / 358.15
//! - 第 0 列高 **65.9** ＝ 21.6(posV) + 3.6(tIns) + 3.6(bIns) + 12×1.2207(14.65)
//!   + 2×(3+8)(pBdr sz24/space8) + ε(0.45)；第 1 列高 **206.1** ＝ 21.05(pct 25%×頁高)
//!   + 185(群組宣告 noAutofit) + ε(0.1)
//! - Textbox3 框（Group 211，387×185）：(72.25, **158.9**)–(459.25, 257.4)
//!
//! # 紅燈門檻（容差 ±1pt：K18 模型盲測九案誤差全 ≤0.6，b0 復刻逐位元組命中）
//! - 燈 1：WC048 Textbox3 的 y ∈ 158.9 ± 1（現況 106.5 → 必紅）。
//! - 燈 2：第 0 列高（第 1、2 條表格橫線間距）∈ 65.9 ± 1（現況 13.45 → 必紅）。
//! - 燈 4：第 2 列頂 y（第 3 條橫線）∈ 344.25 ± 1（現況 184.98 → 必紅；防整段推歪）。
//! - 燈 3：render_lab 內容分不退步（`scripts/check-k17-scoreboard.py`，PM 實測口徑）。
//! - `K17_ANCHOR_ROWGROW_BUGGY=1` → 燈 1／燈 2／燈 4 必紅（回塞鎖，見檔尾鎖測）。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

const WC048: &str = "xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx";

/// LO 真值（K18 §1 pymupdf 實測，非口算）。
const LO_TEXTBOX3_Y: f32 = 158.9;
const LO_ROW0_H: f32 = 65.9;
const LO_ROW2_TOP: f32 = 344.25;
/// K18 §6：模型誤差 ≤0.6 ⇒ 容差 ±1pt 有正當性。
const TOL: f32 = 1.0;

/// 取 p0 中寬高命中 (w,h)±1 的 Rect → (x, y)。
fn rect_xy(eng: &Engine, w: f32, h: f32) -> (f32, f32) {
    let dl = eng.page_display_list(0).expect("p0 存在");
    dl.items
        .iter()
        .find_map(|it| match it {
            DrawItem::Rect {
                x, y, w: rw, h: rh, ..
            } if (rw - w).abs() < 1.0 && (rh - h).abs() < 1.0 => Some((*x, *y)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("p0 找不到 {w}×{h} 的 Rect"))
}

/// p0 全部橫向**表格**線 y（Rule 且 y1==y2；表格框線 0.5pt，文字框邊框 0.75pt，
/// 以線寬排除文字框自己的邊——避免巧合自證）。排序去重（容差 0.1）。
fn horizontal_rule_ys(eng: &Engine) -> Vec<f32> {
    let dl = eng.page_display_list(0).expect("p0 存在");
    let mut ys: Vec<f32> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Rule {
                y1, y2, width_pt, ..
            } if (y1 - y2).abs() < 0.01 && *width_pt < 0.6 => Some(*y1),
            _ => None,
        })
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.1);
    ys
}

/// 燈 1：WC048 Textbox3（Group 211，387×185）的 y ＝ LO 158.9 ± 1。
///
/// 修前 106.5（第 1 列頂未撐＝我方 85.45＋posOffset 21.05）。撐列修復後
/// 第 1 列頂＝72.25＋65.9＝138.15，＋21.05＝159.2（LO 158.9，Δ0.3）。
#[test]
fn k17_lamp1_wc048_textbox3_y_lo_truth() {
    let eng = load_corpus(WC048);
    let (_x, y) = rect_xy(&eng, 387.0, 185.0);
    assert!(
        (y - LO_TEXTBOX3_Y).abs() <= TOL,
        "燈1：Textbox3 y 應落 LO {LO_TEXTBOX3_Y}±{TOL}，實得 {y}（修前 106.5）"
    );
}

/// 燈 2：LO 第 0 列高 65.9pt 的結構鎖——第 1、2 條表格橫線間距 65.9 ± 1。
///
/// K18 §3 完整分解：21.6(posV) + 3.6(tIns) + 3.6(bIns) + 12×1.2207(=14.65)
/// + 2×(3+8)(pBdr sz24/space8) + ε(0.45) ＝ 65.9。
#[test]
fn k17_lamp2_wc048_row0_height_lo_truth() {
    let eng = load_corpus(WC048);
    let ys = horizontal_rule_ys(&eng);
    assert!(
        ys.len() >= 2,
        "表格至少應有 2 條橫線，實得 {} 條: {:?}",
        ys.len(),
        ys
    );
    let row0_h = ys[1] - ys[0];
    assert!(
        (row0_h - LO_ROW0_H).abs() <= TOL,
        "燈2：第 0 列高（ys[1]-ys[0]）應為 LO {LO_ROW0_H}±{TOL}，實得 {row0_h}（橫線序列: {ys:?}；修前 13.45）"
    );
}

/// 燈 4：第 2 列頂（ys[2]）＝ LO 344.25 ± 1（防「只撐第 0 列、整段仍推歪」）。
///
/// row1 高 206.1 ＝ 21.05(pct 25%×頁高) + 185(群組宣告 noAutofit) + ε：
/// ys[2] ＝ 72.25 + 65.9 + 206.1 ＝ 344.25。
#[test]
fn k17_lamp4_wc048_row2_top_lo_truth() {
    let eng = load_corpus(WC048);
    let ys = horizontal_rule_ys(&eng);
    assert!(
        ys.len() >= 3,
        "表格至少應有 3 條橫線（含第 2 列頂界），實得 {} 條: {:?}",
        ys.len(),
        ys
    );
    let row2_top = ys[2];
    assert!(
        (row2_top - LO_ROW2_TOP).abs() <= TOL,
        "燈4：第 2 列頂 y（ys[2]）應落 LO {LO_ROW2_TOP}±{TOL}，實得 {row2_top}（橫線序列: {ys:?}）"
    );
}

/// 燈 3（門檻腳本存在性）：render_lab 內容分門檻由 `scripts/check-k17-scoreboard.py`
/// 把關（WC048 兩檔 ≥ 0.718／0.723、tdf124398 ≥ 0.574，PM 二輪實測基線）。
/// 本燈只鎖「腳本在庫、可被 CI 指到」；實際分數由交付流程跑 render_lab 後餵腳本。
#[test]
fn k17_lamp3_scoreboard_gate_script_exists() {
    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/check-k17-scoreboard.py"
    );
    assert!(
        std::path::Path::new(script).exists(),
        "燈3：gate 腳本 {script} 應存在（render_lab 內容分門檻）"
    );
}

/// 燈 5（零回退鎖，K15 成果的絕對值更新版）：撐列後 Textbox3 仍踩宿主格頂——
/// y − posOffset(21.05) 必須落在某條表格橫線上（K15 燈1 的結構性斷言原樣沿用，
/// 絕對 y band 由 k15 檔的 100..=115 更新為本檔的 LO 真值）。
#[test]
fn k17_lamp5_textbox3_still_anchored_to_cell_top() {
    let eng = load_corpus(WC048);
    let (_x, y) = rect_xy(&eng, 387.0, 185.0);
    let cell_top = y - 21.0475;
    let ys = horizontal_rule_ys(&eng);
    assert!(
        ys.iter().any(|ry| (ry - cell_top).abs() < 0.6),
        "燈5：Textbox3 y={y} 應＝宿主格頂＋21.05（格頂 {cell_top} 須在表格橫線 {ys:?} 上）"
    );
}

/// BUGGY 回塞鎖：`K17_ANCHOR_ROWGROW_BUGGY=1` 時撐列邏輯整支關閉，
/// 燈 1／燈 2／燈 4 的幾何必須退回修前舊值（106.5／13.45／184.98）——
/// 證明三盞紅燈真的由本修復驅動，不是常數僥倖。
#[test]
fn k17_buggy_regression_lock() {
    if std::env::var_os("K17_ANCHOR_ROWGROW_BUGGY").is_none() {
        eprintln!("未設 K17_ANCHOR_ROWGROW_BUGGY：跳過回塞檢驗（以 K17_ANCHOR_ROWGROW_BUGGY=1 跑）。");
        return;
    }
    let eng = load_corpus(WC048);
    let (_x, y) = rect_xy(&eng, 387.0, 185.0);
    assert!(
        (100.0..=115.0).contains(&y),
        "BUGGY 態 Textbox3 y 應退回舊值 ≈106.5，實得 {y}"
    );
    let ys = horizontal_rule_ys(&eng);
    assert!(ys.len() >= 3, "BUGGY 態橫線數異常: {ys:?}");
    let row0_h = ys[1] - ys[0];
    assert!(
        (10.0..=20.0).contains(&row0_h),
        "BUGGY 態第 0 列高應退回舊值 ≈13.45，實得 {row0_h}"
    );
    assert!(
        !(LO_ROW2_TOP - TOL..=LO_ROW2_TOP + TOL).contains(&ys[2]),
        "BUGGY 態第 2 列頂不应命中 {LO_ROW2_TOP}±{TOL}，實得 {}",
        ys[2]
    );
}
