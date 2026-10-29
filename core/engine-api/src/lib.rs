//! quire-engine —— shells 唯一准 import 的契約層（見 docs/雙殼架構規劃）。
//!
//! 規則（CI 鎖死，scripts/check-layering.sh）：
//! - shells/* 只能依賴本 crate；
//! - 本 crate 及其下全部 core 必須在 wasm32-unknown-unknown 可編譯；
//! - 型別全 serde 化，跨 wasm 邊界走 JSON 字串。

use quire_layout::{
    breaks_from_pages, section_capacity, CaretPos, LayoutEngine, LayoutParams, Page, ParaGrid,
    SectionGeometry, SectionSpec, TableSlot, TextMeasurer, MARGIN_BOTTOM, MARGIN_LEFT, MARGIN_TOP,
};
use quire_ooxml::{
    diff_untouched, diff_zip_structure, parse, parse_header_footer_part_with_fonts,
    read_even_and_odd_headers, read_rels, read_sections_header_footer, read_sections_note_pr,
    read_sections_per_paragraph, resolve_relationship, LineSpacing, OoxmlDoc, OoxmlError,
    ParaStyle, RunFonts, SlotRefs, StyleTable, ThemeColors, ThemeFonts, ZipDiff,
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
// 清單符號發射層（K2b）：numPr → 合成 marker（投影層，存檔零影響）。
mod lists;
pub use quire_layout::table::{Pen, RowFrag, TableFrag};

// 頁底註腳層（F2b-2）：幾何與排布純函數（消費 F1 資料層＋F2a/F2b-1 預留機制）。
pub mod footnotes;
/// H2／F3b 共用的編號消費地基（純函式：numFmt 字面、numRestart 重啟、
/// settings/sectPr 逐欄合併）。消費接线在註腳/尾註兩條鏈各自的卡。
pub mod numbering;
pub use footnotes::{AnchorMark, NoteLayout, NoteRun, PageFoot};
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
    /// styles.xml 級聯表（F2b-2 註腳級聯取樣用；純讀取派生）。
    style_table: StyleTable,
    /// 頁底註腳發射層（F2b-2）：收斂定案後的逐頁排布＋正文錨點記號。
    /// 純讀取派生資料——存檔路徑完全不經此層（round-trip 零影響）。
    /// 文件無註腳 ⇒ `is_empty()`＝發射層零擾動快路徑。
    foot: footnotes::NoteLayout,
    /// 註腳條目（級聯烘焙後的發射級樣式；序＝`note_of_anchor` 的值域）。
    note_entries: Vec<footnotes::NoteEntry>,
    /// 正文錨點（發射投影序）與其到條目的映射、記號樣式。
    note_anchors: Vec<footnotes::Anchor>,
    note_of_anchor: Vec<usize>,
    mark_styles: Vec<footnotes::MarkStyle>,
    /// 尾註發射層（F3／J1R）：正文末頁接排條目＋純尾註續頁＋正文錨點記號。
    /// 純讀取派生資料——存檔路徑完全不經此層（round-trip 零影響）。
    /// 文件無尾註 ⇒ `is_empty()`＝發射層零擾動快路徑。
    end: footnotes::EndLayout,
    /// 尾註條目（級聯烘焙後的發射級樣式；序＝`end_note_of_anchor` 的值域）。
    end_note_entries: Vec<footnotes::NoteEntry>,
    /// 尾註正文錨點（發射投影序）與其到條目的映射、記號樣式。
    end_note_anchors: Vec<footnotes::Anchor>,
    end_note_of_anchor: Vec<usize>,
    end_mark_styles: Vec<footnotes::MarkStyle>,
    /// 節幾何全表（section id → 幾何）：頁首頁尾層的 `sec_geo` **只在檔案
    /// 宣告頁首頁尾時才有值**（無宣告＝空表），尾註頁的**末節**幾何不能靠
    /// 它——另存一份全表（純讀取派生，存檔路徑不經）。
    sec_geo_all: HashMap<u32, SectionGeometry>,
    /// 段落 → 所屬 section id（`read_sections_per_paragraph` 的 id 投影；
    /// 空表＝檔案無任何 sectPr，呼叫端退回「無節域」語義）。H2／F3b 的
    /// 節域編號（numRestart=eachSect／節級 numFmt 覆寫）由此查錨點所在節。
    para_sec: Vec<u32>,
    /// 文字框佔位投影（K4e2）：段落索引 → 該段掛著的 shape 框（含文字框）。
    /// 純讀取派生（存檔路徑不經）；空表＝檔案無圖形＝發射層零擾動快路徑。
    shape_boxes: Vec<(usize, quire_ooxml::InlineImage)>,
    /// 圖片像素池（K4c-image）：正文流／浮動／格內圖中**開檔期已解析出
    /// 可渲染像素**的候選（配對鍵＝宿主段落＋extent 寬，與 K4e2 文字框池
    /// 同議——高被帶高鉗制不可作鍵）。消費式配對：每頁一份旗標、用完即
    /// 標記，正文流/浮動/格內切片三路徑共用一池。純讀取派生——存檔路徑
    /// 完全不經此欄（嚴禁把 media part 寫回 docx）。頁首尾圖的像素解析
    /// 另存 `HfPart::pixels`（HF part 有自家的 `.rels`）。
    image_pixels: Vec<PixelSource>,
    /// M1-band（簇 A）：細尺下「格內一行文字」應佔的帶數＝宣告行高的等分
    /// 份數（`refine_band_h` 的 n；未細化＝1＝舊「一行＝一帶」口徑）。
    /// 表格映射層（`table_map::build_slots`）消費；純讀取派生——存檔路徑
    /// 完全不經本欄。
    line_ref_bands: u32,
}

