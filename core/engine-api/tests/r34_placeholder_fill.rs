//! R34 紅燈（S2）：表內無填充框根的佔位殘留——幽靈 Rect 與灰色佔位框線。
//!
//! # 缺陷（agy 審查＋PM `quire-cli page` 實跑坐實，基線 main@9d0407e）
//! R32 把表內文字框根的填色從佔位灰（0xFFF0F0F0）改成全透明（0x0000_0000），
//! 消滅了灰帶遮蓋，但留下兩處 LO 沒有的墨跡／幽靈：
//! 1. 同函數後續的預設邊框發射未攔截——`stroke_argb` 缺位時照舊退
//!    `IMAGE_PLACEHOLDER_EDGE`（0xFFCCCCCC ×4 條 0.75pt），Textbox3 外圍
//!    (72.0,158.5)-(459.0,343.5) 殘留 4 條灰色佔位框線（LO 真值＝無任何框線）。
//! 2. alpha-0 的幽靈 Rect 仍留在顯示列表：native SVG 端照常輸出
//!    `<rect fill-opacity="0.000">`——透明≠不存在，SVG 仍 hit-testable。
//!
//! # LO 真值（pymupdf，`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` p0）
//! - Textbox2（273.6×110.55）與 Textbox3 群組根（387×185）：**無底色、無框線**
//!   （LO 只有 Textbox2 的兩條藍線 y99.2/132.9、Textbox3 的子件白框與藍塊）。
//! - 全頁任何位置都不得出現 alpha-0 Rect 或 0xFFCCCCCC 佔位 Rule。
//!
//! # 紅燈規格（先紅後綠；本檔斷言在修前基線必紅）
//! - 燈 1：WC048 p0 零 alpha-0 Rect＋零 0xFFCCCCCC Rule（修前 2＋4，必紅）。
//! - 燈 2（防假綠）：LO 該有的可見圖元必須還在——Textbox2 兩條藍線、
//!   Textbox3 子件白框（387×98.5）與藍塊、Textbox1 黑框四邊。若把整個
//!   `emit_image_shape` 一刀掐掉（錯誤修法）本燈必紅。
//! - 燈 3（零擾動鎖）：真・佔位場景（n779834 EMF 浮水印，查無可渲染像素）
//!   照舊發灰底 Rect＋四條佔位 Rule——誠實近似不得被本卡閂誤殺。
//! - 燈 4：SVG 兜底閂（alpha-0 Rect 不落 SVG）在 native 殼單元測試
//!   `shells/native/tests/render_svg_alpha_gate.rs`（本檔不依賴 native 殼）。
//! - 燈 5（**#[ignore]，待 D5 落地解鎖**）：fdo76591 頂部块樣式真值。S1
//!   根因＝框內 `w:tab` 停靠點定義被誤轉偽造前導空格 → `txbx_runs_for`
//!   配對全滅（core/ooxml 面，PM 已裁定移交 D5 t_ed8bbd01，本卡禁碰）。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

const WC048: &str = "xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx";
/// R32 幽靈口徑：alpha-0 的透明 Rect（R34 起發射層不得再產出）。
const GHOST_ALPHA: u32 = 0x0000_0000;
/// K4c 佔位邊（IMAGE_PLACEHOLDER_EDGE）：未宣告描邊的兜底淺灰。
const PLACEHOLDER_EDGE: u32 = 0xFFCC_CCCC;
/// K4c 佔位灰（IMAGE_PLACEHOLDER_FILL）：查無像素時的誠實底色。
const PLACEHOLDER_GRAY: u32 = 0xFFF0_F0F0;
const LO_BLUE: u32 = 0xFF44_72C4;

/// p0 全項掃描：(alpha-0 Rect 數, 0xFFCCCCCC Rule 數, 佔位灰 Rect 數)。
fn scan(dl: &quire_render_trait::PageDisplayList) -> (Vec<String>, Vec<(f32, f32, f32, f32)>, usize) {
    let mut ghosts = Vec::new();
    let mut edges = Vec::new();
    let mut grays = 0usize;
    for it in &dl.items {
        match it {
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } => {
                if *fill_argb == GHOST_ALPHA || (*fill_argb >> 24) == 0 {
                    ghosts.push(format!("({x:.1},{y:.1},{w:.1},{h:.1})"));
                } else if *fill_argb == PLACEHOLDER_GRAY {
                    grays += 1;
                }
            }
            DrawItem::Rule {
                x1,
                y1,
                x2,
                y2,
                color_argb,
                ..
            } if *color_argb == PLACEHOLDER_EDGE => {
                edges.push((*x1, *y1, *x2, *y2));
            }
            _ => {}
        }
    }
    (ghosts, edges, grays)
}

