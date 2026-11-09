//! quire-odf —— ODF (ODT) 容器層無損往返與最小內文模型。
//!
//! 定位（ODF-C2 卡）：建立 core/odf 容器層第一切片。
//! 容器層復用 [`quire_ooxml::ZipBundle`]，達成無損 byte-identical round-trip。
//! 最小內文模型提供段落計數與文字流抽取。
//!
//! 規範依據：
//! - OASIS ODF 1.3 Part 2: Packages
//! - OASIS ODF 1.3 Part 3: OpenDocument Schema
//! - docs/odf-design.md
//! - docs/odf-survey/survey.json

mod body;
pub mod display;
pub mod header_footer;
pub mod layout_glue;
mod manifest;
mod objects;
pub mod page_geometry;
mod skeleton;

pub use body::{BodyModel, OdfParagraph};
pub use display::{
    display_list_json, dump_display_lists_json, emit_display_lists, OdtView,
    DEFAULT_TEXT_COLOR_ARGB,
};
pub use header_footer::{
    buggy_single_master, hf_assignment, hf_masters, master_page_switches, HfAssignment, HfPage,
    HfRole, HfSlot, MasterHf, MasterHfSlotKind, ParaSwitch,
};
pub use layout_glue::{
    layout_projection, page_break_positions, project, project_bytes, LayoutParams,
    OdfLayoutParagraph, OdfProjection, DEFAULT_FONT_SIZE_PT, MAX_STYLE_HOPS,
};
pub use manifest::{Manifest, ManifestEntry, ALGORITHM_UNSPECIFIED};
pub use objects::{ObjectKind, OdfObjectText};
pub use page_geometry::{
    odf_length_to_pt, page_geometry, page_geometry_bytes, GeometrySource, MasterPage,
    OdfPageGeometry, Orientation, PageLayout, StyleDomainPage, CM_TO_PT, LO_DEFAULT_MARGIN_CM,
    LO_DEFAULT_MARGIN_PT, LO_DEFAULT_PAGE_H_PT, LO_DEFAULT_PAGE_W_PT,
};
pub use skeleton::OdfSkeleton;

use std::collections::BTreeMap;
use thiserror::Error;

use quire_ooxml::{Change, ZipBundle};

/// ODF 標準 MIME 型別
pub const ODT_MIMETYPE: &str = "application/vnd.oasis.opendocument.text";

#[derive(Error, Debug)]
pub enum OdfError {
    #[error("zip 解析失敗: {0}")]
    Zip(#[from] quire_ooxml::ZipError),
    #[error("缺少 META-INF/manifest.xml")]
    MissingManifest,
    #[error("缺少 content.xml")]
    MissingContentXml,
    #[error("條目加密: {full_path}")]
    EncryptedEntry { full_path: String },
    /// ODF-E1：manifest 宣告 `<manifest:encryption-data>` 之條目，我方不支援
    /// 解密——消費層（內文抽取／文字流投影）必須走這條明確變體老實報錯，
    /// 禁止把密文投影成零段空文件（與 odt-render「不吞錯」紀律一致）。
    ///
    /// 與 `EncryptedEntry` 之別：後者是設計書 §5.1 的「容器層攔截」口徑
    /// （`open` 直接拒收），本變體是 ODF-E1 定案的「消費層攔截」口徑——
    /// 容器層照常開檔以保住無損 round-trip，攔截落在 extract_body／project。
    #[error("加密不支援：條目 {full_path} 宣告加密（演算法 {algorithm}），本引擎不實作解密")]
    Encrypted {
        full_path: String,
        algorithm: String,
    },
    #[error("缺少條目: {path}")]
    MissingEntry { path: String },
    #[error("XML 解析失敗: {0}")]
    Xml(String),
    #[error("無效的路徑: {0}")]
    InvalidPath(String),
}

/// ODF 1.3 Part 2 §3.3 mimetype 狀態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MimetypeStatus {
    /// 合規：首個條目且未壓縮
    Compliant,
    /// 存在但非首個條目
    NotFirstEntry { index: usize },
    /// 已壓縮（違反 METHOD_STORE 規定）
    Compressed { method: u16 },
    /// 非首個條目且被壓縮
    NotFirstAndCompressed { index: usize, method: u16 },
    /// 缺失 mimetype 條目
    Missing,
}

impl MimetypeStatus {
    /// 是否完全符合 ODF 1.3 §3.3 物理規範
    pub fn is_compliant(&self) -> bool {
        matches!(self, Self::Compliant)
    }

    /// 是否違反「首個條目」約束
    pub fn is_not_first(&self) -> bool {
        matches!(
            self,
            Self::NotFirstEntry { .. } | Self::NotFirstAndCompressed { .. }
        )
    }
}

/// ODF 封裝容器
pub struct OdfBundle {
    zip: ZipBundle,
    mimetype_status: MimetypeStatus,
    manifest: Manifest,
    changes: BTreeMap<String, Change>,
    extras: BTreeMap<String, Vec<u8>>,
}

