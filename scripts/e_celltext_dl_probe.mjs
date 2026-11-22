#!/usr/bin/env node
/**
 * e_celltext_dl_probe.mjs
 * E 卡（t_c2bbcb38）紅燈考卷：表格儲存格文字——打字進了引擎、畫面看不見。
 *
 * 來源：U4d（t_2d7a308a）交付報告 §5 附註 1 上報、PM 於 merged main（4c0d532）
 * 用真引擎獨立復測坐實的引擎缺口。本腳本把 PM 探針口逕固化進 repo。
 *
 * 先紅後綠紀律：修複前【燈 2 必紅】（DL `Text` 零命中）、修複後必綠。
 *
 * 誠實紀律（比照 u4d_selfcheck）：斷言只讀兩樣東西——
 *   1. 引擎回讀真值（paragraph_text / tables_summary_json）
 *   2. 顯示列表（page_display_list 的 DrawItem::Text）
 * 禁讀任何自報旗標（不讀 ok 之外的旗標、不讀 pending:* 標籤、不讀註解宣稱）。
 *
 * 鑑別力：`E_CELLDL_BUGGY=1` 把燈 2 的命中判準換成「永不命中」的假常數 ⇒
 * 燈 2 必紅，證明斷言真在比對顯示列表內容而非恆真。
 *
 * 繁中註解，禁簡體禁 emoji，署名 ADT109119。
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '..');
const ADAPTER_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/engine-adapter.js');
const BUGGY = process.env.E_CELLDL_BUGGY === '1';

const { EngineAdapter, getDefaultSampleBytes } = await import(ADAPTER_JS_PATH);

let pass = true;
const errors = [];
const rawLog = console.log.bind(console);
function assert(cond, msg) { if (!cond) { errors.push(msg); pass = false; } }

async function fresh() {
  const a = new EngineAdapter();
  await a.init();
  a.openFile(getDefaultSampleBytes(), 'sample.docx');
  return a;
}
/** 全頁顯示列表的 Text 項（y, x, text）——繪製序不保證，一律自取不分頁序。 */
function dlTexts(a) {
  const out = [];
  for (let p = 0; p < a.pageCount(); p++) {
    for (const it of a.getPageDisplayList(p).items) {
      if (it.Text) out.push({ page: p, y: it.Text.y, x: it.Text.x, text: it.Text.text });
    }
  }
  return out;
}
const hit = (arr, needle) => arr.some((t) => t.text.includes(needle));

console.log('='.repeat(72));
console.log(`E 卡紅燈探針：表格儲存格文字顯示列表發射（${BUGGY ? 'BUGGY 鑑別態' : '真樹態'}）`);
console.log('='.repeat(72));

// ─────────────────────────────────────────────────────────────────────────────
// 燈 1｜對照組：正文段打字 → 引擎回讀有字 且 DL Text 命中（前提鎖，必須綠）
// ─────────────────────────────────────────────────────────────────────────────
{
  const a = await fresh();
  const base = dlTexts(a);
  const typed = a.typeAt(0, 0, '甲');
  assert(typed.ok === true, `燈 1 前提：正文段打字應受理（${JSON.stringify(typed.error)}）`);
  const back = a.paragraphText(0) || '';
  assert(back.includes('甲'), `燈 1：引擎回讀段落應含「甲」（實得 ${JSON.stringify(back.slice(0, 30))}）`);
  assert(hit(dlTexts(a), '甲'),
    `燈 1：正文段打的「甲」必須出現在顯示列表 Text 項（對照組前提，DL 文字項 ${dlTexts(a).length} 條零命中）`);
  rawLog(`  燈 1 對照組：引擎回讀有字＋DL 命中（DL Text ${base.length} → ${dlTexts(a).length} 條）`);
}

