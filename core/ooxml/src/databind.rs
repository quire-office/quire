//! E1-B：`w:sdt/w:sdtPr/w:dataBinding` 的**內容控件真值求值**（ECMA-376 §17.5.2.10）。
//!
//! 協定：`w:sdtContent` 裡缓存的文字是**佔位**（Word 存檔時把當時的顯示值寫進去），
//! 真正要顯示的值取自 `w:xpath` 指向的**外部資料源**。LO 照協定覆寫後才排版
//! （實據一：`NewDocxDocuments_CoverPage.docx` 佔位 `Eric White` → LO p0 實繪
//! `Quire Corpus`＝`docProps/core.xml` 的 `dc:creator`；實據二：`fdo78957.docx`
//! 頁首框佔位 `Bull on the Run` → LO 實繪 `Quire`＝`docProps/app.xml` 的
//! `<Company>`）。我方只發佔位 run＝缺發真值＋多發佔位，兩席同一根因。
//!
//! 求值口徑（只吃 `w:xpath` 首段根元素＋末段葉元素的**命名空間＋local name**，
//! 由 `w:prefixMappings` 解出前綴對應的 URI；解不出＝`None`＝呼叫端保留佔位
//! 原文，絕不虛構）：
//!
//! | 根元素 URI | 資料源 part | 葉元素寫法 |
//! |---|---|---|
//! | `…/package/2006/metadata/core-properties` | `docProps/core.xml` | 帶前綴 `dc:creator`／`dc:title`… |
//! | `…/officeDocument/2006/extended-properties` | `docProps/app.xml` | 裸標籤 `<Company>` |
//! | `…/office/2006/coverPageProps` | `customXml/itemN.xml`（經 `itemPropsN.xml` 的 `ds:uri` 配對） | 裸標籤 `<Abstract>` |
//!
//! `w:storeItemID` 是資料源的固定 GUID（core=`{6C3C8BC8-…}`、extended=
//! `{6668398D-…}`），corpus 內與 URI 兩兩自洽，故求值以 URI 為準、GUID 不另作
//! 判準（缺 GUID 的檔案照樣求值）。
//!
//! **本模組是純函數**：只吃 `OoxmlDoc::parts` 的位元組與一條 `SdtBinding`，
//! 不碰解析器狀態、不改任何 part 位元組 ⇒ `roundtrip --check` 不受影響。

use std::collections::BTreeMap;

const NS_CORE: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
const NS_DC: &str = "http://purl.org/dc/elements/1.1/";
const NS_APP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties";
const NS_COVER: &str = "http://schemas.microsoft.com/office/2006/coverPageProps";

/// 一個 `w:sdt` 的綁定事實（解析層捕獲，見 `drawing.rs::txbx_sdt_bindings`）。
///
/// `xpath`＝`w:dataBinding/@w:xpath` 原字串；`prefix_mappings`＝
/// `w:prefixMappings` 原字串（決定前綴→URI）；`alias`＝`w:sdtPr/w:alias/@w:val`
/// （诊断用，求值不吃）；`placeholder`＝sdtPr 帶 `w:showingPlcHdr`（Word 存檔時
/// 該控件仍顯示佔位提示）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SdtBinding {
    pub xpath: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_mappings: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub placeholder: bool,
}

impl SdtBinding {
    /// 測試／呼叫端構造器（正常路徑由解析層產生）。
    pub fn new(xpath: impl Into<String>) -> Self {
        Self {
            xpath: xpath.into(),
            prefix_mappings: None,
            alias: None,
            placeholder: false,
        }
    }
}

