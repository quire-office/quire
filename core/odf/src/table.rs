//! ODF 正文流二維表格模型（ODF-TABLE-MODEL）。
//!
//! 依據已定稿規格書 `docs/odf-table-model-design.md` 實裝：
//! - 正文流二維表格投影模型（`OdfBodyTable`、`OdfTable`、`OdfTableRow`、`OdfTableCell`）。
//! - 展開邊界與狀態機：欄位重複（`number-columns-repeated`，上限 65535）、
//!   水平跨欄（`number-columns-spanned` 搭配 covered-table-cell 略過）、
//!   垂直跨列（`number-rows-spanned` 搭配 covered-table-cell 延續狀態機）。
//! - 樣式與幾何屬性級聯：欄寬（`style:column-width`）、整表寬度與縮排
//!   （`style:width`、`fo:margin-left`）、儲存格內邊距（`fo:padding`）、
//!   邊框（`fo:border`）與底色（`fo:background-color`）。
//! - 排版對接：將 `OdfBodyTable` 映射為既有 `quire_layout::table::TableInput`
//!   並接入帶尺切片排版器。

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

use quire_layout::table::{
    layout_table, CellBlock, CellInput, CellMarginInput, EdgeSetInput, PenInput, RowInput,
    TableInput, WidthSpec,
};
use quire_layout::{TableSlot, TextMeasurer};

use crate::layout_glue::{
    alt_text_is_metadata, is_alt_text, is_draw_container, LayoutParams, OdfProjection,
};
use crate::page_geometry::CM_TO_PT;
use crate::{OdfBundle, OdfError};

/// 網格欄定義（對齊 quire_ooxml::GridCol）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OdfGridCol {
    /// 欄寬（pt），由 style:column-width 換算；若為相對權重寬度則於排版期依表寬分配。
    pub w_pt: Option<f32>,
    /// 原始寬度字串（如 "1.965cm" 或 "21845*"），保持無損紀錄。
    pub raw: Option<String>,
}

/// 表格全域屬性（對齊 quire_ooxml::TableProps）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OdfTableProps {
    /// 表格總寬度（pt），由 style:table-properties/@style:width 換算。
    pub width_pt: Option<f32>,
    /// 表格對齊方式：Left（左對齊）、Center（置中）、Right（右對齊）、Margins（自適應）。
    pub align: Option<String>,
    /// 表格左外邊距（pt），由 fo:margin-left 換算。
    pub indent_pt: Option<f32>,
    /// 表格預設背景底色（0x00RRGGBB），由 fo:background-color 換算。
    pub background_rgb: Option<u32>,
}

/// 列屬性（對齊 quire_ooxml::RowProps）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OdfRowProps {
    /// 最小列高（pt），由 style:min-row-height 換算。
    pub min_height_pt: Option<f32>,
    /// 固定列高（pt），由 style:row-height 換算。
    pub exact_height_pt: Option<f32>,
    /// 是否為跨頁重複表頭（由父容器 <table:table-header-rows> 標註）。
    pub is_header: bool,
    /// 列內不可跨頁斷行（由 fo:keep-together="always" 標註；缺省為 false）。
    pub cant_split: bool,
}

/// 儲存格跨列合併狀態機狀態（對齊 quire_ooxml::CellMerge 語義）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OdfCellMerge {
    /// 跨列起點格（table:number-rows-spanned > 1）。
    Restart,
    /// 跨列延續格（由 table:covered-table-cell 產生的佔位格）。
    Continue,
}

/// 儲存格單邊框線描述（對齊 quire_ooxml::Border）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OdfBorder {
    pub width_pt: f32,
    pub color_rgb: u32,
    pub is_double: bool,
}

