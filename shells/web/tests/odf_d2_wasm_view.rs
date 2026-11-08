//! ODF-D2 紅燈考卷（wasm 殼面）：`WasmOdtView`——bytes 進、同 schema 顯示列表出。
//!
//! 卡號 t_89c4a196。本檔鎖的是**殼層**（`shells/web/src/lib.rs` 的
//! `#[wasm_bindgen]` 薄包），不重複 D1 對發射層的驗證（那在
//! `core/odf/tests/odf_d1_display_list.rs` 五盞＋`shells/native` CLI 面三盞）：
//!
//! - 燈 1：`page_count() >= 1`（不得靜默回 0 頁——ODF-E1 教訓）。
//! - 燈 2：`page_display_list_json(0)` 含 ≥1 個 Text 項，且欄位名與
//!   `fixtures/display_list_contract.json` **逐欄同名**（頁層欄位全等；Text
//!   必要欄位齊備＋不發基準沒有的欄位——口徑逐字照抄 D1 燈 1，基準檔由
//!   `scripts/odf_d1_contract_probe.py` 從 DOCX 線真實 `dump` 產物抽出，
//!   本卡零新增基準）。
//! - 燈 3：頁碼邊界——第 0 頁有、越界頁碼回 `None`，**不 panic**（wasm 邊
//!   panic＝瀏覽器 abort；`usize::MAX` 也在考面）。
//! - 燈 4（薄殼透傳鎖）：殼輸出與直接 `OdtView::from_odt_bytes`（同
//!   `page_geometry` params）**逐字相同**＋計數基準逐欄等值（D1 帳：13 段／
//!   402 碼點）＋`paper_json` 幾何＝`params()`——證明殼層零業務邏輯。
//! - 燈 5（老實報錯鎖）：加密檔→`Err` 且訊息含「加密」（`OdfError::Encrypted`
//!   語義照 `OdfBundle::open`，不得繞過、不得靜默）；垃圾位元組→`Err` 含
//!   「開啟失敗」；兩者都不得構造出可用視圖。
//! - 燈 6（JS 端零改鎖）：`playground.js` 讀取面逐項存在且形制正確——
//!   顯示列表項的 Canvas2D 消費欄位（x/y/size_pt/flags/color_argb/text 型別）、
//!   `paper_json` 七鍵、`is_settled()` 恆真、`reflow_step_json()` 収容形制
//!   `{"settled":true,"events":[]}`（JS 對 ODT 直接可畫的最小契約面）。
//! - 燈 7（DOCX 零回退鎖）：`WasmEngine` 對 `fixtures/sample.docx` 的
//!   `page_display_list_json(0)` 頁層/Text 欄位名仍與同一基準檔咬合——本卡
//!   動了 `shells/web/src/lib.rs`，DOCX 線行為必須逐位不變。
//!
//! 目標檔：`corpus-odt/ext/lo_core_sw_qa_odfexport_WordTest_edit.odt`。
//! （卡面寫的 `lo_core_sw_qa_odfimport_…` 在語料庫不存在；manifest 記的
//! LibreOffice 上游來源是 **odfexport** 目錄，D1 考卷用的就是這檔——以實檔為準，
//! 勘誤記帳見 `docs/odf-d2-redlight.md`。）
//!
//! 執行環境：host target（`cargo test --workspace` 的路徑）。`#[wasm_bindgen]`
//! 型別在 host 可直接構造（實測坐實），唯一紅線是 `JsError::new`（wasm-bindgen
//! import，host 呼叫即 panic）——故本殼錯誤型別用 `String`（JS 端 throw 語義
//! 不變：playground.js 的 `err.message || String(err)` 兩口徑都吃得下），
//! 加密／垃圾位元組的**錯誤路徑考卷**才能在 host 跑。
//!
//! 紅燈拍紅方式：本檔先入庫，`WasmOdtView` 以**佔位空殼**入庫（`new` 直回
//! `Ok`、`page_count` 回 0、JSON 回 `None`）——考卷斷言拍紅（非編譯紅），
//! 實測輸出存 `docs/odf-d2-redlight.md`，再上真實作轉綠。

use quire_odf::{OdfBundle, OdtView};
use quire_web::WasmOdtView;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// 目標檔（D1 燈 1 同檔；卡面檔名為勘誤，見檔頭註）。
const TARGET: &str = "ext/lo_core_sw_qa_odfexport_WordTest_edit.odt";
/// 加密語料檔（D1 附加鎖同檔：manifest 宣告條目加密，消費層必須落 Err）。
const ENCRYPTED: &str = "ext/lo_core_sw_qa_odfexport_sha1_correct.odt";
/// D1 帳（`core/odf/tests/odf_d1_display_list.rs`）：目標檔段落／文字基準。
const TARGET_PARAGRAPHS: usize = 13;
const TARGET_TEXT_LEN: usize = 402;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").to_path_buf()
}

