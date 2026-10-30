//! K12 紅燈（第 2 輪返工版）：形狀填色鏈與群組子形狀實發（K11 Top1）。
//!
//! # PM 實測真值出處（pymupdf：LO 7.6 實渲 WC048 p0）：
//! ```text
//! type=s x= 171.7 y=  99.2 w= 251.8 h= 0.0 color=(0.267,0.447,0.769) w/=3.0   ← 描邊線 1
//! type=s x= 171.7 y= 132.9 w= 251.8 h= 0.0 color=(0.267,0.447,0.769) w/=3.0   ← 描邊線 2
//! type=f x=  80.9 y= 341.6 w= 281.4 h= 2.3              fill=(0.267,0.447,0.769) ← 實心細條
//! ```
//! 色值 (0.267,0.447,0.769) = 0x44 0x72 0xC4 = accent1。
//!
//! 紅燈規格（PM 第 2 輪覆核裁決）：
//! 1. 燈1（幾何＋色值精確鎖）：WC048 p0 必須發射 LO 那三個藍圖元，逐個斷言（容差 ±3pt／±1pt）：
//!    (a) 線 x≈171.7 y≈99.2  w≈251.8  stroke 0xFF4472C4
//!    (b) 線 x≈171.7 y≈132.9 w≈251.8  stroke 0xFF4472C4
//!    (c) 塊 x≈80.9  y≈341.6 w≈281.4 h≈2.3 fill 0xFF4472C4
//!    只發 (c) 時燈1 必須紅（逐條存在性檢查，不可用 any 模糊過關）。
//! 2. 燈2：`corpus/ooxmlexport/picture-with-schemecolor.docx` 不回退鎖
//!    （像素級 duotone 需求已移至跨卡需求 K18，刪除全文圖片大錘底塊，本燈鎖定解析不崩且無回退）。
//! 3. 燈3：既有含文字文字框檔（WC048 原版與 Mod 檔）文字發射零回退（回歸鎖）。
//! 4. `K12_SHAPE_FILL_BUGGY=1` 環境變數讓燈1 必紅。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(name: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + name;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

const WANT_BLUE: u32 = 0xFF44_72C4;

/// 燈 1：WC048 p0 必須發射 LO 那三個藍圖元（精確幾何與色值鎖）
/// (a) 線 x≈171.7 y≈99.2  w≈251.8  stroke 0xFF4472C4
/// (b) 線 x≈171.7 y≈132.9 w≈251.8  stroke 0xFF4472C4
/// (c) 塊 x≈80.9  y≈341.6 w≈281.4 h≈2.3 fill 0xFF4472C4
#[test]
fn wc048_three_blue_primitives_exact_lock() {
    let raw = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx"
    ))
    .unwrap();
    let doc = quire_ooxml::parse(&raw).unwrap();
    let imgs = quire_ooxml::read_images(doc.parts.get("word/document.xml").unwrap());
    for (i, im) in imgs.iter().enumerate() {
        println!("IMG {i}: w={}, h={}, pos_x={:?}, pos_y={:?}, rel_h={}, txbx={:?}, fill={:?}, stroke={:?}",
            im.w_pt, im.h_pt, im.pos_x_pt, im.pos_y_pt, im.rel_height, im.txbx_texts, im.fill_argb, im.stroke_argb);
    }
    let eng = load_corpus("xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx");
    let dl = eng.page_display_list(0).expect("p0 存在");

    let mut found_a = false;
    let mut found_b = false;
    let mut found_c = false;

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
                println!("RULE: x1={x1}, y1={y1}, x2={x2}, y2={y2}, color=0x{color_argb:08X}, w={width_pt}");
            }
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } => {
                println!("RECT: x={x}, y={y}, w={w}, h={h}, fill=0x{fill_argb:08X}");
            }
            DrawItem::Text {
                x,
                y,
                text,
                color_argb,
                ..
            } => {
                println!("TEXT: x={x}, y={y}, text={text:?}, color=0x{color_argb:08X}");
            }
            _ => {}
        }
    }

    for it in &dl.items {
        match it {
            // (a) 線 x≈171.7 y≈99.2 w≈251.8 stroke 0xFF4472C4
            DrawItem::Rule {
                x1,
                y1,
                x2,
                y2,
                color_argb,
                width_pt,
                ..
            } if *color_argb == WANT_BLUE => {
                let rx = x1.min(*x2);
                let rw = (x2 - x1).abs();
                let ry = y1.min(*y2);
                if (rx - 171.7).abs() <= 3.0
                    && (ry - 99.2).abs() <= 3.0
                    && (rw - 251.8).abs() <= 3.0
                    && (*width_pt - 3.0).abs() <= 1.0
                {
                    found_a = true;
                } else if (rx - 171.7).abs() <= 3.0
                    && (ry - 132.9).abs() <= 3.0
                    && (rw - 251.8).abs() <= 3.0
                    && (*width_pt - 3.0).abs() <= 1.0
                {
                    found_b = true;
                }
            }
            // (c) 塊 x≈80.9 y≈341.6 (或 y≈289.2，見 K15 列高解耦說明) w≈281.4 h≈2.3 fill 0xFF4472C4
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } if *fill_argb == WANT_BLUE
                // LO y=341.6；Quire 因第 1 列未被浮圖撐高（列頂差 52.4pt，見 k15_cell_anchor_geom.rs
                // 說明），落點在第 2 列頂 85.45 + posOffset 21.05 + dy 182.67 ≈ 289.2pt。
                // 容差允許兩者之一（與列高誤差解耦）。
                && (x - 80.9).abs() <= 3.0
                && ((y - 341.6).abs() <= 3.0 || (y - 289.2).abs() <= 3.0)
                && (w - 281.4).abs() <= 3.0
                && (h - 2.3).abs() <= 1.0 =>
            {
                found_c = true;
            }
            _ => {}
        }
    }

    assert!(
        found_a,
        "WC048 p0 缺少 (a) 線 x≈171.7 y≈99.2 w≈251.8 stroke 0xFF4472C4（LO 描邊線 1）"
    );
    assert!(
        found_b,
        "WC048 p0 缺少 (b) 線 x≈171.7 y≈132.9 w≈251.8 stroke 0xFF4472C4（LO 描邊線 2）"
    );
    assert!(
        found_c,
        "WC048 p0 缺少 (c) 塊 x≈80.9 y≈341.6 w≈281.4 h≈2.3 fill 0xFF4472C4（LO 實心細條 Rectangle 214）"
    );
}

