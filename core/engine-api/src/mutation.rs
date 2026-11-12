//! 編輯 API Mutation 型錄、Surgical XML Patch 與 Undo/Redo 實作（K29a / Slice 1）。
//!
//! 依據 `docs/edit-api-design.md` §2、§3 與 §5 規範實作：
//! - 正文文字編輯鎖定正準碼點偏移（cp: usize），不含清單 marker 或點引合成字元。
//! - 外科手術式 XML 修補（Surgical XML Patch）僅替換受影響節點的位元組切片，
//!   未觸碰段落、表格、未知屬性（w:rsidR、w14:paraId、mc:AlternateContent）原樣保留。
//! - 支援精確逆操作（InverseMutation）與 touched 自動退場機制。

use crate::Engine;
use quire_layout::Invalidation;
use quire_ooxml::{NoteKind2, NoteNumberFormat, NoteRestart};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 文件子樹目標（決定操作哪一個 XML part 的段落／表格空間）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubDocTarget {
    /// 正文：對應 `"word/document.xml"` 的 `<w:body>`
    Body,
    /// 頁首：對應第 `section_id` 節之 `slot`（Default/First/Even）所指向的 `"word/header*.xml"`
    Header { section_id: u32, slot: HfSlotKind },
    /// 頁尾：對應第 `section_id` 節之 `slot`（Default/First/Even）所指向的 `"word/footer*.xml"`
    Footer { section_id: u32, slot: HfSlotKind },
    /// 註腳本體：對應 `"word/footnotes.xml"` 中 `@w:id == note_id` 的 `<w:footnote>`
    Footnote { note_id: i32 },
    /// 尾註本體：對應 `"word/endnotes.xml"` 中 `@w:id == note_id` 的 `<w:endnote>`
    Endnote { note_id: i32 },
    /// 圖形／VML 文字框內部：對應宿主段落 `host_para` 第 `box_index` 個 `<w:txbxContent>`
    TextBox { host_para: usize, box_index: usize },
}

/// 頁首頁尾槽位種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HfSlotKind {
    Default,
    First,
    Even,
}

/// 段內正準游標位置（以 char 碼點計）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocPos {
    pub para: usize,
    pub cp: usize,
}

/// 半開選取範圍（保證 start <= end）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocRange {
    pub target: SubDocTarget,
    pub start: DocPos,
    pub end: DocPos,
}

/// 斷點種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakKind {
    /// 軟換行 `<w:br/>`
    LineWrapping,
    /// 硬分頁 `<w:br w:type="page"/>`
    Page,
    /// 分欄斷點 `<w:br w:type="column"/>`
    Column,
    /// 製表符 `<w:tab/>`
    Tab,
}

/// 字元級直接格式覆寫（K29b／M2.1）：`None`＝保持原宣告，`Some`＝明確設定。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunDirectPatch {
    /// 粗體（`w:b`；`Some(false)` 寫出 `<w:b w:val="0"/>`）。
    #[serde(default)]
    pub bold: Option<bool>,
    /// 斜體（`w:i`）。
    #[serde(default)]
    pub italic: Option<bool>,
    /// 字色（`0xAARRGGBB`；寫出 `w:color/@w:val` 的 RRGGBB）。
    #[serde(default)]
    pub color_rgb: Option<u32>,
    /// 字級（pt；寫出 `w:sz/@w:val` 的半磅值）。
    #[serde(default)]
    pub size_pt: Option<f32>,
    /// 字元樣式 id（`w:rStyle/@w:val`）。
    #[serde(default)]
    pub char_style: Option<String>,
}

/// 段落級屬性覆寫（K29b／M2.2）：`None`＝保持原宣告，`Some`＝明確設定。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParaPropsPatch {
    /// 段落樣式 id（`w:pStyle/@w:val`）；須為段落型樣式。
    #[serde(default)]
    pub style_id: Option<String>,
    /// 段前強制分頁（`w:pageBreakBefore`）。
    #[serde(default)]
    pub page_break_before: Option<bool>,
    /// 行距倍數（`w:spacing/@w:line`，240＝單倍）。
    #[serde(default)]
    pub line_spacing: Option<f32>,
    /// 對齊（`w:jc/@w:val`：left/center/right/both）。
    #[serde(default)]
    pub alignment: Option<String>,
}

/// 樣式型別（`w:style/@w:type`，ECMA-376 §17.7.2.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StyleType {
    Paragraph,
    Character,
}

impl StyleType {
    fn as_w_val(self) -> &'static str {
        match self {
            StyleType::Paragraph => "paragraph",
            StyleType::Character => "character",
        }
    }
}

/// 清單編號動作（K29b／M3.1）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListAction {
    /// 掛上既有 `numId` 與層級（僅修補 document.xml，不碰 numbering.xml）。
    AttachExisting { num_id: u32, ilvl: u8 },
    /// 調整既有清單層級。
    ChangeLevel { ilvl: u8 },
    /// 移除段落清單編號。
    RemoveNumbering,
}

/// 列／欄插入方位（K29c／M4.3、M4.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertSide {
    Before,
    After,
}

/// 表格穩定路徑（K29c／M4）：根表序號＋逐層巢狀步進。
///
/// 每一步為 `(row_idx, cell_idx, sub_table_idx)`，指向該格內第幾個巢狀
/// `<w:tbl>`。正文根表以 `root_slot` 定位（`OoxmlDoc::tables` 的序）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TablePath {
    pub target: SubDocTarget,
    pub root_slot: usize,
    #[serde(default)]
    pub nested_steps: Vec<(usize, usize, usize)>,
}

impl TablePath {
    /// 正文第 `root_slot` 張根表（無巢狀步進）。
    pub fn body(root_slot: usize) -> Self {
        Self {
            target: SubDocTarget::Body,
            root_slot,
            nested_steps: Vec::new(),
        }
    }
}

/// 表格屬性補丁（K29c／M4.6）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TablePropsPatch {
    /// `w:tblW/@w:w`（pt，寫回 dxa twip）。
    #[serde(default)]
    pub width_pt: Option<f32>,
    /// `w:jc/@w:val`（left／center／right）。
    #[serde(default)]
    pub alignment: Option<String>,
    /// `w:shd/@w:fill`（RRGGBB）。
    #[serde(default)]
    pub shading_fill: Option<String>,
}

/// 列屬性補丁（K29c／M4.6）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RowPropsPatch {
    /// `w:trHeight/@w:val`（pt，寫回 twip；`hRule="atLeast"`）。
    #[serde(default)]
    pub height_pt: Option<f32>,
    /// `w:tblHeader`（跨頁重複表頭列）。
    #[serde(default)]
    pub header: Option<bool>,
    /// `w:cantSplit`（整列不拆頁）。
    #[serde(default)]
    pub cant_split: Option<bool>,
}

/// 垂直合併狀態（K29c／M4.5、M4.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VMergeState {
    /// 移除 `<w:vMerge>`。
    None,
    /// `<w:vMerge w:val="restart"/>`。
    Restart,
    /// `<w:vMerge/>`（延續）。
    Continue,
}

/// 儲存格屬性補丁（K29c／M4.6）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CellPropsPatch {
    /// `w:tcW/@w:w`（pt，寫回 dxa twip）。
    #[serde(default)]
    pub width_pt: Option<f32>,
    /// `w:shd/@w:fill`（RRGGBB）。
    #[serde(default)]
    pub shading_fill: Option<String>,
    /// `w:vAlign/@w:val`（top／center／bottom）。
    #[serde(default)]
    pub vertical_align: Option<String>,
    /// `w:gridSpan/@w:val`（1＝移除元素）。
    #[serde(default)]
    pub grid_span: Option<u32>,
    /// `w:vMerge` 狀態。
    #[serde(default)]
    pub v_merge: Option<VMergeState>,
}

/// 浮動圖單軸定位規格（K29c／M5.1、M5.2）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloatingAxisSpec {
    /// `wp:positionH|V/@relativeFrom`（page／margin／column／paragraph／line／character）。
    pub relative_from: String,
    /// `wp:posOffset`（pt；與 `align` 互斥）。
    #[serde(default)]
    pub offset_pt: Option<f32>,
    /// `wp:align`（left／center／right／top／bottom）。
    #[serde(default)]
    pub align: Option<String>,
}

/// 文繞圖模式（K29c／M5.1、M5.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WrapModeSpec {
    None,
    Square,
    Tight,
    Through,
    TopAndBottom,
}

