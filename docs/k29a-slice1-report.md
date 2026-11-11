# 任務 AGY-K29A 收案報告：編輯 API 第一片——最小文字級閉環寫回（紅燈 1–3）

## 0. 執行環境與邊界自證

- 工作目錄自證：`pwd` 實測輸出 `/home/jerry/projects/quire/.worktrees/agy-k29a`，所有路徑嚴格以該 worktree 為根。
- 嚴格遵守白名單：僅建立與修改以下白名單檔案，未碰觸任何禁改面（`core/ooxml/**`、`core/layout/**`、`core/odf/**`、`shells/**`、`corpus/**`、`fixtures/**` 全數零動）：
  - `core/engine-api/src/mutation.rs`（新建）
  - `core/engine-api/src/lib.rs`（接線與 re-export、touched 自動退場比對基準、part 唯讀入口）
  - `core/engine-api/tests/k29a_text_roundtrip.rs`（新建紅燈 1–3 考卷）
  - `scripts/k29a_buggy_replay.sh`（新建 buggy/good 雙態回塞腳本）
  - `docs/k29a-slice1-report.md`（新建本報告）
- 語言與格式紀律：全篇繁體中文、無簡體字、無 emoji；Git 歷史維持線性單一分支提交。

---

## 1. 交付物清單核驗表

- [x] **紅燈 1–3 考卷＋實跑輸出**：包含初始紅燈、BUGGY 回塞態紅燈、正常 GOOD 態全綠實跑輸出。
- [x] **apply_mutation／undo／redo 實作說明與核心 diff**：說明扁平段落槽位與 part 位元組切片映射定位演算法。
- [x] **touched 自動退場機制說明＋紅燈 3 byte_identical 實據**：物理 Deflate 壓縮流原位還原機制。
- [x] **六道驗收閘門實際輸出**：FMT、CLIPPY、TEST、RT、GOLD、WASM、LAYER 全部 0 瑕疵通過。
- [x] **未完成項與邊界照實宣告**：明確標定 Slice 1 範圍（Δp=0）與後續 Slice 2/3/4 的邊界切分。

---

## 2. 紅燈 1–3 考卷與實跑紀錄

### 2.1 考卷規格（`core/engine-api/tests/k29a_text_roundtrip.rs`）
- **紅燈 1（文字編輯持久化閉環鎖）**：`fixtures/style_cascade.docx` 於第 0 段執行 `M1.1 InsertText { target: Body, pos: DocPos{para:0, cp:8}, text:"（已存檔）", style_override: None }` → `save_docx()` → `from_docx_bytes(&saved)`。驗證重開後 `reopened.paragraph_text(0) == Some("段落級自閉合粗體（已存檔）")` 且顯示列表中該字串仍帶有 `FLAG_BOLD` 樣式。
- **紅燈 2（未觸碰 part 零失真＋同 part 鄰段不擾動鎖）**：針對 `corpus/ooxmlexport/style-inheritance.docx` 與 `corpus/ooxmlexport/mce-nested.docx` 依序執行單段 M1.1 與 M1.2。驗證：(a) `roundtrip_check().pass == true` 且 `whitelisted_changed == ["word/document.xml"]`；(b) 其餘全部 entry 的 `zero_loss() == true`；(c) `word/document.xml` 內被修改段落以外之所有 `<w:p>`、`<w:tbl>`、`<mc:AlternateContent>` 切片與修改前逐位元組相等。
- **紅燈 3（Edit→Undo→Save 全檔 byte_identical 鎖）**：依序執行 M1.1 InsertText、M1.5 InsertBreak(Page)、單段 M1.2 DeleteRange，連續 3 次呼叫 `undo()` 回到初始態，存檔後比對 `diff_untouched(&original, &saved)`。驗證 `byte_identical == true && pass == true`。

### 2.2 實跑輸出對照