fn corpus(rel: &str) -> Vec<u8> {
    let p = repo_root().join("corpus-odt").join(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取語料失敗 {rel}: {e}"))
}

fn contract() -> Value {
    let p = repo_root().join("fixtures/display_list_contract.json");
    let s = std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("讀取契約基準檔失敗 {}: {e}", p.display()));
    serde_json::from_str(&s).expect("契約基準檔應為合法 JSON")
}

/// 基準檔某欄位（字串陣列）→ 排序字串清單（口徑照 D1 考卷 `str_list`）。
fn str_list(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = v
        .as_array()
        .expect("欄位應為陣列")
        .iter()
        .map(|s| s.as_str().expect("元素應為字串").to_string())
        .collect();
    out.sort();
    out
}

fn sorted_keys(obj: &Value) -> Vec<String> {
    let mut k: Vec<String> = obj
        .as_object()
        .expect("應為物件")
        .keys()
        .cloned()
        .collect();
    k.sort();
    k
}

/// 單頁 JSON →（頁層欄位名, Text 項欄位名聯集, Text 項數）。
fn page_shape(dl: &Value) -> (Vec<String>, Vec<String>, usize) {
    let top = sorted_keys(dl);
    let mut texts: Vec<String> = dl["items"]
        .as_array()
        .expect("items 欄位應為陣列")
        .iter()
        .filter_map(|it| it.get("Text"))
        .flat_map(|t| t.as_object().expect("Text 應為物件").keys().cloned())
        .collect();
    let n = dl["items"]
        .as_array()
        .map(|a| a.iter().filter(|it| it.get("Text").is_some()).count())
        .unwrap_or(0);
    texts.sort();
    texts.dedup();
    (top, texts, n)
}

// ---------------------------------------------------------------------------
// 燈 1：bytes 進 → page_count() >= 1（不得靜默 0 頁）
// ---------------------------------------------------------------------------

#[test]
fn lamp1_open_bytes_yields_at_least_one_page() {
    let view = WasmOdtView::new(&corpus(TARGET)).expect("目標檔必須開檔成功");
    assert!(
        view.page_count() >= 1,
        "目標檔經 wasm 殼至少要 1 頁，實際 {}（靜默 0 頁＝ODF-E1 教訓禁止形態）",
        view.page_count()
    );
}

// ---------------------------------------------------------------------------
// 燈 2：第 0 頁 ≥1 Text 項＋欄位名與基準檔逐欄同名
// ---------------------------------------------------------------------------

#[test]
fn lamp2_page0_text_items_match_docx_contract_field_names() {
    let view = WasmOdtView::new(&corpus(TARGET)).expect("目標檔必須開檔成功");
    let json = view
        .page_display_list_json(0)
        .expect("第 0 頁顯示列表 JSON 應存在");
    let dl: Value = serde_json::from_str(&json).expect("輸出必須是合法 JSON（不准字串拼接）");
    let c = contract();
    let (top, text_keys, n_text) = page_shape(&dl);

    // 頁層欄位名逐欄全等（不多不少）。
    assert_eq!(
        top,
        str_list(&c["top_level_keys"]),
        "頁層欄位名必須與 DOCX 線契約逐欄同名（基準檔由腳本產出，不得手改）"
    );
    // 文字項存在性（防斷言空轉）。
    assert!(
        n_text >= 1,
        "第 0 頁必須發射 ≥1 個 Text 項，實際 items={}",
        dl["items"].as_array().map(|a| a.len()).unwrap_or(0)
    );
    // Text 欄位：必要欄位齊備（交集口徑）＋不發基準沒有的欄位（聯集口徑）
    // ——與 D1 燈 1 同款雙向鎖；ODT 切片 family 跳欄（serde skip）故落必要欄位。
    let required = str_list(&c["variant_required_keys"]["Text"]);
    let union = str_list(&c["variant_keys"]["Text"]);
    for k in &required {
        assert!(text_keys.contains(k), "Text 必要欄位 {k} 缺失（與 DOCX 線脫節）");
    }
    for k in &text_keys {
        assert!(union.contains(k), "Text 發了契約沒有的欄位 {k:?}（欄位名漂移）");
    }
}

// ---------------------------------------------------------------------------
// 燈 3：頁碼邊界——存在頁 Some、越界頁 None，全程不 panic
// ---------------------------------------------------------------------------

