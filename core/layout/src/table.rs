//! 表格幾何（Phase3-1b）：列高計算、欄寬分配、gridSpan/vMerge、跨頁切片。
//!
//! 分層（與 sectPr 同款模式）：**型別定義在 layout 層**——版面是表格幾何的
//! 唯一消費者；quire-ooxml 只把 w:tbl 讀成 canonical 事實，engine 層負責
//! 映射（`TableInput` 即輸入契約）。layout 不依賴 ooxml。
//!
//! 行網格量化（v1 核心模型）：版面主幹以「行」為分頁容量單位
//! （`LayoutParams::per_page_lines`），表格錨進同一網格——列高、儲存格邊距、
//! 巢狀表高全部量化成**行帶（bands）**。無表格文件的分頁行為因此逐位元組
//! 不變；有表格時，一個跨頁切片佔 ceil(切片高/行高) 個 bands。
//!
//! 對照 ECMA-376 §17.4 的 v1 取捨（皆刻意簡化，非漏實作）：
//! - `w:tblW` dxa→pt 由 engine 換算完畢（`WidthSpec::Pt`）；pct 相對**可用寬**。
//!   自然寬（grid 或首列 tcW 總和）與宣告寬不一致時**等比縮放**（Word 的
//!   autofit 不實作，`fixed_layout` 僅記錄）；總寬超出可用寬→等比壓回。
//! - `cantSplit=false` 的列可在**行帶**邊界拆頁；`cantSplit=true` 整列搬下頁
//!   （切片器一律給至少一整列，單列高於一頁時老實溢出——卡片明示）。
//! - `w:trHeight`：atLeast→與自然高取大；exact→以宣告為準（內容超高溢出繪製）。
//! - vMerge：merged block＝restart＋全部 continue 格段落**串接**（Word 語義），
//!   文字自 block 頂逐帶繪製、天然跨過參加列（每條文字只落在其所屬列的切片）；
//!   block 高於參加列總和時餘額補到 continue 列。水平內邊界不畫框線，
//!   block 底邊由末參加格承擔（取 restart 格的底框——Word 語義）。
//! - `w:tblHeader` 跨頁重複、`w:noWrap`、對角線框線、tblStylePr 條件底紋：
//!   v1 不實作（`header` 欄僅記錄）。
//! - 框線繼承：tcBorders ＞ tblBorders ＞ 表格樣式框線（engine 查表餵入）；
//!   **宣告即贏**（tc 寫 none 就停，不向下續找）；sz（1/8 pt）；
//!   color auto/theme→黑、hex→RGB；val none/nil/hidden＝不畫。

use serde::{Deserialize, Serialize};

use crate::{break_lines, Line, TextMeasurer};

// ---------------------------------------------------------------------------
// 輸入契約（engine 由 canonical model 映射而來；長度單位皆已換算成 pt）
// ---------------------------------------------------------------------------

/// 寬度宣告：pt（dxa÷20 已換算）或可用寬百分比（pct÷50）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WidthSpec {
    Pt(f32),
    Pct(f32),
}

impl WidthSpec {
    /// 解析成絕對 pt；`Pct` 相對 `avail_w`。
    pub fn resolve(&self, avail_w: f32) -> f32 {
        match self {
            WidthSpec::Pt(v) => *v,
            WidthSpec::Pct(v) => avail_w * v / 100.0,
        }
    }
}

/// 單邊框線輸入（原字串保留；解析失敗一律退回保守預設，不 panic）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PenInput {
    /// `@w:val`：single／dashed／dotted／thick／double／none／nil／hidden…。
    pub val: Option<String>,
    /// `@w:sz`：1/8 pt。
    pub sz: Option<String>,
    /// `@w:color`：auto／RRGGBB／theme 名（theme 名 v1 退回黑）。
    pub color: Option<String>,
}

/// 一組框線（表格級或儲存格級）；缺位＝未定義（繼承由本模組合成）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EdgeSetInput {
    pub top: Option<PenInput>,
    pub left: Option<PenInput>,
    pub bottom: Option<PenInput>,
    pub right: Option<PenInput>,
    /// 內部橫線（僅表格級有意義）。
    pub inside_h: Option<PenInput>,
    /// 內部直線（僅表格級有意義）。
    pub inside_v: Option<PenInput>,
}

/// 儲存格邊距（pt；缺＝用表格級，再缺＝Word 預設 top/bottom 0、left/right 5.4pt）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CellMarginInput {
    pub top: Option<f32>,
    pub left: Option<f32>,
    pub bottom: Option<f32>,
    pub right: Option<f32>,
}

/// 儲存格輸入。`text` 為段落**原始字串**序列（run 級粗斜體尚未穿透——與正文
/// 同款限制），斷行在本模組完成；`nested` 為巢狀表（遞迴）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CellInput {
    /// `w:tcW`（dxa→pt 已換算；auto/nil＝None）。
    pub width: Option<WidthSpec>,
    /// `w:gridSpan`（缺＝1）。
    pub grid_span: u32,
    /// `w:vMerge`：`Some(true)`＝restart、`Some(false)`＝continue、`None`＝非合併。
    pub vmerge: Option<bool>,
    /// 段落文字（空 vec＝一個空段）。
    pub text: Vec<String>,
    pub nested: Vec<TableInput>,
    pub borders: Option<EdgeSetInput>,
    /// `w:shd/@w:fill` 原字串（RRGGBB／auto；theme 色名退回不填）。
    pub fill: Option<String>,
    /// `w:vAlign`：top／center／bottom。
    pub valign: Option<String>,
    pub margins: Option<CellMarginInput>,
}

/// 列輸入。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RowInput {
    pub cells: Vec<CellInput>,
    /// `w:cantSplit`（生效值）。
    pub cant_split: bool,
    /// `w:trHeight/@w:val`（twip→pt 已換算）。
    pub height_pt: Option<f32>,
    /// `w:trHeight/@w:hRule == "exact"`。
    pub height_exact: bool,
    /// `w:tblHeader`（v1 僅記錄，跨頁重複未實作）。
    pub header: bool,
}

/// 表格輸入（欄寬已換算成 pt）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableInput {
    /// `w:tblGrid/w:gridCol` 欄寬（pt）。空＝無 tblGrid（由首列 tcW 推導）。
    pub grid_pt: Vec<f32>,
    /// `w:tblW`（dxa→pt／pct；auto/nil＝None）。
    pub width: Option<WidthSpec>,
    /// `w:tblLayout/@w:type == "fixed"`（v1 僅記錄：autofit 不實作）。
    pub fixed_layout: bool,
    /// `w:tblInd`（pt）。
    pub indent_pt: Option<f32>,
    /// `w:jc/@w:val`（left／center／right）。
    pub justify: Option<String>,
    /// `w:tblBorders`。
    pub borders: Option<EdgeSetInput>,
    /// 表格樣式（`w:tblStyle` 查表結果）的框線——最低層繼承。
    pub style_borders: Option<EdgeSetInput>,
    /// `w:tblCellMar`。
    pub cell_margins: Option<CellMarginInput>,
    pub rows: Vec<RowInput>,
}

