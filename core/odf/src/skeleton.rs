//! ODF 封裝骨架與核心 XML 串流健康度掃描。
//!
//! 檢查 content.xml / styles.xml / meta.xml / settings.xml / manifest.xml 的存在性，
//! 並透過串流掃描驗證 XML 健全度（well-formedness）。

use quick_xml::events::Event;
use quick_xml::Reader;
use quire_ooxml::ZipBundle;

/// 骨架與健康度統計結構
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

/// 掃描 ZipBundle 的骨架與 XML 健康度
pub fn scan_skeleton(zip: &ZipBundle) -> OdfSkeleton {
    let mut skel = OdfSkeleton {
        has_content_xml: zip.entry("content.xml").is_some(),
        has_styles_xml: zip.entry("styles.xml").is_some(),
        has_meta_xml: zip.entry("meta.xml").is_some(),
        has_settings_xml: zip.entry("settings.xml").is_some(),
        has_manifest_xml: zip.entry("META-INF/manifest.xml").is_some(),
        content_xml_well_formed: false,
        styles_xml_well_formed: false,
        meta_xml_well_formed: false,
    };

    if let Some(entry) = zip.entry("content.xml") {
        if let Ok(bytes) = entry.decoded() {
            skel.content_xml_well_formed = check_xml_well_formed(&bytes);
        }
    }

    if let Some(entry) = zip.entry("styles.xml") {
        if let Ok(bytes) = entry.decoded() {
            skel.styles_xml_well_formed = check_xml_well_formed(&bytes);
        }
    }

    if let Some(entry) = zip.entry("meta.xml") {
        if let Ok(bytes) = entry.decoded() {
            skel.meta_xml_well_formed = check_xml_well_formed(&bytes);
        }
    }

    skel
}

/// 串流檢驗位元組是否為合法 well-formed XML
fn check_xml_well_formed(bytes: &[u8]) -> bool {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => return true,
            Err(_) => return false,
            _ => {}
        }
        buf.clear();
    }
}
