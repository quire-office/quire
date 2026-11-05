# E1 紅燈：分頁缺口兩席（實拍＋LO 真值＋卡面前提更正）

- 測試檔：`core/engine-api/tests/e1_pagination.rs`（新增，6 盞燈）
- 基線：本分支 `wt/t_a14a6dd6`（merge-base `1cc26b3`），**未動任何引擎碼**
- LO 二進位：`~/.local/lo-linux/opt/libreoffice25.8/program/soffice`（LO 25.8，Linux）
- 抽真值：pymupdf 1.28.2（`/usr/bin/python3`），口徑＝`page.get_text("dict")` 逐
  `blocks→lines→spans`，行文字＝同 line 全部 `span.text` 串接後壓空白（與
  `scripts/render_lab.py::pdf_page_facts()` 完全同路徑）；座標單位＝PDF pt，原點左上
- 我方口徑：`quire-cli dump`（顯示列表 JSON，與 SVG 同源）；頁數＝`quire-cli info`

## 0. 卡面前提更正（三條，全部有可複驗實據）

這三條直接決定任務能不能照卡面做，PM 先讀。

1. **`corpus/ooxmlexport/cell-grid-span.docx` 沒有 gridSpan。**
   `unzip -p <檔> word/document.xml | grep -c gridSpan` → **0**（實測輸出 `0`；
   `vMerge` 同 0）。該檔只有一張表：`w:tblGrid` 5 欄（16/15/15/15/11321 twip）、
   **1 列 5 格全空**（每格一個空 `w:p`、`w:hideMark`、`w:vAlign=center`）。
   ⇒ 卡面「源檔是跨列表格，表格內容超出單頁高度時我方不增長頁面——須查 gridSpan
   表格是否走錯分支或高度估算歸零」的前提不成立：該檔無跨列儲存格，且表格本身
   只有 1 列，不可能「超出單頁高度」。實測該表在 LO 與我方都只吃約 1 列高
   （LO：`F1`→`F2` 間隔 45.0pt，含空表列；見 §3 消融 M5）。
2. **「跨列表格切片後 gridSpan 欄寬與合并不變形」不成立為缺陷——它本來就是對的。**
   合成表（5 欄網格 80/1/1/80/200pt、首格 `grid_span=3`、30 列高）走
   `TableLayout::fragment(·,·,20 帶,·)` 出 7 片切片，**每片每格 `(x_pt, w_pt)`
   逐位元組相同**：跨列格 `(0.0, 82.0)`＝前 3 欄和、其餘 `(82.0, 80.0)`／
   `(162.0, 200.0)`。⇒ 卡面燈 2 改為**正向鎖**（比照 `docs/k25-empty-page-diag.md`
   的 K25 純鎖定位），見 §4 燈 2。
3. **CoverPage 的 `Quire`／`Corpus` 是「缺發」不是「多發」。**
   LO p0 實測**渲染了** `Quire Corpus`：第 5 個文字行，bbox
   `(285.8, 526.5, 344.5, 539.9)`。我方同位發射 `Eric White`（bbox 起點
   `(281.5, 526.7)`），且 `Quire`／`Corpus` 在我方全文**零出現**。
   render_lab 報告本身也把它列在**真缺字**欄（不是多發欄）。
   根因：該檔 `Author`/`Title`/`Subtitle` 三個 `w:sdt` 都是 `w:dataBinding` 內容控制
   （`w:xpath="/ns0:coreProperties[1]/ns1:creator[1]"`，
   `storeItemID={6C3C8BC8-F283-45AE-878A-BAB7291924A1}`），而
   `docProps/core.xml` 的 `dc:creator`／`cp:lastModifiedBy`＝`Quire Corpus`
   （`dc:title`＝`The Title is here`、`dc:subject`＝`And a subtitle`）。LO 以文件
   屬性值覆寫 `sdtContent` 的佔位 run，我方原樣發射佔位 run 內的 `Eric White`。
   ⇒ 卡面燈 4「我方發射 token 集合不含 `Quire`／`Corpus`」若照字面寫，會把 LO 真值
   鎖成錯誤行為。本檔**反轉極性**：必須含 `Quire`/`Corpus`，且不得含 `Eric White`
   （後者才是真多發，LO 全文零出現）。

