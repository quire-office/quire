//! w:tbl（表格）解析進 canonical model —— Phase3-1a。
//!
//! 定位（PM 補正 1）：本模組是**唯讀投影**。無損 round-trip 的寫回來源仍是
//! 既有 raw part 明文（[`crate::OoxmlDoc::parts`]＋[`crate::zip_struct`]），
//! canonical model 永不參與序列化——屬性未 100% 映射前，改寫回來源必紅
//! byte-level 閘門。本模組純屬讀入，不動 parts／bundle 任何位元組。
//!
//! 覆蓋（ECMA-376 §17.4）：
//! - `w:tblPr`：tblStyle／tblW／tblLayout／jc／tblInd／tblBorders／tblCellMar／shd；
//! - `w:tblGrid`：`w:gridCol/@w:w` 網格列寬（twip→pt 換算，原字串並存）；
//! - `w:tr`→`w:trPr`：cantSplit／trHeight／tblHeader；
//! - `w:tc`→`w:tcPr`：tcW／gridSpan／vMerge／tcBorders／shd／tcMar／vAlign／noWrap。
//!
//! vMerge 依 PM 補正 3 只存 raw 狀態：`Some(Some("restart"))`＝重啟、
//! `Some(Some("continue"))` 或 `Some(None)`＝延續（val 省略＝continue，
//! §17.4.79）、`None`＝非合併。跨列 2D span 矩陣屬 layout 職責（3-1b），
//! 本 crate 絕不計算幾何。
//!
//! lossless 原則（不吞不改寫）：
//! - 讀取的屬性一律保留原始字串（`raw`／`Option<String>` 欄），換算欄只為
//!   下游便利，兩者同源——不存在「解析器歸一化後覆蓋原文」的路徑；
//! - **未讀取的元素與屬性完全不在本投影裡**：原始 XML 永遠住在 `parts`
//!   明文，round-trip 因此零丟失（自產 custom:* 屬性樣本
//!   `fixtures/table_mixed.docx` 過 `roundtrip --check` 即證明，驗收 6）。
//!
//! 巢狀表格（tc 內再放 tbl）遞迴解析；[`Block::Table`] 經 `Box` 間接化避免
//! infinite size（PM 補正 4）。
//!
//! 段落計數口徑：必須與 `lib.rs::parse_document_xml`、`sectpr.rs` 完全一致
//! ——`w:p` 只算 Start 事件（不含自閉合 `w:p/>`），含表格內段落、含
//! AlternateContent 內段落。canonical 樹以 [`Block::Paragraph { index }`]
//! 引用扁平 `paragraphs` 序列索引：只讀引用、內容不複製、索引唯一真相。
//!
//! 已知骨架近似（皆屬 3-1b 演進點，round-trip 不受影響）：
//! - `mc:AlternateContent` 子樹整體略過不展開（Choice/Fallback 是同一內容的
//!   雙份表示，展開會重複計數）；其內表格不進 canonical，但仍在 parts 明文。
//! - `w:sdt`／`w:ins`／`w:del`／`w:customXml` 等透明包裝：其中的 `w:tbl`
//!   會被解析（計數與 corpus 對照一致）。
//! - 邊框/邊距的繼承覆寫（tblBorders→tcBorders）只記錄事實，不做合成。

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// canonical 型別
// ---------------------------------------------------------------------------

/// 一個 `w:gridCol`：網格欄定義。`w_pt` 為 `@w:w`（twip）÷20 換算之 pt；
/// 欄若無 `@w:w`（畸形但存在）則 `w_pt == None`、`raw` 保留原字串。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridCol {
    pub w_pt: Option<f32>,
    pub raw: Option<String>,
}

/// `w:tblW`／`w:tcW` 等 ST_TblWidth（§17.18.90）。原字串如實保留，
/// 換算（dxa→pt、pct→%）屬 layout 職責。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TblWidth {
    /// `@w:w` 原始字串（數值或 "auto"）。元素存在但缺屬性時為 `None`。
    pub w: Option<String>,
    /// `@w:type`：dxa／pct／auto／nil／…
    pub typ: Option<String>,
}

impl TblWidth {
    /// `@w:w` 的數值解（非法字串＝None，不 panic）；單位換算由呼叫端決定。
    pub fn num(&self) -> Option<f64> {
        self.w.as_deref().and_then(|s| s.trim().parse::<f64>().ok())
    }
}

/// 邊框單邊描述（`w:tblBorders`／`w:tcBorders` 的子元素，§17.4.10）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Border {
    /// `@w:val`：single／dashed／dotted／thick／none／nil／hidden／…
    pub val: Option<String>,
    /// `@w:sz`：八分之一點寬度（原字串）。
    pub sz: Option<String>,
    /// `@w:space`：距文字間距。
    pub space: Option<String>,
    /// `@w:color`：RRGGBB／auto／theme 色名。
    pub color: Option<String>,
    /// `@w:themeColor`（不吞）。
    pub theme_color: Option<String>,
    /// `@w:themeShade`（不吞）。
    pub theme_shade: Option<String>,
    /// `@w:themeTint`（不吞）。
    pub theme_tint: Option<String>,
}

impl Border {
    /// 全屬性皆缺（=宣告了邊框元素但沒有任何值）。
    pub fn is_empty(&self) -> bool {
        self.val.is_none()
            && self.sz.is_none()
            && self.space.is_none()
            && self.color.is_none()
            && self.theme_color.is_none()
            && self.theme_shade.is_none()
            && self.theme_tint.is_none()
    }
}

/// 一組邊框（`w:tblBorders`／`w:tcBorders`）：八個位置。缺位＝該位置未定義
/// （繼承 tblBorders→tcBorders 的覆寫語義由 layout 層處理，此處只如實記錄）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Borders {
    pub top: Option<Border>,
    pub left: Option<Border>,
    pub bottom: Option<Border>,
    pub right: Option<Border>,
    pub inside_h: Option<Border>,
    pub inside_v: Option<Border>,
    /// 僅 `w:tcBorders`：左上→右下對角線。
    pub tl2br: Option<Border>,
    /// 僅 `w:tcBorders`：右上→左下對角線。
    pub tr2bl: Option<Border>,
}

impl Borders {
    /// 八方位全缺。
    pub fn is_empty(&self) -> bool {
        [
            self.top.as_ref(),
            self.left.as_ref(),
            self.bottom.as_ref(),
            self.right.as_ref(),
            self.inside_h.as_ref(),
            self.inside_v.as_ref(),
            self.tl2br.as_ref(),
            self.tr2bl.as_ref(),
        ]
        .iter()
        .all(|b| b.is_none_or(|x| x.is_empty()))
    }
}

/// `w:shd`／`w:tblShd`（ST_Shd，§17.18.66）：底紋。`fill` 為背景色、
/// `color` 為紋路色（val≠clear 時才有意義）。原字串保留。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Shading {
    pub val: Option<String>,
    pub color: Option<String>,
    pub fill: Option<String>,
    pub theme_fill: Option<String>,
    pub theme_color: Option<String>,
}

