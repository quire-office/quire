# ODF-C4 解剖報告：ODT 頁面幾何鏈（style:page-layout＋master-page）

實查日期：2026-11-08。工具鏈：corpus-odt 50 檔直拆（zipfile＋ElementTree）、
LO 24.8（`/mnt/c/Program Files/LibreOffice/program/soffice.exe`
`--convert-to pdf`）＋ pymupdf 1.28.2 `page.rect` 真值、OASIS OpenDocument
v1.3/v1.4 Part 3 schema 原文（`docs.oasis-open.org`，v1.3 Date.Issued
2021-04-27）。全部數值為實測，非印象值。

## §1 語料普查（corpus-odt 50 檔逐檔）

| 指標 | 實測值 |
|---|---|
| 有 `<style:page-layout>` 的檔數 | **48/48 明文檔全有**（加密 2 檔消費層必報錯，ODF-E1 口徑） |
| page-layout 總數 | 70 個（單檔最多 23 個：`lo__poi__test-data_document_bug65649.odt`） |
| **雙軸分歧**（style: 軸與 fo: 軸同檔並存且數值不同）的檔數 | **0** |
| **style: 軸**（`style:paper-width/height`）使用檔數 | **0**（全語料 0 次出現） |
| **fo: 軸**（`fo:page-width/height`）使用 | **70/70 page-layout 全用** |
| 用 `<style:page-margin>` 子元素的檔數 | **0**（全語料 0 次出現；邊距一律 `fo:margin-*` 掛 `page-layout-properties`） |
| 完全無 page-layout（該走 LO 預設頁）的檔數 | **0** |
| 多 master-page 檔數 | 6 檔（最多 14 個：`bug65649`；本切片只取文件級預設，其餘 `unparsed_master_pages` 記帳） |
| `@style:print-orientation` | 只寫 `portrait`/`landscape`（`fo:orientation` 全語料 0 次——LO 寫端不寫 fo: 方向軸） |
| 首段/首表格 `@style:master-page-name` 生效路徑 | 5 檔走 `para_master_page_name`，43 檔走 `default_master_styles`（無任何檔回退 LO 預設頁） |

頁尺寸分佈（換算 pt，=LO PDF `page.rect` 逐檔 ±0.6pt 內全對齊）：
A4（595.3039×841.8898）23 檔、Letter（612×792）20 檔、其餘 5 檔各有獨立宣告
（含 1 檔橫向 A4 841.89×595.30、1 檔 612×936）。

### 卡面前設的修正（實證推翻，照樣實施）
卡面寫「LO 寫檔兩軸都寫（`style:paper-width/height`＋`fo:page-width/height`）」。
**實查不成立**：
1. ODF 1.3 Part 3 schema 原文 `page-layout-properties` 屬性表只有
   `fo:page-width`/`fo:page-height`；`style:paper-width` **0 次出現**
   （`style:width` 17 次全在 separator/section 欄位脈絡，非頁面軸）；
   `style:page-margin` 元素在 v1.3/v1.4 schema 均 **0 次出現**。
2. corpus-odt 50 檔（LO/Word/Poi/PHPWord/xopt 來源混雜）`style:` 軸 0 檔、
   `<style:page-margin>` 0 檔。
3. `odt:writer13`（ODF 1.3 Extended）匯出探針在 LO 24.8 不產出
   （`rc=0 out=False`，兩種 outdir 配置皆試），無法從寫端逼出 style: 軸。

處置：卡面「style: 軸優先、fo: 軸後備」的**讀取政策照裝**（對非 LO 生產者
與未來 ODFx 檔正確），以合成 fixture `c4_dual_axis.odt` 驗證優先序；真實
LO 檔永遠走 fo: 後備路徑，燈 6 全語料對齊證明無副作用。

## §2 sample3_page_sect.odt 完整鏈實拆

```
content.xml：頂層首段 <text:p>（無 @text:style-name）→ 無段落級 master 宣告
styles.xml：<office:automatic-styles>
  <style:page-layout style:name="Mpm1">
    <style:page-layout-properties fo:page-width="21.00cm" fo:page-height="29.70cm"
        fo:margin-top="2.00cm" fo:margin-bottom="2.00cm"
        fo:margin-left="2.00cm" fo:margin-right="2.00cm"/>
<office:master-styles>
  <style:master-page style:name="Standard" style:page-layout-name="Mpm1"/>
```
鏈：首段無宣告 → `Standard`（`<office:master-styles>` 首個）→ `Mpm1` →
**595.3039 × 841.8898 pt，四邊 56.6929 pt**。
LO PDF 真值（pymupdf `page.rect`）：**595.304 × 841.890**（Δ<0.01pt）。
舊值 `LayoutParams::default()`＝595×842＋72pt 邊距——頁寬差 0.30pt、邊距差
15.3pt，燈 1 鑑別力成立。

