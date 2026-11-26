# Word-UI U4N：殼層 Enter 分段接正產線 SplitParagraph（卡 t_d900332c）

## 0. 一句話

殼層 `beforeinput(insertParagraph)` 曾走 `commitText("\\n")`（段內插字面換行、
段數永不增）——這是用戶「無法任意按 Enter 創建新段落」的真根因（殼層，非引擎）。
本卡把 Enter 改接正產線 `apply_mutation_json(SplitParagraph)`，Shift+Enter
改接 `InsertBreak(LineWrapping)`，兩種語意兩條路徑，字面路徑零殘留。

引擎側零觸碰（紅線遵守：`git diff main..HEAD` 對 `core/` 零差異，實錄見 §5）。

## 1. 修復前後對照（真鍵盤實錄，fixture＝fixtures/style_cascade.docx）

| 動作 | 修前（回塞態實錄） | 修後（good 態實錄） |
|---|---|---|
| Enter（段中） | 段數 47→47；段內現字面 `\\n`；`apply_mutation_json` 未被呼叫 | 段數 47→48；頭段＝前半、新段＝被切開的後半；caret＝{para+1, cp:0} |
| Shift+Enter | `commitText("")` 靜默無操作（軟換行連斷點都沒進文件） | `InsertBreak(LineWrapping)` 進文件（存檔 XML 見實錄 `<w:br/>`）；段數不變 |
| 第二次 Enter | 47→47 | 48→49（可連續分段） |
| undo_json | n/a（引擎堆疊無此操作） | 段數回減一，段索引零殘留 |
| 存檔重開（quire-cli 回讀） | **段數 47＝47 但逐段文字不符 2 段**——段內字面 `\\n` 連存檔都不活（第二層缺陷） | 段數與逐段文字全等（不符 0 段） |

回塞態另坐實第二層缺陷：字面 `\\n` 經 `save_docx()` 存檔後讀回消失（`w:t` 內
字面換行不保形）——即修前路徑不僅「不增段」，連它留下的資料都是存檔會蒸發的
髒態。這進一步支持「必須走 Mutation 正產線、不得用字面字元湊」。

## 2. inputType 分派邊界（PM 點名「不得用同一條路徑混用兩種語意」）

外殼 `shells/web/demo/main.js` `beforeinput` 分派（修復後態）：

| inputType | 觸發 | 路徑 | 引擎語意 | 段數 |
|---|---|---|---|---|
| `insertParagraph` | Enter | `applyEnterMutation({SplitParagraph, target:"Body", pos:{para,cp}})` | M1.3 段落一分為二，後半移入 `para+1` | +1 |
| `insertLineBreak` | Shift+Enter | `applyEnterMutation({InsertBreak, target:"Body", pos:{para,cp}, kind:"LineWrapping"})` | M1.5 段內軟斷點 `<w:br/>`（Word 同語意：斷點不是新段） | 不變 |
| `insertText`／`insertFromPaste` | 鍵入／貼上 | `commitText(e.data)`（原路，不變） | M1.1 插入文字 | 不變 |

邊界寫清楚：
- **軟換行語意保留並落正產線**：Shift+Enter 不再是 `commitText("")` 靜默吞掉，
  而是在 caret 處發 `InsertBreak LineWrapping`——存檔 XML 實錄為
  `<w:t>…上落</w:t><w:br/><w:t>地。</w:t>`（同段兩行，非兩段）。
- **caret 落點以引擎回讀真值為準**：兩條路徑共用 `applyEnterMutation`，成功時
  caret 取 `outcome.caret_after`（SplitParagraph＝{para+1, cp:0}；InsertBreak＝
  引擎給的 {para, cp+1}），殼層不自行臆算。
- **引擎誠實拒絕時零副作用**：`apply_mutation_json` 回 `ok:false`（例：caret 在
  表格格內——M1.3／M1.5 只收正文 Body）時狀態列顯示引擎錯誤原文、文件零變動，
  **絕不回退** `commitText("\\n")` 假裝成功。表格內分段支援屬引擎側另案
  （本卡禁改 `core/`，不越界湊燈；已查證引擎 `SplitParagraph` 對非 Body target
  回 `SubDocNotFound` 誠實錯誤，非崩潰）。
- `insertFromYank`／`insertReplacementText` 等其餘 inputType 維持既有
  `default: preventDefault()`（不讓 contenteditable 自行改動），零放寬。

## 3. 考卷與實錄（先紅後綠）

新檔 `shells/web/tests/t_shell_enter_paragraph_split.py`——比照
`t_f073cd6c_click_matrix_live_probe.py` 探針口徑：真 Playwright＋真 pkg 產物＋
拒跑防假綠（pkg／quire-cli／vite 任一缺失＝exit 2 環境錯誤，絕不降斷言）。
本檔另加固雨點：
- **自起 vite（strictPort 5211）**：沿用別處 5199 會測到主倉／別棵 worktree 的
  舊 `main.js`（假綠源）；埠被佔用即環境錯誤。收尾 `killpg` 整團帶走 vite
  子程序（`terminate()` 只打直系子程序，孤兒續佔埠踩過）。
- **取證包裹必須在 engine 就緒後裝**（包的是 engine 實例；早於開檔裝＝包到
  null，呼叫鏈恆空→回塞斷言假紅，踩過一次並在碼內留註解）。
