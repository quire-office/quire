//! ODF-E2d：styles.xml master-page 頁首／頁尾**段落內容**的宣告面與發射面。
//!
//! 缺陷（真值源 `docs/odf-survey/e2-flow-attribution.md` §2 D4／§3.4／§3.5，卡
//! t_b1c58929）：`project()` 讀 styles.xml 只為字級級聯，master-page 的
//! `style:header`／`style:footer` 段落內容一個不發，而 LO 每頁渲染。ODF-C4 明文
//! 「頁首頁尾內容不做（寫進報告記帳）」——本模組把該記帳缺口落地。
//!
//! ## 兩張面（各自獨立入帳，宿主段落流零污染）
//!
//! * **宣告面** [`OdfProjection::hf_masters`]：styles.xml `office:master-styles`
//!   內每筆 master-page 的六種槽位（header／-first／-left × footer／-first／-left）
//!   之 `text:p`／`text:h` 逐段投影。三條實測口徑（LO PDF 覆核，明细見考卷
//!   `tests/odf_e2d_header_footer.rs` 檔頭）：
//!   1. **含巢裝表格／文字框內段落**，但每段只取**自身 inline 文字**（巢裝段落
//!      另筆、文件序 container-before-inner）。tdf102466 頁首是
//!      `text:p > draw:custom-shape > loext:table > loext:table-cell > text:p`，
//!      LO PDF 實測渲染格內文字（`Especificación`／`Serviweb` 命中）——巢裝段落
//!      必發；而外層段落的 itertext 會把巢裝文字重複計一次（探針實測恰 2 倍），
//!      故嚴禁用 itertext 口徑。
//!   2. **`svg:desc`／`svg:title` 零發射**（圖形替代文字，LO 永不渲染＝E2 報告
//!      D3 同律）。鐵證 fdo78420：頁首 `svg:desc` 字面 `iveco`／`press release`
//!      在 LO PDF 零命中（大寫 `Iveco`／`IVECO` 命中全部來自內文）。
//!   3. **欄位節點取快取值字面**（`text:page-number` 等節點內文字照收；LO 重算
//!      屬 `docs/odf-survey/README.md` §11.1 合成軸），元素 **tail 文字必收**
//!      （FDO76312 頁尾 `Page 1 of 2` ＝ `Page `＋快取 `1`＋tail `of `＋快取 `2`
//!      ——E2 探針正是漏在 tail）。
//!      字級級聯＝styles.xml **automatic-styles**（HF 段落樣式 `MP1`/`MP3` 在此
//!      層，不在 `office:styles`）→ `office:styles` 具名（parent 鏈）→
//!      default-style(paragraph) → default-style(text) → 12.0pt，只認 `pt`。
//! * **發射面** [`hf_assignment`]：逐頁指派生效主頁＋頁首尾段。分段依據 ODF 1.3
//!   §19.501「段落樣式宣告 `@style:master-page-name` ⇒ 該段落處換頁」（LO 真值：
//!   tdf124637 四個宣告段落 ⇒ LO 恰 5 頁，修復前我方只 1 頁），chunk 內再用
//!   `quire_layout` 容量分頁（頁寬高與四邊邊距取該主頁的 page-layout——C4 鏈）。
//!
//! ## 頁槽選用三律（全部 LO PDF 實抓定案）
//!
//! 1. **`-first`**：主頁**首次使用**的首頁若宣告 `-first` 槽即採用，**即使內容為
//!    空 ⇒ 零發射，不退 plain**（FDO76312 頁 0 頂／底帶零文字、正頁首文字只在頁
//!    1；fdo78420 頁 0 頁尾發 `-first` 的 `1`，非 plain 的 `12`）。同一主頁在多
//!    主頁鏈中再度啟用按「非首次」走 plain（語料 0 例可分辨，老實記帳）。
//! 2. **`-left` 鏡像**：全域頁序（0 起）的**奇數頁**＝邏輯偶數頁（左頁）宣告
//!    `-left` 槽即採用，未宣告退 plain（tdf123705 實測 p0/p2 吃 plain
//!    `Rev 11/22/2017…`、p1/p3 吃 left `11/22/2017…`，p1/p3 頁首零發射＝left 槽
//!    空宣告）。全語料 `style:mirror` 宣告 0 檔而 LO 仍按偶數頁吃 `-left`，故
//!    我方**不以 mirror 宣告為前提**（LO 讀端口徑，實測覆核）。
//! 3. **空文字段不發**（LO 不渲染零文字段）；`\t` 段**有**文字照發（tab 是字元；
//!    lvlPicBulletId 頁尾 `"\t\t"`——PDF 抽取把全空白行丟掉是抽取口徑，不是渲染
//!    口徑）。槽位存在性由宣告面保留，發射面才濾。
//!
//! ## 宿主零污染鐵律（E2e 同款紀律）
//!
//! 頁首尾段**不進** `paragraphs`／`flatten()`／`total_text_len`／`page_breaks`，
//! 宿主計數口徑逐位不動（考卷燈 6 釘死），發射面另立 [`HfAssignment`] 獨立帳。

