# X1a：corpus-xlsx 80 檔 cell 級普查報告

任務卡：`X1a corpus-xlsx 80 檔 cell 級普查（report-only，零引擎代碼）`

本調查只讀 `corpus-xlsx/`，產出 `docs/xlsx-survey/survey.json` 與本報告，未修改 `core/`、`shells/`、`tests/`、`Cargo.toml` 或任何語料檔。重跑方式：於 worktree 根目錄執行 `python3 scripts/xlsx_survey.py`，腳本會依 `corpus-xlsx/manifest.json` 的順序重新產生 `docs/xlsx-survey/survey.json`，輸出為冪等。

## 1. 全域結果

本批 80 檔的 manifest 與實體檔完全對應，普查共掃到 256 個 workbook sheet 條目，其中 252 個 worksheet part、4 個 chartsheet；實際 `<c>` cell 總數 337575。

| 型別 | cell 數 | 有該型別的檔案數 | 說明 |
|---|---:|---:|---|
| 缺省或 `n` | 282879 | 78 | 絕大多數 cell 不寫 `t`，本調查一律歸入 `n` |
| `s` | 48778 | 73 | sharedStrings 索引 |
| `inlineStr` | 8 | 2 | 只有 2 檔使用，樣本少但關鍵 |
| `b` | 410 | 4 | 布林值 |
| `e` | 498 | 9 | 錯誤值 |
| `str` | 5002 | 14 | 公式快取字串 |
| 其他未見值 | 0 | 0 | 80 檔沒有出現六類以外的 `t` 值 |

sharedStrings 覆蓋 74 檔，共 26138 條 `si`，其中 121 條是含 `<r>` 的 rich text，分布於 17 檔。全部 cell 中有 202401 個空 cell（`<c>` 無子元素），佔 59.96%；`t` 屬性缺省者共 280894 個。`spans` 屬性只出現在 `<row>`，51 檔有使用，`<c>` 上的 `spans` 在整批 corpus 中為 0。

## 2. cell 型別考卷建議

單檔型別覆蓋最廣的是 `poi/test-data_spreadsheet_46535.xlsx`，同時有 `n`、`s`、`b`、`e`、`str` 五型，實測 `b` 389 個、`e` 331 個、`str` 3 個、`s` 6426 個，是 X1b 布林、錯誤值與公式快取字串的首選考卷。

`inlineStr` 只在兩檔出現：`poi/test-data_spreadsheet_InlineStrings.xlsx` 有 5 個 `inlineStr`、11 個 `s`、12 個 `n`，且 `<is>` 與一般 `<t>` 並存；`poi/test-data_spreadsheet_xlsx-jdbc.xlsx` 有 3 個 `inlineStr` 與 3 個 `s`，檔案極小（3072 bytes），適合當最小回歸樣本。兩檔都沒有 rich inline run（`<is>` 內含 `<r>`），若要覆蓋 inline rich run，需另建 fixture 或在 X1b 以程式路徑測試，不能從本 corpus 取得真實樣本。

各型別的密集樣本如下，皆可作為對照組：`b` 以 `poi/test-data_spreadsheet_46535.xlsx`（389 個）為主，次為 `poi/test-data_spreadsheet_ConditionalFormattingSamples.xlsx`（18 個）；`e` 以 `poi/test-data_spreadsheet_46535.xlsx`（331 個）與 `lo/sc_qa_unit_data_xlsx_forum-mso-en4-134670.xlsx`（103 個）為主；`str` 以 `poi/test-data_spreadsheet_StructuredRefs-lots-with-lookups.xlsx`（2174 個）與 `lo/sc_qa_unit_data_xlsx_tdf171828_fail_to_import_file.xlsx`（1918 個）為主；`s` 以 `lo/sc_qa_unit_data_xlsx_forum-mso-en4-134670.xlsx`（9784 個）與 `poi/test-data_spreadsheet_StructuredRefs-lots-with-lookups.xlsx`（9225 個）為主。

小型組合考卷可用 `lo/sc_qa_unit_data_xlsx_formats.xlsx`（40 cells：`n` 20、`s` 18、`b` 2，另有 1 條 rich `si`）與 `poi/test-data_spreadsheet_60512.xlsm`（1239 cells：`n` 1007、`s` 211、`e` 3、`str` 18）。若只挑兩檔覆蓋全部六型，建議 `poi/test-data_spreadsheet_46535.xlsx` 加 `poi/test-data_spreadsheet_InlineStrings.xlsx`。