impl Shading {
    /// 全屬性皆缺。
    pub fn is_empty(&self) -> bool {
        self.val.is_none()
            && self.color.is_none()
            && self.fill.is_none()
            && self.theme_fill.is_none()
            && self.theme_color.is_none()
    }
}

/// 邊距（`w:tblCellMar`／`w:tcMar`，§17.4.40／§17.4.49）：每側一筆 ST_TblWidth。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Margins {
    pub top: Option<TblWidth>,
    /// 新式名（bidi 感知）。
    pub start: Option<TblWidth>,
    pub bottom: Option<TblWidth>,
    /// 新式名。
    pub end: Option<TblWidth>,
    /// 舊式名（僅表格級 `w:tblCellMar`；與 start 可共存，皆保留）。
    pub left: Option<TblWidth>,
    /// 舊式名（僅表格級）。
    pub right: Option<TblWidth>,
    /// 僅儲存格級：左上對角（斜線儲存格用）。
    pub top_left: Option<TblWidth>,
    /// 僅儲存格級：右上對角。
    pub top_right: Option<TblWidth>,
    /// 僅儲存格級：右下對角。
    pub bottom_right: Option<TblWidth>,
    /// 僅儲存格級：左下對角。
    pub bottom_left: Option<TblWidth>,
}

impl Margins {
    /// 十方位全缺（或存在但無值）。
    pub fn is_empty(&self) -> bool {
        let Self {
            top,
            start,
            bottom,
            end,
            left,
            right,
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } = self;
        [
            top,
            start,
            bottom,
            end,
            left,
            right,
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        ]
        .iter()
        .all(|m| m.as_ref().is_none_or(|t| t.w.is_none() && t.typ.is_none()))
    }
}

/// `w:tblPr`（表格屬性，§17.4.79）。未列出的屬性（如 `w:tblLook`、
/// `w:cnfStyle`、custom:* 自訂命名空間）不在本投影——它們原樣活在
/// `parts` 明文（唯讀來源隔離，PM 補正 1）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableProps {
    /// `w:tblStyle/@w:val`：樣式 id。
    pub style: Option<String>,
    pub width: Option<TblWidth>,
    /// `w:tblLayout/@w:type == "fixed"`。`None`＝元素不存在（預設 autofit）。
    pub layout_fixed: Option<bool>,
    /// `w:tblLayout/@w:type` 原字串（fixed／autofit）。
    pub layout_raw: Option<String>,
    pub borders: Option<Borders>,
    pub shading: Option<Shading>,
    pub cell_margins: Option<Margins>,
    /// `w:jc/@w:val`（表格對齊）。
    pub justify: Option<String>,
    /// `w:tblInd/@w:w` 原字串。
    pub indent_raw: Option<String>,
    /// `w:tblInd/@w:type`。
    pub indent_typ: Option<String>,
}

/// `w:trPr`（列屬性，§17.4.77）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RowProps {
    /// `w:cantSplit` 存在且未明確關掉（`@w:val` 非 "0"/"false"）：整列不可拆頁。
    pub cant_split: bool,
    /// `w:cantSplit` 元素是否存在（存在但被關掉也要區分）。
    pub cant_split_present: bool,
    /// `w:trHeight/@w:val` 原字串。
    pub height_raw: Option<String>,
    /// `w:trHeight/@w:hRule`：atLeast／exact／auto。
    pub height_rule: Option<String>,
    /// `w:trHeight/@w:val == "0"`：Word 以此允許此列拆行（覆蓋 cantSplit）。
    pub height_allow_break: bool,
    /// `w:tblHeader` 存在：跨頁時重複此列（頁首列）。
    pub header: bool,
    /// `w:tblHeader/@w:repeat` 原字串（"0"＝不重複）。
    pub header_raw: Option<String>,
}

/// `w:tcPr`（儲存格屬性，§17.4.69）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CellProps {
    pub width: Option<TblWidth>,
    /// `w:gridSpan/@w:val` 原字串（缺＝None，下游按 1）。
    pub grid_span_raw: Option<String>,
    /// `w:vMerge`：外層 `Option`＝元素存在與否；內層 `Option`＝`@w:val`
    /// 存在與否（省略＝continue，PM 補正 3）。原字串不歸一化。
    pub vmerge: Option<Option<String>>,
    pub borders: Option<Borders>,
    pub shading: Option<Shading>,
    pub margins: Option<Margins>,
    /// `w:vAlign/@w:val`：top／center／bottom（缺＝None，預設 top）。
    pub valign: Option<String>,
    /// `w:noWrap`（存在且未明確關掉）。
    pub no_wrap: bool,
}

impl CellProps {
    /// gridSpan 有效值（缺／非法＝1，不 panic——容錯鐵律）。
    pub fn grid_span(&self) -> u32 {
        self.grid_span_raw
            .as_deref()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(1)
    }

    /// vMerge 有效語義：`None`＝非合併；`Some(true)`＝restart；
    /// `Some(false)`＝continue（含 val 省略的寫法）。
    pub fn vmerge_restart(&self) -> Option<bool> {
        self.vmerge
            .as_ref()
            .map(|v| matches!(v.as_deref(), Some("restart" | "1" | "true")))
    }
}

/// block 級元素序列項：段落（以扁平索引引用 [`crate::OoxmlDoc::paragraphs`]）
/// 或巢狀表格。`Table` 變體經 `Box` 間接化（PM 補正 4：遞迴型別需堆指針）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Block {
    /// 引用主解析器扁平 `paragraphs` 序列的第 `i` 段（只讀投影，內容不複製）。
    Paragraph { index: usize },
    /// 巢狀表格（tc 內再放 tbl，§17.4.67）。
    Table(Box<Table>),
}

/// `w:tr`：列屬性＋儲存格序列。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableRow {
    pub props: RowProps,
    pub cells: Vec<TableCell>,
}

/// `w:tc`：tcPr＋block 內容序列（段落／巢狀表格）。
/// 容錯：tc 內無任何 block 時老實回空 Vec——OOXML 合法表示要求至少一個
/// `w:p`，真實畸形檔存在空 tc，此處不虚構段落。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableCell {
    pub props: CellProps,
    pub blocks: Vec<Block>,
}

/// `w:tbl` 完整投影：tblPr＋tblGrid＋列序列。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub props: TableProps,
    /// `w:tblGrid/w:gridCol` 序列。無 tblGrid 時為空 Vec——老實回報缺件，
    /// 不虚構欄寬（corpus 的 selfgen/cjk_traditional_report.docx 即如此）。
    pub grid: Vec<GridCol>,
    pub rows: Vec<TableRow>,
}

impl Table {
    /// 網格欄數（`w:gridCol` 數量）。corpus 閘門：此數必須與
    /// `w:tblGrid` 內 `w:gridCol` 出現數一致（PM 補正 2：tc 數因
    /// gridSpan／畸形表本就可能不等欄數，不得斷言 tc 數一致）。
    pub fn grid_col_count(&self) -> usize {
        self.grid.len()
    }

