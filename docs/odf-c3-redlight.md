# ODF-C3 紅燈紀錄（ODT 文字流投影 → layout 首切片）

考卷：`core/odf/tests/odt_layout_projection.rs`
計數基準腳本：`core/odf/tests/odt_projection_baseline.py`（獨立於 Rust 的第二雙眼，
口徑與 `scripts/odf_survey.py`／`docs/odf-survey/survey.json` 的正則計數一致：
`<text:p[\s>/]`／`<text:h[\s>/]`，Start＋Empty 皆計，巢裝一律入列）

紅燈方式：`core/odf/src/layout_glue.rs` 先以**佔位實作**入庫（`project*` 回傳空
投影、`layout_projection` 回傳空清單、`page_break_positions` 回傳空清單），
考卷因此**斷言拍紅**（不是編譯紅）——證明考卷真的有在檢查東西。

## 一、紅燈實測輸出（`cargo test -p quire-odf --test odt_layout_projection`）

```
test project_bytes_falls_back_to_12pt ... FAILED
test b1_font_size_cascade_tdf152710 ... FAILED
test b2_font_size_cascade_oasis_intro ... FAILED
test c_headings_enter_projection ... FAILED
test layout_integration_smoke ... FAILED
test a_projection_over_corpus_50 ... FAILED

thread 'project_bytes_falls_back_to_12pt' panicked at core/odf/tests/odt_layout_projection.rs:387:5:
assertion `left == right` failed
  left: 0
 right: 1

thread 'b1_font_size_cascade_tdf152710' panicked at core/odf/tests/odt_layout_projection.rs:221:5:
ext/lo_core_sw_qa_odfexport_tdf152710.odt：投影段落數應 ≥3（實測 0）

thread 'b2_font_size_cascade_oasis_intro' panicked at core/odf/tests/odt_layout_projection.rs:270:5:
ext/oasis_opendocument_v1p3_os_part1_introduction.odt：投影段落數應 ≥2（實測 0）

thread 'c_headings_enter_projection' panicked at core/odf/tests/odt_layout_projection.rs:309:5:
assertion `left == right` failed: ext/oasis_opendocument_v1p3_os_part1_introduction.odt text:p 基準
  left: 0
 right: 177

thread 'layout_integration_smoke' panicked at core/odf/tests/odt_layout_projection.rs:353:9:
ext/lo_core_sw_qa_odfexport_tdf152710.odt：有文字的檔案斷行輸出不得為空

thread 'a_projection_over_corpus_50' panicked at core/odf/tests/odt_layout_projection.rs:177:9:
assertion `left == right` failed: ext/lo_core_sw_qa_odfexport_WordTest_edit.odt：text:p 計數應與 XML 原始計數一致
  left: 0
 right: 13

test result: FAILED. 0 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out
```

## 二、(a) 50 檔段落計數基準（基準腳本產出，紅燈 commit 同入）

基準腳本實測：`total_text_p = 19687`、`total_text_h = 248`、**段落總數 19935**。
逐檔值已固化在考卷的 `EXPECTED_COUNTS`（50 筆，`text_p`＋`text_h`），並與
`docs/odf-survey/survey.json` 的 `content.text_p`／`content.text_h` **逐檔交叉
核對一致**（零不一致）。C2 考卷 4 鎖的是 `text_p` 總數 19687；本卡加的是
`text_h` 248 條——兩者相加即 19935，是「heading 也是一段」的直接證據。

含 `<text:h` 的語料檔共 24 檔（基準腳本 `files_with_headings`）：tdf152710、
oasis v1.3 part1、FDO76312、FDO78292、lvlPicBulletId、tdf102466（25 條）、
tdf104061（47 條）、tdf124637、phpword×3、poi×4、selfgen×2、xopt×7。

mimetype 違例三檔（比照 C2 考卷 3 的 `MimetypeStatus` 明確歸類，非
catch-continue）：`ext/lo_core_sw_qa_odfexport_tdf140437.odt`、
`ext/lo_core_sw_qa_odfexport_tdf152710.odt`、`ext/lo_core_sw_qa_odfexport_tdf77961.odt`
——三者皆「非首條目」違例，其餘 47 檔必須 `Compliant`。

加密條目兩檔（`ext/lo_core_sw_qa_odfexport_sha1_{correct,wrong}.odt`）：
manifest 宣告加密 → content.xml 解出的是亂碼（非健全 XML），依 C2 既有口徑
回零段但仍 `Ok`（與 survey.json `content.text_p = 0` 一緻），不計紅也不吞錯。

## 三、(b) 樣式級聯硬斷言（單段寫死，非命中率）

候選篩選：`grep -l 'fo:font-size="[^"]*pt"'` 對 .odt（zip 壓縮體）必然落空，
改用基準腳本對**解壓後** content.xml／styles.xml 掃 `fo:font-size="..pt"`，
得 47 檔含顯式 pt 宣告；從中取**非 fdo 轉出**的真實世界檔兩檔。

### 檔案 1：`ext/lo_core_sw_qa_odfexport_tdf152710.odt`（LibreOffice/core 上游真實檔）

