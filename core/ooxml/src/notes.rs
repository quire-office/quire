//! 註腳／尾註解析（`word/footnotes.xml`、`word/endnotes.xml`；ECMA-376 §17.11）。
//!
//! 背景：註腳本體放在獨立 part（`w:footnote`／`w:endnote` 元素，各以 `w:id`
//! 與正文 `w:footnoteReference`／`w:endnoteReference` 對應）。render-lab 對
//! LO 基線時 LO 渲染了註腳、本引擎完全跳過，造成分頁位移——本卡先**讀進
//! 資料**，版面與發射（F2 卡）後續接手；本模組對顯示列表／測量**零消費**
//! （渲染輸出逐位元組零變化）。
//!
//! 容錯口徑同 [`crate::ThemeColors`]：缺 part／壞 XML ＝空彙總（老實不虛構），
//! 逐註取捨見 [`Notes::from_parts`] 與 [`parse_note_list`]。

use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::Paragraph;

/// 註腳 part 的慣例路徑（OPC 佈局：Word／LO 產出恆為此名）。
const FOOTNOTES_PART: &str = "word/footnotes.xml";
/// 尾註 part 的慣例路徑。
const ENDNOTES_PART: &str = "word/endnotes.xml";

/// 註的型別（`w:footnote`／`w:endnote` 的 `@w:type`；ECMA-376 §17.11.10/§17.11.11）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NoteKind {
    /// 一般註（`@w:type` 缺省＝此型；正文的引用都指向它）。
    #[default]
    Regular,
    /// 分隔線註（`separator`，Word 慣例 id=-1，`<w:separator/>` 繪上緣線）。
    Separator,
    /// 續接分隔線註（`continuationSeparator`，Word 慣例 id=0）。
    ContinuationSeparator,
}

/// 引用端型別（正文 `w:footnoteReference`／`w:endnoteReference` 的對應；
/// ECMA-376 §17.11.2）。與 [`NoteKind`] 分開——separator 型不會被引用，
/// 引用標記只承載「腳註／尾註」兩分法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteKind2 {
    /// `w:footnoteReference` → `word/footnotes.xml`。
    Footnote,
    /// `w:endnoteReference` → `word/endnotes.xml`。
    Endnote,
}

/// 一則註的本體（ECMA-376 §17.11.3 `w:footnote`／§17.11.8 `w:endnote` 同構）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// `@w:id`。一般註腳 ≥1；Word 約定 separator=-1、continuationSeparator=0
    /// （`w:id` 屬 `ST_DecimalNumber` 整數域，i64 值域足夠；越界字面直接
    /// 解析失敗＝老實略註，見 [`parse_note_list`] 取捨）。
    #[serde(default)]
    pub id: i64,
    /// `@w:type`；缺省＝[`NoteKind::Regular`]。**取捨**：其餘型別
    /// （`continuation` 等）老實歸 Regular——corpus 未見該型；不憑名字
    /// 虛擬分發，等 corpus 出現實檔再依實據擴充。
    #[serde(default)]
    pub kind: NoteKind,
    /// 註文字段落（複用正文同款 `w:p` 全解析；引用來源＝本檔自身，
    /// 段落索引只在單一註內自洽）。段內 `w:footnoteRef`／`w:endnoteRef`
    /// 自動編號符與 `w:separator` 等**現階段不進 run 序**（落 catch-all
    /// 被忽略）——F2 發射符號時再定義其投影，本卡不虛構文字。
    #[serde(default)]
    pub paragraphs: Vec<Paragraph>,
}

impl Note {
    /// 註文字的合併純文字（各段落 run 串接；空註＝空字串）。
    pub fn text(&self) -> String {
        self.paragraphs
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect::<String>()
    }
}

/// 一份文件的全部註（兩個 part 各掃一份）。
///
/// 缺 part／壞 XML ＝空 [`Notes`]，等同 [`Notes::default`]；單一註解不出
/// 段落時**仍保留該註**（`paragraphs` 空）——老實：註的存在是事實，文字
/// 解析失敗是能力界線，不虛構也不整註丟棄。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Notes {
    /// `word/footnotes.xml` 的全部 `w:footnote`。
    #[serde(default)]
    pub footnotes: Vec<Note>,
    /// `word/endnotes.xml` 的全部 `w:endnote`。
    #[serde(default)]
    pub endnotes: Vec<Note>,
}

