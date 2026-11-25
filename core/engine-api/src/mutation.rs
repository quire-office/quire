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

/// 底線形態（K29e；ECMA-376 §17.3.2.41 `w:u/@w:val` 之 ST_UnderlineValues 子集）。
///
/// **E-2b 共型**：本符號不再是 mutation.rs 自造的鏡像枚舉（舊五值
/// `Single|Double|Dotted|Dashed|None`），而是解析/發射端單一真值源
/// [`quire_render_trait::Underline`] 的**別名**——寫入端、解析端、發射端三端
/// 同型，編譯期防再次分岔（考卷 `e2b_underline_shared_type` 燈 1 鎖死；舊鏡像
/// 枚舉就漏接了 `Thick`，corpus 實見×5）。既有 `UnderlineKind` import 路徑零
/// 斷裂，但語義要點兩處：
/// - 舊 `None` 變體＝本型別 `Underline::Off`（明確移除，寫 `<w:u w:val="none"/>`
///   ——不是「不碰」；「不碰」是欄位級 `Option::None`，見 [`RunDirectPatch::underline`]）；
/// - 新增可寫形態 `Thick`（共型紅利），`Raw(_)` 寫入端**誠實拒絕**（引擎無從
///   替用戶編造未支援形態，且 Raw 字面直插 `w:val` 屬未驗證注入面）——見
///   `reject_unwritable_underline`。
///
/// serde 遷移記帳（破向後兼容，顯式）：序列化輸出從變體名大寫（`"Single"…
/// "None"`）改為 canonical 小寫（`"single"…"off"`，`Raw` 外部標籤形）。舊態
/// JSON 經 serde alias **雙讀**繼續可反序列化（遷移期紀律：舊殼 pkg 未同步
/// 重建不得炸，考卷燈 4；雙讀是讀端寬容，不是寫端權利，燈 5）。外殼
/// `shells/web/editor/js/engine-adapter.js` 同步改 canonical＋兼容正規化。
pub use quire_render_trait::Underline as UnderlineKind;

/// 刪除線樣式（K29e；ECMA-376 §17.3.2.21 `w:strike` ＋ §17.3.2.11 `w:dstrike`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrikeKind {
    /// 單線刪除線（`<w:strike/>`）。
    Single,
    /// 雙線刪除線。取捨：同時寫出 `<w:strike/>`＋`<w:dstrike/>`——Word/LO
    /// 以 `dstrike` 為準繪雙線，而本引擎發射層目前只消費 `w:strike`
    /// （`w:dstrike` 的解析層消費會動到既有 corpus 三檔的渲染，與本編輯卡
    /// 「全庫 A/B 零 DIFF」口徑衝突，另立卡處理）；保留 `w:strike` 使
    /// 「設定→存檔→重解析→讀回」閉環在現行解析器下真實成立，不虛構讀回。
    Double,
    /// 明確關閉（`<w:strike w:val="0"/>`）。
    None,
}

/// 字元級直接格式覆寫（K29b／M2.1；K29e 補齊底線／刪除線／上下標／
/// highlight／字型族六欄）。語意紀律（全欄統一）：
/// - `None`＝保持原宣告（patch 不碰該欄，XML 原樣保留）；
/// - `Some(值)`＝明確設定；
/// - 移除類值（`Some(false)`／`UnderlineKind::None`／`StrikeKind::None`／
///   `VertAlign::Baseline`／highlight alpha=0／空字串族名）＝明確移除，
///   寫出顯式關閉形態（如 `<w:b w:val="0"/>`），使級聯兜底不得再補回。
///   不引入未文件的第三態。
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
    /// 底線（K29e；`w:u/@w:val`；E-2b 共型後型別＝解析/發射端同型 [`UnderlineKind`]）。
    /// `Some(UnderlineKind::Off)`＝明確移除（寫 `<w:u w:val="none"/>`，級聯層
    /// `toggle_attr_on` 讀為 `Some(false)`）。`Some(Raw(_))`＝寫入端誠實拒絕
    /// （`apply_mutation` 入口 `XmlPatchRejected`，零位元組位移——引擎無從替
    /// 用戶編造未支援形態，Raw 字面直插 `w:val` 屬未驗證注入面）。
    #[serde(default)]
    pub underline: Option<UnderlineKind>,
    /// 刪除線（K29e；`w:strike`＋`w:dstrike`）。`Some(StrikeKind::None)`＝
    /// 明確移除（寫 `<w:strike w:val="0"/>`）。
    #[serde(default)]
    pub strike: Option<StrikeKind>,
    /// 上標／下標（K29e；`w:vertAlign/@w:val`）。型別直接沿用級聯層的
    /// [`quire_ooxml::VertAlign`]（單一真值源，不另造鏡像枚舉）。
    /// `Some(Baseline)`＝明確移除（寫 `<w:vertAlign w:val="baseline"/>`，
    /// 熄滅字元樣式鏈宣告的上/下標）。
    #[serde(default)]
    pub vert_align: Option<quire_ooxml::VertAlign>,
    /// 文字背景高亮（K29e）。取捨（ECMA-376 §17.3.2.15）：`w:highlight/@w:val`
    /// 只能表達 16 個**命名色**枚舉；`Some(c)` 命中命名色表時寫
    /// `<w:highlight w:val="yellow"/>`（Word 高亮 UI 的互通形態），
    /// 非命名色走 `<w:shd w:val="clear" w:fill="RRGGBB"/>` 路徑（底紋承載
    /// 任意 RGB；LO/Word 兩者都繪成文字底方塊，本引擎解析層亦同欄消費）。
    /// 移除口徑：`Some(0x00RRGGBB)`（alpha=0）＝明確移除，寫
    /// `<w:highlight w:val="none"/>`——注意解析層既有口徑（styles.rs
    /// `apply_run_tag` highlight 臂）把 `none` 老實退回「未宣告」而非第四態
    /// 「明確關閉」：直接層移除後若**樣式鏈**另宣告高亮，樣式級高亮會重新
    /// 顯形（corpus 無此疊加實例，引擎不虛構第四態）。
    #[serde(default)]
    pub highlight: Option<u32>,
    /// 字型族（K29e；`w:rFonts`）。同步策略：`w:ascii`／`w:hAnsi`／`w:cs`／
    /// `w:eastAsia` 四欄一律寫同一族名——編輯器字型選取是「整包選族」語義
    /// （Word 字體對話框對 Latin＋Complex 整套生效）；逐欄精修混合腳本宣告
    /// 留給後續卡。空字串＝明確移除直接層宣告（`w:rFonts` 各欄寫空值＝
    /// 解析層不材料化，文字落回樣式級聯的字型宣告）。
    /// 斷行測寬與發射宣告是一體契約：寫入的是 `w:rFonts` 真宣告，重解析後
    /// 級聯烘進 `effective_fonts`，斷行測寬（`face_advance_scale`）與 SVG
    /// `font-family` 宣告同吃同一族——宣告了就用對應的水平尺，不會拉寬越欄。
    #[serde(default)]
    pub font_family: Option<String>,
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

