//! 編輯 API Mutation 型錄、Surgical XML Patch 與 Undo/Redo 實作（K29a / Slice 1）。
//!
//! 依據 `docs/edit-api-design.md` §2、§3 與 §5 規範實作：
//! - 正文文字編輯鎖定正準碼點偏移（cp: usize），不含清單 marker 或點引合成字元。
//! - 外科手術式 XML 修補（Surgical XML Patch）僅替換受影響節點的位元組切片，
//!   未觸碰段落、表格、未知屬性（w:rsidR、w14:paraId、mc:AlternateContent）原樣保留。
//! - 支援精確逆操作（InverseMutation）與 touched 自動退場機制。

use crate::Engine;
use quire_layout::Invalidation;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 文件子樹目標（決定操作哪一個 XML part 的段落／表格空間）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubDocTarget {
    /// 正文：對應 `"word/document.xml"` 的 `<w:body>`
    Body,
    /// 頁首：對應第 `section_id` 節之 `slot`（Default/First/Even）所指向的 `"word/header*.xml"`
    Header { section_id: u32, slot: HfSlotKind },
    /// 頁尾：對應第 `section_id` 節之 `slot`（Default/First/Even）所指向的 `"word/footer*.xml"`
    Footer { section_id: u32, slot: HfSlotKind },
    /// 註腳本體：對應 `"word/footnotes.xml"` 中 `@w:id == note_id` 的 `<w:footnote>`
    Footnote { note_id: i32 },
    /// 尾註本體：對應 `"word/endnotes.xml"` 中 `@w:id == note_id` 的 `<w:endnote>`
    Endnote { note_id: i32 },
    /// 圖形／VML 文字框內部：對應宿主段落 `host_para` 第 `box_index` 個 `<w:txbxContent>`
    TextBox { host_para: usize, box_index: usize },
}

/// 頁首頁尾槽位種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HfSlotKind {
    Default,
    First,
    Even,
}

/// 段內正準游標位置（以 char 碼點計）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocPos {
    pub para: usize,
    pub cp: usize,
}

/// 半開選取範圍（保證 start <= end）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocRange {
    pub target: SubDocTarget,
    pub start: DocPos,
    pub end: DocPos,
}

/// 斷點種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakKind {
    /// 軟換行 `<w:br/>`
    LineWrapping,
    /// 硬分頁 `<w:br w:type="page"/>`
    Page,
    /// 分欄斷點 `<w:br w:type="column"/>`
    Column,
    /// 製表符 `<w:tab/>`
    Tab,
}

/// 字元級直接格式覆寫（預留擴充）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunDirectPatch {}

/// Mutation 操作枚舉。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Mutation {
    /// M1.1 游標處插入文字
    InsertText {
        target: SubDocTarget,
        pos: DocPos,
        text: String,
        style_override: Option<RunDirectPatch>,
    },
    /// M1.2 單段或跨段範圍刪除（Slice 1 僅支援單段）
    DeleteRange { range: DocRange },
    /// M1.5 插入斷點
    InsertBreak {
        target: SubDocTarget,
        pos: DocPos,
        kind: BreakKind,
    },
}

/// 半開位元組區間。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

/// 局部 XML 切片逆修補（保證未知屬性與未動位元組 100% 還原）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XmlReversePatch {
    pub part_name: String,
    /// 正向修補寫入後的半開位元組區間（Undo 時將此區間替換回 original_slice）
    pub applied_span: ByteSpan,
    /// 正向修補前被覆蓋之原始 XML 局部切片
    pub original_slice: Vec<u8>,
}

/// 語義層逆向操作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SemanticInverseOp {
    Mutation(Mutation),
}

/// 容器層新增或移除之臨時媒體／附屬 part 逆動作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartLifecycleInverse {
    RemoveSynthesizedPart(String),
    RestoreRemovedPart { name: String, bytes: Vec<u8> },
}

/// 單筆 Mutation 的精確逆操作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InverseMutation {
    pub semantic_inverse: SemanticInverseOp,
    pub xml_patches: Vec<XmlReversePatch>,
    pub part_lifecycle_inverse: Vec<PartLifecycleInverse>,
}

/// 不可逆原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IrreversibleReason {
    MediaPoolExceeded,
    ExternalSetPart,
    Custom(String),
}

