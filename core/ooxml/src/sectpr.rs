//! sectPr（section properties）頁面幾何與頁首頁尾宣告解析 —— Phase0-2b＋頁首頁尾全鏈。
//!
//! 分頁必須跟著檔案實際的頁面幾何走：`LayoutParams::default()` 只是**檔案沒有
//! 任何 sectPr 資訊時**的後備值，不是硬編碼。本模組由 `word/document.xml`
//! 逐 section 讀取 `pgSz`／`pgMar`／`type`（ECMA-376 §17.2 節），換算成 pt
//! （twip ÷ 20，ECMA-376 §17.18.106 長度單位），回傳**逐段落**的 section 清單。
//!
//! 段落對齊規則（ECMA-376 §17.2.3）：
//! - 段落 `w:pPr` 內的 `w:sectPr` ⇒ 該段落收束它前面的 section（該段屬此 section，
//!   其後的段落屬下一個 section）；
//! - `w:body` 結尾（任何 `w:p` 之外）的 `w:sectPr` ⇒ 收束剩餘段落（檔尾 section）。
//!
//! 實作以「`sectPr` 結束時已計入的段落數」為 section 涵蓋的上界：計數方式與
//! `parse_document_xml` 完全一致（只算 `w:p` 的 Start 事件，含表格內段落，
//! 不含自閉合 `w:p/>`），清單長度恆等於段落數，layout 層逐段直查。
//!
//! 每個 section 帶文件內序號 `id`：同尺寸的相鄰 section（HC031 末兩節皆直式
//! Letter）仍必須在邊界換頁，靠 id 辨識而非尺寸差異。`w:type` 讀 continuous
//! （同頁續排，不強制換頁）；oddPage/evenPage 以 nextPage 近似（頁碼奇偶補白
//! 屬後續卡）。
//!
//! 頁首頁尾宣告（本卡新增）：每個 section 另讀 `pgMar @w:header/@w:footer`
//! 距離（預設 720 twip＝0.5 吋，ECMA-376 §17.4.48 表列預設）、`w:titlePg`
//! （首頁獨立頁尾開關），以及 `w:headerReference`／`w:footerReference` 的
//! `w:type`（default／first／even）＋`r:id`。r:id→part 路徑的 OPC rels 解析
//! 在呼叫端（`OoxmlDoc::resolve_rel`），本模組只保留 rId 原字串。
//! **跨節繼承**依 ECMA-376 §17.10.5：第 k 節未宣告的 slot（型別）沿用第
//! k−1 節（逐 slot 遞移，繼承鏈沿 id 序往前追）。`settings.xml` 的
//! `evenAndOddHeaders` 為文件級旗標，另由 [`read_even_and_odd_headers`] 讀取。
//!
//! 已知骨架近似：表格 Cell 內段落自帶 sectPr（畸形但存在）時，section 邊界
//! 推到該 Cell 所屬表格段落為止。
//!
//! 欄數採集（L1-columns 卡）：`w:cols/@w:num`（欄數）、`w:cols/@w:space`
//! （欄間距）、`w:col/@w:w`＋`@w:space`（逐欄寬與該欄後距，ECMA-376 §17.18.6）
//! 一律讀進 [`SectionGeometry`]；分欄回流在 layout 層消費（那里是幾何唯一
//! 消費者，本模組只負責如實投影）。

use serde::{Deserialize, Serialize};

use crate::notes::NotePr;

/// Word 省略 `w:pgSz` 時的頁面後備尺寸（twip）：A4 直式。
const DEF_PG_W: i64 = 11906;
const DEF_PG_H: i64 = 16838;
/// Word 省略 `w:pgMar` 各欄位時的預設邊界（twip）：上下 1 吋、左右 1.25 吋。
const DEF_MARGIN_TOP: i64 = 1440;
const DEF_MARGIN_BOTTOM: i64 = 1440;
const DEF_MARGIN_LEFT: i64 = 1800;
const DEF_MARGIN_RIGHT: i64 = 1800;
/// `pgMar @w:header/@w:footer` 缺值（twip）：0.5 吋（ECMA-376 §17.4.48 預設）。
const DEF_HF_DIST: i64 = 720;

/// `w:cols` 採集結果（ECMA-376 §17.18.6／§17.18.7），欄寬與間距一律換算成
/// pt 存（geometry 欄位同標尺）。`num <= 1`＝單欄——既有語義逐位元組不變。
///
/// `Copy` 鐵律：本結構隨 [`SectionGeometry`] 四處複製（HashMap 鍵值、逐段落
/// 直查表），禁 Vec——逐欄值以固定長度陣列承載，頭 4 欄為上限（corpus 實見
/// ≤2 欄；超標欄位的顯式寬以「未宣告」近似走平分，屬記錄在案的近似）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionCols {
    /// `w:cols/@w:num`（缺＝0＝未宣告，視同單欄；0/1 同義）。
    pub num: u32,
    /// `w:cols/@w:space`（pt；缺＝36pt＝720 twip，§17.18.6 預設）。
    pub space_pt: f32,
    /// 頭 4 欄 `w:col/@w:w`（pt；None＝該欄未宣告 → 呼叫端平分）。
    pub col_w_pt: [Option<f32>; 4],
    /// 頭 4 欄 `w:col/@w:space`（pt；None＝沿用 `space_pt`；§17.18.7：
    /// 該欄**右側**距）。
    pub col_space_pt: [Option<f32>; 4],
}