## 1. 紅燈實拍（本分支，修前）

```text
$ cargo test -p quire-engine --test e1_pagination
running 6 tests
test lamp2_gridspan_slice_geometry_is_locked ... ok          ← 正向鎖（本已正確）
test lamp1_cell_grid_span_page_count_matches_lo ... FAILED
test lamp1_cell_grid_span_second_page_first_last_line ... FAILED
test lamp3_coverpage_page_count_matches_lo ... FAILED
test lamp3_coverpage_second_page_is_physically_blank ... FAILED
test lamp4_coverpage_emits_data_bound_tokens_and_no_placeholder_run ... FAILED
test result: FAILED. 1 passed; 5 failed; 0 ignored
```

燈 2 鑑別力證明（不是恆真斷言）：

```text
$ E1_GRIDSPAN_EXPECT_BUGGY=1 cargo test -p quire-engine --test e1_pagination lamp2
test lamp2_gridspan_slice_geometry_is_locked ... FAILED
切片 0 列#0 格#0 幾何 (-0,82) 偏離首片基準 (0,80)——跨列切片後欄寬/合併不得變形
```

我方現行頁數（`quire-cli info`）：`cell-grid-span` 1 頁、`NewDocxDocuments_CoverPage`
1 頁；兩者 `page ... 1` 皆回報「頁碼 1 不存在（共 1 頁）」——即卡面所述「p2 我方完全
無 pagebreak 記錄」屬實。

## 2. LO 真值（pymupdf 實測，零「約」）

### 2.1 `corpus/ooxmlexport/cell-grid-span.docx`（LO 2 頁，612.0×792.0）

- p0：**33 個文字行**（去重後 32 個 `y0`，`About`／`iBall` 同 `y0=507.3` 兩行）、
  2 drawings、0 images。首行 `y0=85.1`
  `On Valentine’s Day, it’s perfect to gift the iBall Andi 3.5`；
  末行 `y0=677.8`／`y1=693.0` `country.`。版心下界＝792 − 1440twip(72pt) = **720.0pt**
  ⇒ p0 末行距下界尚餘 27.0pt，**仍把斷點落在段「`For further media information,
  contact:`」之前**。該段帶 `w:beforeAutospacing`，LO 對其加 14.0pt 段前距
  （§3.1 表 N2/N3 實測常數）；p1 首行落在 `y0=71.5`（≈版心上界 72.0）。
  我方同一行在 p0 的 `y=616.1`、我方 p0 末行 `y=668.8 < 720.0` ⇒ 判為不溢出、不分頁
  （我方 p0 各段邊界普遍比 LO 短 6.7–26.7pt，見 §3.1(A) 邊界對照）。
- p1：**5 個文字行**、1 drawing、0 images。逐行 `y0`／`y1`：

  | 行 | y0 | y1 | 文字 |
  |---|---|---|---|
  | 1 | 71.5 | 86.7 | `For further media information, contact:` |
  | 2 | 101.1 | 116.3 | `Claire George` |
  | 3 | 114.8 | 129.9 | `M:09833043394` |
  | 4 | 128.4 | 143.6 | `L: 022-32229646` |
  | 5 | 142.1 | 157.2 | `E:redacted@example.invalid` |

  （p1 第 1、2 行之間 29.6pt 空隙＝該段 `w:beforeAutospacing`，非分頁造成。）
- 源檔旁證：`w:lastRenderedPageBreak`（Word 存檔當下的渲染提示，**協定上不當斷點用**）
  落在 p1 首段 `For further media information, contact:` 的 `E` run 之前
  （`word/document.xml` 偏移 28520／29516），與 LO 斷點位置同段——LO 真值與 Word
  自己的渲染記帳一致。
