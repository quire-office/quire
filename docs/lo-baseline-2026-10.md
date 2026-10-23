# LO 收斂基線（K4m 度量修正版）

- 本版生成：2026-10-23（**度量修正版**：修 `scripts/render_lab.py` 0/0 偽影 + `scripts/lo_scoreboard.py` None 聚合口徑）；seed=20261022；n=25；LO 轉換失敗 0
- LO：`LibreOffice 25.8.7.3 30742500f2d3eb4366ac312fa33d3dcabdb3eba5`（worktree 內相對路徑 `out/lo/root/opt/libreoffice25.8/program/soffice`）
- 腳本：`scripts/lo_scoreboard.py`（`render_lab.compare` 逐頁度量）；指標定義同 `out/lo-scoreboard/baseline.md`
- 每檔內容分 = 該檔各頁 `content_scores` **有效值**平均（`None` 頁＝量測不可比，跳過不計）；分層平均 = 層內**有效**檔案等權平均。
- **新值即後續 `--check` 的預設基線來源**（`out/lo-scoreboard/scores.prev.jsonl`，seed 固定可重跑）。

## 修正內容（只動 0/0 路徑）

| 位置 | 修正前 | 修正後 |
|---|---|---|
| `render_lab.compare` | 雙方皆抽不到文字 ⇒ 內容/版面分 `0.0`（判全錯） | 雙空且 LO 該頁有**內文型圖形實據**（`get_drawings()`；或非逐頁重複的 `get_images()`）⇒ `None`，記 `notes: measurement_artifact:LO有圖形無可抽文字` |
| 雙空且 LO 無內文圖形 | `0.0` | 仍 `0.0`，記 `empty_page_mismatch`（真事故） |
| 單邊有字、另一邊空 | `0.0` | **不變** `0.0`（n751077 p2 真事故，防過度修正） |
| `lo_scoreboard` 聚合 | `sum/len`（None 會崩或誤計） | None 頁／整檔全 None 跳過；整檔全 None ⇒ 標 `measurement_artifact`，**不入分層平均、不入最差榜**，另列不可比清單 |

> 重複判定：同一 image xref 在 LO 多頁出現＝頁首／頁尾浮水印，不算該頁內容；因此 n751077 p2（只有重複頁首圖、無 drawing）維持 `0.0`。

## 總平均（新 → 舊）

| 指標 | 本次（度量修正版） | 純口徑修正前（同引擎，三檔當 0.0） | 前版文件（2026-10-23，舊引擎） |
|---|---:|---:|---:|
| 檔案等權內容平均 | **0.815** | 0.717 | 0.705 |
| 檔案等權版面平均 | **0.737** | 0.649 | 0.649 |
| 頁數一致率 | 92.0% | 92.0% | 92.0% |
| 頁等權內容平均 | **0.721（46 頁）** | 0.677（49 頁） | 0.668（47 頁） |

- 「純口徑修正前」＝同一引擎、只把三檔 `None` 還原為 `0.0` 重算，可隔離度量修正的純效果：**內容 +0.098（0.717→0.815）**，全是三檔退場的貢獻。
- 「前版文件」為先前舊引擎之公開值，逐檔不可直接比（引擎已前進，見下方說明）；列此供歷史對照。

## 分層平均（新 → 舊）

| 層 | n（新/純口徑修正前） | 內容（新 / 純口徑修正前 / 前版文件） | 版面（新 / 純口徑修正前 / 前版文件） | 頁數一致（新 / 純口徑修正前 / 前版） |
|---|---:|---|---|---|
| 真實檔 | 12 / 15 | **0.736** / 0.589 / 0.581 | **0.664** / 0.531 / 0.533 | 83.3% / 86.7% / 86.7% |
| 自製 | 6 / 6 | 0.901 / 0.901 / 0.869 | 0.860 / 0.860 / 0.854 | 100% / 100% / 100% |
| 含表格 | 8 / 9 | **0.813** / 0.723 / 0.693 | **0.746** / 0.663 / 0.655 | 100% / 100% / 100% |
| 含圖 | 8 / 11 | **0.666** / 0.484 / 0.477 | **0.581** / 0.423 / 0.426 | 75.0% / 81.8% / 81.8% |
| 含尾註 | 4 / 4 | 0.907 / 0.907 / 0.912 | 0.789 / 0.789 / 0.795 | 100% / 100% / 100% |
| 含多節 | 6 / 6 | 0.856 / 0.856 / 0.857 | 0.766 / 0.766 / 0.766 | 100% / 100% / 100% |

- **含圖層 0.477→0.666**：全數來自三個 0.000 偽影檔退場（n＝11→8），非圖形發射改善；真實檔／含表格層同步因偽影檔退場而抬升。此三層的實際引擎缺陷（頁首圖不發射、填色框不發射）仍未修，已改列不可比清單追蹤。
- **自製／含尾註／含多節**：新舊逐位不變（本次只動 0/0 路徑，見下方 self-proof），健康區。

