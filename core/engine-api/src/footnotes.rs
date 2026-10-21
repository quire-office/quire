//! F2b-2：頁底註腳的版面與發射（**只消費**：F1 資料層＋F2a/F2b-1 預留機制）。
//!
//! 座標口徑同顯示列表（`y` 為行頂，繪製端自加字級得基線）：
//!
//! ```text
//! 版心底 content_bottom ─┬──────────────────────────┐
//!                       │ 第 2 條註腳              │
//!                       │ 第 1 條註腳              │ ← reserve（整帶量化）
//!   separator_y ────────┤───── 分隔線（版心 1/3）  │
//!                       │ sep_bands 空白           │
//! 正文可用底 ───────────┴──────────────────────────┘
//! ```
//!
//! `reserve` 經 `LayoutParams::footnote_reserves_pt` 餵給 F2b-1 的收斂分頁入口。
//!
//! 兩條構造性保證（PM review 重點）：
//! 1. **無註腳文件零擾動**：正文的 `w:footnoteReference` run 文字本就為空
//!    （F1 口徑），本卡不剜除、不改寫它，只在該 run 的行內終點發射上標記號；
//!    文件無註腳時預留表恆空 ⇒ 分頁走既有路徑，一個位元組都不變。
//! 2. **收斂有界**：保留帶以**整帶**量化（值域離散）。分頁是預留向量的純函數
//!    （F2b-1），連續值預留會讓 1pt 內的浮動翻轉分頁、3 輪可能不收斂；
//!    整帶量化消除該震盪源，輪數上限仍由 `MAX_CONVERGE_ROUNDS` 兜底。
//!
//! 取捨（誠實劃界，F2c／F3 收）：
//! - 註腳高度以註腳自身行高計、再量化到全頁帶尺（一條註腳最少 1 帶）；
//!   故註腳字級與正文帶尺不同時，保留帶可能有 ≤1 帶的保守多留（多留不倒扣）。
//! - 表格／文字框內的錨點不在正文流行程內 ⇒ 其註腳落到**最後一頁**（不靜默遺失）。
//! - 分欄文件的註腳掛整頁版心下方（不逐欄分割）。
//! - `separator`／`continuationSeparator` 型註本體不進條目（Word 慣例
//!   id=-1/0，非正文引用對象）。
//!
//! 尾註（endnote，F3）：**不掛頁底**——全部排在正文之後的專屬頁（追加頁，
//! 不擠正文）。尾註支路與本模組的頁底註腳支路**並存且互不干擾**：兩條鏈
//! 各自獨立（`note_ref` 是單欄位，同一個 run 不會同時掛兩者 ⇒ 無競態）。
//! 尾註的編號是**十進位**；`endnotePr/numFmt`（lowerRoman 等）的消費屬
//! F3b，本卡不做——老實記在模組注裡，不假裝支援。

use quire_layout::TextMeasurer;
use serde::{Deserialize, Serialize};

/// 分隔線與首條註腳間的空白目標（pt；實際以整帶向上取整）。
pub const SEP_GAP_PT: f32 = 8.0;
/// 註腳條目懸掛縮排（pt；Word 註腳樣式約 0.25 吋）。
pub const NOTE_INDENT_PT: f32 = 18.0;
/// 編號上標的字級縮小比。
pub const SUP_SCALE: f32 = 0.65;
/// 上標基線上移比（乘條目行高；發射端把行頂上移本值 ⇒ 基線隨之上移）。
pub const SUP_RISE: f32 = 0.45;
/// F2c：有註腳的頁**正文至少保留的帶數**（與 layout 層容量兜底 `.max(1)`
/// 同尺——頁不能整版被註腳吃掉，否則 `pack_run` 停滯）。註腳區預算＝
/// 頁容量帶 − 本值 − 分隔線帶。
pub const MIN_BODY_BANDS: u32 = 1;
/// 分隔線寬＝版心寬 × 本比值（Word 預設）。
pub const SEP_WIDTH_RATIO: f32 = 1.0 / 3.0;

/// 一則註腳的發射級樣式（engine 級聯烘焙後餵入；本模組不再看 styles.xml）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryStyle {
    pub size_pt: f32,
    pub line_h_pt: f32,
    pub color_argb: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub family: Option<String>,
}

/// 一條註腳（engine 端構建：`body`＝註腳首段拍平文字，`number`＝正文引用序）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteEntry {
    /// `w:footnote/@w:id`（與正文引用同一命名空間）。
    pub id: i64,
    /// 顯示編號（1 起，按正文_anchor_序）。
    pub number: usize,
    pub body: String,
    pub style: EntryStyle,
}

/// 註腳條目內一段文字的發射屬性＋行內位移（發射端零判斷）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteRun {
    pub text: String,
    /// 行內 x（pt，**相對版心左緣**；發射端自加 margin_left）。
    pub x_pt: f32,
    pub size_pt: f32,
    pub color_argb: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub family: Option<String>,
    /// 上標（編號前綴）：發射端把本 run 行頂上移 `SUP_RISE × 條目行高`。
    pub superscript: bool,
}

/// 一條註腳換行後的一行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteLine {
    pub runs: Vec<NoteRun>,
}

/// 一頁中的一條註腳條目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteItem {
    pub lines: Vec<NoteLine>,
    /// 所佔帶數（量化到全頁帶尺；>= 1）。
    pub bands: u32,
    /// 條目行高（pt；註腳自身尺度——發射端逐行累加本值）。
    pub line_h_pt: f32,
    /// F2c：本條是某則註腳的**續排段**（該則註腳跨頁，本段是第二段的起）。
    /// 首段（含編號前綴行）＝false。serde default＝false：舊構造點零改寫。
    #[serde(default)]
    pub continuation: bool,
}

/// 一頁的頁底註腳排布。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageFoot {
    pub items: Vec<NoteItem>,
    /// 本頁保留帶數（整帶量化；無註腳 ⇒ 0）。
    pub reserve_bands: u32,
    /// 本頁保留帶高度（pt＝reserve_bands × 帶尺）；餵 `footnote_reserves_pt`。
    pub reserve_pt: f32,
    /// 分隔線 y（行頂語義；無註腳 ⇒ None）。
    pub separator_y: Option<f32>,
    /// 首條註腳的行頂 y（pt；無註腳 ⇒ None）。
    pub items_top_y: Option<f32>,
    /// 分隔線端點 x（pt，頁絕對；無註腳 ⇒ None）。
    pub separator_x1: Option<f32>,
    pub separator_x2: Option<f32>,
    /// 帶尺（pt；發射端累加條目 y 用）。
    pub band_pt: f32,
    /// F2c：本頁註腳區**只有續排段**（無任何首段）⇒ 分隔線畫**續接線**
    /// （置中短線；ECMA-376 §17.11.4 continuationSeparator 的排版語義）。
    #[serde(default)]
    pub continuation_only: bool,
}

/// 正文錨點上標記號（發在該 run 的行內終點）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorMark {
    /// x（pt，相對版心左緣；發射端自加 margin_left）。
    pub x_pt: f32,
    /// 所在行的行頂（pt，顯示列表 y 口徑；本欄已含上標上移）。
    pub y_pt: f32,
    pub text: String,
    pub size_pt: f32,
    pub color_argb: u32,
    pub family: Option<String>,
}

/// 全文件的註腳渲染資料（頁序與 `Engine::page_count` 對齊）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NoteLayout {
    pub pages: Vec<PageFoot>,
    /// 逐頁錨點記號（依正文閱讀序）。
    pub marks: Vec<Vec<AnchorMark>>,
    /// 最終逐頁預留表（餵 `LayoutParams::footnote_reserves_pt`；收斂定案值）。
    pub reserves: Vec<f32>,
    /// 收斂實際輪數（觀察用；1＝一輪定案）。
    pub rounds: usize,
}

impl NoteLayout {
    /// 文件是否有任何頁底註腳發射（false ⇒ 發射層走零擾動快路徑）。
    pub fn is_empty(&self) -> bool {
        self.pages.iter().all(|p| p.items.is_empty()) && self.marks.iter().all(|m| m.is_empty())
    }
}

