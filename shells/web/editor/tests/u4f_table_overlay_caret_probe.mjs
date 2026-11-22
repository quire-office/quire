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
 * 判定（v2，t_93937dc5 交付改版）：
 *   - OVERLAY：任一頁「正文行×表格框線／格內行交疊」或「真字距 text×text
 *     （引擎 caret_rect 復測）>1pt」即紅。v1 兩缺陷已修：Rule 欄位讀 y1
 *     （v1 讀 y_pt 恆 undefined＝rule×text 恆 0 假綠）；寬度改引擎真字距
 *     （v1 0.6×字級粗估對 CJK 行高估 67%，偽重疊滿屏）。
 *   - CARET_DEAD：任一頁 BODY 歸因（該 y 有可定位段落 caret_rect 行跨度覆蓋）
 *     命中率 < 70% 即紅。CELL（格內段，caret_at None 屬 v1 契約——格內編輯
 *     走 marker-grid 路徑）與 EMPTY（正當空白）不計死區；raw 命中率照輸出，
 *     供對分修前基線。
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
  //
  // U4f v2（t_93937dc5 交付）：兩處探針自身缺陷修正＋命中圖譜歸因判定制。
  // (i) Rule 幾何欄位是 y1/y2（水平框線 y1==y2），v1 取 r.y_pt／r.y 恆 undefined
  //     ⇒ rule×text 判據從落庫起就恆 0（假綠，卡面 35/21/12/11/10 是 text×text
  //     側計數）。v2 改讀 y1，跑版主指標（rule×text 與真字距 text×text）如實紅。
  // (ii) v1 判定制把「表格格內段 None」算進死區：caret_at 對格內段回 None 是
  //     v1 既定契約（格內編輯走 marker-grid 路徑，docs 已載），raw 命中率對
  //     表格頁先天無鑑別力。v2 逐掃描樣本三分歸因（引擎正向 caret_rect 為真值）：
  //       BODY＝該 y 有可定位段落行跨度覆蓋（正文行）→ 必須命中（門檻 >70%），
  //              漏點逐條進 bodyMiss 樣本（坐標，零內文）；
  //       CELL＝該 y 有 DL 文字行但無任何 caret_rect 覆蓋＝格內段，None 屬
  //              v1 契約，另行計數不計死區；
  //       EMPTY＝皆非，None 屬正當（Word 對空白點擊亦不落 caret）。
  //     raw 命中率仍逐頁輸出（對分 PM 修前基線用），判定制以 BODY 為準。
  const result = await evalIn(String.raw`(() => {
    const a = window.QWordEngine;
    const e = a.engine;
    const pages = a.pageCount();
    const perPage = [];
    let overlayTotal = 0;
    // 正向行跨度真值表：逐段逐 cp 取 caret_rect（None＝該段不可定位＝格內段），
    // 依 (page, y) 去重成行。段數×cp 上限為探針成本閘（長段抽樣步進 4 cp）。
    const careBands = [];
    const paras = a.paragraphCount();
    for (let p = 0; p < paras; p++) {
      let lastKey = '';
      for (let cp = 0; cp <= 400; cp += 4) {
        const r = a.caretRect ? a.caretRect(p, cp) : null;
        if (!r) { if (cp > 200) break; continue; }
        const key = r.page + ':' + Math.round(r.y_pt);
        if (key === lastKey) continue;
        lastKey = key;
        careBands.push({ pg: r.page, y0: r.y_pt, y1: r.y_pt + (r.height_pt || 12) });
      }
    }
    // caret_rect.y 是行頂，DL baseline 在行頂下方（上升部）——覆蓋判據容差放到
    // 1.2×行高，否則行尾 baseline 的正文行會被誤判成格內行（用戶檔實測教訓）。
    const coversY = (pg, y) => careBands.some(b => b.pg === pg && y >= b.y0 - 1 && y < b.y0 + (b.y1 - b.y0) * 1.2 + 1);
    // 該 y 是否落在可定位段落行跨度（BODY 掃描判據；容差＝半行高）。
    const onBodyBand = (pg, y) => careBands.some(b => b.pg === pg && y >= b.y0 - 0.5 && y < b.y1 - 0.5);
    for (let pg = 0; pg < pages; pg++) {
      const dl = a.getPageDisplayList(pg);
      const rules = (dl ? dl.items : []).filter(i => i.Rule).map(i => i.Rule);
      const texts = (dl ? dl.items : []).filter(i => i.Text).map(i => i.Text);
      const ruleYs = rules.map(r => (typeof r.y1 === 'number' ? r.y1 : (typeof r.y_pt === 'number' ? r.y_pt : r.y))).filter(v => typeof v === 'number');
      const cellLines = texts.filter(t => !coversY(pg, t.y));
      const bodyLines = texts.filter(t => coversY(pg, t.y));
      // Rule×Text／跑版主徵（v2 精確判據，v1 兩處缺陷見檔頭註解）：
      //   (a) 正文行與格內文字行同 y 帶且 x 實質交疊（>1pt）＝正文排進表格區；
      //   (b) 正文行字框被水平框線穿過（|框線−baseline| < 0.5×字級）且框線 y
      //       在該頁框線全集的跨度內＝文字壓在表格中段框線上。
      // 表格正下方/正上方的正文行與框線距離≈字上升部（≥0.6×字級）不誤紅；
      // 格內行天生貼框線不計紅（v1 連格內一起計，紅綠皆無鑑別力），另計 cellNearRule。
      let overlaps = 0, cellNearRule = 0;
      const samples = [];
      const ruleSpan = ruleYs.length ? [Math.min(...ruleYs), Math.max(...ruleYs)] : null;
      for (const t of bodyLines) {
        const w = t.text.length * t.size_pt * 0.6;
        const clashCell = cellLines.some(c => Math.abs(c.y - t.y) < Math.min(c.size_pt, t.size_pt) * 0.9 &&
          Math.min(t.x + w, c.x + c.text.length * c.size_pt * 0.6) - Math.max(t.x, c.x) > 1);
        const onRule = ruleSpan && t.y > ruleSpan[0] + 0.6 && t.y < ruleSpan[1] - 0.6 &&
          ruleYs.some(ry => Math.abs(ry - t.y) < t.size_pt * 0.5);
        if (clashCell || onRule) {
          overlaps++;
          // 隱私鐵律：只記結構度量（坐標），嚴禁帶出文件內文。
          if (samples.length < 3) samples.push({ textY: Math.round(t.y), clashCell, onRule: onRule ? ruleYs.filter(ry => Math.abs(ry - t.y) < t.size_pt * 0.5).map(Math.round) : null, sizePt: t.size_pt });
        }
      }
      for (const t of cellLines) {
        if (ruleYs.some(ry => Math.abs(ry - t.y) < t.size_pt * 0.9)) cellNearRule++;
      }
      overlayTotal += overlaps;
      // Text×Text（跑版副徵）：y 相近且 x 實質交疊（>4pt）。v1 用 0.6×字級
      // 粗估寬——對 CJK 行（真寬≈1.0×字級）嚴重高估，同行相接 run 全被誤報
      // （用戶檔 49 對裡 48 對是偽訊號）。v2 以引擎正向 caret_rect 取**真字距**：
      //   左項真終點＝caret_rect(反查段, cp+len).x（同頁同行才可用）；
      //   真重疊＝左真終點 − 右起點 > 1pt；相接（≤1pt）或反查不到（格內段，
      //   v1 契約 None）→ 不計紅，後者計 unmeasurable 供覆核。
      let textOverlaps = 0, unmeasurable = 0;
      const tsamples = [];
      const truth = new Map(); // 文字行 → {end, start}（caret 框；null＝不可測）
      // 引擎真字距一律取 caret_rect 框（DL 的 x 是墨跡起點，與 caret 推進框
      // 差可達半字——跨框比較會造出假重疊；用戶檔 p12 實測 r1.x(L)==r0.x(R)
      // 零縫卻被 DL 框判出 6pt 假紅，故 L 終點與 R 起點必須同框）。
      const trueEnd = (t) => {
        const key = t.x + ':' + t.y;
        if (truth.has(key)) return truth.get(key);
        let end = null, start = null;
        const at = e.caret_at_json(pg, t.x + 1, t.y + t.size_pt * 0.3);
        if (at) {
          const q = JSON.parse(at);
          const r0 = a.caretRect(q.para, q.cp);
          const r1 = a.caretRect(q.para, q.cp + t.text.length);
          if (r0 && r1 && r1.page === r0.page && Math.abs(r1.y_pt - r0.y_pt) < 0.5) { end = r1.x_pt; start = r0.x_pt; }
        }
        truth.set(key, { end, start });
        return truth.get(key);
      };
      for (let i = 0; i < texts.length; i++) {
        for (let j = i + 1; j < texts.length; j++) {
          const t1 = texts[i], t2 = texts[j];
          const dy = Math.abs(t1.y - t2.y);
          const sz = Math.min(t1.size_pt, t2.size_pt);
          if (dy >= sz * 0.9) continue;
          const w1 = t1.text.length * t1.size_pt * 0.6;
          const w2 = t2.text.length * t2.size_pt * 0.6;
          const ix = Math.min(t1.x + w1, t2.x + w2) - Math.max(t1.x, t2.x);
          const sameRunRow = dy < 0.5 && (Math.abs(t1.x + w1 - t2.x) < 1.5 || Math.abs(t2.x + w2 - t1.x) < 1.5);
          if (ix <= 4 || sameRunRow) continue;
          const L = t1.x <= t2.x ? t1 : t2, R = t1.x <= t2.x ? t2 : t1;
          const le = trueEnd(L), rs = trueEnd(R);
          if (le.end === null || rs.start === null) { unmeasurable++; continue; }
          const tix = le.end - rs.start;
          if (tix > 1.0) {
            textOverlaps++;
            // 隱私鐵律：只記結構度量，不帶內文。
            if (tsamples.length < 3) tsamples.push({ y: Math.round(L.y), xL: Math.round(le.start), end: Math.round(le.end), xR: Math.round(rs.start), tix: Math.round(tix) });
          }
        }
      }
      // caret 掃描（PM 口徑 x=120、y=72..720 步 12）：raw 命中仍計（對分基線），
      // 判定以 BODY 歸因為準（CELL＝格內段 None 屬 v1 契約；EMPTY 屬正當）。
      let hit = 0, miss = 0, bodyTot = 0, bodyHit = 0, cellTot = 0, cellHit = 0, emptyTot = 0, emptyHit = 0;
      const bodyMiss = [];
      for (let y = 72; y <= 720; y += 12) {
        const v = e.caret_at_json(pg, 120, y);
        const got = !(v === undefined || v === null);
        hit += got ? 1 : 0; miss += got ? 0 : 1;
        if (onBodyBand(pg, y)) {
          bodyTot++; bodyHit += got ? 1 : 0;
          if (!got && bodyMiss.length < 8) bodyMiss.push(y);
        } else if (texts.some(t => y >= t.y - t.size_pt * 0.9 && y <= t.y + t.size_pt * 0.35)) {
          cellTot++; cellHit += got ? 1 : 0;
        } else {
          emptyTot++; emptyHit += got ? 1 : 0;
        }
      }
      perPage.push({ pg, rules: rules.length, texts: texts.length, overlaps, cellNearRule, samples, textOverlaps, unmeasurable, tsamples, caretHit: hit, caretMiss: miss, bodyHit, bodyTot, bodyMiss, cellHit, cellTot, emptyHit, emptyTot });
    }
    return { pages, chars: a.getWordCount(), paraCount: a.paragraphCount(), overlayTotal, perPage };
  })()`);

  const shot = await cdp.send('Page.captureScreenshot', { format: 'png' }, sessionId);
  fs.writeFileSync(SHOT_OUT, Buffer.from(shot.data, 'base64'));

  // 判定制 v2：跑版＝rule×text（正文×表格框線/格內交疊）與 text×text（真字距）
  // 皆 0；caret 死區＝BODY 歸因命中率 <70%（CELL＝格內段 None 屬 v1 契約、
  // EMPTY＝正當，皆不計死區）。raw 命中率保留輸出供對分修前基線。
  const BODY_MIN = 0.7;
  const overlayRed = result.perPage.filter((p) => p.overlaps > 0 || p.textOverlaps > 0);
  const caretRed = result.perPage.filter((p) => p.bodyTot > 0 && p.bodyHit / p.bodyTot < BODY_MIN);
  const summary = {
    docx: path.basename(DOCX), pages: result.pages, chars: result.chars, paraCount: result.paraCount,
    overlayRedPages: overlayRed.map((p) => p.pg), caretRedPages: caretRed.map((p) => p.pg),
    shot: SHOT_OUT,
  };
  console.log('U4F_SUMMARY ' + JSON.stringify(summary));
  for (const p of result.perPage) {
    const rawTot = p.caretHit + p.caretMiss;
    const bodyRate = p.bodyTot ? (p.bodyHit / p.bodyTot * 100).toFixed(1) : 'n/a';
    const flagged = p.overlaps > 0 || p.textOverlaps > 0 || (p.bodyTot > 0 && p.bodyHit / p.bodyTot < BODY_MIN);
    if (flagged || p.unmeasurable > 0) {
      console.log(`  頁${p.pg}: rules=${p.rules} texts=${p.texts} 正文×表格框線=${p.overlaps} 真字距text×text=${p.textOverlaps}(不可測=${p.unmeasurable}) BODY命中=${p.bodyHit}/${p.bodyTot}(${bodyRate}%) CELL=${p.cellHit}/${p.cellTot} raw=${p.caretHit}/${rawTot}${flagged ? ' 樣本=' + JSON.stringify((p.samples && p.samples.length ? p.samples : p.tsamples) || []).slice(0, 160) : ''}`);
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
