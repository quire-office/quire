# J11 三軸面擴散掃描報告（n=30, seed=20261030）

- 執行時間：`2026-10-29T20:40:41`
- 抽樣條件：corpus manifest `n=30`, `seed=20261030`
- LibreOffice：`LibreOffice (from cached json)`
- 總結：掃描 30 檔，**15 檔觸發旗標**（色分<0.3 或 圖分<0.3 或 真缺字>0）；LO 失敗 0 檔

## 1. 觸發旗標檔案彙總表

| 序號 | 檔案 | LO/OW 頁 | 色分序列 | 圖分序列 | 缺字數 | 標籤分類 | 初步歸因研判 |
|---|---|---|---|---|---|---|---|
| 1 | `corpus/ooxmlexport/fdo61343.docx` | 1/1 | 0.26 | 0.23 | 0 | 色彩軸:低分<br>圖片軸:低分 | 色分最低 0.255; 圖分最低 0.234 |
| 2 | `corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Sun.docx` | 2/2 | 0.84, 0.73 | 0.75, 0.00 | 3 | 真缺字:真缺陷候選<br>圖片軸:低分 | LO 有實體文字未發射（3 個，例：['1.1', '1.2', '1.3']）; 圖分最低 0.000 |
| 3 | `corpus/ooxmlexport/tdf115212.docx` | 1/1 | 0.02 | 0.00 | 1 | 真缺字:真缺陷候選<br>色彩軸:低分<br>圖片軸:低分 | LO 有實體文字未發射（1 個，例：['elit']）; 色分最低 0.019; 圖分最低 0.000 |
| 4 | `corpus/poi/test-data_document_58067.docx` | 1/1 | 0.25 | 0.06 | 0 | 色彩軸:低分<br>圖片軸:低分 | 色分最低 0.251; 圖分最低 0.057 |
| 5 | `corpus/ooxmlexport/EquationWithAboveAndBelowCaption.docx` | 1/1 | 1.00 | 1.00 | 12 | 真缺字:真缺陷候選 | LO 有實體文字未發射（8 個，例：['x+a', 'k=0', 'k)x']） |
| 6 | `corpus/ooxmlexport/EquationAsScientificNumbering.docx` | 1/1 | 0.33 | 1.00 | 7 | 真缺字:真缺陷候選 | LO 有實體文字未發射（4 個，例：['x+a', 'k=0', 'k)x']） |
| 7 | `corpus/ooxmlimport/tdf115719b.docx` | 2/1 | 1.00, 0.00 | 1.00, 0.00 | 0 | 色彩軸:低分<br>圖片軸:低分 | 色分最低 0.000; 圖分最低 0.000 |
| 8 | `corpus/ooxmlexport/tdf127814.docx` | 2/2 | 1.00, 0.00 | 1.00, 0.00 | 2 | 真缺字:真缺陷候選<br>色彩軸:低分<br>圖片軸:低分 | LO 有實體文字未發射（2 個，例：['hónap', 'napjai']）; 色分最低 0.000; 圖分最低 0.000 |
| 9 | `corpus/poi/test-data_document_61745.docx` | 1/1 | 0.00 | 0.00 | 13 | 真缺字:真缺陷候選<br>色彩軸:低分<br>圖片軸:低分 | LO 有實體文字未發射（2 個，例：['Category', 'Series']）; 色分最低 0.000; 圖分最低 0.000 |
| 10 | `corpus/poi/test-data_document_bug59058.docx` | 52/36 | 0.00, 1.00, 1.00, 0.00, 1.00, 1.00 | 0.00, 1.00, 1.00, 0.00, 1.00, 1.00 | 885 | 真缺字:真缺陷候選<br>色彩軸:低分<br>圖片軸:低分 | LO 有實體文字未發射（877 個，例：['Fig', 'compare', 'right']）; 色分最低 0.000; 圖分最低 0.000 |
| 11 | `corpus/ooxmlexport/fdo78420.docx` | 23/20 | 0.86, 0.42, 0.39, 0.41, 0.30, 0.07 | 0.04, 0.00, 0.00, 0.07, 0.14, 0.00 | 5 | 真缺字:真缺陷候選<br>色彩軸:低分<br>圖片軸:低分 | LO 有實體文字未發射（2 個，例：['quarry/building-', 'rev/min']）; 色分最低 0.068; 圖分最低 0.000 |
| 12 | `corpus/ooxmlimport/tdf126114.docx` | 1/1 | 1.00 | 0.00 | 0 | 圖片軸:低分 | 圖分最低 0.000 |
| 13 | `corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` | 1/1 | 0.07 | 1.00 | 0 | 色彩軸:低分 | 色分最低 0.072 |
| 14 | `corpus/ooxmlexport/2129393649.docx` | 3/3 | 0.00, 0.00, 0.00 | 0.00, 0.00, 0.00 | 1 | 真缺字:真缺陷候選<br>色彩軸:低分<br>圖片軸:低分 | LO 有實體文字未發射（1 個，例：['C:\\Users\\me\\AppData\\Local\\Temp\\quire-render-lab\\ooxmlexport__2129393649.docx']）; 色分最低 0.000; 圖分最低 0.000 |
| 15 | `corpus/ooxmlimport/testfdo90720.docx` | 1/1 | 0.00 | 0.00 | 0 | 色彩軸:低分<br>圖片軸:低分 | 色分最低 0.000; 圖分最低 0.000 |

