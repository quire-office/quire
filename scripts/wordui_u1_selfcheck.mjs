#!/usr/bin/env node
/**
 * wordui_u1_selfcheck.mjs
 * Quire Word UI (U1 窗體骨架) 自檢驗證腳本
 * 
 * 驗收標準：
 * 1. 掃描 index.html 中所有按鈕的 data-requirement 完整性與格式合法性
 * 2. 檢驗 8 大標籤與 34 個指定群組的覆蓋率
 * 3. 檢驗無外部 CDN、字型或網路依賴（零網路、純本地）
 * 4. 無 DOM 探針驗證：純 Node 環境直接載入 qword-core.js 測試 onCommand 與狀態機
 * 5. 印出各群組與依賴卡統計並輸出 PASS / FAIL
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

const HTML_PATH = path.join(ROOT_DIR, 'shells/web/editor/index.html');
const CSS_PATH = path.join(ROOT_DIR, 'shells/web/editor/css/style.css');
const CORE_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/qword-core.js');
const UI_JS_PATH = path.join(ROOT_DIR, 'shells/web/editor/js/qword-ui.js');

let pass = true;
const errors = [];
const warnings = [];

function assert(condition, message) {
  if (!condition) {
    errors.push(message);
    pass = false;
  }
}

console.log('='.repeat(70));
console.log(' Quire Word UI 外殼原型 (U1) 自動驗收與完整性檢查');
console.log('='.repeat(70));

// 1. 檔案存在性檢查
console.log('\n[檢查 1] 驗證必要外殼檔案存在性...');
const requiredFiles = [
  { path: HTML_PATH, desc: '主頁面 HTML (shells/web/editor/index.html)' },
  { path: CSS_PATH, desc: '樣式表 CSS (shells/web/editor/css/style.css)' },
  { path: CORE_JS_PATH, desc: '無 DOM 核心邏輯層 (shells/web/editor/js/qword-core.js)' },
  { path: UI_JS_PATH, desc: 'UI 綁定層 (shells/web/editor/js/qword-ui.js)' }
];

for (const file of requiredFiles) {
  const exists = fs.existsSync(file.path);
  assert(exists, `缺少關鍵檔案: ${file.desc}`);
  console.log(`  ${exists ? '✓' : '✗'} ${file.desc}`);
}

if (!pass) {
  console.error('\n自檢失敗：缺少關鍵檔案，停止後續檢查。');
  console.error(errors.join('\n'));
  process.exit(1);
}

// 2. 檢驗零外部網路依賴
console.log('\n[檢查 2] 驗證零外部網路與零外部字型依賴...');
const htmlContent = fs.readFileSync(HTML_PATH, 'utf-8');
const cssContent = fs.readFileSync(CSS_PATH, 'utf-8');

const networkPatterns = [
  /https?:\/\//i,
  /\/\//i,
  /@import\s+url/i,
  /<link[^>]+href=["'](http|\/\/)/i,
  /<script[^>]+src=["'](http|\/\/)/i
];

let foundNetworkInHtml = false;
for (const line of htmlContent.split('\n')) {
  if (/xmlns="http:\/\/www\.w3\.org\/2000\/svg"/.test(line)) continue; // 標準 SVG namespace 豁免
  for (const pat of networkPatterns) {
    if (pat.test(line)) {
      foundNetworkInHtml = true;
      errors.push(`HTML 含有外部網路連結: ${line.trim()}`);
      pass = false;
    }
  }
}
assert(!foundNetworkInHtml, 'HTML 包含外部網路依賴');
console.log(`  ${foundNetworkInHtml ? '✗' : '✓'} HTML 零外部網路依賴`);

let foundNetworkInCss = false;
for (const line of cssContent.split('\n')) {
  for (const pat of networkPatterns) {
    if (pat.test(line)) {
      foundNetworkInCss = true;
      errors.push(`CSS 含有外部網路連結: ${line.trim()}`);
      pass = false;
    }
  }
}
assert(!foundNetworkInCss, 'CSS 包含外部網路依賴');
console.log(`  ${foundNetworkInCss ? '✗' : '✓'} CSS 零外部網路依賴`);

// 3. HTML 解析與按鈕 data-requirement 審查
console.log('\n[檢查 3] 掃描所有按鈕之 data-requirement 與 aria-label 完整性...');

// 簡易 HTML 標籤掃描
const buttonRegex = /<button\b([^>]*)>([\s\S]*?)<\/button>/gi;
let match;
let totalButtons = 0;
let liveButtons = 0;
const pendingButtonsByCard = {};
const buttonsWithoutReq = [];
const buttonsWithoutAria = [];

while ((match = buttonRegex.exec(htmlContent)) !== null) {
  totalButtons++;
  const attrs = match[1];
  
  // 檢查 data-requirement
  const reqMatch = attrs.match(/data-requirement=["']([^"']+)["']/i);
  if (!reqMatch) {
    // 檢查是否為對話框確定/關閉按鈕或 modal 內部非指令按鈕
    if (!/onclick|dismiss/i.test(attrs)) {
      buttonsWithoutReq.push(match[0].slice(0, 80));
    }
  } else {
    const req = reqMatch[1];
    if (req === 'live') {
      liveButtons++;
    } else if (req.startsWith('pending:')) {
      const card = req.replace('pending:', '');
      pendingButtonsByCard[card] = (pendingButtonsByCard[card] || 0) + 1;
    } else {
      errors.push(`不合法的 data-requirement 格式: "${req}" 在按鈕: ${match[0].slice(0, 80)}`);
      pass = false;
    }
  }

  // 檢查 aria-label
  const ariaMatch = attrs.match(/aria-label=["']([^"']+)["']/i);
  if (!ariaMatch && !/aria-hidden="true"/i.test(attrs)) {
    // 允許有文字子節點的按鈕（如關閉鍵或純文字鍵）
    const innerText = match[2].replace(/<[^>]+>/g, '').trim();
    if (!innerText) {
      buttonsWithoutAria.push(match[0].slice(0, 80));
    }
  }
}

console.log(`  - 掃描按鈕總數: ${totalButtons}`);
console.log(`  - 即刻可用 (live): ${liveButtons}`);
console.log(`  - 停用待實現 (pending): ${Object.values(pendingButtonsByCard).reduce((a, b) => a + b, 0)}`);
for (const [card, count] of Object.entries(pendingButtonsByCard)) {
  console.log(`      * pending:${card}: ${count} 個`);
}

if (buttonsWithoutReq.length > 0) {
  errors.push(`發現 ${buttonsWithoutReq.length} 個按鈕缺乏 data-requirement 屬性！紅線違規：\n` + buttonsWithoutReq.join('\n'));
  pass = false;
} else {
  console.log(`  ✓ 100% 按鈕皆具備誠實 data-requirement 屬性標記`);
}

if (buttonsWithoutAria.length > 0) {
  warnings.push(`發現 ${buttonsWithoutAria.length} 個按鈕無 aria-label 亦無可讀純文字內容`);
} else {
  console.log(`  ✓ 按鈕無障礙可存取性 (aria-label/文字) 良好`);
}

// 4. 八大標籤與指定群組覆蓋率
console.log('\n[檢查 4] 驗證 8 大標籤與指定群組結構完整性...');

const EXPECTED_TABS = [
  { id: 'tab-home', label: '常用', groups: ['clipboard', 'font', 'paragraph', 'styles', 'editing'] },
  { id: 'tab-insert', label: '插入', groups: ['pages', 'tables', 'illustrations', 'symbols', 'header_footer', 'text', 'structure'] },
  { id: 'tab-design', label: '設計', groups: ['themes', 'doc_formatting', 'page_background', 'page_layout_options'] },
  { id: 'tab-layout', label: '版面配置', groups: ['page_setup', 'arrange'] },
  { id: 'tab-references', label: '參考資料', groups: ['toc', 'footnotes', 'citations', 'captions', 'index', 'authorities'] },
  { id: 'tab-review', label: '校閱', groups: ['proofing', 'language', 'comments', 'tracking', 'protect'] },
  { id: 'tab-view', label: '檢視', groups: ['views', 'show', 'zoom', 'window', 'macros'] },
  { id: 'tab-help', label: '說明', groups: ['help_group', 'search_group'] }
];

let allTabsPresent = true;
let totalGroupsCount = 0;

for (const tab of EXPECTED_TABS) {
  const tabButtonFound = htmlContent.includes(`data-tab-target="${tab.id}"`);
  const tabPanelFound = htmlContent.includes(`id="${tab.id}"`);
  assert(tabButtonFound, `缺少標籤頁按鈕: ${tab.label} (${tab.id})`);
  assert(tabPanelFound, `缺少標籤面板容器: ${tab.label} (${tab.id})`);
  
  const statusStr = (tabButtonFound && tabPanelFound) ? '✓' : '✗';
  console.log(`  ${statusStr} 標籤: ${tab.label} (${tab.id}) - 群組數: ${tab.groups.length}`);

  for (const grp of tab.groups) {
    totalGroupsCount++;
    const grpFound = htmlContent.includes(`data-group="${grp}"`);
    assert(grpFound, `標籤 [${tab.label}] 缺少群組: ${grp}`);
    if (!grpFound) {
      console.log(`      ✗ 缺少群組: ${grp}`);
    }
  }
}

console.log(`  - 總群組檢查數量: ${totalGroupsCount}`);

// 5. 無 DOM 核心邏輯層單元測試 (Node.js ESM probe)
console.log('\n[檢查 5] 探針驗證：Node.js ESM 直接載入 qword-core.js 測試邏輯層與 onCommand...');

try {
  const { QWordCore, TABS, GROUPS } = await import(CORE_JS_PATH);
  
  assert(typeof QWordCore === 'function', 'QWordCore 類別輸出正確');
  assert(Array.isArray(TABS) && TABS.length === 8, 'TABS 包含完整 8 個標籤定義');
  
  const core = new QWordCore();
  const initialState = core.getState();
  assert(initialState.activeTabId === 'tab-home', '初始標籤為常用 (tab-home)');
  assert(initialState.zoom === 100, '初始縮放比例為 100%');
  assert(initialState.viewMode === 'print', '初始檢視模式為 print');
  assert(initialState.rulerVisible === true, '初始標尺為可見');
  
  // 測試標籤切換
  const switchRes = core.onCommand('tab:switch:tab-insert');
  assert(switchRes.handled === true && core.getState().activeTabId === 'tab-insert', 'tab:switch:tab-insert 成功切換');
  
  // 測試縮放
  core.onCommand('zoom:in');
  assert(core.getState().zoom === 110, 'zoom:in 縮放增加');
  core.onCommand('zoom:out');
  assert(core.getState().zoom === 100, 'zoom:out 縮放減少');
  core.onCommand('zoom:set:150');
  assert(core.getState().zoom === 150, 'zoom:set:150 縮放設定');
  core.onCommand('zoom:reset');
  assert(core.getState().zoom === 100, 'zoom:reset 縮放重設至 100%');
  core.onCommand('zoom:fit-width');
  assert(core.getState().zoom === 125, 'zoom:fit-width 適合頁寬');

  // 測試檢視模式切換
  core.onCommand('view:mode:read');
  assert(core.getState().viewMode === 'read', 'view:mode:read 切換至閱讀模式');
  core.onCommand('view:mode:web');
  assert(core.getState().viewMode === 'web', 'view:mode:web 切換至 Web 模式');
  core.onCommand('view:mode:print');
  assert(core.getState().viewMode === 'print', 'view:mode:print 切換回列印配置');

  // 測試標尺開關與縮排
  core.onCommand('ruler:toggle');
  assert(core.getState().rulerVisible === false, 'ruler:toggle 關閉標尺');
  core.onCommand('ruler:toggle');
  assert(core.getState().rulerVisible === true, 'ruler:toggle 開啟標尺');
  core.setIndent('firstLine', 1.5);
  assert(core.getState().indents.firstLine === 1.5, 'setIndent 首行縮排計算');

  // 測試 pending 命令日誌紀錄
  core.onCommand('clipboard:cut', {}, 'pending:K29a');
  core.onCommand('insert:picture', {}, 'pending:K29b');
  const log = core.getLog();
  assert(log.length >= 10, 'commandLog 完整記錄指令流');
  const lastLog = log[log.length - 1];
  assert(lastLog.cmdId === 'insert:picture' && lastLog.requirement === 'pending:K29b', 'pending 指令正確保留 requirement 標記');

  console.log(`  ✓ QWordCore 無 DOM 模組在 Node.js 中載入並通過所有指令探針測試`);
} catch (err) {
  errors.push(`QWordCore ESM 載入或執行發生例外: ${err.message}`);
  pass = false;
}

// 6. 總結報告與 PASS / FAIL 判定
console.log('\n' + '='.repeat(70));
if (warnings.length > 0) {
  console.log('注意事項 / 警告:');
  warnings.forEach(w => console.log('  [WARN] ' + w));
  console.log('-'.repeat(70));
}

if (pass) {
  console.log(' 自檢結果: PASS');
  console.log(' 恭喜！Quire Word 外觀編輯器 (U1 窗體骨架) 通過所有規格與紅線檢查。');
  console.log('='.repeat(70));
  process.exit(0);
} else {
  console.error(' 自檢結果: FAIL');
  console.error(' 發現以下不符合規格問題：');
  errors.forEach(e => console.error('  [ERROR] ' + e));
  console.error('='.repeat(70));
  process.exit(1);
}
