//! ODF-C4 頁面幾何鏈：styles.xml 域的 `style:page-layout` ＋ `style:master-page`。
//!
//! 定位：把 ODF 的頁面幾何（`<style:page-layout>` 的紙張尺寸／邊距）經
//! `<style:master-page>` 主頁鏈解析出來，換算成 `quire_layout::LayoutParams`，
//! 讓 odt-render 脫離 `LayoutParams::default()`（假 A4 595×842＋1 吋邊距）。
//! 規範依據 ODF 1.3 Part 3 §16.5（page-layout）／§16.9（master-page）／
//! §3.15.3（命名空間分域鐵律）；實查口徑與語料實態見 `docs/odf-c4-anatomy.md`。
//!
//! ## 分域鐵律（Part 3 §3.15.3，本模組存在的核心理由）
//!
//! `content.xml` 與 `styles.xml` **各自擁有獨立命名空間的
//! `office:automatic-styles`**——兩域的 `P1` 是兩個不同樣式。因此：
//!
//! - 本模組的 [`StyleDomainPage`] 是**獨立結構**，與 C3 `layout_glue::StyleBook`
//!   （文字級聯用）完全分離：不共用 `HashMap`、不覆寫既有表。
//! - 「宣告了 `@style:master-page-name` 的段落／表格樣式」也按來源分兩張表
//!   （[`StyleDomainPage::mp_by_content_style`] ／
//!   [`StyleDomainPage::mp_by_styles_style`]），查鏈時先 content 域後 styles 域。
//!
//! ## 雙軸與後備鏈（實查結論：卡面前提需修正，見解剖報告 §2）
//!
//! 卡面寫「LO 寫檔兩軸都寫，讀取端以 `style:` 優先、`fo:` 後備」。**實測不
//! 成立**：ODF 1.3 os 與 1.4 os 的 `<style:page-layout-properties>` 官方屬性表
//! 皆只列 `fo:page-width`／`fo:page-height`（無 `style:paper-width` 欄位；
//! `style:width` 只存在於 `<style:footnote-sep>`／`<style:column-sep>`），
//! corpus-odt 50 檔 `fo:` 軸 70/70、`style:` 軸 0 檔。本模組仍照卡面實裝
//! **`style:` 軸優先、`fo:` 軸後備**的讀取鏈（對非 LO 系生產者容錯、零成本），
//! 但實態上走的是 fo: 軸——分歧如實記帳，不假裝語料有雙軸。`<style:page-margin>`
//! 子元素同理（1.3 §17.2 子元素表無此項、語料 0 檔）：照卡面實裝為優先鏈，
//! 實態走 `fo:margin-*`。
//!
//! ## 本切片不做（解剖報告 §6 記帳）
//!
//! 頁首頁尾**內容**渲染（只採存在性 bool）；`style:print-orientation` 與
//! `fo:orientation` 的歷史分歧全解（兩者只原樣採集、不據以對調 w/h——LO 寫檔
//! 時已把實際方向數值寫進 `fo:page-width/height`，實測 7 個 landscape 版面
//! 皆 w>h）；多 master-page 換頁鏈（只取文件級預設一個，其餘計數記帳）；
//! `text:section` 欄內幾何（正交機制）；註腳頁配置。

use std::collections::HashMap;

use quick_xml::events::BytesStart;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::layout_glue::MAX_STYLE_HOPS;
use crate::{OdfBundle, OdfError};

pub use quire_layout::LayoutParams;

/// cm → pt（1in = 2.54cm 精確定義，`1pt = 1/72in`）。
pub const CM_TO_PT: f32 = 72.0 / 2.54;

/// ODF 長度字串 → pt。支援 ODF 1.3 Part 3 §18.3.18 length 的全部單位：
/// `mm` `cm` `in` `pt` `pc`；容錯本地化逗號小數（`2,0cm`——ODF 語法本身是小
/// 數點，只在「逗號前後皆數字」時轉換，避免誤吃千分位寫法）。
/// 無單位／百分比／不可解析／非正數＝`None`（呼叫端老實走後備鏈）。
pub fn odf_length_to_pt(value: &str) -> Option<f32> {
    let s = value.trim();
    let normalized = normalize_decimal_comma(s);
    // 單位表（順序＝優先序：cm 先於 mm，兩者無重疊後綴）。
    const UNITS: [(&str, f32); 5] = [
        ("cm", CM_TO_PT),
        ("mm", CM_TO_PT / 10.0),
        ("in", 72.0),
        ("pt", 1.0),
        ("pc", 12.0),
    ];
    let (num, factor) = UNITS
        .iter()
        .find_map(|(suffix, f)| normalized.strip_suffix(*suffix).map(|n| (n, *f)))?;
    let v: f32 = num.trim().parse().ok()?;
    if v.is_finite() && v > 0.0 {
        Some(v * factor)
    } else {
        None
    }
}

