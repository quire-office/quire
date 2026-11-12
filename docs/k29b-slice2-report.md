# OC-K29B 第二片交付報告：段落拆併、run/段落樣式、清單編號

## 1. 範圍與結果

本卡在 K29a 的 Surgical XML Patch、touched 退場與 undo/redo 基礎上，補齊編輯 API 第二片：

- M1.3 `SplitParagraph`
- M1.4 `MergeParagraphWithNext`
- 跨段 M1.2 `DeleteRange`
- M2.1 `FormatTextRange`
- M2.2 `SetParagraphStyle`
- M2.3 `UpsertNamedStyle`
- M3.1 `SetListNumbering`

三盞紅燈考卷皆已由紅轉綠：

| 紅燈 | 考卷 | 結果 |
|---|---|---|
| 1 拆併段跨解析器索引對齊 | `k29b_lamp1_split_merge_keeps_inline_image_pixels` | 綠 |
| 2 格式與清單編號最小 part 觸碰 | `k29b_lamp2_format_and_numbering_touch_only_document_part`、`k29b_lamp2_upsert_named_style_touches_only_styles_part` | 綠 |
| 3 非法跨容器與樣式錯配誠實拒絕 | `k29b_lamp3_cross_container_and_style_type_rejected_byte_identical` | 綠 |

## 2. 實作重點

### 2.1 型錄與錯誤

`core/engine-api/src/mutation.rs` 新增：

- `RunDirectPatch`（bold／italic／color_rgb／size_pt／char_style）
- `ParaPropsPatch`（style_id／page_break_before／line_spacing／alignment）
- `StyleType`（Paragraph／Character）
- `ListAction`（AttachExisting／ChangeLevel／RemoveNumbering）
- `Mutation::{SplitParagraph, MergeParagraphWithNext, FormatTextRange, SetParagraphStyle, UpsertNamedStyle, SetListNumbering}`

`CrossContainerBoundary`、`InvalidStyleReference`、`InvalidNumberingReference`、`XmlPatchRejected` 皆已於實際路徑回傳。

### 2.2 Δp≠0 索引同步平移

拆段、併段、跨段刪除後，回填 XML 並呼叫 `rebuild_derived()`，由 `from_docx_bytes` 重建 `para_sec`、`shape_boxes`、`image_pixels` 與 `OoxmlDoc::tables` 等派生層，同時保留 `original`、`part_baselines`、`touched` 與 undo/redo 堆疊。因此拆段後行內圖仍按真像素發射，不會退化為 `IMAGE_PLACEHOLDER_FILL` 灰框。

### 2.3 最小 part 觸碰

- run／段落屬性與清單編號只改 `word/document.xml`。
- `UpsertNamedStyle` 只改 `word/styles.xml`，且先以 `StyleTable::style_type` 驗證樣式型別。
- 跨容器拆併與字元樣式誤用段落樣式時，回傳錯誤且 `save_docx()` 與原檔 byte-identical。

### 2.4 巢狀文字框回歸修正

K29a 回歸 `k29a_lamp2_untouched_parts_and_neighbor_paragraphs_zero_loss` 曾因 `mce-nested.docx` 第 0 段位於巢狀文字框內而失敗。單段 `DeleteRange` 改為優先走單一 `<w:t>` 局部刪除（對巢狀 `<w:sdt>`／`wps:txbx` 安全），僅在跨多個 text node 時才 fallback 到 `split_paragraph_xml` 重建。修正後 K29a 三張考卷全綠。

## 3. BUGGY 回塞兩態實跑

腳本：`scripts/k29b_buggy_replay.sh`，以字串就地置換切換三處瑕疵，不使用 git checkout／reset／clean／stash。

- `FLAG_P1_BUGGY_STRUCTURAL_REBUILD`：`skip_structural_rebuild = true`，不重建派生索引。
- `FLAG_P2_BUGGY_RPR_PATCH`：`buggy_rpr = true`，run 屬性修補失效。
- `FLAG_P3_BUGGY_CONTAINER_GUARD`：`buggy_container_guard = true`，略過跨容器與樣式型別守衛。

### 3.1 BUGGY 態

```text
$ bash scripts/k29b_buggy_replay.sh buggy
[FLAG_P1_BUGGY_STRUCTURAL_REBUILD] 成功置換
[FLAG_P2_BUGGY_RPR_PATCH] 成功置換
[FLAG_P3_BUGGY_CONTAINER_GUARD] 成功置換
=== 正在執行紅燈測試（預期必紅）===

failures:
    k29b_lamp1_split_merge_keeps_inline_image_pixels
    k29b_lamp2_format_and_numbering_touch_only_document_part
    k29b_lamp2_upsert_named_style_touches_only_styles_part
    k29b_lamp3_cross_container_and_style_type_rejected_byte_identical

test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.35s
BUGGY 態測試結束，exit_code=101（預期非 0）
```

### 3.2 GOOD 態

```text
$ bash scripts/k29b_buggy_replay.sh good
[FLAG_P1_GOOD_STRUCTURAL_REBUILD] 成功置換
[FLAG_P2_GOOD_RPR_PATCH] 成功置換
[FLAG_P3_GOOD_CONTAINER_GUARD] 成功置換
=== 正在執行正常測試（預期必綠）===

running 4 tests
test k29b_lamp3_cross_container_and_style_type_rejected_byte_identical ... ok
test k29b_lamp1_split_merge_keeps_inline_image_pixels ... ok
test k29b_lamp2_upsert_named_style_touches_only_styles_part ... ok
test k29b_lamp2_format_and_numbering_touch_only_document_part ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s
GOOD 態測試結束，exit_code=0
```

## 4. 驗收閘門實跑

```text
FMT=0
CLIPPY=0
TEST=0
SUM passed=960 failed=0
RT=0
GOLD=0
LAYER=0
```

- `roundtrip --check`：399 檔全數 byte-identical，失敗 0（符合基線 399/399）。
- `goldens`：shaping 12 例、pagination 4 例、tables 5 例全數相符，零位移。
- `check-layering.sh`：分層檢查通過。
- `cargo test --workspace`：960 passed、0 failed，測試總數只增未減。

## 5. 變更檔案

- `core/engine-api/src/mutation.rs`
- `core/engine-api/src/lib.rs`
- `core/ooxml/src/styles.rs`（僅新增 `StyleTable::style_type`、`is_paragraph_style`、`is_character_style`）
- `core/engine-api/tests/k29b_structural_styles.rs`（新建）
- `scripts/k29b_buggy_replay.sh`（新建）
- `docs/k29b-slice2-report.md`（本檔）
