//! E 卡（t_c2bbcb38）紅燈考卷：表格儲存格文字——打字進了引擎、畫面看不見。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 卡面缺口（PM 於 merged main 4c0d532 用真引擎獨立復測坐實）
//! 對照組：body 段打字 → `paragraphText` 有字且顯示列表 `Text` 命中。
//! 實驗組：末段後插 2x2 表 → 游標進 `tables_summary_json().para_start` 段落打字
//! → 打字 ok=true、`paragraph_text` 回讀有字，但全頁顯示列表 `Text` **零命中**。
//!
//! # 真實根因（讀碼坐實，非推測）
//! 切片文字 `CellFrag::texts[].text` 是**建槽當時的斷行快照**（`layout_table`
//! pass 1 把 `CellBlock::Text` 斷成 `Line` 的字串副本，`fragment` 再拷貝）。
//! 打字走 `Mutation::InsertText` → `Engine::replace_paragraph_text` →
//! `LayoutEngine::replace_paragraph_text`，該函式只改 `texts[para]` 並重斷該段；
//! 表格內部段落依「流失權」不進正文流，函式原本在找不到 `FlowItem::Text` 時
//! **直接 return**（原註解自認「切片文字由 engine 重建槽時更新」——但打字路徑
//! 不重建槽，`Mutation::InsertText` 的 `structural=false` 也不觸發
//! `rebuild_derived`）→ 切片永遠帶舊字串。資料面（OOXML＋canonical model）與
//! 發射面就此分岔：存檔重開後 DL 有字（實測），記憶體態 DL 零發射。
//!
//! # 本考的斷言層
//! 排版層（本檔，直接斷 `TableLayout::retext_block` 與切片輸出）＋
//! 真引擎端對端（`scripts/e_celltext_dl_probe.mjs`，斷顯示列表命中／引擎回讀）
//! ＋ U4d 檢查 5 補回 DL 斷言。三層皆禁讀自報旗標。
//!
//! # 鑑別力
//! `E_CELLDL_BUGGY=1` 把「刷新後切片必須含新字」的比對目標換成永不相等的假字
//! ⇒ 燈 1／燈 2 必紅，證明斷言真在比對切片文字而非恆真（比照本庫 `*_BUGGY`
//! 回塞鎖口徑；實跑兩態記卡面回報）。

use quire_layout::table::{layout_table, CellBlock, CellInput, RowInput, TableInput, TableLayout};
use quire_layout::{LayoutEngine, LayoutParams, TableSlot, TextMeasurer};

fn meas() -> TextMeasurer {
    TextMeasurer::new(12.0)
}

