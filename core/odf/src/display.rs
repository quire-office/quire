//! ODF-D1 顯示列表發射層：`OdfProjection` ＋ `LayoutParams` → `PageDisplayList`。
//!
//! 定位：ODT 線的「幾何＋文字」發射面，輸出與 DOCX 線**同一個**顯示列表契約，
//! JS 端（`shells/web/demo/main.js`）零改就能畫。型別直接取自
//! [`quire_render_trait`]——那是全倉庫顯示列表契約的**定義處**
//! （`core/render-trait/src/lib.rs`，`PageDisplayList` 在該檔 §89），engine-api
//! 本身也是 re-export 它（`core/engine-api/src/lib.rs` 檔頭 `pub use
//! quire_render_trait::{...}`）。本 crate 依賴它是**向下邊**（render-trait 零
//! core 依賴、只吃 serde），與 `layout_glue.rs` 分層鐵律（嚴禁引用
//! `quire_ooxml::Paragraph`——不跨線借 DOCX 模型）不衝突：本層只吃
//! `OdfProjection` ＋ `quire_layout` 輸出，零 ooxml 型別。
//!
//! 同源性鐵條：**不另烤一套欄位**。欄位名/序列化行為由 render-trait 的
//! `serde` derive 決定（不准字串拼接造 JSON），故兩線 JSON 契約不可能各自漂移；
//! 真正的防漂移鎖在 `core/odf/tests/odf_d1_display_list.rs` 燈 1（對照
//! `fixtures/display_list_contract.json`，該檔由 DOCX 線真實 `dump` 產物抽出）
//! ＋ `shells/native/tests/odf_d1_dump_contract.rs`（DOCX 側反查同一基準檔）。
//!
//! 座標系（與 DOCX 線逐欄同義）：原點＝頁左上角，單位 pt，y 向下為正；
//! `Text.y`＝行網格帶頂端（渲染端自加字級得基線，見 `main.js` 的
//! `baselineY = (t.y + t.size_pt) * scale`），`Text.x`＝版心左緣＋行內位移。
//!
//! 發射面覆蓋（D1 切片，解剖報告 §4）：**只發文字項**。圖片／表格／頁首尾／
//! 註腳／段落框線／分頁框實體項一律不發，記帳 D1b。缺件欄位發中性值：
//! 字色＝0xFF000000（不透明黑）、`family`＝None（serde 跳欄，渲染端用自身預設）、
//! `flags` 只帶 [`FLAG_BOLD`]（斜體/底線/刪除線未採集＝不發，D1b）。
//!
//! 幾何來源：斷行＝[`layout_projection`]（逐段代表字級斷行，ODF-C3 既有入口），
//! 分頁＝`quire_layout::paginate_per_section`（無 section＝單一本 params 幾何）。
//! 每行發 **逐 run 同質段**（與 DOCX 線 `emit_line_run_items` 同款口徑）：
//! 行內按 `OdfLayoutParagraph::runs` 的碼點區間切段，段自帶該 run 的字級/字重，
//! x 由前段 shaped advance 推進（同一內嵌字型尺，雙殼一致）。

use serde_json::Value;

use crate::layout_glue::{layout_projection, project, LayoutParams, OdfProjection};
use crate::{OdfBundle, OdfError};

// 契約型別再匯出：殼層（quire-cli / shells/web）經本 crate 取用，不直接
// import quire-render-trait（分層紀律同 `layout_glue::LayoutParams` 再匯出）。
pub use quire_render_trait::{
    DrawItem, PageDisplayList, Renderer, FLAG_BOLD, FLAG_ITALIC, FLAG_STRIKE, FLAG_UNDERLINE,
};
// 行網格/邊界索引常量同樣再匯出（考卷與殼層據此對照，不直接 import quire-layout）。
pub use quire_layout::{MARGIN_BOTTOM, MARGIN_LEFT, MARGIN_RIGHT, MARGIN_TOP};

/// 缺件欄位中性值：字色＝不透明黑（與 render-trait 的 `default_opaque_black`
/// 同值；ODF 投影目前不採集 `fo:color`，記帳 D1b）。
pub const DEFAULT_TEXT_COLOR_ARGB: u32 = 0xFF00_0000;

