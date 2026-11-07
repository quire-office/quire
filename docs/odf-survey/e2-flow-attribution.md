# ODF-E2：ODT 文字流偏移 ±20% 六席歸因（odt_flow_lab 實抓解剖）

任務卡：`ODF-E2（backlog）：ODT 文字流偏離 ±20% 六席歸因（odt_flow_lab 實抓）`（t_72dc8bc8，report-only，零 src 觸碰）

素材基線：卡面 `odt_flow_lab @ main 8f6238c`。本解剖同時跑了**卡面基線態**（`git worktree` 掛 8f6238c 編 odtflow）與**現行 main 態**（fc3230b）兩套我方投影，兩者差值單獨列帳（見第 2 節 D1，屬回歸而非原始缺陷）。LO 真值：`soffice --headless --convert-to pdf`（LO 24.8.0.3，`/mnt/c/Program Files/LibreOffice/program`，scratch 專屬 profile，收場只殺自己——render_lab 同口徑），pymupdf 1.28.2（`~/.hermes/tools/python-3.14.9*`）。

卡面六個 ratio 全部逐位複現成功：我方 `odt-render` 的 `total_text_len`（含空白 chars）除以 LO PDF `get_text()` 逐行 strip 後 join 的字數——`3082/2009=1.534`、`5759/4726=1.219`、`66/52=1.269`、`65/114=0.570`、`1092/1723=0.634`、`393/497=0.791`。下文另用**去空白口徑**（我方段落與 LO 行一律去空白字元）做內容級對帳，因為卡面口徑把「我方 `text:s`／`text:tab` 展開成空白」與「LO PDF 抽取把排版空白壓回單欄」混在同一帳裡；去空白後的差距才代表**內容**差異。

## 1. 歸因矩陣（結論先給）

去空白口徑。gap＝我方−LO；項定義見第 4 節；每席殘差以**字元多重集差**逐字元命名，全數歸零或可命名。

| 席 | 檔 | 卡面 ratio | gap(去空白) | 主因（具名項） | 判定 |
|---|---|---:|---:|---|---|
| S1 | `ext/lo_core_sw_qa_odfimport_ooo32780-1.odt` | 1.534 | +830 | 隱藏 section 段落照發 **840**；QX0.41 實體丟字 −7（去空白；raw 口徑 −11）；註腳引用標記未發 −3 | **我方多發**（隱藏段落），疊一筆**我方遺漏回歸**（實體） |
| S2 | `lo_out/lo__poi__test-data_document_form_footnotes.odt` | 1.219 | **−11** | 內容幾乎全對（4076 vs 4087）；缺項目符 PUA 淨 6（LO 發 `\uf094`×9＋`\uf0a8`×4，我方段落只帶 7 個 `\uf094`）＋ `-` 項目符 4＋註腳編號 `1` 1＝恰 11；卡面 ratio 由空白口徑撐起（我方空白 1683 vs LO 665） | **空白口徑偽影**（判 LO 未渲染偽影類之抽取偽影支），另掛項目符/註腳編號既有缺口 |
| S3 | `lo_out/lo__ooxmlexport__mce-nested.odt` | 1.269 | +13 | `svg:desc` 替代文字 `Light vertical` 混入段落流 **13 字**（LO 永不渲染 alt-text） | **我方多發**（svg:desc 洩漏） |
| S4 | `lo_out/lo__ooxmlexport__strict.odt` | 0.570 | −45 | styles.xml 頁首 `This is a header.` **14 字**未投影（C4 已記帳「頁首內容不做」）；嵌入物件 part 內文字面 **16 字**未投影（圖表 part 11：標題 `圖表標題` 4＋資料標籤 `1/2/3` 7；MathML `A=πr2` 5）；LO 合成軸小數刻度＋SmartArt 類別標籤 **15 字**（源檔全 zip 無此字面） | **我方遺漏**（頁首＋嵌入物件 part，30 字）疊 LO 合成 15 字 |
| S5 | `lo_out/lo__ooxmlexport__tdf124637_sectionMargin.odt` | 0.634 | −642 | LO 合成 TOC 點引導符 **628**（源檔宣告 `style:leader-text="."`，源檔全文僅 91 個 `.`，LO 發射 656）＋既有缺口 14：二級項目符 `o`×4＋一級 PUA `\uf0b7`×3（styles.xml `text:bullet-char="o"/"\uf0b7"`，WWNum1 層）、註腳引用 `1`×2（`ftn1` citation＋footnote 區）、頁尾文字 **5**（styles.xml footer 字面 `I` `II` `3` `2` `3`，我方不投頁尾＝D4） | **LO 端合成偽影為主**（628/642；餘 14 字全部掛既有缺口編號層／註腳編號／頁尾，見 §3.5） |
| S6 | `lo_out/lo__ooxmlimport__tdf87533_bidi.odt` | 0.791 | −109 | LO PDF 文字層控制字元 **106**（U+0002×84、U+0005×13、U+0006×9，阿拉伯文成形 kashida 代理，源檔 0 個）＋項目符 PUA 3；去垃圾後 LO 350 == 我方 350 **逐字元完全相等** | **LO 端合成偽影**（我方投影 100% 正確） |

