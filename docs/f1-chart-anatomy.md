# F1 解剖報告：圖表（c:chart）掛載點、chart part 結構、語料普查（實拆，非推測）

署名：ADT109119。繁中註解，禁簡體禁 emoji。
本報告全部數值出自本目錄腳本實跑（`probe_chart.py`／`probe_parts.py`／`survey_charts.py`／
`survey_mount.py`／`probe_cjk.py`／`lo_chart_truth.py`／`fit_axis_law.py`），對照 LO 25.8（`soffice.exe` 轉 PDF＋pymupdf）。

---

## 1. `corpus/ooxmlexport/chart-size.docx` 掛載點實拆

封包內圖表相關 part（`unzip -l` 實抓）：

```
word/charts/chart1.xml     4934 B   （圖表本體）
word/charts/style1.xml     9666 B   （chartstyle，本切片不讀）
word/charts/colors1.xml     878 B   （chartcolorstyle，本切片不讀）
word/theme/theme1.xml      6795 B   （既有 theme.rs 已讀，accent1 供條形色）
```

掛載鏈（`word/document.xml`，實抓位元組偏移）：

```
w:body → w:p → w:r
  └ mc:AlternateContent
      └ mc:Choice Requires="wps"
          └ w:drawing[1332]
              └ wp:anchor（宿主文字框）
                  ├ wp:extent cx=3943350 cy=1404620  → 310.50 × 110.60 pt（既有發射層已發此框）
                  ├ wp:positionH/column posOffset=1781175、wp:positionV/paragraph posOffset=180975
                  └ a:graphicData uri=".../word/2010/wordprocessingShape"
                      └ wps:wsp（txBox="1"，白底黑框）
                          └ wps:txbx → w:txbxContent
                              ├ w:p[2603]  「Before.」
                              ├ w:p[2688]
                              │   └ w:r → w:drawing[2770]           ← 圖表掛載點（段內 wp:inline）
                              │       └ wp:inline
                              │           ├ wp:extent cx=2162810 cy=1297940 → 170.30 × 102.20 pt
                              │           ├ wp:docPr id="1" name="Chart 1"
                              │           └ a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"
                              │               └ c:chart r:id="rId4"     ← 關聯 id
                              └ w:p[3403]  「After.」
      └ mc:Fallback → w:pict → v:shape → v:textbox → w:txbxContent
          └ 同一 w:drawing（w:drawing[7389]，byte 完全同份）→ c:chart r:id="rId4"
```

- `word/_rels/document.xml.rels`：`rId4 → charts/chart1.xml`（Type=.../relationships/chart）。
- `[Content_Types].xml`：`/word/charts/chart1.xml` Override `...drawingml.chart+xml`。
- `word/charts/_rels/chart1.xml.rels`：rId1=style1.xml、rId2=colors1.xml、rId3=**External** `Book1`（`c:externalData`，本切片不讀）。

### 解剖要點 A：mc:Fallback 雙份
同一 `w:drawing`（含同一 `c:chart r:id`）在 `mc:Choice` 與 `mc:Fallback` **各出一次**（偏移 1332 與 7389 的 `w:pict` 內）。
探針 `probe_cjk.py` 同檔掃出 **兩處** 掛載。**解析層必須去重**（同一 r:id＋同一宿主段落只算一次），
否則發射清單會雙份圖表、LO 只有一份，反而新增假缺字／假多字。既有 `drawing.rs` 的圖片路逕已用
「skip_tag 吞沒幀」處理 Fallback 巢狀（`graphic`/`v:shape` 幀），本卡 chart 採集走**新模組獨立掃描**，
自帶同一去重紀律，不碰 `drawing.rs`（F2 卡場禁碰）。

