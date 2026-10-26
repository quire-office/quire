//! L1-columns 紅燈（`w:cols` 分欄回流）——引擎級顯示列表斷言。
//!
//! 紅燈原文（PM run10 紀律 (3)）：雙欄檔 p0 的文字項 x 必須出現**兩個欄
//! 起點**、欄間距＝宣告 space。修前（main 基線）：`w:cols` 全走單欄，
//! p0 只有一個 x 起點 → 紅。
//!
//! fixture：`fixtures/l1_two_col.docx`（zipfile 手搓最小雙欄檔；生成腳本
//! 見交付報告）。A4 11906×16838 twip、邊界 1440、`w:cols w:num=2 w:space=720`
//! → 版心 451.3pt、欄寬 (451.3−36)/2＝207.65、第二欄起點
//! x＝72＋207.65＋36＝315.65pt。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load(path: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/").to_string() + path;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open")
}

/// 頁文字項的 x 起點集合（去重到 0.5pt 精度）。
fn text_xs(eng: &Engine, page: usize) -> Vec<f32> {
    let dl = eng.page_display_list(page).expect("頁存在");
    let mut xs: Vec<f32> = Vec::new();
    for it in &dl.items {
        if let DrawItem::Text { x, text, .. } = it {
            if text.trim().is_empty() {
                continue;
            }
            if !xs.iter().any(|o| (o - x).abs() < 0.5) {
                xs.push(*x);
            }
        }
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs
}

/// 紅燈 1：雙欄檔 p0 必須出現兩個欄起點——x≈72（版心左緣）與
/// x≈315.65（72＋欄寬 207.65＋欄距 36）。修前單欄：僅一組 x → 紅。
#[test]
fn l1_two_column_fixture_has_two_x_origins() {
    let eng = load("l1_two_col.docx");
    let xs = text_xs(&eng, 0);
    assert!(
        xs.iter().any(|x| (x - 72.0).abs() < 0.6),
        "首欄起點 x≈72 必在：xs={xs:?}"
    );
    assert!(
        xs.iter().any(|x| (x - 315.65).abs() < 0.6),
        "次欄起點 x≈315.65（版心左緣＋欄寬＋宣告 space=36pt）必在：xs={xs:?}"
    );
    // 欄間距＝宣告 space（36pt）＋等寬平分欄寬（207.65pt）：
    // 兩起點之差＝欄寬＋欄距（LO 同款等寬近似）。
    let first = xs
        .iter()
        .find(|x| (**x - 72.0).abs() < 0.6)
        .copied()
        .expect("首欄");
    let second = xs
        .iter()
        .find(|x| (**x - 315.65).abs() < 0.6)
        .copied()
        .expect("次欄");
    let pitch = second - first;
    assert!(
        (pitch - 243.65).abs() < 1.2,
        "欄距（第二欄起點−首欄起點）＝宣告欄寬+space≈243.65pt，實得 {pitch}"
    );
}

/// 紅燈 2：斷行寬＝欄寬（非版心寬）。每行文字寬必須 ≤ 欄寬 207.65pt＋容差；
/// 修前單欄斷行寬＝451.3 → 出現遠超欄寬的行 → 紅。
#[test]
fn l1_two_column_lines_fit_column_width() {
    let eng = load("l1_two_col.docx");
    let dl = eng.page_display_list(0).expect("頁存在");
    let mut worst = 0.0f32;
    for it in &dl.items {
        if let DrawItem::Text { text, .. } = it {
            // 顯示列表不帶行寬欄位：以字元數粗閘（12pt 字級下 lorem ipsum
            // 詞序列單欄每行 ≈88 字元，雙欄斷行 ≤ ≈45）。嚴判據在 layout
            // 側鎖 line.width_pt（cols_reflow 測群），此處引擎級防整鏈斷裂。
            worst = worst.max(text.chars().count() as f32);
        }
    }
    assert!(
        worst <= 50.0,
        "雙欄斷行每行字元數必須遠小於單欄（451.3pt 行寬）：最長行 {worst} 字元"
    );
}

/// 零擾動回塞鎖：無 `w:cols` 檔（sample.docx）文字仍全部從版心左緣起——
/// buggy 回塞（把未宣告誤當兩欄／欄序錯亂）會在本斷言紅。
#[test]
fn l1_single_column_unaffected() {
    let eng = load("sample.docx");
    let p = eng.page_breaks_json();
    assert!(!p.is_empty(), "單欄檔仍必須有分頁輸出");
    // p0 首個非空文字項 x 必須恰為版心左緣（sample.docx A4 預設邊界 72pt）。
    let dl = eng.page_display_list(0).expect("頁存在");
    let first_x = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text { x, text, .. } if !text.trim().is_empty() => Some(*x),
            _ => None,
        })
        .expect("p0 有文字項");
    assert!(
        (first_x - 72.0).abs() < 0.6,
        "單欄檔首文字項必須從版心左緣 72pt 起（實得 {first_x}）"
    );
}
