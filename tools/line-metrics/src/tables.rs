//! canonical 表格樹 → layout 層 `TableInput`／`TableSlot` 映射（觀測工具移植版）。
//!
//! 本模組是 `core/engine-api/src/table_map.rs` 的**逐式移植**（twip→pt 換算、
//! 樣式框線查表、build_slots/build_images 全保留）。為什麼要移植：分層鐵律
//! 下 quire-engine 的 table_map 模組不對外開放，而本工具必須以**與引擎完全
//! 相同的輸入**重放分頁（golden 鐵律：觀測數值＝分頁判定的同一個數值）。
//! 移植不會改變任何渲染行為——這裡只是工具側的唯讀副本，引擎本身的分頁
//! 照樣走它自己的原路徑。**引擎側改了 table_map 時，本檔必須同步**（檔案頭
//! 即聲明同步責任），否則工具輸出與真實分頁會分岔。

use std::collections::HashMap;

use quire_layout::table::{
    CellBlock, CellInput, CellMarginInput, EdgeSetInput, PenInput, RowInput, TableInput, WidthSpec,
};
use quire_layout::{
    FloatingAnchor, ImageBand, LayoutParams, SectionSpec, TableSlot, TextMeasurer, MAX_PAGE_BANDS,
};
use quire_ooxml::{
    origin_of, Block, BodyTable, Border, Borders, InlineImage, Margins, Table, TblWidth,
};

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

