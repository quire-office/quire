//! K25-B 紅燈鎖：n751077 釘定 `v:group` 子件框內文字的**逐框 y**（LO 真值）。
//!
//! # 缺陷（卡面實測）
//! 釘定群組（`coordorigin=",1440"` `coordsize="12239,12960"`，style
//! `mso-position-*-center/relative:page|margin`）內三個子 `v:rect` 的 top 分別
//! 1440／11160／2294，LO 把它們畫在三處（基線 y 分佈 144–660），我方舊態把
//! 群組的 12 個框內段落全堆在同一個起點（y≈153–166）——因為發射層只吃
//! 「最近一個子件的矩形」（`txbx_rect`＝K4c `mapped_rect`），子件 top 被丟棄。
//!
//! # LO 真值（pymupdf p0 內容流 `Td` 換算 top-origin，見 §診斷檔）
//! `docs/k25-empty-page-diag.md` §1.2／§2；探針＝scratch `k25_probe{2,5,7,8}.py`。
//!
//! | 標籤             | size | LO 基線 y | LO 基線 x |
//! |------------------|-----:|----------:|----------:|
//! | TEXT1            |   16 |    144.45 |      70.9 |
//! | TEXT2            |   48 |    660.35 |     394.5 |
//! | TEXT3            |   36 |    377.90 |      70.9 |
//! | TEXT4            |   36 |    419.35 |      70.9 |
//! | TEXT5            |   20 |    473.45 |      70.9 |
//! | TEXT6TEXT7TEXT8TEXT9 | 16 |   506.55 |      70.9 |
//!
//! # 盞
//! - 燈1：TEXT2（48pt，`w:jc=right`＋子 rect left=6494）基線 y ∈ 660.35±2、
//!   x ∈ 394.5±2（舊態 155.9／93.0 ⇒ 必紅）。
//! - 燈2：TEXT3／TEXT4／TEXT5／TEXT6-9 基線 y ∈ 377.90／419.35／473.45／506.55
//!   ±2（舊態 158／160／164／166 ⇒ 必紅）；TEXT1 y ∈ 144.45±2、x ∈ 70.9±2。
//! - 燈3（回塞鑑別力）：`K25_B_CHILD_STACK_BUGGY=1` ⇒ 逐框排版關掉 ⇒ 燈1／燈2
//!   的斷言必不成立（同一進程內先綠後紅，證明本鎖真在跑）。
//!
//! ⚠ 燈3 要改環境變數，與同檔其他測試共用行程 → 本檔刻意只留**一個** `#[test]`，
//! 內部依序跑三段（平行執行緒下 `set_var` 會汙染同期讀 env 的斷言）。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

const FILE: &str = "ooxmlimport/n751077.docx";

fn load() -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + FILE;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 corpus docx 失敗")
}

/// p0 顯示列表裡 `want` 標籤首次出現的文字項基線 (x, y)（`contains` 口徑，
/// 與 K25 燈 B 同——LO 把 TEXT6..9 合成一行，我方同形）。
fn label_pos(eng: &Engine, want: &str) -> (f32, f32) {
    let dl = eng.page_display_list(0).expect("p0 顯示列表應存在");
    for it in &dl.items {
        if let DrawItem::Text { x, y, text, .. } = it {
            if text.contains(want) {
                return (*x, *y);
            }
        }
    }
    panic!("p0 找不到標籤 {want}（LO p0 六行實測）");
}

/// 逐框 y 排版是否生效（燈3 用）：TEXT2 是否落在 LO 真值附近。
fn text2_matches_lo(eng: &Engine) -> bool {
    let (x, y) = label_pos(eng, "TEXT2");
    (y - 660.35).abs() <= 2.0 && (x - 394.5).abs() <= 2.0
}

#[test]
fn k25b_pinned_group_children_stack_like_lo() {
    // ---- 燈1／燈2：逐框基線命中 LO ±2 ----
    let eng = load();
    let want: [(&str, f32, f32); 6] = [
        ("TEXT1", 144.45, 70.9),
        ("TEXT2", 660.35, 394.5),
        ("TEXT3", 377.90, 70.9),
        ("TEXT4", 419.35, 70.9),
        ("TEXT5", 473.45, 70.9),
        ("TEXT6TEXT7TEXT8TEXT9", 506.55, 70.9),
    ];
    let mut observed: Vec<String> = Vec::new();
    let mut bad: Vec<String> = Vec::new();
    for (label, wy, wx) in want {
        let (x, y) = label_pos(&eng, label);
        observed.push(format!("{label}: x={x:.2} y={y:.2}"));
        if (y - wy).abs() > 2.0 {
            bad.push(format!("{label} 基線 y={y:.2} 應 ∈ {wy}±2"));
        }
        // x 只對「左內縮」與「右對齊」兩式發話：TEXT2 是 `w:jc=right`。
        if (x - wx).abs() > 2.0 {
            bad.push(format!("{label} 基線 x={x:.2} 應 ∈ {wx}±2"));
        }
    }
    assert!(
        bad.is_empty(),
        "n751077 p0 釘定群組框內文字必須逐框落到 LO 基線（±2pt）；\n實測：\n  {}\n違例：\n  {}",
        observed.join("\n  "),
        bad.join("\n  "),
    );

    // ---- 燈3：回塞鑑別力——關掉逐框排版 ⇒ 燈1／燈2 必紅 ----
    std::env::set_var("K25_B_CHILD_STACK_BUGGY", "1");
    let buggy = load();
    let hit = text2_matches_lo(&buggy);
    // 位置必須在**清除環境變數之前**取（顯示列表按呼叫現算，env 一清就回好路徑）。
    let (bx, by) = label_pos(&buggy, "TEXT2");
    std::env::remove_var("K25_B_CHILD_STACK_BUGGY");
    assert!(
        !hit,
        "回塞態（K25_B_CHILD_STACK_BUGGY=1）TEXT2 仍命中 LO ⇒ 本鎖未真正約束逐框排版；實測 x={bx:.2} y={by:.2}"
    );
    assert!(
        (by - 155.9).abs() <= 3.0,
        "回塞態應回到舊「全塌框頂」形態（y≈155.9）以證明燈2 斷的是同一件事；實測 y={by:.2}"
    );
}
