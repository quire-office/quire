# K7-cal — 水平字形 advance 寬表校準報告（fdo68607 9→8 頁的最後一哩）

> 卡：t_51b0a441（run73 矩陣＋判別、run78 覆核續跑、run80 收尾）。
> 方法論同 K6-cal（`docs/k6-lineheight-calibration.md`）：把「宣告族→LO 實際
> 渲染 face」的誤差從魔術常數救回實測查表；本卡是其**水平軸同構**
> （K6 修 pitch，本卡修 advance）。診斷鏈（為什麼根因在字寬）見
> `docs/k7-wrapwidth-diag.md` §3——本報告接其「處方」段落地。
> 數據檔：`docs/k7-advance-matrix.csv`（可重產，路徑見 §8）。

## 0. 結論表

| 項 | 定案值 | 依據 |
|---|---|---|
| `face_advance_scale` 表 | Calibri 0.8834／Cambria 0.9545／TNR 0.9648／**Arial 0.9627**／NotoSerif 1.0328；未命中＝1.0 零擾動 | §2 矩陣；Arial 以 B 軌覆核定案（§6） |
| fdo68607 右欄總行 | 我方 **311**（LO 316，容差 315±5 內）；BUGGY 退 1.0＝349 | §3 紅燈 1 實測 |
| fdo68607 頁數 | **8**＝LO（由 k7_wrap_width 嚴鎖；本表值改動不回頭破它） | §3 |
| Bug51170 全域正文行 | **183**（LO 185±5；0.9950 態 186→0.9627 態 183） | §3 紅燈 2 |
| Bug51170 逐頁 [29,32,…]±1 | **暫不鎖**——逐頁多吸根因＝LO 頁尾帶預留（容量軸），非 advance | §7 |
| patho_b 13.75 vs 14.85 | **兩者各自環境皆真**：LO 25.8＋真 PMingLiU→13.75／25.8＋WQY fallback→14.85。K6-cal §5「LO 版本漂移」論證修正為**face 替代環境依賴** | §5 |
| 環境穩性 | 真 MS face 環境下 Carlito==Calibri、LibSerif==TNR、LibSans==Arial hmtx 逐位同寬；Caladea≈Cambria（≤0.18%）→表值對兩類環境同成立 | §6.1 |
| 全閘門 | fmt／clippy -D／test（含 collab）／goldens／roundtrip 13+399+2／layering／wasm check+build **11/11 PASS** | §8 |
| 儀表板 A/B（seed20261022 n25 對 k7-sb 基線） | 可比 22 檔：21 檔逐檔內容分零變動；Bug51170 −0.007（PM 授權可動臂）；3 檔 LO 側 artifact | §8 |

## 1. 量測方法（雙軌）

- **A 軌（樣本串）**：標準拉丁樣本串（`a-zA-Z0-9 .,;:!?'-()/&%$#*+=<>[]{}`，
  87 字元）＋五個.script 子樣本（lower／upper／digits／punct／prose），
  fontTools 讀 hmtx，寬比＝LO 渲染 face 寬／quire 內嵌 Noto 子集寬
  （`core/layout/assets/quire-notosanstc-subset.otf`，引擎實際寬源），14pt。
- **B 軌（真實行）**：LO 25.8 轉 PDF 抽真實行文字（pymupdf line），逐行
  LO face hmtx 寬／Noto 子集 hmtx 寬取 **median**（過濾窄行：Calibri≥40pt、
  Arial≥60pt、CJK≥10pt 寬）。重點不是逐字元，是代表性字串的**聚合 advance 比**。
- **容差**：同一族 p5–p95 spread＝±2.1%（K7 diag §3）＝A/B 軌互核容差。
  差 ≤2.1% 用 A 軌（樣本串可重產性佳）；超出以 B 軌為準（樣本串組成分偏）。
- kerning 影響：LO Bug51170 行 bbox 寬／span bbox 寬和＝1.0000（n=200）
  ＝負 kerning 對聚合寬影響可忽略，hmtx 口徑足夠。

## 2. 矩陣（`docs/k7-advance-matrix.csv` 全欄）

