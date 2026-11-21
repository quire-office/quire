# U4b：K29e 入庫標籤失效事件——返修與接線報告

任務：t_b9ae819b（分支 `wt/u4b-k29e-remap`）
執行：ow-render（run 286，接續 run 282/283 半成品）
基準：main `090ab1b`（含 U4 合併 `d6a6af1`、U6 圖示換裝 `9af0be2`）
日期：2026-11-21

---

## 0. 結論速覽

| 驗收項 | 結果 |
|---|---|
| `pending:K29e` 歸零（data-requirement 屬性面） | **0 殘留**（index.html 內 7 處 `K29e` 字樣全為 title／圖例說明文字，非標籤） |
| `format:underline`／`strikethrough`／`subscript`／`superscript` | **live**（瀏覽器閉環實據見 §3） |
| `format:highlight`／`highlight-palette`／`font:family`／`font:clear-formatting` | **live**（同上） |
| `format:underline-style` | **pending:engine-para**（引擎解析＋發射雙層缺口，實測坐實，禁假 live，見 §4） |
| `insert:picture` | **live**（PM 11-13 補正裁決 1：依 U4 合併 `d6a6af1`＋報告 `word-ui-u4-wiring-report.md` §3） |
| 鈕數守恆 | data-requirement 196 顆守恆；控件（button/select/input）203 個逐位守恆；`data-command`＋`aria-label` 多重集與 main 逐位相同 |
| U1／U2／U3／U4／U4b 自檢 repo 根 | **5/5 全綠**（exit 0，本 run 自行重跑） |
| 白名單 | 全部改動落 `shells/web/editor/`、`scripts/u*_selfcheck.mjs`、`scripts/u4b_label_remap.mjs`、`docs/word-ui-*`；`core/` 零觸碰 |

---

## 1. 前 run 半成品處置（run 282/283 → run 286）

PM 值守代 commit 的 `8d6afae`（非驗收態保護 commit）經本 run 逐項獨立查證後**全數沿用，未平行重寫**：

1. **查一（JS 端）**：`qword-core.js` 命令分支（526-564 行域）、`qword-ui.js` 落地層（763-845 行域 `handleFormatUnderline` 等六處理函式）、`engine-adapter.js` `buildFormatTextRangeMutation`（236-278 行域）三側真實存在且互相銜接，非空殼。
2. **查二／查三（引擎面）**：本 run 實讀 `core/engine-api/src/mutation.rs` `RunDirectPatch`（122-174 行）逐欄比對——`bold/italic/color_rgb/size_pt/char_style/underline/strike/vert_align/highlight/font_family` 十一欄宣告齊備，serde 形態（枚舉變體名／0xAARRGGBB 數值／字串）與 adapter 建構器註解逐欄一致。
3. **動態面**：本 run 自行重跑 u1/u2/u3/u4/u4b 五份自檢全綠；重跑 `shells/web/editor/tests/u4b_e1_probe.mjs`（路徑修正後）輸出與前 run 宣稱一致。
4. **純度面**：前 run 宣稱未經查證，本 run 以 opencc s2t 逐字元掃描全 diff 新增行（§7）。

本 run 增量工作：瀏覽器閉環證據鏈（§3）、remap 工具可重放性驗證（§2）、頁尾圖例表與實況同步（§2.3）、`format:*` 家族逐鈕表補齊與 evidence 落庫（附檔 `word-ui-k29e-remap-evidence.csv`）、E-1 引擎發現的瀏覽器層複現（§5）、本報告。

## 2. 項二：標籤重掛（65 顆 pending:K29e＋9 顆相關鈕）

### 2.1 工具與方法（禁字串批次全替）

`scripts/u4b_label_remap.mjs`：以 `data-command` 值為單位逐顆比對映射表，命中才改 `data-requirement`＋`title`；映射缺項跳過不改；每顆寫 evidence 列（命令｜舊標籤｜新標籤｜三查依據）；前後統計鈕數守恆，不守恆即 exit 1。

**可重放性證明（本 run 實跑）**：`node scripts/u4b_label_remap.mjs --html <git show main:index.html> --evid <tmp>` 從 main 基準樹重放，產出與當前 `index.html` 逐位相同（唯一差異為頁尾圖例表 `<td>` 說明文字 6 行，非 data-requirement 屬性；該 6 行本 run 已按實況修正，見 §2.3），且重放 evidence CSV 與入庫 `docs/word-ui-k29e-remap-evidence.csv` diff 為空。即：**目前標籤態是同一工具＋同一映射表從 main 可確定的函數，非手改**。

