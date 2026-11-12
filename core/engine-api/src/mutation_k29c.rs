//! K29c（Slice 3）Mutation 實作：M4 表格家族與 M5 圖片家族。
//!
//! 紀律：
//! - 表格操作以目標 part 的 XML 明文切片為唯一真相，未觸碰列／格原樣保留；
//!   結構變動後一律 `rebuild_derived()` 重解析，四套扁平段落索引同步平移。
//! - 圖片縮放只修補 `<wp:extent>`／`<a:ext>`／定位節點，`word/media/*` 與
//!   `.rels` 一位元組不碰。
//! - 圖片插入走內容雜湊去重；替換走 CoW（新圖新 part、舊圖不刪）；強制 GC
//!   回收本會話新增媒體時誠實回報不可逆並截斷 Undo 歷史。
//! - 所有可逆操作只保存「整份 part 的舊明文」作為逆補丁（Undo 時完整還原）。

use crate::mutation::{
    ByteSpan, DocPos, FloatingAxisSpec, InsertSide, InverseMutation, IrreversibleReason, Mutation,
    MutationError, MutationOutcome, PartLifecycleInverse, ReversibilityLevel, SemanticInverseOp,
    SubDocTarget, TablePath, TablePropsPatch, UndoEntry, VMergeState, WrapModeSpec,
    XmlReversePatch,
};
use crate::Engine;
use std::collections::BTreeMap;

const EMU_PER_PT: f32 = 12700.0;
const IMAGE_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

/// 本模組是否承接該 Mutation（供 `apply_mutation` 前置分派）。
pub(crate) fn is_k29c_mutation(op: &Mutation) -> bool {
    matches!(
        op,
        Mutation::InsertTable { .. }
            | Mutation::DeleteTable { .. }
            | Mutation::InsertTableRow { .. }
            | Mutation::DeleteTableRow { .. }
            | Mutation::InsertTableColumn { .. }
            | Mutation::DeleteTableColumn { .. }
            | Mutation::MergeTableCells { .. }
            | Mutation::SplitTableCell { .. }
            | Mutation::SetTableProps { .. }
            | Mutation::SetRowProps { .. }
            | Mutation::SetCellProps { .. }
            | Mutation::InsertInlineImage { .. }
            | Mutation::InsertFloatingImage { .. }
            | Mutation::ResizeOrMoveImage { .. }
            | Mutation::ReplaceImageMedia { .. }
            | Mutation::DeleteImage { .. }
    )
}

// ---------------------------------------------------------------------------
// XML 明文切片輔助
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Tag {
    start: usize,
    end: usize,
    local: String,
    closing: bool,
    empty: bool,
}

fn next_tag(text: &str, from: usize) -> Option<Tag> {
    let mut cursor = from;
    loop {
        if cursor >= text.len() {
            return None;
        }
        let lt = text[cursor..].find('<')? + cursor;
        if text[lt..].starts_with("<!--") {
            let close = text[lt + 4..].find("-->")? + lt + 4 + 3;
            cursor = close;
            continue;
        }
        if text[lt..].starts_with("<?") {
            let close = text[lt + 2..].find("?>")? + lt + 2 + 2;
            cursor = close;
            continue;
        }
        if text[lt..].starts_with("<!") {
            let close = text[lt..].find('>')? + lt + 1;
            cursor = close;
            continue;
        }
        let gt = text[lt..].find('>')? + lt;
        let tag = &text[lt..=gt];
        let closing = tag.starts_with("</");
        let empty = !closing && tag[..tag.len().saturating_sub(1)].trim_end().ends_with('/');
        let inner = if closing {
            &tag[2..tag.len().saturating_sub(1)]
        } else if empty {
            &tag[1..tag.len().saturating_sub(2)]
        } else {
            &tag[1..tag.len().saturating_sub(1)]
        };
        let name_end = inner
            .find(|c: char| c.is_whitespace() || c == '/')
            .unwrap_or(inner.len());
        let name = &inner[..name_end];
        let local = name.rsplit(':').next().unwrap_or("").to_string();
        if local.is_empty() {
            cursor = gt + 1;
            continue;
        }
        return Some(Tag {
            start: lt,
            end: gt + 1,
            local,
            closing,
            empty,
        });
    }
}

fn tag_at(text: &str, start: usize) -> Option<Tag> {
    if text.get(start..).is_some_and(|s| s.starts_with('<')) {
        next_tag(text, start)
    } else {
        None
    }
}

fn element_end(text: &str, start: usize) -> Option<usize> {
    let first = tag_at(text, start)?;
    if first.closing || first.empty {
        return Some(first.end);
    }
    let mut depth = 1usize;
    let mut cursor = first.end;
    while let Some(t) = next_tag(text, cursor) {
        if t.local == first.local {
            if t.closing {
                depth -= 1;
                if depth == 0 {
                    return Some(t.end);
                }
            } else if !t.empty {
                depth += 1;
            }
        }
        cursor = t.end;
    }
    None
}

