# U4m：caret_at 行框歸屬改行距中線切分 — 交付報告（**含重大前提質疑，待 PM 裁示**）

任務：t_f073cd6c（ow-core）｜分支：`openword/t_f073cd6c-caret-linebox`（基線＝main `e758668`；未 push）
尾標：U4M_LINEBOX｜狀態：**考卷全綠收案，但卡面前提被實測推翻——見 §2，已 block 待裁**

## 1. 血緣與落地（卡面要求的，都做了）

`caret_at` 反查的行框歸屬，由「行的文字跨度＝[起帶, 起帶+span) 對齊基線帶」判制
（U4f 跨度制）改為**相鄰行以基線帶中線切分**（中介帶 [lo,hi)，上下界＝與同欄前／後
行基線帶之中線，被表格／圖片佔用帶夾住），正向 `caret_rect` 零改動（非第二把尺）。
源碼＝`core/layout/src/lib.rs`（db8f901，正向腿零 diff）；回塞鎖 `U4M_LINEBOX_BUGGY`
比照 `U4F_CARET_AT_BUGGY` 口徑。

## 2. 決定性發現：卡面「Text.y＝基線」前提與三支出貨渲染器相反（實測，非推測）

卡面實錄寫「para5 首行 **baseline** `y_pt=408`（display list Text.y 同值）」，據此把
「基線以上 dy=−14..−2 反查回 para4」判為缺陷。但三支出貨渲染器全部把**字形基線**畫在
`Text.y + size_pt`（`shells/web/demo/main.js:410`、`shells/web/editor/vendor/renderer.js:338`
（用戶實錄來源編輯器外殼同尺）、`shells/native/src/render_svg.rs:144` 註解自白「讓 y
語義接近行頂」）——**`Text.y` 是行框頂，不是基線**。用戶實錄檔（12pt／行距 1.4）的
真實視覺算術：

| 量 | 卡面讀法 | 渲染實態 |
|---|---|---|
| para5 首行 `Text.y` | 基線 408 | 行框頂 408（基線實為 420） |
| dy=−14..−2（y=394..406） | 「para5 的 ascent 區」→ 判 para4＝錯 | **para4 自己的字形墨跡**（para4 墨跡≈[392.6,405.2]）→ 判 para4＝**對** |
| dy=0..+8（y=408..416） | 正確區 | para5 字身上半（para5 墨跡≈[409.4,422]）→ 判 para5＝對 |

亦即：**卡面把那批點擊判為「錯判上一段」，是建立在「Text.y 是基線」的誤讀上；以
canvas 像素墨跡實測，修前的跨度制對「點到字形必得本段」是零錯判的**（§3 數字）。
真正的視覺錯判反而是本卡修後的**中線制**造成的：中線 `y + pitch/2 = y + 8.4` 橫穿
字形（墨跡 [y+1.4, y+14]），把每行字形的**下緣約 5.6pt**（下伸部＋字身下半）判給
**下一段**。用戶若在 para5 字形下緣（例 y=418）點擊按 Enter，游標會落 para6——這纔是
修後態的真實回歸面。

### 歸因對照（同批 canvas 像素墨跡、純幾何兩式並跑，探針燈 C 實錄）
- 規則 S（修前跨度制，行框=[y_i, y_{i+1})）：字形墨跡內錯判 **0 點**
- 規則 M（修後中線制）：字形墨跡內錯判 **202 點**（引擎實測 111 點，差額＝表格帶 None 等邊界）
- 修前態（BUGGY）cargo 六盞紅＝u4m 考卷契約化「caret_rect.y 為行錨」的口徑下才成立；
  該口徑本身即此前提問題的一體兩面。

### 「無法任意按 Enter 創建新段落」的實測根因鏈（與 caret 歸屬無關）
殼層取證（`shells/web/tests/t_f073cd6c_shell_enter_evidence.py`，真瀏覽器真鍵盤，
`build/u4m-shell-enter.json`）：main 態 demo 殼 Enter → `beforeinput(insertParagraph)`
→ `commitText("\n")` → `insert_text_at_caret(para, cp, "\n")`——**段內字面換行，
段數 47→47 不增**，`apply_mutation_json`（SplitParagraph 正產線入口）從未被呼叫。
用戶症狀的直接成因在此（PM 裁決 2：已確認、另卡修編輯面 inputType 分派）。卡面矩陣
「grew=1 但新建段落在 para4/5 之間」亦發生在caret 判定本就正確（跨度制）的態，指向
殼層計數口徑而非 `caret_at`。

## 3. 考卷與實錄（全部親跑）

