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

mod chart;
mod diff;
mod drawing;
mod emf_dib;
mod notes;
mod numbering;
mod png_duotone;
pub mod preset;
mod qx;
mod rels;
mod sectpr;
mod styles;
mod table;
mod theme;
mod zip_struct;

pub use chart::{
    parse_chart, read_charts_from_docx, read_charts_from_parts, scan_chart_mounts, ChartAxis,
    ChartDoc, ChartMount, ChartSeries,
};
pub use diff::{diff_untouched, diff_zip_structure, EntryDiff, ZipDiff, ZipStructureError};
pub use drawing::{
    count_paras, origin_of, read_images, read_images_with_theme, DuotoneEffect, InlineChildShape,
    InlineImage, ShapeBorder, TextboxGrow, TxbxDrawing,
};
pub use emf_dib::{emf_dib_png, emf_embedded_dib, emf_embedded_dib_checked, EmfDib, EmfReject};
pub use png_duotone::duotone_png_bytes;
pub use rels::{read_rels, resolve_relationship, Relationship};
pub use sectpr::{
    read_even_and_odd_headers, read_sections_endnote_pr, read_sections_header_footer,
    read_sections_note_pr, read_sections_per_paragraph, SectionGeometry, SectionHeaderFooter,
    SectionSpec, SlotRefs,
};
pub use styles::{
    Align, CondShading, LineSpacing, ParaStyle, RunFonts, RunStyle, StyleTable, VertAlign,
};
pub use table::{
    Block, BodyTable, Border, Borders, CellProps, EffectiveLook, GridCol, Margins, RowProps,
    Shading, Table, TableCell, TableLook, TableProps, TableRow, TblWidth,
};
pub use theme::{apply_shading_transform, theme_slot_for, ThemeColors, ThemeFonts};
pub use zip_struct::{
    Change, ZipBundle, ZipEntry, ZipError, MAX_ENTRY_DECOMPRESSED_SIZE,
    MAX_TOTAL_DECOMPRESSED_SIZE, METHOD_DEFLATE, METHOD_STORE,
};

pub use notes::{
    parse_note_settings, Note, NoteKind, NoteKind2, NoteNumberFormat, NotePos, NotePr, NoteRef,
    NoteRestart, NoteSettings, Notes,
};

pub use numbering::{
    paragraph_numbering_from_parts, parse_numbering, parse_paragraph_numbering, AbstractNum, Level,
    LevelOverride, MultiLevelType, NumInstance, NumberFormat, Numbering, NumberingRef,
};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum OoxmlError {
    #[error("zip 解析失敗: {0}")]
    Zip(#[from] ZipError),
    #[error("找不到 word/document.xml，不是有效的 .docx")]
    MissingDocument,
    #[error("XML 解析失敗: {0}")]
    Xml(String),
}

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
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
    /// run 內 w:tab 的出現次數（0 表示無 tab）。
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub tab_count: u8,
    /// run 內含絕對製表符（`w:ptab`，ECMA-376 §17.3.1.26）：alignment 對齊目標。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ptab: Option<styles::Align>,
    /// 級聯合成的**有效字型族**（`w:rFonts` 逐欄級聯結果；engine 開啟文件
    /// 時烘焙寫入，仿 `size_pt` 的做法；theme 引用已在解析層落地成直接值）。
    /// `None`＝整鏈未宣告任何字型。純派生欄位：顯示列表／SVG 選族消費它，
    /// 但任何測量／分頁不消費（版面幾何不變）；存檔路徑完全不經它（parts
    /// passthrough），serde default 保舊快照相容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_fonts: Option<RunFonts>,
    /// 引用標記（`w:footnoteReference`／`w:endnoteReference`；ECMA-376
    /// §17.11.2）：解析期把這個 run 標成註腳／尾註的**引用錨點**（Word
    /// 產出恆為專屬空文字 run，語義仿 `field` 標記——發射層依標記放
    /// 符號，同一 run 的型別／樣式欄位照舊）。本卡渲染零消費；存檔不
    /// 經此欄（parts passthrough），serde default＋skip 保舊快照相容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_ref: Option<NoteRef>,
    /// 本 run 位於 `w:hyperlink` 內（解析期標記）。合成預設藍／底線**不再**
    /// 進 `direct`——那是直接格式，會依 ECMA-376 §17.7.2 越過用戶的
    /// Hyperlink 字元樣式；改由級聯末端（`RunStyle::with_hyperlink_default`）
    /// 只在整鏈未宣告顏色／底線時兜底。存檔不經此欄位（parts passthrough）。
    #[serde(default)]
    pub hyperlink_default: bool,
    /// 本 run 位於 `w:ins` 子樹（插入側修訂，ECMA-376 §17.13.5.19）。默認
    /// 視圖（LO showChanges 實測口徑，K5-diag §2.3 最終判讀）照發文字＋
    /// **底線註解**。不進 `direct`（同 `hyperlink_default` 的冒充教訓——
    /// 註解不是直接格式，不得越過用戶字元樣式）；由級聯末端 OR 進發射
    /// flags。存檔不經此欄位（parts passthrough）。
    #[serde(default, skip_serializing_if = "is_false")]
    pub rev_insert: bool,
    /// 本 run 位於 `w:del` 子樹：文字取自 `w:delText`（§17.13.5.5），默認
    /// 視圖照發＋**刪除線註解**。K5-diag §2.3：LO 默認視圖 del+ins 都發
    /// （'Chromatpgrams' LO PDF ×1 帶刪除線）；舊實作整段吞＝「delText 被
    /// ins 吞」不一致由來。存檔不經此欄位（parts passthrough）。
    #[serde(default, skip_serializing_if = "is_false")]
    pub rev_delete: bool,
    /// run 內文字背景高亮／底色（`w:highlight` 或 `w:shd`；0xAARRGGBB）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<u32>,
    /// 所屬段落的對齊宣告（`w:pPr/w:jc`；供框內段落排版消費）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub para_align: Option<styles::Align>,
    /// 所屬段落的縮進（pt，`w:pPr/w:ind/@w:left + @w:firstLine - @w:hanging`；供框內段落排版消費）。
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub para_indent_pt: f32,
    /// 上標／下標宣告（`w:vertAlign`；供發射層字級縮放與基線偏移消費）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vert_align: Option<styles::VertAlign>,
}

fn is_zero_f32(v: &f32) -> bool {
    v.abs() < f32::EPSILON
}

/// `skip_serializing_if` 用：布爾欄位為 false 時不外露，保既有序列化輸出
/// 逐位元組不變（金線）。
fn is_false(v: &bool) -> bool {
    !*v
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
/// 且只比對**首個指令 token**——ECMA-376 §17.5：指令第一個 token 就是欄位
/// 名，其餘是參數與開關。全 token 掃描會讓 `{ IF PAGE > 1 ... }` 或
/// `HYPERLINK ".../page/1"` 誤判成頁碼欄位並覆寫顯示文字（t_c1fb4c69 S2-6）。
/// 指令可帶開關（如 ` PAGE  \* MERGEFORMAT `），以非英数字元切 token。
pub fn classify_field_instr(instr: &str) -> Option<FieldKind> {
    let upper = instr.to_ascii_uppercase();
    let first = upper
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|t| !t.is_empty())?;
    match first {
        "PAGE" => Some(FieldKind::Page),
        "NUMPAGES" => Some(FieldKind::NumPages),
        _ => None,
    }
}

/// 超連結內 run 無顯式顏色時的預設藍色（Word Hyperlink 樣式標準色 #0563C1）。
pub const HYPERLINK_DEFAULT_COLOR_ARGB: u32 = 0xFF05_63C1;

