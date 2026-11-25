# save-roundtrip-lab 報告（儲存保真閘門）

- 生成時間：2026-11-26T00:47:24
- LibreOffice：`/home/jerry/.local/lo-linux/opt/libreoffice25.8/program/soffice`（本機 Linux LO；Windows soffice.exe 不在此機，PATH 注入口徑自動偵測）
- quire-cli：`/home/jerry/projects/quire/.worktrees/t_d96c31db/target/debug/quire-cli`｜git：`960c1c5`
- 對象：corpus manifest 分層抽樣 42 檔（42 檔，seed=20261125）
- SAVE_RT_BUGGY：False｜--discriminate：False｜像素容差：0
- 紅燈檔案：4；LO 轉換失敗：0

## 考卷自身效能基線（PM 裁決 3：`paragraph_cp_len` 效能倒退修復實錄）

本輪考卷總時數：1116.0 s（42 檔）。本閘門自身最慢檔（逐檔實測，供 nightly 預算參考）：

| 檔案 | 全臂耗時 (s) |
|---|---|
| poi/test-data_document_bug65649.docx | 768.9 |
| poi/test-data_document_drawing.docx | 48.1 |
| ooxmlexport/tdf102466.docx | 32.5 |
| xopt/TestFiles_RP_RP051-Arabic.docx | 27.2 |
| poi/test-data_document_form_footnotes.docx | 23.5 |

修復前（run 86 基線，逐段呼叫 `Engine::paragraph_cp_len`，每次從頭重掃 `document.xml`＝O(段數 × 檔大小)）：`corpus/poi/test-data_document_bug65649.docx`（`document.xml` 12.5 MB／15,934 段）跑 `quire-cli paragraphs` **>420 s 逾時被殺**（考卷卡死在此檔）。

修復後（單趟掃描器 `scan_paragraph_spans`＋批次出口 `Engine::paragraph_cp_lens`）：

| 項目 | 耗時 | 峰值記憶體 | 備註 |
|---|---|---|---|
| `quire-cli paragraphs`（bug65649） | **27.56 s** | 195,552 KB | 輸出 2,446,372 B；**修復前 >420 s 逾時** |
| `quire-cli pagebreaks`（bug65649） | 23.5 s | — | 同檔分頁斷點 |
| 零編輯存檔（bug65649） | 20.9 s | — | `byte_identical=true` |
| LO `lo_to_pdf`（bug65649） | 9.4 s | — | 180 頁 |
| `pdf_page_facts`（bug65649） | 0.8 s | — | 180 頁文字抽取 |

根因與修法：舊 `paragraph_cp_len` 對每段呼叫 `find_paragraph_span`，該函式每次從 `document.xml` 起點重新掃描 ⇒ 段數越多、檔越大，平方級惡化。抽出單趟掃描器`scan_paragraph_spans(xml, stop_at)`（`Some(k)` 命中即停，保留舊逐段語義；`None` 一趟收全部段）＋批次出口 `Engine::paragraph_cp_lens()`，把 N 次全文重掃收斂為 1 次；每段 `analyze_paragraph` 仍只掃自身區間，`total_cp` 語義逐字同值。


## 臂 A：零編輯（位元組恆等預閘＋像素/文字差分）

| 檔案 | 層 | 位元組恆等 | 像素層 | 判定 |
|---|---|---|---|---|
| poi/test-data_document_table-alignment.docx | table | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| selfgen/patho_table_cell_lists.docx | table,cjk | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/fdo79540.docx | table,mixed_size,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| poi/test-data_document_bug65649.docx | table,merged,table_across,footnote,multisection,mixed_size,cjk | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/floatingtbl_with_formula.docx | table | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/tdf102466.docx | table,merged,image,multisection,mixed_size,header_footer,fields | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| xopt/TestFiles_HC006-Test-01.docx | table,nested_table,mixed_size | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlimport/tdf169173.docx | table,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| xopt/TestFiles_RP_RP051-Arabic.docx | merged,image,mixed_size,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-05.docx | table,merged | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/tdf127814.docx | table,merged,multisection,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| poi/test-data_document_drawing.docx | table,merged,nested_table,image,mixed_size | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/n779642.docx | table,nested_table,image | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/tdf123705.docx | table,merged,table_across,mixed_size,header_footer,fields | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/dkvert.docx | image | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| poi/test-data_document_testComment.docx | image | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| poi/test-data_document_form_footnotes.docx | table,merged,image,footnote,mixed_size,fields | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/fdo76098.docx | table,merged,image,multisection,mixed_size,columns,fields | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| phpword/tests_PhpWordTests__files_documents_reader.docx | table,merged,image,endnote,footnote,multisection,columns,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| selfgen/arabic_rtl_notes.docx | endnote,footnote | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| phpword/samples_resources_Sample_11_ReadWord2007.docx | table,merged,image,endnote,footnote,multisection,columns,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| poi/test-data_document_table_footnotes.docx | table,footnote | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| poi/test-data_document_Bug54849.docx | table,endnote,footnote,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/n830205.docx | table,multisection,mixed_size,columns,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx | multisection,columns | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/fdo78882.docx | multisection | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/090716_Studentische_Arbeit_VWS.docx | image,footnote,multisection,mixed_size,header_footer,fields | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlimport/tdf60351.docx | image,mixed_size | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/tdf117297_tableStyle.docx | table,mixed_size | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| poi/test-data_document_WordWithAttachments.docx | table,image,mixed_size,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| selfgen/patho_nested_table_lists.docx | table,nested_table,cjk | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx | image,cjk,fields | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx | image,cjk,fields | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/fdo78883.docx | table,merged,multisection,mixed_size,columns | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/2col-header.docx | columns,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx | table,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/fdo74605.docx | multisection,mixed_size,header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlexport/fdo66929.docx | header_footer | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| ooxmlimport/tdf154695-ToC_no_numbers.docx | fields | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx | image,endnote,cjk,fields | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| selfgen/embedded_font_odttf.docx | cjk | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |
| xopt/OpenXmlPowerToolsExamples_ChartUpdater01_Chart-Cached-Data-05.docx | random | ✓ | skipped（位元組恆等；跳過像素層但照記錄） | ✓ PASS |

## 臂 B：編輯態（突變區前逐像素零位移＋突變可見）

