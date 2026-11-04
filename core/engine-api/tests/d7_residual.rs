//! D7：fdo76591 殘留四條紅燈考卷。
//!
//! 1. 頁首兩行 span 的 bbox 零重疊，且全部文字右緣 ≤ 外框內緣。
//! 2. 「THE WORK HOURS FOR」結束 x 與「SATURDAY」起點 x 零重疊、粗體段結束與「WILL」起點零重疊（span bbox 逐個相鄰不交會）。
//! 3. IMPORTANT NOTICE 黑底矩形寬度 ≥ 文字 span 組寬度（±2pt）。
//! 4. 班表各欄 span 起點 x 對 LO ±8pt；同欄上下行起點 x 一致（±1pt）。

use quire_engine::{DrawItem, Engine, FLAG_BOLD};
use quire_layout::TextMeasurer;

fn read_corpus(rel: &str) -> Vec<u8> {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取 {}: {e}", p.display()))
}

/// 燈 1：頁首兩行 span 的 bbox 零重疊，且全部文字右緣 ≤ 外框內緣。
///
/// LO pymupdf 真值：
/// - 「BELVIDERE ASSEMBLY PLANT」: y 範圍約 26.68 ~ 48.82, x 最大約 423.56
/// - 「World Class Manufacturing Starts Here」: y 範圍約 49.43 ~ 64.92, x 最大約 426.03
/// - 外框內緣：右側外框 Rule x=589.05, 4pt 邊框，內緣約 587.05pt。
///
/// 現行缺陷：
/// 1. 「BELVIDERE ASSEMBLY PLANT」渲染至 x=343.92，右緣溢出外框（>587.05pt）。
/// 2. 第一行 y=22.20, sz=20.0（底緣 y≈42.20）與第二行 y=36.00 互相壓字重疊。
#[test]
fn d7_lamp1_header_overlap_and_boundary() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let line1 = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                text,
                flags,
                ..
            } if text.contains("BELVIDERE") => Some((*x, *y, *size_pt, *flags, text.clone())),
            _ => None,
        })
        .expect("必須存在「BELVIDERE」文字項");

    let line2 = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                text,
                flags,
                ..
            } if text.contains("World Class Manufacturing") => {
                Some((*x, *y, *size_pt, *flags, text.clone()))
            }
            _ => None,
        })
        .expect("必須存在「World Class Manufacturing」文字項");

    let (l1_x, l1_y, l1_sz, _l1_flags, l1_text) = line1;
    let (l2_x, l2_y, _l2_sz, _l2_flags, l2_text) = line2;

    // 外框右側邊界內緣：Rule x=589.05, width=4.0pt，內緣 587.05pt
    let frame_inner_right = 587.05f32;

    // 測量兩行文字寬度
    let l1_w = TextMeasurer::new(l1_sz).text_width(&l1_text);
    let l2_w = TextMeasurer::new(_l2_sz).text_width(&l2_text);

    let l1_right = l1_x + l1_w;
    let l2_right = l2_x + l2_w;

    assert!(
        l1_right <= frame_inner_right,
        "第一行「{l1_text}」右緣 ({l1_right:.2}) 超出外框內緣 ({frame_inner_right:.2})"
    );
    assert!(
        l2_right <= frame_inner_right,
        "第二行「{l2_text}」右緣 ({l2_right:.2}) 超出外框內緣 ({frame_inner_right:.2})"
    );

    // y 軸 bbox 零重疊：第一行底部 (l1_y + l1_sz) 應小於等於第二行頂部 (l2_y)
    // 考慮基線與行高，第一行 y + l1_sz 必須 <= l2_y
    assert!(
        l1_y + l1_sz <= l2_y + 0.5,
        "頁首兩行 y 軸壓字重疊：第一行底緣 ({:.2}) > 第二行頂緣 ({:.2})",
        l1_y + l1_sz,
        l2_y
    );
}