/// 燈 1：WC048 p0 零幽靈 Rect＋零灰色佔位框線（LO 真值＝textbox 區零墨跡）。
///
/// 修前（main@9d0407e）實測：幽靈 Rect 2 個（Textbox2 根 273.6×110.55、
/// Textbox3 根 387×185）＋ 0xFFCCCCCC Rule 4 條（Textbox3 外圍四邊）。
#[test]
fn wc048_no_ghost_rect_and_no_placeholder_rules() {
    let eng = load_corpus(WC048);
    let dl = eng.page_display_list(0).expect("p0 存在");
    let (ghosts, edges, _grays) = scan(&dl);
    assert!(
        ghosts.is_empty(),
        "燈1：p0 不得有任何 alpha-0 幽靈 Rect（LO Textbox2/3 無底色也不留幽靈；\
         SVG 端 alpha-0 rect 仍 hit-testable），實得 {} 個：{ghosts:?}",
        ghosts.len()
    );
    assert!(
        edges.is_empty(),
        "燈1：p0 不得有任何 0xFFCCCCCC 佔位框線（LO Textbox3 無框線；R32 只改\
         填色未攔截邊框臂），實得 {} 條：{edges:?}",
        edges.len()
    );
}

/// 燈 2（防假綠）：消滅佔位不得連坐可見圖元。LO 真值四組必須逐組還在：
/// - Textbox2 兩條藍線（y99.2／132.9，色 0xFF4472C4，x1≈171.5）
/// - Textbox3 子件白框 Rect（387×98.5，與原根框同原點 72.0,158.5）
/// - Textbox3 子件藍塊 Rect（281.4×2.3，y≈341.2）
/// - Textbox1 黑框四邊（0xFF000000×0.75pt）
///
/// 意圖：若實作把 `emit_image_shape` 整支短路（或把群組子件一起掐掉），
/// 燈 1 會綠、本燈必紅。
#[test]
fn wc048_visible_primitives_survive() {
    let eng = load_corpus(WC048);
    let dl = eng.page_display_list(0).expect("p0 存在");
    let (mut blue_lines, mut white_child, mut blue_block, mut black_edges) = (0, 0, 0, 0);
    for it in &dl.items {
        match it {
            DrawItem::Rule {
                x1,
                y1,
                x2,
                y2,
                color_argb,
                width_pt,
                ..
            } => {
                let horizontal = (y1 - y2).abs() < 0.01;
                let vertical = (x1 - x2).abs() < 0.01;
                if *color_argb == LO_BLUE && horizontal && (*y1 - 99.2).abs() < 1.0 {
                    blue_lines += 1;
                }
                if *color_argb == LO_BLUE && horizontal && (*y1 - 132.9).abs() < 1.0 {
                    blue_lines += 1;
                }
                if *color_argb == 0xFF00_0000 && (*width_pt - 0.75).abs() < 0.01 {
                    // Textbox1 黑框：x=72.0 或 257.9 的豎邊 ＋ y=86.4/197.0 的橫邊。
                    if vertical && ((x1 - 72.0).abs() < 1.0 || (x1 - 257.9).abs() < 1.0) {
                        black_edges += 1;
                    }
                    if horizontal && ((y1 - 86.4).abs() < 1.0 || (y1 - 197.0).abs() < 1.0) {
                        black_edges += 1;
                    }
                }
            }
            DrawItem::Rect { w, h, .. } => {
                if (*w - 387.0).abs() < 1.0 && (*h - 98.5).abs() < 1.0 {
                    white_child += 1;
                }
                if (*w - 281.4).abs() < 1.0 && (*h - 2.3).abs() < 1.0 {
                    blue_block += 1;
                }
            }
            _ => {}
        }
    }
    assert_eq!(blue_lines, 2, "燈2：Textbox2 的兩條 LO 藍線必須仍在");
    assert_eq!(white_child, 1, "燈2：Textbox3 子件白框（387×98.5）必須仍在");
    assert_eq!(blue_block, 1, "燈2：Textbox3 子件藍塊（281.4×2.3）必須仍在");
    assert_eq!(black_edges, 4, "燈2：Textbox1 黑框四邊必須仍在");
}

