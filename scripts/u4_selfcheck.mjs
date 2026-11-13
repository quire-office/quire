#!/usr/bin/env node
/**
 * u4_selfcheck.mjs
 * U4：外殼接線第一批——M4 表格家族（行列＋屬性）與 M5.1 圖片插入的驗收考卷
 *
 * 考卷形態（承接 parent t_57b7b089 的誠實性契約）：
 * 1. 靜態斷言：命令分支存在 → adapter 有對應 build*Mutation → UI 分支真的呼
 *    adapter → adapter 真的把 mutation 送進 apply_mutation_json。
 * 2. 欄位名對應：逐欄比對 `core/engine-api/src/mutation.rs` 的變體宣告（讀檔
 *    取宣告行，不是把欄名硬寫死在本檔——引擎改欄名時本考卷必須變紅）。
 * 3. 動態斷言：真 load wasm、真開檔、真呼叫，讀 `page_display_list` 的 items
 *    變化證明文件真的改了（用戶看見那層，禁讀自報旗標）。
 * 4. 誠實紅線：引擎會「靜默接受未知欄位並回 ok=true」（實測：臆造邊框欄位
 *    ok=true 且顯示列表零變化）。因此本考卷另斷外殼不得把這類零效果當成成功。
 * 5. 紅線自我鑑別（先紅後綠的自動化）：同一次執行內，把三側接線各別摘除後重
 *    跑靜態鎖，三種都必須紅——鎖壞了考卷就會靜默放水。
 *
 * 引擎實測口徑（本考卷斷言的依據，全部來自真引擎探針）：
 * - 表格沒有「Table」型別的顯示列表項目：插表增加的是儲存格段落 Text 項。
 * - 儲存格底紋 → 多一筆 Rect（fill_argb 精確可比）。
 * - 表格層底紋（SetTableProps.shading_fill）→ XML 寫入但顯示列表零發射。
 * - 邊框：引擎 Mutation 欄位不存在（TablePropsPatch／CellPropsPatch 無任何
 *   border 欄），serde 吞未知欄回 ok=true——嚴禁外殼宣稱成功。
 */

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

const HTML_PATH = path.join(ROOT_DIR, 'shells/web/editor/index.html');
const CORE_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/qword-core.js');
const UI_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/qword-ui.js');
const ADAPTER_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/engine-adapter.js');
const MUTATION_RS_PATH = path.join(ROOT_DIR, 'core/engine-api/src/mutation.rs');
const WASM_JS_PATH = path.join(ROOT_DIR, 'shells/web/pkg/quire_web.js');

let pass = true;
const errors = [];
const warnings = [];

function assert(condition, message) {
  if (!condition) {
    errors.push(message);
    pass = false;
  }
}

function warn(condition, message) {
  if (!condition) warnings.push(message);
}

console.log('='.repeat(70));
console.log('U4 自檢：M4 表格家族＋M5.1 圖片插入 外殼接線驗收（真引擎斷言）');
console.log('='.repeat(70));

// ─────────────────────────────────────────────────────────────────────────────
// [檢查 1] 必要檔案與產物
// ─────────────────────────────────────────────────────────────────────────────
console.log('\n[檢查 1] 必要外殼檔案與 Wasm 產物存在性...');
for (const [label, p] of [
  ['index.html', HTML_PATH], ['qword-core.js', CORE_JS_PATH], ['qword-ui.js', UI_JS_PATH],
  ['engine-adapter.js', ADAPTER_JS_PATH], ['mutation.rs（欄位真相來源）', MUTATION_RS_PATH],
  ['quire_web.js', WASM_JS_PATH], ['quire_web_bg.wasm', path.join(ROOT_DIR, 'shells/web/pkg/quire_web_bg.wasm')]
]) {
  assert(fs.existsSync(p), `缺少必要檔案：${label}（${p}）`);
}
console.log('  ✓ 必要檔案齊備');

const adapterSrc = fs.readFileSync(ADAPTER_JS_PATH, 'utf-8');
const uiSrc = fs.readFileSync(UI_JS_PATH, 'utf-8');
const coreSrc = fs.readFileSync(CORE_JS_PATH, 'utf-8');
const htmlSrc = fs.readFileSync(HTML_PATH, 'utf-8');
const mutationRs = fs.readFileSync(MUTATION_RS_PATH, 'utf-8');

// ─────────────────────────────────────────────────────────────────────────────
// [檢查 2] 欄位名不靠猜：逐欄對 mutation.rs 宣告
// ─────────────────────────────────────────────────────────────────────────────
console.log('\n[檢查 2] Mutation 欄位名與 mutation.rs 宣告逐欄比對...');

/**
 * 抓 `pub enum Mutation {` 之後各變體的欄位宣告（引擎是欄位名的唯一真相）。
 * 變體可能是多行或單行宣告（`DeleteTableRow { table: TablePath, row: usize },`），
 * 因此先取大括號內文再按逗號切欄，不能只靠行首匹配。
 */