/// 燈 2：「THE WORK HOURS FOR」結束 x 與「SATURDAY」起點 x 零重疊、粗體段結束與「WILL」起點零重疊。
///
/// LO pymupdf 真值：
/// - 「THE WORK HOURS FOR 」: bbox=[33.35, 342.93, 191.93, 358.42] (寬 158.58pt)
/// - 「SATURDAY, APRIL 02, 2011」: bbox=[192.05, 342.93, 372.83, 358.42] (寬 180.78pt, 粗體)
/// - 「 WILL BE AS SCHEDULED ...」: bbox=[372.90, 342.93, ...] 起點 x=372.90
///
/// 現行缺陷：
/// 粗體 run 字寬推進未計入字型/粗體實際寬度，導致 Span 1 與 Span 2 疊印、Span 2 與 Span 3 疊印。
#[test]
fn d7_lamp2_bold_advance_non_overlapping() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let span1 = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                text,
                ..
            } if text.contains("THE WORK HOURS FOR") => Some((*x, *y, *size_pt, text.clone())),
            _ => None,
        })
        .expect("必須存在「THE WORK HOURS FOR」文字項");

    let span2 = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                text,
                flags,
                ..
            } if text.contains("SATURDAY, APRIL 02, 2011") && (flags & FLAG_BOLD != 0) => {
                Some((*x, *y, *size_pt, text.clone()))
            }
            _ => None,
        })
        .expect("必須存在「SATURDAY, APRIL 02, 2011」粗體文字項");

    let span3 = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                text,
                ..
            } if text.contains("WILL BE AS SCHEDULED") => Some((*x, *y, *size_pt, text.clone())),
            _ => None,
        })
        .expect("必須存在「WILL BE AS SCHEDULED」文字項");

    let (s1_x, _, _s1_sz, s1_text) = span1;
    let (s2_x, _, _s2_sz, s2_text) = span2;
    let (s3_x, _, _s3_sz, s3_text) = span3;

    // 依據字型（Times New Roman 14pt / Times New Roman Bold 14pt）或真實度量計算結束 x：
    // TNR 14pt 下「THE WORK HOURS FOR 」真實字寬為 158.58pt；
    // TNR Bold 14pt 下「SATURDAY, APRIL 02, 2011」真實字寬為 180.78pt。
    // 發射的起點 x 必須保證各 span 逐個相鄰且零重疊。
    let s1_w = 158.58f32;
    let s2_w = 180.78f32;

    let s1_end = s1_x + s1_w;
    let s2_end = s2_x + s2_w;

    assert!(
        s1_end <= s2_x + 0.5,
        "「{s1_text}」結束 x ({s1_end:.2}) 與「{s2_text}」起點 x ({s2_x:.2}) 發生重疊（重疊 {:.2}pt）",
        s1_end - s2_x
    );

    assert!(
        s2_end <= s3_x + 0.5,
        "「{s2_text}」粗體段結束 x ({s2_end:.2}) 與「{s3_text}」起點 x ({s3_x:.2}) 發生重疊（重疊 {:.2}pt）",
        s2_end - s3_x
    );
}