## 量測不可比清單（measurement_artifact）

| 檔案 | 頁數（LO/OW） | LO 圖形實據 |
|---|---|---|
| corpus/ooxmlexport/n779834.docx | 1/1 | 第 1 頁 drawings=17, images=1, 內文圖=1（header 浮水印 EMF） |
| corpus/ooxmlexport/n780563.docx | 1/1 | 第 1 頁 drawings=3, images=0（三色列填色＋表格框） |
| corpus/ooxmlimport/tdf152200-bad_fldChar_end.docx | 1/1 | 第 1 頁 drawings=1, images=0（黑框空矩形） |

- 三檔 LO 該頁可抽取文字為 0、我方同頁亦為 0，但 LO 有非文字內容 ⇒ 內容分不可比，**不入任何平均、不入最差榜**。
- 真缺口（引擎）另計：n779834 header 圖、n780563 填色／框線、tdf152200 空矩形皆未發射（見 `docs/k4-diag-image-gaps.md`）；本卡不碰引擎。

## 最差 10 檔（2026-10-23 度量修正版）

| # | 檔案 | 內容分 | 首個內容分歧頁 | 備註 |
|---:|---|---:|---|---|
| 1 | corpus/poi/test-data_document_Bug51170.docx | 0.256 | 第 3 頁 | — |
| 2 | corpus/ooxmlexport/fdo76591.docx | 0.428 | 第 2 頁 | 第 1 頁內容近同、僅換行位置不同 |
| 3 | corpus/ooxmlimport/n751077.docx | 0.500 | 第 2 頁 | **empty_page_mismatch（LO 有內容、我方空，真事故；防偽影放水通過）** |
| 4 | corpus/ooxmlexport/tdf119800.docx | 0.595 | 第 1 頁 | — |
| 5 | corpus/poi/test-data_document_Bug54771a.docx | 0.706 | — | — |
| 6 | corpus/selfgen/patho_table_across_pages_b.docx | 0.756 | — | — |
| 7 | corpus/poi/test-data_document_bug65738.docx | 0.816 | — | — |
| 8 | corpus/ooxmlexport/fdo76163.docx | 0.846 | — | — |
| 9 | corpus/poi/test-data_document_shapes-with-text.docx | 0.849 | — | — |
| 10 | corpus/selfgen/patho_merged_interleave_b.docx | 0.852 | — | — |

- 三檔 0.000 已自最差榜消失，改列上表不可比清單。
- 版面最差分（page-weighted 口徑外）不另列；逐檔 `baseline.md` 有完整表。

## 逐檔 diff stat（新 vs 純口徑修正前，同一引擎）

- **改變 3 檔（僅 0/0 路徑）**：
  - `corpus/ooxmlexport/n779834.docx`：`[0.0] → [None]`（內容＋版面）
  - `corpus/ooxmlexport/n780563.docx`：`[0.0] → [None]`
  - `corpus/ooxmlimport/tdf152200-bad_fldChar_end.docx`：`[0.0] → [None]`
- **逐位不變 22 檔**（含 `reader-styles` `[0.987,0.887,0.877]`、`n751077` `[1.0,0.0]`、`shapes-with-text` `[0.849]` 等）。
- 驗證法：同一批 LO PDF／quire facts 同時餵舊版（`git show HEAD:scripts/render_lab.py`）與新版 `compare`，逐頁比對 → `相同 22 檔、不同 3 檔`，且不同的三檔差異全落在雙空頁。

## `--check` 自證

- 自身基線（`--check` 預設讀 `out/lo-scoreboard/scores.prev.jsonl`）：六層 Δ=+0.000、個別劣化 0、最差榜新增 0 ⇒ **exit 0**。
- 回歸情境（把 `dropcap_toc_fields` 1.0 砸到 0.1）：自製層 Δ=-0.150 FAIL、個別劣化 1 檔、最差榜新增 1 檔 ⇒ **exit 1**。
- None-safe：以舊式（`None→0.0`）scores 當基線、新式（含 `None`）當現值，跑完不崩（`None` 檔自動跳過比較）。

## 引擎漂移備註（勿與度量修正混淆）

本次重跑與前版文件相比，部分檔案分數變動來自**引擎前進**而非本次度量修正，例：Bug51170 我方 5→9 頁（0.179→0.256）、fdo76591 LO 2→1 頁。故「前版文件」欄僅供歷史對照；本卡的可比結論一律以「純口徑修正前（同引擎）」欄為準。

## PM 覆核補記

- 反向測試（n751077 p2 真事故仍 0.000）已內建於 `render_lab.py --selftest` 4c/4d，並在真實 25 檔驗證中實跑通過；量測只修偽影，不把真缺陷修沒。
- `scores.prev.jsonl` 在 gitignored 的 `out/` 下、不入 repo：基線由本報告承載（seed 固定可重跑）；`run_pipeline` 於每次跑完自動把它更新為當次 `scores.jsonl`，即「新值＝後續 `--check` 預設基線」。