impl Default for SectionCols {
    /// 未宣告態：單欄、間距預設 720 twip＝36pt（§17.18.6）、無逐欄宣告。
    fn default() -> Self {
        Self {
            num: 0,
            space_pt: DEF_COL_SPACE as f32 / 20.0,
            col_w_pt: [None; 4],
            col_space_pt: [None; 4],
        }
    }
}

/// 預設欄間距（twip）：720＝0.5 吋（ECMA-376 §17.18.6 `w:space` 預設）。
const DEF_COL_SPACE: i64 = 720;

/// 一個 section 的頁面幾何（pt）。`landscape` 僅供老實回報，
/// 尺寸一律以 `page_w_pt`／`page_h_pt` 為準（Word/LO 皆直接信任 pgSz 的 w/h）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionGeometry {
    pub page_w_pt: f32,
    pub page_h_pt: f32,
    pub margin_top_pt: f32,
    pub margin_bottom_pt: f32,
    pub margin_left_pt: f32,
    pub margin_right_pt: f32,
    /// `w:pgSz/@w:orient="landscape"`。
    pub landscape: bool,
    /// `w:type/@w:val="continuous"`：同頁續排，不強制換頁。
    /// （oddPage/evenPage 以 nextPage 近似＝false。）
    pub continuous: bool,
}

/// 逐段落清單的一項：段落所屬 section 的識別與幾何。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionSpec {
    /// 文件內 section 序號（0 起）。同尺寸相鄰 section 靠 id 辨識邊界。
    pub id: u32,
    pub geo: SectionGeometry,
}

/// 一個 section 的頁首（或頁尾）三 slot 引用：rId 原字串，缺＝None。
/// 解析 rId→part 與跨節繼承后的最終選用由 engine 層依
/// `(section_id, is_first_page, page % 2)` 決定。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotRefs {
    /// `w:headerReference`／`w:footerReference` 之 `w:type="default"`（缺 w:type 同）。
    pub default: Option<String>,
    /// `w:type="first"`（僅在 `titlePg` 開啟的首頁生效）。
    pub first: Option<String>,
    /// `w:type="even"`（僅在 `evenAndOddHeaders` 開啟的偶數頁生效）。
    pub even: Option<String>,
}

/// 一個 section 的頁首頁尾宣告（已套用 ECMA-376 §17.10.5 跨節繼承）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionHeaderFooter {
    pub id: u32,
    /// `pgMar @w:header`：頁首內容頂端 y（pt，自頁頂）。
    pub header_dist_pt: f32,
    /// `pgMar @w:footer`：頁尾內容頂端錨點距頁底（pt）——發射 y＝頁高−本值。
    pub footer_dist_pt: f32,
    /// `w:titlePg`：本節首頁使用 first slot。
    pub title_pg: bool,
    pub header: SlotRefs,
    pub footer: SlotRefs,
}

/// 一節的 pgSz/pgMar/type/頁首頁尾宣告累積值（距離以 twip 原單位存）。
#[derive(Debug, Clone)]
struct Accum {
    w: i64,
    h: i64,
    top: i64,
    bottom: i64,
    left: i64,
    right: i64,
    landscape: bool,
    continuous: bool,
    header_d: i64,
    footer_d: i64,
    title_pg: bool,
    header: SlotRefs,
    footer: SlotRefs,
    /// `w:footnotePr`（ECMA-376 §17.11.11 節範圍）；缺＝None。
    note_footnote: Option<NotePr>,
    /// `w:endnotePr`（ECMA-376 §17.11.4 節範圍）；缺＝None。
    note_endnote: Option<NotePr>,
}

impl Accum {
    fn new() -> Self {
        Self {
            w: DEF_PG_W,
            h: DEF_PG_H,
            top: DEF_MARGIN_TOP,
            bottom: DEF_MARGIN_BOTTOM,
            left: DEF_MARGIN_LEFT,
            right: DEF_MARGIN_RIGHT,
            landscape: false,
            continuous: false,
            header_d: DEF_HF_DIST,
            footer_d: DEF_HF_DIST,
            title_pg: false,
            header: SlotRefs::default(),
            footer: SlotRefs::default(),
            note_footnote: None,
            note_endnote: None,
        }
    }
    fn into_geom(self) -> SectionGeometry {
        SectionGeometry {
            page_w_pt: self.w as f32 / 20.0,
            page_h_pt: self.h as f32 / 20.0,
            margin_top_pt: self.top as f32 / 20.0,
            margin_bottom_pt: self.bottom as f32 / 20.0,
            // gutter（裝訂線）已於 pgMar 解析時併入 left（LTR 文件左側加寬）。
            margin_left_pt: self.left as f32 / 20.0,
            margin_right_pt: self.right as f32 / 20.0,
            landscape: self.landscape,
            continuous: self.continuous,
        }
    }
    fn into_hf(self, id: u32) -> SectionHeaderFooter {
        SectionHeaderFooter {
            id,
            header_dist_pt: self.header_d as f32 / 20.0,
            footer_dist_pt: self.footer_d as f32 / 20.0,
            title_pg: self.title_pg,
            header: self.header,
            footer: self.footer,
        }
    }
}

