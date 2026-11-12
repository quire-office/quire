# 任務書 WORDUI-U3 驗收報告：樣式命令啟用——K29b 引擎能力進 Ribbon

## 1. 執行身分與工作位置自證

依據任務書第 0 條鐵律，開工第一步自證工作位置：

- **工作目錄路徑**：`<repo>/.worktrees/agy-u3（本機工作樹，不在 repo 內）`
- **自證指令**：`pwd`
- **實測輸出**：
  ```
  <repo>/.worktrees/agy-u3（本機工作樹，不在 repo 內）
  ```
- **工作分支**：`t/agy-u3`（基線 `c242ec6`）
- **規範遵循**：
  - 全流程未跨出此 worktree。
  - 零外部網路與外部字型依賴。
  - 嚴格遵守繁體中文規範，全篇禁用簡體字與 emoji 作為介面圖示或狀態符號。
  - 嚴格遵守「改動留工作區，禁止 git commit/push/reset/checkout」。
  - 嚴格遵守「嚴禁動 core/ 引擎側」，本片純粹改動 `shells/web/editor/`、`scripts/` 與說明文件。
  - 嚴守可見品牌文字不得出現 Microsoft / Word，產品名稱固定為 Quire。

---

## 2. 成果概述

本任務（WORDUI-U3）成功將已入庫之 K29b 引擎樣式與段落能力全面接通至 Ribbon 前端外殼，實現以下核心閉環：
1. **字元直接格式（M2.1 FormatTextRange）**：
   - 粗體（`format:bold`）與斜體（`format:italic`）完整支援選取範圍套用，並具備反轉切換（Toggle）與狀態同步能力。
   - 字型色彩（`format:font-color` 與 `format:font-color-palette`）：支援調色盤色票點選，色碼即時轉換為 `0xAARRGGBB` 正準數值傳遞至引擎。
   - 字級縮放與選擇（`font:grow`、`font:shrink`、`font:size`）：支援點選字級階梯縮放與下拉選單直選，數值直接對應 `RunDirectPatch.size_pt`。
2. **段落對齊格式（M2.2 SetParagraphStyle）**：
   - 段落靠左（`paragraph:align-left`）、置中（`paragraph:align-center`）、靠右（`paragraph:align-right`）、兩端對齊（`paragraph:align-justify`）全面接通 `ParaPropsPatch.alignment`。
3. **具名段落樣式（M2.2 SetParagraphStyle）**：
   - 樣式庫內建樣式（內文 `style:normal`、無間距 `style:no-spacing`、標題 1 `style:h1`、標題 2 `style:h2`、標題 3 `style:h3`）全面接通 `ParaPropsPatch.style_id`。
4. **畫布拖曳選取與高亮閉環**：
   - 在真分頁 Canvas 上支援滑鼠拖曳選取文字，透過 `caretAt` 幾何落點反查建立半開 `DocRange`，並於 `.qword-caret-layer` 上即時疊加視覺半透明選取藍框。
5. **完整的 Undo / Redo 復原閉環**：
   - 所有 live 命令皆經由 `apply_mutation_json` 送入引擎並生成逆修補，按 `Ctrl+Z` 能即時撤銷格式變更並自動觸發增量重繪。

---

## 3. 按鈕狀態清單對照表（產品誠實紅線）

依據產品誠實紅線最高指導原則：只有引擎 Mutation 真支援的 patch 欄位才能轉 live。引擎 `RunDirectPatch`（`core/engine-api/src/mutation.rs:69`）僅支援 `bold`、`italic`、`color_rgb`、`size_pt`、`char_style` 五欄。其餘未支援之樣式功能**一律維持 pending，絕不假裝可用、絕不隱藏按鈕**。

### 3.1 本片轉 Live 按鈕清單（共 16 項）