/// Mutation 操作枚舉。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Mutation {
    /// M1.1 游標處插入文字
    InsertText {
        target: SubDocTarget,
        pos: DocPos,
        text: String,
        style_override: Option<RunDirectPatch>,
    },
    /// M1.2 單段或跨段範圍刪除（K29b 支援跨段同容器刪除）
    DeleteRange { range: DocRange },
    /// M1.5 插入斷點
    InsertBreak {
        target: SubDocTarget,
        pos: DocPos,
        kind: BreakKind,
    },
    /// M1.3 在 `pos` 處將段落一分為二（後半移入 `pos.para + 1`）。
    SplitParagraph { target: SubDocTarget, pos: DocPos },
    /// M1.4 將 `para + 1` 段接續到 `para` 段尾並刪除次段。
    MergeParagraphWithNext { target: SubDocTarget, para: usize },
    /// M2.1 對選取範圍套用字元級直接格式。
    FormatTextRange {
        range: DocRange,
        patch: RunDirectPatch,
    },
    /// M2.2 對一或多個段落套用段落級屬性。
    SetParagraphStyle {
        target: SubDocTarget,
        para_range: std::ops::RangeInclusive<usize>,
        patch: ParaPropsPatch,
    },
    /// M2.3 修改或新增 `word/styles.xml` 中的具名樣式。
    UpsertNamedStyle {
        style_id: String,
        style_type: StyleType,
        based_on: Option<String>,
        para_patch: Option<ParaPropsPatch>,
        run_patch: Option<RunDirectPatch>,
    },
    /// M3.1 設定或移除段落清單編號。
    SetListNumbering {
        target: SubDocTarget,
        para_range: std::ops::RangeInclusive<usize>,
        action: ListAction,
    },
    /// M4.1 在指定段落之後插入 `rows × cols` 新表格。
    InsertTable {
        target: SubDocTarget,
        after_para: Option<usize>,
        rows: usize,
        cols: usize,
        col_widths_pt: Option<Vec<f32>>,
    },
    /// M4.2 刪除整張表格。
    DeleteTable { table: TablePath },
    /// M4.3 插入一列（Before／After）。
    InsertTableRow {
        table: TablePath,
        at_row: usize,
        position: InsertSide,
    },
    /// M4.3 刪除指定列（連帶處理 vMerge 頂格晉升）。
    DeleteTableRow { table: TablePath, row: usize },
    /// M4.4 插入一欄（同步修補 tblGrid 與各列）。
    InsertTableColumn {
        table: TablePath,
        at_grid_col: usize,
        position: InsertSide,
        width_pt: f32,
    },
    /// M4.4 刪除指定網格欄。
    DeleteTableColumn { table: TablePath, grid_col: usize },
    /// M4.5 合併矩形儲存格區域。
    MergeTableCells {
        table: TablePath,
        top_row: usize,
        bottom_row: usize,
        left_grid_col: usize,
        right_grid_col: usize,
    },
    /// M4.5 分割已合併儲存格。
    SplitTableCell {
        table: TablePath,
        row: usize,
        cell: usize,
        into_rows: usize,
        into_cols: usize,
    },
    /// M4.6 修改表格屬性。
    SetTableProps {
        table: TablePath,
        patch: TablePropsPatch,
    },
    /// M4.6 修改列屬性。
    SetRowProps {
        table: TablePath,
        row: usize,
        patch: RowPropsPatch,
    },
    /// M4.6 修改儲存格屬性。
    SetCellProps {
        table: TablePath,
        row: usize,
        cell: usize,
        patch: CellPropsPatch,
    },
    /// M5.1 在游標位置插入行內圖片。
    InsertInlineImage {
        target: SubDocTarget,
        pos: DocPos,
        media_bytes: Vec<u8>,
        mime_type: String,
        width_pt: f32,
        height_pt: f32,
        alt_text: Option<String>,
    },
    /// M5.1 在宿主段落插入浮動圖片。
    InsertFloatingImage {
        target: SubDocTarget,
        host_para: usize,
        media_bytes: Vec<u8>,
        mime_type: String,
        width_pt: f32,
        height_pt: f32,
        pos_h: FloatingAxisSpec,
        pos_v: FloatingAxisSpec,
        wrap_mode: WrapModeSpec,
        behind_doc: bool,
    },
    /// M5.2 調整既有圖片尺寸或浮動定位（零媒體觸碰）。
    ResizeOrMoveImage {
        target: SubDocTarget,
        host_para: usize,
        drawing_index: usize,
        new_width_pt: Option<f32>,
        new_height_pt: Option<f32>,
        new_pos_h: Option<FloatingAxisSpec>,
        new_pos_v: Option<FloatingAxisSpec>,
        new_wrap_mode: Option<WrapModeSpec>,
    },
    /// M5.3 替換圖片媒體（預設 CoW；強制 GC 時不可逆）。
    ReplaceImageMedia {
        target: SubDocTarget,
        host_para: usize,
        drawing_index: usize,
        new_media_bytes: Vec<u8>,
        new_mime_type: String,
    },
    /// M5.3 刪除圖片（可選強制回收未引用媒體）。
    DeleteImage {
        target: SubDocTarget,
        host_para: usize,
        drawing_index: usize,
        gc_unreferenced_media: bool,
    },
    /// M6.1 設定節屬性（分欄／邊距／紙張方向／節起點型別；設計書 §2.3.6）。
    SetSectionProperties {
        section_id: u32,
        patch: SectionPropsPatch,
    },
    /// M6.2 在 `para` 段之前插入分節符（段內 `<w:pPr><w:sectPr>`，ECMA-376 §17.2.3）。
    InsertSectionBreak {
        para: usize,
        kind: SectionStartKind,
    },
    /// M6.2 刪除第 `section_id` 節的分節符（根級 body 尾 sectPr 誠實拒絕）。
    DeleteSectionBreak { section_id: u32 },
    /// M6.3 頁首頁尾 slot 三態設定（titlePg／奇偶頁開關／綁定動作；設計書 §2.2.5）。
    ConfigureHeaderFooterSlot {
        section_id: u32,
        title_pg: Option<bool>,
        even_and_odd_headers: Option<bool>,
        slot_bind: Option<(HfKind, HfSlotKind, HfBindAction)>,
    },
    /// M6.4 插入註腳／尾註（正文錨點＋註體，一次交易）。
    InsertNote {
        kind: NoteKind2,
        pos: DocPos,
        initial_text: String,
    },
    /// M6.4 刪除註腳／尾註（正文錨點與註體同拆；系統分隔線註誠實拒絕）。
    DeleteNote { kind: NoteKind2, note_id: i32 },
    /// M6.4 設定註腳／尾註編號（設定落 `word/settings.xml`，設計書 §2.3.7）。
    SetNoteProperties {
        kind: NoteKind2,
        patch: NotePropsPatch,
    },
}

/// 紙張方向（M6.1；pgSz 的 w/h 是絕對尺寸，方向只是宣告語意）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

/// 節起點型別（`w:type/@w:val`，ECMA-376 §17.18.19）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SectionStartKind {
    NextPage,
    Continuous,
    EvenPage,
    OddPage,
}

/// 分欄設定（M6.1；§2.2.5 平分模式：欄寬＝(版心−(n−1)×space)/n）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionColsPatch {
    /// 欄數（`w:cols/@w:num`；1＝單欄）。
    pub num: u32,
    /// 欄距（pt；`w:cols/@w:space`；None＝沿用既有宣告）。
    #[serde(default)]
    pub space_pt: Option<f32>,
}

/// 節屬性覆寫（M6.1）：`None`＝保持原宣告，`Some`＝明確設定。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SectionPropsPatch {
    /// 分欄（含欄數與欄距）。
    #[serde(default)]
    pub cols: Option<SectionColsPatch>,
    /// 上邊距（pt；`w:pgMar/@w:top`）。
    #[serde(default)]
    pub margin_top_pt: Option<f32>,
    /// 下邊距（pt）。
    #[serde(default)]
    pub margin_bottom_pt: Option<f32>,
    /// 左邊距（pt）。
    #[serde(default)]
    pub margin_left_pt: Option<f32>,
    /// 右邊距（pt）。
    #[serde(default)]
    pub margin_right_pt: Option<f32>,
    /// 頁首距（pt；`w:pgMar/@w:header`）。
    #[serde(default)]
    pub margin_header_pt: Option<f32>,
    /// 頁尾距（pt；`w:pgMar/@w:footer`）。
    #[serde(default)]
    pub margin_footer_pt: Option<f32>,
    /// 紙張方向。
    #[serde(default)]
    pub orientation: Option<PageOrientation>,
    /// 節起點型別。
    #[serde(default)]
    pub section_type: Option<SectionStartKind>,
}

impl SectionPropsPatch {
    /// 全空 patch（拒絕 no-op 用）。
    pub fn is_empty(&self) -> bool {
        self.cols.is_none()
            && self.margin_top_pt.is_none()
            && self.margin_bottom_pt.is_none()
            && self.margin_left_pt.is_none()
            && self.margin_right_pt.is_none()
            && self.margin_header_pt.is_none()
            && self.margin_footer_pt.is_none()
            && self.orientation.is_none()
            && self.section_type.is_none()
    }
}

/// 頁首／頁尾族（M6.3 slot 綁定用；slot 細分見 [`HfSlotKind`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HfKind {
    Header,
    Footer,
}

/// slot 綁定動作（M6.3；Unbind 只拆 sectPr 引用，part 本體與註冊留存）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HfBindAction {
    /// 新建一個空 part 並綁定到該 slot（才允許觸碰 rels 與 [Content_Types].xml）。
    Create,
    /// 解除該 slot 的綁定。
    Unbind,
}

/// 註腳／尾註編號與位置設定（M6.4；§2.3.7 落 `word/settings.xml`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotePropsPatch {
    /// 編號格式（`w:numFmt/@w:val`）。
    #[serde(default)]
    pub num_fmt: Option<NoteNumberFormat>,
    /// 重啟時機（`w:numRestart/@w:val`）。
    #[serde(default)]
    pub num_restart: Option<NoteRestart>,
    /// 起始編號（`w:numStart/@w:val`）。
    #[serde(default)]
    pub num_start: Option<i32>,
    /// 位置（`w:pos/@w:val`；本片未接線，留位）。
    #[serde(default)]
    pub pos: Option<quire_ooxml::NotePos>,
}

impl NotePropsPatch {
    /// 全空 patch（拒絕 no-op 用）。
    pub fn is_empty(&self) -> bool {
        self.num_fmt.is_none() && self.num_restart.is_none() && self.num_start.is_none() && self.pos.is_none()
    }
}

/// 半開位元組區間。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

/// 局部 XML 切片逆修補（保證未知屬性與未動位元組 100% 還原）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XmlReversePatch {
    pub part_name: String,
    /// 正向修補寫入後的半開位元組區間（Undo 時將此區間替換回 original_slice）
    pub applied_span: ByteSpan,
    /// 正向修補前被覆蓋之原始 XML 局部切片
    pub original_slice: Vec<u8>,
}

/// 語義層逆向操作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SemanticInverseOp {
    Mutation(Mutation),
}

/// 容器層新增或移除之臨時媒體／附屬 part 逆動作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartLifecycleInverse {
    RemoveSynthesizedPart(String),
    RestoreRemovedPart { name: String, bytes: Vec<u8> },
}

/// 單筆 Mutation 的精確逆操作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InverseMutation {
    pub semantic_inverse: SemanticInverseOp,
    pub xml_patches: Vec<XmlReversePatch>,
    pub part_lifecycle_inverse: Vec<PartLifecycleInverse>,
}

/// 不可逆原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IrreversibleReason {
    MediaPoolExceeded,
    /// 本會話新增之媒體被強制回收（K29c／M5.3）。
    MediaPayloadEvicted {
        part_name: String,
        bytes_freed: usize,
    },
    ExternalSetPart,
    Custom(String),
}

/// 操作可逆性等級。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReversibilityLevel {
    StrictByteReversible,
    Irreversible { reason: IrreversibleReason },
}

/// 交易級 Undo 堆疊項目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UndoEntry {
    pub tx_id: u64,
    pub inverse_ops: Vec<InverseMutation>,
    pub caret_before: Option<DocPos>,
    pub caret_after: Option<DocPos>,
    pub reversibility: ReversibilityLevel,
}

/// 反序列化預設：未帶可逆欄位的舊快照一律視為嚴格可逆。
fn default_strict_reversible() -> ReversibilityLevel {
    ReversibilityLevel::StrictByteReversible
}

/// Mutation 套用結果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationOutcome {
    pub events: Vec<Invalidation>,
    pub touched_parts: Vec<String>,
    pub inverse: Vec<InverseMutation>,
    pub caret_after: Option<DocPos>,
    /// 本次操作的可逆性宣告（K29c：M5.3 強制 GC 時降級為不可逆）。
    #[serde(default = "default_strict_reversible")]
    pub reversibility: ReversibilityLevel,
}

/// 誠實拒絕不可應用操作的錯誤型別。
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum MutationError {
    #[error("目標子樹不存在或未掛載: {target:?}")]
    SubDocNotFound { target: SubDocTarget },
    #[error("段落索引越界: 請求 {para}，總段數 {len}")]
    ParagraphOutOfBounds { para: usize, len: usize },
    #[error("碼點偏移越界或區間倒置: 段 {para} 長度 {para_len}，請求 ({start_cp}..{end_cp})")]
    CodepointOutOfBounds {
        para: usize,
        start_cp: usize,
        end_cp: usize,
        para_len: usize,
    },
    #[error("跨容器邊界非法: 起點與終點不位於同一儲存格或同一文字框子樹")]
    CrossContainerBoundary { start_para: usize, end_para: usize },
    #[error("受保護的結構節點不可刪除或拆裂: {reason}")]
    ProtectedStructure { reason: String },
    #[error("樣式引用非法或型別不符: style_id={style_id}，期望型別={expected_type}")]
    InvalidStyleReference {
        style_id: String,
        expected_type: String,
    },
    #[error("清單編號引用不存在: num_id={num_id}, ilvl={ilvl}")]
    InvalidNumberingReference { num_id: u32, ilvl: u8 },
    #[error("表格結構座標越界或合併區間非法: {detail}")]
    InvalidTableOperation { detail: String },
    #[error("圖片或媒體格式不支援或超過容量上限: {detail}")]
    InvalidMediaPayload { detail: String },
    #[error("XML 語法樹定位失敗或位於唯讀修訂/替換子樹: {detail}")]
    XmlPatchRejected { detail: String },
    #[error("引擎錯誤: {0}")]
    Engine(String),
}

