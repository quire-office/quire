//! canonical 表格樹 → layout 層 `TableInput`／`TableSlot`（Phase3-1b 映射層）。
//!
//! 分層：canonical 型別定義在 quire-ooxml、幾何演算法在 quire-layout，本模組
//! 只在 engine 層把前者餵成後者的輸入契約（與 sectPr 同款三段式）。換算
//! （twip→pt、pct→%）在此完成，layout 只吃 pt／百分比。
//!
//! 樣式框線：`w:tblStyle` 查 `word/styles.xml` 的 `w:style[@w:type='table']`
//! → `w:tblPr/w:tblBorders`，並沿 `w:basedOn` 鏈向上繼承（鏈環／斷鏈停住，
//! 容錯不 panic）。這是 v1 的最小查表——`tblStylePr` 條件底紋/條件框線
//! （firstRow/band1Horz 等）不實作，與 layout 層头注的取捨一致。

use std::collections::HashMap;

use quire_layout::table::{
    CellBlock, CellInput, CellMarginInput, EdgeSetInput, PenInput, RowInput, TableInput, WidthSpec,
};
use quire_layout::{LayoutParams, SectionSpec, TableSlot, TextMeasurer};
use quire_ooxml::{Block, BodyTable, Borders, Margins, Table, TblWidth};

/// twip → pt。非法／缺＝None（容錯鐵律：不猜、不虚構）。
fn twip_pt(raw: Option<&str>) -> Option<f32> {
    raw.and_then(|s| s.trim().parse::<f32>().ok())
        .map(|v| v / 20.0)
}

/// `w:tblW`/`w:tcW` → `WidthSpec`。dxa→Pt；pct→Pct（pct 單位是 1/50%，
/// ECMA-376 §17.18.90）；auto/nil/缺＝None（由 layout 退回自然寬）。
fn width_spec(tw: &TblWidth) -> Option<WidthSpec> {
    let num = tw.num()? as f32;
    match tw.typ.as_deref() {
        Some("dxa") => Some(WidthSpec::Pt((num / 20.0).max(0.0))),
        Some("pct") => Some(WidthSpec::Pct((num / 50.0).max(0.0))),
        _ => None,
    }
}

/// 邊距單側：dxa→pt；pct/非法＝None（退回上層預設——v1 不做 pct 邊距）。
fn side_pt(tw: Option<&TblWidth>) -> Option<f32> {
    let tw = tw?;
    if matches!(tw.typ.as_deref(), Some("dxa") | None) {
        twip_pt(tw.w.as_deref())
    } else {
        None
    }
}

fn pen(b: Option<&quire_ooxml::Border>) -> Option<PenInput> {
    b.map(|b| PenInput {
        val: b.val.clone(),
        sz: b.sz.clone(),
        // color 原字串直送：auto/theme 名→黑、RRGGBB→RGB 的判定在 layout。
        color: b.color.clone(),
    })
}

fn edge_set(bs: &Borders) -> EdgeSetInput {
    EdgeSetInput {
        top: pen(bs.top.as_ref()),
        left: pen(bs.left.as_ref()),
        bottom: pen(bs.bottom.as_ref()),
        right: pen(bs.right.as_ref()),
        inside_h: pen(bs.inside_h.as_ref()),
        inside_v: pen(bs.inside_v.as_ref()),
    }
}

/// `w:tblCellMar`／`w:tcMar` → 四側 pt：新式 start/end 優先，舊式 left/right 後備。
fn cell_margins(m: &Margins) -> CellMarginInput {
    CellMarginInput {
        top: side_pt(m.top.as_ref()),
        left: side_pt(m.start.as_ref()).or_else(|| side_pt(m.left.as_ref())),
        bottom: side_pt(m.bottom.as_ref()),
        right: side_pt(m.end.as_ref()).or_else(|| side_pt(m.right.as_ref())),
    }
}

fn has_any_edges(e: &EdgeSetInput) -> bool {
    [
        e.top.as_ref(),
        e.left.as_ref(),
        e.bottom.as_ref(),
        e.right.as_ref(),
        e.inside_h.as_ref(),
        e.inside_v.as_ref(),
    ]
    .into_iter()
    .any(|p| p.is_some_and(|p| p.val.is_some()))
}

