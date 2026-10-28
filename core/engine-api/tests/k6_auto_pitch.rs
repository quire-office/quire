//! K6 紅燈：LO auto 行距模型（pitch = size × declared/240 × LH(effective_face)）。
//!
//! 口徑出自 `docs/k6-lineheight-calibration.md` §1／§6.4：pitch 是**行與行之間的
//! 推進量**，不是頁數。本檔一律從引擎顯示列表的文字 y 差分取樣（line-metrics
//! 的 top_pt 同源），跨頁彙總後取眾數，避免把段距／表格列高混進口徑。
//!
//! 裁定 (A)：fdo68607 每頁 40/42/41 行已與 LO 吻合＝pitch 模型正確；9 頁 vs LO 8
//! 頁的差距在**水平換行總行數（358 vs LO ~336）＝表格可用寬軸**（K5-diag §3 #5
//! 前半的已知獨立缺陷，另立卡追）。本檔只鎖 pitch，頁數紅燈**暫記 9**，嚴禁為
//! 湊 8 頁回頭動 pitch（那會把剛殺死的魔術常數放回來）。
//!
//! patho_b 錨點（13.75 vs 14.85）PM 尚未定案——本檔**不**斷言任何 patho_b 值。
//!
//! BUGGY 回塞鎖：`K6_AUTO_PITCH_BUGGY=1` 走舊徑（零宣告 ×1.4、無 real-LH 查表），
//! 同一支紅燈檔必紅。測試進程內不設 env（避免平行執行緒污染），由外部執行
//! `K6_AUTO_PITCH_BUGGY=1 cargo test -p quire-engine --test k6_auto_pitch` 驗證。

use quire_engine::Engine;
use quire_render_trait::DrawItem;
use std::collections::BTreeMap;

fn load_fixture(name: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/").to_string() + name;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open fixture")
}

fn load_corpus(name: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + name;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

/// 一頁內所有非空文字項的 y（去重到 0.05pt；同一行的多 span 只留一個）。
fn page_text_ys(eng: &Engine, page: usize) -> Vec<f32> {
    let dl = eng.page_display_list(page).expect("頁存在");
    let mut ys: Vec<f32> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { y, text, .. } if !text.trim().is_empty() => Some(*y),
            _ => None,
        })
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    ys
}

/// 全文行距樣本：每頁相鄰文字行的 y 差（>0.5pt，濾掉同列的微小基線差）。
fn line_gaps(eng: &Engine) -> Vec<f32> {
    let mut out = Vec::new();
    for p in 0..eng.page_count() {
        let ys = page_text_ys(eng, p);
        for w in ys.windows(2) {
            let d = w[1] - w[0];
            if d > 0.5 {
                out.push(d);
            }
        }
    }
    out
}

/// y 差的眾數（0.1pt 分箱）＝pitch 口徑；空樣本回 0。
fn modal_gap(gaps: &[f32]) -> f32 {
    let mut bins: BTreeMap<i32, u32> = BTreeMap::new();
    for &g in gaps {
        if g.is_finite() && g > 0.0 {
            *bins.entry((g * 10.0).round() as i32).or_default() += 1;
        }
    }
    bins.into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, _)| k as f32 / 10.0)
        .unwrap_or(0.0)
}

fn has_gap(gaps: &[f32], target: f32, tol: f32) -> bool {
    gaps.iter().any(|g| (g - target).abs() <= tol)
}

/// 紅燈 1：四段宣告倍率 240/276/360/480（Liberation Serif 14pt）→
/// pitch 16.1 / 18.5 / 24.2 / 32.2 ±0.6pt（公式與 LO 實測同，報告 §6.4）。
#[test]
fn k6_fixture_four_declared_pitches() {
    let eng = load_fixture("k6_auto_pitch.docx");
    let gaps = line_gaps(&eng);
    for target in [16.1f32, 18.5, 24.2, 32.2] {
        assert!(
            has_gap(&gaps, target, 0.6),
            "宣告倍率段 pitch {target}±0.6 不在 y 差分樣本中：gaps={gaps:?}"
        );
    }
}

/// 紅燈 2：fdo68607 全文（單一大表）pitch 眾數 17.1 ±0.5pt。
/// 斷言「pitch 不是頁數」：眾數必須遠離頁數（9）。表格樣式 declared=240 壓過
/// docDefaults 276 才算對（報告 §4），舊徑 docDefaults 276 會落到 19.6。
///
/// 頁數紅燈暫記 9：LO 8 頁；差額在水平換行總行數 358 vs LO ~336（表格可用寬
/// 軸，另立卡追），與 pitch 無關——本測試不得為了頁數回改 pitch。
#[test]
fn fdo68607_pitch_mode_is_17_1_not_page_count() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    let gaps = line_gaps(&eng);
    let mode = modal_gap(&gaps);
    assert!(
        (mode - 17.1).abs() <= 0.5,
        "fdo68607 pitch 眾數應為 17.1±0.5（表格樣式 240 生效），實得 {mode}：gaps 眾數樣本 {:?}",
        {
            let mut top: Vec<_> = {
                let mut b: BTreeMap<i32, u32> = BTreeMap::new();
                for &g in &gaps {
                    *b.entry((g * 10.0).round() as i32).or_default() += 1;
                }
                b.into_iter().collect()
            };
            top.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
            top.into_iter().take(6).collect::<Vec<_>>()
        }
    );
    let pages = eng.page_count();
    assert!(
        (mode - pages as f32).abs() > 5.0,
        "pitch 口徑 {mode} 不得等於頁數 {pages}（斷言取的是行距，不是頁數）"
    );
    assert_eq!(
        pages, 9,
        "頁數紅燈暫記 9（LO 8；水平換行寬另立卡，fdo68607 總行數 358 vs LO 336）"
    );
}

/// 紅燈 3：零宣告行距的 Calibri 11pt 檔 → pitch 13.5（LO＝11×1.2207）。
/// 舊徑 None→×1.4＝15.4 必紅：兩條斷言分別鎖「新值要對」與「舊值必須死」。
#[test]
fn zero_decl_calibri_pitch_is_13_5_not_1_4() {
    let eng = load_fixture("k6_zero_decl_calibri.docx");
    let gaps = line_gaps(&eng);
    let mode = modal_gap(&gaps);
    assert!(
        (mode - 13.5).abs() <= 0.6,
        "零宣告 Calibri 11pt pitch 應為 13.5±0.6（11×1.2207），實得 {mode}：gaps={gaps:?}"
    );
    assert!(
        (mode - 15.4).abs() > 0.6,
        "舊徑 ×1.4（15.4）必須死透，實得 {mode}：gaps={gaps:?}"
    );
}
