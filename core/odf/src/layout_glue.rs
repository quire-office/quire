//! ODF-C3 文字流投影 → layout 對接層。
//!
//! 定位：ODF 專屬中間表示 `OdfLayoutParagraph`（自產自銷，嚴禁引用
//! `quire_ooxml::Paragraph`——不跨線借模型），把 `content.xml` 的
//! `text:p`／`text:h` 投影成段落流＋逐段字級/粗體，降維
//! `(Vec<String>, Vec<f32>)`（文字＋每段代表字級）對接 `quire_layout` 既有
//! 斷行／分頁入口（`layout_paragraphs_grid`／`page_breaks`）。
//!
//! 字級級聯（嚴格序）：content.xml automatic-style → styles.xml 具名
//! common-style（`office:styles`）→ styles.xml default-style。單位只認
//! `fo:font-size` 以 `pt` 結尾且可解析的正數值；宣告了但非 pt／不可讀＝
//! 該層宣告落空為 `12.0pt`（不越層續找——「只認 pt、其餘一律 fallback
//! 12.0pt」的忠實讀法），缺省（未宣告）＝級續往下找。`fo:font-weight`：
//! 字串 `"bold"` 或數值 ≥700 記 bold，其餘宣告記 normal。樣式經
//! `style:parent-style-name` 與 `style:default-style-name` 鏈各最多追 8 層
//! （`MAX_STYLE_HOPS`），防環。
//!
//! default-style 層取用順序（記帳）：`family="paragraph"` 優先、缺時退
//! `family="text"`——語料實測 LibreOffice 系把段落文字預設寫在 paragraph 族
//! default-style（OASIS v1.3 標準文件即 10pt 在此層），只認 text 族會把整包
//! 誤落 12.0pt 後備值。兩族皆缺＝12.0pt／normal。
//!
//! 粗體只存進中間表示（`OdfLayoutParagraph::runs`）供渲染層未來消費——
//! layout 斷行不認字重（既有事實，非缺陷）。

use std::collections::HashMap;

use quick_xml::events::BytesStart;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::objects::{self, OdfObjectText};
use crate::ws::{self, TextSink, TextStream};
use crate::{OdfBundle, OdfError};

/// layout 層型別再匯出：殼層（quire-cli）經本 crate 取用，不直接 import
/// quire-layout（分層鐵律：shells/native 只准直接 import engine／odf 入口）。
pub use quire_layout::LayoutParams;

/// 字級缺省 fallback（卡面口徑：缺省、百分比、非 pt 一律 12.0pt）。
pub const DEFAULT_FONT_SIZE_PT: f32 = 12.0;
/// parent-style-name／default-style-name 鏈各自的追蹤上限（防環）。
pub const MAX_STYLE_HOPS: usize = 8;

/// 碼點區間（起點含、終點不含），對齊 `quire_layout::Line::char_offset` 標尺。
pub type CpRange = (usize, usize);

/// ODF 專屬段落中間表示。`text:p` 與 `text:h` 一律入列（heading 也是一段，
/// 不另立計數）。
///
/// `runs`：逐個文字片段 `(碼點區間, 字級pt, 粗體)`，依段落文字流文件序，
/// 相鄰同字級同字重合併。不變量：`runs` 恆非空——零文字段也帶一個
/// `(0,0)` 代表 run（段落代表字級由自身樣式級聯決定，與有無文字無關）。
/// 粗體只供渲染層未來消費，斷行層不讀。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OdfLayoutParagraph {
    /// 段落純文字（含 text:s 展開空格／text:tab／text:line-break）。
    pub text: String,
    /// 逐片段 `(碼點區間, 字級pt, 粗體)`。
    pub runs: Vec<(CpRange, f32, bool)>,
}

impl OdfLayoutParagraph {
    /// 該段代表字級：首 run 字級（`runs` 空＝缺省 12.0pt，理論不可達）。
    pub fn font_size_pt(&self) -> f32 {
        self.runs
            .first()
            .map(|r| r.1)
            .unwrap_or(DEFAULT_FONT_SIZE_PT)
    }

    /// 該段代表粗體：首 run 字重（`runs` 空＝normal，理論不可達）。
    pub fn bold(&self) -> bool {
        self.runs.first().map(|r| r.2).unwrap_or(false)
    }
}

/// 一份 .odt 的文字流投影結果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OdfProjection {
    /// 段落流：`text:p`＋`text:h` 一律入列（閉合序，同 body.rs 口徑——
    /// 巢裝文字框內段落先於外層容器段落收段）。
    ///
    /// **本流永不含物件 part 文字**（ODF-E2e：宿主段不得被物件文字污染；
    /// `flatten()`／layout／斷頁口徑因此零變動）。
    pub paragraphs: Vec<OdfLayoutParagraph>,
    /// 段落格式屬性（AGY-ODTIND：與 `paragraphs` 一一對位）。
    pub paragraph_properties: Vec<ParagraphProperties>,
    /// 段落文字總碼點數（宿主段落流；物件段計入 `object_text_len` 另立帳）。
    pub total_text_len: usize,
    /// `<text:p>` 原始計數（Start＋Empty 皆計，同 body.rs／survey 口徑）。
    pub text_p_count: usize,
    /// `<text:h>` 原始計數（同上）。
    pub text_h_count: usize,
    /// ODF-E2e：嵌入物件 part（`Object*/content.xml`）的 LO 發射面文字，
    /// 依引用 part 的文件序、part 內發射序排列。**獨立帳**：不進
    /// `paragraphs`、不進 `flatten()`（渲染層要消費物件文字，讀本欄）。
    pub object_paragraphs: Vec<OdfObjectText>,
    /// 物件發射文字總碼點數（= 各 `object_paragraphs[].text` 碼點和）。
    /// 與 `total_text_len` 分帳；對接 LO 的完整文字流總量＝兩者之和。
    pub object_text_len: usize,
    /// ODF-E2d：styles.xml `office:master-styles` 的頁首／頁尾**宣告面**——逐主頁
    /// 逐槽逐段（含巢裝表格／文字框內段落；宣告面照收空文字段，發射面才濾）。
    ///
    /// **獨立帳**：不進 `paragraphs`／`flatten()`（LO 每頁渲染頁首尾，若併入宿主
    /// 流會把多頁檔的重複發射算成宿主段落，斷頁與計數口徑全亂）。發射面見
    /// [`crate::header_footer::hf_assignment`]。
    pub hf_masters: Vec<crate::header_footer::MasterHf>,
    /// ODF-TABLE-MODEL：正文流二維表格模型（依頂層表格文件序排列）。
    /// 既有 D1 純文字發射時此欄可為空；接入表格排版時由 `with_tables` 填充。
    pub tables: Vec<crate::table::OdfBodyTable>,
}

