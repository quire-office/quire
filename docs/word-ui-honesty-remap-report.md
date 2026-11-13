# Ribbon 誠實性修正收案報告：insert:table 真接線＋57 顆失效標籤逐鈕重掛

執行者：ow-render。分支 `wt/honesty-remap`，基線 main `74d22dd`。嚴禁 push（未 push），
嚴禁分支重置類操作（未執行）。白名單外零觸碰：`core/` 未動過一個 byte。

三筆 commit：
- `9ecebbb` 項一：insert:table 接上引擎 M4.1
- `40a1301` 項二／三：58 顆鈕逐鈕重掛
- `5e8528f` 項四：自檢同步

---

## 1. 項一：`insert:table` 採用哪條路＋程式碼位置

**採優選路：真的接上引擎。** 未採用降標籤次選，未做第三種（未假裝成功、未藏按鈕）。

| 檔案 | 位置 | 內容 |
|---|---|---|
| `shells/web/editor/js/engine-adapter.js` | `buildInsertTableMutation()`（建構器區，`buildSetParagraphStyleMutation` 之後） | 產 `{"InsertTable": {target, after_para, rows, cols, col_widths_pt}}`。欄位名逐欄照抄 `core/engine-api/src/mutation.rs:292-298` 的 M4.1 宣告（**先讀該段再寫，未猜**），未帶欄位以 `null` 明確缺席 |
| `shells/web/editor/js/engine-adapter.js` | `EngineAdapter.insertTable()`（`setParagraphStyle` 之後） | 組件→`apply_mutation_json`→回傳引擎結果，成功時同步 `caret_after` 與 `isDirty` |
| `shells/web/editor/js/qword-core.js` | `onCommand()` 的 `table:grid-select` 分支 | 尺寸驗證＋發 `table:grid-insert` 事件匯流；尺寸非法時**不得沿用呼叫端帶入的 live**，強制降回真實缺口標籤 |
| `shells/web/editor/js/qword-ui.js` | `handleInsertTable()`（`refreshPages` 之前）＋ `bindEvents()` 訂閱 | 真呼叫 `adapter.insertTable`；成功走既有 reflow 路徑（`refreshPages`／`renderCaret`／`renderSelection`／`syncEngineStats`）；未開檔或引擎拒絕走 `showErrorModal` 如實回報 |
| `shells/web/editor/js/qword-ui.js` | `showTooltip()` | `pending:shell` 措辭改「編輯 API 已支援，外殼接線中」，不再對使用者宣稱「開發中」 |
| `shells/web/editor/js/qword-core.js` | `DEPENDENCY_MAP` | 修掉重複鍵（`pending:K29b` 原本宣告兩次、後者蓋前者）與已發行切片的過時描述 |

分層契約：`qword-core.js` 是 DOM-free 純邏輯層（零 import，可在 Node 直接跑），**不能**
直接持有 adapter——實例由 `qword-ui.js` 建立（`this.adapter = new EngineAdapter()`），
UI 層同時是 wasm 載入失敗的 `fallbackMode` 閘門所在。故沿用既有 `format:bold` 形態：
core 只做記帳與事件匯流（`_notify`），UI 層訂閱後呼 adapter。本輪未另創機制，
未讓 core 反向依賴 UI（那會破壞 U1 自檢的零 DOM 前提）。

### 瀏覽器閉環證據鏈（讀用戶看見那層，未讀任何自報旗標）

環境：`python3 -m http.server 8391`，頁面 `shells/web/editor/index.html`，wasm 產物為
repo 內 `shells/web/pkg/`（gitignored 產物，非入庫檔）。