/// K4c-image 像素池項（開檔期一次性解析；像素位元組本身**不複製**進
/// 引擎——只記來源 part 路徑，發射時才從 `doc.parts` 取位元組編 base64，
/// 開檔成本與檔案大小同尺、不雙倍佔内存）。
#[derive(Debug, Clone)]
struct PixelSource {
    /// 宿主段落（扁平段落索引；配對鍵之一）。
    para: usize,
    /// extent 寬（pt；配對鍵之二）。
    w_pt: f32,
    /// 像素來源（`word/media/*` part 路徑＋MIME）。
    src: (String, String),
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
    /// 本 part 的圖片幾何投影（`quire_ooxml::read_images` 原樣複用）。
    /// 空表＝無圖＝發射層零變化（K4a）；發射只做佔位框，不碰存檔路徑。
    images: Vec<quire_ooxml::InlineImage>,
    /// K4c-image：與 `images` **逐位對齊**的像素解析結果（Some＝該候選已
    /// 解出可渲染像素的 part 路徑＋MIME；None＝查無/不可渲染格式→佔位框）。
    /// rId 走本 part 自己的 `.rels`（HF part 有自家 rels，不是
    /// document.xml.rels——Bug51170 實測）。空表＝無圖（零變化守門）。
    pixels: Vec<Option<(String, String)>>,
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
        // 主題色表（word/theme/theme1.xml 的 clrScheme）：純讀取派生資料——
        // `w:themeColor` 查表用（ECMA-376 §17.3.2.25：主題色優先於 `w:val`
        // 回退值）。無 theme part → 空表＝老實退回 `w:val`（既有行為不變）。
        // 存檔路徑完全不經此層（parts passthrough，round-trip 零影響）。
        let theme = ThemeColors::from_parts(&doc.parts);
        // 主題字型表（theme1.xml 的 fontScheme）：純讀取派生資料，供 run 的
        // `w:asciiTheme`／`w:eastAsiaTheme` 引用查表（ECMA-376 §17.3.2.26）。
        // 無 theme part → 四欄全 None＝老實退回直接值（既有行為不變）。
        let theme_fonts = ThemeFonts::from_parts(&doc.parts);
        // 無 styles.xml part 時仍建一張空表而非 None：級聯層除了套用樣式，
        // 還承載「超連結合成預設」的末端兜底（`RunStyle::with_hyperlink_default`）；
        // 若此處短路，無 styles.xml 的檔案連結會失去預設藍／底線。空表一切
        // 落後備值，與舊 None 路徑逐欄等價。
        // K6：styles.xml 必須經 `word/_rels/document.xml.rels` 的 styles 關聯
        // 才生效（校準報告 §3(2) 鐵證；無關聯時 LO 連 docDefaults 都忽略）。
        let style_table = if styles_referenced(&doc.parts) {
            doc.parts
                .get("word/styles.xml")
                .map(|s| StyleTable::parse_with_fonts(s.as_slice(), &theme, Some(&theme_fonts)))
                .unwrap_or_else(|| StyleTable::parse_with_fonts(&[], &theme, Some(&theme_fonts)))
        } else {
            StyleTable::parse_with_fonts(&[], &theme, Some(&theme_fonts))
        };
        let (doc_size, doc_line) = style_table.doc_grid();
        // 文件基準字級：docDefaults∪Normal 鏈的有效 sz（ECMA 無宣告時的
        // Word 預設 12pt 兜底）。無級聯層＝12（既有硬編碼後備）。
        let def_size = doc_size.unwrap_or(12.0);
        // 文件帶尺（行網格量化單位）＝預設段落的行高；無宣告＝12pt×1.4，
        // 與舊 `LayoutParams::default()` 恆等（零宣告檔案逐位元組不變）。
        // M1-band（簇 A 分頁容量）：級聯**有宣告**（基準字級或預設行高任一）
        // 時帶尺細化——粗帶 ceil 是過扣統一根因（15pt 表格列在 12.65pt 帶上
        // 吃 2 帶＝25.3pt，LO 15.0，×1.69；K5-diag §2.6 三檔鐵證）。細化尺＝
        // 宣告行高的 ≤2.8pt 最細等分，宣告行高本身恰佔整數帶（零誤差），
        // 其餘高度量化誤差 ≤2.8pt。零宣告檔（如 k5 防風暴樁）維持
        // 12pt×1.4＝16.8pt 粗尺——零宣告檔案逐位元組不變。
        // K6：細化閘由「含表格檔」擴展到「檔內存在宣告字級／宣告行距可
        // 依據」的檔案（docDefaults／Normal 鏈或段落直接宣告皆算）。M1
        // 已證含表格檔的 trHeight／格內行高需要細尺；純文字檔的 auto
        // 行距基準字級（K6 起改用段內 run 有效字級×宣告倍率）同樣需要
        // 細尺承載，否則 12.65pt 粗帶的 ceil 會把 LO pitch 抬成整帶倍數。
        // 零宣告檔（doc_size/doc_line 皆無、段落亦無直接宣告）維持
        // 12pt×1.4＝16.8pt 粗尺——M1 零宣告守門樁逐位元組不變。
        let any_para_declared = doc.paragraphs.iter().any(|p| {
            (p.line.is_none()
                && (p.prun.as_ref().and_then(|r| r.size_pt).is_some()
                    || p.runs
                        .iter()
                        .any(|r| r.direct.as_ref().and_then(|d| d.size_pt).is_some())))
                || matches!(p.line, Some(LineSpacing::Auto(_)))
        });
        let refine = doc_size.is_some() || doc_line.is_some() || any_para_declared;
        let doc_grid_h = resolve_line_h_pt(def_size, doc_line);
        // 佔位值供級聯前的可用寬／params 使用；級聯後以主要宣告行高重錨。
        let (grid_line_h, _) = refine_band_h(doc_grid_h, refine);
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
                            // L1-columns：ooxml→layout 逐欄對應（分層：兩側
                            // SectionCols 同構雙定義，此處是唯一接線點）。
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
        // 逐段可用寬＝該段 section 的**斷行寬**（L1-columns：多欄 section＝
        // 首欄寬，與 layout 斷行同口徑；無 section 表＝基準 params）。
        let widths: Vec<f32> = (0..n_paras)
            .map(|i| quire_layout::section_break_w_pt(&params, &sections, i))
            .collect();
        // 顯式分頁旗標（parser 直讀，任何檔案都收；全 false＝舊行為）。
        let grid_break_v: Vec<bool> = doc.paragraphs.iter().map(|p| p.page_break).collect();
        // 逐段落所屬 section id（清單 eachSect 重啟與尾註編號共用）。
        let para_sec_v: Vec<u32> = sections.iter().map(|s| s.id).collect();
        // K2b：清單 marker 規劃。無 `w:numPr` 引用 ⇒ 空表（零變化：不插
        // marker、不動任何投影欄位、不解析 numbering.xml）。
        let list_markers = {
            let refs = quire_ooxml::paragraph_numbering_from_parts(&doc.parts);
            if refs.is_empty() {
                Vec::new()
            } else {
                let numbering = quire_ooxml::Numbering::from_parts(&doc.parts);
                lists::compute_markers(&numbering, &refs, &para_sec_v, n_paras)
            }
        };
        // 發射投影：Some＝級聯有作用（run 樣式欄位已烘焙）。style_table 恆有
        // （無 styles.xml＝空表兜底超連結預設，見上），故級聯恆執行——
        // 空表一切落後備值，輸出與舊 None 路徑逐欄等價（t_5dfc8ac7 契約）。
        let doc_mult = match doc_line {
            Some(LineSpacing::Auto(m)) => m,
            _ => 1.0,
        };
        // K6：字型表可用性＝文件任何層宣告過字型。沒有字型宣告的檔案維持
        // 舊 1.4 粗尺（M1 零宣告守門／k5 Exact 防風暴樁逐位元組不變）。
        let font_metrics = style_table.has_fonts()
            || doc.paragraphs.iter().any(|p| {
                p.prun.as_ref().and_then(|r| r.fonts.as_ref()).is_some()
                    || p.runs.iter().any(|r| {
                        r.direct.as_ref().and_then(|d| d.fonts.as_ref()).is_some()
                            || r.effective_fonts.is_some()
                    })
            });
        let auto_buggy = std::env::var_os("K6_AUTO_PITCH_BUGGY").is_some();
        let calibrated = font_metrics && !auto_buggy;
        let mut table_styles_of: HashMap<usize, String> = HashMap::new();
        for bt in &doc.tables {
            collect_table_styles(&bt.table, None, &mut table_styles_of);
        }
        let (eff_all, metrics, spacings, marker_chars) = cascade_paragraphs(
            &style_table,
            &doc.paragraphs,
            def_size,
            doc_mult,
            &widths,
            &list_markers,
            PitchCascade {
                table_styles: &table_styles_of,
                calibrated,
            },
        );
        let grid_size_v: Vec<f32> = metrics.iter().map(|m| m.0).collect();
        // K6：逐段量化前行高（格內文字行距基準；細化檔由 table_map 消費）。
        let mut para_line_h_v: Vec<f32> = metrics.iter().map(|m| m.1).collect();
        let (grid_line_h, line_ref_bands) = if calibrated {
            // K6：宣告字級／字型檔走校準 pitch。單一行高＝M1 的 ≤2.8pt 等分；
            // 多行高（混級、表格多字級）搜尋能同時表示全部目標的細尺。
            choose_grid_line_h(&para_line_h_v).unwrap_or_else(|| refine_band_h(doc_grid_h, refine))
        } else {
            // 舊路徑（K6_AUTO_PITCH_BUGGY=1 或全檔無字型宣告）：保留
            // 51e2e71 的粗尺細化行為，供紅燈回塞鎖比對。零宣告但有字型的
            // 檔，舊徑的 ×1.4 行高就是帶尺（pitch 15.4，留下紅證據）。
            if font_metrics && doc_line.is_none() && doc_size.is_none() && doc.tables.is_empty() {
                let target = modal_line_h(&para_line_h_v, 0.0);
                refine_band_h(target, true)
            } else {
                let modal_h = if refine {
                    modal_line_h(&para_line_h_v, doc_grid_h)
                } else {
                    0.0
                };
                if refine {
                    for (i, p) in doc.paragraphs.iter().enumerate() {
                        let inline = ParaStyle {
                            run: p.prun.clone().unwrap_or_default(),
                            line: p.line,
                            spacing_before_pt: p.spacing_before_pt,
                            spacing_after_pt: p.spacing_after_pt,
                            align: p.jc,
                        };
                        let eff_line = style_table
                            .effective_para(p.style.as_deref(), inline)
                            .1
                            .line;
                        if matches!(eff_line, Some(LineSpacing::Auto(_))) {
                            para_line_h_v[i] = para_line_h_v[i].max(modal_h);
                        }
                    }
                }
                let anchor_h = modal_h.max(modal_line_h(&para_line_h_v, doc_grid_h));
                let anchor_is_grid_multiple = {
                    let r = anchor_h / doc_grid_h;
                    r.is_finite() && (r - r.round()).abs() * doc_grid_h <= 0.11
                };
                let keep_coarse = doc.tables.is_empty() && anchor_is_grid_multiple;
                if refine && !keep_coarse {
                    refine_band_h(anchor_h, true)
                } else {
                    refine_band_h(doc_grid_h, false)
                }
            }
        };
        let params = LayoutParams {
            font_size_pt: def_size,
            line_spacing: if def_size > 0.0 {
                grid_line_h / def_size
            } else {
                1.4
            },
            ..params
        };
        // 段行高量化到文件帶尺（至少 1 帶；帶尺＝12pt×1.4 且段行高不大於
        // 帶尺時恆 1＝舊行為）。
        let grid_bands_v: Vec<u32> = para_line_h_v
            .iter()
            // 帶量化鉗上限（t_c1fb4c69 S1-4）：垃圾宣告行高不得量化出
            // 天量帶數（正常段行高 ≤ 數帶，永不觸頂）。
            .map(|h| {
                // K6 校準路徑取最近帶（pitch 保真）；舊徑維持 ceil（保守
                // 不下壓段高，回塞鎖才能重現舊分頁）。
                let b = if calibrated {
                    (*h / grid_line_h).round()
                } else {
                    (*h / grid_line_h).ceil()
                };
                if !b.is_finite() {
                    quire_layout::MAX_PAGE_BANDS
                } else {
                    ((b.max(1.0)) as u32).min(quire_layout::MAX_PAGE_BANDS)
                }
            })
            .collect();
        // K1 段前／段後距量化帶（未宣告恆 0＝舊分頁路徑逐位元組不變）。
        // 逐段 `ceil` 會把 8pt 距在 11.87pt 帶尺上高估成整帶，全文件累積後
        // 過度擠壓頁面（reader-styles 由 3 頁變 4 頁）；故以「跨段累積餘數」
        // 量化：整數帶數＝floor(Σpt／帶尺)，小數餘數留給下一個距——總佔位
        // 回到精確 pt 的帶量化近似，同時仍維持整數帶制。
        // 主開關（K4b）：docDefaults 有宣告 ⇒ 照 K1 舊徑開帶制（零變化）；
        // docDefaults 無宣告但**預設段落樣式鏈**（Normal）有有效段距 ⇒ 也
        // 開（Bug51170 的 10pt 段後距就只在 Normal）。唯一的額外守門是
        // `w:docGrid/@w:charSpace` 非零的字元網格檔：本引擎未實作 CJK 網格
        // 對齊，兩套垂直制疊加會把臨界頁擠出（reader-styles 3→4 頁實測），
        // 這類檔案維持舊分頁路徑。
        let (doc_before, doc_after) = style_table.doc_spacing();
        let (default_before, default_after) = style_table.doc_default_spacing();
        let default_declared = default_before.is_some() || default_after.is_some();
        let merged_declared = doc_before.is_some() || doc_after.is_some();
        let char_grid = doc
            .parts
            .get("word/document.xml")
            .is_some_and(|xml| doc_grid_has_char_space(xml));
        let bands_on = default_declared || (merged_declared && !char_grid);
        let (grid_before_v, grid_after_v) = if bands_on {
            // K1-R：表格邊界處的段距量化修正（見 `spacing_band_vectors` 注）。
            //
            // ① 表格塊內部段落的段距不參與餘數鏈、表格邊界處餘數歸零：
            //    表格高度由表格幾何獨立計算，段落間距的垂直連續性在表格邊界
            //    中斷；格內段距本就不佔頁面帶（表格列高已含其行高），把它
            //    算進餘數鏈會讓餘數提前推進、下游段落的帶分配整體推高一帶。
            // ② 表格塊之前一段的段後距不再另佔帶：段落與表格塊之間的那段
            //    空白由表格塊的前距承擔（同一段空白不在餘數鏈與版面帶各佔
            //    一次）。二者合起來消除「多佔位把臨界頁擠出」的回歸：
            //    HC031 正文曾多出一頁只含節末段的空頁（總頁數 9→10）。
            let mut spacings = spacings.clone();
            let mut chain_breaks: Vec<usize> = Vec::new();
            for bt in &doc.tables {
                let end = table_map::table_para_end(&bt.table, bt.para_start).min(spacings.len());
                chain_breaks.push(end);
                if let Some(prev) = bt.para_start.checked_sub(1) {
                    if prev < spacings.len() {
                        spacings[prev].1 = None;
                    }
                }
            }
            spacing_band_vectors(&spacings, grid_line_h, &chain_breaks)
        } else {
            (vec![0; spacings.len()], vec![0; spacings.len()])
        };
        let paras_eff: Option<Vec<quire_ooxml::Paragraph>> = Some(eff_all);
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
        // K7-cal：逐段水平 advance 尺（級聯主宣告族→LO face 寬比查表；未宣告
        // 族＝1.0 零擾動）。`K7_WRAP_WIDTH_BUGGY=1` 整表回 1.0＝回塞舊斷行
        // （fdo68607 頁數回 9，紅燈 k7_wrap_width 回塞鎖）。
        let adv_buggy = std::env::var_os("K7_WRAP_WIDTH_BUGGY").is_some();
        let grid_adv_v: Vec<f32> = if adv_buggy {
            vec![1.0; eff_refs.len()]
        } else {
            eff_refs
                .iter()
                .map(|p| para_advance_scale(&p.runs))
                .collect()
        };
        let slots = table_map::build_slots(
            &doc.tables,
            &texts,
            styles,
            &params,
            &sections,
            &images,
            eff_refs,
            &para_line_h_v,
            line_ref_bands,
            &grid_adv_v,
        );
        let bands = table_map::build_images(&images, params.line_h_pt(), &params, &sections);
        // K4c-image：開檔期像素解析（純讀取）。正文流／浮動／格內圖一律沿
        // 候選 rId 走 `word/_rels/document.xml.rels` 查 media part；可渲染
        // 格式優先（PNG/JPEG/…）＞不可渲染（EMF/WMF/PDF）。解析成功才入池
        // ——查無像素的圖不入池，發射層自然退佔位框（零虛構）。
        let doc_rels = doc
            .parts
            .get("word/_rels/document.xml.rels")
            .map(|r| read_rels(r))
            .unwrap_or_default();
        let image_pixels: Vec<PixelSource> = images
            .iter()
            .filter_map(|im| {
                let src = resolve_pixel_source(&doc.parts, &doc_rels, &image_rid_candidates(im))?;
                Some(PixelSource {
                    para: im.para_index,
                    w_pt: im.w_pt,
                    src,
                })
            })
            .collect();
        // br-page 硬斷點（ECMA-376 §17.4.3）：canonical 段落的 w:br w:type="page"
        // 偏移直傳版面層（與 texts 逐段同標）。無 br 檔案＝全空清單＝舊行為。
        // K2b：投影插入 marker 後，原始 `w:br` 碼點偏移需右移 marker 前綴
        // 長度（無 marker＝0 ⇒ 與舊行為逐位元組相同）。
        let breaks: Vec<Vec<usize>> = doc
            .paragraphs
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let shift = marker_chars.get(i).copied().unwrap_or(0);
                p.page_breaks.iter().map(|o| o + shift).collect()
            })
            .collect();
        // 樣式級聯網格（t_5dfc8ac7）：逐段字級/行帶＋pageBreakBefore 旗標。
        let grid = ParaGrid {
            size_pt: grid_size_v,
            bands: grid_bands_v,
            break_before: grid_break_v,
            before_bands: grid_before_v,
            after_bands: grid_after_v,
            advance_scale: grid_adv_v,
        };
        let layout = LayoutEngine::with_breaks_grid(
            &texts,
            params.clone(),
            sections.clone(),
            slots,
            bands,
            breaks,
            grid,
        );
        // 頁首頁尾層（t_b39f108b）：sectPr 宣告＋rels 查表＋part 解析。
        // 全鏈純讀取派生——parts／bundle 一個位元組都不碰（round-trip 零影響）。
        let (hf, hf_parts, even_odd, sec_geo) =
            build_hf_layer(&doc, &sections, Some(&style_table), &theme);
        // 節幾何全表（尾註頁的末節幾何用）：`sec_geo` 只在檔案宣告頁首頁尾時
        // 才有值（`build_hf_layer` 在無宣告時回空表），尾註不能靠它——另存
        // 一份全表（純讀取派生）。
        let mut sec_geo_all: HashMap<u32, SectionGeometry> = HashMap::new();
        for s in &sections {
            sec_geo_all.entry(s.id).or_insert(s.geo);
        }
        let engine = Self {
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
            style_table,
            foot: footnotes::NoteLayout::default(),
            note_entries: Vec::new(),
            note_anchors: Vec::new(),
            note_of_anchor: Vec::new(),
            mark_styles: Vec::new(),
            end: footnotes::EndLayout::default(),
            end_note_entries: Vec::new(),
            end_note_anchors: Vec::new(),
            end_note_of_anchor: Vec::new(),
            end_mark_styles: Vec::new(),
            sec_geo_all,
            para_sec: para_sec_v,
            // K4e2：文字框佔位投影——read_images 標記 text_box 的候選按其
            // 錨定段落索引暫存（發射層查宿主段落的 txbx_texts 落框繪字）。
            // 空表（無文字框檔案）＝發射迴圈零次＝零擾動守門。
            shape_boxes: images
                .iter()
                .filter(|im| !im.txbx_texts.is_empty())
                .map(|im| (im.para_index, im.clone()))
                .collect(),
            image_pixels,
            line_ref_bands,
        };
        Ok(engine.with_footnotes())
    }

    // ---------------- 頁底註腳層（F2b-2）----------------

    /// 開啟尾部：收斂迴圈「分頁→排註腳→新預留」直至逐頁預留穩定
    /// （≤`MAX_CONVERGE_ROUNDS`+1 輪）。**無註腳文件在快路徑直接返回**：
    /// entries/anchors 皆空 ⇒ 不進迴圈、params 與 layout 一個位元組不動。
    ///
    /// 收斂定案後才排**尾註**（F3／J1R）：尾註接排正文末頁、溢出才追加續頁，
    /// 必須知道正文分頁的最終結果（正文頁數、末頁剩餘空間、末節幾何）才能排
    /// ——順序不可調換。
    fn with_footnotes(mut self) -> Self {
        self.recompute_notes();
        self
    }

    /// 註腳＋尾註重算（開檔與編輯後同一路徑）。
    ///
    /// 順序：先讓頁底註腳收斂定案（它會動正文分頁），再用定案後的版面
    /// 排尾註（尾註只接排末頁／追加續頁，不回頭動正文分頁）。
    ///
    /// K7b：頁首推下在註腳**之前**收斂（它同樣動正文分頁——容量自頂扣）；
    /// 註腳收斂可能改變頁數/節邊界（推下表以頁序索引），定案後再校一次
    /// 推下——表未變 ⇒ `set_header_pushes` 早退、零重排（常見路徑零成本）。
    fn recompute_notes(&mut self) {
        self.compute_header_push();
        self.compute_footnotes();
        self.compute_endnotes();
        self.compute_header_push();
    }

    // ---------------- K7b：頁首推下层（Bug51170 尾部流動）----------------
    //
    // LO 口徑（docs/k7-wrapwidth-diag.md §4）：頁首內容塊底端越過 top margin
    // 時，正文起點不再是 margin_top，而是被頁首推到「頁首底端＋一行清距」
    // ——Bug51170 鐵證：頁首圖底 113.45（圖 146×78pt 掛 header 距 35.45），
    // margin 106.35，LO p1+ 正文首行 y=125.65（＝圖底＋12.65 行距清距，
    // 推下 19.3；p0 titlePg 無頁首＝不推，y=106.7）。每頁容量同步扣：
    // LO 逐頁 [29,36,30,33,30,28,14]，我方修前 p1+ 未推 → 每頁多吸 1–2 行、
    // 尾頁 p6 被吸乾（僅頁碼＋頁首）。
    //
    // 實裝＝逐頁推下表（`LayoutParams::header_pushes_pt`，比照 F2b 預留紀律）：
    // 容量與頁蓋章（`Page::margin_top_pt`）都讀同一張表 ⇒ 正文行、表格切片、
    // 圖片、caret 自動跟隨，不需第二份真相。零擾動守門：無頁首宣告檔、
    // 頁首未越界檔（seed25 多數）表恆空 ⇒ layout 一個位元組不動。
    //
    // titlePg 互動：首節首頁選 first slot（Bug51170 無 first ⇒ None ⇒ 不推，
    // p0 起點 106.35 不動——LO p0=106.7 同形）。footer 掛載（y=809）與 R7
    // 尾註掛載頁腳語義不經本層，零觸碰。
    //
    // BUGGY 回塞鎖：`K7B_HDR_PUSH_BUGGY=1` 整組機制停擺（清空表＋不收斂）
    // ⇒ p1+ 起點回 106.35、p6 吸乾——紅燈回舊徑必紅（比照
    // `K6_AUTO_PITCH_BUGGY`／`R6_CLIP_BUGGY` 先例；測試進程內不設 env，
    // 由外部執行驗證）。

    /// 推下收斂主體：「設表→分頁→量頁首底端→新表」直至逐頁推下表穩定
    /// （≤`MAX_CONVERGE_ROUNDS` 輪）。不变量：結束時版面就是按
    /// `self.params.header_pushes_pt` 分頁的結果（發射↔分頁自洽）。
    fn compute_header_push(&mut self) {
        if std::env::var_os("K7B_HDR_PUSH_BUGGY").is_some() {
            if !self.params.header_pushes_pt.is_empty() {
                self.set_header_pushes(Vec::new());
            }
            return; // 回塞鎖：舊口徑（不推）。
        }
        if self.hf.is_empty() {
            return; // 無頁首/頁尾宣告：快路徑零擾動。
        }
        let mut applied: Vec<f32> = Vec::new();
        for _round in 1..=quire_layout::MAX_CONVERGE_ROUNDS {
            self.set_header_pushes(applied.clone());
            let desired = self.header_push_desired();
            if footnotes::reserves_stable(&desired, &applied) {
                return; // 版面已按 applied 分頁且計算值==套用值：定案。
            }
            applied = desired;
        }
        // 輪數用盡仍震盪（罕見；推下只加頁、單調，未見）：保留以末輪
        // `applied` 分頁的版面（不變量優先）。
        self.set_header_pushes(applied);
    }

    /// 套用逐頁推下表（值未變 ⇒ 早退零重排——`recompute_notes` 二校的成本閘）。
    fn set_header_pushes(&mut self, table: Vec<f32>) {
        if footnotes::reserves_stable(&table, &self.params.header_pushes_pt)
            && table.len() == self.params.header_pushes_pt.len()
        {
            return;
        }
        let mut p = self.params.clone();
        p.header_pushes_pt = table;
        self.params = p.clone();
        self.layout.update_params(p);
    }

    /// 以**當前版面**逐頁量「該推多少」：頁首內容底端越過 margin 的頁，
    /// 推下＝底端＋一行清距 − margin（帶尺 ceil 量化；見 `header_push_for_page`）。
    /// 清距＝全域一行（`line_ref_bands`×帶尺＝正文行距，Bug51170 實測 12.65）。
    fn header_push_desired(&self) -> Vec<f32> {
        let pages = self.layout.pages();
        if pages.is_empty() {
            return Vec::new();
        }
        let pitch = self.params.line_h_pt() * self.line_ref_bands.max(1) as f32;
        let mut out = vec![0.0f32; pages.len()];
        for pg in pages {
            out[pg.index] = self.header_push_for_page(pg, pitch);
        }
        out
    }

    /// 單頁推下量（pt）：該頁所選頁首 slot 的內容底端 `bottom`（頁頂系）——
    /// - `bottom <= margin_top`（頁首短於 margin，seed25 多數）⇒ **0**：
    ///   零擾動守門，LO 對此亦不推。
    /// - 越界 ⇒ 推下＝`spill ＋ 正文行距`（LO max() 口徑的直接重建；正文行距
    ///   ＝`line_ref_bands`×帶尺＝全域一行——LO 實測：Bug51170 圖底 113.45
    ///   ＋12.65 − 106.35＝19.75，正文首行 126.10 vs LO 125.65，差 0.45pt）。
    fn header_push_for_page(&self, pg: &Page, pitch: f32) -> f32 {
        let Some(sec) = self.hf.get(&pg.section_id) else {
            return 0.0;
        };
        let is_first = self.is_section_first_page(pg.index);
        let is_even = (pg.index + 1).is_multiple_of(2); // 與發射層同口徑
        let Some(path) = pick_slot(
            &sec.header,
            sec.title_pg && is_first,
            self.even_odd && is_even,
        ) else {
            return 0.0; // titlePg 首頁無 first slot＝空白頁首（不虛構、不推）
        };
        let Some(part) = self.hf_parts.get(path) else {
            return 0.0;
        };
        // sec_geo 在頁首頁尾宣告檔恆命中（build_hf_layer 同條件灌入）；後備
        // 從 params 原始邊界合成——**不可**用 `pg.margin_top_pt`（推下後它已
        // 含推下量，會自我震盪）。
        let geo = self
            .sec_geo
            .get(&pg.section_id)
            .copied()
            .unwrap_or(SectionGeometry {
                page_w_pt: self.params.page_w_pt,
                page_h_pt: self.params.page_h_pt,
                margin_top_pt: self.params.margin(MARGIN_TOP),
                margin_bottom_pt: self.params.margin(quire_layout::MARGIN_BOTTOM),
                margin_left_pt: self.params.margin(MARGIN_LEFT),
                margin_right_pt: self.params.margin(quire_layout::MARGIN_RIGHT),
                landscape: false,
                continuous: false,
                cols: Default::default(),
            });
        let hf_params =
            LayoutParams::from_section(&geo, self.params.font_size_pt, self.params.line_spacing);
        let bottom = self.hf_content_bottom_pt(part, &hf_params, &geo, sec.header_dist_pt);
        let spill = bottom - geo.margin_top_pt;
        if spill <= 0.0 {
            return 0.0;
        }
        // LO 口徑 max(margin_top, 頁首底端＋一行清距)：推下＝spill＋pitch
        // （raw pt 不量化——容量由 `section_capacity` 統一 floor 到帶，量化
        // 只會讓起點錨點再偏 ≤帶尺；Bug51170 7.10＋12.65＝19.75 → 首行
        // 126.10 vs LO 125.65＝0.45pt）。
        spill + pitch
    }

    /// 頁首 part 內容底端（pt，自頁頂）：發射層 `push_hf_slot` 的帶游標鏡像
    /// （段落行＋表格整塊＋逐圖 y=h 底緣取 max）。**只量不發**——發射路徑
    /// 零觸碰（顯示列表逐位元組不受影響）；兩處若未來分叉，推下量偏保守
    /// （正文起點偏下 ≤帶尺），記錄在案。
    fn hf_content_bottom_pt(
        &self,
        part: &HfPart,
        hf_params: &LayoutParams,
        geo: &SectionGeometry,
        emit_y: f32,
    ) -> f32 {
        let line_h = hf_params.line_h_pt();
        let texts: Vec<String> = part
            .styles
            .iter()
            .map(|pp| pp.runs.iter().map(|r| r.text.as_str()).collect())
            .collect();
        let styles = self.doc.parts.get("word/styles.xml").map(|s| s.as_slice());
        let sec_one = [SectionSpec { id: 0, geo: *geo }];
        let hf_line_hs: Vec<f32> = part.metrics.iter().map(|m| m.1).collect();
        // K7-cal：水平尺空表＝頁首表格維持舊斷行（main 09da233 同口徑；
        // 合併時分支調用點滯留 9 參數舊簽名，PM fix-forward 補齊）。
        let slots_tbl = table_map::build_slots(
            &part.tables,
            &texts,
            styles,
            hf_params,
            &sec_one,
            &[],
            &part.styles,
            &hf_line_hs,
            self.line_ref_bands,
            &[],
        );
        let grid: Vec<(f32, f32)> = part.metrics.clone();
        let pls = quire_layout::layout_paragraphs_grid(&texts, hf_params, &[], &grid);
        let mut cursor_bands: u32 = 0;
        let mut ti = 0usize;
        let mut skip_until = 0usize;
        let mut bottom = emit_y;
        for (pi, pl) in pls.iter().enumerate() {
            while ti < slots_tbl.len() && slots_tbl[ti].para_start <= pi {
                let tb = slots_tbl[ti].layout.total_bands.max(1);
                bottom = bottom.max(emit_y + (cursor_bands + tb) as f32 * line_h);
                cursor_bands += tb;
                skip_until = skip_until.max(slots_tbl[ti].para_end);
                ti += 1;
            }
            if pi < skip_until {
                continue;
            }
            cursor_bands += pl.lines.len().max(1) as u32;
            bottom = bottom.max(emit_y + cursor_bands as f32 * line_h);
        }
        while ti < slots_tbl.len() {
            let tb = slots_tbl[ti].layout.total_bands.max(1);
            bottom = bottom.max(emit_y + (cursor_bands + tb) as f32 * line_h);
            cursor_bands += tb;
            ti += 1;
        }
        for im in part.images.iter().filter(|im| !im.in_cell) {
            let y = emit_y + im.para_index as f32 * line_h;
            bottom = bottom.max(y + im.h_pt);
        }
        bottom
    }

    /// 收斂主體。不變量（發射↔分頁自洽）：結束時
    /// `self.params.footnote_reserves_pt == self.foot.reserves`——顯示用的
    /// 排布與實際分頁扣的預留永遠同一份。
    fn compute_footnotes(&mut self) {
        self.build_note_entries();
        if self.note_entries.is_empty() || self.note_anchors.is_empty() {
            // 零擾動快路徑：無註腳（或註腳全未被引用）⇒ 現況逐位元組不變。
            self.foot = footnotes::NoteLayout::default();
            if self.params.footnote_reserves_pt.is_empty() {
                return; // 預留表本就空：layout 無需重排。
            }
            // （理論不可達：只有註腳檔案才會灌過預留表）清表重排回現況。
            let mut p = self.params.clone();
            p.footnote_reserves_pt = Vec::new();
            self.params = p.clone();
            self.layout.update_params(p);
            return;
        }
        let body_size = self.params.font_size_pt;
        // `applied`＝本輪分頁實際套用的逐頁預留（起始＝空表＝現況）。
        let mut applied: Vec<f32> = Vec::new();
        let max_rounds = quire_layout::MAX_CONVERGE_ROUNDS;
        for round in 1..=max_rounds {
            let mut p = self.params.clone();
            p.footnote_reserves_pt = applied.clone();
            // 尾註預留由 compute_endnotes 另行收斂；此處先清，避免舊值干擾註腳分頁。
            p.endnote_reserves_pt = Vec::new();
            self.params = p.clone();
            self.layout.update_params(p);
            // lay.reserves＝本輪版面下「下一輪該套」的計算預留（純由排布算出）。
            let lay = self.footnote_pass(body_size, round);
            if footnotes::reserves_stable(&lay.reserves, &applied) {
                // 計算值 == 實際套用值 ⇒ 分頁與排布自洽，定案返還。
                self.foot = lay;
                return;
            }
            self.foot = lay;
            applied = self.foot.reserves.clone();
        }
        // 輪數用盡仍震盪（罕見；corpus 未見）：保留以 `applied` 分頁的版面，
        // 發射排布錨定在**實際分頁扣的預留**上（不變量優先於末輪計算值），
        // rounds 保持最大值＝誠實回報未收斂。
        self.foot.reserves = applied;
    }

    /// 由 F1 資料層（`doc.notes`）＋級聯建構發射級條目與正文錨點。
    ///
    /// 編號＝正文引用序（1 起）；separator／continuationSeparator 型本體
    /// 不進條目（Word 慣例 id=-1/0，非正文引用對象）。級聯取樣＝正文同款
    /// `cascade_paragraphs`（註腳樣式鏈 footnoteReference 因此在 F1 已就位）。
    ///
    /// 尾註支路同款（尾註段也是 `Paragraph`，一次 `cascade_paragraphs` 即可
    /// 兩邊同尺）：兩條鏈各自獨立——`note_ref` 是單欄位，同一個 run 不會
    /// 同時掛註腳與尾註 ⇒ 兩條支路無競態。
    fn build_note_entries(&mut self) {
        let paras_eff: Vec<quire_ooxml::Paragraph> = self.eff_paragraphs().to_vec();
        let def_size = self.params.font_size_pt;

        // ---- 頁底註腳支路 ----
        let mut foot_by_id =
            self.cascade_note_entries(&self.doc.notes.footnotes, self.params.content_w_pt());
        let foot_anchors = footnotes::collect_anchors(&paras_eff);
        // H2：編號字面表（`numFmt`／`numRestart` 消費結果）。十進位且無重啟
        // ⇒ 空表 ⇒ 呼叫端走十進位兜底原路徑（渲染與 JSON 逐位元組不變）。
        let foot_labels = self.footnote_number_labels(&foot_anchors);
        let (foot_anchors, foot_entries, foot_of_anchor, foot_marks) = note_chain(
            &foot_anchors,
            &mut foot_by_id,
            &paras_eff,
            def_size,
            &foot_labels,
        );

        // ---- 尾註支路 ----
        let end_w = self.end_content_w_pt();
        let mut end_by_id = self.cascade_note_entries(&self.doc.notes.endnotes, end_w);
        let end_anchors = footnotes::collect_endnote_anchors(&paras_eff);
        // F3b：尾註編號消費（鏡像 H2 頁底支路）——settings.endnote ＋逐節
        // sectPr endnotePr 逐欄合併 ⇒ 沿錨點引用序計數 ⇒ 註 id → 字面。
        // 十進位無重啟 ⇒ 空表 ⇒ 零變化（`assign` 不寫十進位 label）。
        let end_labels = self.assign_endnote_labels(&end_anchors);
        let (end_anchors, end_entries, end_of_anchor, end_marks) = note_chain(
            &end_anchors,
            &mut end_by_id,
            &paras_eff,
            def_size,
            &end_labels,
        );

        self.note_anchors = foot_anchors;
        self.note_entries = foot_entries;
        self.note_of_anchor = foot_of_anchor;
        self.mark_styles = foot_marks;
        self.end_note_anchors = end_anchors;
        self.end_note_entries = end_entries;
        self.end_note_of_anchor = end_of_anchor;
        self.end_mark_styles = end_marks;
    }

    /// 尾註編號消費：`settings.endnote` ＋逐節 `sectPr/w:endnotePr` 逐欄合併
    /// （[`numbering::merge_note_pr`]）後，沿錨點引用序計數 ⇒「註 id → 字面」
    /// （[`numbering::assign`]）。呼叫端 [`note_chain`] 據此蓋 `number_label`，
    /// 前綴與記號再經 [`footnotes::note_number_text`] 統一取用。
    ///
    /// **零變化守門**：`numbering::assign` 對「十進位且無重啟／無 numStart≠1」
    /// **不寫** label ⇒ 本函式回空表 ⇒ `number_label` 維持空字串 ⇒ 十進位
    /// `number` 兜底路徑逐字原樣（JSON／渲染皆然）。
    ///
    /// **老實邊界（pos 未消費）**：`w:endnotePr/@w:pos` 的 `sectEnd`／`pageEnd`
    /// **不消費、不實裝分支**——corpus Saturn.docx 實見 `pos=sectEnd`（單節），
    /// 本卡仍按 F3 現行 `documentEnd`（文件末頁）渲染；節末尾註頁屬後續卡。
    /// `pos` 只決定尾註頁摆放位置、與編號無關，故此處不讀 `pos`。分隔線註
    /// （`separator`／`continuationSeparator`）的 id 亦**不解析不渲染**——其
    /// 圖形內容屬後續卡，H1 只存 id（見 [`footnotes::EndLayout`] 注）。
    fn assign_endnote_labels(&self, anchors: &[footnotes::Anchor]) -> HashMap<i64, String> {
        let settings = self.doc.notes.settings.endnote;
        // 逐節 endnotePr（與 `para_sec` 的 section id 同序：兩者皆來自
        // `scan_sections` 的邊界＋body 序，見 sectpr 模組）。無 part ⇒ 空表。
        let sec_prs = self
            .doc
            .parts
            .get("word/document.xml")
            .map(|xml| quire_ooxml::read_sections_endnote_pr(xml))
            .unwrap_or_default();
        numbering::assign(anchors, &self.para_sec, |sec| {
            let sec_pr = sec.and_then(|s| sec_prs.get(s as usize).copied().flatten());
            let pr = numbering::merge_note_pr(settings, sec_pr).unwrap_or_default();
            numbering::ResolvedNumbering {
                num_fmt: pr.num_fmt,
                num_restart: pr.num_restart,
                num_start: pr.num_start,
            }
        })
    }

    /// H2：頁底註腳的**編號字面表**（`numFmt`／`numRestart` 消費結果）。
    ///
    /// 兩層讀取（ECMA-376 §17.11.5 逐欄覆寫語義）：
    /// 1. settings 層＝`doc.notes.settings.footnote`（`word/settings.xml` 的
    ///    `w:footnotePr`＝文件預設）；
    /// 2. 節層＝`read_sections_note_pr`（`word/document.xml` 逐節的
    ///    `w:footnotePr`＝節覆寫）。
    ///
    /// **節 id ↔ 清單下標的對齊方式（已核對）**：`read_sections_note_pr`
    /// 的回傳順序＝`scan_sections` 的 section 收束順序（段落 `w:pPr` 內的
    /// `sectPr` 依序在前、檔尾 body 級 `sectPr` 在最後），其 section id 由
    /// `next_id` 逐節遞增（0 起、連續、不跳號）⇒ **清單下標 == section id**。
    /// 錨點段落所屬節由 `self.para_sec`（段落→section id 的逐段落表）查出
    /// ⇒ 以該 id 直接索引本層清單。
    ///
    /// `pos`（`beneathText`／`pageBottom`）**不消費**：`beneathText`＝現行頁底
    /// 語義（已是 F2b-2 的頁底實態＝現行行為）；`pageBottom` 未見 corpus 實檔
    /// 故不實裝（本卡不做 pos 分支，不臆造）。
    ///
    /// 零變化守門：十進位且無重啟 ⇒ `numbering::assign` 回**空表** ⇒ 呼叫端
    /// 走十進位兜底原路徑（渲染與 JSON 皆逐位元組不變）。
    fn footnote_number_labels(&self, anchors: &[footnotes::Anchor]) -> HashMap<i64, String> {
        if anchors.is_empty() {
            // 無引用 ⇒ 無編號可言（空表＝十進位兜底；順帶省下一次 document.xml 掃描）。
            return HashMap::new();
        }
        // settings 層（文件預設）。
        let settings_pr = self.doc.notes.settings.footnote;
        // 節層（逐節覆寫；索引＝section id，見上）。
        let sect_pr: Vec<Option<quire_ooxml::NotePr>> = self
            .doc
            .parts
            .get("word/document.xml")
            .map(|xml| read_sections_note_pr(xml.as_slice()))
            .unwrap_or_default();
        let resolve = |sec: Option<u32>| -> numbering::ResolvedNumbering {
            let section_pr = sec.and_then(|id| sect_pr.get(id as usize).copied().flatten());
            let Some(pr) = numbering::merge_note_pr(settings_pr, section_pr) else {
                // 兩層都無宣告 ⇒ 全預設（十進位、continuous、起點 1）。
                return numbering::ResolvedNumbering {
                    num_fmt: None,
                    num_restart: None,
                    num_start: None,
                };
            };
            numbering::ResolvedNumbering {
                num_fmt: pr.num_fmt,
                num_restart: pr.num_restart,
                num_start: pr.num_start,
            }
        };
        numbering::assign(anchors, &self.para_sec, resolve)
    }

    /// 註本體（註腳或尾註）→ id → 發射級條目（級聯取樣與正文同款）。
    ///
    /// 尾註段同樣是 `Paragraph`，級聯路徑完全同款——樣式鏈
    /// endnoteText／endnoteReference 因此在 F1 已就位。
    fn cascade_note_entries(
        &self,
        notes: &[quire_ooxml::Note],
        content_w_pt: f32,
    ) -> HashMap<i64, footnotes::NoteEntry> {
        // id → (級聯烘焙段落, 字級, 行高, 樣式) 表（一次級聯，全部註段同尺）。
        let paras: Vec<quire_ooxml::Paragraph> = notes
            .iter()
            .filter(|n| n.kind == quire_ooxml::NoteKind::Regular)
            .map(|n| n.paragraphs.first().cloned().unwrap_or_default())
            .collect();
        let ids: Vec<i64> = notes
            .iter()
            .filter(|n| n.kind == quire_ooxml::NoteKind::Regular)
            .map(|n| n.id)
            .collect();
        let def_size = self.params.font_size_pt;
        let widths: Vec<f32> = vec![content_w_pt; paras.len()];
        let fn_calibrated =
            self.style_table.has_fonts() && std::env::var_os("K6_AUTO_PITCH_BUGGY").is_none();
        let (eff, metrics, _spacings, _marker_chars) = cascade_paragraphs(
            &self.style_table,
            &paras,
            def_size,
            1.0,
            &widths,
            &[],
            PitchCascade {
                table_styles: &HashMap::new(),
                calibrated: fn_calibrated,
            },
        );
        let mut by_id: HashMap<i64, footnotes::NoteEntry> = HashMap::new();
        for (id, (p, (size, line_h))) in ids.iter().zip(eff.into_iter().zip(metrics)) {
            let first = p.runs.first();
            let style = footnotes::EntryStyle {
                size_pt: first.and_then(|r| r.size_pt).unwrap_or(size).max(1.0),
                line_h_pt: line_h.max(1.0),
                color_argb: first
                    .and_then(|r| r.color)
                    .unwrap_or(DEFAULT_TEXT_COLOR_ARGB),
                bold: first.is_some_and(|r| r.bold),
                italic: first.is_some_and(|r| r.italic),
                underline: first.is_some_and(|r| r.underline),
                strike: first.is_some_and(|r| r.strike),
                family: first
                    .and_then(|r| select_text_family(r.effective_fonts.as_ref(), ""))
                    .clone(),
            };
            let body: String = p.runs.iter().map(|r| r.text.as_str()).collect::<String>();
            by_id.insert(
                *id,
                footnotes::NoteEntry {
                    id: *id,
                    number: 0,
                    number_label: String::new(),
                    body: body.trim().to_string(),
                    style,
                },
            );
        }
        by_id
    }

    /// 收斂迴圈的單次求值：以「已套用預留＝`applied`」的當前版面算排布。
    ///
    /// 版心底用**未扣預留**的全容量（`section_capacity` 吃清空預留的 params）：
    /// 註腳區固定貼版心底，正文可用區被預留往上擠——這正是整帶量化保證的
    /// 幾何（末帶底恰與分隔線相切，見 `footnotes` 模組注）。
    fn footnote_pass(&self, body_size: f32, rounds: usize) -> footnotes::NoteLayout {
        let pages = self.layout.pages();
        // 全容量（無預留）基準 params：版心底以此折算，預留變動不動「底」。
        let mut full = self.params.clone();
        full.footnote_reserve_pt = 0.0;
        full.footnote_reserves_pt = Vec::new();
        // （F3 重構）PageInfo 構造抽成 page_infos()，尾註支路同用；
        // F2c 的 capacity_bands 錨定已併入該函式。
        let info = self.page_infos(&full);
        // R7：表格段→掛載頁 對位表（格內錨點在流行程找不到行的改道鏈；
        // 槽序＝`table_slots()` 序＝`FlowItem::Table{slot}`／`TablePlacement::slot`
        // 同一命名空間，錨點段索引＝eff_paragraphs 段序＝layout texts 段序）。
        let slots = self.layout.table_slots();
        let n_paras = self.eff_paragraphs().len();
        let mut slot_of_para = vec![usize::MAX; n_paras];
        let header_of_slot: Vec<usize> = slots
            .iter()
            .enumerate()
            .map(|(si, t)| {
                let (a, b) = (t.para_start.min(n_paras), t.para_end.min(n_paras));
                if a < b {
                    slot_of_para[a..b].fill(si);
                }
                t.para_start
            })
            .collect();
        let lay = footnotes::layout_notes_with_tables(
            pages,
            &info,
            &self.note_anchors,
            &self.note_entries,
            &self.note_of_anchor,
            body_size,
            &self.mark_styles,
            rounds,
            &slot_of_para,
            &header_of_slot,
        );
        // 此處回傳的是**計算值**（下一輪該套的預留）——收斂判準就比這個值；
        // 「發射↔分頁自洽」由呼叫端（compute_footnotes 定案點）保證。
        lay
    }

    // ---------------- 尾註層（F3）----------------

    /// 尾註支路（J1R）：正文分頁**定案後**，把尾註接排到正文末頁底部；
    /// 末頁剩餘空間裝不下才溢出到純尾註續頁。
    ///
    /// 幾何契約（PM 定案，照做）：
    /// 1. **正文零位移**：本函式完全不動 `params`／`layout`；接排只用末頁
    ///    既有內容之後的剩餘空白（不預留、不擠正文——與頁底註腳相反）。
    /// 2. 接排與續頁幾何＝**末節**（正文末頁的 section）。
    /// 3. 接排分隔線在末頁內容之後（版心左緣起、寬＝版心×1/3），條目在其
    ///    下；續頁頂部畫續接線（置中短線，`continuationSeparator` 語義）。
    /// 4. 條目整條不跨頁：一條裝不下就整條去續頁。
    /// 5. `pos=sectEnd`／`pageEnd` **仍不消費**（維持 `documentEnd`），
    ///    分隔線註本體圖形**不解析不渲染**（僅畫幾何短線）。
    ///
    /// **無尾註檔在快路徑直接返回**：`end` 保持空 ⇒ 頁數不變、顯示列表
    /// 逐位元組不變（構造性零擾動）。
    fn compute_endnotes(&mut self) {
        self.end = footnotes::EndLayout::default();
        if self.end_note_entries.is_empty() || self.end_note_anchors.is_empty() {
            // 零擾動快路徑（無尾註，或尾註全未被引用）：清掉上一輪可能留下的
            // 尾註預留，讓分頁回到無尾註現況（編輯刪光尾註時走此路）。
            if !self.params.endnote_reserves_pt.is_empty() {
                let mut p = self.params.clone();
                p.endnote_reserves_pt = Vec::new();
                self.params = p.clone();
                self.layout.update_params(p);
            }
            return;
        }
        if self.layout.pages().is_empty() {
            return;
        }
        let Some(geo) = self.end_geo() else {
            return; // 無末節幾何（病態檔）：老實不排尾註，不虛構幾何。
        };
        let band = self.params.line_h_pt();
        let content_w = self.end_content_w_pt();
        // 全容量（無頁底註腳／無尾註預留）：版心底與容量以此折算，預留不動「底」。
        let mut full = self.params.clone();
        full.footnote_reserve_pt = 0.0;
        full.footnote_reserves_pt = Vec::new();
        full.endnote_reserves_pt = Vec::new();
        let sep_bands = footnotes::endnote_sep_bands(band) as usize;
        // J1-fix：比照 F2c——「排一輪算出下輪該套的尾註預留」迭代至穩定。
        // 預留＝分隔線佔帶＋實際接排條目帶數；餵回分頁後正文超過末頁容量
        // 時**如實溢出多一頁**（不為頁數硬壓）。接排區頂由保留帶推得：
        // content_bottom 往上扣「頁底註腳區＋尾註保留帶」。
        let max_rounds = quire_layout::MAX_CONVERGE_ROUNDS;
        let mut applied: Vec<f32> = Vec::new();
        for _round in 1..=max_rounds {
            let mut p = self.params.clone();
            p.endnote_reserves_pt = applied.clone();
            self.params = p.clone();
            self.layout.update_params(p);
            let pages = self.layout.pages();
            if pages.is_empty() {
                self.end = footnotes::EndLayout::default();
                return;
            }
            let last_page = pages.len() - 1;
            let cap = section_capacity(&full, &geo, pages[last_page].index);
            // 正文末頁已用帶（body-only；呼叫端再扣頁底註腳與分隔線）。
            let used = self.last_body_used_bands(&pages[last_page], &geo, band);
            let foot_bands = self
                .foot
                .pages
                .get(last_page)
                .map(|p| p.reserve_bands)
                .unwrap_or(0) as usize;
            let avail = cap.saturating_sub(used).saturating_sub(foot_bands);
            // 首輪以「整頁可用」估接排容量（不先扣正文佔用）：尾註區是
            // 文件末尾的整塊內容，先算出它需要的保留帶再餵回分頁；正文若
            // 因而超過末頁容量，就**如實溢出**到下一頁（比照 F2c 的
            // reserve→重排語義）。次輪起改用實際末頁剩餘空間修正。
            let inline_capacity = if applied.is_empty() {
                cap.saturating_sub(foot_bands).saturating_sub(sep_bands)
            } else {
                avail.saturating_sub(sep_bands)
            };
            let spec = footnotes::EndGeometry {
                band_pt: band,
                content_w_pt: content_w,
                capacity_first: cap.saturating_sub(sep_bands).max(1),
                capacity_rest: cap.max(1),
                inline_capacity,
                // 先給 0；條目帶數定案後再依保留帶重算（見下）。
                inline_top_y: 0.0,
            };
            // 逐頁版面事實（記號座標用；口徑與頁底註腳支路同款）。
            let info = self.page_infos(&full);
            let mut lay = footnotes::layout_endnotes(
                pages,
                &info,
                &self.end_note_anchors,
                &self.end_note_entries,
                &self.end_note_of_anchor,
                self.params.font_size_pt,
                &self.end_mark_styles,
                &spec,
            );
            let inline_bands: u32 = lay.inline.iter().map(|i| i.bands.max(1)).sum();
            // K7b：原點與正文行同基準（末頁實際 margin_top_pt 含推下；cap
            // 已由 `section_capacity` 自頂扣推下 ⇒ 底端與 packer 一致）。
            // 推下 0＝舊 geo 值逐位元組全等。
            let content_bottom = pages[last_page].margin_top_pt + cap as f32 * band;
            let inline_top_y = content_bottom - (foot_bands as f32 + inline_bands as f32) * band;
            lay.inline_top_y = (!lay.inline.is_empty()).then_some(inline_top_y);
            // 下一輪該套的預留（接排區＝分隔線＋條目帶）。
            let next_bands = if lay.inline.is_empty() {
                0
            } else {
                sep_bands as u32 + inline_bands
            };
            // 保留先前各輪已套用的預留（它們是造成／維持正文溢出的因），
            // 只更新本輪末頁；頁數增長時補 0 後再寫入新末頁。單調累積
            // （頁數不回頭）⇒ 溢出案例也能收斂，不會在「末頁搬到新頁」間震盪。
            let mut next = applied.clone();
            next.resize(pages.len(), 0.0);
            next[last_page] = next_bands as f32 * band;
            next.truncate(pages.len());
            let stable = footnotes::reserves_stable(&next, &applied);
            self.end = lay;
            if stable {
                // 定案：把「計算值」寫回 params（與顯示同一份），分頁不變。
                let mut p = self.params.clone();
                p.endnote_reserves_pt = next;
                self.params = p.clone();
                self.layout.update_params(p);
                return;
            }
            applied = next;
        }
        // 輪數用盡仍震盪（罕見）：保留最後一輪的排布與實際套用的預留。
    }

    /// J1R：正文末頁已佔用的行網格帶數（供尾註接排找剩餘空白）。
    ///
    /// 取各行 `line_bands`、表格切片帶、行內／浮動圖片底緣的最大值。
    /// 帶尺與 page 的節幾何同尺；呼叫端再扣頁底註腳預留。夾在 `capacity`
    /// 內（病態幾何不讓接排容量變成負值）。
    fn last_body_used_bands(&self, p: &Page, geo: &SectionGeometry, band: f32) -> usize {
        let band = band.max(1.0);
        // 全容量（清空頁底註腳預留）：呼叫端再自行扣掉該頁註腳區，
        // 避免 `section_capacity` 與呼叫端重複扣同一塊。
        let mut full = self.params.clone();
        full.footnote_reserve_pt = 0.0;
        full.footnote_reserves_pt = Vec::new();
        full.endnote_reserves_pt = Vec::new();
        let cap = section_capacity(&full, geo, p.index);
        let mut used = 0u32;
        for i in 0..p.lines.len() {
            let b = p.line_bands.get(i).copied().unwrap_or(i as u32);
            used = used.max(b.saturating_add(1));
        }
        for tp in &p.tables {
            let h: f32 = tp.frag.rows.iter().map(|r| r.height_pt).sum();
            let bands = (h / band).ceil().max(1.0) as u32;
            used = used.max(tp.band.saturating_add(bands));
        }
        for im in &p.images {
            let bands = (im.h_pt / band).ceil().max(1.0) as u32;
            used = used.max(im.band.saturating_add(bands));
        }
        for fm in &p.floating_images {
            // K7b：基準與浮圖 y_pt 同原點（pg.margin_top_pt 含推下；推下 0＝
            // 舊 geo 值逐位元組全等）。
            let bands = ((fm.y_pt + fm.h_pt - p.margin_top_pt).max(0.0) / band).ceil() as u32;
            used = used.max(bands);
        }
        (used as usize).min(cap)
    }

    /// 尾註頁的頁面幾何＝**末節**（正文末頁的 section）。
    ///
    /// 節幾何全表 `sec_geo_all` 有值時用它；無值（檔案無任何 sectPr）⇒
    /// 用文件基準 params 合成（後備口徑同頁底註腳支路的 `fallback_geo`）。
    fn end_geo(&self) -> Option<SectionGeometry> {
        let last = self.layout.pages().last()?;
        if let Some(geo) = self.sec_geo_all.get(&last.section_id) {
            return Some(*geo);
        }
        Some(SectionGeometry {
            page_w_pt: self.params.page_w_pt,
            page_h_pt: self.params.page_h_pt,
            margin_top_pt: self.params.margin(MARGIN_TOP),
            margin_bottom_pt: self.params.margin(MARGIN_BOTTOM),
            margin_left_pt: self.params.margin(MARGIN_LEFT),
            margin_right_pt: self.params.margin(quire_layout::MARGIN_RIGHT),
            landscape: false,
            continuous: false,
            cols: Default::default(),
        })
    }

    /// 尾註條目的版心寬（pt）：末節版心寬（無末節幾何時用文件基準）。
    fn end_content_w_pt(&self) -> f32 {
        let Some(geo) = self.end_geo() else {
            return self.params.content_w_pt();
        };
        (geo.page_w_pt - geo.margin_left_pt - geo.margin_right_pt).max(1.0)
    }

    /// 逐頁版面事實（記號座標與註腳區幾何的共用輸入）。
    ///
    /// `full`＝**未扣預留**的 params（版心底以此折算：預留變動不動「底」）。
    fn page_infos(&self, full: &LayoutParams) -> Vec<footnotes::PageInfo> {
        let band = self.params.line_h_pt();
        let fallback_geo = SectionGeometry {
            page_w_pt: self.params.page_w_pt,
            page_h_pt: self.params.page_h_pt,
            margin_top_pt: self.params.margin(MARGIN_TOP),
            margin_bottom_pt: self.params.margin(MARGIN_BOTTOM),
            margin_left_pt: self.params.margin(MARGIN_LEFT),
            margin_right_pt: self.params.margin(quire_layout::MARGIN_RIGHT),
            landscape: false,
            continuous: false,
            cols: Default::default(),
        };
        self.layout
            .pages()
            .iter()
            .map(|pg| {
                let geo = self
                    .sec_geo
                    .get(&pg.section_id)
                    .copied()
                    .unwrap_or(fallback_geo);
                let cap = section_capacity(full, &geo, pg.index);
                footnotes::PageInfo {
                    content_left_pt: pg.margin_left_pt,
                    content_w_pt: (geo.page_w_pt - geo.margin_left_pt - geo.margin_right_pt)
                        .max(1.0),
                    // K7b：起算原點用頁實際 `margin_top_pt`（含頁首推下）——
                    // 正文行實際起點在此，版心底必須同原點才對齊；推下 0＝
                    // 舊 `geo.margin_top_pt` 值逐位元組全等（零擾動）。
                    content_bottom_pt: pg.margin_top_pt + cap as f32 * band,
                    band_pt: band,
                    // F2c：拆分預算錨定「未扣預留的固定容量」——逐輪預留浮動
                    // 不得進入拆分子（否則收斂自我震盪）。
                    capacity_bands: cap as u32,
                }
            })
            .collect()
    }

    /// 段落 → 所屬 section id 表（`para_sec` 的唯讀視圖；空表＝檔案無
    /// sectPr）。節域編號（H2／F3b）與測試查錨點所在節用。
    pub fn para_sections(&self) -> &[u32] {
        &self.para_sec
    }

    /// 尾註發射層的唯讀視圖（逐頁條目＋正文錨點記號）。
    pub fn endnote_layout(&self) -> &footnotes::EndLayout {
        &self.end
    }

    /// 尾註渲染資料 JSON（驗收/除錯口；逐頁條目、正文記號、分隔線旗標）。
    ///
    /// 與 [`Engine::footnotes_json`] 分開：頁底註腳與尾註是兩條支路，
    /// 各自的排布資料各自一個出口（`footnotes_json` 只管頁底註腳）。
    pub fn endnotes_json(&self) -> String {
        serde_json::to_string_pretty(&self.end).unwrap_or_else(|_| "{}".to_string())
    }

    /// 純尾註續頁數（接排裝不下而追加在正文之後；無續頁＝0）。
    /// 接排進正文末頁的條目**不另計頁**。
    pub fn endnote_page_count(&self) -> usize {
        self.end.pages.len()
    }

    /// 正文頁數（不含純尾註續頁；接排不動正文分頁）。
    pub fn body_page_count(&self) -> usize {
        self.layout.page_count()
    }

    /// 註腳渲染資料 JSON（驗收/除錯口；逐頁條目、保留帶、記號、輪數）。
    ///
    /// **只含頁底註腳**。尾註是另一條支路、資料結構不同（接排末頁＋溢出
    /// 續頁、幾何取末節），故另開 [`Engine::endnotes_json`] 出口——兩者不混在一份 JSON
    /// 裡（混了會讓「哪個頁的哪條註」無從判讀）。
    pub fn footnotes_json(&self) -> String {
        serde_json::to_string_pretty(&self.foot).unwrap_or_else(|_| "{}".to_string())
    }

    /// 收斂輪數（觀察用；無註腳＝0）。
    pub fn footnote_rounds(&self) -> usize {
        if self.foot.is_empty() {
            0
        } else {
            self.foot.rounds
        }
    }

    /// 第 `page` 頁的註腳發射項（分隔線＋條目＋錨點記號）。無註腳＝空。
    fn footnote_items(&self, page: usize) -> Vec<DrawItem> {
        let Some(pf) = self.foot.pages.get(page) else {
            return Vec::new();
        };
        let mut items = Vec::new();
        if let (Some(y), Some(x1), Some(x2)) = (pf.separator_y, pf.separator_x1, pf.separator_x2) {
            // F2c：全續段頁 ⇒ 續接線（置中短線，約 1/3 版心）；一般頁 ⇒
            // 分隔線（左緣起 1/3 版心，F2b-2 原樣）。
            let (x1, x2) = if pf.continuation_only {
                // Word：續接線置中於版心、寬約 1/3 版心。分隔線跨度是
                // 「左緣起 × SEP_WIDTH_RATIO」，由此反推版心寬再置中。
                let left = x1;
                let content_w = (x2 - x1) / footnotes::SEP_WIDTH_RATIO;
                (
                    left + content_w * (1.0 - footnotes::SEP_WIDTH_RATIO) / 2.0,
                    left + content_w * (1.0 + footnotes::SEP_WIDTH_RATIO) / 2.0,
                )
            } else {
                (x1, x2)
            };
            items.push(DrawItem::Rule {
                x1,
                y1: y,
                x2,
                y2: y,
                width_pt: 0.5,
                color_argb: DEFAULT_TEXT_COLOR_ARGB,
                dash: None,
            });
        }
        let Some(top0) = pf.items_top_y else {
            // 無條目：記號仍要發（錨點可能定位失敗前已收錄？——不會：記號與
            // 條目同頁同則成對推入；此分支保底直接發記號）。
            for m in self.foot.marks.get(page).map(Vec::as_slice).unwrap_or(&[]) {
                items.push(DrawItem::Text {
                    x: m.x_pt,
                    y: m.y_pt,
                    size_pt: m.size_pt,
                    flags: 0,
                    color_argb: m.color_argb,
                    family: m.family.clone(),
                    text: m.text.clone(),
                });
            }
            return items;
        };
        // 條目由上而下堆疊：第 i 條行頂＝top0 + Σ(前 i-1 條帶數×帶尺)。
        // NoteRun.x 是**相對版心左緣**（layout 層不知道頁邊界）；此處換算頁絕對。
        let base_x = self
            .layout
            .pages()
            .get(page)
            .map(|pg| pg.margin_left_pt)
            .unwrap_or(0.0);
        let mut acc_bands = 0u32;
        for item in &pf.items {
            let mut yy = top0 + acc_bands as f32 * pf.band_pt;
            for line in &item.lines {
                for run in &line.runs {
                    let y = if run.superscript {
                        yy - pf.band_pt * footnotes::SUP_RISE
                    } else {
                        yy
                    };
                    let mut flags = 0u8;
                    if run.bold {
                        flags |= FLAG_BOLD;
                    }
                    if run.italic {
                        flags |= FLAG_ITALIC;
                    }
                    if run.underline {
                        flags |= FLAG_UNDERLINE;
                    }
                    if run.strike {
                        flags |= FLAG_STRIKE;
                    }
                    items.push(DrawItem::Text {
                        x: base_x + run.x_pt,
                        y,
                        size_pt: run.size_pt,
                        flags,
                        color_argb: run.color_argb,
                        family: run.family.clone(),
                        text: run.text.clone(),
                    });
                }
                yy += item.line_h_pt;
            }
            acc_bands += item.bands;
        }
        for m in self.foot.marks.get(page).map(Vec::as_slice).unwrap_or(&[]) {
            items.push(DrawItem::Text {
                x: m.x_pt,
                y: m.y_pt,
                size_pt: m.size_pt,
                flags: 0,
                color_argb: m.color_argb,
                family: m.family.clone(),
                text: m.text.clone(),
            });
        }
        items
    }

    /// 第 `page`（**正文頁**）的尾註錨點記號發射項。
    ///
    /// 記號發在正文引用處的行內終點（口徑與頁底註腳記號同款）——尾註的
    /// **條目**在尾註頁，**記號**在正文頁。兩者不可混：記號不進尾註頁。
    fn endnote_mark_items(&self, page: usize) -> Vec<DrawItem> {
        self.end
            .marks
            .get(page)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .map(|m| DrawItem::Text {
                x: m.x_pt,
                y: m.y_pt,
                size_pt: m.size_pt,
                flags: 0,
                color_argb: m.color_argb,
                family: m.family.clone(),
                text: m.text.clone(),
            })
            .collect()
    }

    /// 尾註條目文字發射（尾註頁與正文末頁接排共用；只有一份幾何算式）。
    fn push_endnote_item_text(
        &self,
        out: &mut Vec<DrawItem>,
        items: &[footnotes::NoteItem],
        content_left: f32,
        top0: f32,
        band: f32,
    ) {
        let mut acc_bands = 0u32;
        for item in items {
            let mut yy = top0 + acc_bands as f32 * band;
            for line in &item.lines {
                for run in &line.runs {
                    let y = if run.superscript {
                        yy - band * footnotes::SUP_RISE
                    } else {
                        yy
                    };
                    let mut flags = 0u8;
                    if run.bold {
                        flags |= FLAG_BOLD;
                    }
                    if run.italic {
                        flags |= FLAG_ITALIC;
                    }
                    if run.underline {
                        flags |= FLAG_UNDERLINE;
                    }
                    if run.strike {
                        flags |= FLAG_STRIKE;
                    }
                    out.push(DrawItem::Text {
                        x: content_left + run.x_pt,
                        y,
                        size_pt: run.size_pt,
                        flags,
                        color_argb: run.color_argb,
                        family: run.family.clone(),
                        text: run.text.clone(),
                    });
                }
                yy += item.line_h_pt;
            }
            acc_bands += item.bands;
        }
    }

    /// J1R：正文末頁的尾註接排發射項（分隔線＋條目文字）。
    ///
    /// 幾何由 [`footnotes::EndLayout::inline_top_y`] 決定（`compute_endnotes`
    /// 已扣該頁正文與頁底註腳佔用）；分隔線固定在條目區上方
    /// `endnote_sep_bands` 帶處，寬＝版心×1/3、版心左緣起。
    fn endnote_inline_items(&self) -> Vec<DrawItem> {
        if self.end.inline.is_empty() {
            return Vec::new();
        }
        let Some(geo) = self.end_geo() else {
            return Vec::new();
        };
        let Some(top0) = self.end.inline_top_y else {
            return Vec::new();
        };
        let band = self.params.line_h_pt();
        let content_w = self.end_content_w_pt();
        let sep_y = top0 - footnotes::endnote_sep_bands(band) as f32 * band;
        let (x1, x2) = footnotes::separator_span(geo.margin_left_pt, content_w);
        let mut out = vec![DrawItem::Rule {
            x1,
            y1: sep_y,
            x2,
            y2: sep_y,
            width_pt: 0.5,
            color_argb: DEFAULT_TEXT_COLOR_ARGB,
            dash: None,
        }];
        self.push_endnote_item_text(&mut out, &self.end.inline, geo.margin_left_pt, top0, band);
        out
    }

    /// 純尾註續頁（接排裝不下的部分）的顯示列表：頁首頁尾＋續接線＋條目文字。
    ///
    /// `idx`＝續頁序（0 起；實體頁號＝正文頁數＋idx）。超出回 `None`。
    /// 有接排區時所有續頁都是「續接線」（置中短線）；無接排區時第一頁仍是
    /// 傳統左緣起分隔線（F3b 原口徑）。
    fn endnote_display_list(&self, idx: usize) -> Option<PageDisplayList> {
        let items = self.end.pages.get(idx)?;
        let geo = self.end_geo()?;
        let band = self.params.line_h_pt();
        let content_w = self.end_content_w_pt();
        let body_pages = self.layout.page_count();
        let page = body_pages + idx;
        let mut out: Vec<DrawItem> = Vec::new();
        // 頁首頁尾：尾註頁也是實體頁（Word 一樣有頁首頁尾；槽位選擇取末節）。
        let synth = Page {
            index: page,
            lines: Vec::new(),
            line_bands: Vec::new(),
            tables: Vec::new(),
            images: Vec::new(),
            floating_images: Vec::new(),
            width_pt: geo.page_w_pt,
            height_pt: geo.page_h_pt,
            margin_left_pt: geo.margin_left_pt,
            margin_top_pt: geo.margin_top_pt,
            section_id: self
                .layout
                .pages()
                .last()
                .map(|p| p.section_id)
                .unwrap_or(0),
        };
        self.push_header_footer_items(&mut out, &synth);
        // 分隔線：版心左緣起、寬＝版心×1/3、y＝margin_top（無接排區的首頁）。
        // F3b：續頁頂部改畫**續接線**（置中短線；Word continuationSeparator
        // 的排版語義）——同 F2c 頁底註腳的置中算式（不改 F2c 那段碼）。
        // J1R：已有接排區 ⇒ 分隔線在正文末頁，續頁全部是續接線。
        if self.end.separator_top {
            let (x1, x2) = footnotes::separator_span(geo.margin_left_pt, content_w);
            let continuation = !self.end.inline.is_empty() || idx > 0;
            let (x1, x2) = if !continuation {
                (x1, x2)
            } else {
                // 續接線：分隔線跨度是「左緣起 × SEP_WIDTH_RATIO」，由此反推
                // 版心寬再置中（寬＝版心×1/3）。
                let left = x1;
                let cw = (x2 - x1) / footnotes::SEP_WIDTH_RATIO;
                (
                    left + cw * (1.0 - footnotes::SEP_WIDTH_RATIO) / 2.0,
                    left + cw * (1.0 + footnotes::SEP_WIDTH_RATIO) / 2.0,
                )
            };
            out.push(DrawItem::Rule {
                x1,
                y1: geo.margin_top_pt,
                x2,
                y2: geo.margin_top_pt,
                width_pt: 0.5,
                color_argb: DEFAULT_TEXT_COLOR_ARGB,
                dash: None,
            });
        }
        // 條目區：自分隔線佔帶之後起（margin_top ＋ 佔帶×帶尺）。
        let top0 = geo.margin_top_pt + footnotes::endnote_sep_bands(band) as f32 * band;
        self.push_endnote_item_text(&mut out, items, geo.margin_left_pt, top0, band);
        sort_display_items(&mut out, &[]);
        Some(PageDisplayList {
            page,
            width_pt: geo.page_w_pt,
            height_pt: geo.page_h_pt,
            items: out,
        })
    }

    /// 頁數＝**正文頁數 ＋ 純尾註續頁數**（接排進正文末頁的部分不另計頁）。
    ///
    /// 無尾註檔 ⇒ 與改前逐位元組相同（尾註層的零擾動快路徑）。
    pub fn page_count(&self) -> usize {
        self.layout.page_count() + self.end.pages.len()
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
    ///
    /// **只認正文頁**：尾註不是正文流行程的一環（尾註段不在 `layout` 的
    /// 段落流裡），故尾註頁沒有 caret 落點（回 None）。取捨：編輯面不觸
    /// 尾註——尾註文字的編輯屬後續卡（F3b），本卡只做發射。
    pub fn caret_rect_json(&self, para: usize, cp: usize) -> Option<String> {
        self.layout
            .caret_rect(CaretPos { para, cp })
            .and_then(|r| serde_json::to_string(&r).ok())
    }

    /// 頁面點擊 (page, x_pt, y_pt) → caret JSON（canvas→輸入端反向契約）。
    ///
    /// **只認正文頁**：尾註頁（`page >= body_page_count()`）不在版面層的
    /// 頁陣列裡 ⇒ 一律回 None（點尾註頁不落 caret）。
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

    /// 第 `page` 頁的顯示列表。
    ///
    /// 頁碼域：**正文頁 0..body_page_count**（末頁含尾註接排）＋
    /// **純尾註續頁 body_page_count..page_count**。超出回 `None`。
    pub fn page_display_list(&self, page: usize) -> Option<PageDisplayList> {
        let body_pages = self.layout.page_count();
        if page >= body_pages {
            // 純尾註續頁分支：正文發射碼原樣不動（接排只加在正文末頁）。
            return self.endnote_display_list(page - body_pages);
        }
        let p: &Page = self.layout.pages().get(page)?;
        let line_h = self.params.line_h_pt();
        let size = self.params.font_size_pt;
        let mut items: Vec<DrawItem> = Vec::new();
        let paras = self.eff_paragraphs();
        // 頁首頁尾先發射（填色/框線在文字之前，與表格同一約定）——其文字項
        // 由下方統一幾何排序落到閱讀序應有位置（頁首 y 小排最前、頁尾最後）。
        self.push_header_footer_items(&mut items, p);
        // K4e2 文字框配對：**消費式**逐一配對（每頁一份、用完即標記），池在
        // 表格切片與正文流之間共用（同一框只允許一條路徑發射文字）。配對條件
        // 取「宿主段落＋同寬」——不取高度：正文流／格內佔位框的高都被帶高
        // 鉗過（tdf130120：14.2pt 框鉗到 7.1pt），拿原始 extent 高比對會永不
        // 命中而吞字（實測：框內「7」整頁蒸發）。寬不经鉗制，是可靠鍵。
        let mut box_used = vec![false; self.shape_boxes.len()];
        // R6：`R6_CLIP_BUGGY=1` 回塞整組 R6 發射（裁切／z 序／宣告字級斷行）
        // ——紅燈 1-3 必回紅（回塞鎖）。
        let r6_buggy = std::env::var_os("R6_CLIP_BUGGY").is_some();
        // K4c-image 像素池：同 K4e2 紀律（每頁一份、消費式），三路徑
        // （表格切片→正文流→浮動）共用一池——同一張圖只發一次真像素。
        let mut px_used = vec![false; self.image_pixels.len()];
        // R5（R2）：多框宿主段的框內文字群組鍵。宿主段落 → 有實文字的框數
        // （≥2 才啟用，單框維持舊 y-sort）；宿主段末行 y 作群組錨點，讓框內
        // 文字落在宿主正文之後、後續段落之前。`txbx_keys` 逐項蓋章後交給
        // `sort_display_items`，不再被 y-sort 打散。
        let mut txbx_host_counts: HashMap<usize, usize> = HashMap::new();
        for (hp, b) in &self.shape_boxes {
            if b.txbx_texts.iter().any(|t| !t.trim().is_empty()) {
                *txbx_host_counts.entry(*hp).or_insert(0) += 1;
            }
        }
        let mut txbx_host_y: HashMap<usize, f32> = HashMap::new();
        for (i, l) in p.lines.iter().enumerate() {
            let band = p.line_bands.get(i).copied().unwrap_or(i as u32) as f32;
            let ly = p.margin_top_pt + band * line_h;
            txbx_host_y
                .entry(l.para_index)
                .and_modify(|e| {
                    if ly > *e {
                        *e = ly;
                    }
                })
                .or_insert(ly);
        }
        let mut txbx_keys: Vec<(usize, f32, i64, i64, i64)> = Vec::new();
        // 底層先畫表格（填色＋框線＋格內文字），再疊正文行；最後對「文字項」
        // 依 (y, x) 幾何閱讀序重排——跨表格/正文統一由上而下、由左而右。
        // 填色與框線保持推入序（穩定排序 key 同鍵，不互相顛倒遮蓋次序）。
        for tp in &p.tables {
            let canon_tbl = self.doc.tables.get(tp.slot).map(|bt| &bt.table);
            // K4e2：格內文字框配對的段落區間＝本槽佔用的扁平段落範圍
            // （含巢狀表；table_para_end 口徑）。
            let pr = self
                .doc
                .tables
                .get(tp.slot)
                .map(|bt| {
                    (
                        bt.para_start,
                        table_map::table_para_end(&bt.table, bt.para_start),
                    )
                })
                .unwrap_or((0, 0));
            push_frag_items(
                &mut items,
                tp.frag.x_pt,
                tp.frag.y_pt,
                &tp.frag,
                size,
                canon_tbl,
                paras,
                pr,
                Some((self.shape_boxes.as_slice(), &mut box_used)),
                // K4c-image：格內圖像素配對（池與正文流共用；para_range＋
                // 同寬為鍵，與文字框池同議）。
                Some((self.image_pixels.as_slice(), &mut px_used, &self.doc.parts)),
                line_h,
            );
        }
        // 圖片佔位（t_a1397522）：正文流圖＝版心左緣＋行網格帶 y；浮動鎖點
        // 圖＝layout 已按各軸 relativeFrom 原點系換算成頁絕對座標，直接發射。
        // K4c：先配對 shape 候補（真色版），配到→ emit_image_shape 吃真
        // 填充／描邊／虛線／filled=f；配不到＝None＝舊灰底零擾動。
        for im in &p.images {
            let y = p.margin_top_pt + im.band as f32 * line_h;
            // K4c 合併語意（像素×實色）：像素池命中（宿主段＋同寬）⇒ 發
            // DrawItem::Image 且**跳過發框**（真像素取代佔位框/實色框，同位
            // 不雙發）；未命中 ⇒ emit_image_shape 實色框（無 shape 候補＝
            // 灰底佔位，零擾動守門）。框內文字照常配對發射（消費式池）。
            let pix = take_image_pixel(
                (self.image_pixels.as_slice(), &mut px_used, &self.doc.parts),
                |s| s.para == im.para && (s.w_pt - im.w_pt).abs() < 0.01,
                p.margin_left_pt,
                y,
                im.w_pt,
                im.h_pt,
            );
            let hit = self.shape_boxes.iter().enumerate().find(|(i, (bp, b))| {
                !box_used[*i] && *bp == im.para && (b.w_pt - im.w_pt).abs() < 0.01
            });
            let box_im = hit.map(|(_, (_, b))| b);
            match pix {
                Some(it) => items.push(it),
                None => emit_image_shape(&mut items, p.margin_left_pt, y, im.w_pt, im.h_pt, box_im),
            }
            if let Some((bi, (_, box_im))) = hit {
                box_used[bi] = true;
                let start = items.len();
                emit_txbx_texts(
                    &mut items,
                    p.margin_left_pt,
                    y,
                    im.w_pt,
                    im.h_pt,
                    &box_im.txbx_texts,
                    txbx_runs_for(paras.get(im.para), box_im),
                    size,
                    line_h,
                    Some(box_im),
                    !r6_buggy,
                );
                push_txbx_keys(
                    &mut txbx_keys,
                    &items,
                    start,
                    im.para,
                    r6_ord(box_im, r6_buggy),
                    &txbx_host_counts,
                    &txbx_host_y,
                    y,
                );
            }
        }
        for fm in &p.floating_images {
            // K4c 合併語意（浮動圖同口徑）：像素命中發真像素並跳過發框，
            // 未命中發 emit_image_shape 實色框；框內文字照常配對。
            let pix = take_image_pixel(
                (self.image_pixels.as_slice(), &mut px_used, &self.doc.parts),
                |s| s.para == fm.para && (s.w_pt - fm.w_pt).abs() < 0.01,
                fm.x_pt,
                fm.y_pt,
                fm.w_pt,
                fm.h_pt,
            );
            // K4e2：浮動錨定文字框（DrawingML anchor）：框內文字落在換算後的鎖點。
            let hit = self.shape_boxes.iter().enumerate().find(|(i, (bp, b))| {
                !box_used[*i] && *bp == fm.para && (b.w_pt - fm.w_pt).abs() < 0.01
            });
            let box_b = hit.map(|(_, (_, b))| b);
            match pix {
                Some(it) => items.push(it),
                None => emit_image_shape(&mut items, fm.x_pt, fm.y_pt, fm.w_pt, fm.h_pt, box_b),
            }
            if let Some((bi, (_, b))) = hit {
                box_used[bi] = true;
                let start = items.len();
                emit_txbx_texts(
                    &mut items,
                    fm.x_pt,
                    fm.y_pt,
                    fm.w_pt,
                    fm.h_pt,
                    &b.txbx_texts,
                    txbx_runs_for(paras.get(fm.para), b),
                    size,
                    line_h,
                    Some(b),
                    !r6_buggy,
                );
                push_txbx_keys(
                    &mut txbx_keys,
                    &items,
                    start,
                    fm.para,
                    r6_ord(b, r6_buggy),
                    &txbx_host_counts,
                    &txbx_host_y,
                    fm.y_pt,
                );
            }
        }
        for (i, l) in p.lines.iter().enumerate() {
            let band = p.line_bands.get(i).copied().unwrap_or(i as u32) as f32;
            let y = p.margin_top_pt + band * line_h;
            let para = paras.get(l.para_index);
            // 欄起點（L1-columns）：欄序由 pack_run 蓋章在 Line::col；欄幾何
            // 從該頁起頭 section 的分欄模型取（頁內不分節混排時逐段欄序仍與
            // 起頭節一致——混節頁的 continuous 邊界會換頁重置欄序，近似記錄
            // 在案）。單欄 col_layout=None → base_x＝版心左緣，舊路徑零變化。
            let base_x = match self
                .sec_geo_all
                .get(&p.section_id)
                .and_then(quire_layout::col_layout)
            {
                Some(cl) if (l.col as usize) < cl.n => p.margin_left_pt + cl.col_x[l.col as usize],
                _ => p.margin_left_pt,
            };
            emit_line_run_items(&mut items, base_x, y, size, l, para);
        }
        // 頁底註腳層（F2b-2）：分隔線（Rule）＋條目文字＋正文錨點記號。
        // 文字項由下方穩定排序落進閱讀序（保留帶 y 最大 ⇒ 自然排最後）。
        if !self.foot.is_empty() {
            items.extend(self.footnote_items(page));
        }
        // 尾註層（F3）：只有**正文錨點記號**發在正文頁（條目在尾註頁）。
        // 尾註不預留、不擠正文——正文內容與分頁斷點與無尾註時逐位元組相同。
        if !self.end.is_empty() {
            items.extend(self.endnote_mark_items(page));
        }
        // J1R：尾註接排正文末頁（分隔線＋條目），不另計頁。
        if page + 1 == body_pages && !self.end.inline.is_empty() {
            items.extend(self.endnote_inline_items());
        }
        // 穩定排序：非文字項（填色/框線）同鍵保持原序且恆在前；文字項依基線 y
        // 再依 x 排列，讓「正文→表格→正文」的閱讀序與 PDF 抽取端一致。
        sort_display_items(&mut items, &txbx_keys);
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
        // 編輯落地後重算註腳排布（錨點所在段/頁可能移位；F2b-2 收斂）
        // 與尾註排布（F3／J1R：正文頁數／末頁剩餘空間可能變，要跟著重排）。
        self.recompute_notes();
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

/// [`LineSpacing`] → 段行高（pt）。三態換算：
/// - `Auto(m)`：字級×倍數（240＝單倍；與 fallback 12pt×1.4 同口徑）；
/// - `Exact(v)`：固定 v pt（文字超出亦不撐高）；
/// - `AtLeast(v)`：v 與「需求行高」取大（ECMA 語義：至少 v，行不低於字面高）。
///   需求由呼叫端決定：段內有更大 run 時餵「最大 run 字級×1.4」（K5 的字形
///   需求近似，與斷行量測同尺）；無更大 run 時餵段落基準字級（單倍下限就是
///   字級本身，不再多乘 1.4——否則 11pt 正文會在 12.65pt 帶尺上虛佔兩帶）。
/// - `None`：字級×1.4（既有硬編碼後備，零宣告檔案逐位元組不變）。
fn resolve_line_h_pt(size_pt: f32, line: Option<LineSpacing>) -> f32 {
    let s = size_pt.max(1.0);
    let h = match line {
        Some(LineSpacing::Auto(m)) => s * m.max(0.1),
        Some(LineSpacing::Exact(v)) => v,
        // AtLeast 的「需求」由呼叫端決定：段內有更大 run 時餵大字字級
        // （K5 的 1.4 倍近似在呼叫端完成），無大字時餵段落基準字級。
        Some(LineSpacing::AtLeast(v)) => v.max(s),
        None => s * 1.4,
    };
    h.max(1.0)
}

/// M1-band（簇 A）：文件帶尺細化——級聯**有宣告**時把粗帶換成宣告行高的
/// 最細等分（每帶 ≤ `BAND_REFINE_MAX_PT`），量化誤差從「最多一整粗帶」
/// 縮到 ≤2.8pt（K5-diag §2.6：15pt 表格列在 12.65pt 粗帶上吃 2 帶＝25.3pt、
/// LO 15.0，×1.69 過扣）。宣告行高本身佔恰整數帶（n 等分零誤差）；未宣告
/// 檔（呼叫端 gate）原值返還＝12pt×1.4 粗尺逐位元組不變。
/// 回傳（細化帶尺, n）：n＝宣告行高的等分份數＝**一行文字在細尺下應佔的
/// 帶數**（表格格內文字／頁首尾行等「一行＝一帶」假設的補償因子；未細化＝1）。
fn refine_band_h(declared_line_h: f32, declared: bool) -> (f32, u32) {
    if !declared || !declared_line_h.is_finite() || declared_line_h <= 0.0 {
        return (declared_line_h, 1);
    }
    // n＝把宣告行高等分成「每帶 ≤2.8pt」所需的最少份數（宣告值＝n×帶尺，
    // 精確整除由 f32 乘回保尺度一致）。
    let n = (declared_line_h / BAND_REFINE_MAX_PT).ceil().max(1.0);
    (declared_line_h / n, n as u32)
}

/// K6：檔內「主要宣告行高」——逐段量化前行高（含表格格內段落）的最常見值，
/// 下限為文件預設行高。細尺以此為錨等分，才能讓最常見的行距落在整數帶
/// （fdo68607 格內 14pt 正文＝17.1pt，錨 12.65pt 會量化成 17.71pt）。
/// 零宣告檔不呼叫（維持粗尺）。
fn modal_line_h(line_hs: &[f32], floor: f32) -> f32 {
    use std::collections::BTreeMap;
    let mut bins: BTreeMap<i32, u32> = BTreeMap::new();
    for &h in line_hs {
        if h.is_finite() && h > 0.0 {
            *bins.entry((h * 10.0).round() as i32).or_default() += 1;
        }
    }
    let mode = bins
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, _)| k as f32 / 10.0)
        .unwrap_or(floor);
    mode.max(floor)
}

