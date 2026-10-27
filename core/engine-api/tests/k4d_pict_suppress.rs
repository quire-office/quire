//! K4d（E2）：錯置 `w:pict` 抑制——LO 繪製開關唯一＝「pict 是否被 `w:r` 包住」。
//!
//! 紅燈口徑（k5-diag §1.3 變體實驗＋§1.5 處方）：
//! - sdt_vml_textbox 的 `w:pict` 直接掛 `w:p`（錯置 OOXML）→ LO 整棵忽略
//!   （dr=0、框內字不入文字流）；我方修前多發一框（Rect）＋四邊 Rule＋
//!   框內文字（基線對照：Rect fill=0xFFFFFFCC ×1、Rule 0xFF80808C ×4、
//!   Text「VML 文字框內容」×1）。修後三者必須全數消失。
//! - 防過抑回塞（紅燈 2）：`K4D_BUGGY=1` 時閂退化為「run 與非 run 都採納」
//!   → 本檔紅燈 1 必須轉紅（照 L2_BUGGY 回塞鎖先例，實測見提交說明）。
//! - 綠燈不動：合法包 run 的 textbox（`k4e2_txbx_emit` 測試群）、K4a/K4c
//!   header 圖、Bug51170 AlternateContent 路徑——由 corpus 全量 dump A/B
//!   對照（400 檔逐檔 diff）承接。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

/// sdt_vml_textbox 錯置框的填色（fillcolor="#FFFFCC" → ARGB）。
const MISPLACED_FILL: u32 = 0xFF_FF_FF_CC;
/// 錯置框四邊 Rule 的描邊色（strokeweight=1pt 預設灰）。
const MISPLACED_STROKE: u32 = 0xFF_80_80_8C;

fn load_sdt_vml() -> Engine {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/selfgen/sdt_vml_textbox.docx"
    ))
    .expect("讀 corpus sdt_vml_textbox");
    Engine::from_docx_bytes(&bytes).expect("open")
}

/// 紅燈 1：錯置 pict 的 Rect/Rule/框內文字**全數**從顯示列表消失；
/// 正文三句（宿主段文字）與頁數零擾動。
#[test]
fn sdt_vml_textbox_misplaced_pict_fully_suppressed() {
    let eng = load_sdt_vml();
    assert_eq!(eng.page_count(), 1, "單頁檔：抑制不動分頁");
    let mut fills = Vec::new();
    let mut strokes = Vec::new();
    let mut texts = Vec::new();
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        for it in &dl.items {
            match it {
                DrawItem::Rect { fill_argb, .. } => fills.push(*fill_argb),
                DrawItem::Rule { color_argb, .. } => strokes.push(*color_argb),
                DrawItem::Text { text, .. } => texts.push(text.clone()),
                DrawItem::Image { .. } => {}
            }
        }
    }
    assert!(
        !fills.contains(&MISPLACED_FILL),
        "錯置 pict 的框 Rect（fill=0xFFFFFFCC）必須消失，實際 fills={fills:?}"
    );
    assert!(
        !strokes.contains(&MISPLACED_STROKE),
        "錯置 pict 的四邊 Rule 必須消失，實際 strokes={strokes:?}"
    );
    assert!(
        !texts.iter().any(|t| t.contains("VML 文字框內容")),
        "框內文字不得再發射（LO dr=0 且不進文字流），實際 texts={texts:?}"
    );
    // 宿主正文零擾動：三句照發（抑制的是圖形，不是段落）。
    for want in ["內容控件與舊式圖形", "此為純文字內容控件的值。", "下方為"]
    {
        assert!(
            texts.iter().any(|t| t.contains(want)),
            "正文句「{want}」必須保留，實際 texts={texts:?}"
        );
    }
}

/// 抑制不得改段落口徑：宿主段（pict 段）仍佔正文槽——修前後頁顯示列表
/// 的**段落數**零位移（四解析器同口徑鐵律：閂只掐圖形，不動 p_count）。
/// 基線對照（dump-base）：該檔正文段＝4（含錯置 pict 宿主空段）。
#[test]
fn suppression_keeps_paragraph_slots() {
    let eng = load_sdt_vml();
    let dl = eng.page_display_list(0).expect("頁存在");
    // 正文 Text 項（含宿主空段）：修前 5＋框內字 1＋空段 1，修後框內字消失、
    // 宿主空段保留——段落計數口徑以**非空正文句**為準，框內字不佔正文段
    // （K4e2 已分離）。此斷言鎖「正文句集」不因閂多吞/少吞。
    let host: Vec<String> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { text, .. } if !text.is_empty() => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        host.len(),
        3,
        "正文非空句恰為 3（框內字已隨錯置 pict 抑制），實際 {host:?}"
    );
}
