# E2 直排框→行錨定定律（LO 實測反解，pymupdf dir+bbox 口徑）

署名：ADT109119。數據來源：`scripts/e2_probe_law.py` 對合成框（lIns/tIns/rIns/bIns=0、
11pt Calibri、框 100x300 與 200x300、段數 1-3）走 `render_lab` 同一條 LO 轉換路徑後，
pymupdf `get_text("dict")` 逐行抽 `dir` 與 `bbox`；原始數值固化於
`docs/e2-vert-truth/`（`probe_truth.json` 合成網格、`corpus_truth.json` 實檔逐行、
`lo_frames.json` 框矩形；冪等可重跑覆核）。座標系為 pymupdf top-origin。

## vert270（dir=[0,-1]，整行由下而上）

| bodyPr/@anchor | 欄群 x 錨定（框 [fx0,fx1]，欄寬 col=13.4≈sz*1.22） |
|---|---|
| `t` | 首欄 x0≈fx0+0.2，第 k 欄 x0=fx0+0.2+k*13.4（靠左緣向右長，與框寬無關） |
| `ctr` | 欄群整體置中：首欄 x0≈框心−n*13.4/2（n=1 時 93.5＝100−6.5） |
| `b` | 末欄 x1≈fx1（末欄右緣貼框右緣，整群向左推） |

- 行高向：行 bbox y1≈fy1−0.1、行長沿 −y 方向延伸（`t`/`ctr`/`b` 三錨點 y 相同，@anchor 只管 x）。
- 段距步長 dx=13.40-13.50（＝col_w，與 `sz*1.22` 一致）。

## vert（dir=[0,1]，整行由上而下）

- 首行 y0≈fy0+0.1（貼框頂向下長）；@anchor 對 y 無作用（t/ctr/b 實測同位）。
- 欄群由左向右**編號向左**：第 k 段 x0＝fx0−0.2＋(n−1−k)*13.4——末段貼框左緣、前段向右排（n=3 實測 49.8/63.3/76.7）。

## corpus 實檔 LO 真值（非合成，pymupdf 口徑同 `scripts/e2_vertical_truth.py`）

- `fdo78957.docx`：頁首 `word/header1.xml` 有 `wps:bodyPr vert="vert270"`，`Quire` dir=[0,-1] bbox=[561.6,380.0,577.1,411.9]（14pt）——我方完全未發射（真缺字＋新版行方向閘門版面 0.0）。
- `tdf103573.docx`：兩個 vert270 框（左／右頁邊），LO 框矩形 [26.8,80.2,90.1,277.5] 與 [528.5,72.0,591.8,269.3]、行 dir=[0,-1]——我方旗標對（fl=16）但**兩框矩形同發 x=281.1**（x 偏 226-276pt，y 對），屬框幾何（錨點解析）缺陷非行方向臂缺陷。
- `btlr-frame-vml.docx`／`tbrl-frame-vml.docx`（純 VML `layout-flow:vertical;mso-layout-flow-alt:bottom-to-top`，拉丁文）：LO 實測**整行旋轉**（btlr dir=[0,-1] bbox=[79.2,72.6,100.3,170.5]；tbrl dir=[0,1] bbox=[100.9,358.9,121.9,456.8]，起點在框底），我方走 `txbx_vertical` 逐字堆疊臂發 12 個單字項——VML 臂對拉丁文同屬偽口徑，但本卡鐵條明文禁改舊臂，需 PM 裁與 `txbx_vertical` 鎖（k4e2／lamp5）的切分方式。
- `groupshape-sdt.docx`（lamp5 eaVert 鎖對象）：LO 抽取**零文字行**（`get_text` 空＋`get_drawings` 無框），故 lamp5 是自我行為鎖、無 LO 真值背書。
- `fdo74605.docx`（eaVert 實檔）：LO 全橫排 128 行零直行——corpus 目前**沒有任何 CJK 直排 LO 真值檔**，逐字堆疊臂的對錯在 corpus 內不可證。
- `n779627.docx`（VML layout-flow:vertical）：LO 全橫排（a-e 五行 dir=[1,0]）——VML 宣告不必然直排，LO 按內容語言判定，逐字堆疊臂不得對拉丁文強制觸發的第二鐵證。
