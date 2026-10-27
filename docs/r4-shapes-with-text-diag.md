# R4-diag — shapes-with-text 劣化歸因（0.849 → 0.650）

- 卡：t_738df8ca（report-only，零引擎觸碰；本文為本卡唯一新檔）
- 日期：2026-10-27；引擎區間實測：`fb06767..75d3648` 全 commits（31+基線，逐檔實跑）
- 檔：`corpus/poi/test-data_document_shapes-with-text.docx`（11 段：1 宿主段掛 7 錨定圖，5 框有字）

---

## 0. 結論速覽（TL;DR）

1. **肇事 commit＝`591bed8`（K4e2 框內文字分離發射），不在本卡聲明的 `fb06767..75d3648` 區間內。**
   實測：區間內可構建的 **28/31 commit＋基線 fb06767 全部 0.650**（無任何一位移點；
   另 3 個 wip 保護點 2c3c324/bbd5003/8122399 為 PM 已註明的編譯紅，跳過）；
   `b6b3dc5`（K4e2 前一個引擎 commit）＝**0.849**、`591bed8`＝**0.650**。
   卡面 base 分數（0.849，scores.jsonl 2026-10-23）是 **K4e2 合入前**生成的——
   「base 來源 2026-10-23 vs 區間起點 fb06767（2026-10-25）」本身就跨過了肇事點，
   本卡是**歸因報告的區間標錯**，不是新一輪 merge（L1/L2/L3/M1）造成的劣化。
2. **K4e2 引入的缺陷有二，疊加造成 −0.199（內容）：**
   - **R1 run 錯位**：`emit_txbx_texts` 用
     `paras[host].txbx_runs[ti]` 取樣，但 `txbx_texts` 是**跨圖累積**的、
     `txbx_runs` 卻是**每圖一組**的（多圖宿主段兩者不同長度/不同對位）→
     第 2、3 框拿第 1 框的 run 來切段：`emit_line_run_items` 以錯位 run 的碼點區間
     截行，把整行在**詞中**鋸成兩段（`"An ellipse with t"＋"ext"`、
     `"A square shape wi"＋"th text"`），且顏色/粗斜錯採別框的樣式
     （白字框採了首框黑字 → dump 實據 `color_argb=4278190080` 黑）。
   - **R2 發射錯序（主兇，−0.19）**：框內文字先按**錨定段落內圖形文件序**插進 items，
     發射尾再被 `sort_display_items` 按 y 重排——LO 抽取序是「先正文流（方框 p1）後
     浮動錨（y 序）」，我方變成「y 序在前、方框在後」。SequenceMatcher 對**跨行調序**
     直接吃順序罰分。實測拆解見 §4。
   - `break_lines` 本身**無錯**：實測餵真實框寬時斷行與 LO 逐詞一致（§3.3）。
3. **修復路徑已量化**：R1＋R2 全修後實測 **content=1.000 / layout=0.985**；
   單修 R1 只回 0.655、單修 R2 得 0.992——**R1/R2 必須同卡共修**（見 §4/§5）。

---

## 1. bisect 實測表

方法：LO PDF 快取一次（Windows soffice，與儀表板同口徑），引擎側逐 commit
`git worktree`（scratch 區，主 checkout 零觸碰）＋獨立 `CARGO_TARGET_DIR` 構建，
`quire-cli dump` → `render_lab.compare` 對 LO 快取重算。腳本見 §7。

### 1.1 卡面聲明的區間（全部 0.650，無位移點）

| commit | 內容分 | 備註 |
|---|---:|---|
| fb06767（卡面「base」） | **0.650** | 卡面聲明的區間起點，**已帶病** |
| 2c3c324 / bbd5003 / 8122399 | — | BUILD-FAIL（PM 已註明的 wip 編譯紅保護點，跳過） |
| K4c-rect 鏈（835d94d, a157be3, 6f408ef, 2ad512e） | 0.650 | |
| K4e2 已在祖先（c3cdf0d, 4a382c4） | 0.650 | |
| L2 鏈（bbd5003*, cfae8fd, 4e131dc, c0e0f9d, 3e38219, bf4772d） | 0.650 | |
| L3 鏈（b079bd6, abb59c9, 3787412, e0881ac, 3f96a9c） | 0.650 | |
| L1 鏈（e7dbde9, 491f100, 3fe1086, f1c6cc8, f71ab34, e486389, b407f3a） | 0.650 | |
| M1 鏈（e741e25, 5c4d9f2, 8122399*, ba9d8b0, b7fff52） | 0.650 | |
| 75d3648（tip） | 0.650 | 與 lo-final/scores.jsonl 2026-10-27 記錄一致（覆核通過） |