/// 表格邊框單邊描述（E 卡／M4.6；語義對齊 OOXML `w:tblBorders`／`w:tcBorders`
/// 的子元素 `w:top`／`w:left`／…，ECMA-376 §17.4.10）。
///
/// 欄位口徑（與 [`quire_ooxml::Border`] 一一對應，原字串保留＝無損）：
/// - `style`＝`@w:val`（single／dashed／dotted／thick／double／none／nil／hidden…）；
/// - `width_pt`＝**渲染語義寬（pt）**，寫回換算 `@w:sz = width_pt × 8`（八分之一點）
///   並四捨五入取整；解析側既有口徑（`quire_layout::table::solve_pen`）即
///   `sz/8`、下限 0.25pt，本欄取其反函數，寫入→讀回零位移；
/// - `space_pt`＝`@w:space`（**pt**，直接取整寫回）。單位口徑＝本庫既有
///   `txbx_borders.space_pt` 同款 pt 約定；**無渲染消費者**——LO 25.8 探針
///   （`scripts/e_tblborder_space_probe.py`，table 級 space=0／12／240 三態）
///   實測框線與文字基線零位移，本庫解析側也只收錄不消費
///   （`quire_ooxml::table::Border::space` 存原字串；唯一 space 消費者是
///   段邊框 `table_map.rs` 的 txbx_borders 路徑，與表格邊框無關）⇒ 本欄
///   定位＝寫入保真透傳（存檔不丟資訊），本卡不宣稱其渲染語義（記 docs §5）。
/// - `color`＝`@w:color`（RRGGBB／auto／theme 色名，原字串透傳）。
///
/// 移除語意（照 K29c 既有「宣告即贏」紀律）：`style = Some("none"|"nil"|"hidden")`
/// ＝顯式關閉該邊（寫出元素、解析層 `solve_pen` 回 None 不發射，直接層贏過
/// 樣式級聯）；`style = None`（整個 `BorderSpec` 傳 None）＝該邊未宣告、零擾動。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BorderSpec {
    /// `@w:val`：框線樣式；`none`／`nil`／`hidden`＝顯式關閉。
    #[serde(default)]
    pub style: Option<String>,
    /// 線寬（pt）；寫回 `@w:sz = round(width_pt × 8)`。
    #[serde(default)]
    pub width_pt: Option<f32>,
    /// 距文字間距（pt）；寫回 `@w:space`。
    #[serde(default)]
    pub space_pt: Option<f32>,
    /// `@w:color`：RRGGBB／auto／theme 色名（原字串）。
    #[serde(default)]
    pub color: Option<String>,
}

impl BorderSpec {
    /// 全欄位皆缺（=`<w:top/>` 形態：宣告了邊框元素但沒有任何值）。
    pub fn is_empty(&self) -> bool {
        self.style.is_none()
            && self.width_pt.is_none()
            && self.space_pt.is_none()
            && self.color.is_none()
    }
}

/// 一組表格邊框（E 卡／M4.6）。表格級對齊 `w:tblBorders` 六方位；儲存格級
/// （[`CellPropsPatch::borders`]）只用四方位——insideH／insideV 在 `w:tcBorders`
/// 非法（ECMA-376 §17.4.10 僅列 top/left/bottom/right/tl2br/tr2bl/start/end），
/// 傳了視為未宣告、零擾動。
///
/// 逐方位獨立：`Some(spec)`＝覆寫該邊；`None`＝該邊不動（保留原宣告／繼承）。
/// 只要任一方位為 `Some`，寫入層就重建整個 `w:tblBorders`／`w:tcBorders` 容器
/// （既有邊框元素整顆替換，未列方位＝缺位＝回到繼承），與 K29c 既有
/// 「欄位有值才動、remove-then-append」口徑一致。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableBordersPatch {
    /// `w:top`。
    #[serde(default)]
    pub top: Option<BorderSpec>,
    /// `w:left`。
    #[serde(default)]
    pub left: Option<BorderSpec>,
    /// `w:bottom`。
    #[serde(default)]
    pub bottom: Option<BorderSpec>,
    /// `w:right`。
    #[serde(default)]
    pub right: Option<BorderSpec>,
    /// `w:insideH`（僅表格級）。
    #[serde(default)]
    pub inside_h: Option<BorderSpec>,
    /// `w:insideV`（僅表格級）。
    #[serde(default)]
    pub inside_v: Option<BorderSpec>,
}

impl TableBordersPatch {
    /// 六方位全缺＝整組未宣告（零擾動；序列化側可據此跳過欄位）。
    pub fn is_empty(&self) -> bool {
        [
            self.top.as_ref(),
            self.left.as_ref(),
            self.bottom.as_ref(),
            self.right.as_ref(),
            self.inside_h.as_ref(),
            self.inside_v.as_ref(),
        ]
        .into_iter()
        .all(|b| b.is_none())
    }

    /// 只保留四方位（儲存格級用：`w:tcBorders` 無 insideH／insideV）。
    pub(crate) fn cell_scope(mut self) -> Self {
        self.inside_h = None;
        self.inside_v = None;
        self
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
    /// E 卡：`w:tblBorders` 六方位（四邊＋insideH／insideV）。
    /// `None`＝零擾動（既有檔案逐位元組不變）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borders: Option<TableBordersPatch>,
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
    /// E 卡：`w:tcBorders` 四方位（top／left／bottom／right；insideH／insideV
    /// 在 tcBorders 非法，傳了＝未宣告、零擾動）。`None`＝零擾動。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borders: Option<TableBordersPatch>,
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
    InsertSectionBreak { para: usize, kind: SectionStartKind },
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
        self.num_fmt.is_none()
            && self.num_restart.is_none()
            && self.num_start.is_none()
            && self.pos.is_none()
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
    let (spans, total) = scan_paragraph_spans(xml, Some(target_para))?;
    let span = *spans.last().ok_or(MutationError::ParagraphOutOfBounds {
        para: target_para,
        len: total,
    })?;
    Ok((span, total))
}

/// 段落區間掃描的共用實作：**單趟**掃描 `xml`，收集扁平段落區間。
/// `stop_at＝Some(k)`＝湊齊 k＋1 段即停（呼叫端只要第 k 段，零多讀）；
/// `None`＝掃到檔尾（批次口徑，`paragraph_spans_all` 用）。
/// 回傳 `(已收集區間, 已見段落總數)`——後者是全文檔的段數計數器，
/// 與 `find_paragraph_span` 舊回傳的第二項逐字同義。
///
/// 抽出來是因為 `Engine::paragraph_cp_lens` 需要「一趟拿全部段」：舊寫法對
/// 每段各呼叫一次 `find_paragraph_span`＝O(段數 × 檔大小) 的二次掃描，
/// 12MB 級 document.xml（corpus/poi/test-data_document_bug65649）直接燒穿
/// 180 秒探針逾時。邏輯零分岔：本函數就是原 `find_paragraph_span` 的主體，
/// 只在停止條件上多一個 `None` 臂。
fn scan_paragraph_spans(
    xml: &[u8],
    stop_at: Option<usize>,
) -> Result<(Vec<ParagraphSpan>, usize), MutationError> {
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

    let mut spans: Vec<ParagraphSpan> = Vec::new();

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
                let span = ParagraphSpan {
                    start: p_start,
                    end,
                };
                match stop_at {
                    // 單段口徑：命中即停（原 find_paragraph_span 行為；回傳的
                    // 第二項因此＝「已見至該段為止」的計數，全庫呼叫端只有一處
                    // 用它做越界判定，語義不變）。
                    Some(k) if para_index == k => {
                        spans.push(span);
                        para_index += 1;
                        return Ok((spans, para_index));
                    }
                    // 批次口徑：一路收，檔尾才停。
                    None => spans.push(span),
                    Some(_) => {}
                }
                para_index += 1;
            }
        }
    }

    Ok((spans, para_index))
}

/// 單趟掃描全文，回傳**全部**扁平段落區間（索引＝扁平段落序）。
/// 與逐段呼叫 `find_paragraph_span` 的差別只有趟數：N 段從 N 趟變 1 趟。
/// 口徑（哪些 `w:p` 佔槽）與 `find_paragraph_span` 完全同源——同一支掃描器。
pub fn paragraph_spans_all(xml: &[u8]) -> (Vec<ParagraphSpan>, usize) {
    scan_paragraph_spans(xml, None).unwrap_or_else(|_| (Vec::new(), 0))
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
    // 名稱邊界守衛：`<w:rPr` 也會前綴配對 `<w:rPrChange`——名稱後必須是
    // 空白／`>`／`/`，否則不是本元素的開標籤。
    let boundary_ok = t[open.len()..]
        .chars()
        .next()
        .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/');
    if !boundary_ok {
        return String::new();
    }
    let gt = match t.find('>') {
        Some(g) => g,
        None => return String::new(),
    };
    if t[..gt].trim_end().ends_with('/') {
        return t[..=gt].to_string();
    }
    // 收尾改用**深度計數**，不可用「首個 `</{name}>`」：`w:rPrChange`（追蹤
    // 修訂，語料 n830205.docx 實測 589 處）本身內含一顆巢裝 `w:rPr`，首個
    // 閉標籤是**內層**的，在此截斷會吐出不平衡 XML。欄位級合併修正前這被
    // 「既有宣告全數丟掉」掩蓋（輸出恆為乾淨的 `<w:rPr>{add}`），合併落地後
    // 同一截斷直接變成 ill-formed document（E-1 燈 8 先紅實測）。
    // 取捨：不解析 XML 註解／CDATA，故註解內若字面出現同名標籤會誤計深度
    // （rPr／pPr 內無此寫法，語料零實例）。
    let local = name.rsplit(':').next().unwrap_or(name);
    let mut depth = 1usize;
    let mut cursor = gt + 1;
    while let Some(lt_rel) = t[cursor..].find('<') {
        let lt = cursor + lt_rel;
        let Some(ngt_rel) = t[lt..].find('>') else {
            return String::new();
        };
        let ngt = lt + ngt_rel;
        let tag = &t[lt..=ngt];
        if tag_local(tag) == local {
            if tag.starts_with("</") {
                depth -= 1;
                if depth == 0 {
                    return t[..=ngt].to_string();
                }
            } else if !tag.ends_with("/>") {
                depth += 1;
            }
        }
        cursor = ngt + 1;
    }
    String::new()
}