## 2. 全 30 檔掃描完整清單

| 序號 | 檔案 | 狀態 | 頁數 | 內容分 | 版面分 | 色分 | 圖分 | 真缺字 | 旗標 |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `corpus/ooxmlexport/fdo61343.docx` | ok | 1/1 | 1.00 | 0.98 | 0.26 | 0.23 | 0 | 🚩 |
| 2 | `corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Sun.docx` | ok | 2/2 | 0.87, 0.86 | 0.80, 0.63 | 0.84, 0.73 | 0.75, 0.00 | 3 | 🚩 |
| 3 | `corpus/ooxmlexport/tdf115212.docx` | ok | 1/1 | 0.78 | 0.78 | 0.02 | 0.00 | 1 | 🚩 |
| 4 | `corpus/ooxmlimport/vml-adjustments.docx` | ok | 1/1 | — | — | 1.00 | 1.00 | 0 | ✓ |
| 5 | `corpus/ooxmlexport/tdf120511_eatenSection.docx` | ok | 2/2 | 1.00, 1.00 | 1.00, 1.00 | 1.00, 1.00 | 1.00, 1.00 | 0 | ✓ |
| 6 | `corpus/ooxmlexport/simple-sdts.docx` | ok | 1/1 | 1.00 | 1.00 | 1.00 | 1.00 | 0 | ✓ |
| 7 | `corpus/poi/test-data_document_58067.docx` | ok | 1/1 | 1.00 | 0.98 | 0.25 | 0.06 | 0 | 🚩 |
| 8 | `corpus/ooxmlexport/shape-in-floattable.docx` | ok | 1/1 | 1.00 | 1.00 | 1.00 | 1.00 | 0 | ✓ |
| 9 | `corpus/ooxmlexport/EquationWithAboveAndBelowCaption.docx` | ok | 1/1 | 0.80 | 0.29 | 1.00 | 1.00 | 12 | 🚩 |
| 10 | `corpus/selfgen/patho_nested_table_3deep.docx` | ok | 1/1 | 0.62 | 0.82 | 1.00 | 1.00 | 0 | ✓ |
| 11 | `corpus/ooxmlexport/EquationAsScientificNumbering.docx` | ok | 1/1 | 0.78 | 0.66 | 0.33 | 1.00 | 7 | 🚩 |
| 12 | `corpus/selfgen/patho_table_across_pages.docx` | ok | 2/2 | 0.99, 0.98 | 1.00, 0.74 | 1.00, 1.00 | 1.00, 1.00 | 0 | ✓ |
| 13 | `corpus/phpword/tests_PhpWordTests__files_documents_reader-2011.docx` | ok | 1/1 | 1.00 | 1.00 | 0.97 | 0.98 | 0 | ✓ |
| 14 | `corpus/ooxmlimport/textbox-wps-only.docx` | ok | 1/1 | 1.00 | 1.00 | 1.00 | 1.00 | 0 | ✓ |
| 15 | `corpus/ooxmlimport/tdf115719b.docx` | ok | 2/1 | 0.39, 0.00 | 0.38, 0.00 | 1.00, 0.00 | 1.00, 0.00 | 0 | 🚩 |
| 16 | `corpus/ooxmlexport/tdf127814.docx` | ok | 2/2 | 0.90, 0.90 | 0.82, 0.64 | 1.00, 0.00 | 1.00, 0.00 | 2 | 🚩 |
| 17 | `corpus/poi/test-data_document_52449.docx` | ok | 1/1 | 1.00 | 0.64 | 1.00 | 1.00 | 0 | ✓ |
| 18 | `corpus/ooxmlexport/tdf119054.docx` | ok | 3/1 | 0.00, 0.00, 0.00 | 0.00, 0.00, 0.00 | 1.00, 1.00, 1.00 | 1.00, 1.00, 1.00 | 0 | ✓ |
| 19 | `corpus/poi/test-data_document_61745.docx` | ok | 1/1 | 0.00 | 0.00 | 0.00 | 0.00 | 13 | 🚩 |
| 20 | `corpus/poi/test-data_document_bug59058.docx` | ok | 52/36 | 0.52, 0.05, 0.02, 0.08, 0.05, 0.18, 0.07, 0.10, 0.10, 0.15, 0.15, 0.09, 0.11, 0.17, 0.15, 0.12, 0.11, 0.15, 0.13, 0.09, 0.09, 0.06, 0.06, 0.13, 0.12, 0.19, 0.11, 0.13, 0.15, 0.13, 0.08, 0.06, 0.14, 0.15, 0.15, 0.07, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00 | 0.98, 0.26, 0.17, 0.21, 0.06, 0.25, 0.17, 0.13, 0.15, 0.17, 0.12, 0.08, 0.15, 0.22, 0.14, 0.19, 0.21, 0.15, 0.16, 0.20, 0.07, 0.14, 0.12, 0.21, 0.15, 0.14, 0.19, 0.16, 0.16, 0.13, 0.15, 0.07, 0.12, 0.20, 0.24, 0.13, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00 | 0.00, 1.00, 1.00, 0.00, 1.00, 1.00 | 0.00, 1.00, 1.00, 0.00, 1.00, 1.00 | 885 | 🚩 |
| 21 | `corpus/ooxmlimport/tdf115094.docx` | ok | 1/1 | 1.00 | 0.94 | 1.00 | 1.00 | 0 | ✓ |
| 22 | `corpus/ooxmlexport/fdo78420.docx` | ok | 23/20 | 0.83, 0.76, 0.63, 0.44, 0.29, 0.30, 0.19, 0.15, 0.09, 0.15, 0.16, 0.14, 0.11, 0.15, 0.11, 0.20, 0.23, 0.16, 0.17, 0.17, 0.00, 0.00, 0.00 | 0.72, 0.22, 0.11, 0.22, 0.20, 0.20, 0.15, 0.21, 0.20, 0.09, 0.20, 0.24, 0.28, 0.25, 0.27, 0.23, 0.28, 0.14, 0.19, 0.08, 0.00, 0.00, 0.00 | 0.86, 0.42, 0.39, 0.41, 0.30, 0.07 | 0.04, 0.00, 0.00, 0.07, 0.14, 0.00 | 5 | 🚩 |
| 23 | `corpus/ooxmlimport/tdf122224.docx` | ok | 1/1 | 0.53 | 0.54 | 1.00 | 1.00 | 0 | ✓ |
| 24 | `corpus/poi/test-data_document_FieldCodes.docx` | ok | 1/1 | 1.00 | 1.00 | 1.00 | 1.00 | 0 | ✓ |
| 25 | `corpus/ooxmlimport/tdf126114.docx` | ok | 1/1 | 1.00 | 1.00 | 1.00 | 0.00 | 0 | 🚩 |
| 26 | `corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` | ok | 1/1 | 0.71 | 0.86 | 0.07 | 1.00 | 0 | 🚩 |
| 27 | `corpus/ooxmlexport/n779642.docx` | ok | 1/1 | 1.00 | 0.17 | 1.00 | 1.00 | 0 | ✓ |
| 28 | `corpus/ooxmlexport/2129393649.docx` | ok | 3/3 | 0.83, 0.81, 0.47 | 0.88, 0.88, 0.88 | 0.00, 0.00, 0.00 | 0.00, 0.00, 0.00 | 1 | 🚩 |
| 29 | `corpus/ooxmlexport/fdo78663.docx` | ok | 1/1 | — | — | 1.00 | 1.00 | 0 | ✓ |
| 30 | `corpus/ooxmlimport/testfdo90720.docx` | ok | 1/1 | 1.00 | 1.00 | 0.00 | 0.00 | 0 | 🚩 |