impl OdfProjection {
    /// 附加正文表格清單（路線 B 建造者模式）。
    pub fn with_tables(mut self, tables: Vec<crate::table::OdfBodyTable>) -> Self {
        self.tables = tables;
        self
    }

    /// 投影段落總數（text:p＋text:h 入列後的段落數）。
    pub fn paragraph_count(&self) -> usize {
        self.paragraphs.len()
    }

    /// 逐段代表字級表（pt）。
    pub fn sizes_pt(&self) -> Vec<f32> {
        self.paragraphs.iter().map(|p| p.font_size_pt()).collect()
    }

    /// 降維輸出：（逐段文字，逐段代表字級）——對接 `quire_layout` 既有入口。
    pub fn flatten(&self) -> (Vec<String>, Vec<f32>) {
        (
            self.paragraphs.iter().map(|p| p.text.clone()).collect(),
            self.sizes_pt(),
        )
    }
}

/// 從已開封的 ODF 封裝投影（含 styles.xml 級聯）。
/// ODF-E1 加密閘門：manifest 宣告 content.xml／styles.xml 加密者回
/// `Err(OdfError::Encrypted)`（老實報錯不吞）。兩條消費軌極性不同：
/// - `project` 吃 content.xml＋styles.xml（字級級聯依賴樣式），兩者加密都擋。
/// - `extract_body` 只吃 content.xml——meta.xml／styles.xml 加密不擋內文抽取。
pub fn project(bundle: &OdfBundle) -> Result<OdfProjection, OdfError> {
    bundle.ensure_not_encrypted("content.xml")?;
    bundle.ensure_not_encrypted("styles.xml")?;
    let content = bundle.read_entry("content.xml")?;
    let styles = match bundle.read_entry("styles.xml") {
        Ok(b) => Some(b),
        Err(OdfError::MissingEntry { .. }) => None,
        Err(e) => return Err(e),
    };
    let mut projection = project_bytes(&content, styles.as_deref())?;
    // ODF-E2e：嵌入物件 part 取字（獨立帳，不進 paragraphs／flatten）。
    let (object_paragraphs, object_text_len) = object_face(bundle, &content)?;
    projection.object_paragraphs = object_paragraphs;
    projection.object_text_len = object_text_len;
    // ODF-E2d：頁首尾宣告面（獨立帳，不進 paragraphs／flatten）。styles.xml
    // 缺如＝空宣告；加密已被上面兩道閘門擋掉，這裡讀到的必是明文。
    projection.hf_masters = crate::header_footer::hf_masters(bundle)?;
    Ok(projection)
}

/// ODF-E2e：依宿主 `content.xml` 的 `draw:object` 引用序，逐 part 取 LO 發射面
/// 文字（規則見 [`crate::objects`]）。回傳（發射段清單, 碼點和）。
///
/// 口徑：
/// * 未被引用的 `Object*/` 目錄不讀（LO 不渲染）；`ObjectReplacements/` 是
///   `draw:image` 替代圖，非文字源——兩者天然不在引用清單內。
/// * manifest 宣告加密的物件 part 一併**跳過**（不強讀密文、不吞錯成零段：
///   宿主軌照常走 `ensure_not_encrypted` 閘門，本欄只是少一筆發射；語料 0 例）。
/// * 引用了但 zip 內無 `content.xml`（`./Obj100` 這類 OLE 懸空引用，語料 7 例）
///   → `MissingEntry` 就地跳過；其餘 zip／XML 錯誤照實上報。
fn object_face(
    bundle: &OdfBundle,
    content: &[u8],
) -> Result<(Vec<crate::objects::OdfObjectText>, usize), OdfError> {
    let mut out: Vec<crate::objects::OdfObjectText> = Vec::new();
    let mut chars = 0usize;
    for dir in objects::object_refs(content)? {
        let entry = format!("{dir}/content.xml");
        if bundle.manifest().is_encrypted(&entry) {
            continue;
        }
        let bytes = match bundle.read_entry(&entry) {
            Ok(b) => b,
            Err(OdfError::MissingEntry { .. }) => continue,
            Err(e) => return Err(e),
        };
        let media = bundle.manifest().media_type_of(&entry);
        for t in objects::object_texts(&dir, &bytes, media)? {
            chars += t.text.chars().count();
            out.push(t);
        }
    }
    Ok((out, chars))
}

/// 直接從 content.xml／styles.xml 位元組投影（純函數入口；styles_xml 缺＝
/// 無具名/default 層，級聯只剩 content.xml automatic-style 與 12.0pt fallback）。
///
/// **安全前置條件（ODF-E1 review）**：呼叫端必須保證傳入位元組為**明文** XML。
/// 本入口不接收 `Manifest`、無加密校驗防線——密文直入會得到 `Xml` 錯誤或零段
/// `Ok`（靜默）。凡手上持有 `OdfBundle` 的呼叫端嚴禁直呼本入口，必須經
/// [`project`] 走加密檢驗閘門（content.xml／styles.xml 雙閘）。
pub fn project_bytes(
    content_xml: &[u8],
    styles_xml: Option<&[u8]>,
) -> Result<OdfProjection, OdfError> {
    let mut book = StyleBook::default();
    if let Some(sx) = styles_xml {
        parse_style_file(sx, &mut book, StyleScope::StylesXml)?;
    }
    // content.xml 兩趟：第一趟只收 automatic-styles（收齊後段落走查才不會
    // 漏掉樣式宣告在引用之後的文件），第二趟走段落。
    parse_style_file(content_xml, &mut book, StyleScope::ContentXml)?;
    walk_paragraphs(content_xml, &book)
}

/// 投影 → `quire_layout` 逐段字級斷行：每段以自身代表字級為斷行量測尺
/// （既有入口 `layout_paragraphs_grid`）。行帶高沿用全域 `params.line_h_pt()`
/// ——分頁容量仍按文件行帶尺，與 DOCX 線 `ParaGrid` 既有口徑一致。
pub fn layout_projection(
    projection: &OdfProjection,
    params: &LayoutParams,
) -> Vec<quire_layout::ParagraphLayout> {
    let (texts, sizes) = projection.flatten();
    let grid: Vec<(f32, f32)> = sizes.iter().map(|&s| (s, params.line_h_pt())).collect();
    quire_layout::layout_paragraphs_grid(&texts, params, &[], &grid)
}

