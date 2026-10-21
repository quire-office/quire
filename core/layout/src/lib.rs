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
///
/// 取捨：`Vec` 欄位（逐頁預留表）使本 struct **不再 `Copy`**（僅 `Clone`）——
/// 逐頁預留是「每頁一個值」的表，值數隨文件長度變動，`Copy` 不再成立。
/// 呼叫端原本靠 `Copy` 隱式複製的地方改為顯式 `.clone()`（值語意不變）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutParams {
    pub page_w_pt: f32,
    pub page_h_pt: f32,
    /// 四邊邊界 [上, 右, 下, 左]（pt）。sectPr 的 pgMar 各邊可不同，單一 margin_pt
    /// 無法如實表達（Letter 文件 Word 預設左右 1.25 吋 ≠ 上下 1 吋）。
    pub margins_pt: [f32; 4],
    pub font_size_pt: f32,
    pub line_spacing: f32,
    /// 頁底預留高（pt）：正文可用區底部要讓給頁底註腳區的空間。
    /// 0＝無預留（現況行為，逐位元組零變化）；負值／NaN 一律視同 0（防呆）。
    ///
    /// 出處：Word 把正文區（text body）壓縮給頁底註腳區讓位（ECMA-376
    /// §17.11 註腳 ＋ §17.6.10 頁邊距同層），正文可用高變矮、分頁隨之前移。
    /// 取捨：本卡只做**機制**（分頁參數化）——「每頁該預留多少」由引擎端
    /// 依實際註腳量算出再灌進本欄（下一張卡 F2b 接線）。既有構造語義一律
    /// 不變：`Default` 與 `from_section` 皆給 0，故全倉庫渲染輸出零變化。
    pub footnote_reserve_pt: f32,
    /// 逐頁頁底預留（pt，索引＝頁序）：超出表長的頁視同 0。
    /// 空表＝無逐頁預留（現況零變化）。與 footnote_reserve_pt（文件層級
    /// 基底）疊加取 max（兩者語義：基底給「全頁統預留」場景，逐頁表給
    /// 「特定頁有註腳」場景；疊加口徑寫進欄注＋測死）。
    ///
    /// 出處：Word 的頁底註腳區是**逐頁**成立的——只有該頁真的排到註腳的
    /// 那一頁才把正文區壓縮，別頁不受影響（ECMA-376 §17.11）。
    /// 取捨：逐頁表＝索引式（頁序→預留高），引擎端算好哪頁有註腳就灌哪頁；
    /// 缺項＝0（視同無預留），多項＝多餘的頁序忽略（不 panic）。
    /// 負值／NaN 元素視同 0（防呆，與 `footnote_reserve_pt` 同款口徑）。
    ///
    /// serde default：舊快照（無此欄）反序列化時得空表＝舊行為。
    #[serde(default)]
    pub footnote_reserves_pt: Vec<f32>,
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
            // 頁底預留恆 0：預留是**文件層級**旋鈕，由上層算好後灌進 params
            // （F2b 接線）；`from_section` 只負責頁面幾何，既有構造語義不變。
            footnote_reserve_pt: 0.0,
            // 逐頁預留恆空：逐頁表由引擎端按「哪頁有註腳」灌入
            // （F2b 接線）；`from_section` 只負責頁面幾何，既有構造語義不變。
            footnote_reserves_pt: Vec::new(),
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
    ///
    /// 頁底預留（`footnote_reserve_pt`）自版心底部扣除：Word 讓正文區壓縮給
    /// 頁底註腳，正文可用高變矮，分頁容量隨之減少。
    ///
    /// **逐頁預留表（`footnote_reserves_pt`）不經本方法**——逐頁預留是「每頁
    /// 各算各的」，由 `section_capacity`（帶頁序）逐頁結算後才決定容量。
    ///
    /// 零變化守門：**預留 0 時逐位元組走舊算式**（`x - 0.0` 恆等於 `x`，但
    /// 這裡直接跳過減法，舊版算式原封不動），故渲染輸出零位移。
    pub fn per_page_lines(&self) -> usize {
        let line_h = self.line_h_pt();
        // 預留正規化：負值／NaN 一律視同 0（防呆）。NaN 與任何值比較皆 false，
        // 故先判 isnan 再判正負——順序反了會讓 NaN 走進比較分支。
        let reserve = reserve_or_zero(self.footnote_reserve_pt);
        // 舊版算式（預留 0 時逐位元組等價）：版心高＝頁高−上邊界−下邊界。
        let mut content_h = self.page_h_pt - self.margin(MARGIN_TOP) - self.margin(MARGIN_BOTTOM);
        if reserve > 0.0 {
            content_h -= reserve;
        }
        // 兜底順序：扣預留 → 以 line_h 夾底（版心至少容一行）→ 量化後 `.max(1)`。
        // 病態預留（超過可用高）時版心被夾到 1 帶，容量恆 ≥1，絕不 panic、絕不為 0。
        let content_h = content_h.max(line_h);
        ((content_h / line_h).floor() as usize).max(1)
    }
}

/// 預留量正規化：負值／NaN 一律視同 0（防呆）。NaN 與任何值比較皆 false，
/// 故先判 isnan 再判正負——順序反了會讓 NaN 走進比較分支。
///
/// 文件層級（`footnote_reserve_pt`）與逐頁表（`footnote_reserves_pt`）共用
/// 此口徑：兩者語義同源（頁底註腳區要留多高），防呆規則必須一致。
fn reserve_or_zero(v: f32) -> f32 {
    if v.is_nan() || v <= 0.0 {
        0.0
    } else {
        v
    }
}

/// 帶量化上限（t_c1fb4c69 S1-4）：`(h/line_h).ceil() as u32` 遇病態高度
/// （f32::MAX／inf／垃圾 trHeight）會衝進 u32::MAX——列帶累加溢出 panic、
/// 切片與分頁迴圈被天量帶數拖垮。量化值一律鉗到此上限（4096 帶 ≈ 68k pt
/// ＝正常檔案永遠到不了的規模，鉗了不影響任何真實版面）。源頭守衛在
/// ooxml 轉換層（值域拒絕），此處是 layout/engine 側的第二道閘。
pub const MAX_PAGE_BANDS: u32 = 4096;

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
            // 頁底預留：後備值無註腳預留需求＝0（現況行為，逐位元組零變化）。
            footnote_reserve_pt: 0.0,
            // 逐頁預留：後備值無逐頁預留需求＝空表（現況行為，逐位元組零變化）。
            footnote_reserves_pt: Vec::new(),
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
    /// 段內強制分頁斷點（`w:br w:type="page"`）：值＝**斷行之後**的軟斷行
    /// 行號（斷點前的文字留在 `lines[o-1]`；`o=0` 斷在段落之前）。由
    /// `break_para` 依 br 的字元偏移對齊既有 `Line::char_offset` 求得；
    /// 偏移落在行內時向上取整到該行之後（誠實近似：斷行層不插硬斷，
    /// 硬語義——斷點前文字留原頁、其後開新頁——由分頁層執行）。
    /// serde default 保持舊快照相容；無 br 段落恆空＝舊行為逐位元組不變。
    #[serde(default)]
    pub para_breaks: Vec<usize>,
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
    /// 排在此頁的圖片佔位（t_a1397522；座標口徑同 line_bands 行網格）。
    #[serde(default)]
    pub images: Vec<PageImage>,
    /// 錨定段落落在本頁的浮動圖片（t_a1397522；不佔行網格，座標＝頁內
    /// 絕對。分頁完成後由 `attach_floating_images` 附掛——浮動圖不參與
    /// pack_run 掃描，只在錨定段落所在頁發射佔位 Rect）。
    #[serde(default)]
    pub floating_images: Vec<PageFloatingImage>,
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

/// 帶**強制斷點**的斷行：`forced_cp` 為段落文字中的碼點偏移（同
/// `Line::char_offset` 標尺），每個偏移處必斷行——這是 `w:br w:type="page"`
/// 段中硬斷的承載點（KP 在此收行＝硬邊界，其後文字必然另起一行；分頁層
/// 再把該行邊界升格為頁邊界）。偏移會先折到最近的字元邊界，重複自動去重；
/// `0` 與段尾偏移由呼叫端（ParagraphLayout::para_breaks→pack_run）處理，
/// 此處只影響斷行形狀。空清單與 `break_lines` 逐位元組等價（golden 鐵律）。
pub fn break_lines_at(text: &str, max_w: f32, m: &TextMeasurer, forced_cp: &[usize]) -> Vec<Line> {
    kp::kp_break_lines_at(text, max_w, m, forced_cp)
        .into_iter()
        .map(|s| s.line)
        .collect()
}

pub fn layout_paragraphs(texts: &[String], p: &LayoutParams) -> Vec<ParagraphLayout> {
    layout_paragraphs_with_breaks(texts, p, &[], &[])
}

/// 逐段落斷行；每段按其**所屬 section**（與 `texts` 等長，engine 層餵入）的
/// 版心寬斷行。`sections` 為空＝全文件單一幾何（退回 `p` 的頁面欄位）。
pub fn layout_paragraphs_per_section(
    texts: &[String],
    p: &LayoutParams,
    sections: &[SectionSpec],
) -> Vec<ParagraphLayout> {
    layout_paragraphs_with_breaks(texts, p, sections, &[])
}

/// 逐段落斷行＋**強制斷點**（`w:br w:type="page"`，碼點偏移逐段清單，短＝無
/// 斷點段落；與 `texts` 同位）。段中斷點走 `break_lines_at`（KP 硬收行），
/// 段首/段尾斷點由 `para_breaks` 承載給分頁層。偏移越界＝鉗到段尾（誠實）。
pub fn layout_paragraphs_with_breaks(
    texts: &[String],
    p: &LayoutParams,
    sections: &[SectionSpec],
    breaks: &[Vec<usize>],
) -> Vec<ParagraphLayout> {
    let m = TextMeasurer::new(p.font_size_pt);
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let content_w = section_params(p, sections, i).content_w_pt();
            let (lines, para_breaks) = break_para_lines(
                t,
                content_w,
                &m,
                breaks.get(i).map(Vec::as_slice).unwrap_or(&[]),
            );
            let mut pl = ParagraphLayout {
                para_index: i,
                lines,
                para_breaks,
            };
            // break_lines 不認識段落索引；在此蓋章，讓分頁搬運行時段落歸屬不丟。
            for l in pl.lines.iter_mut() {
                l.para_index = i;
            }
            pl
        })
        .collect()
}

/// 段落文字 → (斷行行序列, para_breaks 行號)。`brs` 為 br-page 碼點偏移：
/// 段中偏移折入 `break_lines_at`（斷行必在該處收行），再把每個偏移折算成
/// 「斷行之後的行號 k」（斷點＝`lines[..k]` 收尾、`lines[k..]` 續排）：
/// `o=0`→k=0（段前斷）、`o≥段長`→k=行數（段後斷）、段中取 `char_offset ≥ o`
/// 的首行下標（硬收行使之一對齊；未對齊時向上取整，斷點前文字留在原行）。
/// **零文字段（獨立 br-page 段，corpus 高頻）**：偏移 0 屬空行之後 →
/// k=行數（段後斷），與 LO 實測同形（LO 對照 s1/s2/s3：空行留原頁、其後
/// 換頁；文件首 br 不再整處消失，t_c1fb4c69 S1-2）。同位重複 br 仍 `dedup`
/// 合併——LO 實測同 run 連發雙 br＝一處斷點不疊空白頁（s4/s5 裁決鎖）。
pub(crate) fn break_para_lines(
    text: &str,
    content_w: f32,
    m: &TextMeasurer,
    brs: &[usize],
) -> (Vec<Line>, Vec<usize>) {
    let total = text.chars().count();
    let inner: Vec<usize> = brs
        .iter()
        .copied()
        .filter(|&o| o > 0 && o < total)
        .collect();
    let lines = if inner.is_empty() {
        break_lines(text, content_w, m)
    } else {
        break_lines_at(text, content_w, m, &inner)
    };
    let mut para_breaks: Vec<usize> = brs
        .iter()
        .map(|&o| {
            if total == 0 || o >= total {
                // 零文字段：唯一的空行就是 br 之前的全部內容 → 斷點在段後。
                lines.len()
            } else if o == 0 {
                0
            } else {
                lines.partition_point(|l| l.char_offset < o)
            }
        })
        .collect();
    para_breaks.sort_unstable();
    para_breaks.dedup();
    (lines, para_breaks)
}

/// 逐段落斷行（可帶 per-paragraph 字級網格；t_5dfc8ac7）。`grid` 與 `texts`
/// 等長時逐段取 `(字級pt, 行高pt)` 的字級量測斷行；缺位/短＝退回 `p` 全域字級
/// （既有行為逐位元組不變）。頁首頁尾路徑用（正文走 `LayoutEngine` 內部網格）。
/// br-page 不介入頁首頁尾（hdr/ftr 內 br 屬表格跨頁卡範圍）：`para_breaks` 空。
pub fn layout_paragraphs_grid(
    texts: &[String],
    p: &LayoutParams,
    sections: &[SectionSpec],
    grid: &[(f32, f32)],
) -> Vec<ParagraphLayout> {
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let size = grid.get(i).map_or(p.font_size_pt, |g| g.0);
            let content_w = section_params(p, sections, i).content_w_pt();
            let m = TextMeasurer::new(size);
            let mut pl = ParagraphLayout {
                para_index: i,
                lines: break_lines(t, content_w, &m),
                para_breaks: Vec::new(),
            };
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
        None => p.clone(),
    }
}

/// 逐 section 的每頁容量（帶）：頁面幾何取該頁所屬 section，字級/行距與
/// **頁底預留**取文件層級 params ＋ 逐頁預留表。
///
/// `from_section` 恆給 0 預留（既有構造語義不變）；預留是**文件層級**旋鈕——
/// 上層（引擎端依實際註腳量算「每頁該留多少」＝F2b 註腳接線）灌進 params，
/// 此處再把它帶進「逐 section 幾何」的容量計算。預留 0 時兩者完全同值。
///
/// `page_index`＝**該頁的頁序**（0 起算）：逐頁預留表（`footnote_reserves_pt`）
/// 以頁序索引——Word 的頁底註腳區是逐頁成立的，只有排到註腳的那一頁才壓
/// 正文區。缺項＝0（該頁無預留）、多餘項忽略。
///
/// 零變化守門：**逐頁表空時直接走舊單值路徑**（原樣帶入文件層級預留，
/// 不做逐頁查表、也不做 max 運算），故渲染輸出逐位元組零變化。
/// 該頁容量（帶數）：以該 section 的頁面幾何＋文件層級字級/行距/預留算。
/// 公開（F2b-2：engine 要按「未扣預留的全容量」算版心底，故需本函式）。
pub fn section_capacity(p: &LayoutParams, geo: &SectionGeometry, page_index: usize) -> usize {
    let mut sp = LayoutParams::from_section(geo, p.font_size_pt, p.line_spacing);
    sp.footnote_reserve_pt = if p.footnote_reserves_pt.is_empty() {
        p.footnote_reserve_pt
    } else {
        effective_reserve_pt(p, page_index)
    };
    sp.per_page_lines()
}

