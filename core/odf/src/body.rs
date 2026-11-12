//! ODF 內文文字流最小模型抽取模組。
//!
//! 由 content.xml 抽取段落（<text:p>）與標題（<text:h>），
//! 累積段落計數與純文字流長度，並與 survey.json 數字核對。
//!
//! 空白政策（卡 t_4006056b）：文字節點（含 `Event::CData`）一律經 `crate::ws`
//! 的**共用**正規化＋gap 狀態機（與 `layout_glue::walk_paragraphs` 同一實作），
//! 務求 `extract_body()` 與 `project()` 對同一檔產出逐字元一致的段落文字。

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::manifest::Manifest;
use crate::ws::{self, TextStream};
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

    // 回塞旗標（`ODF_BODY_WS_BUGGY`）於進入走查時讀一次：與 layout 側的
    // `ODF_WS_BUGGY` 刻意分開（各扳各側，見 `ws::buggy_body_no_normalize`）。
    let buggy = ws::buggy_body_no_normalize();

    let mut paragraphs = Vec::new();
    // 進行中段落：與 `layout_glue::ParaCtx` 同吃 `ws::TextStream`（gap 狀態機
    // 只此一處實作），差異僅在 body 側不記 run 樣式帳。
    let mut p_stack: Vec<TextStream> = Vec::new();
    let mut text_p_count = 0usize;
    let mut text_h_count = 0usize;
    let mut total_text_len = 0usize;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let qname = e.name();
                if is_text_p(qname.as_ref()) {
                    text_p_count += 1;
                    p_stack.push(TextStream::new());
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
                            cur.push_explicit(&" ".repeat(count));
                        }
                        b"text:tab" | b"tab" => cur.push_explicit("\t"),
                        b"text:line-break" | b"line-break" => cur.push_explicit("\n"),
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
                        if buggy {
                            cur.push_raw(&unescaped);
                        } else {
                            ws::emit_text_node(cur, &unescaped, ws::BODY_PLAIN_STYLE);
                        }
                    }
                }
            }
            // CDATA（quick-xml 0.41 起是獨立事件）：視同文字節點走同一正規化
            // 路徑（卡 t_4006056b）。CDATA 段本就是在 XML 裡保留原文的容器，
            // 解析器不對其內容解實體 ⇒ **只 decode、不 unescape**，
            // `<![CDATA[甲 <b> & 乙]]>` 必須原樣入文。修前落 `_ => {}` 靜默丟棄。
            Ok(Event::CData(e)) => {
                // 回塞態比照舊路徑：CData 不入文（修前落 `_ => {}` 整段丟棄）
                // ⇒ 燈 4 靠「兩軌分岔」取得鑑別力。
                if !buggy {
                    if let Some(cur) = p_stack.last_mut() {
                        if let Ok(raw) = e.decode().map(|d| d.into_owned()) {
                            ws::emit_text_node(cur, &raw, ws::BODY_PLAIN_STYLE);
                        }
                    }
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if let Some(cur) = p_stack.last_mut() {
                    let resolved = resolve_general_ref(&e)?;
                    cur.push_text_node(&resolved);
                }
            }
            Ok(Event::End(e)) => {
                let qname = e.name();
                if is_text_p(qname.as_ref()) {
                    if let Some(stream) = p_stack.pop() {
                        let text = stream.into_text();
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

/// 解析一般實體參照（`&apos;`／`&quot;`／`&amp;`／`&#NN;`／`&#xNN;`）。
///
/// quick-xml 0.41 起一般實體不再併入 `Event::Text`，改發
/// `Event::GeneralRef`；字元參照走 `resolve_char_ref`，具名實體優先查
/// 預定義表，其餘組織回 `&name;` 交 `unescape` 解析。未知實體如實回
/// `OdfError::Xml`，不捏造替代字元（口徑同 `core/ooxml/src/qx.rs`）。
pub(crate) fn resolve_general_ref(r: &quick_xml::events::BytesRef<'_>) -> Result<String, OdfError> {
    if r.is_char_ref() {
        if let Some(ch) = r
            .resolve_char_ref()
            .map_err(|e| OdfError::Xml(e.to_string()))?
        {
            return Ok(ch.to_string());
        }
    }
    let name = r.decode().map_err(|e| OdfError::Xml(e.to_string()))?;
    if let Some(s) = quick_xml::escape::resolve_predefined_entity(&name) {
        return Ok(s.to_string());
    }
    let formatted = format!("&{};", name);
    quick_xml::escape::unescape(&formatted)
        .map(|s| s.into_owned())
        .map_err(|e| OdfError::Xml(e.to_string()))
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
