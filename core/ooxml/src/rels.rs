//! OPC relationships（`word/_rels/document.xml.rels`）——rId → part 路徑查表。
//!
//! ECMA-376 §15 / OPC §3：`headerReference`／`footerReference` 只帶 `r:id`，
//! 真正的 part 路徑住在包裝層的 `.rels` 檔案。本模組只做查表與路徑正規化
//! （相對於 `word/` 基準解析、`..` 折回、%XX 解碼），不做內容解析。
//!
//! 容錯鐵律（查不到的東西一概 None，不猜、不虛構）：
//! - rId 不存在 → None；
//! - `TargetMode="External"`（http 目標）→ None（不是 zip 內 part）；
//! - 查表結果是否真的是 zip 內 part，由呼叫端以 `parts` 成員檢查決定
//!   （指向不存在 part 的懸空 rId 由此容錯跳過）。

use quick_xml::events::Event;

/// 一個 `Relationship` 條目（只保留查表需要的三欄）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    /// `Id`（"rId7" 等，不透明字串）。
    pub id: String,
    /// `Target` 正規化後的 zip part 路徑（相對包装根，如 `word/header1.xml`）。
    /// External 目標＝None。
    pub target_part: Option<String>,
}

/// 解析一份 `.rels` XML 明文為 rId → 正規化 part 路徑的查表。
/// 重複 Id 後者勝（畸形容錯）；XML 錯誤＝盡力回報已讀條目。
pub fn read_rels(xml: &[u8]) -> Vec<Relationship> {
    let text = String::from_utf8_lossy(xml);
    let mut re = quick_xml::Reader::from_str(&text);
    let mut out = Vec::new();
    while let Ok(ev) = re.read_event() {
        match ev {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                if e.local_name().as_ref() != b"Relationship" {
                    continue;
                }
                let id = match attr(&e, b"Id") {
                    Some(v) => v,
                    None => continue, // 無 Id＝畸形條目，跳過
                };
                let external = matches!(
                    attr(&e, b"TargetMode").as_deref(),
                    Some(m) if m.eq_ignore_ascii_case("external")
                );
                let target_part = if external {
                    None
                } else {
                    attr(&e, b"Target").map(|t| normalize_part_path(&t))
                };
                out.push(Relationship { id, target_part });
            }
            _ => {}
        }
    }
    out
}

/// 由 `word/_rels/document.xml.rels` 的查表把 rId 換算成 part 路徑。
/// `base_dir`＝來源 part 的目錄（document.xml 在 `word/`）。
/// rId 缺失／External → None。
pub fn resolve_relationship(rels: &[Relationship], rid: &str) -> Option<String> {
    rels.iter()
        .rev()
        .find(|r| r.id == rid)
        .and_then(|r| r.target_part.clone())
}

/// OPC Target → zip part 路徑正規化：
/// - 絕對目標（`/…`）：相對包装根；
/// - 相對目標：以 `word/` 為基準拼接，`..` 折回上一層、`.` 與空段丟棄；
/// - `%XX` URI 解碼（還原 %20 等萬一檔名內有空格）。
fn normalize_part_path(target: &str) -> String {
    let decoded = percent_decode(target);
    let absolute = decoded.starts_with('/');
    let mut segs: Vec<&str> = if absolute { Vec::new() } else { vec!["word"] };
    for seg in decoded.trim_start_matches('/').split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segs.pop();
            }
            s => segs.push(s),
        }
    }
    segs.join("/")
}

/// 最小 `%XX` 解碼（只處理 ASCII hex；非法序列原樣保留，不 panic）。
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    if !b.contains(&b'%') {
        return s.to_string();
    }
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hi = (b[i + 1] as char).to_digit(16);
            let lo = (b[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    // 解碼後非法 UTF-8（畸形 Target）：退回原字串，老實不崩潰。
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key {
            crate::qx::decode_and_unescape_attr(&a)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
  <Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer2.xml"/>
  <Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/x" TargetMode="External"/>
  <Relationship Id="rId10" Type="x" Target="../customXml/item1.xml"/>
</Relationships>"#;

    fn rels() -> Vec<Relationship> {
        read_rels(RELS)
    }

    #[test]
    fn resolves_header_and_footer_targets() {
        let r = rels();
        assert_eq!(
            resolve_relationship(&r, "rId7").as_deref(),
            Some("word/header1.xml")
        );
        assert_eq!(
            resolve_relationship(&r, "rId8").as_deref(),
            Some("word/footer2.xml")
        );
        assert_eq!(
            resolve_relationship(&r, "rId1").as_deref(),
            Some("word/styles.xml")
        );
    }

    /// rId 缺失 → None（呼叫端據此容錯跳過）。
    #[test]
    fn missing_rid_is_none() {
        let r = rels();
        assert_eq!(resolve_relationship(&r, "rId999"), None);
    }

    /// External 目標（超連結）不是 part → None。
    #[test]
    fn external_target_is_none() {
        let r = rels();
        assert_eq!(resolve_relationship(&r, "rId9"), None);
    }

    /// `../` 折回與 `%XX` 解碼。
    #[test]
    fn relative_parent_and_percent_targets() {
        let r = rels();
        assert_eq!(
            resolve_relationship(&r, "rId10").as_deref(),
            Some("customXml/item1.xml")
        );
        assert_eq!(
            normalize_part_path("media/my%20logo.png"),
            "word/media/my logo.png"
        );
        assert_eq!(normalize_part_path("/word/header3.xml"), "word/header3.xml");
        // 過度 `..`（逃離包装根）：pop 到空仍留在根內，不產生 "../" 路徑。
        assert_eq!(normalize_part_path("../../evil.xml"), "evil.xml");
    }

    /// 重複 Id：後者勝（畸形容錯）。
    #[test]
    fn duplicate_id_last_wins() {
        let xml = br#"<Relationships><Relationship Id="rId1" Target="a.xml"/><Relationship Id="rId1" Target="b.xml"/></Relationships>"#;
        let r = read_rels(xml);
        assert_eq!(
            resolve_relationship(&r, "rId1").as_deref(),
            Some("word/b.xml")
        );
    }
}
