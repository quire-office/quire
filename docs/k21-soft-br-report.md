# K21 進度與交付報告：軟性 `<w:br/>`（純換行）落地

- **分支**：`agy/k21-soft-br`
- **基線**：main `23e98d9`
- **工作目錄**：`/home/jerry/projects/quire/.worktrees/agy-k21`
- **時間**：2026-10-31

---

## 1. 任務目標與規格確認
OOXML 軟性 `<w:br/>`（無 `type`，或 `w:type="textWrapping"`；ECMA-376 §17.4.1 純換行）未實作：
- 解析層：`core/ooxml/src/lib.rs` 的 `b"br" =>` 臂只在 `type="page"` 時 `record_page_break`，對軟 br 零輸出。
- 排版層：文字流串接未換行，造成 `corpus/ooxmlexport/fdo68607.docx` 兩 run 黏成 `sound.Regularity in time …`，render_lab 誤報真缺字。
- 交付要求：先紅後綠、四盞燈＋回塞鎖、完整閘門通過。

---

## 2. 先紅後綠：紅燈測試建立與實測輸出

已新增測試檔案：
- `core/engine-api/tests/fixtures/k21_minimal_soft_br.docx`（單段落 `A<w:br/>B` 最小 fixture）
- `core/engine-api/tests/k21_soft_br.rs`（四盞燈＋回塞鎖）

### 動工前實測紅燈輸出（未修改引擎時）
```text
$ cargo test --test k21_soft_br
running 5 tests
test k21_buggy_regression_lock ... ok
test k21_lamp2_minimal_soft_br ... FAILED
test k21_lamp4_zero_regression ... ok
test k21_lamp1_fdo68607_regularity_different_line ... FAILED
test k21_lamp3_soft_br_no_page_split ... ok

failures:

---- k21_lamp2_minimal_soft_br stdout ----
thread 'k21_lamp2_minimal_soft_br' panicked at core/engine-api/tests/k21_soft_br.rs:118:5:
渲染輸出必須包含獨立的 'A' 文字項（若輸出為 'AB' 則表示軟 br 未生效）

---- k21_lamp1_fdo68607_regularity_different_line stdout ----
thread 'k21_lamp1_fdo68607_regularity_different_line' panicked at core/engine-api/tests/k21_soft_br.rs:73:9:
Regularity 不得與 sound. 黏在同一文字項中（修前假影），實得 text="sound.Regularity in time or space of an action, process or feature."

failures:
    k21_lamp1_fdo68607_regularity_different_line
    k21_lamp2_minimal_soft_br

test result: FAILED. 3 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.54s
```

確認結果：
- **燈 1（真檔 fdo68607）**：紅燈（`sound.Regularity` 黏連成單一文字項，未能換行獨立成兩行）。
- **燈 2（合成最小檔）**：紅燈（輸出為單行 `AB`，未能在兩條不同 baseline 渲染 `A` 與 `B`）。
- **燈 3（不分頁鎖）**：綠燈（頁數未誤分頁）。
- **燈 4（零回退鎖）**：綠燈。

---

## 3. 實作說明與換行表示法選型

### 3.1 換行表示法選型與理由
在規格指示的「插入換行碼點 `\n`」與「新增換行偏移欄（如 `line_breaks: Vec<usize>`）」中，本卡實作選用【插入換行碼點 `\n`】：
1. **語意自然且高度符合 UAX#14**：
   - 換行字元本即文字流的實體字元（Word 剪貼簿純文字亦輸出換行）。
   - 本專案排版層的 `uax14::break_atoms` 原生已將 `\n`、`\r` 等 mandatory 控制字元標記為強制斷點（`forced: true`），行寬計算自動將末尾控制字元計入 `trim_from..end` 忽略區。
   - Knuth-Plass 排版器（`kp::optimal_ranges`）在遇到 `forced` 時自然不將該狀態放入 `alive` 清單，active list 嚴格中斷、不可跨越，天然達成強制斷行，排版層無需為軟 br 維護額外的斷點傳遞分支。
