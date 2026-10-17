//! quire-ooxml —— DOCX(zip+OOXML) 讀取＋無損存回。
//!
//! 定位：這不是完整的 ECMA-376 模型，而是「無損 round-trip」路線（DOCX 為 canonical
//! model，未觸碰的 part 原樣保留）的最小可行起點。完整規格覆蓋是本專案本體，
//! 依「真實文件 corpus 驅動」逐項推進（見研究報告 §7）。
//!
//! 無損的實作分兩層：
//! 1. [`zip_struct`]——zip 結構自己解析／組裝，未觸碰 entry 搬運原始位元組；
//! 2. [`OoxmlDoc::save`]——把 parts map 與 bundle 逐一比對，只有真的改過的 entry
//!    才重新壓縮，其餘逐位元組沿用。開啟→不動→存檔 = byte-identical。

mod diff;
mod sectpr;
mod table;
mod zip_struct;

pub use diff::{diff_untouched, diff_zip_structure, EntryDiff, ZipDiff, ZipStructureError};
pub use sectpr::{read_sections_per_paragraph, SectionGeometry, SectionSpec};
pub use table::{
    Block, BodyTable, Border, Borders, CellProps, GridCol, Margins, RowProps, Shading, Table,
    TableCell, TableProps, TableRow, TblWidth,
};
pub use zip_struct::{Change, ZipBundle, ZipEntry, ZipError};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OoxmlError {
    #[error("zip 解析失敗: {0}")]
    Zip(String),
    #[error("找不到 word/document.xml，不是有效的 .docx")]
    MissingDocument,
    #[error("XML 解析失敗: {0}")]
    Xml(String),
}