三席「偏高」無一由註腳或表頭重複計入造成（卡面假設不成立，實測見 §3.2／§3.1）；三席「偏低」只有 S4 是真缺陷，S5/S6 是 LO 端合成內容，卡面對照尺把 LO 自己長出來的字當真值。

## 2. 缺陷規模（全語料 50 檔普查，非只六席）

| 代號 | 缺陷 | 方向 | 規模（實測） | 處置 |
|---|---|---|---|---|
| **D1** | quick-xml 0.37→0.41 遷移（QX 卡 1cc26b3）把一般實體（`&apos;` `&quot;` `&amp;` `&#NN;`）從 Text 事件改成 `Event::GeneralRef`，`core/odf` 兩支 walker（`layout_glue.rs::walk_paragraphs`、`body.rs::extract_body_from_bytes`）都沒接——**實測回歸**：同一檔在 8f6238c（0.37）vs fc3230b（0.41）A/B 對跑，**16/50 檔丟字合計 −605**（單檔最大 `poi/drawing` −364、`bug65649` −88、`bug59058` −27）；事件探針實錘：0.41 對 `didn&apos;t` 發 `Text("…didn") + GeneralRef(&apos;) + Text("t bode…")`，0.37 的 `Event` 列舉根本沒有 GeneralRef（`unescape()` 全包）。旁證：`core/ooxml/src/qx.rs` 與 `core/xlsx` 都有 GeneralRef 分支，唯 `core/odf` 漏 | 我方遺漏 | 16/50 檔、605 字 | 紅燈卡 ODF-E2a |
| **D2** | `text:section` 的 `text:display="none"` 與 `text:is-hidden="true"`（含 `text:display="condition"`）未跳過，整段照發；LO 真值實測不渲染（story 拷貝源檔 5 份、LO PDF 3 份、我方 5 份） | 我方多發 | 1/50 檔、840 字（去空白） | 紅燈卡 ODF-E2b |
| **D3** | 段落子樹內 `svg:desc`／`svg:title`（圖形替代文字）被walker 以「p_stack 非空就收文字」收進段落；事件序實測：`text:p`(開) → `draw:custom-shape` → `svg:desc` → Text `Light vertical` → `text:p`(空自閉合)——LO 端 alt-text 永不渲染 | 我方多發 | 13/50 檔、1055 字（上限值：含 LO 有渲染之 text-box 內 desc 者需逐檔判） | 紅燈卡 ODF-E2c |
| **D4** | styles.xml `master-page` 的 `style:header`／`style:footer` 段落內容未投影（`project()` 讀 styles.xml 只為字級級聯，不發頁首尾段；LO 每頁渲染）。ODF-C4 明文「頁首頁尾**內容**不做（寫進報告記帳）」，故這是已記帳缺口的落地卡 | 我方遺漏 | 19/50 檔、1406 字/單頁次（多頁檔按頁重複發射） | 卡 ODF-E2d（等 C4 合入後動） |
| **D5** | 嵌入物件 part `Object*/content.xml`（圖表、MathML、巢裝文件）未讀；LO 渲染其文字（圖表標題、MathML 符號、類別標籤） | 我方遺漏 | 5/50 檔、524 字 | 卡 ODF-E2e |
| （既有） | 列表項目符／編號字元（`text:bullet-char`，含 Wingdings PUA）與 `text:note-citation` 標記未發射 | 我方遺漏 | S2 缺 10＋1 字、S5 缺 3 字、S1 缺 3 字 | 編號層屬 K2 系列既有工作面；註腳編號 `oc/h2-footn-num`、`oc/endnote-emit` 分支在途，本卡只記帳不另開卡 |