fn pen(b: Option<&Border>) -> Option<PenInput> {
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

/// 格內圖片投影表（t_a1397522）：錨定段落 → 依文件序的尺寸序列。
/// 同一段落多張圖＝逐張注入（CellBlock::Image 在格內垂直堆疊、帶高累加
/// ——卡片語義；不合併，合併會虛構容量）。
fn cell_image_map(images: &[InlineImage]) -> HashMap<usize, Vec<(f32, f32)>> {
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
    let pick = |tag: &str| -> Option<Border> {
        let pat = format!("<w:{tag} ");
        let k = region.find(&pat)?;
        let e = region[k..]
            .find("/>")
            .map(|e| k + e)
            .unwrap_or(region.len());
        let one = &region[k..e];
        let b = Border {
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
/// `cell_imgs`：格內圖片投影（錨定段落 → 尺寸）——段落的
/// Block 之後追加 `CellBlock::Image`（文件序：文字在上、圖在下）。
/// `sizes`：段落索引 → 級聯有效字級（一格多段取最大）。
pub fn table_input(
    t: &Table,
    texts: &[String],
    styles: &HashMap<String, EdgeSetInput>,
    cell_imgs: &HashMap<usize, Vec<(f32, f32)>>,
    sizes: &HashMap<usize, f32>,
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
                        // 一格多段的級聯字級＝段內最大（斷行量測保守尺：
                        // 大字級少塞字，不虛構塞得下的行）。
                        let mut cell_size: Option<f32> = None;
                        for b in &c.blocks {
                            match b {
                                // 段落索引引用扁平序列：越界＝畸形檔，跳過（容錯）。
                                Block::Paragraph { index } => {
                                    if let Some(s) = texts.get(*index) {
                                        blocks.push(CellBlock::Text(s.clone()));
                                    }
                                    if let Some(sz) = sizes.get(index) {
                                        cell_size = Some(cell_size.map_or(*sz, |v| v.max(*sz)));
                                    }
                                    // 錨定在該段的格內圖：段落
                                    // 之後依序追加（文件序；查無尺寸＝老實跳過）。
                                    if let Some(imgs) = cell_imgs.get(index) {
                                        for &(w, h) in imgs {
                                            blocks.push(CellBlock::Image { w_pt: w, h_pt: h });
                                        }
                                    }
                                }
                                Block::Table(inner) => {
                                    blocks.push(CellBlock::Table(Box::new(table_input(
                                        inner, texts, styles, cell_imgs, sizes,
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

/// `wp:positionH/V` 之 `<wp:align>` 白名單（ECMA-376 §20.4 ST_PosHAlign／
/// ST_PosVAlign）。t：left/center/right＋inside/outside；
/// b：top/center/bottom＋inside/outside。其餘枚舉保守退回退化鏈。
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
        // `inner` 非 ST_PosVAlign 標準枚舉（來源可疑，保留相容語意近似居中）。
        "center" | "inner" => Some((frame_h - h_pt) / 2.0),
        "bottom" | "outside" => Some(frame_h - h_pt),
        _ => None,
    }
}

/// 浮動圖座標解析：`wp:positionH/V` 的 posOffset 與 align 是
/// **互斥兩式**（ECMA-376 §20.4.2.8）——兩軸 posOffset 齊備 → 直接用；缺軸
/// 補 `<wp:align>` 換算（frame 依 relativeFrom 分類取頁寬／版心寬／錨點段寬）；
/// 兩式皆無 → None（呼叫端退化 inline，誠實近似宁多撑帶不虚構空頁）。
/// `page_w`／`page_h`＝頁寬高（page 系 frame）；`avail_w`＝版心寬（margin 系 frame）。
fn resolved_anchor(
    im: &InlineImage,
    page_w: f32,
    page_h: f32,
    avail_w: f32,
    avail_h: f32,
    anchor_para_w: Option<f32>,
) -> Option<FloatingAnchor> {
    // 逐軸可解析才成錨：posOffset 或 `<wp:align>` 兩式任一即可（互斥但
    // 不必同時存在）；relativeFrom 缺＝段落系近似（origin_of 同款容錯）。
    if !im.floating {
        return None;
    }
    let x_origin = origin_of(im.pos_x_rel.as_deref());
    let y_origin = origin_of(im.pos_y_rel.as_deref());
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
    Some(FloatingAnchor {
        x_pt,
        y_pt,
        x_frame: quire_layout::origin_frame(x_origin),
        y_frame: quire_layout::origin_frame(y_origin),
    })
}

/// 正文流圖片錨點：InlineImage 投影 → layout `ImageBand` 序列。
/// 規則：
/// - 格內圖（in_cell）不進正文流（由 CellBlock::Image 負載，格內堆疊）；
/// - 浮動圖（anchor）且座標可解析（posOffset 或 align）→ **不佔帶**
///   （bands=0），`anchor=Some(FloatingAnchor)`，各軸原點系由 relativeFrom 分類；
/// - 鎖點兩式皆缺 → 退化 inline（佔帶，誠實近似宁多撑帶不虚構空頁）；
/// - 帶高 ceil(h/line_h)（至少 1）；寬高老實原值（layout 端再鉗）。
///
/// align 換算的 frame 尺寸逐段查 `sections`（缺項退回 params 基準），
/// 與表格 `avail_w_for` 同口徑。輸出依 para 穩定排序
/// （build_flow_images 的單調游標契約）。
pub fn build_images(
    images: &[InlineImage],
    line_h: f32,
    params: &LayoutParams,
    sections: &[SectionSpec],
) -> Vec<ImageBand> {
    let mut out: Vec<ImageBand> = images
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
                    MAX_PAGE_BANDS
                } else {
                    ((b.max(1.0)) as u32).min(MAX_PAGE_BANDS)
                }
            } else {
                0
            };
            ImageBand {
                para: im.para_index,
                w_pt: im.w_pt,
                h_pt: im.h_pt,
                bands,
                anchor,
            }
        })
        .collect();
    out.sort_by_key(|b| b.para);
    out
}

/// 由 canonical 樹＋扁平文字建出排版完成的表格槽序列（原 engine 開啟文件時呼叫）。
/// 回傳依 para_start 排序（parser 保證 body 序單調），`build_flow` 契約直接成立。
/// `images`：全文圖片投影；格內圖注入對應 `CellBlock::Image`。
/// `paras`：級聯烘焙後的段落投影（run 帶 effective size）——
/// 逐格文字字級＝格內段落首 run 有效字級（首 run 無宣告＝無宣告）。
#[allow(clippy::too_many_arguments)]
pub fn build_slots(
    tables: &[BodyTable],
    texts: &[String],
    styles_xml: Option<&[u8]>,
    params: &LayoutParams,
    sections: &[SectionSpec],
    images: &[InlineImage],
    paras: &[quire_ooxml::Paragraph],
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
    let m = TextMeasurer::new(params.font_size_pt);
    let line_h = params.line_h_pt();
    let mut prev_end = 0usize; // 逐槽正規化游標：槽區間必不重疊且 ≤ n_paras
    let n_paras = texts.len();
    tables
        .iter()
        .map(|bt| {
            let input = table_input(&bt.table, texts, &styles, &cell_imgs, &sizes);
            let avail_w = avail_w_for(sections, params, bt.para_start);
            let layout = quire_layout::table::layout_table(&input, avail_w, line_h, &m, 0);
            let raw_end = table_para_end(&bt.table, bt.para_start);
            // 索引空間對齊（t_580c37ac 後）：三個解析器共用單一計數口徑——
            // 非自閉合 `w:p` 的 Start 佔槽，`mc:Fallback` 整棵不計。
            // clamp 保留——純防衛，守 `build_flow` 的「槽區間不重疊且
            // ≤ n_paras」契約不被畸形輸入擊穿。
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