XML 實際宣告（PM 開檔可逐字核對）：

| 樣式 | 出處 | 宣告 |
|---|---|---|
| `P5` | content.xml automatic-styles（`style:parent-style-name="Heading_20_1"`） | `fo:font-size="10pt"` |
| `Heading_20_1` | styles.xml（P5 的 parent） | `fo:font-size="115%"`、`fo:font-weight="bold"` |
| `P3` | content.xml automatic-styles（parent=`Standard`） | `fo:font-size="10pt"` |
| `Standard` | styles.xml 具名 | 無 `fo:font-size` |
| default-style(family=paragraph) | styles.xml | `fo:font-size="12pt"` |

段落文件序（text:p＋text:h 混排，0 起算）：`[0] text:p style=P1`、
`[1] text:h style=P5`（文字 `Laisvos programos vaikų lavinimui`）、
`[2] text:p style=P3`。

硬斷言：
- 第 **1** 段（heading，style=P5）字級 **== 10.0pt**——自動樣式自己的
  `fo:font-size="10pt"` 取勝於 parent 鏈的 `115%`（百分比不認，卡面口徑），
  且粗體 **== true**（自動樣式未宣告字重 → 沿 parent `Heading_20_1` 的
  `fo:font-weight="bold"` 取得）。
- 第 **2** 段（body，style=P3）字級 **== 10.0pt**、粗體 **== false**。

### 檔案 2：`ext/oasis_opendocument_v1p3_os_part1_introduction.odt`（OASIS 上游標準文件）

XML 實際宣告：

| 樣式 | 出處 | 宣告 |
|---|---|---|
| `Title` | styles.xml 具名（parent=`Normal`） | `fo:font-size="24pt"`、`fo:font-weight="bold"` |
| `P15` | content.xml automatic-styles（parent=`Normal`） | 無 `fo:font-size`（僅 `style:master-page-name="MP0"`） |
| `Normal` | styles.xml | 無 `fo:font-size`（僅 `style:font-size-complex="12pt"`——非 `fo:font-size`，不認） |
| default-style(family=paragraph) | styles.xml | `fo:font-size="10pt"`、`fo:font-weight="normal"` |

段落文件序：`[0] text:p style=P15`、`[1] text:p style=Title`（文字
`Open Document Format for Office Applications …`）。

硬斷言：
- 第 **0** 段（P15 三層皆無 `fo:font-size`）字級 **== 10.0pt**——級聯必須落到
  `styles.xml` default-style 的 `10pt`，**不得**錯填 `12.0pt` 後備值（這條把
  「default-style 層真的有在走」釘死，與 `12.0` fallback 可區分）。
- 第 **1** 段（style=Title）字級 **== 24.0pt**、粗體 **== true**。

## 四、(c) text:h 入列斷言

檔案：`ext/oasis_opendocument_v1p3_os_part1_introduction.odt`（`text:p`=177、
`text:h`=7）。斷言投影段落數 **嚴格大於** 僅 text:p 計數、且
`paragraph_count == text_p_count + text_h_count`（heading 入列成段，不另立
計數欄），並斷言 heading 文字 `Normative References` 出現在投影輸出。

## 五、附考卷（對接煙霧測試）

- `layout_integration_smoke`：三檔（tdf152710／oasis part1／xopt Mercury）走
  `layout_projection` → 斷行結果逐段一筆、有文字檔行數 > 0；
  `page_break_positions` 回傳單調遞增且嚴格小於段落總數。
- `project_bytes_falls_back_to_12pt`：合成 content.xml（無任何樣式宣告、
  `text:h`＋`text:p`＋自閉合 `<text:p/>`）→ 計數 1/2、段落 3 全入列、零文字段
  成形，字級一律 `DEFAULT_FONT_SIZE_PT`（12.0）、粗體一律 normal；降維輸出
  `(Vec<String>, Vec<f32>)` 形狀如實。
- `run_level_cascade_splits_spans`（綠燈後追加）：合成檔驗證 **run 級**級聯粒度
  ——段內 `text:span` 換字級／宣告 `font-weight="normal"` 必須拆 run 並如實
  `((0,2),20.0,true) ((2,4),8.0,true) ((4,6),20.0,false) ((6,7),20.0,true)`；
  同時鎖住「span 未宣告屬性沿承**段落級聯有效值**（20pt），而非檔案
  default-style（11pt）」與「顯式宣告 normal ≠ 未宣告（三態級聯）」兩個口徑。

## 六、記帳缺口（非偷懶，卡面已載明）

- 本卡 `shells/native` 的 `odt-render` 用 `LayoutParams::default()`（A4），
  **不解析** ODF `style:page-layout`（page-width/height/margin 一律 A4 後備）。
- layout 斷行不認字重：粗體只存進 `OdfLayoutParagraph::runs` 供渲染層未來消費。
- 逐段字級目前只影響**斷行量測尺**（走 `layout_paragraphs_grid`），分頁容量
  仍按全域 `params` 的行帶尺——與 DOCX 線既有 `ParaGrid` 口徑一致。
