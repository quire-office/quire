# U4c：表格 Ribbon 入口 17 顆——驗收報告

任務：`t_605866b6`（U4c）｜分支 `wt/u4c-ribbon`｜基線 U4 合併態 `37ffbbb`
署名：ADT109119（專案署名）｜語言：繁體中文

## 0. 一句話總結

U4 已接好引擎＋adapter＋core 命令分支的 18 顆表格／儲存格命令，本卡把缺的
17 顆 Ribbon 按鈕補齊：**16 顆經真瀏覽器閉環實證轉 `live`**，
`table:shading` 依引擎發射缺欄**誠實掛 `pending:engine-tblshading`（未假 live）**；
每顆 live 鈕皆有 display-list 指紋位移＋undo 逐位還原實據，U1／U2／U3／U4b
四份自檢 repo 根全綠。

## 1. 交貨對帳：19 顆 `<button>` = 17 命令鈕 ＋ 2 輔助鈕

PM 記帳點名「index.html 實數 `<button>` 19 個」，與卡面「17 顆」的口徑差在此說清：

| 類別 | 顆數 | data-command |
|---|---|---|
| 行列操作 | 8 | `table:row-insert-above`／`row-insert-below`／`row-insert`／`row-delete`／`col-insert-left`／`col-insert-right`／`col-insert`／`col-delete` |
| 表格屬性 | 5 | `table:shading`（**pending**）／`table:align-left`／`align-center`／`align-right`／`table:width` |
| 儲存格 | 4 | `cell:shading`／`cell:v-align-top`／`v-align-center`／`v-align-bottom` |
| **命令鈕小計** | **17** | 全部 `data-command` 對齊 `js/qword-core.js` 既有命令名，**零另造命令** |
| 輔助鈕 | 2 | 表格屬性面板觸發鈕（`data-popover-target="popover-table-props"`，無 data-command）＋儲存格底紋「清除底紋」鈕（`data-cell-shading-clear`，走 `cell:shading` fill=auto 路） |

`u4c_ribbon_probe` 的 `notes.inventory.uniqueCommands` 實測 17 個唯一命令鈕全在位、
`auxButtons` 兩顆皆 true，對帳一致。探針證據見
`docs/word-ui-u4c-ribbon-evidence.json`，同場截圖 `…-evidence.png`。

## 2. 誠實標籤：16 live ＋ 1 pending（零假 live）

| 標籤 | 顆數 | 背書 |
|---|---|---|
| `live` | 16 | 下方 §3 逐顆 display-list 閉環＋undo 實據 |
| `pending:engine-tblshading` | 1（`table:shading`） | 引擎 `MutateTable.set_tbl_props` 的 `w:tblPr/w:shd` 寫入真實落盤，但 `core/layout` 色塊發射僅唸 `w:tcPr/w:shd` 單格路徑——表格層底紋重排後顯示列表**零發射**（實測），用戶看不見，依誠實紅線不準標 live |

`table:shading` 誠實性三層證據（探針 `buttons["table:shading"]`）：
1. 鈕 `data-requirement` 確為 `pending:engine-tblshading`；
2. 點擊（含直接帶 fill 下單）後 display-list 指紋零變化（`directFpChanged:false`）；
3. 存檔面 `word/document.xml` 真的寫入 `w:shd w:fill="E06699"`（`xmlHasShd:true`）——
   證明「引擎受理層≠發射層」，缺口在發射層（E 卡同源），非外殼可修。

## 3. 瀏覽器閉環證據鏈（U4b 口徑：用戶看見那層）

**方法**：`python3 -m http.server 8791`（repo 根）＋Chrome CDP 9222 開專用 tab。
每顆鈕與用戶操作同形：游標釘進目標格（`table_locator_json` 反查）→ Ribbon
**真 DOM `btn.click()`**（非 onCommand 捷徑）→ `bindEvents`→`_tableCommandPayload`
（座標全由引擎給，外殼零猜測）→ `core.onCommand` 驗證→UI handler→adapter→
`apply_mutation_json`→引擎重排→**`page_display_list` 逐項指紋**
（型別＋幾何＋fill/color）比對→`edit:undo` 還原。任一段斷掉指紋 delta 即零。

**探針誠實性修正**：首版在「插入」分頁隱藏＋面板未開狀態下直接 `click()`，
事件鏈雖同但 `visible=false` 不是「用戶看得見」那層的證據。補齊
`tab:switch`（行列／屬性鈕切插入頁、v-align 切常用頁）＋開表格屬性面板＋
底紋面板重開再點清除鈕後重跑，**命令鈕 `visible` 全 true**（commit 56ecd88）。

**18 站全 PASS**（`docs/word-ui-u4c-ribbon-evidence.json`；基準＝開檔＋Ribbon
插入 3×3 表，baseline 83 項）：

