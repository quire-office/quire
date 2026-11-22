# U4e 外殼返修報告：巢狀表跨層守衛＋換檔/Undo 狀態殘留＋路徑型別強制

卡面：t_74458fe2（分支 `wt/t_74458fe2`，基線 a33944e）
來源：agy（gemini-4-argon-high）對 f83f70d..4c0d532 批次獨立審查 × PM 讀碼/實跑覆核。
紀律：先紅後綠＋真引擎實錄＋自我鑑別（回塞態必紅）。**不碰 `core/`、
`scripts/u4_selfcheck.mjs`／`u4d_selfcheck.mjs`、`shells/web/editor/index.html`。**

## 驗收總覽

| 項目 | 結果 |
|---|---|
| `scripts/u4e_selfcheck.mjs`（新，四盞燈＋前提＋鑑別） | 修前 **4 盞全紅（55 項違規）** → 修後 **全綠** |
| U1／U2／U3／U4／U4b／U4d 六考卷（merged 態） | 全綠 |
| `bash ~/.hermes/scripts/ow-gate.sh <worktree>` | **fail=0**（八閘全 PASS） |
| 自我鑑別（七種回塞態） | 全紅（摘除任一修復對應燈必紅） |
| agy 第二意見代碼審查 | 見文末「第二意見」節 |

### 環境注記（非本卡改動）

`shells/web/pkg/` 為 gitignore 建置產物。worktree 與主幹的既有 `pkg/`（11-22 05:54）
**早於** t_c2bbcb38 的 Rust 修復合併（11-22 10:34），是過期產物——u4d 考卷檢查 5 的
DL 命中斷言在過期 wasm 下必紅（主幹直接跑 u4d 亦紅，非本卡引入）。本卡以
`wasm-pack build --target web --out-dir pkg --release`（`shells/web/`，即
package.json 的 `build:wasm` 原樣指令）重建後，u4d 檢查 5 綠。此為建置同步，
零 source 改動。

## 真引擎探針實錄（四條缺陷坐實）

探針為一次性腳本（scratch 區，不入庫；白名單僅 scripts/u4e_selfcheck.mjs），
全部斷言已折進考卷自帶的即時探針場景，真引擎（真 EngineAdapter＋真 wasm）
HEAD 未修態執行可重現。

1. **S1 巢狀表跨層**：2x2 表（para_start=1）首格再插 1x1 表 →
   `table_locator_json` 對巢狀格段落回 `table_path:{root_slot:0,
   nested_steps:[[0,0,0]]}`；HEAD 態 `resolveRootSlot` 實得 **0**（外層槽號），
   隨後 `deleteTableRow(rootSlot:0,row:1)` **ok=true**——外層表 rows 2→1、
   para_cells 5→3，用戶游標在巢狀表、被改的卻是外層根表，零提示。
2. **S2 undo 幽靈游標**：末段後插 3x2 表（caret_after={cp:0,para:14}，
   paraCount=20）→ `undo()` 回 `caret_after` **null**（key 缺席）、paraCount
   20→14，caret 滯留 `{para:14}` → 緊接 `typeAt` 引擎回
   **「段落索引越界: 請求 14，總段數 14」**。附加實錄：`caret.cp` 越界
   （len=15 放 cp=100000）經 `_applyTableMutation` 完全不被校正；
   `selection={start:{para:9999,...}}` 跨操作永久殘留。redo 真值另測：
   插表→undo→redo 路徑 redo 會回 `caret_after={cp:0,para:14}`（故 redo 紅燈
   以注入 caret_after 缺席態確定性重現，真值路徑另立回歸斷言）。
3. **S3 換檔殘留**：插表後 `lastInsertedRootSlot=0`、`selection` 非空 →
   `openFile()` 換檔後兩者**原值殘留**（caret 倒是歸零）。UI 面
   `loadFile()` 對 `activeTableSlot`／`imagePickMode` 零重置。多表歧義前提
   補強：第二張表須插在首表 para_range **之外**的正文段才是第二張根表
   （探針實錄：插進 para_start..para_end 內會被引擎收編成巢狀表，
   `table_count()` 仍 1）。
4. **S4 靜默轉 0**：`buildTablePath(null/''/'   '/false)` → `root_slot:0`；
   `buildTablePath({})` → `root_slot:null`（serde 端再炸）；`1.5`／`2**32`／
   `2**53` 原樣放行；巢狀步 `{row_idx:null,cell_idx:'',sub_table_idx:false}`
   → **`[[0,0,0]]`** 不拋錯。`table_locator_json` 拋例外時 `insertTableRow`
   把例外原樣洩漏到呼叫端（實測 threw=引擎炸了）。

## 修復（先紅後綠逐條）

### S1 `qword-ui.js` resolveRootSlot 巢狀跨層守衛

- `resolveRootSlot` 取 `located` 後加守衛：`Array.isArray(located.nestedSteps)
  && located.nestedSteps.length > 0` → 記 `_nestedCaretBlocked=true` 回 null
  （進入即清旗標，防誤擋）。