| 宣告族→LO 渲染 face | A 樣本串 | A_lower | A_upper | A_digits | A_punct | A_prose | B 軌 median (n) | 表值 |
|---|---|---|---|---|---|---|---|---|
| Calibri→Carlito／真 Calibri | 0.8834 | 0.8599 | 0.8711 | 0.9132 | 0.9158 | 0.8764 | 0.8745 (416) | **0.8834**（Δ0.9% ✓A） |
| Cambria→Caladea | 0.9545 | 0.9202 | 0.9436 | 0.9977 | 0.9937 | 0.9158 | —（樣本缺） | **0.9545**（A 獨撐） |
| TNR→LiberationSerif | 0.9648 | 0.8662 | 1.0487 | 0.9009 | 0.9933 | 0.8725 | — | **0.9648**（A 獨撐） |
| Arial→LiberationSans／真 Arial | 0.9950 | 0.9233 | **1.0656** | 1.0021 | 0.9689 | 0.9672 | **0.9627 (188)** | **0.9627**（B 覆核，§6.2） |
| (theme 斷鏈)→NotoSerif | 1.0328 | 1.0354 | 1.0518 | 1.0072 | 1.0100 | 1.0287 | — | **1.0328**（A 獨撐） |
| (CJK 零宣告)→真 PMingLiU | 0.9367 | 0.9430 | 0.7865 | 0.9009 | 1.1592 | 1.0705 | 1.0498 (152) | —（CJK 不乘尺） |
| (CJK 舊基線)→WQY Zen Hei | 0.9560 | 0.8867 | 0.9283 | 1.0804 | 1.0174 | 0.9614 | — | —（CJK 不乘尺） |

- **CJK 方塊不乘尺（em 方塊鐵律）**：PMingLiU／WQY 的 CJK advance 均＝em
  （中／中＝1.0，fontTools 實測），LO 替代 face 同為方塊——乘尺會縮出裂縫。
  紅燈 `cjk_clusters_are_scale_immutable` unit 級零位移鎖。
- 單一常數擬合成立（未退化 script×粗細二因子表）：fdo68607（Calibri）、
  Bug51170（Arial）兩族 A/B 雙軌差 ≤1%／3.2% 判後，頁數與行數目標全中（§3）。
  唯一成分敏感族＝Arial（upper 樣本 +10.7% 抬高 A 軌），以 B 軌覆核吸收。
- Tahoma：LO 實際替代 face 本機查無實據，**不虛構**——留 1.0 後備＋TODO。

## 3. 紅燈落地與雙態實測（run80 親跑）

紅燈檔 `core/engine-api/tests/k7_advance_scale.rs`（4 盞）＋既有燈群：

| 燈 | 修復態 | `K7_ADVANCE_SCALE_BUGGY=1`（只退水平尺，幾何軸不動） |
|---|---|---|
| fdo68607 右欄總行 315±5 | **311** 綠（逐頁 [40,42,42,42,42,42,42,19]；LO 316） | **349 真紅**（逐頁 9 頁 [(41,18),(42,12),(42,19),(42,15),(42,20),(41,16),(42,18),(42,17),(15,7)]） |
| Bug51170 全域正文 180–190＋p6≥2 行 | **183** 綠（p6＝3 行） | 186 亦落帶內（本燈非回塞鎖，鎖總量不漂） |
| CJK 不乘尺 unit 守門 | 綠 | 綠（機制分離：env 不動 unit 層） |
| 零擾動臂（sdt_vml_textbox 1／RA001 7／tdf127814 2 頁） | 綠 | 綠（回歸鎖非抓漏燈） |

- 守門群零擾動：k6_auto_pitch＋k7_wrap_width（fdo68607 嚴鎖 8 頁）＋
  M1 錨 tdf124637 全綠（含在 `cargo test --workspace` 11 閘之一，§8）。
- 機制分離鎖意義：`K7_WRAP_WIDTH_BUGGY`＝幾何＋水平全回塞（9 頁）；
  `K7_ADVANCE_SCALE_BUGGY`＝**只**退查表——fdo 右欄燈必紅＋格線幾何燈必綠，
  證明本表值真的驅動換行（311↔349），而非搭幾何便車。