/// 逐頁預留（pt）：索引＝頁序，超出表長的頁視同 0；負值／NaN 元素視同 0。
fn page_reserve_pt(p: &LayoutParams, page_index: usize) -> f32 {
    p.footnote_reserves_pt
        .get(page_index)
        .map(|v| reserve_or_zero(*v))
        .unwrap_or(0.0)
}

/// 該頁的實際底部預留（pt）＝**max**（文件層級基底, 逐頁表）。
///
/// 疊加口徑（測死）：取 max 而非相加——兩者是同一塊「頁底註腳區」的兩種
/// 描述（基底＝全頁統預留、逐頁＝特定頁額外預留），相加會把同一塊區域
/// 算兩次。正規化後再取 max（負值／NaN 視同 0，防呆口徑同款）。
fn effective_reserve_pt(p: &LayoutParams, page_index: usize) -> f32 {
    reserve_or_zero(p.footnote_reserve_pt).max(page_reserve_pt(p, page_index))
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
/// `Image`（t_a1397522）：錨定段落之後的圖片佔位塊（atomic，整塊排放）；
/// `img` ＝ engine 餵入之 `images` 陣列索引（build_flow 只為 inline／退化
/// anchor 開項；浮動帶鎖點者不進流——只在錨定段落所在頁發射鎖點 Rect）。
#[derive(Debug, Clone, PartialEq)]
pub enum FlowItem {
    Text { para: usize },
    Table { slot: usize },
    Image { img: usize },
}

/// 逐段落字級/行帶表（t_5dfc8ac7）：engine 由樣式級聯算出餵入（段落級有效
/// 字級→斷行量測尺；段行高÷文件帶尺向上取整→該段每行佔用帶數）。
/// **兩表皆空＝全域 params 尺**——既有呼叫端（goldens/測試/無樣式宣告文件）
/// 行為逐位元組不變。缺位/短＝該段退回全域值（容錯，不虛構）。
#[derive(Debug, Clone, Default)]
pub struct ParaGrid {
    /// 逐段斷行量測字級（pt）；缺＝`params.font_size_pt`。
    pub size_pt: Vec<f32>,
    /// 逐段每行佔用帶數（engine 算 ceil(段行高/文件帶尺)，至少 1）；缺＝1。
    pub bands: Vec<u32>,
    /// 逐段顯式分頁旗標（`w:br type="page"`／`w:pageBreakBefore`；t_5dfc8ac7）：
    /// true＝該段錨點的流項前強制換頁（Word 語義；段內 br 粗化為整段推頁）。
    /// 缺＝false（全域無宣告＝舊行為）。純 (flow,fi) 函數——增量重排復入同樣成立。
    pub break_before: Vec<bool>,
}

impl ParaGrid {
    /// 段落 i 的斷行字級（缺＝全域 fallback）。
    pub fn size_of(&self, para: usize, fallback: f32) -> f32 {
        self.size_pt.get(para).copied().unwrap_or(fallback)
    }

    /// 段落 i 前是否強制換頁（缺＝false）。
    pub fn break_before_of(&self, para: usize) -> bool {
        self.break_before.get(para).copied().unwrap_or(false)
    }

    /// 段落 i 每行佔用帶數（缺＝1；恆 ≥1，行至少佔一格）。
    pub fn bands_of(&self, para: usize) -> u32 {
        self.bands.get(para).copied().unwrap_or(1).max(1)
    }
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

/// 正文流圖片佔位錨點（t_a1397522）：engine 由 `w:drawing` 投影餵入（僅
/// inline／退化 anchor；浮動帶鎖點者另走 `floating`，格內圖走 CellBlock）。
/// 圖片是 atomic block：不參與斷行，隨錨定段落末行之後整塊排放，佔
/// `bands` 個行網格帶（ceil(h/line_h)，至少 1）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageBand {
    /// 錨定段落（＝圖形所在的扁平段落索引）。
    pub para: usize,
    /// 寬（pt，繪製用）。
    pub w_pt: f32,
    /// 高（pt，繪製用；＝min(extent 高, bands×line_h) 由呼叫端定，此處原值）。
    pub h_pt: f32,
    /// 佔用帶數。
    pub bands: u32,
    /// 浮動鎖點（Some＝不佔帶、不查容量，只記鎖點座標＋原點系；
    /// None＝inline 排帶）。
    pub anchor: Option<FloatingAnchor>,
}

/// 鎖點原點系（ECMA-376 `wp:positionX/@relativeFrom` 的三檔近似分類）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OriginFrame {
    /// (x,y) 以頁左上為原點（page／pageMargin）。
    Page,
    /// (x,y) 以版心左上為原點（column／margin；單欄近似）。
    Margin,
    /// (x,y) 以錨定段落帶左上為原點（paragraph／line／character／未知）。
    Paragraph,
}

/// `quire_ooxml::origin_of` 的穩定字串 → 本 crate 原點系（分層：字串是
/// 兩 crate 間唯一契約，engine 層只做搬運）。
pub fn origin_frame(kind: &str) -> OriginFrame {
    match kind {
        "page" => OriginFrame::Page,
        "margin" => OriginFrame::Margin,
        _ => OriginFrame::Paragraph,
    }
}

/// 浮動圖片鎖點（座標 pt；x/y 各自帶原點系——兩軸 relativeFrom 可不同）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FloatingAnchor {
    pub x_pt: f32,
    pub y_pt: f32,
    pub x_frame: OriginFrame,
    pub y_frame: OriginFrame,
}

/// 頁面上的圖片佔位放置（pack_run flush 時蓋章；座標＝版內相对，
/// y 已是行網格帶索引，繪製端 y_pt＝margin_top＋band×line_h）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageImage {
    pub para: usize,
    pub band: u32,
    pub w_pt: f32,
    pub h_pt: f32,
}

/// 頁面上的浮動圖片鎖點（不佔行網格；座標＝頁絕對 pt，發射前已按各軸
/// `relativeFrom` 原點系換算完成）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageFloatingImage {
    pub x_pt: f32,
    pub y_pt: f32,
    pub w_pt: f32,
    pub h_pt: f32,
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
    /// 當前頁累積的圖片佔位（t_a1397522；flush 時移交 Page.images）。
    cur_images: Vec<PageImage>,
    /// 當前頁已佔行網格帶數（文字行＋表格帶）。
    cur_used: u32,
    cur_spec: Option<SectionSpec>,
    /// 本頁**頁首項**的 section（t_c1fb4c69 S1-1）：continuous 同頁混節時
    /// 末項會覆寫 cur_spec——頁的 section 必須按頁首項蓋章（titlePg／頁首
    /// 選擇以此為準）。fresh_page flush 後重置，下一頁首項重蓋。
    page_spec: Option<SectionSpec>,
    page_start: FlowPos,
}

