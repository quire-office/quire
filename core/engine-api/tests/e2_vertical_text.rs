//! E2 紅燈考卷：wps:bodyPr/@vert 直排解析與發射。
//!
//! 鐵律：斷言必須使用 LO pymupdf 實測真值 dir 向量與 bbox 形狀，嚴禁只斷言文字內容。
//! 包含 btlr-textbox 與非 btlr-textbox 第二檔實證，禁止單檔硬編碼調參。

use quire_engine::{DrawItem, Engine};
use quire_layout::{LineFlow, TextMeasurer, FLAG_DIR_VERT, FLAG_DIR_VERT270};

fn read_corpus(rel: &str) -> Vec<u8> {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取 {}: {e}", p.display()))
}

/// 考卷 1：wps:bodyPr/@vert 解析落地與三族區分。
/// 驗證 btlr-textbox (vert270)、fdo69636 (vert270)、tbrl-textbox (vert)、tdf124754 (vert)、fdo74605 (eaVert) 的解析結果。
#[test]
fn e2_lamp1_vert_attr_parsing_and_classification() {
    // 1. vert270 族：btlr-textbox
    let b_btlr = read_corpus("ooxmlexport/btlr-textbox.docx");
    let eng_btlr = Engine::from_docx_bytes(&b_btlr).expect("open btlr");
    let vert_btlr = eng_btlr.shape_boxes_vert_declared();
    assert_eq!(
        vert_btlr.as_deref(),
        Some("vert270"),
        "btlr-textbox 應解析出 vert270 宣告"
    );
    assert_eq!(
        LineFlow::from_vert_str(vert_btlr.as_deref(), false),
        LineFlow::Vertical270,
        "btlr-textbox 應分類為 Vertical270 族"
    );

    // 2. vert270 族第二檔：fdo69636
    let b_fdo69636 = read_corpus("ooxmlexport/fdo69636.docx");
    let eng_fdo69636 = Engine::from_docx_bytes(&b_fdo69636).expect("open fdo69636");
    let vert_fdo69636 = eng_fdo69636.shape_boxes_vert_declared();
    assert_eq!(
        vert_fdo69636.as_deref(),
        Some("vert270"),
        "fdo69636 應解析出 vert270 宣告"
    );
    assert_eq!(
        LineFlow::from_vert_str(vert_fdo69636.as_deref(), false),
        LineFlow::Vertical270,
        "fdo69636 應分類為 Vertical270 族"
    );

    // 3. vert 族：tbrl-textbox
    let b_tbrl = read_corpus("ooxmlexport/tbrl-textbox.docx");
    let eng_tbrl = Engine::from_docx_bytes(&b_tbrl).expect("open tbrl");
    let vert_tbrl = eng_tbrl.shape_boxes_vert_declared();
    assert_eq!(
        vert_tbrl.as_deref(),
        Some("vert"),
        "tbrl-textbox 應解析出 vert 宣告"
    );
    assert_eq!(
        LineFlow::from_vert_str(vert_tbrl.as_deref(), false),
        LineFlow::Vertical,
        "tbrl-textbox 應分類為 Vertical 族"
    );

    // 4. vert 族第二檔：tdf124754
    let b_tdf124754 = read_corpus("ooxmlimport/tdf124754.docx");
    let eng_tdf124754 = Engine::from_docx_bytes(&b_tdf124754).expect("open tdf124754");
    let vert_tdf124754 = eng_tdf124754.shape_boxes_vert_declared();
    assert_eq!(
        vert_tdf124754.as_deref(),
        Some("vert"),
        "tdf124754 應解析出 vert 宣告"
    );
    assert_eq!(
        LineFlow::from_vert_str(vert_tdf124754.as_deref(), false),
        LineFlow::Vertical,
        "tdf124754 應分類為 Vertical 族"
    );

    // 5. eaVert 族：fdo74605
    let b_fdo74605 = read_corpus("ooxmlexport/fdo74605.docx");
    let eng_fdo74605 = Engine::from_docx_bytes(&b_fdo74605).expect("open fdo74605");
    let vert_fdo74605 = eng_fdo74605.shape_boxes_vert_declared();
    assert_eq!(
        vert_fdo74605.as_deref(),
        Some("eaVert"),
        "fdo74605 應解析出 eaVert 宣告"
    );
    assert_eq!(
        LineFlow::from_vert_str(vert_fdo74605.as_deref(), false),
        LineFlow::EastAsianVertical,
        "fdo74605 應分類為 EastAsianVertical 族"
    );
}