> 每檔 ow 行序列在整個區間逐位元組相同（9 行，含 mid-word 雙發），
> 五個 merge 對本檔**零擾動**——儀表板本輪把它列為「無既有歸因報告的劣化檔」
> 是錯案：它不是本輪劣化，是 **10-24 K4e2 入 main 時漏網**（當時 A/B 宣告
> 「seed42 零劣化、真實 +0.026」——無論本檔當時有無被 seed42 抽中，單檔
> −0.199 都被層平均稀釋而未觸發回歸閘；修復卡落地時應把本檔加入儀表板
> 守門名單，見 §5）。

### 1.2 前追（卡面區間之前）

| commit | 日期 | 內容分 | 說明 |
|---|---|---:|---|
| be1185b（K4b，10-23 波段 tip） | 10-23 14:08 | 0.849 | K4b/K4a/K5 皆不碰本檔 |
| **b6b3dc5**（K4a merge，K4e2 前最後一個引擎 commit） | 10-23 22:22 | **0.849** | 5 行、無斷行、無 mid-word |
| **591bed8（K4e2）** | 10-24 08:57 | **0.650** | **肇事點**；9 行 mid-word 雙發現形 |
| ea92bdd（K4c-image merge） | 10-25 02:13 | 0.650 | 承 591bed8（N1 卡的「K4c-image 證偽」在本檔再證一次） |

`b6b3dc5 → 591bed8` 是本檔唯一的一次分數跳變（0.849→0.650），**單點鎖死 K4e2**。
K4e2 當時的「LO 儀表板 seed42 零劣化」結論對本檔不成立——seed 樣本未覆蓋或
總平均稀釋，非儀表板口徑錯誤。

---

## 2. LO 真值清單（pymupdf 實讀 `lo.pdf`，seed 同儀表板）

頁 0（612×792，1 頁），文字 span（字級全 11pt，LiberationSerif）：

| # | span bbox(x0,y0)–(x1,y1) | 文字 | 宿主框（源檔幾何） |
|---|---|---|---|
| 1 | (97.30, 76.05)–(172.40, 88.23) | `Floating text box` | TextBox 1：183×45pt，pos(18,0) |
| 2 | (100.30, 140.55)–(237.35, 152.73) | `A square shape with text inside` | Rectangle 2：158.25×64.5pt，pos(21,64.5)，**單行不拆** |
| 3 | (350.45, 110.90)–(439.65, 123.08) | `An ellipse with text ` | Oval 3：146.25×69pt，pos(249.75,24.75) |
| 4 | (350.45, 125.40)–(376.70, 137.58) | `inside` | 〃（第二行） |
| 5 | (186.10, 383.75)–(279.17, 395.93) | `Where some contain ` | 群組 TextBox 8：95.25×42.75pt |
| 6 | (186.10, 398.25)–(202.57, 410.43) | `text` | 〃（第二行） |
| 7 | (156.85, 352.20)–(236.57, 364.38) | `A group of shapes` | 群組 rightArrow：175.5×51pt |

要點：
- **LO 抽取序 ≠ y 序 ≠ 完全文件序**：#1→#7 中「方框(正文流)」在前，其後依 y 為
  楕圓；群組兩框 LO 內容流把 Where(383) 排在 group(352) **前**（pymupdf 預設
  natural order＝LO 寫 PDF 的內容流序，非 bbox 排序）。我方若按「宿主段流序＋
  錨定圖文件序」發射（rightArrow 在 TextBox 8 前）得 0.849 水位；要滿分 1.000
  需連群組子框序也對齊 LO 內容流（過度擬合風險，見 §5 R2 註）。
