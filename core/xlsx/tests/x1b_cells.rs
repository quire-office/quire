//! X1b 紅燈考卷——XLSX cell 級唯讀投影（sharedStrings＋cell 型別＋sheetData 掃描）。
//!
//! ground truth 全部由 corpus-xlsx 實檔手抽 XML 原文對照（各斷言旁附原文出處）；
//! 實檔沒有的形態（Unknown t、非法 ref、懸空 s——corpus 八檔實掃零出現）以
//! 內嵌 XML 或「實檔＋zip 層派生改動」補釘，來源均在斷言註解寫明，不虛構。
//! 寫入面不在本卡：全部斷言只讀。

use quire_xlsx::{CellValue, DimensionStatus, XlsxBundle};
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

/// 依 sheet 名稱取 cell 投影（找不到＝考卷前提失效，直接 panic）。
fn cells_of(b: &XlsxBundle, sheet: &str) -> quire_xlsx::SheetCells {
    let info = b
        .workbook_model()
        .sheets
        .into_iter()
        .find(|s| s.name == sheet)
        .unwrap_or_else(|| panic!("無 sheet {sheet}"));
    let part = info.part.unwrap_or_else(|| panic!("sheet {sheet} 無 part"));
    b.sheet_cells(&part)
        .unwrap_or_else(|| panic!("sheet part {part} 讀取失敗"))
}

/// 依 part 路徑取投影（檔內 sheet 名不可靠的檔走這條）。
fn cells_of_part(b: &XlsxBundle, part: &str) -> quire_xlsx::SheetCells {
    b.sheet_cells(part)
        .unwrap_or_else(|| panic!("sheet part {part} 讀取失敗"))
}

fn cell_at<'a>(sc: &'a quire_xlsx::SheetCells, reference: &str) -> &'a quire_xlsx::Cell {
    sc.rows
        .iter()
        .flat_map(|r| r.cells.iter())
        .find(|c| c.reference.as_deref() == Some(reference))
        .unwrap_or_else(|| panic!("無 cell {reference}（考卷前提失效）"))
}

/// 把 SST 截到只剩前 keep_si 條 `<si>`（派生病態檔）：字串級截斷即可，
/// 目的只是讓尾部索引越界成懸空引用。
fn truncate_sst(bytes: &[u8], keep_si: usize) -> Vec<u8> {
    let zb = quire_ooxml::ZipBundle::open(bytes).expect("open for truncate");
    let entry = zb
        .entry(quire_xlsx::SHARED_STRINGS_PART)
        .expect("fixture 前提：SST 存在");
    assert!(!entry.frozen, "fixture 前提：SST entry 可改寫");
    let plain = entry.decoded().expect("decoded");
    let text = String::from_utf8_lossy(&plain).into_owned();
    let mut pos = 0usize;
    for _ in 0..keep_si {
        let at = text[pos..]
            .find("<si")
            .unwrap_or_else(|| panic!("SST 不足 {keep_si} 條 si"));
        pos += at + 1;
    }
    let cut = text[pos..]
        .find("<si")
        .map(|at| pos + at)
        .unwrap_or(text.len());
    let tail_at = text.rfind("</sst>").expect("sst 閉合標籤");
    let mut new_xml = text[..cut].to_string();
    new_xml.push_str(&text[tail_at..]);
    let mut changes = std::collections::BTreeMap::new();
    changes.insert(
        quire_xlsx::SHARED_STRINGS_PART.to_string(),
        quire_ooxml::Change::Replace(new_xml.into_bytes()),
    );
    let extras = std::collections::BTreeMap::new();
    zb.serialize(&changes, &extras).expect("truncate serialize")
}

// ---------------------------------------------------------------- 紅燈 1：sharedStrings