/// 2x2 表（格寬 100pt），第 0 格給 `t0`，其餘空格。
fn table_2x2(t0: &str) -> TableInput {
    let cell = |txt: &str| CellInput {
        blocks: vec![CellBlock::Text(txt.to_string())],
        ..Default::default()
    };
    TableInput {
        grid_pt: vec![100.0, 100.0],
        rows: vec![
            RowInput {
                cells: vec![cell(t0), cell("")],
                ..Default::default()
            },
            RowInput {
                cells: vec![cell(""), cell("")],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

/// 全切片文字（依列序／格序／行序展平）。
fn frag_texts(tl: &TableLayout) -> Vec<String> {
    let (frag, _, _) = tl.fragment(0, 0, u32::MAX, false, 0.0);
    frag.rows
        .iter()
        .flat_map(|r| r.cells.iter())
        .flat_map(|c| c.texts.iter().map(|t| t.text.clone()))
        .collect()
}

fn buggy() -> bool {
    std::env::var_os("E_CELLDL_BUGGY").is_some()
}

/// 回塞態下的比對目標：BUGGY 時換成永不命中的假字（該燈必紅＝鑑別力證明）。
fn want(new_text: &str) -> String {
    if buggy() {
        format!("{new_text}\u{FFFF}")
    } else {
        new_text.to_string()
    }
}

// ---------------------------------------------------------------------------
// 燈 1：缺口本體——格內文字刷新後，切片必須帶出新字（修前無此能力＝紅）
// ---------------------------------------------------------------------------
#[test]
fn lamp1_retext_refreshes_fragment_text() {
    let mut tl = layout_table(&table_2x2(""), 400.0, 16.8, &meas(), 0);
    // 前提：刷新前切片第 0 格是空字串（快照態）。
    let before = frag_texts(&tl);
    assert!(
        before.iter().all(|t| t.is_empty()),
        "前提：新建 2x2 表切片應全空字串（實得 {before:?}）"
    );

    assert!(
        tl.retext_block(0, "", "己"),
        "retext_block 應受理（第 0 個文字 block 現值為空字串）"
    );
    let after = frag_texts(&tl);
    assert!(
        after.contains(&want("己")),
        "缺口本體：格內打字後切片文字必須帶出「己」（實得 {after:?}）——\
         資料面已更新、發射面零發射就是用戶端「打字沒反應」的 S1 根因"
    );
    // 其餘格不受連坐影響（仍各持一條空行——空格佔一行的既有契約）。
    assert_eq!(
        after.iter().filter(|t| t.is_empty()).count(),
        3,
        "其餘三格仍應是空行（實得 {after:?}）"
    );
}

// ---------------------------------------------------------------------------
// 燈 2：列高跟隨——打字把格內撐成多行時，列高與後續列必須一起長（證明走的是
//        完整排版管線，不是「只把字串塞進舊幾何」的捷徑）
// ---------------------------------------------------------------------------
#[test]
fn lamp2_row_height_grows_with_typed_text() {
    let long = "開源原生文書處理器目標是不卡頓的專業排版開源原生文書處理器目標是不卡頓的專業排版";
    let mut tl = layout_table(&table_2x2(""), 400.0, 16.8, &meas(), 0);
    let bands_before = tl.total_bands;
    let row0_before = tl.row_units[0];
    assert!(tl.retext_block(0, "", long), "長文刷新應受理");
    assert!(
        tl.total_bands > bands_before,
        "格內長文必須撐高整表（帶數 {bands_before} → {}）",
        tl.total_bands
    );
    assert!(
        tl.row_units[0] > row0_before,
        "承載長文的第 1 列帶數必須增長（row_units[0] {row0_before} → {}）",
        tl.row_units[0]
    );
    // 後續列必須**整體下移**（列高不是原地膨風——用戶看的第 2 列文字得讓位）。
    let (frag, _, _) = tl.fragment(0, 0, u32::MAX, false, 0.0);
    let row1_first_text_y = frag.rows[1]
        .cells
        .iter()
        .flat_map(|c| c.texts.iter())
        .map(|t| t.y_pt)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    let row1_start_y = frag.rows[0].height_pt;
    assert!(
        row1_start_y > 16.8 * 1.5,
        "第 1 列切片高必須隨內容增長（實得 {row1_start_y}pt）"
    );
    assert!(
        row1_first_text_y.abs() < 0.01,
        "RowFrag 高度口徑：第 2 列切片內文字 y 相對本列頂（實得 {row1_first_text_y}）"
    );

    // 文字守恆（斷行可變、字不能變）：把該格所有行拼回來必須逐字元等於原文。
    let (frag, _, _) = tl.fragment(0, 0, u32::MAX, false, 0.0);
    let joined: String = frag.rows[0].cells[0]
        .texts
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(joined, want(long), "折行後拼接必須逐字元等於原打字內容");
    let lines = frag.rows[0].cells[0].texts.len();
    assert!(lines > 3, "100pt 寬格內 40 字應折多行（實得 {lines} 行）");
}

// ---------------------------------------------------------------------------
// 燈 3：索引身分驗證——序號命中但現值對不上（索引漂移）時必須一格都不動。
//        「打字打到錯的格子」比「看不見」更壞，這條是防呆紅線。
// ---------------------------------------------------------------------------
#[test]
fn lamp3_identity_mismatch_is_a_noop() {
    let mut tl = layout_table(&table_2x2("原有內容"), 400.0, 16.8, &meas(), 0);
    let before = frag_texts(&tl);
    // 現值是「原有內容」，呼叫端卻以為是別的（段落索引平移／槽已被別人改過）。
    assert!(
        !tl.retext_block(0, "我以為是這個", "錯字"),
        "現值不符必須回 false"
    );
    assert_eq!(
        frag_texts(&tl),
        before,
        "現值不符時切片必須逐字不變（絕不把字寫進錯的格子）"
    );
    // 序號越界同態：誠實 noop，不虛構幾何。
    assert!(
        !tl.retext_block(9_999, "原有內容", "錯字"),
        "越界序號必須回 false"
    );
    assert_eq!(frag_texts(&tl), before, "越界後切片仍須逐字不變");
    // 新舊同字＝實質無變動，回 false（不白重排）。
    assert!(
        !tl.retext_block(0, "原有內容", "原有內容"),
        "同字刷新必須回 false"
    );
    assert_eq!(frag_texts(&tl), before, "同字刷新後切片須逐字不變");
}

// ---------------------------------------------------------------------------
// 燈 4：巢狀表——巢狀表的格內打字同樣必須發射（巢狀表在快照內原位遞迴，
//        由外層重排自然重建，不另開第二條路徑）。
// ---------------------------------------------------------------------------
#[test]
fn lamp4_nested_table_cell_text_refreshes() {
    let inner = TableInput {
        grid_pt: vec![40.0],
        rows: vec![RowInput {
            cells: vec![CellInput {
                blocks: vec![CellBlock::Text(String::new())],
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let input = TableInput {
        grid_pt: vec![200.0],
        rows: vec![RowInput {
            cells: vec![CellInput {
                blocks: vec![CellBlock::Table(Box::new(inner))],
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut tl = layout_table(&input, 400.0, 16.8, &meas(), 0);
    // block 序：外層無文字 block → 巢狀表內第 0 個就是目標。
    assert!(tl.retext_block(0, "", "內格"), "巢狀表格內刷新應受理");
    let (frag, _, _) = tl.fragment(0, 0, u32::MAX, false, 0.0);
    let nested_texts: Vec<String> = frag.rows[0].cells[0]
        .nested
        .iter()
        .flat_map(|(_, f)| f.rows.iter())
        .flat_map(|r| r.cells.iter())
        .flat_map(|c| c.texts.iter().map(|t| t.text.clone()))
        .collect();
    assert!(
        nested_texts.contains(&want("內格")),
        "巢狀表格內打字必須在巢狀切片帶出（實得 {nested_texts:?}）"
    );
}

// ---------------------------------------------------------------------------
// 燈 5：引擎面整合——`LayoutEngine::replace_paragraph_text` 對表格內部段落
//        不再靜默 noop：切片文字必須跟隨新文字（流失權段落走槽刷新路徑）。
// ---------------------------------------------------------------------------
#[test]
fn lamp5_layout_engine_retexts_table_slot() {
    let p = LayoutParams::default();
    let texts: Vec<String> = (0..4).map(|i| format!("正文段{i}")).collect();
    // 槽內快照必須與 `texts[3]` 同值——真實建槽路徑（`table_map::table_input`）
    // 就是把 `texts[index]` 原樣塞進 `CellBlock::Text`， fixture 不同值會把
    // 「身分驗證」燈測成假紅（真引擎端對端另有 e_celltext_dl_probe 兜底）。
    let mut tl = layout_table(&table_2x2("正文段3"), 400.0, p.line_h_pt(), &meas(), 0);
    assert!(
        tl.retext_block(0, "正文段3", "己"),
        "前提：refresh 能力可用"
    );
    let slot = TableSlot {
        para_start: 3,
        para_end: 4,
        layout: layout_table(&table_2x2("正文段3"), 400.0, p.line_h_pt(), &meas(), 0),
    };
    let mut eng = LayoutEngine::with_tables(&texts, p, Vec::new(), vec![slot]);
    let cell_texts = |e: &LayoutEngine| -> Vec<String> {
        e.pages()
            .iter()
            .flat_map(|pg| pg.tables.iter())
            .flat_map(|tp| tp.frag.rows.iter())
            .flat_map(|r| r.cells.iter())
            .flat_map(|c| c.texts.iter().map(|t| t.text.clone()))
            .collect()
    };
    assert!(
        cell_texts(&eng).contains(&"正文段3".to_string()),
        "前提：切片初始帶的是建槽快照（實得 {:?}）",
        cell_texts(&eng)
    );
    // 表格內部段落（para_start 那一段）打字：流失權、不在正文流。
    let evs = eng.replace_paragraph_text(3, "己");
    let got = cell_texts(&eng);
    assert!(
        got.contains(&want("己")),
        "缺口本體（引擎面）：表格內部段落打字後切片必須帶出「己」（實得 {got:?}）"
    );
    assert!(
        evs.iter().any(|e| format!("{e:?}").starts_with("Page")),
        "刷新必須回報重繪事件，否則 shell 仍不會重畫（實得 {evs:?}）"
    );
    // 引擎回讀真值側同步行（發射與回讀不得分岔）。
    assert_eq!(eng.paragraph_text(3), Some("己"), "layout 工作副本同步更新");
}

// ---------------------------------------------------------------------------
// 燈 6：跨頁切片——打字撐高的格內文字被切成多片時，逐片拼接必須逐字元等於
//        原文（不丟行、不重複行）。刷新走完整排版管線，切片器才會跟著長。
// ---------------------------------------------------------------------------
#[test]
fn lamp6_cross_page_slices_keep_every_line() {
    let src = "開源原生文書處理器目標是不卡頓的專業排版".repeat(8);
    let mut tl = layout_table(&table_2x2(""), 400.0, 16.8, &meas(), 0);
    assert!(tl.retext_block(0, "", &src), "長文刷新應受理");
    // 以 4 帶為「一頁預算」逐片取完（切片器契約：游標必然前進）。
    let mut joined = String::new();
    let mut cursor = (0usize, 0u32);
    let mut guard = 0usize;
    loop {
        guard += 1;
        assert!(guard < 200, "切片迴圈必須收斂（卡在 {guard} 片）");
        let (frag, next, used) = tl.fragment(cursor.0, cursor.1, 4, false, 0.0);
        if used == 0 {
            break;
        }
        for t in frag
            .rows
            .iter()
            .flat_map(|r| r.cells.iter())
            .flat_map(|c| c.texts.iter())
        {
            joined.push_str(&t.text);
        }
        if next == cursor {
            break;
        }
        cursor = next;
        if cursor.0 >= tl.row_count() {
            break;
        }
    }
    assert_eq!(
        joined,
        want(&src),
        "跨切片拼接必須逐字元等於打字內容（缺行＝用戶翻頁後看不到後段）"
    );
}
