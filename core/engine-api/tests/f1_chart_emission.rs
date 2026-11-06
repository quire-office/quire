//! F1 燈 2／燈 3：圖表發射層考卷（卡 t_f3acd271）。
//!
//! 真值口徑全部出自 `docs/f1-chart-anatomy.md` 的 LO 實抓（非推測）：
//! - `chart-size.docx`：LO p0 繪出數值軸刻度 `0,1,2,3,4`（x≈223.29、字級 8.93）
//!   與類別軸刻度 `1,2,3`（y≈201.34–212.21）；圖表外框 rect
//!   `[219.85, 112.75, 389.35, 214.15]`。我方開檔期即**零發射**（燈 1 已證
//!   解析層有值），故本考卷在動實作前必紅（紅燈證據存 `docs/f1-redlight.md`）。
//! - `fdo76098.docx`：`c:title/c:tx` 真文字 `Sales`（標題發射的實據檔）。
//! - 零回退面：`fdo76591`／`99_Fields`／`n751077` 皆無圖表，其顯示列表必須
//!   逐位元組不動（燈 4）。
//!
//! 三盞燈集中在**單一 test 函數**內依序跑（k25b 紀律：`F1_CHART_BUGGY` 的
//! 設／清只在單一測試內進行，不跨執行緒平行污染——發射層在開檔期讀該 env，
//! 若拆成兩個 test，平行跑時 lamp2 會誤吃到 lamp3 設的回塞態而假紅）。

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

/// 文字項投影（含基線 y 與字級）。
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
                x,
                y,
                size_pt,
                text,
                ..
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

/// 非文字項計數（零回退／格線鎖用）。
struct Geo {
    rects: usize,
    rules: usize,
}

fn geo_items(eng: &Engine, page: usize) -> Geo {
    let dl = eng
        .page_display_list(page)
        .unwrap_or_else(|| panic!("頁 {page} 顯示清單必須存在"));
    let mut g = Geo { rects: 0, rules: 0 };
    for it in &dl.items {
        match it {
            DrawItem::Rect { .. } => g.rects += 1,
            DrawItem::Rule { .. } => g.rules += 1,
            _ => {}
        }
    }
    g
}

