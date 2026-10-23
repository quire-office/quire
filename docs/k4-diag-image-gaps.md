# K4-diag：含圖／文字框／VML 文件的 LibreOffice 發射缺口調查

- 調查卡：K4-diag（report-only，本卡禁修任何程式碼）
- 分支：`oc/k4-diag`
- 調查日期：2026-10-23
- LibreOffice：25.8.7.3（官方 `LibreOffice_25.8.7.3_Linux_x86-64_deb`，解壓至 repo 外 scratch，以 PATH 餵 `scripts/render_lab.py`；未安裝系統套件、未改 repo）
- 量測器：`scripts/render_lab.py`，產物在 gitignored 的 `out/k4-diag/`（6 份 LO PDF、LO/OW PNG、OW SVG、`report.md`）

## 交付狀態

`git status --short` 乾淨（本文件為本卡唯一允許的新檔），無任何 `.rs` / `.py` 變更。本次 6 檔分數與 `docs/lo-baseline-2026-10.md` 逐位一致：

```
0.000 / 0.000 / 0.000 / 0.179 / 0.849 / 0.875
```

## 調查對象

| 檔案 | manifest 層標籤 |
|---|---|
| `corpus/ooxmlexport/n779834.docx` | image / header 浮水印 |
| `corpus/ooxmlexport/n780563.docx` | drawing / 填色表格 |
| `corpus/ooxmlimport/tdf152200-bad_fldChar_end.docx` | legacy_vml / 空矩形黑框 |
| `corpus/poi/test-data_document_Bug51170.docx` | 頁首圖 + 段距分頁 |
| `corpus/poi/test-data_document_shapes-with-text.docx` | drawing / 文字框 |
| `corpus/selfgen/sdt_vml_textbox.docx` | legacy_vml / 文字框 |

## 逐檔缺口表

| 檔案 | 分數 | LO 頁／OW 頁 | 定性 |
|---|---|---|---|
| n779834 | 0.000 | 1/1 | **度量偽影**：LO 該頁可抽取文字為 0，我方同頁也是 0。LO 有 header 浮水印 EMF 圖，我方缺。真缺口＝頁首圖片 |
| n780563 | 0.000 | 1/1 | **度量偽影**：兩邊 0 文字。LO 有三色列填色＋含文字表格框，我方無填充／框線 |
| tdf152200 | 0.000 | 1/1 | **度量偽影**：兩邊 0 文字。LO 只有黑框空矩形，我方該矩形缺失 |
| Bug51170 | 0.179 | 7/5 | **純分頁**：全文 token ratio 0.9756，LO-only token 僅頁尾文字／頁碼；差距全來自頁數（LO 7 頁 vs 我方 5 頁） |
| shapes-with-text | 0.849 | 1/1 | **無缺字**（22 vs 22 token）：唯一差異是 `a group of shapes` 的順序——我方文字排進 x=72 正文流，LO 在形狀內。0.849＝位置／順序／換行 |
| sdt_vml_textbox | 0.875 | 1/1 | **反向**：我方多發「VML 文字框內容」，LO 完全不畫該 VML 文字框（0 drawings） |

## 缺口分類彙總

1. **度量偽影（A）**：`scripts/render_lab.py:254` 的 `if (ca or cb) else 0.0`——兩邊皆空即回 0.0，把「雙方都沒文字、只剩圖」誤判為全錯。三檔 0.000 皆此類，K4 卡不可據此判定文字發射失敗。
2. **頁首／頁尾 part 圖片不發射（B）**：`core/engine-api/src/lib.rs:333-335` `read_images` 只讀 `word/document.xml`；`:122-127` `HfPart` 無 images 欄；`:2480` 頁首 part 傳空 images ⇒ n779834 浮水印、Bug51170 header `wp:inline` 圖都丟。
3. **docDefaults-only 的 K1 gate（C，Bug51170 根因）**：Normal 樣式 `w:spacing w:after="200"`（10pt 段後距）未被消費。`core/ooxml/src/styles.rs:835` `doc_spacing()` 只回 docDefaults；`core/engine-api/src/lib.rs:290-291` docDefaults 無宣告即整關 K1。layout 端 `after_bands` 本有能力（`core/layout/src/lib.rs:552-565`）。
4. **圖形／填色／框線不發射（D）**：`DrawItem` 無 Image 變體（`core/render-trait/src/lib.rs:14-48`），native SVG 只畫 Rect/Rule/Text（`shells/native/src/render_svg.rs:96-110`）；`emit_image_placeholder` 只出灰底＋Rule 無像素（`core/engine-api/src/lib.rs:1931-1957`）。相關：`core/layout/src/lib.rs:639-654`（`PageImage`／`PageFloatingImage`）、`:1345` `attach_floating_images`。
5. **VML textbox 語意（E）**：`core/ooxml/src/lib.rs:571-581` 每個 `w:p` 一律平化，txbxContent 段落變正文段；`core/ooxml/src/drawing.rs:356-365` `finalize_vml` 只認 imagedata/gfxdata，文字框被丟；而 shapes-with-text 的 `push_pending`（`core/ooxml/src/drawing.rs:333-354`）有 extent 就當 InlineImage，導致文字被抽出到正文流。body/floating 圖發射見 `core/engine-api/src/lib.rs:1400-1415`。

## Bug51170 專節

**結論：無文字遺失，0.179 全數來自分頁。**

- 全文 token ratio 0.9756；LO-only token 只有頁尾頁碼／頁尾文字。
- body 無任何 `w:drawing` / `w:pict` / `wp:anchor`（圖只在 `word/header1.xml` 的 `wp:inline`）。
- 根因：Normal 樣式 `w:spacing w:after="200"`（= 10pt 段後距）未被 K1 gate 消費。`core/ooxml/src/styles.rs:835` `doc_spacing()` 只回 docDefaults；`core/engine-api/src/lib.rs:290-291` docDefaults 無宣告即全關 K1；layout 端 `after_bands` 本有能力（`core/layout/src/lib.rs:552-565`）。
- 算術核對：217 bands + 149 × 10pt（≈117.8 bands）÷ 54.2 bands/頁 ≈ 6.18 → 7 頁。
- 附帶缺口：頁首 `wp:inline` 圖因分類 B（頁首 part 圖片不發射）遺失。

## 修復排序（建議拆卡）

- **K4m**（量測，先做）：修 `scripts/render_lab.py:254` 的 0/0 偽影，改以「LO 有非文字內容」標記 `measurement_artifact`，否則後續所有卡片都會被 0.000 誤導。
- **K4a**（頁首／頁尾圖）：`read_images` 擴及 headers/footers，`HfPart` 帶 images，頁首 part 傳入；解 n779834、Bug51170 圖層。
- **K4b**（K1 gate 放寬）：`doc_spacing` 併入預設段落樣式（含鏈）之 spacing，或 gate 改為「文件內任何有效段距宣告」；解 Bug51170 7 頁。
- **K4c**（DrawItem Image）：`render-trait` 加 Image 變體＋native SVG／其餘 shell 實作；解填色／框線／圖片像素。
- **K4e**（VML textbox）：txbxContent 不再平化成正文段，改為 shape 內文字；同時修 shapes-with-text 的順序。
- **K4d**（sdt_vml_textbox 反向）：確認 LO 不畫該 VML 是否因畸形；若 LO 正確，我方需抑制。

## 不確定項（未驗證，勿當事實）

- Bug51170 = 7 頁僅算術推估，未做精確重排模擬。
- shapes-with-text 的 y 漂移未逐 anchor 會計。
- n780563 的 txbxContent 表格究竟丟在 OOXML 層或 layout 層，未定位。