// ─────────────────────────────────────────────────────────────────────────────
// 燈 2｜實驗組：末段後插 2x2 表 → 游標進表格段落打字 → 引擎回讀有字 且 DL 命中
//        （PM 實錄：打字 ok=true、paragraph_text 有字、DL 全頁 Text 零命中）
// ─────────────────────────────────────────────────────────────────────────────
{
  const a = await fresh();
  const last = a.paragraphCount() - 1;
  const ins = a.insertTable(2, 2, { afterPara: last });
  assert(ins.ok === true, `燈 2 前提：末段後插 2x2 表應受理（${JSON.stringify(ins.error)}）`);

  const sum = JSON.parse(a.engine.tables_summary_json()).at(-1);
  assert(Number.isInteger(sum?.para_start), `燈 2 前提：summary.para_start 為整數（實得 ${JSON.stringify(sum)}）`);

  const typed = a.typeAt(sum.para_start, 0, '己');
  assert(typed.ok === true, `燈 2 前提：表內打字應受理（${JSON.stringify({ ok: typed.ok, error: typed.error })}）`);

  // 引擎回讀真值（這一側 PM 實錄為「有字」——它是缺口成立的前提，不是斷言目標）
  const back = a.paragraphText(sum.para_start) || '';
  assert(back.includes('己'),
    `燈 2 前提：引擎回讀表格段落應含「己」（實得 ${JSON.stringify(back.slice(0, 30))}）`);

  // 斷言目標＝顯示列表發射（缺口的定義就在這一側）
  const texts = dlTexts(a);
  const otherHits = texts.filter((t) => !t.text.includes('己')).length;
  // BUGGY 鑑別態：把判準換成永不命中的假字，證明本燈真在比對 DL 內容
  const needle = BUGGY ? '己\u{FFFF}BUGGY' : '己';
  assert(hit(texts, needle),
    `燈 2（缺口本體）：表格儲存格打的「己」寫進 OOXML 與 canonical model（引擎回讀實得 ${JSON.stringify(back.slice(0, 20))}）、` +
    `打字回包 ok=true，但全頁顯示列表 Text 項零命中（DL 其他文字項 ${otherHits} 條俱在）＝畫面完全看不見，用戶端為 S1「打字沒反應」`);
  rawLog(`  燈 2 實驗組：引擎回讀「${back.slice(0, 8)}」／DL Text ${texts.length} 條／命中 ${hit(texts, needle)}`);
}

// ─────────────────────────────────────────────────────────────────────────────
// 燈 3｜資料面與發射面分離（旁證鎖，防止後續卡誤朝寫入層修）：
//        同一條打字操作，引擎回讀（canonical model／OOXML 側）在修復**前後**都
//        必須有字——這是「缺口純在發射面」的結構性證據。若哪天連回讀都沒字，
//        那就是寫入層回歸，屬另一張卡，本燈會先紅並指出是回讀側失守。
//        讀的是引擎回讀真值，不比 ZIP 明文（XML 經 deflate，掃明文會假陰性）。
// ─────────────────────────────────────────────────────────────────────────────
{
  const a = await fresh();
  const last = a.paragraphCount() - 1;
  a.insertTable(2, 2, { afterPara: last });
  const sum = JSON.parse(a.engine.tables_summary_json()).at(-1);
  a.typeAt(sum.para_start, 0, '己');
  const back = a.paragraphText(sum.para_start) || '';
  assert(back.includes('己'),
    `燈 3：引擎回讀（資料面）必須含「己」——回讀側失守＝寫入層回歸，另卡處理（實得 ${JSON.stringify(back.slice(0, 30))}）`);
  // 存檔重開後 DL 一直都有字（PM 實錄），證明發射器本身無恙、缺口只在記憶體態快照。
  const reopened = await fresh();
  reopened.openFile(a.save(), 'roundtrip.docx');
  const rsum = JSON.parse(reopened.engine.tables_summary_json()).at(-1);
  assert(hit(dlTexts(reopened), '己'),
    `燈 3：存檔重開後的 DL 必須含「己」（發射器無恙的旁證；重開表 para_start=${rsum?.para_start}）`);
  rawLog('  燈 3 旁證：資料面回讀有字＋存檔重開 DL 有字 ⇒ 缺口純在記憶體態發射側');
}

console.log('\n' + '='.repeat(72));
if (pass) {
  console.log(BUGGY
    ? 'BUGGY 態預期必紅：本態未紅＝鑑別失效'
    : '自檢結果：PASS — 表格儲存格文字顯示列表發射通過');
  process.exit(BUGGY ? 1 : 0);
}
console.log(`自檢結果：FAIL（${errors.length} 項違規）`);
for (const e of errors) console.log(`  ✗ ${e}`);
process.exit(1);