/// F1 三盞燈依序驗證（單一 test：env 不外洵到其他執行緒）。
#[test]
fn f1_chart_emission_suite() {
    std::env::remove_var("F1_CHART_BUGGY");

    // ================= 燈 2：chart-size 刻度與軸文字發射 =================
    let eng = engine("ooxmlexport/chart-size.docx");
    let items = text_items(&eng, 0);
    let texts: Vec<&str> = items.iter().map(|t| t.text.as_str()).collect();

    // 2a：數值軸刻度 `0`／`4` 必發（LO 真值五支；本檔無 c:cat ⇒ 類別刻度也是
    //     `1/2/3`，故以「含 0 且含 4」鎖定數值臂，不與類別臂混同）。
    for want in ["0", "4"] {
        assert!(
            texts.contains(&want),
            "燈 2a：數值軸刻度 {want} 未發射，實測文字清單 {texts:?}"
        );
    }
    // 2b：`1`／`2`／`3` 必須在數值軸與類別軸**各見一支**（LO：數值 0..4、類別 1..3）。
    for want in ["1", "2", "3"] {
        assert!(
            texts.iter().filter(|t| ***t == *want).count() >= 2,
            "燈 2b：刻度 {want} 未同時出現在數值軸與類別軸，實測 {texts:?}"
        );
    }
    // 2c：刻度字級吃宣告 sz=900 → 9pt（LO 實測 8.93，量化差 ≤0.2）。
    let ticks: Vec<&Tick> = items
        .iter()
        .filter(|t| matches!(t.text.as_str(), "0" | "1" | "2" | "3" | "4"))
        .collect();
    assert!(
        ticks.iter().all(|t| (t.size - 9.0).abs() <= 0.2),
        "燈 2c：刻度字級應 ≈9pt（宣告 sz=900），實測 {:?}",
        ticks
            .iter()
            .map(|t| (t.text.as_str(), t.size))
            .collect::<Vec<_>>()
    );
    // 2d：落點必在圖表帶內（LO 圖表 rect y∈[112.75,214.15]、x∈[219.85,389.35]；
    //     我方宿主框 y=86.25／x=212.25 起，容差按框內段落流既有近似放寬）。
    for t in &ticks {
        assert!(
            t.x >= 212.0 && t.x <= 396.0,
            "燈 2d：刻度 {:?} 的 x={:.2} 溢出圖表水平跨度（LO 219.85–389.35）",
            t.text,
            t.x
        );
        assert!(
            t.y >= 100.0 && t.y <= 215.0,
            "燈 2d：刻度 {:?} 的 y={:.2} 落在圖表帶外（LO 112.75–214.15）",
            t.text,
            t.y
        );
    }
    // 2e：x 精度鎖（LO 數值刻度 x=223.29；我方常數律命中誤差 ≤3pt）。
    let zero = ticks.iter().find(|t| t.text == "0").expect("刻度 0");
    assert!(
        (zero.x - 223.29).abs() <= 3.0,
        "燈 2e：數值刻度 x={:.2} 偏離 LO 223.29 超過 3pt",
        zero.x
    );
    // 2f：格線／軸線筆畫必新增（宿主框 4 條＋圖表外框 4 條＋5 條格線＋2 條軸線）。
    let g = geo_items(&eng, 0);
    assert!(
        g.rules >= 13,
        "燈 2f：圖表應新增格線／軸線筆畫，實測 rules={}",
        g.rules
    );
    assert!(
        g.rects >= 5,
        "燈 2f：圖表外框＋3 柱形應新增 Rect，實測 rects={}",
        g.rects
    );

    // ================= 燈 2g：真標題檔（fdo76098 `Sales`）=================
    let eng = engine("ooxmlexport/fdo76098.docx");
    let mut found = false;
    for page in 0..3 {
        if eng.page_display_list(page).is_none() {
            break;
        }
        let ts: Vec<String> = text_items(&eng, page).into_iter().map(|t| t.text).collect();
        if ts.iter().any(|t| t.contains("Sales")) {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "燈 2g：fdo76098 的圖表標題 Sales 未出現在任何頁發射清單"
    );

    // ================= 燈 3：回塞鎖鑑別力 =================
    let normal = {
        let eng = engine("ooxmlexport/chart-size.docx");
        let ts: Vec<String> = text_items(&eng, 0).into_iter().map(|t| t.text).collect();
        ts.iter().any(|t| t == "0") && ts.iter().any(|t| t == "4")
    };
    assert!(normal, "燈 3 前置：正常態必須有數值刻度");
    std::env::set_var("F1_CHART_BUGGY", "1");
    let buggy = {
        let eng = engine("ooxmlexport/chart-size.docx");
        let ts: Vec<String> = text_items(&eng, 0).into_iter().map(|t| t.text).collect();
        ts.iter().any(|t| t == "0")
    };
    std::env::remove_var("F1_CHART_BUGGY");
    assert!(
        !buggy,
        "燈 3：回塞態（整段跳過圖表）仍見刻度 ⇒ 考卷抓不到舊行為，鑑別力不成立"
    );

    // ================= 燈 2h：strict 命名空間同律（LO 實繪 0,0.5,…,3.5）====
    let eng = engine("ooxmlexport/strict.docx");
    let stexts: Vec<String> = (0..2)
        .flat_map(|p| {
            eng.page_display_list(p)
                .map(|_| text_items(&eng, p))
                .unwrap_or_default()
                .into_iter()
                .map(|t| t.text)
                .collect::<Vec<_>>()
        })
        .collect();
    for want in ["0", "1", "2", "3", "0.5", "3.5"] {
        assert!(
            stexts.iter().any(|t| t == want),
            "燈 2h：strict 刻度 {want} 未發射（LO 真值 0..3.5 步 0.5），實測 {stexts:?}"
        );
    }

    // ================= 燈 4：無圖表檔案逐位元組零擾動 =================
    for rel in [
        "ooxmlexport/fdo76591.docx",
        "ooxmlexport/99_Fields.docx",
        "ooxmlimport/n751077.docx",
    ] {
        let a = engine(rel)
            .page_display_list(0)
            .and_then(|dl| serde_json::to_string(&dl).ok())
            .expect("顯示清單存在");
        std::env::set_var("F1_CHART_BUGGY", "1");
        let b = engine(rel)
            .page_display_list(0)
            .and_then(|dl| serde_json::to_string(&dl).ok())
            .expect("顯示清單存在");
        std::env::remove_var("F1_CHART_BUGGY");
        assert_eq!(a, b, "燈 4：{rel} 無圖表，兩態顯示清單必須逐位元組相同");
    }
}

// （嚴格說：strict 刻度鎖刻意放在主 suite 內而非另立 test——`F1_CHART_BUGGY`
// 是行程級 env，另立 test 平行跑時會與主 suite 的 env 切換 race，k25b 紀律。）
