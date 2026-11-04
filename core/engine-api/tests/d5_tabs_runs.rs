//! D5：tab 停壓平＋字級粗體丟失＋highlight 黑底丟失紅燈考卷。
//!
//! 依據任務卡規範，以 LO PDF pymupdf 抽 span 為真值：
//! 1. 班表「1st Shift」與「6:00」兩 span 的 x 差 ≥ 30pt（tab 拉開），各 span x 對 LO ±12pt。
//! 2. 「Scheduled Saturday Overtime」渲染字級 == LO span 字級 ±0.5pt（20pt），bold 生效（FLAG_BOLD）。
//! 3. 「IMPORTANT NOTICE」item 帶 highlight/shading 底（底層矩形 item 存在且覆蓋文字 bbox）。

use quire_engine::{DrawItem, Engine, FLAG_BOLD, FLAG_UNDERLINE};

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
        DrawItem::Text { x, y, text, .. }
            if (text == "1" || text.starts_with("1st") || text.contains("Shift"))
                && (y - 334.3).abs() <= 5.0 =>
        {
            let has_shift_with_1 = text.contains("Shift")
                && dl.items.iter().any(|other| {
                    matches!(other, DrawItem::Text { text: t, y: oy, .. } if t == "1" && (oy - y).abs() <= 1.0)
                });
            if text.contains("6:00") || has_shift_with_1 {
                None // 未拆開或上標拆分形態下優先取起點「1」
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

/// 燈 6：外框 4pt 黑 stroke：源檔 VML shape _x0000_s1026（567×733.65pt、filled="f"、strokeweight="4pt"、默認 strokecolor=black）。
/// LO 整頁包一圈 4pt 黑實線；我方應發射 4pt 黑色 Rule（color_argb=0xFF000000，width_pt=4.0）。
#[test]
fn d5_lamp6_outer_frame_stroke_4pt_black() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    // 尋找寬度約 567pt、高度約 733.65pt 的外框 Rule（例如頂邊或底邊）
    let frame_rules: Vec<_> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Rule {
                x1,
                y1,
                x2,
                y2,
                width_pt,
                color_argb,
                ..
            } => {
                let len = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
                if len >= 500.0 && (*width_pt - 4.0).abs() <= 0.5 {
                    Some((*width_pt, *color_argb))
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();

    assert!(
        !frame_rules.is_empty(),
        "外框 4pt Rule 未找到（源檔 _x0000_s1026 strokeweight=4pt）"
    );

    for (w, color) in frame_rules {
        let rgb = color & 0x00FF_FFFF;
        assert_eq!(
            rgb, 0x000000,
            "外框 Rule（寬度 {w}pt）顏色應為黑色 (0xFF000000)，實得 0x{color:08X}"
        );
    }
}

/// 燈 7：下劃線：「(NOTE THE START TIME!)」w:u 單線，文字 item 帶 FLAG_UNDERLINE。
#[test]
fn d5_lamp7_note_the_start_time_underline() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let note_item = dl.items.iter().find_map(|it| match it {
        DrawItem::Text { text, flags, .. } if text.contains("NOTE THE START TIME") => {
            Some((*flags, text.clone()))
        }
        _ => None,
    });

    assert!(
        note_item.is_some(),
        "未找到包含「NOTE THE START TIME」的文字項"
    );
    let (flags, text) = note_item.unwrap();
    assert_ne!(
        flags & FLAG_UNDERLINE,
        0,
        "「{text}」文字項未帶有下劃線標記 FLAG_UNDERLINE（flags={flags}）"
    );
}

/// 燈 8：上標：「1st」「2nd」的 st/nd 是 w:vertAlign=superscript，我方純文字。發射需縮字級＋y 上移。
/// LO pymupdf 真值：
/// - 「1」size=16.0, y=391.3
/// - 「st」size=9.3, y=391.0（縮字級至約 0.58~0.6 倍，且 y 向上偏移）
#[test]
fn d5_lamp8_superscript_size_and_y_offset() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let st_item = dl.items.iter().find_map(|it| match it {
        DrawItem::Text {
            x,
            y,
            size_pt,
            text,
            ..
        } if text.trim() == "st" => Some((*x, *y, *size_pt)),
        _ => None,
    });

    assert!(
        st_item.is_some(),
        "「1st」中的「st」未獨立成上標 item（現行被壓入同一純文字字串）"
    );

    let (_st_x, st_y, st_sz) = st_item.unwrap();

    // 尋找同一行宿主基準文字「1」（或「1st Shift」的基線）
    let base_item = dl.items.iter().find_map(|it| match it {
        DrawItem::Text {
            x,
            y,
            size_pt,
            text,
            ..
        } if text.contains("1") && !text.contains("1st") && (y - st_y).abs() <= 10.0 => {
            Some((*x, *y, *size_pt))
        }
        _ => None,
    });

    let base_sz = base_item.map(|(_, _, sz)| sz).unwrap_or(16.0);
    // 上標字級約 0.58~0.65 倍
    assert!(
        st_sz <= base_sz * 0.75,
        "上標「st」字級應縮小（基線 {base_sz}pt，實得 {st_sz}pt）"
    );
    // y 應向上移動（在顯示列表中 y 座標比基線小，或上移 ≥ 1pt）
    if let Some((_, base_y, _)) = base_item {
        assert!(
            st_y <= base_y + 0.01,
            "上標「st」y 座標應向上偏移（基線 y={base_y}，實得 st_y={st_y}）"
        );
    }
}

/// 燈 9：粗體清單確認：SATURDAY, APRIL 02, 2011、班表時間欄、1st/2nd Shift 標籤段皆 w:b（修 bold 時別漏）。
#[test]
fn d5_lamp9_bold_list_confirmation() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let targets = [
        "SATURDAY, APRIL 02, 2011",
        "6:00 a.m. Start",
        "3:30 p.m. Start",
    ];

    for target in targets {
        let item = dl.items.iter().find_map(|it| match it {
            DrawItem::Text { text, flags, .. } if text.contains(target) => {
                Some((*flags, text.clone()))
            }
            _ => None,
        });

        assert!(
            item.is_some(),
            "目標文字項「{target}」未在 display list 中找到"
        );
        let (flags, text) = item.unwrap();
        assert_ne!(
            flags & FLAG_BOLD,
            0,
            "「{text}」粗體未生效（flags={flags}，應含 FLAG_BOLD={FLAG_BOLD}）"
        );
    }
}

