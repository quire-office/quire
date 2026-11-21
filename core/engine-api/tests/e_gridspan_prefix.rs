//! E 返修卡 t_c1dbc6d7 **S2-2＋S3 紅燈考卷**：`w:gridSpan` 前綴 `w:val` 錯讀＋
//! 巢狀表／網格欄直通 M4 的覆蓋缺口。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 缺口（PM 讀碼＋corpus 掃描）
//! - `mutation_k29c::cell_grid_span` 舊實作對**整段 `<w:tcPr>`** 呼叫
//!   `attr_in_tag(raw,"w:val")`＝「本 tcPr 內第一個 `w:val`」，不是
//!   `w:gridSpan/@w:val`。ECMA-376 下 `w:tcBorders`／`w:shd`／`w:vMerge`／
//!   `w:cnfStyle` 皆可先於 `w:gridSpan` 宣告且自帶 `w:val`：
//!   `w:shd w:val="clear"`（列舉字串）⇒ `parse::<usize>()` 失敗 ⇒ 跨欄數錯算 **1**；
//!   `w:vMerge w:val="3"`（畸形數字 val）⇒ 直接把合併序号當成跨欄數（錯算 **3**）。
//! - `table_map.rs` 反查面走 canonical `CellProps::grid_span()`（精確讀
//!   `w:gridSpan/@w:val`）⇒ **同義兩制**：反查給的 `grid_col_start` 與 mutation
//!   側 `row_layout` 算出的 `col_start`/`span` 分岔，餵 `InsertTableColumn`／
//!   `DeleteTableColumn`／`MergeTableCells` 就操作錯欄。
//! - corpus 400 檔實測「`w:gridSpan` 之前出現其他 `w:val`」＝0 命中 ⇒ 實際視覺
//!   人群暫零，屬機率證據；但規格正確性仍屬 S2，故本考卷用**自製合成檔**鎖住。
//!
//! # 鑑別手法（不靠「不 panic 就綠」）
//! 反查面（canonical）與 mutation 面（raw）對同一格必須給**同一個**跨欄數。把
//! `w:gridSpan` 前綴污染格的 `grid_col_start` 當插入點發 `InsertTableColumn`：
//!   * 口徑一致（修後）⇒ 插入點落在該格**內部** ⇒ 引擎把該格 `gridSpan` 加 1、
//!     該列**格數不變**；
//!   * 口徑分岔（修前）⇒ 該格被錯算成 span=1，插入點變成「下一格的起點」⇒
//!     引擎改為**新增一格**，該列格數 +1、原格 `gridSpan` 不變。
//! 「列格數」與「該格 gridSpan」都是 `tables_json` 直接可讀的 canonical 事實，
//! 斷言的是使用者看得見的結果（欄有沒有插對地方），不是內部變數。
//!
//! # fixture（本卡新產，純 stdlib 合成、零內嵌圖片、零個資）
//! - `fixtures/table_gridspan_prefix.docx`（`scripts/gen_gridspan_prefix_fixture.py`）：
//!   3 網格欄；r0c0 前綴 `<w:shd w:val="clear"/>`＋`<w:gridSpan w:val="2"/>`；
//!   r1c0 前綴 `<w:vMerge w:val="3"/>`＋`<w:gridSpan w:val="2"/>`。
//! - `fixtures/table_nested_decoy.docx`（`scripts/gen_nested_decoy_fixture.py`）：
//!   宿主格內先一張 decoy 子表再一張目標子表（驗 `sub_table_idx`≠0 直通 M4）。
//! - `fixtures/table_gridspan_overflow.docx`：畸形 `w:gridSpan`（`u32::MAX`／
//!   `usize::MAX`）——wasm32 debug 的 `attempt to add with overflow` 即瀏覽器
//!   abort，故零 panic 是硬要求（S3-2）。
//!
//! # 鑑別力證明
//! `E_GRIDSPAN_BUGGY=1` 把燈 3 的期望值換成不可能值 ⇒ 該燈必紅（同支 binary
//! 切換＝斷言真在比對 canonical 事實）。實跑兩態記 docs。

use quire_engine::{
    CellPropsPatch, Engine, InsertSide, Mutation, SubDocTarget, TablePath,
};
use serde_json::Value;

const PREFIX_FIXTURE: &str = "fixtures/table_gridspan_prefix.docx";
const DECOY_FIXTURE: &str = "fixtures/table_nested_decoy.docx";
const OVERFLOW_FIXTURE: &str = "fixtures/table_gridspan_overflow.docx";
const MULTI_FIXTURE: &str = "fixtures/table_multi.docx";

