//! K7 紅燈：表格換行可用寬（fdo68607 自然寬表不壓版心＋越界左右均分）。
//!
//! 口徑出自 `docs/k7-wrapwidth-diag.md` §1／§3：
//! - 無宣告 `w:tblW` 的自然寬表（fdo68607：tblGrid 550.8pt＞版心 540pt），
//!   LO **不壓寬**且越界量左右均分——格線中心 30.6／581.4（LO 30.1／580.9，
//!   容差 1.0pt），右欄文字起點 161.8（LO 161.4，容差 1.0pt）。
//! - 右欄格內可用寬＝425.0−2×5.4＝**414.2pt**（cellMargins 108dxa 兩側都扣，
//!   邊框寬**不扣**——LO 最長行文字越格線 2.5pt 實測鐵證）。
//!
//! 頁數紅燈已收緊 **8＝LO**（K7d 收尾鎖）：殘差行數（491 vs 457）根因＝內嵌
//! Noto 子集 vs LO 替代 face 的字形 advance 寬差（+14.3%，報告 §3），由
//! K7-cal 水平advance查表（`face_advance_scale`，Calibri 0.8834 標準拉丁
//! 樣本串口徑）收斂——頁數鎖的支撐來自該查表，**嚴禁**用魔術常數直接淵 8。
//! K6 pitch 軸零觸碰。
//!
//! BUGGY 回塞鎖：`K7_WRAP_WIDTH_BUGGY=1` 回塞舊徑（壓版心＋貼左緣，且
//! K7-cal 水平尺全退 1.0），格線錨點與頁數必紅。測試進程內不設 env
//! （避免平行污染），由外部執行
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
/// 若實作錯扣邊框寬（0.5pt 級）x0 不動、此斷言不咬——由頁數紅燈兜住。
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

/// 紅燈 3：頁數嚴格鎖 **8＝LO**（K7d 收尾鎖，原「暫記 9」台詞退場）。
///
/// 支撐來源＝K7-cal 水平 advance 查表（engine-api `face_advance_scale`：
/// Calibri→Carlito 0.8834，A 軌標準拉丁樣本串 hmtx 寬比口徑，B 軌真實行
/// median 0.8747 覆核；報告 §3——引擎探針 scale=0.8742 直接 8 頁鐵證）。
/// 本斷言**不是**魔術常數淵 8：把查表退 1.0（`K7_WRAP_WIDTH_BUGGY=1`）此燈
/// 必紅（回塞態 9 頁），頁數與查表值因果綁死；改表值會直接反映在此燈。
#[test]
fn fdo68607_page_count_locked_to_lo_8_pages() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    let pages = eng.page_count();
    assert_eq!(
        pages, 8,
        "fdo68607 頁數應嚴格 8＝LO（K7-cal face_advance_scale 查表收斂；\
         BUGGY 回塞態退 9 屬預期紅燈）"
    );
}

/// 零擾動守門臂（K7d 回歸鎖）：抽 3 檔 corpus——宣告 Cambria 檔、宣告
/// Arial 檔、查表未命中族檔各一——頁數與 pre-K7-cal 基線（5ae6fd8，
/// K7-cal 查表進場前的幾何態；以 /tmp/k7-base@5ae6fd8 實測二進位量得，
/// 並與本卡動工前 main tip 12980b8 現態逐位復核相同）逐位相同。
///
/// 語義：本鎖抓的是 **K7-cal 水平尺軸**的回歸（幾何軸已由紅燈 1/2 鎖死）。
/// 未命中查表族→`face_advance_scale` 回 1.0＝零擾動，水平尺對這些檔**不得**
/// 翻動任何分頁斷點。命中族（Cambria 0.9545／Arial 0.9950）尺≈1，亦不得
/// 翻頁；若日後動表值把這些檔頁數翻走，此燈先響。檔名以 zipfile 直掃
/// w:ascii（document＋styles）＋顯示列表實際 family 雙重定檔（Cambria
/// 渲染 42 span、Arial 渲染 7 span、Perpetua 查表未命中）。BUGGY 回塞態
/// 這些頁數同值＝本鎖非抓漏燈，是回歸鎖。
#[test]
fn zero_drift_page_counts_match_pre_k7cal_baseline() {
    // (檔名, 基線頁數@5ae6fd8 實測, 查表臂語義)
    const LOCKS: [(&str, usize, &str); 3] = [
        // 宣告 Cambria（查表命中 0.9545；p0 渲染 42 Cambria span）。
        ("ooxmlexport/FDO77715.docx", 29, "Cambria→Caladea 0.9545"),
        // 宣告 Arial（查表命中 0.9950；p0 渲染 7 Arial span）。
        (
            "ooxmlexport/n830205.docx",
            5,
            "Arial→Liberation Sans 0.9950",
        ),
        // 查表未命中族 Perpetua（→1.0 零擾動守門臂）。
        ("ooxmlexport/n780563.docx", 1, "Perpetua 未命中→1.0"),
    ];
    for (name, baseline, arm) in LOCKS {
        let eng = load_corpus(name);
        let pages = eng.page_count();
        assert_eq!(
            pages, baseline,
            "{name}（{arm}）頁數 {pages}≠pre-K7-cal 基線 {baseline}\
             （源 SHA 5ae6fd8 實測，main tip 12980b8 復核）——水平尺零擾動臂被破壞"
        );
    }
}
