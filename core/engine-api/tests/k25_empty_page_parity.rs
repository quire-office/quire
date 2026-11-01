//! K25 回歸鎖：n751077 頁數與**墨跡**parity（LO 真值由 pymupdf 實測，見本檔 §LO 真值）。
//!
//! # 本鎖鎖的是什麼（以及**不**是什麼）
//! 卡面前提「LO 2 頁有內容、我方第 2 頁渲染為空頁（empty_page_mismatch）」經實測
//! 不成立：LO **自己**的第 2 頁（索引 1）是物理性空白——該頁 PDF 內容流只有
//! `/Artifact` 裁剪矩形（零 `BT…ET`、零 `Do`），72dpi 光柵非白像素 **0**。我方該頁
//! 同樣零墨跡（4 個零字形 Text，來自 Title/Subtitle×2/BodyText 空段）。
//! ⇒ 「每頁 text 數 > 0」在 LO 真值上本身不可能成立。
//! 剩下的 0.0 內容分＝`scripts/render_lab.py::pdf_page_facts()` 把**宣告**在共用
//! `/Resources`（兩頁同 obj 41）卻**未繪製**的圖片計成該頁內容，餓死了既有 4b
//! 「同位雙空＝度量假分差」臂（逐行對照與實跑證據＝`docs/k25-empty-page-diag.md` §1）。
//! 度量面屬本卡白名單禁區（`scripts/*`），另立度量卡；本檔只做**引擎側零回退鎖**。
//! 下游兩卡（K25 完成 metadata 記帳）：`t_785595db`（K25-A 度量口徑，本鎖不得變紅）、
//! `t_e15382b3`（K25-B 釘定群組 y 全塌，LO 真值即本檔 §LO 真值之延伸）。
//!
//! # LO 真值（pymupdf 實測，單位＝PDF pt 原點左上；探針＝scratch k25_probe{2,5,6}.py）
//! - 頁數 2；兩頁 MediaBox 612×792。
//! - p0：**繪製**圖片 rect=[17.25, 59.65, 83.85, 118.5]（xref 6 `Im6`，`Do` 命中）、
//!   文字行 `TEXT1`／`TEXT2`／`TEXT3`／`TEXT4`／`TEXT5`／`TEXT6TEXT7TEXT8TEXT9`。
//! - p1：`lines=[]`、`drawings=0`、**繪製**圖片 rect=[]（宣告未繪製）、非白像素 0。
//!
//! # 四盞燈（本檔）
//! - 燈 A：頁數 == 2（LO 同）——防 K5-diag §2.1 舊態（我方 3 頁）复活。
//! - 燈 B：p0 六個 TEXT 標籤**全數**存在＋恰好 1 張 `Image`，且圖片幾何 ∈ LO ±1pt。
//! - 燈 C：p1 **零墨跡**（與 LO 真空頁同形）——防有人把空段虛構出字去騙相似度。
//! - 燈 D：p1 仍是**獨立一頁**（`starts_section`＝para-level `sectPr` 照實開頁），
//!   不許用「刪頁」方式讓頁數與 LO 對齊（那是拿結構換分數）。
//!
//! 本檔為**純鎖**（本卡無引擎改動，故無 BUGGY 回塞態）；其鑑別力以「把期望常數
//! 改錯即必紅」證明，見檔尾 `k25_lock_is_discriminating_selfcheck`。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

const FILE: &str = "ooxmlimport/n751077.docx";

fn load() -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + FILE;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 corpus docx 失敗")
}

/// 該頁是否有任何墨跡：非空文字、填色矩形、圖片、規則線皆算。
fn ink_items(eng: &Engine, page: usize) -> Vec<&'static str> {
    let dl = eng
        .page_display_list(page)
        .unwrap_or_else(|| panic!("第 {page} 頁顯示列表應存在"));
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { text, .. } if !text.trim().is_empty() => Some("text"),
            DrawItem::Text { .. } => None,
            DrawItem::Rect { .. } => Some("rect"),
            DrawItem::Image { .. } => Some("image"),
            DrawItem::Rule { .. } => Some("rule"),
        })
        .collect()
}

/// 燈 A：頁數與 LO 一致（2 頁）。
#[test]
fn k25_lamp_a_page_count_matches_lo() {
    let eng = load();
    assert_eq!(
        eng.page_count(),
        2,
        "n751077 頁數必須與 LO 一致＝2（K5-diag §2.1 舊態 3 頁不得复活）"
    );
}