impl OdfBundle {
    /// 由位元組開啟 ODF 封裝。
    /// 依據 ODF 1.3 規範檢驗 manifest.xml 與 content.xml 存在性；
    /// 偵測 mimetype 狀態但容許非標準檔歸類不拒收。
    pub fn open(bytes: &[u8]) -> Result<Self, OdfError> {
        let zip = ZipBundle::open(bytes).map_err(OdfError::Zip)?;

        // 偵測 mimetype 狀態（ODF 1.3 Part 2 §3.3）
        let mut mimetype_entry_index: Option<usize> = None;
        let mut mimetype_method: u16 = 0;
        let mut mimetype_offset: u64 = 0;

        for (i, entry) in zip.entries.iter().enumerate() {
            if entry.name_raw == b"mimetype" {
                mimetype_entry_index = Some(i);
                mimetype_method = entry.method;
                mimetype_offset = entry.local_header_offset;
                break;
            }
        }

        let mimetype_status = match mimetype_entry_index {
            None => MimetypeStatus::Missing,
            Some(idx) => {
                let is_first = idx == 0 && mimetype_offset == 0;
                let is_stored = mimetype_method == 0;
                match (is_first, is_stored) {
                    (true, true) => MimetypeStatus::Compliant,
                    (true, false) => MimetypeStatus::Compressed {
                        method: mimetype_method,
                    },
                    (false, true) => MimetypeStatus::NotFirstEntry { index: idx },
                    (false, false) => MimetypeStatus::NotFirstAndCompressed {
                        index: idx,
                        method: mimetype_method,
                    },
                }
            }
        };

        // 檢查與解析 META-INF/manifest.xml
        let manifest_entry = zip
            .entry("META-INF/manifest.xml")
            .ok_or(OdfError::MissingManifest)?;
        let manifest_bytes = manifest_entry.decoded().map_err(OdfError::Zip)?;
        let manifest = manifest::parse_manifest(&manifest_bytes)?;

        // 檢查 content.xml 存在性（ODF 最小封裝契約）
        if zip.entry("content.xml").is_none() {
            return Err(OdfError::MissingContentXml);
        }

        Ok(Self {
            zip,
            mimetype_status,
            manifest,
            changes: BTreeMap::new(),
            extras: BTreeMap::new(),
        })
    }

    /// 無損存回：未修改條目逐位元組保留原始狀態（byte-identical）
    pub fn save(&self) -> Result<Vec<u8>, OdfError> {
        self.zip
            .serialize(&self.changes, &self.extras)
            .map_err(OdfError::Zip)
    }

    /// 與 save 相同語義之別名（對齊設計書提案命名）
    pub fn to_bytes(&self) -> Result<Vec<u8>, OdfError> {
        self.save()
    }

    /// 取得 mimetype 狀態
    pub fn mimetype_status(&self) -> &MimetypeStatus {
        &self.mimetype_status
    }

    /// 取得清單資訊
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// 全部條目名稱清單
    pub fn part_names(&self) -> Vec<String> {
        self.zip.part_names()
    }

    /// 讀取特定條目之未壓縮位元組
    pub fn read_entry(&self, name: &str) -> Result<Vec<u8>, OdfError> {
        self.zip
            .entry(name)
            .ok_or_else(|| OdfError::MissingEntry {
                path: name.to_string(),
            })?
            .decoded()
            .map_err(OdfError::Zip)
    }

    /// 掃描骨架與核心 XML 健康度
    pub fn scan_skeleton(&self) -> OdfSkeleton {
        skeleton::scan_skeleton(&self.zip)
    }

    /// 抽取內文最小模型。
    /// ODF-E1：content.xml 若經 manifest 宣告加密 → `Err(OdfError::Encrypted)`
    /// （老實報錯，禁止投影成零段空文件）。
    pub fn extract_body(&self) -> Result<BodyModel, OdfError> {
        self.ensure_not_encrypted("content.xml")?;
        let content_bytes = self.read_entry("content.xml")?;
        body::extract_body_from_bytes(&content_bytes, &self.manifest)
    }

    /// ODF-E1 加密閘門：manifest 宣告該條目加密者，一律回明確變體
    /// `OdfError::Encrypted`（訊息含演算法名；algorithm-name 缺位寫
    /// `ALGORITHM_UNSPECIFIED`，不捏造）。消費層（內文抽取／投影）入口必經此閘。
    pub(crate) fn ensure_not_encrypted(&self, path: &str) -> Result<(), OdfError> {
        if self.manifest.is_encrypted(path) {
            return Err(OdfError::Encrypted {
                full_path: path.to_string(),
                algorithm: self
                    .manifest
                    .algorithm_of(path)
                    .unwrap_or(ALGORITHM_UNSPECIFIED)
                    .to_string(),
            });
        }
        Ok(())
    }
}
