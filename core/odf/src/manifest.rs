//! ODF 清單 (META-INF/manifest.xml) 解析模組。
//!
//! 依據 OASIS ODF 1.3 Part 2 §3.2 與 §4.2 規範，解析封裝內各條目之
//! media-type、路徑及加密狀態（§4.4）。

use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

use crate::OdfError;

/// 清單中單一條目的描述
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// 封裝內完整路徑（如 "content.xml" 或 "/"）
    pub full_path: String,
    /// 媒體型別（MIME Type）
    pub media_type: String,
    /// 是否具備加密資料宣告（ODF 1.3 Part 2 §4.4）
    pub is_encrypted: bool,
    /// 未加密原始大小（若宣告）
    pub size: Option<u64>,
}

/// 清單對映表
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub entries: HashMap<String, ManifestEntry>,
}

impl Manifest {
    /// 查詢特定路徑之媒體型別
    pub fn media_type_of(&self, path: &str) -> Option<&str> {
        self.entries.get(path).map(|e| e.media_type.as_str())
    }

    /// 判斷特定路徑是否被標記為加密
    pub fn is_encrypted(&self, path: &str) -> bool {
        self.entries.get(path).is_some_and(|e| e.is_encrypted)
    }

    /// 封裝中是否存在任何加密條目
    pub fn has_encryption(&self) -> bool {
        self.entries.values().any(|e| e.is_encrypted)
    }

    /// 取得條目總數
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否為空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// 解析 META-INF/manifest.xml 位元組
pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, OdfError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut entries = HashMap::new();
    let mut buf = Vec::new();

    // 當前正在處理的 file-entry
    let mut current_entry: Option<ManifestEntry> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let local = name.local_name();
                if local.as_ref() == b"file-entry" {
                    let (path, media_type, size) = parse_file_entry_attrs(&e);
                    current_entry = Some(ManifestEntry {
                        full_path: path,
                        media_type,
                        is_encrypted: false,
                        size,
                    });
                } else if local.as_ref() == b"encryption-data" {
                    if let Some(entry) = current_entry.as_mut() {
                        entry.is_encrypted = true;
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name();
                let local = name.local_name();
                if local.as_ref() == b"file-entry" {
                    let (path, media_type, size) = parse_file_entry_attrs(&e);
                    entries.insert(
                        path.clone(),
                        ManifestEntry {
                            full_path: path,
                            media_type,
                            is_encrypted: false,
                            size,
                        },
                    );
                } else if local.as_ref() == b"encryption-data" {
                    if let Some(entry) = current_entry.as_mut() {
                        entry.is_encrypted = true;
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let local = name.local_name();
                if local.as_ref() == b"file-entry" {
                    if let Some(entry) = current_entry.take() {
                        entries.insert(entry.full_path.clone(), entry);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(OdfError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(Manifest { entries })
}

/// 抽取 `<manifest:file-entry>` 的屬性
fn parse_file_entry_attrs(e: &quick_xml::events::BytesStart<'_>) -> (String, String, Option<u64>) {
    let mut full_path = String::new();
    let mut media_type = String::new();
    let mut size = None;

    for attr in e.attributes().flatten() {
        let key = attr.key.local_name();
        let value = String::from_utf8_lossy(&attr.value).into_owned();
        match key.as_ref() {
            b"full-path" => full_path = value,
            b"media-type" => media_type = value,
            b"size" => size = value.parse::<u64>().ok(),
            _ => {}
        }
    }

    (full_path, media_type, size)
}
