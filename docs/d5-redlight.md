# D5 紅燈考卷實拍紀錄

- 基準 commit：`9d0407e`
- 日期：2026-11-04
- 考卷檔案：`core/engine-api/tests/d5_tabs_runs.rs`

## 考卷內容與 LO 真值對照

依據任務卡規範，以 LO PDF pymupdf 抽 span 為真值：
1. 班表「1st Shift」與「6:00」兩 span 的 x 差 ≥ 30pt（tab 拉開），各 span x 對 LO ±12pt。
   - LO 真值：「1st Shift」起點 x≈91.85pt；「6:00 a.m. Start – 2:30 p.m.」起點 x≈177.35pt；兩者 x 差 85.50pt ≥ 30pt。
2. 「Scheduled Saturday Overtime」渲染字級 == LO span 字級 ±0.5pt（20pt），bold 生效（FLAG_BOLD）。
   - LO 真值：size=20.0pt, flags=20 (含 bold)。
3. 「IMPORTANT NOTICE」item 帶 highlight/shading 底（底層矩形 item 存在且覆蓋文字 bbox）。
   - LO 真值：文字 color=white，底層 Rect rect=[221.2, 147.1, 389.95, 168.85] fill=black。

## 接手基線實跑輸出

```text
$ cargo test --package quire-engine --test d5_tabs_runs
   Compiling quire-engine v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-d5/core/engine-api)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.26s
     Running tests/d5_tabs_runs.rs (target/debug/deps/d5_tabs_runs-48b3ff214e0f85c0)

running 3 tests
test d5_lamp1_timetable_tabs_expanded ... FAILED
test d5_lamp2_title_size_and_bold ... FAILED
test d5_lamp3_important_notice_highlight_background ... FAILED

failures:

---- d5_lamp1_timetable_tabs_expanded stdout ----

thread 'd5_lamp1_timetable_tabs_expanded' (70618) panicked at core/engine-api/tests/d5_tabs_runs.rs:58:5:
「6:00」未獨立成 span（現行實測被壓成整條「1st Shift 6:00 a.m. Start – 2:30 p.m.」）
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- d5_lamp2_title_size_and_bold stdout ----

thread 'd5_lamp2_title_size_and_bold' (70619) panicked at core/engine-api/tests/d5_tabs_runs.rs:115:5:
「      Scheduled Saturday Overtime」渲染字級應為 20 ±0.5pt，實得 12pt

---- d5_lamp3_important_notice_highlight_background stdout ----

thread 'd5_lamp3_important_notice_highlight_background' (70620) panicked at core/engine-api/tests/d5_tabs_runs.rs:179:5:
「IMPORTANT NOTICE」底層矩形 item 不存在或未覆蓋文字 bbox（text_x=249.36, text_y=124.8）


failures:
    d5_lamp1_timetable_tabs_expanded
    d5_lamp2_title_size_and_bold
    d5_lamp3_important_notice_highlight_background

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

error: test failed, to rerun pass `-p quire-engine --test d5_tabs_runs`
```

三盞紅燈全部如實拍紅，基線鎖定完畢。