| 鈕 | 閉環實據 | undo |
|---|---|---|
| row-insert-above／below／`row-insert` | 文字項 83→86（新增列 3 格段落）、shapeΔ rows=+1 | 逐位還原 ✓ |
| row-delete | 83→80、shapeΔ rows=−1 | ✓ |
| col-insert-left／right／`col-insert` | shapeΔ cols=+1 | ✓ |
| col-delete | shapeΔ cols=−1 | ✓ |
| align-center | 表格 Text 項 x 77.4→152.9（整表右移置中） | ✓ |
| align-right | 基準 Text x 77.4→右移（整表靠右） | ✓ |
| align-left | **right→left 拉回**：Text x 228.4→77.4，`backToBaselineFp:true`（left=預設態，單點零位移屬 Word 同語義，故走拉回路徑取位移實據） | ✓ |
| table:width | input 填 **120pt** 真點擊，指紋變（寬度重排） | ✓ |
| cell:shading | 面板色塊真點擊，Rect **0→1**（`R\|172.00\|105.60\|100.00\|16.80\|ffffffff`） | ✓ |
| 清除底紋 | 套色後 `fill=auto`，Rect **1→0 回 baseline 指紋** | — |
| v-align-bottom／center | 固定列高 40pt 前置下格內 Text y 位移（105.60→139.20） | ✓ |
| v-align-top | **bottom→top 拉回**（top=引擎預設態，單點零位移屬 Word 同語義） | ✓ |
| table:shading | §2 誠實三層（指紋零發射，符合 pending 標籤） | — |

**Undo 總體**：探針收尾 `edit:undo` 逐筆排回 baseline 指紋，
`finalDrain.restoredToBaseline:true`——所有操作可逆、零殘留。

## 4. 四份自檢（repo 根，本分支 HEAD 56ecd88）

| 自檢 | 結果 |
|---|---|
| `wordui_u1_selfcheck` | **PASS**（新增「儲存格」群組使 home 群組 5→6，U1 斷言採硬寫期望清單、不計額外群組，故不紅） |
| `u2_selfcheck` | **PASS** |
| `u3_selfcheck` | **PASS** |
| `u4b_selfcheck` | **PASS**（檢查 10 顯示級提示「鈕數 196→215」——本卡＋U4b 新增鈕所致，`ok()` 為顯示級、不計 FAIL，總判 PASS） |
| `u4_selfcheck` | **[檢查 7] Ribbon 入口 18/18、缺 0 項**（本卡核心訊號，main 基線此處點名「缺 17」）；整體 FAIL 4 項**全為邊框判定基線漂移**，見 §5 |

## 5. U4 紅燈基線說明（非本卡造成，勿誤讀為回歸）

`u4_selfcheck` 的 4 項 ✗ 皆為同一根因：引擎 E 卡已補 `tblBorders`／`tcBorders`
寫入欄（`mutation.rs` 現含 `borders` 欄），U4 舊考卷仍斷言「引擎零 border 欄」
→ 前提失效變紅。**main（d6a6af1 之後）跑同一支 u4_selfcheck 亦紅 3 項**（本
分支多第 4 項「探針前提失效」，因 worktree `pkg/` 為 Nov-21 新建置、E 卡發射
已上，探針實測 `Rule 0→5` 發射成功、與「臆造欄零發射」舊斷言矛盾）。
此為 **E 卡（表格邊框）合併後 U4 考卷需同步重估**的既有待辦，與本卡「補
Ribbon 按鈕」無關，本卡白名單嚴禁碰 `core/` 亦不觸碰 scripts/ 考卷。建議由
PM 開 U4 考卷邊框重估卡給 ow-render／PM，或將 E 卡 redlight 的邊框判定更新
同步進 `u4_selfcheck`。

## 6. 紀律查核

- 白名單：只改 `shells/web/editor/index.html`、`css/style.css`、`js/qword-core.js`、
  `js/qword-ui.js`（必要 js）＋本報告與證據檔（`docs/word-ui-u4c-*`）。
  `git diff 37ffbbb HEAD -- core/` = **0 行**（嚴禁碰 core/ 達標）。
- **白名單偏離（主動如實報）**：新增探針落在
  `shells/web/editor/tests/u4c_ribbon_probe.mjs`——`tests/` 未明文寫在卡面白名單。
  沿 U4b 先例（1bf87fd 將 `tests/u4b_e1_probe.mjs` 同位置入庫、PM 合併並寫明
  「證據檔歸位白名單」），驗收工具與證據同源、可重放。PM 合併時若認定該路徑
  越界，直接 `git rm` 該檔即可，報告與證據 json/png 不依賴它在場。
- 繁中純度：diff 新增行 opencc s2t 回譯，僅「群」2 處（opencc 對「群」的已知
  誤報，繁中標準字形即「群」），無簡體洩漏。
- 與 U4b 同改 index.html 的調度：起手 `git log -3 -- index.html` 確認 U4b 已
  合併進 main（37ffbbb 為 U4b 合併態），無活體分支在改，安全接手。
- commit 里程碑（嚴禁 push，未 push）：`2665e6d`（PM 代保護半成品）→
  `8574765`（探針入庫＋18 站全綠）→ `56ecd88`（探針誠實用戶路徑修正）。
- run 289 半成品（PM 保護 commit `2665e6d`）**逐項獨立查證後沿用，未重寫、
  未 reset/checkout**（鐵律遵守）。

## 7. 交接

- **鈕數對 U4b 維護者**：U4b `u4b_selfcheck` 檢查 10 硬釘 `196`，U4c 新增命令鈕
  使實際 `data-requirement` 鈕增至 **215**（196 為 U4b 收案時態，含 U4b 自身
  remap 增量）。U4b 維護者下次把該鎖更新為 215（或改區間斷言）。
- **給 PM 的 U4 考卷待辦**：§5 邊框判定重估（E 卡合併後 `u4_selfcheck` 需同步）。
- 收尾：背景 `http.server:8791`（本 run 起，供探針）可停。