impl From<ZipError> for OoxmlError {
    fn from(e: ZipError) -> Self {
        OoxmlError::Zip(e.to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    /// 文字顏色（0xAARRGGBB）。`w:color/@w:val="auto"` 或缺省時為 `None`（發射時預設黑）。
    #[serde(default)]
    pub color: Option<u32>,
    /// 底線（`w:u` 且 `val != "none"`，或 `w:hyperlink` 內無顯式樣式時預設開啟）。
    #[serde(default)]
    pub underline: bool,
    /// 刪除線（`w:strike`）。
    #[serde(default)]
    pub strike: bool,
}

/// 超連結內 run 無顯式顏色時的預設藍色（Word Hyperlink 樣式標準色 #0563C1）。
pub const HYPERLINK_DEFAULT_COLOR_ARGB: u32 = 0xFF05_63C1;

/// 解析 OOXML `<w:color w:val="..."/>` 色值字串為 `0xAARRGGBB`。
///
/// - 6 位 hex（`RRGGBB`）→ 補齊不透明 alpha `0xFF00_0000 | rgb`；
/// - 8 位 hex（`AARRGGBB`）→ 直接解析為 `u32`；
/// - `"auto"`（不分大小寫）或非法字串 → `None`（發射至顯示列表時轉為黑 `0xFF00_0000`）。
/// - 註：`w:themeColor` 佈景主題色暫不解析（誠實近似；若元素同時帶 6/8 位 `w:val`
///   則直接採用該預計算色值）。
pub fn parse_color_val(val: &str) -> Option<u32> {
    let s = val.trim();
    if s.eq_ignore_ascii_case("auto") {
        return None;
    }
    if !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match s.len() {
        6 => u32::from_str_radix(s, 16).ok().map(|rgb| 0xFF00_0000 | rgb),
        8 => u32::from_str_radix(s, 16).ok(),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Paragraph {
    pub style: Option<String>,
    pub runs: Vec<Run>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OoxmlDoc {
    pub paragraphs: Vec<Paragraph>,
    /// body 級表格的 canonical 投影（只讀；寫回來源仍是 parts 明文，
    /// 見 [`table`] 模組頭「唯讀來源隔離」）。段落以索引引用 `paragraphs`。
    pub tables: Vec<BodyTable>,
    /// 全部 zip part 的明文（解壓後）——編輯面的工作副本；save() 以此為準寫回。
    pub parts: BTreeMap<String, Vec<u8>>,
    /// zip 結構快照：存回時未觸碰 entry 原樣搬運的依據。
    pub bundle: ZipBundle,
}

impl OoxmlDoc {
    /// 每個段落的純文字（供給版面層）。
    pub fn paragraph_texts(&self) -> Vec<String> {
        self.paragraphs
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect()
    }

    /// 覆寫／新增一個 part 的明文（供編輯路徑；存回時才真正壓縮寫入）。
    pub fn set_part(&mut self, name: &str, bytes: Vec<u8>) {
        self.parts.insert(name.to_string(), bytes);
    }

    /// 標記刪除一個 part（凍結 entry 会在存檔時被忽略，見 zip_struct 凍結規則）。
    pub fn remove_part(&mut self, name: &str) -> bool {
        self.parts.remove(name).is_some()
    }

    /// 存檔：parts 與 bundle 逐 entry 比對，只有內容真的變了的 entry 才重建，
    /// 其餘（含壓縮位元組、extra field、entry comment、data descriptor gap）
    /// 逐位元組沿用原始 archive。parts 未動時輸出與輸入位元組完全相同。
    pub fn save(&self) -> Result<Vec<u8>, OoxmlError> {
        let mut changes: BTreeMap<String, Change> = BTreeMap::new();
        let mut extras: BTreeMap<String, Vec<u8>> = BTreeMap::new();

        for e in self.bundle.file_entries() {
            let key = e.key();
            match self.parts.get(&key) {
                None => {
                    changes.insert(key, Change::Remove);
                }
                Some(bytes) => {
                    // 先比壓縮位元組（未觸碰最常見的捷徑），再比明文。
                    if e.method == zip_struct::METHOD_STORE && bytes == &e.data_raw {
                        continue;
                    }
                    let original = e.decoded()?;
                    if bytes != &original {
                        changes.insert(key, Change::Replace(bytes.clone()));
                    }
                }
            }
        }
        // parts 裡新增的 key（bundle 沒有對應 entry）→ 追加。
        for (name, bytes) in &self.parts {
            if self.bundle.entry(name).is_none() {
                extras.insert(name.clone(), bytes.clone());
            }
        }
        Ok(self.bundle.serialize(&changes, &extras)?)
    }
}

/// 由 .docx 位元組解析（不碰檔案系統，wasm32 友善）。
pub fn parse(bytes: &[u8]) -> Result<OoxmlDoc, OoxmlError> {
    let bundle = ZipBundle::open(bytes)?;
    let mut parts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for e in bundle.file_entries() {
        parts.insert(e.key(), e.decoded()?);
    }
    let xml = parts
        .get("word/document.xml")
        .ok_or(OoxmlError::MissingDocument)?;
    let paragraphs = parse_document_xml(xml)?;
    // 表格投影：純讀入（PM 補正 1——唯讀來源隔離；無損寫回路徑完全不經此处）。
    let tables = table::read_body_tables(xml);
    Ok(OoxmlDoc {
        paragraphs,
        tables,
        parts,
        bundle,
    })
}

/// 解析 OOXML 開關型或底線屬性（`w:b`、`w:i`、`w:strike`、`w:u`）：
/// 無 `w:val` 時視為開啟（`true`）；有 `w:val` 時若為 `"none"`/`"false"`/`"0"`/`"off"`
/// （不分大小寫）則為關閉（`false`），其餘（如 `"true"`/`"1"`/`"on"`/`"single"`）為開啟。
fn toggle_attr_on(e: &quick_xml::events::BytesStart) -> bool {
    match attr_val(e, "val") {
        None => true,
        Some(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "none" | "false" | "0" | "off"
        ),
    }
}

/// 套用段落或 run 內部的樣式標籤（Start 與 Empty 事件共用）。
fn apply_style_tag(
    name: &[u8],
    e: &quick_xml::events::BytesStart,
    cur_para: &mut Option<Paragraph>,
    cur_run: &mut Option<Run>,
    run_has_color: &mut bool,
    run_has_underline: &mut bool,
) {
    match name {
        b"b" => {
            if let Some(r) = cur_run.as_mut() {
                r.bold = toggle_attr_on(e);
            }
        }
        b"i" => {
            if let Some(r) = cur_run.as_mut() {
                r.italic = toggle_attr_on(e);
            }
        }
        b"u" => {
            if let Some(r) = cur_run.as_mut() {
                *run_has_underline = true;
                r.underline = toggle_attr_on(e);
            }
        }
        b"strike" => {
            if let Some(r) = cur_run.as_mut() {
                r.strike = toggle_attr_on(e);
            }
        }
        b"color" => {
            if let Some(r) = cur_run.as_mut() {
                // themeColor 暫不解析（誠實近似）：僅解析 w:val 的 6/8 位 hex，auto→None。
                let parsed = attr_val(e, "val").and_then(|v| parse_color_val(&v));
                *run_has_color = parsed.is_some();
                r.color = parsed;
            }
        }
        b"pStyle" => {
            if let Some(p) = cur_para.as_mut() {
                if let Some(v) = attr_val(e, "val") {
                    p.style = Some(v);
                }
            }
        }
        _ => {}
    }
}

/// 最小 word/document.xml 解析器：段落 / run / 文字 / 粗斜體 / 顏色 / 底線 / 刪除線 / 超連結 / 段落樣式。
/// 骨架階段刻意不求全面（欄位、圖形皆未處理），演進順序由 corpus 綠燈率驅動。
fn parse_document_xml(xml: &[u8]) -> Result<Vec<Paragraph>, OoxmlError> {
    use quick_xml::events::Event;
    let text = String::from_utf8_lossy(xml).into_owned();
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut out: Vec<Paragraph> = Vec::new();
    let mut cur_para: Option<Paragraph> = None;
    let mut cur_run: Option<Run> = None;
    let mut run_has_color = false;
    let mut run_has_underline = false;
    let mut hyperlink_depth = 0usize;
    let mut in_t = false;

    loop {
        match reader
            .read_event()
            .map_err(|e| OoxmlError::Xml(e.to_string()))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"p" => {
                        cur_para = Some(Paragraph::default());
                        cur_run = None;
                        hyperlink_depth = 0;
                    }
                    b"hyperlink" => {
                        hyperlink_depth += 1;
                    }
                    b"r" => {
                        cur_run = Some(Run::default());
                        run_has_color = false;
                        run_has_underline = false;
                    }
                    b"t" => in_t = true,
                    other => apply_style_tag(
                        other,
                        &e,
                        &mut cur_para,
                        &mut cur_run,
                        &mut run_has_color,
                        &mut run_has_underline,
                    ),
                }
            }
            Event::Empty(e) => {
                let name = e.local_name().as_ref().to_vec();
                apply_style_tag(
                    name.as_slice(),
                    &e,
                    &mut cur_para,
                    &mut cur_run,
                    &mut run_has_color,
                    &mut run_has_underline,
                );
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"t" => in_t = false,
                    b"hyperlink" => {
                        hyperlink_depth = hyperlink_depth.saturating_sub(1);
                    }
                    b"r" => {
                        if let (Some(mut run), Some(p)) = (cur_run.take(), cur_para.as_mut()) {
                            // <w:hyperlink> 內 run 無顯式樣式時預設藍（0xFF0563C1）＋底線。
                            if hyperlink_depth > 0 {
                                if !run_has_color && run.color.is_none() {
                                    run.color = Some(HYPERLINK_DEFAULT_COLOR_ARGB);
                                }
                                if !run_has_underline {
                                    run.underline = true;
                                }
                            }
                            p.runs.push(run);
                        }
                    }
                    b"p" => {
                        if let Some(p) = cur_para.take() {
                            out.push(p);
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if let Some(r) = cur_run.as_mut().filter(|_| in_t) {
                    let s = t.unescape().map_err(|e| OoxmlError::Xml(e.to_string()))?;
                    r.text.push_str(&s);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn attr_val(e: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key.as_bytes() {
            a.unescape_value().ok().map(|v| v.to_string())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = include_bytes!("../../../fixtures/sample.docx");

    #[test]
    fn parses_sample_docx() {
        let doc = parse(SAMPLE).expect("解析 fixtures/sample.docx 失敗");
        assert!(doc.parts.contains_key("[Content_Types].xml"));
        assert!(doc.parts.contains_key("word/document.xml"));
        assert!(!doc.paragraphs.is_empty());
        assert!(doc
            .paragraphs
            .iter()
            .any(|p| p.style.as_deref() == Some("Title")));
        assert!(doc
            .paragraphs
            .iter()
            .any(|p| p.runs.iter().any(|r| r.bold && r.text.contains("Quire"))));
        // &amp; 實體必須還原成 &
        assert!(doc
            .paragraphs
            .iter()
            .any(|p| p.runs.iter().any(|r| r.text.contains("R&D"))));
    }

    #[test]
    fn rejects_non_docx() {
        assert!(parse(b"not a zip at all").is_err());
    }

    #[test]
    fn open_then_untouched_save_is_byte_identical() {
        let doc = parse(SAMPLE).expect("open");
        let out = doc.save().expect("save");
        assert_eq!(out.as_slice(), SAMPLE, "開啟→不動→存檔必須位元組級相同");
    }

    #[test]
    fn edited_part_round_trips_without_disturbing_others() {
        let mut doc = parse(SAMPLE).expect("open");
        doc.set_part("word/footer1.xml", b"<footer>new</footer>".to_vec());
        let out = doc.save().expect("save");
        let again = parse(&out).expect("re-open");
        assert_eq!(
            again.parts.get("word/footer1.xml").unwrap(),
            b"<footer>new</footer>"
        );
        // 未觸碰的 part 明文一致、壓縮位元組一致
        for name in ["[Content_Types].xml", "word/document.xml", "_rels/.rels"] {
            assert_eq!(again.parts[name], doc.parts[name], "{name} 明文應不變");
            assert_eq!(
                again.bundle.entry(name).unwrap().data_raw,
                doc.bundle.entry(name).unwrap().data_raw,
                "{name} 壓縮位元組應不變"
            );
        }
    }

    #[test]
    fn part_removal_drops_only_that_entry() {
        let mut doc = parse(SAMPLE).expect("open");
        assert!(doc.remove_part("_rels/.rels"));
        let out = doc.save().expect("save");
        let again = parse(&out).expect("re-open");
        assert!(!again.parts.contains_key("_rels/.rels"));
        assert!(again.parts.contains_key("word/document.xml"));
    }

    #[test]
    fn color_val_parses_hex6_hex8_auto_and_rejects_invalid() {
        // 6 位 hex → 補齊 0xFF 不透明 alpha
        assert_eq!(parse_color_val("FF0000"), Some(0xFFFF_0000));
        assert_eq!(parse_color_val("0563c1"), Some(0xFF05_63C1));
        assert_eq!(parse_color_val("000000"), Some(0xFF00_0000));
        // 8 位 ARGB → 原樣保留 alpha 與 RGB
        assert_eq!(parse_color_val("80FF0000"), Some(0x80FF_0000));
        assert_eq!(parse_color_val("00112233"), Some(0x0011_2233));
        // auto → None（發射時黑）
        assert_eq!(parse_color_val("auto"), None);
        assert_eq!(parse_color_val("AUTO"), None);
        // 非法值 → None
        assert_eq!(parse_color_val(""), None);
        assert_eq!(parse_color_val("12345"), None);
        assert_eq!(parse_color_val("1234567"), None);
        assert_eq!(parse_color_val("GGGGGG"), None);
        assert_eq!(parse_color_val("#FF0000"), None);
        assert_eq!(parse_color_val("+12345"), None);
    }

    #[test]
    fn parses_run_styles_and_hyperlink_defaults() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r>
                <w:rPr>
                  <w:b w:val="true"/>
                  <w:i w:val="1"/>
                  <w:u w:val="single"/>
                  <w:strike/>
                  <w:color w:val="FF0000"/>
                </w:rPr>
                <w:t>紅粗斜底刪</w:t>
              </w:r>
              <w:r>
                <w:rPr>
                  <w:b w:val="0"/>
                  <w:i w:val="false"/>
                  <w:u w:val="none"/>
                  <w:strike w:val="off"/>
                  <w:color w:val="auto"/>
                </w:rPr>
                <w:t>全關與自動色</w:t>
              </w:r>
              <w:hyperlink>
                <w:r>
                  <w:t>預設超連結</w:t>
                </w:r>
                <w:r>
                  <w:rPr>
                    <w:color w:val="00FF00"/>
                    <w:u w:val="none"/>
                  </w:rPr>
                  <w:t>覆寫超連結</w:t>
                </w:r>
              </w:hyperlink>
            </w:p>
          </w:body>
        </w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析樣式 XML 失敗");
        assert_eq!(paras.len(), 1);
        let runs = &paras[0].runs;
        assert_eq!(runs.len(), 4);

        assert!(runs[0].bold && runs[0].italic && runs[0].underline && runs[0].strike);
        assert_eq!(runs[0].color, Some(0xFFFF_0000));

        assert!(!runs[1].bold && !runs[1].italic && !runs[1].underline && !runs[1].strike);
        assert_eq!(runs[1].color, None);

        assert_eq!(runs[2].color, Some(HYPERLINK_DEFAULT_COLOR_ARGB));
        assert!(runs[2].underline);

        assert_eq!(runs[3].color, Some(0xFF00_FF00));
        assert!(!runs[3].underline);
    }
}
