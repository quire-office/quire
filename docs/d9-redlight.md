# D9 紅燈考卷實拍紀錄

- 基準 commit：`5d551f6`
- 分支：`agy/d9-frame-overlap`
- 日期：2026-11-04
- 考卷檔案：`core/engine-api/tests/d9_frame_overlap.rs`
- 署名：ADT109119

## 考卷內容與 LO 真值對照

依據 D9 任務卡規範，以 LO PDF pymupdf drawings/images 實抽為真值：
1. 4pt 外框底邊 y 對 LO 框底線 ±4pt。
   - LO 真值：Drawing 4 4pt 外框底邊線段 Rect(22.10, 749.85, 589.05, 749.85)，中心線 y=749.85pt。
   - 現行缺陷：我方外框底邊直接取 (y + h) = 18.20 + 733.65 = 751.85pt，未考慮 4pt 描邊內縮 2.0pt，偏離 LO 真值 2.00pt。
2. CHRYSLER logo bbox 與框底線 y 關係對 LO 一致（logo 頂緣 ≥ 框底內緣 −4pt，即落在框下方而非穿越；外框負 z 背景層先繪，logo 繪於框線之上）。
   - LO 真值：CHRYSLER logo（Image 11）bbox: [201.05, 732.80, 411.60, 767.35]，頂緣 y=732.80pt；大框 _x0000_s1026（z-index -251659776）為背景層，logo 框 _x0000_s1036（z-index +251657728）為前景層，LO 中 logo 繪於框線之上，且中間有白色底矩形（Drawing 6: [193.75, 729.15, 418.75, 774.15]）覆蓋外框底線，框線不穿越切過 logo。
   - 現行缺陷：外框 Rule（items[3]）在 logo Image（items[1]）之後發射，4pt 粗黑線硬生生切過 logo 中段壓住文字；且 logo 頂緣 y=729.20pt 偏離 LO 732.80pt 達 3.60pt。
3. logo 下緣 ≤ 頁面高（不得溢出頁底被裁）。
   - LO 真值：頁面高 792.00pt；logo 底緣 767.35pt ≤ 792.00pt（安全留白 24.65pt）。
   - 現行缺陷：我方 logo 發射於 (193.80, 729.20)，底緣為 729.20 + 34.21 = 763.41pt，偏離 LO 真值 767.35pt 達 3.94pt。

## 接手基線實跑輸出（三燈全紅）

```text
$ cargo test -p quire-engine --test d9_frame_overlap
   Compiling quire-engine v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-d9/core/engine-api)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.52s
     Running tests/d9_frame_overlap.rs (target/debug/deps/d9_frame_overlap-31abdb4ae07d7b11)

running 3 tests
test d9_lamp1_frame_bottom_border_y ... FAILED
test d9_lamp3_logo_bottom_within_page ... FAILED
test d9_lamp2_chrysler_logo_bbox_and_frame_overlap ... FAILED

failures:

---- d9_lamp1_frame_bottom_border_y stdout ----

thread 'd9_lamp1_frame_bottom_border_y' (34833) panicked at core/engine-api/tests/d9_frame_overlap.rs:51:5:
外框底邊 y (751.85pt) 未依 4pt 描邊向內收斂，偏離 LO 真值 (749.85pt) 達 2.00pt
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- d9_lamp3_logo_bottom_within_page stdout ----

thread 'd9_lamp3_logo_bottom_within_page' (34835) panicked at core/engine-api/tests/d9_frame_overlap.rs:152:5:
CHRYSLER logo 下緣 (763.41pt, 高=34.21pt) 偏離 LO 真值 (767.35pt) 超過 2pt (差值 3.94pt)

---- d9_lamp2_chrysler_logo_bbox_and_frame_overlap stdout ----

thread 'd9_lamp2_chrysler_logo_bbox_and_frame_overlap' (34834) panicked at core/engine-api/tests/d9_frame_overlap.rs:104:5:
CHRYSLER logo (idx=1) 在顯示列表中繪於外框底線 (idx=3) 之下，外框黑線壓在 logo 之上


failures:
    d9_lamp1_frame_bottom_border_y
    d9_lamp2_chrysler_logo_bbox_and_frame_overlap
    d9_lamp3_logo_bottom_within_page

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s

error: test failed, to rerun pass `-p quire-engine --test d9_frame_overlap`
```
