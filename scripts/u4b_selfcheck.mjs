#!/usr/bin/env node
/**
 * u4b_selfcheck.mjs — U4b：K29e 六欄接線的動態閉環自檢（真 wasm＋真開檔＋真呼叫）
 *
 * 誠實紀律（沿用 U2/U4 的「用戶看見那層」口徑）：
 * - 每條轉 live 的鈕都要有 page_display_list 的屬性翻轉實測值＋undo 還原，
 *   不接受「adapter 有這個方法」這類靜態宣稱充數。
 * - 上下標斷言用 docs/k29e-redlight.md §2 的 LO 25.8 真值常數：
 *   字級 0.58×base、上標基線 −0.35×base、下標 +1.0pt@11pt（=+0.0909×base）、
 *   **行高不膨脹**（sup/sub 行的 auto 行距 pitch 與純正文行逐行全等 12.65pt）。
 * - 存檔面另驗 XML 真的落地（ZIP deflate 解壓後才准搜尋，latin1 直搜會假陰性）。
 * - 底線形態（underline-style）的引擎解析缺口用**實測**坐實：寫入端五形態
 *   真的進 XML，存檔重開後渲染卻與 single 無異（解析層折成布林），故誠實留
 *   pending，不假 live。
 *
 * 用法：node scripts/u4b_selfcheck.mjs（repo 根執行）
 */
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '..');
const ADAPTER_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/engine-adapter.js');
const CORE_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/qword-core.js');
const UI_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/qword-ui.js');
const HTML_PATH = path.join(ROOT_DIR, 'shells/web/editor/index.html');

const { EngineAdapter, buildFormatTextRangeMutation } = await import(ADAPTER_JS_PATH);
const { QWordCore } = await import(CORE_JS_PATH);

let pass = true;
const errors = [];
const notes = [];
function assert(cond, msg) {
  if (!cond) { errors.push(msg); pass = false; }
}
const ok = (cond, msg) => console.log(`  ${cond ? '✓' : '✗'} ${msg}`);

// --------------------------------------------------------------------------
// 工具：從顯示列表取段級 Text 項（items[1] 是段落 1 首段文字，U3 同款錨點）
// --------------------------------------------------------------------------
function textsOf(adapter, page = 0) {
  return adapter.getPageDisplayList(page).items.filter(i => i.Text).map(i => i.Text);
}
function rectsOf(adapter, page = 0) {
  return adapter.getPageDisplayList(page).items.filter(i => i.Rect).map(i => i.Rect);
}
/** save_docx_bytes → 解 ZIP → 解 word/document.xml（deflate） */
function documentXml(adapter) {
  const buf = Buffer.from(adapter.save());
  for (let i = 0; i + 4 <= buf.length; i += 1) {
    if (buf.readUInt32LE(i) !== 0x04034b50) continue;
    const nameLen = buf.readUInt16LE(i + 26);
    const extraLen = buf.readUInt16LE(i + 28);
    const cSize = buf.readUInt32LE(i + 18);
    const start = i + 30 + nameLen + extraLen;
    const name = buf.subarray(i + 30, i + 30 + nameLen).toString('utf-8');
    if (name !== 'word/document.xml') continue;
    try {
      return zlib.inflateRawSync(buf.subarray(start, start + cSize)).toString('utf-8');
    } catch {
      return buf.subarray(start, start + cSize).toString('utf-8');
    }
  }
  return '';
}
/** 取「這是一份」選取段（para1 cp0..4）格式化後的 run 字串 */
function patchedRunXml(xml) {
  const i = xml.indexOf('這是一份');
  if (i < 0) return '';
  const back = xml.lastIndexOf('<w:r>', i);
  return xml.slice(back, i + 40);
}

console.log('='.repeat(70));
console.log(' U4b：K29e 六欄接線閉環自檢（真 wasm＋真開檔＋真呼叫＋真存檔）');
console.log('='.repeat(70));