- 我方 p0：**36 個不同 `y`**（行數與 LO 38 同量級），末行 `y=668.8`
  （`E:redacted@example.invalid`），距版心下界尚餘 51.2pt ⇒ 我方判為不溢出、不分頁，
  全部內文壓在 1 頁。
- **逐頁內容高度（實測，本節是 §3.1 歸因的唯一數字來源）**：
  LO p0 ＝ 693.0 − 85.1 ＝ **607.9pt**；LO p1 ＝ 157.2 − 71.5 ＝ **85.7pt**；
  LO 全篇流量 ＝ **693.6pt**。我方 p0 ＝ 668.8 − 88.3 ＝ **580.5pt**。
  ⇒ **我方比 LO 少排 113.1pt 垂直流量**，這 113.1pt 就是「少一頁」的全部缺口。

### 2.2 `corpus/xopt/NewDocxDocuments_CoverPage.docx`（LO 2 頁，612.0×792.0）

- p0：**5 個文字行**、36 drawings、0 images。逐行 bbox：

  | 行 | bbox (x0,y0,x1,y1) |
  |---|---|
  | `This is my interesting abstract.` | (325.7, 218.4, 461.6, 231.8) |
  | `The Title is` | (285.8, 280.8, 482.3, 333.3) |
  | `here` | (285.8, 329.9, 364.1, 382.4) |
  | `And a subtitle` | (285.8, 386.9, 391.5, 410.2) |
  | `Quire Corpus` | (285.8, 526.5, 344.5, 539.9) |

- p1：**0 words、0 drawings、0 images、文字空字串**＝物理性空白頁（比照
  n751077／K25 口徑：真空頁照實開頁，不得用刪頁湊分數）。
- 我方 p0：前四行存在（y=72.0 起另有多個空 `Text`），第 5 行發 `Eric White`
  （281.5, 526.7）；p1 不存在。

## 3. 真因定位（含消融實據；全部落在禁改面或需跨層協作）

### 3.1 cell-grid-span 少 1 頁 = 我方總豎向流量比 LO 少 113.1pt（非表格、非切片）

逐頁內容高度（§2.1 實測）：LO 全篇 693.6pt、我方 580.5pt ⇒ **缺口 113.1pt**，
即「少一頁」的全部來源。缺口由兩個**解析/因子**機制構成（各自獨立可測，未做
反事實重排，故本檔不宣稱二者如何瓜分 113.1pt）：

- **(A) 段落邊界距：LO 有 13 處 >14.5pt 的段邊界，我方只有 6 處。**
  `w:beforeAutospacing`／`w:afterAutospacing` **完全未實作**。源檔有 **8 段**宣告
  `w:beforeAutospacing="1" w:afterAutospacing="1"`；`core/ooxml/src/lib.rs:860-864`
  與 `:1110-1114` 只讀 `w:before`／`w:after`，全庫 `grep -rn "utospacing" core/`
  → **零命中**。LO 常數（本環境實測，乾淨基準：docDefaults 零段距＋Calibri 11pt＋
  line 240 auto）：

  | 消融 | 配置 | LO 兩行 `y0` 差 |
  |---|---|---|
  | N0 | 零段距 | 13.5 |
  | N1 | `w:before="200" w:after="200"`（10pt/10pt） | 23.5（＝13.5＋10.0） |
  | N2 | `beforeAutospacing`+`afterAutospacing` | **27.5（＝13.5＋14.0）** |
  | N3 | 只 `beforeAutospacing` | 27.5 |
  | N4 | 只 `afterAutospacing` | 27.5 |
  | N5 | autospacing ＋ `w:before/after="1000"` | 27.5（宣告值被 autospacing 蓋掉） |
  | N6 | 中間插一個 autospacing 空段 | 54.9（＝13.5＋13.5＋14.0＋14.0） |
  | N7 | autospacing ＋ 36pt run | 57.5（＝43.9＋13.6）→ 常數隨字級縮放 |

  具體邊界對照（LO p0 三個大邊界 vs 我方同位）：LO `109.7pt`（`Bluetooth feature…`
  →`Available at…`，跨空表列）／我方同位 `83.0pt`；LO `41.6pt`（`you'd want…`→
  `About`）／我方 `39.3pt`；LO `29.7pt`（`About iBall`→`Team, Passion…`）／我方
  `23.0pt`。⇒ 每個 autospacing 邊界我方少 6.7–26.7pt。
  修復點在 `core/ooxml/src/lib.rs`（欄位＋採集）與 engine 的 `before_of/after_of`
  喂值處——**`core/ooxml/` 是本卡嚴禁面**。
