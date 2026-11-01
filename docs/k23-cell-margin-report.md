# K23 報告：表格欄斷行寬「未扣 cell 內距」實錘結果——假說否證＋兩個真缺陷修復

任務卡：`K23 表格爆欄：斷行寬扣 cell 內距`（t_ea7d1b8a）
紅燈測試：`core/engine-api/tests/k23_cell_text_width.rs`（10 測試）

---

## 1. 卡面根因假說 ＝ 否證（PM 初查不成立）

卡面假說：「斷行器給第二欄的行寬 ≈ 欄全寬（425pt），未扣 `w:tcMar`／預設
cell 內距與框線寬」。實測鐵證四連：

1. **源碼層**：`core/layout/src/table.rs::resolve_margins` 自 K4 起即逐側解析
   內距，且 `inner_w = (w − margins.left − margins.right).max(8.0)` 在
   `layout_table` 內確實扣除。fdo68607 表格 `w:tblCellMar` 宣告左右 108 dxa
   ＝5.4pt/側 ⇒ 第二欄可用排版寬 425 − 10.8 ＝ **414.2pt**（非 425）。
2. **斷行模型層**（display list，按發射 face 的 K7-cal 0.8834 水平尺）：
   fdo68607 全 8 頁第二欄 316 條文字切片，推估右緣最遠 **576.0pt ＜
   格右界 581.4pt，越界 0 條**。（若退 scale=1.0 則 630.6pt／越界 169 條——
   PM 的 ~626pt 量法即錯在沒帶 face 水平尺。）
3. **渲染層**（Chrome 實渲我方 SVG，修 A 後）：第二欄 42 條、越 581.4pt＝
   **0 條**、最右 563.8pt。
4. **LO 25.8 同頁真值**：第二欄文字 x1 max 568.8／577.2，越界 0。

與 `docs/k7-wrapwidth-diag.md` §1「缺陷 B：cellMargins 漏扣假說＝**否證**」
一致（本卡為第二獨立復測）。**PM 看到的 ~626pt 爆欄是渲染端假影**：把 SVG
文字強制換成 wide fallback face 重測 → 最右 **665.6pt／29 條越界**，正是
卡面描述的視覺。

框線寬口徑：LO 實測格線中心 580.9 vs 我方幾何 581.4（k7_wrap_width 同口徑
±1.0pt），**未**在行寬內再扣框線——LO 口徑＝內距扣、框線不扣（框線筆畫居中
壓在格線上，不佔排版寬）。維持現狀，零改動。

## 2. 真缺陷 A（視覺爆欄真因）：格內文字發射不宣告字型族

`core/engine-api/src/lib.rs::push_frag_items` 的兩條格內發射路徑（單樣式整行
＋多樣式段級）恆發 `family: None`（原始碼註解「格內選族待後續卡」）⇒
SVG 不寫 `font-family` ⇒ 渲染端選自身預設 face；而斷行按 Calibri 0.8834 尺
收斂。**斷行尺與發射宣告不同 face** ⇒ 同一行被拉寬 13–16% 後右緣越線。

修：`resolve_cell_line_style` 加回傳族（首個覆蓋本行的 run 決定族，正文
`emit_line_run_items`「合併段選族採首個 run」同款口徑），單樣式與段級兩條
路徑都接上。回塞鎖 `K23_CELL_FAMILY_BUGGY=1`。

實測：fdo68607 p0 SVG `font-family="Calibri"` **62/62 條**（修前僅 1 條標題；
BUGGY 態回塞 41/42 條無族）。燈 1a／1b／1c 鎖。

## 3. 真缺陷 B：`w:tblCellMar` 內距級聯漏了「表格樣式級」

修前級聯：直接 `w:tcMar` ＞ `w:tblPr/w:tblCellMar` 直宣告 ＞ ECMA 預設
（108 dxa＝5.4pt/側）。**表格樣式（`w:tblStyle` → `word/styles.xml` 的
`w:style/w:tblPr/w:tblCellMar`）整級從未進級聯**。

全 corpus 掃 152 有表檔，3 檔受害（LO 25.8 ground truth 實測）：

| 檔案 | 樣式宣告 | 修前我方 | LO 真值 | 修後我方 |
|---|---|---|---|---|
| `ooxmlimport/tdf115883.docx` | 0 dxa | x0 41.4（預設 5.4 錯入） | **x0 36.2** | **x0 36.0** |
| `ooxmlexport/NumberedList.docx` | 86 dxa | 內距 5.4 | **內距 4.3** | **內距 4.3**（x0−界=94.3−90.0） |
| `ooxmlexport/tdf103976.docx` | 86 dxa | 內距 5.4 | **內距 4.3** | **內距 4.3**（x0−界=94.3−90.0） |

修（級聯定案，ECMA-376 §17.4.40／§17.4.49＋LO 實測）：

```
直接 w:tcMar ＞ w:tblPr/w:tblCellMar ＞ 樣式級（本卡新增）＞ ECMA 預設
```

- 解析：`core/engine-api/src/table_map.rs::table_style_cell_margins`——
  `word/styles.xml` 逐 `<w:style w:type="table">` 收 `tblCellMar` 四側，沿
  `w:basedOn` 鏈合成（子先寫、父只補缺側 ⇒ 子贏父；鏈長上限 16＋拜訪集防
  環；自閉合 `<w:tblCellMar/>`＝不認）。**宣告 0 是有效宣告**，必須壓過父
  樣式與預設（tdf115883 口徑）。
- 注入：`TableInput::style_cell_margins`（serde default＋空值跳過＝舊快照
  逐位元組不動）；查表口徑同 `style_borders`——只認本表自帶 `w:tblStyle`。
