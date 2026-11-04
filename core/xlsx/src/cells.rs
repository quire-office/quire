//! XLSX cell 級**純讀取投影**（X1b 卡）：sharedStrings＋cell 型別＋sheetData 掃描。
//!
//! 兩個解析器，各自病態容錯（缺 attribute／XML 截斷→盡力回報已讀條目，不
//! panic、不虛構），與 `model.rs` 同款口徑：
//! - [`read_shared_strings`]：`xl/sharedStrings.xml` 的 `<si>` 序列——純 `<t>` 與
//!   rich-text `<r>` run 都要（本卡只取拼接後純文字＋run 計數，不建樣式——X2）；
//! - [`read_sheet_cells`]：sheet part 的 `sheetData/row/c` 掃描——cell ref 解析、
//!   `dimension` 宣告 vs 實際掃描範圍交叉對照、空 cell／跳號 row／`spans` 照實投影。
//!
//! 型別語義以 ECMA-376 Part 1 §18.18.11（`t` 屬性別樹）為準。本卡型錄：
//! `s`／`inlineStr`／`n`（缺省）／`b`／`e`／`str`（公式字串快取值——corpus 實掃
//! 5002 格）／`d`（ISO 日期序列——ECMA 型錄成員但 corpus 零出現，raw 原樣不轉
//! 日期）。**未見于型錄的 t 值 → [`CellValue::Unknown`] 保留 raw，不猜。**
//!
//! 數字/raw 紀律：數值一律以**原始字串**保存，不強轉 f64——corpus 57181 的
//! `0.99829999999999997` 轉 f64 再印回就是精度謊言。
//!
//! 寫入面仍封鎖（卡面鐵律）：本模組不提供 set_cell／改 part；唯一組合點是
//! `XlsxBundle::shared_strings`／`XlsxBundle::sheet_cells`（只讀）。

use serde::{Deserialize, Serialize};

use crate::model::attr_val;
use crate::styles::{EffectiveStyle, StyleSheetModel};

/// sharedStrings part 路徑（ECMA-376 預設命名慣例，corpus 80 檔全數如此）。
/// workbook rels 才是權威歸屬；本常數是 `XlsxBundle::shared_strings` 的查表起點。
pub const SHARED_STRINGS_PART: &str = "xl/sharedStrings.xml";

// ---------------------------------------------------------------- sharedStrings

/// 一條 `<si>`：拼接後的純文字＋run 計數（rich run 樣式不建，X2 再說）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedStringItem {
    /// 純 `<t>` 或 rich `<r><t>` 拼接後的文字（實體已解碼；前後的 xml:space
    /// 保留空白如實入字串——解析器從不 trim）。
    pub text: String,
    /// `<r>` run 個數：0＝純文字 si。（phonetic `<rPh>` 子樹整棵忽略：ECMA
    /// §18.4.9 發音符號提示非內文，其 `<t>` 不進拼接、不計 run。）
    pub runs: u32,
}

/// `xl/sharedStrings.xml` 投影：`<si>` 序列＋sst 宣告計數原值＋截斷旗標。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedStrings {
    /// `<si>` 序列（宣告序；位置索引即 `t="s"` cell 的查表鍵）。
    pub items: Vec<SharedStringItem>,
    /// `sst/@count` 原值（引用總數宣告）——與實際條目數不一致時**不遷就**：
    /// InlineStrings.xlsx 實證宣告 16、實條 10，兩者並陳。
    pub declared_count: Option<u64>,
    /// `sst/@uniqueCount` 原值（同上口徑）。
    pub declared_unique: Option<u64>,
    /// true＝未見根 element 閉合（XML 截斷／畸形）：已讀條目照常回報，
    /// 但如實標記來源不完整。
    pub truncated: bool,
}