/// 紅燈 1a：純 `<t>` si——POI 1_NoIden 的 SST 全 15 條逐條對照（手抽原文
/// `<si><t>하천정비사업 시설부대비</t></si>` …，si[13] 為空條）。
/// 空條 → ""（合法空字串），與懸空引用（None）分家。
#[test]
fn sst_plain_items_match_hand_extracted_xml() {
    let b = open("poi/test-data_spreadsheet_1_NoIden.xlsx");
    let sst = b.shared_strings().expect("1_NoIden 應有 sharedStrings");
    assert_eq!(sst.items.len(), 15);
    let want = [
        "하천정비사업 시설부대비",
        "서울 재난 체험관 운영",
        "하천 편입토지 보상",
        "하천관리과",
        "하천 예경보시설 신설 및 교체",
        "하천 위기상황관리시스템 유지관리",
        "치수관리",
        "치수총괄",
        "청계천관리",
        "하천사용료징수포상금",
        "하천진출입로 차단시설 설치(구로구 도림천)",
        "가나다",
        "123456.0",
        "",
        "abcedfgh",
    ];
    for (i, w) in want.iter().enumerate() {
        assert_eq!(sst.items[i].text, *w, "si[{i}] 與手抽原文分岔");
        assert_eq!(sst.items[i].runs, 0, "1_NoIden 全純 t，無 rich run");
    }
    assert!(!sst.truncated);
}

/// 紅燈 1b：rich-text si——tdf171828 si[9]（手抽原文 3 個 `<r>` run，拼接後
/// 「 Fester Betrag (incl. Zinsen)」，xml:space 前置空格保留）。run 計數如實。
#[test]
fn sst_rich_runs_concatenated_and_counted() {
    let b = open("lo/sc_qa_unit_data_xlsx_tdf171828_fail_to_import_file.xlsx");
    let sst = b.shared_strings().expect("SST 存在");
    assert_eq!(sst.items.len(), 199);
    assert_eq!(sst.declared_count, Some(247), "count 宣告原樣保留");
    assert_eq!(sst.declared_unique, Some(199));
    let rich = sst.items.iter().filter(|s| s.runs > 0).count();
    assert_eq!(rich, 11, "rich si 應 11 條（實檔掃描）");
    assert_eq!(sst.items[9].runs, 3);
    assert_eq!(sst.items[9].text, " Fester Betrag (incl. Zinsen)");
    // si[2] 是單空格字串（xml:space=preserve），不是空字串也不是 None。
    assert_eq!(sst.items[2].text, " ");
}

/// 紅燈 1c：rich run 含 `&#10;` 實體——CVLKRA si[237]（手抽原文 6 run）：
/// 解碼後實換行必須如實進字串。
#[test]
fn sst_rich_runs_unescape_char_refs() {
    let b = open("poi/test-data_spreadsheet_CVLKRA-KYC_Download_File_Structure_V3.1.xlsx");
    let sst = b.shared_strings().expect("SST 存在");
    assert_eq!(sst.items.len(), 1240);
    assert_eq!(sst.declared_count, Some(2482));
    assert_eq!(sst.declared_unique, Some(1240));
    let it = &sst.items[237];
    assert_eq!(it.runs, 6);
    assert_eq!(
        it.text,
        "E - Record Entered by Intermediary\nF - Record Fetched by Intermediary\nM - Information of Updation due to Modification of KYC either Fetched or Entered by Intermediary"
    );
}

/// 紅燈 1d：count 屬性謊言照登不修——InlineStrings.xlsx SST 宣告 count=16，
/// 實際 `<si>` 只有 10 條：投影保留宣告值＋實際條目數，不互相遷就。
#[test]
fn sst_declared_counts_kept_honestly() {
    let b = open("poi/test-data_spreadsheet_InlineStrings.xlsx");
    let sst = b.shared_strings().expect("SST 存在");
    assert_eq!(sst.declared_count, Some(16));
    assert_eq!(sst.declared_unique, Some(10));
    assert_eq!(sst.items.len(), 10, "實際條目以掃描為準");
}

// ---------------------------------------------------------------- 紅燈 2：五種 cell 型別

