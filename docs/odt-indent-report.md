# AGY-ODTIND 交付報告：ODT 段落縮排與對齊消費（公文檔跑版修復鏈第三環）

- 任務代號：AGY-ODTIND（軸 1：縮排與置中消費）
- 執行工作區根路徑：`<repo>/.worktrees/agy-odtind`
- 規範標準：ODF 1.3 Part 3 §20.375（`fo:text-align`）、§20.219（`fo:margin-left`）、§20.220（`fo:margin-right`）、§20.376（`fo:text-indent`）
- 報告日期：2026-11-12

---

## 1. 執行角色與白名單遵守自證

### 1.1 工作路徑自證
開工執行 `pwd` 輸出：
```
<repo>/.worktrees/agy-odtind
```
自證完全位於指定 worktree 根目錄。

### 1.2 白名單檢查
本任務嚴格限制僅修改或建立下列 5 個白名單檔案，其餘面（`core/ooxml/**`、`core/engine-api/**`、`shells/**`、`core/odf/src/objects.rs`、`core/layout/**`、`corpus*/**`、`fixtures/**`）零改動：
- `core/odf/src/layout_glue.rs`（採集＋級聯＋投影欄位，修改）
- `core/odf/src/display.rs`（發射 x 計算，修改）
- `core/odf/tests/odf_paragraph_properties.rs`（紅燈考卷，新建）
- `scripts/odt_indent_buggy_replay.sh`（BUGGY 回塞與還原腳本，新建）
- `docs/odt-indent-report.md`（本報告，新建）

未追蹤之 `user-sample.odt` 僅經由環境變數 `ODT_SAMPLE_PATH` 引用，嚴禁且未曾 `git add` 或讀入版本庫。

---

## 2. 根因分析與架構實作

### 2.1 採集層（`core/odf/src/layout_glue.rs`）
- **根因**：原 `StyleRecord` 僅具備 `size_declared`、`size_pt`、`weight_declared`、`bold` 4 個欄位，XML 走查遇到 `<style:paragraph-properties>` 直接忽略，導致所有段落邊界與對齊宣告全數漏失。
- **實作補齊**：
  1. 定義段落對齊枚舉 `Align`（`Start`, `Center`, `End`, `Right`, `Justify`）與屬性結構體 `ParagraphProperties`。
  2. `StyleRecord` 擴充 4 欄位：`align: Option<Align>`、`indent_left_pt: Option<f32>`、`indent_right_pt: Option<f32>`、`indent_first_line_pt: Option<f32>`。
  3. 新增 `apply_paragraph_properties` 函式，解析 `fo:text-align`、`fo:margin-left`、`fo:margin-right`、`fo:text-indent`。
  4. 新增 `parse_length_signed` 單位換算函式，全面複用 `page_geometry::odf_length_to_pt` 處理單位換算（cm/mm/in/pt/pc 及逗號小數），並擴充支援負值（懸掛縮排）與零長度（`0cm`, `0pt` 等）。
  5. 在 `parse_style_file` 的 `Event::Start` 與 `Event::Empty` 雙臂掛載 `style:paragraph-properties` 採集。

### 2.2 級聯層（`core/odf/src/layout_glue.rs`）
- **實作補齊**：
  1. `StyleBook::resolve_chain` 與 `resolve_opt` 擴充回傳 `ResolvedStyle` 結構體，嚴格遵循 ODF 三層優先序（tier1 automatic → tier2 named 含 parent 鏈 → tier3 default-style，先到先得）。
  2. 提供 `resolve_paragraph` 專門產出段落代表樣式與 `ParagraphProperties`。
  3. `OdfProjection` 擴充 `paragraph_properties: Vec<ParagraphProperties>`，維持與 `paragraphs` 段落槽位 1:1 精確對應，完全不變更 `OdfLayoutParagraph` 既有欄位簽章，確保現存標頭頁尾等外部呼叫端零編譯衝突。

