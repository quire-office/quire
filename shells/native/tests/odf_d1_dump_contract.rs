//! ODF-D1 契約互鎖考卷：**DOCX 側反查** `fixtures/display_list_contract.json`。
//!
//! 為何需要這一側：本卡燈 1（`core/odf/tests/odf_d1_display_list.rs`）拿基準檔
//! 驗 ODT 發射欄位名。若基準檔本身是手寫／過期的，ODT 線「對照基準綠」等於空轉
//! ——兩線各烤一套欄位的破口只是換了地方。故本檔從 DOCX 線**真實 `dump`/`page`
//! 產物**逐欄反查基準檔：
//! 1. 基準檔記的來源樣本檔必須都在（缺檔＝基準來源斷線）。
//! 2. `dump` 實際出現過的欄位名（聯集＋交集）必須與基準檔的 `variant_keys`／
//!    `variant_required_keys` **完全相等**（基準不得多欄、不得少欄）。
//! 3. 頁層欄位名必須與 `top_level_keys` 完全相等。
//! 4. `page`（單頁形）與 `dump`（全頁形）必須同形——`odt-dump` 對齊的就是這個
//!    形制，兩形不同形即咬紅。
//!
//! 基準檔由 `scripts/odf_d1_contract_probe.py` 產出（唯一合法產生路線）；本檔走
//! 的是同一批樣本的另一條路（CLI 直出），兩條路都斷＝基準檔腐化無從藏身。

use serde_json::Value;
use std::path::{Path, PathBuf};

/// 基準檔與樣本檔一律**相對倉庫根**（`repo_root()` 已指向倉庫根，不得再跳兩層）。
const FIXTURE: &str = "fixtures/display_list_contract.json";
/// `page`/`dump` 同形驗證與頁層欄位驗證用的樣本（單頁即含 Text／表格框線項）。
const SAMPLE: &str = "fixtures/sample.docx";
const IMAGE_SAMPLE: &str = "fixtures/inline_image.docx";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .to_path_buf()
}