/// K6：宣告族 → LO 實際渲染 face 的 real line-height 因子（`docs/k6-lineheight-calibration.md` §1）。
/// 鍵一律小寫（宣告族名 casefold）；查不到＝1.1499（Liberation 系）＋TODO。
/// **禁止**再引入 1.4／1.22／0.975 這類普適後備——那些是缺族表時代的湊數。
fn face_line_factor(face: &str) -> f32 {
    let key = face.trim().to_lowercase();
    match key.as_str() {
        // Calibri → Carlito；Cambria → Caladea。
        "calibri" | "carlito" => 1.2207,
        "cambria" | "caladea" => 1.1720,
        // Liberation 系（Times New Roman／Arial 在本基準環境都替到它）。
        "liberation serif" | "liberationserif" | "times new roman" | "times" => 1.1499,
        "arial" | "liberation sans" | "liberationsans" | "helvetica" => 1.1499,
        // theme 斷鏈 fallback（LO 落到自身預設 Noto Serif）。
        "noto serif" | "notoserif" => 1.3620,
        // CJK 宣告族在本基準環境替到 WenQuanYi Zen Hei。
        "pmingliu" | "新細明體" | "kaititc" | "標楷體" | "msyahei" | "microsoft yahei"
        | "微軟雅黑" | "wenquanyi zen hei" | "wenquanyizenhei" => 1.3496,
        // 查不到族：報告 §6.1 指定退回 Liberation 系。
        // TODO(K6)：補齊 fontTools hhea 實測族表；現階段不虛構新因子。
        _ => 1.1499,
    }
}

/// CJK 字元（用於選 eastAsia face；純拉丁 run 不吃 EA 宣告，報告 §3(4)）。
fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c as u32,
            0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF
            | 0xFF00..=0xFFEF)
    })
}

/// K7-cal：宣告族 → LO 實際渲染 face 的水平 advance 寬比
/// （`docs/k7-wrapwidth-diag.md` §3；K6 `face_line_factor` 的水平軸同構）。
/// 鍵一律小寫；**未命中＝1.0＝零擾動守門**（比照 K6 `has_fonts()＋calibrated`
/// 閘——表沒有實測值的族絕不猜數）。值＝A 軌（標準拉丁樣本串 hmtx 寬比
/// LO face／quire 內嵌 Noto 子集，14pt），B 軌（真實行逐行寬比 median）覆核：
/// Calibri 0.8834（B 軌 0.8747，fdo68607 n=418）；spread ±2.1% 屬可接受殘差。
/// face 來源：`~/.local/share/fonts/k6cal`、系統 Liberation、LO 25.8 自帶
/// Caladea／NotoSerif（矩陣重產：scratch `k7cal_faces2.py`）。
fn face_advance_scale(face: &str) -> f32 {
    let key = face.trim().to_lowercase();
    match key.as_str() {
        // Calibri → Carlito（LO 替換對，K6 §1 同基準）。
        "calibri" | "carlito" => 0.8834,
        // Cambria → Caladea（LO 自帶 face 實測；run67 記「待量」已清償）。
        "cambria" | "caladea" => 0.9545,
        // Liberation 系（Times New Roman／Arial 在本基準環境都替到它）。
        "liberation serif" | "liberationserif" | "times new roman" | "times" => 0.9648,
        "arial" | "liberation sans" | "liberationsans" | "helvetica" => 0.9950,
        // theme 斷鏈 fallback（LO 落到自身預設 Noto Serif，K6 同口徑）。
        "noto serif" | "notoserif" => 1.0328,
        // TODO(K7-cal)：Tahoma→LO 實際替代 face 未實測（VCL 替代表本機查無
        // 實據），**不虛構**——1.0 後備；補量後入表。
        _ => 1.0,
    }
}

/// K7-cal 總開關（回塞鎖）：`K7_WRAP_WIDTH_BUGGY=1` ⇒ 全表退 1.0＝舊斷行
/// ＋舊發射累加（比照 R2_BUGGY／K6_AUTO_PITCH_BUGGY 的 env 慣例）。
fn k7_advance_enabled() -> bool {
    std::env::var_os("K7_WRAP_WIDTH_BUGGY").is_none()
}

/// K7-cal：段「主宣告族」的水平 advance 尺＝首個**有文字** run 的有效字型族
/// 查表（無字 run 不決定主族；全無宣告＝1.0 零擾動）。僅乘 Latin/通用
/// cluster（shaping 層守 CJK 方塊不動）；斷行（ParaGrid）與發射段 x 累加、
/// caret 互逆對（同一量測模型）都走本函數，三者恆同尺。
fn para_advance_scale(runs: &[quire_ooxml::Run]) -> f32 {
    if !k7_advance_enabled() {
        return 1.0;
    }
    let fam = runs
        .iter()
        .find(|r| !r.text.is_empty() && r.effective_fonts.is_some())
        .or_else(|| runs.iter().find(|r| r.effective_fonts.is_some()))
        .and_then(|r| select_text_family(r.effective_fonts.as_ref(), &r.text));
    match fam {
        Some(f) => face_advance_scale(&f),
        None => 1.0,
    }
}

/// 單一 run 的 real line-height 因子：有文字時按實際 script 選 face
/// （CJK→eastAsia、其餘→ascii），無文字時退回 ascii/eastAsia 任一。
/// theme 斷鏈優先走 Noto Serif 1.3620。
fn run_line_factor(fonts: Option<&RunFonts>, text: &str) -> Option<f32> {
    let f = fonts?;
    if f.theme_unresolved {
        return Some(1.3620);
    }
    let face = if has_cjk(text) {
        f.east_asia.as_deref().or(f.ascii.as_deref())
    } else {
        f.ascii.as_deref().or(f.east_asia.as_deref())
    }?;
    Some(face_line_factor(face))
}

/// K6：一組目標行高 → 文件帶尺（細尺）。單一值沿用 M1 的 ≤2.8pt 等分
/// （宣告值恰佔整數帶）；多值（混級／表格多字級）搜尋「主要值 / n」中
/// 能同時表示全部目標的最粗尺（每值誤差 ≤0.12pt），避免單尺 ceil 把
/// k6 fixture 的 24.2pt 抬成 26.8pt。搜不到時落到 0.25pt 細尺（誤差 ≤0.125）。
fn choose_grid_line_h(targets: &[f32]) -> Option<(f32, u32)> {
    // 計次後只留「有意義」的行高：單一空段／裝飾段的 14.04pt 不該把整份
    // 17.1pt 表格拖去 0.25pt 細尺（fdo68607 實測 7 vs 8 頁）。低於主值
    // 1/10 出現次數者視為離群，各自按整數帶就近落帶。
    let mut bins: BTreeMap<i32, (f32, u32)> = BTreeMap::new();
    for &h in targets {
        if h.is_finite() && h > 0.0 {
            let e = bins.entry((h * 10.0).round() as i32).or_insert((h, 0));
            e.1 += 1;
        }
    }
    let max_count = bins.values().map(|(_, c)| *c).max().unwrap_or(0);
    let mut vals: Vec<f32> = bins
        .values()
        .filter(|(_, c)| *c * 10 >= max_count)
        .map(|(h, _)| *h)
        .collect();
    if vals.is_empty() {
        return None;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    vals.dedup_by(|a, b| (*a - *b).abs() <= 0.02);
    // 取「最常見值」本身而非 0.1pt 量化後的值：12.65pt 若先捨成 12.7pt，
    // 216 行的累積位移足以讓 Bug51170 的換頁跑掉（內容相似度 0.757→0.677）。
    let modal = bins
        .values()
        .max_by_key(|(_, c)| *c)
        .map(|(h, _)| *h)
        .unwrap_or(0.0);
    // 版面座標以 twip（1/20pt）為單位：錨點先落到 0.05pt 再切帶尺。
    // 12.6489pt（Times 11pt×1.1499）若不落 12.65，`spacing_band_vectors`
    // 的 f64 餘數會逐步漂移，Bug51170 的 10pt 段距樓梯在十幾段後翻帶。
    let modal = (modal * 20.0).round() / 20.0;
    if vals.len() == 1 {
        return Some(refine_band_h(modal, true));
    }
    // n 由小到大＝尺由粗到細；取第一個能同時命中的（偏好粗尺、少帶數）。
    let max_n = (modal / 0.25).ceil().max(1.0) as u32;
    for n in 1..=max_n {
        let g = modal / n as f32;
        if g > BAND_REFINE_MAX_PT {
            continue;
        }
        let err = vals
            .iter()
            .map(|h| ((h / g).round().max(1.0) * g - h).abs())
            .fold(0.0, f32::max);
        if err <= 0.12 {
            let lref = (modal / g).round().max(1.0) as u32;
            return Some((g, lref.min(quire_layout::MAX_PAGE_BANDS)));
        }
    }
    // 最細尺兜底：0.25pt（誤差 ≤0.125pt；仍遠優於一整粗帶）。
    let g = 0.25f32;
    let lref = (modal / g).round().max(1.0) as u32;
    Some((g, lref.min(quire_layout::MAX_PAGE_BANDS)))
}

/// K6：`word/document.xml.rels` 是否真的引用 `word/styles.xml`（報告 §3(2)）。
/// 無 styles 關聯的檔案，LO 會整支忽略 styles.xml（含 docDefaults）。
fn styles_referenced(parts: &BTreeMap<String, Vec<u8>>) -> bool {
    parts
        .get("word/_rels/document.xml.rels")
        .map(|xml| {
            read_rels(xml)
                .iter()
                .any(|r| r.target_part.as_deref() == Some("word/styles.xml"))
        })
        .unwrap_or(false)
}

/// K6：表格內段落索引 → 所屬表格樣式 id（巢狀表以內層樣式為準，缺則繼承外層）。
fn collect_table_styles(
    t: &quire_ooxml::Table,
    inherit: Option<&str>,
    out: &mut HashMap<usize, String>,
) {
    let sid = t.props.style.as_deref().or(inherit);
    for row in &t.rows {
        for cell in &row.cells {
            for b in &cell.blocks {
                match b {
                    quire_ooxml::Block::Paragraph { index } => {
                        if let Some(s) = sid {
                            out.entry(*index).or_insert_with(|| s.to_string());
                        }
                    }
                    quire_ooxml::Block::Table(inner) => collect_table_styles(inner, sid, out),
                }
            }
        }
    }
}

/// 帶尺細化上限（pt）：K5-diag §2.6 處方「帶尺細化 ≤2.8pt」。
const BAND_REFINE_MAX_PT: f32 = 2.8;

/// `word/document.xml` 的 `w:docGrid` 是否帶非零 `w:charSpace`（字元網格）。
///
/// 有＝文件要求 CJK 字元/行距對齊網格；本引擎未實作網格對齊。K4b 只在
/// 「純靠預設段落樣式鏈」的段距上把這類檔案排除，避免段落帶制與網格行距
/// 兩套垂直制疊加（reader-styles 3→4 頁的實測回歸）。docDefaults 有宣告
/// 的檔案仍走 K1 舊徑，不受此守門影響。
fn doc_grid_has_char_space(xml: &[u8]) -> bool {
    let Ok(s) = std::str::from_utf8(xml) else {
        return false;
    };
    let mut from = 0usize;
    while let Some(rel) = s[from..].find("docGrid") {
        let pos = from + rel;
        let start = s[..pos].rfind('<').unwrap_or(0);
        let end = s[pos..]
            .find('>')
            .map(|e| pos + e + 1)
            .unwrap_or(s.len())
            .min(s.len());
        let tag = &s[start..end];
        if tag.starts_with("<w:docGrid") || tag.starts_with("<docGrid") {
            if let Some(cp) = tag.find("charSpace=\"") {
                let val = &tag[cp + "charSpace=\"".len()..];
                if let Some(q) = val.find('"') {
                    let v = val[..q].trim();
                    if !v.is_empty() && v.parse::<i64>().map(|n| n != 0).unwrap_or(true) {
                        return true;
                    }
                }
            }
        }
        from = end.max(pos + 1);
    }
    false
}

/// 逐段（before, after）距 → 兩條整數帶向量；跨距累積小數餘數（見呼叫端
/// 注）。未宣告／非正值不消耗帶也不推進餘數。
///
/// K1-R：`breaks`＝段落間距量化餘數鏈的**斷點**（表格塊結束處）。表格塊的
/// 高度由表格幾何獨立計算（不是段落流的連續堆疊），段落間距的垂直連續性
/// 在表格邊界中斷——累積餘數不跨表格連續（餘數歸零重算），表格內部段落
/// 的段距因而不參與餘數鏈（它們也不佔頁面帶：表格列高已含其行高）。
fn spacing_band_vectors(
    spacings: &[(Option<f32>, Option<f32>)],
    grid_line_h: f32,
    breaks: &[usize],
) -> (Vec<u32>, Vec<u32>) {
    let mut before = Vec::with_capacity(spacings.len());
    let mut after = Vec::with_capacity(spacings.len());
    if !grid_line_h.is_finite() || grid_line_h <= 0.0 {
        before.resize(spacings.len(), 0);
        after.resize(spacings.len(), 0);
        return (before, after);
    }
    let mut residual = 0.0f64;
    for (i, (b, a)) in spacings.iter().enumerate() {
        // 鏈段起點：餘數歸零（表格塊之後的第一段起算新鏈段）。
        if breaks.contains(&i) {
            residual = 0.0;
        }
        let mut bands = [0u32; 2];
        for (k, pt) in [*b, *a].iter().enumerate() {
            let Some(v) = pt else { continue };
            if !v.is_finite() || *v <= 0.0 {
                continue;
            }
            let x = residual + (*v as f64 / grid_line_h as f64);
            let n = x.floor();
            residual = x - n;
            bands[k] = (n.max(0.0) as u32).min(quire_layout::MAX_PAGE_BANDS);
        }
        before.push(bands[0]);
        after.push(bands[1]);
    }
    (before, after)
}

type CascadeResult = (
    Vec<quire_ooxml::Paragraph>,
    Vec<(f32, f32)>,
    Vec<(Option<f32>, Option<f32>)>,
    // 逐段 marker 前綴碼點數（K2b；無 marker＝0）——`page_breaks` 原始碼點
    // 偏移要按此右移，斷頁位置才不會與插入的 marker 錯位。
    Vec<usize>,
);

/// 逐段級聯＋烘焙（t_5dfc8ac7；正文與頁首頁尾 part 共用同一條路徑）。
/// 回傳 (發射投影——run 粗/斜/底/刪/色/字級換成級聯有效值, 逐段
/// (斷行量測字級, 段行高 pt)，逐段有效段距 (before, after) pt)。
/// `def_size`＝文件基準字級（級聯全無宣告時）。
/// `widths`＝逐段可用寬（pt）：製表位點引填補的計算尺；短/空＝該段不展開
/// （無錨點不虛構——頁首尾 part 走空表＝tab 維持原樣，既有行為不變）。
/// K6：級聯的 auto-pitch 輸入束（表格樣式層＋是否走校準路徑）。綁成一個
/// 參數是為了讓 `cascade_paragraphs` 的參數數目守住 clippy 上限，語義不變。
struct PitchCascade<'a> {
    table_styles: &'a HashMap<usize, String>,
    calibrated: bool,
}

