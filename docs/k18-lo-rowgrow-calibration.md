# K18-cal：LO 文字框撐列模型校準（WC048 65.9pt 出處，report-only）

> 引擎基線：LibreOffice **24.8.0.3** (X86_64)（PDF producer 實錄）＋ pymupdf 1.28.2。
> 單位：本檔所有 pt 皆為 **PDF 原生點**（pymupdf 未做光柵化，matrix=1，
> 1pt=1/72in），**不涉及 px×72/DPI 換算**；LO 轉換 PDF 的座標系即 PostScript 點。
> 探針：`scripts/k18_probe.py`（anchors / measure / variants / mutate 四子命令）。
> 產物目錄：`~/.hermes/cache/scratch/k18/`（lo/、b2fix/、mut/，未入庫）。
> 零引擎觸碰：本卡 diff 僅 `docs/` ＋ `scripts/k18_probe.py` 兩檔。

---

## 0. 先修正卡面兩處前提（實測覆核，老實記錄）

1. **「全檔不存在 spAutoFit／mso-fit-shape-to-text」不成立。**
   WC048 兩檔各有 `<a:spAutoFit/>`×2 ＋ `mso-fit-shape-to-text`×2（VML fallback），
   tdf124398 各 1。探針輸出：
   ```
   $ python3 scripts/k18_probe.py anchors corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx | head -8
   { "scan": { "spAutoFit": 2, "noAutofit": 2, "mso-fit-shape-to-text": 2 }, ... }
   ```
   （推測 PM grep 的是 `<wps:spAutoFit` 或帶空格的 `<a:spAutoFit />`，LO/Word 實際寫法
   是 `<a:spAutoFit/>` 無空格。）**spAutoFit 旗標是撐列的主開關**——見 §3。
2. **撐第 1 列（row0，LO 橫線 72.25→138.15）的不是 Group 211，是 Textbox2。**
   Group 211 錨在 row1（cell(1,0)），撐的是 **row1**（138.15→344.25，高 206.1pt）；
   Textbox2（wrapTopAndBottom＋spAutoFit，cell(0,1)）撐 row0 到 65.9pt。
   實據：把群組 extent cy 185→100pt（m2），row1 底 344.25→259.25（Δ=−85=Δ宣告），
   row0 紋絲不動；把 Textbox2 的 spAutoFit 拿掉（m1），row0 65.9→161.7，
   row1 整段被推下去但自身高不變（206.0）。

---

## 1. LO 實測真值（pymupdf `get_drawings()`，線寬分級：表格框線 0.5pt、文字框邊 0.75pt）

### WC048（兩檔幾何全同，Mod 僅文字加一個 `a`）
| 項目 | LO 實測 pt |
|---|---|
| 表格橫線 y 序列 | 72.25 / 138.15 / 344.25 / 358.15 |
| row0 / row1 / row2 高 | **65.9** / **206.1** / 13.9 |
| Textbox1 框（cell00） | (72.2, 86.85)–(251.85, 107.4)，autofit 後高 **20.55** |
| Textbox2 pBdr 藍線 | x 171.7–423.55，y 99.2 / 132.85（框隱形：filled=f stroked=f） |
| Group 211（Textbox3 框） | (72.25, 158.9)–(459.25, 257.4)＝宣告 387×185 原尺寸 |
| Textbox3 文字 span | y 195.13–208.55 |

### tdf124398_groupshapeChart（正文錨，無表格）
| 項目 | LO 實測 pt |
|---|---|
| Group 11（宣告 430.5×285.1） | 頂 423.10／底 708.15＝**高 285.05=宣告 ext cy 原封不動** |
| 群內 Text Box 10（chOff y=−68.3，子 y=195.8） | 底框線 687.20／708.15：423.10+(195.8−(−68.3))=**687.20 逐位元組命中** |
| 圖說框（noAutofit 宣告 423.1×17.5） | 底 514.70，高 17.5＝宣告值 |

### WC048 宣告值對照（`k18_probe.py anchors`，pt）
| anchor | 宿主 | wrap | autofit | 宣告 extent | posV | distT/B |
|---|---|---|---|---|---|---|
| Textbox1（id217） | row0 cell0 | square | **spAutoFit** | 185.9×110.6 | 14.4 (paragraph) | 3.6 |
| Textbox2（id?） | row0 cell1 | **topAndBottom** | **spAutoFit** | 273.6×110.55 | 21.6 (paragraph) | 7.2 |
| Group 211 | row1 cell0 | square | **noAutofit** | 387.0×185.0 | pctPosV 2500（=頁高×2.5%=21.05；K15 已定頁高基準） | 0 |
| trHeight | row1=1961twip=**98.05pt**（atLeast） | | | | | |

