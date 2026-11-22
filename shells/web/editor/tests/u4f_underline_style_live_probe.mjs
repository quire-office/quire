/**
 * u4f_underline_style_live_probe.mjs — U4f：底線形態 Ribbon 下拉轉 live 的瀏覽器閉環探針（正式入庫版）
 *
 * 驗收口徑（U4c 同款）：真 DOM click → bindEvents 委派 → core `format:underline-style`
 * 臂（驗證＋匯流）→ UI handler → adapter.formatUnderline → apply_mutation_json →
 * 存檔件 `w:u/@w:val` 逐形態逐字對（**檔契約**：single/double/thick/dotted/dashed、
 * 關底線=none——ECMA 正字；serde 線契約是 'off'，兩套字面禁互竄，混用即假綠）
 * → 顯示列表 FLAG_UNDERLINE 用戶可見態 → edit:undo 逐位還原。
 *
 * 為什麼不只斷言 UI class：canvas 劃線層（vendor/renderer.js）只唸 flags 兜底單線、
 * 未消費 DrawItem::Text.underline 形態（E-2 記帳未做面，另卡），所以「形態差異」
 * 的用戶可見證據面＝存檔件 w:val 逐字＋flags 0↔4 的開關態；形態間指紋零位移不是
 * 退步，是老實——本探針據此只鎖可宣稱的兩層。
 *
 * 用法（worktree 根起靜態伺服；Chrome 需 --remote-debugging-port=9222）：
 *   python3 -m http.server 8791 --bind 127.0.0.1 &
 *   node shells/web/editor/tests/u4f_underline_style_live_probe.mjs
 * 環境變數：U4F_URL（默認 http://127.0.0.1:8791/shells/web/editor/index.html）、
 *           CDP_URL（默認 http://127.0.0.1:9222）、U4F_OUT（證據 JSON 路徑）。
 *
 * 判定口徑：
 * - 展開鈕＋選單六項（single/double/thick/dotted/dashed/off）：data-requirement 必 live。
 * - 選單值域禁 wavy/words（寫入端誠實拒，外殼不代收）。
 * - 逐形態：存檔件精確含 `<w:u w:val="<形態>"/>` 且零其他形態字面；off＝零殘留
 *   `<w:u` 有效宣告（none/false/0 皆算殘留）。
 * - 誠實三層證據（wavy）：選單無此項、直接帶 wavy 下單 adapter 受理=false、
 *   存檔件零位移。
 * - off 鈕＝引擎明確移除（patch_rpr 寫 `<w:u w:val="none"/>` 熄滅級聯，ECMA 檔
 *   契約正字；serde 線契約是 'off'——兩套字面禁互竄）；證據走 double→off→double
 *   拉回路徑＋flags 歸零＋收尾 undo drain 回基準。
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const BASE_URL = process.env.U4F_URL || 'http://127.0.0.1:8791/shells/web/editor/index.html';
const OUT = process.env.U4F_OUT || path.join(ROOT, 'docs/word-ui-u4f-underline-live-evidence.json');
const CDP_HTTP = process.env.CDP_URL || 'http://127.0.0.1:9222';

/** 頁面 main world 內執行（序列化注入，禁閉包引用）。 */
async function runInPage(baseUrl) {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const shell = () => window.QWordShell;
  const adapter = () => window.QWordEngine;
  const CANON = ['single', 'double', 'thick', 'dotted', 'dashed', 'off'];
  const FILE_VALS = ['single', 'double', 'thick', 'dotted', 'dashed', 'none'];

  const HARD_DEADLINE = Date.now() + 240000;
  const stillTime = () => Date.now() < HARD_DEADLINE;
  while (stillTime()) {
    if (shell() && adapter()) break;
    await sleep(100);
  }
  if (!shell() || !adapter()) throw new Error('外殼未就緒（QWordShell 未掛載）');

  // 重置到開檔態再等文件真的就緒（同 U4c 口徑：先外殼就緒即可開單，不得先等 engine）
  await shell().openDefaultSample();
  while (stillTime()) {
    if (adapter().engine && adapter().paragraphCount() > 0) break;
    await sleep(200);
  }
  if (!adapter().engine) throw new Error('開檔失敗：引擎實例未建立');
  await sleep(400);

  // ---- 存檔件 word/document.xml（檔契約證據源）：zip 本體解 raw-deflate ----
  async function documentXml() {
    const bytes = adapter().save();
    const buf = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const dv = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
    for (let i = 0; i + 4 <= buf.length; i += 1) {
      if (dv.getUint32(i, true) !== 0x04034b50) continue;
      const nameLen = dv.getUint16(i + 26, true);
      const extraLen = dv.getUint16(i + 28, true);
      const cSize = dv.getUint32(i + 18, true);
      const name = new TextDecoder().decode(buf.subarray(i + 30, i + 30 + nameLen));
      if (name !== 'word/document.xml') continue;
      const start = i + 30 + nameLen + extraLen;
      const comp = buf.subarray(start, start + cSize);
      try {
        const ds = new DecompressionStream('deflate-raw');
        const stream = new Blob([comp]).stream().pipeThrough(ds);
        return await new Response(stream).text();
      } catch {
        return new TextDecoder().decode(comp);
      }
    }
    return '';
  }
  const formHits = (xml) => {
    const hits = [];
    for (const v of FILE_VALS) if (xml.includes(`<w:u w:val="${v}"/>`)) hits.push(v);
    // 未宣告 w:val 的 <w:u/>（ECMA 預設＝single）也算單條證據
    if (/<w:u\s*\/>/.test(xml)) hits.push('(裸<w:u/>)');
    return hits;
  };
  const anyUElement = (xml) => /<w:u[\s/>]/.test(xml);

  // ---- display-list 旗標態：游標段的底線開關（用戶可見層：canvas 劃線只唸這個）----
  const underlineFlags = () => {
    const out = [];
    for (let p = 0; p < adapter().pageCount(); p += 1) {
      for (const it of adapter().getPageDisplayList(p).items) {
        if (it.Text && (it.Text.flags & 4) === 4) out.push(1);
      }
    }
    return out.length;
  };

  // ---- 真 DOM 路徑：常用頁 → 展開鈕 → 選單項 ----
  const showHome = () => shell().onCommand('tab:switch:tab-home', {});
  const arrowBtn = () => document.querySelector('[data-command="format:underline-style"]');
  const menuItem = (style) => document.querySelector(`#popover-underline-style [data-underline-style="${style}"]`);
  const rectVisible = (el) => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
  const reopenMenu = async () => {
    const pop = document.getElementById('popover-underline-style');
    if (pop && pop.classList.contains('open')) return true;
    const b = arrowBtn();
    if (!b) return false;
    b.click();
    await sleep(80);
    return !!(pop && pop.classList.contains('open'));
  };
  const caretBodyPara = () => {
    // 游標釘進正文段（para 1，同 e2b 探針口徑）；避免選取殘留，清 selection
    adapter().selection = null;
    adapter().caret = { para: 1, cp: 0 };
  };

  const R = { meta: { baseUrl, at: new Date().toISOString(), ua: navigator.userAgent }, labels: {}, forms: {}, honesty: {}, notes: {} };

  // 0) 基準：乾淨 sample（零 <w:u>、零底線旗標項）
  caretBodyPara();
  const xml0 = await documentXml();
  R.notes.baseline = { xmlHasU: anyUElement(xml0), flagItems: underlineFlags(), paragraphs: adapter().paragraphCount() };

  // 1) 標籤誠實化：展開鈕＋選單六項 data-requirement 必 live、值域禁 wavy/words
  showHome(); await sleep(60);
  const arrow = arrowBtn();
  R.labels.arrow = arrow ? { found: true, req: arrow.getAttribute('data-requirement'), aria: arrow.getAttribute('aria-label'), title: arrow.getAttribute('title'), visible: rectVisible(arrow) } : { found: false };
  const menuOpen = await reopenMenu();
  R.labels.menuOpened = menuOpen;
  R.labels.items = {};
  for (const style of CANON) {
    const it = menuItem(style);
    R.labels.items[style] = it ? { req: it.getAttribute('data-requirement'), text: it.textContent.trim(), visible: rectVisible(it) } : { found: false };
  }
  R.labels.forbidden = ['wavy', 'words'].map((bad) => ({
    val: bad,
    inMenu: !!document.querySelector(`#popover-underline-style [data-underline-style="${bad}"]`)
  }));
  R.labels.menuItemCount = document.querySelectorAll('#popover-underline-style [data-underline-style]').length;
  // 選單必須由 engine-adapter 的 UNDERLINE_KINDS 供值（單一真值源鎖）
  const mod = await import('/shells/web/editor/js/engine-adapter.js');
  R.labels.adapterKinds = mod.UNDERLINE_KINDS.slice();

  // 2) 逐形態真閉環：單項 click → core→UI→adapter → 存檔件逐字 ＋ flags 態 ＋ undo 還原
  for (const style of ['single', 'double', 'thick', 'dotted', 'dashed']) {
    caretBodyPara();
    await reopenMenu(); await sleep(40);
    const before = { xml: await documentXml(), flags: underlineFlags() };
    const it = menuItem(style);
    let dispatched = null;
    if (it) it.click();
    await sleep(260);
    const after = { xml: await documentXml(), flags: underlineFlags() };
    const e = {
      found: !!it,
      req: it ? it.getAttribute('data-requirement') : null,
      beforeHits: formHits(before.xml), afterHits: formHits(after.xml),
      exactForm: formHits(after.xml).length === 1 && formHits(after.xml)[0] === style,
      flagOn: after.flags > 0,
      coreLog: (() => { const log = (window.__qwordLog || []).filter((x) => x.cmdId === 'format:underline-style'); const last = log[log.length - 1]; return last ? { handled: last.handled, requirement: last.requirement } : null; })()
    };
    // undo 逐位還原（回上一形態態）
    const u = shell().onCommand('edit:undo', {}); await sleep(200);
    const back = { xml: await documentXml(), flags: underlineFlags() };
    e.undo = { dispatched: !!u.handled, backHits: formHits(back.xml), restoredToPrev: formHits(back.xml).join() === formHits(before.xml).join() };
    R.forms[style] = e;
  }

  // 3) off 鈕（關底線）：引擎 off＝明確移除（patch_rpr 寫 `<w:u w:val="none"/>`
  //    熄滅級聯宣告，K29e 顯式移除形態，非刪乾淨）；證據走 double→off→double 拉回
  {
    caretBodyPara();
    await reopenMenu(); await sleep(40);
    const dIt = menuItem('double');
    if (dIt) dIt.click();
    await sleep(260);
    const doubled = { xml: await documentXml(), flags: underlineFlags() };
    caretBodyPara();
    await reopenMenu(); await sleep(40);
    const oIt = menuItem('off');
    if (oIt) oIt.click();
    await sleep(260);
    const offed = { xml: await documentXml(), flags: underlineFlags() };
    const e = {
      found: !!oIt, req: oIt ? oIt.getAttribute('data-requirement') : null,
      doubleHits: formHits(doubled.xml),
      // 檔契約口徑：off 寫 ECMA 正字 `<w:u w:val="none"/>`（serde 線契約是
      // 'off'，兩套字面禁互竄）；且不得殘留任何開啟形態字面。
      offWritesNone: offed.xml.includes('<w:u w:val="none"/>'),
      offNoFormLeft: formHits(offed.xml).length === 1 && formHits(offed.xml)[0] === 'none',
      offClearsFlags: offed.flags === 0
    };
    // 拉回：再點 double → 檔契約回到 double（證明 off 是可回路的明確移除，非死態）
    caretBodyPara();
    await reopenMenu(); await sleep(40);
    if (menuItem('double')) menuItem('double').click();
    await sleep(260);
    e.pulledBackHits = formHits(await documentXml());
    // 收尾：drain undo 回基準（double→off→double 共三筆，逐筆退到乾淨態為止）
    let drained = 0;
    for (let i = 0; i < 8 && (anyUElement(await documentXml()) || underlineFlags() > 0); i += 1) {
      shell().onCommand('edit:undo', {}); await sleep(140); drained += 1;
    }
    e.undoDrained = drained;
    const backXml = await documentXml();
    e.finalBaselineClean = !anyUElement(backXml) && underlineFlags() === 0;
    R.forms.off = e;
  }

  // 4) 誠實三層（wavy 不代收）：選單無此項＋直接下單受理=false＋零位元組位移
  {
    const before = await documentXml();
    const direct = adapter().formatUnderline('wavy');
    caretBodyPara();
    const after = await documentXml();
    R.honesty.wavy = {
      menuItem: !!document.querySelector('#popover-underline-style [data-underline-style="wavy"]'),
      adapterAccepted: !!direct && direct.ok === true,
      adapterError: direct && direct.error ? String(direct.error).slice(0, 160) : null,
      xmlUnchanged: before === after
    };
    // core 臂同款：非法形態 handled 必 false（不假 live、不進 UI 落地層）
    const res = shell().onCommand('format:underline-style', { style: 'wavy' });
    R.honesty.coreWavy = { handled: !!res.handled, requirement: res.requirement };
  }
  // 4b) core 臂合法態：canonical 六形態直達指令必 handled＋live（選單 click 之外的第二呼叫端口徑）
  {
    const probes = [];
    for (const style of CANON) {
      const res = shell().onCommand('format:underline-style', { style });
      probes.push({ style, handled: !!res.handled, req: res.requirement });
    }
    R.notes.coreArm = probes;
  }

  return R;
}