/// 單趟掃描結果：邊界（涵蓋至第幾段, id, 累積值）＋檔尾 section。
struct Scan {
    boundaries: Vec<(usize, u32, Accum)>,
    body_sect: Option<(u32, Accum)>,
}

/// 由 `word/document.xml` 明文解析「逐段落」的 section（識別＋頁面幾何）。
///
/// 回傳清單長度 == `n_paragraphs`。檔案完全沒有 sectPr 時回傳空清單
/// （呼叫端退回 `LayoutParams::default()`——後備語義，非硬編碼斷言）。
pub fn read_sections_per_paragraph(xml: &[u8], n_paragraphs: usize) -> Vec<SectionSpec> {
    if n_paragraphs == 0 {
        return Vec::new();
    }
    let scan = scan_sections(xml);
    if scan.boundaries.is_empty() && scan.body_sect.is_none() {
        return Vec::new(); // 無任何 section 資訊 → 呼叫端後備
    }

    // 展開成逐段落清單：邊界 section 各涵蓋到其邊界段為止，檔尾 section 吃其餘。
    let mut out = Vec::with_capacity(n_paragraphs);
    for (through, id, a) in &scan.boundaries {
        let g = clone_accum(a).into_geom();
        while out.len() < (*through).min(n_paragraphs) {
            out.push(SectionSpec { id: *id, geo: g });
        }
    }
    let tail = scan
        .body_sect
        .as_ref()
        .map(|(id, a)| (*id, clone_accum(a)))
        .or_else(|| {
            scan.boundaries
                .last()
                .map(|(_, id, a)| (*id, clone_accum(a)))
        });
    if let Some((id, a)) = tail {
        let g = a.into_geom();
        while out.len() < n_paragraphs {
            out.push(SectionSpec { id, geo: g });
        }
    }
    out
}

/// 由 `word/document.xml` 明文解析逐 section 的頁首頁尾宣告。
///
/// 回傳依 section `id` 升序（＝文件序），且已對六個 slot（頁首/頁尾 ×
/// default/first/even）套用 ECMA-376 §17.10.5 跨節繼承：第 k 節未宣告的
/// slot 沿用第 k−1 節繼承後的值。無任何 section 時回傳空清單。
/// `distance`／`titlePg` 不繼承（pgMar、titlePg 為本節自決屬性，缺值走預設）。
///
/// 檔尾無 body 級 sectPr 時**不**合成新節：`read_sections_per_paragraph`
/// 已把檔尾段落歸給最後一個邊界節（同款回退），頁面的 section id 查找
/// 自然命中該節的宣告——id 空間由逐段落清單單方定義，此處對齊。
pub fn read_sections_header_footer(xml: &[u8]) -> Vec<SectionHeaderFooter> {
    let scan = scan_sections(xml);
    let mut out: Vec<SectionHeaderFooter> = Vec::new();
    for (_, id, a) in &scan.boundaries {
        out.push(clone_accum(a).into_hf(*id));
    }
    if let Some((id, a)) = &scan.body_sect {
        out.push(clone_accum(a).into_hf(*id));
    }
    // 跨節繼承（§17.10.5）：id 升序逐節把缺位 slot 往前補（遞移——k−1 已被
    // 更早節填過時，補的就是更早節的值，語義即「沿文件序找最近的宣告」）。
    for k in 1..out.len() {
        let prev_header = out[k - 1].header.clone();
        let prev_footer = out[k - 1].footer.clone();
        let cur = &mut out[k];
        inherit(&mut cur.header, &prev_header);
        inherit(&mut cur.footer, &prev_footer);
    }
    out
}

/// 由 `word/document.xml` 明文解析逐 section 的 `w:footnotePr`（ECMA-376
/// §17.11.11 節範圍；文件級設定見 [`crate::parse_note_settings`]）。
///
/// 回傳依 section `id` 升序（同 [`read_sections_header_footer`]），每節一個
/// `Option<NotePr>`：該節未宣告 `w:footnotePr` → `None`（逐欄覆寫語意——
/// 缺＝不覆寫，沿用 settings；合併是消費端的事）。無任何 section → 空清單。
/// 逐段對齊請以 [`read_sections_per_paragraph`] 的 section id 對應。
pub fn read_sections_note_pr(xml: &[u8]) -> Vec<Option<NotePr>> {
    read_sections_note_pr_of(xml, true)
}

