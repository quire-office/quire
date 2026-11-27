//! R1-c（紅燈 4 單源）：build 期機械生成「編輯操作能力目錄」。
//!
//! 生成物：`R1C_MUTATION_CENSUS=<Variant>:<0|1>|...`（字典序），由
//! `src/save_honesty.rs` 以 `env!` 讀取。單源紀律：
//! - variant 目錄：掃描 `src/mutation.rs` 的 `pub enum Mutation { ... }`
//!   列舉體（大寫開頭 variant 宣告，到列舉閉合 brace 為止）。
//! - proven：掃描 `tests/*.rs`，找「非 `#[ignore]` 考題」引用的
//!   `Mutation::Variant`；`#[ignore]` 標記緊接其 `fn` 前出線 ⇒ 該 fn 整段
//!   是 ignore 考題，引用不算通過。
//!
//! 演算法與 `tests/r1c_save_honesty.rs` 燈 4 的運行期掃描**逐字同構**——
//! 兩側由燈 4 本身機械對照（清單 vs 考卷 census 雙向一致），人眼退出迴路。
//! 本腳本刻意零外鍵（零 `from_*` 呼叫、零 panic 路徑：掃描假設失效時
//! **不中斷 build**，交紅燈 4 報錯，比照「編譯器是朋友，考卷是法官」）。
//!
//! `cargo:rerun-if-changed`：`src/mutation.rs`（列舉源頭）＋ `tests/`
//! （考卷）。workspace lint（unsafe_code deny）本套用不到 unsafe。

use std::collections::BTreeSet;
use std::path::Path;

/// 掃描 `pub enum Mutation { ... }` 列舉體的 variant 名（與考卷燈 4 同算法）。
fn mutation_variants(path: &Path) -> Vec<String> {
    let Ok(src) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Some(start) = src.find("pub enum Mutation {") else {
        return Vec::new();
    };
    let body = &src[start..];
    let mut out: Vec<String> = Vec::new();
    for line in body.lines().skip(1) {
        let t = line.trim_end();
        if t == "}" {
            break;
        }
        let Some(head) = t.strip_prefix("    ") else {
            continue;
        };
        if head.starts_with(' ')
            || head.starts_with('/')
            || head.starts_with('#')
            || head.starts_with('}')
        {
            continue;
        }
        let word: String = head
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if word.is_empty() || !word.chars().next().unwrap_or(' ').is_uppercase() {
            continue;
        }
        let rest = &head[word.len()..];
        if rest.starts_with(" {") || rest.trim_start() == "," || rest.trim().is_empty() {
            out.push(word);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 掃描 `tests/*.rs` 回傳「有非 `#[ignore]` 考題引用到」的 variant 名集合
/// （與考卷燈 4 `variants_tested_by_exam_paper` 同算法）。
fn tested_variants(dir: &Path) -> BTreeSet<String> {
    let mut tested: BTreeSet<String> = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return tested;
    };
    let mut files: Vec<String> = entries
        .filter_map(|e| {
            e.ok()
                .and_then(|e| e.file_name().to_str().map(str::to_string))
        })
        .filter(|n| n.ends_with(".rs"))
        .collect();
    files.sort();
    for f in files {
        let Ok(text) = std::fs::read_to_string(dir.join(&f)) else {
            continue;
        };
        let mut in_ignored_fn = false;
        let mut pending_ignore = false;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with("#[ignore") {
                pending_ignore = true;
                continue;
            }
            if t.starts_with("#[test]") {
                if !pending_ignore {
                    in_ignored_fn = false;
                }
                continue;
            }
            if t.starts_with("fn ") || t.starts_with("pub fn ") {
                in_ignored_fn = pending_ignore;
                pending_ignore = false;
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
                if !in_ignored_fn {
                    tested.insert(word);
                }
            }
        }
    }
    tested
}

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let src_mutation = Path::new(&manifest).join("src").join("mutation.rs");
    let tests_dir = Path::new(&manifest).join("tests");
    println!("cargo:rerun-if-changed=src/mutation.rs");
    println!("cargo:rerun-if-changed=tests");
    println!("cargo:rerun-if-changed=build.rs");
    // env 變更必須重跑本腳本（考卷雙態的 env 由 test 期自讀，本腳本只讀
    // 檔案內容，故上列檔案即全部輸入）。

    let variants = mutation_variants(&src_mutation);
    let tested = tested_variants(&tests_dir);
    let census: Vec<String> = variants
        .iter()
        .map(|v| format!("{v}:{}", if tested.contains(v) { '1' } else { '0' }))
        .collect();
    println!("cargo:rustc-env=R1C_MUTATION_CENSUS={}", census.join("|"));
}
