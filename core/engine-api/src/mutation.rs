//! 編輯 API Mutation 型錄、Surgical XML Patch 與 Undo/Redo 實作（K29a / Slice 1）。
//!
//! 依據 `docs/edit-api-design.md` §2、§3 與 §5 規範實作：
//! - 正文文字編輯鎖定正準碼點偏移（cp: usize），不含清單 marker 或點引合成字元。
//! - 外科手術式 XML 修補（Surgical XML Patch）僅替換受影響節點的位元組切片，
//!   未觸碰段落、表格、未知屬性（w:rsidR、w14:paraId、mc:AlternateContent）原樣保留。
//! - 支援精確逆操作（InverseMutation）與 touched 自動退場機制。

use crate::Engine;
use quire_layout::Invalidation;
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

/// 字元級直接格式覆寫（預留擴充）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunDirectPatch {}

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
    /// M1.2 單段或跨段範圍刪除（Slice 1 僅支援單段）
    DeleteRange { range: DocRange },
    /// M1.5 插入斷點
    InsertBreak {
        target: SubDocTarget,
        pos: DocPos,
        kind: BreakKind,
    },
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

/// Mutation 套用結果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationOutcome {
    pub events: Vec<Invalidation>,
    pub touched_parts: Vec<String>,
    pub inverse: Vec<InverseMutation>,
    pub caret_after: Option<DocPos>,
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
    CrossContainerBoundary {
        start_para: usize,
        end_para: usize,
    },
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

impl Engine {
    /// 套用單筆 Mutation。
    pub fn apply_mutation(&mut self, _op: Mutation) -> Result<MutationOutcome, MutationError> {
        // 先給紅燈打底：尚未實作時直接回報失敗或未實作
        Err(MutationError::XmlPatchRejected {
            detail: "尚未實作 Surgical XML Patch 寫回".to_string(),
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
        Err(MutationError::XmlPatchRejected {
            detail: "尚未實作 Undo".to_string(),
        })
    }

    /// 重做頂端操作。
    pub fn redo(&mut self) -> Result<Option<MutationOutcome>, MutationError> {
        Err(MutationError::XmlPatchRejected {
            detail: "尚未實作 Redo".to_string(),
        })
    }
}