### 2.2 逐鈕分布（75 筆變動，依據全文見附檔 CSV）

舊標籤面：`pending:K29e` 65、`pending:K29b` 9、`pending:shell` 1。
新標籤面（依缺口族）：

| 新標籤 | 顆數 | family 分布 | 三查依據摘要（逐鈕全文在 CSV） |
|---|---|---|---|
| `live` | 9 | format: 7、font: 2 | 三查全備：欄位宣告＋patch_rpr 寫入臂＋外殼真呼叫；閉環實據 §3 |
| `pending:ext` | 5 | review: | 詞庫／翻譯屬外部資源；mutation*.rs 無缺口（落地走既有 M1.1/M1.2） |
| `pending:shell` | 4 | insert:、references: 等 | 引擎欄位真實存在（M6 註腳族等），缺口純外殼面板／游標邏輯 |
| `pending:engine-drawing` | 7 | dialog:、layout: | z 序／群組／旋轉／自訂繞排點：mutation*.rs `text_wrap` 0 命中，M5.2 僅尺寸／定位／繞排模式 |
| `pending:engine-page` | 8 | design:、layout: | `pgBorders`／`w:background`／`lnNumType`／`autoHyphenation` 於 mutation*.rs 0 命中；`w:gutter` 僅 sectPr 樣板字面量硬值非可寫欄 |
| `pending:engine-theme` | 7 | design: | theme1.xml／docDefaults 無寫入變體（僅只讀解析） |
| `pending:engine-para` | 2 | design:、format: | `w:bidi` 無欄；底線形態族寫入真、解析折布林＋發射一態（§4） |
| `pending:engine-refs` | 18 | references:、insert: | `fldSimple`／`instrText` 0 命中（僅只讀投影） |
| `pending:engine-review` | 15 | review:、dialog: | `w:ins`／`w:del`／`w:comment`／`documentProtection`／`w:lang` 於 mutation*.rs 0 命中 |

**PM 11-13 補正三項裁決的落實**：
1. `insert:picture` 轉 `live`——依 U4 合併 `d6a6af1`＋`word-ui-u4-wiring-report.md` §3（U4 worker 實測閉環），CSV 有列。
2. 17 顆 `table:`／`cell:` Ribbon 入口：**實掃 index.html `data-command="table:`／`cell:` 為 0 命中——入口按鈕不存在，無鈕可掛標籤**。依裁決「不要在 JS 端自造入口」，本卡未造鈕；缺口由 `scripts/u4_selfcheck.mjs` 警告清單持續追蹤（17 項清單本 run 重跑復現，點名清單：table:row-insert-above／table:row-insert-below／table:row-insert／table:row-delete／table:col-insert-left／table:col-insert-right／table:col-insert／table:col-delete／table:shading／table:align-left／table:align-center／table:align-right／table:width／cell:shading／cell:v-align-top／cell:v-align-center／cell:v-align-bottom）。本報告列為 PM 決策點（§8）。
3. 三張引擎卡另開 backlog：本卡 `core/` 零觸碰，紀律遵守。

### 2.3 頁尾圖例表同步

index.html「能力總覽」表的「依賴卡片」欄為說明文字（非 data-requirement）。本 run 逐 tab 實掃分布（tab-home 有 engine-para、tab-insert 無 engine-page、tab-design/layout/review 有 shell）後修正 5 個 `<td>` 與實況一致，防「圖例宣稱過時」型誠實缺陷。

## 3. 項一：format 家族接線與瀏覽器閉環證據鏈

### 3.1 接線面（欄位逐欄對齊 mutation.rs 實讀）

`buildFormatTextRangeMutation`（`engine-adapter.js`）消費 `RunDirectPatch` 全十一欄；K29e 五欄序列化形態：

| 欄位 | JS 端正規化 | 送引擎形態 | 引擎宣告（實讀行域） |
|---|---|---|---|
| underline | `normalizeUnderlineKind` | `'Single'|'Double'|'Dotted'|'Dashed'|'None'` | `Option<UnderlineKind>`（141） |
| strike | 同上 | `'Single'|'Double'|'None'` | `Option<StrikeKind>`（145） |
| vert_align | `normalizeVertAlign` | `'Superscript'|'Subscript'|'Baseline'` | `Option<quire_ooxml::VertAlign>`（151） |
| highlight | `parseColorArgb` | 0xAARRGGBB 數值（alpha=0＝明確移除） | `Option<u32>`（163） |
| font_family | 字串（空字串＝明確移除） | 字串 | `Option<String>`（173） |

