# K25-A：render_lab 圖片軸改「繪製」口徑——n751077 p2 由 0.0 誤判收斂為 None

> 上游診斷＝`docs/k25-empty-page-diag.md`（K25 卡，本檔只記落地與紅燈實測，
> 診斷鏈不重複）。本卡觸碰面：`scripts/render_lab.py`、`scripts/tests/`、
> `docs/`（本檔）。`core/*`、`corpus/`、`out/` **零觸碰**（§4）。

## 0. 改動（一處，函式級）

`pdf_page_facts()` 的 `images`／`image_xrefs` 由**宣告**口徑
（`page.get_images()`＝寫進該頁 `/Resources` 的圖片）改為**繪製**口徑
（只計 `page.get_image_rects(xref)` 非空者＝該頁真的有把它畫出來）。
新增私有 helper `_painted_image_xrefs()`；舊宣告值**不刪**，改存
`images_declared`／`image_xrefs_declared` 兩欄供診斷與 A/B 追溯。

`compare()` **一行未改**——四臂（4／4b／4b2／4d）判別式與 notes 全數原樣，
改的只是餵進去的 LO 事實口徑。

保守方向：`get_image_rects()` 拋異常時該 xref **仍計入**該頁內容（不放水），
與防放水臂語意同側。

## 1. 燈1：n751077 單檔實跑（真 LO＋真引擎，非合成）

`/usr/bin/python3 scripts/render_lab.py corpus/ooxmlimport/n751077.docx`
（LO＝Windows soffice.exe，我方＝`cargo run quire-cli`）：

| 欄位 | 改前（`out/lo-scoreboard/scores.jsonl`） | 改後（實跑） |
|---|---|---|
| 頁數 LO/OW | 2/2 ✓ | 2/2 ✓ |
| 內容相似度 | `[1.0, 0.0]` ⇒ 檔分 **0.500** | `[1.0, —]` ⇒ 檔分 **1.000** |
| 版面相似度 | `[1.0, 0.0]` | `[1.0, —]` |
| 首個內容分歧頁 | 第 2 頁 | **—** |
| notes | `empty_page_mismatch` | `measurement_artifact:雙空同位頁(LO真空)`＋`源檔 w:sectPr×2（雙方照實開頁⇒真空為結構性）` |
| J3 三軸 | 色 `[1.0, 1.0]`／圖 `[1.0, —]`／真缺字 0 | 色 `[1.0, 1.0]`／圖 `[1.0, —]`／真缺字 0（逐位相同，零擾動） |

`empty_page_mismatch` 標籤消失、內容分 0.500→1.000——與 K25 診斷 §1.3 的
實跑預測逐字元一致。報告實檔＝scratch `k25a-after/report.md`。

## 2. 燈2：防放水臂必仍綠（本卡核心風險）

`render_lab.py --selftest` 由 18 項增至 **20 項，ALL-PASS**：

- 既有 4c（LO 有字我方空＝0.0）／4d（手寫 dict 重複頁首圖＝0.0）**原樣綠**，
  未改一個斷言。
- **4d2（新增，燈2 要求的新增變體）**：LO 該頁**真繪製**的重複頁首圖＋我方空
  ⇒ 仍 0.0＋`empty_page_mismatch`、**不得**出現 `measurement_artifacts`。
- **4d3（新增）**：同構 PDF 但 p1 **只宣告未繪製**（兩頁共用同一 Resources
  間接物件，n751077 obj 41 精確重現）⇒ `[1.0, None]`＋雙空同位頁 artifact。
  4d2/4d3 是**同一個合成器只差 `paint_second` 旗標**——兩臂的分界就是
  「繪製與否」本身，寫錯任何一邊立刻紅。
- 手寫 dict 的 4d 對口徑變更盲（K25 卡已點名），故新鎖一律走**真實 PDF**
  （`pdf_page_facts()` 實抽）。

`scripts/tests/test_k25a_painted_ink.py`（新增檔，6 條全綠）補細部口徑鎖：

