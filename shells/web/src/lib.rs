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

    // ---------------- E（卡 t_f91e5773）：表格索引與游標反查綁定 ----------------
    //
    // 引擎側符號早已存在（`table_count`／`tables_json`＝Phase3-1a；摘要與
    // 反查＝本卡在 engine-api 補的**純函數**），本節只做零業務邏輯透傳。
    // 存在意義（U4 報告 §3）：外殼此前拿不到表格索引，只能對越界 root_slot
    // 發「必然被拒的 Mutation」從錯誤訊息反推張數（零副作用探針降級序）——
    // 那是把引擎的真值藏起來後的考古，不是介面。真索引落地後外殼得以
    // 「點哪張改哪張」（換算規則見 `table_locator_json` 文件；移除 adapter
    // 降級序屬另卡，本卡嚴禁碰 shells/web/editor/）。

    /// body 級表格張數（canonical 投影；只讀，不參與存檔）。
    pub fn table_count(&self) -> usize {
        self.inner.table_count()
    }

    /// body 級表格樹 JSON（含巢狀；與 `Engine::tables_json` 逐字同形——
    /// 序列化的 `Vec<BodyTable>`，`rows[r].cells[c].blocks` 樹）。
    pub fn tables_json(&self) -> String {
        self.inner.tables_json()
    }

    /// 表格索引摘要 JSON 陣列：每項 `{root_slot, table_count, rows,
    /// grid_cols, para_cells, para_start, para_end}`——列「表格清單」的輕量
    /// 面，不需整棵 `tables_json`（多表文件可達數 MB）。`root_slot` 即
    /// `TablePath::root_slot`，直接可餵 M4 家族 Mutation。
    pub fn tables_summary_json(&self) -> String {
        self.inner.tables_summary_json()
    }

    /// 段落（游標所在段）→ 表格路徑反查 JSON。回 `Some(字串)`＝該段在某張
    /// 表（含巢狀表）的格內，`JSON.parse` 後形如：
    /// ```json
    /// {"para":5,
    ///  "table_path":{"target":"Body","root_slot":1,"nested_steps":[[0,2,0]]},
    ///  "cell_path":{"row":0,"cell":2,"cell_row":0,"grid_col_start":2,
    ///              "grid_span":1,"grid_cols":3,"rows":2,"depth":1}}
    /// ```
    /// `table_path` 欄位名／語義與 `Mutation` 的 `TablePath` 逐字對齊——
    /// 直接塞進 `SetCellProps.table`／`DeleteTable.table` 等即可，零第二輪
    /// 翻譯。巢狀步三元組＝(列, 格, 該格內第幾張巢狀表)。**回 `None` 不是
    /// 錯誤**：該段是表外正文段，外殼走正文編輯路徑即可（越界索引同 None，
    /// 全路徑零 panic——wasm 邊 panic＝瀏覽器 abort）。
    pub fn table_locator_json(&self, para: usize) -> Option<String> {
        self.inner.table_locator_json(para)
    }
}

// ============================ ODF-D2：ODT 入口 ============================
//
// 薄包 `quire_odf::OdtView`（PM 定案路線 1，卡 t_89c4a196）：**零業務邏輯**——
// 開檔、投影、分頁全在 core/odf（D1 發射層），本殼只做三件事：
//   1. 把 `OdfError` 轉成 JS 可讀的 `String` 錯誤（throw 語義）；
//   2. 補 JS 讀取面需要的 `paper_json`／`is_settled`／`reflow_step_json`
//      （ODT 線開檔一次性分頁，無增量重排——形制與 DOCX 線同契約，
//      playground.js 的 idle 迴圈與紙張狀態列對 ODT 直接可用，JS 端零改）；
//   3. 守住 wasm 邊界紀律：**任何方法不得 panic**（panic＝瀏覽器 abort）。
// 型別出口零新 schema：`page_display_list_json`／`dump_json` 直接轉出 `OdtView`
// 的既有 JSON 字串（同一個 `PageDisplayList` serde 契約）。
//
// 錯誤口徑（ODF-E1 教訓：不得靜默回 0 頁）：
// - 加密檔（manifest 宣告 encryption-data）→ `Err`，訊息照 `OdfError::Encrypted`
//   的顯示文案（含條目路徑與演算法名，不捏造）。實測落地點在 `page_geometry()`
//   那道閘（內部 `ensure_not_encrypted` 雙閘，先於發射）；就算它放過，
//   `OdtView::from_bundle`→`project()` 還有同一變體同一措辭的第二道閘——兩道
//   都是 core/odf 既有的，本殼不另開捷徑、不改寫訊息（紅燈報告 §3.4）。
// - 容器層破損／非 zip／空位元組 → `Err`，訊息前綴「開啟失敗」。
// - 零頁（分頁器給 0 頁）→ `Err`：正常 ODT 至少一頁（空投影也發 1 頁空行，
//   實測坐實），0 頁只可能是上游退化，寧可報錯也不給 JS 一張白頁。
//
// 為何錯誤型別用 `String` 而非 `JsError`：`JsError::new` 是 wasm-bindgen
// import（非 wasm target 呼叫即 panic），用 `String` 考卷才能在 host target
// 跑錯誤路徑。JS 端語義不變——wasm-bindgen 把 `Err(String)` 拋成 `Error`
// 物件（`err.message`＝該字串），playground.js 現行的
// `err.message || String(err)` 兩條口徑都吃得下。