/// `2,0cm` → `2.0cm`（僅在逗號兩側皆數字時；`1,2,3cm` 這類畸形容錯不轉換）。
fn normalize_decimal_comma(s: &str) -> std::borrow::Cow<'_, str> {
    let b = s.as_bytes();
    if !b.contains(&b',') || s.contains('.') {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    for (i, c) in b.iter().enumerate() {
        let as_comma = *c == b','
            && i > 0
            && b[i - 1].is_ascii_digit()
            && b.get(i + 1).is_some_and(|n| n.is_ascii_digit());
        out.push(if as_comma { '.' } else { *c as char });
    }
    std::borrow::Cow::Owned(out)
}

/// `@style:print-orientation`／`@fo:orientation` 的原樣採集值。本切片**不據以
/// 對調 w/h**（見模組頭「不做」清單）；只老實記錄供下游分群。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Unspecified,
    Portrait,
    Landscape,
}

fn parse_orientation(v: &str) -> Orientation {
    match v.trim().to_ascii_lowercase().as_str() {
        "landscape" => Orientation::Landscape,
        "portrait" => Orientation::Portrait,
        _ => Orientation::Unspecified,
    }
}

/// `<style:page-layout>` 一筆的頁面幾何（已換算成 pt）。
#[derive(Debug, Clone, PartialEq)]
pub struct PageLayout {
    /// `@style:name`（如 `Mpm1`）。
    pub name: String,
    /// 紙寬（pt）。取鏈：`style:` 軸優先 → `fo:` 軸後備。
    pub page_w_pt: f32,
    /// 紙高（pt）。同上取鏈。
    pub page_h_pt: f32,
    /// 四邊邊距 `[上, 右, 下, 左]`（pt），對齊 `LayoutParams::margins_pt` 欄序。
    /// 取鏈：`<style:page-margin>` 子元素優先 → `fo:margin-*` 後備。
    pub margins_pt: [f32; 4],
    /// `@style:print-orientation` 原樣值。
    pub print_orientation: Orientation,
    /// `@fo:orientation` 原樣值（1.0 相容軸，只採集不使用）。
    pub fo_orientation: Orientation,
    /// 紙張尺寸是否取自 `style:` 軸（true）或 `fo:` 軸後備（false）。
    pub size_from_style_axis: bool,
    /// 邊距是否取自 `<style:page-margin>` 子元素（true）或 `fo:margin-*`（false）。
    pub margins_from_page_margin_el: bool,
    /// 四邊邊距是否齊備（缺任一邊＝宣告不完整，呼叫端可決定回退）。
    pub margins_complete: bool,
}

/// `<style:master-page>` 一筆（頁首頁尾只採存在性 bool，內容不做）。
#[derive(Debug, Clone, PartialEq)]
pub struct MasterPage {
    /// `@style:name`（如 `Standard`）。
    pub name: String,
    /// `@style:page-layout-name`（指向 `page_layouts`；缺／空＝None）。
    pub page_layout_name: Option<String>,
    /// `@style:next-style-name`（自翻頁鏈，本切片只採集不追）。
    pub next_style_name: Option<String>,
    /// 有無任何頁首宣告（`style:header`／`-first`／`-left`）。
    pub has_header: bool,
    /// 有無任何頁尾宣告（`style:footer`／`-first`／`-left`）。
    pub has_footer: bool,
}