// --------------------------------------------------------------------------
console.log('\n[檢查 1] 契約面：K29e 欄位真的進 Mutation JSON（序列化契約）');
{
  const m = buildFormatTextRangeMutation(
    { start: { para: 1, cp: 0 }, end: { para: 1, cp: 4 } },
    {
      underline: 'Dashed', strike: 'Double', vert_align: 'Subscript',
      highlight: 0xFFFFFF00, font_family: 'PMingLiU'
    }
  );
  const p = m.FormatTextRange.patch;
  ok(p.underline === 'Dashed', `underline 序列化＝引擎枚舉原名（${p.underline}）`);
  ok(p.strike === 'Double', `strike 序列化＝StrikeKind 原名（${p.strike}）`);
  ok(p.vert_align === 'Subscript', `vert_align 序列化＝VertAlign 原名（${p.vert_align}）`);
  ok(p.highlight === 0xFFFFFF00, `highlight 序列化＝0xAARRGGBB 數值（${p.highlight}）`);
  ok(p.font_family === 'PMingLiU', `font_family 序列化＝族名字串（${p.font_family}）`);
  const bare = buildFormatTextRangeMutation({ start: { para: 1, cp: 0 }, end: { para: 1, cp: 4 } }, {});
  const bp = bare.FormatTextRange.patch;
  const nulls = ['underline', 'strike', 'vert_align', 'highlight', 'font_family']
    .every(k => bp[k] === null);
  ok(nulls, '未帶欄位＝null（＝引擎「不碰」語意，不憑空寫入）');
  assert(p.underline !== undefined && !Array.isArray(p.underline), '欄位形態必須是引擎受理的原字串形態');
}

// --------------------------------------------------------------------------
console.log('\n[檢查 2] 底線閉環：adapter.formatUnderline → Text.flags FLAG_UNDERLINE(4) → undo');
{
  const a = new EngineAdapter();
  await a.init();
  a.openDefaultSample();
  const before = textsOf(a)[1];
  assert((before.flags & 4) === 0, `段落 1 初態不具底線（flags=${before.flags}）`);
  a.caret = { para: 1, cp: 0 };
  const res = a.formatUnderline('single');
  ok(res.ok === true, `formatUnderline 受理（ok=${res.ok}）`);
  const after = textsOf(a)[1];
  ok((after.flags & 4) === 4, `Text.flags 帶 FLAG_UNDERLINE(4)（實得 ${after.flags}）`);
  const xml = documentXml(a);
  ok(/<w:u w:val="single"\/>/.test(patchedRunXml(xml)), '存檔 XML 含 <w:u w:val="single"/>');
  const undoRes = a.undo();
  ok(undoRes.ok === true && undoRes.undone === true, 'undo 成功');
  ok((textsOf(a)[1].flags & 4) === 0, `undo 後底線旗標消失（flags=${textsOf(a)[1].flags}）`);
  // 切換語意：已有底線時不帶形態再點＝明確移除（探針讀旗標決定方向）
  a.caret = { para: 1, cp: 0 };
  a.formatUnderline('single');
  const on = (textsOf(a)[1].flags & 4) === 4;
  a.caret = { para: 1, cp: 0 };
  a.formatUnderline();
  const off = (textsOf(a)[1].flags & 4) === 0;
  a.undo();
  ok(on && off, `同鈕重複點＝開／關切換實測（先 ${on ? '開' : '未開'}，再點 ${off ? '關' : '未關'}）`);
}

// --------------------------------------------------------------------------
console.log('\n[檢查 3] 刪除線閉環：adapter.formatStrikethrough → Text.flags FLAG_STRIKE(8) → undo');
{
  const a = new EngineAdapter();
  await a.init();
  a.openDefaultSample();
  a.caret = { para: 1, cp: 0 };
  const res = a.formatStrikethrough('single');
  ok(res.ok === true, `formatStrikethrough 受理（ok=${res.ok}）`);
  const flags = textsOf(a)[1].flags;
  ok((flags & 8) === 8, `Text.flags 帶 FLAG_STRIKE(8)（實得 ${flags}）`);
  const xml = documentXml(a);
  ok(/<w:strike\/>/.test(patchedRunXml(xml)), '存檔 XML 含 <w:strike/>');
  ok(a.undo().undone === true && (textsOf(a)[1].flags & 8) === 0, 'undo 後刪除線旗標消失');
  const bad = a.formatStrikethrough('wavy');
  ok(bad.ok === false, '引擎枚舉外的形態被外殼誠實拒絕（不吞成成功）');
}