/// 發射全部頁的顯示列表（純讀取：不改 `projection` 任何欄位）。
///
/// 頁序連續 0..n；每頁 `width_pt`/`height_pt` 取自該頁起頭 section 的實際蓋章
/// （無 section 時＝`params` 的頁面欄位），與 DOCX 線 `page_display_list` 同口徑。
pub fn emit_display_lists(
    projection: &OdfProjection,
    params: &LayoutParams,
) -> Vec<PageDisplayList> {
    // 回塞鎖（BUGGY 鑑別，同 `odf_encrypted_gate` 的 `ODF_E1_BUGGY` 紀律）：設
    // `ODF_D1_BUGGY` 環境變數時退化為「佔位實作」＝零頁零項，考卷必須拍紅
    // （實測紀錄見 `docs/odf-d1-redlight.md`），證明四盞燈真的在檢查東西。
    // 正常路徑（未設該變數）零影響。
    if std::env::var("ODF_D1_BUGGY").is_ok() {
        return Vec::new();
    }
    let pls = layout_projection(projection, params);
    let pages = quire_layout::paginate_per_section(&pls, params, &[]);
    let line_h = params.line_h_pt();
    let margin_right = params.margins_pt[MARGIN_RIGHT];
    pages
        .iter()
        .map(|pg| {
            let mut items: Vec<DrawItem> = Vec::with_capacity(pg.lines.len());
            let content_w = (pg.width_pt - pg.margin_left_pt - margin_right).max(0.0);
            for (i, line) in pg.lines.iter().enumerate() {
                let band = pg.line_bands.get(i).copied().unwrap_or(i as u32) as f32;
                let y = pg.margin_top_pt + band * line_h;
                let para = projection.paragraphs.get(line.para_index);
                let props = projection.paragraph_properties.get(line.para_index);
                // [ODT_INDENT_BUGGY_POINT: LINE_BASE_X]
                let base_x = compute_line_x(
                    pg.margin_left_pt,
                    content_w,
                    line.width_pt,
                    line.char_offset,
                    props,
                );
                push_line_items(&mut items, base_x, y, line, para);
            }
            PageDisplayList {
                page: pg.index,
                width_pt: pg.width_pt,
                height_pt: pg.height_pt,
                items,
            }
        })
        .collect()
}

/// 單頁顯示列表 JSON（與 DOCX 線 `Engine::page_display_list_json` 同形：
/// serde 序列化的 `PageDisplayList`， externally-tagged enum 以變體名 `Text` 為鍵）。
pub fn display_list_json(lists: &[PageDisplayList], page: usize) -> Option<String> {
    lists
        .get(page)
        .and_then(|dl| serde_json::to_string(dl).ok())
}

/// 全部頁顯示列表 JSON（pretty 陣列；與 `quire-cli dump` 同形，供 QA 與
/// 後續 odt render_lab 比對）。
pub fn dump_display_lists_json(lists: &[PageDisplayList]) -> String {
    serde_json::to_string_pretty(lists).unwrap_or_else(|_| "[]".to_string())
}

/// 一份 .odt 的發射快取面：開檔一次、投影一次、分頁一次，之後逐頁取 JSON
/// （wasm 殼 `new_odt(bytes)` 的後端；重繪同一頁不再重跑斷行/分頁）。
#[derive(Debug, Clone)]
pub struct OdtView {
    projection: OdfProjection,
    params: LayoutParams,
    lists: Vec<PageDisplayList>,
}

impl OdtView {
    /// 由已開封的 ODF 封裝建視圖（走 [`project`] 的加密檢驗閘門，老實報錯不吞）。
    pub fn from_bundle(bundle: &OdfBundle, params: &LayoutParams) -> Result<Self, OdfError> {
        Ok(Self::from_projection(project(bundle)?, params))
    }

