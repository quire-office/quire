# ODF-C4 紅燈紀錄（ODT 頁面幾何鏈：page-layout＋master-page 消費）

考卷：`core/odf/tests/odf_c4_page_geometry.rs`（6 盞）＋
`shells/native/tests/odt_render_page_geometry.rs`（CLI 面 2 盞）
計數基準腳本：`core/odf/tests/odf_c4_truth_baseline.py`（獨立於 Rust 的第二
雙眼，直接 ElementTree 拆 styles.xml/content.xml＋LO PDF 真值對照；紅燈
commit 同入，事後不得改）
真值：LO 24.8 `--convert-to pdf` ＋ pymupdf `page.rect`（見解剖報告 §8）。

紅燈方式：`page_geometry_bytes` 先以**佔位實作**入庫（不解析、一律回
`OdfPageGeometry::lo_default()`），考卷因此**斷言拍紅**（不是編譯紅）——
證明考卷真的有在檢查東西。還原以 sha256 比對確認逐位元組一致
（`d9d9abb6…8031b6`）。

## 一、紅燈實測輸出（`cargo test -p quire-odf --test odf_c4_page_geometry`，佔位實作）

```
test lamp5_style_domains_are_split ... FAILED
test length_units_and_decimal_comma ... ok
test lamp2_dual_axis_prefers_style_axis ... FAILED
test lamp3_missing_page_layout_falls_back_to_lo_default ... FAILED
test lamp1_sample3_reads_page_layout_not_default ... FAILED
test lamp6_corpus_matches_truth_baseline ... FAILED

thread 'lamp1_sample3_reads_page_layout_not_default' panicked at core/odf/tests/odf_c4_page_geometry.rs:68:5:
assertion `left == right` failed: 燈1 master-page 名
  left: None
 right: Some("Standard")

thread 'lamp2_dual_axis_prefers_style_axis' panicked at core/odf/tests/odf_c4_page_geometry.rs:95:31:
燈2 應解到 Mpm1

thread 'lamp3_missing_page_layout_falls_back_to_lo_default' panicked at core/odf/tests/odf_c4_page_geometry.rs:139:5:
assertion `left == right` failed
  left: NoMasterPage
 right: MasterPageWithoutLayout

thread 'lamp5_style_domains_are_split' panicked at core/odf/tests/odf_c4_page_geometry.rs:199:5:
assertion `left == right` failed: 燈5 域優先序錯誤
  left: None
 right: Some("LandPage")

thread 'lamp6_corpus_matches_truth_baseline' panicked at core/odf/tests/odf_c4_page_geometry.rs:315:9:
assertion `left == right` failed: 燈6 ext/lo_core_sw_qa_odfexport_WordTest_edit.odt 生效原因
  left: "no_master_page"
 right: "para_master_page_name"

test result: FAILED. 1 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out
```

`length_units_and_decimal_comma` 綠＝換算純函數與佔位實作無關，屬預期
（燈序先拍與本卡解析鏈直接相關的 5 盞）。

## 二、開發中真實紅燈（非佔位：實作與真值互拍，如實記帳）

1. **燈 6 首跑紅**：`WordTest_edit.odt` Rust 報 `para_master_page_name`，
   Python 基準報 `default_master_styles`。查證：content.xml 首段 `P1` 實
   宣告 `@style:master-page-name="MP0"`（首流程物件包在 `text:list` 內）——
   **Rust 正確、Python 基準腳本 bug**（`local()` 去前綴後仍拿
   `"office:automatic-styles"` 全名比對，content 域宣告全漏）。修基準後
   48 檔 `source` 欄位全對齊。詳見解剖報告 §6。
2. **燈 6 二跑紅**：`checked=51 ≠ 49`。加密 2 檔在生成器即 XML 解析失敗
   被剔出表格（不在 EXPECTED 51 列內），斷言口徑寫錯為 49。修斷言
   51＝48 語料明文＋3 樣本。

## 三、綠燈還原後輸出（實裝 `page_geometry_bytes` 全鏈）

