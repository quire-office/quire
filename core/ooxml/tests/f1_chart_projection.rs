//! F1 燈 1：chart part 解析層的逐字比對考卷（卡 t_f3acd271）。
//!
//! 口徑來源全部出自 `docs/f1-chart-anatomy.md` 的實拆（probe_chart.py／
//! probe_parts.py／survey_charts.py 對 `corpus/` 實抓），非推測：
//! - `chart-size.docx`：1 系列、`c:val/c:numCache` 三點 `1,2,3`、**無 `c:cat`**
//!   （LO 自動編號 `1/2/3`）、`c:title` 存在但**無 `c:tx`**（空標題）、
//!   `c:valAx` 帶 `c:majorGridlines`、`c:catAx` 不帶。
//! - `strict.docx`：同一幾何的 **strict 命名空間**版（解析器兩式都要認）。
//! - `fdo76098.docx`：pieChart，`c:title` 帶真文字 `Sales`（標題發射的實據檔）。
//!
//! 本檔只鎖「解析產物的逐字值」；發射落點屬燈 2
//! （`core/engine-api/tests/f1_chart_emission.rs`）。

use std::path::{Path, PathBuf};

fn corpus(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    let p = Path::new(&p).canonicalize().unwrap_or(p);
    assert!(p.exists(), "語料缺失：{}", p.display());
    p
}

fn theme_of(path: &Path) -> quire_ooxml::ThemeColors {
    let bytes = std::fs::read(path).expect("讀不到語料");
    let bundle = quire_ooxml::ZipBundle::open(&bytes).expect("開不了 zip");
    let parts = bundle.decode_parts().expect("解不了 parts");
    quire_ooxml::ThemeColors::from_parts(&parts)
}

/// 燈 1a：chart-size 的系列數、numCache 逐字、自動類別編號、空標題不虛構。
#[test]
fn f1_lamp1a_chart_size_series_and_cache_values() {
    let path = corpus("ooxmlexport/chart-size.docx");
    let theme = theme_of(&path);
    let bytes = std::fs::read(&path).unwrap();
    let charts = quire_ooxml::read_charts_from_docx(&bytes, &theme);
    // mc:Choice／mc:Fallback 雙份必須去重成一份（解剖報告 §1 要點 A）。
    assert_eq!(charts.len(), 1, "chart-size 去重後應恰為 1 個掛載");
    let (mount, doc) = &charts[0];
    assert_eq!(mount.part, "word/charts/chart1.xml");
    assert!(mount.in_txbx, "chart-size 圖表掛在文字框內（解剖報告 §1）");
    assert!(!mount.floating, "掛載形態是 wp:inline");
    // wp:extent 2162810×1297940 EMU ÷12700 = 170.30 × 102.20 pt。
    assert!((mount.w_pt - 170.30).abs() < 0.02, "寬 {:?}", mount.w_pt);
    assert!((mount.h_pt - 102.20).abs() < 0.02, "高 {:?}", mount.h_pt);

    assert_eq!(doc.kinds, vec!["barChart".to_string()], "圖型");
    assert_eq!(doc.series.len(), 1, "系列數");
    let s = &doc.series[0];
    // cache 值優先：c:numCache 三點逐字 1,2,3（不是引用式重算出來的）。
    assert_eq!(s.vals, vec![1.0, 2.0, 3.0], "c:val/c:numCache 逐字");
    assert_eq!(
        s.val_formula.as_deref(),
        Some("Sheet1!$F$3:$F$5"),
        "c:f 只留原始字串供診斷"
    );
    assert!(s.cats.is_empty(), "本檔無 c:cat（解剖報告 §2）");
    assert_eq!(s.auto_cats, 3, "無 c:cat 時的點數");
    // 生效類別＝LO 同律的自動編號 1/2/3（解剖報告 §3b）。
    assert_eq!(doc.effective_cats(), vec!["1", "2", "3"]);
    // 空標題不虛構：c:title 宣告存在、但無 c:tx ⇒ title 必為 None。
    assert!(doc.title_declared, "c:title 元素存在");
    assert_eq!(doc.title, None, "LO 本地化佔位「圖表標題」不是檔內內容，不得虛構");
    // 軸宣告：valAx 帶 majorGridlines、catAx 不帶；兩者皆未刪除。
    assert!(doc.val_ax.present && !doc.val_ax.deleted);
    assert!(doc.val_ax.major_gridlines, "valAx 有 c:majorGridlines");
    assert!(doc.cat_ax.present && !doc.cat_ax.deleted);
    assert!(!doc.cat_ax.major_gridlines, "catAx 無格線");
    // 刻度字級宣告 sz=900 → 9pt。
    assert_eq!(doc.val_ax.tick_size_pt, Some(9.0));
    assert_eq!(doc.cat_ax.tick_size_pt, Some(9.0));
    assert!(doc.plottable, "barChart 屬可發幾何平面圖型");
    assert!(doc.bar_dir_col, "c:barDir=val=\"col\"");
    assert!(!doc.stacked, "c:grouping=clustered");
    // 資料上下界（LO 律 L2/L3 的輸入）。
    assert_eq!(doc.data_max(), Some(3.0));
    assert_eq!(doc.data_min(), Some(0.0));
}

