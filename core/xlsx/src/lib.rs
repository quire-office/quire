//! quire-xlsx —— XLSX/XLSM（OPC 容器）無損 round-trip ＋ 最小 workbook 模型。
//!
//! 定位（X1 卡）：證明 DOCX 打下的**無損紀律能複製到第二格式**，而不是重寫一個。
//! 容器層完全復用 [`quire_ooxml::ZipBundle`]（entry 級 raw bytes 搬運，格式無關）：
//! 開檔→（本卡不改任何 part）→存回 = **byte-identical**。
//!
//! workbook 模型是**純讀取投影**（只讀不寫回，對齊 `table.rs` 的「唯讀來源隔離」）：
//! - `[Content_Types].xml`：Default/Override 清單；
//! - `xl/workbook.xml`：sheet 清單（name/sheetId/state）＋ definedNames 計數；
//! - `xl/_rels/workbook.xml.rels`：rId → part 路徑對照；
//! - 各 sheet 的 `dimension` 與 row 計數（本卡不解析 cell，留 X1b）。
//!
//! 病態容錯鐵律（對齊 ooxml 既有口徑，不虛構）：
//! - zip 結構毀損／加密 entry → `Err`（[`ZipBundle::open`] 的誠實拒絕）；
//! - 缺 `[Content_Types].xml` → `Err`（不是有效的 OPC 包裝，無損宣稱無從谈起）；
//! - 缺 `xl/workbook.xml.rels` → sheet 清單**空集合**（rId 無從歸屬，不猜）；
//! - XML 畸形 → 盡力回報已讀條目（同 `read_rels` 口徑），不 panic。
//!
//! 純 std＋quick-xml＋復用的 ZipBundle，無檔案系統相依 → wasm32 可編譯。

mod model;

pub use model::{
    read_content_types, read_sheet_stats, read_workbook, read_workbook_rels, ContentTypes,
    DeclaredSheet, SheetInfo, SheetState, WorkbookDecl, WorkbookModel, WorkbookRel, WorksheetStats,
    CONTENT_TYPES_PART, WORKBOOK_PART, WORKBOOK_RELS_PART,
};

use std::collections::BTreeMap;
use thiserror::Error;

use quire_ooxml::{Change, ZipBundle};

#[derive(Error, Debug)]
pub enum XlsxError {
    #[error("zip 解析失敗: {0}")]
    Zip(String),
    #[error("缺少 [Content_Types].xml，不是有效的 OPC 包裝（.xlsx/.xlsm）")]
    MissingContentTypes,
}

impl From<quire_ooxml::ZipError> for XlsxError {
    fn from(e: quire_ooxml::ZipError) -> Self {
        XlsxError::Zip(e.to_string())
    }
}

/// OPC 容器：zip 結構快照（無損存回依據）＋ part 明文索引（只讀投影用）。
///
/// 本卡不提供 `set_part`／`remove_part`——寫入面（模型改動寫回 parts）是後續
/// 卡的事；現在提供編輯面只會誘導繞過尚未存在的模型，破壞「唯一真相」路線。
#[derive(Debug, Clone)]
pub struct XlsxBundle {
    bundle: ZipBundle,
}

impl XlsxBundle {
    /// 由 .xlsx／.xlsm 位元組開啟（不碰檔案系統，wasm32 友善）。
    /// OPC 必要 part `[Content_Types].xml` 缺失 → 誠實 `Err`。
    pub fn open(bytes: &[u8]) -> Result<Self, XlsxError> {
        let bundle = ZipBundle::open(bytes)?;
        if bundle.entry(model::CONTENT_TYPES_PART).is_none() {
            return Err(XlsxError::MissingContentTypes);
        }
        Ok(Self { bundle })
    }

    /// 存回：本卡不改任何 part → changes/extras 全空 → 未觸碰 entry 逐位元組
    /// 沿用原始位元組（local/gap/central 原樣串接），輸出與輸入 byte-identical。
    pub fn save(&self) -> Result<Vec<u8>, XlsxError> {
        let changes: BTreeMap<String, Change> = BTreeMap::new();
        let extras: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        Ok(self.bundle.serialize(&changes, &extras)?)
    }

    /// 全部檔案 part 名稱（目錄 entry 除外），central directory 順序。
    pub fn part_names(&self) -> Vec<String> {
        self.bundle.part_names()
    }

    /// 一般檔案 entry 數（CLI 彙報用）。
    pub fn part_count(&self) -> usize {
        self.bundle.file_entries().count()
    }

    /// part 明文（解壓後）。缺 part → None（誠實，不虛構）。
    pub fn part(&self, name: &str) -> Option<Vec<u8>> {
        self.bundle.entry(name).and_then(|e| e.decoded().ok())
    }

    /// workbook 模型：純讀取投影（見模組頭容錯口徑）——本方法**不失敗**：
    /// 缺 part／空 rels／XML 畸形一律落空集合或盡力已讀條目。
    pub fn workbook_model(&self) -> WorkbookModel {
        let content_types = self
            .part(model::CONTENT_TYPES_PART)
            .map(|b| read_content_types(&b))
            .unwrap_or_default();
        // rels：缺 part＝None（→ sheet 清單空，卡面口徑）；存在＝Some（可為空表）。
        let rels: Option<BTreeMap<String, Option<String>>> = self
            .part(model::WORKBOOK_RELS_PART)
            .map(|b| model::read_workbook_rels(&b, "xl"))
            .map(|v| v.into_iter().map(|r| (r.id, r.target_part)).collect());
        let mut m = read_workbook(
            self.part(model::WORKBOOK_PART).as_deref(),
            rels.as_ref(),
            &|name| self.bundle.entry(name).is_some(),
            &|name| self.part(name),
        );
        m.content_types = content_types;
        m
    }
}
