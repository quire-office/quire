//! F1 燈 2／燈 3：圖表發射層考卷（卡 t_f3acd271）。
//!
//! 真值口徑全部出自 `docs/f1-chart-anatomy.md` 的 LO 實抓（非推測）：
//! - `chart-size.docx`：LO p0 繪出數值軸刻度 `0,1,2,3,4`（x≈223.3、字級 8.93）
//!   與類別軸刻度 `1,2,3`（y≈201.3–212.2）；圖表外框 rect
//!   `[219.85, 112.75, 389.35, 214.15]`。我方開檔期即**零發射**（燈 1 已證
//!   解析層有值），故本考卷在動實作前必紅。
//! - `fdo76098.docx`：`c:title/c:tx` 真文字 `Sales`（標題發射的實據檔）。
//! - 零回退面：`fdo76591`／`WC048`／`99_Fields`／`n751077` 皆無圖表，
//!   其顯示列表必須逐位元組不動（燈 4）。
//!
//! 燈 3（鑑別力）走 `F1_CHART_BUGGY=1` 回塞鎖：該態下發射層整段跳過圖表
//! （＝本卡前的舊行為），考卷必抓得到「刻度與標題消失」。k25b 紀律：env
//! 只在單一測試內依序設與清，不跨執行緒平行污染。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn corpus(rel: &str) -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    let p = std::path::Path::new(&p).canonicalize().unwrap_or(p);
    assert!(p.exists(), "語料缺失：{}", p.display());
    p
}

fn engine(rel: &str) -> Engine {
    let bytes = std::fs::read(corpus(rel)).expect("讀取語料");
    Engine::from_docx_bytes(&bytes).expect("引擎載入 docx 成功")
}

/// 發射清單投影：文字項（含基線 y 與字級）＋非文字項計數。
#[derive(Debug, Clone)]
struct Tick {
    text: String,
    x: f32,
    y: f32,
    size: f32,
}

fn text_items(eng: &Engine, page: usize) -> Vec<Tick> {
    let dl = eng
        .page_display_list(page)
        .unwrap_or_else(|| panic!("頁 {page} 顯示清單必須存在"));
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text {
                x, y, size_pt, text, ..
            } => Some(Tick {
                text: text.clone(),
                x: *x,
                y: *y,
                size: *size_pt,
            }),
            _ => None,
        })
        .collect()
}

/// 非文字項（Rect／Rule）計數：零回退鎖用。
struct Geo {
    rects: usize,
    rules: usize,
}

fn geo_items(eng: &Engine, page: usize) -> Geo {
    let dl = eng
        .page_display_list(page)
        .unwrap_or_else(|| panic!("頁 {page} 顯示清單必須存在"));
    let mut g = Geo {
        rects: 0,
        rules: 0,
    };
    for it in &dl.items {
        match it {
            DrawItem::Rect { .. } => g.rects += 1,
            DrawItem::Rule { .. } => g.rules += 1,
            _ => {}
        }
    }
    g
}

/// 燈 2：chart-size 發射清單必含數值刻度與類別刻度（真實字串＋落點在圖表帶內）。
#[test]
fn f1_lamp2_chart_size_axis_ticks_emitted() {
    let eng = engine("ooxmlexport/chart-size.docx");
    let items = text_items(&eng, 0);
    let texts: Vec<&str> = items.iter().map(|t| t.text.as_str()).collect();

    // 2a：數值軸刻度 `0`／`1`／`2`／`3`／`4` 全數發射（LO 真值五支刻度）。
    //     本檔無 c:cat ⇒ 類別刻度也是 `1/2/3`，故以「含 0 且含 4」鎖定數值臂。
    for want in ["0", "4"] {
        assert!(
            texts.contains(&want),
            "燈 2a：數值軸刻度 {want} 未發射，實測文字清單 {texts:?}"
        );
    }
    // 2b：類別刻度至少 3 支 `1`／`2`／`3`（LO 等距 51.77pt 三支）。
    for want in ["1", "2", "3"] {
        assert!(
            texts.iter().filter(|t| ***t == *want).count() >= 2,
            "燈 2b：刻度 {want} 未同時出現在數值軸與類別軸，實測 {texts:?}"
        );
    }

    // 2c：刻度字級吃宣告值 sz=900 → 9pt（LO 實測 8.93，量化差 ≤0.2）。
    let ticks: Vec<&Tick> = items
        .iter()
        .filter(|t| matches!(t.text.as_str(), "0" | "1" | "2" | "3" | "4"))
        .collect();
    assert!(
        ticks.iter().all(|t| (t.size - 9.0).abs() <= 0.2),
        "燈 2c：刻度字級應 ≈9pt（宣告 sz=900），實測 {:?}",
        ticks.iter().map(|t| (t.text.as_str(), t.size)).collect::<Vec<_>>()
    );

    // 2d：幾何落點必落在圖表帶內（LO 圖表 rect y∈[112.75,214.15]，我方宿主框
    //     y=86.25 起，圖表帶以框頂為基準容差 ±30pt）。刻度 y 必 > 宿主框頂且
    //     < 框內正文末段之後，且 x 在圖表水平跨度（宿主框 212.25..522.75）內。
    for t in &ticks {
        assert!(
            t.x >= 212.0 && t.x <= 523.0,
            "燈 2d：刻度 {:?} 的 x={:.2} 溢出宿主框水平跨度",
            t.text,
            t.x
        );
        assert!(
            t.y >= 100.0 && t.y <= 230.0,
            "燈 2d：刻度 {:?} 的 y={:.2} 落在圖表帶外（LO 112.75–214.15）",
            t.text,
            t.y
        );
    }
    // 2e：軸線／格線要有筆畫項（LO 5 條水平灰格線；本切片至少發底軸＋格線）。
    let g = geo_items(&eng, 0);
    assert!(
        g.rules >= 8,
        "燈 2e：圖表應新增格線／軸線筆畫（宿主框 4 條＋圖表外框 4 條＋格線），實測 rules={}",
        g.rules
    );
}

