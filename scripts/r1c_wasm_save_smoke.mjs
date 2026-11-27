// R1-c 完成閘門 5：Node 直接 load 新建 wasm pkg，實調三個誠實存檔出口，
// 證明 wasm 出口真的接上（不只是 Rust 側有函式）。
// CI 用：先 `wasm-pack build --target web`（或 cargo build --release --target
// wasm32-unknown-unknown ＋ wasm-bindgen CLI），再 `node scripts/r1c_wasm_save_smoke.mjs`。
// 出口契約（考卷 core/engine-api/tests/r1c_save_honesty.rs 同口徑）：
//   save_docx_bytes()        → Uint8Array；失敗必 throw（不再吞成空陣列）
//   save_docx_checked_json() → {ok, bytes_len, error?, error_kind?}
//   save_docx_report_json()  → {ok, bytes_len, touched_parts, entry_total,
//                               preserved_part_count, roundtrip{...}}
//   supported_mutations_json() → [{variant}, ...]（只列有考題實證者）
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const fail = (msg) => {
  console.error(`✗ ${msg}`);
  process.exit(1);
};
const ok = (msg) => console.log(`✓ ${msg}`);

const initMain = (await import(path.join(ROOT, "shells/web/pkg/quire_web.js"))).default;
const { WasmEngine } = await import(path.join(ROOT, "shells/web/pkg/quire_web.js"));
const mainWasm = fs.readFileSync(path.join(ROOT, "shells/web/pkg/quire_web_bg.wasm"));
await initMain({ module_or_path: new Uint8Array(mainWasm) });
ok("wasm pkg 初始化（Node）");

const docBytes = fs.readFileSync(path.join(ROOT, "fixtures/style_cascade.docx"));
const engine = new WasmEngine(new Uint8Array(docBytes));

// ---- 1) checked 出口：零編輯態必須 ok:true ＋ 非零 bytes_len ----
const checked = JSON.parse(engine.save_docx_checked_json());
if (checked.ok !== true) fail(`零編輯 checked 應 ok:true，實際 ${JSON.stringify(checked)}`);
if (!(checked.bytes_len > 0)) fail(`零編輯 checked bytes_len 不得為 0: ${JSON.stringify(checked)}`);
ok(`checked 出口（零編輯）：ok:true, bytes_len=${checked.bytes_len}`);

// ---- 2) 編輯一筆後取證據：touched 只准 word/document.xml ----
engine.replace_paragraph_text(0, "R1c wasm 證據鏈 smoke");
const report = JSON.parse(engine.save_docx_report_json());
if (report.ok !== true) fail(`report 應 ok:true，實際 ${JSON.stringify(report)}`);
if (!Array.isArray(report.touched_parts) || !report.touched_parts.includes("word/document.xml"))
  fail(`touched_parts 應含 word/document.xml：${JSON.stringify(report.touched_parts)}`);
if (!(report.entry_total > 3)) fail(`entry_total 異常: ${report.entry_total}`);
if (report.preserved_part_count !== report.entry_total - report.touched_parts.filter((p) => report.roundtrip.whitelisted_changed.includes(p)).length)
  fail(`preserved_part_count 與 entry_total/whitelisted_changed 不自洽: ${JSON.stringify(report)}`);
if (report.roundtrip.pass !== true) fail(`roundtrip 必須 pass: ${JSON.stringify(report.roundtrip)}`);
if ((report.roundtrip.violations || []).length !== 0) fail(`未觸碰 entry 零失真違規非 0: ${JSON.stringify(report.roundtrip.violations)}`);
ok(
  `report 出口：重寫 ${report.touched_parts.join(",")}｜原位元組搬運 ${report.preserved_part_count}/${report.entry_total}｜roundtrip pass`,
);

// ---- 3) 純觀察鎖（Node 面）：report 前後 save_docx_bytes 逐位元組相等 ----
const before = engine.save_docx_bytes();
engine.save_docx_report_json();
engine.save_docx_report_json();
const after = engine.save_docx_bytes();
if (!(before instanceof Uint8Array) || before.length === 0) fail("save_docx_bytes 應回非空 Uint8Array");
if (Buffer.compare(Buffer.from(before), Buffer.from(after)) !== 0)
  fail("呼叫 report 前後 save_docx_bytes() 輸出不同（證據不是純觀察）");
ok(`純觀察鎖：report 前後的存檔輸出逐位元組相等（${before.length} bytes）`);

// ---- 4) 能力清單出口：陣列、非空、每項帶 variant、且含 InsertText ----
const caps = JSON.parse(WasmEngine.supported_mutations_json());
if (!Array.isArray(caps) || caps.length < 30) fail(`能力清單異常（${caps.length} 條）`);
if (!caps.some((c) => c.variant === "InsertText")) fail("能力清單缺 InsertText");
if (caps.some((c) => c.variant === "SplitTableCell" || c.variant === "InsertFloatingImage"))
  fail("能力清單混入查無非 ignore 考題的 variant（吹噓未驗證能力）");
ok(`能力清單出口：${caps.length} 條已實證能力（單源 build.rs，含 InsertText、無 SplitTableCell）`);

console.log("\nR1-c wasm smoke：全部通過");