- 消費：`resolve_margins(cell, table, style)` 逐側 `c.or(t).or(s).unwrap_or(預設)`。
- 回塞鎖 `K23_STYLE_CELL_MAR_BUGGY=1`（燈 3／燈 4／燈 4b 必紅）。
- `tools/line-metrics`（行度量工具，白名單外 +3 行）：不載入 styles.xml ⇒
  傳 `style_cell_margins: None`＝樣式級不參與，逐位元組維持修前行為。

## 4. 紅燈測試設計（燈2 的「回塞行數必變」如何实现）

卡面燈 2 要求「欄寬 200pt、tblCellMar 左右各 108twip：斷行行寬必＝
200−5.4−5.4；回塞（用全寬）行數必變」。ECMA 預設 108twip 與直接宣告
108twip 數值相同 ⇒ 「回塞成不扣內距」用 env 開關做會汙染**全部**未宣告
內距的檔（爆炸半徑=全 corpus）。定案：燈 2 以**判別 token 雙斷行**實現
——同一 token 在 `break_lines(189.2)` 與 `break_lines(200.0)` 下行數必不同
（6 vs 5 行），引擎端到端行數鎖死 6 行側；「未扣內距＝全寬斷行」態由
斷行器現算反證，不設全域 env。樣式級 env 鎖不影響燈 2（本 fixture 樣式
級無宣告＝零擾動，buggy/fixed 同值）＝燈 2 兼任「樣式級零擾動」反证。

合成 fixture（`core/engine-api/tests/fixtures/`，產生器語義見檔頭註解）：

| fixture | 內距來源 | fixed 行寬 | buggy 行寬 | 判別（fixed/buggy 行數） |
|---|---|---|---|---|
| `k23_style_mar.docx` | 樣式級 86dxa×2 | 191.4 | 189.2 | 2 / 3（燈4） |
| `k23_mar_per_side.docx` | tblPr 直 left=0＋樣式級 right=0 | 200.0 | 194.6 | 6 / 7（燈4b） |
| `k23_tcmar_override.docx` | cell tcMar left=1080＋樣式級 right=0 | 146.0 | 140.6 | 8 / 9（燈3） |
| `k23_cellmar_108.docx` | tblPr 直 left/right=108 | 189.2 | （同值，無樣式級） | 6；全寬斷 5（燈2） |

判別窗由 probe 逐字掃描選定（最寬發射行落 `(buggy, fixed]` 且離兩界最遠），
每檔 x0 鎖＋最寬行鎖雙重防假綠。

燈 5（零擾動鎖）：fdo68607（無樣式級宣告）第二欄 x0 必須仍 161.8、p0 行數
42±4——防樣式級查表誤命中／虛構宣告。

## 5. 回塞實測（非假綠證明）

```
固定態：      10 passed / 0 failed
K23_STYLE_CELL_MAR_BUGGY=1： 燈3、燈4、燈4b 必紅（7 pass / 3 fail）
K23_CELL_FAMILY_BUGGY=1：    燈1a、燈1b、燈1c、回塞鎖A 必紅（6 pass / 4 fail）
```

## 6. 驗收核對

- ow-gate 八閘 `GATE_SUMMARY fail=0`（roundtrip corpus --check 全綠＝parts
  零觸碰；goldens 閘 PASS＝金樣無需重生成——本卡行寬僅在「樣式級宣告
  tblCellMar」的檔改變，corpus 3 檔受益者不在 goldens 合成集內）。
- `render_lab.py corpus/ooxmlexport/fdo68607.docx`：LO 8 頁／我方 8 頁、
  真缺字 **0**、內容 [0.953, 0.92, 0.906, 0.883]、版面 [0.753, 0.438,
  0.399, 0.25]。版面分與 K21 基線**持平**：該指標比對我方 display list 的
  文字行序列，看不到 `font-family` 宣告——卡面所見爆欄是渲染端 face 假影，
  修復面（SVG 62/62 宣告 Calibri＋Chrome 實渲越界 0）在本卡 §2 獨立實錘，
  不影響版面分（指標盲區，非未修）。
- 全 corpus A/B（`K23_STYLE_CELL_MAR_BUGGY`＋`K23_CELL_FAMILY_BUGGY` 雙 on/off，
  ooxmlexport 185＋ooxmlimport 70＋poi 60＝**315 檔逐檔頁數**）：
  **頁數位移 0**——純排版微調（內距 5.4→4.3/0pt、發射宣告 face）零分頁擾動。
- K8 底紋尺、K17/K20 撐列語意：零回退（紅燈 k8/k17/k20 全綠於
  `cargo test --workspace`）。

## 7. 觸碰面

- `core/engine-api/src/lib.rs`：`resolve_cell_line_style` 回傳族＋兩發射路徑
  接上＋`K23_CELL_FAMILY_BUGGY` 鎖。
- `core/engine-api/src/table_map.rs`：`table_style_cell_margins`＋
  `find_tbl_cell_mar`＋`fill_missing_margins`＋`table_input` 注入。
- `core/layout/src/table.rs`：`TableInput::style_cell_margins`＋
  `resolve_margins` 三級＋`K23_STYLE_CELL_MAR_BUGGY` 鎖。
- `tools/line-metrics/src/tables.rs`：+3 行補新欄（None＝舊行為；白名單外，
  結構體新增欄位的編譯必需）。
- `core/engine-api/tests/k23_cell_text_width.rs`＋4 支合成 fixture（新增）。
- `core/ooxml/src/table.rs`：**零觸碰**——直接 `w:tcMar` 解析鏈早已存在
  （`tcPr/tcMar` → `props.margins` → `CellInput::margins`），燈 3 端到端鎖之。