fn cascade_paragraphs(
    st: &StyleTable,
    paras: &[quire_ooxml::Paragraph],
    def_size: f32,
    doc_mult: f32,
    widths: &[f32],
    list_markers: &[Option<lists::ListMarker>],
    pitch: PitchCascade<'_>,
) -> CascadeResult {
    let mut eff_all: Vec<quire_ooxml::Paragraph> = Vec::with_capacity(paras.len());
    let mut metrics: Vec<(f32, f32)> = Vec::with_capacity(paras.len());
    // K1：逐段有效段前／段後距（pt，級聯結果；None＝全鏈未宣告）。
    let mut spacings: Vec<(Option<f32>, Option<f32>)> = Vec::with_capacity(paras.len());
    // K2b：逐段 marker 前綴碼點數（無 marker＝0；供 page_breaks 偏移校正）。
    let mut marker_chars: Vec<usize> = vec![0; paras.len()];
    for (pi, p) in paras.iter().enumerate() {
        let inline = ParaStyle {
            run: p.prun.clone().unwrap_or_default(),
            line: p.line,
            spacing_before_pt: p.spacing_before_pt,
            spacing_after_pt: p.spacing_after_pt,
            align: p.jc,
        };
        let (base, mut para) = st.effective_para(p.style.as_deref(), inline.clone());
        // K6：表格樣式插在「段落＞Normal 樣式＞表格樣式＞docDefaults」第三層。
        // 只有段落樣式鏈（含 Normal）完全沒宣告時才讓表格樣式露面——
        // effective_para 已把 docDefaults 兜進來，這裡用鏈查詢分辨層級。
        if let (true, Some(tid)) = (pitch.calibrated, pitch.table_styles.get(&pi)) {
            if inline.line.is_none() && st.para_chain_line(p.style.as_deref()).is_none() {
                if let Some(line) = st.table_style_line(tid) {
                    para.line = Some(line);
                }
            }
            let (cb, ca) = st.para_chain_spacing(p.style.as_deref());
            if inline.spacing_before_pt.is_none()
                && inline.spacing_after_pt.is_none()
                && cb.is_none()
                && ca.is_none()
            {
                let (tb, ta) = st.table_style_spacing(tid);
                if tb.is_some() {
                    para.spacing_before_pt = tb;
                }
                if ta.is_some() {
                    para.spacing_after_pt = ta;
                }
            }
        }
        let mut meas_size = base.size_pt.unwrap_or(def_size);
        // K6：逐 run 的 real line-height 需求（size × LH(face)）。混排行取
        // max over runs；CJK run 用 eastAsia face，純拉丁 run 不看 EA。
        let mut run_pitch = 0.0f32;
        let mut eff_runs: Vec<quire_ooxml::Run> = Vec::with_capacity(p.runs.len());
        for r in &p.runs {
            let mut b = base.clone();
            if let Some(cid) = r.rstyle.as_deref() {
                b = StyleTable::resolve_run(b, st.char_chain(cid).unwrap_or_default());
            }
            // 級聯末端兜底：只有整條鏈（直接格式／字元樣式／段落樣式／
            // docDefaults）都沒宣告顏色、底線時，超連結 run 才補合成預設。
            let eff = StyleTable::resolve_run(b, r.direct.clone().unwrap_or_default())
                .with_hyperlink_default(r.hyperlink_default);
            // 斷行字級＝段內 run 有效字級最大值（Word 行高/斷行依段內最大
            // 字級成形；本引擎斷行用單一尺，取大者保守不切字）。
            if let Some(sz) = eff.size_pt {
                meas_size = meas_size.max(sz);
            }
            // 未宣告字級的 run 仍吃文件基準字級（ECMA 預設），字型已宣告
            // 就必須進 real-LH（否則「只宣告字型」的檔會被舊 ×1.4 吃掉）。
            if let Some(lf) = run_line_factor(eff.fonts.as_ref(), &r.text) {
                run_pitch = run_pitch.max(eff.size_pt.unwrap_or(def_size) * lf);
            }
            let mut r2 = r.clone();
            // 三態 None＝全鏈未宣告＝未開（與舊「rPr 不見即 false」等價——
            // 舊 bool 欄位本就是 rPr 的有損投影）。
            // 修訂註解（L3-revisions；ECMA §17.13＋LO 默認視圖實測）：插入側
            // 底線／刪除側刪除線是**繪製層註解**（同 LO showChanges 的塗裝），
            // 不受級聯是否宣告 w:u/w:strike 影響——直接 OR，不做「鏈上已宣告
            // 即讓位」的兜底（那會把用戶宣告 w:u=none 誤當「不要修訂線」）。
            // 標記在解析期不打進 direct，正為不冒充直接格式越過級聯其他欄位。
            r2.bold = eff.bold.unwrap_or(false);
            r2.italic = eff.italic.unwrap_or(false);
            r2.underline = eff.underline.unwrap_or(false) || r.rev_insert;
            r2.strike = eff.strike.unwrap_or(false) || r.rev_delete;
            if eff.color_set {
                // 宣告 w:color（含 auto/非法→明確黑）以級聯為準；
                // 未宣告不動——保留解析期代擬色（hyperlink 藍）。
                r2.color = eff.color;
            }
            r2.size_pt = eff.size_pt;
            // 有效字型族（w:rFonts 逐欄級聯結果）：theme 引用已在 ooxml
            // 解析層逐層查表落地，此處只是把級聯結果烘焙進發射用投影，
            // 供 G1 選族消費。測量／分頁／存檔完全不經它（版面零變動）。
            r2.effective_fonts = eff.fonts;
            eff_runs.push(r2);
        }
        // K5：AtLeast 支路在段內有**更大 run** 時吃 meas_size（段內最大有效
        // run 字級×1.4，與斷行量測同尺）→ 行高隨大字撐高（LO/Word 語義）。
        // 無大字 run 時維持段落字級本身（不把基準字級再放大 1.4 倍——那會
        // 讓 11pt 正文在 12.65pt 帶尺上虛佔兩帶，Bug51170 由 5 頁膨脹到 9
        // 頁）。
        // K6：`auto` 行距基準字級改用**段內 run 有效字級**（rPr 級聯最大值），
        // 不是段落標記/docDefaults 的基準字級。LO 對 fdo68607 的 14pt 表格
        // 正文按 14pt×倍率排（實測 pitch 17.1），舊徑卻用 docDefaults 11pt
        // 落成 12.65pt（pitch 12.65，密 26%）。Exact/None 不動（None＝零
        // 宣告粗尺、Exact＝宣告固定 pt，皆無「倍率基準」語義）。
        // `K6_AUTO_PITCH_BUGGY=1` 回塞舊基準（紅燈回塞鎖）：auto 退回段落
        // 基準字級，fdo68607 行距回到 12.65pt 粗尺。
        let base_size = base.size_pt.unwrap_or(def_size);
        // K6：段落基準字型的字形需求（空段／無 run 字級時的唯一來源）。
        if let Some(lf) = run_line_factor(base.fonts.as_ref(), "") {
            run_pitch = run_pitch.max(base_size * lf);
        }
        // K6 校準公式（報告 §1）：pitch＝size×(declared/240)×LH(face)。
        // None ≡ 240（宣告倍率 1.0）；混排已在上方 max-over-runs 完成。
        // 無任何字型可查時退回舊徑（1.4／AtLeast 需求），零宣告守門不變。
        // K6_AUTO_PITCH_BUGGY=1 強制舊徑（紅燈回塞鎖）。
        let line_h = if pitch.calibrated && run_pitch > 0.0 {
            match para.line {
                Some(LineSpacing::Auto(m)) => run_pitch * m.max(0.1),
                Some(LineSpacing::Exact(v)) => v,
                Some(LineSpacing::AtLeast(v)) => v.max(run_pitch),
                None => run_pitch,
            }
        } else {
            let auto_buggy = std::env::var_os("K6_AUTO_PITCH_BUGGY").is_some();
            match para.line {
                Some(LineSpacing::AtLeast(_)) if meas_size > base_size => {
                    resolve_line_h_pt(meas_size * 1.4, para.line) * doc_mult
                }
                Some(LineSpacing::Auto(_)) if !auto_buggy => {
                    let size = meas_size.max(base_size);
                    resolve_line_h_pt(size, para.line) * 0.975
                }
                Some(LineSpacing::AtLeast(_)) => resolve_line_h_pt(base_size, para.line) * doc_mult,
                other => resolve_line_h_pt(base_size, other),
            }
        };
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
        // K2b：清單 marker run 插在發射投影的 eff run **之前**（存檔不經
        // 投影；無 numPr 段整段跳過＝零變化）。懸掛縮排以 marker 前的空白
        // 近似「首行可用寬扣減」——續行左縮排需版面層逐段寬支援，屬後續卡。
        if let Some(Some(marker)) = list_markers.get(pi) {
            let m = TextMeasurer::new(meas_size);
            let space_w = m.text_width(" ");
            let pad = if space_w > 0.0 {
                (marker.indent_pt / space_w).round().max(0.0) as usize
            } else {
                0
            };
            let text = format!("{}{}", " ".repeat(pad), marker.text);
            marker_chars[pi] = text.chars().count();
            let mut marker_run = eff_runs.first().cloned().unwrap_or_default();
            marker_run.text = text;
            marker_run.field = None;
            marker_run.note_ref = None;
            marker_run.tab = false;
            marker_run.hyperlink_default = false;
            eff_runs.insert(0, marker_run);
        }
        // K4e2：框內 run 走與正文**同一條**級聯（段落樣式基底→字元樣式鏈→
        // 直接格式），烘焙進發射投影；發射層據此畫框內的粗/斜/色/字級/字型族
        // （HC031 的 majorHAnsi 實據）。純發射資料——存檔路徑完全不經投影。
        let mut baked_txbx: Vec<Vec<quire_ooxml::Run>> = Vec::new();
        for grp in &p.txbx_runs {
            let mut out_g: Vec<quire_ooxml::Run> = Vec::with_capacity(grp.len());
            for r in grp {
                let mut b = base.clone();
                if let Some(cid) = r.rstyle.as_deref() {
                    b = StyleTable::resolve_run(b, st.char_chain(cid).unwrap_or_default());
                }
                let eff = StyleTable::resolve_run(b, r.direct.clone().unwrap_or_default())
                    .with_hyperlink_default(r.hyperlink_default);
                let mut r2 = r.clone();
                r2.bold = eff.bold.unwrap_or(false);
                r2.italic = eff.italic.unwrap_or(false);
                r2.underline = eff.underline.unwrap_or(false);
                r2.strike = eff.strike.unwrap_or(false);
                if eff.color_set {
                    r2.color = eff.color;
                }
                r2.size_pt = eff.size_pt;
                r2.effective_fonts = eff.fonts;
                out_g.push(r2);
            }
            baked_txbx.push(out_g);
        }
        metrics.push((meas_size, line_h));
        spacings.push((para.spacing_before_pt, para.spacing_after_pt));
        let mut ep = p.clone();
        ep.jc = para.align;
        ep.runs = eff_runs;
        if !baked_txbx.is_empty() {
            ep.txbx_runs = baked_txbx;
        }
        eff_all.push(ep);
    }
    (eff_all, metrics, spacings, marker_chars)
}

/// R5（R1）：取本框自帶的 run 組——解析層已在宿主段落 `txbx_texts` 的展平
/// 清單上蓋章 `txbx_start`（與 `txbx_runs` 同序），據此切片即「逐框對位」，
/// 不再用全域 txbx index 去撞每圖一組的 run（舊缺陷：白字框採到首框黑字、
/// 錯位碼點把整行在詞中鋸開）。
///
/// 容錯：起點越界或文字對不上（例如錯置 pict 被 K4d 抑制、解析游標未推進）
/// 時，退回在宿主段落裡按**完整文字片段**循序搜尋；仍找不到＝空 run 組
/// （老實退預設樣式，絕不串段）。`R5_R1_BUGGY=1` 還原舊行為供紅燈回塞。
fn txbx_runs_for<'a>(
    para: Option<&'a quire_ooxml::Paragraph>,
    box_im: &quire_ooxml::InlineImage,
) -> &'a [Vec<quire_ooxml::Run>] {
    let Some(p) = para else {
        return &[];
    };
    if box_im.txbx_texts.is_empty() {
        return &[];
    }
    if std::env::var_os("R5_R1_BUGGY").is_some() {
        // 回塞鎖：舊路徑＝整段合併的 run 清單（發射端再以每框 index 取樣）。
        return p.txbx_runs.as_slice();
    }
    // VML 候選解析層不蓋 children（見 drawing.rs txbxContent 起點）——維持舊
    // 「整段合併 run」口徑；否則逐 run 切段會把 VML 框的整句拆成多個文字項
    // （fdo76591 的 BELVIDERE 綠燈鎖：整句必須同一個 Text 項）。
    if box_im.txbx_children.is_empty() {
        return p.txbx_runs.as_slice();
    }
    let n = box_im.txbx_texts.len();
    let start = box_im.txbx_start;
    if let (Some(ts), Some(rs)) = (
        p.txbx_texts.get(start..start + n),
        p.txbx_runs.get(start..start + n),
    ) {
        if ts == box_im.txbx_texts.as_slice() {
            return rs;
        }
    }
    // 起點錯位（有候選被抑制、解析游標沒前進）：按文字內容循序找回。
    if n <= p.txbx_texts.len() {
        for pos in 0..=(p.txbx_texts.len() - n) {
            if p.txbx_texts[pos..pos + n] == box_im.txbx_texts {
                if let Some(rs) = p.txbx_runs.get(pos..pos + n) {
                    return rs;
                }
            }
        }
    }
    &[]
}

/// R5（R2）：框內 TEXT 項的抽取序——把子框按解析層給的段落數切段；DrawingML
/// 群組多子框時依 LO 內容流**反向文件序**（shapes-with-text 實據：Where 先於
/// rightArrow），群組內各段仍保文件序。單子框／VML＝文件序。
fn txbx_text_order(n: usize, children: &[usize]) -> Vec<usize> {
    if std::env::var_os("R5_R2_BUGGY").is_some() {
        return (0..n).collect();
    }
    let sum: usize = children.iter().sum();
    if children.len() <= 1 || sum != n {
        return (0..n).collect();
    }
    let mut spans = Vec::with_capacity(children.len());
    let mut at = 0usize;
    for &c in children {
        spans.push((at, at + c));
        at += c;
    }
    let mut order = Vec::with_capacity(n);
    for &(a, b) in spans.iter().rev() {
        order.extend(a..b);
    }
    order
}

/// K4e2：把宿主段落的 `txbx_texts` 發射進 shape 佔位框（LO 同款語義：
/// 框內文字按框寬斷行、落在框內；框本身仍由 `emit_image_placeholder` 撐位）。
/// 字級＝文件基準、x 內縮 3pt（內邊距近似）；**不做高度鉗**——寧可溢出框
/// 也不丟字（比對指標要文字在，LO 對超高框同樣不裁字）。空框段（""）留
/// 一行空白（段落分隔語義）。座標＝頁絕對（呼叫端已換算）。
#[allow(clippy::too_many_arguments)] // 框座標/尺寸/文字/run/樣式基準皆必要輸入，不塞結構體轉運
/// 框內幾何（K4c 契約 6/7）：`v:textbox/@inset` 四值（left,top,right,bottom
/// pt）餵文字起點與可用寬高；`txbx_rect`（組映射後的框內矩形）優先於整框；
/// `txbx_vertical`（layout-flow:vertical）→ 逐字堆疊；`txbx_table` → 框內
/// 表格逐格填色 Rect（n780563 三色列）。全 None/false/空＝舊行為零擾動。
fn emit_txbx_texts(
    items: &mut Vec<DrawItem>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    texts: &[String],
    runs: &[Vec<quire_ooxml::Run>],
    size_pt: f32,
    line_h: f32,
    shape: Option<&quire_ooxml::InlineImage>,
    r6: bool,
) {
    if w <= 0.0 {
        return;
    }
    // R6：只有 `wps:wsp` 標註吃「宿主框裁切＋z 序＋宣告字級斷行」；
    // VML／舊路徑逐位不動。`R6_CLIP_BUGGY=1` 由呼叫端把 r6 傳成 false。
    let wps = r6 && shape.is_some_and(|b| b.wps_annotation);
    // 框內矩形：組映射（txbx_rect）缺 → 整框（老實退化，不虛構位置）。
    let (bx, by, bw, bh) = match shape.and_then(|b| b.txbx_rect) {
        Some([dx, dy, rw, rh]) if rw > 0.0 && rh > 0.0 => (x + dx, y + dy, rw, rh),
        _ => (x, y, w, h),
    };
    // 宿主框裁切底線：標註文字不得越出宿主佔位框（LO 默認視圖口徑）。
    let clip_bottom = if wps { by + bh } else { f32::INFINITY };
    // inset 吃成內邊界（契約 6）：缺宣告＝舊硬近（x+3、inner_w＝w−6）。
    let inset = shape.and_then(|b| b.txbx_inset);
    let [inset_l, inset_t, inset_r, inset_b] = inset.unwrap_or([3.0, 4.0, 3.0, 0.0]);
    let old_style = inset.is_none();
    let inner_x = bx + inset_l;
    let inner_y = by + inset_t;
    let inner_w = if wps {
        // R6：DrawingML 標註無 VML inset 宣告時，LO 以近整框寬排字
        // （fdo74605 實據）；不再套 3pt 硬內縮，斷行才與 LO 對齊。
        (bw - 2.0).max(10.0)
    } else if old_style {
        (bw - 6.0).max(10.0)
    } else {
        (bw - inset_l - inset_r).max(10.0)
    };
    let inner_h = (bh - inset_t - inset_b).max(0.0);

    // ---- K4c：框內表格逐格填色（n780563 三色列表實據）：行按等分帶高、
    // 列按等分寬（格寬宣告缺＝近似；只發有色的格，None 格不發）。----
    if let Some(rows) = shape
        .map(|b| b.txbx_table.clone())
        .filter(|r| !r.is_empty())
    {
        let rh = (inner_h / rows.len() as f32).max(0.5);
        for (ri, row) in rows.iter().enumerate() {
            if row.is_empty() {
                continue;
            }
            let cw = inner_w / row.len() as f32;
            for (ci, cell) in row.iter().enumerate() {
                if let Some(argb) = cell {
                    items.push(DrawItem::Rect {
                        x: inner_x + ci as f32 * cw,
                        y: inner_y + ri as f32 * rh,
                        w: cw,
                        h: rh,
                        fill_argb: *argb,
                    });
                }
            }
        }
    }

    if texts.is_empty() {
        return;
    }
    // ---- R5（R2）：先按文件序排好各段 block（座標／換行照舊），再依
    // `txbx_text_order` 推入 items——群組子框只改抽取序，文字視覺落點
    // （y 連增）不被搬動。單子框／VML 的 order＝0..n，推入序與舊碼逐位相同。
    let order = txbx_text_order(
        texts.len(),
        shape.map(|b| b.txbx_children.as_slice()).unwrap_or(&[]),
    );
    let mut blocks: Vec<Vec<DrawItem>> = Vec::with_capacity(texts.len());

    // ---- K4c 契約 7：layout-flow:vertical → 逐字堆疊（y 單調、x 依框右緣
    // 起欄；既有 DrawItem::Text 逐字發射，零新變體）。欄寬取段字級近似。----
    let vertical = shape.is_some_and(|b| b.txbx_vertical);
    if vertical {
        for (ti, t) in texts.iter().enumerate() {
            let mut block: Vec<DrawItem> = Vec::new();
            if !t.is_empty() {
                let own_runs = runs.get(ti).map(Vec::as_slice).unwrap_or(&[]);
                let seg_size = own_runs.iter().find_map(|r| r.size_pt).unwrap_or(size_pt);
                let (flags, color_argb, _) =
                    own_runs
                        .first()
                        .map(run_style)
                        .unwrap_or((0, DEFAULT_TEXT_COLOR_ARGB, None));
                let fam = own_runs
                    .first()
                    .and_then(|r| select_text_family(r.effective_fonts.as_ref(), &r.text));
                let sz_eff = seg_size.max(1.0);
                // 欄起點＝框右緣往內退 inset_r 再退一欄寬（一個字 Advance 近似）。
                let col_w = TextMeasurer::new(sz_eff).text_width("n").max(sz_eff * 0.6);
                let col_x = (bx + bw - inset_r - col_w).max(bx);
                let step = (sz_eff * 1.2).max(1.0);
                let mut yy = inner_y;
                for c in t.chars() {
                    if yy > inner_y + inner_h + step {
                        break; // 超出框高：老實停發（不溢出到別頁別框）
                    }
                    block.push(DrawItem::Text {
                        x: col_x,
                        y: yy,
                        size_pt: sz_eff,
                        flags,
                        color_argb,
                        family: fam.clone(),
                        text: c.to_string(),
                    });
                    yy += step;
                }
            }
            blocks.push(block);
        }
    } else {
        let mut yy = inner_y;
        for (ti, t) in texts.iter().enumerate() {
            let mut block: Vec<DrawItem> = Vec::new();
            let own_runs0 = runs.get(ti).map(Vec::as_slice).unwrap_or(&[]);
            if t.trim().is_empty() {
                // 全空白／空段：有 run 就照發（HC031：框內只有大字号空白 run，
                // 樣式宣告是實內容；KP 斷行會 trim_end 吞掉純空白行，故繞過
                // break_lines 逐 run 直接發射一個佔位文字項），無 run＝純空白帶。
                if !own_runs0.is_empty() {
                    let mut xx = inner_x;
                    for r in own_runs0 {
                        let (flags, color_argb, sz) = run_style(r);
                        let fam = select_text_family(r.effective_fonts.as_ref(), &r.text);
                        let sz_eff = sz.unwrap_or(size_pt);
                        block.push(DrawItem::Text {
                            x: xx,
                            y: yy,
                            size_pt: sz_eff,
                            flags,
                            color_argb,
                            family: fam,
                            text: r.text.clone(),
                        });
                        xx += TextMeasurer::new(sz_eff).text_width(&r.text);
                    }
                }
                yy += line_h;
                blocks.push(block);
                continue;
            }
            // 段級字級：本框段首個宣告字級的 run（無＝文件基準）。斷行尺與發射
            // 尺同用本段字級（正文 cascade_paragraphs 同款思路的框內近似）。
            let own_runs = runs.get(ti).map(Vec::as_slice).unwrap_or(&[]);
            let seg_size = own_runs.iter().find_map(|r| r.size_pt).unwrap_or(size_pt);
            // R6：wps 標註用 run 自帶字級量寬（LO 實據：8pt 文字按 8pt 排），
            // 不再被文件基準字級（11pt）放大而多斷行；其餘路徑維持舊尺。
            let m = TextMeasurer::new(if wps { seg_size } else { seg_size.max(size_pt) });
            // 行→run 邊界對位借用一個「假段落」（runs＝本框段 runs），讓
            // emit_line_run_items 直接產出粗/斜/色/字級/字型族的同質段。
            let fake = quire_ooxml::Paragraph {
                runs: own_runs.to_vec(),
                ..Default::default()
            };
            for l in quire_layout::break_lines(t, inner_w, &m) {
                // R6：標註矩形越出宿主框底線 → 該行起不發（LO 默認視圖裁切）。
                if yy + line_h > clip_bottom + 0.01 {
                    break;
                }
                emit_line_run_items(&mut block, inner_x, yy, seg_size, &l, Some(&fake));
                yy += line_h;
            }
            blocks.push(block);
        }
    }
    for &ti in &order {
        if let Some(b) = blocks.get_mut(ti) {
            items.append(b);
        }
    }
}

/// 圖片佔位框配色（t_a1397522）：灰底＋淺灰描邊。「誠實的近似」視覺：
/// 用戶看得見這裡有圖、多大、在哪，點陣直出屬像素渲染卡。
pub(crate) const IMAGE_PLACEHOLDER_FILL: u32 = 0xFFF0_F0F0;
pub(crate) const IMAGE_PLACEHOLDER_EDGE: u32 = 0xFFCC_CCCC;
const IMAGE_PLACEHOLDER_RULE_PT: f32 = 0.75;

/// K4c：VML 布林欄位「明确假」判讀（Some(false)＝宣告不發；None/Some(true)
/// ＝照發）。
fn deco_off(flag: Option<bool>) -> bool {
    flag == Some(false)
}

/// 最小 base64 編碼（K4c-image，RFC 4648 標準字母＋`=` 補齊）。零新增依賴
/// 紀律：repo 無 base64 crate 且本卡禁止新增——編碼點必須在發射層
/// （`DrawItem::Image` 於 engine 構造，顯示列表要跨 wasm 進程自帶像素），
/// 故 helper 放本 crate 而非 native shell；SVG 端直接拼
/// `href="data:<mime>;base64,<data_b64>"`，渲染端零 IO。
fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for ch in data.chunks(3) {
        let n = ((ch[0] as u32) << 16)
            | ((ch.get(1).copied().unwrap_or(0) as u32) << 8)
            | ch.get(2).copied().unwrap_or(0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        // 尾組不足 3 位元組：多出的符號位以 `=` 補齊（RFC 4648 §4）。
        out.push(if ch.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if ch.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// K4c-image：媒體副檔名 → MIME（SVG 可渲染白名單）。不認識的
/// （**EMF/WMF/PDF** 等）→ None ⇒ 呼叫端退既有佔位框（n779834 浮水印
/// EMF 即走此路——本卡不解 EMF，誠實近似不虛構）。
fn mime_of_media_path(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

/// 像素候選 rId 序（文档序）：`embed_rids`（Choice＋Fallback 全備選）優先；
/// 空退回 `embed_rid`（舊投影快照無 rids 欄＝單候選語義不变）。
fn image_rid_candidates(im: &quire_ooxml::InlineImage) -> Vec<&str> {
    if !im.embed_rids.is_empty() {
        im.embed_rids.iter().map(|s| s.as_str()).collect()
    } else {
        im.embed_rid.as_deref().into_iter().collect()
    }
}

/// K4c-image 像素解析（PM 決策規則）：沿候選 rId 走**來源 part 自己的**
/// `.rels`（呼叫端傳入）→ `word/media/*`，可渲染格式＞不可渲染格式、皆可
/// 渲染取先見。查無 rId／rels 查無／懸空 part（不在 parts）／副檔名不認
/// ⇒ None ⇒ 發射層退既有佔位框（不可虛構、不可靜默丟）。
fn resolve_pixel_source(
    parts: &BTreeMap<String, Vec<u8>>,
    rels: &[quire_ooxml::Relationship],
    rids: &[&str],
) -> Option<(String, String)> {
    for rid in rids {
        let Some(part) = resolve_relationship(rels, rid) else {
            continue;
        };
        // 指向不存在 part 的懸空 rId → 繼續找下一個備選（容錯跳過）。
        if !parts.contains_key(&part) {
            continue;
        }
        let Some(mime) = mime_of_media_path(&part) else {
            continue;
        };
        return Some((part, mime.to_string()));
    }
    None
}

/// 來源 part 路徑 → 它的 `.rels` part 路徑（OPC §3：`<dir>/_rels/<file>.rels`）。
/// 頁首尾 part 有自家的 rels（Bug51170 的 header1 圖 rId 住在
/// `word/_rels/header1.xml.rels`，不是 document.xml.rels）。
fn rels_path_for(part_path: &str) -> String {
    let (dir, file) = part_path.rsplit_once('/').unwrap_or(("", part_path));
    if dir.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{dir}/_rels/{file}.rels")
    }
}

/// K4c-image 像素池配對上下文：(像素池, 消費旗標, parts 索引)。消費式配對
/// 與 K4e2 文字框池同一紀律——同一張圖只允許一條發射路徑取用一次。
type PixelPool<'a> = (
    &'a [PixelSource],
    &'a mut Vec<bool>,
    &'a BTreeMap<String, Vec<u8>>,
);

/// 配對取像素：池中找到**未消費**且 `pred` 命中者 → 發 `DrawItem::Image`
/// （真像素，base64 內嵌 data URI）並標記消費；找不到 → None，呼叫端發
/// 既有佔位框。非法尺寸（w/h≤0）＝不發像素（同佔位框容錯口徑）。
fn take_image_pixel(
    pool: PixelPool<'_>,
    pred: impl Fn(&PixelSource) -> bool,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Option<DrawItem> {
    if w <= 0.0 || h <= 0.0 {
        return None; // 非法尺寸老實不發射（容錯鐵律，同佔位框）
    }
    let (pixels, used, parts) = pool;
    let (i, src) = pixels
        .iter()
        .enumerate()
        .find(|(i, s)| !used[*i] && pred(s))?;
    let bytes = parts.get(&src.src.0)?; // parts 被編輯抽掉 → 退佔位框（誠實）
    used[i] = true;
    Some(DrawItem::Image {
        x,
        y,
        w,
        h,
        mime: src.src.1.clone(),
        data_b64: base64_encode(bytes),
        alt: None,
    })
}

/// 發射圖片佔位框（K4c 升級：真色版）。`shape`=None 或全欄未宣告＝舊行為
/// （灰底 Rect＋四邊淺灰 Rule，零擾動守門）；有宣告時（卡片契約 2/3/4）：
/// - 填充：`filled="f"` → 整個 Rect 不發（**別把無填充畫成灰底**）；否則
///   `fill_argb`（已按 `v:fill/@color` > `o:fillcolor` 合併）優先、缺→佔位灰。
/// - 框線：`stroked="f"` → 四條 Rule 不發；否則色 `stroke_argb` 缺→佔位邊、
///   線寬 `stroke_width_pt` 缺→0.75pt、虛線 `stroke_dash`（SVG dasharray）透傳。
fn emit_image_shape(
    items: &mut Vec<DrawItem>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    shape: Option<&quire_ooxml::InlineImage>,
) {
    if w <= 0.0 || h <= 0.0 {
        return; // 非法尺寸老實不發射（容錯鐵律）
    }
    let fill = if shape.is_some_and(|b| deco_off(b.filled)) {
        None
    } else {
        Some(
            shape
                .and_then(|b| b.fill_argb)
                .unwrap_or(IMAGE_PLACEHOLDER_FILL),
        )
    };
    if let Some(argb) = fill {
        items.push(DrawItem::Rect {
            x,
            y,
            w,
            h,
            fill_argb: argb,
        });
    }
    if shape.is_some_and(|b| deco_off(b.stroked)) {
        return;
    }
    let color = shape
        .and_then(|b| b.stroke_argb)
        .unwrap_or(IMAGE_PLACEHOLDER_EDGE);
    let width_pt = shape
        .and_then(|b| b.stroke_width_pt)
        .unwrap_or(IMAGE_PLACEHOLDER_RULE_PT);
    let dash = shape.and_then(|b| b.stroke_dash.clone());
    let rule = |x1: f32, y1: f32, x2: f32, y2: f32, items: &mut Vec<DrawItem>| {
        items.push(DrawItem::Rule {
            x1,
            y1,
            x2,
            y2,
            width_pt,
            color_argb: color,
            dash: dash.clone(),
        })
    };
    rule(x, y, x + w, y, items);
    rule(x, y + h, x + w, y + h, items);
    rule(x, y, x, y + h, items);
    rule(x + w, y, x + w, y + h, items);
}

/// 是否為 CJK 碼點（供「只有 eastAsia 宣告」時的全中文判斷）。
/// 涵蓋 CJK 部首／標點、假名、統一漢字（含擴充 A）、相容漢字與諺文；
/// 範圍取自 Unicode 區塊配置，非語言學完備分類。
fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x2E80..=0x2EFF
            | 0x3000..=0x303F
            | 0x3040..=0x30FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0xAC00..=0xD7AF
    )
}

/// 依 run 的有效字型宣告選出顯示列表要用的字型族名（v1 規則）。
///
/// 選取順序：
/// 1. `hint_east_asian == true` 且 `east_asia` 有值 → `east_asia`；
/// 2. 否則 `ascii` 有值 → `ascii`；
/// 3. 否則 `east_asia` 有值且 `text` 非空且全為 CJK 碼點 → `east_asia`；
/// 4. 皆無 → `None`（渲染端沿用自身預設，即舊行為）。
///
/// `cs`（複雜文字）本版不消費（引擎尚無 RTL 路徑）；`asciiTheme`／
/// `eastAsiaTheme` 等 theme 引用已在 ooxml 解析層查 fontScheme 落地成
/// `ascii`／`east_asia` 值（theme 取勝），本函數只讀落地後的值。表格格內
/// 文字（CellText）目前不帶 run 級字型（layout 層限制），發射時傳 `None`，
/// 格內選族待後續卡。
fn select_text_family(fonts: Option<&quire_ooxml::RunFonts>, text: &str) -> Option<String> {
    let f = fonts?;
    if f.hint_east_asian {
        if let Some(ea) = &f.east_asia {
            return Some(ea.clone());
        }
    }
    if let Some(ascii) = &f.ascii {
        return Some(ascii.clone());
    }
    if let Some(ea) = &f.east_asia {
        if !text.is_empty() && text.chars().all(is_cjk) {
            return Some(ea.clone());
        }
    }
    None
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

/// 行內同質段：(起始碼點, 結束碼點, flags, color_argb, 級聯字級, 級聯字型族)。
type LineSegment = (
    usize,
    usize,
    u8,
    u32,
    Option<f32>,
    Option<quire_ooxml::RunFonts>,
);

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
    // K7-cal：段級水平尺（＝KP 斷行同尺——同由 para_advance_scale 出，行寬
    // 與 x 累加恆等；未宣告族/回塞鎖＝1.0＝舊尺逐位元組不變）。
    let adv = para.map_or(1.0, |p| para_advance_scale(&p.runs));
    let char_indices: Vec<(usize, char)> = line.text.char_indices().collect();
    let line_len_cp = char_indices.len();
    if line_len_cp == 0 {
        let first = para.and_then(|p| p.runs.first());
        let (flags, color_argb, sz, family) = match first {
            Some(r) => {
                let (f, c, s) = run_style(r);
                (f, c, s, select_text_family(r.effective_fonts.as_ref(), ""))
            }
            None => (0, DEFAULT_TEXT_COLOR_ARGB, None, None),
        };
        items.push(DrawItem::Text {
            x: base_x,
            y,
            size_pt: sz.unwrap_or(size_pt),
            flags,
            color_argb,
            family,
            text: String::new(),
        });
        return;
    }

    let line_start = line.char_offset;
    let line_end = line_start + line_len_cp;
    // (行內起始碼點, 行內結束碼點, flags, color_argb, 級聯字級, 級聯字型族)
    let mut segments: Vec<LineSegment> = Vec::new();
    let mut push_seg = |s: usize,
                        e: usize,
                        flags: u8,
                        color: u32,
                        size: Option<f32>,
                        fonts: Option<quire_ooxml::RunFonts>| {
        if s >= e {
            return;
        }
        if let Some(last) = segments.last_mut() {
            // 相鄰且樣式相同時合併為單一可繪段（沿用舊版鍵：字型宣告不參與
            // 合併，否則相鄰同款式 run 只因字型不同就被拆成多個 `<text>`，
            // 會破壞「SVG 僅新增 font-family 屬性」的零版面變動契約）。
            // 合併段的選族採**首個 run** 的宣告（見下方發射處）。
            if last.1 == s && last.2 == flags && last.3 == color && last.4 == size {
                last.1 = e;
                return;
            }
        }
        segments.push((s, e, flags, color, size, fonts));
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
                    push_seg(covered, rel_s, 0, DEFAULT_TEXT_COLOR_ARGB, None, None);
                }
                let (f, c, sz) = run_style(r);
                push_seg(rel_s, rel_e, f, c, sz, r.effective_fonts.clone());
                covered = rel_e;
            }
        }
    }
    if covered < line_len_cp {
        push_seg(covered, line_len_cp, 0, DEFAULT_TEXT_COLOR_ARGB, None, None);
    }

    let byte_of = |cp: usize| -> usize {
        char_indices
            .get(cp)
            .map(|(b, _)| *b)
            .unwrap_or(line.text.len())
    };
    let mut x_off = 0.0f32;
    for (rel_s, rel_e, flags, color_argb, run_size, run_fonts) in segments {
        let seg_text = &line.text[byte_of(rel_s)..byte_of(rel_e)];
        let seg_size = run_size.unwrap_or(size_pt);
        items.push(DrawItem::Text {
            x: base_x + x_off,
            y,
            size_pt: seg_size,
            flags,
            color_argb,
            family: select_text_family(run_fonts.as_ref(), seg_text),
            text: seg_text.to_string(),
        });
        // 零漂移契約：無宣告段（seg_size==size_pt）與舊實作同尺；宣告段按自身尺。
        // K7-cal：水平尺經 measurer 選擇性乘入（與 KP 斷行同徑，adv=1.0 零擾動）。
        x_off += TextMeasurer::with_advance_scale(seg_size, adv).text_width(seg_text);
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

/// K4e2 格內文字框配對池：(文字框候選, 消費旗標)。消費式配對——同一框只
/// 允許被一條發射路徑（正文流／格內切片）取用一次，杜絕雙發。
type ShapePool<'a> = (&'a [(usize, quire_ooxml::InlineImage)], &'a mut Vec<bool>);

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
    // K4e2：格內文字框配對上下文——para_range＝本槽（含巢狀表）佔用的扁平
    // 段落區間；shape＝(文字框池, 消費旗標)（與正文流共用一池，杜絕雙發）；
    // 頁首尾表格無宿主段落＝傳 None（零擾動）。
    para_range: (usize, usize),
    mut shape: Option<ShapePool<'_>>,
    // K4c-image：格內圖像素配對池（正文流路徑傳 Some；頁首尾表格傳 None
    // ——HF 圖的像素走 `push_hf_slot` 自家解析，格內 HF 圖維持佔位框）。
    mut pixels: Option<PixelPool<'_>>,
    line_h: f32,
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
            // 框線：筆畫居中畫在格線（cell 邊界）上——與 Word／LO 口徑一致。
            // 舊實作把四端各自內縮半個線寬：水平線變成「上端下移＋下端上移」
            // 的斜線（Bug54771b 實證 y1=74.25≠y2=69.75）；若只內縮又會讓
            // 相鄰格共用邊各移各的→並排雙倍線寬＋角隅缺角。居中發射兩者皆免，
            // 共用邊由相鄰格發出同座標重疊線（同色重繪＝視覺同條）。
            let line =
                |pen: &Pen, x1: f32, y1: f32, x2: f32, y2: f32, items: &mut Vec<DrawItem>| {
                    if (x2 - x1).abs() <= 1e-3 && (y2 - y1).abs() <= 1e-3 {
                        return; // 退化邊（零長度）不發射
                    }
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
                // L2-cellruns（TODO(CardA) 清償）：`t.runs` 非空＝本行有 ≥2
                // 種 run 樣式（來源端已過濾，單樣式行恆為空），按行內
                // [from,to) 平鋪段拆多個 DrawItem::Text——拼接逐字元等於
                // 整行。段 x＝按該段發射字級累加前段寬（正文
                // `emit_line_run_items` 同款零漂移口徑）。空 runs＝整行單
                // 樣式：原封不動走舊整格單樣式路徑（金樣／儀表板零擾動）。
                if t.runs.is_empty() {
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
                        // 格內文字目前不帶 run 級字型（layout 層限制）；格內選族待後續卡。
                        family: None,
                        text: t.text.clone(),
                    });
                    continue;
                }
                // 多樣式行：碼點→字节邊界表（平鋪段切片用）。
                let char_idx: Vec<usize> = {
                    let mut v: Vec<usize> = t.text.char_indices().map(|(b, _)| b).collect();
                    v.push(t.text.len());
                    v
                };
                let base_size = t.size_pt.unwrap_or(size);
                let mut x_off = 0.0f32;
                for r in &t.runs {
                    let s = r.from.min(char_idx.len() - 1);
                    let e = r.to.max(s).min(char_idx.len() - 1);
                    if s >= e {
                        continue;
                    }
                    let seg = &t.text[char_idx[s]..char_idx[e]];
                    let seg_size = r.size_pt.unwrap_or(base_size).max(1.0);
                    items.push(DrawItem::Text {
                        x: base_x + t.x_pt + x_off,
                        y: y + t.y_pt,
                        size_pt: seg_size,
                        flags: r.flags,
                        color_argb: r.color_argb.unwrap_or(DEFAULT_TEXT_COLOR_ARGB),
                        // 格內 run 級字型族未透傳（layout 層限制）——選族屬後續卡。
                        family: None,
                        text: seg.to_string(),
                    });
                    // 零漂移口徑（emit_line_run_items 同款）：未宣告段與整行
                    // 同尺；宣告段按自身尺（Word 同款視覺近似）。
                    x_off += TextMeasurer::new(seg_size).text_width(seg);
                }
            }
            // 圖片佔位框（t_a1397522）＋ K4c 真色版（配到 shape 候補時吃
            // 真填充／描邊／filled=f／stroked=f；配不到＝灰底零擾動）。
            for im in &cell.images {
                let ix = base_x + im.x_pt;
                let iy = base_y + ry + im.y_pt;
                // K4c-image 合併語意：格內圖真像素優先。配對鍵＝宿主段落
                // （本槽 para_range 內）＋寬：切片寬是被格寬鉗過的原寬（只縮
                // 不放），故放寬為「原寬 ≥ 切片寬」的首見未消費項（高同樣被
                // 帶高鉗過，不可作鍵——K4e2 教訓）。像素命中發真像素並跳過
                // 發框；未命中 ⇒ emit_image_shape 實色框（無候補＝灰底佔位）。
                let pix = pixels.as_mut().and_then(|pool| {
                    take_image_pixel(
                        (pool.0, pool.1, pool.2),
                        |s| {
                            s.para >= para_range.0
                                && s.para < para_range.1
                                && s.w_pt >= im.w_pt - 0.01
                        },
                        ix,
                        iy,
                        im.w_pt,
                        im.h_pt,
                    )
                });
                let hit = shape.as_mut().and_then(|(boxes, used)| {
                    boxes.iter().enumerate().find(|(i, (bp, b))| {
                        !used[*i]
                            && *bp >= para_range.0
                            && *bp < para_range.1
                            && (b.w_pt - im.w_pt).abs() < 0.01
                    })
                });
                match pix {
                    Some(it) => items.push(it),
                    None => {
                        emit_image_shape(items, ix, iy, im.w_pt, im.h_pt, hit.map(|(_, (_, b))| b))
                    }
                }
                // K4e2：格內文字框（tdf130120 實據——圖框錨在儲存格段落）。
                if let Some((bi, (bp, b))) = hit {
                    if let Some((_, used)) = shape.as_mut() {
                        used[bi] = true;
                    }
                    emit_txbx_texts(
                        items,
                        ix,
                        iy,
                        im.w_pt,
                        im.h_pt,
                        &b.txbx_texts,
                        txbx_runs_for(paragraphs.get(*bp), b),
                        size,
                        line_h,
                        Some(b),
                        std::env::var_os("R6_CLIP_BUGGY").is_none(),
                    );
                }
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
                    para_range,
                    shape.as_mut().map(|(b, u)| (*b, &mut **u)),
                    // K4c-image：子表與母格共用像素池（消費旗標隨遞迴透傳）。
                    pixels.as_mut().map(|(p, u, parts)| (*p, &mut **u, *parts)),
                    line_h,
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
            cols: Default::default(),
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
        // K7-cal：水平尺傳空表＝頁首尾表格維持舊斷行（本卡範圍＝正文表；
        // hf 表換算風險未量，老實零擾動，屬後續卡）。
        let hf_line_hs: Vec<f32> = part.metrics.iter().map(|m| m.1).collect();
        let slots_tbl = table_map::build_slots(
            &part.tables,
            &texts,
            styles,
            hf_params,
            &sec_one,
            &[],
            &paragraphs,
            &hf_line_hs,
            self.line_ref_bands,
            &[],
        );
        // 流項＝段落行＋表格依序排列（簡化 packer：頁首本就一小節，不跨頁，
        // 表格整張放進自身帶高，超出部分由頁首自然向下延伸——重疊風險見模組注）。
        // 斷行走級聯逐段字級（t_5dfc8ac7；無宣告＝hf_params 全域尺＝舊行為）。
        let grid: Vec<(f32, f32)> = part.metrics.clone();
        let pls = quire_layout::layout_paragraphs_grid(&texts, hf_params, &[], &grid);
        let mut cursor_bands: u32 = 0;
        let mut ti = 0usize;
        let mut skip_until = 0usize;
        let buggy = std::env::var_os("K9_HF_ALIGN_BUGGY").is_some();
        let content_w = hf_params.content_w_pt();
        let margin_left = hf_params.margin(MARGIN_LEFT);

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
                let para = paragraphs.get(l.para_index);
                if buggy {
                    emit_line_run_items(items, margin_left, y, hf_params.font_size_pt, l, para);
                } else if let Some(p) = para {
                    if p.runs.iter().any(|r| r.tab || r.ptab.is_some()) {
                        emit_hf_tab_zones(items, y, hf_params, l, p);
                    } else {
                        let line_base_x = match p.jc {
                            Some(quire_ooxml::Align::Center) => {
                                margin_left + (content_w - l.width_pt).max(0.0) / 2.0
                            }
                            Some(quire_ooxml::Align::Right) => {
                                margin_left + (content_w - l.width_pt).max(0.0)
                            }
                            _ => margin_left,
                        };
                        emit_line_run_items(
                            items,
                            line_base_x,
                            y,
                            hf_params.font_size_pt,
                            l,
                            Some(p),
                        );
                    }
                } else {
                    emit_line_run_items(items, margin_left, y, hf_params.font_size_pt, l, None);
                }
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
        // K4a：頁首／頁尾 part 的圖片佔位（`wp:drawing` inline；`read_images`
        // 同一投影原樣複用）。y 起點＝slot 發射距，同 part 多圖按所屬段落
        // 疊行帶；x＝版心左緣。格內圖由表格切片負責，本輪不重複發射。
        // 無圖（空表）⇒ 迴圈零次 ⇒ 既有顯示列表逐位元組不變（零變化守門）。
        for (idx, im) in part.images.iter().enumerate() {
            if im.in_cell {
                continue;
            }
            let y = emit_y + im.para_index as f32 * line_h;
            let para = paragraphs.get(im.para_index);
            let img_x = if buggy {
                margin_left
            } else {
                match para.and_then(|p| p.jc) {
                    Some(quire_ooxml::Align::Center) => {
                        margin_left + (content_w - im.w_pt).max(0.0) / 2.0
                    }
                    Some(quire_ooxml::Align::Right) => margin_left + (content_w - im.w_pt).max(0.0),
                    _ => margin_left,
                }
            };
            // K4c 合併語意：HF 圖像素＝開檔期按本 part 自家 .rels 解析（part.pixels
            // 與 images 逐位對齊；Bug51170 Choice→pdf 不可渲染、Fallback→png 可
            // 渲染即在此翻綠）。命中發真像素並跳過發框；未命中發 emit_image_shape
            // （HF 候選本身就是 InlineImage——真色直走 shape 版，None 才退灰佔位）。
            let pix = part.pixels.get(idx).and_then(|p| p.as_ref()).and_then(
                |(path, mime)| -> Option<DrawItem> {
                    if im.w_pt <= 0.0 || im.h_pt <= 0.0 {
                        return None;
                    }
                    let bytes = self.doc.parts.get(path)?;
                    Some(DrawItem::Image {
                        x: img_x,
                        y,
                        w: im.w_pt,
                        h: im.h_pt,
                        mime: mime.clone(),
                        data_b64: base64_encode(bytes),
                        alt: None,
                    })
                },
            );
            match pix {
                Some(it) => items.push(it),
                None => emit_image_shape(items, img_x, y, im.w_pt, im.h_pt, Some(im)),
            }
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
            // K4e2：頁首尾表格無宿主正文段落＝無文字框配對（零擾動）。
            (0, 0),
            None,
            // K4c-image：頁首尾表格的格內圖維持佔位框（HF 像素只供正文流
            // 路徑的 push_hf_slot 圖；格內 HF 圖屬後續卡，零擾動守門）。
            None,
            hf_params.line_h_pt(),
        );
    }
}