/// 正文中的一個註腳引用錨點。
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    pub note_id: i64,
    /// 段落序／段內 run 序（發射投影原序）。
    pub para: usize,
    pub run: usize,
    /// 該 run 之前已消費的碼點數（段內；顯示列表以 `Line::char_offset` 為
    /// 碼點標尺，發射端由此換算行內 x）。
    pub cp_before: usize,
    /// 該 run 自身的碼點數（記號落在 run 終點＝cp_before + len）。
    pub cp_len: usize,
}

/// 掃描正文流的註腳錨點（只取 `w:footnoteReference`）。
///
/// 輸入是 engine 的發射投影（`eff_paragraphs()`）——錨點序必須與發射端
/// 讀到的段落/run 序一致，故不能用 doc 本體。
pub fn collect_anchors(paras: &[quire_ooxml::Paragraph]) -> Vec<Anchor> {
    collect_anchors_of(paras, quire_ooxml::NoteKind2::Footnote)
}

/// 掃描正文流的**尾註**錨點（只取 `w:endnoteReference`；F3 尾註支路）。
///
/// 與 [`collect_anchors`] 完全同構（同樣以發射投影為輸入、錨點序＝段落/
/// run 序、cp 標尺同款），只是取另一種引用型別。既有函式簽名不動
/// （既有呼叫點零改動）——兩條支路各自獨立跑：`note_ref` 是單欄位，
/// 同一個 run 不會同時掛註腳與尾註 ⇒ 兩條鏈無競態。
pub fn collect_endnote_anchors(paras: &[quire_ooxml::Paragraph]) -> Vec<Anchor> {
    collect_anchors_of(paras, quire_ooxml::NoteKind2::Endnote)
}

/// 依引用型別掃描正文流的註錨點（兩條支路共用的實作）。
fn collect_anchors_of(
    paras: &[quire_ooxml::Paragraph],
    kind: quire_ooxml::NoteKind2,
) -> Vec<Anchor> {
    let mut out = Vec::new();
    for (pi, p) in paras.iter().enumerate() {
        let mut cp = 0usize;
        for (ri, r) in p.runs.iter().enumerate() {
            let len = r.text.chars().count();
            if let Some(nf) = r.note_ref.as_ref() {
                if nf.kind == kind {
                    out.push(Anchor {
                        note_id: nf.id,
                        para: pi,
                        run: ri,
                        cp_before: cp,
                        cp_len: len,
                    });
                }
            }
            cp += len;
        }
    }
    out
}

/// 一條註腳的換行＋帶數。
///
/// - `band_pt`＝該頁帶尺（全頁行網格尺度；帶數以此量化）
/// - `avail_pt`＝版心寬（縮排與編號寬在內部自行扣除）
pub fn layout_entry(entry: &NoteEntry, band_pt: f32, avail_pt: f32) -> NoteItem {
    let st = &entry.style;
    let size = st.size_pt.max(1.0);
    let m = TextMeasurer::new(size);
    let prefix = format!("{}.", entry.number);
    let prefix_w = m.text_width(&prefix);
    let space_w = m.text_width(" ");
    let indent = NOTE_INDENT_PT.max(0.0);
    let first_w = (avail_pt - indent - prefix_w - space_w).max(1.0);
    let rest_w = (avail_pt - indent).max(1.0);
    let body = entry.body.as_str();

    let mut lines: Vec<NoteLine> = Vec::new();
    let mut rest = body;
    let mut first = true;
    loop {
        let w = if first { first_w } else { rest_w };
        let (head, tail) = take_line(rest, w, &m);
        let mut runs = Vec::new();
        let mut x = indent;
        if first {
            runs.push(NoteRun {
                text: prefix.clone(),
                x_pt: x,
                size_pt: (size * SUP_SCALE).max(1.0),
                color_argb: st.color_argb,
                bold: st.bold,
                italic: st.italic,
                underline: st.underline,
                strike: st.strike,
                family: st.family.clone(),
                superscript: true,
            });
            x += prefix_w;
            if !head.is_empty() {
                x += space_w;
            }
        }
        if !head.is_empty() {
            runs.push(NoteRun {
                text: head.to_string(),
                x_pt: x,
                size_pt: size,
                color_argb: st.color_argb,
                bold: st.bold,
                italic: st.italic,
                underline: st.underline,
                strike: st.strike,
                family: st.family.clone(),
                superscript: false,
            });
        }
        if !(runs.is_empty() && !lines.is_empty()) {
            lines.push(NoteLine { runs });
        }
        if tail.is_empty() {
            break;
        }
        rest = tail;
        first = false;
    }
    if lines.is_empty() {
        lines.push(NoteLine { runs: Vec::new() });
    }

    let height = st.line_h_pt.max(1.0) * lines.len() as f32;
    let band = band_pt.max(1.0);
    let bands = (height / band).ceil().max(1.0) as u32;
    NoteItem {
        lines,
        bands,
        line_h_pt: st.line_h_pt.max(1.0),
        // 本函式產出的是「整條」註腳（未拆分）⇒ 首段語義。
        continuation: false,
    }
}

/// 取一行：回 (本行含分隔空白, 其餘)。優先在空白處斷；單字過長硬切一個字元
/// 保證遞進（不 panic、不溢出）。行首不留空白（trim 起始空白）。
fn take_line<'a>(s: &'a str, max_w: f32, m: &TextMeasurer) -> (&'a str, &'a str) {
    let s = s.trim_start_matches([' ', '\t']);
    if s.is_empty() {
        return ("", "");
    }
    if m.text_width(s) <= max_w {
        return (s, "");
    }
    let mut best: Option<(&'a str, &'a str)> = None;
    let mut boundary: Option<(&'a str, &'a str)> = None;
    for (i, ch) in s.char_indices() {
        if i == 0 {
            continue;
        }
        let head = &s[..i];
        if m.text_width(head) > max_w {
            break;
        }
        best = Some((head, &s[i..]));
        if ch == ' ' || ch == '\t' {
            boundary = Some((&s[..i], &s[i + ch.len_utf8()..]));
        }
    }
    if let Some(b) = boundary {
        return b;
    }
    if let Some((head, tail)) = best {
        // 無空白可斷：切到最長可容前綴（至少一個字元）。
        if head.is_empty() {
            let cut = s.char_indices().nth(1).map(|(i, _)| i).unwrap_or(s.len());
            return (&s[..cut], &s[cut..]);
        }
        return (head, tail);
    }
    let cut = s.char_indices().nth(1).map(|(i, _)| i).unwrap_or(s.len());
    (&s[..cut], &s[cut..])
}

/// 把逐頁條目組裝成一頁排布（含保留帶、分隔線與條目起點）。
pub fn page_foot(items: Vec<NoteItem>, g: &PageInfo) -> PageFoot {
    if items.is_empty() {
        return PageFoot::default();
    }
    let band = g.band_pt.max(1.0);
    let sep_bands = (SEP_GAP_PT / band).ceil().max(1.0) as u32;
    let content_bands: u32 = items.iter().map(|i| i.bands).sum::<u32>().max(1);
    let reserve_bands = sep_bands + content_bands;
    let reserve_pt = reserve_bands as f32 * band;
    let separator_y = g.content_bottom_pt - reserve_pt;
    let (x1, x2) = separator_span(g.content_left_pt, g.content_w_pt);
    // F2c：全頁都是續排段 ⇒ 該頁分隔線是「續接線」（發射端據此改畫置中短線）。
    let continuation_only = items.iter().all(|i| i.continuation);
    PageFoot {
        separator_y: Some(separator_y),
        items_top_y: Some(separator_y + sep_bands as f32 * band),
        separator_x1: Some(x1),
        separator_x2: Some(x2),
        band_pt: band,
        continuation_only,
        reserve_bands,
        reserve_pt,
        items,
    }
}

/// 分隔線端點 x（版心左緣起，寬＝版心 × `SEP_WIDTH_RATIO`）。
pub fn separator_span(content_left_pt: f32, content_w_pt: f32) -> (f32, f32) {
    (
        content_left_pt,
        content_left_pt + content_w_pt * SEP_WIDTH_RATIO,
    )
}

/// 逐頁預留表（餵 `LayoutParams::footnote_reserves_pt`）。
pub fn reserves(foot: &[PageFoot]) -> Vec<f32> {
    foot.iter().map(|f| f.reserve_pt).collect()
}

