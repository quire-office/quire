//! quire-layout —— 斷行 + 分頁。
//!
//! Phase0-5 起斷行走 **UAX#14 斷字規則（`uax14`）＋ Knuth-Plass 最佳化
//! （`kp`）**：寬度取 P0-4 真字型 shaping（`shaping` 模組：swash + 內建
//! Noto Sans TC 子集，OFL），斷字機會取 `unicode-linebreak`（UAX#14 參考
//! 實作），斷行方案由段落級 DP 選全局最小代價——閉標點不落行首、CJK 任意
//! 字間斷、Latin 詞內不斷、行鬆緊度全局最優。
//!
//! 演進目標：
//! 1. ~~Knuth-Plass 最佳化斷行 + UAX#14~~ ✅ Phase0-5
//! 2. ~~增量重排：編輯只重排受影響頁（髒區間）~~ ✅ Phase0-6
//! 3. ~~預算制背景重排（打字期走軟分頁，空檔再對齊）~~ ✅ Phase0-6
//!    （`LayoutEngine::set_sync_budget` ＋ `reflow_step`；排程策略屬 shells）
//!
//! 本 crate 禁止任何平台依賴（wasm32-unknown-unknown 必須可編譯）。
//! 字型**只准內嵌子集**，禁止讀系統字型——跨平台分頁一致性的前提
//! （研究報告 §風險2：字型子集服務統一 metrics）。

pub mod kp;
pub mod shaping;
pub mod table;
pub mod uax14;

pub use shaping::{
    shape_advances, ClusterAdvance, FontMetrics, TextMeasurer, EMBEDDED_FONT, EMBEDDED_FONT_FAMILY,
};
pub use uax14::{break_atoms, Atom};

use serde::{Deserialize, Serialize};

/// mandatory 斷行控制字元（UAX#14 BK/CR/LF/NL/MANDATORY）：斷行標記，
/// 不顯示、不計寬。
pub(crate) const EOL_CONTROLS: [char; 6] =
    ['\n', '\r', '\u{000c}', '\u{0085}', '\u{2028}', '\u{2029}'];

/// 版面參數（後備值＋字級/行距）。
///
/// **頁面幾何（尺寸/邊界）的權威來源是檔案自己的 sectPr**（Phase0-2b）：
/// engine 會逐 section 解析 `pgSz`／`pgMar` 覆寫此處的頁面欄位。本 struct 的
/// `Default`（A4、1 吋邊界）**只在檔案完全沒有任何 sectPr 時**生效——是後備，
/// 不是硬編碼斷言。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LayoutParams {
    pub page_w_pt: f32,
    pub page_h_pt: f32,
    /// 四邊邊界 [上, 右, 下, 左]（pt）。sectPr 的 pgMar 各邊可不同，單一 margin_pt
    /// 無法如實表達（Letter 文件 Word 預設左右 1.25 吋 ≠ 上下 1 吋）。
    pub margins_pt: [f32; 4],
    pub font_size_pt: f32,
    pub line_spacing: f32,
}

/// 頁面幾何（來自 section；pt）。型別定義在 layout 層——版面是唯一消費者，
/// ooxml 層只負責把 sectPr 讀成同構資料，engine 層對應（分層：layout 不依賴 ooxml）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionGeometry {
    pub page_w_pt: f32,
    pub page_h_pt: f32,
    pub margin_top_pt: f32,
    pub margin_bottom_pt: f32,
    pub margin_left_pt: f32,
    pub margin_right_pt: f32,
    /// `w:pgSz/@w:orient="landscape"`（僅供老實回報；尺寸以 w/h 為準）。
    pub landscape: bool,
    /// `w:type/@w:val="continuous"`：同頁續排，section 邊界不強制換頁。
    pub continuous: bool,
}

/// 逐段落清單的一項：段落所屬 section 的識別與幾何（engine 層由 sectPr 餵入）。
/// `id` 為文件內 section 序號——同尺寸的相鄰 section 仍必須在邊界換頁，
/// 靠 id 辨識而非尺寸差異。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionSpec {
    pub id: u32,
    pub geo: SectionGeometry,
}

pub const MARGIN_TOP: usize = 0;
pub const MARGIN_RIGHT: usize = 1;
pub const MARGIN_BOTTOM: usize = 2;
pub const MARGIN_LEFT: usize = 3;

impl LayoutParams {
    /// 由 section 幾何構建（字級/行距維持傳入值）。頁面欄位以檔案為準，
    /// 不走 `Default`。
    pub fn from_section(geo: &SectionGeometry, font_size_pt: f32, line_spacing: f32) -> Self {
        Self {
            page_w_pt: geo.page_w_pt,
            page_h_pt: geo.page_h_pt,
            margins_pt: [
                geo.margin_top_pt,
                geo.margin_right_pt,
                geo.margin_bottom_pt,
                geo.margin_left_pt,
            ],
            font_size_pt,
            line_spacing,
        }
    }

    pub fn margin(&self, side: usize) -> f32 {
        self.margins_pt[side]
    }

    /// 版心寬（pt）：頁寬減左右邊界。
    pub fn content_w_pt(&self) -> f32 {
        (self.page_w_pt - self.margin(MARGIN_LEFT) - self.margin(MARGIN_RIGHT)).max(1.0)
    }

    /// 行網格帶高（pt）＝字級×行距：段落行與表格行帶共用的量化單位
    /// （Phase3-1b——分頁容量與表格高度都以「帶」計）。
    pub fn line_h_pt(&self) -> f32 {
        self.font_size_pt * self.line_spacing
    }

    /// 以本參數（＝文件首 section／後備值）計算每頁行數容量。
    pub fn per_page_lines(&self) -> usize {
        let line_h = self.line_h_pt();
        let content_h =
            (self.page_h_pt - self.margin(MARGIN_TOP) - self.margin(MARGIN_BOTTOM)).max(line_h);
        ((content_h / line_h).floor() as usize).max(1)
    }
}

impl Default for LayoutParams {
    /// 無 sectPr 資訊時的後備：A4、1 吋邊界、12pt 字級、1.4 行距。
    /// （真實檔案的頁面幾何一律由 sectPr 解析覆寫，見 `from_section`。）
    fn default() -> Self {
        Self {
            page_w_pt: 595.0,
            page_h_pt: 842.0,
            margins_pt: [72.0, 72.0, 72.0, 72.0],
            font_size_pt: 12.0,
            line_spacing: 1.4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Line {
    pub text: String,
    pub width_pt: f32,
    /// 本行所屬段落索引（Web-1 caret 命中測試用：顯示列表只帶行不帶段，
    /// 游標要還原 (段落, 字元偏移) 就必須讓行帶著段落歸屬走遍分頁）。
    /// golden 快照只比對 text/width_pt，故此欄新增不動 golden。
    pub para_index: usize,
    /// 本行起點在所屬段落內的字元（碼點）偏移（Card A：供 engine-api 將行與
    /// 段落 run 邊界精確對位求交集；serde default 保持舊快照相容）。
    #[serde(default)]
    pub char_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParagraphLayout {
    pub para_index: usize,
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Page {
    pub index: usize,
    pub lines: Vec<Line>,
    /// 每行的頁內行網格帶索引（無表格頁恆 0..＝舊行為；表格後的文字行帶
    /// 索引不再等於陣標——y＝margin_top＋band×行高）。serde default 兼容。
    #[serde(default)]
    pub line_bands: Vec<u32>,
    /// 排在此頁的表格切片（pack_run flush 時蓋章；無表格頁＝空）。
    #[serde(default)]
    pub tables: Vec<TablePlacement>,
    /// 此頁起頭所屬 section 的實際頁面幾何（老實回報，不假設全文件同一張紙）。
    pub width_pt: f32,
    pub height_pt: f32,
    pub margin_left_pt: f32,
    pub margin_top_pt: f32,
    /// 此頁起頭所屬 section 的文件序號（pagebreaks 靠它辨識 section 邊界）。
    pub section_id: u32,
}

/// 斷行：UAX#14 斷字機會上的 Knuth-Plass 最佳化（見 `kp` 模組）。
/// 寬度一律取 shaped advances（見 `shaping` 模組）。
pub fn break_lines(text: &str, max_w: f32, m: &TextMeasurer) -> Vec<Line> {
    kp::kp_break_lines(text, max_w, m)
        .into_iter()
        .map(|s| s.line)
        .collect()
}

pub fn layout_paragraphs(texts: &[String], p: &LayoutParams) -> Vec<ParagraphLayout> {
    layout_paragraphs_per_section(texts, p, &[])
}

/// 逐段落斷行；每段按其**所屬 section**（與 `texts` 等長，engine 層餵入）的
/// 版心寬斷行。`sections` 為空＝全文件單一幾何（退回 `p` 的頁面欄位）。
pub fn layout_paragraphs_per_section(
    texts: &[String],
    p: &LayoutParams,
    sections: &[SectionSpec],
) -> Vec<ParagraphLayout> {
    let m = TextMeasurer::new(p.font_size_pt);
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let content_w = section_params(p, sections, i).content_w_pt();
            let mut pl = ParagraphLayout {
                para_index: i,
                lines: break_lines(t, content_w, &m),
            };
            // break_lines 不認識段落索引；在此蓋章，讓分頁搬運行時段落歸屬不丟。
            for l in pl.lines.iter_mut() {
                l.para_index = i;
            }
            pl
        })
        .collect()
}

/// 段落 i 所屬 section 的版面參數（字級/行距維持 `p`，頁面幾何來自 section）。
fn section_params(p: &LayoutParams, sections: &[SectionSpec], para: usize) -> LayoutParams {
    match sections.get(para) {
        Some(s) => LayoutParams::from_section(&s.geo, p.font_size_pt, p.line_spacing),
        None => *p,
    }
}

/// 錨點段落 para 的有效 section（無 section 清單時由 `p` 合成——單幾何文件，id=0）。
/// sections 仍是**逐段落**表（sectPr 的權威投影）；流項以 `item_para` 錨到段落。
fn effective_section(p: &LayoutParams, sections: &[SectionSpec], para: usize) -> SectionSpec {
    match sections.get(para) {
        Some(s) => *s,
        None => SectionSpec {
            id: 0,
            geo: SectionGeometry {
                page_w_pt: p.page_w_pt,
                page_h_pt: p.page_h_pt,
                margin_top_pt: p.margin(MARGIN_TOP),
                margin_bottom_pt: p.margin(MARGIN_BOTTOM),
                margin_left_pt: p.margin(MARGIN_LEFT),
                margin_right_pt: p.margin(MARGIN_RIGHT),
                landscape: false,
                continuous: false,
            },
        },
    }
}

/// 排版游標／頁起頭位置：(流項, 流項內列, 列內帶)。詞典序嚴格單調——
/// 增量收斂的對齊鍵。文字項的「列」即斷行行號、帶恆 0；表格項的列＝表格
/// 列序、帶＝列內行網格偏移。**無表格文件的 FlowPos 與舊 (段落, 行) 模型
/// 一一對應**（段落 i ↔ 流項 i），golden／增量語義逐位元組不變。
pub type FlowPos = (usize, usize, u32);

/// 排版器眼中的可流項（Phase3-1b）：段落行流或已排版表格的引用。
/// 表格內部段落（`TableSlot.para_start..para_end`）不出現在流裡——其文字
/// 由表格切片繪製，雙重流出會雙繪。
#[derive(Debug, Clone, PartialEq)]
pub enum FlowItem {
    Text { para: usize },
    Table { slot: usize },
}

/// 版面排定的表格來源（engine 由 canonical 映射餵入；slot＝陣列索引）。
#[derive(Debug, Clone)]
pub struct TableSlot {
    /// 表格排在第 `para_start` 段之前；表格（含巢狀表）內部段落佔用
    /// [para_start, para_end)，流失權。
    pub para_start: usize,
    pub para_end: usize,
    pub layout: crate::table::TableLayout,
}

/// 頁面上的表格放置（pack_run flush 時蓋章）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TablePlacement {
    pub slot: usize,
    /// 錨點段落（＝表格 para_start）：純表格頁的分頁歸屬與 section 判定用。
    pub anchor_para: usize,
    /// 切片首帶在頁內行網格中的索引。
    pub band: u32,
    /// 切片（frag.y_pt 已蓋章＝margin_top＋band×行高，頁內絕對）。
    pub frag: crate::table::TableFrag,
}

/// 分頁掃描游標狀態：下一條待排放的 (流項, 列, 帶)、正在累積的頁、
/// 上一項的 section、當前頁第一項的位置。Page 邊界處（flush 時）這是完整
/// 的「可中斷再續」快照——增量重排（Phase0-6）就靠它從受影響頁重新掃描，
/// 而不是從頭重排整份文件。
#[derive(Debug, Clone)]
struct PackState {
    cursor: FlowPos,
    cur: Vec<Line>,
    /// `cur[i]` 的行網格帶索引（表格插進同一網格後不再等於陣標；
    /// 無表格頁恆 0..n，與舊行為全等）。
    cur_bands: Vec<u32>,
    cur_tables: Vec<TablePlacement>,
    /// 當前頁已佔行網格帶數（文字行＋表格帶）。
    cur_used: u32,
    cur_spec: Option<SectionSpec>,
    page_start: FlowPos,
}

impl PackState {
    fn fresh() -> Self {
        Self {
            cursor: (0, 0, 0),
            cur: Vec::new(),
            cur_bands: Vec::new(),
            cur_tables: Vec::new(),
            cur_used: 0,
            cur_spec: None,
            page_start: (0, 0, 0),
        }
    }
}

/// 可重用的舊版面尾段（增量收斂快速路）。合法性論證見 `PackOutcome::Converged`。
struct ReuseTail {
    /// 舊版面頁起頭位置（逐頁，嚴格單調）。
    starts: Vec<FlowPos>,
    /// 尚未反映進版面的流項（本次編輯對象所在項）：收斂點流項必須 **大於**
    /// 它，否則尾段可能還含該項的舊內容，重用會吞掉編輯（不正確）。
    dirty: usize,
    /// 下一個候選收斂點的下標；`starts[head] < 游標` 的永遠錯失，往後推。
    head: usize,
}

impl ReuseTail {
    /// 掃描越過的舊邊界永不命中，推進 head（邊界嚴格單調 ⇒ 丟掉是安全的）。
    fn advance(&mut self, page_start: FlowPos) {
        while self.head < self.starts.len() && self.starts[self.head] < page_start {
            self.head += 1;
        }
    }

    /// 當前頁邊界是否可收斂：與舊版面某頁起頭**同項同列同帶**，且起頭項
    /// 已在髒項之後。此刻兩條掃描的狀態完全相同——游標同位、cur 剛清空、
    /// cur_spec 同（由起頭項 index 經 item_sections 決定，index 永不移位），
    /// 而其後的流逐項相同（髒項之後的內容未動）。掃描是決定性的 ⇒ 自此
    /// 必然重播舊頁序列，舊尾段可整段重用。
    fn can_converge(&self, page_start: FlowPos) -> bool {
        self.starts.get(self.head) == Some(&page_start) && page_start.0 > self.dirty
    }
}

enum PackOutcome {
    /// 掃描到文末：產物就是完整版面。
    Exhausted,
    /// 頁邊界狀態與舊版面某頁起頭完全對齊 ⇒ 舊版面自此往後就是本次掃描
    /// 必產出的結果。論證：掃描是「行流＋(游標, cur, cur_spec)」的決定函數，
    /// cur_spec 只由段落 index 經 sections 決定（index 永不移位、sections
    /// 生命期內不變），而收斂點之後的段落全部未髒——同一狀態必然重播同一
    /// 組頁。直接把舊尾段接上（重新編號 index），增量重排就此收斂。
    Converged,
    /// 預算用盡，停在頁邊界：`PackState` 保存游標，`reflow_step` 從這裡續排。
    BudgetStopped,
}

/// `pack_run` 的掃描策略：舊版面對齊（收斂）、同步預算、編輯項覆蓋底線。
/// 全速重建（`pack_full`）用 `default_run()`；增量路徑按需餵。
struct PackPolicy<'a> {
    reuse: Option<&'a mut ReuseTail>,
    budget: Option<usize>,
    /// 已定案區必須覆蓋到此流項（打字期編輯項永不落在未定案尾段）。
    cover_item: usize,
}

impl PackPolicy<'_> {
    fn default_run() -> Self {
        Self {
            reuse: None,
            budget: None,
            cover_item: usize::MAX,
        }
    }
}

