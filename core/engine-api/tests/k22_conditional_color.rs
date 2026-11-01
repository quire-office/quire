//! K22 紅燈：`w:tblStylePr` 條件**字色**級聯——表頭白字落地（fdo68607 header）。
//!
//! ## 缺陷（卡面 K22）
//! `corpus/ooxmlexport/fdo68607.docx` 表頭列（Drama Vocabulary／Definition）：
//! LO 渲染＝藍底（#4F81BD）**白字**；我方（K22 前）渲染＝藍底**黑字**
//! （SVG 實測 text fill=#000000）。
//!
//! ## 根因
//! 表格 style＝`MediumShading1-Accent1`：其 `w:tblStylePr w:type="firstRow"`
//! 同時宣告 `w:tcPr/w:shd fill=4F81BD`（底色）與 `w:rPr/w:color val=FFFFFF`
//! （字色）。K8-shd 級聯只提取了 `w:shd`（底色層），`w:rPr/w:color` 字色層
//! 從未接進級聯——條件字色在解析層就被丟掉。
//!
//! ## 處方（本檔鎖的行為）
//! 條件字色與條件底色**同軸**：同 `w:tblLook` 生效閘（[`compute_cond_gates`]
//! 同源）、同 ECMA-376 §17.4.75 優先序（firstRow < lastRow < firstCol < …
//! 逐屬性級聯非整條件覆蓋）；優先序末端：直接 rPr 字色 ＞ 條件字色。
//! 資料流：`core/ooxml/src/styles.rs`（解析直系 `w:rPr`）→
//! `table_map::cond_run_colors`（逐段落查表）→ `cascade_paragraphs`
//! （級聯鏈完全未宣告顏色時烘焙 `Run::color`）→ 發射端既有著色鏈。
//!
//! ## 紅燈口徑
//! - 燈1（真檔）：fdo68607 p0 表頭「Drama Vocabulary」／「Definition」發射
//!   color_argb＝0xFFFFFFFF（非 0xFF000000）。
//! - 燈2（合成最小檔 `fixtures/k22_cond_color.docx`）：firstRow 紅字（FF0000）
//!   白底——表頭 H2 紅、資料列維持預設黑（級聯只作用在 firstRow 列、不外溢）；
//!   同時鎖 firstCol 藍字（0000FF）命中 R1C1／R2C1（實作不得只特化
//!   firstRow）、band1Vert／lastCol 的字色宣告被 `w:tblLook w:val="04A0"`
//!   （noVBand=1／lastColumn=0）閘死不得發黃／發白。
//! - 燈3（直接字色覆蓋鎖）：H1（firstRow∩firstCol 交會格）run 直接
//!   color=00AA00 → 直接色勝條件色（綠，非紅非藍——firstRow 優先序亦高於
//!   firstCol，三色互斥，級聯順序寫錯必紅其一）。
//! - 零回退鎖：fdo68607 表頭藍底 0xFF4F81BD（K8 燈2 同尺）＋ R1C1／R1C2
//!   橫帶 D3DFEE 底色仍在（字色層是擴充，底色級聯不得回退）。
//!
//! ## 回塞鎖
//! `K22_COND_COLOR_BUGGY=1 cargo test -p quire-engine --test k22_conditional_color`
//! → 燈1／燈2／燈3 的**條件色**必紅（退回黑），直接色 H1 仍綠（直接路徑
//! 與條件層機制分離）。測試進程內不設 env（避免平行污染），由外部執行驗證。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 corpus docx 失敗")
}

fn load_fixture(name: &str) -> Engine {
    let full = format!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/{}"),
        name
    );
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 fixtures/{name}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 fixture docx 失敗")
}

const FDO68607: &str = "ooxmlexport/fdo68607.docx";
const FIXTURE: &str = "k22_cond_color.docx";

const BLACK: u32 = 0xFF00_0000;
const WHITE: u32 = 0xFFFF_FFFF;
const RED: u32 = 0xFFFF_0000;
const BLUE: u32 = 0xFF00_00FF;
const GREEN: u32 = 0xFF00_AA00;

