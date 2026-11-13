// E（卡 t_f91e5773）Node 端考卷（免瀏覽器）：對**實際出貨的 wasm 產物**打
// 表格索引＋游標反查面。
//
// 為何另開這條（host cargo 考卷 `tests/e_wasm_table_map.rs` 已八盞綠）：
// host 考卷走 Rust 直接調用，**完全沒經過 wasm-bindgen glue**——glue 段的
// 破口（導出缺名、usize 邊界、Option→undefined、巢狀陣列三元組序列化）
// 只有對 `pkg/` 實際產物打的這條才抓得到（rust-wasm-dual-target 鐵律：
// 殼層考卷兩層都要打）。
//
// 驗證面（外殼 adapter 未來接線的讀取契約，逐字用它的呼叫形）：
//   table_count() → number
//   tables_json() → string（JSON.parse 可吃）
//   tables_summary_json() → string（root_slot 對號陣列）
//   table_locator_json(para) → string | undefined（None→undefined，JS 端
//     以 `if (!s)` 判表外段——**undefined 不是異常**）
//   table_path 直通 apply_mutation_json（M4 真改到目標那張表）
//
// CI 用：cd shells/web && npm run build:wasm && node tests/e_wasm_table_map_node_smoke.mjs
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
const { WasmEngine } = await import(PKG);
const wasmBytes = fs.readFileSync(path.join(ROOT, "shells/web/pkg/quire_web_bg.wasm"));
await init({ module_or_path: new Uint8Array(wasmBytes) });
ok("實際產物 wasm 初始化（Node, module_or_path 手動餵位元組）");

// ---- 0) glue 導出鎖：四條新方法必須在首包（缺名＝JS 端 TypeError） ----
{
  const proto = WasmEngine.prototype;
  for (const m of ["table_count", "tables_json", "tables_summary_json", "table_locator_json"]) {
    if (typeof proto[m] !== "function") fail(`glue 未導出 WasmEngine.${m}`);
  }
  ok("glue 導出四條表格綁定（table_count/tables_json/tables_summary_json/table_locator_json）");
}

const engine = new WasmEngine(
  fs.readFileSync(path.join(ROOT, "fixtures/table_multi.docx")),
);

// ---- 1) 計數：number 型別真值（非 BigInt 洩漏、非 undefined） ----
const count = engine.table_count();
if (typeof count !== "number") fail(`table_count() 回 ${typeof count}（JS 端算術需要 number）`);
if (count !== 3) fail(`table_count()=${count}，fixture 應為 3`);
ok(`table_count()＝${count}（number 型別，JS 直接可用）`);

// ---- 2) tables_json：parse 面＋樹欄位名（JS 端 JSON.parse 是本卡契約） ----
const tree = JSON.parse(engine.tables_json());
if (!Array.isArray(tree) || tree.length !== 3) fail("tables_json 必須是 3 根表陣列");
if (tree[0]["table"]["grid"].length !== 3) fail("T0 網格欄數漂移");
ok("tables_json JSON.parse 直吃＋欄位名（table/grid/rows）穩定");

// ---- 3) 摘要面：root_slot 對號＋列/欄/含巢狀數真值 ----
const summaries = JSON.parse(engine.tables_summary_json());
if (summaries.length !== 3) fail(`摘要應 3 項，實 ${summaries.length}`);
summaries.forEach((s, i) => {
  if (s.root_slot !== i) fail(`摘要 root_slot 錯號：[${i}] 回 ${s.root_slot}`);
});
if (summaries[0].table_count !== 2) fail("T0 含巢狀計數錯");
if (summaries[0].rows !== 4 || summaries[0].grid_cols !== 3) fail("T0 形制漂移");
ok("tables_summary_json：root_slot 對號陣列（列/欄/含巢狀數真值）");

// ---- 4) 游標反查：逐段呼叫形制（string|undefined）＋關鍵落點真值 ----
const N = engine.paragraph_count();
const hits = [];
for (let p = 0; p < N; p++) {
  const s = engine.table_locator_json(p);
  if (s === undefined) continue; // 表外段（本 fixture 無）——undefined 非異常
  if (typeof s !== "string") fail(`table_locator_json(${p}) 回 ${typeof s}（應 string|undefined）`);
  hits.push(JSON.parse(s));
}
if (hits.length !== N) fail(`全 ${N} 段應全數落表內，實命中 ${hits.length}`);
const byRoot = (slot) => hits.find((h) => h.table_path.root_slot === slot);
if (!byRoot(0) || !byRoot(1) || !byRoot(2)) fail("三張根表都必須有段落命中");
const nested = hits.find((h) => h.table_path.nested_steps.length > 0);
if (!nested) fail("巢狀表段落必須反查出 nested_steps");
if (nested.table_path.root_slot !== 0 || nested.table_path.nested_steps[0].join(",") !== "0,2,0") {
  fail(`巢狀步漂移：${JSON.stringify(nested.table_path.nested_steps)}`);
}
if (nested.cell_path.depth !== 1 || nested.cell_path.grid_cols !== 2) fail("cell_path 欄位漂移");
ok(`游標反查：${hits.length} 段全命中、三根表對號、巢狀步 [0,2,0] 透傳（三元組經 glue 序列化無損）`);

// ---- 5) 越界口徑：undefined（不是 throw、不是 0） ----
{
  const oob = engine.table_locator_json(N + 999);
  if (oob !== undefined) fail(`越界反查應 undefined，實 ${typeof oob}`);
  const max = engine.table_locator_json(2 ** 53 - 1);
  if (max !== undefined) fail("JS 上限索引反查應 undefined（usize 邊界）");
  ok("越界索引→undefined（wasm 邊零 throw／零 panic）");
}

// ---- 6) 路徑→Mutation 直通：反查產物原樣餵 M4，真改到目標那張表 ----
{
  const t1 = byRoot(1);
  const res = JSON.parse(
    engine.apply_mutation_json(
      JSON.stringify({
        SetTableProps: { table: t1.table_path, patch: { alignment: "center" } },
      }),
    ),
  );
  if (!res.ok) fail(`M4.6 反查路徑直通失敗：${res.error}`);
  const after = JSON.parse(engine.tables_json());
  const centered = [0, 1, 2].map((i) => after[i]["table"]["props"]["justify"] === "center");
  if (centered.join(",") !== "false,true,false") {
    fail(`點哪張改哪張失敗（centered=${centered.join(",")}）`);
  }
  ok("table_path 原樣餵 apply_mutation_json：T1 中對齊、T0/T2 不受波及");
}

// ---- 7) 零回退：table_mixed（單表）四條綁定語義不變 ----
{
  const e2 = new WasmEngine(fs.readFileSync(path.join(ROOT, "fixtures/table_mixed.docx")));
  if (e2.table_count() !== 1) fail("table_mixed 計數回退");
  if (JSON.parse(e2.tables_json())[0]["table"]["grid"].length !== 3) fail("table_mixed 樹回退");
  if (!e2.page_display_list_json(0)) fail("顯示列表回退（本卡動同一 lib.rs）");
  ok("單表檔 table_mixed 零回退（計數/樹/顯示列表）");
}

console.log("\nE wasm 表格綁定 Node smoke：全綠");
