# M2-scorecard 口徑重算報告（雙空同位頁判準修正；seed=20261022 n=25）

- 卡：t_8ff55202（分支 oc/m2-scorecard，基線 b59a25e）；本版生成：2026-10-25。
- 口徑改動（只動 `scripts/`，零引擎觸碰）：`render_lab.compare()` 對**同位雙空頁**
  （LO 與我方都有該頁、LO 該頁文字/圖形/點陣圖全無）由 `0.0 empty_page_mismatch`
  改判 `None`（measurement_artifact，不進平均、不記分歧）；**我方多出頁是空頁**
  （LO 無此頁）仍 `0.0`——判別＝頁數對齊上下文（LO 頁數 > index 存在＝同位頁）。
  LO 該頁只剩重複頁首/頁尾圖（images>0）亦仍 `0.0`（4d 防放水臂原樣）。
- 佐證強化（§2.5 方法）：真空頁 notes 帶源檔 `w:sectPr` 計數（本檔 `sect_pr` 參數，
  報告可覆核）。
- 指標口徑：每檔內容分＝各頁 content_scores 有效值平均（None 頁跳過）；分層平均＝
  層內有效檔等權平均（同 `lo-baseline-2026-10.md`）。
- LO：`/mnt/c/Program Files/LibreOffice/program/soffice.exe`（Windows soffice 基線）。
- 原始輸出：`out/lo-scoreboard/scores.jsonl`（新口徑）、`scores.oldcaliber.jsonl`
  （**同一批 LO/quire facts** 餵 b59a25e 舊版 compare 的對照，gitignored）、
  `scores.prev.jsonl`（＝新值，後續 `--check` 預設基線）。

## 1. 純口徑效果隔離（同 facts 雙跑，K4m 同法）

25 檔樣本逐檔 content_scores **新舊逐位相同（changed 0/25）**，總平均/分層/最差榜
全部 Δ=+0.000。原因：**seed 20261022 的 n=25 樣本不含 tdf129888dml / tdf130120**，
而新臂只在「同位雙空且 LO 無圖形無點陣圖」時觸發——對不含此型頁的檔案零擾動，
與 selftest 4c/4d/4b2 防放水斷言一致。

- 新口徑（＝舊口徑同 facts）：檔案等權內容 **0.776**、版面 0.703、頁等權 0.742
  （45 頁）、頁數一致率 92.0%、全 None 檔 3（清單見 `baseline.md`）。
- 分層（新＝舊）：真實 0.680／自製 0.869／含表格 0.780／含圖 0.588／含尾註 0.912／
  含多節 0.773。
- 與 2026-10-23 基線（0.827/0.745）的落差來自**引擎漂移**（K4b/K4e2/K4c-image 等
  已入 main 的改動對齊 b59a25e），非本卡口徑——隔離證據即上段 changed 0/25。
  例：`fdo76591` 0.843→0.000 是引擎側真實回退（我方 p1 抽 0 行 vs LO p1 30 行，
  文字全被推到 p2、我方 p1 空而 LO 同位頁有字 ⇒ 非雙空、不歸本臂管），**舊口徑同
  facts 也跑出 [0.0, 0.0]**，與度量口徑無關，已另見 §3。

## 2. 目標兩檔重算（render_lab 直跑，LO 走 Windows soffice）

| 檔 | 舊口徑 p1/p2 | 舊檔分 | 新口徑 p1/p2 | 新檔分 |
|---|---|---:|---|---:|
| corpus/ooxmlexport/tdf129888dml.docx | 1.0 / 0.0 | 0.500 | 1.0 / **—(None)** | **1.000** |
| corpus/ooxmlexport/tdf130120.docx | 1.0 / 0.0 | 0.500 | 1.0 / **—(None)** | **1.000** |

LO p2 實據（pymupdf，重跑再現）：`lines=0 drawings=0 images=0`；源檔 `w:sectPr×2`
（段內＋body）——LO 真空頁屬結構性，K5-diag §2.5 判語成立。notes 落
`measurement_artifact:雙空同位頁(LO真空)` ＋ `源檔 w:sectPr×2`。
兩檔不在 25 檔樣本內，故儀表板總平均不動；此處單檔重算如實記錄口徑效果。