- 楕圓框斷行點在 `text | inside`（詞邊界），**不是** mid-word。
- 方框 158.25pt：LO 整行 137pt 單行容納（11pt 字）。
- `Where some contain text` 在 95.25pt 框斷在 `contain | text`（詞邊界）。

---

## 3. 我方 display list 解剖（cur=75d3648 vs base=b6b3dc5）

### 3.1 cur 實測（dump 全文，Text 項；Rect×7＋Rule×28 兩態逐位相同）

| 序 | x | y | size | color | text |
|---:|---:|---:|---:|---|---|
| 1 | 72.00 | 72.00 | 12.0 | — | `""`（宿主空段） |
| 2 | 93.00 | 76.00 | 11.0 | 黑 | `Floating text box` |
| 3 | 324.75 | 100.75 | 11.0 | 黑* | `An ellipse with t` ← mid-word 鋸 |
| 4 | 405.79 | 100.75 | 11.0 | 黑* | `ext` |
| 5 | 324.75 | 114.65 | 11.0 | 黑* | `inside` |
| 6 | 96.00 | 140.50 | 11.0 | 黑 | `A square shape wi` ← mid-word 鋸 |
| 7 | 187.94 | 140.50 | 11.0 | 黑 | `th text` |
| 8 | 96.00 | 154.40 | 11.0 | 黑 | `inside` |
| 9 | 75.00 | 328.75 | 11.0 | — | `A group of shapes` |
| 10 | 75.00 | 342.65 | 11.0 | — | `Where some contain text` |

\* 楕圓框源檔宣告白字（`w:color FFFFFF`），dump 卻發黑——**樣式錯採**實據：
`txbx_runs` 錯位使楕圓框採了首框（黑字）的 run。
（cur 與 75d3648 逐位元組一致、亦與 fb06767 逐位元組一致＝區間零位移覆核。）

### 3.2 base=b6b3dc5 實測（K4e2 前）

Text×6：5 行整句平化進**正文流**（x=72 版心左緣，y=85.9/113.7/127.6/155.4/169.3
正文帶序：Floating→square→ellipse→group→where）→ content 0.849（靠 content_key
忽略座標/換行換來）。Rect×7＋Rule×28 **y 全部漂移**（150.4/138.45/291.1/243.1/
380.35/542.05…= 框 y 被平化段的帶位推歪 13.9–97.3pt）——K4a 佔位框在錨定段之前
就存在，但浮動 y＝錨定段帶＋posOffset，平化段把帶位推歪了。

**cur 的 Rect/Rule y 與 LO 錨點逐格對齊**（72/136.5/96.75/235.5/187.5/324.75/
444.75 ＝ 72＋posOffset，§2 真值全中）。即 K4e2 把**框形位置修對了**、把文字
如實落框（cur 楕圓字 y=100.75 在框 96.75–165.75 內 ✓），缺陷只在多框宿主段的
**run 取樣**（mid-word＋錯色）與**閱讀序**（sort 打散文件序）兩處（§3.4）。
修復**不是回退 K4e2**——回退連框位一起吐回去。

### 3.3 斷行器自證（排除 layout 嫌疑）

`core/layout/examples/r4_probe.rs`（scratch worktree 一次性探測）餵真實框寬＋
真實字串進 `break_lines`：

- 楕圓 `inner_w=140.25`、12pt 尺：`["An ellipse with text", "inside"]` ✓ 與 LO 同
- 方框 `inner_w=152.25`：`["A square shape with text", "inside"]`（LO 單行——12pt
  尺下 137.4pt>152.25pt 邊界附近，微差；**非** mid-word）
- `Where some contain text` 89.25pt：`["Where some", "contain text"]`（LO `contain | text`，微差）

即 **mid-word 絕非 `break_lines`/KP/UAX14 所為**。

### 3.4 mid-word 機制的實錘（引擎內埋點）

`emit_txbx_texts` 迴圈埋點（scratch 探測，`R4PROBE`）實測：