    /// 遍歷本表與全部巢狀表（含自身），供全域統計／驗證。
    pub fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Table)) {
        visit(self);
        for row in &self.rows {
            for cell in &row.cells {
                for b in &cell.blocks {
                    if let Block::Table(inner) = b {
                        inner.walk(visit);
                    }
                }
            }
        }
    }
}

/// body 級表格：canonical 暴露給 layout（3-1b）的 block-level 入口。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyTable {
    /// 此表開始時「其前已計入的段落數」＝表格內容（若有）第一個段落在扁平
    /// `paragraphs` 序列的索引。表格在 block 序列中位於第 `para_start - 1`
    /// 段之後、第 `para_start` 段之前。
    pub para_start: usize,
    pub table: Table,
}

impl BodyTable {
    /// 遍歷本表與全部巢狀表。
    pub fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Table)) {
        self.table.walk(visit);
    }
}

// ---------------------------------------------------------------------------
// 解析器：quick-xml 事件流＋顯式框架棧（單趟；遞迴結構以棧展開）
// ---------------------------------------------------------------------------

/// 進行中的元素框架：每個正在開啟、且本模組「看得懂位置」的元素一幀。
/// 大payload變體（Table／Tr／Tc）經 `Box`：Frame 棧保持小體積，
/// 也穩壓 clippy large_enum_variant（PM 補正 4 同款間接化）。
enum Frame {
    /// `w:tbl`：累積中的表格。
    Table {
        para_start: usize,
        table: Box<Table>,
    },
    /// `w:tblPr`：其子元素寫入最近 Table 幀的 props。
    TblPr,
    /// `w:tblBorders`。
    TblBorders,
    /// `w:tblCellMar`。
    TblCellMar,
    /// `w:tblGrid`。
    Grid,
    /// `w:tr`：進行中的列（cells 由 tc 完結時推入）。
    Tr { row: Box<TableRow> },
    /// `w:trPr`。
    TrPr,
    /// `w:tc`：進行中的儲存格。
    Tc { cell: Box<TableCell> },
    /// `w:tcPr`。
    TcPr,
    /// `w:tcBorders`。
    TcBorders,
    /// `w:tcMar`。
    TcMar,
    /// `w:p`：只為計數與包圍子樹；內容（runs）屬其他模組職責。
    P,
    /// 透明包裝（`w:sdt`、`w:ins`、`w:del`、`w:customXml`…）：記名供 End
    /// 配對；子元素照常解析（其中的 w:tbl 仍進 canonical）。
    Opaque { name: Vec<u8> },
    /// 吞沒子樹（僅 `mc:AlternateContent`）：內部任何東西都不進 canonical
    /// （Choice/Fallback 雙份），只維護深度；段落計數照常（與主解析器同口徑）。
    Skip { depth: usize },
}

impl Frame {
    /// End 事件配對：此幀由哪個 local name 開啟。
    fn closed_by(&self, name: &[u8]) -> bool {
        match self {
            Frame::Table { .. } => name == b"tbl",
            Frame::TblPr => name == b"tblPr",
            Frame::TblBorders => name == b"tblBorders",
            Frame::TblCellMar => name == b"tblCellMar",
            Frame::Grid => name == b"tblGrid",
            Frame::Tr { .. } => name == b"tr",
            Frame::TrPr => name == b"trPr",
            Frame::Tc { .. } => name == b"tc",
            Frame::TcPr => name == b"tcPr",
            Frame::TcBorders => name == b"tcBorders",
            Frame::TcMar => name == b"tcMar",
            Frame::P => name == b"p",
            Frame::Opaque { name: n } => n.as_slice() == name,
            Frame::Skip { .. } => name == b"AlternateContent",
        }
    }

    fn row_mut(&mut self) -> &mut TableRow {
        match self {
            Frame::Tr { row } => row,
            _ => unreachable!("row_mut 僅用於 Tr 幀索引"),
        }
    }
    fn cell_mut(&mut self) -> &mut TableCell {
        match self {
            Frame::Tc { cell } => cell,
            _ => unreachable!("cell_mut 僅用於 Tc 幀索引"),
        }
    }
}

/// 單趟表格解析器狀態。
struct TableParser {
    frames: Vec<Frame>,
    /// 全局 `w:p` Start 計數——與 parse_document_xml／sectpr 同口徑。
    p_count: usize,
    /// 已完成的 body 級表格（含開始錨點）。
    roots: Vec<BodyTable>,
}

/// `w:tbl` 開始位置的結構判定。
enum TblParent {
    /// body 級（透明包裝不算壁壘）。
    Body,
    /// 某儲存格內（巢狀表）。
    Cell,
    /// 非法位置（段落內、另一表的中間結構內、AlternateContent 內）。
    Invalid,
}

impl TableParser {
    fn new() -> Self {
        Self {
            frames: Vec::new(),
            p_count: 0,
            roots: Vec::new(),
        }
    }

    /// 棧頂是否為指定元素之一。
    fn top_is(&self, names: &[&[u8]]) -> bool {
        self.frames
            .last()
            .is_some_and(|f| names.iter().any(|n| f.closed_by(n)))
    }

    /// 結構位置守衛（由內而外）：穿透非壁壘幀（屬性幀、透明包裝如列級／
    /// 儲存格級 `w:sdt`、`w:ins`），先碰到 `targets` → true，先碰到
    /// `barriers` → false。真實文件的行級 sdt 會插在 `w:tr` 與 `w:tc`
    /// 之間（corpus HC031／tika-3816 實測），嚴格「棧頂配對」會漏列。
    fn under(&self, targets: &[&[u8]], barriers: &[&[u8]]) -> bool {
        for f in self.frames.iter().rev() {
            if targets.iter().any(|n| f.closed_by(n)) {
                return true;
            }
            if barriers.iter().any(|b| f.closed_by(b)) {
                return false;
            }
        }
        false
    }

    /// 由內而外找最近的儲存格幀；`Table`／`Tr`／`P`／`Skip` 為壁壘
    /// （跨過它們抓外層儲存格會把內容錯掛）。
    fn innermost_cell(&self) -> Option<usize> {
        for (i, f) in self.frames.iter().enumerate().rev() {
            match f {
                Frame::Tc { .. } => return Some(i),
                Frame::Table { .. } | Frame::Tr { .. } | Frame::P | Frame::Skip { .. } => {
                    return None
                }
                _ => continue,
            }
        }
        None
    }

    /// 由內而外找最近的列幀；`Table`／`Tc` 為壁壘（列絕不合法巢狀穿過
    /// 儲存格——畸形輸入跨過壁壘會把 trPr 錯掛到外層列）。
    fn innermost_row(&self) -> Option<usize> {
        for (i, f) in self.frames.iter().enumerate().rev() {
            match f {
                Frame::Tr { .. } => return Some(i),
                Frame::Table { .. } | Frame::Tc { .. } | Frame::P | Frame::Skip { .. } => {
                    return None
                }
                _ => continue,
            }
        }
        None
    }

