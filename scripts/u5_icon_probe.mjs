#!/usr/bin/env node
/**
 * u5_icon_probe.mjs
 * AGY-U5: Ribbon 圖示sprite 修復（<g viewBox> 非法 → <symbol>）之真瀏覽器 bbox 探針。
 *
 * 原理：
 * - 以 Node 內建 http 起本機靜態伺服器（port 自動分配），開 shells/web/editor/index.html。
 * - 以系統 Chrome（--headless=new）＋ CDP（WebSocket）量測每個
 *   <svg class="qword-icon"><use> 的字形 getBBox()，逐 tab 切換後量（隱藏 panel 先顯形）。
 * - 對每個 sprite（<symbol> 或修復前的 <g>）以 1:1 尺度 temp <use> 取得內容實際 span，
 *   再用 meet 縮放預期值核對站點 bbox（等比、不溢框、不過度內縮）。
 *
 * 判定（每個圖示站點）：
 * 1) 溢框紅線：bbox.w/h 必須 ≤ CSS 盒 + 0.5px（viewBox 掛 <g> 無效時字形 1:1 溢框即紅）。
 * 2) 等比鎖：bbox 必須與「span × meet 預期縮放量」吻合（±2%）——拉筋/非等比即紅。
 * 3) 內縮下限：glyph 內容在其 viewBox 的主軸跨度 ≥ 55%（防改成過度內縮）。
 *    註：細長字形（如 chevron 2:1）在方形盒中副軸必然低於主軸，此為 intrinsic 長寬比
 *    而非缺陷，故 55% 施加於主軸；副軸是否拉筋由第 2 條等比鎖把關。
 *
 * 輸出：
 * - stdout 人可讀摘要（PASS/FAIL 計數、失敗清單逐個點名）。
 * - --json FILE 存完整逐站點 JSON。
 * - --out DIR 存三張 tab 截圖（常用/插入/設計），預設 ~/.quire/u5-icon-probe/（repo 外持久路徑）。
 *
 * 用法：node scripts/u5_icon_probe.mjs [--root REPO_ROOT] [--out DIR] [--json FILE] [--chrome PATH]
 * 退出碼：0 全綠；1 有 FAIL；2 探針自身錯誤。
 *
 * 相容性：U6 換裝（Font Awesome Free 7.3.1，scripts/gen_icon_sprite.py 產生 sprite）後
 * 已在本機 main 全樹實跑驗證——探針讀 <defs> 內 sprite 的 viewBox 自行推 meet 縮放，
 * 對 24 方框與 FA 非方形 viewBox（如 0 0 448 512）皆適用，sprite 換來源不需改探針。
 * 注意：需要 shells/web/pkg/（wasm 產物）在位，否則頁面模組初始化失敗、僅預設 tab 顯形，
 * 其餘 tab 站點會以 0x0 跳過（跳過≠通過，收案前確認「實量」數等於站點總數）。
 */

import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// ---------- CLI ----------
const argv = process.argv.slice(2);
function argOf(flag, dflt) {
  const i = argv.indexOf(flag);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : dflt;
}
const ROOT = path.resolve(argOf('--root', path.resolve(__dirname, '..')));
const OUT_DIR = path.resolve(argOf('--out', path.join(os.homedir(), '.quire', 'u5-icon-probe')));
const JSON_OUT = argOf('--json', null);
const CHROME = argOf('--chrome', process.env.CHROME_BIN || '/usr/bin/google-chrome');
const EDITOR_REL = 'shells/web/editor/index.html';
const SHOT_TABS = ['tab-home', 'tab-insert', 'tab-design'];
const OVERFLOW_TOL_PX = 0.5;
const UNIFORM_TOL = 0.02;      // 等比鎖容差（±2%）
const MIN_FILL = 0.55;         // 主軸內縮下限