### 解剖要點 B：LO 標題真值不是檔內字串（卡面前提更正）
卡面把真缺字 `圖`,`表`,`標`,`題` 歸給「圖表標題」。實拆：`chart1.xml` 的 `c:title` **只有 layout/overlay/spPr/txPr，完全沒有 `c:tx`**
（`probe_parts.py` 對本檔回 `(EMPTY c:title 無 c:tx/a:t)`），且全封包**零 CJK 字元**
（`probe_cjk.py` 掃全部 15 parts，唯一 CJK 出自 theme1.xml 的日／韓／繁體字型名，無一為「圖表標題」）。
LO 該行真值（`lo_chart_truth.json`）：

```
text="圖表標題" dir=[1,0] bbox=[276.25,113.98,332.68,153.72] size=13.91 font=NotoSansTC-Thin
```

即 LO 對「空 c:title＋autoTitleDeleted=0」自行繪製的**本地化佔位字串**（LO 介面為繁中），**不是文件內容**。
按本專案「像素/文字必須是真來源」紀律（K7「表沒有實測值的族絕不猜數」同義），本切片**不虛構**此四字：
標題只在 `c:title/c:tx//a:t` 真有文字時發射（語料實據：`fdo76098`/`fdo78474` 的 `c:title` 帶 `Sales`）。
本檔剩餘真缺字 `0 1 2 3 4`＋`1 2 3` 全部可由圖表軸發射補齊（見 §4）。

---

## 2. `word/charts/chart1.xml` 實際結構（逐元素實抓）

```
c:chartSpace（xmlns:c = http://schemas.openxmlformats.org/drawingml/2006/chart）
├ c:date1904 val=0 ／ c:lang val=en-US ／ c:roundedCorners val=0
├ mc:AlternateContent → c14:style val=102 ／ c:style val=2
└ c:chart
    ├ c:title        ← 無 c:tx（空標題；見 §1 要點 B）；c:txPr/a:pPr/a:defRPr sz="1400"
    ├ c:autoTitleDeleted val=0
    ├ c:plotArea
    │   ├ c:layout   ← **空 layout**（無 c:manualLayout，故無宣告式繪圖區相對位置 → 走後備幾何）
    │   ├ c:barChart
    │   │   ├ c:barDir val="col" ／ c:grouping val="clustered" ／ c:varyColors val=0
    │   │   ├ c:ser ×1
    │   │   │   ├ c:idx val=0 ／ c:order val=0 ／ c:spPr（solidFill schemeClr accent1）
    │   │   │   ├ c:invertIfNegative val=0
    │   │   │   ├ c:val → c:numRef
    │   │   │   │        ├ c:f = Sheet1!$F$3:$F$5
    │   │   │   │        └ c:numCache（formatCode=General, ptCount=3, pt idx=0/1/2 → v = 1, 2, 3）
    │   │   │   └ （**無 c:cat**：LO 以 pt idx 自動產生類別標籤 1/2/3）
    │   │   ├ c:dLbls（showVal=0、showCatName=0…全 0 → 不發資料標籤）
    │   │   ├ c:gapWidth val=219 ／ c:overlap val=-27
    │   │   └ c:axId 141088744 ／ c:axId 141093056
    │   ├ c:catAx（axId 141088744）
    │   │   ├ c:scaling/orientation=minMax ／ c:delete val=0 ／ c:axPos val=b
    │   │   ├ c:majorTickMark=none ／ c:minorTickMark=none ／ c:tickLblPos=nextTo
    │   │   ├ **無 c:majorGridlines**（類別軸不發格線）
    │   │   ├ c:txPr/a:bodyPr rot=-60000000；a:defRPr sz=900（=9pt）
    │   │   └ c:crossAx=141093056 ／ c:crosses=autoZero ／ c:auto=1 ／ c:lblAlgn=ctr ／ c:lblOffset=100
    │   ├ c:valAx（axId 141093056）
    │   │   ├ c:scaling/orientation=minMax ／ c:delete val=0 ／ c:axPos val=l
    │   │   ├ **c:majorGridlines**（存在 → 格線要畫）
    │   │   ├ c:numFmt formatCode=General sourceLinked=1
    │   │   ├ c:tickLblPos=nextTo ／ c:txPr/a:defRPr sz=900
    │   │   └ c:crossAx=141088744 ／ c:crosses=autoZero ／ c:crossBetween=between
    │   └ c:spPr（noFill／ln noFill）
    ├ c:plotVisOnly=1 ／ c:dispBlanksAs=gap ／ c:showDLblsOverMax=0
    └ c:externalData r:id=rId3（autoUpdate=0）
```

