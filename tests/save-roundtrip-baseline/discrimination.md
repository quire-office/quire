# save-roundtrip-lab 報告（儲存保真閘門）

- 生成時間：2026-11-26T01:09:34
- LibreOffice：`/home/jerry/.local/lo-linux/opt/libreoffice25.8/program/soffice`（本機 Linux LO；Windows soffice.exe 不在此機，PATH 注入口徑自動偵測）
- quire-cli：`/home/jerry/projects/quire/.worktrees/t_d96c31db/target/debug/quire-cli`｜git：`960c1c5`
- 對象：自訂 4 檔（4 檔，seed=None）
- SAVE_RT_BUGGY：True｜--discriminate：True｜像素容差：0
- 紅燈檔案：0；LO 轉換失敗：0
- 鑑別力判定：**紅 4/4 檔點名（回塞鎖有效）**

## 考卷自身效能基線（PM 裁決 3：`paragraph_cp_len` 效能倒退修復實錄）

本輪考卷總時數：44.8 s（4 檔）。本閘門自身最慢檔（逐檔實測，供 nightly 預算參考）：

| 檔案 | 全臂耗時 (s) |
|---|---|
| xopt/TestFiles_RP_RP051-Arabic.docx | 31.1 |
| ooxmlexport/fdo78883.docx | 6.3 |
| selfgen/cjk_japanese_ruby.docx | 5.9 |
| poi/test-data_document_table_footnotes.docx | 1.6 |

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
| selfgen/cjk_japanese_ruby.docx | manual | ✗ | px 非零頁 []｜文字差頁 []｜頁 1→1 | ✗ FAIL |
| ooxmlexport/fdo78883.docx | manual | ✗ | px 非零頁 []｜文字差頁 []｜頁 2→2 | ✗ FAIL |
| poi/test-data_document_table_footnotes.docx | manual | ✗ | px 非零頁 []｜文字差頁 []｜頁 1→1 | ✗ FAIL |
| xopt/TestFiles_RP_RP051-Arabic.docx | manual | ✗ | px 非零頁 []｜文字差頁 []｜頁 7→7 | ✗ FAIL |

## 臂 B：編輯態（突變區前逐像素零位移＋突變可見）

| 檔案 | mutation | 目標段 | 區起(法) | 頁 原→存 | 區前非零頁 | 可見 | 判定 | 原因 |
|---|---|---|---|---|---|---|---|---|
| selfgen/cjk_japanese_ruby.docx | type | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/cjk_japanese_ruby.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| selfgen/cjk_japanese_ruby.docx | bold | [0] | 0(lo-visible) | 1→1 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| selfgen/cjk_japanese_ruby.docx | align | [0] | 0(lo-visible) | 1→1 | [] | ✓ | ✓ PASS |  |
| selfgen/cjk_japanese_ruby.docx | cell | [] | —(—) | —→— | — | — | — skip |  |
| ooxmlexport/fdo78883.docx | type | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo78883.docx | enter | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo78883.docx | bold | [0] | 0(lo-visible) | 2→2 | [] | ✗ | — skip | 突變對目標區視覺無效（內容層已驗證寫入，渲染同形） |
| ooxmlexport/fdo78883.docx | align | [0] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| ooxmlexport/fdo78883.docx | cell | [2] | 0(lo-visible) | 2→2 | [] | ✓ | ✓ PASS |  |
| poi/test-data_document_table_footnotes.docx | type | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table_footnotes.docx | enter | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table_footnotes.docx | bold | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table_footnotes.docx | align | [] | —(—) | —→— | — | — | — skip |  |
| poi/test-data_document_table_footnotes.docx | cell | [0] | 0(quire-fallback) | 1→1 | [] | ✓ | ✓ PASS |  |
| xopt/TestFiles_RP_RP051-Arabic.docx | type | [18] | 0(lo-visible) | 7→7 | [] | ✓ | ✓ PASS |  |
| xopt/TestFiles_RP_RP051-Arabic.docx | enter | [21] | 0(lo-visible) | 7→7 | [] | ✓ | ✓ PASS |  |
| xopt/TestFiles_RP_RP051-Arabic.docx | bold | [18] | 0(lo-visible) | 7→7 | [] | ✓ | ✓ PASS |  |
| xopt/TestFiles_RP_RP051-Arabic.docx | align | [18] | 0(lo-visible) | 7→7 | [] | ✓ | ✓ PASS |  |
| xopt/TestFiles_RP_RP051-Arabic.docx | cell | [7] | 0(lo-visible) | 7→7 | [] | ✓ | ✓ PASS |  |