- Bug51170 逐頁對照（uniq-y 0.5pt、body y<780，口徑＝紅燈）：

  | | p0 | p1 | p2 | p3 | p4 | p5 | p6 | Σ | Σ\|Δ\| |
  |---|---|---|---|---|---|---|---|---|---|
  | LO 25.8 | 29 | 31 | 28 | 31 | 28 | 26 | 12 | 185 | — |
  | 我方 A 軌 0.9950 | 31 | 31 | 31 | 31 | 30 | 29 | 3 | 186 | 19 |
  | 我方 B 軌 0.9627 | 31 | 31 | 31 | 30 | 29 | 28 | 3 | **183** | **18** |

  p4/p5 各向 LO 收一行（字寬退→每頁吸行變少），p3 反向掉一行；全域總數
  186→183（LO 185：A 軌態 +1、B 軌態 −2，同帶內），逐頁 Σ|Δ| 19→18 微改善，
  最大差額全在 p6 尾頁（3 vs 12）＝容量軸（§7），非 advance 軸。

## 4. 落地路徑（engine 整合）

- 查表 `face_advance_scale`（`core/engine-api/src/lib.rs`，鍵＝宣告族小寫，
  未命中＝1.0＝零擾動守門，比照 K6 `has_fonts()＋calibrated` 閘）。
- 段級取族 `para_advance_scale`：首個**有文字** run 的有效字型族（無字 run
  不決定主族；全無宣告＝1.0）。scale≠1 僅作用在「宣告族命中查表」路徑——
  零宣告／內嵌即真族檔／未命中族檔全走 1.0 後備＝零擾動（紅燈 4 守門臂）。
- 斷行（ParaGrid）與發射段 x 累加、caret 互逆對**三者恆同尺**（同一量測模型），
  不出現「斷行用寬尺、發射用窄尺」的裂縫。
- 回塞鎖：`K7_WRAP_WIDTH_BUGGY=1`（全回塞）／`K7_ADVANCE_SCALE_BUGGY=1`
  （只退水平尺）雙開關，比照 R2_BUGGY／K6_AUTO_PITCH_BUGGY env 慣例。

## 5. patho_b 定案（13.75 vs 14.85 之爭）

**判定：非「LO 版本漂移」，是 face 替代環境依賴——兩值在各自環境皆真。**

- 現環境（LO 25.8.7.3，`~/.local/share/fonts/k6cal/mingliu.ttc` 已裝
  2026-10-28 10:32）重跑 `patho_verdict.py`：LO PDF span font 鐵證
  **PMingLiU 11pt ×151 span**（非 fallback），逐頁 pitch 眾數 **13.8**
  （p0 眾數 47/48 個間距、p1 48/49）＝13.75 口徑**可重現**。
- K6-cal §5（2026-10-27）量得 14.85 的當時環境**未裝真 PMingLiU**，LO 落
  WQY Zen Hei fallback：hhea lh 1.3496×11pt＝14.846＝14.85 ✓ 逐頁全中。
  同 LO 版本（25.8.7.3）兩值不同＝變因在 face 環境不在版本。
- 旁證（fontTools）：PMingLiU hhea lh＝1.1992、WQY＝1.3496、CJK advance＝em。
- **收口**：本卡標題掛「patho_b 定案」＝爭議定案為環境依賴，K6-cal §5 的
  「25.8 真值 14.85」需按環境理解（R8 歸因報告 §5 引用本節即足，不再展開）。
  我方引擎現行 pitch 14.85（face_line_factor CJK→WQY 1.3496）對「無真
  PMingLiU」環境忠實；若要對齊現環境 LO（13.75）＝`face_line_factor`
  PMingLiU 1.1992 入表——屬 **K6 pitch 軸管轄，另卡**（儀表板 patho_b p3
  內容分 0.289 最差榜的解法在那張卡）。

## 6. Arial 判別：A 軌 0.9950 → B 軌 0.9627 覆核定案

### 6.1 環境穩性實證（表值對兩類環境同成立）

基準環境（k6cal 真 MS face 裝機後）LO span font 實據渲染**真 MS face**
（Calibri／ArialMT／PMingLiU）。真 face 與 LO 克隆 face 的 hmtx 逐位覆核
（87 共同字元，run80 `env_stability.py` 重跑）：

| 對 | 逐位同寬 | 偏差 |
|---|---|---|
| Carlito vs Calibri | 87/87 | 0（逐位元組同寬） |
| LiberationSerif vs TNR | 87/87 | 0 |
| LiberationSans vs Arial | 87/87 | 0 |
| Caladea vs Cambria | 0/87（非逐位等） | 寬比 0.9988–1.0018（≤0.18%，遠小於容差） |