/// 流項 fi 的 section 錨點段落：文字項＝該段；表格項＝其 para_start
/// （表格排在此段之前，沿用該段所處 section 的頁面幾何）。
fn item_para(flow: &[FlowItem], tables: &[TableSlot], fi: usize) -> usize {
    match &flow[fi] {
        FlowItem::Text { para } => *para,
        FlowItem::Table { slot } => tables[*slot].para_start,
    }
}

/// 流排放主幹（`paginate_per_section` 與增量重排共用同一台狀態機——分頁
/// 語義只有一份實作，golden 與增量結果不可能分岔）。
///
/// 每步對下一個流項單位算 `start_new`（section 邊界非 continuous，或行網格
/// 容量已滿），成立則 flush 當前頁。文字項一次放一條行（1 帶）；表格項放
/// 「至多剩餘容量」的切片（`TableLayout::fragment`，切片可跨頁續排），並就地把
/// 切片 y 蓋章成頁內絕對座標（行網格帶 × 行高＋上邊界）。無表格文件的
/// FlowPos 序列與舊 (段落, 行) 模型一一對應——golden 逐位元組不變。
/// `policy.reuse` 給定時：每次 flush 後比對「新頁起頭 == 舊版面某頁起頭且
/// 起頭項 > 髒項」，命中即收斂（`PackOutcome::Converged`）。
/// `policy.budget` 限制本次最多**新增**幾頁，且至少覆蓋完
/// `policy.cover_item`（打字期編輯項永不落在未定案區）；用盡就在頁邊界
/// 停住（軟分頁：尾段留待 `reflow_step` 空檔續排）。
#[allow(clippy::too_many_arguments)]
fn pack_run(
    flow: &[FlowItem],
    pls: &[&ParagraphLayout],
    tables: &[TableSlot],
    p: &LayoutParams,
    sections: &[SectionSpec],
    st: &mut PackState,
    out: &mut Vec<Page>,
    out_starts: &mut Vec<FlowPos>,
    mut policy: PackPolicy<'_>,
) -> PackOutcome {
    // 預算至少要能容納錨點頁本身（st 自錨點頁起算；base=已保留頁數）——
    // 否則連編輯項所在頁都排不出來，打字期「編輯項永不在未定案區」破功。
    let budget = policy.budget.map(|b| b.max(1));
    let cover_item = policy.cover_item;
    let base = out.len();
    let fresh_page = |out: &mut Vec<Page>, out_starts: &mut Vec<FlowPos>, st: &mut PackState| {
        if st.cur.is_empty() && st.cur_tables.is_empty() {
            return;
        }
        let s = st.cur_spec.unwrap();
        out.push(Page {
            index: out.len(),
            lines: std::mem::take(&mut st.cur),
            line_bands: std::mem::take(&mut st.cur_bands),
            tables: std::mem::take(&mut st.cur_tables),
            width_pt: s.geo.page_w_pt,
            height_pt: s.geo.page_h_pt,
            margin_left_pt: s.geo.margin_left_pt,
            margin_top_pt: s.geo.margin_top_pt,
            section_id: s.id,
        });
        out_starts.push(st.page_start);
        st.page_start = st.cursor;
        st.cur_used = 0;
    };

    while st.cursor.0 < flow.len() {
        let (fi, li, band) = st.cursor;
        let anchor = item_para(flow, tables, fi);
        let spec = effective_section(p, sections, anchor);
        let cap =
            LayoutParams::from_section(&spec.geo, p.font_size_pt, p.line_spacing).per_page_lines();
        let start_new = match st.cur_spec {
            // section 邊界（非 continuous）一律換頁；同尺寸也換。
            Some(prev) if prev.id != spec.id => !spec.geo.continuous,
            Some(_) => st.cur_used >= cap as u32, // 行網格容量已滿
            None => false,
        };
        if start_new {
            fresh_page(out, out_starts, st);
            // 收斂檢查在 flush 後：新頁起頭（== 游標）對齊舊版面 ⇒ 往後可重用。
            if let Some(t) = policy.reuse.as_mut() {
                t.advance(st.page_start);
                if t.can_converge(st.page_start) {
                    return PackOutcome::Converged;
                }
            }
            // 預算：只約束「新增頁」，且編輯項尚未消化完時絕不停車。
            if out.len() - base >= budget.unwrap_or(usize::MAX) && st.cursor.0 > cover_item {
                return PackOutcome::BudgetStopped;
            }
        }
        st.cur_spec = Some(spec);
        let line_h = p.line_h_pt();
        match &flow[fi] {
            FlowItem::Text { para } => {
                st.cur.push(pls[*para].lines[li].clone());
                st.cur_bands.push(st.cur_used);
                st.cur_used += 1;
                st.cursor = if li + 1 < pls[*para].lines.len() {
                    (fi, li + 1, 0)
                } else {
                    (fi + 1, 0, 0)
                };
            }
            FlowItem::Table { slot } => {
                let tl = &tables[*slot].layout;
                if tl.is_empty() || li >= tl.row_count() {
                    st.cursor = (fi + 1, 0, 0);
                    continue;
                }
                let remain = cap.saturating_sub(st.cur_used as usize) as u32;
                let page_left = spec.geo.margin_left_pt;
                let (mut frag, next, used) = tl.fragment(li, band, remain, false, page_left);
                if used == 0 {
                    st.cursor = (fi + 1, 0, 0);
                    continue; // 防呆：空切片直接過項（正常不可達）
                }
                // 切片 y 蓋章：頁內絕對＝上邊界＋已佔帶數×行高。列內偏移由
                // 繪製端累加 RowFrag.height_pt（切片內列序決定，不另存欄位）。
                frag.y_pt = spec.geo.margin_top_pt + st.cur_used as f32 * line_h;
                st.cur_tables.push(TablePlacement {
                    slot: *slot,
                    anchor_para: tables[*slot].para_start,
                    band: st.cur_used,
                    frag,
                });
                st.cur_used += used;
                st.cursor = if next.0 >= tl.row_count() {
                    (fi + 1, 0, 0)
                } else {
                    (fi, next.0, next.1)
                };
            }
        }
    }
    fresh_page(out, out_starts, st);
    PackOutcome::Exhausted
}

pub fn paginate(pls: &[ParagraphLayout], p: &LayoutParams) -> Vec<Page> {
    paginate_per_section(pls, p, &[])
}

/// 逐 section 分頁（ECMA-376 §17.2.3）。
///
/// 每頁容量取**該頁所屬 section** 的版心高；section 邊界（`w:type` 非
/// continuous）必開新頁——同尺寸的相鄰 section 靠 `id` 辨識，不是靠尺寸差異；
/// continuous 邊界同頁續排。每頁帶著自己起頭 section 的實際尺寸
/// （render_svg / pagebreaks 據此老實回報）。無表格的舊呼叫端行為全等。
pub fn paginate_per_section(
    pls: &[ParagraphLayout],
    p: &LayoutParams,
    sections: &[SectionSpec],
) -> Vec<Page> {
    paginate_tables(pls, p, sections, &[]).0
}

/// 含表格的公開版（Phase3-1b）：段落流＋表格槽一起排。
pub fn paginate_tables(
    pls: &[ParagraphLayout],
    p: &LayoutParams,
    sections: &[SectionSpec],
    tables: &[TableSlot],
) -> (Vec<Page>, Vec<FlowPos>) {
    let refs: Vec<&ParagraphLayout> = pls.iter().collect();
    let flow = build_flow(pls.len(), tables);
    paginate_flow(&flow, &refs, tables, p, sections)
}

