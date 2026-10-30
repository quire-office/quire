//! K15 紅燈：表內浮動錨點 `wp:anchor/@layoutInCell="1"` 的**幾何原點**（K10 只鎖「有發射」）。
//!
//! 根因：`core/` 從未解析 `layoutInCell`；`resolved_anchor` 對 `relativeFrom="page"`
//! 一律以頁原點起算，表內錨點整框飛離宿主儲存格（WC048 Textbox3 Rect 在右上
//! x=208.3 y=21.0）。LO 語意：`layoutInCell=1` ＋ 宿主在儲存格內 → 以**宿主儲存格**
//! 為基準（page/margin 的字面意義被壓制）。
//!
//! # LO 真值出處（pymupdf：render_lab.lo_to_pdf → page.get_drawings()/get_text("words")，
//! 座標已是 pt，無 px 換算；corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx p0）
//!
//! | 項目 | LO 實測 (x0,y0,x1,y1) pt | 原始 XML |
//! |---|---|---|
//! | 表格列界（橫線 y）| 72.3 / 138.2 / 344.3 / 358.2（3 列）| — |
//! | Textbox1 框 | (72.2, 86.9, 251.9, 107.4) | positionH column+align center；positionV paragraph |
//! | Textbox2 兩條藍線 | x 171.7–423.5，y 99.2／132.9 | positionH page+align center；positionV paragraph 274320 |
//! | Textbox3 框 | (72.25, 158.9, 459.25, 257.4) | positionH page+align **right**；positionV page posOffset 266700 EMU=21.0pt |
//!
//! 關鍵推導（皆為實測，非口算）：
//! - Textbox3 的 y：LO 第 2 列（index 1）頂 137.9（橫線 138.2 含半線寬）＋ posOffset
//!   21.0 = 158.9 ✓——即 `relativeFrom=page` 的 V 軸被壓成「宿主儲存格頂」；
//! - Textbox3 的 x：靠右對齊於格寬 150 的欄、框寬 387 → 算得負值，LO 落在 72.25＝
//!   **格左緣夾底**（框不得越過宿主格左緣）；Textbox1（185.9 寬置中於 150 寬欄）
//!   同樣落在格左緣 72.2，二例互證。
//!
//! # 與任務書的出入（老實記錄）
//! 任務書寫 LO 真值「y≈314pt／表格第 4 列」「x≈160」——**實測不成立**：此檔表格只有
//! 3 列，Textbox3 的 LO 位置在第 2 列內 y=158.9、x=72.25（見上表與 PDF 繪圖指令）。
//! 本檔以實測值為準。另：LO 第 1 列因 wrapTopAndBottom 的 Textbox2 撐高到 66pt
//! （我方 13.45pt）→ 列頂差 52pt；此「錨定框撐高宿主列」是獨立缺口（非本卡原點
//! 語意），故 y 斷言取**相對宿主格頂**（框 y − 21.0 必須踩在表格橫線上），
//! 與列高誤差解耦。
//!
//! 紅燈：
//! 1. Textbox3 Rect（387×185）：y−21.0 落在某條表格橫線（宿主格頂）；x 夾在格左緣
//!    72±6（LO 72.25）。修前 x=208.3 y=21.0 → 紅。
//! 2. Textbox1／Textbox2 位置回歸鎖（Textbox1 x 向 LO 72.2 收斂；Textbox2 維持）。
//! 3. fdo68607 封面大標位置零回退（K10 成果鎖：x≈207.6、y≈21.6）。
//! 4. `K15_CELL_ANCHOR_BUGGY=1` → 忽略 layoutInCell 旗標，燈 1 必紅。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

const WC048: &str = "xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx";