// ---------------------------------------------------------------------------
// 解析後幾何（packer 切片與 display list 發射的共同輸入）
// ---------------------------------------------------------------------------

/// 解析完成的筆畫（display list 直出）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pen {
    pub width_pt: f32,
    pub color_argb: u32,
    /// SVG dasharray 語義（None＝實線）。
    pub dash: Option<String>,
}

/// 框線四方位索引常數（`CellFrag::edges`／`CellGeom::edges` 下標）。
pub const EDGE_TOP: usize = 0;
pub const EDGE_LEFT: usize = 1;
pub const EDGE_BOTTOM: usize = 2;
pub const EDGE_RIGHT: usize = 3;

/// 切片中的一格文字（x＝**表格本地**＋左邊距；y＝列切片內相對偏移）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellText {
    pub x_pt: f32,
    pub y_pt: f32,
    pub text: String,
}

/// 切片中的一格：x/w 為**表格本地**座標（繪製時加 frag 的 x_pt）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellFrag {
    pub x_pt: f32,
    pub w_pt: f32,
    /// 已解析框線（None＝該邊不畫）；[top, left, bottom, right]。
    pub edges: [Option<Pen>; 4],
    pub fill_argb: Option<u32>,
    /// 切片內文字（y 相對本列切片頂）。
    pub texts: Vec<CellText>,
    /// 巢狀表切片（y 偏移相對本列切片頂；x 已含本格左緣＋表格偏移）。
    pub nested: Vec<(f32, TableFrag)>,
}

/// 切片中的一列（列被跨頁拆開時一列可產出多個列切片）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowFrag {
    pub height_pt: f32,
    /// 本切片起於列的第幾個行帶（0＝列首）。
    pub band_start: u32,
    /// 本列在表格中的列序（同列的多個切片共用）。
    pub row_index: usize,
    /// 本切片是否觸及列底（底邊框線只在觸底時畫）。
    pub ends_row: bool,
    /// 本切片是否起於列頂（頂邊框線只在列首切片畫）。
    pub starts_row: bool,
    pub cells: Vec<CellFrag>,
}

/// 表格在某頁上的一段（跨頁表＝每頁一個 TableFrag）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableFrag {
    /// 表格左緣（**頁座標系**：已含版心左緣＋tblInd/jc 對齊偏移）。
    pub x_pt: f32,
    /// 切片頂（**頁座標系**；`fragment()` 產出時為 0，由排版器放置時蓋章）。
    pub y_pt: f32,
    pub width_pt: f32,
    /// 切片起於表格首列首帶。
    pub starts_table: bool,
    /// 切片之後仍有列排在下一頁。
    pub continues: bool,
    pub rows: Vec<RowFrag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Valign {
    Top,
    Center,
    Bottom,
}

/// 一格的排版結果（欄位座標＋斷行＋幾何屬性）。
#[derive(Debug, Clone, PartialEq)]
struct CellGeom {
    /// 欄起點（grid 索引）。
    col: usize,
    col_span: usize,
    /// 表格本地 x／寬（pt）。
    x: f32,
    w: f32,
    /// 文字左緣偏移（表格本地：x＋左邊距）。
    text_x_off: f32,
    /// 上邊距佔用的帶數（量化）。
    pad_top_bands: u32,
    /// 下邊距佔用的帶數（量化）。
    pad_bottom_bands: u32,
    /// 段落文字總帶高＝Σ max(1, 段帶數)。
    text_bands: u32,
    /// 是否宣告過段落（text 非空）：vMerge block 串接時，完全空白的
    /// continue 格**不**貢獻帶高（Word：合併格內的空段落不顯現）。
    has_text: bool,
    /// 斷行結果（全部段落串接；vMerge block 時含參加格串接後的全文字）。
    lines: Vec<Line>,
    /// 內容總帶高＝pad_top ＋ max(text, nested) ＋ pad_bottom（vMerge 後重算）。
    content_bands: u32,
    edges: [Option<Pen>; 4],
    fill_argb: Option<u32>,
    valign: Valign,
    nested: Vec<TableLayout>,
    /// vMerge 歸屬：Some((block 序號, 是否 restart 格))；None＝非合併。
    merge: Option<(usize, bool)>,
    /// vMerge block 的 restart 列序（block 帶座標原點）。
    block_head_row: usize,
    /// vMerge block 的**末**參加格（底邊由它承擔）。
    block_tail: bool,
}

/// 一個列的排版結果。
#[derive(Debug, Clone, PartialEq)]
struct RowGeom {
    bands: u32,
    cant_split: bool,
    cells: Vec<CellGeom>,
}

/// 表格完整排版（列帶分配完成；跨頁切片由 [`TableLayout::fragment`] 產出）。
#[derive(Debug, Clone, PartialEq)]
pub struct TableLayout {
    /// 表格左緣（相對可用區左緣：tblInd／jc 已計入）。
    pub x_off: f32,
    pub width: f32,
    /// 每列的行帶數（packer 直接拿去和分頁容量比較）。
    pub row_units: Vec<f32>,
    rows: Vec<RowGeom>,
    /// rows 前綴和：row_base[r] = 前 r 列的帶總和。
    row_base: Vec<u32>,
    pub total_bands: u32,
    line_h: f32,
}