- **零未捕獲異常前置燈**：回塞鎖自身拋錯（ReferenceError 之類）會被瀏覽器吞掉
  變成「鎖沒咬」的假態——pageerror 計數入前置燈與證據 JSON。

燈（單一斷言集；`scripts/u4n_enter_split_buggy_replay.sh buggy|good`）：

    good 態（修復後，全綠＝修復證據）：
      前置×3（鎖態一致／零 pageerror／靶數）          PASS
      燈 E1  真 Enter → paragraph_count 增一 47→48     PASS
      燈 E2  頭段＝前半＋新建段落＝被切開的後半         PASS
      燈 E3  Enter 後 caret＝{para+1, cp:0}            PASS
      燈 E4a Shift+Enter 段數不變                       PASS
      燈 E4b Shift+Enter 走 InsertBreak、未走 Split     PASS
      燈 E4c InsertBreak kind＝LineWrapping             PASS
      燈 E5a apply_mutation_json(SplitParagraph) 呼叫   PASS
      燈 E5b insert_text_at_caret("\\n") 未被呼叫        PASS
      燈 E5c 第二次 Enter 再增一（48→49）               PASS
      燈 E6  undo_json 後段數回減一（無殘留）           PASS
      燈 E7a 存檔重開段數一致（quire-cli 回讀）         PASS
      燈 E7b 存檔重開逐段文字全等                       PASS

    buggy 態（U4N_ENTER_SPLIT_BUGGY 回塞鎖，9 盞必紅＝鎖有鑑別力）：
      E1（47→47）E2 E3 E4b E4c E5a E5b E5c E7b 全紅 ✓
      （E4a／E6／E7a 兩態同綠：回塞態本來就不增段、undo 空堆疊無操作、
        段數兩者皆 47——非放水，判準對兩態本來同值）

存檔回讀真值口徑：`save_docx_bytes()` 落盤 → `quire-cli paragraphs <saved>`
（引擎回讀，非自報旗標）逐段比對文字＋段數。

回塞鎖（JS 版 k21/g1 口徑，比照 U4F_*／U4D_*）：`main.js` 內建
`u4EnterBuggy()` 於**事件時**讀 `process.env.U4N_ENTER_SPLIT_BUGGY`（正常瀏覽器
無 process 恆假，零行為擾動）；探針 buggy 態經 `addInitScript` 注入
`window.process` 驅動。零改檔、零 checkout。
教訓記帳：初版把判別寫成模組層常數快照＋一處 `u4EnterBuggy` 拼寫漂移——
前者在注入前定值鎖咬不住、後者 Shift+Enter 臂整臂 TypeError，兩者都會讓
回塞態假綠／假紅混在一起。現行碼以「事件時求值＋零未捕獲異常前置燈」雙防。

## 4. 隨卡收斂：快取段索引／Undo 殘留查證（PM 點名項）

**查證無殘留**。查證方式（三徑）：
1. **狀態面枚舉**：`grep -E "^let |^const |^var " shells/web/demo/main.js` 全枚舉
   模組層狀態——與段落歸屬相關的只有 `caret = {para, cp}` 一處；其餘
   （`page`／`scale`／`composing`／`reflowScheduled`／協作面板）非段索引快取。
   `caret` 在本卡兩條路徑後一律改寫為**引擎回讀** `outcome.caret_after`，
   殼層不再自行推算段號。協作面板 `paraA`/`paraB` 是用戶逐次提交的數字輸入
   （CRDT demo 參數），非編輯面段索引快取，不屬本缺陷面。
2. **讀取面實時性**：段落文字/幾何全部經 `engine.paragraph_text`／
   `caret_rect_json`／`page_display_list_json` 實時查引擎，殼層無段落內容或
   索引的第二份拷貝（grep `paragraph_text(` 呼叫點逐一核對）。
3. **Undo 面**：Undo 狀態只在引擎 undo 堆疊（`undo_json` 正產線），殼層不持有
   自備 undo 佇列；燈 E6 實測「Enter→undo→段數回基線＋後續段索引歸屬正確
   （caret_after 引擎真值）」。

## 5. 紅線自證

- `core/` 零觸碰：`git diff main..HEAD --stat` 僅 3 檔
  （`shells/web/demo/main.js`、`shells/web/tests/t_shell_enter_paragraph_split.py`、
  `scripts/u4n_enter_split_buggy_replay.sh`＋本報告）。
- `corpus/`、`fixtures/` 既有檔零觸碰；既有考卷零觸碰（新檔 only）。
- 新增文字繁中：`git diff` 新增行經 opencc s2t 對照淨空（白名單字零出現）。
- 未 push。

## 6. 重放指令

    cd shells/web && npm run build:wasm
    cargo build -p quire-cli
    ~/.local/chrome-control/bin/python shells/web/tests/t_shell_enter_paragraph_split.py
    # 回塞紅／綠雙態（比照 U4F_*/U4D_* 口徑）：
    bash scripts/u4n_enter_split_buggy_replay.sh buggy   # 9 盞紅＝鎖有效
    bash scripts/u4n_enter_split_buggy_replay.sh good    # 全綠

證據落盤：`build/u4n-enter-split-good.json`／`build/u4n-enter-split-buggy.json`
（含逐盞明細、呼叫鏈實錄、存檔回讀差集）＋同名 PNG 截圖。
