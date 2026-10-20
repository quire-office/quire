//! quire-engine —— shells 唯一准 import 的契約層（見 docs/雙殼架構規劃）。
//!
//! 規則（CI 鎖死，scripts/check-layering.sh）：
//! - shells/* 只能依賴本 crate；
//! - 本 crate 及其下全部 core 必須在 wasm32-unknown-unknown 可編譯；
//! - 型別全 serde 化，跨 wasm 邊界走 JSON 字串。

use quire_layout::{
    breaks_from_pages, CaretPos, LayoutEngine, LayoutParams, Page, ParaGrid, SectionGeometry,
    SectionSpec, TableSlot, TextMeasurer, MARGIN_LEFT,
};
use quire_ooxml::{
    diff_untouched, diff_zip_structure, parse, parse_header_footer_part, read_even_and_odd_headers,
    read_rels, read_sections_header_footer, read_sections_per_paragraph, resolve_relationship,
    LineSpacing, OoxmlDoc, OoxmlError, ParaStyle, SlotRefs, StyleTable, ZipDiff,
};
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

// shells 需要顯示列表型別但只能 import 本 crate：在此 re-export（分層鐵律的唯一合法通道）。
pub use quire_render_trait::{
    DrawItem, PageDisplayList, Renderer, FLAG_BOLD, FLAG_ITALIC, FLAG_STRIKE, FLAG_UNDERLINE,
};

// 表格 canonical model（Phase3-1a）：型別定義在 quire-ooxml（PM 補正 5），
// 本 crate 只做 pass-through re-export——3-1b 的 layout/render 以這裡
// 暴露的 `BodyTable`/`Block` 樹為輸入契約；無損寫回仍走 parts 明文，
// canonical 永不參與序列化（PM 補正 1）。
pub use quire_ooxml::{
    Block as CanonicalBlock, BodyTable, Border as CanonicalBorder, Borders as CanonicalBorders,
    CellProps as CanonicalCellProps, GridCol as CanonicalGridCol, Margins as CanonicalMargins,
    RowProps as CanonicalRowProps, Shading as CanonicalShading, Table as CanonicalTable,
    TableCell as CanonicalTableCell, TableProps as CanonicalTableProps,
    TableRow as CanonicalTableRow, TblWidth as CanonicalTblWidth,
};