/// 全域尺（`params.font_size_pt`）分頁斷點：每頁（首頁除外）起排行的段落
/// index，單調遞增；同段跨頁續排造成的相鄰同值合併為一（段落視角的
/// 「斷點」＝新頁起排的段落邊界表）。對接 `quire_layout::page_breaks`。
pub fn page_break_positions(texts: &[String], params: &LayoutParams) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let breaks = quire_layout::page_breaks(texts, params);
    for pb in breaks.into_iter().skip(1) {
        if out.last() != Some(&pb.first_para) {
            out.push(pb.first_para);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 樣式簿與級聯
// ---------------------------------------------------------------------------

/// 段落對齊方式（ODF 1.3 Part 3 §20.375 fo:text-align）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    Right,
    Justify,
}

/// 段落格式屬性（AGY-ODTIND：縮排與對齊）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParagraphProperties {
    /// `fo:text-align`（center / end / right / start / left / justify）。
    pub align: Option<Align>,
    /// `fo:margin-left`（左邊距，pt）。
    pub indent_left_pt: Option<f32>,
    /// `fo:margin-right`（右邊距，pt）。
    pub indent_right_pt: Option<f32>,
    /// `fo:text-indent`（首行縮排，pt；負值為懸掛縮排）。
    pub indent_first_line_pt: Option<f32>,
}

/// 樣式記錄（級聯用）：保留字級/字重與段落縮排對齊所需欄位。
#[derive(Debug, Clone, Default)]
struct StyleRecord {
    /// `style:name`（default-style 無名稱＝None）。
    name: Option<String>,
    /// `style:family`（級聯只關切 paragraph／text 族）。
    family: String,
    /// `style:parent-style-name`。
    parent: Option<String>,
    /// `style:default-style-name`（罕見於文字族，仍支援）。
    default_name: Option<String>,
    /// `fo:font-size` 是否宣告（宣告了但非 pt/不可讀＝宣告落空為 12.0）。
    size_declared: bool,
    /// 可讀宣告值（pt）；宣告了不可讀者記 `DEFAULT_FONT_SIZE_PT`。
    size_pt: f32,
    /// `fo:font-weight` 是否宣告。
    weight_declared: bool,
    /// 字串 `"bold"` 或數值 ≥700 → true，其餘宣告 → false。
    bold: bool,
    /// 段落對齊方式（`fo:text-align`）。
    align: Option<Align>,
    /// 段落左邊距（`fo:margin-left`，pt）。
    indent_left_pt: Option<f32>,
    /// 段落右邊距（`fo:margin-right`，pt）。
    indent_right_pt: Option<f32>,
    /// 段落首行縮排（`fo:text-indent`，pt，負值為懸掛縮排）。
    indent_first_line_pt: Option<f32>,
}

impl StyleRecord {
    /// 該記錄是否為可供文字級聯值的族（段落族／文字族）。
    fn texty(&self) -> bool {
        self.family == "paragraph" || self.family == "text"
    }
}

/// 樣式簿：tier1 content.xml automatic-styles、tier2 styles.xml 具名
/// common-styles（office:styles）、tier3 styles.xml default-style。
#[derive(Debug, Clone, Default)]
pub(crate) struct StyleBook {
    automatic: HashMap<String, StyleRecord>,
    named: HashMap<String, StyleRecord>,
    /// default-style(family=paragraph)（優先層）。
    default_paragraph: Option<StyleRecord>,
    /// default-style(family=text)（退路層）。
    default_text: Option<StyleRecord>,
}

#[derive(Debug, Clone, Default)]
struct ResolvedStyle {
    size_pt: Option<f32>,
    bold: Option<bool>,
    align: Option<Align>,
    indent_left_pt: Option<f32>,
    indent_right_pt: Option<f32>,
    indent_first_line_pt: Option<f32>,
}

impl ResolvedStyle {
    fn is_complete(&self) -> bool {
        self.size_pt.is_some()
            && self.bold.is_some()
            && self.align.is_some()
            && self.indent_left_pt.is_some()
            && self.indent_right_pt.is_some()
            && self.indent_first_line_pt.is_some()
    }
}

impl StyleBook {
    /// 級聯解析（套用缺省後）：回傳 `(字級pt, 粗體)`——段落代表值用。
    /// ODF-E2d 頁首尾段落字級同層複用（`header_footer`）。
    pub(crate) fn resolve(&self, name: &str) -> (f32, bool) {
        let res = self.resolve_opt(name);
        (
            res.size_pt.unwrap_or(DEFAULT_FONT_SIZE_PT),
            res.bold.unwrap_or(false),
        )
    }

    /// 級聯解析段落格式（含字級、粗體、對齊與縮排屬性）。
    pub(crate) fn resolve_paragraph(&self, name: &str) -> (f32, bool, ParagraphProperties) {
        let res = self.resolve_opt(name);
        (
            res.size_pt.unwrap_or(DEFAULT_FONT_SIZE_PT),
            res.bold.unwrap_or(false),
            ParagraphProperties {
                align: res.align,
                indent_left_pt: res.indent_left_pt,
                indent_right_pt: res.indent_right_pt,
                indent_first_line_pt: res.indent_first_line_pt,
            },
        )
    }

    /// 級聯解析（三態；**含** default-style 層，缺省層已套用，`None`＝全程
    /// 未宣告）——段落用：樣式鏈走完仍缺者落 default-style 層（paragraph 族
    /// 優先、text 族退路），仍缺＝呼叫端缺省。
    fn resolve_opt(&self, name: &str) -> ResolvedStyle {
        let mut res = self.resolve_chain(name);
        // default-style 層（paragraph 族優先，text 族退路）。
        let dflt = self
            .default_paragraph
            .as_ref()
            .or(self.default_text.as_ref());
        if let Some(d) = dflt {
            if res.size_pt.is_none() && d.size_declared {
                res.size_pt = Some(d.size_pt);
            }
            if res.bold.is_none() && d.weight_declared {
                res.bold = Some(d.bold);
            }
            if res.align.is_none() && d.align.is_some() {
                res.align = d.align;
            }
            if res.indent_left_pt.is_none() && d.indent_left_pt.is_some() {
                res.indent_left_pt = d.indent_left_pt;
            }
            if res.indent_right_pt.is_none() && d.indent_right_pt.is_some() {
                res.indent_right_pt = d.indent_right_pt;
            }
            if res.indent_first_line_pt.is_none() && d.indent_first_line_pt.is_some() {
                res.indent_first_line_pt = d.indent_first_line_pt;
            }
        }
        res
    }

    /// 級聯解析（三態；**不含** default-style 層）——`text:span`／`text:a` 用：
    /// 片段缺宣告的屬性應沿承**所屬段落的級聯有效值**（比檔案 default-style
    /// 更具體，LO 實測同形），由呼叫端拿 parent 值補缺。
    fn resolve_chain(&self, name: &str) -> ResolvedStyle {
        let mut res = ResolvedStyle::default();
        let mut cur: Option<String> = if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        };
        let mut hops_parent = 0usize;
        let mut hops_default = 0usize;
        let mut seen: Vec<String> = Vec::new();

