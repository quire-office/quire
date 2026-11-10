// ODF-D2 Node 端考卷（免瀏覽器）：對**實際出貨的 wasm 產物**打 ODT 路徑。
//
// 為何另開這條（host cargo 考卷 `tests/odf_d2_wasm_view.rs` 已七盞綠）：
// host 考卷走的是 Rust 直接調用，**沒經過 wasm-bindgen glue**。本腳本補的正是
// 那一段——`npm run build:wasm` 的 `pkg/quire_web.js`＋`pkg/quire_web_bg.wasm`
// 真產物，逐字照 `quire-www/public/playground/playground.js` 的讀取面呼叫
// （`page_count` / `page_display_list_json` / `paragraph_count` / `paper_json` /
// `is_settled` / `reflow_step_json`），證「JS 端零改」不是口號：同一段繪圖碼
// 的取數序列，對 ODT 直接跑得通。
//
// CI 用：cd shells/web && npm run build:wasm && node tests/odf_d2_node_smoke.mjs
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// 本檔在 shells/web/tests/ ——倉庫根＝往上三層。
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const fail = (msg) => {
  console.error(`✗ ${msg}`);
  process.exit(1);
};
const ok = (msg) => console.log(`✓ ${msg}`);

const PKG = path.join(ROOT, "shells/web/pkg/quire_web.js");
if (!fs.existsSync(PKG)) fail("找不到 pkg/——先在 shells/web 跑 npm run build:wasm");

const init = (await import(PKG)).default;
const { WasmEngine, WasmOdtView } = await import(PKG);
const wasmBytes = fs.readFileSync(
  path.join(ROOT, "shells/web/pkg/quire_web_bg.wasm"),
);
await init({ module_or_path: new Uint8Array(wasmBytes) });
ok("實際產物 wasm 初始化（Node, module_or_path 手動餵位元組）");

if (typeof WasmOdtView !== "function") fail("首包 glue 未導出 WasmOdtView");
ok("首包 glue 導出 WasmOdtView（ODT 面在首包，非分塊）");

const odt = fs.readFileSync(
  path.join(
    ROOT,
    "corpus-odt/ext/lo_core_sw_qa_odfexport_WordTest_edit.odt",
  ),
);

// ---- 1) playground.js 的開檔面：new → page_count（try/catch 口徑同站端） ----
let view;
try {
  view = new WasmOdtView(odt);
} catch (err) {
  // playground.js:690 讀 err.message || String(err)——wasm-bindgen 拋 Error 時
  // message＝Rust 側 Err(String) 原文，這條口徑必須有內容。
  const m = err?.message || String(err);
  if (!m || m === "undefined") fail(`開檔錯誤訊息空白（JS 端只能秀 ${m}）`);
  fail(`ODT 開檔不應失敗：${m}`);
}
const pageCount = view.page_count();
if (pageCount < 1) fail(`ODT page_count()=${pageCount} < 1（站端會秀「分頁異常」）`);
ok(`ODT 開檔渲染：${pageCount} 頁 / ${view.paragraph_count()} 段`);

// ---- 2) 顯示列表：playground.js render() 的取數序列逐字重播 ----
for (let p = 0; p < pageCount; p++) {
  const jsonStr = view.page_display_list_json(p);
  if (!jsonStr) fail(`第 ${p} 頁回傳空（page_count 說有 ${pageCount} 頁）`);
  const dl = JSON.parse(jsonStr);
  const scale = 850 / (dl.width_pt || 595); // playground.js:264 旁的縮放算式
  if (!(scale > 0)) fail(`第 ${p} 頁 width_pt 不可用（${dl.width_pt}）`);
  if (!(dl.height_pt > 0)) fail(`第 ${p} 頁 height_pt 不可用（${dl.height_pt}）`);
  let nText = 0;
  for (const item of dl.items) {
    if (!item.Text) continue;
    nText++;
    const t = item.Text;
    // Canvas2D 直繪算術（playground.js/main.js）：t.x*scale、(t.y+t.size_pt)*scale、
    // t.flags & 位元、argbToCss(t.color_argb ?? ...)、fillText(t.text)。
    for (const k of ["x", "y", "size_pt"]) {
      if (!Number.isFinite(t[k])) fail(`第 ${p} 頁 Text.${k} 非有限數：${t[k]}`);
    }
    if (!Number.isInteger(t.flags ?? 0)) fail(`Text.flags 非整數：${t.flags}`);
    if (!Number.isInteger(t.color_argb ?? 0)) fail(`Text.color_argb 非整數：${t.color_argb}`);
    if (typeof t.text !== "string") fail(`Text.text 非字串：${typeof t.text}`);
    if (!Number.isFinite((t.y + t.size_pt) * scale)) fail("baselineY 算不出來");
  }
  if (p === 0 && nText < 1) fail("第 0 頁零 Text 項——白頁");
  if (p === 0) ok(`第 0 頁可畫：${dl.items.length} 顯示項（${nText} 個 Text）`);
}

