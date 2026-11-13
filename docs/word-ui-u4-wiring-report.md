# U4：外殼接線第一批——M4 表格家族＋M5.1 圖片插入（報告）

任務：`t_e1b68330`（`U4：外殼接線第一批——M4 表格＋M5 圖片 Mutation 進 adapter`）
分支：`wt/u4-wiring`（已合併 parent `t_57b7b089`／`wt/honesty-remap`，自契約第 2 條起）
worker：`ow-render`

## 0. 結論速讀

契約第 1 條（`insert:table` → M4.1）由 parent `t_57b7b089` 完成，本卡未重複接。本卡完成第 2／3／4／5 條，並老實標出一項**未做**（邊框）與一項**降級項**（表格層底紋），兩者都有引擎側證據。

| 契約條目 | 引擎變體 | 狀態 |
| --- | --- | --- |
| 2 行列四變體 | M4.3 InsertTableRow／DeleteTableRow、M4.4 InsertTableColumn／DeleteTableColumn | 完成（三側真接線＋顯示列表斷言） |
| 3 表格／儲存格屬性 | M4.6 SetTableProps／SetCellProps | 部分完成：底紋（儲存格面）、對齊、寬度、垂直對齊已接；**邊框未接（引擎缺欄）**；**表格層底紋降標（引擎零發射）** |
| 4 圖片插入 | M5.1 InsertInlineImage | 完成（媒體位元組真的進引擎＋Image 顯示項＋存檔 `word/media/`） |
| 5 undo/redo | 全部上述變體 | 完成（逐變體顯示列表簽名逐位還原＋redo 回到套用態） |

---

## 1. 逐條交付：adapter 位置＋core 分支位置＋考卷紅/綠兩態

### 1.1 契約第 2 條：行列四變體

**adapter**（`shells/web/editor/js/engine-adapter.js`）

| 函式 | 行 | 對應引擎宣告 |
| --- | --- | --- |
| `buildTablePath(rootSlot, nestedSteps)` | 207 | `TablePath`（`mutation.rs:144-149`） |
| `buildInsertTableRowMutation(table, atRow, position)` | 271 | `InsertTableRow { table, at_row, position }`（`mutation.rs:302-306`） |
| `buildDeleteTableRowMutation(table, row)` | 285 | `DeleteTableRow { table, row }`（`mutation.rs:308`，單行宣告） |
| `buildInsertTableColumnMutation(table, atGridCol, position, widthPt)` | 299 | `InsertTableColumn { table, at_grid_col, position, width_pt }`（`mutation.rs:310-315`） |
| `buildDeleteTableColumnMutation(table, gridCol)` | 314 | `DeleteTableColumn { table, grid_col }`（`mutation.rs:317`） |
| 落地方法 `insertTableRow`／`deleteTableRow`／`insertTableColumn`／`deleteTableColumn` | 990／1013／1034／1060 | 經 `_applyTableMutation` → `apply_mutation_json` |

欄位名未猜：考卷 `[檢查 2]` 用括號深度解析器直接讀 `core/engine-api/src/mutation.rs` 的變體宣告逐欄比對（不是把欄名寫死在考卷裡——引擎改欄名考卷就會紅）。

**core 分支**（`shells/web/editor/js/qword-core.js`）

- 命令表：`U4_TABLE_ROW_COMMANDS`（106）、`U4_TABLE_COL_COMMANDS`（113）
- 分支：`onCommand` 內 `U4_TABLE_ROW_COMMANDS[cmdId] || U4_TABLE_COL_COMMANDS[cmdId]`（約 625 起），驗 `root_slot`／索引／`position`／`width_pt`，合法才 `_notify('table:row-col-mutate', …)`
- **刪除類不要求 `position`**：引擎 `DeleteTableRow`／`DeleteTableColumn` 不收這個欄位。這一條是瀏覽器實測抓回來的缺陷（第一輪把刪除鈕全部誤擋，日誌現 `handled=false, requirement=null`），考卷 `5.13` 已釘成回歸鎖。

