//! K24 紅燈：DrawingML `wps:style` 樣式矩陣（fillRef/lnRef）填色落地。
//!
//! # 缺陷（P2 基線報告 docs/p2-scoreboard-202611.md §2†／§5-1）
//! `corpus/poi/test-data_document_shapes-with-text.docx`：LO 渲染有 ~7.1% 彩色
//! 覆蓋（ellipse 實心填色 #156082，54728pt²）；我方渲染黑白（色分 0.000）。
//!
//! # 根因（實測，非卡面假說）
//! 卡面假說「裸 e:ellipse 的 a:solidFill 未進渲染鏈」**不成立**：該檔全部
//! 11 個 `wps:wsp`（ellipse／rect／triangle／rightArrow／4×star5／群組子形）
//! 的 `wps:spPr` **根本沒有 a:solidFill**——色源在 `wps:style` 樣式矩陣：
//!
//! ```text
//! <wps:style>
//!   <a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="15000"/></a:schemeClr></a:lnRef>
//!   <a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef>…
//! </wps:style>
//! ```
//!
//! theme1.xml accent1=#156082；同檔 VML Fallback 快取欄
//! `fillcolor="#156082 [3204]" strokecolor="#030e13 [484]"` 逐位吻合
//! （030e13＝每通道 floor(c×0.15)——Word 自己算完寫下的快取真值）。
//! 全 repo 舊代碼 grep `fillRef` 零命中＝樣式矩陣解析鏈從未存在。
//!
//! # 紅燈規格
//! 1. 燈1（真檔）：shapes-with-text p0 顯示列表出現 #156082 實心填色圖元
//!    （面積>0；LO ellipse 1857375×876300 EMU = 146.3×69.0pt ≈ 10094pt²，
//!    我方以 Rect 發射——色軸按色直方圖計，橢圓形狀差留口徑，卡面白名單
//!    不含 render-trait/shells，不擴權）。群組子形（star5 等）fillRef 同落。
//! 2. 燈2（合成最小檔）：一個 ellipse solidFill srgbClr=FF0000 →
//!    `InlineImage.fill_argb == 0xFFFF0000`（走 spPr 直路，證明管線端到端
//!    通＋spPr 優先序壓矩陣）。
//! 3. 燈2b（合成）：無 spPr solidFill、僅 fillRef idx=1 schemeClr accent1 →
//!    fill_argb == theme accent1；fillRef idx=0 → None（明確「無」）。
//! 4. 燈3（零回退）：K12 WC048 三藍圖元既有行為逐位不動（spPr solidFill 檔
//!    序列化零位移——本卡只改三者優先序全缺時的退化路徑）。
//! 5. 燈4：lnStyleLst 兜底——fillRef 有色時 Rect 色＝fillRef 色（描邊走
//!    Rule 既有鏈，lnRef 色進 stroke_argb）。
//! 6. `K24_STYLE_BUGGY=1` → 燈1/燈2 必紅（回塞鎖）。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

const SHAPES: &str = "poi/test-data_document_shapes-with-text.docx";
/// theme accent1（shapes-with-text 的 theme1.xml 實測值）。
const ACCENT1: u32 = 0xFF15_6082;

/// 燈1：shapes-with-text p0 必須發射 #156082 實心填色圖元（面積>0）。
/// 修前現況：0 個（全黑白）→ 必紅。
#[test]
fn k24_lamp1_shapes_with_text_accent1_fill() {
    let eng = load_corpus(SHAPES);
    let mut accent_rects = Vec::new();
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        for it in &dl.items {
            if let DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } = it
            {
                if *fill_argb == ACCENT1 && *w > 0.0 && *h > 0.0 {
                    accent_rects.push((*x, *y, *w, *h));
                }
            }
        }
    }
    assert!(
        !accent_rects.is_empty(),
        "shapes-with-text 必須發射 #{ACCENT1:06X}（theme accent1）實心填色圖元（LO 7.1% 彩色覆蓋源）；修前 0 個"
    );
    // LO 真值：ellipse 1857375×876300 EMU = 146.25×69.0pt（±3pt 容差）。
    let ellipse = accent_rects
        .iter()
        .any(|(_, _, w, h)| (w - 146.25).abs() < 3.0 && (h - 69.0).abs() < 3.0);
    assert!(
        ellipse,
        "必須含 ellipse 尺寸級色塊 146.3×69.0±3pt；實測 {accent_rects:?}"
    );
}

/// 燈2（合成最小檔）：`fixtures/k24_minimal_style_fill.docx`（zipfile 手搓，
/// 生成腳本見 docs/k24-ellipse-style-fill.md；不在 scripts/ 白名單外掛）。
/// 兩形：ellipse 無 spPr solidFill、fillRef idx=1 schemeClr accent1(=FF0000
/// 合成 theme) → 必須發 0xFFFF0000 填充 Rect；rect spPr solidFill 00FF00 壓
/// fillRef accent2(=0000FF) → 必須發 0xFF00FF00（優先序對照組）。
#[test]
fn k24_lamp2_synthetic_minimal_fixture() {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/").to_string()
        + "k24_minimal_style_fill.docx";
    let bytes = std::fs::read(&full).expect("讀 k24_minimal_style_fill.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("open fixture");
    let dl = eng.page_display_list(0).expect("p0 存在");
    let fills: Vec<u32> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Rect {
                w, h, fill_argb, ..
            } if *w > 0.0 && *h > 0.0 => Some(*fill_argb),
            _ => None,
        })
        .collect();
    assert!(
        fills.contains(&0xFFFF_0000),
        "ellipse fillRef idx=1 accent1(FF0000) 必須發對應 fill 圖元；實測 {fills:08X?}"
    );
    assert!(
        fills.contains(&0xFF00_FF00),
        "spPr solidFill 00FF00 必須壓過 fillRef accent2（優先序）；實測 {fills:08X?}"
    );
    assert!(
        !fills.contains(&0xFF00_00FF),
        "fillRef accent2(0000FF) 被 spPr 壓制後不得出現；實測 {fills:08X?}"
    );
}

/// 燈3：K12 群組內填色既有行為零回退——WC048 三藍圖元照舊（spPr solidFill
/// 檔序列化零位移鎖）。
#[test]
fn k24_lamp3_wc048_k12_zero_regression() {
    let eng = load_corpus("xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx");
    let dl = eng.page_display_list(0).expect("p0 存在");
    let want = 0xFF44_72C4u32;
    let rules = dl
        .items
        .iter()
        .filter(|it| matches!(it, DrawItem::Rule { color_argb, .. } if *color_argb == want))
        .count();
    let rects = dl
        .items
        .iter()
        .filter(|it| matches!(it, DrawItem::Rect { fill_argb, .. } if *fill_argb == want))
        .count();
    assert_eq!(rules, 2, "WC048 兩條藍描邊 Rule 必須逐位照舊（K12 口徑）");
    assert_eq!(rects, 1, "WC048 一條藍實心 Rect 必須逐位照舊（K12 口徑）");
}