    /// 由內而外找最近的表格幀。
    fn innermost_table(&mut self) -> Option<&mut Table> {
        self.frames.iter_mut().rev().find_map(|f| match f {
            Frame::Table { table, .. } => Some(&mut **table),
            _ => None,
        })
    }

    /// `w:tbl` 開始時的位置。透明包裝（Opaque）與屬性幀穿透；
    /// P／Skip／Table／Tr 為壁壘。
    fn tbl_parent(&self) -> TblParent {
        for f in self.frames.iter().rev() {
            match f {
                Frame::Tc { .. } => return TblParent::Cell,
                Frame::Table { .. } | Frame::Tr { .. } | Frame::P | Frame::Skip { .. } => {
                    return TblParent::Invalid
                }
                _ => continue,
            }
        }
        TblParent::Body
    }

    fn on_start(&mut self, name: &[u8], empty: bool, e: &quick_xml::events::BytesStart) {
        // 段落計數：任何 `w:p` Start 都算（口徑＝主解析器）；自閉合不算。
        // 於 Skip／P 早退之前執行——AlternateContent 內的段落照計（雙份），
        // canonical 不掛 block，但扁平計數不受影響。
        if name == b"p" && !empty {
            self.p_count += 1;
        }

        // 頂端是吞沒幀：只維護深度，其餘一概不看（AlternateContent 全子樹）。
        if let Some(Frame::Skip { depth }) = self.frames.last_mut() {
            if !empty {
                *depth += 1;
            }
            return;
        }
        // 頂端是段落幀：段落內容（runs/pPr…）非本模組職責，深度包圍之。
        // （合法上 w:p 內不會再有 w:tbl／w:p；畸形由包圍幀兜住。）
        if matches!(self.frames.last(), Some(Frame::P)) && name != b"p" {
            if !empty {
                self.frames.push(Frame::Opaque {
                    name: name.to_vec(),
                });
            }
            return;
        }

        match name {
            b"p" => {
                if !empty {
                    // 先掛 block（Frame::P 是 innermost_cell 壁壘，push 後就掛不上）
                    // 再進包圍幀。
                    if let Some(i) = self.innermost_cell() {
                        self.frames[i].cell_mut().blocks.push(Block::Paragraph {
                            index: self.p_count - 1,
                        });
                    }
                    self.frames.push(Frame::P);
                }
            }
            b"tbl" => match self.tbl_parent() {
                TblParent::Invalid => {
                    if !empty {
                        self.frames.push(Frame::Opaque {
                            name: name.to_vec(),
                        });
                    }
                }
                _ => {
                    if empty {
                        // <w:tbl/> 自閉合（畸形）：空表即刻完結。
                        self.finish_table(Box::new(Table::default()), self.p_count);
                    } else {
                        self.frames.push(Frame::Table {
                            para_start: self.p_count,
                            table: Box::new(Table::default()),
                        });
                    }
                }
            },
            // ---- tblPr 及其直接子女（tblPr／tblGrid／tr 以 under() 容忍
            // 行級 sdt 等透明包裝穿插；其子女一律直貼父幀，top_is 即可）----
            b"tblPr"
                if self.under(
                    &[b"tbl"],
                    &[b"tr", b"tc", b"trPr", b"tcPr", b"P", b"AlternateContent"],
                ) =>
            {
                self.frames.push(Frame::TblPr)
            }
            b"tblStyle" if self.top_is(&[b"tblPr"]) => {
                let v = attr(e, b"val");
                if let Some(t) = self.innermost_table() {
                    t.props.style = v;
                }
            }
            b"tblW" if self.top_is(&[b"tblPr"]) => {
                let w = read_tblwidth(e);
                if let Some(t) = self.innermost_table() {
                    t.props.width = Some(w);
                }
            }
            b"tblLayout" if self.top_is(&[b"tblPr"]) => {
                let raw = attr(e, b"type");
                if let Some(t) = self.innermost_table() {
                    t.props.layout_fixed = raw.as_deref().map(|s| s == "fixed");
                    t.props.layout_raw = raw;
                }
            }
            b"jc" if self.top_is(&[b"tblPr"]) => {
                let v = attr(e, b"val");
                if let Some(t) = self.innermost_table() {
                    t.props.justify = v;
                }
            }
            b"tblInd" if self.top_is(&[b"tblPr"]) => {
                let (w, ty) = (attr(e, b"w"), attr(e, b"type"));
                if let Some(t) = self.innermost_table() {
                    t.props.indent_raw = w;
                    t.props.indent_typ = ty;
                }
            }
            b"tblShd" | b"shd" if self.top_is(&[b"tblPr"]) => {
                let s = read_shd(e);
                if let Some(t) = self.innermost_table() {
                    t.props.shading = Some(s);
                }
            }
            b"tblBorders" if self.top_is(&[b"tblPr"]) => self.frames.push(Frame::TblBorders),
            b"tblCellMar" if self.top_is(&[b"tblPr"]) => self.frames.push(Frame::TblCellMar),
            // ---- tblGrid ----
            b"tblGrid"
                if self.under(
                    &[b"tbl"],
                    &[b"tr", b"tc", b"trPr", b"P", b"AlternateContent"],
                ) =>
            {
                self.frames.push(Frame::Grid)
            }
            b"gridCol" if self.under(&[b"tblGrid"], &[b"tbl", b"tr", b"tc", b"tblPr"]) => {
                let raw = attr(e, b"w");
                let w_pt = raw.as_deref().and_then(parse_f64).map(|v| v as f32 / 20.0);
                if let Some(t) = self.innermost_table() {
                    t.grid.push(GridCol { w_pt, raw });
                }
            }
            // ---- tr / trPr（tr 用 under()：行間可插行級 sdt 包裝）----
            b"tr"
                if self.under(
                    &[b"tbl"],
                    &[
                        b"tr",
                        b"tc",
                        b"trPr",
                        b"tcPr",
                        b"tcBorders",
                        b"tcMar",
                        b"P",
                        b"AlternateContent",
                    ],
                ) =>
            {
                if empty {
                    if let Some(t) = self.innermost_table() {
                        t.rows.push(TableRow {
                            props: RowProps::default(),
                            cells: Vec::new(),
                        });
                    }
                } else {
                    self.frames.push(Frame::Tr {
                        row: Box::new(TableRow {
                            props: RowProps::default(),
                            cells: Vec::new(),
                        }),
                    });
                }
            }
            b"trPr" if self.under(&[b"tr"], &[b"tbl", b"tc", b"tcPr", b"P"]) => {
                self.frames.push(Frame::TrPr)
            }
            b"cantSplit" if self.top_is(&[b"trPr"]) => {
                let off = matches!(attr(e, b"val").as_deref(), Some("0") | Some("false"));
                if let Some(i) = self.innermost_row() {
                    let props = &mut self.frames[i].row_mut().props;
                    props.cant_split_present = true;
                    props.cant_split = !off;
                }
            }
            b"trHeight" if self.top_is(&[b"trPr"]) => {
                let v = attr(e, b"val");
                let rule = attr(e, b"hRule");
                if let Some(i) = self.innermost_row() {
                    let props = &mut self.frames[i].row_mut().props;
                    props.height_rule = rule;
                    props.height_allow_break = matches!(v.as_deref(), Some("0"));
                    props.height_raw = v;
                }
            }
            b"tblHeader" if self.top_is(&[b"trPr"]) => {
                let rep = attr(e, b"repeat");
                if let Some(i) = self.innermost_row() {
                    let props = &mut self.frames[i].row_mut().props;
                    props.header = true;
                    props.header_raw = rep;
                }
            }
            // ---- tc / tcPr（tc 用 under()：列內可插行級 sdt 包裝儲存格；
            // tcPr 同理容忍 tc 首層包裝）----
            b"tc"
                if self.under(
                    &[b"tr"],
                    &[
                        b"tbl",
                        b"tr",
                        b"tc",
                        b"tcPr",
                        b"tcBorders",
                        b"tcMar",
                        b"P",
                        b"AlternateContent",
                    ],
                ) =>
            {
                if empty {
                    if let Some(i) = self.innermost_row() {
                        self.frames[i].row_mut().cells.push(TableCell {
                            props: CellProps::default(),
                            blocks: Vec::new(),
                        });
                    }
                } else {
                    self.frames.push(Frame::Tc {
                        cell: Box::new(TableCell {
                            props: CellProps::default(),
                            blocks: Vec::new(),
                        }),
                    });
                }
            }
            b"tcPr"
                if self.under(
                    &[b"tc"],
                    &[b"tbl", b"tr", b"tcPr", b"tcBorders", b"tcMar", b"P"],
                ) =>
            {
                self.frames.push(Frame::TcPr)
            }
            b"tcW" if self.top_is(&[b"tcPr"]) => {
                let w = read_tblwidth(e);
                if let Some(i) = self.innermost_cell() {
                    self.frames[i].cell_mut().props.width = Some(w);
                }
            }
            b"gridSpan" if self.top_is(&[b"tcPr"]) => {
                let v = attr(e, b"val");
                if let Some(i) = self.innermost_cell() {
                    self.frames[i].cell_mut().props.grid_span_raw = v;
                }
            }
            b"vMerge" if self.top_is(&[b"tcPr"]) => {
                // 元素存在即 Some(...)；val 省略 → Some(None)＝continue（PM 補正 3）。
                let v = attr(e, b"val");
                if let Some(i) = self.innermost_cell() {
                    self.frames[i].cell_mut().props.vmerge = Some(v);
                }
            }
            b"tcBorders" if self.top_is(&[b"tcPr"]) => self.frames.push(Frame::TcBorders),
            b"tcMar" if self.top_is(&[b"tcPr"]) => self.frames.push(Frame::TcMar),
            b"shd" if self.top_is(&[b"tcPr"]) => {
                let s = read_shd(e);
                if let Some(i) = self.innermost_cell() {
                    self.frames[i].cell_mut().props.shading = Some(s);
                }
            }
            b"vAlign" if self.top_is(&[b"tcPr"]) => {
                let v = attr(e, b"val");
                if let Some(i) = self.innermost_cell() {
                    self.frames[i].cell_mut().props.valign = v;
                }
            }
            b"noWrap" if self.top_is(&[b"tcPr"]) => {
                let off = matches!(attr(e, b"val").as_deref(), Some("0") | Some("false"));
                if let Some(i) = self.innermost_cell() {
                    self.frames[i].cell_mut().props.no_wrap = !off;
                }
            }
            // ---- 邊框八方位（tblBorders／tcBorders 共用元素名）----
            b"top" | b"left" | b"bottom" | b"right" | b"insideH" | b"insideV" | b"tl2br"
            | b"tr2bl"
                if self.top_is(&[b"tblBorders"]) || self.top_is(&[b"tcBorders"]) =>
            {
                let b = read_border(e);
                if self.top_is(&[b"tblBorders"]) {
                    if let Some(t) = self.innermost_table() {
                        apply_border(
                            t.props.borders.get_or_insert_with(Borders::default),
                            name,
                            b,
                        );
                    }
                } else if let Some(i) = self.innermost_cell() {
                    let borders = self.frames[i]
                        .cell_mut()
                        .props
                        .borders
                        .get_or_insert_with(Borders::default);
                    apply_border(borders, name, b);
                }
            }
            // ---- 邊距方位（tblCellMar／tcMar 共用）----
            b"top" | b"bottom" | b"left" | b"right" | b"start" | b"end" | b"topLeft"
            | b"topRight" | b"bottomRight" | b"bottomLeft"
                if self.top_is(&[b"tblCellMar"]) || self.top_is(&[b"tcMar"]) =>
            {
                let w = read_tblwidth(e);
                if self.top_is(&[b"tblCellMar"]) {
                    if let Some(t) = self.innermost_table() {
                        apply_margin(
                            t.props.cell_margins.get_or_insert_with(Margins::default),
                            name,
                            w,
                        );
                    }
                } else if let Some(i) = self.innermost_cell() {
                    let m = self.frames[i]
                        .cell_mut()
                        .props
                        .margins
                        .get_or_insert_with(Margins::default);
                    apply_margin(m, name, w);
                }
            }
            // ---- AlternateContent：吞沒雙份表示 ----
            b"AlternateContent" => {
                if !empty {
                    self.frames.push(Frame::Skip { depth: 0 });
                }
            }
            // 其餘 Start：透明包裝入棧（子樹繼續看）；Empty 不入棧。
            _ => {
                if !empty {
                    self.frames.push(Frame::Opaque {
                        name: name.to_vec(),
                    });
                }
            }
        }
    }