/// 解析 `xl/sharedStrings.xml`（只認根之下直接子 `<si>`、si 直接子 `<t>`／`<r>`、
/// r 直接子 `<t>` 三個層級；其餘子樹——rPh、rPr、future 標籤——整棵隔開）。
/// XML 畸形／截斷 → 盡力回報已讀條目＋`truncated`，不 panic；截斷中途的半條
/// si 不完整，**不入列**（不虛稱完整）。
pub fn read_shared_strings(xml: &[u8]) -> SharedStrings {
    let text = String::from_utf8_lossy(xml);
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut out = SharedStrings::default();
    let mut stack: Vec<&'static str> = Vec::new();
    let mut root_tag: Option<&'static str> = None;
    let mut closed = false;
    // 累積中的 si（None＝不在 si 內）。
    let mut cur: Option<SharedStringItem> = None;
    // 文字捕捉深度：Some(d)＝正處於深度 d 的 <t> 內。
    let mut t_depth: Option<usize> = None;

    while let Ok(ev) = reader.read_event() {
        match ev {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(e) => {
                let tag = local_tag(e.local_name().as_ref());
                stack.push(tag);
                let d = stack.len();
                if root_tag.is_none() {
                    root_tag = Some(tag);
                    if tag == "sst" {
                        out.declared_count = attr_u64(&e, b"count");
                        out.declared_unique = attr_u64(&e, b"uniqueCount");
                    }
                }
                match tag {
                    "si" if d == 2 && cur.is_none() => {
                        cur = Some(SharedStringItem {
                            text: String::new(),
                            runs: 0,
                        });
                    }
                    // si 直接子 r：run 計數（`<rPr>` 是 r 的子女，不在此層）。
                    "r" if d == 3 && cur.is_some() && stack[1] == "si" => {
                        cur.as_mut().expect("cur").runs += 1;
                    }
                    // t 捕捉：si 直接子 t（d=3）或 r 直接子 t（d=4，且 d3 為 r）。
                    // rPh 直接子 t 落在 d=3 但 stack[1]…判定見 body 內註解。
                    "t" if cur.is_some()
                        && ((d == 3 && stack[1] == "si")
                            || (d == 4 && stack[2] == "r" && stack[1] == "si")) =>
                    {
                        // d==3 且父是 si → 直下 t；但 rPh 的 t 也是 d=3——
                        // 其父 rPh 記於 stack[2]？不：rPh 是 si 直接子（d=3），
                        // rPh 的子女 t 落在 d=4 且 stack[2]=="rPh"，被上半段
                        // `stack[1]=="si"`（此處 stack[1] 是 si、stack[2] 是
                        // rPh 時 d=4 走下半段要求 stack[2]=="r"）擋掉。
                        t_depth = Some(d);
                    }
                    _ => {}
                }
            }
            quick_xml::events::Event::Empty(e) => {
                let tag = local_tag(e.local_name().as_ref());
                match tag {
                    // `<sst/>`：空 SST，正常閉合態（非截斷）。
                    "sst" if stack.is_empty() && root_tag.is_none() => {
                        root_tag = Some("sst");
                        out.declared_count = attr_u64(&e, b"count");
                        out.declared_unique = attr_u64(&e, b"uniqueCount");
                        closed = true;
                    }
                    // `<si/>`：零文字零 run 的合法空條（corpus 未見，ECMA 合法）。
                    "si" if stack.len() == 1 && cur.is_none() => {
                        out.items.push(SharedStringItem {
                            text: String::new(),
                            runs: 0,
                        });
                    }
                    // `<r/>`：零文字 run（畸形但存在——存在性照計）。
                    "r" if stack.len() == 2 && cur.is_some() && stack[1] == "si" => {
                        cur.as_mut().expect("cur").runs += 1;
                    }
                    // `<t/>`：零文字（`<si><t/></si>` 實檔形態）：不追加。
                    _ => {}
                }
            }
            quick_xml::events::Event::Text(e) => {
                if t_depth == Some(stack.len()) {
                    if let (Some(c), Ok(v)) = (cur.as_mut(), crate::qx::unescape_text(&e)) {
                        c.text.push_str(&v);
                    }
                }
            }
            quick_xml::events::Event::GeneralRef(e) => {
                if t_depth == Some(stack.len()) {
                    if let (Some(c), Ok(v)) = (cur.as_mut(), crate::qx::resolve_general_ref(&e)) {
                        c.text.push_str(&v);
                    }
                }
            }
            quick_xml::events::Event::End(e) => {
                let tag = local_tag(e.local_name().as_ref());
                if tag == "t" && t_depth == Some(stack.len()) {
                    t_depth = None;
                }
                if tag == "si" && cur.is_some() && stack.len() == 2 {
                    out.items.push(cur.take().expect("cur"));
                }
                if stack.len() == 1 && root_tag == Some(tag) {
                    closed = true;
                }
                // 彈棧（同 read_workbook_sheets：錯置閉合向棧深找相符者連同其上彈出）。
                if stack.last() == Some(&tag) {
                    stack.pop();
                } else if let Some(pos) = stack.iter().rposition(|&t| t == tag) {
                    stack.truncate(pos);
                }
                // 錯置彈棧把 si 彈掉：累積中的半條 si 不完整 → 丟棄（不虛稱完整）。
                if cur.is_some() && stack.len() < 2 {
                    cur = None;
                    t_depth = None;
                }
            }
            _ => {}
        }
    }
    out.truncated = !closed && !xml.is_empty();
    out
}