/// 由 `word/document.xml` 明文解析逐 section 的 `w:endnotePr`（ECMA-376
/// §17.11.4 節範圍；同 [`read_sections_note_pr`] 口徑）。
pub fn read_sections_endnote_pr(xml: &[u8]) -> Vec<Option<NotePr>> {
    read_sections_note_pr_of(xml, false)
}

/// [`read_sections_note_pr`]／[`read_sections_endnote_pr`] 共用：`footnote`
/// 為 true 取 `w:footnotePr`，否則取 `w:endnotePr`。
fn read_sections_note_pr_of(xml: &[u8], footnote: bool) -> Vec<Option<NotePr>> {
    let scan = scan_sections(xml);
    let mut out = Vec::new();
    for (_, _, a) in &scan.boundaries {
        out.push(if footnote {
            a.note_footnote
        } else {
            a.note_endnote
        });
    }
    if let Some((_, a)) = &scan.body_sect {
        out.push(if footnote {
            a.note_footnote
        } else {
            a.note_endnote
        });
    }
    out
}

fn inherit(dst: &mut SlotRefs, src: &SlotRefs) {
    if dst.default.is_none() {
        dst.default = src.default.clone();
    }
    if dst.first.is_none() {
        dst.first = src.first.clone();
    }
    if dst.even.is_none() {
        dst.even = src.even.clone();
    }
}

fn clone_accum(a: &Accum) -> Accum {
    a.clone()
}

/// `word/settings.xml` → `evenAndOddHeaders`（文件級奇偶頁開關）。
/// 元素存在且 `w:val` 非 falsy（0/false/off/none）＝true。
pub fn read_even_and_odd_headers(settings_xml: &[u8]) -> bool {
    let text = String::from_utf8_lossy(settings_xml);
    let mut re = quick_xml::Reader::from_str(&text);
    use quick_xml::events::Event;
    while let Ok(ev) = re.read_event() {
        match ev {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e)
                if e.local_name().as_ref() == b"evenAndOddHeaders" =>
            {
                return toggle_on(&e);
            }
            _ => {}
        }
    }
    false
}

/// 開關屬性（同 `lib.rs::toggle_attr_on` 語義，本模組自持免反向依賴）：
/// 無 `w:val` ＝開啟；`0`/`false`/`off`/`none`（不分大小寫）＝關閉。
fn toggle_on(e: &quick_xml::events::BytesStart) -> bool {
    match attr_str(e, b"val") {
        None => true,
        Some(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "none" | "false" | "0" | "off"
        ),
    }
}

