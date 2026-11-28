//! R1-a 續（t_ba9bff8f）：`replay` 語意貫穿 K29c／K29d 分流臂——撤除 redo 護欄
//! 的兜底角色（跨家族重放鎖考卷）。
//!
//! 背景：R1-a（t_2ecfe8ca）把 `apply_mutation` 拆成公開新操作臂（`replay=false`
//! 照清 redo 棧）＋私有 `apply_mutation_inner(op, replay)`，重放臂跳結尾
//! `redo_stack.clear()`。但 K29c（表格／圖片族）與 K29d（節／頁首頁尾／註腳族）
//! 在 `apply_mutation_inner` **開頭就前置分派**、自行 `redo_stack.clear()`，
//! `replay` 旗標傳不進去；當時靠 `redo()` 呼叫端的**重放護欄**（`mem::take`
//! 保管剩餘棧→重放→成功／失敗皆放回）在行為面把全族兜住。使用者可見缺陷已癒，
//! 殘留的是**語意未貫穿**：三檔各自 clear、靠呼叫端護欄兜底。
//!
//! 判別軸（卡面鐵律：紅燈必須打**機制面**而非行為面）：現況護欄把缺陷兜住，
//! 光看行為無法分辨「旗標已貫穿」與「純靠護欄兜底」。故以**撤除護欄後行為是否
//! 仍正確**為軸——`R1A_SHIELD_BUGGY=1` 令護欄失效（主幹重放臂照樣不誤清），
//! 此時 K29c／K29d 重放是否誤清剩餘 redo 棧，完全取決於旗標有無貫穿分流臂。
//!
//! 盞別（本檔 5 盞）：
//! - 燈 1（跨家族重放鎖·K29c）：`k29c 編輯 → 主幹編輯 → k29c 編輯` 三筆→
//!   `undo()`×3→`redo()`×3 逐筆斷言：重放 **K29c 那筆時剩餘 redo 棧不被誤清**
//!   （`can_redo()` 仍 true、下一筆仍 `Some`）＋該筆效果如實回來＋走完
//!   `roundtrip_check()` pass（白名單僅 `word/document.xml`）。護欄撤除後須靠
//!   貫穿的旗標過——外部以 `R1A_SHIELD_BUGGY=1 R1A_UNTHREADED_BUGGY=1` 整跑時
//!   本燈必須翻紅（回塞鎖刻意**不**摘外部 env，見 `clear_foreign_env_lock`）。
//! - 燈 2（跨家族重放鎖·K29d）：`主幹 → k29d（InsertNote）→ 主幹`，K29d 那筆
//!   刻意放**中段**：重放它時棧上還有 1 筆待重放，誤清立刻翻紅。
//! - 燈 3（機制面鎖·撤除兜底）：單一測試內令護欄失效（`R1A_SHIELD_BUGGY=1`），
//!   K29c 序列**必須仍逐筆成功**——卡面完成條件「考卷要能在護欄撤除後仍然為真」。
//!   env 用 `EnvFlag` set／restore 成對（比 R1-a 的尾端 `remove_var` 更嚴：外部
//!   閘門整跑時不會把平行盞需要的變數誤清成假綠）。
//! - 燈 4（回塞鑑別鎖·三態對照）：①鏡像單翻（護欄在場）→ 仍全綠（實錄「現況靠
//!   護欄過」）；②護欄失效＋鏡像在場 → 第 2 次 redo 必 `Ok(None)`、`can_redo()`
//!   翻 false（逐字復刻修前世界＝缺陷形態實錄）；③護欄失效＋鏡像解除 → 逐筆
//!   成功。紅綠分岔由鏡像驅動，證明鎖鏈接到真缺陷路徑、考卷非假綠。
//! - 燈 5（新操作照清·反向鎖，iron rule 1）：K29c 家族 `undo→redo→再 k29c
//!   新操作`⇒`can_redo()==false`——貫穿旗標不得把「新操作斷鏈」順手搬走。
//!
//!
//! 本檔只依賴 engine 公開面（不 import 內部模組），fixture 由 repo 根相對讀取。
//!
//! 並行安全：五盞共用以 `R1F_ENV` 靜態互斥鎖 serialize——翻 env 的燈 3／燈 4
//! 全程持鎖，其餘盞的引擎驅動段同鎖（比照 `r1_redo_multistep.rs` 紀律）。

