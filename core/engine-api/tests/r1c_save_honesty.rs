//! R1-c 考卷：存檔失敗不得靜默回空 bytes ＋ 存檔必須交無損證據 ＋ 能力清單單源。
//!
//! 四條紅燈（先紅後綠）：
//! 1. **空 bytes 鎖**：`R1C_SAVE_FAULT=1` 造出一個必然讓 `Engine::save_docx()` 失敗的狀態；
//!    修復後誠實出口必須回報 `ok:false` ＋ 錯誤種類，**任何情況下都不得空 bytes 配 `ok:true`**。
//!    `R1C_SAVE_BUGGY=1` 重現修前的 `unwrap_or_default()` 吞錯態——本條必紅（鑑別力證據）。
//! 2. **證據正確鎖**：`fixtures/style_cascade.docx` 與 `corpus/ooxmlexport/style-inheritance.docx`
//!    各做一輪單段 `M1.1 InsertText`，斷言 `touched_parts == ["word/document.xml"]`、
//!    `preserved_part_count == ZIP 實數 entry 數 − 1`、`roundtrip.pass == true`、
//!    白名單變更只有 `word/document.xml`（entry 總數由本檔自行掃 central directory 實數，不寫魔數）。
//! 3. **證據純觀察鎖**：先呼叫 `save_docx_report_json()` 再存檔，與只存檔的結果逐位元組相等。
//! 4. **能力清單對照鎖**：`supported_mutations_json()` 每一項都要在 `core/engine-api/tests/`
//!    找得到至少一條**非 `#[ignore]`** 的實際考題；無考題（或只有 `#[ignore]` 考題）的
//!    variant 不得出現在清單。全部機械對照（掃 mutation.rs 列舉 ＋ 掃考卷檔），不靠人眼。
//!
//! 環境變數紀律（比照 k25b_group_child_stack／f2_wordart_projection 口徑）：
//! `set_var`／`remove_var` 成對且**只出現在一個測試內**，同檔其他測試零 env 觸碰；
//! 又因 libtest 同檔平行執行緒**共用行程 env**，燈 1 翻 env 期間燈 2／燈 3 的
//! 存檔段會被誤染（PM 值守補記：燈 1 設 R1C_SAVE_FAULT 視窗內，同行程其他燈的
//! `save_docx()` 會誠實失敗）。故三盞 Drive 段同持 `R1C_ENV` 靜態互斥鎖
//! serialize（比照 r1_redo_multistep 的 R1_ENV 口徑）。

use quire_engine::{Engine, Mutation, SubDocTarget};

/// env 翻動 serialize 鎖（PM 值守補）：燈 1 的 set/remove 全程持鎖；燈 2／燈 3
/// 的存檔段同鎖——同行程平行執行緒下「讀 env 存檔的斷言」與「翻 env 的測試」
/// 互斥，零污染（比照 r1_redo_multistep 的 R1_ENV 紀律）。
static R1C_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    R1C_ENV.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------- 通用輔助

/// 預算制軟分頁收斂（與 k29a 同口徑）：未 settle 時版面與 touched 集合都可能續變。
fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

fn load_fixture(rel: &str) -> Vec<u8> {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 {full} 失敗: {e}"))
}

/// 自行掃 zip central directory 實數「一般檔案 entry」數（目錄 entry 不計，與
/// `ZipBundle::file_entries()` 同口徑）。刻意不寫魔數、也不借用引擎的計數，
/// 這樣 preserved_part_count 才是獨立佐證而非引擎自說自話。
fn zip_file_entry_count(bytes: &[u8]) -> usize {
    // EOCD（'PK\x05\x06'）必在檔尾 22 位元組起、往後最多 65535 註解內。
    let sig = [0x50u8, 0x4b, 0x05, 0x06];
    let tail_start = bytes.len().saturating_sub(22 + 65535);
    let eocd = (bytes.len()..tail_start)
        .rev()
        .find(|&i| bytes.get(i..i + 4) == Some(&sig[..]))
        .expect("找不到 EOCD，不是合法 zip");
    let count = u16::from_le_bytes([bytes[eocd + 10], bytes[eocd + 11]]) as usize;
    let cd_off = u32::from_le_bytes([
        bytes[eocd + 16],
        bytes[eocd + 17],
        bytes[eocd + 18],
        bytes[eocd + 19],
    ]) as usize;
    // central record 簽名 'PK\x01\x02'；名稱欄（起點+46）以 '/' 結尾者＝目錄 entry。
    let mut at = cd_off;
    let mut files = 0usize;
    for _ in 0..count {
        assert_eq!(
            bytes.get(at..at + 4),
            Some(&[0x50u8, 0x4b, 0x01, 0x02][..]),
            "central record {at} 簽名不符（考卷掃描器假設失效）"
        );
        let nlen = u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]) as usize;
        let elen = u16::from_le_bytes([bytes[at + 30], bytes[at + 31]]) as usize;
        let clen = u16::from_le_bytes([bytes[at + 32], bytes[at + 33]]) as usize;
        let name = &bytes[at + 46..at + 46 + nlen];
        if !name.ends_with(b"/") {
            files += 1;
        }
        at += 46 + nlen + elen + clen;
    }
    files
}