/// 段落＋表格混排流（Phase3-1b）：依 para_start 把表格項插進段落序列，
/// 表格內部段落（[para_start, para_end)）不進流（文字由表格切片繪製，
/// 雙重流出會雙繪）。表格項排在 para_start 那一**段之前**。
pub fn build_flow(n_paras: usize, tables: &[TableSlot]) -> Vec<FlowItem> {
    let mut flow = Vec::with_capacity(n_paras + tables.len());
    let mut ti = 0usize;
    let mut skip_until = 0usize; // 表格內部段落上界（不含）：流失權
    for para in 0..n_paras {
        // para_start <= para 的表格都排在當前段之前（依序；同插入點保序）。
        while ti < tables.len() && tables[ti].para_start <= para {
            flow.push(FlowItem::Table { slot: ti });
            skip_until = skip_until.max(tables[ti].para_end);
            ti += 1;
        }
        if para < skip_until {
            continue; // 表格內部段落：文字由表格切片繪製
        }
        flow.push(FlowItem::Text { para });
    }
    // para_start 超出末段（文件尾部表格）：續掛文末。
    while ti < tables.len() {
        flow.push(FlowItem::Table { slot: ti });
        ti += 1;
    }
    // sanity：表格區間不得重疊、不得越界（engine 映射層的契約）。
    debug_assert!(tables.iter().enumerate().all(|(i, t)| {
        (i == 0 || tables[i - 1].para_end <= t.para_start) && t.para_end <= n_paras
    }));
    flow
}

/// 純函數分頁核心：流→頁。回傳（頁序列, 逐頁起頭 FlowPos）。
fn paginate_flow(
    flow: &[FlowItem],
    pls: &[&ParagraphLayout],
    tables: &[TableSlot],
    p: &LayoutParams,
    sections: &[SectionSpec],
) -> (Vec<Page>, Vec<FlowPos>) {
    let mut pages: Vec<Page> = Vec::new();
    let mut starts: Vec<FlowPos> = Vec::new();
    // 全速掃描：無重用、無預算、不需覆蓋任何特定流項。
    pack_run(
        flow,
        pls,
        tables,
        p,
        sections,
        &mut PackState::fresh(),
        &mut pages,
        &mut starts,
        PackPolicy::default_run(),
    );

    if pages.is_empty() {
        let geo = effective_section(p, sections, 0);
        pages.push(Page {
            index: 0,
            lines: vec![Line {
                text: String::new(),
                width_pt: 0.0,
                para_index: 0,
                char_offset: 0,
            }],
            line_bands: vec![0],
            tables: Vec::new(),
            width_pt: geo.geo.page_w_pt,
            height_pt: geo.geo.page_h_pt,
            margin_left_pt: geo.geo.margin_left_pt,
            margin_top_pt: geo.geo.margin_top_pt,
            section_id: geo.id,
        });
        starts.push((0, 0, 0)); // 合成空頁＝流首（starts/pages 同步不變量）
    }
    (pages, starts)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Invalidation {
    Paragraph(usize),
    Page(usize),
    /// 增量重排尚有尾段未定案（預算制軟分頁）：此頁起的版面是**暫缺**狀態，
    /// 待 `LayoutEngine::reflow_step` 於空檔排完才會補齊。shell 收到此事件
    /// 應在闲置時呼叫 `reflow_step`，並依其回傳的 `Page(_)` 事件重繪。
    /// （P0-6 起 `Page(_)` 只覆蓋髒頁區間——舊骨架的 AllPages 已無生產者，
    /// 全版面重繪用「對每頁各發一個 Page」表達。）
    ReflowPending {
        from_page: usize,
    },
}

/// 分頁斷點：每頁由哪些段落組成、是否從段落中間續排、此頁實際尺寸。
/// render-lab 用它與 LibreOffice 基線的分頁位置做比對。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageBreak {
    pub page: usize,
    /// 此頁第一行所屬段落 index。
    pub first_para: usize,
    /// 此頁最後一行所屬段落 index（含）。
    pub last_para: usize,
    /// 此頁行數。
    pub line_count: usize,
    /// 此頁第一行是否為上一頁段落的續排（跨頁斷點）。
    pub continues_paragraph: bool,
    /// 此頁所屬 section 的實際尺寸（pt）——render-lab 據此與 LO 逐頁對照。
    pub width_pt: f32,
    pub height_pt: f32,
    /// 此頁是否由 section 邊界起頭（nextPage 換頁，非容量已滿）。
    pub starts_section: bool,
}

/// 依段落重排後計算每頁的段落分佈（不依賴 LayoutEngine 狀態，純函數）。
pub fn page_breaks(texts: &[String], p: &LayoutParams) -> Vec<PageBreak> {
    page_breaks_per_section(texts, p, &[])
}

/// 逐 section 版：每頁尺寸取自身 section 實際幾何（老實回報，不假設全文件 A4）。
pub fn page_breaks_per_section(
    texts: &[String],
    p: &LayoutParams,
    sections: &[SectionSpec],
) -> Vec<PageBreak> {
    let pls = layout_paragraphs_per_section(texts, p, sections);
    let pages = paginate_per_section(&pls, p, sections);
    breaks_from_pages(&pages, p)
}

/// 從已排好的頁序列導出分頁斷點（Phase3-1b：engine 直接拿 LayoutEngine
/// 的實際頁——含表格佔帶——回報，不再假裝只有文字流）。
pub fn breaks_from_pages(pages: &[Page], p: &LayoutParams) -> Vec<PageBreak> {
    let mut breaks = Vec::with_capacity(pages.len());
    for (i, pg) in pages.iter().enumerate() {
        // 頁首/頁尾段落歸屬：文字行優先；純表格頁退回切片錨點段落
        // （首placement 錨點＝頁首、末 placement 錨點＝頁尾的近似口徑）。
        let para_of = |pg: &Page, first: bool| -> usize {
            if let Some(l) = if first {
                pg.lines.first()
            } else {
                pg.lines.last()
            } {
                return l.para_index;
            }
            match (pg.tables.first(), pg.tables.last()) {
                (Some(t), _) if first => t.anchor_para,
                (_, Some(t)) => t.anchor_para,
                _ => 0,
            }
        };
        let first_para = para_of(pg, true);
        let last_para = para_of(pg, false);
        // 續排：此頁首行的段落在上一頁末行已出現過。
        let continues = i > 0 && first_para == para_of(&pages[i - 1], false);
        // starts_section：此頁起頭 section 與上一頁不同（nextPage 邊界換頁）。
        let starts_section = i > 0 && pages[i - 1].section_id != pg.section_id;
        breaks.push(PageBreak {
            page: pg.index,
            first_para,
            last_para,
            line_count: pg.lines.len(),
            continues_paragraph: continues,
            width_pt: pg.width_pt,
            height_pt: pg.height_pt,
            starts_section,
        });
    }
    if breaks.is_empty() {
        breaks.push(PageBreak {
            page: 0,
            first_para: 0,
            last_para: 0,
            line_count: 0,
            continues_paragraph: false,
            width_pt: p.page_w_pt,
            height_pt: p.page_h_pt,
            starts_section: false,
        });
    }
    breaks
}

/// caret 位置（段落 + 段內碼點偏移）。★Web-1：隱形 contenteditable 錨點與
/// canvas 版面之間的游標契約——`cp` 以**碼點**（Unicode scalar）計，JS 端用
/// `Array.from(text)` 換算 UTF-16 索引，emoji（surrogate pair）不會斷一半。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CaretPos {
    pub para: usize,
    pub cp: usize,
}

/// caret 在頁面上的矩形（pt），JS 端縮放後疊畫在 canvas 上。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CaretRect {
    pub page: usize,
    pub x_pt: f32,
    pub y_pt: f32,
    pub height_pt: f32,
}

/// 版面引擎（Phase0-6：段落級快取＋增量分頁＋預算制軟分頁）。
///
/// 三段式增量（取代骨架的「任何編輯整篇重排」）：
/// 1. **斷行快取**：逐段落快取 KP 結果（`cache`），編輯只重斷被改的那一段。
/// 2. **收斂式重排**：分頁掃描（`pack_run`）從受影響頁起算，一旦游標與
///    舊版面某個頁起頭對齊且該頁起頭段落已過髒段，舊版面尾段直接重用
///    （`PackOutcome::Converged`）——打字一個字，重排成本 ≈ 一到兩頁。
/// 3. **預算制軟分頁**：`set_sync_budget(Some(n))` 後，同步路徑最多新增 n
///    頁（但編輯段必落進已定案區）；尾段暫留舊快照並回報
///    `ReflowPending`，shell 空檔呼叫 `reflow_step` 續排。
///
/// 正當性錨點：**增量結果 ≡ 整篇重建**（property test `incremental_matches_full_rebuild`）。
pub struct LayoutEngine {
    texts: Vec<String>,
    params: LayoutParams,
    /// 逐段落所屬 section 的識別與頁面幾何（engine 層由 sectPr 解析餵入；空＝單幾何）。
    sections: Vec<SectionSpec>,
    pages: Vec<Page>,
    // ---------------- Phase0-6 增量狀態 ----------------
    /// 逐段落斷行結果快取（已蓋 para_index 章）；`None` = 待重斷（髒）。
    /// 表格內部段落同樣入快取（編輯面可改；版面呈現由表格切片接管）。
    cache: Vec<Option<ParagraphLayout>>,
    /// 每頁起頭位置（流項, 列, 帶）。不變量：`starts.len() == pages.len()`，
    /// 且嚴格單調——增量收斂的對齊鍵與 shell 重繪錨點。
    starts: Vec<FlowPos>,
    /// 段落＋表格混排流（`build_flow` 產物；表格變動時重建）。
    flow: Vec<FlowItem>,
    /// 已排版表格槽（engine 層由 canonical 映射餵入；排序＝para_start）。
    tables: Vec<TableSlot>,
    /// 預算用盡時的待续掃描狀態＋已定案頁數（Some = 尾段是舊快照的軟分頁）。
    pending: Option<(PackState, usize)>,
    /// 同步路徑預算（頁）；`None`＝全速（預設：既有行為完全不變，只變快）。
    sync_budget: Option<usize>,
}

impl LayoutEngine {
    pub fn new(texts: &[String], params: LayoutParams) -> Self {
        Self::with_sections(texts, params, Vec::new())
    }

    /// 帶 section 資訊的建構：逐段斷行寬度與逐頁容量/尺寸都以檔案為準。
    pub fn with_sections(
        texts: &[String],
        params: LayoutParams,
        sections: Vec<SectionSpec>,
    ) -> Self {
        Self::with_tables(texts, params, sections, Vec::new())
    }

    /// Phase3-1b：帶表格槽的建構。`tables` 須依 para_start 排序且不重疊
    /// （engine 層映射時整理；违约由 build_flow 的 debug_assert 抓）。
    pub fn with_tables(
        texts: &[String],
        params: LayoutParams,
        sections: Vec<SectionSpec>,
        tables: Vec<TableSlot>,
    ) -> Self {
        let cache: Vec<Option<ParagraphLayout>> = vec![None; texts.len()];
        let mut eng = Self {
            texts: texts.to_vec(),
            params,
            sections,
            pages: Vec::new(),
            cache,
            starts: Vec::new(),
            flow: Vec::new(),
            tables,
            pending: None,
            sync_budget: None,
        };
        eng.rebuild_full();
        eng
    }

    /// 整篇重建（斷行快取全填＋全速重排）。增量正確性的對照基準就長這樣。
    fn rebuild_full(&mut self) {
        for i in 0..self.cache.len() {
            if self.cache[i].is_none() {
                let pl = self.break_para(i);
                self.cache[i] = Some(pl);
            }
        }
        let refs: Vec<&ParagraphLayout> = self
            .cache
            .iter()
            .map(|c| c.as_ref().expect("剛填滿"))
            .collect();
        self.flow = build_flow(self.texts.len(), &self.tables);
        let (pages, starts) = paginate_flow(
            &self.flow,
            &refs,
            &self.tables,
            &self.params,
            &self.sections,
        );
        self.pages = pages;
        self.starts = starts;
        self.pending = None;
    }

