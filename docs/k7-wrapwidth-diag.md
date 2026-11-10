# K7-wrapwidth — 換行可用寬診斷報告（先診斷後動刀）

> 卡：t_919b8d54（board `quire`，run67）。LO 真值工具鏈＝Linux soffice
> `~/.local/lo-linux/opt/libreoffice25.8`（25.8.7.3，K6-cal 同基準）。
> 分析腳本與原始輸出：`~/.hermes/profiles/ow-core/cache/scratch/k7/`
> （step0_capture.sh 抓取、step7c/16 逐格對照、step12/21 寬比、
> probe_ab.sh 引擎探針 A/B、k7cal_matrix.py 寬表矩陣）。
> pitch 軸零觸碰（K6-cal 權威；本卡未動任何 pitch 相關行）。

## 0. 結論表

| 缺陷 | 狀態 | 錨點證據 |
|---|---|---|
| A. 無宣告 tblW 自然寬表被壓回版心 | **已修**（2ff9fd3＋6008572） | fdo68607 格線：我方 36.0/586.8 → **30.6/581.4**（LO 30.1/580.9） |
| B. cellMargins 級聯漏扣／邊框寬未扣（卡面假說） | **否證**——不修 | 我方已扣 108dxa＝5.4pt/邊；LO 最長行文字越格線 2.5pt＝LO 不扣邊框寬 |
| C. 內嵌 Noto 子集 vs LO 替代 face 的字形advance寬差（＋14.3%） | **根因確認，修復＝水平寬度表（本卡落地中）** | 離線模擬＋引擎探針雙鐵證，見 §3 |
| D. Bug51170 header 掛載/容量流動 | 診斷完成，修復評估中 | §4 |

## 1. fdo68607 換行點逐格對照（任務 1）

源檔結構：單一大表（1 tbl／137 tr／2 欄），`tblW=auto`（無宣告），
`tblGrid` 2516＋8500 dxa＝125.8＋425.0pt（自然寬 **550.8pt**＞版心 540pt）；
cellMar 級聯＝TableNormal/MediumShading1-Accent1 `108 dxa`＝5.4pt／側，無 tcPr 覆寫；
無 `w:tblInd`、無 `w:jc`、無段縮排（`w:ind` 全文 0 個）。

逐格行數對照（step7c，右欄依格 greedy 前綴匹配，LO 315 行全消費／我方 337）：

- **137 格中 50 格行數不同**，方向壓倒性＝我方多一行（早換行）：
  cell#1 LO 2／我方 3、#3 3/4、#4 2/3、#7 2/3、#10 4/5、#14 3/4…（全表在 scratch step7c 輸出）。
- 右欄總行：LO 315 vs 我方（修前）356；頁 9 vs LO 8。

錯寬數值（pt 級）：

| 量 | LO 實測 | 我方修前 | 我方修後 |
|---|---|---|---|
| 表格左格線 x | 30.1 | 36.0 | **30.6** |
| 表格右格線 x | 580.9 | 586.8 | **581.4** |
| 右欄格線 x | 155.9 | 159.3 | （156.0，同幾何） |
| 右欄文字起點 x0 | 161.4 | 164.73 | **161.8** |
| 右欄可用寬（格寬−2×5.4） | **414.2** | 405.88（**窄 8.3pt**） | **414.2** ✓ |

⇒ **根因 A 定案**：LO 對「無宣告 `w:tblW`＋自然寬＞版心」的表**不壓寬**，
且越界量（550.8−540＝10.8pt）**左右均分**（各越 ~5.4pt）。我方舊徑
`resolve_columns` 無條件 `k=avail/total` 壓回 540pt＋`table_x_off` 貼左緣
＝格內寬縮 8.3pt＋整表右移 5.4pt。兩處皆為純幾何錯，已修並錨定（誤差 ≤0.5pt）。

LO 不扣邊框寬鐵證：LO 右欄最長行 x1=578.4，格右界線 580.9——文字終端**越過**
格內側 2.5pt 而斷行寬仍到 414.2pt（colW−cellMar，無邊框項）。故卡面
「邊框寬度扣除」假說方向錯誤，照做會更窄、更多行。

## 2. 其他多頁表檔同因性（任務 2）

全 corpus 掃描（step10）：無宣告 tblW 且 grid 總寬＞版心的檔＝**差異集**，
fdo68607 屬其中超界比例最大的一檔（10.8pt）。LO 側抽樣覆核（step17）：
fdo53985／tdf131260／Bug54849 的 LO 格線同樣越出或恰貼版心右界
（水平線 x1max 583.2／545.7＞版心右界）＝LO 一律不壓自然寬，同因確認。
patho_table_across_pages_b（grid 450＜版心）不受影響（3/3 頁零變動）；
tdf124637 無表（5/5 頁零變動）。