/// 儲存格屬性（對齊 quire_ooxml::CellProps）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OdfCellProps {
    /// 水平跨欄數（table:number-columns-spanned，預設 1）。
    pub grid_span: u16,
    /// 垂直跨列合併狀態。
    pub v_merge: Option<OdfCellMerge>,
    /// 儲存格背景底色（0x00RRGGBB），由 fo:background-color 換算。
    pub shading_rgb: Option<u32>,
    /// 儲存格四邊框線：[top, left, bottom, right]。
    pub borders: [Option<OdfBorder>; 4],
    /// 儲存格內邊距（pt）：上、下、左、右。
    pub pad_top_pt: f32,
    pub pad_bottom_pt: f32,
    pub pad_left_pt: f32,
    pub pad_right_pt: f32,
    /// 儲存格垂直對齊方式："top" | "middle" | "bottom"。
    pub valign: Option<String>,
}

/// 儲存格內容區塊：以連續半開槽位區間引用頂層平鋪 paragraphs 序列。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OdfTableCell {
    pub props: OdfCellProps,
    /// 指向頂層 OdfProjection::paragraphs 的槽位區間 [para_start, para_end)。
    /// 儲存格為空時，para_start == para_end（老實回空區間，不虛構段落）。
    pub para_range: (usize, usize),
}

/// 單一表格行（對齊 quire_ooxml::TableRow）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OdfTableRow {
    pub props: OdfRowProps,
    pub cells: Vec<OdfTableCell>,
}

/// 完整表格投影模型（對齊 quire_ooxml::Table）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OdfTable {
    pub props: OdfTableProps,
    /// 欄寬網格清單。
    pub grid: Vec<OdfGridCol>,
    /// 資料列清單。
    pub rows: Vec<OdfTableRow>,
}

/// 正文流區塊表格（對齊 quire_ooxml::BodyTable）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OdfBodyTable {
    /// 此表在頂層平鋪段落流中「首個格內段落」的索引 slot_idx。
    /// 表格在正文區塊流中位於第 para_start - 1 段之後、第 para_start 段之前。
    pub para_start: usize,
    pub table: OdfTable,
}

// ---------------------------------------------------------------------------
// 數值與樣式輔助解析純函數
// ---------------------------------------------------------------------------

/// 解析 ODF 長度字串（如 "1.965cm", "0.5pt", "0cm"）為 pt。
pub fn parse_odf_length_pt(value: &str) -> Option<f32> {
    let s = value.trim();
    if s == "0" || s == "0.0" {
        return Some(0.0);
    }
    const UNITS: [(&str, f32); 5] = [
        ("cm", CM_TO_PT),
        ("mm", CM_TO_PT / 10.0),
        ("in", 72.0),
        ("pt", 1.0),
        ("pc", 12.0),
    ];
    let (num, factor) = UNITS
        .iter()
        .find_map(|(suffix, f)| s.strip_suffix(*suffix).map(|n| (n, *f)))?;
    let v: f32 = num.trim().parse().ok()?;
    if v.is_finite() && v >= 0.0 {
        Some(v * factor)
    } else {
        None
    }
}

/// 解析 ODF 色彩字串（如 "#d9e1f2"）為 0x00RRGGBB。
pub fn parse_odf_color_rgb(value: &str) -> Option<u32> {
    let s = value.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("transparent") || s.eq_ignore_ascii_case("none") {
        return None;
    }
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() == 6 {
        u32::from_str_radix(hex, 16).ok()
    } else if hex.len() == 3 {
        let r = u32::from_str_radix(&hex[0..1], 16).ok()?;
        let g = u32::from_str_radix(&hex[1..2], 16).ok()?;
        let b = u32::from_str_radix(&hex[2..3], 16).ok()?;
        Some((r << 20) | (r << 16) | (g << 12) | (g << 8) | (b << 4) | b)
    } else {
        None
    }
}

/// 解析 ODF 邊框字串（如 "0.5pt solid #000000"）為 `OdfBorder`。
pub fn parse_odf_border(value: &str) -> Option<OdfBorder> {
    let s = value.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("hidden") {
        return None;
    }
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }
    let mut width_pt = 0.5f32;
    let mut color_rgb = 0u32;
    let mut is_double = false;
    for tok in tokens {
        if let Some(w) = parse_odf_length_pt(tok) {
            width_pt = w;
        } else if tok.starts_with('#') {
            if let Some(c) = parse_odf_color_rgb(tok) {
                color_rgb = c;
            }
        } else if tok.eq_ignore_ascii_case("double") {
            is_double = true;
        }
    }
    Some(OdfBorder {
        width_pt,
        color_rgb,
        is_double,
    })
}