### 2.3 發射層（`core/odf/src/display.rs`）
- **根因**：原 `push_line_items` 將每一行的 `base_x` 硬編碼為版心左緣 `pg.margin_left_pt`。
- **實作補齊**：
  1. 新增 `compute_line_x` 函式，依頁級真實邊界計算可用寬度 `content_w = (page_w - margin_left - margin_right).max(0.0)`。
  2. 依規範消費段落屬性：
     - 若 `align == Center`：`x = margin_left + max(0, (content_w − line_w) / 2)`。
     - 若 `align == End / Right`：`x = margin_left + max(0, content_w − line_w)`。
     - 若為首行（`line.char_offset == 0`）：`x = margin_left + max(0, indent_left + indent_first_line)`。若 `indent_first_line < 0`（懸掛縮排），依規格夾到 `margin_left + min(indent_left, max(0, indent_left + indent_first_line))`，保證不超出版心左緣。
     - 若為次行起（`line.char_offset > 0`）：`x = margin_left + indent_left.max(0.0)`。
     - 無宣告或無屬性者：恆等回退 `margin_left`（零擾動鎖）。
  3. 在 `emit_display_lists` 取出段落之 `ParagraphProperties`，計算 `base_x` 後傳入 `push_line_items`，維持 `push_line_items` 簡潔參數，避免觸發 Clippy `too_many_arguments`。

---

## 3. 考卷與四盞燈驗證

考卷檔案：`core/odf/tests/odf_paragraph_properties.rs`

### 3.1 考卷實跑結果
```
running 5 tests
test probe_user_sample_x_distribution ... ok
test lamp4_container_roundtrip_untouched ... ok
test lamp3_unspecified_paragraph_stays_at_margin_left ... ok
test lamp2_margin_left_and_text_indent_emission ... ok
test lamp1_center_align_text_item_centered ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### 3.2 各燈契約驗證
- **燈 1（置中對齊）**：
  - 測試標的：`<style:paragraph-properties fo:text-align="center"/>` 之段落。
  - 斷言：`DrawItem::Text` 之 `x > margin_left_pt`，且文字中心點與版心中點差距 `|line_center - content_center| <= 1.0pt`。
  - 結果：PASS。
- **燈 2（左邊距與首行縮排）**：
  - 測試標的：`fo:margin-left="2cm" fo:text-indent="1cm"` 兩行段落。
  - 斷言：首行 `x == margin_left_pt + 2cm_pt + 1cm_pt`（±0.5pt），次行 `x == margin_left_pt + 2cm_pt`（±0.5pt）。
  - 結果：PASS。
- **燈 3（防回歸零擾動鎖）**：
  - 測試標的：無對齊/縮排宣告之普通段落。
  - 斷言：文字項 `x` 恆等於 `margin_left_pt`（差值 `< 0.001pt`）。
  - 結果：PASS。
- **燈 4（存檔往返結構完整）**：
  - 測試標的：`quire-odf` 容器解析與往返零崩潰。
  - 結果：PASS。

---

## 4. BUGGY 回塞實跑紀錄

腳本檔案：`scripts/odt_indent_buggy_replay.sh`（透過字串就地置換將 `base_x` 降級為 `margin_left`，不使用 `git checkout`）。

### 4.1 回塞態（`bash scripts/odt_indent_buggy_replay.sh buggy`）
```
[FLAG_ODT_INDENT_BUGGY] 成功回塞：略過段落縮排與置中計算（紅燈 1、2 驗證點）
=== 正在執行紅燈測試（預期燈 1、燈 2 必紅）===
   Compiling quire-odf v0.1.0 (<repo>/.worktrees/agy-odtind/core/odf)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.03s
     Running tests/odf_paragraph_properties.rs (target/debug/deps/odf_paragraph_properties-467cd3f34fd387bf)

running 5 tests
test probe_user_sample_x_distribution ... ok
test lamp4_container_roundtrip_untouched ... ok
test lamp3_unspecified_paragraph_stays_at_margin_left ... ok
test lamp1_center_align_text_item_centered ... FAILED
test lamp2_margin_left_and_text_indent_emission ... FAILED

failures:

---- lamp1_center_align_text_item_centered stdout ----
thread 'lamp1_center_align_text_item_centered' (139456) panicked at core/odf/tests/odf_paragraph_properties.rs:77:5:
置中文字起排 x (72.000pt) 必須嚴格大於版心左邊距 (72.000pt)

---- lamp2_margin_left_and_text_indent_emission stdout ----
thread 'lamp2_margin_left_and_text_indent_emission' (139457) panicked at core/odf/tests/odf_paragraph_properties.rs:130:5:
首行 x (72.000pt, 內文: 首行縮排文字) 與期望值 (157.039pt) 差距 (85.039pt) 必須在 ±0.5pt 內

failures:
    lamp1_center_align_text_item_centered
    lamp2_margin_left_and_text_indent_emission