/// 頁內發射文字項：(文字, color_argb)。
fn text_items(eng: &Engine, page: usize) -> Vec<(String, u32)> {
    let dl = eng.page_display_list(page).expect("頁存在");
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text {
                text, color_argb, ..
            } => Some((text.clone(), *color_argb)),
            _ => None,
        })
        .collect()
}

/// 找首個文字**恰為** `want` 的發射項顏色；查無＝None。恰等匹配（非
/// contains）：fdo68607 p0 標題「IGCSE Drama Glossary」同樣含 "Drama"，
/// contains 口徑會先命中黑字標題（2026-11 K22 實測踩到的假紅）。
fn color_of_exact(items: &[(String, u32)], want: &str) -> Option<u32> {
    items.iter().find(|(t, _)| t == want).map(|(_, c)| *c)
}

/// 頁內填色 Rect（排除圖片佔位灰）。
fn fill_rects(eng: &Engine, page: usize) -> Vec<(f32, f32, f32, f32, u32)> {
    let dl = eng.page_display_list(page).expect("頁存在");
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } if *fill_argb != 0xFFF0_F0F0 => Some((*x, *y, *w, *h, *fill_argb)),
            _ => None,
        })
        .collect()
}

/// 燈1（真檔）：fdo68607 p0 表頭文字著色＝白（LO 同渲：藍底白字）。
/// 修前現況＝0xFF000000 黑字（SVG 實測），本燈修前必紅。
#[test]
fn k22_lamp1_fdo68607_header_white() {
    let eng = load_corpus(FDO68607);
    let items = text_items(&eng, 0);
    let drama = color_of_exact(&items, "Drama").expect("p0 應有 'Drama' 表頭文字");
    let def = color_of_exact(&items, "Definition").expect("p0 應有 'Definition' 表頭文字");
    assert_eq!(
        drama, WHITE,
        "表頭 'Drama Vocabulary' 應為 firstRow 條件白字（LO 藍底白字），實得 #{drama:08X}"
    );
    assert_eq!(
        def, WHITE,
        "表頭 'Definition' 應為 firstRow 條件白字（LO 藍底白字），實得 #{def:08X}"
    );
    assert_ne!(drama, BLACK, "表頭不得是黑字（K22 前缺陷態）");
    assert_ne!(def, BLACK, "表頭不得是黑字（K22 前缺陷態）");
}

/// 燈1b（零回退鎖）：fdo68607 表頭藍底（K8 firstRow shd）仍在——字色層
/// 是擴充，底色級聯既有行為不得回退。
#[test]
fn k22_lamp1b_fdo68607_header_fill_intact() {
    let eng = load_corpus(FDO68607);
    let want: u32 = 0xFF4F_81BD;
    let near = |c: u32| -> bool {
        let d = |s: u32| (((c >> s) & 0xFF) as i32 - ((want >> s) & 0xFF) as i32).abs();
        c >> 24 == 0xFF && d(0) <= 2 && d(8) <= 2 && d(16) <= 2
    };
    let hit = fill_rects(&eng, 0)
        .into_iter()
        .any(|(_, _, _, _, argb)| near(argb));
    assert!(
        hit,
        "fdo68607 p0 應仍有 firstRow 藍底 ≈4F81BD（K8 燈2 同尺；K22 不得回退底色級聯）"
    );
}

/// 燈2（合成最小檔）：firstRow 紅字只作用在表頭列、不外溢；firstCol 藍字
/// 走同軸級聯（非 firstRow 特化）；band1Vert／lastCol 字色被 tblLook 閘死。
#[test]
fn k22_lamp2_fixture_first_row_only() {
    let eng = load_fixture(FIXTURE);
    assert_eq!(eng.page_count(), 1, "合成最小檔應恰 1 頁");
    let items = text_items(&eng, 0);

    // 表頭 H2（無直接色）＝firstRow 條件紅字。
    assert_eq!(
        color_of_exact(&items, "H2"),
        Some(RED),
        "firstRow 條件紅字必須命中表頭 H2（級聯層從未接字色＝修前紅燈態）"
    );
    // 資料列第 2 欄（非 firstCol、非 firstRow）維持預設黑——firstRow
    // 條件字色不外溢到非表頭列。
    for needle in ["R1C2", "R2C2"] {
        assert_eq!(
            color_of_exact(&items, needle),
            Some(BLACK),
            "資料列 {needle} 必須維持預設黑（firstRow 條件字色不得外溢到非表頭列）"
        );
    }
    // firstCol（縱軸條件，所有列的第 1 欄）走同一級聯：R1C1／R2C1 藍字。
    for needle in ["R1C1", "R2C1"] {
        assert_eq!(
            color_of_exact(&items, needle),
            Some(BLUE),
            "firstCol 條件藍字必須命中 {needle}（實作不得只特化 firstRow）"
        );
    }
    // tblLook 閘：band1Vert（FFFF00）／lastCol（FFFFFF）宣告存在但
    // noVBand=1／lastColumn=0 閘死——任何發射項都不得出現黃／白字。
    for (t, c) in &items {
        assert_ne!(*c, 0xFFFFFF00, "{t:?} 出現 band1Vert 黃字——noVBand 閘失效");
        assert_ne!(
            *c, WHITE,
            "{t:?} 出現 lastCol 白字——lastColumn 閘失效（本檔 lastRow/lastColumn 全關）"
        );
    }
}