/// 綁定真值：`Some(非空字串)`＝求值成功（呼叫端以它覆寫佔位 run）；
/// `None`＝求值失敗（不認識的命名空間／part 缺字／葉元素缺字／值為空），
/// 呼叫端**保留佔位原文**（誠實退化，比照 E2c `hf_bound_text` 口徑）。
pub fn resolve_binding(parts: &BTreeMap<String, Vec<u8>>, binding: &SdtBinding) -> Option<String> {
    let (root_ns, leaf_ns, leaf) = split_xpath(binding)?;
    let prefixes = parse_prefix_mappings(binding.prefix_mappings.as_deref());
    let resolve = |p: &str| -> Option<&'static str> {
        let uri = prefixes.get(p)?;
        Some(match uri.as_str() {
            NS_CORE => NS_CORE,
            NS_DC => NS_DC,
            NS_APP => NS_APP,
            NS_COVER => NS_COVER,
            _ => return None,
        })
    };
    // 根元素決定查哪份 part；葉元素決定標籤。dc: 前綴在 core.xml 內是核心
    // 屬性的正寫法（`<dc:creator>`），故 dc 一律歸 core。
    let root = resolve(&root_ns).or_else(|| {
        // 無前綴寫法（`/Properties[1]/Company[1]`）＝預設命名空間＝extended。
        (root_ns.is_empty()).then_some(NS_APP)
    })?;
    let leaf_ns = resolve(&leaf_ns).unwrap_or(root);
    let value = match root {
        NS_CORE => read_core(parts, &leaf),
        NS_APP => read_part_element(parts, "docProps/app.xml", &leaf),
        NS_COVER => read_cover_page(parts, &leaf),
        _ => None,
    };
    // dc:* 也可能是 core 屬性（`/ns1:coreProperties[1]/ns0:title[1]` 兩式在
    // corpus 內混用，前綴序不定），根判不準時補查 core 一份。
    let value = value.or_else(|| {
        if leaf_ns == NS_DC {
            read_core(parts, &leaf)
        } else {
            None
        }
    })?;
    let v = value.trim().to_string();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// `w:xpath` → (根元素前綴, 葉元素前綴, 葉元素 local name)。
///
/// 逐段剝：`/ns0:coreProperties[1]/ns1:creator[1]` → 根前綴 `ns0`、
/// 葉前綴 `ns1`、葉名 `creator`。不靠單次 `rsplit`（`[1]` 與 `:` 混在一式會
/// 切錯位，同 E2c 紀律）。空段／無葉名＝`None`。
fn split_xpath(b: &SdtBinding) -> Option<(String, String, String)> {
    let mut segs = b
        .xpath
        .split('/')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let root = segs.next()?;
    // clippy 鐵律：DoubleEndedIterator 取尾走 next_back（last 會全迭代）。
    let leaf_raw = segs.next_back()?;
    // 根只取前綴（葉名不需要的時候根前綴也要能對上 URI）。
    let root_ns = root.split_once(':').map_or("", |(p, _)| p);
    let (leaf_ns, leaf_name) = match leaf_raw.split_once(':') {
        Some((p, n)) => (p, n),
        None => ("", leaf_raw),
    };
    let leaf_name = leaf_name.split_once('[').map_or(leaf_name, |(n, _)| n);
    let leaf_name = leaf_name.trim();
    if leaf_name.is_empty() {
        return None;
    }
    Some((
        root_ns.to_string(),
        leaf_ns.to_string(),
        leaf_name.to_string(),
    ))
}

/// `w:prefixMappings` → 前綴→URI 表。
///
/// 實測寫法有兩種引號：`xmlns:ns0='…'`（單引號，Word 常態）與
/// `xmlns:ns0="…"`；分隔是空白。逐 token 剝 `xmlns:`／前綴／引號。
fn parse_prefix_mappings(raw: Option<&str>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(raw) = raw else { return out };
    for tok in raw.split_whitespace() {
        let Some((k, v)) = tok.split_once('=') else {
            continue;
        };
        let key = k.trim().trim_start_matches("xmlns:").trim();
        let val = v
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .trim_end_matches(';')
            .trim();
        if !key.is_empty() && !val.is_empty() {
            out.insert(key.to_string(), val.to_string());
        }
    }
    out
}

/// `docProps/core.xml` 取 `<ns:leaf>文字</ns:leaf>`（`dc:`／`cp:` 前綴都由
/// local name 命中，不比前綴）。
fn read_core(parts: &BTreeMap<String, Vec<u8>>, leaf: &str) -> Option<String> {
    read_part_element(parts, "docProps/core.xml", leaf)
}

/// 一般 part：以 local name 配對元素，取其**直接文字**（不吃巢狀標記）。
fn read_part_element(parts: &BTreeMap<String, Vec<u8>>, path: &str, leaf: &str) -> Option<String> {
    let bytes = parts.get(path)?;
    element_text(bytes, leaf)
}

