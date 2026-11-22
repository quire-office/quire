/**
 * u4g_underline_canvas_probe.mjs — U4g：web 殼 canvas 渲染器消費 DrawItem::Text.underline
 * 繪形態幾何的瀏覽器閉環探針（真 Chrome／Canvas2D 像素指紋）。
 *
 * 驗收口徑（用戶看見那層＝canvas 像素，非旗標、非存檔件——檔契約 `w:u/@w:val`
 * 面已由 u4f_underline_style_live_probe 鎖死，本探針不重複那層，專驗繪製層）：
 *
 * A 層（真閉環 E2E）：真 DOM click → bindEvents 委派 → core `format:underline-style`
 * 臂 → UI handler → refreshPages → adapter.renderPage → vendor/renderer.js →
 * 目標段基線下緣帶行掃描像素指紋：
 *   - 無底線基準零筆畫（探針自證，否則指紋無意義）；
 *   - 逐形態 underline 欄真的從引擎流到渲染器（欄值逐字對）＋筆畫帶形態差異；
 *   - single＝恰一筆畫帶；double＝恰兩條分離筆畫帶（修前單線畫法永不可能）；
 *   - thick＝筆畫帶寬 / single ≥ THICK_RATIO_MIN（真值倍率 ≈0.13/0.07≈1.86）；
 *   - dotted/dashed＝筆畫帶 x 輪廓段級離散（連通白縫數達標）且 dotted 段數多
 *     於 dashed（段長 1:4 倍率口徑）；修前連續單線白縫數＝0；
 *   - 探針先把目標段字級放大（真 DOM font:grow 鈕 ×7 檔到 26pt）再測：12pt 時
 *     筆寬 <2px、被 1px 保底吃掉，形態差異在像素層不可辨（老實記帳：低字級
 *     不承諾指紋鑑別力）。
 *
 * B 層（合成顯示列表直繪，確定性＋回塞鑑別主力）：在頁內 import 真 renderer.js，
 * 直接餵手造 `PageDisplayList`（同一 Text 項 × 多組 underline 欄態，旗標恆
 * 點亮），驗卡面三態口徑逐條——這層能造出引擎發射層造不出的關鍵鑑別態：
 *   - `flags&4 且 underline:'off'` → 一筆都不畫（修前只唸旗標的渲染器在此殘留
 *     單線＝鑑別點；引擎面 Off 恆熄滅旗標，故只能合成）；
 *   - `{'raw':'wavy'}` → 降級恰一筆畫單線＋零例外拋出（禁 panic 的 JS 口徑：
 *     渲染輪完整回傳、window error 零命中、console.error 零命中）；
 *   - 未知字面 `'dashLongHeavy'` → 同樣降級單線不拋錯；
 *   - 欄位缺失（未宣告）＋旗標點亮 → 單線兜底且與 single 指紋全等（serde
 *     default 向後相容零位移）。
 *   A 層證據負責「資料真的從引擎到畫布」，B 層負責「口徑逐條＋回塞鑑別」。
 *
 * C 層（讀取端保形 Raw 真檔閉環）：寫入端誠實拒 Raw（E-2b 燈 3），發射層 Raw
 * 通道只能由**讀取端**保形抵達（corpus fdo61343 實見 words）。node 端造一份
 * document.xml 含 `<w:u w:val="wavy"/>` 的真 docx（全 store ZIP），真開檔 →
 * 引擎保形發射 `underline:{raw:'wavy'}` → canvas 降級單線零例外。
 *
 * 雙端同步鎖：fetch 本檔所載 renderer.js 源碼，逐字比對 ULINE_GEOM 四組常數＝
 * `shells/native/src/render_svg.rs` `uline_geom()`（E-2 落點）取樣值，防兩端漂移。
 *
 * 回塞鎖：注入 `window.process={env:{U4G_UNDERLINE_BUGGY:'1'}}` 後重繪
 * （renderer.js underlineBuggy() 讀 process.env；瀏覽器端本無 process，注入即
 * 回塞），形態全折回單線 → 形態盞必紅。隨貨
 * scripts/u4g_underline_canvas_buggy_replay.sh 跑 buggy|good 兩態。
 *
 * 用法（worktree 根起靜態伺服；Chrome 需 --remote-debugging-port=9222）：
 *   python3 -m http.server 8791 --bind 127.0.0.1 &
 *   node shells/web/editor/tests/u4g_underline_canvas_probe.mjs
 * 環境變數：U4G_URL（默認 http://127.0.0.1:8791/shells/web/editor/index.html）、
 *           CDP_URL（默認 http://127.0.0.1:9222）、U4G_OUT（證據 JSON）、
 *           U4G_BUGGY=1（回塞態）、U4G_CALIB=1（校正態：只出實測指紋不判紅綠）。
 */
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const BASE_URL = process.env.U4G_URL || 'http://127.0.0.1:8791/shells/web/editor/index.html';
const OUT = process.env.U4G_OUT || path.join(ROOT, 'docs/word-ui-u4g-canvas-evidence.json');
const CDP_HTTP = process.env.CDP_URL || 'http://127.0.0.1:9222';
const BUGGY = process.env.U4G_BUGGY === '1';
const CALIB = process.env.U4G_CALIB === '1'; // 校正態：不判紅綠、只出實測指紋（門檻定值用）

