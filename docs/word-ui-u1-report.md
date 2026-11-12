# AGY-WORDUI 任務交付報告：Word 外觀編輯器外殼原型（U1 窗體骨架）

## 0. 工作位置自證與執行規範確認
- 工作目錄 (`pwd`)：`<repo>/.worktrees/agy-wordui`（本機工作樹，不在 repo 內）
- 嚴格遵守檔案白名單操作：
  - `shells/web/editor/**`（全部新建：index.html、css/、js/、README.md、package.json）
  - `scripts/wordui_u1_selfcheck.mjs`（新建：Node 自檢驗收腳本）
  - `docs/word-ui-u1-report.md`（新建：本交付報告）
- 零動用禁改面（`core/**`、`shells/web/src/**`、`shells/web/pkg/**`、`shells/web/demo/**`、`shells/native/**`、`shells/web-collab/**`、根目錄設定檔與 CI）。
- 零外部網路依賴、零第三方圖示/字型庫、純 SVG/CSS 自繪圖示。
- 介面用字遵循繁中文書處理器業界慣用語。

---

## 1. 驗收實際輸出（驗收一：自檢腳本驗證）

執行命令：
```bash
node scripts/wordui_u1_selfcheck.mjs
```

實際輸出：
```
======================================================================
 Quire 編輯器外殼原型 (U1) 自動驗收與完整性檢查
======================================================================

[檢查 1] 驗證必要外殼檔案存在性...
  ✓ 主頁面 HTML (shells/web/editor/index.html)
  ✓ 樣式表 CSS (shells/web/editor/css/style.css)
  ✓ 無 DOM 核心邏輯層 (shells/web/editor/js/qword-core.js)
  ✓ UI 綁定層 (shells/web/editor/js/qword-ui.js)

[檢查 2] 驗證零外部網路與零外部字型依賴...
  ✓ HTML 零外部網路依賴
  ✓ CSS 零外部網路依賴

[檢查 3] 掃描所有按鈕之 data-requirement 與 aria-label 完整性...
  - 掃描按鈕總數: 191
  - 即刻可用 (live): 31
  - 停用待實現 (pending): 160
      * pending:K29f: 3 個
      * pending:K29a: 44 個
      * pending:K29d: 22 個
      * pending:K29u: 4 個
      * pending:K29c: 40 個
      * pending:K29b: 21 個
      * pending:K29e: 21 個
      * pending:K29s: 4 個
      * pending:K29m: 1 個
  ✓ 100% 按鈕皆具備誠實 data-requirement 屬性標記
  ✓ 按鈕無障礙可存取性 (aria-label/文字) 良好

[檢查 4] 驗證 8 大標籤與指定群組結構完整性...
  ✓ 標籤: 常用 (tab-home) - 群組數: 5
  ✓ 標籤: 插入 (tab-insert) - 群組數: 7
  ✓ 標籤: 設計 (tab-design) - 群組數: 4
  ✓ 標籤: 版面配置 (tab-layout) - 群組數: 2
  ✓ 標籤: 參考資料 (tab-references) - 群組數: 6
  ✓ 標籤: 校閱 (tab-review) - 群組數: 5
  ✓ 標籤: 檢視 (tab-view) - 群組數: 5
  ✓ 標籤: 說明 (tab-help) - 群組數: 2
  - 總群組檢查數量: 36

[檢查 5] 探針驗證：Node.js ESM 直接載入 qword-core.js 測試邏輯層與 onCommand...
  ✓ QWordCore 無 DOM 模組在 Node.js 中載入並通過所有指令探針測試

======================================================================
 自檢結果: PASS
 恭喜！Quire 文書編輯器外殼 (U1 窗體骨架) 通過所有規格與紅線檢查。
======================================================================
```

---

## 2. 驗收實際輸出（驗收二：Node.js ESM 純邏輯層與探針測試）

執行命令：
```bash
node -e "
import('./shells/web/editor/js/qword-core.js').then(({ QWordCore }) => {
  const core = new QWordCore();
  console.log('--- 純邏輯層無 DOM 探針輸出 ---');
  console.log('1. 初始標籤:', core.getState().activeTabId);
  core.onCommand('tab:switch:tab-insert');
  console.log('2. 切換標籤至 insert:', core.getState().activeTabId);
  core.onCommand('zoom:set:130');
  console.log('3. 縮放設定為 130%:', core.getState().zoom);
  core.onCommand('ruler:toggle');
  console.log('4. 標尺可見狀態:', core.getState().rulerVisible);
  core.setIndent('firstLine', 2.0);
  console.log('5. 首行縮排位置:', core.getState().indents.firstLine, 'cm');
  core.onCommand('format:bold', {}, 'pending:K29a');
  console.log('6. 歷史指令紀錄數量:', core.getLog().length);
  console.log('7. 最新指令條目內容:', JSON.stringify(core.getLog().at(-1)));
  console.log('--- 探針驗證完成 ---');
});
"
```