## 3. dimension 矛盾考卷建議

38 檔有至少一個 sheet 的 dimension 與實際 cell 外框不一致。依不一致類型拆分，sheet 層級共 55 個不一致 sheet：宣告大於實際 28 個、宣告 A1 但沒有任何 cell 21 個、移位 1 個、缺少 dimension 4 個、實際超出宣告 1 個。真正的「紅燈」是實際 cell 超出宣告範圍，這種檔案只有兩個，應優先納入 X1b。

| 考卷 | dimension 宣告 | 實際 cell 外框 | 類型 |
|---|---|---|---|
| `lo/sc_qa_unit_data_xlsx_tdf113271.xlsx` | `A1:F5` | `A1:F462` | `actual_outside_declared`，實際多出 457 列 |
| `poi/test-data_spreadsheet_1_NoIden.xlsx` | `B1:D9` | `B2:F9` | `shifted`，起點下移且右側多出 E、F 欄 |
| `poi/test-data_spreadsheet_64450.xlsx` | 無 dimension | `A1:H2` 與空 sheet | 缺少 dimension 宣告 |
| `poi/test-data_spreadsheet_LIBRE_OFFICE-128382-0.xlsx` | 無 dimension | `A1:H26`、`A1:F16` | 缺少 dimension 宣告 |

另一類是宣告遠大於實際，雖不是「實際越界」，但可測 X1b 是否誤把 dimension 當成實際 cell 數量。`lo/sc_qa_unit_data_xlsx_tdfSheetProts.xlsx` 的 sheet2 宣告 `A1:F1048576`，實際 cell 只到 `A1:F7`，且 XML 真的有一列 `r="1048576"` 的空 row；`lo/sc_qa_unit_data_xlsx_tdf137091.xlsx` 宣告 `A1:LE1048576`、實際 `A1:LE52`；`lo/sc_qa_unit_data_xlsx_tdf170298.xlsx` 宣告 `A1:AP1048576`、實際 `A1:AP7`。溫和一點的樣本有 `poi/test-data_spreadsheet_57181.xlsm`（`A1:BE264` 對 `A1:BE263`）、`lo/sc_qa_unit_data_xlsm_tdf111974.xlsm`（`A1:B60` 對 `A1:B32`）、`lo/sc_qa_unit_data_xlsx_cool14390_chart_external_ref.xlsx`（`A1:AP88` 對 `A1:AP77`）。

dimension 還有一種合法的非 cell 範圍寫法：`lo/sc_qa_unit_data_xlsx_tdf100034.xlsx` 的 sheet1 宣告 `1:5`、sheet2 宣告 `1:102`，都是只有列範圍、沒有欄範圍。實際 cell 外框分別是 `A1:AMJ5` 與 `A1:AMJ102`，依宣告的列範圍核對是一致的；X1b 的 dimension 解析器必須能接受這種 row range，不能假設 `:` 兩側一定是 cell 座標。

宣告 `A1` 但完全沒有 cell 的空 sheet 也是常見樣態，共 21 個 sheet，例如 `poi/test-data_spreadsheet_57181.xlsm` 的 sheet5 至 sheet7、`lo/sc_qa_unit_data_xlsx_database.xlsx` 的 sheet3、`lo/sc_qa_unit_data_xlsx_universal-content.xlsx` 的 sheet2、`poi/test-data_spreadsheet_clone_sheet.xlsx` 的 sheet2 與 sheet3。這類 sheet 不應被當成「有一個 A1 cell」，也不應讓 dimension 反推 cell 數量時憑空生出 cell。

## 4. sharedStrings 病態與懸空引用

本批 80 檔沒有發現真正的懸空引用：沒有檔案同時出現 `t="s"` 與 sharedStrings 缺失，也沒有任何 `t="s"` 的索引越界；`malformed_shared_string_cells`（`t="s"` 但 `<v>` 缺失或非整數）也是 0。也就是說，X1b 的懸空引用防禦無法直接從本 corpus 取得真實紅燈，必須靠合成 fixture 或單元測試鎖住。

