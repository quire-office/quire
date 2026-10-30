//! K20 紅燈：WC049-Mod 錨定列過度撐列返修（行數估算未帶 K7-cal 水平尺）。
//!
//! # 根因（PM 接手歸因，zipfile＋LO 25.8 ground truth 實測）
//! K17 `k17_grow_floors` 的 spAutoFit 行數估算用 `TextMeasurer::new(size)`
//! （advance_scale=1.0），**未接 K7-cal 水平尺**（`face_advance_scale`）：
//! Calibri 宣告在我們表本裡 1.0 尺＝寬 18.95/unit，LO 真 Calibri/Carlito
//! ＝0.8834 尺。WC049-Mod 框內文字較原版加長（`...is a test.`→
//! `...is an interesting test.`，37 字），1.0 尺斷行判 **2 行**、LO 真尺
//! 判 **1 行**（LO p0 實測框內文字單行：'Textbox1' x=215.1 → 'test.'
//! x=350.8，寬 ~158pt < 可用寬 185.9−lIns−rIns=171.5）。多判一行 ⇒
//! eff_h 多 ~13.4pt ⇒ 錨定列（row2）撐到 48.5／LO 實測 35.5。
//!
//! 任務書「多框同列 sum/max」假設**不成立**：本檔 zipfile 直掃只有 1 個
//! 錨點框（Text Box 2，spAutoFit，posV 14.4/paragraph，wrapSquare，
//! tIns/bIns 3.6/3.6，宣告 wh 185.9×110.6，框內單段無 sz/rFonts/spacing
//! 宣告＝吃 docDefaults 11pt Calibri）。原版 WC049（24 字）1.0 尺也斷 1
//! 行＝現況已收斂（−1.75..−3.25），故本卡紅燈只鎖 Mod 版列高，原版以
//! 零回退 A/B 守。
//!
//! # LO 真值（pymupdf 實測 LO 25.8，render_lab.lo_to_pdf，單位＝PDF pt）
//! Mod 版 p0 表格橫線 y：72.3 / 86.3 / 113.7 / 149.2 / 176.6 / 190.5 / 204.5
//! - row2（錨定列）高 = 149.2 − 113.7 = **35.5**（我方 48.5 → 超推 13.0）
//! - 原版（WC049）row2 同高 35.5（135.7−100.2）——LO 不隨文字加長增行，
//!   因加長後仍單行。
//! - 撐列分解驗證：35.5 ≈ 14.4(posV) + 3.6(tIns) + 3.6(bIns) + 1×13.9
//!   （Calibri 11pt LO 實尺單行；我們表本 11×1.2207=13.43，差 0.47 ≤容差）。
//!
//! # 紅燈門檻（±1pt，K18 §6 盲測誤差 ≤0.6 支撐）
//! - 燈 1：Mod row2 高（橫線[3]−橫線[2]）∈ 35.5 ± 1（現況 48.5 → 必紅）。
//! - 燈 2：Mod 橫線 y 序列逐條 ∈ LO ±1（現況第 4 條起 +11.6/+11.2/+10.8/
//!   +10.2 → 必紅；防只修列高不修下游位移）。
//! - 燈 3：原版 WC049 橫線序列不退步（鎖現況 ±1；防行數尺改動誤傷）。
//! - `K7_ADVANCE_SCALE_BUGGY=1` → 燈 1／燈 2 必紅（回塞鎖：水平尺查表
//!   回退 1.0 即回到 2 行誤判——K17 原缺陷形態）。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

const MOD: &str = "xopt/TestFiles_WC_WC049-Text-Box-in-Cell-Mod.docx";
const PLAIN: &str = "xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx";
const TOL: f32 = 1.0;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

/// p0 全部橫向**表格**線 y（Rule 且 y1==y2，線寬 <0.6 排除文字框邊框；
/// K17 紅燈檔同口徑）。
fn rule_ys(eng: &Engine) -> Vec<f32> {
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
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    ys
}

