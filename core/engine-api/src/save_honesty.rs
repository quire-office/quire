//! R1-c：誠實存檔出口＋無損證據報告＋能力清單單源（先紅後綠的實作面）。
//!
//! 三件事，全部資料來源＝引擎既有事實（卡面鐵律：嚴禁殼層／前端自己猜或
//! 數 ZIP entry 名稱充數）：
//!
//! 1. [`Engine::save_docx_checked_json`]：誠實出口。存檔失敗必須回報
//!    `ok:false` ＋ 錯誤訊息 ＋ 錯誤種類（[`engine_error_kind`]，列舉級派生，
//!    不壓扁成字串常數）。**任何情況下都不得空 bytes 配 `ok:true`**——
//!    缺陷本體（`shells/web` 的 `unwrap_or_default()` 吞錯）的正面防線。
//! 2. [`Engine::save_docx_report_json`]：無損證據。`touched_parts` 取自
//!    引擎 touched 白名單、`entry_total` 取自 bundle 實數、`roundtrip` 取自
//!    [`Engine::roundtrip_check`]（`core/ooxml/src/diff.rs` 的結構級對照）。
//!    純觀察：全程 `&self`，不觸碰任何可變狀態（紅燈 3 鎖此事）。
//! 3. [`surface`]／[`Engine::supported_mutations_json`]：能力清單。
//!    單源＝`src/mutation.rs` 列舉 ＋ `tests/` 考卷 census，**build 期機械生成**
//!    （`build.rs`，比照 U4f 的 `R1C_*` env 注入口徑），零手寫清單——
//!    `proven=true` iff 該 variant 在 `core/engine-api/tests/` 有至少一條
//!    **非 `#[ignore]`** 考題引用（紅燈 4 機械對照，人眼退出迴路）。
//!
//! ## 故障注入（考卷雙態用，比照倉內 `*_BUGGY` 口徑）
//! - `R1C_SAVE_FAULT=1`：[`Engine::save_docx`] 入口誠實失敗（真 `EngineError`，
//!   非 panic、非空 Vec）。注入點在 `Engine::save_docx()`（卡面禁區尊重：
//!   `OoxmlDoc::save()` 一個位元組都不碰——注入在呼叫它之前）。
//! - `R1C_SAVE_BUGGY=1`：僅 [`Engine::save_docx_checked_json`] 重現修前的
//!   `unwrap_or_default()` 吞錯態（失敗被吞成 `ok:true`＋0 bytes）。它是
//!   「鑑別力證據」路徑——紅燈 1 臂 B 在 BUGGY 下必須紅，正常全跑永不觸發。
//!
//! 命名紀律：本檔公開識別符避開全庫 `disallowed_names` 地雷（「mutate」子串
//! 連環命中 identifier，E2D 先例＝`mut_*` 限定路徑）——函數名取 `surface`，
//! 型別取 `SurfaceItem`，呼叫端以 `save_honesty::surface()` 路徑使用。

use serde::Serialize;
use serde_json::json;

use crate::Engine;

// ================================================================ 錯誤種類

/// 錯誤種類（單源＝`EngineError` 列舉本身，列舉加變體編譯器強迫這裡同步；
/// 壓扁成字串常數＝紅燈 1 的 `!= "unknown"` 斷言把關）。
pub fn engine_error_kind(err: &crate::EngineError) -> &'static str {
    match err {
        crate::EngineError::Ooxml(_) => "ooxml",
        crate::EngineError::Diff(_) => "diff",
        crate::EngineError::Xlsx(_) => "xlsx",
    }
}

// ================================================================ 故障注入

/// 注入點：`Engine::save_docx()` 開頭呼叫。`R1C_SAVE_FAULT` 設定＝誠實失敗。
/// 按呼叫當下讀 env（比照 `SAVE_RT_BUGGY`／`R1_REDO_BUGGY` 口徑）。
pub(crate) fn fault_gate() -> Result<(), crate::EngineError> {
    if std::env::var_os("R1C_SAVE_FAULT").is_some() {
        return Err(crate::EngineError::Ooxml(quire_ooxml::OoxmlError::Xml(
            "R1C_SAVE_FAULT：故障注入——存檔必須誠實失敗（R1-c 紅燈 1）".to_string(),
        )));
    }
    Ok(())
}