/// 頁面域樣式簿（**分域**：與 C3 `StyleBook` 分離的獨立結構）。
#[derive(Debug, Clone, Default)]
pub struct StyleDomainPage {
    /// styles.xml `office:automatic-styles` 的 `<style:page-layout>`（鍵＝`@style:name`）。
    pub page_layouts: HashMap<String, PageLayout>,
    /// `<office:master-styles>` 的 `<style:master-page>`（文件序）。
    pub master_pages: Vec<MasterPage>,
    /// `<office:master-styles>` 首個 master-page 名（文件級預設缺省綁定）。
    pub master_styles_first: Option<String>,
    /// **content.xml 域**：宣告 `@style:master-page-name` 的樣式（名 → master-page 名）。
    pub mp_by_content_style: HashMap<String, String>,
    /// **styles.xml 域**：同上（獨立一張表，禁與 content 域混表）。
    pub mp_by_styles_style: HashMap<String, String>,
    /// content.xml 域 `@style:parent-style-name` 鏈。
    parent_content: HashMap<String, String>,
    /// styles.xml 域 `@style:parent-style-name` 鏈。
    parent_styles: HashMap<String, String>,
}

impl StyleDomainPage {
    /// 依 `@style:page-layout-name` 取版面（缺／查無＝None）。
    pub fn layout_of_master(&self, mp: &MasterPage) -> Option<&PageLayout> {
        mp.page_layout_name
            .as_deref()
            .and_then(|n| self.page_layouts.get(n))
    }

    /// 某樣式名的有效 `@style:master-page-name`（先 content 域後 styles 域；
    /// 各域內沿 parent 鏈最多 `MAX_STYLE_HOPS` 層並防環）。
    pub fn master_page_of_style(&self, style: &str) -> Option<String> {
        self.walk(style, &self.mp_by_content_style, &self.parent_content)
            .or_else(|| self.walk(style, &self.mp_by_styles_style, &self.parent_styles))
    }

    fn walk(
        &self,
        style: &str,
        table: &HashMap<String, String>,
        parents: &HashMap<String, String>,
    ) -> Option<String> {
        if style.is_empty() {
            return None;
        }
        let mut cur = style.to_string();
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..MAX_STYLE_HOPS {
            if seen.iter().any(|s| s == &cur) {
                return None; // 防環
            }
            seen.push(cur.clone());
            if let Some(m) = table.get(&cur) {
                return Some(m.clone());
            }
            let Some(next) = parents.get(&cur) else {
                return None; // 鏈到此為止，該域查無宣告
            };
            cur = next.clone();
        }
        None
    }
}

/// 一份 ODT 的文件級頁面幾何解析結果。
#[derive(Debug, Clone, PartialEq)]
pub struct OdfPageGeometry {
    /// 生效版面（None＝解不到可用 page-layout，呼叫端走 LO 預設頁）。
    pub layout: Option<PageLayout>,
    /// 生效 master-page 名（解得到時）。
    pub master_page_name: Option<String>,
    /// 生效 master-page 的 `@style:page-layout-name`。
    pub page_layout_name: Option<String>,
    /// 生效原因（鑑別力依據：區分「真讀到」與「回退預設」）。
    pub source: GeometrySource,
    /// styles.xml 有宣告但本切片不消費的其餘 master-page 數（多 master-page
    /// 換頁鏈未做＝記帳）。
    pub unparsed_master_pages: usize,
}

/// 頁面幾何生效原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometrySource {
    /// 首段／首表格樣式（含 parent 鏈）宣告 `@style:master-page-name` → 該主頁。
    ParaMasterPageName,
    /// 無宣告 → `Standard` 或 `<office:master-styles>` 首個主頁。
    DefaultMasterStyles,
    /// 解到主頁但其 `page-layout-name` 缺／查無定義 → 回退 LO 預設頁。
    MasterPageWithoutLayout,
    /// 完全無主頁／無 styles.xml → 回退 LO 預設頁。
    NoMasterPage,
}

impl GeometrySource {
    /// CLI／JSON 用的穩定字串識別碼（考卷與 odt-render 出口共用，改名即破壞
    /// 鑑別力判據，不得隨意更動）。
    pub fn as_str(self) -> &'static str {
        match self {
            GeometrySource::ParaMasterPageName => "para_master_page_name",
            GeometrySource::DefaultMasterStyles => "default_master_styles",
            GeometrySource::MasterPageWithoutLayout => "master_page_without_layout",
            GeometrySource::NoMasterPage => "no_master_page",
        }
    }

    /// 是否屬「回退 LO 預設頁」這一類（未讀到可用 page-layout）。
    pub fn is_fallback(self) -> bool {
        matches!(
            self,
            GeometrySource::MasterPageWithoutLayout | GeometrySource::NoMasterPage
        )
    }
}