use quick_xml::events::BytesStart;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::layout_glue::{
    parse_style_file, LayoutParams, OdfLayoutParagraph, OdfProjection, StyleBook, StyleScope,
};
use crate::page_geometry::{
    default_master_page_name, style_domain_page, StyleDomainPage, LO_DEFAULT_MARGIN_PT,
    LO_DEFAULT_PAGE_H_PT, LO_DEFAULT_PAGE_W_PT,
};
use crate::{OdfBundle, OdfError};

// ---------------------------------------------------------------------------
// 宣告面型別
// ---------------------------------------------------------------------------

/// 六種槽位別。`as_str` 是基準檔 JSON 的穩定鍵（考卷與燈 7 對帳依歸）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterHfSlotKind {
    Header,
    HeaderFirst,
    HeaderLeft,
    Footer,
    FooterFirst,
    FooterLeft,
}

impl MasterHfSlotKind {
    /// JSON／CLI 用的穩定字串識別碼。
    pub fn as_str(self) -> &'static str {
        match self {
            MasterHfSlotKind::Header => "header",
            MasterHfSlotKind::HeaderFirst => "header-first",
            MasterHfSlotKind::HeaderLeft => "header-left",
            MasterHfSlotKind::Footer => "footer",
            MasterHfSlotKind::FooterFirst => "footer-first",
            MasterHfSlotKind::FooterLeft => "footer-left",
        }
    }

    /// 是否屬頁尾三兄弟（發射面分頁首組／頁尾組用）。
    pub fn is_footer(self) -> bool {
        matches!(
            self,
            MasterHfSlotKind::Footer | MasterHfSlotKind::FooterFirst | MasterHfSlotKind::FooterLeft
        )
    }
}

/// 一個槽位的宣告。段落**含空文字段**（宣告面照收，發射面才濾）。
#[derive(Debug, Clone, PartialEq)]
pub struct HfSlot {
    pub kind: MasterHfSlotKind,
    /// 槽位內逐段（文件序 container-before-inner，含巢裝表格／文字框內段落）。
    pub paragraphs: Vec<OdfLayoutParagraph>,
}

/// 一筆 `<style:master-page>` 的頁首尾宣告。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MasterHf {
    /// `@style:name`。
    pub name: String,
    /// 宣告的槽位（文件序）。
    pub slots: Vec<HfSlot>,
}

impl MasterHf {
    /// 取某槽別宣告。`None`＝**沒宣告**（不同於「宣告了但內容為空」——這個
    /// 區分是 `-first` 律的判準：宣告了空槽 ⇒ 該頁零發射，不退 plain）。
    pub fn slot(&self, kind: MasterHfSlotKind) -> Option<&HfSlot> {
        self.slots.iter().find(|s| s.kind == kind)
    }
}

/// 回塞態旗標：`ODF_E2D_BUGGY=1` ⇒ 主頁鏈不分段（全文件只用文件級主頁——缺陷的
/// 相反極端之一），供考卷燈 8 證「§19.501 分段鏈真的在被檢查」。正常態永不觸發。
pub fn buggy_single_master() -> bool {
    std::env::var_os("ODF_E2D_BUGGY").is_some()
}

// ---------------------------------------------------------------------------
// 宣告面（styles.xml `office:master-styles` 走查）
// ---------------------------------------------------------------------------