/// coverPageProps 資料源：`customXml/itemPropsN.xml` 的 `ds:schemaRef/@ds:uri`
/// 命中 coverPageProps ⇒ 同目錄的 `itemN.xml` 就是該資料源（實據
/// `NewDocxDocuments_CoverPage.docx`：`itemProps1.xml` 宣告
/// `ds:uri="…coverPageProps"`，`item1.xml` 帶 `<Abstract>`）。
fn read_cover_page(parts: &BTreeMap<String, Vec<u8>>, leaf: &str) -> Option<String> {
    for (name, bytes) in parts {
        if !is_item_props(name) {
            continue;
        }
        let text = String::from_utf8_lossy(bytes).into_owned();
        if !text.contains(NS_COVER) {
            continue;
        }
        let base = name.rsplit('/').next()?;
        let item = format!("customXml/{}", base.replace("itemProps", "item"));
        let bytes = parts.get(&item)?;
        if let Some(v) = element_text(bytes, leaf) {
            return Some(v);
        }
    }
    None
}

fn is_item_props(name: &str) -> bool {
    let Some(base) = name.strip_prefix("customXml/") else {
        return false;
    };
    let Some(rest) = base.strip_prefix("itemProps") else {
        return false;
    };
    let Some(stem) = rest.strip_suffix(".xml") else {
        return false;
    };
    !stem.is_empty() && stem.chars().all(|c| c.is_ascii_digit())
}