未帶欄位一律 `null`（引擎 `#[serde(default)]`＋`Option`＝不碰），不憑空寫入。

### 3.2 瀏覽器閉環（真瀏覽器＋真 wasm＋真 UI 落地路徑）

方法：`python3 -m http.server` 於 repo 根（127.0.0.1:8431），開 `/shells/web/editor/index.html`。每條探針與用戶操作同形：寫 `adapter.selection`（pointer-drag 落地形態）→ `window.QWordShell.onCommand(cmd, payload)`（走 Ribbon 同一 dispatch 鏈）→ 量 `page_display_list` Text 旗標／幾何＋canvas 像素 hash → `onCommand('edit:undo')` 還原。原始輸出存 `u4b_browser_batch1.json`（本 run workspace，關鍵數值全錄下方）。

基線態（開檔後）：canvas hash `877248646`、非白像素 `127658`、前 6 行 y `[72, 88.8, 105.6, 122.4, 139.2, 156]`。

| 鈕（轉 live） | display-list 翻轉 | canvas hash 前→後→undo | 逐位還原 |
|---|---|---|---|
| `format:underline` | 底線行 0→1→0 | 877248646→3119406458→**877248646** | 是（非白像素 127658→127735→127658） |
| `format:strikethrough` | 刪除線行 0→1→0 | 877248646→3218157070→**877248646** | 是（127658→127726→127658） |
| `format:subscript` | 折行 size 12→6.96→12（0.58×）；受標行 y 88.8→89.891（**+1.0909pt**）；其後各行 y 逐位不動 | 877248646→1788971909→**877248646** | 是 |
| `format:superscript` | 折行 size→6.96；y 88.8→84.6（**−4.2pt＝−0.35×base**）；其後各行 y 逐位不動 | 877248646→3644640681→**877248646** | 是 |
| `format:highlight` | 底方塊 Rect 項 0→1→0（fill 0xFFFFFF00，u4b 檢查 5 同口徑） | 877248646→3678269089→**877248646** | 是 |
| `font:family` | Text.family 集合 ['—']→['—','Arial']→['—']；測寬推進致全頁行 y 位移（88.8→102.8 起算，斷行幾何變動） | 877248646→902208338→**877248646** | 是 |
| `font:clear-formatting` | 疊加態（底線＋刪除線＋上標＋醒目提示＋族名）一次清除全歸零，寫明確關閉宣告 | 902208338(疊加態)→1750881744→還原 | 是 |

上下標斷言依 `docs/k29e-redlight.md` §2 LO 真值：0.58×字級、上標 −0.35×base、下標 +base/11（11pt 時 +1.0pt）、**行高不膨脹**——受標行之外各行絕對 y 逐位不動（pitch 恆定），卡面「改行高」舊前提已被探針推翻的結論在瀏覽器層同樣成立。

存檔閉環（瀏覽器端）：套底線→`save()` 匯出 1318 bytes ZIP→同瀏覽器 `openFile` 重開→底線行仍 1（旗標經 XML 往返存活；XML 面 `w:u w:val="single"` 斷言在 u4b_selfcheck 檢查 2，deflate 解壓後搜尋）。

視覺證據：`u4b_evidence_before.png`（無底線）、`u4b_evidence_underline_on.png`（para1「這是一份」四字底線繪出，用戶看見那層）、`u4b_evidence_undone.png`（底線消失）。三張 hash 與上表一致。

### 3.3 假 live 回塞鎖

`scripts/u4b_selfcheck.mjs` 檢查 9：core 命令層逐鈕 `handled=true＋requirement='live'`；`format:underline-style` 必須 `handled=false` 且不沿用 live、失敗如實進 commandLog。u3 自檢另鎖「六欄族必 live＋清單族必 pending＋underline-style 准 pending 不准 live」三組雙態斷言（一字未弱化，僅按 K29e 入庫事實精確收窄）。

## 4. underline-style 誠實降標依據（pending:engine-para）

- 查一：JS 無形態選擇面板入口（僅 split-button 主鍵）。
- 查二：`UnderlineKind` 五變體宣告真實存在。
- 查三（斷點實讀＋實測雙證）：`core/ooxml/src/styles.rs` `b"u" => rs.underline = Some(toggle_on_attr(e))`——`w:u/@w:val` 形態不進 RunStyle，發射層僅 `FLAG_UNDERLINE`(4) 一態。u4b 檢查 8 實測：單條／雙條／點線三種 `w:u/@w:val` 真的進 XML，渲染端 flags 全等（4）、幾何全等；存檔重開後雙條仍只是 FLAG_UNDERLINE(4)。用戶看見那層零差異→掛 live 即假宣稱。引擎面缺口（解析＋發射兩層），比照 U4 邊框先例列引擎卡。