實際輸出：
```
--- 純邏輯層無 DOM 探針輸出 ---
1. 初始標籤: tab-home
2. 切換標籤至 insert: tab-insert
3. 縮放設定為 130%: 130
4. 標尺可見狀態: false
5. 首行縮排位置: 2 cm
6. 歷史指令紀錄數量: 4
7. 最新指令條目內容: {"timestamp":1794406953977,"cmdId":"format:bold","requirement":"pending:K29a","payload":{},"handled":false}
--- 探針驗證完成 ---
```

---

## 3. 八大標籤與群組清單逐項核對

### [常用]（tab-home）
- [x] **剪貼簿**（clipboard）：貼上（大字鈕＋下拉）、剪下、複製、格式複製；右下角對話方塊啟動器。
- [x] **字型**（font）：字型名稱下拉（微軟正黑體/新細明體/標楷體/Arial/Calibri/Times New Roman）、字型大小下拉（8..72）、增大/縮小字型、清除格式、粗體(B)、斜體(I)、底線(U)及底線樣式下拉、刪除線(S)、下標、上標、字型色彩（主鈕＋色盤）、文字醒目色（主鈕＋色盤）；右下角對話方塊啟動器。
- [x] **段落**（paragraph）：項目符號（主鈕＋下拉）、編號（主鈕＋下拉）、清單層次、減少縮排、增加縮排、排序、編輯標記顯示(¶)、靠左對齊、置中對齊、靠右對齊、兩端對齊、行距下拉、底紋（主鈕＋色盤）、框線（主鈕＋下拉）；右下角對話方塊啟動器。
- [x] **樣式**（styles）：樣式庫（內文、無間距、標題 1、標題 2、標題 3、其他展開）；右下角對話方塊啟動器。
- [x] **編輯**（editing）：尋找、取代、選取。

### [插入]（tab-insert）
- [x] **頁面**（pages）：封面頁、空白頁、分頁符號。
- [x] **表格**（tables）：表格（8×5 網格選擇器 hover 互動＋下拉）。
- [x] **圖例**（illustrations）：圖片、線上圖片、圖形、圖示、SmartArt、圖表、螢幕擷取。
- [x] **符號**（symbols）：方程式、符號（開啟特殊字元面板含中文字元示例）。
- [x] **頁首與頁尾**（header_footer）：頁首、頁尾、頁碼。
- [x] **文字**（text）：文字方塊、文字方向（橫排/豎排）、文字藝術師、日期及時間、物件、簽名行。
- [x] **結構**（structure）：超連結、書籤、交叉引用、文件部件。

### [設計]（tab-design）
- [x] **主題**（themes）：主題下拉、樣式集。
- [x] **文件格式設定**（doc_formatting）：色彩、字型、效果、段落間距、設為預設值。
- [x] **頁面背景**（page_background）：浮水印、頁面色彩、頁面框線。
- [x] **版面**（page_layout_options）：文字方向、邊界、裝訂線、頁面大小、欄位、分隔符號。

### [版面配置]（tab-layout）
- [x] **頁面設定**（page_setup）：邊界、方向（直向/橫向）、大小（A4/Letter）、欄（單欄/多欄）、分隔符號、行號、連字號；右下角對話方塊啟動器。
- [x] **排列**（arrange）：位置、文繞圖、上移一層、下移一層、對齊、群組、旋轉、編輯繞排位置；右下角對話方塊啟動器。

### [參考資料]（tab-references）
- [x] **目錄**（toc）：目錄、新增文字、更新目錄。
- [x] **註腳**（footnotes）：插入註腳、插入章節附註、下一個註腳。
- [x] **引文與書目**（citations）：插入引文、管理來源、書目樣式（APA/MLA）、書目。
- [x] **題注**（captions）：插入題注、插入圖表目錄、更新圖表。
- [x] **索引**（index）：標記項目、插入索引、更新索引。
- [x] **法律授權目錄**（authorities）：標記引文、公文目錄。