impl TableLayout {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() || self.total_bands == 0
    }
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// 產出「自 (row, band) 起、至多 `max_bands` 帶」的切片。
    /// 回傳 (切片, 下一游標 (row, band), 本切片實際帶數)。
    ///
    /// `whole_row_only=true`：列不拆（巢狀表用——巢狀列不拆頁，v1 簡化）。
    /// `page_left_pt`：可用區左緣（頁座標系），換算 `TableFrag::x_pt`。
    pub fn fragment(
        &self,
        row: usize,
        band: u32,
        max_bands: u32,
        whole_row_only: bool,
        page_left_pt: f32,
    ) -> (TableFrag, (usize, u32), u32) {
        let mut frag_rows: Vec<RowFrag> = Vec::new();
        let (mut r, mut b) = (row, band);
        let mut budget = max_bands.max(1);
        let mut used = 0u32;
        while r < self.rows.len() {
            let rg = &self.rows[r];
            if b >= rg.bands {
                r += 1;
                b = 0;
                continue;
            }
            // 切片只按預算取帶（分頁策略屬 packer：佔位行設計下所有列一律
            // 可行帶拆列）。`cantSplit` 的「整列搬頁」需要 packer 級 atomic
            // 組支援，列後續；v1 容許列在帶邊界拆開（模組頭明示的取捨）。
            let take = if whole_row_only {
                rg.bands - b
            } else if budget > 0 {
                (rg.bands - b).min(budget)
            } else {
                break;
            };
            let take = take.max(1);
            let h = take as f32 * self.line_h;
            let starts_row = b == 0;
            let ends_row = b + take >= rg.bands;
            let cells: Vec<CellFrag> = rg
                .cells
                .iter()
                .map(|cg| frag_of_cell(self, cg, r, b, take, h, starts_row, ends_row))
                .collect();
            frag_rows.push(RowFrag {
                height_pt: h,
                band_start: b,
                row_index: r,
                ends_row,
                starts_row,
                cells,
            });
            used += take;
            budget = budget.saturating_sub(take);
            b += take;
            if budget == 0 {
                break;
            }
        }
        // 游標正規化：停在「列末帶」與停在下列帶 0 同義，統一交後者。
        let (nr, nb) = if r < self.rows.len() && b >= self.rows[r].bands {
            (r + 1, 0)
        } else {
            (r, b)
        };
        let frag = TableFrag {
            x_pt: page_left_pt + self.x_off,
            y_pt: 0.0, // 排版器放置時蓋章（頁內絕對 y）
            width_pt: self.width,
            starts_table: row == 0 && band == 0,
            continues: nr < self.rows.len(),
            rows: frag_rows,
        };
        (frag, (nr, nb), used)
    }
}

/// 一格在切片 (r, b..b+take) 內的顯示形態。
///
/// 文字定位統一模型：格的「內容原點帶」block0＝vMerge 時為 restart 列帶 0，
/// 否則為本列帶 0；第 i 條文字佔帶 block0＋pad_top＋i，**只在其所屬列的切片
/// 視窗內繪製**——merged block 文字因此天然跨列、且不重複。
fn frag_of_cell(
    tl: &TableLayout,
    cg: &CellGeom,
    r: usize,
    b: u32,
    take: u32,
    h: f32,
    starts_row: bool,
    ends_row: bool,
) -> CellFrag {
    let win0 = tl.row_base[r] + b; // 切片視窗（全域帶）
    let win1 = win0 + take;
    let mut edges = cg.edges.clone();
    if !starts_row {
        edges[EDGE_TOP] = None; // 列被拆頁處不補造橫線
    }
    if !ends_row {
        edges[EDGE_BOTTOM] = None;
    }
    // vMerge：block 內部橫線不出（head 底邊除單格 block 外不畫；continue 頂邊
    // 一律不畫，底邊只有末參加格保留——assign_merge_blocks 已把 head 底框
    // 搬給末格、把 head/中間格的對應邊清空）。
    let block0 = tl.row_base[cg.block_head_row];
    let mut texts = Vec::new();
    let shift = valign_shift(cg.valign, h, cg.content_bands, tl.line_h);
    let mut cur = block0 + cg.pad_top_bands;
    for ln in &cg.lines {
        if cur >= win1 {
            break;
        }
        if cur >= win0 {
            texts.push(CellText {
                x_pt: cg.x + cg.text_x_off,
                y_pt: (shift + (cur - win0) as f32 * tl.line_h).max(0.0),
                text: ln.text.clone(),
            });
        }
        cur += 1;
    }
    let nested = if matches!(cg.merge, Some((_, false))) {
        Vec::new()
    } else {
        cg.nested
            .iter()
            .map(|n| {
                // 巢狀表的頁左緣基準＝**表格本地**格左緣（繪製端再疊 frag.x_pt，
                // 外層 x_off 不在這裡加，避免雙重計入）。
                let (f, _, _) = n.fragment(0, 0, u32::MAX, true, cg.x);
                (
                    valign_shift(cg.valign, h, cg.content_bands, tl.line_h)
                        + cg.pad_top_bands as f32 * tl.line_h,
                    f,
                )
            })
            .collect()
    };
    CellFrag {
        x_pt: cg.x,
        w_pt: cg.w,
        edges,
        fill_argb: cg.fill_argb,
        texts,
        nested,
    }
}

fn valign_shift(v: Valign, frag_h: f32, content_bands: u32, line_h: f32) -> f32 {
    let content_h = content_bands as f32 * line_h;
    match v {
        Valign::Top => 0.0,
        Valign::Center => ((frag_h - content_h) * 0.5).max(0.0),
        Valign::Bottom => (frag_h - content_h).max(0.0),
    }
}

// ---------------------------------------------------------------------------
// 主函數：TableInput → TableLayout
// ---------------------------------------------------------------------------

const DEFAULT_SIDE_MARGIN_PT: f32 = 5.4; // 108 twip（Word TableNormal 預設）
/// 巢狀表深度上限（防惡意深巢狀爆堆疊；超過即略過子表——老實不虛構）。
const MAX_NEST_DEPTH: usize = 8;