/// 紅燈 2a：`t="s"` 索引→文字解析——1_NoIden 全 25 cell（手抽 xl/worksheets/
/// sheet1.xml 對照）。E8 `<v>12</v>`→si[12]="123456.0"：SST 文字像數字也不許
/// 強轉型別；row 1 自閉合零 cell 照實投影。
#[test]
fn cell_shared_string_resolves_via_sst() {
    let b = open("poi/test-data_spreadsheet_1_NoIden.xlsx");
    let sc = cells_of(&b, "시트_테스트(1)");
    assert_eq!(sc.cell_count, 25);
    assert_eq!(sc.rows.len(), 9, "9 個 row 標籤（row 1 無 cell）");
    assert_eq!(sc.rows[0].number, Some(1));
    assert!(sc.rows[0].cells.is_empty(), "row 1 零 cell：不虛構填充");
    let c = cell_at(&sc, "B2");
    assert_eq!((c.row, c.col), (Some(2), Some(2)));
    assert_eq!(
        c.value,
        CellValue::SharedString {
            index: Some(11),
            text: Some("가나다".into())
        }
    );
    // 數字外觀的 SST 字串仍是 SharedString（型別由 t 說，不由值外觀說）。
    assert_eq!(
        cell_at(&sc, "E8").value,
        CellValue::SharedString {
            index: Some(12),
            text: Some("123456.0".into())
        }
    );
    // 第二個空 cell（D7 `<c r="D7" s="11"/>`）同釘。
    assert_eq!(cell_at(&sc, "D7").value, CellValue::Empty);
}

/// 紅燈 2b：空 cell——1_NoIden `&lt;c r="C2" s="13"/&gt;`（自閉合、無 t 無 v）→ Empty，
/// 座標仍解析（存在性照報，不虛構內容）。
#[test]
fn cell_empty_selfclosed_projected_without_fabrication() {
    let b = open("poi/test-data_spreadsheet_1_NoIden.xlsx");
    let sc = cells_of(&b, "시트_테스트(1)");
    let c = cell_at(&sc, "C2");
    assert_eq!(c.value, CellValue::Empty);
    assert_eq!((c.row, c.col), (Some(2), Some(3)));
}

/// 紅燈 2c：`t="n"` 缺省/raw 原樣——57181 sheet2（手抽原文）：
/// `&lt;c r="CC10" s="29"&gt;&lt;v&gt;0.99829999999999997&lt;/v&gt;&lt;/c&gt;` 存原始字串
/// （f64 會把它圓整成謊言）；`&lt;v&gt;-1E-8&lt;/v&gt;` 科學記號原樣；缺省 t 的
/// `&lt;c r="BZ10" s="16"&gt;&lt;v&gt;0&lt;/v&gt;&lt;/c&gt;` 與顯式 t="n" 同路。
#[test]
fn cell_number_raw_string_never_coerced() {
    let b = open("poi/test-data_spreadsheet_57181.xlsm");
    let sc = cells_of(&b, "General");
    let cc10 = cell_at(&sc, "CC10");
    assert_eq!(cc10.col, Some(81), "CC = 3*26+3");
    assert_eq!(
        cc10.value,
        CellValue::Number {
            raw: "0.99829999999999997".into()
        }
    );
    assert_eq!(
        cell_at(&sc, "CB10").value,
        CellValue::Number {
            raw: "-1E-8".into()
        },
        "科學記號 raw 原樣"
    );
    assert_eq!(
        cell_at(&sc, "BZ10").value,
        CellValue::Number { raw: "0".into() },
        "缺省 t ＝ n"
    );
    // 顯式 t="n" 與缺省同型：tdf171828 G4 `<v>210896</v>`。
    let b2 = open("lo/sc_qa_unit_data_xlsx_tdf171828_fail_to_import_file.xlsx");
    let sc2 = cells_of(&b2, "Tilgungsverlauf");
    assert_eq!(
        cell_at(&sc2, "G4").value,
        CellValue::Number {
            raw: "210896".into()
        }
    );
    // 帶公式的數字 cell（J5 有 <f>）：f 文本絕不可誤入 raw。
    assert_eq!(
        cell_at(&sc2, "J5").value,
        CellValue::Number { raw: "3.04".into() },
        "公式文本混進 raw 就是型別謊言"
    );
}

/// 紅燈 2d：`t="b"` 布林——tdf134769 L18（手抽原文 `&lt;f ca="1"&gt;AND(...)&lt;/f&gt;&lt;v&gt;0&lt;/v&gt;`）：
/// raw "0" 原樣保留（0/1 語義解讀不寫死，留 X2/X3）。
#[test]
fn cell_boolean_keeps_raw_literal() {
    let b = open("lo/sc_qa_unit_data_xlsx_tdf134769.xlsx");
    let sc = cells_of(&b, "sheet1");
    assert_eq!(
        cell_at(&sc, "L18").value,
        CellValue::Boolean { raw: "0".into() }
    );
}