/// 頁首尾專用字級簿：styles.xml `office:styles` 具名＋default-style 兩族，**外加**
/// styles.xml 自己的 `office:automatic-styles`（HF 段落樣式 `MP1`/`MP3` 宣告在那
/// 層；C3 的 `StyleScope::StylesXml` 不收，故另立 scope 而非改既有行為）。
/// 級聯序與獨立第二雙眼基準 `tests/odf_e2d_hf_truth.py` 逐字一致。
fn hf_style_book(styles_xml: &[u8]) -> Result<StyleBook, OdfError> {
    let mut book = StyleBook::default();
    parse_style_file(styles_xml, &mut book, StyleScope::StylesXml)?;
    parse_style_file(styles_xml, &mut book, StyleScope::StylesXmlAutomatic)?;
    Ok(book)
}

/// 宣告面入口（走 ODF-E1 加密閘門）：styles.xml 缺如＝空宣告（零主頁，非錯）。
pub fn hf_masters(bundle: &OdfBundle) -> Result<Vec<MasterHf>, OdfError> {
    bundle.ensure_not_encrypted("styles.xml")?;
    let styles = match bundle.read_entry("styles.xml") {
        Ok(b) => b,
        Err(OdfError::MissingEntry { .. }) => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    collect_declaration(&styles)
}

/// 純函數入口（**安全前置條件**同 `layout_glue::project_bytes`：呼叫端必須保證
/// 傳入為明文 XML；持 `OdfBundle` 者須經 [`hf_masters`]）。
pub fn collect_declaration(styles_xml: &[u8]) -> Result<Vec<MasterHf>, OdfError> {
    let book = hf_style_book(styles_xml)?;
    let mut reader = Reader::from_reader(styles_xml);
    let mut buf = Vec::new();

    let mut masters: Vec<MasterHf> = Vec::new();
    let mut depth_master_styles = 0usize;
    let mut current: Option<MasterHf> = None;
    let mut open_slot: Option<MasterHfSlotKind> = None;
    let mut slot_paras: Vec<OdfLayoutParagraph> = Vec::new();
    // 進行中段落：(在 slot_paras 的預留位、累積文字、段落級字級/字重)。預留位
    // 讓「容器段落先於其巢裝段落」的入列序與 ElementTree.iter() 文件序一致。
    let mut open_paras: Vec<(usize, String, (f32, bool))> = Vec::new();
    let mut skip_alt_text = 0usize; // svg:desc／svg:title 子樹深度

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if name == b"office:master-styles" {
                    depth_master_styles += 1;
                    continue;
                }
                if depth_master_styles == 0 {
                    continue;
                }
                if is_master_page(name) {
                    // 開始一筆主頁：先把上一筆收帳（正則 XML 下由 End 收尾，
                    // 這裡只為「未閉合即來下一筆」的不健全檔兜底）。
                    if let Some(prev) = current.replace(MasterHf {
                        name: attr_local(&e, b"name").unwrap_or_default(),
                        slots: Vec::new(),
                    }) {
                        masters.push(prev);
                    }
                    open_slot = None;
                    slot_paras.clear();
                    open_paras.clear();
                    skip_alt_text = 0;
                    continue;
                }
                if current.is_none() {
                    continue;
                }
                if let Some(kind) = slot_kind(name) {
                    open_slot = Some(kind);
                    slot_paras = Vec::new();
                    open_paras.clear();
                    skip_alt_text = 0;
                } else if open_slot.is_some() {
                    if is_alt_text(name) {
                        skip_alt_text += 1;
                    } else if skip_alt_text == 0 {
                        if is_para(name) {
                            let style =
                                book.resolve(&attr_local(&e, b"style-name").unwrap_or_default());
                            open_paras.push((slot_paras.len(), String::new(), style));
                            slot_paras.push(OdfLayoutParagraph::default());
                        } else if is_space_like(name) {
                            push_inline(&mut open_paras, name, Some(&e));
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if depth_master_styles == 0 {
                    continue;
                }
                if is_master_page(name) {
                    // 自閉合主頁（WordTest_edit.odt 實測 `<style:master-page/>` 是
                    // 該檔唯一主頁）：收上一筆後，本筆**立即入帳**——自閉合沒有
                    // End 事件，留到 End 會整筆消失（宣告面漏記＝本卡缺陷的變體）。
                    if let Some(prev) = current.replace(MasterHf {
                        name: attr_local(&e, b"name").unwrap_or_default(),
                        slots: Vec::new(),
                    }) {
                        masters.push(prev);
                    }
                    if let Some(mp) = current.take() {
                        masters.push(mp);
                    }
                    open_slot = None;
                    slot_paras.clear();
                    open_paras.clear();
                    continue;
                }
                if current.is_none() || open_slot.is_none() || skip_alt_text > 0 {
                    continue;
                }
                if is_para(name) {
                    let style = book.resolve(&attr_local(&e, b"style-name").unwrap_or_default());
                    slot_paras.push(hf_paragraph(String::new(), style));
                } else if is_space_like(name) {
                    push_inline(&mut open_paras, name, Some(&e));
                }
            }
            Ok(Event::Text(e)) => {
                if open_paras.is_empty() || skip_alt_text > 0 {
                    continue;
                }
                if let Ok(decoded) = e.decode() {
                    if let Ok(unescaped) = quick_xml::escape::unescape(&decoded) {
                        open_paras
                            .last_mut()
                            .expect("段落堆疊非空")
                            .1
                            .push_str(&unescaped);
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if open_paras.is_empty() || skip_alt_text > 0 {
                    continue;
                }
                let resolved = crate::body::resolve_general_ref(&r)?;
                open_paras
                    .last_mut()
                    .expect("段落堆疊非空")
                    .1
                    .push_str(&resolved);
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if name == b"office:master-styles" {
                    depth_master_styles = depth_master_styles.saturating_sub(1);
                    continue;
                }
                if depth_master_styles == 0 {
                    continue;
                }
                if is_master_page(name) {
                    if let Some(mp) = current.take() {
                        masters.push(mp);
                    }
                } else if is_alt_text(name) {
                    skip_alt_text = skip_alt_text.saturating_sub(1);
                } else if let Some(kind) = slot_kind(name) {
                    if open_slot == Some(kind) {
                        if let Some(mp) = current.as_mut() {
                            mp.slots.push(HfSlot {
                                kind,
                                paragraphs: std::mem::take(&mut slot_paras),
                            });
                        }
                        open_slot = None;
                        open_paras.clear();
                    }
                } else if open_slot.is_some() && is_para(name) {
                    if let Some((idx, text, style)) = open_paras.pop() {
                        if idx < slot_paras.len() {
                            slot_paras[idx] = hf_paragraph(text, style);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(OdfError::Xml(err.to_string())),
            _ => {}
        }
        buf.clear();
    }

    // 不健全 XML 收尾：未閉合的殘段照已累積文字入帳（不吞成零段）。
    for (idx, text, style) in open_paras.drain(..) {
        if idx < slot_paras.len() {
            slot_paras[idx] = hf_paragraph(text, style);
        }
    }
    if let Some(kind) = open_slot {
        if let Some(mp) = current.as_mut() {
            mp.slots.push(HfSlot {
                kind,
                paragraphs: std::mem::take(&mut slot_paras),
            });
        }
    }
    if let Some(mp) = current {
        masters.push(mp);
    }
    Ok(masters)
}

#[inline]
fn is_master_page(name: &[u8]) -> bool {
    name == b"style:master-page" || name == b"master-page"
}

#[inline]
fn is_para(name: &[u8]) -> bool {
    name == b"text:p" || name == b"p" || name == b"text:h" || name == b"h"
}

#[inline]
fn is_space_like(name: &[u8]) -> bool {
    matches!(
        name,
        b"text:s" | b"s" | b"text:tab" | b"tab" | b"text:line-break" | b"line-break"
    )
}

/// `svg:desc`／`svg:title`＝圖形替代文字（LO 永不渲染；E2 報告 D3 同律）。
#[inline]
fn is_alt_text(name: &[u8]) -> bool {
    matches!(name, b"svg:desc" | b"desc" | b"svg:title" | b"title")
}

fn slot_kind(name: &[u8]) -> Option<MasterHfSlotKind> {
    match name {
        b"style:header" | b"header" => Some(MasterHfSlotKind::Header),
        b"style:header-first" | b"header-first" => Some(MasterHfSlotKind::HeaderFirst),
        b"style:header-left" | b"header-left" => Some(MasterHfSlotKind::HeaderLeft),
        b"style:footer" | b"footer" => Some(MasterHfSlotKind::Footer),
        b"style:footer-first" | b"footer-first" => Some(MasterHfSlotKind::FooterFirst),
        b"style:footer-left" | b"footer-left" => Some(MasterHfSlotKind::FooterLeft),
        _ => None,
    }
}

/// `text:s`（`@text:c` 展開）／`text:tab`／`text:line-break` 展開進當前段落。
fn push_inline(
    open_paras: &mut [(usize, String, (f32, bool))],
    name: &[u8],
    e: Option<&BytesStart<'_>>,
) {
    let Some((_, text, _)) = open_paras.last_mut() else {
        return;
    };
    match name {
        b"text:s" | b"s" => {
            let mut n = 1usize;
            if let Some(e) = e {
                for attr in e.attributes().flatten() {
                    if attr.key.local_name().as_ref() == b"c" {
                        if let Ok(s) = std::str::from_utf8(&attr.value) {
                            if let Ok(v) = s.parse::<usize>() {
                                n = v;
                            }
                        }
                    }
                }
            }
            text.push_str(&" ".repeat(n));
        }
        b"text:tab" | b"tab" => text.push('\t'),
        _ => text.push('\n'),
    }
}

/// 段落級單一 run（**段落樣式**級聯值；span 內文字級不另拆 run——宣告面的
/// 字級軸是「該段代表字級」，與 C3 宿主段的逐片段 runs 口徑刻意不同，基準
/// 腳本 `odf_e2d_hf_truth.py` 同形）。
fn hf_paragraph(text: String, style: (f32, bool)) -> OdfLayoutParagraph {
    let n = text.chars().count();
    OdfLayoutParagraph {
        text,
        runs: vec![((0, n), style.0, style.1)],
    }
}

fn attr_local(e: &BytesStart<'_>, want: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == want {
            return Some(std::str::from_utf8(&attr.value).unwrap_or("").to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 發射面（逐頁指派）
// ---------------------------------------------------------------------------

/// 頁內段落的角色（[`HfAssignment::flow`] 的段序判定軸）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HfRole {
    Header,
    Body,
    Footer,
}

/// 一頁的頁首尾指派（正文段以宿主段落流的切片區間引用，零複製零污染）。
#[derive(Debug, Clone, PartialEq)]
pub struct HfPage {
    /// 全域頁序（0 起算）。
    pub index: usize,
    /// 所屬主頁鏈分段（chunk）序（0 起算）。
    pub chunk: usize,
    /// 生效主頁名（解不到＝None ⇒ 該頁零頁首尾）。
    pub master_page: Option<String>,
    /// 該頁是否套用 `-first` 槽（鑑別力面：主頁首次使用的首頁）。
    pub first_page_of_master: bool,
    /// 該頁是否屬鏡像左頁（全域頁序奇數＝邏輯偶數頁）。
    pub left_page: bool,
    /// 頁首段（已濾空文字段；宣告面原序）。
    pub header: Vec<OdfLayoutParagraph>,
    /// 頁尾段（已濾空文字段；宣告面原序）。
    pub footer: Vec<OdfLayoutParagraph>,
    /// 正文段起點（`proj.paragraphs` 索引，含）。
    pub body_start: usize,
    /// 正文段終點（`proj.paragraphs` 索引，不含）。
    pub body_end: usize,
}

impl HfPage {
    /// 該頁正文段逐字（宿主切片）。
    pub fn body_texts(&self, proj: &OdfProjection) -> Vec<String> {
        proj.paragraphs[self.body_start..self.body_end]
            .iter()
            .map(|p| p.text.clone())
            .collect()
    }
}

/// 發射面結果（**獨立帳**：不進宿主段落流／`flatten()`／`total_text_len`）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HfAssignment {
    /// 逐頁指派（全域頁序單調）。
    pub pages: Vec<HfPage>,
    /// 發射的頁首尾段數（濾空後合計）。
    pub emitted_paragraphs: usize,
    /// 發射的頁首尾文字碼點和（多頁檔按頁重複計＝D4「字/頁次」口徑）。
    pub emitted_text_len: usize,
    /// 宣告了 `-left` 槽但因非左頁（或該頁用 `-first`）而不發的次數（記帳）。
    pub skipped_left_slots: usize,
    /// §19.501 分段數（≥1；回塞態恆 1）。
    pub chunks: usize,
}

impl HfAssignment {
    /// 展平段序流 `(頁序, 角色, 文字)`：同頁內恆 Header*→Body*→Footer*、頁間
    /// 單調——段序鐵律（頁首段在該頁正文段前、頁尾段在其後）的判定依歸。
    pub fn flow(&self, proj: &OdfProjection) -> Vec<(usize, HfRole, String)> {
        let mut out = Vec::new();
        for page in &self.pages {
            for p in &page.header {
                out.push((page.index, HfRole::Header, p.text.clone()));
            }
            for p in &proj.paragraphs[page.body_start..page.body_end] {
                out.push((page.index, HfRole::Body, p.text.clone()));
            }
            for p in &page.footer {
                out.push((page.index, HfRole::Footer, p.text.clone()));
            }
        }
        out
    }
}

/// 發射面入口：宣告面（`proj.hf_masters`）＋ §19.501 主頁鏈分段 ＋ 逐主頁真實
/// 頁面幾何（C4 鏈）→ 逐頁指派。加密檔由 `project()` 閘門擋在前頭，這裡同口徑
/// 老實上報。
pub fn hf_assignment(bundle: &OdfBundle, proj: &OdfProjection) -> Result<HfAssignment, OdfError> {
    let content = bundle.read_entry("content.xml")?;
    let dom = style_domain_page(bundle)?;
    let switches = master_page_switches(&content, &dom);
    Ok(assign_from(&content, &dom, proj, &switches))
}

/// 指派主體（純函數：內容位元組只用於換頁觸發點的段落計數，可單測）。
fn assign_from(
    content_xml: &[u8],
    dom: &StyleDomainPage,
    proj: &OdfProjection,
    switches: &[ParaSwitch],
) -> HfAssignment {
    let texts: Vec<String> = proj.paragraphs.iter().map(|p| p.text.clone()).collect();
    let file_master = default_master_page_name(dom, content_xml);

    // 分段：每個 §19.501 觸發點切一刀（index 0 的宣告不產生額外換頁——LO 實測
    // FDO76312／mce-nested 首段宣告、LO 頁數不因之增加）。
    let mut bounds: Vec<usize> = Vec::new();
    if !buggy_single_master() {
        for sw in switches {
            if sw.para_index == 0 || sw.para_index >= texts.len() {
                continue;
            }
            if bounds.last() == Some(&sw.para_index) {
                continue;
            }
            bounds.push(sw.para_index);
        }
    }
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    for b in &bounds {
        if *b > start {
            ranges.push((start, *b));
            start = *b;
        }
    }
    ranges.push((start, texts.len()));

    let mut out = HfAssignment {
        chunks: ranges.len(),
        ..Default::default()
    };
    let mut used_master: Vec<String> = Vec::new();
    let mut global_page = 0usize;

    for (chunk_idx, (lo, hi)) in ranges.iter().enumerate() {
        let master_name = switches
            .iter()
            .find(|s| s.para_index == *lo)
            .map(|s| s.master_page.clone())
            .or_else(|| file_master.clone())
            .filter(|n| dom.master_pages.iter().any(|m| &m.name == n));
        let master_hf = master_name
            .as_deref()
            .and_then(|n| proj.hf_masters.iter().find(|m| m.name == n));
        let params = master_params(dom, master_name.as_deref());

        let chunk_texts = &texts[*lo..*hi];
        let breaks = quire_layout::page_breaks(chunk_texts, &params);
        // 容量分頁起點表（首頁起點 0）；空 chunk（0 段）仍佔一頁，頁首尾照發。
        let mut starts: Vec<usize> = vec![0];
        for pb in breaks.iter().skip(1) {
            let rel = pb.first_para.min(chunk_texts.len());
            if *starts.last().unwrap() != rel {
                starts.push(rel);
            }
        }
        for (k, page_start_rel) in starts.iter().enumerate() {
            let page_end_rel = starts.get(k + 1).copied().unwrap_or(chunk_texts.len());
            let first_of_master = match master_name.as_deref() {
                Some(n) => {
                    let seen = used_master.iter().any(|u| u == n);
                    if !seen {
                        used_master.push(n.to_string());
                    }
                    !seen && k == 0
                }
                None => false,
            };
            let left_page = global_page % 2 == 1;
            let (header, footer, skipped_left) = match master_hf {
                Some(m) => pick_slots(m, first_of_master, left_page),
                None => (Vec::new(), Vec::new(), 0usize),
            };
            out.emitted_paragraphs += header.len() + footer.len();
            out.emitted_text_len += header
                .iter()
                .chain(footer.iter())
                .map(|p| p.text.chars().count())
                .sum::<usize>();
            out.skipped_left_slots += skipped_left;
            out.pages.push(HfPage {
                index: global_page,
                chunk: chunk_idx,
                master_page: master_name.clone(),
                first_page_of_master: first_of_master,
                left_page,
                header,
                footer,
                body_start: lo + page_start_rel,
                body_end: lo + page_end_rel,
            });
            global_page += 1;
        }
    }
    out
}

/// 該頁取哪對槽（頁首一組、頁尾一組，規則同）。
///
/// * 主頁首次使用的首頁且宣告 `-first` ⇒ 用之（**空宣告亦用** ⇒ 零發射，不退
///   plain：LO 實測 FDO76312／fdo78420／mce-nested 首頁不渲染正頁首文字）。
/// * 鏡像左頁且宣告 `-left` ⇒ 用之（空宣告同 ⇒ 零發射：tdf123705 p1/p3 頂帶
///   零文字，正頁首**不**補發）。
/// * 其餘 ⇒ plain。
///
/// 回傳（頁首段, 頁尾段, 本頁未消费的 `-left` 槽數）——最後項是記帳軸：宣告了
/// `-left` 卻因該頁非左頁而未發的次數（語料鏡像頁數少，需可查）。
fn pick_slots(
    m: &MasterHf,
    first_of_master: bool,
    left_page: bool,
) -> (Vec<OdfLayoutParagraph>, Vec<OdfLayoutParagraph>, usize) {
    let pick = |plain: MasterHfSlotKind,
                first: MasterHfSlotKind,
                left: MasterHfSlotKind|
     -> (Vec<OdfLayoutParagraph>, usize) {
        if first_of_master {
            if let Some(s) = m.slot(first) {
                // 首頁不可能是左頁（頁序 0 為偶數），此態不消費 -left。
                return (emit_nonempty(s), m.slot(left).is_some() as usize);
            }
        }
        if left_page {
            if let Some(s) = m.slot(left) {
                return (emit_nonempty(s), 0);
            }
        }
        // 右頁（或未宣告 -left）⇒ 退 plain，並記帳一筆未消费的 -left。
        let paras = m.slot(plain).map(emit_nonempty).unwrap_or_default();
        (paras, m.slot(left).is_some() as usize)
    };
    let (header, s1) = pick(
        MasterHfSlotKind::Header,
        MasterHfSlotKind::HeaderFirst,
        MasterHfSlotKind::HeaderLeft,
    );
    let (footer, s2) = pick(
        MasterHfSlotKind::Footer,
        MasterHfSlotKind::FooterFirst,
        MasterHfSlotKind::FooterLeft,
    );
    (header, footer, s1 + s2)
}

/// 濾掉空文字段（LO 不渲染零文字段）。
fn emit_nonempty(slot: &HfSlot) -> Vec<OdfLayoutParagraph> {
    slot.paragraphs
        .iter()
        .filter(|p| !p.text.is_empty())
        .cloned()
        .collect()
}

/// 主頁 → 該主頁 page-layout 的 `LayoutParams`（無主頁／無 page-layout＝LO 預設頁，
/// 與 C4 的回退口徑逐位相同）。
fn master_params(dom: &StyleDomainPage, master_name: Option<&str>) -> LayoutParams {
    let mp = master_name.and_then(|n| dom.master_pages.iter().find(|m| m.name == n));
    match mp.and_then(|m| dom.layout_of_master(m)) {
        Some(l) => l.to_layout_params(),
        None => LayoutParams {
            page_w_pt: LO_DEFAULT_PAGE_W_PT,
            page_h_pt: LO_DEFAULT_PAGE_H_PT,
            margins_pt: [LO_DEFAULT_MARGIN_PT; 4],
            ..Default::default()
        },
    }
}

// ---------------------------------------------------------------------------
// §19.501 換頁觸發點
// ---------------------------------------------------------------------------

/// 一個 §19.501 換頁觸發點：宿主段落流的段落起點 index ＋ 該處生效的主頁名。
#[derive(Debug, Clone, PartialEq)]
pub struct ParaSwitch {
    /// 該觸發項在 `proj.paragraphs` 的起點 index（＝分段邊界；該 index 起改用
    /// `master_page`）。0＝文件級宣告（不產生額外換頁）。
    pub para_index: usize,
    /// 宣告的主頁名（非空）。
    pub master_page: String,
    /// 觸發元素種別（`"p"`／`"h"`／`"table"`／`"frame"`——記帳／考卷對照用）。
    pub kind: &'static str,
}

/// 掃 content.xml 的**頂層流程項**（`office:text` 直下；下鑽 `text:list`／
/// `text:list-item`／`text:section`——LO 與探針同形，這些不是換頁單元），回傳
/// 宣告 `@style:master-page-name`（含 parent 鏈繼承，經
/// [`StyleDomainPage::master_page_of_style`]）的觸發點。
///
/// **index 口徑（關鍵）**：觸發點 index ＝該流程項之前所有流程項貢獻的段落數
/// ＝該項在宿主段落流中那段**連續區間**的起點。§19.501 的換頁語意是「整個流程項
/// 起新頁」，故即使 `walk_paragraphs` 以閉合序入列（容器段落排在巢裝內容**之後**），
/// 邊界仍取該項的**區間起點**而非容器段落自己的 index——巢裝內容與容器同頁。
/// 對照實測：tdf124637 四個觸發點 ⇒ LO 恰 5 頁（探針 `probe/mp_switch_census.py`）。
pub fn master_page_switches(content_xml: &[u8], dom: &StyleDomainPage) -> Vec<ParaSwitch> {
    let mut reader = Reader::from_reader(content_xml);
    let mut buf = Vec::new();

    let mut out: Vec<ParaSwitch> = Vec::new();
    let mut depth_text = 0usize; // office:text
    let mut depth_item = 0usize; // 0＝頂層流程位置；>0＝在某流程項內
    let mut para_count = 0usize; // 已計段落數（含巢裝，同宿主段落流口徑）

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if name == b"office:text" {
                    depth_text += 1;
                    continue;
                }
                if depth_text == 0 {
                    continue;
                }
                if is_para(name) || is_table_or_frame(name) {
                    if depth_item == 0 {
                        // 新流程項：起點＝當前段落計數；查宣告（含 parent 鏈）。
                        if let Some((mp, kind)) = declared_master(dom, &e) {
                            out.push(ParaSwitch {
                                para_index: para_count,
                                master_page: mp,
                                kind,
                            });
                        }
                        depth_item = 1;
                    } else {
                        depth_item += 1;
                    }
                    if is_para(name) {
                        para_count += 1;
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if depth_text == 0 {
                    continue;
                }
                if is_para(name) {
                    if depth_item == 0 {
                        if let Some((mp, kind)) = declared_master(dom, &e) {
                            out.push(ParaSwitch {
                                para_index: para_count,
                                master_page: mp,
                                kind,
                            });
                        }
                    }
                    para_count += 1;
                } else if is_table_or_frame(name) && depth_item == 0 {
                    if let Some((mp, kind)) = declared_master(dom, &e) {
                        out.push(ParaSwitch {
                            para_index: para_count,
                            master_page: mp,
                            kind,
                        });
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if name == b"office:text" {
                    depth_text = depth_text.saturating_sub(1);
                    continue;
                }
                if depth_text == 0 {
                    continue;
                }
                if is_para(name) || is_table_or_frame(name) {
                    depth_item = depth_item.saturating_sub(1);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break, // 不健全 XML：已收集的觸發點照回（不吞舊帳）
            _ => {}
        }
        buf.clear();
    }
    out
}

#[inline]
fn is_table_or_frame(name: &[u8]) -> bool {
    matches!(
        name,
        b"table:table"
            | b"table"
            | b"draw:frame"
            | b"frame"
            | b"draw:custom-shape"
            | b"custom-shape"
            | b"draw:text-box"
            | b"text-box"
            | b"draw:anchor"
            | b"anchor"
    )
}

/// 元素自身樣式（含 parent 鏈）宣告的主頁名＋種別；無宣告＝None。
fn declared_master(dom: &StyleDomainPage, e: &BytesStart<'_>) -> Option<(String, &'static str)> {
    let style = attr_local(e, b"style-name").unwrap_or_default();
    let kind = kind_of(e.name().as_ref());
    dom.master_page_of_style(&style)
        .filter(|s| !s.is_empty())
        .map(|mp| (mp, kind))
}

#[inline]
fn kind_of(name: &[u8]) -> &'static str {
    match name {
        b"text:h" | b"h" => "h",
        b"table:table" | b"table" => "table",
        b"draw:frame" | b"frame" | b"draw:custom-shape" | b"custom-shape" | b"draw:text-box"
        | b"text-box" | b"draw:anchor" | b"anchor" => "frame",
        _ => "p",
    }
}