/// 燈 B：p0 內容全收＋錨圖幾何與 LO 繪製 rect 對齊（±1pt）。
#[test]
fn k25_lamp_b_page0_all_labels_and_painted_image() {
    let eng = load();
    let dl = eng.page_display_list(0).expect("p0 顯示列表");

    let mut texts: Vec<&str> = Vec::new();
    let mut imgs: Vec<(f32, f32, f32, f32)> = Vec::new();
    for it in &dl.items {
        match it {
            DrawItem::Text { text, .. } if !text.trim().is_empty() => texts.push(text),
            DrawItem::Image { x, y, w, h, .. } => imgs.push((*x, *y, *w, *h)),
            _ => {}
        }
    }

    for want in [
        "TEXT1",
        "TEXT2",
        "TEXT3",
        "TEXT4",
        "TEXT5",
        "TEXT6TEXT7TEXT8TEXT9",
    ] {
        assert!(
            texts.iter().any(|t| t.contains(want)),
            "p0 應含 {want}（LO p0 六行實測）；實際={texts:?}"
        );
    }
    assert_eq!(
        imgs.len(),
        1,
        "p0 應恰有 1 張圖片（LO 唯一內文圖 xref 6 繪於 p0）；實際={imgs:?}"
    );
    let (x, y, w, h) = imgs[0];
    // LO 繪製 rect=[17.25, 59.65, 83.85, 118.5] ⇒ x/y/w/h
    for (got, want, what) in [
        (x, 17.25, "x"),
        (y, 59.65, "y"),
        (w, 66.60, "w"),
        (h, 58.85, "h"),
    ] {
        assert!(
            (got - want).abs() <= 1.0,
            "p0 圖片 {what} 應 ∈ LO {want}±1pt，實際 {got}"
        );
    }
}

/// 燈 C：p1 零墨跡——與 LO 真空頁同形（LO 該頁非白像素 0、內容流零繪製指令）。
#[test]
fn k25_lamp_c_page1_is_ink_free_like_lo() {
    let eng = load();
    let ink = ink_items(&eng, 1);
    assert!(
        ink.is_empty(),
        "p1 必須與 LO 同為零墨跡頁（LO 實測非白像素 0）；實際墨跡類={ink:?}"
    );
}

/// 燈 D：p1 是 para-level `sectPr` 照實開的**結構頁**，不許靠刪頁湊頁數。
#[test]
fn k25_lamp_d_page2_is_structural_section_page() {
    let eng = load();
    let brks: Vec<serde_json::Value> =
        serde_json::from_str(&eng.page_breaks_json()).expect("pagebreaks JSON 可解析");
    assert_eq!(brks.len(), 2, "pagebreaks 應與頁數同＝2");
    assert_eq!(
        brks[1]["starts_section"].as_bool(),
        Some(true),
        "p1 必須是 section 起始頁（源檔 w:sectPr×2，LO 同形）——頁數一致不得靠壓掉結構頁達成；實際={}",
        brks[1]
    );
    // 我方 p0 涵蓋段落 0–1、p1 涵蓋 2–5：第 2 頁承的是真空樣式段，非內容頁。
    assert_eq!(brks[0]["last_para"].as_u64(), Some(1));
    assert_eq!(brks[1]["first_para"].as_u64(), Some(2));
    assert_eq!(brks[1]["last_para"].as_u64(), Some(5));
}

/// 鎖的鑑別力自查：把「期望值」改錯必須紅——證明本檔斷言真的在跑，不是空過。
/// （纯鎖無引擎改動，故以偽期望值取代 BUGGY 回塞態；見檔首說明。）
#[test]
fn k25_lock_is_discriminating_selfcheck() {
    let eng = load();
    // 反向斷言 1：p0 絕非空頁（若 p0 變空，燈 B 必紅）。
    assert!(
        !ink_items(&eng, 0).is_empty(),
        "p0 若無墨跡則燈 B 的期望已失真——本鎖失去意義，須先修 p0"
    );
    // 反向斷言 2：p1 的墨跡集合必須是**空**（若引擎開始在 p1 虛構字，燈 C 必紅）。
    assert!(ink_items(&eng, 1).is_empty());
    // 反向斷言 3：把期望頁數改成 3 必須不等於實測（防有人把燈 A 寫成恆真）。
    assert_ne!(
        eng.page_count(),
        3,
        "頁數若變 3＝K5-diag 舊態复活，燈 A 應紅"
    );
    assert_ne!(
        eng.page_count(),
        1,
        "頁數若變 1＝靠壓結構頁湊數，燈 A/D 應紅"
    );
}