/// 解析 OOXML `<w:color w:val="..."/>` 色值字串為 `0xAARRGGBB`。
///
/// - 6 位 hex（`RRGGBB`）→ 補齊不透明 alpha `0xFF00_0000 | rgb`；
/// - 8 位 hex（`AARRGGBB`）→ 直接解析為 `u32`；
/// - `"auto"`（不分大小寫）或非法字串 → `None`（發射至顯示列表時轉為黑 `0xFF00_0000`）。
/// - `w:themeColor`（主題色）**優先於** `w:val`：Word/LO 實際渲染以主題色為準，
///   `w:val` 只是回退值（ECMA-376 §17.3.2.25）。查表見 [`theme`] 模組
///   （`w:themeColor` → theme1.xml 的 `a:clrScheme` 槽位）；查不到（無 theme
///   part／查無該名）→ 老實退回 `w:val`（誠實近似，不虛構）。
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
    /// 段落直接段前距（pt，`w:pPr/w:spacing/@w:before`）；未宣告＝None。
    /// `Some(0.0)`＝明確宣告零（與未宣告區分，零變化守門依據）。
    #[serde(default)]
    pub spacing_before_pt: Option<f32>,
    /// 段落直接段後距（pt，`w:pPr/w:spacing/@w:after`）；未宣告＝None。
    #[serde(default)]
    pub spacing_after_pt: Option<f32>,
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
    /// 首個居中製表位（`w:pPr/w:tabs/w:tab/@w:pos`，pt；val="center"）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_center_pt: Option<f32>,
    /// 該製表位導引字元（`w:tab/@w:leader` 原值：dot/hyphen/underscore/…）。
    #[serde(default)]
    pub tab_leader: Option<String>,
    /// 掛在本段落上的**文字框內容**（`w:drawing`／`w:pict` 內 `w:txbxContent`
    /// 的各段落文字，按框內段落序）。K4e2：框內段落**不再佔正文槽位**（四
    /// 解析器同步跳過，見 `table::count_flat_paras`／`drawing::count_paras`／
    /// `sectpr`），改掛此附屬欄，由發射層落在 shape 佔位框座標上。存檔不經
    /// 此欄（parts passthrough），serde default 保舊快照相容；空表不外露
    /// （無框檔案的序列化位元組逐位不變）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub txbx_texts: Vec<String>,
    /// 框內段落的 run 序列（與 `txbx_texts` 逐段對齊；保留 rPr 直接格式——
    /// 發射層字級／粗斜／字型族要用，HC031 的 majorHAnsi 實據）。**不進
    /// 序列化**（skip）：純派生附屬資料，存檔與舊快照位元組逐位不變。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub txbx_runs: Vec<Vec<Run>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OoxmlDoc {
    pub paragraphs: Vec<Paragraph>,
    /// body 級表格的 canonical 投影（只讀；寫回來源仍是 parts 明文，
    /// 見 [`table`] 模組頭「唯讀來源隔離」）。段落以索引引用 `paragraphs`。
    pub tables: Vec<BodyTable>,
    /// 註腳／尾註本體彙總（`word/footnotes.xml`＋`word/endnotes.xml` 一次性
    /// 解析；ECMA-376 §17.11）。純讀取投影，**本卡版面／顯示列表零消費**；
    /// 缺 part＝空彙總（容錯口徑同 main 主題表）。存檔不經此欄（parts
    /// passthrough），serde default 保舊快照相容。
    #[serde(default)]
    pub notes: Notes,
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
    let parts = bundle.decode_parts()?;
    let xml = parts
        .get("word/document.xml")
        .ok_or(OoxmlError::MissingDocument)?;
    // 主題色表（theme1.xml 的 clrScheme）：純讀取，供 `w:themeColor` 查表
    // （ECMA-376 §17.3.2.25——主題色優先於 `w:val` 回退值）。無 theme part
    // → 空表（呼叫端老實退回 `w:val`，既有行為不變）。
    let theme = ThemeColors::from_parts(&parts);
    // 主題字型表（theme1.xml 的 fontScheme）：與主題色表同處取得，供
    // `w:rFonts` 的 theme 引用在解析當下查表落地（ECMA-376 §17.3.2.26）。
    // 無 theme part → 四欄全 None＝保留直接值（既有行為不變）。
    let theme_fonts = ThemeFonts::from_parts(&parts);
    let paragraphs = parse_document_xml_with_fonts(xml, &theme, Some(&theme_fonts))?;
    // 表格投影：純讀入（PM 補正 1——唯讀來源隔離；無損寫回路徑完全不經此处）。
    let tables = table::read_body_tables(xml);
    // 註腳／尾註彙總（ECMA-376 §17.11）：純讀取；缺 part／壞 XML＝空彙總。
    let notes = Notes::from_parts(&parts);
    Ok(OoxmlDoc {
        paragraphs,
        tables,
        notes,
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
///
/// `theme`＝文件的主題色表（`w:themeColor` 查表用；與正文同一份）。
pub fn parse_header_footer_part(
    xml: &[u8],
    theme: &ThemeColors,
) -> Result<(Vec<Paragraph>, Vec<BodyTable>), OoxmlError> {
    parse_header_footer_part_with_fonts(xml, theme, None)
}

/// [`parse_header_footer_part`] 的 fontScheme 版：`tf`＝文件主題字型表，
/// `w:rFonts` 的 theme 引用在解析當下查表落地（ECMA-376 §17.3.2.26）。
pub fn parse_header_footer_part_with_fonts(
    xml: &[u8],
    theme: &ThemeColors,
    tf: Option<&ThemeFonts>,
) -> Result<(Vec<Paragraph>, Vec<BodyTable>), OoxmlError> {
    let paragraphs = parse_document_xml_with_fonts(xml, theme, tf)?;
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
            if let Some(r) = cur_run.as_mut() {
                // run 內 w:tab：run 級標記（無文字產出；投影層展開）。
                r.tab = true;
                r.tab_count = r.tab_count.saturating_add(1);
            } else if ppr_depth == 1 && ppr_rpr_depth == 0 {
                let val = attr_val(e, "val");
                if let Some(pt) = attr_val(e, "pos")
                    .as_deref()
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .filter(|v| v.is_finite() && v.abs() <= styles::MAX_OOXML_LONG)
                    .map(|v| (v / 20.0) as f32)
                    .filter(|v| v.is_finite())
                {
                    match val.as_deref() {
                        Some("right") if p.tab_right_pt.is_none() => {
                            p.tab_right_pt = Some(pt);
                            p.tab_leader = attr_val(e, "leader");
                        }
                        Some("center") if p.tab_center_pt.is_none() => {
                            p.tab_center_pt = Some(pt);
                        }
                        _ => {}
                    }
                }
            }
        }
        b"ptab" => {
            if let Some(r) = cur_run.as_mut() {
                r.tab = true;
                let align_str = attr_val(e, "alignment");
                r.ptab = styles::align_of(align_str.as_deref());
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
    theme: &ThemeColors,
    tf: Option<&ThemeFonts>,
) {
    // ---- run 直接格式三態快照（t_5dfc8ac7）：語義唯一來源＝styles::apply_run_tag。
    // bool 欄位照舊寫（既有消費者不動），三態快照並行記錄給級聯層。只認
    // run 屬性標籤本身——catch-all 送來的無關標籤（noProof/lang…）不得把
    // direct 材料化成 Some（None＝「未見任何 rPr 宣告」的語義必須守住）。
    // `w:rFonts`（字型族）另有一道閘：空元素（無任何族別屬性）不材料化
    // direct——否則「未見任何 rPr 宣告」的 None 語義會被一個空宣告破掉。
    if matches!(
        name,
        b"b" | b"i" | b"u" | b"strike" | b"color" | b"sz" | b"highlight" | b"shd" | b"vertAlign"
    ) || (name == b"rFonts" && styles::rfonts_of(e, tf).is_some())
    {
        if let Some(r) = cur_run.as_mut() {
            styles::apply_run_tag(
                name,
                e,
                r.direct.get_or_insert_with(RunStyle::default),
                theme,
                tf,
            );
        }
    }
    match name {
        b"vertAlign" => {
            if let Some(r) = cur_run.as_mut() {
                if let Some(v) = attr_val(e, "val") {
                    match v.trim() {
                        "superscript" => r.vert_align = Some(styles::VertAlign::Superscript),
                        "subscript" => r.vert_align = Some(styles::VertAlign::Subscript),
                        "baseline" => r.vert_align = Some(styles::VertAlign::Baseline),
                        _ => {}
                    }
                }
            }
        }
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
                // 有效色：主題色（w:themeColor → theme1.xml 查表）**優先於**
                // w:val 回退值（ECMA-376 §17.3.2.25）；查不到 → 退回 w:val
                // （誠實近似，不虛構）。auto/非法 → None（發射層的黑）。
                r.color = theme::resolve_color(theme, e);
            }
        }
        b"highlight" => {
            if let Some(r) = cur_run.as_mut() {
                if let Some(v) = attr_val(e, "val") {
                    r.highlight = styles::highlight_color_of(&v);
                }
            }
        }
        b"shd" => {
            if let Some(r) = cur_run.as_mut() {
                if let Some(v) = attr_val(e, "fill") {
                    if v != "auto" && v != "none" {
                        if let Some(c) = parse_color_val(&v) {
                            r.highlight = Some(c);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// 最小 word/document.xml 解析器：段落 / run / 文字 / 粗斜體 / 顏色 / 底線 / 刪除線 / 超連結 / 段落樣式。
///
/// `theme`＝文件的主題色表（`w:themeColor` 查表用；純讀取，無 theme part
/// 時為空表＝退回 `w:val` 回退值）。
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
fn parse_document_xml(xml: &[u8], theme: &ThemeColors) -> Result<Vec<Paragraph>, OoxmlError> {
    parse_document_xml_with_fonts(xml, theme, None)
}

/// [`parse_document_xml`] 的 fontScheme 版：`tf`＝文件主題字型表，
/// `w:rFonts` 的 theme 引用在解析當下查表落地（ECMA-376 §17.3.2.26）。
fn parse_document_xml_with_fonts(
    xml: &[u8],
    theme: &ThemeColors,
    tf: Option<&ThemeFonts>,
) -> Result<Vec<Paragraph>, OoxmlError> {
    use quick_xml::events::Event;
    let text = String::from_utf8_lossy(xml).into_owned();
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut out: Vec<Paragraph> = Vec::new();
    // 段落棧：(out 槽位, 半成品, 已入文碼點數)。Start push、End pop 回填——
    // 巢狀 w:p 各自獨立，內層完結不影響外層（舊實作單一 cur_para 會被內層
    // 覆寫＋清空，外層文字全吞；corpus 實測 25.5% 檔案受害）。
    let mut stack: Vec<(usize, Paragraph, usize)> = Vec::new();
    let mut cur_run: Option<Run> = None;
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
    // ---- K4e2：`w:txbxContent` 文字框子樹（巢狀深度；框中框罕見但必須配對）----
    // 深度 >0 時進入「附屬欄模式」：`w:p` **不預位**（口徑鐵律改寫點，四解析器
    // 同步——見 table.rs／drawing.rs／sectpr.rs），框內段落只取文字收進
    // `txbx_cur`，完結掛到外層段落的 `Paragraph::txbx_texts`；樣式／分頁／欄位
    // 在框內一律不消費（發射層按 shape 佔位框繪製，見 engine-api）。
    let mut txbx_depth: usize = 0;
    // 外層段落槽位（首層進入時快照；巢狀框共用同一宿主）。
    let mut txbx_owner: usize = usize::MAX;
    // 本層已完結的框內段落文字（按框內段落序）。
    let mut txbx_done: Vec<String> = Vec::new();
    // 進行中的框內段落文字。
    let mut txbx_cur: String = String::new();
    // 框內 `w:p` 開閉深度（Start/End 配對；完結時 flush `txbx_cur`）。
    let mut txbx_para_depth: usize = 0;
    // 框內 `w:t` 文字區間旗標（獨立於正文 `in_t`：兩者永不重疊）。
    let mut txbx_in_t = false;
    // 框內 run 採集（保留 rPr 直接格式供發射層；序與 txbx_done/cur 對齊）。
    let mut txbx_run: Option<Run> = None;
    let mut txbx_para_runs: Vec<Run> = Vec::new();
    let mut txbx_runs_done: Vec<Vec<Run>> = Vec::new();
    // 框內 pPr 子樹（段落標記樣式：忽略，不灌 run）＋其內 rPr 深度。
    let mut txbx_ppr_depth: usize = 0;
    let mut txbx_rpr_depth: usize = 0;
    // 框內段落對齊與縮進緩衝（供框內 run 攜帶段級排版屬性）。
    let mut txbx_cur_jc: Option<styles::Align> = None;
    let mut txbx_cur_ind_pt: f32 = 0.0;
    // ---- 欄位（ECMA-376 §17.5）：只標記 PAGE／NUMPAGES 結果 run ----
    // 簡單欄 `<w:fldSimple w:instr="…">` 巢狀棧（棧頂 None＝非頁碼欄位）。
    let mut fld_simple_stack: Vec<Option<FieldKind>> = Vec::new();
    // 複雜欄 `fldChar begin→instrText→separate→結果 run→end`：每層記
    // （已辨識型別, 是否已過 separate, 本層指令文字緩衝）。separate 之後的
    // run 才是欄位結果（begin..separate 區間只有指令 run，不標記）。
    // 指令緩衝**逐層持有**（t_c1fb4c69 S2-6）：全域單緩衝在巢套域會串味——
    // 內層 begin 清空外層已累積指令、內層指令又混進外層 separate 的分類。
    let mut complex_stack: Vec<(Option<FieldKind>, bool, String)> = Vec::new();
    // ---- K4e3：修訂標記容器（ECMA-376 §17.13；LO 默認視圖口徑＝變更照畫）----
    // 四容器各自計深（本卡實測 RA001 LO PDF 逐串核驗，2026-10-25）：
    //   `w:ins` 子樹＝插入側（照發＋底線）；`w:del` 內 `w:delText`＝刪除側
    //   （照發＋刪除線，實據：'Reference ChromatpgramsChromatograms:' LO 照畫
    //   刪除線原文 ×1）；`w:moveFrom`＝移出源（照發＋刪除線）；`w:moveTo`＝
    //   移入目的地（照發＋底線）。注意：K5-diag §2.3 曾判「LO 整棵不畫 move」，
    //   本卡用同一支 soffice 重新轉換直掃 PDF 推翻——'der of injectio' LO ×4
    //   （plain w:t 僅 ×2，其餘正是 moveFrom 的 w:t 'der of injectio' 與
    //   moveTo 的 delText 'der of injection:'），且 'Order of injection' 源檔
    //   無任何整串 w:t，LO ×4 全來自 move 兩側拼接——move 內容照畫，抑制會
    //   把 LO 有的字弄丟。舊實作把 delText 整段丟棄（只有 `w:t` 進文字流）＝
    //   「delText 被 ins 吞」不一致由來（K5-diag §2.3 真缺陷 (a)）。
    // 只動 run 採集與標記：不預位、不計段（五台 p_count 鐵律）。
    let mut rev_ins_depth: usize = 0;
    let mut rev_del_depth: usize = 0;
    let mut rev_mfrom_depth: usize = 0;
    let mut rev_mto_depth: usize = 0;
    // `w:delText` 文字區間（`w:t` 的刪除側鏡像；两者永不重疊）。
    let mut in_del_t = false;
    let mut instr_active = false;

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
                // ---- K4e2：文字框子樹內的「附屬欄模式」（先於一切正文邏輯）----
                // 框內只認三件事：巢狀 txbxContent 深度、`w:p` 開閉、`w:t` 文字。
                // 正文槽位、樣式級聯、分頁、欄位一律不消費——框內段落由發射層
                // 按 shape 佔位框繪製（engine-api），不進正文流。
                if txbx_depth > 0 {
                    match name.as_slice() {
                        b"txbxContent" => txbx_depth += 1,
                        b"p" => {
                            txbx_para_depth += 1;
                            // 框內段落起點：run 採集緩衝與段級格式歸零（段內序）。
                            txbx_cur_jc = None;
                            txbx_cur_ind_pt = 0.0;
                            txbx_run = None;
                            txbx_para_runs.clear();
                        }
                        b"t" => txbx_in_t = true,
                        // 框內 run：建採集緩衝（發射層的樣式來源）。
                        b"r" => {
                            txbx_run = Some(Run {
                                para_align: txbx_cur_jc,
                                para_indent_pt: txbx_cur_ind_pt,
                                ..Default::default()
                            });
                        }
                        // pPr 子樹（段落標記）與其內 rPr：標深度並採集段落對齊與縮進。
                        b"pPr" => txbx_ppr_depth += 1,
                        b"jc" if txbx_ppr_depth > 0 => {
                            txbx_cur_jc = styles::align_of(attr_val(&e, "val").as_deref());
                            if let Some(r) = txbx_run.as_mut() {
                                r.para_align = txbx_cur_jc;
                            }
                            for r in &mut txbx_para_runs {
                                r.para_align = txbx_cur_jc;
                            }
                        }
                        b"ind" if txbx_ppr_depth > 0 => {
                            let left =
                                styles::twips_to_pt(attr_val(&e, "left").as_deref()).unwrap_or(0.0);
                            let fl = styles::twips_to_pt(attr_val(&e, "firstLine").as_deref())
                                .unwrap_or(0.0);
                            let hang = styles::twips_to_pt(attr_val(&e, "hanging").as_deref())
                                .unwrap_or(0.0);
                            txbx_cur_ind_pt = left + fl - hang;
                            if let Some(r) = txbx_run.as_mut() {
                                r.para_indent_pt = txbx_cur_ind_pt;
                            }
                            for r in &mut txbx_para_runs {
                                r.para_indent_pt = txbx_cur_ind_pt;
                            }
                        }
                        b"rPr" => {
                            if txbx_ppr_depth > 0 {
                                txbx_ppr_depth += 1;
                            } else {
                                txbx_rpr_depth += 1;
                            }
                        }
                        // run 直接格式標籤（rPr 內、非 pPr 內）→ 灌採集 run。
                        _ if txbx_rpr_depth > 0 && txbx_ppr_depth == 0 => {
                            apply_style_tag(&name, &e, &mut txbx_run, theme, tf);
                        }
                        b"tab" if txbx_ppr_depth == 0 => {
                            if let Some(r) = txbx_run.as_mut() {
                                r.tab = true;
                                r.tab_count = r.tab_count.saturating_add(1);
                            }
                        }
                        // 其餘框內標籤（pStyle／lang／noProof…）一概不消費。
                        _ => {}
                    }
                    continue;
                }
                match name.as_slice() {
                    b"txbxContent" => {
                        // 進入文字框：快照宿主段落（正文棧頂；無宿主＝body 級
                        // 游离框，老實不收，正文零污染）。
                        txbx_owner = stack.last().map(|(slot, _, _)| *slot).unwrap_or(usize::MAX);
                        txbx_depth = 1;
                    }
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
                    b"ins" if ppr_depth == 0 => {
                        // 插入容器（段落級；rPr 內的空 `<w:ins/>` 段落標記宣告走
                        // Empty 臂、不進此臂）。默認視圖照發＋底線（LO 實據）。
                        rev_ins_depth += 1;
                    }
                    b"del" if ppr_depth == 0 => {
                        // 刪除容器（同上；其內 `w:delText` 默認視圖照發＋刪除線）。
                        rev_del_depth += 1;
                    }
                    b"delText" if ppr_depth == 0 => {
                        // 刪除側文字區間（`w:t` 的鏡像；ECMA-376 §17.13.5.5）。只認
                        // `w:del`／`w:moveFrom` 容器內的（容器外的 delText＝畸形，
                        // 不採——LO 亦不畫無容器的孤兒 delText）。
                        if rev_del_depth > 0 || rev_mfrom_depth > 0 {
                            in_del_t = true;
                        }
                    }
                    b"moveFrom" if ppr_depth == 0 => {
                        // 移出源（ECMA §17.13.5.24）：LO 默認視圖照畫＋刪除線
                        // （見上：'Order of injection:' LO ×4 全含 move 兩側拼接）。
                        rev_mfrom_depth += 1;
                    }
                    b"moveTo" if ppr_depth == 0 => {
                        // 移入目的地（§17.13.5.23）：照畫＋底線（插入側同義）。
                        rev_mto_depth += 1;
                    }
                    b"hyperlink" => {
                        hyperlink_depth += 1;
                    }
                    b"br" => {
                        // 成對標籤寫法 `<w:br></w:br>`（合法但罕見）的入口：
                        // Start/Empty 互斥，自閉合不會重入此臂、零雙計。
                        // 依 ECMA-376 §17.4.1 / §17.4.3 分流：type="page" 記硬分頁，
                        // 缺省/textWrapping 記軟性換行（\n）。
                        handle_br_tag(&mut stack, &mut cur_run, &e);
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
                            if let Some(b) = styles::space_pt(attr_val(&e, "before").as_deref()) {
                                p.spacing_before_pt = Some(b);
                            }
                            if let Some(a) = styles::space_pt(attr_val(&e, "after").as_deref()) {
                                p.spacing_after_pt = Some(a);
                            }
                        }
                    }
                    b"jc" if ppr_depth == 1 && ppr_rpr_depth == 0 => {
                        if let Some((_, p, _)) = stack.last_mut() {
                            p.jc = styles::align_of(attr_val(&e, "val").as_deref());
                        }
                    }
                    b"framePr" if ppr_depth == 1 && ppr_rpr_depth == 0 => {
                        if let Some((_, p, _)) = stack.last_mut() {
                            if p.jc.is_none() {
                                p.jc = styles::align_of(attr_val(&e, "xAlign").as_deref());
                            }
                        }
                    }
                    // 顯式分頁開關＋製表位（t_5dfc8ac7）：w:br 已由上方細分臂
                    // 攔截，此臂收 pPr/w:pageBreakBefore 與 w:tabs/w:tab / w:ptab。
                    b"pageBreakBefore" | b"tab" | b"ptab" if ppr_rpr_depth == 0 => {
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
                                theme,
                                tf,
                            );
                        }
                    }
                    // pPr/rPr 內的字型族宣告（w:rFonts；ECMA-376 §17.3.2.26）：
                    // 收進段落棧頂的三態快照（段落級級聯輸入）。空元素不得
                    // 物化 prun（守住「未見任何 rPr 宣告＝None」的語義）。
                    b"rFonts" if ppr_rpr_depth > 0 => {
                        if styles::rfonts_of(&e, tf).is_some() {
                            if let Some((_, p, _)) = stack.last_mut() {
                                styles::apply_run_tag(
                                    name.as_slice(),
                                    &e,
                                    p.prun.get_or_insert_with(RunStyle::default),
                                    theme,
                                    tf,
                                );
                            }
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
                        // 欄位結果 run 標記：簡單欄子樹，或複雜欄 separate 之後。
                        let kind = match fld_simple_stack.last() {
                            Some(k) => *k,
                            None => match complex_stack.last() {
                                Some((k, true, _)) => *k,
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
                    // 註引用標記（ECMA-376 §17.11.2）：成對寫法入口
                    // （`<w:footnoteReference w:id="N"></w:footnoteReference>`
                    // 合法但罕見；Word 產出恆為自閉合，由 Empty 臂收）。
                    b"footnoteReference" | b"endnoteReference" => {
                        record_note_ref(name.as_slice(), &e, &mut cur_run);
                    }
                    other => apply_style_tag(other, &e, &mut cur_run, theme, tf),
                }
            }
            Event::Empty(e) => {
                if fb > 0 {
                    continue;
                }
                if txbx_depth > 0 {
                    // 框內自閉合標籤：
                    // - pPr 內：採集段落對齊（w:jc）與縮進（w:ind）。
                    // - rPr 內：字型族與樣式（w:rFonts／w:sz／w:color／w:b／w:highlight／w:shd…）進採集 run。
                    // - 正文區：非 rPr 內且非 pPr 內的 w:tab 標記為 run 級製表符。
                    // 其餘（pStyle／br…）一概不消費：正文段落、run、分頁不受框內污染。
                    let name = e.local_name().as_ref().to_vec();
                    if txbx_ppr_depth > 0 {
                        if name.as_slice() == b"jc" {
                            txbx_cur_jc = styles::align_of(attr_val(&e, "val").as_deref());
                            if let Some(r) = txbx_run.as_mut() {
                                r.para_align = txbx_cur_jc;
                            }
                            for r in &mut txbx_para_runs {
                                r.para_align = txbx_cur_jc;
                            }
                        } else if name.as_slice() == b"ind" {
                            let left =
                                styles::twips_to_pt(attr_val(&e, "left").as_deref()).unwrap_or(0.0);
                            let fl = styles::twips_to_pt(attr_val(&e, "firstLine").as_deref())
                                .unwrap_or(0.0);
                            let hang = styles::twips_to_pt(attr_val(&e, "hanging").as_deref())
                                .unwrap_or(0.0);
                            txbx_cur_ind_pt = left + fl - hang;
                            if let Some(r) = txbx_run.as_mut() {
                                r.para_indent_pt = txbx_cur_ind_pt;
                            }
                            for r in &mut txbx_para_runs {
                                r.para_indent_pt = txbx_cur_ind_pt;
                            }
                        }
                    } else if txbx_rpr_depth > 0 {
                        if matches!(
                            name.as_slice(),
                            b"rFonts"
                                | b"sz"
                                | b"color"
                                | b"b"
                                | b"i"
                                | b"u"
                                | b"highlight"
                                | b"shd"
                                | b"vertAlign"
                        ) {
                            apply_style_tag(&name, &e, &mut txbx_run, theme, tf);
                        }
                    } else if name.as_slice() == b"tab" {
                        let r = txbx_run.get_or_insert_with(Run::default);
                        r.tab = true;
                        r.tab_count = r.tab_count.saturating_add(1);
                    }
                    continue;
                }
                let name = e.local_name().as_ref().to_vec();
                if name == b"br" {
                    // 分頁與換行（ECMA-376 §17.4.1 / §17.4.3）：
                    // type="page" 記硬分頁；column／缺省（textWrapping）分流處理。
                    // `w:lastRenderedPageBreak` 是另一元素，永不進此臂（渲染提示≠斷點）。
                    handle_br_tag(&mut stack, &mut cur_run, &e);
                } else if name == b"fldChar" {
                    // 複雜欄三態（Word 產出恒為自閉合標籤）：begin 開層、
                    // separate 翻結果標記、end 出層。未配對（漏 begin）老實無效。
                    match attr_val(&e, "fldCharType").as_deref() {
                        Some("begin") => {
                            // 本層自備指令緩衝（S2-6）：不再清空「全域」緩衝
                            // ——那會把外層已累積的指令一併抹掉。
                            complex_stack.push((None, false, String::new()));
                        }
                        Some("separate") => {
                            if let Some(top) = complex_stack.last_mut() {
                                if top.0.is_none() {
                                    // take 出本層緩衝再分類（迴避 top 的重複借用），
                                    // 分類後緩衝已無用、留在原地丟棄即可。
                                    let instr = std::mem::take(&mut top.2);
                                    top.0 = classify_field_instr(&instr);
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
                } else if matches!(
                    name.as_slice(),
                    b"b" | b"i" | b"u" | b"strike" | b"color" | b"sz" | b"rStyle"
                ) && ppr_rpr_depth > 0
                {
                    // pPr/rPr 內自閉合 run 格式標籤（Word 產出恆為 `<w:b/>` 形）：
                    // Start 臂的鏡像（t_5b391cfd S1）。缺本臂時它們落進下方
                    // catch-all apply_style_tag，彼時 cur_run==None（pPr 在 run 群
                    // 之前）→ 靜默丟棄，段落級級聯直接退回樣式鏈——同義兩制：
                    // 成對標籤收、自閉合丟，且 styles.rs 樣式層 parse_ppr 對
                    // 自閉合是收的（in_rpr 態 apply_run_tag），內部不一致。
                    // rStyle 落 apply_run_tag 為明確的無操作（RunStyle 不承 rstyle
                    // 引用），與 Start 臂同形，零雙標。
                    if let Some((_, p, _)) = stack.last_mut() {
                        styles::apply_run_tag(
                            name.as_slice(),
                            &e,
                            p.prun.get_or_insert_with(RunStyle::default),
                            theme,
                            tf,
                        );
                    }
                } else if name == b"rFonts" && ppr_rpr_depth > 0 {
                    // pPr/rPr 內自閉合的字型族宣告（Start 臂的鏡像）。空元素
                    // 不物化 prun（守住「未見任何 rPr 宣告＝None」的語義）。
                    if styles::rfonts_of(&e, tf).is_some() {
                        if let Some((_, p, _)) = stack.last_mut() {
                            styles::apply_run_tag(
                                name.as_slice(),
                                &e,
                                p.prun.get_or_insert_with(RunStyle::default),
                                theme,
                                tf,
                            );
                        }
                    }
                } else if name == b"spacing" && ppr_depth == 1 && ppr_rpr_depth == 0 {
                    // 自閉合直接行距（Word 產出恆為自閉合）。
                    if let Some((_, p, _)) = stack.last_mut() {
                        p.line = styles::line_spacing(
                            attr_val(&e, "line").as_deref(),
                            attr_val(&e, "lineRule").as_deref(),
                        );
                        if let Some(b) = styles::space_pt(attr_val(&e, "before").as_deref()) {
                            p.spacing_before_pt = Some(b);
                        }
                        if let Some(a) = styles::space_pt(attr_val(&e, "after").as_deref()) {
                            p.spacing_after_pt = Some(a);
                        }
                    }
                } else if name == b"jc" && ppr_depth == 1 && ppr_rpr_depth == 0 {
                    if let Some((_, p, _)) = stack.last_mut() {
                        p.jc = styles::align_of(attr_val(&e, "val").as_deref());
                    }
                } else if name == b"framePr" && ppr_depth == 1 && ppr_rpr_depth == 0 {
                    if let Some((_, p, _)) = stack.last_mut() {
                        if p.jc.is_none() {
                            p.jc = styles::align_of(attr_val(&e, "xAlign").as_deref());
                        }
                    }
                } else if matches!(
                    name.as_slice(),
                    b"br" | b"tab" | b"ptab" | b"pageBreakBefore"
                ) && ppr_rpr_depth == 0
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
                } else if matches!(name.as_slice(), b"footnoteReference" | b"endnoteReference") {
                    // 註引用標記（自閉合＝Word 常態寫法）：標進進行中的 run。
                    record_note_ref(name.as_slice(), &e, &mut cur_run);
                } else {
                    apply_style_tag(name.as_slice(), &e, &mut cur_run, theme, tf);
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_vec();
                // MC 標籤完結先行處理（即便在 Fallback 內）：錯置標籤
                // （AC 先於 Fallback 閉合）不致 fb 卡死而吞掉檔尾。
                if name == b"AlternateContent" {
                    fb = ac_stack.pop().unwrap_or(0);
                    // K4e2 防呆：畸形檔（AC 先於 txbxContent 閉合）若放任
                    // txbx_depth 卡死，其後全文文字都會被吞進框內緩衝。老實
                    // 完結當前框（已收文字照常掛到宿主段落），深度歸零。
                    if txbx_depth > 0 {
                        if !txbx_cur.is_empty() || !txbx_para_runs.is_empty() {
                            txbx_done.push(std::mem::take(&mut txbx_cur));
                            txbx_runs_done.push(std::mem::take(&mut txbx_para_runs));
                        }
                        if txbx_owner != usize::MAX {
                            let (texts, runs) =
                                (txbx_done.split_off(0), txbx_runs_done.split_off(0));
                            match stack.iter_mut().find(|(s, _, _)| *s == txbx_owner) {
                                Some((_, p, _)) => attach_txbx(p, texts, runs),
                                None => {
                                    if let Some(p) = out.get_mut(txbx_owner) {
                                        attach_txbx(p, texts, runs);
                                    }
                                }
                            }
                        }
                        txbx_done.clear();
                        txbx_runs_done.clear();
                        txbx_cur.clear();
                        txbx_owner = usize::MAX;
                        txbx_depth = 0;
                        txbx_para_depth = 0;
                    }
                    continue;
                }
                if fb > 0 {
                    if name == b"Fallback" {
                        fb = fb.saturating_sub(1);
                    }
                    continue;
                }
                // ---- K4e2：文字框子樹完結（先於正文邏輯）----
                if txbx_depth > 0 {
                    match name.as_slice() {
                        b"txbxContent" => {
                            txbx_depth -= 1;
                            if txbx_depth == 0 {
                                // 框收束：尾段 flush（框內無 w:p 的直接文字也收），
                                // 整批掛到宿主段落的 txbx_texts。宿主越界（畸形）
                                // ＝老實丟棄，正文零污染。
                                if !txbx_cur.is_empty() || !txbx_para_runs.is_empty() {
                                    txbx_done.push(std::mem::take(&mut txbx_cur));
                                    txbx_runs_done.push(std::mem::take(&mut txbx_para_runs));
                                }
                                if txbx_owner != usize::MAX {
                                    // 掛載目標＝棧上半成品（段落尚未 End 回填）；
                                    // 段落已完結（畸形寫法：框跨段尾）才退到 out。
                                    let (texts, runs) =
                                        (txbx_done.split_off(0), txbx_runs_done.split_off(0));
                                    match stack.iter_mut().find(|(s, _, _)| *s == txbx_owner) {
                                        Some((_, p, _)) => attach_txbx(p, texts, runs),
                                        None => {
                                            if let Some(p) = out.get_mut(txbx_owner) {
                                                attach_txbx(p, texts, runs);
                                            }
                                        }
                                    }
                                    txbx_done.clear();
                                    txbx_runs_done.clear();
                                }
                                txbx_done.clear();
                                txbx_runs_done.clear();
                                txbx_cur.clear();
                                txbx_owner = usize::MAX;
                            }
                        }
                        b"p" => {
                            txbx_para_depth = txbx_para_depth.saturating_sub(1);
                            if txbx_para_depth == 0 {
                                if txbx_para_runs.is_empty() && !txbx_cur.is_empty() {
                                    txbx_para_runs.push(Run {
                                        text: txbx_cur.clone(),
                                        para_align: txbx_cur_jc,
                                        para_indent_pt: txbx_cur_ind_pt,
                                        ..Default::default()
                                    });
                                }
                                // 空段照推（""＋空 runs）：texts 與 runs 逐段對齊。
                                txbx_done.push(std::mem::take(&mut txbx_cur));
                                txbx_runs_done.push(std::mem::take(&mut txbx_para_runs));
                            }
                        }
                        b"t" => txbx_in_t = false,
                        // 框內 run 完結：連同 rPr 快照進段內 run 緩衝。
                        b"r" => {
                            if let Some(mut r) = txbx_run.take() {
                                r.para_align = txbx_cur_jc;
                                r.para_indent_pt = txbx_cur_ind_pt;
                                txbx_para_runs.push(r);
                            }
                        }
                        b"pPr" => txbx_ppr_depth = txbx_ppr_depth.saturating_sub(1),
                        b"rPr" => {
                            if txbx_ppr_depth > 0 {
                                txbx_ppr_depth = txbx_ppr_depth.saturating_sub(1);
                            } else {
                                txbx_rpr_depth = txbx_rpr_depth.saturating_sub(1);
                            }
                        }
                        _ => {}
                    }
                    continue;
                }
                match name.as_slice() {
                    b"t" => in_t = false,
                    b"delText" => in_del_t = false,
                    b"ins" if ppr_depth == 0 => {
                        // End 臂與 Start 臂同閘：pPr/rPr 內的成對段落標記宣告
                        // （罕見寫法）兩側都跳過，深計數不會單邊漂移。
                        rev_ins_depth = rev_ins_depth.saturating_sub(1);
                    }
                    b"del" if ppr_depth == 0 => {
                        rev_del_depth = rev_del_depth.saturating_sub(1);
                    }
                    b"moveFrom" if ppr_depth == 0 => {
                        rev_mfrom_depth = rev_mfrom_depth.saturating_sub(1);
                    }
                    b"moveTo" if ppr_depth == 0 => {
                        rev_mto_depth = rev_mto_depth.saturating_sub(1);
                    }
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
                            // <w:hyperlink> 內 run 只標記來源，不在此合成顏色／
                            // 底線——合成若落 direct 就成了「直接格式」，依
                            // ECMA-376 §17.7.2 會越過用戶 styles.xml 宣告的
                            // Hyperlink 字元樣式。兜底改由級聯末端
                            // （`RunStyle::with_hyperlink_default`）在全鏈未
                            // 宣告時補上。TODO（未支援）：合成預設連結色仍是
                            // 寫死的 #0563C1（Word Hyperlink 樣式標準藍）——
                            // Word 的實際預設連結色來自**主題 hlink**，本卡
                            // 未接（見 theme 模組末「未做清單」）。
                            run.hyperlink_default = hyperlink_depth > 0;
                            // 修訂標記（ECMA §17.13；LO 默認視圖實測口徑）：插入側
                            // （`w:ins`／`w:moveTo`）＝底線、刪除側（`w:del` 容器內
                            // run、`w:moveFrom` 含其 plain w:t）＝刪除線。只打標記、
                            // 不進 direct（冒充直接格式會越過用戶字元樣式——同
                            // hyperlink_default 教訓），發射層級聯末端才 OR 進 flags。
                            run.rev_insert = rev_ins_depth > 0 || rev_mto_depth > 0;
                            run.rev_delete = rev_del_depth > 0 || rev_mfrom_depth > 0;
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
                if txbx_depth > 0 {
                    // 框內文字：只收文字進 txbx_cur（不進正文 run、不動 cp_done）；
                    // 同步進採集 run（發射層按 run 分段取樣式，K4e2）。
                    if txbx_in_t {
                        let s = qx::unescape_text(&t).map_err(OoxmlError::Xml)?;
                        txbx_cur.push_str(&s);
                        if let Some(r) = txbx_run.as_mut() {
                            r.text.push_str(&s);
                        }
                    }
                    continue;
                }
                if instr_active {
                    // 欄位指令文字（可能跨多個 Text 事件）：寫進**最內層**進行中
                    // 的複雜欄緩衝（instrText 必落在其所屬域的 begin..separate
                    // 區間，棧頂就是所屬層；t_c1fb4c69 S2-6 逐層隔離）。
                    if let Ok(s) = qx::unescape_text(&t) {
                        if let Some(top) = complex_stack.last_mut() {
                            top.2.push_str(&s);
                        }
                    }
                    continue;
                }
                if let Some(r) = cur_run.as_mut().filter(|_| in_t || in_del_t) {
                    let s = qx::unescape_text(&t).map_err(OoxmlError::Xml)?;
                    // 已入文碼點計數隨文字增長——br-page 記錄偏移的標尺。
                    if let Some((_, _, cp_done)) = stack.last_mut() {
                        *cp_done += s.chars().count();
                    }
                    r.text.push_str(&s);
                }
            }
            Event::GeneralRef(r) => {
                if fb > 0 {
                    continue;
                }
                let s = qx::resolve_general_ref(&r).map_err(OoxmlError::Xml)?;
                if txbx_depth > 0 {
                    if txbx_in_t {
                        txbx_cur.push_str(&s);
                        if let Some(r) = txbx_run.as_mut() {
                            r.text.push_str(&s);
                        }
                    }
                    continue;
                }
                if instr_active {
                    if let Some(top) = complex_stack.last_mut() {
                        top.2.push_str(&s);
                    }
                    continue;
                }
                if let Some(r) = cur_run.as_mut().filter(|_| in_t || in_del_t) {
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

/// 註引用標記（ECMA-376 §17.11.2）：`w:footnoteReference`／
/// `w:endnoteReference` → 進行中 run 的 [`NoteRef`] 標記（語義仿 `field`
/// 標記：run 本身不改動，只多一枚錨點印記）。取捨：
/// - 缺 `@w:id`／非整數 → **老實跳過此標記**不 panic（無 id 的引用無法
///   對應任何註本體，虛構 id／0 都比略過更錯——同 corpus 病態防護口徑）；
/// - 無進行中 run（段落直下、畸形位置）→ 老實略：憑空造 run 會破壞
///   「run 序＝w:r 序」的佔槽口徑，發射層（F2）也無從定位；
/// - 同 run 多個引用 → **首見為準**（單一錨點語義，後見不覆蓋）；
/// - 引用與 `w:t` 同 run（極罕見、非 Word 產出形態）：標記隨 run 保留、
///   **不分裂 run**——run 序與文字流保持原樣，F2 發射符號時再裁決展開。
fn record_note_ref(name: &[u8], e: &quick_xml::events::BytesStart, cur_run: &mut Option<Run>) {
    let kind = if name == b"footnoteReference" {
        NoteKind2::Footnote
    } else {
        NoteKind2::Endnote
    };
    let id = attr_val(e, "id")
        .as_deref()
        .and_then(|v| v.trim().parse::<i64>().ok());
    if let (Some(id), true) = (id, matches!(cur_run, Some(r) if r.note_ref.is_none())) {
        cur_run.as_mut().expect("剛查 some").note_ref = Some(NoteRef { kind, id });
    }
}

/// K4e2：把框內段落批次掛到宿主段落（`txbx_texts`＋`txbx_runs` 同步附著，
/// 兩者逐段對齊是發射層的取樣前提——段數不一致時多出的 run 老實丟棄、
/// 缺 run 的段落退化为無樣式發射，絕不串段）。
fn attach_txbx(p: &mut Paragraph, texts: Vec<String>, runs: Vec<Vec<Run>>) {
    let n = texts.len();
    p.txbx_texts.extend(texts);
    p.txbx_runs.extend(runs.into_iter().take(n));
}

fn attr_val(e: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key.as_bytes() {
            qx::decode_and_unescape_attr(&a)
        } else {
            None
        }
    })
}

/// 處理 `w:br` 標籤（Start 與 Empty 共通）：
/// 1. `w:type="page"`（ECMA-376 §17.4.3）：硬分頁斷點，記錄碼點偏移至 `Paragraph::page_breaks`。
///    偏移＝段落棧頂「已入文碼點數」：文字一律經 Text 臂先行累計，故 br 前後的文字各歸斷點兩側。
/// 2. 缺省或 `w:type="textWrapping"`（ECMA-376 §17.4.1）：軟換行，在 run 文字流中插入 `\n`。
///
/// 【換行表示法選型與理由】
/// 在「插入換行碼點 `\n`」與「新增換行偏移欄（如 line_breaks: Vec<usize>）」兩者中，本實作選擇【插入換行碼點 `\n`】，理由如下：
/// - 語意自然且高度吻合 UAX#14：換行符號本為文字流實體字元（Word 剪貼簿純文字亦輸出換行）。
///   本引擎 `uax14::break_atoms` 原生將 `\n` 標記為 mandatory 強制斷點（`forced: true`），
///   Knuth-Plass（`kp::optimal_ranges`）在遇 `forced` 時自然不將 state 放入 alive 清單，
///   Active list 嚴格中斷、不可跨越，天然達成強制折行，完全無需在排版層維護第二套平行偏移演算法。
/// - 零架構侵入與向後相容：若新增換行偏移欄，則必須同時侵入 `Paragraph`、`CellInput`、`CellBlock`、
///   `ParagraphLayout`、`table.rs` 等多個核心型別，且極易與 `page_breaks` 混淆而誤觸發分頁；
///   採用 `\n` 則所有下游資料結構與金樣契約保持零變動，且 round-trip 因基於 parts 原文而 100% 保持 byte-identical。
/// - 座標與樣式投影嚴密對齊：`Line::char_offset` 與 `emit_line_run_items` 求交集時，因 `\n` 計入碼點偏移，
///   各行起點與 run 區間嚴密吻合；行尾 `\n` 經 `EOL_CONTROLS` 濾除，不顯示、不佔版面寬度。
fn handle_br_tag(
    stack: &mut [(usize, Paragraph, usize)],
    cur_run: &mut Option<Run>,
    e: &quick_xml::events::BytesStart,
) {
    let br_type = attr_val(e, "type");
    if br_type.as_deref() == Some("page") {
        if let Some((_, p, cp_done)) = stack.last_mut() {
            p.page_breaks.push(*cp_done);
        }
        return;
    }

    // 軟性 br（純換行）：僅在無 type 或 type="textWrapping"（ECMA-376 §17.4.1）時生效。
    // 注意：column 等欄斷點非純換行，不落此臂；`w:lastRenderedPageBreak` 為渲染提示，不進此處。
    if br_type.is_none() || br_type.as_deref() == Some("textWrapping") {
        if std::env::var_os("K21_SOFT_BR_BUGGY").is_some() {
            return; // BUGGY 回塞鎖：關閉軟 br，退回修前行為
        }
        if let Some((_, p, cp_done)) = stack.last_mut() {
            *cp_done += 1;
            if let Some(r) = cur_run.as_mut() {
                r.text.push('\n');
            } else {
                p.runs.push(Run {
                    text: "\n".to_string(),
                    ..Default::default()
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 測試用解析入口：**無 theme part**（空主題表）的文件解析。
    /// 主題色查表另有專屬測試（`theme_color_lookup_*`），此處保持舊行為口徑。
    fn parse_doc(xml: &[u8]) -> Result<Vec<Paragraph>, OoxmlError> {
        parse_document_xml(xml, &ThemeColors::default())
    }

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
        // 首 token 語義（t_c1fb4c69 S2-6 改寫舊斷言）：欄位型別＝指令首個
        // token；「 PAGE NUMPAGES 」這種畸形指令以首個 PAGE 為準（舊「全掃
        // 描 NUMPAGES 優先」語義會把 IF/HYPERLINK 裡的 PAGE 誤判成頁碼）。
        assert_eq!(classify_field_instr(" PAGE NUMPAGES "), Some(Page));
        // 不支援的欄位老實回 None（發射層保留快取文字，不虛構）。
        assert_eq!(classify_field_instr(" SECTIONPAGES "), None);
        assert_eq!(classify_field_instr(" DATE \\@ M "), None);
        assert_eq!(classify_field_instr(""), None);
    }

    /// 首 token 判定（t_c1fb4c69 S2-6）：欄位型別只看**第一個指令 token**，
    /// 全 token 掃描會讓參數／運算式裡的 PAGE 誤判為頁碼欄位並覆寫顯示文字。
    #[test]
    fn field_instr_only_first_token_counts() {
        use FieldKind::{NumPages, Page};
        // IF 條件引用 PAGE：欄位是 IF，不是頁碼——快取顯示文字必須保留。
        assert_eq!(classify_field_instr(r#" IF PAGE > 1 "A" "B" "#), None);
        // HYPERLINK 路徑含 page：不是欄位。
        assert_eq!(
            classify_field_instr(r#" HYPERLINK "https://x/page/1" "#),
            None
        );
        // 引用域引數裡的 NUMPAGES 同理。
        assert_eq!(
            classify_field_instr(r#" IF { NUMPAGES } > 1 "x" "y" "#),
            None
        );
        // 真頁碼欄位（首 token 命中）不受影響；\* MERGEFORMAT 開關在首 token 之後。
        assert_eq!(classify_field_instr(" PAGE \\* MERGEFORMAT "), Some(Page));
        assert_eq!(classify_field_instr("NUMPAGES \\# 0"), Some(NumPages));
    }

    /// 巢套域的 instr_buf 必須逐層隔離（t_c1fb4c69 S2-6）：外層 IF 的指令
    /// 與內層 PAGE 的指令各自獨立——內層分類不得被外層文字污染，外層也不
    /// 得被內層的 PAGE 誤判。
    #[test]
    fn nested_field_instr_is_per_layer() {
        let xml = r#"<w:document xmlns:w="x"><w:body>
            <w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r>
                 <w:r><w:instrText> IF PAGE &gt; 1 </w:instrText></w:r>
                 <w:r><w:fldChar w:fldCharType="begin"/></w:r>
                 <w:r><w:instrText> PAGE </w:instrText></w:r>
                 <w:r><w:fldChar w:fldCharType="separate"/></w:r>
                 <w:r><w:t>7</w:t></w:r>
                 <w:r><w:fldChar w:fldCharType="end"/></w:r>
                 <w:r><w:instrText> "A" "B" </w:instrText></w:r>
                 <w:r><w:fldChar w:fldCharType="separate"/></w:r>
                 <w:r><w:t>甲</w:t></w:r>
                 <w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
            </w:body></w:document>"#;
        let paras = parse_doc(xml.as_bytes()).unwrap();
        let runs = &paras[0].runs;
        let marked: Vec<_> = runs.iter().filter(|r| r.field.is_some()).collect();
        assert_eq!(marked.len(), 1, "只有內層 PAGE 的結果 run 是頁碼欄位");
        assert_eq!(marked[0].field, Some(FieldKind::Page));
        assert_eq!(marked[0].text, "7");
        let jia = runs
            .iter()
            .find(|r| r.text == "甲")
            .expect("外層結果 run 存在");
        assert_eq!(
            jia.field, None,
            "外層 IF 欄位不是頁碼：顯示文字「甲」不得被頁碼覆寫"
        );
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
        let (paras, tables) = parse_header_footer_part(xml.as_bytes(), &ThemeColors::default())
            .expect("解析頁尾 part");
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
            &ThemeColors::default(),
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
        let paras = parse_doc(xml).expect("解析");
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
        let paras = parse_doc(xml).expect("解析");
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
        let paras = parse_doc(xml).expect("解析");
        let texts: Vec<String> = paras
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect();
        // K4e2：框內段落不再佔正文槽——外層段文字完整、框內文掛 txbx_texts
        // 附屬欄（Choice 一份、Fallback 去重口徑不變）。
        assert_eq!(
            texts,
            vec!["框前文框後文"],
            "正文只剩外層段（框內段不佔槽）"
        );
        assert_eq!(
            paras[0].txbx_texts,
            vec!["框內文".to_string()],
            "框內段落文字掛進宿主段落的 txbx_texts（分離但不遺失）"
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
        let paras = parse_doc(xml).expect("解析");
        let texts: Vec<String> = paras
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect();
        assert_eq!(texts, vec!["AD", "B", "C", "E"], "扁平索引＝w:p Start 序");
    }

    /// corpus 段落數回歸（缺陷 1＋2 實據檔）：修前 Choice/Fallback 雙份解析
    /// ＋巢狀 w:p 覆寫吞字（舊口徑 LINKED 57 段雙份）。K4e2 起框內段落**不再
    /// 佔正文槽位**（口徑鐵律改寫點，四解析器同步），故各檔段數＝正文段數；
    /// 「修後必須完整保留的文字」= 正文段文字（框內文字改由
    /// `nested_txbx_paragraph_keeps_outer_text` 驗附屬欄）。
    #[test]
    fn corpus_txbx_paragraph_counts_regression() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        let cases: [(&str, usize, &[&str]); 4] = [
            // (檔案, 修後段落數, 修前被吞／雙份、修後必須完整保留的文字)
            (
                "xopt/OpenXmlPowerToolsExamples_TextReplacer02_Test01.docx",
                3,
                &["the on the Insert tab"],
            ),
            // WC049：修前段數同為 8 但外層段「You can also type…」整段被內層
            // 覆寫吞字（缺陷 2 吞字指紋）；修後必須完整保留。
            (
                "xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx",
                7,
                &["You can also type a keyword"],
            ),
            (
                // K4e2：30 段全部是框內段（LINKED 正文只剩表頭/段 2 個）——
                // 框內文字改由 txbx_texts 保留（must_keep 檢查連框內文字一起
                // join，見迴圈內 joined）。
                "ooxmlexport/LinkedTextBoxes.docx",
                2,
                &["Congratulations", "IA Scholarship Program Nominations"],
            ),
            // shapes-with-text：修前 10 段（VML 雙份）→ 6 段 → K4e2 起 1 段
            // （5 個框內段掛 txbx_texts）。
            (
                "poi/test-data_document_shapes-with-text.docx",
                1,
                &["Floating text box"],
            ),
        ];
        for (rel, want_n, must_keep) in cases {
            let bytes = std::fs::read(corpus.join(rel)).unwrap_or_else(|e| panic!("讀 {rel}: {e}"));
            let doc = parse(&bytes).expect("parse");
            let texts = doc.paragraph_texts();
            assert_eq!(texts.len(), want_n, "{rel}：段落數偏離修後基準");
            // K4e2：框內文字不再混進正文段——「不被吞」檢查連 txbx_texts
            // 附屬欄一起 join（分離後文字仍須完整存在，只是換了棲身之處）。
            let mut all: Vec<String> = texts.clone();
            for p in &doc.paragraphs {
                all.extend(p.txbx_texts.iter().cloned());
            }
            let joined = all.join("\n");
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
            let got = parse_doc(xml).expect("主解析").len();
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
            let got = parse_doc(xml).expect("主解析").len();
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
            let n_paras = parse_doc(xml).expect("主解析").len();
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
        let paras = parse_doc(xml).expect("解析樣式 XML 失敗");
        assert_eq!(paras.len(), 1);
        let runs = &paras[0].runs;
        assert_eq!(runs.len(), 4);

        assert!(runs[0].bold && runs[0].italic && runs[0].underline && runs[0].strike);
        assert_eq!(runs[0].color, Some(0xFFFF_0000));

        assert!(!runs[1].bold && !runs[1].italic && !runs[1].underline && !runs[1].strike);
        assert_eq!(runs[1].color, None);

        // 超連結預設色／底線已降格為級聯末端兜底：解析期只標記來源，
        // 不得寫進 run.color / run.direct（否則冒充直接格式越過字元樣式）。
        assert_eq!(runs[2].color, None);
        assert!(!runs[2].underline);
        assert!(runs[2].hyperlink_default);

        assert_eq!(runs[3].color, Some(0xFF00_FF00));
        assert!(!runs[3].underline);
        assert!(runs[3].hyperlink_default);
    }

    /// 紅燈先行：styles.xml 的 Hyperlink 字元樣式宣告 color=0000FF，
    /// document.xml 的 w:hyperlink 內 run 只有 rStyle 引用、無顯式色。
    /// 依 ECMA-376 §17.7.2（直接格式＞字元樣式＞段落樣式＞docDefaults），
    /// 級聯有效色必須是 0000FF。修復前解析器把合成預設藍寫進 run.direct，
    /// 直接格式越過字元樣式 → 本測試失敗（實際得到 0xFF0563C1）。
    #[test]
    fn hyperlink_character_style_color_beats_synthetic_default() {
        let styles_xml = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:style w:type="character" w:styleId="Hyperlink">
            <w:rPr><w:color w:val="0000FF"/><w:u w:val="single"/></w:rPr>
          </w:style>
        </w:styles>"#;
        let doc_xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:hyperlink w:anchor="_Toc1">
                <w:r>
                  <w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr>
                  <w:t>目錄連結</w:t>
                </w:r>
              </w:hyperlink>
            </w:p>
          </w:body>
        </w:document>"#;
        let st = StyleTable::parse(styles_xml.as_bytes());
        let paras = parse_doc(doc_xml.as_bytes()).expect("解析 document.xml");
        let run = &paras[0].runs[0];
        assert!(run.hyperlink_default, "超連結內 run 須標記來源");
        // 走正式級聯路徑：段落級 → 字元樣式純鏈 → run 直接格式 → 末端兜底。
        let (base, _) = st.effective_para(None, ParaStyle::default());
        let mut b = base;
        if let Some(cid) = run.rstyle.as_deref() {
            b = StyleTable::resolve_run(b, st.char_chain(cid).unwrap_or_default());
        }
        let eff = StyleTable::resolve_run(b, run.direct.clone().unwrap_or_default())
            .with_hyperlink_default(run.hyperlink_default);
        assert_eq!(
            eff.color,
            Some(0xFF00_00FF),
            "Hyperlink 字元樣式的 0000FF 必須勝過解析器合成預設色"
        );
        assert_eq!(eff.underline, Some(true));
    }

    // ---------------- 主題色查表（w:themeColor → theme1.xml clrScheme） ----------------

    /// 最小主題 part：hlink＝305090（LO 實測連結色）、accent1＝4F81BD。
    const THEME_HLINK: &[u8] =
        br#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
      <a:themeElements>
        <a:clrScheme name="Office">
          <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
          <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
          <a:accent1><a:srgbClr val="4F81BD"/></a:accent1>
          <a:hlink><a:srgbClr val="305090"/></a:hlink>
          <a:folHlink><a:srgbClr val="800080"/></a:folHlink>
        </a:clrScheme>
      </a:themeElements>
    </a:theme>"#;

    /// run 直接格式的主題色**優先於** `w:val` 回退值（ECMA-376 §17.3.2.25）：
    /// `<w:color w:val="0000FF" w:themeColor="hyperlink"/>` 有效色＝主題 hlink
    /// ＝0x305090（修復前只吃 w:val＝純藍 0x0000FF，與 Word/LO 偏離）。
    #[test]
    fn run_direct_theme_color_overrides_val() {
        let theme = ThemeColors::parse(THEME_HLINK);
        let doc_xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body><w:p>
            <w:r><w:rPr><w:color w:val="0000FF" w:themeColor="hyperlink"/></w:rPr><w:t>主題連結色</w:t></w:r>
          </w:p></w:body>
        </w:document>"#;
        let paras = parse_document_xml(doc_xml.as_bytes(), &theme).expect("解析 document.xml");
        let run = &paras[0].runs[0];
        assert_eq!(
            run.color,
            Some(0xFF30_5090),
            "run 直接格式的有效色必須是主題 hlink（0x305090），非 w:val 純藍"
        );
        let direct = run.direct.clone().expect("run 直接格式快照必須存在");
        assert_eq!(direct.color, Some(0xFF30_5090));
        assert!(direct.color_set, "宣告了顏色 → color_set 必真");
    }

    /// 回歸保護：**無 theme part** → 老實退回 `w:val`（不虛構主題色）。
    #[test]
    fn run_direct_theme_absent_falls_back_to_val() {
        let doc_xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body><w:p>
            <w:r><w:rPr><w:color w:val="0000FF" w:themeColor="hyperlink"/></w:rPr><w:t>無主題</w:t></w:r>
          </w:p></w:body>
        </w:document>"#;
        let paras = parse_doc(doc_xml.as_bytes()).expect("解析 document.xml");
        let run = &paras[0].runs[0];
        assert_eq!(run.color, Some(0xFF00_00FF), "無主題 → 退回 w:val");
    }

    /// 容錯：`w:themeColor` 指向**查無**的名字（accent9）→ 退回 `w:val`。
    #[test]
    fn run_direct_theme_unknown_name_falls_back_to_val() {
        let theme = ThemeColors::parse(THEME_HLINK);
        let doc_xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body><w:p>
            <w:r><w:rPr><w:color w:val="123456" w:themeColor="accent9"/></w:rPr><w:t>查無</w:t></w:r>
          </w:p></w:body>
        </w:document>"#;
        let paras = parse_document_xml(doc_xml.as_bytes(), &theme).expect("解析 document.xml");
        assert_eq!(paras[0].runs[0].color, Some(0xFF12_3456));
    }

    /// 誠實近似：帶 `w:themeShade`／`w:themeTint` → 保留 Word 預算好的
    /// `w:val`（正確色是變換後的主題色；lumMod/lumOff 變換本卡未實作）。
    #[test]
    fn run_direct_theme_shade_keeps_precomputed_val() {
        let theme = ThemeColors::parse(THEME_HLINK);
        let doc_xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body><w:p>
            <w:r><w:rPr><w:color w:val="365F91" w:themeColor="accent1" w:themeShade="BF"/></w:rPr><w:t>著色標題</w:t></w:r>
          </w:p></w:body>
        </w:document>"#;
        let paras = parse_document_xml(doc_xml.as_bytes(), &theme).expect("解析 document.xml");
        assert_eq!(
            paras[0].runs[0].color,
            Some(0xFF36_5F91),
            "帶 themeShade → 保留 Word 預算色（非未變換的 accent1 4F81BD）"
        );
    }

    /// 回歸保護：styles.xml 無 Hyperlink 樣式、run 也無顯式色時，級聯末端
    /// 仍兜底合成 #0563C1＋底線（原本解析期合成的行為不因降格而消失）。
    #[test]
    fn hyperlink_default_fallback_applies_when_no_layer_declares_color() {
        let st = StyleTable::parse(b"");
        let doc_xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:hyperlink>
                <w:r><w:t>無樣式連結</w:t></w:r>
              </w:hyperlink>
            </w:p>
          </w:body>
        </w:document>"#;
        let paras = parse_doc(doc_xml.as_bytes()).expect("解析 document.xml");
        let run = &paras[0].runs[0];
        assert!(run.color.is_none(), "解析期不得再合成顏色");
        assert!(!run.underline, "解析期不得再合成底線");
        let (base, _) = st.effective_para(None, ParaStyle::default());
        let eff = StyleTable::resolve_run(base, run.direct.clone().unwrap_or_default())
            .with_hyperlink_default(run.hyperlink_default);
        assert_eq!(eff.color, Some(HYPERLINK_DEFAULT_COLOR_ARGB));
        assert_eq!(eff.underline, Some(true));
    }

    /// 直接格式仍贏過一切：run 自帶 `w:color`／`w:u` 時，末端兜底不得覆蓋。
    #[test]
    fn hyperlink_direct_format_beats_cascade_fallback() {
        let st = StyleTable::parse(
            r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
              <w:style w:type="character" w:styleId="Hyperlink">
                <w:rPr><w:color w:val="0000FF"/><w:u w:val="single"/></w:rPr>
              </w:style>
            </w:styles>"#
                .as_bytes(),
        );
        let doc_xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:hyperlink>
                <w:r>
                  <w:rPr><w:color w:val="FF0000"/><w:u w:val="none"/></w:rPr>
                  <w:t>直接格式連結</w:t>
                </w:r>
              </w:hyperlink>
            </w:p>
          </w:body>
        </w:document>"#;
        let paras = parse_doc(doc_xml.as_bytes()).expect("解析 document.xml");
        let run = &paras[0].runs[0];
        let (base, _) = st.effective_para(None, ParaStyle::default());
        let eff = StyleTable::resolve_run(base, run.direct.clone().unwrap_or_default())
            .with_hyperlink_default(run.hyperlink_default);
        assert_eq!(eff.color, Some(0xFFFF_0000), "直接色贏過樣式與兜底");
        assert_eq!(eff.underline, Some(false), "直接 u=none 明確關閉底線");
    }

    /// Hyperlink 樣式已有 `w:u` 宣告（含 none）時，末端兜底不得重複合成底線。
    #[test]
    fn hyperlink_style_underline_declaration_blocks_synthesis() {
        let st = StyleTable::parse(
            r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
              <w:style w:type="character" w:styleId="Hyperlink">
                <w:rPr><w:u w:val="none"/></w:rPr>
              </w:style>
            </w:styles>"#
                .as_bytes(),
        );
        let doc_xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:hyperlink>
                <w:r>
                  <w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr>
                  <w:t>不畫底線的連結</w:t>
                </w:r>
              </w:hyperlink>
            </w:p>
          </w:body>
        </w:document>"#;
        let paras = parse_doc(doc_xml.as_bytes()).expect("解析 document.xml");
        let run = &paras[0].runs[0];
        let (base, _) = st.effective_para(None, ParaStyle::default());
        let mut b = base;
        if let Some(cid) = run.rstyle.as_deref() {
            b = StyleTable::resolve_run(b, st.char_chain(cid).unwrap_or_default());
        }
        let eff = StyleTable::resolve_run(b, run.direct.clone().unwrap_or_default())
            .with_hyperlink_default(run.hyperlink_default);
        assert_eq!(eff.underline, Some(false), "樣式宣告 u=none 即不合成");
        assert_eq!(eff.color, Some(HYPERLINK_DEFAULT_COLOR_ARGB), "色仍兜底");
    }

    // ---------------- pPr/rPr 自閉合 run 標籤（t_5b391cfd S1） ----------------

    /// 同義兩制回歸：`w:pPr/w:rPr` 內的 run 格式標籤，成對寫法 `<w:b></w:b>`
    /// 一直收（Start 臂），自閉合寫法 `<w:b/>`（Word 產出恆為此形）曾被靜默
    /// 丟棄——落進 catch-all apply_style_tag 時 cur_run==None。修後 Empty 臂
    /// 鏡像 Start 臂，七標籤一律收進段落 p.prun（級聯 inline 輸入）。
    #[test]
    fn ppr_rpr_selfclosed_run_tags_collected_into_prun() {
        let xml = r#"<w:document xmlns:w="x">
            <w:body>
            <w:p>
              <w:pPr><w:rPr>
                <w:b/><w:i/><w:u/><w:strike/>
                <w:color w:val="FF0000"/><w:sz w:val="28"/><w:rStyle w:val="Tag"/>
              </w:rPr></w:pPr>
              <w:r><w:t>段落級宣告</w:t></w:r>
            </w:p>
            </w:body></w:document>"#
            .as_bytes();
        let paras = parse_doc(xml).expect("解析");
        let prun = paras[0]
            .prun
            .clone()
            .expect("自閉合標籤必須收進 p.prun（與成對寫法同義）");
        assert_eq!(prun.bold, Some(true));
        assert_eq!(prun.italic, Some(true));
        assert_eq!(prun.underline, Some(true));
        assert_eq!(prun.strike, Some(true));
        assert!(prun.color_set, "color 宣告必須置 color_set");
        assert_eq!(prun.color, Some(0xFFFF_0000));
        assert_eq!(prun.size_pt, Some(14.0), "28 半磅 = 14pt");
        // 對照組：成對寫法（一直收的路徑）結果逐欄位一致——同義兩制消除。
        let paired = parse_doc(
            r#"<w:document xmlns:w="x"><w:body><w:p>
              <w:pPr><w:rPr>
                <w:b></w:b><w:i></w:i><w:u></w:u><w:strike></w:strike>
                <w:color w:val="FF0000"></w:color><w:sz w:val="28"></w:sz>
              </w:rPr></w:pPr>
              <w:r><w:t>對照</w:t></w:r></w:p></w:body></w:document>"#
                .as_bytes(),
        )
        .expect("解析對照")
        .remove(0)
        .prun
        .expect("成對寫法本來就收");
        assert_eq!(prun.bold, paired.bold);
        assert_eq!(prun.italic, paired.italic);
        assert_eq!(prun.underline, paired.underline);
        assert_eq!(prun.strike, paired.strike);
        assert_eq!(prun.color, paired.color);
        assert_eq!(prun.size_pt, paired.size_pt);
    }

    /// 邊界：pPr **外**（run 前）的自閉合標籤不得誤入 prun；`<w:rPr/>` 自閉合
    /// 本體後面的 spacing/jc 仍是段落欄位（既有臂不動）；rPr 外的 b（畸形位
    /// 置）維持原 catch-all 語義（prun 保持 None，不無中生有）。
    #[test]
    fn ppr_rpr_empty_arms_do_not_leak_across_boundaries() {
        // rPr 自閉合本體＋其後 spacing：spacing 仍收進 p.line。
        let paras = parse_doc(
            r#"<w:document xmlns:w="x"><w:body><w:p>
              <w:pPr><w:rPr/><w:spacing w:line="480" w:lineRule="auto"/></w:pPr>
              <w:r><w:t>a</w:t></w:r></w:p></w:body></w:document>"#
                .as_bytes(),
        )
        .expect("解析");
        assert!(paras[0].prun.is_none(), "<w:rPr/> 本體不材料化 prun");
        assert_eq!(paras[0].line, Some(LineSpacing::Auto(2.0)));
        // rPr 外的自閉合 b（pPr 直下，畸形位置）：不進 prun、不進任何 run。
        let paras = parse_doc(
            r#"<w:document xmlns:w="x"><w:body><w:p>
              <w:pPr><w:b/></w:pPr>
              <w:r><w:t>a</w:t></w:r></w:p></w:body></w:document>"#
                .as_bytes(),
        )
        .expect("解析");
        assert!(paras[0].prun.is_none(), "rPr 外的 b 不得材料化 prun");
        assert!(!paras[0].runs[0].bold, "也不得灌給後續 run");
    }

    /// K1(c)：段落 pPr 直接格式的 `w:before`／`w:after` 解析（twips→pt），
    /// 未宣告＝None；`0` 是明確宣告（Some(0)），不得與未宣告混淆。
    #[test]
    fn paragraph_direct_spacing_before_after_parsed() {
        let paras = parse_doc(
            r#"<w:document xmlns:w="x"><w:body>
              <w:p><w:pPr><w:spacing w:before="160" w:after="0"/></w:pPr>
                <w:r><w:t>a</w:t></w:r></w:p>
              <w:p><w:pPr><w:spacing w:line="240" w:lineRule="auto"/></w:pPr>
                <w:r><w:t>b</w:t></w:r></w:p>
            </w:body></w:document>"#
                .as_bytes(),
        )
        .expect("解析");
        assert_eq!(paras[0].spacing_before_pt, Some(8.0), "160 twips = 8pt");
        assert_eq!(paras[0].spacing_after_pt, Some(0.0), "0 是明確宣告");
        assert_eq!(paras[1].spacing_before_pt, None, "未宣告 = None");
        assert_eq!(paras[1].spacing_after_pt, None, "未宣告 = None");
    }

    // ---------------- 字型族宣告（w:rFonts；ECMA-376 §17.3.2.26） ----------------

    /// rFonts 直接格式解析：`w:ascii`／`w:eastAsia`／`w:cs` 與
    /// `w:hint="eastAsia"` 各自入欄（`w:cs` 未宣告＝None，非空字串）。
    #[test]
    fn run_direct_rfonts_parsed() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body><w:p>
            <w:r><w:rPr><w:rFonts w:ascii="Arial" w:eastAsia="標楷體" w:hint="eastAsia"/></w:rPr>
              <w:t>字型</w:t></w:r>
          </w:p></w:body>
        </w:document>"#;
        let paras = parse_doc(xml.as_bytes()).expect("解析 document.xml");
        let direct = paras[0].runs[0]
            .direct
            .clone()
            .expect("w:rFonts 必須物化 run 直接格式快照");
        let fonts = direct.fonts.expect("w:rFonts 必須收進 direct.fonts");
        assert_eq!(fonts.ascii.as_deref(), Some("Arial"), "w:ascii 入欄");
        assert_eq!(
            fonts.east_asia.as_deref(),
            Some("標楷體"),
            "w:eastAsia 入欄"
        );
        assert!(fonts.hint_east_asian, "w:hint=eastAsia 必須置位");
        assert_eq!(fonts.cs, None, "未宣告 w:cs ＝ None（繼承）");
    }

    /// 空 `w:rFonts`（無任何族別屬性）**不得**物化直接格式快照——「未見任何
    /// rPr 宣告＝None」的語義必須守住（否則 direct 會憑空變 Some）。
    #[test]
    fn empty_rfonts_does_not_materialize_direct() {
        let xml = r#"<w:document xmlns:w="x"><w:body><w:p>
          <w:r><w:rPr><w:rFonts/></w:rPr><w:t>空</w:t></w:r>
        </w:p></w:body></w:document>"#;
        let paras = parse_doc(xml.as_bytes()).expect("解析 document.xml");
        assert!(
            paras[0].runs[0].direct.is_none(),
            "空 w:rFonts 不得物化 direct（保持未宣告語義）"
        );
    }

    /// 序列化相容：舊版 JSON（不含 fonts 欄位）必須能反序列化（fonts＝None）；
    /// 新欄位為 None 時**不得**出現在序列化輸出（否則既有快照會多出
    /// `"fonts":null` 而變紅）。
    #[test]
    fn run_style_serde_is_backward_compatible() {
        // 舊版 RunStyle 的序列化輸出（無 fonts 欄位）。
        let old = r#"{"size_pt":12.0,"bold":true,"italic":null,"underline":null,"strike":null,"color":null,"color_set":false}"#;
        let rs: RunStyle = serde_json::from_str(old).expect("舊版 JSON 必須能反序列化");
        assert_eq!(rs.fonts, None, "舊版 JSON 不得帶出字型族");
        assert_eq!(rs.size_pt, Some(12.0), "舊欄位語義不變");
        // 新欄位 None 時不外露（skip_serializing_if）。
        let json = serde_json::to_string(&RunStyle::default()).expect("序列化");
        assert!(
            !json.contains("fonts"),
            "新欄位 None 時不得出現在序列化輸出：{json}"
        );
        // Run 的有效字型族欄位同口徑。
        let run_json = serde_json::to_string(&Run::default()).expect("序列化 Run");
        assert!(
            !run_json.contains("effective_fonts"),
            "Run 的新欄位 None 時不得外露：{run_json}"
        );
        // 帶宣告時必須可來回（serde round-trip）。
        let with_fonts = RunStyle {
            fonts: Some(RunFonts {
                ascii: Some("Arial".to_string()),
                east_asia: Some("標楷體".to_string()),
                cs: None,
                hint_east_asian: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let back: RunStyle = serde_json::from_str(&serde_json::to_string(&with_fonts).unwrap())
            .expect("往返反序列化");
        assert_eq!(back.fonts, with_fonts.fonts, "字型族必須可往返");
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
        let paras = parse_doc(xml).expect("解析");
        assert_eq!(paras.len(), 1);
        let text: String = paras[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(text, "abcd", "br 不得吞文字");
        assert_eq!(
            paras[0].page_breaks,
            vec![0, 2, 4],
            "段首/段中/段尾三口徑偏移"
        );
    }

    /// 非頁斷點一律不進 page_breaks：column、缺省（textWrapping）、換行；渲染提示
    /// `w:lastRenderedPageBreak`（t_01fa57ac 教訓）與欄內斷點同理無效。
    /// 軟性換行（缺省 `<w:br/>`）則在 run 文字流中記為 '\n'。
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
        let paras = parse_doc(xml).expect("解析");
        assert!(paras[0].page_breaks.is_empty(), "非 page 型別/渲染提示不記");
        let text: String = paras[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(text, "x\ny", "缺省軟性 br 產生換行碼點");
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
        let paras = parse_doc(xml).expect("解析");
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
        let paras = parse_doc(xml).expect("解析");
        assert_eq!(paras[0].page_breaks, vec![3], "中文字 2＋emoji 碼點 1＝3");
    }

    // ---------------- tab@pos 數值守衛（t_16a13211 R2） ----------------

    /// `pPr/w:tabs/w:tab/@w:pos` 病態值（t_16a13211 R2）：`parse::<f64>` 對
    /// 「1e45」「Infinity」「NaN」都成功，舊實作無過濾直進 `tab_right_pt` →
    /// engine `cascade_paragraphs` 算 `need = pos − …` 得 inf，`(need/dot_w)
    /// .floor() as usize` 飽和 usize::MAX 再被 min(4000) 截住——不 OOM 但炸開
    /// 4000 字元填充撐爆版面。守衛口徑同 styles.rs `MAX_OOXML_LONG` 族。
    #[test]
    fn tab_pos_guard_rejects_non_finite_and_huge() {
        let paras = parse_doc(
            r#"<w:document xmlns:w="x"><w:body>
              <w:p><w:pPr><w:tabs><w:tab w:val="right" w:pos="1e45" w:leader="dot"/></w:tabs></w:pPr>
                 <w:r><w:t>a</w:t></w:r></w:p>
              <w:p><w:pPr><w:tabs><w:tab w:val="right" w:pos="Infinity"/></w:tabs></w:pPr>
                 <w:r><w:t>b</w:t></w:r></w:p>
              <w:p><w:pPr><w:tabs><w:tab w:val="right" w:pos="NaN"/></w:tabs></w:pPr>
                 <w:r><w:t>c</w:t></w:r></w:p>
              </w:body></w:document>"#
                .as_bytes(),
        )
        .expect("解析");
        for (i, p) in paras.iter().enumerate() {
            assert_eq!(p.tab_right_pt, None, "第 {i} 段：病態 pos 必須拒收");
            assert_eq!(p.tab_leader, None, "第 {i} 段：拒收時 leader 不得單飛");
        }
        // 過度修正防護：正常 twips（4680=234pt）與負值（罕見但字面合法）照收。
        let paras = parse_doc(
            r#"<w:document xmlns:w="x"><w:body>
              <w:p><w:pPr><w:tabs><w:tab w:val="right" w:pos="4680" w:leader="dot"/></w:tabs></w:pPr>
                 <w:r><w:t>a</w:t></w:r></w:p>
              </w:body></w:document>"#
                .as_bytes(),
        )
        .expect("解析");
        assert_eq!(paras[0].tab_right_pt, Some(234.0));
        assert_eq!(paras[0].tab_leader.as_deref(), Some("dot"));
    }
}