/// K9：頁首/頁尾含 tab/ptab 之段落發射。按製表符劃分區域，停靠於 CENTER/RIGHT 停靠點。
fn emit_hf_tab_zones(
    items: &mut Vec<DrawItem>,
    y: f32,
    hf_params: &LayoutParams,
    line: &quire_layout::Line,
    para: &quire_ooxml::Paragraph,
) {
    let content_w = hf_params.content_w_pt();
    let margin_left = hf_params.margin(MARGIN_LEFT);

    let mut zones: Vec<(Option<quire_ooxml::Align>, Vec<quire_ooxml::Run>)> = Vec::new();
    let mut cur_runs = Vec::new();
    let mut cur_align = para.jc;
    let mut tab_count = 0usize;

    for r in &para.runs {
        if r.tab || r.ptab.is_some() {
            let n_tabs = if r.tab {
                r.tab_count.max(1) as usize
            } else {
                1
            };
            for _ in 0..n_tabs {
                zones.push((cur_align, std::mem::take(&mut cur_runs)));
                let tab_align = if let Some(a) = r.ptab {
                    a
                } else if tab_count == 0 {
                    if para.tab_center_pt.is_some() {
                        quire_ooxml::Align::Center
                    } else if para.tab_right_pt.is_some() {
                        quire_ooxml::Align::Right
                    } else {
                        quire_ooxml::Align::Center
                    }
                } else {
                    quire_ooxml::Align::Right
                };
                cur_align = Some(tab_align);
                tab_count += 1;
            }
            if !r.text.is_empty() {
                let mut text_r = r.clone();
                text_r.tab = false;
                text_r.tab_count = 0;
                text_r.ptab = None;
                cur_runs.push(text_r);
            }
        } else {
            cur_runs.push(r.clone());
        }
    }
    zones.push((cur_align, cur_runs));

    for (align, z_runs) in zones {
        let text: String = z_runs.iter().map(|r| r.text.as_str()).collect();
        if text.trim().is_empty() {
            continue;
        }
        let adv = para_advance_scale(&z_runs);
        let mut z_width = 0.0f32;
        for r in &z_runs {
            if r.text.is_empty() {
                continue;
            }
            let sz = r.size_pt.unwrap_or(hf_params.font_size_pt);
            z_width += TextMeasurer::with_advance_scale(sz, adv).text_width(&r.text);
        }
        let z_x = match align {
            Some(quire_ooxml::Align::Center) => margin_left + (content_w - z_width).max(0.0) / 2.0,
            Some(quire_ooxml::Align::Right) => margin_left + (content_w - z_width).max(0.0),
            _ => margin_left,
        };
        let z_line = quire_layout::Line {
            text,
            width_pt: z_width,
            para_index: line.para_index,
            char_offset: 0,
            col: 0,
        };
        let z_para = quire_ooxml::Paragraph {
            runs: z_runs,
            ..para.clone()
        };
        emit_line_run_items(
            items,
            z_x,
            y,
            hf_params.font_size_pt,
            &z_line,
            Some(&z_para),
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
/// 錨點 → (有效錨點, 條目, 錨點→條目映射, 記號樣式)。
///
/// 編號＝正文引用序（1 起，按首次引用；同一則註被引用多次時編號不變）。
/// 引用指向不存在的註 ⇒ 該錨點整條不進鏈（老實跳過，不虛構條目）。
/// 記號樣式取該 anchor run 的級聯有效值（上標尺度/顏色/族）。
///
/// 註腳與尾註各走一次本函式——兩條支路完全同構（`note_ref` 是單欄位，
/// 同一個 run 不會同時掛兩者 ⇒ 兩條鏈無競態）。
fn note_chain(
    anchors: &[footnotes::Anchor],
    by_id: &mut HashMap<i64, footnotes::NoteEntry>,
    paras_eff: &[quire_ooxml::Paragraph],
    def_size: f32,
    // 註 id → 編號字面（numFmt/numRestart 消費結果，`numbering::assign`
    // 產出）。**空表＝零變化**：十進位 `number` 兜底路徑逐字原樣。
    labels: &HashMap<i64, String>,
) -> (
    Vec<footnotes::Anchor>,
    Vec<footnotes::NoteEntry>,
    Vec<usize>,
    Vec<footnotes::MarkStyle>,
) {
    let mut kept: Vec<footnotes::Anchor> = Vec::new();
    let mut entries: Vec<footnotes::NoteEntry> = Vec::new();
    let mut note_of_anchor: Vec<usize> = Vec::new();
    let mut mark_styles: Vec<footnotes::MarkStyle> = Vec::new();
    let mut order: HashMap<i64, usize> = HashMap::new();
    for a in anchors {
        let Some(e) = by_id.get_mut(&a.note_id) else {
            continue; // 引用指向不存在的註：老實跳過（不虛構條目）。
        };
        if e.number == 0 {
            e.number = order.len() + 1;
        }
        // H2/F3b：有消費字面 ⇒ 蓋 number_label（前綴與記號經
        // footnotes::note_number_text 統一取用）；無 ⇒ 空字串＝十進位原樣。
        if let Some(lab) = labels.get(&a.note_id) {
            e.number_label = lab.clone();
        }
        order.insert(a.note_id, e.number);
        let idx = entries.len();
        entries.push(e.clone());
        note_of_anchor.push(idx);
        // 記號樣式：取該 anchor run 的級聯有效值（上標尺度/顏色/族）。
        let run = paras_eff.get(a.para).and_then(|p| p.runs.get(a.run));
        let color = run.and_then(|r| r.color).unwrap_or(DEFAULT_TEXT_COLOR_ARGB);
        let size = run.and_then(|r| r.size_pt).unwrap_or(def_size);
        let family = run
            .and_then(|r| select_text_family(r.effective_fonts.as_ref(), ""))
            .clone();
        mark_styles.push(footnotes::MarkStyle {
            size_pt: size,
            color_argb: color,
            family,
        });
        kept.push(a.clone());
    }
    (kept, entries, note_of_anchor, mark_styles)
}

/// 顯示列表項的穩定排序：非文字項（填色/框線）同鍵保持原序且恆在前；
/// 文字項依基線 y 再依 x 排列，讓「正文→表格→正文」的閱讀序與 PDF
/// 抽取端一致。
///
/// 正文頁與尾註頁共用本函式（同一份閱讀序口徑）。
fn sort_display_items(items: &mut [DrawItem], txbx_keys: &[(usize, f32, i64, i64, i64)]) {
    // R5（R2）：框內文字項帶群組鍵——主鍵取「宿主段落末行 y」（整數化後與
    // 正文同尺度），同 y 時正文（kind 0）先於框內文（kind 1）；框內文再依
    // （宿主段, 錨定圖序, 框內段序）排，不再被 y-sort 打散。無鍵（單框宿主／
    // 表格切片／頁首尾）＝舊 y,x 口徑，零位移。
    // R6：wps 標註的「錨定圖序」改帶 relativeHeight（LO z 序抽取口徑）。
    if txbx_keys.is_empty() {
        items.sort_by_key(|it| match it {
            DrawItem::Text { x, y, .. } => (
                1i8,
                (*y * 4.0).round() as i64,
                0i8,
                0i64,
                0i64,
                (*x * 4.0).round() as i64,
            ),
            _ => (0i8, 0, 0, 0, 0, 0),
        });
        return;
    }
    let mut keymap: HashMap<usize, (f32, i64, i64, i64)> = HashMap::with_capacity(txbx_keys.len());
    for &(i, ey, host, ord, seq) in txbx_keys {
        keymap.insert(i, (ey, host, ord, seq));
    }
    let key = |i: usize| -> (i8, i64, i8, i64, i64, i64) {
        match &items[i] {
            DrawItem::Text { x, y, .. } => match keymap.get(&i) {
                Some((ey, host, ord, seq)) => (1, (*ey * 4.0).round() as i64, 1, *host, *ord, *seq),
                None => (
                    1,
                    (*y * 4.0).round() as i64,
                    0,
                    0,
                    0,
                    (*x * 4.0).round() as i64,
                ),
            },
            _ => (0, 0, 0, 0, 0, 0),
        }
    };
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&a| key(a));
    let reordered: Vec<DrawItem> = order.iter().map(|&i| items[i].clone()).collect();
    items.clone_from_slice(&reordered);
}

/// R6：wps 標註的排序鍵——LO 抽取序＝`wp:anchor/@relativeHeight`（z 序）。
/// 非標註／回塞模式維持 R5 的框起點序（舊行為逐位不變）。
fn r6_ord(b: &quire_ooxml::InlineImage, buggy: bool) -> i64 {
    if !buggy && b.wps_annotation && b.rel_height != 0 {
        b.rel_height
    } else {
        b.txbx_start as i64
    }
}

/// R5（R2）：把本次 `emit_txbx_texts` 推入的文字項蓋上群組鍵。只在宿主段落
/// 掛 ≥2 個「有實文字」的框時啟用——單框宿主維持舊 y-sort（既有 1.0 檔零
/// 擾動）。`R5_R2_BUGGY=1` 時完全不鍵＝舊 y-sort 回塞。
#[allow(clippy::too_many_arguments)]
fn push_txbx_keys(
    keys: &mut Vec<(usize, f32, i64, i64, i64)>,
    items: &[DrawItem],
    start: usize,
    host: usize,
    ord: i64,
    counts: &HashMap<usize, usize>,
    host_y: &HashMap<usize, f32>,
    fallback_y: f32,
) {
    if std::env::var_os("R5_R2_BUGGY").is_some() {
        return;
    }
    if counts.get(&host).copied().unwrap_or(0) < 2 {
        return;
    }
    let ey = host_y.get(&host).copied().unwrap_or(fallback_y) + 0.001;
    let mut seq = 0i64;
    for (i, it) in items.iter().enumerate().skip(start) {
        if matches!(it, DrawItem::Text { .. }) {
            keys.push((i, ey, host as i64, ord, seq));
            seq += 1;
        }
    }
}

/// 樣式級聯烘焙（t_5dfc8ac7：與正文同一條 `cascade_paragraphs` 路徑）。
fn build_hf_layer(
    doc: &OoxmlDoc,
    sections: &[SectionSpec],
    style_table: Option<&StyleTable>,
    theme: &ThemeColors,
) -> HfLayer {
    // 主題字型表：與正文同一份 fontScheme，供頁首頁尾 part 的 `w:rFonts`
    // theme 引用在解析當下查表落地（ECMA-376 §17.3.2.26）。
    let theme_fonts = ThemeFonts::from_parts(&doc.parts);
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
            if let Ok((paragraphs, tables)) =
                parse_header_footer_part_with_fonts(xml, theme, Some(&theme_fonts))
            {
                // K4a：頁首／頁尾 part 的 `wp:drawing` 幾何（`read_images` 對
                // 任意 XML part 通用；images 空＝零變化）。
                let images = quire_ooxml::read_images(xml);
                // K4c-image：HF 圖的像素解析走**本 part 自家的 .rels**
                // （word/_rels/header1.xml.rels——Bug51170 實測；不是
                // document.xml.rels）。逐位對齊 images；查無／EMF 等不可渲染
                // 格式＝None ⇒ 發射退佔位框。無圖 part＝空向量（零擾動）。
                let hf_rels = doc
                    .parts
                    .get(&rels_path_for(&path))
                    .map(|r| read_rels(r))
                    .unwrap_or_default();
                let pixels: Vec<Option<(String, String)>> = images
                    .iter()
                    .map(|im| resolve_pixel_source(&doc.parts, &hf_rels, &image_rid_candidates(im)))
                    .collect();
                let (styles, metrics, _spacings, _marker_chars) = match style_table {
                    // 頁首尾不展開點引（widths 空＝不虛構；既有行為不變）。
                    // 頁首尾無表格樣式插層，故傳空表。
                    Some(st) => {
                        let calibrated =
                            st.has_fonts() && std::env::var_os("K6_AUTO_PITCH_BUGGY").is_none();
                        cascade_paragraphs(
                            st,
                            &paragraphs,
                            def_size,
                            1.0,
                            &[],
                            &[],
                            PitchCascade {
                                table_styles: &HashMap::new(),
                                calibrated,
                            },
                        )
                    }
                    None => (paragraphs, Vec::new(), Vec::new(), Vec::new()),
                };
                hf_parts.insert(
                    path,
                    HfPart {
                        styles,
                        metrics,
                        tables,
                        images,
                        pixels,
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

    // ---------------- 主題色查表（w:themeColor → theme1.xml clrScheme） ----------------

    /// 端到端：`w:themeColor` 指向 theme1.xml 的 clrScheme 槽位，**優先於**
    /// `w:val` 回退值（ECMA-376 §17.3.2.25——Word/LO 實際渲染口徑）。
    /// fixture 的 hlink＝0x305090：樣式層與 run 直接格式層的有效色都必須是
    /// 0x305090（修復前只吃 w:val＝純藍 0x0000FF，與 Word/LO 偏離）。
    #[test]
    fn theme_color_lookup_end_to_end() {
        const THEME_COLOR: &[u8] = include_bytes!("../../../fixtures/theme_color.docx");
        let engine = Engine::from_docx_bytes(THEME_COLOR).expect("開啟 theme_color.docx 失敗");
        let dl = engine.page_display_list(0).expect("第 0 頁顯示列表不存在");
        let color_of = |text: &str| -> Option<u32> {
            dl.items.iter().find_map(|it| match it {
                DrawItem::Text {
                    text: t,
                    color_argb,
                    ..
                } if t == text => Some(*color_argb),
                _ => None,
            })
        };
        assert_eq!(
            color_of("樣式層主題色"),
            Some(0xFF30_5090),
            "字元樣式層的主題色必須勝過 w:val 回退值（0x0000FF）"
        );
        assert_eq!(
            color_of("直接格式主題色"),
            Some(0xFF30_5090),
            "run 直接格式層的主題色必須勝過 w:val 回退值"
        );
        assert_eq!(
            color_of("查無該名退回回退值"),
            Some(0xFF12_3456),
            "查無該名 → 老實退回 w:val（容錯）"
        );
        assert_eq!(
            color_of("帶著色保留預算色"),
            Some(0xFF36_5F91),
            "帶 themeShade → 保留 Word 預算色（變換未實作，不用未變換色冒充）"
        );
    }

    /// 純讀取鐵律：主題色層是純讀取派生資料——開檔不動存檔必須 byte-identical
    /// （theme1.xml 亦是 untouched 條目，必須原樣回填）。
    #[test]
    fn theme_color_round_trip_untouched() {
        const THEME_COLOR: &[u8] = include_bytes!("../../../fixtures/theme_color.docx");
        let engine = Engine::from_docx_bytes(THEME_COLOR).unwrap();
        let saved = engine.save_docx().unwrap();
        assert_eq!(
            saved.as_slice(),
            THEME_COLOR,
            "含 theme part 的文件存檔必須位元組級相同（passthrough）"
        );
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

    // ---------------- 表格框線軸對齊（tbrl 修復：端點內縮誤掰斜水平/垂直線） ----------------

    /// 字型族烘焙（ECMA-376 §17.3.2.26）：engine 開啟文件時把級聯後的有效
    /// 字型族寫進發射用投影的 `Run::effective_fonts`（仿 `size_pt` 的做法）。
    /// **只存不讀**——本卡不改任何測量／分頁／顯示列表／SVG 輸出（渲染
    /// 逐位元組零變化）。
    #[test]
    fn engine_bakes_effective_fonts_into_projection() {
        const DOC: &[u8] = include_bytes!("../../../fixtures/inline_image.docx");
        // fixture 的 docDefaults 宣告 <w:rFonts w:ascii="Calibri" w:eastAsia="PMingLiU"/>。
        let engine = Engine::from_docx_bytes(DOC).expect("開啟 inline_image.docx 失敗");
        let fonts = engine
            .eff_paragraphs()
            .iter()
            .flat_map(|p| p.runs.iter())
            .find_map(|r| r.effective_fonts.clone())
            .expect("docDefaults 宣告了 w:rFonts → 必須烘焙出有效字型族");
        assert_eq!(fonts.ascii.as_deref(), Some("Calibri"));
        assert_eq!(fonts.east_asia.as_deref(), Some("PMingLiU"));
        // 渲染輸出不受影響（只存不讀）：存檔路徑位元組級不變。
        assert_eq!(engine.save_docx().unwrap().as_slice(), DOC);
    }

    /// Bug54771b（粗框線 sz=36 表格）實證返修。舊實作把框線四端各內縮半個
    /// 線寬：(a) 水平線 y1 下移 y2 上移 → 整條傾斜 ~2°（SVG 實測
    /// y1=74.25≠y2=69.75）；(b) 上下相鄰格的共用邊各往自家格心平移 → 同一條
    /// 內框線分裂成兩條錯位線。修復＝筆畫居中畫在格線，不內縮。
    /// 斷言強度：軸對齊（精確相等）＋共用邊座標完全重合（舊修復「只防斜不防
    /// 分裂」亦會紅）。
    #[test]
    fn table_borders_are_axis_aligned() {
        const B54771B: &[u8] =
            include_bytes!("../../../corpus/poi/test-data_document_Bug54771b.docx");
        let engine = Engine::from_docx_bytes(B54771B).expect("開啟 Bug54771b 失敗");
        let dl = engine.page_display_list(0).expect("第 0 頁存在");
        let mut rules: Vec<(f32, f32, f32, f32)> = dl
            .items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Rule { x1, y1, x2, y2, .. } => Some((*x1, *y1, *x2, *y2)),
                _ => None,
            })
            .collect();
        // 此檔 tblBorders 只宣告 top/bottom/insideH（無 left/right/insideV）
        // → 3 列單格共 6 條水平線，其中 2 條為相鄰列共用邊（各畫兩遍）。
        assert_eq!(rules.len(), 6, "3 列×上下各一 = 6 條框線");
        for (x1, y1, x2, y2) in &rules {
            assert!(
                y1 == y2,
                "水平框線 y 兩端必須精確相等：({x1},{y1})→({x2},{y2})"
            );
            assert!(x2 - x1 > 1.0, "框線不得退化為點：({x1},{y1})→({x2},{y2})");
        }
        // 共用邊＝座標完全重合（居中發射的充分證據；四端內縮或各格自移必分裂）。
        rules.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.total_cmp(&b.2))
                .then(a.3.total_cmp(&b.3))
        });
        let dup = rules.windows(2).filter(|w| w[0] == w[1]).count();
        assert_eq!(dup, 2, "兩條共用邊各自重合（重疊線對）：{rules:?}");
    }

    /// K4b：Bug51170 的段距宣告在 **Normal 樣式**（`w:spacing w:after="200"`
    /// ＝10pt），docDefaults 完全沒有 spacing。K1 gate 若只看 docDefaults
    /// 就會整關帶制（段後距全 0 帶）；本卡分支基底（含 K5）實測為 9 頁
    /// （K4-diag 報告量測時為 5 頁），LO 為 7 頁（render_lab 實測）。
    /// 本測試同時釘住兩件事：(a) 級聯後預設段落鏈的有效段後距＝10pt 且
    /// 量化後帶向量非全零；(b) 分頁收斂到 LO 的 7 頁。
    #[test]
    fn bug51170_normal_style_spacing_lights_bands_and_seven_pages() {
        const BUG51170: &[u8] =
            include_bytes!("../../../corpus/poi/test-data_document_Bug51170.docx");
        let doc = parse(BUG51170).expect("解析 Bug51170 失敗");
        let theme = ThemeColors::from_parts(&doc.parts);
        let theme_fonts = ThemeFonts::from_parts(&doc.parts);
        let st = doc
            .parts
            .get("word/styles.xml")
            .map(|s| StyleTable::parse_with_fonts(s.as_slice(), &theme, Some(&theme_fonts)))
            .unwrap_or_else(|| StyleTable::parse_with_fonts(&[], &theme, Some(&theme_fonts)));
        let (doc_size, doc_line) = st.doc_grid();
        let def_size = doc_size.unwrap_or(12.0);
        let grid_line_h = resolve_line_h_pt(def_size, doc_line);
        // (b) 分頁收斂：LO ground truth 7 頁；gate 關著時為 5 頁。
        let engine = Engine::from_docx_bytes(BUG51170).expect("開啟 Bug51170 失敗");
        assert_eq!(
            engine.page_count(),
            7,
            "Bug51170 段後距生效後應為 LO 的 7 頁"
        );
        // (a) 級聯後預設段落（Normal 鏈）的有效段後距＝200 twips＝10pt。
        let (_, doc_after) = st.doc_spacing();
        assert_eq!(
            doc_after,
            Some(10.0),
            "doc_spacing 必須併入 Normal 樣式鏈的段後距（200 twips）"
        );
        let spacings: Vec<(Option<f32>, Option<f32>)> = doc
            .paragraphs
            .iter()
            .map(|p| {
                let inline = ParaStyle {
                    run: p.prun.clone().unwrap_or_default(),
                    line: p.line,
                    spacing_before_pt: p.spacing_before_pt,
                    spacing_after_pt: p.spacing_after_pt,
                    align: p.jc,
                };
                let (_, eff) = st.effective_para(p.style.as_deref(), inline);
                (eff.spacing_before_pt, eff.spacing_after_pt)
            })
            .collect();
        let (_, after_bands) = spacing_band_vectors(&spacings, grid_line_h, &[]);
        assert!(
            after_bands.iter().any(|&b| b > 0),
            "段後距帶向量不得全零（10pt／帶尺 {grid_line_h}）：{after_bands:?}"
        );
    }

    /// K4a→K4c-image（Bug51170）：頁首 part 的 `wp:inline` 圖先以佔位框投影
    /// （K4a），K4c 起像素可渲染 ⇒ **升級**為真像素 `DrawItem::Image`（同位
    /// 不再發灰底佔位框——不雙發）。header1.xml 恰一張 1854200×990600 EMU
    /// （146×78pt）inline 圖；sectPr 宣告 titlePg 但無 first 頁首 ⇒ 首頁依
    /// 語義空白，其餘頁用 default 頁首——每頁都必須在 header 區（y < 正文
    /// 首行 y）看到 Image。A/B：無頁首圖的檔案不得冒出任何 header 區佔位
    /// 或像素（零變化守門）。
    #[test]
    fn bug51170_header_inline_image_emitted() {
        const BUG51170: &[u8] =
            include_bytes!("../../../corpus/poi/test-data_document_Bug51170.docx");
        let engine = Engine::from_docx_bytes(BUG51170).expect("開啟 Bug51170 失敗");
        assert!(engine.page_count() >= 2, "Bug51170 應有多頁");
        // titlePg＋無 first 頁首：首頁空白，正文頁（1..）用 default 頁首。
        for page in 1..engine.page_count() {
            let top = engine.layout.pages()[page].margin_top_pt;
            let dl = engine.page_display_list(page).expect("頁存在");
            let header_imgs: Vec<(f32, f32, f32, f32)> = dl
                .items
                .iter()
                .filter_map(|it| match it {
                    DrawItem::Image { x, y, w, h, .. } if *y < top => Some((*x, *y, *w, *h)),
                    _ => None,
                })
                .collect();
            assert!(
                !header_imgs.is_empty(),
                "第 {page} 頁 header 區（y < {top}）必須有真像素 Image：{:?}",
                dl.items
                    .iter()
                    .filter_map(|it| match it {
                        DrawItem::Rect { y, .. } => Some(*y),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            );
            let (_, _, w, h) = header_imgs[0];
            assert!(
                (w - 1854200.0 / 12700.0).abs() < 0.1 && (h - 990600.0 / 12700.0).abs() < 0.1,
                "extent 應為 146×78pt：{w}×{h}"
            );
            // 真像素取代佔位框：同頁 header 區不得再發灰底 Rect（K4a 語義
            // 由 Image 承接，A/B 翻綠鎖）。
            assert!(
                !dl.items.iter().any(|it| matches!(
                    it,
                    DrawItem::Rect { y, fill_argb, .. }
                        if *fill_argb == IMAGE_PLACEHOLDER_FILL && *y < top
                )),
                "第 {page} 頁頁首圖已真像素發射，不得雙發佔位框"
            );
        }
        // A/B：無頁首圖的檔案——header 區不得冒出佔位或像素（既有顯示列表零變化）。
        const NO_HF_IMG: &[u8] =
            include_bytes!("../../../corpus/xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx");
        let ab = Engine::from_docx_bytes(NO_HF_IMG).expect("開啟 WC049 失敗");
        for page in 0..ab.page_count() {
            let top = ab.layout.pages()[page].margin_top_pt;
            let dl = ab.page_display_list(page).expect("頁存在");
            assert!(
                !dl.items.iter().any(|it| matches!(
                    it,
                    DrawItem::Rect { y, fill_argb, .. }
                        if *fill_argb == IMAGE_PLACEHOLDER_FILL && *y < top
                )),
                "無頁首圖的第 {page} 頁不得發射 header 佔位"
            );
            assert!(
                !dl.items.iter().any(|it| matches!(
                    it,
                    DrawItem::Image { y, .. } if *y < top
                )),
                "無頁首圖的第 {page} 頁不得發射 header 像素"
            );
        }
    }

    // ---------------- t_a1397522：圖片幾何佔位框（end-to-end 投影） ----------------

    const INLINE_IMG: &[u8] = include_bytes!("../../../fixtures/inline_image.docx");

    /// selfgen fixture：文字＋100pt inline 圖＋文字——中間段恰佔
    /// ceil(100/16.8)＝6 帶。K4c-image 起像素（fixture 帶真 67B PNG）可解析
    /// ⇒ 發射升級為真像素 `DrawItem::Image`（灰 Rect＋四 Rule 退位）；
    /// 幾何（extent／帶 y／版心左緣）逐位沿用 K4a 口徑。
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
        assert_eq!(rects.len(), 0, "K4c：真像素取代灰底佔位 Rect");
        let rules = dl
            .items
            .iter()
            .filter(|it| matches!(it, DrawItem::Rule { color_argb, .. } if *color_argb == IMAGE_PLACEHOLDER_EDGE))
            .count();
        assert_eq!(rules, 0, "K4c：佔位四邊 Rule 同退位");
        let imgs: Vec<&DrawItem> = dl
            .items
            .iter()
            .filter(|it| matches!(it, DrawItem::Image { .. }))
            .collect();
        assert_eq!(imgs.len(), 1, "恰好一張真像素 Image");
        // Image 幾何＝extent：200×100pt，x＝版心左緣（A4 1 吋邊界＝72pt）。
        if let DrawItem::Image {
            x,
            y,
            w,
            h,
            mime,
            data_b64,
            ..
        } = imgs[0]
        {
            assert!(
                (w - 200.0).abs() < 1e-2 && (h - 100.0).abs() < 1e-2,
                "{w}x{h}"
            );
            assert!((x - 72.0).abs() < 1e-2, "版心左緣：{x}");
            // K6：前段 CJK 文字（PMingLiU→WQY）pitch＝11×1.3496（族表值）；
            // 圖片錨在次行起點，故 y＝72＋該 pitch（舊粗尺 15.4 的「空行段
            // 整帶數」算法已作廢——pitch 由 real-LH 決定）。
            assert!(
                (y - (72.0 + 11.0 * 1.3496)).abs() < 0.6,
                "圖 y＝前段 CJK 行 pitch 之後（72＋11×1.3496）：{y}"
            );
            assert_eq!(mime, "image/png", "副檔名 png → image/png");
            assert!(data_b64.starts_with("iVBORw0KGgo"), "PNG magic 直通");
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
        // K4c-image：anchor 圖的 rId 不變（像素可解析）⇒ 鎖點發真像素
        // Image，佔位 Rect 退位；鎖點幾何口徑沿用 K4a。
        let rects = dl
            .items
            .iter()
            .filter(|it| matches!(it, DrawItem::Rect { fill_argb, .. } if *fill_argb == IMAGE_PLACEHOLDER_FILL))
            .count();
        assert_eq!(rects, 0, "K4c：像素可解析 ⇒ 鎖點不再發佔位 Rect");
        let img = dl
            .items
            .iter()
            .find(|it| matches!(it, DrawItem::Image { .. }))
            .expect("鎖點真像素 Image");
        if let DrawItem::Image { x, y, .. } = img {
            assert!(
                (x - 72.0).abs() < 1e-2 && (y - 144.0).abs() < 1e-2,
                "頁鎖點 1in/2in：{x},{y}"
            );
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
    ///
    /// 註（F3／J1R 尾註）：本檔正文確有一則尾註（`w:endnoteReference id=2`），
    /// J1R 起尾註**接排正文末頁**、不另計頁 ⇒ `page_count()` 與 LO 同為 3 頁
    /// （正文分頁斷點仍與無尾註時逐位元組相同）。
    #[test]
    fn br_page_corpus_reader_styles_reaches_three_pages() {
        const RS: &[u8] = include_bytes!(
            "../../../corpus/phpword/tests_PhpWordTests__files_documents_reader-styles.docx"
        );
        let engine = Engine::from_docx_bytes(RS).expect("開啟 reader-styles.docx");
        assert_eq!(
            engine.body_page_count(),
            3,
            "br-page 後正文應與 LO 同為 3 頁"
        );
        assert_eq!(
            engine.endnote_page_count(),
            0,
            "J1R：本檔一則尾註接排正文末頁（LO 三頁）"
        );
        assert_eq!(
            engine.page_count(),
            3,
            "page_count ＝ 正文 3（尾註不另計頁）"
        );
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
        // 不佔行帶：第 0 頁必須同時含兩張圖片佔位 Rect（以專用填色辨識，
        // 避免把表格背景 Rect 誤算成圖片）。
        let rects = engine
            .page_display_list(0)
            .expect("第 0 頁存在")
            .items
            .into_iter()
            .filter(|it| {
                matches!(
                    it,
                    DrawItem::Rect { fill_argb, .. } if *fill_argb == IMAGE_PLACEHOLDER_FILL
                )
            })
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
            (gap - 33.6).abs() < 0.5,
            "24pt 段行高 33.6pt：M1-band 細化後＝12 帶×2.8pt（舊 2×16.8，物理高度不變），實得 gap={gap} head_y={head_y} next_y={next_y}"
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

    // ---------------- 字型族選取管線（G1） ----------------

    /// 顯示列表內某段文字的選族結果。
    fn family_of(dl: &PageDisplayList, text: &str) -> String {
        dl.items
            .iter()
            .find_map(|it| match it {
                DrawItem::Text {
                    text: t, family, ..
                } if t == text => family.clone(),
                _ => None,
            })
            .expect("測試文字必須存在於顯示列表")
    }

    /// (a) 直接宣告 `w:rFonts w:ascii` 的 run → `family=Some(ascii)`。
    #[test]
    fn display_list_selects_ascii_font_family() {
        // inline_image.docx 的 docDefaults 宣告 w:ascii="Calibri"。
        const DOC: &[u8] = include_bytes!("../../../fixtures/inline_image.docx");
        let engine = Engine::from_docx_bytes(DOC).expect("開啟 inline_image.docx 失敗");
        let dl = engine.page_display_list(0).unwrap();
        assert_eq!(
            family_of(&dl, "圖片前段：占位框登場之前的文字。"),
            "Calibri".to_string(),
            "未帶 hint 且有 ascii 宣告時應選 ascii 字型族"
        );
    }

    /// (b) `w:hint="eastAsia"`＋`w:eastAsia` → 選 east_asia（即使 ascii 也有值）。
    #[test]
    fn display_list_prefers_east_asia_when_hinted() {
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:rPr>
          <w:rFonts w:ascii="Arial" w:eastAsia="標楷體" w:hint="eastAsia"/>
        </w:rPr>
        <w:t>東亞提示選族</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
        base.set_part("word/document.xml", xml.as_bytes().to_vec());
        let docx = base.save_docx().expect("存回 docx 失敗");
        let engine = Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗");
        let dl = engine.page_display_list(0).unwrap();
        assert_eq!(
            family_of(&dl, "東亞提示選族"),
            "標楷體".to_string(),
            "hint=eastAsia 且 eastAsia 有宣告時必須優先選 east_asia"
        );
    }

    /// (b2) 無 hint 但只有 east_asia 宣告，且文字全為 CJK → 選 east_asia。
    #[test]
    fn display_list_falls_back_to_east_asia_for_all_cjk_text() {
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:rPr>
          <w:rFonts w:eastAsia="新細明體"/>
        </w:rPr>
        <w:t>純中文段落</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
        base.set_part("word/document.xml", xml.as_bytes().to_vec());
        let docx = base.save_docx().expect("存回 docx 失敗");
        let engine = Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗");
        let dl = engine.page_display_list(0).unwrap();
        assert_eq!(
            family_of(&dl, "純中文段落"),
            "新細明體".to_string(),
            "無 ascii 宣告且文字全 CJK 時應退回 east_asia"
        );
    }

    /// (c) 無任何 `w:rFonts` → 顯示列表不帶 family 欄位（serde 跳過）。
    #[test]
    fn display_list_omits_family_without_rfonts() {
        let engine = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        let dl = engine.page_display_list(0).unwrap();
        assert!(
            dl.items.iter().all(|it| !matches!(
                it,
                DrawItem::Text {
                    family: Some(_),
                    ..
                }
            )),
            "sample.docx 無字型宣告，所有文字項的 family 必須是 None"
        );
        let json = engine.page_display_list_json(0).unwrap();
        assert!(
            !json.contains("\"family\""),
            "family=None 時 serde 必須跳過欄位（舊顯示列表格式）：{json}"
        );
    }

    /// 單段長文的頁內行距（相鄰文字行 y 差；同段內無段距干擾）。
    fn first_page_line_pitch(engine: &Engine) -> f32 {
        let dl = engine.page_display_list(0).expect("第 0 頁");
        let mut ys: Vec<f32> = dl
            .items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Text { y, text, .. } if !text.trim().is_empty() => Some(*y),
                _ => None,
            })
            .collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.05);
        ys.windows(2)
            .map(|w| w[1] - w[0])
            .find(|d| *d > 1.0)
            .expect("至少兩行文字才有 pitch")
    }

    /// (f) K6：字型宣告決定 real-LH pitch（不再是版面中性）。
    /// 無字型宣告＝零宣告粗尺 12pt×1.4＝16.8；宣告 Calibri＝12×1.2207＝14.65。
    /// 舊測試「宣告字型不得動分頁」是 K6 前的假設，已被校準報告 §1 取代。
    #[test]
    fn font_declaration_sets_real_lh_pitch() {
        let long_text = "字型宣告不得影響分頁。".repeat(400);
        let doc = |rpr: &str| {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r>{rpr}<w:t>{long_text}</w:t></w:r></w:p>
  </w:body>
</w:document>"#
            )
        };
        let build = |xml: String| {
            let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
            base.set_part("word/document.xml", xml.into_bytes());
            let docx = base.save_docx().expect("存回 docx 失敗");
            Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗")
        };
        let plain = build(doc(""));
        let with_font = build(doc(r#"<w:rPr><w:rFonts w:ascii="Calibri"/></w:rPr>"#));
        let plain_pitch = first_page_line_pitch(&plain);
        let font_pitch = first_page_line_pitch(&with_font);
        assert!(
            (plain_pitch - 16.8).abs() < 0.3,
            "無字型宣告＝零宣告粗尺 16.8，實得 {plain_pitch}"
        );
        assert!(
            (font_pitch - 14.65).abs() < 0.3,
            "Calibri 12pt real-LH＝12×1.2207＝14.65，實得 {font_pitch}"
        );
    }

    // ---------------- theme 字型引用（G2） ----------------

    /// 跨頁找某段文字的選族結果（Heading／正文可能落在不同頁）。
    fn family_on_pages(engine: &Engine, text: &str) -> String {
        for p in 0..engine.page_count() {
            let Some(dl) = engine.page_display_list(p) else {
                continue;
            };
            let Some(item) = dl
                .items
                .iter()
                .find(|it| matches!(it, DrawItem::Text { text: t, .. } if t == text))
            else {
                continue;
            };
            let DrawItem::Text { family, .. } = item else {
                unreachable!("已確認是 Text 項");
            };
            return family
                .clone()
                .unwrap_or_else(|| panic!("測試文字 {text:?} 的 family 不得為 None"));
        }
        panic!("測試文字 {text:?} 必須存在於任一頁顯示列表");
    }

    /// 最小 fontScheme：major latin=Calibri Light／minor latin=Calibri，
    /// 兩個 `a:ea` 皆空字串（對照 HC031 的實檔形狀）。
    const THEME_FONTS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <a:themeElements>
    <a:fontScheme name="測試主題">
      <a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/></a:majorFont>
      <a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/></a:minorFont>
    </a:fontScheme>
  </a:themeElements>
</a:theme>"#;

    /// 以 sample.docx 為容器，換入單段文件＋測試 theme part。
    fn build_theme_doc(rpr: &str, text: &str) -> Engine {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r>{rpr}<w:t>{text}</w:t></w:r></w:p>
  </w:body>
</w:document>"#
        );
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        base.set_part("word/document.xml", xml.into_bytes());
        base.set_part("word/theme/theme1.xml", THEME_FONTS_XML.as_bytes().to_vec());
        let docx = base.save_docx().expect("存回 docx 失敗");
        Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗")
    }

    /// 同 [`build_theme_doc`]，另換入指定的 `styles.xml`（跨層級聯測試用）。
    fn build_theme_doc_with_styles(rpr: &str, text: &str, styles_xml: &str) -> Engine {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r>{rpr}<w:t>{text}</w:t></w:r></w:p>
  </w:body>
</w:document>"#
        );
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        base.set_part("word/document.xml", xml.into_bytes());
        base.set_part("word/styles.xml", styles_xml.as_bytes().to_vec());
        base.set_part("word/theme/theme1.xml", THEME_FONTS_XML.as_bytes().to_vec());
        let docx = base.save_docx().expect("存回 docx 失敗");
        Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗")
    }

    /// (a) HC031 實檔直接值守土（G2b 回歸）：run 層 rPr 宣告的直接字型族
    /// 必須原樣抵達顯示列表，不得被 docDefaults 的 theme 引用跨層覆寫。
    ///
    /// 本檔 docDefaults 宣告 `asciiTheme="minorHAnsi"`（→Calibri），但這三處
    /// 的 run 直接 rPr 各自宣告 Wide Latin／Consolas／Cambria Math——跨層
    /// 是普通逐欄級聯，下層直接值必須贏上層的 theme 引用。
    #[test]
    fn hc031_direct_run_fonts_survive_theme_resolution() {
        const HC031: &[u8] =
            include_bytes!("../../../corpus/xopt/TestFiles_HC031-Complicated-Document.docx");
        let engine = Engine::from_docx_bytes(HC031).expect("開啟 HC031 失敗");
        assert_eq!(
            family_on_pages(
                &engine,
                "Video provides a powerful way to help you prove your point."
            ),
            "Wide Latin".to_string(),
            "run 直接 rPr 的 Wide Latin 不得被 docDefaults 的 minorHAnsi 覆寫"
        );
        assert_eq!(
            family_on_pages(
                &engine,
                "You can also type a keyword to search online for the video that best fits your document."
            ),
            "Consolas".to_string(),
            "run 直接 rPr 的 Consolas 不得被 docDefaults 的 minorHAnsi 覆寫"
        );
        assert_eq!(
            family_on_pages(&engine, "x+an=k=0nnkxkan-k"),
            "Cambria Math".to_string(),
            "數學 run 直接 rPr 的 Cambria Math 不得被 docDefaults 的 minorHAnsi 覆寫"
        );
    }

    /// (c) 跨層直接值取勝（G2b 新語義鎖）：docDefaults `asciiTheme="majorHAnsi"`
    /// （→Calibri Light），run 直接 `w:ascii="Arial"` → **Arial**。
    /// theme 引用的取勝只適用於同一 `w:rFonts` 元素內（ECMA-376 §17.3.2.26），
    /// 跨層就是普通逐欄級聯。
    #[test]
    fn cross_layer_direct_font_wins_over_inherited_theme_ref() {
        const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr>
    <w:rFonts w:asciiTheme="majorHAnsi"/>
  </w:rPr></w:rPrDefault></w:docDefaults>
</w:styles>"#;
        let engine = build_theme_doc_with_styles(
            r#"<w:rPr><w:rFonts w:ascii="Arial"/></w:rPr>"#,
            "跨層直接值",
            STYLES,
        );
        let dl = engine.page_display_list(0).expect("第 0 頁存在");
        assert_eq!(
            family_of(&dl, "跨層直接值"),
            "Arial".to_string(),
            "下層直接值必須贏上層 docDefaults 的 theme 引用（不得變成 Calibri Light）"
        );
    }

    /// (d) theme 引用仍生效：HC031 無直接 rPr 的本文文字（Heading2 樣式鏈）
    /// 繼承 docDefaults 的 `minorHAnsi` → Calibri；text box run 的直接 rPr
    /// `majorHAnsi` → Calibri Light（顯示列表確有落地）。
    ///
    /// 註：此 corpus 檔的 Heading 樣式鏈（Heading1/Heading2→Normal→
    /// docDefaults）宣告的是 `minorHAnsi`，不是 `majorHAnsi`；實檔的 major
    /// 引用落在 text box run 上。測試照實檔宣告驗證，不臆造 Heading 為 major。
    #[test]
    fn hc031_theme_font_refs_resolve_from_real_file() {
        const HC031: &[u8] =
            include_bytes!("../../../corpus/xopt/TestFiles_HC031-Complicated-Document.docx");
        let engine = Engine::from_docx_bytes(HC031).expect("開啟 HC031 失敗");
        // 本文 Heading2 段落繼承 docDefaults minorHAnsi → theme minor latin。
        assert_eq!(
            family_on_pages(&engine, "This is Heading 2"),
            "Calibri".to_string(),
            "minorHAnsi 必須查 theme 的 minor latin（Calibri）"
        );
        // 顯示列表確實把 major 引用落地（text box 的可見項目）。
        let has_light = (0..engine.page_count()).any(|p| {
            engine.page_display_list(p).is_some_and(|dl| {
                dl.items.iter().any(|it| {
                        matches!(it, DrawItem::Text { family: Some(f), .. } if f == "Calibri Light")
                    })
            })
        });
        assert!(has_light, "majorHAnsi 必須在顯示列表落地為 Calibri Light");
    }

    /// (b) 同層 `w:ascii="Arial"`＋`w:asciiTheme="minorHAnsi"` → theme 取勝
    /// （ECMA-376 §17.3.2.26 明文），選 theme 的 minor latin＝Calibri。
    #[test]
    fn theme_font_ref_wins_over_direct_value_same_layer() {
        let engine = build_theme_doc(
            r#"<w:rPr><w:rFonts w:ascii="Arial" w:asciiTheme="minorHAnsi"/></w:rPr>"#,
            "theme 取勝",
        );
        let dl = engine.page_display_list(0).expect("第 0 頁存在");
        assert_eq!(
            family_of(&dl, "theme 取勝"),
            "Calibri".to_string(),
            "theme 引用與直接值同層共存時必須以 theme 為準"
        );
    }

    /// (c) theme 槽的 `@typeface` 空字串 → 視同查不到，不覆寫直接值
    /// （theme 的 `a:ea` 在測試 theme 中即空字串）。
    #[test]
    fn empty_theme_typeface_falls_back_to_direct_value() {
        let engine = build_theme_doc(
            r#"<w:rPr><w:rFonts w:eastAsia="微軟正黑體" w:eastAsiaTheme="minorEastAsia"/></w:rPr>"#,
            "東亞直接值",
        );
        let dl = engine.page_display_list(0).expect("第 0 頁存在");
        assert_eq!(
            family_of(&dl, "東亞直接值"),
            "微軟正黑體".to_string(),
            "空的 theme typeface 不得抹掉直接宣告的 eastAsia"
        );
    }

    /// (f) K6：theme 字型引用決定 real-LH pitch（不再是版面中性）。
    /// `w:asciiTheme="minorHAnsi"` 解到 Calibri ⇒ 12×1.2207＝14.65；
    /// 無宣告＝零宣告粗尺 16.8。舊「theme 引用不得動分頁」假設已作廢。
    #[test]
    fn theme_font_reference_sets_real_lh_pitch() {
        let long_text = "字型主題引用不得影響分頁。".repeat(400);
        let doc = |rpr: &str| {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r>{rpr}<w:t>{long_text}</w:t></w:r></w:p>
  </w:body>
</w:document>"#
            )
        };
        let build = |xml: String| {
            let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
            base.set_part("word/document.xml", xml.into_bytes());
            base.set_part("word/theme/theme1.xml", THEME_FONTS_XML.as_bytes().to_vec());
            let docx = base.save_docx().expect("存回 docx 失敗");
            Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗")
        };
        let plain = build(doc(""));
        let themed = build(doc(
            r#"<w:rPr><w:rFonts w:asciiTheme="minorHAnsi"/></w:rPr>"#,
        ));
        let plain_pitch = first_page_line_pitch(&plain);
        let themed_pitch = first_page_line_pitch(&themed);
        assert!(
            (plain_pitch - 16.8).abs() < 0.3,
            "無 theme 引用＝零宣告粗尺 16.8，實得 {plain_pitch}"
        );
        assert!(
            (themed_pitch - 14.65).abs() < 0.3,
            "minorHAnsi→Calibri real-LH＝14.65，實得 {themed_pitch}"
        );
    }

    // ---------------- 頁底註腳（F2b-2）----------------

    const FOOT_DOC: &[u8] = include_bytes!("../../../corpus/poi/test-data_document_footnotes.docx");
    const TABLE_FOOT_DOC: &[u8] =
        include_bytes!("../../../corpus/poi/test-data_document_table_footnotes.docx");

    /// 零變化守門（構造性）：無註腳文件 ⇒ 排布空、預留表空、存檔 byte-identical。
    /// 本測在**所有**檔案上都該綠——實跑 corpus 代表檔確認快路徑真的零擾動。
    #[test]
    fn doc_without_notes_has_empty_foot_layer() {
        let engine = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        assert!(engine.foot.is_empty(), "無註腳檔案的註腳層必須全空");
        assert!(engine.foot.reserves.iter().all(|v| *v == 0.0));
        assert_eq!(engine.footnote_rounds(), 0, "無註腳不計收斂輪數");
        // 預留表空 ⇒ 分頁參數與現況同（零擾動的另一半：params 未被灌值）。
        assert!(engine.params.footnote_reserves_pt.is_empty());
    }

    /// corpus 實檔：註腳發射三件套——錨點記號（上標數字）、分隔線（Rule）、
    /// 條目文字（編號＋註腳內文）；幾何自洽（條目落在分隔線之下、版心底之上）。
    #[test]
    fn corpus_footnote_doc_emits_mark_separator_and_item() {
        let engine = Engine::from_docx_bytes(FOOT_DOC).expect("開啟 footnotes.docx 失敗");
        let dl = engine.page_display_list(0).expect("第 0 頁");
        let mark = dl.items.iter().find_map(|it| match it {
            DrawItem::Text { text, size_pt, .. } if text == "1" => Some(*size_pt),
            _ => None,
        });
        assert!(mark.is_some(), "正文錨點記號「1」必須發射");
        assert!(mark.unwrap() < 12.0, "記號必須上標化（字級小於正文）");
        let sep = dl.items.iter().find_map(|it| match it {
            DrawItem::Rule { x1, y1, x2, .. } => Some((*x1, *y1, *x2)),
            _ => None,
        });
        let (x1, sy, x2) = sep.expect("頁底必須發射分隔線");
        // 本檔正文行 x 起點＝85.05pt（版心左緣）；分隔線必須與之對齊。
        assert!(
            (x1 - 85.05).abs() < 0.6,
            "分隔線必須自版心左緣起，實得 {x1}"
        );
        assert!(x2 - x1 > 10.0, "分隔線寬為版心 1/3，不得為零");
        // 條目：編號「1.」與內文「snoska」皆為發射文字。
        let texts: Vec<&str> = dl
            .items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"1."), "條目編號前綴必須發射");
        assert!(texts.contains(&"snoska"), "註腳內文必須發射");
        // 幾何自洽：條目行頂 > 分隔線 y（條目在分隔線之下）。
        let item_y = dl.items.iter().find_map(|it| match it {
            DrawItem::Text { text, y, .. } if text == "snoska" => Some(*y),
            _ => None,
        });
        assert!(item_y.unwrap() > sy, "註腳條目必須排在分隔線之下");
        assert!(sy < dl.height_pt, "分隔線不得掉出頁外");
        assert!(engine.footnote_rounds() >= 1, "有註腳必記錄收斂輪數");
    }

    /// 預留真的擠壓正文：同一份文件「灌預留 vs 不灌」頁數必增（端到端走
    /// engine 的收斂迴圈 ⇒ `paginate` 實際扣了帶——F2a/F2b-1 機制被消費的證據）。
    #[test]
    fn footnote_reserve_actually_shrinks_text_area_via_engine() {
        // bug65649：244 頁長文＋6 處註腳引用（多頁大檔，逐頁預留影響面可測）。
        const BIG: &[u8] = include_bytes!("../../../corpus/poi/test-data_document_bug65649.docx");
        let engine = Engine::from_docx_bytes(BIG).expect("開啟 bug65649.docx 失敗");
        let with = engine.page_count();
        assert!(with > 10, "本檔必須是多頁長文才有對照意義");
        assert!(
            engine.foot.pages.iter().any(|p| p.reserve_pt > 0.0),
            "有註腳的頁必須有非零保留帶"
        );
        // 同檔以「清空預留」重建：頁數不得變少（預留只擠不鬆）。
        let mut lean = Engine::from_docx_bytes(BIG).expect("重開失敗");
        lean.params.footnote_reserves_pt = Vec::new();
        let mut p = lean.params.clone();
        p.footnote_reserves_pt = Vec::new();
        lean.layout.update_params(p);
        let without = lean.page_count();
        assert!(
            with >= without,
            "灌預留後頁數 {with} 不得少於不灌 {without}"
        );
        // K1：段前後距進分頁後，本檔「有預留 vs 無預留」的**總頁數**可能同值
        // （244＝244；間距本身已把內容推過頁界），故硬證據改看逐頁：至少一個
        // 有預留的頁，其行數必須真的少於同頁無預留版本（容量被扣的事實）。
        let shrunk = engine
            .foot
            .pages
            .iter()
            .enumerate()
            .filter(|(_, fp)| fp.reserve_pt > 0.0)
            .any(|(i, _)| {
                engine
                    .layout
                    .pages()
                    .get(i)
                    .zip(lean.layout.pages().get(i))
                    .is_some_and(|(w, o)| w.lines.len() < o.lines.len())
            });
        assert!(
            shrunk,
            "有預留頁至少一頁的行數必須少於無預留同頁（機制被消費的硬證據）：with={with} without={without}"
        );
    }

    /// 表格內錨點（corpus 實檔）：本體落末頁不遺失、記號不虛構（誠實劃界）。
    #[test]
    fn table_anchor_note_falls_back_without_fabricated_mark() {
        let engine =
            Engine::from_docx_bytes(TABLE_FOOT_DOC).expect("開啟 table_footnotes.docx 失敗");
        let total_items: usize = engine.foot.pages.iter().map(|p| p.items.len()).sum();
        let total_marks: usize = engine.foot.marks.iter().map(|m| m.len()).sum();
        assert_eq!(total_items, 1, "表格內錨點的註腳本體必須照發（落末頁）");
        assert_eq!(total_marks, 0, "表格內無流行程：記號不得虛構");
    }

    /// 存檔零影響（round-trip 鐵律）：註腳層是純讀取投影——含註腳的實檔
    /// 未觸碰存檔必須 byte-identical。
    #[test]
    fn footnote_layer_does_not_touch_save_path() {
        let engine = Engine::from_docx_bytes(FOOT_DOC).expect("開啟 footnotes.docx 失敗");
        let diff = engine.roundtrip_check().expect("round-trip 對照失敗");
        assert!(
            diff.byte_identical,
            "註腳發射層不得影響存檔位元組：{diff:?}"
        );
    }

    // ---------------- 尾註（F3）----------------

    const HC031_ENDNOTE_DOC: &[u8] =
        include_bytes!("../../../corpus/xopt/TestFiles_HC031-Complicated-Document.docx");
    const TR02_ENDNOTE_DOC: &[u8] =
        include_bytes!("../../../corpus/xopt/OpenXmlPowerToolsExamples_TextReplacer02_Test01.docx");
    const RTL_ENDNOTE_DOC: &[u8] = include_bytes!("../../../corpus/selfgen/arabic_rtl_notes.docx");
    const POI_ENDNOTE_DOC: &[u8] =
        include_bytes!("../../../corpus/poi/test-data_document_endnotes.docx");

    /// 自製尾註文件：以 sample.docx 為容器，換入帶 N 個尾註引用的正文
    /// ＋對應的 `word/endnotes.xml`（仿既有 `build_theme_doc` 手法）。
    ///
    /// `n == 0` ⇒ 正文不含引用、part 為空殼（＝無尾註檔，供 A/B 對照用）。
    /// 註 part 走**慣例路徑**解析（F1 資料層不查 rels），故不動
    /// `[Content_Types].xml`／rels——本測只驗版面與發射，不驗 OPC 完整性。
    fn build_endnote_doc(n: usize, body: &str) -> Engine {
        let mut refs = String::new();
        for i in 1..=n {
            refs.push_str(&format!(r#"<w:r><w:endnoteReference w:id="{i}"/></w:r>"#));
        }
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>正文開頭</w:t></w:r>{refs}</w:p>
  </w:body>
</w:document>"#
        );
        let mut notes = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
        );
        for i in 1..=n {
            notes.push_str(&format!(
                r#"<w:endnote w:id="{i}"><w:p><w:r><w:t xml:space="preserve">{body}</w:t></w:r></w:p></w:endnote>"#
            ));
        }
        notes.push_str("</w:endnotes>");
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        base.set_part("word/document.xml", xml.into_bytes());
        base.set_part("word/endnotes.xml", notes.into_bytes());
        let docx = base.save_docx().expect("存回 docx 失敗");
        Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗")
    }

    /// 自製**兩節**尾註文件（F3b）：可注入 `word/settings.xml` 的 `endnotePr`
    /// （`settings=None`＝不注入＝無尾註設定），第一／第二節各帶 `n_first`／
    /// `n_rest` 條尾註引用（id 連續）。`sectPr` 用 A4 標準幾何。
    fn build_endnote_settings_doc(
        n_first: usize,
        n_rest: usize,
        body: &str,
        settings: Option<&str>,
    ) -> Engine {
        let mut refs_first = String::new();
        for i in 1..=n_first {
            refs_first.push_str(&format!(r#"<w:r><w:endnoteReference w:id="{i}"/></w:r>"#));
        }
        let mut refs_rest = String::new();
        for i in (n_first + 1)..=(n_first + n_rest) {
            refs_rest.push_str(&format!(r#"<w:r><w:endnoteReference w:id="{i}"/></w:r>"#));
        }
        // 第一段內的 sectPr 收束第一節；body 級 sectPr 收尾第二節。
        let sect = r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:left="1134" w:right="1134" w:bottom="1134"/></w:sectPr>"#;
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>第一節</w:t></w:r>{refs_first}{sect}</w:p>
    <w:p><w:r><w:t>第二節</w:t></w:r>{refs_rest}</w:p>
    {sect}
  </w:body>
</w:document>"#
        );
        let mut notes = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
        );
        for i in 1..=(n_first + n_rest) {
            notes.push_str(&format!(
                r#"<w:endnote w:id="{i}"><w:p><w:r><w:t xml:space="preserve">{body}</w:t></w:r></w:p></w:endnote>"#
            ));
        }
        notes.push_str("</w:endnotes>");
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        base.set_part("word/document.xml", xml.into_bytes());
        base.set_part("word/endnotes.xml", notes.into_bytes());
        if let Some(s) = settings {
            base.set_part("word/settings.xml", s.as_bytes().to_vec());
        }
        let docx = base.save_docx().expect("存回 docx 失敗");
        Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗")
    }

    /// 尾註條目編號前綴（`layout_entry` 的 superscript run，如 `i.`／`1.`）。
    ///
    /// J1R：接排正文末頁的條目（`inline`）在前，純尾註續頁依頁序在後——
    /// 即尾註的閱讀序（＝編號序）。
    fn endnote_prefixes(engine: &Engine) -> Vec<String> {
        let end = engine.endnote_layout();
        let mut out = Vec::new();
        for item in end.inline.iter().chain(end.pages.iter().flatten()) {
            for line in &item.lines {
                for run in &line.runs {
                    if run.superscript {
                        out.push(run.text.clone());
                    }
                }
            }
        }
        out
    }

    /// 全文渲染 JSON（逐頁顯示列表序列化；供逐位元組 A/B 比較）。
    fn render_json(engine: &Engine) -> String {
        let mut all = Vec::new();
        for i in 0..engine.page_count() {
            if let Some(dl) = engine.page_display_list(i) {
                all.push(serde_json::to_string(&dl).expect("序列化顯示列表"));
            }
        }
        all.join("\n")
    }
    fn endnote_capacity_bands(engine: &Engine) -> (usize, usize) {
        let geo = engine.end_geo().expect("末節幾何");
        let band = engine.params.line_h_pt();
        // 全容量（無頁底預留）——尾註不擠正文，容量與正文預留無關。
        let mut full = engine.params.clone();
        full.footnote_reserve_pt = 0.0;
        full.footnote_reserves_pt = Vec::new();
        full.endnote_reserves_pt = Vec::new();
        let cap = section_capacity(&full, &geo, 0);
        let sep = footnotes::endnote_sep_bands(band) as usize;
        (cap.saturating_sub(sep).max(1), cap.max(1))
    }

    /// J1R：正文末頁接排容量（帶）——與 `compute_endnotes` 同一算式。
    fn endnote_inline_capacity_bands(engine: &Engine) -> usize {
        let geo = engine.end_geo().expect("末節幾何");
        let band = engine.params.line_h_pt();
        let mut full = engine.params.clone();
        full.footnote_reserve_pt = 0.0;
        full.footnote_reserves_pt = Vec::new();
        full.endnote_reserves_pt = Vec::new();
        let last = engine.body_page_count() - 1;
        let page = engine.layout.pages().get(last).expect("正文末頁");
        let cap = section_capacity(&full, &geo, page.index);
        let used = engine.last_body_used_bands(page, &geo, band);
        let foot = engine
            .foot
            .pages
            .get(last)
            .map(|p| p.reserve_bands as usize)
            .unwrap_or(0);
        let sep = footnotes::endnote_sep_bands(band) as usize;
        cap.saturating_sub(used)
            .saturating_sub(foot)
            .saturating_sub(sep)
    }

    /// 取顯示列表裡的全部文字（串接；空字串不計）。
    fn display_texts(dl: &PageDisplayList) -> Vec<String> {
        dl.items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Text { text, .. } if !text.is_empty() => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// (a) HC031 實檔（J1R 改寫）：尾註**接排正文末頁**——含分隔線 Rule 與
    /// 尾註文字「This is an endnote」（前導空白照級聯拍平結果斷言，不硬扣）。
    /// 意圖保留：尾註內容與分隔線仍必須出現在文件最後一頁；只是末頁不再是
    /// 「獨立追加頁」，而是正文末頁本身。
    #[test]
    fn hc031_last_body_page_has_endnote_separator_and_text() {
        let engine = Engine::from_docx_bytes(HC031_ENDNOTE_DOC).expect("開啟 HC031 失敗");
        assert_eq!(
            engine.endnote_page_count(),
            0,
            "一則尾註接排正文末頁，不另開頁"
        );
        let body = engine.body_page_count();
        let dl = engine
            .page_display_list(body - 1)
            .expect("正文末頁必須存在");
        assert_eq!(dl.page, body - 1, "尾註接排在正文末頁");
        // 分隔線：版心左緣起、寬＝版心×1/3；y 在正文內容之後（不再是頁頂）。
        // K7-cal（t_919b8d54）：水平尺落地後 K6 pitch 不變但每頁少塞字，正文
        // 末頁頂部改由表格開頭（表格框線也是橫線且同寬）——选择器不得取
        // 「第一條 Rule」，改取**最底**那條左緣起橫線（＝接排在正文之後者）。
        let sep = dl
            .items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Rule { x1, y1, x2, y2, .. } if (y1 - y2).abs() < 0.01 => {
                    Some((*x1, *y1, *x2, *y2))
                }
                _ => None,
            })
            .filter(|(x1, _, x2, _)| (x1 - 72.0).abs() < 0.6 && x2 - x1 > 10.0)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let (x1, y1, x2, y2) = sep.expect("正文末頁必須有尾註分隔線 Rule");
        assert_eq!(y1, y2, "分隔線是水平線");
        assert!(
            y1 > engine.params.margin(MARGIN_TOP),
            "分隔線在正文內容之後，實得 y={y1}"
        );
        assert!((x1 - 72.0).abs() < 0.6, "分隔線自版心左緣起，實得 {x1}");
        assert!(x2 - x1 > 10.0, "分隔線寬為版心 1/3，不得為零");
        // 尾註文字接排在末頁。
        let texts = display_texts(&dl);
        assert!(
            texts.iter().any(|t| t.contains("This is an endnote")),
            "尾註文字必須接排正文末頁：{texts:?}"
        );
    }

    /// K1(f)：docDefaults 的段後距進分頁——HC031 第 2 頁（索引 1）行數
    /// 必須從 54 下降（向 LO 的 49 收斂方向；帶制近似，不寫死 49），
    /// 且正文頁數不得低於 8。
    #[test]
    fn hc031_page1_lines_reduced_by_paragraph_spacing() {
        let engine = Engine::from_docx_bytes(HC031_ENDNOTE_DOC).expect("開啟 HC031 失敗");
        let p1 = engine.page_lines(1).expect("HC031 第 2 頁必須存在");
        assert!(
            p1.len() < 54,
            "段後距消費後第 2 頁行數必須 < 54（基線 54），實得 {}",
            p1.len()
        );
        assert!(
            engine.body_page_count() >= 8,
            "正文頁數不得低於 8，實得 {}",
            engine.body_page_count()
        );
    }

    /// K1-R（J1＋K2b 合併態改寫）：段距消費＋清單 marker 下 HC031 頁數守門。
    ///
    /// 原斷言（K1-R 時空）是 `page_count() == 9`（正文 8 ＋ 獨立尾註頁 1）：
    /// J1 契約下尾註接排末頁、獨立尾註頁已不存在；K2b 的 marker ＋縮排近似
    /// 又讓清單段斷行變多（LO 也畫 30 個 marker）。回歸本質（段距量化不得
    /// 擠出空頁）由 `hc031_no_blank_body_page` 守；此處鎖結構契約：正文
    /// 頁數在 [8,9]（8＝無 marker 基線；9＝marker 效應）且
    /// `page_count ＝ 正文 ＋ 純尾註續頁`。合併態實測基線見 K2b 卡報告。
    #[test]
    fn hc031_spacing_does_not_pad_extra_page() {
        let engine = Engine::from_docx_bytes(HC031_ENDNOTE_DOC).expect("開啟 HC031 失敗");
        let body = engine.body_page_count();
        assert!(
            (8..=9).contains(&body),
            "HC031 正文必須是 8（無 marker）或 9（K2b marker 效應），實得 {body}"
        );
        assert_eq!(
            engine.page_count(),
            engine.body_page_count() + engine.endnote_page_count(),
            "page_count ＝ 正文頁數 ＋ 純尾註續頁數"
        );
    }

    /// K1-R：段距消費後**不得產生只含空段的空頁**——HC031 正文每一頁都必須
    /// 有實質內容（空段落的頁＝段距量化把節末段擠出的症狀）。
    #[test]
    fn hc031_no_blank_body_page() {
        let engine = Engine::from_docx_bytes(HC031_ENDNOTE_DOC).expect("開啟 HC031 失敗");
        for page in 0..engine.body_page_count() {
            let lines = engine.page_lines(page).expect("正文頁必須存在");
            let text: String = lines.iter().map(|l| l.trim()).collect::<Vec<_>>().join("");
            assert!(
                !text.is_empty(),
                "HC031 正文第 {} 頁是空頁（只含空段）——段距量化多佔位把臨界頁擠出",
                page + 1
            );
        }
    }

    /// K1-R：段距量化餘數鏈**在表格邊界斷開**——表格塊的高度由表格幾何獨立
    /// 計算（不是段落流的連續堆疊），段落間距的垂直連續性在表格邊界中斷，
    /// 累積餘數不跨表格連續（餘數歸零重算）。
    ///
    /// 帶尺 10pt、每段段後距 8pt（＝0.8 帶）：不斷鏈時帶數為 0,1,1,1（總 3）；
    /// 第 3 段（索引 2）後斷鏈則為 0,1,0,1（總 2）——斷點前後的餘數互不
    /// 污染，且總帶數回到「各自段落流」的真實佔位。
    #[test]
    fn spacing_band_chain_breaks_at_table_boundary() {
        let band = 10.0f32;
        let sp: Vec<(Option<f32>, Option<f32>)> = (0..4).map(|_| (None, Some(8.0f32))).collect();
        let (_, after) = spacing_band_vectors(&sp, band, &[]);
        assert_eq!(after, vec![0, 1, 1, 1], "不斷鏈：跨段累積餘數");
        let (_, after_break) = spacing_band_vectors(&sp, band, &[2]);
        assert_eq!(
            after_break,
            vec![0, 1, 0, 1],
            "表格邊界斷鏈：餘數歸零重算（表格前後互不污染）"
        );
    }

    /// K1-R：表格塊之前一段的**段後距不另佔帶**（段落與表格塊之間的那段
    /// 空白由表格塊的前距承擔，同一段空白不在餘數鏈與版面帶各佔一次）。
    ///
    /// HC031 的實檔證據：正文第 4 頁（索引 3）本已「剛好塞滿」（段後距帶
    /// 吃掉最後一帶），節末段被擠到新頁 → 多出一頁只含空段的空頁（總頁數
    /// 9→10）。修正後節末段回到第 4 頁，總頁數回到 9（見
    /// `hc031_total_page_count_back_to_nine`／`hc031_no_blank_body_page`）。
    #[test]
    fn hc031_section_end_paragraph_stays_on_its_page() {
        let engine = Engine::from_docx_bytes(HC031_ENDNOTE_DOC).expect("開啟 HC031 失敗");
        // 正文第 4 頁（索引 3）必須含實質內容（原本節末段被擠到下一頁）。
        let p3 = engine.page_lines(3).expect("HC031 第 4 頁必須存在");
        assert!(
            p3.iter().any(|l| !l.trim().is_empty()),
            "HC031 第 4 頁必須有實質內容（節末段不得被擠成獨立空頁）"
        );
        // K6：Normal 259 auto × Calibri 1.2207（11pt）＝14.49pt，正文回到 9 頁。
        // LO 25.8 直接檔 render_lab 實測總頁 9（見交付報告）——舊「8＝LO」
        // 是 K6 前粗尺 11.9pt 的巧合，已作廢。仍不得有「只含空段」的空頁
        // （見 `hc031_no_blank_body_page`）。
        assert_eq!(
            engine.body_page_count(),
            9,
            "HC031 正文頁數＝9（K6 校準 pitch 後與 LO 25.8 直接檔實測一致）；實得 {}",
            engine.body_page_count()
        );
    }

    /// (b) HC031（J1R 改寫）：尾註接排正文末頁——正文頁數不變
    /// （`page_count` ＝ 正文 ＋ 純尾註續頁），且正文錨點記號「1」發在正文頁。
    #[test]
    fn hc031_endnote_joins_last_body_page_without_shifting_body() {
        let engine = Engine::from_docx_bytes(HC031_ENDNOTE_DOC).expect("開啟 HC031 失敗");
        let body = engine.body_page_count();
        // K1：段後距進分頁後正文由 8 頁增至 9 頁（LO 同一檔案正文 8 頁；
        // 帶制近似＋ListParagraph 的 contextualSpacing 尚未消費，故不寫死）。
        // 本測試的契約是「尾註只追加頁、不動正文分頁」，故改斷言下限。
        assert!(body >= 8, "HC031 正文頁數不得低於 8，實得 {body}");
        assert_eq!(
            engine.page_count(),
            body + engine.endnote_page_count(),
            "page_count ＝ 正文頁數 ＋ 純尾註續頁數"
        );
        let end = engine.endnote_layout();
        assert_eq!(end.inline.len(), 1, "一則尾註接排正文末頁");
        assert!(end.pages.is_empty(), "裝得下 ⇒ 無純尾註續頁");
        // 正文錨點記號：發在正文頁（不是尾註頁），編號自 1 起。
        let mark_pages: Vec<usize> = end
            .marks
            .iter()
            .enumerate()
            .filter(|(_, m)| !m.is_empty())
            .map(|(i, _)| i)
            .collect();
        assert_eq!(mark_pages.len(), 1, "正文恰一個尾註錨點記號");
        assert!(mark_pages[0] < body, "記號發在正文頁");
        assert_eq!(end.marks[mark_pages[0]][0].text, "1", "尾註編號自 1 起");
        // 記號確實落在正文頁的顯示列表裡。
        let dl = engine
            .page_display_list(mark_pages[0])
            .expect("正文頁必須存在");
        assert!(
            dl.items
                .iter()
                .any(|it| matches!(it, DrawItem::Text { text, .. } if text == "1")),
            "正文頁必須發射尾註錨點記號「1」"
        );
    }

    /// (c) 無尾註檔（SAMPLE）：尾註層全空、頁數與正文頁數相同（零擾動快路徑）。
    #[test]
    fn doc_without_endnotes_has_empty_end_layer() {
        let engine = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        assert!(
            engine.endnote_layout().is_empty(),
            "無尾註檔案的尾註層必須全空"
        );
        assert_eq!(engine.endnote_page_count(), 0, "無尾註 ⇒ 無尾註頁");
        assert_eq!(
            engine.page_count(),
            engine.body_page_count(),
            "無尾註 ⇒ 頁數與正文完全相同"
        );
        assert!(
            engine.page_display_list(engine.page_count()).is_none(),
            "超出頁數必須回 None（不越界 panic）"
        );
    }

    /// (d) 多條尾註（40）⇒ 正文末頁接排一部分、其餘排到純尾註續頁；每頁
    /// 條目帶數 ≤ 該頁容量（續頁頁數不寫死成 LO 常數）。
    #[test]
    fn many_endnotes_flow_to_inline_then_continuation_pages() {
        let engine = build_endnote_doc(40, "尾註內容。");
        let body = engine.body_page_count();
        let end = engine.endnote_layout();
        assert!(!end.inline.is_empty(), "40 條尾註必須有部分接排正文末頁");
        assert!(
            engine.endnote_page_count() >= 1,
            "40 條尾註裝不下末頁 ⇒ 必須有純尾註續頁，實得 {}",
            engine.endnote_page_count()
        );
        assert_eq!(
            end.inline.len() + end.pages.iter().map(|p| p.len()).sum::<usize>(),
            40,
            "條目不得漏（接排＋續頁都要有落點）"
        );
        // 接排容量：與 compute_endnotes 同一算式，接排帶數不得溢出。
        let inline_used: u32 = end.inline.iter().map(|it| it.bands).sum();
        let inline_cap = endnote_inline_capacity_bands(&engine);
        assert!(
            inline_used <= inline_cap as u32,
            "正文末頁接排用了 {inline_used} 帶，容量 {inline_cap} 帶——不得溢出"
        );
        let (cap_first, cap_rest) = endnote_capacity_bands(&engine);
        for (i, page) in end.pages.iter().enumerate() {
            let limit = if i == 0 { cap_first } else { cap_rest };
            let used: u32 = page.iter().map(|it| it.bands).sum();
            assert!(
                used <= limit as u32,
                "第 {i} 個續頁用了 {used} 帶，容量 {limit} 帶——不得溢出"
            );
        }
        // 分隔線：正文末頁畫左緣起分隔線；續頁畫置中續接線（各恰好一條）。
        let last = engine
            .page_display_list(body - 1)
            .expect("正文末頁必須存在");
        assert_eq!(
            last.items
                .iter()
                .filter(|it| matches!(it, DrawItem::Rule { .. }))
                .count(),
            1,
            "正文末頁接排區必須有恰好一條分隔線"
        );
        for i in 0..engine.endnote_page_count() {
            let dl = engine.page_display_list(body + i).expect("續頁必須存在");
            let rules = dl
                .items
                .iter()
                .filter(|it| matches!(it, DrawItem::Rule { .. }))
                .count();
            assert_eq!(rules, 1, "尾註續頁第 {i} 頁必須有恰好一條續接線");
        }
    }

    /// (e) 一條超長尾註（單條帶數超過整頁容量）⇒ 獨佔一頁、不 panic、不切條。
    #[test]
    fn single_oversized_endnote_occupies_its_own_page() {
        let long = "超長尾註內容。".repeat(2000);
        let engine = build_endnote_doc(1, &long);
        assert_eq!(engine.endnote_page_count(), 1, "單條尾註 ⇒ 一個尾註頁");
        let end = engine.endnote_layout();
        assert_eq!(end.pages.len(), 1);
        assert_eq!(end.pages[0].len(), 1, "單條尾註獨佔一頁");
        let item = &end.pages[0][0];
        assert!(item.lines.len() > 1, "超長尾註必須換行成多行");
        let (cap_first, _) = endnote_capacity_bands(&engine);
        assert!(
            item.bands as usize > cap_first,
            "本測的前提：單條帶數必須超過整頁容量（實得 {} 帶）",
            item.bands
        );
    }

    /// (f) `flow_items_to_pages` 純函式邊界（細節測在 footnotes 模組）：
    /// 空輸入回空；單條裝不下獨佔一頁。
    #[test]
    fn flow_items_to_pages_edge_cases_are_total() {
        assert!(footnotes::flow_items_to_pages(&[], 10, 10).is_empty());
        let big = footnotes::NoteItem {
            lines: vec![footnotes::NoteLine { runs: Vec::new() }],
            bands: 99,
            line_h_pt: 14.0,
            continuation: false,
        };
        let pages = footnotes::flow_items_to_pages(&[big], 3, 3);
        assert_eq!(pages, vec![vec![0]], "單條超容量 ⇒ 獨佔一頁");
    }

    /// (g) RTL／中文尾註實檔（J1R 改寫）：中文尾註文字接排正文末頁、不 panic。
    /// 意圖保留：尾註文字仍必須落在文件最後一頁（現在是最後正文頁）。
    #[test]
    fn rtl_chinese_endnote_file_emits_text_on_last_body_page() {
        let engine = Engine::from_docx_bytes(RTL_ENDNOTE_DOC).expect("開啟 arabic_rtl_notes 失敗");
        assert_eq!(engine.endnote_page_count(), 0, "一則尾註接排正文末頁");
        let body = engine.body_page_count();
        let dl = engine
            .page_display_list(body - 1)
            .expect("正文末頁必須存在");
        let texts = display_texts(&dl);
        assert!(
            texts.iter().any(|t| t.contains("尾註一")),
            "中文尾註文字必須接排正文末頁：{texts:?}"
        );
    }

    /// (h) POI 實檔（J1R 改寫；endnote id=2、separator 慣例 0/1）：
    /// 尾註文字「XXX」接排正文末頁。
    #[test]
    fn poi_endnote_doc_emits_item_on_last_body_page() {
        let engine = Engine::from_docx_bytes(POI_ENDNOTE_DOC).expect("開啟 POI endnotes 失敗");
        assert_eq!(engine.endnote_page_count(), 0, "一則尾註接排正文末頁");
        let body = engine.body_page_count();
        let dl = engine
            .page_display_list(body - 1)
            .expect("正文末頁必須存在");
        let texts = display_texts(&dl);
        assert!(
            texts.iter().any(|t| t.contains("XXX")),
            "尾註文字必須接排正文末頁：{texts:?}"
        );
    }

    /// (i) TextReplacer02 實檔（J1R 改寫）：尾註文字「This is the endnote」
    /// 接排正文末頁。
    #[test]
    fn text_replacer_endnote_doc_emits_item_on_last_body_page() {
        let engine = Engine::from_docx_bytes(TR02_ENDNOTE_DOC).expect("開啟 TextReplacer02 失敗");
        assert_eq!(engine.endnote_page_count(), 0, "一則尾註接排正文末頁");
        let body = engine.body_page_count();
        let dl = engine
            .page_display_list(body - 1)
            .expect("正文末頁必須存在");
        let texts = display_texts(&dl);
        assert!(
            texts.iter().any(|t| t.contains("This is the endnote")),
            "尾註文字必須接排正文末頁：{texts:?}"
        );
    }

    /// (j) J1-fix 改寫：末頁**仍有餘裕**時，尾註預留不得位移正文分頁斷點
    /// （單行正文＋少量尾註：reserve 遠小於末頁容量）。尾註預留若超過末頁
    /// 容量，正文**如實溢出**多一頁（比照 F2c；見 (b)/(d) 的保留帶測試）。
    #[test]
    fn endnote_reserve_keeps_body_when_last_page_has_room() {
        let with_end = build_endnote_doc(3, "尾註內容。");
        let without_end = build_endnote_doc(0, "尾註內容。");
        assert_eq!(
            with_end.page_breaks_json(),
            without_end.page_breaks_json(),
            "尾註不得位移任何正文分頁斷點"
        );
        assert_eq!(
            with_end.body_page_count(),
            without_end.body_page_count(),
            "尾註不得增加正文頁數"
        );
    }

    /// (k) 存檔零影響（round-trip 鐵律）：尾註層是純讀取投影——含尾註的實檔
    /// 未觸碰存檔必須 byte-identical。
    #[test]
    fn endnote_layer_does_not_touch_save_path() {
        let engine = Engine::from_docx_bytes(HC031_ENDNOTE_DOC).expect("開啟 HC031 失敗");
        let diff = engine.roundtrip_check().expect("round-trip 對照失敗");
        assert!(
            diff.byte_identical,
            "尾註發射層不得影響存檔位元組：{diff:?}"
        );
    }

    /// J1R（紅燈）：reader-styles 實檔——尾註接排**正文末頁**（LO 3 頁）。
    /// 修前我方把尾註放到獨立追加頁 ⇒ 4 頁。
    #[test]
    fn reader_styles_endnotes_join_last_body_page() {
        const RS: &[u8] = include_bytes!(
            "../../../corpus/phpword/tests_PhpWordTests__files_documents_reader-styles.docx"
        );
        let engine = Engine::from_docx_bytes(RS).expect("開啟 reader-styles.docx");
        assert_eq!(engine.page_count(), 3, "LO 三頁：尾註接排正文末頁");
        assert_eq!(engine.endnote_page_count(), 0, "裝得下 ⇒ 不應有純尾註續頁");
        let last = engine.body_page_count() - 1;
        let dl = engine.page_display_list(last).expect("正文末頁必須存在");
        assert!(
            dl.items
                .iter()
                .any(|it| matches!(it, DrawItem::Rule { .. })),
            "正文末頁必須有尾註分隔線"
        );
        let texts = display_texts(&dl);
        assert!(
            texts.iter().any(|t| t.contains("End note")),
            "正文末頁必須有尾註文字：{texts:?}"
        );
    }

    /// J1R（紅燈）：POI 實檔——LO 1 頁（尾註接排正文末頁）。
    #[test]
    fn poi_endnotes_join_last_body_page() {
        let engine = Engine::from_docx_bytes(POI_ENDNOTE_DOC).expect("開啟 POI endnotes 失敗");
        assert_eq!(engine.page_count(), 1, "LO 一頁：尾註接排正文末頁");
        assert_eq!(engine.endnote_page_count(), 0, "裝得下 ⇒ 不應有純尾註續頁");
        let last = engine.body_page_count() - 1;
        let dl = engine.page_display_list(last).expect("正文末頁必須存在");
        assert!(
            dl.items
                .iter()
                .any(|it| matches!(it, DrawItem::Rule { .. })),
            "正文末頁必須有尾註分隔線"
        );
        let texts = display_texts(&dl);
        assert!(
            texts.iter().any(|t| t.contains("XXX")),
            "正文末頁必須有尾註文字：{texts:?}"
        );
    }

    /// J1R（紅燈）：單則短尾註接排正文末頁，不另開尾註頁。
    #[test]
    fn single_endnote_joins_last_body_page() {
        let engine = build_endnote_doc(1, "尾註內容。");
        assert_eq!(engine.endnote_page_count(), 0, "單則短尾註必須接排正文末頁");
        let last = engine.body_page_count() - 1;
        let dl = engine.page_display_list(last).expect("正文末頁必須存在");
        let texts = display_texts(&dl);
        assert!(
            texts.iter().any(|t| t.contains("尾註內容。")),
            "正文末頁必須有尾註文字：{texts:?}"
        );
    }

    /// J1R（紅燈）：多條溢出——正文末頁接排一部分，其餘走純尾註續頁；
    /// 續頁頁數不寫死（不得把 LO 頁數當常數）。
    #[test]
    fn many_endnotes_inline_then_continue_pages() {
        let engine = build_endnote_doc(40, "尾註內容。");
        let body = engine.body_page_count();
        assert!(
            engine.endnote_page_count() >= 1,
            "40 條尾註必須有純尾註續頁，實得 {}",
            engine.endnote_page_count()
        );
        let last = body - 1;
        let dl = engine.page_display_list(last).expect("正文末頁必須存在");
        let texts = display_texts(&dl);
        assert!(
            texts.iter().any(|t| t.contains("尾註內容。")),
            "正文末頁必須接排至少一條尾註：{texts:?}"
        );
        assert!(
            dl.items
                .iter()
                .any(|it| matches!(it, DrawItem::Rule { .. })),
            "正文末頁尾註區必須有分隔線"
        );
        assert_eq!(
            engine.page_count(),
            body + engine.endnote_page_count(),
            "page_count ＝正文頁＋純尾註續頁（接排部分不另計頁）"
        );
    }

    /// (l) 編輯面不觸尾註：尾註頁的 caret 命中回 None（編輯面只認正文頁）。
    #[test]
    fn caret_on_endnote_page_is_none() {
        let engine = Engine::from_docx_bytes(HC031_ENDNOTE_DOC).expect("開啟 HC031 失敗");
        let body = engine.body_page_count();
        assert!(
            engine.caret_at_json(body, 100.0, 100.0).is_none(),
            "尾註頁不得有 caret 命中（編輯面只認正文頁）"
        );
        assert!(
            engine.caret_rect_json(0, 0).is_some(),
            "正文段落仍有 caret 落點（尾註不影響編輯面）"
        );
    }

    /// (m) 尾註接排幾何＝**末節**（J1R 改寫）：兩節文件（第一節 Letter、
    /// 第二節 A4）＋一則尾註 ⇒ 接排所在正文末頁必須是 A4（末節），不是
    /// 第一節的 Letter。意圖保留：尾註永遠取末節紙張。
    #[test]
    fn endnote_inline_uses_last_section_geometry() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:pPr><w:r><w:t>第一節</w:t></w:r></w:p>
    <w:p><w:r><w:t>第二節</w:t></w:r><w:r><w:endnoteReference w:id="1"/></w:r></w:p>
    <w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr>
  </w:body>
</w:document>"#;
        let notes = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:endnote w:id="1"><w:p><w:r><w:t xml:space="preserve">末節尾註。</w:t></w:r></w:p></w:endnote>
</w:endnotes>"#;
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        base.set_part("word/document.xml", xml.as_bytes().to_vec());
        base.set_part("word/endnotes.xml", notes.as_bytes().to_vec());
        let docx = base.save_docx().expect("存回 docx 失敗");
        let engine = Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗");
        assert_eq!(engine.endnote_page_count(), 0, "一則尾註接排正文末頁");
        let body = engine.body_page_count();
        assert!(body >= 2, "兩節 ⇒ 正文至少兩頁");
        let dl = engine
            .page_display_list(body - 1)
            .expect("正文末頁必須存在");
        assert_eq!(engine.endnote_layout().inline.len(), 1, "尾註接排末頁");
        // 末節＝A4（11906×16838 twips ＝ 595.3×841.9pt）；第一節是 Letter。
        assert!(
            (dl.width_pt - 595.3).abs() < 0.5,
            "尾註接排頁必須用末節（A4）頁寬，實得 {}",
            dl.width_pt
        );
        assert!(
            (dl.height_pt - 841.9).abs() < 0.5,
            "尾註接排頁必須用末節（A4）頁高，實得 {}",
            dl.height_pt
        );
    }

    // ---------------- F3b：尾註編號消費＋續接線 ----------------

    /// (F3b-1) reader-styles 實檔：settings `endnotePr numFmt=lowerRoman`
    /// ⇒ 正文記號與條目前綴都應是 `i`（十進位對照由 fixture 測）。
    #[test]
    fn reader_styles_endnote_uses_lower_roman_label() {
        const RS: &[u8] = include_bytes!(
            "../../../corpus/phpword/tests_PhpWordTests__files_documents_reader-styles.docx"
        );
        let engine = Engine::from_docx_bytes(RS).expect("開啟 reader-styles.docx");
        assert_eq!(engine.endnote_page_count(), 0, "一則尾註接排正文末頁");
        // 記號（正文頁）：lowerRoman ⇒ i。
        let end = engine.endnote_layout();
        let marks: Vec<String> = end.marks.iter().flatten().map(|m| m.text.clone()).collect();
        assert_eq!(
            marks,
            vec!["i".to_string()],
            "記號應為小寫羅馬 i：{marks:?}"
        );
        // 條目前綴：i.（layout_entry 的 superscript run）。
        assert_eq!(
            endnote_prefixes(&engine),
            vec!["i.".to_string()],
            "條目前綴應為 i."
        );
    }

    /// (F3b-2) Saturn 實檔：`endnotePr pos=sectEnd`（單節）——**pos 未消費**
    /// （老實邊界）：本卡不實裝 sectEnd/pageEnd 分支，渲染照 F3 現行
    /// `documentEnd`（文件末頁）；此檔無 numFmt ⇒ 編號十進位。
    #[test]
    fn saturn_pos_sectend_renders_as_document_end_decimal() {
        const SATURN: &[u8] = include_bytes!(
            "../../../corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Saturn.docx"
        );
        let engine = Engine::from_docx_bytes(SATURN).expect("開啟 Saturn.docx");
        // pos=sectEnd 未消費 ⇒ 仍按 documentEnd 收在文件末頁（接排正文末頁）。
        assert_eq!(
            engine.endnote_page_count(),
            0,
            "pos 未消費 ⇒ 仍按 documentEnd 接排正文末頁"
        );
        let end = engine.endnote_layout();
        let marks: Vec<String> = end.marks.iter().flatten().map(|m| m.text.clone()).collect();
        assert_eq!(
            marks,
            vec!["1".to_string(), "2".to_string()],
            "無 numFmt ⇒ 十進位 1, 2（pos 不影響編號）：{marks:?}"
        );
        assert_eq!(
            endnote_prefixes(&engine),
            vec!["1.".to_string(), "2.".to_string()],
            "條目前綴應為十進位"
        );
    }

    /// (F3b-3) 單則短尾註（J1R 改寫）：接排正文末頁有分隔線、無續頁
    /// （⇒ 不畫續接線）。意圖保留：短尾註不得多佔一頁。
    #[test]
    fn single_endnote_inline_has_separator_no_continuation() {
        let engine = build_endnote_doc(1, "尾註內容。");
        assert_eq!(engine.endnote_page_count(), 0, "一則短尾註接排正文末頁");
        let body = engine.body_page_count();
        let dl = engine
            .page_display_list(body - 1)
            .expect("正文末頁必須存在");
        let rules = dl
            .items
            .iter()
            .filter(|it| matches!(it, DrawItem::Rule { .. }))
            .count();
        assert_eq!(rules, 1, "正文末頁接排區恰一條分隔線");
        assert!(
            engine.page_display_list(body).is_none(),
            "無續頁（不得越界）"
        );
    }

    /// (F3b-4) 有接排區時，溢出續頁頂部畫**置中短線**（寬＝版心×1/3、
    /// y＝margin_top）；分隔線留在正文末頁（左緣起）。
    #[test]
    fn endnote_continuation_page_draws_centered_short_rule() {
        // 120 條：確保接排區之後仍有溢出續頁（頁數不寫死）。
        let engine = build_endnote_doc(120, "尾註內容。");
        let body = engine.body_page_count();
        assert!(
            !engine.endnote_layout().inline.is_empty(),
            "120 條尾註必須有部分接排正文末頁"
        );
        assert!(
            engine.endnote_page_count() >= 1,
            "120 條尾註必須有溢出續頁，實得 {}",
            engine.endnote_page_count()
        );
        let geo = engine.end_geo().expect("末節幾何");
        let content_w = engine.end_content_w_pt();
        // 第一個溢出續頁（有接排區 ⇒ 全部是置中續接線）。
        let dl = engine.page_display_list(body).expect("續頁必須存在");
        let rule = dl
            .items
            .iter()
            .find_map(|it| match it {
                DrawItem::Rule { x1, y1, x2, y2, .. } => Some((*x1, *y1, *x2, *y2)),
                _ => None,
            })
            .expect("續頁必須有續接線 Rule");
        let (x1, y1, x2, y2) = rule;
        assert_eq!(y1, y2, "續接線必須是水平線");
        assert!(
            (y1 - geo.margin_top_pt).abs() < 0.6,
            "續接線 y＝margin_top，實得 {y1}"
        );
        assert!(
            ((x2 - x1) - content_w * footnotes::SEP_WIDTH_RATIO).abs() < 1.0,
            "續接線寬＝版心×1/3，實得 {}",
            x2 - x1
        );
        let center = (x1 + x2) / 2.0;
        let content_center = geo.margin_left_pt + content_w / 2.0;
        assert!(
            (center - content_center).abs() < 1.0,
            "續接線必須置中於版心，實得 {center} vs {content_center}"
        );
        // 接排區分隔線仍在正文末頁、左緣起（非置中），且在正文內容之後。
        let dl0 = engine
            .page_display_list(body - 1)
            .expect("正文末頁必須存在");
        let sep = dl0
            .items
            .iter()
            .find_map(|it| match it {
                DrawItem::Rule { x1, y1, x2, y2, .. } => Some((*x1, *y1, *x2, *y2)),
                _ => None,
            })
            .expect("正文末頁必須有接排分隔線 Rule");
        assert!(
            (sep.0 - geo.margin_left_pt).abs() < 0.6,
            "接排分隔線自版心左緣起，實得 {}",
            sep.0
        );
        assert!(
            sep.1 > geo.margin_top_pt,
            "接排分隔線在正文內容之後，實得 y={}",
            sep.1
        );
    }

    /// (F3b-5) 自製 fixture：settings `endnotePr numFmt=lowerRoman`
    /// 兩節×各 2 條——continuous 得 `i. ii. iii. iv.`；eachSect 得節邊重啟
    /// `i. ii. i. ii.`；記號與前綴一致。
    #[test]
    fn endnote_lower_roman_labels_and_section_restart() {
        let cont = build_endnote_settings_doc(
            2,
            2,
            "尾註內容。",
            Some(
                r#"<w:endnotePr><w:numFmt w:val="lowerRoman"/><w:numRestart w:val="continuous"/></w:endnotePr>"#,
            ),
        );
        let prefixes = endnote_prefixes(&cont);
        assert_eq!(
            prefixes,
            vec!["i.", "ii.", "iii.", "iv."],
            "continuous：跨節連續 i..iv：{prefixes:?}"
        );
        let marks: Vec<String> = cont
            .endnote_layout()
            .marks
            .iter()
            .flatten()
            .map(|m| m.text.clone())
            .collect();
        assert_eq!(marks, vec!["i", "ii", "iii", "iv"], "記號同字面：{marks:?}");

        let sect = build_endnote_settings_doc(
            2,
            2,
            "尾註內容。",
            Some(
                r#"<w:endnotePr><w:numFmt w:val="lowerRoman"/><w:numRestart w:val="eachSect"/></w:endnotePr>"#,
            ),
        );
        let prefixes = endnote_prefixes(&sect);
        assert_eq!(
            prefixes,
            vec!["i.", "ii.", "i.", "ii."],
            "eachSect：第二節重啟 i..ii：{prefixes:?}"
        );
        let marks: Vec<String> = sect
            .endnote_layout()
            .marks
            .iter()
            .flatten()
            .map(|m| m.text.clone())
            .collect();
        assert_eq!(marks, vec!["i", "ii", "i", "ii"], "記號同字面：{marks:?}");
    }

    /// (F3b-6) 零變化守門：十進位 settings 與無 settings 兩版渲染逐位元組相同
    /// （`assign` 對十進位不寫 label ⇒ number_label 空 ⇒ 十進位兜底）。
    #[test]
    fn decimal_and_no_settings_endnote_render_byte_identical() {
        let decimal = build_endnote_settings_doc(
            2,
            2,
            "尾註內容。",
            Some(r#"<w:endnotePr><w:numFmt w:val="decimal"/></w:endnotePr>"#),
        );
        let none = build_endnote_settings_doc(2, 2, "尾註內容。", None);
        assert_eq!(
            render_json(&decimal),
            render_json(&none),
            "十進位與無 settings 必須逐位元組相同"
        );
    }

    // ---------------- H2：頁底註腳編號消費（numFmt／numRestart）----------------

    /// corpus 實檔：settings 層與節層各宣告 `w:footnotePr/numFmt=chicago`，
    /// 但正文**零引用** ⇒ 有設定無引用 ⇒ 渲染零變化＋`footnotes_json` 空
    /// （**不得虛構引用**）。
    const CHICAGO_NO_REF_DOC: &[u8] =
        include_bytes!("../../../corpus/ooxmlexport/tableCurrupt.docx");
    /// corpus 實檔：12 節＋`numRestart=eachSect`，正文**零引用** ⇒ 同上零擾動
    /// （eachSect 的「有引用重啟」實證只能靠自製 fixture，見下）。
    const EACH_SECT_NO_REF_DOC: &[u8] = include_bytes!(
        "../../../corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_solar-system.docx"
    );
    /// corpus 實檔：單節＋`numRestart=eachSect`＋**一處引用** ⇒ 消費路徑
    /// 走通；單節的 eachSect 與 continuous 同值 ⇒ 渲染不變。
    const EACH_SECT_ONE_REF_DOC: &[u8] =
        include_bytes!("../../../corpus/ooxmlexport/fdo77725.docx");

    /// 移除 XML 中某元素（成對與自閉合兩形；字串層，A/B 基準檔用）。
    fn strip_el(xml: &str, tag: &str) -> String {
        let open = format!("<w:{tag}>");
        let close = format!("</w:{tag}>");
        let mut out = String::with_capacity(xml.len());
        let mut rest = xml;
        loop {
            let Some(i) = rest.find(&open) else {
                out.push_str(rest);
                break;
            };
            let Some(j) = rest[i..].find(&close) else {
                out.push_str(rest);
                break;
            };
            out.push_str(&rest[..i]);
            rest = &rest[i + j + close.len()..];
        }
        out.replace(&format!("<w:{tag}/>"), "")
    }

    /// 把 `word/settings.xml` 與 `word/document.xml` 裡的 `w:footnotePr`
    /// 元素整段移除 ⇒ 得到「無註腳設定」的 A/B 基準檔（同一份其餘內容）。
    fn engine_without_footnote_pr(bytes: &[u8]) -> Engine {
        let mut doc = parse(bytes).expect("解析 docx 失敗");
        for name in ["word/settings.xml", "word/document.xml"] {
            if let Some(raw) = doc.parts.get(name) {
                let text = String::from_utf8_lossy(raw).to_string();
                doc.set_part(name, strip_el(&text, "footnotePr").into_bytes());
            }
        }
        let docx = doc.save().expect("存回 docx 失敗");
        Engine::from_docx_bytes(&docx).expect("開啟 A/B 基準檔失敗")
    }

    /// 全部頁的顯示列表 JSON（渲染逐位元組比較的口徑）。
    fn all_pages_json(engine: &Engine) -> Vec<String> {
        (0..engine.page_count())
            .map(|p| engine.page_display_list_json(p).unwrap_or_default())
            .collect()
    }

    /// 自 `footnotes_json` 取「逐條 (編號前綴, 註腳內文)」與「逐個記號字面」。
    ///
    /// 前綴經 `footnotes::note_number_text` 的同一出口（條目首 run 的文字
    /// 即前綴字面）；記號的 `text` 欄同值（無尾點）。
    fn footnote_json_items(engine: &Engine) -> (Vec<(String, String)>, Vec<String>) {
        let v: serde_json::Value =
            serde_json::from_str(&engine.footnotes_json()).expect("footnotes_json 必須是合法 JSON");
        let mut items: Vec<(String, String)> = Vec::new();
        for page in v["pages"].as_array().cloned().unwrap_or_default() {
            for it in page["items"].as_array().cloned().unwrap_or_default() {
                let runs = it["lines"][0]["runs"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let prefix = runs
                    .first()
                    .and_then(|r| r["text"].as_str())
                    .unwrap_or_default()
                    .to_string();
                let body: String = runs
                    .iter()
                    .skip(1)
                    .filter_map(|r| r["text"].as_str())
                    .collect();
                items.push((prefix, body));
            }
        }
        let mut marks: Vec<String> = Vec::new();
        for page in v["marks"].as_array().cloned().unwrap_or_default() {
            for m in page.as_array().cloned().unwrap_or_default() {
                if let Some(t) = m["text"].as_str() {
                    marks.push(t.to_string());
                }
            }
        }
        (items, marks)
    }

    /// 自製 fixture：兩節 × 各 3 條被引用註腳。
    ///
    /// corpus 沒有「多節＋有引用＋非十進位」實檔（PM 已實掃），故本 fixture
    /// 是 eachSect／numFmt 的主實證。仿既有 `build_theme_doc`／`set_part`
    /// 手法：以 sample.docx 為容器灌入 document.xml（兩節）、footnotes.xml
    /// （6 則）與 settings.xml（可選）。
    ///
    /// - `settings_pr`：寫進 `word/settings.xml` 的 `w:footnotePr` 內文
    ///   （空字串＝不寫 settings 層）。
    /// - `sect_prs`：逐節的 `w:footnotePr` 內文（索引＝**section id**；
    ///   短於節數者以空字串補）。
    fn build_two_section_footnote_doc(settings_pr: &str, sect_prs: &[&str]) -> Engine {
        let sect = |i: usize| {
            let inner = sect_prs.get(i).copied().unwrap_or("");
            if inner.is_empty() {
                "<w:sectPr/>".to_string()
            } else {
                format!("<w:sectPr><w:footnotePr>{inner}</w:footnotePr></w:sectPr>")
            }
        };
        let mut body = String::new();
        for i in 1..=3 {
            body.push_str(&format!(
                r#"<w:p><w:r><w:t>第一節第{i}段</w:t></w:r><w:r><w:footnoteReference w:id="{i}"/></w:r></w:p>"#
            ));
        }
        // 第一節的節界：段落 pPr 內的 sectPr 收束本節（該段屬本節）。
        body.push_str(&format!(
            r#"<w:p><w:pPr>{}</w:pPr><w:r><w:t>第一節末段</w:t></w:r></w:p>"#,
            sect(0)
        ));
        for i in 4..=6 {
            body.push_str(&format!(
                r#"<w:p><w:r><w:t>第二節第{i}段</w:t></w:r><w:r><w:footnoteReference w:id="{i}"/></w:r></w:p>"#
            ));
        }
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    {body}
    {}
  </w:body>
</w:document>"#,
            sect(1)
        );
        let mut notes = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
        );
        for i in 1..=6 {
            notes.push_str(&format!(
                r#"<w:footnote w:id="{i}"><w:p><w:r><w:t>註腳內文{i}</w:t></w:r></w:p></w:footnote>"#
            ));
        }
        notes.push_str("</w:footnotes>");
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        base.set_part("word/document.xml", xml.into_bytes());
        base.set_part("word/footnotes.xml", notes.into_bytes());
        if !settings_pr.is_empty() {
            let settings = format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{settings_pr}</w:settings>"#
            );
            base.set_part("word/settings.xml", settings.into_bytes());
        }
        let docx = base.save_docx().expect("存回 docx 失敗");
        Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗")
    }

    /// (a) corpus 實檔（`tableCurrupt.docx`）：`numFmt=chicago` 有設定但正文
    /// 零引用 ⇒ **渲染零變化＋`footnotes_json` 空**（不得虛構引用）。
    #[test]
    fn chicago_setting_without_reference_changes_nothing() {
        let engine = Engine::from_docx_bytes(CHICAGO_NO_REF_DOC).expect("開啟 tableCurrupt 失敗");
        let (items, marks) = footnote_json_items(&engine);
        assert!(items.is_empty(), "零引用 ⇒ 不得有任何註腳條目：{items:?}");
        assert!(marks.is_empty(), "零引用 ⇒ 不得有任何記號：{marks:?}");
        assert!(engine.foot.is_empty(), "註腳層必須全空");
        assert_eq!(engine.footnote_rounds(), 0, "零引用不計收斂輪數");
        // A/B：與「移除 w:footnotePr」的同一檔逐位元組相同。
        let stripped = engine_without_footnote_pr(CHICAGO_NO_REF_DOC);
        assert_eq!(
            all_pages_json(&engine),
            all_pages_json(&stripped),
            "chicago 設定但零引用 ⇒ 渲染必須與無設定逐位元組相同"
        );
    }

    /// (b) corpus 實檔（solar-system.docx）：12 節＋`numRestart=eachSect`，
    /// 正文零引用 ⇒ 同 (a) 零擾動（eachSect 的「有引用重啟」實證靠 fixture）。
    #[test]
    fn each_sect_setting_without_reference_changes_nothing() {
        let engine = Engine::from_docx_bytes(EACH_SECT_NO_REF_DOC).expect("開啟 solar-system 失敗");
        let (items, marks) = footnote_json_items(&engine);
        assert!(items.is_empty(), "零引用 ⇒ 不得有任何註腳條目：{items:?}");
        assert!(marks.is_empty(), "零引用 ⇒ 不得有任何記號：{marks:?}");
        assert!(engine.foot.is_empty(), "註腳層必須全空");
        let stripped = engine_without_footnote_pr(EACH_SECT_NO_REF_DOC);
        assert_eq!(
            all_pages_json(&engine),
            all_pages_json(&stripped),
            "eachSect 設定但零引用 ⇒ 渲染必須與無設定逐位元組相同"
        );
    }

    /// (c) corpus 實檔（fdo77725.docx）：單節＋`numRestart=eachSect`＋**一處
    /// 引用** ⇒ 消費路徑走通；單節的 eachSect 與 continuous 同值 ⇒ 編號仍是
    /// 十進位「1」，渲染與「無設定」逐位元組相同。
    #[test]
    fn each_sect_single_section_reference_keeps_decimal_numbering() {
        let engine = Engine::from_docx_bytes(EACH_SECT_ONE_REF_DOC).expect("開啟 fdo77725 失敗");
        let (items, marks) = footnote_json_items(&engine);
        assert_eq!(items.len(), 1, "一處引用 ⇒ 一條註腳：{items:?}");
        assert_eq!(items[0].0, "1.", "單節 eachSect ⇒ 仍是十進位前綴「1.」");
        assert_eq!(marks, vec!["1".to_string()], "記號字面與前綴同值（無尾點）");
        let stripped = engine_without_footnote_pr(EACH_SECT_ONE_REF_DOC);
        assert_eq!(
            all_pages_json(&engine),
            all_pages_json(&stripped),
            "單節 eachSect ＝ continuous 同值 ⇒ 渲染零變化"
        );
    }

    /// (d) 自製 fixture：settings 層 `numFmt=lowerRoman` ⇒ 兩節連續編號
    /// `i. ii. iii. iv. v. vi.`（十進位不再出現）。
    #[test]
    fn lower_roman_num_fmt_is_consumed_from_settings() {
        let engine = build_two_section_footnote_doc(
            r#"<w:footnotePr><w:numFmt w:val="lowerRoman"/></w:footnotePr>"#,
            &[],
        );
        let (items, marks) = footnote_json_items(&engine);
        let prefixes: Vec<&str> = items.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            prefixes,
            vec!["i.", "ii.", "iii.", "iv.", "v.", "vi."],
            "settings 層 lowerRoman 必須被消費（連續編號）"
        );
        assert_eq!(
            marks,
            vec!["i", "ii", "iii", "iv", "v", "vi"],
            "記號字面必須與前綴同值（經 note_number_text 同一出口）"
        );
    }

    /// (e) 自製 fixture：加 `numRestart=eachSect` ⇒ 第二節從 `i.` 重啟
    /// （節域計數器歸位）。
    #[test]
    fn each_sect_restarts_numbering_at_section_boundary() {
        let engine = build_two_section_footnote_doc(
            r#"<w:footnotePr><w:numFmt w:val="lowerRoman"/><w:numRestart w:val="eachSect"/></w:footnotePr>"#,
            &[],
        );
        let (items, _) = footnote_json_items(&engine);
        let prefixes: Vec<&str> = items.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            prefixes,
            vec!["i.", "ii.", "iii.", "i.", "ii.", "iii."],
            "eachSect ⇒ 第二節必須從 i. 重啟"
        );
    }

    /// (f) 自製 fixture：節層 `numFmt` 只作用於**自己的節**（節 id ↔ 清單
    /// 下標對齊的實證）：第一節 lowerRoman、第二節無 numFmt ⇒ 第二節回到
    /// 十進位（＝十進位兜底原路徑）。
    #[test]
    fn section_level_num_fmt_applies_to_its_own_section_only() {
        let engine = build_two_section_footnote_doc("", &[r#"<w:numFmt w:val="lowerRoman"/>"#, ""]);
        let (items, _) = footnote_json_items(&engine);
        let prefixes: Vec<&str> = items.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            prefixes,
            vec!["i.", "ii.", "iii.", "4.", "5.", "6."],
            "節層 numFmt 只作用於自己的節（節 id 索引不得錯位）"
        );
    }

    /// (g) 自製 fixture：全十進位版 ⇒ 與「無 numFmt」版渲染逐位元組相同
    /// （零變化守門：十進位不消費 ⇒ 編號表為空 ⇒ 十進位兜底原路徑）。
    #[test]
    fn decimal_num_fmt_changes_nothing_byte_for_byte() {
        let dec = build_two_section_footnote_doc(
            r#"<w:footnotePr><w:numFmt w:val="decimal"/></w:footnotePr>"#,
            &[],
        );
        let none = build_two_section_footnote_doc(r#"<w:footnotePr/>"#, &[]);
        assert_eq!(
            all_pages_json(&dec),
            all_pages_json(&none),
            "十進位 numFmt 不得改動任何位元組"
        );
    }

    /// (h) corpus 實檔（`test-data_document_footnotes.docx`）：**有註腳但無
    /// numFmt**（十進位）⇒ 逐位元組不變——最關鍵的零變化組（有註腳卻不動
    /// 一個位元組：十進位不消費 ⇒ 編號表為空 ⇒ 十進位兜底原路徑）。
    #[test]
    fn decimal_footnote_corpus_doc_changes_nothing_byte_for_byte() {
        let engine = Engine::from_docx_bytes(FOOT_DOC).expect("開啟 footnotes.docx 失敗");
        let (items, marks) = footnote_json_items(&engine);
        assert_eq!(items.len(), 1, "一處引用 ⇒ 一條註腳：{items:?}");
        assert_eq!(items[0].0, "1.", "十進位 ⇒ 前綴仍是「1.」");
        assert_eq!(marks, vec!["1".to_string()], "記號字面仍是十進位「1」");
        // A/B：與「移除 w:footnotePr」的同一檔逐位元組相同。
        let stripped = engine_without_footnote_pr(FOOT_DOC);
        assert_eq!(
            all_pages_json(&engine),
            all_pages_json(&stripped),
            "十進位註腳 ⇒ 渲染必須與無設定逐位元組相同"
        );
    }

    /// J1-fix（紅燈）：reader-styles 末頁——尾註接排區必須落在**保留帶**內
    /// （由 content_bottom 往上堆、貼在頁底註腳區上方），且 y 單調不重疊。
    #[test]
    fn reader_styles_endnote_zone_sits_in_reserved_bands() {
        const RS: &[u8] = include_bytes!(
            "../../../corpus/phpword/tests_PhpWordTests__files_documents_reader-styles.docx"
        );
        let engine = Engine::from_docx_bytes(RS).expect("開啟 reader-styles.docx");
        let geo = engine.end_geo().expect("末節幾何");
        let band = engine.params.line_h_pt();
        let body = engine.body_page_count();
        let last = body - 1;
        let page = engine.layout.pages().get(last).expect("正文末頁");
        let mut full = engine.params.clone();
        full.footnote_reserve_pt = 0.0;
        full.footnote_reserves_pt = Vec::new();
        full.endnote_reserves_pt = Vec::new();
        let cap = section_capacity(&full, &geo, page.index);
        let content_bottom = geo.margin_top_pt + cap as f32 * band;
        let foot_pt = engine
            .foot
            .pages
            .get(last)
            .map(|p| p.reserve_pt)
            .unwrap_or(0.0);
        let inline_bands: u32 = engine.end.inline.iter().map(|i| i.bands.max(1)).sum();
        let sep_bands = footnotes::endnote_sep_bands(band);
        let reserve_pt = (sep_bands + inline_bands) as f32 * band;
        let expected_sep_y = content_bottom - foot_pt - reserve_pt;
        let dl = engine.page_display_list(last).expect("正文末頁必須存在");
        // 正文末行 y：取該頁行網格的最大帶（權威來源，不猜顯示列表）。
        let body_y = page
            .line_bands
            .iter()
            .copied()
            .max()
            .map(|b| page.margin_top_pt + b as f32 * band)
            .expect("正文末頁必須有正文行");
        let item_y = dl
            .items
            .iter()
            .find_map(|it| match it {
                DrawItem::Text { text, y, .. } if text == "End note" => Some(*y),
                _ => None,
            })
            .expect("尾註條目文字必須存在");
        // 尾註分隔線＝正文與條目之間那條 Rule（最靠下者）。
        let sep_y = dl
            .items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Rule { y1, .. } if *y1 > body_y && *y1 < item_y => Some(*y1),
                _ => None,
            })
            .fold(f32::MIN, f32::max);
        let footer_y = dl
            .items
            .iter()
            .find_map(|it| match it {
                DrawItem::Text { text, y, .. } if text.starts_with("Page ") => Some(*y),
                _ => None,
            })
            .expect("頁尾欄位必須存在");
        assert!(
            body_y < sep_y && sep_y < item_y && item_y < footer_y,
            "y 必須單調（正文 {body_y} < 分隔線 {sep_y} < 條目 {item_y} < 頁尾 {footer_y}）"
        );
        assert!(
            (sep_y - expected_sep_y).abs() <= band,
            "尾註分隔線必須由保留帶推得：期望 {expected_sep_y}，實得 {sep_y}"
        );
    }

    /// J1-fix (b)：尾註 reserve > 0 且與顯示幾何同一份（`reserves_stable` 語義）。
    #[test]
    fn endnote_reserve_is_positive_and_converged() {
        const RS: &[u8] = include_bytes!(
            "../../../corpus/phpword/tests_PhpWordTests__files_documents_reader-styles.docx"
        );
        let engine = Engine::from_docx_bytes(RS).expect("開啟 reader-styles.docx");
        let band = engine.params.line_h_pt();
        let last = engine.body_page_count() - 1;
        let inline_bands: u32 = engine.end.inline.iter().map(|i| i.bands.max(1)).sum();
        let expected = (footnotes::endnote_sep_bands(band) + inline_bands) as f32 * band;
        let applied = engine
            .params
            .endnote_reserves_pt
            .get(last)
            .copied()
            .unwrap_or(0.0);
        assert!(applied > 0.0, "末頁尾註接排預留必須 > 0，實得 {applied}");
        assert!(
            (applied - expected).abs() < 0.01,
            "預留須等於（分隔線＋接排條目）帶高：期望 {expected}，實得 {applied}"
        );
        // 收斂語義：把同值重算一輪，預留向量等價（不再變動）。
        let mut recomputed = vec![0.0; last + 1];
        recomputed[last] = expected;
        assert!(
            footnotes::reserves_stable(&engine.params.endnote_reserves_pt, &recomputed),
            "尾註預留必須收斂（{:?} vs {:?}）",
            engine.params.endnote_reserves_pt,
            recomputed
        );
    }

    /// J1-fix (c)：無尾註檔零擾動——尾註預留表為空、尾註層全空、頁數不變。
    #[test]
    fn no_endnote_docs_keep_empty_endnote_reserve() {
        const XOPT_NO_NOTES: &[u8] =
            include_bytes!("../../../corpus/xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx");
        const POI_FOOT_ONLY: &[u8] =
            include_bytes!("../../../corpus/poi/test-data_document_footnotes.docx");
        for (name, bytes) in [
            ("xopt/WC049", XOPT_NO_NOTES),
            ("poi/footnotes", POI_FOOT_ONLY),
        ] {
            let engine = Engine::from_docx_bytes(bytes).expect("開啟對照檔");
            assert!(
                engine.params.endnote_reserves_pt.is_empty(),
                "{name}：無尾註 ⇒ 尾註預留表必須為空"
            );
            assert!(
                engine.endnote_layout().is_empty(),
                "{name}：無尾註 ⇒ 尾註層必須全空"
            );
            assert_eq!(
                engine.page_count(),
                engine.body_page_count(),
                "{name}：無尾註 ⇒ 頁數與正文頁數相同"
            );
        }
    }

    /// J1-fix (d)：多條溢出——接排帶裝滿後，續頁存在且**續接線在頁頂**
    /// （置中短線）；接排區仍貼保留帶、與正文不重疊。
    #[test]
    fn endnote_inline_full_then_continuation_rule_at_page_top() {
        let engine = build_endnote_doc(40, "尾註內容。");
        let geo = engine.end_geo().expect("末節幾何");
        let band = engine.params.line_h_pt();
        assert!(
            engine.endnote_page_count() >= 1,
            "接排帶裝滿 ⇒ 必須有純尾註續頁"
        );
        assert!(!engine.end.inline.is_empty(), "末頁必須有接排條目");
        let body = engine.body_page_count();
        let last = body - 1;
        let sep_bands = footnotes::endnote_sep_bands(band);
        let inline_bands: u32 = engine.end.inline.iter().map(|i| i.bands.max(1)).sum();
        let applied = engine
            .params
            .endnote_reserves_pt
            .get(last)
            .copied()
            .unwrap_or(0.0);
        assert!(
            applied >= (sep_bands + inline_bands) as f32 * band - 0.01,
            "接排保留帶不得小於（分隔線＋條目）帶高：{applied}"
        );
        // 續頁第 0 頁：續接線在頁頂（y＝margin_top），且置中（不從版心左緣起）。
        let dl = engine.page_display_list(body).expect("純尾註續頁必須存在");
        let rule = dl.items.iter().find_map(|it| match it {
            DrawItem::Rule { x1, y1, .. } => Some((*x1, *y1)),
            _ => None,
        });
        let (x1, y1) = rule.expect("續頁必須有續接線");
        assert!(
            (y1 - geo.margin_top_pt).abs() < 0.01,
            "續接線必須在頁頂（margin_top={}），實得 {y1}",
            geo.margin_top_pt
        );
        assert!(
            x1 > geo.margin_left_pt,
            "續接線必須置中（不得自版心左緣起），實得 x1={x1}"
        );
    }

    /// J1-fix：末頁正文帶數超過「容量 − 尾註保留帶」時，正文**如實溢出**
    /// 多一頁（比照 F2c）；尾註接排落新末頁，不與正文重疊。
    #[test]
    fn endnote_reserve_pushes_body_to_extra_page_when_last_page_is_full() {
        // SAMPLE 幾何（A4、1 吋邊界、12pt×1.4）一頁容量 41 帶。
        let mut paras = String::new();
        for i in 0..40 {
            paras.push_str(&format!(r#"<w:p><w:r><w:t>正文行 {i}</w:t></w:r></w:p>"#));
        }
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>{paras}<w:p><w:r><w:t>末行</w:t></w:r><w:r><w:endnoteReference w:id="1"/></w:r></w:p></w:body>
</w:document>"#
        );
        let notes = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:endnote w:id="1"><w:p><w:r><w:t xml:space="preserve">尾註內容。</w:t></w:r></w:p></w:endnote>
</w:endnotes>"#;
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        base.set_part("word/document.xml", xml.into_bytes());
        base.set_part("word/endnotes.xml", notes.as_bytes().to_vec());
        let docx = base.save_docx().expect("存回 docx 失敗");
        let engine = Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗");
        let body = engine.body_page_count();
        assert_eq!(
            body, 2,
            "正文 41 行＋尾註保留帶超過單頁容量 ⇒ 必須如實溢出為 2 頁，實得 {body}"
        );
        assert_eq!(engine.endnote_page_count(), 0, "一條短尾註接排新末頁");
        assert!(!engine.end.inline.is_empty(), "尾註必須接排正文末頁");
        let reserve = engine.params.endnote_reserves_pt.iter().sum::<f32>();
        assert!(reserve > 0.0, "溢出案例也必須留下尾註預留，實得 {reserve}");
        // 正文末頁（新頁）幾何：尾註分隔線在正文之後、頁尾之上。
        let last = body - 1;
        let dl = engine.page_display_list(last).expect("正文末頁必須存在");
        let sep_y = dl
            .items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Rule { y1, .. } => Some(*y1),
                _ => None,
            })
            .fold(f32::MIN, f32::max);
        let item_y = dl
            .items
            .iter()
            .find_map(|it| match it {
                DrawItem::Text { text, y, .. } if text == "尾註內容。" => Some(*y),
                _ => None,
            })
            .expect("尾註條目文字必須存在");
        assert!(
            item_y > sep_y,
            "尾註條目必須在分隔線之下：分隔線 {sep_y}、條目 {item_y}"
        );
    }

    /// (a) HC031 實檔：30 個 `w:numPr` 段必須發射清單符號（修前 SVG 符號數＝0）。
    /// 斷言三種實見形態：`%1)` → `1)`、upperRoman → `I.`／`II.`、Symbol
    /// 私用區 F0B7 → `•`。
    #[test]
    fn hc031_list_markers_emitted() {
        const HC031: &[u8] =
            include_bytes!("../../../corpus/xopt/TestFiles_HC031-Complicated-Document.docx");
        let engine = Engine::from_docx_bytes(HC031).expect("開啟 HC031 失敗");
        let texts: Vec<String> = (0..engine.page_count())
            .flat_map(|p| engine.page_display_list(p).expect("頁存在").items)
            .filter_map(|it| match it {
                DrawItem::Text { text, .. } => Some(text),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| t.starts_with("1)")),
            "HC031 應有 %1) 展開的 1) 條目，實際文字：{texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.trim_start().starts_with("I.") || t.trim_start().starts_with("II.")),
            "HC031 應有 upperRoman 展開的 I./II.，實際文字：{texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains('•')),
            "HC031 應有 Symbol F0B7 映射的 •，實際文字：{texts:?}"
        );
    }

    /// 以 sample.docx 為容器灌一份單段正文（可選 numbering.xml）；仿既有
    /// `set_part` 手法，不往 corpus 放新檔。
    fn build_list_doc(p_pr: &str, numbering: Option<&str>, text: &str) -> Engine {
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        let doc = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:pPr>{p_pr}</w:pPr><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p></w:body>
</w:document>"#
        );
        base.set_part("word/document.xml", doc.into_bytes());
        if let Some(n) = numbering {
            base.set_part("word/numbering.xml", n.as_bytes().to_vec());
        }
        let docx = base.save_docx().expect("存回測試 docx 失敗");
        Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗")
    }

    fn list_numbering(ind_left: u32, hanging: u32) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/>
      <w:pPr><w:ind w:left="{ind_left}" w:hanging="{hanging}"/></w:pPr></w:lvl>
  </w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
