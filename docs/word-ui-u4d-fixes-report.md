# U4d 返修報告——U4 合併後獨立審查九條（2×S1＋確認 S2/S3），先紅後綠實錄

- 分支：`wt/u4d-fixes`（未 push）；工作區 `.worktrees/t_2d7a308a`
- 改動檔案（白名單內）：`shells/web/editor/js/engine-adapter.js`、`qword-core.js`、`qword-ui.js`、`scripts/u4d_selfcheck.mjs`（新考卷）、`scripts/u4_selfcheck.mjs`（過期前提重估，見 §6）、`docs/word-ui-u4d-fixes-report.md`（本檔）
- 驗收考卷：`scripts/u4d_selfcheck.mjs`（九盞燈＋自我鑑別）。**修前 9 盞全紅（48 項違規）、修後全綠**；U1/U2/U3/U4/U4b 五份既有考卷同場全綠；`ow-gate.sh` fail=0。

---

## 1. 先紅：修前紅燈實錄（`node scripts/u4d_selfcheck.mjs` @ 未修 tree）

```
[檢查 0｜真索引綁定前提]              綠（前提：db865bf 三顆真索引已綁進 wasm 殼）
[檢查 1｜L1 單列／單欄表格結構]        紅（14 項）
[檢查 2｜L2 檔案選取器取消殘留]        紅（6 項）
[檢查 3｜L3 nested_steps serde 契約]   紅（4 項）
[檢查 4｜L4 圖片單邊尺寸等比折算]      紅（3 項）
[檢查 5｜L5 刪列後游標校正]            紅（3 項）
[檢查 6｜L6 activeTableSlot 真反查]    紅（3 項）
[檢查 7｜L7 table:shading 誠實標籤]    紅（2 項）
[檢查 8｜L8 拒絕事件的 UI 監聽]        紅（8 項）
[檢查 9｜L9 引擎回傳 JSON 解析容錯]    紅（4 項）
[檢查 10｜自我鑑別]                    紅（1 項）
自檢結果：FAIL（48 項違規）
```

每盞燈的關鍵紅燈輸出（原樣摘自修前 log）：

- **L1**：`單列表未帶 at_row 的「插入列」不得被外殼拒（實得 {"ok":false,"error":"未帶 at_row 且無法讀取列數：…不得刪除表格僅剩的一列（請改用 DeleteTable）"}`、`3x1 表：tableShape 需與引擎 summary 逐字一致（shape=…列序 1000000000 越界（共 3 列）…）`——越界探針對 `len()<=1` 先回「僅剩的一列」訊息、不含「共 N 列」，坐實 agy S1-1 與 PM 補正的根因。
- **L2**：`檔案選取器只監聽 change：用戶按「取消」時只觸發 cancel 不觸發 change，旗標與 accept 永久殘留`＋復原函數缺位（全檔 grep 零 `cancel` 監聽，坐實 S1-2）。
- **L3**：`buildTablePath 需把物件三元組正規化為數值陣列（實得 [{"row_idx":0,"cell_idx":1,"sub_table_idx":0}]）`＋引擎真反序列化實錄 `invalid type: map, expected a tuple of size 3`（坐實 S2-3：舊 u4 考卷用物件形態假通過）。
- **L4**：`只指定寬 100pt 的 2:1 圖，高必須等比折算為 50pt（實得 75pt → 非正方形圖被拉伸）`（坐實 S2-4：單邊指定時另一邊取 natural 原換算值不按用戶邊折算）。
- **L5**：`刪列後游標段落必須 < 段落總數（實得 caret={"para":18,"cp":0} paraCount=16 → 幽靈游標）`＋`刪列後緊接打字必須成功（實得 {"ok":false,"error":"段落索引越界: 請求 18，總段數 16"}）`——**真引擎重現成功**（卡面要求「若引擎真報 XmlPatchRejected 才修」，實際報錯訊息即段落索引越界，屬同一越界家族）。
- **L6**：`UI 仍盲設 activeTableSlot = tableCount()-1`＋探針實錄：游標在前段插第二張表時**新表拿到 root_slot 0**（先插的被擠到 1）——「新表在末張」非契約，盲設必命中舊表（坐實 S2-6）。
- **L7**：`table:shading 無可見發射（引擎缺口 t_f0cbc25d），不得宣稱 live（實得 "live"）`＋動態實錄 `setTableProps({shadingFill})` ok=true 但顯示列表零變化（坐實 S3-7）。
- **L8**：core 派發的 `table:row-col-rejected`／`table:props-rejected`／`image:inline-rejected` 三事件 UI 零監聽（坐實 S3-8）。
- **L9**：adapter 直接 `JSON.parse(引擎回傳)` 4 處無容錯＋壞字串注入拋未捕獲例外（坐實 S3-9）。