**UI 落地**：`qword-ui.js` `handleTableRowCol`（927）→ `adapter.insertTableRow(position, { rootSlot, atRow })` 等。

**考卷兩態**（`scripts/u4_selfcheck.mjs`，於 repo 根實跑）

綠（真引擎）：
```
✓ M4.3 列操作：80 → 83（插列）→ 80（刪列，如實減少且回到基線）
✓ M4.4 欄操作：引擎讀回欄數 3 → 4 → 3（幾何由引擎決定，JS 端零推算）
```
紅（摘除接線，`[檢查 8]` 同執行內自動回塞）：
```
✓ 鑑別 remove-engine-call      → 紅（adapter 未透過 apply_mutation_json 送 mutation）
✓ 鑑別 remove-ui-handler       → 紅（UI 未接聽 table:row-col-mutate）
✓ 鑑別 remove-ui-adapter-call  → 紅（UI 未呼叫 adapter.insertTableRow（只 dispatch 事件＝假接線））
✓ 鑑別 remove-core-dispatch    → 紅（core 分支只記帳不匯流）
```

### 1.2 契約第 3 條：SetTableProps／SetCellProps

**adapter**：`buildSetTablePropsMutation`（332）、`buildSetCellPropsMutation`（352）、`normalizeHex6Fill`（226）、`normalizeVerticalAlign`（257）；落地方法 `setTableProps`（1084）、`setCellProps`（1122）。

**core 分支**：`U4_TABLE_PROPS_COMMANDS`（120）／`U4_CELL_PROPS_COMMANDS`（130）＋兩段獨立分支（表格面派 `table:props-apply`、儲存格面派 `cell:props-apply`）。

**已接且可驗證的效果（實測值）**

| 效果 | 引擎欄位 | 顯示列表真相 |
| --- | --- | --- |
| 儲存格底紋 | `CellPropsPatch.shading_fill` | 多一筆 `Rect`，`fill_argb = 0xFF00AAFF`（#00AAFF）；存檔 `word/document.xml` 含 `<w:shd w:fill="00AAFF">` |
| 表格對齊 | `TablePropsPatch.alignment` | 格內文字 x `77.4/177.4/277.4 → 152.9/252.9/352.9`（置中位移） |
| 表格寬度 | `TablePropsPatch.width_pt` | 顯示列表簽名變動（`width_pt=200`） |
| 儲存格垂直對齊 | `CellPropsPatch.vertical_align` | 列高 40pt 時格內文字 y `88.8 → 139.2`（Word 同口徑：列高大於內容高才可見） |

**未做項一：邊框（判定引擎缺欄，停手未妥協）**

具體證據（全部實跑，非推測）：

1. `core/engine-api/src/mutation.rs` 的 `TablePropsPatch`（162-174）與 `CellPropsPatch`（201-219）**零 border 欄位**；全檔 `grep` 無 `border` 欄位宣告。
2. 硬送臆造欄位 `border_pt`／`borders`／`border_color_rgb`／`tcBorders` → 引擎一律回 `ok=true`、`error=none`，而全頁 `Rule` 型別項目恆為 **0→0**（serde 靜默接受未知欄位，零副作用）。
3. `core/layout/src/table.rs` 無邊框發射路徑（`DrawItem::Rule` 於表格切片零產出）。

→ 若接邊框，外殼必然宣稱「成功」而畫面什麼都沒有＝假成功，違反誠實紀律。故 `setTableProps`／`setCellProps` **只放行引擎宣告過的欄位**，邊框參數回 `{ok:false, rejectedByShell:true}`。考卷 `[檢查 2]` 有反向鎖：一旦引擎補上 border 欄位，斷言會紅並要求重估。考卷 `5.8` 同時釘住「引擎對臆造欄回 ok=true 且零發射」這個前提（前提改變即紅）。

**降級項二：表格層底紋（引擎受理但零發射）**

實測：`SetTableProps { shading_fill: 'E06699' }` → `ok=true`、存檔 `document.xml` 確實含 `<w:shd w:fill="E06699">`，但顯示列表項目數 **80→80 零變化**（`Rect` 零發射）。同一個 `shading_fill` 放在 `SetCellProps` 就有 `Rect`。