/// ODT 顯示列表視圖（wasm 殼）：bytes 進、同 schema 顯示列表出。
///
/// 構造即完成開檔＋投影＋分頁（一次），後讀逐頁取 JSON 零重算。唯讀：
/// 本型別**不暴露**任何編輯面（ODT 線目前無編輯契約，見 D1 解剖報告 §5）。
#[wasm_bindgen]
pub struct WasmOdtView {
    inner: OdtView,
}

#[wasm_bindgen]
impl WasmOdtView {
    /// 由 .odt 位元組建視圖（wasm 無 fs：JS 端 `File.arrayBuffer()` 後餵位元組）。
    ///
    /// 失敗一律 `Err(訊息)`：加密／開檔／頁面幾何／零頁四條口徑見節頭註。
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<WasmOdtView, String> {
        console_error_panic_hook::set_once();
        // 先開封裝：容器層錯誤（非 zip／破損／空位元組）落「開啟失敗」口徑，
        // 與 shells/native 的 odt-dump 同措辭（跨殼錯誤訊息一致性）。
        let bundle = OdfBundle::open(bytes).map_err(|e| format!("開啟失敗: {e}"))?;
        // 頁面幾何：走 style:page-layout＋master-page 鏈（ODF-C4），解不到者
        // 由 quire-odf 回退 LO 預設頁——本殼不自烤 A4（假幾何是 D1 前的老帳）。
        let params = quire_odf::page_geometry(&bundle)
            .map_err(|e| format!("頁面幾何解析失敗: {e}"))?
            .to_layout_params();
        // from_bundle 內走 `project()` 的加密閘門：content.xml／styles.xml 任一
        // 宣告加密即 `Err(OdfError::Encrypted)`，訊息原文轉出（不吞、不改寫）。
        let inner = OdtView::from_bundle(&bundle, &params).map_err(|e| format!("發射失敗: {e}"))?;
        if inner.page_count() == 0 {
            return Err(
                "發射失敗: 分頁器回傳 0 頁（正常 ODT 至少 1 頁，拒絕交回空白視圖）".to_string(),
            );
        }
        Ok(Self { inner })
    }

    /// 頁數（分頁器給的實際頁數，≥1——構造期已守）。
    pub fn page_count(&self) -> usize {
        self.inner.page_count()
    }

    /// 單頁顯示列表 JSON（**與 `WasmEngine::page_display_list_json` 同 schema**：
    /// serde 序列化的 `PageDisplayList`，Text 項以變體名 `"Text"` 為鍵）。
    ///
    /// 越界頁碼回 `None`（`OdtView` 內部 `Vec::get`，不 index、不 panic）。
    /// JS 端照 playground.js 現行寫法先判 `if (!jsonStr) return;` 即可。
    pub fn page_display_list_json(&self, page: usize) -> Option<String> {
        self.inner.page_display_list_json(page)
    }

    /// 全頁顯示列表 JSON（pretty 陣列；`quire-cli odt-dump`／`dump` 同形，
    /// 供 QA 與 render_lab 逐欄比對，JS 端渲染不走這條）。
    pub fn dump_json(&self) -> String {
        self.inner.dump_json()
    }

    /// 段落數（`text:p`＋`text:h`；playground.js 狀態列 `metaParas` 讀取面）。
    pub fn paragraph_count(&self) -> usize {
        self.inner.paragraph_count()
    }