/// 預留正規化：負值／NaN 視同 0（與 layout 層 `reserve_or_zero` 同款口徑）。
fn norm(v: f32) -> f32 {
    if v.is_nan() || v < 0.0 {
        0.0
    } else {
        v
    }
}

/// 兩輪預留是否等價（收斂判準；短者補 0、長者截斷——同 layout 層口徑）。
pub fn reserves_stable(a: &[f32], b: &[f32]) -> bool {
    let n = a.len().max(b.len());
    (0..n).all(|i| norm(a.get(i).copied().unwrap_or(0.0)) == norm(b.get(i).copied().unwrap_or(0.0)))
}
/// 一頁的版面事實（engine 從實際分頁結果餵入；本模組用它把註腳分配到頁）。
///
/// `content_bottom_pt` 是**未扣預留**的正文可用底（margin_top ＋ 全容量 × 帶尺）：
/// 分隔線與條目由此往上堆疊，正好落在該頁保留帶內（保留帶＝reserve_bands × 帶尺）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageInfo {
    /// 版心左緣（pt）。
    pub content_left_pt: f32,
    /// 版心寬（pt）。
    pub content_w_pt: f32,
    /// 未扣預留的正文可用底（pt）。
    pub content_bottom_pt: f32,
    /// 帶尺（pt）。
    pub band_pt: f32,
    /// F2c：本頁**未扣預留**的總容量帶數（engine 以 `section_capacity` 餵）。
    /// 註腳跨頁拆分的預算基準——必須錨定固定容量，不能跟隨逐輪預留浮動，
    /// 否則收斂迴圈自我震盪。0＝未知（拆分支路保守退化為不拆分）。
    pub capacity_bands: u32,
}

/// 錨點記號的發射樣式（engine 由該 anchor run 的級聯有效值餵入）。
#[derive(Debug, Clone, PartialEq)]
pub struct MarkStyle {
    pub size_pt: f32,
    pub color_argb: u32,
    pub family: Option<String>,
}

/// 收斂迴圈的**單次求值**：錨點定位 → 頁分配 → 條目排布 → 逐頁預留。
///
/// 定位順序（為什麼這樣排）：
/// 1. 先掃各頁行程找錨點所在行（決定「哪一頁有這則註腳」＋記號座標）。
///    段落跨頁時以**記號實際落點**為準，不用段落首/末 index 猜——那是跨頁
///    段落唯一不會兩頁都算進去（⇒ 預留抖動不收斂）的判準。
/// 2. 找不到行（錨點在表格／文字框內，不在正文流行程中）⇒ 該則註腳落到
///    **最後一頁**（不靜默遺失；記號不發射，誠實）。
/// 3. 同一則註腳被多頁引用 ⇒ 每頁各發一條（編號相同）；同頁重複引用 ⇒ 一條。
///
/// `rounds` 由呼叫端（收斂迴圈）填；本函式純函數、不帶狀態。
#[allow(clippy::too_many_arguments)] // 版面事實/錨點/條目/映射/樣式皆必要輸入，不塞結構體中转
pub fn layout_notes(
    pages: &[quire_layout::Page],
    info: &[PageInfo],
    anchors: &[Anchor],
    entries: &[NoteEntry],
    note_of_anchor: &[usize],
    body_size_pt: f32,
    styles: &[MarkStyle],
    rounds: usize,
) -> NoteLayout {
    let mut per_page: Vec<Vec<usize>> = vec![Vec::new(); pages.len()];
    let mut marks: Vec<Vec<AnchorMark>> = vec![Vec::new(); pages.len()];
    for (ai, a) in anchors.iter().enumerate() {
        let Some(&ni) = note_of_anchor.get(ai) else {
            continue;
        };
        if ni >= entries.len() {
            continue;
        }
        let cp_end = a.cp_before + a.cp_len;
        // (1) 定位：段內 cp 終點落在哪一頁的哪一行（記號口徑與尾註支路共用
        //     `anchor_mark`——兩者規則必須逐位元組相同，故只有一份實作）。
        let hit = anchor_mark(
            pages,
            info,
            a,
            cp_end,
            entries[ni].number,
            styles.get(ai),
            body_size_pt,
        );
        let (page_idx, mark) = match hit {
            Some((pi, m)) => (pi, Some(m)),
            // (2) 定位失敗：註腳落末頁、記號不發。
            None => (pages.len().saturating_sub(1), None),
        };
        if per_page[page_idx].contains(&ni) {
            continue; // (3) 同頁同則只一條（記號亦只一條）。
        }
        per_page[page_idx].push(ni);
        if let Some(m) = mark {
            marks[page_idx].push(m);
        }
    }
    // F2c：裝箱＋跨頁拆分（單頁可納的註腳走 `layout_entry` 原路徑，數值零變化）。
    let banded = distribute_with_splits(&per_page, entries, info);
    let mut page_foots = Vec::with_capacity(pages.len());
    for (pi, page) in pages.iter().enumerate() {
        let g = info.get(pi).copied().unwrap_or(PageInfo {
            content_left_pt: page.margin_left_pt,
            content_w_pt: 1.0,
            content_bottom_pt: page.margin_top_pt,
            band_pt: 16.8,
            capacity_bands: 0,
        });
        let items: Vec<NoteItem> = banded[pi].iter().map(|(_, it)| it.clone()).collect();
        page_foots.push(page_foot(items, &g));
    }
    let reserves = reserves(&page_foots);
    NoteLayout {
        pages: page_foots,
        marks,
        reserves,
        rounds,
    }
}

/// 一條註腳一行所佔帶數（ceil(行高/帶尺)，至少 1）。
fn bands_per_line(line_h_pt: f32, band_pt: f32) -> u32 {
    (line_h_pt.max(1.0) / band_pt.max(1.0)).ceil().max(1.0) as u32
}

/// F2c：該頁註腳區可占的帶預算（負值夾 0）。
///
/// `capacity`＝該頁未扣預留的總容量帶（`PageInfo::capacity_bands`）；
/// `min_body`＝正文保底帶（`MIN_BODY_BANDS`，與 layout 層容量兜底同尺：
/// 頁不能整版被註腳吃掉，否則 `pack_run` 停滯）；`sep_bands`＝該頁
/// 分隔線／續接線帶。
pub fn page_budget_bands(capacity: u32, min_body: u32, sep_bands: u32) -> u32 {
    capacity.saturating_sub(min_body).saturating_sub(sep_bands)
}

