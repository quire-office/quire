//! R1-b 紅燈考卷（根因 1）：`rebuild_derived()` 局部重建 ≡ 整包重開——
//! byte 級同形＋派生期狀態保留。PM 裁定（run 5 放行 comment）：
//! 「增量收尾安全條件矩陣須先紅考卷鎖『局部態與整包態逐頁 byte 級同形』
//! （K29 口徑）——這條紅燈沒落地前不許投藥。」
//!
//! 燈序列（先紅後綠口徑）：
//! - 燈 1（PaperSpec 跨重建保留，**修前必紅**）：以 `PaperSpec`（A3）開檔，
//!   做一次結構編輯（`SetParagraphStyle` ⇒ `rebuild_derived()`）後
//!   `paper_json` 仍必須是 A3。修前：`rebuild_derived` 走
//!   `from_docx_bytes(&saved)`（不帶 paper）⇒ 覆寫被靜默沖銷 ⇒ 本盞紅。
//!   Undo 臂同樣要鎖（undo 內側亦呼叫 rebuild_derived）。
//! - 燈 2（sync_budget 跨重建保留，**修前必紅**）：`set_sync_budget(Some(3))`
//!   後結構編輯，`sync_budget()` 必須仍是 `Some(3)`。修前：整包重開把
//!   `LayoutEngine` 連預算一起重建 ⇒ `None` ⇒ 本盞紅。
//! - 燈 3（byte 級同形主鎖，K29 口徑）：同一份位元組、同一 mutation 序列，
//!   編輯態引擎 A（現行重建路徑產物）與「把 A 存檔後 `from_docx_bytes`
//!   重開的獨立引擎 C」**逐頁顯示列表 JSON byte-identical**＋頁數相同＋
//!   `A.save_docx() == C.save_docx()` byte-identical。修前兩側同路徑（同形
//!   平凡成立＝基準鎖）；局部重建落地後本盞才是真鑑別——局部派生與「序列化
//!   再整包重開」若有任何排版分岔，逐頁 JSON 立刻爆紅。
//! - 燈 4（Undo 全淨回歸鎖）：結構編輯→Undo 全淨後，逐頁顯示列表與「原始
//!   位元組直接開檔」byte-identical，且 `save_docx()` 與原始位元組
//!   byte-identical（K29 bundle 基準保真鐵律的考卷化）。
//!
//! 鑑別回塞（閘門 3）：`R1B_BUGGY=1` 把 `rebuild_derived` 刷回「全檔
//! save＋from_docx_bytes 整包重開」舊路徑——由
//! `scripts/r1b_local_isomorph_buggy_replay.sh` 於外部執行驗證燈 1／燈 2
//! 必紅（本檔內**不設 env**，比照 `R1B_NOTE_SEED_BUGGY`／`K7B_HDR_PUSH_BUGGY`
//! 紀律）。燈 3／燈 4 是正確性鎖，兩態都必須綠。
//!
//! 無損零回退（本卡最高優先）由 K29 系列 `byte_identical`／`zero_loss` 考卷
//! ＋roundtrip corpus 守門；本檔鎖的是「重建路徑的派生同形與狀態保留」。

use quire_engine::{DocPos, DocRange, Engine, Mutation, PaperSpec, ParaPropsPatch, SubDocTarget};

const SECTION_DOC: &[u8] =
    include_bytes!("../../../corpus/ooxmlexport/tdf124637_sectionMargin.docx");
const BUG65649: &[u8] = include_bytes!("../../../corpus/poi/test-data_document_bug65649.docx");

// ISO 216 A3 點數尺寸（與 pg_paper 同口徑）。
const A3_W_PT: f32 = 841.89;
const A3_H_PT: f32 = 1190.55;

#[derive(Debug, serde::Deserialize)]
struct PaperJson {
    page_w_pt: f32,
    page_h_pt: f32,
    paper_override_applied: bool,
}

fn a3_spec() -> PaperSpec {
    PaperSpec {
        page_w_pt: A3_W_PT,
        page_h_pt: A3_H_PT,
        margin_top_pt: None,
        margin_bottom_pt: None,
        margin_left_pt: None,
        margin_right_pt: None,
    }
}