// ---------------------------------------------------------------- sheetData 掃描

/// cell 值的型別投影（ECMA-376 §18.18.11 別樹；數值一律 raw 字串，不強轉）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CellValue {
    /// `t="s"`：`<v>` 是 SST 索引。`text`＝查表結果；**查不到（SST part 缺失／
    /// 索引越界／v 缺席）＝None**——懸空引用誠實標記：不 panic、不空字串假冒。
    SharedString {
        index: Option<u32>,
        text: Option<String>,
    },
    /// `t="inlineStr"`：直接 `<is>` 子樹拼接文字（rich run 只計數不建樣式）。
    InlineString { text: String, runs: u32 },
    /// `t="n"` 或缺省：原始數字字串（科學記號／超長精度原樣，不做 f64 圓整）。
    Number { raw: String },
    /// `t="b"`：原始布林字串（"0"/"1" 的語義解讀留 X2/X3，不寫死真值表）。
    Boolean { raw: String },
    /// `t="e"`：原始錯誤碼字串（"#REF!" 等）。
    Error { code: String },
    /// `t="str"`：公式快取的字符串結果（ECMA 型錄正式成員；`<v></v>` 空快取
    /// ＝ raw 空字串——「有值態」不是 Empty）。
    FormulaString { raw: String },
    /// `t="d"`：ISO 日期序列字串（ECMA 型錄成員、corpus 零出現；不轉日期）。
    DateSerial { raw: String },
    /// 未見于型錄的 t 值：型錄外宣告＋raw（`<v>` 缺席＝None）原樣保留，不猜。
    Unknown { t: String, raw: Option<String> },
    /// 無值可投影：空 cell（`<c r="B2"/>`；或帶 t 但 `<v>` 缺席的「只有格式的
    /// 空白格」ECMA §18.3.1.4——`t="s"` 除外，索引存在即有意義）。
    /// 存在性照報、不虛構內容。
    Empty,
}

/// 一個 cell 的投影（位置＋宣告型別＋值＋樣式索引＋有效樣式；只讀）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    /// `c/@r` 原樣（A1 式）；缺＝None（不猜位置）。
    pub reference: Option<String>,
    /// ref 解析出的列座標（非法 ref＝None）。
    pub row: Option<u32>,
    /// ref 解析出的欄座標（A=1；非法 ref＝None）。
    pub col: Option<u32>,
    pub value: CellValue,
    /// 儲存格原始樣式索引（設計書 §1.4 與 §2.3；`c/@s`，缺省為 None）。
    pub s: Option<u32>,
    /// 儲存格有效樣式投影（設計書 §2.3；若解析器有傳入 styles 則計算出 EffectiveStyle，否則為 None）。
    pub style: Option<EffectiveStyle>,
}

/// 一個 `<row>` 的投影（跳號照實：row 20/21 缺席就缺席，不虛構填充）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowCells {
    /// `row/@r`（缺省／非法＝None；孤兒 cell 的承載 row 也是 None——虛構
    /// row 編號是謊言）。
    pub number: Option<u32>,
    /// `row/@spans` 原樣（缺屬性＝None——LO 輸出全無 spans，corpus 實證）。
    pub spans: Option<String>,
    /// 本 row 直接子 cell 序列（宣告序）。
    pub cells: Vec<Cell>,
}

/// 實際掃描出的 cell 佔用框（僅由**可解析 ref 的 cell** 構成；零格＝None）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannedExtent {
    pub min_row: u32,
    pub min_col: u32,
    pub max_row: u32,
    pub max_col: u32,
}

impl ScannedExtent {
    /// A1 式標籤（如 "A1:F462"）。
    pub fn label(&self) -> String {
        format!(
            "{}{}:{}{}",
            col_letters(self.min_col),
            self.min_row,
            col_letters(self.max_col),
            self.max_row
        )
    }
}