## 3. 逐檔解剖

### 3.1 S1 `ext/lo_core_sw_qa_odfimport_ooo32780-1.odt`（1.534 → 判：我方多發＋實體回歸）

結構：6 個 `text:section`（content.xml offset 4984 起）：`Section2` 帶 `text:display="none"`（offset 5005）、`Section5` 帶 `text:is-hidden="true" text:display="condition" text:condition="ooow:x==1"`（offset 6148）。兩段內各有一份 524 字（raw）的故事拷貝「He heard quiet steps behind him…」。

逐段差異清單（去空白口徑）：

| 項 | 字數 | 定位 | 說明 |
|---|---:|---|---|
| Section2 段（我方多發） | 420 | `text:name="Section2"` 內兩個 `text:p` | LO PDF 無此文（hidden 段計數 0 發射） |
| Section5 段（我方多發） | 420 | `text:name="Section5"` 內兩個 `text:p` | 同上，`is-hidden="true"` 條件隱藏 |
| 段落內實體（我方遺漏，回歸） | 7 字（`'`×6＋`>`×1） | `didn&apos;t`（offset 4190，3 份可見拷貝各 2 處＋`who&apos;d`）、`-&gt;`（offset 5817） | D1；基線態（0.37）3082→現行（0.41）3071 恰差 11 字（含空白口徑） |
| 註腳引用標記（我方遺漏） | 3 字（`4` `v` `i`） | `text:note`／`note-citation`（羅馬 `vi`、阿拉伯 `4`） | LO 在段間渲染引用符；我方未發（K 系列在途） |

閉合（字元多重集）：H 840 −（LO-only 10：`'`×6 `>` `4` `v` `i`）＝ 830 == gap 實測值，殘差 0。
卡面假設「註腳重複計入」不成立：腳註正文我方只發一次（note-body 94 字，LO 同量），多發全部來自隱藏 section。

### 3.2 S2 `lo_out/lo__poi__test-data_document_form_footnotes.odt`（1.219 → 判：空白口徑偽影）

220 段全部 1:1 對上（表內格 198＋正文 12＋清單 10），卡面假設「表頭重複計入」不成立——XML 220 段 ↔ 我方 220 段 ↔ LO 行 119 行（LO 把相鄰格併行）。去空白後我方 4076 vs LO 4087，**只差 11 字**：

| 項 | 字數 | 定位 |
|---|---:|---|
| 項目符字元（styles.xml `text:bullet-char`：`\uf0a8`×2 `\uf094`×2 `-`×8，共 18 個 `list-level-style-bullet`，offset 48413 區） | 淨 10（PUA 6＋`-` 4） | LO 渲染 Wingdings PUA（`\uf094`×9 `\uf0a8`×4，我方段落已有 7 個 `\uf094` 從文字本體來）與 `-`×4；我方無編號層 |
| `note-citation` 數字 `1` | 1 | 同上既有缺口 |