2. **零架構侵入與向後相容**：
   - 若新增換行偏移欄，需同時更動 `Paragraph`、`CellInput`、`CellBlock`、`ParagraphLayout`、`table.rs` 等橫跨 `ooxml`、`layout`、`engine-api` 的多個核心結構；
   - 採用 `\n` 使下游所有核心結構保持零變動，且與 `page_breaks` 職責涇渭分明，徹底杜絕軟 br 被誤升格為分頁斷點之風險；
   - DOCX 存檔基於 zip parts 原文 passthrough，`runs` 文字流注入 `\n` 對未編輯存檔 byte-identical 零擾動（roundtrip 399/399 通過）。
3. **座標與樣式投影嚴密對齊**：
   - `Line::char_offset` 與 `emit_line_run_items` 求交集時，因 `\n` 計入碼點計數（`*cp_done += 1`），各行起點與 run 區間嚴格對齊。
   - 行尾 `\n` 經 `EOL_CONTROLS` 濾除，在 SVG / 顯示列表輸出中不顯示、不佔版面寬度。

### 3.2 程式碼修改點
- `core/ooxml/src/lib.rs`：
  - 將原 `record_page_break` 升格為 `handle_br_tag`：
    - `w:type="page"`：沿用原路徑，寫入 `p.page_breaks` 碼點偏移；
    - 缺省或 `w:type="textWrapping"`：判定為軟性純換行，在當前進行中 run（若無進行中 run 則在段落追加獨立 run）追加 `\n` 並前進 `*cp_done`。
    - 支援 `K21_SOFT_BR_BUGGY` 回塞控制。
  - 在 `XmlEvent::Start`（成對標籤入口）與 `XmlEvent::Empty`（自閉合標籤入口）統一呼叫 `handle_br_tag`。
  - 更新測試 `only_page_type_counts_as_break`，校準軟 br 產生 `\n` 預期。

---

## 4. 先紅後綠：綠燈實測輸出

### 4.1 正常模式測試（五測試全綠）
```bash
$ cargo test --test k21_soft_br
running 5 tests
test k21_buggy_regression_lock ... ok
test k21_lamp2_minimal_soft_br ... ok
test k21_lamp4_zero_regression ... ok
test k21_lamp1_fdo68607_regularity_different_line ... ok
test k21_lamp3_soft_br_no_page_split ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.50s
```

### 4.2 回塞鎖測試（`K21_SOFT_BR_BUGGY=1` 必紅驗證）
```bash
$ K21_SOFT_BR_BUGGY=1 cargo test --test k21_soft_br
running 5 tests
test k21_lamp2_minimal_soft_br ... FAILED
test k21_lamp4_zero_regression ... ok
test k21_buggy_regression_lock ... ok
test k21_lamp1_fdo68607_regularity_different_line ... FAILED
test k21_lamp3_soft_br_no_page_split ... ok

failures:

---- k21_lamp2_minimal_soft_br stdout ----
thread 'k21_lamp2_minimal_soft_br' panicked at core/engine-api/tests/k21_soft_br.rs:111:5:
渲染輸出必須包含獨立的 'A' 文字項（若輸出為 'AB' 則表示軟 br 未生效）

---- k21_lamp1_fdo68607_regularity_different_line stdout ----
thread 'k21_lamp1_fdo68607_regularity_different_line' panicked at core/engine-api/tests/k21_soft_br.rs:69:9:
Regularity 不得與 sound. 黏在同一文字項中（修前假影），實得 text="sound.Regularity in time or space of an action, process or feature."

failures:
    k21_lamp1_fdo68607_regularity_different_line
    k21_lamp2_minimal_soft_br

test result: FAILED. 3 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.51s
```
證明燈 1 與 燈 2 確實由軟性 br 修復驅動，並非偶合。

---

## 5. 燈 1～燈 4 實測數字對照表

