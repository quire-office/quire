# D6 表格邊框缺線 修前紅燈報告

- 記錄時間：2026-11-04
- 分支：agy/d6-borders
- 署名：ADT109119

## 1. core/ooxml 考卷紅燈輸出 (`cargo test -p quire-ooxml --test d6_table_borders`)
```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.01s
     Running tests/d6_table_borders.rs (target/debug/deps/d6_table_borders-dfe378ba7cf40d95)

running 3 tests

thread 'lamp3_start_and_end_borders_collected' (75680) panicked at core/ooxml/tests/d6_table_borders.rs:332:10:
tblPr tblBorders 應存在
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

thread 'lamp2_vmerge_with_gridspan_inherits_borders' (75679) panicked at core/ooxml/tests/d6_table_borders.rs:284:10:
燈 2 失敗：跨欄又跨列 continuation cell 必須繼承邊框
test lamp3_start_and_end_borders_collected ... FAILED

thread 'lamp1_vmerge_continuation_cell_inherits_borders' (75678) panicked at core/ooxml/tests/d6_table_borders.rs:185:10:
燈 1 失敗：末尾 continuation cell 必須繼承邊框，不可為 None（修前缺線根因）
test lamp2_vmerge_with_gridspan_inherits_borders ... FAILED
test lamp1_vmerge_continuation_cell_inherits_borders ... FAILED

failures:

failures:
    lamp1_vmerge_continuation_cell_inherits_borders
    lamp2_vmerge_with_gridspan_inherits_borders
    lamp3_start_and_end_borders_collected

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p quire-ooxml --test d6_table_borders`
```

## 2. core/engine-api 發射面斷言紅燈輸出 (`cargo test -p quire-engine --test d6_table_borders`)
```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.01s
     Running tests/d6_table_borders.rs (target/debug/deps/d6_table_borders-a6d7bd31030706b5)

running 2 tests
=== dump 合併格發射 Rule items ===
Rule: (72.00, 88.80) -> (372.00, 88.80), w=2.00
Rule: (72.00, 72.00) -> (72.00, 88.80), w=1.00
Rule: (372.00, 72.00) -> (372.00, 88.80), w=1.00

【修前紅燈檢測】合併格現缺線段清單（逐條列 bbox）：
  [1] 合併格底邊邊框缺失：期望在 y≈105.6 發射 [72.0, 105.6, 372.0, 105.6] w=2.0，實得 None
  [2] 合併格 continuation cell 左緣垂直分隔線缺段：期望在 x=72.0 覆蓋 y=[88.8, 105.6]，實得缺失
  [3] 合併格 continuation cell 右緣邊框丟失：期望在 x=372.0 覆蓋 y=[88.8, 105.6]，實得缺失

thread 'lamp2_vmerge_continuation_emits_bottom_rule' (75694) panicked at core/engine-api/tests/d6_table_borders.rs:329:5:
assertion `left == right` failed: 燈 2 失敗：合併格缺失線段數應為 0，實缺 3 條（修前肉眼實錘缺陷）
  left: 3
 right: 0
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test lamp2_vmerge_continuation_emits_bottom_rule ... FAILED
=== dump 邊框 item (WC048 p0) ===
Rule: (72.00, 72.00) -> (222.25, 72.00), w=0.50
Rule: (72.00, 137.45) -> (222.25, 137.45), w=0.50
Rule: (72.00, 72.00) -> (72.00, 137.45), w=0.50
Rule: (222.25, 72.00) -> (222.25, 137.45), w=0.50
Rule: (222.25, 72.00) -> (372.50, 72.00), w=0.50
Rule: (222.25, 137.45) -> (372.50, 137.45), w=0.50
Rule: (222.25, 72.00) -> (222.25, 137.45), w=0.50
Rule: (372.50, 72.00) -> (372.50, 137.45), w=0.50
Rule: (372.50, 72.00) -> (522.80, 72.00), w=0.50
Rule: (372.50, 137.45) -> (522.80, 137.45), w=0.50
Rule: (372.50, 72.00) -> (372.50, 137.45), w=0.50
Rule: (522.80, 72.00) -> (522.80, 137.45), w=0.50
Rule: (72.00, 137.45) -> (222.25, 137.45), w=0.50
Rule: (72.00, 343.50) -> (222.25, 343.50), w=0.50
Rule: (72.00, 137.45) -> (72.00, 343.50), w=0.50
Rule: (222.25, 137.45) -> (222.25, 343.50), w=0.50
Rule: (222.25, 137.45) -> (372.50, 137.45), w=0.50
Rule: (222.25, 343.50) -> (372.50, 343.50), w=0.50
Rule: (222.25, 137.45) -> (222.25, 343.50), w=0.50
Rule: (372.50, 137.45) -> (372.50, 343.50), w=0.50
Rule: (372.50, 137.45) -> (522.80, 137.45), w=0.50
Rule: (372.50, 343.50) -> (522.80, 343.50), w=0.50
Rule: (372.50, 137.45) -> (372.50, 343.50), w=0.50
Rule: (522.80, 137.45) -> (522.80, 343.50), w=0.50
Rule: (72.00, 343.50) -> (222.25, 343.50), w=0.50
Rule: (72.00, 356.95) -> (222.25, 356.95), w=0.50
Rule: (72.00, 343.50) -> (72.00, 356.95), w=0.50
Rule: (222.25, 343.50) -> (222.25, 356.95), w=0.50
Rule: (222.25, 343.50) -> (372.50, 343.50), w=0.50
Rule: (222.25, 356.95) -> (372.50, 356.95), w=0.50
Rule: (222.25, 343.50) -> (222.25, 356.95), w=0.50
Rule: (372.50, 343.50) -> (372.50, 356.95), w=0.50
Rule: (372.50, 343.50) -> (522.80, 343.50), w=0.50
Rule: (372.50, 356.95) -> (522.80, 356.95), w=0.50
Rule: (372.50, 343.50) -> (372.50, 356.95), w=0.50
Rule: (522.80, 343.50) -> (522.80, 356.95), w=0.50
我方表格邊框 Rule 總數: 36
test lamp1_wc048_table_grid_emitted_rules_match_lo ... ok

failures:

failures:
    lamp2_vmerge_continuation_emits_bottom_rule

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p quire-engine --test d6_table_borders`
```