/// 考卷 2：vert270 發射——btlr-textbox.docx
/// LO 真值（pymupdf dir 向量實抓）：
///   First paragraph. dir=[0.0, -1.0], bbox=[225.78, 154.23, 239.2, 224.9]
///   Second paragraph. dir=[0.0, -1.0], bbox=[239.23, 141.13, 252.65, 224.9]
#[test]
fn e2_lamp2_vert270_emission_btlr_textbox() {
    let bytes = read_corpus("ooxmlexport/btlr-textbox.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("open btlr");
    let dl = eng.page_display_list(0).expect("dl page 0");

    let p1 = dl.items.iter().find_map(|it| match it {
        DrawItem::Text { x, y, size_pt, flags, text, .. } if text.contains("First paragraph.") => {
            Some((*x, *y, *size_pt, *flags, text.clone()))
        }
        _ => None,
    }).expect("必須存在 First paragraph. 文字項");

    let p2 = dl.items.iter().find_map(|it| match it {
        DrawItem::Text { x, y, size_pt, flags, text, .. } if text.contains("Second paragraph.") => {
            Some((*x, *y, *size_pt, *flags, text.clone()))
        }
        _ => None,
    }).expect("必須存在 Second paragraph. 文字項");

    // 1. 斷言方向旗標必須帶 FLAG_DIR_VERT270（代表 dir=[0.0, -1.0]），不可為橫排預設
    assert!(
        p1.3 & FLAG_DIR_VERT270 != 0,
        "First paragraph. flags={:#x} 未帶 FLAG_DIR_VERT270 方向旗標（仍為橫排發射）",
        p1.3
    );
    assert!(
        p2.3 & FLAG_DIR_VERT270 != 0,
        "Second paragraph. flags={:#x} 未帶 FLAG_DIR_VERT270 方向旗標（仍為橫排發射）",
        p2.3
    );

    // 2. 斷言段落沿 x 軸由左向右排列（段落 2 在段落 1 右側，不可橫排重疊於相同 x）
    assert!(
        p2.0 >= p1.0 + 10.0,
        "段落 2 x={:.2} 應在段落 1 x={:.2} 右側至少一個行寬（修前重疊於同 x）",
        p2.0, p1.0
    );

    // 3. 斷言 bbox 形狀細長（高 >> 寬）且命中 LO bbox 真值 ±4pt
    let len1 = TextMeasurer::new(p1.2).text_width(&p1.4);
    let col_w1 = p1.2 * 1.22;
    let bbox1 = LineFlow::Vertical270.compute_line_bbox(p1.0, p1.1, len1, col_w1);
    assert!(
        (bbox1[3] - bbox1[1]) > 2.0 * (bbox1[2] - bbox1[0]),
        "First paragraph. bbox 形狀應細長（高 >> 寬），實得 {:?}",
        bbox1
    );
    assert!(
        (bbox1[0] - 225.78).abs() <= 4.0 && (bbox1[1] - 154.23).abs() <= 4.0
        && (bbox1[2] - 239.2).abs() <= 4.0 && (bbox1[3] - 224.9).abs() <= 4.0,
        "First paragraph. bbox={:?} 偏離 LO 真值 [225.8, 154.2, 239.2, 224.9] 超過 4pt",
        bbox1
    );

    let len2 = TextMeasurer::new(p2.2).text_width(&p2.4);
    let col_w2 = p2.2 * 1.22;
    let bbox2 = LineFlow::Vertical270.compute_line_bbox(p2.0, p2.1, len2, col_w2);
    assert!(
        (bbox2[0] - 239.23).abs() <= 4.0 && (bbox2[1] - 141.13).abs() <= 4.0
        && (bbox2[2] - 252.65).abs() <= 4.0 && (bbox2[3] - 224.9).abs() <= 4.0,
        "Second paragraph. bbox={:?} 偏離 LO 真值 [239.2, 141.1, 252.6, 224.9] 超過 4pt",
        bbox2
    );
}

/// 考卷 3：vert270 非 btlr-textbox 第二檔實證——fdo69636.docx
/// LO 真值：Vertical Text dir=[0.0, -1.0], bbox=[90.0, 72.57, 111.1, 170.55]
#[test]
fn e2_lamp3_vert270_second_file_fdo69636() {
    let bytes = read_corpus("ooxmlexport/fdo69636.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("open fdo69636");
    let dl = eng.page_display_list(0).expect("dl page 0");

    let item = dl.items.iter().find_map(|it| match it {
        DrawItem::Text { x, y, size_pt, flags, text, .. } if text.contains("Vertical Text") => {
            Some((*x, *y, *size_pt, *flags, text.clone()))
        }
        _ => None,
    }).expect("必須存在 Vertical Text 文字項");

    assert!(
        item.3 & FLAG_DIR_VERT270 != 0,
        "fdo69636 文字項 flags={:#x} 未帶 FLAG_DIR_VERT270 方向旗標",
        item.3
    );

    let len = TextMeasurer::new(item.2).text_width(&item.4);
    let col_w = item.2 * 1.22;
    let bbox = LineFlow::Vertical270.compute_line_bbox(item.0, item.1, len, col_w);
    assert!(
        (bbox[3] - bbox[1]) > 2.0 * (bbox[2] - bbox[0]),
        "fdo69636 bbox 形狀應細長（高 >> 寬），實得 {:?}",
        bbox
    );
    assert!(
        (bbox[0] - 90.0).abs() <= 4.0 && (bbox[1] - 72.57).abs() <= 4.0
        && (bbox[2] - 111.1).abs() <= 4.0 && (bbox[3] - 170.55).abs() <= 4.0,
        "fdo69636 bbox={:?} 偏離 LO 真值 [90.0, 72.57, 111.1, 170.55] 超過 4pt",
        bbox
    );
}

/// 考卷 4：vert 旋轉（由上而下）發射與雙檔實證——tbrl-textbox.docx + tdf124754.docx
/// tbrl-textbox LO 真值：
///   AAA1. dir=[0.0, 1.0], bbox=[382.4, 78.7, 395.82, 106.12]
///   AAA2. dir=[0.0, 1.0], bbox=[356.95, 78.7, 370.37, 106.12]
/// tdf124754 LO 真值：
///   This is a hyperlink: dir=[0.0, 1.0], bbox=[126.74, 83.85, 137.81, 163.85]
#[test]
fn e2_lamp4_vert_emission_tbrl_and_tdf124754() {
    // 檔 1：tbrl-textbox
    let b1 = read_corpus("ooxmlexport/tbrl-textbox.docx");
    let eng1 = Engine::from_docx_bytes(&b1).expect("open tbrl");
    let dl1 = eng1.page_display_list(0).expect("dl page 0");

    let p1 = dl1.items.iter().find_map(|it| match it {
        DrawItem::Text { x, y, size_pt, flags, text, .. } if text.contains("AAA1.") => {
            Some((*x, *y, *size_pt, *flags, text.clone()))
        }
        _ => None,
    }).expect("必須存在 AAA1. 文字項");

    let p2 = dl1.items.iter().find_map(|it| match it {
        DrawItem::Text { x, y, size_pt, flags, text, .. } if text.contains("AAA2.") => {
            Some((*x, *y, *size_pt, *flags, text.clone()))
        }
        _ => None,
    }).expect("必須存在 AAA2. 文字項");

    assert!(
        p1.3 & FLAG_DIR_VERT != 0,
        "AAA1. flags={:#x} 未帶 FLAG_DIR_VERT 方向旗標（dir=[0, 1]）",
        p1.3
    );
    assert!(
        p2.3 & FLAG_DIR_VERT != 0,
        "AAA2. flags={:#x} 未帶 FLAG_DIR_VERT 方向旗標（dir=[0, 1]）",
        p2.3
    );

    // 段落 2 應在段落 1 左側（tbrl 由右而左排列）
    assert!(
        p2.0 <= p1.0 - 10.0,
        "段落 2 x={:.2} 應在段落 1 x={:.2} 左側（tbrl 右至左分欄）",
        p2.0, p1.0
    );

    let len1 = TextMeasurer::new(p1.2).text_width(&p1.4);
    let col_w1 = p1.2 * 1.22;
    let bbox1 = LineFlow::Vertical.compute_line_bbox(p1.0, p1.1, len1, col_w1);
    assert!(
        (bbox1[0] - 382.4).abs() <= 4.0 && (bbox1[1] - 78.7).abs() <= 4.0
        && (bbox1[2] - 395.82).abs() <= 4.0 && (bbox1[3] - 106.12).abs() <= 4.0,
        "AAA1. bbox={:?} 偏離 LO 真值 [382.4, 78.7, 395.82, 106.12] 超過 4pt",
        bbox1
    );

    // 檔 2：tdf124754 第二檔實證
    let b2 = read_corpus("ooxmlimport/tdf124754.docx");
    let eng2 = Engine::from_docx_bytes(&b2).expect("open tdf124754");
    let dl2 = eng2.page_display_list(0).expect("dl page 0");

    let item2 = dl2.items.iter().find_map(|it| match it {
        DrawItem::Text { x, y, size_pt, flags, text, .. } if text.contains("This is a hyperlink") => {
            Some((*x, *y, *size_pt, *flags, text.clone()))
        }
        _ => None,
    }).expect("必須存在 This is a hyperlink 文字項");

    assert!(
        item2.3 & FLAG_DIR_VERT != 0,
        "tdf124754 flags={:#x} 未帶 FLAG_DIR_VERT 方向旗標",
        item2.3
    );
    let len2 = TextMeasurer::new(item2.2).text_width(&item2.4);
    let col_w2 = item2.2 * 1.22;
    let bbox2 = LineFlow::Vertical.compute_line_bbox(item2.0, item2.1, len2, col_w2);
    assert!(
        (bbox2[0] - 126.74).abs() <= 4.0 && (bbox2[1] - 83.85).abs() <= 4.0
        && (bbox2[2] - 137.81).abs() <= 4.0 && (bbox2[3] - 163.85).abs() <= 4.0,
        "tdf124754 bbox={:?} 偏離 LO 真值 [126.74, 83.85, 137.81, 163.85] 超過 4pt",
        bbox2
    );
}

/// 考卷 5：vertEastAsian* 與既有 layout-flow:vertical CJK 逐字堆疊臂保持不回退。
/// 驗證 groupshape-sdt.docx 框內文字仍以單字元 y 單調遞增發射。
#[test]
fn e2_lamp5_vert_east_asian_cjk_stack_no_regression() {
    let bytes = read_corpus("ooxmlimport/groupshape-sdt.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("open groupshape-sdt");
    let dl = eng.page_display_list(0).expect("dl page 0");

    let mut chars: Vec<(f32, f32, char)> = Vec::new();
    for it in &dl.items {
        if let DrawItem::Text { x, y, text, .. } = it {
            if text.chars().count() == 1 && *text != " " {
                chars.push((*y, *x, text.chars().next().unwrap()));
            }
        }
    }
    chars.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let seq: String = chars.iter().map(|(_, _, c)| *c).collect();
    assert!(
        seq.contains("placeholder"),
        "groupshape-sdt 仍應以逐字堆疊發射 placeholder，實得 {:?}",
        seq
    );
}