/// 排版一張表格。`avail_w`＝可用寬（pt；頂層＝版心寬，巢狀＝儲存格內寬）；
/// `line_h`＝行網格高度（pt）；`m`＝文字量測器；`depth`＝巢狀深度。
pub fn layout_table(
    input: &TableInput,
    avail_w: f32,
    line_h: f32,
    m: &TextMeasurer,
    depth: usize,
) -> TableLayout {
    let line_h = line_h.max(1.0);
    let avail_w = avail_w.max(1.0);
    let cols = resolve_columns(input, avail_w);
    let width = cols.iter().sum::<f32>().max(1.0);
    let x_off = table_x_off(input, width, avail_w);

    // ---- pass 1：逐格斷行＋邊框/填色/邊距解析（欄位座標先定） ----
    let n_rows = input.rows.len();
    let mut rows: Vec<RowGeom> = Vec::with_capacity(n_rows);
    for (ri, row) in input.rows.iter().enumerate() {
        let n_cells = row.cells.len().max(1);
        let mut col_cursor = 0usize;
        let mut cells: Vec<CellGeom> = Vec::with_capacity(row.cells.len());
        for (ci, cell) in row.cells.iter().enumerate() {
            let span = (cell.grid_span.max(1) as usize)
                .min(cols.len().saturating_sub(col_cursor))
                .max(1);
            let x: f32 = cols[..col_cursor].iter().sum();
            let mut w: f32 = cols[col_cursor..(col_cursor + span).min(cols.len())]
                .iter()
                .sum();
            if w <= 0.0 {
                // 無 grid 且首列推導失敗時兜底：tcW → 平均分配。
                w = cell
                    .width
                    .map(|s| s.resolve(avail_w).min(avail_w).max(1.0))
                    .unwrap_or_else(|| (avail_w / n_cells as f32).max(1.0));
            }
            let col = col_cursor;
            col_cursor = (col_cursor + span).min(cols.len());

            let margins = resolve_margins(cell.margins.as_ref(), input.cell_margins.as_ref());
            let inner_w = (w - margins.left - margins.right).max(8.0);
            let pad_top_bands = (margins.top / line_h).ceil() as u32;
            let pad_bottom_bands = (margins.bottom / line_h).ceil() as u32;

            let (lines, text_bands) = break_cell_paras(&cell.text, inner_w, m);

            // 巢狀表：以儲存格內寬排版，帶高併入本格。
            let mut nested = Vec::new();
            let mut nested_bands = 0u32;
            if depth < MAX_NEST_DEPTH {
                for n in &cell.nested {
                    let nl = layout_table(n, inner_w, line_h, m, depth + 1);
                    nested_bands = nested_bands.max(nl.total_bands);
                    nested.push(nl);
                }
            }
            let content_bands =
                pad_top_bands + text_bands.max(nested_bands).max(1) + pad_bottom_bands;

            let edges = resolve_cell_edges(cell, input, ri, ci, n_rows, n_cells);
            cells.push(CellGeom {
                col,
                col_span: span,
                x,
                w,
                text_x_off: margins.left,
                pad_top_bands,
                pad_bottom_bands,
                text_bands,
                has_text: cell.text.iter().any(|t| !t.trim().is_empty()),
                lines,
                content_bands,
                edges,
                fill_argb: parse_fill(cell.fill.as_deref()),
                valign: match cell.valign.as_deref() {
                    Some("center") => Valign::Center,
                    Some("bottom") => Valign::Bottom,
                    _ => Valign::Top,
                },
                nested,
                merge: cell.vmerge.map(|restart| (usize::MAX, restart)), // 序號 pass 2 填
                block_head_row: ri,
                block_tail: false,
            });
        }
        rows.push(RowGeom {
            bands: 1,
            cant_split: row.cant_split,
            cells,
        });
    }

    // ---- pass 2：vMerge block（串接文字＋邊框搬移） ----
    assign_merge_blocks(&mut rows);

    // ---- pass 3：列高（自然高＋trHeight＋vMerge 餘額補列） ----
    let mut tl = TableLayout {
        x_off,
        width,
        row_units: Vec::new(),
        rows,
        row_base: Vec::new(),
        total_bands: 0,
        line_h,
    };
    compute_row_heights(&mut tl, input);
    tl.rebuild_base();
    tl
}

/// 段落逐條斷行；空段也占一帶（Word：空段落仍占一行高）。
fn break_cell_paras(paras: &[String], inner_w: f32, m: &TextMeasurer) -> (Vec<Line>, u32) {
    let mut all = Vec::new();
    let mut bands = 0u32;
    if paras.is_empty() {
        all.extend(break_lines("", inner_w, m));
        return (all, 1);
    }
    for t in paras {
        let ls = break_lines(t, inner_w, m);
        bands += ls.len().max(1) as u32;
        all.extend(ls);
    }
    (all, bands)
}

/// 找 vMerge block（同欄 col==c 且 span==1 才參與——跨欄格 vMerge v1 不實作）：
/// - head（restart）：lines＝head＋各 continue 段落串接；content_bands 重算
///   （含參加格的上下邊距）；底框搬給末參加格（block 底邊），自身底邊清空。
/// - continue：清空自身 lines（文字已由 head 持有）；頂邊清空；
///   末參加格 block_tail=true 並承接 head 底框。
fn assign_merge_blocks(rows: &mut [RowGeom]) {
    let n_rows = rows.len();
    let max_col = rows
        .iter()
        .flat_map(|r| r.cells.iter().map(|g| g.col))
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);
    let mut next_block = 0usize;
    for c in 0..max_col {
        let mut r = 0usize;
        while r < n_rows {
            let is_restart = rows[r]
                .cells
                .iter()
                .find(|g| g.col == c && g.col_span == 1)
                .is_some_and(|g| matches!(g.merge, Some((_, true))));
            if !is_restart {
                r += 1;
                continue;
            }
            let head = r;
            let mut last = r;
            while last + 1 < n_rows {
                let cont = rows[last + 1]
                    .cells
                    .iter()
                    .find(|g| g.col == c && g.col_span == 1);
                if matches!(cont.map(|g| g.merge), Some(Some((_, false)))) {
                    last += 1;
                } else {
                    break;
                }
            }
            let block = next_block;
            next_block += 1;

            // 收集參加格資料（一次讀完，避免多輪 borrow）。
            let mut members: Vec<(usize, u32, u32, u32, Vec<Line>)> = Vec::new();
            for rr in head..=last {
                let g = rows[rr]
                    .cells
                    .iter()
                    .find(|g| g.col == c && g.col_span == 1)
                    .expect("參加格存在");
                members.push((
                    rr,
                    g.pad_top_bands,
                    g.pad_bottom_bands,
                    g.text_bands,
                    g.lines.clone(),
                ));
            }
            let mut joined: Vec<Line> = Vec::new();
            let mut text_bands = 0u32;
            let mut extra_pad = 0u32;
            for (k, (rr, pt, pb, tb, ls)) in members.iter().enumerate() {
                let blank = !rows[*rr].cells.iter().any(|g| g.col == c && g.has_text);
                if !blank {
                    joined.extend(ls.iter().cloned());
                    text_bands += tb;
                }
                if k > 0 && !blank {
                    extra_pad += pt + pb;
                }
            }
            let (head_pad_top, head_pad_bot, nested_keep) = {
                let hg = rows[head]
                    .cells
                    .iter()
                    .find(|g| g.col == c)
                    .expect("head 存在");
                (
                    hg.pad_top_bands,
                    hg.pad_bottom_bands,
                    hg.nested.iter().map(|n| n.total_bands).max().unwrap_or(0),
                )
            };
            {
                let hg = rows[head]
                    .cells
                    .iter_mut()
                    .find(|g| g.col == c)
                    .expect("head 存在");
                hg.merge = Some((block, true));
                hg.block_head_row = head;
                hg.block_tail = last == head;
                hg.lines = joined;
                hg.text_bands = text_bands;
                hg.content_bands =
                    head_pad_top + text_bands.max(nested_keep).max(1) + head_pad_bot + extra_pad;
                // block 底邊交給末格；head 自身底邊清空（內部橫線不畫）。
                let bottom = hg.edges[EDGE_BOTTOM].clone();
                if last > head {
                    hg.edges[EDGE_BOTTOM] = None;
                    if let Some(tg) = rows[last]
                        .cells
                        .iter_mut()
                        .find(|g| g.col == c && g.col_span == 1)
                    {
                        tg.edges[EDGE_BOTTOM] = bottom;
                    }
                }
            }
            for rr in head + 1..=last {
                if let Some(g) = rows[rr]
                    .cells
                    .iter_mut()
                    .find(|g| g.col == c && g.col_span == 1)
                {
                    g.merge = Some((block, false));
                    g.block_head_row = head;
                    g.block_tail = rr == last;
                    g.lines.clear();
                    g.text_bands = 0;
                    g.edges[EDGE_TOP] = None; // block 內部橫線
                    if rr != last {
                        g.edges[EDGE_BOTTOM] = None;
                    }
                }
            }
            r = last + 1;
        }
    }
}