| 步驟 | 觀測 |
|---|---|
| 1. 開示範檔 | `pageCount=2`、`paragraphCount=14`、`page_display_list(0).items.length=41`（全 `Text`）、`isSettled=true` |
| 2. 按「表格」鈕（真實 click） | popover `open=true`，網格 40 格建立 |
| 3. 滑鼠真實座標點 `3 列 × 5 欄` 格 | `click_at_xy(98, 83)` |
| 4. **引擎被呼到**（非旗標） | `paragraphCount: 14 → 29`（＋15 ＝ 3×5 格內段落）；`page_display_list(0).items.length: 41 → 53` |
| 5. item 內容變化 | 新增 15 筆空 `Text` item，x 依欄位推進（77.4／167.6／257.8……），y 依列推進——即表格儲存格段落真的進顯示列表 |
| 6. **canvas 像素層**（用戶看見那層） | 第 0 頁 canvas hash `2157956449 → 474386859`，非白像素 `127215 → 116813`（版面重排，文字下推） |
| 7. Undo（真實 Ctrl+Z） | `paragraphCount` 回 14、items 回 41、**canvas hash 逐位回到 2157956449、非白像素回 127215**——插入是嚴格可逆且畫面完全還原 |
| 8. 錯誤面 | `modal-error.open=false`（無失敗彈窗），指令日誌 `table:grid-select / live / handled=true / payload={rows:3,cols:5}` |

截圖：`evidence_table_before.png`／`evidence_table_after.png`（隨本卡 artifacts 交付）。

Node 側另有同鏈路探針（`apply_mutation_json` 直呼）：paras 14→20、items 41→44、
undo 後顯示列表與插入前 **byte-identical**（`reversibility="StrictByteReversible"`）。

---

## 2. 項二：逐鈕重掛表（58 顆，含每條查證依據）

### 紀律

- **未做字串批次全替**：改寫腳本以 `data-command` 逐顆為單位，每顆獨立指定
  新標籤＋新 title＋查證依據；腳本內映射表缺項即跳過不改（查不到依據就不改）。
- 三查判據（逐鈕實跑，工具：`grep -n "<命令名>" js/qword-core.js js/engine-adapter.js js/qword-ui.js`
  ＋讀 `core/engine-api/src/mutation.rs` 變體宣告與 patch 結構體＋讀
  `mutation_k29c.rs`／`mutation_k29d.rs` 實寫路徑）：
  - 查一：該 `data-command` 在 `js/` 有無真實 handler → **57 顆全部 0 命中**
  - 查二：其後端 Mutation 是否已在 `mutation.rs` 宣告
  - 查三：該 Mutation 的 patch 欄位是否真的涵蓋按鈕所需效果
- 三者全備才可 `live` → **本輪無一顆可轉 live**（查一全滅）。

### 統計

| 舊→新 | 顆數 | 語意 |
|---|---|---|
| `pending:K29c` → `pending:shell` | 12 | 引擎已有 Mutation 且欄位涵蓋，缺外殼接線 |
| `pending:K29c` → `pending:K29e` | 23 | 引擎無對應 Mutation（樣式面／版面面缺口） |
| `pending:K29d` → `pending:K29e` | 21 | 引擎無對應 Mutation（校閱面缺口） |
| `pending:K29d` → `pending:shell` | 1 | `review:word-count`：走既有段落文字出口即可 |
| `pending:K29b` → `pending:shell` | 1 | `insert:picture`：M5.1 欄位齊備（項三） |
| **合計** | **58** | 原 K29c/K29d 57 顆全數清償，加 1 顆項三 |

`pending:K29c` 與 `pending:K29d` 標籤現已歸零（`grep -c` 實測），標籤總數守恆 196 顆不變
（無鈕被移除或隱藏）。

### 逐鈕表