---

## 2. 假說檢定：誰贏寫誰

| 假說 | 判決 | 實據 |
|---|---|---|
| (a) 框內文字排版高（行數×行高＋邊距）＋上下邊距 | **spAutoFit 框 wins**（row0=65.9 逐項分解見 §3）；noAutofit 框/群組不適用（用宣告高） | b0 復刻精確重現 65.9；b17（宣告 h 改成 40）spAutoFit 下撐列值**不動**（仍 65.9）——宣告值對 spAutoFit 完全無效 |
| (b) anchor 有效高被夾到列可視區 | **loss** | b6_h160（noAutofit 160pt）列照撐 182.2 不夾；b3_h40 縮到 62.2 線性跟隨，無下限夾取 |
| (c) 群組內真正佔位的是子形狀（AutoShape 14 高 395 vs Rectangle 214 高 9.35 各異）而非群組 extent | **loss（鐵證）** | m3：刪 Rectangle 214 → 列界序列 [72.25,138.15,344.25,358.15] **零變化**；m2：群組宣告 cy 185→100 → row1 底 344.25→**259.25**＝21.05+100+ε，逐 pt 線性跟隨**群組宣告** extent。子形狀只影響繪製，不參與撐列。 |

---

## 3. 撐列模型（本卡交付的核心；b 系列 21 變體擬合，全誤差 ≤0.6pt）

```
grow(anchor) = posV_pt + eff_h_pt + clash + ε   （ε≈0.4±0.3，0.5pt 表格線帽+量化）
clash        = (posV < 13.9_anchor行行盒底 + distT) ？ (13.9 + distT) ： 0
row_h        = max( natural_h, max{ grow(a) | a 錨在該列 } )
```
**碰撞疊加項 clash（pA/pD/b7/m6 四案定位，見 §5.4）**：LO 把「框與錨行行盒碰撞」
的位移**疊加**進撐列量（非夾取）。判別實據：b7（noAutofit posV=0 dist 7.2）
＝0+110.55+21.1+0.15＝131.8 ✓；m6（tb2 posV=0）＝0+43.85+21.1＝64.95 vs 實測
65.0 ✓；dist=0 同案（y1）clash 界降到 13.9，posV=15≥13.9 → 不疊加、精確命中預測 ✓。
WC048 現態 tb2 posV=21.6 ≥ 13.9+7.2=21.1（安全側）clash=0。
* **撐列資格**：`layoutInCell=1` 且 wrap ≠ none。topAndBottom 與 square **都撐**
  （b14 wrapSquare row0=132.7＝b1 topAndBottom；WC048 tb1 square 的 grow=35.0<tb2 的 65.9
  故被 max 吸收）。wrapNone 不撐（b15=14.0=自然列高；m8 群組改 wrapNone → row1 掉回
  trHeight 98.05）。
* **eff_h**：
  * `noAutofit`（或無 autofit 旗標）→ **宣告 extent cy 原值**（群組用群組宣告 ext cy，
    不碰子形狀——見 (c) 判決）。
  * `spAutoFit` → 排版高：
    `eff_h = tIns + bIns + Σ_par( (before+after) + n_lines × pitch )
            + Σ_pBdr_rules( sz/8 + space_pt )`
    * **pitch = size_pt × declared/240 × LH_face**（LH Carlito=1.2207，出自 K6）。
    * **框內段落 declared 恆 240**：WC048 docDefaults `w:line=259` 對 txbxContent
      **不生效**（鐵證：Textbox1 框高實測 20.55＝3.6+11×1.2207+3.6=20.63；若 259 應 21.69。
      合成檔無 docDefaults 亦同值）。
    * pBdr 每條規則（top/bottom）加 `border_pt + space_pt`（sz 單位 1/8pt：sz24=3pt）。
      b19 無框線 43.9 → b18 space=0 49.9（+6=2×3）→ b0 space=8 65.9（再+16=2×8）三點共線。
* **WC048 row0 的 65.9 完整分解**：
  `21.6(posV) + 3.6(tIns) + 3.6(bIns) + 12×1.2207(=14.65 行高) + 2×(3+8)(pBdr 藍線) + ε(0.45)`
  ＝ **65.9 ✓**（b0 合成復刻檔實測精確等於 LO 真值，行高/邊距/框線一項不缺）。
