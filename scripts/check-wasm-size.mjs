#!/usr/bin/env node
// Web-2 包體積閘門：記錄＋閘死「首包小、協作延遲」。
//
// 檢查（任何一條紅即非零離開碼）：
//   1) 首包 quire_web_bg.wasm：gzip 必須 < MAIN_GZIP_BUDGET（3 MB 硬頂）。
//   2) 拆包真實性：首包 wasm 導出符號**不得**出現 CRDT 符號；協作分塊**必須**有。
//      （防「假拆包」——把 CRDT 打回主包只為少一個檔案。）
//   3) 協作分塊存在且有上限（避免拖車式長大）。
//   4) vite 產物（demo/dist）：入口 chunk gzip 記錄，CRDT 必須在獨立 chunk。
// 全部體積以 Markdown 表輸出（CI log 可直接讀）；--json <path> 另存機器可讀紀錄。
import { readFileSync, readdirSync, statSync, existsSync, writeFileSync, mkdirSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const WEB = join(ROOT, "shells/web");
const COLLAB_PKG = join(ROOT, "shells/web-collab/pkg");
const MAIN_WASM = join(WEB, "pkg/quire_web_bg.wasm");
const MAIN_JS = join(WEB, "pkg/quire_web.js");
const COLLAB_WASM = join(COLLAB_PKG, "quire_web_collab_bg.wasm");
const COLLAB_JS = join(COLLAB_PKG, "quire_web_collab.js");
const DIST = join(WEB, "demo/dist");

const MAIN_GZIP_BUDGET = 3 * 1024 * 1024; // gzip < 3MB（任務門檻）
const COLLAB_GZIP_BUDGET = 1.5 * 1024 * 1024; // 分塊不許無序長大（crdt+loro 餘量）
 

const failures = [];
const rows = [];
function kb(n) { return (n / 1024).toFixed(1); }
function measure(path, label, kind) {
  const raw = readFileSync(path);
  const gz = gzipSync(raw, { level: 9 }).length;
  rows.push({ kind, label, raw: raw.length, gzip: gz });
  return { raw, gz };
}

if (!existsSync(MAIN_WASM) || !existsSync(COLLAB_WASM)) {
  console.error("✗ 找不到 wasm 產物——先跑 npm run build:wasm（shells/web）");
  process.exit(1);
}

// 1) 首包體積閘門
const main = measure(MAIN_WASM, "首包 wasm（quire_web_bg.wasm）", "main-wasm");
measure(MAIN_JS, "首包 glue（quire_web.js）", "main-js");
if (main.gz >= MAIN_GZIP_BUDGET) {
  failures.push(`首包 gzip ${kb(main.gz)} KB ≥ 預算 ${kb(MAIN_GZIP_BUDGET)} KB——首包膨脹，查是不是把重活打回主包`);
}

// 2) 拆包真實性：符號級雙向檢查（CRDT 只准活在協作分塊）
//    wasm 二進位查運算符號；glue JS 查導出類名（WasmCollab 只在 glue）。
const CRDT_WASM_MARKERS = ["commit_local", "apply_remote", "ReplaceRunText", "lamport"];
const mainBytes = readFileSync(MAIN_WASM);
const collabBytes = readFileSync(COLLAB_WASM);
for (const m of CRDT_WASM_MARKERS) {
  if (mainBytes.includes(m)) {
    failures.push(`假拆包：首包 wasm 含 CRDT 符號「${m}」`);
  }
  if (!collabBytes.includes(m)) {
    failures.push(`協作分塊缺 CRDT 符號「${m}」——分塊內容不對或符號被洗掉`);
  }
}
if (!readFileSync(COLLAB_JS).includes("WasmCollab")) {
  failures.push("協作分塊 glue 缺 WasmCollab 導出——分塊契約破損");
}
if (readFileSync(MAIN_JS).includes("WasmCollab")) {
  failures.push("假拆包：首包 glue 含 WasmCollab 導出");
}

// 3) 協作分塊上限
const collab = measure(COLLAB_WASM, "協作分塊 wasm（quire_web_collab_bg.wasm）", "collab-wasm");
measure(COLLAB_JS, "協作分塊 glue（quire_web_collab.js）", "collab-js");
if (collab.gz >= COLLAB_GZIP_BUDGET) {
  failures.push(`協作分塊 gzip ${kb(collab.gz)} KB ≥ 預算 ${kb(COLLAB_GZIP_BUDGET)} KB`);
}

// 4) vite 產物（有 dist 才查；CI 在 vite build 後跑，本體積必在）
let viteChecked = false;
if (existsSync(DIST)) {
  viteChecked = true;
  const assets = [];
  const walk = (d) => readdirSync(d).forEach((f) => {
    const p = join(d, f);
    if (statSync(p).isDirectory()) walk(p); else assets.push(p);
  });
  walk(DIST);
  const crdtChunks = [];
  for (const p of assets.filter((f) => /\.(js|css|wasm)$/.test(f))) {
    const m = measure(p, `vite: ${p.slice(DIST.length + 1)}`, "vite");
    const txt = readFileSync(p);
    // 只用 wasm 二進位級符號判定：入口 chunk 必然含「quire_web_collab」動態
    // import 字串與 glue 參考，那只是載入器；CRDT 程式碼本體在 wasm 裡。
    if (/\.wasm$/.test(p) && CRDT_WASM_MARKERS.some((mk) => txt.includes(mk))) {
      crdtChunks.push(p.slice(DIST.length + 1));
    }
  }
  // 協作 wasm 必須存在於 dist（被動態 import 拆出去的證據）。
  if (!crdtChunks.length) {
    failures.push("vite：dist 找不到含 CRDT 符號的 wasm——協作分塊沒被打進來或符號被洗掉");
  }
  // 入口 HTML 引用的 modulepreload 不得把協作 chunk 拉進首屏——
  // 簡化判準：CRDT 只准出現在「非 main 入口」的 chunk 名裡（quire_web_collab 或 async chunk）。
  const entryHtml = readFileSync(join(DIST, "index.html"), "utf8");
  for (const c of crdtChunks) {
    if (entryHtml.includes(c) && !c.includes("collab")) {
      failures.push(`vite：協作程式碼落在預載 chunk「${c}」——拆包被打包器併回首屏`);
    }
  }
}

// ---- 報告（CI 記錄包體積）----
const totalGz = rows.filter((r) => r.kind === "main-wasm" || r.kind === "collab-wasm").reduce((s, r) => s + r.gzip, 0);
console.log("| 產物 | raw | gzip |");
console.log("|---|---:|---:|");
for (const r of rows) console.log(`| ${r.label} | ${kb(r.raw)} KB | ${kb(r.gzip)} KB |`);
console.log(`\n首包+協作 wasm gzip 合計：${kb(totalGz)} KB（首包預算 ${kb(MAIN_GZIP_BUDGET)} KB）`);
console.log(`拆包符號檢查：${failures.some((f) => f.includes("拆包")) ? "✗" : "✓"} CRDT 僅在協作分塊${viteChecked ? "；vite 產物已查" : "（未找到 demo/dist——跳過 vite 檢查，CI 會跑 vite build）"}`);

const jsonPath = process.argv.indexOf("--json");
if (jsonPath >= 0 && process.argv[jsonPath + 1]) {
  const out = process.argv[jsonPath + 1];
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, JSON.stringify({ at: new Date().toISOString(), rows, failures }, null, 2));
}

if (failures.length) {
  console.error("\n" + failures.map((f) => `✗ ${f}`).join("\n"));
  process.exit(1);
}
console.log("✓ 包體積閘門通過");
