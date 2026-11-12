# Quire 文書編輯器外殼原型（U1 窗體骨架）

本目錄提供一個視覺上對齊 傳統桌上型文書處理器 Ribbon 佈局風格的靜態原型外殼，零構建（zero-build）、零網路請求、零第三方依賴，旨在作為 Quire 排版與編輯引擎後續接軌的桌面型使用者介面骨架。

---

## 1. 如何開啟與檢視

### 本地瀏覽器直接開啟（最簡方式）
本原型完全採用靜態標準 HTML5、CSS 與純 JavaScript（原生 ES Module），無需任何 `npm install` 或編譯流程。
可以直接以任一現代瀏覽器開啟主頁面：

```bash
# Linux
xdg-open shells/web/editor/index.html

# macOS
open shells/web/editor/index.html

# Windows
start shells/web/editor/index.html
```

### 透過本地 HTTP 伺服器開啟
若偏好透過 HTTP 協定瀏覽：

```bash
# 在 worktree 根目錄啟動簡易靜態伺服器
python3 -m http.server 8080

# 瀏覽器訪問：
# http://localhost:8080/shells/web/editor/index.html
```

---

## 2. 架構設計與內部事件匯流排

外殼程式碼遵循純邏輯層與 DOM 表現層解耦設計：

1. **純邏輯層 (`js/qword-core.js`)**：
   - 零 DOM 依賴（無 `window`、`document` 等全域物件），可在 Node.js 環境中直接 `import` 進行無頭（headless）單元測試與探針驗證。
   - 內建 `QWordCore` 類別，管理標籤狀態、檢視模式（閱讀/列印/Web）、縮放比例（60%–180%）、標尺邊界與縮排量、以及指令歷史日誌。
   - 核心分發入口為 `onCommand(cmdId, payload, requirement)`。

2. **DOM 綁定與呈現層 (`js/qword-ui.js`)**：
   - 負責 DOM 事件委派、Ribbon 標籤與鍵盤方向鍵導航（支援 ArrowLeft、ArrowRight、Home、End）。
   - 標尺滑桿（首行縮排、懸掛縮排、左縮排、右縮排）拖曳互動與畫布輔助對齊虛線。
   - 縮放滑桿與畫布 CSS 等比變換 (`transform: scale(...)`)。
   - 互動彈出層（8×5 表格選擇器、特殊字元符號面板、色彩挑選器、說明對話方塊）。
   - 掛載全域對象 `window.QWordShell` 與 `window.__qwordLog`。

### 事件匯流排呼叫介面
任何自外部注入或自動化測試探針皆可透過 `window.QWordShell` 觸發命令：

```javascript
// 派發命令
window.QWordShell.onCommand('zoom:in');
window.QWordShell.onCommand('tab:switch:tab-insert');
window.QWordShell.onCommand('clipboard:cut', {}, 'pending:K29a');

// 取得當前核心狀態
const state = window.QWordShell.getState();
console.log(state.activeTabId, state.zoom, state.viewMode);

// 讀取指令歷史日誌
console.log(window.__qwordLog);
```

---

## 3. 產品誠實紅線：命令依賴與停用態清單

所有按鈕與指令皆帶有明確的 `data-requirement` 屬性標記：
- `live`：現有外殼已具備之互動能力（如標籤切換、縮放、檢視模式切換、標尺開關、表格網格與符號面板預覽）。
- `pending:<依賴卡>`：Quire 引擎尚在開發中之能力。按鈕呈現為**可見但停用**（半透明置灰、游標為 `not-allowed`、hover 浮現 Tooltip 說明等待之後續卡號）。絕不假裝可用，亦不隱藏功能。

### 依賴卡片對照與命令統計

| 需求代碼 | 依賴卡片名稱與說明 | 按鈕數量 | 代表性指令 |
| :--- | :--- | :---: | :--- |
| `live` | **外殼與檢視既有能力**<br>標籤切換、縮放 60–180%、頁寬適配、閱讀/列印/Web 配置切換、標尺切換與刻度拖曳、8×5 表格選擇器、特殊符號面板 | 31 | `tab:switch:*`, `zoom:*`, `view:mode:*`, `ruler:toggle`, `table:grid-select`, `symbol:palette-select` |
| `pending:K29a` | **K29a 基礎編輯與格式化**<br>文字選取、剪貼簿、字型字號、粗斜底刪、上標下標、字型色彩、段落對齊、清單、行距、縮排、尋找取代 | 44 | `clipboard:paste`, `format:bold`, `format:font-color`, `paragraph:align-left`, `editing:replace` |
| `pending:K29b` | **K29b 物件與插入引擎**<br>封面頁、分頁符號、實體表格插入、圖片、形狀、SmartArt、圖表、文字方塊、直書文字、頁首頁尾、超連結 | 21 | `insert:picture`, `insert:shapes`, `insert:header`, `insert:footer`, `insert:textbox`, `insert:hyperlink` |
| `pending:K29c` | **K29c 版面與樣式系統**<br>主題、樣式集、自訂樣式庫套用、頁面邊界、紙張直橫向、多欄排版、頁面框線、浮水印、文繞圖、物件對齊 | 40 | `style:h1`, `style:normal`, `layout:margins`, `layout:orientation`, `layout:columns`, `layout:wrap-text` |
| `pending:K29d` | **K29d 校對與審閱協同**<br>拼字語法檢查、同義字辭典、字數統計、翻譯、註解/批註增修、追蹤修訂與接受/拒絕、文件編輯保護 | 22 | `review:spelling-grammar`, `review:thesaurus`, `review:new-comment`, `review:track-changes`, `review:restrict-editing` |
| `pending:K29e` | **K29e 參考資料與文獻**<br>自動目錄生成與更新、註腳與章節附註、APA/MLA 引文與書目、圖表題注與圖表目錄、索引項目標記 | 21 | `references:toc`, `references:insert-footnote`, `references:insert-citation`, `references:insert-caption`, `references:mark-entry` |
| `pending:K29s` | **K29s 視窗與多重視圖**<br>開新視窗、多重視窗重排、視窗水平分割、切換視窗 | 4 | `window:new`, `window:arrange-all`, `window:split`, `window:switch` |
| `pending:K29m` | **K29m 巨集與腳本擴充**<br>VBA / JavaScript 巨集錄製與執行管理 | 1 | `macro:view` |
| `pending:K29f` | **K29f 檔案與協同傳輸**<br>本機與雲端儲存、線上共用分享 | 3 | `file:save`, `file:menu`, `doc:share` |
| `pending:K29u` | **K29u 支援與說明連線**<br>連絡線上技術支援、使用者意見反饋、線上教學資源 | 4 | `help:contact-support`, `help:feedback`, `help:show-training`, `qat:customize` |

---

## 4. 自動化自檢驗證

工作區提供 `scripts/wordui_u1_selfcheck.mjs` 腳本，可自動驗證全部規格：

```bash
node scripts/wordui_u1_selfcheck.mjs
```

驗證範圍包括：
1. 外殼必要檔案存在性與路徑正確。
2. 零外部網路依賴（零 CDN、零外部字型、零遠端 API）。
3. 100% 按鈕皆具備合法 `data-requirement` 與 `aria-label`。
4. 8 大 Ribbon 標籤與 34 個指定群組之 DOM 結構完整性。
5. Node.js ESM 無 DOM 環境直接載入 `qword-core.js` 執行指令與狀態驗證。