#[test]
fn lamp3_page_bounds_return_option_never_panic() {
    let view = WasmOdtView::new(&corpus(TARGET)).expect("目標檔必須開檔成功");
    let n = view.page_count();
    assert!(n >= 1, "前提：目標檔至少 1 頁");
    assert!(
        view.page_display_list_json(0).is_some(),
        "第 0 頁必須存在（燈 1 已證有頁）"
    );
    // 越界三連發：n、n+999、usize::MAX——全部必須 None 且函數返回（不 panic＝
    // 本斷言能執行完就是證據；wasm 邊 panic＝瀏覽器 abort）。
    assert!(view.page_display_list_json(n).is_none(), "頁數边界外必須 None");
    assert!(view.page_display_list_json(n + 999).is_none(), "遠端越界必須 None");
    assert!(
        view.page_display_list_json(usize::MAX).is_none(),
        "usize::MAX 必須 None（不得運算溢位 panic）"
    );
}

// ---------------------------------------------------------------------------
// 燈 4：薄殼透傳鎖——殼輸出＝直接 OdtView 逐字相同；計數走 D1 帳
// ---------------------------------------------------------------------------

#[test]
fn lamp4_shell_is_a_thin_passthrough_of_odtview() {
    let bytes = corpus(TARGET);
    let view = WasmOdtView::new(&bytes).expect("目標檔必須開檔成功");

    // 直接面（不经殼）：同 params 路線（page_geometry 鏈，odt-dump 同口徑）。
    let bundle = OdfBundle::open(&bytes).expect("直接面開檔失敗");
    let params = quire_odf::page_geometry(&bundle)
        .expect("直接面頁面幾何")
        .to_layout_params();
    let direct = OdtView::from_bundle(&bundle, &params).expect("直接面發射失敗");

    assert_eq!(
        direct.page_count(),
        view.page_count(),
        "殼與直接面頁數必須一致"
    );
    for i in 0..view.page_count() {
        assert_eq!(
            direct.page_display_list_json(i),
            view.page_display_list_json(i),
            "第 {i} 頁殼輸出與直接面必須逐字相同（殼零業務邏輯）"
        );
    }
    assert_eq!(
        view.dump_json(),
        direct.dump_json(),
        "全頁 dump 殼與直接面必須逐字相同"
    );
    // D1 帳逐欄等值（計數口徑不得被殼層動手腳）。
    assert_eq!(
        view.paragraph_count(),
        TARGET_PARAGRAPHS,
        "段落數必須維持 D1 帳 {TARGET_PARAGRAPHS}"
    );
    assert_eq!(
        view.total_text_len(),
        TARGET_TEXT_LEN,
        "文字總碼點必須維持 D1 帳 {TARGET_TEXT_LEN}"
    );
    // paper_json 幾何＝params()（JS 端狀態列讀的就是這七鍵）。
    let pj: Value = serde_json::from_str(&view.paper_json()).expect("paper_json 必須合法 JSON");
    assert_eq!(
        pj["page_w_pt"].as_f64(),
        Some(f64::from(direct.params().page_w_pt)),
        "paper_json 頁寬必須取自實際生效 params"
    );
    assert_eq!(
        pj["page_h_pt"].as_f64(),
        Some(f64::from(direct.params().page_h_pt)),
        "paper_json 頁高必須取自實際生效 params"
    );
    assert!(
        !view.paper_override_applied(),
        "ODT 線恆 false（紙張覆寫是 DOCX 線概念，本殼不假造）"
    );
}

// ---------------------------------------------------------------------------
// 燈 5：老實報錯鎖——加密檔明確報錯、垃圾位元組明確報錯，都不得靜默
// ---------------------------------------------------------------------------

#[test]
fn lamp5_encrypted_and_corrupt_fail_loudly_not_silently() {
    // 加密檔：`Err` 訊息必須帶「加密」語義（照 OdfError::Encrypted 顯示文案，
    // 含條目路徑與演算法名——不得被新入口繞過、不得降格成 0 頁空視圖）。
    match WasmOdtView::new(&corpus(ENCRYPTED)) {
        Ok(v) => panic!(
            "加密檔必須報錯，竟構造成功（頁數 {}）——靜默空視圖＝ODF-E1 教訓禁止形態",
            v.page_count()
        ),
        Err(e) => {
            assert!(e.contains("加密"), "加密檔錯誤訊息必須含「加密」，實際 {e:?}");
            assert!(!e.is_empty(), "錯誤訊息不得為空");
        }
    }
    // 垃圾位元組：必須落容器層錯誤口徑（「開啟失敗」前綴），不得 Ok。
    match WasmOdtView::new(&[0u8, 1, 2, 3, 4, 5]) {
        Ok(v) => panic!("垃圾位元組必須報錯，竟構造成功（頁數 {}）", v.page_count()),
        Err(e) => assert!(
            e.contains("開啟失敗"),
            "垃圾位元組必須落「開啟失敗」口徑，實際 {e:?}"
        ),
    }
    // 空位元組：同樣必須報錯（zip 層無從解析）。
    assert!(WasmOdtView::new(&[]).is_err(), "空位元組必須報錯");
}