```
R4PROBE runs: t="A square shape with text inside" nruns=1
              runs=[("Floating text box", Some(11.0), …)]   ← 方框拿到了首框的 run！
R4PROBE runs: t="An ellipse with text inside" nruns=1
              runs=[("Floating text box", Some(11.0), …)]   ← 楕圓框也是
```

本檔宿主段掛 7 個錨定圖、5 框有字。解析層 `Paragraph::txbx_texts` 把 5 框文字
**展平累積**成 5 項，`txbx_runs` 卻**每圖一組**各掛各的（發射端
`paras.get(im.para).map(|pp| pp.txbx_runs.as_slice())` 後以 `runs.get(ti)` 取樣，
`txbx_texts` 的全域 index ti 對到**第 ti 個圖**的 run 組——首圖之後全部錯位）。
`emit_line_run_items` 以錯位 run 的碼點區間 `[r_start,r_end)`∩行截段，錯位 run
（"Floating text box"，17 cp）落在真實行（"An ellipse with text"，20 cp）上 →
`covered` 在 17cp 收口，殘段 `["ext"]` 以預設樣式**另發一項**——dump 的
`"An ellipse with t"＋"ext"`（17/3 碼點切割）分毫不差；`A square shape wi|th text`
同理。顏色錯採（白框發黑）同根。

**發射錯序（R2）**：`emit_txbx_texts` 對每框在宿主段迴圈內就地 push，宿主段之後
`sort_display_items` 按 (y,x) 重排文字項。LO 序是「正文流（方框, y=140）最先、
其後浮動 y 序、群組文件序」；我方 sort 把 y=76/100/114 的楕圓字**插到方框字前**、
又把群組兩框按 y 拆散——SequenceMatcher 跨行順序罰分即 −0.19 的主體（§4 拆解）。

---

## 4. 分數拆解（LO 快取＋render_lab.compare 重算，腳本 step8/step9）

| 情境 | content | layout | 說明 |
|---|---:|---:|---|
| 基線 b6b3dc5（實測） | 0.849 | 0.829 | 5 行、LO 序（平化正文流） |
| 現況 75d3648（實測） | 0.650 | 0.628 | mid-word＋錯序＋錯色 |
| 只修 R1（run 錯位） | 0.655 | 0.641 | 斷行正常但 y 序仍錯 → **幾乎白修** |
| 只修 R2（順序，仍 mid-word） | 0.992 | 0.856 | 順序即主兇的實證 |
| R1＋R2（文件序） | 0.849 | 0.832 | 回到基線水位（楕圓/Where 斷行微差殘留） |
| R1＋R2（LO 抽取序） | **1.000** | **0.985** | 全修上限 |

> 拆解要點：mid-word 拆行**本身**只傷 ~0.005（0.655 vs 0.650），調序傷 ~0.19
> （0.992 vs 0.849）——但 R2 單修的 0.992 是「錯序修正後 mid-word 兩段恰好拼接
> 回整句」的僥倖（content_key 忽略換行），顏色錯採與 SVG 視覺錯位仍在。
> **R1/R2 必須同卡共修**；只修其一等于沒修完。
> 「文件序 0.849」與「LO 抽取序 1.000」的差＝**群組內兩框的相對次序**
> （我方文件序 rightArrow→TextBox8；LO 抽取把 Where 排在 group 前，§2 要點）。
> 修到文件序即回基線水位；要滿分需把 sort 鍵對齊 LO 抽取序（見 R2 處方）。

---

## 5. 處方建議（供 PM 開修復卡）

