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
//! - vMerge：merged block＝restart 格持有內容；continue 格內容在渲染時被忽略
//!   （ECMA-376＋LO ground truth），自身 block 序列清空。格內 block（段落／
//!   巢狀表）按文件序**垂直累加**堆疊（流式，非並列 max）；文字自 block
//!   頂（restart 列帶 0＋pad_top）逐帶繪製；block 高於參加列總和時餘額補到
//!   continue 列。水平內邊界不畫框線，block 底邊由末參加格承擔（取 restart
//!   格的底框——Word 語義）。
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

/// 格內流式 block：文字段落或巢狀表，**保持文件序**（ECMA-376 §17.4.66：
/// `w:tc` 是 block 序列；真實公文常「段落→子表→段落」交錯，拆成兩個獨立
/// Vec 會丟失先後拓撲序——t_f5d47469 根因 1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CellBlock {
    /// 段落原始字串（run 級粗斜體尚未穿透——與正文同款限制），斷行在本模組完成。
    Text(String),
    /// 巢狀表（遞迴）。`Box` 間接化：`TableInput` 遠大於 `String`，
    /// 直接嵌入會讓整個 enum 膨到 1KB（clippy large_enum_variant，
    /// 與 canonical `Block::Table(Box<Table>)` 同款處置）。
    Table(Box<TableInput>),
    /// 儲存格內圖片（t_a1397522）：尺寸已由 ooxml 層完成 EMU／VML style
    /// 換算（pt）。參與格內垂直堆疊與帶高累加（`ceil(h / line_h)` 帶，
    /// 至少 1 帶）；繪製＝灰底 Rect＋四邊 Rule（尺寸＝extent，寬超出格寬
    /// 時由繪製端鉗到格內寬，不虛構重排）。行內圖（不屬於任何儲存格）
    /// 不經此路徑——engine 以段落錨點另建佔位 Rect。
    Image { w_pt: f32, h_pt: f32 },
}

/// 儲存格輸入。`blocks` 為格內 block 交錯序列（段落／巢狀表，文件序）；
/// 空 vec＝一個空段（Word：空格仍佔一行高）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CellInput {
    /// `w:tcW`（dxa→pt 已換算；auto/nil＝None）。
    pub width: Option<WidthSpec>,
    /// `w:gridSpan`（缺＝1）。
    pub grid_span: u32,
    /// `w:vMerge`：`Some(true)`＝restart、`Some(false)`＝continue、`None`＝非合併。
    pub vmerge: Option<bool>,
    /// 格內 block 序列（段落／子表交錯，保持文件序）。
    pub blocks: Vec<CellBlock>,
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

/// 切片中的一格圖片佔位（t_a1397522）：x/y 為表格本地／切片相對（與
/// CellText 同款座標系），繪製端發射灰底 Rect＋四邊 Rule。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellImage {
    pub x_pt: f32,
    /// 切片內相對 y（列切片頂起算）。
    pub y_pt: f32,
    pub w_pt: f32,
    /// 本切片可見高度（跨頁續排時只含落在切片視窗內的帶段）。
    pub h_pt: f32,
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
    /// 巢狀表切片（y 偏移相對本列切片頂；x 已含本格左緣＋左內邊距＋子表
    /// 自身 x_off，繪製端＝外層 base_x ＋ 本值，見 engine-api push_frag_items）。
    pub nested: Vec<(f32, TableFrag)>,
    /// 圖片佔位切片（t_a1397522；y 相對本列切片頂，座標口徑同 texts）。
    #[serde(default)]
    pub images: Vec<CellImage>,
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

/// 一格內單一 block 的排版結果（垂直流式堆疊的單位；根因 2/3：並列假設→累加）。
#[derive(Debug, Clone, PartialEq)]
enum CellBlockGeom {
    /// 段落：斷行結果＋佔用帶數（空段規則見 pass 1 注）。
    Text { lines: Vec<Line>, bands: u32 },
    /// 巢狀子表：完整排版；帶高＝自身 total_bands（至少 1）。
    Table { layout: TableLayout, bands: u32 },
    /// 圖片佔位（t_a1397522）：extent 尺寸（pt）＋量化帶高 ceil(h/line_h)，
    /// 繪製寬超出可用寬時只縮不放大（Word 原樣溢出，我們鉗進格寬老實近似）。
    Image {
        /// 發射寬（pt，已鉗到可用寬）。
        w_pt: f32,
        /// extent 高（pt；發射Rect 高＝min(h, bands×line_h) 由繪製端自 y 差推得，
        /// 此處只保帶數供跨頁切片裁切）。
        h_pt: f32,
        bands: u32,
    },
}