- `_requireTableTarget` 消費旗標：彈窗「巢狀表操作尚未支援——巢狀表內的
  操作尚未支援，請直接點外層表格後再操作」＋`_reportEngineRejection`
  （`rejectedByShell:true`）記帳。誠實拒勝過靜默改錯表。
- `engine-adapter._resolveTablePath` 的 `buildTablePath(located.rootSlot,
  located.nestedSteps)` 分支**原樣保留**（真巢狀支援的引擎側後路，本卡不動）。
- 考卷鎖：真原型方法呼叫——巢狀態 `resolveRootSlot` 必回 null、旗標必上、
  彈窗標題點名巢狀＋內容含指引句、拒絕日誌必補、**外層表 summary 零變動**、
  游標移回表外後正常路徑不受誤擋。

### S2 `engine-adapter.js` `_clampCaretAndSelection()` 統一夾限

- 新抽 `_clampCaretAndSelection()`：para < paragraphCount()（越界重置末段）、
  `cp <= Array.from(paragraphText(para)).length`（越界夾段尾）、selection
  任一端越界整筆重置 null（界內選區原樣保留，不得連坐清空）。
- `_applyTableMutation`、`undo()`、`redo()` 三處 `caret_after` 缺席路徑統一
  調用（舊版 `_applyTableMutation` 只夾 para 不夾 cp、undo/redo 零校正）。
- 考卷鎖：真引擎閉環「插表→undo→打字必成功且引擎回讀有字」；redo 真值
  路徑（redone=true＋打字成功）＋注入 caret_after 缺席態（caret 滯留高段
  必被夾回）；cp 越界刪除列後 `cp <= len`＋打字成功；越界 selection 重置、
  界內 selection 保留；undo/redo/_applyTableMutation 三處調用點結構鎖。

### S3 換檔狀態重置（openFile＋loadFile 兩面）

- `openFile()`：`lastInsertedRootSlot=null`＋`selection=null`（連同既有
  caret 歸零；新引擎實例配新狀態）。
- `loadFile()`：`activeTableSlot=null`＋`_nestedCaretBlocked=false`＋
  `resetImagePickMode()`。拖放 `drop` 路徑同走 `loadFile`，一站收口。
- 考卷鎖：換檔後 adapter 欄位真值必 null；UI 真原型 `loadFile` 閉環
  （activeTableSlot／imagePickMode／accept／旗標＋adapter 狀態全清）；
  換檔後新檔**兩張根表**（插入點在首表 para_range 外）＋表外游標 →
  `resolveRootSlot` 必回 null 誠實拒——U4d L6 防線不再被殘留槽號繞過。

### S4 `buildTablePath` 型別強制＋`_resolveTablePath` 收斂

- 新 `coerceUsizeIndex(value,label)`：先排 boolean/null/object/空白字串，
  再 `Number.isSafeInteger(n) && 0<=n<=0xFFFFFFFF`，不合規直接拋錯。
- `root_slot` 與巢狀步三欄全部改走守門（陣列／物件形態同檢；物件形態改
  `'row_idx' in step` 欄位存在性檢查，`{row_idx:null,...}` 不再被 `Number()`
  洗成 0）。合法形態（u4d L3 契約：物件三元組正規化、陣列原樣保留）零誤傷。
- `_resolveTablePath` 整段包 try/catch 收斂為 `{path:null, error}`——locator
  壞結構或構造器拋錯不再洩漏未捕獲例外給 Ribbon onclick。
- 考卷鎖：null/''/false/{}/-1/1.5/2**32/2**53 全拋錯＋0/3/0xFFFFFFFF 合法
  回歸＋真引擎 SetCellProps 通過；locator 注入拋例外 → `insertTableRow` 回
  `{ok:false, rejectedByShell:true, error}` 零洩漏，且收斂後同一路徑仍可用
  （try/catch 不得吞成功路徑）。

## 紅燈實錄（修前，HEAD a33944e 源碼跑同一考卷）

```
→ 檢查 0｜真引擎前提 綠
→ 檢查 S1｜巢狀表游標→靜默改外層根表 紅（11 項）
→ 檢查 S2｜undo／redo 幽靈游標 紅（7 項）
→ 檢查 S3｜換檔殘留上一檔表格狀態 紅（11 項）
→ 檢查 S4｜路徑型別強制 紅（25 項）
→ 檢查 9｜自我鑑別 紅（1 項）
自檢結果：FAIL（55 項違規）
```

代表性紅燈（行為面，非源碼鎖）：