/// 燈 10：頁腳 tab：「11-016」靠左＋「3-23-2011」靠右（LO 分佈兩端），我方相鄰置中——同 tab 根因。
/// LO pymupdf 真值：
/// - 「11-016」起點 x≈33.3pt
/// - 「3-23-2011」起點 x≈501.4pt
///
/// 兩者 x 差 = 501.4 - 33.3 = 468.1pt ≥ 400pt。
#[test]
fn d5_lamp10_footer_tabs_spread_ends() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let left_item = dl.items.iter().find_map(|it| match it {
        DrawItem::Text { text, x, y, .. } if text.contains("11-016") => {
            Some((*x, *y, text.clone()))
        }
        _ => None,
    });

    let right_item = dl.items.iter().find_map(|it| match it {
        DrawItem::Text { text, x, y, .. } if text.contains("3-23-2011") => {
            Some((*x, *y, text.clone()))
        }
        _ => None,
    });

    assert!(left_item.is_some(), "未找到包含「11-016」的頁腳文字項");
    assert!(right_item.is_some(), "未找到包含「3-23-2011」的頁腳文字項");

    let (left_x, _left_y, _) = left_item.unwrap();
    let (right_x, _right_y, _) = right_item.unwrap();

    let diff = right_x - left_x;
    assert!(
        diff >= 400.0,
        "頁腳「11-016」({left_x}) 與「3-23-2011」({right_x}) 的 x 差應 ≥ 400pt，實得 {diff}pt（LO 分佈兩端）"
    );

    let lo_left_x = 33.3f32;
    let lo_right_x = 501.4f32;
    assert!(
        (left_x - lo_left_x).abs() <= 15.0,
        "頁腳「11-016」x={left_x} 應在 LO 真值 {lo_left_x} ±15pt 內"
    );
    assert!(
        (right_x - lo_right_x).abs() <= 30.0,
        "頁腳「3-23-2011」x={right_x} 應在 LO 真值 {lo_right_x} ±30pt 內"
    );
}