/// LO（LibreOffice Writer）在**完全無頁面宣告**時使用的預設頁：A4 portrait、
/// 四邊 2.0cm。數值以語料／探針 LO PDF 真值實測覆核（解剖報告 §5，非憑印象）：
///
/// - 紙張 `21.001cm × 29.7cm` = `595.3031pt × 841.8898pt`。LO 的 A4 常數以
///   `1/100mm` 整數存放（21001×29700），故**不等於** `LayoutParams::default()`
///   的 595×842——這 0.30／0.11pt 差是「真讀到 LO 口徑」的判據之一（LO PDF
///   `page.rect` 實測 A4 檔 consistently `595.304 × 841.890`）。
/// - 邊距四邊 `2.0cm` = `56.6929pt`：探針檔
///   `core/odf/tests/fixtures/c4_no_page_layout.odt`（styles.xml 無任何
///   `style:page-layout`）經 `soffice --convert-to pdf` 實測文字左緣
///   `x0 = 56.80pt`、頁寬 `595.304`——與 2.0cm／A4 吻合（差 0.11pt 為 pymupdf
///   量化噪聲），排除「LO 預設邊距是 1 吋（72pt）」的 `LayoutParams::default()`
///   舊值。
pub const LO_DEFAULT_PAGE_W_PT: f32 = 21.001 * CM_TO_PT;
pub const LO_DEFAULT_PAGE_H_PT: f32 = 29.7 * CM_TO_PT;
pub const LO_DEFAULT_MARGIN_CM: f32 = 2.0;
pub const LO_DEFAULT_MARGIN_PT: f32 = LO_DEFAULT_MARGIN_CM * CM_TO_PT;

impl OdfPageGeometry {
    /// LO 預設頁（無 page-layout 檔的回退值）。
    pub fn lo_default() -> Self {
        Self {
            layout: None,
            master_page_name: None,
            page_layout_name: None,
            source: GeometrySource::NoMasterPage,
            unparsed_master_pages: 0,
        }
    }

    /// 生效紙寬（pt）。
    pub fn page_w_pt(&self) -> f32 {
        self.layout
            .as_ref()
            .map(|l| l.page_w_pt)
            .unwrap_or(LO_DEFAULT_PAGE_W_PT)
    }

    /// 生效紙高（pt）。
    pub fn page_h_pt(&self) -> f32 {
        self.layout
            .as_ref()
            .map(|l| l.page_h_pt)
            .unwrap_or(LO_DEFAULT_PAGE_H_PT)
    }

    /// 生效四邊邊距 `[上, 右, 下, 左]`（pt）。
    pub fn margins_pt(&self) -> [f32; 4] {
        self.layout
            .as_ref()
            .filter(|l| l.margins_complete)
            .map(|l| l.margins_pt)
            .unwrap_or([LO_DEFAULT_MARGIN_PT; 4])
    }

    /// 是否使用 LO 預設頁（未讀到任何可用 page-layout）。
    pub fn is_lo_default(&self) -> bool {
        self.layout.is_none()
    }

    /// 換算成 `quire_layout::LayoutParams`：只覆寫頁面幾何軸（紙寬高＋四邊
    /// 邊距），其餘欄位（字級 12pt／行距 1.4／註腳與頁首預留表）沿用既有
    /// `Default` 口徑——本卡不動那些軸（零回退鐵閘）。
    pub fn to_layout_params(&self) -> LayoutParams {
        LayoutParams {
            page_w_pt: self.page_w_pt(),
            page_h_pt: self.page_h_pt(),
            margins_pt: self.margins_pt(),
            ..Default::default()
        }
    }
}

/// 從已開封封裝解析頁面幾何（走 ODF-E1 加密閘門：styles.xml／content.xml
/// 任一宣告加密者回 `Err(OdfError::Encrypted)`，不把密文當 XML 硬 parse）。
pub fn page_geometry(bundle: &OdfBundle) -> Result<OdfPageGeometry, OdfError> {
    bundle.ensure_not_encrypted("styles.xml")?;
    bundle.ensure_not_encrypted("content.xml")?;
    let content = bundle.read_entry("content.xml")?;
    let styles = match bundle.read_entry("styles.xml") {
        Ok(b) => Some(b),
        Err(OdfError::MissingEntry { .. }) => None,
        Err(e) => return Err(e),
    };
    page_geometry_bytes(&content, styles.as_deref())
}