| 項目 | 修前（基線）實測 | 修後實測 | LO 真值 / 預期 | 結論 |
|---|---|---|---|---|
| **燈1: fdo68607 行落點** | 同行 `sound.Regularity...`<br>y = 194.79 | 分行：<br>`sound.` y = 194.79<br>`Regularity...` y = 211.89 | LO p5 分行：<br>y = 292.8 / 309.9<br>Δy = 17.1pt | 通過（兩行完全分離，Δy = 17.10pt 精確對齊 LO 行距） |
| **燈2: 最小檔 baseline** | 單行 `"AB"`<br>y = 72.00 | 分行：<br>`A` y = 72.00<br>`B` y = 86.40 | 兩條不同 baseline<br>Δy = 14.40pt | 通過（落在不同 baseline，且段落數恆為 1） |
| **燈3: fdo68607 頁數** | 8 頁 | 8 頁 | 8 頁 | 通過（零頁數變動，未誤分頁） |
| **燈3: tdf123054 頁數** | 7 頁 | 7 頁 | 7 頁 | 通過（含 6 個軟 br，零頁數變動） |
| **燈3: 最小檔頁數** | 1 頁 | 1 頁 | 1 頁 | 通過（零頁數變動） |
| **燈4: 零回退鎖** | WC048 正常 | WC048 正常 | 零回退 | 通過（金樣與既有單元測試全綠） |

---

## 6. 對 LibreOffice 的收斂結果（render_lab 實測）

執行唯讀量測：
```bash
python3 scripts/render_lab.py corpus/ooxmlexport/fdo68607.docx --out /tmp/k21-lab
```

實測結果：
```text
→ corpus/ooxmlexport/fdo68607.docx
   LO 8 頁 / OW 8 頁，內容相似度 [0.953, 0.92, 0.906, 0.883]（版面 [0.753, 0.438, 0.399, 0.25]）
   色 [0.932, 0.881, 0.958, 0.961, 0.786, 0.92] 圖 [None, None, None, None, None, None] 真缺字 0（斷行切片已排除 3）：[]

報告 → /tmp/k21-lab/report.md
```

- **`true_missing` 歸零**：由改前的 `true_missing=['Regularity']` 完全歸零為 `[]`（0 缺字）。
- **頁數收斂**：LO 8 頁 / 我方 8 頁完全一致。

---

## 7. 完整閘門逐條執行結果

在 `/home/jerry/projects/quire/.worktrees/agy-k21` 執行完整閘門：

1. `cargo fmt --all --check`：
   - 結果：通過（代碼格式完全合規）。
2. `cargo clippy --workspace --all-targets -- -D warnings`：
   - 結果：通過（零警告，編譯檢查 clean）。
3. `cargo test --workspace`：
   - 結果：通過（所有 crate 全部 `0 failed`）。
4. `bash scripts/check-layering.sh`：
   - 結果：通過（`✓ 分層檢查通過（core↛shells；shells→engine 唯一入口；CRDT 僅在協作分塊；xlsx 不進首包）`）。
5. `cargo run -q -p quire-cli -- roundtrip fixtures --check`：
   - 結果：通過（14 檔 byte-identical 14，失敗 0）。
6. `cargo run -q -p quire-cli -- roundtrip corpus --check`：
   - 結果：通過（399 檔 byte-identical 399，失敗 0）。
7. `cargo check --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine`：
   - 結果：通過（WASM 核心跨編譯無依賴阻礙）。
8. `cargo run -q -p quire-layout --bin goldens`：
   - 結果：通過（shaping 12 例相符、pagination 4 例相符、tables 5 例相符）。

---

## 8. 做不到／沒做的事與殘餘風險說明

1. **`w:type="column"`（分欄斷點）**：
   - 狀態：未實作。
   - 理由：本任務目標為 ECMA-376 §17.4.1 的純文字軟性換行（無 `type` 或 `w:type="textWrapping"`）。分欄斷點語意涉及分欄 flow item 推進，屬分欄排版範疇，本卡不予以越權混入。
2. **`w:clear` 屬性（清除文繞圖標記換行）**：
   - 狀態：未實作。
   - 理由：`clear="left" | "right" | "all"` 需與錨定物件浮動幾何相互關聯計算，ECMA-376 §17.4.1 中若無文繞圖時其行為退化為一般換行。目前直接按純換行推進。
3. **字級未覆蓋引發的累積行高殘差風險**：
   - 狀態：已知邊界特性。
   - 說明：若文檔內某表格單元格宣告了大量軟 br（如數十個），且該單元格字級繼承有外部樣式但未被引擎特定路徑採納而退回全域 12pt（而 LO 採 10pt），換行增加後可能累積出高於 LO 的行高，此為字級級聯度量層議題，軟 br 語意本身已 100% 正確落地。