/// `dimension/@ref` 宣告 vs 實際掃描的交叉對照結論（卡面：不一致如實回報，
/// 不修飾、不回填、不改寫宣告原值）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DimensionStatus {
    /// 無 `dimension` 標籤（LO／POI 部分檔不寫——不虛構一個範圍）。
    #[default]
    NotDeclared,
    /// 宣告（正規化後）＝實掃範圍。
    Matches,
    /// 不一致：兩邊原值並陳（零 cell 時 `scanned`＝None）。
    Mismatch {
        declared: String,
        scanned: Option<String>,
    },
    /// 宣告無法正規化成行列框（純列式 "1:102"、純欄式 "A:A"、亂碼等）：無從
    /// 與 cell 掃描對照，宣告原值保留（不是錯誤，是「無從對照」）。
    Unparseable { declared: String },
    /// 有宣告但本 part 零 cell：無從對照（不假裝 Matches）。
    NoCells,
}

/// 一個 sheet part 的 cell 級投影。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SheetCells {
    /// `dimension/@ref` 原值（首見即定格，同 `read_sheet_stats` 口徑；缺＝None）。
    pub dimension: Option<String>,
    /// 宣告 vs 實掃的交叉對照結論。
    pub dimension_status: DimensionStatus,
    /// 實際掃描範圍（零 cell＝None）。
    pub scanned: Option<ScannedExtent>,
    /// `sheetData` 直接子 `<row>` 投影序列（宣告序；跳號照實）。
    pub rows: Vec<RowCells>,
    /// 已投影 cell 總數（含非法／缺 ref 者——存在性照報）。
    pub cell_count: usize,
    /// true＝未見根 element 閉合（XML 截斷／畸形）：已讀條目照常回報。
    pub truncated: bool,
}

/// 掃描一個 sheet part 的 `sheetData/row/c`（唯讀）。
pub fn read_sheet_cells(xml: &[u8], sst: Option<&SharedStrings>) -> SheetCells {
    read_sheet_cells_with_styles(xml, sst, None)
}

