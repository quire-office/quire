//! ODF 內文文字流最小模型抽取模組。
//!
//! 由 content.xml 抽取段落（<text:p>）與標題（<text:h>），
//! 累積段落計數與純文字流長度，並與 survey.json 數字核對。

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::manifest::Manifest;
use crate::OdfError;

/// 內文段落最小模型
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OdfParagraph {
    pub text: String,
}

/// 內文文字流最小模型
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BodyModel {
    /// 段落清單（對齊 survey.json 之 text_p）
    pub paragraphs: Vec<OdfParagraph>,
    /// 文字總字元數
    pub total_text_len: usize,
    /// 段落總數（<text:p> 計數）
    pub text_p_count: usize,
    /// 標題總數（<text:h> 計數）
    pub text_h_count: usize,
}

/// 由 content.xml 位元組抽取最小內文模型。
/// ODF-E1：manifest 若宣告 content.xml 加密 → `Err(OdfError::Encrypted)`。
/// （`OdfBundle::extract_body` 已先行攔截；此處是直入純函數的第二道防線，
/// 口徑必須一致——寧可報錯，不得把密文投成零段。）
pub fn extract_body_from_bytes(
    content_bytes: &[u8],
    manifest: &Manifest,
) -> Result<BodyModel, OdfError> {
    if manifest.is_encrypted("content.xml") {
        return Err(OdfError::Encrypted {
            full_path: "content.xml".to_string(),
            algorithm: manifest
                .algorithm_of("content.xml")
                .unwrap_or(crate::ALGORITHM_UNSPECIFIED)
                .to_string(),
        });
    }

    let mut reader = Reader::from_reader(content_bytes);
    let mut buf = Vec::new();

    let mut paragraphs = Vec::new();
    let mut p_stack: Vec<String> = Vec::new();
    let mut text_p_count = 0usize;
    let mut text_h_count = 0usize;
    let mut total_text_len = 0usize;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let qname = e.name();
                if is_text_p(qname.as_ref()) {
                    text_p_count += 1;
                    p_stack.push(String::new());
                } else if is_text_h(qname.as_ref()) {
                    text_h_count += 1;
                }
            }
            Ok(Event::Empty(e)) => {
                let qname = e.name();
                if is_text_p(qname.as_ref()) {
                    text_p_count += 1;
                    paragraphs.push(OdfParagraph {
                        text: String::new(),
                    });
                } else if is_text_h(qname.as_ref()) {
                    text_h_count += 1;
                } else if !p_stack.is_empty() {
                    let cur = p_stack.last_mut().expect("段落堆疊非空");
                    match qname.as_ref() {
                        b"text:s" | b"s" => {
                            let count = parse_space_count(&e);
                            for _ in 0..count {
                                cur.push(' ');
                            }
                        }
                        b"text:tab" | b"tab" => cur.push('\t'),
                        b"text:line-break" | b"line-break" => cur.push('\n'),
                        _ => {}
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if let Some(cur) = p_stack.last_mut() {
                    if let Ok(unescaped) = e.decode().map_err(|e| e.to_string()).and_then(|d| {
                        quick_xml::escape::unescape(&d)
                            .map(|u| u.into_owned())
                            .map_err(|e| e.to_string())
                    }) {
                        cur.push_str(&unescaped);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let qname = e.name();
                if is_text_p(qname.as_ref()) {
                    if let Some(text) = p_stack.pop() {
                        total_text_len += text.chars().count();
                        paragraphs.push(OdfParagraph { text });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                // 若解析中途失敗（如非完整 XML），回報錯誤
                return Err(OdfError::Xml(e.to_string()));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(BodyModel {
        paragraphs,
        total_text_len,
        text_p_count,
        text_h_count,
    })
}

#[inline]
fn is_text_p(qname: &[u8]) -> bool {
    qname == b"text:p" || qname == b"p"
}

#[inline]
fn is_text_h(qname: &[u8]) -> bool {
    qname == b"text:h" || qname == b"h"
}

fn parse_space_count(e: &quick_xml::events::BytesStart<'_>) -> usize {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"c" {
            if let Ok(s) = std::str::from_utf8(&attr.value) {
                if let Ok(n) = s.parse::<usize>() {
                    return n;
                }
            }
        }
    }
    1
}