/// 列高分配：自然帶高＝非 continue 格的最大 content_bands；
/// trHeight atLeast 取大、exact 以宣告為準；vMerge block 不足額平均分給 continue 列。
fn compute_row_heights(tl: &mut TableLayout, input: &TableInput) {
    for (ri, rg) in tl.rows.iter_mut().enumerate() {
        let natural = rg
            .cells
            .iter()
            .filter(|g| !matches!(g.merge, Some((_, false))))
            .map(|g| g.content_bands)
            .max()
            .unwrap_or(1)
            .max(1);
        let mut bands = natural;
        if let Some(row) = input.rows.get(ri) {
            if let Some(h) = row.height_pt {
                let need = (h / tl.line_h).ceil() as u32;
                bands = if row.height_exact {
                    need.max(1)
                } else {
                    bands.max(need)
                };
            }
        }
        rg.bands = bands;
    }
    // vMerge 餘額：head 內容帶高 > 參加列帶高總和→餘額補到 continue 列。
    let n_rows = tl.rows.len();
    let mut heads: Vec<(usize, usize, usize)> = Vec::new(); // (col, head, last)
    for r in 0..n_rows {
        for g in &tl.rows[r].cells {
            if let Some((_, true)) = g.merge {
                let col = g.col;
                let mut last = r;
                while last + 1 < n_rows
                    && tl.rows[last + 1]
                        .cells
                        .iter()
                        .any(|g2| g2.col == col && matches!(g2.merge, Some((_, false))))
                {
                    last += 1;
                }
                heads.push((col, r, last));
            }
        }
    }
    for (col, head, last) in heads {
        if last <= head {
            continue;
        }
        let need = tl.rows[head]
            .cells
            .iter()
            .find(|g| g.col == col)
            .map(|g| g.content_bands)
            .unwrap_or(0);
        let sum: u32 = tl.rows[head..=last].iter().map(|g| g.bands).sum();
        if need > sum {
            let extra = need - sum;
            let k = (last - head) as u32;
            let share = extra / k;
            let rem = extra % k;
            for rr in head + 1..=last {
                tl.rows[rr].bands += share + if rr == last { rem } else { 0 };
            }
        }
    }
}

impl TableLayout {
    fn rebuild_base(&mut self) {
        self.row_base = Vec::with_capacity(self.rows.len());
        let mut acc = 0u32;
        for rg in &self.rows {
            self.row_base.push(acc);
            acc += rg.bands;
        }
        self.total_bands = acc;
        self.row_units = self.rows.iter().map(|r| r.bands as f32).collect();
    }
}

// ---------------------------------------------------------------------------
// 欄寬、x 偏移、邊距、框線
// ---------------------------------------------------------------------------

/// 欄寬分配：tblGrid 優先（總寬≠宣告寬→等比縮放）；無 grid→首列 tcW 推導；
/// 都没有→單欄佔滿。最後總寬 > 可用寬→等比壓回（不越版心）。
fn resolve_columns(input: &TableInput, avail_w: f32) -> Vec<f32> {
    let mut cols: Vec<f32> = Vec::new();
    if !input.grid_pt.is_empty() {
        cols = input.grid_pt.iter().map(|v| v.max(0.0)).collect();
    } else if let Some(row0) = input.rows.first() {
        let n_cols: usize = row0.cells.iter().map(|c| c.grid_span.max(1) as usize).sum();
        if n_cols > 0 {
            cols = vec![0.0; n_cols];
            let mut idx = 0usize;
            for cell in &row0.cells {
                let span = cell.grid_span.max(1) as usize;
                if let Some(w) = cell.width {
                    let wpt = w.resolve(avail_w).max(1.0);
                    for k in 0..span {
                        if idx + k < n_cols {
                            cols[idx + k] = wpt / span as f32;
                        }
                    }
                }
                idx += span;
            }
            if cols.iter().sum::<f32>() <= 0.0 {
                cols = vec![avail_w / n_cols as f32; n_cols];
            }
        }
    }
    if cols.is_empty() {
        cols = vec![avail_w];
    }
    if let Some(w) = input.width {
        let target = w.resolve(avail_w);
        let natural: f32 = cols.iter().sum();
        if target > 1.0 && natural > 1.0 && (target - natural).abs() > 0.5 {
            let k = target / natural;
            for c in cols.iter_mut() {
                *c *= k;
            }
        }
    }
    let total: f32 = cols.iter().sum();
    if total > avail_w && total > 0.0 {
        let k = avail_w / total;
        for c in cols.iter_mut() {
            *c *= k;
        }
    }
    cols
}

fn table_x_off(input: &TableInput, width: f32, avail_w: f32) -> f32 {
    let ind = input.indent_pt.unwrap_or(0.0).max(0.0);
    match input.justify.as_deref() {
        Some("center") => (((avail_w - width) * 0.5).max(0.0)).max(ind),
        Some("right") => (avail_w - width).max(ind).max(0.0),
        _ => ind.min(avail_w.max(0.0)),
    }
}

struct SolvedMargins {
    top: f32,
    left: f32,
    bottom: f32,
    right: f32,
}

fn resolve_margins(
    cell: Option<&CellMarginInput>,
    table: Option<&CellMarginInput>,
) -> SolvedMargins {
    let side = |c: Option<f32>, t: Option<f32>, dflt: f32| c.or(t).unwrap_or(dflt).max(0.0);
    SolvedMargins {
        top: side(cell.and_then(|c| c.top), table.and_then(|t| t.top), 0.0),
        left: side(
            cell.and_then(|c| c.left),
            table.and_then(|t| t.left),
            DEFAULT_SIDE_MARGIN_PT,
        ),
        bottom: side(
            cell.and_then(|c| c.bottom),
            table.and_then(|t| t.bottom),
            0.0,
        ),
        right: side(
            cell.and_then(|c| c.right),
            table.and_then(|t| t.right),
            DEFAULT_SIDE_MARGIN_PT,
        ),
    }
}

