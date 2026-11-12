//! K29d（Slice 4）：M6 節屬性／分節符／頁首頁尾／註腳尾註家族＋`SubDocTarget` 完整寫入路由。
//!
//! 依 `docs/edit-api-design.md` §6.5 與 K29d 任務卡。沿用 K29c 分檔慣例：本檔只由
//! `apply_mutation` 入口的 `is_k29d_mutation` 分流進入，不把 M6 實作塞回 Slice 1/2/3 主幹檔。
//!
//! 路由鐵律（設計書 §2.2.5／§2.3.6／§2.3.7／§6.5）：
//! - 節／頁首頁尾綁定／註腳錨點一律按 body-level 段落座標尋址；子樹文字編輯以
//!   part-local 段落座標定位，註腳／尾註目標的掃描範圍限定在該 `w:footnote`／
//!   `w:endnote` 元素內部（鐵律：不得把整份 notes part 當扁平段落流）。
//! - 最小觸碰面：分欄／邊距／橫向／節起點／titlePg／分節符只碰 `word/document.xml`；
//!   奇偶頁開關與註腳編號設定只碰 `word/settings.xml`；頁首頁尾本體只碰對應
//!   `word/header*.xml`／`word/footer*.xml`；註腳本體只碰 `word/footnotes.xml`／
//!   `word/endnotes.xml`（正文錨點變動時才加 document.xml）；新建 part 才碰
//!   `word/_rels/document.xml.rels` 與 `[Content_Types].xml`。
//! - 受保護結構誠實拒絕：根級（body 尾）`sectPr` 不準刪（§2.3.6-2）、系統分隔線註
//!   （separator／continuationSeparator）不準刪、邊距把版心歸零拒絕、全空 patch 拒絕。
//! - 無 sectPr 文檔（如 sample.docx）首寫時以開檔生效幾何合成 body 級 `<w:sectPr>`
//!   （§2.3.6 鐵律 2）：合成值＝開檔預設 ⇒ 零視覺位移。
//! - 逆補丁採「整 part 原文快照」：Undo 將 part 完整還原為變動前位元組，保證未動
//!   位元組 100% 歸位（touched 自動退場＋存檔 byte_identical 由既有 Slice 1 機制負責）。

use crate::mutation::{
    analyze_paragraph, find_paragraph_span, xml_escape, ByteSpan, DocPos, HfBindAction, HfKind,
    HfSlotKind, InverseMutation, Mutation, MutationError, MutationOutcome, NotePropsPatch,
    PageOrientation, ParagraphSpan, PartLifecycleInverse, ReversibilityLevel, SectionPropsPatch,
    SectionStartKind, SemanticInverseOp, SubDocTarget, UndoEntry, XmlReversePatch,
};
use crate::Engine;
use quire_ooxml::{NoteKind2, NoteNumberFormat, NoteRestart};

/// 正文 part 名。
const DOC: &str = "word/document.xml";
const SETTINGS: &str = "word/settings.xml";
const RELS: &str = "word/_rels/document.xml.rels";
const CONTENT_TYPES: &str = "[Content_Types].xml";

// ---------------------------------------------------------------------------
// 入口分流
// ---------------------------------------------------------------------------

/// 本片攔下的操作族：M6 七變體，以及文字族（InsertText／DeleteRange）在非 Body
/// 目標上的 SubDocTarget 路由。Body 目標維持主幹路徑，Slice 1/2 行為零變動。
pub(crate) fn is_k29d_mutation(op: &Mutation) -> bool {
    match op {
        Mutation::SetSectionProperties { .. }
        | Mutation::InsertSectionBreak { .. }
        | Mutation::DeleteSectionBreak { .. }
        | Mutation::ConfigureHeaderFooterSlot { .. }
        | Mutation::InsertNote { .. }
        | Mutation::DeleteNote { .. }
        | Mutation::SetNoteProperties { .. } => true,
        Mutation::InsertText { target, .. } => *target != SubDocTarget::Body,
        Mutation::DeleteRange { range } => range.target != SubDocTarget::Body,
        _ => false,
    }
}

impl Engine {
    /// K29d 分流入口（由 `apply_mutation` 於 K29c 分流之後呼叫）。
    pub(crate) fn apply_k29d_mutation(
        &mut self,
        op: Mutation,
    ) -> Result<MutationOutcome, MutationError> {
        match op.clone() {
            Mutation::SetSectionProperties { section_id, patch } => {
                self.k29d_set_section_properties(op, section_id, &patch)
            }
            Mutation::InsertSectionBreak { para, kind } => {
                self.k29d_insert_section_break(op, para, kind)
            }
            Mutation::DeleteSectionBreak { section_id } => {
                self.k29d_delete_section_break(op, section_id)
            }
            Mutation::ConfigureHeaderFooterSlot {
                section_id: _,
                title_pg,
                even_and_odd_headers,
                slot_bind,
            } => self.k29d_configure_hf_slot(op, title_pg, even_and_odd_headers, slot_bind),
            Mutation::InsertNote {
                kind,
                pos,
                initial_text,
            } => self.k29d_insert_note(op, kind, pos, &initial_text),
            Mutation::DeleteNote { kind, note_id } => self.k29d_delete_note(op, kind, note_id),
            Mutation::SetNoteProperties { kind, patch } => {
                self.k29d_set_note_properties(op, kind, &patch)
            }
            Mutation::InsertText {
                target,
                pos,
                text,
                style_override,
            } => {
                if style_override.is_some() {
                    return Err(rejected("子樹插入暫不接受 style_override（K29d 範圍外）"));
                }
                self.k29d_subdoc_insert_text(op, &target, pos.para, pos.cp, &text)
            }
            Mutation::DeleteRange { range } => {
                if range.start.para != range.end.para {
                    return Err(MutationError::CrossContainerBoundary {
                        start_para: range.start.para,
                        end_para: range.end.para,
                    });
                }
                self.k29d_subdoc_delete_text(
                    op,
                    &range.target,
                    range.start.para,
                    range.start.cp,
                    range.end.cp,
                )
            }
            _ => Err(rejected("不屬 K29d 分流的 Mutation")),
        }
    }

    /// 統一提交：整 part 原文快照逆補丁＋touched 標記＋派生層重建（四套索引同步
    /// 由 `rebuild_derived` 全量重解析保證，K29b 紀律）。
    fn commit_k29d(
        &mut self,
        op: Mutation,
        updates: Vec<(String, Vec<u8>)>,
        additions: Vec<(String, Vec<u8>)>,
    ) -> Result<MutationOutcome, MutationError> {
        let mut patches = Vec::new();
        let mut lifecycle = Vec::new();
        let mut touched_parts = Vec::new();
        for (name, bytes) in updates {
            let old = self.doc.parts.get(&name).cloned().unwrap_or_default();
            patches.push(XmlReversePatch {
                part_name: name.clone(),
                applied_span: ByteSpan {
                    start: 0,
                    end: bytes.len(),
                },
                original_slice: old,
            });
            self.doc.set_part(&name, bytes);
            // touched 自動退場（Slice 1 主幹同口徑）：改完與開檔基線逐位相同 ⇒
            // 退出白名單（淨零變動誠實回報 byte_identical，不虛報觸碰面）。
            if self.doc.parts.get(&name) == self.part_baselines.get(&name) {
                self.touched.remove(&name);
            } else {
                self.touched.insert(name.clone(), ());
            }
            touched_parts.push(name);
        }
        for (name, bytes) in additions {
            self.doc.set_part(&name, bytes);
            self.touched.insert(name.clone(), ());
            lifecycle.push(PartLifecycleInverse::RemoveSynthesizedPart(name.clone()));
            touched_parts.push(name);
        }
        let entry = UndoEntry {
            tx_id: self.undo_stack.len() as u64 + 1,
            inverse_ops: vec![InverseMutation {
                semantic_inverse: SemanticInverseOp::Mutation(op),
                xml_patches: patches,
                part_lifecycle_inverse: lifecycle,
            }],
            caret_before: None,
            caret_after: None,
            reversibility: ReversibilityLevel::StrictByteReversible,
        };
        self.undo_stack.push(entry);
        self.redo_stack.clear();
        self.rebuild_derived()
            .map_err(|e| MutationError::Engine(e.to_string()))?;
        while !self.is_settled() {
            self.reflow_step_json();
        }
        Ok(MutationOutcome {
            events: Vec::new(),
            touched_parts,
            inverse: Vec::new(),
            caret_after: None,
            reversibility: ReversibilityLevel::StrictByteReversible,
        })
    }
}

