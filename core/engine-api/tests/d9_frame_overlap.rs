//! D9：滿版外框底邊與 VML 圖壓疊紅燈考卷。
//!
//! 1. 4pt 外框底邊 y 對 LO 框底線 ±4pt（LO 真值 y=749.85pt）。
//! 2. CHRYSLER logo bbox 與框底線 y 關係對 LO 一致（logo 頂緣 ≥ 框底內緣 −4pt，即落在框下方而非穿越；外框負 z 背景層先繪，logo 繪於框線之上）。
//! 3. logo 下緣 ≤ 頁面高（不得溢出頁底被裁，LO 真值下緣 y=767.35pt ≤ 792.0pt）。

use quire_engine::{DrawItem, Engine};

fn read_corpus(rel: &str) -> Vec<u8> {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取 {}: {e}", p.display()))
}

/// 燈 1：4pt 外框底邊 y 對 LO 框底線 ±4pt。
///
/// LO pymupdf drawings 真值：
/// - Drawing 4: 4pt 外框底邊線段 Rect(22.10, 749.85, 589.05, 749.85), width_pt=4.0
/// - 中心線 y = 749.85pt
///
/// 現行缺陷：
/// 我方外框底邊線直接取 (y + h) = 18.20 + 733.65 = 751.85pt，
/// 未計入 4pt 筆畫向內描邊內縮（2.0pt），偏離 LO 框底線 749.85pt 達 2.0pt。
#[test]
fn d9_lamp1_frame_bottom_border_y() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let lo_frame_bottom_y = 749.85f32;

    // 尋找 4pt 外框底邊 Rule（width_pt≈4.0, y > 700 且為水平線）
    let frame_bottom = dl.items.iter().find_map(|it| match it {
        DrawItem::Rule {
            y1, y2, width_pt, ..
        } if (*width_pt - 4.0).abs() < 0.1 && (y1 - y2).abs() < 0.01 && *y1 > 700.0 => Some(*y1),
        _ => None,
    });

    let frame_bottom_y = frame_bottom.expect("必須存在 4pt 外框底邊 Rule");

    // 粗門檻：對 LO 框底線 ±4pt
    assert!(
        (frame_bottom_y - lo_frame_bottom_y).abs() <= 4.0,
        "外框底邊 y ({frame_bottom_y:.2}pt) 偏離 LO 框底線 ({lo_frame_bottom_y:.2}pt) 超過 4pt (差值 {:.2}pt)",
        (frame_bottom_y - lo_frame_bottom_y).abs()
    );

    // 精確微差收斂：消除 2pt 描邊位移差，收斂至 LO 內嵌描邊中心 ±1.0pt
    assert!(
        (frame_bottom_y - lo_frame_bottom_y).abs() <= 1.0,
        "外框底邊 y ({frame_bottom_y:.2}pt) 未依 4pt 描邊向內收斂，偏離 LO 真值 ({lo_frame_bottom_y:.2}pt) 達 {:.2}pt",
        (frame_bottom_y - lo_frame_bottom_y).abs()
    );
}