    /// 由 .odt 位元組建視圖（wasm 無 fs：殼層讀檔後傳位元組進來）。
    pub fn from_odt_bytes(bytes: &[u8], params: &LayoutParams) -> Result<Self, OdfError> {
        Self::from_bundle(&OdfBundle::open(bytes)?, params)
    }

    /// 由既有投影建視圖（純函數入口；測試與 engine 端共用）。
    pub fn from_projection(projection: OdfProjection, params: &LayoutParams) -> Self {
        let lists = emit_display_lists(&projection, params);
        Self {
            projection,
            params: params.clone(),
            lists,
        }
    }

    /// 頁數（分頁器給的實際頁數）。
    pub fn page_count(&self) -> usize {
        self.lists.len()
    }

    /// 段落數（text:p＋text:h 入列後的段落數，同 [`OdfProjection::paragraph_count`]）。
    pub fn paragraph_count(&self) -> usize {
        self.projection.paragraph_count()
    }

    /// 段落文字總碼點數（老實回報口徑，同 odt-render）。
    pub fn total_text_len(&self) -> usize {
        self.projection.total_text_len
    }

    /// 本視圖使用的排版參數（分頁後備值；ODF `style:page-layout` 未解析＝缺口記帳）。
    pub fn params(&self) -> &LayoutParams {
        &self.params
    }

    pub fn projection(&self) -> &OdfProjection {
        &self.projection
    }

    pub fn page_display_list(&self, page: usize) -> Option<&PageDisplayList> {
        self.lists.get(page)
    }

    pub fn page_display_list_json(&self, page: usize) -> Option<String> {
        display_list_json(&self.lists, page)
    }

    /// 全頁 JSON（`dump` 同形）。
    pub fn dump_json(&self) -> String {
        dump_display_lists_json(&self.lists)
    }
}

/// 計算單行起始 x 座標（消費段落對齊與縮排）。
///
/// 規格：
/// - `align == Center`：`x = margin_left + max(0, (content_w − line_w)/2)`
/// - `align == End/Right`：`x = margin_left + max(0, content_w − line_w)`
/// - 首行：`margin_left + max(0, indent_left + indent_first_line)`；次行起：`margin_left + indent_left`
///   （負 first-line 不得讓 x 小於版心左緣，夾到 `margin_left + min(indent_left, max(0, indent_left+indent_first))` 邊界）
fn compute_line_x(
    margin_left: f32,
    content_w: f32,
    line_w: f32,
    char_offset: usize,
    props: Option<&crate::layout_glue::ParagraphProperties>,
) -> f32 {
    let Some(p) = props else {
        return margin_left;
    };

    match p.align {
        Some(crate::layout_glue::Align::Center) => {
            margin_left + (content_w - line_w).max(0.0) / 2.0
        }
        Some(crate::layout_glue::Align::End) | Some(crate::layout_glue::Align::Right) => {
            margin_left + (content_w - line_w).max(0.0)
        }
        _ => {
            let indent_left = p.indent_left_pt.unwrap_or(0.0);
            let indent_first = p.indent_first_line_pt.unwrap_or(0.0);
            if char_offset == 0 {
                let offset = if indent_first < 0.0 {
                    // 負 first-line（懸掛縮排）：不得小於版心左緣，夾到 min(indent_left, max(0, indent_left + indent_first))
                    indent_left.min((indent_left + indent_first).max(0.0))
                } else {
                    (indent_left + indent_first).max(0.0)
                };
                margin_left + offset
            } else {
                margin_left + indent_left.max(0.0)
            }
        }
    }
}

