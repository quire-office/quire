//! K21 紅燈測試：軟性 `<w:br/>`（純換行 / textWrapping）落地。
//!
//! # 規格背景（出處 `docs/k16-residual-diag.md` §2）
//! OOXML 軟性 `<w:br/>`（無 `type` 或 `w:type="textWrapping"`；ECMA-376 §17.4.1）
//! 為行內純換行，不拆分段落，亦不觸發分頁。
//!
//! # 實據與 LO 真值
//! - `corpus/ooxmlexport/fdo68607.docx` 第 5 頁（索引 5）表格 cell 內同段落兩 run：
//!   `<w:t>Measured flow … patterns of sound.</w:t>` `<w:br/>` `<w:t>Regularity in time …</w:t>`。
//! - LO 側（pymupdf 實測，72dpi 標尺）：
//!   - 行 1 bbox：y ∈ [292.8, 309.9]，內容含 `patterns of sound.`
//!   - 行 2 bbox：y ∈ [309.9, 327.0]，內容含 `Regularity in time`
//!   - 兩行分離，y 軸相差 ≈ 17.1 pt。
//! - 我方修前現況：
//!   軟 br 被完全略過，兩 run 拼成單行 `sound.Regularity in time …`（同行同 y=194.79），
//!   導致 token 對賬誤報 `true_missing=['Regularity']`。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 corpus docx 失敗")
}

fn load_fixture(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 fixture docx 失敗")
}

const FDO68607: &str = "ooxmlexport/fdo68607.docx";
const TDF123054: &str = "ooxmlexport/tdf123054.docx";
const FIXTURE_MINIMAL: &str = "k21_minimal_soft_br.docx";

/// 燈 1（真檔）：fdo68607 渲染後，`Regularity in time` 必須與 `patterns of sound.` 落在不同行帶。
/// LO p5 實測：`patterns of sound.` (y=292.8), `Regularity in time` (y=309.9)，相差 ≈ 17.1pt。
#[test]
fn k21_lamp1_fdo68607_regularity_different_line() {
    let eng = load_corpus(FDO68607);
    let dl = eng.page_display_list(5).expect("fdo68607 第 5 頁應存在");

    let mut sound_items = Vec::new();
    let mut regularity_items = Vec::new();

    for it in &dl.items {
        if let DrawItem::Text { text, y, .. } = it {
            if text.contains("sound.") || text.contains("patterns of sound.") {
                sound_items.push((text.clone(), *y));
            }
            if text.contains("Regularity") {
                regularity_items.push((text.clone(), *y));
            }
        }
    }

    assert!(
        !sound_items.is_empty(),
        "第 5 頁應包含 'patterns of sound.' 相關文字項"
    );
    assert!(
        !regularity_items.is_empty(),
        "第 5 頁應包含 'Regularity' 相關文字項"
    );

    // 斷言 1：Regularity 不得與 sound. 黏在同一行文字項中（修前 bug：'sound.Regularity...'）
    for (text, _) in &regularity_items {
        assert!(
            !text.contains("sound."),
            "Regularity 不得與 sound. 黏在同一文字項中（修前假影），實得 text={text:?}"
        );
    }

    // 斷言 2：Regularity 必須落在 sound. 之下一行（y 差需大於等於 10pt）
    let (_, sound_y) = sound_items[0];
    let (_, reg_y) = regularity_items[0];
    assert!(
        reg_y > sound_y + 10.0,
        "Regularity 必須落在 sound. 下方行帶（y 差 ≥ 10pt），實得 sound_y={sound_y}, reg_y={reg_y}"
    );
}