    /// 單段 KP 斷行（蓋 para_index 章）。只給髒段落呼叫——快取命中零成本。
    fn break_para(&self, i: usize) -> ParagraphLayout {
        let content_w = section_params(&self.params, &self.sections, i).content_w_pt();
        let m = TextMeasurer::new(self.params.font_size_pt);
        let mut pl = ParagraphLayout {
            para_index: i,
            lines: break_lines(&self.texts[i], content_w, &m),
        };
        for l in pl.lines.iter_mut() {
            l.para_index = i;
        }
        pl
    }

    /// 預算制軟分頁開關：`Some(n)` 時同步重排最多新增 n 頁（未定案尾段留給
    /// `reflow_step`）；`None`（預設）全速。n 下限 1（0 會餓死同步路徑）。
    pub fn set_sync_budget(&mut self, pages: Option<usize>) {
        self.sync_budget = pages.map(|n| n.max(1));
    }

    /// 版面是否已定案（無待排尾段）。shell 用于決定要不要排 `reflow_step`。
    pub fn is_settled(&self) -> bool {
        self.pending.is_none()
    }

    /// 每頁起頭位置（流項, 列, 帶）——除錯/基準/增量測試錨點。
    pub fn page_starts(&self) -> &[FlowPos] {
        &self.starts
    }

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// 已排版表格槽（Phase3-1b；engine 層顯示列表換算用）。
    pub fn table_slots(&self) -> &[TableSlot] {
        &self.tables
    }

    /// 段落純文字（編輯面讀目前工作副本；JS 殼游標插入需要）。
    pub fn paragraph_text(&self, para: usize) -> Option<&str> {
        self.texts.get(para).map(String::as_str)
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn params(&self) -> LayoutParams {
        self.params
    }

    /// 逐段落所屬 section 的識別與頁面幾何（engine 層組 pagebreaks／除錯用）。
    pub fn sections(&self) -> &[SectionSpec] {
        &self.sections
    }

    /// 增量重排：只重斷被改段落、只重排受影響頁，游標與舊版面頁起頭對齊後
    /// 舊版面尾段原樣重用（typical 打字＝重排 1–2 頁）。
    ///
    /// 若上一筆編輯因預算未排完（軟分頁），先排完再接新編輯——保證每次重排
    /// 只有一個髒段落，且軟分頁的「暫缺尾頁」不會跨越兩筆編輯疊代累積。
    pub fn replace_paragraph_text(&mut self, para: usize, new_text: &str) -> Vec<Invalidation> {
        // 軟分頁未定案 → 先同步排完；重繪事件并入本次回報（不然 shell 漏畫）。
        let mut evs: Vec<Invalidation> = Vec::new();
        while self.pending.is_some() {
            let (mut settle_evs, _) = self.reflow_bounded(None);
            evs.append(&mut settle_evs);
        }
        let Some(cur) = self.texts.get(para) else {
            // 段落不存在：版面必不變（骨架版會白重排整篇）；如實回報 noop。
            return evs;
        };
        if cur == new_text {
            return evs;
        }
        self.texts[para] = new_text.to_string();
        self.cache[para] = None;
        let pl = self.break_para(para);
        self.cache[para] = Some(pl);

        // 段落 → 流項：表格內部段落（流失權）改了也不動版面切片——老實 noop
        // （顯示列表不經 Line 途徑，切片文字由 engine 重建槽時更新）。
        let Some(fi) = self
            .flow
            .iter()
            .position(|it| matches!(it, FlowItem::Text { para: p } if *p == para))
        else {
            return evs;
        };
        // 錨點 a＝包含編輯項首行的那頁：[0, a) 各頁全部來自更前面的流項
        // （未動），原樣保留；掃描從 (fi,0,0) 所在頁起重新打包。
        let a = self
            .starts
            .partition_point(|s| *s <= (fi, 0, 0))
            .saturating_sub(1);
        let mut st = PackState {
            cursor: self.starts[a],
            cur: Vec::new(),
            cur_bands: Vec::new(),
            cur_tables: Vec::new(),
            cur_used: 0,
            // 空 cur ⇒ 首行必不 trigger 換頁（前頁已保留），None 安全。
            cur_spec: None,
            page_start: self.starts[a],
        };
        let mut out = self.pages[..a].to_vec();
        let mut out_starts = self.starts[..a].to_vec();
        // 收斂候選：a+1 頁起的舊頁起頭（a 頁含髒項，永不收斂）。
        let mut reuse = ReuseTail {
            starts: self.starts[a + 1..].to_vec(),
            dirty: fi,
            head: 0,
        };
        let refs: Vec<&ParagraphLayout> = self
            .cache
            .iter()
            .map(|c| c.as_ref().expect("髒段已重斷"))
            .collect();
        let old_len = self.pages.len();
        let outcome = pack_run(
            &self.flow,
            &refs,
            &self.tables,
            &self.params,
            &self.sections,
            &mut st,
            &mut out,
            &mut out_starts,
            PackPolicy {
                reuse: Some(&mut reuse),
                budget: self.sync_budget,
                cover_item: fi,
            },
        );
        let pending = match outcome {
            PackOutcome::Exhausted => None,
            PackOutcome::Converged => {
                // 掃描在舊頁起頭 k 對齊：舊版面自 k 頁往後就是本次必產結果
                // （論證見 PackOutcome::Converged），接上並重新編號即可。
                let k = (a + 1 + reuse.head).min(old_len);
                for pg in self.pages[k..].iter() {
                    let mut pg = pg.clone();
                    pg.index = out.len();
                    out.push(pg);
                }
                out_starts.extend_from_slice(&self.starts[k..]);
                None
            }
            PackOutcome::BudgetStopped => {
                // 軟分頁：已打包頁就是本次定案前全部版面——暫缺未排尾頁
                // （寧可少頁不可重複/丟內容），`reflow_step` 空檔補齊。
                evs.push(Invalidation::ReflowPending {
                    from_page: out.len(),
                });
                Some((st, out.len()))
            }
        };
        let new_len = out.len();
        evs.push(Invalidation::Paragraph(para));
        // 重繪範圍：錨點頁起到新舊較長邊界（多報幾個 Page 事件無害，
        // 換 shell 實作簡單可靠；精確髒頁清單留作後續最佳化）。
        evs.extend((a..new_len.max(old_len)).map(Invalidation::Page));
        self.pages = out;
        self.starts = out_starts;
        self.pending = pending;
        evs
    }

    /// 背景續排一步（shell 於 requestIdleCallback/空檔呼叫）：把軟分頁暫缺
    /// 的尾段續排掉，回傳（重繪事件, 是否已定案）。已定案時回空。
    pub fn reflow_step(&mut self) -> (Vec<Invalidation>, bool) {
        self.reflow_bounded(self.sync_budget)
    }

    fn reflow_bounded(&mut self, budget: Option<usize>) -> (Vec<Invalidation>, bool) {
        let Some((mut st, committed)) = self.pending.take() else {
            return (Vec::new(), true);
        };
        let mut out = self.pages[..committed].to_vec();
        let mut out_starts = self.starts[..committed].to_vec();
        let refs: Vec<&ParagraphLayout> = self
            .cache
            .iter()
            .map(|c| c.as_ref().expect("髒段已在同步路徑重斷"))
            .collect();
        let old_len = self.pages.len();
        // 無舊尾段可收斂（軟分頁已把暫缺尾頁摘除），直接續排到收斂點不存在
        // 為止：要嘛排完（Exhausted），要嘛預算再停一檔（BudgetStopped）。
        let outcome = pack_run(
            &self.flow,
            &refs,
            &self.tables,
            &self.params,
            &self.sections,
            &mut st,
            &mut out,
            &mut out_starts,
            PackPolicy {
                reuse: None,
                budget,
                cover_item: 0,
            },
        );
        let pending = match outcome {
            PackOutcome::Exhausted => None,
            PackOutcome::Converged => {
                // reflow 不餵 reuse，理論不可達；防呆走 Exhausted 語義。
                unreachable!("reflow 不帶收斂候選")
            }
            PackOutcome::BudgetStopped => Some((st, out.len())),
        };
        let new_len = out.len();
        let mut evs: Vec<Invalidation> = (committed..new_len.max(old_len))
            .map(Invalidation::Page)
            .collect();
        if let Some((_, c)) = &pending {
            evs.push(Invalidation::ReflowPending { from_page: *c });
        }
        self.pages = out;
        self.starts = out_starts;
        self.pending = pending;
        (evs, self.pending.is_none())
    }

    /// caret (段落, 碼點) → 頁面矩形。Web-1 錨點定位：JS 把隱形 contenteditable
    /// 移到此處，IME candidate window 才會跟著游標走；同時疊畫 caret。
    /// 找不到（段落不存在／空段落在不可見頁）回 None，呼叫端退回段落首。
    pub fn caret_rect(&self, caret: CaretPos) -> Option<CaretRect> {
        let text = self.texts.get(caret.para)?;
        let total_cp = text.chars().count();
        let cp = caret.cp.min(total_cp);
        let line_h = self.params.font_size_pt * self.params.line_spacing;

        // 逐頁掃描屬於本段的行：以各行字元數累計，找出 cp 落在哪一行。
        // 座標原點取「此頁所屬 section」的邊界（跨 section 續排的段落，兩頁邊界可不同）。
        let mut seen_cp = 0usize; // 本段已消費的碼點數
        let mut last: Option<CaretRect> = None; // 空行/末段行兜底
        for page in &self.pages {
            for (li, line) in page.lines.iter().enumerate() {
                if line.para_index != caret.para {
                    continue;
                }
                let line_cp = line.text.chars().count();
                let x = self.caret_x_in_line(&line.text, cp.saturating_sub(seen_cp));
                // y 走行網格帶索引（表格插進網格後 li ≠ 帶；缺欄位退回 li）。
                let band = page.line_bands.get(li).copied().unwrap_or(li as u32) as f32;
                let rect = CaretRect {
                    page: page.index,
                    x_pt: page.margin_left_pt + x,
                    y_pt: page.margin_top_pt + band * line_h,
                    height_pt: self.params.font_size_pt,
                };
                // cp 落在本行範圍內（含行尾）即命中；最後一行兜底（cp=段尾等）。
                if cp <= seen_cp + line_cp || line_cp == 0 {
                    return Some(rect);
                }
                seen_cp += line_cp;
                last = Some(rect);
            }
        }
        last
    }

    /// 頁面點擊 (page, x_pt, y_pt) → caret。canvas 是顯示端、contenteditable 是
    /// 輸入端，點 canvas 時必須能把點擊換算回輸入端游標（Web-1 反向契約）。
    pub fn caret_at(&self, page: usize, x_pt: f32, y_pt: f32) -> Option<CaretPos> {
        let p = self.pages.get(page)?;
        let line_h = self.params.font_size_pt * self.params.line_spacing;
        let band = (((y_pt - p.margin_top_pt) / line_h).floor().max(0.0)) as u32;
        // y→帶→行陣列索引：有表格的頁 band ≠ 陣標（`line_bands` 才是映射，
        // caret_rect 正向走同一映射）。點在表格帶／空帶上命中不到文字行 →
        // None（表格內 caret 不在 v1 範圍）。無表格頁 line_bands＝0..n，
        // position 命中恒等索引，舊行為逐位元組不變。
        let li = if p.line_bands.is_empty() {
            band as usize // 舊快照無 line_bands：退回陣標（與 caret_rect 同策）
        } else {
            p.line_bands.iter().position(|&b| b == band)?
        };
        let line = p.lines.get(li)?;
        let rel = (x_pt - p.margin_left_pt).max(0.0);
        let cp_in_line = self.caret_cp_in_line(&line.text, rel);
        // 段落內偏移 = 該段在目標行之前所有行的碼點數之和（跨頁續排亦正確：
        // 行帶著 para_index，線性掃過全部頁面對應段落行）。
        let mut before = 0usize;
        'outer: for pg in &self.pages {
            for (i, l) in pg.lines.iter().enumerate() {
                if l.para_index != line.para_index {
                    continue;
                }
                if pg.index == page && i == li {
                    break 'outer;
                }
                before += l.text.chars().count();
            }
        }
        Some(CaretPos {
            para: line.para_index,
            cp: before + cp_in_line,
        })
    }