/// 掃描一個 sheet part 的 `sheetData/row/c` 並帶入樣式表投影（設計書 §2.3 樣式投影）。
///
/// - `sst`：共享字串表；None＝SST part 缺失 → 所有 `t="s"` cell 投影為懸空引用。
/// - `styles`：樣式表投影；Some 則為每個 cell 計算 EffectiveStyle，None 則 cell.style 為 None。
pub fn read_sheet_cells_with_styles(
    xml: &[u8],
    sst: Option<&SharedStrings>,
    styles: Option<&StyleSheetModel>,
) -> SheetCells {
    let text = String::from_utf8_lossy(xml);
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut out = SheetCells::default();
    let mut stack: Vec<&'static str> = Vec::new();
    let mut root_tag: Option<&'static str> = None;
    let mut closed = false;
    // 進行中狀態（以「該標籤 Start 入棧後的深度」為鍵，End/彈棧時核銷）：
    let mut sd_active = false; // 在 sheetData 內
    let mut row: Option<(usize, RowCells)> = None;
    let mut cell: Option<(usize, CellBuilder)> = None;
    let mut v_depth: Option<usize> = None;
    let mut is_depth: Option<usize> = None;
    let mut t_depth: Option<usize> = None; // is 子樹的 t（inlineStr 文字）
    let mut root_depth_seen = false;

    while let Ok(ev) = reader.read_event() {
        match ev {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(e) => {
                let tag = local_tag(e.local_name().as_ref());
                stack.push(tag);
                let d = stack.len();
                if !root_depth_seen {
                    root_depth_seen = true;
                    root_tag = Some(tag);
                }
                match tag {
                    "dimension" if d == 2 && out.dimension.is_none() => {
                        out.dimension = attr_val(&e, b"ref");
                    }
                    "sheetData" if d == 2 => sd_active = true,
                    "row" if sd_active && d == 3 && row.is_none() => {
                        row = Some((
                            d,
                            RowCells {
                                number: attr_val(&e, b"r")
                                    .as_deref()
                                    .and_then(|v| v.trim().parse::<u32>().ok()),
                                spans: attr_val(&e, b"spans"),
                                cells: Vec::new(),
                            },
                        ));
                    }
                    "c" if row.is_some() && d == 4 => {
                        let s = attr_val(&e, b"s")
                            .as_deref()
                            .and_then(|v| v.trim().parse::<u32>().ok());
                        cell = Some((
                            d,
                            CellBuilder {
                                reference: attr_val(&e, b"r"),
                                s,
                                t: attr_val(&e, b"t"),
                                v: None,
                                v_seen: false,
                                is_runs: 0,
                                is_text: String::new(),
                            },
                        ));
                    }
                    "v" if cell.is_some() && d == 5 && stack[3] == "c" => {
                        if let Some((_, c)) = cell.as_mut() {
                            c.v_seen = true;
                            c.v = Some(String::new());
                        }
                        v_depth = Some(d);
                    }
                    "is" if cell.is_some() && d == 5 && stack[3] == "c" => {
                        is_depth = Some(d);
                    }
                    // <is> 直接子 r：inlineStr 的 rich run 計數。
                    "r" if is_depth == Some(d - 1) && d == 6 && stack[4] == "is" => {
                        if let Some((_, c)) = cell.as_mut() {
                            c.is_runs += 1;
                        }
                    }
                    // <is> 直接子 t 或 <is><r> 直接子 t：文字捕捉。
                    "t" if is_depth.is_some()
                        && (d == is_depth.map(|x| x + 1).unwrap_or(0)
                            || (d == is_depth.map(|x| x + 2).unwrap_or(0)
                                && stack.get(d - 2) == Some(&"r")))
                        && stack.get(d - 1).is_some() =>
                    {
                        t_depth = Some(d);
                    }
                    _ => {}
                }
            }
            quick_xml::events::Event::Empty(e) => {
                let tag = local_tag(e.local_name().as_ref());
                match tag {
                    "dimension" if stack.len() == 1 && out.dimension.is_none() => {
                        out.dimension = attr_val(&e, b"ref");
                    }
                    "row" if sd_active && stack.len() == 2 && row.is_none() => {
                        let rc = RowCells {
                            number: attr_val(&e, b"r")
                                .as_deref()
                                .and_then(|v| v.trim().parse::<u32>().ok()),
                            spans: attr_val(&e, b"spans"),
                            cells: Vec::new(),
                        };
                        out.cell_count += rc.cells.len();
                        out.rows.push(rc);
                    }
                    "c" if row.is_some() && stack.len() == 3 => {
                        let s = attr_val(&e, b"s")
                            .as_deref()
                            .and_then(|v| v.trim().parse::<u32>().ok());
                        let b = CellBuilder {
                            reference: attr_val(&e, b"r"),
                            s,
                            t: attr_val(&e, b"t"),
                            v: None,
                            v_seen: false,
                            is_runs: 0,
                            is_text: String::new(),
                        };
                        finish_cell(&mut out, row.as_mut().map(|(_, r)| r), b, sst, styles);
                    }
                    "v" if cell.is_some() && stack.len() == 4 && stack[3] == "c" => {
                        // `<v/>`：值標籤存在但零文字 → Some("")（與缺 v 分家）。
                        if let Some((_, c)) = cell.as_mut() {
                            c.v_seen = true;
                            c.v = Some(String::new());
                        }
                    }
                    "t" if is_depth.is_some() && stack.len() >= 5 => {
                        // `<t/>`／`<is><t/>`：零文字，不追加。
                    }
                    _ => {}
                }
            }
            quick_xml::events::Event::Text(e) => {
                if v_depth == Some(stack.len()) {
                    if let (Some((_, c)), Ok(v)) = (cell.as_mut(), crate::qx::unescape_text(&e)) {
                        if let Some(buf) = c.v.as_mut() {
                            buf.push_str(&v);
                        }
                    }
                } else if t_depth == Some(stack.len()) {
                    if let (Some((_, c)), Ok(v)) = (cell.as_mut(), crate::qx::unescape_text(&e)) {
                        c.is_text.push_str(&v);
                    }
                }
            }
            quick_xml::events::Event::GeneralRef(e) => {
                if v_depth == Some(stack.len()) {
                    if let (Some((_, c)), Ok(v)) =
                        (cell.as_mut(), crate::qx::resolve_general_ref(&e))
                    {
                        if let Some(buf) = c.v.as_mut() {
                            buf.push_str(&v);
                        }
                    }
                } else if t_depth == Some(stack.len()) {
                    if let (Some((_, c)), Ok(v)) =
                        (cell.as_mut(), crate::qx::resolve_general_ref(&e))
                    {
                        c.is_text.push_str(&v);
                    }
                }
            }
            quick_xml::events::Event::End(e) => {
                let tag = local_tag(e.local_name().as_ref());
                if tag == "t" && t_depth == Some(stack.len()) {
                    t_depth = None;
                }
                if tag == "v" && v_depth == Some(stack.len()) {
                    v_depth = None;
                }
                if tag == "is" && is_depth == Some(stack.len()) {
                    is_depth = None;
                }
                if tag == "c" && cell.as_ref().map(|(d, _)| *d) == Some(stack.len()) {
                    let (_, b) = cell.take().expect("cell");
                    finish_cell(&mut out, row.as_mut().map(|(_, r)| r), b, sst, styles);
                }
                if tag == "row" && row.as_ref().map(|(d, _)| *d) == Some(stack.len()) {
                    let (_, rc) = row.take().expect("row");
                    out.cell_count += rc.cells.len();
                    out.rows.push(rc);
                }
                if tag == "sheetData" && stack.len() == 2 && sd_active {
                    sd_active = false;
                }
                if stack.len() == 1 && root_tag == Some(tag) {
                    closed = true;
                }
                // 彈棧（同 read_workbook_sheets 的容錯彈棧）。
                if stack.last() == Some(&tag) {
                    stack.pop();
                } else if let Some(pos) = stack.iter().rposition(|&t| t == tag) {
                    stack.truncate(pos);
                }
                // 錯置彈棧把進行中的 cell／row 彈掉：cell 不完整→丟棄（不虛稱
                // 完整）；row 被彈→已完成 cell 照常入列（盡力回報口徑）。
                if cell.is_some() && stack.len() < 4 {
                    cell = None;
                    v_depth = None;
                    is_depth = None;
                    t_depth = None;
                }
                if row.is_some() && stack.len() < 3 {
                    let (_, rc) = row.take().expect("row");
                    out.cell_count += rc.cells.len();
                    out.rows.push(rc);
                }
            }
            _ => {}
        }
    }
    // Eof 收尾：截斷中途——cell 半成品丟棄；row 半成品含已完成 cell → 誠實入列。
    if row.is_some() {
        let (_, rc) = row.take().expect("row");
        out.cell_count += rc.cells.len();
        out.rows.push(rc);
    }
    out.truncated = !closed && !xml.is_empty();
    // 交叉對照（宣告原值保留在 out.dimension，這裡只出結論）。
    out.scanned = scanned_extent(&out);
    out.dimension_status = classify_dimension(out.dimension.as_deref(), out.scanned);
    out
}