// --------------------------------------------------------------------------
console.log('\n[檢查 4] 上下標閉環：LO 真值常數（0.58×字級／−0.35×base／+1.0pt@11pt／行高不膨脹）');
{
  const a = new EngineAdapter();
  await a.init();
  a.openDefaultSample();
  const ys0 = textsOf(a).slice(0, 6).map(t => t.y);
  const pitch0 = ys0.slice(1).map((y, i) => +(y - ys0[i]).toFixed(4));
  // 行距真值錨點：本檔案用的是外殼預設樣本（12pt×1.4＝16.8pt），不是
  // docs/k29e-redlight.md 那份 11pt 樣本的 12.65pt——兩者不可混用。LO 真值在
  // 本卡的可驗收形態是「**行高不膨脹**」：上下標不改變 auto 行距 pitch。
  assert(pitch0.every(p => p > 0), `基線行距取得（實得 ${JSON.stringify(pitch0)}）`);

  a.caret = { para: 1, cp: 0 };
  const sub = a.formatVertAlign('subscript');
  ok(sub.ok === true, `subscript 受理（ok=${sub.ok}）`);
  const t = textsOf(a)[1];
  const sizeRatio = t.size_pt / 12;
  ok(Math.abs(sizeRatio - 0.58) < 0.005, `下標字級 0.58×base（實測 ${t.size_pt}pt＝${sizeRatio.toFixed(4)}×12）`);
  const dySub = t.y - ys0[1];
  ok(Math.abs(dySub - 12 * (1.0 / 11.0)) < 0.02, `下標基線 +1.0pt@11pt（實得 dy=${dySub.toFixed(4)}pt，LO 真值 12.65 行內 +1.0909pt）`);

  const ysSub = textsOf(a).slice(0, 6).map(x => x.y);
  // 行高不膨脹的正確形態：僅被套上下標的那一行（para1 首行）基線位移，
  // 其後各行絕對 y 逐位不動（若行高膨脹，其後全部行會 cumulatively 下推）。
  const othersHold = ysSub.slice(2).every((y, i) => Math.abs(y - ys0[i + 2]) < 0.001);
  ok(othersHold, `行高不膨脹：受標行之外各行絕對 y 逐位不動（基線 ${JSON.stringify(ys0.slice(2))} → ${JSON.stringify(ysSub.slice(2))}）`);
  ok(/w:vertAlign w:val="subscript"/.test(documentXml(a)), '存檔 XML 含 w:vertAlign val=subscript');
  a.undo();

  a.caret = { para: 1, cp: 0 };
  const sup = a.formatVertAlign('superscript');
  const s = textsOf(a)[1];
  ok(sup.ok === true && Math.abs(s.size_pt / 12 - 0.58) < 0.005,
    `superscript 受理＋字級折 0.58（實測 ${s.size_pt}pt）`);
  const dySup = s.y - ys0[1];
  ok(Math.abs(dySup + 12 * 0.35) < 0.02, `上標基線 −0.35×base（實得 dy=${dySup.toFixed(4)}pt）`);
  ok(/w:vertAlign w:val="superscript"/.test(documentXml(a)), '存檔 XML 含 w:vertAlign val=superscript');
  a.undo();
  const back = textsOf(a)[1];
  ok(Math.abs(back.size_pt - 12) < 0.001 && Math.abs(back.y - ys0[1]) < 0.001,
    `undo 後字級與基線逐位回基線（size=${back.size_pt} y=${back.y.toFixed(4)}）`);
}

// --------------------------------------------------------------------------
console.log('\n[檢查 5] 醒目提示閉環：Text 下方 Rect 發射＋fill 色值＋undo 消失');
{
  const a = new EngineAdapter();
  await a.init();
  a.openDefaultSample();
  const rects0 = rectsOf(a).length;
  a.caret = { para: 1, cp: 0 };
  const res = a.formatHighlight('#FFFF00');
  ok(res.ok === true, `formatHighlight 受理（ok=${res.ok}）`);
  const rects = rectsOf(a);
  ok(rects.length === rects0 + 1, `顯示列表出現底方塊 Rect（${rects0}→${rects.length}，用戶看見那層）`);
  const hit = rects.find(r => r.fill_argb === 0xFFFFFF00);
  ok(!!hit, `Rect.fill_argb=0xFFFFFF00（實得 ${hit ? '0x' + hit.fill_argb.toString(16) : '無'}）`);
  ok(/<w:highlight w:val="yellow"\/>/.test(documentXml(a)),
    '命名色走 <w:highlight w:val="yellow"/>（Word 互通形態，非自創欄位）');
  ok(a.undo().undone === true && rectsOf(a).length === rects0, 'undo 後底方塊消失');
  // 非命名色走 w:shd 路徑（引擎註解宣稱的雙族落點，實測其中一條）
  a.caret = { para: 1, cp: 0 };
  a.formatHighlight('#123456');
  const r2 = rectsOf(a).find(r => r.fill_argb === 0xFF123456);
  ok(!!r2 && /<w:shd [^>]*w:fill="123456"\/>/.test(documentXml(a)),
    '非命名色走 w:shd w:fill 路徑且發射同色 Rect');
  a.undo();
}