use quire_engine::{DocPos, Engine, InsertSide, Mutation, NoteKind2, SubDocTarget, TablePath};

/// env 翻動 serialize 鎖：燈 3／燈 4 的 set／remove 全程持鎖，其餘盞的引擎驅動
/// 段同鎖——平行執行緒下「讀 env 的斷言」與「翻 env 的測試」互斥，零汙染
/// （比照 `r1_redo_multistep.rs` 的 `R1_ENV` 紀律）。
static R1F_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 取鎖（中毒不致命：持鎖者 panic 只可能留髒 env，本檔尾端照樣 `remove_var`
/// 收尾，這裡把中毒誤差還原成守衛即可繼續）。
fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    R1F_ENV.lock().unwrap_or_else(|e| e.into_inner())
}

/// 行為鎖（燈 1／燈 2／燈 5）專用：只摘 `R1_REDO_BUGGY`，**刻意保留**外部設定的
/// `R1A_SHIELD_BUGGY`／`R1A_UNTHREADED_BUGGY`。理由與 `r1_redo_multistep.rs` 同口徑
/// ——外部以 `R1A_SHIELD_BUGGY=1 R1A_UNTHREADED_BUGGY=1` 整跑時本檔行為鎖必須翻紅
/// （撤除兜底＋分流臂未貫穿＝修前世界），PM 的外部雙態閘門才有鑑別力；在這裡清掉
/// 變數會讓外部回塞態假綠。`R1_REDO_BUGGY` 屬 R1-a 主幹閘門，與本卡判別軸正交，
/// 摘除以免兩張卡的回塞鎖互相誤染。
fn clear_foreign_env_lock() {
    std::env::remove_var("R1_REDO_BUGGY");
}

/// set／restore 成對的 env 守衛：進場記住原值（可能是外部閘門設的），`Drop` 時
/// **逐字還原**而非一律 `remove_var`。比 R1-a 當年的「尾端 remove_var」更嚴——
/// 外部整跑時平行執行的行為鎖不會被本檔的收尾誤清成假綠（R1-a 的時序僥倖在
/// 本卡不複製）。
struct EnvFlag {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl EnvFlag {
    /// 置位（`val` 通常 `"1"`）；原值為 `None` 時 Drop 會 `remove_var`。
    fn set(key: &'static str, val: &str) -> Self {
        let prev = std::env::var_os(key);
        std::env::set_var(key, val);
        Self { key, prev }
    }