- **(B) `Trebuchet MS` 不在 real-LH 族表 ⇒ 該組每行少 4.1–4.2pt。**
  源檔 100% 宣告 `w:rFonts w:ascii="Trebuchet MS"`、100% `w:sz="20"`（＝10pt），
  另有 `NoSpacing` 樣式段 11pt。LO 側字級分布實測：**21 行 @10.0pt**（Trebuchet 組，
  行距眾數 **15.6／15.7pt**）＋ **17 行 @11.0pt**（`NoSpacing`，行距眾數 13.4／13.5pt
  ＝11×1.2207，對得上 `docs/k6-lineheight-calibration.md` 的 Calibri→Carlito 因子）。
  我方同組只有 **19 行 @10.0pt**（斷行本身也與 LO 差 2 行），行距眾數 **11.5pt**
  ＝ 10 × 後備因子 1.1499（`core/engine-api/src/lib.rs:2592 face_line_factor()`
  的 match 無 `trebuchet`，落 `_ => 1.1499`）。
  ⇒ 僅行距一項：19 行 × (15.65 − 11.50) ≈ **78.9pt**，是 113.1pt 缺口的主體。
  LO 對 Trebuchet 的 real-LH 約 **1.56–1.57**（15.65／10），既不等於 Noto Serif
  1.3620 也不等於任何現存表值——**必須重新實測入庫，不可套用既有因子**。

⇒ **本卡准改面（`core/layout/`、`table_map.rs`、發射清單組裝處）動不到任一成因。**
按鐵律「根因若落在禁改面，停在 comment 回報，不硬改」，引擎零動碼。

### 3.2 CoverPage 少 1 頁 = LO 的空白頁由「錨定圖形 × 尾端 br-page」交互產生

真檔 ET 消融（每格都實際經 LO 開檔，座標同 §2.2 口徑）：

| 變體 | 改動 | LO 頁數 | 我方頁數 |
|---|---|---|---|
| W1 | 原樣（ET 重序列化控制組） | **2**（p1 空） | 1 |
| W2 | 圖形段 → 純文字段（去全部 `mc:AlternateContent`） | **1** | 1 |
| W3 | br 段移出 `w:sdtContent` 到 body 層 | 2（p1 空） | 1 |
| W4 | 解開 `w:sdt`（內容升 body） | 2（p1 空） | 1 |
| W5 | 圖形段整段刪除，只留 br 段 | 1 | 1 |
| W6 | br 段前插一個純文字段 `EXTRA` | 2（p1=`EXTRA`） | 1 |
| W7 | br 段後補一個文字段 `AFTER BR` | 3（p1 空、p2=`AFTER BR`） | 2 |

配合純合成實驗（同一 LO，完整 OPC）：

| 合成案例 | 內容 | LO 頁數 | 我方 |
|---|---|---|---|
| `trail_br` | `BEFORE` ＋ 空段內 br-page（br 後無內容） | **1** | 1 |
| `trail_br_then_empty_para` | 同上但 br 後補一個空段 | **2** | 1 |
| `mid_br` | `BEFORE` ＋ br-page ＋ `AFTER` | 2 | 2 |
| `trail_br_x2` | 雙 br-page | 2 | 2 |
| `S1`–`S5`（sdt 包 br 各態） | 見 scratch `e1_abl3.py` | 與我方**逐案一致** | 同 |