/// 儲存格四邊合成（宣告即贏：tc 有 `@w:val` 就到此為止，none 亦為有效宣告）。
/// 列/欄首末走 top/bottom/left/right 方位，內部走 inside_h/inside_v。
fn resolve_cell_edges(
    cell: &CellInput,
    input: &TableInput,
    ri: usize,
    ci: usize,
    n_rows: usize,
    n_cells: usize,
) -> [Option<Pen>; 4] {
    let tc = cell.borders.clone().unwrap_or_default();
    let first_row = ri == 0;
    let last_row = ri + 1 == n_rows;
    let first_col = ci == 0;
    let last_col = ci + 1 == n_cells;
    [
        edge(tc.top.as_ref(), input, first_row, true, true),
        edge(tc.left.as_ref(), input, first_col, false, true),
        edge(tc.bottom.as_ref(), input, last_row, true, false),
        edge(tc.right.as_ref(), input, last_col, false, false),
    ]
}

/// `own`＝tcBorders 該邊；`on_outer`＝該邊落在表格外框（true 用 tbl/style 的
/// 外框方位，false 用 inside 方位）。
fn edge(
    own: Option<&PenInput>,
    input: &TableInput,
    on_outer: bool,
    horizontal: bool,
    leading: bool,
) -> Option<Pen> {
    if let Some(p) = own {
        return solve_pen(p); // 宣告即贏（val=none → None）
    }
    let tbl = tbl_side(input.borders.as_ref(), on_outer, horizontal, leading);
    let st = tbl_side(input.style_borders.as_ref(), on_outer, horizontal, leading);
    tbl.or(st).and_then(solve_pen)
}

fn tbl_side(
    set: Option<&EdgeSetInput>,
    on_outer: bool,
    horizontal: bool,
    leading: bool,
) -> Option<&PenInput> {
    let s = set?;
    let side = if !on_outer {
        if horizontal {
            s.inside_h.as_ref()
        } else {
            s.inside_v.as_ref()
        }
    } else if horizontal {
        if leading {
            s.top.as_ref()
        } else {
            s.bottom.as_ref()
        }
    } else if leading {
        s.left.as_ref()
    } else {
        s.right.as_ref()
    };
    side.filter(|p| p.val.is_some())
}

/// 原字串 → Pen；val 缺/none/nil/hidden → None（不畫）。
pub fn solve_pen(p: &PenInput) -> Option<Pen> {
    match p.val.as_deref() {
        None | Some("none") | Some("nil") | Some("hidden") => return None,
        Some(_) => {}
    }
    let sz =
        p.sz.as_deref()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .unwrap_or(4.0);
    let width_pt = (sz / 8.0).max(0.25);
    let dash = match p.val.as_deref()? {
        "dashed" | "dashSmallGap" | "dashedEmphasis" | "longDash" | "heavyDash" => {
            Some("3 2".to_string())
        }
        "dotted" | "dotDash" | "dotDotDash" | "dottedEmphasis" | "roundDot" => {
            Some("1 2".to_string())
        }
        _ => None,
    };
    Some(Pen {
        width_pt,
        color_argb: parse_color(p.color.as_deref()),
        dash,
    })
}

fn parse_color(c: Option<&str>) -> u32 {
    match c {
        None | Some("auto") => 0xFF00_0000,
        Some(s) => parse_hex6(s)
            .map(|rgb| 0xFF00_0000 | rgb)
            .unwrap_or(0xFF00_0000),
    }
}

fn parse_fill(f: Option<&str>) -> Option<u32> {
    let s = f?;
    if s.eq_ignore_ascii_case("auto") {
        return None;
    }
    parse_hex6(s).map(|rgb| 0xFF00_0000 | rgb)
}

fn parse_hex6(s: &str) -> Option<u32> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