fn cli_json(args: &[&str]) -> Value {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_quire-cli"))
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("執行 quire-cli");
    assert!(
        out.status.success(),
        "quire-cli {args:?} 必須成功退出，stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "quire-cli {args:?} stdout 必須是合法 JSON（{e}）：{:?}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

fn fixture() -> Value {
    let p = repo_root().join(FIXTURE);
    let s = std::fs::read_to_string(&p).unwrap_or_else(|e| {
        panic!(
            "基準檔缺失 {}: {e}（跑 scripts/odf_d1_contract_probe.py 產出）",
            p.display()
        )
    });
    serde_json::from_str(&s).expect("基準檔應為合法 JSON")
}

/// 基準檔某欄位（字串陣列）→ 排序字串清單。
fn want_list(c: &Value, path: &[&str]) -> Vec<String> {
    let mut node = c;
    for key in path {
        node = node
            .get(*key)
            .unwrap_or_else(|| panic!("基準檔缺欄位 {path:?}"));
    }
    node.as_array()
        .unwrap_or_else(|| panic!("基準檔 {path:?} 應為陣列"))
        .iter()
        .map(|v| v.as_str().expect("元素應為字串").to_string())
        .collect()
}

fn sorted_keys(obj: &Value) -> Vec<String> {
    let mut k: Vec<String> = obj
        .as_object()
        .expect("項應為物件")
        .keys()
        .cloned()
        .collect();
    k.sort();
    k
}

/// `dump` 的頁層欄位名（各頁必須一致）＋頁數。
fn dump_top_keys(path: &str) -> (Vec<String>, usize) {
    let all = cli_json(&["dump", path]);
    let pages = all.as_array().expect("dump 必須是頁陣列");
    assert!(!pages.is_empty(), "{path}：dump 不得為空陣列");
    let top = sorted_keys(&pages[0]);
    for pg in pages {
        assert_eq!(sorted_keys(pg), top, "{path}：各頁頁層欄位名必須一致");
    }
    (top, pages.len())
}

/// 逐檔 `dump` 的某變體欄位名：（聯集, 交集, 命中項數, 命中檔數）。
/// 聯集＝基準 `variant_keys` 口徑；交集＝`variant_required_keys` 口徑（**含該變體
/// 的每個樣本檔**都發的欄位＝必要欄位）。無該變體的樣本檔跳過——與探針腳本同式
/// （腳本只在變體出現時才記聯集/交集）。`family` 只在文件宣告字型族時才發（serde
/// `skip_serializing_if`），故落聯集不落交集。
fn variant_keys_of(paths: &[String], variant: &str) -> (Vec<String>, Vec<String>, usize, usize) {
    let mut union: Vec<String> = Vec::new();
    let mut inter: Option<Vec<String>> = None;
    let mut hits = 0usize;
    let mut files = 0usize;
    for rel in paths {
        let all = cli_json(&["dump", rel]);
        let mut here: Vec<String> = Vec::new();
        for pg in all.as_array().expect("頁陣列") {
            for it in pg["items"].as_array().expect("items 陣列") {
                if let Some(fields) = it.get(variant) {
                    here.extend(sorted_keys(fields));
                    hits += 1;
                }
            }
        }
        if here.is_empty() {
            continue; // 本樣本無該變體（與探針同式：不計入聯集/交集）
        }
        here.sort();
        here.dedup();
        files += 1;
        union.extend(here.iter().cloned());
        inter = Some(match inter {
            None => here,
            Some(prev) => prev.into_iter().filter(|k| here.contains(k)).collect(),
        });
    }
    union.sort();
    union.dedup();
    (union, inter.unwrap_or_default(), hits, files)
}

// ---------------------------------------------------------------------------

/// 燈 a：DOCX 線真實 `dump` 的 Text 欄位名（聯集＋交集）與頁層欄位名必須與
/// 基準檔**逐欄全等**——基準檔一旦與出貨產物脫節，本燈立刻咬紅。
#[test]
fn docx_dump_keys_match_fixture_exactly() {
    let c = fixture();
    // 來源帳：基準檔記的樣本檔必須都在（斷線即紅）。
    let srcs: Vec<String> = c["source_samples"]
        .as_array()
        .expect("source_samples 應為陣列")
        .iter()
        .map(|v| v.as_str().expect("樣本路徑應為字串").to_string())
        .collect();
    assert!(!srcs.is_empty(), "基準檔必須記錄來源樣本檔");
    for s in &srcs {
        assert!(
            repo_root().join(s).exists(),
            "基準檔來源樣本 {s} 不存在＝基準來源斷線"
        );
    }

    let (top, pages) = dump_top_keys(SAMPLE);
    assert_eq!(
        top,
        want_list(&c, &["top_level_keys"]),
        "DOCX 線真實 dump 的頁層欄位名與基準檔不符（基準檔必須由腳本重產，不得手改）"
    );

    let (union, inter, hits, files) = variant_keys_of(&srcs, "Text");
    assert_eq!(
        union,
        want_list(&c, &["variant_keys", "Text"]),
        "DOCX 線真實 Text 項欄位名**聯集**與基準檔不符"
    );
    assert_eq!(
        inter,
        want_list(&c, &["variant_required_keys", "Text"]),
        "DOCX 線真實 Text 項欄位名**交集**（必要欄位）與基準檔不符"
    );
    assert_eq!(
        files,
        srcs.len(),
        "每個基準來源樣本都必須含 Text 項（否則聯集口徑與腳本脫節）"
    );
    assert!(
        hits > 0 && pages >= 1,
        "基準驗證面不得空轉（hits={hits} pages={pages}）"
    );
}

/// 燈 b：ODT 切片**不發**的變體（Image／Rect／Rule），欄位名同樣必須與基準檔
/// 全等——防「ODT 只發 Text，於是沒人守另外三型的欄位名」。口徑與燈 a 相同
/// （全樣本聯集＋交集），逐變體跑遍基準檔記的所有變體。
#[test]
fn other_variants_match_fixture() {
    let c = fixture();
    let srcs: Vec<String> = c["source_samples"]
        .as_array()
        .expect("source_samples 應為陣列")
        .iter()
        .map(|v| v.as_str().expect("樣本路徑應為字串").to_string())
        .collect();
    let obj = c["variant_keys"]
        .as_object()
        .expect("variant_keys 應為物件");
    assert!(
        obj.len() >= 4,
        "基準檔應涵蓋四變體，實際 {:?}",
        obj.keys().collect::<Vec<_>>()
    );
    for variant in obj.keys() {
        let (union, inter, hits, _files) = variant_keys_of(&srcs, variant);
        assert_eq!(
            union,
            want_list(&c, &["variant_keys", variant]),
            "{variant} 變體欄位名**聯集**與基準檔不符"
        );
        assert_eq!(
            inter,
            want_list(&c, &["variant_required_keys", variant]),
            "{variant} 變體欄位名**交集**（必要欄位）與基準檔不符"
        );
        assert!(hits > 0, "{variant} 驗證面不得空轉");
    }
    // Image 另補一條：必須真的由 inline_image.docx 帶出來（該變體最容易被樣本退化掉）。
    let (union, _i, hits, files) = variant_keys_of(&[IMAGE_SAMPLE.to_string()], "Image");
    assert!(
        !union.is_empty() && hits > 0 && files == 1,
        "Image 樣本面退化"
    );
}

/// 燈 c：`page`（單頁形）與 `dump`（全頁形）必須同形——`odt-dump` 對齊的就是
/// 這個形制，兩形不一致時「ODT 與 DOCX 同形」會各說各話。
#[test]
fn page_and_dump_subcommands_emit_the_same_shape() {
    let one = cli_json(&["page", SAMPLE, "0"]);
    let all = cli_json(&["dump", SAMPLE]);
    let first = &all.as_array().expect("dump 頁陣列")[0];
    assert_eq!(
        sorted_keys(&one),
        sorted_keys(first),
        "單頁形與全頁形的頁層欄位不一致"
    );
    let one_items = one["items"].as_array().expect("items 陣列");
    let first_items = first["items"].as_array().expect("items 陣列");
    assert_eq!(one_items.len(), first_items.len(), "兩形項數必須相同");
    for (a, b) in one_items.iter().zip(first_items.iter()) {
        assert_eq!(
            sorted_keys(a),
            sorted_keys(b),
            "單頁形與全頁形的項欄位不一致"
        );
    }
    assert_eq!(
        one.to_string(),
        first.to_string(),
        "同頁兩形產物必須逐字相同"
    );
}