## §3 雙軸探針 LO 真值（合成 fixture）

`c4_dual_axis.odt`：`style:paper-width/height=21×29cm`、
`fo:page-width/height=20×28cm`（故意差 1cm）。
- 我方（卡面政策，style: 軸優先）：595.276 × 822.047。
- LO PDF 實渲：**566.957 × 793.701（= fo: 軸 20×28cm）**——LO 讀端自己吃
  fo: 軸。口徑差僅存在於合成檔（真實語料 0 檔雙軸），如實記帳，不影響燈 6。

## §4 單位換算映射表（ODF → pt）

| ODF 單位 | 換算 | 語料實態 |
|---|---|---|
| `cm` | ×28.3464566929 | 16851 次（主流，`D.DDDcm` 為主寫法） |
| `mm` | ×2.83464566929 | 0 次（支援備未來） |
| `in` | ×72 | 39 次（`8.5in`/`11in`/`0.7875in` 級小數） |
| `pt` `pc` | ×1 / ×12 | 0 次（支援備未來） |
| 負值（`-2.0cm`） | 不入列（None→後備鏈） | 38 次（全在 margin 以外的屬性；頁面軸 0 次） |
| 本地化逗號 `2,0cm` | 容錯轉 `2.0cm`（僅逗號兩側皆數字） | 語料 0 次（防非 LO 生產者） |
| 百分比/無單位 | 不做單位猜測→None | — |

`LayoutParams.margins_pt` 欄序 `[上,右,下,左]`；ODF `fo:margin-*` 四邊直映射。
`SectionSpec`（`core/ooxml/src/sectpr.rs`）對照：pgSz w/h↔page-width/height、
四邊 mar↔fo:margin-*——ODF 無 gutter/mirror 軸，本卡不造欄位。

## §5 LO 預設頁實測（回退值，非印象值）

- 探針 `c4_no_page_layout.odt`（master-page 指向不存在版面名、automatic-styles
  域無任何 page-layout）經 LO：頁 **595.304 × 841.890**、文字左緣
  x0=**56.80pt**、上緣 y0=47.92pt（首行行距補償）。
- ⇒ LO 預設邊距 = **2.0cm（56.6929pt）**，排除 `LayoutParams::default()` 的
  1 吋（72pt）；LO A4 常數以 1/100mm 整數存放（21001×29700），頁寬
  595.3031pt ≠ default 的 595.0。
- 定案常數：`LO_DEFAULT_PAGE_W_PT=21.001cm`、`LO_DEFAULT_PAGE_H_PT=29.7cm`、
  `LO_DEFAULT_MARGIN_{CM,PT}=2.0cm`。

## §6 WordTest_edit 教訓（第二雙眼的價值）

`ext/lo_core_sw_qa_odfexport_WordTest_edit.odt`：首流程物件包在
`text:list>text:list-item` 內、首段 `P1` 宣告 `@style:master-page-name="MP0"`
→ 正確值 `para_master_page_name`/`PL0`（612×792）。Python 基準腳本初版因
`local()` 去前綴後仍用 `"office:automatic-styles"` 全名比對而漏採 content 域
宣告，Rust 燈 6 拍紅抓出——**兩邊獨立實作互驗**為本卡資料可信度的來源
（修正後 48 檔 source 欄位全對齊）。

## §7 記帳（本切片明確不做）

1. 頁首頁尾**內容**渲染（只採 `has_header`/`has_footer` bool）。
2. `style:print-orientation` 與 `fo:orientation` 歷史分歧全解（只採集
   `print_orientation`/`fo_orientation` 兩欄，不據以對調 w/h——語料 0 檔 fo:
   方向軸，無實需）。
3. 多 master-page 換頁鏈（橫向章節）：6 檔有副作用 master-page，本切片只取
   文件級預設 1 個，計數入 `unparsed_master_pages`。
4. `text:section` 欄內幾何（正交機制，另卡）。
5. 註腳頁配置（`style:footnote-layout`）。
6. style: 軸真實檔驗證——LO 24.8 寫不出該軸（§1），僅合成探針覆蓋；待有
   真實 ODFx 語料再補燈。

## §8 驗證工件

- 計數基準：`core/odf/tests/odf_c4_truth_baseline.py`（獨立第二雙眼；
  50 檔＋3 樣本對 LO PDF 偏差>0.6pt 者 **0 檔**）。
- LO 真值：`c4_lo_pdf/index.json`＋pymupdf 量測（51/53 轉換，2  misses＝
  加密檔）。
- 考卷：`core/odf/tests/odf_c4_page_geometry.rs`（6 盞）＋
  `shells/native/tests/odt_render_page_geometry.rs`（CLI 面 2 盞）。
- 紅燈紀錄：`docs/odf-c4-redlight.md`。