// ---------------------------------------------------------------------------
// 測試
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn measurer() -> TextMeasurer {
        TextMeasurer::new(12.0)
    }

    fn cell(text: &str) -> CellInput {
        CellInput {
            text: vec![text.to_string()],
            ..Default::default()
        }
    }

    fn simple_input() -> TableInput {
        TableInput {
            grid_pt: vec![100.0, 100.0],
            rows: vec![
                RowInput {
                    cells: vec![cell("甲"), cell("乙")],
                    ..Default::default()
                },
                RowInput {
                    cells: vec![cell("丙"), cell("丁")],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn columns_from_grid_scale_to_declared_width_and_cap_avail() {
        let mut inp = simple_input();
        inp.width = Some(WidthSpec::Pt(300.0));
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert!((t.width - 300.0).abs() < 0.01, "宣告 300pt 等比放大");
        let mut inp2 = simple_input();
        inp2.width = Some(WidthSpec::Pt(1000.0));
        let t2 = layout_table(&inp2, 400.0, 16.8, &measurer(), 0);
        assert!((t2.width - 400.0).abs() < 0.01, "超出可用寬被壓回");
    }

    #[test]
    fn width_pct_resolves_against_avail() {
        let mut inp = simple_input();
        inp.width = Some(WidthSpec::Pct(50.0));
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert!((t.width - 200.0).abs() < 0.01, "50% 可用寬＝200pt");
    }

    #[test]
    fn no_grid_falls_back_to_first_row_tcw() {
        let mut inp = simple_input();
        inp.grid_pt.clear();
        inp.rows[0].cells[0].width = Some(WidthSpec::Pt(200.0));
        inp.rows[0].cells[1].width = Some(WidthSpec::Pt(100.0));
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert!((t.width - 300.0).abs() < 0.01, "首列 tcW 推導 200+100");
    }

    #[test]
    fn no_grid_no_tcw_equal_divides() {
        let mut inp = simple_input();
        inp.grid_pt.clear();
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert!((t.width - 400.0).abs() < 0.01, "無任何宣告→佔滿可用寬");
    }

    #[test]
    fn grid_span_cell_spans_columns_width() {
        let mut inp = simple_input();
        inp.rows[0].cells.truncate(1);
        inp.rows[0].cells[0].grid_span = 2;
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        let (f, _, _) = t.fragment(0, 0, 99, true, 72.0);
        let c = &f.rows[0].cells[0];
        assert!((c.w_pt - 200.0).abs() < 0.01, "跨兩欄＝200pt");
        assert!((c.x_pt - 0.0).abs() < 0.01);
    }

    #[test]
    fn row_bands_quantize_content_and_trheight_atleast() {
        let mut inp = simple_input();
        inp.rows[0].height_pt = Some(40.0); // 40/16.8 → ceil 3 帶
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert_eq!(t.row_units[0], 3.0, "atLeast 取大");
        inp.rows[0].height_exact = true;
        inp.rows[0].height_pt = Some(8.0); // exact：以宣告為準（1 帶）
        let t2 = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert_eq!(t2.row_units[0], 1.0, "exact 以宣告為準");
    }

    #[test]
    fn cant_split_row_splits_like_others_until_packer_supports_atomic_rows() {
        // v1 取捨（模組頭）：cantSplit 的「整列搬頁」屬 packer 職責，占位行
        // 設計下尚未實作——切片層一律按帶拆列；flag 保留供 packer 後續使用。
        let mut inp = simple_input();
        inp.rows[0].cant_split = true;
        inp.rows[0].height_pt = Some(40.0); // 3 帶
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert_eq!(t.row_units[0], 3.0);
        let (f, next, used) = t.fragment(0, 0, 1, false, 72.0);
        assert_eq!(used, 1, "v1：預算 1 帶就取 1 帶");
        assert_eq!(next, (0, 1));
        // whole_row_only（巢狀表路徑）仍保證整列輸出。
        let (f2, next2, used2) = t.fragment(0, 0, 1, true, 72.0);
        assert_eq!(used2, 3);
        assert_eq!(next2, (1, 0));
        assert!(f2.rows[0].ends_row && f.rows[0].starts_row);
    }

    #[test]
    fn splittable_row_splits_at_band_window() {
        let mut inp = simple_input();
        inp.rows[0].height_pt = Some(40.0); // 3 帶
        inp.borders = Some(EdgeSetInput {
            top: Some(PenInput {
                val: Some("single".into()),
                ..Default::default()
            }),
            ..Default::default()
        });
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert_eq!(t.row_units[0], 3.0);
        let (f, next, used) = t.fragment(0, 0, 2, false, 72.0);
        assert_eq!(used, 2, "受容量限制只取 2 帶");
        assert_eq!(next, (0, 2), "游標停在列 0 帶 2");
        assert!(!f.rows[0].ends_row);
        assert!(
            f.rows[0].cells[0].edges[EDGE_TOP].is_some(),
            "列頂切片保留頂邊"
        );
        let (f2, _, _) = t.fragment(0, 2, 5, false, 72.0);
        assert!(
            f2.rows[0].cells[0].edges[EDGE_TOP].is_none(),
            "列中切片不畫頂邊"
        );
    }

    #[test]
    fn borders_resolution_order_and_none() {
        let mut inp = simple_input();
        inp.borders = Some(EdgeSetInput {
            top: Some(PenInput {
                val: Some("single".into()),
                sz: Some("8".into()),
                color: Some("FF0000".into()),
            }),
            bottom: Some(PenInput {
                val: Some("single".into()),
                sz: Some("8".into()),
                color: None,
            }),
            left: Some(PenInput {
                val: Some("single".into()),
                sz: Some("8".into()),
                color: None,
            }),
            right: Some(PenInput {
                val: Some("single".into()),
                sz: Some("8".into()),
                color: None,
            }),
            inside_h: Some(PenInput {
                val: Some("dotted".into()),
                sz: Some("4".into()),
                color: None,
            }),
            inside_v: Some(PenInput {
                val: Some("dashed".into()),
                sz: Some("4".into()),
                color: None,
            }),
        });
        // 列0格0：tc 宣告 top=none——有效宣告，覆寫表格級單線（宣告即贏）。
        inp.rows[0].cells[0].borders = Some(EdgeSetInput {
            top: Some(PenInput {
                val: Some("none".into()),
                ..Default::default()
            }),
            ..Default::default()
        });
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        let (f, _, _) = t.fragment(0, 0, 99, true, 0.0);
        assert!(
            f.rows[0].cells[0].edges[EDGE_TOP].is_none(),
            "tc none 覆寫成功"
        );
        let top1 = f.rows[0].cells[1].edges[EDGE_TOP]
            .as_ref()
            .expect("第二格吃表格級單線");
        assert!((top1.width_pt - 1.0).abs() < 0.001, "sz=8 → 1pt");
        assert_eq!(top1.color_argb, 0xFFFF0000);
        let lv = f.rows[0].cells[1].edges[EDGE_LEFT]
            .as_ref()
            .expect("內部直線存在");
        assert_eq!(lv.dash.as_deref(), Some("3 2"), "dashed→dasharray");
        // 列間內部橫線（列1 頂邊＝內部→dotted）
        let ih = f.rows[1].cells[0].edges[EDGE_TOP]
            .as_ref()
            .expect("列1 頂邊是內部橫線");
        assert_eq!(ih.dash.as_deref(), Some("1 2"));
    }

    #[test]
    fn style_borders_are_lowest_precedence() {
        let mut inp = simple_input();
        inp.style_borders = Some(EdgeSetInput {
            inside_v: Some(PenInput {
                val: Some("single".into()),
                sz: Some("4".into()),
                color: None,
            }),
            ..Default::default()
        });
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        let (f, _, _) = t.fragment(0, 0, 99, true, 0.0);
        assert!(
            f.rows[0].cells[1].edges[EDGE_LEFT].is_some(),
            "樣式框線兜底"
        );
        assert!(
            f.rows[0].cells[0].edges[EDGE_TOP].is_none(),
            "無宣告→外框不畫"
        );
    }

    #[test]
    fn fill_parsed_to_argb() {
        let mut inp = simple_input();
        inp.rows[0].cells[0].fill = Some("D9E2F3".into());
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        let (f, _, _) = t.fragment(0, 0, 99, true, 0.0);
        assert_eq!(f.rows[0].cells[0].fill_argb, Some(0xFFD9E2F3));
    }

    #[test]
    fn nested_table_adds_height_and_is_emitted() {
        let mut inp = simple_input();
        let inner = TableInput {
            grid_pt: vec![40.0, 40.0],
            rows: vec![
                RowInput {
                    cells: vec![cell("內A"), cell("內B")],
                    ..Default::default()
                },
                RowInput {
                    cells: vec![cell("內C"), cell("內D")],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        inp.rows[0].cells[0].nested.push(inner);
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert!(t.row_units[0] >= 2.0, "巢狀表把列撐高到 ≥2 帶");
        let (f, _, _) = t.fragment(0, 0, 99, true, 0.0);
        assert_eq!(f.rows[0].cells[0].nested.len(), 1);
        assert_eq!(f.rows[0].cells[0].nested[0].1.rows[0].cells.len(), 2);
    }

    #[test]
    fn vmerge_block_text_spans_rows_without_duplicate() {
        let mk = |restart: bool, text: &[&str]| CellInput {
            text: text.iter().map(|s| s.to_string()).collect(),
            vmerge: Some(restart),
            ..Default::default()
        };
        let mut inp = TableInput {
            grid_pt: vec![100.0, 100.0],
            rows: vec![
                RowInput {
                    cells: vec![mk(true, &["合併甲"]), cell("右1")],
                    height_pt: Some(8.0),
                    height_exact: true, // 1 帶
                    ..Default::default()
                },
                RowInput {
                    cells: vec![mk(false, &["合併乙"]), cell("右2")],
                    height_pt: Some(8.0),
                    height_exact: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        // 讓 block 需要 2 帶、兩列各 1 帶→餘額補 0，文字跨列。
        inp.rows[0].height_exact = false;
        inp.rows[1].height_exact = false;
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        let (f, _, _) = t.fragment(0, 0, 99, true, 0.0);
        assert_eq!(f.rows.len(), 2);
        let all: Vec<(&str, f32)> = f
            .rows
            .iter()
            .flat_map(|rf| {
                rf.cells[0]
                    .texts
                    .iter()
                    .map(|t| (t.text.as_str(), rf.height_pt))
            })
            .collect();
        let head_texts: Vec<&str> = all.iter().map(|(s, _)| *s).collect();
        assert!(head_texts.contains(&"合併甲"), "head 段在列0切片");
        // 合併乙屬 block 帶 1，落在哪列切片取決於列高；總之必須出現且只出現一次。
        assert_eq!(
            head_texts.iter().filter(|s| **s == "合併乙").count(),
            1,
            "block 文字不遺失、不重複"
        );
        // continue 格不畫水平內邊。
        let last_row = &f.rows[f.rows.len() - 1];
        assert!(last_row.cells[0].edges[EDGE_TOP].is_none());
    }

    #[test]
    fn vmerge_extra_height_goes_to_continue_rows() {
        let mk = |restart: bool, text: &[&str]| CellInput {
            text: text.iter().map(|s| s.to_string()).collect(),
            vmerge: Some(restart),
            ..Default::default()
        };
        let inp = TableInput {
            grid_pt: vec![100.0, 100.0],
            rows: vec![
                RowInput {
                    cells: vec![mk(true, &["一", "二", "三"]), cell("右1")],
                    height_pt: Some(8.0),
                    height_exact: true, // 1 帶
                    ..Default::default()
                },
                RowInput {
                    cells: vec![mk(false, &[]), cell("右2")],
                    height_pt: Some(8.0),
                    height_exact: true, // 1 帶
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert_eq!(t.row_units[0], 1.0, "restart 列保持宣告高");
        assert_eq!(
            t.row_units[1], 2.0,
            "block 3 帶 > 參加列 2 帶→餘額補 continue"
        );
    }

    #[test]
    fn vmerge_bottom_edge_moves_from_head_to_tail() {
        let mk = |restart: bool| CellInput {
            text: vec!["x".into()],
            vmerge: Some(restart),
            borders: if restart {
                Some(EdgeSetInput {
                    bottom: Some(PenInput {
                        val: Some("thick".into()),
                        sz: Some("24".into()),
                        color: None,
                    }),
                    ..Default::default()
                })
            } else {
                None
            },
            ..Default::default()
        };
        let inp = TableInput {
            grid_pt: vec![100.0],
            rows: vec![
                RowInput {
                    cells: vec![mk(true)],
                    ..Default::default()
                },
                RowInput {
                    cells: vec![mk(false)],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        let (f, _, _) = t.fragment(0, 0, 99, true, 0.0);
        assert!(
            f.rows[0].cells[0].edges[EDGE_BOTTOM].is_none(),
            "head 底邊清空"
        );
        let bottom = f.rows[1].cells[0].edges[EDGE_BOTTOM]
            .as_ref()
            .expect("末格承接 block 底邊");
        assert!((bottom.width_pt - 3.0).abs() < 0.001, "thick sz=24 → 3pt");
    }

    #[test]
    fn valign_center_offsets_text_down() {
        let mut inp = simple_input();
        inp.rows[0].height_pt = Some(100.0); // ~6 帶
        inp.rows[0].cells[0].valign = Some("center".into());
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        let (f, _, _) = t.fragment(0, 0, 99, true, 0.0);
        let top_text = f.rows[0].cells[1].texts[0].y_pt;
        let mid_text = f.rows[0].cells[0].texts[0].y_pt;
        assert!(
            mid_text > top_text,
            "center 應低於 top：{mid_text} vs {top_text}"
        );
    }

    #[test]
    fn empty_cell_still_occupies_one_band() {
        let mut inp = simple_input();
        inp.rows[0].cells[0].text.clear();
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert!(t.row_units[0] >= 1.0);
    }

    #[test]
    fn empty_table_is_empty() {
        let t = layout_table(&TableInput::default(), 400.0, 16.8, &measurer(), 0);
        assert!(t.is_empty());
    }

    #[test]
    fn justify_center_offsets_table() {
        let mut inp = simple_input();
        inp.justify = Some("center".into());
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert!(
            (t.x_off - 100.0).abs() < 0.01,
            "200 寬置中於 400 可用寬 → x=100"
        );
    }

    #[test]
    fn fragment_carries_page_coordinates_and_flags() {
        let t = layout_table(&simple_input(), 400.0, 16.8, &measurer(), 0);
        let (f, next, used) = t.fragment(1, 0, 1, false, 72.0);
        assert_eq!(used, 1);
        assert_eq!(next, (2, 0), "末列之後游標出界＝表排完");
        assert!(!f.starts_table && !f.continues);
        assert!((f.x_pt - 72.0).abs() < 0.01, "可用區左緣傳入");
    }

    #[test]
    fn long_cell_text_grows_row_bands() {
        let mut inp = simple_input();
        inp.rows[0].cells[0].text =
            vec!["這是一段很長很長需要斷行很多次才會排完的中文段落文字內容".repeat(4)];
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert!(t.row_units[0] > 1.0, "長文斷行撐高列帶");
        // 斷出的每一行都要落在切片裡
        let (f, _, _) = t.fragment(0, 0, t.total_bands, false, 0.0);
        let n_frag_lines = f.rows.iter().map(|r| r.cells[0].texts.len()).sum::<usize>();
        let n_geom_lines = t.total_bands as usize; // 此表只有列0 撐高
        assert!(n_frag_lines >= 2 && n_frag_lines <= n_geom_lines);
    }
}
