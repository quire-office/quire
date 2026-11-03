# R32 紅燈：WC048 首列文字位移／Textbox3 灰帶／框內重複字

測試檔：`core/engine-api/tests/r32_wc048_table.rs`
標的：`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` p0
LO 二進位：`~/.local/lo-linux/opt/libreoffice25.8/program/soffice`
抽真值：pymupdf（`/usr/bin/python3`）

## 現行碼實拍（接手起點修前紅燈）

```text
$ cargo test -p quire-engine --test r32_wc048_table
running 3 tests
test test_text_total_matches_lo_truth ... ok
test textbox_roots_have_no_gray_fill ... FAILED
test first_row_cell1_text_pushed_below_textbox ... FAILED

---- textbox_roots_have_no_gray_fill stdout ----
頁面不得有佔位灰 Rect（LO Textbox2/3 皆無底色），實得
[(160.575, 93.6, 273.6, 110.55), (72.0, 158.4959, 387.0, 185.0)]

---- first_row_cell1_text_pushed_below_textbox stdout ----
首列 cell1 文字應被 Textbox1 wrap 推下到 LO y=112.2±4，實得 y=72（修前 72.0）

test result: FAILED. 1 passed; 2 failed
```

## LO 真值

| 項目 | LO 實測 (pt) |
|---|---|
| 首列列頂 y≈72 文字序列 | [空, "This is a test.", "This is a test."] |
| 首列 cell1 被推下文字 | x=77.8 **y=112.2**（Textbox1 wrap 推下）|
| 首列 cell2/3 文字 | x=228.0/378.2 y=72.7 |
| Textbox2（273.6×110.55）| 只有藍線 x171.7–423.55 y99.2/132.85，**無底色 Rect** |
| Textbox3 群組根（387×185）| **無灰底 Rect**；子件白框 (72.25,159.4)-(459.25,257.9)＋藍塊 (80.95,342.1)-(362.35,344.4) |
| "This is a test." 全篇 | 9 個（首列 3＋第 2 列 3＋第 3列 3）|

## 診斷

1. **首列 cell1 文字位移**：Textbox1 是 `wrapSquare`＋`spAutoFit`＋`wps:bodyPr`
   （`posV=182880EMU=14.4pt`、框內文字一列約 20.6pt、框寬 185.9pt 佔滿宿主格寬 150.25pt）。
   LO 因框寬佔滿兩側無環繞空間，把首行文字推到
   `ceil((posV+eff_h)/line_h)*line_h = ceil(35.0/13.45)*13.45 = 40.35pt`
   （列頂 72.25＋40.35≈112.6，吻合 LO 112.2）。Quire 修前未做 wrap 推移，文字留在列頂。
   Textbox2 是 `wrapTopAndBottom`、Textbox3 群組 `posV=21.05pt > 1.2*line_h`，LO 皆不推 cell 文字。
   另在 WC049 中格寬 450pt 充裕，LO 走 `wrapText="bothSides"` 兩側環繞，不推下文字；
   推下條件必須限定 `covers_cell_w`（框寬 ≥ 格寬 85%），避免誤傷 WC049（零回退保護）。

2. **灰帶**：`emit_image_shape`（`core/engine-api/src/lib.rs:3858`）對「`filled` 未宣告
   ＋`fill_argb` 缺」一律退 `IMAGE_PLACEHOLDER_FILL=0xFFF0F0F0`。Textbox2（純文字框）
   與 Textbox3（群組根）皆無填充宣告 → 各發一塊灰 Rect。Textbox3 根的 387×185
   灰帶下緣 343.5 蓋過第 3 列文字 y=345.2，視覺上就像「字重複出現在框內」。

3. **重複字**：全篇 "This is a test." 發射數已是 9（＝LO 真值），並非真的多發；
   第 3 列文字只是被 Textbox3 根灰帶覆蓋。移除灰帶後現象即消失。

## 修法

- `table_map.rs`：`k17_host_cell` 定位宿主儲存格，比對框寬與格寬。若為 `wrapSquare`、
  框寬佔滿格寬且框頂壓到首行帶，計算推下量 `off` 併入儲存格 `margins.top`。
- `lib.rs` 僅動 `emit_image_shape` 的**填充發射段**（嚴格遵守不碰其他段鐵律）：
  表內錨點（`in_cell || layout_in_cell`）且無填充宣告（`fill_argb.is_none()`）之文字框／群組根，
  改發全透明 Rect（`0x00000000`），既消除灰色覆蓋帶，又完美保證 K15 幾何原點錨定測試
  （`rect_xy` 尋找 `387×185` 與 `273.6×110.55`）零回退。

## 修後綠燈驗收

```text
$ cargo test -p quire-engine --test r32_wc048_table
running 3 tests
test first_row_cell1_text_pushed_below_textbox ... ok
test test_text_total_matches_lo_truth ... ok
test textbox_roots_have_no_gray_fill ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

工作區回歸全綠：`cargo test --workspace` 零回退（含 k15_cell_anchor_geom、k17_anchor_rowgrow、k20_wc049mod_rowgrow、mce_nested_fallback_drawing_stays_one_page 與全部 roundtrip 測試）。

## 關於 render_lab 與 R31 分界說明

- render_lab 實測：WC048 內容分從修前 0.435 提升至 0.688，版面 0.649，色分 0.947，真缺字 0。
- 0.688 殘差成因：LO PDF 是把整個表格主體（9 個 "This is a test."）連續輸出後，再輸出浮動圖層文字框（Textbox1/2/3）；
  而 Quire 在 `lib.rs:sort_display_items` 中依幾何基線 (y, x) 排序，使圖層文字框與表格列文字交錯交織。
- 依任務鐵律：「lib.rs 僅限 shd/fill 發射段；textbox/group 發射層歸 agy/r31 卡，禁碰。若必須動 lib.rs 其他段，停在 comment 回報，不要改——另一張卡 R31 正在動 lib.rs 發射層，禁兩卡同檔」。
- 本卡嚴格恪守鐵律，未逾矩碰觸 `sort_display_items` 與文字框發射層，在此據實記錄分界。