```
✗ 游標在巢狀表內時 resolveRootSlot 必回 null（實得 0——外殼拿外層根表槽號操作）
✗ undo 後游標段落必須 < 段落總數（實得 caret={"cp":0,"para":14} paraCount=14 → 幽靈游標）
✗ undo 後緊接打字必須成功（實得 {"ok":false,"error":"段落索引越界: 請求 14，總段數 14"}）
✗ 表格操作後 caret.cp 必須 <= 游標段落字元碼點數（實得 cp=515 len=15）
✗ cp 夾限後打字必須成功（實得 "碼點偏移越界或區間倒置: 段 0 長度 15，請求 (515..515)"）
✗ root_slot=false 必被誠實拒（實得 ok=true, target.rootSlot=0——Number(false)=0 洗白即靜默操作第 0 張表）
✗ root_slot=[] 必被誠實拒（實得 ok=true——同上，Number([])=0）
✗ 循環引用巢狀步需拋守門 Error 帶欄位名（實得 Converting circular structure to JSON——序列化器自爆覆蓋守門語意）
✗ 換檔後 lastInsertedRootSlot 必須 null（實得 0）
✗ loadFile 後 activeTableSlot 必須 null（實得 0）
✗ buildTablePath(null) 必須拋錯（實得不拋：root_slot:0——壞座標靜默送進引擎）
✗ nested 物件形態 {null,"",false} 必須拋錯（實得不拋：nested_steps=[[0,0,0]]）
✗ locator 拋例外不得洩漏未捕獲例外到呼叫端（實得 threw=引擎炸了）
```

## 綠燈實錄（修後，merged 態）

```
→ 檢查 0 綠；檢查 S1 綠；檢查 S2 綠；檢查 S3 綠；檢查 S4 綠；檢查 9 綠
自檢結果：PASS — U4e 四盞燈全綠
鑑別：S1 守衛／S1 彈窗文案／S2 undo 夾限／S3 openFile 重置／S3 loadFile 重置／
      S4 嚴格轉型／S4 try/catch —— 七種回塞態全紅，真樹零污染
U1/U2/U3/U4/U4b/U4d 六考卷：PASS ×6
ow-gate：GATE_SUMMARY fail=0
```

## 第二意見（agy gemini-3.8-flash-high 審查 git diff）

第一輪 agy 對完整 diff 提六點；逐條覆核處置如下（第二輪增量覆核確認修正合格、
無新增問題）：

| agy 問題 | 嚴重度 | 覆核 | 處置 |
|---|---|---|---|
| 1. `_resolveTablePath` 入口先 `Number(rootSlot)` 判定，`Number(false)=0`／`Number([])=0` 在守門前被洗白 | 嚴重 | **坐實**（我最初 S4 修復的入口漏網，真引擎實跑 `insertTableRow({rootSlot:false})` 曾 ok） | 入口改直接走 `coerceUsizeIndex`（拋錯由既有 try/catch 收斂）＋考卷行為鎖（false/[]/true 入口必拒）＋結構鎖（入口必見 `coerceUsizeIndex(rootSlot`） |
| 2. para 越界夾回末段時保留舊 cp，游標落在末段文字中間，與註解「末段起點」矛盾 | 中度 | **坐實**（我抽的 `_clampCaretAndSelection` 把 U4d L5 原契約「末段起點」降格成「夾 para 保 cp」） | paraClamped 時 `cp=0`（與 U4d L5 原契約一致）；cp-only 越界才夾段尾＋行為鎖（雙越界必得末段起點）＋結構鎖 |
| 3. `caret_after` 在場路徑完全跳過 selection 校正 | 中度 | 部分採納：卡面定案把夾限 scope 在「caret_after 缺席時」，但幽靈選區同屬幽靈狀態，成功路徑跳過校正是真缺口 | `_applyTableMutation` 成功後統一夾限（合法回訊在場＝游標 no-op，越界選區照清；undo/redo 維持缺席分支——undo 系回訊在場時動游標屬卡面 scope 外）＋行為鎖（caret_after 在場路徑的越界選區必清＋合法游標零變動）＋回塞鑑別鎖 |
| 4. `_nestedCaretBlocked` 是實例副作用，未來若有按鈕態輪詢會誤擋 | 低 | 現行單執行緒呼叫鏈無此路徑；卡面定案即「旗標＋彈窗」設計 | 不改設計，接受記錄（輪詢接入時再結構化回傳） |
| 5. `JSON.stringify` 對循環引用/BigInt 自爆 TypeError 覆蓋守門語意 | 低 | **坐實**（真機重現：循環引用步實得 `Converting circular structure to JSON`） | 抽 `safeRepr()`（失敗退 `Object.prototype.toString`）＋`bigint` 在 `Number()` 前擋（`Number(2n)` 自身拋 TypeError）＋考卷鎖（BigInt/循環引用必拋帶欄位名的守門 Error） |
| 6. 寬泛 catch 吞堆疊，開發期除錯線索消失 | 低 | 採納 | catch 內 `console.warn` 保留真例外（含堆疊），呼叫端照樣收 `{path:null,error}` |

第二輪增量審查（final diff）結語：「問題5、6修正確認合格；**無新增問題**」，
並提示問題1～3需確認已處理——1、2、3 已在最終版落地（見上表），對應考卷加鎖
後先紅後綠重跑：HEAD 未修態 55 項全紅（S1:11／S2:7／S3:11／S4:25）、merged 態
七考卷全綠、ow-gate fail=0、七種回塞鑑別全紅。