/// 燈 2：CHRYSLER logo bbox 與框底線 y 關係對 LO 一致（logo 頂緣 ≥ 框底內緣 −4pt，即落在框下方而非穿越）。
///
/// LO pymupdf drawings/images 真值：
/// - 外框底線（Drawing 4）：y=749.85pt, width_pt=4.0
/// - CHRYSLER logo（Image 11）：bbox = [201.05, 732.80, 411.60, 767.35]，頂緣 y=732.80pt
/// - 外框大框 _x0000_s1026 為 z-index 負值（-251659776）背板層；
///   logo 框 _x0000_s1036 為正值（+251657728）前景層，且在 LO 繪於外框線之上。
///
/// 現行缺陷：
/// 1. 圖層 z 序倒置：外框 Rule（items[3]）在 logo Image（items[1]）之後發射，4pt 黑線硬生生壓過 logo 中段。
/// 2. logo 幾何偏離：我方 logo 發射於 (193.80, 729.20)，頂緣 y=729.20pt 偏離 LO 真值 732.80pt 達 3.60pt。
#[test]
fn d9_lamp2_chrysler_logo_bbox_and_frame_overlap() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let lo_logo_top = 732.80f32;

    // 尋找 CHRYSLER logo（寬度 > 200pt, y > 700pt）
    let (logo_idx, logo_y) = dl
        .items
        .iter()
        .enumerate()
        .find_map(|(i, it)| match it {
            DrawItem::Image { y, w, .. } if *w > 200.0 && *y > 700.0 => Some((i, *y)),
            _ => None,
        })
        .expect("必須存在 CHRYSLER logo Image");

    // 尋找 4pt 外框底邊 Rule
    let (frame_bottom_idx, _frame_bottom_y) = dl
        .items
        .iter()
        .enumerate()
        .find_map(|(i, it)| match it {
            DrawItem::Rule {
                y1, y2, width_pt, ..
            } if (*width_pt - 4.0).abs() < 0.1 && (y1 - y2).abs() < 0.01 && *y1 > 700.0 => {
                Some((i, *y1))
            }
            _ => None,
        })
        .expect("必須存在 4pt 外框底邊 Rule");

    // 斷言 1：圖層順序——外框為負 z-index 背景層，logo 必須繪於框線之上（logo 索引在框線之後）
    assert!(
        logo_idx > frame_bottom_idx,
        "CHRYSLER logo (idx={logo_idx}) 在顯示列表中繪於外框底線 (idx={frame_bottom_idx}) 之下，外框黑線壓在 logo 之上"
    );

    // 斷言 2：幾何坐標——logo 頂緣對齊 LO 真值 732.80pt（±2.0pt）
    assert!(
        (logo_y - lo_logo_top).abs() <= 2.0,
        "CHRYSLER logo 頂緣 ({logo_y:.2}pt) 偏離 LO 真值 ({lo_logo_top:.2}pt) 超過 2pt (差值 {:.2}pt)",
        (logo_y - lo_logo_top).abs()
    );
}

/// 燈 3：logo 下緣 ≤ 頁面高（不得溢出頁底被裁）。
///
/// LO pymupdf images 真值：
/// - 頁面高度：792.00pt
/// - CHRYSLER logo 底緣：767.35pt ≤ 792.00pt（距頁底留有 24.65pt 安全邊界）
///
/// 現行缺陷：
/// 我方 logo 發射尺寸為 h=34.21pt，底緣為 729.20 + 34.21 = 763.41pt，偏離 LO 767.35pt 達 3.94pt。
#[test]
fn d9_lamp3_logo_bottom_within_page() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let page_height = dl.height_pt;
    let lo_logo_bottom = 767.35f32;

    let (_logo_y, logo_h, logo_bottom) = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Image { y, w, h, .. } if *w > 200.0 && *y > 700.0 => {
                Some((*y, *h, *y + *h))
            }
            _ => None,
        })
        .expect("必須存在 CHRYSLER logo Image");

    // 斷言 1：不得溢出頁底被裁
    assert!(
        logo_bottom <= page_height,
        "CHRYSLER logo 下緣 ({logo_bottom:.2}pt) 溢出頁底 ({page_height:.2}pt) 被裁切"
    );

    // 斷言 2：底緣對齊 LO 真值 767.35pt（±2.0pt）
    assert!(
        (logo_bottom - lo_logo_bottom).abs() <= 2.0,
        "CHRYSLER logo 下緣 ({logo_bottom:.2}pt, 高={logo_h:.2}pt) 偏離 LO 真值 ({lo_logo_bottom:.2}pt) 超過 2pt (差值 {:.2}pt)",
        (logo_bottom - lo_logo_bottom).abs()
    );
}