/// 父元素直屬子元素（同名巢狀由 `element_end` 整段跳過）。
fn direct_children(text: &str, parent_start: usize, filter: &str) -> Vec<(usize, usize)> {
    let Some(parent) = tag_at(text, parent_start) else {
        return Vec::new();
    };
    if parent.empty || parent.closing {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cursor = parent.end;
    let mut depth = 0usize;
    while let Some(t) = next_tag(text, cursor) {
        if t.local == parent.local && depth == 0 && t.closing {
            break;
        }
        if depth == 0 && !t.closing && t.local == filter {
            let end = element_end(text, t.start).unwrap_or(t.end);
            out.push((t.start, end));
            cursor = end;
            continue;
        }
        if !t.closing && !t.empty {
            depth += 1;
        } else if t.closing {
            depth = depth.saturating_sub(1);
        }
        cursor = t.end;
    }
    out
}

fn first_child(text: &str, parent_start: usize, filter: &str) -> Option<(usize, usize)> {
    direct_children(text, parent_start, filter)
        .into_iter()
        .next()
}

fn find_local_tag(text: &str, local: &str) -> Option<usize> {
    let mut cursor = 0usize;
    while let Some(t) = next_tag(text, cursor) {
        if !t.closing && t.local == local {
            return Some(t.start);
        }
        cursor = t.end;
    }
    None
}

fn tag_open_string(text: &str, start: usize) -> String {
    let Some(gt) = text[start..].find('>') else {
        return String::new();
    };
    text[start..start + gt + 1].to_string()
}

fn attr_in_tag(tag: &str, name: &str) -> Option<String> {
    for q in ['"', '\''] {
        let pat = format!(" {name}={q}");
        if let Some(pos) = tag.find(&pat) {
            let rest = &tag[pos + pat.len()..];
            if let Some(end) = rest.find(q) {
                return Some(rest[..end].to_string());
            }
        }
        // 開標籤第一個屬性前是標籤名，不是空白：補一次無前導空白的退化比對。
        let pat2 = format!("{name}={q}");
        if tag.starts_with(&format!("<{name}")) {
            continue;
        }
        if let Some(pos) = tag.find(&pat2) {
            let rest = &tag[pos + pat2.len()..];
            if let Some(end) = rest.find(q) {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

fn set_attr_in_tag(tag: &str, name: &str, value: &str) -> String {
    for q in ['"', '\''] {
        let pat = format!(" {name}={q}");
        if let Some(pos) = tag.find(&pat) {
            let rest_start = pos + pat.len();
            if let Some(end) = tag[rest_start..].find(q) {
                let end = rest_start + end;
                let mut out = String::new();
                out.push_str(&tag[..rest_start]);
                out.push_str(value);
                out.push_str(&tag[end..]);
                return out;
            }
        }
    }
    let insert_at = tag
        .rfind("/>")
        .or_else(|| tag.rfind('>'))
        .unwrap_or(tag.len());
    let mut out = String::new();
    out.push_str(&tag[..insert_at]);
    out.push_str(&format!(" {name}=\"{}\"", xml_attr_escape(value)));
    out.push_str(&tag[insert_at..]);
    out
}

fn xml_attr_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn remove_named_element(body: &str, name: &str) -> String {
    let mut out = body.to_string();
    let open = format!("<{name}");
    while let Some(start) = out.find(&open) {
        let after = &out[start + open.len()..];
        if !after
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/')
        {
            break;
        }
        let Some(gt_rel) = out[start..].find('>') else {
            break;
        };
        let gt = start + gt_rel;
        if out[start..gt].trim_end().ends_with('/') {
            out.replace_range(start..=gt, "");
            continue;
        }
        let close = format!("</{name}>");
        let Some(end_rel) = out[gt..].find(&close) else {
            break;
        };
        let end = gt + end_rel + close.len();
        out.replace_range(start..end, "");
    }
    out
}

fn apply_replacements(xml: &[u8], reps: &[(ByteSpan, Vec<u8>)]) -> Result<Vec<u8>, MutationError> {
    let mut sorted = reps.to_vec();
    sorted.sort_by_key(|(s, _)| s.start);
    for w in sorted.windows(2) {
        if w[0].0.end > w[1].0.start {
            return Err(MutationError::XmlPatchRejected {
                detail: "K29c XML 修補區間重疊".to_string(),
            });
        }
    }
    let mut out = xml.to_vec();
    for (span, bytes) in sorted.into_iter().rev() {
        if span.end > out.len() || span.start > span.end {
            return Err(MutationError::XmlPatchRejected {
                detail: "K29c XML 修補區間越界".to_string(),
            });
        }
        let mut next = Vec::with_capacity(out.len() + bytes.len() - (span.end - span.start));
        next.extend_from_slice(&out[..span.start]);
        next.extend_from_slice(&bytes);
        next.extend_from_slice(&out[span.end..]);
        out = next;
    }
    Ok(out)
}

fn text_of(xml: &[u8]) -> Result<&str, MutationError> {
    std::str::from_utf8(xml).map_err(|_| MutationError::XmlPatchRejected {
        detail: "XML 明文非合法 UTF-8".to_string(),
    })
}

// ---------------------------------------------------------------------------
// 表格定位與版面輔助
// ---------------------------------------------------------------------------

fn part_for_target(target: &SubDocTarget) -> Result<&'static str, MutationError> {
    match target {
        SubDocTarget::Body => Ok("word/document.xml"),
        other => Err(MutationError::SubDocNotFound {
            target: other.clone(),
        }),
    }
}

fn locate_table(text: &str, path: &TablePath) -> Result<(usize, usize), MutationError> {
    if path.target != SubDocTarget::Body {
        return Err(MutationError::SubDocNotFound {
            target: path.target.clone(),
        });
    }
    let body_start =
        find_local_tag(text, "body").ok_or_else(|| MutationError::XmlPatchRejected {
            detail: "找不到 <w:body>".to_string(),
        })?;
    let roots = direct_children(text, body_start, "tbl");
    let (mut start, mut end) =
        *roots
            .get(path.root_slot)
            .ok_or_else(|| MutationError::InvalidTableOperation {
                detail: format!("根表序 {} 越界（共 {} 張）", path.root_slot, roots.len()),
            })?;
    for (step_idx, (row, cell, sub)) in path.nested_steps.iter().enumerate() {
        let rows = direct_children(text, start, "tr");
        let row_span = *rows
            .get(*row)
            .ok_or_else(|| MutationError::InvalidTableOperation {
                detail: format!("巢狀步 {step_idx} 列 {row} 越界"),
            })?;
        let cells = direct_children(text, row_span.0, "tc");
        let cell_span = *cells
            .get(*cell)
            .ok_or_else(|| MutationError::InvalidTableOperation {
                detail: format!("巢狀步 {step_idx} 格 {cell} 越界"),
            })?;
        let tables = direct_children(text, cell_span.0, "tbl");
        let sub_span = *tables
            .get(*sub)
            .ok_or_else(|| MutationError::InvalidTableOperation {
                detail: format!("巢狀步 {step_idx} 子表 {sub} 越界"),
            })?;
        start = sub_span.0;
        end = sub_span.1;
    }
    Ok((start, end))
}

#[derive(Debug, Clone)]
struct CellLayout {
    start: usize,
    end: usize,
    tcpr: Option<(usize, usize)>,
    col_start: usize,
    span: usize,
}

#[derive(Debug, Clone)]
struct RowLayout {
    start: usize,
    end: usize,
    cells: Vec<CellLayout>,
}

fn cell_grid_span(text: &str, tcpr: Option<(usize, usize)>) -> usize {
    let Some((s, e)) = tcpr else {
        return 1;
    };
    let raw = &text[s..e];
    attr_in_tag(raw, "w:val")
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(1)
}

fn row_layout(text: &str, row: (usize, usize)) -> RowLayout {
    let mut col = 0usize;
    let mut cells = Vec::new();
    for (start, end) in direct_children(text, row.0, "tc") {
        let tcpr = first_child(text, start, "tcPr");
        let span = cell_grid_span(text, tcpr);
        cells.push(CellLayout {
            start,
            end,
            tcpr,
            col_start: col,
            span,
        });
        col += span;
    }
    RowLayout {
        start: row.0,
        end: row.1,
        cells,
    }
}

fn row_layouts(text: &str, table_start: usize) -> Vec<RowLayout> {
    direct_children(text, table_start, "tr")
        .into_iter()
        .map(|r| row_layout(text, r))
        .collect()
}

fn table_grid_span(text: &str, table_start: usize) -> Option<(usize, usize)> {
    first_child(text, table_start, "tblGrid")
}

fn grid_col_spans(text: &str, table_start: usize) -> Vec<(usize, usize)> {
    match table_grid_span(text, table_start) {
        Some((s, _)) => direct_children(text, s, "gridCol"),
        None => Vec::new(),
    }
}

fn twips(pt: f32) -> i64 {
    (pt * 20.0).round() as i64
}

fn table_props_xml(patch: &TablePropsPatch) -> String {
    let mut add = String::new();
    if let Some(w) = patch.width_pt {
        add.push_str(&format!(
            r#"<w:tblW w:w="{}" w:type="dxa"/>"#,
            twips(w).max(0)
        ));
    }
    if let Some(jc) = &patch.alignment {
        add.push_str(&format!(r#"<w:jc w:val="{}"/>"#, xml_attr_escape(jc)));
    }
    if let Some(fill) = &patch.shading_fill {
        add.push_str(&format!(
            r#"<w:shd w:val="clear" w:color="auto" w:fill="{}"/>"#,
            xml_attr_escape(fill)
        ));
    }
    add
}

fn row_props_xml(patch: &crate::mutation::RowPropsPatch) -> String {
    let mut add = String::new();
    if let Some(h) = patch.height_pt {
        add.push_str(&format!(
            r#"<w:trHeight w:val="{}" w:hRule="atLeast"/>"#,
            twips(h).max(0)
        ));
    }
    if patch.header == Some(true) {
        add.push_str("<w:tblHeader/>");
    }
    if patch.cant_split == Some(true) {
        add.push_str("<w:cantSplit/>");
    }
    add
}

fn cell_props_xml(patch: &crate::mutation::CellPropsPatch) -> String {
    let mut add = String::new();
    if let Some(w) = patch.width_pt {
        add.push_str(&format!(
            r#"<w:tcW w:w="{}" w:type="dxa"/>"#,
            twips(w).max(0)
        ));
    }
    if let Some(span) = patch.grid_span {
        if span > 1 {
            add.push_str(&format!(r#"<w:gridSpan w:val="{span}"/>"#));
        }
    }
    match patch.v_merge {
        Some(VMergeState::Restart) => add.push_str(r#"<w:vMerge w:val="restart"/>"#),
        Some(VMergeState::Continue) => add.push_str("<w:vMerge/>"),
        Some(VMergeState::None) | None => {}
    }
    if let Some(fill) = &patch.shading_fill {
        add.push_str(&format!(
            r#"<w:shd w:val="clear" w:color="auto" w:fill="{}"/>"#,
            xml_attr_escape(fill)
        ));
    }
    if let Some(v) = &patch.vertical_align {
        add.push_str(&format!(r#"<w:vAlign w:val="{}"/>"#, xml_attr_escape(v)));
    }
    add
}

fn replace_tcpr_reps(
    text: &str,
    cell: &CellLayout,
    patch: &crate::mutation::CellPropsPatch,
) -> Result<(ByteSpan, Vec<u8>), MutationError> {
    let old = match cell.tcpr {
        Some((s, e)) => text[s..e].to_string(),
        None => String::new(),
    };
    if cell.tcpr.is_none() {
        let Some(tc) = tag_at(text, cell.start) else {
            return Err(MutationError::XmlPatchRejected {
                detail: "w:tc 開標籤解析失敗".to_string(),
            });
        };
        let new_tcpr = format!("<w:tcPr>{}</w:tcPr>", cell_props_xml(patch));
        return Ok((
            ByteSpan {
                start: tc.end,
                end: tc.end,
            },
            new_tcpr.into_bytes(),
        ));
    }
    let (start, end) = cell.tcpr.unwrap();
    let gt = old.find('>').unwrap_or(0);
    let body = old[gt + 1..old.len() - "</w:tcPr>".len()].to_string();
    let mut body = body;
    if patch.width_pt.is_some() {
        body = remove_named_element(&body, "w:tcW");
    }
    if patch.grid_span.is_some() {
        body = remove_named_element(&body, "w:gridSpan");
    }
    if patch.v_merge.is_some() {
        body = remove_named_element(&body, "w:vMerge");
    }
    if patch.shading_fill.is_some() {
        body = remove_named_element(&body, "w:shd");
    }
    if patch.vertical_align.is_some() {
        body = remove_named_element(&body, "w:vAlign");
    }
    let new_tcpr = format!("<w:tcPr>{body}{}</w:tcPr>", cell_props_xml(patch));
    Ok((ByteSpan { start, end }, new_tcpr.into_bytes()))
}

fn set_vmerge_in_tcpr(tcpr: &str, state: VMergeState) -> String {
    let body = if tcpr.is_empty() {
        String::new()
    } else {
        let gt = tcpr.find('>').unwrap_or(0);
        tcpr[gt + 1..tcpr.len() - "</w:tcPr>".len()].to_string()
    };
    let body = remove_named_element(&body, "w:vMerge");
    let add = match state {
        VMergeState::Restart => r#"<w:vMerge w:val="restart"/>"#,
        VMergeState::Continue => "<w:vMerge/>",
        VMergeState::None => "",
    };
    format!("<w:tcPr>{body}{add}</w:tcPr>")
}

fn set_gridspan_in_tcpr(tcpr: &str, span: usize) -> String {
    let body = if tcpr.is_empty() {
        String::new()
    } else {
        let gt = tcpr.find('>').unwrap_or(0);
        tcpr[gt + 1..tcpr.len() - "</w:tcPr>".len()].to_string()
    };
    let body = remove_named_element(&body, "w:gridSpan");
    if span > 1 {
        format!(r#"<w:tcPr>{body}<w:gridSpan w:val="{span}"/></w:tcPr>"#)
    } else {
        format!("<w:tcPr>{body}</w:tcPr>")
    }
}

// ---------------------------------------------------------------------------
// 圖片／媒體輔助
// ---------------------------------------------------------------------------

struct MediaPlan {
    media_part: String,
    created: bool,
    rels_part: String,
    rels_new: Option<Vec<u8>>,
    /// `.rels` 在開檔基線不存在（本次會話新建）：Undo 必須整份移除而非還原成空 part。
    rels_created: bool,
    rid: String,
    content_types_new: Option<Vec<u8>>,
}

struct K29cCommit {
    updates: Vec<(String, Vec<u8>)>,
    additions: Vec<(String, Vec<u8>)>,
    removals: Vec<String>,
    touched: Vec<String>,
    caret: Option<DocPos>,
    irreversibility: Option<ReversibilityLevel>,
}

impl K29cCommit {
    fn new() -> Self {
        Self {
            updates: Vec::new(),
            additions: Vec::new(),
            removals: Vec::new(),
            touched: Vec::new(),
            caret: None,
            irreversibility: None,
        }
    }

    fn update(&mut self, name: impl Into<String>, bytes: Vec<u8>) {
        let name = name.into();
        self.touched.push(name.clone());
        self.updates.push((name, bytes));
    }

    fn add(&mut self, name: impl Into<String>, bytes: Vec<u8>) {
        let name = name.into();
        self.touched.push(name.clone());
        self.additions.push((name, bytes));
    }

    fn remove(&mut self, name: impl Into<String>) {
        self.removals.push(name.into());
    }
}

fn media_ext(mime: &str) -> Option<&'static str> {
    match mime.to_ascii_lowercase().as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpeg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" | "image/svg" => Some("svg"),
        _ => None,
    }
}

fn validate_media(bytes: &[u8], mime: &str) -> Result<&'static str, MutationError> {
    if bytes.is_empty() {
        return Err(MutationError::InvalidMediaPayload {
            detail: "媒體位元組為空".to_string(),
        });
    }
    let ext = media_ext(mime).ok_or_else(|| MutationError::InvalidMediaPayload {
        detail: format!("不支援的 MIME 型別: {mime}"),
    })?;
    let ok = match ext {
        "png" => bytes.starts_with(&[0x89, b'P', b'N', b'G']),
        "jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        "svg" => {
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_lowercase();
            head.trim_start().starts_with('<') && head.contains("<svg")
        }
        _ => false,
    };
    if !ok {
        return Err(MutationError::InvalidMediaPayload {
            detail: format!("圖片檔頭與 MIME 不符（宣告 {mime}）"),
        });
    }
    Ok(ext)
}

fn content_type_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn relationship_target(base_part: &str, target: &str) -> String {
    let dir = base_part.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut segs: Vec<&str> = if target.starts_with('/') || dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for seg in target.trim_start_matches('/').split('/') {
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

fn rels_part_for(base_part: &str) -> String {
    let (dir, file) = base_part.rsplit_once('/').unwrap_or(("", base_part));
    if dir.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{dir}/_rels/{file}.rels")
    }
}

fn max_doc_pr_id(text: &str) -> u32 {
    let mut max = 0u32;
    let mut cursor = 0usize;
    while let Some(t) = next_tag(text, cursor) {
        if !t.closing && t.local == "docPr" {
            let tag = tag_open_string(text, t.start);
            if let Some(v) = attr_in_tag(&tag, "id").and_then(|v| v.parse::<u32>().ok()) {
                max = max.max(v);
            }
        }
        cursor = t.end;
    }
    max
}

fn next_rid(text: &str) -> String {
    let mut max = 0u32;
    let mut cursor = 0usize;
    while let Some(t) = next_tag(text, cursor) {
        if !t.closing && t.local == "Relationship" {
            let tag = tag_open_string(text, t.start);
            if let Some(id) = attr_in_tag(&tag, "Id") {
                if let Some(n) = id.strip_prefix("rId").and_then(|n| n.parse::<u32>().ok()) {
                    max = max.max(n);
                }
            }
        }
        cursor = t.end;
    }
    format!("rId{}", max + 1)
}

fn drawing_xml(
    rid: &str,
    name: &str,
    doc_pr_id: u32,
    w_pt: f32,
    h_pt: f32,
    alt: Option<&str>,
) -> String {
    let cx = (w_pt * EMU_PER_PT).round() as i64;
    let cy = (h_pt * EMU_PER_PT).round() as i64;
    let descr = alt
        .map(|a| format!(r#" descr="{}""#, xml_attr_escape(a)))
        .unwrap_or_default();
    format!(
        concat!(
            r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0" "#,
            r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">"#,
            r#"<wp:extent cx="{cx}" cy="{cy}"/>"#,
            r#"<wp:docPr id="{id}" name="{name}"{descr}/>"#,
            r#"<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
            r#"<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
            r#"<pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
            r#"<pic:nvPicPr><pic:cNvPr id="{id}" name="{name}"/><pic:cNvPicPr/></pic:nvPicPr>"#,
            r#"<pic:blipFill><a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="{rid}"/>"#,
            r#"<a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
            r#"<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>"#,
            r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>"#,
            r#"</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#,
        ),
        cx = cx,
        cy = cy,
        id = doc_pr_id,
        name = xml_attr_escape(name),
        descr = descr,
        rid = rid,
    )
}

fn axis_child_xml(axis: &FloatingAxisSpec, is_h: bool) -> String {
    let tag = if is_h { "wp:positionH" } else { "wp:positionV" };
    let rel = xml_attr_escape(&axis.relative_from);
    if let Some(align) = &axis.align {
        format!(
            r#"<{tag} relativeFrom="{rel}"><wp:align>{}</wp:align></{tag}>"#,
            xml_attr_escape(align)
        )
    } else {
        let off = (axis.offset_pt.unwrap_or(0.0) * EMU_PER_PT).round() as i64;
        format!(r#"<{tag} relativeFrom="{rel}"><wp:posOffset>{off}</wp:posOffset></{tag}>"#)
    }
}

fn wrap_xml(mode: WrapModeSpec) -> &'static str {
    match mode {
        WrapModeSpec::None => "<wp:wrapNone/>",
        WrapModeSpec::Square => r#"<wp:wrapSquare wrapText="bothSides"/>"#,
        WrapModeSpec::Tight => r#"<wp:wrapTight wrapText="bothSides"/>"#,
        WrapModeSpec::Through => r#"<wp:wrapThrough wrapText="bothSides"/>"#,
        WrapModeSpec::TopAndBottom => "<wp:wrapTopAndBottom/>",
    }
}

#[allow(clippy::too_many_arguments)] // 錨點兩軸、繞圖、z 序與尺寸皆為獨立必要輸入，不塞中間結構體
fn floating_drawing_xml(
    rid: &str,
    name: &str,
    doc_pr_id: u32,
    w_pt: f32,
    h_pt: f32,
    pos_h: &FloatingAxisSpec,
    pos_v: &FloatingAxisSpec,
    wrap_mode: WrapModeSpec,
    behind_doc: bool,
) -> String {
    let cx = (w_pt * EMU_PER_PT).round() as i64;
    let cy = (h_pt * EMU_PER_PT).round() as i64;
    format!(
        concat!(
            r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" "#,
            r#"relativeHeight="{id}" behindDoc="{behind}" locked="0" layoutInCell="1" allowOverlap="1" "#,
            r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">"#,
            r#"<wp:simplePos x="0" y="0"/>"#,
            r#"{pos_h}{pos_v}"#,
            r#"<wp:extent cx="{cx}" cy="{cy}"/>"#,
            r#"<wp:effectExtent l="0" t="0" r="0" b="0"/>"#,
            r#"{wrap}"#,
            r#"<wp:docPr id="{id}" name="{name}"/>"#,
            r#"<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
            r#"<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
            r#"<pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
            r#"<pic:nvPicPr><pic:cNvPr id="{id}" name="{name}"/><pic:cNvPicPr/></pic:nvPicPr>"#,
            r#"<pic:blipFill><a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="{rid}"/>"#,
            r#"<a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
            r#"<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>"#,
            r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>"#,
            r#"</pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#,
        ),
        cx = cx,
        cy = cy,
        id = doc_pr_id,
        name = xml_attr_escape(name),
        behind = if behind_doc { 1 } else { 0 },
        pos_h = axis_child_xml(pos_h, true),
        pos_v = axis_child_xml(pos_v, false),
        wrap = wrap_xml(wrap_mode),
        rid = rid,
    )
}

fn drawings_in_span(text: &str, span: (usize, usize)) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut cursor = span.0;
    while cursor < span.1 {
        let Some(t) = next_tag(text, cursor) else {
            break;
        };
        if t.start >= span.1 {
            break;
        }
        if !t.closing && (t.local == "drawing" || t.local == "pict") {
            let end = element_end(text, t.start).unwrap_or(t.end);
            out.push((t.start, end));
            cursor = end;
            continue;
        }
        cursor = t.end;
    }
    out
}

fn drawing_embed_rid(text: &str, span: (usize, usize)) -> Option<String> {
    let mut cursor = span.0;
    while cursor < span.1 {
        let Some(t) = next_tag(text, cursor) else {
            break;
        };
        if !t.closing {
            let tag = tag_open_string(text, t.start);
            if t.local == "blip" {
                if let Some(v) = attr_in_tag(&tag, "r:embed") {
                    return Some(v);
                }
            }
            if t.local == "imagedata" {
                if let Some(v) = attr_in_tag(&tag, "r:id") {
                    return Some(v);
                }
            }
        }
        cursor = t.end;
    }
    None
}

fn replace_embed_xml(text: &str, span: (usize, usize), old: &str, new: &str) -> String {
    let mut cursor = span.0;
    let mut reps = Vec::new();
    while cursor < span.1 {
        let Some(t) = next_tag(text, cursor) else {
            break;
        };
        if !t.closing {
            let tag = tag_open_string(text, t.start);
            let mut changed = false;
            for name in ["r:embed", "r:id"] {
                if attr_in_tag(&tag, name).as_deref() == Some(old) {
                    changed = true;
                }
            }
            if changed {
                let mut next = tag.clone();
                for name in ["r:embed", "r:id"] {
                    if attr_in_tag(&next, name).as_deref() == Some(old) {
                        next = set_attr_in_tag(&next, name, new);
                    }
                }
                reps.push((
                    ByteSpan {
                        start: t.start,
                        end: t.end,
                    },
                    next.into_bytes(),
                ));
            }
        }
        cursor = t.end;
    }
    let mut out = text.to_string();
    for (span, bytes) in reps.into_iter().rev() {
        out.replace_range(span.start..span.end, &String::from_utf8_lossy(&bytes));
    }
    out
}

// ---------------------------------------------------------------------------
// Engine 實作
// ---------------------------------------------------------------------------

impl Engine {
    pub(crate) fn apply_k29c_mutation(
        &mut self,
        op: Mutation,
    ) -> Result<MutationOutcome, MutationError> {
        match op.clone() {
            Mutation::InsertTable {
                target,
                after_para,
                rows,
                cols,
                col_widths_pt,
            } => self.k29c_insert_table(op, target, after_para, rows, cols, col_widths_pt),
            Mutation::DeleteTable { table } => self.k29c_delete_table(op, table),
            Mutation::InsertTableRow {
                table,
                at_row,
                position,
            } => self.k29c_insert_row(op, table, at_row, position),
            Mutation::DeleteTableRow { table, row } => self.k29c_delete_row(op, table, row),
            Mutation::InsertTableColumn {
                table,
                at_grid_col,
                position,
                width_pt,
            } => self.k29c_insert_column(op, table, at_grid_col, position, width_pt),
            Mutation::DeleteTableColumn { table, grid_col } => {
                self.k29c_delete_column(op, table, grid_col)
            }
            Mutation::MergeTableCells {
                table,
                top_row,
                bottom_row,
                left_grid_col,
                right_grid_col,
            } => self.k29c_merge_cells(
                op,
                table,
                top_row,
                bottom_row,
                left_grid_col,
                right_grid_col,
            ),
            Mutation::SplitTableCell {
                table,
                row,
                cell,
                into_rows,
                into_cols,
            } => self.k29c_split_cell(op, table, row, cell, into_rows, into_cols),
            Mutation::SetTableProps { table, patch } => self.k29c_set_table_props(op, table, patch),
            Mutation::SetRowProps { table, row, patch } => {
                self.k29c_set_row_props(op, table, row, patch)
            }
            Mutation::SetCellProps {
                table,
                row,
                cell,
                patch,
            } => self.k29c_set_cell_props(op, table, row, cell, patch),
            Mutation::InsertInlineImage {
                target,
                pos,
                media_bytes,
                mime_type,
                width_pt,
                height_pt,
                alt_text,
            } => self.k29c_insert_image(
                op,
                target,
                Some(pos),
                None,
                media_bytes,
                mime_type,
                width_pt,
                height_pt,
                alt_text,
                None,
            ),
            Mutation::InsertFloatingImage {
                target,
                host_para,
                media_bytes,
                mime_type,
                width_pt,
                height_pt,
                pos_h,
                pos_v,
                wrap_mode,
                behind_doc,
            } => self.k29c_insert_image(
                op,
                target,
                None,
                Some(host_para),
                media_bytes,
                mime_type,
                width_pt,
                height_pt,
                None,
                Some((pos_h, pos_v, wrap_mode, behind_doc)),
            ),
            Mutation::ResizeOrMoveImage {
                target,
                host_para,
                drawing_index,
                new_width_pt,
                new_height_pt,
                new_pos_h,
                new_pos_v,
                new_wrap_mode,
            } => self.k29c_resize_image(
                op,
                target,
                host_para,
                drawing_index,
                new_width_pt,
                new_height_pt,
                new_pos_h,
                new_pos_v,
                new_wrap_mode,
            ),
            Mutation::ReplaceImageMedia {
                target,
                host_para,
                drawing_index,
                new_media_bytes,
                new_mime_type,
            } => self.k29c_replace_image(
                op,
                target,
                host_para,
                drawing_index,
                new_media_bytes,
                new_mime_type,
            ),
            Mutation::DeleteImage {
                target,
                host_para,
                drawing_index,
                gc_unreferenced_media,
            } => {
                self.k29c_delete_image(op, target, host_para, drawing_index, gc_unreferenced_media)
            }
            _ => Err(MutationError::XmlPatchRejected {
                detail: "非 K29c Mutation 誤入 K29c 實作".to_string(),
            }),
        }
    }

    fn commit_k29c(
        &mut self,
        op: Mutation,
        mut commit: K29cCommit,
    ) -> Result<MutationOutcome, MutationError> {
        let mut patches = Vec::new();
        let mut lifecycle = Vec::new();
        let mut touched_parts = Vec::new();

        for (name, bytes) in std::mem::take(&mut commit.updates) {
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
            self.touched.insert(name.clone(), ());
            touched_parts.push(name);
        }
        for (name, bytes) in std::mem::take(&mut commit.additions) {
            self.doc.set_part(&name, bytes);
            self.touched.insert(name.clone(), ());
            lifecycle.push(PartLifecycleInverse::RemoveSynthesizedPart(name.clone()));
            touched_parts.push(name);
        }
        for name in std::mem::take(&mut commit.removals) {
            if self.doc.remove_part(&name) {
                self.touched.remove(&name);
            }
            touched_parts.push(name);
        }
        for name in &commit.touched {
            self.touched.insert(name.clone(), ());
        }

        let reversibility = commit
            .irreversibility
            .clone()
            .unwrap_or(ReversibilityLevel::StrictByteReversible);

        if matches!(reversibility, ReversibilityLevel::Irreversible { .. }) {
            // 誠實紅線：媒體位元組已釋放，依賴它的歷史全部失效。
            self.undo_stack.clear();
            self.redo_stack.clear();
        } else {
            let entry = UndoEntry {
                tx_id: self.undo_stack.len() as u64 + 1,
                inverse_ops: vec![InverseMutation {
                    semantic_inverse: SemanticInverseOp::Mutation(op),
                    xml_patches: patches,
                    part_lifecycle_inverse: lifecycle,
                }],
                caret_before: None,
                caret_after: commit.caret,
                reversibility: reversibility.clone(),
            };
            self.undo_stack.push(entry);
            self.redo_stack.clear();
        }

        self.rebuild_derived()
            .map_err(|e| MutationError::Engine(e.to_string()))?;
        while !self.is_settled() {
            self.reflow_step_json();
        }

        Ok(MutationOutcome {
            events: Vec::new(),
            touched_parts,
            inverse: Vec::new(),
            caret_after: commit.caret,
            reversibility,
        })
    }

    fn body_part(&self) -> Result<(&'static str, Vec<u8>), MutationError> {
        let name = "word/document.xml";
        let xml =
            self.doc
                .parts
                .get(name)
                .cloned()
                .ok_or_else(|| MutationError::XmlPatchRejected {
                    detail: "找不到 word/document.xml".to_string(),
                })?;
        Ok((name, xml))
    }

    // ------------------------- M4.1 / M4.2 -------------------------

    fn k29c_insert_table(
        &mut self,
        op: Mutation,
        target: SubDocTarget,
        after_para: Option<usize>,
        rows: usize,
        cols: usize,
        col_widths_pt: Option<Vec<f32>>,
    ) -> Result<MutationOutcome, MutationError> {
        if rows == 0 || cols == 0 {
            return Err(MutationError::InvalidTableOperation {
                detail: "rows／cols 必須大於 0".to_string(),
            });
        }
        let part = part_for_target(&target)?;
        if part != "word/document.xml" {
            return Err(MutationError::SubDocNotFound { target });
        }
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let insert_at = match after_para {
            Some(p) => {
                let (span, _) = crate::mutation::find_paragraph_span(&xml, p)?;
                span.end
            }
            None => {
                let body = find_local_tag(text, "body").ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "找不到 <w:body>".to_string(),
                    }
                })?;
                tag_at(text, body).map(|t| t.end).ok_or_else(|| {
                    MutationError::XmlPatchRejected {
                        detail: "w:body 開標籤解析失敗".to_string(),
                    }
                })?
            }
        };
        let widths: Vec<f32> = (0..cols)
            .map(|i| {
                col_widths_pt
                    .as_ref()
                    .and_then(|v| v.get(i).copied())
                    .filter(|w| w.is_finite() && *w > 0.0)
                    .unwrap_or(100.0)
            })
            .collect();
        let mut grid = String::from("<w:tblGrid>");
        for w in &widths {
            grid.push_str(&format!(r#"<w:gridCol w:w="{}"/>"#, twips(*w).max(0)));
        }
        grid.push_str("</w:tblGrid>");
        let mut table = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="{}" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr>{grid}"#,
            twips(widths.iter().sum()).max(0)
        );
        for _ in 0..rows {
            table.push_str("<w:tr>");
            for w in &widths {
                table.push_str(&format!(
                    r#"<w:tc><w:tcPr><w:tcW w:w="{}" w:type="dxa"/></w:tcPr><w:p></w:p></w:tc>"#,
                    twips(*w).max(0)
                ));
            }
            table.push_str("</w:tr>");
        }
        table.push_str("</w:tbl>");
        let new_xml = apply_replacements(
            &xml,
            &[(
                ByteSpan {
                    start: insert_at,
                    end: insert_at,
                },
                table.into_bytes(),
            )],
        )?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        commit.caret = Some(DocPos {
            para: after_para.map(|p| p + 1).unwrap_or(0),
            cp: 0,
        });
        self.commit_k29c(op, commit)
    }

    fn k29c_delete_table(
        &mut self,
        op: Mutation,
        table: TablePath,
    ) -> Result<MutationOutcome, MutationError> {
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let (start, end) = locate_table(text, &table)?;
        let new_xml = apply_replacements(&xml, &[(ByteSpan { start, end }, Vec::new())])?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.commit_k29c(op, commit)
    }

    // ------------------------- M4.3 列 -------------------------

    fn k29c_insert_row(
        &mut self,
        op: Mutation,
        table: TablePath,
        at_row: usize,
        position: InsertSide,
    ) -> Result<MutationOutcome, MutationError> {
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let (table_start, table_end) = locate_table(text, &table)?;
        let rows = direct_children(text, table_start, "tr");
        let template = *rows
            .get(at_row)
            .ok_or_else(|| MutationError::InvalidTableOperation {
                detail: format!("列序 {at_row} 越界（共 {} 列）", rows.len()),
            })?;
        let row = row_layout(text, template);
        if row.cells.is_empty() {
            return Err(MutationError::InvalidTableOperation {
                detail: "基準列沒有儲存格".to_string(),
            });
        }
        let mut new_row = String::from("<w:tr>");
        for cell in &row.cells {
            let tcpr = match cell.tcpr {
                Some((s, e)) => text[s..e].to_string(),
                None => "<w:tcPr></w:tcPr>".to_string(),
            };
            let has_vmerge = tcpr.contains("<w:vMerge");
            let mut tcpr_new = tcpr;
            if has_vmerge {
                tcpr_new = set_vmerge_in_tcpr(&tcpr_new, VMergeState::Continue);
            }
            new_row.push_str(&format!("<w:tc>{tcpr_new}<w:p></w:p></w:tc>"));
        }
        new_row.push_str("</w:tr>");
        let insert_at = match position {
            InsertSide::Before => template.0,
            InsertSide::After => template.1,
        };
        let _ = table_end;
        let new_xml = apply_replacements(
            &xml,
            &[(
                ByteSpan {
                    start: insert_at,
                    end: insert_at,
                },
                new_row.into_bytes(),
            )],
        )?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.commit_k29c(op, commit)
    }

    fn k29c_delete_row(
        &mut self,
        op: Mutation,
        table: TablePath,
        row_idx: usize,
    ) -> Result<MutationOutcome, MutationError> {
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let (table_start, _) = locate_table(text, &table)?;
        let rows = direct_children(text, table_start, "tr");
        if rows.len() <= 1 {
            return Err(MutationError::InvalidTableOperation {
                detail: "不得刪除表格僅剩的一列（請改用 DeleteTable）".to_string(),
            });
        }
        let target = *rows
            .get(row_idx)
            .ok_or_else(|| MutationError::InvalidTableOperation {
                detail: format!("列序 {row_idx} 越界（共 {} 列）", rows.len()),
            })?;
        let target_layout = row_layout(text, target);
        let mut reps = vec![(
            ByteSpan {
                start: target.0,
                end: target.1,
            },
            Vec::new(),
        )];

        // vMerge 頂格刪除後，下一列同欄 continue 晉升 restart，避免孤兒延續格。
        if let Some(next) = rows.get(row_idx + 1) {
            let next_layout = row_layout(text, *next);
            for cell in &target_layout.cells {
                let tcpr = match cell.tcpr {
                    Some((s, e)) => &text[s..e],
                    None => continue,
                };
                let restart = attr_in_tag(tcpr, "w:val").as_deref() == Some("restart");
                if !restart || !tcpr.contains("<w:vMerge") {
                    continue;
                }
                for ncell in &next_layout.cells {
                    let overlaps = ncell.col_start < cell.col_start + cell.span
                        && cell.col_start < ncell.col_start + ncell.span;
                    if !overlaps {
                        continue;
                    }
                    let Some((s, e)) = ncell.tcpr else { continue };
                    let ntcpr = &text[s..e];
                    if ntcpr.contains("<w:vMerge") {
                        let promoted = set_vmerge_in_tcpr(ntcpr, VMergeState::Restart);
                        reps.push((ByteSpan { start: s, end: e }, promoted.into_bytes()));
                    }
                }
            }
        }
        let new_xml = apply_replacements(&xml, &reps)?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.commit_k29c(op, commit)
    }

    // ------------------------- M4.4 欄 -------------------------

    fn k29c_insert_column(
        &mut self,
        op: Mutation,
        table: TablePath,
        at_grid_col: usize,
        position: InsertSide,
        width_pt: f32,
    ) -> Result<MutationOutcome, MutationError> {
        if !width_pt.is_finite() || width_pt <= 0.0 {
            return Err(MutationError::InvalidTableOperation {
                detail: "插入欄寬必須為有限正數".to_string(),
            });
        }
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let (table_start, _) = locate_table(text, &table)?;
        let grid_cols = grid_col_spans(text, table_start);
        let insert_at = match position {
            InsertSide::Before => at_grid_col,
            InsertSide::After => at_grid_col + 1,
        };
        if insert_at > grid_cols.len() {
            return Err(MutationError::InvalidTableOperation {
                detail: format!("欄插入點 {insert_at} 越界（網格 {} 欄）", grid_cols.len()),
            });
        }
        let mut reps = Vec::new();
        let grid_insert = if insert_at < grid_cols.len() {
            grid_cols[insert_at].0
        } else {
            table_grid_span(text, table_start)
                .map(|(s, e)| e.saturating_sub("</w:tblGrid>".len()).max(s))
                .ok_or_else(|| MutationError::InvalidTableOperation {
                    detail: "表格缺少 w:tblGrid".to_string(),
                })?
        };
        reps.push((
            ByteSpan {
                start: grid_insert,
                end: grid_insert,
            },
            format!(r#"<w:gridCol w:w="{}"/>"#, twips(width_pt).max(0)).into_bytes(),
        ));

        for row in row_layouts(text, table_start) {
            let row_open_end = tag_at(text, row.start).map(|t| t.end).unwrap_or(row.start);
            let mut inserted = false;
            for cell in &row.cells {
                let cell_end_col = cell.col_start + cell.span;
                if insert_at == cell.col_start {
                    let new_cell = format!(
                        r#"<w:tc><w:tcPr><w:tcW w:w="{}" w:type="dxa"/></w:tcPr><w:p></w:p></w:tc>"#,
                        twips(width_pt).max(0)
                    );
                    reps.push((
                        ByteSpan {
                            start: cell.start,
                            end: cell.start,
                        },
                        new_cell.into_bytes(),
                    ));
                    inserted = true;
                    break;
                }
                if insert_at > cell.col_start && insert_at < cell_end_col {
                    let tcpr = match cell.tcpr {
                        Some((s, e)) => &text[s..e],
                        None => "",
                    };
                    let old_span = cell.span;
                    let new_tcpr = set_gridspan_in_tcpr(tcpr, old_span + 1);
                    let (s, e) = cell.tcpr.unwrap_or((cell.start, cell.start));
                    let rep = if cell.tcpr.is_some() {
                        (ByteSpan { start: s, end: e }, new_tcpr.into_bytes())
                    } else {
                        (
                            ByteSpan {
                                start: row_open_end,
                                end: row_open_end,
                            },
                            new_tcpr.into_bytes(),
                        )
                    };
                    reps.push(rep);
                    inserted = true;
                    break;
                }
            }
            if !inserted {
                let new_cell = format!(
                    r#"<w:tc><w:tcPr><w:tcW w:w="{}" w:type="dxa"/></w:tcPr><w:p></w:p></w:tc>"#,
                    twips(width_pt).max(0)
                );
                reps.push((
                    ByteSpan {
                        start: row.end,
                        end: row.end,
                    },
                    new_cell.into_bytes(),
                ));
            }
        }

        let new_xml = apply_replacements(&xml, &reps)?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.commit_k29c(op, commit)
    }

    fn k29c_delete_column(
        &mut self,
        op: Mutation,
        table: TablePath,
        grid_col: usize,
    ) -> Result<MutationOutcome, MutationError> {
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let (table_start, _) = locate_table(text, &table)?;
        let grid_cols = grid_col_spans(text, table_start);
        if grid_cols.len() <= 1 {
            return Err(MutationError::InvalidTableOperation {
                detail: "不得刪除表格僅剩的一欄（請改用 DeleteTable）".to_string(),
            });
        }
        let gspan =
            *grid_cols
                .get(grid_col)
                .ok_or_else(|| MutationError::InvalidTableOperation {
                    detail: format!("網格欄 {grid_col} 越界（共 {} 欄）", grid_cols.len()),
                })?;
        let mut reps = vec![(
            ByteSpan {
                start: gspan.0,
                end: gspan.1,
            },
            Vec::new(),
        )];

        for row in row_layouts(text, table_start) {
            for cell in &row.cells {
                let cell_end_col = cell.col_start + cell.span;
                let contains = cell.col_start <= grid_col && grid_col < cell_end_col;
                if !contains {
                    continue;
                }
                if cell.span > 1 {
                    let tcpr = match cell.tcpr {
                        Some((s, e)) => &text[s..e],
                        None => "",
                    };
                    let new_tcpr = set_gridspan_in_tcpr(tcpr, cell.span - 1);
                    let (s, e) = cell.tcpr.unwrap_or((cell.start, cell.start));
                    let rep = if cell.tcpr.is_some() {
                        (ByteSpan { start: s, end: e }, new_tcpr.into_bytes())
                    } else {
                        (
                            ByteSpan {
                                start: cell.start,
                                end: cell.start,
                            },
                            new_tcpr.into_bytes(),
                        )
                    };
                    reps.push(rep);
                } else {
                    reps.push((
                        ByteSpan {
                            start: cell.start,
                            end: cell.end,
                        },
                        Vec::new(),
                    ));
                }
            }
        }
        let new_xml = apply_replacements(&xml, &reps)?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.commit_k29c(op, commit)
    }

    // ------------------------- M4.5 合併／分割 -------------------------

    fn k29c_merge_cells(
        &mut self,
        op: Mutation,
        table: TablePath,
        top_row: usize,
        bottom_row: usize,
        left_grid_col: usize,
        right_grid_col: usize,
    ) -> Result<MutationOutcome, MutationError> {
        if top_row > bottom_row || left_grid_col > right_grid_col {
            return Err(MutationError::InvalidTableOperation {
                detail: "合併矩形倒置".to_string(),
            });
        }
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let (table_start, _) = locate_table(text, &table)?;
        let rows = row_layouts(text, table_start);
        if bottom_row >= rows.len() {
            return Err(MutationError::InvalidTableOperation {
                detail: format!("合併底列 {bottom_row} 越界"),
            });
        }
        let span_w = right_grid_col - left_grid_col + 1;
        let mut reps = Vec::new();
        let mut top_content = String::new();
        let mut have_top_content = false;

        for (r, row) in rows.iter().enumerate().take(bottom_row + 1).skip(top_row) {
            let selected: Vec<&CellLayout> = row
                .cells
                .iter()
                .filter(|c| {
                    c.col_start < left_grid_col + span_w && left_grid_col < c.col_start + c.span
                })
                .collect();
            if selected.is_empty() {
                return Err(MutationError::InvalidTableOperation {
                    detail: format!("第 {r} 列在合併欄範圍內沒有儲存格"),
                });
            }
            // 嚴格涵蓋：任何與矩形相交的格必須完整落在矩形內。
            for c in &selected {
                if c.col_start < left_grid_col || c.col_start + c.span > left_grid_col + span_w {
                    return Err(MutationError::InvalidTableOperation {
                        detail: format!(
                            "第 {r} 列儲存格跨出合併邊界（格 {}..{}，矩形 {}..{}）",
                            c.col_start,
                            c.col_start + c.span,
                            left_grid_col,
                            left_grid_col + span_w
                        ),
                    });
                }
            }
            let first = selected[0];
            let first_tcpr = match first.tcpr {
                Some((s, e)) => text[s..e].to_string(),
                None => "<w:tcPr></w:tcPr>".to_string(),
            };
            let mut new_tcpr = set_gridspan_in_tcpr(&first_tcpr, span_w);
            if bottom_row > top_row {
                let state = if r == top_row {
                    VMergeState::Restart
                } else {
                    VMergeState::Continue
                };
                new_tcpr = set_vmerge_in_tcpr(&new_tcpr, state);
            } else {
                new_tcpr = set_vmerge_in_tcpr(&new_tcpr, VMergeState::None);
            }
            let first_open = tag_open_string(text, first.start);
            let first_gt = first_open.find('>').unwrap_or(first_open.len() - 1);
            let first_open = first_open[..=first_gt].to_string();
            let first_content_start = first.tcpr.map(|(_, e)| e).unwrap_or(first.start);
            let first_content_end = first.end.saturating_sub("</w:tc>".len());
            let mut new_first = format!(
                "{first_open}{new_tcpr}{}",
                &text[first_content_start..first_content_end]
            );

            if r == top_row {
                for other in selected.iter().skip(1) {
                    let cs = other.tcpr.map(|(_, e)| e).unwrap_or(other.start);
                    let ce = other.end.saturating_sub("</w:tc>".len());
                    top_content.push_str(&text[cs..ce]);
                }
                have_top_content = true;
            } else {
                for other in &selected {
                    let cs = other.tcpr.map(|(_, e)| e).unwrap_or(other.start);
                    let ce = other.end.saturating_sub("</w:tc>".len());
                    top_content.push_str(&text[cs..ce]);
                }
                // 非頂列保留一格空段（vMerge continue 的合法形狀）。
                new_first = format!("{first_open}{new_tcpr}<w:p></w:p>");
            }
            new_first.push_str("</w:tc>");
            reps.push((
                ByteSpan {
                    start: first.start,
                    end: first.end,
                },
                new_first.into_bytes(),
            ));
            for other in selected.iter().skip(1) {
                reps.push((
                    ByteSpan {
                        start: other.start,
                        end: other.end,
                    },
                    Vec::new(),
                ));
            }
        }

        if have_top_content && !top_content.is_empty() {
            // 把被吞併格的內容搬進頂列首格尾端。
            let top_first = rows[top_row]
                .cells
                .iter()
                .find(|c| c.col_start == left_grid_col)
                .ok_or_else(|| MutationError::InvalidTableOperation {
                    detail: "頂列找不到合併起點格".to_string(),
                })?;
            let top_end = top_first.end.saturating_sub("</w:tc>".len());
            for rep in reps.iter_mut() {
                if rep.0.start == top_first.start && rep.0.end == top_first.end {
                    let mut bytes = rep.1.clone();
                    let cut = bytes.len().saturating_sub("</w:tc>".len());
                    let mut next = bytes[..cut].to_vec();
                    next.extend_from_slice(top_content.as_bytes());
                    next.extend_from_slice(b"</w:tc>");
                    bytes = next;
                    rep.1 = bytes;
                    break;
                }
            }
            let _ = top_end;
        }

        let new_xml = apply_replacements(&xml, &reps)?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.commit_k29c(op, commit)
    }

    fn k29c_split_cell(
        &mut self,
        op: Mutation,
        table: TablePath,
        row_idx: usize,
        cell_idx: usize,
        into_rows: usize,
        into_cols: usize,
    ) -> Result<MutationOutcome, MutationError> {
        if into_rows == 0 || into_cols == 0 {
            return Err(MutationError::InvalidTableOperation {
                detail: "分割行列數必須大於 0".to_string(),
            });
        }
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let (table_start, _) = locate_table(text, &table)?;
        let rows = row_layouts(text, table_start);
        let row = rows
            .get(row_idx)
            .ok_or_else(|| MutationError::InvalidTableOperation {
                detail: format!("列序 {row_idx} 越界"),
            })?;
        let cell = row
            .cells
            .get(cell_idx)
            .ok_or_else(|| MutationError::InvalidTableOperation {
                detail: format!("格序 {cell_idx} 越界"),
            })?;
        let mut reps = Vec::new();
        if into_rows > 1 {
            // 垂直解除：把 vMerge 區間內同欄各列還原成獨立格。
            let mut freed = 0usize;
            for (r, row) in rows.iter().enumerate().skip(row_idx) {
                let Some(c) = row.cells.iter().find(|c| c.col_start == cell.col_start) else {
                    break;
                };
                let Some((s, e)) = c.tcpr else { break };
                let tcpr = &text[s..e];
                if r == row_idx {
                    let new = set_vmerge_in_tcpr(tcpr, VMergeState::None);
                    reps.push((ByteSpan { start: s, end: e }, new.into_bytes()));
                    freed += 1;
                    continue;
                }
                if !tcpr.contains("<w:vMerge") {
                    break;
                }
                let new = set_vmerge_in_tcpr(tcpr, VMergeState::None);
                reps.push((ByteSpan { start: s, end: e }, new.into_bytes()));
                freed += 1;
                if freed >= into_rows {
                    break;
                }
            }
        }
        if into_cols > 1 {
            let cell_open = tag_open_string(text, cell.start);
            let gt = cell_open.find('>').unwrap_or(cell_open.len() - 1);
            let open = cell_open[..=gt].to_string();
            let tcpr = match cell.tcpr {
                Some((s, e)) => text[s..e].to_string(),
                None => "<w:tcPr></w:tcPr>".to_string(),
            };
            let mut tcpr_new = set_gridspan_in_tcpr(&tcpr, 1);
            tcpr_new = set_vmerge_in_tcpr(&tcpr_new, VMergeState::None);
            let content_start = cell.tcpr.map(|(_, e)| e).unwrap_or(cell.start);
            let content_end = cell.end.saturating_sub("</w:tc>".len());
            let content = &text[content_start..content_end];
            let mut split = String::new();
            split.push_str(&format!("{open}{tcpr_new}{content}</w:tc>"));
            for _ in 1..into_cols {
                split.push_str(&format!("{open}{tcpr_new}<w:p></w:p></w:tc>"));
            }
            reps.push((
                ByteSpan {
                    start: cell.start,
                    end: cell.end,
                },
                split.into_bytes(),
            ));
        } else if into_rows == 1 {
            // 純水平／完全未合併：清掉 gridSpan／vMerge。
            let mut reps2 = Vec::new();
            if let Some((s, e)) = cell.tcpr {
                let tcpr = &text[s..e];
                let mut new = set_gridspan_in_tcpr(tcpr, 1);
                new = set_vmerge_in_tcpr(&new, VMergeState::None);
                reps2.push((ByteSpan { start: s, end: e }, new.into_bytes()));
            }
            reps.extend(reps2);
        }

        let new_xml = apply_replacements(&xml, &reps)?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.commit_k29c(op, commit)
    }

    // ------------------------- M4.6 屬性 -------------------------

    fn k29c_set_table_props(
        &mut self,
        op: Mutation,
        table: TablePath,
        patch: TablePropsPatch,
    ) -> Result<MutationOutcome, MutationError> {
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let (table_start, _) = locate_table(text, &table)?;
        let add = table_props_xml(&patch);
        let reps = vec![match first_child(text, table_start, "tblPr") {
            Some((s, e)) => {
                let old = &text[s..e];
                let gt = old.find('>').unwrap_or(0);
                let mut body = old[gt + 1..old.len() - "</w:tblPr>".len()].to_string();
                if patch.width_pt.is_some() {
                    body = remove_named_element(&body, "w:tblW");
                }
                if patch.alignment.is_some() {
                    body = remove_named_element(&body, "w:jc");
                }
                if patch.shading_fill.is_some() {
                    body = remove_named_element(&body, "w:shd");
                }
                (
                    ByteSpan { start: s, end: e },
                    format!("<w:tblPr>{body}{add}</w:tblPr>").into_bytes(),
                )
            }
            None => {
                let open_end = tag_at(text, table_start)
                    .map(|t| t.end)
                    .unwrap_or(table_start);
                (
                    ByteSpan {
                        start: open_end,
                        end: open_end,
                    },
                    format!("<w:tblPr>{add}</w:tblPr>").into_bytes(),
                )
            }
        }];
        let new_xml = apply_replacements(&xml, &reps)?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.commit_k29c(op, commit)
    }

    fn k29c_set_row_props(
        &mut self,
        op: Mutation,
        table: TablePath,
        row_idx: usize,
        patch: crate::mutation::RowPropsPatch,
    ) -> Result<MutationOutcome, MutationError> {
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let (table_start, _) = locate_table(text, &table)?;
        let rows = direct_children(text, table_start, "tr");
        let row = *rows
            .get(row_idx)
            .ok_or_else(|| MutationError::InvalidTableOperation {
                detail: format!("列序 {row_idx} 越界"),
            })?;
        let add = row_props_xml(&patch);
        let reps = vec![match first_child(text, row.0, "trPr") {
            Some((s, e)) => {
                let old = &text[s..e];
                let gt = old.find('>').unwrap_or(0);
                let mut body = old[gt + 1..old.len() - "</w:trPr>".len()].to_string();
                if patch.height_pt.is_some() {
                    body = remove_named_element(&body, "w:trHeight");
                }
                if patch.header.is_some() {
                    body = remove_named_element(&body, "w:tblHeader");
                }
                if patch.cant_split.is_some() {
                    body = remove_named_element(&body, "w:cantSplit");
                }
                (
                    ByteSpan { start: s, end: e },
                    format!("<w:trPr>{body}{add}</w:trPr>").into_bytes(),
                )
            }
            None => {
                let open_end = tag_at(text, row.0).map(|t| t.end).unwrap_or(row.0);
                (
                    ByteSpan {
                        start: open_end,
                        end: open_end,
                    },
                    format!("<w:trPr>{add}</w:trPr>").into_bytes(),
                )
            }
        }];
        let new_xml = apply_replacements(&xml, &reps)?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.commit_k29c(op, commit)
    }

    fn k29c_set_cell_props(
        &mut self,
        op: Mutation,
        table: TablePath,
        row_idx: usize,
        cell_idx: usize,
        patch: crate::mutation::CellPropsPatch,
    ) -> Result<MutationOutcome, MutationError> {
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let (table_start, _) = locate_table(text, &table)?;
        let rows = row_layouts(text, table_start);
        let row = rows
            .get(row_idx)
            .ok_or_else(|| MutationError::InvalidTableOperation {
                detail: format!("列序 {row_idx} 越界"),
            })?;
        let cell = row
            .cells
            .get(cell_idx)
            .ok_or_else(|| MutationError::InvalidTableOperation {
                detail: format!("格序 {cell_idx} 越界"),
            })?;
        let rep = replace_tcpr_reps(text, cell, &patch)?;
        let new_xml = apply_replacements(&xml, &[rep])?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.commit_k29c(op, commit)
    }

    // ------------------------- M5：媒體 -------------------------

    fn plan_media(
        &self,
        base_part: &str,
        media_bytes: &[u8],
        mime: &str,
    ) -> Result<MediaPlan, MutationError> {
        let ext = validate_media(media_bytes, mime)?;
        let mut existing_part = None;
        for (name, bytes) in &self.doc.parts {
            if name.starts_with("word/media/") && bytes == media_bytes {
                existing_part = Some(name.clone());
                break;
            }
        }
        let (media_part, created) = match existing_part {
            Some(p) => (p, false),
            None => {
                let mut n = 1usize;
                let part = loop {
                    let candidate = format!("word/media/image{n}.{ext}");
                    if !self.doc.parts.contains_key(&candidate) {
                        break candidate;
                    }
                    n += 1;
                };
                (part, true)
            }
        };
        let rels_part = rels_part_for(base_part);
        let rels_old = self.doc.parts.get(&rels_part).cloned().unwrap_or_default();
        let rels_text = String::from_utf8_lossy(&rels_old).to_string();
        let mut existing_rid = None;
        let mut cursor = 0usize;
        while let Some(t) = next_tag(&rels_text, cursor) {
            if !t.closing && t.local == "Relationship" {
                let tag = tag_open_string(&rels_text, t.start);
                if attr_in_tag(&tag, "Type").as_deref() == Some(IMAGE_REL_TYPE) {
                    if let Some(target) = attr_in_tag(&tag, "Target") {
                        let resolved = relationship_target(base_part, &target);
                        if resolved == media_part {
                            existing_rid = attr_in_tag(&tag, "Id");
                            break;
                        }
                    }
                }
            }
            cursor = t.end;
        }
        let (rid, rels_new) = match existing_rid {
            Some(id) => (id, None),
            None => {
                let mut rid = next_rid(&rels_text);
                while rels_text.contains(&format!(r#"Id="{rid}""#)) {
                    let n = rid
                        .strip_prefix("rId")
                        .and_then(|v| v.parse::<u32>().ok())
                        .unwrap_or(0);
                    rid = format!("rId{}", n + 1);
                }
                let target = if let Some((dir, _)) = base_part.rsplit_once('/') {
                    let _ = dir;
                    media_part
                        .strip_prefix(&format!("{}/", dir))
                        .unwrap_or(&media_part)
                        .to_string()
                } else {
                    media_part.clone()
                };
                let rel = format!(
                    r#"<Relationship Id="{rid}" Type="{IMAGE_REL_TYPE}" Target="{}"/>"#,
                    xml_attr_escape(&target)
                );
                let new_rels = if rels_old.is_empty() {
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rel}</Relationships>"#
                    )
                    .into_bytes()
                } else {
                    let pos = rels_text.rfind("</Relationships>").ok_or_else(|| {
                        MutationError::XmlPatchRejected {
                            detail: format!("{rels_part} 缺少 </Relationships>"),
                        }
                    })?;
                    let mut out = rels_text[..pos].to_string();
                    out.push_str(&rel);
                    out.push_str(&rels_text[pos..]);
                    out.into_bytes()
                };
                (rid, Some(new_rels))
            }
        };

        let content_types_new = if created {
            let ct_part = "[Content_Types].xml";
            if let Some(ct) = self.doc.parts.get(ct_part) {
                let ct_text = String::from_utf8_lossy(ct).to_string();
                let needle = format!(r#"Extension="{ext}""#);
                if !ct_text.contains(&needle) {
                    let pos = ct_text.rfind("</Types>").ok_or_else(|| {
                        MutationError::XmlPatchRejected {
                            detail: "[Content_Types].xml 缺少 </Types>".to_string(),
                        }
                    })?;
                    let add = format!(
                        r#"<Default Extension="{ext}" ContentType="{}"/>"#,
                        content_type_for_ext(ext)
                    );
                    let mut out = ct_text[..pos].to_string();
                    out.push_str(&add);
                    out.push_str(&ct_text[pos..]);
                    Some(out.into_bytes())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let rels_created = !self.doc.parts.contains_key(&rels_part);
        Ok(MediaPlan {
            media_part,
            created,
            rels_part,
            rels_new,
            rels_created,
            rid,
            content_types_new,
        })
    }

    fn apply_media_plan(&self, plan: &MediaPlan, media_bytes: &[u8], commit: &mut K29cCommit) {
        if plan.created {
            commit.add(plan.media_part.clone(), media_bytes.to_vec());
        }
        if let Some(rels) = &plan.rels_new {
            if plan.rels_created {
                // 基線沒有 `.rels`：以新增 part 記帳，Undo 才能整份移除。
                commit.add(plan.rels_part.clone(), rels.clone());
            } else {
                commit.update(plan.rels_part.clone(), rels.clone());
            }
        }
        if let Some(ct) = &plan.content_types_new {
            commit.update("[Content_Types].xml".to_string(), ct.clone());
        }
    }

    #[allow(clippy::too_many_arguments)] // 行內／浮動共用入口：定位、媒體與錨點參數皆來自 Mutation 原欄位
    fn k29c_insert_image(
        &mut self,
        op: Mutation,
        target: SubDocTarget,
        pos: Option<DocPos>,
        host_para: Option<usize>,
        media_bytes: Vec<u8>,
        mime_type: String,
        width_pt: f32,
        height_pt: f32,
        alt_text: Option<String>,
        floating: Option<(FloatingAxisSpec, FloatingAxisSpec, WrapModeSpec, bool)>,
    ) -> Result<MutationOutcome, MutationError> {
        if !width_pt.is_finite() || width_pt <= 0.0 || !height_pt.is_finite() || height_pt <= 0.0 {
            return Err(MutationError::InvalidMediaPayload {
                detail: "圖片寬高必須為有限正數".to_string(),
            });
        }
        let part = part_for_target(&target)?;
        if part != "word/document.xml" {
            return Err(MutationError::SubDocNotFound { target });
        }
        let plan = self.plan_media(part, &media_bytes, &mime_type)?;
        let (_, xml) = self.body_part()?;
        let text = text_of(&xml)?;
        let para = match (pos, host_para) {
            (Some(p), _) => p.para,
            (None, Some(h)) => h,
            _ => 0,
        };
        let (p_span, _) = crate::mutation::find_paragraph_span(&xml, para)?;
        let analysis = crate::mutation::analyze_paragraph(&xml, p_span);
        let insert_at = if let Some(pos) = pos {
            if pos.cp == 0 {
                analysis
                    .ppr_end_offset
                    .unwrap_or(analysis.p_open_end_offset)
            } else {
                let node = analysis
                    .text_nodes
                    .iter()
                    .find(|n| n.start_cp < pos.cp && pos.cp <= n.end_cp);
                node.and_then(|n| n.run_span)
                    .and_then(|r| run_end_at(text, r.start))
                    .unwrap_or(analysis.p_close_start_offset)
            }
        } else {
            analysis.p_close_start_offset
        };
        let doc_pr_id = max_doc_pr_id(text) + 1;
        let name = plan
            .media_part
            .rsplit('/')
            .next()
            .unwrap_or("image.png")
            .to_string();
        let run = match floating {
            Some((pos_h, pos_v, wrap, behind)) => floating_drawing_xml(
                &plan.rid, &name, doc_pr_id, width_pt, height_pt, &pos_h, &pos_v, wrap, behind,
            ),
            None => drawing_xml(
                &plan.rid,
                &name,
                doc_pr_id,
                width_pt,
                height_pt,
                alt_text.as_deref(),
            ),
        };
        let new_xml = apply_replacements(
            &xml,
            &[(
                ByteSpan {
                    start: insert_at,
                    end: insert_at,
                },
                run.into_bytes(),
            )],
        )?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.apply_media_plan(&plan, &media_bytes, &mut commit);
        commit.caret = Some(DocPos {
            para,
            cp: analysis.total_cp,
        });
        self.commit_k29c(op, commit)
    }

    fn locate_drawing(
        &self,
        xml: &[u8],
        host_para: usize,
        drawing_index: usize,
    ) -> Result<((usize, usize), String, String), MutationError> {
        let (p_span, _) = crate::mutation::find_paragraph_span(xml, host_para)?;
        let text = text_of(xml)?;
        let drawings = drawings_in_span(text, (p_span.start, p_span.end));
        let span =
            *drawings
                .get(drawing_index)
                .ok_or_else(|| MutationError::InvalidMediaPayload {
                    detail: format!(
                        "段落 {host_para} 的第 {drawing_index} 張圖不存在（共 {} 張）",
                        drawings.len()
                    ),
                })?;
        let rid =
            drawing_embed_rid(text, span).ok_or_else(|| MutationError::InvalidMediaPayload {
                detail: "目標圖形沒有 r:embed／r:id 參照".to_string(),
            })?;
        Ok((span, rid, "word/document.xml".to_string()))
    }

    fn media_part_for_rid(&self, base_part: &str, rid: &str) -> Option<String> {
        let rels_part = rels_part_for(base_part);
        let rels = self.doc.parts.get(&rels_part)?;
        let rels = quire_ooxml::read_rels(rels);
        quire_ooxml::resolve_relationship(&rels, rid)
    }

    fn count_rid_refs(&self, rid: &str) -> usize {
        let needle = format!(r#"r:embed="{rid}""#);
        let needle2 = format!(r#"r:id="{rid}""#);
        self.doc
            .parts
            .iter()
            .filter(|(name, _)| name.starts_with("word/") && name.ends_with(".xml"))
            .map(|(_, bytes)| {
                let s = String::from_utf8_lossy(bytes);
                s.matches(&needle).count() + s.matches(&needle2).count()
            })
            .sum()
    }

    fn remove_rel_from_rels(&self, base_part: &str, rid: &str) -> Option<Vec<u8>> {
        let rels_part = rels_part_for(base_part);
        let bytes = self.doc.parts.get(&rels_part)?;
        let text = String::from_utf8_lossy(bytes).to_string();
        let mut cursor = 0usize;
        while let Some(t) = next_tag(&text, cursor) {
            if !t.closing && t.local == "Relationship" {
                let tag = tag_open_string(&text, t.start);
                if attr_in_tag(&tag, "Id").as_deref() == Some(rid) {
                    let end = element_end(&text, t.start).unwrap_or(t.end);
                    let mut out = text[..t.start].to_string();
                    out.push_str(&text[end..]);
                    return Some(out.into_bytes());
                }
            }
            cursor = t.end;
        }
        None
    }

    #[allow(clippy::too_many_arguments)] // 尺寸與兩軸定位／繞圖各自可獨立缺省，逐欄傳遞比打包結構更直白
    fn k29c_resize_image(
        &mut self,
        op: Mutation,
        target: SubDocTarget,
        host_para: usize,
        drawing_index: usize,
        new_width_pt: Option<f32>,
        new_height_pt: Option<f32>,
        new_pos_h: Option<FloatingAxisSpec>,
        new_pos_v: Option<FloatingAxisSpec>,
        new_wrap_mode: Option<WrapModeSpec>,
    ) -> Result<MutationOutcome, MutationError> {
        let part = part_for_target(&target)?;
        if part != "word/document.xml" {
            return Err(MutationError::SubDocNotFound { target });
        }
        for v in [new_width_pt, new_height_pt].into_iter().flatten() {
            if !v.is_finite() || v <= 0.0 {
                return Err(MutationError::InvalidMediaPayload {
                    detail: "圖片新寬高必須為有限正數".to_string(),
                });
            }
        }
        let (_, xml) = self.body_part()?;
        let ((ds, de), _rid, _) = self.locate_drawing(&xml, host_para, drawing_index)?;
        let text = text_of(&xml)?;
        let mut reps = Vec::new();
        let mut cursor = ds;
        while cursor < de {
            let Some(t) = next_tag(text, cursor) else {
                break;
            };
            if !t.closing && (t.local == "extent" || t.local == "ext") {
                let old = tag_open_string(text, t.start);
                let mut next = old.clone();
                if let Some(w) = new_width_pt {
                    next = set_attr_in_tag(
                        &next,
                        "cx",
                        &((w * EMU_PER_PT).round() as i64).to_string(),
                    );
                }
                if let Some(h) = new_height_pt {
                    next = set_attr_in_tag(
                        &next,
                        "cy",
                        &((h * EMU_PER_PT).round() as i64).to_string(),
                    );
                }
                reps.push((
                    ByteSpan {
                        start: t.start,
                        end: t.end,
                    },
                    next.into_bytes(),
                ));
            }
            cursor = t.end;
        }
        if let Some(spec) = &new_pos_h {
            reps.extend(position_reps(text, (ds, de), spec, true)?);
        }
        if let Some(spec) = &new_pos_v {
            reps.extend(position_reps(text, (ds, de), spec, false)?);
        }
        if let Some(wrap) = new_wrap_mode {
            reps.extend(wrap_reps(text, (ds, de), wrap)?);
        }
        let new_xml = apply_replacements(&xml, &reps)?;
        let mut commit = K29cCommit::new();
        commit.update("word/document.xml", new_xml);
        self.commit_k29c(op, commit)
    }

    fn k29c_replace_image(
        &mut self,
        op: Mutation,
        target: SubDocTarget,
        host_para: usize,
        drawing_index: usize,
        new_media_bytes: Vec<u8>,
        new_mime_type: String,
    ) -> Result<MutationOutcome, MutationError> {
        let part = part_for_target(&target)?;
        if part != "word/document.xml" {
            return Err(MutationError::SubDocNotFound { target });
        }
        let (_, xml) = self.body_part()?;
        let ((ds, de), old_rid, _) = self.locate_drawing(&xml, host_para, drawing_index)?;
        let text = text_of(&xml)?;
        let plan = self.plan_media(part, &new_media_bytes, &new_mime_type)?;
        let mut new_text = replace_embed_xml(text, (ds, de), &old_rid, &plan.rid);
        let mut commit = K29cCommit::new();
        let mut irreversible = None;

        // 既有圖若為本會話新增且已無任何引用，可依呼叫端要求強制回收。
        if let Some(old_part) = self.media_part_for_rid(part, &old_rid) {
            if self.media_part_evictable(&old_part) && self.count_rid_refs(&old_rid) <= 1 {
                let bytes_freed = self.doc.parts.get(&old_part).map(|b| b.len()).unwrap_or(0);
                new_text = new_text.replace(
                    &format!(r#"r:embed="{old_rid}""#),
                    &format!(r#"r:embed="{}""#, plan.rid),
                );
                commit.remove(old_part.clone());
                if let Some(rels) = self.remove_rel_from_rels(part, &old_rid) {
                    commit.update(rels_part_for(part), rels);
                }
                irreversible = Some(ReversibilityLevel::Irreversible {
                    reason: IrreversibleReason::MediaPayloadEvicted {
                        part_name: old_part,
                        bytes_freed,
                    },
                });
            }
        }

        commit.update(part.to_string(), new_text.into_bytes());
        self.apply_media_plan(&plan, &new_media_bytes, &mut commit);
        commit.irreversibility = irreversible;
        self.commit_k29c(op, commit)
    }

    fn k29c_delete_image(
        &mut self,
        op: Mutation,
        target: SubDocTarget,
        host_para: usize,
        drawing_index: usize,
        gc_unreferenced_media: bool,
    ) -> Result<MutationOutcome, MutationError> {
        let part = part_for_target(&target)?;
        if part != "word/document.xml" {
            return Err(MutationError::SubDocNotFound { target });
        }
        let (_, xml) = self.body_part()?;
        let ((ds, de), rid, _) = self.locate_drawing(&xml, host_para, drawing_index)?;
        let new_xml = apply_replacements(&xml, &[(ByteSpan { start: ds, end: de }, Vec::new())])?;
        let mut commit = K29cCommit::new();
        let mut irreversible = None;
        if gc_unreferenced_media {
            if let Some(media_part) = self.media_part_for_rid(part, &rid) {
                if self.media_part_evictable(&media_part) && self.count_rid_refs(&rid) <= 1 {
                    let bytes_freed = self
                        .doc
                        .parts
                        .get(&media_part)
                        .map(|b| b.len())
                        .unwrap_or(0);
                    commit.remove(media_part.clone());
                    if let Some(rels) = self.remove_rel_from_rels(part, &rid) {
                        commit.update(rels_part_for(part), rels);
                    }
                    irreversible = Some(ReversibilityLevel::Irreversible {
                        reason: IrreversibleReason::MediaPayloadEvicted {
                            part_name: media_part,
                            bytes_freed,
                        },
                    });
                }
            }
        }
        commit.update(part.to_string(), new_xml);
        commit.irreversibility = irreversible;
        self.commit_k29c(op, commit)
    }

    fn media_part_evictable(&self, part: &str) -> bool {
        !self.part_baselines.contains_key(part)
    }
}

fn run_end_at(text: &str, start: usize) -> Option<usize> {
    let first = tag_at(text, start)?;
    if first.local != "r" {
        return None;
    }
    element_end(text, start)
}

fn position_reps(
    text: &str,
    drawing: (usize, usize),
    spec: &FloatingAxisSpec,
    is_h: bool,
) -> Result<Vec<(ByteSpan, Vec<u8>)>, MutationError> {
    let local = if is_h { "positionH" } else { "positionV" };
    let mut cursor = drawing.0;
    while cursor < drawing.1 {
        let Some(t) = next_tag(text, cursor) else {
            break;
        };
        if !t.closing && t.local == local {
            let end = element_end(text, t.start).unwrap_or(t.end);
            let span = (t.start, end);
            let old = &text[span.0..span.1];
            let gt = old.find('>').unwrap_or(0);
            let open = old[..=gt].to_string();
            let open = set_attr_in_tag(&open, "relativeFrom", &spec.relative_from);
            let close = format!("</wp:{local}>");
            let body_start = gt + 1;
            let body_end = old.len().saturating_sub(close.len());
            let mut body = old[body_start..body_end].to_string();
            if spec.align.is_some() {
                body = remove_named_element(&body, "wp:posOffset");
            } else {
                body = remove_named_element(&body, "wp:align");
            }
            let add = if let Some(align) = &spec.align {
                format!("<wp:align>{}</wp:align>", xml_attr_escape(align))
            } else {
                let off = (spec.offset_pt.unwrap_or(0.0) * EMU_PER_PT).round() as i64;
                format!("<wp:posOffset>{off}</wp:posOffset>")
            };
            let repl = format!("{open}{body}{add}{close}");
            return Ok(vec![(
                ByteSpan {
                    start: span.0,
                    end: span.1,
                },
                repl.into_bytes(),
            )]);
        }
        cursor = t.end;
    }
    Err(MutationError::XmlPatchRejected {
        detail: format!("浮動圖缺少 <wp:{local}> 定位節點"),
    })
}

fn wrap_reps(
    text: &str,
    drawing: (usize, usize),
    mode: WrapModeSpec,
) -> Result<Vec<(ByteSpan, Vec<u8>)>, MutationError> {
    let anchor = {
        let mut cursor = drawing.0;
        let mut found = None;
        while cursor < drawing.1 {
            let Some(t) = next_tag(text, cursor) else {
                break;
            };
            if !t.closing && t.local == "anchor" {
                found = Some((t.start, element_end(text, t.start).unwrap_or(t.end)));
                break;
            }
            cursor = t.end;
        }
        found
    };
    let Some(anchor) = anchor else {
        return Err(MutationError::XmlPatchRejected {
            detail: "只有浮動圖（wp:anchor）可設定文繞圖".to_string(),
        });
    };
    let mut cursor = anchor.0;
    while cursor < anchor.1 {
        let Some(t) = next_tag(text, cursor) else {
            break;
        };
        if !t.closing && t.local.starts_with("wrap") {
            let end = element_end(text, t.start).unwrap_or(t.end);
            return Ok(vec![(
                ByteSpan {
                    start: t.start,
                    end,
                },
                wrap_xml(mode).as_bytes().to_vec(),
            )]);
        }
        cursor = t.end;
    }
    let open_end =
        tag_at(text, anchor.0)
            .map(|t| t.end)
            .ok_or_else(|| MutationError::XmlPatchRejected {
                detail: "wp:anchor 開標籤解析失敗".to_string(),
            })?;
    Ok(vec![(
        ByteSpan {
            start: open_end,
            end: open_end,
        },
        wrap_xml(mode).as_bytes().to_vec(),
    )])
}

#[allow(dead_code)]
fn _silence_unused(_: BTreeMap<String, ()>) {}