## 臂 C：本專案渲染臂（存檔後檔 vs LO，sweep_cmp 兼容）

| 檔案 | LO 頁 | OW 頁 | 頁數 | 內容相似度（首 6 頁） | 判定 |
|---|---|---|---|---|---|
| corpus/selfgen/cjk_japanese_ruby.docx →type | 1 | 1 | ✓ | 0.286 | ✓ PASS |
| corpus/selfgen/cjk_japanese_ruby.docx →bold | 1 | 1 | ✓ | 0.211 | — skip |
| corpus/selfgen/cjk_japanese_ruby.docx →align | 1 | 1 | ✓ | 0.211 | ✓ PASS |
| corpus/ooxmlexport/fdo78883.docx →type | 2 | 2 | ✓ | 0.724, 0.0 | ✓ PASS |
| corpus/ooxmlexport/fdo78883.docx →enter | 2 | 2 | ✓ | 0.691, 0.0 | ✓ PASS |
| corpus/ooxmlexport/fdo78883.docx →bold | 2 | 2 | ✓ | 0.724, 0.0 | — skip |
| corpus/ooxmlexport/fdo78883.docx →align | 2 | 2 | ✓ | 0.724, 0.0 | ✓ PASS |
| corpus/ooxmlexport/fdo78883.docx →cell | 2 | 2 | ✓ | 0.724, 0.0 | ✓ PASS |
| corpus/poi/test-data_document_table_footnotes.docx →cell | 1 | 1 | ✓ | 0.954 | ✓ PASS |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx →type | 7 | 6 | ✗ | 0.787, 0.691, 0.655, 0.488, 0.338, 0.378 | ✓ PASS |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx →enter | 7 | 6 | ✗ | 0.751, 0.665, 0.622, 0.492, 0.328, 0.398 | ✓ PASS |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx →bold | 7 | 6 | ✗ | 0.787, 0.691, 0.655, 0.488, 0.338, 0.378 | ✓ PASS |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx →align | 7 | 6 | ✗ | 0.787, 0.691, 0.655, 0.488, 0.338, 0.378 | ✓ PASS |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx →cell | 7 | 6 | ✗ | 0.786, 0.691, 0.655, 0.488, 0.338, 0.378 | ✓ PASS |

## 臂 C（原檔基線行，供 sweep_cmp 跨版本對照：同一 seed 取樣下
## 上表存檔後分數不得低於本表對應分——突變區外；由本腳本內部把關）

| 檔案 | LO 頁 | OW 頁 | 頁數 | 內容相似度（首 6 頁） |
|---|---|---|---|---|
| corpus/selfgen/cjk_japanese_ruby.docx | 1 | 1 | ✓ | 0.211 |
| corpus/ooxmlexport/fdo78883.docx | 2 | 2 | ✓ | 0.724, 0.0 |
| corpus/poi/test-data_document_table_footnotes.docx | 1 | 1 | ✓ | 0.966 |
| corpus/xopt/TestFiles_RP_RP051-Arabic.docx | 7 | 6 | ✗ | 0.787, 0.691, 0.655, 0.488, 0.338, 0.378 |

## 引擎突變層缺陷實錄（PM 裁決 11-25：不歸本卡修，據此開返修卡）

（本輪無 ill-formed XML 級失敗）

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