### [校閱]（tab-review）
- [x] **校對**（proofing）：拼字及文法檢查、同義字、字數統計、反向校對。
- [x] **語言**（language）：翻譯、語言設定。
- [x] **註解**（comments）：新增註解、刪除、解決、顯示註解、上一個、下一個。
- [x] **追蹤與修訂**（tracking）：追蹤修訂、顯示標記、修訂窗格、接受、拒絕。
- [x] **溝通與保護**（protect）：限制編輯、限制格式、強制保護、停止保護。

### [檢視]（tab-view）
- [x] **檢視**（views）：閱讀模式、列印配置、Web 版面配置、大綱、焦點、草稿。
- [x] **顯示**（show）：標尺開關（live）、格線開關（live）、導覽窗格開關（live）。
- [x] **顯示比例**（zoom）：100% 重設（live）、適合頁寬（live）、單頁、多頁、縮放對話方塊。
- [x] **視窗**（window）：開新視窗、整齊排列、分割、切換視窗。
- [x] **巨集**（macros）：巨集。

### [說明]（tab-help）
- [x] **說明**（help_group）：說明（開啟系統說明對話方塊，live）、連絡客戶支援、意見反應、顯示訓練課程。
- [x] **操作搜尋**（search_group）：操作搜尋輸入框與執行按鈕。

---

## 4. 停用態命令與依賴卡片對照表

所有停用態按鈕皆嚴格遵循「產品誠實紅線」，在視覺上呈現半透明置灰（`opacity: 0.62`）、禁止游標（`cursor: not-allowed`），hover 時浮現 Tooltip 提示「⚠️ 需要編輯 API（開發中: <依賴卡>）」，點擊時派發至 `window.QWordShell.onCommand` 記錄於日誌中但標註 `handled: false`。

| 依賴卡號 | 依賴項目範圍 | 按鈕數 | 說明 |
| :--- | :--- | :---: | :--- |
| `pending:K29a` | 基礎文字編輯、剪貼簿、字型字號色彩、段落對齊縮排行距、尋找取代 | 44 | 等待 K29a 編輯輸入與文字排版 API |
| `pending:K29b` | 實體物件插入（表格、圖片、形狀、文字方塊、頁首頁尾、分頁符、超連結） | 21 | 等待 K29b 容器與圖形物件模型 API |
| `pending:K29c` | 樣式套用、版面邊界紙張設定、多欄、文繞圖、浮水印、背景色 | 40 | 等待 K29c 樣式級聯與分頁排版引擎 |
| `pending:K29d` | 拼字校對、同義字、字數統計、翻譯、註解評論、修訂追蹤、保護模式 | 22 | 等待 K29d 語法校對與協同審閱 API |
| `pending:K29e` | 目錄生成、註腳章節附註、引文文獻、圖表題注、索引標記 | 21 | 等待 K29e 參考資料與文獻引用結構 |
| `pending:K29f` | 本機檔案儲存、雲端同步、線上共用 | 3 | 等待 K29f 儲存與通訊傳輸協定 |
| `pending:K29s` | 視窗分割、重排、多視窗檢視 | 4 | 等待 K29s 桌面多檢視視窗管理系統 |
| `pending:K29m` | 巨集錄製與自動化腳本 | 1 | 等待 K29m 巨集執行環境 |
| `pending:K29u` | 線上技術支援、使用者反饋通道、線上教學影片 | 4 | 等待 K29u 外部服務端介接 |
| `live` | 標籤切換、縮放 60–180%、頁寬適配、檢視模式、標尺切換與刻度拖曳、8×5 網格選擇器、特殊符號面板、說明視窗 | 31 | 當前靜態外殼完整具備之能力 |

---

## 5. 已知未實作項（照實清單）

以下為 U1 窗體骨架階段未包含之深層實作（保留予後續卡片）：
1. **Quire 渲染排版引擎接入**：當前編輯區紙頁內文為展示用靜態 HTML 結構，後續卡片將掛載 WASM/Canvas 渲染器。
2. **文字實時排版輸入與游標 (Caret)**：當前紙頁設定為 `contenteditable="false"`，真實文字輸入、選區與 IME 處理需等待 K29a。
3. **對話方塊實體內容**：各群組右下角的對話方塊啟動器（Launcher）目前派發 `dialog:*` 指令並記錄日誌，具體設定對話方塊視窗待後續卡片實作。
4. **巨集執行器與 VBA 解譯器**：巨集按鈕僅保留外殼位與停用態。
