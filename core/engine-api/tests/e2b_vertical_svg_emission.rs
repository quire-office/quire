//! E2b 紅燈考卷：直排發射層消費直排（真修畫面）（ADT109119）。
//!
//! 檢驗目標：
//! 1. 走渲染端（render_svg 的每頁 SVG 字串），不讀內部 dump；
//! 2. 斷言 btlr-textbox 的 `First paragraph.` 發射元素帶旋轉（`transform="rotate(-90 ...)"`）；
//! 3. 斷言兩段文字的繪製基線不共線（防疊字）；
//! 4. 文字落在方框 bbox 內（LO bbox [225.8, 154.2, 239.2, 224.9] ±4pt）。

use quire_engine::Engine;

#[path = "../../../shells/native/src/render_svg.rs"]
mod render_svg;

fn read_corpus(rel: &str) -> Vec<u8> {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    std::fs::read(&full).unwrap_or_else(|e| panic!("讀取測試檔失敗 {full}: {e}"))
}

#[test]
fn e2b_vertical_svg_emission_btlr_textbox() {
    let bytes = read_corpus("ooxmlexport/btlr-textbox.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 btlr-textbox 失敗");
    let dl = eng
        .page_display_list(0)
        .expect("讀取第 0 頁 display list 失敗");
    let svg = render_svg::page_to_svg(&dl);

    // 1. 斷言文字項存在
    assert!(
        svg.contains("First paragraph."),
        "SVG 必須包含 First paragraph. 文字"
    );
    assert!(
        svg.contains("Second paragraph."),
        "SVG 必須包含 Second paragraph. 文字"
    );

    // 2. 抽取 First paragraph. 與 Second paragraph. 的 <text ...> 標籤行
    let p1_tag = svg
        .lines()
        .find(|line| line.contains("<text ") && line.contains("First paragraph."))
        .expect("必須找到包含 First paragraph. 的 <text> 標籤");
    let p2_tag = svg
        .lines()
        .find(|line| line.contains("<text ") && line.contains("Second paragraph."))
        .expect("必須找到包含 Second paragraph. 的 <text> 標籤");

    // 3. 斷言 1：First paragraph. 發射元素必須帶旋轉 transform="rotate(-90 ...)"
    assert!(
        p1_tag.contains("transform=\"rotate(-90"),
        "First paragraph. 必須帶 rotate(-90 ...) 旋轉變換（實得：{p1_tag}）"
    );
    assert!(
        p2_tag.contains("transform=\"rotate(-90"),
        "Second paragraph. 必須帶 rotate(-90 ...) 旋轉變換（實得：{p2_tag}）"
    );

    // 4. 解析繪製基線與定位參數
    // 預期格式如：<text x="236.8" y="224.9" ... transform="rotate(-90 236.8 224.9)" ...>
    let extract_attr = |tag: &str, attr: &str| -> f32 {
        let pat = format!("{attr}=\"");
        let start = tag
            .find(&pat)
            .unwrap_or_else(|| panic!("標籤遺漏屬性 {attr}: {tag}"))
            + pat.len();
        let end = tag[start..]
            .find('"')
            .unwrap_or_else(|| panic!("屬性引號未閉合: {tag}"));
        tag[start..start + end]
            .parse::<f32>()
            .unwrap_or_else(|e| panic!("屬性 {attr} 數值解析失敗: {e}"))
    };

    let p1_x = extract_attr(p1_tag, "x");
    let p1_y = extract_attr(p1_tag, "y");
    let p2_x = extract_attr(p2_tag, "x");
    let _p2_y = extract_attr(p2_tag, "y");
    let p1_size = extract_attr(p1_tag, "font-size");

    // 斷言 2：斷言兩段文字的繪製基線不共線（防疊字）
    // 在橫排發射時，兩段基線都在相同的 y；在直排 vert270 模式下，兩段沿 x 分欄並排，x 基線相差至少 10pt。
    assert!(
        (p2_x - p1_x).abs() >= 10.0,
        "兩段文字繪製基線必須分開至少 10pt 防止疊字（p1_x={p1_x}, p2_x={p2_x}）"
    );

    // 斷言 3：文字落在方框 bbox 內（LO bbox [225.8, 154.2, 239.2, 224.9] ±4pt）
    // 在 vert270（旋轉 -90 度）下，文字字頂朝向 -x，字串往 -y 延伸。
    // 基線 (x, y) 為右下角基準點：
    // x0 = x - size_pt（或行寬）, x1 = x; y0 = y - line_len, y1 = y。
    let line_len1 = 70.67f32; // First paragraph. 測量長度約 70.67pt
    let bbox_x0 = p1_x - p1_size;
    let bbox_y0 = p1_y - line_len1;
    let bbox_x1 = p1_x;
    let bbox_y1 = p1_y;

    assert!(
        (bbox_x0 - 225.8).abs() <= 4.0
            && (bbox_y0 - 154.2).abs() <= 4.0
            && (bbox_x1 - 239.2).abs() <= 4.0
            && (bbox_y1 - 224.9).abs() <= 4.0,
        "First paragraph. SVG bbox [{bbox_x0:.2}, {bbox_y0:.2}, {bbox_x1:.2}, {bbox_y1:.2}] 偏離 LO 真值 [225.8, 154.2, 239.2, 224.9] 超過 4pt"
    );
}