/// 單行 → 逐 run 同質文字項（x 由前段 advance 推進）。
///
/// 行內切段口徑：`Line::char_offset` ＋行長（碼點）與該段 `runs` 的碼點區間求交；
/// 交段之外的行內區間（理論不可達——runs 完整覆蓋段落文字流）以中性樣式補發，
/// 與 DOCX 線 `emit_line_run_items` 的 `covered` 兜底同款紀律，絕不吞字。
fn push_line_items(
    items: &mut Vec<DrawItem>,
    base_x: f32,
    y: f32,
    line: &quire_layout::Line,
    para: Option<&crate::layout_glue::OdfLayoutParagraph>,
) {
    let line_len = line.text.chars().count();
    // 幽靈空行（正規化後長度為零）在**解析層**已被剝除（見 layout_glue::normalize_xml_text），
    // 到這裡的行都是真行：零長度行只剩「真空段」與「連續顯式 text:line-break」兩類，
    // 二者都是 LO 會留垂直間距的真行 ⇒ 一律發 1 個空文字項維持行高，不在此層砍行
    // （砍行會破壞 `odf_d1_display_list` 燈 3 的「一頁行數＝發射行群數」幾何契約）。
    if line_len == 0 {
        // 空行（零文字段／行首強制斷點）：發一個空文字項佔位，帶段落代表字級/字重
        // ——與 DOCX 線 `emit_line_run_items` 的零寬行分支同口徑（渲染端據此畫插入點）。
        let (size_pt, bold) = match para {
            Some(p) => (p.font_size_pt(), p.bold()),
            None => (crate::layout_glue::DEFAULT_FONT_SIZE_PT, false),
        };
        items.push(text_item(base_x, y, size_pt, bold, String::new()));
        return;
    }

    let line_start = line.char_offset;
    let line_end = line_start + line_len;
    // 行內位元組邊界表（碼點 → UTF-8 位元組），供逐段切片。
    let byte_of: Vec<usize> = line.text.char_indices().map(|(b, _)| b).collect();
    let slice = |rel_from: usize, rel_to: usize| -> &str {
        let f = byte_of.get(rel_from).copied().unwrap_or(line.text.len());
        let t = byte_of.get(rel_to).copied().unwrap_or(line.text.len());
        &line.text[f..t]
    };

    let mut covered = 0usize; // 行內已發射的碼點數
    let mut x = base_x;
    let runs: &[(crate::layout_glue::CpRange, f32, bool)] =
        para.map(|p| p.runs.as_slice()).unwrap_or(&[]);
    for ((r_from, r_to), size_pt, bold) in runs.iter().copied() {
        let inter_s = r_from.max(line_start);
        let inter_e = r_to.min(line_end);
        if inter_s >= inter_e {
            continue;
        }
        let rel_s = inter_s - line_start;
        let rel_e = inter_e - line_start;
        if rel_s > covered {
            // 兜底段：runs 未覆蓋的行內區間（正常不可達）＝中性樣式。
            let gap = slice(covered, rel_s);
            let w = quire_layout::TextMeasurer::new(crate::layout_glue::DEFAULT_FONT_SIZE_PT)
                .text_width(gap);
            items.push(text_item(
                x,
                y,
                crate::layout_glue::DEFAULT_FONT_SIZE_PT,
                false,
                gap.to_string(),
            ));
            x += w;
        }
        let seg = slice(rel_s, rel_e);
        items.push(text_item(x, y, size_pt, bold, seg.to_string()));
        x += quire_layout::TextMeasurer::new(size_pt).text_width(seg);
        covered = rel_e;
    }
    if covered < line_len {
        let tail = slice(covered, line_len);
        items.push(text_item(
            x,
            y,
            crate::layout_glue::DEFAULT_FONT_SIZE_PT,
            false,
            tail.to_string(),
        ));
    }
}

#[inline]
fn text_item(x: f32, y: f32, size_pt: f32, bold: bool, text: String) -> DrawItem {
    DrawItem::Text {
        x,
        y,
        size_pt,
        flags: if bold { FLAG_BOLD } else { 0 },
        color_argb: DEFAULT_TEXT_COLOR_ARGB,
        // 字型族名未採集（ODF `style:font-name` 屬 D1b）→ None＝serde 跳欄，
        // 渲染端用自己的字型（Canvas2D 端 "Noto Sans TC" 串）。
        family: None,
        text,
    }
}

/// 顯示列表項的 JSON 值（除錯/對照用；契約面仍以 serde 序列化為準）。
pub fn display_list_to_value(lists: &[PageDisplayList]) -> Value {
    serde_json::to_value(lists).unwrap_or_else(|_| Value::Array(Vec::new()))
}
