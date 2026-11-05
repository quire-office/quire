//! E1：分頁缺口兩席紅燈考卷（LO 25.8 ground truth，pymupdf 實測）。
//!
//! # 卡面前提實測更正（三條全部與檔案位元組不符，PM 須先讀這段）
//!
//! 1. **`corpus/ooxmlexport/cell-grid-span.docx` 檔案內 `w:gridSpan` 計數＝0**
//!    （`unzip -p … | grep -c gridSpan` → 0；同檔 `w:vMerge`＝0）。該檔唯一一張表是
//!    **單列 5 格全空**（`w:tblGrid` 5 欄：16/15/15/15/11321 twip，每格一個空段、
//!    `w:hideMark`）。卡面「源檔是跨列表格、跨頁切片未觸發、高度估算歸零」的前提
//!    不成立——該檔沒有任何跨列儲存格可走錯分支。
//! 2. **跨列表格切片後欄寬與合併不變形（卡面燈 2）在合成表格上實測為真**：見
//!    `lamp2_gridspan_slice_geometry_is_locked`——3 列切片 ×6 片，每片每格
//!    `(x_pt, w_pt)` 逐位元組相同（82.0/80.0/200.0，跨列格 82.0＝前 3 欄和）。
//!    故此燈是**正向鎖**（比照 `k25_empty_page_parity.rs` 的純鎖定位），不是待修缺陷。
//! 3. **CoverPage 的 `Quire`／`Corpus` 不是「多發」，是「缺發」**：LO p0 實測**渲染了**
//!    `Quire Corpus`（`NewDocxDocuments_CoverPage.pdf` p0 第 5 行 bbox
//!    `(285.8, 526.5, 344.5, 539.9)`）。我方同位發射的是 `Eric White`（bbox
//!    `(281.5, 526.7, …)`）——該檔 `Title`/`Subtitle`/`Author` 三個 `w:sdt` 都是
//!    `w:dataBinding` 內容控制（`xpath=/ns0:coreProperties[1]/ns1:creator[1]`，
//!    `docProps/core.xml` 的 `dc:creator`＝`Quire Corpus`），LO 以文件屬性值覆寫
//!    佔位run，我方原樣發射 run 內的 `Eric White`。render_lab 報告亦把它列在
//!    「真缺字」而非多發欄。卡面燈 4 若照字面寫成「token 集合不含 Quire/Corpus」
//!    會把 LO 真值鎖死成錯誤行為，故本檔反轉極性：**必須含 `Quire`/`Corpus`，
//!    且不得含 `Eric White`**（後者才是真多發）。
//!
//! # 現行碼實拍（本分支未動碼，基線 main@1cc26b3）
//!
//! ```text
//! $ ./target/debug/quire-cli info corpus/ooxmlexport/cell-grid-span.docx
//! 頁數: 1   （LO 2 頁）
//! $ ./target/debug/quire-cli info corpus/xopt/NewDocxDocuments_CoverPage.docx
//! 頁數: 1   （LO 2 頁）
//! ```
//!
//! ## 燈號狀態（PM 掛帳制；比照 `r34_placeholder_fill.rs` 燈 5 的 `#[ignore]` 慣例）
//!
//! 燈 1（兩盞）、燈 3（兩盞）、燈 4 共**五盞**標 `#[ignore]`：它們測的是**本卡準改面
//! 之外**的缺陷（autospacing／族表在 `core/ooxml/`＋engine 行高表；dataBinding 覆寫在
//! `core/ooxml/`），本卡若讓它們紅在 `cargo test --workspace`，等於把零回退鐵條
//! 的閘門自己弄髒。解 ignore 的條件寫在各自燈的 `#[ignore = "..."]` 字串裡。
//! 燈 2 是正向鎖（現行碼即綠、`E1_GRIDSPAN_EXPECT_BUGGY=1` 證鑑別力）；燈 3b
//! （`..._second_page_is_physically_blank`）是形態鎖，但形態鎖在頁數還錯時無從驗證，
//! 故與燈 3a 一同 ignore，待頁數燈可綠時一併解鎖。
//!
//! ```text
//! $ cargo test -p quire-engine --test e1_pagination
//! test result: ok. 1 passed; 0 failed; 5 ignored
//! $ cargo test -p quire-engine --test e1_pagination -- --ignored
//! test result: FAILED. 0 passed; 5 failed   ← 這五盞是欠債，修復卡須使其轉綠
//! ```
//!
//! 五盞欠債燈的真實成因（逐條定量見 `docs/e1-redlight.md`）皆**不在本卡準改面**：
//! - cell-grid-span：`w:beforeAutospacing`/`w:afterAutospacing` 未實作（實測 LO 對
//!   每個 autospacing 段落對加 14.0pt，本檔 8 段宣告）＋ `Trebuchet MS` 不在
//!   `face_line_factor` 族表（我方吃後備 1.1499 ⇒ 該組行距 11.5pt，LO 實測
//!   15.6／15.7pt ⇒ real-LH ≈1.56，既非 Noto Serif 1.3620 亦非任何現存表值）。
//!   兩者合起來使我方總豎向流量比 LO 少排 113.1pt（LO 全篇 693.6pt／我方 580.5pt，
//!   實測）——解析處在 `core/ooxml/src/lib.rs`，族表處在 engine 側 `face_line_factor`。
//! - CoverPage：LO 第 2 頁是**物理性空白頁**（0 words／0 drawings／0 images），
//!   消融實驗證明其存廢取決於「尾端 `w:br type="page"` ＋ 同檔存在錨定圖形」的
//!   組合（抽掉圖形段 ⇒ LO 掉回 1 頁；br 後補一段 ⇒ LO 3 頁），非分頁容量誤判。
//!   另一席（`Eric White` 覆寫）需讀 `docProps/core.xml`，落在 `core/ooxml/`。

