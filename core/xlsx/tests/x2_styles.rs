//! X2 紅燈考卷——XLSX 樣式鏈唯讀投影（styles.xml 級聯＋數字格式 effective＋date1904）。
//!
//! ground truth 全部由 corpus-xlsx 實檔手抽 XML 原文對照；
//! 實檔沒有的形態（越界 s、越界 xfId、1900 Day 60 幻影日、負數序列）以單元合成考卷驗證，
//! 來源均在斷言註解寫明。寫入面不在本卡：全部斷言只讀。

use quire_xlsx::{
    parse_date_serial, read_styles, DateSerialError, NumFmtCategory, ParsedDateTime,
    StyleSheetModel, XlsxBundle,
};
use std::path::{Path, PathBuf};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-xlsx")
}

fn manifest_files() -> Vec<String> {
    let m = std::fs::read_to_string(corpus_root().join("manifest.json")).expect("manifest.json");
    m.split("\"file\": \"")
        .skip(1)
        .map(|chunk| chunk.split('"').next().unwrap_or_default().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn open(rel: &str) -> XlsxBundle {
    let bytes = std::fs::read(corpus_root().join(rel)).unwrap_or_else(|e| panic!("讀 {rel}: {e}"));
    XlsxBundle::open(&bytes).unwrap_or_else(|e| panic!("{rel} 開啟失敗: {e}"))
}

// ---------------------------------------------------------------- 紅燈 1：自訂與覆寫數字格式

/// 紅燈 1a：自訂 numFmt 與覆寫內建低號碼 ID（設計書 §1.2 與 §1.6 證據 1）。
/// xf_default_values.xlsx 在 <numFmts> 顯式定義了 ID 0, 1, 2，覆蓋預設定義：
/// - ID 0: formatCode="dd\.mm\.yyyy"（Date 類別）
/// - ID 1: formatCode="#,##0.00;[Red]\-#,##0.00"（Number 類別）
/// - ID 2: formatCode="#,##0.00%"（Percentage 類別）
#[test]
fn num_fmts_override_builtin_low_ids() {
    let b = open("lo/sc_qa_unit_data_xlsx_xf_default_values.xlsx");
    let styles = b.styles();

    // 驗證自訂字典精確保存 formatCode 原文字串
    assert_eq!(
        styles.num_fmts.get(&0).map(|s| s.as_str()),
        Some(r#"dd\.mm\.yyyy"#)
    );
    assert_eq!(
        styles.num_fmts.get(&1).map(|s| s.as_str()),
        Some(r#"#,##0.00;[Red]\-#,##0.00"#)
    );
    assert_eq!(
        styles.num_fmts.get(&2).map(|s| s.as_str()),
        Some(r#"#,##0.00%"#)
    );

    // 驗證 effective 解析時檔案宣告勝出（設計書 §4.3 情境 C）
    // xf_default_values.xlsx 中：cellXfs[0] 指向 ID 0，cellXfs[8] 指向 ID 1，cellXfs[10] 指向 ID 2
    let eff0 = styles.resolve_effective_style(Some(0));
    assert_eq!(eff0.num_fmt.id, 0);
    assert_eq!(eff0.num_fmt.format_code, r#"dd\.mm\.yyyy"#);
    assert_eq!(eff0.num_fmt.category, NumFmtCategory::Date);

    let eff1 = styles.resolve_effective_style(Some(8));
    assert_eq!(eff1.num_fmt.id, 1);
    assert_eq!(eff1.num_fmt.format_code, r#"#,##0.00;[Red]\-#,##0.00"#);
    assert_eq!(eff1.num_fmt.category, NumFmtCategory::Number);

    let eff2 = styles.resolve_effective_style(Some(10));
    assert_eq!(eff2.num_fmt.id, 2);
    assert_eq!(eff2.num_fmt.format_code, r#"#,##0.00%"#);
    assert_eq!(eff2.num_fmt.category, NumFmtCategory::Percentage);

    // 直接依 numFmtId 查詢有效格式
    assert_eq!(
        styles.resolve_effective_num_fmt(0).format_code,
        r#"dd\.mm\.yyyy"#
    );
    assert_eq!(
        styles.resolve_effective_num_fmt(1).format_code,
        r#"#,##0.00;[Red]\-#,##0.00"#
    );
    assert_eq!(
        styles.resolve_effective_num_fmt(2).format_code,
        r#"#,##0.00%"#
    );
}

/// 紅燈 1b：內建數字格式靜態表查表與未定義 ID 誠實標記（設計書 §2.5 與 §4.3）。
#[test]
fn builtin_num_fmts_and_unknown_fallback() {
    let empty_styles = StyleSheetModel::default();

    // ID 14：短日期 "mm-dd-yy"
    let eff14 = empty_styles.resolve_effective_style_for_num_fmt(14);
    assert_eq!(eff14.id, 14);
    assert_eq!(eff14.format_code, "mm-dd-yy");
    assert_eq!(eff14.category, NumFmtCategory::Date);

    // ID 49：純文字 "@"
    let eff49 = empty_styles.resolve_effective_style_for_num_fmt(49);
    assert_eq!(eff49.id, 49);
    assert_eq!(eff49.format_code, "@");
    assert_eq!(eff49.category, NumFmtCategory::Text);

    // 未定義自訂 ID 9999：Unknown，代碼為空字串，不猜不 panic（設計書 §4.3 情境 A）
    let eff9999 = empty_styles.resolve_effective_style_for_num_fmt(9999);
    assert_eq!(eff9999.id, 9999);
    assert_eq!(eff9999.format_code, "");
    assert_eq!(eff9999.category, NumFmtCategory::Unknown);
}

// ---------------------------------------------------------------- 紅燈 2：級聯演算法與 applyX 抑制規則

/// 紅燈 2：cellXfs 引用 cellStyleXfs 基準與 applyX="false" 繼承（設計書 §1.5 與 §1.6 證據 2）。
///
/// 實檔 formats.xlsx 的 cellXfs[24]：
/// - xfId="20"
/// - applyAlignment="false", applyBorder="false", applyFont="false", applyProtection="false"
/// - borderId="0", fillId="2", fontId="4", numFmtId="173"
///
/// cellStyleXfs[20]：
/// - applyAlignment="true", applyBorder="true", applyFont="true", applyProtection="true"
/// - borderId="0", fillId="2", fontId="4", numFmtId="173"
#[test]
fn cascade_resolution_with_apply_flags() {
    let b = open("lo/sc_qa_unit_data_xlsx_formats.xlsx");
    let styles = b.styles();

    assert!(styles.cell_xfs.len() > 24);
    assert!(styles.cell_style_xfs.len() > 20);

    let xf24 = &styles.cell_xfs[24];
    assert_eq!(xf24.xf_id, Some(20));
    assert_eq!(xf24.apply_font, Some(false));
    assert_eq!(xf24.apply_border, Some(false));
    assert_eq!(xf24.apply_alignment, Some(false));

    let eff = styles.resolve_effective_style(Some(24));
    assert_eq!(eff.s_index, 24);
    assert!(!eff.status.cell_xf_out_of_bounds);
    assert!(!eff.status.base_style_out_of_bounds);

    // 驗證字體與邊框成功透過基準樣式解析（設計書 §2.3）
    assert!(eff.font.is_some());
    assert!(eff.border.is_some());
}

// ---------------------------------------------------------------- 紅燈 3：具名樣式清單

/// 紅燈 3：具名樣式 cellStyles 映射到 cellStyleXfs（設計書 §1.6 證據 3）。
/// 實檔 poi 46535：
/// - "Normal" -> xfId="0", builtinId="0"
/// - "Check Cell" -> xfId="1", builtinId="23"
/// - "一般_Sheet1" -> xfId="4"
#[test]
fn named_cell_styles_mapping() {
    let b = open("poi/test-data_spreadsheet_46535.xlsx");
    let styles = b.styles();

    assert_eq!(styles.cell_styles.len(), 5);
    let normal = styles
        .cell_styles
        .iter()
        .find(|s| s.name == "Normal")
        .expect("應有 Normal 樣式");
    assert_eq!(normal.xf_id, 0);
    assert_eq!(normal.builtin_id, Some(0));

    let check_cell = styles
        .cell_styles
        .iter()
        .find(|s| s.name == "Check Cell")
        .expect("應有 Check Cell 樣式");
    assert_eq!(check_cell.xf_id, 1);
    assert_eq!(check_cell.builtin_id, Some(23));

    let custom = styles
        .cell_styles
        .iter()
        .find(|s| s.name == "一般_Sheet1")
        .expect("應有 一般_Sheet1 自訂樣式");
    assert_eq!(custom.xf_id, 4);
}

// ---------------------------------------------------------------- 紅燈 4：date1904 與序列值語義

/// 紅燈 4a：1904 日期系統開啟（設計書 §3.3 與 §3.5）。
/// 實檔 tdf165180_date1904.xlsx：
/// - workbookPr 宣告 date1904="true"
/// - A1 儲存格序列值為 60，1904 系統下解析為 1904-03-01
/// - A2 儲存格序列值為 37145，1904 系統下解析為 2005-09-12
#[test]
fn date1904_system_enabled_parses_serial_correctly() {
    let b = open("lo/sc_qa_unit_data_xlsx_tdf165180_date1904.xlsx");
    let wb = b.workbook_model();
    assert!(wb.date1904, "tdf165180 活頁簿 date1904 應為 true");

    // 序列 60 在 1904 系統下為 1904-03-01（設計書 §3.5 考卷矩陣）
    let dt60 = parse_date_serial(60.0, true).expect("1904 下序列 60 應解析成功");
    assert_eq!(
        dt60,
        ParsedDateTime::DateTime {
            year: 1904,
            month: 3,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: 0,
        }
    );

    // 序列 37145 在 1904 系統下為 2005-09-12
    let dt37145 = parse_date_serial(37145.0, true).expect("1904 下序列 37145 應解析成功");
    assert_eq!(
        dt37145,
        ParsedDateTime::DateTime {
            year: 2005,
            month: 9,
            day: 12,
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: 0,
        }
    );
}

/// 紅燈 4b：1900 日期系統關閉（預設）常規與邊界值（設計書 §3.2 與 §3.5）。
///
/// 實檔 poi 61652：
/// - workbookPr 缺省 date1904 -> false
/// - Sheet2 A1 序列 0 -> TimeOnly (00:00:00)
/// - Sheet2 B4 序列 31 -> 1900-01-31
///
/// 實檔 xlsm tdf111974：
/// - 序列 42961 -> 2017-08-14
/// - 序列 345 -> 1900-12-10
#[test]
fn date1900_system_parses_serials_correctly() {
    let b = open("poi/test-data_spreadsheet_61652.xlsx");
    let wb = b.workbook_model();
    assert!(!wb.date1904, "poi 61652 date1904 預設應為 false");

    // 序列 0：TimeOnly（設計書 §3.4 表格）
    let dt0 = parse_date_serial(0.0, false).expect("序列 0 應為 TimeOnly");
    assert_eq!(
        dt0,
        ParsedDateTime::TimeOnly {
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: 0,
        }
    );

    // 序列 31：1900-01-31
    let dt31 = parse_date_serial(31.0, false).expect("序列 31 應解析為 1900-01-31");
    assert_eq!(
        dt31,
        ParsedDateTime::DateTime {
            year: 1900,
            month: 1,
            day: 31,
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: 0,
        }
    );

    // 實檔 xlsm tdf111974 測試
    let b_xlsm = open("lo/sc_qa_unit_data_xlsm_tdf111974.xlsm");
    assert!(!b_xlsm.workbook_model().date1904);

    let dt42961 = parse_date_serial(42961.0, false).expect("序列 42961 應為 2017-08-14");
    assert_eq!(
        dt42961,
        ParsedDateTime::DateTime {
            year: 2017,
            month: 8,
            day: 14,
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: 0,
        }
    );

    let dt345 = parse_date_serial(345.0, false).expect("序列 345 應為 1900-12-10");
    assert_eq!(
        dt345,
        ParsedDateTime::DateTime {
            year: 1900,
            month: 12,
            day: 10,
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: 0,
        }
    );
}

/// 紅燈 4c：自製邊界 fixture（Day 60 幻影日、負數序列；設計書 §3.4 與 §3.5）。
#[test]
fn date_boundaries_and_fixtures() {
    // 1900 下序列 60：Lotus 1-2-3 幻影閏日，誠實回報 Err(PhantomLeapDay1900)
    let res60 = parse_date_serial(60.0, false);
    assert_eq!(res60, Err(DateSerialError::PhantomLeapDay1900));

    // 負數序列值：回報 Err(NegativeSerial)
    let res_neg = parse_date_serial(-1.5, false);
    assert_eq!(res_neg, Err(DateSerialError::NegativeSerial(-1.5)));

    // 1900 下序列 59：1900-02-28
    let dt59 = parse_date_serial(59.0, false).expect("序列 59 應為 1900-02-28");
    assert_eq!(
        dt59,
        ParsedDateTime::DateTime {
            year: 1900,
            month: 2,
            day: 28,
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: 0,
        }
    );

    // 1900 下序列 61：1900-03-01
    let dt61 = parse_date_serial(61.0, false).expect("序列 61 應為 1900-03-01");
    assert_eq!(
        dt61,
        ParsedDateTime::DateTime {
            year: 1900,
            month: 3,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: 0,
        }
    );

    // 1904 下序列 0：1904-01-01
    let dt0_1904 = parse_date_serial(0.0, true).expect("1904 序列 0 應為 1904-01-01");
    assert_eq!(
        dt0_1904,
        ParsedDateTime::DateTime {
            year: 1904,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: 0,
        }
    );
}

// ---------------------------------------------------------------- 紅燈 5：病態容錯（越界與缺失）

/// 紅燈 5：越界 xf 與缺失樣式檔誠實標記（設計書 §4.1 與 §4.2）。
#[test]
fn pathological_tolerance_honesty() {
    let xml = br#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
        <cellXfs count="1">
            <xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="99"/>
        </cellXfs>
    </styleSheet>"#;
    let styles = read_styles(xml);

    // s=999 越界：不 panic，退回首筆並標記 cell_xf_out_of_bounds
    let eff_oob = styles.resolve_effective_style(Some(999));
    assert!(eff_oob.status.cell_xf_out_of_bounds);
    assert!(!eff_oob.status.warnings.is_empty());

    // xfId=99 基準越界（cellStyleXfs 為空）：不 panic，標記 base_style_out_of_bounds
    let eff_base_oob = styles.resolve_effective_style(Some(0));
    assert!(eff_base_oob.status.base_style_out_of_bounds);

    // 缺 styles.xml：回傳 default，不 panic（設計書 §4.5）
    let empty = read_styles(b"");
    assert_eq!(empty, StyleSheetModel::default());
}

// ---------------------------------------------------------------- 紅燈 6：儲存格投影結合樣式

/// 紅燈 6：cell 投影保留 s 屬性並解析為具體 EffectiveStyle（任務卡目標 1 與 2）。
#[test]
fn cell_projection_resolves_effective_style() {
    let b = open("poi/test-data_spreadsheet_61652.xlsx");
    let sheet_info = b
        .workbook_model()
        .sheets
        .into_iter()
        .find(|s| s.name == "Model")
        .expect("應有 Model 工作表（sheet2）");
    let part = sheet_info.part.expect("Sheet2 應有 part");
    let cells = b.sheet_cells(&part).expect("應成功讀取 cell 投影");

    // 找到 A1 儲存格（XML 原文：<c r="A1" s="12"><f>IRPPCalc!D6</f><v>0</v></c>）
    let a1 = cells
        .rows
        .iter()
        .flat_map(|r| r.cells.iter())
        .find(|c| c.reference.as_deref() == Some("A1"))
        .expect("應有 A1 儲存格");

    assert_eq!(a1.s, Some(12), "A1 原始樣式索引應為 12");
    let st = a1.style.as_ref().expect("A1 應包含解析後的有效樣式");
    assert_eq!(st.s_index, 12);
    assert_eq!(st.num_fmt.id, 14);
    assert_eq!(st.num_fmt.category, NumFmtCategory::Date);
    assert!(!st.status.cell_xf_out_of_bounds);
}

// ---------------------------------------------------------------- 紅燈 7：語料庫普查

/// 紅燈 7：全 80 檔跑一遍 styles 解析不得 panic（設計書驗收指標）。
#[test]
fn corpus_all_80_files_styles_never_panic() {
    for rel in manifest_files() {
        let b = open(&rel);
        let styles = b.styles();
        // 嘗試解析首筆樣式，驗證無未捕捉 panic
        let _ = styles.resolve_effective_style(Some(0));
    }
}