    /// 行內水平偏移 → caret x（shaped advances 逐 cluster 累計，取「不越過 rel」
    /// 的最後 cluster 邊界）。
    fn caret_x_in_line(&self, line_text: &str, cp: usize) -> f32 {
        let clusters = shape_advances(line_text, self.params.font_size_pt);
        let mut byte_target = line_text.len();
        if cp < line_text.chars().count() {
            byte_target = line_text
                .char_indices()
                .nth(cp)
                .map(|(i, _)| i)
                .unwrap_or(line_text.len());
        }
        let mut x = 0.0f32;
        for c in &clusters {
            if (c.start as usize) >= byte_target {
                break;
            }
            x += c.w;
        }
        x
    }

    /// 行內 x → caret cp（與 `caret_x_in_line` 互逆：取最接近 rel 的 cluster 邊界）。
    fn caret_cp_in_line(&self, line_text: &str, rel: f32) -> usize {
        let clusters = shape_advances(line_text, self.params.font_size_pt);
        let mut x = 0.0f32;
        let mut best_cp = 0usize;
        let mut best_err = f32::INFINITY;
        for c in &clusters {
            let err = (x - rel).abs();
            if err < best_err {
                best_err = err;
                best_cp = line_text[..c.start as usize].chars().count();
            }
            x += c.w;
        }
        let err_end = (x - rel).abs();
        if err_end < best_err {
            best_cp = line_text.chars().count();
        }
        best_cp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_linebreak::{break_property, BreakClass};

    #[test]
    fn cjk_wraps_at_width() {
        let p = LayoutParams::default();
        let m = TextMeasurer::new(p.font_size_pt);
        let text = "開源文書處理器測試".repeat(50);
        let lines = break_lines(&text, 400.0, &m);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| l.width_pt <= 400.0 + 1.0));
    }

    #[test]
    fn latin_breaks_on_spaces() {
        let p = LayoutParams::default();
        let m = TextMeasurer::new(p.font_size_pt);
        let text = "word ".repeat(200);
        let lines = break_lines(text.trim(), 400.0, &m);
        assert!(lines.iter().all(|l| l.width_pt <= 400.0 + 1.0));
        assert!(lines.len() > 1);
        // 詞不被切斷：每行只剩完整 "word"。
        assert!(lines
            .iter()
            .all(|l| l.text.trim().replace(' ', "").len() % 4 == 0));
    }

    #[test]
    fn kp_beats_greedy_on_ragged_cost() {
        // 經典案例：貪心把「窄」留在行尾造成大鬆散；KP 把它挪去下一行。
        let m = TextMeasurer::new(12.0);
        let text = "寬寬寬寬 窄 寬寬寬寬 窄 寬寬寬寬 窄 寬寬寬寬 窄";
        let max_w = 72.0; // 6 em：一行恰裝 5 個漢字寬上下
        let kp = break_lines(text, max_w, &m);

        // 貪心基線（舊演算法：前瞻斷，塞不下才換行）。
        let clusters = shape_advances(text, 12.0);
        let width_of = |s: &str| -> f32 {
            let base = text.find(s).unwrap_or(0);
            clusters
                .iter()
                .filter(|c| (c.start as usize) >= base && (c.start as usize) < base + s.len())
                .map(|c| c.w)
                .sum()
        };
        let mut greedy: Vec<String> = Vec::new();
        let mut cur = String::new();
        for tok in text.split_inclusive(' ') {
            let cand = format!("{cur}{tok}");
            let w = width_of(cand.trim_end());
            if !cur.is_empty() && w > max_w {
                greedy.push(std::mem::take(&mut cur).trim_end().to_string());
            }
            cur.push_str(tok);
        }
        if !cur.trim_end().is_empty() {
            greedy.push(cur.trim_end().to_string());
        }

        // KP 總代價必須 ≤ 貪心總代價（同尺標下比較）。
        let total_cost = |lines: &[String]| -> f64 {
            lines
                .iter()
                .map(|l| kp::line_cost(width_of(l), max_w, true, false))
                .sum()
        };
        let kp_cost: f64 = kp
            .iter()
            .map(|l| kp::line_cost(l.width_pt, max_w, true, false))
            .sum();
        let greedy_cost = total_cost(&greedy);
        assert!(
            kp_cost <= greedy_cost + 1e-6,
            "KP {kp_cost} 應 ≤ 貪心 {greedy_cost}；kp={kp:?} greedy={greedy:?}"
        );
    }

    #[test]
    fn mandatory_controls_break_lines() {
        let m = TextMeasurer::new(12.0);
        let lines = break_lines("第一段\n第二段\r\n第三段", 400.0, &m);
        assert_eq!(
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            vec!["第一段", "第二段", "第三段"]
        );
    }

    // ---------------- property tests：任意文字不斷字 ----------------

    /// 決定性偽亂數（LCG）：無依賴、可重現。
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
            &xs[(self.next() as usize) % xs.len()]
        }
    }

    const WORDS: [&str; 22] = [
        "Quire",
        "processor",
        "hi",
        "a",
        "extraordinary",
        "Rust",
        "fi",
        "café",
        "naïve",
        "3,000",
        "95,000.50",
        "$42",
        "50%",
        "2026-10-14",
        "https://example.com/x",
        "he-llo",
        "中文",
        "文書處理",
        "斷行",
        "演算法",
        "\u{1F600}",
        "ﬃ",
    ];
    const SEPS: [&str; 11] = [
        " ", " ", " ", "\u{3000}", "\u{a0}", "", "\t", "，", "。", "、", "　",
    ];
    const PUNCT: [&str; 16] = [
        "，", "。", "、", "！", "？", "：", "；", "「", "」", "（", "）", "——", "…", "·", "'", "）",
    ];
    const CTRL: [&str; 7] = [
        "\n", "\r\n", "\r", "\u{85}", "\u{2028}", "\u{2029}", "\u{0c}",
    ];

    fn random_text(rng: &mut Rng, budget: usize) -> String {
        let mut s = String::new();
        while s.chars().count() < budget {
            match rng.next() % 100 {
                0..=44 => s.push_str(rng.pick(&WORDS)),
                45..=69 => s.push_str(rng.pick(&SEPS)),
                70..=84 => s.push_str(rng.pick(&PUNCT)),
                85..=89 => s.push_str(rng.pick(&CTRL)),
                90..=94 => {
                    // Latin-1 高位區（é ÿ 等）：測試非 ASCII 非 CJK 的字寬路徑。
                    // （舊寫法 `as u8 as char` 把 CJK 码位截斷成 Latin-1，是 bug。）
                    if let Some(c) = char::from_u32(0xA0 + (rng.next() % 0x60) as u32) {
                        s.push(c);
                    }
                }
                _ => {
                    let c = 0x4E00 + (rng.next() % 0x5000) as u32;
                    if let Some(ch) = char::from_u32(c) {
                        s.push(ch);
                    }
                }
            }
        }
        s
    }

    /// 行首禁現的 UAX#14 類別（LB6/13/19 等：這些字元永不許出現在軟斷點右側）。
    const LINE_START_FORBIDDEN: [BreakClass; 9] = [
        BreakClass::CombiningMark,
        BreakClass::ZeroWidthJoiner,
        BreakClass::NonBreakingGlue,
        BreakClass::WordJoiner,
        BreakClass::ClosePunctuation,
        BreakClass::CloseParenthesis,
        BreakClass::Exclamation,
        BreakClass::Inseparable,
        BreakClass::NonStarter,
    ];

    /// P2b 的孤兒標記豁免類別：UAX#14 規則優先序中 LB18「SP ÷」高於各條
    /// `×` 規則（LB6 × CM、LB21 × NS…），官方 LineBreakTest（Unicode 15.1）
    /// 明文 `SP ÷ CM1`（`0020 ÷ 0308` ×129）、`SP ÷ GL`（NBSP ×123）、`SP ÷ IN`
    /// （`%` ×123）、`SP ÷ ZWJ`——空格之後的標記/黏著字元是「孤兒」形态
    /// （LB1 後無基字可附），LB18 把它與前文分開是合法斷點。真正禁的是
    /// **附著**序列被拆（基字+組合記號分家）：那類位置前無可斷空白，由
    /// P2a（斷點必須來自 UAX#14）＋P2b 雙重封鎖。
    /// WJ/CL/CP/EX 不在豁免名單：LB13「even after spaces」連空格後也不放
    /// （官方資料佐證 `0020 × 2060` ×128、`0020 × 0029` ×132）。
    const ORPHAN_EXEMPT: [BreakClass; 5] = [
        BreakClass::CombiningMark,
        BreakClass::ZeroWidthJoiner,
        BreakClass::NonBreakingGlue,
        BreakClass::Inseparable,
        BreakClass::NonStarter,
    ];

    fn check_case(text: &str, max_w: f32) {
        let m = TextMeasurer::new(12.0);
        let spans = kp::kp_break_lines(text, max_w, &m);
        // UAX#14 底層認可的斷字機會位址集（P2a：本層不准自創斷點）。
        let legal: std::collections::HashSet<usize> = unicode_linebreak::linebreaks(text)
            .map(|(p, _)| p)
            .collect();
        let mut pos = 0usize;
        for (k, sp) in spans.iter().enumerate() {
            let (s, e) = sp.range;
            // P1 不丟字不重複：區間連續鋪滿全文。
            assert_eq!(s, pos, "行 {k} 區間不連續：{text:?}");
            pos = e;
            let raw = &text[s..e];
            // P1b 顯示文字＝raw 去行尾空白/控制字元＋去掉全部控制字元。
            let expect: String = raw
                .trim_end_matches(|c: char| uax14::is_ignorable(c))
                .chars()
                .filter(|c| !EOL_CONTROLS.contains(c))
                .collect();
            assert_eq!(sp.line.text, expect, "行 {k} 顯示文字不符：{text:?}");
            // P4 控制字元只准出现在行尾區（不進顯示文字）。
            assert!(
                raw.chars()
                    .rev()
                    .skip_while(|c| uax14::is_ignorable(*c))
                    .all(|c| !EOL_CONTROLS.contains(&c))
                    || raw
                        .find(EOL_CONTROLS)
                        .is_none_or(|i| raw[i..].chars().all(uax14::is_ignorable)),
                "行 {k} 中段出現控制字元：{raw:?}"
            );
            // P3 行寬約束（超寬只准出現在無空白可斷的孤例行）。
            if sp.line.width_pt > max_w + 1.0 {
                assert!(
                    !sp.line.text.contains([' ', '\t', '\u{3000}']),
                    "行 {k} 超寬卻含可斷空白：{text:?} w={}",
                    sp.line.width_pt
                );
            }
            // P2 軟斷點合法性（前一行硬斷者豁免：強制邊界/緊急切分無選擇）。
            if k > 0 && !spans[k - 1].hard_end {
                // P2a：軟斷點位址必須是 UAX#14（unicode-linebreak）認可的
                //      斷字機會——KP/atom 層永不許在 «×» 位置自創斷點。
                assert!(
                    legal.contains(&s),
                    "軟斷點 @{s} 非 UAX#14 斷字機會：{text:?} 行 {k}={:?}",
                    sp.line.text
                );
                // P2b：行首禁現禁斷類別（孤兒標記＋前導可斷空白豁免，見 ORPHAN_EXEMPT）。
                // 判據＝斷點左鄰字元本身：可斷空白（LB18 `SP ÷`）或 ZWSP
                // （LB8 `ZW SP* ÷`）時，孤兒標記另起一行合法。左鄰若是 EOL
                // 控制字元，前一行已是 hard_end、整段 P2 跳過，不進此分支。
                if let Some(first) = sp.line.text.chars().next() {
                    let cls = break_property(first as u32);
                    let prev = (s > 0).then(|| text[..s].chars().next_back().unwrap());
                    let orphan_ok = ORPHAN_EXEMPT.contains(&cls)
                        && matches!(prev, Some(c) if uax14::is_ws(c) || c == '\u{200b}');
                    assert!(
                        !LINE_START_FORBIDDEN.contains(&cls) || orphan_ok,
                        "軟斷行首出現 {:?}({:?})：{text:?} 行 {k}={:?} max_w={max_w}",
                        cls,
                        first,
                        sp.line.text
                    );
                }
            }
        }
        assert_eq!(pos, text.len(), "行未鋪滿全文：{text:?}");
    }

    #[test]
    fn property_no_word_split_and_no_loss() {
        // 固定種子 ⇒ 可重現；也餵一輪邊界字串。
        let fixed = [
            "",
            " ",
            "\n",
            "a",
            "中",
            "   \n  ",
            "word\u{a0}word\u{a0}word",
            "結束。結束。結束。結束。結束。",
            "引用「開頭」結尾（括號）《书名号》",
            "a\n\nb\r\nc\u{2028}\u{2029}d",
            "非常長的一個沒有空白的字串abcdefghijabcdefghijabcdefghij結束",
            "3,000,000元（約USD95,000.50）於2026-10-14核定，共50%！",
        ];
        for t in fixed {
            for w in [12.0, 24.0, 47.0, 100.0, 451.0] {
                check_case(t, w);
            }
        }
        let mut rng = Rng(0x5EED_2026);
        for i in 0..400 {
            let budget = 8 + (rng.next() % 800) as usize;
            let text = random_text(&mut rng, budget);
            let max_w = 12.0 + (rng.next() % 600) as f32;
            check_case(&text, max_w);
            if i % 50 == 0 {
                // 極窄行寬壓緊急切分路徑。
                check_case(&text, 6.0);
            }
        }
    }

    /// OOM 回歸（run40/42/44 死因）：長 CJK 段每字皆可斷，是 active list 膨脹
    /// 的最壞輸入。演算法若回到組合爆炸，這支測試會被記憶體管理機制打掉
    /// （舊碼 n=8000 吃 15.4GB）；線性實作下應在毫秒級完成。
    #[test]
    fn kp_scales_linearly_on_long_cjk() {
        let m = TextMeasurer::new(12.0);
        let text = "開源文書處理器測試".repeat(2500); // 25000 字元 ≈ 50 頁
        let t0 = std::time::Instant::now();
        let lines = break_lines(&text, 451.0, &m);
        let dt = t0.elapsed();
        assert!(
            lines.len() > 600,
            "25000 字元應排超過 600 行：{}",
            lines.len()
        );
        assert!(lines.iter().all(|l| l.width_pt <= 451.0 + 1.0));
        // 寬鬆上限：正常 <0.1s（release）。舊爆炸實作根本跑不完（先 OOM）。
        assert!(
            dt.as_secs_f64() < 5.0,
            "25k 字元排了 {dt:?}——疑似複雜度回歸"
        );
    }

    // ---------------- 回歸：引擎/分頁 ----------------

    #[test]
    fn pagination_produces_multiple_pages() {
        let texts: Vec<String> = (0..40)
            .map(|i| format!("第 {i} 段：{}", "內文".repeat(60)))
            .collect();
        let p = LayoutParams::default();
        let eng = LayoutEngine::new(&texts, p);
        assert!(eng.page_count() > 3);
    }

    #[test]
    fn page_breaks_track_paragraphs() {
        // 段 0 很長（跨頁）、後面都是短段：第 1 頁應從段 0 續排。
        let texts: Vec<String> = std::iter::once("長文".repeat(2000))
            .chain((0..30).map(|i| format!("短段 {i}")))
            .collect();
        let p = LayoutParams::default();
        let brks = page_breaks(&texts, &p);
        assert!(brks.len() >= 2, "段 0 超長，必定跨頁");
        assert_eq!(brks[0].page, 0);
        assert!(brks[1].continues_paragraph, "段 0 未結束即跨頁");
        assert_eq!(brks[1].first_para, 0);
        // 每頁行數不超過版面容量，且段落覆蓋連續不跳號。
        let per_page = p.per_page_lines();
        assert!(brks.iter().all(|b| b.line_count <= per_page));
        assert!(brks
            .windows(2)
            .all(|w| w[1].first_para <= w[0].last_para + 1));
    }

    #[test]
    fn edit_updates_pagination() {
        let texts: Vec<String> = (0..30).map(|_| "短段落".to_string()).collect();
        let mut eng = LayoutEngine::new(&texts, LayoutParams::default());
        let before = eng.page_count();
        let evs = eng.replace_paragraph_text(0, &"超長".repeat(4000));
        assert!(evs.contains(&Invalidation::Paragraph(0)));
        assert!(
            evs.iter().any(|e| matches!(e, Invalidation::Page(_))),
            "重排必須回報髒頁重繪事件"
        );
        assert!(eng.is_settled(), "預設全速模式：編輯後版面即定案");
        assert!(eng.page_count() > before);
    }

    #[test]
    fn caret_rect_follows_shaped_advances() {
        // 純 CJK 段：caret x 隨碼點單調遞增，且首字元在版心左緣。
        let texts = vec!["測試文字一二三四".to_string()];
        let eng = LayoutEngine::new(&texts, LayoutParams::default());
        let r0 = eng.caret_rect(CaretPos { para: 0, cp: 0 }).unwrap();
        let r2 = eng.caret_rect(CaretPos { para: 0, cp: 2 }).unwrap();
        let r4 = eng.caret_rect(CaretPos { para: 0, cp: 4 }).unwrap();
        assert_eq!(r0.page, 0);
        assert!((r0.x_pt - 72.0).abs() < f32::EPSILON, "cp=0 應在版心左緣");
        assert!(r2.x_pt > r0.x_pt && r4.x_pt > r2.x_pt, "x 隨碼點單調遞增");
        assert_eq!(r0.height_pt, 12.0, "caret 高＝字級");
    }

    #[test]
    fn caret_at_is_inverse_of_caret_rect() {
        // 正反契約互逆：caret→rect→（同一頁點擊 rect 位置）→caret 必須還原。
        let texts = vec![
            "Quire 是一個開源的中文文書處理器，目標是取代 Word。".repeat(3),
            "第二段 混排 English 與中文。".to_string(),
        ];
        let eng = LayoutEngine::new(&texts, LayoutParams::default());
        let total = texts[0].chars().count();
        for cp in [0usize, 1, 5, total / 3, total] {
            let rect = eng.caret_rect(CaretPos { para: 0, cp }).unwrap();
            let back = eng
                .caret_at(rect.page, rect.x_pt, rect.y_pt + 6.0)
                .expect("rect 位置必命中");
            assert_eq!(back.para, 0);
            // 行邊界處 cluster 量化可能差 1 碼點（最接近邊界取整），容差 1。
            assert!(
                back.cp.abs_diff(cp) <= 1,
                "互逆容差：cp {cp} → {:?}",
                back.cp
            );
        }
    }

    #[test]
    fn caret_follows_paragraph_across_pages() {
        // 長段跨頁：段尾 caret 必須落到後續頁（canvas 切頁＋錨點定位的前提）。
        let texts = vec!["長文跨頁測試。".repeat(300), "尾段".to_string()];
        let eng = LayoutEngine::new(&texts, LayoutParams::default());
        let total = texts[0].chars().count();
        let end = eng.caret_rect(CaretPos { para: 0, cp: total }).unwrap();
        assert!(end.page > 0, "段尾 caret 應在後續頁，實得 {}", end.page);
        let back = eng.caret_at(end.page, end.x_pt, end.y_pt + 6.0).unwrap();
        assert_eq!((back.para, back.cp), (0, total), "跨頁反查必須還原段尾");
    }

    // ---------------- Phase0-2b：頁面幾何讀 sectPr ----------------

    fn letter_geo() -> SectionGeometry {
        // Letter 直式 12240×15840 twip = 612×792 pt，邊界全 1 吋（1440 twip）。
        SectionGeometry {
            page_w_pt: 612.0,
            page_h_pt: 792.0,
            margin_top_pt: 72.0,
            margin_bottom_pt: 72.0,
            margin_left_pt: 72.0,
            margin_right_pt: 72.0,
            landscape: false,
            continuous: false,
        }
    }

    fn letter_landscape_geo() -> SectionGeometry {
        SectionGeometry {
            page_w_pt: 792.0,
            page_h_pt: 612.0,
            margin_top_pt: 72.0,
            margin_bottom_pt: 72.0,
            margin_left_pt: 72.0,
            margin_right_pt: 72.0,
            landscape: true,
            continuous: false,
        }
    }

    /// 單 section 文件（id 全 0）。
    fn single(id_geo: SectionGeometry, n: usize) -> Vec<SectionSpec> {
        vec![SectionSpec { id: 0, geo: id_geo }; n]
    }

    #[test]
    fn letter_section_pages_report_letter_size_not_a4() {
        // 整份 Letter 文件：每頁（含 pagebreaks）必須報 612×792，不再是固定 A4。
        let texts: Vec<String> = (0..40)
            .map(|i| format!("第 {i} 段：{}", "內文".repeat(60)))
            .collect();
        let sections = single(letter_geo(), texts.len());
        let eng = LayoutEngine::with_sections(&texts, LayoutParams::default(), sections.clone());
        assert!(eng.page_count() > 3);
        assert!(
            eng.pages().iter().all(|pg| {
                (pg.width_pt - 612.0).abs() < f32::EPSILON
                    && (pg.height_pt - 792.0).abs() < f32::EPSILON
            }),
            "Letter 文件的每一頁都必須報 612×792"
        );
        let brks = page_breaks_per_section(&texts, &LayoutParams::default(), &sections);
        assert_eq!(brks.len(), eng.page_count());
        assert!(brks
            .iter()
            .all(|b| (b.width_pt - 612.0).abs() < f32::EPSILON));
        // Letter 版心高比 A4 少 → 同內容頁數必須 ≥ A4 版（頁數收斂方向正確）。
        let a4 = LayoutEngine::new(&texts, LayoutParams::default());
        assert!(eng.page_count() >= a4.page_count());
    }

    #[test]
    fn same_size_adjacent_sections_still_break_page() {
        // 相鄰兩節同為 Letter 直式（reader-styles 形態）：幾何不變仍必須在
        // section 邊界換頁——靠 SectionSpec::id 辨識，不是靠尺寸差異。
        let texts = vec!["第一段內容".repeat(8), "第二段內容".repeat(8)];
        let sections = vec![
            SectionSpec {
                id: 0,
                geo: letter_geo(),
            },
            SectionSpec {
                id: 1,
                geo: letter_geo(),
            },
        ];
        let eng = LayoutEngine::with_sections(&texts, LayoutParams::default(), sections.clone());
        let brks = page_breaks_per_section(&texts, &LayoutParams::default(), &sections);
        assert!(
            brks.iter().any(|b| b.starts_section && b.first_para == 1),
            "同尺寸相鄰 section 的邊界必須換頁：{brks:?}"
        );
        assert_eq!(brks.len(), eng.page_count());
    }

    #[test]
    fn continuous_section_does_not_break_page() {
        // w:type=continuous：邊界不強制換頁（同頁續排）。
        let texts = vec!["甲".to_string(), "乙".to_string()];
        let mut cont = letter_geo();
        cont.continuous = true;
        let sections = vec![
            SectionSpec {
                id: 0,
                geo: letter_geo(),
            },
            SectionSpec { id: 1, geo: cont },
        ];
        let eng = LayoutEngine::with_sections(&texts, LayoutParams::default(), sections);
        assert_eq!(eng.page_count(), 1, "continuous 邊界不換頁");
    }

    #[test]
    fn landscape_section_breaks_page_and_uses_wider_text_column() {
        // 兩 section：前段直式、後段橫向（ole_embedded_landscape / HC031 形態）。
        // 斷言：(a) section 邊界必定換頁；(b) 橫向頁報 792×612；(c) 橫向段以 792-144
        // 版心寬斷行 → 同一段文字比直式版心（612-144）斷得更少行。
        let long = "橫向段落測試內容".repeat(30);
        let texts = vec!["直式段落內容。".to_string(), long.clone(), long.clone()];
        let sections = vec![
            SectionSpec {
                id: 0,
                geo: letter_geo(),
            },
            SectionSpec {
                id: 0,
                geo: letter_geo(),
            },
            SectionSpec {
                id: 1,
                geo: letter_landscape_geo(),
            },
        ];
        let eng = LayoutEngine::with_sections(&texts, LayoutParams::default(), sections.clone());

        // (a)(b) 橫向頁存在、尺寸老實回報、且其起頭是 section 邊界而非容量已滿。
        let land_pages: Vec<&Page> = eng
            .pages()
            .iter()
            .filter(|pg| (pg.width_pt - 792.0).abs() < f32::EPSILON)
            .collect();
        assert!(!land_pages.is_empty(), "橫向 section 必須產出橫向頁");
        assert!(land_pages
            .iter()
            .all(|pg| (pg.height_pt - 612.0).abs() < f32::EPSILON));
        let brks = page_breaks_per_section(&texts, &LayoutParams::default(), &sections);
        let land_first = brks
            .iter()
            .find(|b| (b.width_pt - 792.0).abs() < f32::EPSILON)
            .expect("pagebreaks 應含橫向頁");
        assert!(land_first.starts_section, "橫向頁必由 section 邊界起頭");
        assert_eq!(land_first.first_para, 2);
        assert!(!land_first.continues_paragraph, "section 邊界不續排");

        // (c) 斷行寬度真的跟著 section：橫向版心 648pt vs 直式 468pt。
        let m = TextMeasurer::new(12.0);
        let wide = break_lines(&long, 792.0 - 144.0, &m).len();
        let narrow = break_lines(&long, 612.0 - 144.0, &m).len();
        assert!(
            wide < narrow,
            "同文在橫向版心應斷更少行：{wide} vs {narrow}"
        );
        let pl = layout_paragraphs_per_section(&texts, &LayoutParams::default(), &sections);
        assert_eq!(pl[1].lines.len(), narrow, "直式段按直式版心斷行");
        assert_eq!(pl[2].lines.len(), wide, "橫向段按橫向版心斷行");
    }

    #[test]
    fn landscape_line_widths_never_exceed_landing_content_width() {
        // 橫向段的行寬必須 ≤ 橫向版心（648pt）且可超過直式版心（468pt）——
        // 證明斷行用的是該段 section 的寬度，不是全域預設。
        let texts = vec!["混排 English words 與中文內容".repeat(20)];
        let sections = single(letter_landscape_geo(), 1);
        let pl = layout_paragraphs_per_section(&texts, &LayoutParams::default(), &sections);
        assert!(pl[0].lines.iter().all(|l| l.width_pt <= 648.0 + 1.0));
        assert!(
            pl[0].lines.iter().any(|l| l.width_pt > 468.0),
            "橫向版心應允許比直式更寬的行"
        );
    }

    // ---------------- Phase0-6：增量重排（收斂式） ----------------

    /// 版面不變量：starts/pages 同步、嚴格單調、行段落歸屬單調。
    fn assert_layout_invariants(eng: &LayoutEngine, ctx: &str) {
        assert_eq!(
            eng.page_starts().len(),
            eng.page_count(),
            "{ctx}: starts/pages 長度必須一致"
        );
        assert!(
            eng.page_starts().windows(2).all(|w| w[0] < w[1]),
            "{ctx}: 頁起頭位置必須嚴格單調"
        );
        let mut last_para = 0usize;
        for pg in eng.pages() {
            assert!(!pg.lines.is_empty(), "{ctx}: 頁內不可空（空段以單行表達）");
            for l in &pg.lines {
                assert!(l.para_index >= last_para, "{ctx}: 段落歸屬不可倒退");
                last_para = l.para_index;
            }
        }
    }

    /// 正當性錨點：增量重排後的版面必須與「整篇重建」逐頁逐行全等。
    fn assert_matches_full_rebuild(
        eng: &LayoutEngine,
        texts: &[String],
        p: &LayoutParams,
        ctx: &str,
    ) {
        let fresh = LayoutEngine::new(texts, *p);
        assert_eq!(
            eng.page_starts(),
            fresh.page_starts(),
            "{ctx}: 頁起頭與重建不符"
        );
        assert_eq!(eng.pages(), fresh.pages(), "{ctx}: 版面內容與重建不符");
    }

    #[test]
    fn incremental_matches_full_rebuild() {
        // 混合文件：長段跨頁、短段、空段、中英混排。逐輪編輯後全等對照。
        let base: Vec<String> = (0..60)
            .map(|i| match i % 5 {
                0 => format!("第 {i} 段：{}", "混排 content 文字。".repeat(60)),
                1 => format!("短段 {i}"),
                2 => String::new(),
                3 => format!("標點段：「引號——省略號……」{i}"),
                _ => format!("長段 {i}：{}", "字".repeat(900)),
            })
            .collect();
        let p = LayoutParams::default();
        let mut eng = LayoutEngine::new(&base, p);
        assert_layout_invariants(&eng, "初始");
        let mut texts = base.clone();

        // 編輯腳本（確定性）：插入、跨頁灌長、清空、還原、尾段、不存在的段落。
        let edits: Vec<(usize, String)> = vec![
            (0, format!("★{}", base[0])),
            (1, "x".repeat(100_000)), // 單段灌爆幾十頁 → 全域移位
            (1, String::new()),       // 清空 → 頁數回縮
            (41, "改動長段。".to_string()),
            (59, "尾段編輯 mixed 中英".to_string()),
            (0, base[0].clone()), // 還原 → 版面必須回得去
            (2, "填補空段".to_string()),
        ];
        for (i, (para, text)) in edits.iter().enumerate() {
            eng.replace_paragraph_text(*para, text);
            texts[*para] = text.clone();
            assert_layout_invariants(&eng, &format!("第 {i} 筆後"));
            assert_matches_full_rebuild(&eng, &texts, &p, &format!("第 {i} 筆後"));
            assert!(eng.is_settled(), "預設全速模式編輯後必定案");
        }
        // noop 編輯：零事件、版面不動。
        let snapshot = eng.pages().to_vec();
        let evs = eng.replace_paragraph_text(3, &texts[3].clone());
        assert!(evs.is_empty(), "相同文字的重複提交不應回報事件：{evs:?}");
        assert_eq!(eng.pages(), snapshot.as_slice());
    }

    #[test]
    fn incremental_typing_reflow_is_local() {
        // 打字 locality：中段插一個字，事件只覆蓋錨點頁起的髒頁區間——
        // 前面的頁一頁都不准進重繪清單（整篇重排的舊骨架會全報）。
        let texts: Vec<String> = (0..80)
            .map(|i| format!("第 {i} 段：{} mixed words 混排。", "內容".repeat(30)))
            .collect();
        let p = LayoutParams::default();
        let mut eng = LayoutEngine::new(&texts, p);
        let page_of_edit = eng.page_starts().partition_point(|s| *s <= (40, 0, 0)) - 1;
        let evs = eng.replace_paragraph_text(40, &format!("插字：{}", texts[40]));
        let dirty: Vec<usize> = evs
            .iter()
            .filter_map(|e| match e {
                Invalidation::Page(pg) => Some(*pg),
                _ => None,
            })
            .collect();
        assert!(!dirty.is_empty(), "重排必須回報髒頁");
        assert!(
            dirty.first() == Some(&page_of_edit),
            "髒頁區間必從編輯段所在頁起算（實得 {dirty:?}，錨點 {page_of_edit}）"
        );
        assert!(
            dirty.len() < eng.page_count(),
            " locality：髒頁數（{}）必須少於總頁數（{}）",
            dirty.len(),
            eng.page_count()
        );
        // 前段版面原樣保留（同一批 Page 值）。
        let before: Vec<Page> = eng.pages()[..page_of_edit].to_vec();
        assert!(before.iter().enumerate().all(|(i, pg)| pg.index == i));
    }

    #[test]
    fn budgeted_soft_pagination_settles_to_full_rebuild() {
        // 預算制軟分頁：小預算下編輯長段 → ReflowPending（暫缺尾頁）；
        // reflow_step 逐步推進，最終版面與整篇重建全等。
        let texts: Vec<String> = (0..120)
            .map(|i| format!("第 {i} 段：{}", "預算測試內容。".repeat(40)))
            .collect();
        let p = LayoutParams::default();
        let mut eng = LayoutEngine::new(&texts, p);
        eng.set_sync_budget(Some(2));

        // 對第一段灌入超長文：同步路徑必須把編輯段自己排進定案區
        // （cover_para 底線），其後尾段才可暫缺。
        let evs = eng.replace_paragraph_text(0, &"灌爆".repeat(3000));
        assert!(eng.page_starts().len() == eng.page_count());
        if !eng.is_settled() {
            assert!(
                evs.iter()
                    .any(|e| matches!(e, Invalidation::ReflowPending { .. })),
                "未定案必須回報 ReflowPending：{evs:?}"
            );
            // 已定案前段必須包含編輯段的內容（首頁首行就是新文字）。
            assert!(eng.pages()[0].lines[0].text.contains("灌爆"));
            let mut steps = 0usize;
            let mut last_committed = 0usize;
            while !eng.is_settled() {
                let (_, done) = eng.reflow_step();
                steps += 1;
                assert!(steps < 10_000, "reflow 不收斂＝餓死");
                let committed = eng.page_count();
                assert!(
                    committed > last_committed || done,
                    "每步 reflow 必須推進已定案頁數（卡在 {last_committed}）"
                );
                last_committed = committed;
            }
            assert!(
                steps >= 2,
                "預算 2 頁/步：幾十頁尾段至少要排幾步（實得 {steps}）"
            );
        }
        let mut edited = texts.clone();
        edited[0] = "灌爆".repeat(3000);
        assert_matches_full_rebuild(&eng, &edited, &p, "軟分頁定案後");
        assert_layout_invariants(&eng, "軟分頁定案後");
    }

    #[test]
    fn incremental_converges_across_sections() {
        // 跨 section：編輯第一段後，橫向段的舊版面必須被重用（收斂點在
        // section 邊界頁），且換頁行為不受增量影響。
        let base: Vec<String> = (0..30)
            .map(|i| format!("段落 {i}：{} 內容", "文字".repeat(40)))
            .collect();
        let mut sections = vec![
            SectionSpec {
                id: 0,
                geo: letter_geo(),
            };
            20
        ];
        sections.extend(vec![
            SectionSpec {
                id: 1,
                geo: letter_landscape_geo(),
            };
            10
        ]);
        let p = LayoutParams::default();
        let mut eng = LayoutEngine::with_sections(&base, p, sections.clone());
        let land_before: Vec<Page> = eng
            .pages()
            .iter()
            .filter(|pg| (pg.width_pt - 792.0).abs() < f32::EPSILON)
            .cloned()
            .collect();
        assert!(!land_before.is_empty(), "測試前提：必須有橫向頁");

        let mut texts = base.clone();
        for round in 0..4 {
            texts[2] = format!("第 {round} 輪改動 {}", "插".repeat(round * 37));
            eng.replace_paragraph_text(2, &texts[2]);
            assert_layout_invariants(&eng, &format!("跨 section 第 {round} 輪"));
            let fresh = LayoutEngine::with_sections(&texts, p, sections.clone());
            assert_eq!(
                eng.pages(),
                fresh.pages(),
                "跨 section 第 {round} 輪版面與重建不符"
            );
            assert_eq!(eng.page_starts(), fresh.page_starts());
        }
    }

    // ---------------- Phase3-1b：packer 跨頁表格（review r1 閘門 3 補測） ----------------
    //
    // tables.jsonl 的 cross_page 例只鎖 layout 層 `fragment()` 切片原語；
    // 本節直接走公開管線 `paginate_tables`／`LayoutEngine::with_tables`，鎖
    // pack_run 的 FlowItem::Table 分支：cur_tables 蓋章、切片 y 蓋章、跨頁
    // 續排游標、切片後文字行帶（caret 正反向換算）。

    /// 單欄 n 列表格輸入（每列恰 1 帶：一格一行字）。
    fn tall_table_input(rows: usize) -> crate::table::TableInput {
        use crate::table::{CellBlock, CellInput, RowInput, TableInput, WidthSpec};
        TableInput {
            grid_pt: vec![100.0],
            width: Some(WidthSpec::Pt(100.0)),
            rows: (0..rows)
                .map(|i| RowInput {
                    cells: vec![CellInput {
                        blocks: vec![CellBlock::Text(format!("列{i}"))],
                        ..Default::default()
                    }],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// 前段 1＋表格（10 列，佔 10 帶）＋後段 1；頁面只容 7 帶 ⇒ 必然跨頁。
    fn cross_page_fixture() -> (Vec<String>, LayoutParams, Vec<TableSlot>) {
        let p = LayoutParams {
            page_w_pt: 400.0,
            page_h_pt: 200.0,
            margins_pt: [40.0; 4],
            font_size_pt: 12.0,
            line_spacing: 1.4,
        };
        let input = tall_table_input(10);
        let mut texts = vec!["前段".to_string()];
        texts.extend((0..10).map(|i| format!("列{i}")));
        texts.push("後段".to_string());
        let m = TextMeasurer::new(p.font_size_pt);
        let tl = crate::table::layout_table(&input, p.content_w_pt(), p.line_h_pt(), &m, 0);
        let slot = TableSlot {
            para_start: 1,
            para_end: 1 + 10,
            layout: tl,
        };
        (texts, p, vec![slot])
    }

    #[test]
    fn packer_cross_page_table_slices_and_band_handoff() {
        let (texts, p, tables) = cross_page_fixture();
        let pls = layout_paragraphs(&texts, &p);
        let (pages, starts) = paginate_tables(&pls, &p, &[], &tables);
        assert_eq!(pages.len(), 2, "10 帶表＋前段 1 帶＞7 帶容量，必跨頁");
        assert_eq!(starts, vec![(0, 0, 0), (1, 6, 0)], "第二頁起於表格列 6");
        let line_h = p.line_h_pt();

        // (a) 兩頁各一切片、相鄰不重疊：列序連續，切片整高不越版心下緣。
        assert_eq!(pages[0].tables.len(), 1);
        assert_eq!(pages[1].tables.len(), 1);
        let f0 = &pages[0].tables[0].frag;
        let f1 = &pages[1].tables[0].frag;
        assert_eq!((f0.rows.len(), f1.rows.len()), (6, 4));
        let last0 = f0.rows.last().unwrap().row_index;
        assert_eq!(f1.rows[0].row_index, last0 + 1, "切片列必須續排不重不跳");
        // y 蓋章＝上邊界＋帶×行高：頁0 接在前段（帶 0）之後；頁1 從帶 0 起。
        assert!(
            (f0.y_pt - (40.0 + 1.0 * line_h)).abs() < 1e-3,
            "頁0 切片 y 接在前段行後：{}",
            f0.y_pt
        );
        assert!(
            (f1.y_pt - 40.0).abs() < 1e-3,
            "頁1 切片 y 從帶 0 起：{}",
            f1.y_pt
        );
        let bottom0 = f0.y_pt + f0.rows.iter().map(|r| r.height_pt).sum::<f32>();
        assert!(
            bottom0 <= 200.0 - 40.0 + 1e-3,
            "頁0 切片不越版心下緣：{bottom0}"
        );
        // 同頁內文字帶與表格帶互斥（不重疊的另一面）：頁0 只有前段帶 0。
        assert_eq!(pages[0].line_bands, vec![0]);

        // (b) 切片旗標：首片 starts_table+continues，末片兩者皆假。
        assert!(f0.starts_table && f0.continues, "首片：起表且續排");
        assert!(!f1.starts_table && !f1.continues, "末片：非起表且排完");
        assert_eq!(pages[0].tables[0].band, 1);
        assert_eq!(pages[0].tables[0].anchor_para, 1);
        assert_eq!(pages[1].tables[0].anchor_para, 1);

        // (c) 表格之後的文字行帶接在切片末帶之後（caret y 換算的依據）。
        assert_eq!(pages[1].line_bands, vec![4], "後段行帶＝切片末帶 3＋1");
        let eng = LayoutEngine::with_tables(&texts, p, Vec::new(), tables.clone());
        let tail = eng
            .caret_rect(CaretPos {
                para: texts.len() - 1,
                cp: 0,
            })
            .unwrap();
        assert_eq!(tail.page, 1);
        assert!(
            (tail.y_pt - (40.0 + 4.0 * line_h)).abs() < 1e-3,
            "caret y 依帶 4 換算：{}",
            tail.y_pt
        );
        // 反查：點後段行的 caret rect 位置命中後段段首；點表格帶不回文字行。
        let back = eng
            .caret_at(tail.page, tail.x_pt, tail.y_pt + 6.0)
            .expect("後段行可命中");
        assert_eq!((back.para, back.cp), (texts.len() - 1, 0));
        assert!(
            eng.caret_at(0, 50.0, 40.0 + 1.0 * line_h + 6.0).is_none(),
            "表格帶上無文字行，反查必 None（v1 表格內 caret 不支援）"
        );
    }

    #[test]
    fn packer_cross_page_nested_container_emits_window_only() {
        // t_f5d47469 根因 5 packer 級鎖：外層容器表（單格＝段前＋子表＋段後）
        // 走 paginate_tables 真管線跨頁時，第二頁不得重畫子表頭列，子表切片
        // 必須按頁視窗接排。頁面容 7 帶：段前1＋子表帶1..6＋子表帶6續頁。
        use crate::table::{CellBlock, CellInput, RowInput, TableInput, WidthSpec};
        let p = LayoutParams {
            page_w_pt: 400.0,
            page_h_pt: 200.0,
            margins_pt: [40.0; 4],
            font_size_pt: 12.0,
            line_spacing: 1.4,
        };
        let inner = TableInput {
            grid_pt: vec![80.0],
            width: Some(WidthSpec::Pt(80.0)),
            rows: (0..6)
                .map(|i| RowInput {
                    cells: vec![CellInput {
                        blocks: vec![CellBlock::Text(format!("內{i}"))],
                        ..Default::default()
                    }],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let input = TableInput {
            grid_pt: vec![200.0],
            width: Some(WidthSpec::Pt(200.0)),
            rows: vec![RowInput {
                cells: vec![CellInput {
                    blocks: vec![
                        CellBlock::Text("段前".into()),
                        CellBlock::Table(Box::new(inner)),
                        CellBlock::Text("段後".into()),
                    ],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut texts = vec!["段前".to_string()];
        texts.extend((0..6).map(|i| format!("內{i}")));
        texts.push("段後".to_string());
        let m = TextMeasurer::new(p.font_size_pt);
        let tl = crate::table::layout_table(&input, p.content_w_pt(), p.line_h_pt(), &m, 0);
        assert_eq!(tl.total_bands, 8, "1＋6＋1 流式累加");
        let slot = TableSlot {
            para_start: 0,
            para_end: 8,
            layout: tl,
        };
        let pls = layout_paragraphs(&texts, &p);
        let (pages, _starts) = paginate_tables(&pls, &p, &[], &[slot]);
        assert!(pages.len() >= 2, "8 帶＞7 帶容量，必跨頁");
        let collect_inner = |pg: &Page| -> Vec<String> {
            pg.tables
                .iter()
                .flat_map(|tp| {
                    tp.frag.rows.iter().flat_map(|r| {
                        r.cells.iter().flat_map(|c| {
                            c.nested.iter().flat_map(|(_, nf)| {
                                nf.rows.iter().flat_map(|nr| {
                                    nr.cells[0]
                                        .texts
                                        .iter()
                                        .map(|t| t.text.clone())
                                        .collect::<Vec<_>>()
                                })
                            })
                        })
                    })
                })
                .collect()
        };
        let p0_inner = collect_inner(&pages[0]);
        let p1_inner = collect_inner(&pages[1]);
        assert!(p0_inner.contains(&"內0".to_string()), "第 1 頁必有子表頭列");
        assert!(
            !p1_inner.contains(&"內0".to_string()),
            "第 2 頁重畫子表頭列＝根因 5 復發：{p1_inner:?}"
        );
        // 無縫接排：兩頁子表列合起來恰為 0..5 各一次。
        let mut joined = p0_inner;
        joined.extend(p1_inner);
        joined.sort();
        let want: Vec<String> = (0..6).map(|i| format!("內{i}")).collect();
        assert_eq!(joined, want, "子表列跨頁不重不跳");
    }

    #[test]
    fn incremental_with_tables_matches_full_rebuild() {
        // 表格存在下打字：增量 packer（FlowItem::Table 分支＋收斂重用）與
        // 整篇重建逐頁全等（含 TablePlacement 切片、line_bands、starts）。
        let (texts, p, tables) = cross_page_fixture();
        let mut eng = LayoutEngine::with_tables(&texts, p, Vec::new(), tables.clone());
        assert_eq!(eng.page_count(), 2);

        // 表格內部段落（流失權）：編輯必 noop——版面零變動。
        let snapshot = eng.pages().to_vec();
        let evs = eng.replace_paragraph_text(3, "表格內改字");
        assert!(evs.is_empty(), "流失權段改了不動版面：{evs:?}");
        assert_eq!(eng.pages(), snapshot.as_slice());

        let edits: Vec<(usize, String)> = vec![
            (
                texts.len() - 1,
                "後段變長：".to_string() + &"字".repeat(200),
            ),
            (0, "前段灌長：".to_string() + &"字".repeat(300)), // 表格整體被推頁
            (0, "前段".to_string()),                           // 還原 → 版面回得去
            (texts.len() - 1, "後段".to_string()),
        ];
        for (i, (para, text)) in edits.iter().enumerate() {
            let mut want_texts = eng.texts.clone();
            want_texts[*para] = text.clone();
            let evs = eng.replace_paragraph_text(*para, text);
            assert!(
                evs.iter().any(|e| matches!(e, Invalidation::Page(_))),
                "第 {i} 筆編輯必須回報髒頁：{evs:?}"
            );
            let fresh = LayoutEngine::with_tables(&want_texts, p, Vec::new(), tables.clone());
            assert_eq!(
                eng.pages(),
                fresh.pages(),
                "表格增量第 {i} 筆版面與重建不符"
            );
            assert_eq!(eng.page_starts(), fresh.page_starts());
        }
        // 最終版面仍含跨頁切片（兩頁以上各一 TablePlacement）。
        assert!(
            eng.pages()
                .iter()
                .filter(|pg| !pg.tables.is_empty())
                .count()
                >= 2
        );
    }

    #[test]
    fn caret_respects_per_page_margins_across_sections() {
        // caret 座標取「此頁所屬 section」的邊界：直式頁左緣 72pt、
        // 橫向頁（此處故意 90pt）取自身邊界——跨 section 後不再用全域 margin。
        let mut land = letter_landscape_geo();
        land.margin_left_pt = 90.0;
        let texts = vec!["甲段".repeat(30), "乙段".repeat(30)];
        let sections = vec![
            SectionSpec {
                id: 0,
                geo: letter_geo(),
            },
            SectionSpec { id: 1, geo: land },
        ];
        let eng = LayoutEngine::with_sections(&texts, LayoutParams::default(), sections);
        let r1 = eng.caret_rect(CaretPos { para: 1, cp: 0 }).unwrap();
        let pg1 = &eng.pages()[r1.page];
        assert!(
            (r1.x_pt - pg1.margin_left_pt).abs() < f32::EPSILON,
            "caret x 起於此頁邊界"
        );
        assert!(
            (r1.y_pt - pg1.margin_top_pt).abs() < f32::EPSILON,
            "段首 caret y 起於此頁上緣邊界"
        );
    }
}
