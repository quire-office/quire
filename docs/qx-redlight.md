# QX：quick-xml 0.37→0.41 遷移紅燈快照與安全防線設計

署名：ADT109119

## 1. 任務背景與遷移目標
本卡任務為升級 quick-xml 至 0.41.0，修復兩項高危險安全漏洞：
- **RUSTSEC-2026-0194**：start tag 重複屬性檢查的二次方複雜度 DoS（quadratic complexity in duplicate attribute checks）。
- **RUSTSEC-2026-0195**：NsReader 深巢狀命名空間宣告無界記憶體耗盡 DoS（unbounded memory allocation in namespace declarations）。

由於本專案為文件解析器、處理不可信 XML，上述漏洞屬真實攻擊面。PM 已查證 0.41 僅需遷移 19 個站點（11 處 `BytesText::unescape()`、8 處 `Attribute::unescape_value()`），相比 0.42 需 96 處變動更為精準可控且零回退。

## 2. 升級前安全防線紅燈實拍（0.37 二次方複雜度 DoS）
在 quick-xml 0.37.5 基準下，執行包含大量屬性檢查之對抗測試時，因每遇到新屬性即對已見屬性進行線性比對，導致 $O(N^2)$ 計算耗時暴增，觸發限時紅燈：

執行指令：
`cargo test -p quire-ooxml --test qx_unescape_parity`

執行結果（0.37 下）：
```
running 3 tests
test test_unescape_and_decode_parity_entities_and_multilingual ... ok
test test_cve_2026_0195_deep_nested_namespaces_dos_defense ... ok
test test_cve_2026_0194_duplicate_attributes_dos_defense ... FAILED

failures:

---- test_cve_2026_0194_duplicate_attributes_dos_defense stdout ----

thread 'test_cve_2026_0194_duplicate_attributes_dos_defense' (56806) panicked at core/ooxml/tests/qx_unescape_parity.rs:324:5:
重複屬性解析必須在 500ms 內線性時間完成，實際耗時: 564.721856ms
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

failures:
    test_cve_2026_0194_duplicate_attributes_dos_defense

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s
```

## 3. 依賴更新至 0.41.0 後之編譯紅燈實拍（19 處未遷移）
在根目錄 `Cargo.toml` 將 `quick-xml = "0.37"` 升級為 `"0.41"`，並執行 `cargo update -p quick-xml` 更新至 0.41.0。在尚未對 19 個站點進行 API 遷移前，編譯器回報 `BytesText::unescape` 已移除與 `Attribute::unescape_value` 棄用警告：

執行指令：
`cargo test -p quire-ooxml --test qx_unescape_parity`

編譯輸出快照：
```
   Compiling quick-xml v0.41.0
   Compiling quire-ooxml v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-qx/core/ooxml)
warning: use of deprecated method `quick_xml::events::attributes::Attribute::<'a>::unescape_value`: use `Self::normalized_value()`
    --> core/ooxml/src/drawing.rs:3270:15
     |
3270 |             a.unescape_value().ok().map(|v| v.into_owned())
     |               ^^^^^^^^^^^^^^
     |
     = note: `#[warn(deprecated)]` on by default

error[E0599]: no method named `unescape` found for struct `BytesText<'_>` in the current scope
    --> core/ooxml/src/drawing.rs:3295:34
     |