/// 燈 2f：真有 `c:title/c:tx` 的檔案（fdo76098 `Sales`）標題文字必須發射。
#[test]
fn f1_lamp2f_title_text_emitted() {
    let eng = engine("ooxmlexport/fdo76098.docx");
    let mut found = false;
    for page in 0..3 {
        if eng.page_display_list(page).is_none() {
            break;
        }
        let texts: Vec<String> = text_items(&eng, page).into_iter().map(|t| t.text).collect();
        if texts.iter().any(|t| t.contains("Sales")) {
            found = true;
            break;
        }
    }
    assert!(found, "燈 2f：fdo76098 的圖表標題 Sales 未出現在任何頁發射清單");
}

/// 燈 3（鑑別力）＋燈 4（零回退）。
#[test]
fn f1_lamp3_buggy_toggle_and_no_chart_files_untouched() {
    // 燈 3：正常態有刻度；`F1_CHART_BUGGY=1` 回塞態整段跳過 ⇒ 刻度消失。
    let normal = {
        std::env::remove_var("F1_CHART_BUGGY");
        let eng = engine("ooxmlexport/chart-size.docx");
        let texts: Vec<String> = text_items(&eng, 0).into_iter().map(|t| t.text).collect();
        texts.iter().any(|t| t == "0") && texts.iter().any(|t| t == "4")
    };
    assert!(normal, "燈 3 前置：正常態必須有數值刻度");
    std::env::set_var("F1_CHART_BUGGY", "1");
    let buggy = {
        let eng = engine("ooxmlexport/chart-size.docx");
        let texts: Vec<String> = text_items(&eng, 0).into_iter().map(|t| t.text).collect();
        texts.iter().any(|t| t == "0")
    };
    std::env::remove_var("F1_CHART_BUGGY");
    assert!(
        !buggy,
        "燈 3：回塞態（整段跳過圖表）仍見刻度 ⇒ 考卷抓不到舊行為，鑑別力不成立"
    );

    // 燈 4：無圖表檔案的顯示列表逐位元組不動（新發射零擾動）。
    for (rel, page) in [
        ("ooxmlexport/fdo76591.docx", 0usize),
        ("ooxmlexport/99_Fields.docx", 0usize),
        ("ooxmlimport/n751077.docx", 0usize),
    ] {
        std::env::remove_var("F1_CHART_BUGGY");
        let a = engine(rel)
            .page_display_list(page)
            .and_then(|dl| serde_json::to_string(&dl).ok())
            .expect("顯示清單存在");
        std::env::set_var("F1_CHART_BUGGY", "1");
        let b = engine(rel)
            .page_display_list(page)
            .and_then(|dl| serde_json::to_string(&dl).ok())
            .expect("顯示清單存在");
        std::env::remove_var("F1_CHART_BUGGY");
        assert_eq!(a, b, "燈 4：{rel} 無圖表，兩態顯示清單必須逐位元組相同");
        assert!(
            !a.contains("c:chart") && !a.contains("chart1.xml"),
            "燈 4：{rel} 顯示清單不应出現圖表字串"
        );
    }
}
