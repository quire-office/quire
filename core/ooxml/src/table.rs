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
//! ——`w:p` 只算 Start 事件（不含自閉合 `w:p/>`），含表格內段落、含巢狀段落
//! （文字框 `w:txbxContent` 內的 `w:p` 在主解析器各佔一槽），`mc:Fallback`
//! 內不計（Choice/Fallback 雙份只取 Choice 一份）。canonical 樹以
//! [`Block::Paragraph { index }`] 引用扁平 `paragraphs` 序列索引：只讀引用、
//! 內容不複製、索引唯一真相。
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
    ///
    /// TODO（未支援）：框線的主題色尚未查表（`w:themeColor` → theme1.xml 的
    /// clrScheme，見 `crate::theme` 模組）——目前一律當「非 hex」處理。
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
    /// `@w:themeFill`（不吞）。
    ///
    /// TODO（未支援）：底紋的主題色尚未查表（`w:themeFill` → theme1.xml 的
    /// clrScheme，見 `crate::theme` 模組的槽位對照表）——目前只保留原字串。
    pub theme_fill: Option<String>,
    /// `@w:themeColor`（不吞；紋路色，見上欄 TODO）。
    pub theme_color: Option<String>,
    /// `@w:themeFillTint`（K8-shd：底色朝白混合量，ST_PosPercentage 的
    /// 十六進位 byte 寫法，`3F`＝63/255）。變換實作見 `crate::theme::apply_tint`
    /// ＋`table_map` 的底色級聯；cache `@w:fill` 優先於主題推導值。
    #[serde(default)]
    pub theme_fill_tint: Option<String>,
    /// `@w:themeFillShade`（K8-shd：底色朝黑混合量，同上色序寫法）。
    #[serde(default)]
    pub theme_fill_shade: Option<String>,
}

impl Shading {
    /// 全屬性皆缺。
    pub fn is_empty(&self) -> bool {
        self.val.is_none()
            && self.color.is_none()
            && self.fill.is_none()
            && self.theme_fill.is_none()
            && self.theme_color.is_none()
            && self.theme_fill_tint.is_none()
            && self.theme_fill_shade.is_none()
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

/// `w:tblLook`（表格條件格式開關，§17.4.75）：**條件樣式的生效閘**。
///
/// K8-shd：`w:tblStylePr` 的條件底紋（firstRow／band1Horz／band1Vert…）
/// 只有一律經本閘才可生效——fdo68607 實測 `noVBand="1"` 把縱帶整體閘死
/// （LO p2 像素剖面：evenHBand 行在 band1Vert 宣告欄仍白），`noHBand="0"`
/// 讓橫帶放行。只讀不看錯位（`w:cnfStyle` 未投影）。
///
/// 兩種寫法（ECMA-376 §17.4.75）：`@w:val` 是位元旗標、六個具名屬性是
/// 同意義的人讀形式——**具名屬性優先**，缺位才從 `@w:val` 解位；兩者全缺
/// 走 ECMA 預設（=0x04A0：firstRow／firstColumn 開、縱帶關）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableLook {
    /// `@w:val` 原字串（4 位 hex 旗標；不歸一化大小寫）。
    pub val: Option<String>,
    pub first_row: Option<bool>,
    pub last_row: Option<bool>,
    pub first_column: Option<bool>,
    pub last_column: Option<bool>,
    /// `@w:noHBand`：**反相**旗標（1＝橫帶關）。
    pub no_h_band: Option<bool>,
    /// `@w:noVBand`：**反相**旗標（1＝縱帶關）。
    pub no_v_band: Option<bool>,
}

impl TableLook {
    /// `@w:val` 的位元旗標（ECMA-376 §17.4.75 位對照）。未知低位元老實忽略
    /// （corpus 實測有 `00B7` 等帶保留位的寫法，不得因它猜不出而放棄整條）。
    fn bit(val: Option<&str>, mask: u32) -> Option<bool> {
        let raw = val?.trim().trim_start_matches("0x").trim();
        let v = u32::from_str_radix(raw, 16).ok()?;
        Some(v & mask != 0)
    }