/// 純函數入口。**安全前置條件**（同 `layout_glue::project_bytes`，ODF-E1
/// review）：呼叫端必須保證傳入位元組為**明文** XML；本入口不接收
/// `Manifest`、無加密校驗防線。持 `OdfBundle` 者嚴禁直呼，須經
/// [`page_geometry`]。
pub fn page_geometry_bytes(
    content_xml: &[u8],
    styles_xml: Option<&[u8]>,
) -> Result<OdfPageGeometry, OdfError> {
    let mut dom = StyleDomainPage::default();
    if let Some(sx) = styles_xml {
        parse_page_styles_file(sx, &mut dom)?;
    }
    parse_master_refs_file(content_xml, &mut dom)?;
    Ok(resolve_geometry(&dom, content_xml))
}

/// 主頁鏈決策（Part 3 §16.9／設計書 §4.2 軸線 A）：
///
/// 1. 首段（或首表格）樣式（含 parent 鏈）宣告非空 `@style:master-page-name`
///    → 該 master-page（ODF 1.3 §19.501：宣告即換頁，本切片只做幾何、不做
///    分頁鏈，換頁觸發屬另卡）。
/// 2. 否則 → 名為 `Standard` 的 master-page（若宣告）；否則
///    `<office:master-styles>` 首個 master-page。
/// 3. 主頁名宣告了但查無定義 → 退回檔層缺省（首個 master-page）。
/// 4. 完全無 master-page／無 styles.xml → LO 預設頁。
///
/// 多 master-page 文件（橫向章節）本切片只取文件級預設一個，其餘計數進
/// `unparsed_master_pages`。
fn resolve_geometry(dom: &StyleDomainPage, content_xml: &[u8]) -> OdfPageGeometry {
    let unparsed = dom.master_pages.len().saturating_sub(1);
    let first_style = first_flow_style_name(content_xml).unwrap_or_default();

    let mut source = GeometrySource::DefaultMasterStyles;
    let mut chosen: Option<String> = dom
        .master_page_of_style(&first_style)
        .filter(|s| !s.is_empty());
    if chosen.is_some() {
        source = GeometrySource::ParaMasterPageName;
    } else {
        chosen = dom
            .master_pages
            .iter()
            .find(|m| m.name == "Standard")
            .map(|m| m.name.clone())
            .or_else(|| dom.master_styles_first.clone())
            .or_else(|| dom.master_pages.first().map(|m| m.name.clone()));
    }

    let Some(name) = chosen else {
        return OdfPageGeometry {
            layout: None,
            master_page_name: None,
            page_layout_name: None,
            source: GeometrySource::NoMasterPage,
            unparsed_master_pages: unparsed,
        };
    };

    let mp = match dom.master_pages.iter().find(|m| m.name == name) {
        Some(m) => m,
        // 主頁名查無定義（設計書 §5.1「主頁樣式缺失＝忽略該次主頁切換，維持
        // 當前頁面幾何」＝退回檔層缺省）。
        None => match dom.master_pages.first() {
            Some(m) => {
                source = GeometrySource::DefaultMasterStyles;
                m
            }
            None => {
                return OdfPageGeometry {
                    layout: None,
                    master_page_name: None,
                    page_layout_name: None,
                    source: GeometrySource::NoMasterPage,
                    unparsed_master_pages: unparsed,
                }
            }
        },
    };

    match dom.layout_of_master(mp) {
        Some(l) => OdfPageGeometry {
            layout: Some(l.clone()),
            master_page_name: Some(mp.name.clone()),
            page_layout_name: mp.page_layout_name.clone(),
            source,
            unparsed_master_pages: unparsed,
        },
        None => OdfPageGeometry {
            layout: None,
            master_page_name: Some(mp.name.clone()),
            page_layout_name: mp.page_layout_name.clone(),
            source: GeometrySource::MasterPageWithoutLayout,
            unparsed_master_pages: unparsed,
        },
    }
}

// ---------------------------------------------------------------------------
// styles.xml 走查（page-layout ＋ master-page）
// ---------------------------------------------------------------------------