| 按鈕 `data-command` | 原 requirement | 現 requirement | 引擎 Mutation 映射 | 說明 |
| :--- | :--- | :--- | :--- | :--- |
| `format:bold` | `pending:K29b` | `live` | `M2.1 FormatTextRange` (`patch.bold`) | 粗體切換，套用於選取範圍；未選取時套用於目前段落 |
| `format:italic` | `pending:K29b` | `live` | `M2.1 FormatTextRange` (`patch.italic`) | 斜體切換，套用於選取範圍；未選取時套用於目前段落 |
| `format:font-color` | `pending:K29b` | `live` | `M2.1 FormatTextRange` (`patch.color_rgb`) | 字型色彩（預設紅色 #e74c3c，寫出 0xFFFF0000） |
| `format:font-color-palette` | `pending:K29b` | `live` | `M2.1 FormatTextRange` (`patch.color_rgb`) | 開啟字型調色盤，點選色票直接套用選取色彩 |
| `font:grow` | `pending:K29b` | `live` | `M2.1 FormatTextRange` (`patch.size_pt`) | 放大字型（依標準磅值級距遞增） |
| `font:shrink` | `pending:K29b` | `live` | `M2.1 FormatTextRange` (`patch.size_pt`) | 縮小字型（依標準磅值級距遞減，下限 1pt） |
| `font:size` | `pending:K29b` | `live` | `M2.1 FormatTextRange` (`patch.size_pt`) | 字級下拉選單直選（8~72pt） |
| `paragraph:align-left` | `pending:K29b` | `live` | `M2.2 SetParagraphStyle` (`alignment: "left"`) | 目前段落或選取段落範圍靠左對齊 |
| `paragraph:align-center` | `pending:K29b` | `live` | `M2.2 SetParagraphStyle` (`alignment: "center"`) | 目前段落或選取段落範圍置中對齊 |
| `paragraph:align-right` | `pending:K29b` | `live` | `M2.2 SetParagraphStyle` (`alignment: "right"`) | 目前段落或選取段落範圍靠右對齊 |
| `paragraph:align-justify` | `pending:K29b` | `live` | `M2.2 SetParagraphStyle` (`alignment: "both"`) | 目前段落或選取段落範圍兩端對齊 |
| `style:normal` | `pending:K29c` | `live` | `M2.2 SetParagraphStyle` (`style_id: "Normal"`) | 套用內文樣式 |
| `style:no-spacing` | `pending:K29c` | `live` | `M2.2 SetParagraphStyle` (`style_id: "NoSpacing"`) | 套用無間距樣式 |
| `style:h1` | `pending:K29c` | `live` | `M2.2 SetParagraphStyle` (`style_id: "Heading1"`) | 套用標題 1 樣式 |
| `style:h2` | `pending:K29c` | `live` | `M2.2 SetParagraphStyle` (`style_id: "Heading2"`) | 套用標題 2 樣式 |
| `style:h3` | `pending:K29c` | `live` | `M2.2 SetParagraphStyle` (`style_id: "Heading3"`) | 套用標題 3 樣式 |

### 3.2 誠實維持 Pending 按鈕清單（嚴守紅線，共 141 項）

引擎尚未支援或需要後續切片支援之功能，全部誠實標註 requirement：
- **pending:K29b 樣式與排版面（42 個）**：
  - 字型裝飾未支援：底線 (`format:underline`、`format:underline-style`)、刪除線 (`format:strikethrough`)、上標 (`format:superscript`)、下標 (`format:subscript`)、文字醒目色 (`format:highlight`、`format:highlight-palette`)、清除所有格式 (`font:clear-formatting`)、字體族名稱 (`font:family`)。
  - 段落進階未支援：項目符號清單 (`paragraph:bullets`)、編號清單 (`paragraph:numbering`)、多層清單 (`paragraph:multilevel`)、減少縮排 (`paragraph:decrease-indent`)、增加縮排 (`paragraph:increase-indent`)、排序 (`paragraph:sort`)、段落標記 (`paragraph:marks`)、行距選單 (`paragraph:line-spacing*`)、底紋 (`paragraph:shading*`)、框線 (`paragraph:borders*`)。
  - 插入物件未支援：表格、圖片、圖形、SmartArt、圖表、方程式、文字方塊、頁首/頁尾/頁碼、分頁符號等。
- **pending:K29c 版面與樣式面（35 個）**：樣式展開 (`style:more`)、樣式對話方塊 (`dialog:styles`)、版面方向、紙張大小、分欄、邊界、浮水印等。
- **pending:K29d 校閱與註釋面（22 個）**：拼字檢查、翻譯、語言、批註、追蹤修訂、保護等。
- **pending:K29e 參考資料面（21 個）**：目錄、註腳、引文、索引等。
- **pending:shell 外殼待接線（11 個）**：剪貼簿面板、尋找/取代對話方塊、段落對話方塊 (`dialog:paragraph`)、字型對話方塊 (`dialog:font`)。
- **pending:K29s 視窗面（4 個）**：開新視窗、分割視窗等。
- **pending:K29u 支援面（4 個）**：線上客戶支援、意見反應等。
- **pending:K29m 巨集面（1 個）**：巨集檢視。
- **pending:K29f 協同面（1 個）**：文件共用。