**快取 vs 引用式判定（卡面必答）**：本檔 `c:val` 走 `c:numRef`，但其下 **同時帶 `c:numCache`（3 個 pt）**——
LO 讀 cache 即可成圖，不需外部 sheet；檔內**無嵌入 xlsx part**（只有 External `Book1` 關聯）。
`c:cat` **元素不存在**，故本檔無 `strCache`／`numCache` 類別值可讀，LO 改以 `c:pt/@idx`＋1 自動編號產生 `1/2/3`。

## 2b. 語料普查（`survey_charts.py` 實跑，14 檔含圖表）

| 檔案 | chart parts | 系列 | numCache | strCache | c:title 真文字 | catAx | valAx | majorGridlines | externalData | 圖型 |
|---|---|---|---|---|---|---|---|---|---|---|
| xopt/ChartUpdater01_Chart-Cached-Data-04 | 1 | 3 | 0 | 0 | 無（空 title） | 0 | 1 | 1 | 0 | lineChart |
| xopt/ChartUpdater01_Chart-Cached-Data-05 | 1 | 3 | 0 | 0 | — | 1 | 1 | 0 | 0 | lineChart |
| xopt/ChartUpdater01_Chart-Embedded-Xlsx-08 | 1 | 3 | 6 | 3 | 有 | 0 | 1 | 1 | 1 | lineChart |
| xopt/DocumentBuilder04_SolarOverview | 1 | 3 | 3 | 6 | — | 1 | 1 | 0 | 1 | lineChart |
| poi/bar-chart-template | 2 | 4 | 4 | 8 | 2×空 | 2 | 2 | 2 | 2 | barChart |
| poi/test-data_document_61745 | 2 | 6 | 6 | 12 | 2×空 | 2 | 2 | 2 | 2 | barChart |
| poi/test-data_document_chartex | 6 | 11 | 16 | 17 | 4 | 3 | 4 | 4 | 4 | stockChart／lineChart／barChart／radarChart×2 |
| ooxmlimport/tdf124398 | 1 | 3 | 3 | 6 | — | 1 | 1 | 0 | 1 | barChart |
| ooxmlimport/tdf82824 | 1 | 2 | 4 | 2 | 空 | 0 | 1 | 1 | 1 | areaChart |
| ooxmlexport/chart-size | 1 | 1 | 1 | 0 | 空 | 1 | 1 | 1 | 1 | barChart |
| ooxmlexport/fdo76098 | 1 | 1 | 1 | 2 | **Sales** | 0 | 0 | 0 | 1 | pieChart |
| ooxmlexport/fdo78474 | 2 | 2 | 2 | 4 | **Sales×2** | 0 | 0 | 0 | 2 | pieChart |
| ooxmlexport/strict | 1 | 1 | 1 | 0 | 空 | 1 | 1 | 1 | 1 | barChart（**strict 命名空間**） |
| ooxmlexport/tdf124398_groupshapeChart | 1 | 2 | 4 | 1 | 3（含軸標題） | 0 | 2 | 1 | 1 | scatterChart×2 |

普查結論（影響面，實據非猜）：

1. **掛載形態兩式**：`wp:inline`（正文流／框內，9 檔）與 `wp:anchor`（浮動，3 檔）；本卡目標檔 chart-size 是
   「**框內 wp:inline**」（在 `wps:txbxContent` 第 2 段），屬最難的一式（既有 body 流佔位路徑看不到框內段落）。
   `probe_parts.py`＋`survey_mount.py` 實抓 `depth_txbx`：chart-size／fdo78474(chart2) 為框內（txbx 深度 1），其餘為框外。