⇒ 表值錨「LO 渲染 face」，而真 face 環境與克隆環境的渲染 face 同寬——
**查表值對兩類環境同成立**，無環境特異性。（run73 記「Caladea==Cambria
逐位同寬」，run80 覆核修正為 ≤0.18% 近等，不影響表值。）

### 6.2 判別數據（B 軌覆核）

- A 軌 0.9950 的成分拆解：lower 0.9233／digits 1.0021／punct 0.9689／
  prose 0.9672，唯 **upper 1.0656**（Arial 大寫 vs Noto 子集大寫偏窄）把
  樣本串拉高。
- B 軌：Bug51170 LO PDF 真實行（≥60pt 寬，n=188）逐行寬比 median **0.9627**
  ＝prose 口徑（真實行文以 smallcase 為主）。
- 判別：Δ＝(0.9950−0.9627)/0.9950＝3.2% ＞ 容差 2.1% ⇒ 以 B 軌為準（表值
  0.9627）。落地效果：Bug51170 p4/p5 各收一行向 LO（§3 表）、全域 186→183、
  n830205 零擾動臂綠；fdo68607 不受 Arial 改值影響（Calibri 族）。
- 其餘族 A/B 差 ≤1%（Calibri Δ0.9%）維持 A 軌——A 軌 87 字元樣本串
  可逐位重產（`gen_csv.py` 重跑 CSV byte-identical，run80 實測），
  可重現性優先。

## 7. Bug51170 逐頁燈暫不鎖＝頁尾帶容量軸（另卡）

逐頁 [29,32,29,32,29,27,13]±1（K7 紅燈檔原暫記口徑）**不可靠 advance 軸達**：

- 我方全域正文行 183–186 vs LO 185（差 ≤2%，LO 側另有 pt-PT 連字号影響），
  但**逐頁多吸 1–3 行**且分佈隨頁。
- docx 手術探針（`foot_probe.py` V0/V1/V3，LO UNO 免）鐵證：LO 正文底界
  止於 `ph−bottom−footer_dist`（_bug51170 每頁 body ylast=781.4 截於 780 窗_），
  **刪頁尾內容不改變預留**＝非內容驅動；n830205 同頁尾環境却不預留
  （表格越界特例）⇒ LO 頁尾帶容量建模屬**獨立容量軸**。
- 處置：逐頁硬鎖暫不可綠；紅燈改鎖全域總量 180–190＋p6 非空（已落 §3），
  k7b 軟鎖（總行 180–210）維持。容量軸入另卡（與 §5 pitch 軸兩案可併規畫）。

## 8. 閘門、A/B 與可重跑路徑

- **全閘門 11/11 PASS**（run80，pipefail，`.github/workflows/ci.yml` 同口徑）：
  fmt／clippy -D／test／test-collab／goldens（shaping 12 例零位移）／
  roundtrip fixtures 13＋corpus **399/399**＋xlsx 2／layering／wasm check
  （含 collab+xlsx feature）／wasm build。日誌 `run80/gates.log`。
- **儀表板 A/B**：`seed20261022 n25` 對 `~/.hermes/cache/scratch/k7-sb/scores.jsonl`
  （K7 合併後基線）——可比 22 檔 21 檔逐檔內容分零變動；唯 Bug51170
  0.751→0.744（−0.007；PM run2 指令預先授權「Bug51170 分可動」臂——本表
  改值只動它的逐頁行分佈 ±1 行，內容分微降與行數總量改善（186→183）
  同源自 p3–p5 行重排，非新缺陷）；3 檔 LO 側 artifact（有圖形無可抽文字）。
  **其餘檔零劣化底線成立**。
- **可重跑**（scratch `k7cal/`，PM 復核用）：`gen_csv.py`（矩陣→CSV，
  byte-identical 實證）→ `run80/env_stability.py`（§6.1 hmtx／kerning）→
  `run80/verify_counts.py`＋`run78_b51170*.py`＋`run78_fdo_right.py`（§3 行數）→
  `run78_lo_b51170.py`（LO 逐頁口徑）→ `patho_verdict.py`＋`pminglh.py`（§5）→
  `foot_probe.py`（§7 探針）→ `run78_ab.py`（儀表板 A/B）→ `run80/gates.sh`。
- LO 收場：本 run 僅讀既有 PDF＋SOFFICE 單次 `--version`，未起 LO 實況
  轉換進程（`lo_scoreboard.py` 內建 atexit 殺手負責有起進程的場合）。