use quire_engine::{DrawItem, Engine};

fn load(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

const GRID_SPAN: &str = "ooxmlexport/cell-grid-span.docx";
const COVER: &str = "xopt/NewDocxDocuments_CoverPage.docx";

/// 該頁全部非空文字 item（顯示列表序）。
fn page_texts(eng: &Engine, page: usize) -> Vec<String> {
    let dl = eng
        .page_display_list(page)
        .unwrap_or_else(|| panic!("第 {page} 頁顯示列表應存在"));
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

/// 正規化：壓空白、全小寫、統一連結號（LO 吐 U+2019、我方同 U+2019，此處仍
/// 做防呆；`-`/`–`/`—` 一律折成 `-`）。
fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .replace(['\u{2019}', '\u{2018}'], "'")
        .replace(['\u{2013}', '\u{2014}'], "-")
}

// ---------------------------------------------------------------------------
// 燈 1：cell-grid-span 頁數與 p1 首末行（LO 真值）
// ---------------------------------------------------------------------------
//
// LO 真值（pymupdf，`cell-grid-span.pdf`，612×792，兩頁）：
// - p0：33 個文字行（去重後 32 個 y0），首行 y0=85.1 `On Valentine's Day, it's
//   perfect to gift the iBall Andi 3.5`，末行 y0=677.8／y1=693.0 `country.`。
//   版心下界 792−1440twip=720pt，p0 末行距下界尚餘 27.0pt。
// - p1：5 個文字行、1 條 drawing、0 images；首行 y0=71.5
//   `For further media information, contact:`，末行 y0=142.1／y1=157.2
//   `E:redacted@example.invalid`。LO 斷點成因＝該段帶 w:beforeAutospacing，
//   LO 對其加 14.0pt 段前距後越過 720pt 下界（定量見 docs/e1-redlight.md §2.1）。

#[test]
#[ignore = "欠債：頁數差 1 頁的真因（autospacing 未實作＋Trebuchet 族表缺失）落在 core/ooxml/ 與 engine 行高表，本卡準改面（core/layout/、table_map.rs、發射組裝）動不到。見 docs/e1-redlight.md §3.1(A)(B)"]
fn lamp1_cell_grid_span_page_count_matches_lo() {
    let eng = load(GRID_SPAN);
    assert_eq!(
        eng.page_count(),
        2,
        "cell-grid-span：LO 2 頁（p0 末行 y1=693.0、p1 五行 y0=71.5–142.1），我方 \
         {} 頁——真因是 autospacing＋Trebuchet 族表缺失導致總流量少排 ~90pt，\
         不是表格跨頁切片",
        eng.page_count()
    );
}

#[test]
#[ignore = "欠債：同 lamp1 頁數燈（我方無 p1 可查），待 autospacing＋族表修復卡解鎖"]
fn lamp1_cell_grid_span_second_page_first_last_line() {
    let eng = load(GRID_SPAN);
    if eng.page_count() < 2 {
        panic!(
            "cell-grid-span：我方僅 {} 頁，無 p1 可查（LO p1 首行 `For further media \
             information, contact:`／末行 `E:redacted@example.invalid`）",
            eng.page_count()
        );
    }
    let lines = page_texts(&eng, 1);
    let first = norm(lines.first().expect("我方 p1 首行不得為空"));
    let last = norm(lines.last().expect("我方 p1 末行不得為空"));
    assert_eq!(
        first,
        norm("For further media information, contact:"),
        "我方 p1 首行須對 LO p1 首行（y0=71.5）"
    );
    assert_eq!(
        last,
        norm("E:redacted@example.invalid"),
        "我方 p1 末行須對 LO p1 末行（y0=142.1／y1=157.2）"
    );
    assert_eq!(
        lines.len(),
        5,
        "LO p1 恰 5 行（contact／Claire George／M:09833043394／L: 022-32229646／\
         E:redacted@example.invalid），我方 p1 行數須同：{lines:?}"
    );
}

// ---------------------------------------------------------------------------
// 燈 2：跨列表格切片後欄寬與合併不變形（正向鎖；標的檔零 gridSpan，故用合成表）
// ---------------------------------------------------------------------------
//
// 卡面指定用 `cell-grid-span.docx`，但該檔 `w:gridSpan` 計數＝0（見檔頭更正 1），
// 無從驗證「跨列切片變形」；本燈改以 `quire_layout::table` 直接構造 5 欄網格
// （80/1/1/80/200pt）＋首格 `grid_span=3` 的 30 列高表，逼出多片跨頁切片，逐片
// 斷言每格 `(x_pt, w_pt)` 與首片逐位元組全等。
//
// 鑑別力證明（比照 `k25_empty_page_parity.rs` 的常數自證口徑，非引擎回塞鎖——
// 本卡動不了引擎，沒有回塞態可關）：`E1_GRIDSPAN_EXPECT_BUGGY=1` 把期望常數換成
// 「跨列格退化為單欄寬 80.0」，實際引擎輸出仍是 82.0 ⇒ 本燈必紅，證明斷言真的
// 在比幾何、不是恆真。

#[test]
fn lamp2_gridspan_slice_geometry_is_locked() {
    use quire_layout::table::{CellBlock, CellInput, RowInput, TableInput, WidthSpec};
    use quire_layout::TextMeasurer;

    let mut input = TableInput {
        grid_pt: vec![80.0, 1.0, 1.0, 80.0, 200.0],
        width: Some(WidthSpec::Pt(362.0)),
        ..Default::default()
    };
    for r in 0..30 {
        input.rows.push(RowInput {
            cells: vec![
                CellInput {
                    grid_span: 3,
                    blocks: vec![CellBlock::Text(format!(
                        "row {r} spanned cell text long enough to wrap over lines"
                    ))],
                    ..Default::default()
                },
                CellInput {
                    blocks: vec![CellBlock::Text(format!("c{r}"))],
                    ..Default::default()
                },
                CellInput {
                    blocks: vec![CellBlock::Text("x".repeat(40))],
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
    }
    let tl = quire_layout::table::layout_table(&input, 468.0, 13.3, &TextMeasurer::new(10.0), 0);
    let buggy = std::env::var_os("E1_GRIDSPAN_EXPECT_BUGGY").is_some();

    // 期望幾何：跨列格＝前 3 欄和 82.0，其餘格依網格序 80.0／200.0。
    let want: Vec<(f32, f32)> = if buggy {
        vec![(0.0, 80.0), (82.0, 80.0), (162.0, 200.0)]
    } else {
        vec![(0.0, 82.0), (82.0, 80.0), (162.0, 200.0)]
    };

    let mut cur = (0usize, 0u32);
    let mut slices = 0usize;
    while cur.0 < tl.row_count() {
        let (frag, next, used) = tl.fragment(cur.0, cur.1, 20, false, 72.0);
        assert!(used > 0, "切片必須有帶數（第 {slices} 片）");
        assert_ne!(next, cur, "游標必須前進，否則 packer 死循環");
        for rf in &frag.rows {
            assert_eq!(rf.cells.len(), 3, "每列 3 格（首格跨 3 欄）");
            for (ci, c) in rf.cells.iter().enumerate() {
                assert!(
                    (c.x_pt - want[ci].0).abs() < 0.01 && (c.w_pt - want[ci].1).abs() < 0.01,
                    "切片 {slices} 列#{} 格#{ci} 幾何 ({},{}) 偏離首片基準 ({},{})\
                     ——跨列切片後欄寬/合併不得變形",
                    rf.row_index,
                    c.x_pt,
                    c.w_pt,
                    want[ci].0,
                    want[ci].1
                );
            }
        }
        cur = next;
        slices += 1;
        assert!(slices <= 8, "切片數上限防護（30 列／20 帶預算）");
    }
    assert!(
        slices >= 5,
        "30 列高表必須真的跨多片才有意義，實得 {slices} 片"
    );
}

// ---------------------------------------------------------------------------
// 燈 3：CoverPage 頁數（LO 第 2 頁＝物理性空白頁，須老實開頁）
// ---------------------------------------------------------------------------
//
// LO 真值：`NewDocxDocuments_CoverPage.pdf` 2 頁；p1 `lines=[]`、`words=0`、
// `drawings=0`、`images=0`（比照 n751077/K25 口徑：真空頁照實開頁，不得用「刪頁」
// 湊分數）。

#[test]
#[ignore = "欠債：LO 第 2 頁是尾端 w:br type=\"page\" 遇錨定圖形的產物（消融實證，docs/e1-redlight.md §3.2），判頁邏輯屬 core/layout/ 但本卡無準改動可觸及該組合，留待專卡"]
fn lamp3_coverpage_page_count_matches_lo() {
    let eng = load(COVER);
    assert_eq!(
        eng.page_count(),
        2,
        "CoverPage：LO 2 頁（p1 為物理性空白頁：0 words／0 drawings／0 images），\
         我方 {} 頁——LO 空白頁存廢取決於「尾端 w:br type=\"page\" ＋ 同檔錨定圖形」\
         組合（消融：抽掉圖形段 ⇒ LO 1 頁；br 後補段 ⇒ LO 3 頁）",
        eng.page_count()
    );
}

#[test]
#[ignore = "欠債：形態鎖，須先有 p1 才可驗證，隨 lamp3 頁數燈一併解鎖"]
fn lamp3_coverpage_second_page_is_physically_blank() {
    let eng = load(COVER);
    if eng.page_count() < 2 {
        panic!(
            "CoverPage：我方僅 {} 頁，無從驗證 p1 零墨跡（LO p1 0 words／0 drawings）",
            eng.page_count()
        );
    }
    let dl = eng.page_display_list(1).expect("我方 p1 存在");
    let ink: Vec<&str> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { text, .. } if !text.trim().is_empty() => Some("text"),
            DrawItem::Text { .. } => None,
            DrawItem::Rect { .. } => Some("rect"),
            DrawItem::Image { .. } => Some("image"),
            DrawItem::Rule { .. } => Some("rule"),
        })
        .collect();
    assert!(
        ink.is_empty(),
        "LO p1 是真空頁（零文字／零 drawing／零 image），我方多開的 p1 必須同形零墨跡，\
         不得虛構內容去騙相似度：{ink:?}"
    );
}

// ---------------------------------------------------------------------------
// 燈 4：CoverPage 發射 token 對 LO p0 真值（缺發＋真多發各一）
// ---------------------------------------------------------------------------
//
// LO p0 五個文字行（pymupdf bbox，單位 pt）：
// | 行 | bbox |
// |---|---|
// | `This is my interesting abstract.` | (325.7, 218.4, 461.6, 231.8) |
// | `The Title is` | (285.8, 280.8, 482.3, 333.3) |
// | `here` | (285.8, 329.9, 364.1, 382.4) |
// | `And a subtitle` | (285.8, 386.9, 391.5, 410.2) |
// | `Quire Corpus` | (285.8, 526.5, 344.5, 539.9) |
//
// 我方現行（`quire-cli dump`）：前四行全數存在，第 5 行發 `Eric White`
// （281.5, 526.7），`Quire`／`Corpus` **零出現** ⇒ 缺發；`Eric White` ⇒ 多發。

#[test]
#[ignore = "欠債：dataBinding 覆寫需讀 docProps/core.xml＋xpath 求值，解析面在 core/ooxml/（本卡嚴禁面）；見 docs/e1-redlight.md §3.3"]
fn lamp4_coverpage_emits_data_bound_tokens_and_no_placeholder_run() {
    let eng = load(COVER);
    let dl = eng.page_display_list(0).expect("p0 存在");
    let joined = norm(
        &dl.items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
    );
    // (a) LO p0 正文 token 零遺漏——含被 dataBinding 覆寫的 `Quire Corpus`。
    for want in [
        "this is my interesting abstract.",
        "the title is",
        "here",
        "and a subtitle",
        "quire corpus",
    ] {
        assert!(
            joined.contains(want),
            "LO p0 實測有 `{want}`（Author/Title/Subtitle 為 w:dataBinding 內容控制，\
             dc:creator＝`Quire Corpus`），我方發射清單完全沒有它＝缺發"
        );
    }
    // (b) 佔位 run 文字不得發射：LO 以文件屬性值覆寫 sdtContent 的佔位 run，
    //     `Eric White` 在 LO 全文零出現——它才是本檔的多發 token。
    assert!(
        !joined.contains("eric white"),
        "`Eric White` 是 dataBound 內容控制的佔位 run 原文，LO p0/p1 全文零出現\
         （LO 該位發 `Quire Corpus`）——我方原樣發射＝多發，須抑制"
    );
}