    /// 段落文字總碼點數（老實回報口徑，同 odt-render／D1 帳）。
    pub fn total_text_len(&self) -> usize {
        self.inner.total_text_len()
    }

    /// 生效紙張幾何 JSON。**欄位名與 `WasmEngine::paper_json` 逐欄同名**
    /// （JS 端 `updatePaperStatus`／`updatePaperUrl` 對 ODT 直接可用）。
    ///
    /// `paper_override_applied` 恆 `false`：紙張覆寫是 DOCX 線（playground
    /// 紙張面板）的概念，ODT 本殼不接受覆寫——發 `false` 是老實值，不是假裝支援。
    pub fn paper_json(&self) -> String {
        let p = self.inner.params();
        let m = p.margins_pt; // [上, 右, 下, 左]
        serde_json::to_string(&serde_json::json!({
            "page_w_pt": p.page_w_pt,
            "page_h_pt": p.page_h_pt,
            "margin_top_pt": m[0],
            "margin_bottom_pt": m[2],
            "margin_left_pt": m[3],
            "margin_right_pt": m[1],
            "paper_override_applied": false,
        }))
        .unwrap_or_else(|_| "{}".to_string())
    }

    /// 是否套用紙張覆寫（ODT 線恆 `false`，見 `paper_json` 檔注）。
    pub fn paper_override_applied(&self) -> bool {
        false
    }

    /// 版面是否已定案：ODT 線開檔一次性分頁，恆 `true`。
    ///
    /// 為何要這條：playground.js `render()` 先問 `is_settled()` 才決定要不要排
    /// `requestIdleCallback` 背景續排——缺這條 JS 直接 `TypeError`（函數不存在）。
    pub fn is_settled(&self) -> bool {
        true
    }

    /// 背景續排一步：ODT 線無增量重排（構造期已排完），恆回已定案＋零事件。
    /// 形制與 DOCX 線同契約（`{"settled":bool,"events":[...]}`），JS 迴圈零改。
    pub fn reflow_step_json(&self) -> String {
        "{\"settled\":true,\"events\":[]}".to_string()
    }
}

// ============================ K29a / U2：編輯與存檔綁定 ============================

#[wasm_bindgen]
impl WasmEngine {
    /// 依據 Mutation 規格套用單筆變更（JSON 進 JSON 出，錯誤亦序列化）。
    pub fn apply_mutation_json(&mut self, mutation_json: &str) -> String {
        let op: quire_engine::Mutation = match serde_json::from_str(mutation_json) {
            Ok(op) => op,
            Err(e) => {
                return serde_json::to_string(&serde_json::json!({
                    "ok": false,
                    "error": format!("Mutation JSON 反序列化失敗: {e}"),
                }))
                .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"反序列化失敗\"}".to_string());
            }
        };

        match self.inner.apply_mutation(op) {
            Ok(outcome) => serde_json::to_string(&serde_json::json!({
                "ok": true,
                "outcome": outcome,
            }))
            .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"結果序列化失敗\"}".to_string()),
            Err(e) => serde_json::to_string(&serde_json::json!({
                "ok": false,
                "error": e.to_string(),
            }))
            .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"錯誤序列化失敗\"}".to_string()),
        }
    }

    /// 復原頂端操作。
    pub fn undo_json(&mut self) -> String {
        match self.inner.undo() {
            Ok(outcome_opt) => serde_json::to_string(&serde_json::json!({
                "ok": true,
                "undone": outcome_opt.is_some(),
                "outcome": outcome_opt,
            }))
            .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"序列化失敗\"}".to_string()),
            Err(e) => serde_json::to_string(&serde_json::json!({
                "ok": false,
                "undone": false,
                "error": e.to_string(),
            }))
            .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"序列化失敗\"}".to_string()),
        }
    }

    /// 重做頂端操作。
    pub fn redo_json(&mut self) -> String {
        match self.inner.redo() {
            Ok(outcome_opt) => serde_json::to_string(&serde_json::json!({
                "ok": true,
                "redone": outcome_opt.is_some(),
                "outcome": outcome_opt,
            }))
            .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"序列化失敗\"}".to_string()),
            Err(e) => serde_json::to_string(&serde_json::json!({
                "ok": false,
                "redone": false,
                "error": e.to_string(),
            }))
            .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"序列化失敗\"}".to_string()),
        }
    }

    /// 存檔為 DOCX 位元組陣列。
    pub fn save_docx_bytes(&self) -> Vec<u8> {
        self.inner.save_docx().unwrap_or_default()
    }
}