// ---------------------------------------------------------------------------
// 表格樣式簿（TableStyleBook）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct TableStyleBook {
    pub tables: HashMap<String, OdfTableProps>,
    pub columns: HashMap<String, OdfGridCol>,
    pub rows: HashMap<String, OdfRowProps>,
    pub cells: HashMap<String, OdfCellProps>,
}

impl TableStyleBook {
    pub fn collect_from_xml(&mut self, xml_bytes: &[u8]) {
        let mut reader = Reader::from_reader(xml_bytes);
        let mut buf = Vec::new();
        let mut cur_family: Option<String> = None;
        let mut cur_name: Option<String> = None;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    let qname = e.name();
                    let name = qname.as_ref();
                    if name == b"style:style" {
                        let mut fam = None;
                        let mut s_name = None;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"style:family" {
                                fam = String::from_utf8(attr.value.to_vec()).ok();
                            } else if attr.key.as_ref() == b"style:name" {
                                s_name = String::from_utf8(attr.value.to_vec()).ok();
                            }
                        }
                        cur_family = fam;
                        cur_name = s_name;
                    } else if (name == b"style:table-properties" || name == b"table-properties")
                        && cur_family.as_deref() == Some("table")
                    {
                        if let Some(n) = cur_name.as_deref() {
                            let mut props = self.tables.remove(n).unwrap_or_default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                if key == b"style:width" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        props.width_pt = parse_odf_length_pt(v);
                                    }
                                } else if key == b"table:align" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        props.align = Some(v.to_string());
                                    }
                                } else if key == b"fo:margin-left" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        props.indent_pt = parse_odf_length_pt(v);
                                    }
                                } else if key == b"fo:background-color" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        props.background_rgb = parse_odf_color_rgb(v);
                                    }
                                }
                            }
                            self.tables.insert(n.to_string(), props);
                        }
                    } else if (name == b"style:table-column-properties"
                        || name == b"table-column-properties")
                        && cur_family.as_deref() == Some("table-column")
                    {
                        if let Some(n) = cur_name.as_deref() {
                            let mut col = self.columns.remove(n).unwrap_or(OdfGridCol {
                                w_pt: None,
                                raw: None,
                            });
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                if key == b"style:column-width" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        col.w_pt = parse_odf_length_pt(v);
                                        col.raw = Some(v.to_string());
                                    }
                                } else if key == b"style:rel-column-width" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        if col.raw.is_none() {
                                            col.raw = Some(v.to_string());
                                        }
                                    }
                                }
                            }
                            self.columns.insert(n.to_string(), col);
                        }
                    } else if (name == b"style:table-row-properties"
                        || name == b"table-row-properties")
                        && cur_family.as_deref() == Some("table-row")
                    {
                        if let Some(n) = cur_name.as_deref() {
                            let mut row = self.rows.remove(n).unwrap_or_default();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                if key == b"style:min-row-height" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        row.min_height_pt = parse_odf_length_pt(v);
                                    }
                                } else if key == b"style:row-height" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        row.exact_height_pt = parse_odf_length_pt(v);
                                    }
                                } else if key == b"fo:keep-together" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        row.cant_split = v == "always";
                                    }
                                }
                            }
                            self.rows.insert(n.to_string(), row);
                        }
                    } else if (name == b"style:table-cell-properties"
                        || name == b"table-cell-properties")
                        && cur_family.as_deref() == Some("table-cell")
                    {
                        if let Some(n) = cur_name.as_deref() {
                            let mut cell = self.cells.remove(n).unwrap_or(OdfCellProps {
                                grid_span: 1,
                                ..Default::default()
                            });
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                if key == b"fo:background-color" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        cell.shading_rgb = parse_odf_color_rgb(v);
                                    }
                                } else if key == b"fo:border" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        let b = parse_odf_border(v);
                                        cell.borders = [b.clone(), b.clone(), b.clone(), b];
                                    }
                                } else if key == b"fo:border-top" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        cell.borders[0] = parse_odf_border(v);
                                    }
                                } else if key == b"fo:border-left" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        cell.borders[1] = parse_odf_border(v);
                                    }
                                } else if key == b"fo:border-bottom" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        cell.borders[2] = parse_odf_border(v);
                                    }
                                } else if key == b"fo:border-right" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        cell.borders[3] = parse_odf_border(v);
                                    }
                                } else if key == b"fo:padding" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        if let Some(pad) = parse_odf_length_pt(v) {
                                            cell.pad_top_pt = pad;
                                            cell.pad_bottom_pt = pad;
                                            cell.pad_left_pt = pad;
                                            cell.pad_right_pt = pad;
                                        }
                                    }
                                } else if key == b"fo:padding-top" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        cell.pad_top_pt = parse_odf_length_pt(v).unwrap_or(0.0);
                                    }
                                } else if key == b"fo:padding-bottom" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        cell.pad_bottom_pt = parse_odf_length_pt(v).unwrap_or(0.0);
                                    }
                                } else if key == b"fo:padding-left" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        cell.pad_left_pt = parse_odf_length_pt(v).unwrap_or(0.0);
                                    }
                                } else if key == b"fo:padding-right" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        cell.pad_right_pt = parse_odf_length_pt(v).unwrap_or(0.0);
                                    }
                                } else if key == b"style:vertical-align" {
                                    if let Ok(v) = std::str::from_utf8(&attr.value) {
                                        cell.valign = Some(v.to_string());
                                    }
                                }
                            }
                            self.cells.insert(n.to_string(), cell);
                        }
                    }
                }
                Ok(Event::End(e)) => {
                    let qname = e.name();
                    if qname.as_ref() == b"style:style" {
                        cur_family = None;
                        cur_name = None;
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }
    }
}