impl CellBlockGeom {
    fn bands(&self) -> u32 {
        match self {
            CellBlockGeom::Text { bands, .. } => *bands,
            CellBlockGeom::Table { bands, .. } => *bands,
            CellBlockGeom::Image { bands, .. } => *bands,
        }
    }
}

/// 一格的排版結果（欄位座標＋流式 block 序列＋幾何屬性）。
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
    /// 格內 block 排版序列（文件序；vMerge continue 格渲染時內容被忽略→清空）。
    blocks: Vec<CellBlockGeom>,
    /// 內容總帶高＝pad_top ＋ Σ block 帶 ＋ pad_bottom（vMerge 後重算）。
    content_bands: u32,
    edges: [Option<Pen>; 4],
    fill_argb: Option<u32>,
    valign: Valign,
    /// vMerge 歸屬：Some((block 序號, 是否 restart 格))；None＝非合併。
    merge: Option<(usize, bool)>,
    /// vMerge block 的 restart 列序（block 帶座標原點）。
    block_head_row: usize,
    /// vMerge block 的**末**列序（非合併＝自身列；valign 錨定用）。
    block_end_row: usize,
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

    /// 全域帶序 → (row, band) 游標（跨列跳帶用；越界＝(rows.len(), 0)）。
    /// `fragment(row, band, …)` 不代為跨列正規化 band，呼叫端先過此換算。
    fn cursor_at_band(&self, band: u32) -> (usize, u32) {
        let mut r = 0usize;
        while r < self.rows.len() {
            let base = self.row_base[r];
            if band < base + self.rows[r].bands {
                return (r, band - base);
            }
            r += 1;
        }
        (self.rows.len(), 0)
    }

    /// 產出「自 (row, band) 起、至多 `max_bands` 帶」的切片。
    /// 回傳 (切片, 下一游標 (row, band), 本切片實際帶數)。
    ///
    /// `whole_row_only=true`：列不拆（外層 packer 之外尚有整列需求用）。
    /// 巢狀表**不再**用此旗標——t_f5d47469 起子表按外層切片視窗換算的
    /// (inner_start, inner_budget) 逐帶切片（根因 5：整張重畫修復）。
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
                .map(|cg| {
                    frag_of_cell(
                        self,
                        cg,
                        SliceWin {
                            row: r,
                            band: b,
                            take,
                            starts_row,
                            ends_row,
                        },
                    )
                })
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

/// 切片視窗參數（frag_of_cell 用）：列 r 內自帶 b 起取 take 帶。
#[derive(Clone, Copy)]
struct SliceWin {
    row: usize,
    band: u32,
    take: u32,
    starts_row: bool,
    ends_row: bool,
}

