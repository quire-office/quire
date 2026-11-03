//! R33 紅燈：WC048／WC049 顯示列表文字序列與 LO PDF 發射序分岔。
//!
//! 標的：`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` p0、
//!       `corpus/xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx` p0。
//! LO 真值來源：LibreOffice 25.8 headless 轉 PDF，pymupdf 抽 span 流序
//! （`render_lab.pdf_page_facts` 同口徑）。
//!
//! # LO 實拍（pymupdf span 流序，非幾何序）
//!
//! WC048 p0（12 個 span）——表格 9 句「This is a test.」依**儲存格序**連續
//! 發射（首列 cell1 因 Textbox1 wrap 推到 y=112.2 仍排在該列第一位），
//! 三個浮動文字框（Textbox1／2／3）整層**最後**發射：
//!
//! | 序 | y | x | 文字 |
//! |---|---|---|---|
//! | 0–2 | 112.2 / 72.7 / 72.7 | 77.8 / 228.0 / 378.2 | This is a test. ×3（首列）|
//! | 3–5 | 138.6 | 77.8 / 228.0 / 378.2 | This is a test. ×3（第 2 列）|
//! | 6–8 | 345.2 | 77.8 / 228.0 / 378.2 | This is a test. ×3（第 3 列）|
//! | 9 | 90.7 | 64.8 | Textbox1 |
//! | 10 | 108.9 | 171.8 | Textbox2 |
//! | 11 | 194.8 | 86.7 | Textbox3 |
//!
//! WC049 p0（11 個 span）——正文 10 行依文序發完，最後才是浮動框
//! `Textbox1 this is a test.`（y=118.6，落在正文中段，LO 仍排最後）。
//!
//! 修前（`sort_display_items` 依幾何 (y,x) 單層排序）：WC048 的
//! Textbox1(y=90.4)／Textbox2(y=97.6) 插在首列文字中間、Textbox3(y=162.5)
//! 插在第二列與第三列之間；WC049 的 Textbox1(y=117.3) 插在正文中段。
//! 詞序逐個比 ⇒ render_lab 內容分停在 0.688（卡面目標 ≥0.80）。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

const WC048: &str = "xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx";
const WC049: &str = "xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx";

fn load(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

/// 顯示列表的非空文字項序列（保持顯示列表序＝抽取序），逐項正規化空白。
fn dump_texts(eng: &Engine, page: usize) -> Vec<String> {
    let dl = eng.page_display_list(page).expect("頁存在");
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { text, .. } => {
                let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
                (!t.is_empty()).then_some(t)
            }
            _ => None,
        })
        .collect()
}

/// 逐個詞序鍵（`render_lab.content_key` 口徑：逐行正規化後以單空格相接＋casefold）。
fn token_key(texts: &[String]) -> String {
    texts.join(" ").to_lowercase()
}

/// LO WC048 p0 span 序列（pymupdf 實拍，詞序逐個）。
const LO_WC048_P0: [&str; 12] = [
    "This is a test.",
    "This is a test.",
    "This is a test.",
    "This is a test.",
    "This is a test.",
    "This is a test.",
    "This is a test.",
    "This is a test.",
    "This is a test.",
    "Textbox1",
    "Textbox2",
    "Textbox3",
];

/// LO WC049 p0 span 序列（pymupdf 實拍；我方斷行位置不同屬版面軸，
/// 故比對口徑＝逐個詞序鍵，與 render_lab 內容分同議）。
const LO_WC049_P0_JOIN: &str = "video provides a powerful way to help you prove your point. \
when you click online video, you can paste in the embed code for the video you want to add. \
you can also type a keyword to search online for the video that best fits your document. \
to make your document look professionally produced, word provides header, footer, cover page, \
and text box designs that complement each other. for example, you can add a matching cover \
page, header, and sidebar. click insert and then choose the elements you want from the \
different galleries. textbox1 this is a test.";

/// 燈 1（修前必紅）：WC048 p0 顯示列表文字序列逐個等於 LO span 流序。
#[test]
fn r33_wc048_p0_text_sequence_matches_lo_stream() {
    let got = dump_texts(&load(WC048), 0);
    assert_eq!(
        got, LO_WC048_P0,
        "WC048 p0 顯示列表文字序列 ≠ LO pymupdf span 流序\
         （浮動文字框應整層排在表格文字之後，修前被 y-sort 打散交錯）"
    );
}

/// 燈 2（修前必紅）：WC049 p0 詞序鍵等於 LO；浮動框文字是整頁最後一項。
#[test]
fn r33_wc049_p0_text_sequence_matches_lo_stream() {
    let got = dump_texts(&load(WC049), 0);
    assert_eq!(
        token_key(&got),
        LO_WC049_P0_JOIN,
        "WC049 p0 詞序 ≠ LO pymupdf span 詞序（LO 把浮動框文字排在正文之後）"
    );
    assert!(
        got.last().is_some_and(|t| t.starts_with("Textbox1")),
        "WC049 p0 末項必須是浮動框文字（LO 真值：正文發完才發浮動層），實得 {:?}",
        got.last()
    );
}

/// 燈 3（零回退鎖）：fdo76591 的 VML 框文字與正文相對序維持 LO——
/// 兩個框標題（IMPORTANT NOTICE／Scheduled Saturday Overtime）先於正文
/// 首句 UNLESS…。修前修後皆綠：本燈鎖的是「浮動層調整不得把這檔的
/// inline VML 框推到正文之後」。
#[test]
fn r33_fdo76591_inline_vml_stays_before_body_flow() {
    if std::env::var_os("R33_LAYER_BUGGY").is_some() {
        eprintln!("R33_LAYER_BUGGY 已設：本燈為零回退鎖，回塞態不驗。");
        return;
    }
    let got = dump_texts(&load("ooxmlexport/fdo76591.docx"), 0);
    let pos = |needle: &str| got.iter().position(|t| t.contains(needle));
    let notice = pos("IMPORTANT NOTICE").expect("IMPORTANT NOTICE 必須發射");
    let sched = pos("Scheduled Saturday Overtime").expect("框內標題必須發射");
    let body = pos("UNLESS OTHERWISE NOTIFIED").expect("正文首句必須發射");
    assert!(
        notice < sched && sched < body,
        "fdo76591 相對序 ≠ LO（框標題先於正文）：notice@{notice} sched@{sched} body@{body}"
    );
}

/// 燈 4（回塞鎖）：`R33_LAYER_BUGGY=1` 關閉圖層主鍵 ⇒ 燈 1／燈 2 必紅。
/// 證明紅燈精確命中「浮動層圖層鍵」這一處修正，而非順手改到別處。
#[test]
fn r33_buggy_env_restores_red() {
    if std::env::var_os("R33_LAYER_BUGGY").is_none() {
        eprintln!("未設 R33_LAYER_BUGGY：本測試只驗回塞鎖（以 R33_LAYER_BUGGY=1 跑）。");
        return;
    }
    let got = dump_texts(&load(WC048), 0);
    assert_ne!(
        got, LO_WC048_P0,
        "回塞後 WC048 序列應重現修前 y-sort 交錯態（燈 1 必紅）"
    );
    let got49 = token_key(&dump_texts(&load(WC049), 0));
    assert_ne!(
        got49, LO_WC049_P0_JOIN,
        "回塞後 WC049 詞序應重現修前交錯態（燈 2 必紅）"
    );
}
