# G1：錨點圖 `positionV/H relativeFrom` 原點換算與附掛頁修復報告

卡號：`t_76bbb908`｜分支：`wt/t_76bbb908`（`agy/g1`）｜署名：`ADT109119`

## 1. 問題根因與四落點實作摘要

根據 `docs/g1-anchor-relativity-anatomy.md` 解剖報告，採集層（`core/ooxml/src/drawing.rs`）已完整捕獲 `wp:anchor` 的 `pos_x_rel`、`pos_y_rel`、`pos_x_pt`、`pos_y_pt`、`pos_x_align`、`layout_in_cell` 與 `rel_height`，缺陷集中在排版與映射層的四個落點（A/B/C/D）以及多欄節（`w:cols`）非表格欄原點換算。

- **落點 A（靜態宿主列 y 改為渲染態實際列頂 y）**：舊版 `find_cell_for_para` 以 `margin_top + tblp_y + geom.1` 靜態預估儲存格頂部，假設表格永遠從頁面版心頂端起算且不跨頁；當表格前有續列或段落推下、或跨頁切片時，浮動圖垂直座標與宿主列實際位置脫鉤（如用戶症狀 p1 圖片滯留版心頂 `y=72.1pt` 被前段表格續列覆蓋）。本次於 `quire_layout::OriginFrame::CellRow(CellHostAnchor)` 記錄宿主 `(slot, row, static_top_bits, y_rel_to_row)`，由 `attach_floating_images` 在分頁定案後讀取落點頁 `TablePlacement` 的 `tp.frag.y_pt + 切片內前置列高`，動態替換靜態列頂。
- **落點 B（附掛頁改取宿主列實際落點頁）**：舊版 `attach_floating_images` 對表格內段落一律以表格流項起頭 `(fi, 0, 0)` 查 `starts` 分頁點，導致宿主列被推至次頁時圖片仍掛在表格起頭頁。本次改為優先搜尋含 `tp.slot == ch.slot` 且 `rf.row_index == ch.row && rf.starts_row`（次取首見 `rf.row_index == ch.row`）的頁面作為附掛頁。
- **落點 C（V 軸 `relativeFrom="paragraph"` 補上框線寬與上內距）**：在 `!layout_in_cell` 且宿主位於儲存格內時，依 LibreOffice 實測真值律加上儲存格上框線寬（`border_top_pt`）與上內距（`pad_top_pt`），使 `y = 宿主列實際頂 + border_top_pt + pad_top_pt + posOffset`；`layoutInCell=1` 維持 K15/K17 原路徑以確保既有 213 枚錨點零回退。
- **落點 D（H 軸 `relativeFrom="column"` 取欄內容盒與多欄節 `w:cols` 欄起點）**：宿主在儲存格內且 `!layout_in_cell` 時，H 軸原點取 `表左 + 格左 + 0.5 * border_left_pt + pad_left_pt`，對齊框架寬度取內容盒寬 `cell_w_pt - pad_left_pt - pad_right_pt`；宿主不在儲存格內且處於多欄節（`w:cols`）時，新增 `OriginFrame::Column`，以 `section_break_w_pt` 為欄寬並在 `attach_floating_images` 按宿主段所在行之 `Line::col` 加上 `col_layout` 的 `col_x[col]`，不再偷渡版心左緣。

## 2. 先紅後綠實測紀錄

### 2.1 修前紅燈輸出（未修改引擎碼）

```text
running 6 tests
未設 G1_BUGGY：略過回塞態斷言（以 G1_BUGGY=1 執行可驗證修前退化態）。
test g1_buggy_env_regression_lock ... ok
未設定 QUIRE_G1_FIXTURE：略過外部真實檔案測試。
test g1_user_fixture_env_gated ... ok

thread 'g1_lamp4_multicolumn_w_cols_and_z_order' panicked at core/engine-api/tests/g1_anchor_relativity.rs:383:5:
燈4（多欄 column 原點）：次欄錨點圖 x=82 應為次欄起點 315.65 + 10.0 = 325.65 (±2pt；修前偷渡版心左緣得 82.0）
test g1_lamp4_multicolumn_w_cols_and_z_order ... FAILED

thread 'g1_lamp3_horizontal_column_content_box_and_center_align' panicked at core/engine-api/tests/g1_anchor_relativity.rs:325:5:
燈3（落點D）：B 圖（posOffset=0）x=300.4 應命中 LO 真值 304.1±2pt（修前 300.40，缺 ½框線 1.0＋左內距 2.75）
test g1_lamp3_horizontal_column_content_box_and_center_align ... FAILED

thread 'g1_lamp2_vertical_paragraph_origin_dynamic_row_top_and_border_padding' panicked at core/engine-api/tests/g1_anchor_relativity.rs:251:5:
assertion `left == right` failed: p5 必須有 2 枚錨點圖（修前掛在上一頁且 y=525.65）
  left: 0
 right: 2
test g1_lamp2_vertical_paragraph_origin_dynamic_row_top_and_border_padding ... FAILED

thread 'g1_lamp1_anchor_attached_to_host_row_actual_page' panicked at core/engine-api/tests/g1_anchor_relativity.rs:225:5:
燈1（落點B）：錨定表起頭頁 p4 不得滯留第 1 列的浮動圖，實得: [(95.5, 525.64996, 202.15, 113.7), (300.4, 525.64996, 202.15, 113.7)]
test g1_lamp1_anchor_attached_to_host_row_actual_page ... FAILED

test result: FAILED. 2 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

### 2.2 修後綠燈與 `G1_BUGGY=1` 回塞鎖輸出

```text
running 6 tests
test g1_buggy_env_regression_lock ... ok
test g1_user_fixture_env_gated ... ok
test g1_lamp3_horizontal_column_content_box_and_center_align ... ok
test g1_lamp2_vertical_paragraph_origin_dynamic_row_top_and_border_padding ... ok
test g1_lamp1_anchor_attached_to_host_row_actual_page ... ok
test g1_lamp4_multicolumn_w_cols_and_z_order ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s

