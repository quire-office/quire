//! XLSX workbook 最小模型——**純讀取投影**（只讀不寫回；對齊 `table.rs` 唯讀來源隔離）。
//!
//! 四個獨立解析器，各自病態容錯（缺 attribute／XML 截斷→盡力已讀，不 panic、
//! 不虛構），組合在 `XlsxBundle::workbook_model`（lib.rs）：
//! - [`read_content_types`]：`[Content_Types].xml` 的 Default/Override 清單；
//! - [`read_workbook_rels`]：`xl/_rels/workbook.xml.rels` 的 rId → part 對照
//!   （自有正規化：ooxml `read_rels` 的基準目錄寫死 `word/`，xlsx 基準是 `xl/`，
//!   兩者不可共用——共用需先抽 base_dir 參數，那會動 core/ooxml，本卡不做）；
//! - [`read_workbook_sheets`]：`xl/workbook.xml` 的 sheet 宣告＋definedNames 計數；
//! - [`read_sheet_stats`]：sheet part 的 `dimension` 與 row 計數（不解析 cell，留 X1b）。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// OPC 必要 part（缺 → `XlsxError::MissingContentTypes`）。
pub const CONTENT_TYPES_PART: &str = "[Content_Types].xml";
/// workbook 主 part（ECMA-376 Part 1 §12.2.25）。
pub const WORKBOOK_PART: &str = "xl/workbook.xml";
/// workbook 的 relationships part（缺 → sheet 清單空集合，不猜 rId 歸屬）。
pub const WORKBOOK_RELS_PART: &str = "xl/_rels/workbook.xml.rels";

// ---------------------------------------------------------------- Content_Types

/// `[Content_Types].xml` 投影：副檔名預設型別＋逐 part 覆寫型別。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentTypes {
    /// `Default/@Extension`（小寫化）→ `@ContentType`。
    pub defaults: Vec<(String, String)>,
    /// `Override/@PartName`（原樣，含前置 `/`）→ `@ContentType`。
    pub overrides: Vec<(String, String)>,
}

/// 解析 `[Content_Types].xml`：只認 `Default`／`Override` 兩個直接標籤，
/// 缺關鍵 attribute 的條目跳過（畸形容錯，同 `read_rels` 口徑）。
pub fn read_content_types(xml: &[u8]) -> ContentTypes {
    let text = String::from_utf8_lossy(xml);
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut out = ContentTypes::default();
    while let Ok(ev) = reader.read_event() {
        match ev {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e) => {
                match e.local_name().as_ref() {
                    b"Default" => {
                        if let (Some(ext), Some(ct)) =
                            (attr_val(&e, b"Extension"), attr_val(&e, b"ContentType"))
                        {
                            out.defaults.push((ext.to_ascii_lowercase(), ct));
                        }
                    }
                    b"Override" => {
                        if let (Some(pn), Some(ct)) =
                            (attr_val(&e, b"PartName"), attr_val(&e, b"ContentType"))
                        {
                            out.overrides.push((pn, ct));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------- workbook rels

/// `xl/_rels/workbook.xml.rels` 的一條 rId → part 對照（External 目標＝None）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbookRel {
    pub id: String,
    /// 正規化後的 zip part 路徑（以 `base_dir` 為基準）；External → None。
    pub target_part: Option<String>,
}

/// 解析一份 `.rels`（workbook 用 base_dir="xl"）。重複 Id 後者勝（畸形容錯，
/// 對齊 `resolve_relationship` 的 rev().find 語義）。
pub fn read_workbook_rels(xml: &[u8], base_dir: &str) -> Vec<WorkbookRel> {
    let text = String::from_utf8_lossy(xml);
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut out = Vec::new();
    while let Ok(ev) = reader.read_event() {
        match ev {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e) => {
                if e.local_name().as_ref() != b"Relationship" {
                    continue;
                }
                let Some(id) = attr_val(&e, b"Id") else {
                    continue; // 無 Id＝畸形條目，跳過
                };
                let external = matches!(
                    attr_val(&e, b"TargetMode").as_deref(),
                    Some(m) if m.eq_ignore_ascii_case("external")
                );
                let target_part = if external {
                    None
                } else {
                    attr_val(&e, b"Target").map(|t| normalize_part_path(&t, base_dir))
                };
                out.push(WorkbookRel { id, target_part });
            }
            _ => {}
        }
    }
    out
}

/// OPC Target → zip part 路徑正規化（邏輯同 ooxml 私有的 `normalize_part_path`，
/// 差別是基準目錄可傳參：xlsx 的 workbook rels 基準是 `xl/`，ooxml 寫死 `word/`）。
/// 絕對目標（`/…`）相對包裝根；`..` 折回、`.`／空段丟棄；`%XX` 解碼。
fn normalize_part_path(target: &str, base_dir: &str) -> String {
    let decoded = percent_decode(target);
    let absolute = decoded.starts_with('/');
    let mut segs: Vec<&str> = if absolute {
        Vec::new()
    } else {
        base_dir
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect()
    };
    for seg in decoded.trim_start_matches('/').split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segs.pop();
            }
            s => segs.push(s),
        }
    }
    segs.join("/")
}

/// 最小 `%XX` 解碼（非法序列原樣保留；解碼後非法 UTF-8 退回原字串，不崩潰）。
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    if !b.contains(&b'%') {
        return s.to_string();
    }
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hi = (b[i + 1] as char).to_digit(16);
            let lo = (b[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

// ---------------------------------------------------------------- workbook.xml

/// sheet 可見性（`sheet/@state`；缺省＝visible）。未Recognised值退 visible
/// （ECMA-376 枚舉外的值無定義語義，退最常見的預設態，老實不虛構）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SheetState {
    Visible,
    Hidden,
    VeryHidden,
}

impl SheetState {
    fn from_attr(v: Option<&str>) -> Self {
        match v.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            Some("hidden") => Self::Hidden,
            Some("veryhidden") => Self::VeryHidden,
            _ => Self::Visible,
        }
    }
}

/// `xl/workbook.xml` 的 sheet 宣告（rels 歸屬前的原始三欄＋rId）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredSheet {
    /// `sheet/@name`（缺＝空字串；畸形檔不丟整條宣告）。
    pub name: String,
    /// `sheet/@sheetId`（缺省／非法＝None）。
    pub sheet_id: Option<u32>,
    pub state: SheetState,
    /// `sheet/@r:id`（空字串／缺＝None：macro-only 模組 sheet 實務上存在）。
    pub rid: Option<String>,
}