/// 內部 XML 手術補丁。
#[derive(Debug, Clone)]
pub struct XmlBytePatch {
    pub span: ByteSpan,
    pub replacement: Vec<u8>,
}

/// 判斷字元是否為 XML 空白字元。
pub fn is_xml_space(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\r' || c == '\n'
}

/// XML 跳脫字元處理。
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// XML 反跳脫字元處理。
pub fn xml_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            let mut entity = String::new();
            for ec in chars.by_ref() {
                if ec == ';' {
                    break;
                }
                entity.push(ec);
            }
            match entity.as_str() {
                "amp" => out.push('&'),
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "quot" => out.push('"'),
                "apos" => out.push('\''),
                _ => {
                    out.push('&');
                    out.push_str(&entity);
                    out.push(';');
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 段落節點在 XML 中的位元組區間。
#[derive(Debug, Clone, Copy)]
pub struct ParagraphSpan {
    pub start: usize,
    pub end: usize,
}

/// 依據解析器口徑掃描並定位指定索引的扁平段落位元組區間。
pub fn find_paragraph_span(
    xml: &[u8],
    target_para: usize,
) -> Result<(ParagraphSpan, usize), MutationError> {
    let text = match std::str::from_utf8(xml) {
        Ok(t) => t,
        Err(_) => {
            return Err(MutationError::XmlPatchRejected {
                detail: "XML 明文非合法 UTF-8".to_string(),
            })
        }
    };

    let mut cursor = 0;
    let mut fb_depth = 0usize;
    let mut ac_stack = Vec::new();
    let mut txbx_depth = 0usize;
    let mut para_index = 0usize;
    let n = text.len();

    let mut target_span: Option<ParagraphSpan> = None;

    while cursor < n {
        let lt = match text[cursor..].find('<') {
            Some(i) => cursor + i,
            None => break,
        };
        let gt = match text[lt..].find('>') {
            Some(i) => lt + i,
            None => break,
        };
        let tag = &text[lt..=gt];
        cursor = gt + 1;

        let is_close = tag.starts_with("</");
        let is_empty = tag.ends_with("/>");
        let inner = if is_close {
            &tag[2..tag.len() - 1]
        } else if is_empty {
            &tag[1..tag.len() - 2]
        } else {
            &tag[1..tag.len() - 1]
        };
        let name = inner.split_whitespace().next().unwrap_or("");
        let local = name.rsplit(':').next().unwrap_or("");

        if !is_close {
            if local == "AlternateContent" {
                ac_stack.push(fb_depth);
            } else if local == "Fallback" {
                fb_depth += 1;
            }
        } else if local == "Fallback" {
            fb_depth = fb_depth.saturating_sub(1);
        } else if local == "AlternateContent" {
            if let Some(prev) = ac_stack.pop() {
                fb_depth = prev;
            }
        }

        if fb_depth > 0 {
            continue;
        }

        if !is_close {
            if local == "txbxContent" {
                txbx_depth += 1;
            }
        } else if local == "txbxContent" {
            txbx_depth = txbx_depth.saturating_sub(1);
        }

        if txbx_depth > 0 {
            continue;
        }

        if !is_close && !is_empty && local == "p" {
            let p_start = lt;
            let mut p_depth = 1usize;
            let mut search_cur = gt + 1;
            let mut p_end = None;

            while search_cur < n {
                let next_lt = match text[search_cur..].find('<') {
                    Some(i) => search_cur + i,
                    None => break,
                };
                let next_gt = match text[next_lt..].find('>') {
                    Some(i) => next_lt + i,
                    None => break,
                };
                let t2 = &text[next_lt..=next_gt];
                search_cur = next_gt + 1;

                let ic = t2.starts_with("</");
                let ie = t2.ends_with("/>");
                let in2 = if ic {
                    &t2[2..t2.len() - 1]
                } else if ie {
                    &t2[1..t2.len() - 2]
                } else {
                    &t2[1..t2.len() - 1]
                };
                let nm2 = in2.split_whitespace().next().unwrap_or("");
                let loc2 = nm2.rsplit(':').next().unwrap_or("");

                if loc2 == "p" {
                    if !ic && !ie {
                        p_depth += 1;
                    } else if ic {
                        p_depth = p_depth.saturating_sub(1);
                        if p_depth == 0 {
                            p_end = Some(next_gt + 1);
                            cursor = next_gt + 1;
                            break;
                        }
                    }
                }
            }

            if let Some(end) = p_end {
                if para_index == target_para {
                    target_span = Some(ParagraphSpan {
                        start: p_start,
                        end,
                    });
                }
                para_index += 1;
            }
        }
    }

    if let Some(span) = target_span {
        Ok((span, para_index))
    } else {
        Err(MutationError::ParagraphOutOfBounds {
            para: target_para,
            len: para_index,
        })
    }
}

/// 段落內 `<w:t>` 節點定位資訊。
#[derive(Debug, Clone)]
pub struct TextNodeInfo {
    /// `<w:t ...>...</w:t>` 整個標籤的半開位元組區間
    pub node_span: ByteSpan,
    /// 內容文字的半開位元組區間
    pub text_span: ByteSpan,
    /// 是否帶有 xml:space="preserve"
    pub preserve_space: bool,
    /// 反跳脫後的純文字
    pub unescaped: String,
    /// 碼點在段落中的起點（含）
    pub start_cp: usize,
    /// 碼點在段落中的終點（不含）
    pub end_cp: usize,
    /// 所屬 run 的 `<w:r>` 起訖位元組區間
    pub run_span: Option<ByteSpan>,
}

/// 段落內部結構分析結果。
#[derive(Debug, Clone)]
pub struct ParagraphAnalysis {
    pub ppr_end_offset: Option<usize>,
    pub p_open_end_offset: usize,
    pub p_close_start_offset: usize,
    pub text_nodes: Vec<TextNodeInfo>,
    pub total_cp: usize,
}

/// 解析單一段落 XML 內部的文字節點、換行與斷點結構。
pub fn analyze_paragraph(xml: &[u8], p_span: ParagraphSpan) -> ParagraphAnalysis {
    let p_slice = &xml[p_span.start..p_span.end];
    let text = std::str::from_utf8(p_slice).unwrap_or("");

    let first_gt = text.find('>').unwrap_or(0);
    let p_open_end = p_span.start + first_gt + 1;
    let p_close_start = p_span.start + text.rfind("</").unwrap_or(text.len());

    let mut ppr_end_offset = None;
    let mut text_nodes = Vec::new();
    let mut current_cp = 0usize;

    let mut cursor = 0;
    let mut current_run_span: Option<ByteSpan> = None;

    while cursor < text.len() {
        let lt = match text[cursor..].find('<') {
            Some(i) => cursor + i,
            None => break,
        };
        let gt = match text[lt..].find('>') {
            Some(i) => lt + i,
            None => break,
        };
        let tag = &text[lt..=gt];
        cursor = gt + 1;

        let is_close = tag.starts_with("</");
        let is_empty = tag.ends_with("/>");
        let inner = if is_close {
            &tag[2..tag.len() - 1]
        } else if is_empty {
            &tag[1..tag.len() - 2]
        } else {
            &tag[1..tag.len() - 1]
        };
        let name = inner.split_whitespace().next().unwrap_or("");
        let local = name.rsplit(':').next().unwrap_or("");

        if local == "pPr" && (is_close || is_empty) {
            ppr_end_offset = Some(p_span.start + gt + 1);
        } else if local == "r" && !is_close && !is_empty {
            // 尋找對應的 </w:r>
            let r_start = p_span.start + lt;
            if let Some(r_close_rel) = text[cursor..].find("</") {
                let r_close_lt = cursor + r_close_rel;
                if let Some(r_close_gt_rel) = text[r_close_lt..].find('>') {
                    let r_end = p_span.start + r_close_lt + r_close_gt_rel + 1;
                    current_run_span = Some(ByteSpan {
                        start: r_start,
                        end: r_end,
                    });
                }
            }
        } else if local == "r" && is_close {
            current_run_span = None;
        } else if local == "br" && is_empty {
            let has_page_type = inner.contains(r#"type="page""#)
                || inner.contains(r#"type='page'"#)
                || inner.contains(r#"w:type="page""#);
            let has_col_type = inner.contains(r#"type="column""#)
                || inner.contains(r#"type='column'"#)
                || inner.contains(r#"w:type="column""#);
            if !has_page_type && !has_col_type {
                // 軟換行算 1 個碼點 (\n)
                current_cp += 1;
            }
        } else if local == "t" && !is_close && !is_empty {
            let preserve = inner.contains(r#"xml:space="preserve""#)
                || inner.contains(r#"xml:space='preserve'"#)
                || inner.contains(r#"space="preserve""#);

            // 尋找 </w:t>
            let t_open_end = lt + gt - lt + 1;
            if let Some(t_close_rel) = text[t_open_end..].find("</") {
                let t_close_lt = t_open_end + t_close_rel;
                if let Some(t_close_gt_rel) = text[t_close_lt..].find('>') {
                    let t_close_end = t_close_lt + t_close_gt_rel + 1;
                    let raw_content = &text[t_open_end..t_close_lt];
                    let unescaped_raw = xml_unescape(raw_content);
                    let final_str = if preserve {
                        unescaped_raw
                    } else {
                        unescaped_raw.trim_matches(is_xml_space).to_string()
                    };
                    let char_count = final_str.chars().count();
                    let start_cp = current_cp;
                    let end_cp = current_cp + char_count;
                    current_cp = end_cp;

                    text_nodes.push(TextNodeInfo {
                        node_span: ByteSpan {
                            start: p_span.start + lt,
                            end: p_span.start + t_close_end,
                        },
                        text_span: ByteSpan {
                            start: p_span.start + t_open_end,
                            end: p_span.start + t_close_lt,
                        },
                        preserve_space: preserve,
                        unescaped: final_str,
                        start_cp,
                        end_cp,
                        run_span: current_run_span,
                    });
                    cursor = t_close_end;
                }
            }
        }
    }

    ParagraphAnalysis {
        ppr_end_offset,
        p_open_end_offset: p_open_end,
        p_close_start_offset: p_close_start,
        text_nodes,
        total_cp: current_cp,
    }
}

/// 從段落 XML 切片提取純文字（同步 LayoutEngine 用）。
pub fn extract_para_text_from_xml(xml: &[u8], p_span: ParagraphSpan) -> String {
    let analysis = analyze_paragraph(xml, p_span);
    let mut out = String::new();
    for node in analysis.text_nodes {
        out.push_str(&node.unescaped);
    }
    out
}

// ---------------- K29b：結構拆併與格式修補輔助 ----------------

/// 由 `<w:r>` 開標籤起點解析出**真正**的 run 半開區間。
///
/// `analyze_paragraph` 的 `run_span` 取「下一個 `</`」，在含 `<w:t>` 的 run
/// 會停在 `</w:t>`；K29b 拆段／run 級格式必須用本函式重算到 `</w:r>`。
fn run_span_at(xml: &[u8], start: usize) -> Option<ByteSpan> {
    let text = std::str::from_utf8(xml.get(start..)?).ok()?;
    let first_gt = text.find('>')?;
    let mut cursor = first_gt + 1;
    let mut depth = 1usize;
    while cursor < text.len() {
        let lt = text[cursor..].find('<')? + cursor;
        let gt = text[lt..].find('>')? + lt;
        let tag = &text[lt..=gt];
        let is_close = tag.starts_with("</");
        let is_empty = tag.ends_with("/>");
        let local = tag_local(tag);
        if local == "r" {
            if !is_close && !is_empty {
                depth += 1;
            } else if is_close {
                depth -= 1;
                if depth == 0 {
                    return Some(ByteSpan {
                        start,
                        end: start + gt + 1,
                    });
                }
            }
        }
        cursor = gt + 1;
    }
    None
}

fn tag_local(tag: &str) -> &str {
    let is_close = tag.starts_with("</");
    let is_empty = tag.ends_with("/>");
    let inner = if is_close {
        &tag[2..tag.len().saturating_sub(1)]
    } else if is_empty {
        &tag[1..tag.len().saturating_sub(2)]
    } else {
        &tag[1..tag.len().saturating_sub(1)]
    };
    let name = inner.split_whitespace().next().unwrap_or("");
    name.rsplit(':').next().unwrap_or("")
}

/// 段落所屬容器路徑（`w:tbl`／`w:tr`／`w:tc`／`w:txbxContent` 的開標籤偏移）。
/// 同一路徑＝同一儲存格／文字框子樹；不同路徑即跨容器。
fn paragraph_container_path(xml: &[u8], target_para: usize) -> Result<Vec<usize>, MutationError> {
    // [K29B_BUGGY_POINT_3: CONTAINER_GUARD]
    let buggy_container_guard = false;
    if buggy_container_guard {
        return Ok(Vec::new());
    }
    let text = std::str::from_utf8(xml).map_err(|_| MutationError::XmlPatchRejected {
        detail: "XML 明文非合法 UTF-8".to_string(),
    })?;
    let mut cursor = 0usize;
    let mut fb_depth = 0usize;
    let mut ac_stack: Vec<usize> = Vec::new();
    let mut txbx_depth = 0usize;
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut para_index = 0usize;
    let n = text.len();

    while cursor < n {
        let lt = match text[cursor..].find('<') {
            Some(i) => cursor + i,
            None => break,
        };
        let gt = match text[lt..].find('>') {
            Some(i) => lt + i,
            None => break,
        };
        let tag = &text[lt..=gt];
        cursor = gt + 1;
        let is_close = tag.starts_with("</");
        let is_empty = tag.ends_with("/>");
        let local = tag_local(tag);

        if !is_close {
            if local == "AlternateContent" {
                ac_stack.push(fb_depth);
            } else if local == "Fallback" {
                fb_depth += 1;
            }
        } else if local == "Fallback" {
            fb_depth = fb_depth.saturating_sub(1);
        } else if local == "AlternateContent" {
            if let Some(prev) = ac_stack.pop() {
                fb_depth = prev;
            }
        }

        if fb_depth > 0 {
            continue;
        }

        if !is_close {
            if local == "txbxContent" {
                txbx_depth += 1;
            }
        } else if local == "txbxContent" {
            txbx_depth = txbx_depth.saturating_sub(1);
        }

        if !is_close && !is_empty {
            if matches!(local, "tbl" | "tr" | "tc" | "txbxContent") {
                stack.push((local.to_string(), lt));
            }
        } else if is_close {
            if let Some((top, _)) = stack.last() {
                if top == local {
                    stack.pop();
                }
            }
        }

        if txbx_depth > 0 {
            continue;
        }

        if !is_close && !is_empty && local == "p" {
            if para_index == target_para {
                return Ok(stack.iter().map(|(_, off)| *off).collect());
            }
            para_index += 1;
        }
    }

    Err(MutationError::ParagraphOutOfBounds {
        para: target_para,
        len: para_index,
    })
}

fn leading_element(inner: &str, name: &str) -> String {
    let t = inner.trim_start();
    let open = format!("<{name}");
    if !t.starts_with(&open) {
        return String::new();
    }
    let gt = match t.find('>') {
        Some(g) => g,
        None => return String::new(),
    };
    if t[..gt].trim_end().ends_with('/') {
        return t[..=gt].to_string();
    }
    let close = format!("</{name}>");
    match t.find(&close) {
        Some(end) => t[..end + close.len()].to_string(),
        None => String::new(),
    }
}

fn strip_leading_element(inner: &str, name: &str) -> String {
    let head = leading_element(inner, name);
    if head.is_empty() {
        inner.to_string()
    } else {
        inner[head.len()..].to_string()
    }
}

fn make_text_tag(text: &str, preserve_hint: bool) -> String {
    let needs_preserve =
        preserve_hint || text.starts_with(is_xml_space) || text.ends_with(is_xml_space);
    if needs_preserve {
        format!(r#"<w:t xml:space="preserve">{}</w:t>"#, xml_escape(text))
    } else {
        format!(r#"<w:t>{}</w:t>"#, xml_escape(text))
    }
}

fn strip_sectpr(ppr: &str) -> String {
    let mut out = ppr.to_string();
    while let Some(start) = out.find("<w:sectPr") {
        let Some(gt_rel) = out[start..].find('>') else {
            break;
        };
        let gt = start + gt_rel;
        if out[start..gt].trim_end().ends_with('/') {
            out.replace_range(start..=gt, "");
            continue;
        }
        let Some(end_rel) = out[gt..].find("</w:sectPr>") else {
            break;
        };
        let end = gt + end_rel + "</w:sectPr>".len();
        out.replace_range(start..end, "");
    }
    out
}

/// 將單一段落在 `cp` 處拆成兩段（回傳兩段完整 `<w:p>` XML）。
fn split_paragraph_xml(
    xml: &[u8],
    p_span: ParagraphSpan,
    analysis: &ParagraphAnalysis,
    cp: usize,
) -> Result<(String, String), MutationError> {
    let text = std::str::from_utf8(xml).map_err(|_| MutationError::XmlPatchRejected {
        detail: "XML 明文非合法 UTF-8".to_string(),
    })?;
    let p_open = text[p_span.start..analysis.p_open_end_offset].to_string();
    let ppr = analysis
        .ppr_end_offset
        .map(|e| text[analysis.p_open_end_offset..e].to_string())
        .unwrap_or_default();
    let p_close = text[analysis.p_close_start_offset..p_span.end].to_string();
    let content_start = analysis.p_open_end_offset;
    let content_end = analysis.p_close_start_offset;

    let left_content;
    let right_content;
    if cp == 0 {
        left_content = String::new();
        right_content = text[content_start..content_end].to_string();
    } else if cp >= analysis.total_cp {
        left_content = text[content_start..content_end].to_string();
        right_content = String::new();
    } else {
        let chosen = analysis
            .text_nodes
            .iter()
            .find(|n| n.start_cp <= cp && cp <= n.end_cp && n.run_span.is_some());
        match chosen {
            Some(node) => {
                let run = run_span_at(xml, node.run_span.unwrap().start).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "run 區間解析失敗".to_string(),
                    }
                })?;
                let run_text = &text[run.start..run.end];
                let open_gt =
                    run_text
                        .find('>')
                        .ok_or_else(|| MutationError::XmlPatchRejected {
                            detail: "run 開標籤解析失敗".to_string(),
                        })?;
                let run_open = run_text[..=open_gt].to_string();
                let run_open_end = run.start + open_gt + 1;
                let close_rel =
                    run_text
                        .rfind("</w:r>")
                        .ok_or_else(|| MutationError::XmlPatchRejected {
                            detail: "run 閉標籤解析失敗".to_string(),
                        })?;
                let run_close_start = run.start + close_rel;
                let run_inner = &text[run_open_end..run_close_start];
                let rpr = leading_element(run_inner, "w:rPr");

                let chars: Vec<char> = node.unescaped.chars().collect();
                let local = cp.saturating_sub(node.start_cp).min(chars.len());
                let left_txt: String = chars[..local].iter().collect();
                let right_txt: String = chars[local..].iter().collect();

                let before = &text[run_open_end..node.node_span.start];
                let after = &text[node.node_span.end..run_close_start];
                let left_t = make_text_tag(&left_txt, node.preserve_space);
                let right_t = make_text_tag(&right_txt, node.preserve_space);

                let left_inner = format!("{before}{left_t}");
                let right_inner = format!("{rpr}{right_t}{after}");

                left_content = format!(
                    "{}{}{}</w:r>",
                    &text[content_start..run.start],
                    run_open,
                    left_inner
                );
                right_content = format!(
                    "{}{}</w:r>{}",
                    run_open,
                    right_inner,
                    &text[run.end..content_end]
                );
            }
            None => {
                let split_abs = analysis
                    .text_nodes
                    .iter()
                    .find(|n| cp <= n.start_cp)
                    .map(|n| n.node_span.start)
                    .unwrap_or(content_end);
                left_content = text[content_start..split_abs].to_string();
                right_content = text[split_abs..content_end].to_string();
            }
        }
    }

    let left = format!("{p_open}{ppr}{left_content}{p_close}");
    let right = format!("{p_open}{}{right_content}{p_close}", strip_sectpr(&ppr));
    Ok((left, right))
}

fn paragraph_inner(full: &str) -> String {
    let Some(gt) = full.find('>') else {
        return String::new();
    };
    let Some(close) = full.rfind("</w:p>") else {
        return String::new();
    };
    full[gt + 1..close].to_string()
}

fn merge_paragraph_xml(
    xml: &[u8],
    p1_span: ParagraphSpan,
    p2_span: ParagraphSpan,
) -> Result<(ByteSpan, Vec<u8>), MutationError> {
    let text = std::str::from_utf8(xml).map_err(|_| MutationError::XmlPatchRejected {
        detail: "XML 明文非合法 UTF-8".to_string(),
    })?;
    let a1 = analyze_paragraph(xml, p1_span);
    let a2 = analyze_paragraph(xml, p2_span);
    let p1_open = &text[p1_span.start..a1.p_open_end_offset];
    let p1_ppr = a1
        .ppr_end_offset
        .map(|e| &text[a1.p_open_end_offset..e])
        .unwrap_or("");
    let p1_content = &text[a1.p_open_end_offset..a1.p_close_start_offset];
    let p2_start = a2.ppr_end_offset.unwrap_or(a2.p_open_end_offset);
    let p2_content = &text[p2_start..a2.p_close_start_offset];
    let merged = format!("{p1_open}{p1_ppr}{p1_content}{p2_content}</w:p>");
    Ok((
        ByteSpan {
            start: p1_span.start,
            end: p2_span.end,
        },
        merged.into_bytes(),
    ))
}

fn remove_element(inner: &str, name: &str) -> String {
    let mut out = inner.to_string();
    let open = format!("<{name}");
    while let Some(start) = out.find(&open) {
        let after = &out[start + open.len()..];
        let boundary_ok = after
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/');
        if !boundary_ok {
            break;
        }
        let Some(gt_rel) = out[start..].find('>') else {
            break;
        };
        let gt = start + gt_rel;
        if out[start..gt].trim_end().ends_with('/') {
            out.replace_range(start..=gt, "");
            continue;
        }
        let close = format!("</{name}>");
        let Some(end_rel) = out[gt..].find(&close) else {
            break;
        };
        let end = gt + end_rel + close.len();
        out.replace_range(start..end, "");
    }
    out
}

fn patch_rpr(rpr: &str, patch: &RunDirectPatch) -> String {
    // [K29B_BUGGY_POINT_2: RPR_PATCH]
    let buggy_rpr = false;
    if buggy_rpr {
        return rpr.to_string();
    }
    let inner = if rpr.is_empty() {
        String::new()
    } else {
        strip_leading_element(rpr, "w:rPr")
    };
    let mut body = inner;
    if patch.bold.is_some() {
        body = remove_element(&body, "w:b");
    }
    if patch.italic.is_some() {
        body = remove_element(&body, "w:i");
    }
    if patch.color_rgb.is_some() {
        body = remove_element(&body, "w:color");
    }
    if patch.size_pt.is_some() {
        body = remove_element(&body, "w:sz");
    }
    if patch.char_style.is_some() {
        body = remove_element(&body, "w:rStyle");
    }

    let mut add = String::new();
    if let Some(style) = &patch.char_style {
        add.push_str(&format!(r#"<w:rStyle w:val="{}"/>"#, xml_escape(style)));
    }
    if let Some(bold) = patch.bold {
        add.push_str(if bold {
            "<w:b/>"
        } else {
            r#"<w:b w:val="0"/>"#
        });
    }
    if let Some(italic) = patch.italic {
        add.push_str(if italic {
            "<w:i/>"
        } else {
            r#"<w:i w:val="0"/>"#
        });
    }
    if let Some(color) = patch.color_rgb {
        add.push_str(&format!(
            r#"<w:color w:val="{:06X}"/>"#,
            color & 0x00FF_FFFF
        ));
    }
    if let Some(size) = patch.size_pt {
        let half = (size * 2.0).round().max(1.0) as i64;
        add.push_str(&format!(r#"<w:sz w:val="{half}"/>"#));
    }
    format!("<w:rPr>{body}{add}</w:rPr>")
}

fn patch_ppr(ppr: &str, patch: &ParaPropsPatch) -> String {
    let inner = if ppr.is_empty() {
        String::new()
    } else {
        strip_leading_element(ppr, "w:pPr")
    };
    let mut body = inner;
    if patch.style_id.is_some() {
        body = remove_element(&body, "w:pStyle");
    }
    if patch.page_break_before.is_some() {
        body = remove_element(&body, "w:pageBreakBefore");
    }
    if patch.line_spacing.is_some() {
        body = remove_element(&body, "w:spacing");
    }
    if patch.alignment.is_some() {
        body = remove_element(&body, "w:jc");
    }

    let mut add = String::new();
    if let Some(style) = &patch.style_id {
        add.push_str(&format!(r#"<w:pStyle w:val="{}"/>"#, xml_escape(style)));
    }
    if let Some(before) = patch.page_break_before {
        add.push_str(if before {
            "<w:pageBreakBefore/>"
        } else {
            r#"<w:pageBreakBefore w:val="0"/>"#
        });
    }
    if let Some(mult) = patch.line_spacing {
        let line = (mult * 240.0).round().max(1.0) as i64;
        add.push_str(&format!(
            r#"<w:spacing w:line="{line}" w:lineRule="auto"/>"#
        ));
    }
    if let Some(jc) = &patch.alignment {
        add.push_str(&format!(r#"<w:jc w:val="{}"/>"#, xml_escape(jc)));
    }
    format!("<w:pPr>{body}{add}</w:pPr>")
}

fn patch_numpr(ppr: &str, action: &ListAction) -> String {
    let inner = if ppr.is_empty() {
        String::new()
    } else {
        strip_leading_element(ppr, "w:pPr")
    };
    let body = remove_element(&inner, "w:numPr");
    let add = match action {
        ListAction::AttachExisting { num_id, ilvl } => {
            format!(r#"<w:numPr><w:ilvl w:val="{ilvl}"/><w:numId w:val="{num_id}"/></w:numPr>"#)
        }
        ListAction::ChangeLevel { ilvl } => {
            let num = extract_num_id(&inner).unwrap_or(0);
            format!(r#"<w:numPr><w:ilvl w:val="{ilvl}"/><w:numId w:val="{num}"/></w:numPr>"#)
        }
        ListAction::RemoveNumbering => String::new(),
    };
    format!("<w:pPr>{body}{add}</w:pPr>")
}

fn extract_num_id(ppr: &str) -> Option<u32> {
    let pos = ppr.find("<w:numId")?;
    let rest = &ppr[pos..];
    let val_pos = rest.find("w:val=")?;
    let after = &rest[val_pos + 6..];
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let end = after[1..].find(quote)?;
    after[1..1 + end].parse().ok()
}

fn replace_paragraph_ppr(
    xml: &[u8],
    p_span: ParagraphSpan,
    new_ppr: String,
) -> Result<Vec<u8>, MutationError> {
    let analysis = analyze_paragraph(xml, p_span);
    let (start, end) = match analysis.ppr_end_offset {
        Some(end) => (analysis.p_open_end_offset, end),
        None => (analysis.p_open_end_offset, analysis.p_open_end_offset),
    };
    let mut out = Vec::with_capacity(xml.len() + new_ppr.len());
    out.extend_from_slice(&xml[..start]);
    out.extend_from_slice(new_ppr.as_bytes());
    out.extend_from_slice(&xml[end..]);
    Ok(out)
}

fn format_paragraph_xml(
    xml: &[u8],
    p_span: ParagraphSpan,
    start_cp: usize,
    end_cp: usize,
    patch: &RunDirectPatch,
) -> Result<Vec<u8>, MutationError> {
    if start_cp >= end_cp {
        return Ok(xml.to_vec());
    }
    let text = std::str::from_utf8(xml).map_err(|_| MutationError::XmlPatchRejected {
        detail: "XML 明文非合法 UTF-8".to_string(),
    })?;
    let analysis = analyze_paragraph(xml, p_span);

    let mut runs: Vec<ByteSpan> = Vec::new();
    for node in &analysis.text_nodes {
        if node.end_cp <= start_cp || node.start_cp >= end_cp {
            continue;
        }
        if let Some(run) = node.run_span.and_then(|r| run_span_at(xml, r.start)) {
            if !runs
                .iter()
                .any(|x| x.start == run.start && x.end == run.end)
            {
                runs.push(run);
            }
        }
    }
    runs.sort_by_key(|r| r.start);

    let mut out = xml.to_vec();
    for run in runs.iter().rev() {
        let run_text = &text[run.start..run.end];
        let Some(open_gt) = run_text.find('>') else {
            continue;
        };
        let run_open = &run_text[..=open_gt];
        let run_open_end = run.start + open_gt + 1;
        let Some(close_rel) = run_text.rfind("</w:r>") else {
            continue;
        };
        let run_close_start = run.start + close_rel;
        let run_inner = &text[run_open_end..run_close_start];
        let rpr = leading_element(run_inner, "w:rPr");
        let nodes: Vec<&TextNodeInfo> = analysis
            .text_nodes
            .iter()
            .filter(|n| n.run_span.is_some_and(|r| r.start == run.start))
            .collect();

        let mut replacement = String::new();
        for node in nodes {
            let chars: Vec<char> = node.unescaped.chars().collect();
            let local_start = start_cp.saturating_sub(node.start_cp).min(chars.len());
            let local_end = end_cp.saturating_sub(node.start_cp).min(chars.len());
            if local_start >= local_end {
                replacement.push_str(&format!(
                    "{run_open}{}{}</w:r>",
                    rpr,
                    make_text_tag(&node.unescaped, node.preserve_space)
                ));
                continue;
            }
            let before: String = chars[..local_start].iter().collect();
            let selected: String = chars[local_start..local_end].iter().collect();
            let after: String = chars[local_end..].iter().collect();
            if !before.is_empty() {
                replacement.push_str(&format!(
                    "{run_open}{}{}</w:r>",
                    rpr,
                    make_text_tag(&before, node.preserve_space)
                ));
            }
            let patched = patch_rpr(&rpr, patch);
            replacement.push_str(&format!(
                "{run_open}{}{}</w:r>",
                patched,
                make_text_tag(&selected, node.preserve_space)
            ));
            if !after.is_empty() {
                replacement.push_str(&format!(
                    "{run_open}{}{}</w:r>",
                    rpr,
                    make_text_tag(&after, node.preserve_space)
                ));
            }
        }

        let mut next = Vec::with_capacity(out.len() + replacement.len());
        next.extend_from_slice(&out[..run.start]);
        next.extend_from_slice(replacement.as_bytes());
        next.extend_from_slice(&out[run.end..]);
        out = next;
    }
    Ok(out)
}

fn find_style_span(xml: &[u8], style_id: &str) -> Option<ByteSpan> {
    let text = std::str::from_utf8(xml).ok()?;
    let mut cursor = 0usize;
    while let Some(rel) = text[cursor..].find("<w:style") {
        let start = cursor + rel;
        let after = &text[start + 8..];
        if !after
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/')
        {
            cursor = start + 8;
            continue;
        }
        let gt_rel = text[start..].find('>')?;
        let gt = start + gt_rel;
        let open = &text[start..=gt];
        if open.contains(&format!(r#"w:styleId="{style_id}""#))
            || open.contains(&format!(r#"w:styleId='{style_id}'"#))
        {
            let close_rel = text[gt..].find("</w:style>")?;
            let end = gt + close_rel + "</w:style>".len();
            return Some(ByteSpan { start, end });
        }
        cursor = gt + 1;
    }
    None
}

impl Engine {
    /// 依據 Mutation 規格套用單筆變更。
    pub fn apply_mutation(&mut self, op: Mutation) -> Result<MutationOutcome, MutationError> {
        // K29c（M4＋M5）：表格與圖片家族走獨立實作檔，避免與 Slice 1/2 的
        // 單段落外科手術主幹糾纏；兩者共用同一 Undo／touched／rebuild 紀律。
        if crate::mutation_k29c::is_k29c_mutation(&op) {
            return self.apply_k29c_mutation(op);
        }
        // K29d（Slice 4）：M6 節／分節／頁首頁尾／註腳尾註家族與非 Body 目標的
        // SubDocTarget 文字路由走獨立實作檔（共用同一 Undo／touched／rebuild 紀律）。
        if crate::mutation_k29d::is_k29d_mutation(&op) {
            return self.apply_k29d_mutation(op);
        }
        let (
            part_name,
            applied_span,
            original_slice,
            affected_para,
            caret_after,
            new_xml,
            structural,
        ) = match &op {
            Mutation::InsertText {
                target,
                pos,
                text,
                style_override: _,
            } => {
                if *target != SubDocTarget::Body {
                    return Err(MutationError::SubDocNotFound {
                        target: target.clone(),
                    });
                }
                let part_name = "word/document.xml";
                let xml = self.doc.parts.get(part_name).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "找不到 word/document.xml".to_string(),
                    }
                })?;

                let (p_span, _total_paras) = find_paragraph_span(xml, pos.para)?;
                let analysis = analyze_paragraph(xml, p_span);

                if pos.cp > analysis.total_cp {
                    return Err(MutationError::CodepointOutOfBounds {
                        para: pos.para,
                        start_cp: pos.cp,
                        end_cp: pos.cp,
                        para_len: analysis.total_cp,
                    });
                }

                let mut replaced_span = None;
                let mut replacement_bytes = Vec::new();

                // 嘗試尋找涵蓋該碼點之既有 <w:t>
                for node in &analysis.text_nodes {
                    if (node.start_cp <= pos.cp && pos.cp < node.end_cp)
                        || (pos.cp == node.end_cp && pos.cp == analysis.total_cp)
                    {
                        let local_cp = pos.cp - node.start_cp;
                        let mut new_text = String::new();
                        let mut count = 0;
                        for ch in node.unescaped.chars() {
                            if count == local_cp {
                                new_text.push_str(text);
                            }
                            new_text.push(ch);
                            count += 1;
                        }
                        if count == local_cp {
                            new_text.push_str(text);
                        }

                        let needs_preserve = node.preserve_space
                            || new_text.starts_with(is_xml_space)
                            || new_text.ends_with(is_xml_space);
                        let new_tag = if needs_preserve {
                            format!(
                                r#"<w:t xml:space="preserve">{}</w:t>"#,
                                xml_escape(&new_text)
                            )
                        } else {
                            format!(r#"<w:t>{}</w:t>"#, xml_escape(&new_text))
                        };

                        replaced_span = Some(node.node_span);
                        replacement_bytes = new_tag.into_bytes();
                        break;
                    }
                }

                if replaced_span.is_none() {
                    // 段落內無 <w:t>，在 pPr 後或段首插入新 run
                    let ins_offset = analysis
                        .ppr_end_offset
                        .unwrap_or(analysis.p_open_end_offset);
                    let needs_preserve =
                        text.starts_with(is_xml_space) || text.ends_with(is_xml_space);
                    let new_run = if needs_preserve {
                        format!(
                            r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                            xml_escape(text)
                        )
                    } else {
                        format!(r#"<w:r><w:t>{}</w:t></w:r>"#, xml_escape(text))
                    };
                    replaced_span = Some(ByteSpan {
                        start: ins_offset,
                        end: ins_offset,
                    });
                    replacement_bytes = new_run.into_bytes();
                }

                let span = replaced_span.unwrap();
                let orig_slice = xml[span.start..span.end].to_vec();

                let mut new_xml =
                    Vec::with_capacity(xml.len() + replacement_bytes.len() - orig_slice.len());
                new_xml.extend_from_slice(&xml[..span.start]);
                new_xml.extend_from_slice(&replacement_bytes);
                new_xml.extend_from_slice(&xml[span.end..]);

                let applied = ByteSpan {
                    start: span.start,
                    end: span.start + replacement_bytes.len(),
                };

                let caret_after = DocPos {
                    para: pos.para,
                    cp: pos.cp + text.chars().count(),
                };

                (
                    part_name.to_string(),
                    applied,
                    orig_slice,
                    Some(pos.para),
                    Some(caret_after),
                    new_xml,
                    false,
                )
            }

            Mutation::DeleteRange { range } => {
                if range.target != SubDocTarget::Body {
                    return Err(MutationError::SubDocNotFound {
                        target: range.target.clone(),
                    });
                }
                if range.start.para > range.end.para {
                    return Err(MutationError::CodepointOutOfBounds {
                        para: range.start.para,
                        start_cp: range.start.cp,
                        end_cp: range.end.cp,
                        para_len: 0,
                    });
                }
                let part_name = "word/document.xml";
                let xml = self.doc.parts.get(part_name).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "找不到 word/document.xml".to_string(),
                    }
                })?;

                let (p_start_span, _total_paras) = find_paragraph_span(xml, range.start.para)?;
                let a_start = analyze_paragraph(xml, p_start_span);
                if range.start.cp > a_start.total_cp {
                    return Err(MutationError::CodepointOutOfBounds {
                        para: range.start.para,
                        start_cp: range.start.cp,
                        end_cp: range.end.cp,
                        para_len: a_start.total_cp,
                    });
                }

                if range.start.para == range.end.para {
                    if range.start.cp > range.end.cp || range.end.cp > a_start.total_cp {
                        return Err(MutationError::CodepointOutOfBounds {
                            para: range.start.para,
                            start_cp: range.start.cp,
                            end_cp: range.end.cp,
                            para_len: a_start.total_cp,
                        });
                    }
                    if range.start.cp == range.end.cp {
                        return Ok(MutationOutcome {
                            events: Vec::new(),
                            touched_parts: Vec::new(),
                            inverse: Vec::new(),
                            caret_after: Some(range.start),
                            reversibility: ReversibilityLevel::StrictByteReversible,
                        });
                    }
                    // 先走 Slice 1 的單一 <w:t> 局部刪除（巢狀文字框段落安全）。
                    let mut replaced_span = None;
                    let mut replacement_bytes = Vec::new();
                    for node in &a_start.text_nodes {
                        if node.start_cp <= range.start.cp && range.end.cp <= node.end_cp {
                            let local_start = range.start.cp - node.start_cp;
                            let local_end = range.end.cp - node.start_cp;
                            let mut new_text = String::new();
                            for (idx, ch) in node.unescaped.chars().enumerate() {
                                if idx < local_start || idx >= local_end {
                                    new_text.push(ch);
                                }
                            }
                            let new_tag = make_text_tag(&new_text, node.preserve_space);
                            replaced_span = Some(node.node_span);
                            replacement_bytes = new_tag.into_bytes();
                            break;
                        }
                    }
                    if let Some(span) = replaced_span {
                        let orig_slice = xml[span.start..span.end].to_vec();
                        let mut new_xml = Vec::with_capacity(
                            xml.len() + replacement_bytes.len() - orig_slice.len(),
                        );
                        new_xml.extend_from_slice(&xml[..span.start]);
                        new_xml.extend_from_slice(&replacement_bytes);
                        new_xml.extend_from_slice(&xml[span.end..]);
                        let applied = ByteSpan {
                            start: span.start,
                            end: span.start + replacement_bytes.len(),
                        };
                        let caret_after = DocPos {
                            para: range.start.para,
                            cp: range.start.cp,
                        };
                        (
                            part_name.to_string(),
                            applied,
                            orig_slice,
                            Some(range.start.para),
                            Some(caret_after),
                            new_xml,
                            false,
                        )
                    } else {
                        let text = std::str::from_utf8(xml).unwrap_or("");
                        let (left_full, _) =
                            split_paragraph_xml(xml, p_start_span, &a_start, range.start.cp)?;
                        let (_, right_full) =
                            split_paragraph_xml(xml, p_start_span, &a_start, range.end.cp)?;
                        let p_open = &text[p_start_span.start..a_start.p_open_end_offset];
                        let prefix = paragraph_inner(&left_full);
                        let suffix = strip_leading_element(&paragraph_inner(&right_full), "w:pPr");
                        let replacement = format!("{p_open}{prefix}{suffix}</w:p>");
                        let orig_slice = xml[p_start_span.start..p_start_span.end].to_vec();
                        let applied = ByteSpan {
                            start: p_start_span.start,
                            end: p_start_span.start + replacement.len(),
                        };
                        let mut new_xml =
                            Vec::with_capacity(xml.len() + replacement.len() - orig_slice.len());
                        new_xml.extend_from_slice(&xml[..p_start_span.start]);
                        new_xml.extend_from_slice(replacement.as_bytes());
                        new_xml.extend_from_slice(&xml[p_start_span.end..]);
                        let caret_after = DocPos {
                            para: range.start.para,
                            cp: range.start.cp,
                        };
                        (
                            part_name.to_string(),
                            applied,
                            orig_slice,
                            Some(range.start.para),
                            Some(caret_after),
                            new_xml,
                            false,
                        )
                    }
                } else {
                    let path_start = paragraph_container_path(xml, range.start.para)?;
                    let path_end = paragraph_container_path(xml, range.end.para)?;
                    if path_start != path_end {
                        return Err(MutationError::CrossContainerBoundary {
                            start_para: range.start.para,
                            end_para: range.end.para,
                        });
                    }
                    let (p_end_span, _) = find_paragraph_span(xml, range.end.para)?;
                    let a_end = analyze_paragraph(xml, p_end_span);
                    if range.end.cp > a_end.total_cp {
                        return Err(MutationError::CodepointOutOfBounds {
                            para: range.end.para,
                            start_cp: range.start.cp,
                            end_cp: range.end.cp,
                            para_len: a_end.total_cp,
                        });
                    }

                    let text = std::str::from_utf8(xml).unwrap_or("");
                    let (left_full, _) =
                        split_paragraph_xml(xml, p_start_span, &a_start, range.start.cp)?;
                    let (_, right_full) =
                        split_paragraph_xml(xml, p_end_span, &a_end, range.end.cp)?;
                    let p_open = &text[p_start_span.start..a_start.p_open_end_offset];
                    let prefix = paragraph_inner(&left_full);
                    let suffix = strip_leading_element(&paragraph_inner(&right_full), "w:pPr");
                    let replacement = format!("{p_open}{prefix}{suffix}</w:p>");
                    let span = ByteSpan {
                        start: p_start_span.start,
                        end: p_end_span.end,
                    };
                    let orig_slice = xml[span.start..span.end].to_vec();
                    let mut new_xml =
                        Vec::with_capacity(xml.len() + replacement.len() - orig_slice.len());
                    new_xml.extend_from_slice(&xml[..span.start]);
                    new_xml.extend_from_slice(replacement.as_bytes());
                    new_xml.extend_from_slice(&xml[span.end..]);
                    let applied = ByteSpan {
                        start: span.start,
                        end: span.start + replacement.len(),
                    };
                    let caret_after = DocPos {
                        para: range.start.para,
                        cp: range.start.cp,
                    };
                    (
                        part_name.to_string(),
                        applied,
                        orig_slice,
                        Some(range.start.para),
                        Some(caret_after),
                        new_xml,
                        true,
                    )
                }
            }

            Mutation::InsertBreak { target, pos, kind } => {
                if *target != SubDocTarget::Body {
                    return Err(MutationError::SubDocNotFound {
                        target: target.clone(),
                    });
                }
                let part_name = "word/document.xml";
                let xml = self.doc.parts.get(part_name).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "找不到 word/document.xml".to_string(),
                    }
                })?;

                let (p_span, _total_paras) = find_paragraph_span(xml, pos.para)?;
                let analysis = analyze_paragraph(xml, p_span);

                if pos.cp > analysis.total_cp {
                    return Err(MutationError::CodepointOutOfBounds {
                        para: pos.para,
                        start_cp: pos.cp,
                        end_cp: pos.cp,
                        para_len: analysis.total_cp,
                    });
                }

                let break_tag = match kind {
                    BreakKind::LineWrapping => "<w:br/>",
                    BreakKind::Page => r#"<w:br w:type="page"/>"#,
                    BreakKind::Column => r#"<w:br w:type="column"/>"#,
                    BreakKind::Tab => "<w:tab/>",
                };

                let mut replaced_span = None;
                let mut replacement_bytes = Vec::new();

                for node in &analysis.text_nodes {
                    if node.start_cp <= pos.cp && pos.cp <= node.end_cp {
                        let local_cp = pos.cp - node.start_cp;
                        if local_cp == 0 {
                            if let Some(r_span) = node.run_span {
                                replaced_span = Some(ByteSpan {
                                    start: r_span.start,
                                    end: r_span.start,
                                });
                                replacement_bytes =
                                    format!("<w:r>{}</w:r>", break_tag).into_bytes();
                                break;
                            }
                        } else if local_cp == node.unescaped.chars().count() {
                            if let Some(r_span) = node.run_span {
                                replaced_span = Some(ByteSpan {
                                    start: r_span.end,
                                    end: r_span.end,
                                });
                                replacement_bytes =
                                    format!("<w:r>{}</w:r>", break_tag).into_bytes();
                                break;
                            }
                        } else {
                            // 拆開 <w:t>
                            let chars: Vec<char> = node.unescaped.chars().collect();
                            let s1: String = chars[..local_cp].iter().collect();
                            let s2: String = chars[local_cp..].iter().collect();

                            let t1 = if node.preserve_space
                                || s1.starts_with(is_xml_space)
                                || s1.ends_with(is_xml_space)
                            {
                                format!(r#"<w:t xml:space="preserve">{}</w:t>"#, xml_escape(&s1))
                            } else {
                                format!(r#"<w:t>{}</w:t>"#, xml_escape(&s1))
                            };

                            let t2 = if node.preserve_space
                                || s2.starts_with(is_xml_space)
                                || s2.ends_with(is_xml_space)
                            {
                                format!(r#"<w:t xml:space="preserve">{}</w:t>"#, xml_escape(&s2))
                            } else {
                                format!(r#"<w:t>{}</w:t>"#, xml_escape(&s2))
                            };

                            let new_content = format!("{}{}{}", t1, break_tag, t2);
                            replaced_span = Some(node.node_span);
                            replacement_bytes = new_content.into_bytes();
                            break;
                        }
                    }
                }

                if replaced_span.is_none() {
                    let ins_offset = analysis.p_close_start_offset;
                    replaced_span = Some(ByteSpan {
                        start: ins_offset,
                        end: ins_offset,
                    });
                    replacement_bytes = format!("<w:r>{}</w:r>", break_tag).into_bytes();
                }

                let span = replaced_span.unwrap();
                let orig_slice = xml[span.start..span.end].to_vec();

                let mut new_xml =
                    Vec::with_capacity(xml.len() + replacement_bytes.len() - orig_slice.len());
                new_xml.extend_from_slice(&xml[..span.start]);
                new_xml.extend_from_slice(&replacement_bytes);
                new_xml.extend_from_slice(&xml[span.end..]);

                let applied = ByteSpan {
                    start: span.start,
                    end: span.start + replacement_bytes.len(),
                };

                let caret_after = DocPos {
                    para: pos.para,
                    cp: pos.cp + 1,
                };

                (
                    part_name.to_string(),
                    applied,
                    orig_slice,
                    Some(pos.para),
                    Some(caret_after),
                    new_xml,
                    false,
                )
            }

            Mutation::SplitParagraph { target, pos } => {
                if *target != SubDocTarget::Body {
                    return Err(MutationError::SubDocNotFound {
                        target: target.clone(),
                    });
                }
                let part_name = "word/document.xml";
                let xml = self.doc.parts.get(part_name).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "找不到 word/document.xml".to_string(),
                    }
                })?;
                let (p_span, _) = find_paragraph_span(xml, pos.para)?;
                let analysis = analyze_paragraph(xml, p_span);
                if pos.cp > analysis.total_cp {
                    return Err(MutationError::CodepointOutOfBounds {
                        para: pos.para,
                        start_cp: pos.cp,
                        end_cp: pos.cp,
                        para_len: analysis.total_cp,
                    });
                }
                let (left, right) = split_paragraph_xml(xml, p_span, &analysis, pos.cp)?;
                let replacement = format!("{left}{right}");
                let orig_slice = xml[p_span.start..p_span.end].to_vec();
                let applied = ByteSpan {
                    start: p_span.start,
                    end: p_span.start + replacement.len(),
                };
                let mut new_xml =
                    Vec::with_capacity(xml.len() + replacement.len() - orig_slice.len());
                new_xml.extend_from_slice(&xml[..p_span.start]);
                new_xml.extend_from_slice(replacement.as_bytes());
                new_xml.extend_from_slice(&xml[p_span.end..]);
                let caret_after = DocPos {
                    para: pos.para + 1,
                    cp: 0,
                };
                (
                    part_name.to_string(),
                    applied,
                    orig_slice,
                    Some(pos.para),
                    Some(caret_after),
                    new_xml,
                    true,
                )
            }

            Mutation::MergeParagraphWithNext { target, para } => {
                if *target != SubDocTarget::Body {
                    return Err(MutationError::SubDocNotFound {
                        target: target.clone(),
                    });
                }
                let part_name = "word/document.xml";
                let xml = self.doc.parts.get(part_name).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "找不到 word/document.xml".to_string(),
                    }
                })?;
                let path1 = paragraph_container_path(xml, *para)?;
                let path2 = paragraph_container_path(xml, para + 1)?;
                if path1 != path2 {
                    return Err(MutationError::CrossContainerBoundary {
                        start_para: *para,
                        end_para: para + 1,
                    });
                }
                let (p1, _) = find_paragraph_span(xml, *para)?;
                let (p2, _) = find_paragraph_span(xml, para + 1)?;
                let (span, merged) = merge_paragraph_xml(xml, p1, p2)?;
                let orig_slice = xml[span.start..span.end].to_vec();
                let mut new_xml = Vec::with_capacity(xml.len() + merged.len() - orig_slice.len());
                new_xml.extend_from_slice(&xml[..span.start]);
                new_xml.extend_from_slice(&merged);
                new_xml.extend_from_slice(&xml[span.end..]);
                let applied = ByteSpan {
                    start: span.start,
                    end: span.start + merged.len(),
                };
                let caret_after = DocPos { para: *para, cp: 0 };
                (
                    part_name.to_string(),
                    applied,
                    orig_slice,
                    Some(*para),
                    Some(caret_after),
                    new_xml,
                    true,
                )
            }

            Mutation::FormatTextRange { range, patch } => {
                if range.target != SubDocTarget::Body {
                    return Err(MutationError::SubDocNotFound {
                        target: range.target.clone(),
                    });
                }
                if range.start.para > range.end.para {
                    return Err(MutationError::CodepointOutOfBounds {
                        para: range.start.para,
                        start_cp: range.start.cp,
                        end_cp: range.end.cp,
                        para_len: 0,
                    });
                }
                let part_name = "word/document.xml";
                let mut new_xml = self
                    .doc
                    .parts
                    .get(part_name)
                    .ok_or_else(|| MutationError::XmlPatchRejected {
                        detail: "找不到 word/document.xml".to_string(),
                    })?
                    .clone();
                let orig_slice = new_xml.clone();
                for p in (range.start.para..=range.end.para).rev() {
                    let (p_span, _) = find_paragraph_span(&new_xml, p)?;
                    let analysis = analyze_paragraph(&new_xml, p_span);
                    let s = if p == range.start.para {
                        range.start.cp
                    } else {
                        0
                    };
                    let e = if p == range.end.para {
                        range.end.cp
                    } else {
                        analysis.total_cp
                    };
                    if s > analysis.total_cp || e > analysis.total_cp {
                        return Err(MutationError::CodepointOutOfBounds {
                            para: p,
                            start_cp: s,
                            end_cp: e,
                            para_len: analysis.total_cp,
                        });
                    }
                    new_xml = format_paragraph_xml(&new_xml, p_span, s, e, patch)?;
                }
                let applied = ByteSpan {
                    start: 0,
                    end: new_xml.len(),
                };
                let caret_after = DocPos {
                    para: range.start.para,
                    cp: range.start.cp,
                };
                (
                    part_name.to_string(),
                    applied,
                    orig_slice,
                    Some(range.start.para),
                    Some(caret_after),
                    new_xml,
                    true,
                )
            }

            Mutation::SetParagraphStyle {
                target,
                para_range,
                patch,
            } => {
                if *target != SubDocTarget::Body {
                    return Err(MutationError::SubDocNotFound {
                        target: target.clone(),
                    });
                }
                if let Some(style_id) = &patch.style_id {
                    if !self.style_table.is_paragraph_style(style_id) {
                        return Err(MutationError::InvalidStyleReference {
                            style_id: style_id.clone(),
                            expected_type: "paragraph".to_string(),
                        });
                    }
                }
                let part_name = "word/document.xml";
                let mut new_xml = self
                    .doc
                    .parts
                    .get(part_name)
                    .ok_or_else(|| MutationError::XmlPatchRejected {
                        detail: "找不到 word/document.xml".to_string(),
                    })?
                    .clone();
                let orig_slice = new_xml.clone();
                let start_para = *para_range.start();
                let end_para = *para_range.end();
                if start_para > end_para {
                    return Err(MutationError::ParagraphOutOfBounds {
                        para: start_para,
                        len: 0,
                    });
                }
                for p in (start_para..=end_para).rev() {
                    let (p_span, _) = find_paragraph_span(&new_xml, p)?;
                    let analysis = analyze_paragraph(&new_xml, p_span);
                    let existing_ppr = analysis
                        .ppr_end_offset
                        .map(|e| {
                            std::str::from_utf8(&new_xml[analysis.p_open_end_offset..e])
                                .unwrap_or("")
                                .to_string()
                        })
                        .unwrap_or_default();
                    let new_ppr = patch_ppr(&existing_ppr, patch);
                    new_xml = replace_paragraph_ppr(&new_xml, p_span, new_ppr)?;
                }
                let applied = ByteSpan {
                    start: 0,
                    end: new_xml.len(),
                };
                let caret_after = DocPos {
                    para: start_para,
                    cp: 0,
                };
                (
                    part_name.to_string(),
                    applied,
                    orig_slice,
                    Some(start_para),
                    Some(caret_after),
                    new_xml,
                    true,
                )
            }

            Mutation::UpsertNamedStyle {
                style_id,
                style_type,
                based_on,
                para_patch,
                run_patch,
            } => {
                let part_name = "word/styles.xml";
                let xml = self.doc.parts.get(part_name).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "找不到 word/styles.xml".to_string(),
                    }
                })?;
                if let Some(existing) = self.style_table.style_type(style_id) {
                    if existing != style_type.as_w_val() {
                        return Err(MutationError::InvalidStyleReference {
                            style_id: style_id.clone(),
                            expected_type: style_type.as_w_val().to_string(),
                        });
                    }
                }
                if let Some(base) = based_on {
                    if base == style_id {
                        return Err(MutationError::InvalidStyleReference {
                            style_id: style_id.clone(),
                            expected_type: style_type.as_w_val().to_string(),
                        });
                    }
                    if let Some(base_type) = self.style_table.style_type(base) {
                        if base_type != style_type.as_w_val() {
                            return Err(MutationError::InvalidStyleReference {
                                style_id: style_id.clone(),
                                expected_type: style_type.as_w_val().to_string(),
                            });
                        }
                    }
                }
                let text = std::str::from_utf8(xml).unwrap_or("");
                let (orig_slice, replacement, span) = match find_style_span(xml, style_id) {
                    Some(span) => {
                        let style_xml = &text[span.start..span.end];
                        let open_gt = style_xml.find('>').unwrap_or(0);
                        let open = &style_xml[..=open_gt];
                        let close = "</w:style>";
                        let inner_end = style_xml.len() - close.len();
                        let inner = &style_xml[open_gt + 1..inner_end];
                        let mut body = inner.to_string();
                        if let Some(rp) = run_patch {
                            let rpr = leading_element(&body, "w:rPr");
                            let patched = patch_rpr(&rpr, rp);
                            if rpr.is_empty() {
                                body.push_str(&patched);
                            } else {
                                body = body.replacen(&rpr, &patched, 1);
                            }
                        }
                        if let Some(pp) = para_patch {
                            let ppr = leading_element(&body, "w:pPr");
                            let patched = patch_ppr(&ppr, pp);
                            if ppr.is_empty() {
                                body.push_str(&patched);
                            } else {
                                body = body.replacen(&ppr, &patched, 1);
                            }
                        }
                        let new_style = format!("{open}{body}{close}");
                        (
                            xml[span.start..span.end].to_vec(),
                            new_style.into_bytes(),
                            span,
                        )
                    }
                    None => {
                        let mut new_style = format!(
                            r#"<w:style w:type="{}" w:styleId="{}">"#,
                            style_type.as_w_val(),
                            xml_escape(style_id)
                        );
                        new_style
                            .push_str(&format!(r#"<w:name w:val="{}"/>"#, xml_escape(style_id)));
                        if let Some(base) = based_on {
                            new_style
                                .push_str(&format!(r#"<w:basedOn w:val="{}"/>"#, xml_escape(base)));
                        }
                        if let Some(pp) = para_patch {
                            new_style.push_str(&patch_ppr("", pp));
                        }
                        if let Some(rp) = run_patch {
                            new_style.push_str(&patch_rpr("", rp));
                        }
                        new_style.push_str("</w:style>");
                        let insert_at = text.rfind("</w:styles>").ok_or_else(|| {
                            MutationError::XmlPatchRejected {
                                detail: "styles.xml 缺少 </w:styles>".to_string(),
                            }
                        })?;
                        (
                            Vec::new(),
                            new_style.into_bytes(),
                            ByteSpan {
                                start: insert_at,
                                end: insert_at,
                            },
                        )
                    }
                };
                let mut new_xml =
                    Vec::with_capacity(xml.len() + replacement.len() - orig_slice.len());
                new_xml.extend_from_slice(&xml[..span.start]);
                new_xml.extend_from_slice(&replacement);
                new_xml.extend_from_slice(&xml[span.end..]);
                let applied = ByteSpan {
                    start: span.start,
                    end: span.start + replacement.len(),
                };
                (
                    part_name.to_string(),
                    applied,
                    orig_slice,
                    None,
                    None,
                    new_xml,
                    true,
                )
            }

            Mutation::SetListNumbering {
                target,
                para_range,
                action,
            } => {
                if *target != SubDocTarget::Body {
                    return Err(MutationError::SubDocNotFound {
                        target: target.clone(),
                    });
                }
                let (num_id, ilvl) = match action {
                    ListAction::AttachExisting { num_id, ilvl } => (Some(*num_id), Some(*ilvl)),
                    ListAction::ChangeLevel { ilvl } => (None, Some(*ilvl)),
                    ListAction::RemoveNumbering => (None, None),
                };
                if let Some(lv) = ilvl {
                    if lv > 8 {
                        return Err(MutationError::InvalidNumberingReference {
                            num_id: num_id.unwrap_or(0),
                            ilvl: lv,
                        });
                    }
                }
                if let Some(id) = num_id {
                    let numbering = quire_ooxml::Numbering::from_parts(&self.doc.parts);
                    if numbering.num(id).is_none() {
                        return Err(MutationError::InvalidNumberingReference {
                            num_id: id,
                            ilvl: ilvl.unwrap_or(0),
                        });
                    }
                }
                let part_name = "word/document.xml";
                let mut new_xml = self
                    .doc
                    .parts
                    .get(part_name)
                    .ok_or_else(|| MutationError::XmlPatchRejected {
                        detail: "找不到 word/document.xml".to_string(),
                    })?
                    .clone();
                let orig_slice = new_xml.clone();
                let start_para = *para_range.start();
                let end_para = *para_range.end();
                if start_para > end_para {
                    return Err(MutationError::ParagraphOutOfBounds {
                        para: start_para,
                        len: 0,
                    });
                }
                for p in (start_para..=end_para).rev() {
                    let (p_span, _) = find_paragraph_span(&new_xml, p)?;
                    let analysis = analyze_paragraph(&new_xml, p_span);
                    let existing_ppr = analysis
                        .ppr_end_offset
                        .map(|e| {
                            std::str::from_utf8(&new_xml[analysis.p_open_end_offset..e])
                                .unwrap_or("")
                                .to_string()
                        })
                        .unwrap_or_default();
                    let new_ppr = patch_numpr(&existing_ppr, action);
                    new_xml = replace_paragraph_ppr(&new_xml, p_span, new_ppr)?;
                }
                let applied = ByteSpan {
                    start: 0,
                    end: new_xml.len(),
                };
                let caret_after = DocPos {
                    para: start_para,
                    cp: 0,
                };
                (
                    part_name.to_string(),
                    applied,
                    orig_slice,
                    Some(start_para),
                    Some(caret_after),
                    new_xml,
                    true,
                )
            }

            // K29c 變體已於函式開頭前置分派（見 `apply_k29c_mutation`）。
            _ => unreachable!("K29c Mutation 不應進入 Slice 1/2 主幹"),
        };

        // [K29A_BUGGY_POINT_1: PARTS_WRITEBACK]
        self.doc.parts.insert(part_name.clone(), new_xml);

        // 維護 touched 白名單（若位元組與開檔完全相同則自動退場）
        if let (Some(cur), Some(baseline)) = (
            self.doc.parts.get(&part_name),
            self.part_baselines.get(&part_name),
        ) {
            if cur == baseline {
                self.touched.remove(&part_name);
            } else {
                self.touched.insert(part_name.clone(), ());
            }
        }

        // [K29B_BUGGY_POINT_1: STRUCTURAL_REBUILD]
        let skip_structural_rebuild = false;
        if structural && !skip_structural_rebuild {
            self.rebuild_derived()
                .map_err(|e| MutationError::Engine(e.to_string()))?;
        } else if let Some(affected_para) = affected_para {
            // 同步記憶體語義模型
            let updated_xml = self.doc.parts.get(&part_name).unwrap();
            let (updated_p_span, _) = find_paragraph_span(updated_xml, affected_para)?;
            let new_para_text = extract_para_text_from_xml(updated_xml, updated_p_span);

            let _ = self.replace_paragraph_text(affected_para, &new_para_text);

            if let Some(p) = self.doc.paragraphs.get_mut(affected_para) {
                if let Some(first) = p.runs.first_mut() {
                    first.text = new_para_text.clone();
                    first.field = None;
                    for r in p.runs.iter_mut().skip(1) {
                        r.text.clear();
                    }
                } else {
                    p.runs.push(quire_ooxml::Run {
                        text: new_para_text.clone(),
                        ..Default::default()
                    });
                }
            }
        }

        while !self.is_settled() {
            self.reflow_step_json();
        }

        // 建立 Undo 記錄
        let reverse_patch = XmlReversePatch {
            part_name: part_name.clone(),
            applied_span,
            original_slice,
        };

        let undo_entry = UndoEntry {
            tx_id: self.undo_stack.len() as u64 + 1,
            inverse_ops: vec![InverseMutation {
                semantic_inverse: SemanticInverseOp::Mutation(op),
                xml_patches: vec![reverse_patch],
                part_lifecycle_inverse: Vec::new(),
            }],
            caret_before: None,
            caret_after,
            reversibility: ReversibilityLevel::StrictByteReversible,
        };

        self.undo_stack.push(undo_entry);
        self.redo_stack.clear();

        Ok(MutationOutcome {
            events: Vec::new(),
            touched_parts: vec![part_name],
            inverse: Vec::new(),
            caret_after,
            reversibility: ReversibilityLevel::StrictByteReversible,
        })
    }

    /// 批次套用 Mutation。
    pub fn apply_batch(&mut self, ops: Vec<Mutation>) -> Result<MutationOutcome, MutationError> {
        let mut last_outcome = None;
        for op in ops {
            last_outcome = Some(self.apply_mutation(op)?);
        }
        last_outcome.ok_or_else(|| MutationError::XmlPatchRejected {
            detail: "空批次操作".to_string(),
        })
    }

    /// 復原頂端操作。
    pub fn undo(&mut self) -> Result<Option<MutationOutcome>, MutationError> {
        let entry = match self.undo_stack.pop() {
            Some(e) => e,
            None => return Ok(None),
        };

        let mut affected_parts = Vec::new();

        for inv in entry.inverse_ops.iter().rev() {
            for patch in inv.xml_patches.iter().rev() {
                let xml = self.doc.parts.get_mut(&patch.part_name).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: format!("找不到 part {}", patch.part_name),
                    }
                })?;

                let mut restored_xml = Vec::with_capacity(
                    xml.len() + patch.original_slice.len()
                        - (patch.applied_span.end - patch.applied_span.start),
                );
                restored_xml.extend_from_slice(&xml[..patch.applied_span.start]);
                restored_xml.extend_from_slice(&patch.original_slice);
                restored_xml.extend_from_slice(&xml[patch.applied_span.end..]);
                // [K29A_BUGGY_POINT_2: UNDO_XML_RESTORE]
                *xml = restored_xml;

                affected_parts.push(patch.part_name.clone());
            }
            // 容器層生命週期逆動作（K29c M5.1／M5.3）：本次會話新增的媒體
            // part 在 Undo 時移除；新增 part 不進 part_baselines，移除後自然
            // 回到開檔 part 集合。
            for life in inv.part_lifecycle_inverse.iter().rev() {
                match life {
                    PartLifecycleInverse::RemoveSynthesizedPart(name) => {
                        self.doc.remove_part(name);
                        self.touched.remove(name);
                    }
                    PartLifecycleInverse::RestoreRemovedPart { name, bytes } => {
                        self.doc.set_part(name, bytes.clone());
                        self.touched.insert(name.clone(), ());
                        affected_parts.push(name.clone());
                    }
                }
            }
        }

        // touched 自動退場機制
        for part_name in &affected_parts {
            if let (Some(cur), Some(baseline)) = (
                self.doc.parts.get(part_name),
                self.part_baselines.get(part_name),
            ) {
                if cur == baseline {
                    self.touched.remove(part_name);
                } else {
                    self.touched.insert(part_name.clone(), ());
                }
            }
        }

        // K29b：結構／樣式／清單修補後以 parts 重建全部派生層。
        self.rebuild_derived()
            .map_err(|e| MutationError::Engine(e.to_string()))?;

        self.redo_stack.push(entry);

        Ok(Some(MutationOutcome {
            events: Vec::new(),
            touched_parts: affected_parts,
            inverse: Vec::new(),
            caret_after: None,
            reversibility: ReversibilityLevel::StrictByteReversible,
        }))
    }

    /// 是否仍有可復原的歷史（K29c 不可逆降級會截斷堆疊）。
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// 是否仍有可重做的歷史。
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// 重做頂端操作。
    pub fn redo(&mut self) -> Result<Option<MutationOutcome>, MutationError> {
        let entry = match self.redo_stack.pop() {
            Some(e) => e,
            None => return Ok(None),
        };

        // Redo 藉由重新執行原語義操作或逆向補丁
        for inv in &entry.inverse_ops {
            match &inv.semantic_inverse {
                SemanticInverseOp::Mutation(m) => {
                    self.apply_mutation(m.clone())?;
                }
            }
        }

        Ok(Some(MutationOutcome {
            events: Vec::new(),
            touched_parts: vec!["word/document.xml".to_string()],
            inverse: Vec::new(),
            caret_after: entry.caret_after,
            reversibility: ReversibilityLevel::StrictByteReversible,
        }))
    }
}