    fn on_end(&mut self, name: &[u8]) {
        // Skip 頂端：深度優先；深度 0 時的同名 End 才弹出 Skip 幀。
        if let Some(Frame::Skip { depth }) = self.frames.last() {
            let last = *depth;
            if last == 0 {
                if name == b"AlternateContent" {
                    self.frames.pop();
                }
                // 非同名 End 出现在 Skip 頂端＝標籤錯置（畸形）：忽略。
            } else if let Some(Frame::Skip { depth }) = self.frames.last_mut() {
                *depth -= 1;
            }
            return;
        }
        // 常規：顶端配對→弹出完結；未配對則向下找同名幀（畸形容忍：
        // 彈掉其上所有幀——未閉合結構老實丟棄，明文仍在 parts）。
        let mut idx = None;
        if let Some(last) = self.frames.last() {
            if last.closed_by(name) {
                idx = Some(self.frames.len() - 1);
            }
        }
        if idx.is_none() {
            idx = (0..self.frames.len())
                .rev()
                .find(|&i| self.frames[i].closed_by(name));
        }
        let Some(i) = idx else { return };
        // drain 而非 remove：同名幀之上的滯留幀（標籤錯置殘骸）一併老實丟棄。
        let frame = self.frames.drain(i..).next().expect("drain 起點必存在");
        match frame {
            Frame::Table { para_start, table } => self.finish_table(table, para_start),
            Frame::Tr { row } => {
                // 列完結：cells 已由 tc End 逐一推入。
                if let Some(t) = self.innermost_table() {
                    t.rows.push(*row);
                }
            }
            Frame::Tc { cell } => {
                // 儲存格完結：掛到目前的列。無列（畸形）則老實遺棄。
                if let Some(ri) = self.innermost_row() {
                    self.frames[ri].row_mut().cells.push(*cell);
                }
            }
            _ => {} // 屬性／包圍幀：值已就地寫入，無需完結。
        }
    }