* **row1 的 206.1**：`21.05(pct 25%×頁高) + 185(群組宣告 noAutofit) + ε(0.1)` ＝ 206.1 ✓。
* natural_h：無 trHeight 列＝11pt 單行 14.0pt（b16 實測）；row1 natural＝trHeight 98.05。

### b 系列關鍵數據（`k18_probe.py variants --out .../b2fix`，全部可重跑）
```
b0_replica    row0=65.9  WC048 tb2 復刻（含 pBdr sz24 sp8,12pt italic,posv21.6,dist7.2）
b1_noautofit  row0=132.7 =21.6+110.55+ε（宣告接管）
b2..b6        42.2/62.2/82.2/102.2/182.2（noAutofit extent 掃描＝posV+宣告+ε 線性）
b11_dist0     132.7（dist 0＝dist 7.2：預設 dist 不入撐列）
b12_dist14    160.6 / b13_dist28 175.0（dist>7.2 膨脹，機制未解，見 §5 殘角）
b14_square    132.7（square 也撐）；b15_none 14.0（none 不撐）
b17_h40_auto  65.9（spAutoFit 下宣告值 40 無效）
b18/19        49.9/43.9（pBdr space/sz 分解共線）
```
### m 系列（真實 WC048 單變因突變，`k18_probe.py mutate`）
```
m1_tb2_noautofit  [72.25,233.95,439.95,453.95]  row0=161.7（宣告接管，+29.55 殘角見§5）
m2_grp_cy100      [72.25,138.15,259.25,273.15]  row1 底=138.15+21.05+100+ε ✓宣告線性
m3_del_rect214    [72.25,138.15,344.25,358.15]  零變化 → 假說(c)死
m5_tb2_bdrspace0  [72.25,125.65,...]            row0=53.4=65.9−16 ✓ pBdr space×2 入式
m8_grp_wrapnone   row1=98.1（=trHeight 98.05）  群組不再撐；框仍繪製
m9_grp_pct0       零變化（pct 失效時 LO 用 mc:Fallback posOffset 21pt）
```

---

## 4. 外推驗證（測試集，未參與擬合）

* **WC048-Mod**：宣告與 WC048 逐欄位全同（僅 txbx 文字＋一個 `a`，同字級同段數）。
  模型預測：**列高序列全同 [65.9, 206.1, 13.9]**。LO 實測：橫線
  [72.25, 138.15, 344.25, 358.15] **逐位元組相同 → 預測命中** ✓
* **盲測九案（clash 項入式後重跑 `scripts/k18_predict.py`，先寫預測再對 LO）**：
  | 案 | 參數 | 預測 | LO | err |
  |---|---|---|---|---|
  | pA | 11pt pBdr(16,6) tIns5/bIns10 tb posv15（碰崖） | 80.53 | 80.5 | −0.03 ✓ |
  | pB | 2 段 12pt italic pBdr(24,8) tb posv21.6 段前6 | 86.10 | 86.6 | +0.5 ✓ |
  | pC | noAutofit 宣告 40pt tb posv30 | 70.00 | 70.6 | +0.6 ✓ |
  | pD | 11pt 無框線 square posv14.4（碰崖） | 56.13 | 56.1 | −0.03 ✓ |
  | y1–y5 | 碰撞崖判別（dist=0/posV<界/noAutofit 交叉） | — | — | 全 ∈[−0.03,+0.6] ✓ |
  （clash 入式前 pA/pD 曾 +21.07 偏——正是碰撞崖的證據鏈，見 §5.4。）
* **tdf124398**：此檔**無表格**（anchor 宿主是正文段落，非列），無「列高」可預——
  卡面把它歸為列高測試集不成立，老實報。它能驗的是**同一模型的放置定律**
  `top = origin + posV`、`noAutofit 高＝宣告`：
  Group 11 頂=段落原點+9.5、高=285.05=宣告 ✓；群內子框 y=687.20
  由 `群頂+(子chOff映射 195.8+68.3)` **逐位元組命中** ✓；圖說框高=宣告 17.5 ✓。
  正文段落被撐多少 tdf 無法判定（正文字文極少、無法從 PDF 反推段落原點），
  **不外推宣稱**。

---

## 5. 未解殘角（K17 目標檔全部不踩，如實列出）