**卡面考題 1/2/4（cargo，core/layout/tests/u4m_linebox_hit.rs 六盞＋u4f/t4aa 零擾動）**
- `cargo test -p quire-layout --test u4m_linebox_hit`：**7 passed 0 failed**
- `U4M_LINEBOX_BUGGY=1` 回塞態：恰 4 盞鑑別燈紅（燈1/2/3/6），前提燈綠（run 85 已錄，run 88 複跑一致）
- u4f_band_ruler 4 passed／t4aa_cell_caret 7 passed（零擾動）
- `cargo fmt` 淨／`cargo clippy -D` 淨／**workspace 132 blocks 1143 passed 0 failed**

**卡面考題 3（真滑鼠 CDP 矩陣，shells/web/tests/t_f073cd6c_click_matrix_live_probe.py，
真 Chrome＋真 `page.mouse.click`＋實際 pkg/ 產物；style_cascade.docx 第 0 頁混級）**
- 燈 A1（契約 y ±(該邊 pitch/2−1)＝卡面考題2 同段面）：76 次點擊 → **錯段 0／死區 0**
- 燈 A2（跨中線 pitch/2+1＝「跨中線才換段」）：74 次 → **caret 全落相鄰顯示行碼點區間**
- 燈 B（缺陷面點擊定 caret → 真產線 `apply_mutation_json(SplitParagraph)`）：18 靶 →
  **段數增一＋新建段落＝點擊段被切開的後半，全等**
- 燈 C（記帳盞，非 gate）：像素墨跡真值稽核＝§2 表（111/202 點）＋規則 M/S 對照
- 證據：`build/u4m-click-matrix.evidence.json`＋截圖 `build/u4m-click-matrix.png`
- 探針自身三處假紅根因（靶誤錨 `Text.y+size`＝兩把尺混用／行首 cp 邊界回上一行 y／
  混級檔 pitch 必逐邊取）已全部寫入檔頭，覆核者可據以重放。

**真鍵盤 Enter（卡面考題3 後半的字面路徑）**：demo 殼按 Enter 不增段（§2 根因鏈），
故燈 B 依 PM run-85 記帳改打正產線 SplitParagraph 入口；字面鍵盤路徑證據另檔留給
殼層卡。

## 4. 修訂史簡表（本卡 commit 鏈）
| commit | 內容 |
|---|---|
| db8f901 | caret_at 中線制＋u4m 六盞＋u4f 燈3(b) 改寫（run 85，PM 預警保護 commit） |
| dbfae0f | run 85 殘留探針保護 commit（PM 代） |
| 02d8a57 | 點擊矩陣探針重寫至卡面口徑＋殼層 Enter 取證檔＋幾何偵察檔（真滑鼠全綠） |
| 442eeba | 簡繁收案（正字化 8 種字 15 行；左側引文屬有意簡體記錄；群→羣 屬 opencc 誤報照白名單不動） |
| 8703a04 | 探針補規則 M/S 歸因對照盞 |

## 5. 待 PM 裁示（本卡為何 block 而非 request_review）
卡面四條考題＋回塞鎖已全部滿足並綠，但 §2 的實測證明**考題所依的座標前提**與出貨
渲染器矛盾：照卡面收案＝把「點擊字形的視覺歸屬」從零錯判改成每行錯 ~1/3 字高。
鐵律（禁未經允許還原源碼）下我不動 `caret_at`，裁示選項：
- **(A) 維持現狀收案**：卡面口徑（caret_rect.y 為行錨）優先；墨跡回歸另卡。
- **(B) caret_at 退回跨度制（規則 S）**：以「點到字形必得本段」為準；u4m 六盞與
  u4f 燈3(b) 需連帶重寫（需用戶明確授權還原動作）。
- **(C) 改發射/渲染座標語義**（caret_rect.y 或渲染器 baseline 對齊成行中）： blast
  radius 最大（goldens/正向腿契約），另卡。
續工（裁示後）：依裁項收攏考卷 → request_review。殼層 Enter 卡（PM 裁決 2）與本裁
示互不阻塞。

## 6. 重放指令
```
cd shells/web && npm run build:wasm
cargo test -p quire-layout --test u4m_linebox_hit          # 修後 7 綠
U4M_LINEBOX_BUGGY=1 cargo test -p quire-layout --test u4m_linebox_hit   # 回塞恰 4 紅
cd <repo 根> && ~/.local/chrome-control/bin/python \
    shells/web/tests/t_f073cd6c_click_matrix_live_probe.py  # 真滑鼠矩陣（vite 5199 自管）
~/.local/chrome-control/bin/python \
    shells/web/tests/t_f073cd6c_shell_enter_evidence.py     # 殼層 Enter 呼叫鏈取證
```