// ---------------------------------------------------------------------------
// 燈 6：JS 端零改鎖——playground.js 讀取面逐項存在且形制正確
// ---------------------------------------------------------------------------

#[test]
fn lamp6_playground_read_surface_is_complete() {
    let view = WasmOdtView::new(&corpus(TARGET)).expect("目標檔必須開檔成功");
    // playground.js render() 的消費面（quake-www public/playground/playground.js）：
    //   page_count / page_display_list_json / paragraph_count / paper_json /
    //   is_settled / reflow_step_json。六項都在，且形制 JS 直接可用。
    assert!(view.page_count() >= 1);
    assert!(view.paragraph_count() >= 1);
    assert!(view.is_settled(), "開檔一次性分頁：必須恆已定案（否則 JS 無限空轉 idle 迴圈）");
    let reflow: Value =
        serde_json::from_str(&view.reflow_step_json()).expect("reflow_step_json 必須合法 JSON");
    assert_eq!(reflow["settled"].as_bool(), Some(true));
    assert!(reflow["events"].as_array().is_some_and(|a| a.is_empty()));

    // paper_json 七鍵（playground.js updatePaperStatus/updatePaperUrl 讀取面）。
    let pj: Value = serde_json::from_str(&view.paper_json()).expect("paper_json 必須合法 JSON");
    for k in [
        "page_w_pt",
        "page_h_pt",
        "margin_top_pt",
        "margin_bottom_pt",
        "margin_left_pt",
        "margin_right_pt",
        "paper_override_applied",
    ] {
        assert!(!pj[k].is_null(), "paper_json 缺鍵 {k}（playground 狀態列會空白）");
    }
    assert_eq!(pj["paper_override_applied"].as_bool(), Some(false));

    // Canvas2D 消費欄位的**型別**形制（main.js/playground.js 直接算術與字串操作）。
    let dl: Value = serde_json::from_str(
        &view.page_display_list_json(0).expect("第 0 頁必須存在"),
    )
    .expect("頁 JSON 合法");
    assert!(dl["width_pt"].as_f64().is_some_and(|w| w > 0.0), "width_pt 必須正數（JS 算 scale）");
    assert!(dl["height_pt"].as_f64().is_some_and(|h| h > 0.0), "height_pt 必須正數");
    for it in dl["items"].as_array().expect("items 陣列") {
        let t = it.get("Text").expect("D1 切片只准發 Text 變體");
        for k in ["x", "y", "size_pt"] {
            assert!(
                t[k].as_f64().is_some_and(f64::is_finite),
                "Text.{k} 必須有限數（JS 直接乘 scale）"
            );
        }
        assert!(t["flags"].is_number(), "Text.flags 必須整數（JS 位元運算）");
        assert!(t["color_argb"].is_number(), "Text.color_argb 必須整數（JS >>> 運算）");
        assert!(t["text"].is_string(), "Text.text 必須字串（JS fillText）");
    }
}

// ---------------------------------------------------------------------------
// 燈 7：DOCX 零回退鎖——同檔 lib.rs 動過，WasmEngine 輸出的欄位契約咬合不動
// ---------------------------------------------------------------------------

#[test]
fn lamp7_docx_side_contract_unchanged() {
    let bytes = std::fs::read(repo_root().join("fixtures/sample.docx"))
        .expect("讀取 fixtures/sample.docx 失敗");
    let engine = quire_web::WasmEngine::new(&bytes).expect("DOCX 樣本必須開檔成功");
    assert!(engine.page_count() >= 1, "DOCX 線頁數不得回退");
    let json = engine
        .page_display_list_json(0)
        .expect("DOCX 第 0 頁顯示列表必須存在");
    let dl: Value = serde_json::from_str(&json).expect("DOCX 頁 JSON 合法");
    let c = contract();
    let (top, text_keys, n_text) = page_shape(&dl);
    assert_eq!(top, str_list(&c["top_level_keys"]), "DOCX 頁層欄位名回退");
    assert!(n_text >= 1, "DOCX Text 項不得消失");
    for k in str_list(&c["variant_required_keys"]["Text"]) {
        assert!(text_keys.contains(&k), "DOCX Text 必要欄位 {k} 缺失");
    }
}