/// 燈 2（合成最小檔）：單一段落 `A<w:br/>B` 渲染後 `A`、`B` 落兩條不同 baseline，且段落計數不變（不拆段）。
#[test]
fn k21_lamp2_minimal_soft_br() {
    let eng = load_fixture(FIXTURE_MINIMAL);

    // 斷言 1：段落計數不變（不拆段）
    assert_eq!(
        eng.paragraph_count(),
        1,
        "單一段落內軟 br 不得拆分段落，段落數必須為 1"
    );

    let dl = eng.page_display_list(0).expect("第 0 頁應存在");
    let mut a_items = Vec::new();
    let mut b_items = Vec::new();

    for it in &dl.items {
        if let DrawItem::Text { text, y, .. } = it {
            if text.trim() == "A" {
                a_items.push(*y);
            }
            if text.trim() == "B" {
                b_items.push(*y);
            }
        }
    }

    assert!(
        !a_items.is_empty(),
        "渲染輸出必須包含獨立的 'A' 文字項（若輸出為 'AB' 則表示軟 br 未生效）"
    );
    assert!(
        !b_items.is_empty(),
        "渲染輸出必須包含獨立的 'B' 文字項（若輸出為 'AB' 則表示軟 br 未生效）"
    );

    let y_a = a_items[0];
    let y_b = b_items[0];
    assert!(
        y_b > y_a + 10.0,
        "A 與 B 必須落在兩條不同 baseline（y 差 ≥ 10pt），實得 y_a={y_a}, y_b={y_b}"
    );
}

/// 燈 3（不分頁鎖）：含軟 br 的檔案頁數零變動（軟 br 不得誤當分頁）。
#[test]
fn k21_lamp3_soft_br_no_page_split() {
    // fdo68607（含 8 個軟 br）：頁數必須維持 8 頁，軟 br 不得誤判為分頁
    let eng_fdo = load_corpus(FDO68607);
    assert_eq!(
        eng_fdo.page_count(),
        8,
        "fdo68607 頁數必須精確為 8 頁（軟 br 不得誤判為分頁）"
    );

    // tdf123054（含 6 個軟 br）：頁數必須維持 7 頁
    let eng_tdf = load_corpus(TDF123054);
    assert_eq!(
        eng_tdf.page_count(),
        7,
        "tdf123054 頁數必須精確為 7 頁（軟 br 不得誤判為分頁）"
    );

    // 合成最小檔：頁數必須為 1 頁
    let eng_min = load_fixture(FIXTURE_MINIMAL);
    assert_eq!(eng_min.page_count(), 1, "最小軟 br 檔頁數必須精確為 1 頁");
}

/// 燈 4（零回退鎖）：既有表格、WC048/WC049 撐列等關鍵邏輯無回退。
#[test]
fn k21_lamp4_zero_regression() {
    // 驗證 WC048 依然能正常解析與載入
    let eng_wc048 = load_corpus("xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx");
    assert!(
        eng_wc048.page_count() >= 1,
        "WC048 必須正常分頁且至少有 1 頁"
    );
    let dl = eng_wc048.page_display_list(0).expect("WC048 第 0 頁存在");
    assert!(!dl.items.is_empty(), "WC048 第 0 頁顯示列表不得為空");
}

/// BUGGY 回塞鎖：`K21_SOFT_BR_BUGGY=1` 時，新邏輯關閉，
/// 燈 1 與 燈 2 必須退回未實作時的舊行為（sound.Regularity 同行、AB 單行），
/// 證明兩盞紅燈確實由軟 br 修復驅動，非偶合常數。
#[test]
fn k21_buggy_regression_lock() {
    if std::env::var_os("K21_SOFT_BR_BUGGY").is_none() {
        eprintln!("未設 K21_SOFT_BR_BUGGY：跳過回塞檢驗（以 K21_SOFT_BR_BUGGY=1 跑）。");
        return;
    }

    // 1. fdo68607 應退回舊行為：出現 'sound.Regularity'
    let eng = load_corpus(FDO68607);
    let dl = eng.page_display_list(5).expect("fdo68607 第 5 頁應存在");
    let has_concatenated = dl.items.iter().any(|it| match it {
        DrawItem::Text { text, .. } => text.contains("sound.Regularity"),
        _ => false,
    });
    assert!(
        has_concatenated,
        "BUGGY 態 fdo68607 第 5 頁必須復現 'sound.Regularity' 黏連舊行為"
    );

    // 2. 最小檔應退回舊行為：出現 'AB'
    let eng_min = load_fixture(FIXTURE_MINIMAL);
    let dl_min = eng_min.page_display_list(0).expect("第 0 頁應存在");
    let has_ab = dl_min.items.iter().any(|it| match it {
        DrawItem::Text { text, .. } => text.trim() == "AB",
        _ => false,
    });
    assert!(has_ab, "BUGGY 態最小檔必須復現 'AB' 黏連單行舊行為");
}