/// 取 p0 中寬高命中 (w,h)±1 的 Rect → (x, y)。
fn rect_xy(eng: &Engine, w: f32, h: f32) -> (f32, f32) {
    let dl = eng.page_display_list(0).expect("p0 存在");
    dl.items
        .iter()
        .find_map(|it| match it {
            DrawItem::Rect {
                x, y, w: rw, h: rh, ..
            } if (rw - w).abs() < 1.0 && (rh - h).abs() < 1.0 => Some((*x, *y)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("p0 找不到 {w}×{h} 的 Rect"))
}

/// p0 全部橫向**表格**線 y（Rule 且 y1==y2；表格框線 0.5pt，文字框邊框 0.75pt，
/// 以線寬排除文字框自己的邊——避免巧合自證）。
fn horizontal_rule_ys(eng: &Engine) -> Vec<f32> {
    let dl = eng.page_display_list(0).expect("p0 存在");
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Rule {
                y1, y2, width_pt, ..
            } if (y1 - y2).abs() < 0.01 && *width_pt < 0.6 => Some(*y1),
            _ => None,
        })
        .collect()
}

/// 燈 1：Textbox3（387×185）原點＝宿主儲存格（LO：格頂＋21.0、x 夾格左緣 72.25）。
#[test]
fn wc048_textbox3_anchored_to_host_cell() {
    let eng = load_corpus(WC048);
    let (x, y) = rect_xy(&eng, 387.0, 185.0);
    // 相對宿主格頂：y − posOffset(21.0475pt) 必須踩在某條表格橫線上（容差 0.6）。
    let cell_top = y - 21.0475;
    let ys = horizontal_rule_ys(&eng);
    assert!(
        ys.iter().any(|ry| (ry - cell_top).abs() < 0.6),
        "Textbox3 y={y} 應＝宿主格頂＋21.05（格頂 {cell_top} 須在表格橫線 {ys:?} 上）；\
         修前 y=21.0（頁原點，飛到右上）"
    );
    // 絕對值鎖：第 2 列頂 85.45＋21.05 ≈ 106.5（遠離修前 21.0）。
    assert!(
        (100.0..=115.0).contains(&y),
        "Textbox3 y 應落第 2 列內（≈106.5；LO 絕對 158.9 差在列撐高，見檔頭），實得 {y}"
    );
    // x：夾格左緣（LO 72.25）。修前 208.3（頁右對齊）。
    assert!(
        (66.0..=78.0).contains(&x),
        "Textbox3 x 應夾在宿主格左緣 72±6（LO 72.25），實得 {x}（修前 208.3）"
    );
}

/// 燈 2：Textbox1／Textbox2 位置回歸鎖。
#[test]
fn wc048_textbox1_textbox2_position_locked() {
    let eng = load_corpus(WC048);
    // Textbox2（273.6×110.55）：頁置中＝欄置中（中欄恰在頁中軸），LO 兩線中心 297.6。
    let (x2, y2) = rect_xy(&eng, 273.6, 110.55);
    assert!(
        (x2 - 160.85).abs() <= 1.0,
        "Textbox2 x 零回退（160.85±1），實得 {x2}"
    );
    assert!(
        (y2 - 93.6).abs() <= 1.0,
        "Textbox2 y 零回退（93.6±1），實得 {y2}"
    );
    // Textbox1（185.9×110.6）：LO x=72.2（框寬 > 欄寬 → 夾格左緣）。
    let (x1, y1) = rect_xy(&eng, 185.9, 110.6);
    assert!(
        (x1 - 72.2).abs() <= 3.0,
        "Textbox1 x 向 LO 72.2 收斂（±3），實得 {x1}"
    );
    assert!(
        (y1 - 86.4).abs() <= 1.0,
        "Textbox1 y 零回退（86.4±1），實得 {y1}"
    );
}

/// 燈 3：fdo68607 封面大標位置零回退（K10 成果；positionH column／V paragraph，
/// layoutInCell=1 但原點系本來就不是 page）。LO：x=210.2 y=23.7（DRAW 框 126.95,19.65）。
#[test]
fn fdo68607_cover_title_position_locked() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    let dl = eng.page_display_list(0).expect("p0 存在");
    let (x, y) = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text { x, y, text, .. } if text.contains("IGCSE Drama Glossary") => {
                Some((*x, *y))
            }
            _ => None,
        })
        .expect("p0 必須含「IGCSE Drama Glossary」");
    assert!(
        (x - 207.58).abs() <= 0.5,
        "封面大標 x 零回退（207.58±0.5），實得 {x}"
    );
    assert!(
        (y - 21.64).abs() <= 0.5,
        "封面大標 y 零回退（21.64±0.5），實得 {y}"
    );
}
