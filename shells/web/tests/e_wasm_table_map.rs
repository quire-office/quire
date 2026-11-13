//! E（卡 t_f91e5773）紅燈考卷（wasm 殼面）：表格索引綁定＋段落→表格路徑反查。
//!
//! 本檔鎖 `shells/web/src/lib.rs` 的 **WasmEngine 表格讀取面**——
//! `table_count`／`tables_json`／`tables_summary_json`／`table_locator_json`
//! 四條綁定在 wasm 邊界的可用性。引擎側符號早在 Phase3-1a 就有
//! （`core/engine-api`），但殼層零綁定（U4 報告 §3）→ 外殼只能靠
//! 「越界探針讀拒絕訊息」反推張數。本卡把真索引交回 JS。
//!
//! 目标檔：`fixtures/table_multi.docx`（本卡新產，gen_multi_table_fixture.py）——
//! **三張 body 級根表**（T0 四列含 gridSpan/vMerge/巢狀表、T1 一列、T2 兩列）。
//! 單表檔（table_mixed.docx）測不到「點哪張改哪張」，只作零回退對照。
//!
//! 七盞：
//! - 燈 1（計數真值）：殼 `table_count()==3`＝直接引擎面同值（薄殼透傳），
//!   且 ≠1（探針降級序的假設值——多表檔上探針會給錯答案）。
//! - 燈 2（表格樹透傳）：殼 `tables_json()` 與 `Engine::tables_json()` 逐字
//!   相同；可 parse；三根表的 `para_start` 單調不重疊。
//! - 燈 3（摘要面）：殼 `tables_summary_json()` 逐項對上 canonical 事實
//!   （root_slot 序、列數、網格欄數、含巢狀表數）。
//! - 燈 4（游標反查＝本卡核心）：逐段反查落點斷言**逐格手核**（fixture 檔頭
//!   結構表）：T0/T1/T2 各自 root_slot、巢狀段落回 nested_steps=[[0,2,0]]、
//!   表外段落回 None（不是錯誤）。
//! - 燈 5（路徑→Mutation 直通鎖）：反查產物餵 `apply_mutation_json` 的
//!   SetTableProps 真改到**目標那張**表（改 T1 不碰 T0/T2——多表文件
//!   「點哪張改哪張」的端到端證據，走殼的 JSON 邊界零第二輪翻譯）。
//! - 燈 6（panic 面）：越界段落索引／usize::MAX 反查回 None 不 panic
//!   （wasm 邊 panic＝瀏覽器 abort）。
//! - 燈 7（零回退鎖）：`fixtures/table_mixed.docx`（單表）在四條新綁定下
//!   計數/樹/反查仍與改前語義一致；`page_display_list_json(0)` 照舊存在。

use quire_web::WasmEngine;
use serde_json::Value;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> Vec<u8> {
    let p = repo_root().join("fixtures").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取 fixture 失敗 {}: {e}", p.display()))
}

fn multi() -> WasmEngine {
    WasmEngine::new(&fixture("table_multi.docx")).expect("table_multi.docx 必須開檔成功")
}

fn json(s: String) -> Value {
    serde_json::from_str(&s).expect("殼輸出必須是合法 JSON（嚴禁字串拼接）")
}

/// 反查 helper：殼 JSON → (root_slot, nested_steps, cell_path 欄)。
fn locate(e: &WasmEngine, para: usize) -> Option<Value> {
    e.table_locator_json(para).map(json)
}

// ---------------------------------------------------------------------------
// 燈 1：計數真值——殼＝直接引擎面，且多表檔上不等於探針假設值
// ---------------------------------------------------------------------------

#[test]
fn lamp1_table_count_matches_engine_and_is_multi() {
    let shell = multi();
    let bytes = fixture("table_multi.docx");
    let direct = quire_engine::Engine::from_docx_bytes(&bytes).expect("直接面開檔");
    assert_eq!(
        shell.table_count(),
        direct.table_count(),
        "殼與直接引擎面計數必須一致（薄殼透傳）"
    );
    assert_eq!(shell.table_count(), 3, "fixture 是三張 body 級根表");
    assert_ne!(
        shell.table_count(),
        1,
        "多表檔上『單表用 0』降級序的假設值必須不成立——這正是探針路徑給不出真索引的證明"
    );
}