// ---------------------------------------------------------------------------
// 表格結構抽取（read_body_tables）
// ---------------------------------------------------------------------------

/// 走查 content.xml 抽取正文流二維表格模型。
pub fn read_body_tables(
    content_xml: &[u8],
    styles_xml: Option<&[u8]>,
) -> Result<Vec<OdfBodyTable>, OdfError> {
    // BUGGY 回塞鎖：若設有 ODF_TABLE_BUGGY 環境變數，退回不攔截表格邊界的舊態（返回空清單）。
    if std::env::var("ODF_TABLE_BUGGY").is_ok() {
        return Ok(Vec::new());
    }

    let mut style_book = TableStyleBook::default();
    if let Some(s_xml) = styles_xml {
        style_book.collect_from_xml(s_xml);
    }
    style_book.collect_from_xml(content_xml);

    let mut reader = Reader::from_reader(content_xml);
    let mut buf = Vec::new();

    let mut body_tables: Vec<OdfBodyTable> = Vec::new();
    let mut para_idx = 0usize;

    struct TableContext {
        para_start: usize,
        table: OdfTable,
        cur_row: Option<OdfTableRow>,
        cur_cell: Option<OdfTableCell>,
        in_header_rows: bool,
        active_v_spans: Vec<(usize, u16)>,
        col_cursor: usize,
    }

    let mut table_stack: Vec<TableContext> = Vec::new();
    let mut draw_stack: Vec<Vec<u8>> = Vec::new();
    let mut alt_skip_depth = 0usize;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let qname = e.name();
                let name = qname.as_ref();

                if alt_skip_depth > 0 {
                    alt_skip_depth += 1;
                    buf.clear();
                    continue;
                }
                if is_alt_text(name) && alt_text_is_metadata(&draw_stack) {
                    alt_skip_depth = 1;
                    buf.clear();
                    continue;
                }
                if is_draw_container(name) {
                    draw_stack.push(name.to_vec());
                }

                if name == b"table:table" {
                    let mut props = OdfTableProps::default();
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"table:style-name" {
                            if let Ok(s) = std::str::from_utf8(&attr.value) {
                                if let Some(p) = style_book.tables.get(s) {
                                    props = p.clone();
                                }
                            }
                        }
                    }
                    table_stack.push(TableContext {
                        para_start: para_idx,
                        table: OdfTable {
                            props,
                            grid: Vec::new(),
                            rows: Vec::new(),
                        },
                        cur_row: None,
                        cur_cell: None,
                        in_header_rows: false,
                        active_v_spans: Vec::new(),
                        col_cursor: 0,
                    });
                } else if name == b"table:table-column" {
                    if let Some(ctx) = table_stack.last_mut() {
                        let mut s_name = String::new();
                        let mut repeat = 1usize;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"table:style-name" {
                                s_name = String::from_utf8_lossy(&attr.value).to_string();
                            } else if attr.key.as_ref() == b"table:number-columns-repeated" {
                                if let Ok(s) = std::str::from_utf8(&attr.value) {
                                    if let Ok(n) = s.parse::<usize>() {
                                        repeat = n.clamp(1, 65535);
                                    }
                                }
                            }
                        }
                        let col_def =
                            style_book
                                .columns
                                .get(&s_name)
                                .cloned()
                                .unwrap_or(OdfGridCol {
                                    w_pt: None,
                                    raw: None,
                                });
                        for _ in 0..repeat {
                            ctx.table.grid.push(col_def.clone());
                        }
                    }
                } else if name == b"table:table-header-rows" {
                    if let Some(ctx) = table_stack.last_mut() {
                        ctx.in_header_rows = true;
                    }
                } else if name == b"table:table-row" {
                    if let Some(ctx) = table_stack.last_mut() {
                        let mut s_name = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"table:style-name" {
                                s_name = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        let mut r_props = style_book.rows.get(&s_name).cloned().unwrap_or_default();
                        if ctx.in_header_rows {
                            r_props.is_header = true;
                        }
                        ctx.cur_row = Some(OdfTableRow {
                            props: r_props,
                            cells: Vec::new(),
                        });
                        ctx.col_cursor = 0;
                    }
                } else if name == b"table:table-cell" {
                    if let Some(ctx) = table_stack.last_mut() {
                        if ctx.cur_row.is_some() {
                            let mut s_name = String::new();
                            let mut col_span = 1u16;
                            let mut row_span = 1usize;
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                if key == b"table:style-name" {
                                    s_name = String::from_utf8_lossy(&attr.value).to_string();
                                } else if key == b"table:number-columns-spanned" {
                                    if let Ok(s) = std::str::from_utf8(&attr.value) {
                                        if let Ok(n) = s.parse::<u16>() {
                                            col_span = n.max(1);
                                        }
                                    }
                                } else if key == b"table:number-rows-spanned" {
                                    if let Ok(s) = std::str::from_utf8(&attr.value) {
                                        if let Ok(n) = s.parse::<usize>() {
                                            row_span = n.max(1);
                                        }
                                    }
                                }
                            }
                            let mut c_props =
                                style_book
                                    .cells
                                    .get(&s_name)
                                    .cloned()
                                    .unwrap_or(OdfCellProps {
                                        grid_span: 1,
                                        ..Default::default()
                                    });
                            c_props.grid_span = col_span;
                            if row_span > 1 {
                                c_props.v_merge = Some(OdfCellMerge::Restart);
                                while ctx.active_v_spans.len() <= ctx.col_cursor {
                                    ctx.active_v_spans.push((0, 1));
                                }
                                ctx.active_v_spans[ctx.col_cursor] = (row_span - 1, col_span);
                            }
                            ctx.cur_cell = Some(OdfTableCell {
                                props: c_props,
                                para_range: (para_idx, para_idx),
                            });
                            ctx.col_cursor += col_span as usize;
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let qname = e.name();
                let name = qname.as_ref();

                if alt_skip_depth > 0 {
                    buf.clear();
                    continue;
                }
                if is_alt_text(name) && alt_text_is_metadata(&draw_stack) {
                    buf.clear();
                    continue;
                }

                if name == b"table:table-column" {
                    if let Some(ctx) = table_stack.last_mut() {
                        let mut s_name = String::new();
                        let mut repeat = 1usize;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"table:style-name" {
                                s_name = String::from_utf8_lossy(&attr.value).to_string();
                            } else if attr.key.as_ref() == b"table:number-columns-repeated" {
                                if let Ok(s) = std::str::from_utf8(&attr.value) {
                                    if let Ok(n) = s.parse::<usize>() {
                                        repeat = n.clamp(1, 65535);
                                    }
                                }
                            }
                        }
                        let col_def =
                            style_book
                                .columns
                                .get(&s_name)
                                .cloned()
                                .unwrap_or(OdfGridCol {
                                    w_pt: None,
                                    raw: None,
                                });
                        for _ in 0..repeat {
                            ctx.table.grid.push(col_def.clone());
                        }
                    }
                } else if name == b"table:table-cell" {
                    if let Some(ctx) = table_stack.last_mut() {
                        if ctx.cur_row.is_some() {
                            let mut s_name = String::new();
                            let mut col_span = 1u16;
                            let mut row_span = 1usize;
                            for attr in e.attributes().flatten() {
                                let key = attr.key.as_ref();
                                if key == b"table:style-name" {
                                    s_name = String::from_utf8_lossy(&attr.value).to_string();
                                } else if key == b"table:number-columns-spanned" {
                                    if let Ok(s) = std::str::from_utf8(&attr.value) {
                                        if let Ok(n) = s.parse::<u16>() {
                                            col_span = n.max(1);
                                        }
                                    }
                                } else if key == b"table:number-rows-spanned" {
                                    if let Ok(s) = std::str::from_utf8(&attr.value) {
                                        if let Ok(n) = s.parse::<usize>() {
                                            row_span = n.max(1);
                                        }
                                    }
                                }
                            }
                            let mut c_props =
                                style_book
                                    .cells
                                    .get(&s_name)
                                    .cloned()
                                    .unwrap_or(OdfCellProps {
                                        grid_span: 1,
                                        ..Default::default()
                                    });
                            c_props.grid_span = col_span;
                            if row_span > 1 {
                                c_props.v_merge = Some(OdfCellMerge::Restart);
                                while ctx.active_v_spans.len() <= ctx.col_cursor {
                                    ctx.active_v_spans.push((0, 1));
                                }
                                ctx.active_v_spans[ctx.col_cursor] = (row_span - 1, col_span);
                            }
                            let cell = OdfTableCell {
                                props: c_props,
                                para_range: (para_idx, para_idx),
                            };
                            ctx.col_cursor += col_span as usize;
                            if let Some(r) = ctx.cur_row.as_mut() {
                                r.cells.push(cell);
                            }
                        }
                    }
                } else if name == b"table:covered-table-cell" {
                    if let Some(ctx) = table_stack.last_mut() {
                        if ctx.cur_row.is_some() {
                            let has_v_merge = ctx
                                .active_v_spans
                                .get(ctx.col_cursor)
                                .is_some_and(|(rem, _)| *rem > 0);
                            if has_v_merge {
                                let (rem, span) = ctx.active_v_spans[ctx.col_cursor];
                                ctx.active_v_spans[ctx.col_cursor] = (rem - 1, span);
                                let cell = OdfTableCell {
                                    props: OdfCellProps {
                                        grid_span: span,
                                        v_merge: Some(OdfCellMerge::Continue),
                                        ..Default::default()
                                    },
                                    para_range: (para_idx, para_idx),
                                };
                                ctx.col_cursor += span as usize;
                                if let Some(r) = ctx.cur_row.as_mut() {
                                    r.cells.push(cell);
                                }
                            } else {
                                ctx.col_cursor += 1;
                            }
                        }
                    }
                } else if is_text_p_or_h(name) {
                    para_idx += 1;
                }
            }
            Ok(Event::End(e)) => {
                let qname = e.name();
                let name = qname.as_ref();

                if alt_skip_depth > 0 {
                    alt_skip_depth -= 1;
                    buf.clear();
                    continue;
                }
                if is_draw_container(name) {
                    draw_stack.pop();
                }

                if name == b"table:table-cell" {
                    if let Some(ctx) = table_stack.last_mut() {
                        if let Some(mut cell) = ctx.cur_cell.take() {
                            cell.para_range.1 = para_idx;
                            if let Some(r) = ctx.cur_row.as_mut() {
                                r.cells.push(cell);
                            }
                        }
                    }
                } else if name == b"table:table-row" {
                    if let Some(ctx) = table_stack.last_mut() {
                        if let Some(row) = ctx.cur_row.take() {
                            ctx.table.rows.push(row);
                        }
                    }
                } else if name == b"table:table-header-rows" {
                    if let Some(ctx) = table_stack.last_mut() {
                        ctx.in_header_rows = false;
                    }
                } else if name == b"table:table" {
                    if let Some(mut ctx) = table_stack.pop() {
                        if ctx.table.grid.is_empty() {
                            if let Some(r0) = ctx.table.rows.first() {
                                for c in &r0.cells {
                                    for _ in 0..c.props.grid_span {
                                        ctx.table.grid.push(OdfGridCol {
                                            w_pt: None,
                                            raw: None,
                                        });
                                    }
                                }
                            }
                        }
                        body_tables.push(OdfBodyTable {
                            para_start: ctx.para_start,
                            table: ctx.table,
                        });
                    }
                } else if is_text_p_or_h(name) {
                    para_idx += 1;
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(OdfError::Xml(err.to_string())),
            _ => {}
        }
        buf.clear();
    }

    body_tables.sort_by_key(|bt| bt.para_start);
    Ok(body_tables)
}