/// 累積中的 cell（解析器內部狀態）。
struct CellBuilder {
    reference: Option<String>,
    s: Option<u32>,
    t: Option<String>,
    v: Option<String>,
    v_seen: bool,
    is_runs: u32,
    is_text: String,
}

/// cell 收尾：按 t 型錄投影值＋解析座標，塞入進行中 row（無 row 則以
/// number=None 的承載 row 虛構位置＝謊言，只報存在性）。
fn finish_cell(
    out: &mut SheetCells,
    row: Option<&mut RowCells>,
    b: CellBuilder,
    sst: Option<&SharedStrings>,
    styles: Option<&StyleSheetModel>,
) {
    let (row_no, col_no) = match b.reference.as_deref() {
        Some(r) => parse_cell_ref(r),
        None => (None, None),
    };
    let value = match b.t.as_deref() {
        None | Some("n") => {
            if b.v_seen {
                CellValue::Number {
                    raw: b.v.unwrap_or_default(),
                }
            } else {
                CellValue::Empty
            }
        }
        Some("s") => {
            let index = b.v.as_deref().and_then(|v| v.trim().parse::<u32>().ok());
            let text = index
                .and_then(|i| sst.and_then(|t| t.items.get(i as usize).map(|it| it.text.clone())));
            // text＝None＝懸空引用（SST 缺失／索引越界）——誠實標記，非空字串。
            CellValue::SharedString { index, text }
        }
        Some("inlineStr") => CellValue::InlineString {
            text: b.is_text,
            runs: b.is_runs,
        },
        Some("b") => {
            if b.v_seen {
                CellValue::Boolean {
                    raw: b.v.unwrap_or_default(),
                }
            } else {
                CellValue::Empty
            }
        }
        Some("e") => {
            if b.v_seen {
                CellValue::Error {
                    code: b.v.unwrap_or_default(),
                }
            } else {
                CellValue::Empty
            }
        }
        Some("str") => {
            if b.v_seen {
                CellValue::FormulaString {
                    raw: b.v.unwrap_or_default(),
                }
            } else {
                CellValue::Empty
            }
        }
        Some("d") => {
            if b.v_seen {
                CellValue::DateSerial {
                    raw: b.v.unwrap_or_default(),
                }
            } else {
                CellValue::Empty
            }
        }
        Some(other) => CellValue::Unknown {
            t: other.to_string(),
            raw: if b.v_seen {
                Some(b.v.unwrap_or_default())
            } else {
                None
            },
        },
    };
    let style = styles.map(|st| st.resolve_effective_style(b.s));
    let c = Cell {
        reference: b.reference,
        row: row_no,
        col: col_no,
        value,
        s: b.s,
        style,
    };
    match row {
        Some(r) => r.cells.push(c),
        None => {
            out.cell_count += 1;
            out.rows.push(RowCells {
                number: None,
                spans: None,
                cells: vec![c],
            });
        }
    }
}

