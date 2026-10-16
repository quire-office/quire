// Node 端 smoke test（免瀏覽器）：驗證拆包後兩個 wasm 產物的真實功能。
//   1) 首包：開檔→分頁→顯示列表→編輯→round-trip（經 glue JS）。
//   2) 協作分塊：延遲 init → A/B 兩會話交叉合併 → 操作落地主引擎。
//   3) 反「假拆包」：主 wasm 載入後，CRDT 符號不在其中（由 size 閘門把關，
//      這裡再驗一次功能面：主 glue 不導出 WasmCollab）。
// CI 用：node scripts/wasm-smoke.mjs（先跑兩個 wasm-pack build）。
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const fail = (msg) => { console.error(`✗ ${msg}`); process.exit(1); };
const ok = (msg) => console.log(`✓ ${msg}`);

// ---- 1) 首包 ----
const initMain = (await import(path.join(ROOT, "shells/web/pkg/quire_web.js"))).default;
const { WasmEngine } = await import(path.join(ROOT, "shells/web/pkg/quire_web.js"));
const mainWasm = fs.readFileSync(path.join(ROOT, "shells/web/pkg/quire_web_bg.wasm"));
await initMain({ module_or_path: new Uint8Array(mainWasm) });
ok("首包 wasm 初始化（Node, module_or_path 手動餵位元組）");

const docBytes = fs.readFileSync(path.join(ROOT, "fixtures/sample.docx"));
const engine = new WasmEngine(new Uint8Array(docBytes));
if (engine.page_count() < 1) fail("首包開檔後頁數 < 1");
if (engine.paragraph_count() < 3) fail("首包開檔後段落數 < 3");
const dl = JSON.parse(engine.page_display_list_json(0));
if (!dl.items.some((i) => i.Text)) fail("顯示列表無 Text 項");
ok(`首包渲染：${engine.page_count()} 頁 / ${engine.paragraph_count()} 段 / ${dl.items.length} 顯示項`);

const events = engine.replace_paragraph_text(1, "煙霧測試文字".repeat(500));
if (events.length === 0) fail("編輯後無失效事件");
if (engine.page_count() < 2) fail("長文編輯後未重排增頁");
ok(`首包編輯重排：事件 ${events.length} 筆 → ${engine.page_count()} 頁`);

// ---- 2) 協作分塊（延遲載入語意的 Node 等價：此刻才第一次讀檔+init） ----
const collabMod = await import(
  path.join(ROOT, "shells/web-collab/pkg/quire_web_collab.js")
);
if (typeof collabMod.WasmCollab !== "function") fail("協作分塊未導出 WasmCollab");
const collabWasm = fs.readFileSync(
  path.join(ROOT, "shells/web-collab/pkg/quire_web_collab_bg.wasm"),
);
await collabMod.default({ module_or_path: new Uint8Array(collabWasm) });
ok("協作分塊 wasm 初始化（延遲路徑）");

const A = new collabMod.WasmCollab(1);
const B = new collabMod.WasmCollab(2);
const msgA = A.commit_local(2, 0, "A 端協作文字");
const msgB = B.commit_local(2, 0, "B 端協作文字");
const landedOnA = A.apply_remote(msgB);
const landedOnB = B.apply_remote(msgA);
// (lamport=1, session=2) > (1,1)：B 勝。敗端的 apply_remote 回傳空（自己保留=
// 就是勝者），勝者端則收到要落地的操作。收斂判準＝兩端最終文字相同。
const finalText = (landed, ownText) =>
  landed.length ? JSON.parse(landed[0]).ReplaceRunText.text : ownText;
const wA = finalText(landedOnA, "A 端協作文字");
const wB = finalText(landedOnB, "B 端協作文字");
if (wA !== "B 端協作文字" || wB !== "B 端協作文字") {
  fail(`併發合併未收斂到 B：A 端=${wA}，B 端=${wB}`);
}
ok(`協作分塊合併收斂：雙端最終皆「B 端協作文字」（落地 ${landedOnA.length}/${landedOnB.length} op）`);

// 落地到主引擎（JS 橋的 Node 等價）。
engine.replace_paragraph_text(2, wA);
ok("協作操作經 JSON 橋落地主引擎");

// ---- 3) 假拆包防護（功能面）：主 glue 不得導出 WasmCollab ----
const mainGlue = await import(path.join(ROOT, "shells/web/pkg/quire_web.js"));
if ("WasmCollab" in mainGlue) fail("假拆包：主 glue 導出了 WasmCollab");
ok("主 glue 無 WasmCollab 導出（CRDT 不在首包）");

console.log("✓ wasm smoke 全綠（首包＋協作分塊）");