fn open(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 {full} 失敗: {e}"));
    Engine::from_docx_bytes(&bytes).unwrap_or_else(|e| panic!("開啟 {rel} 失敗: {e}"))
}

fn json(s: String) -> Value {
    serde_json::from_str(&s).expect("引擎輸出必須是合法 JSON")
}

fn locate(e: &Engine, para: usize) -> Option<Value> {
    e.table_locator_json(para).map(json)
}

fn path_of(hit: &Value) -> TablePath {
    TablePath {
        target: SubDocTarget::Body,
        root_slot: hit["table_path"]["root_slot"].as_u64().expect("root_slot") as usize,
        nested_steps: hit["table_path"]["nested_steps"]
            .as_array()
            .expect("nested_steps 是陣列")
            .iter()
            .map(|s| {
                let a = s.as_array().expect("巢狀步是三元組");
                (
                    a[0].as_u64().expect("row") as usize,
                    a[1].as_u64().expect("cell") as usize,
                    a[2].as_u64().expect("sub") as usize,
                )
            })
            .collect(),
    }
}

/// `tables_json` 根表節點（`[{ "table": {...} }, ...]`）。
fn root_table(e: &Engine, slot: usize) -> Value {
    json(e.tables_json())[slot]["table"].clone()
}

/// 某列的格數（canonical 事實）。
fn row_cell_count(t: &Value, row: usize) -> usize {
    t["rows"][row]["cells"]
        .as_array()
        .expect("rows[row].cells 是陣列")
        .len()
}

/// 某格的有效跨欄數：直接吃 canonical `props.grid_span_raw`（缺＝1）。
fn canonical_span(cell: &Value) -> u32 {
    match cell["props"]["grid_span_raw"].as_str() {
        Some(s) => s.trim().parse::<u32>().unwrap_or(1).max(1),
        None => 1,
    }
}

/// 取「列 `row` 第 0 格」內段落的反查結果（本考卷 fixture 每格恰一段）。
fn locate_row0_cell0(e: &Engine, row: usize) -> (usize, Value) {
    for p in 0..e.paragraph_count() {
        if let Some(h) = locate(e, p) {
            if h["cell_path"]["row"].as_u64() == Some(row as u64)
                && h["cell_path"]["cell"].as_u64() == Some(0)
            {
                return (p, h);
            }
        }
    }
    panic!("列 {row} 第 0 格要有可反查段落");
}

fn insert_col(e: &mut Engine, table: TablePath, at_grid_col: usize) -> Result<(), String> {
    e.apply_mutation(Mutation::InsertTableColumn {
        table,
        at_grid_col,
        position: InsertSide::Before,
        width_pt: 20.0,
    })
    .map(|_| ())
    .map_err(|err| format!("{err:?}"))
}

// ---------------------------------------------------------------------------
// 燈 1：fixture 自證——污染前綴確實在檔（上游改掉形态時本燈先紅，不假綠）
// ---------------------------------------------------------------------------