/// F2c：把逐頁註腳按預算裝箱，超頁的註腳沿頁序**拆分續排**（Word 同款：
/// 長註腳跨頁續排，編號前綴只屬於首段）。
///
/// 規則（測死）：
/// - 每頁預算＝`page_budget_bands(capacity_bands, MIN_BODY_BANDS, sep)`；
///   `capacity_bands == 0`（未知）⇒ 該頁不拆分（保守退化，零變化口徑）。
/// - 先放來自前頁的**續排段**（閱讀序在前），再放本頁新註腳（錨點序）。
/// - 整條放得下 ⇒ 原樣放（`layout_entry` 的原 bands——單頁可納路徑數值零變化）。
/// - 放不下且本頁塞得下一行 ⇒ 切 `floor(room/每行帶)` 行（至少 1）作本頁段，
///   其餘成續排段（`continuation=true`，行區間天然不含編號前綴行）推往後頁。
/// - 本頁連一行都塞不下 ⇒ 整段（前綴保留在原段）推往後頁。
/// - 走到最後仍有余段 ⇒ 誠實溢出：全部掛末頁（寧可溢出重疊，不靜默吞字；
///   真·新增頁屬 F3 分頁回饋，本卡不做）。
///
/// 收斂性論證（給 F2b-2 收斂迴圈用）：預算錨定在**未扣預留的固定容量**
/// （`PageInfo::capacity_bands`），拆分子對 per_page 決定性；預留變動不進
/// 拆分子 ⇒ 不自我震盪。注意：拆分段的帶數用「逐行 ceil」刻度
/// （`bands_per_line`×行數 ≥ `layout_entry` 的整體 ceil），與未拆分段可差
/// 1 帶——只在溢出路徑出現，方向保守（多留不擠）。
fn distribute_with_splits(
    per_page: &[Vec<usize>],
    entries: &[NoteEntry],
    infos: &[PageInfo],
) -> Vec<Vec<(usize, NoteItem)>> {
    use std::collections::VecDeque;
    let n = per_page.len();
    let mut out: Vec<Vec<(usize, NoteItem)>> = vec![Vec::new(); n];
    if n == 0 {
        return out;
    }
    let sep_of = |p: usize| -> u32 {
        let band = infos.get(p).map(|g| g.band_pt.max(1.0)).unwrap_or(16.8);
        (SEP_GAP_PT / band).ceil().max(1.0) as u32
    };
    let room_of = |p: usize| -> u32 {
        match infos.get(p) {
            // 容量未知（0）⇒ 不拆分：給飽和預算。
            Some(g) if g.capacity_bands == 0 => u32::MAX,
            Some(g) => page_budget_bands(g.capacity_bands, MIN_BODY_BANDS, sep_of(p)),
            None => u32::MAX,
        }
    };
    let mut room: Vec<u32> = (0..n).map(room_of).collect();
    let mut pending: VecDeque<(usize, NoteItem)> = VecDeque::new();
    for p in 0..n {
        let g = infos.get(p).copied().unwrap_or(PageInfo {
            content_left_pt: 0.0,
            content_w_pt: 1.0,
            content_bottom_pt: 0.0,
            band_pt: 16.8,
            capacity_bands: 0,
        });
        let mut queue: VecDeque<(usize, NoteItem)> = VecDeque::new();
        queue.append(&mut pending);
        for &ni in &per_page[p] {
            if ni < entries.len() {
                queue.push_back((ni, layout_entry(&entries[ni], g.band_pt, g.content_w_pt)));
            }
        }
        while let Some((ni, mut item)) = queue.pop_front() {
            if item.bands <= room[p] {
                room[p] = room[p].saturating_sub(item.bands);
                out[p].push((ni, item));
                continue;
            }
            let per = bands_per_line(item.line_h_pt, g.band_pt);
            if room[p] < per {
                // 連一行都塞不下：本段與其後全部推往後頁（序保持）。
                pending.push_back((ni, item));
                pending.append(&mut queue);
                break;
            }
            let take = ((room[p] / per) as usize).max(1).min(item.lines.len());
            let head: Vec<NoteLine> = item.lines.drain(..take).collect();
            let head_bands = take as u32 * per;
            room[p] = room[p].saturating_sub(head_bands);
            out[p].push((
                ni,
                NoteItem {
                    lines: head,
                    bands: head_bands,
                    line_h_pt: item.line_h_pt,
                    continuation: item.continuation,
                },
            ));
            if !item.lines.is_empty() {
                // 其餘成續排段（前綴行已在首段，天然不含編號）。
                item.bands = item.lines.len() as u32 * per;
                item.continuation = true;
                queue.push_front((ni, item));
            }
        }
    }
    // 誠實溢出：全頁耗盡仍有剩餘 ⇒ 掛末頁。
    if n > 0 {
        while let Some(x) = pending.pop_front() {
            out[n - 1].push(x);
        }
    }
    out
}

/// 錨點記號（發在該 anchor run 的行內終點）：回 (正文頁序, 記號)。
///
/// 定位口徑同 [`find_anchor_line`]：段落跨頁時以**記號實際落點**為準；
/// 定位失敗 ⇒ `None`（老實留白，不虛構記號）。座標口徑與顯示列表一致
/// （`y` 為行頂，本欄已含上標上移）。
///
/// 註腳與尾註**共用**本函式——兩者的記號規則必須逐位元組相同（PM 契約：
/// 「規則與 F2b-2 註腳記號完全相同」），故只有一份實作。
pub fn anchor_mark(
    pages: &[quire_layout::Page],
    info: &[PageInfo],
    anchor: &Anchor,
    cp_end: usize,
    number: usize,
    style: Option<&MarkStyle>,
    body_size_pt: f32,
) -> Option<(usize, AnchorMark)> {
    let body = body_size_pt.max(1.0);
    let (pi, li, rel_cp) = find_anchor_line(pages, anchor.para, cp_end)?;
    let page = &pages[pi];
    let line = &page.lines[li];
    let byte = line
        .text
        .char_indices()
        .nth(rel_cp)
        .map(|(b, _)| b)
        .unwrap_or(line.text.len());
    let x = TextMeasurer::new(body).text_width(&line.text[..byte]);
    let band_idx = page.line_bands.get(li).copied().unwrap_or(li as u32) as f32;
    let band = info.get(pi).map(|g| g.band_pt.max(1.0)).unwrap_or(16.8);
    let st = style.cloned().unwrap_or(MarkStyle {
        size_pt: body,
        color_argb: 0xFF00_0000,
        family: None,
    });
    Some((
        pi,
        AnchorMark {
            x_pt: page.margin_left_pt + x,
            y_pt: page.margin_top_pt + band_idx * band - band * SUP_RISE,
            text: format!("{}", number),
            size_pt: (st.size_pt.max(1.0) * SUP_SCALE).max(1.0),
            color_argb: st.color_argb,
            family: st.family,
        },
    ))
}

/// 找錨點落點：段內第 `cp` 碼點所在的 (頁序, 行序, 行內 cp)。
///
/// 行的 `char_offset` 是段內碼點標尺（Card A 契約），直接比較即可。落在行尾
/// 邊界時歸**前一行**（記號貼在前文之後，與 Word 的「上標跟引用文字」一致）；
/// `cp == 0`（段首即錨點，例：段落以註腳引用開頭）歸本行行首。無任何行回 None。
pub fn find_anchor_line(
    pages: &[quire_layout::Page],
    para: usize,
    cp: usize,
) -> Option<(usize, usize, usize)> {
    for page in pages {
        for (li, line) in page.lines.iter().enumerate() {
            if line.para_index != para {
                continue;
            }
            let line_cp = line.text.chars().count();
            let start = line.char_offset;
            if cp == 0 {
                return Some((page.index.min(pages.len() - 1), li, 0));
            }
            if cp > start && cp <= start + line_cp {
                return Some((page.index.min(pages.len() - 1), li, cp - start));
            }
        }
    }
    None
}

// ---------------- 尾註（F3）----------------
//
// 尾註**不掛頁底**：全部排在正文之後的**專屬頁**（ECMA-376 §17.11.10；
// Word 預設 `endnotePr/pos=documentEnd`）。本段只消費 F1 資料層
// （`word/endnotes.xml` ＋ 正文 `w:endnoteReference`）與既有條目尺
// （`layout_entry`），**不重寫**頁底註腳機制。
//
// 三條幾何鐵律（PM 定案）：
// 1. **正文頁零位移**：尾註只在正文分頁定案後**追加頁**；正文每一頁的
//    內容與分頁斷點與無尾註時逐位元組相同（尾註不預留、不擠正文——
//    這正是它與頁底註腳的根本差異）。
// 2. **尾註頁幾何＝末節**（正文末頁的 section）。
// 3. **條目整條不跨頁**：一條裝不下就整條去下一頁（註腳跨頁切分屬 F2c，
//    尾註同款延後——本卡老實不切條）。

/// 尾註首頁分隔線佔帶數：1 帶畫線 ＋ `SEP_GAP_PT` 量化帶留白。
///
/// 條目區自 `margin_top ＋ 本值×帶尺` 起（首頁容量也要扣掉這幾帶）。
/// 整帶量化（與頁底註腳同款）：值域離散 ⇒ 幾何可重現。
pub fn endnote_sep_bands(band_pt: f32) -> u32 {
    let band = band_pt.max(1.0);
    1 + (SEP_GAP_PT / band).ceil().max(1.0) as u32
}

/// 尾註頁的幾何輸入（末節幾何量化後的裝箱尺；純資料，無行為）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EndGeometry {
    /// 帶尺（pt；條目帶數以此量化）。
    pub band_pt: f32,
    /// 版心寬（pt；條目換行尺）。
    pub content_w_pt: f32,
    /// 首頁容量（帶；已扣掉分隔線佔帶）。
    pub capacity_first: usize,
    /// 後續頁容量（帶；末節全容量）。
    pub capacity_rest: usize,
}

