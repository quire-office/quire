//! ODF-C3 文字流投影 → layout 對接層（紅燈佔位實作）。
//!
//! 定位：ODF 專屬中間表示 `OdfLayoutParagraph`（自產自銷，嚴禁引用
//! `quire_ooxml::Paragraph`），把 `content.xml` 的 `text:p`／`text:h` 投影成
//! 段落流＋逐段字級/粗體，降維 `(Vec<String>, Vec<f32>)` 對接
//! `quire_layout` 既有斷行／分頁入口。
//!
//! 字級級聯（嚴格序）：content.xml automatic-style → styles.xml 具名
//! common-style → styles.xml default-style。單位只認 `pt` 結尾數值；缺省、
//! 百分比、非 pt 一律 fallback `12.0pt`。`fo:font-weight`：字串 `"bold"` 或
//! 數值 ≥700 記 bold，其餘 normal。樣式經 `style:parent-style-name` 與
//! `style:default-style-name` 鏈各最多追 8 層，防環。
//!
//! 紅燈佔位：`project*` 回傳空投影、`layout_projection` 回傳空清單——考卷
//! `core/odf/tests/odt_layout_projection.rs` 必須因此拍紅（不是編譯紅）。

use crate::OdfBundle;

/// layout 層型別再匯出：殼層（quire-cli）經本 crate 取用，不直接 import
/// quire-layout（分層鐵律：shells/native 只准直接 import engine／odf 入口）。
pub use quire_layout::LayoutParams;

/// 字級缺省 fallback（卡面口徑：缺省、百分比、非 pt 一律 12.0pt）。
pub const DEFAULT_FONT_SIZE_PT: f32 = 12.0;
/// parent-style-name／default-style-name 鏈各自的追蹤上限（防環）。
pub const MAX_STYLE_HOPS: usize = 8;

/// 碼點區間（起點含、終點不含），對齊 `quire_layout::Line::char_offset` 標尺。
pub type CpRange = (usize, usize);

/// ODF 專屬段落中間表示。`text:p` 與 `text:h` 一律入列（heading 也是一段）。
///
/// `runs`：逐個文字片段 `(碼點區間, 字級pt, 粗體)`。粗體只存進中間表示供
/// 渲染層未來消費——layout 斷行不認字重（既有事實，非缺陷）。
/// 不變量：`runs` 恆非空（至少一個覆蓋全段的代表 run）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OdfLayoutParagraph {
    /// 段落純文字（含 text:s 展開空格／text:tab／text:line-break）。
    pub text: String,
    /// 逐片段 `(碼點區間, 字級pt, 粗體)`，依段落文字流文件序。
    pub runs: Vec<(CpRange, f32, bool)>,
}

impl OdfLayoutParagraph {
    /// 該段代表字級：首 run 字級（`runs` 空＝缺省 12.0pt）。
    pub fn font_size_pt(&self) -> f32 {
        self.runs
            .first()
            .map(|r| r.1)
            .unwrap_or(DEFAULT_FONT_SIZE_PT)
    }

    /// 該段代表粗體：首 run 字重（`runs` 空＝normal）。
    pub fn bold(&self) -> bool {
        self.runs.first().map(|r| r.2).unwrap_or(false)
    }
}

/// 一份 .odt 的文字流投影結果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OdfProjection {
    /// 段落流：`text:p`＋`text:h` 一律入列（不另立 heading 計數欄）。
    pub paragraphs: Vec<OdfLayoutParagraph>,
    /// 段落文字總碼點數。
    pub total_text_len: usize,
    /// `<text:p>` 原始計數（Start＋Empty 皆計，同 body.rs／survey 口徑）。
    pub text_p_count: usize,
    /// `<text:h>` 原始計數（同上）。
    pub text_h_count: usize,
}

impl OdfProjection {
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

/// 紅燈佔位：從已開封的 ODF 封裝投影（綠燈實作後取代）。
pub fn project(bundle: &OdfBundle) -> Result<OdfProjection, crate::OdfError> {
    let _ = bundle;
    Ok(OdfProjection::default())
}

/// 紅燈佔位：直接從 content.xml／styles.xml 位元組投影（純函數入口）。
pub fn project_bytes(
    content_xml: &[u8],
    styles_xml: Option<&[u8]>,
) -> Result<OdfProjection, crate::OdfError> {
    let _ = (content_xml, styles_xml);
    Ok(OdfProjection::default())
}

/// 紅燈佔位：投影 → `quire_layout` 逐段字級斷行（既有入口
/// `layout_paragraphs_grid`；全域尺走 `layout_paragraphs_per_section`）。
pub fn layout_projection(
    projection: &OdfProjection,
    params: &LayoutParams,
) -> Vec<quire_layout::ParagraphLayout> {
    let _ = (projection, params);
    Vec::new()
}

/// 紅燈佔位：全域尺（`params.font_size_pt`）分頁斷點——每頁起排的段落 index
/// （不含首頁的 0，「斷點」語意）。對接 `quire_layout::page_breaks_per_section`。
pub fn page_break_positions(texts: &[String], params: &LayoutParams) -> Vec<usize> {
    let _ = (texts, params);
    Vec::new()
}
