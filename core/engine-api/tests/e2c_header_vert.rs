//! E2c 紅燈考卷：頁首 part 的 `wps:bodyPr/@vert` 直排框未消費（fdo78957）。
//!
//! 鐵律（比照 `e2_vertical_text.rs`）：斷言一律使用 LO pymupdf 實測真值的
//! **dir 向量與行 bbox 形狀**，嚴禁只斷言文字內容；第二檔實證禁止單檔調參。
//!
//! LO 真值來源：`docs/e2c-redlight.md` §1（pymupdf `get_text("dict")` 逐行
//! `dir`＋`bbox`，口徑同 `scripts/e2_vertical_truth.py`；數值由本卡
//! `scripts/e2c_lo_truth.py` 走 `render_lab` 同一條 LO 轉換路徑實跑取得）。
//!
//! - 頁 0：`Quire` dir=[0.0,-1.0] bbox=[561.6,380.0,577.1,411.9] size=14.0
//!   （頁首 `word/header1.xml` 的 `wps:bodyPr vert="vert270"` 框，由下而上整行旋轉）
//! - 第二檔 `n779627.docx`：頁尾 `word/footer1.xml` 是**純 VML**
//!   `v:textbox style="layout-flow:vertical"`，LO 實測該頁 a–e 五行**全橫排**
//!   （dir=[1,0]）、頁尾三框零文字零 drawing ⇒ 直排臂不得被 VML 宣告觸發。

use quire_engine::{DrawItem, Engine};
use quire_layout::{LineFlow, TextMeasurer, FLAG_DIR_EAVERT, FLAG_DIR_VERT, FLAG_DIR_VERT270};

fn read_corpus(rel: &str) -> Vec<u8> {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取 {}: {e}", p.display()))
}

/// 取頁 0 全部文字項（text, x, y, size_pt, flags）。
fn text_items(engine: &Engine, page: usize) -> Vec<(String, f32, f32, f32, u8)> {
    let dl = engine
        .page_display_list(page)
        .unwrap_or_else(|| panic!("顯示列表缺頁 {page}"));
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                flags,
                text,
                ..
            } => Some((text.clone(), *x, *y, *size_pt, *flags)),
            _ => None,
        })
        .collect()
}