    /// 摘位（本燈要求該鎖不存在，例如燈 3 需要「護欄失效但旗標已貫穿」的純態）。
    fn unset(key: &'static str) -> Self {
        let prev = std::env::var_os(key);
        std::env::remove_var(key);
        Self { key, prev }
    }
}

impl Drop for EnvFlag {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

fn load_bytes(rel: &str) -> Vec<u8> {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 {full} 失敗: {e}"))
}

fn open(rel: &str) -> Engine {
    Engine::from_docx_bytes(&load_bytes(rel)).unwrap_or_else(|e| panic!("開啟 {rel} 失敗: {e}"))
}

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

fn document_xml(e: &Engine) -> String {
    String::from_utf8_lossy(e.part("word/document.xml").expect("必須有 document.xml")).into_owned()
}

/// 根表（body 第 0 張）的列數／欄數（K29c 考卷同款觀察口）。
fn root_table(engine: &Engine) -> serde_json::Value {
    let v: serde_json::Value =
        serde_json::from_str(&engine.tables_json()).expect("tables_json 不是合法 JSON");
    v.get(0).cloned().expect("必須有第一張表")
}

fn rows_len(table: &serde_json::Value) -> usize {
    table["table"]["rows"].as_array().unwrap().len()
}

fn grid_len(table: &serde_json::Value) -> usize {
    table["table"]["grid"].as_array().unwrap().len()
}

fn footnotes_xml(e: &Engine) -> String {
    String::from_utf8_lossy(
        e.part("word/footnotes.xml")
            .expect("fixture 必須自帶 word/footnotes.xml"),
    )
    .into_owned()
}

/// 於第 `para` 段**段尾**追加文字（cp 由當下段長動態取得，不寫死——兩份 fixture
/// 的段長差一與零都有，寫死必踩 `CodepointOutOfBounds`）。
fn append_text(e: &mut Engine, para: usize, text: &str) {
    let cp = e
        .paragraph_text(para)
        .map(|t| t.chars().count())
        .unwrap_or(0);
    e.apply_mutation(Mutation::InsertText {
        target: SubDocTarget::Body,
        pos: DocPos { para, cp },
        text: text.to_string(),
        style_override: None,
    })
    .unwrap_or_else(|err| panic!("主幹 InsertText para={para} cp={cp} 失敗：{err:?}"));
    settle(e);
}

fn insert_row(e: &mut Engine) {
    e.apply_mutation(Mutation::InsertTableRow {
        table: TablePath::body(0),
        at_row: 1,
        position: InsertSide::After,
    })
    .expect("K29c InsertTableRow 失敗");
    settle(e);
}

fn delete_column(e: &mut Engine) {
    e.apply_mutation(Mutation::DeleteTableColumn {
        table: TablePath::body(0),
        grid_col: 2,
    })
    .expect("K29c DeleteTableColumn 失敗");
    settle(e);
}

/// `word/footnotes.xml` 中 `w:footnote` 標籤數（觀察插註腳是否如實回來／消失）。
fn footnote_tag_count(e: &Engine) -> usize {
    footnotes_xml(e).matches("<w:footnote ").count()
}

// ---------------------------------------------------------------------------
// 燈 1／燈 3 共用序列：K29c 首筆＋主幹中段＋K29c 末筆
// ---------------------------------------------------------------------------

/// 鋪三筆編輯：K29c `InsertTableRow` → 主幹 `InsertText`（第 0 段，表格前的
/// 穩定段落）→ K29c `DeleteTableColumn`。三筆必須分屬兩族，且**首筆走 K29c**
/// ——重放順序由舊到新，首筆重放時棧上還有 2 筆待重放，分流臂誤清立刻可見。
fn three_edits_k29c_first(e: &mut Engine) {
    insert_row(e);
    append_text(e, 0, "（甲）");
    delete_column(e);
}

/// 斷言三筆效果全部不在場（undo ×3 走完的基準態）。
fn assert_edits_absent(e: &Engine, xml_before: &str) {
    assert_eq!(
        document_xml(e),
        xml_before,
        "undo ×3 後 document.xml 必須逐位元組回原文"
    );
    assert!(
        !e.paragraph_text(0)
            .as_deref()
            .is_some_and(|t| t.contains('甲')),
        "undo ×3 後主幹編輯（甲）必須消失"
    );
}

/// 燈 1 主體：`undo()`×3 後 `redo()`×3 逐筆斷言。`require_stack_alive` 是
/// 本卡的核心斷言——重放 K29c 那筆之後，剩餘 redo 棧必須仍在（護欄撤除後
/// 只能靠貫穿的 `replay` 旗標守住）。
fn drive_multistep_redo_k29c(e: &mut Engine, xml_before: &str, ctx: &str) {
    for i in 1..=3 {
        let out = e
            .undo()
            .unwrap_or_else(|err| panic!("{ctx}：undo #{i} 不該失敗：{err:?}"));
        assert!(out.is_some(), "{ctx}：第 {i} 次 undo 必須回 Some");
        settle(e);
    }
    assert_edits_absent(e, xml_before);
    assert!(e.can_redo(), "{ctx}：undo ×3 後必須可 redo");
    assert!(!e.can_undo(), "{ctx}：undo ×3 必須清空 undo 棧");

    // ---- redo #1：重放 K29c InsertTableRow（棧上尚有 2 筆）----
    e.redo()
        .unwrap_or_else(|err| panic!("{ctx}：redo #1（K29c 重放）不該失敗：{err:?}"))
        .unwrap_or_else(|| panic!("{ctx}：redo #1 必須回 Some（K29c 首筆重放）"));
    settle(e);
    assert_eq!(
        rows_len(&root_table(e)),
        5,
        "{ctx}：redo #1 後 K29c 插入列必須回來（根表列數 4→5）"
    );
    assert!(
        e.can_redo(),
        "{ctx}：重放 K29c 首筆後**剩餘 redo 棧不得被誤清**——還有 2 筆待重放\
         （分流臂無條件 clear 時本斷言翻紅＝本卡缺陷本體）"
    );
    assert!(
        !e.paragraph_text(0)
            .as_deref()
            .is_some_and(|t| t.contains('甲')),
        "{ctx}：redo #1 只重放首筆，主幹編輯（甲）不得提前回來（重放順序鎖）"
    );

    // ---- redo #2：重放主幹 InsertText（棧上尚有 1 筆）----
    e.redo()
        .unwrap_or_else(|err| panic!("{ctx}：redo #2 不該失敗：{err:?}"))
        .unwrap_or_else(|| {
            panic!(
                "{ctx}：redo #2 必須回 Some——修前形態正是這裡 Ok(None)\
             （K29c 首筆重放把整個 redo 棧清空）"
            )
        });
    settle(e);
    assert!(
        e.paragraph_text(0)
            .as_deref()
            .is_some_and(|t| t.ends_with("（甲）")),
        "{ctx}：redo #2 後主幹編輯（甲）必須回來"
    );
    assert_eq!(
        grid_len(&root_table(e)),
        3,
        "{ctx}：redo #2 階段 K29c 末筆（刪欄）尚未重放，網格仍 3 欄"
    );
    assert!(e.can_redo(), "{ctx}：redo #2 後仍有 1 筆待重放，棧不得空");

    // ---- redo #3：重放 K29c DeleteTableColumn（末筆）----
    e.redo()
        .unwrap_or_else(|err| panic!("{ctx}：redo #3（K29c 重放）不該失敗：{err:?}"))
        .unwrap_or_else(|| panic!("{ctx}：redo #3 必須回 Some（K29c 末筆重放）"));
    settle(e);
    assert_eq!(
        grid_len(&root_table(e)),
        2,
        "{ctx}：redo #3 後 K29c 刪欄必須生效（網格 3→2 欄）"
    );
    assert!(!e.can_redo(), "{ctx}：三筆全部重放後 can_redo() 必須 false");
    assert!(
        e.can_undo(),
        "{ctx}：重放照舊 push undo 棧——重做完 Ctrl+Z 仍能再撤銷"
    );
}

// ---------------------------------------------------------------------------
// 燈 1：跨家族重放鎖（K29c 首筆／末筆，正常態）
// ---------------------------------------------------------------------------

/// 燈 1：`k29c → 主幹 → k29c` 三筆→`undo()`×3→`redo()`×3 逐筆成功，重放
/// K29c 那筆時剩餘 redo 棧不被誤清。走完 `roundtrip_check()` pass 且白名單僅
/// `word/document.xml`——貫穿旗標不得碰壞「未觸碰 part 零失真」。
#[test]
fn r1af_lamp1_k29c_family_redo_keeps_remaining_redo_stack() {
    let _g = env_guard();
    clear_foreign_env_lock();
    let mut e = open("fixtures/table_mixed.docx");
    settle(&mut e);
    let xml_before = document_xml(&e);
    assert_eq!(
        rows_len(&root_table(&e)),
        4,
        "table_mixed.docx 根表列數基準 4"
    );
    assert_eq!(
        grid_len(&root_table(&e)),
        3,
        "table_mixed.docx 根表網格欄數基準 3"
    );

    three_edits_k29c_first(&mut e);
    assert_eq!(rows_len(&root_table(&e)), 5, "前置：插入列後列數 4→5");
    assert_eq!(grid_len(&root_table(&e)), 2, "前置：刪欄後網格 3→2");
    assert!(
        e.paragraph_text(0)
            .as_deref()
            .is_some_and(|t| t.ends_with("（甲）")),
        "前置：主幹編輯（甲）已入第 0 段段尾"
    );

    drive_multistep_redo_k29c(&mut e, &xml_before, "燈1");

    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(
        diff.pass,
        "redo ×3 後未觸碰 part 零失真必須 pass：{:?}",
        diff.violations
    );
    assert_eq!(
        diff.whitelisted_changed,
        vec!["word/document.xml".to_string()],
        "白名單真變更只准 word/document.xml"
    );
    assert!(
        diff.missing.is_empty() && diff.added.is_empty(),
        "不得增缺 part：missing={:?} added={:?}",
        diff.missing,
        diff.added
    );
}

// ---------------------------------------------------------------------------
// 燈 2：跨家族重放鎖（K29d 中段）
// ---------------------------------------------------------------------------

/// 燈 2：`主幹 → K29d InsertNote → 主幹` 三筆，K29d 那筆放中段——重放它時棧上
/// 還有 1 筆待重放，分流臂無條件 `clear()` 立刻翻紅（第 3 次 redo 即 `Ok(None)`）。
#[test]
fn r1af_lamp2_k29d_family_redo_keeps_remaining_redo_stack() {
    let _g = env_guard();
    clear_foreign_env_lock();
    let mut e = open("corpus/poi/test-data_document_form_footnotes.docx");
    settle(&mut e);
    let xml_before = document_xml(&e);
    let notes_before = footnote_tag_count(&e);
    let host = (0..e.paragraph_count())
        .find(|&i| {
            e.paragraph_text(i)
                .as_deref()
                .is_some_and(|t| t.contains("Testdoc"))
        })
        .unwrap_or(0);

    // ---- 三筆：主幹 → K29d（中段）→ 主幹 ----
    append_text(&mut e, 0, "（丁）");
    e.apply_mutation(Mutation::InsertNote {
        kind: NoteKind2::Footnote,
        pos: DocPos { para: host, cp: 0 },
        initial_text: String::new(),
    })
    .expect("K29d InsertNote 失敗");
    settle(&mut e);
    let notes_after_insert = footnote_tag_count(&e);
    assert_eq!(
        notes_after_insert,
        notes_before + 1,
        "前置：InsertNote 必須多一支 w:footnote"
    );
    append_text(&mut e, 1, "（戊）");

    // ---- undo ×3 ----
    for i in 1..=3 {
        e.undo()
            .unwrap_or_else(|err| panic!("燈2：undo #{i} 不該失敗：{err:?}"))
            .unwrap_or_else(|| panic!("燈2：第 {i} 次 undo 必須回 Some"));
        settle(&mut e);
    }
    assert_eq!(
        footnote_tag_count(&e),
        notes_before,
        "燈2：undo ×3 後新註腳必須誠實消失"
    );
    assert_eq!(
        document_xml(&e),
        xml_before,
        "燈2：undo ×3 後 document.xml 必須回原文"
    );
    assert!(e.can_redo(), "燈2：undo ×3 後必須可 redo");

    // ---- redo ×3：中段那筆是 K29d ----
    e.redo()
        .expect("燈2：redo #1 不該失敗")
        .expect("燈2：redo #1 必須回 Some");
    settle(&mut e);
    assert!(
        e.paragraph_text(0)
            .as_deref()
            .is_some_and(|t| t.ends_with("（丁）")),
        "燈2：redo #1 後（丁）必須回來"
    );
    assert_eq!(
        footnote_tag_count(&e),
        notes_before,
        "燈2：redo #1 階段 K29d 那筆尚未重放，註腳數不得變（順序鎖）"
    );
    assert!(e.can_redo(), "燈2：redo #1 後棧上還有 2 筆");

    e.redo()
        .expect("燈2：redo #2（K29d 重放）不該失敗")
        .expect("燈2：redo #2 必須回 Some（K29d InsertNote 重放）");
    settle(&mut e);
    assert_eq!(
        footnote_tag_count(&e),
        notes_before + 1,
        "燈2：redo #2 後 K29d 新註腳必須回來"
    );
    assert!(
        e.can_redo(),
        "燈2：重放 K29d 那筆後**剩餘 redo 棧不得被誤清**——還有 1 筆待重放\
         （分流臂無條件 clear 時本斷言翻紅＝本卡缺陷本體）"
    );

    e.redo()
        .expect("燈2：redo #3 不該失敗")
        .expect("燈2：redo #3 必須回 Some（末筆重放）");
    settle(&mut e);
    assert!(
        e.paragraph_text(1)
            .as_deref()
            .is_some_and(|t| t.ends_with("（戊）")),
        "燈2：redo #3 後（戊）必須回來"
    );
    assert!(!e.can_redo(), "燈2：三筆全部重放後 can_redo() 必須 false");
    assert!(e.can_undo(), "燈2：重放照舊 push undo 棧");

    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(
        diff.pass,
        "燈2：redo ×3 後未觸碰 part 零失真必須 pass：{:?}",
        diff.violations
    );
}

// ---------------------------------------------------------------------------
// 燈 3：機制面鎖——撤除護欄兜底後，行為仍須正確（靠貫穿的旗標過）
// ---------------------------------------------------------------------------

/// 燈 3（卡面完成條件「考卷要能在護欄撤除後仍然為真」）：單一測試內令 `redo()`
/// 的重放護欄失效（`R1A_SHIELD_BUGGY=1`），主幹重放臂照樣不誤清——此時 K29c 家族
/// 重放能否逐筆成功，完全取決於 `replay` 有無貫穿 `commit_k29c`。護欄的兜底角色
/// 撤掉後本燈仍綠＝語意已貫穿；有人把旗標抽掉（或改回無條件 clear）本燈立刻翻紅。
///
/// env 紀律：用 `EnvFlag` set／restore 成對（比 R1-a 的「尾端 remove_var」更嚴，
/// 外部閘門整跑時不會把平行盞需要的變數誤清成假綠）。本燈只測「護欄失效＋旗標
/// 已貫穿」這一條軸，故進場暫時摘除 `R1A_UNTHREADED_BUGGY`、出場逐字還原。
#[test]
fn r1af_lamp3_shield_off_still_redoes_every_step() {
    let _g = env_guard();
    std::env::remove_var("R1_REDO_BUGGY");
    // 本燈只測「護欄失效＋旗標已貫穿」這一條軸：暫時摘除鏡像鎖，出場逐字還原。
    let _mirror_off = EnvFlag::unset("R1A_UNTHREADED_BUGGY");

    // ---- 先實錄正常態多步 redo 真通（回塞斷言的對照組，防本燈假綠）----
    {
        let mut e = open("fixtures/table_mixed.docx");
        settle(&mut e);
        let xml_before = document_xml(&e);
        three_edits_k29c_first(&mut e);
        drive_multistep_redo_k29c(&mut e, &xml_before, "燈3正常態");
    }

    // ---- 護欄失效態：同型序列必須仍逐筆成功 ----
    // EnvFlag 逐字 restore：外部閘門若已設 R1A_UNTHREADED_BUGGY，本燈不收它
    // （那是燈 4 的世界），但也不會把它誤清成平行盞的假綠。
    let _shield = EnvFlag::set("R1A_SHIELD_BUGGY", "1");
    let result = {
        let mut e = open("fixtures/table_mixed.docx");
        settle(&mut e);
        let xml_before = document_xml(&e);
        three_edits_k29c_first(&mut e);
        let mut ok = true;
        for _ in 0..3 {
            e.undo()
                .expect("燈3護欄失效態：undo 不該失敗")
                .expect("燈3護欄失效態：undo 回 Some");
            settle(&mut e);
        }
        assert_edits_absent(&e, &xml_before);
        for i in 1..=3 {
            match e.redo() {
                Ok(Some(_)) => {
                    settle(&mut e);
                    if !e.can_redo() && i < 3 {
                        ok = false;
                        break;
                    }
                }
                Ok(None) => {
                    ok = false;
                    break;
                }
                Err(err) => panic!("燈3護欄失效態：redo #{i} 不該失敗：{err:?}"),
            }
        }
        let alive_after = e.can_redo();
        let undone = e.can_undo();
        (ok, alive_after, undone)
    };
    assert!(
        result.0,
        "護欄失效態（R1A_SHIELD_BUGGY=1）三筆重放必須逐筆成功且剩餘棧存活\
         ——replay 旗標未貫穿 K29c 分流臂時本斷言紅（＝本卡要收拾的語意未貫穿）"
    );
    assert!(
        !result.1,
        "護欄失效態：三筆重放完 can_redo() 必須 false（末筆重放後棧空，不得虛留）"
    );
    assert!(
        result.2,
        "護欄失效態：重放照舊 push undo 棧（can_undo() 必須 true）"
    );
}

// ---------------------------------------------------------------------------
// 燈 4：回塞鑑別鎖——撤除兜底＋未貫穿鏡像必紅（證明考卷有鑑別力）
// ---------------------------------------------------------------------------

/// 燈 4（鑑別力證明）：三態對照，證明回塞鎖真接到缺陷路徑、考卷非假綠——
///
/// 1. **鏡像單翻**（只 `R1A_UNTHREADED_BUGGY=1`）：護欄仍在場 ⇒ 行為面被兜住 ⇒
///    多步 redo 必須**仍全綠**。這盞把「紅燈來自護欄失效、不是鏡像本身」釘死，
///    也忠實記錄卡面「現況靠護欄過」的事實。
/// 2. **護欄失效＋鏡像在場**（兩鎖併翻）：撤除兜底＋分流臂未貫穿＝逐字復刻
///    R1-a 續**修前**世界 ⇒ 第 2 次 redo 必 `Ok(None)`、`can_redo()` 翻 false
///    （缺陷形態實錄）。
/// 3. **護欄失效＋鏡像解除**（只 `R1A_SHIELD_BUGGY=1`）：修後世界 ⇒ 多步 redo
///    必須**逐筆成功**（與燈 3 同軸的對照組）。
///
/// 三態合起來才構成完整鑑別：鏡像翻不翻決定紅綠、護欄在不在場決定鏡像能否傷害
/// 行為。env 用 `EnvFlag` set／restore 成對——外部閘門整跑時不會把平行盞需要的
/// 變數誤清成假綠。
#[test]
fn r1af_lamp4_unthreaded_mirror_reproduces_the_defect() {
    let _g = env_guard();
    std::env::remove_var("R1_REDO_BUGGY");

    // ---- 態 1：鏡像單翻（護欄在場）→ 行為仍全綠（卡面「現況靠護欄過」實錄）----
    {
        // 護欄在場是本態前提：外部閘門若已設 R1A_SHIELD_BUGGY，本態要的是「有
        // 護欄」的世界，故顯式摘除（EnvFlag 出場逐字還原，不污染外部整跑）。
        let _shield_on = EnvFlag::unset("R1A_SHIELD_BUGGY");
        let _mirror = EnvFlag::set("R1A_UNTHREADED_BUGGY", "1");
        let mut e = open("fixtures/table_mixed.docx");
        settle(&mut e);
        let xml_before = document_xml(&e);
        three_edits_k29c_first(&mut e);
        drive_multistep_redo_k29c(&mut e, &xml_before, "燈4態1（鏡像單翻·護欄在場）");
    }

    // ---- 態 2：護欄失效＋鏡像在場 → 修前缺陷形態必復辟 ----
    let buggy = {
        let _shield = EnvFlag::set("R1A_SHIELD_BUGGY", "1");
        let _mirror = EnvFlag::set("R1A_UNTHREADED_BUGGY", "1");
        let mut e = open("fixtures/table_mixed.docx");
        settle(&mut e);
        three_edits_k29c_first(&mut e);
        for i in 1..=3 {
            e.undo()
                .unwrap_or_else(|err| panic!("燈4態2：undo #{i} 不該失敗：{err:?}"))
                .unwrap_or_else(|| panic!("燈4態2：第 {i} 次 undo 必須回 Some"));
            settle(&mut e);
        }
        assert!(e.can_redo(), "燈4態2 前置：undo 後 3 筆在 redo 棧上");
        let r1 = e.redo().expect("燈4態2：redo #1 不該失敗").is_some();
        settle(&mut e);
        let r2 = e.redo().expect("燈4態2：redo #2 不該失敗").is_some();
        let alive = e.can_redo();
        (r1, r2, alive)
    };
    assert!(
        buggy.0,
        "燈4態2：第 1 次 redo 仍成功（pop 出來的那筆重放得動）"
    );
    assert!(
        !buggy.1,
        "燈4態2（護欄撤除＋分流臂未貫穿）第 2 次 redo 必須回到缺陷形態 Ok(None)\
         ——否則回塞鎖沒接上真缺陷路徑，本卡考卷即為假綠"
    );
    assert!(
        !buggy.2,
        "燈4態2：K29c 首筆重放後 can_redo() 必翻 false（剩餘 redo 棧被誤清＝缺陷本體實錄）"
    );

    // ---- 態 3：護欄失效＋鏡像解除 → 修後世界逐筆成功（紅綠分岔由鏡像驅動）----
    {
        let _shield = EnvFlag::set("R1A_SHIELD_BUGGY", "1");
        let _mirror_off = EnvFlag::unset("R1A_UNTHREADED_BUGGY");
        let mut e = open("fixtures/table_mixed.docx");
        settle(&mut e);
        let xml_before = document_xml(&e);
        three_edits_k29c_first(&mut e);
        drive_multistep_redo_k29c(&mut e, &xml_before, "燈4態3（護欄失效·旗標已貫穿）");
    }
}

// ---------------------------------------------------------------------------
// 燈 5：新操作照清·反向鎖（iron rule 1 在 K29c 家族同樣成立）
// ---------------------------------------------------------------------------

/// 燈 5（防修復過度）：K29c 家族 `undo→redo→再一筆 K29c 新操作`⇒`can_redo()==false`
/// ——貫穿旗標只准治「重放誤清」，不得順手把「新操作斷鏈」搬走（分欄臂的
/// `replay=false` 路徑必須照清）。順帶鎖住「重放後 can_redo() 仍 true」的中繼
/// 事實，證明斷紅的來源是新操作而非重放。
#[test]
fn r1af_lamp5_new_k29c_edit_after_redo_breaks_redo_chain() {
    let _g = env_guard();
    clear_foreign_env_lock();
    let mut e = open("fixtures/table_mixed.docx");
    settle(&mut e);
    assert_eq!(grid_len(&root_table(&e)), 3, "前置：根表網格欄數基準 3");

    // 三筆：K29c → 主幹 → K29c，全部 undo 後 redo 棧上三筆待重放。
    three_edits_k29c_first(&mut e);
    assert_eq!(grid_len(&root_table(&e)), 2, "前置：刪欄生效（網格 3→2）");
    for i in 1..=3 {
        e.undo()
            .unwrap_or_else(|err| panic!("燈5：undo #{i} 不該失敗：{err:?}"))
            .unwrap_or_else(|| panic!("燈5：第 {i} 次 undo 必須回 Some"));
        settle(&mut e);
    }
    assert_eq!(grid_len(&root_table(&e)), 3, "燈5：undo ×3 後網格回 3 欄");

    // redo #1＝重放**K29c** 首筆：剩餘 2 筆必須存活（貫穿旗標／護欄共同負責）。
    e.redo()
        .expect("燈5：redo 不該失敗")
        .expect("燈5：redo 回 Some");
    settle(&mut e);
    assert_eq!(
        rows_len(&root_table(&e)),
        5,
        "燈5：redo 後 K29c 插入列效果回來"
    );
    assert!(
        e.can_redo(),
        "燈5：redo 後仍有 2 筆待重放——重放 K29c 那筆不得誤清剩餘棧"
    );

    // K29c 家族的新操作入棧 ⇒ 標準語義：剩餘重做歷史整條作廢。
    e.apply_mutation(Mutation::InsertTableRow {
        table: TablePath::body(0),
        at_row: 0,
        position: InsertSide::Before,
    })
    .expect("K29c InsertTableRow（新操作）失敗");
    settle(&mut e);
    assert!(
        !e.can_redo(),
        "K29c 新操作必須丟棄剩餘 redo 歷史（iron rule 1；把 clear() 從新操作臂\
         搬走過度時本燈紅）"
    );
    assert!(e.can_undo(), "K29c 新操作照舊進 undo 棧");
}