// 表格映射層（Phase3-1b）：canonical 樹 → layout `TableInput`/`TableSlot`。
mod table_map;
pub use quire_layout::table::{Pen, RowFrag, TableFrag};
#[derive(Error, Debug)]
pub enum EngineError {
    #[error(transparent)]
    Ooxml(#[from] OoxmlError),
    #[error("zip 結構對照失敗: {0}")]
    Diff(#[from] quire_ooxml::ZipStructureError),
    /// XLSX 容器面（feature = xlsx）：不透明字串出口——shells 因此
    /// 不需要 import quire-xlsx 也能處理錯誤（分層鐵律）。
    #[error("XLSX 容器失敗: {0}")]
    Xlsx(String),
}

pub struct Engine {
    doc: OoxmlDoc,
    layout: LayoutEngine,
    params: LayoutParams,
    /// 樣式級聯的發射投影（t_5dfc8ac7）：`Some`＝run 級粗/斜/底/刪/色/字級
    /// 已換成級聯有效值（發射層與表格格內樣式一律讀本欄）；`None`＝文件
    /// 無 styles.xml 或全無宣告——讀本體 `doc.paragraphs`，既有輸出逐位元
    /// 組不變。純讀取派生：存檔路徑（save_docx／roundtrip）完全不經本欄。
    paras_eff: Option<Vec<quire_ooxml::Paragraph>>,
    /// 開啟時的原始位元組——roundtrip 對照的基準。
    original: Vec<u8>,
    /// 被編輯器動過的 part key（round-trip 白名單：這些 entry 允許內容變動）。
    touched: BTreeMap<String, ()>,
    /// 頁首頁尾發射層（t_b39f108b）：section id → 宣告（rId 已換算成 part
    /// 路徑，跨節繼承已套用）；`hf_parts` 為 part 路徑 → 已解析投影。
    /// 純讀取派生資料——存檔路徑完全不經此層（round-trip 零影響）。
    hf: HashMap<u32, SectionHf>,
    hf_parts: HashMap<String, HfPart>,
    /// `settings.xml` 的 `evenAndOddHeaders`（文件級奇偶頁開關）。
    even_odd: bool,
    /// section id → 頁面幾何（取首見值；頁首頁尾排版寬度與欄位換算用）。
    sec_geo: HashMap<u32, SectionGeometry>,
}

/// 頁首／頁尾 part 的已解析投影（開啟文件時一次性解析；純讀取）。
/// 段落與表格走與 document.xml 同一組解析器（同 w:p Start 口徑、同
/// mc:Fallback 吞沒）；索引只在 part 內自洽。
/// `styles`＝級聯烘焙後的發射投影（t_5dfc8ac7；無 styles.xml＝解析原樣）；
/// `metrics`＝逐段 (斷行字級, 段行高 pt)。
#[derive(Debug)]
struct HfPart {
    styles: Vec<quire_ooxml::Paragraph>,
    metrics: Vec<(f32, f32)>,
    tables: Vec<quire_ooxml::BodyTable>,
}

/// 一節的頁首頁尾發射資料：宣告（已繼承、rId 已換算為「確實在 parts 裡」
/// 的 part 路徑，缺件＝None 容錯跳過）＋發射距離＋titlePg 開關。
#[derive(Debug, Default)]
struct SectionHf {
    /// 頁首發射 y＝本值（pt，自頁頂；ECMA-376 pgMar/@w:header）。
    header_dist_pt: f32,
    /// 頁尾發射 y＝頁高−本值（pt；ECMA-376 pgMar/@w:footer）。
    footer_dist_pt: f32,
    title_pg: bool,
    header: SlotPaths,
    footer: SlotPaths,
}

/// 已解析的三 slot part 路徑（None＝未宣告或指向不存在 part）。
#[derive(Debug, Default)]
struct SlotPaths {
    default: Option<String>,
    first: Option<String>,
    even: Option<String>,
}

impl Engine {
    /// 位元組進（wasm 無 fs；native 自己讀檔再傳位元組進來）。
    ///
    /// 頁面幾何不寫死：逐 section 解析 `sectPr/pgSz`／`pgMar` 餵進版面層
    /// （Phase0-2b）。檔案毫無 sectPr 時退回 `LayoutParams::default()`（後備）。
    pub fn from_docx_bytes(bytes: &[u8]) -> Result<Self, EngineError> {
        let doc = parse(bytes)?;
        // ---- 樣式級聯層（t_5dfc8ac7）----
        // 開啟文件時一次性解析 styles.xml：逐段級聯合成的 (斷行字級, 段行高)
        // 網格餵 `ParaGrid`，run 級有效樣式烘焙進發射用投影 `paras_eff`（見
        // 欄位注）。純讀取派生資料——`doc` 本體一個欄位不改，存檔路徑完全不
        // 經此層（parts passthrough，round-trip 零影響）。
        // 無 styles.xml part 時仍給一張空表而非 None：級聯層除了套用樣式，
        // 還承載「超連結合成預設」的末端兜底（`RunStyle::with_hyperlink_default`）；
        // 若此處短路，無 styles.xml 的檔案連結會失去預設藍／底線。空表一切
        // 落後備值，與舊 None 路徑逐欄等價。
        let style_table = Some(
            doc.parts
                .get("word/styles.xml")
                .map(|s| StyleTable::parse(s.as_slice()))
                .unwrap_or_else(|| StyleTable::parse(&[])),
        );
        let (doc_size, doc_line) = style_table.as_ref().map_or((None, None), |t| t.doc_grid());
        // 文件基準字級：docDefaults∪Normal 鏈的有效 sz（ECMA 無宣告時的
        // Word 預設 12pt 兜底）。無級聯層＝12（既有硬編碼後備）。
        let def_size = doc_size.unwrap_or(12.0);
        // 文件帶尺（行網格量化單位）＝預設段落的行高；無宣告＝12pt×1.4，
        // 與舊 `LayoutParams::default()` 恆等（零宣告檔案逐位元組不變）。
        let grid_line_h = resolve_line_h_pt(def_size, doc_line);
        let n_paras = doc.paragraphs.len();
        let sections: Vec<SectionSpec> = doc
            .parts
            .get("word/document.xml")
            .map(|xml| {
                read_sections_per_paragraph(xml, n_paras)
                    .into_iter()
                    .map(|s| SectionSpec {
                        id: s.id,
                        geo: SectionGeometry {
                            page_w_pt: s.geo.page_w_pt,
                            page_h_pt: s.geo.page_h_pt,
                            margin_top_pt: s.geo.margin_top_pt,
                            margin_bottom_pt: s.geo.margin_bottom_pt,
                            margin_left_pt: s.geo.margin_left_pt,
                            margin_right_pt: s.geo.margin_right_pt,
                            landscape: s.geo.landscape,
                            continuous: s.geo.continuous,
                        },
                    })
                    .collect()
            })
            .unwrap_or_default();
        // 基準 params：字級/行距吃級聯文件基準（無級聯＝12pt×1.4 後備不變），
        // 再覆寫首 section 的頁面欄位，讓「無 section 段落」與 shells 讀
        // params() 時也拿到文件實際尺寸。
        let params = LayoutParams {
            font_size_pt: def_size,
            line_spacing: if def_size > 0.0 {
                grid_line_h / def_size
            } else {
                1.4
            },
            ..LayoutParams::default()
        };
        let params = match sections.first() {
            Some(s) => LayoutParams::from_section(&s.geo, params.font_size_pt, params.line_spacing),
            None => params,
        };
        // 級聯烘焙（需 params 已定：製表位點引填補按該段可用寬計算）：
        // 逐段可用寬＝該段 section 的 content_w（無 section 表＝基準 params）。
        let widths: Vec<f32> = (0..n_paras)
            .map(|i| match sections.get(i) {
                Some(s) => {
                    LayoutParams::from_section(&s.geo, params.font_size_pt, params.line_spacing)
                        .content_w_pt()
                }
                None => params.content_w_pt(),
            })
            .collect();
        let mut grid_size_v: Vec<f32> = Vec::with_capacity(n_paras);
        let mut grid_bands_v: Vec<u32> = Vec::with_capacity(n_paras);
        // 顯式分頁旗標（parser 直讀，任何檔案都收；全 false＝舊行為）。
        let grid_break_v: Vec<bool> = doc.paragraphs.iter().map(|p| p.page_break).collect();
        // 發射投影：Some＝級聯有作用（run 樣式欄位已烘焙）；None＝發射層直讀
        // `doc.paragraphs`（零級聯檔案逐位元組舊行為的快路徑）。
        let mut paras_eff: Option<Vec<quire_ooxml::Paragraph>> = None;
        if let Some(st) = style_table.as_ref() {
            let (eff_all, metrics) = cascade_paragraphs(st, &doc.paragraphs, def_size, &widths);
            grid_size_v = metrics.iter().map(|m| m.0).collect();
            // 段行高量化到文件帶尺（至少 1 帶；帶尺＝12pt×1.4 且段行高不大於
            // 帶尺時恆 1＝舊行為）。
            grid_bands_v = metrics
                .iter()
                // 帶量化鉗上限（t_c1fb4c69 S1-4）：垃圾宣告行高不得量化出
                // 天量帶數（正常段行高 ≤ 數帶，永不觸頂）。
                .map(|m| {
                    let b = (m.1 / grid_line_h).ceil();
                    if !b.is_finite() {
                        quire_layout::MAX_PAGE_BANDS
                    } else {
                        ((b.max(1.0)) as u32).min(quire_layout::MAX_PAGE_BANDS)
                    }
                })
                .collect();
            paras_eff = Some(eff_all);
        }
        // 版面文字：以發射投影為準（級聯路徑含點引填補——斷行與發射同源，
        // run↔line 碼點對齊不破）。無級聯＝原始串接（舊行為）。
        let texts: Vec<String> = match paras_eff.as_ref() {
            Some(eff) => eff
                .iter()
                .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
                .collect(),
            None => doc.paragraph_texts(),
        };
        // 表格槽（Phase3-1b）：canonical 樹＋styles.xml 框線查表 → layout 幾何。
        // 圖片投影（t_a1397522）：純讀取（PM 補正 1）——格內圖注入 CellBlock，
        // 正文流圖走 ImageBand，浮動鎖點圖附掛錨定頁。
        let styles = doc.parts.get("word/styles.xml").map(|s| s.as_slice());
        let images = doc
            .parts
            .get("word/document.xml")
            .map(|xml| quire_ooxml::read_images(xml))
            .unwrap_or_default();
        let eff_refs: &[quire_ooxml::Paragraph] = paras_eff.as_deref().unwrap_or(&doc.paragraphs);
        let slots = table_map::build_slots(
            &doc.tables,
            &texts,
            styles,
            &params,
            &sections,
            &images,
            eff_refs,
        );
        let bands = table_map::build_images(&images, params.line_h_pt(), &params, &sections);
        // br-page 硬斷點（ECMA-376 §17.4.3）：canonical 段落的 w:br w:type="page"
        // 偏移直傳版面層（與 texts 逐段同標）。無 br 檔案＝全空清單＝舊行為。
        let breaks: Vec<Vec<usize>> = doc
            .paragraphs
            .iter()
            .map(|p| p.page_breaks.clone())
            .collect();
        // 樣式級聯網格（t_5dfc8ac7）：逐段字級/行帶＋pageBreakBefore 旗標。
        let grid = ParaGrid {
            size_pt: grid_size_v,
            bands: grid_bands_v,
            break_before: grid_break_v,
        };
        let layout = LayoutEngine::with_breaks_grid(
            &texts,
            params,
            sections.clone(),
            slots,
            bands,
            breaks,
            grid,
        );
        // 頁首頁尾層（t_b39f108b）：sectPr 宣告＋rels 查表＋part 解析。
        // 全鏈純讀取派生——parts／bundle 一個位元組都不碰（round-trip 零影響）。
        let (hf, hf_parts, even_odd, sec_geo) =
            build_hf_layer(&doc, &sections, style_table.as_ref());
        Ok(Self {
            doc,
            layout,
            params,
            paras_eff,
            original: bytes.to_vec(),
            touched: BTreeMap::new(),
            hf,
            hf_parts,
            even_odd,
            sec_geo,
        })
    }

    pub fn page_count(&self) -> usize {
        self.layout.page_count()
    }

    /// 發射用段落投影（t_5dfc8ac7）：級聯有作用時回烘焙版（run 的粗/斜/底/
    /// 刪/色/字級＝級聯有效值）；否則回本體（零級聯檔案逐位元組舊行為）。
    fn eff_paragraphs(&self) -> &[quire_ooxml::Paragraph] {
        self.paras_eff.as_deref().unwrap_or(&self.doc.paragraphs)
    }

    pub fn paragraph_count(&self) -> usize {
        self.doc.paragraphs.len()
    }

    /// lossless 主張的最低保證：所有 zip part 都還在。
    pub fn preserved_part_count(&self) -> usize {
        self.doc.parts.len()
    }

    /// body 級表格數（canonical 投影，Phase3-1a）。只讀——本數不參與存檔，
    /// 無損寫回來源仍是 parts 明文（PM 補正 1）。
    pub fn table_count(&self) -> usize {
        self.doc.tables.len()
    }

    /// body 級表格樹 JSON（含巢狀；wasm/JS 邊界序列化入口）。
    pub fn tables_json(&self) -> String {
        serde_json::to_string(&self.doc.tables).unwrap_or_else(|_| "[]".to_string())
    }

    pub fn page_lines(&self, page: usize) -> Option<Vec<String>> {
        self.layout
            .pages()
            .get(page)
            .map(|p| p.lines.iter().map(|l| l.text.clone()).collect())
    }

    /// 段落純文字（Web-1 隱形錨點游標插入讀目前段落用）。
    pub fn paragraph_text(&self, para: usize) -> Option<String> {
        self.layout.paragraph_text(para).map(str::to_owned)
    }

    /// caret (段落, 碼點) → 頁面矩形 JSON（JS 疊畫 caret、定位隱形錨點）。
    pub fn caret_rect_json(&self, para: usize, cp: usize) -> Option<String> {
        self.layout
            .caret_rect(CaretPos { para, cp })
            .and_then(|r| serde_json::to_string(&r).ok())
    }

    /// 頁面點擊 (page, x_pt, y_pt) → caret JSON（canvas→輸入端反向契約）。
    pub fn caret_at_json(&self, page: usize, x_pt: f32, y_pt: f32) -> Option<String> {
        self.layout
            .caret_at(page, x_pt, y_pt)
            .and_then(|c| serde_json::to_string(&c).ok())
    }

    /// 在 caret 處插入文字（IME commit 的引擎端入口）。回傳失效事件 JSON。
    /// `cp` 以碼點計；位元組邊界換算在引擎做——JS 端不碰 UTF-8 切口。
    pub fn insert_text_at_caret(&mut self, para: usize, cp: usize, text: &str) -> Vec<String> {
        let Some(cur) = self.layout.paragraph_text(para).map(str::to_owned) else {
            return Vec::new();
        };
        // 碼點 → 位元組偏移（不可在 char 邊界外切：UTF-8 安全）。
        let byte = cur
            .char_indices()
            .nth(cp)
            .map(|(i, _)| i)
            .unwrap_or(cur.len());
        let mut next = String::with_capacity(cur.len() + text.len());
        next.push_str(&cur[..byte]);
        next.push_str(text);
        next.push_str(&cur[byte..]);
        self.replace_paragraph_text(para, &next)
    }

    pub fn page_display_list(&self, page: usize) -> Option<PageDisplayList> {
        let p: &Page = self.layout.pages().get(page)?;
        let line_h = self.params.line_h_pt();
        let size = self.params.font_size_pt;
        let mut items: Vec<DrawItem> = Vec::new();
        let paras = self.eff_paragraphs();
        // 頁首頁尾先發射（填色/框線在文字之前，與表格同一約定）——其文字項
        // 由下方統一幾何排序落到閱讀序應有位置（頁首 y 小排最前、頁尾最後）。
        self.push_header_footer_items(&mut items, p);
        // 底層先畫表格（填色＋框線＋格內文字），再疊正文行；最後對「文字項」
        // 依 (y, x) 幾何閱讀序重排——跨表格/正文統一由上而下、由左而右。
        // 填色與框線保持推入序（穩定排序 key 同鍵，不互相顛倒遮蓋次序）。
        for tp in &p.tables {
            let canon_tbl = self.doc.tables.get(tp.slot).map(|bt| &bt.table);
            push_frag_items(
                &mut items,
                tp.frag.x_pt,
                tp.frag.y_pt,
                &tp.frag,
                size,
                canon_tbl,
                paras,
            );
        }
        // 圖片佔位（t_a1397522）：正文流圖＝版心左緣＋行網格帶 y；浮動鎖點
        // 圖＝layout 已按各軸 relativeFrom 原點系換算成頁絕對座標，直接發射。
        for im in &p.images {
            let y = p.margin_top_pt + im.band as f32 * line_h;
            emit_image_placeholder(&mut items, p.margin_left_pt, y, im.w_pt, im.h_pt);
        }
        for fm in &p.floating_images {
            emit_image_placeholder(&mut items, fm.x_pt, fm.y_pt, fm.w_pt, fm.h_pt);
        }
        for (i, l) in p.lines.iter().enumerate() {
            let band = p.line_bands.get(i).copied().unwrap_or(i as u32) as f32;
            let y = p.margin_top_pt + band * line_h;
            let para = paras.get(l.para_index);
            emit_line_run_items(&mut items, p.margin_left_pt, y, size, l, para);
        }
        // 穩定排序：非文字項（填色/框線）同鍵保持原序且恆在前；文字項依基線 y
        // 再依 x 排列，讓「正文→表格→正文」的閱讀序與 PDF 抽取端一致。
        items.sort_by(|a, b| {
            let key = |it: &DrawItem| match it {
                DrawItem::Text { x, y, .. } => {
                    (1i8, (*y * 4.0).round() as i64, (*x * 4.0).round() as i64)
                }
                _ => (0i8, 0, 0),
            };
            key(a).cmp(&key(b))
        });
        Some(PageDisplayList {
            page,
            width_pt: p.width_pt,
            height_pt: p.height_pt,
            items,
        })
    }

    /// 分頁斷點清單（JSON）：每頁段落分佈＋跨頁續排標記＋逐頁實際尺寸，
    /// render-lab 基線比對用。
    pub fn page_breaks_json(&self) -> String {
        // 直接拿 LayoutEngine 的實際頁（含表格佔帶）——3-1b 起分頁斷點
        // 必須反映表格擠壓後的位置，不再是「假裝只有文字流」。
        let brks = breaks_from_pages(self.layout.pages(), &self.params);
        serde_json::to_string_pretty(&brks).unwrap_or_else(|_| "[]".to_string())
    }

    /// 跨 wasm 邊界的序列化入口。
    pub fn page_display_list_json(&self, page: usize) -> Option<String> {
        self.page_display_list(page)
            .and_then(|dl| serde_json::to_string(&dl).ok())
    }

    /// 回傳失效事件（序列化），供 shells 做增量重繪。
    pub fn replace_paragraph_text(&mut self, para: usize, text: &str) -> Vec<String> {
        let evs = self.layout.replace_paragraph_text(para, text);
        // paras_eff 同步（t_5b391cfd S4）：發射層讀的是投影不是 doc 本體——
        // 只改 layout.texts 的話，`eff_paragraphs()` 發射時仍以**舊 run 碼點
        // 區間**套新文字，編輯後段的 flags/顏色錯映；級聯 leader 段的填充
        // run 也會在下次發射時無痕消失（發射與斷行兩條來源分岔）。
        // 契約＝段落級整段置換（shells/web 傳入的是 layout 顯示全文，含
        // 填充；crdt 落地同一路徑）。新文字寫進首個有字 run、其餘清空：
        // run 區間和＝段長＝texts 段長，發射↔斷行碼點對齊天然成立；樣式
        // 取首個有字 run（多段樣式混排的整段置換是骨架契約外的近似——
        // run 級穿透編輯面屬後續卡，此處只求「不錯映、投影不陳舊」）。
        if let Some(eff) = self.paras_eff.as_mut() {
            if let Some(p) = eff.get_mut(para) {
                let mut first = true;
                for r in &mut p.runs {
                    if r.text.is_empty() {
                        continue; // 空 run（如欄位標記殘影）不動。
                    }
                    if first {
                        r.text = text.to_string();
                        // 編輯落地＝使用者字面文字：欄位標記撤除，否則發射層
                        // 會把頁碼覆寫回使用者剛打的字（欄位快取語義已死）。
                        r.field = None;
                        first = false;
                    } else {
                        r.text = String::new();
                    }
                }
                if first {
                    // 原段全空：文字掛進首個 run 保留其樣式；無 run 才新掛。
                    match p.runs.first_mut() {
                        Some(r) => r.text = text.to_string(),
                        None => p.runs.push(quire_ooxml::Run {
                            text: text.to_string(),
                            ..Default::default()
                        }),
                    }
                }
            }
        }
        evs.into_iter().map(|ev| format!("{ev:?}")).collect()
    }

    // ---------------- Phase0-6：增量重排的控制面（shells 唯一入口） ----------------

    /// 預算制軟分頁開關（`Some(n)`＝同步重排最多新增 n 頁，尾段留給
    /// `reflow_step_json` 於空檔續排；`None`＝全速，編輯後版面即定案）。
    pub fn set_sync_budget(&mut self, pages: Option<usize>) {
        self.layout.set_sync_budget(pages);
    }

    /// 版面是否已定案（無待排尾段）。
    pub fn is_settled(&self) -> bool {
        self.layout.is_settled()
    }

    /// 背景續排一步：`{"settled":bool,"events":[...]}` JSON。shell 在收到
    /// `ReflowPending` 後於 requestIdleCallback/空檔重複呼叫直到 settled。
    pub fn reflow_step_json(&mut self) -> String {
        let (evs, settled) = self.layout.reflow_step();
        let events: Vec<String> = evs.into_iter().map(|ev| format!("{ev:?}")).collect();
        serde_json::to_string(&serde_json::json!({ "settled": settled, "events": events }))
            .unwrap_or_else(|_| "{\"settled\":true,\"events\":[]}".to_string())
    }

    // ---------------- 無損存檔 / round-trip 驗證 ----------------

    /// 存檔：未觸碰 part 逐位元組沿用（見 quire-ooxml 文件）。
    pub fn save_docx(&self) -> Result<Vec<u8>, EngineError> {
        Ok(self.doc.save()?)
    }

    /// 開啟→（編輯）→存檔後，與原始位元組做結構級對照。
    /// 已編輯過的 part 自動進白名單；其餘 entry 必須零失真。
    pub fn roundtrip_check(&self) -> Result<ZipDiff, EngineError> {
        let saved = self.save_docx()?;
        if self.touched.is_empty() {
            Ok(diff_untouched(&self.original, &saved)?)
        } else {
            Ok(diff_zip_structure(&self.original, &saved, &self.touched)?)
        }
    }

    /// 編輯 part 明文（標記白名單）。
    pub fn set_part(&mut self, name: &str, bytes: Vec<u8>) {
        self.touched.insert(name.to_string(), ());
        self.doc.set_part(name, bytes);
    }

    /// 離線對照：原始位元組 vs 存回位元組，`allow` 為允許內容變動的 part 白名單。
    /// CLI 的 roundtrip --check 走這裡（shells 只能經 engine，不直接 import ooxml）。
    pub fn roundtrip_diff(
        original: &[u8],
        saved: &[u8],
        allow: &[String],
    ) -> Result<ZipDiff, EngineError> {
        let touched: BTreeMap<String, ()> = allow.iter().map(|k| (k.clone(), ())).collect();
        Ok(diff_zip_structure(original, saved, &touched)?)
    }
}

// ---------------- XLSX 容器面（feature = xlsx）----------------
//
// 拆包紀律同 collab：主 wasm 不帶此 feature（首包體積＋攻擊面），XLSX 程式碼
// 只由 shells/native（quire-cli）經 `quire-engine/xlsx` 鏈入。分層閘門
// （scripts/check-layering.sh）以 cargo tree 守住 quire-web 不得直接或傳遞
// 依賴 quire-xlsx。
//
// X1 卡範圍＝容器級無損 round-trip ＋最小 workbook 投影（只讀）。型別经本
// crate pass-through re-export（分層鐵律：shells 只准 import engine）。

/// XLSX/XLSM 容器句柄（zip 結構快照＋只讀 part 索引）。
#[cfg(feature = "xlsx")]
pub struct XlsxDoc {
    bundle: quire_xlsx::XlsxBundle,
    /// 開啟時的原始位元組——round-trip 對照基準。
    original: Vec<u8>,
}

#[cfg(feature = "xlsx")]
impl XlsxDoc {
    /// 由 .xlsx/.xlsm 位元組開啟（不碰檔案系統；缺 [Content_Types].xml → Err）。
    pub fn from_xlsx_bytes(bytes: &[u8]) -> Result<Self, EngineError> {
        let bundle = quire_xlsx::XlsxBundle::open(bytes)?;
        Ok(Self {
            bundle,
            original: bytes.to_vec(),
        })
    }

    /// 存回：本卡不改任何 part → 未觸碰 entry 原樣搬運。
    pub fn save_xlsx(&self) -> Result<Vec<u8>, EngineError> {
        Ok(self.bundle.save()?)
    }

    pub fn part_names(&self) -> Vec<String> {
        self.bundle.part_names()
    }

    pub fn part_count(&self) -> usize {
        self.bundle.part_count()
    }

    pub fn part(&self, name: &str) -> Option<Vec<u8>> {
        self.bundle.part(name)
    }

    /// workbook 最小模型（純讀取投影；本方法不失敗，病態輸入落空集合）。
    pub fn workbook_model_json(&self) -> String {
        serde_json::to_string(&self.bundle.workbook_model()).unwrap_or_else(|_| "{}".to_string())
    }

    /// sheet 數（投影；模型層容錯後的誠實數字）。
    pub fn sheet_count(&self) -> usize {
        self.bundle.workbook_model().sheets.len()
    }

    /// 未觸碰存檔是否 byte-identical（CLI 閘門的直接斷言）。
    pub fn untouched_is_byte_identical(&self) -> Result<bool, EngineError> {
        Ok(self.save_xlsx()? == self.original)
    }
}

/// XLSX 離線對照：原始位元組 vs 存回位元組的 zip 結構級 diff（CLI --check 用）。
/// 復用 ooxml 的 `diff_untouched`——zip 結構層本就是格式無關的。
#[cfg(feature = "xlsx")]
pub fn xlsx_roundtrip_diff(
    original: &[u8],
    saved: &[u8],
) -> Result<quire_ooxml::ZipDiff, EngineError> {
    Ok(quire_ooxml::diff_untouched(original, saved)?)
}

#[cfg(feature = "xlsx")]
impl From<quire_xlsx::XlsxError> for EngineError {
    fn from(e: quire_xlsx::XlsxError) -> Self {
        EngineError::Xlsx(e.to_string())
    }
}

// ---------------- 協作面（feature = collab）----------------
//
// 拆包鐵律：本區塊只在 `collab` feature 下編譯。主 wasm（shells/web）不帶此
// feature，CRDT 程式碼物理上不在首包裡；只有 shells/web-collab（延遲載入的
// 第二個 wasm）用 `quire-engine/collab` 把它鏈進去。CI 以 cargo tree 守住
// quire-web 不得（直接或傳遞）依賴 quire-crdt。
//
// 協作會話刻意與 Engine 分離：第二個 wasm 模組不能與主 wasm 共享記憶體，
// 若把會話掛在 Engine 上就得在主包外重開一份文件。會話只碰操作序列，
// 落地（套用到主引擎的編輯面）由 JS 端橋接。

/// 協作會話句柄（佔位 in-memory 後端；loro 落地後換實作、契約不變）。
#[cfg(feature = "collab")]
pub struct CollabSession {
    backend: quire_crdt::InMemoryBackend,
}

#[cfg(feature = "collab")]
use quire_crdt::CrdtBackend as _;

#[cfg(feature = "collab")]
impl CollabSession {
    pub fn new(session: u64) -> Self {
        Self {
            backend: quire_crdt::InMemoryBackend::new(session),
        }
    }

    /// 本地編輯提交 → 回傳要廣播給協作者的位元組（JSON 字串）。
    pub fn commit_local(&mut self, para: usize, run: usize, text: &str) -> String {
        let bytes = self
            .backend
            .commit_local(&quire_crdt::EditOp::ReplaceRunText {
                para,
                run,
                text: text.to_string(),
            });
        String::from_utf8(bytes).unwrap_or_default()
    }

    /// 收到協作者廣播（JSON 字串）→ 回傳需落地到編輯面的操作 JSON 陣列，
    /// 由呼叫端（JS 橋）套回主引擎。
    pub fn apply_remote(&mut self, msg: &str) -> Vec<String> {
        self.backend
            .apply_remote(msg.as_bytes())
            .into_iter()
            .map(|op| serde_json::to_string(&op).unwrap_or_default())
            .collect()
    }
}

#[cfg(all(feature = "collab", test))]
mod collab_tests {
    use super::*;

    #[test]
    fn commit_and_remote_apply_roundtrip() {
        let mut host = CollabSession::new(1);
        let msg = host.commit_local(0, 0, "協作文字");
        let mut peer = CollabSession::new(2);
        let landed = peer.apply_remote(&msg);
        assert_eq!(landed.len(), 1, "遠端操作應落地到編輯面");
        assert!(landed[0].contains("ReplaceRunText"));
        assert!(landed[0].contains("協作文字"));
    }

    #[test]
    fn garbage_message_lands_nothing() {
        let mut peer = CollabSession::new(2);
        assert!(peer.apply_remote("不是 JSON").is_empty());
    }
}

/// 預設文字顏色：不透明黑（0xFF000000）。
const DEFAULT_TEXT_COLOR_ARGB: u32 = 0xFF00_0000;

/// 發射字級後備（無任何級聯宣告時的既有硬編碼尺；`LayoutParams::default()`
/// 的 12pt 同源，只準有一處定義）。
const FALLBACK_SIZE_PT: f32 = 12.0;

/// [`LineSpacing`] → 段行高（pt）。本引擎行高無內容自適應成分，三態換算：
/// - `Auto(m)`：字級×倍數（240＝單倍；與 fallback 12pt×1.4 同口徑）；
/// - `Exact(v)`：固定 v pt（文字超出亦不撐高）；
/// - `AtLeast(v)`：v 與「字級×單倍」取大（ECMA 語義：至少 v，行不低於字面高）；
/// - `None`：字級×1.4（既有硬編碼後備，零宣告檔案逐位元組不變）。
fn resolve_line_h_pt(size_pt: f32, line: Option<LineSpacing>) -> f32 {
    let s = size_pt.max(1.0);
    let h = match line {
        Some(LineSpacing::Auto(m)) => s * m.max(0.1),
        Some(LineSpacing::Exact(v)) => v,
        Some(LineSpacing::AtLeast(v)) => v.max(s),
        None => s * 1.4,
    };
    h.max(1.0)
}

/// 逐段級聯＋烘焙（t_5dfc8ac7；正文與頁首頁尾 part 共用同一條路徑）。
/// 回傳 (發射投影——run 粗/斜/底/刪/色/字級換成級聯有效值, 逐段
/// (斷行量測字級, 段行高 pt))。`def_size`＝文件基準字級（級聯全無宣告時）。
/// `widths`＝逐段可用寬（pt）：製表位點引填補的計算尺；短/空＝該段不展開
/// （無錨點不虛構——頁首尾 part 走空表＝tab 維持原樣，既有行為不變）。
fn cascade_paragraphs(
    st: &StyleTable,
    paras: &[quire_ooxml::Paragraph],
    def_size: f32,
    widths: &[f32],
) -> (Vec<quire_ooxml::Paragraph>, Vec<(f32, f32)>) {
    let mut eff_all: Vec<quire_ooxml::Paragraph> = Vec::with_capacity(paras.len());
    let mut metrics: Vec<(f32, f32)> = Vec::with_capacity(paras.len());
    for (pi, p) in paras.iter().enumerate() {
        let inline = ParaStyle {
            run: p.prun.unwrap_or_default(),
            line: p.line,
            align: p.jc,
        };
        let (base, para) = st.effective_para(p.style.as_deref(), inline);
        let mut meas_size = base.size_pt.unwrap_or(def_size);
        let mut eff_runs: Vec<quire_ooxml::Run> = Vec::with_capacity(p.runs.len());
        for r in &p.runs {
            let mut b = base;
            if let Some(cid) = r.rstyle.as_deref() {
                b = StyleTable::resolve_run(b, st.char_chain(cid).unwrap_or_default());
            }
            // 級聯末端兜底：只有整條鏈（直接格式／字元樣式／段落樣式／
            // docDefaults）都沒宣告顏色、底線時，超連結 run 才補合成預設。
            let eff = StyleTable::resolve_run(b, r.direct.unwrap_or_default())
                .with_hyperlink_default(r.hyperlink_default);
            // 斷行字級＝段內 run 有效字級最大值（Word 行高/斷行依段內最大
            // 字級成形；本引擎斷行用單一尺，取大者保守不切字）。
            if let Some(sz) = eff.size_pt {
                meas_size = meas_size.max(sz);
            }
            let mut r2 = r.clone();
            // 三態 None＝全鏈未宣告＝未開（與舊「rPr 不見即 false」等價——
            // 舊 bool 欄位本就是 rPr 的有損投影）。
            r2.bold = eff.bold.unwrap_or(false);
            r2.italic = eff.italic.unwrap_or(false);
            r2.underline = eff.underline.unwrap_or(false);
            r2.strike = eff.strike.unwrap_or(false);
            if eff.color_set {
                // 宣告 w:color（含 auto/非法→明確黑）以級聯為準；
                // 未宣告不動——保留解析期代擬色（hyperlink 藍）。
                r2.color = eff.color;
            }
            r2.size_pt = eff.size_pt;
            eff_runs.push(r2);
        }
        let line_h = resolve_line_h_pt(base.size_pt.unwrap_or(def_size), para.line);
        // 點引 leads 展開（t_5dfc8ac7）：段落宣告右對齊製表位＋leader＋內含
        // w:tab run 時，於首個 tab run 前插入填充符串至「製表位 − 尾串寬」
        // （Word/LO 同款視覺 `Heading 1 ......... 2`）。填充是獨立 run——
        // texts 由 eff run 串接重建，run↔line 碼點對齊天然不破。斷行量測
        // 尺＝meas_size（與 break_lines 同尺，填充寬計算與斷行一致）。
        // widths 缺位（頁首尾 part／畸形檔）＝不展開——無可用寬不虛構。
        // page_breaks 非空＝不展開（t_5b391cfd S3 案 a）：breaks 直讀原始
        // 文字碼點偏移（from_docx_bytes），填充讓 texts 碼點軸右移，斷點
        // 位置即與文字錯位（填充越長錯越多）。同段共現 corpus 0/400 但
        // web 編輯器打「標題....頁碼」＋插分頁就踩。誠實退化：寧可少畫
        // 點引，不可錯斷頁（與「無錨點不虛構」同一取徑）。
        let leader_ch = match p.tab_leader.as_deref() {
            Some("dot") => Some('.'),
            Some("underscore") => Some('_'),
            Some("hyphen") => Some('-'),
            _ => None,
        };
        // 顯式分頁段：取消導引字元＝整段跳過填充展開（見上注）。
        let leader_fill = if p.page_breaks.is_empty() {
            leader_ch
        } else {
            None
        };
        if let (Some(pos), Some(ch)) = (p.tab_right_pt, leader_fill) {
            if let Some(ti) = eff_runs.iter().position(|r| r.tab) {
                if widths.len() > pi {
                    let m = TextMeasurer::new(meas_size);
                    let prefix: String = eff_runs[..ti].iter().map(|r| r.text.as_str()).collect();
                    let suffix: String = eff_runs[ti..].iter().map(|r| r.text.as_str()).collect();
                    let dot_w = m.text_width(&ch.to_string());
                    let need = pos - m.text_width(&suffix) - m.text_width(&prefix);
                    let n = if dot_w > 0.0 {
                        (need / dot_w).floor() as usize
                    } else {
                        0
                    };
                    if n > 0 {
                        let mut fill = eff_runs[ti].clone();
                        fill.text = ch.to_string().repeat(n.min(4000));
                        fill.tab = false;
                        eff_runs.insert(ti, fill);
                    }
                }
            }
        }
        metrics.push((meas_size, line_h));
        let mut ep = p.clone();
        ep.runs = eff_runs;
        eff_all.push(ep);
    }
    (eff_all, metrics)
}

/// 圖片佔位框配色（t_a1397522）：灰底＋淺灰描邊。「誠實的近似」視覺：
/// 用戶看得見這裡有圖、多大、在哪，點陣直出屬像素渲染卡。
const IMAGE_PLACEHOLDER_FILL: u32 = 0xFFF0_F0F0;
const IMAGE_PLACEHOLDER_EDGE: u32 = 0xFFCC_CCCC;
const IMAGE_PLACEHOLDER_RULE_PT: f32 = 0.75;

/// 發射圖片佔位框：灰底 Rect＋四邊 Rule（尺寸＝extent）。不動 DrawItem
/// 契約（零新增變體，卡片鐵律）；座標＝頁絕對。
fn emit_image_placeholder(items: &mut Vec<DrawItem>, x: f32, y: f32, w: f32, h: f32) {
    if w <= 0.0 || h <= 0.0 {
        return; // 非法尺寸老實不發射（容錯鐵律）
    }
    items.push(DrawItem::Rect {
        x,
        y,
        w,
        h,
        fill_argb: IMAGE_PLACEHOLDER_FILL,
    });
    let rule = |x1: f32, y1: f32, x2: f32, y2: f32, items: &mut Vec<DrawItem>| {
        items.push(DrawItem::Rule {
            x1,
            y1,
            x2,
            y2,
            width_pt: IMAGE_PLACEHOLDER_RULE_PT,
            color_argb: IMAGE_PLACEHOLDER_EDGE,
            dash: None,
        })
    };
    rule(x, y, x + w, y, items);
    rule(x, y + h, x + w, y + h, items);
    rule(x, y, x, y + h, items);
    rule(x + w, y, x + w, y + h, items);
}

/// 將 OOXML `Run` 樣式換算為 `(flags, color_argb, size_pt)`。
/// `size_pt`＝級聯烘焙的有效字級（None＝未宣告，發射層退回段落／文件級尺）。
fn run_style(r: &quire_ooxml::Run) -> (u8, u32, Option<f32>) {
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
    let color_argb = r.color.unwrap_or(DEFAULT_TEXT_COLOR_ARGB);
    (flags, color_argb, r.size_pt)
}

/// 將單行文字與所屬段落的 `Run` 邊界（依 char offset 累積）求交集，
/// 每個樣式同質段發射一個 `DrawItem::Text`；段 x 偏移以**該段發射字級**的
/// `TextMeasurer` 累加前段寬度——全行無字級宣告時量測尺恆等 `size_pt`，
/// 所有段寬總和與整行寬零漂移（既有契約）；有宣告時各段按自身尺排版
/// （Word 同款視覺近似：字級不同的 run 本就不共用同一基線寬）。
fn emit_line_run_items(
    items: &mut Vec<DrawItem>,
    base_x: f32,
    y: f32,
    size_pt: f32,
    line: &quire_layout::Line,
    para: Option<&quire_ooxml::Paragraph>,
) {
    let char_indices: Vec<(usize, char)> = line.text.char_indices().collect();
    let line_len_cp = char_indices.len();
    if line_len_cp == 0 {
        let (flags, color_argb, sz) = para.and_then(|p| p.runs.first()).map(run_style).unwrap_or((
            0,
            DEFAULT_TEXT_COLOR_ARGB,
            None,
        ));
        items.push(DrawItem::Text {
            x: base_x,
            y,
            size_pt: sz.unwrap_or(size_pt),
            flags,
            color_argb,
            text: String::new(),
        });
        return;
    }

    let line_start = line.char_offset;
    let line_end = line_start + line_len_cp;
    // (行內起始碼點, 行內結束碼點, flags, color_argb, 級聯字級)
    let mut segments: Vec<(usize, usize, u8, u32, Option<f32>)> = Vec::new();
    let mut push_seg = |s: usize, e: usize, flags: u8, color: u32, size: Option<f32>| {
        if s >= e {
            return;
        }
        if let Some(last) = segments.last_mut() {
            // 相鄰且樣式相同時合併為單一可繪段。
            if last.1 == s && last.2 == flags && last.3 == color && last.4 == size {
                last.1 = e;
                return;
            }
        }
        segments.push((s, e, flags, color, size));
    };

    let mut covered = 0usize;
    if let Some(p) = para {
        let mut run_cp = 0usize;
        for r in &p.runs {
            let r_len = r.text.chars().count();
            let r_start = run_cp;
            let r_end = run_cp + r_len;
            run_cp = r_end;
            let inter_s = r_start.max(line_start);
            let inter_e = r_end.min(line_end);
            if inter_s < inter_e {
                let rel_s = inter_s - line_start;
                let rel_e = inter_e - line_start;
                if rel_s > covered {
                    push_seg(covered, rel_s, 0, DEFAULT_TEXT_COLOR_ARGB, None);
                }
                let (f, c, sz) = run_style(r);
                push_seg(rel_s, rel_e, f, c, sz);
                covered = rel_e;
            }
        }
    }
    if covered < line_len_cp {
        push_seg(covered, line_len_cp, 0, DEFAULT_TEXT_COLOR_ARGB, None);
    }

    let byte_of = |cp: usize| -> usize {
        char_indices
            .get(cp)
            .map(|(b, _)| *b)
            .unwrap_or(line.text.len())
    };
    let mut x_off = 0.0f32;
    for (rel_s, rel_e, flags, color_argb, run_size) in segments {
        let seg_text = &line.text[byte_of(rel_s)..byte_of(rel_e)];
        let seg_size = run_size.unwrap_or(size_pt);
        items.push(DrawItem::Text {
            x: base_x + x_off,
            y,
            size_pt: seg_size,
            flags,
            color_argb,
            text: seg_text.to_string(),
        });
        // 零漂移契約：無宣告段（seg_size==size_pt）與舊實作同尺；宣告段按自身尺。
        if (seg_size - size_pt).abs() < 1e-3 {
            x_off += TextMeasurer::new(size_pt).text_width(seg_text);
        } else {
            x_off += TextMeasurer::new(seg_size).text_width(seg_text);
        }
    }
}

/// 表格格內文字的單樣式解析：由儲存格所屬段落之 Run 查詢樣式旗標與顏色。
fn resolve_cell_line_style(
    canon_cell: Option<&CanonicalTableCell>,
    canon_tbl: Option<&CanonicalTable>,
    paragraphs: &[quire_ooxml::Paragraph],
    line_text: &str,
) -> (u8, u32) {
    if line_text.is_empty() {
        return (0, DEFAULT_TEXT_COLOR_ARGB);
    }
    let match_in_para = |p: &quire_ooxml::Paragraph| -> Option<(u8, u32)> {
        let full: String = p.runs.iter().map(|r| r.text.as_str()).collect();
        let byte_pos = full.find(line_text)?;
        let start_cp = full[..byte_pos].chars().count();
        let end_cp = start_cp + line_text.chars().count();
        let mut flags = 0u8;
        let mut color = None;
        let mut run_cp = 0usize;
        for r in &p.runs {
            let r_len = r.text.chars().count();
            let r_start = run_cp;
            let r_end = run_cp + r_len;
            run_cp = r_end;
            if r_start.max(start_cp) < r_end.min(end_cp) {
                let (rf, rc, _sz) = run_style(r);
                flags |= rf;
                if color.is_none() && r.color.is_some() {
                    color = Some(rc);
                }
            }
        }
        Some((flags, color.unwrap_or(DEFAULT_TEXT_COLOR_ARGB)))
    };
    if let Some(cc) = canon_cell {
        for b in &cc.blocks {
            if let CanonicalBlock::Paragraph { index } = b {
                if let Some(p) = paragraphs.get(*index) {
                    if let Some(st) = match_in_para(p) {
                        return st;
                    }
                }
            }
        }
    }
    if let Some(ct) = canon_tbl {
        for r in &ct.rows {
            for c in &r.cells {
                for b in &c.blocks {
                    if let CanonicalBlock::Paragraph { index } = b {
                        if let Some(p) = paragraphs.get(*index) {
                            if let Some(st) = match_in_para(p) {
                                return st;
                            }
                        }
                    }
                }
            }
        }
    }
    (0, DEFAULT_TEXT_COLOR_ARGB)
}

/// 表格切片 → 顯示列表項（遞迴巢狀表）。座標系：`base_*`＝本切片左上（頁絕對），
/// CellFrag/RowFrag 內座標全為切片相對（layout 契約），此處一次性換算。
#[allow(clippy::too_many_arguments)]
fn push_frag_items(
    items: &mut Vec<DrawItem>,
    base_x: f32,
    base_y: f32,
    frag: &TableFrag,
    size: f32,
    canon_tbl: Option<&CanonicalTable>,
    paragraphs: &[quire_ooxml::Paragraph],
) {
    let mut ry = 0.0f32; // 切片內列游標（累加列切片高）
    for row in &frag.rows {
        let h = row.height_pt;
        let canon_row = canon_tbl.and_then(|ct| ct.rows.get(row.row_index));
        for (ci, cell) in row.cells.iter().enumerate() {
            let canon_cell = canon_row.and_then(|cr| cr.cells.get(ci));
            let x = base_x + cell.x_pt;
            let y = base_y + ry;
            if let Some(argb) = cell.fill_argb {
                items.push(DrawItem::Rect {
                    x,
                    y,
                    w: cell.w_pt,
                    h,
                    fill_argb: argb,
                });
            }
            // 框線：邊界線往內收半個線寬（Word 框線畫在格內，不外溢）。
            let line = |pen: &Pen, x1, y1, x2, y2, items: &mut Vec<DrawItem>| {
                let (x1, y1, x2, y2) = (
                    x1 + pen.width_pt / 2.0,
                    y1 + pen.width_pt / 2.0,
                    x2 - pen.width_pt / 2.0,
                    y2 - pen.width_pt / 2.0,
                );
                items.push(DrawItem::Rule {
                    x1,
                    y1,
                    x2,
                    y2,
                    width_pt: pen.width_pt.max(0.25),
                    color_argb: pen.color_argb,
                    dash: pen.dash.clone(),
                });
            };
            if let Some(p) = &cell.edges[quire_layout::table::EDGE_TOP] {
                line(p, x, y, x + cell.w_pt, y, items);
            }
            if let Some(p) = &cell.edges[quire_layout::table::EDGE_BOTTOM] {
                line(p, x, y + h, x + cell.w_pt, y + h, items);
            }
            if let Some(p) = &cell.edges[quire_layout::table::EDGE_LEFT] {
                line(p, x, y, x, y + h, items);
            }
            if let Some(p) = &cell.edges[quire_layout::table::EDGE_RIGHT] {
                line(p, x + cell.w_pt, y, x + cell.w_pt, y + h, items);
            }
            for t in &cell.texts {
                // TODO(CardA): 表格格內文字本輪維持整格單樣式發射（不拆分為多個 DrawItem::Text），
                // 待後續表格卡在 CellText 攜帶段落與字元偏移後，再與正文行共用跨 run 樣式拆段。
                let (flags, color_argb) =
                    resolve_cell_line_style(canon_cell, canon_tbl, paragraphs, &t.text);
                items.push(DrawItem::Text {
                    x: base_x + t.x_pt,
                    y: y + t.y_pt,
                    // 級聯字級（t_5dfc8ac7）：table_map 已按格內段落有效字級
                    // 填入 CellText.size_pt；無宣告＝全域尺（舊行為）。
                    size_pt: t.size_pt.unwrap_or(size),
                    flags,
                    color_argb,
                    text: t.text.clone(),
                });
            }
            // 圖片佔位框（t_a1397522）：灰底 Rect＋四邊 Rule（誠實近似——
            // 點陣直出屬像素渲染卡；分頁容量先對，像素後補）。
            for im in &cell.images {
                emit_image_placeholder(
                    items,
                    base_x + im.x_pt,
                    base_y + ry + im.y_pt,
                    im.w_pt,
                    im.h_pt,
                );
            }
            let nested_tbls: Vec<&CanonicalTable> = canon_cell
                .map(|cc| {
                    cc.blocks
                        .iter()
                        .filter_map(|b| match b {
                            CanonicalBlock::Table(nt) => Some(nt.as_ref()),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            for (ni, (yoff, nf)) in cell.nested.iter().enumerate() {
                // nf.x_pt 是「外層表本地」的子表左緣（含格左緣＋內邊距＋子表
                // 自身 x_off）；換算頁絕對＝外層 base_x ＋ nf.x_pt，與頂層
                // base_x＝frag.x_pt 同款約定（舊代碼直接傳 base_x，丟失子表
                // 水準偏移）。
                push_frag_items(
                    items,
                    base_x + nf.x_pt,
                    base_y + ry + yoff,
                    nf,
                    size,
                    nested_tbls.get(ni).copied(),
                    paragraphs,
                );
            }
        }
        ry += h;
    }
}

/// 給 native shell 的顯示列表批次匯出（評測 render-lab 用）。
pub fn dump_display_lists_json(engine: &Engine) -> String {
    let all: Vec<PageDisplayList> = (0..engine.page_count())
        .filter_map(|i| engine.page_display_list(i))
        .collect();
    serde_json::to_string_pretty(&all).unwrap_or_else(|_| "[]".to_string())
}

// ---------------- 頁首頁尾發射層（t_b39f108b） ----------------
//
// 純讀取投影：sectPr 宣告 → rels 查表 → part 解析 → 依
// (section_id, is_first_page, page % 2) 選 slot 發射顯示列表項。
// 存檔路徑（save_docx／roundtrip）完全不經本層——round-trip 零影響。
//
// 座標系：頁首文字頂端 y＝pgMar/@w:header（pt，自頁頂）；頁尾頂端
// y＝頁高−pgMar/@w:footer。本輪**不縮正文容量**：頁首內容高度若超過
// top margin 減 header 距離，會與正文重疊（誠實近似，溢出處理屬後續卡）。
//
// slot 選用（ECMA-376 語義優先）：titlePg 首頁有 first 用 first、無 first
// 則**空白**（不虛構頁首）；evenAndOddHeaders 偶數頁缺 even 時退回 default
// （生成器常只宣告 default，退回可讓既有頁首在全部頁碼如實出現）；
// 缺件（rId 不存在／指向無 part）容錯跳過。

impl Engine {
    /// 本頁是否為其所屬 section 的首頁（文件首頁或 section 邊界頁）。
    fn is_section_first_page(&self, page: usize) -> bool {
        let cur = self.layout.pages().get(page).map(|p| p.section_id);
        match page.checked_sub(1).and_then(|i| self.layout.pages().get(i)) {
            Some(prev) => match cur {
                Some(id) => prev.section_id != id,
                None => false,
            },
            None => true,
        }
    }

    /// 頁首頁尾 → 顯示列表項。找不到宣告/投影時靜默略過（容錯）。
    fn push_header_footer_items(&self, items: &mut Vec<DrawItem>, p: &Page) {
        if self.hf.is_empty() {
            return;
        }
        let Some(sec) = self.hf.get(&p.section_id) else {
            return;
        };
        let is_first = self.is_section_first_page(p.index);
        let is_even = (p.index + 1).is_multiple_of(2); // 1-based 物理頁碼的偶數頁
                                                       // PAGE／NUMPAGES 代換值（欄位快取值過舊問題由代換解決）。
        let page_no = (p.index + 1).to_string();
        let total_no = self.page_count().to_string();
        let styles = self.doc.parts.get("word/styles.xml").map(|s| s.as_slice());
        let geo = self
            .sec_geo
            .get(&p.section_id)
            .copied()
            .unwrap_or_else(|| self.params_to_geo(p));
        let hf_params =
            LayoutParams::from_section(&geo, self.params.font_size_pt, self.params.line_spacing);
        // (slot 選擇, 發射 y)：頁首 y=header 距離；頁尾 y=頁高−footer 距離。
        self.push_hf_slot(
            items,
            &sec.header,
            sec.title_pg && is_first,
            self.even_odd && is_even,
            sec.header_dist_pt,
            &geo,
            &hf_params,
            styles,
            &page_no,
            &total_no,
        );
        self.push_hf_slot(
            items,
            &sec.footer,
            sec.title_pg && is_first,
            self.even_odd && is_even,
            (p.height_pt - sec.footer_dist_pt).max(0.0),
            &geo,
            &hf_params,
            styles,
            &page_no,
            &total_no,
        );
    }

    /// 頁內 section 幾何後備（正常路徑 sec_geo 必命中；此處只兜底）。
    fn params_to_geo(&self, p: &Page) -> SectionGeometry {
        SectionGeometry {
            page_w_pt: p.width_pt,
            page_h_pt: p.height_pt,
            margin_top_pt: p.margin_top_pt,
            margin_bottom_pt: p.margin_top_pt,
            margin_left_pt: p.margin_left_pt,
            margin_right_pt: p.margin_left_pt,
            landscape: false,
            continuous: false,
        }
    }

    /// 單一 slot（頁首或頁尾）的發射：段落行＋表格切片＋欄位代換。
    #[allow(clippy::too_many_arguments)]
    fn push_hf_slot(
        &self,
        items: &mut Vec<DrawItem>,
        slots: &SlotPaths,
        use_first: bool,
        use_even: bool,
        emit_y: f32,
        geo: &SectionGeometry,
        hf_params: &LayoutParams,
        styles: Option<&[u8]>,
        page_no: &str,
        total_no: &str,
    ) {
        let path = pick_slot(slots, use_first, use_even);
        let Some(path) = path else { return };
        let Some(part) = self.hf_parts.get(path) else {
            return;
        };
        // 欄位代換：帶 field 標記**且有實際文字**的 run，以當頁頁碼/總頁數
        // 覆寫（end 標記 run 只帶 fldChar、無文字——空文字注入會虛増頁碼）。
        let paragraphs: Vec<quire_ooxml::Paragraph> = part
            .styles
            .iter()
            .map(|pp| {
                let mut pp = pp.clone();
                for r in pp.runs.iter_mut() {
                    if r.text.is_empty() {
                        continue;
                    }
                    match r.field {
                        Some(quire_ooxml::FieldKind::Page) => r.text = page_no.to_string(),
                        Some(quire_ooxml::FieldKind::NumPages) => r.text = total_no.to_string(),
                        None => {}
                    }
                }
                pp
            })
            .collect();
        let texts: Vec<String> = paragraphs
            .iter()
            .map(|pp| pp.runs.iter().map(|r| r.text.as_str()).collect())
            .collect();
        let line_h = hf_params.line_h_pt();
        // 表格槽：與正文同一映射層（styles 框線查表）；單 section 合成表。
        let sec_one = [SectionSpec { id: 0, geo: *geo }];
        // 頁首 part 內無圖片投影需求：空清單（build_slots 新簽名，t_a1397522）。
        let slots_tbl = table_map::build_slots(
            &part.tables,
            &texts,
            styles,
            hf_params,
            &sec_one,
            &[],
            &paragraphs,
        );
        // 流項＝段落行＋表格依序排列（簡化 packer：頁首本就一小節，不跨頁，
        // 表格整張放進自身帶高，超出部分由頁首自然向下延伸——重疊風險見模組注）。
        // 斷行走級聯逐段字級（t_5dfc8ac7；無宣告＝hf_params 全域尺＝舊行為）。
        let grid: Vec<(f32, f32)> = part.metrics.clone();
        let pls = quire_layout::layout_paragraphs_grid(&texts, hf_params, &[], &grid);
        let mut cursor_bands: u32 = 0;
        let mut ti = 0usize;
        let mut skip_until = 0usize;
        for (pi, pl) in pls.iter().enumerate() {
            while ti < slots_tbl.len() && slots_tbl[ti].para_start <= pi {
                self.emit_hf_table(
                    items,
                    &slots_tbl[ti],
                    emit_y,
                    cursor_bands,
                    line_h,
                    hf_params,
                    &paragraphs,
                );
                cursor_bands += slots_tbl[ti].layout.total_bands.max(1);
                skip_until = skip_until.max(slots_tbl[ti].para_end);
                ti += 1;
            }
            if pi < skip_until {
                continue; // 表格內部段落：文字由切片繪製
            }
            for (li, l) in pl.lines.iter().enumerate() {
                if l.text.is_empty() {
                    continue; // 空行不發射：空 Text 項不繪製，卻會佔據
                              // render-lab 前 8 行比對視窗、稀釋相似度。
                }
                let y = emit_y + (cursor_bands + li as u32) as f32 * line_h;
                emit_line_run_items(
                    items,
                    hf_params.margin(MARGIN_LEFT),
                    y,
                    hf_params.font_size_pt,
                    l,
                    paragraphs.get(l.para_index),
                );
            }
            cursor_bands += pl.lines.len().max(1) as u32;
        }
        while ti < slots_tbl.len() {
            self.emit_hf_table(
                items,
                &slots_tbl[ti],
                emit_y,
                cursor_bands,
                line_h,
                hf_params,
                &paragraphs,
            );
            cursor_bands += slots_tbl[ti].layout.total_bands.max(1);
            ti += 1;
        }
    }

    /// 頁首表格：整張切片（不跨頁）＋既有 push_frag_items 渲染路徑。
    // 發射層內部函數：參數即 emission 上下文，拆群反而模糊資料流。
    #[allow(clippy::too_many_arguments)]
    fn emit_hf_table(
        &self,
        items: &mut Vec<DrawItem>,
        slot: &TableSlot,
        emit_y: f32,
        cursor_bands: u32,
        line_h: f32,
        hf_params: &LayoutParams,
        paragraphs: &[quire_ooxml::Paragraph],
    ) {
        let (frag, _next, _used) = slot.layout.fragment(
            0,
            0,
            slot.layout.total_bands.max(1),
            false,
            hf_params.margin(MARGIN_LEFT),
        );
        let y = emit_y + cursor_bands as f32 * line_h;
        // canonical 對照：頁首表格的格內樣式解析暫以 None 進行（樣式查表屬
        // Card A 表格樣式深化範圍；文字/框線/底紋不受影響——push_frag_items
        // 對 canon_tbl=None 走「整格單樣式」既有路徑）。
        push_frag_items(
            items,
            frag.x_pt,
            y,
            &frag,
            hf_params.font_size_pt,
            None,
            paragraphs,
        );
    }
}

/// slot 選用（ECMA-376 語義優先）：
/// - titlePg 首頁：有 first 用 first；**無 first 則空白**（Word 即如此，
///   不退回 default——生成器宣告 titlePg 卻未給 first 時，退回會虛構頁首，
///   FDO76312 render-lab 實測 LO/Word 首頁皆無頁首）。
/// - evenAndOddHeaders 偶數頁：有 even 用 even；無 even 退回 default
///   （生成器常只宣告 default，退回可讓頁首在全部頁碼如實出現，
///   PageSpecificHeadFoot 實測此近似與 LO 一致）。
fn pick_slot(slots: &SlotPaths, use_first: bool, use_even: bool) -> Option<&String> {
    if use_first {
        return slots.first.as_ref();
    }
    if use_even {
        if let Some(p) = slots.even.as_ref() {
            return Some(p);
        }
    }
    slots.default.as_ref()
}

/// 頁首頁尾發射層四元組：(section id → 發射資料, part 路徑 → 投影,
/// evenAndOddHeaders, section id → 幾何)。
type HfLayer = (
    HashMap<u32, SectionHf>,
    HashMap<String, HfPart>,
    bool,
    HashMap<u32, SectionGeometry>,
);

/// 構建頁首頁尾發射層：宣告解析→rels 換算→存在性過濾→part 解析（去重）→
/// 樣式級聯烘焙（t_5dfc8ac7：與正文同一條 `cascade_paragraphs` 路徑）。
fn build_hf_layer(
    doc: &OoxmlDoc,
    sections: &[SectionSpec],
    style_table: Option<&StyleTable>,
) -> HfLayer {
    let mut sec_geo: HashMap<u32, SectionGeometry> = HashMap::new();
    for s in sections {
        sec_geo.entry(s.id).or_insert(s.geo);
    }
    let Some(doc_xml) = doc.parts.get("word/document.xml") else {
        return Default::default();
    };
    let decls = read_sections_header_footer(doc_xml);
    if decls.is_empty() {
        return Default::default();
    }
    let even_odd = doc
        .parts
        .get("word/settings.xml")
        .map(|s| read_even_and_odd_headers(s))
        .unwrap_or(false);
    let rels = doc
        .parts
        .get("word/_rels/document.xml.rels")
        .map(|r| read_rels(r))
        .unwrap_or_default();

    let resolve = |rid: &Option<String>| -> Option<String> {
        let rid = rid.as_ref()?;
        let path = resolve_relationship(&rels, rid)?;
        // 指向不存在 part 的懸空 rId → None（容錯跳過，不虛構）。
        doc.parts.contains_key(&path).then_some(path)
    };
    let paths_of = |sr: &SlotRefs| SlotPaths {
        default: resolve(&sr.default),
        first: resolve(&sr.first),
        even: resolve(&sr.even),
    };

    let mut hf: HashMap<u32, SectionHf> = HashMap::new();
    let mut want: Vec<String> = Vec::new();
    let push_want = |v: &Option<String>, want: &mut Vec<String>| {
        if let Some(p) = v {
            if !want.contains(p) {
                want.push(p.clone());
            }
        }
    };
    for d in decls {
        let header = paths_of(&d.header);
        let footer = paths_of(&d.footer);
        push_want(&header.default, &mut want);
        push_want(&header.first, &mut want);
        push_want(&header.even, &mut want);
        push_want(&footer.default, &mut want);
        push_want(&footer.first, &mut want);
        push_want(&footer.even, &mut want);
        hf.insert(
            d.id,
            SectionHf {
                header_dist_pt: d.header_dist_pt,
                footer_dist_pt: d.footer_dist_pt,
                title_pg: d.title_pg,
                header,
                footer,
            },
        );
    }
    let mut hf_parts: HashMap<String, HfPart> = HashMap::new();
    // 級聯基準字級：hf part 與正文共用同一文件基準（doc_grid；無級聯＝12）。
    let def_size = style_table.map_or(FALLBACK_SIZE_PT, |t| t.doc_grid().0.unwrap_or(12.0));
    for path in want {
        if let Some(xml) = doc.parts.get(&path) {
            if let Ok((paragraphs, tables)) = parse_header_footer_part(xml) {
                let (styles, metrics) = match style_table {
                    // 頁首尾不展開點引（widths 空＝不虛構；既有行為不變）。
                    Some(st) => cascade_paragraphs(st, &paragraphs, def_size, &[]),
                    None => (paragraphs, Vec::new()),
                };
                hf_parts.insert(
                    path,
                    HfPart {
                        styles,
                        metrics,
                        tables,
                    },
                );
            }
        }
    }
    (hf, hf_parts, even_odd, sec_geo)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = include_bytes!("../../../fixtures/sample.docx");

    // ---------------- 頁首頁尾（t_b39f108b） ----------------

    /// 自製 fixture：titlePg＋first 頁首、default 表格頁首、PAGE/NUMPAGES
    /// 頁尾——發射的 y 座標取自 pgMar/@w:header｜頁高−footer，slot 依
    /// 「首頁→first／其餘→default」選用，欄位快取值被當頁數覆寫。
    #[test]
    fn header_footer_emits_with_geometry_and_fields() {
        const HF: &[u8] = include_bytes!("../../../fixtures/hf_table_header.docx");
        let engine = Engine::from_docx_bytes(HF).expect("開啟 hf_table_header.docx 失敗");
        assert!(
            engine.page_count() >= 2,
            "fixture 必須至少兩頁才有 default 頁首"
        );
        // 第一頁：first slot 文字在 header 距離處（432twip=21.6pt）；無表格框線。
        let p0 = engine.page_display_list(0).unwrap();
        let first_hdr = p0
            .items
            .iter()
            .find_map(|it| match it {
                DrawItem::Text { text, y, .. } if text == "首頁專用頁首" => Some(*y),
                _ => None,
            })
            .expect("首頁應發射 first 頁首文字");
        assert!(
            (first_hdr - 21.6).abs() < 0.6,
            "first 頁首 y 應≈header 距離 21.6pt，實際 {first_hdr}"
        );
        assert!(
            p0.items
                .iter()
                .all(|it| !matches!(it, DrawItem::Rule { .. })),
            "first 頁首是純文字段，不該有表格框線"
        );
        // 第二頁起：default 表格頁首——公文表頭文字＋四條框線 Rule。
        let p1 = engine.page_display_list(1).unwrap();
        let tbl_hdr = p1
            .items
            .iter()
            .find_map(|it| match it {
                DrawItem::Text { text, y, .. } if text == "機關名義：奎爾文書局" => {
                    Some(*y)
                }
                _ => None,
            })
            .expect("第二頁應發射 default 表格頁首格內文字");
        assert!(
            tbl_hdr < 72.0,
            "頁首表格文字必須落在 top margin 上方：{tbl_hdr}"
        );
        let rules = p1
            .items
            .iter()
            .filter(|it| matches!(it, DrawItem::Rule { .. }))
            .count();
        assert!(rules >= 4, "單格表格至少四邊框線，實際 {rules} 條");
        // 頁尾欄位：PAGE 與 NUMPAGES 以當頁數代換，過期快取值絕不露出。
        // 注意 fixture 只宣告 default 頁尾（無 first 頁尾）——titlePg strict
        // 語義下首頁頁尾為空白，第一頁的頁碼要從第二頁驗證。
        let foot0 = page_footer_text(&p0);
        assert!(
            foot0.is_empty(),
            "titlePg 且無 first 頁尾時首頁頁尾應空白：{foot0:?}"
        );
        let foot1 = page_footer_text(&p1);
        assert_eq!(foot1, format!("2 / {}", engine.page_count()));
        assert!(
            !foot1.contains("過期"),
            "欄位快取髒值不得出現在發射結果：{foot1}"
        );
    }

    /// 取頁面下部（y>780pt，正文底 769.9 與頁尾 805.9 之間）的頁尾文字串接。
    fn page_footer_text(dl: &PageDisplayList) -> String {
        dl.items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Text { text, y, .. } if *y > 780.0 => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// 純讀取鐵律：頁首頁尾層完全不觸碰 parts——開檔不動存檔必須
    /// byte-identical（頁首 part 亦是 untouched 條目，必須原樣回填）。
    #[test]
    fn header_footer_round_trip_untouched() {
        const HF: &[u8] = include_bytes!("../../../fixtures/hf_table_header.docx");
        let engine = Engine::from_docx_bytes(HF).unwrap();
        let saved = engine.save_docx().unwrap();
        assert_eq!(saved.as_slice(), HF, "含頁首頁尾的文件存檔必須位元組級相同");
        // header/footer part 確實是 untouched 條目（並非被改後碰巧還原）。
        assert!(
            engine.preserved_part_count() >= 7,
            "七個 part 全部應為 untouched"
        );
    }

    #[test]
    fn end_to_end_open_and_layout() {
        let engine = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        assert!(engine.page_count() >= 1);
        assert!(engine.paragraph_count() >= 3);
        assert_eq!(
            engine.preserved_part_count(),
            3,
            "[Content_Types].xml + .rels + document.xml"
        );
        let json = engine
            .page_display_list_json(0)
            .expect("第 0 頁顯示列表不存在");
        // DrawItem 為 externally-tagged enum：序列化後以變體名 "Text" 為鍵。
        assert!(json.contains("\"Text\""), "顯示列表 JSON 應含 Text 變體");
        let all_pages: String = (0..engine.page_count())
            .filter_map(|i| engine.page_lines(i))
            .flatten()
            .collect();
        assert!(all_pages.contains("Quire"));
    }

    #[test]
    fn edit_then_repaginate() {
        let mut engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        let before = engine.page_count();
        let events = engine.replace_paragraph_text(1, &"重排測試".repeat(3000));
        assert!(!events.is_empty());
        assert!(engine.page_count() > before);
        assert!(engine.is_settled(), "全速模式：編輯後即定案");
    }

    #[test]
    fn soft_pagination_reflow_settles_via_json_api() {
        // Phase0-6 控制面：小預算灌長文 → ReflowPending；reflow_step_json
        // 逐步推進直到 settled，頁數收斂到全速重建同款。
        let mut engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        engine.set_sync_budget(Some(1));
        let evs = engine.replace_paragraph_text(1, &"預算".repeat(20000));
        assert!(
            evs.iter().any(|e| e.starts_with("ReflowPending")),
            "小預算長文必須出現軟分頁：{evs:?}"
        );
        assert!(!engine.is_settled());
        let mut steps = 0usize;
        while !engine.is_settled() {
            let v: serde_json::Value = serde_json::from_str(&engine.reflow_step_json()).unwrap();
            assert!(v["events"].is_array(), "reflow JSON 契約：events 陣列");
            steps += 1;
            assert!(steps < 500, "reflow 必須收敛（卡在 {steps} 步）");
        }
        // 定案後版面必須與「重開一份同內容文件」頁數一致（全速對照）。
        let full = Engine::from_docx_bytes(SAMPLE).unwrap();
        assert!(engine.page_count() > full.page_count());
    }

    #[test]
    fn ime_commit_inserts_at_caret_including_emoji() {
        // Web-1 契約：IME commit（含注音合成結果與 emoji）經 caret 插入進引擎。
        let mut engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        let p0 = engine.paragraph_text(0).unwrap();
        let n = p0.chars().count();
        // 段尾 commit「測試😀」——emoji 是 surrogate pair，碼點切口不可斷半。
        let evs = engine.insert_text_at_caret(0, n, "測試😀");
        assert!(!evs.is_empty(), "commit 必須回報失效事件");
        let after = engine.paragraph_text(0).unwrap();
        assert!(after.ends_with("測試😀"), "commit 結果應附於段尾：{after}");
        assert_eq!(
            after.chars().count(),
            n + 3,
            "碼點數必須精確（emoji=1 碼點）"
        );
        // 段首插入（caret=0）。
        engine.insert_text_at_caret(0, 0, "★");
        assert!(engine.paragraph_text(0).unwrap().starts_with('★'));
        // caret 越界鉗到段尾，不可 panic。
        engine.insert_text_at_caret(0, 1 << 20, "END");
        assert!(engine.paragraph_text(0).unwrap().ends_with("END"));
        // 不存在的段落回空事件、不 panic。
        assert!(engine.insert_text_at_caret(9999, 0, "X").is_empty());
    }

    #[test]
    fn caret_round_trip_via_json() {
        // wasm/JS 邊界走 JSON：caret_rect_json / caret_at_json 必須可解析。
        let engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        let rect: serde_json::Value =
            serde_json::from_str(&engine.caret_rect_json(0, 1).unwrap()).unwrap();
        assert!(rect["x_pt"].as_f64().unwrap() > 0.0);
        let back: serde_json::Value = serde_json::from_str(
            &engine
                .caret_at_json(
                    rect["page"].as_u64().unwrap() as usize,
                    rect["x_pt"].as_f64().unwrap() as f32,
                    rect["y_pt"].as_f64().unwrap() as f32 + 6.0,
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(back["para"].as_u64().unwrap(), 0);
    }

    #[test]
    fn untouched_round_trip_is_byte_identical() {
        let engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        let saved = engine.save_docx().unwrap();
        assert_eq!(saved.as_slice(), SAMPLE, "開檔→不動→存檔必須位元組級相同");
        let d = engine.roundtrip_check().unwrap();
        assert!(
            d.byte_identical && d.pass,
            "未觸碰對照必須綠燈:\n{}",
            d.report()
        );
    }

    /// Phase3-1a：表格 canonical 投影經 engine 暴露（3-1b 的上游契約），
    /// 且含表格檔案的 byte-level 無損鐵律不倒退（唯讀來源隔離的實證）。
    #[test]
    fn table_projection_exposed_and_round_trip_stays_byte_identical() {
        const TABLE_MIXED: &[u8] = include_bytes!("../../../fixtures/table_mixed.docx");
        let engine = Engine::from_docx_bytes(TABLE_MIXED).unwrap();
        assert_eq!(engine.table_count(), 1, "body 級表一張（巢狀表掛 tc 內）");
        // wasm/JS 邊界：tables_json 可解析且欄數正確。
        let json = engine.tables_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v[0]["table"]["grid"].as_array().unwrap().len(), 3);
        // typed 路徑：re-export 型別（CanonicalBlock／BodyTable）經 engine 可用。
        let typed = serde_json::from_str::<Vec<BodyTable>>(&json).unwrap();
        let CanonicalBlock::Table(nested) = &typed[0].table.rows[2].cells[1].blocks[1] else {
            panic!("巢狀表格必須經 canonical 樹暴露");
        };
        assert_eq!(nested.grid_col_count(), 2);
        let _: &CanonicalTable = &typed[0].table;
        // 無損鐵律：開檔→不動→存檔位元組級相同（解析純讀入，不碰寫回來源）。
        assert_eq!(engine.save_docx().unwrap().as_slice(), TABLE_MIXED);
    }

    // ---------------- t_a1397522：圖片幾何佔位框（end-to-end 投影） ----------------

    const INLINE_IMG: &[u8] = include_bytes!("../../../fixtures/inline_image.docx");

    /// selfgen fixture：文字＋100pt inline 圖＋文字——中間段恰佔
    /// ceil(100/16.8)＝6 帶，SVG 層（display list）有灰 Rect＋四邊 Rule。
    #[test]
    fn inline_image_fixture_bands_and_placeholder_items() {
        let engine = Engine::from_docx_bytes(INLINE_IMG).unwrap();
        assert_eq!(engine.paragraph_count(), 3);
        let dl = engine.page_display_list(0).unwrap();
        let rects: Vec<&DrawItem> = dl
            .items
            .iter()
            .filter(|it| matches!(it, DrawItem::Rect { fill_argb, .. } if *fill_argb == IMAGE_PLACEHOLDER_FILL))
            .collect();
        assert_eq!(rects.len(), 1, "恰好一張佔位 Rect");
        let rules = dl
            .items
            .iter()
            .filter(|it| matches!(it, DrawItem::Rule { color_argb, .. } if *color_argb == IMAGE_PLACEHOLDER_EDGE))
            .count();
        assert_eq!(rules, 4, "Rect 四邊各一條 Rule");
        // Rect 幾何＝extent：200×100pt，x＝版心左緣（A4 1 吋邊界＝72pt）。
        if let DrawItem::Rect { x, y, w, h, .. } = rects[0] {
            assert!(
                (w - 200.0).abs() < 1e-2 && (h - 100.0).abs() < 1e-2,
                "{w}x{h}"
            );
            assert!((x - 72.0).abs() < 1e-2, "版心左緣：{x}");
            // 段落 0 帶 0 → 圖頂帶 1（吸收態：空行帶＝圖頂帶；本段行=空行）。
            let line_h = engine.params.line_h_pt();
            assert!((y - (72.0 + 1.0 * line_h)).abs() < 1e-2, "圖 y＝帶 1：{y}");
        }
    }

    /// anchor（頁面鎖點）不撐正文行帶：與 inline 同 XML 換成 anchor 後，
    /// 正文行序不動，只有 floating Rect 出現。
    #[test]
    fn anchored_fixture_does_not_push_lines() {
        let mut base = Engine::from_docx_bytes(INLINE_IMG).unwrap();
        let inline_xml =
            String::from_utf8_lossy(base.doc.parts.get("word/document.xml").unwrap().as_slice())
                .into_owned();
        let anchor_xml = inline_xml
            .replace("<wp:inline", "<wp:anchor")
            .replace("</wp:inline>", "</wp:anchor>")
            .replace(
                "<wp:extent",
                "<wp:positionH relativeFrom=\"page\"><wp:posOffset>914400</wp:posOffset></wp:positionH>\
                 <wp:positionV relativeFrom=\"page\"><wp:posOffset>1828800</wp:posOffset></wp:positionV><wp:extent",
            );
        base.set_part("word/document.xml", anchor_xml.into_bytes());
        let engine = Engine::from_docx_bytes(&base.save_docx().unwrap()).unwrap();
        // 正文行與無圖基線一致：三行（圖段是空行，浮動圖不再疊幽靈行）。
        let lines: Vec<String> = (0..engine.page_count())
            .flat_map(|i| engine.page_lines(i).unwrap_or_default())
            .collect();
        assert_eq!(
            lines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            vec![
                "圖片前段：占位框登場之前的文字。",
                "",
                "圖片後段：占位框之後的文字。"
            ],
            "浮動圖不得撐動正文行：{lines:?}"
        );
        let dl = engine.page_display_list(0).unwrap();
        let rects = dl
            .items
            .iter()
            .filter(|it| matches!(it, DrawItem::Rect { fill_argb, .. } if *fill_argb == IMAGE_PLACEHOLDER_FILL))
            .count();
        assert_eq!(rects, 1, "鎖點佔位 Rect 仍發射");
        if let Some(DrawItem::Rect { x, y, .. }) = dl
            .items
            .iter()
            .find(|it| matches!(it, DrawItem::Rect { fill_argb, .. } if *fill_argb == IMAGE_PLACEHOLDER_FILL))
        {
            assert!((x - 72.0).abs() < 1e-2 && (y - 144.0).abs() < 1e-2, "頁鎖點 1in/2in：{x},{y}");
        }
    }

    /// 格內圖注入 CellBlock::Image：表內儲存格發射佔位 Rect（隨表格切片）。
    #[test]
    fn cell_image_injects_into_table_slot() {
        let mut base = Engine::from_docx_bytes(SAMPLE).unwrap();
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">
  <w:body>
    <w:p><w:r><w:t>表前段</w:t></w:r></w:p>
    <w:tbl>
      <w:tblPr><w:tblW w:w="4000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr>
      <w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>
      <w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr>
        <w:p><w:r><w:t>格內文字</w:t></w:r></w:p>
        <w:p><w:r><w:drawing><wp:inline><wp:extent cx="317500" cy="254000"/>
          <a:graphic xmlns:a="a"><a:graphicData><pic:pic xmlns:pic="p"/></a:graphicData></a:graphic>
        </wp:inline></w:drawing></w:r></w:p>
      </w:tc></w:tr>
    </w:tbl>
    <w:p><w:r><w:t>表後段</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        base.set_part("word/document.xml", xml.as_bytes().to_vec());
        let docx = base.save_docx().unwrap();
        let engine = Engine::from_docx_bytes(&docx).unwrap();
        let dl = engine.page_display_list(0).unwrap();
        let rects: Vec<(f32, f32, f32, f32)> = dl
            .items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Rect {
                    x,
                    y,
                    w,
                    h,
                    fill_argb,
                } if *fill_argb == IMAGE_PLACEHOLDER_FILL => Some((*x, *y, *w, *h)),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 1, "格內圖＝一格佔位 Rect：{rects:?}");
        let (_, _, w, h) = rects[0];
        assert!(
            (w - 25.0).abs() < 0.5 && (h - 20.0).abs() < 0.5,
            "extent 25×20pt：{w}x{h}"
        );
        // 格內圖不進正文流：表後段仍是第 3 段，行數不受圖影響多出幽靈行。
        let lines: Vec<String> = (0..engine.page_count())
            .flat_map(|i| engine.page_lines(i).unwrap_or_default())
            .collect();
        assert_eq!(lines.len(), 2, "正文只有表前/表後兩行：{lines:?}");
    }

    #[test]
    fn edited_part_passes_with_whitelist_and_others_are_zero_loss() {
        let mut engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        engine.set_part("word/document.xml", b"<w:document edited/>".to_vec());
        let d = engine.roundtrip_check().unwrap();
        assert!(d.pass, "白名單內改動，其餘零失真:\n{}", d.report());
        assert_eq!(d.whitelisted_changed, vec!["word/document.xml".to_string()]);
        // 重新開啟後編輯內容確實生效、其他 part 還在
        let again = Engine::from_docx_bytes(&engine.save_docx().unwrap()).unwrap();
        assert_eq!(again.preserved_part_count(), 3);
    }

    #[test]
    fn multi_run_line_emits_styled_segments_with_zero_width_drift() {
        // 以 sample.docx 為基底替換 document.xml，構造含紅粗體、斜底線、刪除線與超連結的多 run 段落。
        let mut base = Engine::from_docx_bytes(SAMPLE).unwrap();
        let custom_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:rPr><w:b/><w:color w:val="FF0000"/></w:rPr>
        <w:t>紅粗體</w:t>
      </w:r>
      <w:r>
        <w:rPr><w:i/><w:u w:val="single"/></w:rPr>
        <w:t>斜底線</w:t>
      </w:r>
      <w:r>
        <w:rPr><w:strike/></w:rPr>
        <w:t>刪除線</w:t>
      </w:r>
      <w:hyperlink>
        <w:r>
          <w:t>連結</w:t>
        </w:r>
      </w:hyperlink>
    </w:p>
  </w:body>
</w:document>"#
            .as_bytes();
        base.set_part("word/document.xml", custom_xml.to_vec());
        let saved = base.save_docx().unwrap();
        let engine = Engine::from_docx_bytes(&saved).unwrap();
        let dl = engine.page_display_list(0).unwrap();
        let texts: Vec<_> = dl
            .items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Text {
                    x,
                    flags,
                    color_argb,
                    text,
                    ..
                } => Some((*x, *flags, *color_argb, text.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 4, "四個不同樣式 run 應發射四個 DrawItem::Text");
        assert_eq!(texts[0].1, FLAG_BOLD);
        assert_eq!(texts[0].2, 0xFFFF_0000);
        assert_eq!(texts[0].3, "紅粗體");

        assert_eq!(texts[1].1, FLAG_ITALIC | FLAG_UNDERLINE);
        assert_eq!(texts[1].2, 0xFF00_0000);
        assert_eq!(texts[1].3, "斜底線");

        assert_eq!(texts[2].1, FLAG_STRIKE);
        assert_eq!(texts[2].2, 0xFF00_0000);
        assert_eq!(texts[2].3, "刪除線");

        assert_eq!(texts[3].1, FLAG_UNDERLINE);
        assert_eq!(texts[3].2, 0xFF05_63C1);
        assert_eq!(texts[3].3, "連結");

        // 行寬零漂移：所有段寬總和必須等於原整行寬，且段 x 偏移嚴格等於前段累加寬。
        let m = TextMeasurer::new(12.0);
        let whole_line_w = m.text_width("紅粗體斜底線刪除線連結");
        let sum_seg_w: f32 = texts.iter().map(|(_, _, _, s)| m.text_width(s)).sum();
        assert!(
            (sum_seg_w - whole_line_w).abs() < 1e-4,
            "所有段寬總和 {sum_seg_w} 必須等於整行寬 {whole_line_w}"
        );
        let mut expected_x = texts[0].0;
        for (x, _, _, s) in &texts {
            assert!(
                (*x - expected_x).abs() < 1e-4,
                "段 x {x} 應等於累加位置 {expected_x}"
            );
            expected_x += m.text_width(s);
        }
    }

    #[test]
    fn fdo77887_bold_title_has_nonzero_flags_in_display_list() {
        const FDO77887: &[u8] = include_bytes!("../../../corpus/ooxmlexport/fdo77887.docx");
        let engine = Engine::from_docx_bytes(FDO77887).expect("開啟 fdo77887.docx 失敗");
        let dl = engine
            .page_display_list(0)
            .expect("fdo77887 第 0 頁顯示列表");
        let title_item = dl.items.iter().find_map(|it| match it {
            DrawItem::Text { flags, text, .. } if text.contains("INFORME DE ASISTENCIA") => {
                Some(*flags)
            }
            _ => None,
        });
        let flags = title_item.expect("顯示列表應包含 fdo77887 標題段 INFORME DE ASISTENCIA");
        assert_ne!(
            flags, 0,
            "fdo77887 粗體標題段在 display list 中 flags 不可為 0"
        );
        assert_ne!(flags & FLAG_BOLD, 0, "fdo77887 粗體標題段應帶有 FLAG_BOLD");
    }

    /// 超連結色級聯回歸：style-inheritance.docx 的 TOC 連結 run 帶
    /// `w:rStyle="Hyperlink"`，styles.xml 宣告 `w:color="0000FF"`；顯示列表
    /// 必須採宣告色，不得被解析器合成預設 #0563C1 越過（本卡先紅後綠）。
    #[test]
    fn style_inheritance_hyperlink_uses_declared_style_color() {
        const SI: &[u8] = include_bytes!("../../../corpus/ooxmlexport/style-inheritance.docx");
        let engine = Engine::from_docx_bytes(SI).expect("開啟 style-inheritance.docx");
        let link_color = (0..engine.page_count())
            .flat_map(|p| engine.page_display_list(p).expect("頁存在").items)
            .find_map(|it| match it {
                DrawItem::Text {
                    text, color_argb, ..
                } if text == "Heading 1" => Some(color_argb),
                _ => None,
            })
            .expect("TOC 應有「Heading 1」連結文字");
        assert_eq!(
            link_color, 0xFF00_00FF,
            "TOC 連結必須採用 styles.xml Hyperlink 字元樣式宣告的 0000FF"
        );
    }

    // ---------------- 強制分頁 br-page（t_01fa57af） ----------------

    /// corpus 回歸：style-inheritance.docx（14 處 br-page）頁數收斂到 LO 的
    /// 15 頁（±1）；斷點前段留原頁、斷點後段落從第 2 頁起頭（Word 語義）。
    #[test]
    fn br_page_corpus_style_inheritance_hits_lo_page_count() {
        const SI: &[u8] = include_bytes!("../../../corpus/ooxmlexport/style-inheritance.docx");
        let engine = Engine::from_docx_bytes(SI).expect("開啟 style-inheritance.docx");
        assert!(
            (14..=16).contains(&engine.page_count()),
            "14 處 br-page 後頁數應收斂 LO 15±1，實得 {}",
            engine.page_count()
        );
        // round-trip 鐵律不受影響：canonical 加欄位，寫回來源仍是 parts 明文。
        assert_eq!(engine.save_docx().unwrap().as_slice(), SI);
    }

    /// corpus 回歸：reader-styles.docx（2 處 br-page＋3 sectPr）2 頁→3 頁，
    /// 第 3 頁以 br 後的「Images」段起頭（LO 3 頁對齊）。
    #[test]
    fn br_page_corpus_reader_styles_reaches_three_pages() {
        const RS: &[u8] = include_bytes!(
            "../../../corpus/phpword/tests_PhpWordTests__files_documents_reader-styles.docx"
        );
        let engine = Engine::from_docx_bytes(RS).expect("開啟 reader-styles.docx");
        assert_eq!(engine.page_count(), 3, "br-page 後應與 LO 同為 3 頁");
        let last: Vec<String> = engine.page_lines(2).expect("第 2 頁存在");
        assert!(
            last.iter().any(|l| l.contains("Images")),
            "br-page 後的 Images 段應從末頁起頭，實得 {last:?}"
        );
        assert_eq!(engine.save_docx().unwrap().as_slice(), RS);
    }

    /// corpus 回歸（t_1a79f1c5）：mce-nested.docx（`mc:Fallback`×6，Choice
    /// 的 DrawingML＋Fallback 的 VML 雙份表示，positionH/V 只有 `<wp:align>`
    /// ／`<wp14:pctPosVOffset>` 無 posOffset）。舊口徑缺座標→浮圖退化
    /// inline，792pt 高的封面圖撐爆行網格（1 頁變 3 頁，LO=1）。修後：
    /// Fallback 整棵吞沒＋align/pct 兩式解鎖點 → 回 1 頁；round-trip 零影響。
    #[test]
    fn mce_nested_fallback_drawing_stays_one_page() {
        const MN: &[u8] = include_bytes!("../../../corpus/ooxmlexport/mce-nested.docx");
        let engine = Engine::from_docx_bytes(MN).expect("開啟 mce-nested.docx");
        assert_eq!(
            engine.page_count(),
            1,
            "Fallback 雙份＋align 式浮圖應收斂 LO 的 1 頁"
        );
        // 兩張 Choice 圖（550.8×50.4 與 245.55×792）都以 align/pct 鎖點發射，
        // 不佔行帶：第 0 頁必須同時含兩張 Rect。
        let rects = engine
            .page_display_list(0)
            .expect("第 0 頁存在")
            .items
            .into_iter()
            .filter(|it| matches!(it, DrawItem::Rect { .. }))
            .count();
        assert!(rects >= 2, "兩張浮圖都應以鎖點 Rect 發射，實得 {rects}");
        // round-trip 鐵律不受影響：parts passthrough，存檔位元組級相同。
        assert_eq!(engine.save_docx().unwrap().as_slice(), MN);
    }

    /// Uranus（無 br-page，僅 lastRenderedPageBreak 渲染提示）：頁數不受本卡
    /// 影響＝印證「渲染提示不計斷點」。Uranus/Saturn 縮水成因是圖片高度 0
    /// （Card C 範圍），非 br-page 缺失。
    #[test]
    fn last_rendered_page_break_hint_never_changes_page_count() {
        const U: &[u8] = include_bytes!(
            "../../../corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Uranus.docx"
        );
        let engine = Engine::from_docx_bytes(U).expect("開啟 Uranus.docx");
        let before = engine.page_count();
        // 同檔重複開啟＋編輯再排：lastRenderedPageBreak 恆零影響，頁數回穩。
        let mut eng2 = Engine::from_docx_bytes(U).expect("重開 Uranus.docx");
        eng2.replace_paragraph_text(
            0,
            &format!("{}x", eng2.paragraph_text(0).unwrap_or_default()),
        );
        while !eng2.is_settled() {
            eng2.reflow_step_json();
        }
        assert_eq!(
            eng2.page_count(),
            before,
            "僅含渲染提示的檔案頁數不可因 br-page 功能變動"
        );
    }

    /// 增量互動（軟分頁預算）：帶 br-page 的檔案編輯後，髒區間＋預算制背景
    /// 重排照走既有失效路徑，排完頁數與全速重建一致（斷點不繞過預算制）。
    #[test]
    fn br_page_edit_respects_sync_budget_and_settles() {
        const SI: &[u8] = include_bytes!("../../../corpus/ooxmlexport/style-inheritance.docx");
        let full = Engine::from_docx_bytes(SI).unwrap();
        let target = full.page_count();

        let mut eng = Engine::from_docx_bytes(SI).unwrap();
        eng.set_sync_budget(Some(2));
        let long_text = "本段被編輯：插入大量文字以擠壓版面行數。".repeat(30);
        eng.replace_paragraph_text(3, &long_text);
        // 未定案時也必須是合法版面（頁存在、可取顯示列表）。
        let mut guard = 0usize;
        while !eng.is_settled() {
            eng.reflow_step_json();
            guard += 1;
            assert!(guard < 1000, "背景重排必須有限步內定案");
        }
        assert!(eng.page_count() >= target, "編輯灌長後頁數只增不減");
        // 顯示列表全頁可取＝分頁結構自洽。
        for p in 0..eng.page_count() {
            assert!(
                eng.page_display_list_json(p).is_some(),
                "第 {p} 頁顯示列表缺失"
            );
        }
    }

    // ---------------- 樣式級聯返修（t_5b391cfd S1–S4＋測試缺口） ----------------

    /// S1＋量化佔帶＋break_before＋leader×br（fixture 四合一；形態由
    /// scripts/gen_style_cascade_fixture.py 落地）。
    /// - pPr/rPr 自閉合 `<w:b/>` 段（內容 run 零 rPr）→ 級聯必須把 FLAG_BOLD
    ///   送達 display list（修前 prun=None → flags 恆 0）；
    /// - 24pt Heading1 段行高 33.6pt > 帶尺 16.8pt → 佔 2 帶：大字段與緊鄰
    ///   下段的 y 距必須是 2×行高（量化佔帶路徑的產物鎖——3-1b 教訓：宣稱
    ///   「尺不變」只證了未觸發面，本斷言咬住已觸發面）；
    /// - pageBreakBefore 段必須從頁首（y＝top margin）起頭（break_before_of）；
    /// - 同段 leader＋br-page：填充不得展開（S3 案 a 誠實退化），且 br 後
    ///   段落從新頁起頭（斷點碼點不偏移）。
    #[test]
    fn style_cascade_fixture_bold_bands_break_and_leader_br() {
        const FX: &[u8] = include_bytes!("../../../fixtures/style_cascade.docx");
        let engine = Engine::from_docx_bytes(FX).expect("開啟 style_cascade.docx 失敗");
        let line_h = engine.params.line_h_pt();
        // 頁頂＝該頁 section 的 top margin（發射層 y 座標的原點，同頁必同值）。
        let top_of = |page: usize| engine.layout.pages()[page].margin_top_pt;

        // --- S1：段落級自閉合 b → display list flags ---
        let bold_item = (0..engine.page_count())
            .flat_map(|p| engine.page_display_list(p).expect("頁存在").items)
            .find_map(|it| match it {
                DrawItem::Text { text, flags, .. } if text == "段落級自閉合粗體" => {
                    Some(flags)
                }
                _ => None,
            })
            .expect("自閉合 b 段文字必須發射");
        assert_ne!(
            bold_item & FLAG_BOLD,
            0,
            "pPr/rPr 自閉合 <w:b/>（run 群零 rPr）級聯後必須帶 FLAG_BOLD"
        );
        // 對照：直接格式粗體綠字照舊（回歸面不破）。
        let direct = (0..engine.page_count())
            .flat_map(|p| engine.page_display_list(p).expect("頁存在").items)
            .find_map(|it| match it {
                DrawItem::Text {
                    text,
                    flags,
                    color_argb,
                    ..
                } if text == "直接格式粗體綠字" => Some((flags, color_argb)),
                _ => None,
            })
            .expect("直接格式段文字必須發射");
        assert_ne!(direct.0 & FLAG_BOLD, 0);
        assert_eq!(direct.1, 0xFF00_FF00);

        // --- 量化佔帶：24pt 大字段＋緊鄰下段 y 距＝2×行高 ---
        let all: Vec<(usize, f32, f32, String)> = (0..engine.page_count())
            .flat_map(|p| {
                engine
                    .page_display_list(p)
                    .expect("頁存在")
                    .items
                    .into_iter()
                    .filter_map(move |it| match it {
                        DrawItem::Text {
                            x,
                            y,
                            size_pt,
                            text,
                            ..
                        } => Some((p, y, x, (text, size_pt))),
                        _ => None,
                    })
                    .map(|(p, y, x, (text, size_pt))| (p, y, x, format!("{text}|{size_pt}")))
            })
            .collect();
        let (head_p, head_y, _, _) = *all
            .iter()
            .find(|(_, _, _, k)| k.starts_with("大字標題二十六磅|"))
            .expect("大字段必須發射");
        let (_, next_y, _, _) = all
            .iter()
            .filter(|(p, y, _, _)| *p == head_p && *y > head_y)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .expect("大字段同頁之後必有其他行");
        let gap = next_y - head_y;
        assert!(
            (gap - 2.0 * line_h).abs() < 0.5,
            "24pt 段行高 33.6pt→ceil(33.6/16.8)=2 帶：下段 y 距應 2×行高={}，實得 gap={gap} head_y={head_y} next_y={next_y}",
            2.0 * line_h
        );

        // --- break_before_of：pageBreakBefore 段從頁首起頭 ---
        let pb = all
            .iter()
            .find(|(_, _, _, k)| k.starts_with("強制換頁段從新頁起頭|"))
            .expect("pageBreakBefore 段必須發射");
        let first_on_page = all
            .iter()
            .filter(|(p, ..)| *p == pb.0)
            .map(|(_, y, _, _)| *y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            (pb.1 - first_on_page).abs() < 0.01,
            "pageBreakBefore 段必須是所在頁最頂行（y={}，頁頂={}）",
            pb.1,
            first_on_page
        );
        assert!(
            (pb.1 - top_of(pb.0)).abs() < 0.01,
            "頁首起頭＝margin_top_pt，實得 {}",
            pb.1
        );

        // --- leader 正常展開（同段無 br）：點引填充確實出現 ---
        let all_lines: Vec<String> = (0..engine.page_count())
            .flat_map(|p| engine.page_lines(p).unwrap_or_default())
            .collect();
        let leader_lines = all_lines.iter().find(|l| l.contains("目錄條目前綴"));
        assert!(
            leader_lines.is_some_and(|l| l.contains('.')),
            "無 br 的 leader 段必須照常展開點引填充（既有行為不破）：{:?}",
            leader_lines
        );

        // --- S3：同段 leader＋br-page → 不展開填充；br 後段從新頁起頭 ---
        let s3_lines: Vec<String> = (0..engine.page_count())
            .flat_map(|p| engine.page_lines(p).unwrap_or_default())
            .collect();
        let s3 = s3_lines
            .iter()
            .find(|l| l.contains("分頁段前綴"))
            .expect("br×leader 段必須發射");
        assert!(
            !s3.contains('.'),
            "page_breaks 非空段填充必須取消（S3 案 a），實得 {s3:?}"
        );
        let tail = all
            .iter()
            .find(|(_, _, _, k)| k.starts_with("分頁後的尾段|"))
            .expect("br 後段必須發射");
        let tail_first = all
            .iter()
            .filter(|(p, ..)| *p == tail.0)
            .map(|(_, y, _, _)| *y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            (tail.1 - tail_first).abs() < 0.01 && (tail.1 - top_of(tail.0)).abs() < 0.01,
            "br-page 後段必須從新頁頁首起頭（y={}）",
            tail.1
        );
    }

    /// S4：編輯級聯段後 paras_eff 必須同步——帶自閉合 b 的段整段置換後，
    /// 新文字仍帶 FLAG_BOLD（修前：舊 run 碼點區間套新文字 → flags 錯映／
    /// 填充永久丟失）。round-trip 鐵律不受影響（編輯走 layout，parts 不動）。
    #[test]
    fn edit_syncs_paras_eff_styles_and_round_trip() {
        const FX: &[u8] = include_bytes!("../../../fixtures/style_cascade.docx");
        let mut engine = Engine::from_docx_bytes(FX).expect("開啟失敗");
        while !engine.is_settled() {
            engine.reflow_step_json();
        }
        engine.replace_paragraph_text(0, "改後新文字");
        while !engine.is_settled() {
            engine.reflow_step_json();
        }
        assert_eq!(engine.paragraph_text(0).as_deref(), Some("改後新文字"));
        let edited = (0..engine.page_count())
            .flat_map(|p| engine.page_display_list(p).expect("頁存在").items)
            .find_map(|it| match it {
                DrawItem::Text { text, flags, .. } if text == "改後新文字" => Some(flags),
                _ => None,
            })
            .expect("編輯後新文字必須發射");
        assert_ne!(
            edited & FLAG_BOLD,
            0,
            "編輯後段落仍承載段落級級聯粗體（paras_eff 未同步時此處歸 0）"
        );
        // round-trip：parts passthrough——發射投影 edits 不得污染存檔。
        let saved = engine.save_docx().expect("存檔");
        assert_eq!(saved.as_slice(), FX, "編輯發射投影後存檔仍須位元組級相同");
    }

    /// S2：病態數值（sz=4e12 半磅、line=999999999、sz="1e400"→inf）——
    /// 開啟不 panic（debug 建置帶整數溢出檢查）、頁數確定（兩次開啟一致、
    /// 全速與預算制收斂同頁數）、全頁顯示列表可取、重排有限步定案。
    #[test]
    fn pathological_sizes_are_stable() {
        const PX: &[u8] = include_bytes!("../../../fixtures/style_pathological.docx");
        let a = Engine::from_docx_bytes(PX).expect("病態檔開啟不可 panic");
        let b = Engine::from_docx_bytes(PX).expect("重開不可 panic");
        assert_eq!(a.page_count(), b.page_count(), "頁數須確定性");
        assert!(a.page_count() >= 1, "至少一頁（病態段不得吞掉版面）");
        for p in 0..a.page_count() {
            assert!(
                a.page_display_list_json(p).is_some(),
                "第 {p} 頁顯示列表缺失"
            );
        }
        // 增量重排路徑（cur_used saturating 的第二條入口）：編輯後必有限步定案。
        let mut c = Engine::from_docx_bytes(PX).unwrap();
        c.set_sync_budget(Some(1));
        c.replace_paragraph_text(0, "病態檔上的編輯");
        let mut guard = 0usize;
        while !c.is_settled() {
            c.reflow_step_json();
            guard += 1;
            assert!(guard < 1000, "病態檔背景重排必須有限步內定案");
        }
        for p in 0..c.page_count() {
            assert!(c.page_display_list_json(p).is_some());
        }
    }
}
