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

## 5. 本卡返修：發現新版閘門仍有自報破口（`data-line-dir`）並予以關閉
第一版發射層對 `FLAG_DIR_EAVERT`（東亞逐字直排）發的是 `data-line-dir="ea-vert"` 這個**不參與繪製的自報屬性**，
而 `parse_svg_lines_and_dirs` 又把該屬性當成直排證據。這等於把本卡要消滅的形態（我方自報、非像素）
從 `dump` 旗標搬到 SVG 屬性通道：純橫排的 `<text>` 只要掛上該屬性，就被新版閘門判成直排。
實測破口（橫排像素＋自報屬性 ⇒ 被判定直排）：
```
lines  = ['Purely horizontal text']
dirs   = [(0.0, 1.0)]
被判定為直排？ True
```
處置（兩步，缺一不可）：
1. `shells/native/src/render_svg.rs` 的 `FLAG_DIR_EAVERT` 分支**不再發任何自報屬性**，該旗標不影響 SVG 輸出；
   自報屬性也不該出現在出貨產物裡（污染出貨產物、且不參與繪製）。
2. `parse_svg_lines_and_dirs` 改為**只准讀真實幾何**：旋轉 transform，或卡面所指的**逐字直排定位**
   （連續單字元 `<text>` 同 x、y 單向推進 ⇒ 該欄為逐字堆疊的直排欄，方向依 y 指向取 `(0,±1)`）。
   `writing-mode`／`data-line-dir` 一律不讀。
效果：`btlr-frame-vml`／`tbrl-frame-vml`／`groupshape-sdt` 三檔的 SVG 回到與舊引擎**逐位元組相同**，
但它們的直排判定改由真實像素定位支撐（我方 `any_vert` 仍為真），不再靠宣稱。
`--selftest` 新增兩盞鎖住此性質：
```
PASS 8c 防自報：純橫排像素掛直排自報屬性仍判 (1,0)（不得讀宣稱屬性）（dirs=[(1.0, 0.0), (1.0, 0.0)]）
PASS 8d 逐字直排定位：同 x 單向 y 推進判 (0,1)、同 y 橫向推進判 (1,0)
```

## 6. 獨立覆核記錄（非沿用上一輪自報）
本卡接手時工作區已有上一輪 crash 遺留的 commit，以下全部為本次重跑實測。
### 6.1 驗收條件 1 反證矩陣（同一份 LO PDF，只換閘門與引擎）
| 引擎態 | 閘門 | btlr-textbox 版面分 | 判定 |
|---|---|---|---|
| 舊（E2 現態：旗標直排、SVG 橫排） | 舊（讀 dump 旗標） | **1.0** | 假綠（本次事故形態） |
| 舊（E2 現態） | 新（讀 SVG 幾何） | **0.0** | 抓出 `line_direction_mismatch:LO直排我方橫排` |
| 新（發射層消費旗標） | 新（讀 SVG 幾何） | **1.0** | 通過 |
### 6.2 零回退（`/usr/bin/python3` 實跑，LO 真值側同一份 PDF）
btlr-textbox 1.0/1.0、fdo76591 0.98/1.0、WC048 1.0/1.0、99_Fields 1.0/1.0、
n751077 p1 0.88/0.867（p2 None）、picture-with-schemecolor None/None、dkvert 0.538/0.433
——新舊兩側逐檔逐軸**完全相同**，無任何 >0.02 抖動。
另對全部 8 個帶直排旗標的檔案（btlr/tbrl-textbox、fdo69636、tdf103573、tdf124754、
groupshape-sdt、btlr/tbrl-frame-vml）跑新舊對照，分數逐檔逐軸亦完全相同（無回退）。
### 6.3 SVG byte 級零擾動（全庫渲染，舊引擎 vs 新引擎）
只有 **5 個**檔案的 SVG 變化，且全部是消費旋轉旗標的直排分支：
`btlr-textbox`、`tbrl-textbox`、`fdo69636`、`tdf103573`、`tdf124754`；其餘檔案逐位元組相同。
### 6.4 肉眼對照（驗收條件 2）
LO 真值 `lo-png/btlr-textbox-p0.png`：細長框內由下而上、兩行左右並排。
我方 `ow-png/btlr-textbox-p0.png`：同一版面（文字置於框內左下角、由下而上、兩欄並排、無疊字無溢出）。
發射產物實抓（兩段基線 x 差 13.4pt 不共線 ⇒ 不疊字，bbox 命中 LO 真值 ±4pt）：
```
<text x="236.8" y="224.9" font-size="11.0" fill="#000000" transform="rotate(-90 236.8 224.9)" ...>First paragraph.</text>
<text x="250.2" y="224.9" font-size="11.0" fill="#000000" transform="rotate(-90 250.2 224.9)" ...>Second paragraph.</text>
```
### 6.5 八閘
`bash ~/.hermes/scripts/ow-gate.sh <worktree>` ⇒ `GATE_SUMMARY fail=0`（fmt／clippy -D／test／layering／roundtrip fixtures／roundtrip corpus／wasm32／goldens）。