→ 這層「XML 寫了、用戶看不見」不是外殼能補的（補就是 JS 自繪，違反「嚴禁平行另造排版器」）。外殼做法：命令照送（XML 層是真實寫入），但**不得宣稱可見效果**；考卷 `5.9` 斷言顯示列表零發射並在標籤面維持誠實（`warn` 於發射出現時提醒重估），`[檢查 4(d)]`／`[檢查 7]` 禁止把它掛成 live。發射缺口屬引擎發射面（`table_map.rs` 的 `tblStylePr` 條件底紋有路徑，`tblPr` 直接底紋無發射），建議 PM 另開引擎卡。

### 1.3 契約第 4 條：圖片插入

**adapter**：`buildInsertInlineImageMutation`（378，`media_bytes` 走位元組陣列＝引擎 `Vec<u8>` serde 形態）、`readImageNaturalSize`（399，讀 PNG／GIF／JPEG 檔頭中繼資料，非排版計算）、`insertInlineImage`（1183）。

**core 分支**：`insert:picture`（兩態：帶位元組→`image:inline-insert`；無位元組→`image:pick-request` 走既有 `#qword-file-input`）；`ENGINE_SUPPORTED_IMAGE_MIMES` 對齊引擎 `validate_media`（`mutation_k29c.rs:643-649`）。

**UI**：`handleInlineImageInsert`（1004）、`requestImageFilePick`（1029，`accept` 暫切圖片型別）、`initFilePipeline` 的 `imagePickMode` 分流（275-288）＋`restoreFileInputAccept`（1040，避免污染開檔路徑）。**未新增任何按鈕**，沿用既有 `#qword-file-input`（`index.html:440`）與 `insert:picture`（775）。

**考卷兩態**
```
✓ M5.1 InsertInlineImage：Image 項 0 → 1、像素 75 bytes（PNG 檔頭驗證）、存檔含 word/media/
✓ 圖片誠實失敗：偽造檔頭引擎拒絕（訊息回傳），非支援 MIME 外殼先擋
```
引擎誠實拒絕原文（偽造 `GIF89a…` 宣稱 `image/png`）：
`圖片或媒體格式不支援或超過容量上限: 圖片檔頭與 MIME 不符（宣告 image/png）` → `ok=false`，外殼改走 `recordCommandFailure` ＋錯誤彈窗，不靜默。

### 1.4 契約第 5 條：undo／redo

`edit:undo`／`edit:redo` 沿用既有 live 路徑；考卷逐變體斷簽名可逆：
```
✓ undo/redo：InsertTableRow 嚴格可逆（簽名逐位還原＋redo 回到套用態）
✓ undo/redo：InsertTableColumn 嚴格可逆
✓ undo/redo：SetCellProps(shading) 嚴格可逆
✓ undo/redo：SetTableProps(align) 嚴格可逆
✓ undo/redo：InsertInlineImage 嚴格可逆
```

---

## 2. 真瀏覽器閉環證據（用戶看見那層）

`python3 -m http.server 8421` 於 repo 根 → `/shells/web/editor/index.html`，`openDefaultSample()` 後經 `QWordShell.onCommand` 逐條派發，讀 `page_display_list` ＋ canvas 像素：

| 階段 | page_display_list | 畫布像素 |
| --- | --- | --- |
| 基線（樣本檔） | items 41（全 Text）、paras 14、tables 0 | hash `2157956449`、非白 127215 |
| `table:grid-select` 2×3 | items 45（+4 格段落）、paras 20、`tableShape` 讀回 2 列 3 欄 | — |
| `table:row-insert-below` | items 47、shape rows 3 | — |
| `table:row-delete` | items 45、shape rows 2（**如實減少**） | — |
| `table:col-insert-right` ＋ `table:col-delete` | shape cols `3 → 4 → 3` | — |
| `table:align-center` | 格內文字 x `72.0`（整表位移，見簽名） | — |
| `cell:shading #00AAFF` | items 46、`Rect` 1 筆 `fill=0xFF00AAFF`、`x=259 y=89 w=78 h=17` | hash `1168052406`、非白 123325、藍色素 2948 |
| `insert:picture`（40×24 PNG） | `Image` 項 1 筆（mime `image/png`、30×18pt、base64 像素 236 字元） | 紅色素 816 |
| 兩次撤銷（表格＋底紋各一筆，經 `edit:undo`） | items 回 41、Rect 0、paras 14、tables 0 | hash 逐位回 `2157956449`、非白回 127215 |

