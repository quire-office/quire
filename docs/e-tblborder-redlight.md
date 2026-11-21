# E 卡：表格邊框（tblBorders／tcBorders）紅燈報告

任務：t_fe46914f（分支 `wt/e-tblborder`）。U4 t_e1b68330 收案點名、PM 實查坐實的
引擎缺口——`TablePropsPatch`／`CellPropsPatch` 無任何 border 欄位，serde 對未知欄位
靜默吞掉回 ok=true（「成功但零效果」形態）。本卡補寫入欄位＋顯示列表 Rule 發射，
先紅後綠，六盞全綠收案。

繁中註解、署名 ADT109119、未 push（照紀律等顏群學口令）。

## §0 結論速覽

| 項目 | 狀態 |
| --- | --- |
| 寫入欄位（`BorderSpec`／`TableBordersPatch` 進 `TablePropsPatch`／`CellPropsPatch`） | 落地（宣告位 `mutation.rs`，寫入落點 `mutation_k29c.rs`） |
| 顯示列表發射 | 既有 `push_frag_items` 四邊臂承擔（本卡零改動，經燈 1／2／3／6 實鎖） |
| 無損紅線 | 空補丁 byte_identical；有值補丁僅觸碰 `word/document.xml`（燈 4／5） |
| Undo | StrictByteReversible，Undo 後整檔 byte_identical（燈 5） |
| 考卷 | `core/engine-api/tests/e_tblborder.rs` 六盞：綠態 6 passed／0 failed |
| 鑑別力 | `E_TBLBORDER_BUGGY=1` ⇒ 燈 1／燈 2 必紅（實跑 2 failed／4 passed） |
| `RowPropsPatch` | 不加欄位——OOXML `w:trPr` 無邊框元素（ECMA-376 §17.4.81），加了必成死欄 |

## §1 合成探針件設計

`fixtures/e_tblborder_probe.docx`（產生器 `scripts/gen_e_tblborder_fixture.py`，
純 stdlib、STORE 壓縮、固定時間戳＝byte 級可重現；合成件零真檔零 PII）。

- 表 0（2×2）：`w:tblBorders` 直接格式六方位——top single sz16 FF0000／left single
  sz8 auto／bottom none／right none／insideH dashed sz12 0000FF／insideV none。
  考 sz→pt 換算、none 不發射、inside 只畫內部、虛線。
- 表 1（1×2）：tblBorders 全 none ＋ cell(0,0) `w:tcBorders` top single sz24 00B050／
  bottom none／left nil／right hidden。考「宣告即贏」覆寫與 none／nil／hidden
  三態皆不發射。

## §2 LO 25.8 真值與我方逐條對照（探針 2026-11-13/14）

LO 側：`scripts/e_tblborder_lo_probe.py`（Linux soffice 25.8 ＋ 獨立
UserInstallation；pymupdf `get_drawings()`）。我方側：`page_display_list` 的
`DrawItem::Rule` 逐條（臨時診斷檔實跑打印後刪除，輸出逐值抄錄如下）。

| 邊 | 宣告 | LO 實測 | 我方發射 | 判定 |
| --- | --- | --- | --- | --- |
| 表 0 top | single sz16 FF0000 | 線 (71.0,87.20)→(287.5,87.20)，w=2.0pt，實線 | 紅 2.0pt 兩段 (72,88.80)→(180)/(180)→(288) | 寬逐條吻合（sz/8）；y 差 1.6pt＝行高度量差（§6） |
| 表 0 insideH | dashed sz12 0000FF | (72.0,103.15)→(287.5,103.15)，w=1.5pt，dashes `[ 8 2.5 ]` | 藍 1.5pt 虛線（dash=`"3 2"`）四段 y=105.60 | 寬吻合；只在列交界＝inside 語義吻合；dash 字串口徑不同（§6） |
| 表 0 left | single sz8 auto(黑) | 垂直 (71.5,86.2)→(71.5,118.1)，w=1.0pt | 黑 1.0pt 兩段 x=72.0（y 88.8→105.6→122.4） | 寬吻合；LO 外框線中心外飄半線寬 0.5pt（§6） |
| 表 1 cell(0,0) top | tcBorders single sz24 00B050 | (72.0,133.80)→(180.0,133.80)，w=3.0pt | 綠 3.0pt (72,139.20)→(180,139.20) | 寬吻合；y 差 5.4pt＝跨表累積行高差（§6） |
| 表 0 bottom／right／insideV（none） | none | 零線段 | 零發射 | 吻合 |
| 表 1 五邊（none）＋cell left(nil)／right(hidden)／bottom(none) | none／nil／hidden | 零線段 | 零發射 | 吻合 |

- **sz→pt 換算逐條吻合（零容差）**：sz16→2.0／sz12→1.5／sz8→1.0／sz24→3.0——
  LO 渲染寬與我方 `width_pt` 完全同值，坐實 `solve_pen` 的 `sz/8` 口徑與寫回
  `×8` 反函數閉環。
- **計數形態差**：LO 把共線邊框併成單一路徑（4 條），我方按格發射（9 條 Rule，
  共用邊重疊）——既有 Bug54771b 居中發射契約，非缺發／多發。
- 燈 3 以 `assert_eq!(all.len(), 9)` 釘死我方基準總數，防止下游卡改壞換算尺。

## §3 寫入層實作與取捨