// ---------- 靜態伺服器 ----------
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.json': 'application/json',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.d.ts': 'text/plain',
};
function startServer(rootDir) {
  const server = http.createServer((req, res) => {
    const urlPath = decodeURIComponent(req.url.split('?')[0]);
    let file = path.normalize(path.join(rootDir, urlPath));
    if (!file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    if (urlPath.endsWith('/')) file = path.join(file, 'index.html');
    fs.readFile(file, (err, buf) => {
      if (err) { res.writeHead(404); res.end('404'); return; }
      res.writeHead(200, { 'Content-Type': MIME[path.extname(file)] || 'application/octet-stream' });
      res.end(buf);
    });
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

// ---------- 最小 CDP 客戶端（Node 內建 WebSocket） ----------
class CDP {
  constructor(ws) {
    this.ws = ws;
    this.nextId = 1;
    this.pending = new Map();
    ws.addEventListener('message', (ev) => {
      const msg = JSON.parse(typeof ev.data === 'string' ? ev.data : ev.data.toString());
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(`CDP ${msg.method||''}: ${msg.error.message}`));
        else resolve(msg.result);
      }
    });
  }
  send(method, params = {}, sessionId = undefined) {
    const id = this.nextId++;
    const payload = { id, method, params };
    if (sessionId) payload.sessionId = sessionId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify(payload));
    });
  }
}