/// 一格在切片 (r, b..b+take) 內的顯示形態。
///
/// 流式堆疊模型（根因 4/5/6/7 修復）：格的 block 序列自「內容原點帶」
/// block0＋pad_top 起**依序累加**排布（段落逐帶、子表整塊），每個 block 只在其
/// 與切片視窗 `win0..win1` 相交的部分發射——文字逐帶裁、子表按剩餘帶預算切片
/// （外層視窗換算成子表的 (inner_start, inner_budget)，杜絕整張子表重畫）。
/// valign 以**整個 block 的列高**為基準且各切片一致，續頁不再重複疊加 pad。
fn frag_of_cell(tl: &TableLayout, cg: &CellGeom, w: SliceWin) -> CellFrag {
    let win0 = tl.row_base[w.row] + w.band; // 切片視窗（全域帶）
    let win1 = win0 + w.take;
    let mut edges = cg.edges.clone();
    if !w.starts_row {
        edges[EDGE_TOP] = None; // 列被拆頁處不補造橫線
    }
    if !w.ends_row {
        edges[EDGE_BOTTOM] = None;
    }
    // vMerge：block 內部橫線不出（head 底邊除單格 block 外不畫；continue 頂邊
    // 一律不畫，底邊只有末參加格保留——assign_merge_blocks 已把 head 底框
    // 搬給末格、把 head/中間格的對應邊清空）。
    let block0 = tl.row_base[cg.block_head_row];
    let mut texts = Vec::new();
    let mut nested = Vec::new();
    let mut images = Vec::new();
    if !matches!(cg.merge, Some((_, false))) {
        // 根因 6：valign 錨點＝整個 merge block 的列總高（非單頁切片高 w.h），
        // 且所有切片共用同一 shift——續頁不再錯位、不再重複疊加 pad_top。
        let block_bands = tl.row_base[cg.block_end_row] + tl.rows[cg.block_end_row].bands - block0;
        let shift = valign_shift(
            cg.valign,
            block_bands as f32 * tl.line_h,
            cg.content_bands,
            tl.line_h,
        );
        // 流式掃描：內容原點＋逐 block 累加（根因 2/3/4：max 並列假設→累加）。
        let mut cur = block0 + cg.pad_top_bands;
        'blocks: for bg in &cg.blocks {
            match bg {
                CellBlockGeom::Text { lines, bands } => {
                    let start = cur;
                    for ln in lines {
                        if cur >= win1 {
                            break 'blocks;
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
                    // bands 可多於 lines（空段佔帶）：游標補到 block 尾。
                    cur = cur.max(start + *bands);
                }
                CellBlockGeom::Table { layout, bands } => {
                    let nb0 = cur;
                    let nb1 = cur + *bands;
                    // 根因 5：子表只在與切片視窗相交的帶段發射——外層
                    // win0..win1 換算成子表的 (inner_start, inner_budget)，
                    // 完全在視窗外＝整塊不發射（第 2 頁不再從頭重畫子表）。
                    if nb1 > win0 && nb0 < win1 {
                        let vis0 = nb0.max(win0);
                        let vis1 = nb1.min(win1);
                        // 根因 7：子表左緣加本格左內邊距（cg.text_x_off，
                        // 預設 5.4pt），不再貼死外層格左框線。
                        let (sr, sb) = layout.cursor_at_band(vis0 - nb0);
                        let (f, _, _) = layout.fragment(
                            sr,
                            sb,
                            (vis1 - vis0).max(1),
                            false,
                            cg.x + cg.text_x_off,
                        );
                        nested.push((shift + (vis0 - win0) as f32 * tl.line_h, f));
                    }
                    cur = nb1;
                }
                CellBlockGeom::Image { w_pt, h_pt, bands } => {
                    // 跨頁切片（同子表根因 5 策略）：只在與切片視窗相交的
                    // 帶段發射，Rect 高＝可見帶段（續頁不重複整張）。
                    let ib0 = cur;
                    let ib1 = cur + *bands;
                    if ib1 > win0 && ib0 < win1 {
                        let vis0 = ib0.max(win0);
                        let vis1 = ib1.min(win1);
                        // extent 高與帶量化取小者（帶向上取整的餘量不虛構成圖）。
                        let full_h = h_pt.min(*bands as f32 * tl.line_h);
                        let frac = (vis1 - vis0) as f32 / *bands as f32;
                        images.push(CellImage {
                            x_pt: cg.x + cg.text_x_off,
                            y_pt: (shift + (vis0 - win0) as f32 * tl.line_h).max(0.0),
                            w_pt: *w_pt,
                            h_pt: full_h * frac,
                        });
                    }
                    cur = ib1;
                }
            }
        }
    }
    CellFrag {
        x_pt: cg.x,
        w_pt: cg.w,
        edges,
        fill_argb: cg.fill_argb,
        texts,
        nested,
        images,
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

            // 格內 block 流式堆疊（根因 1/2/3）：段落與子表按文件序逐塊排版，
            // 帶高**累加**而非 max()。ECMA-376 §17.4.66 要求「以子表結尾的 tc
            // 必須補一個空 w:p」——該尾部空段（最後一個 block、文字空、且前方
            // 有子表）是協定填充，**不佔帶高**；填錯會把 7 份正常檔案與 golden
            // 全多撐 1 帶（評審陷阱，測試鎖死）。
            let mut blocks: Vec<CellBlockGeom> = Vec::new();
            if cell.blocks.is_empty() {
                blocks.push(CellBlockGeom::Text {
                    lines: break_lines("", inner_w, m),
                    bands: 1,
                });
            } else {
                let has_table = cell.blocks.iter().any(|b| matches!(b, CellBlock::Table(_)));
                let last_i = cell.blocks.len() - 1;
                for (bi, b) in cell.blocks.iter().enumerate() {
                    match b {
                        CellBlock::Text(t) => {
                            let filler = has_table && bi == last_i && t.trim().is_empty();
                            if filler {
                                blocks.push(CellBlockGeom::Text {
                                    lines: Vec::new(),
                                    bands: 0,
                                });
                            } else {
                                let ls = break_lines(t, inner_w, m);
                                let bands = ls.len().max(1) as u32;
                                blocks.push(CellBlockGeom::Text { lines: ls, bands });
                            }
                        }
                        CellBlock::Table(n) => {
                            if depth < MAX_NEST_DEPTH {
                                let nl = layout_table(n, inner_w, line_h, m, depth + 1);
                                let bands = nl.total_bands.max(1);
                                blocks.push(CellBlockGeom::Table { layout: nl, bands });
                            }
                            // 深度超限：老實略過子表（不虛構幾何）。
                        }
                        CellBlock::Image { w_pt, h_pt } => {
                            // 圖片＝atomic block：帶高 ceil(h/line_h)（至少 1）；
                            // 寬只縮不放大（超出可用寬＝老實鉗進格寬，不虚構溢出）。
                            let bands = (h_pt / line_h).ceil().max(1.0) as u32;
                            blocks.push(CellBlockGeom::Image {
                                w_pt: w_pt.min(inner_w).max(1.0),
                                h_pt: *h_pt,
                                bands,
                            });
                        }
                    }
                }
            }
            let content_bands =
                (pad_top_bands + blocks.iter().map(|b| b.bands()).sum::<u32>() + pad_bottom_bands)
                    .max(1);

            let edges = resolve_cell_edges(cell, input, ri, ci, n_rows, n_cells);
            cells.push(CellGeom {
                col,
                col_span: span,
                x,
                w,
                text_x_off: margins.left,
                pad_top_bands,
                pad_bottom_bands,
                blocks,
                content_bands,
                edges,
                fill_argb: parse_fill(cell.fill.as_deref()),
                valign: match cell.valign.as_deref() {
                    Some("center") => Valign::Center,
                    Some("bottom") => Valign::Bottom,
                    _ => Valign::Top,
                },
                merge: cell.vmerge.map(|restart| (usize::MAX, restart)), // 序號 pass 2 填
                block_head_row: ri,
                block_end_row: ri,
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

/// 找 vMerge block（同欄 col==c 且 span==1 才參與——跨欄格 vMerge v1 不實作）：
/// - head（restart）：保留自身 block 序列；底框搬給末參加格（block 底邊），
///   自身底邊清空。
/// - continue：清空自身 blocks（ECMA-376＋LO ground truth：continue
///   格內容在渲染時被忽略——實測 LO 不顯現 continue 段落，串接顯示是錯的）；
///   頂邊清空；末參加格 block_tail=true 並承接 head 底框。
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

            // continue 格內容在渲染時被忽略（ECMA-376）：不需串接參加格段落；
            // head 保留自身 block 序列，content_bands 維持 pass1 值。
            {
                let hg = rows[head]
                    .cells
                    .iter_mut()
                    .find(|g| g.col == c)
                    .expect("head 存在");
                hg.merge = Some((block, true));
                hg.block_head_row = head;
                hg.block_end_row = last;
                hg.block_tail = last == head;
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
            for (k, row) in rows[head + 1..=last].iter_mut().enumerate() {
                if let Some(g) = row.cells.iter_mut().find(|g| g.col == c && g.col_span == 1) {
                    let rr = head + 1 + k;
                    g.merge = Some((block, false));
                    g.block_head_row = head;
                    g.block_end_row = last;
                    g.block_tail = rr == last;
                    g.blocks.clear();
                    g.content_bands = (g.pad_top_bands + g.pad_bottom_bands).max(1);
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
            blocks: vec![CellBlock::Text(text.to_string())],
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
        inp.rows[0].cells[0]
            .blocks
            .push(CellBlock::Table(Box::new(inner)));
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert!(t.row_units[0] >= 2.0, "巢狀表把列撐高到 ≥2 帶");
        let (f, _, _) = t.fragment(0, 0, 99, true, 0.0);
        assert_eq!(f.rows[0].cells[0].nested.len(), 1);
        assert_eq!(f.rows[0].cells[0].nested[0].1.rows[0].cells.len(), 2);
    }

    #[test]
    fn vmerge_continue_cell_content_is_ignored() {
        // ECMA-376＋LO ground truth：vMerge continue 格的段落在渲染時被忽略，
        // 只有 restart 格內容顯現（實測 LO PDF：continue 文字不出現）。
        let mk = |restart: bool, text: &[&str]| CellInput {
            blocks: text
                .iter()
                .map(|s| CellBlock::Text(s.to_string()))
                .collect(),
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
        inp.rows[0].height_exact = false;
        inp.rows[1].height_exact = false;
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        let (f, _, _) = t.fragment(0, 0, 99, true, 72.0);
        assert_eq!(f.rows.len(), 2);
        let all: Vec<&str> = f
            .rows
            .iter()
            .flat_map(|rf| rf.cells[0].texts.iter().map(|t| t.text.as_str()))
            .collect();
        assert!(all.contains(&"合併甲"), "restart 格內容顯現");
        assert!(
            !all.contains(&"合併乙"),
            "continue 格內容被忽略（ECMA-376），不得出現：{all:?}"
        );
        // continue 格不畫水平內邊。
        let last_row = &f.rows[f.rows.len() - 1];
        assert!(last_row.cells[0].edges[EDGE_TOP].is_none());
    }

    #[test]
    fn vmerge_extra_height_goes_to_continue_rows() {
        let mk = |restart: bool, text: &[&str]| CellInput {
            blocks: text
                .iter()
                .map(|s| CellBlock::Text(s.to_string()))
                .collect(),
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
            blocks: vec![CellBlock::Text("x".into())],
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
        inp.rows[0].cells[0].blocks.clear();
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
        inp.rows[0].cells[0].blocks = vec![CellBlock::Text(
            "這是一段很長很長需要斷行很多次才會排完的中文段落文字內容".repeat(4),
        )];
        let t = layout_table(&inp, 400.0, 16.8, &measurer(), 0);
        assert!(t.row_units[0] > 1.0, "長文斷行撐高列帶");
        // 斷出的每一行都要落在切片裡
        let (f, _, _) = t.fragment(0, 0, t.total_bands, false, 0.0);
        let n_frag_lines = f.rows.iter().map(|r| r.cells[0].texts.len()).sum::<usize>();
        let n_geom_lines = t.total_bands as usize; // 此表只有列0 撐高
        assert!(n_frag_lines >= 2 && n_frag_lines <= n_geom_lines);
    }

    // ---------------- t_f5d47469：儲存格流式 block ＋ 表格疊印七點根因鎖 ----------------

    /// 1×1 外殼表，單格內容自訂（跨頁/流式測試共用）。
    fn shell_table(cell: CellInput) -> TableInput {
        TableInput {
            grid_pt: vec![300.0],
            width: Some(WidthSpec::Pt(300.0)),
            rows: vec![RowInput {
                cells: vec![cell],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// 單欄 n 列子表（每列 1 帶；「內{off+i}」文字供跨頁/多表比對）。
    fn inner_table(rows: usize) -> TableInput {
        inner_table_at(0, rows)
    }

    fn inner_table_at(off: usize, rows: usize) -> TableInput {
        TableInput {
            grid_pt: vec![80.0],
            width: Some(WidthSpec::Pt(80.0)),
            rows: (0..rows)
                .map(|i| RowInput {
                    cells: vec![cell(&format!("內{}", off + i))],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn cell_mixed_paragraphs_and_nested_table_do_not_overlap() {
        // 根因 1/2/4：單格內 Text → Table → Text 三段，各 block y 嚴格遞增、
        // 互不重疊（評審骨架 A-2；子表高 4 帶＝4×16.8＝67.2pt）。
        let c = CellInput {
            blocks: vec![
                CellBlock::Text("Title Before Table".into()),
                CellBlock::Table(Box::new(inner_table(4))),
                CellBlock::Text("Footer After Table".into()),
            ],
            ..Default::default()
        };
        let t = layout_table(&shell_table(c), 400.0, 16.8, &measurer(), 0);
        let (f, _, _) = t.fragment(0, 0, u32::MAX, true, 0.0);
        let fc = &f.rows[0].cells[0];
        assert_eq!(fc.texts.len(), 2, "兩段文字都要發射");
        assert_eq!(fc.nested.len(), 1, "子表發射一次");
        let text_before_y = fc.texts[0].y_pt;
        let nested_y = fc.nested[0].0;
        let text_after_y = fc.texts[1].y_pt;
        let nested_h = fc.nested[0].1.rows.iter().map(|r| r.height_pt).sum::<f32>();
        assert!(
            nested_y >= text_before_y + 16.8 - 0.01,
            "子表必須在 Title 之下：{nested_y} vs {text_before_y}"
        );
        assert!(
            text_after_y >= nested_y + nested_h - 0.01,
            "Footer 必須在整張子表之下：{text_after_y} vs {nested_y}+{nested_h}"
        );
        // 帶高＝累加：1(Title)＋4(子表)＋1(Footer)＝6 帶（無 pad 宣告）。
        assert_eq!(t.row_units[0], 6.0, "垂直累加而非 max 並列");
    }

    #[test]
    fn two_nested_tables_in_one_cell_stack_vertically() {
        // 根因 3：同格兩張子表必須 `+=` 累加（max 假設會整張重疊）。
        let c = CellInput {
            blocks: vec![
                CellBlock::Table(Box::new(inner_table_at(0, 3))),
                CellBlock::Table(Box::new(inner_table_at(3, 2))),
            ],
            ..Default::default()
        };
        let t = layout_table(&shell_table(c), 400.0, 16.8, &measurer(), 0);
        assert_eq!(t.row_units[0], 5.0, "3 帶＋2 帶＝5 帶（max 誤設會得 3）");
        let (f, _, _) = t.fragment(0, 0, u32::MAX, true, 0.0);
        let fc = &f.rows[0].cells[0];
        assert_eq!(fc.nested.len(), 2);
        let (y1, f1) = &fc.nested[0];
        let (y2, f2) = &fc.nested[1];
        let h1 = f1.rows.iter().map(|r| r.height_pt).sum::<f32>();
        assert!(*y2 >= *y1 + h1 - 0.01, "第二張子表必須排在第一張之下");
        // 第二張確實是「內3/內4」而非第一張的重畫。
        let t2: Vec<&str> = f2
            .rows
            .iter()
            .flat_map(|r| r.cells[0].texts.iter().map(|t| t.text.as_str()))
            .collect();
        assert_eq!(t2, vec!["內3", "內4"], "第二張子表內容不得是第一張重畫");
    }

    #[test]
    fn cross_page_container_slices_nested_table_by_window() {
        // 根因 5（最致命）：外層跨頁時，子表只在視窗內的列被發射，第 2 頁
        // 不得出現子表第 0 列重複，第 1 頁子表不得衝破切片底。
        // 構造：段前 1＋子表 6＋段後 1＝8 帶；切片預算 3 帶 → 3 頁。
        let c = CellInput {
            blocks: vec![
                CellBlock::Text("段前".into()),
                CellBlock::Table(Box::new(inner_table(6))),
                CellBlock::Text("段後".into()),
            ],
            ..Default::default()
        };
        let t = layout_table(&shell_table(c), 400.0, 16.8, &measurer(), 0);
        assert_eq!(t.total_bands, 8);
        let mut cur = (0usize, 0u32);
        let mut slices = Vec::new();
        while cur.0 < t.row_count() {
            let (f, next, used) = t.fragment(cur.0, cur.1, 3, false, 72.0);
            if used == 0 {
                break;
            }
            slices.push(f);
            if next == cur {
                break;
            }
            cur = next;
        }
        assert_eq!(slices.len(), 3, "8 帶÷3 → 3 片");
        // 切片 0（帶 0..3）：段前＋子表帶 0..2（內0/內1）。
        let s0c = &slices[0].rows[0].cells[0];
        let s0_inner: Vec<&str> = s0c
            .nested
            .iter()
            .flat_map(|(_, nf)| {
                nf.rows
                    .iter()
                    .flat_map(|r| r.cells[0].texts.iter().map(|t| t.text.as_str()))
            })
            .collect();
        assert_eq!(s0_inner, vec!["內0", "內1"], "第 1 片只發射視窗內的子表列");
        let s0_nested_bottom = s0c.nested[0].0
            + s0c.nested[0]
                .1
                .rows
                .iter()
                .map(|r| r.height_pt)
                .sum::<f32>();
        assert!(
            s0_nested_bottom <= slices[0].rows[0].height_pt + 0.01,
            "子表不得衝破切片底：{s0_nested_bottom} vs {}",
            slices[0].rows[0].height_pt
        );
        // 切片 1（帶 3..6＝內2/內3/內4，段前佔帶 0）：無「內0/內1」重畫。
        let s1c = &slices[1].rows[0].cells[0];
        let s1_inner: Vec<&str> = s1c
            .nested
            .iter()
            .flat_map(|(_, nf)| {
                nf.rows
                    .iter()
                    .flat_map(|r| r.cells[0].texts.iter().map(|t| t.text.as_str()))
            })
            .collect();
        assert_eq!(
            s1_inner,
            vec!["內2", "內3", "內4"],
            "第 2 片只接排視窗內子表列"
        );
        assert!(
            !s1_inner.contains(&"內0"),
            "跨頁續片重複子表頭列＝根因 5 復發：{s1_inner:?}"
        );
        // 切片 2（帶 6..8）：子表帶 6..7（內6？不存在→內5）＋段後。
        let s2c = &slices[2].rows[0].cells[0];
        let s2_inner: Vec<&str> = s2c
            .nested
            .iter()
            .flat_map(|(_, nf)| {
                nf.rows
                    .iter()
                    .flat_map(|r| r.cells[0].texts.iter().map(|t| t.text.as_str()))
            })
            .collect();
        assert_eq!(s2_inner, vec!["內5"]);
        assert_eq!(s2c.texts.last().map(|t| t.text.as_str()), Some("段後"));
    }

    #[test]
    fn trailing_empty_para_after_nested_table_occupies_no_band() {
        // ECMA-376 §17.4.66 陷阱鎖死：以子表結尾的 tc 必補空 w:p——該協定填充
        // 段落**不佔帶高**（若 +1 會把 7 份正常檔與 golden 全撐高 1 帶）。
        let with_filler = CellInput {
            blocks: vec![
                CellBlock::Table(Box::new(inner_table(2))),
                CellBlock::Text(String::new()), // §17.4.66 強制填充
            ],
            ..Default::default()
        };
        let t = layout_table(&shell_table(with_filler), 400.0, 16.8, &measurer(), 0);
        assert_eq!(t.row_units[0], 2.0, "尾部空段不佔帶：只算子表 2 帶");
        // 但「子表＋非空尾段」與「子表＋中間空段」都必須如實佔帶。
        let real_tail = CellInput {
            blocks: vec![
                CellBlock::Table(Box::new(inner_table(2))),
                CellBlock::Text("尾註".into()),
            ],
            ..Default::default()
        };
        let t2 = layout_table(&shell_table(real_tail), 400.0, 16.8, &measurer(), 0);
        assert_eq!(t2.row_units[0], 3.0, "非空尾段照樣佔 1 帶");
        let mid_empty = CellInput {
            blocks: vec![
                CellBlock::Text("上".into()),
                CellBlock::Text(String::new()),
                CellBlock::Table(Box::new(inner_table(2))),
            ],
            ..Default::default()
        };
        let t3 = layout_table(&shell_table(mid_empty), 400.0, 16.8, &measurer(), 0);
        assert_eq!(t3.row_units[0], 4.0, "中間空段仍佔 1 帶（Word 空段佔行）");
    }

    #[test]
    fn nested_table_left_inset_uses_cell_side_margin() {
        // 根因 7：子表左緣＝cg.x＋text_x_off（5.4pt 預設），不再貼死外層框線。
        let c = CellInput {
            blocks: vec![CellBlock::Table(Box::new(inner_table(1)))],
            ..Default::default()
        };
        let t = layout_table(&shell_table(c), 400.0, 16.8, &measurer(), 0);
        let (f, _, _) = t.fragment(0, 0, u32::MAX, true, 0.0);
        let fc = &f.rows[0].cells[0];
        let nested_x = fc.nested[0].1.x_pt;
        assert!(
            (nested_x - 5.4).abs() < 0.01,
            "子表左緣應含左內邊距 5.4pt，實得 {nested_x}"
        );
    }

    #[test]
    fn valign_shift_anchors_on_full_block_across_slices() {
        // 根因 6：center 對齊跨頁時，各切片的 shift 以整個 block 高度為基準，
        // 續片 y 從 (cur-win0) 換算——同一條帶在兩頁的頁內偏移差 = take×line_h。
        let mut c = CellInput {
            blocks: vec![
                CellBlock::Text("甲".into()),
                CellBlock::Text("乙".into()),
                CellBlock::Text("丙".into()),
                CellBlock::Text("丁".into()),
            ],
            valign: Some("center".into()),
            ..Default::default()
        };
        c.margins = Some(CellMarginInput {
            top: Some(0.0),
            bottom: Some(0.0),
            ..Default::default()
        });
        let t = layout_table(&shell_table(c), 400.0, 16.8, &measurer(), 0);
        assert_eq!(t.total_bands, 4);
        let (f1, _, _) = t.fragment(0, 0, 2, false, 0.0);
        let (f2, _, _) = t.fragment(0, 2, 2, false, 0.0);
        // content_bands=4 → 全高 67.2；block 高也 67.2 → shift=0（無偏移錯位）。
        let y1 = f1.rows[0].cells[0].texts[0].y_pt;
        let y2 = f2.rows[0].cells[0].texts[0].y_pt;
        assert!(y1 < 1.0, "無剩餘空間時 center 不該憑空下移：{y1}");
        assert!(y2 < 1.0, "續頁首條文字不疊加 pad：{y2}");
        // 同帶（帶 2）在兩片中的換算一致：第二片的「丙」就在頂。
        assert_eq!(f2.rows[0].cells[0].texts[0].text, "丙");
    }

    // ---------------- t_a1397522：CellBlock::Image（格內圖片佔位） ----------------

    #[test]
    fn cell_image_adds_ceil_bands_and_stacks_after_text() {
        // 文字 1 帶＋圖 ceil(50/16.8)=3 帶＝4 帶；圖 y 接在文字帶之後。
        let c = CellInput {
            blocks: vec![
                CellBlock::Text("圖上文字".into()),
                CellBlock::Image {
                    w_pt: 120.0,
                    h_pt: 50.0,
                },
            ],
            ..Default::default()
        };
        let t = layout_table(&shell_table(c), 400.0, 16.8, &measurer(), 0);
        assert_eq!(t.total_bands, 4, "1＋ceil(50/16.8)=4 帶");
        let (f, _, _) = t.fragment(0, 0, t.total_bands, false, 0.0);
        let cf = &f.rows[0].cells[0];
        assert_eq!(cf.texts.len(), 1);
        assert_eq!(cf.images.len(), 1);
        let im = &cf.images[0];
        assert!(
            (im.y_pt - (cf.texts[0].y_pt + 16.8)).abs() < 0.01,
            "圖 y 接在文字行帶之後：{} vs {}",
            im.y_pt,
            cf.texts[0].y_pt
        );
        assert!((im.w_pt - 120.0).abs() < 0.01);
        // 發射高＝extent（50pt；帶量化餘量不虛構成圖高）。
        assert!(
            (im.h_pt - 50.0).abs() < 0.01,
            "整片可見→高＝extent：{}",
            im.h_pt
        );
    }

    #[test]
    fn cell_image_width_clamps_to_cell_inner_width() {
        // 極寬圖（1000pt）進 300pt 欄：老實鉗進格內寬，不虛構溢出。
        let c = CellInput {
            blocks: vec![CellBlock::Image {
                w_pt: 1000.0,
                h_pt: 20.0,
            }],
            ..Default::default()
        };
        let t = layout_table(&shell_table(c), 400.0, 16.8, &measurer(), 0);
        let (f, _, _) = t.fragment(0, 0, t.total_bands, false, 0.0);
        let im = &f.rows[0].cells[0].images[0];
        assert!(im.w_pt < 300.0, "寬必被鉗進格內：{}", im.w_pt);
        assert!(im.w_pt > 200.0, "鉗到格內可用寬而非任意小：{}", im.w_pt);
    }

    #[test]
    fn cell_image_cross_page_slice_emits_visible_fraction_only() {
        // 單格：文字 1 帶＋圖 ceil(100/16.8)=6 帶＝7 帶；切片預算 3 帶。
        // 首片帶 0..3（文字＋圖前 2 帶）、續片帶 3..7（圖剩 4/6 高）。
        let c = CellInput {
            blocks: vec![
                CellBlock::Text("首".into()),
                CellBlock::Image {
                    w_pt: 100.0,
                    h_pt: 100.0,
                },
            ],
            ..Default::default()
        };
        let t = layout_table(&shell_table(c), 400.0, 16.8, &measurer(), 0);
        assert_eq!(t.total_bands, 7);
        let (f1, next, _) = t.fragment(0, 0, 3, false, 0.0);
        let i1 = &f1.rows[0].cells[0].images[0];
        assert!(
            (i1.h_pt - 100.0 * 2.0 / 6.0).abs() < 0.5,
            "首片只發可見 2/6 帶高：{}",
            i1.h_pt
        );
        assert!(i1.y_pt > 0.0, "首片圖接在文字帶後：{}", i1.y_pt);
        let (f2, _, _) = t.fragment(0, 3, 4, false, 0.0);
        let i2 = &f2.rows[0].cells[0].images[0];
        assert!(
            (i2.h_pt - 100.0 * 4.0 / 6.0).abs() < 0.5,
            "續片發剩餘 4/6 帶高：{}",
            i2.h_pt
        );
        assert!(i2.y_pt < 1.0, "續片圖頂對齊切片頂：{}", i2.y_pt);
        // 兩片帶數和＝圖全高，不重不跳。
        assert!((i1.h_pt + i2.h_pt - 100.0).abs() < 1.0);
        assert_eq!((next.0, next.1), (0, 3), "單列表續排游標＝同列帶 3");
    }
}