2. **命名空間兩式**：transitional `http://schemas.openxmlformats.org/drawingml/2006/chart`（13 檔）與
   strict `http://purl.oclc.org/ooxml/drawingml/chart`（strict.docx）。strict 另把屬性值改百分比字串
   （`lumMod val="65%"`、`baseline="0%"`）——解析器兩式都要認，否則 strict 零命中。
3. **cache 優先鐵律的實據**：14 檔共 41 個 `c:ser`，`c:val` 幾乎全走 `c:numRef`，但 **39/41 同時帶 `c:numCache`**
   ——即 cache 值是常態、引用式是備援；`c:cat` 有 `c:strCache`（12 檔）與 `c:numCache`（tdf82824 日期序列）兩式，
   chart-size/strict 兩檔 **完全沒有 `c:cat`**（LO 自動編號）。
4. **外部工作表引用重算必須留帳**：11/14 檔帶 `c:externalData`（Embedded-Xlsx-08 另含真嵌入 xlsx part），
   本切片**只做 cache**，引用式不重算（見 §5）。

## 2c. 掛載點幾何實抓（供發射定位，禁單檔調參）

`survey_mount.py` 實抓各檔 `wp:extent`（EMU）與位置宣告，全部換算 ÷12700＝pt：

| 檔案 | 形態 | extent → pt | 位置宣告 |
|---|---|---|---|
| chart-size（框內 inline） | inline | 170.30 × 102.20 | 無（框內段落流） |
| Chart-Cached-Data-04/05、Embedded-Xlsx-08、SolarOverview、61745×2、chartex×6、bar-chart-template | inline | 432.00 × 252.00 等 | 無 |
| tdf82824 | anchor | 216.75 × 168.00 | V/paragraph posOffset=200660（15.80pt） |
| tdf124398_groupshapeChart | anchor | 430.50 × 285.10 | H/column=314325、V/paragraph=120650 |
| fdo78474 | anchor | 186.35 × 117.00 | V/paragraph=0 |

LO 真值對照（chart-size，`probe_lo_draw.py` 實抓 15 個 drawing）：

- 圖表外框（LO drawing#1）rect `[219.85, 112.75, 389.35, 214.15]` ＝ 169.50 × 101.40 pt，
  與 `wp:extent` 170.30 × 102.20 差 ≈0.8／0.8 pt（框線 0.73pt 對齊誤差內）→ **外框尺寸直接吃 wp:extent 成立**。
- 繪圖區（plot）由格線／柱形實抓：格線 x∈[230.65, 385.95]（寬 155.30）、底軸 y=200.35、最高格線 y=153.5，
  柱形底 200.35、`val=1` 柱高 11.75、`val=2` 23.45、`val=3` 35.15 → **單位高 11.7pt 線性**（`fit_axis_law.py`）。
- 數值軸刻度（LO 實抓，x≈223.29，size 8.93）：由下而上 `0,1,2,3,4`，步進 1；
  類別軸刻度（y≈201.34–212.21，size 8.93）x=254.27／306.04／357.81（等距 51.77pt）。
  **字級 8.93 對應宣告 `sz=900`（9pt）**（LO 縮放後像素量化），即刻度字級直接吃 `c:txPr//a:defRPr/@sz ÷ 100`。

---

## 3. LO 數值軸刻度律（`fit_axis_law.py`，10 檔實抓，非單檔擬合）

| 檔案 | 圖型 | grouping | cache 資料 min/max | LO 數值軸刻度（由上而下） | 步進 | 字級 |
|---|---|---|---|---|---|---|
| Chart-Cached-Data-04 | lineChart | standard | 1.0 / 11.0 | 8,6,4,2,0 | 2 | 9.02 |
| Chart-Cached-Data-05 | lineChart | standard | 180 / 330 | 350,300,250,200,150,100 | 50 | 10.01 |
| Embedded-Xlsx-08 | lineChart | standard | 1.8 / 5.0 | 6,5,4,3,2,1,0 | 1 | 9.02 |
| SolarOverview | lineChart | standard | 4.6e7 / 7.38e9 | 8e9,7e9,…,1e9 | 1e9 | 10.01 |
| bar-chart-template | barChart | clustered | 1.8 / 5.0 | 6,5,4,3,2,1,0 | 1 | 8.99 |
| test-data_document_61745 | barChart | clustered | 1.8 / 5.0 | 6,5,4,3,2,1,0 | 1 | 8.99 |
| test-data_document_chartex | bar/line/radar/stock | 混合 | 1.8 / 58 | 6…0（兩組） | 1 | 9.02 |
| tdf82824 | areaChart | standard | 12 / 32 | 35,30,25,20,15,10 | 5 | 9.00 |
| chart-size | barChart | clustered | 1.0 / 3.0 | **4,3,2,1,0** | 1 | 8.93 |
| strict | barChart | clustered | 1.0 / 3.0 | **3,2,1,0** | 1 | 9.02 |