async function launchChrome(profileDir) {
  const args = [
    '--headless=new', '--disable-gpu', '--no-sandbox', '--disable-dev-shm-usage',
    '--hide-scrollbars', '--remote-debugging-port=0',
    `--user-data-dir=${profileDir}`, '--window-size=1440,900', 'about:blank',
  ];
  const child = spawn(CHROME, args, { stdio: ['ignore', 'ignore', 'pipe'] });
  const portFile = path.join(profileDir, 'DevToolsActivePort');
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (fs.existsSync(portFile)) {
      const lines = fs.readFileSync(portFile, 'utf8').trim().split(/\r?\n/);
      if (lines.length >= 2 && lines[0].trim()) return { child, wsUrl: `ws://127.0.0.1:${lines[0].trim()}${lines[1].trim()}` };
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  child.kill('SIGKILL');
  throw new Error(`無法啟動 Chrome（${CHROME}）：DevToolsActivePort 未出現`);
}

// ---------- 頁面內量測腳本（以 Runtime.evaluate 注入字串執行） ----------
// 回傳 {sites:[...], glyphs:{id:{vb,span,fill,...}}}；先建 glyph 尺度表，再逐站點比對。
const MEASURE_JS = String.raw`
(() => {
  const NS = 'http://www.w3.org/2000/svg';
  const container = (document.querySelector('svg[style*="display: none"]') || document);
  const defs = [...container.querySelectorAll('defs > [id^="icon-"]')];
  // 1) glyph 本徵 span：以 meet 縮放=1 的 temp <use> 量測。
  //    外框 svg 的 width/height 設成該 glyph 的 viewBox 尺寸 → symbol 的 meet scale=1，
  //    故量得值＝內容在其自身使用者單位的真實跨度（掛 <g> 者本來就是 1:1）。
  const probe = document.createElementNS(NS, 'svg');
  probe.setAttribute('style', 'position:absolute;left:-10000px;top:0;overflow:visible;');
  document.body.appendChild(probe);
  const glyphs = {};
  for (const el of defs) {
    const id = el.id;
    const vb = (el.getAttribute('viewBox') || '0 0 24 24').split(/[\s,]+/).map(Number);
    const [vbx, vby, vbw, vbh] = vb.length === 4 ? vb : [0, 0, 24, 24];
    probe.setAttribute('width', String(vbw));
    probe.setAttribute('height', String(vbh));
    probe.innerHTML = '';
    const u = document.createElementNS(NS, 'use');
    u.setAttribute('href', '#' + id);
    probe.appendChild(u);
    let span;
    try { const b = u.getBBox(); span = { x: b.x, y: b.y, w: b.width, h: b.height }; }
    catch (e) { span = null; }
    glyphs[id] = {
      tag: el.tagName.toLowerCase(), vb: [vbx, vby, vbw, vbh], span,
      fillX: span ? span.w / vbw : 0, fillY: span ? span.h / vbh : 0,
    };
    glyphs[id].domFill = Math.max(glyphs[id].fillX, glyphs[id].fillY);
    glyphs[id].shrinkOk = glyphs[id].domFill >= 0.55;
  }
  probe.remove();
  // 2) 站點量測
  const sites = [];
  const svgs = [...document.querySelectorAll('svg.qword-icon')];
  svgs.forEach((svg, idx) => {
    const use = svg.querySelector('use');
    if (!use) return;
    const href = use.getAttribute('href') || use.getAttributeNS('http://www.w3.org/1999/xlink', 'href') || '';
    const gid = href.replace(/^#/, '');
    const g = glyphs[gid];
    const rect = svg.getBoundingClientRect();
    const ownerPanel = svg.closest('.qword-tab-panel');
    const rec = {
      site: idx, gid, tag: g ? g.tag : 'missing',
      panel: ownerPanel ? ownerPanel.id : '(outside-panels)',
      box: { w: +rect.width.toFixed(3), h: +rect.height.toFixed(3) },
      btn: (svg.closest('button,[role=button],[data-command]') || {}).getAttribute
        ? ((svg.closest('button,[data-command]') || {}).getAttribute('data-command') || '')
        : '',
      status: 'skip-zero',
    };
    if (rect.width === 0 || rect.height === 0 || !g || !g.span) { sites.push(rec); return; }
    const [vbx, vby, vbw, vbh] = g.vb;
    const s = Math.min(rect.width / vbw, rect.height / vbh);   // meet 預期縮放
    const exp = { w: g.span.w * s, h: g.span.h * s };
    let b;
    try { b = use.getBBox(); } catch (e) { rec.status = 'skip-bbox-error'; sites.push(rec); return; }
    rec.bbox = { w: +b.width.toFixed(3), h: +b.height.toFixed(3) };
    rec.expected = { w: +exp.w.toFixed(3), h: +exp.h.toFixed(3) };
    rec.fill = { x: +(b.width / rect.width).toFixed(4), y: +(b.height / rect.height).toFixed(4) };
    const overflowOk = b.width <= rect.width + 0.5 && b.height <= rect.height + 0.5;
    const uniformOk = exp.w > 0 && exp.h > 0
      && Math.abs(b.width / exp.w - 1) <= 0.02 && Math.abs(b.height / exp.h - 1) <= 0.02;
    const shrinkOk = g.shrinkOk;
    rec.checks = { overflowOk, uniformOk, shrinkOk };
    rec.status = (overflowOk && uniformOk && shrinkOk) ? 'pass' : 'fail';
    sites.push(rec);
  });
  return JSON.stringify({
    glyphCount: defs.length,
    symbolCount: defs.filter(d => d.tagName.toLowerCase() === 'symbol').length,
    legacyGCount: defs.filter(d => d.tagName.toLowerCase() === 'g').length,
    glyphs, sites,
  });
})()
`;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function evalIn(session, cdp, expr) {
  const r = await cdp.send('Runtime.evaluate', {
    expression: expr, returnByValue: true, awaitPromise: true,
  }, session.sessionId);
  if (r.exceptionDetails) throw new Error('頁內拋錯: ' + JSON.stringify(r.exceptionDetails).slice(0, 400));
  return r.result.value;
}

async function main() {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  const server = await startServer(ROOT);
  const port = server.address().port;
  const url = `http://127.0.0.1:${port}/${EDITOR_REL}`;
  const profileDir = fs.mkdtempSync(path.join(os.tmpdir(), 'u5-probe-'));
  let chrome; let cdp; let wsUrl;
  try {
    ({ child: chrome, wsUrl } = await launchChrome(profileDir));
    cdp = await new Promise((resolve, reject) => {
      const ws = new WebSocket(wsUrl);
      ws.addEventListener('open', () => resolve(new CDP(ws)));
      ws.addEventListener('error', (e) => reject(new Error('CDP WebSocket 連線失敗')));
    });
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    const session = { sessionId };
    await cdp.send('Page.enable', {}, sessionId);
    await cdp.send('Page.navigate', { url }, sessionId);
    // 等 DOM 與模組腳本就緒（圖示站點出現即可，wasm 載入與否不影響 sprite 量測）
    const deadline = Date.now() + 20000;
    let ready = false;
    while (Date.now() < deadline && !ready) {
      const n = await evalIn(session, cdp, 'document.querySelectorAll("svg.qword-icon use").length');
      ready = n > 0;
      if (!ready) await sleep(300);
    }
    if (!ready) throw new Error('頁面未出現任何 svg.qword-icon use 站點');
    await sleep(600);

    const tabButtons = await evalIn(session, cdp,
      'JSON.stringify([...document.querySelectorAll(".qword-tab-btn[data-tab-target]")].map(b=>b.getAttribute("data-tab-target")))');
    const tabs = JSON.parse(tabButtons);
    const seen = new Map();          // 站點最終判定（跨 pass 取最後一次可見量測）
    const shots = {};
    let meta = null;
    let lastGlyphs = null;

    for (const tabId of tabs) {
      await evalIn(session, cdp,
        `(() => { const b=document.querySelector('[data-tab-target="${tabId}"]'); if(!b) return 'no-btn'; b.click(); return b.getAttribute('aria-controls')||tabId; })()`);
      await sleep(250);
      const raw = await evalIn(session, cdp, MEASURE_JS);
      const data = JSON.parse(raw);
      for (const s of data.sites) {
        const key = `${s.panel}|${s.gid}|${s.btn}|${s.site}`;
        if (s.status === 'skip-zero') {
          if (!seen.has(key)) seen.set(key, s);   // 全 hidden 才保留 skip
          continue;
        }
        seen.set(key, s);                          // 可見量測覆蓋 skip-zero
      }
      if (SHOT_TABS.includes(tabId)) {
        const shot = await cdp.send('Page.captureScreenshot', { format: 'png' }, sessionId);
        const file = path.join(OUT_DIR, `u5-icons-${tabId}.png`);
        fs.writeFileSync(file, Buffer.from(shot.data, 'base64'));
        shots[tabId] = file;
      }
      // 記錄 sprite 定義形態（每輪相同，取最後一輪）
      meta = { glyphCount: data.glyphCount, symbolCount: data.symbolCount, legacyGCount: data.legacyGCount };
      lastGlyphs = data.glyphs;
    }

    const sites = [...seen.values()];
    const measured = sites.filter(s => s.status === 'pass' || s.status === 'fail');
    const fails = measured.filter(s => s.status === 'fail');
    const skipped = sites.filter(s => s.status !== 'pass' && s.status !== 'fail');

    console.log('='.repeat(72));
    console.log(' U5 圖示 bbox 探針（真瀏覽器 · 逐 tab 切換量測）');
    console.log('='.repeat(72));
    console.log(`sprite 定義數: ${meta.glyphCount}（symbol ${meta.symbolCount} / 殘留 g ${meta.legacyGCount}）`);
    console.log(`tab 數: ${tabs.length}（${tabs.join(', ')}）`);
    console.log(`站點總數: ${sites.length}｜實量: ${measured.length}｜跳過(0x0 隱藏): ${skipped.length}`);
    console.log(`PASS: ${measured.length - fails.length}｜FAIL: ${fails.length}`);
    if (fails.length) {
      console.log('\n--- 失敗清單（逐個點名）---');
      for (const f of fails) {
        const c = f.checks || {};
        console.log(`[FAIL] #${f.site} #${f.gid} @${f.panel}${f.btn ? ' btn=' + f.btn : ''} ` +
          `盒 ${f.box.w}x${f.box.h}｜bbox ${f.bbox ? f.bbox.w + 'x' + f.bbox.h : '?'}｜` +
          `預期 ${f.expected ? f.expected.w + 'x' + f.expected.h : '?'}｜` +
          `溢框${c.overflowOk ? 'ok' : '紅'} 等比${c.uniformOk ? 'ok' : '紅'} 內縮${c.shrinkOk ? 'ok' : '紅'}`);
      }
    }
    if (skipped.length) {
      console.log(`\n跳過明細（0x0 未顯形，非缺陷）: ${skipped.map(s => `#${s.site} #${s.gid}@${s.panel}`).join(', ') || '無'}`);
    }
    console.log('\n截圖:');
    for (const [t, f] of Object.entries(shots)) console.log(`  ${t}: ${f}`);
    if (JSON_OUT) {
      fs.writeFileSync(JSON_OUT, JSON.stringify({ meta, tabs, glyphs: lastGlyphs, sites, fails: fails.map(f => f.site) }, null, 2));
      console.log(`JSON: ${JSON_OUT}`);
    }
    console.log('\n' + (fails.length === 0 ? 'RESULT: PASS' : 'RESULT: FAIL'));
    process.exitCode = fails.length === 0 ? 0 : 1;
  } finally {
    if (cdp && cdp.ws) { try { cdp.ws.close(); } catch {} }
    if (chrome) { try { chrome.kill('SIGKILL'); } catch {} }
    server.close();
    try { fs.rmSync(profileDir, { recursive: true, force: true }); } catch {}
  }
}

main().catch((e) => { console.error('探針錯誤:', e.message); process.exit(2); });