test result: FAILED. 3 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
BUGGY 態測試結束，exit_code=101
```

### 4.2 還原正常態（`bash scripts/odt_indent_buggy_replay.sh good`）
```
[FLAG_ODT_INDENT_GOOD] 成功還原：恢復段落縮排與置中計算
=== 正在執行正常測試（預期全綠）===
   Compiling quire-odf v0.1.0 (<repo>/.worktrees/agy-odtind/core/odf)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.74s
     Running tests/odf_paragraph_properties.rs (target/debug/deps/odf_paragraph_properties-467cd3f34fd387bf)

running 5 tests
test lamp4_container_roundtrip_untouched ... ok
test probe_user_sample_x_distribution ... ok
test lamp3_unspecified_paragraph_stays_at_margin_left ... ok
test lamp2_margin_left_and_text_indent_emission ... ok
test lamp1_center_align_text_item_centered ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
GOOD 態測試結束，exit_code=0
```

---

## 5. 用戶公文檔（`user-sample.odt`）修復前後 x 分佈對照

測試探針：以環境變數 `ODT_SAMPLE_PATH=user-sample.odt` 驅動，嚴守零內文文字洩漏原則，僅統計數值與計數。

### 5.1 修復前（零消費舊態）
```
=== 用戶公文檔逐頁 x 座標分佈統計（零文字洩漏）===
總頁數: 10
頁碼  0: 相異 x 數 =  1, 典型 x 座標分佈: [70.9pt:45次]
頁碼  1: 相異 x 數 =  1, 典型 x 座標分佈: [70.9pt:45次]
頁碼  2: 相異 x 數 =  1, 典型 x 座標分佈: [70.9pt:45次]
頁碼  3: 相異 x 數 =  1, 典型 x 座標分佈: [70.9pt:45次]
頁碼  4: 相異 x 數 =  1, 典型 x 座標分佈: [70.9pt:45次]
頁碼  5: 相異 x 數 =  1, 典型 x 座標分佈: [70.9pt:45次]
頁碼  6: 相異 x 數 =  1, 典型 x 座標分佈: [70.9pt:45次]
頁碼  7: 相異 x 數 =  1, 典型 x 座標分佈: [70.9pt:45次]
頁碼  8: 相異 x 數 =  1, 典型 x 座標分佈: [70.9pt:45次]
頁碼  9: 相異 x 數 =  1, 典型 x 座標分佈: [70.9pt:44次]
全書相異 x 坐標總數: 1
```
全書 449 個發射項目全部死貼版心左緣 `70.9pt`，對齊與縮排消費率為 0%。

### 5.2 修復後（本卡交付態）
```
=== 用戶公文檔逐頁 x 座標分佈統計（零文字洩漏）===
總頁數: 10
頁碼  0: 相異 x 數 = 16, 典型 x 座標分佈: [70.9pt:21次, 94.9pt:3次, 173.5pt:1次, 178.9pt:3次, 228.2pt:1次, 263.7pt:1次]
頁碼  1: 相異 x 數 = 10, 典型 x 座標分佈: [70.9pt:26次, 94.9pt:1次, 99.2pt:1次, 285.7pt:1次, 464.5pt:2次, 471.1pt:1次]
頁碼  2: 相異 x 數 = 12, 典型 x 座標分佈: [70.9pt:11次, 94.9pt:3次, 99.2pt:1次, 273.7pt:2次, 275.7pt:2次, 279.4pt:3次]
頁碼  3: 相異 x 數 =  6, 典型 x 座標分佈: [70.9pt:15次, 279.4pt:7次, 283.6pt:6次, 471.1pt:11次, 481.1pt:1次, 524.4pt:5次]
頁碼  4: 相異 x 數 =  8, 典型 x 座標分佈: [70.9pt:16次, 279.4pt:5次, 280.5pt:5次, 464.5pt:3次, 471.1pt:5次, 487.8pt:2次]
頁碼  5: 相異 x 數 = 13, 典型 x 座標分佈: [70.9pt:14次, 94.9pt:1次, 99.2pt:1次, 273.7pt:1次, 275.7pt:1次, 276.1pt:2次]
頁碼  6: 相異 x 數 = 11, 典型 x 座標分佈: [70.9pt:13次, 94.9pt:1次, 99.2pt:1次, 273.7pt:1次, 275.7pt:1次, 279.4pt:5次]
頁碼  7: 相異 x 數 =  9, 典型 x 座標分佈: [70.9pt:16次, 276.1pt:1次, 279.4pt:5次, 280.5pt:5次, 464.5pt:1次, 471.1pt:3次]
頁碼  8: 相異 x 數 =  8, 典型 x 座標分佈: [70.9pt:14次, 279.4pt:5次, 280.5pt:6次, 464.5pt:2次, 471.1pt:5次, 481.1pt:5次]
頁碼  9: 相異 x 數 =  7, 典型 x 座標分佈: [70.9pt:16次, 142.9pt:1次, 279.4pt:5次, 280.5pt:5次, 481.1pt:9次, 487.8pt:2次]
全書相異 x 坐標總數: 29
```
全書相異 x 坐標由 1 個暴增至 29 個，第 0 頁由 1 個增至 16 個；受文機關、公文標題置中與條款縮排幾何全數甦醒。

---

## 6. 驗收閘門完整實跑輸出

依據任務書 §6 驗收腳本逐行自跑結果：

```bash
export PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 RUST_MIN_STACK=16777216
cargo fmt --all --check > /tmp/ind_fmt.log 2>&1; echo "FMT=$?"
cargo clippy --workspace --all-targets -- -D warnings > /tmp/ind_clippy.log 2>&1; echo "CLIPPY=$?"
cargo test --workspace > /tmp/ind_test.log 2>&1; echo "TEST=$?"
grep -E "^test result" /tmp/ind_test.log | awk -F'[ ;]+' '{p+=$4; f+=$6} END{print "SUM passed="p" failed="f}'
cargo test -q -p quire-odf --test odf_container > /tmp/ind_odfc.log 2>&1; echo "ODFC=$?"
cargo test -q -p quire-odf --test odt_layout_projection > /tmp/ind_odfl.log 2>&1; echo "ODFL=$?"
cargo run -q -p quire-layout --bin goldens > /tmp/ind_gold.log 2>&1; echo "GOLD=$?"
bash scripts/check-layering.sh > /tmp/ind_layer.log 2>&1; echo "LAYER=$?"
```

實測終端輸出：
```
FMT=0
CLIPPY=0
TEST=101
SUM passed=579 failed=1
ODFC=0
ODFL=0
GOLD=0
LAYER=0
```

---

## 7. 既有考卷錨點位移停手回報（依 §4 規範）

在 `cargo test --workspace` 中，全倉庫 580 個測試通過 579 個，唯一失敗測試為：
```
test lamp3_page_membership_matches_pagination_boundaries in core/odf/tests/odf_d1_display_list.rs
```

### 7.1 失敗詳細訊息
```
thread 'lamp3_page_membership_matches_pagination_boundaries' panicked at core/odf/tests/odf_d1_display_list.rs:383:17:
ext/lo_core_sw_qa_odfexport_WordTest_edit.odt p0 行 0：行首 x=108 未貼版心左緣 72
```

### 7.2 根因定位
在先前 ODF-D1 切片實裝時，發射層 `push_line_items` 尚未採集段落樣式，每行起排硬編碼為 `pg.margin_left_pt`。當時 D1 考卷在 `lamp3` 設立了防護斷言（第 383 行）：
```rust
assert!(
    (first_x - pg.margin_left_pt).abs() <= 0.01,
    "{rel} p{page} 行 {i}：行首 x={first_x} 未貼版心左緣 {}",
    pg.margin_left_pt
);
```
該斷言假設全語料 50 個檔案的所有文字行首字元都必須緊貼版心左緣。

然而，在 `corpus-odt/ext/lo_core_sw_qa_odfexport_WordTest_edit.odt` 中，第 0 段樣式 `P1` 繼承自 `ListParagraph`，其 `styles.xml` 明確宣告了：
```xml
<style:style style:name="ListParagraph" ...>
  <style:paragraph-properties fo:margin-left="0.5in"/>
</style:style>
```
內文文字正是測試清單縮排之官方樣本文檔：`Onelist firstline -1cm, textleft 2cm, no...`。
在本次 AGY-ODTIND 合法補齊段落左邊距採集與發射後，該行起排座標正確推進至：
`x = 72pt (版心左邊距) + 36pt (0.5in 縮排) = 108pt`。

### 7.3 規範遵守
依據任務書 §4 之明確禁令：
> 「既有 ODF 考卷斷言數值（可加斷言不可改既有值；若你的合法改動使既有錨點位移，停手回報，不要自行改鎖）」

且 `core/odf/tests/odf_d1_display_list.rs` 不在本次修改白名單內。因此執行者嚴守紀律，**不私自更動 `odf_d1_display_list.rs` 的斷言數值**，停手在此如實回報。
建議 PM 在後續整合或後續 ODF 切片中，將 D1 考卷第 383 行「未貼版心左緣」的硬鎖放寬為「大於等於版心左緣」或與段落投影之縮排期望值比對。
