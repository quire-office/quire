//! DOCX → 分頁輸入管線（觀測工具移植版）。
//!
//! 本模組移植 `core/engine-api/src/lib.rs::from_docx_bytes` 的**輸入準備段**：
//! 樣式級聯（逐段量測字級／段行高／有效 LineSpacing）、逐段 section 投影、
//! 點引（tab leader）展開、br-page 斷點、表格槽與圖片帶——與引擎開啟文件
//! 時組出的分頁輸入逐欄等價，因此本工具跑 `LayoutEngine` 得到的每一頁
//! 就是真實分頁結果。不改變任何渲染行為：這裡只是唯讀副本，引擎照走原路。
//! **引擎側動了 from_docx_bytes／cascade_paragraphs 時，本檔必須同步**。
//!
//! 分層說明：本工具 crate 直接依賴 quire-ooxml＋quire-layout（如 engine 同款
//! 三段式），不經 quire-engine——engine 的 crate 邊界在分層鐵律下只輸出
//! 顯示列表／存檔/round-trip 面，不輸出分頁輸入，故工具端改為同構重放。

use quire_layout::{
    ImageBand, LayoutEngine, LayoutParams, Page, ParaGrid, SectionGeometry, SectionSpec, TableSlot,
    TextMeasurer, MAX_PAGE_BANDS,
};
use quire_ooxml::{
    parse, read_images, read_sections_per_paragraph, LineSpacing, OoxmlError, ParaStyle, Paragraph,
    StyleTable, ThemeColors,
};

use crate::tables;

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

/// M1-band（簇 A，同步引擎側 `refine_band_h`）：級聯有宣告時把粗帶尺細化成
/// 宣告行高的 ≤2.8pt 最細等分。引擎側不動本工具就報不出細化後的真實帶制。
/// 回傳（細化帶尺, n）：n＝宣告行高的等分份數＝細尺下一行文字應佔帶數
/// （與引擎側 `refine_band_h` 同款；未細化＝1）。
fn refine_band_h(declared_line_h: f32, declared: bool) -> (f32, u32) {
    if !declared || !declared_line_h.is_finite() || declared_line_h <= 0.0 {
        return (declared_line_h, 1);
    }
    let n = (declared_line_h / BAND_REFINE_MAX_PT).ceil().max(1.0);
    (declared_line_h / n, n as u32)
}

/// 帶尺細化上限（pt；與引擎側 `BAND_REFINE_MAX_PT` 同值鏡像）。
const BAND_REFINE_MAX_PT: f32 = 2.8;

/// 分頁輸入束（＝engine 開檔時餵 `LayoutEngine` 的同一組輸入＋級聯衍生的
/// 逐段風格註記——報告的 line_rule／font_size 欄由此輸出）。
pub struct DocInputs {
    /// 版面文字（級聯路徑含點引填補——斷行與發射同源）。
    pub texts: Vec<String>,
    /// 基準版面參數（字級/行距吃級聯文件基準；頁面欄位由首 section 覆寫）。
    pub params: LayoutParams,
    /// 逐段落所屬 section 的識別與頁面幾何（sectPr 的權威投影）。
    pub sections: Vec<SectionSpec>,
    /// 已排版表格槽（canonical 樹映射；依 para_start 排序）。
    pub tables: Vec<TableSlot>,
    /// 正文流圖片佔位錨點（inline／退化 anchor；浮動鎖點者不在其內）。
    pub images: Vec<ImageBand>,
    /// 逐段落 br-page 硬斷點（碼點偏移；`Paragraph::page_breaks` 直傳）。
    pub breaks: Vec<Vec<usize>>,
    /// 逐段落字級/行帶表（級聯量化；分頁佔帶就用這組數）。
    pub grid: ParaGrid,
    /// 逐段有效 [`LineSpacing`]（None＝全鏈未宣告——報告老實標 unknown）。
    pub para_line: Vec<Option<LineSpacing>>,
    /// 逐段行高（pt，量化前原值）——帶數＝ceil(本值／帶尺)。報告的
    /// `line_h_prequant_pt` 取本欄（與量化後的 `height_pt` 併陳才有診斷力）。
    pub para_line_h: Vec<f32>,
    /// 逐段級聯有效段前／段後距（pt；None＝全鏈未宣告）。
    pub spacings: Vec<(Option<f32>, Option<f32>)>,
}

