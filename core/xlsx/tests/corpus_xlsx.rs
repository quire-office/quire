//! XLSX corpus 閘門（X1 卡）：80 檔容器級 byte-identical round-trip ＋
//! workbook 模型斷言 ＋ 病態容錯固定行為 ＋ dimension/row 計數交叉核對。
//!
//! 口徑來源：corpus-xlsx/manifest.json（80 檔＝poi 40＋LibreOffice 40）。
//! 測試直接吃 git 追蹤的 corpus 實體檔（鏡像 ooxml 的 corpus 柵欄做法）。

use quire_xlsx::{SheetState, XlsxBundle, XlsxError};
use std::path::{Path, PathBuf};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-xlsx")
}

/// manifest 的 "file" 欄（與 shells/native/ooxml corpus 測試同款轻量抽取，
/// 避免為一個清單拉進 serde_json 依賴）。
fn manifest_files() -> Vec<String> {
    let m = std::fs::read_to_string(corpus_root().join("manifest.json")).expect("manifest.json");
    m.split("\"file\": \"")
        .skip(1)
        .map(|chunk| chunk.split('"').next().unwrap_or_default().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn read(rel: &str) -> Vec<u8> {
    std::fs::read(corpus_root().join(rel)).unwrap_or_else(|e| panic!("讀 {rel}: {e}"))
}

/// 閘門 1：全 80 檔 開檔→（不改任何 part）→存回 = byte-identical。
/// 這一條就是本卡的存在意義：DOCX 的無損紀律（ZipBundle）原封不動複製到 XLSX。
#[test]
fn corpus_all_byte_identical_round_trip() {
    let files = manifest_files();
    assert_eq!(files.len(), 80, "corpus-xlsx 應為 80 檔");
    let mut checked = 0usize;
    for rel in &files {
        let bytes = read(rel);
        let bundle = XlsxBundle::open(&bytes).unwrap_or_else(|e| panic!("{rel} 開啟失敗: {e}"));
        let saved = bundle
            .save()
            .unwrap_or_else(|e| panic!("{rel} 存檔失敗: {e}"));
        assert_eq!(
            saved.as_slice(),
            bytes.as_slice(),
            "{rel}：未觸碰存檔必須位元組級相同"
        );
        checked += 1;
    }
    assert_eq!(checked, 80);
}

/// 閘門 1b：存回結果再開一次，part 清單一致（容器級語義不退化）。
#[test]
fn corpus_part_lists_survive_round_trip() {
    for rel in manifest_files() {
        let bytes = read(&rel);
        let b = XlsxBundle::open(&bytes).expect("open");
        let again = XlsxBundle::open(&b.save().expect("save")).expect("reopen");
        assert_eq!(b.part_names(), again.part_names(), "{rel}：part 清單應一致");
    }
}

/// 模型斷言 1：POI 典型檔 1_NoIden——單 sheet（韓文名＋全形括號）、sheetId=2、
/// rId 歸屬經 rels 換算成 xl/worksheets/sheet1.xml、dimension B1:D9、9 rows、
/// definedNames 計數 1。
#[test]
fn poi_1_noiden_sheet_list() {
    let bytes = read("poi/test-data_spreadsheet_1_NoIden.xlsx");
    let b = XlsxBundle::open(&bytes).expect("open");
    let m = b.workbook_model();
    assert_eq!(m.sheets.len(), 1);
    let s = &m.sheets[0];
    assert_eq!(s.name, "시트_테스트(1)");
    assert_eq!(s.sheet_id, Some(2));
    assert_eq!(s.state, SheetState::Visible);
    assert_eq!(s.rid.as_deref(), Some("rId1"));
    assert_eq!(s.part.as_deref(), Some("xl/worksheets/sheet1.xml"));
    let st = s.stats.as_ref().expect("sheet1 統計應存在");
    assert_eq!(st.dimension.as_deref(), Some("B1:D9"));
    assert_eq!(st.row_count, 9);
    assert_eq!(m.defined_names, 1);
    // content types：Default 至少 rels/xml/bin 三項，Override 有 workbook。
    assert!(m.content_types.defaults.iter().any(|(e, _)| e == "rels"));
    assert!(m
        .content_types
        .overrides
        .iter()
        .any(|(p, _)| p == "/xl/workbook.xml"));
}

/// 模型斷言 2：LO tdf 檔 sheet state——tdf111974.xlsm（LO sc/qa 真實檔）含
/// hidden 與 veryHidden；10 sheet 全數投影，空 r:id（macro-only）容錯為 None。
#[test]
fn lo_tdf_sheet_states_hidden() {
    let bytes = read("lo/sc_qa_unit_data_xlsm_tdf111974.xlsm");
    let b = XlsxBundle::open(&bytes).expect("open");
    let m = b.workbook_model();
    assert_eq!(m.sheets.len(), 10);
    let hidden: Vec<&str> = m
        .sheets
        .iter()
        .filter(|s| s.state == SheetState::Hidden)
        .map(|s| s.name.as_str())
        .collect();
    let very: Vec<&str> = m
        .sheets
        .iter()
        .filter(|s| s.state == SheetState::VeryHidden)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(hidden.len(), 4, "hidden 應 4 份，實際 {hidden:?}");
    assert_eq!(very.len(), 3, "veryHidden 應 3 份，實際 {very:?}");
    // macro-only sheet（r:id=""）：宣告保留、part 歸屬為 None（不虛構）。
    let mo = m
        .sheets
        .iter()
        .find(|s| s.name == "Roll weeks macro")
        .expect("Roll weeks macro 宣告應保留");
    assert_eq!(mo.rid, None);
    assert_eq!(mo.part, None);
    assert_eq!(mo.stats, None);
    // definedNames 計數（grep 口徑 <definedName 直子計 32；交叉核對見下）。
    assert_eq!(m.defined_names, 32);
}

/// LO tdf171828（hidden sheet 真實檔）：3 sheets，Hitab 為 hidden，
/// 且 hidden sheet 的 part 照常歸屬（hidden ≠ 不存在）。
#[test]
fn lo_tdf171828_hidden_sheet_still_resolves_part() {
    let bytes = read("lo/sc_qa_unit_data_xlsx_tdf171828_fail_to_import_file.xlsx");
    let b = XlsxBundle::open(&bytes).expect("open");
    let m = b.workbook_model();
    assert_eq!(m.sheets.len(), 3);
    let hitab = m.sheets.iter().find(|s| s.name == "Hitab").expect("Hitab");
    assert_eq!(hitab.state, SheetState::Hidden);
    assert_eq!(hitab.part.as_deref(), Some("xl/worksheets/sheet3.xml"));
    let st = hitab.stats.as_ref().expect("Hitab 統計");
    assert_eq!(st.dimension.as_deref(), Some("A1:P15"));
    assert_eq!(st.row_count, 14);
}

/// 交叉核對（卡面「unzip grep 抽 3 檔」）：第二條獨立實作——對 part 明文做
/// 最笨的位元組掃描計數（與 quick-xml 狀態機完全分路），3 檔逐 sheet 比對
/// dimension 首現值與 row 標籤計數。
#[test]
fn dimension_row_counts_cross_checked_against_naive_scanner() {
    // (檔案, 期望核對的 sheet part 數下限)
    let cases = [
        "poi/test-data_spreadsheet_1_NoIden.xlsx",
        "lo/sc_qa_unit_data_xlsx_tdf171828_fail_to_import_file.xlsx",
        "poi/test-data_spreadsheet_mv-calculator-final-2-20-2013.xlsm",
    ];
    for rel in cases {
        let bytes = read(rel);
        let b = XlsxBundle::open(&bytes).expect("open");
        let m = b.workbook_model();
        let mut n = 0usize;
        for s in &m.sheets {
            let (Some(part), Some(stats)) = (&s.part, &s.stats) else {
                continue;
            };
            let plain = b.part(part).expect("part 明文");
            let (dim, rows) = naive_scan(&plain);
            assert_eq!(
                stats.dimension, dim,
                "{rel} {part}：dimension 與笨掃描器分岔"
            );
            assert_eq!(
                stats.row_count, rows,
                "{rel} {part}：row 計數與笨掃描器分岔"
            );
            n += 1;
        }
        assert!(n >= 1, "{rel}：至少要比對一份 sheet");
    }
}

/// 笨掃描器（僅供測試交叉核對）：主命名空間的 `<row` 標籤計數＋
/// `<dimension ref="` 首現值。與主解析器完全分路（純位元組掃描、無事件
/// 狀態機），但共用兩條已驗證結論：
/// 1. `<row` 後面必須是空白、`/` 或 `>` 才算 row 標籤（`<rowBreaks` 排除）；
/// 2. `<` 與 `row` 之間有 `:` 即帶前綴 → 非預設 NS（`<xdr:row>` 是繪圖
///    錨點列座標，不是工作表列）。
fn naive_scan(xml: &[u8]) -> (Option<String>, usize) {
    let text = String::from_utf8_lossy(xml);
    let mut dim = None;
    if let Some(at) = text.find("<dimension ") {
        if let Some(q1) = text[at..].find("ref=\"") {
            let s = &text[at + q1 + 5..];
            if let Some(q2) = s.find('"') {
                dim = Some(s[..q2].to_string());
            }
        }
    }
    let mut rows = 0usize;
    let mut from = 0usize;
    while let Some(at) = text[from..].find("<row") {
        let abs = from + at;
        let next_ok = matches!(
            text.as_bytes().get(abs + 4),
            Some(&b' ') | Some(&b'/') | Some(&b'>') | None
        );
        let prefixed = text[abs + 1..abs + 4].contains(':');
        if next_ok && !prefixed {
            rows += 1;
        }
        from = abs + 4;
    }
    (dim, rows)
}

/// round-trip 語義 1（macroEnabled）：xlsm 的 vbaProject.bin part **原樣搬運**
/// ——壓縮位元組（data_raw）與 entry 標頭逐位元組不變，內容也不變。
#[test]
fn xlsm_vba_project_part_moves_untouched() {
    let bytes = read("lo/sc_qa_unit_data_xlsm_tdf111974.xlsm");
    let b = XlsxBundle::open(&bytes).expect("open");
    assert!(b.part("xl/vbaProject.bin").is_some(), "vbaProject.bin 應在");
    let saved = XlsxBundle::open(&b.save().expect("save")).expect("reopen");
    assert_eq!(
        b.part("xl/vbaProject.bin").unwrap(),
        saved.part("xl/vbaProject.bin").unwrap(),
        "macro part 明文必須逐位元組原樣"
    );
    // 整體 byte-identical 已在 corpus 閘門斷言；這裡再釘 macro part 的存在性。
    assert!(saved
        .part_names()
        .contains(&"xl/vbaProject.bin".to_string()));
}

/// round-trip 語義 2（未知 part）：Content_Types 沒登記的自訂 part 原樣保留。
/// 真實 corpus 的形态：poi 檔常見 customXml／無 Override 的 part——掃全 corpus，
/// 找「不在任何 Override 且副檔名不在 Default」的 part，確認存回後仍在。
#[test]
fn unknown_parts_survive_round_trip() {
    let mut seen = 0usize;
    for rel in manifest_files() {
        let bytes = read(&rel);
        let b = XlsxBundle::open(&bytes).expect("open");
        let m = b.workbook_model();
        let defaults: Vec<String> = m
            .content_types
            .defaults
            .iter()
            .map(|(e, _)| e.to_ascii_lowercase())
            .collect();
        let overrides: Vec<String> = m
            .content_types
            .overrides
            .iter()
            .map(|(p, _)| p.clone())
            .collect();
        for name in b.part_names() {
            let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            let covered = defaults.iter().any(|d| d == &ext)
                || overrides.iter().any(|p| p == &format!("/{name}"));
            if covered {
                continue;
            }
            // Content_Types 完全沒登記的 part（未知 part）：存回後必須還在。
            let saved = XlsxBundle::open(&b.save().expect("save")).expect("reopen");
            assert!(
                saved.part_names().contains(&name),
                "{rel}：未知 part {name} 在存回後消失"
            );
            seen += 1;
        }
        if seen >= 5 {
            break; // 抽樣即止：全 corpus byte-identical 閘門已覆蓋逐位元組面。
        }
    }
    assert!(
        seen >= 1,
        "corpus 應至少有一個未登記 part（customXml 等形態）"
    );
}

/// 病態容錯固定行為 1：缺 [Content_Types].xml → Err（不是有效 OPC 包裝）。
#[test]
fn missing_content_types_is_err() {
    let src = read("poi/test-data_spreadsheet_1_NoIden.xlsx");
    let without = strip_entry(&src, "[Content_Types].xml");
    assert!(
        matches!(
            XlsxBundle::open(&without),
            Err(XlsxError::MissingContentTypes)
        ),
        "缺 [Content_Types].xml 必須誠實 Err"
    );
}

/// 病態容錯固定行為 2：缺 xl/_rels/workbook.xml.rels → sheet 清單空（不猜 rId）。
/// workbook.xml 本身還在、definedNames 計數照常（誠實回報能讀的部分）。
#[test]
fn missing_workbook_rels_gives_empty_sheet_list() {
    let src = read("poi/test-data_spreadsheet_1_NoIden.xlsx");
    let without = strip_entry(&src, "xl/_rels/workbook.xml.rels");
    let b = XlsxBundle::open(&without).expect("缺 rels 不影響容器開啟");
    let m = b.workbook_model();
    assert!(m.sheets.is_empty(), "缺 rels → sheet 清單必須空集合");
    assert!(m.rels.is_empty());
    assert_eq!(
        m.defined_names, 1,
        "definedNames 計數與 rels 無關，照常回報"
    );
}

/// 病態容錯固定行為 3：workbook.xml 缺 part → 模型全空但不炸。
#[test]
fn missing_workbook_xml_is_empty_model() {
    let src = read("poi/test-data_spreadsheet_1_NoIden.xlsx");
    let without = strip_entry(&src, "xl/workbook.xml");
    let b = XlsxBundle::open(&without).expect("open");
    let m = b.workbook_model();
    assert!(m.sheets.is_empty());
    assert_eq!(m.defined_names, 0);
}

/// fixtures：純 xlsx 與含 macro part 的 xlsm 都必須 byte-identical，
/// 且 macro fixture 的未知 part（Content_Types 未登記）也原樣保留。
#[test]
fn fixtures_round_trip_byte_identical() {
    let fix = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (name, sheet_n) in [("xlsx_minimal.xlsx", 2usize), ("xlsx_macro.xlsm", 3usize)] {
        let bytes = std::fs::read(fix.join(name)).unwrap_or_else(|e| panic!("讀 {name}: {e}"));
        let b = XlsxBundle::open(&bytes).expect(name);
        assert_eq!(
            b.save().expect("save").as_slice(),
            bytes.as_slice(),
            "{name} 必須 byte-identical"
        );
        let m = b.workbook_model();
        assert_eq!(m.sheets.len(), sheet_n, "{name} sheet 數");
    }
    // xlsm fixture：macro part 原樣＋未知 part 保留＋veryHidden state。
    let bytes = std::fs::read(fix.join("xlsx_macro.xlsm")).expect("macro fixture");
    let b = XlsxBundle::open(&bytes).expect("open");
    let m = b.workbook_model();
    assert!(b.part("xl/vbaProject.bin").is_some());
    assert!(m.sheets.iter().any(|s| s.state == SheetState::VeryHidden));
    assert!(m.content_types.defaults.iter().any(|(e, _)| e == "bin"));
    let again = XlsxBundle::open(&b.save().expect("save")).expect("reopen");
    assert!(again
        .part_names()
        .contains(&"xl/custom/unknown-part.xml".to_string()));
}

/// 把一個 entry 從 zip 裡拿掉（生成「缺 part」病態檔）：直接用 quire-ooxml 的
/// `Change::Remove`（test target 看得到本 crate 的 dependencies；容器層的
/// Remove 能力住在 ZipBundle，XlsxBundle 刻意不暴露編輯面）。
fn strip_entry(bytes: &[u8], name: &str) -> Vec<u8> {
    let b = quire_ooxml::ZipBundle::open(bytes).expect("open for strip");
    assert!(
        b.entry(name).is_some() && !b.entry(name).unwrap().frozen,
        "測試前提：{name} 必須存在且可移除"
    );
    let mut changes = std::collections::BTreeMap::new();
    changes.insert(name.to_string(), quire_ooxml::Change::Remove);
    let extras = std::collections::BTreeMap::new();
    b.serialize(&changes, &extras).expect("strip serialize")
}