/// 單趟掃描 document.xml，收集 section 邊界（幾何＋頁首頁尾宣告）。
fn scan_sections(xml: &[u8]) -> Scan {
    let text = String::from_utf8_lossy(xml);
    let mut re = quick_xml::Reader::from_str(&text);

    use quick_xml::events::Event;

    let mut p_count: usize = 0; // 與 parse_document_xml 同口徑：只算 w:p 的 Start（Fallback 內不計）
    let mut block_depth: i32 = 0; // w:p／w:tbl 巢狀深度（0＝body 級）
    let mut fb: usize = 0; // mc:Fallback 巢狀深度：>0 時其內 w:p 不入扁平口徑
                           // K4e2：`w:txbxContent` 文字框子樹深度：>0 時其內 w:p **不入扁平口徑**
                           // （主解析器改掛外層段落的 `txbx_texts`）——四解析器同口徑鐵律的改寫點。
    let mut txbx_depth: usize = 0;
    let mut ac_stack: Vec<usize> = Vec::new(); // AC 開標的 fb 快照（錯置標籤不致卡死）
    let mut in_sect = false;
    let mut accum: Option<Accum> = None;
    let mut pending: Option<Accum> = None; // 段落邊界收束、待所屬 w:p 結束時歸檔
                                           // (涵蓋至第幾段, section 序號, 累積值)
    let mut boundaries: Vec<(usize, u32, Accum)> = Vec::new();
    let mut body_sect: Option<(u32, Accum)> = None;
    let mut next_id: u32 = 0; // section 文件序號：依收束出現順序編號
                              // 進行中的 notePr 子樹（sectPr 內 footnotePr／endnotePr；兩者互斥）。
    let mut note_footnote: Option<NotePr> = None;
    let mut note_endnote: Option<NotePr> = None;
    let mut in_note_footnote = false;
    let mut in_note_endnote = false;

    // pgSz／pgMar／type／headerReference… 在真實文件一律自閉合（Event::Empty），
    // 必須與 Start 同臂處理；Err＝尾部截斷，容忍並照常回報已解析的 section。
    while let Ok(ev) = re.read_event() {
        let ev_is_block_start = matches!(ev, Event::Start(_));
        match ev {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"Fallback" if ev_is_block_start => fb += 1,
                    b"AlternateContent" if ev_is_block_start => ac_stack.push(fb),
                    b"p" if ev_is_block_start => {
                        // K4e2：框內段落不計 p_count（主解析器改掛宿主段落的
                        // txbx_texts，不佔正文槽）；block_depth 仍成對遞增——
                        // 它只用於「body 級 block 是否完結」，跳過會讓框內
                        // sectPr（畸形）誤判成 body 級邊界。
                        if fb == 0 && txbx_depth == 0 {
                            p_count += 1;
                        }
                        block_depth += 1;
                    }
                    // K4e2：文字框子樹深度（口徑同 p_count：Fallback 內不跟）。
                    b"txbxContent" if ev_is_block_start && fb == 0 => txbx_depth += 1,
                    b"tbl" if ev_is_block_start => block_depth += 1,
                    b"sectPr" if fb == 0 => {
                        if ev_is_block_start {
                            in_sect = true;
                            accum = Some(Accum::new());
                        } else {
                            // <w:sectPr/> 無子元素：以預設值收束一個 section。
                            let a = Accum::new();
                            if block_depth > 0 {
                                pending = Some(a);
                            } else {
                                body_sect = Some((next_id, a));
                                next_id += 1;
                            }
                        }
                    }
                    b"pgSz" if in_sect => {
                        if let Some(a) = accum.as_mut() {
                            a.w = attr_i64(&e, b"w", a.w);
                            a.h = attr_i64(&e, b"h", a.h);
                            a.landscape = attr_str(&e, b"orient").as_deref() == Some("landscape");
                        }
                    }
                    b"pgMar" if in_sect => {
                        if let Some(a) = accum.as_mut() {
                            let top = attr_i64(&e, b"top", a.top);
                            let bottom = attr_i64(&e, b"bottom", a.bottom);
                            let left = attr_i64(&e, b"left", a.left);
                            let right = attr_i64(&e, b"right", a.right);
                            let gutter = attr_i64(&e, b"gutter", 0);
                            a.top = top;
                            a.bottom = bottom;
                            a.left = left + gutter;
                            a.right = right;
                            // header/footer 距離缺值＝720 twip（§17.4.48 預設）。
                            a.header_d = attr_i64(&e, b"header", DEF_HF_DIST).max(0);
                            a.footer_d = attr_i64(&e, b"footer", DEF_HF_DIST).max(0);
                        }
                    }
                    b"type" if in_sect => {
                        if let Some(a) = accum.as_mut() {
                            a.continuous = attr_str(&e, b"val").as_deref() == Some("continuous");
                        }
                    }
                    b"titlePg" if in_sect => {
                        if let Some(a) = accum.as_mut() {
                            a.title_pg = toggle_on(&e);
                        }
                    }
                    b"headerReference" if in_sect => {
                        if let Some(a) = accum.as_mut() {
                            set_slot(&mut a.header, &e);
                        }
                    }
                    b"footerReference" if in_sect => {
                        if let Some(a) = accum.as_mut() {
                            set_slot(&mut a.footer, &e);
                        }
                    }
                    b"footnotePr" if in_sect && !in_note_endnote => {
                        if ev_is_block_start {
                            in_note_footnote = true;
                            note_footnote = Some(NotePr::default());
                        } else if let Some(a) = accum.as_mut() {
                            // <w:footnotePr/> 空組：以全 None 收一個。
                            a.note_footnote = Some(NotePr::default());
                        }
                    }
                    b"endnotePr" if in_sect && !in_note_footnote => {
                        if ev_is_block_start {
                            in_note_endnote = true;
                            note_endnote = Some(NotePr::default());
                        } else if let Some(a) = accum.as_mut() {
                            a.note_endnote = Some(NotePr::default());
                        }
                    }
                    // notePr 的子元素（numFmt／numRestart／num／pos／footnote）
                    // 統一交给 notes 模組的分流口徑（不另造第二台狀態機）。
                    _ if in_note_footnote => {
                        if let Some(p) = note_footnote.as_mut() {
                            crate::notes::apply_note_pr_child(p, &e, b"footnote");
                        }
                    }
                    _ if in_note_endnote => {
                        if let Some(p) = note_endnote.as_mut() {
                            crate::notes::apply_note_pr_child(p, &e, b"endnote");
                        }
                    }
                    _ => {}
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"AlternateContent" => {
                        fb = ac_stack.pop().unwrap_or(0);
                    }
                    // K4e2：文字框子樹完結（Start 只在 fb==0 時遞增；End 一律
                    // 遞減——非驗證式解析器開閉各自跟隨，深層 Fallback 內的
                    // 鏡像 txbxContent End 以 saturating 兜住，不致轉負）。
                    b"txbxContent" => {
                        txbx_depth = txbx_depth.saturating_sub(1);
                    }
                    b"Fallback" => {
                        fb = fb.saturating_sub(1);
                    }
                    b"sectPr" => {
                        if in_sect {
                            in_sect = false;
                            if let Some(a) = accum.take() {
                                if block_depth > 0 {
                                    pending = Some(a);
                                } else {
                                    body_sect = Some((next_id, a));
                                    next_id += 1;
                                }
                            }
                        }
                    }
                    b"p" | b"tbl" => {
                        block_depth -= 1;
                        if block_depth == 0 {
                            if let Some(a) = pending.take() {
                                // 此 body 級 block 收束一個 section：涵蓋到目前計入的所有段落。
                                boundaries.push((p_count, next_id, a));
                                next_id += 1;
                            }
                        }
                    }
                    b"footnotePr" if in_note_footnote => {
                        in_note_footnote = false;
                        if let (Some(a), Some(p)) = (accum.as_mut(), note_footnote.take()) {
                            a.note_footnote = Some(p);
                        }
                    }
                    b"endnotePr" if in_note_endnote => {
                        in_note_endnote = false;
                        if let (Some(a), Some(p)) = (accum.as_mut(), note_endnote.take()) {
                            a.note_endnote = Some(p);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Scan {
        boundaries,
        body_sect,
    }
}

/// `headerReference`／`footerReference`：`w:type`（缺＝default）選 slot、
/// `r:id` 存 rId 原字串。無 r:id（畸形）＝不記。
fn set_slot(slot: &mut SlotRefs, e: &quick_xml::events::BytesStart) {
    let Some(rid) = attr_str(e, b"id") else {
        return;
    };
    let target = match attr_str(e, b"type").as_deref() {
        Some("first") => &mut slot.first,
        Some("even") => &mut slot.even,
        _ => &mut slot.default, // default 與缺 w:type（ECMA-376 §17.18.43 預設 default）
    };
    *target = Some(rid);
}

fn attr_i64(e: &quick_xml::events::BytesStart, key: &[u8], def: i64) -> i64 {
    attr_str(e, key)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .map(|v| v as i64)
        .unwrap_or(def)
}

fn attr_str(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key {
            a.unescape_value().ok().map(|v| v.to_string())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xml(s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    /// 典型 Word 檔：單 section（body 級 sectPr，Letter 直式 12240×15840 twip）。
    #[test]
    fn letter_section_converts_twips_to_pt() {
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:p><w:r><w:t>B</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="12240" w:h="15840"/>
                 <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>
               </w:sectPr>
             </w:body></w:document>"#);
        let g = read_sections_per_paragraph(&x, 2);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0], g[1]);
        assert_eq!(g[0].id, 0);
        assert_eq!(g[0].geo.page_w_pt, 612.0, "12240 twip = 612 pt = Letter 寬");
        assert_eq!(g[0].geo.page_h_pt, 792.0, "15840 twip = 792 pt = Letter 高");
        assert_eq!(g[0].geo.margin_top_pt, 72.0);
        assert_eq!(g[0].geo.margin_left_pt, 72.0);
        assert!(!g[0].geo.landscape);
        assert!(!g[0].geo.continuous);
    }

    /// 三 section：直式→橫向→直式（HC031 形態：pPr 內 sectPr 收束 + body 級收尾；
    /// 首尾兩節同尺寸，靠 id 辨識邊界）。
    #[test]
    fn multi_section_landscape_boundary() {
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
               <w:p><w:pPr><w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr></w:pPr><w:r><w:t>第二段</w:t></w:r></w:p>
               <w:p><w:r><w:t>第三段</w:t></w:r></w:p>
               <w:p><w:pPr><w:sectPr><w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/></w:sectPr></w:pPr><w:r><w:t>第四段</w:t></w:r></w:p>
               <w:p><w:r><w:t>第五段</w:t></w:r></w:p>
               <w:p><w:r><w:t>第六段</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>
             </w:body></w:document>"#);
        let g = read_sections_per_paragraph(&x, 6);
        assert_eq!(g.len(), 6);
        // 段 1-2：Letter 直式；段 3-4：Letter 橫向；段 5-6：Letter 直式（body 級收尾）。
        assert_eq!(g[0].geo.page_w_pt, 612.0);
        assert_eq!(g[1].geo.page_w_pt, 612.0);
        assert_eq!(g[2].geo.page_w_pt, 792.0, "橫向段寬 792");
        assert_eq!(g[2].geo.page_h_pt, 612.0);
        assert!(g[2].geo.landscape);
        assert_eq!(g[3].geo.page_w_pt, 792.0, "橫向 section 涵蓋其邊界段");
        assert_eq!(g[4].geo.page_w_pt, 612.0);
        assert_eq!(g[5].geo.page_w_pt, 612.0);
        assert!(!g[5].geo.landscape);
        // section 序號：0,0,1,1,2,2——首尾同尺寸的邊界仍以 id 辨識。
        assert_eq!(
            g.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![0, 0, 1, 1, 2, 2]
        );
    }

    #[test]
    fn continuous_type_flagged() {
        // 邊界段（pPr 帶 sectPr 者）屬於該 sectPr 描述的 section；檔尾段落屬
        // body 級 section。`w:type=continuous` 標在 body 級 → 末節同頁續排。
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:p><w:pPr><w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:pPr></w:p>
               <w:p><w:r><w:t>C</w:t></w:r></w:p>
               <w:sectPr><w:type w:val="continuous"/><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
             </w:body></w:document>"#);
        let g = read_sections_per_paragraph(&x, 3);
        assert_eq!(g.iter().map(|s| s.id).collect::<Vec<_>>(), vec![0, 0, 1]);
        assert!(
            !g[0].geo.continuous,
            "首節（inline sectPr）未標 type → nextPage"
        );
        assert!(
            g[2].geo.continuous,
            "body 級 type=continuous → 末節同頁續排"
        );
    }

    #[test]
    fn no_sectpr_returns_empty_for_fallback() {
        let x = xml(
            r#"<w:document xmlns:w="x"><w:body><w:p><w:r><w:t>A</w:t></w:r></w:p></w:body></w:document>"#,
        );
        assert!(read_sections_per_paragraph(&x, 1).is_empty());
    }

    #[test]
    fn gutter_adds_to_left_margin() {
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="11906" w:h="16838"/>
                 <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:gutter="720"/>
               </w:sectPr>
             </w:body></w:document>"#);
        let g = read_sections_per_paragraph(&x, 1);
        assert_eq!(
            g[0].geo.margin_left_pt, 108.0,
            "left 1440 + gutter 720 = 2160 twip = 108 pt"
        );
        assert_eq!(g[0].geo.margin_right_pt, 72.0);
    }

    #[test]
    fn table_paragraphs_counted_like_parser() {
        // 表格 Cell 內段落也進段落清單（與 parse_document_xml 同口徑）；
        // body 級 sectPr 涵蓋全部段落。
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
               <w:p><w:r><w:t>B</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
             </w:body></w:document>"#);
        let g = read_sections_per_paragraph(&x, 3);
        assert_eq!(g.len(), 3);
        assert_eq!(g[1].geo.page_w_pt, 595.3, "11906 twip = A4 寬 595.3 pt");
    }

    // ---------------- 頁首頁尾宣告（t_b39f108b） ----------------

    /// pgMar header/footer 距離與 titlePg、headerReference/footerReference 讀取。
    #[test]
    fn hf_announcements_read() {
        let x = xml(r#"<w:document xmlns:w="x" xmlns:r="r"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:sectPr>
                 <w:headerReference w:type="default" r:id="rId7"/>
                 <w:headerReference w:type="first" r:id="rId8"/>
                 <w:footerReference w:type="even" r:id="rId9"/>
                 <w:pgSz w:w="11906" w:h="16838"/>
                 <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="851" w:footer="992" w:gutter="0"/>
                 <w:titlePg/>
               </w:sectPr>
             </w:body></w:document>"#);
        let hf = read_sections_header_footer(&x);
        assert_eq!(hf.len(), 1);
        let s = &hf[0];
        assert_eq!(s.header_dist_pt, 42.55, "851 twip ÷ 20 = 42.55 pt");
        assert_eq!(s.footer_dist_pt, 49.6, "992 twip ÷ 20 = 49.6 pt");
        assert!(s.title_pg, "w:titlePg 存在＝開啟");
        assert_eq!(s.header.default.as_deref(), Some("rId7"));
        assert_eq!(s.header.first.as_deref(), Some("rId8"));
        assert_eq!(s.header.even, None);
        assert_eq!(s.footer.even.as_deref(), Some("rId9"));
    }

    /// pgMar 缺 header/footer 屬性 → 預設 720 twip＝36 pt（ECMA-376 預設）。
    #[test]
    fn hf_distance_defaults_to_720_twips() {
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="11906" w:h="16838"/>
                 <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/>
               </w:sectPr>
             </w:body></w:document>"#);
        let hf = read_sections_header_footer(&x);
        assert_eq!(hf[0].header_dist_pt, 36.0);
        assert_eq!(hf[0].footer_dist_pt, 36.0);
        assert!(!hf[0].title_pg);
        assert_eq!(hf[0].header, SlotRefs::default());
    }

    /// `w:titlePg w:val="0"`＝關閉（開關語義，與 b/i 等同款）。
    #[test]
    fn title_pg_val_off_is_false() {
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:titlePg w:val="0"/></w:sectPr>
             </w:body></w:document>"#);
        assert!(!read_sections_header_footer(&x)[0].title_pg);
    }

    /// 跨節繼承（ECMA-376 §17.10.5）：第二節未宣告 header default → 沿用第一節；
    /// 已宣告的 slot 覆寫；第三節兩者皆缺 → 沿鏈吃到第一節的 even。
    #[test]
    fn hf_inherits_across_sections() {
        let x = xml(r#"<w:document xmlns:w="x" xmlns:r="r"><w:body>
               <w:p><w:r><w:t>S1</w:t></w:r></w:p>
               <w:p><w:pPr><w:sectPr>
                 <w:headerReference r:id="rIdH1"/>
                 <w:footerReference w:type="even" r:id="rIdF1"/>
                 <w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:pPr><w:r><w:t>S1b</w:t></w:r></w:p>
               <w:p><w:r><w:t>S2</w:t></w:r></w:p>
               <w:p><w:pPr><w:sectPr>
                 <w:headerReference w:type="first" r:id="rIdH2"/>
                 <w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:pPr><w:r><w:t>S2b</w:t></w:r></w:p>
               <w:p><w:r><w:t>S3</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
             </w:body></w:document>"#);
        let hf = read_sections_header_footer(&x);
        assert_eq!(hf.len(), 3);
        // 第一節：自宣告。
        assert_eq!(hf[0].header.default.as_deref(), Some("rIdH1"));
        assert_eq!(hf[0].footer.even.as_deref(), Some("rIdF1"));
        // 第二節：header.default 繼承第一節；first 自宣告；footer 全slot繼承。
        assert_eq!(
            hf[1].header.default.as_deref(),
            Some("rIdH1"),
            "未宣告 → 沿用第一節"
        );
        assert_eq!(hf[1].header.first.as_deref(), Some("rIdH2"), "自宣告覆寫");
        assert_eq!(hf[1].footer.even.as_deref(), Some("rIdF1"));
        // 第三節：沿鏈吃到第一節（經第二節遞移）。
        assert_eq!(hf[2].header.default.as_deref(), Some("rIdH1"));
        assert_eq!(
            hf[2].header.first.as_deref(),
            Some("rIdH2"),
            "沿鏈吃到第二節 first"
        );
        assert_eq!(hf[2].footer.even.as_deref(), Some("rIdF1"));
        // 逐段落清單口徑不受影響。
        let g = read_sections_per_paragraph(&x, 6);
        assert_eq!(
            g.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![0, 0, 1, 1, 2, 2]
        );
    }

    /// `evenAndOddHeaders`：存在＝true；w:val falsy／缺 part 內容＝false。
    #[test]
    fn even_odd_headers_toggle() {
        assert!(read_even_and_odd_headers(
            br#"<w:settings xmlns:w="x"><w:evenAndOddHeaders/></w:settings>"#
        ));
        assert!(read_even_and_odd_headers(
            br#"<w:settings xmlns:w="x"><w:evenAndOddHeaders w:val="1"/></w:settings>"#
        ));
        assert!(!read_even_and_odd_headers(
            br#"<w:settings xmlns:w="x"><w:evenAndOddHeaders w:val="0"/></w:settings>"#
        ));
        assert!(!read_even_and_odd_headers(
            br#"<w:settings xmlns:w="x"><w:zoom w:val="100"/></w:settings>"#
        ));
    }

    // ---------------- (H1) sectPr 層 footnotePr／endnotePr ----------------

    /// (b) fdo77725.docx：sectPr 的 `w:footnotePr/w:numRestart@w:val=eachSect`
    /// 落 `NoteRestart::EachSect`（實檔值；settings 層另有同值，兩層各自成組）。
    #[test]
    fn fdo77725_section_note_restart() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/ooxmlexport/fdo77725.docx"),
        )
        .expect("讀 fdo77725.docx");
        let doc = crate::parse(&bytes).expect("解析 fdo77725");
        let xml = doc
            .parts
            .get("word/document.xml")
            .expect("document.xml 必在 parts");
        let prs = read_sections_note_pr(xml);
        assert_eq!(prs.len(), 1, "fdo77725 單節");
        let f = prs[0].as_ref().expect("sectPr footnotePr 存在");
        assert_eq!(f.num_restart, Some(crate::NoteRestart::EachSect));
    }

    /// sectPr 層 endnotePr 獨立讀取（fdo77725：`numFmt=decimal`）。
    #[test]
    fn fdo77725_section_endnote_num_fmt() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/ooxmlexport/fdo77725.docx"),
        )
        .expect("讀 fdo77725.docx");
        let doc = crate::parse(&bytes).expect("解析 fdo77725");
        let xml = doc
            .parts
            .get("word/document.xml")
            .expect("document.xml 必在 parts");
        let prs = read_sections_endnote_pr(xml);
        assert_eq!(prs.len(), 1);
        assert_eq!(
            prs[0].as_ref().unwrap().num_fmt,
            Some(crate::NoteNumberFormat::Decimal)
        );
    }

    /// sectPr 無 footnotePr／endnotePr → 該節回 None（逐欄覆寫語意：
    /// 缺＝不覆寫，沿用 settings；本函式如實回報 None）。
    #[test]
    fn sectpr_without_note_pr_is_none() {
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
             </w:body></w:document>"#);
        assert_eq!(read_sections_note_pr(&x), vec![None]);
        assert_eq!(read_sections_endnote_pr(&x), vec![None]);
    }
}
