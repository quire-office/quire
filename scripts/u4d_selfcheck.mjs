#!/usr/bin/env node
/**
 * u4d_selfcheck.mjs
 * U4d：U4 合併後獨立審查返修的驗收考卷（agy 覆核 × PM 讀碼坐實 × worker 真引擎實測）
 *
 * 先紅後綠紀律：本考卷在修複前必須紅、修複後必須綠，每一盞都帶實測輸出。
 * 考卷只收窄不弱化：斷言全部讀「引擎真值」與「顯示列表」，禁讀自報旗標。
 *
 * 九盞燈（對卡面九條）：
 *   L1  單列／單欄表格的結構探針必失效 → 撤除越界探針，改讀引擎真索引
 *       （table_count／tables_summary_json／table_locator_json，合併點 db865bf）。
 *   L2  圖片鈕取消檔案選取器 → 開檔管線永久壞死（imagePickMode／accept 殘留）。
 *   L3  nested_steps 物件形態與 serde 契約錯配（引擎要 Vec<(usize,usize,usize)>）。
 *   L4  單邊指定圖片尺寸破壞長寬比。
 *   L5  刪列後幽靈游標（caret_after 恆 None，外殼不校正 → 越界 para 殘留）。
 *   L6  activeTableSlot 與多表降級序衝突（插表後盲設 tableCount()-1）。
 *   L7  table:shading 假 live（引擎表格層底紋零發射）。
 *   L8  core 派發的三個拒絕事件 UI 零監聽（非法輸入畫面完全靜默）。
 *   L9  adapter 直視 JSON.parse(引擎回傳) 無容錯。
 *
 * 真值口徑（本考卷斷言依據，全部由 worker 真引擎探針實測取得，見
 * docs/word-ui-u4d-fixes-report.md）：
 * - 1x1 表對越界 DeleteTableRow 回「…不得刪除表格僅剩的一列（請改用 DeleteTable）」，
 *   訊息不含「共 N 列」→ 舊越界探針必然回 ok:false（L1 紅燈根因）。
 * - nested_steps 傳物件：引擎回 `invalid type: map, expected a tuple of size 3`。
 * - 表格 Mutation 的 outcome.caret_after 實測恆 null → 外殼必須自行校正游標。
 * - 游標段落 → 表格反查：engine.table_locator_json(para) 回 {table_path:{target,
 *   root_slot,nested_steps}, cell_path:{row,cell,...}}，table_path 欄位名與
 *   mutation::TablePath 逐字對齊，可直接餵 M4 家族。
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

const ADAPTER_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/engine-adapter.js');
const CORE_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/qword-core.js');
const UI_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/qword-ui.js');
const MUTATION_RS_PATH = path.join(ROOT_DIR, 'core/engine-api/src/mutation.rs');
const SHELL_RS_PATH = path.join(ROOT_DIR, 'shells/web/src/lib.rs');

let pass = true;
const errors = [];
const notes = [];
function assert(cond, msg) { if (!cond) { errors.push(msg); pass = false; } }
function note(msg) { notes.push(msg); (lampLog || rawLog)(`  [NOTE] ${msg}`); }

/**
 * 一盞燈一包：擲例外也算紅（考卷不得因某盞燈崩掉而少報其他盞）。
 * 誠實紀律：燈內的成功列（含 note）只在「本燈目前零違規」時才放行——本燈已紅
 * 卻還印 ✓ 就是把失敗寫成成功，前一個 run 犯過，鎖死在這裡。
 */
let lampBase = 0;
const rawLog = console.log.bind(console);
let lampLog = null;
/** 燈內成功列出口：本燈已有違規時靜默（紅燈不印 ✓），紅燈明細由結尾統一列出。 */
function okLog(msg) { if (lampLog) lampLog(msg); else rawLog(msg); }
async function lamp(label, fn) {
  const before = errors.length;
  lampBase = before;
  rawLog(`\n[${label}]`);
  lampLog = (msg) => { if (errors.length === lampBase) rawLog(msg); };
  try {
    await fn();
  } catch (err) {
    assert(false, `${label} 擲出例外：${err && err.message ? err.message : String(err)}`);
  } finally {
    lampLog = null;
  }
  const added = errors.length - before;
  rawLog(added === 0 ? `  → ${label} 綠` : `  → ${label} 紅（${added} 項）`);
}