/// 尾註的版面與發射（**追加頁**：正文分頁定案後才排）。
///
/// 幾何契約（PM 定案）：
/// - 尾註頁＝追加頁，正文每頁內容與分頁斷點與無尾註時逐位元組相同；
/// - 尾註頁幾何＝末節（正文末頁的 section）；
/// - 首頁頂部有分隔線（版心左緣起、寬＝版心×1/3、y＝margin_top），
///   條目區自 `margin_top ＋ (1＋SEP_GAP 量化帶)×帶尺` 起；
/// - 後續頁**不畫**分隔線（Word 的 continuationSeparator 屬 F3b，
///   本卡老實留白——不憑空畫一條沒有依據的線）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EndLayout {
    /// 逐尾註頁的條目（條目整條不跨頁——一條裝不下就整條去下一頁）。
    pub pages: Vec<Vec<NoteItem>>,
    /// 逐**正文頁**的錨點記號（記號發在正文錨點 run 的行內終點；
    /// 索引＝正文頁序，長度＝正文頁數）。
    pub marks: Vec<Vec<AnchorMark>>,
    /// 首頁是否畫分隔線（true＝畫在頁頂）。
    pub separator_top: bool,
}

impl EndLayout {
    /// 文件是否有任何尾註發射（false ⇒ 發射層走零擾動快路徑）。
    pub fn is_empty(&self) -> bool {
        self.pages.iter().all(|p| p.is_empty()) && self.marks.iter().all(|m| m.is_empty())
    }
}