## 2. 後綠：修後全綠實錄（同考卷 tree 修後）

```
[檢查 0] ✓ 三顆真索引已綁進 wasm 殼；nested_steps 契約＝三元組陣列
[檢查 1] ✓ 越界探針已撤除：結構讀真索引（summary＋locator）
         ✓ 1x1 表：插列／插欄／setCellProps 三條路徑全通（顯示列表 75 → 77）
         ✓ 行列數讀回與引擎 summary 逐字一致（1x3／3x1／2x3／刪至剩 1 列）
         ✓ table_locator_json 反查：游標在表內命中該表、表外回 null
[檢查 2] ✓ cancel 監聽＋file:open 前置重置齊備（取消不再殘留圖片過濾）
[檢查 3] ✓ 物件形態引擎回 serde 錯（實錄），正規化形態通過反序列化
[檢查 4] ✓ 單邊指定按 natural 2:1 等比折算（100×50pt），兩邊指定照用戶值
[檢查 5] ✓ 刪列後越界游標被校正、打字閉環通過；表外游標不動
[檢查 6] ✓ 游標前段插表：反查 root_slot=0（引擎真值），UI 零盲設
[檢查 7] ✓ table:shading 降 pending:engine-*（零發射實錄），儲存格層底紋不受影響
[檢查 8] ✓ 三顆拒絕事件全數接住＋彈窗＋帶明細
[檢查 9] ✓ 壞字串注入：回 ok:false＋錯誤訊息，零未捕獲例外
[檢查 10] ✓ 五種回塞態全紅（探針回填／cancel 摘除／假 live 回填／監聽摘除／盲設回填），真樹零污染
自檢結果：PASS — U4d 九盞燈全綠
```

## 3. 逐條修法

| # | 修復 | 實作要點 |
|---|------|----------|
| L1 | 撤除越界探針（PM 補正版：不走「補匹配錯誤訊息」regex 路線） | `tableCount()` 只讀 `engine.table_count()`；結構讀 `tables_summary_json()`；游標反查 `table_locator_json(para)`（欄位名與 `mutation::TablePath` 逐字對齊，直餵 SetCellProps/DeleteTable）。`setCellProps` 座標缺席才延遲取真值：游標反查 → summary 末格，帶座標者零連坐 |
| L2 | cancel 清理 | `fileInput` 補 `cancel` 監聽呼 `resetImagePickMode()`＋清 `value`；`change` 無檔案同清；`file:open` 處理前主動重置；`requestImageFilePick()` 統一設圖片 accept |
| L3 | nested_steps 正規化 | `buildTablePath` 具名欄位物件→數值三元組 `[row_idx,cell_idx,sub_table_idx]`；不完整／長度非 3／形態非法直接拋錯（不靜默送引擎）；考卷送真 `apply_mutation_json` 驗 serde |
| L4 | 單邊尺寸等比 | 只給寬：`hPt = wPt × naturalH / naturalW`（反之同理）；兩缺 96dpi 並鉗紙寬；`placed` 回傳實際幾何供考卷斷言 |
| L5 | 幽靈游標校正 | `_applyTableMutation` 成功且引擎未給 `caret_after`（表格類實測恆 null）時自查 `caret.para >= paragraphCount()` → 重置末段起點；游標在表外時不動（考卷對稱斷言） |
| L6 | 真反查取代盲設 | `insertTable` 成功後僅在引擎回 `caret_after` 時反查 `currentTableCell()` 記 `lastInsertedRootSlot`（探針實錄 root_slot 按 XML 文檔序，可落在任何 slot），否則記 null；UI `activeTableSlot = adapter.lastInsertedRootSlot`；`resolveRootSlot`＝指定＞游標反查＞最近真反查＞單表 0，不可得回 null＋誠實彈窗，**零 `count-1` 靜默回退** |
| L7 | 誠實標籤 | `U4_TABLE_PROPS_COMMANDS['table:shading']` 掛 `engineGap: 'pending:engine-table'`（dependency map 補名稱/說明），mutation 照送（XML 真寫入）但不宣稱可見；儲存格層底紋（Rect 發射實測）不受連坐 |
| L8 | 拒絕事件接聽 | UI 接聽三顆 `*:rejected` 事件→`handleCoreRejected()`：`showErrorModal`＋problems 明細＋`_reportEngineRejection` 留痕 |
| L9 | JSON 容錯 | 兩級出口：讀取面 `safeEngineJson`（壞字串→null＋warn）、mutation 面 `engineMutationResult`（壞字串→`{ok:false, rejectedByShell, badEngineJson, error}`），全檔 17 處直接 `JSON.parse(引擎回傳)` 悉數收斂，helper 外殘留 0 |