## 3. 防過度修正實證（必須仍 0.0 的案子）

- **n751077**（corpus/ooxmlimport/n751077.docx）：新口徑 content_scores
  `[0.0, 0.0, 0.0]`（LO 2 頁／我方 3 頁）——p2/p3 是我方多出空頁（LO 無此頁 ⇒
  `i >= len(lo_pages)` 走 0.0 臂），p1 是我方空而 LO 同位頁有字（走非雙空臂）。
  舊口徑同 facts `[0.0, 0.0, 0.0]`，逐位相同：**真事故沒被修沒**。
  （10/23 基線為 [1.0, 0.0]：p1 現 0.0 是引擎側浮動圖掛載漂移所致，屬 K5-diag
  #7「頁面釘定浮動框」下游案，非本卡口徑。）
- selftest 4b2（我方多出空頁＝0.0）、4c（LO 有字我方空＝0.0）、4d（LO 只剩重複
  頁首圖＝0.0）三案全綠（`python3 scripts/render_lab.py --selftest` ALL-PASS）。
- **fdo76591**（0.000，最差榜第 1）：舊/新口徑同 facts 皆 `[0.0, 0.0]`——我方 p1
  可抽行 0、LO p1 有 30 行（同位頁**非雙空**，本臂不觸發），是引擎內容分佈問題，
  本卡不放水、如實留榜。

## 4. 最差榜（新口徑，seed=20261022 n=25）

| # | 檔案 | 內容分 | 口徑影響 |
|---:|---|---:|---|
| 1 | corpus/ooxmlexport/fdo76591.docx | 0.000 | 無（引擎漂移，雙口徑同分） |
| 2 | corpus/ooxmlimport/n751077.docx | 0.000 | 無（真事故，4b2/4c 型） |
| 3 | corpus/selfgen/patho_table_across_pages_b.docx | 0.541 | 無 |
| 4 | corpus/ooxmlexport/tdf119800.docx | 0.595 | 無 |
| 5 | corpus/poi/test-data_document_shapes-with-text.docx | 0.650 | 無 |
| 6 | corpus/poi/test-data_document_Bug54771a.docx | 0.706 | 無 |
| 7 | corpus/poi/test-data_document_Bug51170.docx | 0.757 | 無 |
| 8 | corpus/poi/test-data_document_bug65738.docx | 0.766 | 無 |
| 9 | corpus/ooxmlexport/fdo76163.docx | 0.850 | 無 |
| 10 | corpus/selfgen/patho_merged_interleave_b.docx | 0.852 | 無 |

- tdf129888dml／tdf130120 **不在本樣本**，故無「從最差榜消失」可言；但它們若入樣本，
  新口徑下會從 0.5 升到 1.0——**是口徑不是引擎**（§2 表）。
- 榜上兩筆 0.000 與 10/23 版榜單的差異（Bug51170 0.256→0.757、fdo76591 0.428→0.000
  等）全屬引擎漂移，K5-diag §2 已逐檔歸因。

## 5. diag_pagination 空 LO 頁 IndexError（§4 懸#8）已閉合

`python3 scripts/diag_pagination.py corpus/ooxmlexport/tdf129888dml.docx` →
exit 0，對齊表 p2 顯示 `(LO 本頁為空——跳過)`（原 `splitlines()[0]` IndexError）。
LO 無此頁（我方多頁）另標 `(LO 無此頁)`。

## 6. 自證清單

- `scripts/render_lab.py --selftest`：10 案 ALL-PASS（含改寫 4b、新增 4b2）。
- `lo_scoreboard --check`（自身基線）：六層 Δ=+0.000、個別劣化 0、最差榜新增 0 ⇒ exit 0。
- 零引擎改動：`git diff --stat b59a25e..HEAD` 僅 `scripts/`（3 檔）；
  `cargo check -q --workspace` 通過、`cargo test -q -p quire-cli` 6/6 通過（零 Rust 改動表態）。
- 口徑變動逐案說明：見 `baseline.md` 同目錄的 diff 表與 §1–§3。