字元多重集差恰＝LO 比我方多 PUA 6（`\uf094`×2＋`\uf0a8`×4）＋`-`×4＋`1`×1＝11 字，閉合殘差 0（我方發射無一 LO 沒有）。

卡面 ratio 1.219 的來源是空白：我方 raw 5759 內含空白 1683（`text:s` 展開 1507＋`text:tab` 13 等，源檔 `<text:s text:c=` offset 48235、`<text:tab/>` offset 67460），LO PDF 抽取端把排版空白壓成單空白（LO raw 4726 僅含空白 665）。這是**計量偽影**，非內容缺陷；寫入 README 口徑節。

### 3.3 S3 `lo_out/lo__ooxmlexport__mce-nested.odt`（1.269 → 判：我方多發）

唯一內容差 13 字：`Light vertical`。定位：content.xml offset 12535 `draw:name="Rectangle 366"`（0.359cm 細條 shape）內 `<svg:desc>Light vertical</svg:desc>`（offset 12694），緊接自閉合 `<text:p/>`。quick-xml 事件實測（evdump 探針）：該 `svg:desc` 的 Text 事件發生在**外層 `text:p` 仍開著**（p_depth=1）時，walker「p_stack 非空即收文字」照單全收；LO 端 alt-text 從不渲染。小樣本雜訊排除：`Sample Doc 002`（LO 有發、我方有發，非差）；styles.xml 頁首 `Sample Doc 002`（27 字）LO 未發（該頁 footer 未渲染 `[Type text]Page 0`），不計入本席。
閉合：+13（desc）== gap，殘差 0。全語料 D3 規模 13 檔 1055 字（最大 `bug59058` 459），一併由紅燈卡 ODF-E2c 處理。

### 3.4 S4 `lo_out/lo__ooxmlexport__strict.odt`（0.570 → 判：我方遺漏）

LO 發射 22 行逐行歸屬（去空白 LO 100 vs 我方 55，gap −45；我方-only 多重集為空，即我方無一字多發）。四分桶閉合（`e2_s4_buckets.py`，和恰 45）：

| 來源 | 未投影字數 | 實證定位 | 定性 |
|---|---:|---|---|
| styles.xml `style:header`「This is a header.」 | **14** | styles.xml `<style:header>`（master-page `Standard`） | 我方遺漏（D4；C4 已記帳不做內容） |
| `Object 2/content.xml` 圖表 part 內文字面：標題 `圖表標題` 4＋資料標籤 `1`×3 `2`×2 `3`×2 | **11** | offset 6132 `chart:title`；資料表 `chart:table` | 我方遺漏（D5） |
| `Object 3/content.xml` MathML `mi/mn`：`A` `=` `π` `r` `2` | **5** | offset 257 | 我方遺漏（D5） |
| LO 合成：Y 軸小數刻度 `0`×2 `5`×4 `.`×4 `2` `3`（`0 0.5 1 1.5 2 2.5 3 3.5` 中的小數與補零）＋ SmartArt 類別標籤 `a` `b` `c` | **15** | 全 zip（含 part）無此字面——`a` `b` `c` 出自 `draw:g`「Diagram 3」SmartArt（LO 自繪，XML 無逐字），刻度由 LO 依 chart 資料自動 scale | **LO 端合成**（非我方缺陷，§5 口徑） |

閉合：−14 −11 −5 −15 ＝ −45 == gap，殘差 0；`ow_only` 空集。
細節：圖表 part 內的 `;` 與 `序列1`（資料表列首欄）LO 端**不渲染**（未入 lo_only）——D5 修復時須按 LO 發射面取字，不得把整個 chart table 一股腦倒進段落流，否則本席反向超發。

### 3.5 S5 `lo_out/lo__ooxmlexport__tdf124637_sectionMargin.odt`（0.634 → 判：LO 端合成偽影為主）

gap −642 分解（`e2_s5_lines.py` 逐行對帳，殘差 0）：