/// 燈 3（零擾動鎖）：真・佔位場景不受本卡閂影響。
///
/// n779834 頁首浮水印是 EMF（不可渲染）——K4c 誠實契約要求照舊發灰底
/// Rect（0xFFF0F0F0）＋四條 0xFFCCCCCC 佔位 Rule。R34 的兩個閂只吃
/// 「表內文字框／群組根＋填色描邊皆未宣告」一條面，純圖片佔位零擾動。
#[test]
fn emf_watermark_placeholder_untouched() {
    let eng = load_corpus("ooxmlexport/n779834.docx");
    let mut gray_rects = 0usize;
    let mut edge_rules = 0usize;
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        let (ghosts, edges, grays) = scan(&dl);
        assert!(
            ghosts.is_empty(),
            "n779834 p{p} 本就不該有幽靈 Rect，實得 {ghosts:?}"
        );
        gray_rects += grays;
        edge_rules += edges.len();
    }
    assert!(
        gray_rects > 0 && edge_rules >= 4,
        "燈3：EMF 誠實佔位（灰底 Rect＋四邊淺灰 Rule）不得被 R34 閂誤殺，\
         實得 灰 Rect={gray_rects} 佔位 Rule={edge_rules}"
    );
}

/// 燈 5（待解鎖）：fdo76591 頂部块樣式真值——**S1 歸 D5（t_ed8bbd01）**。
///
/// 根因在 core/ooxml：VML 框內 `<w:pPr><w:tabs><w:tab w:val=… w:pos=…/>`
/// 停靠點定義被 drawing.rs 無條件轉成偽造前導空格（' '）進 `txbx_texts`，
/// 與 lib.rs 側（不轉）口徑不一致 → `txbx_runs_for` 字串配對恆 false →
/// 整個文字框樣式退化成 12pt／無粗體／無字型。PM 裁定 tab 口徑屬 core/ooxml
/// 面、與 D5 撞車，本卡（engine-api＋render_svg 白名單）禁碰 core/ooxml。
/// D5 合併後由 PM 解除本 `#[ignore]` 重跑；斷言值取自基線 a6dadee／LO 實測
/// （PM A/B 腳本 ow-style-ab.py 坐實的 7 條回退主角）。
#[test]
#[ignore = "S1 屬 D5 t_ed8bbd01（core/ooxml tab 口徑）；D5 合併後解 ignore 重跑"]
fn fdo76591_top_block_style_matches_lo_truth() {
    let eng = load_corpus("ooxmlexport/fdo76591.docx");
    let dl = eng.page_display_list(0).expect("p0 存在");
    let mut notice = None;
    let mut overtime = None;
    for it in &dl.items {
        if let DrawItem::Text {
            text,
            size_pt,
            flags,
            family,
            ..
        } = it
        {
            // 偽造前導空格口徑：斷言前不 trim——修態行首必須零偽造空格。
            if text.trim() == "IMPORTANT NOTICE" {
                notice = Some((text.clone(), *size_pt, *flags, family.clone()));
            }
            if text.trim() == "Scheduled Saturday Overtime" {
                overtime = Some((text.clone(), *size_pt, *flags, family.clone()));
            }
        }
    }
    let (t1, s1, f1, _fam1) = notice.expect("燈5：p0 必須有 IMPORTANT NOTICE");
    assert!(
        !t1.starts_with(' '),
        "燈5：行首不得有偽造空格（S1 修前='      IMPORTANT NOTICE'），實得 {t1:?}"
    );
    assert!(
        s1 >= 16.0 && (f1 & quire_render_trait::FLAG_BOLD) != 0,
        "燈5：IMPORTANT NOTICE 應 ≥16pt 且粗體（LO/基線 a6dadee 實測），實得 sz={s1} flags={f1}"
    );
    let (t2, s2, f2, _fam2) = overtime.expect("燈5：p0 必須有 Scheduled Saturday Overtime");
    assert!(
        !t2.starts_with(' '),
        "燈5：行首不得有偽造空格（S1 修前=6 個前導空格），實得 {t2:?}"
    );
    assert!(
        s2 >= 18.0 && (f2 & quire_render_trait::FLAG_BOLD) != 0,
        "燈5：Scheduled Saturday Overtime 應 ≥20pt 粗體 Arial（LO 實測），實得 sz={s2} flags={f2}"
    );
}