// ---------------------------------------------------------------- ref／dimension 工具

/// A1 式 ref → (row, col)（A=1；`$` 絕對引用記號純語法，先剝除）。
/// 非法 ref（空、無字母欄、無數字列、混雜如 "A7F"）→ (None, None)：座標不猜，
/// ref 原字串由 caller 保留。
pub(crate) fn parse_cell_ref(reference: &str) -> (Option<u32>, Option<u32>) {
    let s: String = reference.trim().chars().filter(|&c| c != '$').collect();
    if s.is_empty() {
        return (None, None);
    }
    let split = s.find(|c: char| c.is_ascii_digit());
    let (col_part, row_part) = match split {
        Some(i) => (&s[..i], &s[i..]),
        None => return (None, None), // 純字母：無列座標
    };
    if col_part.is_empty()
        || !col_part.bytes().all(|b| b.is_ascii_alphabetic())
        || row_part.is_empty()
        || !row_part.bytes().all(|b| b.is_ascii_digit())
    {
        return (None, None);
    }
    let row = row_part.parse::<u32>().ok();
    let mut col: u32 = 0;
    for b in col_part.bytes().map(|b| (b | 0x20) - b'a' + 1) {
        col = match col
            .checked_mul(26)
            .and_then(|v| v.checked_add(u32::from(b)))
        {
            Some(v) => v,
            None => return (row, None), // 欄字母溢出 u32：欄不猜、列照報
        };
    }
    (row, Some(col))
}

/// 1-based 欄號 → 字母（1→A，27→AA，16384→XFD）。
pub(crate) fn col_letters(col: u32) -> String {
    let mut n = col;
    let mut s = String::new();
    while n > 0 {
        n -= 1;
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    s
}

/// dimension ref（"A1"／"A1:B2"／"$A$1:$XFD$1048576"）正規化成行列框。
/// 單格宣告 "B2" ≡ "B2:B2"（ECMA ref 語法，不是修飾）；純列式 "1:102"、
/// 純欄式 "A:A"、三段以上亂碼 → None（Unparseable 態）。
fn parse_dimension(refstr: &str) -> Option<ScannedExtent> {
    let mut bounds = refstr.split(':');
    let a = bounds.next()?.trim();
    let b = bounds.next();
    if bounds.next().is_some() {
        return None;
    }
    let (a_row, a_col) = parse_ref_bound(a)?;
    let (b_row, b_col) = match b {
        Some(b) => parse_ref_bound(b.trim())?,
        None => (a_row, a_col),
    };
    Some(ScannedExtent {
        min_row: a_row.min(b_row),
        min_col: a_col.min(b_col),
        max_row: a_row.max(b_row),
        max_col: a_col.max(b_col),
    })
}

/// dimension 單邊界：欄＋列俱全才合格（"1" 與 "A" 皆不合格）。
fn parse_ref_bound(s: &str) -> Option<(u32, u32)> {
    let (row, col) = parse_cell_ref(s);
    match (row, col) {
        (Some(r), Some(c)) => Some((r, c)),
        _ => None,
    }
}

/// 由投影重算實掃框（只取可解析座標的 cell）。
fn scanned_extent(sc: &SheetCells) -> Option<ScannedExtent> {
    let mut e: Option<ScannedExtent> = None;
    for r in &sc.rows {
        for c in &r.cells {
            let (Some(row), Some(col)) = (c.row, c.col) else {
                continue;
            };
            e = Some(match e {
                None => ScannedExtent {
                    min_row: row,
                    min_col: col,
                    max_row: row,
                    max_col: col,
                },
                Some(x) => ScannedExtent {
                    min_row: x.min_row.min(row),
                    min_col: x.min_col.min(col),
                    max_row: x.max_row.max(row),
                    max_col: x.max_col.max(col),
                },
            });
        }
    }
    e
}

/// 宣告 vs 實掃分類（卡面：不一致如實回報，不修飾）。
fn classify_dimension(declared: Option<&str>, scanned: Option<ScannedExtent>) -> DimensionStatus {
    let Some(declared_raw) = declared else {
        return DimensionStatus::NotDeclared;
    };
    let Some(parsed) = parse_dimension(declared_raw) else {
        return DimensionStatus::Unparseable {
            declared: declared_raw.to_string(),
        };
    };
    match scanned {
        None => DimensionStatus::NoCells,
        Some(a) if a == parsed => DimensionStatus::Matches,
        Some(a) => DimensionStatus::Mismatch {
            declared: declared_raw.to_string(),
            scanned: Some(a.label()),
        },
    }
}

// ---------------------------------------------------------------- 標籤／helpers

/// 靜態標籤映射（本模組關心的；其餘 "other"）。
fn local_tag(name: &[u8]) -> &'static str {
    match name {
        b"dimension" => "dimension",
        b"sheetData" => "sheetData",
        b"row" => "row",
        b"c" => "c",
        b"v" => "v",
        b"t" => "t",
        b"is" => "is",
        b"r" => "r",
        b"sst" => "sst",
        b"si" => "si",
        _ => "other",
    }
}