#[test]
fn lamp1_prefix_contamination_is_real_in_the_file() {
    let e = open(PREFIX_FIXTURE);
    let xml = String::from_utf8(
        e.part("word/document.xml")
            .expect("要有 document.xml")
            .to_vec(),
    )
    .expect("UTF-8");

    // 兩段污染前綴必須在場（shd 字串 val／vMerge 數字 val）。
    let shd_at = xml
        .find(r#"<w:shd w:val="clear""#)
        .expect("fixture 要有 <w:shd w:val=\"clear\" 前綴");
    // 起點＝shd **自己**的開標籤結束處（它自帶 w:val，不得計入「其他 w:val」）。
    let shd_open_end = shd_at + xml[shd_at..].find('>').expect("shd 開標籤要有 '>'") + 1;
    let gs_after_shd = xml[shd_open_end..]
        .find(r#"<w:gridSpan w:val="2""#)
        .expect("shd 之後要有 <w:gridSpan w:val=\"2\"");
    let gs_at = shd_open_end + gs_after_shd;
    // 中間不得先出現別的 w:val（否則「第一個 w:val」就不是 shd 的，燈 3 失去指向）。
    assert!(
        !xml[shd_open_end..gs_at].contains("w:val="),
        "shd 與 gridSpan 之間不得有其他 w:val（本燈紅＝fixture 形态變了，燈 3 斷言需重推）"
    );

    let vm_at = xml
        .find(r#"<w:vMerge w:val="3""#)
        .expect("fixture 要有 <w:vMerge w:val=\"3\" 前綴（畸形數字 val）");
    let vm_open_end = vm_at + xml[vm_at..].find('>').expect("vMerge 開標籤要有 '>'") + 1;
    let gs_after_vm = xml[vm_open_end..]
        .find(r#"<w:gridSpan w:val="2""#)
        .expect("vMerge 之後要有 <w:gridSpan w:val=\"2\"");
    assert!(
        !xml[vm_open_end..vm_open_end + gs_after_vm].contains("w:val="),
        "vMerge 與 gridSpan 之間不得有其他 w:val"
    );

    // canonical 側（精確讀）必須把兩格都認成 span 2——這是燈 3 的基準真值。
    let t = root_table(&e, 0);
    for row in 0..2usize {
        let c0 = &t["rows"][row]["cells"][0];
        assert_eq!(
            canonical_span(c0),
            2,
            "canonical `CellProps::grid_span()` 對污染前綴格必須給 2（真值基準）"
        );
    }
    // 3 網格欄＋r0 只有兩格＝跨欄形態在場。
    assert_eq!(t["grid"].as_array().expect("tblGrid").len(), 3);
    assert_eq!(row_cell_count(&t, 0), 2, "r0 兩格（c0 跨 2 欄）");
}

// ---------------------------------------------------------------------------
// 燈 2：反查面（canonical）欄起點口徑——跨欄之後的格起點必須右移
// （本燈是燈 3 的前提；修前修後都該綠，故另由燈 6 BUGGY 門控證明是活的）
// ---------------------------------------------------------------------------

#[test]
fn lamp2_reverse_lookup_reports_canonical_grid_cursor() {
    let e = open(PREFIX_FIXTURE);
    let (p_span, hit_span) = locate_row0_cell0(&e, 0);
    assert_eq!(
        (
            hit_span["cell_path"]["grid_col_start"].as_u64(),
            hit_span["cell_path"]["grid_span"].as_u64()
        ),
        (Some(0), Some(2)),
        "污染前綴格（段 {p_span}）反查必須給 grid_col_start=0、grid_span=2\
         （canonical 精確讀 w:gridSpan/@w:val）"
    );

    // 同列下一格：起點＝前一格 span 累算＝2（不是 cell 索引 1）。
    let mut nxt: Option<Value> = None;
    for p in 0..e.paragraph_count() {
        if let Some(h) = locate(&e, p) {
            if h["cell_path"]["row"].as_u64() == Some(0)
                && h["cell_path"]["cell"].as_u64() == Some(1)
            {
                nxt = Some(h);
            }
        }
    }
    let nxt = nxt.expect("r0c1 要有可反查段落");
    assert_eq!(
        nxt["cell_path"]["grid_col_start"].as_u64(),
        Some(2),
        "跨欄之後的格：grid_col_start 必須＝2（網格起點 ≠ cell 索引，S3-5 缺口的根）"
    );
    assert_ne!(
        nxt["cell_path"]["grid_col_start"].as_u64(),
        nxt["cell_path"]["cell"].as_u64(),
        "本 fixture 必須真正做到 cell(1) != grid_col_start(2)（否則三層考卷測不到兩者不等）"
    );
}

// ---------------------------------------------------------------------------
// 燈 3（S2-2 核心紅燈）：反查欄起點餵 InsertTableColumn——mutation 側 row_layout
// 必須與 canonical 同口徑（修前：span 錯算 1／3 ⇒ 插入點錯位、該列多出一格）
// ---------------------------------------------------------------------------

/// 對前綴污染列發「在網格欄 1 之前插一欄」：
/// - 口徑一致 ⇒ 插入點在該格內部 ⇒ `gridSpan` 2→3、**列格數不變（2）**；
/// - 修前 shd 態（span 錯算 1）⇒ 插入點錯成下一格起點 ⇒ **列格數 3**、原格 2；
/// - 修前 vMerge 態（span 錯算 3）⇒ 插入點仍在格內但基準錯 ⇒ `gridSpan` 變 **4**。
fn assert_insert_inside_spanned_cell(row: usize, tag: &str) {
    let e0 = open(PREFIX_FIXTURE);
    let (para, hit) = locate_row0_cell0(&e0, row);
    let path = path_of(&hit);
    assert_eq!(
        hit["cell_path"]["grid_span"].as_u64(),
        Some(2),
        "{tag}：前提＝反查認該格跨 2 欄（燈 2 已鎖 canonical 真值）"
    );

    let mut e = open(PREFIX_FIXTURE);
    insert_col(&mut e, path, 1).unwrap_or_else(|err| panic!("{tag}：InsertTableColumn 失敗: {err}"));

    let t = root_table(&e, 0);
    let cells = row_cell_count(&t, row);
    let span = canonical_span(&t["rows"][row]["cells"][0]);
    let grid = t["grid"].as_array().expect("tblGrid").len();
    assert_eq!(grid, 4, "{tag}：tblGrid 必須新增 1 欄（3→4）");
    assert_eq!(
        cells, 2,
        "{tag}（段 {para}）：在跨欄格內部插欄必須**加寬該格**而非新增一格；\
         列格數 2→{cells} ⇒ mutation 側把 gridSpan 錯算（cell_grid_span 讀到前綴 w:val）"
    );
    assert_eq!(
        span, 3,
        "{tag}：該格 gridSpan 必須 2→3（錯算態會停在 2 或跳到 4）"
    );
}

#[test]
fn lamp3a_string_val_prefix_does_not_break_insert_column() {
    assert_insert_inside_spanned_cell(0, "shd w:val=\"clear\" 前綴");
}

#[test]
fn lamp3b_numeric_val_prefix_does_not_break_insert_column() {
    assert_insert_inside_spanned_cell(1, "vMerge w:val=\"3\" 前綴");
}

// ---------------------------------------------------------------------------
// 燈 4（S3-5）：巢狀表路徑直通 M4＋cell_path 餵 SetCellProps——正是讓 S2-2
// 漏網的那個覆蓋洞（端到端從前只測根表 SetTableProps）
// ---------------------------------------------------------------------------

#[test]
fn lamp4_nested_table_locator_path_feeds_m4_directly() {
    let e0 = open(DECOY_FIXTURE);
    // 目標子表 N1 的段落：其反查路徑必須帶 nested_steps=[(0,0,1)]（decoy 在序 0）。
    let mut target: Option<(usize, Value)> = None;
    for p in 0..e0.paragraph_count() {
        if let Some(h) = locate(&e0, p) {
            let steps = h["table_path"]["nested_steps"].as_array().unwrap();
            if steps.len() == 1
                && steps[0][2].as_u64() == Some(1)
                && h["cell_path"]["cell"].as_u64() == Some(1)
            {
                target = Some((p, h));
            }
        }
    }
    let (para, hit) = target.expect("目標子表 N1（sub_table_idx=1, cell=1）要可反查");
    let decoy_steps: Vec<Value> = (0..e0.paragraph_count())
        .filter_map(|p| locate(&e0, p))
        .filter(|h| {
            let s = h["table_path"]["nested_steps"].as_array().unwrap();
            s.len() == 1 && s[0][2].as_u64() == Some(0)
        })
        .collect();
    assert!(
        !decoy_steps.is_empty(),
        "decoy 子表（sub_table_idx=0）也要可反查（否則本燈測不到 decoy 波及）"
    );

    // 反查產物原封不動餵 M4.7 SetCellProps（零第二輪翻譯）。
    let mut e = open(DECOY_FIXTURE);
    e.apply_mutation(Mutation::SetCellProps {
        table: path_of(&hit),
        row: hit["cell_path"]["row"].as_u64().unwrap() as usize,
        cell: hit["cell_path"]["cell"].as_u64().unwrap() as usize,
        patch: CellPropsPatch {
            shading_fill: Some("FF0000".to_string()),
            ..Default::default()
        },
    })
    .unwrap_or_else(|err| panic!("巢狀表路徑餵 SetCellProps 必須成功（段 {para}）: {err:?}"));

    let root = root_table(&e, 0);
    let host_cells = root["rows"][0]["cells"].as_array().expect("r0 cells");
    let host_blocks = host_cells[0]["blocks"].as_array().expect("宿主格 blocks");
    let sub_tables: Vec<&Value> = host_blocks
        .iter()
        .filter_map(|b| b.as_object().and_then(|o| o.get("Table")))
        .collect();
    assert_eq!(sub_tables.len(), 2, "宿主格要有 decoy＋目標兩張子表");
    assert_eq!(
        sub_tables[1]["rows"][0]["cells"][1]["props"]["shading"]["fill"].as_str(),
        Some("FF0000"),
        "反查宣稱的目標子表 N1 必須被改到"
    );
    assert_ne!(
        sub_tables[0]["rows"][0]["cells"][0]["props"]["shading"]["fill"].as_str(),
        Some("FF0000"),
        "decoy 子表（sub_table_idx=0）不得被波及——錯位就是 sub_table_idx 口徑分岔"
    );
}

// ---------------------------------------------------------------------------
// 燈 5（S3-2）：畸形 w:gridSpan 不得在任何公開面上 panic（wasm abort 零容忍）
// ---------------------------------------------------------------------------

#[test]
fn lamp5_malformed_huge_gridspan_never_panics() {
    let mut e = open(OVERFLOW_FIXTURE);
    // 反查面全段掃描（走 table_map 的 col_cursor）。
    for p in 0..e.paragraph_count() + 3 {
        let _ = locate(&e, p);
    }
    let _ = e.tables_summary_json();
    let _ = e.tables_json();
    // mutation 面（row_layout 的 col += span）。
    let hit = (0..e.paragraph_count())
        .filter_map(|p| locate(&e, p))
        .find(|h| h["table_path"]["nested_steps"].as_array().unwrap().is_empty())
        .expect("溢位表要可反查");
    // 允許成功或誠實拒絕，唯獨不准 panic（本燈能跑到這裡就代表沒炸）。
    let _ = insert_col(&mut e, path_of(&hit), 1);
    let _ = e.apply_mutation(Mutation::DeleteTableColumn {
        table: path_of(&hit),
        grid_col: 1,
    });
    let _ = e.apply_mutation(Mutation::MergeTableCells {
        table: path_of(&hit),
        top_row: 0,
        bottom_row: 1,
        left_grid_col: 0,
        right_grid_col: 1,
    });
    let diff = e
        .roundtrip_check()
        .expect("溢位檔 roundtrip_check 不得炸");
    assert!(diff.pass, "溢位檔未編輯部分零失真: {:?}", diff.violations);
}

// ---------------------------------------------------------------------------
// 燈 6：鑑別力——BUGGY 期望門控（證明燈 3 的斷言真在比 canonical 事實）
// ---------------------------------------------------------------------------

#[test]
fn lamp6_buggy_gate_proves_lamp3_assertions_are_live() {
    let e0 = open(PREFIX_FIXTURE);
    let (_, hit) = locate_row0_cell0(&e0, 0);
    let mut e = open(PREFIX_FIXTURE);
    insert_col(&mut e, path_of(&hit), 1).expect("InsertTableColumn 必須成功");
    let t = root_table(&e, 0);
    let cells = row_cell_count(&t, 0);
    let span = canonical_span(&t["rows"][0]["cells"][0]);
    if std::env::var("E_GRIDSPAN_BUGGY").is_ok() {
        assert_eq!(
            (cells, span),
            (777, 777),
            "BUGGY 態：期望換成不可能值 ⇒ 此燈必須紅（實得 {cells}/{span}）"
        );
    } else {
        assert_eq!(
            (cells, span),
            (2, 3),
            "綠態：加寬斷言必須成立（實得列格數 {cells}、gridSpan {span}）"
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 7：零回退鎖——未污染的多表檔（含 gridSpan 合法用法）行為不變＋無損
// ---------------------------------------------------------------------------

#[test]
fn lamp7_clean_gridspan_files_behave_and_roundtrip() {
    let e0 = open(MULTI_FIXTURE);
    // 本檔必须至少有一格 cell != grid_col_start（S3-5 補的列），否則此燈測不到點。
    let divergent: Vec<(u64, u64)> = (0..e0.paragraph_count())
        .filter_map(|p| locate(&e0, p))
        .filter_map(|h| {
            let c = h["cell_path"]["cell"].as_u64()?;
            let g = h["cell_path"]["grid_col_start"].as_u64()?;
            (c != g).then_some((c, g))
        })
        .collect();
    assert!(
        !divergent.is_empty(),
        "table_multi 必須含 cell != grid_col_start 的案例（fixture 生成器被改回舊形時本燈紅）\
         ：實得 {divergent:?}"
    );

    for slot in 0..e0.table_count() {
        let (_, hit) = (0..e0.paragraph_count())
            .filter_map(|p| locate(&e0, p).map(|h| (p, h)))
            .find(|(_, h)| h["table_path"]["root_slot"].as_u64() == Some(slot as u64))
            .unwrap_or_else(|| panic!("root_slot {slot} 要有可反查段落"));
        let mut e = open(MULTI_FIXTURE);
        let path = path_of(&hit);
        insert_col(&mut e, path.clone(), 0).unwrap_or_else(|err| {
            panic!("合法 gridSpan 檔 root_slot {slot} 插欄不得失敗（零回退）: {err}")
        });
        assert_eq!(
            root_table(&e, slot)["grid"].as_array().unwrap().len(),
            root_table(&e0, slot)["grid"].as_array().unwrap().len() + 1,
            "root_slot {slot} 的 tblGrid 必須恰新增 1 欄"
        );
    }

    let diff = open(MULTI_FIXTURE)
        .roundtrip_check()
        .expect("roundtrip_check 失敗");
    assert!(diff.pass, "未編輯檔 roundtrip 必須通過: {:?}", diff.violations);
    assert!(diff.byte_identical, "未編輯檔必須 byte-identical");
}
