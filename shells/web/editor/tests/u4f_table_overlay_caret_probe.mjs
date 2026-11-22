#!/usr/bin/env node
/**
 * u4f_table_overlay_caret_probe.mjs
 *
 * U4f 缺陷複現探針（用戶實測檔「Gemini 4 Hydrogen 資訊調查.docx」）：
 *   缺陷一（跑版）：表格區文字與表格框線／其他列文字重疊發射。
 *   缺陷二（游標死區）：表格頁與表格之後的內容 caret_at_json 大面積回 None，
 *     用戶表現為「無法點擊定位、無法改變 cursor 位置、完全無法編輯」。
 *
 * 原理：
 *   - Node 內建 http 起本機靜態伺服器 + headless Chrome（CDP flatten session）。
 *   - 以 DOM.setFileInputFiles 把 docx 塞給 #qword-file-input 並派 change
 *     （與用戶手選檔案同一條 loadFile 路徑，零捷徑）。
 *   - 逐頁解析 display list：Rule 的 y 與 Text 行的 y 接近（<0.9×字級）記重疊。
 *   - 逐頁以 12pt 間距掃 y=72..720 問 caret_at_json，統計命中／None。
 *   - 截圖存檔供人工覆核。
 *
 * 判定：
 *   - OVERLAY：任一页 Rule×Text y 重疊數 ≥ 1 即紅（表格區發射疊字）。
 *   - CARET_DEAD：任一頁 caret 命中率 < 30% 即紅（游標死區）。
 *   退出碼：0 全綠；1 有紅燈；2 探針自身錯誤。
 *
 * 用法：node shells/web/editor/tests/u4f_table_overlay_caret_probe.mjs [docx路徑] [--json F] [--shot F]
 * 預設 docx：$QUIRE_U4F_DOCX，或 repo 根的 user-sample 不存在時直接報錯退出 2
 * （本探針刻意不把任何用戶文件拷進 repo——隱私鐵律）。
 */
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const argv = process.argv.slice(2);
function argOf(flag, dflt) {
  const i = argv.indexOf(flag);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : dflt;
}
const DOCX = argv.find((a) => !a.startsWith('--')) || process.env.QUIRE_U4F_DOCX || '';
const JSON_OUT = argOf('--json', null);
const SHOT_OUT = argOf('--shot', path.join(os.tmpdir(), 'u4f-overlay-shot.png'));
const CHROME = process.env.CHROME_BIN || '/usr/bin/google-chrome';
const EDITOR_REL = 'shells/web/editor/index.html';

