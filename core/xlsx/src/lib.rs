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
//! - 各 sheet 的 `dimension` 與 row 計數（X1）；
//! - **cell 級投影（X1b）**：`xl/sharedStrings.xml` 的 `<si>` 序列＋rich run
//!   拼接，`sheetData/row/c` 的 cell 型別五態（＋ECMA 型錄的 str/d）、A1 座標
//!   解析、dimension 宣告 vs 實掃交叉對照、空 cell／跳號 row／spans 照實投影。
//!   寫入面仍封鎖：不提供 set_cell／改 part（見 `cells` 模組頭）。
//!
//! 病態容錯鐵律（對齊 ooxml 既有口徑，不虛構）：
//! - zip 結構毀損／加密 entry → `Err`（[`ZipBundle::open`] 的誠實拒絕）；
//! - 缺 `[Content_Types].xml` → `Err`（不是有效的 OPC 包裝，無損宣稱無從谈起）；
//! - 缺 `xl/workbook.xml.rels` → sheet 清單**空集合**（rId 無從歸屬，不猜）；
//! - XML 畸形 → 盡力回報已讀條目（同 `read_rels` 口徑），不 panic。
//!
//! 純 std＋quick-xml＋復用的 ZipBundle，無檔案系統相依 → wasm32 可編譯。

mod cells;
mod model;
mod qx;
mod styles;

pub use cells::{
    read_shared_strings, read_sheet_cells, read_sheet_cells_with_styles, Cell, CellValue,
    DimensionStatus, RowCells, ScannedExtent, SharedStringItem, SharedStrings, SheetCells,
    SHARED_STRINGS_PART,
};
pub use model::{
    read_content_types, read_sheet_stats, read_workbook, read_workbook_rels, ContentTypes,
    DeclaredSheet, SheetInfo, SheetState, WorkbookDecl, WorkbookModel, WorkbookRel, WorksheetStats,
    CONTENT_TYPES_PART, WORKBOOK_PART, WORKBOOK_RELS_PART,
};
pub use styles::{
    parse_date_serial, read_styles, AlignmentModel, BorderLine, BorderModel, CellStyleModel,
    ColorModel, DateSerialError, EffectiveNumFmt, EffectiveStyle, FillModel, FontModel,
    NumFmtCategory, ParsedDateTime, StyleResolutionStatus, StyleSheetModel, XfRecord, STYLES_PART,
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

    /// `xl/sharedStrings.xml` 投影（X1b，只讀）。歸屬順序：**workbook rels 的
    /// sharedStrings 關係為準**（權威；POI InlineStrings.xlsx 實證 SST 不在慣例
    /// 路徑），查無才退回慣例路徑 [`SHARED_STRINGS_PART`]。part 缺失 → None
    /// （誠實；`sheet_cells` 對 `t="s"` 走懸空引用口徑）。
    pub fn shared_strings(&self) -> Option<SharedStrings> {
        let mut path: Option<String> = None;
        if let Some(rels_bytes) = self.part(model::WORKBOOK_RELS_PART) {
            for rel in model::read_workbook_rels(&rels_bytes, "xl") {
                if let Some(target) = rel.target_part {
                    if target.rsplit('/').next() == Some("sharedStrings.xml") {
                        path = Some(target);
                        break;
                    }
                }
            }
        }
        // 查無 rels 歸屬才退回慣例路徑（兩條都缺 → None，誠實不虛構）。
        let bytes = path
            .as_deref()
            .and_then(|p| self.part(p))
            .or_else(|| self.part(SHARED_STRINGS_PART))?;
        Some(read_shared_strings(&bytes))
    }

    /// `xl/styles.xml` 投影（X2，只讀）。歸屬順序：**workbook rels 的
    /// styles 關係為準**，查無才退回慣例路徑 [`STYLES_PART`]。part 缺失 → 回傳空模型
    /// （設計書 §4.5 缺樣式檔容錯口徑）。
    pub fn styles(&self) -> StyleSheetModel {
        let mut path: Option<String> = None;
        if let Some(rels_bytes) = self.part(model::WORKBOOK_RELS_PART) {
            for rel in model::read_workbook_rels(&rels_bytes, "xl") {
                if let Some(target) = rel.target_part {
                    if target.rsplit('/').next() == Some("styles.xml") {
                        path = Some(target);
                        break;
                    }
                }
            }
        }
        let bytes = path
            .as_deref()
            .and_then(|p| self.part(p))
            .or_else(|| self.part(STYLES_PART));
        match bytes {
            Some(b) => read_styles(&b),
            None => StyleSheetModel::default(),
        }
    }

    /// 一個 sheet part 的 cell 級投影（X1b / X2，只讀；自動帶入 SST 查表與 styles 樣式投影）。
    /// part 不存在／CRC 毀損（誠實解碼失敗）→ None，不虛構。
    pub fn sheet_cells(&self, part: &str) -> Option<SheetCells> {
        let xml = self.part(part)?;
        let styles = self.styles();
        Some(read_sheet_cells_with_styles(
            &xml,
            self.shared_strings().as_ref(),
            Some(&styles),
        ))
    }
}