// 判準常數（CALIB 實跑後定值；口徑見紅綠板各盞）
const THICK_RATIO_MIN = 1.4;   // 真值倍率 0.13/0.07≈1.86；26pt 行掃描實測 5px/3px
const DOTTED_GAPS_MIN = 20;    // 段級離散下限（真 CJK 段與合成段同閘；CALIB 實調）
const DASHED_GAPS_MIN = 10;    // 段長 4:2 倍率 → 縫數明顯少於 dotted
const GAP_ORDER_MIN = 10;      // dotted 縫數 − dashed 縫數（段長 1:4 序差）
const OFF_INK_SLACK_A = 400;   // A 層 off 歸零容差（CJK 字底殘影行墨量；實線墨量遠超）
const GROW_STEPS = 7;          // 放大檔數（SIZES 12→…→26pt；≤18pt 筆寬被保底吃掉）
const WAVY_FIXTURE_REL = 'tmp/scratch_u4g_wavy.docx'; // node 產、頁面經 http 取
const GEOM_SOURCE = {
  double: 'double: { offsets: [0.13, 0.25], width: 0.055, dash: null }',
  thick: 'thick: { offsets: [0.15], width: 0.13, dash: null }',
  dotted: 'dotted: { offsets: [0.15], width: 0.09, dash: [1.0, 2.0] }',
  dashed: 'dashed: { offsets: [0.15], width: 0.075, dash: [4.0, 2.0] }',
};

// ---------------------------------------------------------------------------
// 讀取端保形 fixture（node 端）：取樣本 docx（adapter 內嵌 base64，零外部
// 依賴），把目標 run 的 rPr 塞 <w:u w:val="wavy"/>，全部 entry 以 store
// （method 0）重建 ZIP；寫入 tmp/（.gitignore 已覆蓋）由頁面經 http 取用。
// ---------------------------------------------------------------------------
function buildWavyDocx(sampleBytes) {
  const buf = Buffer.from(sampleBytes);
  const entries = [];
  for (let i = 0; i + 4 <= buf.length; i += 1) {
    if (buf.readUInt32LE(i) !== 0x04034b50) continue;
    const method = buf.readUInt16LE(i + 8);
    const cSize = buf.readUInt32LE(i + 18);
    const nameLen = buf.readUInt16LE(i + 26);
    const extraLen = buf.readUInt16LE(i + 28);
    const start = i + 30 + nameLen + extraLen;
    const name = buf.subarray(i + 30, i + 30 + nameLen).toString('utf-8');
    const comp = buf.subarray(start, start + cSize);
    let data;
    if (method === 8) data = zlib.inflateRawSync(comp);
    else if (method === 0) data = Buffer.from(comp);
    else throw new Error(`fixture：不支援的 ZIP 壓縮法 ${method}`);
    entries.push({ name, data });
  }
  const doc = entries.find((e) => e.name === 'word/document.xml');
  if (!doc) throw new Error('fixture：查無 word/document.xml');
  let xml = doc.data.toString('utf-8');
  const anchor = xml.indexOf('這是一份');
  if (anchor < 0) throw new Error('fixture：樣本無「這是一份」錨點');
  // 目標 run：錨點前最近的 <w:r>（u4b patchedRunXml 同款錨點口徑）。rPr 只在
  // 本 run 區間（run 開頭→錨點）內找，避免把宣告插到前一個 run。
  const runOpen = xml.lastIndexOf('<w:r>', anchor);
  if (runOpen < 0) throw new Error('fixture：目標 run 起點不可定位');
  const runTagEnd = runOpen + '<w:r>'.length;
  const slice = xml.slice(runTagEnd, anchor);
  const relOpen = slice.indexOf('<w:rPr>');
  if (relOpen >= 0 && slice.indexOf('</w:rPr>', relOpen) >= 0) {
    const at = runTagEnd + relOpen + '<w:rPr>'.length;
    xml = xml.slice(0, at) + '<w:u w:val="wavy"/>' + xml.slice(at);
  } else {
    xml = xml.slice(0, runTagEnd) + '<w:rPr><w:u w:val="wavy"/></w:rPr>' + xml.slice(runTagEnd);
  }
  doc.data = Buffer.from(xml, 'utf-8');

  // 重建 ZIP（全 store，CRC 走 node zlib.crc32，零第三方依賴）
  const parts = [];
  const central = [];
  let offset = 0;
  for (const e of entries) {
    const nameBuf = Buffer.from(e.name, 'utf-8');
    const crc = (zlib.crc32 ? zlib.crc32(e.data) : require('zlib').crc32(e.data)) >>> 0;
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0, 6);
    local.writeUInt16LE(0, 8); // store
    local.writeUInt16LE(0, 10);
    local.writeUInt16LE(0x2821, 12); // 固定日期（fixture 用，穩定即可）
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(e.data.length, 18);
    local.writeUInt32LE(e.data.length, 22);
    local.writeUInt16LE(nameBuf.length, 26);
    local.writeUInt16LE(0, 28);
    parts.push(local, nameBuf, e.data);
    const cen = Buffer.alloc(46);
    cen.writeUInt32LE(0x02014b50, 0);
    cen.writeUInt16LE(20, 4);
    cen.writeUInt16LE(20, 6);
    cen.writeUInt16LE(0, 8);
    cen.writeUInt16LE(0, 10);
    cen.writeUInt16LE(0, 12);
    cen.writeUInt16LE(0x2821, 14);
    cen.writeUInt32LE(crc, 16);
    cen.writeUInt32LE(e.data.length, 20);
    cen.writeUInt32LE(e.data.length, 24);
    cen.writeUInt16LE(nameBuf.length, 28);
    cen.writeUInt32LE(0, 38);
    cen.writeUInt32LE(offset, 42);
    central.push(Buffer.concat([cen, nameBuf]));
    offset += local.length + nameBuf.length + e.data.length;
  }
  const centralBuf = Buffer.concat(central);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(entries.length, 8);
  eocd.writeUInt16LE(entries.length, 10);
  eocd.writeUInt32LE(centralBuf.length, 12);
  eocd.writeUInt32LE(offset, 16);
  return Buffer.concat([...parts, centralBuf, eocd]);
}