/// 由 `OdfBundle` 抽取正文流二維表格模型。
pub fn extract_tables(bundle: &OdfBundle) -> Result<Vec<OdfBodyTable>, OdfError> {
    bundle.ensure_not_encrypted("content.xml")?;
    let content = bundle.read_entry("content.xml")?;
    let styles = match bundle.read_entry("styles.xml") {
        Ok(b) => Some(b),
        Err(OdfError::MissingEntry { .. }) => None,
        Err(e) => return Err(e),
    };
    read_body_tables(&content, styles.as_deref())
}

#[inline]
fn is_text_p_or_h(qname: &[u8]) -> bool {
    qname == b"text:p" || qname == b"p" || qname == b"text:h" || qname == b"h"
}

// ---------------------------------------------------------------------------
// 表格模型對接 TableInput 與 TableSlot
// ---------------------------------------------------------------------------

/// 將 `OdfTable` 轉化為排版引擎輸入契約 `TableInput`。
pub fn odf_table_to_table_input(
    table: &OdfTable,
    projection: &OdfProjection,
    _p: &LayoutParams,
) -> TableInput {
    let mut grid_pt = Vec::new();
    for col in &table.grid {
        grid_pt.push(col.w_pt.unwrap_or(0.0));
    }
    let total_w = table.props.width_pt.or_else(|| {
        let sum: f32 = grid_pt.iter().sum();
        if sum > 0.0 {
            Some(sum)
        } else {
            None
        }
    });

    let mut rows = Vec::new();
    for r in &table.rows {
        let mut cells = Vec::new();
        for c in &r.cells {
            let mut blocks = Vec::new();
            for pi in c.para_range.0..c.para_range.1 {
                if let Some(para) = projection.paragraphs.get(pi) {
                    blocks.push(CellBlock::Text(para.text.clone()));
                }
            }
            if blocks.is_empty() {
                blocks.push(CellBlock::Text(String::new()));
            }

            let vmerge = match c.props.v_merge {
                Some(OdfCellMerge::Restart) => Some(true),
                Some(OdfCellMerge::Continue) => Some(false),
                None => None,
            };

            let borders = {
                let to_pen = |ob: &Option<OdfBorder>| -> Option<PenInput> {
                    ob.as_ref().map(|b| PenInput {
                        val: Some(if b.is_double {
                            "double".into()
                        } else {
                            "single".into()
                        }),
                        sz: Some(format!("{}", (b.width_pt * 8.0).round() as u32)),
                        color: Some(format!("{:06x}", b.color_rgb & 0xFFFFFF)),
                    })
                };
                Some(EdgeSetInput {
                    top: to_pen(&c.props.borders[0]),
                    left: to_pen(&c.props.borders[1]),
                    bottom: to_pen(&c.props.borders[2]),
                    right: to_pen(&c.props.borders[3]),
                    inside_h: None,
                    inside_v: None,
                })
            };

            let fill = c
                .props
                .shading_rgb
                .map(|rgb| format!("{:06x}", rgb & 0xFFFFFF));
            let margins = Some(CellMarginInput {
                top: Some(c.props.pad_top_pt),
                bottom: Some(c.props.pad_bottom_pt),
                left: Some(c.props.pad_left_pt),
                right: Some(c.props.pad_right_pt),
            });

            cells.push(CellInput {
                grid_span: c.props.grid_span as u32,
                vmerge,
                blocks,
                borders,
                fill,
                valign: c.props.valign.clone(),
                margins,
                ..Default::default()
            });
        }

        let (height_pt, height_exact) = if let Some(h) = r.props.exact_height_pt {
            (Some(h), true)
        } else if let Some(h) = r.props.min_height_pt {
            (Some(h), false)
        } else {
            (None, false)
        };

        rows.push(RowInput {
            cells,
            header: r.props.is_header,
            cant_split: r.props.cant_split,
            height_pt,
            height_exact,
            ..Default::default()
        });
    }

    TableInput {
        grid_pt,
        width: total_w.map(WidthSpec::Pt),
        indent_pt: table.props.indent_pt,
        justify: table.props.align.clone(),
        rows,
        ..Default::default()
    }
}