/// 考卷 1（目標檔發射存在性）：fdo78957 頁 0 顯示清單必須存在 `Quire` 項。
///
/// 修前實測：頁 0 顯示清單只有 `test` 一條（＋兩個灰佔位 Rect），直排框連框
/// 帶字完全不發 ⇒ 本燈紅（真缺字 `Quire` 的顯示列表側實據）。
#[test]
fn e2c_lamp1_header_vert_text_emitted() {
    let bytes = read_corpus("ooxmlexport/fdo78957.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("open fdo78957");
    let items = text_items(&eng, 0);
    let hit = items.iter().find(|(t, ..)| t == "Quire");
    assert!(
        hit.is_some(),
        "頁 0 顯示清單必須存在頁首直排框的 `Quire` 文字項；實得 {:?}",
        items.iter().map(|(t, ..)| t.as_str()).collect::<Vec<_>>()
    );
    // 同檔正文 `test` 不得因本臂而消失或變成直排（橫排零回退）。
    let body = items
        .iter()
        .find(|(t, ..)| t == "test")
        .expect("正文 test 項不得消失");
    assert_eq!(
        body.4 & (FLAG_DIR_VERT270 | FLAG_DIR_VERT | FLAG_DIR_EAVERT),
        0,
        "正文 test 項 flags={:#x} 不應帶任何行方向旗標（橫排零回退）",
        body.4
    );
}

/// 考卷 2（方向旗標＋行 bbox 命中 LO 真值）：`Quire` 帶 `FLAG_DIR_VERT270`，
/// 且 `LineFlow::Vertical270` 的行 bbox 命中 LO 真值 ±4pt。
///
/// 斷言形體照 `e2_vertical_text.rs`：dir 取 `LineFlow::dir_vector()`（方向向量，
/// 非布林），bbox 取 `compute_line_bbox(base, len, col_w)`（細長形＋四邊命中），
/// 嚴禁只比對文字。
#[test]
fn e2c_lamp2_line_dir_and_bbox_hit_lo_truth() {
    let bytes = read_corpus("ooxmlexport/fdo78957.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("open fdo78957");
    let items = text_items(&eng, 0);
    let (text, x, y, size, flags) = items
        .iter()
        .find(|(t, ..)| t == "Quire")
        .cloned()
        .expect("必須存在 Quire 文字項（見 lamp1）");

    // 1. 行方向旗標＝vert270 族（由下而上），且不可與 vert／eaVert 族混用。
    assert_ne!(
        flags & FLAG_DIR_VERT270,
        0,
        "Quire flags={flags:#x} 未帶 FLAG_DIR_VERT270（頁首直排宣告未進發射層）"
    );
    assert_eq!(
        flags & (FLAG_DIR_VERT | FLAG_DIR_EAVERT),
        0,
        "Quire flags={flags:#x} 不得同時帶 vert/eaVert 旗標（族混淆）"
    );

    // 2. 方向向量：LO pymupdf dir=[0.0,-1.0]（整行由下而上），非布林旗標可比。
    let flow = LineFlow::from_vert_str(Some("vert270"), false);
    assert_eq!(flow, LineFlow::Vertical270, "vert270 應分類為 Vertical270 族");
    let dir = flow.dir_vector();
    assert!(
        dir[0].abs() < 1e-6 && (dir[1] + 1.0).abs() < 1e-6,
        "dir 向量應為 [0.0,-1.0]（LO 實測），實得 {dir:?}"
    );

    // 3. 字級：LO size=14.0。
    assert!(
        (size - 14.0).abs() <= 1.0,
        "Quire 字級 {size} 應命中 LO 14.0（±1pt）"
    );

    // 4. 行 bbox 形狀細長（高 >> 寬）＋四邊命中 LO 真值 ±4pt。
    let len = TextMeasurer::new(size).text_width(&text);
    let col_w = size * 1.22; // E2 定律 col=13.4@11pt（docs/e2-vert-anchor-law.md）
    let bbox = flow.compute_line_bbox(x, y, len, col_w);
    assert!(
        (bbox[3] - bbox[1]) > 2.0 * (bbox[2] - bbox[0]),
        "Quire bbox 應細長（高 >> 寬，整行旋轉），實得 {bbox:?}"
    );
    const LO: [f32; 4] = [561.6, 380.0, 577.1, 411.9];
    let dev = [
        (bbox[0] - LO[0]).abs(),
        (bbox[1] - LO[1]).abs(),
        (bbox[2] - LO[2]).abs(),
        (bbox[3] - LO[3]).abs(),
    ];
    assert!(
        dev.iter().all(|d| *d <= 4.0),
        "Quire bbox={bbox:?} 偏離 LO 真值 {LO:?} 超過 4pt（各邊偏差 {dev:?}）",
    );

    // 5. 不重發：同檔同頁 `Quire` 只允許一個文字項（框內兩段，第二段空）。
    let n = items.iter().filter(|(t, ..)| t == "Quire").count();
    assert_eq!(n, 1, "Quire 文字項應恰一項（空段不發射），實得 {n}");
}

/// 考卷 3（第二檔實證，禁單檔調參）：`n779627.docx` 頁尾框是**純 VML**
/// `layout-flow:vertical`，LO 實測全橫排（a–e 五行 dir=[1,0]）＋頁尾框零文字。
///
/// 本燈鎖的是「新直排臂只認 DrawingML `wps:bodyPr/@vert`，不認 VML 舊宣告」：
/// 若把 VML `txbx_vertical` 一併灌進直排臂，拉丁文會被逐字堆疊／整行旋轉，
/// 與 LO 真值相反（`docs/e2-vert-anchor-law.md` n779627 鐵條的第二實據）。
#[test]
fn e2c_lamp3_vml_layout_flow_must_not_trigger_vertical() {
    let bytes = read_corpus("ooxmlimport/n779627.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("open n779627");
    let items = text_items(&eng, 0);
    assert!(
        items.iter().any(|(t, ..)| t == "a"),
        "正文 a–e 行不得消失（本卡零擾動守門），實得 {:?}",
        items.iter().map(|(t, ..)| t.as_str()).collect::<Vec<_>>()
    );
    for (t, _, _, _, flags) in &items {
        assert_eq!(
            flags & (FLAG_DIR_VERT270 | FLAG_DIR_VERT | FLAG_DIR_EAVERT),
            0,
            "n779627 文字項 {t:?} flags={flags:#x} 不得帶行方向旗標：LO 實測該頁全橫排，\
             頁尾 VML layout-flow:vertical 對拉丁文不觸發直排"
        );
    }
    // LO 頁尾三框零文字：我方亦不得從頁尾 part 冒出文字（`[Pick the date]` 屬
    // 內容控件佔位提示，LO 該頁未繪製 ⇒ 發射它會製造真多字）。
    assert!(
        !items.iter().any(|(t, ..)| t.contains("Pick")),
        "頁尾 VML 框的 sdt 佔位提示不得發射（LO 該頁零文字實據）"
    );
}

/// 考卷 4（橫排零回退）：fdo78957 橫排項的幾何不因本臂改變。
///
/// `test` 項是頁 0 唯一的橫排正文行；本臂只在 `wps:bodyPr/@vert` 命中時觸發，
/// 橫排項的 x/y/字級必須與修前同值（SVG byte 級不變鐵條的顯示列表側鏡像）。
#[test]
fn e2c_lamp4_horizontal_geometry_untouched() {
    let bytes = read_corpus("ooxmlexport/fdo78957.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("open fdo78957");
    let items = text_items(&eng, 0);
    let (t, x, y, size, flags) = items
        .iter()
        .find(|(t, ..)| t == "test")
        .cloned()
        .expect("正文 test 項必須存在");
    assert_eq!(t, "test");
    assert_eq!(flags, 0, "test 項 flags 應為 0（無行方向旗標）");
    // 修前實測值（本卡改動前 dump：x=90.0 y=844.1 size=14.0）。
    assert!(
        (x - 90.0).abs() <= 0.01 && (y - 844.1).abs() <= 0.01 && (size - 14.0).abs() <= 0.01,
        "test 項幾何被本臂改動：({x}, {y}, {size})，應逐位保持 (90.0, 844.1, 14.0)"
    );
}