| 檔案 | mutation | 目標段 | 區起(法) | 頁 原→存 | 區前非零頁 | 可見 | 判定 | 原因 |
|---|---|---|---|---|---|---|---|---|
| poi/test-data_document_table-alignment.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table-alignment.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table-alignment.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table-alignment.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table-alignment.docx | cell | [0] | 0(no-target) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/patho_table_cell_lists.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/patho_table_cell_lists.docx | enter | [1] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/patho_table_cell_lists.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| selfgen/patho_table_cell_lists.docx | align | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/patho_table_cell_lists.docx | cell | [5] | 0(no-target) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo79540.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo79540.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo79540.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo79540.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo79540.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_bug65649.docx | type | [0] | 0(lo-visible) | 180→180 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_bug65649.docx | enter | [0] | 0(lo-visible) | 180→180 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_bug65649.docx | bold | [0] | 0(lo-visible) | 180→180 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| poi/test-data_document_bug65649.docx | align | [0] | 0(lo-visible) | 180→180 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| poi/test-data_document_bug65649.docx | cell | [2] | 0(lo-visible) | 180→180 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/floatingtbl_with_formula.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/floatingtbl_with_formula.docx | enter | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/floatingtbl_with_formula.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| ooxmlexport/floatingtbl_with_formula.docx | align | [0] | 0(lo-visible) | 1→1 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| ooxmlexport/floatingtbl_with_formula.docx | cell | [1] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/tdf102466.docx | type | [6] | 1(lo-visible) | 11→11 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/tdf102466.docx | enter | [6] | 1(lo-visible) | 11→11 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/tdf102466.docx | bold | [6] | 1(lo-visible) | 11→11 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| ooxmlexport/tdf102466.docx | align | [6] | 1(lo-visible) | 11→11 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/tdf102466.docx | cell | [7] | 1(lo-visible) | 11→11 | [] | ✓ | ✓ PASS |  |
| xopt/TestFiles_HC006-Test-01.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| xopt/TestFiles_HC006-Test-01.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| xopt/TestFiles_HC006-Test-01.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| xopt/TestFiles_HC006-Test-01.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| xopt/TestFiles_HC006-Test-01.docx | cell | [6] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlimport/tdf169173.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlimport/tdf169173.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlimport/tdf169173.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlimport/tdf169173.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlimport/tdf169173.docx | cell | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| xopt/TestFiles_RP_RP051-Arabic.docx | type | [18] | 0(lo-visible) | 7→7 | [] | ✓ | ✓ PASS |  |
| xopt/TestFiles_RP_RP051-Arabic.docx | enter | [21] | 0(lo-visible) | 7→7 | [] | ✓ | ✓ PASS |  |
| xopt/TestFiles_RP_RP051-Arabic.docx | bold | [18] | 0(lo-visible) | 7→7 | [] | ✓ | ✓ PASS |  |
| xopt/TestFiles_RP_RP051-Arabic.docx | align | [18] | 0(lo-visible) | 7→7 | [] | ✓ | ✓ PASS |  |
| xopt/TestFiles_RP_RP051-Arabic.docx | cell | [7] | 0(lo-visible) | 7→7 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-05.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-05.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-05.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-05.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-05.docx | cell | [0] | 0(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/tdf127814.docx | type | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/tdf127814.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/tdf127814.docx | bold | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/tdf127814.docx | align | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/tdf127814.docx | cell | [2] | 1(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_drawing.docx | type | [0] | 0(lo-visible) | 20→20 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_drawing.docx | enter | [3] | 0(lo-visible) | 20→20 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_drawing.docx | bold | [0] | 0(lo-visible) | 20→20 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| poi/test-data_document_drawing.docx | align | [0] | 0(lo-visible) | 20→20 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_drawing.docx | cell | [11] | 0(lo-visible) | 20→20 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/n779642.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/n779642.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/n779642.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/n779642.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/n779642.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/tdf123705.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/tdf123705.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/tdf123705.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/tdf123705.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/tdf123705.docx | cell | [0] | 0(lo-visible) | 4→4 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/dkvert.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/dkvert.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/dkvert.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/dkvert.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/dkvert.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_testComment.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_testComment.docx | enter | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_testComment.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_testComment.docx | align | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_testComment.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_form_footnotes.docx | type | [7] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_form_footnotes.docx | enter | [7] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_form_footnotes.docx | bold | [7] | 0(lo-visible) | 2→2 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| poi/test-data_document_form_footnotes.docx | align | [7] | 0(lo-visible) | 2→2 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| poi/test-data_document_form_footnotes.docx | cell | [1] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo76098.docx | type | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo76098.docx | enter | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo76098.docx | bold | [0] | 0(lo-visible) | 2→2 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| ooxmlexport/fdo76098.docx | align | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo76098.docx | cell | [5] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| phpword/tests_PhpWordTests__files_documents_reader.docx | type | [0] | 0(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| phpword/tests_PhpWordTests__files_documents_reader.docx | enter | [1] | 0(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| phpword/tests_PhpWordTests__files_documents_reader.docx | bold | [0] | 0(lo-visible) | 3→3 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| phpword/tests_PhpWordTests__files_documents_reader.docx | align | [0] | 0(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| phpword/tests_PhpWordTests__files_documents_reader.docx | cell | [23] | 0(no-target) | 3→3 | [] | ✓ | ✓ PASS |  |
| selfgen/arabic_rtl_notes.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/arabic_rtl_notes.docx | enter | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/arabic_rtl_notes.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/arabic_rtl_notes.docx | align | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/arabic_rtl_notes.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| phpword/samples_resources_Sample_11_ReadWord2007.docx | type | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| phpword/samples_resources_Sample_11_ReadWord2007.docx | enter | [1] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| phpword/samples_resources_Sample_11_ReadWord2007.docx | bold | [0] | 0(lo-visible) | 2→2 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| phpword/samples_resources_Sample_11_ReadWord2007.docx | align | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| phpword/samples_resources_Sample_11_ReadWord2007.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table_footnotes.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table_footnotes.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table_footnotes.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table_footnotes.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table_footnotes.docx | cell | [0] | 0(quire-fallback) | 1→1 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_Bug54849.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_Bug54849.docx | enter | [2] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_Bug54849.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_Bug54849.docx | align | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_Bug54849.docx | cell | [3] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/n830205.docx | type | [2] | 0(lo-visible) | 5→5 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/n830205.docx | enter | [10] | —(—) | —→— | — | — | ✗ FAIL | 編輯存檔失敗: save rc=3: mutation #0 套用失敗: 引擎錯誤: XML 解析失敗: ill-formed document: expected `</w:ins>`, but `</w:p>` was found
原始: {"SplitParagraph": {"target": "Body", "pos": {"para": 10, "cp": 29}}} |
| ooxmlexport/n830205.docx | bold | [2] | 0(lo-visible) | 5→5 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/n830205.docx | align | [2] | 0(lo-visible) | 5→5 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| ooxmlexport/n830205.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx | enter | [1] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx | align | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo78882.docx | type | [0] | 0(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo78882.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo78882.docx | bold | [0] | 0(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo78882.docx | align | [0] | 0(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo78882.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/090716_Studentische_Arbeit_VWS.docx | type | [6] | 2(lo-visible) | 6→6 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/090716_Studentische_Arbeit_VWS.docx | enter | [6] | 2(lo-visible) | 6→6 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/090716_Studentische_Arbeit_VWS.docx | bold | [6] | 2(lo-visible) | 6→6 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/090716_Studentische_Arbeit_VWS.docx | align | [6] | 2(lo-visible) | 6→6 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/090716_Studentische_Arbeit_VWS.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlimport/tdf60351.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlimport/tdf60351.docx | enter | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlimport/tdf60351.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlimport/tdf60351.docx | align | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlimport/tdf60351.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/tdf117297_tableStyle.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/tdf117297_tableStyle.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/tdf117297_tableStyle.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/tdf117297_tableStyle.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/tdf117297_tableStyle.docx | cell | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_WordWithAttachments.docx | type | [1] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_WordWithAttachments.docx | enter | [4] | —(—) | —→— | — | — | ✗ FAIL | 編輯存檔失敗: save rc=3: mutation #0 套用失敗: 引擎錯誤: XML 解析失敗: ill-formed document: expected `</w:hyperlink>`, but `</w:p>` was found
原始: {"SplitParagraph": {"target": "Body", "pos": {"para": 4, "cp": 23}}} |
| poi/test-data_document_WordWithAttachments.docx | bold | [1] | 0(lo-visible) | 2→2 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| poi/test-data_document_WordWithAttachments.docx | align | [1] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_WordWithAttachments.docx | cell | [5] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| selfgen/patho_nested_table_lists.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/patho_nested_table_lists.docx | enter | [1] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/patho_nested_table_lists.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| selfgen/patho_nested_table_lists.docx | align | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/patho_nested_table_lists.docx | cell | [2] | 0(no-target) | 1→1 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx | type | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx | enter | [2] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx | bold | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx | align | [2] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx | type | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx | enter | [2] | —(—) | —→— | — | — | ✗ FAIL | 編輯存檔失敗: save rc=3: mutation #0 套用失敗: 引擎錯誤: XML 解析失敗: ill-formed document: expected `</w:fldSimple>`, but `</w:p>` was found
原始: {"SplitParagraph": {"target": "Body", "pos": {"para": 2, "cp": 8}}} |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx | bold | [0] | 0(lo-visible) | 2→2 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx | align | [2] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo78883.docx | type | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo78883.docx | enter | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo78883.docx | bold | [0] | 0(lo-visible) | 2→2 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| ooxmlexport/fdo78883.docx | align | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo78883.docx | cell | [2] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/2col-header.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/2col-header.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/2col-header.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/2col-header.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/2col-header.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx | type | [0] | 0(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx | enter | [0] | 0(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx | bold | [0] | 0(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx | align | [0] | 0(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx | cell | [21] | 1(lo-visible) | 3→3 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo74605.docx | type | [4] | 1(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo74605.docx | enter | [4] | 1(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo74605.docx | bold | [4] | 1(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo74605.docx | align | [4] | 1(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo74605.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo66929.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo66929.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo66929.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo66929.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo66929.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlimport/tdf154695-ToC_no_numbers.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlimport/tdf154695-ToC_no_numbers.docx | enter | [1] | —(—) | —→— | — | — | ✗ FAIL | 編輯存檔失敗: save rc=3: mutation #0 套用失敗: 引擎錯誤: XML 解析失敗: ill-formed document: expected `</w:hyperlink>`, but `</w:p>` was found
原始: {"SplitParagraph": {"target": "Body", "pos": {"para": 1, "cp": 5}}} |
| ooxmlimport/tdf154695-ToC_no_numbers.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlimport/tdf154695-ToC_no_numbers.docx | align | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| ooxmlimport/tdf154695-ToC_no_numbers.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx | enter | [2] | 0(lo-visible) | 1→2 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx | align | [2] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| selfgen/embedded_font_odttf.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/embedded_font_odttf.docx | enter | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/embedded_font_odttf.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/embedded_font_odttf.docx | align | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/embedded_font_odttf.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_ChartUpdater01_Chart-Cached-Data-05.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_ChartUpdater01_Chart-Cached-Data-05.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_ChartUpdater01_Chart-Cached-Data-05.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_ChartUpdater01_Chart-Cached-Data-05.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| xopt/OpenXmlPowerToolsExamples_ChartUpdater01_Chart-Cached-Data-05.docx | cell | [] | —(—) | —→— | — | — | — skip |  |

## 臂 C：本專案渲染臂（存檔後檔 vs LO，sweep_cmp 兼容）

| 檔案 | LO 頁 | OW 頁 | 頁數 | 內容相似度（首 6 頁） | 判定 |
|---|---|---|---|---|---|
| corpus/poi/test-data_document_table-alignment.docx →cell | 1 | 1 | ✓ | 0.984 | ✓ PASS |
| corpus/selfgen/patho_table_cell_lists.docx →type | 1 | 1 | ✓ | 0.887 | ✓ PASS |
| corpus/selfgen/patho_table_cell_lists.docx →enter | 1 | 1 | ✓ | 0.883 | ✓ PASS |
| corpus/selfgen/patho_table_cell_lists.docx →bold | 1 | 1 | ✓ | 0.882 | — skip |
| corpus/selfgen/patho_table_cell_lists.docx →align | 1 | 1 | ✓ | 0.882 | ✓ PASS |
| corpus/selfgen/patho_table_cell_lists.docx →cell | 1 | 1 | ✓ | 0.886 | ✓ PASS |
| corpus/poi/test-data_document_bug65649.docx →type | 180 | 190 | ✗ | 0.903, 0.069, 0.311, 0.28, 0.339, 0.412 | ✓ PASS |
| corpus/poi/test-data_document_bug65649.docx →enter | 180 | 190 | ✗ | 0.903, 0.069, 0.311, 0.28, 0.339, 0.412 | ✓ PASS |
| corpus/poi/test-data_document_bug65649.docx →bold | 180 | 190 | ✗ | 0.903, 0.069, 0.311, 0.28, 0.339, 0.412 | — skip |
| corpus/poi/test-data_document_bug65649.docx →align | 180 | 190 | ✗ | 0.903, 0.069, 0.311, 0.28, 0.339, 0.412 | — skip |
| corpus/poi/test-data_document_bug65649.docx →cell | 180 | 190 | ✗ | 0.895, 0.069, 0.311, 0.28, 0.339, 0.412 | ✓ PASS |
| corpus/ooxmlexport/floatingtbl_with_formula.docx →type | 1 | 1 | ✓ | 0.86 | ✓ PASS |
| corpus/ooxmlexport/floatingtbl_with_formula.docx →enter | 1 | 1 | ✓ | 0.86 | ✓ PASS |
| corpus/ooxmlexport/floatingtbl_with_formula.docx →bold | 1 | 1 | ✓ | 0.86 | — skip |
| corpus/ooxmlexport/floatingtbl_with_formula.docx →align | 1 | 1 | ✓ | 0.86 | — skip |
| corpus/ooxmlexport/floatingtbl_with_formula.docx →cell | 1 | 1 | ✓ | 0.853 | ✓ PASS |
| corpus/ooxmlexport/tdf102466.docx →type | 11 | 11 | ✓ | 0.0, 0.893, 0.896, 0.38, 0.425, 1.0 | ✓ PASS |
| corpus/ooxmlexport/tdf102466.docx →enter | 11 | 11 | ✓ | 0.0, 0.893, 0.896, 0.38, 0.425, 1.0 | ✓ PASS |
| corpus/ooxmlexport/tdf102466.docx →bold | 11 | 11 | ✓ | 0.0, 0.893, 0.896, 0.38, 0.425, 1.0 | — skip |
| corpus/ooxmlexport/tdf102466.docx →align | 11 | 11 | ✓ | 0.0, 0.893, 0.896, 0.38, 0.425, 1.0 | ✓ PASS |
| corpus/ooxmlexport/tdf102466.docx →cell | 11 | 11 | ✓ | 0.0, 0.886, 0.896, 0.38, 0.425, 1.0 | ✓ PASS |
| corpus/xopt/TestFiles_HC006-Test-01.docx →cell | 2 | 1 | ✗ | 0.95, 0.0 | ✓ PASS |
| corpus/ooxmlimport/tdf169173.docx →cell | 1 | 1 | ✓ | 1.0 | ✓ PASS |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx →type | 7 | 6 | ✗ | 0.787, 0.691, 0.655, 0.488, 0.338, 0.378 | ✓ PASS |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx →enter | 7 | 6 | ✗ | 0.751, 0.665, 0.622, 0.492, 0.328, 0.398 | ✓ PASS |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx →bold | 7 | 6 | ✗ | 0.787, 0.691, 0.655, 0.488, 0.338, 0.378 | ✓ PASS |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx →align | 7 | 6 | ✗ | 0.787, 0.691, 0.655, 0.488, 0.338, 0.378 | ✓ PASS |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx →cell | 7 | 6 | ✗ | 0.786, 0.691, 0.655, 0.488, 0.338, 0.378 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-05.docx →cell | 3 | 3 | ✓ | 0.828, 0.751, 0.539 | ✓ PASS |
| corpus/ooxmlexport/tdf127814.docx →type | 2 | 2 | ✓ | 0.933, 0.899 | ✓ PASS |
| corpus/ooxmlexport/tdf127814.docx →bold | 2 | 2 | ✓ | 0.818, 0.899 | ✓ PASS |
| corpus/ooxmlexport/tdf127814.docx →align | 2 | 2 | ✓ | 0.9, 0.899 | ✓ PASS |
| corpus/ooxmlexport/tdf127814.docx →cell | 2 | 2 | ✓ | 0.9, 0.883 | ✓ PASS |
| corpus/poi/test-data_document_drawing.docx →type | 20 | 18 | ✗ | 0.871, 0.757, 0.62, 0.217, 0.198, 0.024 | ✓ PASS |
| corpus/poi/test-data_document_drawing.docx →enter | 20 | 18 | ✗ | 0.886, 0.769, 0.643, 0.312, 0.227, 0.074 | ✓ PASS |
| corpus/poi/test-data_document_drawing.docx →bold | 20 | 18 | ✗ | 0.871, 0.757, 0.62, 0.217, 0.198, 0.024 | — skip |
| corpus/poi/test-data_document_drawing.docx →align | 20 | 18 | ✗ | 0.871, 0.757, 0.62, 0.217, 0.198, 0.024 | ✓ PASS |
| corpus/poi/test-data_document_drawing.docx →cell | 20 | 18 | ✗ | 0.871, 0.757, 0.62, 0.217, 0.198, 0.024 | ✓ PASS |
| corpus/ooxmlexport/tdf123705.docx →cell | 4 | 4 | ✓ | 0.829, 0.731, 0.757, 0.037 | ✓ PASS |
| corpus/poi/test-data_document_testComment.docx →type | 1 | 1 | ✓ | 1.0 | ✓ PASS |
| corpus/poi/test-data_document_testComment.docx →enter | 1 | 1 | ✓ | 1.0 | ✓ PASS |
| corpus/poi/test-data_document_testComment.docx →bold | 1 | 1 | ✓ | 0.982 | ✓ PASS |
| corpus/poi/test-data_document_testComment.docx →align | 1 | 1 | ✓ | 1.0 | ✓ PASS |
| corpus/poi/test-data_document_form_footnotes.docx →type | 2 | 3 | ✗ | 0.713, 0.791, 0.0 | ✓ PASS |
| corpus/poi/test-data_document_form_footnotes.docx →enter | 2 | 3 | ✗ | 0.71, 0.788, 0.0 | ✓ PASS |
| corpus/poi/test-data_document_form_footnotes.docx →bold | 2 | 3 | ✗ | 0.712, 0.791, 0.0 | — skip |
| corpus/poi/test-data_document_form_footnotes.docx →align | 2 | 3 | ✗ | 0.712, 0.791, 0.0 | — skip |
| corpus/poi/test-data_document_form_footnotes.docx →cell | 2 | 3 | ✗ | 0.71, 0.788, 0.0 | ✓ PASS |
| corpus/ooxmlexport/fdo76098.docx →type | 2 | 2 | ✓ | 0.884, 0.734 | ✓ PASS |
| corpus/ooxmlexport/fdo76098.docx →enter | 2 | 2 | ✓ | 0.884, 0.734 | ✓ PASS |
| corpus/ooxmlexport/fdo76098.docx →bold | 2 | 2 | ✓ | 0.884, 0.734 | — skip |
| corpus/ooxmlexport/fdo76098.docx →align | 2 | 2 | ✓ | 0.884, 0.734 | ✓ PASS |
| corpus/ooxmlexport/fdo76098.docx →cell | 2 | 2 | ✓ | 0.884, 0.734 | ✓ PASS |
| corpus/phpword/tests_PhpWordTests__files_documents_reader.docx →type | 3 | 3 | ✓ | 0.964, 0.901, 0.856 | ✓ PASS |
| corpus/phpword/tests_PhpWordTests__files_documents_reader.docx →enter | 3 | 3 | ✓ | 0.965, 0.901, 0.856 | ✓ PASS |
| corpus/phpword/tests_PhpWordTests__files_documents_reader.docx →bold | 3 | 3 | ✓ | 0.965, 0.901, 0.856 | — skip |
| corpus/phpword/tests_PhpWordTests__files_documents_reader.docx →align | 3 | 3 | ✓ | 0.965, 0.901, 0.856 | ✓ PASS |
| corpus/phpword/tests_PhpWordTests__files_documents_reader.docx →cell | 3 | 3 | ✓ | 0.965, 0.9, 0.856 | ✓ PASS |
| corpus/selfgen/arabic_rtl_notes.docx →type | 1 | 1 | ✓ | 0.929 | ✓ PASS |
| corpus/selfgen/arabic_rtl_notes.docx →enter | 1 | 1 | ✓ | 0.927 | ✓ PASS |
| corpus/selfgen/arabic_rtl_notes.docx →bold | 1 | 1 | ✓ | 0.92 | ✓ PASS |
| corpus/selfgen/arabic_rtl_notes.docx →align | 1 | 1 | ✓ | 0.926 | ✓ PASS |
| corpus/phpword/samples_resources_Sample_11_ReadWord2007.docx →type | 2 | 3 | ✗ | 0.979, 0.463, 0.0 | ✓ PASS |
| corpus/phpword/samples_resources_Sample_11_ReadWord2007.docx →enter | 2 | 3 | ✗ | 0.98, 0.463, 0.0 | ✓ PASS |
| corpus/phpword/samples_resources_Sample_11_ReadWord2007.docx →bold | 2 | 3 | ✗ | 0.98, 0.463, 0.0 | — skip |
| corpus/phpword/samples_resources_Sample_11_ReadWord2007.docx →align | 2 | 3 | ✗ | 0.98, 0.463, 0.0 | ✓ PASS |
| corpus/poi/test-data_document_table_footnotes.docx →cell | 1 | 1 | ✓ | 0.954 | ✓ PASS |
| corpus/poi/test-data_document_Bug54849.docx →type | 1 | 1 | ✓ | 0.941 | ✓ PASS |
| corpus/poi/test-data_document_Bug54849.docx →enter | 1 | 1 | ✓ | 0.944 | ✓ PASS |
| corpus/poi/test-data_document_Bug54849.docx →bold | 1 | 1 | ✓ | 0.942 | ✓ PASS |
| corpus/poi/test-data_document_Bug54849.docx →align | 1 | 1 | ✓ | 0.944 | ✓ PASS |
| corpus/poi/test-data_document_Bug54849.docx →cell | 1 | 1 | ✓ | 0.943 | ✓ PASS |
| corpus/ooxmlexport/n830205.docx →type | 5 | 5 | ✓ | 0.048, 0.019, 0.945, 0.998, 1.0 | ✓ PASS |
| corpus/ooxmlexport/n830205.docx →bold | 5 | 5 | ✓ | 0.045, 0.019, 0.945, 0.998, 1.0 | ✓ PASS |
| corpus/ooxmlexport/n830205.docx →align | 5 | 5 | ✓ | 0.045, 0.019, 0.945, 0.998, 1.0 | — skip |
| corpus/phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx →type | 1 | 1 | ✓ | 0.975 | ✓ PASS |
| corpus/phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx →enter | 1 | 1 | ✓ | 0.975 | ✓ PASS |
| corpus/phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx →bold | 1 | 1 | ✓ | 0.975 | — skip |
| corpus/phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx →align | 1 | 1 | ✓ | 0.975 | ✓ PASS |
| corpus/ooxmlexport/fdo78882.docx →type | 3 | 2 | ✗ | 0.966, 0.967, 0.0 | ✓ PASS |
| corpus/ooxmlexport/fdo78882.docx →bold | 3 | 2 | ✗ | 0.909, 0.967, 0.0 | ✓ PASS |
| corpus/ooxmlexport/fdo78882.docx →align | 3 | 2 | ✗ | 1.0, 0.967, 0.0 | ✓ PASS |
| corpus/ooxmlexport/090716_Studentische_Arbeit_VWS.docx →type | 6 | 6 | ✓ | 0.0, 0.036, 0.976, 1.0, 1.0, 0.963 | ✓ PASS |
| corpus/ooxmlexport/090716_Studentische_Arbeit_VWS.docx →enter | 6 | 6 | ✓ | 0.0, 0.036, 1.0, 1.0, 1.0, 0.963 | ✓ PASS |
| corpus/ooxmlexport/090716_Studentische_Arbeit_VWS.docx →bold | 6 | 6 | ✓ | 0.0, 0.036, 0.941, 1.0, 1.0, 0.963 | ✓ PASS |
| corpus/ooxmlexport/090716_Studentische_Arbeit_VWS.docx →align | 6 | 6 | ✓ | 0.0, 0.036, 1.0, 1.0, 1.0, 0.963 | ✓ PASS |
| corpus/ooxmlimport/tdf60351.docx →type | 1 | 1 | ✓ | 1.0 | ✓ PASS |
| corpus/ooxmlimport/tdf60351.docx →enter | 1 | 1 | ✓ | 1.0 | ✓ PASS |
| corpus/ooxmlimport/tdf60351.docx →bold | 1 | 1 | ✓ | 0.996 | ✓ PASS |
| corpus/ooxmlimport/tdf60351.docx →align | 1 | 1 | ✓ | 1.0 | ✓ PASS |
| corpus/ooxmlexport/tdf117297_tableStyle.docx →cell | 1 | 2 | ✗ | 0.809, 0.0 | ✓ PASS |
| corpus/poi/test-data_document_WordWithAttachments.docx →type | 2 | 2 | ✓ | 0.96, 0.65 | ✓ PASS |
| corpus/poi/test-data_document_WordWithAttachments.docx →bold | 2 | 2 | ✓ | 0.961, 0.65 | — skip |
| corpus/poi/test-data_document_WordWithAttachments.docx →align | 2 | 2 | ✓ | 0.961, 0.65 | ✓ PASS |
| corpus/poi/test-data_document_WordWithAttachments.docx →cell | 2 | 2 | ✓ | 0.96, 0.65 | ✓ PASS |
| corpus/selfgen/patho_nested_table_lists.docx →type | 1 | 1 | ✓ | 0.832 | ✓ PASS |
| corpus/selfgen/patho_nested_table_lists.docx →enter | 1 | 1 | ✓ | 0.826 | ✓ PASS |
| corpus/selfgen/patho_nested_table_lists.docx →bold | 1 | 1 | ✓ | 0.824 | — skip |
| corpus/selfgen/patho_nested_table_lists.docx →align | 1 | 1 | ✓ | 0.824 | ✓ PASS |
| corpus/selfgen/patho_nested_table_lists.docx →cell | 1 | 1 | ✓ | 0.83 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx →type | 2 | 2 | ✓ | 0.887, 0.897 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx →enter | 2 | 2 | ✓ | 0.916, 0.928 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx →bold | 2 | 2 | ✓ | 0.928, 0.93 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx →align | 2 | 2 | ✓ | 0.928, 0.93 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx →type | 2 | 2 | ✓ | 0.904, 0.84 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx →bold | 2 | 2 | ✓ | 0.939, 0.887 | — skip |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx →align | 2 | 2 | ✓ | 0.939, 0.887 | ✓ PASS |
| corpus/ooxmlexport/fdo78883.docx →type | 2 | 2 | ✓ | 0.724, 0.0 | ✓ PASS |
| corpus/ooxmlexport/fdo78883.docx →enter | 2 | 2 | ✓ | 0.691, 0.0 | ✓ PASS |
| corpus/ooxmlexport/fdo78883.docx →bold | 2 | 2 | ✓ | 0.724, 0.0 | — skip |
| corpus/ooxmlexport/fdo78883.docx →align | 2 | 2 | ✓ | 0.724, 0.0 | ✓ PASS |
| corpus/ooxmlexport/fdo78883.docx →cell | 2 | 2 | ✓ | 0.724, 0.0 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx →type | 3 | 3 | ✓ | 0.989, 0.97, 0.975 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx →enter | 3 | 3 | ✓ | 0.963, 0.898, 0.901 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx →bold | 3 | 3 | ✓ | 0.963, 0.898, 0.901 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx →align | 3 | 3 | ✓ | 0.963, 0.898, 0.901 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx →cell | 3 | 3 | ✓ | 0.963, 0.898, 0.901 | ✓ PASS |
| corpus/ooxmlexport/fdo74605.docx →type | 2 | 2 | ✓ | 0.999, 0.581 | ✓ PASS |
| corpus/ooxmlexport/fdo74605.docx →enter | 2 | 2 | ✓ | 0.999, 0.573 | ✓ PASS |
| corpus/ooxmlexport/fdo74605.docx →bold | 2 | 2 | ✓ | 0.999, 0.573 | ✓ PASS |
| corpus/ooxmlexport/fdo74605.docx →align | 2 | 2 | ✓ | 0.999, 0.573 | ✓ PASS |
| corpus/ooxmlimport/tdf154695-ToC_no_numbers.docx →type | 1 | 1 | ✓ | 0.942 | ✓ PASS |
| corpus/ooxmlimport/tdf154695-ToC_no_numbers.docx →bold | 1 | 1 | ✓ | 0.937 | ✓ PASS |
| corpus/ooxmlimport/tdf154695-ToC_no_numbers.docx →align | 1 | 1 | ✓ | 0.944 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx →type | 1 | 1 | ✓ | 0.978 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx →enter | 2 | 1 | ✗ | 0.924, 0.0 | ✓ PASS |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx →bold | 1 | 1 | ✓ | 0.979 | — skip |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx →align | 1 | 1 | ✓ | 0.979 | ✓ PASS |
| corpus/selfgen/embedded_font_odttf.docx →type | 1 | 1 | ✓ | 1.0 | ✓ PASS |
| corpus/selfgen/embedded_font_odttf.docx →enter | 1 | 1 | ✓ | 1.0 | ✓ PASS |
| corpus/selfgen/embedded_font_odttf.docx →bold | 1 | 1 | ✓ | 0.977 | ✓ PASS |
| corpus/selfgen/embedded_font_odttf.docx →align | 1 | 1 | ✓ | 1.0 | ✓ PASS |

## 臂 C（原檔基線行，供 sweep_cmp 跨版本對照：同一 seed 取樣下
## 上表存檔後分數不得低於本表對應分——突變區外；由本腳本內部把關）

| 檔案 | LO 頁 | OW 頁 | 頁數 | 內容相似度（首 6 頁） |
|---|---|---|---|---|
| corpus/poi/test-data_document_table-alignment.docx | 1 | 1 | ✓ | 0.985 |
| corpus/selfgen/patho_table_cell_lists.docx | 1 | 1 | ✓ | 0.882 |
| corpus/ooxmlexport/fdo79540.docx | 1 | 1 | ✓ | — |
| corpus/poi/test-data_document_bug65649.docx | 180 | 190 | ✗ | 0.903, 0.069, 0.311, 0.28, 0.339, 0.412 |
| corpus/ooxmlexport/floatingtbl_with_formula.docx | 1 | 1 | ✓ | 0.86 |
| corpus/ooxmlexport/tdf102466.docx | 11 | 11 | ✓ | 0.0, 0.893, 0.896, 0.38, 0.425, 1.0 |
| corpus/xopt/TestFiles_HC006-Test-01.docx | 2 | 1 | ✗ | 0.95, 0.0 |
| corpus/ooxmlimport/tdf169173.docx | 1 | 1 | ✓ | 1.0 |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx | 7 | 6 | ✗ | 0.787, 0.691, 0.655, 0.488, 0.338, 0.378 |
| corpus/xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-05.docx | 3 | 3 | ✓ | 0.828, 0.751, 0.539 |
| corpus/ooxmlexport/tdf127814.docx | 2 | 2 | ✓ | 0.9, 0.899 |
| corpus/poi/test-data_document_drawing.docx | 20 | 18 | ✗ | 0.871, 0.757, 0.62, 0.217, 0.198, 0.024 |
| corpus/ooxmlexport/n779642.docx | 1 | 1 | ✓ | 1.0 |
| corpus/ooxmlexport/tdf123705.docx | 4 | 4 | ✓ | 0.829, 0.731, 0.757, 0.037 |
| corpus/ooxmlexport/dkvert.docx | 1 | 1 | ✓ | 1.0 |
| corpus/poi/test-data_document_testComment.docx | 1 | 1 | ✓ | 1.0 |
| corpus/poi/test-data_document_form_footnotes.docx | 2 | 3 | ✗ | 0.712, 0.791, 0.0 |
| corpus/ooxmlexport/fdo76098.docx | 2 | 2 | ✓ | 0.884, 0.734 |
| corpus/phpword/tests_PhpWordTests__files_documents_reader.docx | 3 | 3 | ✓ | 0.965, 0.901, 0.856 |
| corpus/selfgen/arabic_rtl_notes.docx | 1 | 1 | ✓ | 0.926 |
| corpus/phpword/samples_resources_Sample_11_ReadWord2007.docx | 2 | 3 | ✗ | 0.98, 0.463, 0.0 |
| corpus/poi/test-data_document_table_footnotes.docx | 1 | 1 | ✓ | 0.966 |
| corpus/poi/test-data_document_Bug54849.docx | 1 | 1 | ✓ | 0.944 |
| corpus/ooxmlexport/n830205.docx | 5 | 5 | ✓ | 0.045, 0.019, 0.945, 0.998, 1.0 |
| corpus/phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx | 1 | 1 | ✓ | 0.975 |
| corpus/ooxmlexport/fdo78882.docx | 3 | 2 | ✗ | 1.0, 0.967, 0.0 |
| corpus/ooxmlexport/090716_Studentische_Arbeit_VWS.docx | 6 | 6 | ✓ | 0.0, 0.036, 1.0, 1.0, 1.0, 0.963 |
| corpus/ooxmlimport/tdf60351.docx | 1 | 1 | ✓ | 1.0 |
| corpus/ooxmlexport/tdf117297_tableStyle.docx | 1 | 2 | ✗ | 0.807, 0.0 |
| corpus/poi/test-data_document_WordWithAttachments.docx | 2 | 2 | ✓ | 0.961, 0.65 |
| corpus/selfgen/patho_nested_table_lists.docx | 1 | 1 | ✓ | 0.824 |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx | 2 | 2 | ✓ | 0.928, 0.93 |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx | 2 | 2 | ✓ | 0.939, 0.887 |
| corpus/ooxmlexport/fdo78883.docx | 2 | 2 | ✓ | 0.724, 0.0 |
| corpus/ooxmlexport/2col-header.docx | 1 | 1 | ✓ | 0.632 |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder01_Source1.docx | 3 | 3 | ✓ | 0.963, 0.898, 0.901 |
| corpus/ooxmlexport/fdo74605.docx | 2 | 2 | ✓ | 0.999, 0.573 |
| corpus/ooxmlexport/fdo66929.docx | 1 | 1 | ✓ | 1.0 |
| corpus/ooxmlimport/tdf154695-ToC_no_numbers.docx | 1 | 1 | ✓ | 0.944 |
| corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx | 1 | 1 | ✓ | 0.979 |
| corpus/selfgen/embedded_font_odttf.docx | 1 | 1 | ✓ | 1.0 |
| corpus/xopt/OpenXmlPowerToolsExamples_ChartUpdater01_Chart-Cached-Data-05.docx | 1 | 1 | ✓ | 0.224 |

## 引擎突變層缺陷實錄（PM 裁決 11-25：不歸本卡修，據此開返修卡）

### ooxmlexport/n830205.docx｜enter 臂

- 錯誤：編輯存檔失敗: save rc=3: mutation #0 套用失敗: 引擎錯誤: XML 解析失敗: ill-formed document: expected `</w:ins>`, but `</w:p>` was found
原始: {"SplitParagraph": {"target": "Body", "pos": {"para": 10, "cp": 29}}}
- 最小重現：`quire-cli save ooxmlexport/n830205.docx --out x.docx --mutation '{"SplitParagraph": {"target": "Body", "pos": {"para": 10, "cp": 29}}}'`
- 目標段外包裝容器計數（跨切點未閉合＝根因線索）：{'w:del': '上游未閉（前4000字 3開/1閉）'}

```xml
<w:p w:rsidR="000C113D" w:rsidRPr="00722059" w:rsidRDefault="000C113D" w:rsidP="00100072">
      <w:pPr>
        <w:jc w:val="center"/>
        <w:rPr>
          <w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:cs="Arial"/>
          <w:b/>
          <w:sz w:val="18"/>
          <w:szCs w:val="18"/>
          <w:rPrChange w:id="6" w:author="JimBob" w:date="2013-07-17T12:26:00Z">
            <w:rPr>
              <w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:cs="Arial"/>
              <w:b/>
            </w:rPr>
          </w:rPrChange>
        </w:rPr>
      </w:pPr>
      <w:r w:rsidRPr="00722059">
        <w:rPr>
          <w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:cs="Arial"/>
          <w:b/>
          <w:sz w:val="18"/>
          <w:szCs w:val="18"/>
          <w:rPrChange w:id="7" w:author="JimBob" w:date="2013-07-17T12:26:00Z">
            <w:rPr>
              <w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:cs="Arial"/>
              <w:b/>
            </w:rPr>
          </w:rPrChange>
        </w:rPr>
        <w:t>BodyText</w:t>
      </w:r>
    </w:p>
```

### poi/test-data_document_WordWithAttachments.docx｜enter 臂

- 錯誤：編輯存檔失敗: save rc=3: mutation #0 套用失敗: 引擎錯誤: XML 解析失敗: ill-formed document: expected `</w:hyperlink>`, but `</w:p>` was found
原始: {"SplitParagraph": {"target": "Body", "pos": {"para": 4, "cp": 23}}}
- 最小重現：`quire-cli save poi/test-data_document_WordWithAttachments.docx --out x.docx --mutation '{"SplitParagraph": {"target": "Body", "pos": {"para": 4, "cp": 23}}}'`
- 目標段外包裝容器計數（跨切點未閉合＝根因線索）：{'w:hyperlink': '段內3開/3閉'}

```xml
<w:p w:rsidR="0047220F" w:rsidRPr="00A17CAE" w:rsidRDefault="0047220F"/></w:txbxContent></v:textbox></v:shape><w10:wrap type="none"/><w10:anchorlock/></v:group></w:pict></w:r><w:hyperlink r:id="rId10" w:history="1"><w:r w:rsidR="00571BF5"><w:rPr><w:rStyle w:val="af"/><w:b/><w:sz w:val="40"/><w:szCs w:val="40"/><w:lang w:val="en-US"/></w:rPr><w:t>Hyperlink</w:t></w:r></w:hyperlink><w:r w:rsidR="00F2789D" w:rsidRPr="00A50BB5"><w:rPr><w:b/><w:sz w:val="40"/><w:szCs w:val="40"/><w:lang w:val="en-US"/></w:rPr><w:t xml:space="preserve"> </w:t></w:r><w:hyperlink r:id="rId11" w:history="1"><w:r w:rsidR="00F2789D" w:rsidRPr="00F2789D"><w:rPr><w:rStyle w:val="af"/><w:b/><w:sz w:val="40"/><w:szCs w:val="40"/></w:rPr><w:t>Ссылка</w:t></w:r></w:hyperlink><w:r w:rsidR="00F2789D" w:rsidRPr="00A50BB5"><w:rPr><w:b/><w:sz w:val="40"/><w:szCs w:val="40"/><w:lang w:val="en-US"/></w:rPr><w:t xml:space="preserve"> </w:t></w:r><w:hyperlink r:id="rId12" w:history="1"><w:r w:rsidR="00F2789D" w:rsidRPr="004A4699"><w:rPr><w:rStyle w:val="af"/><w:b/><w:sz w:val="40"/><w:szCs w:val="40"/></w:rPr><w:t>юбо</w:t></w:r><w:r w:rsidR="004A4699" w:rsidRPr="004A4699"><w:rPr><w:rStyle w:val="af"/><w:b/><w:sz w:val="40"/><w:szCs w:val="40"/><w:lang w:val="uz-Cyrl-UZ"/></w:rPr><w:t>қ</w:t></w:r><w:r w:rsidR="00F2789D" w:rsidRPr="004A4699"><w:rPr><w:rStyle w:val="af"/><w:b/><w:sz w:val="40"/><w:szCs w:val="40"/></w:rP
```

### xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx｜enter 臂

- 錯誤：編輯存檔失敗: save rc=3: mutation #0 套用失敗: 引擎錯誤: XML 解析失敗: ill-formed document: expected `</w:fldSimple>`, but `</w:p>` was found
原始: {"SplitParagraph": {"target": "Body", "pos": {"para": 2, "cp": 8}}}
- 最小重現：`quire-cli save xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx --out x.docx --mutation '{"SplitParagraph": {"target": "Body", "pos": {"para": 2, "cp": 8}}}'`
- 目標段外包裝容器計數（跨切點未閉合＝根因線索）：{'w:fldSimple': '段內1開/1閉'}

```xml
<w:p w:rsidR="00D757CE" w:rsidRDefault="00D757CE" w:rsidP="00D757CE"><w:pPr><w:pStyle w:val="Caption"/><w:rPr><w:rFonts w:eastAsia="宋体"/></w:rPr></w:pPr><w:r><w:t xml:space="preserve">Figure </w:t></w:r><w:fldSimple w:instr=" SEQ Figure \* ARABIC "><w:r><w:rPr><w:noProof/></w:rPr><w:t>1</w:t></w:r></w:fldSimple><w:r><w:rPr><w:rFonts w:eastAsia="宋体" w:hint="eastAsia"/></w:rPr><w:t xml:space="preserve"> Jupiter</w:t></w:r></w:p>
```

### ooxmlimport/tdf154695-ToC_no_numbers.docx｜enter 臂

- 錯誤：編輯存檔失敗: save rc=3: mutation #0 套用失敗: 引擎錯誤: XML 解析失敗: ill-formed document: expected `</w:hyperlink>`, but `</w:p>` was found
原始: {"SplitParagraph": {"target": "Body", "pos": {"para": 1, "cp": 5}}}
- 最小重現：`quire-cli save ooxmlimport/tdf154695-ToC_no_numbers.docx --out x.docx --mutation '{"SplitParagraph": {"target": "Body", "pos": {"para": 1, "cp": 5}}}'`
- 目標段外包裝容器計數（跨切點未閉合＝根因線索）：{'w:hyperlink': '段內1開/1閉', 'w:sdt': '上游未閉（前4000字 1開/0閉）'}

```xml
<w:p w:rsidR="00036FAB" w:rsidRDefault="00036FAB"><w:pPr><w:pStyle w:val="TOC1"/><w:tabs><w:tab w:val="right" w:leader="dot" w:pos="8296"/></w:tabs><w:rPr><w:noProof/></w:rPr></w:pPr><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \o "1-3" \h \z \u </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:hyperlink w:anchor="_Toc131755898" w:history="1"><w:r w:rsidRPr="0080186F"><w:rPr><w:rStyle w:val="Hyperlink"/><w:noProof/></w:rPr><w:t>Heading 1</w:t></w:r><w:r><w:rPr><w:noProof/><w:webHidden/></w:rPr><w:tab/></w:r><w:r><w:rPr><w:noProof/><w:webHidden/></w:rPr><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:rPr><w:noProof/><w:webHidden/></w:rPr><w:instrText xml:space="preserve"> PAGEREF _Toc131755898 \h </w:instrText></w:r><w:r><w:rPr><w:noProof/><w:webHidden/></w:rPr></w:r><w:r><w:rPr><w:noProof/><w:webHidden/></w:rPr><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:rPr><w:noProof/><w:webHidden/></w:rPr><w:t>1</w:t></w:r><w:r><w:rPr><w:noProof/><w:webHidden/></w:rPr><w:fldChar w:fldCharType="end"/></w:r></w:hyperlink></w:p>
```


口徑說明：
- **臂 A 預閘＝位元組恆等**（sha256 對源檔逐位）；恆等過＝像素層跳過但記錄。
  不恆等即紅，並照跑 LO 逐頁像素 diff 計數＋逐頁文字差分以鑑別變動性質。
- **臂 B 判據**：打字/Enter/粗體/對齊/格內皆為內容單調不縮小編輯 ⇒ LO 重排
  只能往後推 ⇒「突變段所在頁之前」的任何頁像素非零＝存檔動到別處（紅）；
  突變區內像素與頁數全無變動＝存檔吞編輯（紅）。區界 quire pagebreaks 錨定，
  LO 原文搜尋校正（fallback 標記如實入報告）。
- **臂 C 判據**：突變區外逐頁內容分 ≥ 原檔 −0.002（存檔不得讓渲染保真退步）；
  `| corpus…` 行與 scripts/sweep_cmp.py 兼容。
- 相似度分只做臂 C 的「不得退步」判據；**臂 A/B 一律用像素 diff 計數**
  （skill 教訓：相似度分對局部缺線不敏感）。