fn strip_leading_element(inner: &str, name: &str) -> String {
    let head = leading_element(inner, name);
    if head.is_empty() {
        inner.to_string()
    } else {
        inner[head.len()..].to_string()
    }
}

/// E-1（欄位級合併根因修正）：取出元素 `<{name}…>…</{name}>` 的**內文明文**。
///
/// 與 [`strip_leading_element`] 的差別是決定性的：後者的語義是「把開頭的
/// 元素摘掉、保留**其餘**」——拆段路徑（`split_paragraph_xml`）傳進來的是
/// 段落內文（pPr＋正文），摘掉 pPr 留正文正確；但 `patch_rpr`／`patch_ppr`
/// 傳進來的是那顆元素**本身**（`leading_element` 或 `analyze_paragraph` 切片
/// 的產物），「其餘」恆為空字串 ⇒ 每次重組都把既有宣告全部丟掉、只寫回本次
/// 補丁的欄位，分次單欄呼叫因此互相覆蓋（先點底線再點粗體 → 底線蒸發）。
/// 重組路徑要的是本函式的「取內文」語義。
///
/// 自閉合形態（`<w:rPr/>`）回空字串；病態輸入（找不到開／閉標籤）老實回空
/// 字串＝退回「無既有宣告」，不虛構內容。
///
/// 閉標籤以**深度計數**定位，不可用 `find`／`rfind` 首尾錨：`w:rPrChange`／
/// `w:pPrChange` 內含巢裝同族（`<w:pPrChange><w:pPr>…</w:pPr></w:pPrChange>`），
/// 首個 `</w:pPr>` 是**內層**的（`find` 截斷），末個亦可能在畸形輸入裡落到
/// 巢裝之外（`rfind` 越界，E-1 燈 8 實測）。深度計數對兩種失配都免疫，且
/// 不要求呼叫端保證 `el` 已是平衡元素。
///
/// 已知取捨（與本檔既有字串掃描口徑一致，語料零實例）：不解析 XML 註解／
/// CDATA，故註解內若字面出現同名標籤會誤計深度。
fn element_inner(el: &str, name: &str) -> String {
    let open = format!("<{name}");
    // 游標式續掃（同 `remove_element` 的假配對紀律）：首個 `<w:rPr` 可能是
    // `<w:rPrChange` 這類同前綴假配對，必須越過它續找真開標籤，不可見非邊界
    // 即放棄整題（否則合法宣告被誤判成「無既有宣告」）。
    let mut cursor = 0usize;
    let (rel, gt) = loop {
        let Some(rel) = el[cursor..].find(&open) else {
            return String::new();
        };
        let rel = cursor + rel;
        // 名稱邊界守衛（同 remove_element／leading_element 紀律）：`<w:rPr` 也會
        // 前綴配對 `<w:rPrChange`——名稱後必須是空白／`>`／`/`。
        let boundary_ok = el[rel + open.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/');
        let Some(gt_rel) = el[rel..].find('>') else {
            return String::new();
        };
        let gt = rel + gt_rel;
        if boundary_ok {
            break (rel, gt);
        }
        cursor = rel + open.len();
    };
    // 自閉合判定用**半開**區間 `rel..gt`（排除 `>`）：閉區間 `rel..=gt` 末位
    // 恆為 `>`，`ends_with('/')` 永假＝防線死碼（agy 第二意見實抓，對齊
    // leading_element `t[..gt]` 的既有口徑）。
    if el[rel..gt].trim_end().ends_with('/') {
        return String::new();
    }
    let local = name.rsplit(':').next().unwrap_or(name);
    let mut depth = 1usize;
    let mut cursor = gt + 1;
    while let Some(lt_rel) = el[cursor..].find('<') {
        let lt = cursor + lt_rel;
        let Some(ngt_rel) = el[lt..].find('>') else {
            return String::new();
        };
        let ngt = lt + ngt_rel;
        let tag = &el[lt..=ngt];
        if tag_local(tag) == local {
            if tag.starts_with("</") {
                depth -= 1;
                if depth == 0 {
                    // 平衡閉標籤的 `<` 之前＝本元素的內文。
                    return el[gt + 1..lt].to_string();
                }
            } else if !tag.ends_with("/>") {
                depth += 1;
            }
        }
        cursor = ngt + 1;
    }
    String::new()
}

/// 取元素 `<{name}…>` 的**開標籤明文**，並正規化成「非自閉合」形態。
///
/// 用途：`patch_ppr` 重組時要沿用原開標籤，不得把帶屬性的 `<w:pPr pt14:Unid=
/// "…">`（語料 400 檔實測 215 處）正規化成裸 `<w:pPr>` 而把廠商擴充屬性洗掉。
/// 自閉合 `<w:pPr/>` 的產物去掉尾斜棒 → `<w:pPr>`（重組必寫成成對形態，內容
/// 由呼叫端給）。找不到／名稱邊界不符（`<w:pPrChange`）＝`None`。
fn element_open_tag(el: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    // 游標式續掃（同 element_inner／remove_element 假配對紀律）：越過
    // `<w:pPrChange` 這類同前綴假配對續找真開標籤。
    let mut cursor = 0usize;
    loop {
        let rel = el[cursor..].find(&open)? + cursor;
        let boundary_ok = el[rel + open.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/');
        let gt = rel + el[rel..].find('>')?;
        if boundary_ok {
            let tag = &el[rel..=gt];
            return Some(match tag.trim_end().strip_suffix("/>") {
                // 自閉合：去掉 `/>` 換成 `>`（屬性等其餘部分逐字保留）。
                Some(head) => format!("{head}>"),
                None => tag.to_string(),
            });
        }
        cursor = rel + open.len();
    }
}

/// `w:rPr` 子元素的 ECMA-376 §17.3.2.27（CT_RPr／EG_RPrBase）宣告序。
///
/// 鍵名是 [`tag_local`] 的產出＝**已去命名空間前綴**（`<w:bCs/>` → `bCs`）；
/// 寫成 `w:bCs` 永遠查不中（實測踩過：全線查不中 ⇒ 排序退化成插入序，
/// b→bCs→i→iCs 序守不住）。未知元素名回 `None`（由 [`merge_rpr_children`]
/// 以「隨前件」規則安置）。
fn rpr_child_rank(name: &str) -> Option<i32> {
    let rank = match name {
        "rStyle" => 0,
        "rFonts" => 1,
        "b" => 2,
        "bCs" => 3,
        "i" => 4,
        "iCs" => 5,
        "caps" => 6,
        "smallCaps" => 7,
        "strike" => 8,
        "dstrike" => 9,
        "outline" => 10,
        "shadow" => 11,
        "emboss" => 12,
        "imprint" => 13,
        "noProof" => 14,
        "snapToGrid" => 15,
        "vanish" => 16,
        "webHidden" => 17,
        "color" => 18,
        "spacing" => 19,
        "w" => 20,
        "kern" => 21,
        "position" => 22,
        "sz" => 23,
        "szCs" => 24,
        "highlight" => 25,
        "u" => 26,
        "effect" => 27,
        "bdr" => 28,
        "shd" => 29,
        "fitText" => 30,
        "vertAlign" => 31,
        "rtl" => 32,
        "cs" => 33,
        "em" => 34,
        "lang" => 35,
        "eastAsianLayout" => 36,
        "specVanish" => 37,
        "oMath" => 38,
        // CT_RPr := EG_RPrBase*, rPrChange?——追蹤修訂標記恆在末位（語料
        // l3_revisions 有實例）。若不給它高位，補丁新增 lang(35) 之類的欄位
        // 會把它擠到中間，破 schema 尾段序。
        "rPrChange" => 1000,
        _ => return None,
    };
    Some(rank)
}

/// 把元素內文切成頂層子元素明文序列。每個 token 由「元素間的雜訊（換行／
/// 縮排）＋該元素明文」組成，深度計數處理巢狀容器，不假設子元素自閉合，
/// 且**不遺失任何位元組**——這是「未觸碰宣告逐字元原樣」無損口徑的依據。
fn top_level_children(inner: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut depth = 0usize;
    let mut cursor = 0usize;
    while cursor < inner.len() {
        let Some(lt_rel) = inner[cursor..].find('<') else {
            buf.push_str(&inner[cursor..]);
            break;
        };
        let lt = cursor + lt_rel;
        let Some(gt_rel) = inner[lt..].find('>') else {
            buf.push_str(&inner[cursor..]);
            break;
        };
        let gt = lt + gt_rel;
        let tag = &inner[lt..=gt];
        buf.push_str(&inner[cursor..=gt]);
        cursor = gt + 1;
        if tag.starts_with("</") {
            depth = depth.saturating_sub(1);
        } else if !tag.ends_with("/>") && !tag.starts_with("<!") && !tag.starts_with("<?") {
            depth += 1;
        }
        if depth == 0 {
            out.push(std::mem::take(&mut buf));
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

/// E-1 併入器：既有宣告（`preserved`）＋補丁新宣告（`added`）→ 依 CT_RPr
/// 宣告序重組的 rPr 內文。規則見 [`merge_children_ordered`]。
fn merge_rpr_children(preserved: &str, added: &str) -> String {
    merge_children_ordered(preserved, added, &[], rpr_child_rank)
}

/// 取 token 的**開標籤明文**（`<w:numPr …>`）。跳過前導雜訊，並排除閉標籤／
/// 註解／宣告處理指令（那些不是「一個子宣告的開始」）。
fn token_open_tag(token: &str) -> Option<&str> {
    let lt = token.find('<')?;
    let gt = token[lt..].find('>')?;
    let tag = &token[lt..=lt + gt];
    if tag.starts_with("</") || tag.starts_with("<!") || tag.starts_with("<?") {
        return None;
    }
    Some(tag)
}

/// 從開標籤明文取**全名**（含命名空間前綴，如 `w:numPr`）。與 [`tag_local`]
/// 的差別是本函式**保留**前綴——段落側併入器要靠全名做「同族剔除」的比對鍵，
/// 比 `b`／`bCs` 這類同前綴兄弟名時不會誤傷。
fn tag_full_name(tag: &str) -> &str {
    debug_assert!(tag.starts_with('<') && tag.ends_with('>'));
    let inner = tag[1..tag.len() - 1].trim_end_matches('/');
    inner.split_whitespace().next().unwrap_or("")
}

/// 取 token 開標籤的**全名**（含命名空間前綴，如 `w:numPr`）。
fn token_tag_name(token: &str) -> Option<&str> {
    token_open_tag(token).map(tag_full_name)
}

/// `w:pPr` 子元素的 ECMA-376 §17.3.1.26（CT_PPr／EG_PPrBase）宣告序。
///
/// **不可複用 [`rpr_child_rank`]**：兩張表鍵名重疊（`snapToGrid`／`shd`／
/// `spacing`／`bidi`／`rPr`…）但序不同——`snapToGrid` 在 rPr 是第 15、在 pPr
/// 是第 20（`adjustRightInd` 之後）；把 pPr 拿去吃 rPr 表會把 `spacing` 排到
/// `jc` 之後、`shd` 排到 `tabs` 之前，破 schema 序（Word 直接判文件毀損）。
/// 鍵名同樣是 [`tag_local`] 產出＝**已去命名空間前綴**。未知元素回 `None`
/// （由 [`merge_children_ordered`] 以「隨前件」規則安置）。
fn ppr_child_rank(name: &str) -> Option<i32> {
    let rank = match name {
        "pStyle" => 0,
        "keepNext" => 1,
        "keepLines" => 2,
        "pageBreakBefore" => 3,
        "framePr" => 4,
        "widowControl" => 5,
        "numPr" => 6,
        "suppressLineNumbers" => 7,
        "pBdr" => 8,
        "shd" => 9,
        "tabs" => 10,
        "suppressAutoHyphens" => 11,
        "kinsoku" => 12,
        "wordWrap" => 13,
        "overflowPunct" => 14,
        "topLinePunct" => 15,
        "autoSpaceDE" => 16,
        "autoSpaceDN" => 17,
        "bidi" => 18,
        "adjustRightInd" => 19,
        "snapToGrid" => 20,
        "spacing" => 21,
        "ind" => 22,
        "contextualSpacing" => 23,
        "mirrorIndents" => 24,
        "suppressOverlap" => 25,
        "jc" => 26,
        "textDirection" => 27,
        "textAlignment" => 28,
        "textboxTightWrap" => 29,
        "outlineLvl" => 30,
        "divId" => 31,
        "cnfStyle" => 32,
        // 段落標記 `rPr`／段內分節 `sectPr` 恆在 EG_PPrBase 之後（CT_PPr 尾段）。
        "rPr" => 33,
        "sectPr" => 34,
        // `pPrChange` 同 `rPrChange`：追蹤修訂標記恆在末位，高位釘死，不給它
        // 高位會被補丁新增的 `jc`／`spacing` 擠進中間，破 schema 尾段序。
        "pPrChange" => 1000,
        _ => return None,
    };
    Some(rank)
}

/// 宣告序併入器的共用核心（`merge_rpr_children` 與段落側同吃一份，紀律一致）。
///
/// 規則：
/// - 每個子元素 token 依其開標籤取 rank；**未知元素**隨前件（沿用前一個已知
///   token 的 rank）；尚未出現任何已知元素前的未知 token 取 rank 0——不可取
///   −1：那會把未知擴展元素（如 `w14:` 系）**提調**到首位宣告之前（agy 第二
///   意見實抓）。
/// - 隨前件的 rank 由「前件」定義，**與前件是否被剔除無關**：被同族覆寫剔除
///   的宣告仍定義其後未知元素的鄰域（agy 第二意見 P1 實抓：先 `continue` 再
///   記 rank 會讓未知元素斷鏈往前漂）。
/// - 同 rank 以「補丁新宣告在前、既有宣告在後」排列，兩側內部各保穩定序——
///   新 `w:pStyle`／`w:rStyle`（rank 0）必須贏過「隨前件漂到 rank 0」的既有
///   未知元素，否則首位宣告契約破防（agy 第二意見 P2 實抓）。
/// - 未觸碰的宣告逐字元原樣（含元素間空白），零正規化重寫。
///
/// 已知取捨（與本檔既有字串掃描口徑一致，語料零實例）：掃描 `>` 時不識別
/// 屬性值內字面 `>` 與 XML 註解內 `>`——真需要時應整檔改走事件式解析器，
/// 不在本卡範圍。
fn merge_children_ordered(
    preserved: &str,
    added: &str,
    drop: &[&str],
    rank_of: fn(&str) -> Option<i32>,
) -> String {
    let mut ranked: Vec<(i32, bool, String)> = Vec::new();
    for (is_add, src) in [(false, preserved), (true, added)] {
        let mut last_rank = 0i32;
        for token in top_level_children(src) {
            let dropped = !is_add
                && drop
                    .iter()
                    .any(|name| token_tag_name(&token) == Some(*name));
            // rank 必須在剔除判定**之前**更新：被覆寫的舊宣告仍是其後未知元素
            // 的前件（隨前件鄰域由原文位置決定，不由存活與否決定）。
            let rank = match token_open_tag(&token).and_then(|tag| rank_of(tag_local(tag))) {
                Some(r) => {
                    last_rank = r;
                    r
                }
                None => last_rank,
            };
            if dropped {
                continue;
            }
            ranked.push((rank, is_add, token));
        }
    }
    // 穩定排序：同 rank 時補丁新宣告（is_add=true）在前、既有宣告在後；兩側
    // 內部保持插入序。
    ranked.sort_by_key(|(rank, is_add, _)| (*rank, !*is_add));
    ranked.into_iter().map(|(_, _, token)| token).collect()
}

/// 取元素內文裡首個**頂層**同族子元素的明文（含元素前雜訊）；無則 `None`。
fn top_level_element(inner: &str, name: &str) -> Option<String> {
    top_level_children(inner)
        .into_iter()
        .find(|token| token_tag_name(token) == Some(name))
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

/// SplitParagraph 回塞鎖（t_e93fb47b，比照 `E2B_UNDERLINE_BUGGY` 口徑）：
/// `SPLIT_REV_BUGGY=1` 把本卡「切點容器配對補全」與「pPr 單份複製」兩條線
/// 退回修前形態（該翻紅的盞別實錄見 `tests/e_split_revision_containers.rs`
/// 檔頭；env 鎖零改檔，禁 checkout/reset 鐵律天然滿足）。
fn split_rev_buggy() -> bool {
    std::env::var_os("SPLIT_REV_BUGGY").is_some()
}

/// 開標籤（非閉、非自閉合）的完整元素名（`w:fldSimple` 之類）；否則 `None`。
fn open_tag_full_name(tag: &str) -> Option<&str> {
    if !tag.starts_with('<') || tag.starts_with("</") || tag.ends_with("/>") {
        return None;
    }
    tag[1..]
        .split_whitespace()
        .next()
        .map(|name| name.trim_end_matches('>'))
        .filter(|name| !name.is_empty())
}

/// 掃描 `[from, to)`，回傳「截至 `to` 仍未閉合」容器的**完整開標籤字串**
/// （陣列序＝由外而內，堆疊頂＝最內層）。
///
/// 配對規矩（卡面鐵律：不得假設上游配對完好）：閉標籤與堆疊中任一同名項
/// 配對 → 剔除該項（頂層同名最優）；堆疊無同名（上游容器跨段起點的孤兒閉）
/// → 容忍略過。自閉合標籤永不進堆疊。
///
/// 取捨（比照 `leading_element` 既有註明）：不解析 XML 註解／CDATA——註解
/// 內字面出現同名標籤會誤計深度（語料零實例）。
fn open_containers_at(text: &str, from: usize, to: usize) -> Vec<String> {
    let mut stack: Vec<String> = Vec::new();
    let mut cursor = from;
    while cursor < to {
        let Some(lt_rel) = text[cursor..to].find('<') else {
            break;
        };
        let lt = cursor + lt_rel;
        let Some(gt_rel) = text[lt..to].find('>') else {
            break;
        };
        let gt = lt + gt_rel;
        let tag = &text[lt..=gt];
        cursor = gt + 1;
        // XML 宣告／處理指令／註解／CDATA：不進標籤堆疊（`<!`／`<?` 前綴）。
        if tag.starts_with("<!") || tag.starts_with("<?") {
            continue;
        }
        if open_tag_full_name(tag).is_some() {
            stack.push(tag.to_string());
        } else if tag.starts_with("</") {
            let local = tag_local(tag);
            if let Some(idx) = stack.iter().rposition(|open| tag_local(open) == local) {
                stack.remove(idx);
            }
        }
    }
    stack
}

/// 由開標籤字串導出配對閉標籤（`<w:hyperlink r:id="rId10">` → `</w:hyperlink>`）。
fn close_tag_for(open_tag: &str) -> String {
    let name = open_tag_full_name(open_tag).unwrap_or("");
    format!("</{name}>")
}

/// 自段落正文起點 `from` 掃到**頂層**（堆疊空＝直接掛在 `w:p` 下）首顆
/// `w:pPr` 元素的結束偏移（含自閉合空 pPr）；無頂層 pPr ⇒ `None`。
///
/// 不可直接用 `ParagraphAnalysis::ppr_end_offset`：該欄是整段切片**最後**一個
/// `</w:pPr>`，段內含 txbxContent／Fallback 巢裝段（WordWithAttachments para 4
/// 實證）時會落在深處，以其為正文起點＝把整塊 txbx 內容劃進 pPr 前綴。
fn first_top_level_ppr_end(text: &str, from: usize, to: usize) -> Option<usize> {
    let mut cursor = from;
    let mut depth = 0usize;
    while cursor < to {
        let Some(lt_rel) = text[cursor..to].find('<') else {
            break;
        };
        let lt = cursor + lt_rel;
        let Some(gt_rel) = text[lt..to].find('>') else {
            break;
        };
        let gt = lt + gt_rel;
        let tag = &text[lt..=gt];
        cursor = gt + 1;
        if tag.starts_with("<!") || tag.starts_with("<?") {
            continue;
        }
        if tag.starts_with("</") {
            depth = depth.saturating_sub(1);
        } else if open_tag_full_name(tag).is_some() {
            let at_top = depth == 0;
            depth += 1;
            if at_top && tag_local(tag) == "pPr" {
                // 頂層 pPr 開標籤：找它自己的閉標籤結束（深度計數，pPr 內
                // 可合法巢 rPr／pPrChange 等異名，同名只可能來自 pPrChange
                // 內再裝 pPr 的病態形態——深度計數天然覆蓋）。
                let mut inner_depth = 1usize;
                let mut sc = gt + 1;
                while sc < to {
                    let Some(l2) = text[sc..to].find('<') else {
                        break;
                    };
                    let l2 = sc + l2;
                    let Some(g2) = text[l2..to].find('>') else {
                        break;
                    };
                    let g2 = l2 + g2;
                    let t2 = &text[l2..=g2];
                    sc = g2 + 1;
                    if t2.starts_with("<!") || t2.starts_with("<?") {
                        continue;
                    }
                    if t2.starts_with("</") {
                        inner_depth -= 1;
                        if inner_depth == 0 {
                            return Some(g2 + 1);
                        }
                    } else if open_tag_full_name(t2).is_some() {
                        inner_depth += 1;
                    }
                }
                // 頂層 pPr 未閉＝上游已病態，保守回 None（維持修前 pPr 前綴
                // 口徑，不憑空切半顆 pPr）。
                return None;
            }
        } else if depth == 0 && tag_local(tag) == "pPr" {
            // 自閉合 `<w:pPr/>`（或帶屬性的空宣告）：本身即頂層 pPr 全元素。
            return Some(gt + 1);
        }
    }
    None
}

/// 段切片「段內開啟者必須段內閉合」良構檢查：跨段邊界的上游容器其開／閉
/// 在切片之外——孤兒閉標籤屬合法（容忍），但切片掃到末尾仍有未閉開標籤＝
/// `expected </w:ins> but </w:p>` 族 ill-formed 形態，拒絕在寫入前（零位元
/// 組位移，比照 `reject_unwritable_underline` 「拒絕＝零位元組位移」契約）。
fn paragraph_slice_balanced(slice: &str) -> bool {
    open_containers_at(slice, 0, slice.len()).is_empty()
}

/// 將單一段落在 `cp` 處拆成兩段（回傳兩段完整 `<w:p>` XML）。
///
/// 跨切點容器配對（t_e93fb47b 主修）：切點落在段內行內容器（`w:ins`／
/// `w:del`／`w:hyperlink`／`w:fldSimple`…）包裹的 run 內部時，舊實作只斷
/// run 文字流——第一段把切點之前開了沒關的容器起點原樣留在段尾直接接
/// `</w:p>`（ill-formed，存檔整批失敗），第二段頭留孤兒閉標籤。修法＝切點
/// 之前仍未閉合的容器，第一段尾**由內而外**補閉標籤、第二段頭**由外而內**
/// 補同名開標籤（開標籤逐字複製，屬性全保留＝不丟追蹤標記），兩側配對
/// 完備後文字流與 canonical 樹逐字守恆。
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
    let p_close = text[analysis.p_close_start_offset..p_span.end].to_string();
    let content_start = analysis.p_open_end_offset;
    let content_end = analysis.p_close_start_offset;
    // pPr 不複製＋不越界線（燈 R1，雙份 pPr 缺陷實錄＝Jupiter cp=3 態）：
    // 舊寫法 `ppr` 取到 `analysis.ppr_end_offset`（整段切片**最後**一個
    // `</w:pPr>`）而正文起點卻從 `<w:p>` 尾算 ⇒ ①段內有頂層 pPr 時左段吃到
    // 兩顆 pPr（違反 CT_P 單 pPr 契約）；②段內含 txbxContent／Fallback 巢裝
    // 段時（WordWithAttachments para 4 實證）該偏移落在深處，整塊圖形內容被
    // 劃進「pPr 前綴」而複製進兩半。好態一律以「首顆**頂層** pPr」為唯一
    // 事實來源：ppr 前綴與正文起點同源，兩者不可能重疊。
    let top_ppr_end = first_top_level_ppr_end(text, content_start, content_end);
    let (ppr, body_start) = if split_rev_buggy() {
        // 回塞態＝修前形態：pPr 前綴沿用 analysis 口徑、正文從 `<w:p>` 尾起切。
        let legacy_ppr = analysis
            .ppr_end_offset
            .map(|e| text[content_start..e].to_string())
            .unwrap_or_default();
        (legacy_ppr, content_start)
    } else {
        (
            top_ppr_end
                .map(|e| text[content_start..e].to_string())
                .unwrap_or_default(),
            top_ppr_end.unwrap_or(content_start),
        )
    };

    let mut left_content;
    let mut right_content;
    // 切點絕對偏移（容器鏈在該點求值；各臂各恰好賦值一次）。
    let cut;
    if cp == 0 {
        left_content = String::new();
        right_content = text[body_start..content_end].to_string();
        cut = body_start;
    } else if cp >= analysis.total_cp {
        left_content = text[body_start..content_end].to_string();
        right_content = String::new();
        cut = content_end;
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
                // 病態上遊守衛：CT_P 契約 pPr 必須是 w:p 首個子元素，若頂層
                // pPr 竟排在 run 之後（run.start < body_start）則切片會反向
                // panic——誠實拒絕（零位元組位移），不容錯位切片進 parts。
                if run.start < body_start {
                    return Err(MutationError::XmlPatchRejected {
                        detail: "段落結構違反 CT_P（pPr 非首個子元素），拆段拒絕".to_string(),
                    });
                }
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
                    &text[body_start..run.start],
                    run_open,
                    left_inner
                );
                right_content = format!(
                    "{}{}</w:r>{}",
                    run_open,
                    right_inner,
                    &text[run.end..content_end]
                );
                cut = run.start;
            }
            None => {
                let split_abs = analysis
                    .text_nodes
                    .iter()
                    .find(|n| cp <= n.start_cp)
                    .map(|n| n.node_span.start)
                    .unwrap_or(content_end)
                    .max(body_start);
                left_content = text[body_start..split_abs].to_string();
                right_content = text[split_abs..content_end].to_string();
                cut = split_abs;
            }
        }
    }

    // 跨切點容器配對補全（回塞態跳過＝鏡像修前「只斷文字流」形態）。
    if !split_rev_buggy() {
        let chain = open_containers_at(text, body_start, cut);
        if !chain.is_empty() {
            for open in chain.iter().rev() {
                left_content.push_str(&close_tag_for(open));
            }
            let mut head = String::new();
            for open in &chain {
                head.push_str(open);
            }
            right_content = format!("{head}{right_content}");
        }
    }

    let left = format!("{p_open}{ppr}{left_content}{p_close}");
    let right = format!("{p_open}{}{right_content}{p_close}", strip_sectpr(&ppr));
    // 良構閘門（回塞態不設＝修前無從拒絕的語義）：兩半各自必須配對收斂，
    // 未收斂＝誠實拒絕、零位元組位移，不容 ill-formed 落進 parts。
    if !split_rev_buggy() && !(paragraph_slice_balanced(&left) && paragraph_slice_balanced(&right))
    {
        return Err(MutationError::XmlPatchRejected {
            detail: "拆段後段切片容器配對未收斂（段內開啟的標籤未在段內閉合），\
                     拒絕本次拆段且零位元組位移"
                .to_string(),
        });
    }
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
    // 游標式掃描（而非每輪從 0 重找）：名字有同前綴兄弟的元素——`w:bCs` 對
    // `w:b`、`w:iCs` 對 `w:i`、`w:szCs` 對 `w:sz`（語料實測 bCs 258、szCs
    // 2455 處）——第一次命中是假配對，必須**跳到下一處**續掃。舊實作在這裡
    // `break`＝整輪放棄，同族舊宣告因此逃過清理、與補丁新宣告並存成重複宣告
    // （`<w:b/>`＋`<w:b w:val="0"/>`，覆寫失效）。欄位級合併讓既有宣告真正
    // 進入重組路徑後，這塊原本走不到的死路徑變成活路徑，故一併修正。
    let mut cursor = 0usize;
    while let Some(rel) = out[cursor..].find(&open) {
        let start = cursor + rel;
        let after_ok = out[start + open.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/');
        if !after_ok {
            // 假配對（同前綴兄弟名）：越過本處開標籤前綴續找，不放棄整輪。
            cursor = start + open.len();
            continue;
        }
        let Some(gt_rel) = out[start..].find('>') else {
            break;
        };
        let gt = start + gt_rel;
        if out[start..gt].trim_end().ends_with('/') {
            out.replace_range(start..=gt, "");
            cursor = start;
            continue;
        }
        let close = format!("</{name}>");
        let Some(end_rel) = out[gt..].find(&close) else {
            break;
        };
        let end = gt + end_rel + close.len();
        out.replace_range(start..end, "");
        cursor = start;
    }
    out
}

/// E-2b 共型：底線寫入端誠實閘門——`Underline::Raw(_)` 拒絕。
///
/// 定案（卡面任務 1，採「寫入端拒絕」建議案）：寫入端無從替用戶編造未支援
/// 形態（Raw 的繪製配方連發射端都還是 TODO），且 Raw 原字面直接插值進
/// `w:u/@w:val` 是未驗證字串注入面（`"/></w:rPr><w:b/>` 之流可竄改 rPr 結構）。
/// 故 Raw 只允許**讀取端**保形（wavy/words 從檔讀進來、發射端降級單線，E-2
/// 既有口徑），寫出路徑一律誠實報錯。拒絕點在 `apply_mutation` 入口（任何
/// XML 觸碰之前）＝零位元組位移；`patch_rpr` 寫出臂再以 `as_u_val()` 回 None
/// 為二道防線（型別層出口，防未來繞過入口的新呼叫點）。
/// BUGGY 回塞態（`E2B_UNDERLINE_BUGGY=1`）：`as_u_val()` 把 Raw 折成單線，
/// 本閘門同款放行（＝修前鏡像枚舉根本沒有 Raw、無從拒絕的語義），考卷燈 3
/// 據此鑑別回紅。
fn reject_unwritable_underline(patch: &RunDirectPatch) -> Result<(), MutationError> {
    if let Some(UnderlineKind::Raw(raw)) = &patch.underline {
        // 回塞態與 `as_u_val` 同口徑：Raw 放行降級單線（K29e 鏡像枚舉時代行為）。
        if std::env::var_os("E2B_UNDERLINE_BUGGY").is_some() {
            return Ok(());
        }
        return Err(MutationError::XmlPatchRejected {
            detail: format!(
                "底線形態 Raw(\"{raw}\") 寫入端不支援：引擎無從替用戶編造未支援形態\
                 （ECMA ST_UnderlineValues 未支援值僅供讀取端保形）。可寫形態＝\
                 single/double/thick/dotted/dashed/off。"
            ),
        });
    }
    Ok(())
}

fn patch_rpr(rpr: &str, patch: &RunDirectPatch) -> String {
    // [K29B_BUGGY_POINT_2: RPR_PATCH]
    let buggy_rpr = false;
    if buggy_rpr {
        return rpr.to_string();
    }
    // [K29E_BUGGY_POINT: SIX_FIELDS]——BUGGY 回塞總開關（scripts/k29e_buggy_replay.sh
    // 以字串置換翻轉本旗標）：true＝六欄寫入路徑整體退回「舊五欄版」——remove
    // 不碰六族、add 不寫六欄，且全 None 硬閂失效（空 rPr 被材料化）。同款於
    // buggy_rpr 的先例紀律：單行翻轉、可完全逆轉、good 態以 git diff 對照 HEAD。
    let buggy_k29e_fields = false;
    // 卡面紀律「寫入路徑只在 patch 明確指定時才動 XML」的硬閂：全 None patch
    // ＝零意圖，連既有 rPr 都不重排（原字串原樣返回，零注入、零位元組位移）。
    if !buggy_k29e_fields && patch == &RunDirectPatch::default() {
        return rpr.to_string();
    }
    // BUGGY 態把六欄澆成 None＝六族 remove/add 臂全部不觸發（舊五欄版行為）；
    // 硬閂已在上一行被 buggy 態繞過，零 rPr 段將被材料化空 <w:rPr></w:rPr>
    // ——燈 2（byte-identity）與燈 1/3/5/6（六欄讀回）據此鑑別回紅。
    let mut patch = patch.clone();
    if buggy_k29e_fields {
        patch.underline = None;
        patch.strike = None;
        patch.vert_align = None;
        patch.highlight = None;
        patch.font_family = None;
    }
    // E-1 根因修正：取「既有宣告內文」必須用 `element_inner`（取元素**內文**），
    // 不可用 `strip_leading_element`（摘掉開頭元素、保留其餘）——後者對
    // `leading_element` 的產物恆回空字串，等于每次重組都把既有宣告全數丟掉，
    // 欄位 None 因此不是「不碰 XML」而是「連別人寫的也一起抹除」，分次單欄
    // 呼叫互相覆蓋（先點底線再點粗體 → 底線蒸發）。
    //
    // BUGGY 回塞態（`buggy_k29e_fields`）刻意重現舊缺陷語義＝取內文恆空 ⇒
    // 既有宣告全滅＋空 rPr 材料化：`scripts/k29e_buggy_replay.sh` 只翻轉旗標
    // 單行，靠本行重現「舊版把整顆 rPr 澆掉」的形態，故 k29e 燈 2（空 rPr
    // 材料化破 byte-identity）在 buggy 態照樣轉紅，鑑別力不受本次修正稀釋。
    let inner = if buggy_k29e_fields || rpr.is_empty() {
        String::new()
    } else {
        element_inner(rpr, "w:rPr")
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
    // K29e 六欄：紀律與既有五欄完全一致——欄位 Some＝先移除同族既有宣告
    // （含 w:dstrike 成對清理），再按明確值寫回；欄位 None＝不碰 XML。
    if patch.underline.is_some() {
        body = remove_element(&body, "w:u");
    }
    if patch.strike.is_some() {
        body = remove_element(&body, "w:strike");
        body = remove_element(&body, "w:dstrike");
    }
    if patch.vert_align.is_some() {
        body = remove_element(&body, "w:vertAlign");
    }
    if patch.highlight.is_some() {
        // highlight 與 shd 共用解析層同一落點（styles.rs 兩臂都寫
        // RunStyle.highlight），移除直格式時必須兩族一起清，否則殘留
        // w:shd 會在重解析後贏過新的 highlight 宣告。
        body = remove_element(&body, "w:highlight");
        body = remove_element(&body, "w:shd");
    }
    if patch.font_family.is_some() {
        body = remove_element(&body, "w:rFonts");
    }

    let mut add = String::new();
    if let Some(style) = &patch.char_style {
        add.push_str(&format!(r#"<w:rStyle w:val="{}"/>"#, xml_escape(style)));
    }
    // K29e：`w:rFonts` 緊接 rStyle（CT_RPr 宣告序）。空字串＝明確移除：
    // 各欄寫空值 → `rfonts_of` 逐欄 filter 空字串 → 全欄 None 且無 theme
    // 引用 → 不材料化直接宣告（不虛構空 RunFonts 遮罩樣式級聯）。
    if let Some(family) = &patch.font_family {
        if family.is_empty() {
            add.push_str(r#"<w:rFonts w:ascii="" w:hAnsi="" w:cs="" w:eastAsia=""/>"#);
        } else {
            let esc_family = xml_escape(family);
            add.push_str(&format!(
                r#"<w:rFonts w:ascii="{f}" w:hAnsi="{f}" w:cs="{f}" w:eastAsia="{f}"/>"#,
                f = esc_family
            ));
        }
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
    // K29e：刪除線（CT_RPr 序：strike 在 i 之後、color 之前）。
    if let Some(strike) = patch.strike {
        match strike {
            StrikeKind::Single => add.push_str("<w:strike/>"),
            // 雙線成對宣告（見 StrikeKind::Double 取捨註解）：strike 供現行
            // 解析器讀回，dstrike 供 Word/LO 繪雙線。
            StrikeKind::Double => add.push_str("<w:strike/><w:dstrike/>"),
            StrikeKind::None => add.push_str(r#"<w:strike w:val="0"/>"#),
        }
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
    // K29e：highlight（CT_RPr 序：sz 之後、u 之前）。alpha=0＝明確移除。
    if let Some(bg) = patch.highlight {
        if (bg >> 24) == 0 {
            add.push_str(r#"<w:highlight w:val="none"/>"#);
        } else if let Some(name) = highlight_named_color(bg & 0x00FF_FFFF) {
            add.push_str(&format!(r#"<w:highlight w:val="{name}"/>"#));
        } else {
            // 非命名色走 w:shd 底紋路徑（任意 RGB 承載；解析層 shd 臂與
            // highlight 臂同落 RunStyle.highlight，發射同為文字底方塊）。
            add.push_str(&format!(
                r#"<w:shd w:val="clear" w:color="auto" w:fill="{:06X}"/>"#,
                bg & 0x00FF_FFFF
            ));
        }
    }
    // K29e：底線（CT_RPr 序：highlight 之後）。E-2b 共型：`as_u_val()` 是檔契約
    // 字面唯一出口（render-trait 單一真值源）；回 None（Raw 且非回塞態）＝
    // **不寫**——二道防線，正常路徑已被 `apply_mutation` 入口閘門拒絕，這裡
    // 防的是未來繞過入口的新呼叫點把 Raw 字面直插 `w:val`（注入面）。
    if let Some(u) = patch.underline.as_ref() {
        if let Some(val) = u.as_u_val() {
            add.push_str(&format!(r#"<w:u w:val="{val}"/>"#));
        }
    }
    // K29e：上下標（CT_RPr 末位；EG_RPrBase 尾段 vertAlign 在 lang 之前）。
    if let Some(va) = patch.vert_align {
        add.push_str(&format!(
            r#"<w:vertAlign w:val="{}"/>"#,
            match va {
                quire_ooxml::VertAlign::Superscript => "superscript",
                quire_ooxml::VertAlign::Subscript => "subscript",
                quire_ooxml::VertAlign::Baseline => "baseline",
            }
        ));
    }
    // E-1：既有宣告與補丁新宣告依 CT_RPr 宣告序重組（非「舊宣告＋新宣告」
    // 直覺串接），未觸碰宣告逐字元原樣保留；rPr 全空時（body 空）輸出與
    // 修正前逐位元組相同（add 段本依宣告序建構，穩定排序恆等）。
    format!("<w:rPr>{}</w:rPr>", merge_rpr_children(&body, &add))
}

/// K29e：0xRRGGBB → ECMA-376 §17.3.2.15 十六命名色反查（命中寫
/// `<w:highlight w:val="名稱"/>` 與 Word 高亮 UI 互通；未命中回 None 走
/// `w:shd` 底紋路徑）。色表與 `quire_ooxml::styles::highlight_color_of`
/// 正向表逐色對齊（新增命名色時兩處都要動——命名色是封閉枚舉，漂移風險低）。
fn highlight_named_color(rgb: u32) -> Option<&'static str> {
    Some(match rgb {
        0x00_00_00 => "black",
        0x00_00_FF => "blue",
        0x00_FF_FF => "cyan",
        0x00_00_80 => "darkblue",
        0x00_80_80 => "darkcyan",
        0x80_80_80 => "darkgray",
        0x00_80_00 => "darkgreen",
        0x80_00_80 => "darkmagenta",
        0x80_00_00 => "darkred",
        0x80_80_00 => "darkyellow",
        0x00_FF_00 => "green",
        0xD3_D3_D3 => "lightgray",
        0xFF_00_FF => "magenta",
        0xFF_00_00 => "red",
        0xFF_FF_FF => "white",
        0xFF_FF_00 => "yellow",
        _ => return None,
    })
}

/// 段落補丁的既有宣告族剔除表：`Some` 欄位對應的頂層同族宣告，交由併入器在
/// **頂層**粒度剔除（巢裝 `w:pPrChange` 快照內的同族不被波及）。
fn ppr_drop_families(patch: &ParaPropsPatch) -> Vec<&'static str> {
    let mut drop = Vec::new();
    if patch.style_id.is_some() {
        drop.push("w:pStyle");
    }
    if patch.page_break_before.is_some() {
        drop.push("w:pageBreakBefore");
    }
    if patch.line_spacing.is_some() {
        drop.push("w:spacing");
    }
    if patch.alignment.is_some() {
        drop.push("w:jc");
    }
    drop
}

fn patch_ppr(ppr: &str, patch: &ParaPropsPatch) -> String {
    // E-1 同源的根因修正（本卡）：取「既有宣告內文」必須用 `element_inner`
    // （取元素**內文**），不可用 `strip_leading_element`（摘掉開頭元素、保留
    // 其餘）——後者對 `leading_element` 的產物（元素本身）恆回空字串，於是
    // 每次重組都把既有 pStyle／numPr／spacing／pageBreakBefore 全數丟掉，欄位
    // `None` 因此不是「不碰 XML」而是「連別人寫的也一起抹除」——設對齊 →
    // 段落樣式／清單編號／行距／分頁前全部消失（用戶可見形態）。
    //
    // 零意圖硬閂（同 `patch_rpr` 紀律）：全 `None` 補丁＝原字串原樣返回。零
    // pPr 段落因此不會被材料化成空 `<w:pPr></w:pPr>`（該形態是 k29d 拆分節
    // 「空段整節移除」判定的依賴項，不可無謂材料化），有 pPr 段落零位元組位移。
    if patch == &ParaPropsPatch::default() {
        return ppr.to_string();
    }
    let inner = if ppr.is_empty() {
        String::new()
    } else {
        element_inner(ppr, "w:pPr")
    };

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

    // 開標籤沿用原文明（帶廠商擴充屬性的 `<w:pPr pt14:Unid="…">` 語料實測
    // 215 處，不可被正規化成裸標籤）。
    let open = element_open_tag(ppr, "w:pPr").unwrap_or_else(|| "<w:pPr>".to_string());
    let drop = ppr_drop_families(patch);
    format!(
        "{open}{}</w:pPr>",
        merge_children_ordered(&inner, &add, &drop, ppr_child_rank)
    )
}

fn patch_numpr(ppr: &str, action: &ListAction) -> String {
    // 根因與 `patch_ppr` 同源（見彼處註解）：`strip_leading_element` 對
    // `leading_element` 產物恆回空字串 ⇒ 設清單編號時既有 pStyle／spacing／jc
    // 全數蒸發。取內文一律走 `element_inner`。
    let inner = if ppr.is_empty() {
        String::new()
    } else {
        element_inner(ppr, "w:pPr")
    };
    // numPr 只認**頂層**那一顆：`w:pPrChange` 快照裡也可能有一顆 numPr，全域
    // 字串掃描會把快照的 numId 誤當成本段現行值（ChangeLevel 因此沿用錯號）。
    let existing_numpr = top_level_element(&inner, "w:numPr");

    // 段頂本來就沒有 numPr ⇒ AttachExisting 以外都是零意圖：原字串原樣返回。
    // RemoveNumbering 無號可摘、ChangeLevel 無層可改——後者若照常重組會合成
    // `<w:numId w:val="0"/>` 的寡人宣告（agy 第二意見 P3 實抓），兩者的合法
    // 產物都是零位元組位移（不材料化空 pPr、不重排既有宣告）。
    if existing_numpr.is_none() && !matches!(action, ListAction::AttachExisting { .. }) {
        return ppr.to_string();
    }

    let add = match action {
        ListAction::AttachExisting { num_id, ilvl } => {
            format!(r#"<w:numPr><w:ilvl w:val="{ilvl}"/><w:numId w:val="{num_id}"/></w:numPr>"#)
        }
        ListAction::ChangeLevel { ilvl } => {
            let num = existing_numpr
                .as_deref()
                .and_then(extract_num_id)
                .unwrap_or(0);
            format!(r#"<w:numPr><w:ilvl w:val="{ilvl}"/><w:numId w:val="{num}"/></w:numPr>"#)
        }
        ListAction::RemoveNumbering => String::new(),
    };
    let open = element_open_tag(ppr, "w:pPr").unwrap_or_else(|| "<w:pPr>".to_string());
    format!(
        "{open}{}</w:pPr>",
        merge_children_ordered(&inner, &add, &["w:numPr"], ppr_child_rank)
    )
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
    /// 段落的**突變碼點空間**長度（與 `apply_mutation` 的 `total_cp` 同源：
    /// `analyze_paragraph` 口徑）。版面層 `paragraph_text` 長度可能分歧
    /// （TOC 前導點、欄代碼等只進其中一口徑），考卷腳本選 cp 前必查此值，
    /// 免得拿版面文字長度算出越界偏移。段不存在／無 document.xml → None。
    pub fn paragraph_cp_len(&self, para: usize) -> Option<usize> {
        let xml = self.doc.parts.get("word/document.xml")?;
        let (p_span, total) = crate::mutation::find_paragraph_span(xml, para).ok()?;
        if para >= total {
            return None;
        }
        Some(crate::mutation::analyze_paragraph(xml, p_span).total_cp)
    }

    /// 全部段落的突變碼點空間長度（索引＝扁平段落序，缺段以 None 補位）。
    /// **單趟**掃描版：`paragraph_cp_len` 逐段呼叫是 O(段數 × 檔大小)——
    /// 12MB 級 document.xml（bug65649）會燒穿分鐘級探針逾時；批次口徑把
    /// N 次全文重掃收斂成 1 次。每段的 `analyze_paragraph` 仍只掃自身區間，
    /// total_cp 語義與 `paragraph_cp_len` 逐字同值（同一支分析器）。
    pub fn paragraph_cp_lens(&self) -> Vec<Option<usize>> {
        let Some(xml) = self.doc.parts.get("word/document.xml") else {
            return Vec::new();
        };
        let (spans, _total) = crate::mutation::paragraph_spans_all(xml);
        spans
            .into_iter()
            .map(|s| Some(crate::mutation::analyze_paragraph(xml, s).total_cp))
            .collect()
    }

    /// 依據 Mutation 規格套用單筆變更。
    pub fn apply_mutation(&mut self, op: Mutation) -> Result<MutationOutcome, MutationError> {
        // E-2b 共型收尾：寫入端誠實閘門必須在觸碰任何 XML 之前（拒絕＝零位元組
        // 位移，不得先寫一半再反悔）。全庫走 `patch_rpr` 寫入面的只有兩臂
        // （FormatTextRange／UpsertNamedStyle，grep `patch_rpr` 呼叫點坐實），
        // 閘門只裝一處、兩臂共軌，堵「樣式臂旁路寫 Raw」的口子。本閘置於
        // K29c/K29d 分流之前：那兩族的型錄不承載 `RunDirectPatch`，閘門對其
        // 恆為直通（零行為變動），但未來任何新增承載臂都自動被本閘覆蓋。
        match &op {
            Mutation::FormatTextRange { patch, .. } => reject_unwritable_underline(patch)?,
            Mutation::UpsertNamedStyle {
                run_patch: Some(rp),
                ..
            } => reject_unwritable_underline(rp)?,
            _ => {}
        }
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
