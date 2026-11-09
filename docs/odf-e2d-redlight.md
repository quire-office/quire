# ODF-E2d 紅燈紀錄（styles.xml master-page 頁首／頁尾段落內容投影）

缺陷（真值源 `docs/odf-survey/e2-flow-attribution.md` §2 D4／§3.4／§3.5，卡
t_b1c58929）：`project()` 讀 styles.xml 只為字級級聯，master-page 的
`style:header`／`style:footer` 段落內容一個不發，而 LO 每頁渲染。ODF-C4 明文
「頁首頁尾內容不做（寫進報告記帳）」——本卡把該記帳缺口落地。

考卷：`core/odf/tests/odf_e2d_header_footer.rs`（8 盞）＋
`shells/native/tests/odt_render_header_footer.rs`（CLI 面 3 盞）
獨立第二雙眼：`core/odf/tests/odf_e2d_hf_truth.py`（ElementTree 實作，生成
`core/odf/tests/fixtures/e2d_hf_truth.json`，紅燈 commit 同入，事後不得改）。
LO 真值：LO 24.8 `--convert-to pdf` ＋ pymupdf 逐頁文字（探針固化於
kanban workspace `t_b1c58929/probe/`，35 支）。

紅燈方式：燈 1／燈 2 先以「缺陷在時必然拍紅」的斷言入庫（strict 頁首段逐字
`This is a header.`、tdf124637 五頁頁尾字面 `I` `II` `3` `2` `3`），修復前
斷言拍紅（非編譯紅）。收案 run（本檔）另以 `ODF_E2D_BUGGY=1` 回塞態**實測再
紅一次**（§5），證明分段鏈與 per-page 指派真的在被檢查、不是寫死期望值。

## 一、口徑定案

### 1.1 宣告面（`hf_masters`／`collect_declaration`）

* **只掃 styles.xml**：全語料 `content.xml` 宣告 `office:master-styles`＝0 檔（實測）。
* 槽位六種：header／header-first／header-left × footer／footer-first／footer-left。
* 段落＝槽位內 `text:p`／`text:h`，**含巢裝表格／文字框內段落**（LO 同渲染——
  tdf102466 頁首表格三格、FDO76312 頁首表格實測有渲染）。容器段只取自身
  inline 文字，**巢裝段落另筆、依文件序**（見 §4.3 itertext 重計根因）。
* `svg:desc`／`svg:title` **零發射**（alt-text，LO 永不渲染＝E2 報告 D3 同律；
  實證 fdo78420：頁首 `svg:desc` 字面 `iveco`／`press release`，LO PDF 小寫
  `iveco` 0 次、`press release` 0 次——大寫命中全部來自內文）。
* 欄位節點取**快取值字面**（`text:page-number` 等節點內文字面照收；LO 重算＝
  README §11.1 合成軸，非我方目標）；元素 tail 文字必收（FDO76312 頁尾
  `Page 1 of 2`＝`Page `＋快取 `1`＋tail `of `＋快取 `2`——E2 探針正是漏在 tail）。
* 字級級聯（燈 7 對帳軸）：styles.xml `office:automatic-styles`（HF 段落樣式
  `MP1`/`MP3` 宣告在此層，C3 `StyleScope::StylesXml` 不收，故另立 HF 專用
  scope 而非改既有行為）→ `office:styles` 具名（parent 鏈）→
  default-style(paragraph) → default-style(text) → 12.0pt；只認 `pt`。

### 1.2 發射面（`***`）

逐頁指定生效主頁與槽位，宿主流 `paragraphs`／`flatten()`／`total_text_len`／
`page_breaks` **零變動**（E2e 同款獨立入帳紀律）。空文字段不發（LO 不渲染），
槽位存在性由宣告面 `slot_count` 保留。段序鐵律：同頁 Header* → Body* → Footer*。

## 二、§19.501「宣告即換頁」分段鏈——LO 真值定案

ODF 1.3 §19.501：段落樣式宣告 `@style:master-page-name` ⇒ **該段落處換頁**。
我方以此作主頁鏈分段依據：

* 分段單元＝頂層流程項（`text:p`／`text:h`／`table:table`；下鑽
  list／section，表格／框內段落**不重複計**，否則 tdf102466 會炸出假分段）。
* 段落 index **0** 的宣告不產生額外換頁（檔首本就在主頁起點——
  FDO76312／mce-nested／oasis-intro 皆此形，實測）。
* 連續重複宣告同一主頁不重複切（去重併段）。

LO 頁數對照實據（`--headless --convert-to pdf` 逐頁文字帶判頁）：

| 檔 | §19.501 觸發數 | 分段頁數 | LO PDF 實測 | 我方修復前 |
|---|---|---|---|---|
| tdf124637_sectionMargin | 4（四個 `text:h`） | **5** | **5 頁**，逐頁頁尾字面 `I` `II` `3` `2` `3` | 1 頁（燈 2 紅） |
| tdf102466 | 2 | 12 | 11 | — |
| HC031-Complicated-Document | 3 | 6 | 9 | — |
| FDO76312／mce-nested／oasis-intro | index 0 宣告 | 不加分頁 | 同 | — |