// ---- CDP 骨架（U4c 同款）----
async function main() {
  const list = await (await fetch(`${CDP_HTTP}/json/list`)).json();
  const created = await (await fetch(`${CDP_HTTP}/json/new?about:blank`, { method: 'PUT' })).json();
  const sock = new WebSocket(created.webSocketDebuggerUrl);
  await new Promise((res, rej) => { sock.onopen = res; sock.onerror = rej; });
  let seq = 0;
  const pending = new Map();
  sock.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
  };
  const send = (method, params = {}) => new Promise((res) => {
    const id = ++seq;
    pending.set(id, (m) => res(m.result || {}));
    sock.send(JSON.stringify({ id, method, params }));
  });
  await send('Page.enable');
  await send('Page.navigate', { url: BASE_URL });
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
  const v = out.result.value;
  fs.writeFileSync(OUT, JSON.stringify(v, null, 2));
  console.log(`證據檔 → ${OUT}`);

  // ---- 紅綠板 ----
  let fail = 0;
  const line = (cond, msg) => { console.log(`  ${cond ? '✓' : '✗'} ${msg}`); if (!cond) fail += 1; };
  console.log('=== U4f 底線形態下拉 瀏覽器閉環判定 ===');
  console.log('baseline:', JSON.stringify(v.notes.baseline));
  const CANON = ['single', 'double', 'thick', 'dotted', 'dashed', 'off'];
  line(v.labels.arrow && v.labels.arrow.found && v.labels.arrow.req === 'live',
    `展開鈕 live（req=${v.labels.arrow ? v.labels.arrow.req : '-'}）`);
  line(v.labels.menuOpened, '選單可開（展開鈕 click → popover）');
  for (const s of CANON) {
    const it = v.labels.items[s];
    line(!!it && it.req === 'live' && it.visible !== false, `選單項 ${s} live`);
  }
  line(v.labels.menuItemCount === 6, `選單恰六項（實=${v.labels.menuItemCount}）`);
  line(v.labels.forbidden.every((f) => !f.inMenu), '值域禁 wavy/words');
  line(JSON.stringify(v.labels.adapterKinds) === JSON.stringify(CANON),
    `選單值域源＝adapter.UNDERLINE_KINDS canonical（${JSON.stringify(v.labels.adapterKinds)}）`);
  for (const s of ['single', 'double', 'thick', 'dotted', 'dashed']) {
    const e = v.forms[s];
    line(!!e && e.found && e.exactForm && e.flagOn && e.undo && e.undo.restoredToPrev,
      `${s} 檔契約逐字=${!!e && e.exactForm}（hits=${e ? JSON.stringify(e.afterHits) : '-'}）flagOn=${!!e && e.flagOn} undo回前態=${!!e && e.undo && e.undo.restoredToPrev}`);
  }
  const off = v.forms.off;
  line(!!off && off.offWritesNone && off.offNoFormLeft && off.offClearsFlags && (off.pulledBackHits || []).includes('double') && off.finalBaselineClean,
    `off 明確移除（寫none=${!!off && off.offWritesNone} 形態零殘留=${!!off && off.offNoFormLeft} flags歸零=${!!off && off.offClearsFlags} 拉回=${!!off && (off.pulledBackHits || []).includes('double')} 收尾乾淨=${!!off && off.finalBaselineClean}`);
  line(!!v.honesty.wavy && !v.honesty.wavy.menuItem && v.honesty.wavy.adapterAccepted === false && v.honesty.wavy.xmlUnchanged,
    `wavy 誠實三層（選單無/受理false/零位移）= ${JSON.stringify(v.honesty.wavy)}`);
  line(!!v.honesty.coreWavy && v.honesty.coreWavy.handled === false,
    `core 臂拒非法形態（handled=false, req=${v.honesty.coreWavy ? v.honesty.coreWavy.requirement : '-'}）`);
  line(v.notes.coreArm.every((p) => p.handled && p.req === 'live'),
    `core 臂 canonical 六形態直達 handled＋live = ${JSON.stringify(v.notes.coreArm)}`);
  console.log(`\n判定：${fail === 0 ? 'PASS' : `FAIL（${fail} 項）`}`);
  process.exit(fail === 0 ? 0 : 1);
}

main().catch((e) => { console.error('探針失敗:', e.message); process.exit(2); });