可採納的律（多數檔成立）：

- **L1 步進是「整數友好步進」**：從 `{1,2,5}×10^k` 取最小者使刻度數 ≤ 約 8（9.02/10.01/8.99 三檔皆吻合）。
- **L2 下界**：資料 min ≥ 0 時下界＝0（10/10 檔成立）；負值本切片不處理（見 §5）。
- **L3 上界＝ceil(有效上界 / step)×step**，其中「有效上界」= clustered 取 max(cache val)、
  stacked/standard 取逐類別加總后的 max（SolarOverview/04/05/tdf82824 四檔 stack 律成立）。
- **L4 刻度數不固定**（4～8 個），故 L3 必須搭配「繪圖區高 → 可容納刻度數」的容量尺：
  chart-size（框高 102.2pt）給到 4、strict（框高較小）只給到 3——**同一資料、不同框高，刻度上界不同**。
  本切片以「每刻度最低 11pt 高」作容量尺（實據：chart-size 單位高 11.7pt），並在考卷只鎖
  「刻度序列必含 0 與步進 1、且至少 4 個」的鑑別性子集（見 f1-redlight）。

## 3b. 類別軸律

- 有 `c:cat/c:strCache` → 發 cache 字串（tdf124398：`Category 1..4`；fdo76098：`1st Qtr..4th Qtr`）。
- 有 `c:cat/c:numCache` → 發數值字串（tdf82824：序列 37261.. 為 Excel 日期序列；LO 因 `c:numFmt` 為日期格式
  而繪成日期字串，本切片照 cache 原值發字串，**不做日期序列轉換**，記帳於 §5）。
- **無 `c:cat`** → LO 自動以 `pt/@idx + 1` 編號（chart-size 實抓 `1,2,3` 於 y≈206），本切片同律。

---

## 4. 本切片選擇做到哪裡

1. `core/ooxml/src/chart.rs`（新增，既有 crate 內新模組）：chart part 解析——`c:ser` 序、
   `c:val`／`c:cat` 的 cache 值（**cache 優先**，`c:f` 引用式只留原始字串不入值）、`c:tx`（系列名）、
   `c:title`（圖表標題，僅 `c:tx//a:t` 真有文字才算標題）、`c:catAx`／`c:valAx` 存在性＋`c:delete`＋
   `c:txPr/@sz`＋`c:majorGridlines` 存在性、`c:barDir`／`c:grouping`／`c:gapWidth`、axId 配對、
   `c:layout`（無 manualLayout＝None）。兩式命名空間（transitional＋strict）皆認；`val` 屬性
   同時吃整數（65000）與百分比字串（"65%"）。
2. 掛載採集：獨立掃描 `word/document.xml`（＋ header/footer part 不改，本切片只 document.xml），
   取 `a:graphicData/@uri`＝chart（兩式）下的 `c:chart/@r:id`，查 `word/_rels/document.xml.rels` 得 part 路徑，
   帶宿主段落索引（`drawing.rs` 同口徑的扁平 `w:p` Start 計數）、`wp:inline`/`wp:anchor` 形態、
   `wp:extent`（EMU→pt）、anchor 的 posOffset；**mc:Choice／mc:Fallback 雙份去重**。