---

## 4. 引擎欄位與 Mutation JSON 映射規格

### 4.1 M2.1 FormatTextRange
- **用途**：對半開正準碼點範圍（`start..end`）套用字元級直接格式。
- **JSON 結構範例**：
  ```json
  {
    "FormatTextRange": {
      "range": {
        "target": "Body",
        "start": { "para": 1, "cp": 0 },
        "end": { "para": 1, "cp": 4 }
      },
      "patch": {
        "bold": true,
        "italic": null,
        "color_rgb": 4294901760,
        "size_pt": 24.0,
        "char_style": null
      }
    }
  }
  ```
- **引擎對應**：`core/engine-api/src/mutation.rs:154` 之 `RunDirectPatch`。

### 4.2 M2.2 SetParagraphStyle
- **用途**：對一或多個連續段落（`para_range`）套用段落對齊或具名樣式。
- **JSON 結構範例（對齊）**：
  ```json
  {
    "SetParagraphStyle": {
      "target": "Body",
      "para_range": { "start": 1, "end": 1 },
      "patch": {
        "style_id": null,
        "page_break_before": null,
        "line_spacing": null,
        "alignment": "center"
      }
    }
  }
  ```
- **JSON 結構範例（樣式）**：
  ```json
  {
    "SetParagraphStyle": {
      "target": "Body",
      "para_range": { "start": 0, "end": 0 },
      "patch": {
        "style_id": "Normal",
        "page_break_before": null,
        "line_spacing": null,
        "alignment": null
      }
    }
  }
  ```
- **引擎對應**：`core/engine-api/src/mutation.rs:159` 之 `ParaPropsPatch`。

---

## 5. 邊界說明與語義降級記帳

### 5.1 未選取語義說明（鍵入即生效邊界）
- **引擎現狀**：引擎 `InsertText` 雖在列舉宣告有 `style_override: Option<RunDirectPatch>`，但內部實作於 `mutation.rs:1321` 暫以 `style_override: _` 忽略該欄位，尚未支援輸入時自帶全新 run 格式。
- **外殼處理原則**：
  - 若使用者已在 Canvas 上拖曳框選文字（`this.adapter.selection` 存在），命令精確作用於該 `DocRange`。
  - 若使用者未選取任何文字（僅有點狀游標），命令降級為**作用於游標所在之整段段落文字**。
  - 按鈕 tooltip 誠實標示：「套用於選取範圍（未選取時套用於目前整段）」，絕不欺瞞使用者。

### 5.2 樣式 id 參照邊界說明
- **引擎守衛規則**：依據 `mutation.rs:1924`，引擎對 `patch.style_id` 設有型別與存在性守衛，只有在該 DOCX 檔案的 `word/styles.xml` 確實宣告有該段落型樣式時才允許套用；若檔案缺少 `styles.xml`（如極簡示範檔）或未宣告該樣式 id，引擎將誠實回傳 `MutationError::InvalidStyleReference`。
- **外殼處理**：外殼在收到引擎拒絕時，於主控台誠實警告，維持既有排版不崩潰。

---

## 6. 驗收閘門完整實測輸出

