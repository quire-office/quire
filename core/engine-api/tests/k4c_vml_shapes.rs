use quire_engine::Engine;
use quire_render_trait::DrawItem;

/// 佔位灰（IMAGE_PLACEHOLDER_FILL）與佔位邊（IMAGE_PLACEHOLDER_EDGE）——
/// 測試端以字面常數對照（引擎端為私有常數），「非佔位灰」＝填色不是灰底。
const PLACEHOLDER_GRAY: u32 = 0xFFF0_F0F0;

fn load(path: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + path;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open")
}

/// 頁顯示列表摘要（紅燈原文診斷用）：每頁 (文字項數, Rect 填色清單, Rule 顏色清單)。
fn dump(eng: &Engine, tag: &str) {
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        let mut texts = Vec::new();
        let mut fills = Vec::new();
        let mut rules = Vec::new();
        for it in &dl.items {
            match it {
                DrawItem::Text { text, .. } => texts.push(text.clone()),
                DrawItem::Rect { fill_argb, .. } => fills.push(*fill_argb),
                DrawItem::Rule { color_argb, .. } => rules.push(*color_argb),
            }
        }
        println!("[{tag}] p{p}: texts={texts:?} fills={fills:?} rules={rules:?}");
    }
}

/// K4c 紅燈 1（groupshape-sdt）：`v:group`（巢狀兩層，coordorigin/coordsize
/// 座標系）→ `v:rect filled="f" stroked="f"` → `v:textbox
/// style="layout-flow:vertical"` → `w:txbxContent` → `w:sdt/w:sdtContent` 的
/// 「placeholder text」必須以**直排**（逐字、y 單調遞增、x 近似同欄）出現在
/// 某頁顯示列表。修前：v:group 不遞進、框以 px 慣例吃成 11879pt 怪物或被丟
/// 棄、emit_txbx_texts 只有水平斷行 ⇒ 紅。
#[test]
fn k4c_groupshape_sdt_vertical_text() {
    let eng = load("ooxmlimport/groupshape-sdt.docx");
    dump(&eng, "groupshape-sdt");
    let mut ok = false;
    let mut observed = String::new();
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        // 收集「placeholder text」拆字序列：單字元文字項，依 y 閱讀序。
        let mut chars: Vec<(f32, f32, char)> = Vec::new();
        let mut joined = String::new();
        for it in &dl.items {
            if let DrawItem::Text { x, y, text, .. } = it {
                if text.chars().count() == 1 && *text != " " {
                    chars.push((*y, *x, text.chars().next().unwrap()));
                }
                if text.contains("placeholder") {
                    joined.push_str(text);
                }
            }
        }
        observed = format!("p{p} joined={joined:?} chars={chars:?}");
        if joined.is_empty() && chars.is_empty() {
            continue;
        }
        chars.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap()
                .then(a.1.partial_cmp(&b.1).unwrap())
        });
        let seq: String = chars.iter().map(|(_, _, c)| *c).collect();
        let has_word = seq.contains("placeholder");
        let strictly_down = chars.windows(2).all(|w| w[1].0 > w[0].0 + 0.5);
        let x_spread = if chars.len() < 2 {
            0.0
        } else {
            let xs: Vec<f32> = chars.iter().map(|c| c.1).collect();
            xs.iter().fold(f32::MIN, |a, &b| a.max(b)) - xs.iter().fold(f32::MAX, |a, &b| a.min(b))
        };
        if has_word && strictly_down && x_spread <= 8.0 {
            ok = true;
            break;
        }
    }
    assert!(
        ok,
        "groupshape-sdt：框內「placeholder text」未以直排（逐字 y 單調遞增、同欄）發射。最後觀察：{observed}"
    );
}

/// K4c 紅燈 2（n780563）： textbox 內三色列表格（shd fill F4B29B/D34817/
/// 918485）——某頁必須存在**非佔位灰**的填充 Rect（數量 > 0）。修前我方
/// 「無填充無框線」⇒ 紅。
#[test]
fn k4c_n780563_non_placeholder_fill() {
    let eng = load("ooxmlexport/n780563.docx");
    dump(&eng, "n780563");
    let mut count = 0usize;
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        for it in &dl.items {
            if let DrawItem::Rect { fill_argb, .. } = it {
                if *fill_argb != PLACEHOLDER_GRAY {
                    count += 1;
                }
            }
        }
    }
    assert!(
        count > 0,
        "n780563：全無非佔位灰的填充 Rect（LO 第 1 頁有三色列填色）"
    );
}

/// K4c 紅燈 3（tdf152200）：黑框空矩形——某頁必須存在黑色系框線 Rule
/// （RGB 皆 < 0x40）或非佔位灰填充 Rect（數量 > 0）。修前只發灰底＋淺灰
/// 邊（0xFFCCCCCC 不算黑）⇒ 紅。
#[test]
fn k4c_tdf152200_black_frame() {
    let eng = load("ooxmlimport/tdf152200-bad_fldChar_end.docx");
    dump(&eng, "tdf152200");
    let mut count = 0usize;
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        for it in &dl.items {
            match it {
                DrawItem::Rule { color_argb, .. } => {
                    let (a, r, g, b) = (
                        (color_argb >> 24) & 0xFF,
                        (color_argb >> 16) & 0xFF,
                        (color_argb >> 8) & 0xFF,
                        color_argb & 0xFF,
                    );
                    if a != 0 && r < 0x40 && g < 0x40 && b < 0x40 {
                        count += 1;
                    }
                }
                DrawItem::Rect { fill_argb, .. } => {
                    if *fill_argb != PLACEHOLDER_GRAY {
                        count += 1;
                    }
                }
                DrawItem::Text { .. } => {}
            }
        }
    }
    assert!(
        count > 0,
        "tdf152200：無黑色系框線 Rule 也無非佔位灰填充 Rect（LO 有黑框空矩形）"
    );
}
