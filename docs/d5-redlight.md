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

## 用戶肉眼比對新實錘（燈 6～燈 10）實拍紀錄

- 基準 commit：`00b19c8`
- 日期：2026-11-04
- 考卷更新：納入五條新實錘（外框 4pt 黑 stroke、下劃線、上標、粗體確認、頁腳 tab）

### 新增項目 LO 真值與現行狀態對照
6. 外框 4pt 黑 stroke：源檔 VML shape `_x0000_s1026`（567×733.65pt、filled="f"、strokeweight="4pt"、默認 strokecolor=black）。
   - LO 真值：外框四邊 4pt 黑色線條（color=black, width=4.0）。
   - 現行實測：線寬 4.0pt，但顏色退為淺灰佔位邊（0xFFCCCCCC）。
7. 下劃線：「(NOTE THE START TIME!)」源檔 `w:u single`。
   - 現行實測：帶有 FLAG_UNDERLINE（flags=4），已生效。
8. 上標：「1st」「2nd」的 st/nd 是 `w:vertAlign=superscript`。
   - LO 真值：「1」size=16.0pt, y=391.3；「st」size=9.3pt (約 0.58倍), y=391.0（縮字級＋y 向上偏移）。
   - 現行實測：未採集 vertAlign，壓成純文字「1st Shift」，st 未獨立且未縮小/偏移。
9. 粗體清單確認：SATURDAY, APRIL 02, 2011、班表時間欄、1st/2nd Shift 標籤段皆 `w:b`。
   - 現行實測：已帶有 FLAG_BOLD，確認生效。
10. 頁腳 tab：「11-016」靠左＋「3-23-2011」靠右（LO 分佈兩端）。
   - LO 真值：「11-016」起點 x≈33.3pt，「3-23-2011」起點 x≈501.4pt，兩者 x 差 468.1pt ≥ 400pt。
   - 現行實測：兩者相距僅 72pt（相鄰置中）。

### 實跑拍紅輸出

```text
$ cargo test --package quire-engine --test d5_tabs_runs
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.33s
     Running tests/d5_tabs_runs.rs (target/debug/deps/d5_tabs_runs-48b3ff214e0f85c0)

running 8 tests
test d5_lamp6_outer_frame_stroke_4pt_black ... FAILED
test d5_lamp1_timetable_tabs_expanded ... ok
test d5_lamp3_important_notice_highlight_background ... ok
test d5_lamp10_footer_tabs_spread_ends ... FAILED
test d5_lamp8_superscript_size_and_y_offset ... FAILED
test d5_lamp7_note_the_start_time_underline ... ok
test d5_lamp9_bold_list_confirmation ... ok
test d5_lamp2_title_size_and_bold ... ok

failures:

---- d5_lamp6_outer_frame_stroke_4pt_black stdout ----

thread 'd5_lamp6_outer_frame_stroke_4pt_black' (27068) panicked at core/engine-api/tests/d5_tabs_runs.rs:238:9:
assertion `left == right` failed: 外框 Rule（寬度 4pt）顏色應為黑色 (0xFF000000)，實得 0xFFCCCCCC
  left: 13421772
 right: 0
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- d5_lamp10_footer_tabs_spread_ends stdout ----

thread 'd5_lamp10_footer_tabs_spread_ends' (27064) panicked at core/engine-api/tests/d5_tabs_runs.rs:391:5:
頁腳「11-016」(25.05) 與「3-23-2011」(97.05) 的 x 差應 ≥ 400pt，實得 72pt（LO 分佈兩端）

---- d5_lamp8_superscript_size_and_y_offset stdout ----

thread 'd5_lamp8_superscript_size_and_y_offset' (27070) panicked at core/engine-api/tests/d5_tabs_runs.rs:289:5:
「1st」中的「st」未獨立成上標 item（現行被壓入同一純文字字串）

failures:
    d5_lamp10_footer_tabs_spread_ends
    d5_lamp6_outer_frame_stroke_4pt_black
    d5_lamp8_superscript_size_and_y_offset

test result: FAILED. 5 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
```

燈 6、燈 8、燈 10 三盞新紅燈如實拍紅鎖定，接手者依序攻克。

## 修復成果與轉綠紀錄

### 考卷全綠輸出（8 盞燈全數通過）

```text
$ cargo test --package quire-engine --test d5_tabs_runs
   Compiling quire-ooxml v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-d5/core/ooxml)
   Compiling quire-engine v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-d5/core/engine-api)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.47s
     Running tests/d5_tabs_runs.rs (target/debug/deps/d5_tabs_runs-48b3ff214e0f85c0)

running 8 tests
test d5_lamp2_title_size_and_bold ... ok
test d5_lamp3_important_notice_highlight_background ... ok
test d5_lamp7_note_the_start_time_underline ... ok
test d5_lamp10_footer_tabs_spread_ends ... ok
test d5_lamp6_outer_frame_stroke_4pt_black ... ok
test d5_lamp9_bold_list_confirmation ... ok
test d5_lamp1_timetable_tabs_expanded ... ok
test d5_lamp8_superscript_size_and_y_offset ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
```

### 零回退驗證（render_lab 前後對照）

```text
$ /usr/bin/python3 scripts/render_lab.py corpus/ooxmlexport/fdo76591.docx corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx corpus/ooxmlimport/n751077.docx --out /tmp/d5-lab-final
→ corpus/ooxmlexport/fdo76591.docx
   LO 1 頁 / OW 1 頁，內容相似度 [0.98]（版面 [0.987]）
   色 [0.795] 圖 [0.657] 真缺字 2（斷行切片已排除 0）：['1st', '2nd']
→ corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx
   LO 1 頁 / OW 1 頁，內容相似度 [1.0]（版面 [1.0]）
   色 [0.922] 圖 [None] 真缺字 0（斷行切片已排除 0）：[]
→ corpus/ooxmlimport/n751077.docx
   LO 2 頁 / OW 2 頁，內容相似度 [0.88, None]（版面 [0.867, None]）
   色 [1.0, 1.0] 圖 [1.0, None] 真缺字 0（斷行切片已排除 0）：[]
```

- `fdo76591`：版面分 `0.987`（完全符合基線，零回退）。
- `WC048`：`1.0/1.0`（完全一致，零回退）。
- `n751077`：`[0.88, None]` / `[0.867, None]`（完全一致，零回退）。
- 工作區測試：`cargo test --workspace` 163+ 測試全綠。
- 代碼品質：`cargo fmt --check` 與 `cargo clippy --workspace --all-targets` 零警告。