（tdf102466／HC031 我方分段數與 LO 實測頁數仍有行內換頁差，屬 C4 頁斷
A/B 殘留軸（行內換頁走後備字級/行距），非本卡缺陷；本卡分段鏈只對
「宣告即換頁」軸負責，燈 2 五頁逐頁頁尾字面已釘死該軸。）

## 三、`-first`／`-left` 槽位律——LO 真值定案

### 3.1 `-first`：宣告即採用，**即使內容為空**

該主頁段落（chunk）首頁若宣告 `-first` 槽即採用之；空宣告 ⇒ **零發射**，
**不退回 plain**。實據（LO PDF 逐頁文字帶）：

* FDO76312：p0 頂／底帶**零文字**（宣告 header-first＋footer-first 皆空），
  p1 才發 plain `Prep (2)…`／`Page 1 of 2`。
* fdo78420：p0 頁尾發 `-first` 快取 `1`（非 plain 快取 `12`；LO p1.. 渲染
  `2`..＝重算，我方發快取字面屬 README §11.1 合成軸，記帳）。
* mce-nested：p0 頁首零發射（正頁首 `Sample Doc 002` 在 PDF 的唯一命中在
  y=203＝**內文帶**，content.xml 本身有同一字面，不可當作「頁首有渲染」證據）。

記帳：語料 **0 檔**同時有「多主頁鏈＋`-first` 槽」⇒ per-chunk 首頁 vs
全文件首頁兩說不可分辨，採 per-chunk（LO 頁面樣式實例語意），留待真檔判例。

### 3.2 `-left`：0 起頁序的奇數頁（＝邏輯偶數頁）取 `-left`

**不以 `style:mirror` 宣告為前提**——全語料 mirror 宣告 0 檔（tdf123705 亦無）
而 LO 仍按偶數頁吃 `-left`。未宣告 `-left` 退 plain。實據 tdf123705：

* p0/p2 頁尾 plain `Rev 11/22/2017 Page 1 of 3`；
* p1/p3 頁尾 `-left` `11/22/2017 Page 2 of 2`；
* p1/p3 頁首 `-left` **空宣告 ⇒ 零發射**（正頁首 `Holland America Group…`
  不在偶數頁出現，LO PDF 實測）。

空 `-left` 壓 plain 之形計帳 `skipped_left_slots`（宣告了卻該頁未消費的槽），
全語料實測 **12**（tdf123705 8＋phpword reader／reader-styles 各 2）。

## 四、前 run 紅燈考卷的三處臆測值——實測更正（記帳）

1. **燈 6 宿主 `total_text_len` 基準 65569 ⇒ 593902**。65569 是 strict 附近
   量級的**單檔**數，非全庫和。定案雙線：CLI 全語料實測和＝593902（本 run
   `probe/final_e2d_metrics.py` 重跑覆核：48 明文檔、`paragraph_count` 和
   19935、`total_text_len` 和 593902）＋ `docs/odf-e2a-redlight.md` §合計表
   同值。燈 6 釘宿主兩值逐位不動。
2. **strict 頁 0 正文段切片 `[\"Hello world!\",\"\",\"\",\"\",\"\",\"This is math:\"]`
   ⇒ 宿主全 6 段**。前 run 草稿臆測頁 0 只切出 2 段；LO PDF 實測 strict 為
   **單頁** 612×792（Letter），頁首 `This is a header.` y=36.23、正文首段
   y=72.23——六段全在該頁。燈 1 段序斷言按「頁首段先於該頁全部正文段」修正。
3. **tdf102466 頁首文字 2× 差根因＝外層 `text:p` 用 `itertext()` 把巢裝表格
   段落重計**。舊探針 census 得 632 字（LO/E2 口徑 316 的恰 2 倍）；定案改為
   「容器段只取自身 inline、巢裝段另筆依文件序」後，基準檔與 E2 口徑逐檔差
   歸零（本 run `probe/reconcile_e2_vs_final_fixture.py` 實測：tdf102466 與
   HC031 不再出現在差表）。

## 五、回塞鑑別實測（收案 run 親跑）

`ODF_E2D_BUGGY=1 cargo test -p quire-odf --test odf_e2d_header_footer`
（回塞態＝「主頁鏈不分段，全文件只認文件級主頁」）：

```
test lamp2_tdf124637_master_chain_footers_and_order ... FAILED
test lamp8_buggy_single_master_is_distinguishable ... ok
（其餘 6 盞 ok；7 passed; 1 failed）

thread 'lamp2_...' panicked at core/odf/tests/odf_e2d_header_footer.rs:178:5:
assertion `left == right` failed: 燈2 §19.501 宣告即換頁 ⇒ 我方應分 5 頁
  left: 1
 right: 5
```