/// 紅燈 2e：`t="e"` 錯誤——tdf100034 sheet2（手抽原文
/// `&lt;f aca="false"&gt;ROUND(...#REF!...)&lt;/f&gt;&lt;v&gt;#REF!&lt;/v&gt;`）：
/// 存原始錯誤碼字串（公式文本不混入）。
#[test]
fn cell_error_keeps_error_code_string() {
    let b = open("lo/sc_qa_unit_data_xlsx_tdf100034.xlsx");
    let part = {
        let m = b.workbook_model();
        m.sheets[1].part.clone().expect("第二份 sheet 應歸屬成功")
    };
    let sc = cells_of_part(&b, &part);
    assert_eq!(
        cell_at(&sc, "G97").value,
        CellValue::Error {
            code: "#REF!".into()
        }
    );
    assert_eq!(
        cell_at(&sc, "I97").value,
        CellValue::Error {
            code: "#REF!".into()
        }
    );
}

/// 紅燈 2f：`t="inlineStr"`——InlineStrings.xlsx sheet1（手抽原文
/// `&lt;c r="C2" s="0" t="inlineStr"&gt;&lt;is&gt;&lt;t&gt;1st Inline String&lt;/t&gt;&lt;/is&gt;`）：
/// 直接 &lt;is&gt; 取文字；同頁 D2 顯式 t="n" 帶 &lt;f&gt; 照常 raw；B2 SST 引用共存。
#[test]
fn cell_inline_string_reads_is_subtree() {
    let b = open("poi/test-data_spreadsheet_InlineStrings.xlsx");
    let sc = cells_of(&b, "Sheet1");
    assert_eq!(
        cell_at(&sc, "C2").value,
        CellValue::InlineString {
            text: "1st Inline String".into(),
            runs: 0
        }
    );
    assert_eq!(
        cell_at(&sc, "C7").value,
        CellValue::InlineString {
            text: "The End".into(),
            runs: 0
        }
    );
    assert_eq!(
        cell_at(&sc, "D2").value,
        CellValue::Number { raw: "12".into() }
    );
    assert_eq!(
        cell_at(&sc, "B2").value,
        CellValue::SharedString {
            index: Some(4),
            text: Some("A".into())
        }
    );
}

/// 紅燈 2g：`t="str"`（ECMA-376 §18.18.11 型錄內的公式字串快取值，corpus 實掃
/// 5002 格——不是 Unknown）——forum-mso-en4-134670 E3（手抽原文
/// `&lt;f&gt;C3&amp;"  /  "&amp;D3&lt;/f&gt;&lt;v&gt;1  /  1&lt;/v&gt;`）raw 原樣含空格；
/// tdf171828 N4 `<v></v>` 空快取 → raw 空字串（有值態，非 Empty 非懸空）。
#[test]
fn cell_formula_string_cache_value_is_catalogued() {
    let b = open("lo/sc_qa_unit_data_xlsx_forum-mso-en4-134670.xlsx");
    let part = {
        let m = b.workbook_model();
        m.sheets[0].part.clone().expect("sheet1 歸屬成功")
    };
    let sc = cells_of_part(&b, &part);
    assert_eq!(
        cell_at(&sc, "E3").value,
        CellValue::FormulaString {
            raw: "1  /  1".into()
        }
    );
    let b2 = open("lo/sc_qa_unit_data_xlsx_tdf171828_fail_to_import_file.xlsx");
    let sc2 = cells_of(&b2, "Tilgungsverlauf");
    assert_eq!(
        cell_at(&sc2, "N4").value,
        CellValue::FormulaString { raw: "".into() }
    );
}

/// 紅燈 2h：未見於型錄的 t 值 → Unknown＋raw 保留，不猜。
/// （corpus 實掃 t 值只有 s/str/n/inlineStr/b/e 六種，Unknown 無實檔形態，
/// 以合成 XML 釘；t="d" 在 ECMA 型錄（日期序列）但 corpus 零出現——照型錄
/// 收錄、raw 原樣不轉日期。）
#[test]
fn cell_unknown_t_keeps_raw_without_guessing() {
    let xml = br#"<worksheet><sheetData>
        <row r="1"><c r="A1" t="gibberish"><v>42</v></c><c r="B1" t="d"><v>45000</v></c></row>
    </sheetData></worksheet>"#;
    let sc = quire_xlsx::read_sheet_cells(xml, None);
    assert_eq!(
        cell_at(&sc, "A1").value,
        CellValue::Unknown {
            t: "gibberish".into(),
            raw: Some("42".into())
        }
    );
    assert_eq!(
        cell_at(&sc, "B1").value,
        CellValue::DateSerial {
            raw: "45000".into()
        }
    );
}

