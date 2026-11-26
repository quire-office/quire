# U4m：caret_at 行框歸屬改**行框墨跡真值制** — 交付報告（前提已反轉定案、PM 已裁）

任務：t_f073cd6c（ow-core）｜分支：`openword/t_f073cd6c-caret-linebox`（基線＝main `e758668`；未 push）
尾標：U4M_LINEBOX｜狀態：**全閘門綠＋真滑鼠矩陣全綠，request_review**
最高指導＝卡面 body 首區【PM 最終裁示 11-26 04:45】＋comment【PM 裁決 11-26 03:55】。

## 0. 定案前提（本版報告的座標口徑）

`Text.y`／`caret_rect.y` ＝ **行框頂**，不是基線。三支出貨渲染器（`shells/web/demo/main.js:411`、
`shells/web/editor/vendor/renderer.js:338`、`shells/native/src/render_svg.rs:144`）皆把**字形基線**
畫在 `Text.y + size_pt`。卡面原稿「Text.y＝基線」為 PM 拆卡時寫錯的前提，PM 已承擔改卷責任
（比照 main `k29e_char_format` 過期前提補正與 K29d 前提裁決先例）。用戶症狀（基線以上點擊錯判
上一段）**屬實有效**，判制邊界缺陷有效，修法依反轉後前提落地。

## 1. 落地：命中中介帶＝行框墨跡真值制

`caret_at` 反查的行框歸屬，最終定案為（PM 裁示二，取代原稿「行距中線切分」與 run 88 已實作的
`U4M_LINEBOX` 中線制）：

- 每行命中區間＝**該行模型墨跡上下緣**；相鄰行切分點＝上行墨跡下緣與下行墨跡上緣之中點；
  行間空白歸最近墨跡行。
- 切分點夾到**不越下行行框頂**（dense leading／混級致兩行模型墨跡物理重疊時的退化態），
  上下兩行用**同一條邊界算式**算同一條邊 ⇒ 無死區、正向腿落點不會變死區。
- 首行上緣＝`行框頂 − 上緣越出量`（保留命中面）；末行下緣＝`起帶 + span`。
- 尺源唯一：`core/layout/src/shaping.rs::ink_em_ratios()` 吃內嵌字型 hhea metrics
  （`asc/em = 1.16000`、`desc/em = 0.28800`，`OnceLock` 常數化），與斷行／渲染／caret **同一把尺**，
  未造第二把尺。正向腿 `caret_rect` 零改動。
- 回塞鎖 `U4M_LINEBOX_BUGGY` 比照 `U4F_CARET_AT_BUGGY` 口徑。

源碼＝`core/layout/src/lib.rs`（`caret_at` 中介帶 + `ink_edges`/`prev_same`/`next_same`/`split_between`）、
`core/layout/src/shaping.rs`（`ink_em_ratios`/`ink_offsets_pt`）。

## 2. 為什麼不是跨度制、也不是中線制（PM 逐行復算＋canvas 像素真值）

同批 canvas 像素墨跡（`docs/word-ui-u4m-click-evidence.json`，38 行，第 0 頁混級）、純幾何三式並跑
（探針燈 C 實錄，本輪重跑）：

| 規則 | 「點到自己字形卻判別段」錯判 |
|---|---|
| S＝修前跨度制 `[y_i, y_{i+1})` | 0 點（PM 復算 1/38 破口，本檔樣本未落在破口帶） |
| M＝作廢的行距中線制 | **207 點**（每行字形下緣中位 5.61pt、最深 14.01pt 被判給下一行） |
| I＝本卡行框墨跡真值制 | **0 點**（模型自證，且比跨度制更強：堵死跨度制殘餘破口） |
| 引擎實測（真滑鼠反查） | **0 點** |

用戶最常見落點（基線以下、字形下半）在中線制下整排錯段，故「維持現狀收案」（原選項 A）會把新
使用者 bug 收進主線——PM 否 A、否 C（改發射座標語義觸及三支渲染器＋全部 y 口徑），採 B 意向但以
**正常 commit 前瞻修正**執行，未動 git 歷史（鐵律）。