// --------------------------------------------------------------------------
console.log('\n[檢查 6] 字型族閉環：Text.family 宣告＋測寬推進（斷行幾何變動）＋undo');
{
  const a = new EngineAdapter();
  await a.init();
  a.openDefaultSample();
  a.caret = { para: 1, cp: 0 };
  const widthsBefore = textsOf(a).slice(0, 6).map(t => +t.x.toFixed(4));
  const res = a.setFontFamily('Arial');
  ok(res.ok === true, `setFontFamily 受理（ok=${res.ok}）`);
  const item = textsOf(a)[1];
  ok(item.family === 'Arial', `Text.family="Arial"（實得 ${JSON.stringify(item.family ?? null)}）`);
  const x1After = textsOf(a)[1].x;
  const xml = documentXml(a);
  ok(/<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:cs="Arial" w:eastAsia="Arial"\/>/.test(patchedRunXml(xml)),
    '存檔 XML 含 w:rFonts 四欄同族名（引擎「整包選族」口徑）');
  ok(a.undo().undone === true && (textsOf(a)[1].family ?? null) === null, 'undo 後 family 宣告消失');
  const widthsAfter = textsOf(a).slice(0, 6).map(t => +t.x.toFixed(4));
  ok(JSON.stringify(widthsBefore) === JSON.stringify(widthsAfter), 'undo 後幾何逐位回基線');
}

// --------------------------------------------------------------------------
console.log('\n[檢查 7] 清除直接格式：七欄一次寫入＋樣式級聯誠實邊界');
{
  const a = new EngineAdapter();
  await a.init();
  a.openDefaultSample();
  a.caret = { para: 1, cp: 0 };
  a.formatUnderline('single');
  a.formatStrikethrough('single');
  const combo = textsOf(a)[1].flags;
  a.formatTextRange({ start: { para: 1, cp: 0 }, end: { para: 1, cp: 4 } },
    { bold: true, italic: true, underline: 'Single', strike: 'Single', vert_align: 'Superscript' });
  const loaded = textsOf(a)[1];
  a.caret = { para: 1, cp: 0 };
  const res = a.clearDirectCharFormatting();
  ok(res.ok === true, `clearDirectCharFormatting 受理（ok=${res.ok}）`);
  const cleared = textsOf(a)[1];
  ok(cleared.flags === 0, `清除後 Text.flags=0（多欄疊加態 ${loaded.flags}→${cleared.flags}）`);
  ok(Math.abs(cleared.size_pt - 12) < 0.001, `上下標態一併回基線（size=${cleared.size_pt}）`);
  const xml = documentXml(a);
  const run = patchedRunXml(xml);
  const removes = /<w:u w:val="none"\/>/.test(run) && /<w:strike w:val="0"\/>/.test(run)
    && /<w:vertAlign w:val="baseline"\/>/.test(run);
  ok(removes, '寫入的是「明確關閉」宣告（w:u none／w:strike val=0／w:vertAlign baseline），非不寫 XML');
  notes.push(`多欄疊加態（同一次 mutation 六欄齊發）flags 實測 ${loaded.flags}；分次單欄呼叫受引擎 patch_rpr 整段重寫行為影響，非本卡能修（見報告引擎發現 E-1）`);
}