### 6.1 U3 樣式命令驗收腳本 (`node scripts/u3_selfcheck.mjs`)
```
======================================================================
 AGY-U3 樣式命令啟用——K29b 引擎能力進 Ribbon 自動化驗收檢查
======================================================================

[檢查 1] 驗證必要外殼檔案與 Wasm 產物存在性...
  ✓ Wasm-pack JS 產物 (shells/web/pkg/quire_web.js)
  ✓ Wasm-pack 二進位檔 (shells/web/pkg/quire_web_bg.wasm)
  ✓ 引擎適配器 (shells/web/editor/js/engine-adapter.js)
  ✓ 核心邏輯層 (shells/web/editor/js/qword-core.js)
  ✓ UI 綁定層 (shells/web/editor/js/qword-ui.js)
  ✓ Vendor 繪製器 (shells/web/editor/vendor/renderer.js)
  ✓ 外殼頁面 (shells/web/editor/index.html)
  ✓ 樣式表 (shells/web/editor/css/style.css)

[檢查 2] 驗證零外部網路依賴...
  ✓ HTML 零外部網路依賴
  ✓ CSS 零外部網路依賴

[檢查 3] 驗證 Mutation JSON 結構與工廠函數契約 (FormatTextRange & SetParagraphStyle)...
  ✓ M2.1 FormatTextRange (bold) Mutation JSON 符合引擎契約
  ✓ M2.1 FormatTextRange (color_rgb / size_pt) Mutation JSON 符合引擎契約
  ✓ M2.2 SetParagraphStyle (alignment) Mutation JSON 符合引擎契約
  ✓ M2.2 SetParagraphStyle (style_id) Mutation JSON 符合引擎契約

[檢查 4] 審查 pending -> live 轉移清單與按鈕標記誠實性（產品誠實紅線）...
  ✓ 按鈕 [format:bold] 標記為 live
  ✓ 按鈕 [format:italic] 標記為 live
  ✓ 按鈕 [format:font-color] 標記為 live
  ✓ 按鈕 [format:font-color-palette] 標記為 live
  ✓ 按鈕 [font:grow] 標記為 live
  ✓ 按鈕 [font:shrink] 標記為 live
  ✓ 按鈕 [font:size] 標記為 live
  ✓ 按鈕 [paragraph:align-left] 標記為 live
  ✓ 按鈕 [paragraph:align-center] 標記為 live
  ✓ 按鈕 [paragraph:align-right] 標記為 live
  ✓ 按鈕 [paragraph:align-justify] 標記為 live
  ✓ 按鈕 [style:normal] 標記為 live
  ✓ 按鈕 [style:no-spacing] 標記為 live
  ✓ 按鈕 [style:h1] 標記為 live
  ✓ 按鈕 [style:h2] 標記為 live
  ✓ 按鈕 [style:h3] 標記為 live
  ✓ 按鈕 [format:underline] 誠實維持 pending:K29b
  ✓ 按鈕 [format:underline-style] 誠實維持 pending:K29b
  ✓ 按鈕 [format:strikethrough] 誠實維持 pending:K29b
  ✓ 按鈕 [format:subscript] 誠實維持 pending:K29b
  ✓ 按鈕 [format:superscript] 誠實維持 pending:K29b
  ✓ 按鈕 [format:highlight] 誠實維持 pending:K29b
  ✓ 按鈕 [format:highlight-palette] 誠實維持 pending:K29b
  ✓ 按鈕 [font:family] 誠實維持 pending:K29b
  ✓ 按鈕 [font:clear-formatting] 誠實維持 pending:K29b
  ✓ 按鈕 [paragraph:bullets] 誠實維持 pending:K29b
  ✓ 按鈕 [paragraph:numbering] 誠實維持 pending:K29b
  ✓ 按鈕 [paragraph:multilevel] 誠實維持 pending:K29b

[檢查 5] 探針驗證：Node.js ESM 執行 WasmEngine 樣式變更閉環與 Undo...
  ✓ WebAssembly 模組初始化成功
  ✓ DOCX 開檔成功：頁數=2，段落數=14
  ✓ M2.1 粗體閉環成功：Text.flags & 1 = 1 (bold 成立)
  ✓ 粗體 Undo 閉環成功：文字樣式已完全還原
  ✓ M2.1 斜體閉環成功：Text.flags & 2 = 2 (italic 成立)
  ✓ 斜體 Undo 閉環成功
  ✓ M2.1 字色閉環成功：color_argb = 0xFFFF0000
  ✓ 字色 Undo 閉環成功
  ✓ M2.1 字級閉環成功：size_pt = 24
  ✓ 字級 Undo 閉環成功
  ✓ M2.2 段落對齊 (center/right/both/left) 閉環與 Undo 驗證通過
  ✓ M2.2 樣式 style_id (Normal) 閉環與 Undo 驗證通過
  ✓ 存檔閉環驗證通過：匯出 1302 位元組合規 DOCX

======================================================================
 自檢結果: PASS
 恭喜！AGY-U3 樣式命令啟用（K29b 引擎能力進 Ribbon）通過所有規格與紅線檢查。
======================================================================
```