// ---------------------------------------------------------------------------
// 燈 2：表格樹透傳——殼輸出與直接面逐字相同；根表 para_start 單調不重疊
// ---------------------------------------------------------------------------

#[test]
fn lamp2_tables_json_passthrough_verbatim_and_monotonic() {
    let shell = multi();
    let bytes = fixture("table_multi.docx");
    let direct = quire_engine::Engine::from_docx_bytes(&bytes).expect("直接面開檔");
    assert_eq!(
        shell.tables_json(),
        direct.tables_json(),
        "殼 tables_json 與直接面必須逐字相同（零業務邏輯）"
    );
    let v = json(shell.tables_json());
    let roots = v.as_array().expect("根表陣列");
    assert_eq!(roots.len(), 3);
    let starts: Vec<usize> = roots
        .iter()
        .map(|t| t["para_start"].as_u64().expect("para_start 存在") as usize)
        .collect();
    for w in starts.windows(2) {
        assert!(w[0] < w[1], "根表 para_start 必須嚴格單調（{starts:?}）");
    }
}

// ---------------------------------------------------------------------------
// 燈 3：摘要面——逐項對上 canonical 事實
// ---------------------------------------------------------------------------

#[test]
fn lamp3_summary_surface_matches_canonical_facts() {
    let shell = multi();
    let v = json(shell.tables_summary_json());
    let rows = v.as_array().expect("摘要陣列");
    assert_eq!(rows.len(), 3, "一張根表一項");
    // root_slot 序＝陣列序（JS 端直接 index 用）。
    for (i, r) in rows.iter().enumerate() {
        assert_eq!(r["root_slot"].as_u64(), Some(i as u64), "root_slot 對號");
    }
    // T0：4 列、3 網格欄、含巢狀 2 張、格內段落 12（根表 10＋巢狀 2）。
    assert_eq!(rows[0]["rows"], 4);
    assert_eq!(rows[0]["grid_cols"], 3);
    assert_eq!(rows[0]["table_count"], 2, "T0 含 1 巢狀表");
    assert_eq!(rows[0]["para_cells"], 12);
    // T1：1 列 2 欄無巢狀；T2：2 列 2 欄。
    assert_eq!(rows[1]["rows"], 1);
    assert_eq!(rows[1]["grid_cols"], 2);
    assert_eq!(rows[1]["table_count"], 1);
    assert_eq!(rows[2]["rows"], 2);
    assert_eq!(rows[2]["grid_cols"], 2);
    // 區間互斥（燈 2 單調性的強化版：前表 para_end ≤ 後表 para_start——表間無
    // 段落時取等號允許；本 fixture 三表相連且首表前無段落＝全段都在表內）。
    assert_eq!(rows[0]["para_start"], 0);
    let u = |v: &Value| v.as_u64().expect("數值欄位") as usize;
    assert!(u(&rows[0]["para_end"]) <= u(&rows[1]["para_start"]));
    assert!(u(&rows[1]["para_end"]) <= u(&rows[2]["para_start"]));
}

// ---------------------------------------------------------------------------
// 燈 4：游標反查（本卡核心）——逐格手核落點
// ---------------------------------------------------------------------------