| data-command | 舊標籤 | 新標籤 | 判據（三項查證結果） |
|---|---|---|---|
| `style:more` | K29c | shell | 查一 js/ 無 handler；查二 M2.2 `SetParagraphStyle` 存在（mutation.rs:272）；查三 `patch.style_id` 涵蓋套用具名段落樣式 |
| `dialog:styles` | K29c | shell | 查一 無 handler；查二 M2.2 存在；查三 `style_id` 涵蓋效果，對話框本體屬外殼面 |
| `layout:margins` | K29c | shell | 查一 無 handler；查二 M6.1 `SetSectionProperties`（mutation.rs:402）；查三 `margin_top/bottom/left/right_pt` 四欄齊備，`mutation_k29d.rs` 實寫 `w:pgMar` |
| `design:layout-margins` | K29c | shell | 同上（M6.1 邊距欄位涵蓋） |
| `layout:orientation` | K29c | shell | 查一 無 handler；查二 M6.1 存在；查三 `orientation` 欄存在，`mutation_k29d.rs:454-472` 實作 pgSz 長短邊互換＋`w:orient` |
| `layout:columns` | K29c | shell | 查一 無 handler；查二 M6.1 存在；查三 `SectionColsPatch{num, space_pt}` 涵蓋分欄 |
| `design:layout-columns` | K29c | shell | 同上（M6.1 分欄欄位涵蓋） |
| `layout:breaks` | K29c | shell | 查一 無 handler；查二 M6.2 `InsertSectionBreak`（mutation.rs:407）＋M1.5 `InsertBreak`；查三 `SectionStartKind` 四態＋`BreakKind{Page,Column}` 涵蓋分隔符號 |
| `design:layout-breaks` | K29c | shell | 同上（M6.2＋M1.5 欄位涵蓋） |
| `dialog:page-setup` | K29c | shell | 查一 無 handler；查二 M6.1 存在；查三 邊距／方向／分欄欄位涵蓋（紙張絕對尺寸引擎尚無欄，title 已如實標註該項限制） |
| `layout:wrap-text` | K29c | shell | 查一 無 handler；查二 M5.1／M5.2（mutation.rs:363,376）；查三 `WrapModeSpec{None,Square,Tight,Through,TopAndBottom}` 涵蓋文繞圖模式 |
| `layout:position` | K29c | shell | 查一 無 handler；查二 M5.2 `ResizeOrMoveImage`；查三 `new_pos_h`／`new_pos_v`（`FloatingAxisSpec`）涵蓋圖片定位（圖形／文字框無對應 Mutation，title 已限定範圍） |
| `review:word-count` | K29d | shell | 查一 無 handler；查二 引擎段落文字讀取面存在（`paragraph_count`／`paragraph_text`，adapter 已封裝 `getWordCount`）；查三 統計資料可自現有出口取得，不需新 Mutation |
| `insert:picture` | K29b | shell | 查一 js/ 無 handler；查二 M5.1 `InsertInlineImage`／`InsertFloatingImage`（mutation.rs:353,363，屬 K29c 切片已入 main）；查三 `media_bytes`／`mime_type`／`width_pt`／`height_pt`／`alt_text` 欄位齊備涵蓋插圖效果 |
| `design:theme` | K29c | K29e | 查一 無 handler；查二 `theme1.xml` 於 `mutation*.rs` 0 命中（clrScheme/fontScheme 僅只讀投影）；查三 無寫入欄位 |
| `design:colors` | K29c | K29e | 同上（clrScheme 只讀） |
| `design:fonts` | K29c | K29e | 同上（fontScheme 只讀） |
| `design:effects` | K29c | K29e | 同上（effectScheme 無 Mutation） |
| `design:style-set` | K29c | K29e | 查一 無 handler；查二 M2.3 `UpsertNamedStyle` 僅單一樣式；查三 樣式集需 docDefaults＋批次改寫，無對應欄位 |
| `design:paragraph-spacing` | K29c | K29e | 查一 無 handler；查二 M2.2 `para_range` 限選取段落；查三 全文件預設間距需 docDefaults，`mutation.rs` 查無該欄 |
| `design:set-as-default` | K29c | K29e | 查一 無 handler；查二 `docDefaults` 於 `mutation*.rs` 0 命中；查三 預設值效果需 docDefaults 欄，不存在 |
| `design:watermark` | K29c | K29e | 查一 無 handler；查二 `w:background` 於 `core/` 0 命中；查三 無承載欄位 |
| `design:page-color` | K29c | K29e | 查一 無 handler；查二 `w:background` 0 命中；查三 無欄位 |
| `design:page-borders` | K29c | K29e | 查一 無 handler；查二 `pgBorders` 於 `core/` 0 命中；查三 無欄位 |
| `design:layout-direction` | K29c | K29e | 查一 無 handler；查二 `ParaPropsPatch` 僅 `style_id`／`page_break_before`／`line_spacing`／`alignment` 四欄；查三 `w:bidi` 於 `core/` 只出現在只讀註解，無寫入欄 |
| `design:layout-gutter` | K29c | K29e | 查一 無 handler；查二 M6.1 存在但 `gutter` 於 `mutation.rs` 0 命中；查三 patch 無 gutter 欄，效果無法涵蓋 |
| `design:layout-size` | K29c | K29e | 查一 無 handler；查二 M6.1 只有 `orientation` 互換，無絕對寬高欄；查三 紙張尺寸覆寫僅開檔期 `new_with_paper`（PG-PAPER-1），非 Mutation 路徑 |
| `layout:size` | K29c | K29e | 同上（無絕對尺寸欄位） |
| `layout:line-numbers` | K29c | K29e | 查一 無 handler；查二 `lnNumType` 於 `core/` 0 命中（未解析亦無 Mutation）；查三 無欄位 |
| `layout:hyphenation` | K29c | K29e | 查一 無 handler；查二 `hyphenation`（不分大小寫）於 `core/` 0 命中；查三 無欄位 |
| `dialog:arrange` | K29c | K29e | 查一 無 handler；查二 無排列類 Mutation；查三 無欄位可涵蓋 z 序／對齊／群組／旋轉 |
| `layout:bring-forward` | K29c | K29e | 查一 無 handler；查二 M5.2 欄位僅尺寸／定位／繞排；查三 `relativeHeight` 於 `mutation*.rs` 僅 1 處＝插入期常值，無 z 序調整欄 |
| `layout:send-backward` | K29c | K29e | 同上（無 z 序調整欄） |
| `layout:align` | K29c | K29e | 查一 無 handler；查二 無物件對齊變體；查三 無欄位 |
| `layout:group` | K29c | K29e | 查一 無 handler；查二 群組僅只讀解析（`v:group`／DML group）；查三 無 group/ungroup 欄位 |
| `layout:rotate` | K29c | K29e | 查一 無 handler；查二 無旋轉寫入變體；查三 無 `rot` 欄 |
| `layout:edit-wrap-points` | K29c | K29e | 查一 無 handler；查二 `WrapModeSpec` 僅五種模式列舉；查三 無自訂繞排點欄，效果無法涵蓋 |
| `review:quick-spelling` | K29d | K29e | 查一 無 handler；查二 `spell` 於 `core/` 0 命中；查三 無欄位（拼字面未入任何引擎切片） |
| `review:spelling-grammar` | K29d | K29e | 同上 |
| `review:reverse-spellcheck` | K29d | K29e | 同上 |
| `review:thesaurus` | K29d | K29e | 查一 無 handler；查二 無同義字變體（需外部詞庫）；查三 無欄位 |
| `review:translate` | K29d | K29e | 查一 無 handler；查二 無翻譯變體（需外部服務）；查三 無欄位 |
| `review:language` | K29d | K29e | 查一 無 handler；查二 `w:lang` 於 `core/` 0 命中；查三 無欄位 |
| `review:new-comment` | K29d | K29e | 查一 無 handler；查二 `w:comment`／`word/comments.xml` 於 `core/` 0 命中；查三 引擎無註解模型，無欄位 |
| `review:delete-comment` | K29d | K29e | 同上 |
| `review:resolve-comment` | K29d | K29e | 同上 |
| `review:show-comments` | K29d | K29e | 同上（無註解模型可列舉） |
| `review:prev-comment` | K29d | K29e | 同上 |
| `review:next-comment` | K29d | K29e | 同上 |
| `review:track-changes` | K29d | K29e | 查一 無 handler；查二 `w:ins`／`w:del` 於 `mutation*.rs` 0 命中（`core/ooxml` 僅只讀渲染修訂）；查三 無修訂寫入欄位 |
| `review:display-markup` | K29d | K29e | 同上（無修訂模型） |
| `review:reviewing-pane` | K29d | K29e | 同上 |
| `review:accept-change` | K29d | K29e | 同上 |
| `review:reject-change` | K29d | K29e | 同上 |
| `review:restrict-editing` | K29d | K29e | 查一 無 handler；查二 `documentProtection` 於 `core/` 0 命中；查三 無欄位 |
| `review:restrict-formatting` | K29d | K29e | 同上 |
| `review:enforce-protect` | K29d | K29e | 同上 |
| `review:stop-protect` | K29d | K29e | 同上 |

