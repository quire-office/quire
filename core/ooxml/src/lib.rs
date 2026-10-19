//! quire-ooxml —— DOCX(zip+OOXML) 讀取＋無損存回。
//!
//! 定位：這不是完整的 ECMA-376 模型，而是「無損 round-trip」路線（DOCX 為 canonical
//! model，未觸碰的 part 原樣保留）的最小可行起點。完整規格覆蓋是本專案本體，
//! 依「真實文件 corpus 驅動」逐項推進（見研究報告 §7）。
//!
//! 無損的實作分兩層：
//! 1. [`zip_struct`]——zip 結構自己解析／組裝，未觸碰 entry 搬運原始位元組；
//! 2. [`OoxmlDoc::save`]——把 parts map 與 bundle 逐一比對，只有真的改過的 entry
//!    才重新壓縮，其餘逐位元組沿用。開啟→不動→存檔 = byte-identical。

mod diff;
mod drawing;
mod rels;
mod sectpr;
mod styles;
mod table;
mod zip_struct;

pub use diff::{diff_untouched, diff_zip_structure, EntryDiff, ZipDiff, ZipStructureError};
pub use drawing::{count_paras, origin_of, read_images, InlineImage};
pub use rels::{read_rels, resolve_relationship, Relationship};
pub use sectpr::{
    read_even_and_odd_headers, read_sections_header_footer, read_sections_per_paragraph,
    SectionGeometry, SectionHeaderFooter, SectionSpec, SlotRefs,
};
pub use styles::{Align, LineSpacing, ParaStyle, RunStyle, StyleTable};
pub use table::{
    Block, BodyTable, Border, Borders, CellProps, GridCol, Margins, RowProps, Shading, Table,
    TableCell, TableProps, TableRow, TblWidth,
};
pub use zip_struct::{Change, ZipBundle, ZipEntry, ZipError};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OoxmlError {
    #[error("zip 解析失敗: {0}")]
    Zip(String),
    #[error("找不到 word/document.xml，不是有效的 .docx")]
    MissingDocument,
    #[error("XML 解析失敗: {0}")]
    Xml(String),
}

