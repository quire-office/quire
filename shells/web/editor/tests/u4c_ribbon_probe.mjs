/**
 * u4c_ribbon_probe.mjs — U4c：表格 Ribbon 入口的瀏覽器閉環探針（正式入庫版）
 *
 * 為什麼在 node 探針之外再寫瀏覽器版（U4b 慣例）：本卡驗收口徑是「用戶看見
 * 那層」——Ribbon 按鈕真 DOM click → bindEvents → _tableCommandPayload
 * （游標反查 table_locator_json）→ core 驗證 → UI handler → adapter →
 * apply_mutation_json → 引擎重排 → page_display_list 指紋變化 → edit:undo
 * 還原。任一段斷掉指紋 delta 就是零，騙不了人。
 *
 * 用法（repo 根起靜態伺服；Chrome 需 --remote-debugging-port=9222）：
 *   python3 -m http.server 8791 --bind 127.0.0.1 &
 *   node shells/web/editor/tests/u4c_ribbon_probe.mjs
 * 環境變數：U4C_URL（默認 http://127.0.0.1:8791/shells/web/editor/index.html）、
 *           CDP_URL（默認 http://127.0.0.1:9222）、U4C_OUT（證據 JSON 路徑）。
 *
 * 判定口徑：
 * - 16 顆轉 live 的鈕：指紋必變（display-list 閉環）＋ undo 逐位還原。
 * - table:align-left 語義上是「回到預設態」，單點零位移屬 Word 同款行為——
 *   其位移實據走 right→left 拉回路徑（與 U4 收尾報告口徑一致）。
 * - table:shading 誠實掛 pending:engine-tblshading，證據三層：鈕標籤真 pending、
 *   點擊後指紋零變化、且直接帶 fill 下單時 XML 真的寫入 w:shd 而顯示列表零發射
 *   （寫入層≠發射層，E 卡同源缺口，非外殼可修）。
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const BASE_URL = process.env.U4C_URL || 'http://127.0.0.1:8791/shells/web/editor/index.html';
const OUT = process.env.U4C_OUT || path.join(ROOT, 'docs/word-ui-u4c-ribbon-evidence.json');
const CDP_HTTP = process.env.CDP_URL || 'http://127.0.0.1:9222';

/** 頁面 main world 內執行（序列化注入，禁閉包引用；baseUrl 由 CDP 端傳參）。 */
async function runInPage(baseUrl) {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const shell = () => window.QWordShell;
  const adapter = () => window.QWordEngine;

  const HARD_DEADLINE = Date.now() + 240000;
  const stillTime = () => Date.now() < HARD_DEADLINE;
  // 構造器只掛 shell/adapter 並預載 wasm；engine 實例要 openFile 後才存在
  // （loadFile→adapter.init()→openFile 鏈，見 qword-ui.js:572-596），故先等
  // 外殼就緒即可開單，不得先等 engine——那是死等。
  while (stillTime()) {
    if (shell() && adapter()) break;
    await sleep(100);
  }
  if (!shell() || !adapter()) throw new Error('外殼未就緒（QWordShell 未掛載）');

  // 重置到開檔態（清掉任何前次操作殘留），再等文件真的就緒
  await shell().openDefaultSample();
  while (stillTime()) {
    if (adapter().engine && adapter().paragraphCount() > 0) break;
    await sleep(200);
  }
  if (!adapter().engine) throw new Error('開檔失敗：引擎實例未建立（見頁面錯誤彈窗/console）');
  await sleep(400);

  // ---- display-list 指紋：逐項型別＋幾何＋顏色（用戶看見那層的完整狀態）----
  const allItems = () => {
    const out = [];
    for (let p = 0; p < adapter().pageCount(); p += 1) {
      for (const it of adapter().getPageDisplayList(p).items) out.push(it);
    }
    return out;
  };
  const itemStr = (it) => {
    const k = Object.keys(it)[0]; const v = it[k];
    if (k === 'Text') return `T|${(v.text || '').slice(0, 12)}|${v.x.toFixed(2)}|${v.y.toFixed(2)}|sz${v.size_pt}|f${v.flags}|c${(v.color_argb >>> 0).toString(16)}`;
    if (k === 'Rect') return `R|${v.x.toFixed(2)}|${v.y.toFixed(2)}|${v.w.toFixed(2)}|${v.h.toFixed(2)}|${(v.fill_argb >>> 0).toString(16)}`;
    if (k === 'Rule') return `L|${v.x1.toFixed(2)}|${v.y1.toFixed(2)}|${v.x2.toFixed(2)}|${v.y2.toFixed(2)}|w${v.width_pt}`;
    if (k === 'Image') return `I|${v.x.toFixed(2)}|${v.y.toFixed(2)}|${(v.w ?? 0).toFixed(2)}`;
    return `${k}|${JSON.stringify(v).slice(0, 60)}`;
  };
  const stateOf = () => {
    const items = allItems();
    const strs = items.map(itemStr);
    const texts = items.filter((i) => i.Text).map((i) => i.Text);
    return {
      strs,
      fp: strs.join('\n'),
      total: items.length,
      texts: texts.length,
      rects: items.filter((i) => i.Rect).length,
      rules: items.filter((i) => i.Rule).length,
      minTextX: texts.length ? Math.min(...texts.map((t) => Math.round(t.x * 10) / 10)) : null
    };
  };
  /** 逐位 diff：回前 4 筆不同的項目（型別＋前後值），是「哪項動了」的直接實據 */
  const diffItems = (a, b) => {
    const out = [];
    const n = Math.max(a.strs.length, b.strs.length);
    for (let i = 0; i < n && out.length < 4; i += 1) {
      if (a.strs[i] !== b.strs[i]) out.push({ i, before: a.strs[i] || '(無)', after: b.strs[i] || '(消失)' });
    }
    return out;
  };

  // 游標→格反查（與 qword-ui._caretCellTarget 同源綁定，座標全部引擎給）
  const locatorOf = (p) => { try { return JSON.parse(adapter().engine.table_locator_json(p)); } catch { return null; } };
  const caretIntoCell = (row, col) => {
    for (let p = 0; p < adapter().paragraphCount(); p += 1) {
      const j = locatorOf(p);
      if (j && j.cell_path && j.cell_path.row === row && j.cell_path.cell === col) {
        adapter().caret = { para: p, cp: 0 };
        return true;
      }
    }
    return false;
  };
  const cellParas = (row, col) => {
    const hits = [];
    for (let p = 0; p < adapter().paragraphCount(); p += 1) {
      const j = locatorOf(p);
      if (j && j.cell_path && j.cell_path.row === row && j.cell_path.cell === col) hits.push(p);
    }
    return hits;
  };

  const tableShape = () => adapter().tableShape(0);
  // 誠實用戶路徑：table 家族鈕在「插入」分頁、面板鈕需先開面板——隱藏態
  // btn.click() 的事件鏈雖相同，但 visible=false 就不是「用戶看得見」的證據。
  const showHome = () => shell().onCommand('tab:switch:tab-home', {});
  const showInsert = () => shell().onCommand('tab:switch:tab-insert', {});
  const openPropsPanel = () => {
    const pop = document.getElementById('popover-table-props');
    if (pop && pop.classList.contains('open')) return; // 冪等：已開不重複點（重點會關）
    const t = document.querySelector('[data-popover-target="popover-table-props"]');
    if (t) t.click();
  };
  const clickBtn = (sel) => {
    const btn = document.querySelector(sel);
    if (!btn) return { found: false };
    const r = btn.getBoundingClientRect();
    btn.click();
    return { found: true, visible: r.width > 0 && r.height > 0, req: btn.getAttribute('data-requirement') };
  };
  const undoOnce = async () => {
    const res = shell().onCommand('edit:undo', {});
    await sleep(80);
    return res;
  };

  const R = { meta: { baseUrl: baseUrl, at: new Date().toISOString(), ua: navigator.userAgent }, buttons: {}, notes: {} };

  // ---- 基準：插入 3×3 測試表（經 Ribbon 同款指令鏈），游標釘進 (1,1) ----
  const pre = shell().onCommand('table:grid-select', { rows: 3, cols: 3 });
  await sleep(300);
  R.notes.setup = { dispatchHandled: pre.handled, tableCount: adapter().tableCount(), shape: tableShape(), paragraphs: adapter().paragraphCount() };
  if (!caretIntoCell(1, 1)) throw new Error('游標無法進入 (1,1) 格（locator 綁定失靈）');
  const baseline = stateOf();
  R.notes.baseline = { total: baseline.total, texts: baseline.texts, rects: baseline.rects, rules: baseline.rules, minTextX: baseline.minTextX };

  // 通用單鈕探針：基準態→真點擊→後態→diff→undo→還原態
  const probe = async (key, cmd, opts = {}) => {
    if (opts.tab === 'insert') { showInsert(); await sleep(60); }
    else if (opts.tab === 'home') { showHome(); await sleep(60); }
    if (opts.propsPanel) { openPropsPanel(); await sleep(60); }
    if (opts.reenter) caretIntoCell(1, 1);
    const before = stateOf();
    const shape0 = opts.shape ? tableShape() : null;
    const click = clickBtn(`[data-command="${cmd}"]`);
    await sleep(opts.wait || 150);
    const after = stateOf();
    const e = {
      selector: `[data-command="${cmd}"]`, ...click,
      before: { total: before.total, texts: before.texts, rects: before.rects, rules: before.rules, minTextX: before.minTextX },
      after: { total: after.total, texts: after.texts, rects: after.rects, rules: after.rules, minTextX: after.minTextX },
      fpChanged: before.fp !== after.fp,
      diffHead: diffItems(before, after)
    };
    if (opts.shape) {
      const shape1 = tableShape();
      e.shapeBefore = shape0; e.shapeAfter = shape1;
      e.shapeDelta = { rows: (shape1.ok && shape0.ok) ? shape1.rows - shape0.rows : null, cols: (shape1.ok && shape0.ok) ? shape1.cols - shape0.cols : null };
    }
    if (opts.undo !== false) {
      const u = await undoOnce();
      const back = stateOf();
      e.undo = { dispatched: !!u.handled, restoredFp: back.fp === before.fp, total: back.total };
      if (opts.shape) e.undoShape = tableShape();
    }
    R.buttons[key] = e;
    return e;
  };

  // ── 行列 8 顆：指紋位移＋行列數增減＋undo 逐位還原 ──
  await probe('table:row-insert-above', 'table:row-insert-above', { shape: true, reenter: true, tab: 'insert' });
  await probe('table:row-insert-below', 'table:row-insert-below', { shape: true, reenter: true, tab: 'insert' });
  await probe('table:row-insert', 'table:row-insert', { shape: true, reenter: true, tab: 'insert' });
  await probe('table:row-delete', 'table:row-delete', { shape: true, reenter: true, tab: 'insert' });
  await probe('table:col-insert-left', 'table:col-insert-left', { shape: true, reenter: true, tab: 'insert' });
  await probe('table:col-insert-right', 'table:col-insert-right', { shape: true, reenter: true, tab: 'insert' });
  await probe('table:col-insert', 'table:col-insert', { shape: true, reenter: true, tab: 'insert' });
  await probe('table:col-delete', 'table:col-delete', { shape: true, reenter: true, tab: 'insert' });

  // ── 表格對齊 3 顆：整表 Text 項 x 位移；left 走 right→left 拉回路徑 ──
  await probe('table:align-center', 'table:align-center', { reenter: true, tab: 'insert', propsPanel: true });
  await probe('table:align-right', 'table:align-right', { reenter: true, tab: 'insert', propsPanel: true });
  {
    // 先點 right 建立右移態（不 undo），再點 left 拉回基準——left 鈕的位移實據
    showInsert(); await sleep(60); openPropsPanel(); await sleep(60);
    caretIntoCell(1, 1);
    const before = stateOf();
    clickBtn('[data-command="table:align-right"]');
    await sleep(150);
    const righted = stateOf();
    caretIntoCell(1, 1);
    clickBtn('[data-command="table:align-left"]');
    await sleep(150);
    const backed = stateOf();
    R.buttons['table:align-left'] = {
      selector: '[data-command="table:align-left"]',
      found: !!document.querySelector('[data-command="table:align-left"]'),
      visible: (() => { const el = document.querySelector('[data-command="table:align-left"]'); if (!el) return false; const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; })(),
      req: document.querySelector('[data-command="table:align-left"]')?.getAttribute('data-requirement') || null,
      path: 'right→left 拉回（left=預設態，單點零位移屬 Word 同款語義）',
      rightedMinTextX: righted.minTextX, backedMinTextX: backed.minTextX,
      rightShifted: righted.fp !== before.fp,
      leftPullsBack: backed.fp !== righted.fp,
      backToBaselineFp: backed.fp === before.fp,
      diffHead: diffItems(righted, backed)
    };
    const u = await undoOnce(); // 殘留兩筆（align-right＋align-left），必須雙次 undo 才回本區塊起點
    const u2 = await undoOnce();
    R.buttons['table:align-left'].cleanupUndo = [!!u.handled, !!u2.handled];
  }

  // ── 表格寬度：面板 input 填 120pt → 套用寬度鈕（真點擊路徑）──
  {
    showInsert(); await sleep(60); openPropsPanel(); await sleep(60);
    caretIntoCell(1, 1);
    const input = document.getElementById('table-width-input');
    const before = stateOf();
    if (input) input.value = '120';
    const click = clickBtn('[data-command="table:width"]');
    await sleep(150);
    const after = stateOf();
    const e = { selector: '[data-command="table:width"]', ...click, widthInputSeen: input ? input.value : null,
      before: { total: before.total, minTextX: before.minTextX }, after: { total: after.total, minTextX: after.minTextX },
      fpChanged: before.fp !== after.fp, diffHead: diffItems(before, after) };
    const u = await undoOnce();
    const back = stateOf();
    e.undo = { dispatched: !!u.handled, restoredFp: back.fp === before.fp };
    R.buttons['table:width'] = e;
  }

  // ── 儲存格底紋：觸發鈕開面板→色塊真點擊→Rect +1→undo 還原 ──
  {
    showInsert(); await sleep(60);
    caretIntoCell(1, 1);
    const before = stateOf();
    const trigger = document.querySelector('[data-command="cell:shading"]');
    if (trigger) trigger.click();
    await sleep(80);
    const swatch = document.querySelector('#popover-cell-shading .qword-color-grid [data-color]');
    if (swatch) swatch.click();
    await sleep(150);
    const after = stateOf();
    const e = { selector: '#popover-cell-shading .qword-color-grid [data-color]',
      triggerFound: !!trigger, swatchFound: !!swatch, swatchColor: swatch ? swatch.getAttribute('data-color') : null,
      popoverPresent: !!document.getElementById('popover-cell-shading'),
      before: { total: before.total, rects: before.rects }, after: { total: after.total, rects: after.rects },
      rectDelta: after.rects - before.rects, fpChanged: before.fp !== after.fp,
      newRect: (after.strs.find((s, i) => s.startsWith('R|') && before.strs[i] !== s) || null) };
    const u = await undoOnce();
    const back = stateOf();
    e.undo = { dispatched: !!u.handled, restoredFp: back.fp === before.fp, rects: back.rects };
    R.buttons['cell:shading'] = e;
  }
  // ── 清除底紋：先套色（基準+1 Rect）→ 清除鈕 → 指紋回基準（fill=auto）──
  {
    showInsert(); await sleep(60);
    caretIntoCell(1, 1);
    const trigger = document.querySelector('[data-command="cell:shading"]');
    if (trigger) trigger.click();
    await sleep(60);
    const swatch = document.querySelector('#popover-cell-shading .qword-color-grid [data-color]');
    if (swatch) swatch.click();
    await sleep(150);
    const tinted = stateOf();
    // 誠實用戶路徑：套色後面板已關（swatch handler 收合），重新開面板再點清除
    const trigger2 = document.querySelector('[data-command="cell:shading"]');
    if (trigger2) trigger2.click();
    await sleep(80);
    const click = clickBtn('[data-cell-shading-clear]');
    await sleep(150);
    const after = stateOf();
    R.buttons['cell:shading-clear'] = { selector: '[data-cell-shading-clear]', ...click,
      tintedRects: tinted.rects, afterRects: after.rects,
      clearsToBaseline: after.fp === baseline.fp };
  }

  // ── v-align 3 顆：固定列高 40pt 前置（自動列高時行高貼字、位移 0＝Word 同語義）──
  showHome(); await sleep(60);
  {
    const ins = adapter().engine.apply_mutation_json(JSON.stringify({
      SetRowProps: { table: { target: 'Body', root_slot: 0, nested_steps: [] }, row: 1, patch: { height_pt: 40, header: null, cant_split: null } }
    }));
    R.notes.rowHeightPrecondition = JSON.parse(ins).ok === true;
  }
  const valignProbe = async (cmd) => {
    caretIntoCell(1, 1);
    const paras = cellParas(1, 1);
    const before = stateOf();
    const click = clickBtn(`[data-command="${cmd}"]`);
    await sleep(150);
    const after = stateOf();
    const e = { selector: `[data-command="${cmd}"]`, ...click, cellParas: paras,
      fpChanged: before.fp !== after.fp, diffHead: diffItems(before, after) };
    const u = await undoOnce();
    const back = stateOf();
    e.undo = { dispatched: !!u.handled, restoredFp: back.fp === before.fp };
    R.buttons[cmd] = e;
  };
  await valignProbe('cell:v-align-bottom');
  await valignProbe('cell:v-align-center');
  // v-align-top＝引擎預設態（w:vAlign 未宣告＝top，Word 同款語義），單點零位移
  // 不是壞事——其位移實據走 bottom→top 拉回路徑（與 align-left 同款口徑）。
  {
    caretIntoCell(1, 1);
    const before = stateOf();
    // 先釘到 bottom（不 undo）建立位移態
    const triggerBottom = document.querySelector('[data-command="cell:v-align-bottom"]');
    if (triggerBottom) triggerBottom.click();
    await sleep(150);
    const bottomed = stateOf();
    caretIntoCell(1, 1);
    const el = document.querySelector('[data-command="cell:v-align-top"]');
    const r = el ? el.getBoundingClientRect() : null;
    if (el) el.click();
    await sleep(150);
    const backed = stateOf();
    R.buttons['cell:v-align-top'] = {
      selector: '[data-command="cell:v-align-top"]',
      found: !!el, visible: !!(r && r.width > 0 && r.height > 0),
      req: el ? el.getAttribute('data-requirement') : null,
      path: 'bottom→top 拉回（top=引擎預設態，單點零位移屬 Word 同款語義）',
      bottomShifted: bottomed.fp !== before.fp,
      topPullsBack: backed.fp !== bottomed.fp,
      backToBaselineFp: backed.fp === before.fp,
      diffHead: diffItems(bottomed, backed)
    };
    const u = await undoOnce(); // 殘留兩筆（bottom＋top），雙次 undo 回起點
    const u2 = await undoOnce();
    R.buttons['cell:v-align-top'].cleanupUndo = [!!u.handled, !!u2.handled];
  }

  // ── table:shading：誠實 pending 三層證據（標籤／零發射／寫入真發射零）──
  {
    showInsert(); await sleep(60);
    caretIntoCell(1, 1);
    const before = stateOf();
    const click = clickBtn('[data-command="table:shading"]');
    await sleep(150);
    const after = stateOf();
    // 直接帶 fill 下單（繞過面板）：引擎受理、XML 寫入、顯示列表仍零發射
    const direct = shell().onCommand('table:shading', { fill: '#E06699', rootSlot: 0 });
    await sleep(150);
    const afterDirect = stateOf();
    // 存檔面：save() → ZIP 本機解析（DecompressionStream deflate-raw）
    let xmlHasShd = null;
    try {
      const buf = new Uint8Array(adapter().engine.save_docx_bytes());
      const dv = new DataView(buf.buffer);
      for (let i = 0; i + 4 <= buf.length; i += 1) {
        if (dv.getUint32(i, true) !== 0x04034b50) continue;
        const nameLen = dv.getUint16(i + 26, true);
        const extraLen = dv.getUint16(i + 28, true);
        const cSize = dv.getUint32(i + 18, true);
        const name = new TextDecoder().decode(buf.subarray(i + 30, i + 30 + nameLen));
        if (name !== 'word/document.xml') continue;
        const start = i + 30 + nameLen + extraLen;
        const ds = new DecompressionStream('deflate-raw');
        const stream = new Blob([buf.subarray(start, start + cSize)]).stream().pipeThrough(ds);
        const text = await new Response(stream).text();
        xmlHasShd = /w:shd[^>]*w:fill="E06699"/i.test(text);
        break;
      }
    } catch (err) { xmlHasShd = `解析失敗:${err.message}`; }
    R.buttons['table:shading'] = { selector: '[data-command="table:shading"]', ...click,
      btnClickFpChanged: before.fp !== after.fp,
      directDispatch: { handled: direct.handled, requirement: direct.requirement },
      directFpChanged: before.fp !== afterDirect.fp,
      directRects: { before: before.rects, after: afterDirect.rects },
      xmlHasShd,
      honestPendingProof: click.req === 'pending:engine-tblshading' && before.fp === after.fp && afterDirect.fp === before.fp };
    const u = await undoOnce();
    R.buttons['table:shading'].cleanupUndo = !!u.handled;
  }

  // ---- 收尾還原：undo 排到基準態（測試表保留，其餘操作全數還原）----
  let guard = 0;
  while (guard < 60) {
    const cur = stateOf();
    if (cur.fp === baseline.fp) break;
    const u = await undoOnce();
    if (!u.handled) break;
    guard += 1;
  }
  R.notes.finalDrain = { loops: guard, restoredToBaseline: stateOf().fp === baseline.fp };

  // ---- 入口盤點：data-command 逐鈕＋輔助鈕＋標籤分佈 ----
  const cmds = [...document.querySelectorAll('[data-command^="table:"],[data-command^="cell:"]')]
    .map((b) => ({ cmd: b.getAttribute('data-command'), req: b.getAttribute('data-requirement') }));
  R.notes.inventory = {
    commandButtons: cmds,
    uniqueCommands: [...new Set(cmds.map((c) => c.cmd))].sort(),
    auxButtons: {
      propsPanelTrigger: !!document.querySelector('[data-popover-target="popover-table-props"]'),
      shadingClear: !!document.querySelector('[data-cell-shading-clear]')
    },
    pendingTagged: cmds.filter((c) => (c.req || '').startsWith('pending:'))
  };
  return R;
}