/// 進行中 `<style:page-layout>` 的收集器（子元素 page-layout-properties／
/// page-margin 逐筆餵入，End 時入簿）。
#[derive(Debug, Default)]
struct PendingLayout {
    name: String,
    style_w: Option<f32>,
    style_h: Option<f32>,
    fo_w: Option<f32>,
    fo_h: Option<f32>,
    /// `[上, 右, 下, 左]`（fo:margin-* 軸）。
    fo_margins: [Option<f32>; 4],
    /// `[上, 右, 下, 左]`（`<style:page-margin>` 子元素軸，優先）。
    pm_margins: [Option<f32>; 4],
    print_orientation: Orientation,
    fo_orientation: Orientation,
}

impl PendingLayout {
    fn finish(self, name: String) -> Option<PageLayout> {
        let (w, from_style) = match (self.style_w, self.fo_w) {
            (Some(w), _) => (Some(w), true),
            (None, Some(w)) => (Some(w), false),
            (None, None) => (None, false),
        };
        let h = match (self.style_h, self.fo_h) {
            (Some(h), _) => Some(h),
            (None, Some(h)) => Some(h),
            (None, None) => None,
        };
        let (w, h) = match (w, h) {
            (Some(w), Some(h)) => (w, h),
            // 紙張尺寸解不出＝這筆版面不可用（老實：不產 0×0 假頁）。
            _ => return None,
        };
        // 邊距逐邊取鏈：page-margin 子元素優先，fo:margin 後備。
        let mut margins = [0.0f32; 4];
        let mut complete = true;
        let mut from_el = false;
        for (dst, (pm, fo)) in margins.iter_mut().zip(
            self.pm_margins
                .iter()
                .copied()
                .zip(self.fo_margins.iter().copied()),
        ) {
            let v = match (pm, fo) {
                (Some(v), _) => {
                    from_el = true;
                    Some(v)
                }
                (None, Some(v)) => Some(v),
                (None, None) => None,
            };
            match v {
                Some(v) => *dst = v,
                None => complete = false,
            }
        }
        Some(PageLayout {
            name,
            page_w_pt: w,
            page_h_pt: h,
            margins_pt: margins,
            print_orientation: self.print_orientation,
            fo_orientation: self.fo_orientation,
            size_from_style_axis: from_style,
            margins_from_page_margin_el: from_el,
            margins_complete: complete,
        })
    }
}

