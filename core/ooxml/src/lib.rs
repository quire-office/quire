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

/// 最小 word/document.xml 解析器：段落 / run / 文字 / 粗斜體 / 段落樣式。
///
/// 扁平段落序列的索引口徑（與 `table.rs`、`sectpr.rs` 三解析器對齊的唯一真相）：
/// - 每個非自閉合 `w:p` 的 **Start** 佔一個槽位（Start 預位、End 回填完成品）
///   ——扁平索引＝Start 序，巢狀 `w:p`（`w:txbxContent` 文字框段落）不改寫
///   外層槽位，外層段落於文字框前後的文字完整保留；
/// - `mc:Fallback` 子樹整體吞沒：`mc:AlternateContent` 的 Choice（DrawingML）
///   與 Fallback（VML）是同一內容的雙份表示，只解析 Choice 一份，圖形文字框
///   段落不再出現雙份（對照 `table.rs` `Frame::Skip` 同款做法；其內段落與
///   table／sectpr 同口徑——不計）。
///
/// 骨架階段刻意不求全面（欄位、圖形幾何皆未處理），演進順序由 corpus 綠燈率驅動。
fn parse_document_xml(xml: &[u8]) -> Result<Vec<Paragraph>, OoxmlError> {
    use quick_xml::events::Event;
    let text = String::from_utf8_lossy(xml).into_owned();
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut out: Vec<Paragraph> = Vec::new();
    // 段落棧：(out 槽位, 半成品)。Start push、End pop 回填——巢狀 w:p 各自
    // 獨立，內層完結不影響外層（舊實作單一 cur_para 會被內層覆寫＋清空，
    // 外層文字全吞；corpus 實測 25.5% 檔案受害）。
    let mut stack: Vec<(usize, Paragraph)> = Vec::new();
    let mut cur_run: Option<Run> = None;
    let mut in_t = false;
    // >0＝正在吞 mc:Fallback 子樹（巢狀 Fallback 以開閉計數自行平衡）。
    let mut fb: usize = 0;
    // mc:AlternateContent 巢狀棧：開標時快照 fb，閉標時還原——錯置標籤
    // （AC 先於 Fallback 閉合／Fallback 漏閉）不致 fb 卡死而吞掉檔尾。
    let mut ac_stack: Vec<usize> = Vec::new();

    loop {
        match reader
            .read_event()
            .map_err(|e| OoxmlError::Xml(e.to_string()))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_vec();
                if fb > 0 {
                    // Fallback 子樹內：只維護巢狀深度與 AC 快照，其餘一概不看。
                    if name == b"Fallback" {
                        fb += 1;
                    } else if name == b"AlternateContent" {
                        ac_stack.push(fb);
                    }
                    continue;
                }
                match name.as_slice() {
                    b"p" => {
                        // Start 即預位：扁平索引＝Start 序（口徑鐵律，見函式頭注）。
                        stack.push((out.len(), Paragraph::default()));
                        out.push(Paragraph::default());
                        cur_run = None;
                    }
                    b"r" => cur_run = Some(Run::default()),
                    b"t" => in_t = true,
                    b"pStyle" => {
                        if let Some((_, p)) = stack.last_mut() {
                            if let Some(v) = attr_val(&e, "val") {
                                p.style = Some(v);
                            }
                        }
                    }
                    b"Fallback" => fb += 1,
                    b"AlternateContent" => ac_stack.push(fb),
                    _ => {}
                }
            }
            Event::Empty(e) => {
                if fb > 0 {
                    continue;
                }
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"b" => {
                        if let Some(r) = cur_run.as_mut() {
                            r.bold = true;
                        }
                    }
                    b"i" => {
                        if let Some(r) = cur_run.as_mut() {
                            r.italic = true;
                        }
                    }
                    b"pStyle" => {
                        if let Some((_, p)) = stack.last_mut() {
                            if let Some(v) = attr_val(&e, "val") {
                                p.style = Some(v);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_vec();
                // MC 標籤完結先行處理（即便在 Fallback 內）：錯置標籤
                // （AC 先於 Fallback 閉合）不致 fb 卡死而吞掉檔尾。
                if name == b"AlternateContent" {
                    fb = ac_stack.pop().unwrap_or(0);
                    continue;
                }
                if fb > 0 {
                    if name == b"Fallback" {
                        fb = fb.saturating_sub(1);
                    }
                    continue;
                }
                match name.as_slice() {
                    b"t" => in_t = false,
                    b"r" => {
                        if let (Some(run), Some((_, p))) = (cur_run.take(), stack.last_mut()) {
                            p.runs.push(run);
                        }
                    }
                    b"p" => {
                        // 完結回填 Start 時預留的槽位；無配對 Start（錯置標籤）＝忽略。
                        // 未閉合到 EOF 的段落保持預位的空段落——Start 口徑不因此漂移。
                        if let Some((slot, p)) = stack.pop() {
                            out[slot] = p;
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if fb > 0 {
                    continue;
                }
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

    // ---------------- 解析器缺陷回歸（t_580c37ac） ----------------

    /// 缺陷 1 回歸：`mc:AlternateContent` 的 Choice（DrawingML）與 Fallback
    /// （VML）是同一內容的雙份表示——只解析 Choice 一份，段落不得雙份。
    #[test]
    fn alternate_content_fallback_parsed_once() {
        let xml = r#"<w:document xmlns:w="x" xmlns:mc="m" xmlns:wps="w">
            <w:body>
            <w:p><w:r><w:t>前</w:t></w:r></w:p>
            <mc:AlternateContent>
              <mc:Choice Requires="wps"><w:p><w:r><w:t>框內文</w:t></w:r></w:p></mc:Choice>
              <mc:Fallback><w:p><w:r><w:t>框內文</w:t></w:r></w:p></mc:Fallback>
            </mc:AlternateContent>
            <w:p><w:r><w:t>後</w:t></w:r></w:p>
            </w:body></w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析");
        let texts: Vec<String> = paras
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect();
        assert_eq!(
            texts,
            vec!["前", "框內文", "後"],
            "Choice/Fallback 同文只出一份段落"
        );
    }

    /// 缺陷 1 邊界：Fallback 內有樣板段落與 pStyle 也不得洩漏到扁平序列；
    /// 裸 Fallback（無 AC 包裝，畸形但存在）同樣整棵吞沒。
    #[test]
    fn fallback_content_never_leaks() {
        let xml = r#"<w:document xmlns:w="x" xmlns:mc="m">
            <w:body>
            <mc:Fallback>
              <w:p><w:pPr><w:pStyle w:val="BodyText"/></w:pPr><w:r><w:t>VML雙份</w:t></w:r></w:p>
            </mc:Fallback>
            <w:p><w:r><w:t>正文</w:t></w:r></w:p>
            </w:body></w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析");
        assert_eq!(paras.len(), 1, "裸 Fallback 內段落不進扁平序列");
        assert_eq!(paras[0].runs[0].text, "正文");
        assert!(paras[0].style.is_none());
    }

    /// 缺陷 2 回歸：巢狀 `w:p`（`w:txbxContent` 文字框段落）不改寫外層槽位
    /// ——外層段落在文字框前後的文字完整保留，內層段落各自佔位。
    #[test]
    fn nested_txbx_paragraph_keeps_outer_text() {
        let xml = r#"<w:document xmlns:w="x" xmlns:mc="m" xmlns:wps="w" xmlns:wp="wp">
            <w:body>
            <w:p>
              <w:r><w:t>框前文</w:t></w:r>
              <w:r><mc:AlternateContent>
                <mc:Choice Requires="wps"><w:drawing><wp:anchor><wps:txbx>
                  <w:txbxContent><w:p><w:r><w:t>框內文</w:t></w:r></w:p></w:txbxContent>
                </wps:txbx></wp:anchor></w:drawing></mc:Choice>
                <mc:Fallback><w:pict><v:shape xmlns:v="v"><w:txbxContent>
                  <w:p><w:r><w:t>框內文</w:t></w:r></w:p></w:txbxContent></v:shape></w:pict></mc:Fallback>
              </mc:AlternateContent></w:r>
              <w:r><w:t>框後文</w:t></w:r>
            </w:p>
            </w:body></w:document>"#
            .as_bytes();
        let paras = parse_document_xml(xml).expect("解析");
        let texts: Vec<String> = paras
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect();
        assert_eq!(
            texts,
            vec!["框前文框後文", "框內文"],
            "Start 序：外層先佔位且前後文完整；內層文字框段落另佔一槽（Choice 一份）"
        );
    }

    /// 缺陷 2 的段落索引口徑：巢狀段落以 Start 預位——索引＝Start 序，
    /// 外層 End 回填不位移任何既有槽位（table.rs／sectpr.rs 引用同一口徑）。
    #[test]
    fn nested_paragraph_uses_start_slot_order() {
        let xml = br#"<w:body>
            <w:p><w:r><w:t>A</w:t></w:r>
              <w:p><w:r><w:t>B</w:t></w:r></w:p>
              <w:p><w:r><w:t>C</w:t></w:r></w:p>
            <w:r><w:t>D</w:t></w:r></w:p>
            <w:p><w:r><w:t>E</w:t></w:r></w:p>
            </w:body>"#;
        let paras = parse_document_xml(xml).expect("解析");
        let texts: Vec<String> = paras
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect();
        assert_eq!(texts, vec!["AD", "B", "C", "E"], "扁平索引＝w:p Start 序");
    }

    /// corpus 段落數回歸（缺陷 1＋2 實據檔）：修前 Choice/Fallback 雙份解析
    /// ＋巢狀 w:p 覆寫吞字（舊口徑 LINKED 57 段雙份）；修後與獨立口徑
    /// （start 佔槽＋Fallback 吞沒）逐檔一致的 30 段。
    #[test]
    fn corpus_txbx_paragraph_counts_regression() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        let cases: [(&str, usize, &[&str]); 4] = [
            // (檔案, 修後段落數, 修前被吞／雙份、修後必須完整保留的文字)
            (
                "xopt/OpenXmlPowerToolsExamples_TextReplacer02_Test01.docx",
                4,
                &["the on the Insert tab"],
            ),
            // WC049：修前段數同為 8 但外層段「You can also type…」整段被內層
            // 覆寫吞字（缺陷 2 吞字指紋）；修後必須完整保留。
            (
                "xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx",
                8,
                &["You can also type a keyword"],
            ),
            (
                "ooxmlexport/LinkedTextBoxes.docx",
                30,
                &["Congratulations", "IA Scholarship Program Nominations"],
            ),
            // shapes-with-text：修前 10 段（VML 雙份），修後 6 段。
            (
                "poi/test-data_document_shapes-with-text.docx",
                6,
                &["Floating text box"],
            ),
        ];
        for (rel, want_n, must_keep) in cases {
            let bytes = std::fs::read(corpus.join(rel)).unwrap_or_else(|e| panic!("讀 {rel}: {e}"));
            let doc = parse(&bytes).expect("parse");
            let texts = doc.paragraph_texts();
            assert_eq!(texts.len(), want_n, "{rel}：段落數偏離修後基準");
            let joined = texts.join("\n");
            for frag in must_keep {
                assert!(
                    joined.contains(frag),
                    "{rel}：文字「{frag}」被吞（缺陷 2 復發？）"
                );
            }
        }
    }

    /// corpus 全檔柵欄：含 txbxContent／Fallback 的檔案，主解析器扁平段落數
    /// 必須等於表格解析器（獨立狀態機）的 Start 計數器——兩個實作共用以
    /// 上注「同口徑」為契約，此柵欄讓契約在 corpus 上逐檔可驗證。
    #[test]
    fn corpus_txbx_files_start_count_agrees() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        let manifest = std::fs::read_to_string(corpus.join("manifest.json")).expect("manifest");
        let mut checked = 0usize;
        for chunk in manifest.split("\"file\": \"").skip(1) {
            let rel = chunk.split('"').next().unwrap_or_default();
            if rel.is_empty() {
                continue;
            }
            let path = corpus.join(rel);
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(doc) = parse(&bytes) else { continue };
            let Some(xml) = doc.parts.get("word/document.xml") else {
                continue;
            };
            if !String::from_utf8_lossy(xml).contains("txbxContent") {
                continue;
            }
            let got = parse_document_xml(xml).expect("主解析").len();
            let want = crate::table::count_flat_paras(xml);
            assert_eq!(got, want, "{rel}：主解析器與表格計數器口徑分岔");
            checked += 1;
        }
        assert!(checked >= 50, "txbx 檔案數異常：{checked}");
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
}
