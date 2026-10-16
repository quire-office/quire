//! quire-web-collab —— 協作延遲分塊的 wasm 殼：只做綁定，零業務邏輯。
//!
//! 拆包鐵律：主殼 shells/web **不依賴**本 crate、也不帶 engine 的 collab feature；
//! CRDT/協作程式碼物理上只存在於本 crate 的 wasm 產物，由 JS 在用戶開啟協作時
//! 動態 import。分塊間不共享記憶體：本殼只操作「編輯操作序列」，
//! 落地到文件由 JS 端把操作套回主殼的 WasmEngine（見 demo/main.js 的橋接）。

use quire_engine::CollabSession;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmCollab {
    inner: CollabSession,
}

#[wasm_bindgen]
impl WasmCollab {
    #[wasm_bindgen(constructor)]
    pub fn new(session: u32) -> WasmCollab {
        Self {
            inner: CollabSession::new(u64::from(session)),
        }
    }

    /// 提交本地編輯 → 回傳要廣播給協作者的 JSON 字串。
    pub fn commit_local(&mut self, para: usize, run: usize, text: &str) -> String {
        self.inner.commit_local(para, run, text)
    }

    /// 收到廣播 → 回傳需套回主引擎的操作 JSON 陣列（JS 橋接執行）。
    pub fn apply_remote(&mut self, msg: &str) -> Vec<String> {
        self.inner.apply_remote(msg)
    }
}