/// 將 `OdfBodyTable` 清單轉化為分頁排版槽位 `TableSlot` 清單。
pub fn build_table_slots(
    tables: &[OdfBodyTable],
    projection: &OdfProjection,
    p: &LayoutParams,
) -> Vec<TableSlot> {
    if tables.is_empty() {
        return Vec::new();
    }

    // 若存在巢狀/重疊表格，優先保留列數更多（更具體）的表格，確保傳給 layout 的槽位互不重疊且遞增
    let mut resolved_tables: Vec<&OdfBodyTable> = Vec::new();
    for bt in tables {
        let bt_end = bt
            .table
            .rows
            .iter()
            .flat_map(|r| &r.cells)
            .map(|c| c.para_range.1)
            .max()
            .unwrap_or(bt.para_start);

        let mut overlap_idx = None;
        for (i, chosen) in resolved_tables.iter().enumerate() {
            let ch_end = chosen
                .table
                .rows
                .iter()
                .flat_map(|r| &r.cells)
                .map(|c| c.para_range.1)
                .max()
                .unwrap_or(chosen.para_start);
            if bt.para_start < ch_end && chosen.para_start < bt_end {
                overlap_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = overlap_idx {
            if bt.table.rows.len() > resolved_tables[idx].table.rows.len() {
                resolved_tables[idx] = bt;
            }
        } else {
            resolved_tables.push(bt);
        }
    }

    resolved_tables.sort_by_key(|bt| bt.para_start);

    let mut slots = Vec::with_capacity(resolved_tables.len());
    let measurer = TextMeasurer::new(p.font_size_pt);
    let mut prev_end = 0usize;
    let n_paras = projection.paragraphs.len();

    for bt in resolved_tables {
        let input = odf_table_to_table_input(&bt.table, projection, p);
        let tl = layout_table(&input, p.content_w_pt(), p.line_h_pt(), &measurer, 0);
        let para_end = bt
            .table
            .rows
            .iter()
            .flat_map(|r| &r.cells)
            .map(|c| c.para_range.1)
            .max()
            .unwrap_or(bt.para_start);

        let start = bt.para_start.max(prev_end).min(n_paras);
        let end = para_end.max(start).min(n_paras);
        if end > start || (start == end && prev_end <= start) {
            slots.push(TableSlot {
                para_start: start,
                para_end: end,
                layout: tl,
            });
            prev_end = end;
        }
    }
    slots
}