判讀（逐條）：

1. **`w:sdt` 包裝不是原因**：W3/W4/S1–S5 證明我方對 sdt 內 br-page 的採集與 LO
   逐案全等（含 `docPartGallery="Cover Pages"` 態）。卡面「須查是 docProps 還是 SDT
   placeholder」在**頁數**這一席上是錯的方向。
2. **「尾端 br-page 是否該多開一頁」我方與 LO 一致，不是缺口**：合成 `trail_br`
   LO＝1 頁＝我方（LO 不為孤立 br 開頁）；`trail_br_then_empty_para` LO＝2 頁，
   我方＝1 頁——該態我方少一頁，但 LO 那頁同样是真空頁，且此態非本檔形態。
3. **CoverPage 的 p1 空白頁是 LO 側「錨定 wps 圖形＋尾端 br-page」的交互產物**：
   W2/W5 抽掉圖形 ⇒ LO 掉回 1 頁；br 後補段 ⇒ LO 3 頁；保留原樣 ⇒ LO 2 頁。
   我方發射路徑把錨定框當附掛項（不進流、不佔帶），因此該交互在我們的分頁模型裡
   根本不存在。要收斂這一頁，必須重現 LO 的「錨定框對其後 br-page 的分頁影響」——
   這是 LO 實態的**未文書化行為**，不是 ECMA-376 協定要求，且其觸發條件在
   `drawing.rs` 的 VML/AC 採集狀態機（R35 場，本卡嚴禁面）＋ paint/z 序（D9/D9b 場）。

### 3.3 CoverPage `Eric White` 多發／`Quire Corpus` 缺發 = dataBinding 未實作

- 源檔證據：9 個 `w:sdt`，其中 `Author`（`xpath=…/ns1:creator[1]`，
  `storeItemID={6C3C8BC8-…}`，佔位 run 文字 `Eric White`）、`Abstract`
  （`…/ns0:CoverPageProperties[1]/ns0:Abstract[1]`）、`Title`
  （`…/ns1:title[1]`，佔位 `The Title is here`）、`Subtitle`
  （`…/ns1:subject[1]`）。`docProps/core.xml` 的 `dc:creator`＝`Quire Corpus`。
- 協定根據：ECMA-376 §17.5.2.10 `dataBinding`——`w:sdtContent` 的內容是**佔位**，
  實際值取自 `w:xpath` 指向的外部資料源（此處＝文件屬性 part）。LO 照協定覆寫，
  我方原樣發射佔位 run。
- 修復需要：讀 `docProps/core.xml`（zip 內另一個 part）＋ xpath 求值＋在發射層以
  屬性值取代 `sdtContent` 文字。解析與 part 存取在 `core/ooxml/`（嚴禁面）；
  發射側需要 parser 先把 `dataBinding` 欄位透出，**兩層都要動**才可能綠。

## 4. 考卷與零回退

`core/engine-api/tests/e1_pagination.rs` 六盞燈（LO 真值座標／計數逐條對上 §2；
斷言全部用**等值或行數**，不用相似度閾值）：

- **燈 2（正向鎖，現行碼即綠）**：跨列表格切片後欄寬與合併不變形。合成表
  5 欄網格 80/1/1/80/200pt、首格 `grid_span=3`、30 列、`fragment(.., 20 帶, ..)`
  → 實得 **7 片**，逐片每格 `(x_pt, w_pt)` 與首片全等（跨列格 **82.0pt**＝前 3 欄和）。
  鑑別力證明：`E1_GRIDSPAN_EXPECT_BUGGY=1` 把期望跨列寬改成退化單欄 80.0 ⇒
  該燈必紅（實跑證實），證明斷言真的在比幾何、不是恆真。