3295 |                 if let Ok(s) = t.unescape() {
     |                                  ^^^^^^^^ method not found in `BytesText<'_>`

error[E0277]: the size for values of type `str` cannot be known at compilation time
    --> core/ooxml/src/drawing.rs:3295:24
     |
3295 |                 if let Ok(s) = t.unescape() {
     |                        ^^^^^ doesn't have a size known at compile-time
     |
     = help: the trait `Sized` is not implemented for `str`
note: required by a bound in `std::prelude::v1::Ok`
    --> /rustc/88d9e12ae178fab0fb5cc050a94da85685d449ea/library/core/src/result.rs:561:4

warning: use of deprecated method `quick_xml::events::attributes::Attribute::<'a>::unescape_value`: use `Self::normalized_value()`
   --> core/ooxml/src/rels.rs:117:15
    |
117 |             a.unescape_value().ok().map(|v| v.to_string())
    |               ^^^^^^^^^^^^^^

warning: use of deprecated method `quick_xml::events::attributes::Attribute::<'a>::unescape_value`: use `Self::normalized_value()`
   --> core/ooxml/src/sectpr.rs:639:15
    |
639 |             a.unescape_value().ok().map(|v| v.to_string())
    |               ^^^^^^^^^^^^^^

warning: use of deprecated method `quick_xml::events::attributes::Attribute::<'a>::unescape_value`: use `Self::normalized_value()`
   --> core/ooxml/src/styles.rs:871:15
    |
871 |             a.unescape_value().ok().map(|v| v.to_string())
    |               ^^^^^^^^^^^^^^

warning: use of deprecated method `quick_xml::events::attributes::Attribute::<'a>::unescape_value`: use `Self::normalized_value()`
    --> core/ooxml/src/table.rs:1299:15
     |
1299 |             a.unescape_value().ok().map(|v| v.to_string())
     |               ^^^^^^^^^^^^^^

warning: use of deprecated method `quick_xml::events::attributes::Attribute::<'a>::unescape_value`: use `Self::normalized_value()`
   --> core/ooxml/src/theme.rs:513:15
    |
513 |             a.unescape_value().ok().map(|v| v.to_string())
    |               ^^^^^^^^^^^^^^

error[E0599]: no method named `unescape` found for struct `BytesText<'_>` in the current scope
    --> core/ooxml/src/lib.rs:1339:35
     |
1339 | ...   let s = t.unescape().map_err(|e| OoxmlError::Xml(e.to_string()))?;
     |                 ^^^^^^^^ method not found in `BytesText<'_>`

error[E0599]: no method named `unescape` found for struct `BytesText<'_>` in the current scope
    --> core/ooxml/src/lib.rs:1351:38
     |
1351 |                     if let Ok(s) = t.unescape() {
     |                                      ^^^^^^^^ method not found in `BytesText<'_>`

error[E0277]: the size for values of type `str` cannot be known at compilation time
    --> core/ooxml/src/lib.rs:1351:28
     |
1351 |                     if let Ok(s) = t.unescape() {
     |                            ^^^^^ doesn't have a size known at compile-time
     |
     = help: the trait `Sized` is not implemented for `str`
note: required by a bound in `std::prelude::v1::Ok`
    --> /rustc/88d9e12ae178fab0fb5cc050a94da85685d449ea/library/core/src/result.rs:561:4

error[E0599]: no method named `unescape` found for struct `BytesText<'_>` in the current scope
    --> core/ooxml/src/lib.rs:1359:31
     |
1359 | ...   let s = t.unescape().map_err(|e| OoxmlError::Xml(e.to_string()))?;
     |                 ^^^^^^^^ method not found in `BytesText<'_>`

warning: use of deprecated method `quick_xml::events::attributes::Attribute::<'a>::unescape_value`: use `Self::normalized_value()`
    --> core/ooxml/src/lib.rs:1409:15
     |
1409 |             a.unescape_value().ok().map(|v| v.to_string())
     |               ^^^^^^^^^^^^^^

error: could not compile `quire-ooxml` (lib) due to 6 previous errors; 7 warnings emitted
```

同樣地，`core/xlsx` 與 `core/odf` 亦存在對應之未遷移站點。

## 4. 遷移對應與修復計畫
按照 PM 查證之 API 對應進行等價遷移：
1. `BytesText::unescape()` → `quick_xml::escape::unescape(&t.decode()?)`：
   先由 `t.decode()?` 完成位元組至 UTF-8 解碼，再由 `quick_xml::escape::unescape(...)` 還原 XML 預設實體。
2. `Attribute::unescape_value()` → `Attribute::decode_and_unescape_value(quick_xml::reader::Decoder::utf8())`。
3. 建立各 crate 內部私有 `mod qx` 小工具函式，避免重複代碼，嚴禁新建 workspace crate。
4. 每修好一組站點 commit 一次，落實「零回退鐵條」驗收。