/// 對一份 docx 做「單段 M1.1 InsertText」，回傳（引擎, 插字後的段落序）。
fn insert_once(bytes: &[u8]) -> Engine {
    let mut engine = Engine::from_docx_bytes(bytes)
        .unwrap_or_else(|e| panic!("開檔失敗: {e}"));
    settle(&mut engine);
    // 找第一個可插字的段落（cp=0 恒合法：CodepointOutOfBounds 的邊界是 cp > para_len）。
    let para = (0..engine.paragraph_count())
        .find(|&p| engine.paragraph_text(p).is_some())
        .expect("至少要有一個段落");
    let op = Mutation::InsertText {
        target: SubDocTarget::Body,
        pos: quire_engine::DocPos { para, cp: 0 },
        text: "R1c".to_string(),
        style_override: None,
    };
    engine
        .apply_mutation(op)
        .unwrap_or_else(|e| panic!("套用 M1.1 InsertText 失敗: {e}"));
    engine
}

// ------------------------------------------------- 紅燈 1：空 bytes 鎖（含回塞態）

/// 紅燈 1（**環境矩陣集中在這一個測試**，set/remove 成對，避免平行污染）：
///
/// - 正常態：注入失敗 ⇒ 誠實出口必報 `ok:false` ＋ 非空錯誤 ＋ 錯誤種類（不得壓扁成常數）。
/// - BUGGY 態（`R1C_SAVE_BUGGY=1`）：重現修前 `unwrap_or_default()`——失敗被吞成空 Vec，
///   考卷必須紅（證明本燈有鑑別力，不是空斷言）。
#[test]
fn r1c_lamp1_save_failure_never_returns_empty_bytes_silently() {
    let _g = env_guard();
    const FX: &[u8] = include_bytes!("../../../fixtures/style_cascade.docx");
    // 進程起手先把兩個 env 清乾淨（CI 可能帶著殘值進來）。
    std::env::remove_var("R1C_SAVE_FAULT");
    std::env::remove_var("R1C_SAVE_BUGGY");

    // ---- 臂 A：注入失敗態（未開 BUGGY）——修復後必綠 ----
    std::env::set_var("R1C_SAVE_FAULT", "1");
    {
        let engine = Engine::from_docx_bytes(FX).expect("開檔失敗");
        let err = engine
            .save_docx()
            .expect_err("R1C_SAVE_FAULT=1 時 save_docx() 必須誠實失敗");
        // 錯誤種類必須是真的 EngineError 結構，不是字串常數。
        let kind = quire_engine::engine_error_kind(&err);
        assert!(!kind.is_empty(), "錯誤種類不得為空: {err}");
        assert_ne!(kind, "unknown", "錯誤種類不得壓扁成 unknown: {err}");

        let v: serde_json::Value =
            serde_json::from_str(&engine.save_docx_checked_json()).expect("checked JSON 可解析");
        assert_eq!(v["ok"], false, "存檔失敗時 ok 必須是 false，實際: {v}");
        assert!(
            v.get("bytes_len").is_none() || v["bytes_len"].is_null(),
            "失敗時不得偽報 bytes_len，實際: {v}"
        );
        let msg = v["error"].as_str().unwrap_or("");
        assert!(!msg.is_empty(), "失敗時必須帶出錯誤訊息，實際: {v}");
        assert!(
            v["error_kind"].as_str().is_some_and(|k| !k.is_empty()),
            "失敗時必須帶出錯誤種類（error_kind），實際: {v}"
        );
        // 核心承諾：任何情況下都不得「空 bytes 配 ok:true」。
        assert!(
            !(v["ok"] == true && v["bytes_len"].as_u64() == Some(0)),
            "嚴禁空 bytes 配 ok:true，實際: {v}"
        );
        // 證據出口同聲：失敗時 ok 必 false，不得交出一張假無損憑證。
        let r: serde_json::Value =
            serde_json::from_str(&engine.save_docx_report_json()).expect("report JSON 可解析");
        assert_eq!(r["ok"], false, "存檔失敗時證據報告不得宣稱 ok:true: {r}");
        assert_eq!(r["error"].as_str().unwrap_or(""), msg, "兩出口錯誤須同源");
    }
    std::env::remove_var("R1C_SAVE_FAULT");

    // ---- 臂 B：BUGGY 態——修前吞錯態必紅（鑑別力證據）----
    std::env::set_var("R1C_SAVE_FAULT", "1");
    std::env::set_var("R1C_SAVE_BUGGY", "1");
    {
        let engine = Engine::from_docx_bytes(FX).expect("開檔失敗");
        let v: serde_json::Value =
            serde_json::from_str(&engine.save_docx_checked_json()).expect("checked JSON 可解析");
        // 修前態的特徵：失敗被吞掉 ⇒ 交出 ok:true ＋ 0 位元組（用戶拿到空檔）。
        // 本燈在 BUGGY 下必須紅，故把「修後應有的行为」寫成斷言，讓它如實翻紅。
        let swallowed_empty_ok = v["ok"] == true && v["bytes_len"].as_u64() == Some(0);
        assert!(
            !swallowed_empty_ok,
            "回塞態（R1C_SAVE_BUGGY=1）重現修前 unwrap_or_default() 吞錯：\
             用戶拿到 0 位元組空檔且完全不知道發生了什麼。實際: {v}"
        );
    }
    std::env::remove_var("R1C_SAVE_BUGGY");
    std::env::remove_var("R1C_SAVE_FAULT");

    // ---- 臂 C：零編輯態（未設任何 env）——正常存檔必須綠且非空 ----
    {
        let engine = Engine::from_docx_bytes(FX).expect("開檔失敗");
        let v: serde_json::Value =
            serde_json::from_str(&engine.save_docx_checked_json()).expect("checked JSON 可解析");
        assert_eq!(v["ok"], true, "零編輯存檔必須 ok:true，實際: {v}");
        assert!(v["bytes_len"].as_u64().unwrap_or(0) > 0, "零編輯存檔不得是空 bytes: {v}");
    }
}