// ---------------------------------------------------------------------------
// 通用 XML 工具（本檔自用；與主幹 mutation.rs 零共享可動面）
// ---------------------------------------------------------------------------

fn rejected(detail: impl Into<String>) -> MutationError {
    MutationError::XmlPatchRejected {
        detail: detail.into(),
    }
}

fn part_text(engine: &Engine, name: &str) -> Result<String, MutationError> {
    let bytes = engine
        .part(name)
        .ok_or_else(|| rejected(format!("找不到 {name}")))?;
    std::str::from_utf8(bytes)
        .map(str::to_string)
        .map_err(|_| rejected(format!("{name} 明文非合法 UTF-8")))
}

/// 定位 `<name ...>`（含自闭合）從 `from` 起；回傳整元素半開區間。本檔涵蓋的
/// 元素（sectPr/cols/pgSz/pgMar/type/titlePg/footnotePr/endnotePr/numFmt/
/// numRestart/numStart/footnote/endnote/headerReference/footerReference）全不允許自嵌套。
fn locate_element(text: &str, name: &str, from: usize) -> Option<(usize, usize)> {
    let open = format!("<{name}");
    let mut cursor = from;
    while let Some(rel) = text[cursor..].find(&open) {
        let start = cursor + rel;
        let boundary = text[start + open.len()..]
            .chars()
            .next()
            .is_none_or(|c| matches!(c, '>' | '/' | ' ' | '\t' | '\n' | '\r'));
        let gt = text[start..].find('>')? + start;
        if !boundary {
            cursor = gt + 1;
            continue;
        }
        // 自闭合判準必須含 `>`（`/>`）——`text[start..=gt]` 尾元就是 `>`。
        if text[start..=gt].trim_end().ends_with("/>") {
            return Some((start, gt + 1));
        }
        let close = format!("</{name}>");
        let end = text[gt..].find(&close)? + gt + close.len();
        return Some((start, end));
    }
    None
}

/// 文件內所有 `<w:sectPr>`（文件序：段落級在前、body 級在最後）。
fn all_sectprs(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some((s, e)) = locate_element(text, "w:sectPr", cursor) {
        out.push((s, e));
        cursor = e;
    }
    out
}