    /// 六項生效旗標：具名屬性 > `@w:val` 位元 > ECMA 預設（0x04A0）。
    ///
    /// 回傳值已把 noHBand／noVBand **反相**成正向語義（`h_band`＝橫帶開）。
    pub fn effective(&self) -> EffectiveLook {
        // 正向旗標：具名 > 位元 > ECMA 預設（0x04A0：firstRow／firstColumn
        // 開、lastRow／lastColumn 關）。
        let pick = |named: Option<bool>, mask: u32, dflt: bool| -> bool {
            named
                .or_else(|| Self::bit(self.val.as_deref(), mask))
                .unwrap_or(dflt)
        };
        // 反相旗標（no*）：1＝關。ECMA 預設 noHBand=0（橫帶開）、
        // noVBand=1（縱帶關）——取回值後一律取反換成正向「開」。
        let pick_off = |named: Option<bool>, mask: u32, dflt_off: bool| -> bool {
            !named
                .or_else(|| Self::bit(self.val.as_deref(), mask))
                .unwrap_or(dflt_off)
        };
        EffectiveLook {
            first_row: pick(self.first_row, 0x20, true),
            last_row: pick(self.last_row, 0x40, false),
            first_column: pick(self.first_column, 0x80, true),
            last_column: pick(self.last_column, 0x100, false),
            h_band: pick_off(self.no_h_band, 0x200, false),
            v_band: pick_off(self.no_v_band, 0x400, true),
        }
    }
}

/// [`TableLook::effective`] 的落地結果（正向語義；缺件走 ECMA 預設）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EffectiveLook {
    pub first_row: bool,
    pub last_row: bool,
    pub first_column: bool,
    pub last_column: bool,
    /// 橫帶（band1Horz／band2Horz）放行。
    pub h_band: bool,
    /// 縱帶（band1Vert／band2Vert）放行。
    pub v_band: bool,
}