        while let Some(key) = cur {
            if seen.iter().any(|s| s == &key) {
                break; // 防環
            }
            seen.push(key.clone());
            let rec = match self.automatic.get(&key).or_else(|| self.named.get(&key)) {
                Some(r) => r,
                None => break,
            };
            if rec.texty() {
                if res.size_pt.is_none() && rec.size_declared {
                    res.size_pt = Some(rec.size_pt);
                }
                if res.bold.is_none() && rec.weight_declared {
                    res.bold = Some(rec.bold);
                }
                if res.align.is_none() && rec.align.is_some() {
                    res.align = rec.align;
                }
                if res.indent_left_pt.is_none() && rec.indent_left_pt.is_some() {
                    res.indent_left_pt = rec.indent_left_pt;
                }
                if res.indent_right_pt.is_none() && rec.indent_right_pt.is_some() {
                    res.indent_right_pt = rec.indent_right_pt;
                }
                if res.indent_first_line_pt.is_none() && rec.indent_first_line_pt.is_some() {
                    res.indent_first_line_pt = rec.indent_first_line_pt;
                }
                if res.is_complete() {
                    break; // 全屬性皆定案
                }
            }
            // 續鏈：parent 優先；各鏈上限 MAX_STYLE_HOPS。
            cur = None;
            if let Some(p) = rec.parent.as_deref() {
                if hops_parent < MAX_STYLE_HOPS {
                    hops_parent += 1;
                    cur = Some(p.to_string());
                }
            }
            if cur.is_none() {
                if let Some(d) = rec.default_name.as_deref() {
                    if hops_default < MAX_STYLE_HOPS {
                        hops_default += 1;
                        cur = Some(d.to_string());
                    }
                }
            }
        }

        res
    }
}

/// `parse_style_file` 的收集範圍。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StyleScope {
    /// content.xml：只收 office:automatic-styles 內的 style:style。
    ContentXml,
    /// styles.xml：只收 office:styles 內的 style:style 與 style:default-style。
    StylesXml,
    /// styles.xml：只收 **office:automatic-styles** 內的 style:style（ODF-E2d
    /// 頁首尾段落樣式 MP1/MP3 宣告在這層——C3 的 `StylesXml` 不收，故另立一
    /// 個 scope 而不是改既有 scope 的行為：C3 三層口徑逐字不動）。
    StylesXmlAutomatic,
}

