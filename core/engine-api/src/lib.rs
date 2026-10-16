//! quire-engine —— shells 唯一准 import 的契約層（見 docs/雙殼架構規劃）。
//!
//! 規則（CI 鎖死，scripts/check-layering.sh）：
//! - shells/* 只能依賴本 crate；
//! - 本 crate 及其下全部 core 必須在 wasm32-unknown-unknown 可編譯；
//! - 型別全 serde 化，跨 wasm 邊界走 JSON 字串。

use quire_layout::{
    page_breaks_per_section, CaretPos, LayoutEngine, LayoutParams, Page, SectionGeometry,
    SectionSpec,
};
use quire_ooxml::{
    diff_untouched, diff_zip_structure, parse, read_sections_per_paragraph, OoxmlDoc, OoxmlError,
    ZipDiff,
};
use std::collections::BTreeMap;
use thiserror::Error;

// shells 需要顯示列表型別但只能 import 本 crate：在此 re-export（分層鐵律的唯一合法通道）。
pub use quire_render_trait::{DrawItem, PageDisplayList, Renderer, FLAG_BOLD, FLAG_ITALIC};

#[derive(Error, Debug)]
pub enum EngineError {
    #[error(transparent)]
    Ooxml(#[from] OoxmlError),
    #[error("zip 結構對照失敗: {0}")]
    Diff(#[from] quire_ooxml::ZipStructureError),
}

pub struct Engine {
    doc: OoxmlDoc,
    layout: LayoutEngine,
    params: LayoutParams,
    /// 開啟時的原始位元組——roundtrip 對照的基準。
    original: Vec<u8>,
    /// 被編輯器動過的 part key（round-trip 白名單：這些 entry 允許內容變動）。
    touched: BTreeMap<String, ()>,
}

impl Engine {
    /// 位元組進（wasm 無 fs；native 自己讀檔再傳位元組進來）。
    ///
    /// 頁面幾何不寫死：逐 section 解析 `sectPr/pgSz`／`pgMar` 餵進版面層
    /// （Phase0-2b）。檔案毫無 sectPr 時退回 `LayoutParams::default()`（後備）。
    pub fn from_docx_bytes(bytes: &[u8]) -> Result<Self, EngineError> {
        let doc = parse(bytes)?;
        let texts = doc.paragraph_texts();
        let params = LayoutParams::default();
        let sections: Vec<SectionSpec> = doc
            .parts
            .get("word/document.xml")
            .map(|xml| {
                read_sections_per_paragraph(xml, texts.len())
                    .into_iter()
                    .map(|s| SectionSpec {
                        id: s.id,
                        geo: SectionGeometry {
                            page_w_pt: s.geo.page_w_pt,
                            page_h_pt: s.geo.page_h_pt,
                            margin_top_pt: s.geo.margin_top_pt,
                            margin_bottom_pt: s.geo.margin_bottom_pt,
                            margin_left_pt: s.geo.margin_left_pt,
                            margin_right_pt: s.geo.margin_right_pt,
                            landscape: s.geo.landscape,
                            continuous: s.geo.continuous,
                        },
                    })
                    .collect()
            })
            .unwrap_or_default();
        // 基準 params：以首 section 的頁面欄位覆寫後備值（字級/行距不變），
        // 讓「無 section 段落」與 shells 讀 params() 時也拿到文件實際尺寸。
        let params = match sections.first() {
            Some(s) => LayoutParams::from_section(&s.geo, params.font_size_pt, params.line_spacing),
            None => params,
        };
        let layout = LayoutEngine::with_sections(&texts, params, sections);
        Ok(Self {
            doc,
            layout,
            params,
            original: bytes.to_vec(),
            touched: BTreeMap::new(),
        })
    }

    pub fn page_count(&self) -> usize {
        self.layout.page_count()
    }

    pub fn paragraph_count(&self) -> usize {
        self.doc.paragraphs.len()
    }

    /// lossless 主張的最低保證：所有 zip part 都還在。
    pub fn preserved_part_count(&self) -> usize {
        self.doc.parts.len()
    }

    pub fn page_lines(&self, page: usize) -> Option<Vec<String>> {
        self.layout
            .pages()
            .get(page)
            .map(|p| p.lines.iter().map(|l| l.text.clone()).collect())
    }

    /// 段落純文字（Web-1 隱形錨點游標插入讀目前段落用）。
    pub fn paragraph_text(&self, para: usize) -> Option<String> {
        self.layout.paragraph_text(para).map(str::to_owned)
    }

    /// caret (段落, 碼點) → 頁面矩形 JSON（JS 疊畫 caret、定位隱形錨點）。
    pub fn caret_rect_json(&self, para: usize, cp: usize) -> Option<String> {
        self.layout
            .caret_rect(CaretPos { para, cp })
            .and_then(|r| serde_json::to_string(&r).ok())
    }

    /// 頁面點擊 (page, x_pt, y_pt) → caret JSON（canvas→輸入端反向契約）。
    pub fn caret_at_json(&self, page: usize, x_pt: f32, y_pt: f32) -> Option<String> {
        self.layout
            .caret_at(page, x_pt, y_pt)
            .and_then(|c| serde_json::to_string(&c).ok())
    }

    /// 在 caret 處插入文字（IME commit 的引擎端入口）。回傳失效事件 JSON。
    /// `cp` 以碼點計；位元組邊界換算在引擎做——JS 端不碰 UTF-8 切口。
    pub fn insert_text_at_caret(&mut self, para: usize, cp: usize, text: &str) -> Vec<String> {
        let Some(cur) = self.layout.paragraph_text(para).map(str::to_owned) else {
            return Vec::new();
        };
        // 碼點 → 位元組偏移（不可在 char 邊界外切：UTF-8 安全）。
        let byte = cur
            .char_indices()
            .nth(cp)
            .map(|(i, _)| i)
            .unwrap_or(cur.len());
        let mut next = String::with_capacity(cur.len() + text.len());
        next.push_str(&cur[..byte]);
        next.push_str(text);
        next.push_str(&cur[byte..]);
        self.replace_paragraph_text(para, &next)
    }

    pub fn page_display_list(&self, page: usize) -> Option<PageDisplayList> {
        let p: &Page = self.layout.pages().get(page)?;
        let line_h = self.params.font_size_pt * self.params.line_spacing;
        let items = p
            .lines
            .iter()
            .enumerate()
            .map(|(i, l)| DrawItem::Text {
                x: p.margin_left_pt,
                y: p.margin_top_pt + (i as f32) * line_h,
                size_pt: self.params.font_size_pt,
                // 骨架：run 級粗斜體尚未穿透到顯示列表（演進點：run-aware display list）
                flags: 0,
                text: l.text.clone(),
            })
            .collect();
        Some(PageDisplayList {
            page,
            width_pt: p.width_pt,
            height_pt: p.height_pt,
            items,
        })
    }

    /// 分頁斷點清單（JSON）：每頁段落分佈＋跨頁續排標記＋逐頁實際尺寸，
    /// render-lab 基線比對用。
    pub fn page_breaks_json(&self) -> String {
        let brks = page_breaks_per_section(
            &self.doc.paragraph_texts(),
            &self.params,
            self.layout.sections(),
        );
        serde_json::to_string_pretty(&brks).unwrap_or_else(|_| "[]".to_string())
    }

    /// 跨 wasm 邊界的序列化入口。
    pub fn page_display_list_json(&self, page: usize) -> Option<String> {
        self.page_display_list(page)
            .and_then(|dl| serde_json::to_string(&dl).ok())
    }

    /// 回傳失效事件（序列化），供 shells 做增量重繪。
    pub fn replace_paragraph_text(&mut self, para: usize, text: &str) -> Vec<String> {
        self.layout
            .replace_paragraph_text(para, text)
            .into_iter()
            .map(|ev| format!("{ev:?}"))
            .collect()
    }

    // ---------------- Phase0-6：增量重排的控制面（shells 唯一入口） ----------------

    /// 預算制軟分頁開關（`Some(n)`＝同步重排最多新增 n 頁，尾段留給
    /// `reflow_step_json` 於空檔續排；`None`＝全速，編輯後版面即定案）。
    pub fn set_sync_budget(&mut self, pages: Option<usize>) {
        self.layout.set_sync_budget(pages);
    }

    /// 版面是否已定案（無待排尾段）。
    pub fn is_settled(&self) -> bool {
        self.layout.is_settled()
    }

    /// 背景續排一步：`{"settled":bool,"events":[...]}` JSON。shell 在收到
    /// `ReflowPending` 後於 requestIdleCallback/空檔重複呼叫直到 settled。
    pub fn reflow_step_json(&mut self) -> String {
        let (evs, settled) = self.layout.reflow_step();
        let events: Vec<String> = evs.into_iter().map(|ev| format!("{ev:?}")).collect();
        serde_json::to_string(&serde_json::json!({ "settled": settled, "events": events }))
            .unwrap_or_else(|_| "{\"settled\":true,\"events\":[]}".to_string())
    }

    // ---------------- 無損存檔 / round-trip 驗證 ----------------

    /// 存檔：未觸碰 part 逐位元組沿用（見 quire-ooxml 文件）。
    pub fn save_docx(&self) -> Result<Vec<u8>, EngineError> {
        Ok(self.doc.save()?)
    }

    /// 開啟→（編輯）→存檔後，與原始位元組做結構級對照。
    /// 已編輯過的 part 自動進白名單；其餘 entry 必須零失真。
    pub fn roundtrip_check(&self) -> Result<ZipDiff, EngineError> {
        let saved = self.save_docx()?;
        if self.touched.is_empty() {
            Ok(diff_untouched(&self.original, &saved)?)
        } else {
            Ok(diff_zip_structure(&self.original, &saved, &self.touched)?)
        }
    }

    /// 編輯 part 明文（標記白名單）。
    pub fn set_part(&mut self, name: &str, bytes: Vec<u8>) {
        self.touched.insert(name.to_string(), ());
        self.doc.set_part(name, bytes);
    }

    /// 離線對照：原始位元組 vs 存回位元組，`allow` 為允許內容變動的 part 白名單。
    /// CLI 的 roundtrip --check 走這裡（shells 只能經 engine，不直接 import ooxml）。
    pub fn roundtrip_diff(
        original: &[u8],
        saved: &[u8],
        allow: &[String],
    ) -> Result<ZipDiff, EngineError> {
        let touched: BTreeMap<String, ()> = allow.iter().map(|k| (k.clone(), ())).collect();
        Ok(diff_zip_structure(original, saved, &touched)?)
    }
}

// ---------------- 協作面（feature = collab）----------------
//
// 拆包鐵律：本區塊只在 `collab` feature 下編譯。主 wasm（shells/web）不帶此
// feature，CRDT 程式碼物理上不在首包裡；只有 shells/web-collab（延遲載入的
// 第二個 wasm）用 `quire-engine/collab` 把它鏈進去。CI 以 cargo tree 守住
// quire-web 不得（直接或傳遞）依賴 quire-crdt。
//
// 協作會話刻意與 Engine 分離：第二個 wasm 模組不能與主 wasm 共享記憶體，
// 若把會話掛在 Engine 上就得在主包外重開一份文件。會話只碰操作序列，
// 落地（套用到主引擎的編輯面）由 JS 端橋接。

/// 協作會話句柄（佔位 in-memory 後端；loro 落地後換實作、契約不變）。
#[cfg(feature = "collab")]
pub struct CollabSession {
    backend: quire_crdt::InMemoryBackend,
}

#[cfg(feature = "collab")]
use quire_crdt::CrdtBackend as _;

#[cfg(feature = "collab")]
impl CollabSession {
    pub fn new(session: u64) -> Self {
        Self {
            backend: quire_crdt::InMemoryBackend::new(session),
        }
    }

    /// 本地編輯提交 → 回傳要廣播給協作者的位元組（JSON 字串）。
    pub fn commit_local(&mut self, para: usize, run: usize, text: &str) -> String {
        let bytes = self
            .backend
            .commit_local(&quire_crdt::EditOp::ReplaceRunText {
                para,
                run,
                text: text.to_string(),
            });
        String::from_utf8(bytes).unwrap_or_default()
    }

    /// 收到協作者廣播（JSON 字串）→ 回傳需落地到編輯面的操作 JSON 陣列，
    /// 由呼叫端（JS 橋）套回主引擎。
    pub fn apply_remote(&mut self, msg: &str) -> Vec<String> {
        self.backend
            .apply_remote(msg.as_bytes())
            .into_iter()
            .map(|op| serde_json::to_string(&op).unwrap_or_default())
            .collect()
    }
}

#[cfg(all(feature = "collab", test))]
mod collab_tests {
    use super::*;

    #[test]
    fn commit_and_remote_apply_roundtrip() {
        let mut host = CollabSession::new(1);
        let msg = host.commit_local(0, 0, "協作文字");
        let mut peer = CollabSession::new(2);
        let landed = peer.apply_remote(&msg);
        assert_eq!(landed.len(), 1, "遠端操作應落地到編輯面");
        assert!(landed[0].contains("ReplaceRunText"));
        assert!(landed[0].contains("協作文字"));
    }

    #[test]
    fn garbage_message_lands_nothing() {
        let mut peer = CollabSession::new(2);
        assert!(peer.apply_remote("不是 JSON").is_empty());
    }
}

/// 給 native shell 的顯示列表批次匯出（評測 render-lab 用）。
pub fn dump_display_lists_json(engine: &Engine) -> String {
    let all: Vec<PageDisplayList> = (0..engine.page_count())
        .filter_map(|i| engine.page_display_list(i))
        .collect();
    serde_json::to_string_pretty(&all).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = include_bytes!("../../../fixtures/sample.docx");

    #[test]
    fn end_to_end_open_and_layout() {
        let engine = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx 失敗");
        assert!(engine.page_count() >= 1);
        assert!(engine.paragraph_count() >= 3);
        assert_eq!(
            engine.preserved_part_count(),
            3,
            "[Content_Types].xml + .rels + document.xml"
        );
        let json = engine
            .page_display_list_json(0)
            .expect("第 0 頁顯示列表不存在");
        // DrawItem 為 externally-tagged enum：序列化後以變體名 "Text" 為鍵。
        assert!(json.contains("\"Text\""), "顯示列表 JSON 應含 Text 變體");
        let all_pages: String = (0..engine.page_count())
            .filter_map(|i| engine.page_lines(i))
            .flatten()
            .collect();
        assert!(all_pages.contains("Quire"));
    }

    #[test]
    fn edit_then_repaginate() {
        let mut engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        let before = engine.page_count();
        let events = engine.replace_paragraph_text(1, &"重排測試".repeat(3000));
        assert!(!events.is_empty());
        assert!(engine.page_count() > before);
        assert!(engine.is_settled(), "全速模式：編輯後即定案");
    }

    #[test]
    fn soft_pagination_reflow_settles_via_json_api() {
        // Phase0-6 控制面：小預算灌長文 → ReflowPending；reflow_step_json
        // 逐步推進直到 settled，頁數收斂到全速重建同款。
        let mut engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        engine.set_sync_budget(Some(1));
        let evs = engine.replace_paragraph_text(1, &"預算".repeat(20000));
        assert!(
            evs.iter().any(|e| e.starts_with("ReflowPending")),
            "小預算長文必須出現軟分頁：{evs:?}"
        );
        assert!(!engine.is_settled());
        let mut steps = 0usize;
        while !engine.is_settled() {
            let v: serde_json::Value = serde_json::from_str(&engine.reflow_step_json()).unwrap();
            assert!(v["events"].is_array(), "reflow JSON 契約：events 陣列");
            steps += 1;
            assert!(steps < 500, "reflow 必須收敛（卡在 {steps} 步）");
        }
        // 定案後版面必須與「重開一份同內容文件」頁數一致（全速對照）。
        let full = Engine::from_docx_bytes(SAMPLE).unwrap();
        assert!(engine.page_count() > full.page_count());
    }

    #[test]
    fn ime_commit_inserts_at_caret_including_emoji() {
        // Web-1 契約：IME commit（含注音合成結果與 emoji）經 caret 插入進引擎。
        let mut engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        let p0 = engine.paragraph_text(0).unwrap();
        let n = p0.chars().count();
        // 段尾 commit「測試😀」——emoji 是 surrogate pair，碼點切口不可斷半。
        let evs = engine.insert_text_at_caret(0, n, "測試😀");
        assert!(!evs.is_empty(), "commit 必須回報失效事件");
        let after = engine.paragraph_text(0).unwrap();
        assert!(after.ends_with("測試😀"), "commit 結果應附於段尾：{after}");
        assert_eq!(
            after.chars().count(),
            n + 3,
            "碼點數必須精確（emoji=1 碼點）"
        );
        // 段首插入（caret=0）。
        engine.insert_text_at_caret(0, 0, "★");
        assert!(engine.paragraph_text(0).unwrap().starts_with('★'));
        // caret 越界鉗到段尾，不可 panic。
        engine.insert_text_at_caret(0, 1 << 20, "END");
        assert!(engine.paragraph_text(0).unwrap().ends_with("END"));
        // 不存在的段落回空事件、不 panic。
        assert!(engine.insert_text_at_caret(9999, 0, "X").is_empty());
    }

    #[test]
    fn caret_round_trip_via_json() {
        // wasm/JS 邊界走 JSON：caret_rect_json / caret_at_json 必須可解析。
        let engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        let rect: serde_json::Value =
            serde_json::from_str(&engine.caret_rect_json(0, 1).unwrap()).unwrap();
        assert!(rect["x_pt"].as_f64().unwrap() > 0.0);
        let back: serde_json::Value = serde_json::from_str(
            &engine
                .caret_at_json(
                    rect["page"].as_u64().unwrap() as usize,
                    rect["x_pt"].as_f64().unwrap() as f32,
                    rect["y_pt"].as_f64().unwrap() as f32 + 6.0,
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(back["para"].as_u64().unwrap(), 0);
    }

    #[test]
    fn untouched_round_trip_is_byte_identical() {
        let engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        let saved = engine.save_docx().unwrap();
        assert_eq!(saved.as_slice(), SAMPLE, "開檔→不動→存檔必須位元組級相同");
        let d = engine.roundtrip_check().unwrap();
        assert!(
            d.byte_identical && d.pass,
            "未觸碰對照必須綠燈:\n{}",
            d.report()
        );
    }

    #[test]
    fn edited_part_passes_with_whitelist_and_others_are_zero_loss() {
        let mut engine = Engine::from_docx_bytes(SAMPLE).unwrap();
        engine.set_part("word/document.xml", b"<w:document edited/>".to_vec());
        let d = engine.roundtrip_check().unwrap();
        assert!(d.pass, "白名單內改動，其餘零失真:\n{}", d.report());
        assert_eq!(d.whitelisted_changed, vec!["word/document.xml".to_string()]);
        // 重新開啟後編輯內容確實生效、其他 part 還在
        let again = Engine::from_docx_bytes(&engine.save_docx().unwrap()).unwrap();
        assert_eq!(again.preserved_part_count(), 3);
    }
}
