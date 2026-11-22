# U4f 交付報告：表格頁跑版＋caret 全面死區（t_93937dc5）

用戶實測檔（15 頁、3 表、208 段、18746 字；檔名與路徑不入檔，覆核由 PM 親供）。本報告與所有探針輸出只含結構度量，零內文、零機構名、零業務數值。

## 一、根因

### 根因 A（跑版）：表格內部帶尺與全域帶尺兩套單位

K6 帶尺細化後全域帶尺可低至 0.69285715pt／帶（用戶檔 fs=11、line_spacing=0.062987015）。`core/layout/src/table.rs` 舊 `layout_table` 開頭 `let line_h = line_h.max(1.0)`（修前行號 1075）是 Phase3-1b 粗帶尺時代（帶尺恆 ≥16.8pt）的防呆鉗，當時恆等 noop；細尺態下它把表格**內部**繪製帶尺無條件復辟成 1pt／帶，而 packer `pack_run` 的 `FlowItem::Table` 分支（`core/layout/src/lib.rs` 1513～1548）用**全域**細尺換算切片 y 並按 `used` 帶數計帳。同一帶數、兩套單位：表格按 1pt／帶畫高、按 0.69pt／帶計帳 ⇒ 表格高度少計 30.7%（1／0.69285715＝1.4433），表格下方的正文行從「表格還有 30.7% 高度沒畫完」的 y 起排 ⇒ 表格列與正文段同 y 區交疊發射（用戶截圖實態）。

紅燈實錄（`U4F_TABLE_LINEH_BUGGY=1` 回塞態，燈 1／燈 2）：切片繪製高 160pt vs packer 計帳高 110.857pt（比值 1.4433）；正文行 y=183.55 落入表格跨度 [72.7, 232.7)。

修法＝內部帶尺與全域同源（嚴禁平行另造尺），防呆改擋非有限／非正值（原鉗的真正防呆語義），細帶尺原樣穿透；粗尺（≥1pt）路徑逐位元組不變。

### 根因 B（caret 死區）：反向反查的「起帶完全相等」判制

`core/layout/src/lib.rs:2756` `caret_at` 舊制 `p.line_bands.iter().position(|&b| b == band)?`。粗帶尺（每行 1 帶）下 `line_bands` 恆為連續 0..n，任何 y 都命中，判定制看起來無害；細帶尺下每行佔 `para_band` 帶（用戶檔 23 帶／行），`line_bands` 成稀疏點集（0, 23, 46, 69, …），點擊 y 換算出的帶幾乎必然落在兩行之間 ⇒ 整頁 None ⇒ 外殼 `pointerdown`（`shells/web/editor/js/qword-ui.js:381`）拿不到 caret ⇒ `adapter.caret` 不更新 ⇒ 打字／退格／選取全失效。用戶口述「完全無法編輯也無法改變 cursor 位置」＝此缺陷的直接後果。

紅燈實錄（`U4F_CARET_AT_BUGGY=1` 回塞態，燈 3）：細尺命中率 66.7%（24/72 死，死樣本全部 dy=5.54pt＝帶中段）；用戶檔修前 raw 頁 1／3 ＝ 0/55 全死。

修法＝行跨度包含判制：行的文字跨度＝[起帶, 起帶＋該段每行帶數)（`grid.bands_of`），點在跨度內即命中該行；表格帶／段前段後距帶／空帶仍回 None（既有「表格帶必 None」契約考卷零擾動）；粗尺退化單帶＝與修前逐位元組等價（燈 4 零擾動守門實證）。

### 正逆映射是否同源：**同源**（卡面重點嫌疑排除）

正向 `caret_rect`（lib.rs:2718）與反向 `caret_at`（lib.rs:2756）用**同一** `line_h` 與**同一** `page.line_bands` 換算 y。證據：

- 代碼行號：正向 y＝`margin_top + line_bands[li] × line_h`；反向 band＝`(y − margin_top) ÷ line_h` 再查**同一** `line_bands`（修前斷點僅在反向的完全相等判制，非映射尺）。
- 實證（wasm 終態）：全檔掃 x∈{100,120,200,300,420,500}、y=72..720 步 4pt 共 8280 個 caret_at 命中樣本，逐一 `caret_rect(para,cp)` 全部可還原（不可還原＝0）；反向 100 個可定位段落行錨點正→反→正互逆零失配。
- 考卷：`u4f_band_ruler.rs` 燈 3(c) `caret_rect→caret_at` 互逆斷言（caret_rect 給出的每一點反查必還原同段）。

## 二、修復 commit 鏈（wt/t_93937dc5，未 push）

1. `f85dc06` 測試先紅：`core/layout/tests/u4f_band_ruler.rs` 四盞燈（燈 1 繪製高／計帳高同單位、燈 2 表後正文不落表格 y 區、燈 3 反查命中>70%＋行距帶全命中＋正逆互逆、燈 4 粗尺零擾動）。BUGGY 回塞鎖 `U4F_TABLE_LINEH_BUGGY=1`（燈 1/2 紅）、`U4F_CARET_AT_BUGGY=1`（燈 3 紅）、`U4F_COARSE=1`（全綠）各別親跑坐實鑑別力。
2. `fe3139f` 修復根因 A：`core/layout/src/table.rs` 帶尺同源自 `max(1.0)`。
3. `a4f4f61` 修復根因 B：`core/layout/src/lib.rs` `caret_at` 行跨度包含判制。
4. `f920bad` 探針 v2（判定制收斂，見 §四）：`shells/web/editor/tests/u4f_table_overlay_caret_probe.mjs`。

驗證：`cargo test --workspace` **1124 passed / 0 failed**（含尾註頁 None、表格帶 None 等既有 caret 契約考卷零擾動）；`cargo clippy -p quire-engine -p quire-layout --all-targets` 零警告；`cargo fmt --all --check` 淨。