/// `R1C_SAVE_BUGGY`＝修前吞錯態重現開關（僅 checked 出口讀取；鑑別力證據）。
fn buggy_swallow_enabled() -> bool {
    std::env::var_os("R1C_SAVE_BUGGY").is_some()
}

// ================================================================ 誠實出口

/// [`Engine::save_docx_checked_json`] 的結構（serde 單源，杜絕手拼 JSON 漂移）。
/// 失敗時 `bytes_len` 序列化為 `null`（不得偽報長度），`error`／`error_kind`
/// 必非空。BUGGY 吞錯態由實作點直接交出 `ok:true`＋`bytes_len:Some(0)`。
#[derive(Serialize)]
struct CheckedSave<'a> {
    ok: bool,
    bytes_len: Option<u64>,
    error: Option<&'a str>,
    error_kind: Option<&'a str>,
}

fn checked_json(v: &CheckedSave<'_>) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| {
        "{\"ok\":false,\"bytes_len\":null,\"error\":\"序列化失敗\",\"error_kind\":\"serialise\"}"
            .to_string()
    })
}

// ================================================================ Engine 出口

impl Engine {
    /// 誠實存檔出口（紅燈 1）：`{ok, bytes_len, error?, error_kind?}`。
    ///
    /// 失敗路徑鐵律：`ok:false` 時 `bytes_len` 必須為 `null`——「空 bytes 配
    /// ok:true」就是用戶下載到 0 字節檔案 yet 毫無知覺的缺陷本體。
    /// `R1C_SAVE_BUGGY=1` 重現修前 `unwrap_or_default()`：失敗被吞成
    /// `ok:true`＋0 bytes（紅燈 1 臂 B 靠它在回塞態翻紅）。
    pub fn save_docx_checked_json(&self) -> String {
        match self.save_docx() {
            Ok(bytes) => checked_json(&CheckedSave {
                ok: true,
                bytes_len: Some(bytes.len() as u64),
                error: None,
                error_kind: None,
            }),
            Err(e) => {
                if buggy_swallow_enabled() {
                    // 修前態逐字重現（鑑別力證據，正常路徑永不抵達）：
                    // unwrap_or_default() 把 Err 吞成空 Vec，用戶拿到空檔且
                    // 完全不知道發生了什麼。
                    return checked_json(&CheckedSave {
                        ok: true,
                        bytes_len: Some(0),
                        error: None,
                        error_kind: None,
                    });
                }
                let msg = e.to_string();
                let kind = engine_error_kind(&e);
                checked_json(&CheckedSave {
                    ok: false,
                    bytes_len: None,
                    error: Some(msg.as_str()),
                    error_kind: Some(kind),
                })
            }
        }
    }