```
running 6 tests
test lamp5_style_domains_are_split ... ok
test length_units_and_decimal_comma ... ok
test lamp2_dual_axis_prefers_style_axis ... ok
test lamp3_missing_page_layout_falls_back_to_lo_default ... ok
test lamp1_sample3_reads_page_layout_not_default ... ok
test lamp6_corpus_matches_truth_baseline ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

CLI 面（`shells/native/tests/odt_render_page_geometry.rs`）：

```
test odt_render_fallback_is_lo_default_not_layout_default ... ok
test odt_render_reports_real_page_geometry ... ok
```

CLI 實抓（`cargo run -p quire-cli -- odt-render`）：

```
sample3_page_sect.odt →
{"page":{"h_pt":841.8897705078125,"margins_pt":[56.69291305541992,56.69291305541992,
56.69291305541992,56.69291305541992],"w_pt":595.3038940429688},
"page_breaks":[],"page_source":"default_master_styles","paragraph_count":5,
"total_text_len":123}
```

頁寬 595.304（LO 真值）≠ default 假值 595.0；邊距 56.69（2cm）≠ 72（1 吋）。

## 四、燈 4 零回退佐證

既有考卷全綠不動（`cargo test -p quire-odf`）：odt_layout_projection 7 盞、
odf_container 5 盞、odf_e2a_generalref、odf_encrypted_gate 全綠——本卡只加
模組與 CLI 出口欄位，`project`/`layout_projection`/`page_break_positions`
簽名與行為零改動（`page_breaks` 改用真實頁參數屬本卡目的，出口欄位純增量）。

## 五、驗收閘門實測（odt_flow_lab ratio 重跑＋效益面 A/B）

### 5.1 ratio 口徑更正（重要：前一版腳本量錯分母）

卡面 ratio 口徑（`docs/odf-survey/e2-flow-attribution.md` 開頭逐位複現值）＝
我方 `total_text_len`（**含空白**）÷ LO PDF `get_text()` 逐行 strip 後 join
的字數（**含空白**）。前一版重跑腳本 `c4_flow_ratio.py` 的分母誤用「去空白」
口徑（那是 E2 解剖用來做**內容對帳**的口徑，不是卡面 ratio 口徑），median
因此虛高到 1.114 且偏低三席對不上基線。更正後的腳本
`core/odf/tests/odf_c4_flow_ratio.py`（卡面口徑，入庫版）實測：

```
可比對檔數: 47
median ratio: 1.000
mean   ratio: 0.946
偏低三席（卡面基線 strict 0.570 / tdf124637 0.634 / tdf87533 0.791）:
  lo__ooxmlexport__strict:                    0.570（Δ+0.000）
  lo__ooxmlexport__tdf124637_sectionMargin:   0.634（Δ+0.000）
  lo__ooxmlimport__tdf87533_bidi:             0.791（Δ+0.000）
頁幾何生效原因分佈: default_master_styles 42 檔／para_master_page_name 5 檔
```

**median 保持 1.000**（卡面閘門成立）。偏低三席**逐位不變**（Δ 恰為
+0.000，非「收窄」）——與 E2 解剖的代碼級結論一致：`total_text_len` 在
`walk_paragraphs`（投影層）算出、**不讀 `LayoutParams`**，頁幾何只影響
`page_breaks` 軸。三席偏低成因是頁首尾內容／TOC 點引導符合成／bidi 控制
字元（ODF-E2d／E2e 範圍），本卡不強求轉綠，如實記帳。

「48 可比對檔」實測為 **47**：`ext/lo_core_sw_qa_odfimport_feature_table_merged-cells.odt`
的 LO PDF 實測抽出 **0 字**（pymupdf：1 頁、raw 0 chars、0 images、0
drawings——該檔 LO 根本沒發文字層），分母為 0 不可比值，照實剔除。這是
LO 端事實，非本卡改動造成（main 態同檔同口徑同樣不可比對）。

### 5.2 效益面：`page_breaks` 軸 A/B（main 假 A4 vs 本卡真頁幾何）

本卡唯一能動的輸出口是 `page_breaks`，故效益以「我方頁數（
`len(page_breaks)+1`）對 LO PDF 實際頁數」量（腳本
`core/odf/tests/odf_c4_pagebreak_ab.py`，48 可比對檔）：

| 態 | \|我方頁數 − LO 頁數\| ≤ 1 的檔數 | 總偏差（頁） |
|---|---|---|
| main（`LayoutParams::default()` 假 A4＋1 吋） | 34/48 | 283 |
| 本卡（page-layout 真實頁寬高＋邊距） | **35/48** | **264** |

改善例：`tdf123705` 5→3、`poi/drawing` 6→4、`tdf152710` 1→0、`fdo77887`
3→2、`tdf102466` 2→1、大檔 `bug65649` 205→185。變差例（如實記帳）：
`bug59058` 5→8、`tdf104061` 8→10、`fdo78420` 4→6——成因是本卡把頁面幾何
換對之後，**行內換頁仍走後備字級/行距**（12pt／1.4），頁數誤差不再由頁幾何
單一變數決定；多 master-page 換頁鏈與頁首尾預留未做（解剖報告 §7 記帳）是
剩餘偏差主因。此軸非本卡考卷的判準（考卷判「頁寬高＝styles.xml 換算值」），
此處只作效益佐證，不設閘門。

### 5.3 全閘門實測清單（本分支態）

| 閘門 | 結果 |
|---|---|
| `cargo fmt --all --check` | 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 |
| `cargo test --workspace` | 856 盞全綠（含 CLI 面 2 盞） |
| `cargo test -p quire-odf` | 21 盞（本卡新考卷 6＋既有 15） |
| `cargo run -p quire-cli -- roundtrip corpus --check` | 399/399 byte-identical |
| `roundtrip fixtures --check` / `roundtrip-xlsx` | 14/14、80/80＋2/2 |
| `cargo run -p quire-layout --bin goldens` | shaping 12／pagination 4／tables 5 相符 |
| `cargo check -p quire-odf --target wasm32-unknown-unknown` | 0（雙目標鐵律） |
| `bash scripts/check-layering.sh` | 通過 |
| 基準腳本 `odf_c4_truth_baseline.py --lo-index` | 50 檔對 LO 頁尺寸偏差>0.6pt 者 **0 檔** |