## 三、探針終態輸出（wasm 重建後，真瀏覽器檔案選擇器路徑開檔）

PM 覆核重跑法（與修前基線對分）：

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cd <repo> && (cd shells/web && wasm-pack build --target web --out-dir pkg --release)
node shells/web/editor/tests/u4f_table_overlay_caret_probe.mjs <用戶docx路徑> --json /tmp/u4f-user-final.json
node shells/web/editor/tests/u4f_table_overlay_caret_probe.mjs SAMPLE
```

用戶檔終態（x=120、y=72..720 步 12pt，55 樣本／頁）：

| 頁 | BODY 命中 | CELL（v1 契約） | raw（對分基線） | 正文×框線 | 真字距 text×text |
|---:|---:|---:|---:|---:|---:|
| 0 | 30/30 (100%) | 14/22 | 45/55 | 0 | 0 |
| 1 | 10/10 (100%) | 4/32 | 14/55 | 0 | 0 |
| 2 | 15/15 (100%) | 9/16 | 26/55 | 0 | 0 |
| 3 | 5/5 (100%) | 4/39 | 10/55 | 0 | 0 |
| 4 | 5/5 (100%) | 1/2 | 8/55 | 0 | 0 |
| 5 | 11/11 (100%) | 5/5 | 16/55 | 0 | 0 |
| 6 | 3/3 (100%) | 0/1 | 4/55 | 0 | 0 |
| 7 | 31/31 (100%) | 22/22 | 55/55 | 0 | 0 |
| 8 | 5/5 (100%) | 2/3 | 7/55 | 0 | 0 |
| 9 | 29/29 (100%) | 11/23 | 40/55 | 0 | 0 |
| 10 | 23/23 (100%) | 13/25 | 37/55 | 0 | 0 |
| 11 | 36/36 (100%) | 18/18 | 54/55 | 0 | 0 |
| 12 | 36/36 (100%) | 19/19 | 55/55 | 0 | 0 |
| 13 | 44/44 (100%) | 11/11 | 55/55 | 0 | 0 |
| 14 | 31/31 (100%) | 8/8 | 40/55 | 0 | 0 |

- **跑版＝0**：`U4F_PROBE=GREEN`，正文×表格框線重疊全 15 頁＝0，真字距 text×text 重疊全 15 頁＝0（修前：12/15 頁 text×text 紅、頁 1=35、頁 3=21）。
- **caret 命中**：BODY 歸因（該 y 有可定位段落 `caret_rect` 行跨度覆蓋）15/15 頁 **100%**（門檻 70%；修前頁 1/3 raw 0/55 全死）。
- **SAMPLE 對照組維持 GREEN**（2 頁：BODY 39/39、33/33；raw 55/55、47/55）。

## 四、殘留項逐條正當性（卡面要求）

探針 v1 的 raw 命中率口徑混入兩類**非缺陷**樣本，v2（`f920bad`）如實拆帳，raw 照輸出供對分：

1. **CELL＝表格格內段**（如頁 1 raw 14/55 中 32 格內樣本僅 4 命中）：`caret_at` 對格內段回 None 是 v1 既定契約（格內編輯走 marker-grid 路徑，`docs/word-ui-u4*` 系列與 lib.rs:3793 「表格帶上無文字行，反查必 None」考卷俱在），非死區。全部 108 個表格段的 `caret_rect` 亦 None——正逆對格內段**同為** None，無單邊斷裂。用戶點格內時游標不動屬「表格內 caret」產品缺口，非本卡 raw 死區形態，已另開 triage 卡給 PM 定範圍（見 §六）。
2. **EMPTY＝正當空白**：頁面上無任何 DL 文字行的 y（頁邊、段距、表格前留白），None 屬 Word 同義（點空白不落 caret），不計死區。
3. **探針自身三缺陷（v1 誤報源，v2 已修）**：(a) Rule 幾何欄位是 `y1/y2`，v1 讀 `y_pt` 恆 undefined ⇒ rule×text 判據自落庫起恆零（假綠）；(b) text×text 用 0.6×字級粗估寬，對 CJK（真寬≈1.0×字級）高估 67% ——修前用戶檔 49 對 flag 裡 48 對是偽訊號；且 DL 的 `x` 是墨跡起點、caret_rect 是推進框，跨框比較自相矛盾——頁 12 y104 對（曾判 5.5pt「真重疊」）以 caret_rect 同框復測 r1.x(左)=r0.x(右)=460.5，**零縫相接，非重疊**；(c) 覆蓋判據容差需含 baseline 上升部（1.2×行高），否則行尾正文行誤判格內。
4. **unmeasurable 對**（探針 flag 但引擎真字距不可測）：逐條查驗全部至少一側為格內段（caret_rect None，v1 契約），無正文×正文不可測殘留。

## 五、分界鑑別（引擎 vs 外殼）

雙缺陷根因**俱在引擎**（quire-layout），外殼零改動：`qword-ui.js` pointerdown 只是 `caret_at` 的回讀端，修後即通。外殼側無需返修。探針 v1 的兩個判據缺陷屬測試面（已收斂），不影響根因結論。

## 六、後續（已開卡，不吞）

- 表格格內 caret（點格內落 caret＋格內打字游標）＝v1 契約缺口的產品化，屬跨引擎×外殼功能（caret_rect/caret_at 需認表格段），開 triage 卡請 PM 定範圍與優先序。

## 七、隱私自查

探針與考卷輸出欄位只有坐標／計數／段落號；用戶 docx 未拷入 repo（`.gitignore:3` 只 ignore shells/web/pkg；docx 走路徑引用）；本報告與 commit message 零內文、零檔名。`git log -p` 掃零洩漏。