sharedStrings part 缺失的檔案有 6 檔：`lo/sc_qa_unit_data_xlsx_tdf134769.xlsx`、`lo/sc_qa_unit_data_xlsx_tdf130959.xlsx`、`poi/test-data_spreadsheet_npe.xlsx`、`poi/test-data_spreadsheet_WithTwoCharts.xlsx`、`poi/test-data_spreadsheet_WithThreeCharts.xlsx`、`poi/test-data_spreadsheet_61652.xlsx`。這 6 檔的 `s` cell 數都是 0，因此是「part 缺失但無人引用」的防禦考卷，適合驗證解析器不要在找不到 sharedStrings 時直接爆掉。

sharedStrings 存在但完全無人引用的檔案為 0；不過有 4 檔存在部分未被引用的 `si`，可測「多餘條目不得影響索引」與除錯統計：`poi/test-data_spreadsheet_1_NoIden.xlsx` 有 1/15 條未引用、`poi/test-data_spreadsheet_64450.xlsx` 有 5/13 條、`lo/sc_qa_unit_data_xlsx_cool14390_chart_external_ref.xlsx` 有 12/46 條、`lo/sc_qa_unit_data_xlsx_tdf113271.xlsx` 有 18/177 條未引用（索引 0 至 14 等）。

## 5. 特殊結構與病態標記

空 cell 是本批最常見的結構，`lo/sc_qa_unit_data_xlsx_tdf130959.xlsx` 的 4281 個 cell 全部是空 cell（只有樣式、沒有值），是「格式化空 cell 不得產生文字」的極端樣本；`poi/test-data_spreadsheet_CVLKRA-KYC_Download_File_Structure_V3.1.xlsx` 有 49300/52287 個空 cell，`lo/sc_qa_unit_data_xlsm_tdf111974.xlsm` 有 49222/52280 個，`poi/test-data_spreadsheet_57181.xlsm` 有 42791/55245 個。

跳號 row 共 36 檔。常規樣本有 `lo/sc_qa_unit_data_xlsx_tdf171828_fail_to_import_file.xlsx`（缺 60 個列號）、`lo/sc_qa_unit_data_xlsm_tdf111974.xlsm`（缺 119 個）、`lo/sc_qa_unit_data_xlsx_orderOfCNumFmtElements.xlsx`（缺 53 個）、`poi/test-data_spreadsheet_45540_classic_Header.xlsx`（缺 43 個）。極端樣本是 `poi/test-data_spreadsheet_clusterfuzz-testcase-minimized-POIXSSFFuzzer-5937385319563264.xlsx`，其 sheet8 出現 `r="231999999999940"` 的 row，與前一列相差超過 2.3 乘 10 的 14 次方，但 dimension 仍只宣告 `A1:K23`，是列號溢位與跳號防禦的現成素材。

`spans` 屬性只出現在 row 層級，使用最密集的是 `poi/test-data_spreadsheet_StructuredRefs-lots-with-lookups.xlsx`（5034 個 row）、`poi/test-data_spreadsheet_dataValidationTableRange.xlsx`（3164 個）、`poi/test-data_spreadsheet_46535.xlsx`（3141 個）。`<is>` 的 rich run 在整批 corpus 中為 0；rich text 只存在於 sharedStrings，最大樣本是 `poi/test-data_spreadsheet_45540_classic_Header.xlsx`、`poi/test-data_spreadsheet_45540_classic_Footer.xlsx`、`poi/test-data_spreadsheet_45544.xlsx` 各 19 條 rich `si`，以及 `lo/sc_qa_unit_data_xlsx_cool14390_chart_external_ref.xlsx` 的 12 條。

讀取層有一個現成病態：`poi/test-data_spreadsheet_clusterfuzz-testcase-minimized-POIXSSFFuzzer-5937385319563264.xlsx` 的 `xl/worksheets/sheet8.xml` CRC 損壞，`zipfile` 直接讀會拋 `BadZipFile`。本次腳本以 raw DEFLATE 補讀成功，普查仍完整；X1b 若遇到 CRC 錯誤，應明確回報而非默默吞掉，並決定是否採寬容解壓。

## 6. 建議 X1b 必蓋 case 清單