3. `core/engine-api/src/lib.rs`：發射組裝——圖表外框＋軸線（直線 Rule）＋類別標籤文字＋數值刻度文字＋
   標題文字（真有 `c:tx` 才發），幾何走 §3 律（`c:layout` 缺＝等分後備＋11pt/刻度容量尺），
   字級吃 `c:txPr//a:defRPr/@sz`、色吃 `schemeClr`（既有 `ThemeColors`）。
4. `shells/native/src/render_svg.rs`：只吃既有 `DrawItem`（Text／Rule／Rect），**不新增 DrawItem 變體**，
   故無圖表檔案的 SVG 輸出位元組零變動（閘門可證）。
5. 考卷 `core/ooxml/tests/f1_chart_projection.rs`（燈 1 解析逐字）＋
   `core/engine-api/tests/f1_chart_emission.rs`（燈 2 發射落 bbox／燈 3 環境變數回塞鑑別力）。

## 5. 明確不做到哪裡（順手做＝越界，寫死在此）

1. **引用式重算不做**：`c:numRef/c:strRef` 的 `c:f`（如 `Sheet1!$F$3:$F$5`）只存原始字串診斷用，
   **不讀外部 sheet、不重算**（`core/xlsx/` 屬下一切片公式引擎，且 11/14 檔的 externalData 是 External 關聯，
   連 part 都不在封包內）。cache 缺＝該系列值列為空，不虛構。
2. **3D 圖型不做**：`c:bar3DChart`／`c:pie3DChart`／`c:ofPieChart`／surface／bubble／radar 之 3D 與極座標幾何
   一律不發（語料實據：chartex.docx 含 radarChart×2 與 stockChart——**本切片只發 bar／line／area／pie／scatter
   的 2D 平面臂**，其餘老實跳過並在報告留數）。
3. **漸層／陰影／effectLst／chartStyle 與 colorStyle part 不做**（style1.xml、colors1.xml 不讀）。
4. **圖例（c:legend）排版不做**：語料多數檔無 legend；tdf124398_groupshapeChart 的軸標題多份只發主標題。
5. **資料標籤引線（c:dLbls 的 showVal/showCatName 排版、leader lines）不做**（chart-size dLbls 全 0，實據不需）。
6. **error bar（c:errBars）、複合圖型混合（c:comboChart／多 plotArea）、c:trendline 不做。**
7. **Excel 日期序列（numCache 的 37261.. 配 `c:numFmt` 日期格式）不轉日期字串**——照 cache 原值發，
   LO 會繪 `1/1/2002` 型式，此差異記為殘留（tdf82824／Chart-Cached-Data-04 兩檔）。
8. **負值軸／`c:min`／`c:max` 覆寫／對數軸（c:logBase）／次座標軸（多 axId 組）不做**（chartex 的雙 valAx 只發第一組）。
9. **LO 本地化佔位標題不虛構**（§1 要點 B：`圖表標題` 四字不出自檔內，我方不發）。
10. **`core/layout/`、`core/xlsx/`、`core/odf/`、`drawing.rs` 零觸碰**；圖表不進分頁器（框內圖表隨宿主框發射，
    跨頁行為＝宿主框所在頁，與既有框內文字同口徑）。

## 6. 殘留風險（如實記）

- L4 容量尺（11pt/刻度）為 10 檔擬合中挑出的最保守值，chart-size 給 5 刻度（LO 5）、strict 給 4（LO 4）吻合；
  其他檔未逐一鎖死，若收案後 render_lab 對其他 13 檔出現刻度數漂移，屬本切片殘留，另卡處理。
- 語料 14 檔中 **LO 轉換 4 檔失敗**（xopt 全 4 檔，`lo_chart_truth.py` 實測 LO_FAIL；`render_lab.py` 對同批
  既有語料亦記 lo_fail），故 xopt 檔無 LO 真值可鎖，考卷真值只取 LO 成功轉換的 10 檔。
- `c:pt/@idx` 稀疏（跳 idx）時 LO 以 idx 定位；本切片假定 idx 連續 0..ptCount-1（語料全數如此，實據 §2）。
