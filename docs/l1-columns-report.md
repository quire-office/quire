# L1-columns：`w:cols` 分欄回流交付報告（oc/l1-columns）

任務卡：t_26c3607d（run10）。基線 = PM 代 commit 的 `e7dbde9`（sectpr 側
`SectionCols` 半成品，認領續做不重寫）。

## 實作範圍（三個 commit）

1. `491f100` feat：分欄回流全鏈路。
2. `3fe1086` fix：tools/line-metrics 跟隨 `SectionGeometry.cols`（E0063）。
3. `f1c6cc8` chore：移除兩個 l1_probe 診斷 example（PM 紀律(1)）。

### ooxml 側（`core/ooxml/src/sectpr.rs`）

- `SectionGeometry.cols: SectionCols`（serde default，舊快照相容）。
- scan 分支：`w:cols` 讀 `@num`/`@space`（缺=720twip=36pt）、`w:col` 逐欄
  `@w:w`/`@w:space` 入頭 4 欄固定槽（Copy，無 Vec，PM 骨架）；`w:cols`
  重開重置 col_seen；超 4 欄槽位截斷、num 原值保留（呼叫端 clamp）。

### layout 側（`core/layout/src/lib.rs`，主戰場）

- layout 側鏡像 `SectionCols`＋`col_layout(&geo)`：`num<=1`/未宣告 →
  `None`（**單欄零擾動閘門**）；宣告逐欄寬 → 用宣告值 else 等寬平分；
  右側距 = 宣告 `col_space` else `space_pt`。產出 `col_x[]`/`col_w[]`。
- `pack_run` 回流：版心寬按欄切片（`section_break_w_pt`）→ 段落填滿當前
  欄（`cur_used` 對 `cap`）→ 容量溢出先換欄（`col_break_now`：`cur_col+1`、
  `cur_used=page_used`，頁內位移，不動頁 flush/收斂鍵）→ 欄盡（最後一欄
  溢出）才換頁（`fresh_page` 重置 `cur_col/page_used`）。
  硬斷（`w:br`）與 `pageBreakBefore` 越過換欄直接換頁；非 continuous
  section 邊界照舊強制換頁；continuous 邊界只免邊界本身。
- `Line.col: u8`（serde default 0）：行蓋欄序章；goldens 只收
  text/width_pt，不擾動（實測三檔全相符）。
- 跨欄項簡化（卡上已定）：表格/整幅圖佔滿版心寬；多欄下以頁級
  `page_used` 讓每一欄頂端共同讓位；單欄時 `place_band==cur_used`
  與舊算式逐位元組等價。多欄表格加 remain=0 保護（先換頁重放，防整項
  被丟）。

### engine 側（`core/engine-api/src/lib.rs`，唯一接線點）

- ooxml→layout `SectionCols` 逐欄對應（分欄鐵律：layout 不依賴 ooxml）。
- 級聯斷行寬統一走 `section_break_w_pt`（與 layout 斷行同口徑）。
- 發射層：文字項 `x = margin_left + col_x[line.col]`；單欄/越界回退
  `margin_left`（舊行為逐位元組不變）。

## 紅燈（PM 紀律(3)：先寫先紅→紅綠即 commit）

- **紅燈1（engine 級）** `core/engine-api/tests/l1_two_col.docx`（zipfile
  手搓，1262 bytes，commit 入庫）＋`tests/l1_columns.rs`：
  - `l1_two_column_fixture_has_two_x_origins`：p0 文字項 x 必須出現兩個
    欄起點（72.0 與 315.65=72+207.65+36），欄距（第二欄起差）=宣告
    欄寬+space≈243.65pt。→ 綠。
  - `l1_two_column_lines_fit_column_width`：斷行寬=欄寬（行長字元數
    遠小於單欄 451.3pt 口徑）。→ 綠。
  - `l1_single_column_unaffected`：單欄檔（sample.docx）首文字項恆從
    版心左緣起（buggy 回塞把未宣告誤當兩欄即紅）。→ 綠。
- **紅燈2（layout 級）** `tests::cols_*` 7 例：未宣告/num<=1 → None；
  Letter 兩欄等寬平分幾何（216/252）；40+40 段欄內回流第 81 段起第 2
  頁；逐欄宣告寬優先；欄滿換頁。→ 全綠（layout lib 126 passed）。
- **紅燈3（ooxml 級）** `cols_*` 4 例：未宣告預設/宣告逐字投影（twip→pt）/
  自閉合+跨節獨立/超 4 欄截斷。→ 全綠（ooxml lib 167 passed）。
