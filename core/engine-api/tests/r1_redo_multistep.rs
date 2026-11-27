//! R1-a：多步 Redo 全失——`apply_mutation` 結尾 `redo_stack.clear()` 自我擦除
//! （先紅後綠考卷，t_2ecfe8ca）。
//!
//! 缺陷（PM 坐實於 main tip `bf4dfa1`）：`redo()` 逐筆重放 `entry.inverse_ops`
//! 時走 `apply_mutation`，其結尾**無條件** `redo_stack.clear()`——重放第一筆
//! 就把整個 redo 棧清空，棧頂之後尚未重放的 entries 全滅 ⇒ 多步 redo 結構性
//! 不可能成功（用戶視角：Ctrl+Z 兩次→Ctrl+Y 只回來最後一步）。
//!
//! 修法軸（PM 定案）：區分「重放路徑」與「新操作路徑」——`apply_mutation`
//! 拆出私有 `apply_mutation_inner(op, replay)`，新操作（replay=false）照舊清
//! redo 棧（標準編輯器語義，燈 2 鎖死不得過度搬走），重放（replay=true）保留
//! 剩餘棧；`redo()` 另設呼叫端護欄兜住 K29c／K29d 分流臂的無條件清棧，並守住
//! 失敗原子性（iron rule 3）。
//!
//! 盞別（本檔 3 盞）：
//! - 燈 1（多步重做鎖）：3 筆編輯→`undo()`×3（回原文）→`redo()`×3 逐筆斷言
//!   效果回來＋第三次後 `can_redo()==false`；走完 `roundtrip_check()` 鎖
//!   「未觸碰 part 零失真」不被本修復破壞。**修前必紅**：第二次 redo 即
//!   `Ok(None)`（棧已被第一筆重放清空）。
//! - 燈 2（redo 後新操作斷鏈鎖）：2 筆編輯→`undo()`×2→`redo()`×1→新增編輯
//!   →`can_redo()==false`（標準語義；防把 `clear()` 從新操作路徑搬走過度）。
//!   此盞修前修後皆綠——它鎖的是「修復不得過度」的反向邊界。
//! - 燈 3（回塞鑑別 buggy-replay）：`R1_REDO_BUGGY=1`（比照倉內
//!   `E2B_UNDERLINE_BUGGY` 口徑：入口 `remove_var` 防外洩、set/remove 成對、
//!   只在單一測試內驅動）時逐字復刻「重放時清棧」缺陷，考卷燈 1 必紅、本燈
//!   實錄缺陷形態（第二次 redo 回 `Ok(None)`、`can_redo()` 翻 false）。
//!   正常態本燈同時驗證多步 redo 真通（防鑑別斷言假綠）。
//!
//! 並行安全：三盞共用以 `R1_ENV` 靜態互斥鎖 serialize——燈 3 翻 env 期間燈 1／
//! 燈 2 不會被同進程平行執行緒誤染（比照 f1／k25b「env 不外洩」紀律的顯式化）。

use quire_engine::{BreakKind, DocPos, Engine, Mutation, SubDocTarget};

/// env 翻動 serialize 鎖：燈 3 的 set/remove 全程持鎖，燈 1／燈 2 的引擎驅動
/// 段同鎖——平行執行緒下「讀 env 的斷言」與「翻 env 的測試」互斥，零汙染。
static R1_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 取鎖（中毒不致命：持鎖者 panic 只可能留髒 env，本檔燈 3 尾端照樣 remove_var
/// 收尾，這裡把中毒誤差還原成守衛即可繼續）。
fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    R1_ENV.lock().unwrap_or_else(|e| e.into_inner())
}

const FX: &[u8] = include_bytes!("../../../fixtures/style_cascade.docx");

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

fn engine() -> Engine {
    let mut e = Engine::from_docx_bytes(FX).expect("開啟 fixtures/style_cascade.docx 失敗");
    settle(&mut e);
    e
}

fn document_xml(e: &Engine) -> String {
    String::from_utf8_lossy(e.part("word/document.xml").expect("必須有 document.xml")).into_owned()
}