## 3. 換行殘差根因＝字形advance寬差（任務 1 延伸，卡外新缺陷 C）

幾何修正後 fdo68607 仍 9 頁（總行 491 vs LO 457）。排除可用寬變數的鐵證鏈：

1. **離線斷行模擬**（step16，單字貪心、同文字、同 414.2pt 可用寬）：
   - LO 實際渲染 face（Calibri）advance → **310 行**（LO PDF 實測 315 ✓）
   - quire 內嵌 Noto 子集 advance → **350 行**（我方 dump 實測 349 ✓）
   ⇒ 斷行器與可用寬已與 LO 等效；**殘差 100% 在字寬源**。
2. **寬比定量**（step12/21，n=418 真實行）：Carlito/Noto 逐行寬比
   median **0.8747**、mean 0.8718、p5–p95 0.8475–0.8849；
   標準樣本串法（K6 雙軌類比）＝0.8834。
3. **引擎級探針**（臨時 env `K7_LATIN_WIDTH_SCALE`，Latin advance 乘常數）：
   scale=0.8742 → fdo68607 **9→8 頁**（每頁 table 行 [19,16,20,20,19,21,18,12]，
   LO [17,13,20,16,20,16,18,8]+尾頁）；1.0/0.90 → 仍 9 頁。
   ⇒ 水平軸存在 K6 real-LH 的同構缺陷：**「宣告族→LO 實際 face」缺 advance
   寬表**，內嵌 Noto 子集對拉丁文整體偏寬 14.3%。

處方（水平寬度表，K6-cal 同紀律）：`family→advance_scale` 查表
（鍵＝宣告族小寫，值＝LO 渲染 face 寬／Noto 子集寬；未命中→1.0＝零擾動守門，
比照 K6 `has_fonts()＋calibrated` 閘），逐段以該段主宣告族取 scale 乘入
Latin/通用 run 的 cluster advance。矩陣（k7cal_matrix.py，A 軌樣本串）：

| 宣告族→LO 渲染 face | scale（樣本串） | scale（真實行 median，覆核） |
|---|---|---|
| Calibri→Calibri/Carlito | **0.8834** | 0.8747（n=418） |
| Times New Roman→LiberationSerif | 0.9648 | — |
| Arial→LiberationSans | 0.9950 | — |
| Cambria→Caladea | 待量（本機 face 缺） | — |

殘差風險如實記帳：p5–p95 spread＝±2.1%（同一族不同文字內容寬比浮动），
單一族標量對行斷點的誤差與 K6 的 ±0.1pt pitch 殘差同量級——可接受，
但每檔以 LO 實測覆核為準；混合族 run 級細分屬後續。

## 4. Bug51170 尾部流動（任務 4 診斷）

源檔：A4、pgMar top 106.35／bottom 49.65、header 35.45、footer 35.45、
titlePg；header part 含**大圖**（146×78pt，佔 y 35.4–113.4＝越過 top margin）。

- LO：p0 正文起 y=106.7（首頁無頁首＝titlePg ✓）；p1+ 頁首圖 35.4–113.4，
  **正文起點被頁首推下到 125.6**（＝margin_top＋~19.2pt 溢出補償）；
  頁尾文字「Estatutos…」y=781.4（頁尾區）、頁碼同排。每頁 [29,36,30,33,30,28,14]。
- 我方（修前）：每頁行 [32,32,31,33,32,30,＋圖]——**p1+ 正文起點未隨頁首圖
  下移**（仍 106.35），每頁多吸 1–2 行 → 尾頁 p6 被吸乾（僅頁碼＋頁首）。
  且我方「Estatutos…」發射在 y=809（頁尾距離值）語義正確（footer），
  LO 同位置（781.4 與 806.55 差＝LO footer 文字基線上移），**非缺陷**。
- 根因：**頁首內容高度＞header 距時不推正文起點**（代碼內註明
  「本輪不縮正文容量…溢出處理屬後續卡」——engine-api lib.rs:3630 區）。
  LO 口徑＝頁首塊底部＋header→margin 距離，正文起點取 max(margin_top, 頁首底端)。

## 5. 驗收狀態（本 run 實測）

- `cargo test --workspace`：全綠（layout 127／engine 535／全 0 failed，修後實跑）。
- 儀表板 seed20261022 n25：均值 0.8884 → 0.8884（幾何修正零劣化；
  fdo68607 不在 seed25 內，故 fdo 改動不反映於均值——逐檔 diff 空）。
- goldens／roundtrip／clippy／layering：續跑於實作後（§6 交付鏈）。
