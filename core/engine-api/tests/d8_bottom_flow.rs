//! D8：fdo76591 底部排版差距紅燈考卷。
//!
//! 1. 「PLANT HUMAN RESOURCES DEPARTMENT」span bbox 與徽章圖 bbox 零重疊（LO 零重疊）。
//! 2. 頁腳兩 span y 對 LO ±10pt，且在正文末段之下。
//! 3. 首段每個 span 右緣 ≤ 外框內緣（對 LO 折行點 ±1 詞）。
//! 4. 班表末行與徽章圖起點之間的 y 間隔對 LO ±20pt（空白段落帶不得被吞）。

use quire_engine::{DrawItem, Engine};
use quire_layout::TextMeasurer;

fn read_corpus(rel: &str) -> Vec<u8> {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取 {}: {e}", p.display()))
}

/// 燈 1：「PLANT HUMAN RESOURCES DEPARTMENT」span bbox 與徽章圖 bbox 零重疊。
///
/// LO pymupdf 真值：
/// - 徽章圖 IMAGE bbox: [264.1, 594.1, 348.0, 687.0]，底緣 y=687.0pt。
/// - 「PLANT HUMAN RESOURCES DEPARTMENT」: bbox [159.05, 706.23, 452.04, 721.85]，頂緣 y=706.23pt。
/// - LO 兩者零重疊，頂緣大於底緣 19.23pt。
///
/// 現行缺陷：
/// 我方徽章圖 bottom≈683.60pt，而正文末段 y≈625.50pt，直接壓穿徽章圖正中。
#[test]
fn d8_lamp1_plant_hr_badge_non_overlapping() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    // 尋找徽章圖（x≈256, y≈590, w≈84, h≈93）
    let badge = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Image { x, y, w, h, .. }
                if (*x - 256.0).abs() < 10.0 && (*y - 590.0).abs() < 20.0 =>
            {
                Some((*x, *y, *w, *h))
            }
            _ => None,
        })
        .expect("必須存在徽章圖");

    let (_bx, by, _bw, bh) = badge;
    let badge_bottom = by + bh;

    // 尋找正文末段「PLANT HUMAN RESOURCES DEPARTMENT」
    let plant_hr = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                text,
                ..
            } if text.contains("PLANT HUMAN RESOURCES DEPARTMENT") => {
                Some((*x, *y, *size_pt, text.clone()))
            }
            _ => None,
        })
        .expect("必須存在「PLANT HUMAN RESOURCES DEPARTMENT」文字項");

    let (_px, py, _psz, _ptext) = plant_hr;

    assert!(
        py >= badge_bottom,
        "正文末段「PLANT HUMAN RESOURCES DEPARTMENT」y ({py:.2}) 壓穿徽章圖下緣 ({badge_bottom:.2})，發生重疊"
    );
}

/// 燈 2：頁腳兩 span y 對 LO ±10pt，且在正文末段之下。
///
/// LO pymupdf 真值：
/// - 「11-016」: y=730.53pt
/// - 「3-23-2011」: y=730.53pt
/// - 正文末段「PLANT HUMAN RESOURCES DEPARTMENT」: y=706.23~721.85pt。
///
/// 現行缺陷：
/// 我方「11-016」與「3-23-2011」y 僅為 641.60pt，上浮嚴重，偏離 LO 730.53pt 達 88.93pt。
#[test]
fn d8_lamp2_footer_anchoring_and_position() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let plant_hr = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                y, size_pt, text, ..
            } if text.contains("PLANT HUMAN RESOURCES DEPARTMENT") => Some(*y + *size_pt),
            _ => None,
        })
        .expect("必須存在「PLANT HUMAN RESOURCES DEPARTMENT」文字項");

    let footer_11_016 = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text { y, text, .. } if text.contains("11-016") => Some(*y),
            _ => None,
        })
        .expect("必須存在頁腳「11-016」文字項");

    let footer_date = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text { y, text, .. } if text.contains("3-23-2011") => Some(*y),
            _ => None,
        })
        .expect("必須存在頁腳「3-23-2011」文字項");

    let lo_footer_y = 730.53f32;

    // 必須在正文末段之下
    assert!(
        footer_11_016 >= plant_hr,
        "頁腳「11-016」y ({footer_11_016:.2}) 未在正文末段底緣 ({plant_hr:.2}) 之下"
    );
    assert!(
        footer_date >= plant_hr,
        "頁腳「3-23-2011」y ({footer_date:.2}) 未在正文末段底緣 ({plant_hr:.2}) 之下"
    );

    // 對 LO 真值 ±10pt
    assert!(
        (footer_11_016 - lo_footer_y).abs() <= 10.0,
        "頁腳「11-016」y ({footer_11_016:.2}) 偏離 LO 真值 ({lo_footer_y:.2}) 超過 10pt (差值 {:.2}pt)",
        (footer_11_016 - lo_footer_y).abs()
    );
    assert!(
        (footer_date - lo_footer_y).abs() <= 10.0,
        "頁腳「3-23-2011」y ({footer_date:.2}) 偏離 LO 真值 ({lo_footer_y:.2}) 超過 10pt (差值 {:.2}pt)",
        (footer_date - lo_footer_y).abs()
    );
}