/// 從 XML 收樣式記錄進簿（巢裝深度掃描：style 記錄以 `style:style`／
/// `style:default-style` 的 Start～End 為界，text-properties 子元素補
/// 字級/字重欄位；`text:list-style` 等外來巢裝的 text-properties 因
/// depth_style 未起不誤收）。
pub(crate) fn parse_style_file(
    bytes: &[u8],
    book: &mut StyleBook,
    scope: StyleScope,
) -> Result<(), OdfError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();

    let mut depth_office_styles = 0usize; // office:styles（styles.xml 具名層）
    let mut depth_automatic = 0usize; // office:automatic-styles
    let mut pending: Option<StyleRecord> = None; // 當前 style:style / default-style
    let mut pending_is_default = false;
    let mut depth_style = 0usize; // style:style / style:default-style 巢裝深度

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if name == b"office:styles" {
                    depth_office_styles += 1;
                } else if name == b"office:automatic-styles" {
                    depth_automatic += 1;
                } else if is_style_element(name) {
                    depth_style += 1;
                    let in_scope = match scope {
                        StyleScope::StylesXml => depth_office_styles > 0,
                        StyleScope::ContentXml | StyleScope::StylesXmlAutomatic => {
                            depth_automatic > 0
                        }
                    };
                    if in_scope && pending.is_none() {
                        pending = Some(style_record_from_attrs(&e));
                    }
                } else if name == b"style:default-style" {
                    depth_style += 1;
                    if scope == StyleScope::StylesXml && pending.is_none() {
                        pending = Some(style_record_from_attrs(&e));
                        pending_is_default = true;
                    }
                } else if (name == b"style:text-properties" || name == b"text-properties")
                    && depth_style > 0
                {
                    if let Some(rec) = pending.as_mut() {
                        apply_text_properties(&e, rec);
                    }
                } else if (name == b"style:paragraph-properties" || name == b"paragraph-properties")
                    && depth_style > 0
                {
                    if let Some(rec) = pending.as_mut() {
                        apply_paragraph_properties(&e, rec);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if name == b"office:styles" {
                    depth_office_styles = depth_office_styles.saturating_sub(1);
                } else if name == b"office:automatic-styles" {
                    depth_automatic = depth_automatic.saturating_sub(1);
                } else if is_style_element(name) || name == b"style:default-style" {
                    depth_style = depth_style.saturating_sub(1);
                    if depth_style == 0 {
                        if let Some(rec) = pending.take() {
                            if pending_is_default {
                                commit_default_record(book, rec);
                            } else {
                                commit_named_record(book, rec, scope);
                            }
                        }
                        pending_is_default = false;
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                // 自閉合 style:style（無 text-properties 子元素）。
                let name = e.name();
                let name = name.as_ref();
                if is_style_element(name) {
                    let in_scope = match scope {
                        StyleScope::StylesXml => depth_office_styles > 0,
                        StyleScope::ContentXml | StyleScope::StylesXmlAutomatic => {
                            depth_automatic > 0
                        }
                    };
                    if in_scope && pending.is_none() {
                        let rec = style_record_from_attrs(&e);
                        commit_named_record(book, rec, scope);
                    }
                } else if (name == b"style:text-properties" || name == b"text-properties")
                    && depth_style > 0
                {
                    // 自閉合 text-properties（語料常態：`<style:text-properties .../>`）。
                    if let Some(rec) = pending.as_mut() {
                        apply_text_properties(&e, rec);
                    }
                } else if (name == b"style:paragraph-properties" || name == b"paragraph-properties")
                    && depth_style > 0
                {
                    // 自閉合 paragraph-properties（語料常態：`<style:paragraph-properties .../>`）。
                    if let Some(rec) = pending.as_mut() {
                        apply_paragraph_properties(&e, rec);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(OdfError::Xml(err.to_string())),
            _ => {}
        }
        buf.clear();
    }
    Ok(())
}

#[inline]
fn is_style_element(name: &[u8]) -> bool {
    name == b"style:style" || name == b"style"
}

/// 具名／自動樣式入簿（tier 由 scope 決定；同名時 automatic 覆寫 named，
/// `StyleBook::resolve` 先查 automatic）。
fn commit_named_record(book: &mut StyleBook, rec: StyleRecord, scope: StyleScope) {
    let Some(name) = rec.name.clone() else {
        return;
    };
    if name.is_empty() {
        return;
    }
    match scope {
        // styles.xml 的 automatic-styles 層與 content.xml 同屬 automatic 簿
        // （ODF-E2d 頁首尾段落樣式 MP1/MP3 在此層；automatic 覆寫 named 正是
        //  既有 `StyleBook::resolve` 的優先序，不另造規則）。
        StyleScope::ContentXml | StyleScope::StylesXmlAutomatic => {
            book.automatic.insert(name, rec);
        }
        StyleScope::StylesXml => {
            book.named.entry(name).or_insert(rec);
        }
    }
}

/// default-style 入簿（只收 paragraph／text 族；先來者勝，防重複宣告覆寫）。
fn commit_default_record(book: &mut StyleBook, rec: StyleRecord) {
    match rec.family.as_str() {
        "paragraph" if book.default_paragraph.is_none() => {
            book.default_paragraph = Some(rec);
        }
        "text" if book.default_text.is_none() => {
            book.default_text = Some(rec);
        }
        _ => {}
    }
}

/// 自 Start 事件屬性構樣式記錄（name/family/parent/default-name）。
fn style_record_from_attrs(e: &BytesStart<'_>) -> StyleRecord {
    let mut rec = StyleRecord::default();
    for attr in e.attributes().flatten() {
        let key = attr.key;
        let local = key.local_name();
        let local = local.as_ref();
        if local == b"name" {
            rec.name = Some(attr_value_str(&attr.value));
        } else if local == b"family" {
            rec.family = attr_value_str(&attr.value);
        } else if local == b"parent-style-name" {
            rec.parent = Some(attr_value_str(&attr.value));
        } else if local == b"default-style-name" {
            rec.default_name = Some(attr_value_str(&attr.value));
        }
    }
    rec
}

/// `style:text-properties` 只取 `fo:font-size`／`fo:font-weight` 兩個宣告。
fn apply_text_properties(e: &BytesStart<'_>, rec: &mut StyleRecord) {
    for attr in e.attributes().flatten() {
        let local = attr.key.local_name();
        match local.as_ref() {
            b"font-size" => {
                rec.size_declared = true;
                rec.size_pt = parse_pt(&attr.value);
            }
            b"font-weight" => {
                rec.weight_declared = true;
                rec.bold = parse_bold(&attr.value);
            }
            _ => {}
        }
    }
}

/// `style:paragraph-properties` 採集 `fo:text-align`／`fo:margin-left`／`fo:margin-right`／`fo:text-indent`。
fn apply_paragraph_properties(e: &BytesStart<'_>, rec: &mut StyleRecord) {
    for attr in e.attributes().flatten() {
        let local = attr.key.local_name();
        match local.as_ref() {
            b"text-align" => {
                let s = attr_value_str(&attr.value);
                rec.align = parse_align(&s);
            }
            b"margin-left" => {
                let s = attr_value_str(&attr.value);
                rec.indent_left_pt = parse_length_signed(&s);
            }
            b"margin-right" => {
                let s = attr_value_str(&attr.value);
                rec.indent_right_pt = parse_length_signed(&s);
            }
            b"text-indent" => {
                let s = attr_value_str(&attr.value);
                rec.indent_first_line_pt = parse_length_signed(&s);
            }
            _ => {}
        }
    }
}

/// 解析 `fo:text-align` 屬性值（center / end / right / start / left / justify）。
fn parse_align(s: &str) -> Option<Align> {
    match s.trim() {
        "start" | "left" => Some(Align::Start),
        "center" => Some(Align::Center),
        "end" => Some(Align::End),
        "right" => Some(Align::Right),
        "justify" => Some(Align::Justify),
        _ => None,
    }
}

/// 解析帶符號長度（支援負值懸掛縮排與 0 長度，複用 `odf_length_to_pt`）。
fn parse_length_signed(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('-') {
        crate::page_geometry::odf_length_to_pt(rest).map(|v| -v)
    } else if let Some(rest) = s.strip_prefix('+') {
        crate::page_geometry::odf_length_to_pt(rest)
    } else if let Some(v) = crate::page_geometry::odf_length_to_pt(s) {
        Some(v)
    } else {
        // 容錯 0 / 0cm / 0mm / 0in / 0pt / 0pc 等零長度
        let trimmed = s.trim_end_matches(|c: char| c.is_alphabetic() || c == '%');
        if let Ok(v) = trimmed.trim().parse::<f32>() {
            if v == 0.0 {
                return Some(0.0);
            }
        }
        None
    }
}

/// 只認 `pt` 結尾且可解析為有限正數的值；其餘（百分比、cm、不可讀）一律
/// `DEFAULT_FONT_SIZE_PT`（呼叫端已記 size_declared＝宣告落空不再下探）。
fn parse_pt(v: &[u8]) -> f32 {
    let s = match std::str::from_utf8(v) {
        Ok(s) => s.trim(),
        Err(_) => return DEFAULT_FONT_SIZE_PT,
    };
    let Some(num) = s.strip_suffix("pt") else {
        return DEFAULT_FONT_SIZE_PT;
    };
    match num.trim().parse::<f32>() {
        Ok(x) if x.is_finite() && x > 0.0 => x,
        _ => DEFAULT_FONT_SIZE_PT,
    }
}

/// `fo:font-weight`：字串 `"bold"` 或數值 ≥700 → bold。
fn parse_bold(v: &[u8]) -> bool {
    let s = match std::str::from_utf8(v) {
        Ok(s) => s.trim(),
        Err(_) => return false,
    };
    if s == "bold" {
        return true;
    }
    match s.parse::<f32>() {
        Ok(x) if x.is_finite() => x >= 700.0,
        _ => false,
    }
}

fn attr_value_str(v: &[u8]) -> String {
    std::str::from_utf8(v).unwrap_or("").to_string()
}

// ---------------------------------------------------------------------------
// 段落走查（段落流＋runs）
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// XML 格式化空白正規化：實作已抽至共用作業模組 `crate::ws`（卡 t_4006056b
// 雙走查器口徑對齊）。以下 re-export 是**薄包裝**：既有外部路徑
// （`layout_glue::normalize_xml_text` 等，考卷 `odf_xml_whitespace_policy`
// 燈 1 直接打它）逐字不動。本檔不再持有任何一份正規化/gap 實作——
// `body.rs` 與下方 `walk_paragraphs` 同吃 `ws::emit_text_node`＋
// `TextStream` 的 pending_gap 狀態機（單一實作，嚴禁複製貼上第二份）。
// 語義管轄在 t_ae9a9f22／t_2a983570 的已定案契約，搬移零改寫；
// `ParaCtx` 只保留 layout 側的 run 樣式帳。
// ---------------------------------------------------------------------------
pub use crate::ws::{
    buggy_no_whitespace_normalize, is_xml_space, normalize_xml_text, normalize_xml_text_with_edge,
    EdgeInfo,
};

/// 進行中的段落上下文：文字流與 pending_gap 語義由共用的 [`TextStream`] 承載
/// （卡 t_4006056b——`body.rs` 走查器吃同一實作），本結構只疊 layout 側的
/// **run 樣式帳**（`(字級, 粗體)` 斷點合併）。
#[derive(Debug, Clone)]
struct ParaCtx {
    stream: TextStream,
    runs: Vec<(CpRange, f32, bool)>,
    /// 段落自身樣式的代表字級/字重（零文字段的代表 run）。
    own: (f32, bool),
}

impl ParaCtx {
    /// 建一段全新上下文（欄位初值集中一處，三處建站點不再各自展開）。
    fn new(own: (f32, bool)) -> Self {
        ParaCtx {
            stream: TextStream::new(),
            runs: Vec::new(),
            own,
        }
    }

    /// 追加文字並按當前樣式開/續 run（相鄰同字級同字重合併）。碼點起訖取自
    /// `TextStream::push_raw` 的碼點快取，不逐次全掃（O(L²) 熱點，卡 t_2a983570
    /// 順車修）。
    fn push_str(&mut self, s: &str, style: (f32, bool)) {
        let (from, to) = self.stream.push_raw(s);
        self.push_range((from, to), style);
    }

    /// 為**已入文**的碼點區間開/續 run（`flush_gap` 落地的空格走這條——空格由
    /// `TextStream` 推入，run 帳在本側補）。
    fn push_range(&mut self, (from, to): CpRange, style: (f32, bool)) {
        if let Some(last) = self.runs.last_mut() {
            if last.0 .1 == from && last.1 == style.0 && last.2 == style.1 {
                last.0 .1 = to;
                return;
            }
        }
        self.runs.push(((from, to), style.0, style.1));
    }

    /// 顯式空白元素（`text:s`／`text:tab`／`text:line-break`）：顯式元素是權威
    /// 空白來源，其周圍的格式化縮排屬噪音 ⇒ 待發空格直接放棄，不疊加
    /// （`TextStream::push_explicit` 同款語義，差別只在 run 帳要記在本側）。
    fn push_explicit(&mut self, s: &str, style: (f32, bool)) {
        self.stream.drop_gap();
        self.push_str(s, style);
    }

    /// 實體引用（GeneralRef）解析值：是**正文字元**（如 `&nbsp;` → NBSP），
    /// 不經正規化，但要先刷掉節點間待發空格（t_2a983570 燈 10 口徑）。發射
    /// 路徑直接複用 `TextSink::sink_push_content`（刷 gap＋推入），不另寫一份。
    fn push_entity(&mut self, s: &str, style: (f32, bool)) {
        TextSink::sink_push_content(self, s, style);
    }

    fn finish(self) -> OdfLayoutParagraph {
        let mut runs = self.runs;
        if runs.is_empty() {
            // 零文字段：段落樣式的代表 run（字級仍由級聯決定）。
            runs.push(((0, 0), self.own.0, self.own.1));
        }
        OdfLayoutParagraph {
            text: self.stream.into_text(),
            runs,
        }
    }
}

/// 共用發射器實作（卡 t_4006056b）：`ws::emit_text_node` 經這兩條方法把文字節點
/// 打進本上下文——gap 記帳落 `TextStream`，run 開窗留本側。
impl TextSink for ParaCtx {
    #[inline]
    fn sink_arm_gap(&mut self) {
        self.stream.arm_gap();
    }

    #[inline]
    fn sink_push_content(&mut self, s: &str, style: (f32, bool)) {
        // 正規化後的內容節點：先刷待發空格（可能落地成一個空格 run），再推入
        // （呼叫端保證 `s` 非純空白）。順序是 t_2a983570 的契約，不可顛倒。
        if let Some(range) = self.stream.flush_gap() {
            self.push_range(range, style);
        }
        self.push_str(s, style);
    }
}

/// 走查 content.xml：收段落流（text:p＋text:h 一律入列）與原始計數。
/// 段落閉合序入列（同 body.rs）；文字流內 text:s／text:tab／text:line-break
/// 展開同 body.rs 口徑。`text:span`／`text:a` 宣告樣式時換斷行尺，並把粗體
/// 以「沿承 parent 或自身宣告」合成（run 級粗體＝該片段宣告級聯值）。
fn walk_paragraphs(content_xml: &[u8], book: &StyleBook) -> Result<OdfProjection, OdfError> {
    let mut reader = Reader::from_reader(content_xml);
    let mut buf = Vec::new();

    let default_style = (DEFAULT_FONT_SIZE_PT, false);
    let mut paragraphs: Vec<OdfLayoutParagraph> = Vec::new();
    let mut paragraph_properties: Vec<ParagraphProperties> = Vec::new();
    let mut p_stack: Vec<ParaCtx> = Vec::new();
    let mut p_prop_stack: Vec<ParagraphProperties> = Vec::new();
    // 巢裝文字樣式堆疊：段落起點推入段落樣式；span/a 再推其樣式（頂部＝
    // 當前文字片段該用的字級/字重）。
    let mut style_stack: Vec<(f32, bool)> = Vec::new();
    // 巢裝 text:section 隱藏狀態堆疊：進入每個 section 時依自身屬性獨立判定並推入，離開時彈出；棧頂為 true 時抑制該區段內的文字發射，但維持段落平鋪槽位與原始計數不變。
    let mut section_hidden_stack: Vec<bool> = Vec::new();
    // ODF-E2c：圖形替代文字（svg:desc／svg:title）抑制。draw_stack 只記目前開著的繪圖容器，
    // 由最近者判定替代文字歸屬；alt_skip_depth 大於零表示正位於要跳過的替代文字子樹內（含自身）。
    // 邊界：draw:text-box 直屬的替代文字屬正文（LO 會渲染），不跳過；text-box 內巢裝 frame 的仍跳過。
    // 前提不變：ODF schema 不允許 text:section 出現在 svg:desc／svg:title 子樹內，故跳過期間
    // 不推進 section 堆疊不會造成堆疊失衡（實測 50 檔無此形）。
    let mut draw_stack: Vec<Vec<u8>> = Vec::new();
    let mut alt_skip_depth = 0usize;
    let mut text_p_count = 0usize;
    let mut text_h_count = 0usize;
    let mut total_text_len = 0usize;
    // 回塞旗標於走查進入時讀一次存區域變數：現況每個文字節點讀一次 env ⇒
    // 數萬節點＝數萬次全域 env 鎖（卡 t_2a983570 順車效能項）。
    let buggy = buggy_no_whitespace_normalize();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                if alt_skip_depth > 0 {
                    alt_skip_depth += 1;
                    buf.clear();
                    continue;
                }
                if is_alt_text(name) && alt_text_is_metadata(&draw_stack) {
                    alt_skip_depth = 1;
                    buf.clear();
                    continue;
                }
                if is_draw_container(name) {
                    draw_stack.push(name.to_vec());
                }
                if is_text_section(name) {
                    section_hidden_stack.push(is_section_hidden(&e));
                } else if is_text_p(name) {
                    text_p_count += 1;
                    let (own, p_props) = resolve_paragraph_style_of(&e, book);
                    style_stack.push(own);
                    p_stack.push(ParaCtx::new(own));
                    p_prop_stack.push(p_props);
                } else if is_text_h(name) {
                    text_h_count += 1;
                    let (own, p_props) = resolve_paragraph_style_of(&e, book);
                    style_stack.push(own);
                    p_stack.push(ParaCtx::new(own));
                    p_prop_stack.push(p_props);
                } else if is_span_like(name) {
                    let inherit = *style_stack.last().unwrap_or(&default_style);
                    style_stack.push(span_style_of(&e, book, inherit));
                } else if !p_stack.is_empty() && !in_hidden_section(&section_hidden_stack) {
                    let style = *style_stack.last().unwrap_or(&default_style);
                    let cur = p_stack.last_mut().expect("段落堆疊非空");
                    match name {
                        b"text:s" | b"s" => {
                            let spaces = " ".repeat(parse_space_count(&e));
                            cur.push_explicit(&spaces, style);
                        }
                        b"text:tab" | b"tab" => cur.push_explicit("\t", style),
                        b"text:line-break" | b"line-break" => cur.push_explicit("\n", style),
                        _ => {}
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                if alt_skip_depth > 0 {
                    buf.clear();
                    continue;
                }
                if is_alt_text(name) && alt_text_is_metadata(&draw_stack) {
                    buf.clear();
                    continue;
                }
                if is_text_p(name) || is_text_h(name) {
                    if is_text_p(name) {
                        text_p_count += 1;
                    } else {
                        text_h_count += 1;
                    }
                    let (own, p_props) = resolve_paragraph_style_of(&e, book);
                    let ctx = ParaCtx::new(own);
                    let para = ctx.finish();
                    total_text_len += para.text.chars().count();
                    paragraphs.push(para);
                    paragraph_properties.push(p_props);
                } else if !p_stack.is_empty() && !in_hidden_section(&section_hidden_stack) {
                    let style = *style_stack.last().unwrap_or(&default_style);
                    let cur = p_stack.last_mut().expect("段落堆疊非空");
                    match name {
                        b"text:s" | b"s" => {
                            let spaces = " ".repeat(parse_space_count(&e));
                            cur.push_explicit(&spaces, style);
                        }
                        b"text:tab" | b"tab" => cur.push_explicit("\t", style),
                        b"text:line-break" | b"line-break" => cur.push_explicit("\n", style),
                        _ => {}
                    }
                }
            }
            Ok(Event::Text(e)) if !in_hidden_section(&section_hidden_stack) => {
                if alt_skip_depth == 0 {
                    if let Some(cur) = p_stack.last_mut() {
                        if let Ok(unescaped) = e.decode().map_err(|e| e.to_string()).and_then(|d| {
                            quick_xml::escape::unescape(&d)
                                .map(|u| u.into_owned())
                                .map_err(|e| e.to_string())
                        }) {
                            let style = *style_stack.last().unwrap_or(&default_style);
                            // XML 格式化空白剝除（本卡契約）：正規化＋邊界記帳。
                            // * 正規化後為空＝整節點純格式化空白 → 只掛「節點間單空格」。
                            // * `EdgeInfo` 任一旗標＝該節點首／尾的格式化空白被剷除
                            //   （卡 t_2a983570 返修）→ 同一套 pending_gap 記帳，否則
                            //   相鄰節點文字黏連（`甲`＋尾縮排＋`乙` ⇒ `甲乙`）。
                            // * 首端記帳要在推入**之前**掛（flush_gap 於推入前落地），
                            //   尾端記帳在推入**之後**掛（留給下一節點刷）。
                            // * 純空格節點（無 CR／LF／TAB）不折疊、原樣入文。
                            // 回塞態（`ODF_WS_BUGGY`）走零正規化舊路徑，供燈 6 鑑別。
                            if buggy {
                                cur.push_str(&unescaped, style);
                            } else {
                                // 共用發射路徑（卡 t_4006056b）：正規化＋邊界記帳
                                // ＋pending_gap 記帳與 `body.rs` 同一實作
                                // （`ws::emit_text_node`——首端記帳在推入前掛、
                                // 正規化空節點只掛 gap、尾端記帳留給下一節點刷，
                                // 順序契約見 `ws` 模組文件）。
                                ws::emit_text_node(cur, &unescaped, style);
                            }
                        }
                    }
                }
            }
            // CDATA（quick-xml 0.41 起是獨立事件；卡 t_4006056b）：視同文字節點
            // 走與 `body.rs` 同一條共發射路徑。CDATA 段本就是在 XML 裡保留原文
            // 的容器，解析器不對其內容解實體 ⇒ **只 decode、不 unescape**，
            // `<![CDATA[甲 <b> & 乙]]>` 必須原樣入文。修前落 `_ => {}` 靜默丟棄。
            // 隱藏區段與替代文字跳過守則與 `Event::Text` 完全同形。
            // 回塞態 `ODF_WS_BUGGY` 的零正規化臂不擴及 CData：修前 CData 本就
            // 整段丟棄，回塞態維持「CData 不入文」以如實反映卡前語料行為。
            Ok(Event::CData(e)) if !in_hidden_section(&section_hidden_stack) => {
                if alt_skip_depth == 0 && !buggy {
                    if let Some(cur) = p_stack.last_mut() {
                        if let Ok(raw) = e.decode().map(|d| d.into_owned()) {
                            let style = *style_stack.last().unwrap_or(&default_style);
                            ws::emit_text_node(cur, &raw, style);
                        }
                    }
                }
            }
            Ok(Event::GeneralRef(e)) if !in_hidden_section(&section_hidden_stack) => {
                if alt_skip_depth == 0 {
                    if let Some(cur) = p_stack.last_mut() {
                        let resolved = crate::body::resolve_general_ref(&e)?;
                        let style = *style_stack.last().unwrap_or(&default_style);
                        // 實體引用值（如 `&nbsp;` → NBSP）是**正文字元**，不是格式化
                        // 空白，一律原樣入文（不經正規化）。
                        cur.push_entity(&resolved, style);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                if alt_skip_depth > 0 {
                    alt_skip_depth -= 1;
                    buf.clear();
                    continue;
                }
                if is_draw_container(name) {
                    draw_stack.pop();
                }
                if is_text_section(name) {
                    section_hidden_stack.pop();
                } else if is_text_p(name) || is_text_h(name) {
                    if let Some(ctx) = p_stack.pop() {
                        style_stack.pop();
                        let para = ctx.finish();
                        total_text_len += para.text.chars().count();
                        paragraphs.push(para);
                        paragraph_properties.push(p_prop_stack.pop().unwrap_or_default());
                    }
                } else if is_span_like(name) && p_stack.len() < style_stack.len() {
                    style_stack.pop();
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(OdfError::Xml(err.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(OdfProjection {
        paragraphs,
        paragraph_properties,
        total_text_len,
        text_p_count,
        text_h_count,
        // 物件獨立帳由 `project` 另行填入（`project_bytes` 純函數入口無容器，
        // 讀不到 part——恆為空，非缺陷）。頁首尾宣告面同理需容器（讀 styles.xml），
        // 由 `project` 填入。
        object_paragraphs: Vec::new(),
        object_text_len: 0,
        hf_masters: Vec::new(),
        tables: Vec::new(),
    })
}

#[inline]
fn is_text_section(qname: &[u8]) -> bool {
    qname == b"text:section" || qname == b"section"
}

#[inline]
fn in_hidden_section(section_hidden_stack: &[bool]) -> bool {
    section_hidden_stack.last().copied().unwrap_or(false)
}

/// 判定單一 `text:section` 是否隱藏：`text:is-hidden="true"` 或 `text:display="none"` 一律隱藏；`text:display="condition"`／`condition(...)` 若無條件參數則視為不顯示，若帶有可求值的顯示條件且未標註 `is-hidden="true"`，因本引擎不實作條件變數求值器，採保守做法照發以避免誤砍正文。
fn is_section_hidden(e: &BytesStart<'_>) -> bool {
    let mut display: Option<String> = None;
    let mut is_hidden: Option<String> = None;
    let mut condition: Option<String> = None;

    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"display" => display = Some(attr_value_str(&attr.value)),
            b"is-hidden" => is_hidden = Some(attr_value_str(&attr.value)),
            b"condition" => condition = Some(attr_value_str(&attr.value)),
            _ => {}
        }
    }

    if is_hidden.as_deref().map(str::trim) == Some("true") {
        return true;
    }

    let Some(disp) = display.as_deref().map(str::trim) else {
        return false;
    };

    if disp == "none" {
        return true;
    }

    let has_cond_attr = condition
        .as_deref()
        .map(str::trim)
        .is_some_and(|c| !c.is_empty());

    if disp == "condition" {
        return !has_cond_attr;
    }

    if let Some(inner) = disp
        .strip_prefix("condition(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return inner.trim().is_empty() && !has_cond_attr;
    }

    false
}

/// ODF-E2c：是否為圖形替代文字元素（svg:desc／svg:title）。
#[inline]
pub(crate) fn is_alt_text(qname: &[u8]) -> bool {
    matches!(qname, b"svg:desc" | b"desc" | b"svg:title" | b"title")
}

/// ODF-E2c：是否為繪圖容器（用於追蹤替代文字歸屬）。所有 draw:* 元素都算，
/// 另接受無前綴的常見容器名（與本檔 p/h/span 的無前綴容忍口徑一致）。
#[inline]
pub(crate) fn is_draw_container(qname: &[u8]) -> bool {
    qname.starts_with(b"draw:")
        || matches!(
            qname,
            b"frame" | b"custom-shape" | b"text-box" | b"g" | b"group"
        )
}

/// ODF-E2c：draw:text-box 直屬的替代文字屬正文（LO 會渲染），其餘繪圖容器的屬替代文字。
/// 以最近一個繪圖容器判定：text-box 內巢裝 frame 的替代文字仍歸 frame，故照跳。
#[inline]
pub(crate) fn alt_text_is_metadata(draw_stack: &[Vec<u8>]) -> bool {
    match draw_stack.last() {
        Some(name) => !matches!(name.as_slice(), b"draw:text-box" | b"text-box"),
        None => false,
    }
}

#[inline]
fn is_text_p(qname: &[u8]) -> bool {
    qname == b"text:p" || qname == b"p"
}

#[inline]
fn is_text_h(qname: &[u8]) -> bool {
    qname == b"text:h" || qname == b"h"
}

#[inline]
fn is_span_like(qname: &[u8]) -> bool {
    matches!(qname, b"text:span" | b"span" | b"text:a" | b"a")
}

/// 段落元素的 `text:style-name` → (字級, 粗體, 段落屬性) 級聯值。
fn resolve_paragraph_style_of(
    e: &BytesStart<'_>,
    book: &StyleBook,
) -> ((f32, bool), ParagraphProperties) {
    match attr_text_style_name(e) {
        Some(name) if !name.is_empty() => {
            let (sz, b, props) = book.resolve_paragraph(&name);
            ((sz, b), props)
        }
        _ => {
            let (sz, b, props) = book.resolve_paragraph("");
            ((sz, b), props)
        }
    }
}

/// span/a 元素：有宣告樣式則走級聯（三態：未宣告的屬性沿承 parent 片段，
/// 顯式宣告 normal／pt 值則壓過 inherit）；無宣告樣式名＝全段沿用。
fn span_style_of(e: &BytesStart<'_>, book: &StyleBook, inherit: (f32, bool)) -> (f32, bool) {
    match attr_text_style_name(e) {
        Some(name) if !name.is_empty() => {
            let res = book.resolve_chain(&name);
            (
                res.size_pt.unwrap_or(inherit.0),
                res.bold.unwrap_or(inherit.1),
            )
        }
        _ => inherit,
    }
}

fn attr_text_style_name(e: &BytesStart<'_>) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"style-name" {
            return Some(attr_value_str(&attr.value));
        }
    }
    None
}

fn parse_space_count(e: &BytesStart<'_>) -> usize {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"c" {
            if let Ok(s) = std::str::from_utf8(&attr.value) {
                if let Ok(n) = s.parse::<usize>() {
                    return n;
                }
            }
        }
    }
    1
}
