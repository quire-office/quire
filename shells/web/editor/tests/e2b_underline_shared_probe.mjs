// E-2b 共型收尾探針：底線 serde 線契約遷移的真 wasm 閉環（node，零瀏覽器變因）。
// 位置：shells/web/editor/tests/（U4b 同款正式探針位，非暫存檔）；repo 根向上四層。
//
// 驗證四事（考卷 e2b_underline_shared_type 的殼側鏡像，引擎已鎖的这里只驗通路）：
//   P1 canonical 小寫寫入真通（thick 共型紅利逐字查 XML）；
//   P2 舊大寫態雙讀不炸（遷移期：舊殼 pkg 未同步重建的實際 payload）；
//   P3 Raw（wavy）寫入端誠實拒絕（引擎打臉訊息老實冒出，非静默降級）；
//   P4 切換語意走 canonical（formatUnderline 未帶參數的 next 值是小寫）。
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import zlib from 'node:zlib';
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const ADAPTER = path.join(ROOT, 'shells/web/editor/js/engine-adapter.js');
const { EngineAdapter, normalizeUnderlineKind } = await import(ADAPTER);

let fails = 0;
const check = (name, cond, extra = '') => {
  console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${extra ? '  ' + extra : ''}`);
  if (!cond) fails += 1;
};

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

const range = { start: { para: 1, cp: 0 }, end: { para: 1, cp: 4 } };

// P1 canonical 小寫寫入真通（thick 是共型紅利：鏡像枚舉時代無此變體）。
const a = new EngineAdapter();
await a.init();
a.openDefaultSample();
const r1 = a.formatTextRange(range, { underline: 'thick' });
check('P1a formatTextRange({underline:\'thick\'}) 受理', r1.ok === true, JSON.stringify(r1).slice(0, 160));
const xml1 = documentXml(a);
check('P1b 存檔件寫出 <w:u w:val="thick"/>', xml1.includes('<w:u w:val="thick"/>'));
const r1b = a.formatTextRange(range, { underline: 'dashed' });
check('P1c canonical dashed 受理', r1b.ok === true);
check('P1d 存檔件寫出 <w:u w:val="dashed"/>', documentXml(a).includes('<w:u w:val="dashed"/>'));

// P2 舊大寫態雙讀：旧殼 pkg（U4b 版 send 'Single'/'None'）對新引擎不得炸。
const b = new EngineAdapter();
await b.init();
b.openDefaultSample();
const r2 = b.formatTextRange(range, { underline: 'Single' });
check('P2a 舊大寫 \'Single\' 雙讀受理', r2.ok === true, JSON.stringify(r2).slice(0, 160));
check('P2b 舊大寫寫出檔契約 single', documentXml(b).includes('<w:u w:val="single"/>'));
const r2b = b.formatTextRange(range, { underline: 'None' });
check('P2c 舊大寫 \'None\'（移除值）雙讀受理', r2b.ok === true);
check('P2d 移除值寫出檔契約 none', documentXml(b).includes('<w:u w:val="none"/>'));

// P3 Raw 誠實拒絕：外殼正規化先擋（wavy 不入值域），繞過正規化直接送引擎 payload
//    也必須被引擎打臉（雙層防線，外殼不得代收）。
const r3 = a.formatUnderline('wavy');
check('P3a 外殼拒絕 wavy', r3.ok === false && String(r3.error).includes('不支援'), String(r3.error).slice(0, 120));

// P4 normalizeUnderlineKind 值域：canonical 輸出＋舊名/別名/移除語意全集。
const norm = normalizeUnderlineKind;
check('P4a thick 收（共型紅利）', norm('thick') === 'thick');
check('P4b 舊大寫 DOTTED 歸一 small', norm('DOTTED') === 'dotted');
check('P4c dash 別名歸一 dashed', norm('dash') === 'dashed');
check('P4d false→off', norm(false) === 'off');
check('P4e none/off/0 皆 off', norm('none') === 'off' && norm('off') === 'off' && norm('0') === 'off');
check('P4f wavy 拒', norm('wavy') === null);
check('P4g UNDERLINE_KINDS 含 thick', (await import(ADAPTER)).UNDERLINE_KINDS.includes('thick'));

// P5 切換語意：未帶參數的 formatUnderline() 走 canonical（小寫 next 值）。
const c = new EngineAdapter();
await c.init();
c.openDefaultSample();
const on = c.formatUnderline();
check('P5a 切換開底線 ok', on.ok === true);
check('P5b 切換後檔契約 single', documentXml(c).includes('<w:u w:val="single"/>'));
const off = c.formatUnderline();
check('P5c 再切換關底線 ok', off.ok === true);
check('P5d 切換後檔契約 none', documentXml(c).includes('<w:u w:val="none"/>'));

console.log(fails === 0 ? 'E2B_PROBE_OK 全過' : `E2B_PROBE_FAIL ${fails} 項`);
process.exit(fails === 0 ? 0 : 1);