/// 從標籤字串取屬性值。
fn attr_of(tag: &str, attr: &str) -> Option<String> {
    let pattern = format!(r#"{attr}=""#);
    let rel = tag.find(&pattern)?;
    let rest = &tag[rel + pattern.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// 在 `<elem` 開標籤內設／取代／移除屬性（val=None ⇒ 移除）。
fn set_element_attr(
    text: &mut String,
    elem: &str,
    attr: &str,
    val: Option<&str>,
) -> Result<(), MutationError> {
    let (start, end) = locate_element(text, elem, 0)
        .ok_or_else(|| rejected(format!("找不到 <{elem}>，無法設定 {attr}")))?;
    let gt = text[start..end].find('>').expect("元素定位必含 '>'") + start;
    let self_closing = text[start..=gt].trim_end().ends_with("/>");
    let attrs_end = if self_closing { gt - 1 } else { gt };
    let pattern = format!(r#"{attr}=""#);
    if let Some(rel) = text[start..attrs_end].find(&pattern) {
        let s = start + rel;
        let open_q = s + pattern.len() - 1;
        let close_q = text[open_q + 1..attrs_end]
            .find('"')
            .ok_or_else(|| rejected(format!("{elem} 的 {attr} 屬性值引號未閉合")))?
            + open_q
            + 1;
        match val {
            Some(v) => text.replace_range(s..close_q + 1, &format!(r#"{attr}="{v}""#)),
            None => {
                let mut from = s;
                while from > start && text.as_bytes()[from - 1] == b' ' {
                    from -= 1;
                }
                text.replace_range(from..close_q + 1, "");
            }
        }
    } else if let Some(v) = val {
        text.replace_range(attrs_end..attrs_end, &format!(r#" {attr}="{v}""#));
    }
    Ok(())
}

fn element_attr(text: &str, elem: &str, attr: &str) -> Option<String> {
    let (s, e) = locate_element(text, elem, 0)?;
    attr_of(&text[s..e], attr)
}

/// 移除所有 `<elem/>` 或 `<elem>…</elem>`；回傳移除數。
fn strip_element_all(text: &mut String, elem: &str) -> usize {
    let mut removed = 0usize;
    while let Some((s, e)) = locate_element(text, elem, 0) {
        text.replace_range(s..e, "");
        removed += 1;
        if removed > 4096 {
            break;
        }
    }
    removed
}

/// 在父元素（文件首見）的第一個 anchor 之前插入子元素；anchor 全落空插閉標籤前；
/// 父元素自閉合則展開後插入。
fn insert_child_in_parent(
    text: &mut String,
    parent: &str,
    child_xml: &str,
    anchors: &[&str],
) -> Result<(), MutationError> {
    let (start, end) =
        locate_element(text, parent, 0).ok_or_else(|| rejected(format!("找不到 <{parent}>")))?;
    let gt = text[start..end].find('>').expect("父元素開標籤必含 '>'") + start;
    if text[start..=gt].trim_end().ends_with("/>") {
        let head = text[start..gt].trim_end().trim_end_matches('/').to_string();
        text.replace_range(start..end, &format!("{head}>{child_xml}</{parent}>"));
        return Ok(());
    }
    for anchor in anchors {
        if let Some(rel) = text[gt + 1..end].find(anchor) {
            let at = gt + 1 + rel;
            text.replace_range(at..at, child_xml);
            return Ok(());
        }
    }
    let close_len = format!("</{parent}>").len();
    text.replace_range(end - close_len..end - close_len, child_xml);
    Ok(())
}

fn pt_to_twip(pt: f32) -> i64 {
    (pt as f64 * 20.0).round() as i64
}

/// 依主幹 `find_paragraph_span`（AlternateContent／Fallback／txbxContent 跳過規則
/// 同口徑）掃描全部段落 span。呼叫點全是小 part（頁首／註腳切片／測試規格文檔），
/// O(p²) 可接受；主幹改公有迭代器屬 Slice 1 檔案，禁動故就地迴圈。
fn para_spans(xml: &str) -> Vec<ParagraphSpan> {
    let mut out = Vec::new();
    let bytes = xml.as_bytes();
    while let Ok((span, _)) = find_paragraph_span(bytes, out.len()) {
        out.push(span);
    }
    out
}

/// 包住 `offset` 的 `<w:r>` 元素區間（同層不嵌套前提）。開標籤有兩種字面
/// （`<w:r>`／`<w:r attrs…>`）：兩者都要查，單邊 None 不得短路成解析失敗——
/// hf_table_header.docx 的 footer1.xml 首段全是無屬性 `<w:r>`，舊寫法用 `?` 吃
/// `<w:r ` 會把整段判成「首 run 解析失敗」。
fn enclosing_run(text: &str, offset: usize) -> Option<(usize, usize)> {
    let head = &text[..offset];
    let open = match (head.rfind("<w:r>"), head.rfind("<w:r ")) {
        (Some(a), Some(b)) => a.max(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };
    let close_rel = text[offset..].find("</w:r>")?;
    Some((open, offset + close_rel + "</w:r>".len()))
}

/// 取 run 內容的首個 `<w:rPr>`（含標籤）；無則空字串。
fn leading_rpr(inner: &str) -> String {
    let trimmed = inner.trim_start();
    if !trimmed.starts_with("<w:rPr") {
        return String::new();
    }
    let gt = trimmed.find('>').unwrap() + 1;
    if trimmed[..gt].trim_end().ends_with('/') {
        return trimmed[..gt].to_string();
    }
    match trimmed[gt..].find("</w:rPr>") {
        Some(rel) => trimmed[..gt + rel + "</w:rPr>".len()].to_string(),
        None => String::new(),
    }
}

/// `<w:t>` 標籤生成（首尾空白自動 preserve，主幹 make_text_tag 同口徑）。
fn make_text_tag(text: &str) -> String {
    let needs_preserve = text.chars().next().is_some_and(|c| c == ' ')
        || text.chars().last().is_some_and(|c| c == ' ');
    if needs_preserve {
        format!(r#"<w:t xml:space="preserve">{}</w:t>"#, xml_escape(text))
    } else {
        format!(r#"<w:t>{}</w:t>"#, xml_escape(text))
    }
}

/// `w:type/@w:val` 字面值（ECMA-376 §17.18.19）。
fn start_kind_oxml(kind: SectionStartKind) -> &'static str {
    match kind {
        SectionStartKind::NextPage => "nextPage",
        SectionStartKind::Continuous => "continuous",
        SectionStartKind::EvenPage => "evenPage",
        SectionStartKind::OddPage => "oddPage",
    }
}

fn num_fmt_oxml(fmt: NoteNumberFormat) -> &'static str {
    match fmt {
        NoteNumberFormat::Decimal => "decimal",
        NoteNumberFormat::LowerRoman => "lowerRoman",
        NoteNumberFormat::Chicago => "chicago",
    }
}

fn restart_oxml(r: NoteRestart) -> &'static str {
    match r {
        NoteRestart::Continuous => "continuous",
        NoteRestart::EachPage => "eachPage",
        NoteRestart::EachSect => "eachSect",
    }
}

// ---------------------------------------------------------------------------
// M6.1 SetSectionProperties（§2.3.6：無 sectPr 文檔以開檔幾何合成）
// ---------------------------------------------------------------------------

impl Engine {
    fn k29d_set_section_properties(
        &mut self,
        op: Mutation,
        section_id: u32,
        patch: &SectionPropsPatch,
    ) -> Result<MutationOutcome, MutationError> {
        if patch.is_empty() {
            return Err(rejected("SetSectionProperties patch 全空，拒絕 no-op"));
        }
        let doc = part_text(self, DOC)?;
        let sects = all_sectprs(&doc);
        let (mut seg, seg_span) = if sects.is_empty() {
            // 無 sectPr 文檔：僅第 0 節可寫（合成件），其餘誠實拒絕。PM 裁決（11-12）：
            // 查無節 id 一律 SubDocNotFound（設計書口徑），detail 不改變型別。
            if section_id != 0 {
                return Err(MutationError::SubDocNotFound {
                    target: unknown_section_target(section_id),
                });
            }
            let synth = synthesized_sectpr(self);
            (synth, None)
        } else {
            let Some(&(s, e)) = sects.get(section_id as usize) else {
                return Err(MutationError::SubDocNotFound {
                    target: unknown_section_target(section_id),
                });
            };
            (doc[s..e].to_string(), Some((s, e)))
        };

        // 橫向：pgSz 的 w/h 是絕對尺寸，換向＝長短邊互換＋orient 標記。
        if let Some(orient) = patch.orientation {
            let cur_w = element_attr(&seg, "w:pgSz", "w:w")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(11906.0);
            let cur_h = element_attr(&seg, "w:pgSz", "w:h")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(16838.0);
            let (long, short) = if cur_w >= cur_h {
                (cur_w, cur_h)
            } else {
                (cur_h, cur_w)
            };
            let (w, h) = match orient {
                PageOrientation::Portrait => (short, long),
                PageOrientation::Landscape => (long, short),
            };
            set_element_attr(&mut seg, "w:pgSz", "w:w", Some(&w.round().to_string()))?;
            set_element_attr(&mut seg, "w:pgSz", "w:h", Some(&h.round().to_string()))?;
            set_element_attr(
                &mut seg,
                "w:pgSz",
                "w:orient",
                if orient == PageOrientation::Landscape {
                    Some("landscape")
                } else {
                    None
                },
            )?;
        }

        // 邊距（twip＝pt×20）。
        let margins: [(&str, Option<f32>); 6] = [
            ("w:top", patch.margin_top_pt),
            ("w:bottom", patch.margin_bottom_pt),
            ("w:left", patch.margin_left_pt),
            ("w:right", patch.margin_right_pt),
            ("w:header", patch.margin_header_pt),
            ("w:footer", patch.margin_footer_pt),
        ];
        for (attr, value) in margins {
            if let Some(pt) = value {
                set_element_attr(&mut seg, "w:pgMar", attr, Some(&pt_to_twip(pt).to_string()))?;
            }
        }

        // 分欄（§2.2.5 平分模式口徑：欄寬＝(版心−(n−1)×space)/n；次欄起點＝首欄＋欄寬＋space）。
        if let Some(cols) = &patch.cols {
            if cols.num == 0 {
                return Err(rejected("分欄數必須 >= 1"));
            }
            let page_w = element_attr(&seg, "w:pgSz", "w:w")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(11906.0);
            let left = element_attr(&seg, "w:pgMar", "w:left")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(1800.0);
            let right = element_attr(&seg, "w:pgMar", "w:right")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(1800.0);
            let body_w = (page_w - left - right).max(1.0);
            let space_twip = cols.space_pt.map(pt_to_twip).unwrap_or(720);
            // 顯式逐欄 <w:col/> 在欄數變更後全數失效：拆掉重建（平分模式）。
            strip_element_all(&mut seg, "w:col");
            let (cs, ce) = locate_element(&seg, "w:cols", 0)
                .ok_or_else(|| rejected("找不到 <w:cols>，無法改分欄"))?;
            let head = seg[cs..ce]
                .split_once('>')
                .map(|(h, _)| h.trim_end().trim_end_matches('/').to_string())
                .ok_or_else(|| rejected("<w:cols> 開標籤解析失敗"))?;
            if cols.num > 1 {
                let col_w = ((body_w - space_twip as f64 * (cols.num as f64 - 1.0))
                    / cols.num as f64)
                    .max(1.0)
                    .round();
                let mut inner = String::new();
                for _ in 0..cols.num {
                    inner.push_str(&format!(r#"<w:col w:w="{col_w}" w:space="{space_twip}"/>"#));
                }
                seg.replace_range(cs..ce, &format!("{head}>{inner}</w:cols>"));
                set_element_attr(&mut seg, "w:cols", "w:num", Some(&cols.num.to_string()))?;
                set_element_attr(&mut seg, "w:cols", "w:space", Some(&space_twip.to_string()))?;
            } else {
                seg.replace_range(cs..ce, &format!("{head}/>"));
                set_element_attr(&mut seg, "w:cols", "w:num", None)?;
                if let Some(space) = cols.space_pt {
                    set_element_attr(
                        &mut seg,
                        "w:cols",
                        "w:space",
                        Some(&pt_to_twip(space).to_string()),
                    )?;
                }
            }
        }

        // 節起點型別（ECMA §17.18.19：w:type 位於 pgSz 之前）。
        if let Some(kind) = patch.section_type {
            strip_element_all(&mut seg, "w:type");
            let xml = format!(r#"<w:type w:val="{}"/>"#, start_kind_oxml(kind));
            insert_child_in_parent(&mut seg, "w:sectPr", &xml, &["<w:pgSz", "</w:sectPr>"])?;
        }

        // 幾何驗證（§2.3.6-3）：邊距把版心吃掉 ⇒ 誠實拒絕（拒絕路徑零副作用）。
        let page_w = element_attr(&seg, "w:pgSz", "w:w")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(11906.0);
        let left = element_attr(&seg, "w:pgMar", "w:left")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        let right = element_attr(&seg, "w:pgMar", "w:right")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        if page_w - left - right <= 0.0 {
            return Err(rejected(format!(
                "邊距把版心吃掉（寬 {page_w}−{left}−{right} twip <= 0），拒絕寫入"
            )));
        }

        let out = match seg_span {
            Some((s, e)) => {
                let mut out = doc;
                out.replace_range(s..e, &seg);
                out
            }
            None => {
                // 首寫 body 級 sectPr：插到 `</w:body>` 之前（ECMA：body 尾必存）
                let at = doc
                    .rfind("</w:body>")
                    .ok_or_else(|| rejected("document.xml 無 </w:body>，無法插入 sectPr"))?;
                let mut out = doc;
                out.replace_range(at..at, &seg);
                out
            }
        };
        self.commit_k29d(op, vec![(DOC.to_string(), out.into_bytes())], vec![])
    }
}

/// 以開檔生效幾何合成 body 級 `<w:sectPr>`（§2.3.6 鐵律 2：合成值＝開檔預設
/// ⇒ 首寫零視覺位移；twip＝pt×20；header/footer 距 720 twip＝36pt＝引擎預設）。
fn synthesized_sectpr(engine: &Engine) -> String {
    let geo = engine.first_sec_geo;
    let short = geo.page_w_pt.min(geo.page_h_pt);
    let long = geo.page_w_pt.max(geo.page_h_pt);
    let (w, h) = if geo.landscape {
        (long, short)
    } else {
        (short, long)
    };
    let orient = if geo.landscape {
        r#" w:orient="landscape""#
    } else {
        ""
    };
    format!(
        "<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"{orient}/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/><w:cols w:space=\"425\"/><w:docGrid w:linePitch=\"360\"/></w:sectPr>",
        pt_to_twip(w),
        pt_to_twip(h),
        pt_to_twip(geo.margin_top_pt),
        pt_to_twip(geo.margin_right_pt),
        pt_to_twip(geo.margin_bottom_pt),
        pt_to_twip(geo.margin_left_pt),
    )
}

// ---------------------------------------------------------------------------
// M6.2 分節符插刪（ECMA-376 §17.2.3：節收束於段內 pPr/sectPr）
// ---------------------------------------------------------------------------

impl Engine {
    fn k29d_insert_section_break(
        &mut self,
        op: Mutation,
        para: usize,
        kind: SectionStartKind,
    ) -> Result<MutationOutcome, MutationError> {
        let doc = part_text(self, DOC)?;
        let spans = para_spans(&doc);
        let sp = spans.get(para).ok_or(MutationError::ParagraphOutOfBounds {
            para,
            len: spans.len(),
        })?;
        let sects = all_sectprs(&doc);
        if sects.is_empty() {
            return Err(rejected(
                "document.xml 無 <w:sectPr>，請先以 SetSectionProperties 建立節設定",
            ));
        }
        // 新段 sectPr ＝「現行節」設定之複製（包含 para 的最首個邊界件）。
        let boundary = sects
            .iter()
            .find(|(s, _)| *s >= sp.start)
            .or(sects.last())
            .copied()
            .expect("sects 非空已由上游驗證");
        let seg = doc[boundary.0..boundary.1].to_string();
        // 空段＋pPr/sectPr 插到 para 之前＝前節收束處（§17.2.3；插刪互為逆）。
        let new_para = format!("<w:p><w:pPr>{seg}</w:pPr></w:p>");
        let mut out = doc.clone();
        out.replace_range(sp.start..sp.start, &new_para);

        // 分節型別記於「新節的收束件」（插入後其後第一個 sectPr；ECMA：w:type
        // 描述該節起點方式）。插入後偏移已變，重新定位兩輪（先拆後插）。
        let after = sp.start + new_para.len();
        for _ in 0..2 {
            let Some((bs, be)) = all_sectprs(&out)
                .into_iter()
                .find(|(s, _)| *s >= after)
                .or_else(|| all_sectprs(&out).last().copied())
            else {
                break;
            };
            let mut sect = out[bs..be].to_string();
            strip_element_all(&mut sect, "w:type");
            let gt = sect.find('>').expect("sectPr 必含 '>'") + 1;
            sect.replace_range(
                gt..gt,
                &format!(r#"<w:type w:val="{}"/>"#, start_kind_oxml(kind)),
            );
            out.replace_range(bs..be, &sect);
        }
        self.commit_k29d(op, vec![(DOC.to_string(), out.into_bytes())], vec![])
    }

    fn k29d_delete_section_break(
        &mut self,
        op: Mutation,
        section_id: u32,
    ) -> Result<MutationOutcome, MutationError> {
        let doc = part_text(self, DOC)?;
        let sects = all_sectprs(&doc);
        if sects.is_empty() {
            return Err(rejected("本檔沒有任何 <w:sectPr>"));
        }
        // 只有段落級（非 body 尾）sectPr 是可拆的分節符；根級永不刪（§2.3.6-2）。
        let body_level = sects[sects.len() - 1];
        let para_level: Vec<(usize, usize)> = sects
            .iter()
            .copied()
            .filter(|(s, _)| *s != body_level.0)
            .collect();
        let Some(&(s, e)) = para_level.get(section_id as usize) else {
            return Err(MutationError::ProtectedStructure {
                reason: format!(
                    "section_id={section_id} 非段落級分節符（根級 body 尾 sectPr ECMA 必存，不準刪）"
                ),
            });
        };
        // 宿主段落拆掉 sectPr 後若殘留為空段 ⇒ 整段移除（與插分節互為逆、索引回穩）；
        // 有內容的段 ⇒ 只拆 sectPr（段落保留、索引零位移）。
        let host = para_spans(&doc)
            .into_iter()
            .find(|sp| sp.start <= s && e <= sp.end);
        let mut out = doc.clone();
        let mut remove_whole = false;
        if let Some(sp) = host {
            let raw = &out[sp.start..sp.end];
            let mut residual = String::new();
            residual.push_str(&raw[..s - sp.start]);
            residual.push_str(&raw[e - sp.start..]);
            let inner = residual
                .find('>')
                .map(|gt| &residual[gt + 1..])
                .unwrap_or("")
                .strip_suffix("</w:p>")
                .unwrap_or("")
                .trim()
                .to_string();
            remove_whole = inner.is_empty()
                || inner == "<w:pPr></w:pPr>"
                || inner == "<w:pPr/>"
                || inner.starts_with("<w:pPr></w:pPr>")
                    && inner["<w:pPr></w:pPr>".len()..].trim().is_empty();
        }
        if remove_whole {
            if let Some(sp) = host {
                out.replace_range(sp.start..sp.end, "");
            }
        } else {
            out.replace_range(s..e, "");
        }
        self.commit_k29d(op, vec![(DOC.to_string(), out.into_bytes())], vec![])
    }
}

// ---------------------------------------------------------------------------
// M6.3 ConfigureHeaderFooterSlot（三態最小觸碰面逐條）
// ---------------------------------------------------------------------------

impl Engine {
    fn k29d_configure_hf_slot(
        &mut self,
        op: Mutation,
        title_pg: Option<bool>,
        even_odd: Option<bool>,
        slot_bind: Option<(HfKind, HfSlotKind, HfBindAction)>,
    ) -> Result<MutationOutcome, MutationError> {
        if title_pg.is_none() && even_odd.is_none() && slot_bind.is_none() {
            return Err(rejected(
                "ConfigureHeaderFooterSlot 三態皆 None，拒絕 no-op",
            ));
        }
        let mut updates: Vec<(String, Vec<u8>)> = Vec::new();
        let mut additions: Vec<(String, Vec<u8>)> = Vec::new();

        // (1) titlePg：僅碰 word/document.xml（sectPr 旗標；col 之後 docGrid 之前）。
        if let Some(enable) = title_pg {
            let doc = part_text(self, DOC)?;
            all_sectprs(&doc)
                .last()
                .ok_or_else(|| rejected("document.xml 無 <w:sectPr>，無法設定 titlePg"))?;
            let mut out = doc;
            if enable {
                if !out.contains("<w:titlePg") {
                    insert_child_in_parent(&mut out, "w:sectPr", "<w:titlePg/>", &["<w:docGrid"])?;
                }
            } else {
                strip_element_all(&mut out, "w:titlePg");
            }
            updates.push((DOC.to_string(), out.into_bytes()));
        }

        // (2) evenAndOddHeaders：僅碰 word/settings.xml（§2.2.5）。
        if let Some(enable) = even_odd {
            let mut settings = part_text(self, SETTINGS)?;
            if enable {
                if !settings.contains("<w:evenAndOddHeaders") {
                    let at = settings_insert_point(&settings);
                    settings.replace_range(at..at, "<w:evenAndOddHeaders/>");
                }
            } else {
                strip_element_all(&mut settings, "w:evenAndOddHeaders");
            }
            updates.push((SETTINGS.to_string(), settings.into_bytes()));
        }

        // (3) slot 綁定：Create 才碰 .rels 與 [Content_Types].xml。
        if let Some((kind, slot, action)) = slot_bind {
            let elem = hf_ref_elem(kind);
            let ty = oxml_slot(slot);
            let mut doc = part_text(self, DOC)?;
            let sects = all_sectprs(&doc);
            let (sect_s, sect_e) = sects
                .last()
                .copied()
                .ok_or_else(|| rejected("document.xml 無 <w:sectPr>"))?;
            let mut sect = doc[sect_s..sect_e].to_string();
            match action {
                HfBindAction::Unbind => {
                    let mut cuts: Vec<(usize, usize)> = Vec::new();
                    let mut cursor = 0usize;
                    while let Some((es, ee)) = locate_element(&sect, elem, cursor) {
                        cursor = ee;
                        if attr_of(&sect[es..ee], "w:type")
                            .as_deref()
                            .unwrap_or("default")
                            == ty
                        {
                            cuts.push((es, ee));
                        }
                    }
                    if cuts.is_empty() {
                        return Err(rejected(format!(
                            "查無 {elem} w:type=\"{ty}\" 綁定，無需解除"
                        )));
                    }
                    for (es, ee) in cuts.into_iter().rev() {
                        sect.replace_range(es..ee, "");
                    }
                    // part 本體與 rels／CT 註冊留存（誠實邊界：解除綁定 ≠ 刪 part）
                    doc.replace_range(sect_s..sect_e, &sect);
                    updates.push((DOC.to_string(), doc.into_bytes()));
                }
                HfBindAction::Create => {
                    if resolve_hf_part_exact(self, kind, slot).is_ok() {
                        return Err(MutationError::ProtectedStructure {
                            reason: format!("{elem} w:type=\"{ty}\" 已綁定 part，拒絕重複新建"),
                        });
                    }
                    let new_part = next_hf_part_name(self, kind);
                    let kind_elem = match kind {
                        HfKind::Header => "header",
                        HfKind::Footer => "footer",
                    };
                    let content_type = match kind {
                        HfKind::Header => {
                            "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"
                        }
                        HfKind::Footer => {
                            "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"
                        }
                    };
                    let root = match kind {
                        HfKind::Header => "w:hdr",
                        HfKind::Footer => "w:ftr",
                    };
                    let rid = allocate_rel_id(self)?;
                    // 1) 新 part：最小合法空殼（單個空段）
                    let xml = format!(
                        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<{root} xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:p></w:p></{root}>"
                    );
                    additions.push((new_part.clone(), xml.into_bytes()));
                    // 2) rels 註冊（Target 為 word/ 相對——header/footer 與正文同目錄）
                    let mut rels = part_text(self, RELS)?;
                    let at = rels
                        .rfind("</Relationships>")
                        .ok_or_else(|| rejected("rels 無 </Relationships>"))?;
                    let target_leaf = new_part
                        .strip_prefix(&format!("word/{kind_elem}"))
                        .and_then(|r| r.strip_suffix(".xml"))
                        .unwrap_or_default();
                    rels.replace_range(
                        at..at,
                        &format!(
                            r#"<Relationship Id="{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind_elem}" Target="{kind_elem}{target_leaf}.xml"/>"#
                        ),
                    );
                    updates.push((RELS.to_string(), rels.into_bytes()));
                    // 3) [Content_Types].xml 註冊 Override
                    let mut ct = part_text(self, CONTENT_TYPES)?;
                    if !ct.contains(&format!(r#"PartName="/{new_part}""#)) {
                        let at = ct
                            .rfind("</Types>")
                            .ok_or_else(|| rejected("[Content_Types].xml 無 </Types>"))?;
                        ct.replace_range(
                            at..at,
                            &format!(
                                r#"<Override PartName="/{new_part}" ContentType="{content_type}"/>"#
                            ),
                        );
                        updates.push((CONTENT_TYPES.to_string(), ct.into_bytes()));
                    }
                    // 4) sectPr 綁 reference（群組內維持 default→first→even 序）
                    insert_ref_by_order(
                        &mut sect,
                        elem,
                        ty,
                        &format!(r#"<{elem} w:type="{ty}" r:id="{rid}"/>"#),
                    );
                    doc.replace_range(sect_s..sect_e, &sect);
                    updates.push((DOC.to_string(), doc.into_bytes()));
                }
            }
        }
        self.commit_k29d(op, updates, additions)
    }
}

/// settings.xml 內文件級開關的插入點（ECMA §17.15 群組：defaultTabStop 之後、
/// 其餘之前；Word 實檔 evenAndOddHeaders 即落此位）。
fn settings_insert_point(settings: &str) -> usize {
    if let Some(rel) = settings.find("<w:defaultTabStop") {
        if let Some(gt) = settings[rel..].find('>') {
            return rel + gt + 1;
        }
    }
    settings
        .find("<w:compat")
        .or_else(|| settings.rfind("</w:settings>"))
        .unwrap_or(settings.len())
}

fn hf_ref_elem(kind: HfKind) -> &'static str {
    match kind {
        HfKind::Header => "w:headerReference",
        HfKind::Footer => "w:footerReference",
    }
}

fn oxml_slot(slot: HfSlotKind) -> &'static str {
    match slot {
        HfSlotKind::Default => "default",
        HfSlotKind::First => "first",
        HfSlotKind::Even => "even",
    }
}

/// rels 內未被佔用的最大數值尾號＋1（`rIdN` 慣例，避免與既有 Id 相撞）。
fn allocate_rel_id(engine: &Engine) -> Result<String, MutationError> {
    let rels = part_text(engine, RELS)?;
    let mut max: u64 = 0;
    let mut cursor = 0usize;
    let key = r#"Id="rId"#;
    while let Some(rel) = rels[cursor..].find(key) {
        let start = cursor + rel + key.len();
        let end = rels[start..]
            .find('"')
            .map(|r| start + r)
            .unwrap_or(rels.len());
        if let Ok(n) = rels[start..end].parse::<u64>() {
            max = max.max(n);
        }
        cursor = end;
    }
    Ok(format!("rId{}", max + 1))
}

/// word/ 下既有同型 part 最大編號＋1（header1.xml… ⇒ header{n+1}.xml）。
fn next_hf_part_name(engine: &Engine, kind: HfKind) -> String {
    let prefix = format!(
        "word/{}",
        match kind {
            HfKind::Header => "header",
            HfKind::Footer => "footer",
        }
    );
    let mut max: u32 = 0;
    for name in engine.doc.parts.keys() {
        if let Some(rest) = name.strip_prefix(&prefix) {
            if let Some(num) = rest.strip_suffix(".xml") {
                if let Ok(n) = num.parse::<u32>() {
                    max = max.max(n);
                }
            }
        }
    }
    format!("{prefix}{}.xml", max + 1)
}

/// sectPr 內插入 reference，維持 Word 寫出的 default→first→even 群組序。
fn insert_ref_by_order(sect: &mut String, elem: &str, ty: &str, ref_xml: &str) {
    let order = ["default", "first", "even"];
    let rank = |t: &str| order.iter().position(|o| *o == t).unwrap_or(0);
    match locate_element(sect, elem, 0) {
        None => {
            // 同型群組不存在：插到 <w:pgSz 之前（ECMA：refs 在 pgSz 之前）
            let at = locate_element(sect, "w:pgSz", 0)
                .map(|(s, _)| s)
                .unwrap_or_else(|| sect.find('>').unwrap() + 1);
            sect.replace_range(at..at, ref_xml);
        }
        Some((gs, _)) => {
            let mut cursor = gs;
            let mut insert_at = None;
            let mut last_end = gs;
            while let Some((s, e)) = locate_element(sect, elem, cursor) {
                cursor = e;
                last_end = e;
                let cur_ty = attr_of(&sect[s..e], "w:type").unwrap_or_else(|| "default".into());
                if rank(ty) < rank(&cur_ty) {
                    insert_at = Some(s);
                    break;
                }
            }
            let at = insert_at.unwrap_or(last_end);
            sect.replace_range(at..at, ref_xml);
        }
    }
}

/// rels 的 `Id` → part 名（Target 恆為 word/ 相對）。
fn rel_target(engine: &Engine, rid: &str) -> Option<String> {
    let rels = engine.part(RELS)?;
    let text = std::str::from_utf8(rels).ok()?;
    let mut cursor = 0usize;
    while let Some(rel) = text[cursor..].find("<Relationship ") {
        let start = cursor + rel;
        let gt = text[start..].find('>')? + start;
        let tag = &text[start..=gt];
        cursor = gt + 1;
        if attr_of(tag, "Id").as_deref() != Some(rid) {
            continue;
        }
        let target = attr_of(tag, "Target")?;
        let cleaned = target.trim_start_matches("./").trim_start_matches('/');
        return Some(format!("word/{cleaned}"));
    }
    None
}

/// 精確 slot 綁定解析（末節＝body 級 sectPr；不走兜底繼承）。
fn resolve_hf_part_exact(
    engine: &Engine,
    kind: HfKind,
    slot: HfSlotKind,
) -> Result<String, MutationError> {
    let doc = part_text(engine, DOC)?;
    let sect = all_sectprs(&doc)
        .last()
        .copied()
        .ok_or_else(|| rejected("無 <w:sectPr>"))?;
    let sect_xml = &doc[sect.0..sect.1];
    let elem = hf_ref_elem(kind);
    let want = oxml_slot(slot);
    let mut cursor = 0usize;
    while let Some((s, e)) = locate_element(sect_xml, elem, cursor) {
        cursor = e;
        if attr_of(&sect_xml[s..e], "w:type")
            .as_deref()
            .unwrap_or("default")
            == want
        {
            let rid =
                attr_of(&sect_xml[s..e], "r:id").ok_or_else(|| rejected("reference 缺 r:id"))?;
            return rel_target(engine, &rid)
                .ok_or_else(|| rejected("reference 的 r:id 查無 rels 對應"));
        }
    }
    Err(MutationError::SubDocNotFound {
        target: target_for(kind, slot),
    })
}

/// 查無節 id 時的拒絕承載體（PM 裁決 11-12：未知節 id 一律 `SubDocNotFound`；
/// `SubDocTarget` 沒有「裸節 id」變體，借 Header/Default 當診斷承載，型別正確性
/// 重於承載體美化，detail 訊息不進錯誤型別）。
fn unknown_section_target(section_id: u32) -> SubDocTarget {
    SubDocTarget::Header {
        section_id,
        slot: HfSlotKind::Default,
    }
}

fn target_for(kind: HfKind, slot: HfSlotKind) -> SubDocTarget {
    match kind {
        HfKind::Header => SubDocTarget::Header {
            section_id: 0,
            slot,
        },
        HfKind::Footer => SubDocTarget::Footer {
            section_id: 0,
            slot,
        },
    }
}

/// SubDocTarget → part 名（含存在性守門；TextBox 子樹誠實未開放）。
fn resolve_subdoc_part(engine: &Engine, target: &SubDocTarget) -> Result<String, MutationError> {
    match target {
        SubDocTarget::Body => Ok(DOC.to_string()),
        SubDocTarget::Header { slot, .. } => resolve_hf_part_exact(engine, HfKind::Header, *slot),
        SubDocTarget::Footer { slot, .. } => resolve_hf_part_exact(engine, HfKind::Footer, *slot),
        SubDocTarget::Footnote { note_id } => {
            ensure_note_exists(engine, NoteKind2::Footnote, *note_id)?;
            Ok("word/footnotes.xml".to_string())
        }
        SubDocTarget::Endnote { note_id } => {
            ensure_note_exists(engine, NoteKind2::Endnote, *note_id)?;
            Ok("word/endnotes.xml".to_string())
        }
        SubDocTarget::TextBox {
            host_para,
            box_index,
        } => Err(MutationError::SubDocNotFound {
            target: SubDocTarget::TextBox {
                host_para: *host_para,
                box_index: *box_index,
            },
        }),
    }
}

fn note_tag(kind: NoteKind2) -> &'static str {
    match kind {
        NoteKind2::Footnote => "w:footnote",
        NoteKind2::Endnote => "w:endnote",
    }
}

fn note_part(kind: NoteKind2) -> &'static str {
    match kind {
        NoteKind2::Footnote => "word/footnotes.xml",
        NoteKind2::Endnote => "word/endnotes.xml",
    }
}

/// 定位 `<w:footnote w:id="N">…</w:footnote>`（含自闭合）。
fn note_element(xml: &str, kind: NoteKind2, note_id: i32) -> Option<(usize, usize)> {
    let tag = note_tag(kind);
    let mut cursor = 0usize;
    while let Some((s, e)) = locate_element(xml, tag, cursor) {
        cursor = e;
        if attr_of(&xml[s..e], "w:id").and_then(|v| v.trim().parse::<i32>().ok()) == Some(note_id) {
            return Some((s, e));
        }
    }
    None
}

fn ensure_note_exists(engine: &Engine, kind: NoteKind2, note_id: i32) -> Result<(), MutationError> {
    let xml = part_text(engine, note_part(kind))?;
    if note_element(&xml, kind, note_id).is_some() {
        Ok(())
    } else {
        Err(MutationError::SubDocNotFound {
            target: match kind {
                NoteKind2::Footnote => SubDocTarget::Footnote { note_id },
                NoteKind2::Endnote => SubDocTarget::Endnote { note_id },
            },
        })
    }
}

// ---------------------------------------------------------------------------
// M6.4 註腳／尾註插刪與編號設定（§2.3.7）
// ---------------------------------------------------------------------------

impl Engine {
    fn k29d_insert_note(
        &mut self,
        op: Mutation,
        kind: NoteKind2,
        pos: DocPos,
        initial_text: &str,
    ) -> Result<MutationOutcome, MutationError> {
        let doc = part_text(self, DOC)?;
        let notes_name = note_part(kind);
        let notes = part_text(self, notes_name)?;
        let tag = note_tag(kind);
        let (ref_elem, ref_tag) = match kind {
            NoteKind2::Footnote => ("w:footnoteReference", "w:footnoteRef"),
            NoteKind2::Endnote => ("w:endnoteReference", "w:endnoteRef"),
        };

        // 新 id＝既有最大 id＋1（系統分隔線註 -1/0/1 慣例自然避開）。
        let mut max_id: i32 = -1;
        let mut cursor = 0usize;
        while let Some((s, e)) = locate_element(&notes, tag, cursor) {
            cursor = e;
            if let Some(id) =
                attr_of(&notes[s..e], "w:id").and_then(|v| v.trim().parse::<i32>().ok())
            {
                max_id = max_id.max(id);
            }
        }
        let new_id = max_id + 1;

        // 正文錨點：cp 所在 run 之前（cp 在節點首）／之後（節點末）／run 中部拆 run。
        let (span, _) = find_paragraph_span(doc.as_bytes(), pos.para).map_err(|_| {
            MutationError::ParagraphOutOfBounds {
                para: pos.para,
                len: 0,
            }
        })?;
        let analysis = analyze_paragraph(doc.as_bytes(), span);
        let ref_run = format!(r#"<w:r><{ref_elem} w:id="{new_id}"/></w:r>"#);
        let mut out = doc.clone();
        let node = analysis
            .text_nodes
            .iter()
            .find(|n| n.start_cp <= pos.cp && pos.cp <= n.end_cp && n.run_span.is_some());
        match node {
            Some(n) => {
                let run = enclosing_run(&doc, n.node_span.start)
                    .ok_or_else(|| rejected("引用點所在 run 區間解析失敗"))?;
                if pos.cp <= n.start_cp {
                    out.replace_range(run.0..run.0, &ref_run);
                } else if pos.cp >= n.end_cp {
                    out.replace_range(run.1..run.1, &ref_run);
                } else {
                    let run_raw = &doc[run.0..run.1];
                    let open_gt = run_raw
                        .find('>')
                        .ok_or_else(|| rejected("run 開標籤解析失敗"))?;
                    let run_open = &run_raw[..=open_gt];
                    let close_rel = run_raw
                        .rfind("</w:r>")
                        .ok_or_else(|| rejected("run 閉標籤解析失敗"))?;
                    let run_inner = &run_raw[open_gt + 1..close_rel];
                    let rpr = leading_rpr(run_inner);
                    let chars: Vec<char> = n.unescaped.chars().collect();
                    let local = (pos.cp - n.start_cp).min(chars.len());
                    let left: String = chars[..local].iter().collect();
                    let right: String = chars[local..].iter().collect();
                    let replacement = format!(
                        "{run_open}{}</w:r>{ref_run}<w:r>{}{}</w:r>",
                        make_text_tag(&left),
                        rpr,
                        make_text_tag(&right),
                    );
                    out.replace_range(run.0..run.1, &replacement);
                }
            }
            None => {
                // 空段或純非文字段：pPr 之後（或 p 開標籤後）插引用 run
                let at = analysis
                    .ppr_end_offset
                    .unwrap_or(analysis.p_open_end_offset);
                out.replace_range(at..at, &ref_run);
            }
        }

        // 註體：一般註排在系統分隔線註之前（Word 序）；首字前導空格隔開註號。
        let body = if initial_text.is_empty() {
            format!(r#"<w:p><w:r><{ref_tag}/></w:r></w:p>"#)
        } else {
            format!(
                r#"<w:p><w:r><{ref_tag}/></w:r><w:r>{}</w:r></w:p>"#,
                make_text_tag(&format!(" {initial_text}")),
            )
        };
        let note_xml = format!(r#"<{tag} w:id="{new_id}">{body}</{tag}>"#);
        let mut notes = notes;
        let mut insert_at = None;
        let mut cursor = 0usize;
        while let Some((s, e)) = locate_element(&notes, tag, cursor) {
            cursor = e;
            let ty = attr_of(&notes[s..e], "w:type").unwrap_or_default();
            if ty == "separator" || ty == "continuationSeparator" {
                insert_at = Some(s);
                break;
            }
        }
        let at = insert_at.unwrap_or(notes.len());
        notes.replace_range(at..at, &note_xml);

        self.commit_k29d(
            op,
            vec![
                (DOC.to_string(), out.into_bytes()),
                (notes_name.to_string(), notes.into_bytes()),
            ],
            vec![],
        )
    }

    fn k29d_delete_note(
        &mut self,
        op: Mutation,
        kind: NoteKind2,
        note_id: i32,
    ) -> Result<MutationOutcome, MutationError> {
        let name = note_part(kind);
        let mut notes = part_text(self, name)?;
        let (s, e) = note_element(&notes, kind, note_id).ok_or(MutationError::SubDocNotFound {
            target: match kind {
                NoteKind2::Footnote => SubDocTarget::Footnote { note_id },
                NoteKind2::Endnote => SubDocTarget::Endnote { note_id },
            },
        })?;
        // 系統分隔線註誠實拒絕（§2.3.7 鐵律；r7_footnote_anchor 同口徑）。
        let ty = attr_of(&notes[s..e], "w:type").unwrap_or_default();
        if ty == "separator" || ty == "continuationSeparator" {
            return Err(MutationError::ProtectedStructure {
                reason: format!("id={note_id} 是系統 {ty} 註，不準刪除"),
            });
        }
        notes.replace_range(s..e, "");

        // 正文錨點：引用專 run ⇒ 整 run 移除；與文字混排 ⇒ 僅拆引用標籤。
        let mut doc = part_text(self, DOC)?;
        let ref_elem = match kind {
            NoteKind2::Footnote => "w:footnoteReference",
            NoteKind2::Endnote => "w:endnoteReference",
        };
        let needle = format!(r#"<{ref_elem} w:id="{note_id}""#);
        let mut removed = 0usize;
        while let Some(rel) = doc.find(&needle) {
            let gt = doc[rel..]
                .find('>')
                .ok_or_else(|| rejected("引用標籤未閉合"))?
                + rel;
            let tag_end = gt + 1;
            match enclosing_run(&doc, rel) {
                Some((rs, re)) => {
                    let run_raw = doc[rs..re].to_string();
                    let mut without = run_raw.clone();
                    without.replace_range(rel - rs..tag_end - rs, "");
                    // run 只剩 rPr／空白 ⇒ 引用專 run，整段移除
                    let residual = without
                        .find('>')
                        .map(|g| &without[g + 1..])
                        .unwrap_or("")
                        .strip_suffix("</w:r>")
                        .unwrap_or("")
                        .to_string();
                    let mut probe = residual.clone();
                    strip_element_all(&mut probe, "w:rPr");
                    if probe.trim().is_empty() {
                        doc.replace_range(rs..re, "");
                    } else {
                        doc.replace_range(rs..re, &without);
                    }
                }
                None => doc.replace_range(rel..tag_end, ""),
            }
            removed += 1;
            if removed > 512 {
                break;
            }
        }
        self.commit_k29d(
            op,
            vec![
                (DOC.to_string(), doc.into_bytes()),
                (name.to_string(), notes.into_bytes()),
            ],
            vec![],
        )
    }

    fn k29d_set_note_properties(
        &mut self,
        op: Mutation,
        kind: NoteKind2,
        patch: &NotePropsPatch,
    ) -> Result<MutationOutcome, MutationError> {
        if patch.is_empty() {
            return Err(rejected("SetNoteProperties patch 全空，拒絕 no-op"));
        }
        // §2.3.7：編號格式／重啟／起始值＝settings.xml 的文件級 w:footnotePr／
        // w:endnotePr（節級覆寫留待後續片，報告列明偏差）。
        let mut settings = part_text(self, SETTINGS)?;
        let pr = match kind {
            NoteKind2::Footnote => "w:footnotePr",
            NoteKind2::Endnote => "w:endnotePr",
        };
        if locate_element(&settings, pr, 0).is_none() {
            let at = settings_insert_point(&settings);
            settings.replace_range(at..at, &format!("<{pr}></{pr}>"));
        }
        let (ps, pe) = locate_element(&settings, pr, 0).expect("剛確保存在");
        let mut sect = settings[ps..pe].to_string();
        strip_element_all(&mut sect, "w:numFmt");
        strip_element_all(&mut sect, "w:numStart");
        strip_element_all(&mut sect, "w:numRestart");
        let mut children = String::new();
        if let Some(fmt) = patch.num_fmt {
            children.push_str(&format!(r#"<w:numFmt w:val="{}"/>"#, num_fmt_oxml(fmt)));
        }
        if let Some(start) = patch.num_start {
            children.push_str(&format!(r#"<w:numStart w:val="{start}"/>"#));
        }
        if let Some(restart) = patch.num_restart {
            children.push_str(&format!(
                r#"<w:numRestart w:val="{}"/>"#,
                restart_oxml(restart)
            ));
        }
        // ECMA §17.11.13 子元素序：numFmt→numStart→numRestart 置于節點首。
        let gt = sect.find('>').expect("剛定位必含 '>'") + 1;
        sect.replace_range(gt..gt, &children);
        settings.replace_range(ps..pe, &sect);
        self.commit_k29d(
            op,
            vec![(SETTINGS.to_string(), settings.into_bytes())],
            vec![],
        )
    }
}

// ---------------------------------------------------------------------------
// SubDocTarget 文字路由（M1.1／M1.2 於子樹的同等實作；§6.5 路由鐵律）
// ---------------------------------------------------------------------------

/// 取子樹的（掃描切片, 前綴, 後綴）：註腳／尾註目標切出目標註元素（鐵律：段落
/// 座標限定該元素內部）；頁首頁尾用整 part。
fn subdoc_slice(
    xml: &str,
    target: &SubDocTarget,
) -> Result<(String, String, String), MutationError> {
    match target {
        SubDocTarget::Footnote { note_id } | SubDocTarget::Endnote { note_id } => {
            let kind = match target {
                SubDocTarget::Footnote { .. } => NoteKind2::Footnote,
                _ => NoteKind2::Endnote,
            };
            let (s, e) = note_element(xml, kind, *note_id).expect("上游已守門");
            Ok((
                xml[s..e].to_string(),
                xml[..s].to_string(),
                xml[e..].to_string(),
            ))
        }
        _ => Ok((xml.to_string(), String::new(), String::new())),
    }
}

fn subdoc_para(
    slice: &str,
    para: usize,
) -> Result<(ParagraphSpan, crate::mutation::ParagraphAnalysis), MutationError> {
    let spans = para_spans(slice);
    let sp = *spans.get(para).ok_or(MutationError::ParagraphOutOfBounds {
        para,
        len: spans.len(),
    })?;
    Ok((sp, analyze_paragraph(slice.as_bytes(), sp)))
}

impl Engine {
    fn k29d_subdoc_insert_text(
        &mut self,
        op: Mutation,
        target: &SubDocTarget,
        para: usize,
        cp: usize,
        text: &str,
    ) -> Result<MutationOutcome, MutationError> {
        if text.is_empty() {
            return Err(rejected("空字串插入不產生變更，拒絕"));
        }
        let part = resolve_subdoc_part(self, target)?;
        let xml = part_text(self, &part)?;
        let (slice, prefix, suffix) = subdoc_slice(&xml, target)?;
        let (_sp, analysis) = subdoc_para(&slice, para)?;
        let cp = cp.min(analysis.total_cp);
        let inject_run = format!(r#"<w:r>{}</w:r>"#, make_text_tag(text));
        let mut out = slice.clone();
        // 注入點兩式：節點中部拆 run／cp=0（含段尾）一律插段落起點（pPr 之後）。
        // cp=0 走段首而非「首 run 之前」的取捨（K29d 探針實據）：hf_table_header
        // 的 footer1.xml 首段是 fldChar 複雜欄，插進 begin～end 之間的結果 run 前
        // 會被欄位標記波及——代換層把新 run 一併覆寫成頁碼（「頁尾覆核」變「2」）。
        // 段首注入在欄位結構之外，Word 游標段首語意一致且零欄位污染。
        let mid = analysis
            .text_nodes
            .iter()
            .find(|n| n.start_cp < cp && cp < n.end_cp && n.run_span.is_some());
        if let Some(n) = mid {
            let run = enclosing_run(&slice, n.node_span.start)
                .ok_or_else(|| rejected("run 區間解析失敗"))?;
            let run_raw = &slice[run.0..run.1];
            let open_gt = run_raw
                .find('>')
                .ok_or_else(|| rejected("run 開標籤解析失敗"))?;
            let run_open = &run_raw[..=open_gt];
            let close_rel = run_raw
                .rfind("</w:r>")
                .ok_or_else(|| rejected("run 閉標籤解析失敗"))?;
            let run_inner = &run_raw[open_gt + 1..close_rel];
            let rpr = leading_rpr(run_inner);
            let chars: Vec<char> = n.unescaped.chars().collect();
            let local = (cp - n.start_cp).min(chars.len());
            let left: String = chars[..local].iter().collect();
            let right: String = chars[local..].iter().collect();
            let _ = run_inner;
            let replacement = format!(
                "{run_open}{}</w:r>{inject_run}<w:r>{}{}</w:r>",
                make_text_tag(&left),
                rpr,
                make_text_tag(&right),
            );
            out.replace_range(run.0..run.1, &replacement);
        } else {
            // cp=0（或整段無可拆文字節點）：插段落起點＝pPr 之後（無 pPr 則 p 開
            // 標籤之後）。此位置在 fldChar 欄位 begin 之前／段內欄位結構之外。
            let at = analysis
                .ppr_end_offset
                .unwrap_or(analysis.p_open_end_offset);
            out.replace_range(at..at, &inject_run);
        }
        let final_xml = format!("{prefix}{out}{suffix}");
        self.commit_k29d(op, vec![(part, final_xml.into_bytes())], vec![])
    }

    fn k29d_subdoc_delete_text(
        &mut self,
        op: Mutation,
        target: &SubDocTarget,
        para: usize,
        start: usize,
        end: usize,
    ) -> Result<MutationOutcome, MutationError> {
        if start >= end {
            return Err(MutationError::CodepointOutOfBounds {
                para,
                start_cp: start,
                end_cp: end,
                para_len: 0,
            });
        }
        let part = resolve_subdoc_part(self, target)?;
        let xml = part_text(self, &part)?;
        let (slice, prefix, suffix) = subdoc_slice(&xml, target)?;
        let (_sp, analysis) = subdoc_para(&slice, para)?;
        if end > analysis.total_cp {
            return Err(MutationError::CodepointOutOfBounds {
                para,
                start_cp: start,
                end_cp: end,
                para_len: analysis.total_cp,
            });
        }
        // 逐文字節點摘除 [start..end) 覆蓋段（逆序位移以保持區間有效）。
        let mut out = slice.clone();
        for n in analysis.text_nodes.iter().rev() {
            let a = n.start_cp.max(start);
            let b = n.end_cp.min(end);
            if a >= b {
                continue;
            }
            let chars: Vec<char> = n.unescaped.chars().collect();
            let mut kept: String = chars[..a - n.start_cp].iter().collect();
            kept.extend(chars[b - n.start_cp..].iter());
            out.replace_range(n.text_span.start..n.text_span.end, &xml_escape(&kept));
        }
        let final_xml = format!("{prefix}{out}{suffix}");
        self.commit_k29d(op, vec![(part, final_xml.into_bytes())], vec![])
    }
}

#[cfg(test)]
mod k29d_unit_probes {
    use super::*;

    /// `locate_element` 回歸鎖：attempt 1 的根因修復（自閉合判準 off-by-one——
    /// `text[start..=gt]` 尾元就是 `>`，舊寫法 `ends_with('/')` 恆假）必須守死，
    /// 否則 pgSz／cols 全部定位失敗、燈 1 全紅。
    #[test]
    fn locate_element_self_closing_and_nested() {
        let seg = r#"<w:sectPr><w:pgSz w:w="11900" w:h="16840"/><w:pgMar w:top="1440"/><w:cols w:space="425"/></w:sectPr>"#;
        assert!(
            locate_element(seg, "w:pgSz", 0).is_some(),
            "自閉合 pgSz 必須定位成功"
        );
        assert!(
            locate_element(seg, "w:cols", 0).is_some(),
            "自閉合 cols 必須定位成功"
        );
        assert_eq!(
            locate_element(seg, "w:sectPr", 0),
            Some((0, seg.len())),
            "含嵌套子元素的 sectPr 必須回整元素半開區間"
        );
        // 前綴同名元素不得誤命中（邊界字元判準：`<w:col` 命中 `<w:cols` 須續掃）。
        assert_eq!(
            locate_element(seg, "w:col", 0),
            None,
            "前綴誤命中（w:col vs w:cols）必須回 None"
        );
    }
}
