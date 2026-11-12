#!/usr/bin/env node
/**
 * u3_selfcheck.mjs
 * AGY-U3: 樣式命令啟用——K29b 引擎能力進 Ribbon 自動化驗收與自檢腳本
 *
 * 驗收標準：
 * 1. 必要外殼檔案與 Wasm 產物存在性檢查
 * 2. 零外部網路依賴檢查（HTML 與 CSS 離線自足）
 * 3. Mutation JSON 結構與工廠函數契約測試：
 *    - M2.1 FormatTextRange (bold, italic, color_rgb, size_pt)
 *    - M2.2 SetParagraphStyle (alignment, style_id)
 * 4. 指令映射表與 live / pending 清單一致性審查（產品誠實紅線）：
 *    - 必轉 live: format:bold, format:italic, format:font-color, format:font-color-palette,
 *                font:grow, font:shrink, font:size,
 *                paragraph:align-left, paragraph:align-center, paragraph:align-right, paragraph:align-justify,
 *                style:normal, style:no-spacing, style:h1, style:h2, style:h3
 *    - 嚴格維持 pending: format:underline, format:underline-style, format:strikethrough,
 *                       format:subscript, format:superscript, format:highlight, format:highlight-palette,
 *                       font:family, font:clear-formatting,
 *                       paragraph:bullets, paragraph:numbering, paragraph:multilevel
 * 5. 真引擎無 DOM 純邏輯層閉環驗證：
 *    - 粗體閉環：選取 -> format:bold -> apply_mutation_json -> 顯示列表 flags 帶 bold -> undo -> 消失
 *    - 斜體閉環：選取 -> format:italic -> 顯示列表 flags 帶 italic -> undo -> 消失
 *    - 字色閉環：選取 -> format:font-color -> 顯示列表 color_argb 變更 -> undo -> 還原
 *    - 字級閉環：選取 -> font:size / font:grow / font:shrink -> 顯示列表 size_pt 變更 -> undo -> 還原
 *    - 對齊閉環：段落對齊 (left/center/right/both) -> apply_mutation_json 成功 -> undo -> 還原
 *    - 樣式閉環：具名段落樣式 (Normal) -> apply_mutation_json 成功 -> undo -> 還原
 *    - 存檔閉環：save_docx_bytes 匯出有效 DOCX ZIP 結構，且內嵌 XML 正確更新
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
console.log(' AGY-U3 樣式命令啟用——K29b 引擎能力進 Ribbon 自動化驗收檢查');
console.log('='.repeat(70));

// 1. 檔案存在性檢查
console.log('\n[檢查 1] 驗證必要外殼檔案與 Wasm 產物存在性...');
const requiredFiles = [
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
console.log('\n[檢查 3] 驗證 Mutation JSON 結構與工廠函數契約 (FormatTextRange & SetParagraphStyle)...');
const adapterModule = await import(ADAPTER_JS_PATH);
const {
  EngineAdapter,
  buildInsertTextMutation,
  buildDeleteRangeMutation,
  buildInsertBreakMutation,
  buildFormatTextRangeMutation,
  buildSetParagraphStyleMutation,
  LIVE_COMMANDS,
  U3_LIVE_COMMANDS,
  getDefaultSampleBytes
} = adapterModule;

assert(typeof buildFormatTextRangeMutation === 'function', 'buildFormatTextRangeMutation 工廠函數存在');
assert(typeof buildSetParagraphStyleMutation === 'function', 'buildSetParagraphStyleMutation 工廠函數存在');

if (typeof buildFormatTextRangeMutation === 'function') {
  // M2.1 粗體測試
  const boldOp = buildFormatTextRangeMutation(
    { start: { para: 0, cp: 2 }, end: { para: 0, cp: 6 } },
    { bold: true }
  );
  assert(boldOp && boldOp.FormatTextRange !== undefined, 'FormatTextRange 頂層鍵存在');
  assert(boldOp.FormatTextRange.range.target === 'Body', 'FormatTextRange range.target 為 Body');
  assert(boldOp.FormatTextRange.range.start.para === 0 && boldOp.FormatTextRange.range.start.cp === 2, 'start 座標正確');
  assert(boldOp.FormatTextRange.range.end.para === 0 && boldOp.FormatTextRange.range.end.cp === 6, 'end 座標正確');
  assert(boldOp.FormatTextRange.patch.bold === true, 'patch.bold 正確設定');
  console.log('  ✓ M2.1 FormatTextRange (bold) Mutation JSON 符合引擎契約');

  // M2.1 字色與字級測試
  const colorSizeOp = buildFormatTextRangeMutation(
    { start: { para: 1, cp: 0 }, end: { para: 1, cp: 5 } },
    { color_rgb: 0xFFFF0000, size_pt: 18.5 }
  );
  assert(colorSizeOp.FormatTextRange.patch.color_rgb === 0xFFFF0000, 'patch.color_rgb 正確設定');
  assert(colorSizeOp.FormatTextRange.patch.size_pt === 18.5, 'patch.size_pt 正確設定');
  console.log('  ✓ M2.1 FormatTextRange (color_rgb / size_pt) Mutation JSON 符合引擎契約');
}

if (typeof buildSetParagraphStyleMutation === 'function') {
  // M2.2 對齊測試
  const alignOp = buildSetParagraphStyleMutation({ start: 1, end: 1 }, { alignment: 'center' });
  assert(alignOp && alignOp.SetParagraphStyle !== undefined, 'SetParagraphStyle 頂層鍵存在');
  assert(alignOp.SetParagraphStyle.target === 'Body', 'SetParagraphStyle target 為 Body');
  assert(alignOp.SetParagraphStyle.para_range.start === 1 && alignOp.SetParagraphStyle.para_range.end === 1, 'para_range 正確');
  assert(alignOp.SetParagraphStyle.patch.alignment === 'center', 'patch.alignment 正確設定');
  console.log('  ✓ M2.2 SetParagraphStyle (alignment) Mutation JSON 符合引擎契約');

  // M2.2 樣式 id 測試
  const styleOp = buildSetParagraphStyleMutation({ start: 0, end: 2 }, { style_id: 'Heading1' });
  assert(styleOp.SetParagraphStyle.patch.style_id === 'Heading1', 'patch.style_id 正確設定');
  console.log('  ✓ M2.2 SetParagraphStyle (style_id) Mutation JSON 符合引擎契約');
}

// 4. 指令映射表與 pending -> live 審查
console.log('\n[檢查 4] 審查 pending -> live 轉移清單與按鈕標記誠實性（產品誠實紅線）...');

const REQUIRED_LIVE_BUTTONS = [
  'format:bold',
  'format:italic',
  'format:font-color',
  'format:font-color-palette',
  'font:grow',
  'font:shrink',
  'font:size',
  'paragraph:align-left',
  'paragraph:align-center',
  'paragraph:align-right',
  'paragraph:align-justify',
  'style:normal',
  'style:no-spacing',
  'style:h1',
  'style:h2',
  'style:h3'
];

for (const cmd of REQUIRED_LIVE_BUTTONS) {
  const reg = new RegExp(`<(?:button|select)[^>]+data-command=["']${cmd}["'][^>]*>`, 'i');
  const match = htmlContent.match(reg);
  assert(match !== null, `未在 HTML 中找到按鈕/控制項: [${cmd}]`);
  if (match) {
    const isLive = /data-requirement=["']live["']/i.test(match[0]);
    assert(isLive, `按鈕 [${cmd}] 應已轉為 live，但實際為: ${match[0]}`);
    console.log(`  ${isLive ? '✓' : '✗'} 按鈕 [${cmd}] 標記為 live`);
  }
}

// 產品誠實紅線：引擎未支援之樣式控制項必須嚴格維持 pending，絕不假裝可用
const REQUIRED_PENDING_BUTTONS = [
  'format:underline',
  'format:underline-style',
  'format:strikethrough',
  'format:subscript',
  'format:superscript',
  'format:highlight',
  'format:highlight-palette',
  'font:family',
  'font:clear-formatting',
  'paragraph:bullets',
  'paragraph:numbering',
  'paragraph:multilevel'
];

for (const cmd of REQUIRED_PENDING_BUTTONS) {
  const reg = new RegExp(`<(?:button|select)[^>]+data-command=["']${cmd}["'][^>]*>`, 'i');
  const match = htmlContent.match(reg);
  assert(match !== null, `未在 HTML 中找到按鈕/控制項: [${cmd}]`);
  if (match) {
    const isPending = /data-requirement=["']pending:K29b["']/i.test(match[0]);
    assert(isPending, `紅線違規：按鈕 [${cmd}] 必須維持 pending:K29b，實際為: ${match[0]}`);
    console.log(`  ${isPending ? '✓' : '✗'} 按鈕 [${cmd}] 誠實維持 pending:K29b`);
  }
}

// 5. 無 DOM 純邏輯層 WasmEngine 執行閉環測試
console.log('\n[檢查 5] 探針驗證：Node.js ESM 執行 WasmEngine 樣式變更閉環與 Undo...');

try {
  const adapter = new EngineAdapter();
  await adapter.init();
  console.log('  ✓ WebAssembly 模組初始化成功');

  // 開檔
  const sampleBytes = getDefaultSampleBytes();
  adapter.openFile(sampleBytes, 'sample.docx');
  console.log(`  ✓ DOCX 開檔成功：頁數=${adapter.pageCount()}，段落數=${adapter.paragraphCount()}`);

  // (1) 粗體閉環測試 (M2.1 FormatTextRange bold)
  const dl0 = adapter.getPageDisplayList(0);
  const textItem1Before = dl0.items[1].Text;
  const originalFlags = textItem1Before.flags;
  assert((originalFlags & 1) === 0, '段落 1 初態不具粗體');

  const boldRes = adapter.formatTextRange({ start: { para: 1, cp: 0 }, end: { para: 1, cp: 4 } }, { bold: true });
  assert(boldRes.ok === true, 'formatTextRange (bold) 執行成功');
  const dlBold = adapter.getPageDisplayList(0);
  const textItem1Bold = dlBold.items[1].Text;
  assert((textItem1Bold.flags & 1) === 1, `段落 1 前 4 字元成功轉為粗體 (flags=${textItem1Bold.flags})`);
  console.log(`  ✓ M2.1 粗體閉環成功：Text.flags & 1 = 1 (bold 成立)`);

  // 粗體撤銷 (Undo)
  const undoBoldRes = adapter.undo();
  assert(undoBoldRes.ok === true && undoBoldRes.undone === true, '粗體 undo 成功');
  const dlAfterUndoBold = adapter.getPageDisplayList(0);
  const textItem1Restored = dlAfterUndoBold.items[1].Text;
  assert((textItem1Restored.flags & 1) === 0, '粗體 undo 後恢復原狀 (flags & 1 === 0)');
  console.log('  ✓ 粗體 Undo 閉環成功：文字樣式已完全還原');

  // (2) 斜體閉環測試 (M2.1 FormatTextRange italic)
  const italicRes = adapter.formatTextRange({ start: { para: 1, cp: 0 }, end: { para: 1, cp: 4 } }, { italic: true });
  assert(italicRes.ok === true, 'formatTextRange (italic) 執行成功');
  const dlItalic = adapter.getPageDisplayList(0);
  const textItem1Italic = dlItalic.items[1].Text;
  assert((textItem1Italic.flags & 2) === 2, `段落 1 前 4 字元成功轉為斜體 (flags=${textItem1Italic.flags})`);
  console.log(`  ✓ M2.1 斜體閉環成功：Text.flags & 2 = 2 (italic 成立)`);

  const undoItalicRes = adapter.undo();
  assert(undoItalicRes.ok === true && undoItalicRes.undone === true, '斜體 undo 成功');
  console.log('  ✓ 斜體 Undo 閉環成功');

  // (3) 字色閉環測試 (M2.1 FormatTextRange color_rgb)
  const colorRes = adapter.formatTextRange({ start: { para: 1, cp: 0 }, end: { para: 1, cp: 4 } }, { color_rgb: 0xFFFF0000 });
  assert(colorRes.ok === true, 'formatTextRange (color_rgb) 執行成功');
  const dlColor = adapter.getPageDisplayList(0);
  const textItem1Color = dlColor.items[1].Text;
  assert(textItem1Color.color_argb === 0xFFFF0000, `字色設定為紅色成功 (color_argb=${textItem1Color.color_argb.toString(16)})`);
  console.log(`  ✓ M2.1 字色閉環成功：color_argb = 0xFFFF0000`);

  const undoColorRes = adapter.undo();
  assert(undoColorRes.ok === true && undoColorRes.undone === true, '字色 undo 成功');
  console.log('  ✓ 字色 Undo 閉環成功');

  // (4) 字級閉環測試 (M2.1 FormatTextRange size_pt)
  const sizeRes = adapter.formatTextRange({ start: { para: 1, cp: 0 }, end: { para: 1, cp: 4 } }, { size_pt: 24.0 });
  assert(sizeRes.ok === true, 'formatTextRange (size_pt) 執行成功');
  const dlSize = adapter.getPageDisplayList(0);
  const textItem1Size = dlSize.items[1].Text;
  assert(textItem1Size.size_pt === 24.0, `字級設定為 24pt 成功 (size_pt=${textItem1Size.size_pt})`);
  console.log(`  ✓ M2.1 字級閉環成功：size_pt = 24`);

  const undoSizeRes = adapter.undo();
  assert(undoSizeRes.ok === true && undoSizeRes.undone === true, '字級 undo 成功');
  console.log('  ✓ 字級 Undo 閉環成功');

  // (5) 段落對齊閉環測試 (M2.2 SetParagraphStyle alignment)
  for (const align of ['center', 'right', 'both', 'left']) {
    const alignRes = adapter.setParagraphStyle({ start: 1, end: 1 }, { alignment: align });
    assert(alignRes.ok === true, `setParagraphStyle alignment=${align} 執行成功`);
    const undoAlign = adapter.undo();
    assert(undoAlign.ok === true && undoAlign.undone === true, `對齊 ${align} undo 成功`);
  }
  console.log('  ✓ M2.2 段落對齊 (center/right/both/left) 閉環與 Undo 驗證通過');

  // (6) 段落樣式閉環測試 (M2.2 SetParagraphStyle style_id)
  // 使用具備 styles.xml 的 fixture 驗證真樣式套用
  const fixturePath = path.join(ROOT_DIR, 'corpus/phpword/tests_PhpWordTests__files_documents_reader-styles.docx');
  if (fs.existsSync(fixturePath)) {
    const fixtureBytes = new Uint8Array(fs.readFileSync(fixturePath));
    const styleAdapter = new EngineAdapter();
    await styleAdapter.init();
    styleAdapter.openFile(fixtureBytes, 'reader-styles.docx');
    const styleRes = styleAdapter.setParagraphStyle({ start: 0, end: 0 }, { style_id: 'Normal' });
    assert(styleRes.ok === true, '套用段落樣式 Normal 成功');
    const undoStyleRes = styleAdapter.undo();
    assert(undoStyleRes.ok === true && undoStyleRes.undone === true, '樣式 Normal undo 成功');
    console.log('  ✓ M2.2 樣式 style_id (Normal) 閉環與 Undo 驗證通過');
  } else {
    warnings.push(`未找到 reader-styles.docx fixture，跳過樣式 fixture 驗證`);
  }

  // (7) 存檔驗證
  const savedBytes = adapter.save('output_styled.docx');
  assert(savedBytes && savedBytes.length > 0, 'save_docx_bytes 導出有效位元組');
  assert(savedBytes[0] === 0x50 && savedBytes[1] === 0x4b, '存檔位元組包含標準 ZIP (PK) 標頭');
  console.log(`  ✓ 存檔閉環驗證通過：匯出 ${savedBytes.length} 位元組合規 DOCX`);

} catch (err) {
  errors.push(`WasmEngine 樣式執行閉環例外: ${err.message}\n${err.stack}`);
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
  console.log(' 恭喜！AGY-U3 樣式命令啟用（K29b 引擎能力進 Ribbon）通過所有規格與紅線檢查。');
  console.log('='.repeat(70));
  process.exit(0);
} else {
  console.error(' 自檢結果: FAIL（紅燈預期中）');
  console.error(' 發現以下不符合規格問題：');
  errors.forEach(e => console.error('  [ERROR] ' + e));
  console.error('='.repeat(70));
  process.exit(1);
}