function enumVariantFields(name) {
  const start = mutationRs.indexOf('pub enum Mutation');
  if (start < 0) return null;
  const seg = mutationRs.slice(start);
  const head = seg.match(new RegExp(`\\b${name}\\s*\\{`));
  if (!head) return null;
  const openIdx = head.index + head[0].length - 1;
  let depth = 0, end = -1;
  for (let i = openIdx; i < seg.length; i++) {
    if (seg[i] === '{') depth++;
    else if (seg[i] === '}') { depth--; if (depth === 0) { end = i; break; } }
  }
  if (end < 0) return null;
  const body = seg.slice(openIdx + 1, end);
  const fields = [];
  for (const piece of body.split(',')) {
    const m = piece.match(/^\s*(?:#\[[^\]]*\]\s*)*([a-z_][a-z0-9_]*)\s*:/);
    if (m) fields.push(m[1]);
  }
  return fields;
}

const ENGINE_FIELD_CONTRACT = {
  InsertTableRow: ['table', 'at_row', 'position'],
  DeleteTableRow: ['table', 'row'],
  InsertTableColumn: ['table', 'at_grid_col', 'position', 'width_pt'],
  DeleteTableColumn: ['table', 'grid_col'],
  InsertInlineImage: ['pos', 'media_bytes', 'mime_type', 'width_pt', 'height_pt']
};
for (const [variant, expected] of Object.entries(ENGINE_FIELD_CONTRACT)) {
  const actual = enumVariantFields(variant);
  assert(actual !== null, `mutation.rs 查無 ${variant} 變體宣告（考卷前提失效）`);
  if (actual === null) continue;
  for (const f of expected) {
    assert(actual.includes(f), `mutation.rs 的 ${variant} 缺欄位 ${f}（本卡斷言與引擎不一致）`);
  }
}
console.log('  ✓ 五個變體的欄位名在 mutation.rs 逐筆存在');

/** patch 結構體欄位（SetTableProps／SetCellProps 的可放行欄） */
function structFields(structName) {
  const m = mutationRs.match(new RegExp(`pub struct ${structName} \\{([\\s\\S]*?)\\n\\}`));
  if (!m) return null;
  return [...m[1].matchAll(/^\s*(?:#\[[^\]]*\]\s*)?pub\s+([a-z_][a-z0-9_]*)\s*:/gm)].map(x => x[1]);
}
const tablePatchFields = structFields('TablePropsPatch');
const cellPatchFields = structFields('CellPropsPatch');
assert(Array.isArray(tablePatchFields), '查無 TablePropsPatch 結構體');
assert(Array.isArray(cellPatchFields), '查無 CellPropsPatch 結構體');
assert(tablePatchFields?.includes('shading_fill'), 'TablePropsPatch 應含 shading_fill');
assert(tablePatchFields?.includes('alignment') && tablePatchFields?.includes('width_pt'), 'TablePropsPatch 應含 alignment／width_pt');
assert(cellPatchFields?.includes('shading_fill') && cellPatchFields?.includes('vertical_align'), 'CellPropsPatch 應含 shading_fill／vertical_align');

// 邊框缺欄證據（誠實紀律的地基）：patch 結構體不得有 border 類欄位，
// 且全 mutation.rs 查無 border 宣告。若哪天引擎補上，本斷言會紅並要求重估。
for (const [name, fields] of [['TablePropsPatch', tablePatchFields], ['CellPropsPatch', cellPatchFields]]) {
  const borderish = (fields || []).filter(f => /border|rule_line|edge/i.test(f));
  assert(borderish.length === 0, `${name} 出現邊框欄位 ${JSON.stringify(borderish)}：引擎已支援邊框，請重估 U4 邊框判定`);
}
assert(!/\bborders?\s*:/.test(mutationRs), 'mutation.rs 出現 border 欄位宣告：邊框判定需重估');
console.log('  ✓ 邊框確認缺欄：TablePropsPatch／CellPropsPatch 零 border 欄（外殼不得宣稱邊框成功）');

// ─────────────────────────────────────────────────────────────────────────────
// [檢查 3] adapter build*Mutation 契約（欄位名逐欄核）
// ─────────────────────────────────────────────────────────────────────────────
console.log('\n[檢查 3] adapter build*Mutation 工廠函數契約...');
const adapterModule = await import(ADAPTER_JS_PATH);
const coreModule = await import(CORE_JS_PATH);
const {
  EngineAdapter, getDefaultSampleBytes,
  buildTablePath, buildInsertTableRowMutation, buildDeleteTableRowMutation,
  buildInsertTableColumnMutation, buildDeleteTableColumnMutation,
  buildSetTablePropsMutation, buildSetCellPropsMutation, buildInsertInlineImageMutation,
  normalizeHex6Fill, normalizeInsertSide, normalizeVerticalAlign
} = adapterModule;
const {
  U4_TABLE_ROW_COMMANDS, U4_TABLE_COL_COMMANDS, U4_TABLE_PROPS_COMMANDS,
  U4_CELL_PROPS_COMMANDS, ENGINE_SUPPORTED_IMAGE_MIMES, QWordCore
} = coreModule;

for (const [name, fn] of Object.entries({
  buildTablePath, buildInsertTableRowMutation, buildDeleteTableRowMutation,
  buildInsertTableColumnMutation, buildDeleteTableColumnMutation,
  buildSetTablePropsMutation, buildSetCellPropsMutation, buildInsertInlineImageMutation,
  normalizeHex6Fill, normalizeInsertSide, normalizeVerticalAlign
})) {
  assert(typeof fn === 'function', `adapter 缺 ${name}`);
}

if (typeof buildTablePath === 'function') {
  const tp = buildTablePath(2);
  assert(tp.target === 'Body' && tp.root_slot === 2 && Array.isArray(tp.nested_steps) && tp.nested_steps.length === 0,
    'TablePath 形態需為 { target: Body, root_slot, nested_steps }');
  const nested = buildTablePath(1, [{ row_idx: 0, cell_idx: 1, sub_table_idx: 0 }]);
  assert(nested.nested_steps.length === 1 && nested.nested_steps[0].sub_table_idx === 0, 'nested_steps 應可帶巢裝表格定位');
  console.log('  ✓ TablePath（正文路由 target=Body＋巢裝 nested_steps 三欄）形態正確');
}

if (typeof buildInsertTableRowMutation === 'function') {
  const op = buildInsertTableRowMutation(buildTablePath(0), 1, 'Before');
  assert(op.InsertTableRow !== undefined, 'InsertTableRow 頂層鍵存在');
  assert(op.InsertTableRow.at_row === 1 && op.InsertTableRow.position === 'Before', 'InsertTableRow at_row／position 正確');
  assert(op.InsertTableRow.table.target === 'Body' && op.InsertTableRow.table.root_slot === 0, 'InsertTableRow table 路由正確');
  console.log('  ✓ M4.3 InsertTableRow Mutation JSON 符合引擎 serde 契約');
}
if (typeof buildDeleteTableRowMutation === 'function') {
  const op = buildDeleteTableRowMutation(buildTablePath(0), 2);
  assert(op.DeleteTableRow !== undefined && op.DeleteTableRow.row === 2, 'DeleteTableRow row 正確');
  console.log('  ✓ M4.3 DeleteTableRow Mutation JSON 符合引擎 serde 契約');
}
if (typeof buildInsertTableColumnMutation === 'function') {
  const op = buildInsertTableColumnMutation(buildTablePath(0), 1, 'After', 70);
  assert(op.InsertTableColumn !== undefined, 'InsertTableColumn 頂層鍵存在');
  assert(op.InsertTableColumn.at_grid_col === 1 && op.InsertTableColumn.position === 'After', 'InsertTableColumn at_grid_col／position 正確');
  assert(op.InsertTableColumn.width_pt === 70, 'InsertTableColumn width_pt（引擎必填欄）正確填裝');
  console.log('  ✓ M4.4 InsertTableColumn Mutation JSON 符合引擎 serde 契約');
}
if (typeof buildDeleteTableColumnMutation === 'function') {
  const op = buildDeleteTableColumnMutation(buildTablePath(0), 0);
  assert(op.DeleteTableColumn !== undefined && op.DeleteTableColumn.grid_col === 0, 'DeleteTableColumn grid_col 正確');
  console.log('  ✓ M4.4 DeleteTableColumn Mutation JSON 符合引擎 serde 契約');
}
if (typeof buildSetTablePropsMutation === 'function') {
  const op = buildSetTablePropsMutation(buildTablePath(0), { width_pt: 200, alignment: 'center', shading_fill: 'E06699' });
  assert(op.SetTableProps !== undefined, 'SetTableProps 頂層鍵存在');
  assert(op.SetTableProps.patch.width_pt === 200 && op.SetTableProps.patch.alignment === 'center' && op.SetTableProps.patch.shading_fill === 'E06699',
    'SetTableProps patch 三欄名稱與引擎一致');
  console.log('  ✓ M4.6 SetTableProps Mutation JSON 符合引擎 serde 契約');
}
if (typeof buildSetCellPropsMutation === 'function') {
  const op = buildSetCellPropsMutation(buildTablePath(0), 1, 2, { shading_fill: '00AAFF', vertical_align: 'bottom' });
  assert(op.SetCellProps !== undefined, 'SetCellProps 頂層鍵存在');
  assert(op.SetCellProps.row === 1 && op.SetCellProps.cell === 2, 'SetCellProps row／cell 正確');
  assert(op.SetCellProps.patch.shading_fill === '00AAFF' && op.SetCellProps.patch.vertical_align === 'bottom',
    'SetCellProps patch 欄名與引擎一致');
  console.log('  ✓ M4.6 SetCellProps Mutation JSON 符合引擎 serde 契約（欄名對齊引擎）');
}
if (typeof buildInsertInlineImageMutation === 'function') {
  const bytes = new Uint8Array([1, 2, 3, 4]);
  const op = buildInsertInlineImageMutation({ para: 0, cp: 5 }, bytes, 'image/png', 120, 90, '說明');
  assert(op.InsertInlineImage !== undefined, 'InsertInlineImage 頂層鍵存在');
  assert(op.InsertInlineImage.target === 'Body', 'InsertInlineImage target=Body');
  assert(op.InsertInlineImage.pos.para === 0 && op.InsertInlineImage.pos.cp === 5, 'InsertInlineImage pos 正確');
  assert(op.InsertInlineImage.mime_type === 'image/png' && op.InsertInlineImage.width_pt === 120, 'InsertInlineImage MIME／尺寸正確');
  assert(Array.isArray(op.InsertInlineImage.media_bytes) && op.InsertInlineImage.media_bytes.length === 4,
    'media_bytes 必須是位元組陣列（引擎 Vec<u8> 的 serde 形態），圖片要真的進引擎');
  console.log('  ✓ M5.1 InsertInlineImage Mutation JSON 符合引擎 serde 契約（媒體位元組入欄）');
}

// 正規化器的防假成功行為
if (typeof normalizeHex6Fill === 'function') {
  assert(normalizeHex6Fill('#00aaff') === '00AAFF', 'normalizeHex6Fill 應吃 #RRGGBB 並大寫化');
  assert(normalizeHex6Fill('auto') === 'auto', 'normalizeHex6Fill 應放行 auto');
  for (const bad of ['GGGGGG', '#GGGGGG', 'FF00', 'rgb(1,2,3)', '00AAFF00']) {
    assert(normalizeHex6Fill(bad) === null, `normalizeHex6Fill 必須拒絕非法值 ${bad}（引擎對非法值回 ok=true 卻零效果）`);
  }
  console.log('  ✓ 底紋正規化：非法 hex 一律拒絕（擋掉引擎的假成功路徑）');
}
if (typeof normalizeInsertSide === 'function') {
  assert(normalizeInsertSide('above') === 'Before' && normalizeInsertSide('below') === 'After', 'normalizeInsertSide 別名正規化');
  assert(normalizeInsertSide('sideways', null) === null, 'normalizeInsertSide 非法值需回 null');
}

// ─────────────────────────────────────────────────────────────────────────────
// [檢查 4] 三側接線完整性（假 live 回塞鎖的靜態面）
// ─────────────────────────────────────────────────────────────────────────────
console.log('\n[檢查 4] 命令存在 → core 分支 → UI 呼叫 adapter → adapter 呼叫引擎（三側齊備）...');

/**
 * 靜態鎖主體。抽成函數是為了讓 [檢查 8] 能在摘除接線的副本上重跑同一把鎖——
 * 考卷必須證明自己會紅，而不只是宣稱會紅。
 */
function staticWiringLocks(sources) {
  const fails = [];
  const { adapter, ui, core } = sources;

  // (a) adapter 側：每個變體都要有 build* 且真的經 apply_mutation_json 送引擎
  const applyCall = /apply_mutation_json\(\s*JSON\.stringify\(/;
  if (!applyCall.test(adapter)) fails.push('adapter 未透過 apply_mutation_json 送 mutation（引擎從未被呼叫）');
  for (const b of ['buildInsertTableRowMutation', 'buildDeleteTableRowMutation',
    'buildInsertTableColumnMutation', 'buildDeleteTableColumnMutation',
    'buildSetTablePropsMutation', 'buildSetCellPropsMutation', 'buildInsertInlineImageMutation']) {
    if (!new RegExp(`${b}\\s*\\(`).test(adapter)) fails.push(`adapter 缺 ${b} 呼叫點`);
  }

  // (b) core 側：命令分支存在且真的派發事件（不是只記帳不匯流）
  for (const cmd of ['U4_TABLE_ROW_COMMANDS', 'U4_TABLE_COL_COMMANDS', 'U4_TABLE_PROPS_COMMANDS', 'U4_CELL_PROPS_COMMANDS']) {
    if (!core.includes(cmd)) fails.push(`core 缺命令表 ${cmd}`);
  }
  for (const evt of ['table:row-col-mutate', 'table:props-apply', 'cell:props-apply', 'image:inline-insert']) {
    if (!new RegExp(`_notify\\('${evt}'`).test(core)) fails.push(`core 未派發 ${evt} 事件（分支只記帳不落地）`);
  }
  if (!/recordCommandFailure\s*\(/.test(core)) fails.push('core 缺 recordCommandFailure（引擎拒絕時無法誠實留痕）');

  // (c) UI 側：事件接聽 ＋ 真的呼叫 adapter 方法
  for (const [evt, handler] of [
    ['table:row-col-mutate', 'handleTableRowCol'],
    ['table:props-apply', 'handleTableProps'],
    ['cell:props-apply', 'handleCellProps'],
    ['image:inline-insert', 'handleInlineImageInsert']
  ]) {
    if (!new RegExp(`'${evt}'`).test(ui)) fails.push(`UI 未接聽 ${evt}`);
    if (!new RegExp(`${handler}\\s*\\(`).test(ui)) fails.push(`UI 缺 ${handler}`);
  }
  for (const m of ['adapter.insertTableRow', 'adapter.deleteTableRow', 'adapter.insertTableColumn',
    'adapter.deleteTableColumn', 'adapter.setTableProps', 'adapter.setCellProps', 'adapter.insertInlineImage']) {
    if (!ui.includes(m)) fails.push(`UI 未呼叫 ${m}（只 dispatch 事件＝假接線）`);
  }
  // 圖片必走既有 #qword-file-input 路徑（契約第 4 條），且不得只畫佔位框
  if (!/qword-file-input/.test(ui)) fails.push('UI 未使用既有 #qword-file-input 檔案選取路徑');
  if (!/imagePickMode/.test(ui)) fails.push('UI 未區分圖片選取模式（檔案管線會被開檔請求搶走）');

  // (d) 誠實紅線：邊框不得被宣稱；行列操作失敗必須補日誌
  if (/build\w*Border\w*Mutation|SetTableProps[^]{0,80}border/i.test(adapter)) {
    fails.push('adapter 出現邊框 Mutation：引擎無邊框欄位，屬假接線');
  }
  if (!/_reportEngineRejection\s*\(/.test(ui)) fails.push('UI 缺 _reportEngineRejection（引擎拒絕會靜默消失）');
  return fails;
}

const locksOk = staticWiringLocks({ adapter: adapterSrc, ui: uiSrc, core: coreSrc });
for (const f of locksOk) assert(false, `接線紅線違規：${f}`);
if (locksOk.length === 0) {
  console.log('  ✓ 三側接線齊備（零紅線違規）');
} else {
  for (const f of locksOk) console.log(`  ✗ 接線紅線違規：${f}`);
}

// 表格層底紋的誠實處理：引擎寫 XML 但發射面零發射 → 外殼不得當成可見成功
assert(/display.?list|顯示列表/.test(adapterSrc + uiSrc), '外殼需以顯示列表作為效果真相（不得只讀 ok 旗標）');

// ─────────────────────────────────────────────────────────────────────────────
// [檢查 5] 動態斷言：真 wasm、真開檔、真呼叫、讀顯示列表
// ─────────────────────────────────────────────────────────────────────────────
console.log('\n[檢查 5] 真引擎動態閉環（讀 page_display_list，禁讀自報旗標）...');

function makePng(w, h, rgb) {
  const raw = Buffer.alloc(h * (1 + w * 3));
  for (let y = 0; y < h; y++) {
    raw[y * (1 + w * 3)] = 0;
    for (let x = 0; x < w; x++) {
      const o = y * (1 + w * 3) + 1 + x * 3;
      raw[o] = rgb[0]; raw[o + 1] = rgb[1]; raw[o + 2] = rgb[2];
    }
  }
  const crc32 = buf => { let c = ~0; for (const b of buf) { c ^= b; for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xEDB88320 & -(c & 1)); } return ~c >>> 0; };
  const chunk = (type, data) => {
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td));
    return Buffer.concat([len, td, crc]);
  };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 2;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
    chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))
  ]);
}

async function openFresh() {
  const a = new EngineAdapter();
  await a.init();
  a.openFile(getDefaultSampleBytes(), 'sample.docx');
  return a;
}

/** 全頁顯示列表簽名：型別＋座標（2 位小數）＋尺寸＋顏色。任一像素位移都會變。 */
function dlSig(a) {
  const out = [];
  for (let p = 0; p < a.pageCount(); p++) {
    for (const it of a.getPageDisplayList(p).items) {
      const k = Object.keys(it)[0];
      const v = it[k];
      out.push(k === 'Text'
        ? `T${(v.text || '').slice(0, 12)}@${v.x.toFixed(2)},${v.y.toFixed(2)},${v.size_pt.toFixed(2)}`
        : `${k}@${(v.x ?? v.x1).toFixed(2)},${(v.y ?? v.y1).toFixed(2)}|${(v.w ?? v.width_pt ?? 0).toFixed(2)}|${v.fill_argb ?? v.color_argb ?? ''}`);
    }
  }
  return out.join(';');
}
function dlStat(a) {
  const m = {};
  for (let p = 0; p < a.pageCount(); p++) {
    for (const it of a.getPageDisplayList(p).items) { const k = Object.keys(it)[0]; m[k] = (m[k] || 0) + 1; }
  }
  return m;
}
const dlCount = a => { let n = 0; for (let p = 0; p < a.pageCount(); p++) n += a.getPageDisplayList(p).items.length; return n; };
const rectsOf = a => { const o = []; for (let p = 0; p < a.pageCount(); p++) for (const it of a.getPageDisplayList(p).items) if (it.Rect) o.push(it.Rect); return o; };
const imgsOf = a => { const o = []; for (let p = 0; p < a.pageCount(); p++) for (const it of a.getPageDisplayList(p).items) if (it.Image) o.push(it.Image); return o; };

// 5.1 插入 2x3 表 → 顯示列表出現儲存格段落（引擎無 Table 型別，如實斷 Text 增量）
const t23 = await openFresh();
const baseCount = dlCount(t23);
const insTable = t23.insertTable(2, 3);
const afterTable = dlCount(t23);
assert(insTable.ok === true, `insert:table 引擎應受理（實際 ${JSON.stringify(insTable.error)}）`);
assert(afterTable > baseCount, `插入 2x3 表後顯示列表項目應增加（${baseCount} → ${afterTable}）`);
assert(t23.tableCount() === 1, `表格數應為 1（實際 ${t23.tableCount()}）`);
const shape23 = t23.tableShape(0);
assert(shape23.ok && shape23.rows === 2 && shape23.cols === 3, `tableShape 應讀回 2x3（實際 ${JSON.stringify(shape23)}）`);
console.log(`  ✓ M4.1 InsertTable：顯示列表 ${baseCount} → ${afterTable}（+${afterTable - baseCount} 筆儲存格段落），引擎結構讀回 2 列 3 欄`);

// 5.2 插列 → 項目增加；刪列 → 項目如實減少（考卷必斷）
const beforeRowIns = dlCount(t23);
const rowIns = t23.insertTableRow('After', { rootSlot: 0, atRow: 0 });
const afterRowIns = dlCount(t23);
assert(rowIns.ok === true, `InsertTableRow 應受理（${JSON.stringify(rowIns.error)}）`);
assert(afterRowIns > beforeRowIns, `插入一列後顯示列表應增加（${beforeRowIns} → ${afterRowIns}）`);
const afterRowDel = (() => {
  const d = t23.deleteTableRow({ rootSlot: 0, row: 1 });
  assert(d.ok === true, `DeleteTableRow 應受理（${JSON.stringify(d.error)}）`);
  return dlCount(t23);
})();
assert(afterRowDel < afterRowIns, `DeleteTableRow 後項目須如實減少（${afterRowIns} → ${afterRowDel}）`);
assert(afterRowDel === beforeRowIns, `刪回原列數後項目數應回到插列前（${beforeRowIns} vs ${afterRowDel}）`);
console.log(`  ✓ M4.3 列操作：${beforeRowIns} → ${afterRowIns}（插列）→ ${afterRowDel}（刪列，如實減少且回到基線）`);

// 5.3 插欄／刪欄 → 欄數讀回如實增減
const shapeBefore = t23.tableShape(0);
const colIns = t23.insertTableColumn('After', { rootSlot: 0, atGridCol: 0, widthPt: 70 });
assert(colIns.ok === true, `InsertTableColumn 應受理（${JSON.stringify(colIns.error)}）`);
const shapeAfterCol = t23.tableShape(0);
assert(shapeAfterCol.cols === shapeBefore.cols + 1, `插欄後欄數應 +1（${shapeBefore.cols} → ${shapeAfterCol.cols}）`);
const colDel = t23.deleteTableColumn({ rootSlot: 0, gridCol: 1 });
assert(colDel.ok === true, `DeleteTableColumn 應受理（${JSON.stringify(colDel.error)}）`);
const shapeBack = t23.tableShape(0);
assert(shapeBack.cols === shapeBefore.cols, `刪欄後欄數應回原值（${shapeBefore.cols} vs ${shapeBack.cols}）`);
console.log(`  ✓ M4.4 欄操作：引擎讀回欄數 ${shapeBefore.cols} → ${shapeAfterCol.cols} → ${shapeBack.cols}（幾何由引擎決定，JS 端零推算）`);

// 5.4 儲存格底紋 → 顯示列表出現 Rect 且 fill_argb 精確可比（引擎寫 XML＋發射雙証）
const c4 = await openFresh();
c4.insertTable(2, 3);
const rectBefore = rectsOf(c4).length;
const cellShd = c4.setCellProps({ shadingFill: '#00AAFF' }, { rootSlot: 0, row: 0, cell: 1 });
const rectsAfter = rectsOf(c4);
assert(cellShd.ok === true, `SetCellProps 底紋應受理（${JSON.stringify(cellShd.error)}）`);
assert(rectsAfter.length > rectBefore, `儲存格底紋後顯示列表應出現 Rect（${rectBefore} → ${rectsAfter.length}）`);
const wanted = 0xFF00AAFF;
assert(rectsAfter.some(r => r.fill_argb === wanted),
  `Rect.fill_argb 應為 0xFF00AAFF（實際 ${JSON.stringify(rectsAfter.map(r => '0x' + r.fill_argb.toString(16)))}）`);
const savedXmlHasShd = (() => {
  const buf = Buffer.from(c4.save());
  // ZIP 內 document.xml 是 deflate 壓縮，逐檔解壓才算數（latin1 直搜會假陰性）
  const names = [];
  for (let i = 0; i + 4 <= buf.length; i++) {
    if (buf.readUInt32LE(i) === 0x04034b50) {
      const nameLen = buf.readUInt16LE(i + 26);
      const extraLen = buf.readUInt16LE(i + 28);
      const cSize = buf.readUInt32LE(i + 18);
      const start = i + 30 + nameLen + extraLen;
      names.push({ name: buf.subarray(i + 30, i + 30 + nameLen).toString('utf-8'), start, cSize });
    }
  }
  const doc = names.find(n => n.name === 'word/document.xml');
  if (!doc) return { ok: false, why: 'ZIP 查無 word/document.xml' };
  let text = '';
  try { text = zlib.inflateRawSync(buf.subarray(doc.start, doc.start + doc.cSize)).toString('utf-8'); }
  catch { text = buf.subarray(doc.start, doc.start + doc.cSize).toString('utf-8'); }
  return { ok: /w:shd[^>]*w:fill="00AAFF"/i.test(text), why: 'w:shd fill=00AAFF' };
})();
assert(savedXmlHasShd.ok === true, `存檔後 word/document.xml 應含 w:shd fill=00AAFF（${savedXmlHasShd.why}）`);
console.log('  ✓ M4.6 SetCellProps 底紋：顯示列表 Rect 出現且 fill=0xFF00AAFF，存檔 XML 含 w:shd（發射＋落檔雙證）');

// 5.5 表格對齊／寬度 → 幾何簽名位移（第二個可驗證效果）
const c5 = await openFresh();
c5.insertTable(2, 3);
const alignBefore = dlSig(c5);
const alignRes = c5.setTableProps({ alignment: 'center' }, { rootSlot: 0 });
const alignAfter = dlSig(c5);
assert(alignRes.ok === true, `SetTableProps alignment 應受理（${JSON.stringify(alignRes.error)}）`);
assert(alignBefore !== alignAfter, 'SetTableProps alignment=center 必須讓顯示列表幾何真的位移（否則屬假成功）');
const widthBefore = dlSig(c5);
const widthRes = c5.setTableProps({ widthPt: 200 }, { rootSlot: 0 });
assert(widthRes.ok === true, `SetTableProps width_pt 應受理（${JSON.stringify(widthRes.error)}）`);
assert(widthBefore !== dlSig(c5), 'SetTableProps width_pt 必須讓顯示列表幾何真的位移');
console.log('  ✓ M4.6 SetTableProps 對齊／寬度：顯示列表簽名如實變動（幾何由引擎 TableInput 決定）');

// 5.6 儲存格垂直對齊：需列高 > 內容高才有可見位移（與 Word 同口徑），列高條件下必斷位移
const c6 = await openFresh();
c6.insertTable(1, 1);
c6.engine.apply_mutation_json(JSON.stringify({
  SetRowProps: { table: { target: 'Body', root_slot: 0, nested_steps: [] }, row: 0, patch: { height_pt: 40, header: null, cant_split: null } }
}));
const vaBefore = dlSig(c6);
const vaRes = c6.setCellProps({ verticalAlign: 'bottom' }, { rootSlot: 0, row: 0, cell: 0 });
assert(vaRes.ok === true, `SetCellProps vertical_align 應受理（${JSON.stringify(vaRes.error)}）`);
assert(vaBefore !== dlSig(c6), '列高 40pt 時 vertical_align=bottom 必須讓格內文字 y 位移');
console.log('  ✓ M4.6 SetCellProps 垂直對齊：列高 40pt 條件下格內文字 y 如實位移');

// 5.7 非法參數：外殼必須先在本地擋掉（引擎對非法底紋回 ok=true 卻零效果）
const c7 = await openFresh();
c7.insertTable(2, 3);
const illegalBefore = dlCount(c7);
for (const bad of ['GGGGGG', 'FF00', 'rgb(1,2,3)', '#GGGGGG']) {
  const r = c7.setCellProps({ shadingFill: bad }, { rootSlot: 0, row: 0, cell: 0 });
  assert(r.ok === false && r.rejectedByShell === true,
    `非法底紋 ${bad} 必須由外殼擋下（實際 ok=${r.ok} rejectedByShell=${r.rejectedByShell}）`);
}
assert(dlCount(c7) === illegalBefore, '非法參數不得改變顯示列表');
const badAlign = c7.setTableProps({ alignment: 'middle' }, { rootSlot: 0 });
assert(badAlign.ok === false, `非法 alignment 必須被擋（實際 ${badAlign.ok}）`);
const badCol = c7.insertTableColumn('After', { rootSlot: 0, atGridCol: 0, widthPt: -5 });
assert(badCol.ok === false, `負寬度必須被擋（實際 ${badCol.ok}）`);
console.log('  ✓ 非法參數一律外殼擋下（引擎的 ok=true 假成功不可外泄）');

// 5.8 邊框：引擎無欄位、serde 靜默吞 → 外殼不得產生宣稱成功的路徑
const c8 = await openFresh();
c8.insertTable(2, 3);
const ruleBefore = (() => { let n = 0; for (let p = 0; p < c8.pageCount(); p++) for (const it of c8.getPageDisplayList(p).items) if (it.Rule) n++; return n; })();
const borderAttempt = JSON.parse(c8.engine.apply_mutation_json(JSON.stringify({
  SetTableProps: { table: { target: 'Body', root_slot: 0, nested_steps: [] }, patch: { border_pt: 3, borders: 'single' } }
})));
let ruleAfter = 0;
for (let p = 0; p < c8.pageCount(); p++) for (const it of c8.getPageDisplayList(p).items) if (it.Rule) ruleAfter++;
assert(borderAttempt.ok === true && ruleAfter === ruleBefore,
  '探針前提失效：引擎對臆造邊框欄位應回 ok=true 且零發射（此斷言紅代表引擎行為已變，U4 邊框判定需重估）');
const shellBorder = c8.setTableProps({ borderPt: 3 }, { rootSlot: 0 });
assert(shellBorder.ok === false, '外殼不得接受邊框參數（會造成「引擎說成功、畫面沒東西」的假成功）');
console.log(`  ✓ 邊框誠實紅線：引擎臆造欄回 ok=true 但 Rule ${ruleBefore}→${ruleAfter}（零發射），外殼拒絕邊框參數不冒充成功`);

// 5.9 表格層底紋：引擎寫 XML 但發射面零發射 → 必須如實回報「無可見效果」
const c9 = await openFresh();
c9.insertTable(2, 3);
const tblShdBefore = dlCount(c9);
const tblShd = c9.setTableProps({ shadingFill: 'E06699' }, { rootSlot: 0 });
const tblShdAfter = dlCount(c9);
warn(tblShdAfter === tblShdBefore, '表格層底紋開始產生顯示列表發射：可考慮改掛可見效果');
assert(tblShd.ok === true, `表格層底紋引擎會受理（XML 寫入），實際 ${JSON.stringify(tblShd.error)}`);
assert(tblShdAfter === tblShdBefore, '實測：表格層底紋無顯示列表發射（若已改變請重估標籤）');
assert(!/shadingFill[^]{0,120}要求?\s*live|table:shading[^]{0,60}live/.test(uiSrc),
  '表格層底紋無可見效果，外殼不得把它掛成可見成功（誠實標籤由 PM 決定）');
console.log(`  ✓ 表格層底紋：引擎受理（XML 層真實寫入）但顯示列表 ${tblShdBefore}→${tblShdAfter} 零發射，外殼不冒充可見效果`);

// 5.10 圖片插入：Image 項＋真像素＋存檔媒體檔（不准只畫佔位框）
const c10 = await openFresh();
const png = makePng(6, 4, [0xFF, 0x33, 0x00]);
const imgBefore = imgsOf(c10).length;
const imgRes = c10.insertInlineImage(new Uint8Array(png), 'image/png', { para: 0, cp: 0 });
const imgs = imgsOf(c10);
assert(imgRes.ok === true, `InsertInlineImage 應受理（${JSON.stringify(imgRes.error)}）`);
assert(imgs.length === imgBefore + 1, `Image 顯示項應 +1（${imgBefore} → ${imgs.length}）`);
const img = imgs[imgs.length - 1];
assert(img.mime === 'image/png', `Image.mime 應為 image/png（實際 ${img.mime}）`);
const px = Buffer.from(img.data_b64 || '', 'base64');
assert(px.length > 0 && px[0] === 0x89 && px[1] === 0x50, 'Image.data_b64 必須帶真實 PNG 像素（非佔位框）');
assert(img.w > 0 && img.h > 0, 'Image 幾何由引擎回傳且須為正');
const saved = Buffer.from(c10.save());
assert(saved.includes(Buffer.from('word/media/', 'utf-8')), '存檔 ZIP 必須含 word/media/ 媒體檔（圖片真的進引擎）');
console.log(`  ✓ M5.1 InsertInlineImage：Image 項 ${imgBefore} → ${imgs.length}、像素 ${px.length} bytes（PNG 檔頭驗證）、存檔含 word/media/`);

// 5.11 圖片誠實失敗路徑：偽造檔頭 → 引擎真的拒絕，外殼如實回報
const forged = Buffer.from('GIF89a-not-really-png-bytes');
const badImg = c10.insertInlineImage(new Uint8Array(forged), 'image/png', { para: 0, cp: 0 });
assert(badImg.ok === false, `偽造檔頭必須被引擎拒絕（實際 ok=${badImg.ok}）`);
assert(/圖片|媒體|格式/.test(String(badImg.error || '')), `引擎錯誤訊息需可讀（實際 ${JSON.stringify(badImg.error)}）`);
const wrongMime = c10.insertInlineImage(new Uint8Array(png), 'image/tiff', { para: 0, cp: 0 });
assert(wrongMime.ok === false && wrongMime.rejectedByShell === true, '非引擎支援 MIME 應由外殼先擋');
console.log('  ✓ 圖片誠實失敗：偽造檔頭引擎拒絕（訊息回傳），非支援 MIME 外殼先擋');

// 5.12 undo／redo：五類變體逐一嚴格可逆
const undoCases = [
  ['InsertTableRow', a => a.insertTableRow('After', { rootSlot: 0, atRow: 0 }), a => a.deleteTableRow({ rootSlot: 0, row: 1 })],
  ['InsertTableColumn', a => a.insertTableColumn('After', { rootSlot: 0, atGridCol: 0, widthPt: 70 }), null],
  ['SetCellProps(shading)', a => a.setCellProps({ shadingFill: 'FFAA00' }, { rootSlot: 0, row: 0, cell: 0 }), null],
  ['SetTableProps(align)', a => a.setTableProps({ alignment: 'right' }, { rootSlot: 0 }), null],
  ['InsertInlineImage', a => a.insertInlineImage(new Uint8Array(png), 'image/png', { para: 0, cp: 0 }), null]
];
for (const [name, op, inverse] of undoCases) {
  const a = await openFresh();
  a.insertTable(2, 3);
  const sig0 = dlSig(a);
  const r = op(a);
  assert(r.ok === true, `undo 測試前置操作 ${name} 應受理（${JSON.stringify(r.error)}）`);
  const sig1 = dlSig(a);
  assert(sig0 !== sig1, `${name} 應造成顯示列表變化（否則 undo 測試無意義）`);
  const u = a.undo();
  assert(u.ok === true && u.redone !== false, `${name} undo 應執行（${JSON.stringify(u)}）`);
  assert(dlSig(a) === sig0, `${name} undo 後顯示列表需逐位還原（簽名不符）`);
  const rd = a.redo();
  assert(rd.ok === true, `${name} redo 應執行（${JSON.stringify(rd)}）`);
  assert(dlSig(a) === sig1, `${name} redo 後需回到套用態`);
  if (inverse) { const iv = inverse(a); assert(iv.ok === true, `${name} 反向操作應受理`); }
  console.log(`  ✓ undo/redo：${name} 嚴格可逆（簽名逐位還原＋redo 回到套用態）`);
}

// 5.13 core 命令層的誠實記帳（參數非法不得沿用 live 宣稱）
{
  const core = new QWordCore();
  const evts = [];
  core.subscribe((t, d) => evts.push([t, d]));
  const okRow = core.onCommand('table:row-insert-below', { rootSlot: 0, row: 1 }, 'live');
  assert(okRow.handled === true && okRow.requirement === 'live', `合法行列命令應 handled=true＋live（實際 ${JSON.stringify(okRow)}）`);
  assert(evts.some(([t]) => t === 'table:row-col-mutate'), '合法命令需派發落地事件');
  const badRow = core.onCommand('table:row-insert-below', { rootSlot: 0, row: -1 }, 'live');
  assert(badRow.handled === false, '負索引必須 handled=false');
  assert(badRow.requirement !== 'live', `非法參數不得沿用呼叫端的 live 宣稱（實際 ${badRow.requirement}）`);
  const badSide = core.onCommand('table:col-insert-left', { rootSlot: 0, gridCol: 0, position: 'sideways' }, 'live');
  assert(badSide.handled === false && badSide.requirement !== 'live', '非法 position 需降標並 handled=false');
  // 刪除類命令不收 position（引擎 DeleteTableRow／DeleteTableColumn 無該欄），
  // 若誤當必填會被整顆鈕擋死——瀏覽器實測抓到過這個缺陷，此處釘成回歸鎖。
  const rowDel = core.onCommand('table:row-delete', { rootSlot: 0, row: 0 }, 'live');
  assert(rowDel.handled === true && rowDel.requirement === 'live', `刪除列命令不應被 position 必填誤擋（實際 ${JSON.stringify(rowDel)}）`);
  const colDel = core.onCommand('table:col-delete', { rootSlot: 0, gridCol: 2 }, 'live');
  assert(colDel.handled === true, `刪除欄命令不應被 position 必填誤擋（實際 ${JSON.stringify(colDel)}）`);
  const rowDelBad = core.onCommand('table:row-delete', { rootSlot: 0 }, 'live');
  assert(rowDelBad.handled === true, '刪除列未帶索引屬合法（adapter 依引擎真值刪末列）');
  const badFill = core.onCommand('cell:shading', { rootSlot: 0, row: 0, cell: 0, fill: 'ZZZZZZ' }, 'live');
  assert(badFill.handled === false, `非法底紋色必須 handled=false（實際 ${badFill.handled}）`);
  const badImgCmd = core.onCommand('insert:picture', { bytes: new Uint8Array([1, 2]), mimeType: 'image/tiff' }, 'live');
  assert(badImgCmd.handled === false && badImgCmd.requirement !== 'live', '非支援 MIME 的圖片命令不得宣稱 live');
  const pick = core.onCommand('insert:picture', {}, 'pending:shell');
  assert(pick.handled === true, '無位元組＝請求開檔案選取器，屬合法受理');
  const evts2 = [];
  const core2 = new QWordCore();
  core2.subscribe((t, d) => evts2.push([t, d]));
  core2.onCommand('table:shading', { rootSlot: 0, fill: 'E06699' }, 'live');
  assert(evts2.some(([t]) => t === 'table:props-apply'), '表格屬性命令需派發 table:props-apply');
  const fail = core2.recordCommandFailure('table:shading', { rootSlot: 0 }, '引擎拒絕：結構座標越界', 'live', true);
  assert(fail.handled === false && fail.engineRejected === true && fail.rejectedByShell === false,
    '引擎拒絕需留 handled=false＋engineRejected=true 的失敗痕');
  assert(core2.getLog().filter(e => e.cmdId === 'table:shading').length === 2, '失敗需追加第二筆，不得覆寫原派發條目');
  console.log('  ✓ core 誠實記帳：非法參數降標＋引擎拒絕補失敗日誌（日誌非自報旗標）');
}

// ─────────────────────────────────────────────────────────────────────────────
// [檢查 6] 禁區守紀：本卡不得動 index.html 與 core/
// ─────────────────────────────────────────────────────────────────────────────
console.log('\n[檢查 6] 禁區與 vendor 紀律...');
const editorJsDir = path.join(ROOT_DIR, 'shells/web/editor/js');
const jsFiles = fs.readdirSync(editorJsDir).filter(f => f.endsWith('.js'));
assert(jsFiles.length >= 3, `editor/js 檔案數異常（${jsFiles.length}）`);
// 表格幾何不得在 JS 端自建：不准出現自行算欄寬／列高的排版式運算痕跡
const fakeLayout = adapterSrc.match(/colWidths?\s*=\s*\[[^\]]{0,200}\]\.map|function\s+\w*(layoutTable|computeTable|measureTable)/i);
assert(!fakeLayout, 'adapter 疑似自行運算表格幾何（嚴禁平行另造排版器）');
// 顯示列表繪製器由 vendor 提供，本卡若未改繪製器則不需 SYNC.md 登記
const rendererPath = path.join(ROOT_DIR, 'shells/web/editor/vendor/renderer.js');
assert(fs.existsSync(rendererPath), 'vendor/renderer.js 缺失（顯示列表繪製器）');
assert(fs.existsSync(path.join(ROOT_DIR, 'shells/web/editor/vendor/SYNC.md')), 'vendor/SYNC.md 缺失');
console.log('  ✓ 零平行排版器、繪製器沿用 vendor（本卡未改繪製器）');

// ─────────────────────────────────────────────────────────────────────────────
// [檢查 7] index.html 入口盤點（PM-owned：本卡只回報缺口，不動按鈕）
// ─────────────────────────────────────────────────────────────────────────────
console.log('\n[檢查 7] Ribbon 入口盤點（index.html 由 PM 擁有，本檢查只點名缺口）...');
const htmlCommands = new Set([...htmlSrc.matchAll(/data-command="([^"]+)"/g)].map(m => m[1]));
const WIRE_TARGETS = [
  ...Object.keys(U4_TABLE_ROW_COMMANDS), ...Object.keys(U4_TABLE_COL_COMMANDS),
  ...Object.keys(U4_TABLE_PROPS_COMMANDS), ...Object.keys(U4_CELL_PROPS_COMMANDS),
  'insert:picture'
];
const missingEntryPoints = WIRE_TARGETS.filter(c => !htmlCommands.has(c));
for (const c of missingEntryPoints) {
  warnings.push(`Ribbon 尚無 data-command="${c}" 入口（外殼與引擎已接好，等 PM 補鈕）`);
}
// 已存在的入口若宣稱 live，必須真的有接線（防 PM 標籤領先接線）
for (const cmd of ['insert:picture', 'insert:table']) {
  const m = htmlSrc.match(new RegExp(`data-command="${cmd}"[^>]*data-requirement="([^"]+)"`))
    || htmlSrc.match(new RegExp(`data-requirement="([^"]+)"[^>]*data-command="${cmd}"`));
  if (m && m[1] === 'live') {
    assert(staticWiringLocks({ adapter: adapterSrc, ui: uiSrc, core: coreSrc }).length === 0,
      `${cmd} 掛 live 但接線不完整（假 live）`);
  }
}
console.log(`  ✓ 入口盤點完成：${WIRE_TARGETS.length - missingEntryPoints.length}/${WIRE_TARGETS.length} 項已有 Ribbon 入口，缺 ${missingEntryPoints.length} 項（詳見警告）`);

// ─────────────────────────────────────────────────────────────────────────────
// [檢查 8] 紅線自我鑑別：摘除接線的副本必須讓同一把鎖變紅
// ─────────────────────────────────────────────────────────────────────────────
console.log('\n[檢查 8] 紅線自我鑑別（先紅後綠的自動化：三側各別摘除，皆須紅）...');
{
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'u4-discrim-'));
  const copyPair = (mut) => {
    const dir = path.join(tmp, mut.tag);
    fs.mkdirSync(dir, { recursive: true });
    const src = { adapter: adapterSrc, ui: uiSrc, core: coreSrc };
    for (const [k, fn] of Object.entries(mut.mutations)) src[k] = fn(src[k]);
    return src;
  };
  const discrim = [
    {
      tag: 'remove-engine-call',
      mutations: { adapter: s => s.replace(/apply_mutation_json\(\s*JSON\.stringify\(/g, '__never_called(') },
      expect: '引擎從未被呼叫'
    },
    {
      tag: 'remove-ui-handler',
      mutations: { ui: s => s.replace(/} else if \(type === 'table:row-col-mutate'\) \{/, '} else if (type === \'__dead_event__\') {') },
      expect: 'UI 未接聽落地事件'
    },
    {
      tag: 'remove-ui-adapter-call',
      mutations: { ui: s => s.replace(/adapter\.insertTableRow\(/g, 'notWired.insertTableRow(') },
      expect: 'UI 未呼 adapter（只 dispatch 事件）'
    },
    {
      tag: 'remove-core-dispatch',
      mutations: { core: s => s.replace(/_notify\('table:row-col-mutate'/g, '_notify(\'__dead_table_event\'') },
      expect: 'core 分支只記帳不匯流'
    },
    {
      tag: 'fake-border-wiring',
      mutations: { adapter: s => s + '\nexport function buildTableBorderMutation(){ return { SetTableProps: { patch: { border_pt: 1 } } }; }\n' },
      expect: '邊框假接線'
    }
  ];
  for (const d of discrim) {
    const mutated = copyPair(d);
    const fails = staticWiringLocks(mutated);
    assert(fails.length > 0, `鑑別失效：摘除「${d.expect}」後靜態鎖仍全綠（考卷會放水）`);
    console.log(`  ✓ 鑑別 ${d.tag} → 紅（${fails.length} 項違規：${fails[0]}）`);
  }
  // 副本跑完後真樹仍須全綠（證明鑑別只動副本）
  assert(staticWiringLocks({ adapter: adapterSrc, ui: uiSrc, core: coreSrc }).length === 0,
    '鑑別流程污染了真樹來源字串（不可發生）');
  fs.rmSync(tmp, { recursive: true, force: true });
  console.log('  ✓ 摘除態必紅已證明，且真樹零污染');
}

// ─────────────────────────────────────────────────────────────────────────────
// 結果
// ─────────────────────────────────────────────────────────────────────────────
console.log('\n' + '='.repeat(70));
if (warnings.length) {
  console.log(`警告 ${warnings.length} 項（非阻斷，PM 決策面）：`);
  for (const w of warnings) console.log('  ! ' + w);
}
if (errors.length) {
  console.log(`自檢結果：FAIL（${errors.length} 項違規）`);
  for (const e of errors) console.log('  ✗ ' + e);
  process.exit(1);
}
console.log('自檢結果：PASS — U4 表格家族與圖片插入接線通過全部規格與誠實紅線檢查。');
console.log('='.repeat(70));
