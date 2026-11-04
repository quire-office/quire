//! quire-odf —— ODF (ODT) 容器層無損往返與最小內文模型。
//!
//! 定位（ODF-C2 卡）：建立 core/odf 容器層第一切片。
//! 容器層復用 [`quire_ooxml::ZipBundle`]，達成無損 byte-identical round-trip。
//! 最小內文模型提供段落計數與文字流抽取。

use std::collections::{BTreeMap, HashMap};
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
    #[error("缺少條目: {path}")]
    MissingEntry { path: String },
    #[error("XML 解析失敗: {0}")]
    Xml(String),
    #[error("無效的路徑: {0}")]
    InvalidPath(String),
}

/// ODF 1.3 §3.3 mimetype 狀態
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
    pub fn is_compliant(&self) -> bool {
        matches!(self, Self::Compliant)
    }

    pub fn is_not_first(&self) -> bool {
        matches!(
            self,
            Self::NotFirstEntry { .. } | Self::NotFirstAndCompressed { .. }
        )
    }
}

/// 清單條目描述
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub full_path: String,
    pub media_type: String,
    pub is_encrypted: bool,
    pub size: Option<u64>,
}

/// 清單對映表
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub entries: HashMap<String, ManifestEntry>,
}

impl Manifest {
    pub fn media_type_of(&self, path: &str) -> Option<&str> {
        self.entries.get(path).map(|e| e.media_type.as_str())
    }

    pub fn is_encrypted(&self, path: &str) -> bool {
        self.entries.get(path).map_or(false, |e| e.is_encrypted)
    }

    pub fn has_encryption(&self) -> bool {
        self.entries.values().any(|e| e.is_encrypted)
    }
}

/// 骨架掃描結果
#[derive(Debug, Clone, Default)]
pub struct OdfSkeleton {
    pub has_content_xml: bool,
    pub has_styles_xml: bool,
    pub has_meta_xml: bool,
    pub has_settings_xml: bool,
    pub has_manifest_xml: bool,
    pub content_xml_well_formed: bool,
    pub styles_xml_well_formed: bool,
    pub meta_xml_well_formed: bool,
}

/// 內文段落最小模型
#[derive(Debug, Clone, Default)]
pub struct OdfParagraph {
    pub text: String,
}

/// 內文文字流最小模型
#[derive(Debug, Clone, Default)]
pub struct BodyModel {
    pub paragraphs: Vec<OdfParagraph>,
    pub total_text_len: usize,
    pub text_p_count: usize,
    pub text_h_count: usize,
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
    /// 由位元組開啟 ODF 封裝
    pub fn open(_bytes: &[u8]) -> Result<Self, OdfError> {
        // 紅燈樁代碼：未實作，拋出錯誤以產生紅燈
        Err(OdfError::MissingContentXml)
    }

    /// 無損存回：未修改條目保證逐位元組相同
    pub fn save(&self) -> Result<Vec<u8>, OdfError> {
        self.zip
            .serialize(&self.changes, &self.extras)
            .map_err(OdfError::Zip)
    }

    /// mimetype 狀態
    pub fn mimetype_status(&self) -> &MimetypeStatus {
        &self.mimetype_status
    }

    /// 清單參照
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// 條目名稱清單
    pub fn part_names(&self) -> Vec<String> {
        self.zip.part_names()
    }

    /// 讀取特定條目未壓縮內容
    pub fn read_entry(&self, name: &str) -> Result<Vec<u8>, OdfError> {
        self.zip
            .entry(name)
            .ok_or_else(|| OdfError::MissingEntry {
                path: name.to_string(),
            })?
            .decoded()
            .map_err(OdfError::Zip)
    }

    /// 掃描骨架與 XML 健康度
    pub fn scan_skeleton(&self) -> OdfSkeleton {
        OdfSkeleton::default()
    }

    /// 抽取內文模型
    pub fn extract_body(&self) -> Result<BodyModel, OdfError> {
        Ok(BodyModel::default())
    }
}