/// 收 styles.xml 的頁面域樣式：`page-layout` 只收 `office:automatic-styles`
/// 內、`master-page` 只收 `office:master-styles` 內（分域鐵律的走查層實作）。
fn parse_page_styles_file(bytes: &[u8], dom: &mut StyleDomainPage) -> Result<(), OdfError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();

    let mut depth_automatic = 0usize;
    let mut depth_master_styles = 0usize;
    let mut pending: Option<PendingLayout> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                match name {
                    b"office:automatic-styles" => depth_automatic += 1,
                    b"office:master-styles" => depth_master_styles += 1,
                    b"style:page-layout" | b"page-layout" => {
                        if depth_automatic > 0 && pending.is_none() {
                            pending = Some(PendingLayout {
                                name: attr_local(&e, b"name").unwrap_or_default(),
                                ..Default::default()
                            });
                        }
                    }
                    b"style:page-layout-properties" | b"page-layout-properties" => {
                        if let Some(p) = pending.as_mut() {
                            apply_page_layout_props(&e, p);
                        }
                    }
                    b"style:page-margin" | b"page-margin" => {
                        if let Some(p) = pending.as_mut() {
                            apply_page_margin_el(&e, p);
                        }
                    }
                    b"style:style" | b"style" => {
                        collect_style_master_ref(
                            &e,
                            &mut dom.mp_by_styles_style,
                            &mut dom.parent_styles,
                        );
                    }
                    b"style:master-page" | b"master-page" => {
                        push_master_page(&e, dom, depth_master_styles);
                    }
                    b"style:header"
                    | b"header"
                    | b"style:header-first"
                    | b"header-first"
                    | b"style:header-left"
                    | b"header-left" => {
                        mark_header_footer(dom, depth_master_styles, true);
                    }
                    b"style:footer"
                    | b"footer"
                    | b"style:footer-first"
                    | b"footer-first"
                    | b"style:footer-left"
                    | b"footer-left" => {
                        mark_header_footer(dom, depth_master_styles, false);
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                match name {
                    b"style:page-layout-properties" | b"page-layout-properties" => {
                        if let Some(p) = pending.as_mut() {
                            apply_page_layout_props(&e, p);
                        }
                    }
                    b"style:page-margin" | b"page-margin" => {
                        if let Some(p) = pending.as_mut() {
                            apply_page_margin_el(&e, p);
                        }
                    }
                    b"style:master-page" | b"master-page" => {
                        push_master_page(&e, dom, depth_master_styles);
                    }
                    b"style:style" | b"style" => {
                        collect_style_master_ref(
                            &e,
                            &mut dom.mp_by_styles_style,
                            &mut dom.parent_styles,
                        );
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                match name {
                    b"office:automatic-styles" => {
                        depth_automatic = depth_automatic.saturating_sub(1)
                    }
                    b"office:master-styles" => {
                        depth_master_styles = depth_master_styles.saturating_sub(1)
                    }
                    b"style:page-layout" | b"page-layout" => {
                        if let Some(p) = pending.take() {
                            let name = p.name.clone();
                            if name.is_empty() {
                                continue; // 無名版面不可引用（page-layout-name 查不到）
                            }
                            if let Some(rec) = p.finish(name) {
                                dom.page_layouts.insert(rec.name.clone(), rec);
                            }
                        }
                    }
                    _ => {}
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

fn push_master_page(e: &BytesStart<'_>, dom: &mut StyleDomainPage, depth_master_styles: usize) {
    let mp = MasterPage {
        name: attr_local(e, b"name").unwrap_or_default(),
        page_layout_name: non_empty(attr_local(e, b"page-layout-name")),
        next_style_name: non_empty(attr_local(e, b"next-style-name")),
        has_header: false,
        has_footer: false,
    };
    if depth_master_styles > 0 && dom.master_styles_first.is_none() {
        dom.master_styles_first = Some(mp.name.clone());
    }
    dom.master_pages.push(mp);
}

/// 頁首頁尾存在性（只採 bool 標在最近一筆 master-page 上，內容不做）。
fn mark_header_footer(dom: &mut StyleDomainPage, depth_master_styles: usize, header: bool) {
    if depth_master_styles == 0 {
        return;
    }
    if let Some(last) = dom.master_pages.last_mut() {
        if header {
            last.has_header = true;
        } else {
            last.has_footer = true;
        }
    }
}

/// `style:style`：`@style:master-page-name` 進分域表 ＋ `@style:parent-style-name`
/// 進該域 parent 鏈（主頁名要能沿 parent 鏈繼承）。
fn collect_style_master_ref(
    e: &BytesStart<'_>,
    table: &mut HashMap<String, String>,
    parents: &mut HashMap<String, String>,
) {
    let Some(sname) = attr_local(e, b"name") else {
        return;
    };
    if sname.is_empty() {
        return;
    }
    if let Some(mp) = attr_local(e, b"master-page-name") {
        if !mp.is_empty() {
            table.insert(sname.clone(), mp);
        }
    }
    if let Some(p) = attr_local(e, b"parent-style-name") {
        if !p.is_empty() {
            parents.insert(sname, p);
        }
    }
}

/// `<style:page-layout-properties>` 屬性分派。
///
/// 雙軸（卡面口徑）：`style:paper-width`／`style:paper-height`（新軸，local
/// name 為 `paper-*`）優先，`fo:page-width`／`fo:page-height`（1.0 相容軸）後備。
/// 另容錯 `style:width`／`style:height`（local name `width`／`height`——該元素
/// 的官方屬性表無此二名，出現者一律歸 style: 軸候選，不與 fo: 軸衝突）。
fn apply_page_layout_props(e: &BytesStart<'_>, p: &mut PendingLayout) {
    for attr in e.attributes().flatten() {
        let local = attr.key.local_name();
        let local = local.as_ref();
        let v = attr_value_str(&attr.value);
        match local {
            b"paper-width" | b"width" => {
                p.style_w = odf_length_to_pt(&v).or(p.style_w);
            }
            b"paper-height" | b"height" => {
                p.style_h = odf_length_to_pt(&v).or(p.style_h);
            }
            b"page-width" => p.fo_w = odf_length_to_pt(&v).or(p.fo_w),
            b"page-height" => p.fo_h = odf_length_to_pt(&v).or(p.fo_h),
            b"margin-top" => p.fo_margins[0] = odf_length_to_pt(&v).or(p.fo_margins[0]),
            b"margin-right" => p.fo_margins[1] = odf_length_to_pt(&v).or(p.fo_margins[1]),
            b"margin-bottom" => p.fo_margins[2] = odf_length_to_pt(&v).or(p.fo_margins[2]),
            b"margin-left" => p.fo_margins[3] = odf_length_to_pt(&v).or(p.fo_margins[3]),
            b"print-orientation" => p.print_orientation = parse_orientation(&v),
            b"orientation" => p.fo_orientation = parse_orientation(&v),
            _ => {}
        }
    }
}

/// `<style:page-margin>` 子元素（新式邊距軸，優先於 `fo:margin-*`；語料 0 檔，
/// defensive 實裝）。容錯兩種鍵名：`style:margin-top` 與 `style:top`。
fn apply_page_margin_el(e: &BytesStart<'_>, p: &mut PendingLayout) {
    for attr in e.attributes().flatten() {
        let local = attr.key.local_name();
        let local = local.as_ref();
        let v = odf_length_to_pt(&attr_value_str(&attr.value));
        let Some(v) = v else { continue };
        match local {
            b"margin-top" | b"top" => p.pm_margins[0] = Some(v),
            b"margin-right" | b"right" => p.pm_margins[1] = Some(v),
            b"margin-bottom" | b"bottom" => p.pm_margins[2] = Some(v),
            b"margin-left" | b"left" => p.pm_margins[3] = Some(v),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// content.xml 走查（content 域 master-page-name ＋ 首段／首表格樣式名）
// ---------------------------------------------------------------------------

/// 收 content.xml `office:automatic-styles` 內宣告 `@style:master-page-name`
/// 的樣式（分域表之一）與 parent 鏈。
fn parse_master_refs_file(bytes: &[u8], dom: &mut StyleDomainPage) -> Result<(), OdfError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();
    let mut depth_automatic = 0usize;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                if name == b"office:automatic-styles" {
                    depth_automatic += 1;
                } else if (name == b"style:style" || name == b"style") && depth_automatic > 0 {
                    collect_style_master_ref(
                        &e,
                        &mut dom.mp_by_content_style,
                        &mut dom.parent_content,
                    );
                }
            }
            Ok(Event::Empty(e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                if (name == b"style:style" || name == b"style") && depth_automatic > 0 {
                    collect_style_master_ref(
                        &e,
                        &mut dom.mp_by_content_style,
                        &mut dom.parent_content,
                    );
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"office:automatic-styles" {
                    depth_automatic = depth_automatic.saturating_sub(1);
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

/// 文件「首段（或首表格）」的 `@text:style-name`：設計書 §4.2 口徑＝**頂層**
/// 第一個 `text:p`／`text:h`／`table:table`（Start 與 Empty 皆算）；巢裝在
/// `text:section`／`table:table` 內的不算（其宿主才是文件首流程物件）。有元素
/// 但無宣告樣式名＝空字串；完全無此三者＝None。
fn first_flow_style_name(content_xml: &[u8]) -> Option<String> {
    let mut reader = Reader::from_reader(content_xml);
    let mut buf = Vec::new();
    let mut depth_nested = 0usize; // text:section／table:table 巢裝深度
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                if name == b"text:section" || name == b"section" {
                    depth_nested += 1;
                    continue;
                }
                let is_table = name == b"table:table" || name == b"table";
                let is_para = matches!(name, b"text:p" | b"p" | b"text:h" | b"h");
                if (is_para || is_table) && depth_nested == 0 {
                    return Some(attr_local(&e, b"style-name").unwrap_or_default());
                }
                if is_table {
                    depth_nested += 1; // 巢裝表格（表格內段落不是首段）
                }
            }
            Ok(Event::Empty(e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                let is_table = name == b"table:table" || name == b"table";
                let is_para = matches!(name, b"text:p" | b"p" | b"text:h" | b"h");
                if (is_para || is_table) && depth_nested == 0 {
                    return Some(attr_local(&e, b"style-name").unwrap_or_default());
                }
            }
            Ok(Event::End(e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                if name == b"text:section"
                    || name == b"section"
                    || name == b"table:table"
                    || name == b"table"
                {
                    depth_nested = depth_nested.saturating_sub(1);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

#[inline]
fn attr_local(e: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == key {
            return Some(attr_value_str(&attr.value));
        }
    }
    None
}

#[inline]
fn attr_value_str(v: &[u8]) -> String {
    std::str::from_utf8(v).unwrap_or("").to_string()
}

#[inline]
fn non_empty(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.is_empty())
}
