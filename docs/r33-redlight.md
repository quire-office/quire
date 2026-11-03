# R33 紅燈：sort_display_items 閱讀序與 LO 發射序分岔（WC048 內容分 0.688 殘差）

測試檔：`core/engine-api/tests/r33_display_order.rs`
標的：`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` p0、
      `corpus/xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx` p0
LO 二進位：`~/.local/lo-linux/opt/libreoffice25.8/program/soffice`（25.8，headless）
抽真值：pymupdf（`/usr/bin/python3`）；分數口徑＝`scripts/render_lab.py`（同一支
`content_key`／`layout_key`／`SequenceMatcher(autojunk=False)`，未改度量一字）

## 現行碼實拍（接手起點修前紅燈）

```text
$ cargo test -p quire-engine --test r33_display_order
running 4 tests
test r33_buggy_env_restores_red ... ok
test r33_wc049_p0_text_sequence_matches_lo_stream ... FAILED
test r33_wc048_p0_text_sequence_matches_lo_stream ... FAILED
test r33_fdo76591_inline_vml_stays_before_body_flow ... ok

---- r33_wc048_p0_text_sequence_matches_lo_stream stdout ----
assertion `left == right` failed: WC048 p0 顯示列表文字序列 ≠ LO pymupdf span 流序
（浮動文字框應整層排在表格文字之後，修前被 y-sort 打散交錯）
  left: ["This is a test.", "This is a test.", "Textbox1", "Textbox2",
         "This is a test.", "This is a test.", "This is a test.", "This is a test.",
         "Textbox3", "This is a test.", "This is a test.", "This is a test."]
 right: ["This is a test.", "This is a test.", "This is a test.", "This is a test.",
         "This is a test.", "This is a test.", "This is a test.", "This is a test.",
         "This is a test.", "Textbox1", "Textbox2", "Textbox3"]

---- r33_wc049_p0_text_sequence_matches_lo_stream stdout ----
assertion `left == right` failed: WC049 p0 詞序 ≠ LO pymupdf span 詞序
  left:  "... your document. textbox1 this is a test. to make your document look ..."
 right:  "... your document. to make your document look ... different galleries. textbox1 this is a test."

test result: FAILED. 2 passed; 2 failed
```

## LO 真值（pymupdf span 流序，非幾何序）

WC048 p0（12 span）——表格 9 句依**儲存格序**連續發射（首列 cell1 被 Textbox1
wrap 推到 y=112.2 仍排該列第一位），三個浮動框整層**最後**發射，層內按
`wp:anchor/@relativeHeight` 遞增：

| LO 流序 | y | x | 文字 | 所屬層 | relativeHeight |
|---|---|---|---|---|---|
| 0 | 112.2 | 77.8 | This is a test. | 表格首列 cell1 | — |
| 1–2 | 72.7 | 228.0 / 378.2 | This is a test. ×2 | 表格首列 cell2/3 | — |
| 3–5 | 138.6 | 77.8 / 228.0 / 378.2 | This is a test. ×3 | 表格第 2 列 | — |
| 6–8 | 345.2 | 77.8 / 228.0 / 378.2 | This is a test. ×3 | 表格第 3 列 | — |
| 9 | 90.7 | 64.8 | Textbox1 | 浮動層 | 251659264 |
| 10 | 108.9 | 171.8 | Textbox2 | 浮動層 | 251661312 |
| 11 | 194.8 | 86.7 | Textbox3 | 浮動層 | 251663360 |

我方修前把 Textbox1（我方 y=90.4）／Textbox2（y=97.6）插在首列文字中間、
Textbox3（y=162.5）插在第 2、3 列之間 → SequenceMatcher 詞序分 0.688。

WC049 p0（11 span）同樣是「正文發完才發浮動框」：LO 末位＝
`Textbox1 this is a test.`（框身 y=118.6 落在正文中段，LO 仍發最後）。
WC048-Mod／WC049-Mod 兩檔形態相同（真值同表）。

## 診斷：LO 的文字流是**兩層**，不是一條 y 軸