/// 操作可逆性等級。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReversibilityLevel {
    StrictByteReversible,
    Irreversible { reason: IrreversibleReason },
}

/// 交易級 Undo 堆疊項目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UndoEntry {
    pub tx_id: u64,
    pub inverse_ops: Vec<InverseMutation>,
    pub caret_before: Option<DocPos>,
    pub caret_after: Option<DocPos>,
    pub reversibility: ReversibilityLevel,
}

/// Mutation 套用結果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationOutcome {
    pub events: Vec<Invalidation>,
    pub touched_parts: Vec<String>,
    pub inverse: Vec<InverseMutation>,
    pub caret_after: Option<DocPos>,
}

/// 誠實拒絕不可應用操作的錯誤型別。
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum MutationError {
    #[error("目標子樹不存在或未掛載: {target:?}")]
    SubDocNotFound { target: SubDocTarget },
    #[error("段落索引越界: 請求 {para}，總段數 {len}")]
    ParagraphOutOfBounds { para: usize, len: usize },
    #[error("碼點偏移越界或區間倒置: 段 {para} 長度 {para_len}，請求 ({start_cp}..{end_cp})")]
    CodepointOutOfBounds {
        para: usize,
        start_cp: usize,
        end_cp: usize,
        para_len: usize,
    },
    #[error("跨容器邊界非法: 起點與終點不位於同一儲存格或同一文字框子樹")]
    CrossContainerBoundary { start_para: usize, end_para: usize },
    #[error("受保護的結構節點不可刪除或拆裂: {reason}")]
    ProtectedStructure { reason: String },
    #[error("樣式引用非法或型別不符: style_id={style_id}，期望型別={expected_type}")]
    InvalidStyleReference {
        style_id: String,
        expected_type: String,
    },
    #[error("清單編號引用不存在: num_id={num_id}, ilvl={ilvl}")]
    InvalidNumberingReference { num_id: u32, ilvl: u8 },
    #[error("表格結構座標越界或合併區間非法: {detail}")]
    InvalidTableOperation { detail: String },
    #[error("圖片或媒體格式不支援或超過容量上限: {detail}")]
    InvalidMediaPayload { detail: String },
    #[error("XML 語法樹定位失敗或位於唯讀修訂/替換子樹: {detail}")]
    XmlPatchRejected { detail: String },
    #[error("引擎錯誤: {0}")]
    Engine(String),
}

/// 內部 XML 手術補丁。
#[derive(Debug, Clone)]
pub struct XmlBytePatch {
    pub span: ByteSpan,
    pub replacement: Vec<u8>,
}

/// 判斷字元是否為 XML 空白字元。
pub fn is_xml_space(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\r' || c == '\n'
}

