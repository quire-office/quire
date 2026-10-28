//! R7 紅燈：尾注／頁底註腳掛載到引用所在頁的頁腳區（poi form_footnotes）。
//!
//! 契約（K5-diag §2.2 處方／卡面紅燈）：`w:footnoteReference` 落在**表格格內
//! 段落**時，正文流行程沒有該段的行可定位（`find_anchor_line` 必敗 ⇒ 舊行為
//! 整條落「文件最後一頁」）。LO 口徑：註腳掛**引用首現頁**的頁腳區
//! （引用跨頁跟首現頁）。
//!
//! 天然 fixture：`corpus/poi/test-data_document_form_footnotes.docx`
//! （LO 3 頁；我方 4 頁＝簇 A 表格頁容量疊加，另行回報）。其唯一正文引用
//! `<w:footnoteReference w:id="2"/>` 在表格首列首格（pStyle=Header 段），
//! 表格跨 p1–p3 切片；註文 `testdoc test phrase` LO 畫在 **p1 頁腳**。
//! 修前實測：註文發在 p4 末兩行（`['1.', 'testdoc       test phrase']`）。
//!
//! 紅燈：
//! 1. `r7_note_attaches_to_anchor_page_footer`：註文「testdoc test phrase」
//!    出現在 **p1** 顯示列表末段（頁腳區 y），且最後一頁不再出現。
//! 2. `r7_page_count_not_worsened_by_anchoring`：掛載改动不新增頁（頁數
//!    守門＝4；卡面 LO 目標 3 頁之殘差屬簇 A unknown 帶容量，證據見提交說明，
//!    本卡掛載修復不得使頁數惡化）。
//! 3. buggy 回塞鎖：`R7_BUGGY=1` 關閉表格段→頁對位 ⇒ 紅燈 1 必紅
//!    （照 L2_BUGGY／R2_BUGGY／K4D_BUGGY 先例；實測見提交說明。測試進程內
//!    不设 env，避免平行執行緒污染）。
//!
//! 綠燈守門（既有行為零擾動）：
//! - `r7_single_page_cell_note_unchanged`：單頁檔（格內註腳、引用頁＝末頁）
//!   的註腳條目位置與修前逐值相同——對位改道不該動它們。
use quire_engine::Engine;
use quire_render_trait::DrawItem;

const FORM: &str = "poi/test-data_document_form_footnotes.docx";

fn load(path: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + path;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open")
}

/// 第 `page` 頁的非空 Text 項（顯示列表序）。
fn page_texts(eng: &Engine, page: usize) -> Vec<(f32, f32, String)> {
    let dl = eng.page_display_list(page).expect("頁存在");
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { x, y, text, .. } if !text.trim().is_empty() => {
                Some((*x, *y, text.trim().to_string()))
            }
            _ => None,
        })
        .collect()
}

/// 頁內是否出現含 `needle` 的文字項。
fn page_has(eng: &Engine, page: usize, needle: &str) -> bool {
    page_texts(eng, page)
        .iter()
        .any(|(_, _, t)| t.contains(needle))
}

/// 紅燈 1：註文掛在引用首現頁（p1）的頁腳區，且不再壓在最後一頁。
#[test]
fn r7_note_attaches_to_anchor_page_footer() {
    let eng = load(FORM);
    let last = eng.page_count() - 1;
    assert!(last >= 1, "本 fixture 應為多頁檔");

    // (a) p1 必須有註文本體（LO：p1 頁腳 `1 testdoc test phrase`）。
    let hits: Vec<(f32, f32, String)> = page_texts(&eng, 0)
        .into_iter()
        .filter(|(_, _, t)| t.contains("testdoc") || t.contains("test phrase"))
        .collect();
    assert!(
        !hits.is_empty(),
        "註文 'testdoc test phrase' 未出現在 p1（引用首現頁）——掛載仍壓末頁＝紅燈"
    );

    // (b) 頁腳區：註文 y 必須落在該頁版心底部（LO 頁腳語義；本檔版心底≈741.6，
    //     註腳條目在保留帶內 y≥700）。
    for (_, y, t) in &hits {
        assert!(
            *y > 690.0,
            "註文 {t:?} y={y} 不在 p1 頁腳區（應 ≥690，版心底 741.6）"
        );
    }

    // (c) 編號前綴條目（'1.'）同頁同區；且最後一頁不再有註文（不再壓末頁）。
    assert!(
        !page_has(&eng, last, "testdoc"),
        "註文仍在最後一頁 p{}＝掛載未改道",
        last + 1
    );
    assert!(page_has(&eng, 0, "1."), "p1 頁腳區必須有編號前綴條目 '1.'");
}

/// 紅燈 2（守門形）：掛載修復不新增頁。卡面 LO 目標 4→3；K6 校準 pitch
/// 落地後簇 A 的級帶過扣一併消失，頁數收斂到 LO 的 3 頁——本斷言鎖
/// 「掛載不使頁數惡化」，並以新的 LO 同頁數為準。
#[test]
fn r7_page_count_not_worsened_by_anchoring() {
    let eng = load(FORM);
    assert_eq!(
        eng.page_count(),
        3,
        "頁數偏離 3（LO 目標）：K6 校準 pitch 後應與 LO 同頁數"
    );
}

/// 綠燈守門：單頁檔的格內註腳（引用頁恰＝末頁）位置。K6 校準 pitch 改變
/// 正文行高，頁腳區隨之整體位移（仍為單頁、條目仍在頁腳區底部），故
/// 錨點改記 K6 後實測值。
#[test]
fn r7_single_page_cell_note_unchanged() {
    // poi/test-data_document_table_footnotes：1 頁；註文 'snoska' 條目
    // y≈769.6（K6 前 772.7；編號前綴 '1.' 因上標在 768.4）。
    let eng = load("poi/test-data_document_table_footnotes.docx");
    assert_eq!(eng.page_count(), 1);
    let hit = page_texts(&eng, 0)
        .into_iter()
        .find(|(_, _, t)| t.contains("snoska"))
        .expect("格內註腳條目必須保留");
    assert!(
        (hit.1 - 769.6).abs() < 1.0,
        "單頁格內註腳 y 位移：{} ≠ 769.6（K6 pitch 後實測）",
        hit.1
    );

    // ooxmlexport/tdf104354-2：1 頁；'Footnote text.' 條目 y≈526.3
    // （K6 前 524.5；LO 真值 528.9——細化後距 LO 由 4.4 收斂至 2.6，PM 實測錨定）。
    let eng = load("ooxmlexport/tdf104354-2.docx");
    assert_eq!(eng.page_count(), 1);
    let hit = page_texts(&eng, 0)
        .into_iter()
        .find(|(_, _, t)| t.contains("Footnote text."))
        .expect("格內註腳條目必須保留");
    assert!(
        (hit.1 - 526.3).abs() < 1.0,
        "單頁格內註腳 y 位移：{} ≠ 526.3（K6 pitch 後實測）",
        hit.1
    );
}