撤銷鏈實際時序（如實記，避免誤讀成「一筆撤銷還原全部」）：先插表再上底紋（兩筆 mutation），第一筆撤銷走 `adapter.undo()` 直連（顯示列表 Rect 1→0，但畫布不重繪——重繪屬 UI 路徑，非缺陷），第二筆撤銷走 UI 的 `edit:undo`（觸發 reflow），畫布 hash 逐位回基線 `2157956449`、非白像素回 127215。

截圖（同一 viewport，含 Ribbon／尺規／狀態列的真實編輯器）：
- `u4_evidence_before.png`（無表格）
- `u4_evidence_after.png`（置中表格＋藍色底紋儲存格可見）

非法參數的誠實態（瀏覽器實測）：`cell:shading` 帶 `fill:'ZZZZZZ'` → `handled=false`、未觸引擎、顯示列表零變化、指令日誌留下拒絕痕。

---

## 3. 白名單與禁區

`git diff --name-only main...HEAD`（本卡新增／修改，全部在白名單內）：

```
scripts/u4_selfcheck.mjs            （新增，考卷）
shells/web/editor/js/engine-adapter.js
shells/web/editor/js/qword-core.js
shells/web/editor/js/qword-ui.js
docs/word-ui-u4-wiring-report.md    （本報告）
```

- `git diff main...HEAD -- core/` 為空：**core/ 零觸碰**（欄位名一律用讀的）。
- `shells/web/editor/index.html`：**零觸碰**（PM-owned）。
- `shells/web/editor/vendor/*`：零觸碰 → 未改繪製器，無需 SYNC.md 登記（繪製器本已支援 Rect／Rule／Text／Image 四分支）。
- 無 `push`／`checkout`／`reset`／`rebase`／`stash`／`clean`／`worktree` 操作。

### PM 需補的 Ribbon 入口（本卡已把引擎與外殼接好，缺按鈕）

考卷 `[檢查 7]` 逐項點名，目前 1/18 有入口（僅 `insert:picture`）：

- 行列：`table:row-insert-above`、`table:row-insert-below`、`table:row-insert`、`table:row-delete`、`table:col-insert-left`、`table:col-insert-right`、`table:col-insert`、`table:col-delete`
- 表格屬性：`table:shading`（建議先不掛 live，見 §1.2 降級項）、`table:align-left`、`table:align-center`、`table:align-right`、`table:width`
- 儲存格：`cell:shading`、`cell:v-align-top`、`cell:v-align-center`、`cell:v-align-bottom`

另點名一顆本卡判定**不應轉 live** 的既有鈕：無邊框鈕存在，但若 PM 打算新增「表格邊框」鈕，在引擎補 `tblBorders`／`tcBorders` 寫入面＋發射之前，任何標籤都必須是引擎面缺口（非 `pending:shell`）。

`insert:picture` 現掛 `pending:shell`（`index.html:775`），本卡已完成真接線，**建議 PM 轉 live**（考卷 `[檢查 4]`＋`5.10` 已備好三查依據）。

### 外殼面缺口（誠實標註，非妥協）