/// XML 跳脫字元處理。
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// XML 反跳脫字元處理。
pub fn xml_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            let mut entity = String::new();
            for ec in chars.by_ref() {
                if ec == ';' {
                    break;
                }
                entity.push(ec);
            }
            match entity.as_str() {
                "amp" => out.push('&'),
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "quot" => out.push('"'),
                "apos" => out.push('\''),
                _ => {
                    out.push('&');
                    out.push_str(&entity);
                    out.push(';');
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 段落節點在 XML 中的位元組區間。
#[derive(Debug, Clone, Copy)]
pub struct ParagraphSpan {
    pub start: usize,
    pub end: usize,
}

/// 依據解析器口徑掃描並定位指定索引的扁平段落位元組區間。
pub fn find_paragraph_span(
    xml: &[u8],
    target_para: usize,
) -> Result<(ParagraphSpan, usize), MutationError> {
    let text = match std::str::from_utf8(xml) {
        Ok(t) => t,
        Err(_) => {
            return Err(MutationError::XmlPatchRejected {
                detail: "XML 明文非合法 UTF-8".to_string(),
            })
        }
    };

    let mut cursor = 0;
    let mut fb_depth = 0usize;
    let mut ac_stack = Vec::new();
    let mut txbx_depth = 0usize;
    let mut para_index = 0usize;
    let n = text.len();

    let mut target_span: Option<ParagraphSpan> = None;

    while cursor < n {
        let lt = match text[cursor..].find('<') {
            Some(i) => cursor + i,
            None => break,
        };
        let gt = match text[lt..].find('>') {
            Some(i) => lt + i,
            None => break,
        };
        let tag = &text[lt..=gt];
        cursor = gt + 1;

        let is_close = tag.starts_with("</");
        let is_empty = tag.ends_with("/>");
        let inner = if is_close {
            &tag[2..tag.len() - 1]
        } else if is_empty {
            &tag[1..tag.len() - 2]
        } else {
            &tag[1..tag.len() - 1]
        };
        let name = inner.split_whitespace().next().unwrap_or("");
        let local = name.rsplit(':').next().unwrap_or("");

        if !is_close {
            if local == "AlternateContent" {
                ac_stack.push(fb_depth);
            } else if local == "Fallback" {
                fb_depth += 1;
            }
        } else if local == "Fallback" {
            fb_depth = fb_depth.saturating_sub(1);
        } else if local == "AlternateContent" {
            if let Some(prev) = ac_stack.pop() {
                fb_depth = prev;
            }
        }

        if fb_depth > 0 {
            continue;
        }

        if !is_close {
            if local == "txbxContent" {
                txbx_depth += 1;
            }
        } else if local == "txbxContent" {
            txbx_depth = txbx_depth.saturating_sub(1);
        }

        if txbx_depth > 0 {
            continue;
        }

        if !is_close && !is_empty && local == "p" {
            let p_start = lt;
            let mut p_depth = 1usize;
            let mut search_cur = gt + 1;
            let mut p_end = None;

            while search_cur < n {
                let next_lt = match text[search_cur..].find('<') {
                    Some(i) => search_cur + i,
                    None => break,
                };
                let next_gt = match text[next_lt..].find('>') {
                    Some(i) => next_lt + i,
                    None => break,
                };
                let t2 = &text[next_lt..=next_gt];
                search_cur = next_gt + 1;

                let ic = t2.starts_with("</");
                let ie = t2.ends_with("/>");
                let in2 = if ic {
                    &t2[2..t2.len() - 1]
                } else if ie {
                    &t2[1..t2.len() - 2]
                } else {
                    &t2[1..t2.len() - 1]
                };
                let nm2 = in2.split_whitespace().next().unwrap_or("");
                let loc2 = nm2.rsplit(':').next().unwrap_or("");

                if loc2 == "p" {
                    if !ic && !ie {
                        p_depth += 1;
                    } else if ic {
                        p_depth = p_depth.saturating_sub(1);
                        if p_depth == 0 {
                            p_end = Some(next_gt + 1);
                            cursor = next_gt + 1;
                            break;
                        }
                    }
                }
            }

            if let Some(end) = p_end {
                if para_index == target_para {
                    target_span = Some(ParagraphSpan {
                        start: p_start,
                        end,
                    });
                }
                para_index += 1;
            }
        }
    }

    if let Some(span) = target_span {
        Ok((span, para_index))
    } else {
        Err(MutationError::ParagraphOutOfBounds {
            para: target_para,
            len: para_index,
        })
    }
}

/// 段落內 `<w:t>` 節點定位資訊。
#[derive(Debug, Clone)]
pub struct TextNodeInfo {
    /// `<w:t ...>...</w:t>` 整個標籤的半開位元組區間
    pub node_span: ByteSpan,
    /// 內容文字的半開位元組區間
    pub text_span: ByteSpan,
    /// 是否帶有 xml:space="preserve"
    pub preserve_space: bool,
    /// 反跳脫後的純文字
    pub unescaped: String,
    /// 碼點在段落中的起點（含）
    pub start_cp: usize,
    /// 碼點在段落中的終點（不含）
    pub end_cp: usize,
    /// 所屬 run 的 `<w:r>` 起訖位元組區間
    pub run_span: Option<ByteSpan>,
}

/// 段落內部結構分析結果。
#[derive(Debug, Clone)]
pub struct ParagraphAnalysis {
    pub ppr_end_offset: Option<usize>,
    pub p_open_end_offset: usize,
    pub p_close_start_offset: usize,
    pub text_nodes: Vec<TextNodeInfo>,
    pub total_cp: usize,
}

/// 解析單一段落 XML 內部的文字節點、換行與斷點結構。
pub fn analyze_paragraph(xml: &[u8], p_span: ParagraphSpan) -> ParagraphAnalysis {
    let p_slice = &xml[p_span.start..p_span.end];
    let text = std::str::from_utf8(p_slice).unwrap_or("");

    let first_gt = text.find('>').unwrap_or(0);
    let p_open_end = p_span.start + first_gt + 1;
    let p_close_start = p_span.start + text.rfind("</").unwrap_or(text.len());

    let mut ppr_end_offset = None;
    let mut text_nodes = Vec::new();
    let mut current_cp = 0usize;

    let mut cursor = 0;
    let mut current_run_span: Option<ByteSpan> = None;

    while cursor < text.len() {
        let lt = match text[cursor..].find('<') {
            Some(i) => cursor + i,
            None => break,
        };
        let gt = match text[lt..].find('>') {
            Some(i) => lt + i,
            None => break,
        };
        let tag = &text[lt..=gt];
        cursor = gt + 1;

        let is_close = tag.starts_with("</");
        let is_empty = tag.ends_with("/>");
        let inner = if is_close {
            &tag[2..tag.len() - 1]
        } else if is_empty {
            &tag[1..tag.len() - 2]
        } else {
            &tag[1..tag.len() - 1]
        };
        let name = inner.split_whitespace().next().unwrap_or("");
        let local = name.rsplit(':').next().unwrap_or("");

        if local == "pPr" && (is_close || is_empty) {
            ppr_end_offset = Some(p_span.start + gt + 1);
        } else if local == "r" && !is_close && !is_empty {
            // 尋找對應的 </w:r>
            let r_start = p_span.start + lt;
            if let Some(r_close_rel) = text[cursor..].find("</") {
                let r_close_lt = cursor + r_close_rel;
                if let Some(r_close_gt_rel) = text[r_close_lt..].find('>') {
                    let r_end = p_span.start + r_close_lt + r_close_gt_rel + 1;
                    current_run_span = Some(ByteSpan {
                        start: r_start,
                        end: r_end,
                    });
                }
            }
        } else if local == "r" && is_close {
            current_run_span = None;
        } else if local == "br" && is_empty {
            let has_page_type = inner.contains(r#"type="page""#)
                || inner.contains(r#"type='page'"#)
                || inner.contains(r#"w:type="page""#);
            let has_col_type = inner.contains(r#"type="column""#)
                || inner.contains(r#"type='column'"#)
                || inner.contains(r#"w:type="column""#);
            if !has_page_type && !has_col_type {
                // 軟換行算 1 個碼點 (\n)
                current_cp += 1;
            }
        } else if local == "t" && !is_close && !is_empty {
            let preserve = inner.contains(r#"xml:space="preserve""#)
                || inner.contains(r#"xml:space='preserve'"#)
                || inner.contains(r#"space="preserve""#);

            // 尋找 </w:t>
            let t_open_end = lt + gt - lt + 1;
            if let Some(t_close_rel) = text[t_open_end..].find("</") {
                let t_close_lt = t_open_end + t_close_rel;
                if let Some(t_close_gt_rel) = text[t_close_lt..].find('>') {
                    let t_close_end = t_close_lt + t_close_gt_rel + 1;
                    let raw_content = &text[t_open_end..t_close_lt];
                    let unescaped_raw = xml_unescape(raw_content);
                    let final_str = if preserve {
                        unescaped_raw
                    } else {
                        unescaped_raw.trim_matches(is_xml_space).to_string()
                    };
                    let char_count = final_str.chars().count();
                    let start_cp = current_cp;
                    let end_cp = current_cp + char_count;
                    current_cp = end_cp;

                    text_nodes.push(TextNodeInfo {
                        node_span: ByteSpan {
                            start: p_span.start + lt,
                            end: p_span.start + t_close_end,
                        },
                        text_span: ByteSpan {
                            start: p_span.start + t_open_end,
                            end: p_span.start + t_close_lt,
                        },
                        preserve_space: preserve,
                        unescaped: final_str,
                        start_cp,
                        end_cp,
                        run_span: current_run_span,
                    });
                    cursor = t_close_end;
                }
            }
        }
    }

    ParagraphAnalysis {
        ppr_end_offset,
        p_open_end_offset: p_open_end,
        p_close_start_offset: p_close_start,
        text_nodes,
        total_cp: current_cp,
    }
}

/// 從段落 XML 切片提取純文字（同步 LayoutEngine 用）。
pub fn extract_para_text_from_xml(xml: &[u8], p_span: ParagraphSpan) -> String {
    let analysis = analyze_paragraph(xml, p_span);
    let mut out = String::new();
    for node in analysis.text_nodes {
        out.push_str(&node.unescaped);
    }
    out
}

impl Engine {
    /// 依據 Mutation 規格套用單筆變更。
    pub fn apply_mutation(&mut self, op: Mutation) -> Result<MutationOutcome, MutationError> {
        let (applied_span, original_slice, affected_para, caret_after, new_xml) = match &op {
            Mutation::InsertText {
                target,
                pos,
                text,
                style_override: _,
            } => {
                if *target != SubDocTarget::Body {
                    return Err(MutationError::SubDocNotFound {
                        target: target.clone(),
                    });
                }
                let part_name = "word/document.xml";
                let xml = self.doc.parts.get(part_name).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "找不到 word/document.xml".to_string(),
                    }
                })?;

                let (p_span, _total_paras) = find_paragraph_span(xml, pos.para)?;
                let analysis = analyze_paragraph(xml, p_span);

                if pos.cp > analysis.total_cp {
                    return Err(MutationError::CodepointOutOfBounds {
                        para: pos.para,
                        start_cp: pos.cp,
                        end_cp: pos.cp,
                        para_len: analysis.total_cp,
                    });
                }

                let mut replaced_span = None;
                let mut replacement_bytes = Vec::new();

                // 嘗試尋找涵蓋該碼點之既有 <w:t>
                for node in &analysis.text_nodes {
                    if (node.start_cp <= pos.cp && pos.cp < node.end_cp)
                        || (pos.cp == node.end_cp && pos.cp == analysis.total_cp)
                    {
                        let local_cp = pos.cp - node.start_cp;
                        let mut new_text = String::new();
                        let mut count = 0;
                        for ch in node.unescaped.chars() {
                            if count == local_cp {
                                new_text.push_str(text);
                            }
                            new_text.push(ch);
                            count += 1;
                        }
                        if count == local_cp {
                            new_text.push_str(text);
                        }

                        let needs_preserve = node.preserve_space
                            || new_text.starts_with(is_xml_space)
                            || new_text.ends_with(is_xml_space);
                        let new_tag = if needs_preserve {
                            format!(
                                r#"<w:t xml:space="preserve">{}</w:t>"#,
                                xml_escape(&new_text)
                            )
                        } else {
                            format!(r#"<w:t>{}</w:t>"#, xml_escape(&new_text))
                        };

                        replaced_span = Some(node.node_span);
                        replacement_bytes = new_tag.into_bytes();
                        break;
                    }
                }

                if replaced_span.is_none() {
                    // 段落內無 <w:t>，在 pPr 後或段首插入新 run
                    let ins_offset = analysis
                        .ppr_end_offset
                        .unwrap_or(analysis.p_open_end_offset);
                    let needs_preserve =
                        text.starts_with(is_xml_space) || text.ends_with(is_xml_space);
                    let new_run = if needs_preserve {
                        format!(
                            r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                            xml_escape(text)
                        )
                    } else {
                        format!(r#"<w:r><w:t>{}</w:t></w:r>"#, xml_escape(text))
                    };
                    replaced_span = Some(ByteSpan {
                        start: ins_offset,
                        end: ins_offset,
                    });
                    replacement_bytes = new_run.into_bytes();
                }

                let span = replaced_span.unwrap();
                let orig_slice = xml[span.start..span.end].to_vec();

                let mut new_xml =
                    Vec::with_capacity(xml.len() + replacement_bytes.len() - orig_slice.len());
                new_xml.extend_from_slice(&xml[..span.start]);
                new_xml.extend_from_slice(&replacement_bytes);
                new_xml.extend_from_slice(&xml[span.end..]);

                let applied = ByteSpan {
                    start: span.start,
                    end: span.start + replacement_bytes.len(),
                };

                let caret_after = DocPos {
                    para: pos.para,
                    cp: pos.cp + text.chars().count(),
                };

                (applied, orig_slice, pos.para, Some(caret_after), new_xml)
            }

            Mutation::DeleteRange { range } => {
                if range.target != SubDocTarget::Body {
                    return Err(MutationError::SubDocNotFound {
                        target: range.target.clone(),
                    });
                }
                if range.start.para != range.end.para {
                    return Err(MutationError::ProtectedStructure {
                        reason: "跨段刪除留待 Slice 2 實作".to_string(),
                    });
                }
                let part_name = "word/document.xml";
                let xml = self.doc.parts.get(part_name).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "找不到 word/document.xml".to_string(),
                    }
                })?;

                let (p_span, _total_paras) = find_paragraph_span(xml, range.start.para)?;
                let analysis = analyze_paragraph(xml, p_span);

                if range.start.cp > range.end.cp || range.end.cp > analysis.total_cp {
                    return Err(MutationError::CodepointOutOfBounds {
                        para: range.start.para,
                        start_cp: range.start.cp,
                        end_cp: range.end.cp,
                        para_len: analysis.total_cp,
                    });
                }

                if range.start.cp == range.end.cp {
                    return Ok(MutationOutcome {
                        events: Vec::new(),
                        touched_parts: Vec::new(),
                        inverse: Vec::new(),
                        caret_after: Some(range.start),
                    });
                }

                let mut replaced_span = None;
                let mut replacement_bytes = Vec::new();

                // 處理單一 <w:t> 內部的局部刪除
                for node in &analysis.text_nodes {
                    if node.start_cp <= range.start.cp && range.end.cp <= node.end_cp {
                        let local_start = range.start.cp - node.start_cp;
                        let local_end = range.end.cp - node.start_cp;

                        let mut new_text = String::new();
                        for (idx, ch) in node.unescaped.chars().enumerate() {
                            if idx < local_start || idx >= local_end {
                                new_text.push(ch);
                            }
                        }

                        let needs_preserve = node.preserve_space
                            || new_text.starts_with(is_xml_space)
                            || new_text.ends_with(is_xml_space);
                        let new_tag = if needs_preserve {
                            format!(
                                r#"<w:t xml:space="preserve">{}</w:t>"#,
                                xml_escape(&new_text)
                            )
                        } else {
                            format!(r#"<w:t>{}</w:t>"#, xml_escape(&new_text))
                        };

                        replaced_span = Some(node.node_span);
                        replacement_bytes = new_tag.into_bytes();
                        break;
                    }
                }

                if replaced_span.is_none() {
                    return Err(MutationError::XmlPatchRejected {
                        detail: "跨節點刪除尚未支援".to_string(),
                    });
                }

                let span = replaced_span.unwrap();
                let orig_slice = xml[span.start..span.end].to_vec();

                let mut new_xml =
                    Vec::with_capacity(xml.len() + replacement_bytes.len() - orig_slice.len());
                new_xml.extend_from_slice(&xml[..span.start]);
                new_xml.extend_from_slice(&replacement_bytes);
                new_xml.extend_from_slice(&xml[span.end..]);

                let applied = ByteSpan {
                    start: span.start,
                    end: span.start + replacement_bytes.len(),
                };

                let caret_after = DocPos {
                    para: range.start.para,
                    cp: range.start.cp,
                };

                (
                    applied,
                    orig_slice,
                    range.start.para,
                    Some(caret_after),
                    new_xml,
                )
            }

            Mutation::InsertBreak { target, pos, kind } => {
                if *target != SubDocTarget::Body {
                    return Err(MutationError::SubDocNotFound {
                        target: target.clone(),
                    });
                }
                let part_name = "word/document.xml";
                let xml = self.doc.parts.get(part_name).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "找不到 word/document.xml".to_string(),
                    }
                })?;

                let (p_span, _total_paras) = find_paragraph_span(xml, pos.para)?;
                let analysis = analyze_paragraph(xml, p_span);

                if pos.cp > analysis.total_cp {
                    return Err(MutationError::CodepointOutOfBounds {
                        para: pos.para,
                        start_cp: pos.cp,
                        end_cp: pos.cp,
                        para_len: analysis.total_cp,
                    });
                }

                let break_tag = match kind {
                    BreakKind::LineWrapping => "<w:br/>",
                    BreakKind::Page => r#"<w:br w:type="page"/>"#,
                    BreakKind::Column => r#"<w:br w:type="column"/>"#,
                    BreakKind::Tab => "<w:tab/>",
                };

                let mut replaced_span = None;
                let mut replacement_bytes = Vec::new();

                for node in &analysis.text_nodes {
                    if node.start_cp <= pos.cp && pos.cp <= node.end_cp {
                        let local_cp = pos.cp - node.start_cp;
                        if local_cp == 0 {
                            if let Some(r_span) = node.run_span {
                                replaced_span = Some(ByteSpan {
                                    start: r_span.start,
                                    end: r_span.start,
                                });
                                replacement_bytes =
                                    format!("<w:r>{}</w:r>", break_tag).into_bytes();
                                break;
                            }
                        } else if local_cp == node.unescaped.chars().count() {
                            if let Some(r_span) = node.run_span {
                                replaced_span = Some(ByteSpan {
                                    start: r_span.end,
                                    end: r_span.end,
                                });
                                replacement_bytes =
                                    format!("<w:r>{}</w:r>", break_tag).into_bytes();
                                break;
                            }
                        } else {
                            // 拆開 <w:t>
                            let chars: Vec<char> = node.unescaped.chars().collect();
                            let s1: String = chars[..local_cp].iter().collect();
                            let s2: String = chars[local_cp..].iter().collect();

                            let t1 = if node.preserve_space
                                || s1.starts_with(is_xml_space)
                                || s1.ends_with(is_xml_space)
                            {
                                format!(r#"<w:t xml:space="preserve">{}</w:t>"#, xml_escape(&s1))
                            } else {
                                format!(r#"<w:t>{}</w:t>"#, xml_escape(&s1))
                            };

                            let t2 = if node.preserve_space
                                || s2.starts_with(is_xml_space)
                                || s2.ends_with(is_xml_space)
                            {
                                format!(r#"<w:t xml:space="preserve">{}</w:t>"#, xml_escape(&s2))
                            } else {
                                format!(r#"<w:t>{}</w:t>"#, xml_escape(&s2))
                            };

                            let new_content = format!("{}{}{}", t1, break_tag, t2);
                            replaced_span = Some(node.node_span);
                            replacement_bytes = new_content.into_bytes();
                            break;
                        }
                    }
                }

                if replaced_span.is_none() {
                    let ins_offset = analysis.p_close_start_offset;
                    replaced_span = Some(ByteSpan {
                        start: ins_offset,
                        end: ins_offset,
                    });
                    replacement_bytes = format!("<w:r>{}</w:r>", break_tag).into_bytes();
                }

                let span = replaced_span.unwrap();
                let orig_slice = xml[span.start..span.end].to_vec();

                let mut new_xml =
                    Vec::with_capacity(xml.len() + replacement_bytes.len() - orig_slice.len());
                new_xml.extend_from_slice(&xml[..span.start]);
                new_xml.extend_from_slice(&replacement_bytes);
                new_xml.extend_from_slice(&xml[span.end..]);

                let applied = ByteSpan {
                    start: span.start,
                    end: span.start + replacement_bytes.len(),
                };

                let caret_after = DocPos {
                    para: pos.para,
                    cp: pos.cp + 1,
                };

                (applied, orig_slice, pos.para, Some(caret_after), new_xml)
            }
        };

        let part_name = "word/document.xml";
        // [K29A_BUGGY_POINT_1: PARTS_WRITEBACK]
        self.doc.parts.insert(part_name.to_string(), new_xml);

        // 維護 touched 白名單（若位元組與開檔完全相同則自動退場）
        if let (Some(cur), Some(baseline)) = (
            self.doc.parts.get(part_name),
            self.part_baselines.get(part_name),
        ) {
            if cur == baseline {
                self.touched.remove(part_name);
            } else {
                self.touched.insert(part_name.to_string(), ());
            }
        }

        // 同步記憶體語義模型
        let updated_xml = self.doc.parts.get(part_name).unwrap();
        let (updated_p_span, _) = find_paragraph_span(updated_xml, affected_para)?;
        let new_para_text = extract_para_text_from_xml(updated_xml, updated_p_span);

        let _ = self.replace_paragraph_text(affected_para, &new_para_text);

        if let Some(p) = self.doc.paragraphs.get_mut(affected_para) {
            if let Some(first) = p.runs.first_mut() {
                first.text = new_para_text.clone();
                first.field = None;
                for r in p.runs.iter_mut().skip(1) {
                    r.text.clear();
                }
            } else {
                p.runs.push(quire_ooxml::Run {
                    text: new_para_text.clone(),
                    ..Default::default()
                });
            }
        }

        while !self.is_settled() {
            self.reflow_step_json();
        }

        // 建立 Undo 記錄
        let reverse_patch = XmlReversePatch {
            part_name: part_name.to_string(),
            applied_span,
            original_slice,
        };

        let undo_entry = UndoEntry {
            tx_id: self.undo_stack.len() as u64 + 1,
            inverse_ops: vec![InverseMutation {
                semantic_inverse: SemanticInverseOp::Mutation(op),
                xml_patches: vec![reverse_patch],
                part_lifecycle_inverse: Vec::new(),
            }],
            caret_before: None,
            caret_after,
            reversibility: ReversibilityLevel::StrictByteReversible,
        };

        self.undo_stack.push(undo_entry);
        self.redo_stack.clear();

        Ok(MutationOutcome {
            events: Vec::new(),
            touched_parts: vec![part_name.to_string()],
            inverse: Vec::new(),
            caret_after,
        })
    }

    /// 批次套用 Mutation。
    pub fn apply_batch(&mut self, ops: Vec<Mutation>) -> Result<MutationOutcome, MutationError> {
        let mut last_outcome = None;
        for op in ops {
            last_outcome = Some(self.apply_mutation(op)?);
        }
        last_outcome.ok_or_else(|| MutationError::XmlPatchRejected {
            detail: "空批次操作".to_string(),
        })
    }

    /// 復原頂端操作。
    pub fn undo(&mut self) -> Result<Option<MutationOutcome>, MutationError> {
        let entry = match self.undo_stack.pop() {
            Some(e) => e,
            None => return Ok(None),
        };

        let mut affected_parts = Vec::new();

        for inv in entry.inverse_ops.iter().rev() {
            for patch in inv.xml_patches.iter().rev() {
                let xml = self.doc.parts.get_mut(&patch.part_name).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: format!("找不到 part {}", patch.part_name),
                    }
                })?;

                let mut restored_xml = Vec::with_capacity(
                    xml.len() + patch.original_slice.len()
                        - (patch.applied_span.end - patch.applied_span.start),
                );
                restored_xml.extend_from_slice(&xml[..patch.applied_span.start]);
                restored_xml.extend_from_slice(&patch.original_slice);
                restored_xml.extend_from_slice(&xml[patch.applied_span.end..]);
                // [K29A_BUGGY_POINT_2: UNDO_XML_RESTORE]
                *xml = restored_xml;

                affected_parts.push(patch.part_name.clone());
            }
        }

        // touched 自動退場機制
        for part_name in &affected_parts {
            if let (Some(cur), Some(baseline)) = (
                self.doc.parts.get(part_name),
                self.part_baselines.get(part_name),
            ) {
                if cur == baseline {
                    self.touched.remove(part_name);
                } else {
                    self.touched.insert(part_name.clone(), ());
                }
            }
        }

        // 同步語義模型
        let part_name = "word/document.xml";
        let mut updates = Vec::new();
        if let Some(xml) = self.doc.parts.get(part_name) {
            let p_count = self.doc.paragraphs.len();
            for p_idx in 0..p_count {
                if let Ok((p_span, _)) = find_paragraph_span(xml, p_idx) {
                    let text = extract_para_text_from_xml(xml, p_span);
                    updates.push((p_idx, text));
                }
            }
        }
        for (p_idx, text) in updates {
            let _ = self.replace_paragraph_text(p_idx, &text);
            if let Some(p) = self.doc.paragraphs.get_mut(p_idx) {
                if let Some(first) = p.runs.first_mut() {
                    first.text = text;
                    for r in p.runs.iter_mut().skip(1) {
                        r.text.clear();
                    }
                }
            }
        }

        while !self.is_settled() {
            self.reflow_step_json();
        }

        self.redo_stack.push(entry);

        Ok(Some(MutationOutcome {
            events: Vec::new(),
            touched_parts: affected_parts,
            inverse: Vec::new(),
            caret_after: None,
        }))
    }

    /// 重做頂端操作。
    pub fn redo(&mut self) -> Result<Option<MutationOutcome>, MutationError> {
        let entry = match self.redo_stack.pop() {
            Some(e) => e,
            None => return Ok(None),
        };

        // Redo 藉由重新執行原語義操作或逆向補丁
        for inv in &entry.inverse_ops {
            match &inv.semantic_inverse {
                SemanticInverseOp::Mutation(m) => {
                    self.apply_mutation(m.clone())?;
                }
            }
        }

        Ok(Some(MutationOutcome {
            events: Vec::new(),
            touched_parts: vec!["word/document.xml".to_string()],
            inverse: Vec::new(),
            caret_after: entry.caret_after,
        }))
    }
}