（上表 58 列＝原 `pending:K29c` 35 顆＋`pending:K29d` 22 顆全數，另加項三 `insert:picture`。）

### 未改動的標籤（查無依據或本就正確，不動）

- 42 顆 `pending:K29b`：引擎端 K29b 切片確未發行（底線／刪除線／上下標／highlight／
  字體族等六欄尚未進 `RunDirectPatch`），屬真實缺口，一字未動。
- 21 顆 `pending:K29e`（目錄／註腳／引文／索引族）：本輪不屬改掛範圍。
  **注意**：其中 `references:insert-footnote`／`insert-endnote` 對應的 M6.4
  `InsertNote` 已隨 K29d 入庫，真實缺口應是 `pending:shell`——但這兩顆不在本卡
  「57 顆 K29c/K29d」範圍內，且本卡紀律是「查不到依據就不改，寧可留舊標籤」，
  故**留舊標籤並在此點名**，交 PM 裁是否另開一張 K29e 面的同類清償卡。
- 11 顆 `pending:shell`（剪貼簿／尋找取代族）：屬外殼自身功能，標籤本就正確。
- 4 顆 `pending:K29s`、4 顆 `pending:K29u`、1 顆 `pending:K29m`、1 顆 `pending:K29f`：非本卡範圍。
- 54 顆 `live`：其中 U2/U3 的 16 顆有自檢引擎閉環鎖守死，未動。