- `root_slot` 解析：wasm 殼未綁 `table_count()`／`tables_json()`（`shells/web/src/lib.rs`），也無「游標 → 表格索引」反查。外殼用引擎自己的誠實拒絕訊息做**零副作用探針**（探針五／八已證不入 undo 堆），解析順序為「呼叫端指定 → 本 session 最近操作那張 → 文件只有一張表時用 0」，三者皆不可得時彈窗如實回報，不猜、不默默改到別張表。多表文件的「點哪張改哪張」需要引擎綁定 `table_map` 查詢，建議另卡。
- `nested_steps`（巢裝表格）欄位形態已在 `buildTablePath` 支援，但 UI 目前只操作根表格（同樣缺幾何反查綁定）。

---

## 4. 自檢實跑（repo 根）

```
$ node scripts/u4_selfcheck.mjs          → 自檢結果：PASS（警告 17 項＝PM 決策面入口缺口）
$ node scripts/wordui_u1_selfcheck.mjs   → exit 0  恭喜！…(U1 窗體骨架) 通過所有規格與紅線檢查。
$ node scripts/u2_selfcheck.mjs          → exit 0  恭喜！AGY-U2 真引擎接線與打字閉環通過所有規格與紅線檢查。
$ node scripts/u3_selfcheck.mjs          → exit 0  恭喜！AGY-U3 樣式命令啟用…通過所有規格與紅線檢查。
```

U1／U2／U3 三份本卡**未修改**，只重跑確認未回歸。考卷 `5.x` 的動態斷言全部經真 wasm（`shells/web/pkg/`，gitignored，自 main repo 硬鏈結）。

考卷自身設計要點：`[檢查 8]` 在**同一次執行內**把接線各別摘除（引擎呼叫／UI 接聽／UI 呼 adapter／core 匯流／邊框假接線）重跑靜態鎖，五態全部轉紅，且跑完驗證真樹來源字串未被污染——「未接線態必紅」是考卷自己證明的，不需要手動摘除再截圖。

---

## 5. 未做項與後續建議（照實列，無佔位冒充）

1. **表格／儲存格邊框**：未做。引擎缺欄（§1.2 證據三條）。要清償需引擎補 `w:tblBorders`／`w:tcBorders` 寫入面＋`core/layout/src/table.rs` 的 `Rule` 發射。
2. **表格層底紋的可見性**：接了但無可見發射，標籤保持非 live。要清償需引擎發射面讀 `tblPr/w:shd`。
3. **多表文件目標解析**：需引擎綁定表格索引查詢（§3 缺口）。
4. **巢裝表格操作**：欄位就緒，UI 未接（同 3 的綁定缺口）。
5. `references:insert-figures-table`／`references:update-figures-table` 兩顆鈕與本卡表格無關（圖表目錄，引擎未入庫），未觸碰。

## 6. 繁中純度

diff 新增行以 opencc s2t 逐字元比對＋碼位掃描（623 個唯一 CJK 字元）。共修掉 **5 處**真實簡體洩漏（本節由後續 run 補正——首版宣稱「一處」為不實，實際掃描抓出五處）：

| 檔 | 位置 | 洩漏碼位 | 修正碼位 |
| --- | --- | --- | --- |
| `qword-core.js` | U4 指令表註解「core 分支?什麼」 | U+9A8C | U+9A57（驗） |
| `qword-core.js` | `pickIndex` 註解「第?個」 | U+4E2A | U+500B（個） |
| `engine-adapter.js` | `buildSetTablePropsMutation` 註解「?默忽略」 | U+9759 | U+975C（靜） |
| `engine-adapter.js` | 表格路徑解析註解「最?一張」 | U+540E | U+5F8C（後） |
| `word-ui-u4-wiring-report.md` | §1.2 降級項「缺口?引擎發射面」 | U+5C5E | U+5C6C（屬） |

opencc 另報三筆**同形字誤報**，依 skill 白名單原則**不改**：`蒙`（「蒙混過關」為正體用法，`矇` 僅用於失明義）、`准`（「不准只畫佔位框」之「不准」為正體用法，`準` 用於標準／依準義）、`后`（白名單字）。

本報告依紀律以 U+ 碼位描述洩漏字、不貼簡體字面，以免污染後續掃描。零 emoji（考卷輸出的 `✓`／`✗` 為 U+2713／U+2717 幾何符號，非 emoji 區段）。