if (!DOCX || (DOCX !== 'SAMPLE' && !fs.existsSync(DOCX))) {
  console.error('需要 docx 路徑（argv 或 QUIRE_U4F_DOCX），且檔必須存在。探針不自帶文件。');
  process.exit(2);
}
const MIME = {
  '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm', '.json': 'application/json', '.png': 'image/png', '.svg': 'image/svg+xml',
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

class CDP {
  constructor(ws) {
    this.ws = ws; this.nextId = 1; this.pending = new Map(); this.events = [];
    ws.addEventListener('message', (ev) => {
      const m = JSON.parse(typeof ev.data === 'string' ? ev.data : ev.data.toString());
      if (m.method === 'Runtime.exceptionThrown') {
        this.events.push('PAGEERR: ' + String(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text).slice(0, 300));
      }
      if (m.id && this.pending.has(m.id)) {
        const { resolve, reject } = this.pending.get(m.id);
        this.pending.delete(m.id);
        if (m.error) reject(new Error(`CDP ${m.method}: ${m.error.message}`)); else resolve(m.result);
      }
    });
  }
  send(method, params = {}, sessionId) {
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
  const args = ['--headless=new', '--disable-gpu', '--no-sandbox', '--disable-dev-shm-usage',
    '--remote-debugging-port=0', `--user-data-dir=${profileDir}`, '--window-size=1440,900', 'about:blank'];
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
  throw new Error(`無法啟動 Chrome（${CHROME}）`);
}

const server = await startServer(ROOT);
const port = server.address().port;
const url = `http://127.0.0.1:${port}/${EDITOR_REL}`;
const profileDir = fs.mkdtempSync(path.join(os.tmpdir(), 'u4f-probe-'));
let chrome; let cdp;
try {
  const { child, wsUrl } = await launchChrome(profileDir);
  chrome = child;
  cdp = await new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    ws.addEventListener('open', () => resolve(new CDP(ws)));
    ws.addEventListener('error', () => reject(new Error('CDP WebSocket 連線失敗')));
  });
  const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
  await cdp.send('Runtime.enable', {}, sessionId);
  await cdp.send('Page.enable', {}, sessionId);
  await cdp.send('Page.navigate', { url }, sessionId);

  const evalIn = async (expr, awaitPromise = false) => {
    const r = await cdp.send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise }, sessionId);
    if (r.exceptionDetails) throw new Error('頁內例外: ' + String(r.exceptionDetails.exception?.description || r.exceptionDetails.text).slice(0, 300));
    return r.result ? r.result.value : null;
  };
  const deadline = Date.now() + 25000;
  let ready = false;
  while (Date.now() < deadline && !ready) {
    ready = await evalIn('!!(window.QWordShell && window.QWordEngine)').catch(() => false);
    if (!ready) await new Promise((r) => setTimeout(r, 300));
  }
  if (!ready) throw new Error('編輯器未就緒（shell/engine 全域未掛載）\n' + cdp.events.slice(0, 6).join('\n'));

  // 真檔案選擇器路徑開檔；DOCX==="SAMPLE" 時改開內建樣本（對照組）
  if (path.resolve(DOCX) === path.resolve('SAMPLE') || DOCX === 'SAMPLE') {
    await evalIn('window.QWordShell.openDefaultSample()');
  } else {
    const doc = await cdp.send('DOM.getDocument', {}, sessionId);
    const { nodeId } = await cdp.send('DOM.querySelector', { nodeId: doc.root.nodeId, selector: '#qword-file-input' }, sessionId);
    const desc = await cdp.send('DOM.describeNode', { nodeId }, sessionId);
    await cdp.send('DOM.setFileInputFiles', { files: [path.resolve(DOCX)], backendNodeId: desc.node.backendNodeId }, sessionId);
    await evalIn("(() => { const el = document.getElementById('qword-file-input'); el.dispatchEvent(new Event('change', { bubbles: true })); return 1; })()");
  }
  // 等開檔＋排版
  const t0 = Date.now();
  let opened = false;
  while (Date.now() - t0 < 30000 && !opened) {
    opened = await evalIn('!!(window.QWordEngine.engine && window.QWordEngine.isSettled && window.QWordEngine.isSettled())').catch(() => false);
    if (!opened) await new Promise((r) => setTimeout(r, 400));
  }
  if (!opened) throw new Error('開檔 30s 內未定案\n' + cdp.events.slice(0, 6).join('\n'));

  // 量測：逐頁 Rule×Text 重疊＋caret 命中圖譜
  const result = await evalIn(String.raw`(() => {
    const a = window.QWordEngine;
    const e = a.engine;
    const pages = a.pageCount();
    const perPage = [];
    let overlayTotal = 0;
    for (let pg = 0; pg < pages; pg++) {
      const dl = a.getPageDisplayList(pg);
      const rules = (dl ? dl.items : []).filter(i => i.Rule).map(i => i.Rule);
      const texts = (dl ? dl.items : []).filter(i => i.Text).map(i => i.Text);
      const ruleYs = rules.map(r => (typeof r.y_pt === 'number' ? r.y_pt : r.y)).filter(v => typeof v === 'number');
      let overlaps = 0;
      const samples = [];
      for (const t of texts) {
        for (const ry of ruleYs) {
          if (Math.abs(ry - t.y) < t.size_pt * 0.9) {
            overlaps++;
            // 隱私鐵律：只記結構度量（坐標），嚴禁帶出文件內文。
            if (samples.length < 3) samples.push({ textY: Math.round(t.y), ruleY: Math.round(ry), sizePt: t.size_pt });
            break;
          }
        }
      }
      overlayTotal += overlaps;
      // Text×Text 重疊（跑版主徵：表格列文字與正文文字同 y 段交疊發射）。
      // 同段同行多 run 屬正當（y 近且 x 連續相接），排除之；
      // 異常＝兩段 y 差 < 0.9×較小字級 且 x 區間實質交疊（>4pt）。
      let textOverlaps = 0;
      const tsamples = [];
      for (let i = 0; i < texts.length; i++) {
        for (let j = i + 1; j < texts.length; j++) {
          const t1 = texts[i], t2 = texts[j];
          const dy = Math.abs(t1.y - t2.y);
          const sz = Math.min(t1.size_pt, t2.size_pt);
          if (dy >= sz * 0.9) continue;
          const w1 = t1.width_pt || (t1.text.length * t1.size_pt * 0.6);
          const w2 = t2.width_pt || (t2.text.length * t2.size_pt * 0.6);
          const ix = Math.min(t1.x + w1, t2.x + w2) - Math.max(t1.x, t2.x);
          const sameRunRow = dy < 0.5 && (Math.abs(t1.x + w1 - t2.x) < 1.5 || Math.abs(t2.x + w2 - t1.x) < 1.5);
          if (ix > 4 && !sameRunRow) {
            textOverlaps++;
            // 隱私鐵律：只記結構度量，不帶內文。
            if (tsamples.length < 3) tsamples.push({ y1: Math.round(t1.y), y2: Math.round(t2.y), x1: Math.round(t1.x), x2: Math.round(t2.x), ix: Math.round(ix) });
          }
        }
      }
      let hit = 0, miss = 0;
      const hits = [];
      for (let y = 72; y <= 720; y += 12) {
        const v = e.caret_at_json(pg, 120, y);
        if (v === undefined || v === null) miss++; else { hit++; if (hits.length < 2) hits.push(y); }
      }
      perPage.push({ pg, rules: rules.length, texts: texts.length, overlaps, samples, textOverlaps, tsamples, caretHit: hit, caretMiss: miss });
    }
    return { pages, chars: a.getWordCount(), paraCount: a.paragraphCount(), overlayTotal, perPage };
  })()`);

  const shot = await cdp.send('Page.captureScreenshot', { format: 'png' }, sessionId);
  fs.writeFileSync(SHOT_OUT, Buffer.from(shot.data, 'base64'));

  const CARET_MIN = 0.3;
  const overlayRed = result.perPage.filter((p) => p.overlaps > 0 || p.textOverlaps > 0);
  const caretRed = result.perPage.filter((p) => p.caretHit / (p.caretHit + p.caretMiss) < CARET_MIN);
  const summary = {
    docx: path.basename(DOCX), pages: result.pages, chars: result.chars, paraCount: result.paraCount,
    overlayRedPages: overlayRed.map((p) => p.pg), caretRedPages: caretRed.map((p) => p.pg),
    shot: SHOT_OUT,
  };
  console.log('U4F_SUMMARY ' + JSON.stringify(summary));
  for (const p of result.perPage) {
    if (p.overlaps > 0 || p.textOverlaps > 0 || p.caretMiss > p.caretHit) {
      console.log(`  頁${p.pg}: rules=${p.rules} texts=${p.texts} rule×text重疊=${p.overlaps} text×text重疊=${p.textOverlaps} caret=${p.caretHit}/${p.caretHit + p.caretMiss} 樣本=${JSON.stringify(p.tsamples).slice(0, 160)}`);
    }
  }
  if (JSON_OUT) fs.writeFileSync(JSON_OUT, JSON.stringify(result, null, 2));
  if (cdp.events.length) console.log('PAGE_EVENTS: ' + cdp.events.slice(0, 5).join(' | '));
  const red = overlayRed.length > 0 || caretRed.length > 0;
  console.log(red ? 'U4F_PROBE=RED' : 'U4F_PROBE=GREEN');
  process.exitCode = red ? 1 : 0;
} catch (err) {
  console.error('探針錯誤: ' + (err && err.message));
  process.exitCode = 2;
} finally {
  if (chrome) chrome.kill('SIGKILL');
  server.close();
}