/// 燈 3：IMPORTANT NOTICE 黑底矩形寬度 ≥ 文字 span 組寬度（±2pt）。
///
/// LO pymupdf 真值：
/// - text: 'IMPORTANT NOTICE ' bbox: [222.3, 140.59, 393.0, 158.31], 寬約 170.7pt (去尾空格約 166.7pt)
/// - drawing: rect [222.2, 139.9, 388.85, 158.25], 寬 166.65pt
///
/// 現行缺陷：
/// 黑底矩形寬度僅 144.57pt，只覆蓋「IMPORTANT NOTI」，末尾「CE」落在白底上。
#[test]
fn d7_lamp3_important_notice_shading_width() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let (text_x, text_y, _text_sz, text) = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                text,
                ..
            } if text.contains("IMPORTANT NOTICE") => Some((*x, *y, *size_pt, text.clone())),
            _ => None,
        })
        .expect("必須存在「IMPORTANT NOTICE」文字項");

    // LO 中「IMPORTANT NOTICE」文字真實寬度約 166.7pt
    let text_w = 166.7f32;

    // 尋找對應的黑底矩形
    let bg_rect = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } => {
                let matches_x = (*x - text_x).abs() <= 5.0;
                let matches_y = (*y - text_y).abs() <= 10.0;
                let is_black = (*fill_argb & 0x00FF_FFFF) == 0;
                if matches_x && matches_y && is_black {
                    Some((*x, *y, *w, *h))
                } else {
                    None
                }
            }
            _ => None,
        })
        .expect("必須存在覆蓋「IMPORTANT NOTICE」的黑底矩形");

    let (_rect_x, _rect_y, rect_w, _rect_h) = bg_rect;

    assert!(
        rect_w >= text_w - 2.0,
        "「{text}」黑底矩形寬度 ({rect_w:.2}pt) 小於文字寬度 ({text_w:.2}pt - 2.0pt = {:.2}pt)，末尾字落在白底上",
        text_w - 2.0
    );
}

/// 燈 4：班表各欄 span 起點 x 對 LO ±8pt；同欄上下行起點 x 一致（±1pt）。
///
/// LO pymupdf 真值：
/// - 欄 1（Shift）：x=91.85pt
/// - 欄 2（時間）：x=177.35pt（所有行均為 177.35pt）
/// - 欄 3（休息/用餐）：x=357.35pt（所有行均為 357.35pt）
///
/// 現行缺陷：
/// 班表 tab 未按段落 w:tabs 與預設 tab 停靠點計算，第 3 欄在部分行跳至 393.35pt（誤差高達 36pt），導致上下行起點錯位且與時間欄擦碰。
#[test]
fn d7_lamp4_timetable_tabs_alignment() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let mut col3_items: Vec<(f32, f32, String)> = Vec::new();

    // 在 display list 中尋找第 3 欄的文字項（注意 Break 可能被上標拆分，取「1」或「2」或「Lunch」起點）
    for it in &dl.items {
        if let DrawItem::Text { x, y, text, .. } = it {
            if *y >= 330.0 && *y <= 560.0 {
                // 排除時間欄 (x ≈ 177) 與 Shift 標籤 (x ≈ 91)
                if *x > 300.0
                    && (text == "1" || text == "2" || text == "Lunch" || text.contains("Break"))
                {
                    // 避免重複計入拆分後的「Break」
                    if text.contains("Break") && dl.items.iter().any(|o| {
                        matches!(o, DrawItem::Text { text: t, x: ox, y: oy, .. } if (t == "1" || t == "2") && (oy - y).abs() <= 1.0 && *ox < *x)
                    }) {
                        continue;
                    }
                    col3_items.push((*x, *y, text.clone()));
                }
            }
        }
    }

    assert!(
        col3_items.len() >= 6,
        "第 3 欄項目數不足（應有 6 行：1st Break, Lunch, 2nd Break 各 2 次，實得 {} 項：{:?}）",
        col3_items.len(),
        col3_items
    );

    let lo_col3_x = 357.35f32;
    let first_x = col3_items[0].0;

    for (x, y, text) in &col3_items {
        assert!(
            (*x - lo_col3_x).abs() <= 8.0,
            "第 3 欄項目「{text}」(y={y:.1}) 起點 x={x:.2} 偏離 LO 真值 {lo_col3_x:.2} 超過 8pt（實差 {:.2}pt）",
            (*x - lo_col3_x).abs()
        );
        assert!(
            (*x - first_x).abs() <= 1.0,
            "第 3 欄項目「{text}」(y={y:.1}) 起點 x={x:.2} 與首行 x={first_x:.2} 不一致（實差 {:.2}pt > 1.0pt）",
            (*x - first_x).abs()
        );
    }
}