    /// 表格完結：巢狀→掛到外層儲存格；body 級→roots。
    fn finish_table(&mut self, table: Box<Table>, para_start: usize) {
        if let Some(i) = self.innermost_cell() {
            self.frames[i].cell_mut().blocks.push(Block::Table(table));
        } else {
            self.roots.push(BodyTable {
                para_start,
                table: *table,
            });
        }
    }
}

fn apply_border(tb: &mut Borders, pos: &[u8], b: Border) {
    match pos {
        b"top" => tb.top = Some(b),
        b"left" => tb.left = Some(b),
        b"bottom" => tb.bottom = Some(b),
        b"right" => tb.right = Some(b),
        b"insideH" => tb.inside_h = Some(b),
        b"insideV" => tb.inside_v = Some(b),
        b"tl2br" => tb.tl2br = Some(b),
        b"tr2bl" => tb.tr2bl = Some(b),
        _ => {}
    }
}

fn apply_margin(m: &mut Margins, side: &[u8], w: TblWidth) {
    match side {
        b"top" => m.top = Some(w),
        b"bottom" => m.bottom = Some(w),
        b"left" => m.left = Some(w),
        b"right" => m.right = Some(w),
        b"start" => m.start = Some(w),
        b"end" => m.end = Some(w),
        b"topLeft" => m.top_left = Some(w),
        b"topRight" => m.top_right = Some(w),
        b"bottomRight" => m.bottom_right = Some(w),
        b"bottomLeft" => m.bottom_left = Some(w),
        _ => {}
    }
}

fn read_tblwidth(e: &quick_xml::events::BytesStart) -> TblWidth {
    TblWidth {
        w: attr(e, b"w"),
        typ: attr(e, b"type"),
    }
}

fn read_border(e: &quick_xml::events::BytesStart) -> Border {
    Border {
        val: attr(e, b"val"),
        sz: attr(e, b"sz"),
        space: attr(e, b"space"),
        color: attr(e, b"color"),
        theme_color: attr(e, b"themeColor"),
        theme_shade: attr(e, b"themeShade"),
        theme_tint: attr(e, b"themeTint"),
    }
}

fn read_shd(e: &quick_xml::events::BytesStart) -> Shading {
    Shading {
        val: attr(e, b"val"),
        color: attr(e, b"color"),
        fill: attr(e, b"fill"),
        theme_fill: attr(e, b"themeFill"),
        theme_color: attr(e, b"themeColor"),
    }
}

fn parse_f64(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key {
            a.unescape_value().ok().map(|v| v.to_string())
        } else {
            None
        }
    })
}

/// 由 `word/document.xml` 明文解析全部 body 級表格（含巢狀投影）。
///
/// 純函數、只讀；段落計數與主解析器同口徑（見模組頭）。XML 尾部截斷或
/// 標籤錯置不 panic——未閉合結構老實丟棄（明文仍在 parts，零失真）。
pub(crate) fn read_body_tables(xml: &[u8]) -> Vec<BodyTable> {
    use quick_xml::events::Event;
    let text = String::from_utf8_lossy(xml);
    let mut re = quick_xml::Reader::from_str(&text);
    let mut tp = TableParser::new();

    // Err＝尾部截斷／畸形：容忍，回報已收集部分（sectpr 同款策略）。
    while let Ok(ev) = re.read_event() {
        match ev {
            Event::Eof => break,
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_vec();
                tp.on_start(&name, false, &e);
            }
            Event::Empty(e) => {
                let name = e.local_name().as_ref().to_vec();
                tp.on_start(&name, true, &e);
            }
            Event::End(e) => tp.on_end(e.local_name().as_ref()),
            _ => {}
        }
    }

    tp.roots
}