1. **noAutofit＋pBdr＋真檔**（m1）撐列多 +29.55pt（＝distT+distB+行高+ε 的組合值），
   同參數合成檔（b1）無此項。差異源於真檔 row1 的 trHeight/群組環境還是 pBdr 外溢，
   本輪未收斂。WC048 現態（tb2=spAutoFit）不踩。
2. **distT/B>7.2** 時撐列額外膨脹（b12/b13），非線性，機制未解。目標檔 dist≤7.2。
3. **posOffset=0** 時 LO 不縮撐高（m6：刪 21.6pt 的 posV 列只掉 0.9pt；b7 同象），
   推測回退段落行盒參與高。目標檔 posV 皆顯式>0。
4. **posV 碰撞崖（pA/pD +21.07 之源，二分實據）**：框頂被 LO 夾到
   「錨行行盒底＋distT」以下時，撐列量不是 `posV+eff` 而是
   `夾頂補償(≈行盒底 13.9)＋posV＋distT 級` 的疊加（y2: posv5 dist0 → 63.7,
   疊加 +13.9；y3: posv10 dist7.2 → 71.2, 疊加 +20.7）。判別實驗（`k18_clamp.py`）：
   **dist=0 時崖界消失**（y1 精確命中預測）。K17 條件：`posV ≥ 宿主段落行盒底＋distT`
   （WC048 tb2：21.6 ≥ 21.1，恰在界上安全側）；超出此域的檔案grow 預測會**低估**，
   如實報不硬擬。

---

## 6. 給 K17 復活的實作規格

* **層**：`table_map`（列高地板計算處）。流程：列 natural（trHeight/text）算完後，
  對「錨在該列」的每個 anchor 取 `grow = posV + eff_h`，列高=max(natural, grow)。
  不動 layout 層的框繪製（繪製仍按宣告/autofit 高，撐列只是列高地板）。
* **需新增解析欄位**（core 目前全缺）：
  1. `anchor.autofit` ∈ {spAutoFit, noAutofit, none→視同 noAutofit}（`wps:bodyPr` 子元素）；
  2. `anchor.posV_pt`（`wp:positionV/wp:posOffset`；pctPosVOffset 時＝頁高×pct/100000，
     pct=0 退回 mc:Fallback 的 posOffset——m9 實據）；
  3. `anchor.wrap`（**none 除外全撐**；K15 的 layoutInCell 壓制頁基準沿用）；
  4. spAutoFit 時：`bodyPr tIns/bIns` ＋ `txbxContent` 每段 `w:spacing before/after`、
     `w:pBdr top/bottom 的 sz+space`、run 字級 → `pitch = size×declared/240×LH_face`
     （**框內 declared 恆 240，勿吃 docDefaults**；LH 表用 K6 的 Carlito 1.2207）；
  5. 群組**不需要子形狀 extent 分解**（(c) 已死）——只要群組宣告 `wp:extent cy`。
* **紅燈改法**（解鎖 K17 三盞）：容差收到 **±1pt** 有正當性：模型誤差 ≤0.6，
  b0 復刻逐位元組命中。燈2 row0 ∈ 65.9±1；燈4 row1 頂 ∈ 344.25±1（且 row1 高
  206.1±1 可加鎖）；燈1 Textbox3 y ∈ 158.9±1。BUGGY 回塞鎖沿用。
* **防過度擬合警示**：勿把 ε(0.4)/21pt 等量當普適常數寫死——ε 只來自 0.5pt 線帽，
  實作以 `posV+eff_h` 落地、斷言容差吸收 ε 即可。

## 7. 重跑命令一覽
```bash
python3 scripts/k18_probe.py anchors corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx
python3 scripts/k18_probe.py measure corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx \
    corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell-Mod.docx \
    corpus/ooxmlexport/tdf124398_groupshapeChart.docx --out ~/.hermes/cache/scratch/k18/lo
python3 scripts/k18_probe.py variants --out ~/.hermes/cache/scratch/k18/b2fix   # 20 LO run
python3 scripts/k18_probe.py mutate   --out ~/.hermes/cache/scratch/k18/mut     #  8 LO run
python3 scripts/k18_predict.py --out ~/.hermes/cache/scratch/k18/predfinal      #  9 LO run（盲測九案）
```
完跑 `pgrep -c soffice` == 0（atexit render_lab.kill_own_lo 已清殭屍，本卡收尾實測 0）。