| 項 | 字數 | 實證 |
|---|---:|---|
| TOC 點引導符 | **628** | 源檔宣告 `<style:tab-stop … style:leader-style="dotted" style:leader-text="."/>`；源檔全文 `.` 只有 91 個、LO PDF 發射 656 個。源檔 TOC 段實測文字是 `Test0\tII`（零點），點是 LO 排 TOC 時按 leader 生成——**我方不排 TOC leader，不該由我方字數買帳**。逐行實測：4 條 TOC 行各 LO-only 129 個 `.` |
| 二級項目符 `o`×4 ＋一級項目符 `\uf0b7`×3 | 7 | styles.xml `text:bullet-char="o"`／`"\uf0b7"`（WWNum1 巢裝 list，LO 於二級條目前發 `o`）——既有編號層缺口，非本檔特有 |
| 註腳引用標記 `1`×2 | 2 | content.xml `ftn1` 的 `note-citation`（內文引用＋頁腳註腳區行首 `1 `）——註腳編號既有缺口（`oc/*-footn-num` 在途） |
| 頁尾文字 `I`×3 `2`×1 `3`×1 | 5 | styles.xml 各 master-page `<style:footer>` 字面 `I` `II` `3` `2` `3`（源檔是**字面文字**，非 `text:page-number` 欄位；本檔 `text:page-number` 在 content 出現 0 次）——我方不投頁尾（D4） |

我方 TOC 段落本身有投影（`Test0\tII` 等 4 段含 tab 與頁碼），正文/註腳文字全數在帳。628 歸 LO 合成口徑（§5），其餘 14 字全掛既有缺口（編號層＋註腳編號＋頁尾），**本檔無新增缺陷種**。
**注意**：本檔另有 5 頁、Letter 尺寸（見 §4），但 gap 與頁幾何無關——字數軸不讀 LayoutParams。

### 3.6 S6 `lo_out/lo__ooxmlimport__tdf87533_bidi.odt`（0.791 → 判：LO 端合成偽影）

LO PDF 文字層含源檔完全沒有的控制字元：U+0002×84、U+0005×13、U+0006×9（阿拉伯文連字/kashida 位置代理，LO 寫 PDF 文字層時的產物）＋PUA 項目符 3。把這 109 個垃圾字元扣掉後：

```
LO 去垃圾去空白 350 == 我方去空白 350（字元多重集雙向皆空）
```

我方 6 段逐字全中，**本檔我方投影零缺陷**。卡面 0.791 完全由 pymupdf 抽出 LO 自己的控制字元撐出。寫入 README 口徑節：bidi/阿語檔的 LO 側計數必須先濾 `ord < 0x20` 與 PUA。

## 4. 頁幾何檢查（PM 11-07 硬性要求）

LO PDF 頁 rect vs 我方 `LayoutParams::default()`（假 A4 595×842pt）：

| 席 | LO rect（pt） | LO 頁數 | vs 我方 default | 是否頁尺寸/邊距錯位所致 |
|---|---|---:|---|---|
| S1 ooo32780-1 | 595.2×841.9 | 1 | 相符 | 否 |
| S2 form_footnotes | 612.0×792.0（Letter） | 3 | 錯位 | **幾何錯位存在，但非本席 gap 成因** |
| S3 mce-nested | 612.0×792.0 | 1 | 錯位 | 同上 |
| S4 strict | 612.0×792.0 | 1 | 錯位 | 同上 |
| S5 tdf124637 | 612.0×792.0 | 5 | 錯位 | 同上 |
| S6 tdf87533_bidi | 595.3×841.9 | 1 | 相符 | 否 |

判定依據（代碼級＋實測雙證）：卡面 ratio 只消費 `odt-render` 的 `total_text_len`（與 `paragraph_count`），該值在 `walk_paragraphs`（投影層）算出，**不讀 `LayoutParams`**——`page_break_positions(&texts, &params)` 才是唯一吃頁幾何的輸出口。實測 A/B：同一 Letter 檔餵 default A4，`total_text_len` 與逐段文字完全不變。故**六席 ratio 無一由頁幾何造成**，不得歸入 page_geometry 類的字數成因；頁幾何錯位（S2–S5 四席 Letter 頁）只污染 `page_breaks` 軸。

