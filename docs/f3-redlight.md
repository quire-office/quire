# F3 文字框內圖片未渲染紅燈快照

署名：ADT109119

## 考卷執行結果

考卷路徑：`core/engine-api/tests/f3_txbx_picture.rs`
執行命令：`cargo test -p quire-engine --test f3_txbx_picture`

```text
   Compiling quire-engine v0.1.0 (<repo>/.worktrees/agy-f3-txbx-picture/core/engine-api)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.32s
     Running tests/f3_txbx_picture.rs (target/debug/deps/f3_txbx_picture-a957826eba950ec3)

running 2 tests
test test_f3_discriminative_power_under_buggy_mode ... ok
test test_f3_txbx_picture_emitted_and_geometry ... FAILED

failures:

---- test_f3_txbx_picture_emitted_and_geometry stdout ----

thread 'test_f3_txbx_picture_emitted_and_geometry' (68154) panicked at core/engine-api/tests/f3_txbx_picture.rs:83:5:
燈 1 紅燈：文字框內 DML 圖片未發射，發射清單圖片數為 0
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    test_f3_txbx_picture_emitted_and_geometry

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p quire-engine --test f3_txbx_picture`
```

## 紅燈原因分析

1. `corpus/ooxmlexport/dml-picture-in-textframe.docx` 於 `core/ooxml/src/drawing.rs` 解析時，DrawingML 文字框（`self.graphic`）其子樹 `<w:txbxContent>` 內部出現的 `<w:drawing>` 在 `b"drawing"` Start 事件處被整棵作為吞沒標籤（`skip_tag = Some((b"drawing", 0))`）掠過。
2. 內層圖片之 `wp:extent` 與 `a:blip/@r:embed` 未進入 `txbx_drawings` 向量，`txbx_drawings` 向量長度為 0。
3. 發射層 `core/engine-api/src/lib.rs` 的 `emit_txbx_texts` 在段落索引 `ti == 0` 時找不到可發射候選圖片，整頁發射結果之 `DrawItem::Image` 個數為 0，圖形軸分數實測為 0.0。