impl Notes {
    /// 從整包 parts 解析兩個註 part（缺＝空；容錯口徑同
    /// [`crate::ThemeColors::from_parts`]）。
    ///
    /// 註 part 取法：**只用慣例路徑** `word/footnotes.xml`／
    /// `word/endnotes.xml`（Word／LO 產出恆為此名）；不走
    /// `document.xml.rels` 關聯解析——異名落點 → 老實略＝空彙總（見
    /// 模組末未做清單）。
    pub fn from_parts(parts: &BTreeMap<String, Vec<u8>>) -> Notes {
        // 註段落解析複用正文同款 `parse_document_xml`（w:themeColor 查表）
        // ——此處仿單次注入取一份主題色表（theme part 數 KB，重取成本
        // 可忽略），讓本函式簽名維持「一包 parts 進、彙總出」的形態。
        let theme = crate::ThemeColors::from_parts(parts);
        Notes {
            footnotes: parts
                .get(FOOTNOTES_PART)
                .map(|b| parse_note_list(b, b"footnote", &theme))
                .unwrap_or_default(),
            endnotes: parts
                .get(ENDNOTES_PART)
                .map(|b| parse_note_list(b, b"endnote", &theme))
                .unwrap_or_default(),
        }
    }

    /// 依 id 查一則腳註本體（**同 id 多註時首見為準**；查不到＝None，
    /// 不虛構預留位）。查詢不做 kind 過濾——separator(-1) 也查得到，
    /// 由呼叫端（F2）決定語義。
    pub fn footnote(&self, id: i64) -> Option<&Note> {
        self.footnotes.iter().find(|n| n.id == id)
    }

    /// 依 id 查尾註本體（同 [`Notes::footnote`] 口徑）。
    pub fn endnote(&self, id: i64) -> Option<&Note> {
        self.endnotes.iter().find(|n| n.id == id)
    }
}

/// run 上的引用標記（ECMA-376 §17.11.2：正文 run 內 `w:footnoteReference`
/// ／`w:endnoteReference`）。語義同 [`crate::FieldKind`] 的標記做法：
/// 解析期標出「這個 run 是引用錨點」（Word 產出恆為專屬空文字 run），
/// 呈現層（F2）依此放註腳／尾註符號；本卡渲染／測量零消費，存檔路徑
/// 不經它（parts passthrough），serde default＋skip 保舊快照相容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRef {
    /// 腳註或尾註。
    pub kind: NoteKind2,
    /// `@w:id`（與 [`Note`] 的 id 同一命名空間：同一 kind 維度內唯一）。
    pub id: i64,
}

/// 正在掃描的一個註元素（[`parse_note_list`] 的內部狀態）。
///
/// `detail`＝`Some((id, kind, start))` 正常開注（start＝
/// `<w:footnote …>` 後一個位元組＝孩子的起點），`None`＝id 不可解的
/// 整註略過（依舊吞到配對 End 才回 Outside，日後同名再開不串味）；
/// `depth`＝同名巢深（按規範不巢套，開閉計數護欄只防畸形檔誤切 slice）。
struct OpenNote {
    detail: Option<(i64, NoteKind, usize)>,
    depth: usize,
}