impl PackState {
    fn fresh() -> Self {
        Self {
            cursor: (0, 0, 0),
            cur: Vec::new(),
            cur_bands: Vec::new(),
            cur_tables: Vec::new(),
            cur_images: Vec::new(),
            cur_used: 0,
            cur_spec: None,
            page_spec: None,
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
/// （表格排在此段之前，沿用該段所處 section 的頁面幾何）；圖片項＝其
/// 錨定段落（沿用該段 section 幾何）。
fn item_para(flow: &[FlowItem], tables: &[TableSlot], images: &[ImageBand], fi: usize) -> usize {
    match &flow[fi] {
        FlowItem::Text { para } => *para,
        FlowItem::Table { slot } => tables[*slot].para_start,
        FlowItem::Image { img } => images[*img].para,
    }
}

/// br-page 偏移隨段落改文平移（碼點標尺，undo 快照同鐵律）：新舊文取公共
/// 前後綴定出舊改區 `[lo, hi_old)`——斷點 `b ≤ lo`（改區前）原樣、`b ≥ hi_old`
/// （改區後）平移 `hi_new − hi_old`、`lo < b < hi_old`（改區內，文字被蓋掉）
/// 折到 `lo`。越界鉗到段長，排序去重。改文永不無腦丟使用者的手插硬斷點。
fn shift_breaks(old: &str, new: &str, brs: &[usize]) -> Vec<usize> {
    let o: Vec<char> = old.chars().collect();
    let n: Vec<char> = new.chars().collect();
    let lo = o.iter().zip(n.iter()).take_while(|(a, b)| a == b).count();
    let max_suf = (o.len() - lo).min(n.len() - lo);
    let mut suf = 0usize;
    while suf < max_suf && o[o.len() - 1 - suf] == n[n.len() - 1 - suf] {
        suf += 1;
    }
    let hi_old = o.len() - suf;
    let delta = (n.len() - suf) as isize - hi_old as isize;
    let mut out: Vec<usize> = brs
        .iter()
        .map(|&b| {
            if b <= lo {
                // 改區前（含恰在插入點）：斷點緊貼其前文字 → 原樣。
                b
            } else if b >= hi_old {
                // 改區後（嚴格越過插入點）：隨插入/刪除量平移。
                (b as isize + delta).max(lo as isize) as usize
            } else {
                // 改區內（文字被蓋掉）：折到改區首，絕不無腦丟斷點。
                lo
            }
        })
        .map(|b| b.min(n.len()))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// 前一個流項是否以「段尾 br-page」收尾（k＝行數：最後一行已放置，硬斷點
/// 落在該段之後 → 下一個流項必須開新頁）。游標位於流項起頭 (fi, 0, 0) 時
/// 查此項；斷點行號 k 的語義＝「在 lines[k-1] 之後、lines[k] 之前」。
/// 行內圖項回溯到所屬段落再判（t_c1fb4c69 S1-3）：段尾 br 的斷點在**圖之後**
/// （圖是段落的行內內容，斷點屬圖後）——下一流項仍是同段圖時延後命中，
/// 圖項前不推頁（否則圖與段落被拆到兩頁）。
fn prev_item_breaks_after(
    flow: &[FlowItem],
    pls: &[&ParagraphLayout],
    images: &[ImageBand],
    fi_prev: usize,
) -> bool {
    let para = match &flow[fi_prev] {
        FlowItem::Text { para } => *para,
        FlowItem::Image { img } => images[*img].para,
        _ => return false,
    };
    // fi_prev 必須是該段的最後一個流項：下一項仍是同段圖 → 斷點延到圖後。
    if let Some(FlowItem::Image { img }) = flow.get(fi_prev + 1) {
        if images[*img].para == para {
            return false;
        }
    }
    pls.get(para)
        .is_some_and(|pl| pl.para_breaks.last().copied() == Some(pl.lines.len()))
}

/// 流排放主幹（`paginate_per_section` 與增量重排共用同一台狀態機——分頁
/// 語義只有一份實作，golden 與增量結果不可能分岔）。
///
/// 每步對下一個流項單位算 `start_new`（section 邊界非 continuous、行網格
/// 容量已滿、或游標恰落在 br-page 硬斷點上），成立則 flush 當前頁。文字項
/// 一次放一條行（1 帶）；表格項放「至多剩餘容量」的切片（`TableLayout::fragment`，
/// 切片可跨頁續排），並就地把切片 y 蓋章成頁內絕對座標（行網格帶 × 行高＋
/// 上邊界）。無表格文件的 FlowPos 序列與舊 (段落, 行) 模型一一對應——golden
/// 逐位元組不變。
///
/// 硬斷點判定是游標位置的純函數（不另存游標態）：斷點 k 只在游標
/// `(fi, k, 0)` 處命中（段前 k=0、段中 0<k<nli），段尾 k=nli 折疊為「游標在
/// 下一流項起頭且前一文字項以 k=nli 收尾」。增量重排從舊頁邊界復位時會
/// 重遇同一斷點位置，此刻 cur 恆空 ⇒ flush 為 no-op，冪等不破。
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
    images: &[ImageBand],
    p: &LayoutParams,
    sections: &[SectionSpec],
    grid: &ParaGrid,
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
        if st.cur.is_empty() && st.cur_tables.is_empty() && st.cur_images.is_empty() {
            return;
        }
        // 頁幾何/身分別取**頁首項**的 section（t_c1fb4c69 S1-1）：continuous
        // 同頁混節時 cur_spec 已被末項覆寫，直接拿它蓋章會讓頁首/頁尾選擇
        // （titlePg 依 section_id）錯判成末節。
        let s = st.page_spec.unwrap_or_else(|| st.cur_spec.unwrap());
        out.push(Page {
            index: out.len(),
            lines: std::mem::take(&mut st.cur),
            line_bands: std::mem::take(&mut st.cur_bands),
            tables: std::mem::take(&mut st.cur_tables),
            images: std::mem::take(&mut st.cur_images),
            floating_images: Vec::new(),
            width_pt: s.geo.page_w_pt,
            height_pt: s.geo.page_h_pt,
            margin_left_pt: s.geo.margin_left_pt,
            margin_top_pt: s.geo.margin_top_pt,
            section_id: s.id,
        });
        out_starts.push(st.page_start);
        st.page_start = st.cursor;
        st.cur_used = 0;
        st.page_spec = None;
    };

    while st.cursor.0 < flow.len() {
        let (fi, li, band) = st.cursor;
        let anchor = item_para(flow, tables, images, fi);
        let spec = effective_section(p, sections, anchor);
        // 當前頁序＝已 flush 的頁數（`fresh_page` 以 `out.len()` 為頁序落章，
        // 故正在組裝的這一頁其序號就是 `out.len()`）。逐頁預留表以頁序索引，
        // 故每頁的容量各自獨立（Word 的頁底註腳區逐頁成立）。
        let cap = section_capacity(p, &spec.geo, out.len());
        // 圖片項（t_a1397522）：atomic block——本頁放不下整塊就整塊推到下一頁
        // （Word 同款）。bands>cap 的巨型圖只能頂格放（cur_used==0 時不換頁，
        // 容許溢出——否則無限換頁）。
        // 純圖段落吸收（卡片帶數公式：段落總佔位＝ceil(h/line_h)）：錨定段落
        // 只有空行＋圖時，空行不另佔帶——Text 項以整段 bands 原子檢查（段落
        // ＋圖一起推頁，Word 同款），Image 項扣除共用的空行帶，否則二次換頁
        // 會把空行孤留在前一頁。判定全走靜態輸入（flow 前項＋pls 現況），增量重排從任意游標復入都
        // 同樣成立。游標頁尾行先拷出（避免與 st.cur 的可變借用打架）。
        let last_line_para = st.cur.last().map(|l| l.para_index);
        let absorb_slot = |img: usize, fi: usize| -> bool {
            let im = &images[img];
            im.anchor.is_none()
                && fi > 0
                && matches!(&flow[fi - 1], FlowItem::Text { para } if *para == im.para)
                && last_line_para == Some(im.para)
                && pls
                    .get(im.para)
                    .is_some_and(|pl| pl.lines.len() == 1 && pl.lines[0].text.trim().is_empty())
        };
        // 段落佔帶（t_5dfc8ac7）：段落行按段行高量化（ceil(行高/帶尺)，至少 1）。
        // 全域尺（ParaGrid 空）時恆 1——既有行為逐位元組不變。
        let para_band = |para: usize| grid.bands_of(para);
        let para_img_bands = |fi: usize| -> u32 {
            // Text 項前視：純圖段落（唯一空行＋下一流項即其流內圖）→ 整段帶數。
            let FlowItem::Text { para } = &flow[fi] else {
                return 0;
            };
            let Some(pl) = pls.get(*para) else {
                return 0;
            };
            if pl.lines.len() != 1 || !pl.lines[0].text.trim().is_empty() {
                return 0;
            }
            let Some(FlowItem::Image { img }) = flow.get(fi + 1) else {
                return 0;
            };
            if images[*img].anchor.is_some() {
                return 0; // 浮動鎖點項不在流裡，不構成吸收
            }
            // 整段（空行＋圖）原子檢查＝max(圖帶, 空行帶)——absorb 語義：
            // 圖頂落在段落起始帶，空行不另計（原 ceil(h/line_h) 公式的
            // per-para 行高推廣；全域尺 P=1 時＝舊式逐位元組全等）。
            images[*img].bands.max(1).max(para_band(*para))
        };
        let item_bands = match &flow[fi] {
            FlowItem::Image { img } => {
                let b = images[*img].bands.max(1);
                if absorb_slot(*img, fi) {
                    // 空行帶已由前項 Text 涵蓋：只補差額（b<P 時補 0）。
                    b.saturating_sub(para_band(images[*img].para))
                } else {
                    b
                }
            }
            FlowItem::Text { .. } => para_img_bands(fi),
            _ => 0,
        };
        // 吸收態下那頁「唯一內容」就是共用的空行（cur_used==該段行帶數）：
        // 容量不足也不得換頁（巨型圖頂格放規則對空行同樣豁免），否則空行會
        // 孤留在前一頁、圖獨排下頁。
        let absorb_here = matches!(&flow[fi], FlowItem::Image { img } if absorb_slot(*img, fi));
        let absorb_exemption = if absorb_here {
            // 空行自身已計帶（該段 para_band）：豁免門檻＝空行不計入後的用量。
            match &flow[fi] {
                FlowItem::Image { img } => para_band(images[*img].para),
                _ => 0,
            }
        } else {
            0
        };
        // br-page 硬斷點（游標位置的純函數，語義見 pack_run doc）：段前
        // (k=0) 與段中 (0<k<nli) 斷點在游標恰落 (fi,k,0) 時命中；段尾斷點
        // (k=nli) 折疊為「游標在下一流項起頭且前一文字項以 k=nli 收尾」。
        // 斷點前的行已在 cur＝留在原頁，其後一切開新頁（Word 硬斷語義）。
        let hard = match &flow[fi] {
            FlowItem::Text { para } => {
                let pl = pls[*para];
                li < pl.lines.len() && pl.para_breaks.binary_search(&li).is_ok()
            }
            // 表格切片與圖片佔位項不帶段內斷點（格內 br 屬表格跨頁卡範圍）。
            _ => false,
        };
        let hard_after =
            li == 0 && band == 0 && fi > 0 && prev_item_breaks_after(flow, pls, images, fi - 1);
        // 顯式分頁（pageBreakBefore，t_5dfc8ac7）：段首行前一律換頁。
        let page_break_before = li == 0
            && band == 0
            && matches!(&flow[fi], FlowItem::Text { para } if grid.break_before_of(*para));
        // 容量/硬斷複合條件（t_c1fb4c69 S1-1）：頁滿、游標恰落 br 硬斷點、
        // 前一項段尾 br、原子項放不下——這些與「是否跨 section 邊界」正交，
        // continuous 邊界**不得**短路它們（協定只免「邊界強制換頁」一項）。
        // 帶數加算 saturating（S1-4 防呆）：垃圾 bands 不得在 debug 下溢出
        // panic（源頭守衛在 ooxml 轉換層，此處是 layout 側第二道閘）。
        let cap_hard = st.cur_used >= cap as u32
            || hard
            || hard_after
            || (item_bands > 0
                && st.cur_used > absorb_exemption
                && st.cur_used.saturating_add(item_bands) > cap as u32);
        let start_new = match st.cur_spec {
            // section 邊界：非 continuous 一律換頁（同尺寸也換）；continuous
            // 只免邊界本身，容量已滿／硬斷照樣換頁（S1-1）。
            Some(prev) if prev.id != spec.id => {
                !spec.geo.continuous || cap_hard || page_break_before
            }
            // 顯式分頁（pageBreakBefore，t_5dfc8ac7）：段首行前一律換頁（純 flow
            // 輸入函數：增量復入、收斂比較皆決定性）。首項（None＝本頁尚無內容）
            // 不換頁——游標前已 flush，段本就從頁首起，無須空白頁。
            Some(_) if page_break_before => true,
            Some(_) => cap_hard,
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
        // 頁首項蓋章（S1-1）：本頁第一個被放置的流項決定頁的 section。
        if st.page_spec.is_none() {
            st.page_spec = Some(spec);
        }
        let line_h = p.line_h_pt();
        match &flow[fi] {
            FlowItem::Text { para } => {
                st.cur.push(pls[*para].lines[li].clone());
                st.cur_bands.push(st.cur_used);
                // 段落行按段行高量化佔帶（全域尺＝1，舊行為逐位元組不變）。
                // saturating（t_5b391cfd S2／t_c1fb4c69 S1-4 同口徑）：病態段
                // 行高（畸形 w:sz／line 的殘影——入口守衛已擋，此處為第二道
                // 柵欄）量化出的巨型帶數不得 debug panic／release 繞回把分頁
                // 狀態機打亂。飽和後 `cur_used >= cap` 恆真＝之後每項強換頁，
                // 游標仍逐項前進、必然收斂，不觸斷點邏輯。
                st.cur_used = st.cur_used.saturating_add(para_band(*para));
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
                st.cur_used = st.cur_used.saturating_add(used); // 同 S2 守衛口徑
                st.cursor = if next.0 >= tl.row_count() {
                    (fi + 1, 0, 0)
                } else {
                    (fi, next.0, next.1)
                };
            }
            FlowItem::Image { img } => {
                // 圖片佔位（t_a1397522）：atomic 整塊排放。普通情形接在錨定
                // 段落末行的下一帶；純圖段落（吸收）圖頂＝段落起始帶（Word
                // 實態：圖就在那一行上，行高由圖撐起）。容量檢查在 start_new
                // （放不下整塊已先換頁）。
                let im = &images[*img];
                let b = im.bands.max(1);
                let absorb = absorb_slot(*img, fi);
                let top = if absorb {
                    // 圖頂＝段落起始帶：回退空行自身已計的帶數（全域尺＝1）。
                    st.cur_used.saturating_sub(para_band(im.para))
                } else {
                    st.cur_used
                };
                st.cur_images.push(PageImage {
                    para: im.para,
                    band: top,
                    w_pt: im.w_pt,
                    h_pt: im.h_pt.min(b as f32 * line_h),
                });
                // 佔用：absorb＝max(圖帶, 空行帶)－空行帶（空行帶已由 Text 空行
                // 計入）；非 absorb＝圖帶原額（段落另有實文，不共用帶）。
                st.cur_used = st.cur_used.saturating_add(if absorb {
                    b.saturating_sub(para_band(im.para))
                } else {
                    b
                }); // 同 S2 守衛口徑
                st.cursor = (fi + 1, 0, 0);
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
    paginate_tables_images(pls, p, sections, tables, &[])
}

/// 含表格＋圖片佔位的公開版（t_a1397522）。`images` 為正文流錨點（函式
/// 自行忽略浮動鎖點項——那些不進流，只附掛到錨定段落所在頁）。
pub fn paginate_tables_images(
    pls: &[ParagraphLayout],
    p: &LayoutParams,
    sections: &[SectionSpec],
    tables: &[TableSlot],
    images: &[ImageBand],
) -> (Vec<Page>, Vec<FlowPos>) {
    let refs: Vec<&ParagraphLayout> = pls.iter().collect();
    let flow = build_flow_images(pls.len(), tables, images);
    // 公開版無逐段網格（全域尺＝舊行為逐位元組不變）。
    paginate_flow(
        &flow,
        &refs,
        tables,
        images,
        p,
        sections,
        &ParaGrid::default(),
    )
}

/// 段落＋表格＋圖片混排流（Phase3-1b＋t_a1397522）：依 para_start 把表格
/// 項插進段落序列，表格內部段落（[para_start, para_end)）不進流（文字由
/// 表格切片繪製，雙重流出會雙繪）。表格項排在 para_start 那一**段之前**；
/// 圖片項排在錨定段落**之後**（段落文字在上、圖在下，Word 同款）。
/// 浮動鎖點項（`anchor == Some`）不進流——ECMA-376：浮動圖形不排進文字流。
pub fn build_flow_images(
    n_paras: usize,
    tables: &[TableSlot],
    images: &[ImageBand],
) -> Vec<FlowItem> {
    let mut flow = Vec::with_capacity(n_paras + tables.len() + images.len());
    let mut ti = 0usize;
    let mut gi = 0usize;
    let mut skip_until = 0usize; // 表格內部段落上界（不含）：流失權
                                 // 錨定段落落在表格區間內的流項（若 engine 未先行攔下）不進流：表格切片
                                 // 自己負責格內容量，正文流再加帶＝雙重計帳。
    let in_table_span = |para: usize| {
        tables
            .iter()
            .any(|t| t.para_start <= para && para < t.para_end)
    };
    for para in 0..n_paras {
        // para_start <= para 的表格都排在當前段之前（依序；同插入點保序）。
        while ti < tables.len() && tables[ti].para_start <= para {
            flow.push(FlowItem::Table { slot: ti });
            skip_until = skip_until.max(tables[ti].para_end);
            ti += 1;
        }
        // 錨定段落已過的浮動項：跳過（不進流；附掛在 paginate_flow 收尾）。
        while gi < images.len() && images[gi].para < para {
            gi += 1;
        }
        if para < skip_until {
            continue; // 表格內部段落：文字由表格切片繪製
        }
        flow.push(FlowItem::Text { para });
        // 錨定在該段的圖片（文档序）；越界/表格區間內項老實略過。
        while gi < images.len() && images[gi].para == para {
            if images[gi].anchor.is_none() && !in_table_span(para) {
                flow.push(FlowItem::Image { img: gi });
            }
            gi += 1;
        }
    }
    // para_start 超出末段（文件尾部表格）：續掛文末。
    while ti < tables.len() {
        flow.push(FlowItem::Table { slot: ti });
        ti += 1;
    }
    // 錨定段落越界（末段之後的圖，畸形檔容錯）：續掛文末。
    while gi < images.len() {
        if images[gi].anchor.is_none() && !in_table_span(images[gi].para) {
            flow.push(FlowItem::Image { img: gi });
        }
        gi += 1;
    }
    // sanity：表格區間不得重疊、不得越界（engine 映射層的契約）。
    debug_assert!(tables.iter().enumerate().all(|(i, t)| {
        (i == 0 || tables[i - 1].para_end <= t.para_start) && t.para_end <= n_paras
    }));
    flow
}

/// `build_flow` 舊簽名相容（無圖片）。
pub fn build_flow(n_paras: usize, tables: &[TableSlot]) -> Vec<FlowItem> {
    build_flow_images(n_paras, tables, &[])
}

/// 純函數分頁核心：流→頁。回傳（頁序列, 逐頁起頭 FlowPos）。
fn paginate_flow(
    flow: &[FlowItem],
    pls: &[&ParagraphLayout],
    tables: &[TableSlot],
    images: &[ImageBand],
    p: &LayoutParams,
    sections: &[SectionSpec],
    grid: &ParaGrid,
) -> (Vec<Page>, Vec<FlowPos>) {
    let mut pages: Vec<Page> = Vec::new();
    let mut starts: Vec<FlowPos> = Vec::new();
    // 全速掃描：無重用、無預算、不需覆蓋任何特定流項。
    pack_run(
        flow,
        pls,
        tables,
        images,
        p,
        sections,
        grid,
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
            images: Vec::new(),
            floating_images: Vec::new(),
            width_pt: geo.geo.page_w_pt,
            height_pt: geo.geo.page_h_pt,
            margin_left_pt: geo.geo.margin_left_pt,
            margin_top_pt: geo.geo.margin_top_pt,
            section_id: geo.id,
        });
        starts.push((0, 0, 0)); // 合成空頁＝流首（starts/pages 同步不變量）
    }
    attach_floating_images(&mut pages, &starts, flow, tables, images, p.line_h_pt());
    (pages, starts)
}

// 浮圖附掛全掃計數（debug 斷言用，t_c1fb4c69 S2-7）：「無鎖點圖也全頁
// clear＋全 flow 重掃」是退化路徑——計數器設在 para_fi 全掃起點，早退
// 修好後純內嵌圖檔案的重排不得再推進它。
// 必須 thread_local：cargo test 同 process 平行跑，全域計數器會被其他
// 執行緒（含鎖點圖檔案的測試）推進，讀 before/after 差值的斷言即偶發紅
// （跨執行緒汙染，非被測行為退化）。編輯與斷言同執行緒，thread_local 語義正確。
#[cfg(debug_assertions)]
thread_local! {
    pub(crate) static FLOATING_ATTACH_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(all(test, debug_assertions))]
fn floating_attach_scans() -> usize {
    FLOATING_ATTACH_SCANS.with(|c| c.get())
}

/// 浮動圖片附掛（t_a1397522）：鎖點項不進流，分頁完成後按錨定段落所在頁
/// 掛到該頁的 `floating_images`。段落 → 流項 → 頁：起頭 FlowPos 嚴格單調，
/// partition_point 一步到位。錨定段落查無所在頁（未排/畸形）＝老實丟棄。
/// **冪等全量重算**：先清空再附掛——增量重排（含重用尾段）後呼叫同一函數
/// 即可讓鎖點跟隨新頁界，永不出現雙掛。
fn attach_floating_images(
    pages: &mut [Page],
    starts: &[FlowPos],
    flow: &[FlowItem],
    tables: &[TableSlot],
    images: &[ImageBand],
    line_h: f32,
) {
    // 無鎖點圖＝零可掛項（images 生命期內不變，浮掛只由 anchor 填充）：
    // 在「全頁 clear＋全 flow 重掃」之前早退（t_c1fb4c69 S2-7——純內嵌圖
    // 檔案每次編輯都白掃全版的退化路徑）。
    if images.is_empty() || pages.is_empty() || !images.iter().any(|i| i.anchor.is_some()) {
        return;
    }
    #[cfg(debug_assertions)]
    FLOATING_ATTACH_SCANS.with(|c| c.set(c.get() + 1));
    for pg in pages.iter_mut() {
        pg.floating_images.clear();
    }
    // 段落 → 流項下標（文字項＝其本身；表格內部段落＝該表格項）。
    let mut para_fi: Vec<usize> =
        vec![usize::MAX; images.iter().map(|i| i.para).max().map_or(0, |m| m + 1)];
    for (fi, item) in flow.iter().enumerate() {
        match item {
            FlowItem::Text { para } => {
                if *para < para_fi.len() {
                    para_fi[*para] = fi;
                }
            }
            FlowItem::Table { slot } => {
                let t = &tables[*slot];
                for p in t.para_start..t.para_end.min(para_fi.len()) {
                    para_fi[p] = fi;
                }
            }
            FlowItem::Image { .. } => {}
        }
    }
    for im in images.iter().filter(|i| i.anchor.is_some()) {
        let Some(a) = im.anchor else {
            continue;
        };
        let fi = *para_fi.get(im.para).unwrap_or(&usize::MAX);
        if fi == usize::MAX {
            continue; // 錨定段落不在流（畸形）：不虛構放置
        }
        // 段落**起頭頁**（S2-7）：頁 p 涵蓋 [starts[p], starts[p+1])，段落首行
        // 位址 (fi,0,0) 落在哪個區間就掛哪頁。舊式 `s.0 <= fi` 只比流項下標
        // ——跨頁段落的後續頁起頭（同 fi、更大 li）也命中，被鉗到末頁。
        let page = starts
            .partition_point(|s| *s <= (fi, 0, 0))
            .saturating_sub(1);
        let Some(pg) = pages.get_mut(page) else {
            continue;
        };
        // 各軸按 relativeFrom 原點系換算成頁絕對座標：
        //   Page      → (0,0)＋offset
        //   Margin    → 版心左上＋offset
        //   Paragraph → 錨定段落首行帶左上＋offset（查無首行＝版心左上近似）
        let para_band = pg
            .lines
            .iter()
            .position(|l| l.para_index == im.para)
            .and_then(|i| pg.line_bands.get(i))
            .map(|b| *b as f32);
        let x = a.x_pt
            + match a.x_frame {
                OriginFrame::Page => 0.0,
                OriginFrame::Margin | OriginFrame::Paragraph => pg.margin_left_pt,
            };
        let y = a.y_pt
            + match a.y_frame {
                OriginFrame::Page => 0.0,
                OriginFrame::Margin => pg.margin_top_pt,
                OriginFrame::Paragraph => pg.margin_top_pt + para_band.unwrap_or(0.0) * line_h,
            };
        pg.floating_images.push(PageFloatingImage {
            x_pt: x,
            y_pt: y,
            w_pt: im.w_pt,
            h_pt: im.h_pt,
        });
    }
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

/// 單頁錨點：該頁的首/末段落 index（迭代收斂時回饋「哪頁有什麼」用）。
#[derive(Debug, Clone, PartialEq)]
pub struct PageAnchor {
    /// 頁序（0 起算）＝逐頁預留表的索引。
    pub page: usize,
    /// 此頁第一行所屬段落 index。
    pub first_para: usize,
    /// 此頁最後一行所屬段落 index（含）。
    pub last_para: usize,
}

/// 一輪分頁的逐頁錨點（`paginate_converged` 餵給 `reserve_fn` 的回饋面）。
#[derive(Debug, Clone, PartialEq)]
pub struct PageAnchors {
    /// 逐頁錨點（索引＝頁序）。
    pub pages: Vec<PageAnchor>,
}

impl PageAnchors {
    /// 由分頁斷點導出錨點（首/末段落直接取斷點既有欄位，不重算）。
    pub fn from_breaks(breaks: &[PageBreak]) -> Self {
        Self {
            pages: breaks
                .iter()
                .map(|b| PageAnchor {
                    page: b.page,
                    first_para: b.first_para,
                    last_para: b.last_para,
                })
                .collect(),
        }
    }
}

/// 迭代收斂的輪數上限：預留→重排→再算預留，最多來回 3 輪。
///
/// 取捨：Word 內部同款收斂是迴圈到穩定為止（理論上可能不終止）；此處設
/// 硬上限 3 輪保證**有界**（最壞情況＝第 3 輪的分頁結果，呼叫端可重跑）。
/// 預留恆 0 時第 1 輪即收斂（實務上常見情形不付迭代成本）。
pub const MAX_CONVERGE_ROUNDS: usize = 3;

/// 以「預留→重排→再算預留」迭代到穩定的分頁入口（Word 同款收斂思路）。
///
/// `reserve_fn`：吃「本輪每頁的首/末段落錨點」回該頁預留（pt）。回傳向量
/// 索引＝頁序：短於頁數＝缺的視同 0，長於頁數＝多的忽略（皆不 panic）。
///
/// 收斂判準：**預留向量穩定**（本輪算出的預留 ≡ 上一輪套用的預留）——分頁是
/// 預留向量的純函數，故預留穩定 ⇒ 分頁結果必然穩定。最多 `MAX_CONVERGE_ROUNDS`
/// 輪；輪數用盡仍未穩定 ⇒ 回傳**最後一輪**（第 3 輪）的分頁結果與輪數。
///
/// 回傳（分頁斷點, 實際輪數）。預留恆 0 時第 1 輪即收斂（輪數＝1）。
///
/// 取捨：此入口**不動**既有 `page_breaks`／`paginate_flow` 簽名——既有入口
/// 語義不變（現況零變化），收斂機制另開一條路。
pub fn paginate_converged(
    texts: &[String],
    p: &LayoutParams,
    reserve_fn: &dyn Fn(&PageAnchors) -> Vec<f32>,
) -> (Vec<PageBreak>, usize) {
    paginate_converged_per_section(texts, p, &[], reserve_fn)
}

/// 逐 section 版收斂分頁（見 `paginate_converged`）：每頁尺寸取自身 section
/// 實際幾何，與 `page_breaks_per_section` 同口徑。
pub fn paginate_converged_per_section(
    texts: &[String],
    p: &LayoutParams,
    sections: &[SectionSpec],
    reserve_fn: &dyn Fn(&PageAnchors) -> Vec<f32>,
) -> (Vec<PageBreak>, usize) {
    // 本輪套用的逐頁預留（起始＝params 既有的表；空表＝無逐頁預留）。
    let mut cur: Vec<f32> = p.footnote_reserves_pt.clone();
    let mut rounds = 0usize;
    loop {
        rounds += 1;
        // 本輪分頁用的 params：逐頁預留表＝上一輪算出的值。
        let mut pp = p.clone();
        pp.footnote_reserves_pt = cur.clone();
        let brks = page_breaks_per_section(texts, &pp, sections);
        let anchors = PageAnchors::from_breaks(&brks);
        let next = reserve_fn(&anchors);
        if reserves_equivalent(&next, &cur) {
            return (brks, rounds); // 預留穩定 ⇒ 分頁穩定，收斂。
        }
        if rounds >= MAX_CONVERGE_ROUNDS {
            return (brks, rounds); // 輪數用盡：回傳最後一輪結果（呼叫端可重跑）。
        }
        cur = next;
    }
}

/// 預留向量等價（收斂判準）：逐元素正規化（負值／NaN 視同 0）後比較；
/// 長度不同者短者補 0（缺頁＝無預留）、長者截斷（多餘頁序不影響既有頁）。
fn reserves_equivalent(a: &[f32], b: &[f32]) -> bool {
    let n = a.len().max(b.len());
    (0..n).all(|i| {
        let x = a.get(i).map(|v| reserve_or_zero(*v)).unwrap_or(0.0);
        let y = b.get(i).map(|v| reserve_or_zero(*v)).unwrap_or(0.0);
        x == y
    })
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
    /// 逐段落的 br-page 強制分頁斷點（碼點偏移，`Paragraph::page_breaks` 直傳；
    /// 短＝無斷點段落）。斷行（`break_para`）與分頁（pack_run 讀快取的
    /// `para_breaks`）都由此取數；編輯改文時偏移依 changed_bytes 平移（同
    /// undo 快照的標尺鐵律：改文不丟使用者手插的硬斷點）。
    breaks: Vec<Vec<usize>>,
    /// 逐段落字級/行帶表（t_5dfc8ac7；空＝全域 params 尺＝既有行為）。
    grid: ParaGrid,
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
    /// 正文流圖片佔位錨點（t_a1397522；engine 由 `w:drawing` 投影餵入，
    /// 生命期內不變——圖片不隨打字編輯變動）。
    images: Vec<ImageBand>,
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
        Self::with_tables_images(texts, params, sections, tables, Vec::new())
    }

    /// 帶表格槽＋圖片佔位的建構（t_a1397522）。`images` 依 para 排序
    /// （engine 映射層保證；越界錨點由 build_flow_images 容錯掛文末）。
    pub fn with_tables_images(
        texts: &[String],
        params: LayoutParams,
        sections: Vec<SectionSpec>,
        tables: Vec<TableSlot>,
        images: Vec<ImageBand>,
    ) -> Self {
        Self::with_breaks(texts, params, sections, tables, images, Vec::new())
    }

    /// 帶 br-page 硬斷點的建構（`w:br w:type="page"`，ECMA-376 §17.4.3）：
    /// `breaks` 與 `texts` 逐段同標（碼點偏移；短＝無斷點段落）。既有呼叫端
    /// 傳空＝行為逐位元組不變。
    pub fn with_breaks(
        texts: &[String],
        params: LayoutParams,
        sections: Vec<SectionSpec>,
        tables: Vec<TableSlot>,
        images: Vec<ImageBand>,
        breaks: Vec<Vec<usize>>,
    ) -> Self {
        Self::with_breaks_grid(
            texts,
            params,
            sections,
            tables,
            images,
            breaks,
            ParaGrid::default(),
        )
    }

    /// 帶 br-page 硬斷點＋逐段落字級/行帶表的建構（t_5dfc8ac7，合併 t_01fa57af
    /// 的 `with_breaks` 為單一最終形）。`grid` 由 engine 層從樣式級聯算出餵入：
    /// 段落斷行用段字級量測、分頁按段行高量化佔帶。空表＝全域 params 尺
    /// （既有全部呼叫端行為逐位元組不變）。
    pub fn with_breaks_grid(
        texts: &[String],
        params: LayoutParams,
        sections: Vec<SectionSpec>,
        tables: Vec<TableSlot>,
        images: Vec<ImageBand>,
        breaks: Vec<Vec<usize>>,
        grid: ParaGrid,
    ) -> Self {
        let cache: Vec<Option<ParagraphLayout>> = vec![None; texts.len()];
        let mut eng = Self {
            texts: texts.to_vec(),
            params,
            sections,
            breaks,
            grid,
            pages: Vec::new(),
            cache,
            starts: Vec::new(),
            flow: Vec::new(),
            tables,
            images,
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
        self.flow = build_flow_images(self.texts.len(), &self.tables, &self.images);
        let (pages, starts) = paginate_flow(
            &self.flow,
            &refs,
            &self.tables,
            &self.images,
            &self.params,
            &self.sections,
            &self.grid,
        );
        self.pages = pages;
        self.starts = starts;
        self.pending = None;
    }

    /// 單段 KP 斷行（蓋 para_index 章）。只給髒段落呼叫——快取命中零成本。
    /// br-page 段中斷點折入斷行（KP 硬收行）、段首/段尾斷點進 `para_breaks`
    /// 給 pack_run 升格為頁邊界；段字級取自 band grid（缺位＝全域 params
    /// 字級＝舊行為，t_5dfc8ac7）。
    fn break_para(&self, i: usize) -> ParagraphLayout {
        let content_w = section_params(&self.params, &self.sections, i).content_w_pt();
        let size = self.grid.size_of(i, self.params.font_size_pt);
        let m = TextMeasurer::new(size);
        let (lines, para_breaks) = break_para_lines(
            &self.texts[i],
            content_w,
            &m,
            self.breaks.get(i).map(Vec::as_slice).unwrap_or(&[]),
        );
        let mut pl = ParagraphLayout {
            para_index: i,
            lines,
            para_breaks,
        };
        for l in pl.lines.iter_mut() {
            l.para_index = i;
        }
        pl
    }

    /// F2b-2：換分頁參數並整篇重排（註腳收斂迴圈用——逐頁預留表變動時，
    /// 版面必須按新預留重排）。斷行快取保守清空：params 頁面欄位變動會
    /// 改變逐段可用寬，留舊快取有錯斷行風險（正確性優先於增量速度；
    /// 收斂迴圈最多 `MAX_CONVERGE_ROUNDS` 次，代價有界）。
    pub fn update_params(&mut self, params: LayoutParams) {
        self.params = params;
        self.cache = vec![None; self.cache.len()];
        self.rebuild_full();
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
        self.params.clone()
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
        let old_text = std::mem::replace(&mut self.texts[para], new_text.to_string());
        // br-page 偏移隨編輯平移（碼點標尺，與 undo 快照同一鐵律）：斷點在
        // 改區之前原樣保留；在改區之後按增減量平移；落在改區內（文字被蓋掉）
        // → 折到改區末（寧可保守留在原位附近，絕不無腦丟斷點）。
        if let Some(bks) = self.breaks.get_mut(para) {
            if !bks.is_empty() {
                *bks = shift_breaks(&old_text, new_text, bks);
            }
        }
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
            cur_images: Vec::new(),
            cur_used: 0,
            // 空 cur ⇒ 首行必不 trigger 換頁（前頁已保留），None 安全。
            cur_spec: None,
            // 重排頁從掃描首項重蓋 section（S1-1 頁首項蓋章）。
            page_spec: None,
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
            &self.images,
            &self.params,
            &self.sections,
            &self.grid,
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
        // 浮動鎖點跟隨新頁界（冪等全量重算，見函式注）。
        attach_floating_images(
            &mut self.pages,
            &self.starts,
            &self.flow,
            &self.tables,
            &self.images,
            self.params.line_h_pt(),
        );
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
            &self.images,
            &self.params,
            &self.sections,
            &self.grid,
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
        // 浮動鎖點跟隨新頁界（冪等全量重算，見函式注）。
        attach_floating_images(
            &mut self.pages,
            &self.starts,
            &self.flow,
            &self.tables,
            &self.images,
            self.params.line_h_pt(),
        );
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
        let eng = LayoutEngine::new(&texts, p.clone());
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
            assert!(
                !pg.lines.is_empty() || !pg.tables.is_empty(),
                "{ctx}: 頁不可全空（空段以單行表達；br-page 後接表格允許純表格頁）"
            );
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
        let fresh = LayoutEngine::new(texts, p.clone());
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
        let mut eng = LayoutEngine::new(&base, p.clone());
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
        let mut eng = LayoutEngine::new(&texts, p.clone());
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
        let mut eng = LayoutEngine::new(&texts, p.clone());
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
        let mut eng = LayoutEngine::with_sections(&base, p.clone(), sections.clone());
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
            let fresh = LayoutEngine::with_sections(&texts, p.clone(), sections.clone());
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
            // 頁底預留 0：既有測試尺不預留（預留 0 逐位元組不變）。
            footnote_reserve_pt: 0.0,
            // 逐頁預留空表：既有測試尺不逐頁預留（空表逐位元組不變）。
            footnote_reserves_pt: Vec::new(),
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
            // 頁底預留 0：既有測試尺不預留（預留 0 逐位元組不變）。
            footnote_reserve_pt: 0.0,
            // 逐頁預留空表：既有測試尺不逐頁預留（空表逐位元組不變）。
            footnote_reserves_pt: Vec::new(),
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

    // ---------------- t_a1397522：圖片佔位帶（inline 撐帶／anchor 不佔帶） ----------------

    /// 帶數公式的尺：line_h＝12pt×1.4＝16.8pt；每頁容量＝(200-80)/16.8 → 7 帶。
    fn img_params() -> LayoutParams {
        LayoutParams {
            page_w_pt: 400.0,
            page_h_pt: 200.0,
            margins_pt: [40.0; 4],
            font_size_pt: 12.0,
            line_spacing: 1.4,
            // 頁底預留 0：既有測試尺不預留（預留 0 逐位元組不變）。
            footnote_reserve_pt: 0.0,
            // 逐頁預留空表：既有測試尺不逐頁預留（空表逐位元組不變）。
            footnote_reserves_pt: Vec::new(),
        }
    }

    fn band(para: usize, h_pt: f32, line_h: f32) -> ImageBand {
        ImageBand {
            para,
            w_pt: 200.0,
            h_pt,
            bands: (h_pt / line_h).ceil().max(1.0) as u32,
            anchor: None,
        }
    }

    #[test]
    fn inline_image_band_math_and_page_push() {
        let p = img_params();
        let line_h = p.line_h_pt();
        assert!((line_h - 16.8).abs() < 1e-3);
        let texts = vec!["前段".to_string(), String::new(), "後段".to_string()];
        let imgs = vec![band(1, 100.0, line_h)];
        let pls = layout_paragraphs(&texts, &p);
        let (pages, _starts) = paginate_tables_images(&pls, &p, &[], &[], &imgs);
        assert_eq!(
            pages.len(),
            2,
            "前段1＋圖段6＋後段1＝8 帶＞7 帶容，後段推頁"
        );
        let pg0 = &pages[0];
        assert_eq!(pg0.line_bands, vec![0, 1], "頁0＝前段帶 0＋圖段空行帶 1");
        assert_eq!(pg0.images.len(), 1);
        let im = &pg0.images[0];
        assert_eq!((im.para, im.band), (1, 1), "純圖段：圖頂＝段落起始帶");
        assert!((im.w_pt - 200.0).abs() < 1e-3);
        assert!(
            (im.h_pt - (100.0f32).min(6.0 * line_h)).abs() < 1e-3,
            "發射高＝min(extent, bands×line_h)：{}",
            im.h_pt
        );
        // 段落總佔位恰 6 帶：下一段的帶＝1＋6＝7（頁 1 起算 0）。
        assert_eq!(pages[1].line_bands, vec![0], "後段整段推到第 2 頁帶 0");
    }

    #[test]
    fn inline_image_after_text_line_starts_next_band() {
        // 圖掛在有文字的段落末行之後：圖頂＝文字行帶＋1（不吸收）。
        let p = img_params();
        let line_h = p.line_h_pt();
        let texts = vec!["這段有文字，圖跟在後面。".to_string(), "尾段".to_string()];
        let imgs = vec![band(0, 50.0, line_h)]; // ceil(50/16.8)=3 帶
        let pls = layout_paragraphs(&texts, &p);
        let (pages, _starts) = paginate_tables_images(&pls, &p, &[], &[], &imgs);
        assert_eq!(pages.len(), 1, "1＋3＋1＝5 帶 ≤ 7 帶容：同頁排完");
        assert_eq!(pages[0].images[0].band, 1, "文字行帶 0 → 圖帶 1");
        assert_eq!(
            pages[0].line_bands,
            vec![0, 4],
            "尾段接在圖末帶 3 之後＝帶 4"
        );
    }

    #[test]
    fn anchored_image_does_not_consume_bands_but_emits_rect() {
        // 卡片斷言：anchor（帶可用鎖點）不佔正文流——正文行帶數與無圖時
        // 全等，但該頁 floating_images 有一筆。
        let p = img_params();
        let line_h = p.line_h_pt();
        let texts = vec!["前段".to_string(), "後段".to_string()];
        let pls = layout_paragraphs(&texts, &p);
        let (bare, _s0) = paginate_tables_images(&pls, &p, &[], &[], &[]);
        let mut floats = vec![band(1, 100.0, line_h)];
        floats[0].anchor = Some(FloatingAnchor {
            x_pt: 36.0,
            y_pt: 72.0,
            x_frame: OriginFrame::Page,
            y_frame: OriginFrame::Margin,
        });
        floats[0].bands = 0;
        let (pages, _s1) = paginate_tables_images(&pls, &p, &[], &[], &floats);
        assert_eq!(
            pages[0].line_bands, bare[0].line_bands,
            "浮動圖不得撐動正文行帶"
        );
        assert_eq!(pages.len(), bare.len(), "浮動圖不得多擠一頁");
        assert_eq!(
            pages[0].floating_images.len(),
            1,
            "錨定段落在第 1 頁→鎖點掛該頁"
        );
        assert!(pages[0].images.is_empty(), "鎖點項不得進正文流佔位");
        let fm = &pages[0].floating_images[0];
        assert!((fm.x_pt - 36.0).abs() < 1e-3, "page 系 x 原值：{}", fm.x_pt);
        assert!(
            (fm.y_pt - (40.0 + 72.0)).abs() < 1e-3,
            "margin 系 y：{}",
            fm.y_pt
        );
    }

    #[test]
    fn anchored_floating_image_attaches_to_anchor_page() {
        // 鎖點圖掛在段 0（Paragraph 系 y）：附掛第 1 頁，y＝段落帶頂＋offset。
        let p = img_params();
        let line_h = p.line_h_pt();
        let texts = vec!["錨段有文字。".to_string()];
        let mut floats = vec![band(0, 60.0, line_h)];
        floats[0].anchor = Some(FloatingAnchor {
            x_pt: 12.5,
            y_pt: 30.0,
            x_frame: OriginFrame::Margin,
            y_frame: OriginFrame::Paragraph,
        });
        floats[0].bands = 0;
        let pls = layout_paragraphs(&texts, &p);
        let (pages, _starts) = paginate_tables_images(&pls, &p, &[], &[], &floats);
        assert_eq!(pages[0].images.len(), 0, "鎖點項不進正文流");
        assert_eq!(pages[0].floating_images.len(), 1);
        let fm = &pages[0].floating_images[0];
        assert!(
            (fm.x_pt - (40.0 + 12.5)).abs() < 1e-3,
            "margin x：{}",
            fm.x_pt
        );
        assert!(
            (fm.y_pt - (40.0 + 30.0)).abs() < 1e-3,
            "paragraph 系 y（段 0 帶 0）：{}",
            fm.y_pt
        );
        assert_eq!(pages[0].line_bands, vec![0], "正文仍只佔 1 帶");
    }

    #[test]
    fn oversized_image_packs_top_of_page_without_infinite_loop() {
        // bands(10)＞cap(7)：頂格放、容許溢出，不得無限換頁。
        let p = img_params();
        let line_h = p.line_h_pt();
        let texts = vec![String::new()];
        let imgs = vec![band(0, 160.0, line_h)]; // ceil(160/16.8)=10 帶
        let pls = layout_paragraphs(&texts, &p);
        let (pages, _starts) = paginate_tables_images(&pls, &p, &[], &[], &imgs);
        assert_eq!(pages.len(), 1, "巨型圖只能頂格溢出單頁");
        assert_eq!(pages[0].images[0].band, 0);
    }

    #[test]
    fn degraded_anchor_bands_like_inline() {
        // 鎖點不可信（anchor=None 但 floating 投影已退化）＝inline 佔帶。
        let p = img_params();
        let line_h = p.line_h_pt();
        let texts = vec![String::new()];
        let imgs = vec![band(0, 33.6, line_h)]; // 恰 2 帶
        let pls = layout_paragraphs(&texts, &p);
        let (pages, _starts) = paginate_tables_images(&pls, &p, &[], &[], &imgs);
        assert_eq!(pages[0].images.len(), 1);
        assert_eq!(pages[0].images[0].band, 0, "吸收：圖頂＝段落帶 0");
    }

    #[test]
    fn incremental_with_tables_matches_full_rebuild() {
        // 表格存在下打字：增量 packer（FlowItem::Table 分支＋收斂重用）與
        // 整篇重建逐頁全等（含 TablePlacement 切片、line_bands、starts）。
        let (texts, p, tables) = cross_page_fixture();
        let mut eng = LayoutEngine::with_tables(&texts, p.clone(), Vec::new(), tables.clone());
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
            let fresh =
                LayoutEngine::with_tables(&want_texts, p.clone(), Vec::new(), tables.clone());
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

    // ---------------- br-page 強制分頁（t_01fa57af） ----------------

    /// br-page 三條硬性測試之一：斷點前文字留在原頁、其後開新頁。段首斷
    /// （k=0）→ 本段落整段換頁；斷點恰在頁尾（既有分頁已斷）→ 冪等不炸、
    /// 頁數不重複疊加。
    #[test]
    fn br_page_forces_new_page_and_collapses_at_page_end() {
        let p = LayoutParams {
            page_w_pt: 400.0,
            page_h_pt: 40.0,
            margins_pt: [0.0; 4],
            font_size_pt: 10.0,
            line_spacing: 1.0,
            // 頁底預留 0：既有測試尺不預留（預留 0 逐位元組不變）。
            footnote_reserve_pt: 0.0,
            // 逐頁預留空表：既有測試尺不逐頁預留（空表逐位元組不變）。
            footnote_reserves_pt: Vec::new(),
        };
        // 容量＝4 行/頁；三段各 80 碼點＝2 行。無 br：6 行→2 頁（4＋2）。
        let texts: Vec<String> = (0..3).map(|_| "字".repeat(80)).collect();
        let plain = LayoutEngine::new(&texts, p.clone());
        let base_pages = plain.page_count();
        assert_eq!(base_pages, 2, "無 br：6 行→2 頁");

        let breaks = vec![
            vec![texts[0].chars().count()],
            vec![texts[1].chars().count()],
            vec![],
        ];
        let eng = LayoutEngine::with_breaks(
            &texts,
            p.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            breaks,
        );
        assert_layout_invariants(&eng, "br-page");
        // 段 0 尾斷→段 1 開新頁；段 1 尾斷→段 2 開新頁：每段各自成頁。
        assert_eq!(eng.page_count(), 3, "斷點把後續段落推到新頁");
        assert!(eng.page_starts().windows(2).all(|w| w[0] < w[1]));
        // 每頁恰一節段（斷點前文字留原頁、斷點後全部另起）。
        for (i, pg) in eng.pages().iter().enumerate() {
            assert!(
                pg.lines.iter().all(|l| l.para_index == i),
                "頁 {i} 只屬段 {i}"
            );
        }

        // 冪等：斷點恰落在既有頁尾（段 0 末行恰填滿頁）時不得疊加空白頁。
        // 段 1 首行 (1,0,0) 原本就因容量已滿換頁——br 與既有斷點重合＝零新增。
        let coincide = LayoutEngine::with_breaks(
            &texts,
            p.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![vec![], vec![0], vec![]],
        );
        assert_eq!(
            coincide.page_count(),
            base_pages,
            "斷點恰落在頁尾（與既有分頁重合）→ 冪等不疊頁"
        );
        // 文件首斷點（游標起頭、cur 恆空）＝flush no-op，同樣零新增。
        let lead = LayoutEngine::with_breaks(
            &texts,
            p.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![vec![0], vec![], vec![]],
        );
        assert_eq!(
            lead.page_count(),
            base_pages,
            "文件首 br 無前置內容＝不產空頁"
        );
    }

    /// br-page 硬性測試之二：段中斷點——斷點前的行留在原頁，其後行開新頁；
    /// 斷點處必是斷行邊界（KP 硬收行），其後文字不回填原行。
    #[test]
    fn mid_paragraph_break_hard_ends_line_and_page() {
        let p = LayoutParams {
            page_w_pt: 400.0,
            page_h_pt: 100.0,
            margins_pt: [0.0; 4],
            font_size_pt: 10.0,
            line_spacing: 1.0,
            // 頁底預留 0：既有測試尺不預留（預留 0 逐位元組不變）。
            footnote_reserve_pt: 0.0,
            // 逐頁預留空表：既有測試尺不逐頁預留（空表逐位元組不變）。
            footnote_reserves_pt: Vec::new(),
        };
        // 段內 20 碼點，行寬 400pt/字寬 10pt＝40 字/行 → 全段一行；br 放碼點
        // 8 → 斷行硬收：前 8 字一行、後 12 字另起一行，且該行落新頁。
        let text = "字".repeat(20);
        let eng = LayoutEngine::with_breaks(
            &text_only(&text),
            p.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![vec![8usize]],
        );
        assert_layout_invariants(&eng, "段中 br");
        assert_eq!(eng.page_count(), 2, "段中 br＝其後行換頁");
        let pg0 = &eng.pages()[0];
        let pg1 = &eng.pages()[1];
        assert_eq!(pg0.lines.len(), 1);
        assert_eq!(pg0.lines[0].text.chars().count(), 8, "斷點前文字留原頁");
        assert_eq!(pg1.lines[0].text.chars().count(), 12, "斷點後文字開新頁");
        assert_eq!(pg1.lines[0].para_index, pg0.lines[0].para_index, "同段跨頁");
    }

    /// br-page 硬性測試之三：br-page 上線後任何檔案頁數只增不減——用合成的
    /// 混合文件（長短段＋跨頁）逐段加斷點驗證單調性。
    #[test]
    fn adding_breaks_never_reduces_page_count() {
        let texts: Vec<String> = (0..40)
            .map(|i| match i % 4 {
                0 => format!("長段{i}：{}", "字".repeat(700)),
                1 => format!("短段{i}"),
                2 => format!("中英 mix content 混排。{}{}", "文".repeat(200), i),
                _ => String::new(),
            })
            .collect();
        let p = LayoutParams::default();
        let base = LayoutEngine::new(&texts, p.clone()).page_count();
        for (i, t) in texts.iter().enumerate() {
            let len = t.chars().count();
            let brs: Vec<Vec<usize>> = texts
                .iter()
                .enumerate()
                .map(|(j, _)| if j == i { vec![len / 2] } else { vec![] })
                .collect();
            let eng = LayoutEngine::with_breaks(
                &texts,
                p.clone(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                brs,
            );
            assert!(
                eng.page_count() >= base,
                "段 {i} 加 br-page 後頁數 {n} < 基準 {base}",
                n = eng.page_count()
            );
            assert_layout_invariants(&eng, &format!("段 {i} 斷點"));
        }
    }

    /// 改文平移（undo 快照同標尺）：改區前斷點原樣、改區後斷點位移、改區內
    /// 斷點折到改區首——且增量重排結果與整篇重建逐頁全等。
    #[test]
    fn edit_shifts_breaks_and_incremental_matches_full_rebuild() {
        assert_eq!(
            shift_breaks("hello world", "hello big world", &[5, 8]),
            vec![5, 12],
            "改區後斷點按增量平移"
        );
        assert_eq!(
            // 插入點恰在斷點處（b=6）：斷點緊貼其前文字 → 原樣不動。
            shift_breaks("hello world", "hello big world", &[2, 6, 8]),
            vec![2, 6, 12],
            "插入點上的斷點留在原地"
        );
        assert_eq!(
            // 改區重疊斷點（world→earth 覆蓋碼點 7）：斷點折到改區首 lo=6。
            shift_breaks("hello world", "hello earth", &[7]),
            vec![6],
            "改區內斷點折到改區首"
        );
        assert_eq!(
            shift_breaks("abcd", "ab", &[1, 4]),
            vec![1, 2],
            "刪除：改區後斷點回退並鉗到段長"
        );

        let p = LayoutParams::default();
        let texts: Vec<String> = (0..30)
            .map(|i| format!("段落 {i}：{}", "混排 content 字。".repeat(40)))
            .collect();
        let mut breaks: Vec<Vec<usize>> = vec![vec![]; texts.len()];
        breaks[3] = vec![texts[3].chars().count()]; // 段尾 br
        breaks[7] = vec![5]; // 段中 br
        breaks[9] = vec![texts[9].chars().count() / 2];
        let mut eng = LayoutEngine::with_breaks(
            &texts,
            p.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            breaks.clone(),
        );
        assert_layout_invariants(&eng, "br 初始");

        // 編輯斷點段落之前/之後/之內各一筆，每筆後增量 ≡ 重建。
        let mut live = texts.clone();
        let mut live_breaks = breaks.clone();
        let edits: Vec<(usize, &str)> = vec![
            (0, "段首前綴插入 0123456789"),
            (9, "在 br 之後追加一大段文字文字文字。"),
            (7, "x在 br 之前插入"),
            (12, "無斷點段落隨意編輯內容。"),
        ];
        for (para, new) in edits {
            eng.replace_paragraph_text(para, new);
            live[para] = new.to_string();
            live_breaks[para] = shift_breaks(&texts[para], new, &breaks[para]);
            let fresh = LayoutEngine::with_breaks(
                &live,
                p.clone(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                live_breaks.clone(),
            );
            assert_eq!(
                eng.page_starts(),
                fresh.page_starts(),
                "段 {para} 編輯後頁起頭與重建不符"
            );
            assert_eq!(eng.pages(), fresh.pages(), "段 {para} 編輯後版面與重建不符");
            assert_layout_invariants(&eng, "編輯後");
        }
    }

    /// 段尾 br-page 的「下一個流項換頁」對表格同樣成立：斷點段之後是表格時，
    /// 表格切片必從新頁起排（無斷點時表格首片接在段後同頁續排）。
    #[test]
    fn br_page_after_para_pushes_table_to_new_page() {
        let p = LayoutParams {
            page_w_pt: 400.0,
            page_h_pt: 200.0,
            margins_pt: [40.0; 4],
            font_size_pt: 12.0,
            line_spacing: 1.4,
            // 頁底預留 0：既有測試尺不預留（預留 0 逐位元組不變）。
            footnote_reserve_pt: 0.0,
            // 逐頁預留空表：既有測試尺不逐頁預留（空表逐位元組不變）。
            footnote_reserves_pt: Vec::new(),
        };
        let texts = vec!["前段一行".to_string()];
        // 容量＝(200−80)/16.8＝7 帶：前段 1＋表 6 恰好同頁；段尾 br 後表格
        // （6 帶）整體被推到新頁。
        let input = tall_table_input(6);
        let m = TextMeasurer::new(p.font_size_pt);
        let tl = crate::table::layout_table(&input, p.content_w_pt(), p.line_h_pt(), &m, 0);
        let mk_slot = || TableSlot {
            para_start: 1,
            para_end: 1,
            layout: tl.clone(),
        };
        let without = LayoutEngine::with_tables(&texts, p.clone(), Vec::new(), vec![mk_slot()]);
        let with = LayoutEngine::with_breaks(
            &texts,
            p.clone(),
            Vec::new(),
            vec![mk_slot()],
            Vec::new(),
            vec![vec![texts[0].chars().count()]],
        );
        assert_eq!(without.page_count(), 1, "無斷點：前段＋表格首片同頁");
        assert_eq!(with.page_count(), 2, "段尾 br：表格整片推到新頁");
        assert_eq!(with.pages()[1].tables.len(), 1);
        assert!(with.pages()[0].tables.is_empty(), "表格不得回填斷點前的頁");
        assert_layout_invariants(&with, "br＋表格");
    }

    // ---------------- ParaGrid：段行帶量化＋段首換頁（t_5b391cfd 測試缺口 ii） ----------------

    /// ParaGrid.bands_of 語義：缺位/0＝1（行至少一格）；break_before_of／
    /// size_of 缺位退回 false／全域 fallback。
    #[test]
    fn para_grid_accessors_fall_back_per_contract() {
        let g = ParaGrid {
            size_pt: vec![14.0],
            bands: vec![3, 0],
            break_before: vec![false, true],
        };
        assert_eq!(g.bands_of(0), 3);
        assert_eq!(g.bands_of(1), 1, "0 帶視為 1（行至少佔一格）");
        assert_eq!(g.bands_of(9), 1, "缺位＝1（全域尺行為）");
        assert!(!g.break_before_of(0));
        assert!(g.break_before_of(1));
        assert!(!g.break_before_of(9), "缺位＝false（零宣告＝舊行為）");
        assert_eq!(g.size_of(0, 12.0), 14.0);
        assert_eq!(g.size_of(9, 12.0), 12.0, "缺位＝全域 fallback");
    }

    /// break_before_of 段首換頁：段 2 宣告換頁→該段**首行**必從新頁起頭
    /// （帶索引 0＝頁首）；首項宣告不換頁（游標前已 flush，段本就從頁首起，
    /// 不產空白頁）。bands 量化：段 1 每行佔 2 帶→緊鄰行帶距 2。
    #[test]
    fn para_grid_break_before_and_band_quantization() {
        let p = LayoutParams {
            page_w_pt: 400.0,
            page_h_pt: 300.0,
            margins_pt: [40.0; 4],
            font_size_pt: 12.0,
            line_spacing: 1.4,
            // 頁底預留 0：既有測試尺不預留（預留 0 逐位元組不變）。
            footnote_reserve_pt: 0.0,
            // 逐頁預留空表：既有測試尺不逐頁預留（空表逐位元組不變）。
            footnote_reserves_pt: Vec::new(),
        };
        let texts: Vec<String> = (0..4).map(|i| format!("第{i}段內容")).collect();

        // 段 2（索引 2）宣告 break_before：容量足以容全部 4 段（每段 1 行），
        // 換頁純由旗標驅動。
        let grid = ParaGrid {
            size_pt: vec![],
            bands: vec![1, 1, 1, 1],
            break_before: vec![false, false, true, false],
        };
        let eng = LayoutEngine::with_breaks_grid(
            &texts,
            p.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            grid,
        );
        assert_layout_invariants(&eng, "break_before");
        assert_eq!(eng.page_count(), 2, "段 2 起強推到新頁");
        assert_eq!(eng.pages()[1].lines[0].para_index, 2, "新頁以段 2 起頭");
        assert_eq!(
            eng.pages()[1].line_bands[0],
            0,
            "換頁後首行帶索引必回 0（頁首）"
        );

        // 首項（索引 0）宣告：不換頁、不產空白頁（游標語義，見 pack_run 注）。
        let first_flag = ParaGrid {
            break_before: vec![true],
            ..ParaGrid::default()
        };
        let eng0 = LayoutEngine::with_breaks_grid(
            &texts,
            p.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            first_flag,
        );
        assert_eq!(eng0.page_count(), 1, "首項宣告不換頁（段本就從頁首起）");

        // bands 量化：段 1 每行佔 2 帶（cap 內），緊鄰行帶距＝2。
        let band_grid = ParaGrid {
            size_pt: vec![],
            bands: vec![1, 2, 1, 1],
            break_before: vec![],
        };
        let engb = LayoutEngine::with_breaks_grid(
            &texts,
            p.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            band_grid,
        );
        assert_layout_invariants(&engb, "bands 量化");
        let bands: Vec<u32> = engb.pages()[0].line_bands.clone();
        assert_eq!(bands.len(), 4, "四段各 1 行同頁");
        assert_eq!(
            bands[2].saturating_sub(bands[1]),
            2,
            "段 1 行佔 2 帶：其後行帶距應 2，實得 {bands:?}"
        );
        // 飽和守衛（S2 柵欄面）：間隔超大 bands——正常行把 cur_used 抬到
        // (0, cap) 後，巨型帶段在**未換頁**分支直接相加：修前 debug panic
        // ／release 繞回；修後 saturating 收斂（飽和態 cur_used >= cap
        // 恆真＝每項強換頁，游標逐項前進、有限頁收尾）。
        let huge = ParaGrid {
            size_pt: vec![],
            bands: vec![1, u32::MAX, 1, u32::MAX],
            break_before: vec![],
        };
        let eng_h = LayoutEngine::with_breaks_grid(
            &texts,
            p.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            huge,
        );
        assert_eq!(
            eng_h.page_count(),
            2,
            "飽和語義：頁1＝正常段＋巨型段（溢出後飽和），頁2＝正常段＋巨型段"
        );
        let total_lines: usize = eng_h.pages().iter().map(|pg| pg.lines.len()).sum();
        assert_eq!(total_lines, texts.len(), "飽和態不得吞行");
        assert!(
            eng_h.page_count() <= texts.len() + 2,
            "飽和態不得爆炸性增頁：{}",
            eng_h.page_count()
        );
        assert_layout_invariants(&eng_h, "飽和巨型帶");
    }

    // ---------------- t_c1fb4c69 返修紅燈：S1-1/2/3＋S1-4 layout 側＋S2-7 ----------------
    // 基準來源：LibreOffice 實測（scratch/t_c1fb4c69/lo_ground.py，s1–s7 合成檔）。

    /// 小頁尺：cap＝(40−0)/10＝4 行/頁（font 10、行距 1.0、零邊界）。
    fn cap4_params() -> LayoutParams {
        LayoutParams {
            page_w_pt: 400.0,
            page_h_pt: 40.0,
            margins_pt: [0.0; 4],
            font_size_pt: 10.0,
            line_spacing: 1.0,
            // 頁底預留 0：既有測試尺不預留（預留 0 逐位元組不變）。
            footnote_reserve_pt: 0.0,
            // 逐頁預留空表：既有測試尺不逐頁預留（空表逐位元組不變）。
            footnote_reserves_pt: Vec::new(),
        }
    }

    /// S1-2（LO s2_standalone_lead）：文件首的獨立 br-page 段（空文字 `<w:p>`
    /// 只含 br，corpus 高頻形態）斷點不得消失——斷點屬空行**之後**，第一頁
    /// 是該空行，文字自第 2 頁起。現行 k=0（段前斷）＋cur 空＝flush no-op
    /// → 斷點整處被吞（1 頁 vs LO 2 頁）。
    #[test]
    fn standalone_br_page_at_doc_head_is_not_swallowed() {
        let p = cap4_params();
        let texts = vec![
            String::new(),
            "第一段文字".to_string(),
            "第二段文字".to_string(),
        ];
        // 純 br 段的 w:br 偏移＝碼點 0（record_page_break 於零文字段記 0）。
        let breaks = vec![vec![0usize], vec![], vec![]];
        let eng = LayoutEngine::with_breaks(&texts, p, Vec::new(), Vec::new(), Vec::new(), breaks);
        assert_layout_invariants(&eng, "文件首純 br 段");
        assert_eq!(eng.page_count(), 2, "LO 對照：空行獨佔頁 1，文字自頁 2");
        assert_eq!(eng.pages()[0].lines.len(), 1, "頁 1＝br 段的空行");
        assert!(eng.pages()[0].lines[0].text.is_empty());
        assert_eq!(eng.pages()[1].lines[0].para_index, 1, "第一段文字從頁 2 起");
    }

    /// S1-2（LO s1_standalone_mid）：文中間的獨立 br-page 段——斷點在空行之
    /// 後：空行留在前頁（與段前文字同頁），下一段自新頁起。現行 k=0 把空行
    /// 推落新頁（頁數同但斷點位置錯，LO 版面不同）。
    #[test]
    fn standalone_br_page_keeps_empty_line_on_prev_page() {
        let p = cap4_params();
        let texts = vec![
            "第一段文字".to_string(),
            String::new(),
            "第二段文字".to_string(),
        ];
        let breaks = vec![vec![], vec![0usize], vec![]];
        let eng = LayoutEngine::with_breaks(&texts, p, Vec::new(), Vec::new(), Vec::new(), breaks);
        assert_layout_invariants(&eng, "文中純 br 段");
        assert_eq!(eng.page_count(), 2);
        assert_eq!(
            eng.pages()[0].lines.len(),
            2,
            "頁 1＝第一段＋br 段空行（LO 同形）"
        );
        assert_eq!(
            eng.pages()[1].lines[0].para_index,
            2,
            "第二段自頁 2 起，空行不跟"
        );
    }

    /// S1-2（LO s3_double_standalone）：兩個連續獨立 br-page 段＝兩處斷點
    /// （各段空行各自收頁）：LO 3 頁。
    #[test]
    fn consecutive_standalone_br_pages_each_break() {
        let p = cap4_params();
        let texts = vec![
            "第一段文字".to_string(),
            String::new(),
            String::new(),
            "第二段文字".to_string(),
        ];
        let breaks = vec![vec![], vec![0usize], vec![0usize], vec![]];
        let eng = LayoutEngine::with_breaks(&texts, p, Vec::new(), Vec::new(), Vec::new(), breaks);
        assert_layout_invariants(&eng, "雙純 br 段");
        assert_eq!(eng.page_count(), 3, "LO 對照：兩個純 br 段各推一頁邊界");
        assert_eq!(eng.pages().last().unwrap().lines[0].para_index, 3);
    }

    /// S1-2 裁決鎖（LO s4/s5）：同位重複 br（同 run 連發、同碼點偏移）＝
    /// LibreOffice 合併為一處斷點（實測 2 頁，不產空白頁）——`dedup()` 的
    /// 同 k 合併與 LO 一致，本測試鎖死此裁決防回歸。
    #[test]
    fn same_offset_double_br_merges_per_lo_baseline() {
        let p = cap4_params();
        let texts = vec![
            "第一段文字".to_string(),
            String::new(),
            "第二段文字".to_string(),
        ];
        // 同 run 雙 br：record_page_break 於同一 cp 記兩次 → [0, 0]。
        let breaks = vec![vec![], vec![0usize, 0usize], vec![]];
        let eng = LayoutEngine::with_breaks(&texts, p, Vec::new(), Vec::new(), Vec::new(), breaks);
        assert_eq!(
            eng.page_count(),
            2,
            "LO 對照：同位雙 br＝一斷點，不疊空白頁"
        );
    }

    /// S1-3：段尾 br-page＋行內圖——圖與段落同頁（斷點在**圖之後**），下一
    /// 流項才換頁。現行雙錯：(a) hard_after 在圖項前就命中（圖被推落新頁，
    /// 與段落分離）；(b) 修正 (a) 後若 `prev_item_breaks_after` 不認 Image
    /// 項，斷點又整處消失。
    #[test]
    fn trailing_br_page_keeps_inline_image_then_breaks() {
        let p = img_params(); // line_h 16.8、cap 7
        let line_h = p.line_h_pt();
        let texts = vec!["段落文字".to_string(), "後段文字".to_string()];
        let brs = vec![vec![texts[0].chars().count()], vec![]]; // 段尾 br
        let imgs = vec![band(0, 30.0, line_h)]; // ceil(30/16.8)＝2 帶，掛段 0
        let eng = LayoutEngine::with_breaks(&texts, p, Vec::new(), Vec::new(), imgs, brs);
        assert_layout_invariants(&eng, "段尾 br＋行內圖");
        assert_eq!(eng.page_count(), 2, "斷點在圖之後：後段另起一頁");
        assert_eq!(eng.pages()[0].lines.len(), 1, "段文字留頁 1");
        assert_eq!(
            eng.pages()[0].images.len(),
            1,
            "圖與段落同頁（斷點在圖之後）"
        );
        assert_eq!(eng.pages()[1].lines[0].para_index, 1, "後段自頁 2 起");
    }

    /// 小頁尺幾何：cap＝(40−0)/10＝4 帶/頁（與 `cap4_params` 同尺）。
    fn small_geo(continuous: bool) -> SectionGeometry {
        SectionGeometry {
            page_w_pt: 400.0,
            page_h_pt: 40.0,
            margin_top_pt: 0.0,
            margin_bottom_pt: 0.0,
            margin_left_pt: 0.0,
            margin_right_pt: 0.0,
            landscape: false,
            continuous,
        }
    }

    /// S1-1（continuous 邊界＋頁滿）：continuous 只免「邊界強制換頁」，
    /// **不得**短路容量已滿——現行把第 5 行塞進 cap=4 的頁（溢出）。
    #[test]
    fn continuous_boundary_never_overflows_page_capacity() {
        let p = cap4_params();
        let one_line = "字".repeat(40); // content 400pt＝恰 1 行
        let texts: Vec<String> = (0..6).map(|_| one_line.clone()).collect();
        // 前 4 段 section 0（恰好填滿 cap=4），後 2 段 continuous section 1。
        let sections: Vec<SectionSpec> = (0..6)
            .map(|i| SectionSpec {
                id: if i < 4 { 0 } else { 1 },
                geo: small_geo(i >= 4),
            })
            .collect();
        let eng = LayoutEngine::with_sections(&texts, p, sections);
        assert!(
            eng.pages().iter().all(|pg| pg.lines.len() <= 4),
            "continuous 邊界不得讓頁超出容量：{:?}",
            eng.pages()
                .iter()
                .map(|pg| pg.lines.len())
                .collect::<Vec<_>>()
        );
        assert_eq!(eng.pages()[0].lines.len(), 4, "頁 1＝section 0 的四行");
        assert_eq!(eng.pages()[1].lines[0].para_index, 4, "邊界項因頁滿換頁");
    }

    /// S1-1（continuous 邊界＋br-page 硬斷點）：硬斷點在 continuous 邊界
    /// 同樣生效——現行邊界 arm 直接短路 `false`，斷點被吞。
    #[test]
    fn continuous_boundary_still_honors_hard_break() {
        let p = cap4_params();
        let one_line = "字".repeat(40);
        let texts = vec![one_line.clone(), one_line.clone()];
        let mut cont = letter_geo();
        cont.continuous = true;
        let sections = vec![
            SectionSpec {
                id: 0,
                geo: letter_geo(),
            },
            SectionSpec { id: 1, geo: cont },
        ];
        // 段 0 尾 br＝段 1 前硬斷點（游標在 (1,0,0) 的 hard_after）。
        let breaks = vec![vec![one_line.chars().count()], vec![]];
        let eng = LayoutEngine::with_breaks(&texts, p, sections, Vec::new(), Vec::new(), breaks);
        assert_eq!(eng.page_count(), 2, "continuous 邊界吞不掉 br 硬斷點");
        assert_eq!(eng.pages()[1].lines[0].para_index, 1);
    }

    /// S1-1（Page.section_id 蓋章）：continuous 同頁混節時，頁的 section
    /// 必取**頁首項**的 section（現行被末項覆寫→首頁 titlePg/頁首選擇錯）。
    #[test]
    fn continuous_shared_page_stamps_first_item_section() {
        let p = cap4_params();
        let one_line = "字".repeat(40);
        let texts: Vec<String> = (0..4).map(|_| one_line.clone()).collect();
        let mut cont = letter_geo();
        cont.continuous = true;
        let sections: Vec<SectionSpec> = (0..4)
            .map(|i| SectionSpec {
                id: if i < 2 { 0 } else { 1 },
                geo: if i < 2 { letter_geo() } else { cont },
            })
            .collect();
        let eng = LayoutEngine::with_sections(&texts, p, sections);
        assert_eq!(
            eng.page_count(),
            1,
            "continuous＋未滿頁＝同頁續排（既有行為）"
        );
        assert_eq!(
            eng.pages()[0].section_id,
            0,
            "混節頁蓋章取頁首項 section（titlePg 依此判定）"
        );
    }

    /// S1-4（layout 側防呆）：u32::MAX 帶數的畸形圖不得令打包算術溢出
    /// （debug 下 `cur_used + item_bands` panic）；照巨型圖規則整塊頂格、
    /// 前後段各自歸頁、不無限換頁。
    #[test]
    fn giant_image_bands_saturate_instead_of_overflowing() {
        let p = img_params();
        let texts = vec!["前段".to_string(), "後段".to_string()];
        let mut huge = band(0, 100.0, p.line_h_pt());
        huge.bands = u32::MAX;
        let pls = layout_paragraphs(&texts, &p);
        let (pages, _s) = paginate_tables_images(&pls, &p, &[], &[], &[huge]);
        assert_eq!(
            pages.len(),
            3,
            "前段 p1／巨圖頂格 p2／後段 p3，不得溢出或死迴圈"
        );
        assert_eq!(pages[1].images.len(), 1);
    }

    /// S2-7（跨頁段落錨點取首頁）：鎖點圖錨定跨頁段落——掛該段落**起頭頁**
    /// （現行 partition_point 只比 .0，段落跨頁時鉗到最後一頁）。
    #[test]
    fn floating_image_attaches_to_anchor_paragraph_first_page() {
        let p = cap4_params();
        let texts = vec!["字".repeat(400), "尾段".to_string()]; // 段 0＝10 行跨 3 頁
        let mut floats = vec![band(0, 100.0, p.line_h_pt())];
        floats[0].bands = 0;
        floats[0].anchor = Some(FloatingAnchor {
            x_pt: 0.0,
            y_pt: 0.0,
            x_frame: OriginFrame::Page,
            y_frame: OriginFrame::Page,
        });
        let eng = LayoutEngine::with_tables_images(&texts, p, Vec::new(), Vec::new(), floats);
        assert!(eng.page_count() >= 3);
        assert_eq!(
            eng.pages()[0].floating_images.len(),
            1,
            "跨頁段落的鎖點圖掛段落起頭頁（非末頁）"
        );
        for pg in &eng.pages()[1..] {
            assert!(pg.floating_images.is_empty(), "單圖不得雙掛");
        }
    }

    /// S2-7（無浮圖早退）：純內嵌圖（無任何 anchor）文件，編輯重排不得觸發
    /// 浮圖附掛的全頁 clear＋全 flow 重掃（debug 計數器斷言）。
    #[cfg(debug_assertions)]
    #[test]
    fn inline_only_doc_skips_floating_attach_scan() {
        let p = img_params();
        let texts = vec!["前段".to_string(), "後段".to_string()];
        let eng_imgs = vec![band(0, 30.0, p.line_h_pt())]; // anchor=None：純內嵌
        let mut eng = LayoutEngine::with_tables_images(&texts, p, Vec::new(), Vec::new(), eng_imgs);
        let before = floating_attach_scans();
        eng.replace_paragraph_text(1, "後段被編輯成更長的一段文字。");
        let after = floating_attach_scans();
        assert_eq!(before, after, "無鎖點圖的重排不得走附掛全掃");
    }

    /// S1-2（LO s8_page_head_br）：段中 br 換頁後，**頁首**又遇獨立 br 段
    /// ——空行留在新頁（斷點在空行之後），其後段落再推一頁：LO 3 頁。
    #[test]
    fn standalone_br_page_at_page_head_breaks_after_empty_line() {
        let p = cap4_params();
        let text = format!("{}{}", "甲".repeat(30), "乙".repeat(30)); // br at 30
        let texts = vec!["前段".to_string(), text, String::new(), "後段".to_string()];
        let breaks = vec![
            vec![],
            vec![30usize], // 段中 br
            vec![0usize],  // 純 br 段
            vec![],
        ];
        let eng = LayoutEngine::with_breaks(&texts, p, Vec::new(), Vec::new(), Vec::new(), breaks);
        assert_layout_invariants(&eng, "頁首純 br 段");
        assert_eq!(eng.page_count(), 3, "LO 對照：頁 2 尾空行、後段落頁 3");
        assert_eq!(
            eng.pages()[1].lines.last().unwrap().para_index,
            2,
            "空行留頁 2 尾"
        );
        assert_eq!(eng.pages()[2].lines[0].para_index, 3);
    }

    /// S1-2（LO s10_lead_double）：文件首**兩個**獨立 br 段＝兩處斷點：
    /// 空行各自收頁（頁 1 一行、頁 2 一行），文字自頁 3：LO 3 頁。
    #[test]
    fn two_standalone_br_paragraphs_at_doc_head_make_two_boundaries() {
        let p = cap4_params();
        let texts = vec![String::new(), String::new(), "第一段".to_string()];
        let breaks = vec![vec![0usize], vec![0usize], vec![]];
        let eng = LayoutEngine::with_breaks(&texts, p, Vec::new(), Vec::new(), Vec::new(), breaks);
        assert_layout_invariants(&eng, "文件首雙 br 段");
        assert_eq!(eng.page_count(), 3, "LO 對照：兩個純 br 段各推一頁邊界");
        assert_eq!(eng.pages().last().unwrap().lines[0].para_index, 2);
    }

    /// S1-2（LO s11_diff_offsets）：同 run **不同偏移**雙 br（甲|乙|丙）＝
    /// 真兩處斷點（段中口徑，既有 KP 硬收行路徑）：LO 3 頁。
    #[test]
    fn same_run_br_at_different_offsets_makes_two_breaks() {
        let p = cap4_params();
        let texts = vec!["前段".to_string(), "甲乙丙".to_string(), "後段".to_string()];
        // record_page_break 記碼點偏移：甲後＝1、乙後＝2。
        let breaks = vec![vec![], vec![1usize, 2usize], vec![]];
        let eng = LayoutEngine::with_breaks(&texts, p, Vec::new(), Vec::new(), Vec::new(), breaks);
        assert_eq!(eng.page_count(), 3, "LO 對照：不同偏移雙 br＝兩處斷點");
    }

    fn text_only(t: &str) -> Vec<String> {
        vec![t.to_string()]
    }

    // ---------------- F2a：頁底預留帶（機制卡） ----------------
    //
    // 本區只驗「機制」：`LayoutParams::footnote_reserve_pt` 能讓每頁容量
    // （帶）依預留高縮減，且預留 0 時逐位元組等於舊公式。
    // 「每頁該預留多少」（引擎端依實際註腳量算）＝下一張卡 F2b，不在本區。

    /// F2a 測試尺：頁高 842、上下邊界 72、字級 16pt、行距 1.0。
    /// line_h＝16.0（二進制可精確表示）、版心高＝842−72−72＝698.0、
    /// 容量＝floor(698/16)＝43 帶（698/16＝43.625）。全用可精確表示的值，
    /// 預留 2×line_h 時的量化差**恰為 2**（不是近似值）。
    fn f2a_params() -> LayoutParams {
        LayoutParams {
            page_w_pt: 595.0,
            page_h_pt: 842.0,
            margins_pt: [72.0; 4],
            font_size_pt: 16.0,
            line_spacing: 1.0,
            footnote_reserve_pt: 0.0,
            // 逐頁預留空表：既有測試尺不逐頁預留（空表逐位元組不變）。
            footnote_reserves_pt: Vec::new(),
        }
    }

    /// F2a 長文件：86 個單行段（＝86 帶）。預留 0 時 43＋43＝2 頁；
    /// 預留 2 帶（2×line_h＝32pt）時容量 41，41＋41＋4＝3 頁。
    fn f2a_long_texts() -> Vec<String> {
        (0..86).map(|_| "字".to_string()).collect()
    }

    /// F2a(a)：同一份長文件輸入，reserve=0 vs reserve=2×line_h——
    /// 後者首頁 `line_count` **恰少 2**（精確等式，不是 `<`）。
    ///
    /// 路徑：`page_breaks`（純函式分頁主入口）→ `paginate_per_section` →
    /// `paginate_flow` → `pack_run`（容量取 `section_capacity`）。
    #[test]
    fn footnote_reserve_two_bands_shrinks_first_page_exactly_two_lines() {
        let p0 = f2a_params();
        let mut p2 = f2a_params();
        p2.footnote_reserve_pt = 2.0 * p2.line_h_pt();
        let texts = f2a_long_texts();

        let brk0 = page_breaks(&texts, &p0);
        let brk2 = page_breaks(&texts, &p2);
        assert_eq!(brk0[0].line_count, 43, "預留 0：容量 43 帶");
        assert_eq!(brk2[0].line_count, 41, "預留 2 帶：容量 41 帶");
        assert_eq!(
            brk0[0].line_count - brk2[0].line_count,
            2,
            "首頁行數恰少 2（精確等式，不是 <）"
        );
        assert!(
            brk2.len() > brk0.len(),
            "預留讓正文區變矮：總頁數必增（{} → {}）",
            brk0.len(),
            brk2.len()
        );
    }

    /// F2a(b)：預留 0 時 `per_page_lines()` 與手算舊公式
    /// `(content_h/line_h).floor()` **精確相等**（零變化守門）。
    ///
    /// 舊公式＝改碼前 `per_page_lines()` 的逐字複製（版心高不扣預留）。
    #[test]
    fn footnote_reserve_zero_matches_legacy_capacity_formula() {
        let legacy = |p: &LayoutParams| -> usize {
            let line_h = p.line_h_pt();
            let content_h =
                (p.page_h_pt - p.margin(MARGIN_TOP) - p.margin(MARGIN_BOTTOM)).max(line_h);
            ((content_h / line_h).floor() as usize).max(1)
        };
        for p in [f2a_params(), LayoutParams::default()] {
            let mut zero = p.clone();
            zero.footnote_reserve_pt = 0.0;
            assert_eq!(
                zero.per_page_lines(),
                legacy(&p),
                "預留 0 必與舊公式逐位元組等值"
            );
            assert_eq!(zero.per_page_lines(), p.per_page_lines());
        }
        // A4 後備值：容量 41 帶（golden pagination.jsonl 的既有數字）。
        assert_eq!(LayoutParams::default().per_page_lines(), 41);
    }

    /// F2a(c)：病態預留不得 panic、容量恆 ≥1；負值／NaN 一律視同 0
    /// （與 reserve=0 等值）。
    #[test]
    fn footnote_reserve_pathological_values_are_clamped() {
        let p0 = f2a_params();
        let cap0 = p0.per_page_lines();

        // 預留超過可用高（版心被壓成負）→ 兜底到 1 帶，不 panic。
        let mut huge = f2a_params();
        huge.footnote_reserve_pt = huge.page_h_pt;
        assert_eq!(huge.per_page_lines(), 1, "病態預留：容量兜底 1 帶");
        assert!(huge.per_page_lines() >= 1, "容量恆 ≥1 帶");

        // 無窮大預留同樣兜底（不得 panic、不得為 0）。
        let mut inf = f2a_params();
        inf.footnote_reserve_pt = f32::INFINITY;
        assert_eq!(inf.per_page_lines(), 1, "inf 預留：容量兜底 1 帶");

        // 負值／NaN 一律視同 0（與 reserve=0 等值）。
        let mut neg = f2a_params();
        neg.footnote_reserve_pt = -50.0;
        assert_eq!(neg.per_page_lines(), cap0, "負值視同 0");
        let mut nan = f2a_params();
        nan.footnote_reserve_pt = f32::NAN;
        assert_eq!(nan.per_page_lines(), cap0, "NaN 視同 0");
        assert!(nan.per_page_lines() >= 1, "容量恆 ≥1 帶");
    }

    /// F2a(d)：端到端——經**分頁主入口 `LayoutEngine::new` → `rebuild_full`
    /// → `paginate_flow` → `pack_run`** 餵 reserve≠0 的 params：總頁數增加，
    /// 且首頁末行不進頁底預留區。
    ///
    /// 預留區＝[頁高−下邊界−預留, 頁高−下邊界]；行頂 y＝上邊界＋帶序×行高。
    #[test]
    fn footnote_reserve_end_to_end_via_pagination_entry() {
        let p0 = f2a_params();
        let mut p2 = f2a_params();
        let reserve = 2.0 * p2.line_h_pt();
        p2.footnote_reserve_pt = reserve;
        let texts = f2a_long_texts();

        let eng0 = LayoutEngine::new(&texts, p0);
        let eng2 = LayoutEngine::new(&texts, p2.clone());
        assert!(
            eng2.page_count() > eng0.page_count(),
            "預留 {}pt：總頁數必增（{} → {}）",
            reserve,
            eng0.page_count(),
            eng2.page_count()
        );

        let pg = &eng2.pages()[0];
        let band = *pg.line_bands.last().expect("首頁必有行") as f32;
        let line_h = p2.line_h_pt();
        let top = pg.margin_top_pt + band * line_h;
        let zone_start = p2.page_h_pt - p2.margin(MARGIN_BOTTOM) - reserve;
        assert!(
            top < zone_start,
            "首頁末行頂不得進預留區（top={top} zone_start={zone_start}）"
        );
        assert!(
            top + line_h <= zone_start,
            "首頁末行底也不得進預留區（bottom={} zone_start={}）",
            top + line_h,
            zone_start
        );
    }

    // ---------------- F2b：逐頁預留表＋迭代收斂（機制卡） ----------------
    //
    // 本區只驗「機制」：`LayoutParams::footnote_reserves_pt`（逐頁頁底預留表）
    // 讓**每頁**的容量各自獨立，且提供「預留→重排→再算預留」的迭代收斂入口
    // （`paginate_converged`）。「哪頁該留多少」（引擎端依實際註腳量算）＝
    // 下一張卡 F2b2 接線，不在本區——本區除測試外無任何消費端。
    //
    // 測試尺沿用 F2a 尺（`f2a_params`）：字級 16pt／行距 1.0（line_h＝16.0，
    // 二進制可精確表示）、版心 698pt、容量 43 帶。預留 2 帶（32pt）時量化差
    // **恰為 2**（不是近似值），故本區的等式斷言都是精確等式。

    /// F2b 長文件：127 個單行段（＝127 帶）。
    ///
    /// 逐頁預留 [0, 2 帶, 0] 生效時三頁容量 43/41/43＝127（恰好排滿不溢出）；
    /// 無逐頁預留時為 43/43/41。
    fn f2b_long_texts(n: usize) -> Vec<String> {
        (0..n).map(|_| "字".to_string()).collect()
    }

    /// 逐欄位比對兩組分頁斷點（零變化守門用：欄位級等價才叫「零變化」）。
    fn assert_breaks_field_equal(a: &[PageBreak], b: &[PageBreak], ctx: &str) {
        assert_eq!(
            a.len(),
            b.len(),
            "{ctx}：頁數不同（{} vs {}）",
            a.len(),
            b.len()
        );
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.page, y.page, "{ctx}：page 欄位不同");
            assert_eq!(x.first_para, y.first_para, "{ctx}：first_para 欄位不同");
            assert_eq!(x.last_para, y.last_para, "{ctx}：last_para 欄位不同");
            assert_eq!(x.line_count, y.line_count, "{ctx}：line_count 欄位不同");
            assert_eq!(
                x.continues_paragraph, y.continues_paragraph,
                "{ctx}：continues_paragraph 欄位不同"
            );
            assert_eq!(x.width_pt, y.width_pt, "{ctx}：width_pt 欄位不同");
            assert_eq!(x.height_pt, y.height_pt, "{ctx}：height_pt 欄位不同");
            assert_eq!(
                x.starts_section, y.starts_section,
                "{ctx}：starts_section 欄位不同"
            );
        }
    }

    /// F2b(a)：逐頁預留表 [0, 2 帶, 0]——**第 2 頁行數恰少 2**，第 1/3 頁
    /// 不受影響（精確等式，不是近似）。
    ///
    /// 路徑：`page_breaks` → `paginate_per_section` → `paginate_flow` →
    /// `pack_run`（容量取 `section_capacity(p, geo, 頁序)`）。
    #[test]
    fn per_page_reserve_affects_only_its_own_page() {
        let p0 = f2a_params();
        let mut p2 = f2a_params();
        p2.footnote_reserves_pt = vec![0.0, 2.0 * p2.line_h_pt(), 0.0];
        let texts = f2b_long_texts(127);

        let brk0 = page_breaks(&texts, &p0);
        let brk2 = page_breaks(&texts, &p2);

        assert_eq!(brk0.len(), 3, "無預留：127 帶＝43＋43＋41 → 3 頁");
        assert_eq!(
            brk2.len(),
            3,
            "逐頁預留：43＋41＋43 → 3 頁（第 3 頁未溢出）"
        );
        assert_eq!(
            brk2[0].line_count, brk0[0].line_count,
            "第 1 頁不受影響（精確等式）"
        );
        assert_eq!(brk2[1].line_count, 41, "第 2 頁容量 43−2＝41 帶");
        assert_eq!(
            brk2[1].line_count,
            brk0[1].line_count - 2,
            "第 2 頁行數恰少 2（精確等式，不是 <）"
        );
        assert_eq!(
            brk2[2].line_count, 43,
            "第 3 頁容量未縮（排滿 43 帶——若該頁也被預留則最多 41 帶）"
        );

        // 同一機制經**引擎主入口**（`LayoutEngine::new` → `rebuild_full` →
        // `paginate_flow` → `pack_run`）必同結果。
        let eng2 = LayoutEngine::new(&texts, p2);
        assert_eq!(eng2.page_count(), 3, "引擎路徑同為 3 頁");
        assert_eq!(eng2.pages()[0].lines.len(), 43, "引擎路徑第 1 頁 43 帶");
        assert_eq!(eng2.pages()[1].lines.len(), 41, "引擎路徑第 2 頁 41 帶");
        assert_eq!(eng2.pages()[2].lines.len(), 43, "引擎路徑第 3 頁 43 帶");
    }

    /// F2b(b)：**逐頁預留表空 vs 根本不設此欄**——`page_breaks` 逐欄位相等
    /// （零變化守門：空表＝無逐頁預留，渲染輸出必須逐位元組零變化）。
    ///
    /// 「不設」＝以 `..LayoutParams::default()` 補其餘欄位（此欄取 Default
    /// ＝空表）；「空表」＝明確給一個空 Vec；另比「全 0 表」——全 0 的逐頁
    /// 表走的是逐頁分支（max(基底, 0)），結果必與空表同值。
    #[test]
    fn empty_reserve_table_is_field_identical_to_unset() {
        // 「不設」：逐頁預留欄不寫，取 Default（空表）。
        let mut p_unset = LayoutParams {
            page_w_pt: 595.0,
            page_h_pt: 842.0,
            margins_pt: [72.0; 4],
            font_size_pt: 16.0,
            line_spacing: 1.0,
            footnote_reserve_pt: 0.0,
            ..LayoutParams::default()
        };
        // 「空表」：明確空 Vec。
        let mut p_empty = p_unset.clone();
        p_empty.footnote_reserves_pt = Vec::new();
        // 「全 0 表」：走逐頁分支但每頁預留皆 0。
        let mut p_zeros = p_unset.clone();
        p_zeros.footnote_reserves_pt = vec![0.0; 8];
        let texts = f2b_long_texts(127);

        let brk_unset = page_breaks(&texts, &p_unset);
        let brk_empty = page_breaks(&texts, &p_empty);
        let brk_zeros = page_breaks(&texts, &p_zeros);
        assert_breaks_field_equal(&brk_unset, &brk_empty, "空表 vs 不設");
        assert_breaks_field_equal(&brk_unset, &brk_zeros, "全 0 表 vs 不設");

        // 文件層級預留非 0 時同樣成立（逐頁分支不得擾動基底路徑）。
        p_unset.footnote_reserve_pt = 32.0;
        p_empty.footnote_reserve_pt = 32.0;
        p_zeros.footnote_reserve_pt = 32.0;
        let brk_unset2 = page_breaks(&texts, &p_unset);
        let brk_empty2 = page_breaks(&texts, &p_empty);
        let brk_zeros2 = page_breaks(&texts, &p_zeros);
        assert_breaks_field_equal(&brk_unset2, &brk_empty2, "有基底預留：空表 vs 不設");
        assert_breaks_field_equal(&brk_unset2, &brk_zeros2, "有基底預留：全 0 表 vs 不設");
    }

    /// F2b(c)：文件層級 10pt ＋ 逐頁表 [0, 30pt]——第 2 頁取 **30**（不是 10、
    /// 也不是 10＋30＝40），其餘頁取 **10**（max 疊加口徑測死）。
    #[test]
    fn doc_level_and_per_page_reserves_take_max_not_sum() {
        let mut p = f2a_params();
        p.footnote_reserve_pt = 10.0;
        p.footnote_reserves_pt = vec![0.0, 30.0];
        let geo = effective_section(&p, &[], 0).geo;

        assert_eq!(effective_reserve_pt(&p, 0), 10.0, "第 1 頁：max(10, 0)＝10");
        assert_eq!(
            effective_reserve_pt(&p, 1),
            30.0,
            "第 2 頁：max(10, 30)＝30"
        );
        assert_eq!(
            effective_reserve_pt(&p, 2),
            10.0,
            "第 3 頁：表外 → max(10, 0)＝10"
        );

        // 容量口徑（版心 698pt、行高 16pt）：10pt → 43 帶；30pt → 41 帶。
        assert_eq!(section_capacity(&p, &geo, 0), 43, "第 1 頁容量 43 帶");
        assert_eq!(section_capacity(&p, &geo, 1), 41, "第 2 頁容量 41 帶");
        assert_eq!(section_capacity(&p, &geo, 2), 43, "第 3 頁容量 43 帶");

        // 相加口徑會被容量區分出來（挑一組 max ≠ sum 的數）：max(10, 90)＝90
        // → 698−90＝608 → 38 帶；若相加（10＋90＝100）→ 598 → 37 帶。
        let mut p2 = f2a_params();
        p2.footnote_reserve_pt = 10.0;
        p2.footnote_reserves_pt = vec![0.0, 90.0];
        assert_eq!(
            section_capacity(&p2, &geo, 1),
            38,
            "max 口徑：90pt → 38 帶（相加口徑會是 37 帶）"
        );
    }

    /// F2b(d)：迭代收斂入口——reserve_fn 回「第 1 頁留 2 帶、其餘 0」→
    /// 第 2 輪預留不變即收斂（輪數 ≤3）；reserve_fn 回全 0 → **恰 1 輪**
    /// （預留恆 0 時第 1 輪即收斂）。
    #[test]
    fn paginate_converged_reaches_stable_reserves() {
        let p = f2a_params();
        let line_h = p.line_h_pt();
        let texts = f2a_long_texts();

        // 情形一：reserve_fn 固定回「第 1 頁 2 帶、其餘 0」。
        let (brks, rounds) = paginate_converged(&texts, &p, &|_a| vec![2.0 * line_h, 0.0]);
        assert!(
            rounds <= MAX_CONVERGE_ROUNDS,
            "收斂輪數必 ≤ 上限（實得 {rounds}）"
        );
        assert_eq!(rounds, 2, "第 1 輪算出非零預留 → 第 2 輪預留不變 → 收斂");
        // 收斂後的結果＝以該預留表重排的結果（分頁是預留向量的純函數）。
        let mut expect = p.clone();
        expect.footnote_reserves_pt = vec![2.0 * line_h, 0.0];
        assert_eq!(
            brks,
            page_breaks(&texts, &expect),
            "收斂結果必與以該預留重排一致"
        );

        // 情形二：reserve_fn 回全 0 → 恰 1 輪（預留恆 0，第 1 輪即收斂）。
        let (brks0, rounds0) = paginate_converged(&texts, &p, &|_a| vec![0.0; 2]);
        assert_eq!(rounds0, 1, "預留恆 0：第 1 輪即收斂");
        assert_eq!(
            brks0,
            page_breaks(&texts, &p),
            "全 0 預留的結果＝無預留的結果"
        );
    }

    /// F2b(e)：病態逐頁預留不得 panic——NaN／負值視同 0（與 F2a 同款口徑）、
    /// 超大值把容量兜底到 1 帶（容量恆 ≥1）。
    #[test]
    fn per_page_reserve_pathological_values_are_clamped() {
        let p0 = f2a_params();
        let geo = effective_section(&p0, &[], 0).geo;
        let cap0 = section_capacity(&p0, &geo, 0);

        let mut p = f2a_params();
        p.footnote_reserves_pt = vec![f32::NAN, -50.0, 1.0e9, f32::INFINITY];

        assert_eq!(effective_reserve_pt(&p, 0), 0.0, "NaN 視同 0");
        assert_eq!(effective_reserve_pt(&p, 1), 0.0, "負值視同 0");
        assert_eq!(
            section_capacity(&p, &geo, 0),
            cap0,
            "NaN 視同 0 → 容量與無預留同值"
        );
        assert_eq!(
            section_capacity(&p, &geo, 1),
            cap0,
            "負值視同 0 → 容量與無預留同值"
        );
        assert_eq!(section_capacity(&p, &geo, 2), 1, "超大預留：容量兜底 1 帶");
        assert_eq!(section_capacity(&p, &geo, 3), 1, "inf 預留：容量兜底 1 帶");
        assert!(section_capacity(&p, &geo, 2) >= 1, "容量恆 ≥1 帶");

        // 端到端不得 panic（分頁必產出結果）。
        let texts = f2a_long_texts();
        let brks = page_breaks(&texts, &p);
        assert!(!brks.is_empty(), "病態預留仍須產出分頁結果");
    }

    /// F2b(f)：逐頁預留表長短於／長於頁數——缺的視同 0、多的忽略，皆不 panic。
    #[test]
    fn per_page_reserve_table_length_mismatch_is_tolerated() {
        let line_h = f2a_params().line_h_pt();
        let texts = f2b_long_texts(127);

        // 表比頁數短：只有第 1 頁有預留（容量 41），其餘頁視同 0（容量 43）。
        let mut short = f2a_params();
        short.footnote_reserves_pt = vec![2.0 * line_h];
        let brk_short = page_breaks(&texts, &short);
        assert_eq!(brk_short.len(), 3, "41＋43＋43＝127 → 3 頁");
        assert_eq!(brk_short[0].line_count, 41, "第 1 頁有預留：41 帶");
        assert_eq!(brk_short[1].line_count, 43, "表外頁視同 0：43 帶");
        assert_eq!(brk_short[2].line_count, 43, "表外頁視同 0：43 帶");

        // 表比頁數長：多餘的項被忽略（不 panic、不影響既有頁）。
        let mut long = f2a_params();
        long.footnote_reserves_pt = vec![2.0 * line_h, 0.0, 0.0, 0.0, 0.0];
        let brk_long = page_breaks(&texts, &long);
        assert_breaks_field_equal(&brk_short, &brk_long, "表長於頁數：多餘項忽略");
    }
}
