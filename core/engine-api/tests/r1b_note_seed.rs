//! R1-b 紅燈考卷（根因 2）：`recompute_notes` 收斂種子化——零變動檔零清快取零重排。
//!
//! 卡面門檻（R1-b 修法 2）：「`recompute_notes()` 收斂迴圈改從現行
//! `self.params.*` 初始比對；**未變動時零呼叫 `update_params()`、零清排版快取**」。
//!
//! 探針口徑：`Engine::relayout_calls()`＝自程序啟動以來
//! `LayoutEngine::update_pack_params/update_params`（換分頁參數＋重排）的累計次數；
//! `Engine::reset_relayout_calls()` 歸零後做增量比對（AtomicU64，跨考卷平行安全）。
//!
//! 燈序列：
//! - 燈 1（先紅主鎖）：含註腳檔開檔定案後，插入一字 ⇒ 若逐頁預留表不變，
//!   recompute 段的 relayout 增量必須 **0**。修復前（`applied` 空表起步＋每輪
//!   無條件 `update_params`）此增量恆 ≥2（bug65649 實測每次編輯付 2 次整篇
//!   重排、每次 211ms）⇒ 本盞在修前必紅。
//! - 燈 2（快路徑確定性鎖）：同一份位元組＋同一mutation 序列，兩個獨立引擎
//!   收斂後的顯示列表與頁數必須逐位相同——`update_pack_params` 保留斷行快取
//!   只重跑打包，若快取與新預留表不自洽，這裡會分岔（先紅後綠由
//!   update_pack_params 引入前的等價性保證，回塞鎖 `R1B_PACK_CACHE_BUGGY`）。
//! - 燈 3（鑑別力）：`R1B_NOTE_SEED_BUGGY=1` 由外部執行（本檔內**不設 env**，
//!   比照 `K7B_HDR_PUSH_BUGGY`／`K6_AUTO_PITCH_BUGGY` 紀律），回塞態下燈 1
//!   的增量必 >0——由 `scripts/r1b_note_seed_buggy_replay.sh` 機械證明。
//!
//! 無損零回退（本卡最高優先）不由本檔負責：K29 系列 `byte_identical`／
//! `zero_loss` 考卷＋roundtrip corpus 守門；本檔只鎖「收斂成本」行為。
//!
//! workload：`corpus/poi/test-data_document_footnotes.docx`（語料最小含註腳檔，
//! 16KB；`footnotes.xml` 有被引用的註腳 ⇒ 開檔即有非空逐頁預留表）。

use quire_engine::{DocPos, Engine, Mutation, SubDocTarget};

fn corpus(rel: &str) -> Vec<u8> {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 {full} 失敗: {e}"))
}

const FOOT_DOC: &str = "corpus/poi/test-data_document_form_footnotes.docx";

/// 找一個**不掛註腳錨點、且插入一字不推頁**的段落：從文件後段往回掃，
/// 用「插入前後 `page_count` 相同」作頁漂移守門（真推頁的段落直接換下一個
/// 候選——探針量的是「零變動⇒零重排」，段落選擇必須落在零變動的實例上）。
/// `Engine` 非 `Clone`：每個候選用同一份位元組重開一個臨時引擎探測。
fn zero_change_para(bytes: &[u8], page_count: usize) -> Option<usize> {
    let n = Engine::from_docx_bytes(bytes)
        .expect("開檔失敗")
        .paragraph_count();
    if n <= 8 {
        return None;
    }
    // 後 1/3 優先（避開標題／註腳密集的開頭段），再退全檔掃。
    let order: Vec<usize> = ((n * 2 / 3)..n).chain((0..n * 2 / 3).rev()).collect();
    for &para in &order {
        let mut probe = match Engine::from_docx_bytes(bytes) {
            Ok(e) => e,
            Err(_) => return None,
        };
        if probe
            .apply_mutation(Mutation::InsertText {
                target: SubDocTarget::Body,
                pos: DocPos { para, cp: 2 },
                text: "字".to_string(),
                style_override: None,
            })
            .is_err()
        {
            continue; // 該段拒絕插入（受保護結構等）：換下一個候選。
        }
        if probe.page_count() == page_count {
            return Some(para);
        }
    }
    None
}

fn insert_char(engine: &mut Engine, para: usize) {
    engine
        .apply_mutation(Mutation::InsertText {
            target: SubDocTarget::Body,
            pos: DocPos { para, cp: 2 },
            text: "字".to_string(),
            style_override: None,
        })
        .expect("InsertText 失敗");
}

#[test]
fn r1b_lamp1_zero_change_recompute_relayouts_zero() {
    let bytes = corpus(FOOT_DOC);
    let engine = Engine::from_docx_bytes(&bytes).expect("開檔失敗");
    let para = zero_change_para(&bytes, engine.page_count())
        .expect("找不到零變動插入段（workload 異常）");

    // 計數歸零後只量「編輯觸發的 recompute」段。
    Engine::reset_relayout_calls();
    let mut engine = engine;
    insert_char(&mut engine, para);
    let delta = Engine::relayout_calls();
    // 主鎖：插入一字未動任何頁的註腳預留 ⇒ 收斂從現行 reserves 起步直接定案，
    // 換參重排零次。修前恆 ≥2（空表起步無條件清快取重排）⇒ 先紅。
    assert_eq!(
        delta, 0,
        "零註腳位置變動的編輯仍觸發 {delta} 次換參重排——recompute_notes \
         收斂未從現行 reserves 起步（根因 2 回退）"
    );
    // 存檔路徑零污染：編輯只該進 document.xml 白名單。
    let diff = engine.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 零失真未過：{:?}", diff.violations);
    let strayed: Vec<String> = diff
        .whitelisted_changed
        .iter()
        .filter(|n| n.as_str() != "word/document.xml")
        .cloned()
        .collect();
    assert!(strayed.is_empty(), "打字多動了 part：{strayed:?}");
}

#[test]
fn r1b_lamp2_pack_cache_determinism() {
    // 同位元組＋同編輯序列 ⇒ 兩獨立引擎必須收斂到同一版面（顯示列表逐位相同）。
    // `update_pack_params` 保快取只重跑打包的確定性鎖：快取與新預留表若不自洽，
    // 兩引擎在「開檔即定案 vs 編輯後換參」兩條路徑上會分岔。
    let bytes = corpus(FOOT_DOC);
    let mut a = Engine::from_docx_bytes(&bytes).expect("開檔失敗 A");
    let mut b = Engine::from_docx_bytes(&bytes).expect("開檔失敗 B");

    let para = zero_change_para(&bytes, a.page_count())
        .expect("找不到零變動插入段（workload 異常）");
    insert_char(&mut a, para);
    // B 走「編輯→Undo 回原點→再編輯同一段」：換參歷史不同（多一輪還原重排），
    // 終態必須與 A 逐位一致（發射↔分頁自洽不變量的確定性面）。
    insert_char(&mut b, para);
    b.undo().expect("undo").expect("undo 有內容");
    insert_char(&mut b, para);

    assert_eq!(a.page_count(), b.page_count(), "A/B 頁數分岔");
    for p in 0..a.page_count() {
        let da = a.page_display_list(p).expect("A 頁存在");
        let db = b.page_display_list(p).expect("B 頁存在");
        let sa: Vec<String> = da.items.iter().map(|i| format!("{i:?}")).collect();
        let sb: Vec<String> = db.items.iter().map(|i| format!("{i:?}")).collect();
        assert_eq!(sa, sb, "頁 {p} 顯示列表分岔（打包快取不自洽）");
    }
}