## 4. 真瀏覽器閉環補充證據（CDP :9222，本 worktree `shells/web` 起 http.server 8777）

- **L2（真 DOM）**：`insert:picture` 鈕 click 後 `#qword-file-input.accept` 變圖片過濾 → 派 `cancel` 事件 → `accept` 回 `.docx,.odt`、`value` 清空。實錄：`{"before":".docx,.odt","afterPick":"image/png,image/jpeg,...","afterCancel":".docx,.odt","valueAfterCancel":""}`。
- **L8（真 DOM＋真引擎）**：載入樣本後 `core.onCommand('table:width',{rootSlot:999,widthPt:200})` → 引擎拒（「根表序 999 越界（共 0 張）」）→ `#modal-error` 彈窗可見（class `qword-modal-backdrop open`）＋訊息含引擎原話。參數驗證層路徑（`widthPt:-50`）同驗：彈窗列「width_pt 需為正數，收到 -50」。
- **L6（真 DOM＋真引擎）**：游標放段 0 插表 → `lastInsertedRootSlot=0`（引擎反查真值，非 `tableCount()-1`），隨後不帶 rootSlot 的 `table:row-insert-below` 命中**新表**（summary rows 2→3）。

## 5. 附註（考卷內 NOTE 同步記錄，不假裝綠的邊界)

1. **表格儲存格文字 DL 零發射＝引擎顯示缺口**：真引擎對照實錄——全新 2x2 表游標正常打「己」→ `paragraph_text` 有字但顯示列表命中 0；同字打 body 段則進 DL。屬 t_f0cbc25d 同族（core/ 白名單外）。L5 綠燈斷言因此鎖「引擎回讀真值（打字寫進游標段落）」＋打字 ok，DL 缺口如實掛附註並在此上報 PM。
2. L2 的 cancel 語義屬真實瀏覽器行為（取消不觸發 change），考卷源碼態鎖＋本報告 §4 真瀏覽器閉環雙軌互補。

## 6. `scripts/u4_selfcheck.mjs` 過期前提重估（考卷可收窄不可弱化）

修後跑 U4 考卷紅 5 項，全部是考卷**自己設計好的Tripwire 如實發作**，逐條重估（非弱化）：

- 邊框缺欄斷言 ×3＋探針前提 ×1：E 卡 `2cb6a18` 已把 `borders: Option<TableBordersPatch>` 送進引擎。真引擎重測實錄：合規 patch `{borders:{top:{style:'single',width_pt:3}}}` → ok=true 且 **Rule 發射 0→3**（四邊→10）；字串形態 `borders:'single'` 被 serde 型別拒；`bogus_field` 仍被靜默吞（誠實紅線照舊有效）。考卷從「確認缺欄」改鎖「具型別欄位存在＋合規有發射＋外殼未接線前仍拒邊框參數冒充成功」（外殼邊框接線屬 PM 新工作面，U4d 不做）。
- `nested_steps` 物件形態斷言：U4d L3 坐實舊斷言是假通過（引擎 serde 要三元組陣列），改鎖正規化契約 `[0,1,0]`。
- 另修正 `engine-adapter.js`/`qword-core.js` 兩處過期註解（「引擎無 w:tblBorders 寫入面」——E 卡後已不实）。

重估後 U4 考卷 PASS。

## 7. 驗收清單

- [x] 九條先紅後綠：修前 9 盞全紅 48 項違規（§1），修後全綠（§2），每盞有實測輸出
- [x] 自我鑑別：五種回塞態全紅（§2 檢查 10），防考卷放水
- [x] U1/U2/U3/U4/U4b/U4d 六份自檢 repo 根全綠（U4 經 §6 重估）
- [x] `bash ~/.hermes/scripts/ow-gate.sh <worktree>` → `GATE_SUMMARY fail=0`（fmt/clippy/test/分層/roundtrip fixtures+corpus/wasm check/goldens 全過）
- [x] 白名單紀律：僅動 `shells/web/editor/js/` 三檔＋考卷兩檔＋docs/；`core/`、`index.html` 零改動（`git status` 為證）
- [x] 嚴禁 push/checkout/reset/rebase/stash：分支 `wt/u4d-fixes` 僅本地 commit，未 push