| # | 檔案 × 函數 | 改什麼 | 紅燈建議 | 性價比 |
|---|---|---|---|---|
| **R1** | `core/engine-api/src/lib.rs::emit_txbx_texts`（呼叫點 1550/1589/3038）＋`core/ooxml/src/drawing.rs::attach_txbx`（1228） | **run 錯位**：`txbx_texts` 全域 index 對 `txbx_runs` 每圖一組 index 不同源。擇一：(a) 解析層 `attach_txbx` 讓 `txbx_runs` 與 `txbx_texts` **逐項同序展平**（texts.extend 時 runs 同位 extend，跨圖對齊由解析層收口）；(b) `InlineImage` 帶自家 txbx run 組、發射端直接吃框自帶 run，廢掉 `paras[..].txbx_runs` 二次取樣。(a) 改動小、不動發射簽名，建議先 (a) | 新檔紅燈：宿主段掛 ≥2 個有字框、樣式互異（本檔即天然 fixture，白/黑字對照），斷言第 2 框文字項 color=白、無 mid-word 雙發（同 y 同行兩 Text 項拼接必為來源整句） | **必修**（無它任何修復無效） |
| **R2** | `core/engine-api/src/lib.rs::emit_page` 文字迴圈＋`sort_display_items`（3485） | **發射序**：框內文字項應在宿主段正文之後、**按錨定圖文件序**插入且不被 y-sort 打散。最小改：框內文字項打「群組鍵」（宿主段序,圖序），`sort_display_items` 同組內維持文件序、組間按宿主段流序（sort 的 Text 鍵加一級組序）。或發射時就按文件序 push、對框內文字項**退出 sort**（改 sort 契約需查既有依賴：規則「非文字項恆在前」不動） | 本檔 content≥0.95；fdo76591（N1 卡同宿主段多框，雙發消失）；k4e2_txbx_emit 迴歸鎖改「同行拼接＝整句」斷言防 mid-word 復辟 | **必修**（與 R1 同卡） |
| R3 | `emit_txbx_texts`（2273–2281 inset/inner_w） | 11pt 宣告字級下 `TextMeasurer::new(seg_size.max(size_pt))` 把尺撐到 12pt（文件基準），楕圓/Where 斷行點與 LO 差一拍（0.849 場景的殘差）。改 `TextMeasurer::new(seg_size)` 需全量 A/B——LO 對 auto 行距的字級口徑屬 K6 權限，本卡只記錄不動議 | 本檔 layout≥0.98（即 1.000 情境的剩餘 0.015） | 緩（K6 合併考慮） |

**紅燈建檔紀律**：修復卡應先以 `corpus/poi/test-data_document_shapes-with-text.docx`
落三個斷言（(1) 5 框各為單一同質 Text 段、無 mid-word；(2) 楕圓白字；(3) 抽取序
＝LO 序），**先紅後綠**再動函數。

---

## 6. 紀律覆核

- 禁改 core/、shells/、scripts/：本卡**零觸碰**（git status 乾淨，僅 docs/ 新增本檔）。
- 禁 commit/push/checkout/reset/stash（主 checkout）：全部構建走 scratch
  worktree（`git worktree add --detach`）＋獨立 CARGO_TARGET_DIR；主 checkout
  與 worker worktree 未動（未 push）。探測用 examples/eprintln 埋點只存在於
  scratch 克隆，不入库。
- 禁往 corpus/ 放新檔：未放（測試用檔直接引用既有 corpus 檔）。

## 7. 可重跑腳本路徑（scratch，24h 後可能被清）

前置（本卡收工時已 `git worktree remove`，重跑先重建）：
```
S=~/.hermes/profiles/ow-qa/cache/scratch/r4
git -C ~/projects/quire worktree add --detach $S/worktrees/cur  75d3648
git -C ~/projects/quire worktree add --detach $S/worktrees/base fb06767
```

`/home/jerry/.hermes/profiles/ow-qa/cache/scratch/r4/`
- `step1_lo.py` — LO PDF 生成＋快取（`lo.pdf`/`lo_facts.json`）
- `step3_sweep.py` — 全 commits 掃描（雙 lane，產 `scores/sweep.jsonl`）
- `step5_backbisect.py` — 前追 bisect（`scores/back.jsonl`）
- `step7_lo_words.py` — LO span/word 級實讀（§2 表來源）
- `step8_decompose.py` / `step9_scenarios.py` — 分數拆解（§4 表來源）
- `worktrees/cur/core/layout/examples/r4_probe.rs` — 斷行器自證（§3.3）
- 引擎埋點 diff（§3.4）：`worktrees/cur` 內 `core/engine-api/src/lib.rs`
  （R4PROBE eprintln，僅 scratch）