/// workbook.xml 解析結果：sheet 宣告序列＋definedNames 計數＋date1904 旗標。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbookDecl {
    /// `sheets` 直接子 element 序列（宣告序）。
    pub sheets: Vec<DeclaredSheet>,
    /// `definedNames` 直接子 `definedName` 個數（§18.6；型別計數留後續卡）。
    pub defined_names: usize,
    /// 是否啟用 1904 日期系統（設計書 §3.3；`workbookPr/@date1904`）。
    pub date1904: bool,
}

/// 解析 `xl/workbook.xml`：只認 workbook 根之下 `sheets/w:sheet` 與
/// `definedNames/w:definedName` 兩個直接子層（`definedName` 的巢狀嵌套不存在
/// 於規格，直接子層口徑同時擋掉未來其他 `definedName` 同名元素）。
pub fn read_workbook_sheets(xml: &[u8]) -> WorkbookDecl {
    let text = String::from_utf8_lossy(xml);
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut out = WorkbookDecl::default();
    // 標籤棧：直接子層判定用（根＝層級 1；sheets/definedNames 的子女＝層級 3）。
    // **所有** Start 都入棧（無關標籤記 "other"）——只推不彈的棧會在錯置 End
    // 時把真實層級算錯（容錯棧的代價是必須完整配對）。
    let mut stack: Vec<&'static str> = Vec::new();
    while let Ok(ev) = reader.read_event() {
        match ev {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(e) => {
                let name = e.local_name();
                let tag = local_tag(name.as_ref());
                stack.push(tag);
                if stack.len() == 2 && tag == "workbookPr" {
                    // 設計書 §3.3 與 §4.4：解析 workbookPr/@date1904
                    out.date1904 = parse_bool_attr(attr_val(&e, b"date1904").as_deref());
                } else if stack.len() == 3 {
                    // sheets/definedNames 的**直接**子女（層級判定；無名前空間驗證——
                    // 對齊本專案「只比 local_name」的一貫口徑）。
                    if tag == "sheet" && stack[1] == "sheets" {
                        out.sheets.push(parse_sheet(&e));
                    } else if tag == "definedName" && stack[1] == "definedNames" {
                        out.defined_names += 1;
                    }
                }
            }
            quick_xml::events::Event::End(e) => {
                let name = e.local_name();
                let tag = local_tag(name.as_ref());
                if stack.last() == Some(&tag) {
                    stack.pop();
                } else if let Some(pos) = stack.iter().rposition(|&t| t == tag) {
                    // 錯置閉合（中間有未閉合子女）：向棧深找相符者、連同其上彈出。
                    stack.truncate(pos);
                }
                // 棧裡根本沒有此標籤（多餘的 End）：忽略。
            }
            quick_xml::events::Event::Empty(e) => {
                let name = e.local_name();
                let tag = local_tag(name.as_ref());
                if stack.len() == 1 && tag == "workbookPr" {
                    // 設計書 §3.3 與 §4.4：自閉合 workbookPr/@date1904
                    out.date1904 = parse_bool_attr(attr_val(&e, b"date1904").as_deref());
                } else if stack.len() == 2 {
                    if tag == "sheet" && stack[1] == "sheets" {
                        out.sheets.push(parse_sheet(&e));
                    } else if tag == "definedName" && stack[1] == "definedNames" {
                        out.defined_names += 1;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// 靜態標籤名映射（只區分本模型關心的標籤；其餘一律 "other"——
/// 棧只需為判定層級存在即可）。
fn local_tag(name: &[u8]) -> &'static str {
    match name {
        b"workbook" => "workbook",
        b"workbookPr" => "workbookPr",
        b"sheets" => "sheets",
        b"sheet" => "sheet",
        b"definedNames" => "definedNames",
        b"definedName" => "definedName",
        _ => "other",
    }
}

/// 解析布林值屬性（設計書 §4.4 情境 B；支援 "true"/"1" 為 true，其餘退回 false）。
pub(crate) fn parse_bool_attr(v: Option<&str>) -> bool {
    matches!(
        v.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("true") | Some("1")
    )
}

fn parse_sheet(e: &quick_xml::events::BytesStart) -> DeclaredSheet {
    DeclaredSheet {
        name: attr_val(e, b"name").unwrap_or_default(),
        sheet_id: attr_val(e, b"sheetId")
            .as_deref()
            .and_then(|v| v.trim().parse::<u32>().ok()),
        state: SheetState::from_attr(attr_val(e, b"state").as_deref()),
        // r:id：local name 就是 "id"（prefix 不驗證——ECMA sheet 上唯一 id
        // 類 attribute 就是 r:id，對齊 ooxml attr() 只比 local_name 的口徑）。
        rid: attr_val(e, b"id").filter(|v| !v.is_empty()),
    }
}

// ---------------------------------------------------------------- sheet part

/// 一個 sheet part 的統計投影（**不解析 cell**，留 X1b）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorksheetStats {
    /// `dimension/@ref`（如 "B1:D9"；缺＝None，LO 部分檔案不寫 dimension）。
    pub dimension: Option<String>,
    /// 主命名空間 `row` 標籤出現數（Start＋Empty；`rowBreaks/w:rowBreak`
    /// local name 不同、帶前綴的 `<xdr:row>`（繪圖錨點列座標）也排除——
    /// 與 `unzip -p … | grep -c '<row[ />]'` 交叉核對過）。
    pub row_count: usize,
}

/// 解析一個 sheet part 的 dimension 與 row 計數（首見 `dimension` 即定格；
/// 規格上唯一，重複出現取首個，老實口徑）。
pub fn read_sheet_stats(xml: &[u8]) -> WorksheetStats {
    let text = String::from_utf8_lossy(xml);
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut out = WorksheetStats::default();
    while let Ok(ev) = reader.read_event() {
        match ev {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e) => {
                match e.local_name().as_ref() {
                    // local_name 會把 `xdr:row` 剝成 `row`——繪圖錨點用的是
                    // spreadsheetDrawing NS，不是工作表列。工作表列在真實
                    // 檔案（LO/POI）一律寫成預設 NS 的 `<row>`（不帶前綴），
                    // 故以「QName 無前綴」為準；帶前綴聲明主 NS 的病態檔案
                    // 會少計，但絕不虛構（誠實口徑，X1b 再做 NS 完整解析）。
                    b"row" if !e.name().as_ref().contains(&b':') => out.row_count += 1,
                    b"dimension" if out.dimension.is_none() => {
                        out.dimension = attr_val(&e, b"ref");
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------- helpers

/// attribute 取值（只比 local_name；對齊 ooxml `attr` 口徑）。
pub(crate) fn attr_val(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key {
            crate::qx::decode_and_unescape_attr(&a)
        } else {
            None
        }
    })
}

// ---------------------------------------------------------------- 組合模型

/// workbook 的一份 sheet：宣告（workbook.xml）＋歸屬（rels）＋統計（sheet part）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetInfo {
    pub name: String,
    pub sheet_id: Option<u32>,
    pub state: SheetState,
    /// 宣告的 rId（空字串／缺＝None：macro-only 模組 sheet）。
    pub rid: Option<String>,
    /// rId 經 rels 歸屬、且 zip 中確實存在的 part 路徑；懸空/缺 rels＝None。
    pub part: Option<String>,
    /// sheet part 的統計投影（part 存在才有；缺 part＝None，不虛構）。
    pub stats: Option<WorksheetStats>,
}

/// `XlsxBundle::workbook_model` 的完整投影（只讀；serde 化供 wasm 邊界）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbookModel {
    pub content_types: ContentTypes,
    /// `xl/workbook.xml.rels` 的 rId→target 對照（缺 part＝空清單）。
    pub rels: Vec<WorkbookRel>,
    /// sheet 清單（宣告序）。**缺 rels part → 空清單**：rId 無從歸屬，不猜。
    pub sheets: Vec<SheetInfo>,
    /// `definedNames` 個數（缺 workbook.xml＝0）。
    pub defined_names: usize,
    /// 是否啟用 1904 日期系統（設計書 §3.3；`workbookPr/@date1904`）。
    pub date1904: bool,
}

/// 組合三個獨立解析器成 workbook 模型（純函數：資料由呼叫端注入，
/// 不持有容器——`XlsxBundle::workbook_model` 是它唯一的組合點）。
///
/// - `workbook_xml`：`xl/workbook.xml` 明文；None＝缺 part → sheets 空、計數 0。
/// - `rels`：None＝缺 `xl/_rels/workbook.xml.rels` → **sheets 空清單**（rId
///   無從歸屬，卡面口徑）；Some（含空表）→ 照常解析宣告，rId 查不到＝part None。
/// - `has_part`／`read_part`：zip 成員查詢與明文讀取（懸空 rId 容錯要查成員）。
pub fn read_workbook(
    workbook_xml: Option<&[u8]>,
    rels: Option<&BTreeMap<String, Option<String>>>,
    has_part: &dyn Fn(&str) -> bool,
    read_part: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> WorkbookModel {
    let mut model = WorkbookModel::default();
    let Some(xml) = workbook_xml else {
        return model; // 缺 workbook.xml：全部空集合（誠實，不虛構）
    };
    let decl = read_workbook_sheets(xml);
    model.defined_names = decl.defined_names;
    model.date1904 = decl.date1904;
    let Some(rels) = rels else {
        return model; // 缺 rels：sheet 清單空（卡面口徑）
    };
    for d in decl.sheets {
        let part = d
            .rid
            .as_deref()
            .and_then(|rid| rels.get(rid).cloned().flatten())
            .filter(|p| has_part(p));
        let stats = part
            .as_deref()
            .and_then(read_part)
            .map(|b| read_sheet_stats(&b));
        model.sheets.push(SheetInfo {
            name: d.name,
            sheet_id: d.sheet_id,
            state: d.state,
            rid: d.rid,
            part,
            stats,
        });
    }
    model.rels = rels
        .iter()
        .map(|(id, target)| WorkbookRel {
            id: id.clone(),
            target_part: target.clone(),
        })
        .collect();
    model
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_types_reads_defaults_and_overrides_and_skips_malformed() {
        let xml = r#"<?xml version="1.0"?>
        <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
          <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
          <Default Extension="bin" ContentType="application/vnd.ms-office.vbaProject"/>
          <Override PartName="/xl/workbook.xml" ContentType="ct-workbook"/>
          <Override ContentType="缺PartName的畸形條目"/>
        </Types>"#.as_bytes();
        let ct = read_content_types(xml);
        assert_eq!(
            ct.defaults,
            vec![
                (
                    "rels".into(),
                    "application/vnd.openxmlformats-package.relationships+xml".into()
                ),
                ("bin".into(), "application/vnd.ms-office.vbaProject".into()),
            ]
        );
        assert_eq!(
            ct.overrides,
            vec![("/xl/workbook.xml".into(), "ct-workbook".into())]
        );
    }

    #[test]
    fn rels_normalizes_relative_absolute_and_external() {
        let xml = br#"<Relationships xmlns="pkgrels">
          <Relationship Id="rId1" Type="t" Target="worksheets/sheet1.xml"/>
          <Relationship Id="rId2" Type="t" Target="../sharedStrings.xml"/>
          <Relationship Id="rId3" Type="t" Target="/root/abs.xml"/>
          <Relationship Id="rId4" Type="t" Target="http://example.com/x" TargetMode="External"/>
          <Relationship Type="t" Target="no-id.xml"/>
        </Relationships>"#;
        let rels = read_workbook_rels(xml, "xl");
        assert_eq!(rels.len(), 4, "無 Id 條目跳過");
        assert_eq!(
            rels[0].target_part.as_deref(),
            Some("xl/worksheets/sheet1.xml")
        );
        assert_eq!(
            rels[1].target_part.as_deref(),
            Some("sharedStrings.xml"),
            ".. 折回 xl/"
        );
        assert_eq!(
            rels[2].target_part.as_deref(),
            Some("root/abs.xml"),
            "絕對目標相對包裝根"
        );
        assert_eq!(rels[3].target_part, None, "External 不是 zip 內 part");
    }

    #[test]
    fn sheet_state_parses_all_and_falls_back_visible() {
        let xml = br#"<workbook xmlns:r="r">
          <sheets>
            <sheet name="A" sheetId="1" state="visible" r:id="rId1"/>
            <sheet name="B" sheetId="2" state="hidden" r:id="rId2"/>
            <sheet name="C" sheetId="3" state="veryHidden" r:id="rId3"/>
            <sheet name="D" sheetId="4" r:id=""/>
            <sheet name="E" sheetId="x" state="garbage"/>
          </sheets>
          <definedNames>
            <definedName name="_xlnm.Print_Area" localSheetId="0">Sheet1!$A$1</definedName>
          </definedNames>
        </workbook>"#;
        let d = read_workbook_sheets(xml);
        assert_eq!(d.sheets.len(), 5);
        assert_eq!(d.sheets[0].state, SheetState::Visible);
        assert_eq!(d.sheets[1].state, SheetState::Hidden);
        assert_eq!(d.sheets[2].state, SheetState::VeryHidden);
        assert_eq!(
            d.sheets[3].rid, None,
            "空字串 r:id ＝ None（macro-only sheet）"
        );
        assert_eq!(d.sheets[4].sheet_id, None, "非法 sheetId＝None");
        assert_eq!(d.sheets[4].state, SheetState::Visible, "枚舉外值退 visible");
        assert_eq!(d.defined_names, 1);
    }

    /// 直接子層口徑：巢在别的元素（如 future/畸形嵌套）裡的 `sheet` 不計——
    /// 只認 `sheets` 直接子女（棧層級 3）。
    #[test]
    fn sheets_only_count_direct_children() {
        let xml = r#"<workbook>
          <sheets>
            <wrapper><sheet name="深藏" sheetId="9" r:id="rId9"/></wrapper>
            <sheet name="正規" sheetId="1" r:id="rId1"/>
          </sheets>
        </workbook>"#
            .as_bytes();
        let d = read_workbook_sheets(xml);
        assert_eq!(d.sheets.len(), 1);
        assert_eq!(d.sheets[0].name, "正規");
    }

    #[test]
    fn sheet_stats_counts_paired_and_selfclosed_rows_and_first_dimension() {
        let xml = br#"<worksheet>
          <dimension ref="B1:D9"/>
          <sheetData>
            <row r="1"><c r="B1"><v>1</v></c></row>
            <row r="2"/>
          </sheetData>
          <rowBreaks count="1"><brk id="5"/></rowBreaks>
        </worksheet>"#;
        let s = read_sheet_stats(xml);
        assert_eq!(s.dimension.as_deref(), Some("B1:D9"));
        assert_eq!(s.row_count, 2, "成對＋自閉合各計一次；rowBreak 不誤計");
    }

    #[test]
    fn empty_and_garbage_inputs_never_panic() {
        assert_eq!(read_content_types(b""), ContentTypes::default());
        assert!(read_workbook_rels(b"not xml at all", "xl").is_empty());
        let d = read_workbook_sheets(b"<workbook><sheets>");
        assert!(d.sheets.is_empty());
        assert_eq!(read_sheet_stats(b""), WorksheetStats::default());
    }

    #[test]
    fn percent_decodes_targets() {
        assert_eq!(
            normalize_part_path("worksheets/my%20sheet.xml", "xl"),
            "xl/worksheets/my sheet.xml"
        );
        // 非法 % 序列原樣保留，不 panic。
        assert_eq!(normalize_part_path("a%zz.xml", "xl"), "xl/a%zz.xml");
    }
}
