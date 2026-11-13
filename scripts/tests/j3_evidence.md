# J3 度量升級自證數據（2026-10-29，分支 agy/j3-metrics）

## A/B：fdo68607 與 Bug51170（升級版實測）
```
→ corpus/ooxmlexport/fdo68607.docx
   LO 8 頁 / OW 8 頁，內容相似度 [0.969, 0.934, 0.906, 0.883]（版面 [0.951, 0.619, 0.399, 0.25]）
   色 [0.078, 0.061, 0.073, 0.09, 0.089, 0.08] 圖 [0.069, 0.061, 0.069, 0.082, 0.089, 0.08] 真缺字 3（斷行切片已排除 5）：['IGCSE', 'Glossary', 'Regularity']
→ corpus/poi/test-data_document_Bug51170.docx
   LO 7 頁 / OW 7 頁，內容相似度 [0.96, 0.9, 0.799, 0.818]（版面 [0.994, 0.402, 0.492, 0.32]）
   色 [1.0, 0.668, 0.668, 0.668, 0.668, 0.668] 圖 [1.0, 0.0, 0.0, 0.0, 0.0, 0.0] 真缺字 0（斷行切片已排除 0）：[]

報告 → out/j3-ab/report.md
```

判讀：
- fdo68607：色分 0.06–0.09（≈0；殘餘＝我方只畫彩色表格線）、圖分 0.06–0.09（空心框 vs 實心底紋，密度因子壓低）、真缺字＝IGCSE／Glossary／Regularity（斷行切片已排除 5 個）。
- Bug51170：p2–p7 圖分 0.0——LO 頁首圖 bbox x≈0.42–0.61（置中），我方 x≈0.18–0.37（靠左），即「圖片置中」缺陷；p1 無彩色塊＝1.0。

## n=6 抽樣（--n 6 --seed 20261029）舊欄位零漂移：升級前（HEAD 前一版）vs 升級後同欄

| 檔案 | 升級前（頁/內容/版面/分歧/備註） | 升級後同欄 | 一致 | 色 | 圖 | 真缺字 |
|---|---|---|---|---|---|---|
| corpus/ooxmlexport/picture-with-schemecolor.docx | 1 / 1 / ✓ / — / — / — / measurement_artifact:LO有圖形無可抽文字 | 1 / 1 / ✓ / — / — / — / measurement_artifact:LO有圖形無可抽文字 | ✓ | 0.0 | 0.0 | 0 |
| corpus/ooxmlexport/fdo77129.docx | 3 / 3 / ✓ / 0.773, 0.788, 0.753 / 0.765, 0.778, 0.747 / — / — | 3 / 3 / ✓ / 0.773, 0.788, 0.753 / 0.765, 0.778, 0.747 / — / — | ✓ | 1.0, 0.0, 1.0 | 1.0, 1.0, 1.0 | 1：`How........................................................................................................................................................................2` |
| corpus/xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-03.docx | 1 / 1 / ✓ / 0.843 / 0.754 / — / — | 1 / 1 / ✓ / 0.843 / 0.754 / — / — | ✓ | 0.674 | 0.564 | 2：`1/1/2014`, `notes` |
| corpus/ooxmlexport/2col-header.docx | 1 / 1 / ✓ / 0.632 / 0.609 / — / — | 1 / 1 / ✓ / 0.632 / 0.609 / — / — | ✓ | 1.0 | 1.0 | 0 |
| corpus/ooxmlimport/tdf153791-shd_overrides_fontRef.docx | 1 / 1 / ✓ / 1.0 / 1.0 / — / — | 1 / 1 / ✓ / 1.0 / 1.0 / — / — | ✓ | 0.0 | 0.0 | 0 |
| corpus/ooxmlexport/dml-picture-in-textframe.docx | 1 / 1 / ✓ / — / — / — / measurement_artifact:LO有圖形無可抽文字 | 1 / 1 / ✓ / — / — / — / measurement_artifact:LO有圖形無可抽文字 | ✓ | 0.0 | 0.0 | 0 |

零漂移判定：舊 8 欄逐檔逐字相同 ✓

已知度量雜訊：fdo77129 的『How......2』為目錄點線引導符黏成單一 token（非真缺字）。

## --selftest 輸出（尾段）
```
  PASS 2 大小寫不敏感（content=1.0）
  PASS 3 真缺段（互異填充）內容分如實掉（content=0.396 div=0）
  PASS 3b 符號佔位不分歧（content=0.981）
  PASS 4 雙空＋LO 有圖形＝None 偽影（content=None notes=['measurement_artifact:LO有圖形無可抽文字']）
  PASS 4b 同位雙空且 LO 無圖形＝None 度量假分差（tdf129888dml 型）（scores=[1.0, None] notes=['measurement_artifact:雙空同位頁(LO真空)']）
  PASS 4b2 我方多出空頁＝0.0 真事故（LO 無此頁）（scores=[1.0, 0.0] div=1 notes=['empty_page_mismatch']）
  PASS 4c LO 有字我方空＝0.0（防過度修正）（content=0.0 div=0）
  PASS 4d 重複頁首圖頁仍 0.0（n751077 p2 防偽影放水）（scores=[1.0, 0.0] div=1）
  PASS 5 色彩軸：LO 有底紋我方全白≈0（fdo68607 型）（color=0.000）
  PASS 5b 色彩軸：同圖＝1、純灰字頁 vs 白頁＝1（黑白灰不計）
  PASS 6 圖片軸：同圖＝1、缺圖＝0、錯位縮小圖 <0.5（錯位縮小=0.227）
  PASS 7 真缺字：IGCSE/glossary 真缺、sharp-edged 斷行切片不算缺（true_missing=['IGCSE', 'glossary'] split_ok=['sharp-edged']）
  PASS 7b 真缺字：LO 被切我方連著／標點大小寫差不算缺（true_missing=[]）
SELFTEST ALL-PASS
```

## test_j3_metrics.py 輸出
```
  PASS test_compare_old_fields_untouched
  PASS test_image_boxes_bbox
  PASS test_image_hollow_frame_vs_solid_fill_is_low
  PASS test_image_missing_and_misplaced_are_low
  PASS test_image_same_box_is_one
  PASS test_kill_own_lo_safe_without_powershell_or_on_error
（收場：清掉本輪殘留 LibreOffice 進程 3 個）
  PASS test_kill_own_lo_targets_only_own_profile
→ <hermes>/cache/scratch/tmpqmx09abp/a.docx
   ✗ LO 轉換失敗（記 lo_fail，繼續）

報告 → <hermes>/cache/scratch/tmpqmx09abp/out/report.md
  PASS test_main_registers_kill_own_lo_before_first_lo_call
  PASS test_report_has_j3_columns_and_header_total
  PASS test_tokens_cjk_char_level_and_bullets
  PASS test_tokens_reverse_split_and_punct_and_case
  PASS test_tokens_true_missing_vs_line_split
J3-TESTS ALL-PASS (16 cases)
```