## 5. 引擎發現（回報不修，core/ 零觸動）

**E-1（新，本 run 瀏覽器層坐實）**：`RunDirectPatch` 的 `patch_rpr` 整段重寫受標 run 的 rPr——未帶欄位（null）不從既有 rPr 繼承。外殼每次鈕點擊都是單欄 mutation，於是**同選區先點底線、再點粗體，底線消失**：瀏覽器實測 `{u:1,b:1} →（點粗體）→ {u:0,b:2}`；node＋真 wasm 探針 `tests/u4b_e1_probe.mjs` 同現（呼叫一 bold flags=1，呼叫二 italic 後 flags=2 非 3，受標 run XML 只剩最後一欄）。同一次呼叫多欄齊發則正確（flags=15）。

- 對本卡 live 標籤的影響判定：**各鈕單獨「開／關」語義閉環全數真實（§3.2 逐條 hash 逐位還原），單鈕誠實性成立**；缺口是「多欄疊加保持」屬引擎 patch 語義層（寫入前應讀既有 rPr 或引擎側合併），非外殼可修，且與 K29e 卡面「六欄同發」口徑相容。列 PM 決策點（§8）。
- **E-2（沿用前 run，本 run 復跑確認）**：underline 形態解析折布林＋發射一態（§4）。
- **E-3（U4 遺留，非本卡範圍）**：邊框寫入＋Rule 發射、tblPr 直接底紋發射、table_map 索引綁定——PM 已另開 backlog。

## 6. 自檢重放記錄（repo 根，本 run 自行重跑，exit code 實錄）

| 自檢 | 結果 |
|---|---|
| `scripts/wordui_u1_selfcheck.mjs` | PASS exit 0（未修改） |
| `scripts/u2_selfcheck.mjs` | PASS exit 0（前 run 已把 underline 斷言精確收窄成 pending|live 雙態，無弱化；本 run 重跑） |
| `scripts/u3_selfcheck.mjs` | PASS exit 0（K29E_LIVE_BUTTONS 必 live＋清單族必 pending＋underline-style 禁 live＋檢查 5.5 上下標 LO 真值閉環） |
| `scripts/u4_selfcheck.mjs` | PASS exit 0（17 項 Ribbon 入口警告＝PM 決策面，非阻斷） |
| `scripts/u4b_selfcheck.mjs` | PASS exit 0（檢查 1-10：序列化契約＋五家族閉環＋LO 真值＋回塞鎖＋K29e 歸零＋鈕數守恆 196） |
| `shells/web/editor/tests/u4b_e1_probe.mjs` | 實跑輸出如 §5（前 run 在 repo 根的暫存探針，本 run 依白名單遷入 editor/tests/ 並修正相對路徑） |

## 7. 繁中純度

全 diff 新增行（程式碼＋報告）以 opencc s2t 逐字元比對掃描，白名單同形字（稽核／台／了／干／吃／于／才／回／藉／游／岩／晒／峰／蒙／准／后）放行；報告內不貼簡體字面。結果見 commit 訊息與本節（掃描輸出錄於 workspace `u4b_purity_out.txt`）。

## 8. PM 決策點

1. **E-1 引擎卡**：`patch_rpr` 整段重寫致跨呼叫欄位互相覆蓋（同選區「底線後點粗體→底線沒了」）。用戶可見行爲缺陷；外殼規避（每次呼叫先讀既有態再合併）可做但屬引擎語義，建議開引擎卡「RunDirectPatch 欄位級合併（null＝繼承既有 rPr 宣告）」。
2. **17 顆 table:/cell: 入口鈕**：index.html 零存在（實掃 0 命中），非標籤問題而是按鈕缺失；本卡依裁決未自造入口。需補鈕卡（掛 live，U4 閉環已備）。
3. **underline-style 引擎卡**：解析層 `w:u/@w:val` 保形態＋發射層多態繪製（§4）。

## 附檔

- `docs/word-ui-k29e-remap-evidence.csv`——75 筆逐鈕表（命令｜舊標籤｜新標籤｜三查依據），與 remap 工具重放輸出 diff 為空。