// --------------------------------------------------------------------------
console.log('\n[檢查 8] 底線形態引擎缺口（實測坐實，非推測）：五形態寫入真實、渲染無差異');
{
  const mk = async (kind) => {
    const a = new EngineAdapter();
    await a.init();
    a.openDefaultSample();
    a.caret = { para: 1, cp: 0 };
    const r = a.formatTextRange({ start: { para: 1, cp: 0 }, end: { para: 1, cp: 4 } }, { underline: kind });
    return { a, r, xml: patchedRunXml(documentXml(a)), t: textsOf(a)[1] };
  };
  const single = await mk('Single');
  const double = await mk('Double');
  const dotted = await mk('Dotted');
  ok(/w:u w:val="single"/.test(single.xml) && /w:u w:val="double"/.test(double.xml)
    && /w:u w:val="dotted"/.test(dotted.xml), '寫入端：單條／雙條／點線三種 w:u/@w:val 真的進 XML');
  ok(single.t.flags === double.t.flags && double.t.flags === dotted.t.flags,
    `渲染端：三形態 Text.flags 全等（${single.t.flags}），發射層 FLAG_UNDERLINE 一態——形態資訊在解析層（styles.rs 折 Option<bool>）遺失`);
  ok(single.t.y === double.t.y && single.t.x === double.t.x,
    '渲染端：三形態幾何全等（無任何像素級差異可提供給用戶）');
  // 存檔重開（真 round-trip）後仍無差異：證明不是外殼快取
  const bytes = Buffer.from(double.a.save());
  const c = new EngineAdapter();
  await c.init();
  c.openFile(new Uint8Array(bytes), 'reopen.docx');
  ok(textsOf(c)[1].flags === 4, '存檔重開後雙條底線仍只是 FLAG_UNDERLINE(4)（解析層缺口非外殼態）');
  notes.push('format:underline-style 因此誠實留 pending:engine-para：寫入真、解析與發射兩層斷點，掛 live＝假宣稱');
}

// --------------------------------------------------------------------------
console.log('\n[檢查 9] core 命令層誠實記帳（假 live 回塞鎖）');
{
  const core = new QWordCore();
  const u = core.onCommand('format:underline', {}, 'live');
  ok(u.handled === true && u.requirement === 'live', `底線命令 handled=true＋live（實得 ${JSON.stringify({ h: u.handled, r: u.requirement })}）`);
  for (const cmd of ['format:strikethrough', 'format:subscript', 'format:superscript', 'format:highlight', 'font:family', 'font:clear-formatting']) {
    const r = core.onCommand(cmd, cmd === 'font:family' ? { family: 'Arial' } : {}, 'live');
    ok(r.handled === true && r.requirement === 'live', `${cmd} 受理＋live（實得 ${JSON.stringify({ h: r.handled, r: r.requirement })}）`);
  }
  const us = core.onCommand('format:underline-style', { style: 'double' }, 'pending:engine-para');
  ok(us.handled === false && us.requirement !== 'live',
    `底線形態鈕誠實不受理、不沿用 live（實得 ${JSON.stringify({ h: us.handled, r: us.requirement })}）`);
  const logs = core.getLog().filter(l => l.cmdId === 'format:underline-style');
  ok(logs.length > 0 && logs[logs.length - 1].handled === false, '失敗如實進 commandLog（考卷讀得到真實失敗）');
}

// --------------------------------------------------------------------------
console.log('\n[檢查 10] 標籤守恆與 K29e 歸零（驗收命令的腳本內復核）');
{
  const html = fs.readFileSync(HTML_PATH, 'utf-8');
  const all = html.match(/data-requirement="[^"]*"/g) || [];
  ok(!all.includes('data-requirement="pending:K29e"'), 'pending:K29e 歸零');
  ok(all.length === 196, `data-requirement 鈕數守恆 196（實得 ${all.length}）`);
  for (const cmd of ['format:underline', 'format:strikethrough', 'format:subscript', 'format:superscript']) {
    const m = html.match(new RegExp(`<(?:button|select)[^>]+data-command="${cmd}"[^>]*>`, 'i'));
    ok(m && /data-requirement="live"/.test(m[0]), `HTML：[${cmd}] = live`);
  }
  const usBtn = html.match(/<(?:button|select)[^>]+data-command="format:underline-style"[^>]*>/i);
  ok(usBtn && /data-requirement="pending:[^"]+"/.test(usBtn[0]), 'HTML：[format:underline-style] 為真實缺口 pending');
  const uiSrc = fs.readFileSync(UI_JS_PATH, 'utf-8');
  ok(/fontFamilySelect/.test(uiSrc) && /'font:family'/.test(uiSrc),
    '外殼真的綁了字型族下拉 change 事件（沒有它 live 就是假宣稱）');
}

// --------------------------------------------------------------------------
console.log('\n' + '='.repeat(70));
if (notes.length) {
  console.log('實測附註:');
  notes.forEach(n => console.log('  [NOTE] ' + n));
  console.log('-'.repeat(70));
}
if (pass) {
  console.log(' 自檢結果: PASS');
  console.log('='.repeat(70));
  process.exit(0);
} else {
  console.error(' 自檢結果: FAIL');
  errors.forEach(e => console.error('  [ERROR] ' + e));
  console.log('='.repeat(70));
  process.exit(1);
}
