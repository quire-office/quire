# D8 紅燈考卷實拍紀錄

- 基準 commit：`a9f558c`
- 分支：`agy/d8-bottom`
- 日期：2026-11-04
- 考卷檔案：`core/engine-api/tests/d8_bottom_flow.rs`
- 署名：ADT109119

## 考卷內容與 LO 真值對照

依據 D8 任務卡規範，以 LO PDF pymupdf 實抽為真值：
1. 「PLANT HUMAN RESOURCES DEPARTMENT」span bbox 與徽章圖 bbox 零重疊（LO 零重疊）。
   - LO 真值：徽章圖 IMAGE bbox: [264.1, 594.1, 348.0, 687.0]，底緣 y=687.00pt；正文末段「PLANT HUMAN RESOURCES DEPARTMENT」bbox: [159.05, 706.23, 452.04, 721.85]，頂緣 y=706.23pt。LO 正文末段在徽章圖下緣之下 19.23pt，零重疊。
2. 頁腳兩 span y 對 LO ±10pt，且在正文末段之下。
   - LO 真值：頁腳「11-016」與「3-23-2011」y≈730.53pt，貼近外框底部內緣，且在正文末段底緣（~721.85pt）之下。
3. 首段每個 span 右緣 ≤ 外框內緣（對 LO 折行點 ±1 詞）。
   - LO 真值：外框內緣 587.05pt；首段折成三行，第 2 行在「THE」後折行，右緣 573.77pt ≤ 587.05pt；「FOLLOWING」落入第 3 行，不穿出右外框。
4. 班表末行與徽章圖起點之間的 y 間隔對 LO ±20pt（空白段落帶不得被吞）。
   - LO 真值：班表末行（2nd Break / 10:30 – 10:50 p.m.）y 頂緣 538.49pt、底緣 556.21pt；徽章圖起點 y=594.10pt；頂緣至起點間隔 55.61pt。

## 接手基線實跑輸出（四燈全紅）

```text
$ cargo test -p quire-engine --test d8_bottom_flow
   Compiling quire-engine v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-d8/core/engine-api)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.93s
     Running tests/d8_bottom_flow.rs (target/debug/deps/d8_bottom_flow-bd6647046a47bf42)

running 4 tests
test d8_lamp2_footer_anchoring_and_position ... FAILED
test d8_lamp1_plant_hr_badge_non_overlapping ... FAILED
test d8_lamp4_timetable_gap_to_badge ... FAILED
test d8_lamp3_first_paragraph_width_and_wrap ... FAILED

failures:

---- d8_lamp2_footer_anchoring_and_position stdout ----

thread 'd8_lamp2_footer_anchoring_and_position' (62516) panicked at core/engine-api/tests/d8_bottom_flow.rs:135:5:
頁腳「11-016」y (641.60) 偏離 LO 真值 (730.53) 超過 10pt (差值 88.93pt)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- d8_lamp1_plant_hr_badge_non_overlapping stdout ----

thread 'd8_lamp1_plant_hr_badge_non_overlapping' (62515) panicked at core/engine-api/tests/d8_bottom_flow.rs:70:5:
正文末段「PLANT HUMAN RESOURCES DEPARTMENT」y (625.50) 壓穿徽章圖下緣 (683.60)，發生重疊

---- d8_lamp4_timetable_gap_to_badge stdout ----

thread 'd8_lamp4_timetable_gap_to_badge' (62518) panicked at core/engine-api/tests/d8_bottom_flow.rs:252:5:
班表末行 (y=475.10) 與徽章圖 (y=590.45) 的間隔 (115.35pt) 偏離 LO 真值 (55.61pt) 超過 20pt (差值 59.74pt)

---- d8_lamp3_first_paragraph_width_and_wrap stdout ----

thread 'd8_lamp3_first_paragraph_width_and_wrap' (62517) panicked at core/engine-api/tests/d8_bottom_flow.rs:187:9:
首段行 (y=210.90)「SKILLED TRADES EMPLOYEES, WILL BE REQUIRED TO WORK THE FOLLOWING」右緣 (603.80) 超過外框內緣 (587.05)


failures:
    d8_lamp1_plant_hr_badge_non_overlapping
    d8_lamp2_footer_anchoring_and_position
    d8_lamp3_first_paragraph_width_and_wrap
    d8_lamp4_timetable_gap_to_badge

test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s

error: test failed, to rerun pass `-p quire-engine --test d8_bottom_flow`
```