/// 燈 3：首段每個 span 右緣 ≤ 外框內緣（對 LO 折行點 ±1 詞）。
///
/// LO pymupdf 真值：
/// - Line 1:「UNLESS OTHERWISE NOTIFIED, ALL PRODUCTION EMPLOYEES AND 」右緣 554.97pt
/// - Line 2:「ALL SKILLED TRADES EMPLOYEES, WILL BE REQUIRED TO WORK THE 」右緣 573.77pt
/// - Line 3:「FOLLOWING SATURDAY FOR 8 HOURS OF PRODUCTION:」右緣 460.71pt
/// - 外框內緣：587.05pt。
///
/// 現行缺陷：
/// 我方 Line 2 把「FOLLOWING」也排入行內，行尾超過 587.05pt 甚至穿透外框被裁切。
#[test]
fn d8_lamp3_first_paragraph_width_and_wrap() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let frame_inner_right = 587.05f32;

    // 檢查首段文字的所有行（包含 UNLESS, SKILLED, FOLLOWING/SATURDAY）
    let first_para_lines: Vec<(&DrawItem, f32, f32, &String)> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                text,
                ..
            } if text.contains("UNLESS")
                || text.contains("SKILLED")
                || text.contains("PRODUCTION:")
                || (text.contains("FOLLOWING") && text.contains("SATURDAY")) =>
            {
                let w = TextMeasurer::new(*size_pt).text_width(text);
                let right = *x + w;
                Some((it, right, *y, text))
            }
            _ => None,
        })
        .collect();

    assert!(!first_para_lines.is_empty(), "必須找到首段文字行");

    for (_it, right, y, text) in &first_para_lines {
        assert!(
            *right <= frame_inner_right,
            "首段行 (y={y:.2})「{text}」右緣 ({right:.2}) 超過外框內緣 ({frame_inner_right:.2})"
        );
    }

    // 檢查折行點：包含「SKILLED」的行不應包含「FOLLOWING」
    for (_it, _right, _y, text) in &first_para_lines {
        if text.contains("SKILLED") {
            assert!(
                !text.contains("FOLLOWING"),
                "首段未在「THE」後折行，依然將「FOLLOWING」排入同一行：「{text}」"
            );
        }
    }
}

/// 燈 4：班表末行與徽章圖起點之間的 y 間隔對 LO ±20pt（空白段落帶不得被吞）。
///
/// LO pymupdf 真值：
/// - 班表末行（2nd Break / 10:30 – 10:50 p.m.）：y 頂緣 538.49pt, 底緣 556.21pt, baseline≈554.20pt。
/// - 徽章圖起點：y = 594.10pt。
/// - 間隔（以班表末行 y 為基準）：594.10 - 538.49 = 55.61pt（若以底緣計則為 37.89pt）。
///
/// 現行缺陷：
/// 我方班表末行上浮至 y=475.10pt，徽章圖起點 y=590.45pt，間隔為 115.35pt，偏離 LO 真值達 60pt 以上。
#[test]
fn d8_lamp4_timetable_gap_to_badge() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    // 班表末行：尋找第二班最後的「2nd Break」或「10:30 – 10:50 p.m.」
    let timetable_last_row_y = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { y, text, .. }
                if text.contains("10:30 – 10:50 p.m.")
                    || (text.contains("Break") && *y > 450.0) =>
            {
                Some(*y)
            }
            _ => None,
        })
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .expect("必須找到班表末行");

    // 徽章圖起點 y
    let badge_y = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Image { x, y, .. }
                if (*x - 256.0).abs() < 10.0 && (*y - 590.0).abs() < 20.0 =>
            {
                Some(*y)
            }
            _ => None,
        })
        .expect("必須找到徽章圖");

    let gap = badge_y - timetable_last_row_y;
    let lo_gap = 55.61f32; // 594.10 - 538.49

    assert!(
        (gap - lo_gap).abs() <= 20.0,
        "班表末行 (y={timetable_last_row_y:.2}) 與徽章圖 (y={badge_y:.2}) 的間隔 ({gap:.2}pt) 偏離 LO 真值 ({lo_gap:.2}pt) 超過 20pt (差值 {:.2}pt)",
        (gap - lo_gap).abs()
    );
}