| 必蓋 case | 檔案證據 | 備註 |
|---|---|---|
| 六種 `t` 全覆蓋 | `poi/test-data_spreadsheet_46535.xlsx` 加 `poi/test-data_spreadsheet_InlineStrings.xlsx` | 前者 `n/s/b/e/str`，後者 `inlineStr` |
| 最小 `inlineStr` | `poi/test-data_spreadsheet_xlsx-jdbc.xlsx` | 3 個 inlineStr，3072 bytes |
| 布林與錯誤值 | `poi/test-data_spreadsheet_46535.xlsx` | `b` 389、`e` 331 |
| 公式快取字串 | `poi/test-data_spreadsheet_StructuredRefs-lots-with-lookups.xlsx` | `str` 2174 |
| sharedStrings rich text | `poi/test-data_spreadsheet_45540_classic_Header.xlsx` | 19/147 條 rich `si`，共 38 個 run |
| inline rich run | 無真實樣本 | 本批 `<is>` rich run 為 0，須另建 fixture |
| dimension 實際越界 | `lo/sc_qa_unit_data_xlsx_tdf113271.xlsx` | `A1:F5` 對實際 `A1:F462` |
| dimension 錯位 | `poi/test-data_spreadsheet_1_NoIden.xlsx` | `B1:D9` 對實際 `B2:F9` |
| 缺 dimension | `poi/test-data_spreadsheet_64450.xlsx`、`poi/test-data_spreadsheet_LIBRE_OFFICE-128382-0.xlsx` | 共 4 個 sheet 無宣告 |
| row-only dimension | `lo/sc_qa_unit_data_xlsx_tdf100034.xlsx` | `1:5`、`1:102`，實際到 AMJ 欄 |
| dimension 超大範圍 | `lo/sc_qa_unit_data_xlsx_tdfSheetProts.xlsx`、`lo/sc_qa_unit_data_xlsx_tdf137091.xlsx`、`lo/sc_qa_unit_data_xlsx_tdf170298.xlsx` | 宣告到 1048576 列，實際只有數十列 |
| 宣告 A1 的空 sheet | `poi/test-data_spreadsheet_57181.xlsm`、`lo/sc_qa_unit_data_xlsx_database.xlsx` | 不得憑 dimension 生出 cell |
| 全空 cell sheet | `lo/sc_qa_unit_data_xlsx_tdf130959.xlsx` | 4281 個 cell 全為空 cell |
| 一般跳號 row | `lo/sc_qa_unit_data_xlsx_tdf171828_fail_to_import_file.xlsx` | 缺 60 個列號 |
| 極端列號 | `poi/test-data_spreadsheet_clusterfuzz-testcase-minimized-POIXSSFFuzzer-5937385319563264.xlsx` | row `r=231999999999940` |
| `spans` 密集 | `poi/test-data_spreadsheet_StructuredRefs-lots-with-lookups.xlsx` | 5034 個 row 有 spans |
| sharedStrings 缺失 | `lo/sc_qa_unit_data_xlsx_tdf134769.xlsx` | part 不存在且無 `s` cell |
| sharedStrings 部分未引用 | `lo/sc_qa_unit_data_xlsx_tdf113271.xlsx` | 18/177 條未引用 |
| chartsheet 與多 sheet | `poi/test-data_spreadsheet_WithChartSheet.xlsx`、`poi/test-data_spreadsheet_CVLKRA-KYC_Download_File_Structure_V3.1.xlsx` | 後者 30 個 worksheet |
| xlsm 與一般 xlsx 同路徑 | `poi/test-data_spreadsheet_57181.xlsm` | 與 xlsx 共用 worksheet 解析 |
| CRC 損壞 part | `poi/test-data_spreadsheet_clusterfuzz-testcase-minimized-POIXSSFFuzzer-5937385319563264.xlsx` | sheet8 的 CRC 與資料不一致 |

## 7. 自抽查驗過程

自抽查驗以 `shuf` 從 80 檔中隨機抽出 3 檔，直接用 `unzip -p` 取出 worksheet XML，再以 `grep -o` 計數 `<c>`、各 `t` 屬性、`<row>`、`spans`、`<dimension ref>` 與 sharedStrings 的 `<si>`，最後與 `survey.json` 對照。抽查指令的形式如下，實際執行時逐 sheet 套用：

```sh
unzip -p corpus-xlsx/<file> xl/worksheets/sheetN.xml | grep -o '<c[ >]' | wc -l
unzip -p corpus-xlsx/<file> xl/worksheets/sheetN.xml | grep -o 't="s"' | wc -l
unzip -p corpus-xlsx/<file> xl/sharedStrings.xml | grep -o '<si[ >]' | wc -l
```

隨機抽中的 3 檔與對照結果如下，`survey.json` 的數字與手抽完全一致：