/// 條目逐頁裝箱（純函式）：回每頁的**條目索引**（依序、不漏不重）。
///
/// 容量以「帶」計（與 `layout_entry` 的帶數同尺）。首頁容量與後續頁
/// 容量分開給（首頁要扣掉分隔線佔帶）。
///
/// 邊界（樣樣不炸）：
/// - 空輸入 ⇒ 空頁面清單；
/// - 容量 ≤ 0 ⇒ 視同 1（不空迴圈、不吞條目）；
/// - **單條帶數超過整頁容量 ⇒ 該條獨佔一頁**（不切條、不丟條）。
pub fn flow_items_to_pages(
    items: &[NoteItem],
    capacity_first: usize,
    capacity_rest: usize,
) -> Vec<Vec<usize>> {
    let cap_first = capacity_first.max(1) as u32;
    let cap_rest = capacity_rest.max(1) as u32;
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    let mut cur_bands: u32 = 0;
    let mut page_no = 0usize;
    for (i, it) in items.iter().enumerate() {
        let cap = if page_no == 0 { cap_first } else { cap_rest };
        let bands = it.bands.max(1);
        if !cur.is_empty() && cur_bands.saturating_add(bands) > cap {
            out.push(std::mem::take(&mut cur));
            cur_bands = 0;
            page_no += 1;
        }
        cur.push(i);
        cur_bands = cur_bands.saturating_add(bands);
        if cur_bands >= cap {
            // 單條自帶就超過整頁容量 ⇒ 獨佔一頁（不切條、不丟條）。
            out.push(std::mem::take(&mut cur));
            cur_bands = 0;
            page_no += 1;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 尾註的排布：條目裝頁 ＋ 正文錨點記號。
///
/// 與 [`layout_notes`] 的關鍵差異（幾何契約）：
/// - 條目**不掛頁底**：全部進尾註專屬頁（追加在正文之後）；
/// - 條目**整條不跨頁**（一條裝不下就整條去下一頁）；
/// - 記號仍在**正文頁**發（正文引用處的上標數字），口徑與註腳記號同款
///   （共用 [`anchor_mark`]——只有一份實作）。
///
/// 同一則尾註被引用多次 ⇒ 條目只排**一條**（Word 慣例：尾註列一次），
/// 但每個引用處仍各發一個記號（上標數字是引用點的記號，不是條目的記號）。
#[allow(clippy::too_many_arguments)] // 版面事實/錨點/條目/映射/樣式/幾何皆必要輸入，同 layout_notes 口徑
pub fn layout_endnotes(
    pages: &[quire_layout::Page],
    info: &[PageInfo],
    anchors: &[Anchor],
    entries: &[NoteEntry],
    note_of_anchor: &[usize],
    body_size_pt: f32,
    styles: &[MarkStyle],
    geo: &EndGeometry,
) -> EndLayout {
    // 條目：同一則尾註只排一條（依首次引用序＝編號序）。
    let mut seen: Vec<i64> = Vec::new();
    let mut items: Vec<NoteItem> = Vec::new();
    for e in entries.iter() {
        if seen.contains(&e.id) {
            continue;
        }
        seen.push(e.id);
        items.push(layout_entry(e, geo.band_pt, geo.content_w_pt));
    }
    // 裝頁（純函式；單條超容量 ⇒ 獨佔一頁）。
    let assign = flow_items_to_pages(&items, geo.capacity_first, geo.capacity_rest);
    let end_pages: Vec<Vec<NoteItem>> = assign
        .iter()
        .map(|idxs| idxs.iter().map(|&i| items[i].clone()).collect())
        .collect();
    // 記號（正文頁；口徑與註腳記號同款）。
    let mut marks: Vec<Vec<AnchorMark>> = vec![Vec::new(); pages.len()];
    for (ai, a) in anchors.iter().enumerate() {
        let Some(&ni) = note_of_anchor.get(ai) else {
            continue;
        };
        if ni >= entries.len() {
            continue;
        }
        let cp_end = a.cp_before + a.cp_len;
        let Some((pi, m)) = anchor_mark(
            pages,
            info,
            a,
            cp_end,
            entries[ni].number,
            styles.get(ai),
            body_size_pt,
        ) else {
            continue; // 定位失敗：老實不發記號（不虛構座標）。
        };
        if let Some(slot) = marks.get_mut(pi) {
            slot.push(m);
        }
    }
    EndLayout {
        separator_top: !end_pages.is_empty(),
        pages: end_pages,
        marks,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use quire_ooxml::NoteKind2;

    const BAND: f32 = 16.8;

    fn entry(number: usize, body: &str) -> NoteEntry {
        NoteEntry {
            id: number as i64,
            number,
            body: body.to_string(),
            style: EntryStyle {
                size_pt: 10.0,
                line_h_pt: 14.0,
                color_argb: 0xFF00_0000,
                bold: false,
                italic: false,
                underline: false,
                strike: false,
                family: Some("serif".to_string()),
            },
        }
    }

    fn anchor(note_id: i64, para: usize, run: usize, cp_before: usize) -> Anchor {
        Anchor {
            note_id,
            para,
            run,
            cp_before,
            cp_len: 0,
        }
    }

    fn info() -> PageInfo {
        PageInfo {
            content_left_pt: 72.0,
            content_w_pt: 450.0,
            content_bottom_pt: 770.0,
            band_pt: BAND,
            capacity_bands: 43,
        }
    }

    /// 構造最小 `Page`：行帶著 (段落序, 段內 char_offset)，座標欄位定值。
    fn page(index: usize, lines: &[(usize, usize, &str)]) -> quire_layout::Page {
        quire_layout::Page {
            index,
            lines: lines
                .iter()
                .map(|(para, off, text)| quire_layout::Line {
                    text: text.to_string(),
                    width_pt: 0.0,
                    para_index: *para,
                    char_offset: *off,
                })
                .collect(),
            line_bands: (0..lines.len() as u32).collect(),
            tables: Vec::new(),
            images: Vec::new(),
            floating_images: Vec::new(),
            width_pt: 595.0,
            height_pt: 842.0,
            margin_left_pt: 72.0,
            margin_top_pt: 72.0,
            section_id: 0,
        }
    }

    // ---------- 零變化守門 ----------

    /// (a) 無註腳 ⇒ 保留帶恆 0：預留表全 0 ⇒ 分頁走既有路徑（構造性零擾動）。
    #[test]
    fn empty_notes_reserve_nothing() {
        let foot = page_foot(Vec::new(), &info());
        assert_eq!(foot.reserve_bands, 0);
        assert_eq!(foot.reserve_pt, 0.0);
        assert!(foot.separator_y.is_none() && foot.items_top_y.is_none());
        assert!(reserves(&[foot, PageFoot::default()])
            .iter()
            .all(|v| *v == 0.0));
    }

    /// (a2) 空條目集經 `layout_notes` ⇒ `is_empty()`（engine 快路徑的判準）。
    #[test]
    fn layout_notes_without_entries_is_empty() {
        let pages = vec![page(0, &[(0, 0, "hello")])];
        let lay = layout_notes(&pages, &[info()], &[], &[], &[], 12.0, &[], 1);
        assert!(lay.is_empty());
        assert!(lay.reserves.iter().all(|v| *v == 0.0));
    }

    // ---------- 保留帶幾何 ----------

    /// (b) 一條單行註腳：保留帶＝sep_bands + 1 帶；分隔線與條目頂恰落帶邊界，
    /// 且末條底部恰不越過版心底（幾何自洽——正文與註腳區不重疊）。
    #[test]
    fn single_note_reserve_is_whole_bands_and_geometry_is_tight() {
        let items = vec![layout_entry(&entry(1, "note"), BAND, 450.0)];
        let foot = page_foot(items, &info());
        let sep_bands = (SEP_GAP_PT / BAND).ceil() as u32; // 8pt → 1 帶
        assert_eq!(sep_bands, 1);
        assert_eq!(foot.items[0].bands, 1);
        assert_eq!(foot.reserve_bands, sep_bands + 1);
        assert_eq!(foot.reserve_pt, foot.reserve_bands as f32 * BAND);
        let sep = foot.separator_y.unwrap();
        assert_eq!(sep, info().content_bottom_pt - foot.reserve_pt);
        assert_eq!(foot.items_top_y.unwrap(), sep + sep_bands as f32 * BAND);
        // 條目視覺底（首行頂＋行高）仍在保留帶內（不溢出到版心底之下）。
        assert!(
            foot.items_top_y.unwrap() + foot.items[0].line_h_pt <= sep + foot.reserve_pt + 0.01
        );
    }

    /// (c) 多條註腳：帶數累加、堆疊序＝推入序（第一條在最上）。
    #[test]
    fn multiple_notes_stack_in_order() {
        let items = vec![
            layout_entry(&entry(1, "first"), BAND, 450.0),
            layout_entry(&entry(2, "second"), BAND, 450.0),
        ];
        let foot = page_foot(items, &info());
        assert_eq!(foot.reserve_bands, 1 + 1 + 1); // sep + 1 + 1
        assert_eq!(foot.items[0].lines[0].runs[0].text, "1.");
        assert_eq!(foot.items[1].lines[0].runs[0].text, "2.");
    }

    // ---------- 條目排布 ----------

    /// (d) 編號前綴＝上標、字級縮小；條目首行有掛排縮排（x ≥ NOTE_INDENT_PT）。
    #[test]
    fn note_prefix_is_superscript_and_indented() {
        let it = layout_entry(&entry(3, "abc"), BAND, 450.0);
        let r0 = &it.lines[0].runs[0];
        assert!(r0.superscript, "編號前綴必須標為上標");
        assert_eq!(r0.text, "3.");
        assert_eq!(r0.size_pt, 10.0 * SUP_SCALE);
        assert_eq!(r0.x_pt, NOTE_INDENT_PT);
        let body = &it.lines[0].runs[1];
        assert!(!body.superscript);
        assert!(body.x_pt > r0.x_pt, "正文段必須排在編號之後");
    }

    /// (e) 長註腳自動換行：行數 > 1、帶數 ceil(高/帶尺) 且 ≥ 行數所需。
    #[test]
    fn long_note_wraps_into_multiple_bands() {
        let long = "x ".repeat(400); // 遠超單行可容
        let it = layout_entry(&entry(1, &long), BAND, 200.0);
        assert!(
            it.lines.len() > 1,
            "長註腳必須換行，實得 {}",
            it.lines.len()
        );
        // 不變量：保留帶必須**覆蓋**堆疊總高（bands×帶尺 ≥ 行數×行高），
        // 否則條目會溢出保留帶、壓進正文。bands ≥ 行數並非必要條件——
        // 註腳行高（14）小於帶尺（16.8）時，ceil 量化本來就會省帶。
        assert!(
            it.bands as f32 * BAND >= it.lines.len() as f32 * it.line_h_pt - 1e-3,
            "帶數必須覆蓋堆疊高度：bands={} 帶尺={BAND} 行數={} 行高={}",
            it.bands,
            it.lines.len(),
            it.line_h_pt
        );
        // 每行不得超出可用寬（縮排已扣）：以行寬近似＝各 run 起點＋文字寬。
        for ln in &it.lines {
            let last = ln.runs.last().expect("行必非空");
            assert!(last.x_pt < 200.0, "行內 x 不得超出版心寬");
        }
    }

    /// (f) 斷行健全性：超長單字（無空白可斷）必須硬切、遞進、不 panic。
    #[test]
    fn unbreakable_long_word_hard_cuts() {
        let word = "超".repeat(100);
        let it = layout_entry(&entry(1, &word), BAND, 60.0);
        assert!(
            it.lines.len() >= 3,
            "必被硬切成多行，實得 {}",
            it.lines.len()
        );
        // 逐行重組必須還原全文（不吞字、不重複）。
        let rebuilt: String = it
            .lines
            .iter()
            .flat_map(|l| l.runs.iter())
            .filter(|r| !r.superscript)
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(rebuilt, word);
    }

    /// (f2) 行首空白不殘留（前導空白被 trim）。
    #[test]
    fn leading_whitespace_is_not_kept() {
        let it = layout_entry(&entry(1, "   Lead"), BAND, 450.0);
        let body = it.lines[0]
            .runs
            .iter()
            .find(|r| !r.superscript)
            .expect("正文段");
        assert_eq!(body.text, "Lead", "前導空白不得進發射文字");
    }

    // ---------- 錨點定位與頁分配 ----------

    /// (g) 同頁同則重複引用 ⇒ 一條註腳、一個記號。
    #[test]
    fn duplicate_ref_same_page_emits_one_item_and_mark() {
        let pages = vec![page(0, &[(0, 0, "ab")])];
        let anchors = vec![anchor(1, 0, 1, 1), anchor(1, 0, 2, 2)];
        let entries = vec![entry(1, "n")];
        let lay = layout_notes(&pages, &[info()], &anchors, &entries, &[0, 0], 12.0, &[], 1);
        assert_eq!(lay.pages[0].items.len(), 1);
        assert_eq!(lay.marks[0].len(), 1);
    }

    /// (h) 跨頁段落的錨點只屬**記號實際落點**那一頁（預留不雙算 ⇒ 收斂不抖動）。
    #[test]
    fn anchor_belongs_only_to_page_where_mark_lands() {
        // 段落 0 跨兩頁：第 0 頁行 [0,3)、第 1 頁行 [3,5)。錨點 cp=4 ⇒ 第 1 頁。
        let pages = vec![page(0, &[(0, 0, "abc")]), page(1, &[(0, 3, "de")])];
        let entries = vec![entry(1, "n")];
        let lay = layout_notes(
            &pages,
            &[info(), info()],
            &[anchor(1, 0, 1, 4)],
            &entries,
            &[0],
            12.0,
            &[],
            1,
        );
        assert!(lay.pages[0].items.is_empty(), "第 0 頁不該有這條註腳");
        assert_eq!(lay.pages[1].items.len(), 1);
        assert_eq!(lay.marks[0].len(), 0);
        assert_eq!(lay.marks[1].len(), 1);
    }

    /// (i) 定位失敗（錨點段落不在任何行程，如表格／文字框內）⇒ 註腳落末頁、
    /// 記號不發射（不靜默遺失本體，也不虛構記號）。
    #[test]
    fn unlocatable_anchor_falls_back_to_last_page_without_mark() {
        let pages = vec![page(0, &[(0, 0, "a")]), page(1, &[(1, 0, "b")])];
        let lay = layout_notes(
            &pages,
            &[info(), info()],
            &[anchor(9, 7, 0, 0)], // 段落 7 不存在
            &[entry(9, "orphan")],
            &[0],
            12.0,
            &[],
            1,
        );
        assert!(lay.pages[0].items.is_empty());
        assert_eq!(lay.pages[1].items.len(), 1, "孤兒註腳必須落末頁");
        assert!(
            lay.marks.iter().all(|m| m.is_empty()),
            "定位失敗不得虛構記號"
        );
    }

    /// (j) 記號 x＝行內前綴寬＋版心左緣；y＝行頂上移上標量。
    #[test]
    fn mark_uses_prefix_width_and_rises_above_baseline() {
        let pages = vec![page(0, &[(0, 0, "abcd")])];
        let lay = layout_notes(
            &pages,
            &[info()],
            &[anchor(1, 0, 1, 2)],
            &[entry(1, "n")],
            &[0],
            12.0,
            &[],
            1,
        );
        let m = &lay.marks[0][0];
        let w = TextMeasurer::new(12.0).text_width("ab");
        assert_eq!(m.x_pt, 72.0 + w);
        assert_eq!(m.y_pt, 72.0 + 0.0 * BAND - BAND * SUP_RISE);
        assert_eq!(m.text, "1");
    }

    /// (k) 引用指向不存在的註（呼叫端過濾）⇒ 該錨點整條不進排布。
    #[test]
    fn anchor_without_entry_is_skipped() {
        let pages = vec![page(0, &[(0, 0, "a")])];
        let lay = layout_notes(
            &pages,
            &[info()],
            &[anchor(1, 0, 1, 1)],
            &[],
            &[0],
            12.0,
            &[],
            1,
        );
        assert!(lay.is_empty());
    }

    // ---------- 收斂判準 ----------

    /// (l) 收斂判準：長度不同補 0、負值／NaN 視同 0（與 layout 層同款口徑）。
    #[test]
    fn reserve_equivalence_normalizes_like_layout_layer() {
        assert!(reserves_stable(&[0.0, 0.0], &[0.0]));
        assert!(reserves_stable(&[0.0, -5.0, f32::NAN], &[]));
        assert!(!reserves_stable(&[1.0], &[0.0]));
        assert!(reserves_stable(&[0.0, 8.4], &[0.0, 8.4, 0.0]));
    }

    /// (m) 錨點掃描只取 footnote 型、cp 標尺正確（空 run 不位移 cp）。
    #[test]
    fn collect_anchors_walks_cp_correctly() {
        let paras: Vec<quire_ooxml::Paragraph> = vec![quire_ooxml::Paragraph {
            style: None,
            runs: vec![
                quire_ooxml::Run {
                    text: "ab".into(),
                    ..Default::default()
                },
                quire_ooxml::Run {
                    text: String::new(),
                    note_ref: Some(quire_ooxml::NoteRef {
                        kind: NoteKind2::Footnote,
                        id: 7,
                    }),
                    ..Default::default()
                },
                quire_ooxml::Run {
                    text: "cd".into(),
                    ..Default::default()
                },
                quire_ooxml::Run {
                    text: String::new(),
                    note_ref: Some(quire_ooxml::NoteRef {
                        kind: NoteKind2::Endnote,
                        id: 8,
                    }),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }];
        let got = collect_anchors(&paras);
        assert_eq!(got.len(), 1, "尾註型不得進正文註腳錨點");
        assert_eq!(got[0].note_id, 7);
        assert_eq!(got[0].cp_before, 2, "空 run 的 cp 標尺＝前文碼點數");
    }

    /// (n) 分隔線寬＝版心 × 1/3（Word 預設）。
    #[test]
    fn separator_span_is_one_third_of_text_block() {
        let (x1, x2) = separator_span(72.0, 450.0);
        assert_eq!(x1, 72.0);
        assert_eq!(x2, 72.0 + 150.0);
    }

    // ---------- F2c：跨頁註腳拆分 ----------

    /// 長註腳在預算不足時拆成多頁續排：首段帶編號前綴、續段不帶；
    /// 全行不遺失；每頁段帶數不超預算。
    #[test]
    fn long_note_splits_across_pages_with_continuation() {
        // 4 頁、每頁容量 8 帶（預算＝8 − 1(min body) − 1(sep) = 6 帶）；
        // 註腳 14pt 行高、16.8 帶尺 ⇒ 每行 1 帶 ⇒ 一條 10 行註腳需拆。
        let e = entry(1, &"word ".repeat(220));
        let full = layout_entry(&e, BAND, 450.0);
        assert!(
            full.lines.len() >= 10,
            "本測前提：註腳至少 10 行，實得 {}",
            full.lines.len()
        );
        let pages: Vec<quire_layout::Page> = (0..4).map(|i| page(i, &[(0, 0, "body")])).collect();
        let infos: Vec<PageInfo> = (0..4)
            .map(|_| PageInfo {
                capacity_bands: 8,
                ..info()
            })
            .collect();
        let lay = layout_notes(
            &pages,
            &infos,
            &[anchor(1, 0, 0, 0)],
            std::slice::from_ref(&e),
            &[0usize],
            12.0,
            &[],
            1,
        );
        // 全行不遺失。
        let total: usize = lay
            .pages
            .iter()
            .flat_map(|p| p.items.iter())
            .map(|i| i.lines.len())
            .sum();
        assert_eq!(total, full.lines.len(), "拆分不得吞行");
        // 首段帶前綴、續段不帶前綴。
        assert_eq!(lay.pages[0].items[0].lines[0].runs[0].text, "1.");
        assert!(!lay.pages[0].items[0].continuation);
        let cont_count = lay
            .pages
            .iter()
            .flat_map(|p| p.items.iter())
            .filter(|i| i.continuation)
            .count();
        assert!(cont_count >= 1, "必須出現續排段");
        for pf in &lay.pages {
            for it in pf.items.iter().filter(|i| i.continuation) {
                assert!(
                    it.lines
                        .iter()
                        .all(|l| l.runs.iter().all(|r| !r.superscript)),
                    "續段不得帶編號前綴"
                );
            }
        }
        // 每頁保留帶 ≤ 容量帶（正文至少 1 帶）。
        for pf in lay.pages.iter().filter(|p| !p.items.is_empty()) {
            assert!(pf.reserve_bands + MIN_BODY_BANDS <= 8, "預留不得吃光正文");
        }
    }

    /// 首頁預算不足一行 ⇒ 整條（含前綴）推到往後頁，不拆出零行首段。
    #[test]
    fn split_waits_for_a_page_that_fits_at_least_one_line() {
        let e = entry(2, &"w ".repeat(300));
        let full = layout_entry(&e, BAND, 450.0);
        let pages: Vec<quire_layout::Page> = (0..3).map(|i| page(i, &[(0, 0, "body")])).collect();
        // 頁0 容量 2（預算 0）；頁1/2 容量 30（預算 28，放得下整條或大半）。
        let infos: Vec<PageInfo> = vec![
            PageInfo {
                capacity_bands: 2,
                ..info()
            },
            PageInfo {
                capacity_bands: 30,
                ..info()
            },
            PageInfo {
                capacity_bands: 30,
                ..info()
            },
        ];
        let lay = layout_notes(
            &pages,
            &infos,
            &[anchor(1, 0, 0, 0)],
            std::slice::from_ref(&e),
            &[0usize],
            12.0,
            &[],
            1,
        );
        assert!(lay.pages[0].items.is_empty(), "零預算頁不放");
        let first = lay.pages.iter().find(|p| !p.items.is_empty()).unwrap();
        assert!(
            !first.items[0].continuation,
            "首段仍應帶編號（整條推後頁，前綴不丟）"
        );
        let total: usize = lay
            .pages
            .iter()
            .flat_map(|p| p.items.iter())
            .map(|i| i.lines.len())
            .sum();
        assert_eq!(total, full.lines.len(), "不得吞行");
    }

    /// 註腳長過所有頁總預算 ⇒ 末頁誠實溢出（不靜默吞字、不 panic）。
    #[test]
    fn split_overflow_lands_on_last_page() {
        let e = entry(3, &"x ".repeat(900));
        let full = layout_entry(&e, BAND, 450.0);
        let pages: Vec<quire_layout::Page> = (0..2).map(|i| page(i, &[(0, 0, "body")])).collect();
        let infos: Vec<PageInfo> = vec![
            PageInfo {
                capacity_bands: 4,
                ..info()
            }, // 預算 2
            PageInfo {
                capacity_bands: 4,
                ..info()
            },
        ];
        let lay = layout_notes(
            &pages,
            &infos,
            &[anchor(1, 0, 0, 0)],
            std::slice::from_ref(&e),
            &[0usize],
            12.0,
            &[],
            1,
        );
        let total: usize = lay
            .pages
            .iter()
            .flat_map(|p| p.items.iter())
            .map(|i| i.lines.len())
            .sum();
        assert_eq!(total, full.lines.len(), "溢出必須留在末頁，不得吞行");
        assert!(lay.pages[1].items.iter().any(|i| i.continuation));
    }

    /// `page_budget_bands`：容量 − 正文下限 − 分隔線帶；負值夾 0。
    #[test]
    fn page_budget_math() {
        assert_eq!(page_budget_bands(43, 1, 1), 41);
        assert_eq!(page_budget_bands(2, 1, 1), 0, "負值夾 0");
        assert_eq!(page_budget_bands(0, 0, 0), 0);
    }

    /// 全續段頁 ⇒ continuation_only（發射端續接線）；混合頁 ⇒ false。
    #[test]
    fn continuation_only_flag_flows_to_page_foot() {
        let mut it = layout_entry(&entry(1, "tail"), BAND, 450.0);
        it.continuation = true;
        it.lines[0].runs.retain(|r| !r.superscript);
        let foot = page_foot(vec![it], &info());
        assert!(foot.continuation_only);
        let mixed = page_foot(
            vec![layout_entry(&entry(2, "head"), BAND, 450.0), {
                let mut t = layout_entry(&entry(1, "tail"), BAND, 450.0);
                t.continuation = true;
                t
            }],
            &info(),
        );
        assert!(!mixed.continuation_only);
        let empty = page_foot(Vec::new(), &info());
        assert!(!empty.continuation_only, "空頁預設 false（零變化）");
    }

    /// 零變化守門（F2c 版）：容量充足（capacity_bands 大）時單條註腳的
    /// 帶數／預留與 F2b-2 完全同值、無拆分標記。
    #[test]
    fn single_page_note_is_untouched_by_splitting() {
        let pages = vec![page(0, &[(0, 0, "hello")])];
        let lay = layout_notes(
            &pages,
            &[info()],
            &[anchor(1, 0, 1, 1)],
            &[entry(1, "short")],
            &[0],
            12.0,
            &[],
            1,
        );
        let pf = &lay.pages[0];
        assert_eq!(pf.items.len(), 1);
        assert!(!pf.continuation_only);
        assert!(!pf.items[0].continuation);
        assert_eq!(pf.reserve_bands, 2, "1 分隔帶 + 1 條目帶（F2b-2 原值）");
    }

    /// 容量未知（capacity_bands=0，舊呼叫口徑）⇒ 保守不拆分（零退化）。
    #[test]
    fn unknown_capacity_never_splits() {
        let e = entry(1, &"word ".repeat(400));
        let pages: Vec<quire_layout::Page> = (0..2).map(|i| page(i, &[(0, 0, "body")])).collect();
        let infos: Vec<PageInfo> = (0..2)
            .map(|_| PageInfo {
                capacity_bands: 0,
                ..info()
            })
            .collect();
        let lay = layout_notes(
            &pages,
            &infos,
            &[anchor(1, 0, 0, 0)],
            std::slice::from_ref(&e),
            &[0usize],
            12.0,
            &[],
            1,
        );
        assert_eq!(
            lay.pages[0].items.len(),
            1,
            "容量未知 ⇒ 首頁整條照放（舊行為）"
        );
        assert!(lay.pages[1].items.is_empty());
        assert!(!lay.pages[0].items[0].continuation);
    }

    // ---------- 尾註（F3）----------

    /// 測試用條目（只帶帶數；行內容與本組測試無關）。
    fn item(bands: u32) -> NoteItem {
        NoteItem {
            lines: vec![NoteLine { runs: Vec::new() }],
            bands,
            line_h_pt: 14.0,
            continuation: false,
        }
    }

    /// (f1) `flow_items_to_pages` 空輸入 ⇒ 空頁面清單（零擾動快路徑的判準）。
    #[test]
    fn flow_items_to_pages_empty_input_yields_no_pages() {
        assert!(flow_items_to_pages(&[], 10, 10).is_empty());
    }

    /// (f2) 單條帶數超過整頁容量 ⇒ 該條**獨佔一頁**（不切條、不丟條、不 panic）。
    #[test]
    fn flow_items_to_pages_oversized_item_owns_its_page() {
        let items = vec![item(1), item(99), item(1)];
        let pages = flow_items_to_pages(&items, 2, 2);
        assert_eq!(pages.len(), 3, "三條 ⇒ 三頁（超大條獨佔一頁）");
        assert_eq!(pages[1], vec![1], "超大條必須獨佔一頁");
        assert_eq!(
            pages.iter().flatten().count(),
            3,
            "條目索引不得漏（逐條都得有落點）"
        );
    }

    /// (f3) 逐頁裝箱：首頁容量與後續頁容量分別生效；索引按序、不漏不重。
    #[test]
    fn flow_items_to_pages_packs_by_capacity_in_order() {
        let items: Vec<NoteItem> = (0..5).map(|_| item(1)).collect();
        let pages = flow_items_to_pages(&items, 2, 3);
        assert_eq!(pages, vec![vec![0, 1], vec![2, 3, 4]]);
    }

    /// (f4) 容量 ≤ 0 ⇒ 視同 1（不空迴圈、不 panic、不吞條目）。
    #[test]
    fn flow_items_to_pages_zero_capacity_degrades_to_one() {
        let items: Vec<NoteItem> = (0..3).map(|_| item(1)).collect();
        let pages = flow_items_to_pages(&items, 0, 0);
        assert_eq!(pages, vec![vec![0], vec![1], vec![2]]);
    }

    /// (f5) 尾註錨點掃描：只取 `w:endnoteReference`；cp 標尺與註腳支路同款
    /// （空 run 不位移 cp）。兩條支路互不撈對方的錨點。
    #[test]
    fn collect_endnote_anchors_takes_only_endnote_refs() {
        let paras: Vec<quire_ooxml::Paragraph> = vec![quire_ooxml::Paragraph {
            style: None,
            runs: vec![
                quire_ooxml::Run {
                    text: "ab".into(),
                    ..Default::default()
                },
                quire_ooxml::Run {
                    text: String::new(),
                    note_ref: Some(quire_ooxml::NoteRef {
                        kind: NoteKind2::Footnote,
                        id: 7,
                    }),
                    ..Default::default()
                },
                quire_ooxml::Run {
                    text: "cd".into(),
                    ..Default::default()
                },
                quire_ooxml::Run {
                    text: String::new(),
                    note_ref: Some(quire_ooxml::NoteRef {
                        kind: NoteKind2::Endnote,
                        id: 8,
                    }),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }];
        let got = collect_endnote_anchors(&paras);
        assert_eq!(got.len(), 1, "尾註支路只取尾註型");
        assert_eq!(got[0].note_id, 8);
        assert_eq!(got[0].cp_before, 4, "cp 標尺＝前文碼點數（空 run 不位移）");
        assert!(
            collect_anchors(&paras).iter().all(|a| a.note_id != 8),
            "註腳支路不得撈到尾註錨點"
        );
    }

    /// (f6) 尾註首頁分隔線佔帶：1 帶畫線 ＋ SEP_GAP 量化帶（條目區自此帶起）。
    #[test]
    fn endnote_sep_bands_covers_rule_band_and_gap() {
        // SEP_GAP_PT=8、帶尺 16.8 ⇒ 8pt 量化成 1 帶；再加畫線的 1 帶。
        assert_eq!(endnote_sep_bands(BAND), 2);
        // 帶尺極小時量化帶變多（離散值域，仍是整帶）。
        assert!(endnote_sep_bands(4.0) > endnote_sep_bands(BAND));
    }

    /// (f7) 空尾註層 ⇒ `is_empty()`（發射層零擾動快路徑的判準）。
    #[test]
    fn empty_end_layout_is_empty() {
        assert!(EndLayout::default().is_empty());
        let mut lay = EndLayout {
            pages: vec![Vec::new()],
            marks: vec![Vec::new()],
            separator_top: false,
        };
        assert!(lay.is_empty());
        lay.separator_top = true;
        assert!(lay.is_empty(), "只有分隔線旗標不算有內容");
        lay.pages.push(vec![item(1)]);
        assert!(!lay.is_empty(), "有條目 ⇒ 不是空層");
    }
}