fn attr_u64(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<u64> {
    attr_val(e, key)
        .as_deref()
        .and_then(|v| v.trim().parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_parsing_roundtrip_letters() {
        assert_eq!(col_letters(1), "A");
        assert_eq!(col_letters(26), "Z");
        assert_eq!(col_letters(27), "AA");
        assert_eq!(col_letters(16384), "XFD");
        assert_eq!(parse_cell_ref("A1"), (Some(1), Some(1)));
        assert_eq!(parse_cell_ref("$AB$12"), (Some(12), Some(28)));
        assert_eq!(parse_cell_ref("XFD1048576"), (Some(1048576), Some(16384)));
        assert_eq!(parse_cell_ref("A7F"), (None, None));
        assert_eq!(parse_cell_ref(""), (None, None));
        assert_eq!(parse_cell_ref("12"), (None, None));
        assert_eq!(parse_cell_ref("A"), (None, None));
    }

    #[test]
    fn dimension_parse_forms() {
        assert_eq!(
            parse_dimension("A1").map(|e| e.label()),
            Some("A1:A1".into())
        );
        assert_eq!(
            parse_dimension("B2").map(|e| e.label()),
            Some("B2:B2".into())
        );
        assert_eq!(
            parse_dimension("$A$1:$F$462").map(|e| e.label()),
            Some("A1:F462".into())
        );
        assert_eq!(parse_dimension("1:102"), None);
        assert_eq!(parse_dimension("A:A"), None);
        assert_eq!(parse_dimension("A1:B2:C3"), None);
    }

    #[test]
    fn never_panics_on_garbage() {
        let _ = read_shared_strings(b"\x00\x01garbage");
        let sc = read_sheet_cells(b"\xff<worksheet><sheetData><row", None);
        assert!(sc.truncated);
        let _ = read_sheet_cells(b"", None);
    }

    #[test]
    fn inline_rich_is_counts_runs() {
        let xml = br#"<worksheet><sheetData><row r="1">
            <c r="A1" t="inlineStr"><is><r><rPr><b/></rPr><t>ab</t></r><r><t>cd</t></r></is></c>
        </row></sheetData></worksheet>"#;
        let sc = read_sheet_cells(xml, None);
        match &sc.rows[0].cells[0].value {
            CellValue::InlineString { text, runs } => {
                assert_eq!(text, "abcd");
                assert_eq!(*runs, 2);
            }
            other => panic!("應為 InlineString，實際 {other:?}"),
        }
    }

    /// 層級隔離：`<xdr:row>`（繪圖錨點列座標，深在 drawing 子樹）與
    /// `rowBreaks/brk` 一律不進投影——深度閘門（row 只認根>sheetData>row）
    /// 天然隔開；tdf134769／mv-calculator 兩實檔（xdr:row 實存於 worksheet part）
    /// 的全 corpus 計數閘門已證，此處再以合成釘死語義。
    #[test]
    fn drawing_rows_and_breaks_stay_out() {
        let xml = br#"<worksheet>
          <sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>
          <rowBreaks count="1"><brk id="5"/></rowBreaks>
          <drawing><anchor><xdr:row>9</xdr:row></anchor></drawing>
        </worksheet>"#;
        let sc = read_sheet_cells(xml, None);
        assert_eq!(sc.rows.len(), 1, "xdr:row 與 brk 不計");
        assert_eq!(sc.cell_count, 1);
    }
}
