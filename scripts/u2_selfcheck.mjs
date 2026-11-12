#!/usr/bin/env node
/**
 * u2_selfcheck.mjs
 * AGY-U2: 編輯器接線——真引擎進外殼（wasm 載入＋打字閉環）自動化驗收與自檢腳本
 *
 * 驗收標準：
 * 1. 必要外殼與 wasm 產物存在性檢查
 * 2. 零外部網路依賴檢查（HTML 與 CSS 離線自足）
 * 3. Mutation JSON 結構與建構器契約測試 (InsertText / DeleteRange / InsertBreak)
 * 4. 指令映射表與 pending -> live 清單一致性審查 (含 K29a/K29f 轉 live，B/I/U 維持 pending)
 * 5. 無 DOM 純邏輯層 WasmEngine 執行閉環：
 *    - wasm 載入與初始化
 *    - DOCX 開檔 (openFile / openDefaultSample)
 *    - 游標反查與幾何換算 (caret_at_json / caret_rect_json)
 *    - 打字輸入 (M1.1 InsertText)
 *    - 退格鍵 (M1.2 DeleteRange)
 *    - 斷點換行 (M1.5 InsertBreak)
 *    - 復原與重做 (undo / redo)
 *    - 存檔 (save_docx_bytes)
 * 6. 輸出 PASS / FAIL
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

const HTML_PATH = path.join(ROOT_DIR, 'shells/web/editor/index.html');
const CSS_PATH = path.join(ROOT_DIR, 'shells/web/editor/css/style.css');
const CORE_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/qword-core.js');
const UI_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/qword-ui.js');
const ADAPTER_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/engine-adapter.js');
const RENDERER_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/vendor/renderer.js');
const WASM_JS_PATH = path.join(ROOT_DIR, 'shells/web/pkg/quire_web.js');
const WASM_BIN_PATH = path.join(ROOT_DIR, 'shells/web/pkg/quire_web_bg.wasm');
const LIB_RS_PATH = path.join(ROOT_DIR, 'shells/web/src/lib.rs');

let pass = true;
const errors = [];
const warnings = [];

function assert(condition, message) {
  if (!condition) {
    errors.push(message);
    pass = false;
  }
}

console.log('='.repeat(70));
console.log(' AGY-U2 真引擎進外殼（Wasm 載入＋打字閉環）自動化驗收檢查');
console.log('='.repeat(70));

// 1. 檔案存在性檢查
console.log('\n[檢查 1] 驗證必要外殼檔案與 Wasm 產物存在性...');
const requiredFiles = [
  { path: LIB_RS_PATH, desc: 'Wasm 殼導出層 (shells/web/src/lib.rs)' },
  { path: WASM_JS_PATH, desc: 'Wasm-pack JS 產物 (shells/web/pkg/quire_web.js)' },
  { path: WASM_BIN_PATH, desc: 'Wasm-pack 二進位檔 (shells/web/pkg/quire_web_bg.wasm)' },
  { path: ADAPTER_JS_PATH, desc: '引擎適配器 (shells/web/editor/js/engine-adapter.js)' },
  { path: CORE_JS_PATH, desc: '核心邏輯層 (shells/web/editor/js/qword-core.js)' },
  { path: UI_JS_PATH, desc: 'UI 綁定層 (shells/web/editor/js/qword-ui.js)' },
  { path: RENDERER_JS_PATH, desc: 'Vendor 繪製器 (shells/web/editor/vendor/renderer.js)' },
  { path: HTML_PATH, desc: '外殼頁面 (shells/web/editor/index.html)' },
  { path: CSS_PATH, desc: '樣式表 (shells/web/editor/css/style.css)' }
];

for (const file of requiredFiles) {
  const exists = fs.existsSync(file.path);
  assert(exists, `缺少關鍵檔案: ${file.desc}`);
  console.log(`  ${exists ? '✓' : '✗'} ${file.desc}`);
}

if (!pass) {
  console.error('\n自檢失敗：缺少關鍵檔案，停止後續檢查。');
  console.error(errors.join('\n'));
  process.exit(1);
}

// 2. 檢驗零外部網路依賴
console.log('\n[檢查 2] 驗證零外部網路依賴...');
const htmlContent = fs.readFileSync(HTML_PATH, 'utf-8');
const cssContent = fs.readFileSync(CSS_PATH, 'utf-8');

const networkPatterns = [
  /https?:\/\//i,
  /\/\//i,
  /@import\s+url/i,
  /<link[^>]+href=["'](http|\/\/)/i,
  /<script[^>]+src=["'](http|\/\/)/i
];

let foundNetworkInHtml = false;
for (const line of htmlContent.split('\n')) {
  if (/xmlns="http:\/\/www\.w3\.org\/2000\/svg"/.test(line)) continue;
  for (const pat of networkPatterns) {
    if (pat.test(line)) {
      foundNetworkInHtml = true;
      errors.push(`HTML 含有外部網路連結: ${line.trim()}`);
      pass = false;
    }
  }
}
assert(!foundNetworkInHtml, 'HTML 包含外部網路依賴');
console.log(`  ${foundNetworkInHtml ? '✗' : '✓'} HTML 零外部網路依賴`);

let foundNetworkInCss = false;
for (const line of cssContent.split('\n')) {
  for (const pat of networkPatterns) {
    if (pat.test(line)) {
      foundNetworkInCss = true;
      errors.push(`CSS 含有外部網路連結: ${line.trim()}`);
      pass = false;
    }
  }
}
assert(!foundNetworkInCss, 'CSS 包含外部網路依賴');
console.log(`  ${foundNetworkInCss ? '✗' : '✓'} CSS 零外部網路依賴`);

// 3. Mutation JSON 結構與工廠函數驗證
console.log('\n[檢查 3] 驗證 Mutation JSON 結構與工廠函數契約...');
const {
  EngineAdapter,
  buildInsertTextMutation,
  buildDeleteRangeMutation,
  buildInsertBreakMutation,
  LIVE_COMMANDS,
  TRANSFERRED_LIVE_COMMANDS,
  getDefaultSampleBytes
} = await import(ADAPTER_JS_PATH);

// 測試 M1.1 InsertText 結構
const insertOp = buildInsertTextMutation(2, 5, '測試打字');
assert(insertOp.InsertText !== undefined, 'InsertText 頂層鍵存在');
assert(insertOp.InsertText.target === 'Body', 'InsertText 目標為 Body');
assert(insertOp.InsertText.pos.para === 2 && insertOp.InsertText.pos.cp === 5, 'InsertText 座標正確');
assert(insertOp.InsertText.text === '測試打字', 'InsertText 文本正確');
console.log('  ✓ M1.1 InsertText Mutation JSON 符合引擎 serde 契約');

// 測試 M1.2 DeleteRange 結構
const deleteOp = buildDeleteRangeMutation(1, 3, 4);
assert(deleteOp.DeleteRange !== undefined, 'DeleteRange 頂層鍵存在');
assert(deleteOp.DeleteRange.range.target === 'Body', 'DeleteRange 目標為 Body');
assert(deleteOp.DeleteRange.range.start.para === 1 && deleteOp.DeleteRange.range.start.cp === 3, 'DeleteRange start 座標正確');
assert(deleteOp.DeleteRange.range.end.para === 1 && deleteOp.DeleteRange.range.end.cp === 4, 'DeleteRange end 座標正確');
console.log('  ✓ M1.2 DeleteRange Mutation JSON 符合引擎 serde 契約');

// 測試 M1.5 InsertBreak 結構
const breakOp = buildInsertBreakMutation(0, 8, 'LineWrapping');
assert(breakOp.InsertBreak !== undefined, 'InsertBreak 頂層鍵存在');
assert(breakOp.InsertBreak.target === 'Body', 'InsertBreak 目標為 Body');
assert(breakOp.InsertBreak.pos.para === 0 && breakOp.InsertBreak.pos.cp === 8, 'InsertBreak 座標正確');
assert(breakOp.InsertBreak.kind === 'LineWrapping', 'InsertBreak kind 正確');
console.log('  ✓ M1.5 InsertBreak Mutation JSON 符合引擎 serde 契約');

// 4. 指令映射表與 pending -> live 審查
console.log('\n[檢查 4] 審查 pending -> live 轉移清單與按鈕標記誠實性...');
assert(Array.isArray(LIVE_COMMANDS) && LIVE_COMMANDS.length >= 20, 'LIVE_COMMANDS 清單完整');
assert(Array.isArray(TRANSFERRED_LIVE_COMMANDS), 'TRANSFERRED_LIVE_COMMANDS 清單存在');
console.log(`  - 本卡轉為 live 的指令清單: ${TRANSFERRED_LIVE_COMMANDS.join(', ')}`);

// 審查 index.html 中的轉移按鈕
const saveBtnMatch = htmlContent.match(/<button[^>]+data-command="file:save"[^>]*>/i);
assert(saveBtnMatch && /data-requirement="live"/i.test(saveBtnMatch[0]), 'file:save 按鈕已正確標記為 live');
console.log('  ✓ 按鈕 [file:save] 標記為 live (接 save_docx_bytes)');

const undoBtnMatch = htmlContent.match(/<button[^>]+data-command="edit:undo"[^>]*>/i);
assert(undoBtnMatch && /data-requirement="live"/i.test(undoBtnMatch[0]), 'edit:undo 按鈕已正確標記為 live');
console.log('  ✓ 按鈕 [edit:undo] 標記為 live (接 undo_json)');

const redoBtnMatch = htmlContent.match(/<button[^>]+data-command="edit:redo"[^>]*>/i);
assert(redoBtnMatch && /data-requirement="live"/i.test(redoBtnMatch[0]), 'edit:redo 按鈕已正確標記為 live');
console.log('  ✓ 按鈕 [edit:redo] 標記為 live (接 redo_json)');

// 嚴格紅線檢查：粗斜底、字級等仍須為 pending，不準假裝支援
const boldBtnMatch = htmlContent.match(/<button[^>]+data-command="format:bold"[^>]*>/i);
assert(boldBtnMatch && /data-requirement="pending:K29a"/i.test(boldBtnMatch[0]), 'format:bold 誠實維持 pending:K29a');
console.log('  ✓ 按鈕 [format:bold] 誠實維持 pending:K29a（不假裝可用）');

const italicBtnMatch = htmlContent.match(/<button[^>]+data-command="format:italic"[^>]*>/i);
assert(italicBtnMatch && /data-requirement="pending:K29a"/i.test(italicBtnMatch[0]), 'format:italic 誠實維持 pending:K29a');
console.log('  ✓ 按鈕 [format:italic] 誠實維持 pending:K29a（不假裝可用）');

const underlineBtnMatch = htmlContent.match(/<button[^>]+data-command="format:underline"[^>]*>/i);
assert(underlineBtnMatch && /data-requirement="pending:K29a"/i.test(underlineBtnMatch[0]), 'format:underline 誠實維持 pending:K29a');
console.log('  ✓ 按鈕 [format:underline] 誠實維持 pending:K29a（不假裝可用）');

// 5. 無 DOM 純邏輯層 WasmEngine 執行閉環測試
console.log('\n[檢查 5] 探針驗證：Node.js ESM 執行 WasmEngine 打字閉環...');

try {
  const adapter = new EngineAdapter();
  await adapter.init();
  console.log('  ✓ WebAssembly 模組初始化成功');

  // 開檔測試
  const sampleBytes = getDefaultSampleBytes();
  adapter.openFile(sampleBytes, 'sample.docx');
  const pages = adapter.pageCount();
  const paras = adapter.paragraphCount();
  const words = adapter.getWordCount();
  assert(pages >= 1, `頁數必須大於等於 1（實測: ${pages}）`);
  assert(paras >= 1, `段落數必須大於等於 1（實測: ${paras}）`);
  assert(words > 0, `字數統計必須大於 0（實測: ${words}）`);
  console.log(`  ✓ DOCX 開檔成功：頁數=${pages}，段落數=${paras}，總字數=${words}`);

  // 游標定位反查測試
  const rect0 = adapter.caretRect(0, 0);
  assert(rect0 && typeof rect0.x_pt === 'number' && typeof rect0.y_pt === 'number', 'caret_rect_json(0, 0) 回傳合法座標');
  const pos0 = adapter.caretAt(rect0.page, rect0.x_pt + 1, rect0.y_pt + 1);
  assert(pos0 && pos0.para === 0 && pos0.cp === 0, 'caret_at_json 游標反查對應 (0, 0)');
  console.log(`  ✓ 游標幾何定位閉環驗證通過 (para=0, cp=0 -> page=${rect0.page}, x=${rect0.x_pt}, y=${rect0.y_pt})`);

  // 打字輸入測試 (M1.1)
  const initialText = adapter.paragraphText(0);
  const typeRes = adapter.typeAt(0, 0, 'QuireU2Test_');
  assert(typeRes.ok === true, 'typeAt 執行成功');
  assert(adapter.caret.cp === 12, '打字後游標位移至 cp=12');
  const textAfterType = adapter.paragraphText(0);
  assert(textAfterType.startsWith('QuireU2Test_'), '段落文字成功寫入真引擎');
  console.log(`  ✓ M1.1 打字閉環成功：插入 12 字元，游標位移至 cp=${adapter.caret.cp}`);

  // 退格鍵測試 (M1.2)
  const bsRes = adapter.backspace();
  assert(bsRes.ok === true, 'backspace 執行成功');
  assert(adapter.caret.cp === 11, '退格後游標退回至 cp=11');
  const textAfterBs = adapter.paragraphText(0);
  assert(textAfterBs.startsWith('QuireU2Test'), '退格後末尾底線成功刪除');
  console.log(`  ✓ M1.2 退格閉環成功：刪除 1 字元，游標位移至 cp=${adapter.caret.cp}`);

  // 斷點換行測試 (M1.5)
  const enterRes = adapter.enter();
  assert(enterRes.ok === true, 'enter 執行成功');
  console.log('  ✓ M1.5 軟換行 (LineWrapping) 執行成功');

  // 復原測試 (Undo)
  const undoRes = adapter.undo();
  assert(undoRes.ok === true && undoRes.undone === true, 'undo 執行成功');
  console.log('  ✓ 復原 (Undo) 成功還原最新操作');

  // 重做測試 (Redo)
  const redoRes = adapter.redo();
  assert(redoRes.ok === true && redoRes.redone === true, 'redo 執行成功');
  console.log('  ✓ 重做 (Redo) 成功重做操作');

  // 存檔測試 (Save)
  const savedBytes = adapter.save('output.docx');
  assert(savedBytes && savedBytes.length > 0, 'save_docx_bytes 回傳有效位元組');
  assert(savedBytes[0] === 0x50 && savedBytes[1] === 0x4b, '存檔位元組包含標準 ZIP (PK) 標頭');
  console.log(`  ✓ 存檔閉環驗證通過：匯出 ${savedBytes.length} 位元組，合規 DOCX 容器`);

} catch (err) {
  errors.push(`WasmEngine 執行閉環發生例外: ${err.message}\n${err.stack}`);
  pass = false;
}

// 6. 總結報告與 PASS / FAIL 判定
console.log('\n' + '='.repeat(70));
if (warnings.length > 0) {
  console.log('注意事項 / 警告:');
  warnings.forEach(w => console.log('  [WARN] ' + w));
  console.log('-'.repeat(70));
}

if (pass) {
  console.log(' 自檢結果: PASS');
  console.log(' 恭喜！AGY-U2 真引擎接線與打字閉環通過所有規格與紅線檢查。');
  console.log('='.repeat(70));
  process.exit(0);
} else {
  console.error(' 自檢結果: FAIL');
  console.error(' 發現以下不符合規格問題：');
  errors.forEach(e => console.error('  [ERROR] ' + e));
  console.error('='.repeat(70));
  process.exit(1);
}