/// `word/styles.xml` → styleId → 表格框線（沿 basedOn 鏈合成後的結果）。
///
/// 實作：先收錄每張樣式「自帶」的 tblBorders，再逐樣式沿 basedOn 向上補缺位
/// （缺位＝該方位未宣告，非宣告 none）。鏈長上限 16＋拜訪集防環。
pub fn table_style_borders(styles_xml: &[u8]) -> HashMap<String, EdgeSetInput> {
    let xml = String::from_utf8_lossy(styles_xml);
    // ---- 第一趟：逐 <w:style …>…</w:style> 記錄自帶框線與 basedOn ----
    let mut own: HashMap<String, (Option<EdgeSetInput>, Option<String>)> = HashMap::new();
    let mut cursor = 0usize;
    while let Some(rel) = xml[cursor..].find("<w:style ") {
        let start = cursor + rel;
        let end = match xml[start..].find("</w:style>") {
            Some(e) => start + e + "</w:style>".len(),
            None => xml.len(),
        };
        cursor = end;
        let seg = &xml[start..end];
        if !seg.contains("w:type=\"table\"") {
            continue;
        }
        let Some(id) = attr(seg, "w:styleId") else {
            continue;
        };
        let based_on = {
            let re_at = seg.find("<w:basedOn ");
            re_at.and_then(|i| attr(&seg[i..], "w:val"))
        };
        let borders = find_tbl_borders(seg).map(|bs| edge_set(&bs));
        own.insert(id, (borders, based_on));
    }
    // ---- 第二趟：沿 basedOn 補缺位 ----
    let mut out: HashMap<String, EdgeSetInput> = HashMap::new();
    for id in own.keys() {
        let mut acc: EdgeSetInput = Default::default();
        let mut seen: Vec<&String> = Vec::new();
        let mut cur: Option<&String> = Some(id);
        for _ in 0..16 {
            let Some(key) = cur else { break };
            let Some((borders, based_on)) = own.get(key) else {
                break;
            };
            if seen.contains(&key) {
                break; // 鏈環：老實停在已合成結果
            }
            seen.push(key);
            if let Some(b) = borders {
                fill_missing(&mut acc, b);
            }
            cur = based_on
                .as_ref()
                .and_then(|b| own.get_key_value(b))
                .map(|(k, _)| k);
        }
        if has_any_edges(&acc) {
            out.insert(id.clone(), acc);
        }
    }
    out
}

fn fill_missing(dst: &mut EdgeSetInput, src: &EdgeSetInput) {
    let e = [
        (&mut dst.top, &src.top),
        (&mut dst.left, &src.left),
        (&mut dst.bottom, &src.bottom),
        (&mut dst.right, &src.right),
        (&mut dst.inside_h, &src.inside_h),
        (&mut dst.inside_v, &src.inside_v),
    ];
    for (d, s) in e {
        if d.is_none() {
            *d = s.clone();
        }
    }
}

fn attr(seg: &str, name: &str) -> Option<String> {
    let pat = format!("{name}=\"");
    let i = seg.find(&pat)? + pat.len();
    let j = seg[i..].find('"')?;
    let v = &seg[i..i + j];
    (!v.is_empty()).then(|| v.to_string())
}

/// 在 style 段落內找 `w:tblBorders`（簡易字首掃描：tblBorders 只出現在
/// tblPr 內，一段 style 一份，取首份即可）。
fn find_tbl_borders(seg: &str) -> Option<Borders> {
    let i = seg.find("<w:tblBorders")?;
    let j = seg[i..]
        .find("</w:tblBorders>")
        .map(|j| i + j)
        .unwrap_or(seg.len());
    let region = &seg[i..j];
    let pick = |tag: &str| -> Option<quire_ooxml::Border> {
        let pat = format!("<w:{tag} ");
        let k = region.find(&pat)?;
        let e = region[k..]
            .find("/>")
            .map(|e| k + e)
            .unwrap_or(region.len());
        let one = &region[k..e];
        let b = quire_ooxml::Border {
            val: attr(one, "w:val"),
            sz: attr(one, "w:sz"),
            space: attr(one, "w:space"),
            color: attr(one, "w:color"),
            theme_color: attr(one, "w:themeColor"),
            theme_shade: attr(one, "w:themeShade"),
            theme_tint: attr(one, "w:themeTint"),
        };
        (!b.is_empty()).then_some(b)
    };
    let bs = Borders {
        top: pick("top"),
        left: pick("left"),
        bottom: pick("bottom"),
        right: pick("right"),
        inside_h: pick("insideH"),
        inside_v: pick("insideV"),
        tl2br: None,
        tr2bl: None,
    };
    (!bs.is_empty()).then_some(bs)
}