---

## 3. 項三：`insert:picture`

標籤 `pending:K29b` → `pending:shell`，title 改「引擎已支援圖片插入（M5.1），檔案選取與
外殼接線進行中」。判據見上表第 58 列。

同步把 U1 自檢的 pending 日誌探針標籤由 `pending:K29b` 改為 `pending:shell`
（探針餵的是 index.html 的真實缺口，原值是錯掛標籤的鏡像）。

---

## 4. 項四：三份自檢同步（只准精確收窄，其餘一字不弱化）

| 自檢 | 改動 | 弱化檢查 |
|---|---|---|
| `scripts/wordui_u1_selfcheck.mjs` | 探針標籤同步（`clipboard:cut`、`insert:picture` 改餵 `pending:shell`）；**新增**兩條斷言：`table:grid-select` 非法尺寸→`handled=false` 且 `requirement≠live`；合法尺寸→`live`＋`handled=true` | 斷言數 1 條改值＋2 條新增，無刪除、無放寬 |
| `scripts/u2_selfcheck.mjs` | **新增** M4.1 `InsertTable` JSON 契約鎖（五欄逐欄＋`after_para=null` 尾插態＋`col_widths_pt` 填裝）；**新增**三條假 live 回塞鎖：adapter 必須真的 `buildInsertTableMutation`→`apply_mutation_json`、UI 必須真的接 `adapter.insertTable`、`insert:table` 按鈕必須存在且掛 live | 既有斷言全數保留（`format:underline` 誠實維持 `pending:K29b` 那條原樣不動） |
| `scripts/u3_selfcheck.mjs` | **零改動** | 其斷言面（16 顆 live＋12 顆 `pending:K29b`）與本卡重掛集合零交集 |

### 回塞鑑別實跑（一次性副本，真樹零觸碰）

| 態 | 結果 |
|---|---|
| GOOD（副本原樣） | `自檢結果: PASS` |
| 摘除 adapter 的 `apply_mutation_json` 呼叫 | `FAIL` —「紅線違規：engine-adapter.js 未真的以 buildInsertTableMutation 產件並呼叫 apply_mutation_json」 |
| 摘除 UI 接線（事件名改死） | `FAIL` —「紅線違規：qword-ui.js 未把 table:grid-select 接到 adapter.insertTable（假 live）」 |
| 把 `insert:table` 按鈕改名（模擬藏按鈕） | `FAIL` —「insert:table 應為 live（已真接 M4.1）」 |