### 頁幾何修正後需重驗清單（ODF-C4 t_baff9a58 合入後執行）

1. S2（3 頁 Letter）、S5（5 頁 Letter）：重抓 `page_breaks` 對 LO PDF 頁數（我方斷頁 vs LO 實際分頁），字數 ratio 預期不動。
2. S3/S4（1 頁 Letter）：`page_breaks` 應仍為空/單頁，驗證 Letter 頁寬不再按 A4 折行造成假斷頁。
3. 六席 `total_text_len`/`paragraph_count` 必須與本報告逐位相同（D1–D5 修復前），作為 C4 零回歸對照。
4. D1 修好後全語料 odt_flow_lab 重跑：預期 16 檔 ratio 上移（最大 S1 +11 字級），median 仍應 1.000。

## 5. LO 端合成／抽取偽影口徑（同步寫入 README）

「LO 抽出字數」非中性真值，含兩類源沒有的字與一類抽取雜訊，比值對照前一律先扣：

1. **版面合成**：TOC 點引導符（`style:leader-text`）、頁碼/頁數欄位展開、圖表軸刻度（LO 按資料自動算 scale，源檔無 `0.5` 等字串）。
2. **編號層發射**：`text:bullet-char`（Wingdings PUA `U+F0A8/F094/F0B7`、`-`）、`note-citation` 標記——我方未實作編號層前必然短少。
3. **PDF 文字層雜訊**：bidi/阿語檔的 U+0002/0005/0006 控制字元；PUA 區段。計數前濾 `ord < 0x20` 與 `U+E000–U+F8FF`。
4. **空白不對稱**：我方 `text:s`（`text:c` 展開）／`text:tab` 是實字元，LO PDF 抽取把排版空白壓回單空白——內容級對帳一律去空白。

## 6. 紅燈卡（本卡不動 src，全部另開）

考卷口徑鐵律：**逐段文字斷言**（`assert_eq!(para_text, "含 &apos; 的完整段")`），不只計數。card ids 見 `created_cards`。

| 卡 | 缺陷 | 首紅斷言例（真值源＝本解剖） |
|---|---|---|
| ODF-E2a | D1 GeneralRef | `ooo32780-1` 第 2 段（Section1 story）含 `didn't`（U+0027）與 `-> `；`poi/drawing` 段級 `&quot;` 還原。A/B 對照表 16 檔全收 |
| ODF-E2b | D2 隱藏 section | `ooo32780-1` 投影**不得**含 Section2/Section5 拷貝（逐段文字斷言 absence）；可見三拷貝必須在 |
| ODF-E2c | D3 svg:desc | `mce-nested` 投影不得含 `Light vertical`（段級 absence）；`bug59058` desc 字串同判 |
| ODF-E2d | D4 頁首尾內容 | `strict` 投影段序含 `This is a header.`（styles.xml master-page，逐段文字）；等 C4 動 styles.xml 面 |
| ODF-E2e | D5 嵌入物件 part | `strict` 含 `圖表標題`＋MathML `A=π`；`SolarOverview`（368 字最大席）含物件段文字 |

## 7. 複現

解剖工具全部固化於本目錄 `e2-probes/`（report-only，不進 build）：`odtflow_main.rs`（我方逐段投影探針，依賴 `quire-odf` 公開 API）、`evdump_main.rs`（quick-xml 事件流探針，0.37/0.41 兩版共用）、`e2_matrix_final.py`（歸因矩陣＋多重集閉合）、`lo_pdf.py`（LO --convert-to pdf，自備 profile＋收場只殺自己）。LO 轉換一次成功（六檔 PDF 於 scratch `e2_odt/lo_pdf/`）。
