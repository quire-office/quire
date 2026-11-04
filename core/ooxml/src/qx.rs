//! QX 私有小工具函式（quick-xml 0.37 -> 0.41 遷移等價封裝）。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。

use std::borrow::Cow;

/// 對 BytesText 執行 UTF-8 解碼與 XML 實體還原（0.37 BytesText::unescape 等價遷移）。
pub(crate) fn unescape_text<'a>(
    t: &'a quick_xml::events::BytesText<'a>,
) -> Result<Cow<'a, str>, String> {
    let decoded = t.decode().map_err(|e| e.to_string())?;
    match quick_xml::escape::unescape(&decoded) {
        Ok(Cow::Borrowed(_)) => Ok(decoded),
        Ok(Cow::Owned(s)) => Ok(Cow::Owned(s)),
        Err(e) => Err(e.to_string()),
    }
}

/// 解析 GeneralRef 實體參照（如 &amp;, &lt;, &#10;, &#x2019; 等）。
pub(crate) fn resolve_general_ref(r: &quick_xml::events::BytesRef) -> Result<String, String> {
    if r.is_char_ref() {
        if let Some(ch) = r.resolve_char_ref().map_err(|e| e.to_string())? {
            return Ok(ch.to_string());
        }
    }
    let name = r.decode().map_err(|e| e.to_string())?;
    if let Some(s) = quick_xml::escape::resolve_predefined_entity(&name) {
        return Ok(s.to_string());
    }
    let formatted = format!("&{};", name);
    quick_xml::escape::unescape(&formatted)
        .map(|s| s.into_owned())
        .map_err(|e| e.to_string())
}

/// 取得 UTF-8 解碼器（對齊 Decoder::utf8() 語義）。
#[inline]
pub(crate) fn utf8_decoder() -> quick_xml::Decoder {
    quick_xml::Reader::from_str("").decoder()
}

/// 對 Attribute 執行 UTF-8 解碼、正規化與實體還原（0.37 unescape_value 等價遷移）。
#[allow(deprecated)]
pub(crate) fn decode_and_unescape_attr(
    a: &quick_xml::events::attributes::Attribute,
) -> Option<String> {
    a.decode_and_unescape_value(utf8_decoder())
        .ok()
        .map(|v| v.into_owned())
}