</w:numbering>"#
        )
    }

    fn para_line_count(engine: &Engine, para: usize) -> usize {
        engine
            .layout
            .pages()
            .iter()
            .flat_map(|p| p.lines.iter())
            .filter(|l| l.para_index == para)
            .count()
    }

    /// (g) 縮排：同文字、同 marker，`ind_left − ind_hanging` 較大者首行可用
    /// 寬確實被扣（斷行位置變；縮排以 marker 前空白近似）。
    #[test]
    fn list_indent_reduces_first_line_width() {
        let text = "Video provides a powerful way to help you prove your point. \
                    When you click Online Video you can paste in the embed code for the video. \
                    You can also type a keyword to search online for the video that best fits your document. \
                    To make your document look professionally produced, Word provides header, footer, \
                    cover page, and text box designs that complement each other.";
        let flat = build_list_doc(
            r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>"#,
            Some(&list_numbering(360, 360)),
            text,
        );
        let indented = build_list_doc(
            r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>"#,
            Some(&list_numbering(1800, 360)),
            text,
        );
        assert!(
            para_line_count(&indented, 0) > para_line_count(&flat, 0),
            "縮排 72pt 後可用寬被扣，行數必須增加（flat={}, indented={}）",
            para_line_count(&flat, 0),
            para_line_count(&indented, 0)
        );
    }

    /// (e) 自製 fixture（numbering.xml 灌 part）：Symbol 的 U+F0B7 在
    /// engine 顯示列表翻成「•」（符號字型證據只認 `w:rFonts@ascii`）。
    #[test]
    fn list_symbol_private_use_maps_in_engine() {
        let numbering = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/>
      <w:lvlText w:val="&#xF0B7;"/>
      <w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr>
      <w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol"/></w:rPr></w:lvl>
  </w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
</w:numbering>"#;
        let engine = build_list_doc(
            r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>"#,
            Some(numbering),
            "項目符號文字",
        );
        let texts: Vec<String> = engine
            .page_display_list(0)
            .expect("第 0 頁存在")
            .items
            .into_iter()
            .filter_map(|it| match it {
                DrawItem::Text { text, .. } => Some(text),
                _ => None,
            })
            .collect();
        assert!(
            texts
                .iter()
                .any(|t| t.contains('•') && !t.contains('\u{f0b7}')),
            "Symbol F0B7 必須映射成 •，實際：{texts:?}"
        );
    }

    /// (h) 零變化：WC049（無清單）發射投影逐段文字＝原始正文，且不得出現
    /// marker（無 numbering.xml／無 numPr ⇒ marker 不插、寬度不扣）。
    #[test]
    fn wc049_without_lists_has_byte_identical_projection() {
        const WC049: &[u8] =
            include_bytes!("../../../corpus/xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx");
        let engine = Engine::from_docx_bytes(WC049).expect("開啟 WC049 失敗");
        let eff: Vec<String> = engine
            .eff_paragraphs()
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect())
            .collect();
        assert_eq!(
            eff,
            engine.doc.paragraph_texts(),
            "無清單檔投影必須逐字不變"
        );
    }

    // ---------------- K5：atLeast 行高自適應（宣告下限 ∨ 段內需求高） ----------------
    //
    // LO ground truth（scripts/render_lab 路徑實測，2026-10-23）：
    // - k5_atleast_grow.docx：`line=240 atLeast`（12pt 下限）＋段內 40pt run
    //   ⇒ LO 行距 48.85pt（40pt 字級主導，LO 吃字型 metrics≈1.22em），3 頁。
    // - k5_exact_guard.docx：同文 `exact` 12pt ⇒ LO 行距恰 12pt、1 頁
    //   （Exact 是硬值，K5 一個字不準動——本測試是防風暴樁）。
    // - k5_atleast_floor.docx：`line=480 atLeast`（24pt 下限）＋ 12pt run
    //   ⇒ LO 行距恰 24pt、1 頁（宣告主導，維持不變）。
    // 架構實態（紅燈實測確立）：顯示層 y 步进＝文件帶尺 grid_line_h（無宣告
    // ＝12×1.4＝16.8pt，本 fixtures 皆無 styles.xml），段行高只決定佔帶數
    // bands＝ceil(段行高/帶尺)。本引擎單倍尺＝字級×1.4（全引擎口徑，字型
    // metrics 表屬後續卡）：AtLeast 需求高＝段內最大有效 run 字級×1.4。
    //   grow：需求 40×1.4＝56 ⇒ bands＝ceil(56/16.8)＝4 ⇒ 行距 4×16.8＝67.2
    //         （LO 48.85；量化殘差 37%，屬精確 pt 垂直度量制範圍，老實記錄）
    //   exact：Exact 12（硬值，不動）⇒ bands＝ceil(12/16.8)＝1 ⇒ 16.8（前後一致）
    //   floor：max(24, 12×1.4)＝24 ⇒ bands＝2 ⇒ 33.6（前後一致）

    const K5_GROW: &[u8] = include_bytes!("../../../fixtures/k5_atleast_grow.docx");
    const K5_EXACT_GUARD: &[u8] = include_bytes!("../../../fixtures/k5_exact_guard.docx");
    const K5_FLOOR: &[u8] = include_bytes!("../../../fixtures/k5_atleast_floor.docx");

    /// 顯示列表 p0 相鄰兩行（同段）的 y 距（pt）；行數不足回 None。
    fn k5_first_line_gap(engine: &Engine) -> Option<f32> {
        let dl = engine.page_display_list(0)?;
        let mut ys: Vec<f32> = dl
            .items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Text { y, text, .. } if !text.trim().is_empty() => Some(*y),
                _ => None,
            })
            .collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        if ys.len() < 2 {
            return None;
        }
        Some(ys[1] - ys[0])
    }

    #[test]
    fn k5_atleast_grow_line_height_follows_large_run() {
        // 紅燈核心：atLeast 12pt 下限＋40pt run ⇒ 段行高必須由大字需求主導
        // （修前 1 帶＝16.8，修後 4 帶＝67.2；LO 48.85，量化殘差見上註）。
        let engine = Engine::from_docx_bytes(K5_GROW).expect("開 k5_atleast_grow 失敗");
        let gap = k5_first_line_gap(&engine).expect("p0 必須有至少兩行");
        assert!(
            (gap - 67.2).abs() < 0.01,
            "atLeast＋40pt run 段行高應=max(12, 40×1.4)=56pt ⇒ ceil(56/16.8)=4 帶＝67.2，實得 {gap}"
        );
    }

    #[test]
    fn k5_atleast_grow_pagination_grows_with_demand() {
        // 修前 1 頁（行高吃 12 的錯值）；修後行高 56pt ⇒ 頁數必須顯著增長
        // （LO 實測 3 頁；我方帶量化下斷行尺與 LO 不同，鎖方向不鎖等值，
        //  精確等值待垂直度量制——見上註量化殘差）。
        let engine = Engine::from_docx_bytes(K5_GROW).expect("開 k5_atleast_grow 失敗");
        assert!(
            engine.page_count() >= 3,
            "需求高 56pt/行、336 詞正文：頁數必須 ≥3（修前 1 頁，LO 3 頁），實得 {}",
            engine.page_count()
        );
    }

    #[test]
    fn k5_exact_guard_unchanged() {
        // Exact 防風暴樁（鎖現態）：Exact 支路 K5 不動 ⇒ 行高硬值 12 ⇒
        // 1 帶、行距＝帶尺 16.8、1 頁——K5 前後必須逐位元組同行為。
        let engine = Engine::from_docx_bytes(K5_EXACT_GUARD).expect("開 exact 失敗");
        assert_eq!(engine.page_count(), 1, "exact 12pt 必須維持 1 頁");
        let gap = k5_first_line_gap(&engine).expect("p0 必須有至少兩行");
        assert!(
            (gap - 16.8).abs() < 0.01,
            "exact 段行高硬值 12 ⇒ 1 帶（帶尺 16.8），實得 {gap}"
        );
    }

    #[test]
    fn k5_atleast_floor_declared_value_wins() {
        // 下限主導態：宣告 24pt > 需求 12×1.4=16.8 ⇒ 行高 24、2 帶＝33.6、
        // 1 頁——K5 前後必須一致（鎖現態防回歸）。
        let engine = Engine::from_docx_bytes(K5_FLOOR).expect("開 floor 失敗");
        assert_eq!(engine.page_count(), 1, "atLeast 24pt 下限檔必須 1 頁");
        let gap = k5_first_line_gap(&engine).expect("p0 必須有至少兩行");
        assert!(
            (gap - 33.6).abs() < 0.01,
            "宣告下限 24pt 大於需求高 ⇒ ceil(24/16.8)=2 帶＝33.6，實得 {gap}"
        );
    }

    // ---------------- K4c-image：像素解析 helper 單元鎖 ----------------

    /// base64（RFC 4648 §4 附錄範例向量）：本卡自製 helper 的正確性鎖。
    #[test]
    fn k4c_base64_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        // PNG magic 全 8 位元組（含 >0x7F 位元組：字節級而非字元級編碼）。
        assert_eq!(
            base64_encode(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
            "iVBORw0KGgo="
        );
    }

    /// 副檔名 → MIME 白名單；EMF/WMF/PDF 等不可渲染格式＝None（退佔位框）。
    #[test]
    fn k4c_mime_whitelist_and_opaque_formats() {
        assert_eq!(
            mime_of_media_path("word/media/image2.png"),
            Some("image/png")
        );
        assert_eq!(mime_of_media_path("word/media/i.JPG"), Some("image/jpeg"));
        assert_eq!(mime_of_media_path("a/b/c.jpeg"), Some("image/jpeg"));
        assert_eq!(mime_of_media_path("x.gif"), Some("image/gif"));
        assert_eq!(mime_of_media_path("x.bmp"), Some("image/bmp"));
        assert_eq!(mime_of_media_path("x.TIFF"), Some("image/tiff"));
        assert_eq!(mime_of_media_path("x.webp"), Some("image/webp"));
        assert_eq!(mime_of_media_path("x.svg"), Some("image/svg+xml"));
        // 本卡不明理解的格式：一律 None（n779834 浮水印 EMF 即此路）。
        assert_eq!(mime_of_media_path("word/media/image1.emf"), None);
        assert_eq!(mime_of_media_path("word/media/image1.wmf"), None);
        assert_eq!(mime_of_media_path("word/media/image1.pdf"), None);
        assert_eq!(mime_of_media_path("noext"), None);
    }

    /// 像素解析決策規則：可渲染＞不可渲染、皆可渲染取先見；懸空 rId／
    /// 查無 rId 跳下一個備選；全部不可渲染＝None。
    #[test]
    fn k4c_resolve_pixel_priority_rules() {
        use quire_ooxml::Relationship;
        let mut parts = BTreeMap::new();
        parts.insert("word/media/image1.pdf".to_string(), vec![1u8; 3]);
        parts.insert("word/media/image2.png".to_string(), vec![2u8; 3]);
        let rels = read_rels(
            br#"<Relationships>
  <Relationship Id="rId1" Target="media/image1.pdf"/>
  <Relationship Id="rId2" Target="media/image2.png"/>
  <Relationship Id="rId3" Target="media/gone.png"/>
</Relationships>"#,
        );
        assert_eq!(rels.len(), 3);
        // Choice→pdf 不可渲染、Fallback→png 可渲染 ⇒ 跳 pdf 取 png（Bug51170）。
        assert_eq!(
            resolve_pixel_source(&parts, &rels, &["rId1", "rId2"]),
            Some(("word/media/image2.png".into(), "image/png".into()))
        );
        // 皆可渲染 ⇒ 取先見（文档序）。
        parts.insert("word/media/image1.pdf".to_string(), vec![1u8; 3]);
        let rels_png2 = read_rels(
            br#"<Relationships>
  <Relationship Id="rIdA" Target="media/image2.png"/>
  <Relationship Id="rIdB" Target="media/a.jpg"/>
</Relationships>"#,
        );
        parts.insert("word/media/a.jpg".to_string(), vec![3u8; 3]);
        assert_eq!(
            resolve_pixel_source(&parts, &rels_png2, &["rIdA", "rIdB"]),
            Some(("word/media/image2.png".into(), "image/png".into()))
        );
        // 懸空 rId（part 不在包內）→ 跳過續找。
        assert_eq!(
            resolve_pixel_source(&parts, &rels, &["rId3", "rId2"]),
            Some(("word/media/image2.png".into(), "image/png".into()))
        );
        // 全部不可渲染／查無 ⇒ None（發射退佔位框）。
        assert_eq!(resolve_pixel_source(&parts, &rels, &["rId1"]), None);
        assert_eq!(resolve_pixel_source(&parts, &rels, &["rIdX"]), None);
        assert_eq!(resolve_pixel_source(&parts, &rels, &[]), None);
        // External（http 目標）不是 part → 跳過（read_rels 已置 None）。
        let rels_ext = read_rels(
            br#"<Relationships><Relationship Id="rIdE" Target="https://e/x.png" TargetMode="External"/></Relationships>"#,
        );
        assert_eq!(resolve_pixel_source(&parts, &rels_ext, &["rIdE"]), None);
        // Relationship 型別經 engine re-export 可用（分層鐵律的通道驗證）。
        let _ = Relationship {
            id: "r".into(),
            target_part: None,
        };
    }

    /// OPC rels 路徑推算：part → 它自家的 `.rels`（HF part 有自家 rels）。
    #[test]
    fn k4c_rels_path_mapping() {
        assert_eq!(
            rels_path_for("word/header1.xml"),
            "word/_rels/header1.xml.rels"
        );
        assert_eq!(
            rels_path_for("word/document.xml"),
            "word/_rels/document.xml.rels"
        );
        assert_eq!(rels_path_for("a.xml"), "_rels/a.xml.rels");
        assert_eq!(
            rels_path_for("customXml/item1.xml"),
            "customXml/_rels/item1.xml.rels"
        );
    }

    // ---------------- M1-band：簇 A 分頁容量（K5-diag §2.6，一卡三席） ----------------
    //
    // 統一根因：級聯段行高／表格列高 ceil 到粗帶（docDefaults 帶尺 12.65／
    // 15.4／16.8pt）→ 15pt 列吃 2 帶＝25.3pt（LO 15.0，×1.69）。處方＝帶尺
    // 細化 ≤2.8pt（宣告級聯時；零宣告檔維持舊帶尺＝零宣告逐位元組不變）。
    // 報告差異點名：K5-diag 說「後備行高固定 12pt×1.4」——實測 doc_grid
    // 級聯已讓 grid_line_h 跟隨 docDefaults 字級（tdf127814 實為 11pt×1.15
    // ＝12.65），真正的過扣來自 ceil 到粗帶本身，非 12pt 常數。

    const M1_TDF127814: &[u8] = include_bytes!("../../../corpus/ooxmlexport/tdf127814.docx");
    const M1_TDF129575: &[u8] =
        include_bytes!("../../../corpus/ooxmlexport/tdf129575-directBefore.docx");
    const M1_RA001: &[u8] =
        include_bytes!("../../../corpus/xopt/TestFiles_RA001-Tracked-Revisions-01.docx");

    /// 頁「可抽行數」＝文字行＋表格列（lm 報告同口徑）。
    fn m1_page_rows(engine: &Engine, page: usize) -> usize {
        let pg = engine.layout.pages().get(page).expect("頁存在");
        pg.lines.len() + pg.tables.iter().map(|t| t.frag.rows.len()).sum::<usize>()
    }

    /// 紅燈 1（tdf127814）：LO 2 頁（p2 裝 28 項表格列）；基線 3 頁。
    #[test]
    fn m1_band_tdf127814_reaches_lo_two_pages() {
        let engine = Engine::from_docx_bytes(M1_TDF127814).expect("開 tdf127814 失敗");
        assert_eq!(
            engine.page_count(),
            2,
            "tdf127814 帶尺細化後應與 LO 同為 2 頁（基線 3 頁：28 列×ceil(15/12.65)×12.65＝25.3/列 撐爆 p2）"
        );
        assert!(
            m1_page_rows(&engine, 1) >= 28,
            "p2 應裝 28 項表格列（LO p2 30 行），實得 {}",
            m1_page_rows(&engine, 1)
        );
    }

    /// 紅燈 2（tdf129575）：LO 3 頁；基線 4 頁（p2 少裝 3 項，整段位移）。
    #[test]
    fn m1_band_tdf129575_reaches_lo_three_pages() {
        let engine = Engine::from_docx_bytes(M1_TDF129575).expect("開 tdf129575 失敗");
        assert_eq!(
            engine.page_count(),
            3,
            "tdf129575 帶尺細化後應與 LO 同為 3 頁（基線 4 頁：列高 46.2pt vs LO pitch 34.0）"
        );
    }

    /// 紅燈 3（RA001）：頁數下降（基線 9）且 p2 可抽行上升（基線 25）。
    /// LO＝6 頁；delCopy 雙發屬簇 B 卡，本卡紅燈只釘「頁數降＋p2 行升」。
    #[test]
    fn m1_band_ra001_pages_drop_and_p2_rows_rise() {
        let engine = Engine::from_docx_bytes(M1_RA001).expect("開 RA001 失敗");
        assert!(
            engine.page_count() < 9,
            "RA001 帶尺細化後頁數應低於基線 9（LO 6），實得 {}",
            engine.page_count()
        );
        assert!(
            m1_page_rows(&engine, 1) > 25,
            "RA001 p2 可抽行應高於基線 25（18 段×33.6pt 粗帶壓扁容量），實得 {}",
            m1_page_rows(&engine, 1)
        );
    }

    /// 零宣告守門（反向紅燈）：無樣式宣告檔 k5_exact_guard（12pt×1.4 後備
    /// 帶尺）帶制不得細化——顯示 y 步進維持 16.8pt（既有 k5 防風暴樁同值）。
    #[test]
    fn m1_band_undeclared_doc_keeps_coarse_fallback() {
        const GUARD: &[u8] = include_bytes!("../../../fixtures/k5_exact_guard.docx");
        let engine = Engine::from_docx_bytes(GUARD).expect("開 k5_exact_guard 失敗");
        let gap = k5_first_line_gap(&engine).expect("p0 必須有至少兩行");
        assert!(
            (gap - 16.8).abs() < 0.01,
            "零宣告檔後備帶尺不得細化：應 1 帶＝16.8pt，實得 {gap}"
        );
    }

    // ---------------- R3：VML 浮動定位消費（fdo76591／n751077） ----------------

    const R3_FDO76591: &[u8] = include_bytes!("../../../corpus/ooxmlexport/fdo76591.docx");
    const R3_N751077: &[u8] = include_bytes!("../../../corpus/ooxmlimport/n751077.docx");

    /// 席1 紅燈（fdo76591）：2→1 頁；三框絕對定位（頁邊距原點＋margin
    /// 偏移）；'BELVIDERE ASSEMBLY PLANT' 落 p0 且只發一次。
    #[test]
    fn r3_fdo76591_vml_absolute_reaches_lo_one_page() {
        let engine = Engine::from_docx_bytes(R3_FDO76591).expect("開 fdo76591 失敗");
        assert_eq!(
            engine.page_count(),
            1,
            "fdo76591 應與 LO 同為 1 頁（基線 2 頁）"
        );
        let pg0 = engine.layout.pages().first().expect("p0 存在");
        assert!(pg0.images.is_empty(), "絕對定位框不得再佔正文流帶");
        assert_eq!(pg0.floating_images.len(), 3, "三個 VML 框全數浮動附掛");
        // LO 實測：框1 (256.05,590.45)、框2 (193.8,729.2)、框3 (22.05,18.2)。
        let got: Vec<(f32, f32, f32, f32)> = pg0
            .floating_images
            .iter()
            .map(|f| (f.x_pt, f.y_pt, f.w_pt, f.h_pt))
            .collect();
        for (want_x, want_y, want_w, want_h) in [
            (256.05f32, 590.45f32, 99.95f32, 100.2f32),
            (193.8, 729.2, 225.0, 45.0),
            (22.05, 18.2, 567.0, 733.65),
        ] {
            assert!(
                got.iter().any(|(x, y, w, h)| (x - want_x).abs() < 0.1
                    && (y - want_y).abs() < 0.1
                    && (w - want_w).abs() < 0.1
                    && (h - want_h).abs() < 0.1),
                "缺 LO 真值框 ({want_x},{want_y},{want_w},{want_h})；實得 {got:?}"
            );
        }
        let dl = engine.page_display_list(0).expect("p0 顯示列表");
        let bel = dl
            .items
            .iter()
            .filter(|it| matches!(it, DrawItem::Text { text, .. } if text.contains("BELVIDERE ASSEMBLY PLANT")))
            .count();
        assert_eq!(bel, 1, "BELVIDERE 應落 p0 且只發一遍");
        assert!(
            dl.items
                .iter()
                .any(|it| matches!(it, DrawItem::Text { text, .. } if text.contains("3-23-2011"))),
            "末行日期應同在 p0"
        );
    }

    /// 席2 紅燈（n751077）：3→2 頁；TEXT1-9 與圖同落 p1（我方的 p0）；
    /// 頁面釘定群組掛「所屬 section 起始頁」而非錨定段落流頁。
    #[test]
    fn r3_n751077_page_pinned_group_reaches_lo_two_pages() {
        let engine = Engine::from_docx_bytes(R3_N751077).expect("開 n751077 失敗");
        assert_eq!(
            engine.page_count(),
            2,
            "n751077 應與 LO 同為 2 頁（基線 3 頁）"
        );
        let pg0 = engine.layout.pages().first().expect("p0 存在");
        // p0 另有段 0 的 DrawingML 錨定圖（66.6pt 寬）；群組以 611.95pt 寬辨識。
        let grp = pg0
            .floating_images
            .iter()
            .find(|f| (f.w_pt - 611.95).abs() < 0.1)
            .expect("頁面釘定群組應掛在節首頁");
        assert_eq!(grp.para, 1, "浮動群組宿主段落＝含 sectPr 的段 1");
        assert!(
            pg0.lines.iter().any(|l| l.para_index == 1),
            "宿主段落應留在節首頁（para-level sectPr 的 break 前內容留原頁）"
        );
        let dl = engine.page_display_list(0).expect("p0 顯示列表");
        for i in 1..=9 {
            let want = format!("TEXT{i}");
            assert!(
                dl.items
                    .iter()
                    .any(|it| matches!(it, DrawItem::Text { text, .. } if text.contains(&want))),
                "{want} 應落 p1（我方 p0）"
            );
        }
        assert!(
            dl.items
                .iter()
                .any(|it| matches!(it, DrawItem::Image { .. })),
            "段 0 的浮動圖應同在 p0"
        );
        assert!(
            engine
                .layout
                .pages()
                .get(1)
                .is_some_and(|pg| pg.floating_images.is_empty()),
            "第 2 頁不得再掛頁面釘定群組"
        );
    }
}
