//! quire-web —— wasm 殼：只做 wasm-bindgen 適配，零業務邏輯。
//! 渲染在 JS 端依顯示列表 JSON 執行（Canvas2D → 未來 WebGPU）。
//!
//! 兩個入口、同一個顯示列表契約（ODF-D2）：
//! - [`WasmEngine`]：DOCX 線（可編輯），經 `quire-engine`。
//! - [`WasmOdtView`]：ODT 線（唯讀渲染），經 `quire-odf` 的 `OdtView`。
//!   兩者 `page_display_list_json(page)` 吐出的 JSON **同 schema**（同一個
//!   `PageDisplayList` serde 契約），JS 端繪圖碼零改即可畫 ODT。
//!   路線定案（PM 11-08）：ODT 不走 `Engine` 邊——唯讀檔型塞 docx 中心的
//!   Engine ＝ 半契約兩極性（見 `docs/odf-d1-anatomy.md` §5）。

use quire_engine::Engine;
// 紅燈佔位態：以下兩型別由真實作 commit 啟用（拍紅 commit 先備好 import 面，
// 避免紅→綠 diff 混入無關行）。
#[allow(unused_imports)]
use quire_odf::{OdfBundle, OdtView};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmEngine {
    inner: Engine,
}

#[wasm_bindgen]
impl WasmEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<WasmEngine, JsError> {
        console_error_panic_hook::set_once();
        let inner = Engine::from_docx_bytes(bytes).map_err(|e| JsError::new(&format!("{e}")))?;
        Ok(Self { inner })
    }

    #[wasm_bindgen]
    pub fn new_with_paper(
        bytes: &[u8],
        page_w_pt: f32,
        page_h_pt: f32,
        margin_top_pt: Option<f32>,
        margin_bottom_pt: Option<f32>,
        margin_left_pt: Option<f32>,
        margin_right_pt: Option<f32>,
    ) -> Result<WasmEngine, JsError> {
        console_error_panic_hook::set_once();
        let spec = quire_engine::PaperSpec {
            page_w_pt,
            page_h_pt,
            margin_top_pt,
            margin_bottom_pt,
            margin_left_pt,
            margin_right_pt,
        };
        let inner = Engine::from_docx_bytes_with_paper(bytes, Some(&spec))
            .map_err(|e| JsError::new(&format!("{e}")))?;
        Ok(Self { inner })
    }

    pub fn paper_json(&self) -> String {
        self.inner.paper_json()
    }

    pub fn paper_override_applied(&self) -> bool {
        self.inner.paper_override_applied()
    }

    pub fn page_count(&self) -> usize {
        self.inner.page_count()
    }

    pub fn paragraph_count(&self) -> usize {
        self.inner.paragraph_count()
    }

    /// PageDisplayList 的 JSON（JS 端 JSON.parse 後畫到 canvas）。
    pub fn page_display_list_json(&self, page: usize) -> Option<String> {
        self.inner.page_display_list_json(page)
    }

    /// 回傳失效事件的 JSON 陣列。
    pub fn replace_paragraph_text(&mut self, para: usize, text: &str) -> Vec<String> {
        self.inner.replace_paragraph_text(para, text)
    }

    // ---------------- Web-1：IME 隱形 contenteditable 錨點契約 ----------------

    /// 段落純文字（JS 游標插入前讀目前段落）。
    pub fn paragraph_text(&self, para: usize) -> Option<String> {
        self.inner.paragraph_text(para)
    }

    /// caret (段落, 碼點) → CaretRect JSON（錨點定位＋caret 疊畫）。
    pub fn caret_rect_json(&self, para: usize, cp: usize) -> Option<String> {
        self.inner.caret_rect_json(para, cp)
    }

    /// canvas 點擊 (page, x_pt, y_pt) → CaretPos JSON。
    pub fn caret_at_json(&self, page: usize, x_pt: f32, y_pt: f32) -> Option<String> {
        self.inner.caret_at_json(page, x_pt, y_pt)
    }

    /// IME commit：在 caret 處插入文字，回傳失效事件 JSON 陣列。
    pub fn insert_text_at_caret(&mut self, para: usize, cp: usize, text: &str) -> Vec<String> {
        self.inner.insert_text_at_caret(para, cp, text)
    }

    // ---------------- Phase0-6：增量重排控制面 ----------------

    /// 預算制軟分頁開關（None＝全速；Some(n)＝同步最多排 n 頁，尾段 idle 續排）。
    pub fn set_sync_budget(&mut self, pages: Option<usize>) {
        self.inner.set_sync_budget(pages);
    }

    /// 版面是否已定案（無待排尾段）。
    pub fn is_settled(&self) -> bool {
        self.inner.is_settled()
    }

    /// 背景續排一步：{"settled":bool,"events":[...]} JSON（requestIdleCallback 迴圈）。
    pub fn reflow_step_json(&mut self) -> String {
        self.inner.reflow_step_json()
    }
}

// ============================ ODF-D2：ODT 入口 ============================
//
// 紅燈佔位態（拍紅用，見 `docs/odf-d2-redlight.md`）：`WasmOdtView` 以下為
// **佔位空殼**——`new` 一律 `Ok`、`page_count` 回 0、JSON 一律 `None`。考卷
// `shells/web/tests/odf_d2_wasm_view.rs` 因此**斷言拍紅**（非編譯紅），證明
// 七盞燈真的在檢查東西。真實作於下一個 commit 頂掉本節。

/// ODT 顯示列表視圖（wasm 殼）——佔位態，尚未實作。
#[wasm_bindgen]
pub struct WasmOdtView {
    _marker: (),
}

#[wasm_bindgen]
impl WasmOdtView {
    #[wasm_bindgen(constructor)]
    pub fn new(_bytes: &[u8]) -> Result<WasmOdtView, String> {
        Ok(Self { _marker: () })
    }

    pub fn page_count(&self) -> usize {
        0
    }

    pub fn page_display_list_json(&self, _page: usize) -> Option<String> {
        None
    }

    pub fn dump_json(&self) -> String {
        "[]".to_string()
    }

    pub fn paragraph_count(&self) -> usize {
        0
    }

    pub fn total_text_len(&self) -> usize {
        0
    }

    pub fn paper_json(&self) -> String {
        "{}".to_string()
    }

    pub fn paper_override_applied(&self) -> bool {
        false
    }

    pub fn is_settled(&self) -> bool {
        true
    }

    /// 讀取面：ODT 線無增量重排（開檔一次性分頁），恆回已定案。
    pub fn reflow_step_json(&self) -> String {
        "{\"settled\":true,\"events\":[]}".to_string()
    }
}