    /// 無損證據出口（紅燈 2／3）：本次存檔「重寫了哪些 part、原封不動搬運了
    /// 多少、roundtrip 是否零失真」。全部欄位取自引擎既有事實：
    /// - `touched_parts`：引擎 touched 白名單（`set_part`／編輯標記，
    ///   內容復原即自動退場——故它是「真正被動過」的事實，不是樂觀宣告）。
    /// - `entry_total`：原始 bundle 的一般檔案 entry 實數（`file_entries()`，
    ///   與 `ZipBundle::file_entries()` 同口徑排除目錄 entry）。
    /// - `preserved_part_count`：原位元組搬運的 entry 數＝總數減「白名單內
    ///   內容真的變了」的數（touched 但內容復原者照樣原位搬運，計入 preserved）。
    /// - `roundtrip`：[`Engine::roundtrip_check`] 的結構級對照結果。
    ///
    /// 純觀察（紅燈 3）：`&self`＋呼叫鏈零可變狀態——先 report 再 save 與只
    /// save 的輸出逐位元組相等。故障注入下與 checked 出口同源誠實 `ok:false`
    /// ＋同款 error 字串（兩出口錯誤須同源，紅燈 1 臂 A 斷言）。
    pub fn save_docx_report_json(&self) -> String {
        let saved = match self.save_docx() {
            Ok(b) => b,
            Err(e) => {
                let msg = e.to_string();
                return json!({
                    "ok": false,
                    "bytes_len": null,
                    "error": msg,
                    "error_kind": engine_error_kind(&e),
                    "touched_parts": self.touched.keys().collect::<Vec<_>>(),
                    "entry_total": self.doc.bundle.file_entries().count(),
                    "preserved_part_count": null,
                    "roundtrip": null,
                })
                .to_string();
            }
        };
        let diff = match self.roundtrip_check() {
            Ok(d) => d,
            Err(e) => {
                // 對照本身失敗＝證據不完整，誠實 ok:false（絕不交假憑證）。
                let msg = e.to_string();
                return json!({
                    "ok": false,
                    "bytes_len": saved.len(),
                    "error": msg,
                    "error_kind": engine_error_kind(&e),
                    "touched_parts": self.touched.keys().collect::<Vec<_>>(),
                    "entry_total": self.doc.bundle.file_entries().count(),
                    "preserved_part_count": null,
                    "roundtrip": null,
                })
                .to_string();
            }
        };
        let entry_total = self.doc.bundle.file_entries().count();
        // 「重寫」以 roundtrip 實測為準（whitelisted_changed）：touched 但內容
        // 還原回基線的 entry 在 save() 層照原壓縮位元組搬運，不該吹噓成重寫，
        // 也不該從 preserved 漏計。
        let preserved = entry_total.saturating_sub(diff.whitelisted_changed.len());
        json!({
            "ok": true,
            "bytes_len": saved.len(),
            "touched_parts": self.touched.keys().collect::<Vec<_>>(),
            "entry_total": entry_total,
            "preserved_part_count": preserved,
            "roundtrip": {
                "pass": diff.pass,
                "byte_identical": diff.byte_identical,
                "whitelisted_changed": diff.whitelisted_changed,
                "violations": diff.violations,
                "missing": diff.missing,
                "added": diff.added,
            },
        })
        .to_string()
    }

    /// 能力清單 JSON（wasm 出口同名透傳）：只列**有非 ignore 考題實證**的
    /// 編輯操作（`proven=true` 過濾）。紅燈 4 斷言本清單與考卷 census 雙向
    /// 一致——清單既不得吹噓未驗證能力，也不得藏已驗證能力。
    pub fn supported_mutations_json() -> String {
        let items: Vec<serde_json::Value> = surface()
            .into_iter()
            .filter(|m| m.proven)
            .map(|m| json!({ "variant": m.variant }))
            .collect();
        json!(items).to_string()
    }
}

// ================================================================ 能力清單單源

/// 一個 variant 的能力目錄條目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceItem {
    /// variant 名（單源＝`pub enum Mutation` 列舉體，build 期掃描）。
    pub variant: &'static str,
    /// 有非 `#[ignore]` 考題實證＝true；只有 ignore 考題或查無考題＝false。
    pub proven: bool,
}

/// build 期由 `build.rs` 掃描注入（`R1C_MUTATION_CENSUS`，格式
/// `Variant:0|Variant:1|...`，字典序）。列舉漂移或考卷變動 ⇒ build.rs
/// 重跑（`cargo:rerun-if-changed` 盯 `src/mutation.rs` 與 `tests/`）。
const CENSUS_RAW: &str = env!("R1C_MUTATION_CENSUS");

/// 能力目錄（全部 variant ＋ proven 旗標）。
///
/// **單源紀律**（紅燈 4 機械對照把關）：本清單由 `build.rs` 從
/// `src/mutation.rs` 的 `pub enum Mutation` 掃描生成，非手寫；`proven` 由同
/// build 期對 `tests/` 的考卷 census 決定（演算法＝紅燈 4 運行期掃描的鏡像，
/// 兩側逐字一致由紅燈 4 本身把關）。列舉分派（`Mutation::apply` 臂）與本
/// 清單無運行期耦合：分派是能力，考題通過才是「已驗證」——兩者的差集正是
/// 後續逐鈕真相卡（R1-d）的輸入。
pub fn surface() -> Vec<SurfaceItem> {
    CENSUS_RAW
        .split('|')
        .filter(|s| !s.is_empty())
        .map(|pair| {
            let (name, flag) = pair.split_once(':').expect("R1C_MUTATION_CENSUS 格式違規");
            SurfaceItem {
                variant: name,
                proven: flag == "1",
            }
        })
        .collect()
}
