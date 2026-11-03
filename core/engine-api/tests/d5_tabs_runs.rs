//! D5：tab 停壓平＋字級粗體丟失＋highlight 黑底丟失紅燈考卷。
//!
//! 依據任務卡規範，以 LO PDF pymupdf 抽 span 為真值：
//! 1. 班表「1st Shift」與「6:00」兩 span 的 x 差 ≥ 30pt（tab 拉開），各 span x 對 LO ±12pt。
//! 2. 「Scheduled Saturday Overtime」渲染字級 == LO span 字級 ±0.5pt（20pt），bold 生效（FLAG_BOLD）。
//! 3. 「IMPORTANT NOTICE」item 帶 highlight/shading 底（底層矩形 item 存在且覆蓋文字 bbox）。

use quire_engine::{DrawItem, Engine, FLAG_BOLD};

fn read_corpus(rel: &str) -> Vec<u8> {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取 {}: {e}", p.display()))
}

/// 燈 1：班表「1st Shift」與「6:00」兩 span 的 x 差 ≥ 30pt（tab 拉開），各 span x 對 LO ±12pt。
///
/// LO pymupdf 真值：
/// - 「1」x=91.85, 「Shift」x=106.50（「1st Shift」起點 x≈91.85）
/// - 「6:00 a.m. Start – 2:30 p.m.」x=177.35
/// - 兩者 x 差 = 177.35 - 91.85 = 85.50pt ≥ 30pt
///
/// 現行缺陷：
/// 源檔班表壓成一整條字串「1st Shift 6:00 a.m. Start – 2:30 p.m.」，x≈209.95 置中，tab 語意丟失。
#[test]
fn d5_lamp1_timetable_tabs_expanded() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let shift_span = dl.items.iter().find_map(|it| match it {
        DrawItem::Text { x, y, text, .. } if text.contains("1st") || text.contains("Shift") => {
            if text.contains("6:00") {
                None // 未拆開
            } else {
                Some((*x, *y, text.clone()))
            }
        }
        _ => None,
    });

    let time_span = dl.items.iter().find_map(|it| match it {
        DrawItem::Text { x, y, text, .. } if text.contains("6:00") => {
            if text.contains("Shift") {
                None // 未拆開
            } else {
                Some((*x, *y, text.clone()))
            }
        }
        _ => None,
    });

    assert!(
        shift_span.is_some(),
        "「1st Shift」未獨立成 span（現行實測被壓成整條「1st Shift 6:00 a.m. Start – 2:30 p.m.」）"
    );
    assert!(
        time_span.is_some(),
        "「6:00」未獨立成 span（現行實測被壓成整條「1st Shift 6:00 a.m. Start – 2:30 p.m.」）"
    );

    let (shift_x, _shift_y, shift_text) = shift_span.unwrap();
    let (time_x, _time_y, time_text) = time_span.unwrap();

    let diff = time_x - shift_x;
    assert!(
        diff >= 30.0,
        "班表「1st Shift」({shift_x}) 與「6:00」({time_x}) 的 x 差應 ≥ 30pt，實得 {diff}pt"
    );

    let lo_shift_x = 91.85f32;
    let lo_time_x = 177.35f32;
    assert!(
        (shift_x - lo_shift_x).abs() <= 12.0,
        "「{shift_text}」x={shift_x} 應在 LO 真值 {lo_shift_x} ±12pt 內"
    );
    assert!(
        (time_x - lo_time_x).abs() <= 12.0,
        "「{time_text}」x={time_x} 應在 LO 真值 {lo_time_x} ±12pt 內"
    );
}

/// 燈 2：「Scheduled Saturday Overtime」渲染字級 == LO span 字級 ±0.5pt（20pt），bold 生效。
///
/// LO pymupdf 真值：
/// - text: 'Scheduled Saturday Overtime'
/// - bbox: [163.4, 185.9, 447.72, 208.22]
/// - size: 20.0, flags: 20 (含 bold)
///
/// 現行缺陷：
/// size_pt=12.0（預設值），flags=0（未生效粗體）。
#[test]
fn d5_lamp2_title_size_and_bold() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let title_item = dl.items.iter().find_map(|it| match it {
        DrawItem::Text {
            x,
            y,
            size_pt,
            flags,
            text,
            ..
        } if text.contains("Scheduled Saturday Overtime") => {
            Some((*x, *y, *size_pt, *flags, text.clone()))
        }
        _ => None,
    });

    let (_, _, size_pt, flags, text) = title_item.expect("必須存在「Scheduled Saturday Overtime」");
    let lo_size = 20.0f32;
    assert!(
        (size_pt - lo_size).abs() <= 0.5,
        "「{text}」渲染字級應為 {lo_size} ±0.5pt，實得 {size_pt}pt"
    );
    assert!(
        (flags & FLAG_BOLD) != 0,
        "「{text}」粗體未生效（flags={flags}，FLAG_BOLD={FLAG_BOLD}）"
    );
}

/// 燈 3：「IMPORTANT NOTICE」item 帶 highlight/shading 底（底層矩形 item 存在且覆蓋文字 bbox）。
///
/// LO pymupdf 真值：
/// - text: 'IMPORTANT NOTICE' bbox: [221.3, 147.1, 394.21, 170.42], color: white
/// - drawing: rect [221.2, 147.1, 389.95, 168.85] fill: black
///
/// 現行缺陷：
/// 「IMPORTANT NOTICE」無底層矩形 item（Rect），純白紙黑字。
#[test]
fn d5_lamp3_important_notice_highlight_background() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let (text_x, text_y, text_sz, color_argb) = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                color_argb,
                text,
                ..
            } if text.contains("IMPORTANT NOTICE") => Some((*x, *y, *size_pt, *color_argb)),
            _ => None,
        })
        .expect("必須存在「IMPORTANT NOTICE」文字項");

    // 文字 bbox 大致寬度（約 16 字 * 16pt * 0.6 ≈ 150pt）
    let approx_w = 120.0f32;
    let approx_h = text_sz;

    // 尋找覆蓋文字 bbox 的底層矩形
    let bg_rect = dl.items.iter().find_map(|it| match it {
        DrawItem::Rect {
            x,
            y,
            w,
            h,
            fill_argb,
        } => {
            let covers_x = *x <= text_x + 5.0 && (*x + *w) >= (text_x + approx_w - 5.0);
            let covers_y = (*y - text_y).abs() <= 20.0 && *h >= approx_h * 0.7;
            if covers_x && covers_y {
                Some((*x, *y, *w, *h, *fill_argb))
            } else {
                None
            }
        }
        _ => None,
    });

    assert!(
        bg_rect.is_some(),
        "「IMPORTANT NOTICE」底層矩形 item 不存在或未覆蓋文字 bbox（text_x={text_x}, text_y={text_y}）"
    );

    // 文字顏色應為白色（0xFFFFFFFF），底色為黑色（0xFF000000）
    let (_, _, _, _, fill_argb) = bg_rect.unwrap();
    let fill_rgb = fill_argb & 0x00FF_FFFF;
    assert_eq!(
        fill_rgb, 0x000000,
        "「IMPORTANT NOTICE」底色應為黑色，實得 0x{fill_argb:08X}"
    );
    let text_rgb = color_argb & 0x00FF_FFFF;
    assert_eq!(
        text_rgb, 0xFFFFFF,
        "「IMPORTANT NOTICE」字色應為白色，實得 0x{color_argb:08X}"
    );
}