#### 初始紅燈階段實跑輸出
```text
running 3 tests
test k29a_lamp3_edit_undo_save_byte_identical ... FAILED
test k29a_lamp1_text_edit_persistence_closed_loop ... FAILED
test k29a_lamp2_untouched_parts_and_neighbor_paragraphs_zero_loss ... FAILED

failures:

---- k29a_lamp3_edit_undo_save_byte_identical stdout ----
thread 'k29a_lamp3_edit_undo_save_byte_identical' panicked at core/engine-api/tests/k29a_text_roundtrip.rs:299:10:
第 1 步 InsertText 失敗: XmlPatchRejected { detail: "尚未實作 Surgical XML Patch 寫回" }

---- k29a_lamp1_text_edit_persistence_closed_loop stdout ----
thread 'k29a_lamp1_text_edit_persistence_closed_loop' panicked at core/engine-api/tests/k29a_text_roundtrip.rs:39:31:
套用 InsertText 失敗: XmlPatchRejected { detail: "尚未實作 Surgical XML Patch 寫回" }

---- k29a_lamp2_untouched_parts_and_neighbor_paragraphs_zero_loss stdout ----
thread 'k29a_lamp2_untouched_parts_and_neighbor_paragraphs_zero_loss' panicked at core/engine-api/tests/k29a_text_roundtrip.rs:222:14:
套用 M1.1 失敗: XmlPatchRejected { detail: "尚未實作 Surgical XML Patch 寫回" }

failures:
    k29a_lamp1_text_edit_persistence_closed_loop
    k29a_lamp2_untouched_parts_and_neighbor_paragraphs_zero_loss
    k29a_lamp3_edit_undo_save_byte_identical

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

#### BUGGY 回塞腳本實跑輸出（`bash scripts/k29a_buggy_replay.sh buggy`）
```text
[FLAG_P1_BUGGY] 成功回塞：略過 doc.parts 寫回（紅燈 1、2 驗證點）
[FLAG_P2_BUGGY] 成功回塞：略過 Undo XML 反向還原（紅燈 3 驗證點）
=== 正在執行紅燈測試（預期必紅）===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 4.12s
     Running tests/k29a_text_roundtrip.rs (target/debug/deps/k29a_text_roundtrip-7a2f73e12b2790ea)

running 3 tests
test k29a_lamp3_edit_undo_save_byte_identical ... FAILED
test k29a_lamp1_text_edit_persistence_closed_loop ... FAILED
test k29a_lamp2_untouched_parts_and_neighbor_paragraphs_zero_loss ... FAILED

failures:

---- k29a_lamp3_edit_undo_save_byte_identical stdout ----
thread 'k29a_lamp3_edit_undo_save_byte_identical' panicked at core/engine-api/tests/k29a_text_roundtrip.rs:308:10:
第 2 步 InsertBreak 失敗: CodepointOutOfBounds { para: 0, start_cp: 10, end_cp: 10, para_len: 8 }

---- k29a_lamp1_text_edit_persistence_closed_loop stdout ----
thread 'k29a_lamp1_text_edit_persistence_closed_loop' panicked at core/engine-api/tests/k29a_text_roundtrip.rs:45:5:
assertion `left == right` failed: 重開後第 0 段文字必須持久化
  left: Some("段落級自閉合粗體")
 right: Some("段落級自閉合粗體（已存檔）")

---- k29a_lamp2_untouched_parts_and_neighbor_paragraphs_zero_loss stdout ----
thread 'k29a_lamp2_untouched_parts_and_neighbor_paragraphs_zero_loss' panicked at core/engine-api/tests/k29a_text_roundtrip.rs:232:14:
套用 M1.2 失敗: CodepointOutOfBounds { para: 0, start_cp: 0, end_cp: 9, para_len: 8 }

failures:
    k29a_lamp1_text_edit_persistence_closed_loop
    k29a_lamp2_untouched_parts_and_neighbor_paragraphs_zero_loss
    k29a_lamp3_edit_undo_save_byte_identical

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
BUGGY 態測試結束，exit_code=101
```

#### 正常 GOOD 態實跑輸出（`bash scripts/k29a_buggy_replay.sh good`）
```text
[FLAG_P1_GOOD] 成功還原：恢復 doc.parts 寫回
[FLAG_P2_GOOD] 成功還原：恢復 Undo XML 反向還原
=== 正在執行正常測試（預期必綠）===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.04s
     Running tests/k29a_text_roundtrip.rs (target/debug/deps/k29a_text_roundtrip-7a2f73e12b2790ea)