/// `w:tblPr`（表格屬性，§17.4.79）。未列出的屬性（`w:cnfStyle`、custom:*
/// 自訂命名空間）不在本投影——它們原樣活在 `parts` 明文（唯讀來源隔離，
/// PM 補正 1）。`w:tblLook` K8-shd 起投影（條件底紋的生效閘）。
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
    /// `w:tblpPr/@w:tblpY` 原字串（浮動表格垂直偏移）。
    #[serde(default)]
    pub tblp_y_raw: Option<String>,
    /// `w:tblLook`（K8-shd：條件底紋的生效閘；缺＝None，下游走 ECMA 預設）。
    #[serde(default)]
    pub look: Option<TableLook>,
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
    /// 吞沒子樹（`mc:AlternateContent`／裸 `mc:Fallback`）：內部任何東西都
    /// 不進 canonical（Choice/Fallback 是同一內容的雙份表示），只維護深度。
    /// 段落計數由 `TableParser::fb`（Fallback 巢狀深度，parser 級）把關：
    /// `fb == 0`（Choice 路徑）才計——與主解析器「只解析 Choice 一份」同口徑。
    Skip {
        depth: usize,
        /// 完結此幀的 local name（`AlternateContent` 或 `Fallback`）。
        close: &'static [u8],
    },
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
            Frame::Skip { close, .. } => name == *close,
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
    /// `mc:Fallback` 巢狀深度（parser 級）：>0 時其內 `w:p` 不入扁平口徑
    /// （主解析器整棵吞沒 Fallback，只解析 Choice 一份）。
    fb: usize,
    /// K4e2：`w:txbxContent` 文字框子樹深度。>0 時其內 `w:p` **不入扁平口徑**
    /// （主解析器把框內段落掛進外層段落的 `txbx_texts`，不佔正文槽；四解析器
    /// 同口徑鐵律的改寫點——見 `parse_document_xml`／`drawing::count_paras`／
    /// `sectpr`）。
    txbx_depth: usize,
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
            fb: 0,
            txbx_depth: 0,
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
        // ---- K4e2：txbxContent 深度追蹤（先於 Skip 幀處理：Choice 路徑內的
        // 文字框也要跟；Fallback 內不跟——主解析器整棵吞沒 Fallback）----
        if self.fb == 0 && name == b"txbxContent" && !empty {
            self.txbx_depth += 1;
        }
        // 頂端是吞沒幀：只維護深度＋parser 級 Fallback 計數，其餘一概不看。
        // 段落計數：Choice 路徑（fb == 0）照計，Fallback 內不計——主解析器
        // 只解析 Choice 一份，扁平口徑三者必須一致（parse_document_xml／sectpr）。
        if let Some(Frame::Skip { depth, close }) = self.frames.last_mut() {
            if !empty {
                if name == b"Fallback" && close == b"AlternateContent" {
                    // AC 幀內的 Fallback 子樹：parser 級計數把關段落口徑
                    // （Choice 一份、Fallback 不計）。
                    self.fb += 1;
                } else {
                    if name == b"p" && self.fb == 0 && close == b"AlternateContent" {
                        // K4e2：框內段落不佔正文槽（主解析器掛 txbx_texts）。
                        if self.txbx_depth == 0 {
                            self.p_count += 1;
                        }
                    }
                    *depth += 1;
                }
            }
            return;
        }
        // 段落計數：任何 `w:p` Start 都算（口徑＝主解析器：Start 佔槽、自閉合
        // 不算、Fallback 內不計）；於 P 早退之前執行——巢狀段落（文字框
        // `w:txbxContent` 內的 `w:p`）自 K4e2 起**不佔正文槽**（主解析器改掛
        // `Paragraph::txbx_texts`），此處以 txbx_depth 同步跳過。
        if name == b"p" && !empty && self.fb == 0 && self.txbx_depth == 0 {
            self.p_count += 1;
        }
        // 頂端是段落幀：段落內容（runs/pPr…）非本模組職責，深度包圍之。
        // （合法上 w:p 內不會再有 w:tbl／w:p；畸形由包圍幀兜住。例外：
        // AC／Fallback 要走到下方 match 拿 Skip 幀，否則其雙份口徑失守。）
        if matches!(self.frames.last(), Some(Frame::P))
            && !matches!(name, b"p" | b"AlternateContent" | b"Fallback")
        {
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
            b"tblLook" if self.top_is(&[b"tblPr"]) => {
                // K8-shd：條件底紋的生效閘（§17.4.75）。只讀不看錯位
                // （`w:cnfStyle` 未投影）；round-trip 仍走 parts 明文，本
                // 投影純讀取。
                let look = TableLook {
                    val: attr(e, b"val"),
                    first_row: attr_bool(e, b"firstRow"),
                    last_row: attr_bool(e, b"lastRow"),
                    first_column: attr_bool(e, b"firstColumn"),
                    last_column: attr_bool(e, b"lastColumn"),
                    no_h_band: attr_bool(e, b"noHBand"),
                    no_v_band: attr_bool(e, b"noVBand"),
                };
                if let Some(t) = self.innermost_table() {
                    t.props.look = Some(look);
                }
            }
            b"tblShd" | b"shd" if self.top_is(&[b"tblPr"]) => {
                let s = read_shd(e);
                if let Some(t) = self.innermost_table() {
                    t.props.shading = Some(s);
                }
            }
            b"tblpPr" if self.top_is(&[b"tblPr"]) => {
                let y = attr(e, b"tblpY");
                if let Some(t) = self.innermost_table() {
                    t.props.tblp_y_raw = y;
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
                // R2：`w:val`（ECMA-376 §17.4.86 布林）與舊 `w:repeat` 皆須
                // 判 off——「標記存在即頁首列」在 val="0"（取消重複）時會虛重貼。
                let rep = attr(e, b"repeat");
                let val = attr(e, b"val");
                let off = matches!(val.as_deref(), Some("0") | Some("false"))
                    || matches!(rep.as_deref(), Some("0") | Some("false"));
                if let Some(i) = self.innermost_row() {
                    let props = &mut self.frames[i].row_mut().props;
                    props.header = !off;
                    props.header_raw = val.or(rep);
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
            // ---- 邊框八方位（tblBorders／tcBorders 共用元素名，含 start/end 相容）----
            b"top" | b"left" | b"bottom" | b"right" | b"insideH" | b"insideV" | b"tl2br"
            | b"tr2bl" | b"start" | b"end"
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
            // ---- AlternateContent／裸 Fallback：吞沒雙份表示 ----
            b"AlternateContent" => {
                if !empty {
                    self.frames.push(Frame::Skip {
                        depth: 0,
                        close: b"AlternateContent",
                    });
                }
            }
            b"Fallback" => {
                // 裸 Fallback（無 AlternateContent 包裝，畸形但容錯）：單獨入幀
                // 吞沒；AC 內的 Fallback 由 parser 級 fb 計數處理，不另入幀。
                if !empty {
                    self.frames.push(Frame::Skip {
                        depth: 0,
                        close: b"Fallback",
                    });
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
        // ---- K4e2：txbxContent 完結（先於 Skip 幀處理，與主解析器同位：
        // 主解析器在 fb>0 時整棵跳過，故此處也只在 fb==0 時遞減）----
        if self.fb == 0 && name == b"txbxContent" {
            self.txbx_depth = self.txbx_depth.saturating_sub(1);
        }
        // Skip 頂端：parser 級 Fallback 計數優先（</mc:Fallback> 出棧）；
        // 深度 0 時同名 End 才彈出 Skip 幀。
        if let Some(Frame::Skip { depth, close }) = self.frames.last() {
            let (d, close) = (*depth, *close);
            if name == b"Fallback" && self.fb > 0 {
                self.fb -= 1;
                return;
            }
            if d == 0 {
                if name == close {
                    self.frames.pop();
                }
                // 非同名 End 出現在 Skip 頂端＝標籤錯置（畸形）：忽略。
            } else if let Some(Frame::Skip { depth, .. }) = self.frames.last_mut() {
                *depth -= 1;
            }
            return;
        }
        // 常規：顶端配對→彈出完結；未配對則向下找同名幀（畸形容忍：
        // 彈掉其上所有幀——未閉合結構老實丟棄，明文仍在 parts）。
        // Skip 幀已被錯置標籤提前彈出時，</mc:Fallback> 在此兜底降計，
        // 避免 fb 卡死導致其後所有段落漏計（主解析器無幀、必然遞減）。
        if name == b"Fallback" {
            self.fb = self.fb.saturating_sub(1);
        }
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
    fn finish_table(&mut self, mut table: Box<Table>, para_start: usize) {
        inherit_vmerge_borders(&mut table);
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

/// D6：依 ECMA-376 §17.4.4 實作垂直合併儲存格（vMerge）的邊框繼承。
///
/// 規則：
/// 1. 2D 欄位對齊：各列儲存格依 `grid_span` 累加計算起始欄位與結束欄位 `[col_start, col_end)`。
/// 2. 垂直合併鏈：在相同欄位區間上，以 `vMerge == restart` 為 head，後續相鄰列的
///    `vMerge == continue` 為 continuation cells。鏈中最後一格為 tail（末格），其餘為 mid。
/// 3. 邊框繼承語義：
///    - tail（末格）：承接合併區域的底線；若末格自身未明確宣告 bottom，繼承 head 的 bottom；
///      若末格自身未宣告 left/right，沿用 head 的 left/right；
///    - mid（中間格）：內部橫線不畫（bottom 為 None），但 left/right 沿用 head 的 left/right；
///    - head（起始格）：若有 continuation cells，head 自身的 bottom 清除，由末格承接。
fn inherit_vmerge_borders(table: &mut Table) {
    if std::env::var_os("D6_BORDERS_BUGGY").is_some() {
        return; // 回塞鎖：退回修前無繼承狀態
    }
    let n_rows = table.rows.len();
    if n_rows <= 1 {
        return;
    }

    let mut row_spans: Vec<Vec<(usize, usize)>> = Vec::with_capacity(n_rows);
    for r in &table.rows {
        let mut spans = Vec::with_capacity(r.cells.len());
        let mut col = 0usize;
        for c in &r.cells {
            let span = c.props.grid_span() as usize;
            spans.push((col, col + span));
            col += span;
        }
        row_spans.push(spans);
    }

    for r in 0..n_rows {
        for c in 0..table.rows[r].cells.len() {
            let restart = table.rows[r].cells[c].props.vmerge_restart();
            if restart != Some(true) {
                continue;
            }
            let (col_start, col_end) = row_spans[r][c];
            let head_borders = table.rows[r].cells[c].props.borders.clone();

            let mut chain: Vec<(usize, usize)> = Vec::new();
            let mut cur_r = r + 1;
            while cur_r < n_rows {
                let cont_idx = row_spans[cur_r]
                    .iter()
                    .position(|&(cs, ce)| cs == col_start && ce == col_end);
                let Some(ci) = cont_idx else { break };
                if table.rows[cur_r].cells[ci].props.vmerge_restart() == Some(false) {
                    chain.push((cur_r, ci));
                    cur_r += 1;
                } else {
                    break;
                }
            }

            if chain.is_empty() {
                continue;
            }

            let Some(head_b) = head_borders else {
                continue;
            };

            let tail_idx = chain.len() - 1;
            for (i, &(cr, ci)) in chain.iter().enumerate() {
                let is_tail = i == tail_idx;
                let cell = &mut table.rows[cr].cells[ci];
                let b = cell.props.borders.get_or_insert_with(Borders::default);

                if b.left.is_none() {
                    b.left = head_b.left.clone();
                }
                if b.right.is_none() {
                    b.right = head_b.right.clone();
                }

                if is_tail {
                    if b.bottom.is_none() {
                        b.bottom = head_b.bottom.clone();
                    }
                } else {
                    b.bottom = None;
                }
            }

            if let Some(hb) = table.rows[r].cells[c].props.borders.as_mut() {
                hb.bottom = None;
            }
        }
    }
}

fn apply_border(tb: &mut Borders, pos: &[u8], b: Border) {
    match pos {
        b"top" => tb.top = Some(b),
        b"left" | b"start" => tb.left = Some(b),
        b"bottom" => tb.bottom = Some(b),
        b"right" | b"end" => tb.right = Some(b),
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
        theme_fill_tint: attr(e, b"themeFillTint"),
        theme_fill_shade: attr(e, b"themeFillShade"),
    }
}

fn parse_f64(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key {
            crate::qx::decode_and_unescape_attr(&a)
        } else {
            None
        }
    })
}

/// 布林屬性取值（ST_OnOff）：`1`/`true`/`on` → Some(true)、`0`/`false`/`off`
/// → Some(false)、缺／非法 → None（呼叫端走預設，不猜）。
fn attr_bool(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<bool> {
    match attr(e, key)?.trim() {
        "1" | "true" | "on" => Some(true),
        "0" | "false" | "off" => Some(false),
        _ => None,
    }
}

/// 事件流單趟餵入（read_body_tables／count_flat_paras 共用，口徑必然一致）。
fn run_parser(xml: &[u8]) -> TableParser {
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
    tp
}

/// 扁平段落計數（與 [`crate::parse_document_xml`] 同口徑的獨立狀態機：
/// 非自閉合 `w:p` Start 佔槽、`mc:Fallback` 內不計）。僅供交叉驗證柵欄
/// ——兩實作分岔即測試紅燈（口徑契約的物理防呆）。
#[cfg(test)]
pub(crate) fn count_flat_paras(xml: &[u8]) -> usize {
    run_parser(xml).p_count
}

/// 由 `word/document.xml` 明文解析全部 body 級表格（含巢狀投影）。
///
/// 純函數、只讀；段落計數與主解析器同口徑（見模組頭）。XML 尾部截斷或
/// 標籤錯置不 panic——未閉合結構老實丟棄（明文仍在 parts，零失真）。
pub(crate) fn read_body_tables(xml: &[u8]) -> Vec<BodyTable> {
    run_parser(xml).roots
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

    /// 段落計數口徑（t_580c37ac 統一後）：AlternateContent 內 Choice 路徑的
    /// `w:p` 照計（主解析器解析 Choice 一份），`mc:Fallback` 內不計（雙份去重）；
    /// 兩者的表格都不進 canonical（Skip 幀吞沒）。
    #[test]
    fn alternate_content_skipped_but_counted() {
        let xml = "<w:document xmlns:w=\"x\" xmlns:mc=\"m\"><w:body>\n            <w:p><w:r><w:t>前</w:t></w:r></w:p>\n            <mc:AlternateContent><mc:Choice Requires=\"wps\">\n              <w:tbl><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr><w:tc><w:p><w:t>MC</w:t></w:p></w:tc></w:tr></w:tbl>\n            </mc:Choice><mc:Fallback>\n              <w:p><w:r><w:t>MC</w:t></w:r></w:p>\n            </mc:Fallback></mc:AlternateContent>\n            <w:tbl><w:tblGrid><w:gridCol w:w=\"200\"/></w:tblGrid></w:tbl>\n            </w:body></w:document>"
            .as_bytes();
        let tables = read_body_tables(xml);
        assert_eq!(tables.len(), 1, "MC 表略過，只見真表");
        assert_eq!(
            tables[0].para_start, 2,
            "Choice 內 w:p 計入扁平口徑（前1＋Choice內1）；Fallback 內同文不重複計"
        );
        assert_eq!(tables[0].table.grid_col_count(), 1);
        assert_eq!(tables[0].table.grid[0].w_pt, Some(10.0));
    }

    /// 巢狀 `w:p`（文字框）錨點口徑：表格解析器的 `para_start`／
    /// `Block::Paragraph{index}` 必須與主解析器扁平 Start 序一致——
    /// 儲存格內段落先於其文字框段落佔位，計數器不得分岔
    /// （engine-api table_map 因此不再需要「可見段落最小索引」重錨）。
    #[test]
    fn nested_txbx_para_start_matches_main_parser() {
        let xml = "<w:document xmlns:w=\"x\" xmlns:mc=\"m\"><w:body>\n            <w:p><w:r><w:t>表前</w:t></w:r></w:p>\n            <w:tbl><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr><w:tc>\n              <w:p><w:r><w:t>格內前</w:t></w:r>\n                <w:r><mc:AlternateContent><mc:Choice Requires=\"wps\">\n                  <w:txbxContent><w:p><w:r><w:t>框內</w:t></w:r></w:p></w:txbxContent>\n                </mc:Choice><mc:Fallback>\n                  <w:p><w:r><w:t>框內</w:t></w:r></w:p>\n                </mc:Fallback></mc:AlternateContent></w:r>\n              <w:r><w:t>格內後</w:t></w:r></w:p>\n            </w:tc></w:tr></w:tbl>\n            </w:body></w:document>"
            .as_bytes();
        // 主解析器同口徑交叉驗證（Start 佔槽＋Fallback 吞沒＋K4e2 框內段
        // 不佔槽）：表前段、格內段＝2（框內段掛宿主 txbx_texts）。
        assert_eq!(
            crate::parse_document_xml(xml, &crate::ThemeColors::default())
                .expect("主解析")
                .len(),
            2
        );
        assert_eq!(
            crate::table::count_flat_paras(xml),
            2,
            "表格計數器與主解析器分岔"
        );
        let tables = read_body_tables(xml);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].para_start, 1, "表前段 1 個 → 錨點＝1");
        let blocks = &tables[0].table.rows[0].cells[0].blocks;
        let idx: Vec<usize> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph { index } => Some(*index),
                _ => None,
            })
            .collect();
        // 格內段 index＝1（Start 序無漂移）；框內段在 Skip 幀內不掛
        // canonical block，且 K4e2 起連扁平計數也不入（掛宿主 txbx_texts）。
        assert_eq!(idx, vec![1], "格內段 index 直指主解析器扁平段落 1");
    }

    /// corpus 閘門（PM 補正 2）：manifest 標 table 的全部檔案，解析不 panic，
    /// 且解析出的 column 定義數與「可投影」的 `w:gridCol` 出現數一致——
    /// 不可投影 = AlternateContent 子樹（不進 canonical）或段落/VML 圖形
    /// 容器（`w:pict`/`w:drawing` 的 `w:txbxContent`：Frame::P 包圍幀整棵吞沒，
    /// 同為不進 canonical 之類；口徑見 count_tag_unprojected）。
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
                                                 // 全域對照：gridCol 出現數（口徑見 count_tag_unprojected）。
            let expect = count_tag_unprojected(&xml, b"<w:gridCol");
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
    /// 排除「不進 canonical」的子樹：`mc:AlternateContent`（Skip 幀吞沒）、
    /// `w:pict`／`w:drawing`／`w:txbxContent`（VML/圖形文字框——其內容的
    /// `w:tbl` 因 tbl_parent 遇 Frame::P 壁壘判 Invalid，整棵不投影）。
    /// 口徑必須與解析器的投影邊界一致，否則柵欄會把「解析器故意不看」
    /// 誤報成漏解析（270 檔擴充後首發：3 檔表格藏於 VML text box）。
    fn count_tag_unprojected(xml: &[u8], tag: &[u8]) -> usize {
        let mut spans = alt_spans(xml);
        spans.extend(tagged_spans(xml, b"<w:pict", b"</w:pict>"));
        spans.extend(tagged_spans(xml, b"<w:drawing", b"</w:drawing>"));
        spans.extend(tagged_spans(xml, b"<w:txbxContent", b"</w:txbxContent>"));
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

    /// 泛化版最外層區間集合：`open`／`close` 如 `<w:pict`／`</w:pict>`。
    fn tagged_spans(xml: &[u8], open: &[u8], close: &[u8]) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        let mut i = 0usize;
        loop {
            match (find_bytes(xml, open, i), find_bytes(xml, close, i)) {
                (Some(o), Some(c)) if o < c => {
                    stack.push(o);
                    i = o + open.len();
                }
                (_, Some(c)) => {
                    if stack.len() == 1 {
                        if let Some(s) = stack.pop() {
                            spans.push((s, c + close.len()));
                        }
                    } else {
                        stack.pop();
                    }
                    i = c + close.len();
                }
                _ => break,
            }
        }
        if let Some(s) = stack.first() {
            spans.push((*s, xml.len()));
        }
        spans
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