/// 解析一個註 part：`w:footnotes`／`w:endnotes` 根下的同名 `w:footnote`
/// ／`w:endnote` 子元素，逐註一則 [`Note`]。
///
/// 段落解析**複用正文解析路徑** [`crate::parse_document_xml`]：先用
/// quick_xml（byte 級 Reader，位置＝原始位元組偏移）把每註**孩子的
/// 位元組範圍**（不含外包 `w:footnote` 標籤——留下的 fragment 仍良構，
/// 正文解析器可原樣吃）定位，再逐註重推正文解析器。同一套 `w:p` 佔槽
/// ／mc:Fallback／巢狀段落口徑，不另造第二台段落狀態機。
///
/// 取捨（病態輸入，樣樣不炸）：
/// - 進註元素時 `@w:id` 缺失或非整數 → 整註老實略過（無法被引用，卻
///   會干擾 id 查找語義）；
/// - 同名元素按規範不巢套，仍以開閉計數護欄，畸形檔不誤切 slice；
/// - XML 中途壞掉 → 停在已收部分（同 `ThemeColors::parse` 容錯口徑）；
/// - 單註 fragment 重解析失敗 → 該註照留、`paragraphs` 空（能力界線）。
fn parse_note_list(xml: &[u8], tag: &[u8], theme: &crate::ThemeColors) -> Vec<Note> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut out: Vec<Note> = Vec::new();
    // 進行中的註（None＝Outside：不在任何註元素內）。
    let mut open: Option<OpenNote> = None;
    // read_event 之前的位置＝上一事件結束點；End 事件到達時，註 fragment
    // 的右界就是它（**不含** `</w:footnote>` 本身——留下的 fragment 可
    // 原樣交給正文解析器）。
    let mut pos_before: usize;
    loop {
        pos_before = reader.buffer_position() as usize;
        let event = match reader.read_event() {
            Ok(ev) => ev,
            // 壞 XML → 停在已收部分（同 ThemeColors::parse 容錯口徑）。
            Err(_) => break,
        };
        match event {
            Event::Eof => break,
            Event::Start(e) if e.local_name().as_ref() == tag => match &mut open {
                None => {
                    let kind = kind_of(crate::attr_val(&e, "type").as_deref());
                    match crate::attr_val(&e, "id")
                        .as_deref()
                        .and_then(|v| v.trim().parse::<i64>().ok())
                    {
                        Some(id) => {
                            open = Some(OpenNote {
                                detail: Some((id, kind, pos_before)),
                                depth: 1,
                            });
                        }
                        None => {
                            open = Some(OpenNote {
                                detail: None,
                                depth: 1,
                            });
                        }
                    }
                }
                Some(inner) => inner.depth += 1,
            },
            Event::End(e) if e.local_name().as_ref() == tag => {
                if let Some(inner) = &mut open {
                    if inner.depth == 1 {
                        if let Some((id, kind, start)) = inner.detail.take() {
                            let frag = &xml[start..pos_before.min(xml.len())];
                            let paragraphs =
                                crate::parse_document_xml(frag, theme).unwrap_or_default();
                            out.push(Note {
                                id,
                                kind,
                                paragraphs,
                            });
                        }
                        // 細節 None＝整註略過：本就無事可收，歸位即可。
                        open = None;
                    } else {
                        inner.depth -= 1;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// `@w:type` → [`NoteKind`]：`separator`／`continuationSeparator` 對應；
/// 缺省與其餘值都歸 Regular（取捨見 [`Note::kind`] 欄注）。
fn kind_of(type_attr: Option<&str>) -> NoteKind {
    match type_attr.map(str::trim) {
        Some("separator") => NoteKind::Separator,
        Some("continuationSeparator") => NoteKind::ContinuationSeparator,
        _ => NoteKind::Regular,
    }
}

// ---- 未做清單（誠實 TODO，本卡刻意不實作） ----
//
// - **`document.xml.rels` 關聯解析**：註 part 只按慣例路徑找；不符
//   （罕見）→ 空彙總＝老實不虛構。
// - **`w:footnoteRef`／`w:endnoteRef`／`w:separator`（註內符號）**：
//   不進 run 序；F2 發射符號時定義投影。
// - **`w:footnotePr`／`w:endnotePr`**（settings/sectPr 上的編號格式、
//   重啟、位置設定）：本卡不收，F2 需要時再依 corpus 實據擴充。
// - **`w:continuousBookmarks`／`w:bookmarkStart` 內嵌**：註內書籤
//   跑正文解析器自然落在 paragraphs；本卡不另行投影。

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn corpus() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
    }

    fn parse_corpus(rel: &str) -> crate::OoxmlDoc {
        let bytes = std::fs::read(corpus().join(rel)).unwrap_or_else(|e| panic!("讀 {rel}: {e}"));
        crate::parse(&bytes).expect("解析 corpus 檔")
    }

    // ---------------- (a/f) 實檔註本體 ----------------

    /// HC031：footnotes.xml 含 Separator(-1)／ContinuationSeparator(0)／
    /// Regular(1)，Regular 文字含「This is a footnote」；endnotes 同構。
    #[test]
    fn hc031_notes_from_parts_well() {
        let doc = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let notes = Notes::from_parts(&doc.parts);
        assert_eq!(notes.footnotes.len(), 3, "三則：-1/0/1");
        assert!(notes
            .footnotes
            .iter()
            .any(|n| n.id == -1 && n.kind == NoteKind::Separator));
        assert!(notes
            .footnotes
            .iter()
            .any(|n| n.id == 0 && n.kind == NoteKind::ContinuationSeparator));
        let f1 = notes.footnote(1).expect("Regular 腳註 id=1");
        assert_eq!(f1.kind, NoteKind::Regular);
        assert!(
            f1.text().contains("This is a footnote"),
            "腳註文字：{:?}",
            f1.text()
        );
        assert_eq!(notes.endnotes.len(), 3, "尾註同構三則");
        let e1 = notes.endnote(1).expect("尾註 id=1");
        assert_eq!(e1.kind, NoteKind::Regular);
        assert!(e1.text().contains("This is an endnote"));
    }

    /// POI 表單檔：separator id 從 0 起算、Regular(2)；正文恰 1 個
    /// Footnote(2) 引用。驗證非 -1/0/1 慣例佈局的檔也走同一口徑。
    #[test]
    fn poi_form_footnotes_well() {
        let doc = parse_corpus("poi/test-data_document_form_footnotes.docx");
        let notes = Notes::from_parts(&doc.parts);
        assert_eq!(
            notes.footnotes.len(),
            3,
            "separator(0)/contSep(1)/regular(2)"
        );
        assert!(notes
            .footnotes
            .iter()
            .any(|n| n.id == 0 && n.kind == NoteKind::Separator));
        assert!(notes
            .footnotes
            .iter()
            .any(|n| n.id == 1 && n.kind == NoteKind::ContinuationSeparator));
        let f2 = notes.footnote(2).expect("Regular 腳註 id=2");
        assert!(f2.text().contains("test phrase"), "註文字：{:?}", f2.text());
        let refs: Vec<NoteRef> = doc
            .paragraphs
            .iter()
            .flat_map(|p| p.runs.iter())
            .filter_map(|r| r.note_ref)
            .collect();
        assert_eq!(refs.len(), 1, "正文恰 1 個引用：{refs:?}");
        assert_eq!(
            refs[0],
            NoteRef {
                kind: NoteKind2::Footnote,
                id: 2
            }
        );
    }

    // ---------------- (b) 正文引用標記投影 ----------------

    /// HC031 正文：恰好 1 個 note_ref(Footnote,1) run 與 1 個
    /// note_ref(Endnote,1) run；純文字 run 不帶標記。
    #[test]
    fn hc031_document_note_refs_exactly_one_each() {
        let doc = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let refs: Vec<NoteRef> = doc
            .paragraphs
            .iter()
            .flat_map(|p| p.runs.iter())
            .filter_map(|r| r.note_ref)
            .collect();
        assert_eq!(refs.len(), 2, "全檔引用標記恰 2 個：{refs:?}");
        assert!(
            refs.contains(&NoteRef {
                kind: NoteKind2::Footnote,
                id: 1
            }),
            "須含 Footnote(1)：{refs:?}"
        );
        assert!(
            refs.contains(&NoteRef {
                kind: NoteKind2::Endnote,
                id: 1
            }),
            "須含 Endnote(1)：{refs:?}"
        );
        // 帶標記的 run 必是空文字（引用錨點；Word 產出恆為專屬 run）。
        for (i, p) in doc.paragraphs.iter().enumerate() {
            for r in &p.runs {
                if r.note_ref.is_some() {
                    assert_eq!(r.text, "", "第 {i} 段：引用 run 文字必空");
                }
            }
        }
    }

    // ---------------- (c) 缺 part 容錯 ----------------

    /// 無 footnotes.xml／endnotes.xml 的 corpus 檔 → 與 Notes::default()
    /// 等價、不 panic、footnote／endnote 查找得 None。
    #[test]
    fn missing_note_parts_yield_default_notes() {
        let doc = parse_corpus("ooxmlexport/date_field_in_shape.docx");
        assert!(
            !doc.parts.contains_key(FOOTNOTES_PART) && !doc.parts.contains_key(ENDNOTES_PART),
            "前置條件：本測試檔必須沒有註 part"
        );
        let notes = Notes::from_parts(&doc.parts);
        assert_eq!(notes, Notes::default());
        assert_eq!(notes.footnote(1), None);
        assert_eq!(notes.endnote(1), None);
        assert!(doc
            .paragraphs
            .iter()
            .flat_map(|p| p.runs.iter())
            .all(|r| r.note_ref.is_none()));
    }

    // ---------------- (d) serde 相容 ----------------

    /// 舊 JSON（不含 notes／note_ref）可反序列化；新欄位 None／空時
    /// 不出現於序列化輸出（Run 端 skip；OoxmlDoc 走 serde default）。
    #[test]
    fn notes_serde_backward_compatible() {
        // 舊版 Run 快照（無 note_ref 欄；與既有測試的舊快照形同）。
        let old = r#"{"text":"x","bold":false,"italic":false,"color":null,"underline":false,"strike":false,"field":null,"rstyle":null,"direct":null,"size_pt":null,"tab":false,"effective_fonts":null}"#;
        let run: crate::Run = serde_json::from_str(old).expect("舊版 Run JSON 必須能反序列化");
        assert_eq!(run.note_ref, None, "舊 JSON → note_ref=None");
        // 新欄位 None 時不得外露。
        let j = serde_json::to_string(&crate::Run::default()).expect("序列化 Run");
        assert!(
            !j.contains("note_ref"),
            "Run::note_ref None 時不得外露：{j}"
        );
        // 帶標記時可來回。
        let with_ref = crate::Run {
            note_ref: Some(NoteRef {
                kind: NoteKind2::Endnote,
                id: 7,
            }),
            ..Default::default()
        };
        let back: crate::Run =
            serde_json::from_str(&serde_json::to_string(&with_ref).unwrap()).expect("往返");
        assert_eq!(back.note_ref, with_ref.note_ref, "note_ref 必須可往返");
        // 舊版 OoxmlDoc 快照（缺 notes 欄）→ serde default 給空彙總。
        let sample =
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample.docx"))
                .expect("樣本檔");
        let doc = crate::parse(&sample).expect("解析樣本");
        let mut v = serde_json::to_value(&doc).expect("序列化 OoxmlDoc");
        v.as_object_mut().expect("obj").remove("notes");
        let old_doc: crate::OoxmlDoc =
            serde_json::from_value(v).expect("舊版 OoxmlDoc JSON 反序列化");
        assert_eq!(old_doc.notes, Notes::default(), "缺 notes 欄 → 空彙總");
    }

    // ---------------- (e) 病態輸入 ----------------

    /// 缺 id 的 `<w:footnoteReference/>`／`<w:endnoteReference/>`：老實
    /// 跳過該標記（run 無標記、文字完整），不 panic。**取捨**：缺 id
    /// 的引用無法對應任何註本體，虛構 id（或 0）都比「略過」更錯；
    /// 畸形檔以不炸為先。註 part 端同理：無 id 的 w:footnote 整註略過。
    #[test]
    fn missing_id_references_skipped_safely() {
        let xml = r#"<w:document xmlns:w="x"><w:body>
            <w:p>
              <w:r><w:footnoteReference/><w:t>a</w:t></w:r>
              <w:r><w:endnoteReference/><w:t>b</w:t></w:r>
              <w:r><w:footnoteReference w:id="abc"/><w:t>c</w:t></w:r>
            </w:p></w:body></w:document>"#;
        let paras = crate::parse_document_xml(xml.as_bytes(), &crate::ThemeColors::default())
            .expect("解析");
        assert_eq!(paras[0].runs[0].text, "a", "引用標記失敗不得吞文字");
        assert_eq!(paras[0].runs[1].text, "b");
        assert_eq!(paras[0].runs[2].text, "c");
        assert!(
            paras[0].runs.iter().all(|r| r.note_ref.is_none()),
            "缺 id／非整數 id → 全部無標記"
        );
        // 註 part 端：id 缺失的 w:footnote 整註略過（字面含中文 → 用 &str）。
        let no_id = r#"<w:footnotes xmlns:w="x">
            <w:footnote><w:p><w:r><w:t>無id</w:t></w:r></w:p></w:footnote>
            <w:footnote w:id="1"><w:p><w:r><w:t>有id</w:t></w:r></w:p></w:footnote>
            </w:footnotes>"#;
        let theme = crate::ThemeColors::default();
        let list = parse_note_list(no_id.as_bytes(), b"footnote", &theme);
        assert_eq!(list.len(), 1, "無 id 的註老實略過");
        assert_eq!(list[0].id, 1);
        // 空輸入／壞 XML → 空彙總（不炸）。
        assert!(parse_note_list(b"", b"footnote", &theme).is_empty());
        assert!(parse_note_list(
            b"<w:footnotes xmlns:w=\"x\"><w:footnote",
            b"footnote",
            &theme
        )
        .is_empty());
    }
}