正常態（不設變數）同考卷 **8/8 綠**；CLI 面 `odt_render_header_footer`
**3/3 綠**。回塞態燈 1 形態仍綠（單主頁檔不受分段影響）、燈 8 斷言兩態形態
差——鑑別力成立。

## 六、全語料度量（收案 run 實測，`probe/final_e2d_metrics.py`）

宣告面（獨立第二雙眼基準檔 `e2d_hf_truth.json`）：50 檔／75 master／82 槽／
156 段；宣告面**去空白非空** 19 檔／1286 字（raw 非空 21 檔——lvlPicBulletId
與 table-row-data-displayed-twice 的頁尾是純 `\t` 字面，屬卡面「字面文字
兩類都要發」形，raw 口徑計入）。

發射面（CLI `odt-render` 五欄獨立帳，48 明文檔）：

| 欄 | 全庫和 |
|---|---|
| `hf_page_count` | 736 |
| `hf_paragraph_count` | 148 |
| `hf_text_len`（raw） | 4782 |
| `hf_chunks` | 70 |
| `hf_skipped_left_slots` | 12 |
| 有頁首尾文字的檔數 | 18 |

top：oasis-intro 1296（9 頁×頁首尾）、bug57031 1221（33 頁）、tdf123705 738
（7 頁，含 left 交替）、tdf104061 408、tdf102466 364、FDO78292 332。

### 6.1 與 E2 報告 D4 口徑（19/50 檔、1406 字）的差對帳

E2 舊 regex 口徑全語料重放＝19 檔／1417 字（去空白）。與本卡基準檔逐檔差
（本 run `probe/reconcile_e2_vs_final_fixture.py`）僅三筆，根因皆有實據：

| 檔 | E2 − 本卡 | 根因 |
|---|---|---|
| FDO78292 | +72 | 頁尾 `svg:desc` alt-text（LO 永不渲染，§1.1 零發射律） |
| fdo78420 | +50 | 頁首 `svg:desc` alt-text（LO PDF 小寫 0 次鐵證，§1.1） |
| tdf123705 | +9 | XML 實體口徑：E2 regex 留 `&amp;`/`&apos;` 原樣 5/6 字，ElementTree 解碼為 1 字 |

即：本卡比 E2 記帳口徑**更貼 LO**（alt-text 零發射＋實體解碼），1406 的字數
差不是漏發。

### 6.2 宣告了卻零發射的檔（3 檔）——分頁缺口傳導，記帳非本卡缺陷

FDO76312／fdo69616／mce-nested 宣告面非空但 `hf_text_len=0`：三檔首頁皆
宣告空 `-first` 槽（LO 實測首頁頂底零文字），而我方版面把整檔切成 **1 頁**
（LO 2 頁）⇒ plain 槽永遠沒有第二頁可落。根因＝我方分頁數 < LO（C4 頁斷
A/B 殘留軸：行內換頁走後備字級/行距），非頁首尾投影缺陷；發射面規則本身
由 FDO76312 的 LO 真值（p0 空／p1 plain）釘死，我方分頁收斂後自動轉正。

## 七、殘留（記帳，另卡或 C4 續軸）

* 分頁數缺口傳導（§6.2）：C4 A/B 軸（行內換頁後備字級/行距、多 master-page
  換頁鏈）收斂後覆核 hf 三口徑檔集對齊（`probe/caliber_sets.py` 可重放）。
* fdo78420 頁尾 `-first` 快取 `1` vs LO 重算序列（README §11.1 合成軸，鐵律
  不追）。
* per-chunk `-first` vs 全文件首頁兩說待真檔判例（語料 0 檔可分辨）。
* `style: 軸`／`print-orientation` 歷史分歧＝C4 已記帳項，本卡未觸。

## 八、閘門與署名

* 本卡新增燈：quire-odf 8 盞＋CLI 面 3 盞＝11 盞（先紅後綠＋§5 回塞再紅）。
* 八閘由 PM 值守於本 worktree（tip d934820）親跑全綠 fail=0（見卡面 PM 補正
  comment）；本收案 run 未改實作碼（只增本檔＋探針），依 PM 指示不重跑八閘。
  本 run 親跑：燈 8 盞綠、CLI 3 盞綠、BUGGY 回塞紅（§5 實測輸出）。
* 宿主零侵入實測：全庫 `paragraph_count`=19935、`total_text_len`=593902
  逐位不動（§六度量與燈 6 雙鎖）。
* 探針固化：kanban workspace `t_b1c58929/probe/`（LO PDF 逐頁真值、
  mirror/`-left` 查證、leaf-census、口徑對帳四組＋本 run 四支）。
* 署名 ADT109119；簡繁掃描（traditional-chinese-purity 無歧義簡體字集版）
  對新增檔＋改動檔執行。
