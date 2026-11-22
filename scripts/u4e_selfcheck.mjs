#!/usr/bin/env node
/**
 * u4e_selfcheck.mjs
 * U4e：外殼返修（agy 批次審查坐實 × PM 讀碼覆核）的驗收考卷
 *
 * 先紅後綠紀律：本考卷在修複前四盞必紅、修複後全綠，每一盞都帶實測輸出。
 * 考卷只收窄不弱化：斷言讀「引擎真值」與「源碼狀態機」，禁讀自報旗標。
 *
 * 四盞燈（對卡面四條）：
 *   S1  巢狀表游標→靜默改外層根表：游標在巢狀表內時 resolveRootSlot 取
 *       不過濾巢狀的 adapter.currentTableCell().rootSlot（外層），用戶點
 *       「刪除列」刪掉的是外層根表末列（無任何提示）。
 *   S2  undo()/redo() 漏游標夾限：引擎 undo/redo 的 outcome.caret_after 同為
 *       null，_applyTableMutation 的越界校正在 undo/redo 路徑完全缺席——
 *       插表後游標在末列按 Undo 立即重現幽靈游標；且舊校正只夾 para 不夾 cp、
 *       不清 selection。
 *   S3  換檔殘留上一檔表格狀態：openFile() 只重置 caret，lastInsertedRootSlot／
 *       selection 殘留；loadFile() 零重置，activeTableSlot 殘留——新檔多表＋
 *       游標表外時命中 resolveRootSlot 的 activeTableSlot<count 分支，繞過
 *       U4d L6 防線靜默操作新檔的舊槽號表。
 *   S4  buildTablePath 把 null/""/false 靜默轉 0（Number(null)===Number("")
 *       ===Number(false)===0），且 throw 路徑在 _resolveTablePath 呼叫鏈無
 *       try/catch，觸發即洩漏未捕獲例外。
 *
 * 真值口徑（全部由真引擎探針實測取得，見 docs/word-ui-u4e-fixes-report.md）：
 * - 巢狀表反查：2x2 表儲存格段落再插 1x1 表後，table_locator_json 對巢狀格段落
 *   回 {table_path:{root_slot:0, nested_steps:[[0,0,0]]}}——root_slot 是**外層**
 *   根表；舊版 resolveRootSlot 丟棄 nestedSteps 以純整數 0 傳下去（紅燈實錄：
 *   resolveRootSlot 實得 0，隨後 deleteTableRow(rootSlot:0,row:1) ok=true，
 *   外層表 rows 2→1、para_cells 5→3）。
 * - InsertTable 後 undo：outcome.caret_after 實測 null（插表前 caret={para:14}，
 *   undo 後 paraCount 20→14，caret 仍 {para:14}），緊接 typeAt 引擎回
 *   「段落索引越界: 請求 14，總段數 14」（幽靈游標實錄）。
 * - 換檔實錄：開檔後 openFile(sample) 實得 lastInsertedRootSlot=0、
 *   selection={start:{para:1,cp:0},...} 全部殘留。
 * - buildTablePath 實錄：null/''/false → root_slot 0、{} → root_slot null；
 *   nested 物件 {row_idx:null,cell_idx:'',sub_table_idx:false} → [[0,0,0]]。
 * - 引擎 locator 拋例外時 insertTableRow 直接把例外洩漏到呼叫端（實錄 threw）。
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

const ADAPTER_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/engine-adapter.js');
const UI_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/qword-ui.js');

let pass = true;
const errors = [];
const notes = [];
function assert(cond, msg) { if (!cond) { errors.push(msg); pass = false; } }
function note(msg) { notes.push(msg); (lampLog || rawLog)(`  [NOTE] ${msg}`); }

/**
 * 一盞燈一包：擲例外也算紅（考卷不得因某盞燈崩掉而少報其他盞）。
 * 誠實紀律：燈內的成功列只在「本燈目前零違規」時才放行——本燈已紅卻還印 ✓
 * 就是把失敗寫成成功（沿用 u4d 考卷同款防線）。
 */
let lampBase = 0;
const rawLog = console.log.bind(console);
let lampLog = null;
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
console.log('U4e 自檢：巢狀表跨層守衛／換檔・Undo 狀態殘留／路徑型別強制（四盞燈）');
console.log('='.repeat(72));

const adapterSrc = fs.readFileSync(ADAPTER_JS_PATH, 'utf-8');
const uiSrc = fs.readFileSync(UI_JS_PATH, 'utf-8');
const J = (x) => JSON.stringify(x);

const adapterModule = await import(ADAPTER_JS_PATH);
const uiModule = await import(UI_JS_PATH);
const { EngineAdapter, getDefaultSampleBytes, buildTablePath } = adapterModule;
const { QWordUI } = uiModule;

async function fresh() {
  const a = new EngineAdapter();
  await a.init();
  a.openFile(getDefaultSampleBytes(), 'sample.docx');
  return a;
}

/** 類方法體擷取：行首兩空格＋方法名( 定位，到第一個兩空格結尾 }（u4d 同款）。 */
function methodBody(src, name) {
  const re = new RegExp(`\\n {2}(?:async )?${name}\\s*\\(`);
  const m = re.exec(src);
  if (!m) return null;
  const end = src.indexOf('\n  }', m.index);
  return src.slice(m.index, end < 0 ? m.index + 1600 : end + 4);
}
/** 頂層函數體擷取：export function name 到第一個行首 }。 */
function topLevelBody(src, name) {
  const m = new RegExp(`export function ${name}\\s*\\(`).exec(src);
  if (!m) return null;
  const end = src.indexOf('\n}', m.index);
  return src.slice(m.index, end < 0 ? m.index + 2000 : end + 2);
}