/// 在一份 XML 位元組流裡找第一個 local name == `leaf` 的元素，回其直接文字
/// （`w:t` 式的純文字節點；含子元素時只取子元素外的文字，缺＝`None`）。
fn element_text(bytes: &[u8], leaf: &str) -> Option<String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut capture = false;
    let mut depth = 0usize;
    let mut text = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if capture {
                    depth += 1;
                } else if e.local_name().as_ref() == leaf.as_bytes() {
                    capture = true;
                    depth = 0;
                    text.clear();
                }
            }
            Ok(Event::Empty(_)) => {
                if capture {
                    // 葉元素自閉合＝無文字（空值），老實停捕。
                    return None;
                }
            }
            Ok(Event::Text(t)) => {
                if capture && depth == 0 {
                    text.push_str(&String::from_utf8_lossy(t.as_ref()));
                }
            }
            Ok(Event::End(_e)) => {
                if capture {
                    if depth == 0 {
                        return Some(std::mem::take(&mut text));
                    }
                    depth -= 1;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    capture.then(|| std::mem::take(&mut text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts() -> BTreeMap<String, Vec<u8>> {
        let mut m = BTreeMap::new();
        m.insert(
            "docProps/core.xml".to_string(),
            br#"<?xml version="1.0"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>The Title is here</dc:title><dc:subject>And a subtitle</dc:subject><dc:creator>Quire Corpus</dc:creator></cp:coreProperties>"#.to_vec(),
        );
        m.insert(
            "docProps/app.xml".to_string(),
            br#"<?xml version="1.0"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Company>Quire</Company><Application>Microsoft Office Word</Application></Properties>"#.to_vec(),
        );
        m.insert(
            "customXml/itemProps1.xml".to_string(),
            br#"<ds:datastoreItem ds:itemID="{55AF091B-3C7A-41E3-B477-F2FDAA23CFDA}" xmlns:ds="http://schemas.openxmlformats.org/officeDocument/2006/customXml"><ds:schemaRefs><ds:schemaRef ds:uri="http://schemas.microsoft.com/office/2006/coverPageProps"/></ds:schemaRefs></ds:datastoreItem>"#.to_vec(),
        );
        m.insert(
            "customXml/item1.xml".to_string(),
            br#"<CoverPageProperties xmlns="http://schemas.microsoft.com/office/2006/coverPageProps"><PublishDate/><Abstract>This is my interesting abstract</Abstract></CoverPageProperties>"#.to_vec(),
        );
        m
    }

    fn bind(xpath: &str, mappings: &str) -> SdtBinding {
        SdtBinding {
            xpath: xpath.to_string(),
            prefix_mappings: Some(mappings.to_string()),
            alias: None,
            placeholder: false,
        }
    }

    const CORE_MAP: &str = "xmlns:ns0='http://schemas.openxmlformats.org/package/2006/metadata/core-properties' xmlns:ns1='http://purl.org/dc/elements/1.1/'";
    const APP_MAP: &str =
        "xmlns:ns0='http://schemas.openxmlformats.org/officeDocument/2006/extended-properties'";
    const COVER_MAP: &str = "xmlns:ns0='http://schemas.microsoft.com/office/2006/coverPageProps'";

    #[test]
    fn core_properties_paths_resolve() {
        let p = parts();
        // CoverPage 實據三式：creator／title／subject（LO 真值 Quire Corpus）。
        assert_eq!(
            resolve_binding(&p, &bind("/ns0:coreProperties[1]/ns1:creator[1]", CORE_MAP))
                .as_deref(),
            Some("Quire Corpus")
        );
        assert_eq!(
            resolve_binding(&p, &bind("/ns0:coreProperties[1]/ns1:title[1]", CORE_MAP)).as_deref(),
            Some("The Title is here")
        );
        assert_eq!(
            resolve_binding(&p, &bind("/ns0:coreProperties[1]/ns1:subject[1]", CORE_MAP))
                .as_deref(),
            Some("And a subtitle")
        );
        // 前綴序對調（corpus 實測 fdo82123／dml-groupshape-sdt 兩式混用）。
        assert_eq!(
            resolve_binding(
                &p,
                &bind(
                    "/ns1:coreProperties[1]/ns0:title[1]",
                    "xmlns:ns0='http://purl.org/dc/elements/1.1/' xmlns:ns1='http://schemas.openxmlformats.org/package/2006/metadata/core-properties'"
                )
            )
            .as_deref(),
            Some("The Title is here")
        );
    }

    #[test]
    fn extended_properties_resolves_company() {
        let p = parts();
        // fdo78957 頁首框實據：佔位 `Bull on the Run` → LO `Quire`。
        assert_eq!(
            resolve_binding(&p, &bind("/ns0:Properties[1]/ns0:Company[1]", APP_MAP)).as_deref(),
            Some("Quire")
        );
    }

    #[test]
    fn cover_page_datastore_resolves_abstract() {
        let p = parts();
        assert_eq!(
            resolve_binding(
                &p,
                &bind("/ns0:CoverPageProperties[1]/ns0:Abstract[1]", COVER_MAP)
            )
            .as_deref(),
            Some("This is my interesting abstract")
        );
    }

    #[test]
    fn unknown_shape_degrades_to_none() {
        let p = parts();
        // 不認識的命名空間（自訂 datastore）＝不猜。
        assert_eq!(
            resolve_binding(
                &p,
                &bind(
                    "/ns0:Whatever[1]/ns0:Thing[1]",
                    "xmlns:ns0='http://example.com/whatever'"
                )
            ),
            None
        );
        // 缺字 part／缺字元素＝None。
        assert_eq!(
            resolve_binding(&p, &bind("/ns0:Properties[1]/ns0:Manager[1]", APP_MAP)),
            None
        );
        // 空值（PublishDate 自閉合）＝None，呼叫端保留佔位。
        assert_eq!(
            resolve_binding(
                &p,
                &bind("/ns0:CoverPageProperties[1]/ns0:PublishDate[1]", COVER_MAP)
            ),
            None
        );
        // xpath 畸形＝None（不 panic）。
        assert_eq!(resolve_binding(&p, &SdtBinding::new("/")), None);
        assert_eq!(resolve_binding(&p, &SdtBinding::new("")), None);
    }

    #[test]
    fn prefix_mappings_accepts_both_quote_styles() {
        let single = parse_prefix_mappings(Some("xmlns:ns0='http://a' xmlns:ns1='http://b'"));
        let double = parse_prefix_mappings(Some("xmlns:ns0=\"http://a\" xmlns:ns1=\"http://b\" "));
        assert_eq!(single, double);
        assert_eq!(single.get("ns1").map(|s| s.as_str()), Some("http://b"));
    }
}