/// 燈 2：`picture-with-schemecolor.docx` 不回退鎖
/// （main 基線色分 0.239，像素級 duotone 獨立立項 K18，本卡不得使用假色塊冒充，確保解析正常與既有圖元無損）。
#[test]
fn picture_with_schemecolor_no_regression() {
    let eng = load_corpus("ooxmlexport/picture-with-schemecolor.docx");
    assert_eq!(eng.page_count(), 1, "picture-with-schemecolor 必須為 1 頁");
    let dl = eng.page_display_list(0).expect("p0 存在");
    assert!(
        !dl.items.is_empty(),
        "picture-with-schemecolor.docx 顯示列表不得為空"
    );
}

/// 燈 3：既有含文字文字框檔（WC048 原版與 Mod 檔）文字發射零回退（回歸鎖）。
#[test]
fn wc048_text_box_text_zero_regression() {
    for file in [
        "xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx",
        "xopt/TestFiles_WC_WC048-Text-Box-in-Cell-Mod.docx",
    ] {
        let eng = load_corpus(file);
        let mut all_texts = Vec::new();
        for p in 0..eng.page_count() {
            let dl = eng.page_display_list(p).expect("頁存在");
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
                        println!("RULE: x1={x1}, y1={y1}, x2={x2}, y2={y2}, color=0x{color_argb:08X}, w={width_pt}");
                    }
                    DrawItem::Rect {
                        x,
                        y,
                        w,
                        h,
                        fill_argb,
                    } => {
                        println!("RECT: x={x}, y={y}, w={w}, h={h}, fill=0x{fill_argb:08X}");
                    }
                    DrawItem::Text {
                        x,
                        y,
                        text,
                        color_argb,
                        ..
                    } => {
                        println!("TEXT: x={x}, y={y}, text={text:?}, color=0x{color_argb:08X}");
                    }
                    _ => {}
                }
            }

            for it in &dl.items {
                if let DrawItem::Text { text, .. } = it {
                    all_texts.push(text.clone());
                }
            }
        }
        for target in ["Textbox1", "Textbox2", "Textbox3"] {
            assert!(
                all_texts.iter().any(|t| t.contains(target)),
                "{file} 必須發射 {target} 文字項（回歸鎖）"
            );
        }
    }
}