// ---------------------------------------------------------------- 紅燈 3：跳號 row / spans / ref

/// 紅燈 3a：跳號 row＋spans 照實投影——tdf134769 sheet1（手抽原文）row 序列
/// 1..19, 22：20/21 缺席就缺席，不虛構填充；row 22 只有一個空 cell E22。
#[test]
fn row_skips_projected_as_is() {
    let b = open("lo/sc_qa_unit_data_xlsx_tdf134769.xlsx");
    let sc = cells_of(&b, "sheet1");
    let nums: Vec<Option<u32>> = sc.rows.iter().map(|r| r.number).collect();
    let mut want: Vec<Option<u32>> = (1..=19).map(Some).collect();
    want.push(Some(22));
    assert_eq!(nums, want, "row 20/21 缺席必須如實（不虛構填充）");
    assert_eq!(sc.rows[0].spans.as_deref(), Some("1:13"), "spans 原樣保留");
    assert_eq!(sc.rows[19].number, Some(22));
    assert_eq!(sc.rows[19].cells.len(), 1, "row 22 僅 E22 一格");
    assert_eq!(sc.rows[19].cells[0].reference.as_deref(), Some("E22"));
    assert_eq!(sc.rows[19].cells[0].value, CellValue::Empty);
}

/// 紅燈 3b：cell ref 解析成 (row, col)——多字母欄（AA/XFD）、非法 ref、無 ref：
/// 合成 XML 釘邊角（corpus 全 80 檔實掃 `<c>` 全數帶合法 r，非法態無實檔形態）。
#[test]
fn cell_reference_parsed_to_coordinates() {
    let xml = br#"<worksheet><sheetData>
        <row r="7">
          <c r="A7"><v>1</v></c>
          <c r="AA7"><v>2</v></c>
          <c r="XFD7"><v>3</v></c>
          <c r="A7F"><v>4</v></c>
          <c><v>5</v></c>
        </row>
    </sheetData></worksheet>"#;
    let sc = quire_xlsx::read_sheet_cells(xml, None);
    let cs: Vec<_> = sc.rows[0].cells.iter().collect();
    assert_eq!((cs[0].row, cs[0].col), (Some(7), Some(1)));
    assert_eq!((cs[1].row, cs[1].col), (Some(7), Some(27)), "AA = 26+1");
    assert_eq!(
        (cs[2].row, cs[2].col),
        (Some(7), Some(16384)),
        "XFD = 26^2+26*6+4（ECMA 最大欄）"
    );
    assert_eq!(
        (cs[3].row, cs[3].col),
        (None, None),
        "『A7F』非合法 ref：座標 None，ref 字串保留"
    );
    assert_eq!(cs[3].reference.as_deref(), Some("A7F"));
    assert_eq!(
        cs[4].reference, None,
        "無 r 屬性：reference None（不猜位置）"
    );
    assert_eq!(sc.cell_count, 5, "非法/無 ref 的 cell 仍計數（存在性照報）");
}

/// 紅燈 3c：spans 缺就報缺——1_NoIden row 1（自閉合帶 spans="2:4"）與
/// tdf171828 sheet2（LO 輸出全無 spans 屬性）並陳：有就原樣、缺就 None。
#[test]
fn spans_present_and_absent_both_honest() {
    let b = open("poi/test-data_spreadsheet_1_NoIden.xlsx");
    let sc = cells_of(&b, "시트_테스트(1)");
    assert_eq!(sc.rows[0].spans.as_deref(), Some("2:4"));
    let b2 = open("lo/sc_qa_unit_data_xlsx_tdf171828_fail_to_import_file.xlsx");
    let part2 = {
        let m = b2.workbook_model();
        m.sheets[1].part.clone().expect("Infos sheet part")
    };
    let sc2 = cells_of_part(&b2, &part2);
    assert!(
        sc2.rows.iter().all(|r| r.spans.is_none()),
        "LO 檔全無 spans → None"
    );
}

