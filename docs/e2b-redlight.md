# E2b 直排發射層未消費與度量閘門補強紅燈快照

署名：ADT109119

## 1. 任務背景與問題定位
前一張卡 E2（合併 `833bdc9`）將直排文字修到顯示清單層（`core/layout` 的 `LineFlow` 與 dump text item 方向旗標 bit16/32/64）。既有五盞考卷雖然全綠，但全部打在內部顯示清單 `page_display_list`。
實際出貨產物（如 `/tmp/test-btlr/btlr-textbox-p0.svg`）卻仍是純橫排：
`<text x="225.8" y="235.9" font-size="11.0" fill="#000000" font-family="Calibri" xml:space="preserve">First paragraph.</text>`
`<text x="239.2" y="235.9" font-size="11.0" fill="#000000" font-family="Calibri" xml:space="preserve">Second paragraph.</text>`
元素完全零 `rotate` transform，且兩段文字基線都在相同的 `y=235.9`，導致文字在方框內重疊疊字、橫向溢出方框邊界（LibreOffice 真值為細長方框內由下而上、兩行左右並排）。發射層根本未消費直排旗標。

同時，舊版 `scripts/render_lab.py` 的度量閘門存在重大漏洞：其 `run_quire` 是從 `quire-cli dump` 讀取 text item 旗標來推算 `line_dirs`（`flags & 16 -> (0, -1)`）。這屬於引擎自報機制，即使 SVG 繪製純橫排，舊版度量閘門依舊給出內容 1.0／版面 1.0 的滿分偽影，無法防範旗標與像素分家。

## 2. 驗收條件 1：新版閘門在 E2 現態下判失敗（價值證明快照）
新版 `scripts/render_lab.py` 改造：`run_quire` 的 `lines` 與 `line_dirs` 改為直接從出貨產物 SVG 解析。
若 SVG 為純 `<text>` 無旋轉 transform，解析出的行方向向量為 `dir=[1.0, 0.0]`（橫排）；若帶 `rotate(-90 ...)` 則為 `dir=[0.0, -1.0]`。
在尚未修改發射層（即「旗標直排＋SVG 橫排」的 E2 現態）下執行度量比對：
執行指令：`python3 scripts/render_lab.py corpus/ooxmlexport/btlr-textbox.docx --out /tmp/lab-btlr-e2-redlight`
比對輸出：
```
→ corpus/ooxmlexport/btlr-textbox.docx
   LO 1 頁 / OW 1 頁，內容相似度 [1.0]（版面 [0.0]）
   色 [1.0] 圖 [None] 真缺字 0（斷行切片已排除 0）：[]

報告 → /tmp/lab-btlr-e2-redlight/report.md
```
報告內容明細（`/tmp/lab-btlr-e2-redlight/report.md`）：
```markdown
| 檔案 | LO 頁 | OW 頁 | 頁數 | 內容相似度（首 6 頁） | 版面相似度（首 6 頁） | 首個內容分歧頁 | 備註 | 色相似度（首 6 頁） | 圖相似度（首 6 頁） | 真缺字 |
|---|---|---|---|---|---|---|---|---|---|---|
| corpus/ooxmlexport/btlr-textbox.docx | 1 | 1 | ✓ | 1.0 | 0.0 | 第 1 頁 | line_direction_mismatch:LO直排我方橫排; 第 1 頁內容近同、僅換行位置不同 | 1.0 | — | 0 |
```
證明：舊版閘門在此狀態下判 1.0，新版閘門精確抓出 `line_direction_mismatch:LO直排我方橫排` 並判定版面 0.0 失敗。

## 3. 紅燈考卷 `core/engine-api/tests/e2b_vertical_svg_emission.rs` 執行快照
考卷直球打在渲染端（`render_svg::page_to_svg` 輸出的每頁 SVG 字串）：
1. 斷言 `First paragraph.` 與 `Second paragraph.` 發射元素帶旋轉 `transform="rotate(-90 ...)"`；
2. 斷言兩段文字繪製基線不共線（防疊字，x 軸距離差距 ≥ 10pt）；
3. 斷言文字邊界框落在方框 bbox 內（LO bbox `[225.8, 154.2, 239.2, 224.9]` ±4pt）。

執行指令：`cargo test -p quire-engine --test e2b_vertical_svg_emission`
執行結果（修前紅燈）：
```
running 9 tests
test render_svg::tests::svg_escapes_xml_special_characters ... ok
test render_svg::tests::svg_skips_fully_transparent_rects ... ok
test render_svg::tests::svg_emits_font_family_from_declaration ... ok
test render_svg::tests::svg_emits_embedded_image_pixels ... ok
test e2b_vertical_svg_emission_btlr_textbox ... FAILED
test render_svg::tests::svg_emits_image_placeholder_rect_and_rules ... ok
test render_svg::tests::svg_escapes_font_family_attribute ... ok
test render_svg::tests::svg_outputs_red_fill_bold_and_text_decorations_from_fixture ... ok
test render_svg::tests::svg_omits_font_family_without_declaration ... ok

failures:

---- e2b_vertical_svg_emission_btlr_textbox stdout ----

thread 'e2b_vertical_svg_emission_btlr_textbox' (64558) panicked at core/engine-api/tests/e2b_vertical_svg_emission.rs:47:5:
First paragraph. 必須帶 rotate(-90 ...) 旋轉變換（實得：<text x="225.8" y="235.9" font-size="11.0" fill="#000000" font-family="Calibri" xml:space="preserve">First paragraph.</text>）
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

failures:
    e2b_vertical_svg_emission_btlr_textbox

test result: FAILED. 8 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
```

## 4. 預期修復路徑
1. 修改 `shells/native/src/render_svg.rs`：消費 `DrawItem::Text.flags` 中的直排方向旗標（`FLAG_DIR_VERT270` 與 `FLAG_DIR_VERT`）。
2. 在 `FLAG_DIR_VERT270` 分支中，發射帶有 `transform="rotate(-90 {base_x} {base_y})"` 的 `<text>` 元素，使文字沿 y 負方向由下而上延伸，字頂落在文字框列寬內，命中 LO 真值 bbox ±4pt。
3. 確保橫排檔案輸出零擾動，既有五盞考卷及基準檔測試零回退。