三態皆如預期：接線退回首輪必紅，證明該鎖不是裝飾。

### 三份自檢實跑尾端（repo 根執行）

```
########## U1 TAIL
  ✓ QWordCore 無 DOM 模組在 Node.js 中載入並通過所有指令探針測試
======================================================================
 自檢結果: PASS
 恭喜！Quire 文書編輯器外殼 (U1 窗體骨架) 通過所有規格與紅線檢查。
======================================================================
########## U2 TAIL
  ✓ 存檔閉環驗證通過：匯出 1315 位元組，合規 DOCX 容器
======================================================================
 自檢結果: PASS
 恭喜！AGY-U2 真引擎接線與打字閉環通過所有規格與紅線檢查。
======================================================================
########## U3 TAIL
  ✓ 存檔閉環驗證通過：匯出 1302 位元組合規 DOCX
======================================================================
 自檢結果: PASS
 恭喜！AGY-U3 樣式命令啟用（K29b 引擎能力進 Ribbon）通過所有規格與紅線檢查。
======================================================================
U1=0 U2=0 U3=0
```

---

## 5. `git diff --name-only main...HEAD`（白名單覆核）

```
scripts/u2_selfcheck.mjs
scripts/wordui_u1_selfcheck.mjs
shells/web/editor/index.html
shells/web/editor/js/engine-adapter.js
shells/web/editor/js/qword-core.js
shells/web/editor/js/qword-ui.js
```

逐檔對白名單：`shells/web/editor/index.html` ✓、`shells/web/editor/js/*.js` ✓、
`scripts/u*_selfcheck.mjs` ✓、`docs/word-ui-*`（本報告）✓。
**`core/` 零觸碰**（`git diff main...HEAD -- core/` 為空）。未 push。

---

## 6. 繁中純度

掃 diff 新增行的 s2t 轉換差異，逐 pair 審計：
- 「側」字（U+4FA7 簡體形，此處不貼字面以免污染掃描）：自身註解真洩漏，已改為 U+5074。
- `群`(U+7FA4)→`羣`：正體即用「群」，`羣` 是異體，屬 skill 已記錄的 s2t 誤報，不改。
- 無歧義簡體字集掃描：0 hit。

---

## 7. 殘留與給 PM 的裁決點

1. **`references:insert-footnote`／`insert-endnote` 仍掛 `pending:K29e`**，但 M6.4
   `InsertNote` 已隨 K29d 入庫 → 真實缺口是 `pending:shell`。不屬本卡 57 顆範圍，
   依「查不到依據就不改」紀律留舊標籤並點名（見 §2 末）。建議另開一張卡做 K29e
   面的同類清償，或授權本卡範圍外補掛。
2. **`dialog:page-setup` 掛 `pending:shell` 的邊界**：M6.1 涵蓋邊距／方向／分欄，
   但無紙張絕對尺寸欄（只有開檔期 `new_with_paper`）。title 已寫明該限制。
   若 PM 認為「對話框主體有一項引擎做不到就整顆不准 shell」，此顆可改 `pending:K29e`。
3. **`layout:position` 掛 `pending:shell` 的邊界**：M5.2 涵蓋圖片定位，圖形／文字框
   無 Mutation。title 已限定「圖片」。同上裁決邏輯適用。
4. **M4.1 插入位置**：`insertTable()` 以目前游標段落作 `after_para`（未帶
   `col_widths_pt`，引擎走 100pt 預設）。網格選擇器目前無法指定欄寬——這是 UI
   面缺口，不是引擎缺口，未在本卡擴張範圍。
5. `pending:K29e` 現累積 65 顆（21 原＋44 本輪），與正在跑的 K29e 卡
   （t_4b55fc9d，字元格式六欄）**範圍不符**：該卡只解六顆字元格式鈕，本輪掛上去的
   44 顆多數是版面／樣式／校閱面。建議 K29e 後續切片按面拆卡，否則「掛 K29e」會
   再度變成第二個失效標籤池。
