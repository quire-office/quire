# D7 紅燈考卷實拍紀錄

- 基準 commit：`5badcb8`
- 分支：`agy/d7-residual`
- 日期：2026-11-04
- 考卷檔案：`core/engine-api/tests/d7_residual.rs`
- 署名：ADT109119

## 考卷內容與 LO 真值對照

依據 D7 任務卡規範，以 LO PDF pymupdf 抽 span / drawing 為真值：
1. 頁首兩行 span 的 bbox 零重疊，且全部文字右緣 ≤ 外框內緣。
   - LO 真值：「BELVIDERE ASSEMBLY PLANT」y≈26.68~48.82、x 最大約 423.56pt；「World Class Manufacturing Starts Here」y≈49.43~64.92、x 最大約 426.03pt；右外框內緣約 587.05pt。兩行 y 軸不重疊且居中不溢出。
2. 「THE WORK HOURS FOR」結束 x 與「SATURDAY」起點 x 零重疊、粗體段結束與「WILL」起點零重疊（span bbox 逐個相鄰不交會）。
   - LO 真值：「THE WORK HOURS FOR 」寬 158.58pt，結束於 x≈191.93pt；「SATURDAY, APRIL 02, 2011」粗體寬 180.78pt，起點 x≈192.05pt、結束於 x≈372.83pt；後續「 WILL BE AS SCHEDULED ...」起點 x≈372.90pt。各 span 逐個相鄰不交會。
3. IMPORTANT NOTICE 黑底矩形寬度 ≥ 文字 span 組寬度（±2pt）。
   - LO 真值：文字寬約 166.7pt，黑底矩形寬約 166.65pt。
4. 班表各欄 span 起點 x 對 LO ±8pt；同欄上下行起點 x 一致（±1pt）。
   - LO 真值：第 1 欄 x≈91.85pt；第 2 欄 x≈177.35pt；第 3 欄（1st Break, Lunch, 2nd Break）所有行 x≈357.35pt。

## 接手基線實跑輸出（四燈全紅）

```text
$ cargo test --test d7_residual
   Compiling quire-engine v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-d7/core/engine-api)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.34s
     Running tests/d7_residual.rs (target/debug/deps/d7_residual-9be8a407c45bc668)

running 4 tests
test d7_lamp3_important_notice_shading_width ... FAILED
test d7_lamp2_bold_advance_non_overlapping ... FAILED
test d7_lamp4_timetable_tabs_alignment ... FAILED
test d7_lamp1_header_overlap_and_boundary ... FAILED

failures:

---- d7_lamp3_important_notice_shading_width stdout ----

thread 'd7_lamp3_important_notice_shading_width' (48877) panicked at core/engine-api/tests/d7_residual.rs:186:5:
「IMPORTANT NOTICE」黑底矩形寬度 (144.57pt) 小於文字寬度 (166.70pt - 2.0pt = 164.70pt)，末尾字落在白底上
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- d7_lamp2_bold_advance_non_overlapping stdout ----

thread 'd7_lamp2_bold_advance_non_overlapping' (48876) panicked at core/engine-api/tests/d7_residual.rs:132:5:
「THE WORK HOURS FOR 」結束 x (191.93) 與「SATURDAY, APRIL 02, 2011 」起點 x (182.35) 發生重疊（重疊 9.58pt）

---- d7_lamp4_timetable_tabs_alignment stdout ----

thread 'd7_lamp4_timetable_tabs_alignment' (48878) panicked at core/engine-api/tests/d7_residual.rs:241:9:
第 3 欄項目「2」(y=389.5) 起點 x=393.35 偏離 LO 真值 357.35 超過 8pt（實差 36.00pt）

---- d7_lamp1_header_overlap_and_boundary stdout ----

thread 'd7_lamp1_header_overlap_and_boundary' (48875) panicked at core/engine-api/tests/d7_residual.rs:61:5:
第一行「BELVIDERE ASSEMBLY PLANT」右緣 (613.16) 超出外框內緣 (587.05)


failures:
    d7_lamp1_header_overlap_and_boundary
    d7_lamp2_bold_advance_non_overlapping
    d7_lamp3_important_notice_shading_width
    d7_lamp4_timetable_tabs_alignment

test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

error: test failed, to rerun pass `-p quire-engine --test d7_residual`
```
