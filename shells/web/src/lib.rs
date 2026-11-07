//! quire-web —— wasm 殼：只做 wasm-bindgen 適配，零業務邏輯。
//! 渲染在 JS 端依顯示列表 JSON 執行（Canvas2D → 未來 WebGPU）。

use quire_engine::Engine;
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