/// LO 25.8 實測橫線 y（檔頭出處）。
const LO_MOD: [f32; 7] = [72.3, 86.3, 113.7, 149.2, 176.6, 190.5, 204.5];
/// 原版**我方現況**橫線 y（PM 實測：對 LO 偏差 −0.25..−3.25 屬已知殘差，
/// 非本卡標的）。本燈鎖我方現況逐條 ±0.5＝零漂移鎖：行數尺修正若誤傷
/// 原版斷行，這裡必紅；殘差本身留給後續卡（勿在此掩蓋）。
const OURS_PLAIN: [f32; 7] = [72.0, 85.4, 98.9, 133.9, 160.8, 174.3, 187.7];

/// 燈 2：Mod **相鄰列高**逐條命中 LO ±1（本卡標的＝錨定列列高口徑）。
///
/// 口徑更正記錄（PM）：任務書原寫「四頁 y 對 LO ±1pt」——本檔 LO 實測
/// 1 頁，且絕對 y 序列會把 row0/row1 的上游正文 pitch 殘差（合計 ≈1.4pt，
/// K6 級既有記帳、非本卡範圍）誤拉進紅燈。列高＝相鄰橫線差，把上游偏移
/// 差分掉，紅燈只歸因撐列口徑本身（修前誤差僅落 row2：+12.96）。
#[test]
fn k20_lamp2_wc049mod_row_heights() {
    let eng = load_corpus(MOD);
    let ys = rule_ys(&eng);
    assert_eq!(
        ys.len(),
        LO_MOD.len(),
        "橫線數不符 LO（期望 {}）: {ys:?}",
        LO_MOD.len()
    );
    for i in 0..ys.len() - 1 {
        let got = ys[i + 1] - ys[i];
        let want = LO_MOD[i + 1] - LO_MOD[i];
        assert!(
            (got - want).abs() <= TOL,
            "燈2：row{i} 列高應 ∈ {want:.2}±{TOL}（LO 橫線差實測），實得 {got:.2}（修前 row2=48.46/+12.96 行數誤判）"
        );
    }
}

/// 燈 1：Mod 錨定列 row2 高 ∈ 35.5 ± 1（現況 48.5 → 必紅）。
#[test]
fn k20_lamp1_wc049mod_row2_height_lo_truth() {
    let eng = load_corpus(MOD);
    let ys = rule_ys(&eng);
    assert!(ys.len() >= 4, "橫線數異常（期望 ≥4）: {ys:?}");
    let row2_h = ys[3] - ys[2];
    assert!(
        (35.5 - TOL..=35.5 + TOL).contains(&row2_h),
        "燈1：WC049-Mod row2（錨定列）高應 ∈ 35.5±{TOL}（LO 149.2−113.7），實得 {row2_h}（修前 48.5=行數誤判 2 行）\n橫線序列: {ys:?}"
    );
}

/// 燈 3：原版 WC049 橫線序列零漂移（行數尺修正不得改變已收斂的原版斷行）。
#[test]
fn k20_lamp3_wc049_plain_no_regression() {
    let eng = load_corpus(PLAIN);
    let ys = rule_ys(&eng);
    assert_eq!(
        ys.len(),
        OURS_PLAIN.len(),
        "原版橫線數漂移（期望 {}）: {ys:?}",
        OURS_PLAIN.len()
    );
    for (i, (got, want)) in ys.iter().zip(OURS_PLAIN.iter()).enumerate() {
        assert!(
            (got - want).abs() <= 0.5,
            "燈3：原版橫線[{i}] 漂移不得 >0.5（現況錨點 {want}），實得 {got}"
        );
    }
}

/// 回塞鎖：`K7_ADVANCE_SCALE_BUGGY=1`（水平尺查表回退 1.0＝K17 原缺陷形態）
/// ⇒ 燈 1／燈 2 必紅。未設該 env 時跳過（比照 K17 慣例）。
#[test]
fn k20_buggy_regression_lock() {
    if std::env::var_os("K7_ADVANCE_SCALE_BUGGY").is_none() {
        eprintln!("未設 K7_ADVANCE_SCALE_BUGGY：跳過回塞檢驗（以 =1 跑）。");
        return;
    }
    let eng = load_corpus(MOD);
    let ys = rule_ys(&eng);
    assert!(ys.len() >= 4, "BUGGY 態橫線數異常: {ys:?}");
    let row2_h = ys[3] - ys[2];
    assert!(
        (46.0..=50.0).contains(&row2_h),
        "BUGGY 態 row2 高應退回 ≈48.5（1.0 尺斷 2 行），實得 {row2_h}"
    );
}