// ---------------------------------------------------------------------------
// 測試
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE_MIXED: &[u8] = include_bytes!("../../../fixtures/table_mixed.docx");

    fn doc_xml_of(bytes: &[u8]) -> Vec<u8> {
        let doc = crate::parse(bytes).expect("開啟 docx 失敗");
        doc.parts["word/document.xml"].clone()
    }

    /// 取 block 序列首項的段落索引（斷言失敗時打出實際結構）。
    fn first_para_index(blocks: &[Block]) -> usize {
        match blocks.first() {
            Some(Block::Paragraph { index }) => *index,
            other => panic!("期待段落 block 於首位，實際 {other:?}"),
        }
    }

    /// 自產樣本：結構 golden 斷言（合併列／合併行／巢狀表／邊框底紋）。
    #[test]
    fn table_mixed_golden_structure() {
        let xml = doc_xml_of(TABLE_MIXED);
        let tables = read_body_tables(&xml);
        assert_eq!(tables.len(), 1, "body 級表恰一張（巢狀表掛 tc 內）");
        let bt = &tables[0];
        assert_eq!(bt.para_start, 1, "表格前有 1 個 body 段落");
        let t = &bt.table;

        // ---- tblPr ----
        assert_eq!(t.props.style.as_deref(), Some("TableGrid"));
        let w = t.props.width.clone().expect("tblW");
        assert_eq!(w.w.as_deref(), Some("9000"));
        assert_eq!(w.typ.as_deref(), Some("dxa"));
        assert_eq!(w.num(), Some(9000.0));
        assert_eq!(t.props.layout_fixed, Some(true));
        assert_eq!(t.props.layout_raw.as_deref(), Some("fixed"));
        let tb = t.props.borders.clone().expect("tblBorders");
        assert_eq!(tb.top.as_ref().expect("top").val.as_deref(), Some("single"));
        assert_eq!(
            tb.inside_v.as_ref().expect("insideV").val.as_deref(),
            Some("dotted")
        );
        assert!(tb.tl2br.is_none(), "表格級無對角線");
        assert!(tb.bottom.as_ref().unwrap().theme_color.is_none());

        // ---- tblGrid：欄數與寬度（twip÷20＝pt）----
        assert_eq!(t.grid_col_count(), 3);
        assert_eq!(t.grid[0].w_pt, Some(100.0), "2000 twip = 100 pt");
        assert_eq!(t.grid[1].w_pt, Some(175.0), "3500 twip = 175 pt");
        assert_eq!(t.grid[2].raw.as_deref(), Some("3500"), "原字串保留");

        // ---- 列 ----
        assert_eq!(t.rows.len(), 4);
        // 列 0：cantSplit＋tblHeader；單格橫跨三欄＋底紋
        let r0 = &t.rows[0];
        assert!(r0.props.cant_split && r0.props.cant_split_present);
        assert!(r0.props.header, "w:tblHeader → 頁首列");
        assert_eq!(r0.cells.len(), 1);
        assert_eq!(r0.cells[0].props.grid_span(), 3);
        assert_eq!(
            r0.cells[0]
                .props
                .shading
                .as_ref()
                .expect("shd")
                .fill
                .as_deref(),
            Some("D9E2F3")
        );
        // 列 1：三欄；首欄邊框＋底紋＋置中（custom:* 未知屬性不進投影）
        let r1 = &t.rows[1];
        assert_eq!(r1.cells.len(), 3);
        let c = &r1.cells[0].props;
        assert_eq!(c.valign.as_deref(), Some("center"));
        let top = c
            .borders
            .as_ref()
            .expect("tcBorders")
            .top
            .as_ref()
            .expect("top");
        assert_eq!(top.val.as_deref(), Some("single"));
        assert_eq!(top.sz.as_deref(), Some("12"));
        assert_eq!(top.color.as_deref(), Some("8B0000"));
        assert_eq!(
            c.borders
                .as_ref()
                .unwrap()
                .left
                .as_ref()
                .unwrap()
                .val
                .as_deref(),
            Some("dashed")
        );
        assert_eq!(c.shading.as_ref().unwrap().val.as_deref(), Some("solid"));
        // 列 2：vMerge restart＋巢狀表（blocks：段落後接 Table）
        let r2 = &t.rows[2];
        assert_eq!(r2.cells[0].props.vmerge_restart(), Some(true));
        assert_eq!(r2.cells[1].props.grid_span(), 2);
        let blocks = &r2.cells[1].blocks;
        assert_eq!(blocks.len(), 2, "段落＋巢狀表");
        assert_eq!(blocks[0], Block::Paragraph { index: 6 });
        let Block::Table(nested) = &blocks[1] else {
            panic!("第二 block 應為巢狀表格，實際 {:?}", blocks[1])
        };
        assert_eq!(nested.grid_col_count(), 2);
        assert_eq!(nested.grid[0].w_pt, Some(75.0));
        assert_eq!(nested.props.layout_fixed, Some(true));
        assert_eq!(nested.rows.len(), 1);
        assert_eq!(nested.rows[0].cells.len(), 2);
        // 列 3：vMerge continue（<w:vMerge/> 省略 val）；第二欄兩段落
        let r3 = &t.rows[3];
        assert_eq!(
            r3.cells[0].props.vmerge,
            Some(None),
            "val 省略必須區分為 Some(None)＝continue"
        );
        assert_eq!(r3.cells[0].props.vmerge_restart(), Some(false));
        assert_eq!(
            r3.cells[1].blocks,
            vec![
                Block::Paragraph { index: 10 },
                Block::Paragraph { index: 11 }
            ]
        );

        // walk：巢狀遍歷計 2 表
        let mut n = 0;
        bt.walk(&mut |_| n += 1);
        assert_eq!(n, 2);
    }

    /// 段落索引口徑：canonical 引用的 index 必須指到扁平段落清單的正確段落。
    /// table_mixed 的 w:p 序列：0 表前、1 財務摘要、2..4 列1 三欄、5 備註、
    /// 6 含附註、7 內層A、8 內層B、9 空段、10..11 續排兩段、12 表後。
    #[test]
    fn paragraph_indices_match_flat_list() {
        let doc = crate::parse(TABLE_MIXED).expect("open");
        let texts = doc.paragraph_texts();
        assert_eq!(texts.len(), 13, "扁平段落清單（含表格／巢狀表內段落）");
        let t = &doc.tables[0];
        assert_eq!(
            first_para_index(&t.table.rows[0].cells[0].blocks),
            1,
            "列0首格段落索引"
        );
        assert_eq!(
            texts[first_para_index(&t.table.rows[0].cells[0].blocks)],
            "財務摘要"
        );
        let Block::Table(nested) = &t.table.rows[2].cells[1].blocks[1] else {
            panic!("應為巢狀表")
        };
        let i = first_para_index(&nested.rows[0].cells[0].blocks);
        assert_eq!(texts[i], "內層A", "巢狀表段落同樣走扁平索引");
    }

    /// 驗收 6：未知屬性（custom:* 命名空間、tblLook、cnfStyle）round-trip
    /// 不丟失——解析後 save 仍與原始位元組完全相同（寫回來源仍是 parts 明文）。
    #[test]
    fn unknown_table_attrs_survive_round_trip() {
        let doc = crate::parse(TABLE_MIXED).expect("open");
        let xml = String::from_utf8_lossy(&doc.parts["word/document.xml"]).into_owned();
        // 樣本自證：確有未讀屬性（自訂命名空間＋本卡未投影元素）。
        assert!(xml.contains("custom:tblNote"), "樣本應含自訂命名空間屬性");
        assert!(xml.contains("w:tblLook"), "樣本應含未投影元素 w:tblLook");
        // 且這些屬性確實不在 canonical 投影裡（證明「不吞」靠明文而非投影）。
        assert!(doc.tables[0].table.props.justify.is_none());
        let out = doc.save().expect("save");
        assert_eq!(
            out.as_slice(),
            TABLE_MIXED,
            "含未知屬性表格檔 round-trip 必須位元組級相同"
        );
    }

    /// vMerge 三態（PM 補正 3）與 gridSpan 缺值容錯。
    #[test]
    fn vmerge_raw_states_and_gridspan_defaults() {
        let xml = br#"<w:document xmlns:w="x"><w:body>
            <w:tbl><w:tr><w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p/></w:tc>
            <w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>
            <w:tc><w:p/></w:tc>
            <w:tc><w:tcPr><w:gridSpan/></w:tcPr><w:p/></w:tc>
            <w:tc><w:tcPr><w:gridSpan w:val="not-a-number"/></w:tcPr><w:p/></w:tc>
            </w:tr></w:tbl>
            </w:body></w:document>"#;
        let tables = read_body_tables(xml);
        assert_eq!(tables.len(), 1);
        let cells = &tables[0].table.rows[0].cells;
        assert_eq!(cells.len(), 5);
        assert_eq!(cells[0].props.vmerge_restart(), Some(true));
        assert_eq!(cells[1].props.vmerge_restart(), Some(false));
        assert_eq!(cells[2].props.vmerge, None);
        assert_eq!(cells[3].props.grid_span(), 1, "缺 val＝1");
        assert_eq!(cells[4].props.grid_span(), 1, "非法 val＝1，不 panic");
    }

    /// 畸形容錯：未閉合表格不 panic、自閉合 tbl 為空表、段內 tbl 不進 canonical。
    #[test]
    fn malformed_tables_never_panic() {
        // 尾部截斷：回報已收集部分即可，不死迴圈不 panic。
        let xml = "<w:document xmlns:w=\"x\"><w:body><w:tbl><w:tblGrid><w:gridCol w:w=\"100\"/>\n            <w:tr><w:tc><w:p><w:r><w:t>截"
            .as_bytes();
        let tables = read_body_tables(xml);
        assert!(tables.len() <= 1);
        // 自閉合 tbl：空表老實完結。
        let xml = br#"<w:body><w:tbl/><w:p/></w:body>"#;
        let tables = read_body_tables(xml);
        assert_eq!(tables.len(), 1);
        assert!(tables[0].table.rows.is_empty());
        // 段內包 tbl（非法位置）：不進 canonical，不 panic。
        let xml = br#"<w:body><w:p><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:p></w:body>"#;
        let tables = read_body_tables(xml);
        assert!(tables.is_empty(), "段內 tbl 不進 canonical");
        // 標籤錯置（tr 直接閉在 tbl 內、tc 閉合順序錯亂）：不死迴圈。
        let xml = br#"<w:body><w:tbl><w:tblGrid></w:tr></w:tc><w:tc/></w:tbl></w:body>"#;
        let _ = read_body_tables(xml);
    }

    /// 段落計數口徑：AlternateContent 內的 w:p 照計（與主解析器同口徑），
    /// 但其內表格不進 canonical（Choice/Fallback 雙份只取非 MC 路徑）。
    #[test]
    fn alternate_content_skipped_but_counted() {
        let xml = "<w:document xmlns:w=\"x\" xmlns:mc=\"m\"><w:body>\n            <w:p><w:r><w:t>前</w:t></w:r></w:p>\n            <mc:AlternateContent><mc:Choice Requires=\"wps\">\n              <w:tbl><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr><w:tc><w:p><w:t>MC</w:t></w:p></w:tc></w:tr></w:tbl>\n            </mc:Choice></mc:AlternateContent>\n            <w:tbl><w:tblGrid><w:gridCol w:w=\"200\"/></w:tblGrid></w:tbl>\n            </w:body></w:document>"
            .as_bytes();
        let tables = read_body_tables(xml);
        assert_eq!(tables.len(), 1, "MC 表略過，只見真表");
        assert_eq!(
            tables[0].para_start, 2,
            "MC 內 w:p 計入扁平口徑（前1＋MC內1）"
        );
        assert_eq!(tables[0].table.grid_col_count(), 1);
        assert_eq!(tables[0].table.grid[0].w_pt, Some(10.0));
    }

    /// corpus 閘門（PM 補正 2）：manifest 標 table 的全部檔案，解析不 panic，
    /// 且解析出的 column 定義數與 `w:tblGrid` 的 `w:gridCol` 出現數一致
    /// （AlternateContent 子樹除外——那本子樹不進 canonical，見上測）。
    #[test]
    fn corpus_table_files_parse_and_grid_counts_match() {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/manifest.json");
        let manifest_txt = std::fs::read_to_string(&manifest).expect("讀 corpus/manifest.json");
        let corpus_root = manifest.parent().unwrap().to_path_buf();
        let tagged = table_tagged_files(&manifest_txt);
        assert!(
            tagged.len() >= 50,
            "manifest table 標籤檔案數異常：{}",
            tagged.len()
        );
        for rel in &tagged {
            let path = corpus_root.join(rel);
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("讀 {path:?}: {e}"));
            let xml = doc_xml_of(&bytes);
            let tables = read_body_tables(&xml); // 不 panic 即第一閘門
                                                 // 全域對照：gridCol 出現數（排除 AlternateContent 子樹）。
            let expect = count_tag_outside_alt(&xml, b"<w:gridCol");
            let got: usize = tables.iter().map(grid_cols_of_root).sum();
            assert_eq!(
                got, expect,
                "{rel}：解析欄數 {got} ≠ 原始 gridCol 數 {expect}"
            );
        }
    }

    fn grid_cols_of_root(bt: &BodyTable) -> usize {
        let mut n = 0;
        bt.walk(&mut |t| n += t.grid_col_count());
        n
    }

    /// 從 manifest 文字挑出 tags 含 "table" 的檔案清單（不引 serde_json：
    /// 以 `"file": "` 切塊，塊內取 tags 區段查 `"table"`）。
    fn table_tagged_files(manifest_txt: &str) -> Vec<String> {
        let mut out = Vec::new();
        for chunk in manifest_txt.split("\"file\": \"").skip(1) {
            let file = chunk.split('"').next().unwrap_or_default().to_string();
            if let Some(tag_start) = chunk.split_once("\"tags\"").map(|(_, r)| r) {
                let upto = tag_start.find(']').unwrap_or(tag_start.len());
                if tag_start[..upto].contains("\"table\"") {
                    out.push(file);
                }
            }
        }
        out
    }

    /// 統計 `tag`（如 `<w:gridCol`）在 XML 中的出現次數（含邊界檢查），
    /// 排除 `mc:AlternateContent` 子樹。
    fn count_tag_outside_alt(xml: &[u8], tag: &[u8]) -> usize {
        let spans = alt_spans(xml);
        let mut n = 0;
        let mut from = 0usize;
        while let Some(i) = find_bytes(xml, tag, from) {
            let after = i + tag.len();
            let boundary_ok = xml
                .get(after)
                .is_some_and(|c| matches!(c, b'>' | b' ' | b'/' | b'\t' | b'\r' | b'\n'));
            if boundary_ok && !spans.iter().any(|(s, e)| i >= *s && i < *e) {
                n += 1;
            }
            from = after;
        }
        n
    }

    /// mc:AlternateContent 最外層區間集合（巢狀 MC 合併進外層；尾部截斷
    /// 以檔尾收尾——容忍，與解析器 Skip 行為一致）。
    fn alt_spans(xml: &[u8]) -> Vec<(usize, usize)> {
        const OPEN: &[u8] = b"<mc:AlternateContent";
        const CLOSE: &[u8] = b"</mc:AlternateContent>";
        let mut spans = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        let mut i = 0usize;
        loop {
            match (find_bytes(xml, OPEN, i), find_bytes(xml, CLOSE, i)) {
                (Some(o), Some(c)) if o < c => {
                    stack.push(o);
                    i = o + OPEN.len();
                }
                (_, Some(c)) => {
                    if let Some(s) = stack.pop().filter(|_| stack.is_empty()) {
                        spans.push((s, c + CLOSE.len()));
                    }
                    i = c + CLOSE.len();
                }
                _ => break,
            }
        }
        if let Some(s) = stack.first() {
            spans.push((*s, xml.len()));
        }
        spans
    }

    fn find_bytes(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
        if from > hay.len() || needle.is_empty() {
            return None;
        }
        hay[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|p| from + p)
    }

    /// 回歸：既有 sample.docx（無表格）不得受表格解析影響。
    #[test]
    fn table_free_doc_yields_no_tables() {
        let xml = doc_xml_of(include_bytes!("../../../fixtures/sample.docx"));
        assert!(read_body_tables(&xml).is_empty());
    }
}