/// 燈 1b：strict 命名空間版必須同样命中（語料普查實據：strict.docx 用
/// `http://purl.oclc.org/ooxml/drawingml/chart`，且屬性值是百分比字串）。
#[test]
fn f1_lamp1b_chart_part_parses_under_strict_namespace() {
    let path = corpus("ooxmlexport/strict.docx");
    let theme = theme_of(&path);
    let bytes = std::fs::read(&path).unwrap();
    let charts = quire_ooxml::read_charts_from_docx(&bytes, &theme);
    assert!(!charts.is_empty(), "strict 命名空間必須零命中之外的事實：要解析得到");
    let (_, doc) = charts
        .iter()
        .find(|(_, d)| d.plottable)
        .expect("strict 的 barChart");
    assert_eq!(doc.kinds, vec!["barChart".to_string()]);
    assert_eq!(doc.series.len(), 1);
    assert_eq!(doc.series[0].vals, vec![1.0, 2.0, 3.0]);
    assert_eq!(doc.effective_cats(), vec!["1", "2", "3"]);
}

/// 燈 1c：真有 `c:title/c:tx` 的檔案必須解析出標題文字（fdo76098：`Sales`），
/// 與燈 1a 的「空標題不虛構」構成雙向鑑別。
#[test]
fn f1_lamp1c_title_text_parsed_when_present() {
    let path = corpus("ooxmlexport/fdo76098.docx");
    let theme = theme_of(&path);
    let bytes = std::fs::read(&path).unwrap();
    let charts = quire_ooxml::read_charts_from_docx(&bytes, &theme);
    assert!(!charts.is_empty());
    let titles: Vec<Option<String>> = charts.iter().map(|(_, d)| d.title.clone()).collect();
    assert!(
        titles.iter().any(|t| t.as_deref() == Some("Sales")),
        "fdo76098 的 c:title 真文字 Sales 必須解析出來，實測 {titles:?}"
    );
    // pieChart 不發平面幾何（解剖報告 §5-2），但標題與 cache 值照解析。
    let pie = charts.iter().find(|(_, d)| d.kinds.iter().any(|k| k == "pieChart"));
    if let Some((_, d)) = pie {
        assert!(!d.plottable, "pieChart 屬明確不做臂，plottable 必 false");
        assert!(!d.series.is_empty());
    }
}

/// 燈 1d：無圖表檔案必須回空清單（零擾動快路徑；新發射不得讓無圖表檔
/// 的顯示列表產生任何位元組差異的前置條件）。
#[test]
fn f1_lamp1d_files_without_charts_return_empty() {
    for rel in [
        "ooxmlexport/fdo76591.docx",
        "ooxmlexport/99_Fields.docx",
        "ooxmlimport/n751077.docx",
    ] {
        let path = corpus(rel);
        let theme = theme_of(&path);
        let bytes = std::fs::read(&path).unwrap();
        let charts = quire_ooxml::read_charts_from_docx(&bytes, &theme);
        assert!(charts.is_empty(), "{rel} 無圖表，實測 {} 個掛載", charts.len());
    }
}