| 鎖 | 內容 |
|---|---|
| 前提鎖 | 共用 Resources 的 p1：`images==0` 而 `images_declared==1`（殭屍宣告可見），且該頁光柵**零墨跡**（獨立覆核，不靠圖庫語意） |
| 前提鎖 b | 真繪製的逐頁重複頁首圖：兩頁 `images==1` 同 xref（4d 保護對象不被誤刪） |
| 鎖1 | 真繪製重複頁首圖＋我方空 ⇒ 0.0（燈2 變體在 tests/ 層再鎖一輪） |
| 鎖2 | 殭屍宣告同位雙空 ⇒ `[1.0, None]`、無 `empty_page_mismatch`、artifact `reason=both_empty_same_position`、檔分 0.5→1.0 |
| 鎖3 | 真內容頁 p0（繪製圖＋文字）逐位不動：文字/圖/頁尺寸/內容分在兩變體下都不變——新口徑只刪殭屍宣告 |
| **回塞鎖** | 把實作還原成宣告口徑（BUGGY 態）：鎖2 **必紅**、鎖1 仍綠——證明紅燈精確命中殭屍宣告類，非連帶誤傷、非假綠 |

## 3. 燈3：全庫零誤傷 A/B（實掃，非推估）

對 `out/lo-scoreboard/scores.jsonl` **全部 25 檔**（主庫路徑，僅**讀**）各轉一次
LO PDF，同一份 PDF 同時餵舊（宣告）與新（繪製）兩套事實進**同一個** `compare()`，
我方事實（`run_quire`）兩側共用同一份，檔分取 `lo_scoreboard.file_score` 口徑。
產物＝scratch `k25a-ab/ab.log`＋`ab.json`。

| 項 | 實測 |
|---|---|
| 掃描 | 25/25 檔成功（lo_fail 0、err 0） |
| **shifted（檔分變動）** | **1 檔＝`corpus/ooxmlimport/n751077.docx`（0.5 → 1.0）** |
| 其餘 24 檔 | 內容分逐位、版面分、首個分歧頁、頁數、notes 標籤**逐位不變**（diff 計 0） |
| `empty_page_mismatch` 標籤 | 改前 1 檔（僅 n751077）⇒ 改後 **0 檔**（零誤傷、零殘留） |
| 新增 `measurement_artifacts` | 僅 n751077（`page=1, drawings=0, images=0, reason=both_empty_same_position`）；無任何檔「被拿掉」artifact |
| 劣化（新分 < 舊分） | **0 檔** |

忠實度檢查（`k25a_ab_vs_baseline.py`）：A/B 的 old 側（逐字還原的舊實作）與
**已記錄**的 `scores.jsonl` 基線逐位比對 ⇒ **25 檔 0 不一致**（分數、頁數、標籤、
檔分全等），故 old 側可作為「改前真值」，shifted=1 的結論不是掃描器假影。

## 4. 燈4＋閘門＋禁區

- `render_lab.py --selftest`：**20 項 ALL-PASS**（原 18 項全數原樣綠＋新增 4d2/4d3）。
- `scripts/tests/` 既測：J3 **16 條**、J4 **3 條** ALL-PASS，零回退；
  新增 `test_k25a_painted_ink.py` **6 條 ALL-PASS**。
- `core/engine-api/tests/k25_empty_page_parity.rs`（K25 交付的引擎側鎖，已入 main）：
  本卡分支 rebase 至 main `1c92fa6` 後實跑 ⇒ **5 passed / 0 failed**（且它本來就在
  `cargo test --workspace` 那一道閘門內）。本卡引擎零觸碰，它不可能被我變紅。
- 八閘 `bash ~/.hermes/scripts/ow-gate.sh .`：rebase 後重跑 ⇒ **PASS×8、
  `GATE_SUMMARY fail=0`**（fmt／clippy -D／workspace test／layering／roundtrip
  fixtures／roundtrip corpus／wasm check／goldens），日誌＝scratch
  `k25a-gate.log`（pre-rebase）＋`k25a-gate-rebased.log`（收口態）。
- 下游消費面（同口徑受益，非回歸）：`scripts/lo_scoreboard.py`、`scripts/j11_scan.py`
  都經 `RL.pdf_page_facts()` 取同一事實 ⇒ 自動同口徑；`scripts/p5_emf_probe.py`
  自建 `analyze_pdf()` 的 `images`（EMF 診斷用），與本欄位無關，未受影響。
- 觸碰面：`git status` 僅 `M scripts/render_lab.py`、
  `?? scripts/tests/test_k25a_painted_ink.py`、`?? docs/k25a-painted-ink-metric.md`
  （`scripts/render_lab.py` ＋98/−4 行）。`core/*`、`corpus/`、`out/` **零位元組觸碰**
  （A/B 掃描對主庫 `out/` 僅讀，輸出全落 scratch）。