// ---------------------------------------------------------------- 紅燈 4：dimension 交叉對照

/// 紅燈 4a：宣告 A1:F5 實際掃到 A1:F462——tdf113271（實檔 dimension 謊言，
/// corpus 全掃 37 處不一致之一）：Mismatch，兩邊原值並陳不修飾。
#[test]
fn dimension_mismatch_reported_without_polish() {
    let b = open("lo/sc_qa_unit_data_xlsx_tdf113271.xlsx");
    let sc = cells_of(&b, "Accruals Report");
    assert_eq!(sc.dimension.as_deref(), Some("A1:F5"));
    assert_eq!(sc.scanned.as_ref().expect("有 cell").label(), "A1:F462");
    match &sc.dimension_status {
        DimensionStatus::Mismatch { declared, scanned } => {
            assert_eq!(declared, "A1:F5");
            assert_eq!(scanned.as_deref(), Some("A1:F462"));
        }
        other => panic!("應為 Mismatch，實際 {other:?}"),
    }
}

/// 紅燈 4b：宣告比實際窄也是錯——1_NoIden 宣告 B1:D9、cell 實掃 B2:F9
/// （E8/F8 越過宣告欄）：Mismatch（宣告行 1 無 cell 也不回填：cell 範圍為準）。
#[test]
fn dimension_mismatch_wider_cells_than_declared() {
    let b = open("poi/test-data_spreadsheet_1_NoIden.xlsx");
    let sc = cells_of(&b, "시트_테스트(1)");
    assert_eq!(sc.dimension.as_deref(), Some("B1:D9"));
    match &sc.dimension_status {
        DimensionStatus::Mismatch { declared, scanned } => {
            assert_eq!(declared, "B1:D9");
            assert_eq!(scanned.as_deref(), Some("B2:F9"));
        }
        other => panic!("應為 Mismatch，實際 {other:?}"),
    }
}

/// 紅燈 4c：一致態——InlineStrings sheet1 宣告 A1:D7 ＝ 實掃 A1:D7 → Matches。
#[test]
fn dimension_match_case() {
    let b = open("poi/test-data_spreadsheet_InlineStrings.xlsx");
    let sc = cells_of(&b, "Sheet1");
    assert_eq!(sc.dimension.as_deref(), Some("A1:D7"));
    assert_eq!(sc.scanned.as_ref().expect("有 cell").label(), "A1:D7");
    assert_eq!(sc.dimension_status, DimensionStatus::Matches);
}

/// 紅燈 4d：單格宣告「B2」正規化成 B2:B2 後比對——formats sheet4（手抽原文
/// `&lt;dimension ref="B2"/&gt;`）：Matches（正規化是 ECMA ref 語法，不是修飾）。
#[test]
fn dimension_single_ref_normalized_then_matches() {
    let b = open("lo/sc_qa_unit_data_xlsx_formats.xlsx");
    let sc = cells_of_part(&b, "xl/worksheets/sheet4.xml");
    assert_eq!(sc.dimension.as_deref(), Some("B2"));
    assert_eq!(sc.scanned.as_ref().expect("有 cell").label(), "B2:B2");
    assert_eq!(sc.dimension_status, DimensionStatus::Matches);
}

/// 紅燈 4e：無法正規化的宣告如實回報——tdf100034 sheet2 宣告「1:102」（純列式
/// ref，ECMA 合法但無欄座標可與 cell 範圍對照）：Unparseable，宣告原值保留。
#[test]
fn dimension_unparseable_declared_kept_raw() {
    let b = open("lo/sc_qa_unit_data_xlsx_tdf100034.xlsx");
    let part = {
        let m = b.workbook_model();
        m.sheets[1].part.clone().expect("sheet2 part")
    };
    let sc = cells_of_part(&b, &part);
    assert_eq!(sc.dimension.as_deref(), Some("1:102"));
    match &sc.dimension_status {
        DimensionStatus::Unparseable { declared } => assert_eq!(declared, "1:102"),
        other => panic!("純列式宣告應 Unparseable，實際 {other:?}"),
    }
}