/// 燈2b（零回退鎖）：合成檔 band1Horz 底色（D3DFEE）仍在——同節點收字色
/// 後，`w:shd` 底色收錄不得回退（逐條件獨立：firstCol 無 shd 續條 band1Horz）。
#[test]
fn k22_lamp2b_fixture_band_fill_intact() {
    let eng = load_fixture(FIXTURE);
    let d3: u32 = 0xFFD3_DFEE;
    let hit = fill_rects(&eng, 0)
        .into_iter()
        .any(|(_, _, _, _, argb)| argb == d3);
    assert!(
        hit,
        "合成檔 band1Horz 橫帶 D3DFEE 底色必須仍在（firstCol 未宣告 shd 時續條下一條件——逐屬性級聯）"
    );
}

/// 燈3（直接字色覆蓋鎖）：H1 為 firstRow∩firstCol 交會格，條件色紅（firstRow
/// 優先序最高）與藍（firstCol）皆在宣告，但 run 帶直接 color=00AA00 →
/// 直接色勝（ECMA-376 §17.4.75：直接格式＞條件格式）。
#[test]
fn k22_lamp3_direct_color_wins() {
    let eng = load_fixture(FIXTURE);
    let items = text_items(&eng, 0);
    assert_eq!(
        color_of_exact(&items, "H1"),
        Some(GREEN),
        "H1 直接字色 00AA00 必須勝條件字色（紅=firstRow 誤勝／藍=firstCol 誤勝/級聯順序錯皆紅）"
    );
}

/// 回塞鎖（外部跑 `K22_COND_COLOR_BUGGY=1 cargo test -p quire-engine
/// --test k22_conditional_color`）：條件字色層整滅——燈1／燈2／燈3 的
/// **條件色**退回黑（必紅態復現），直接色 H1 仍綠（機制分離：直接路徑
/// 不受回塞變數影響）。未設變數時跳過（平行測試進程零污染）。
#[test]
fn k22_buggy_regression_lock() {
    if std::env::var_os("K22_COND_COLOR_BUGGY").is_none() {
        eprintln!("未設 K22_COND_COLOR_BUGGY：跳過回塞檢驗（以 K22_COND_COLOR_BUGGY=1 跑）。");
        return;
    }
    let eng = load_corpus(FDO68607);
    let items = text_items(&eng, 0);
    assert_eq!(
        color_of_exact(&items, "Drama"),
        Some(BLACK),
        "BUGGY 態 fdo68607 表頭必須退回黑字（證明燈1由條件字色級聯驅動，非偶合）"
    );

    let eng = load_fixture(FIXTURE);
    let items = text_items(&eng, 0);
    assert_eq!(
        color_of_exact(&items, "H2"),
        Some(BLACK),
        "BUGGY 態合成檔 H2 必須退回黑字"
    );
    assert_eq!(
        color_of_exact(&items, "R1C1"),
        Some(BLACK),
        "BUGGY 態合成檔 R1C1 必須退回黑字（firstCol 條件層同滅）"
    );
    assert_eq!(
        color_of_exact(&items, "H1"),
        Some(GREEN),
        "BUGGY 態 H1 直接字色仍綠——回塞只滅條件層，直接路徑機制分離"
    );
}