/// 一個觸發 `rebuild_derived()` 的結構編輯（bench (a) 同型）。
fn structural_edit(engine: &mut Engine, para: usize) {
    engine
        .apply_mutation(Mutation::SetParagraphStyle {
            target: SubDocTarget::Body,
            para_range: para..=para,
            patch: ParaPropsPatch {
                page_break_before: Some(true),
                ..Default::default()
            },
        })
        .expect("結構編輯（SetParagraphStyle）失敗");
}

fn paper_of(engine: &Engine) -> PaperJson {
    serde_json::from_str(&engine.paper_json()).expect("paper_json 解析失敗")
}

/// 燈 1：PaperSpec 覆寫必須跨越 `rebuild_derived()`（結構編輯與 Undo 雨臂）。
/// 修前必紅：整包重開以 `from_docx_bytes(&saved)`（不帶 paper）重建 ⇒
/// 覆寫值靜默遺失。
#[test]
fn lamp1_paper_spec_survives_rebuild() {
    let mut engine =
        Engine::from_docx_bytes_with_paper(SECTION_DOC, Some(&a3_spec())).expect("A3 覆寫開檔失敗");
    let before = paper_of(&engine);
    assert!(before.paper_override_applied);
    assert!(
        (before.page_w_pt - A3_W_PT).abs() < 0.01 && (before.page_h_pt - A3_H_PT).abs() < 0.01,
        "開檔後即應為 A3，實得 {}×{}",
        before.page_w_pt,
        before.page_h_pt
    );

    structural_edit(&mut engine, 0);
    let after = paper_of(&engine);
    assert!(
        after.paper_override_applied,
        "結構編輯觸發 rebuild_derived 後 paper_override_applied 被沖銷（派生層整包重開未帶 paper）"
    );
    assert!(
        (after.page_w_pt - A3_W_PT).abs() < 0.01 && (after.page_h_pt - A3_H_PT).abs() < 0.01,
        "結構編輯後紙張必須仍是 A3，實得 {}×{}",
        after.page_w_pt,
        after.page_h_pt
    );

    engine
        .undo()
        .expect("undo 必須成功")
        .expect("undo 必須有內容");
    let undone = paper_of(&engine);
    assert!(
        undone.paper_override_applied
            && (undone.page_w_pt - A3_W_PT).abs() < 0.01
            && (undone.page_h_pt - A3_H_PT).abs() < 0.01,
        "Undo（同樣觸發 rebuild_derived）後紙張必須仍是 A3，實得 {}×{}（applied={:?}）",
        undone.page_w_pt,
        undone.page_h_pt,
        undone.paper_override_applied
    );
}

/// 燈 2：預算制軟分頁的 `sync_budget` 必須跨越 `rebuild_derived()`。
/// 修前必紅：整包重開連 `LayoutEngine` 一起重生 ⇒ 預算靜默歸 `None`。
#[test]
fn lamp2_sync_budget_survives_rebuild() {
    let mut engine = Engine::from_docx_bytes(SECTION_DOC).expect("開檔失敗");
    engine.set_sync_budget(Some(3));
    assert_eq!(engine.sync_budget(), Some(3), "設定後探針必須讀得到 3");

    structural_edit(&mut engine, 0);
    assert_eq!(
        engine.sync_budget(),
        Some(3),
        "結構編輯觸發 rebuild_derived 後 sync_budget 被靜默 reset（卡面根因 1 副作用：預算制軟分頁被清零）"
    );

    engine.undo().expect("undo 呼叫失敗");
    assert_eq!(
        engine.sync_budget(),
        Some(3),
        "Undo 觸發 rebuild_derived 後 sync_budget 必須仍是 Some(3)"
    );
}