/// 硬分頁標籤計數（fixture 自帶 1 支在末段 `w:br w:type="page"`，M1.5 再插入
/// 第 2 支；undo 後必須回到恰 1 支＝插入的那支誠實消失）。
fn page_break_count(e: &Engine) -> usize {
    document_xml(e).matches(r#"<w:br w:type="page"/>"#).count()
}

fn insert_text(e: &mut Engine, para: usize, cp: usize, text: &str) {
    e.apply_mutation(Mutation::InsertText {
        target: SubDocTarget::Body,
        pos: DocPos { para, cp },
        text: text.to_string(),
        style_override: None,
    })
    .unwrap_or_else(|err| panic!("InsertText para={para} cp={cp} 套用失敗：{err:?}"));
    settle(e);
}

fn insert_page_break(e: &mut Engine, para: usize, cp: usize) {
    e.apply_mutation(Mutation::InsertBreak {
        target: SubDocTarget::Body,
        pos: DocPos { para, cp },
        kind: BreakKind::Page,
    })
    .unwrap_or_else(|err| panic!("InsertBreak(Page) para={para} cp={cp} 套用失敗：{err:?}"));
    settle(e);
}

/// 依卡面規格鋪 3 筆編輯：M1.1 InsertText ×2（第 0、1 段不同段）＋
/// M1.5 InsertBreak(Page)（第 2 段段尾）。
fn three_edits(e: &mut Engine) {
    insert_text(e, 0, 8, "（甲）");
    insert_text(e, 1, 8, "（乙）");
    insert_page_break(e, 2, 8);
}

// ─────────────────────── 燈 1：多步重做鎖（修前必紅） ───────────────────────

/// 燈 1（缺陷本體鎖）：undo ×3 回原文後，redo 必須**逐筆**重放 3 筆編輯，
/// 而不是只回來最後一步。修前形態：第 1 次 redo 重放時 `apply_mutation`
/// 結尾把 redo 棧整個清空 ⇒ 第 2 次 redo 即 `Ok(None)`（本燈必紅）。
/// 走完還須 `roundtrip_check()` pass 且白名單變更只有 `word/document.xml`
/// ——本修復不得碰壞「未觸碰 part 零失真」。
#[test]
fn r1a_lamp1_multistep_redo_restores_every_edit() {
    let _g = env_guard();
    let mut e = engine();
    let xml_before = document_xml(&e);
    let base_pb = page_break_count(&e);
    assert_eq!(base_pb, 1, "fixture 自帶恰 1 支硬分頁（末段）");
    assert_eq!(e.paragraph_text(0).as_deref(), Some("段落級自閉合粗體"));
    assert_eq!(e.paragraph_text(1).as_deref(), Some("直接格式粗體綠字"));

    three_edits(&mut e);
    assert!(
        e.paragraph_text(0)
            .as_deref()
            .is_some_and(|t| t.ends_with("（甲）")),
        "前置：編輯 A 後第 0 段尾必須帶（甲）"
    );
    assert!(
        page_break_count(&e) == base_pb + 1,
        "前置：M1.5 後多 1 支硬分頁"
    );

    // ---- undo ×3：逐筆回到原文 ----
    for i in 1..=3 {
        let out = e.undo().expect("undo 不該失敗");
        assert!(out.is_some(), "第 {i} 次 undo 必須回 Some（棧上還有筆）");
        settle(&mut e);
    }
    assert_eq!(
        e.paragraph_text(0).as_deref(),
        Some("段落級自閉合粗體"),
        "undo ×3 後第 0 段必須回到原文"
    );
    assert_eq!(
        e.paragraph_text(1).as_deref(),
        Some("直接格式粗體綠字"),
        "undo ×3 後第 1 段必須回到原文"
    );
    assert_eq!(
        page_break_count(&e),
        base_pb,
        "undo ×3 後插入的硬分頁必須消失"
    );
    assert_eq!(
        document_xml(&e),
        xml_before,
        "undo ×3 後 document.xml 必須逐位元組回原文"
    );
    assert!(e.can_redo(), "undo ×3 後必須可 redo");
    assert!(!e.can_undo(), "undo ×3 必須把 3 筆全部退出 undo 棧");

    // ---- redo ×3：逐筆斷言該步效果回來（修前第 2 步即 Ok(None) 紅）----
    let out1 = e
        .redo()
        .expect("redo #1 不該失敗")
        .expect("redo #1 必須回 Some（第 1 筆重放）");
    assert_eq!(
        out1.touched_parts,
        vec!["word/document.xml".to_string()],
        "redo #1 outcome 的觸碰面白名單鎖（零意外擴散）"
    );
    settle(&mut e);
    assert!(
        e.paragraph_text(0)
            .as_deref()
            .is_some_and(|t| t.ends_with("（甲）")),
        "redo #1 後編輯 A（第 0 段（甲））必須回來"
    );
    assert!(
        !e.paragraph_text(1)
            .as_deref()
            .is_some_and(|t| t.contains('乙')),
        "redo #1 只重放第 1 筆，編輯 B 不得提前回來（重放順序鎖）"
    );

    e.redo()
        .expect("redo #2 不該失敗")
        .expect("redo #2 必須回 Some——修前這裡就是 Ok(None)：第一筆重放把整個 redo 棧清空");
    settle(&mut e);
    assert!(
        e.paragraph_text(1)
            .as_deref()
            .is_some_and(|t| t.ends_with("（乙）")),
        "redo #2 後編輯 B（第 1 段（乙））必須回來"
    );
    assert_eq!(
        page_break_count(&e),
        base_pb,
        "redo #2 階段第 3 筆（硬分頁）尚未重放，不得提前出現"
    );

    let out3 = e
        .redo()
        .expect("redo #3 不該失敗")
        .expect("redo #3 必須回 Some——多步重做結構性可行（修前棧已全滅）");
    assert_eq!(
        out3.touched_parts,
        vec!["word/document.xml".to_string()],
        "redo #3 outcome 的觸碰面白名單鎖"
    );
    settle(&mut e);
    assert_eq!(
        page_break_count(&e),
        base_pb + 1,
        "redo #3 後編輯 C 的硬分頁必須回來"
    );
    assert!(!e.can_redo(), "3 筆全部重放後 can_redo() 必須 false");
    assert!(
        e.can_undo(),
        "重放照舊 push undo 棧——重做完 Ctrl+Z 仍能再撤銷"
    );
    assert!(
        e.paragraph_text(0)
            .as_deref()
            .is_some_and(|t| t.ends_with("（甲）"))
            && e.paragraph_text(1)
                .as_deref()
                .is_some_and(|t| t.ends_with("（乙）")),
        "redo ×3 走完後三筆編輯效果必須全部在場（甲乙分頁齊備）"
    );

    // ---- 零失真護欄：本修復不得讓未觸碰 part 失真 ----
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

// ─────────────────── 燈 2：redo 後新操作斷鏈鎖（防修復過度） ───────────────────

/// 燈 2（iron rule 1 反向鎖）：新語義操作（非重放）必須照標準編輯器語義丟棄
/// redo 歷史——2 筆編輯→undo ×2→redo ×1→**新增一筆編輯**⇒`can_redo()==false`
/// （重做鏈斷）。把 `redo_stack.clear()` 從 `apply_mutation` 整個搬到 `undo()`
/// 的「過度修復」會讓本燈翻紅（新操作不再斷鏈、重做鏈復活）。
#[test]
fn r1a_lamp2_new_edit_after_redo_breaks_redo_chain() {
    let _g = env_guard();
    let mut e = engine();

    insert_text(&mut e, 0, 8, "（甲）");
    insert_text(&mut e, 1, 8, "（乙）");

    e.undo().expect("undo #1").expect("undo #1 回 Some");
    settle(&mut e);
    e.undo().expect("undo #2").expect("undo #2 回 Some");
    settle(&mut e);
    assert!(e.can_redo(), "undo ×2 後必須可 redo");

    e.redo().expect("redo 不該失敗").expect("redo 必須回 Some");
    settle(&mut e);

    // 新操作入棧 ⇒ 標準語義：剩餘重做歷史整條作廢。（刻意不在 redo 後中繼
    // 斷言 can_redo()——回塞態重放照清棧，中繼斷言會被 env 誤染；本燈的判別
    // 點在新操作後的 !can_redo()，新操作路徑與 env 鎖正交、兩態恆綠。）
    insert_text(&mut e, 3, 0, "（新）");
    assert!(
        !e.can_redo(),
        "redo 後的新操作必須丟棄剩餘 redo 歷史（標準編輯器語義；clear() 被搬走過度時本燈紅）"
    );
    assert!(e.can_undo(), "新操作照舊進 undo 棧");
}

// ────────────────── 燈 3：回塞鑑別 buggy-replay（env 鎖） ──────────────────

/// 燈 3（鑑別力證明）：單一改測試內驅動 `R1_REDO_BUGGY=1`（比照倉內
/// `E2B_UNDERLINE_BUGGY` 口徑：入口 `remove_var` 防外洩、set/remove 成對）。
/// 正常態先實錄多步 redo 真通（防本燈斷言假綠），回塞態必須逐字復刻缺陷：
/// 第 2 次 redo 回 `Ok(None)`、`can_redo()` 翻 false（重放清棧＝缺陷本體）。
/// PM 雙態閘門：外部以 `R1_REDO_BUGGY=1` 整跑時燈 1 必紅；本燈兩態皆綠
/// （它斷的是「缺陷在回塞態存在、在正常態不存在」這件事本身）。
#[test]
fn r1a_lamp3_buggy_replay_discrimination() {
    let _g = env_guard();
    std::env::remove_var("R1_REDO_BUGGY");

    // ---- 正常態：多步 redo 真通（回塞斷言的對照組）----
    {
        let mut e = engine();
        three_edits(&mut e);
        for _ in 0..3 {
            e.undo().expect("正常態 undo").expect("undo 回 Some");
            settle(&mut e);
        }
        assert!(
            e.redo().expect("正常 redo #1").is_some(),
            "正常態 redo #1 必 Some"
        );
        settle(&mut e);
        assert!(
            e.redo().expect("正常 redo #2").is_some(),
            "正常態 redo #2 必 Some（多步重做真通，回塞斷言才有對照）"
        );
        settle(&mut e);
        assert!(
            e.redo().expect("正常 redo #3").is_some(),
            "正常態 redo #3 必 Some"
        );
        settle(&mut e);
        assert!(!e.can_redo(), "正常態三筆重放完棧空");
    }

    // ---- 回塞態：重放時清棧的缺陷逐字復刻 ----
    std::env::set_var("R1_REDO_BUGGY", "1");
    let buggy = {
        let mut e = engine();
        three_edits(&mut e);
        for _ in 0..3 {
            e.undo().expect("回塞 undo").expect("undo 回 Some");
            settle(&mut e);
        }
        assert!(e.can_redo(), "回塞前置：undo 後 3 筆在 redo 棧上");
        let r1 = e.redo().expect("回塞 redo #1 不該失敗").is_some();
        settle(&mut e);
        // 缺陷形態讀取必須在 remove_var 之前（redo 按呼叫當下讀 env）。
        let r2 = e.redo().expect("回塞 redo #2 不該失敗").is_some();
        let redo_alive = e.can_redo();
        std::env::remove_var("R1_REDO_BUGGY");
        (r1, r2, redo_alive)
    };
    assert!(
        buggy.0,
        "回塞態第 1 次 redo 仍成功（pop 出來的那筆重放得動）"
    );
    assert!(
        !buggy.1,
        "回塞態第 2 次 redo 必須回到缺陷形態 Ok(None)——否則回塞鎖沒接上真缺陷路徑"
    );
    assert!(
        !buggy.2,
        "回塞態第一筆重放後 can_redo() 必翻 false（redo 棧被重放清光＝缺陷本體實錄）"
    );
}
