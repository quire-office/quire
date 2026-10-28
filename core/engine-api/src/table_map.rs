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
    CellBlock, CellInput, CellMarginInput, CellRun, EdgeSetInput, PenInput, RowInput, TableInput,
    WidthSpec,
};
use quire_layout::{LayoutParams, SectionSpec, TableSlot, TextMeasurer};
use quire_ooxml::{Block, BodyTable, Borders, Margins, Table, TblWidth};
use quire_render_trait::{FLAG_BOLD, FLAG_ITALIC, FLAG_STRIKE, FLAG_UNDERLINE};

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

/// 格內圖片投影表（t_a1397522）：錨定段落 → 依文档序的尺寸序列。
/// 同一段落多張圖＝逐張注入（CellBlock::Image 在格內垂直堆疊、帶高累加
/// ——卡片語義；不合併，合併會虛構容量）。
fn cell_image_map(images: &[quire_ooxml::InlineImage]) -> HashMap<usize, Vec<(f32, f32)>> {
    let mut m: HashMap<usize, Vec<(f32, f32)>> = HashMap::new();
    for im in images.iter().filter(|im| im.in_cell) {
        m.entry(im.para_index).or_default().push((im.w_pt, im.h_pt));
    }
    m
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

/// 段落級聯 run → 格內文字的 run 級樣式標注（L2-cellruns／TODO(CardA) 清償）。
///
/// 只為**確有多樣式**的段落產出標注：逐 run 累加碼點偏移、相鄰同鍵
/// `(flags, size_pt, color_argb)` 段先併段，併完仍 ≥2 段才入表——單樣式段
/// （含全段同鍵）整段不入表＝發射端走舊單樣式路徑，金樣與儀表板零擾動。
/// 鍵含新欄位（段合併鍵判別鐵律：size/color 三態入鍵，缺＝級聯未宣告）。
/// `flags` 以 render-trait 位元烘焙（layout 只透傳不解釋）；`color_argb`
/// 缺＝級聯未宣告（發射端退回預設黑，與 `run_style` 口徑一致）。
fn cell_run_spans(paras: &[quire_ooxml::Paragraph]) -> HashMap<usize, Vec<CellRun>> {
    if std::env::var("L2_BUGGY").is_ok() {
        return HashMap::new(); // 回塞：整體關閉標注（紅燈必須倒）
    }
    let mut out: HashMap<usize, Vec<CellRun>> = HashMap::new();
    for (i, p) in paras.iter().enumerate() {
        if p.runs.len() < 2 {
            continue;
        }
        let mut spans: Vec<CellRun> = Vec::new();
        let mut cp = 0usize;
        for r in &p.runs {
            let n = r.text.chars().count();
            if n == 0 {
                continue; // 零文字 run（tab／br-only）不佔碼點
            }
            let mut flags = 0u8;
            if r.bold {
                flags |= FLAG_BOLD;
            }
            if r.italic {
                flags |= FLAG_ITALIC;
            }
            if r.underline {
                flags |= FLAG_UNDERLINE;
            }
            if r.strike {
                flags |= FLAG_STRIKE;
            }
            let from = cp;
            cp += n;
            if let Some(last) = spans.last_mut() {
                // 相鄰同鍵併段（沿用正文 `emit_line_run_items` 合併鍵口徑：
                // 字型宣告不入鍵——選族屬後續卡，否則同款式 run 只因字型
                // 不同就被拆開，違反零版面變動契約）。
                if last.flags == flags && last.size_pt == r.size_pt && last.color_argb == r.color {
                    last.to = cp;
                    continue;
                }
            }
            spans.push(CellRun {
                from,
                to: cp,
                flags,
                size_pt: r.size_pt,
                color_argb: r.color,
            });
        }
        if spans.len() >= 2 {
            out.insert(i, spans);
        }
    }
    out
}

/// canonical 表 → layout 輸入（遞迴巢狀表；樣式框線查 `styles`）。
/// `cell_imgs`（t_a1397522）：格內圖片投影（錨定段落 → 尺寸）——段落的
/// Block 之後追加 `CellBlock::Image`（文件序：文字在上、圖在下）。
/// `sizes`（t_5dfc8ac7）：段落索引 → 級聯有效字級（一格多段取最大）。
/// `run_spans`（L2-cellruns）：段落索引 → run 級樣式標注（只含確有多樣式
/// 的段落）；依格內**文字 block 出現序**平行落入 `CellInput::run_spans`。
/// `advs`（K7-cal）：段落索引 → 水平 advance 尺（一格多段取**最大**＝保守
/// 少塞字，同 `sizes` 取最大哲學；缺位＝不入格內欄＝舊斷行）。
#[allow(clippy::too_many_arguments)]
pub fn table_input(
    t: &Table,
    texts: &[String],
    styles: &HashMap<String, EdgeSetInput>,
    cell_imgs: &HashMap<usize, Vec<(f32, f32)>>,
    sizes: &HashMap<usize, f32>,
    line_hs: &HashMap<usize, f32>,
    line_ref_bands: u32,
    run_spans: &HashMap<usize, Vec<CellRun>>,
    advs: &HashMap<usize, f32>,
) -> TableInput {
    let p = &t.props;
    TableInput {
        // M1-band：細尺份數逐層直送（巢狀表同尺；粗尺＝0/1＝舊行為）。
        line_ref_bands,
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
                        // run 標注（L2-cellruns）：與 blocks 中**文字 block
                        // 出現序**逐位對齊的平行欄位；缺位（無標注段落）
                        // 存空 vec，保持索引對齊。
                        let mut spans: Vec<Vec<CellRun>> = Vec::new();
                        // K6：與文字 block 出現序平行的宣告行高（細化檔才
                        // 填；見 `CellInput::text_line_h_pt`）。
                        let mut text_line_h: Vec<f32> = Vec::new();
                        // 一格多段的級聯字級＝段內最大（斷行量測保守尺：
                        // 大字級少塞字，不虛構塞得下的行）。
                        let mut cell_size: Option<f32> = None;
                        // K7-cal：格內水平尺（一格多段取最大＝保守少塞字）。
                        let mut cell_adv: Option<f32> = None;
                        for b in &c.blocks {
                            match b {
                                // 段落索引引用扁平序列：越界＝畸形檔，跳過（容錯）。
                                Block::Paragraph { index } => {
                                    if let Some(s) = texts.get(*index) {
                                        blocks.push(CellBlock::Text(s.clone()));
                                        spans.push(
                                            run_spans.get(index).cloned().unwrap_or_default(),
                                        );
                                        text_line_h
                                            .push(line_hs.get(index).copied().unwrap_or(0.0));
                                    }
                                    if let Some(sz) = sizes.get(index) {
                                        cell_size = Some(cell_size.map_or(*sz, |v| v.max(*sz)));
                                    }
                                    // K7-cal：格內水平尺＝有文字段的最大值
                                    // （呼叫端已濾掉空文字段；未命中族＝1.0，
                                    // 混排格保守走舊斷行，零誤窄）。
                                    if let Some(a) = advs.get(index) {
                                        cell_adv = Some(cell_adv.map_or(*a, |v: f32| v.max(*a)));
                                    }
                                    // 錨定在該段的格內圖（t_a1397522）：段落
                                    // 之後依序追加（文件序；查無尺寸＝老實跳過）。
                                    if let Some(imgs) = cell_imgs.get(index) {
                                        for &(w, h) in imgs {
                                            blocks.push(CellBlock::Image { w_pt: w, h_pt: h });
                                        }
                                    }
                                }
                                Block::Table(inner) => {
                                    blocks.push(CellBlock::Table(Box::new(table_input(
                                        inner,
                                        texts,
                                        styles,
                                        cell_imgs,
                                        sizes,
                                        line_hs,
                                        line_ref_bands,
                                        run_spans,
                                        advs,
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
                            text_size_pt: cell_size,
                            text_advance_scale: cell_adv,
                            text_line_h_pt: text_line_h,
                            run_spans: spans,
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// 表格（含全部巢狀表）覆蓋的段落上界（不含）＝最大段索引＋1；全無段落＝para_start。
pub(crate) fn table_para_end(t: &Table, para_start: usize) -> usize {
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

/// `wp:positionH/V` 之 `<wp:align>` 白名單（ECMA-376 §20.4 ST_PosHAlign／
/// ST_PosVAlign；t_1a79f1c5）。t：left/center/right＋inside/outside；
/// b：top/center/bottom＋inside/outside。其餘枚舉（insideLeft/bar/...）
/// 保守退回退化鏈。
///
/// `inside`／`outside` 為裝訂雙面頁枚舉，真實落點取決於奇偶頁與裝訂方向；
/// 本層無頁碼奇偶資訊，故以 frame 邊界近似：inside≈left、outside≈right。
fn h_align_x(align: &str, frame_w: f32, w_pt: f32) -> Option<f32> {
    // 比對前轉小寫：第三方工具可能輸出 `<wp:align>Center</wp:align>`（S3），
    // 與 drawing 儲存處的正規化互為雙保險。
    match align.to_ascii_lowercase().as_str() {
        "left" | "inside" => Some(0.0),
        "center" => Some((frame_w - w_pt) / 2.0),
        "right" | "outside" => Some(frame_w - w_pt),
        _ => None,
    }
}
fn v_align_y(align: &str, frame_h: f32, h_pt: f32) -> Option<f32> {
    match align.to_ascii_lowercase().as_str() {
        // 同 h 軸：inside≈top、outside≈bottom 之近似（無奇偶頁資訊）。
        "top" | "inside" => Some(0.0),
        // `inner` 非 ST_PosVAlign 標準枚舉（來源可疑，保留相容），
        // 語意近似居中。
        "center" | "inner" => Some((frame_h - h_pt) / 2.0),
        "bottom" | "outside" => Some(frame_h - h_pt),
        _ => None,
    }
}

/// 浮動圖座標解析（t_1a79f1c5）：`wp:positionH/V` 的 posOffset 與 align 是
/// **互斥兩式**（ECMA-376 §20.4.2.8）——OOXMLExport 對整頁封面圖（wrapNone
/// ＋`<wp:align>center</wp:align>`）只寫 align 不寫 posOffset，舊口徑把缺
/// posOffset 一律退化 inline，792pt 封面圖撐爆行網格（1 頁变 3 頁）。
/// 現行規則：兩軸 posOffset 齊備 → 直接用；缺軸補 `<wp:align>` 換算
/// （frame 依 relativeFrom 分類取頁寬／版心寬／錨點段寬）；兩式皆無 →
/// None（呼叫端退化 inline，誠實近似宁多撑帶不虚構空頁）。
/// `page_w`／`page_h`＝頁寬高（page 系 frame）；`avail_w`＝版心寬（margin 系 frame）。
fn resolved_anchor(
    im: &quire_ooxml::InlineImage,
    page_w: f32,
    page_h: f32,
    avail_w: f32,
    avail_h: f32,
    anchor_para_w: Option<f32>,
) -> Option<quire_layout::FloatingAnchor> {
    // 逐軸可解析才成錨：posOffset 或 `<wp:align>` 兩式任一即可（互斥但
    // 不必同時存在）；relativeFrom 缺＝段落系近似（origin_of 同款容錯）。
    if !im.floating {
        return None;
    }
    let x_origin = quire_ooxml::origin_of(im.pos_x_rel.as_deref());
    let y_origin = quire_ooxml::origin_of(im.pos_y_rel.as_deref());
    let frame_w = match x_origin {
        "page" => page_w,
        "column" | "margin" => avail_w,
        _ => anchor_para_w.unwrap_or(avail_w),
    };
    let frame_h = match y_origin {
        "page" => page_h,
        _ => avail_h.max(1.0),
    };
    // 逐軸三式平滑退檔：posOffset → `<wp:align>` → `<wp14:pctPos*Offset>`。
    // align 值不被識別（如 insideLeft／非法字串）只算該式落空，不可短路
    // 整個錨點——否則第三式 pct 永遠退不到（S2）。
    let x_pt = im
        .pos_x_pt
        .or_else(|| {
            im.pos_x_align
                .as_deref()
                .and_then(|a| h_align_x(a, frame_w, im.w_pt))
        })
        .or_else(|| im.pos_x_pct.map(|p| frame_w * p / 100_000.0))?;
    let y_pt = im
        .pos_y_pt
        .or_else(|| {
            im.pos_y_align
                .as_deref()
                .and_then(|a| v_align_y(a, frame_h, im.h_pt))
        })
        .or_else(|| im.pos_y_pct.map(|p| frame_h * p / 100_000.0))?;
    Some(quire_layout::FloatingAnchor {
        x_pt,
        y_pt,
        x_frame: quire_layout::origin_frame(x_origin),
        y_frame: quire_layout::origin_frame(y_origin),
    })
}

/// 正文流圖片錨點（t_a1397522）：InlineImage 投影 → layout `ImageBand` 序列。
/// 規則：
/// - 格內圖（in_cell）不進正文流（由 CellBlock::Image 負載，格內堆疊）；
/// - 浮動圖（anchor）且座標可解析（posOffset 或 align，t_1a79f1c5）→
///   **不佔帶**（bands=0），`anchor=Some(FloatingAnchor)`，各軸原點系由
///   relativeFrom 分類；
/// - 鎖點兩式皆缺 → 退化 inline（佔帶，誠實近似宁多撑帶不虚構空頁）；
/// - 帶高 ceil(h/line_h)（至少 1）；寬高老實原值（layout 端再鉗）。
///
/// align 換算的 frame 尺寸逐段查 `sections`（缺項退回 params 基準），
/// 與表格 `avail_w_for` 同口徑。輸出依 para 穩定排序
/// （build_flow_images 的單調游標契約）。
pub fn build_images(
    images: &[quire_ooxml::InlineImage],
    line_h: f32,
    params: &LayoutParams,
    sections: &[SectionSpec],
) -> Vec<quire_layout::ImageBand> {
    let mut out: Vec<quire_layout::ImageBand> = images
        .iter()
        .filter(|im| !im.in_cell)
        .map(|im| {
            // 錨定段落的頁面幾何：page 系 frame 用頁寬高，margin 系用版心寬高。
            let (page_w, page_h, avail_w, avail_h) = match sections.get(im.para_index) {
                Some(s) => (
                    s.geo.page_w_pt,
                    s.geo.page_h_pt,
                    (s.geo.page_w_pt - s.geo.margin_left_pt - s.geo.margin_right_pt).max(1.0),
                    (s.geo.page_h_pt - s.geo.margin_top_pt - s.geo.margin_bottom_pt).max(1.0),
                ),
                None => (
                    params.page_w_pt,
                    params.page_h_pt,
                    params.content_w_pt(),
                    (params.page_h_pt
                        - params.margin(quire_layout::MARGIN_TOP)
                        - params.margin(quire_layout::MARGIN_BOTTOM))
                    .max(1.0),
                ),
            };
            let anchor = resolved_anchor(im, page_w, page_h, avail_w, avail_h, None);
            let bands = if anchor.is_none() {
                // 帶量化鉗上限（t_c1fb4c69 S1-4）：病態 extent（源頭值域
                // 拒絕後仍可能漏進投影層）不得量化出天量帶數拖垮分頁。
                let b = (im.h_pt / line_h).ceil();
                if !b.is_finite() {
                    quire_layout::MAX_PAGE_BANDS
                } else {
                    ((b.max(1.0)) as u32).min(quire_layout::MAX_PAGE_BANDS)
                }
            } else {
                0
            };
            quire_layout::ImageBand {
                para: im.para_index,
                w_pt: im.w_pt,
                h_pt: im.h_pt,
                bands,
                anchor,
                page_pinned: im.page_pinned,
            }
        })
        .collect();
    out.sort_by_key(|b| b.para);
    out
}

/// 由 canonical 樹＋扁平文字建出排版完成的表格槽序列（engine 開啟文件時呼叫）。
/// 回傳依 para_start 排序（parser 保證 body 序單調），`build_flow` 契約直接成立。
/// `images`（t_a1397522）：全文圖片投影；格內圖注入對應 `CellBlock::Image`。
/// `paras`（t_5dfc8ac7）：級聯烘焙後的段落投影（run 帶 effective size）——
/// 逐格文字字級＝格內段落首 run 有效字級（首 run 無宣告＝無宣告）。
#[allow(clippy::too_many_arguments)]
pub fn build_slots(
    tables: &[BodyTable],
    texts: &[String],
    styles_xml: Option<&[u8]>,
    params: &LayoutParams,
    sections: &[SectionSpec],
    images: &[quire_ooxml::InlineImage],
    paras: &[quire_ooxml::Paragraph],
    line_hs: &[f32],
    line_ref_bands: u32,
    advance_scales: &[f32],
) -> Vec<TableSlot> {
    let styles = styles_xml.map(table_style_borders).unwrap_or_default();
    let cell_imgs = cell_image_map(images);
    // 段落 → 級聯有效字級（首 run；None 不入表＝全域尺）。
    let mut sizes: HashMap<usize, f32> = HashMap::new();
    for (i, p) in paras.iter().enumerate() {
        if let Some(sz) = p.runs.first().and_then(|r| r.size_pt) {
            sizes.insert(i, sz);
        }
    }
    // K7-cal：段落索引 → 水平 advance 尺。**空文字段不參與**（空行不斷行、
    // 不 demanded 寬——ECMA 子表尾部填充空段不得把整格推回 1.0）；有文字段
    // 一律入表（未命中族＝1.0），格內取最大＝混排保守（最寬尺少塞字，
    // 未校準族與校準族混排的格整體走舊斷行，零誤窄）。
    let advs: HashMap<usize, f32> = advance_scales
        .iter()
        .enumerate()
        .filter(|(i, a)| a.is_finite() && texts.get(*i).is_some_and(|t| !t.trim().is_empty()))
        .map(|(i, a)| (i, *a))
        .collect();
    // L2-cellruns：格內文字的 run 級樣式標注（只含確有多樣式的段落；
    // 單樣式段整段不入表＝發射端走舊路徑，金樣零位移）。
    let run_spans = cell_run_spans(paras);
    // K6：段落索引 → 級聯量化前行高（僅細化檔消費；粗尺 lref=1 時表空，
    // 格內行高逐位元組不動）。
    let line_h_map: HashMap<usize, f32> = if line_ref_bands > 1 {
        line_hs
            .iter()
            .enumerate()
            .filter(|(_, h)| h.is_finite() && **h > 0.0)
            .map(|(i, h)| (i, *h))
            .collect()
    } else {
        HashMap::new()
    };
    let m = TextMeasurer::new(params.font_size_pt);
    let line_h = params.line_h_pt();
    let mut prev_end = 0usize; // 逐槽正規化游標：槽區間必不重疊且 ≤ n_paras
    let n_paras = texts.len();
    tables
        .iter()
        .map(|bt| {
            let input = table_input(
                &bt.table,
                texts,
                &styles,
                &cell_imgs,
                &sizes,
                &line_h_map,
                line_ref_bands,
                &run_spans,
                &advs,
            );
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
    use quire_ooxml::{parse, InlineImage};

    /// 最小 InlineImage（只填測試有意義的欄位）。
    fn img(para: usize, h_pt: f32) -> InlineImage {
        InlineImage {
            para_index: para,
            w_pt: 100.0,
            h_pt,
            floating: false,
            in_cell: false,
            pos_x_pt: None,
            pos_x_rel: None,
            pos_x_align: None,
            pos_x_pct: None,
            pos_y_pt: None,
            pos_y_rel: None,
            pos_y_align: None,
            pos_y_pct: None,
            overlap: None,
            embed_rid: None,
            embed_rids: Vec::new(),
            txbx_texts: Vec::new(),
            txbx_start: 0,
            txbx_children: Vec::new(),
            // K4c 新增欄位（全部老實缺省＝舊行為；本測試只吃幾何）。
            fill_argb: None,
            filled: None,
            stroke_argb: None,
            stroked: None,
            stroke_width_pt: None,
            stroke_dash: None,
            txbx_rect: None,
            txbx_inset: None,
            txbx_vertical: false,
            txbx_table: Vec::new(),
            page_pinned: false,
            rel_height: 0,
            wps_annotation: false,
        }
    }

    /// 帶兩軸 page 原點的浮圖；y 已由 posOffset 鎖定，測試只折騰 x 軸。
    fn anchored_img() -> InlineImage {
        let mut im = img(0, 50.0);
        im.floating = true;
        im.pos_x_rel = Some("page".into());
        im.pos_y_rel = Some("page".into());
        im.pos_y_pt = Some(10.0);
        im
    }

    /// S2（t_1a79f1c5 後續）：align 值不被識別時只算該式落空，必須平滑退到
    /// 第三式 pct；舊 `?` 會讓整個錨點回 None（浮圖誤退化 inline 撐帶）。
    /// `inside` 已於本卡補入白名單，故用仍不識別的 `insideLeft` 觸發同一路徑。
    #[test]
    fn unrecognized_align_falls_back_to_pct() {
        let mut im = anchored_img();
        im.pos_x_align = Some("insideLeft".into());
        im.pos_x_pct = Some(50_000.0);
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None)
            .expect("align 落空後應退到 pct，不得整體回 None");
        assert!((a.x_pt - 300.0).abs() < 1e-4, "pct 半頁 x={}", a.x_pt);
    }

    /// 回歸保護：`left` 仍走 align 算式得 0，不受退檔重構影響。
    #[test]
    fn left_align_computes_zero_x() {
        let mut im = anchored_img();
        im.pos_x_align = Some("left".into());
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None).expect("left 應可解");
        assert_eq!(a.x_pt, 0.0);
    }

    /// S3：第三方工具輸出首字母大寫 `<wp:align>Center</wp:align>` 仍須命中
    /// center 算式（drawing 儲存正規化＋此處比對轉小寫雙保險）。
    #[test]
    fn uppercase_center_align_matches_center_formula() {
        let mut im = anchored_img();
        im.pos_x_align = Some("Center".into());
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None).expect("Center 應可解");
        assert!((a.x_pt - 250.0).abs() < 1e-4, "置中 x={}", a.x_pt);
    }

    /// S2：ECMA-376 `inside`／`outside` 裝訂枚舉以 frame 邊界近似
    /// （inside≈left、outside≈right，無奇偶頁資訊）。
    #[test]
    fn inside_outside_align_approximate_frame_edges() {
        let mut im = anchored_img();
        im.pos_x_align = Some("inside".into());
        assert_eq!(
            resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None)
                .expect("inside 應可解")
                .x_pt,
            0.0
        );
        im.pos_x_align = Some("outside".into());
        assert_eq!(
            resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None)
                .expect("outside 應可解")
                .x_pt,
            500.0
        );
    }

    /// 回歸保護：mce-nested（Fallback 雙份＋align/pct 鎖點）仍收斂 1 頁。
    #[test]
    fn mce_nested_corpus_stays_one_page() {
        let bytes = corpus_bytes("ooxmlexport/mce-nested.docx");
        let engine = crate::Engine::from_docx_bytes(&bytes).expect("開啟 mce-nested.docx");
        assert_eq!(engine.page_count(), 1, "align/pct 三式退檔後仍應為 1 頁");
    }

    /// S1-4（t_c1fb4c69）engine 側同閘：正文流圖帶高 `(h/line_h).ceil() as u32`
    /// 遇病態 extent（f32::MAX→天量帶數／inf）必鉗到 quire_layout 的帶量化
    /// 上限——源頭 ooxml 值域拒絕後，這是投影層的第二道閘。
    #[test]
    fn build_images_clamps_giant_extent_bands() {
        let params = quire_layout::LayoutParams::default();
        let out = build_images(
            &[img(0, f32::MAX), img(1, f32::INFINITY)],
            16.8,
            &params,
            &[],
        );
        assert_eq!(out.len(), 2);
        for b in &out {
            assert!(
                b.bands <= quire_layout::MAX_PAGE_BANDS && b.bands >= 1,
                "巨 extent 帶數必鉗上限：{}",
                b.bands
            );
        }
    }

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
            let slots = build_slots(
                &doc.tables,
                &texts,
                styles,
                &params,
                &[],
                &[],
                &doc.paragraphs,
                &[],
                1,
                &[],
            );
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