running 3 tests
test k29a_lamp1_text_edit_persistence_closed_loop ... ok
test k29a_lamp3_edit_undo_save_byte_identical ... ok
test k29a_lamp2_untouched_parts_and_neighbor_paragraphs_zero_loss ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
GOOD 態測試結束，exit_code=0
```

---

## 3. Surgical XML Patch 定位策略與實作說明

### 3.1 段落槽位映射至 XML 位元組切片演算法
既有 OOXML 解析器（`parse_document_xml_with_fonts`）在處理 `word/document.xml` 時具備兩大口徑約束：
1. **Fallback 子樹吞沒**：`mc:AlternateContent` 中的 `mc:Fallback` 分支段落純為兼容備份，不計入正文扁平段落空間。
2. **文字框分流**：`w:txbxContent` 內的段落歸屬文字框附屬欄，不計入扁平段落索引。
3. **扁平段落計數**：排除上述子樹後，非自閉合的 `<w:p ...>` Start 標籤出現順序即為扁平段落索引 `para: usize`。

在 `core/engine-api/src/mutation.rs::find_paragraph_span` 中，實作了完全對齊解析器狀態機的零依賴位元組定位掃描器：
- 維護 `fb_depth`、`ac_stack`、`txbx_depth` 與 `para_index`。
- 遇目標段落時精確標註起始偏移 `start`，並以括號深度配對找出對應 `</w:p>` 的結束偏移 `end`。
- 鎖定目標段落區間後，進一步於段內解析 `<w:pPr>`、`<w:r>`、`<w:t>`、`<w:br>` 之位元組區間與正準碼點偏移（Canonical Codepoint），確保文字插入、刪除與斷點皆精準作用於目標 `<w:t>` 或 `<w:r>`。

### 3.2 未觸碰位元組 100% 原樣保障
- 編輯時僅將目標節點的 `ByteSpan` 置換為新片段，前後位元組（`raw_xml[..span.start]` 與 `raw_xml[span.end..]`）直接拼裝。
- 段外所有段落、表格、圖形、命名空間宣告、修訂屬性（`w:rsidR`、`w14:paraId`）隻字不改，完全避免全檔 XML 重序列化引起的屬性丟失與重排問題。

---

## 4. touched 自動退場機制與紅燈 3 原位元組還原

### 4.1 物理原理
- 在開檔時，`Engine` 快照記錄各 XML part 之明文基準 `part_baselines: BTreeMap<String, Vec<u8>>`。
- 每次 `apply_mutation` 與 `undo` 執行後，比對 `self.doc.parts["word/document.xml"] == self.part_baselines["word/document.xml"]`：
  - 若明文逐位元組完全等於開檔解壓縮值，立即將該 part 從 `self.touched` 移除。
  - 若 `self.touched` 清空，`roundtrip_check()` 自動走 `diff_untouched`。
- 存檔時，`OoxmlDoc::save` 偵測到 `doc.parts["word/document.xml"] == e.decoded()?`，不產生 `Change::Replace`；底層 `ZipBundle::serialize` 自動將該 part 判定為 `Planned::untouched`，直接沿用開檔凍結的原始 Deflate 壓縮流（`data_raw`）、標頭、extra field 與 gap，使檔案達成 100% `byte_identical == true`。

---

## 5. 六道驗收閘門實際輸出

執行環境環境變數：
`export PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 RUST_MIN_STACK=16777216`

| 閘門代號 | 執行命令 | 退出碼 | 實跑結果與輸出摘要 |
|---|---|---|---|
| **Gate 1: FMT** | `cargo fmt --all --check` | 0 | 通過（無任何格式差異） |
| **Gate 2: CLIPPY** | `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 通過（零 warning） |
| **Gate 3: TEST** | `cargo test --workspace` | 0 | `SUM passed=936 failed=0`（基線 918+，全數綠燈） |
| **Gate 4: RT** | `cargo run -q -p quire-cli -- roundtrip corpus --check` | 0 | `399 檔 byte-identical 399，失敗 0`（100% 零失真） |
| **Gate 5: GOLD** | `cargo run -q -p quire-layout --bin goldens` | 0 | shaping (12)、pagination (4)、tables (5) 零位移通過 |
| **Gate 6: WASM** | `cargo check -q -p quire-odf -p quire-engine -p quire-web --target wasm32-unknown-unknown` | 0 | wasm32-unknown-unknown 跨編譯無編譯錯誤 |
| **Gate 7: LAYER** | `bash scripts/check-layering.sh` | 0 | 分層檢查通過（core 零反向依賴，shells 唯一入口） |

---

## 6. 未完成項與後續切片邊界聲明

依據 `docs/edit-api-design.md` §6.1–§6.5 規劃，本卡（Slice 1 / `K29a`）已完整交付所屬範疇，其餘高級操作按契約留待後續分期：
1. **跨段文字刪除（Cross-paragraph DeleteRange）**：涉及扁平段落數縮減（Δp < 0）及結構平移，本卡遇 `start.para != end.para` 誠實回傳 `MutationError::ProtectedStructure`，留待 Slice 2（`K29b`）交付。
2. **段落拆分與合併（SplitParagraph / MergeParagraphWithNext）**：留待 Slice 2（`K29b`）交付。
3. **字元樣式覆寫與段落樣式（FormatTextRange / SetParagraphStyle）**：留待 Slice 2（`K29b`）交付。
4. **表格與圖片結構編輯（M4 / M5 家族）**：留待 Slice 3（`K29c`）交付。
5. **分節、頁首頁尾與註腳子樹編輯（M6 家族）**：留待 Slice 4（`K29d`）交付。