impl From<ZipError> for OoxmlError {
    fn from(e: ZipError) -> Self {
        OoxmlError::Zip(e.to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    /// 文字顏色（0xAARRGGBB）。`w:color/@w:val="auto"` 或缺省時為 `None`（發射時預設黑）。
    #[serde(default)]
    pub color: Option<u32>,
    /// 底線（`w:u` 且 `val != "none"`，或 `w:hyperlink` 內無顯式樣式時預設開啟）。
    #[serde(default)]
    pub underline: bool,
    /// 刪除線（`w:strike`）。
    #[serde(default)]
    pub strike: bool,
    /// 欄位結果標記（`PAGE`／`NUMPAGES`）：此 run 的文字是欄位的**過期快取值**，
    /// 發射顯示列表時必須以當頁頁碼／總頁數覆寫（ECMA-376 §17.5 欄位語義；
    /// 快取值只在存檔那一刻正確，渲染當下即時換算）。
    #[serde(default)]
    pub field: Option<FieldKind>,
    /// 字元樣式引用（`w:rPr/w:rStyle/@w:val`＝styleId；t_5dfc8ac7）。級聯層
    /// 以它查 `StyleTable::effective_char` 作 run 直接格式之下的基底；缺＝None。
    #[serde(default)]
    pub rstyle: Option<String>,
    /// run 直接格式 `w:rPr` 的三態快照（sz/b/i/u/strike/color；t_5dfc8ac7）。
    /// 既有 bool 欄位是它的有損投影（無「未宣告」態）；級聯層只讀本欄。
    /// 未見任何 rPr 宣告＝None（整段繼承樣式）。存檔不經此欄位。
    #[serde(default)]
    pub direct: Option<RunStyle>,
    /// 級聯合成的**有效字級**（pt；t_5dfc8ac7，engine 開啟文件時烘焙寫入）。
    /// `None`＝級聯未宣告（發射層退回段落／文件級字級）。純派生欄位——
    /// 存檔路徑完全不經它（parts passthrough），serde default 保舊快照相容。
    #[serde(default)]
    pub size_pt: Option<f32>,
    /// run 內含製表符（`w:tab`；t_5dfc8ac7）。文字欄不動（tab 不是字元），
    /// engine 投影時以「前綴→點導引填補」展開（TOC 點引leads）。
    #[serde(default)]
    pub tab: bool,
}

/// 本卡支援的頁碼欄位型別（其余欄位名如 SECTIONPAGES／DATE 老實保留快取文字）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FieldKind {
    /// `PAGE`：當前頁碼（1-based）。
    Page,
    /// `NUMPAGES`：總頁數。
    NumPages,
}

/// 欄位指令字串 → `FieldKind`（只認 `PAGE`／`NUMPAGES` 兩個**完整 token**，
/// 先比 NUMPAGES——"PAGE" 是 "NUMPAGES" 的子串）。指令可帶開關（如
/// ` PAGE  \* MERGEFORMAT `），以非英数字元切 token 後逐字比對。
pub fn classify_field_instr(instr: &str) -> Option<FieldKind> {
    let mut hit: Option<FieldKind> = None;
    for token in instr
        .to_ascii_uppercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
    {
        match token {
            "NUMPAGES" => return Some(FieldKind::NumPages),
            "PAGE" => hit = Some(FieldKind::Page),
            _ => {}
        }
    }
    hit
}

/// 超連結內 run 無顯式顏色時的預設藍色（Word Hyperlink 樣式標準色 #0563C1）。
pub const HYPERLINK_DEFAULT_COLOR_ARGB: u32 = 0xFF05_63C1;

/// 解析 OOXML `<w:color w:val="..."/>` 色值字串為 `0xAARRGGBB`。
///
/// - 6 位 hex（`RRGGBB`）→ 補齊不透明 alpha `0xFF00_0000 | rgb`；
/// - 8 位 hex（`AARRGGBB`）→ 直接解析為 `u32`；
/// - `"auto"`（不分大小寫）或非法字串 → `None`（發射至顯示列表時轉為黑 `0xFF00_0000`）。
/// - 註：`w:themeColor` 佈景主題色暫不解析（誠實近似；若元素同時帶 6/8 位 `w:val`
///   則直接採用該預計算色值）。
pub fn parse_color_val(val: &str) -> Option<u32> {
    let s = val.trim();
    if s.eq_ignore_ascii_case("auto") {
        return None;
    }
    if !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match s.len() {
        6 => u32::from_str_radix(s, 16).ok().map(|rgb| 0xFF00_0000 | rgb),
        8 => u32::from_str_radix(s, 16).ok(),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Paragraph {
    pub style: Option<String>,
    pub runs: Vec<Run>,
    /// 強制分頁斷點（`w:r/w:br w:type="page"`，ECMA-376 §17.4.3）：本段純文字
    /// 序列中的**碼點偏移**，語義＝斷點落在「第 o 個碼點之前」：
    /// - `o = 0`：斷點前無文字（br 是 run 開頭）→ 本段之前硬斷頁；
    /// - `o = 段長`：br 後無文字 → 本段之後硬斷頁；
    /// - `0 < o < 段長`：段中硬斷，斷點前的文字留在原頁。
    ///
    /// 一個段落可有多處（逐 br 記錄，允許重複——分頁層自行去重）。
    ///
    /// 鐵律：`w:lastRenderedPageBreak` 是 Word 存檔當下的**渲染提示**，依
    /// ECMA-376 不具強制斷頁語義，**不得**記入此欄（對齊 LO 卻錯 Word 的
    /// 教訓，見 t_01fa57ac 背景）；`w:type="column"`／`textWrapping`（預設）
    /// 亦非頁斷點。round-trip 與此欄無關：寫回來源仍是 parts 明文。
    #[serde(default)]
    pub page_breaks: Vec<usize>,
    /// 段落直接格式 `w:pPr/w:rPr`（run 級三態宣告；t_5dfc8ac7）。段落級級聯
    /// （`styles::StyleTable::effective_para`）的 inline 輸入；無宣告＝None
    /// （serde default：舊序列化相容）。存檔不經此欄位（parts passthrough）。
    #[serde(default)]
    pub prun: Option<RunStyle>,
    /// 段落直接行距宣告（`w:pPr/w:spacing/@w:line`＋`@w:lineRule`）。
    #[serde(default)]
    pub line: Option<LineSpacing>,
    /// 段落直接對齊宣告（`w:pPr/w:jc/@w:val`；本卡解析保留，發射不消費）。
    #[serde(default)]
    pub jc: Option<Align>,
    /// 顯式分頁開關（`w:pPr/w:pageBreakBefore`；t_5dfc8ac7）。段級語義＝本段
    /// 從新頁開始。**w:br type="page" 不寫此欄**——段內 br 一律走 `page_breaks`
    /// 碼點偏移（粗細兩條路涇渭分明，合併裁決：br 由 br-page 卡細分語義為準，
    /// 此欄只承載 pageBreakBefore，避免同一 br 雙機制重複推頁）。存檔不經此欄。
    #[serde(default)]
    pub page_break: bool,
    /// 首個右對齊製表位（`w:pPr/w:tabs/w:tab/@w:pos`，pt；t_5dfc8ac7）。
    /// TOC 點引leads 的對齊目標；無宣告＝None。
    #[serde(default)]
    pub tab_right_pt: Option<f32>,
    /// 該製表位導引字元（`w:tab/@w:leader` 原值：dot/hyphen/underscore/…）。
    #[serde(default)]
    pub tab_leader: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OoxmlDoc {
    pub paragraphs: Vec<Paragraph>,
    /// body 級表格的 canonical 投影（只讀；寫回來源仍是 parts 明文，
    /// 見 [`table`] 模組頭「唯讀來源隔離」）。段落以索引引用 `paragraphs`。
    pub tables: Vec<BodyTable>,
    /// 全部 zip part 的明文（解壓後）——編輯面的工作副本；save() 以此為準寫回。
    pub parts: BTreeMap<String, Vec<u8>>,
    /// zip 結構快照：存回時未觸碰 entry 原樣搬運的依據。
    pub bundle: ZipBundle,
}

impl OoxmlDoc {
    /// 每個段落的純文字（供給版面層）。
    pub fn paragraph_texts(&self) -> Vec<String> {
        self.paragraphs
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect()
    }

    /// 覆寫／新增一個 part 的明文（供編輯路徑；存回時才真正壓縮寫入）。
    pub fn set_part(&mut self, name: &str, bytes: Vec<u8>) {
        self.parts.insert(name.to_string(), bytes);
    }

    /// 標記刪除一個 part（凍結 entry 会在存檔時被忽略，見 zip_struct 凍結規則）。
    pub fn remove_part(&mut self, name: &str) -> bool {
        self.parts.remove(name).is_some()
    }

    /// 存檔：parts 與 bundle 逐 entry 比對，只有內容真的變了的 entry 才重建，
    /// 其餘（含壓縮位元組、extra field、entry comment、data descriptor gap）
    /// 逐位元組沿用原始 archive。parts 未動時輸出與輸入位元組完全相同。
    pub fn save(&self) -> Result<Vec<u8>, OoxmlError> {
        let mut changes: BTreeMap<String, Change> = BTreeMap::new();
        let mut extras: BTreeMap<String, Vec<u8>> = BTreeMap::new();

        for e in self.bundle.file_entries() {
            let key = e.key();
            match self.parts.get(&key) {
                None => {
                    changes.insert(key, Change::Remove);
                }
                Some(bytes) => {
                    // 先比壓縮位元組（未觸碰最常見的捷徑），再比明文。
                    if e.method == zip_struct::METHOD_STORE && bytes == &e.data_raw {
                        continue;
                    }
                    let original = e.decoded()?;
                    if bytes != &original {
                        changes.insert(key, Change::Replace(bytes.clone()));
                    }
                }
            }
        }
        // parts 裡新增的 key（bundle 沒有對應 entry）→ 追加。
        for (name, bytes) in &self.parts {
            if self.bundle.entry(name).is_none() {
                extras.insert(name.clone(), bytes.clone());
            }
        }
        Ok(self.bundle.serialize(&changes, &extras)?)
    }
}

/// 由 .docx 位元組解析（不碰檔案系統，wasm32 友善）。
pub fn parse(bytes: &[u8]) -> Result<OoxmlDoc, OoxmlError> {
    let bundle = ZipBundle::open(bytes)?;
    let mut parts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for e in bundle.file_entries() {
        parts.insert(e.key(), e.decoded()?);
    }
    let xml = parts
        .get("word/document.xml")
        .ok_or(OoxmlError::MissingDocument)?;
    let paragraphs = parse_document_xml(xml)?;
    // 表格投影：純讀入（PM 補正 1——唯讀來源隔離；無損寫回路徑完全不經此处）。
    let tables = table::read_body_tables(xml);
    Ok(OoxmlDoc {
        paragraphs,
        tables,
        parts,
        bundle,
    })
}

/// 解析頁首／頁尾 part（`w:hdr`／`w:ftr` 根）的投影：段落＋body 級表格。
///
/// 與 `word/document.xml` 走**完全同一組解析器**（`parse_document_xml`＋
/// `read_body_tables`）：同一套 w:p Start 佔槽口徑、同一套 mc:Fallback 吞沒
/// ——段落索引只在 part 內自洽（engine 層按 part 各自持有 texts 序列），
/// 不與正文索引共享計數空間。18.9% 的頁首用 `w:tbl` 排版（公文表頭），
/// 表格投影這一條路必須與段落同時可解析。純讀取：parts 明文不動。
pub fn parse_header_footer_part(
    xml: &[u8],
) -> Result<(Vec<Paragraph>, Vec<BodyTable>), OoxmlError> {
    let paragraphs = parse_document_xml(xml)?;
    let tables = table::read_body_tables(xml);
    Ok((paragraphs, tables))
}

/// 解析 OOXML 開關型或底線屬性（`w:b`、`w:i`、`w:strike`、`w:u`）：
/// 無 `w:val` 時視為開啟（`true`）；有 `w:val` 時若為 `"none"`/`"false"`/`"0"`/`"off"`
/// （不分大小寫）則為關閉（`false`），其餘（如 `"true"`/`"1"`/`"on"`/`"single"`）為開啟。
pub(crate) fn toggle_attr_on(e: &quick_xml::events::BytesStart) -> bool {
    match attr_val(e, "val") {
        None => true,
        Some(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "none" | "false" | "0" | "off"
        ),
    }
}

/// 段落級分頁/製表位標籤（Start 與 Empty 共用；t_5dfc8ac7）。呼叫端已保證
/// 處於 pPr 區（ppr_depth==1 且非 pPr/rPr）或 run 區（依標籤各自判定）。
/// - `w:br/@w:type="page"`：段級顯式分頁（見 `Paragraph::page_break` 注）。
/// - `w:pageBreakBefore`：同上（開關語義同 toggle_attr_on）。
/// - `w:tabs/w:tab`：首個 `val="right"` 的製表位（pos twips→pt）＋leader。
/// - run 內 `w:tab`：製表符標記（文字欄不動，engine 投影展開）。
fn apply_break_tab_tag(
    name: &[u8],
    e: &quick_xml::events::BytesStart,
    p: &mut Paragraph,
    cur_run: &mut Option<Run>,
    ppr_depth: usize,
    ppr_rpr_depth: usize,
) {
    match name {
        b"pageBreakBefore" if ppr_depth == 1 && ppr_rpr_depth == 0 => {
            p.page_break |= toggle_attr_on(e);
        }
        b"tab" => {
            if cur_run.is_some() {
                // run 內 w:tab：run 級標記（無文字產出；投影層展開）。
                cur_run.as_mut().expect("剛查 some").tab = true;
            } else if ppr_depth == 1
                && ppr_rpr_depth == 0
                && p.tab_right_pt.is_none()
                && attr_val(e, "val").as_deref() == Some("right")
            {
                // pPr/w:tabs/w:tab：只取首個右對齊位（本引擎 leads 錨點）。
                // ppr_depth 守門：tblPr/w:tblTabs 的 w:tab（表格尺度）不收。
                if let Some(twips) = attr_val(e, "pos")
                    .as_deref()
                    .and_then(|v| v.trim().parse::<f64>().ok())
                {
                    p.tab_right_pt = Some((twips / 20.0) as f32);
                    p.tab_leader = attr_val(e, "leader");
                }
            }
        }
        _ => {}
    }
}

/// 套用段落或 run 內部的樣式標籤（Start 與 Empty 事件共用）。
/// 注意：不含 `pStyle`——段落樣式改由呼叫端寫進段落棧頂（巢狀 `w:p` 口徑）。
fn apply_style_tag(
    name: &[u8],
    e: &quick_xml::events::BytesStart,
    cur_run: &mut Option<Run>,
    run_has_color: &mut bool,
    run_has_underline: &mut bool,
) {
    // ---- run 直接格式三態快照（t_5dfc8ac7）：語義唯一來源＝styles::apply_run_tag。
    // bool 欄位照舊寫（既有消費者不動），三態快照並行記錄給級聯層。只認
    // run 屬性標籤本身——catch-all 送來的無關標籤（noProof/lang…）不得把
    // direct 材料化成 Some（None＝「未見任何 rPr 宣告」的語義必須守住）。
    if matches!(name, b"b" | b"i" | b"u" | b"strike" | b"color" | b"sz") {
        if let Some(r) = cur_run.as_mut() {
            styles::apply_run_tag(name, e, r.direct.get_or_insert_with(RunStyle::default));
        }
    }
    match name {
        b"rStyle" => {
            // 字元樣式引用（styleId）：級聯層在 run 直接格式之下作基底查表。
            if let Some(r) = cur_run.as_mut() {
                if let Some(v) = attr_val(e, "val") {
                    r.rstyle = Some(v);
                }
            }
        }
        b"b" => {
            if let Some(r) = cur_run.as_mut() {
                r.bold = toggle_attr_on(e);
            }
        }
        b"i" => {
            if let Some(r) = cur_run.as_mut() {
                r.italic = toggle_attr_on(e);
            }
        }
        b"u" => {
            if let Some(r) = cur_run.as_mut() {
                *run_has_underline = true;
                r.underline = toggle_attr_on(e);
            }
        }
        b"strike" => {
            if let Some(r) = cur_run.as_mut() {
                r.strike = toggle_attr_on(e);
            }
        }
        b"color" => {
            if let Some(r) = cur_run.as_mut() {
                // themeColor 暫不解析（誠實近似）：僅解析 w:val 的 6/8 位 hex，auto→None。
                let parsed = attr_val(e, "val").and_then(|v| parse_color_val(&v));
                *run_has_color = parsed.is_some();
                r.color = parsed;
            }
        }
        _ => {}
    }
}

/// 最小 word/document.xml 解析器：段落 / run / 文字 / 粗斜體 / 顏色 / 底線 / 刪除線 / 超連結 / 段落樣式。
///
/// 扁平段落序列的索引口徑（與 `table.rs`、`sectpr.rs` 三解析器對齊的唯一真相）：
/// - 每個非自閉合 `w:p` 的 **Start** 佔一個槽位（Start 預位、End 回填完成品）
///   ——扁平索引＝Start 序，巢狀 `w:p`（`w:txbxContent` 文字框段落）不改寫
///   外層槽位，外層段落於文字框前後的文字完整保留；
/// - `mc:Fallback` 子樹整體吞沒：`mc:AlternateContent` 的 Choice（DrawingML）
///   與 Fallback（VML）是同一內容的雙份表示，只解析 Choice 一份，圖形文字框
///   段落不再出現雙份（對照 `table.rs` `Frame::Skip` 同款做法；其內段落與
///   table／sectpr 同口徑——不計）。
///
/// 骨架階段刻意不求全面（欄位、圖形幾何皆未處理），演進順序由 corpus 綠燈率驅動。
fn parse_document_xml(xml: &[u8]) -> Result<Vec<Paragraph>, OoxmlError> {
    use quick_xml::events::Event;
    let text = String::from_utf8_lossy(xml).into_owned();
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut out: Vec<Paragraph> = Vec::new();
    // 段落棧：(out 槽位, 半成品, 已入文碼點數)。Start push、End pop 回填——
    // 巢狀 w:p 各自獨立，內層完結不影響外層（舊實作單一 cur_para 會被內層
    // 覆寫＋清空，外層文字全吞；corpus 實測 25.5% 檔案受害）。
    let mut stack: Vec<(usize, Paragraph, usize)> = Vec::new();
    let mut cur_run: Option<Run> = None;
    let mut run_has_color = false;
    let mut run_has_underline = false;
    // 段落直接格式（t_5dfc8ac7）：w:pPr 巢狀深度（只認 depth==1；深一層＝
    // w:pPrChange 修訂殘影不收）＋其內 w:rPr 深度（spacing/jc 只收 rPr 外的）。
    let mut ppr_depth = 0usize;
    let mut ppr_rpr_depth = 0usize;
    let mut hyperlink_depth = 0usize;
    let mut in_t = false;
    // >0＝正在吞 mc:Fallback 子樹（巢狀 Fallback 以開閉計數自行平衡）。
    let mut fb: usize = 0;
    // mc:AlternateContent 巢狀棧：開標時快照 fb，閉標時還原——錯置標籤
    // （AC 先於 Fallback 閉合／Fallback 漏閉）不致 fb 卡死而吞掉檔尾。
    let mut ac_stack: Vec<usize> = Vec::new();
    // ---- 欄位（ECMA-376 §17.5）：只標記 PAGE／NUMPAGES 結果 run ----
    // 簡單欄 `<w:fldSimple w:instr="…">` 巢狀棧（棧頂 None＝非頁碼欄位）。
    let mut fld_simple_stack: Vec<Option<FieldKind>> = Vec::new();
    // 複雜欄 `fldChar begin→instrText→separate→結果 run→end`：每層記
    // （已辨識型別, 是否已過 separate）。separate 之後的 run 才是欄位結果
    // （begin..separate 區間只有指令 run，不標記）。
    let mut complex_stack: Vec<(Option<FieldKind>, bool)> = Vec::new();
    let mut instr_active = false;
    let mut instr_buf = String::new();

    loop {
        match reader
            .read_event()
            .map_err(|e| OoxmlError::Xml(e.to_string()))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_vec();
                if fb > 0 {
                    // Fallback 子樹內：只維護巢狀深度與 AC 快照，其餘一概不看。
                    if name == b"Fallback" {
                        fb += 1;
                    } else if name == b"AlternateContent" {
                        ac_stack.push(fb);
                    }
                    continue;
                }
                match name.as_slice() {
                    b"p" => {
                        // Start 即預位：扁平索引＝Start 序（口徑鐵律，見函式頭注）。
                        stack.push((out.len(), Paragraph::default(), 0));
                        out.push(Paragraph::default());
                        cur_run = None;
                        hyperlink_depth = 0;
                        // 防呆：上游若有未閉合的 pPr/rPr（畸形檔），新段落起點
                        // 一律歸零（同 hyperlink_depth 的防禦策略）。
                        ppr_depth = 0;
                        ppr_rpr_depth = 0;
                    }
                    b"hyperlink" => {
                        hyperlink_depth += 1;
                    }
                    b"br" => {
                        // 成對標籤寫法 `<w:br w:type="page"></w:br>`（合法但罕見）
                        // 的入口：Start/Empty 互斥，自閉合不會重入此臂、零雙計。
                        // 合併裁決：段內 w:br type="page" 一律走此細分記錄
                        // （page_breaks 碼點偏移），t_5dfc8ac7 的粗化臂只留
                        // pPr/w:pageBreakBefore（見下方 pageBreakBefore 臂）。
                        record_page_break(&mut stack, &e);
                    }
                    // 段落直接格式（t_5dfc8ac7）：w:pPr 欄位＋其內 w:rPr 收進
                    // 段落棧頂（三態級聯輸入）。只認 depth==1 的 pPr——深一層
                    // 的 pPr 只可能是 w:pPrChange（修訂歷史殘影），Word 正常
                    // 顯示不套它，我們也不收（收了會用過期值污染級聯）。
                    b"pPr" if fb == 0 => {
                        ppr_depth += 1;
                    }
                    b"rPr" if ppr_depth == 1 => {
                        ppr_rpr_depth += 1;
                    }
                    b"spacing" if ppr_depth == 1 && ppr_rpr_depth == 0 => {
                        if let Some((_, p, _)) = stack.last_mut() {
                            p.line = styles::line_spacing(
                                attr_val(&e, "line").as_deref(),
                                attr_val(&e, "lineRule").as_deref(),
                            );
                        }
                    }
                    b"jc" if ppr_depth == 1 && ppr_rpr_depth == 0 => {
                        if let Some((_, p, _)) = stack.last_mut() {
                            p.jc = styles::align_of(attr_val(&e, "val").as_deref());
                        }
                    }
                    // 顯式分頁開關＋製表位（t_5dfc8ac7）：w:br 已由上方細分臂
                    // 攔截，此臂收 pPr/w:pageBreakBefore 與 w:tabs/w:tab。
                    b"pageBreakBefore" | b"tab" if ppr_rpr_depth == 0 => {
                        if let Some((_, p, _)) = stack.last_mut() {
                            apply_break_tab_tag(
                                name.as_slice(),
                                &e,
                                p,
                                &mut cur_run,
                                ppr_depth,
                                ppr_rpr_depth,
                            );
                        }
                    }
                    // pPr 直下的 w:rPr 內容：收進段落棧頂的三態快照（段落級
                    // 級聯輸入）。ppr_rpr_depth>0 時 run 標籤**不得**走下方
                    // apply_style_tag（會把段落標記屬性誤灌給進行中的 run）。
                    b"b" | b"i" | b"u" | b"strike" | b"color" | b"sz" | b"rStyle"
                        if ppr_rpr_depth > 0 =>
                    {
                        if let Some((_, p, _)) = stack.last_mut() {
                            styles::apply_run_tag(
                                name.as_slice(),
                                &e,
                                p.prun.get_or_insert_with(RunStyle::default),
                            );
                        }
                    }
                    b"fldSimple" => {
                        // 簡單欄：w:instr 指令立即分類，其子樹 run 全標記。
                        let kind = attr_val(&e, "instr")
                            .as_deref()
                            .and_then(classify_field_instr);
                        fld_simple_stack.push(kind);
                    }
                    b"r" => {
                        cur_run = Some(Run::default());
                        run_has_color = false;
                        run_has_underline = false;
                        // 欄位結果 run 標記：簡單欄子樹，或複雜欄 separate 之後。
                        let kind = match fld_simple_stack.last() {
                            Some(k) => *k,
                            None => match complex_stack.last() {
                                Some((k, true)) => *k,
                                _ => None,
                            },
                        };
                        if kind.is_some() {
                            cur_run.as_mut().expect("剛建 run").field = kind;
                        }
                    }
                    b"instrText" => {
                        instr_active = true;
                    }
                    b"t" => in_t = true,
                    b"pStyle" => {
                        if let Some((_, p, _)) = stack.last_mut() {
                            if let Some(v) = attr_val(&e, "val") {
                                p.style = Some(v);
                            }
                        }
                    }
                    b"Fallback" => fb += 1,
                    b"AlternateContent" => ac_stack.push(fb),
                    other => apply_style_tag(
                        other,
                        &e,
                        &mut cur_run,
                        &mut run_has_color,
                        &mut run_has_underline,
                    ),
                }
            }
            Event::Empty(e) => {
                if fb > 0 {
                    continue;
                }
                let name = e.local_name().as_ref().to_vec();
                if name == b"br" {
                    // 強制分頁（§17.4.3）：只認 w:type="page"；column／缺省
                    // （textWrapping）不是頁斷點。`w:lastRenderedPageBreak`
                    // 是另一元素，永不進此臂（渲染提示≠斷點，見 Paragraph 欄注）。
                    record_page_break(&mut stack, &e);
                } else if name == b"fldChar" {
                    // 複雜欄三態（Word 產出恒為自閉合標籤）：begin 開層、
                    // separate 翻結果標記、end 出層。未配對（漏 begin）老實無效。
                    match attr_val(&e, "fldCharType").as_deref() {
                        Some("begin") => {
                            complex_stack.push((None, false));
                            instr_buf.clear();
                        }
                        Some("separate") => {
                            if let Some(top) = complex_stack.last_mut() {
                                if top.0.is_none() {
                                    top.0 = classify_field_instr(&instr_buf);
                                }
                                top.1 = true;
                            }
                        }
                        Some("end") => {
                            complex_stack.pop();
                        }
                        _ => {}
                    }
                } else if name == b"pStyle" {
                    // pStyle 幾乎總是自閉合標籤：寫進段落棧頂（巢狀 w:p 口徑）。
                    if let Some((_, p, _)) = stack.last_mut() {
                        if let Some(v) = attr_val(&e, "val") {
                            p.style = Some(v);
                        }
                    }
                } else if name == b"rPr" && ppr_depth == 1 {
                    // 自閉合 pPr/rPr（`<w:rPr/>`）：不進 run 區（其後的
                    // spacing/jc 仍是段落欄位）。
                } else if name == b"spacing" && ppr_depth == 1 && ppr_rpr_depth == 0 {
                    // 自閉合直接行距（Word 產出恆為自閉合）。
                    if let Some((_, p, _)) = stack.last_mut() {
                        p.line = styles::line_spacing(
                            attr_val(&e, "line").as_deref(),
                            attr_val(&e, "lineRule").as_deref(),
                        );
                    }
                } else if name == b"jc" && ppr_depth == 1 && ppr_rpr_depth == 0 {
                    if let Some((_, p, _)) = stack.last_mut() {
                        p.jc = styles::align_of(attr_val(&e, "val").as_deref());
                    }
                } else if matches!(name.as_slice(), b"br" | b"tab" | b"pageBreakBefore")
                    && ppr_rpr_depth == 0
                {
                    // 顯式分頁開關/製表符（Word 產出恆為自閉合；t_5dfc8ac7）。
                    // w:br type="page" 已在上方 record_page_break 細分記錄，
                    // 此處 br 落 apply_break_tab_tag 只剩 run 級 w:tab 標記。
                    if let Some((_, p, _)) = stack.last_mut() {
                        apply_break_tab_tag(
                            name.as_slice(),
                            &e,
                            p,
                            &mut cur_run,
                            ppr_depth,
                            ppr_rpr_depth,
                        );
                    }
                } else {
                    apply_style_tag(
                        name.as_slice(),
                        &e,
                        &mut cur_run,
                        &mut run_has_color,
                        &mut run_has_underline,
                    );
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_vec();
                // MC 標籤完結先行處理（即便在 Fallback 內）：錯置標籤
                // （AC 先於 Fallback 閉合）不致 fb 卡死而吞掉檔尾。
                if name == b"AlternateContent" {
                    fb = ac_stack.pop().unwrap_or(0);
                    continue;
                }
                if fb > 0 {
                    if name == b"Fallback" {
                        fb = fb.saturating_sub(1);
                    }
                    continue;
                }
                match name.as_slice() {
                    b"t" => in_t = false,
                    b"pPr" => ppr_depth = ppr_depth.saturating_sub(1),
                    b"rPr" => ppr_rpr_depth = ppr_rpr_depth.saturating_sub(1),
                    b"instrText" => {
                        instr_active = false;
                    }
                    b"fldSimple" => {
                        fld_simple_stack.pop();
                    }
                    b"hyperlink" => {
                        hyperlink_depth = hyperlink_depth.saturating_sub(1);
                    }
                    b"r" => {
                        if let (Some(mut run), Some((_, p, _))) = (cur_run.take(), stack.last_mut())
                        {
                            // 欄位標記只給「有文字」的結果 run：fldChar 標記 run
                            //（begin/separate/end）同處欄位子樹但無文字，若保留
                            // 標記，發射層代換會把頁碼注入空 run（虛增字）。
                            if run.text.is_empty() {
                                run.field = None;
                            }
                            // <w:hyperlink> 內 run 無顯式樣式時預設藍（0xFF0563C1）＋底線。
                            if hyperlink_depth > 0 {
                                if !run_has_color && run.color.is_none() {
                                    run.color = Some(HYPERLINK_DEFAULT_COLOR_ARGB);
                                    // 代擬色同時進三態快照（t_5dfc8ac7）：它是
                                    // 解析器代擬的**直接格式**，級聯時必須與
                                    // 真實 rPr/w:color 同層，否則 docDefaults
                                    // 的顏色會越過它搶先命中（連結變色）。
                                    let d = run.direct.get_or_insert_with(RunStyle::default);
                                    d.color_set = true;
                                    d.color = Some(HYPERLINK_DEFAULT_COLOR_ARGB);
                                }
                                if !run_has_underline {
                                    run.underline = true;
                                    if let Some(d) = run.direct.as_mut() {
                                        d.underline = Some(true);
                                    } else {
                                        run.direct = Some(RunStyle {
                                            underline: Some(true),
                                            ..Default::default()
                                        });
                                    }
                                }
                            }
                            p.runs.push(run);
                        }
                    }
                    b"p" => {
                        // 完結回填 Start 時預留的槽位；無配對 Start（錯置標籤）＝忽略。
                        // 未閉合到 EOF 的段落保持預位的空段落——Start 口徑不因此漂移。
                        if let Some((slot, p, _)) = stack.pop() {
                            out[slot] = p;
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if fb > 0 {
                    continue;
                }
                if instr_active {
                    // 欄位指令文字（可能跨多個 Text 事件）：累積供分類。
                    if let Ok(s) = t.unescape() {
                        instr_buf.push_str(&s);
                    }
                    continue;
                }
                if let Some(r) = cur_run.as_mut().filter(|_| in_t) {
                    let s = t.unescape().map_err(|e| OoxmlError::Xml(e.to_string()))?;
                    // 已入文碼點計數隨文字增長——br-page 記錄偏移的標尺。
                    if let Some((_, _, cp_done)) = stack.last_mut() {
                        *cp_done += s.chars().count();
                    }
                    r.text.push_str(&s);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn attr_val(e: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key.as_bytes() {
            a.unescape_value().ok().map(|v| v.to_string())
        } else {
            None
        }
    })
}

/// `w:br` 自閉合標籤 → 段級斷點記錄（僅 `w:type="page"`，ECMA-376 §17.4.3）。
/// 偏移＝段落棧頂「已入文碼點數」：文字一律經 Text 臂先行累計，故 br 前後
/// 的文字各歸斷點兩側（`o=0` 段前、`o=段長` 段後、中間段中）。無配對段落
/// （body 級游离 br，畸形）＝老實忽略。`w:type="column"`／缺省（textWrapping）
/// 不記；`w:lastRenderedPageBreak` 是另一元素名，呼叫端標籤比對就擋掉。
fn record_page_break(stack: &mut [(usize, Paragraph, usize)], e: &quick_xml::events::BytesStart) {
    if attr_val(e, "type").as_deref() != Some("page") {
        return;
    }
    if let Some((_, p, cp_done)) = stack.last_mut() {
        p.page_breaks.push(*cp_done);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = include_bytes!("../../../fixtures/sample.docx");

    #[test]
    fn parses_sample_docx() {
        let doc = parse(SAMPLE).expect("解析 fixtures/sample.docx 失敗");
        assert!(doc.parts.contains_key("[Content_Types].xml"));
        assert!(doc.parts.contains_key("word/document.xml"));
        assert!(!doc.paragraphs.is_empty());
        assert!(doc
            .paragraphs
            .iter()
            .any(|p| p.style.as_deref() == Some("Title")));
        assert!(doc
            .paragraphs
            .iter()
            .any(|p| p.runs.iter().any(|r| r.bold && r.text.contains("Quire"))));
        // &amp; 實體必須還原成 &
        assert!(doc
            .paragraphs
            .iter()
            .any(|p| p.runs.iter().any(|r| r.text.contains("R&D"))));
    }

    #[test]
    fn rejects_non_docx() {
        assert!(parse(b"not a zip at all").is_err());
    }

    // ---------------- 頁首頁尾（t_b39f108b） ----------------

    /// 欄位指令歸類：完整 token 比對、NUMPAGES 先於 PAGE（子串陷阱）、
    /// 大小寫與開關旗標（\* MERGEFORMAT）皆不誤傷。
    #[test]
    fn field_instr_classification() {
        use FieldKind::{NumPages, Page};
        assert_eq!(classify_field_instr(" PAGE "), Some(Page));
        assert_eq!(classify_field_instr("page"), Some(Page));
        assert_eq!(classify_field_instr(" PAGE  \\* MERGEFORMAT "), Some(Page));
        assert_eq!(classify_field_instr("NUMPAGES"), Some(NumPages));
        assert_eq!(
            classify_field_instr(" NUMPAGES \\* MERGEFORMAT "),
            Some(NumPages)
        );
        // "PAGE" 是 "NUMPAGES" 的子串：混合指令以 NUMPAGES 為準。
        assert_eq!(classify_field_instr(" PAGE NUMPAGES "), Some(NumPages));
        // 不支援的欄位老實回 None（發射層保留快取文字，不虛構）。
        assert_eq!(classify_field_instr(" SECTIONPAGES "), None);
        assert_eq!(classify_field_instr(" DATE \\@ M "), None);
        assert_eq!(classify_field_instr(""), None);
    }

    /// 頁首頁尾 part 解析：與正文同一 w:p 佔槽口徑；表格（w:tbl 公文表頭）
    /// 與欄位（fldChar 複雜欄＋fldSimple 簡單欄）皆可解析，欄位結果 run
    /// 帶 FieldKind 標記、指令文字與instrText 不入正文序列。
    #[test]
    fn hf_part_parses_paragraphs_tables_and_fields() {
        let xml = r#"<w:ftr xmlns:w="x" xmlns:r="y">
            <w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r>
                 <w:r><w:instrText>PAGE</w:instrText></w:r>
                 <w:r><w:fldChar w:fldCharType="separate"/></w:r>
                 <w:r><w:t>7</w:t></w:r>
                 <w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
            <w:p><w:r><w:t>頁首</w:t></w:r></w:p>
            <w:tbl><w:tblGrid><w:gridCol w:w="100"/></w:tblGrid>
              <w:tr><w:tc><w:p><w:r><w:t>表頭</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
            </w:ftr>"#;
        let (paras, tables) = parse_header_footer_part(xml.as_bytes()).expect("解析頁尾 part");
        assert_eq!(
            paras.len(),
            3,
            "兩段＋表格格內一段（同 document.xml 佔槽口徑）"
        );
        assert_eq!(tables.len(), 1, "w:tbl 表格投影必須可解析");
        assert_eq!(tables[0].table.rows.len(), 1);
        let runs = &paras[0].runs;
        let marked: Vec<_> = runs.iter().filter(|r| r.field.is_some()).collect();
        assert_eq!(marked.len(), 1, "只有 separate 之後的結果 run 帶欄位標記");
        assert_eq!(marked[0].field, Some(FieldKind::Page));
        assert_eq!(
            marked[0].text, "7",
            "快取結果文字保留於 run（發射層才代換）"
        );
        // instrText 是指令不是文字：不得混入 runs。
        assert!(!runs.iter().any(|r| r.text.contains("PAGE")));
        // 簡單欄：fldSimple 內 run 亦帶標記（Word 一律以 w:p 包 fldSimple）。
        let (sp, _) = parse_header_footer_part(
            r#"<w:hdr xmlns:w="x"><w:p><w:fldSimple w:instr=" NUMPAGES "><w:r><w:t>3</w:t></w:r></w:fldSimple></w:p></w:hdr>"#
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(sp[0].runs[0].field, Some(FieldKind::NumPages));
    }

    // ---------------- 解析器缺陷回歸（t_580c37ac） ----------------

    /// 缺陷 1 回歸：`mc:AlternateContent` 的 Choice（DrawingML）與 Fallback
    /// （VML）是同一內容的雙份表示——只解析 Choice 一份，段落不得雙份。
    #[test]
    fn alternate_content_fallback_parsed_once() {
        let xml = r#"<w:document xmlns:w="x" xmlns:mc="m" xmlns:wps="w">
            <w:body>
            <w:p><w:r><w:t>前</w:t></w:r></w:p>
            <mc:AlternateContent>
              <mc:Choice Requires="wps"><w:p><w:r><w:t>框內文</w:t></w:r></w:p></mc:Choice>
              <mc:Fallback><w:p><w:r><w:t>框內文</w:t></w:r></w:p></mc:Fallback>
            </mc:AlternateContent>
            <w:p><w:r><w:t>後</w:t></w:r></w:p>
            </w:body></w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析");
        let texts: Vec<String> = paras
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect();
        assert_eq!(
            texts,
            vec!["前", "框內文", "後"],
            "Choice/Fallback 同文只出一份段落"
        );
    }

    /// 缺陷 1 邊界：Fallback 內有樣板段落與 pStyle 也不得洩漏到扁平序列；
    /// 裸 Fallback（無 AC 包裝，畸形但存在）同樣整棵吞沒。
    #[test]
    fn fallback_content_never_leaks() {
        let xml = r#"<w:document xmlns:w="x" xmlns:mc="m">
            <w:body>
            <mc:Fallback>
              <w:p><w:pPr><w:pStyle w:val="BodyText"/></w:pPr><w:r><w:t>VML雙份</w:t></w:r></w:p>
            </mc:Fallback>
            <w:p><w:r><w:t>正文</w:t></w:r></w:p>
            </w:body></w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析");
        assert_eq!(paras.len(), 1, "裸 Fallback 內段落不進扁平序列");
        assert_eq!(paras[0].runs[0].text, "正文");
        assert!(paras[0].style.is_none());
    }

    /// 缺陷 2 回歸：巢狀 `w:p`（`w:txbxContent` 文字框段落）不改寫外層槽位
    /// ——外層段落在文字框前後的文字完整保留，內層段落各自佔位。
    #[test]
    fn nested_txbx_paragraph_keeps_outer_text() {
        let xml = r#"<w:document xmlns:w="x" xmlns:mc="m" xmlns:wps="w" xmlns:wp="wp">
            <w:body>
            <w:p>
              <w:r><w:t>框前文</w:t></w:r>
              <w:r><mc:AlternateContent>
                <mc:Choice Requires="wps"><w:drawing><wp:anchor><wps:txbx>
                  <w:txbxContent><w:p><w:r><w:t>框內文</w:t></w:r></w:p></w:txbxContent>
                </wps:txbx></wp:anchor></w:drawing></mc:Choice>
                <mc:Fallback><w:pict><v:shape xmlns:v="v"><w:txbxContent>
                  <w:p><w:r><w:t>框內文</w:t></w:r></w:p></w:txbxContent></v:shape></w:pict></mc:Fallback>
              </mc:AlternateContent></w:r>
              <w:r><w:t>框後文</w:t></w:r>
            </w:p>
            </w:body></w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析");
        let texts: Vec<String> = paras
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect();
        assert_eq!(
            texts,
            vec!["框前文框後文", "框內文"],
            "Start 序：外層先佔位且前後文完整；內層文字框段落另佔一槽（Choice 一份）"
        );
    }

    /// 缺陷 2 的段落索引口徑：巢狀段落以 Start 預位——索引＝Start 序，
    /// 外層 End 回填不位移任何既有槽位（table.rs／sectpr.rs 引用同一口徑）。
    #[test]
    fn nested_paragraph_uses_start_slot_order() {
        let xml = br#"<w:body>
            <w:p><w:r><w:t>A</w:t></w:r>
              <w:p><w:r><w:t>B</w:t></w:r></w:p>
              <w:p><w:r><w:t>C</w:t></w:r></w:p>
            <w:r><w:t>D</w:t></w:r></w:p>
            <w:p><w:r><w:t>E</w:t></w:r></w:p>
            </w:body>"#;
        let paras = parse_document_xml(xml).expect("解析");
        let texts: Vec<String> = paras
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect();
        assert_eq!(texts, vec!["AD", "B", "C", "E"], "扁平索引＝w:p Start 序");
    }

    /// corpus 段落數回歸（缺陷 1＋2 實據檔）：修前 Choice/Fallback 雙份解析
    /// ＋巢狀 w:p 覆寫吞字（舊口徑 LINKED 57 段雙份）；修後與獨立口徑
    /// （start 佔槽＋Fallback 吞沒）逐檔一致的 30 段。
    #[test]
    fn corpus_txbx_paragraph_counts_regression() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        let cases: [(&str, usize, &[&str]); 4] = [
            // (檔案, 修後段落數, 修前被吞／雙份、修後必須完整保留的文字)
            (
                "xopt/OpenXmlPowerToolsExamples_TextReplacer02_Test01.docx",
                4,
                &["the on the Insert tab"],
            ),
            // WC049：修前段數同為 8 但外層段「You can also type…」整段被內層
            // 覆寫吞字（缺陷 2 吞字指紋）；修後必須完整保留。
            (
                "xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx",
                8,
                &["You can also type a keyword"],
            ),
            (
                "ooxmlexport/LinkedTextBoxes.docx",
                30,
                &["Congratulations", "IA Scholarship Program Nominations"],
            ),
            // shapes-with-text：修前 10 段（VML 雙份），修後 6 段。
            (
                "poi/test-data_document_shapes-with-text.docx",
                6,
                &["Floating text box"],
            ),
        ];
        for (rel, want_n, must_keep) in cases {
            let bytes = std::fs::read(corpus.join(rel)).unwrap_or_else(|e| panic!("讀 {rel}: {e}"));
            let doc = parse(&bytes).expect("parse");
            let texts = doc.paragraph_texts();
            assert_eq!(texts.len(), want_n, "{rel}：段落數偏離修後基準");
            let joined = texts.join("\n");
            for frag in must_keep {
                assert!(
                    joined.contains(frag),
                    "{rel}：文字「{frag}」被吞（缺陷 2 復發？）"
                );
            }
        }
    }

    /// corpus 全檔柵欄：含 txbxContent／Fallback 的檔案，主解析器扁平段落數
    /// 必須等於表格解析器（獨立狀態機）的 Start 計數器——兩個實作共用以
    /// 上注「同口徑」為契約，此柵欄讓契約在 corpus 上逐檔可驗證。
    #[test]
    fn corpus_txbx_files_start_count_agrees() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        let manifest = std::fs::read_to_string(corpus.join("manifest.json")).expect("manifest");
        let mut checked = 0usize;
        for chunk in manifest.split("\"file\": \"").skip(1) {
            let rel = chunk.split('"').next().unwrap_or_default();
            if rel.is_empty() {
                continue;
            }
            let path = corpus.join(rel);
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(doc) = parse(&bytes) else { continue };
            let Some(xml) = doc.parts.get("word/document.xml") else {
                continue;
            };
            if !String::from_utf8_lossy(xml).contains("txbxContent") {
                continue;
            }
            let got = parse_document_xml(xml).expect("主解析").len();
            let want = crate::table::count_flat_paras(xml);
            assert_eq!(got, want, "{rel}：主解析器與表格計數器口徑分岔");
            checked += 1;
        }
        assert!(checked >= 50, "txbx 檔案數異常：{checked}");
    }

    /// corpus 全檔柵欄（t_a1397522）：**四解析器同口徑**——主解析器扁平段落
    /// 數＝表格計數器＝圖片計數器（drawing 狀態機走完整份文件，含圖／Fallback
    /// ／巢狀文字框檔案也必須逐檔一致）。柵欄只挑「有圖或有 txbx」的檔案：
    /// 那才是口徑會分岔的形態。
    #[test]
    fn corpus_drawing_files_start_count_agrees() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        let manifest = std::fs::read_to_string(corpus.join("manifest.json")).expect("manifest");
        let mut checked = 0usize;
        for chunk in manifest.split("\"file\": \"").skip(1) {
            let rel = chunk.split('"').next().unwrap_or_default();
            if rel.is_empty() {
                continue;
            }
            let path = corpus.join(rel);
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(doc) = parse(&bytes) else { continue };
            let Some(xml) = doc.parts.get("word/document.xml") else {
                continue;
            };
            let text = String::from_utf8_lossy(xml).into_owned();
            let has_pic = text.contains("<w:drawing") || text.contains("<w:pict");
            if !has_pic && !text.contains("txbxContent") {
                continue;
            }
            let got = parse_document_xml(xml).expect("主解析").len();
            assert_eq!(
                got,
                crate::table::count_flat_paras(xml),
                "{rel}：主解析器與表格計數器口徑分岔"
            );
            assert_eq!(
                got,
                crate::drawing::count_paras(xml),
                "{rel}：主解析器與圖片計數器口徑分岔"
            );
            checked += 1;
        }
        assert!(checked >= 30, "含圖檔案數異常：{checked}");
    }

    /// 基線 corpus 圖片投影實據（t_a1397522）：兩份基準檔必須真的掃出圖，
    /// 且每張圖的幾何合法（尺寸正、錨點段落存在）。老實數字寫死在斷言裡：
    /// 投影器退化成「掃不到圖」時此柵欄立即紅。
    #[test]
    fn corpus_baseline_image_projection_is_sane() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        for (rel, min_imgs) in [
            ("ooxmlexport/fdo77887.docx", 1usize),
            ("xopt/TestFiles_HC031-Complicated-Document.docx", 1),
        ] {
            let bytes = std::fs::read(corpus.join(rel)).expect("baseline corpus file");
            let doc = parse(&bytes).expect("parse");
            let xml = doc.parts.get("word/document.xml").expect("document.xml");
            let n_paras = parse_document_xml(xml).expect("主解析").len();
            let imgs = crate::drawing::read_images(xml);
            assert!(
                imgs.len() >= min_imgs,
                "{rel}：圖片投影只掃到 {} 張（下限 {min_imgs}）",
                imgs.len()
            );
            for im in &imgs {
                assert!(
                    im.w_pt > 0.0 && im.h_pt > 0.0,
                    "{rel}：非法尺寸 {:?}",
                    (im.w_pt, im.h_pt)
                );
                assert!(
                    im.para_index < n_paras,
                    "{rel}：錨點段落越界 {} >= {n_paras}",
                    im.para_index
                );
            }
        }
    }

    #[test]
    fn open_then_untouched_save_is_byte_identical() {
        let doc = parse(SAMPLE).expect("open");
        let out = doc.save().expect("save");
        assert_eq!(out.as_slice(), SAMPLE, "開啟→不動→存檔必須位元組級相同");
    }

    #[test]
    fn edited_part_round_trips_without_disturbing_others() {
        let mut doc = parse(SAMPLE).expect("open");
        doc.set_part("word/footer1.xml", b"<footer>new</footer>".to_vec());
        let out = doc.save().expect("save");
        let again = parse(&out).expect("re-open");
        assert_eq!(
            again.parts.get("word/footer1.xml").unwrap(),
            b"<footer>new</footer>"
        );
        // 未觸碰的 part 明文一致、壓縮位元組一致
        for name in ["[Content_Types].xml", "word/document.xml", "_rels/.rels"] {
            assert_eq!(again.parts[name], doc.parts[name], "{name} 明文應不變");
            assert_eq!(
                again.bundle.entry(name).unwrap().data_raw,
                doc.bundle.entry(name).unwrap().data_raw,
                "{name} 壓縮位元組應不變"
            );
        }
    }

    #[test]
    fn part_removal_drops_only_that_entry() {
        let mut doc = parse(SAMPLE).expect("open");
        assert!(doc.remove_part("_rels/.rels"));
        let out = doc.save().expect("save");
        let again = parse(&out).expect("re-open");
        assert!(!again.parts.contains_key("_rels/.rels"));
        assert!(again.parts.contains_key("word/document.xml"));
    }

    #[test]
    fn color_val_parses_hex6_hex8_auto_and_rejects_invalid() {
        // 6 位 hex → 補齊 0xFF 不透明 alpha
        assert_eq!(parse_color_val("FF0000"), Some(0xFFFF_0000));
        assert_eq!(parse_color_val("0563c1"), Some(0xFF05_63C1));
        assert_eq!(parse_color_val("000000"), Some(0xFF00_0000));
        // 8 位 ARGB → 原樣保留 alpha 與 RGB
        assert_eq!(parse_color_val("80FF0000"), Some(0x80FF_0000));
        assert_eq!(parse_color_val("00112233"), Some(0x0011_2233));
        // auto → None（發射時黑）
        assert_eq!(parse_color_val("auto"), None);
        assert_eq!(parse_color_val("AUTO"), None);
        // 非法值 → None
        assert_eq!(parse_color_val(""), None);
        assert_eq!(parse_color_val("12345"), None);
        assert_eq!(parse_color_val("1234567"), None);
        assert_eq!(parse_color_val("GGGGGG"), None);
        assert_eq!(parse_color_val("#FF0000"), None);
        assert_eq!(parse_color_val("+12345"), None);
    }

    #[test]
    fn parses_run_styles_and_hyperlink_defaults() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r>
                <w:rPr>
                  <w:b w:val="true"/>
                  <w:i w:val="1"/>
                  <w:u w:val="single"/>
                  <w:strike/>
                  <w:color w:val="FF0000"/>
                </w:rPr>
                <w:t>紅粗斜底刪</w:t>
              </w:r>
              <w:r>
                <w:rPr>
                  <w:b w:val="0"/>
                  <w:i w:val="false"/>
                  <w:u w:val="none"/>
                  <w:strike w:val="off"/>
                  <w:color w:val="auto"/>
                </w:rPr>
                <w:t>全關與自動色</w:t>
              </w:r>
              <w:hyperlink>
                <w:r>
                  <w:t>預設超連結</w:t>
                </w:r>
                <w:r>
                  <w:rPr>
                    <w:color w:val="00FF00"/>
                    <w:u w:val="none"/>
                  </w:rPr>
                  <w:t>覆寫超連結</w:t>
                </w:r>
              </w:hyperlink>
            </w:p>
          </w:body>
        </w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析樣式 XML 失敗");
        assert_eq!(paras.len(), 1);
        let runs = &paras[0].runs;
        assert_eq!(runs.len(), 4);

        assert!(runs[0].bold && runs[0].italic && runs[0].underline && runs[0].strike);
        assert_eq!(runs[0].color, Some(0xFFFF_0000));

        assert!(!runs[1].bold && !runs[1].italic && !runs[1].underline && !runs[1].strike);
        assert_eq!(runs[1].color, None);

        assert_eq!(runs[2].color, Some(HYPERLINK_DEFAULT_COLOR_ARGB));
        assert!(runs[2].underline);

        assert_eq!(runs[3].color, Some(0xFF00_FF00));
        assert!(!runs[3].underline);
    }

    // ---------------- 強制分頁 br-page（t_01fa57af） ----------------

    /// br-page 記錄：段首（o=0）／段中（o=碼點偏移）／段尾（o=段長）三口徑，
    /// 一併驗證文字不被 br 吞、多 br 依序記錄。
    #[test]
    fn br_page_records_codepoint_offsets() {
        let xml = r#"<w:document xmlns:w="x">
            <w:body>
            <w:p>
              <w:r><w:br w:type="page"/></w:r>
              <w:r><w:t>ab</w:t></w:r>
              <w:r><w:br w:type="page"/></w:r>
              <w:r><w:t>cd</w:t></w:r>
              <w:r><w:br w:type="page"/></w:r>
            </w:p>
            </w:body></w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析");
        assert_eq!(paras.len(), 1);
        let text: String = paras[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(text, "abcd", "br 不得吞文字");
        assert_eq!(
            paras[0].page_breaks,
            vec![0, 2, 4],
            "段首/段中/段尾三口徑偏移"
        );
    }

    /// 非頁斷點一律不記：column、缺省（textWrapping）、換行；渲染提示
    /// `w:lastRenderedPageBreak`（t_01fa57ac 教訓）與欄內斷點同理無效。
    #[test]
    fn only_page_type_counts_as_break() {
        let xml = r#"<w:document xmlns:w="x">
            <w:body>
            <w:p>
              <w:r><w:t>x</w:t><w:br w:type="column"/><w:br/><w:br w:type="line"/></w:r>
              <w:r><w:lastRenderedPageBreak/><w:t>y</w:t></w:r>
            </w:p>
            </w:body></w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析");
        assert!(paras[0].page_breaks.is_empty(), "非 page 型別/渲染提示不記");
        let text: String = paras[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(text, "xy");
    }

    /// 成對標籤 `<w:br></w:br>`（合法罕見寫法）同樣記錄且**不雙計**；
    /// 無配對段落的游离 br（畸形）老實忽略、不炸。
    #[test]
    fn paired_br_tag_recorded_once() {
        let xml = r#"<w:document xmlns:w="x">
            <w:body>
            <w:p><w:r><w:t>ab</w:t><w:br w:type="page"></w:br></w:r></w:p>
            <w:br w:type="page"/>
            </w:body></w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析");
        assert_eq!(paras[0].page_breaks, vec![2], "成對標籤記錄一次");
    }

    /// 碼點標尺：中文與 emoji（surrogate pair）以**碼點**計，非 UTF-8 位元組。
    #[test]
    fn break_offset_counts_codepoints_not_bytes() {
        let xml = r#"<w:document xmlns:w="x">
            <w:body>
            <w:p><w:r><w:t>中文🎉</w:t><w:br w:type="page"/><w:t>後</w:t></w:r></w:p>
            </w:body></w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析");
        assert_eq!(paras[0].page_breaks, vec![3], "中文字 2＋emoji 碼點 1＝3");
    }
}
