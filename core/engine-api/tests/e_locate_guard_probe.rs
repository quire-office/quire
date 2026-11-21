//! E 返修卡 t_c1dbc6d7 **全語料假拒收探針**（`#[ignore]`，PM 手跑，不掛 CI）。
//!
//! # 為什麼需要它
//! S2-1 的身分驗證防線（`table_identity_error`）是「寧可拒收、不可靜默錯改」。
//! 防線本身正確，但它**可能過度拒收**：把正常檔（無透明包裝、兩套口徑本就一致）
//! 也一併擋掉，就是使用者可见的能力回退——考卷綠燈看不出來（斷言只驗「拒收時
//! 零波及」，不會因為「拒收太多」而紅）。本探針把這件事變成可量測的數字。
//!
//! # 紀律
//! - `#[ignore]`：本檔是**探針**不是斷言閘門。全語料 400 檔×每檔多條路徑的
//!   Mutation 掃描耗時過長，不得進 `cargo test --workspace`（零回退鐵閘）。
//! - 跑法：`cargo test -p quire-engine --test e_locate_guard_probe -- --ignored --nocapture`
//! - 唯一的硬斷言（必須綠）：**零靜默錯改**——凡 `Ok` 的路徑，被改的根表集合必恰
//!   為反查宣稱的那一張。拒收率只報數字、不設閥值（語料再生成不假綠、也不誤紅）。
//! - 「防線确实有在擋東西」的正向對照＝`e_sdt_wrapped_table` 燈 2／燈 6（修前紅
//!   已存證 docs/e-locate-parity-redlight.md §4），本探針只補反向的過度拒收面。
//!
//! 繁中註解、禁簡體禁 emoji，署名 ADT109119。

use quire_engine::{Engine, Mutation, SubDocTarget, TablePath, TablePropsPatch};
use serde_json::Value;

fn corpus_manifest() -> Vec<String> {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/manifest.json"
    ))
    .expect("corpus/manifest.json 不在場");
    let v: Value = serde_json::from_str(&raw).expect("manifest 非合法 JSON");
    // manifest 實測欄位名＝`file`（相對於 corpus/）；兼容其他形狀，但**不可**在
    // 欄位名對不上時靜默回空集合（空清單＝探針空轉報假綠，本卡踩过這個坑）。
    let arr = v
        .get("files")
        .and_then(|f| f.as_array())
        .cloned()
        .or_else(|| v.as_array().cloned())
        .expect("manifest 頂層必須是 files 陣列或陣列");
    let mut out = Vec::with_capacity(arr.len());
    for it in &arr {
        let rel = it
            .get("file")
            .and_then(|s| s.as_str())
            .or_else(|| it.as_str())
            .unwrap_or_else(|| {
                panic!("manifest 條目缺少 `file` 欄位（欄位名改版＝探針會空轉）: {it}")
            });
        out.push(rel.to_string());
    }
    assert!(
        out.len() >= 300,
        "manifest 條目數異常（探針不得空轉）：實得 {}",
        out.len()
    );
    out
}

fn locate(e: &Engine, para: usize) -> Option<Value> {
    e.table_locator_json(para)
        .map(|s| serde_json::from_str(&s).expect("反查輸出必須合法 JSON"))
}

fn path_of(hit: &Value) -> TablePath {
    TablePath {
        target: SubDocTarget::Body,
        root_slot: hit["table_path"]["root_slot"].as_u64().unwrap() as usize,
        nested_steps: hit["table_path"]["nested_steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let a = s.as_array().unwrap();
                (
                    a[0].as_u64().unwrap() as usize,
                    a[1].as_u64().unwrap() as usize,
                    a[2].as_u64().unwrap() as usize,
                )
            })
            .collect(),
    }
}

fn centered_roots(e: &Engine) -> Vec<usize> {
    let v: Value = serde_json::from_str(&e.tables_json()).expect("tables_json 合法");
    v.as_array()
        .map(|roots| {
            roots
                .iter()
                .enumerate()
                .filter(|(_, t)| subtree_centered(&t["table"]))
                .map(|(i, _)| i)
                .collect()
        })
        .unwrap_or_default()
}