- 序列化：`border_edge_xml` 缺欄不寫屬性（全缺＝`<w:top/>`）；`w:sz`＝
  `round(width_pt×8)`；`w:space`＝`round(space_pt)`；`w:color` 原字串透傳。
- 併入語義（`merge_borders_body`）：逐方位獨立——補丁觸碰的方位先
  remove 再 append（既有 remove-then-append 口徑），未觸碰子元素（含
  `w:tl2br`／`w:tr2bl`／未知自訂元素）逐字元原樣保留；容器不存在＝新建。
  空補丁＝`body` 原樣回傳（零擾動，燈 4(a) byte_identical 釘死）。
- 儲存格級 `cell_scope()` 先丟棄 insideH／insideV（`w:tcBorders` 無此兩方位，
  ECMA-376 §17.4.10），傳了＝未宣告、零擾動（燈 2 以 `sz="72"` 零滲漏釘死）。
- 已知取捨（老實記帳，不修）：
  (a) 被覆寫方位移到容器尾 ⇒ 容器內方位序偏離 schema 正則序（LO／Word 讀取端
      方位序無關，探針件 LO 25.8 正常渲染實證；K29c 同款口徑）。
  (b) `w:start`／`w:end` 別名方位不被併入器辨識為 left／right 既有宣告（語料
      直接格式層未見此寫法；並存時由解析端後寫贏前決定）。
  (c) 病態未閉合容器＝不動原文、新容器附加 body 尾（老實妥協，正常語料不觸發）。
- 試錯記帳（run 284）：`merge_borders_body` 首版（run 280）`open_tag` 少閉角
  括號 ⇒ 新建容器 ill-formed，五盞紅在解析層；另考卷燈 4 keep 清單三條臆造值
  （未實測 fixture 就寫入）＋診斷 helper `container_segment` 首版 off-by-one
  （游標只前移 1 字元）均由實跑抓出修正。教訓重申：**斷言值必逐字取自產生器
  實文**，跨容器斷言必限縮段。

## §4 鑑別力實跑（BUGGY 回塞）

- 綠態：`cargo test -p quire-engine --test e_tblborder` ⇒ **6 passed／0 failed**。
- 回塞態：`E_TBLBORDER_BUGGY=1 cargo test -p quire-engine --test e_tblborder`
  ⇒ **2 failed／4 passed**（燈 1／燈 2 的期望常數被換成錯誤 sz ⇒ 該兩盞必紅），
  證明寫入面斷言真在比對存檔件明文而非恆真。兩態同 binary（env 切換）。

## §5 `w:space` 單位查證（前 run 無實測宣稱之補正）

run 280 半成品註解宣稱「ECMA-376 §17.4.10 註：框線 space 單位即 pt」**無實測
支撐**，本卡補量：探針 `scripts/e_tblborder_space_probe.py`（單格表、top border
single sz8，`w:space` 依序 0／12／240 三態，LO 25.8 Linux）：

- 三態框線 y（72.5）與文字基線 y（73.23）**零位移**。

結論：LO 25.8 渲染端對 table border `@w:space` **不消費**；本庫解析側
（`quire_ooxml::table::Border::space`）收錄原字串但無消費者（唯一 space 消費者
是段邊框 txbx_borders 高度路徑，與表格邊框無關）。⇒ `space_pt` 欄定位＝**寫入
保真透傳**（存檔不丟資訊、roundtrip 無損），本卡不宣稱其渲染語義；pt 口徑沿用
本庫 `txbx_borders.space_pt` 既有約定。若未來實測到有消費者，改此處即可。

## §6 既有缺口／給 PM 的下一步建議（非本卡引入，零掛帳燈——六盞皆正向鎖）

1. **行高度量差**（表 0 頂邊 y 差 1.6pt、insideH 2.45pt、表 1 頂邊 5.4pt）：
   無宣告列高時 LO 與我方各行高由各自字體度量推得，跨表累積。屬既有行高
   度量族（K6/k6cal 查表尺）範疇，非邊框缺陷；燈 3 對 insideH 採 ±2.5pt 容差。
   建議：歸行高度量卡，勿在邊框側硬修。
2. **外框半線寬**：LO 把表格最外圈框線中心外飄半線寬（實測 left 線 x=71.5 ＝
   表界 72.0 − 0.5）；我方四邊恆居中於格線（Bug54771b 契約）。1pt 邊框可見差
   0.5pt。建議：若 SVG 對映要貼 LO，需外框臂單獨 inset——但會與居中契約衝突，
   需 PM 先裁契約方向，本卡不動。
3. **虛線字串**：LO `[8 2.5]` vs 我方 `"3 2"`（SVG stroke-dasharray 口徑）。
   本卡只鎖「虛線可區分」旗標；逐字對齊需先定義 SVG 側目標尺，建議另卡。
4. **`w:themeColor`／`w:themeFill`**：`quire_ooxml::table::Border` 收錄不查表
   （既有 TODO），theme 色邊框會渲染成黑色 fallback。既有缺口，非本卡引入。
5. **Ribbon「表格邊框」鈕**：引擎欄位已就緒，外殼接線歸 U4c 卡——本卡不動
   `shells/web/editor/index.html`。

## §7 閘門紀錄

見本卡 kanban 收單 metadata（ow-gate.sh 八閘 fail=0 實跑輸出）。
