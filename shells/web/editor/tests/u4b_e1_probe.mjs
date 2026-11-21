// E-1 根因探針：分次單欄呼叫是否互相覆蓋（node＋真 wasm，零瀏覽器變因）
// 位置：shells/web/editor/tests/（U4b 白名單內正式探針，非暫存檔）；
// repo 根需向上四層。
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const ADAPTER = path.join(ROOT, 'shells/web/editor/js/engine-adapter.js');
const { EngineAdapter } = await import(ADAPTER);

const a = new EngineAdapter();
await a.init();
a.openDefaultSample();
const texts = () => a.getPageDisplayList(0).items.filter(i => i.Text).map(i => i.Text);
const range = { start: { para: 1, cp: 0 }, end: { para: 1, cp: 4 } };

console.log('初態 flags =', texts()[1].flags);
const r1 = a.formatTextRange(range, { bold: true });
console.log('呼叫一 bold        ok=' + r1.ok, 'flags =', texts()[1].flags);
const r2 = a.formatTextRange(range, { italic: true });
console.log('呼叫二 italic(同段) ok=' + r2.ok, 'flags =', texts()[1].flags, '（疊加應為 3）');
const r3 = a.formatTextRange(range, { underline: 'Single' });
console.log('呼叫三 underline    ok=' + r3.ok, 'flags =', texts()[1].flags, '（疊加應為 7）');

// XML 真值：粗體宣告還在不在？
import zlib from 'node:zlib';
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
    try { return zlib.inflateRawSync(buf.subarray(start, start + cSize)).toString('utf-8'); }
    catch { return buf.subarray(start, start + cSize).toString('utf-8'); }
  }
  return '';
}
const xml = documentXml(a);
const i = xml.indexOf('這是一份');
const runStart = xml.lastIndexOf('<w:r>', i);
const run = xml.slice(runStart, i + 30);
console.log('--- 受標 run XML ---');
console.log(run);
console.log('XML 含 <w:b/>:', /<w:b\b/.test(run), '| 含 <w:i/>:', /<w:i\b/.test(run), '| 含 <w:u:', /<w:u\b/.test(run));

// 對照：單次呼叫四欄
const b = new EngineAdapter();
await b.init();
b.openDefaultSample();
const textsB = () => b.getPageDisplayList(0).items.filter(i => i.Text).map(i => i.Text);
const s1 = b.formatTextRange(range, { bold: true, italic: true, underline: 'Single', strike: 'Single' });
console.log('單次四欄 ok=' + s1.ok, 'flags =', textsB()[1].flags, '（應為 15）');