# G1_BUGGY=1 cargo test -p quire-engine --test g1_anchor_relativity g1_buggy_env_regression_lock
running 1 test
test g1_buggy_env_regression_lock ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out; finished in 0.05s
```

## 3. 四落點 A/B/C/D 收案對照表

| 落點 | 狀態 | 考卷覆蓋 | 修前實測值 | 修後實測值 | LO 真值基準（容差 ±2pt） |
|---|---|---|---|---|---|
| A：動態宿主列 y | 已收案 | `g1_lamp2`（子測 A/B/C） | 頁中表滯留 `72.05pt`；跨頁列留前頁 `525.65pt` | 跨頁列頂 `72.0pt → y=76.80pt`；同頁列頂 `522.0pt → y=526.80pt`；頁中表 `表頂+4.80pt` | `列頂 + 4.75pt`（同頁 `526.75pt`，誤差 `0.05pt`） |
| B：附掛頁取宿主列實際落點頁 | 已收案 | `g1_lamp1` | 掛在錨定表起頭頁 `p4`（2 枚），落點頁 `p5` 為 0 枚 | `p4` 為 0 枚，實際落點頁 `p5` 發射 2 枚 | 附掛於錨定列實際所在頁 |
| C：V 軸補框線寬與上內距 | 已收案 | `g1_lamp2`（子測 A/B） | 相對列頂僅 `+0.05pt`（缺上框線 `2.0pt`＋上內距 `2.75pt`） | 相對列頂 `+4.80pt`（`2.0 + 2.75 + 0.05`） | 相對列頂 `+4.75pt`（誤差 `0.05pt`） |
| D：H 軸欄內容盒與多欄 `w:cols` | 已收案 | `g1_lamp3`、`g1_lamp4` | A 圖 `x=95.50pt`、B 圖 `x=300.40pt`、間距 `204.90pt`；雙欄次欄 `x=82.0pt` | A 圖 `x=96.50pt`、B 圖 `x=304.15pt`、間距 `207.65pt`；雙欄次欄 `x=325.65pt` | A 圖 `96.50pt`（誤差 `0.00pt`）、B 圖 `304.10pt`（誤差 `0.05pt`）、間距 `207.60pt`（誤差 `0.05pt`）、次欄 `325.65pt` |

## 4. 六條收尾閘門實跑結果

1. `cargo fmt --all -- --check`：通過（退出碼 `0`，零差異）。
2. `cargo clippy --workspace --all-targets -- -D warnings`：通過（退出碼 `0`，0 warnings / 0 errors）。
3. `cargo test --workspace`：通過（共 `90` 個 test suite `test result: ok`，`0 FAILED`；主線基準 89 suite 外加新考卷 `g1_anchor_relativity` 1 suite）。
4. `cargo test -p quire-engine --test g1_anchor_relativity`：通過（`6 passed; 0 failed`；含 `G1_BUGGY=1` 鑑別鎖驗證通過）。
5. `cargo run -q -p quire-cli -- roundtrip corpus --check`：通過（`roundtrip --check: 399 檔（另有 1 檔隔離不計紅），byte-identical 399，失敗 0`）。
6. `cargo check -q -p quire-engine --target wasm32-unknown-unknown`：通過（退出碼 `0`）。

## 5. 未收口項清單（本卡範圍外，老實記帳不猜數）

1. **`wrapSquare` 文字繞排（G1b）**：目前僅消費 `wp:anchor` 原點與幾何發射位置，尚未實作正文或格內文字對 `wrapSquare` 浮動圖矩形的左右繞排退讓。
2. **`behindDoc="1"` 背景浮圖壓文字底層**：本卡合成檔與目標場景皆為 `behindDoc="0"`（浮於表格與文字之上），`behindDoc="1"` 壓入表格填色或文字底層的圖層拆分尚未實測與實作，維持現況發射序。
3. **`layoutInCell="1"` 的框線與內距微差（K15/K17 路徑）**：語料庫中 213 枚 `layoutInCell="1"` 錨點受既有 K15/K17 考卷 `±0.5~1pt` 鎖保護，本卡僅為其接入動態列頂（A）與跨頁落點頁附掛（B），未改動其 C/D 框線內距偏移口徑。
4. **儲存格內多段落前置高度累加**：當同一儲存格內有多個前置段落且錨點掛在第 2 段以後時，格內段落流偏移尚未單獨拆分累加，目前以儲存格內容盒頂為段落原點。
5. **E1 跨頁表格列高微差（另卡 `t_8573d294`）**：長表跨頁處因列高量化與 LibreOffice 存在 4 列分頁落點差，屬 E1 表格分頁欠債，不在本卡錨點原點換算範圍。
6. **`twoCellAnchor` 與 `effectExtent`／旋轉邊框膨脹**：語料未見文書流 `twoCellAnchor` 實例，且 `rot`／`effectExtent` 對定位原點的額外膨脹未在本卡實測，維持原行為。