- **buggy 回塞雙紅鎖死**：`col_break_now = false && …` 人為關換欄 →
  layout `cols_reflow` 與 engine `two_x_origins` 同步必紅（實測），恢復後
  全綠。測試真咬得住，非恆真斷言。

## 全閘門（PM 紀律(4)）

- `cargo test --workspace`：layout 126 ✓、ooxml 167 ✓、engine 154 ✓、
  其餘套件全綠（0 failed）。
- `cargo clippy --all-targets -- -D warnings`：RC=0。
- goldens：shaping 12 / pagination 4 / tables 5 三檔相符（**單欄零擾動
  鐵律達成**——`Line.col` serde default 不入 golden 口徑）。
- `scripts/check-layering.sh`：✓（layout 不依賴 ooxml；engine 唯一接線）。
- 真實語料探針（rm 前 l1_probe）：n830205（OOO 導出雙欄節）6 頁 → 5 頁，
  p1 出現第二欄起點 315.6/354.4/378.9/383.9pt（BASE 全頁無 >300pt 起點）。

## 儀表板 A/B（seed42 雙目錄，基線 e7dbde9 worktree vs HEAD）

- 25 檔（seed=42, n=25）逐檔比對：**25/25 分數逐位元組零變動**（WORSE=0、
  BETTER=0），content avg 0.666 / layout avg 0.584 / page_match 21/25
  兩側全等。零劣化達成。
- 樣本含 3 個雙欄檔（phpword reader / reader-styles / clone-delete-block），
  dump 實測其雙欄段的**斷行確實變短**（如 clone-delete-block p0 段行從
  單欄長行切成欄寬短行），但分數不動的原因要講明：
  1. `content_key` 口徑忽略換行位置（J2 校正設計）；
  2. `layout_key` 只比每頁前 8 行拼接，這 3 檔的雙欄段都不落在前 8 行；
  3. 這些檔雙欄段內容短、不溢出欄，頁數不變（`pages_lo/pages_ow=None`
     =LO 側轉換不可比）。
  即：**A/B 零劣化可信（無檔變差），零改善是度量口徑盲區，不是分欄未
  生效**——生效證據 = 紅燈組 + n830205 探針（6→5 頁、第二欄 x 315.6）。
  證據存檔：`out/ab-base/scores.jsonl`、`out/ab-head/scores.jsonl`。

## 與 LO 的已知殘差（簡化項，註釋已寫明）

1. **跨欄項佔滿全幅**：表格/整幅圖不切欄、佔版心寬；多欄下各欄頂端
   共同讓位（頁級 `page_used`）。LO 會把跨欄表格按整幅插斷點；我們不做
   欄內表格切片。
2. **不做欄平衡（column balancing）**：末欄不回填平衡高度（continuous
   節 LO 會平衡各欄高度；我們是貪心填滿）。n830205 頁數 5 頁 vs LO 若
   平衡可能 4 頁，屬此殘差。
3. **逐欄宣告超 4 欄截斷**：corpus 實見 ≤4 欄（alphabeticalIndex 4 欄），
   >4 畸形檔用平分近似。
4. **欄內浮動圖形定位**：浮動圖仍以頁為參照（未做欄內繞排），與 K4 系列
   既有殘差同源。

## LO 頁數組五瓶實讀（pymupdf，禁憑印象；render_lab.lo_to_pdf）

| 瓶 | LO 頁數 | quire 修前 | quire 修後 | 收斂 |
|---|---|---|---|---|
| floating-table-section-columns | 1 | 1 | 1 | ✓ |
| tdf103975_notPageBreakD | 1 | 1 | 1 | ✓ |
| n830205 | **5** | 6 | **5** | ✓（分欄回流直接收斂） |
| fdo53985 | 1 | 1 | 1 | ✓ |
| 090716_Studentische_Arbeit_VWS | 6 | 6 | 6 | ✓ |

n830205 是本案最強改善證據：修前單欄斷行多擠 1 頁，修後與 LO 全等，
且 p1 實測兩欄起點 [72.0, 315.6…]（第二欄=72+207.65+36 口徑）、p2 起
72/198 兩欄結構。dump 前後存檔 `~/.hermes/cache/scratch/l1/`
（baseline_before.txt / final_five.txt）。

## 未盡事項

- seed42 樣本無溢出型雙欄檔（內容都不滿欄），BETTER 側無法在本樣本內
  呈現；建議下輪以多欄專檔子集（scan_cols.py 列出的 22 檔）跑定向 A/B。
- 未 push（依鐵律等用戶指令）。