| 檔案 | 手抽 worksheet 合計 | survey.json | 手抽 sharedStrings | survey.json |
|---|---|---|---|---|
| `poi/test-data_spreadsheet_45540_classic_Header.xlsx` | 7 sheet、923 cells、`s` 226、row 222、spans row 222 | 7 worksheet、923 cells、`s` 226、row 222、spans row 222 | 147 `si`、19 條含 `<r>` | 147 `si`、19 條 rich |
| `lo/sc_qa_unit_data_xlsx_formats.xlsx` | 4 sheet、40 cells、`s` 18、`b` 2、si 18、rich 1 | 4 worksheet、40 cells、`s` 18、`b` 2、si 18、rich 1 | 18 `si`、1 條含 `<r>` | 18 `si`、1 條 rich |
| `lo/sc_qa_unit_data_xlsx_tdfSheetProts.xlsx` | 4 sheet、92 cells、`s` 30、si 11 | 4 worksheet、92 cells、`s` 30、si 11 | 11 `si`、0 條含 `<r>` | 11 `si`、0 條 rich |

其中 `lo/sc_qa_unit_data_xlsx_formats.xlsx` 的手抽同時驗到 dimension 無矛盾（4 個 sheet 的宣告與實際外框一致），以及 sheet3 的 row 跳號 1 個；`lo/sc_qa_unit_data_xlsx_tdfSheetProts.xlsx` 的手抽驗到 sheet2 宣告 `A1:F1048576`、實際 cell 外框 `A1:F7`，且該 sheet 的 row 清單包含 `r="1048576"` 的空 row，與 survey.json 標記的 `declared_larger_than_actual` 相符。

驗收當日的最終重跑指令為 `python3 scripts/xlsx_survey.py`，重跑後 `survey.json` 仍為 80 筆 record、cell 總數 337575；另以 `git diff --name-only` 核對，變更只落在 `scripts/xlsx_survey.py` 與 `docs/xlsx-survey/`，`core/`、`shells/`、`tests/`、`Cargo.toml`、`corpus-xlsx/` 均未被修改。

## 8. survey.json 欄位導讀

`files` 陣列每檔一筆 record，固定欄位包含：`file`、`size`、`sheet_count`（workbook sheet 條目，含 chartsheet）、`worksheet_count`、`chartsheet_count`、`shared_strings`（`present`、`count`、`rich_text_count`、`rich_run_count`）、`cells`（`total`、`by_type`、`other_types`、`t_missing`、`empty`、`spans`、`inline_rich`）、`structure`（`rows`、`row_gap_sheets`、`missing_row_numbers`）、`dimension`（`mismatch`、`mismatch_kind_counts`、`actual_outside_declared`、`mismatch_sheets`、`missing_dimension_sheets`）、`pathology`（懸空引用、未引用 sharedString、缺 dimension、讀取錯誤）與 `flags`。`sheets` 陣列保留每個 worksheet 的 dimension 宣告、實際外框、不一致類型、row 跳號與 cell 型別計數，方便後續卡片直接定位 sheet。

總結統計放在 `summary`：`cell_totals_by_type` 是各型別總數，`files_with_cell_type` 是有該型別的檔案數，`dimension_mismatch_files` 與 `dimension_actual_outside_declared_files` 是 dimension 問題清單，`dangling_shared_string_files`、`unused_shared_string_files`、`shared_strings_missing_files`、`read_error_files`、`row_gap_files`、`spans_files`、`inline_rich_files` 是各類病態與特徵清單，`pathology_files` 則把每檔的病態原因彙整成單一清單。

## 9. 已知限制

dimension 交叉核對只使用 `<c r="...">` 的實際座標，不把只有 `<row>` 沒有 cell 的列算進實際外框，因此「宣告大於實際」有時是因為檔案保留了結尾的空 row 或空格式化列，而非解析錯誤；本報告已用 `actual_outside_declared` 與 `declared_larger_than_actual` 區分真正的越界與單純宣告過大。row 跳號的 `missing_row_numbers` 是範圍差聯集，遇到 `r="1048576"` 這類合法但稀疏的列號會得到很大的數字，解讀時應搭配 `row_gap_count` 與實際 row 清單，不應當成真的有一百萬列缺漏。sharedStrings 的「未引用」只統計檔案內 `t="s"` 的索引，不處理公式或外部連結間接引用；本批沒有發現間接引用造成的誤判樣本。