// ── 源碼狀態機鎖（純函數，自我鑑別段复用）─────────────────────────────────
/** S1 守衛鎖：resolveRootSlot 取 located 後必擋巢狀（旗標＋回 null）。 */
function s1GuardLock(ui) {
  const fails = [];
  const body = methodBody(ui, 'resolveRootSlot');
  if (!body) { return ['UI 缺 resolveRootSlot()']; }
  if (!/located\.nestedSteps[\s\S]{0,160}?_nestedCaretBlocked\s*=\s*true[\s\S]{0,80}?return null/.test(body)) {
    fails.push('resolveRootSlot 未守衛巢狀表（located.nestedSteps 非空時須記旗標並回 null，不得沿用外層 rootSlot 靜默改錯表）');
  }
  if (!/_nestedCaretBlocked\s*=\s*false/.test(body)) {
    fails.push('resolveRootSlot 進入時未清除 _nestedCaretBlocked 旗標（殘留旗標會誤擋後續合法操作）');
  }
  return fails;
}
/** S1 彈窗鎖：_requireTableTarget 的巢狀拒絕路徑（誠實拒＋明細彈窗）。 */
function s1ModalLock(ui) {
  const fails = [];
  const body = methodBody(ui, '_requireTableTarget');
  if (!body) { return ['UI 缺 _requireTableTarget()']; }
  if (!/_nestedCaretBlocked/.test(body)) fails.push('_requireTableTarget 未接巢狀拒絕旗標');
  if (!/巢狀表內的操作尚未支援，請直接點外層表格/.test(body)) {
    fails.push('巢狀拒絕未彈窗說明「巢狀表內的操作尚未支援，請直接點外層表格」（誠實拒勝過靜默改錯表）');
  }
  if (!/_reportEngineRejection/.test(body.match(/_nestedCaretBlocked[\s\S]*/)?.[0] || '')) {
    fails.push('巢狀拒絕需補一筆外殼拒絕日誌（不得只彈窗不記帳）');
  }
  return fails;
}
/** S2 夾限鎖：undo／redo／_applyTableMutation 三處統一調用 _clampCaretAndSelection。 */
function s2ClampLock(adapter) {
  const fails = [];
  const clamp = methodBody(adapter, '_clampCaretAndSelection');
  if (!clamp) fails.push('adapter 缺 _clampCaretAndSelection()（para／cp／selection 三面統一校正）');
  else {
    if (!/paragraphCount\(\)/.test(clamp)) fails.push('_clampCaretAndSelection 未依 paragraphCount() 夾 para');
    if (!/Array\.from\(/.test(clamp)) fails.push('_clampCaretAndSelection 未用 Array.from 做字元碼點計數夾 cp');
    if (!/this\.selection\s*=\s*null/.test(clamp)) fails.push('_clampCaretAndSelection 越界時未重置 selection');
  }
  for (const name of ['undo', 'redo', '_applyTableMutation']) {
    const b = methodBody(adapter, name);
    if (!b) { fails.push(`adapter 缺 ${name}()`); continue; }
    if (!/this\._clampCaretAndSelection\(\)/.test(b)) {
      fails.push(`${name}() 在 caret_after 缺席路徑未調用 _clampCaretAndSelection()（幽靈游標復發）`);
    }
  }
  // agy 二審問題2：段落越界必須回「末段起點」（cp=0），只夾 para 保留舊 cp
  // 會讓游標落在縮段後末段文字中間（U4d L5 原契約「末段起點」）。
  const cb = methodBody(adapter, '_clampCaretAndSelection');
  if (cb && !/paraClamped[\s\S]{0,120}cp\s*=\s*0/.test(cb)) {
    fails.push('_clampCaretAndSelection 段落越界時未把 cp 重置到段起點（保留舊 cp＝游標落在末段文字中間）');
  }
  // agy 二審問題3：_applyTableMutation 的夾限不得只在 else 缺席分支——
  // caret_after 在場的成功路徑也須走夾限（合法回訊是 no-op，幽靈選區照清）。
  const ab = methodBody(adapter, '_applyTableMutation');
  if (ab && /caret_after[\s\S]{0,160}?\}\s*else\s*\{[\s\S]{0,120}?_clampCaretAndSelection/.test(ab)) {
    fails.push('_applyTableMutation 夾限被塞回 caret_after 缺席 else 分支（caret_after 在場路徑又跳過選區校正）');
  }
  return fails;
}
/** S3 換檔鎖（adapter 面）：openFile 重置 lastInsertedRootSlot＋selection。 */
function s3OpenFileLock(adapter) {
  const fails = [];
  const body = methodBody(adapter, 'openFile');
  if (!body) return ['adapter 缺 openFile()'];
  if (!/this\.lastInsertedRootSlot\s*=\s*null/.test(body)) {
    fails.push('openFile() 未重置 lastInsertedRootSlot（上一檔的最近插表槽號殘留→換檔後可繞過 U4d L6 防線）');
  }
  if (!/this\.selection\s*=\s*null/.test(body)) {
    fails.push('openFile() 未重置 selection（上一檔選區殘留進新檔）');
  }
  return fails;
}
/** S3 換檔鎖（UI 面）：loadFile 重置 activeTableSlot＋呼叫 resetImagePickMode。 */
function s3LoadFileLock(ui) {
  const fails = [];
  const body = methodBody(ui, 'loadFile');
  if (!body) return ['UI 缺 loadFile()'];
  if (!/this\.activeTableSlot\s*=\s*null/.test(body)) {
    fails.push('loadFile() 未重置 activeTableSlot（上一檔表格槽號殘留→新檔靜默操作舊槽）');
  }
  if (!/this\.resetImagePickMode\(\)/.test(body)) {
    fails.push('loadFile() 未呼叫 resetImagePickMode()（圖片選取態殘留污染開檔管線；drop 同走 loadFile）');
  }
  if (!/this\.loadFile\(/.test(ui)) fails.push('drop 路徑未經 loadFile（重置覆盖前提失實）');
  return fails;
}
/** S4 型別鎖（構造器）：coerceUsizeIndex 嚴格轉換且被 root_slot／巢狀步採用。 */
function s4CoerceLock(adapter) {
  const fails = [];
  const body = topLevelBody(adapter, 'buildTablePath');
  if (!body) return ['adapter 缺 buildTablePath()'];
  if (!/coerceUsizeIndex\(rootSlot/.test(body)) {
    fails.push('buildTablePath 的 root_slot 未走嚴格轉型（Number(null)===Number("")===Number(false)===0 靜默轉 0）');
  }
  if (!/coerceUsizeIndex\(v,/.test(body)) {
    fails.push('buildTablePath 的巢狀步座標未走嚴格轉型（{row_idx:null,cell_idx:""} 靜默轉 [0,0,0]）');
  }
  const helper = /function coerceUsizeIndex\([\s\S]*?\n\}/.exec(adapter);
  if (!helper) fails.push('adapter 缺 coerceUsizeIndex()（轉型前排除 boolean／object／空串的安全整數守門）');
  else {
    if (!/typeof value === 'boolean'/.test(helper[0])) fails.push('coerceUsizeIndex 未排除 boolean');
    if (!/Number\.isSafeInteger/.test(helper[0])) fails.push('coerceUsizeIndex 未用 Number.isSafeInteger');
    if (!/0xFFFFFFFF|4294967295/.test(helper[0])) fails.push('coerceUsizeIndex 未設 usize 上界 0xFFFFFFFF');
  }
  return fails;
}
/** S4 收斂鎖（呼叫鏈）：_resolveTablePath 以 try/catch 收斂為 {path:null,error}。 */
function s4TryLock(adapter) {
  const fails = [];
  const body = methodBody(adapter, '_resolveTablePath');
  if (!body) return ['adapter 缺 _resolveTablePath()'];
  if (!/try\s*\{/.test(body) || !/catch\s*\(/.test(body)) {
    fails.push('_resolveTablePath 未以 try/catch 收斂（locator／構造器拋錯即洩漏未捕獲例外給呼叫端）');
  }
  if (!/path:\s*null,\s*error/.test(body.match(/catch[\s\S]*/)?.[0] || body.match(/catch[\s\S]*/)?.[0] || '')) {
    fails.push('_resolveTablePath 的 catch 需收斂為 {path:null, error}');
  }
  // agy 二審問題1：入口 rootSlot 必須直接走 coerceUsizeIndex——先 Number()
  // 過關會讓 Number(false)=0／Number([])=0 在守門前被洗白成合法索引。
  if (!/coerceUsizeIndex\(rootSlot/.test(body)) {
    fails.push('_resolveTablePath 入口未走 coerceUsizeIndex（Number(false)=0 洗白合法索引，擊穿 S4 守門）');
  }
  if (!/buildTablePath\(located\.rootSlot,\s*located\.nestedSteps\)/.test(body)) {
    fails.push('_resolveTablePath 的巢狀真支援後路（buildTablePath(located.rootSlot, located.nestedSteps)）被移除——本卡只加 UI 守衛，不得拆引擎側後路');
  }
  return fails;
}

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 0｜真引擎前提（巢狀反查／undo 無游標回訊）', async () => {
  // S1 前提：巢狀表的 locator 給外層 root_slot＋非空 nested_steps
  const a = await fresh();
  assert(typeof a.engine.table_locator_json === 'function', 'wasm 殼缺 table_locator_json 綁定（S1 前提）');
  const outer = a.insertTable(2, 2, { afterPara: 0 });
  assert(outer.ok === true, `前置：外層插 2x2 表應受理（${J(outer.error)}）`);
  const nested = a.insertTable(1, 1, { afterPara: a.caret.para });
  assert(nested.ok === true, `前置：向儲存格段落插巢狀 1x1 表應受理（${J(nested.error)}）`);
  const loc = a.locateTableCell(a.caret.para);
  assert(loc && loc.rootSlot === 0 && Array.isArray(loc.nestedSteps) && loc.nestedSteps.length === 1,
    `巢狀格段落反查應給 root_slot=0＋nested_steps 長 1（實得 ${J(loc)}）`);

  // S2 前提：表格Mutation 與 undo 的 caret_after 皆為 null（幽靈游標根因）
  const b = await fresh();
  const ins = b.insertTable(3, 2, { afterPara: b.paragraphCount() - 1 });
  assert(ins.ok === true && ins.outcome && ins.outcome.caret_after,
    `插表應帶 caret_after 回訊（實得 ${J(ins.outcome ?? null)}）`);
  const u = b.undo();
  assert(u.ok === true && u.undone === true, `前置：undo 應執行（${J({ ok: u.ok, undone: u.undone })}）`);
  assert(u.outcome == null || u.outcome.caret_after == null,
    `前提：undo 的 caret_after 實測恆 null（實得 ${J(u.outcome ?? null)}）`);
  okLog('  ✓ 巢狀 locator 給外層 root_slot＋nested_steps；undo 零游標回訊（四燈前提錨定）');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 S1｜巢狀表游標→靜默改外層根表（跨層守衛）', async () => {
  // 真引擎巢狀場景＋真原型方法呼叫：游標在巢狀表內時 resolveRootSlot 必回 null
  const a = await fresh();
  a.insertTable(2, 2, { afterPara: 0 });
  a.insertTable(1, 1, { afterPara: a.caret.para });
  assert(a.locateTableCell(a.caret.para)?.nestedSteps?.length > 0, '前置：游標應落在巢狀表內');
  const rowsBefore = JSON.parse(a.engine.tables_summary_json()).map((s) => s.rows);
  const fake = { adapter: a, activeTableSlot: null, _nestedCaretBlocked: false };
  const slot = QWordUI.prototype.resolveRootSlot.call(fake, undefined);
  assert(slot === null,
    `游標在巢狀表內時 resolveRootSlot 必回 null（實得 ${J(slot)}——非 null 即外殼拿外層根表槽號去操作，用戶點巢狀表、改的卻是外層表）`);
  assert(fake._nestedCaretBlocked === true, '巢狀拒絕旗標未記上（上層無從給出巢狀專屬說明）');

  // 誠實拒下游：_requireTableTarget 走巢狀專屬彈窗＋拒絕日誌，且外層表零變動
  const modals = [];
  const rejections = [];
  const ui = {
    adapter: a,
    activeTableSlot: null,
    _nestedCaretBlocked: false,
    resolveRootSlot: QWordUI.prototype.resolveRootSlot,
    showErrorModal: (title, msg) => modals.push({ title, msg }),
    _reportEngineRejection: (cmdId, payload, res) => rejections.push({ cmdId, res })
  };
  const got = QWordUI.prototype._requireTableTarget.call(ui, 'table:row-delete', { cmdId: 'table:row-delete' }, undefined, '刪除列');
  assert(got === null, `_requireTableTarget 巢狀態必拒（實得 ${J(got)}）`);
  assert(modals.length === 1 && /巢狀/.test(modals[0].title), `巢狀態須彈窗且標題點名巢狀表（實得 ${J(modals)}）`);
  assert(modals[0] && /巢狀表內的操作尚未支援，請直接點外層表格/.test(modals[0].msg),
    `彈窗內容須如實說明巢狀未支援並指引點外層表格（實得 ${J(modals[0] && modals[0].msg)}）`);
  assert(rejections.length === 1 && rejections[0].res && rejections[0].res.rejectedByShell === true,
    `巢狀拒絕需補外殼拒絕日誌（實得 ${J(rejections)}）`);
  const rowsAfter = JSON.parse(a.engine.tables_summary_json()).map((s) => s.rows);
  assert(J(rowsBefore) === J(rowsAfter), `巢狀態外層表必須零變動（${J(rowsBefore)} → ${J(rowsAfter)}）`);
  // 巢狀守衛不得誤擋：游標移到表外正文段後，單表文件仍走 count===1 的正常路徑
  a.caret = { para: 0, cp: 0 };
  const ok2 = QWordUI.prototype.resolveRootSlot.call({ adapter: a, activeTableSlot: null, _nestedCaretBlocked: false }, undefined);
  assert(Number.isInteger(ok2), `游標移回表外後 resolveRootSlot 應正常解析（實得 ${J(ok2)}）`);
  for (const f of [...s1GuardLock(uiSrc), ...s1ModalLock(uiSrc)]) assert(false, `結構面違規：${f}`);
  okLog('  ✓ 巢狀游標回 null＋旗標＋專屬彈窗＋拒絕日誌；外層表零變動；表外路徑不受誤擋');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 S2｜undo／redo 幽靈游標（夾限缺席）', async () => {
  // 2a. 真引擎閉環：末段後插表（游標進新表）→ Undo → 游標必須被夾回界內可打字
  const a = await fresh();
  const last = a.paragraphCount() - 1;
  const ins = a.insertTable(3, 2, { afterPara: last });
  assert(ins.ok === true, `前置：末段後插 3x2 表應受理（${J(ins.error)}）`);
  const u = a.undo();
  assert(u.ok === true && u.undone === true, `前置：undo 應執行（${J({ ok: u.ok, undone: u.undone })}）`);
  const pc = a.paragraphCount();
  assert(a.caret && Number.isInteger(a.caret.para) && a.caret.para >= 0 && a.caret.para < pc,
    `undo 後游標段落必須 < 段落總數（實得 caret=${J(a.caret)} paraCount=${pc} → 幽靈游標）`);
  const typed = a.typeAt(a.caret.para, a.caret.cp, '甲');
  assert(typed.ok === true, `undo 後緊接打字必須成功（實得 ${J({ ok: typed.ok, error: typed.error })}）`);
  assert((a.paragraphText(a.caret.para) || '').includes('甲'),
    `校正後打字必須真的寫進游標段落（引擎回讀 ${J((a.paragraphText(a.caret.para) || '').slice(0, 30))}）`);
  // redo 路徑同檢（探針實錄 A：真引擎「插表→undo→redo」redo 會回
  // caret_after={cp:0,para:14}、redone=true；插表→undo→打字→redo 時
  // redone=false 屬新操作截斷 redo 棧的正確語義，另立場景測）
  const a2 = await fresh();
  const ins2 = a2.insertTable(3, 2, { afterPara: a2.paragraphCount() - 1 });
  assert(ins2.ok === true, `前置：redo 場景插表應受理（${J(ins2.error)}）`);
  const u2 = a2.undo();
  assert(u2.ok === true && u2.undone === true, `前置：redo 場景 undo 應執行（${J({ ok: u2.ok, undone: u2.undone })}）`);
  const rTrue = a2.redo();
  assert(rTrue.ok === true && rTrue.redone === true, `前置：redo 應執行（${J({ ok: rTrue.ok, redone: rTrue.redone })}）`);
  assert(a2.caret.para < a2.paragraphCount(), `redo 後游標段落仍需 < 段落總數（實得 ${J(a2.caret)} pc=${a2.paragraphCount()}）`);
  const typed2 = a2.typeAt(a2.caret.para, a2.caret.cp, '乙');
  assert(typed2.ok === true, `redo 後緊接打字必須成功（實得 ${J({ ok: typed2.ok, error: typed2.error })}）`);
  // redo 的 caret_after 缺席態（undo 系出口契約內可能）用注入確定性重現：
  // 換掉 redo_json 回傳、真引擎其餘不動——caret 停在高段、redo 後段數不漲，
  // 夾限缺席即幽靈游標。
  const b0 = await fresh();
  b0.insertTable(3, 2, { afterPara: b0.paragraphCount() - 1 });
  b0.undo();
  b0.caret = { para: 14, cp: 0 }; // 注入態：caret 處在 redo 後也不會變大的越界段
  const origRedo = b0.engine.redo_json.bind(b0.engine);
  b0.engine.redo_json = () => JSON.stringify({ ok: true, redone: true, outcome: { caret_after: null, events: [] } });
  const rNo = b0.redo();
  b0.engine.redo_json = origRedo;
  assert(rNo.ok === true && rNo.redone === true, `注入 redo（caret_after 缺席）應執行（${J({ ok: rNo.ok, redone: rNo.redone })}）`);
  assert(b0.caret && b0.caret.para < b0.paragraphCount(),
    `redo 的 caret_after 缺席時游標必須被夾限（實得 caret=${J(b0.caret)} pc=${b0.paragraphCount()} → 幽靈游標）`);

  // 2b. cp 面：caret.cp 越界必須被 _applyTableMutation 路徑夾限（舊版只夾 para）
  const b = await fresh();
  b.insertTable(2, 2, { afterPara: 0 });
  const len0 = Array.from(b.paragraphText(0) || '').length;
  b.caret = { para: 0, cp: len0 + 500 };
  const del = b.deleteTableRow({ rootSlot: 0, row: 1 });
  assert(del.ok === true, `前置：刪列應受理（${J(del.error)}）`);
  const len0b = Array.from(b.paragraphText(0) || '').length;
  assert(b.caret.cp <= len0b,
    `表格操作後 caret.cp 必須 <= 游標段落字元碼點數（實得 cp=${b.caret.cp} len=${len0b} → cp 越界未夾）`);
  const typed3 = b.typeAt(b.caret.para, b.caret.cp, '丙');
  assert(typed3.ok === true, `cp 夾限後打字必須成功（實得 ${J({ ok: typed3.ok, error: typed3.error })}）`);
  // agy 二審問題2行為鎖：段落越界重置＝回「末段起點」（cp=0），不是夾到縮段後
  // 末段的文字中間（保留舊 cp 的實現游標會落在段內錯位）。
  const g = await fresh();
  g.insertTable(3, 2, { afterPara: 0 });
  const gPc = g.paragraphCount();
  g.caret = { para: gPc + 5, cp: 9 }; // 段落與 cp 雙越界
  g._clampCaretAndSelection();
  assert(g.caret.para === gPc - 1 && g.caret.cp === 0,
    `段落越界重置必須回末段起點 cp=0（實得 ${J(g.caret)}——保留舊 cp 即游標落在末段文字中間）`);
  const typed4 = g.typeAt(g.caret.para, g.caret.cp, '丁');
  assert(typed4.ok === true, `末段起點校正後打字必須成功（實得 ${J({ ok: typed4.ok, error: typed4.error })}）`);

  // 2c. selection 面：越界選區在表格操作與 undo 後必須重置（舊版完全不管）
  const c = await fresh();
  c.insertTable(2, 2, { afterPara: 0 });
  c.selection = { target: 'Body', start: { para: 9999, cp: 9999 }, end: { para: 9999, cp: 9999 } };
  c.deleteTableRow({ rootSlot: 0, row: 1 });
  assert(c.selection === null, `表格操作後越界 selection 必須重置（實得 ${J(c.selection)}）`);
  const d = await fresh();
  d.insertTable(3, 2, { afterPara: d.paragraphCount() - 1 });
  d.selection = { target: 'Body', start: { para: 9999, cp: 0 }, end: { para: 9999, cp: 1 } };
  d.undo();
  assert(d.selection === null, `undo 後越界 selection 必須重置（實得 ${J(d.selection)}）`);
  // agy 二審問題3行為鎖：caret_after **在場**的成功路徑也不得完全跳過選區校正
  // （引擎回訊合法時夾限對游標是 no-op，但幽靈選區照樣要清）。
  const f2 = await fresh();
  const insF = f2.insertTable(3, 2, { afterPara: f2.paragraphCount() - 1 });
  assert(insF.ok && insF.outcome && insF.outcome.caret_after,
    `前置：本場景須走 caret_after 在場路徑（實得 ${J(insF.outcome ?? null)}）`);
  assert(f2.caret && f2.caret.para > 0 && f2.caret.cp === 0,
    `caret_after 在場時游標須照引擎回訊落地（實得 ${J(f2.caret)}——夾限不得動合法游標）`);
  f2.selection = { target: 'Body', start: { para: 9999, cp: 0 }, end: { para: 9999, cp: 1 } };
  const delF = f2.deleteTableRow({ rootSlot: 0, row: 1 });
  assert(delF.ok === true, `前置：caret_after 在場路徑刪列應受理（${J(delF.error)}）`);
  assert(f2.selection === null,
    `caret_after 在場的成功路徑也必須清越界選區（實得 ${J(f2.selection)}——幽靈選區跳過校正）`);
  // 合法選區不得被誤清
  const e2 = await fresh();
  e2.insertTable(2, 2, { afterPara: 0 });
  e2.selection = { target: 'Body', start: { para: 0, cp: 0 }, end: { para: 0, cp: 1 } };
  const del2 = e2.deleteTableRow({ rootSlot: 0, row: 1 });
  assert(del2.ok === true, `前置：刪列應受理（${J(del2.error)}）`);
  assert(e2.selection !== null, `界內選區不得被連坐清空（實得 ${J(e2.selection)}）`);
  for (const f of s2ClampLock(adapterSrc)) assert(false, `結構面違規：${f}`);
  okLog('  ✓ undo／redo 後游標界內可打字；cp 越界被夾；越界選區重置、界內選區保留');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 S3｜換檔殘留上一檔表格狀態', async () => {
  // 3a. adapter 面真值：換檔後 lastInsertedRootSlot／selection 必清空
  const a = await fresh();
  const ins = a.insertTable(2, 2);
  assert(ins.ok === true, `前置：插表應受理（${J(ins.error)}）`);
  a.selection = { target: 'Body', start: { para: 1, cp: 0 }, end: { para: 2, cp: 3 } };
  assert(a.lastInsertedRootSlot === 0, `前置：插表後 lastInsertedRootSlot 應記 0（實得 ${J(a.lastInsertedRootSlot)}）`);
  a.openFile(getDefaultSampleBytes(), 'other.docx');
  assert(a.lastInsertedRootSlot === null,
    `換檔後 lastInsertedRootSlot 必須 null（實得 ${J(a.lastInsertedRootSlot)}——殘留槽號可繞過 U4d L6 防線）`);
  assert(a.selection === null, `換檔後 selection 必須 null（實得 ${J(a.selection)}）`);
  assert(J(a.caret) === '{"para":0,"cp":0}', `換檔後 caret 應歸零（實得 ${J(a.caret)}）`);

  // 3b. UI 面真原型方法：loadFile 重置 activeTableSlot＋圖片選取態（drop 同走 loadFile）
  const b = await fresh();
  b.insertTable(2, 2);
  const ui = {
    adapter: b,
    activeTableSlot: b.lastInsertedRootSlot,
    _nestedCaretBlocked: true,
    imagePickMode: true,
    fileInput: { accept: 'image/png,image/jpeg' },
    resetImagePickMode: QWordUI.prototype.resetImagePickMode,
    renderDocument: async () => {},
    syncEngineStats: () => {},
    showErrorModal: () => {}
  };
  assert(ui.activeTableSlot === 0, '前置：activeTableSlot 應殘留上一檔槽號 0');
  await QWordUI.prototype.loadFile.call(ui, getDefaultSampleBytes(), 'x.docx');
  assert(ui.activeTableSlot === null, `loadFile 後 activeTableSlot 必須 null（實得 ${J(ui.activeTableSlot)}）`);
  assert(ui.imagePickMode === false, `loadFile 後 imagePickMode 必須清除（實得 ${J(ui.imagePickMode)}）`);
  assert(/\.docx/.test(ui.fileInput.accept), `loadFile 後 accept 必須回開檔白名單（實得 ${J(ui.fileInput.accept)}）`);
  assert(ui._nestedCaretBlocked === false, 'loadFile 後巢狀拒絕旗標不得殘留（會誤擋新檔首次操作）');
  assert(b.lastInsertedRootSlot === null && b.selection === null,
    `loadFile 閉環後 adapter 狀態必須乾淨（實得 ${J({ l: b.lastInsertedRootSlot, s: b.selection })}）`);
  // 換檔後新檔多表＋游標表外：resolveRootSlot 不得命中舊槽號（U4d L6 防線完整）。
  // 第二張表插在第一張表的 para_range 之外（表後正文段）才是第二張**根表**——
  // 插進 para_start..para_end 內會被引擎當成巢狀表（探針實錄 B：tableCount
  // 仍 1、summary 單筆 table_count=2），歧義前提失實。
  b.insertTable(2, 2, { afterPara: 0 });
  const afterTablePara = JSON.parse(b.engine.tables_summary_json())[0].para_end + 2;
  const insB2 = b.insertTable(2, 2, { afterPara: afterTablePara });
  assert(insB2.ok === true, `前置：表後正文段插第二張根表應受理（${J(insB2.error)}）`);
  assert(b.tableCount() === 2, `前置：必須存在兩張根表（實得 ${b.tableCount()}）`);
  b.caret = { para: 0, cp: 0 };
  const ui2 = { adapter: b, activeTableSlot: null, _nestedCaretBlocked: false };
  const slot2 = QWordUI.prototype.resolveRootSlot.call(ui2, undefined);
  assert(slot2 === null, `換檔後新檔多表＋游標表外必回 null 誠實拒（實得 ${J(slot2)}——非 null 即殘留槽號繞過防線）`);
  for (const f of [...s3OpenFileLock(adapterSrc), ...s3LoadFileLock(uiSrc)]) assert(false, `結構面違規：${f}`);
  okLog('  ✓ 換檔後 adapter／UI 表格狀態全清；新檔多表＋表外游標走誠實拒，防線不被繞過');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 S4｜路徑型別強制（null/\"\"/false 靜默轉 0）', async () => {
  const expectThrow = (fn, what) => {
    let threw = false, out = null;
    try { out = fn(); } catch { threw = true; }
    assert(threw, `${what} 必須拋錯（實得不拋：${J(out)}——壞座標被靜默送進引擎）`);
  };
  // 4a. root_slot：null／空串／false／{}／負數／小數／越上界一律拒
  for (const bad of [null, '', '   ', false, true, {}, -1, 1.5, NaN, 2 ** 32, 2 ** 53]) {
    expectThrow(() => buildTablePath(bad), `buildTablePath(${J(bad)})`);
  }
  for (const good of [0, 3, 4294967295]) {
    let threw = false;
    try { buildTablePath(good); } catch { threw = true; }
    assert(!threw, `合法 root_slot=${good} 不得被誤擋`);
  }
  // 4b. 巢狀步：null／空串／false 混入必拒（不問陣列／物件形態）
  expectThrow(() => buildTablePath(0, [{ row_idx: null, cell_idx: '', sub_table_idx: false }]),
    'nested 物件形態 {null,"",false}');
  expectThrow(() => buildTablePath(0, [[null, '', false]]), 'nested 陣列形態 [null,"",false]');
  expectThrow(() => buildTablePath(0, [{ row_idx: 0 }]), 'nested 物件缺欄');
  expectThrow(() => buildTablePath(0, [[0, 1]]), 'nested 長度非 3');
  // agy 二審問題5鎖：BigInt 與循環引用物件都必須被誠實拋錯，且拋的是守門
  // Error（不是 JSON.stringify 自爆的 TypeError——字串化失敗不得覆蓋語意）
  let bigintErr = null;
  try { buildTablePath(2n); } catch (e) { bigintErr = e; }
  assert(bigintErr && /root_slot/.test(bigintErr.message) && !/Converting circular|Do not know how to serialize/.test(bigintErr.message),
    `BigInt root_slot 需拋守門 Error（實得 ${bigintErr && bigintErr.message}）`);
  const circular = { row_idx: 0 };
  circular.self = circular;
  let nestedErr = null;
  try { buildTablePath(0, [circular]); } catch (e) { nestedErr = e; }
  assert(nestedErr && /nested_steps\[0\]/.test(nestedErr.message) && !/Converting circular/.test(nestedErr.message),
    `循環引用巢狀步需拋守門 Error 帶欄位名（實得 ${nestedErr && nestedErr.message}）`);
  // 合法形態回帰（u4d L3 契約不得被加嚴誤傷）
  assert(J(buildTablePath(1, [{ row_idx: 0, cell_idx: 1, sub_table_idx: 0 }]).nested_steps) === '[[0,1,0]]',
    '物件形態合法三元組仍須正規化為數值陣列');
  assert(J(buildTablePath(2, [[3, 4, 5]]).nested_steps) === '[[3,4,5]]', '陣列形態合法值原樣保留');
  assert(buildTablePath(0).root_slot === 0, 'root_slot=0 合法值照過');

  // 4c. 呼叫鏈收斂：locator 拋例外時表格操作回 ok:false，不外洩未捕獲例外
  const a = await fresh();
  a.insertTable(2, 2);
  const orig = a.engine.table_locator_json.bind(a.engine);
  a.engine.table_locator_json = () => { throw new Error('引擎炸了'); };
  let threw = null, res = null;
  try { res = a.insertTableRow('After', {}); } catch (err) { threw = err; }
  a.engine.table_locator_json = orig;
  assert(threw === null, `locator 拋例外不得洩漏未捕獲例外到呼叫端（實得 threw=${threw && threw.message}）`);
  assert(res && res.ok === false && res.rejectedByShell === true && res.error,
    `locator 拋例外時必須回 {ok:false, rejectedByShell:true, error}（實得 ${J(res)}）`);
  // 收斂後引擎復原，同一路徑仍正常（try/catch 不得把成功路徑也吞了）
  const again = a.insertTableRow('After', { rootSlot: 0, atRow: 0 });
  assert(again.ok === true, `前置收斂後表格操作仍應可用（${J(again.error)}）`);
  // root_slot 壞值經 _resolveTablePath 入口也被誠實擋（既有防線回歸）
  const bad = a.insertTableRow('After', { rootSlot: 'abc' });
  assert(bad.ok === false && bad.error, `root_slot='abc' 必被誠實拒（實得 ${J(bad)}）`);
  // agy 二審問題1行為鎖：false／[] 在呼叫鏈入口也不得被 Number() 洗白成 0
  for (const [label, v] of [['false', false], ['[]', []], ['true', true]]) {
    const rBad = a.insertTableRow('After', { rootSlot: v });
    assert(rBad.ok === false && rBad.rejectedByShell === true && rBad.error,
      `root_slot=${label} 必被誠實拒（實得 ${J(rBad)}——Number(${label})=0 洗白即靜默操作第 0 張表）`);
  }
  for (const f of [...s4CoerceLock(adapterSrc), ...s4TryLock(adapterSrc)]) assert(false, `結構面違規：${f}`);
  okLog('  ✓ 壞座標全拋錯、合法座標零誤傷、真引擎回帰通過；locator 拋例外收斂為 ok:false 零洩漏');
});

// ─────────────────────────────────────────────────────────────────────────────
await lamp('檢查 9｜自我鑑別（回塞態必紅）', async () => {
  const discrim = [
    { tag: 'S1 巢狀守衛摘除', pick: 'ui', mut: (s) => {
        const b = methodBody(s, 'resolveRootSlot');
        if (!b) return s.replace('Array.isArray(located.nestedSteps)', 'false');
        return s.replace(b, b
          .replace('Array.isArray(located.nestedSteps) && located.nestedSteps.length > 0', 'false')
          .replace('this._nestedCaretBlocked = true;\n      return null;', 'return null;'));
      }, lock: s1GuardLock },
    { tag: 'S1 彈窗文案摘除', pick: 'ui', mut: (s) => s.split('巢狀表內的操作尚未支援，請直接點外層表格').join('__deleted__'), lock: s1ModalLock },
    { tag: 'S2 undo 夾限摘除', pick: 'adapter', mut: (s) => {
        const b = methodBody(s, 'undo');
        return s.replace(b, b.split('this._clampCaretAndSelection();').join(''));
      }, lock: s2ClampLock },
    { tag: 'S3 openFile 重置摘除', pick: 'adapter', mut: (s) => {
        const b = methodBody(s, 'openFile');
        if (!b) return s.replace('this.lastInsertedRootSlot = null;', '');
        return s.replace(b, b.split('this.lastInsertedRootSlot = null;').join(''));
      }, lock: s3OpenFileLock },
    { tag: 'S3 loadFile 重置摘除', pick: 'ui', mut: (s) => {
        const b = methodBody(s, 'loadFile');
        if (!b) return s.split('this.activeTableSlot = null;').join('');
        return s.replace(b, b.split('this.activeTableSlot = null;').join(''));
      }, lock: s3LoadFileLock },
    { tag: 'S4 嚴格轉型摘除', pick: 'adapter', mut: (s) => s.split('coerceUsizeIndex(rootSlot').join('Number(rootSlot'), lock: s4CoerceLock },
    { tag: 'S4 try/catch 摘除', pick: 'adapter', mut: (s) => {
        const b = methodBody(s, '_resolveTablePath');
        if (!b) return s.replace('try {', '/*no-try*/ {');
        return s.replace(b, b.replace('try {', '/*no-try*/ if (true) {').replace(/catch \(/, '} catch_removed ('));
      }, lock: s4TryLock },
  ];
  const srcs = { adapter: adapterSrc, ui: uiSrc };
  let allRed = true;
  for (const d of discrim) {
    const mutated = d.mut(srcs[d.pick]);
    const fails = d.lock(mutated);
    assert(fails.length > 0, `鑑別失效：摘除「${d.tag}」後考卷仍全綠（會放水）`);
    console.log(fails.length ? `  ✓ 鑑別 ${d.tag} → 紅（${fails.length} 項）` : `  ✗ 鑑別 ${d.tag} → 仍綠`);
    if (!fails.length) allRed = false;
  }
  const liveFails = [
    ...s1GuardLock(uiSrc), ...s1ModalLock(uiSrc), ...s2ClampLock(adapterSrc),
    ...s3OpenFileLock(adapterSrc), ...s3LoadFileLock(uiSrc),
    ...s4CoerceLock(adapterSrc), ...s4TryLock(adapterSrc)
  ];
  assert(liveFails.length === 0, `真樹結構面有 ${liveFails.length} 項違規（鑑別變異不得污染源碼）`);
  if (allRed && liveFails.length === 0) okLog('  ✓ 七種回塞態全紅，真樹零污染');
});

console.log('\n' + '='.repeat(72));
if (notes.length) console.log(`附註 ${notes.length} 項`);
if (pass) {
  console.log('自檢結果：PASS — U4e 四盞燈全綠（先紅後綠實錄見 docs/word-ui-u4e-fixes-report.md）');
  process.exit(0);
}
console.log(`自檢結果：FAIL（${errors.length} 項違規）`);
for (const e of errors) console.log(`  ✗ ${e}`);
process.exit(1);