// ------------------------------------------------- 紅燈 2：證據正確鎖

fn lamp2_evidence_lock(rel: &str) {
    let _g = env_guard();
    let bytes = load_fixture(rel);
    let total = zip_file_entry_count(&bytes);
    assert!(total > 3, "{rel} 的 entry 數異常（{total}），掃描器假設可能失效");

    let engine = insert_once(&bytes);
    let v: serde_json::Value =
        serde_json::from_str(&engine.save_docx_report_json()).expect("report JSON 可解析");

    assert_eq!(v["ok"], true, "{rel}：存檔必須成功，實際: {v}");
    assert!(v["bytes_len"].as_u64().unwrap_or(0) > 0, "{rel}：bytes_len 不得為 0: {v}");

    let touched: Vec<String> = v["touched_parts"]
        .as_array()
        .unwrap_or_else(|| panic!("{rel}：touched_parts 需為陣列: {v}"))
        .iter()
        .map(|x| x.as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        touched,
        vec!["word/document.xml".to_string()],
        "{rel}：單段 InsertText 只准重寫 word/document.xml，實際: {touched:?}"
    );

    // 未觸碰 entry 全部原位元組搬運 ⇒ 總數減一（總數自行從 ZIP 實數，不寫魔數）。
    assert_eq!(
        v["preserved_part_count"].as_u64(),
        Some((total - 1) as u64),
        "{rel}：preserved_part_count 必須等於 ZIP 實數 entry 數({total}) − 1"
    );
    assert_eq!(v["entry_total"].as_u64(), Some(total as u64), "{rel}：entry_total 與 ZIP 實數不符");

    let rt = &v["roundtrip"];
    assert_eq!(rt["pass"], true, "{rel}：roundtrip 必須 pass，實際: {rt}");
    assert_eq!(
        rt["whitelisted_changed"]
            .as_array()
            .map(|a| a.iter().map(|x| x.as_str().unwrap_or_default()).collect::<Vec<_>>())
            .unwrap_or_default(),
        vec!["word/document.xml"],
        "{rel}：白名單內真正變更的 part 只准 word/document.xml"
    );
    assert_eq!(
        rt["violations"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(usize::MAX),
        0,
        "{rel}：未觸碰 entry 零失真違規必須為 0，實際: {rt}"
    );
    assert_eq!(rt["missing"].as_array().map(|a| a.len()).unwrap_or(1), 0, "{rel}：不得缺 part");
    assert_eq!(rt["added"].as_array().map(|a| a.len()).unwrap_or(1), 0, "{rel}：不得憑空新增 part");
}

#[test]
fn r1c_lamp2_evidence_correct_on_style_cascade() {
    lamp2_evidence_lock("fixtures/style_cascade.docx");
}

#[test]
fn r1c_lamp2_evidence_correct_on_style_inheritance() {
    lamp2_evidence_lock("corpus/ooxmlexport/style-inheritance.docx");
}

// ------------------------------------------------- 紅燈 3：證據純觀察鎖

/// 先呼叫 `save_docx_report_json()` 再存檔，與只存檔的結果**逐位元組相等**：
/// 證據產出必須是純觀察，不得把 bundle／parts 摸髒（污染未觸碰 entry 的搬運依據）。
#[test]
fn r1c_lamp3_report_is_pure_observation() {
    let _g = env_guard();
    const FX: &[u8] = include_bytes!("../../../fixtures/style_cascade.docx");

    // 對照組：完全不呼叫 report。
    let mut plain = insert_once(FX);
    let bytes_plain = plain.save_docx().expect("對照組存檔失敗");

    // 實驗組：同一份編輯，先 report（連呼叫兩次，覆盖重複觀察）、再存檔。
    let mut observed = insert_once(FX);
    let _ = observed.save_docx_report_json();
    let _ = observed.save_docx_report_json();
    let bytes_observed = observed.save_docx().expect("實驗組存檔失敗");

    assert_eq!(
        bytes_plain, bytes_observed,
        "呼叫 report 前後 save_docx() 輸出必須逐位元組相等（證據不得改變存檔結果）"
    );
    // 兩組的證據本身也必須可重現（同編輯 ⇒ 同 JSON）。
    assert_eq!(
        plain.save_docx_report_json(),
        observed.save_docx_report_json(),
        "同編輯態重復取證必須給出相同證據"
    );
}

// ------------------------------------------------- 紅燈 4：能力清單對照鎖

/// 讀取來源檔文字（相對於 workspace 根）。
fn read_repo(rel: &str) -> String {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"))
}

/// 機械掃 `pub enum Mutation { ... }` 的 variant 名（大寫開頭、頂層四格縮排），
/// 到該列舉的閉合 brace 為止。不做語意猜測，只認「列舉體內的 variant 宣告」。
fn mutation_variants_from_source() -> Vec<String> {
    let src = read_repo("core/engine-api/src/mutation.rs");
    let start = src
        .find("pub enum Mutation {")
        .expect("找不到 `pub enum Mutation {`");
    let body = &src[start..];
    let mut out: Vec<String> = Vec::new();
    for line in body.lines().skip(1) {
        let t = line.trim_end();
        if t == "}" {
            break;
        }
        // variant 宣告形如：`    InsertText {` 或 `    DeleteRange { range: DocRange },`
        let Some(head) = t.strip_prefix("    ") else { continue };
        if head.starts_with(' ') || head.starts_with('/') || head.starts_with('#') || head.starts_with('}') {
            continue;
        }
        let word: String = head
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if word.is_empty() || !word.chars().next().unwrap().is_uppercase() {
            continue;
        }
        // 只認「大寫 variant ＋空白＋'{'」或「大寫 variant ＋ ','」兩種形態。
        let rest = &head[word.len()..];
        if rest.starts_with(" {") || rest.trim_start() == "," || rest.trim().is_empty() {
            out.push(word);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 掃 `core/engine-api/tests/*.rs`，回傳「有**非 `#[ignore]` 考題**引用到」的 variant 名集合。
/// `#[ignore]` 標記在其所屬 `fn` 之前出線即視為該 fn 整段是 ignore 考題（比照 g1/e_probe
/// 的寫法：`#[test]` ＋ `#[ignore = "..."]` 緊接 `fn`）。
fn variants_tested_by_exam_paper() -> (std::collections::BTreeSet<String>, Vec<String>) {
    use std::collections::BTreeSet;
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests");
    let mut tested: BTreeSet<String> = BTreeSet::new();
    let mut ignored_hits: Vec<String> = Vec::new();
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .expect("讀不到 tests 目錄")
        .filter_map(|e| e.ok().and_then(|e| e.file_name().to_str().map(str::to_string)))
        .filter(|n| n.ends_with(".rs"))
        .collect();
    files.sort();
    for f in files {
        let text = std::fs::read_to_string(format!("{dir}/{f}")).expect("讀考卷失敗");
        // 逐行掃描：追問「這一行位於 ignore 掉的 fn 體內嗎？」
        let mut in_ignored_fn = false;
        let mut depth_seen_attr_ignore = false;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with("#[ignore") {
                depth_seen_attr_ignore = true;
                continue;
            }
            if t.starts_with("#[test]") {
                // 新的 #[test]：上一個 ignore 若還沒吃到 fn，屬陣列外殘值 ⇒ 重設。
                if !depth_seen_attr_ignore {
                    in_ignored_fn = false;
                }
                continue;
            }
            if t.starts_with("fn ") || t.starts_with("pub fn ") {
                in_ignored_fn = depth_seen_attr_ignore;
                depth_seen_attr_ignore = false;
            }
            for cap in t.match_indices("Mutation::") {
                let after = &t[cap.0 + "Mutation::".len()..];
                let word: String = after
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if word.is_empty() {
                    continue;
                }
                if in_ignored_fn {
                    let tag = format!("{f}:{word}");
                    if !ignored_hits.contains(&tag) {
                        ignored_hits.push(tag);
                    }
                } else {
                    tested.insert(word.clone());
                }
            }
        }
    }
    (tested, ignored_hits)
}

#[test]
fn r1c_lamp4_supported_mutations_matches_exam_paper_census() {
    // 1) 列舉源頭對照：能力清單目錄必須逐一对上 mutation.rs 的 variant（漂移閘）。
    let src_variants = mutation_variants_from_source();
    assert!(
        src_variants.len() >= 30,
        "mutation.rs 列舉掃描結果異常（只有 {} 條），掃描器假設失效",
        src_variants.len()
    );
    let catalog: Vec<String> = quire_engine::mutation_surface()
        .into_iter()
        .map(|m| m.variant.to_string())
        .collect();
    let mut catalog_sorted = catalog.clone();
    catalog_sorted.sort();
    assert_eq!(
        catalog_sorted, src_variants,
        "能力目錄與 `pub enum Mutation` 不同源（缺: {:?}／多: {:?}）",
        src_variants
            .iter()
            .filter(|v| !catalog.contains(v))
            .collect::<Vec<_>>(),
        catalog
            .iter()
            .filter(|v| !src_variants.contains(v))
            .collect::<Vec<_>>()
    );

    // 2) 考卷 census：每一項 supported 都要有非 ignore 的實際考題。
    let (tested, ignored_hits) = variants_tested_by_exam_paper();
    let reported: Vec<String> = serde_json::from_str::<serde_json::Value>(
        &quire_engine::Engine::supported_mutations_json(),
    )
    .expect("supported_mutations_json 必須是合法 JSON")
    .as_array()
    .expect("外層必須是陣列")
    .iter()
    .map(|x| x["variant"].as_str().unwrap_or_default().to_string())
    .collect();

    for item in &reported {
        assert!(
            tested.contains(item),
            "能力清單回報了 {item}，但 core/engine-api/tests/ 找不到任何非 #[ignore] 考題（清單不得吹噓未驗證能力）"
        );
    }
    // 3) 反向：有非 ignore 考題的 variant 必須被回報（不得藏能力）。
    for v in src_variants.iter().filter(|v| tested.contains(*v)) {
        assert!(
            reported.contains(v),
            "{v} 有非 #[ignore] 考題卻沒出現在能力清單（清單不得漏報已驗證能力）"
        );
    }
    // 4) 只有 ignore 考題（或完全沒考題）者，一律不得出現；且必須在目錄裡被誠實標記未驗證。
    let untested: Vec<&String> = src_variants.iter().filter(|v| !tested.contains(*v)).collect();
    for v in &untested {
        assert!(
            !reported.contains(v),
            "{v} 查無非 #[ignore] 考題，不得出現在能力清單（ignore 考題不算通過）"
        );
    }
    let unproven_in_catalog: Vec<String> = quire_engine::mutation_surface()
        .into_iter()
        .filter(|m| !m.proven)
        .map(|m| m.variant.to_string())
        .collect();
    let mut untested_str: Vec<String> = untested.iter().map(|s| s.to_string()).collect();
    untested_str.sort();
    let mut unproven_sorted = unproven_in_catalog.clone();
    unproven_sorted.sort();
    assert_eq!(
        unproven_sorted, untested_str,
        "目錄中 proven:false 的 variant 必須恰好等於「查無非 ignore 考題」集合（機械對照，不靠人眼）"
    );
    // 留檔：把只有 ignore 考題的命中列印出來，交付報告可直接引用。
    if !ignored_hits.is_empty() {
        eprintln!("[r1c_lamp4] 僅 #\[ignore\] 考題命中的 variant 引用: {ignored_hits:?}");
    }
}