### 6.2 既有 U2 接線迴歸測試 (`node scripts/u2_selfcheck.mjs`)
```
======================================================================
 AGY-U2 真引擎進外殼（Wasm 載入＋打字閉環）自動化驗收檢查
======================================================================

[檢查 1] 驗證必要外殼檔案與 Wasm 產物存在性...
  ✓ Wasm 殼導出層 (shells/web/src/lib.rs)
  ✓ Wasm-pack JS 產物 (shells/web/pkg/quire_web.js)
  ✓ Wasm-pack 二進位檔 (shells/web/pkg/quire_web_bg.wasm)
  ✓ 引擎適配器 (shells/web/editor/js/engine-adapter.js)
  ✓ 核心邏輯層 (shells/web/editor/js/qword-core.js)
  ✓ UI 綁定層 (shells/web/editor/js/qword-ui.js)
  ✓ Vendor 繪製器 (shells/web/editor/vendor/renderer.js)
  ✓ 外殼頁面 (shells/web/editor/index.html)
  ✓ 樣式表 (shells/web/editor/css/style.css)

[檢查 2] 驗證零外部網路依賴...
  ✓ HTML 零外部網路依賴
  ✓ CSS 零外部網路依賴

[檢查 3] 驗證 Mutation JSON 結構與工廠函數契約...
  ✓ M1.1 InsertText Mutation JSON 符合引擎 serde 契約
  ✓ M1.2 DeleteRange Mutation JSON 符合引擎 serde 契約
  ✓ M1.5 InsertBreak Mutation JSON 符合引擎 serde 契約

[檢查 4] 審查 pending -> live 轉移清單與按鈕標記誠實性...
  - 本卡轉為 live 的指令清單: file:save, edit:undo, edit:redo, format:bold, format:italic, format:font-color, format:font-color-palette, format:color:font, font:grow, font:shrink, font:size, paragraph:align-left, paragraph:align-center, paragraph:align-right, paragraph:align-justify, style:normal, style:no-spacing, style:h1, style:h2, style:h3
  ✓ 按鈕 [file:save] 標記為 live (接 save_docx_bytes)
  ✓ 按鈕 [edit:undo] 標記為 live (接 undo_json)
  ✓ 按鈕 [edit:redo] 標記為 live (接 redo_json)
  ✓ 按鈕 [format:bold] 標記合規（pending:K29b 或 live）
  ✓ 按鈕 [format:italic] 標記合規（pending:K29b 或 live）
  ✓ 按鈕 [format:underline] 誠實維持 pending（K29b 樣式面，不假裝可用）（不假裝可用）

[檢查 5] 探針驗證：Node.js ESM 執行 WasmEngine 打字閉環...
  ✓ WebAssembly 模組初始化成功
  ✓ DOCX 開檔成功：頁數=2，段落數=14，總字數=3579
  ✓ 游標幾何定位閉環驗證通過 (para=0, cp=0 -> page=0, x=72, y=72)
  ✓ M1.1 打字閉環成功：插入 12 字元，游標位移至 cp=12
  ✓ M1.2 退格閉環成功：刪除 1 字元，游標位移至 cp=11
  ✓ M1.5 軟換行 (LineWrapping) 執行成功
  ✓ 復原 (Undo) 成功還原最新操作
  ✓ 重做 (Redo) 成功重做操作
  ✓ 存檔閉環驗證通過：匯出 1315 位元組，合規 DOCX 容器

======================================================================
 自檢結果: PASS
 恭喜！AGY-U2 真引擎接線與打字閉環通過所有規格與紅線檢查。
======================================================================
```

### 6.3 既有 U1 外殼原型迴歸測試 (`node scripts/wordui_u1_selfcheck.mjs`)
```
======================================================================
 Quire Word UI 外殼原型 (U1) 自動驗收與完整性檢查
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
  - 掃描按鈕總數: 194
  - 即刻可用 (live): 53
  - 停用待實現 (pending): 141
      * pending:K29d: 22 個
      * pending:K29u: 4 個
      * pending:K29f: 1 個
      * pending:shell: 11 個
      * pending:K29b: 42 個
      * pending:K29c: 35 個
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

### 6.4 Wasm 目標編譯檢查 (`cargo check -p quire-web --target wasm32-unknown-unknown`)
```
$ cargo check -p quire-web --target wasm32-unknown-unknown
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.94s
```

### 6.5 工作區靜態分析檢查
```
$ cargo fmt --all --check
$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.28s
```