/**
 * 頁面 main world 內執行（序列化注入，禁閉包引用——一切外部值經 opts 傳入）。
 * @param {string} baseUrl
 * @param {boolean} buggy 回塞鎖
 * @param {{growSteps:number, wavyPath:string, synthText:string, synthSizePt:number}} opts
 */
async function runInPage(baseUrl, buggy, opts) {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const shell = () => window.QWordShell;
  const adapter = () => window.QWordEngine;
  const PAGE_W_PT = 595, PAGE_H_PT = 842, RENDER_W_PX = 794;
  const GROW = opts.growSteps, WAVY_URL = '/' + opts.wavyPath;
  const SYNTH_TEXT = opts.synthText, SYNTH_PT = opts.synthSizePt;
  // 實線列判準（CALIB t_d0a46cec 實測）：底線筆畫列覆蓋率 100%；CJI 基線下殘影
  // 列≤50%、換行第二行字頂列≤60%——≥0.90 判實線列把兩者分開（鑑別餘量 ≥40pt）。
  const SOLID_ROW = 0.90;
  // 點/段列判準：dotted 列（點＋殘影疊加）實測 63-69%，無底線同帶 ≤49%。
  const DASH_ROW_LO = 0.30, DASH_ROW_HI = 0.88;

  const HARD_DEADLINE = Date.now() + 300000;
  const stillTime = () => Date.now() < HARD_DEADLINE;
  while (stillTime()) {
    if (shell() && adapter()) break;
    await sleep(100);
  }
  if (!shell() || !adapter()) throw new Error('外殼未就緒（QWordShell 未掛載）');

  // 回塞鎖注入（renderer.js underlineBuggy() 讀 process.env；瀏覽器端原無 process）
  if (buggy) window.process = { env: { U4G_UNDERLINE_BUGGY: '1' } };
  // 零例外監視（禁拋錯口徑證據面）＋ console.error 收集
  window.__u4gErrors = [];
  window.addEventListener('error', (e) => window.__u4gErrors.push(String((e && e.message) || e)));
  if (!window.__u4gPatched) {
    const orig = console.error.bind(console);
    console.error = (...a) => { window.__u4gErrors.push('console.error:' + a.map(String).join(' ').slice(0, 160)); orig(...a); };
    window.__u4gPatched = true;
  }

  await shell().openDefaultSample();
  while (stillTime()) {
    if (adapter().engine && adapter().paragraphCount() > 0) break;
    await sleep(200);
  }
  if (!adapter().engine) throw new Error('開檔失敗：引擎實例未建立');
  await sleep(400);

  const renderer = await import('/shells/web/editor/vendor/renderer.js');
  const canvasOf = (pageIdx) => document.querySelector(`[data-page-index="${pageIdx}"] .qword-page-canvas`);

  // ---- 目標段定位：para 1 首段 Text 項（U4b/U4f 同款錨點「這是一份」）----
  const targetText = () => {
    const items = (adapter().getPageDisplayList(0).items || []).filter((i) => i.Text).map((i) => i.Text);
    for (const t of items) if (t.text && t.text.indexOf('這是一份') === 0) return t;
    return null;
  };

  // ---- 像素指紋：對指定 canvas 上目標文字列的「基線下緣帶」做行掃描 ----
  // 帶高 0.42×字級 px（覆蓋 double 第二條 0.25＋筆寬，不含 0.3 刪除線帶）。
  // 實線列＝覆蓋率 ≥SOLID_ROW；點/段列＝[DASH_ROW_LO, DASH_ROW_HI)；聚簇＝連通實線列。
  function fingerprintOn(canvas, t) {
    if (!canvas || !t) return { error: 'canvas 或目標段缺失' };
    const scale = RENDER_W_PX / (canvas.__u4gWidthPt || PAGE_W_PT);
    const px = t.size_pt * scale;
    const mctx = canvas.getContext('2d');
    const italic = t.flags & 2 ? 'italic ' : '';
    const bold = t.flags & 1 ? 'bold ' : '';
    mctx.font = `${italic}${bold}${px}px "Noto Sans TC", "Microsoft JhengHei", "PingFang TC", sans-serif`;
    const wPx = mctx.measureText(t.text).width;
    const x0 = Math.floor(t.x * scale);
    const bx0 = Math.max(0, x0 + 4);
    const bx1 = Math.min(canvas.width - 1, Math.floor(x0 + wPx) - 4);
    const bandTop = Math.floor((t.y + t.size_pt) * scale + 1);
    const bandBot = Math.min(canvas.height - 1, Math.ceil((t.y + t.size_pt) * scale + px * 0.42));
    const W = Math.max(1, bx1 - bx0), H = Math.max(1, bandBot - bandTop + 1);
    const img = mctx.getImageData(bx0, bandTop, W, H);
    const d = img.data;
    const isInk = (x, y) => { const i = (y * W + x) * 4; return d[i] < 240 || d[i + 1] < 240 || d[i + 2] < 240; };
    const ratios = [];
    for (let y = 0; y < H; y += 1) {
      let ink = 0;
      for (let x = 0; x < W; x += 1) if (isInk(x, y)) ink += 1;
      ratios.push(ink / W);
    }
    // 實線列聚簇（≥SOLID_ROW 的連通行）
    const clusters = [];
    let s = -1;
    for (let y = 0; y <= H; y += 1) {
      const on = y < H && ratios[y] >= SOLID_ROW;
      if (on && s < 0) s = y;
      if (!on && s >= 0) { clusters.push({ from: s, to: y - 1, thickness: y - s }); s = -1; }
    }
    // 點/段列（dotted/dashed 專用：覆蓋率落在點列窗內、且帶週期性離散）
    let dashBest = null; // {ratio, runGaps, maxRun, row}
    for (let y = 0; y < H; y += 1) {
      if (ratios[y] < DASH_ROW_LO || ratios[y] >= DASH_ROW_HI) continue;
      let gaps = 0, runs = 0, maxRun = 0, cur = 0, gap = 0;
      for (let x = 0; x < W; x += 1) {
        if (isInk(x, y)) { if (gap > 1) gaps += 1; gap = 0; cur += 1; if (cur > maxRun) maxRun = cur; }
        else { if (cur > 0) runs += 1; cur = 0; gap += 1; }
      }
      if (cur > 0) runs += 1;
      const cand = { row: y, ratio: Math.round(ratios[y] * 100), runGaps: gaps, maxRun, runs };
      if (!dashBest || cand.runGaps > dashBest.runGaps) dashBest = cand;
    }
    // 實線聚簇的 x 連通性（實線＝長段；取最厚聚簇中線）
    let solidRun = 0;
    if (clusters.length > 0) {
      const mid = clusters.reduce((a, b) => (b.thickness > a.thickness ? b : a));
      const my = mid.from + Math.floor((mid.to - mid.from) / 2);
      let cur = 0;
      for (let x = 0; x < W; x += 1) { if (isInk(x, my)) { cur += 1; if (cur > solidRun) solidRun = cur; } else cur = 0; }
    }
    // 帶總墨量（基線下 1px 起全帶；off 歸零斷言用）
    let totalInk = 0;
    for (let y = 1; y < H; y += 1) for (let x = 0; x < W; x += 1) if (isInk(x, y)) totalInk += 1;
    return {
      clusters, strokeCount: clusters.length,
      penThickness: clusters.length ? Math.max(...clusters.map((c) => c.thickness)) : 0,
      solidRun,
      dashBest,
      dashRowCount: ratios.filter((r) => r >= DASH_ROW_LO && r < DASH_ROW_HI).length,
      totalInk,
      ratios: ratios.map((r) => Math.round(r * 100)),
      W, H, px: Math.round(px * 100) / 100,
      sizePt: t.size_pt, flags: t.flags,
      underlineField: t.underline === undefined ? null : t.underline,
      textHead: String(t.text || '').slice(0, 6),
    };
  }
  const fpLive = () => fingerprintOn(canvasOf(0), targetText());

  // ---- B 層：合成顯示列表直繪（乾淨白布＋零降部字串，確定性鑑別主力）----
  async function fingerprintSynthetic(underlineValue) {
    const cv = document.createElement('canvas');
    cv.__u4gWidthPt = PAGE_W_PT;
    const item = {
      x: 72, y: 90, size_pt: SYNTH_PT,
      flags: 4, // 旗標恆點亮：鑑別「渲染器唸不唸 underline 欄」而非唸旗標
      color_argb: 0xff000000,
      text: SYNTH_TEXT,
    };
    if (underlineValue === '__NOFLAG__') { item.flags = 0; }
    else if (underlineValue !== '__OMIT__') { item.underline = underlineValue; }
    const dl = { width_pt: PAGE_W_PT, height_pt: PAGE_H_PT, items: [{ Text: item }] };
    const before = (window.__u4gErrors || []).length;
    const meta = await renderer.renderDisplayListToCanvas(dl, cv, RENDER_W_PX);
    const fp = fingerprintOn(cv, item);
    fp.rendered = !!meta;
    fp.newErrors = (window.__u4gErrors || []).slice(before);
    return fp;
  }

  // ---- 真 DOM 路徑（U4f 同款骨架）----
  const showHome = () => shell().onCommand('tab:switch:tab-home', {});
  const arrowBtn = () => document.querySelector('[data-command="format:underline-style"]');
  const menuItem = (style) => document.querySelector(`#popover-underline-style [data-underline-style="${style}"]`);
  const growBtn = () => document.querySelector('[data-command="font:grow"]');
  const reopenMenu = async () => {
    const pop = document.getElementById('popover-underline-style');
    if (pop && pop.classList.contains('open')) return true;
    const b = arrowBtn();
    if (!b) return false;
    b.click();
    await sleep(80);
    return !!(pop && pop.classList.contains('open'));
  };
  const caretBodyPara = () => { adapter().selection = null; adapter().caret = { para: 1, cp: 0 }; };
  const clickStyle = async (style) => {
    caretBodyPara();
    const opened = await reopenMenu(); await sleep(40);
    const it = menuItem(style);
    if (!it) return { clicked: false, opened };
    it.click();
    await sleep(300); // handler async：refreshPages 重繪完成
    return { clicked: true, opened };
  };

  const R = { meta: { baseUrl, buggy: !!buggy, at: new Date().toISOString(), ua: navigator.userAgent }, e2e: {}, synth: {}, notes: {} };

  // A0) 基準：無底線段（零實線列——探針自證盞）
  showHome(); await sleep(60);
  caretBodyPara();
  R.e2e.baseline = fpLive();
  if (R.e2e.baseline.strokeCount !== 0) R.notes.baselineWarning = '基準帶非零實線列，判準失效';

  // A1) 字級放大（鑑別力：≤18pt 筆寬被 1px 保底吃掉 → grow 七檔到 26pt；
  //     走真 DOM 鈕，與用戶路徑同式）＋放大後無底線參考態
  caretBodyPara();
  for (let i = 0; i < GROW; i += 1) {
    const b = growBtn();
    if (b) b.click(); else shell().onCommand('font:grow', {});
    await sleep(180);
  }
  R.notes.grownSizePt = (targetText() || {}).size_pt;
  R.e2e.cleanAtSize = fpLive(); // 同字級無底線參考（dotted/dashed 墨量基線）

  // A2) 逐形態真閉環：單項 click → core→UI→adapter→引擎→渲染器→像素
  for (const style of ['single', 'double', 'thick', 'dotted', 'dashed']) {
    const c = await clickStyle(style);
    const fp = fpLive();
    fp.clicked = c.clicked;
    R.e2e[style] = fp;
  }

  // A3) off 真閉環：double→off（引擎旗標歸零＋欄 off；渲染器不畫）
  await clickStyle('double');
  R.e2e.beforeOff = fpLive();
  await clickStyle('off');
  R.e2e.afterOff = fpLive();

  // A4) undo drain 回基準（grow×7 ＋ 形態×7）
  let drained = 0;
  const baseGone = () => {
    const t = targetText();
    return !t || t.flags !== 0 || t.underline !== undefined || t.size_pt !== 12;
  };
  while (baseGone() && drained < 20 && stillTime()) { shell().onCommand('edit:undo', {}); await sleep(140); drained += 1; }
  R.notes.drained = drained;
  R.e2e.afterDrain = fpLive();

  // B1) 合成多態（乾淨白布＋無降部字串＋26pt：逐形態像素級鑑別主力）
  R.synth.blankFlag0 = await fingerprintSynthetic('__NOFLAG__'); // flags=0：帶內必須零墨
  R.synth.single = await fingerprintSynthetic('single');
  R.synth.double = await fingerprintSynthetic('double');
  R.synth.thick = await fingerprintSynthetic('thick');
  R.synth.dotted = await fingerprintSynthetic('dotted');
  R.synth.dashed = await fingerprintSynthetic('dashed');
  R.synth.offFlagOn = await fingerprintSynthetic('off');           // Some(Off) 不畫
  R.synth.rawWavy = await fingerprintSynthetic({ raw: 'wavy' });    // 降級單線禁拋錯
  R.synth.unknownLiteral = await fingerprintSynthetic('dashLongHeavy');
  R.synth.omitField = await fingerprintSynthetic('__OMIT__');       // 未宣告兜底單線

  // C1) 讀取端保形 Raw 真檔（node 端產 fixture；真 docx 帶 w:u w:val="wavy"）
  try {
    const before = (window.__u4gErrors || []).length;
    const resp = await fetch(WAVY_URL);
    if (!resp.ok) throw new Error(`fixture 取檔失敗 HTTP ${resp.status}`);
    const bytes = new Uint8Array(await resp.arrayBuffer());
    await shell().openFile(bytes, 'u4g-wavy.docx');
    while (stillTime()) {
      const t = targetText();
      if (t && t.underline && t.underline.raw === 'wavy') break;
      await sleep(150);
    }
    await sleep(300);
    const t = targetText();
    const fp = fingerprintOn(canvasOf(0), t);
    fp.opened = true;
    fp.newErrors = (window.__u4gErrors || []).slice(before);
    R.notes.rawFileField = t ? t.underline : null;
    R.notes.rawFileFlags = t ? t.flags : null;
    R.notes.rawFileSizePt = t ? t.size_pt : null;
    R.e2e.rawWavyFile = fp;
  } catch (e) {
    R.e2e.rawWavyFile = { error: String((e && e.message) || e) };
  }

  // D) 雙端同步鎖：源碼常數逐字（本頁 fetch，非 node fs——同源口徑）
  {
    const src = await (await fetch('/shells/web/editor/vendor/renderer.js')).text();
    const grab = (re) => { const m = src.match(re); return m ? m[0].replace(/\s+/g, ' ') : null; };
    R.notes.geomSource = {
      double: grab(/double:\s*\{[^}]*\}/),
      thick: grab(/thick:\s*\{[^}]*\}/),
      dotted: grab(/dotted:\s*\{[^}]*\}/),
      dashed: grab(/dashed:\s*\{[^}]*\}/),
      single: grab(/UNDERLINE_SINGLE_GEOM\s*=\s*\{[^}]*\}/),
    };
  }
  R.notes.uncaughtTotal = (window.__u4gErrors || []).length;
  R.notes.errors = (window.__u4gErrors || []).slice(0, 6);
  return R;
}