console.log('='.repeat(72));
console.log('U4d 自檢：表格探針撤除／圖片管線／游標校正／標籤誠實性（九盞燈）');
console.log('='.repeat(72));

const adapterSrc = fs.readFileSync(ADAPTER_JS_PATH, 'utf-8');
const coreSrc = fs.readFileSync(CORE_JS_PATH, 'utf-8');
const uiSrc = fs.readFileSync(UI_JS_PATH, 'utf-8');
const mutationRs = fs.readFileSync(MUTATION_RS_PATH, 'utf-8');
const shellRs = fs.readFileSync(SHELL_RS_PATH, 'utf-8');
const J = (x) => JSON.stringify(x);

const adapterModule = await import(ADAPTER_JS_PATH);
const coreModule = await import(CORE_JS_PATH);
const { EngineAdapter, getDefaultSampleBytes, buildTablePath } = adapterModule;
const { QWordCore } = coreModule;

async function fresh() {
  const a = new EngineAdapter();
  await a.init();
  a.openFile(getDefaultSampleBytes(), 'sample.docx');
  return a;
}
const dlCount = (a) => { let n = 0; for (let p = 0; p < a.pageCount(); p++) n += a.getPageDisplayList(p).items.length; return n; };

/** 靜態鎖：tableShape／tableCount 必須讀真索引、零越界探針（L1 的結構面）。 */
function shapeProbeLocks(adapter) {
  const fails = [];
  // 方法體以「行首兩空格＋方法名(」定位（方法定義專屬形態），避免被散在註解
  // 裡的同名文字（如「不是 tableCount()-1 的盲設」）餵錯視窗。
  const body = (name) => {
    const re = new RegExp(`\\n {2}${name}\\(`);
    const m = re.exec(adapter);
    if (!m) return null;
    const end = adapter.indexOf('\n  }', m.index);
    return adapter.slice(m.index, end < 0 ? m.index + 1200 : end + 4);
  };
  for (const name of ['tableShape', 'tableCount']) {
    const b = body(name);
    if (b === null) { fails.push(`adapter 缺 ${name}() 方法定義`); continue; }
    if (/apply_mutation_json/.test(b)) fails.push(`${name}() 仍以越界 Mutation 探針讀結構（引擎真索引已可用，探針須撤除）`);
  }
  if (!/this\.engine\.tables_summary_json\(/.test(adapter)) fails.push('adapter 未讀 this.engine.tables_summary_json()（行列數真值來源）');
  if (!/this\.engine\.table_locator_json\(/.test(adapter)) fails.push('adapter 未讀 this.engine.table_locator_json()（游標→表格反查真值來源）');
  if (/1000000000/.test(adapter)) fails.push('adapter 殘留越界探針常數 1000000000');
  return fails;
}

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 0｜真索引綁定前提', async () => {
  for (const fn of ['table_count', 'tables_summary_json', 'table_locator_json']) {
    assert(new RegExp(`pub fn ${fn}\\s*\\(`).test(shellRs),
      `shells/web/src/lib.rs 未綁 ${fn}()——撤除越界探針的前提失效`);
  }
  const tp = mutationRs.match(/pub struct TablePath \{[\s\S]*?\n\}/);
  assert(tp && /pub nested_steps:\s*Vec<\(usize,\s*usize,\s*usize\)>/.test(tp[0]),
    'mutation.rs 的 TablePath.nested_steps 需為 Vec<(usize,usize,usize)>（L3 斷言前提）');
  okLog('  ✓ 三顆真索引已綁進 wasm 殼；nested_steps 契約＝三元組陣列');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 1｜L1 單列／單欄表格結構（真索引撤除探針）', async () => {
  for (const f of shapeProbeLocks(adapterSrc)) assert(false, `結構面違規：${f}`);
  if (shapeProbeLocks(adapterSrc).length === 0) okLog('  ✓ 越界探針已撤除：結構讀真索引（summary＋locator）');

  // 1a. 1x1 表：三盞紅燈的根因場景（舊探針回 ok:false → 外殼拒操作）
  const a = await fresh();
  const ins = a.insertTable(1, 1);
  assert(ins.ok === true, `前置：插入 1x1 表應受理（${J(ins.error)}）`);
  const shape = a.tableShape(0);
  assert(shape.ok === true && shape.rows === 1 && shape.cols === 1,
    `單列單欄表 tableShape 應讀回 1×1（實得 ${J(shape)}）`);
  const base = dlCount(a);
  const rowIns = a.insertTableRow('After', { rootSlot: 0 });
  assert(rowIns.ok === true,
    `單列表未帶 at_row 的「插入列」不得被外殼拒（實得 ${J({ ok: rowIns.ok, error: rowIns.error })}）`);
  assert(dlCount(a) > base, `單列表插列後顯示列表應增加（${base} → ${dlCount(a)}）`);
  const colIns = a.insertTableColumn('After', { rootSlot: 0 });
  assert(colIns.ok === true,
    `單欄表未帶 at_grid_col 的「插入欄」不得被外殼拒（實得 ${J({ ok: colIns.ok, error: colIns.error })}）`);
  const cellRes = a.setCellProps({ verticalAlign: 'bottom' }, { rootSlot: 0 });
  assert(cellRes.ok === true,
    `setCellProps 未帶座標不得因 tableShape 連坐被拒（實得 ${J({ ok: cellRes.ok, error: cellRes.error })}）`);
  okLog(`  ✓ 1x1 表：插列／插欄／setCellProps 三條路徑全通（顯示列表 ${base} → ${dlCount(a)}）`);

  // 1b. 行列數讀回必須與引擎索引逐字一致（多形態＋刪到剩 1 列）
  for (const [rows, cols] of [[1, 3], [3, 1], [2, 3]]) {
    const t = await fresh();
    t.insertTable(rows, cols);
    const sum = JSON.parse(t.engine.tables_summary_json())[0];
    const sh = t.tableShape(0);
    assert(sh.ok === true && sh.rows === sum.rows && sh.cols === sum.grid_cols,
      `${rows}x${cols} 表：tableShape 需與引擎 summary 逐字一致（shape=${J(sh)} summary=${J({ rows: sum.rows, grid_cols: sum.grid_cols })}）`);
    assert(t.tableCount() === t.engine.table_count(),
      `tableCount() 需等於引擎 table_count()（實得 ${t.tableCount()} vs ${t.engine.table_count()}）`);
  }
  const c = await fresh();
  c.insertTable(3, 2);
  c.deleteTableRow({ rootSlot: 0, row: 2 });
  c.deleteTableRow({ rootSlot: 0, row: 1 });
  const shape1 = c.tableShape(0);
  assert(shape1.ok === true && shape1.rows === 1, `刪到剩 1 列後 tableShape 仍需讀回 rows=1（實得 ${J(shape1)}）`);
  const tail = c.deleteTableRow({ rootSlot: 0 });
  assert(tail.ok === false, '真值末列＝0，引擎應拒刪（外殼不得把越界座標偷偷送出去）');
  okLog('  ✓ 行列數讀回與引擎 summary 逐字一致（1x3／3x1／2x3／刪至剩 1 列）');

  // 1c. 游標反查真值：游標在表內時必須命中「游標那張」而非末張
  const d = await fresh();
  const last = d.paragraphCount() - 1;
  d.insertTable(2, 2, { afterPara: 0 });
  d.insertTable(1, 1, { afterPara: last });
  const loc = d.locateTableCell(d.caret.para);
  assert(loc && loc.rootSlot === 1 && Number.isInteger(loc.row) && Number.isInteger(loc.cell),
    `游標在表 B 格內，locateTableCell 應反查 root_slot=1（實得 ${J(loc)}）`);
  assert(d.currentTableCell() !== null, 'currentTableCell() 游標在表內時不得回 null');
  assert(d.locateTableCell(0) === null, `表外正文段的反查必須回 null（實得 ${J(d.locateTableCell(0))}）`);
  okLog('  ✓ table_locator_json 反查：游標在表內命中該表、表外回 null');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 2｜L2 檔案選取器取消殘留', async () => {
  assert(/requestImageFilePick\s*\(\s*\)\s*\{/.test(uiSrc), 'UI 缺 requestImageFilePick()');
  assert(/addEventListener\(\s*'cancel'/.test(uiSrc),
    '檔案選取器只監聽 change：用戶按「取消」時只觸發 cancel 不觸發 change，旗標與 accept 永久殘留（開檔管線永久壞死），必須補 cancel 監聽');
  const reset = uiSrc.match(/(resetImagePickMode|_resetImagePickMode)\s*\(\s*\)\s*\{[\s\S]*?\n  \}/);
  assert(reset, 'UI 缺復原函數（resetImagePickMode：imagePickMode=false＋accept 回開檔白名單）');
  const resetBody = reset ? reset[0] : '';
  assert(/imagePickMode\s*=\s*false/.test(resetBody), '復原函數未清 imagePickMode 旗標');
  assert(/accept\s*=\s*['"]\.docx/.test(resetBody), '復原函數未把 accept 還原為開檔白名單');
  const openBranch = uiSrc.match(/type === 'file:open'[\s\S]{0,260}/);
  assert(openBranch && /resetImagePickMode/.test(openBranch[0]),
    '「檔案→開啟」處理前必須主動重置圖片選取態（殘留態不得讓圖片過濾污染開檔）');
  const changeBranch = uiSrc.match(/fileInput\.addEventListener\(\s*'change'[\s\S]{0,900}/);
  assert(changeBranch && (/imagePickMode\s*=\s*false/.test(changeBranch[0]) || /resetImagePickMode/.test(changeBranch[0])),
    'change 路徑需清旗標（直接清或呼復原函數皆可，既有行為不得丟）');
  const cancelBody = uiSrc.match(/addEventListener\(\s*'cancel'[\s\S]{0,300}/);
  assert(cancelBody && /resetImagePickMode/.test(cancelBody[0]), 'cancel 監聽需呼復原函數');
  note('取消態屬真實瀏覽器行為（cancel 時 change 不觸發），本燈以源碼狀態機鎖驗；瀏覽器閉環另見報告');
  okLog('  ✓ cancel 監聽＋file:open 前置重置齊備（取消不再殘留圖片過濾）');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 3｜L3 nested_steps serde 契約（真引擎反序列化）', async () => {
  const tp = buildTablePath(1, [{ row_idx: 0, cell_idx: 1, sub_table_idx: 0 }]);
  assert(JSON.stringify(tp.nested_steps) === '[[0,1,0]]',
    `buildTablePath 需把物件三元組正規化為數值陣列（實得 ${J(tp.nested_steps)}）`);
  assert(JSON.stringify(buildTablePath(2, [[3, 4, 5]]).nested_steps) === '[[3,4,5]]',
    '已是陣列者需原樣保留');
  let threw1 = false; try { buildTablePath(0, [{ row_idx: 0 }]); } catch { threw1 = true; }
  assert(threw1, '不完整的巢狀步必須直接拋錯（不得把壞形態靜默送引擎）');
  let threw2 = false; try { buildTablePath(0, [[0, 1]]); } catch { threw2 = true; }
  assert(threw2, '長度非 3 的巢狀步必須直接拋錯');

  const a = await fresh();
  a.insertTable(2, 2);
  const objForm = JSON.parse(a.engine.apply_mutation_json(JSON.stringify({
    SetCellProps: {
      table: { target: 'Body', root_slot: 0, nested_steps: [{ row_idx: 0, cell_idx: 0, sub_table_idx: 0 }] },
      row: 0, cell: 0, patch: { vertical_align: 'bottom' }
    }
  })));
  assert(objForm.ok === false && /invalid type: map, expected a tuple of size 3/.test(objForm.error || ''),
    `前提失實：引擎對物件形態 nested_steps 應回 serde 錯（實得 ${J(objForm)}）`);
  const normForm = JSON.parse(a.engine.apply_mutation_json(JSON.stringify({
    SetCellProps: {
      table: buildTablePath(0, [{ row_idx: 0, cell_idx: 0, sub_table_idx: 0 }]),
      row: 0, cell: 0, patch: { vertical_align: 'bottom' }
    }
  })));
  assert(normForm.ok === false && !/反序列化失敗/.test(normForm.error || ''),
    `正規化後的形態必須通過 serde（引擎只該抱怨巢狀表不存在，實得 ${J(normForm)}）`);
  okLog('  ✓ 物件形態引擎回 serde 錯（實錄），正規化形態通過反序列化');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 4｜L4 圖片單邊尺寸等比折算', async () => {
  const zlib = await import('node:zlib');
  const mkPng = (w, h) => {
    const raw = Buffer.alloc(h * (1 + w * 3));
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const o = y * (1 + w * 3) + 1 + x * 3; raw[o] = 210; raw[o + 1] = 60; raw[o + 2] = 60;
    }
    const crc32 = (buf) => { let c = ~0; for (const b of buf) { c ^= b; for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xEDB88320 & -(c & 1)); } return ~c >>> 0; };
    const chunk = (t, d) => { const l = Buffer.alloc(4); l.writeUInt32BE(d.length); const td = Buffer.concat([Buffer.from(t), d]); const cr = Buffer.alloc(4); cr.writeUInt32BE(crc32(td)); return Buffer.concat([l, td, cr]); };
    const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 2;
    return Buffer.concat([Buffer.from([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
  };
  const bytes = Uint8Array.from(mkPng(200, 100));   // 長寬比 2:1
  const a = await fresh();
  const dlImgs = () => { const o = []; for (let p = 0; p < a.pageCount(); p++) for (const it of a.getPageDisplayList(p).items) if (it.Image) o.push({ w: it.Image.w, h: it.Image.h }); return o; };
  const onlyW = a.insertInlineImage(bytes, 'image/png', { para: 0, cp: 0, widthPt: 100 });
  assert(onlyW.ok === true, `前置：只給 widthPt 應受理（${J(onlyW.error)}）`);
  assert(Math.abs(onlyW.placed.height_pt - 50) < 0.5,
    `只指定寬 100pt 的 2:1 圖，高必須等比折算為 50pt（實得 ${onlyW.placed.height_pt}pt → 非正方形圖被拉伸）`);
  // 顯示列表按繪製序不保證插入序——逐張差分比對，不抓 imgs[0]
  const imgsAfterW = dlImgs();
  assert(imgsAfterW.some(g => Math.abs(g.w - 100) < 0.5 && Math.abs(g.h - 50) < 0.5),
    `只指定寬的圖，顯示列表需有一張 100×50（實得 ${J(imgsAfterW)}）`);
  const beforeH = dlImgs();
  const onlyH = a.insertInlineImage(bytes, 'image/png', { para: 0, cp: 0, heightPt: 50 });
  assert(Math.abs(onlyH.placed.width_pt - 100) < 0.5,
    `只指定高 50pt 的 2:1 圖，寬必須等比折算為 100pt（實得 ${onlyH.placed.width_pt}pt）`);
  const afterH = dlImgs();
  const n100x50 = (arr) => arr.filter(g => Math.abs(g.w - 100) < 0.5 && Math.abs(g.h - 50) < 0.5).length;
  assert(afterH.length === beforeH.length + 1 && n100x50(afterH) === n100x50(beforeH) + 1,
    `只指定高的圖，顯示列表需新增一張 100×50（前後差分 ${J(beforeH)} → ${J(afterH)}）`);
  const both = a.insertInlineImage(bytes, 'image/png', { para: 0, cp: 0, widthPt: 90, heightPt: 90 });
  assert(both.placed.width_pt === 90 && both.placed.height_pt === 90,
    `兩邊都指定時必須照用戶值、不誤折算（實得 ${J(both.placed)}）`);
  const afterBoth = dlImgs();
  assert(afterBoth.length === afterH.length + 1 && afterBoth.some(g => g.w === 90 && g.h === 90),
    `兩邊指定 90×90 時顯示列表需照實有一張 90×90（實得 ${J(afterBoth)}）`);
  okLog('  ✓ 單邊指定按 natural 2:1 等比折算（100×50pt），兩邊指定照用戶值');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 5｜L5 刪列後游標校正（幽靈游標）', async () => {
  const a = await fresh();
  const last = a.paragraphCount() - 1;
  const ins = a.insertTable(2, 2, { afterPara: last });
  assert(ins.ok === true, `前置：末段後插 2x2 表應受理（${J(ins.error)}）`);
  const sum = JSON.parse(a.engine.tables_summary_json()).at(-1);
  a.caret = { para: sum.para_end, cp: 0 };
  const del = a.deleteTableRow({ rootSlot: sum.root_slot, row: sum.rows - 1 });
  assert(del.ok === true, `前置：刪末列應受理（${J(del.error)}）`);
  assert(del.outcome == null || del.outcome.caret_after == null,
    '前提：表格 Mutation 的 caret_after 實測恆 null（本燈存在的前提）');
  const pc = a.paragraphCount();
  assert(a.caret && a.caret.para < pc,
    `刪列後游標段落必須 < 段落總數（實得 caret=${J(a.caret)} paraCount=${pc} → 幽靈游標）`);
  const typed = a.typeAt(a.caret.para, a.caret.cp, '甲');
  assert(typed.ok === true, `刪列後緊接打字必須成功（實得 ${J({ ok: typed.ok, error: typed.error })}）`);
  // 真值斷言＝引擎回讀：幽靈游標修好的定義是「打字落在游標那段且引擎存到」。
  assert((a.paragraphText(a.caret.para) || '').includes('甲'),
    `校正後打字必須真的寫進游標段落（引擎回讀實得 ${J(a.paragraphText(a.caret.para).slice(0, 30))}）`);
  // DL 斷言補回（t_c2bbcb38 修好後加嚴；考卷可收窄不可弱化）。U4d 當年的實錄是
  // 「全新 2x2 表打字 → paragraph_text 有字、全頁 DL Text 命中 0」（引擎顯示缺口，
  // 本燈因此只鎖引擎回讀）。該缺口已由 t_c2bbcb38 修掉——切片文字隨打字刷新——
  // 故此燈從「只鎖回讀」升級為「回讀＋DL 雙鎖」：缺口復發時本燈立即紅。
  {
    const cellTexts = [];
    for (let p = 0; p < a.pageCount(); p++) {
      for (const it of a.getPageDisplayList(p).items) if (it.Text) cellTexts.push(it.Text.text);
    }
    assert(cellTexts.some((t) => t.includes('甲')),
      `表格儲存格打的「甲」必須出現在顯示列表 Text 項（引擎回讀有字、畫面卻看不見＝S1；` +
      `t_c2bbcb38 缺口，本燈 DL 文字項 ${cellTexts.length} 條零命中）`);
    okLog('  ✓ 表格儲存格文字 DL 命中（t_c2bbcb38 修復後補回的加嚴斷言）');
  }
  const b = await fresh();
  b.insertTable(2, 2);
  b.caret = { para: 0, cp: 0 };
  const before = J(b.caret);
  b.deleteTableRow({ rootSlot: 0, row: 1 });
  assert(J(b.caret) === before, `游標在表外正文段時刪列不應移動游標（${before} → ${J(b.caret)}）`);
  okLog('  ✓ 刪列後越界游標被校正、打字閉環通過；表外游標不動');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 6｜L6 activeTableSlot 真反查（禁盲設末張）', async () => {
  assert(!/activeTableSlot\s*=\s*this\.adapter\.tableCount\(\)\s*-\s*1/.test(uiSrc),
    'UI 仍盲設 activeTableSlot = tableCount()-1：游標在前段插表時新表不排在最後，會指向舊表');
  const a = await fresh();
  a.insertTable(2, 2);
  a.caret = { para: 0, cp: 0 };
  const b = a.insertTable(1, 1, { afterPara: 0 });
  assert(b.ok === true, `前置：游標前段插表應受理（${J(b.error)}）`);
  const truth = JSON.parse(a.engine.tables_summary_json());
  const newIsLast = truth.at(-1).rows === 1 && truth.at(-1).grid_cols === 1;
  const want = newIsLast ? truth.length - 1 : 0;
  assert(a.lastInsertedRootSlot === want,
    `新表實際 root_slot=${want}（引擎 summary 真值），adapter 反查需給對（實得 ${J(a.lastInsertedRootSlot)}）`);
  const rrs = uiSrc.match(/resolveRootSlot\s*\([\s\S]*?\n  \}/);
  assert(rrs, 'UI 缺 resolveRootSlot()');
  assert(/currentTableCell|locateTableCell|tableLocator/.test(rrs[0]),
    'resolveRootSlot 需先走游標反查（真索引），不得只在「最近操作／末張」之間猜');
  okLog(`  ✓ 游標前段插表：反查 root_slot=${J(a.lastInsertedRootSlot)}（引擎真值），UI 零盲設`);
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 7｜L7 table:shading 誠實標籤', async () => {
  const core = new QWordCore();
  const res = core.onCommand('table:shading', { rootSlot: 0, fill: 'E06699' }, 'live');
  assert(res.handled === true, `前置：合法底紋命令仍需受理（實得 ${J({ h: res.handled })}）`);
  assert(res.requirement !== 'live' && /^pending:/.test(res.requirement),
    `table:shading 無可見發射（引擎缺口 t_f0cbc25d），不得宣稱 live（實得 ${J(res.requirement)}）`);
  assert(/pending:engine/.test(res.requirement),
    `底紋缺口屬引擎面，標籤需掛 pending:engine-*（實得 ${J(res.requirement)}）`);
  const a = await fresh();
  a.insertTable(2, 3);
  const before = dlCount(a);
  const shd = a.setTableProps({ shadingFill: 'E06699' }, { rootSlot: 0 });
  assert(shd.ok === true, `引擎仍受理表格層底紋（XML 真寫入），實得 ${J(shd.error)}`);
  assert(dlCount(a) === before, `前提：表格層底紋零發射（顯示列表 ${before} → ${dlCount(a)}）`);
  const cellShd = a.setCellProps({ shadingFill: 'E06699' }, { rootSlot: 0, row: 0, cell: 0 });
  assert(cellShd.ok === true && dlCount(a) > before, '儲存格層底紋仍是可見效果（Rect 發射），不得被連坐降標');
  okLog('  ✓ table:shading 降 pending:engine-*（零發射實錄），儲存格層底紋不受影響');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 8｜L8 拒絕事件的 UI 監聽（非法輸入不得靜默）', async () => {
  for (const evt of ['table:row-col-rejected', 'table:props-rejected', 'image:inline-rejected']) {
    assert(new RegExp(`'${evt}'`).test(uiSrc), `UI 未接聽 ${evt}（core 派發了、畫面完全無反應）`);
    assert(new RegExp(`\\} else if \\(type === '${evt}'\\)`).test(uiSrc) || new RegExp(`case '${evt}'`).test(uiSrc),
      `${evt} 未接進 core 訂閱分支`);
  }
  const handlers = uiSrc.match(/handle\w*Rejected[\s\S]{0,900}/g) || [];
  assert(handlers.some(h => /showErrorModal/.test(h)), '拒絕事件處理器必須真的彈窗（showErrorModal）');
  assert(/problems/.test(uiSrc), '彈窗需把 core 給的 problems 明細呈現給用戶，不得只說「失敗」');
  const dispatched = [...coreSrc.matchAll(/_notify\('(table:row-col-rejected|table:props-rejected|image:inline-rejected)'/g)].map(m => m[1]);
  for (const evt of ['table:row-col-rejected', 'table:props-rejected', 'image:inline-rejected']) {
    assert(dispatched.includes(evt), `core 未派發 ${evt}（本燈前提失實）`);
  }
  okLog('  ✓ 三顆拒絕事件全數接住＋彈窗＋帶明細');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 9｜L9 引擎回傳 JSON 解析容錯', async () => {
  assert(/function safeEngineJson|const safeEngineJson\s*=|function safeJsonParse|const safeJsonParse\s*=/.test(adapterSrc),
    'adapter 缺引擎回傳安全解析封裝（safeEngineJson：壞字串回 null＋warn，不拋例外）');
  assert(/function engineMutationResult|const engineMutationResult\s*=/.test(adapterSrc),
    'mutation 路徑需有回 ok:false＋錯誤訊息 的容錯出口（不得把壞字串炸成例外，也不得靜默當成功）');
  const direct = [...adapterSrc.matchAll(/JSON\.parse\(\s*(?:this\.engine\.|resStr|jsonStr|str\b)/g)];
  assert(direct.length === 0,
    `仍有 ${direct.length} 處直接 JSON.parse(引擎回傳)：壞字串會把整個操作炸成未捕獲例外`);
  const a = await fresh();
  a.insertTable(1, 1);
  const orig = a.engine.apply_mutation_json.bind(a.engine);
  a.engine.apply_mutation_json = () => '{ this is not json';
  let threw = null, r = null;
  try { r = a.insertTableRow('After', { rootSlot: 0, atRow: 0 }); } catch (err) { threw = err; }
  a.engine.apply_mutation_json = orig;
  assert(threw === null, `引擎回壞字串不得拋例外到呼叫端（實得 ${threw && threw.message}）`);
  assert(r && r.ok === false && r.error, `引擎回壞字串時必須回 ok:false＋錯誤訊息（實得 ${J(r)}）`);
  okLog('  ✓ 壞字串注入：回 ok:false＋錯誤訊息，零未捕獲例外');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 10｜自我鑑別（摘除修復 → 對應燈必紅）', async () => {
  const discrim = [
    { tag: 'L1 探針回填', pick: 'adapter', mut: s => s.split('this.engine.tables_summary_json()').join('__probe_deleted()'), lock: shapeProbeLocks },
    { tag: 'L2 cancel 監聽摘除', pick: 'ui', mut: s => s.split("addEventListener('cancel'").join("addEventListener('never'"), lock: (u) => (/addEventListener\(\s*'cancel'/.test(u) ? [] : ['缺 cancel 監聽']) },
    { tag: 'L7 假 live 回填', pick: 'core', mut: s => s.split('engineGap').join('gapRemoved'), lock: (c) => (/spec\.gapRemoved/.test(c) && !/spec\.engineGap/.test(c) ? ['table:shading 的缺口標籤路徑被摘除＝又掛回 live'] : []) },
    { tag: 'L8 監聽摘除', pick: 'ui', mut: s => s.split("type === 'table:row-col-rejected'").join("type === '__dead__'"), lock: (u) => (/type === 'table:row-col-rejected'/.test(u) ? [] : ['UI 不再接聽拒絕事件']) },
    { tag: 'L6 盲設回填', pick: 'ui', mut: s => s.replace(/this\.activeTableSlot = this\.adapter\.lastInsertedRootSlot/, 'this.activeTableSlot = this.adapter.tableCount() - 1'), lock: (u) => (/activeTableSlot = this\.adapter\.tableCount\(\) - 1/.test(u) ? ['又盲設末張'] : []) }
  ];
  let allRed = true;
  for (const d of discrim) {
    const mutated = d.mut({ adapter: adapterSrc, ui: uiSrc, core: coreSrc }[d.pick]);
    const fails = d.lock(mutated);
    assert(fails.length > 0, `鑑別失效：摘除「${d.tag}」後考卷仍全綠（會放水）`);
    console.log(fails.length ? `  ✓ 鑑別 ${d.tag} → 紅（${fails.length} 項）` : `  ✗ 鑑別 ${d.tag} → 仍綠`);
    if (!fails.length) allRed = false;
  }
  assert(shapeProbeLocks(adapterSrc).length === 0, '真樹必須零違規（鑑別用的變異不可汙染源碼）');
  if (allRed) okLog('  ✓ 五種回塞態全紅，真樹零污染');
});

console.log('\n' + '='.repeat(72));
if (notes.length) console.log(`附註 ${notes.length} 項`);
if (pass) {
  console.log('自檢結果：PASS — U4d 九盞燈全綠（先紅後綠實錄見 docs/word-ui-u4d-fixes-report.md）');
  process.exit(0);
}
console.log(`自檢結果：FAIL（${errors.length} 項違規）`);
for (const e of errors) console.log(`  ✗ ${e}`);
process.exit(1);