// --------------------------------------------------------------------------
// CDP 傳輸層：連 127.0.0.1:9222，開專用新 tab，跑完即關
// --------------------------------------------------------------------------
async function main() {
  const WS = globalThis.WebSocket;
  if (!WS) throw new Error('需要 Node >= 22 的全域 WebSocket');

  const created = await (await fetch(`${CDP_HTTP}/json/new?${encodeURIComponent(BASE_URL)}`, { method: 'PUT' })).json();
  const sock = new WS(created.webSocketDebuggerUrl);
  let id = 0;
  const pending = new Map();
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const mid = ++id;
    pending.set(mid, { resolve, reject });
    sock.send(JSON.stringify({ id: mid, method, params }));
  });
  await new Promise((res, rej) => { sock.onopen = res; sock.onerror = (e) => rej(new Error(`CDP ws 連線失敗 ${String(e && e.message || e)}`)); });
  sock.onmessage = (ev) => {
    const msg = JSON.parse(typeof ev.data === 'string' ? ev.data : ev.data.toString());
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(`${msg.error.message} (${msg.error.code})`));
      else resolve(msg.result);
    }
  };

  await send('Page.enable');
  await send('Runtime.enable');
  for (let i = 0; i < 240; i += 1) {
    const r = await send('Runtime.evaluate', { expression: 'document.readyState', returnByValue: true });
    if (r.result.value === 'complete') break;
    await new Promise((res) => setTimeout(res, 250));
  }
  const out = await send('Runtime.evaluate', {
    expression: `(${runInPage.toString()})(${JSON.stringify(BASE_URL)})`,
    returnByValue: true, awaitPromise: true, timeout: 600000
  });
  try {
    const shot = await send('Page.captureScreenshot', { format: 'png' });
    if (shot && shot.data) fs.writeFileSync(OUT.replace(/\.json$/, '.png'), Buffer.from(shot.data, 'base64'));
  } catch {}
  try { await fetch(`${CDP_HTTP}/json/close/${created.id}`); } catch {}
  sock.close();

  if (out.exceptionDetails) {
    console.error('探針拋例外：', JSON.stringify(out.exceptionDetails).slice(0, 3000));
    process.exit(2);
  }
  const value = out.result.value;
  fs.writeFileSync(OUT, JSON.stringify(value, null, 2));
  console.log(`證據檔 → ${OUT}`);

  // ---- 紅綠板 ----
  let fail = 0;
  const line = (cond, msg) => { console.log(`  ${cond ? '✓' : '✗'} ${msg}`); if (!cond) fail += 1; };
  console.log('=== U4c Ribbon 瀏覽器閉環判定 ===');
  console.log('setup:', JSON.stringify(value.notes.setup));
  console.log('baseline:', JSON.stringify(value.notes.baseline));
  const b = value.buttons;
  const rowCol = ['table:row-insert-above', 'table:row-insert-below', 'table:row-insert', 'table:row-delete',
    'table:col-insert-left', 'table:col-insert-right', 'table:col-insert', 'table:col-delete'];
  for (const k of rowCol) {
    const e = b[k];
    line(!!e && e.found && e.fpChanged && e.undo && e.undo.restoredFp,
      `${k} found=${!!e && e.found} req=${e ? e.req : '-'} fp=${!!e && e.fpChanged} shapeΔ=${e && e.shapeDelta ? JSON.stringify(e.shapeDelta) : '-'} undo=${e && e.undo ? e.undo.restoredFp : '-'}`);
  }
  line(!!b['table:align-center'] && b['table:align-center'].fpChanged, `table:align-center fp位移=${!!b['table:align-center'] && b['table:align-center'].fpChanged}`);
  line(!!b['table:align-right'] && b['table:align-right'].fpChanged, `table:align-right fp位移=${!!b['table:align-right'] && b['table:align-right'].fpChanged}`);
  line(!!b['table:align-left'] && b['table:align-left'].leftPullsBack && b['table:align-left'].backToBaselineFp,
    `table:align-left right→left 拉回=${!!b['table:align-left'] && b['table:align-left'].leftPullsBack} 回基準=${!!b['table:align-left'] && b['table:align-left'].backToBaselineFp}`);
  line(!!b['table:width'] && b['table:width'].fpChanged && b['table:width'].undo.restoredFp, `table:width(120pt) fp=${!!b['table:width'] && b['table:width'].fpChanged} undo=${!!b['table:width'] && b['table:width'].undo.restoredFp}`);
  line(!!b['cell:shading'] && b['cell:shading'].rectDelta === 1 && b['cell:shading'].undo.restoredFp,
    `cell:shading RectΔ=${!!b['cell:shading'] && b['cell:shading'].rectDelta} undo=${!!b['cell:shading'] && b['cell:shading'].undo.restoredFp}`);
  line(!!b['cell:shading-clear'] && b['cell:shading-clear'].clearsToBaseline, `cell 清除底紋回基準=${!!b['cell:shading-clear'] && b['cell:shading-clear'].clearsToBaseline}`);
  for (const k of ['cell:v-align-bottom', 'cell:v-align-center']) {
    line(!!b[k] && b[k].fpChanged && b[k].undo.restoredFp, `${k} fp=${!!b[k] && b[k].fpChanged} undo=${!!b[k] && b[k].undo.restoredFp}`);
  }
  line(!!b['cell:v-align-top'] && b['cell:v-align-top'].bottomShifted && b['cell:v-align-top'].topPullsBack && b['cell:v-align-top'].backToBaselineFp,
    `cell:v-align-top bottom→top 拉回=${!!b['cell:v-align-top'] && b['cell:v-align-top'].topPullsBack} 回基準=${!!b['cell:v-align-top'] && b['cell:v-align-top'].backToBaselineFp}`);
  line(!!b['table:shading'] && b['table:shading'].honestPendingProof,
    `table:shading 誠實pending（標籤/零發射/XML寫入）= ${b['table:shading'] ? JSON.stringify({ req: b['table:shading'].req, fp0: !b['table:shading'].directFpChanged, xml: b['table:shading'].xmlHasShd }) : '無證據'}`);
  console.log('收尾還原:', JSON.stringify(value.notes.finalDrain));
  console.log(`\n判定：${fail === 0 ? 'PASS' : `FAIL（${fail} 項）`}`);
  process.exit(fail === 0 ? 0 : 1);
}

main().catch((e) => { console.error('探針失敗:', e.message); process.exit(2); });