/// 紅燈 4f：缺宣告 → NotDeclared，scanned 照常（POI 64450 實證：整個 sheet
/// 無 dimension 標籤）。
#[test]
fn dimension_absent_is_not_declared_not_fabricated() {
    let b = open("poi/test-data_spreadsheet_64450.xlsx");
    let sc = cells_of(&b, "Hoja 1");
    assert_eq!(sc.dimension, None);
    assert_eq!(sc.dimension_status, DimensionStatus::NotDeclared);
    assert_eq!(sc.scanned.as_ref().expect("有 cell").label(), "A1:H2");
}

/// 紅燈 4g：有宣告但零 cell → NoCells（不假裝 Matches）——InlineStrings
/// sheet2（手抽原文：`<dimension ref="A1"/>` 且 sheetData 整段缺失）。
#[test]
fn dimension_declared_but_no_cells_is_no_cells() {
    let b = open("poi/test-data_spreadsheet_InlineStrings.xlsx");
    let sc = cells_of(&b, "Sheet2");
    assert_eq!(sc.dimension.as_deref(), Some("A1"));
    assert_eq!(sc.scanned, None);
    assert_eq!(sc.dimension_status, DimensionStatus::NoCells);
    assert_eq!(sc.cell_count, 0);
    assert!(sc.rows.is_empty(), "無 row 標籤：不虛構任何不存在的 row");
}

// ---------------------------------------------------------------- 紅燈 5：病態容錯

/// 紅燈 5a：SST part 整個移除（InlineStrings 實檔派生）→ t="s" cell 懸空引用：
/// index 如實、text=None——不 panic、不以空字串假冒。inlineStr cell 不受累。
/// （corpus 八檔實掃：無一檔「缺 SST 卻引用 t=s」，故以派生改動釘此形態。）
#[test]
fn dangling_shared_string_when_sst_part_missing() {
    let bytes = std::fs::read(corpus_root().join("poi/test-data_spreadsheet_InlineStrings.xlsx"))
        .expect("fixture");
    let zb = quire_ooxml::ZipBundle::open(&bytes).expect("open");
    let mut changes = std::collections::BTreeMap::new();
    changes.insert(
        quire_xlsx::SHARED_STRINGS_PART.to_string(),
        quire_ooxml::Change::Remove,
    );
    let mutated = zb
        .serialize(&changes, &std::collections::BTreeMap::new())
        .expect("serialize");
    let b = XlsxBundle::open(&mutated).expect("open mutated");
    let sc = cells_of(&b, "Sheet1");
    assert_eq!(
        cell_at(&sc, "B2").value,
        CellValue::SharedString {
            index: Some(4),
            text: None
        },
        "SST 缺失 → 懸空引用（index 保留、text None，非空字串假冒）"
    );
    assert_eq!(
        cell_at(&sc, "C2").value,
        CellValue::InlineString {
            text: "1st Inline String".into(),
            runs: 0
        },
        "同頁 inlineStr 不受累"
    );
}

/// 紅燈 5b：SST 在但索引越界（派生截斷到 5 條 si）→ 界內照常解析、界外懸空。
#[test]
fn dangling_shared_string_index_out_of_range() {
    let bytes = std::fs::read(corpus_root().join("poi/test-data_spreadsheet_InlineStrings.xlsx"))
        .expect("fixture");
    let cut = truncate_sst(&bytes, 5);
    let b = XlsxBundle::open(&cut).expect("open truncated");
    let sst = b.shared_strings().expect("截斷後 SST 仍在");
    assert_eq!(sst.items.len(), 5);
    let sc = cells_of(&b, "Sheet1");
    assert_eq!(
        cell_at(&sc, "B2").value,
        CellValue::SharedString {
            index: Some(4),
            text: Some("A".into())
        },
        "界內 idx4 照常解析"
    );
    let dangle: Vec<u32> = sc
        .rows
        .iter()
        .flat_map(|r| r.cells.iter())
        .filter_map(|c| match &c.value {
            CellValue::SharedString {
                index: Some(i),
                text: None,
            } => Some(*i),
            _ => None,
        })
        .collect();
    assert_eq!(dangle.len(), 6, "idx≥5 的 6 格應全部懸空，實際 {dangle:?}");
}