`core/engine-api/src/lib.rs:sort_display_items` 把**所有** Text 項塞進同一把
(y, x) 鍵。對正文與表格這是對的；對「帶 z 宣告的浮動圖層」是錯的——LO 把該層
整層壓到正文層之後發射，層內按 `relativeHeight`（z 序）。故 y 軸把框文字錯插
回正文中間。

### 全 corpus 覆核（規則是否純函數、可否統一排序鍵）

判別法：由 OOXML 抽每個 `wp:anchor` 的框內段文字，在 LO 該頁 span 流序中定位，
看框文字是否構成**後綴**、框之間是否按 relH **遞增**。

| 標的 | LO 實拍 | 後綴？ | relH 單調？ |
|---|---|---|---|
| WC048 p0 | Textbox1/2/3 @9,10,11（末三位） | 是 | 是 |
| WC048-Mod p0 | Textbox1a/2a/3a @9,10,11 | 是 | 是 |
| WC049 p0 | Textbox1 @10（末位） | 是 | 是 |
| WC049-Mod p0 | Textbox1 @11（末位） | 是 | 是 |
| HC031 p7 | `Here is a text box.` @55/56（末位，relH 251654656） | 是 | 是 |
| tdf130120 p0 | 唯一 span 就是框內「7」 | 是（全頁皆框文字） | 是 |
| shapes-with-text p0 | 全部 8 span 都是框文字（無正文），序＝relH 遞增 | 是 | 是 |
| fdo76591 | 三個框是 `wp:inline`（relH **缺＝0**），LO 照段落序夾在正文中間 | **不套** | 不套 |
| n751077 | `v:group` VML（relH 缺＝0），K25-B 逐框排版已校準 | **不套** | 不套 |
| fdo74605 p0 | SmartArt 51 個 wps 標註混排（既有 R6 校準，p0 純 SmartArt 無正文） | 由 R6 口徑管 | R6 已校 |

結論：**「帶 relativeHeight 宣告的浮動框 → 第二層、層內依 relH」是全 corpus 一致的
純函數規則**，不需按 shape 類型查表硬排，也不需要 WC048 專屬特例。兩條關鍵邊界：

1. `wp:inline` 與 VML（`v:shape`／`v:group`，無 relH 宣告）屬**段落流**，LO 照
   文字流發射（fdo76591 實據：框標題 IMPORTANT NOTICE／Scheduled Saturday
   Overtime 先於正文 UNLESS…）⇒ 規則對 `rel_height == 0` 一概不套。
2. R5 群組鍵（同段多框）與 R6 relH 口徑已在既有卡校準 ⇒ 圖層鍵放**最外層**、
   層內次鍵沿用 R5 群組鍵，同 relH 的群組子框維持推入序（shapes-with-text
   「Where→A group」逐位不變，該鎖零擾動）。

## 修法（只動允許範圍：`sort_display_items` 與其排序鍵）

- 排序鍵由 6 元組升為「圖層帶 + 6 元組」：`0`＝填色/框線（不動）、`1`＝正文層
  （舊 y,x／R5 群組鍵，逐位不變）、`2`＝浮動層（主鍵 `relativeHeight`）。
- 新函式 `r33_layer_key(b)`：資格＝`floating && rel_height != 0`；資格不合一律
  回 `None`＝舊 y,x 口徑（零擾動守門）。
- 發射層只在**浮動圖層路徑**（`p.floating_images` 配對到的框）蓋圖層鍵；表格切片、
  正文流、頁首尾、註腳／尾註路徑一概不動。
- 回塞界縫對齊：圖層鍵屬 z 序抽取口徑家族，`R6_CLIP_BUGGY`／`R5_R2_BUGGY` 一併
  退回舊 y-sort（兩卡既有回塞鎖語意逐位不變）；`R33_LAYER_BUGGY` 只關本卡。

## 修後綠燈驗收

```text
$ cargo test -p quire-engine --test r33_display_order
running 4 tests
test r33_buggy_env_restores_red ... ok
test r33_wc048_p0_text_sequence_matches_lo_stream ... ok
test r33_wc049_p0_text_sequence_matches_lo_stream ... ok
test r33_fdo76591_inline_vml_stays_before_body_flow ... ok
test result: ok. 4 passed; 0 failed

$ R33_LAYER_BUGGY=1 cargo test -p quire-engine --test r33_display_order
test result: FAILED. 2 passed; 2 failed      # 燈 1／燈 2 必紅＝回塞鎖精確命中
                                             # 燈 3（fdo76591 零回退鎖）仍綠
```