/// canonical 表 → layout 輸入（遞迴巢狀表；樣式框線查 `styles`）。
pub fn table_input(
    t: &Table,
    texts: &[String],
    styles: &HashMap<String, EdgeSetInput>,
) -> TableInput {
    let p = &t.props;
    TableInput {
        grid_pt: t.grid.iter().map(|g| g.w_pt.unwrap_or(0.0)).collect(),
        width: p.width.as_ref().and_then(width_spec),
        fixed_layout: p.layout_fixed == Some(true),
        indent_pt: match p.indent_typ.as_deref() {
            Some("pct") => None, // v1：pct 縮排不換算（退回 0，不虚構）
            _ => twip_pt(p.indent_raw.as_deref()),
        },
        justify: p.justify.clone(),
        borders: p.borders.as_ref().map(edge_set),
        style_borders: p
            .style
            .as_deref()
            .and_then(|s| styles.get(s))
            .cloned()
            .filter(has_any_edges),
        cell_margins: p.cell_margins.as_ref().map(cell_margins),
        rows: t
            .rows
            .iter()
            .map(|r| RowInput {
                cant_split: r.props.cant_split,
                height_pt: twip_pt(r.props.height_raw.as_deref()),
                height_exact: r.props.height_rule.as_deref() == Some("exact"),
                header: r.props.header,
                cells: r
                    .cells
                    .iter()
                    .map(|c| {
                        // 格內 block **保持文件序**交錯映射（根因 1）：段落與
                        // 子表各自入獨立 Vec 會丟失拓撲序，子表後的尾部段落
                        // 會被拉回頂部——流式堆疊的前提就在這裡。
                        let mut blocks = Vec::new();
                        for b in &c.blocks {
                            match b {
                                // 段落索引引用扁平序列：越界＝畸形檔，跳過（容錯）。
                                Block::Paragraph { index } => {
                                    if let Some(s) = texts.get(*index) {
                                        blocks.push(CellBlock::Text(s.clone()));
                                    }
                                }
                                Block::Table(inner) => {
                                    blocks.push(CellBlock::Table(Box::new(table_input(
                                        inner, texts, styles,
                                    ))));
                                }
                            }
                        }
                        CellInput {
                            width: c.props.width.as_ref().and_then(width_spec),
                            grid_span: c.props.grid_span(),
                            vmerge: c.props.vmerge_restart(),
                            blocks,
                            borders: c.props.borders.as_ref().map(edge_set),
                            fill: c
                                .props
                                .shading
                                .as_ref()
                                .and_then(|s| s.fill.clone())
                                .filter(|f| !f.is_empty() && f != "auto"),
                            valign: c.props.valign.clone(),
                            margins: c.props.margins.as_ref().map(cell_margins),
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// 表格（含全部巢狀表）覆蓋的段落上界（不含）＝最大段索引＋1；全無段落＝para_start。
fn table_para_end(t: &Table, para_start: usize) -> usize {
    let mut end = para_start;
    t.walk(&mut |sub| {
        for row in &sub.rows {
            for cell in &row.cells {
                for b in &cell.blocks {
                    if let Block::Paragraph { index } = b {
                        end = end.max(index + 1);
                    }
                }
            }
        }
    });
    end
}

/// 逐頁 section 的可用寬（首 section 缺件時退回 params 基準值）。
fn avail_w_for(sections: &[SectionSpec], params: &LayoutParams, para: usize) -> f32 {
    match sections.get(para) {
        Some(s) => (s.geo.page_w_pt - s.geo.margin_left_pt - s.geo.margin_right_pt).max(1.0),
        None => params.content_w_pt(),
    }
}

/// 由 canonical 樹＋扁平文字建出排版完成的表格槽序列（engine 開啟文件時呼叫）。
/// 回傳依 para_start 排序（parser 保證 body 序單調），`build_flow` 契約直接成立。
pub fn build_slots(
    tables: &[BodyTable],
    texts: &[String],
    styles_xml: Option<&[u8]>,
    params: &LayoutParams,
    sections: &[SectionSpec],
) -> Vec<TableSlot> {
    let styles = styles_xml.map(table_style_borders).unwrap_or_default();
    let m = TextMeasurer::new(params.font_size_pt);
    let line_h = params.line_h_pt();
    let mut prev_end = 0usize; // 逐槽正規化游標：槽區間必不重疊且 ≤ n_paras
    let n_paras = texts.len();
    tables
        .iter()
        .map(|bt| {
            let input = table_input(&bt.table, texts, &styles);
            let avail_w = avail_w_for(sections, params, bt.para_start);
            let layout = quire_layout::table::layout_table(&input, avail_w, line_h, &m, 0);
            let raw_end = table_para_end(&bt.table, bt.para_start);
            // 索引空間對齊（t_580c37ac 後）：三個解析器（parse_document_xml／
            // table／sectpr）共用單一計數口徑——非自閉合 `w:p` 的 Start 佔槽
            // （主解析器 Start 預位、End 回填），`mc:Fallback` 整棵不計。
            // `para_start` 與 `Block::Paragraph{index}` 出自同一計數器，錨點
            // 天然落在扁平索引空間，不再需要「可見段落最小索引」重錨（舊
            // workaround：txbxContent 使兩計數器分岔，WC048 實證；已隨口徑
            // 統一移除）。clamp 保留——純防衛，守 `build_flow` 的「槽區間
            // 不重疊且 ≤ n_paras」契約不被畸形輸入擊穿。
            let start = bt.para_start.max(prev_end).min(n_paras);
            let end = raw_end.max(start).min(n_paras).max(start);
            prev_end = end;
            TableSlot {
                para_start: start,
                para_end: end,
                layout,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 測試
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use quire_ooxml::parse;

    fn corpus_bytes(rel: &str) -> Vec<u8> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        std::fs::read(root.join(rel)).unwrap_or_else(|e| panic!("讀 {rel}: {e}"))
    }

    /// 口徑統一回歸（t_580c37ac）：含巢狀 `w:p`（txbxContent）的 corpus 檔，
    /// `build_slots` 的槽區間必須直接合法（不重疊、≤ 段落數）——舊
    /// 「可見段落最小索引」重錨 workaround 已隨三解析器口徑統一移除，
    /// 錨點不再漂移。WC048／WC049／shape-in-floattable 為 txbx-in-cell
    /// 實據檔（舊註 WC048 實證）。
    #[test]
    fn txbx_corpus_slots_anchor_without_rework() {
        for rel in [
            "xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx",
            "xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx",
            "ooxmlexport/shape-in-floattable.docx",
            "xopt/TestFiles_HC031-Complicated-Document.docx",
        ] {
            let doc = parse(&corpus_bytes(rel)).expect("parse");
            let texts = doc.paragraph_texts();
            let params = quire_layout::LayoutParams::default();
            let styles = doc.parts.get("word/styles.xml").map(|s| s.as_slice());
            let slots = build_slots(&doc.tables, &texts, styles, &params, &[]);
            assert!(!slots.is_empty(), "{rel}：標為 txbx-in-cell 檔應有表格槽");
            let mut prev_end = 0usize;
            for s in &slots {
                assert!(
                    s.para_start >= prev_end
                        && s.para_end <= texts.len()
                        && s.para_end >= s.para_start,
                    "{rel}：槽區間 [{}，{}) 違反 build_flow 契約（prev_end={prev_end}, n={})",
                    s.para_start,
                    s.para_end,
                    texts.len()
                );
                prev_end = s.para_end;
            }
            // 錨點即解析器計數（無重錨）：每個槽的 para_start 必須與首個
            // 可見段落索引一致或由空表段前錨點自然給出——以不重疊＋不越界
            // 為硬契約（上面已驗），此處再鎖「槽起點不晚於首可見段落＋1」
            // 防錨點向後漂移吞正文段。
            for (slot, bt) in slots.iter().zip(&doc.tables) {
                let mut min_vis: Option<usize> = None;
                bt.walk(&mut |sub| {
                    for row in &sub.rows {
                        for cell in &row.cells {
                            for b in &cell.blocks {
                                if let Block::Paragraph { index } = b {
                                    min_vis = Some(min_vis.map_or(*index, |v| v.min(*index)));
                                }
                            }
                        }
                    }
                });
                if let Some(v) = min_vis {
                    assert!(
                        slot.para_start <= v && v < slot.para_end.max(slot.para_start + 1),
                        "{rel}：槽 [{}, {}) 未涵蓋首可見段落 {v}（錨點漂移？）",
                        slot.para_start,
                        slot.para_end
                    );
                }
            }
        }
    }
}