/// fixture 結構（gen_multi_table_fixture.py 檔頭；段落由主解析器按文件序編號）：
/// 段 0,1 → T0.r0 的 c0/c1；段 2 → T0.r0c2（其後掛巢狀表：段 3,4＝巢狀 r0c0/c1）；
/// 段 5,6 → T0.r1（gridSpan=3 單格）；段 7 → T0.r2c0（vMerge restart）；
/// 段 8,9 → T0.r2c1/c2；r3c0 空 tc 零段；段 10,11 → T0.r3c1/c2；
/// 段 12,13 → T1.r0c0/c1；段 14..17 → T2 四格。
#[test]
fn lamp4_locator_resolves_every_paragraph_by_hand_verified_coords() {
    let e = multi();
    // 根表本體的段→ nested_steps 空、root_slot 對號。
    let l0 = locate(&e, 0).expect("段 0 在 T0");
    assert_eq!(l0["table_path"]["root_slot"], 0);
    assert_eq!(
        l0["table_path"]["nested_steps"].as_array().unwrap().len(),
        0
    );
    assert_eq!(l0["cell_path"]["row"], 0);
    assert_eq!(l0["cell_path"]["cell"], 0);
    // 段 2 掛 r0c2（巢狀表的宿主格本身）。
    let l2 = locate(&e, 2).expect("段 2 在 T0.r0c2");
    assert_eq!(
        (
            l2["cell_path"]["row"].as_u64(),
            l2["cell_path"]["cell"].as_u64()
        ),
        (Some(0), Some(2))
    );
    assert!(
        l2["table_path"]["nested_steps"]
            .as_array()
            .unwrap()
            .is_empty(),
        "宿主格段落＝根表命中"
    );

    // T0 r1 的跨欄格（gridSpan=3）：起點欄 0、跨 3。
    let l5 = locate(&e, 5).expect("段 5 在 T0.r1 跨欄格");
    assert_eq!(l5["cell_path"]["grid_col_start"], 0);
    assert_eq!(l5["cell_path"]["grid_span"], 3);
    assert_eq!(l5["cell_path"]["row"], 1);

    // vMerge continue 空格零段落：r3 只有 c1/c2 兩段可查（段 10,11），
    // 其 cell 座標是 tc 序 1/2（空 tc 佔 cell 0——tc 序≠網格欄，網格欄起點
    // 由 grid_col_start 給）。
    let l10 = locate(&e, 10).expect("段 10 在 T0.r3c1");
    assert_eq!(l10["cell_path"]["row"], 3);
    assert_eq!(l10["cell_path"]["cell"], 1, "tc 序：空 vMerge 格佔 0");
    assert_eq!(
        l10["cell_path"]["grid_col_start"], 1,
        "網格起點：前格 span=1 累算（空 tc 也佔 1 欄）"
    );

    // 巢狀表段（T0.r0c2 內掛）：nested_steps=[[0,2,0]]、cell_path 指巢狀表自身。
    let nested_para = (0..e.paragraph_count())
        .find(|p| {
            locate(&e, *p).is_some_and(|l| {
                !l["table_path"]["nested_steps"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            })
        })
        .expect("巢狀表段落必須存在且反查得出巢狀步");
    let ln = locate(&e, nested_para).unwrap();
    assert_eq!(ln["table_path"]["root_slot"], 0);
    assert_eq!(
        ln["table_path"]["nested_steps"],
        serde_json::json!([[0, 2, 0]])
    );
    assert_eq!(ln["cell_path"]["depth"], 1);
    assert_eq!(ln["cell_path"]["rows"], 1, "巢狀表 1 列");
    assert_eq!(ln["cell_path"]["grid_cols"], 2, "巢狀表 2 欄");

    // 跨根表：T1（root_slot 1）與 T2（root_slot 2）各自的段。
    let lt1 = (0..e.paragraph_count())
        .filter_map(|p| locate(&e, p))
        .find(|l| l["table_path"]["root_slot"] == 1)
        .expect("T1 段落必須反查得到");
    assert_eq!(lt1["cell_path"]["row"], 0);
    let lt2 = (0..e.paragraph_count())
        .filter_map(|p| locate(&e, p))
        .find(|l| l["table_path"]["root_slot"] == 2)
        .expect("T2 段落必須反查得到");
    assert_eq!(lt2["cell_path"]["rows"], 2);

    // 本 fixture 無表外段落（全部段落都在表內）→ 「表外回 None」由燈 4b 的
    // sample.docx（純正文檔）與燈 6 越界共同封頂。
}

/// 燈 4b：無表文件（sample.docx）反查恆 None——**None 不是錯誤**，外殼據此
/// 走正文編輯路徑。
#[test]
fn lamp4b_body_paragraphs_locate_to_none_not_error() {
    let e = WasmEngine::new(&fixture("sample.docx")).expect("sample.docx 開檔");
    assert_eq!(e.table_count(), 0, "sample.docx 無表");
    for p in 0..e.paragraph_count() {
        assert!(
            e.table_locator_json(p).is_none(),
            "無表文件的段落必須反查為 None（非錯誤），段 {p}"
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 5：路徑→Mutation 直通——反查產物直接餵 M4 改到目標那張表
// ---------------------------------------------------------------------------

#[test]
fn lamp5_located_path_feeds_mutation_targeting_the_right_table() {
    let mut e = multi();
    // 取 T1（root_slot 1）內任一段的反查產物。
    let hit = (0..e.paragraph_count())
        .filter_map(|p| e.table_locator_json(p).map(json))
        .find(|l| l["table_path"]["root_slot"] == 1)
        .expect("T1 段落存在");
    // 把 table_path 原封不動塞進 SetTableProps（零第二輪翻譯——欄位名逐字
    // 對齊就是要在這裡兌現）。
    let mutation = serde_json::json!({
        "SetTableProps": {
            "table": hit["table_path"],
            "patch": {"alignment": "center"}
        }
    });
    let res = json(e.apply_mutation_json(&mutation.to_string()));
    assert!(res["ok"].as_bool().unwrap_or(false), "M4.6 必須成功：{res}");
    // 效果斷言：T1 的 jc=center，T0/T2 不受波及（點哪張改哪張）。
    let tables = json(e.tables_json());
    for (slot, want) in [(0usize, false), (1, true), (2, false)] {
        let got = tables[slot]["table"]["props"]["justify"].as_str() == Some("center");
        assert_eq!(
            got,
            want,
            "root_slot {slot} 的對齊應{}被改",
            if want { "" } else { "不" }
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 6：panic 面——越界索引回 None 不 panic（wasm 邊 panic＝瀏覽器 abort）
// ---------------------------------------------------------------------------

#[test]
fn lamp6_out_of_range_inputs_never_panic() {
    let e = multi();
    assert!(e.table_locator_json(e.paragraph_count() + 999).is_none());
    assert!(
        e.table_locator_json(usize::MAX).is_none(),
        "usize::MAX 不得溢位 panic"
    );
    // 反查不受編輯後狀態影響的邊界：表數 0 的文件越界索引同口徑。
    let plain = WasmEngine::new(&fixture("sample.docx")).expect("sample 開檔");
    assert!(plain.table_locator_json(usize::MAX).is_none());
}

// ---------------------------------------------------------------------------
// 燈 7：零回退鎖——單表檔 table_mixed 語義不變＋顯示列表照舊
// ---------------------------------------------------------------------------

#[test]
fn lamp7_single_table_fixture_semantics_unchanged() {
    let shell = WasmEngine::new(&fixture("table_mixed.docx")).expect("table_mixed 開檔");
    assert_eq!(
        shell.table_count(),
        1,
        "Phase3-1a 帳：body 級一張（巢狀掛 tc 內）"
    );
    let v = json(shell.tables_json());
    assert_eq!(v[0]["table"]["grid"].as_array().unwrap().len(), 3);
    // 巢狀表段落反查→ nested_steps 非空且 root_slot 0。
    let nested = (0..shell.paragraph_count())
        .filter_map(|p| shell.table_locator_json(p).map(json))
        .find(|l| {
            !l["table_path"]["nested_steps"]
                .as_array()
                .unwrap()
                .is_empty()
        })
        .expect("table_mixed 的巢狀表段落必須反查得到巢狀步");
    assert_eq!(nested["table_path"]["root_slot"], 0);
    // 顯示列表零回退（本卡動了同一個 lib.rs）。
    assert!(
        shell.page_display_list_json(0).is_some(),
        "第 0 頁顯示列表不得消失"
    );
}