## render_lab 逐檔前後對照（度量口徑一字未改）

同一份 LO PDF 真值，只換我方顯示列表（LO 一側全同，故差異純來自本次改動）：

| 檔 | 內容分 修前→修後 | 版面分 修前→修後 | 備註 |
|---|---|---|---|
| WC048-Text-Box-in-Cell | **0.688 → 1.000** | 0.649 → 1.000 | 卡面目標 ≥0.80 達成 |
| WC048-…-Mod | **0.705 → 1.000** | 0.644 → 1.000 | 同形態孿檔同步 |
| WC049-Text-Box-in-Cell | **0.955 → 1.000** | 0.807 → 0.807 | 浮動框文字歸位末位 |
| WC049-…-Mod | **0.935 → 1.000** | 0.810 → 0.772 | 見下方單檔說明 |
| fdo76591（R31 剛收） | 0.987 → 0.987 | 0.987 → 0.987 | 零回退（inline VML 不套） |
| n780563 | None → None | None → None | 雙空＋LO 有圖形（度量不可比） |
| tdf130120 | 0.500 → 0.500 | 0.500 → 0.500 | 零回退 |
| shapes-with-text（R5 鎖） | 1.000 → 1.000 | 0.985 → 0.985 | 逐位等序 |
| fdo74605（R6 鎖） | 0.750 → 0.786 | 0.454 → 0.749 | p0 0.927→0.999（SmartArt 層歸位） |
| HC031 | 0.897 → 0.898 | 0.798 → 0.798 | p7 0.984→0.994（`Here is a text box.` 歸末位） |
| fdo68607 | 0.880 → 0.880 | 0.352 → 0.377 | p0 0.753→0.951 |
| n751077 | 0.440 → 0.440 | 0.433 → 0.433 | 零回退（VML group relH 缺＝不套） |

### WC049-Mod 版面分 0.810 → 0.772（降 0.038，單檔說明）

`layout_key` 只取**前 8 行**逐行拼接（度量卡既有口徑，本次未改）。該檔正文是
9 行，浮動框文字修前錯插在**第 5 行**（窗口內），LO 真值在**末位（窗口外）**：

- 修前：窗口內錯位文字意外填補了 LO 的第 4 行環繞短行 → 前 8 行窗口 0.810。
- 修後：框文字移出窗口 → 前 8 行窗口 0.772。
- 逐 opcode 核對：差異全部來自 LO 把「You can also type a keyword …」一列切成
  4 個環繞短行（LO 12 行 vs 我方 9 行）——這是 `wrapText` 環繞斷行的**版面軸既有
  缺口**，非本次改動引入。
- 鐵證：同一比對改採**全行清單**口徑 ⇒ 修前 0.9290、修後 **0.9950**（修後才是對的
  一側）；內容分 0.935 → 1.000 同步佐證。
- 結論：這是 8 行截斷窗口的度量偽影（方向＝修前僥倖、修後誠實），非引擎回退。
  修訂版面度量口徑屬 `render_lab` 度量卡禁區，本卡不碰，如實記錄交 PM 裁。

### 全庫零回退覆核（25 檔 scoreboard 池）

BUGGY（本次改動關閉）vs 固定版，同檔同 LO 真值逐檔比：22 個可測檔（44 個
content/layout 比較項）Δ **全部恰為 0.000**，回退(>0.02) 清單＝空。改動只影響
「有 relativeHeight 宣告的浮動框」那三個標的族，其餘檔案顯示列表逐位不變。

## 殘留（本卡範圍外，老實記錄）

- `R6_CLIP_BUGGY=1` 回塞鎖在 **main（fa2e3d9）即已紅**：實測 `p1 可抽行 134`，
  斷言要 `>140`。已於 main 與本 worktree 雙次重現（同 134），**非本次改動引入**；
  正常 `cargo test --workspace`（不設 env）該鎖綠。
- fdo74605 p1／n751077／patho 等檔的內容分地板屬其他軸（SmartArt 裁切、
  v:group 排版），不屬閱讀序，本卡不動。