// ---- 3) 越界頁碼：JS 端拿到 null/undefined 必須不 throw（playground.js 先判空） ----
for (const bad of [pageCount, pageCount + 999, 2 ** 53 - 1]) {
  let got;
  try {
    got = view.page_display_list_json(bad);
  } catch (err) {
    fail(`越界頁碼 ${bad} 拋出例外（wasm 邊 panic＝瀏覽器 abort）：${err}`);
  }
  if (got) fail(`越界頁碼 ${bad} 竟回傳內容`);
}
ok(`越界頁碼全回空且不 abort（含 2**53-1，共 ${pageCount} 頁界外三連發）`);

// ---- 4) 紙張狀態列：paper_json 七鍵（playground.js updatePaperStatus/updatePaperUrl） ----
let pj;
try {
  pj = JSON.parse(view.paper_json());
} catch (err) {
  fail(`paper_json 不是合法 JSON：${err}`);
}
for (const k of [
  "page_w_pt",
  "page_h_pt",
  "margin_top_pt",
  "margin_bottom_pt",
  "margin_left_pt",
  "margin_right_pt",
  "paper_override_applied",
]) {
  if (pj[k] === undefined || pj[k] === null) fail(`paper_json 缺鍵 ${k}`);
}
if (pj.paper_override_applied !== false) {
  fail("ODT 線 paper_override_applied 必須恆 false（本殼不接受紙張覆寫）");
}
ok(`紙張狀態列可用：${pj.page_w_pt}×${pj.page_h_pt}pt（未覆寫）`);

// ---- 5) idle 迴圈面：is_settled / reflow_step_json（JS 端零改的必要條件） ----
if (view.is_settled() !== true) fail("ODT 開檔即定案：is_settled() 必須 true");
const rf = JSON.parse(view.reflow_step_json());
if (rf.settled !== true || !Array.isArray(rf.events) || rf.events.length !== 0) {
  fail(`reflow_step_json 形制不符：${view.reflow_step_json()}`);
}
ok("idle 續排迴圈零改可用（is_settled 恆真、reflow_step_json 零事件）");

// ---- 6) 老實報錯：加密檔必須 throw 且訊息帶「加密」（ODF-E1 教訓） ----
const enc = fs.readFileSync(
  path.join(ROOT, "corpus-odt/ext/lo_core_sw_qa_odfexport_sha1_correct.odt"),
);
try {
  const v = new WasmOdtView(enc);
  fail(`加密檔竟開檔成功（${v.page_count()} 頁）——靜默白頁是禁止形態`);
} catch (err) {
  const m = err?.message || String(err);
  if (!m.includes("加密")) fail(`加密檔錯誤訊息未含「加密」：${m}`);
  ok(`加密檔老實報錯：${m.slice(0, 72)}…`);
}
try {
  new WasmOdtView(new Uint8Array([0, 1, 2, 3, 4, 5]));
  fail("垃圾位元組竟開檔成功");
} catch (err) {
  const m = err?.message || String(err);
  if (!m.includes("開啟失敗")) fail(`垃圾位元組錯誤口徑不對：${m}`);
  ok("垃圾位元組落「開啟失敗」口徑");
}

// ---- 7) 假拆包防護：ODT 面在首包，CRDT 符號仍不得進首包 glue ----
if ("WasmCollab" in (await import(PKG))) fail("假拆包：主 glue 導出了 WasmCollab");

// ---- 8) DOCX 零回退：同產物的 WasmEngine 照舊可開可畫 ----
const doc = fs.readFileSync(path.join(ROOT, "fixtures/sample.docx"));
const engine = new WasmEngine(doc);
if (engine.page_count() < 1) fail("DOCX 線頁數回退");
const docDl = JSON.parse(engine.page_display_list_json(0));
if (!docDl.items.some((i) => i.Text)) fail("DOCX 線顯示列表無 Text 項");
ok(`DOCX 線零回退：${engine.page_count()} 頁 / ${docDl.items.length} 顯示項`);

console.log("✓ ODF-D2 wasm 端考卷全綠（實際產物＋playground 取數序列）");