/// 燈 3（byte 級同形主鎖）：編輯態引擎 A 的派生層 ≡ 「A.save_docx() 再
/// `from_docx_bytes` 整包重開」的引擎 C——逐頁顯示列表 JSON 逐位相同、
/// 頁數相同、存檔位元組逐位相同。局部重建落地後這是「局部態與整包態同形」
/// 的機械證明（K29 口徑）；任何派生層分岔（級聯、表格槽、註腳收斂、頁首
/// 推下、圖片池）都會讓某一頁 JSON 立刻爆紅。
#[test]
fn lamp3_edited_state_isomorphic_to_full_reopen() {
    let mut a = Engine::from_docx_bytes(SECTION_DOC).expect("A 開檔失敗");
    structural_edit(&mut a, 0);
    // 第二發：跨段落格式編輯（rPr 路徑，同樣觸發 rebuild）。
    a.apply_mutation(Mutation::FormatTextRange {
        range: DocRange {
            target: SubDocTarget::Body,
            start: DocPos { para: 0, cp: 0 },
            end: DocPos { para: 0, cp: 1 },
        },
        patch: quire_engine::RunDirectPatch {
            italic: Some(true),
            ..Default::default()
        },
    })
    .expect("FormatTextRange 失敗");

    let saved = a.save_docx().expect("A 存檔失敗");
    let c = Engine::from_docx_bytes(&saved).expect("C（整包重開）失敗");

    assert_eq!(
        a.page_count(),
        c.page_count(),
        "A（編輯態重建）與 C（存檔重開）頁數必須相同"
    );
    for p in 0..a.page_count() {
        let da = a
            .page_display_list_json(p)
            .unwrap_or_else(|| panic!("A 第 {p} 頁缺失"));
        let dc = c
            .page_display_list_json(p)
            .unwrap_or_else(|| panic!("C 第 {p} 頁缺失"));
        assert_eq!(da, dc, "第 {p} 頁顯示列表必須 byte-identical（重建≡重開）");
    }
    let saved_c = c.save_docx().expect("C 存檔失敗");
    assert_eq!(
        saved, saved_c,
        "A.save_docx() 與重開後 C.save_docx() 必須 byte-identical（save 收斂性）"
    );
}

/// 燈 3b（同形主鎖·大檔臂）：bug65649（190 頁多節混排）同樣逐頁 byte 級
/// 同形——小檔過、大檔掛（表格槽／圖片池／節幾何规模相關分岔）由本盞抓。
#[test]
fn lamp3b_isomorphism_on_bug65649() {
    let mut a = Engine::from_docx_bytes(BUG65649).expect("A 開檔失敗");
    let mid = a.paragraph_count() / 2;
    structural_edit(&mut a, mid);

    let saved = a.save_docx().expect("A 存檔失敗");
    let c = Engine::from_docx_bytes(&saved).expect("C（整包重開）失敗");

    assert_eq!(a.page_count(), c.page_count(), "大檔頁數必須同形");
    assert!(
        a.page_count() >= 150,
        "workload 頁數異常：{}",
        a.page_count()
    );
    for p in 0..a.page_count() {
        let da = a
            .page_display_list_json(p)
            .unwrap_or_else(|| panic!("A 第 {p} 頁缺失"));
        let dc = c
            .page_display_list_json(p)
            .unwrap_or_else(|| panic!("C 第 {p} 頁缺失"));
        assert_eq!(da, dc, "大檔第 {p} 頁顯示列表必須 byte-identical");
    }
    assert_eq!(a.save_docx().expect("A 再存檔"), saved, "save 收斂性");
}

/// 燈 4（Undo 全淨回歸鎖）：結構編輯→Undo 全淨後逐頁顯示列表 ≡「原始位元組
/// 直接開檔」，且存檔 byte-identical（K29 bundle 鐵律的考卷化；重建路徑若
/// 讓派生層與重開態分岔，這裡先紅）。
#[test]
fn lamp4_undo_clean_restores_display_and_bytes() {
    let fresh = Engine::from_docx_bytes(SECTION_DOC).expect("基準開檔失敗");
    let mut engine = Engine::from_docx_bytes(SECTION_DOC).expect("編輯開檔失敗");
    structural_edit(&mut engine, 0);
    while engine.can_undo() {
        engine.undo().expect("收尾 undo").expect("undo 有內容");
    }
    assert_eq!(engine.page_count(), fresh.page_count(), "Undo 全淨頁數復原");
    for p in 0..engine.page_count() {
        assert_eq!(
            engine
                .page_display_list_json(p)
                .unwrap_or_else(|| panic!("engine 第 {p} 頁缺失")),
            fresh
                .page_display_list_json(p)
                .unwrap_or_else(|| panic!("fresh 第 {p} 頁缺失")),
            "Undo 全淨第 {p} 頁必須與重開態 byte-identical"
        );
    }
    let saved = engine.save_docx().expect("存檔失敗");
    assert_eq!(
        saved.as_slice(),
        SECTION_DOC,
        "Undo 全淨後存檔必須 byte-identical（bundle 基準保真鐵律）"
    );
}