- **燈 1a／1b／3a／3b／4（五盞欠債燈，標 `#[ignore]`）**：斷言值全部取自 LO 實測真值，
  但它們的主題落在本卡**准改面之外**（`core/ooxml/` 解析面＋engine 行高族表），
  按鐵律「根因若落在禁改面，停在 comment 回報，不硬改」，不得讓零回退閘門紅著。
  故採 PM 掛帳制（比照 `r34_placeholder_fill.rs` 燈 5 慣例）：`#[ignore = "..."]` 字串
  內寫明欠債原因與解鎖條件。實跑：

  ```text
  $ cargo test -p quire-engine --test e1_pagination
  test result: ok. 1 passed; 0 failed; 5 ignored
  $ cargo test -p quire-engine --test e1_pagination -- --ignored
  test result: FAILED. 0 passed; 5 failed; 0 ignored
  ```

  修復卡（§5 那三張）落地後須把對應燈解除 ignore 轉綠，才算收案。
- 本卡未動引擎碼 ⇒ 零回退鐵條（fdo76591 版面 ≥1.000／內容 ≥0.98、WC048 1.000/1.000、
  n751077 2 頁、99_Fields／btlr-textbox 1.000/1.000）按定義不受影響；
  `cargo test --workspace` 於本 worktree 實跑 **fail=0**（燈 2 綠、五盞欠債燈 ignore，
  不污染其他考卷），八閘由 PM 於收案時覆核。

## 5. 給 PM 的下一步建議（不在本卡範圍，未動手）

1. 開「autospacing 欄位＋採集」卡（`core/ooxml/src/lib.rs` 加
   `spacing_before_autospacing`／`after`，engine 側換算成帶數）——這是本檔
   cell-grid-span 少頁的**主因之一**，且是協定內行為（ECMA-376 §17.3.1.30/§17.3.1.15）。
   本卡已量出 LO 常數：**每個 autospacing 段落對加 14.0pt（隨字級縮放）**，見 §3.1 表。
2. 開「族表補 `Trebuchet MS`」卡：**不可套用 Noto Serif 1.3620**——本檔 LO 實測行距
   15.6／15.7pt（10pt 字級）⇒ LO 對該族的 real-LH ≈ **1.56–1.57**，與任何現存表值皆不同，
   須以 `render_lab`/LO 重測後入庫（`face_line_factor`，`core/engine-api/src/lib.rs:2592`）；
   需全庫 399 檔頁數 A/B 零回退佐證。
3. 開「`dataBinding` 內容控制覆寫」卡（`core/ooxml/` 透出欄位＋發射層消費）——
   這是 CoverPage 唯一**可證**的發射面缺陷（缺 `Quire Corpus`／多 `Eric White`）。
   落地後解 `lamp4_coverpage_emits_data_bound_tokens_and_no_placeholder_run` 的 ignore。
4. CoverPage 的 p1 空白頁建議**歸檔為 LO 實態缺口**（比照 K25 的度量口徑卡）：
   §3.2 消融證明它是 LO 側「錨定圖形 × 尾端 br-page」的未文書化交互，非協定要求，
   且觸發條件落在 R35（VML/AC 採集狀態機）與 D9/D9b（paint/z 序）兩個嚴禁面。
   燈 3a／3b 保留 `#[ignore]` 作追蹤，待該專卡解鎖。
5. 卡面（本卡 body）三條前提需由 PM 更正後才適合作為後續修復卡的規格輸入：
   (i) `cell-grid-span.docx` 無 `gridSpan`（計數 0）⇒ 該檔與跨列切片無關；
   (ii) CoverPage 的 `Quire`／`Corpus` 是 **LO 有渲染、我方缺發**，卡面寫反了；
   (iii) 兩席缺頁都不是表格/分頁容量問題，而是豎向流量少排（§3.1 實測 113.1pt）。

## 附：本卡產出的複驗腳本（scratch，不入庫）

`e1_probe{6..13}.py`、`e1_min{,2,3}.py`、`e1_abl{,2..9}.py`、`e1_const.py`、
`e1_autosp.py`、`e1_dump.py`、`e1_lh.py`（LO PDF 抽取／docx XML 結構探針／
LO-vs-我方消融矩陣／autospacing 常數矩陣）。