// ---- CDP 骨架（U4c/U4f 同款）----
async function main() {
  // node 端先產讀取端保形 fixture（頁面經 http 取）
  const { getDefaultSampleBytes } = await import(path.join(ROOT, 'shells/web/editor/js/engine-adapter.js'));
  const wavy = buildWavyDocx(getDefaultSampleBytes());
  fs.mkdirSync(path.dirname(path.join(ROOT, WAVY_FIXTURE_REL)), { recursive: true });
  fs.writeFileSync(path.join(ROOT, WAVY_FIXTURE_REL), wavy);

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
  const opts = { growSteps: GROW_STEPS, wavyPath: WAVY_FIXTURE_REL, synthText: 'QUICKBROWNFOXJUMPS', synthSizePt: 26 };
  const out = await send('Runtime.evaluate', {
    expression: `(${runInPage.toString()})(${JSON.stringify(BASE_URL)}, ${BUGGY ? 'true' : 'false'}, ${JSON.stringify(opts)})`,
    returnByValue: true, awaitPromise: true, timeout: 600000
  });
  try {
    const shot = await send('Page.captureScreenshot', { format: 'png' });
    if (shot && shot.data) fs.writeFileSync(OUT.replace(/\.json$/, `.${BUGGY ? 'buggy' : 'good'}.png`), Buffer.from(shot.data, 'base64'));
  } catch {}
  try { await fetch(`${CDP_HTTP}/json/close/${created.id}`); } catch {}
  sock.close();

  if (out.exceptionDetails) {
    console.error('探針拋例外：', JSON.stringify(out.exceptionDetails).slice(0, 3000));
    process.exit(2);
  }
  const v = out.result.value;
  const tag = BUGGY ? 'buggy' : 'good';
  fs.writeFileSync(OUT.replace(/\.json$/, `.${tag}.json`), JSON.stringify(v, null, 2));
  if (!BUGGY) fs.writeFileSync(OUT, JSON.stringify(v, null, 2));
  console.log(`證據檔 → ${OUT.replace(/\.json$/, `.${tag}.json`)}（態=${tag}）`);

  const E = v.e2e, S = v.synth;
  if (CALIB) {
    console.log('=== CALIB 校正態（不判紅綠，實測指紋）===');
    const pick = (f) => f ? {
      strokes: f.strokeCount, pen: f.penThickness, solidRun: f.solidRun,
      dash: f.dashBest, dashRows: f.dashRowCount, ink: f.totalInk,
      size: f.sizePt, clusters: f.clusters, ratios: f.ratios, err: f.error,
    } : f;
    for (const k of ['baseline', 'cleanAtSize', 'single', 'double', 'thick', 'dotted', 'dashed', 'beforeOff', 'afterOff', 'afterDrain', 'rawWavyFile'])
      console.log(`A/${k}:`, JSON.stringify(pick(E[k])));
    for (const k of Object.keys(S)) console.log(`B/${k}:`, JSON.stringify(pick(S[k])));
    console.log('notes:', JSON.stringify({ grown: v.notes.grownSizePt, drained: v.notes.drained, rawField: v.notes.rawFileField, rawFlags: v.notes.rawFileFlags, geom: v.notes.geomSource, errs: v.notes.uncaughtTotal }));
    process.exit(0);
  }

  // ---- 紅綠板 ----
  // 分層口徑（誠實紀律）：A 層（真 CJK 換行段）承諾＝欄位從引擎流到繪製器＋
  // 實線系形態（single/double/thick）像素差異＋off/基準/drain 歸零。dotted/dashed
  // 的段級像素鑑別在真 CJK 段不足（字底殘影列與點列重疊，CALIB 實測），由 B 層
  // （乾淨合成態、同一 renderer 入口）承擔逐形態幾何斷言——A 層 field 值＋B 層
  // 幾何＝完整閉環。回塞態下 A 形態盞與 B 形態盞同步翻紅。
  let fail = 0;
  const line = (cond, msg) => { console.log(`  ${cond ? '✓' : '✗'} ${msg}`); if (!cond) fail += 1; };
  const st = (f) => (f && f.strokeCount !== undefined ? f.strokeCount : -1);
  const pen = (f) => (f && f.penThickness) || 0;
  const dashGaps = (f) => (f && f.dashBest && f.dashBest.runGaps) || 0;
  const dashRun = (f) => (f && f.dashBest && f.dashBest.maxRun) || 0;
  const ink = (f) => (f && f.totalInk) || 0;
  const noStroke = (f) => !!f && f.error === undefined && f.strokeCount === 0;

  console.log(`字級放大後=${v.notes.grownSizePt}pt；drain=${v.notes.drained} 筆；基準警告=${v.notes.baselineWarning || '無'}`);
  // A 層：資料真的從引擎流到畫布（真 DOM click → core→UI→adapter→引擎→渲染器）
  line(noStroke(E.baseline), `A0 基準段零實線列（探針自證；strokes=${st(E.baseline)}）`);
  line(noStroke(E.cleanAtSize), `A1 同字級無底線參考零實線列（strokes=${st(E.cleanAtSize)}）`);
  line(noStroke(E.afterDrain), `A4 undo drain 回零實線列（strokes=${st(E.afterDrain)}）`);
  line(E.single && E.single.clicked && E.single.underlineField === 'single' && st(E.single) === 1,
    `A-single 欄值='single' 到渲染器＋恰一實線聚簇（strokes=${st(E.single)} field=${JSON.stringify(E.single && E.single.underlineField)}）`);
  line(E.double && E.double.underlineField === 'double' && st(E.double) === 2,
    `A-double 欄值='double'＋恰兩分離實線聚簇（strokes=${st(E.double)}）`);
  line(E.thick && E.thick.underlineField === 'thick' && st(E.thick) === 1 &&
    pen(E.single) > 0 && pen(E.thick) >= pen(E.single) * THICK_RATIO_MIN,
    `A-thick 實線聚簇厚 ≥ single×${THICK_RATIO_MIN}（thick=${pen(E.thick)}px single=${pen(E.single)}px）`);
  line(E.dotted && E.dotted.underlineField === 'dotted' && noStroke(E.dotted),
    `A-dotted 欄值='dotted' 到渲染器＋不畫實線（strokes=${st(E.dotted)}；段級幾何鑑別由 B 層乾淨態承擔——CJK 字底殘影列與點列重疊，CALIB 實測不承諾此層段數）`);
  line(E.dashed && E.dashed.underlineField === 'dashed' && noStroke(E.dashed),
    `A-dashed 欄值='dashed' 到渲染器＋不畫實線（strokes=${st(E.dashed)}；段級幾何同 B 層口徑）`);
  line(E.beforeOff && st(E.beforeOff) === 2 && noStroke(E.afterOff) && ink(E.afterOff) === ink(E.cleanAtSize),
    `A-off double(2簇)→off 實線歸零＋殘墨與同字級零底線參考逐像素等量（strokes=${st(E.afterOff)} ink=${ink(E.afterOff)}==clean ${ink(E.cleanAtSize)}）`);
  // B 層：合成顯示列表直繪（乾淨白布＋零降部字串＋26pt——逐形態幾何斷言主力；
  // off+旗標態為引擎發射層造不出的鑑別態：修前只唸旗標的渲染器在此必殘留）
  // off 斷言基準＝同文字 flags=0 態（同布同字同級，唯一變因＝渲染器是否唸欄）
  line(noStroke(S.blankFlag0) && st(S.single) === 1,
    `B-blank flags=0 零實線聚簇（strokes=${st(S.blankFlag0)} ink=${ink(S.blankFlag0)}，指紋窗純度自證）`);
  line(st(S.single) === 1, `B-single 恰一實線聚簇（strokes=${st(S.single)} pen=${pen(S.single)}）`);
  line(st(S.double) === 2 && pen(S.double) > 0,
    `B-double 恰兩分離聚簇（strokes=${st(S.double)} 簇=${JSON.stringify(S.double && S.double.clusters)}）`);
  line(st(S.thick) === 1 && pen(S.single) > 0 && pen(S.thick) >= pen(S.single) * THICK_RATIO_MIN,
    `B-thick 聚簇厚 ≥ single×${THICK_RATIO_MIN}（thick=${pen(S.thick)} single=${pen(S.single)}）`);
  line(noStroke(S.dotted) && dashGaps(S.dotted) >= DOTTED_GAPS_MIN && dashRun(S.dotted) <= 6,
    `B-dotted 非實線＋點級離散（gaps=${dashGaps(S.dotted)} ≥ ${DOTTED_GAPS_MIN}，maxRun=${dashRun(S.dotted)}≤6）`);
  line(noStroke(S.dashed) && dashGaps(S.dashed) >= DASHED_GAPS_MIN &&
    dashRun(S.dashed) >= dashRun(S.dotted) * 2,
    `B-dashed 非實線＋段級離散＋段長序（gaps=${dashGaps(S.dashed)}，maxRun=${dashRun(S.dashed)} ≥ dotted ${dashRun(S.dotted)}×2）`);
  line(dashGaps(S.dotted) >= dashGaps(S.dashed) + GAP_ORDER_MIN,
    `B-段數序：dotted gaps − dashed gaps ≥ ${GAP_ORDER_MIN}（${dashGaps(S.dotted)}−${dashGaps(S.dashed)}）`);
  line(S.offFlagOn && S.offFlagOn.rendered && noStroke(S.offFlagOn) &&
    ink(S.offFlagOn) === ink(S.blankFlag0) &&
    JSON.stringify(S.offFlagOn.clusters) === JSON.stringify(S.blankFlag0.clusters) &&
    S.offFlagOn.newErrors.length === 0,
    `B-off flags&4 點亮但 underline='off' → 與同文字零底線態逐像素全等（strokes=${st(S.offFlagOn)} ink=${ink(S.offFlagOn)}==blank ${ink(S.blankFlag0)}；修前只唸旗標在此必殘留）`);
  line(st(S.rawWavy) === 1 && S.rawWavy.newErrors.length === 0 &&
    JSON.stringify(S.rawWavy.underlineField) === JSON.stringify({ raw: 'wavy' }),
    `B-raw{wavy} 降級恰一實線聚簇＋零例外（strokes=${st(S.rawWavy)}, errors=${S.rawWavy && S.rawWavy.newErrors.length}）`);
  line(st(S.unknownLiteral) === 1 && S.unknownLiteral.newErrors.length === 0,
    `B-未知字面 'dashLongHeavy' 降級單線不拋錯（strokes=${st(S.unknownLiteral)}）`);
  line(st(S.omitField) === 1 && JSON.stringify(S.omitField.clusters) === JSON.stringify(S.single.clusters),
    `B-欄位未宣告＋旗標點亮 → 單線兜底且與 single 聚簇全等（serde default 零位移）`);
  // C 層：讀取端保形 Raw 真檔閉環（引擎發射 {raw:'wavy'} → canvas 降級單線）
  line(E.rawWavyFile && !E.rawWavyFile.error && st(E.rawWavyFile) === 1 &&
    JSON.stringify(v.notes.rawFileField) === JSON.stringify({ raw: 'wavy' }) &&
    (E.rawWavyFile.newErrors || []).length === 0,
    `C-真檔 w:u w:val="wavy" → 發射保形 {raw:'wavy'}（flags=${v.notes.rawFileFlags}）→ canvas 降級單線零例外（strokes=${st(E.rawWavyFile)}）`);
  // D) 雙端同步鎖
  const g = v.notes.geomSource || {};
  line(g.double === GEOM_SOURCE.double && g.thick === GEOM_SOURCE.thick &&
    g.dotted === GEOM_SOURCE.dotted && g.dashed === GEOM_SOURCE.dashed && !!g.single,
    `D-渲染器常數逐字＝SVG uline_geom 取樣（double=${g.double ? 'ok' : 'MISS'} thick=${g.thick ? 'ok' : 'MISS'} dotted=${g.dotted ? 'ok' : 'MISS'} dashed=${g.dashed ? 'ok' : 'MISS'} single 在庫=${g.single ? 'ok' : 'MISS'}）`);
  line(v.notes.uncaughtTotal === 0, `全程零未捕獲例外／零 console.error（實得 ${v.notes.uncaughtTotal}：${JSON.stringify(v.notes.errors)}）`);

  if (BUGGY) console.log('（回塞態：形態盞按口徑應紅——紅數即鑑別力，收案以 u4g_underline_canvas_buggy_replay.sh 判定）');
  console.log(`\n判定：${fail === 0 ? 'PASS' : `FAIL（${fail} 項）`}`);
  process.exit(fail === 0 ? 0 : 1);
}

main().catch((e) => { console.error('探針失敗:', e.message); process.exit(2); });