/// 節點（含巢狀子表）任一處 center——判斷「該根表子樹是否已被動過」。
fn subtree_centered(node: &Value) -> bool {
    if node["props"]["justify"].as_str() == Some("center") {
        return true;
    }
    node["rows"]
        .as_array()
        .map(|rows| {
            rows.iter().any(|r| {
                r["cells"]
                    .as_array()
                    .map(|cs| {
                        cs.iter().any(|c| {
                            c["blocks"]
                                .as_array()
                                .map(|bs| {
                                    bs.iter()
                                        .any(|b| b.get("Table").is_some_and(subtree_centered))
                                })
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

#[test]
#[ignore = "探針（非 CI 閘門）：全語料假拒收計數，PM 手跑；跑法見檔頭。"]
fn probe_full_corpus_false_refusal_rate() {
    let mut files_with_tables = 0usize;
    let mut files_skipped = 0usize;
    let mut paths = 0usize;
    let mut applied = 0usize;
    let mut refused = 0usize;
    let mut already_centered = 0usize;
    let mut refuse_detail: Vec<(String, String)> = Vec::new();

    for rel in corpus_manifest() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + &rel;
        let bytes = match std::fs::read(&p) {
            Ok(b) => b,
            Err(_) => {
                files_skipped += 1;
                continue;
            }
        };
        let Ok(e0) = Engine::from_docx_bytes(&bytes) else {
            files_skipped += 1;
            continue;
        };
        if e0.table_count() == 0 {
            continue;
        }
        files_with_tables += 1;

        // 每檔取每張根表的第一個可反查段落。
        let mut per_slot: Vec<(usize, Value)> = Vec::new();
        for q in 0..e0.paragraph_count() {
            if let Some(h) = locate(&e0, q) {
                let slot = h["table_path"]["root_slot"].as_u64().unwrap() as usize;
                if !per_slot.iter().any(|(s, _)| *s == slot) {
                    per_slot.push((slot, h));
                }
            }
        }
        for (slot, hit) in per_slot {
            paths += 1;
            let mut e = match Engine::from_docx_bytes(&bytes) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let pristine = Engine::from_docx_bytes(&bytes).expect("基準引擎");
            let before = centered_roots(&pristine);
            let res = e.apply_mutation(Mutation::SetTableProps {
                table: path_of(&hit),
                patch: TablePropsPatch {
                    alignment: Some("center".to_string()),
                    ..Default::default()
                },
            });
            match res {
                Ok(_) => {
                    applied += 1;
                    let roots: Value = serde_json::from_str(&e.tables_json()).expect("tables_json");
                    let roots = roots.as_array().expect("陣列");
                    let changed: Vec<usize> = roots
                        .iter()
                        .enumerate()
                        .filter(|(i, t)| {
                            *i != slot && subtree_centered(&t["table"]) && !before.contains(i)
                        })
                        .map(|(i, _)| i)
                        .collect();
                    assert!(
                        changed.is_empty(),
                        "{rel} root_slot {slot} 成功但波及他表 {changed:?} ⇒ 界內錯位（S2-1 漏網）"
                    );
                    assert!(
                        subtree_centered(&roots[slot]["table"]),
                        "{rel} root_slot {slot} 成功但目標表未被改 ⇒ 靜默空寫"
                    );
                    if before.contains(&slot) {
                        // 目標根表**原本**就自帶 center ⇒「目標被改到」這條斷言
                        // 在此檔是恆真（無法鑑別）。計數如實報出，不謊稱覆蓋。
                        already_centered += 1;
                    }
                }
                Err(err) => {
                    refused += 1;
                    if refuse_detail.len() < 40 {
                        refuse_detail.push((rel.clone(), format!("{err:?}")));
                    }
                    assert_eq!(
                        centered_roots(&e),
                        before,
                        "{rel} root_slot {slot} 拒絕後必須零半寫"
                    );
                }
            }
        }
    }

    eprintln!(
        "探針實測：有表檔 {files_with_tables}（跳過 {files_skipped}）／反查路徑 {paths}；\
         套改 {applied}（其中目標表原自帶 center、該斷言恆真者 {already_centered} 條）、\
         誠實拒絕 {refused}（拒收率 {:.1}%）",
        100.0 * refused as f64 / paths.max(1) as f64
    );
    for (rel, err) in &refuse_detail {
        eprintln!("  拒收 {rel}: {err}");
    }
    assert!(
        applied > 0,
        "全語料套改數為 0＝防線把所有檔都擋了（極度過度拒收），實得拒絕 {refused} 條"
    );
}