/// 逐段級聯＋烘焙（正文輸入共用路徑；移植自
/// `core/engine-api/src/lib.rs::cascade_paragraphs`）。
/// 回傳 (發射投影——run 樣式換成級聯有效值, 逐段 (量測字級, 段行高 pt)，
/// 逐段有效 LineSpacing，逐段有效段距 (before, after) pt)。`def_size`＝
/// 文件基準字級；`widths`＝逐段可用寬
/// （點引填補的計算尺）。無 styles.xml 時仍給一張空表走級聯兜底
/// （超連結合成預設），與引擎同徑。
#[allow(clippy::type_complexity)]
fn cascade_paragraphs(
    st: &StyleTable,
    paras: &[Paragraph],
    def_size: f32,
    widths: &[f32],
) -> (
    Vec<Paragraph>,
    Vec<(f32, f32)>,
    Vec<Option<LineSpacing>>,
    Vec<(Option<f32>, Option<f32>)>,
) {
    let mut eff_all: Vec<Paragraph> = Vec::with_capacity(paras.len());
    let mut metrics: Vec<(f32, f32)> = Vec::with_capacity(paras.len());
    let mut para_lines: Vec<Option<LineSpacing>> = Vec::with_capacity(paras.len());
    let mut spacings: Vec<(Option<f32>, Option<f32>)> = Vec::with_capacity(paras.len());
    for (pi, p) in paras.iter().enumerate() {
        let inline = ParaStyle {
            run: p.prun.clone().unwrap_or_default(),
            line: p.line,
            spacing_before_pt: p.spacing_before_pt,
            spacing_after_pt: p.spacing_after_pt,
            align: p.jc,
        };
        let (base, para) = st.effective_para(p.style.as_deref(), inline);
        let mut meas_size = base.size_pt.unwrap_or(def_size);
        let mut eff_runs: Vec<quire_ooxml::Run> = Vec::with_capacity(p.runs.len());
        for r in &p.runs {
            let mut b = base.clone();
            if let Some(cid) = r.rstyle.as_deref() {
                b = StyleTable::resolve_run(b, st.char_chain(cid).unwrap_or_default());
            }
            // 級聯末端兜底：只有整條鏈都沒宣告顏色、底線時，超連結 run 才
            // 補合成預設（ECMA-376 §17.7.2 級聯末端）。
            let eff = StyleTable::resolve_run(b, r.direct.clone().unwrap_or_default())
                .with_hyperlink_default(r.hyperlink_default);
            // 斷行字級＝段內 run 有效字級最大值（Word 行高/斷行依段內最大
            // 字級成形；本引擎斷行用單一尺，取大者保守不切字）。
            if let Some(sz) = eff.size_pt {
                meas_size = meas_size.max(sz);
            }
            let mut r2 = r.clone();
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
        // 點引 leads 展開：段落宣告右對齊製表位＋leader＋內含
        // w:tab run 時，於首個 tab run 前插入填充符串至「製表位 − 尾串寬」
        // （Word/LO 同款視覺 `Heading 1 ......... 2`）。填充是獨立 run——
        // texts 由 eff run 串接重建，run↔line 碼點對齊天然不破。斷行量測
        // 尺＝meas_size（與 break_lines 同尺，填充寬計算與斷行一致）。
        // widths 缺位＝不展開——無可用寬不虛構。
        // page_breaks 非空＝不展開（t_5b391cfd S3 案 a）：breaks 直讀原始
        // 文字碼點偏移，填充讓 texts 碼點軸右移，斷點
        // 位置即與文字錯位。誠實退化：寧可少畫點引，不可錯斷頁。
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
        para_lines.push(para.line);
        spacings.push((para.spacing_before_pt, para.spacing_after_pt));
        let mut ep = p.clone();
        ep.runs = eff_runs;
        eff_all.push(ep);
    }
    (eff_all, metrics, para_lines, spacings)
}

/// DOCX 原始位元組 → 分頁輸入束（逐欄移植 engine `from_docx_bytes` 的
/// 輸入準備段；頁首／存檔／round-trip 相關屬 engine 私事，本工具不碰）。
pub fn from_docx_bytes(bytes: &[u8]) -> Result<DocInputs, OoxmlError> {
    let doc = parse(bytes)?;
    // 樣式級聯層：開啟時一次性解析 styles.xml（無 part 仍給空表＝超連結
    // 兜底與後備值路徑與引擎逐欄同徑）。
    let theme = ThemeColors::from_parts(&doc.parts);
    let style_table = doc
        .parts
        .get("word/styles.xml")
        .map(|s| StyleTable::parse_with_theme(s.as_slice(), &theme))
        .unwrap_or_else(|| StyleTable::parse_with_theme(&[], &theme));
    let (doc_size, doc_line) = style_table.doc_grid();
    // 文件基準字級：docDefaults∪Normal 鏈的有效 sz（ECMA 無宣告時 Word
    // 預設 12pt 兜底）。
    let def_size = doc_size.unwrap_or(FALLBACK_SIZE_PT);
    // 文件帶尺（行網格量化單位）＝預設段落行高；無宣告＝12pt×1.4。
    // M1-band：有宣告且**含表格**時細化（同步引擎 from_docx_bytes 同款閘門：
    // 處方根治 trHeight／格內宣告 pt 高 ceil 粗帶的過扣；純文字檔細化會把
    // LO auto 行距 pitch 保真缺口（K6 權限）暴露成分數位移——run6 A/B 實證）。
    let (grid_line_h, line_ref_bands) = refine_band_h(
        resolve_line_h_pt(def_size, doc_line),
        (doc_size.is_some() || doc_line.is_some()) && !doc.tables.is_empty(),
    );
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
                        // L1-columns：ooxml→layout 逐欄對應（同 engine-api 口徑）。
                        cols: quire_layout::SectionCols {
                            num: s.geo.cols.num,
                            space_pt: s.geo.cols.space_pt,
                            col_w_pt: s.geo.cols.col_w_pt,
                            col_space_pt: s.geo.cols.col_space_pt,
                        },
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    // 基準 params：字級/行距吃級聯文件基準，再覆寫首 section 的頁面欄位。
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
    // 逐段可用寬＝該段 section 的 content_w（點引填補計算尺）。
    let widths: Vec<f32> = (0..n_paras)
        .map(|i| match sections.get(i) {
            Some(s) => LayoutParams::from_section(&s.geo, params.font_size_pt, params.line_spacing)
                .content_w_pt(),
            None => params.content_w_pt(),
        })
        .collect();
    // 級聯烘焙。
    let (eff_all, metrics, para_lines, spacings) =
        cascade_paragraphs(&style_table, &doc.paragraphs, def_size, &widths);
    let grid_size_v: Vec<f32> = metrics.iter().map(|m| m.0).collect();
    // 段行高量化到文件帶尺（至少 1 帶；鉗上限＝分頁帶累加溢出護欄同款）。
    let grid_bands_v: Vec<u32> = metrics
        .iter()
        .map(|m| {
            let b = (m.1 / grid_line_h).ceil();
            if !b.is_finite() {
                MAX_PAGE_BANDS
            } else {
                ((b.max(1.0)) as u32).min(MAX_PAGE_BANDS)
            }
        })
        .collect();
    // 版面文字：以發射投影為準（級聯路徑含點引填補）。
    let texts: Vec<String> = eff_all
        .iter()
        .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
        .collect();
    // 圖片投影（唯讀）＋表格槽映射（canonical 樹＋styles.xml 框線查表）。
    let images_xml = doc
        .parts
        .get("word/document.xml")
        .map(|xml| read_images(xml))
        .unwrap_or_default();
    let styles_xml = doc.parts.get("word/styles.xml").map(|s| s.as_slice());
    let eff_refs: &[Paragraph] = &eff_all;
    let slots = tables::build_slots(
        &doc.tables,
        &texts,
        styles_xml,
        &params,
        &sections,
        &images_xml,
        eff_refs,
        line_ref_bands,
    );
    let bands = tables::build_images(&images_xml, params.line_h_pt(), &params, &sections);
    // br-page 硬斷點（ECMA-376 §17.4.3）：canonical 段落偏移直傳版面層。
    let breaks: Vec<Vec<usize>> = doc
        .paragraphs
        .iter()
        .map(|p| p.page_breaks.clone())
        .collect();
    // 顯式分頁旗標（`w:pageBreakBefore`；br-page 走 breaks 通道，不一欄兩用）。
    let grid_break_v: Vec<bool> = doc.paragraphs.iter().map(|p| p.page_break).collect();
    // 段距佔帶：與 engine-api `spacing_band_vectors` 同式——跨段累積餘數
    // 量化（逐段 ceil 會過度擠壓）；K1-R：餘數鏈在**表格邊界斷開**
    // （表格高度由表格幾何獨立計算，段落間距的垂直連續性在表格邊界中斷），
    // 且**表格塊之前一段的段後距不另佔帶**（段落與表格之間的空白由表格
    // 塊的前距承擔，不重複佔位）——與 engine-api 完全同式。
    let (grid_before_v, grid_after_v) = {
        let (doc_before, doc_after) = style_table.doc_spacing();
        if doc_before.is_some() || doc_after.is_some() {
            let mut before = Vec::with_capacity(spacings.len());
            let mut after = Vec::with_capacity(spacings.len());
            let line_h = params.line_h_pt();
            if !line_h.is_finite() || line_h <= 0.0 {
                before.resize(spacings.len(), 0);
                after.resize(spacings.len(), 0);
            } else {
                let mut sp = spacings.clone();
                let mut chain_breaks: Vec<usize> = Vec::new();
                for bt in &doc.tables {
                    let end = tables::table_para_end(&bt.table, bt.para_start).min(sp.len());
                    chain_breaks.push(end);
                    if let Some(prev) = bt.para_start.checked_sub(1) {
                        if prev < sp.len() {
                            sp[prev].1 = None;
                        }
                    }
                }
                let mut residual = 0.0f64;
                for (i, (b, a)) in sp.iter().enumerate() {
                    // 鏈段起點：餘數歸零（表格塊之後的第一段起算新鏈段）。
                    if chain_breaks.contains(&i) {
                        residual = 0.0;
                    }
                    let mut bands = [0u32; 2];
                    for (k, pt) in [*b, *a].iter().enumerate() {
                        let Some(v) = pt else { continue };
                        if !v.is_finite() || *v <= 0.0 {
                            continue;
                        }
                        let x = residual + (*v as f64 / line_h as f64);
                        let n = x.floor();
                        residual = x - n;
                        bands[k] = (n.max(0.0) as u32).min(quire_layout::MAX_PAGE_BANDS);
                    }
                    before.push(bands[0]);
                    after.push(bands[1]);
                }
            }
            (before, after)
        } else {
            (vec![0; spacings.len()], vec![0; spacings.len()])
        }
    };
    let grid = ParaGrid {
        size_pt: grid_size_v.clone(),
        bands: grid_bands_v,
        break_before: grid_break_v,
        before_bands: grid_before_v,
        after_bands: grid_after_v,
    };
    Ok(DocInputs {
        texts,
        params,
        sections,
        tables: slots,
        images: bands,
        breaks,
        grid,
        para_line: para_lines,
        para_line_h: metrics.iter().map(|m| m.1).collect(),
        spacings,
    })
}

/// 由輸入束驅動 [`LayoutEngine`]（與 engine 同構）取得每頁結果。
pub fn layout_pages(inputs: &DocInputs) -> Vec<Page> {
    let engine = LayoutEngine::with_breaks_grid(
        &inputs.texts,
        inputs.params.clone(),
        inputs.sections.clone(),
        inputs.tables.clone(),
        inputs.images.clone(),
        inputs.breaks.clone(),
        inputs.grid.clone(),
    );
    engine.pages().to_vec()
}

/// 逐段落斷行行起點（碼點偏移）——報告的 line_in_para 對位尺。
/// 與 `LayoutEngine::break_para` 同徑：段字級取 grid（缺＝全域後備），
/// 版心寬取該段 section；段中 br 偏移走 `break_lines_at`（KP 硬收行）。
pub fn para_line_char_offsets(inputs: &DocInputs) -> Vec<Vec<usize>> {
    inputs
        .texts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let content_w = match inputs.sections.get(i) {
                Some(s) => LayoutParams::from_section(
                    &s.geo,
                    inputs.params.font_size_pt,
                    inputs.params.line_spacing,
                ),
                None => inputs.params.clone(),
            }
            .content_w_pt();
            let size = inputs.grid.size_of(i, inputs.params.font_size_pt);
            let m = TextMeasurer::new(size);
            let brs = inputs.breaks.get(i).cloned().unwrap_or_default();
            let total = t.chars().count();
            let inner: Vec<usize> = brs.into_iter().filter(|&o| o > 0 && o < total).collect();
            let lines = if inner.is_empty() {
                quire_layout::break_lines(t, content_w, &m)
            } else {
                quire_layout::break_lines_at(t, content_w, &m, &inner)
            };
            lines.into_iter().map(|l| l.char_offset).collect()
        })
        .collect()
}