### 「無法任意按 Enter 創建新段落」的實測根因鏈（殼層，PM 另卡）
殼層取證（`shells/web/tests/t_f073cd6c_shell_enter_evidence.py`，真瀏覽器真鍵盤，
`docs/word-ui-u4m-shell-enter-evidence.json`）：main 態 demo 殼 Enter → `beforeinput(insertParagraph)`
→ `commitText("\n")` → `insert_text_at_caret(para, cp, "\n")`——**段內字面換行，段數 47→47 不增**，
`apply_mutation_json`（SplitParagraph 正產線入口）從未被呼叫。此为編輯面第二根因（PM 裁決四：
確認但不併修，收案後另開卡；本卡未碰殼層）。

## 3. 考卷與實錄（全部親跑）

**卡面考題 1／2／4（cargo；`core/layout/tests/u4m_linebox_hit.rs` 七盞）**
- `cargo test -p quire-layout --test u4m_linebox_hit`：**7 passed 0 failed**（依反轉前提重寫）
  - 燈 1「每一行**自己墨跡**內任一點反查歸屬＝該行」＋cp 必落本行碼點區間（PM 裁示三：只收窄不弱化）
  - 燈 2 逐行掃 y（含行框頂以上本行模型墨跡）全程還原同段同列
  - 燈 3 正逆互逆：`caret_rect` 的 y 上下掃，換段邊界以 **ink 邊界**表述（原稿「跨中線才換段」作廢）
  - 燈 4 表格帶／無主空帶仍回 `None`（既有契約零擾動）
  - 燈 5 正向腿 y 與發射同源（護欄，BUGGY 態恆綠）
  - 燈 6 點擊段 ↔ caret 段全等（Enter 分錯段的引擎面等價命題）
  - 燈 0 靶地形自證（缺陷地形必須存在）
- `U4M_LINEBOX_BUGGY=1` 回塞態：**恰 4 盞鑑別燈紅（燈1/2/3/6）**，前提／護欄燈（0/4/5）綠＝鑑別力在
- `u4f_band_ruler` 燈3(b) 依裁示三改寫為「墨跡切分點歸屬」＋**判別性斷言**（中線以上、墨跡切分以下
  的帶現行制必屬本行＝作廢中線制的本地證據）；燈3(c) 正逆互逆臂與燈4 粗尺臂**零改動**
  （`U4F_CARET_AT_BUGGY=1` 既有燈仍紅＝原鎖未弱化）
- `t4aa_cell_caret` 7 passed：**正逆互逆契約零擾動**（PM 裁示三：不得動；實測未動）
- `cargo fmt --check` 淨／`cargo clippy -p quire-layout --all-targets -D warnings` 淨／
  `quire-layout` 160 passed／**workspace 132 blocks 1143 passed 0 failed**
- 簡繁：opencc s2t 掃本片新增行 524 漢字行，殘留僅白名單字（吃／才／核／游），無簡體洩漏

**卡面考題 3（真滑鼠 CDP 矩陣，`shells/web/tests/t_f073cd6c_click_matrix_live_probe.py`；
真 Chrome＋真 `page.mouse.click`＋實際 `pkg/` wasm 產物；style_cascade.docx 第 0 頁混級 38 行）**
- 燈 A1（Tripwire 主燈）：**304 次真滑鼠點擊**——點到自己**實測像素墨跡**（canvas ground truth，
  非被測反查碼＝非循環）→ 點擊段全等＋cp 落本行碼點區間：**錯判 0 例**；含「行框頂以上」缺陷面取樣
  → **死區 0 例**（caret_at 不返 None）
- 燈 A2（卡面考題2，以 ink 邊界表述）：**492 次點擊**逐點掃相鄰兩行像素墨跡之間的空白帶
  （跨段配對 19／段內配對 18）→ 零死區＋不野跳＋**段內零換段**＋**跨段恰一次換段**＋換段點不穿字：
  **破口 0 例**
- 燈 B（缺陷面點擊定 caret → 真產線 `apply_mutation_json(SplitParagraph)`）：18 靶 →
  **段數增一＋新建段落＝點擊段被切開的後半，全等**
- 燈 C（升格為 **gate**，PM 裁示二）：引擎實測「點到自己墨跡卻判別段」**必 0 點**＋規則 I 純幾何
  對照同 0 點；規則 S／M 對照記帳（§2 表）