/// 紅燈 5c：t="s" 但 &lt;v&gt; 缺席 → index=None＋text=None（缺就報缺，不猜索引）。
#[test]
fn shared_string_without_v_reports_nothing() {
    let xml = br#"<worksheet><sheetData>
        <row r="1"><c r="A1" t="s"/></row>
    </sheetData></worksheet>"#;
    let sst = quire_xlsx::read_shared_strings(b"<sst><si><t>x</t></si></sst>");
    let sc = quire_xlsx::read_sheet_cells(xml, Some(&sst));
    assert_eq!(
        cell_at(&sc, "A1").value,
        CellValue::SharedString {
            index: None,
            text: None
        }
    );
}

/// 紅燈 5d：XML 畸形/截斷 → 盡力回報已讀條目，不 panic（對齊 ooxml 口徑）。
#[test]
fn malformed_xml_reports_entries_read_so_far() {
    // sheet 截斷在第二格中途：第一格必須照常回報。
    let xml = br#"<worksheet><sheetData>
        <row r="1"><c r="A1"><v>12</v></c><c r="B1" t="n"><v>13"#;
    let sc = quire_xlsx::read_sheet_cells(xml, None);
    assert_eq!(sc.cell_count, 1, "截斷前已完成的 cell 必須保留");
    assert_eq!(
        cell_at(&sc, "A1").value,
        CellValue::Number { raw: "12".into() }
    );
    // SST 無閉合標籤 → truncated=true，已讀 si 保留。
    let sst = quire_xlsx::read_shared_strings(b"<sst><si><t>a</t></si><si><t>b</t></s");
    assert_eq!(sst.items.len(), 1);
    assert_eq!(sst.items[0].text, "a");
    assert!(sst.truncated, "未見 sst 閉合 → truncated 如實");
    // 完全非 XML → 空集合不炸。
    assert_eq!(
        quire_xlsx::read_shared_strings(b"not xml at all")
            .items
            .len(),
        0
    );
    let sc2 = quire_xlsx::read_sheet_cells(b"garbage", None);
    assert_eq!(sc2.cell_count, 0);
}

// ---------------------------------------------------------------- 紅燈 6：全 corpus 掃過不退化

/// 全 80 檔 × 全部可歸屬 sheet 做一次 cell 掃描：零 panic、row/cell 總數與
/// 第二條獨立實作（python 狀態機逐 part 掃描，同 sheetData/row/c 口徑）同數。
#[test]
fn corpus_wide_cell_scan_matches_independent_count() {
    let mut total_cells = 0usize;
    let mut total_rows = 0usize;
    let mut sheets = 0usize;
    for rel in manifest_files() {
        let b = open(&rel);
        let m = b.workbook_model();
        for s in &m.sheets {
            let Some(part) = &s.part else { continue };
            // CRC 毀損 entry（clusterfuzz 檔 sheet8）→ part() 誠實 None，跳過。
            let Some(_plain) = b.part(part) else { continue };
            let sc = b.sheet_cells(part).expect("有可解碼 part 就應有投影");
            total_cells += sc.cell_count;
            total_rows += sc.rows.len();
            sheets += 1;
        }
    }
    assert!(sheets >= 250, "可歸屬 sheet 數 {sheets}（python 實掃 253）");
    assert_eq!(total_cells, 337_355, "cell 總數與獨立掃描器（python）分岔");
    assert_eq!(total_rows, 25_362, "row 總數與獨立掃描器分岔");
}

/// 無損紀律零動鏡像：X1b 的掃描純讀取，掃描前後 part 明文與存回位元組不動。
#[test]
fn read_only_projection_leaves_bytes_untouched() {
    let rel = "poi/test-data_spreadsheet_1_NoIden.xlsx";
    let b = open(rel);
    let before = b.part("xl/worksheets/sheet1.xml").expect("part");
    let _sc = cells_of_part(&b, "xl/worksheets/sheet1.xml");
    let after = b.part("xl/worksheets/sheet1.xml").expect("part");
    assert_eq!(before, after, "讀取投影不得觸碰 part 明文");
    let src = std::fs::read(corpus_root().join(rel)).expect("read");
    assert_eq!(
        b.save().expect("save").as_slice(),
        src.as_slice(),
        "掃描後存回仍須 byte-identical"
    );
}
