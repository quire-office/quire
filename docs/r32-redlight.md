# R32 紅燈：WC048 首列文字位移／Textbox3 灰帶／框內重複字

測試檔：`core/engine-api/tests/r32_wc048_table.rs`
標的：`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` p0
LO 二進位：`~/.local/lo-linux/opt/libreoffice25.8/program/soffice`
抽真值：pymupdf（`/usr/bin/python3`）

## 現行碼實拍（修前）

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
| 首列 cell1 文字 | x=77.8 **y=112.2**（Textbox1 wrap 推下）|
| 首列 cell2/3 文字 | x=228.0/378.2 y=72.7 |
| Textbox2（273.6×110.55）| 只有藍線 x171.7–423.55 y99.2/132.85，**無底色 Rect** |
| Textbox3 群組根（387×185）| **無底色 Rect**；子件白框 (72.25,159.4)-(459.25,257.9)＋藍塊 (80.95,342.1)-(362.35,344.4) |
| "This is a test." 全篇 | 9 個（首列 3＋第 2 列 3＋第 3 列 3）|

## 診斷

1. **首列 cell1 文字位移**：Textbox1 是 `wrapSquare`＋`spAutoFit`＋`wps:bodyPr`
   （`posV=182880EMU=14.4pt`、框內文字一列約 20.6pt）。LO 把首行文字推到
   `ceil((posV+eff_h)/line_h)*line_h = ceil(35.0/13.45)*13.45 = 40.35pt`
   （列頂 72.25＋40.35≈112.6，吻合 LO 112.2）。Quire 未做 wrap 推移，文字留在列頂。
   Textbox2 是 `wrapTopAndBottom`、Textbox3 群組 `posV=21.05pt > 1.2*line_h`，
   LO 皆不推 cell 文字——推移條件須排除這兩者。

2. **灰帶**：`emit_image_shape`（`core/engine-api/src/lib.rs:3849`）對「`filled` 未宣告
   ＋`fill_argb` 缺」一律退 `IMAGE_PLACEHOLDER_FILL=0xFFF0F0F0`。Textbox2（純文字框）
   與 Textbox3（群組根）皆無填充宣告 → 各發一塊灰 Rect。Textbox3 根的 387×185
   灰帶下緣 343.5 蓋過第 3 列文字 y=345.2，視覺上就像「字重複出現在框內」。

3. **重複字**：全篇 "This is a test." 發射數已是 9（＝LO 真值），並非真的多發；
   第 3 列文字只是被 Textbox3 根灰帶覆蓋。移除灰帶後現象即消失。

## 修法（本卡範圍）

- `table_map.rs`：K17 撐列計算同時回傳「wrapSquare 文字框推下量」（僅
  `posV < 1.2*line_h` 且非群組/非 TopAndBottom），`table_input` 把它併入宿主
  儲存格 `margins.top`；不動 `lib.rs` 幾何發射。
- `lib.rs` 僅動 `emit_image_shape` 的**填充段**：`shape` 是文字框（`txbx_texts`
  非空）或群組（`group_shapes` 非空）且無 `fill_argb` 宣告 → 不發灰底；
  `shape=None`（純圖片/EMF 佔位）仍維持灰底零擾動。