- 證據：`docs/word-ui-u4m-click-evidence-postfix.json`（修後態逐點 y／實測墨跡／caret 全量，
  rows 與修前 PM 復算輸入 `docs/word-ui-u4m-click-evidence.json` **逐欄全等**，僅計數欄變化：
  `ink_misattributed_points` 111 → **0**）＋截圖 `docs/word-ui-u4m-click-evidence-postfix.png`
  （`build/u4m-click-matrix.*` 為同檔產出源）
- 探針紀律：切分位置不預設任何一把尺（避免考卷與引擎互抄），改以**像素墨跡真值**定斷言；實測換段點
  與「兩行像素墨跡相鄰邊中點」偏差 med 0.44pt／max 2.14pt（引擎用 hhea 模型墨跡，與實測字形盒可差
  <1pt）以 NOTE 記帳——換段點落在空白帶內＝不穿字，契約成立。探針自身三處假紅根因（靶誤錨
  `Text.y+size`＝兩把尺混用／行首 cp 邊界回上一行 y／混級檔 pitch 必逐邊取）在檔頭，覆核者可據以重放。

**真鍵盤 Enter（考題3 後半的字面路徑）**：demo 殼按 Enter 不增段（§2 殼層根因鏈，PM 另卡），故燈 B
打正產線 SplitParagraph 入口（PM run-85 記帳裁示）；字面鍵盤路徑證據另檔留給殼層卡。

## 4. 修訂史簡表（本卡 commit 鏈）
| commit | 內容 |
|---|---|
| db8f901 | run 85：caret_at 中線制＋u4m 六盞＋u4f 燈3(b) 改寫（PM 預警保護 commit） |
| dbfae0f | run 85 殘留探針保護 commit（PM 代） |
| 02d8a57 / 8703a04 | 點擊矩陣探針重寫至卡面口徑＋殼層 Enter 取證＋規則 M/S 歸因盞 |
| 442eeba | 簡繁收案（opencc 白名單紀律） |
| b49f92f | 交付報告＋證據三檔 |
| （本輪） | **前提反轉落地**：caret_at 改行框墨跡真值制＋`shaping::ink_em_ratios`/`ink_offsets_pt`＋u4m 七盞依反轉前提重寫＋u4f 燈3(b) 改墨跡切分＋探針改像素真值制（A1/A2/C 升 gate）＋本報告改寫 |

## 5. 已裁事項（原「待 PM 裁示」章，依 04:45 裁示改寫）
- **A 否**：維持中線制收案會把「每行字形下緣錯段」收進主線（PM 逐行復算 38/38 破口）。
- **C 否**：改發射／渲染座標語義 blast radius 觸及三支渲染器與既有全部 y 口徑。
- **B 意向，以正常 commit 前瞻修正**（未走 git 歷史操作，鐵律遵行）：命中中介帶已改為墨跡真值制，
  效果＝U4f 跨度制的正確歸屬 **＋** 修掉跨度制殘餘 1/38 破口（比修前契約更強）。
- 卡面四條考題＋回塞鎖全部滿足並綠（§3）。殼層 Enter 返修（PM 裁決四）與 `SplitParagraph` ill-formed
  XML 返修（t_e93fb47b）與本卡無依賴、本卡未碰 `core/engine-api/`。

## 6. 重放指令
```
cd shells/web && npm run build:wasm
cargo test -p quire-layout --test u4m_linebox_hit          # 修後 7 綠
U4M_LINEBOX_BUGGY=1 cargo test -p quire-layout --test u4m_linebox_hit   # 回塞恰 4 紅（燈1/2/3/6）
cargo test -p quire-layout --test u4f_band_ruler --test t4aa_cell_caret # 4＋7 綠（燈3(b) 已改墨跡切分）
U4F_CARET_AT_BUGGY=1 cargo test -p quire-layout --test u4f_band_ruler   # 既有燈仍紅（原鎖未弱化）
cd <repo 根> && ~/.local/chrome-control/bin/python \
    shells/web/tests/t_f073cd6c_click_matrix_live_probe.py  # 真滑鼠矩陣（vite 5199 自管）→ U4M_CLICK_MATRIX_OK
~/.local/chrome-control/bin/python \
    shells/web/tests/t_f073cd6c_shell_enter_evidence.py     # 殼層 Enter 呼叫鏈取證（另卡用）
```
