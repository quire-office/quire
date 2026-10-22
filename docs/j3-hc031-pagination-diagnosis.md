# J3：HC031 分頁斷點位移診斷報告（HC031-Complicated-Document）

## 0. 調查範圍與方法
- 對象檔：`corpus/xopt/TestFiles_HC031-Complicated-Document.docx`（9 段落群、含表格/圖片錨點/橫向節/清單）。
- 我方：`cargo run -q -p quire-cli -- render/dump` 產出 `pagebreaks.json`（8 個 body 頁斷點，另含尾註頁 SVG）。
- LO：LibreOffice headless 轉 PDF，pymupdf 抽每頁文字（9 頁）。
- 診斷腳本：`scripts/diag_pagination.py`（可重跑，輸出對齊表與首個分歧段落 XML）。

## 1. 逐頁錨點對齊表（段落 index → 頁 index）

| quire 頁 (0-based) | first_para | last_para | LO 對應頁 (0-based) | LO 該頁首行（節錄） |
|---|---|---|---|---|
| 0 | 0 | 9 | 0 | Video provides a powerful way… |
| 1 | 10 | 37 | 1 | HEADING1 |
| 2 | 37 | 73 | 2 | o / You can also type … / • |
| 3 | 73 | 88 | 3 | provides header, footer, cover page… |
| 4 (landscape 792×612) | 89 | 90 | 5 | This is a section that is in landscape mode. |
| 5 | 91 | 91 | 6 | This is in the following section. |
| 6 | 92 | 130 | 7 | Video provides a powerful way… |
| 7 | 130 | 130 | 8 | Video, you can paste in the… |

- LO 橫向頁落在第 6 頁（index5）、我方第 5 頁（index4），位移一頁。
- 我方頁數 9（含尾註頁）、LO 9 頁；body 頁斷點 8 個。

## 2. 首個分歧段落

- **索引 17**：quire 歸頁 1，LO 歸頁 2。
- 段落文字：`Video provides a powerful way to help you prove your point. When you click Online Video, you can paste in the embed code for the video you want to add. You can also type a keyword to search online for the video that best fits your document. Video provides a powerful way to help you prove your point…`（含 `w:endnoteReference w:id="1"`）。
- 前一段（索引 16）`Reading is easier, too, in the new Reading view…` 兩邊仍在頁 1；自索引 17 起 LO 改放到頁 2。

XML 片段：
```xml
<w:p w14:paraId="091E5A3F" w14:textId="77777777" w:rsidR="00C32A83" w:rsidRDefault="009B292A">
  <w:r><w:t xml:space="preserve">Video provides a powerful way to help you prove your point. When you click Online Video, you can paste in the embed code for the video you want to add. You can also type a keyword to search online for the video that best fits your document. </w:t></w:r>
  <w:r><w:rPr><w:rStyle w:val="EndnoteReference"/></w:rPr><w:endnoteReference w:id="1"/></w:r>
  <w:r><w:t>To make your document look professionally produced, Word provides header, footer, cover page, and text box designs that complement each other. For example, you can add a matching cover page, header, and sidebar.</w:t></w:r>
</w:p>
```
前後各 1 段摘要：
- 索引 16：`Reading is easier, too, in the new Reading view. You can collapse parts of the document and focus on the text you want…`（一般段落，無 pPr 特殊屬性）。
- 索引 18：`Video provides a powerful way to help you prove your point.`（`w:numPr` 編號清單項，ListParagraph）。

## 3. 結構歸因（逐項實據／排除）

(a) 表格列高／儲存格換行
- 實據：HC031 有 3 張表（3×3 含表頭＋空列＋400–600；3×3 含 100–600；1×3）。我方頁 2 把 400–600 與 100–300 等列塞在同一頁，LO 則在 300 後換頁、400–600 落到次頁（`scripts/diag_pagination.py` 對照 LO 文字可見）。現有 golden 僅 5 例（merged_cells / nested_table / cross_page / cell_image / cross_page_packer），未見 HC031 這種「含空列＋多表連排」的完全對應案例，屬未覆蓋口徑。
- 與首個分歧關係：首個分歧段落 17 為純文字段落，表格非其直接成因；但表格分頁差異是整體頁數差的疊加因素。

(b) 圖片錨點（inline/floating）
- 實據：document.xml 有 4 個 `wp:anchor` 浮動錨點與 6 個 `w:txbxContent` 文字框；`core/layout` 以 `attach_floating_images` 後附掛浮動圖。我方 SVG 可見文字框與矩形錨點。
- 排除/未證：浮動圖錨點段落索引偏後（標題區與文字框），非索引 17 之前；未見直接造成首個分歧的證據。屬可能疊加因素，需隔離測試。

(c) 清單符號寬度與懸掛縮排
- 實據：HC031 有 28 個含 `w:numPr` 的編號段落（numId=1）。我方 SVG 文字元素中清單標記數 = 0（`I.`/`a.`/`b.`/`c.`/`II.`/`•`/`` 皆無），LO PDF 頁 7 渲染 `I.`、`a.`、`b.`、`c.`、`II.`。⇒ **我方漏發清單符號，確認缺陷**（修復另開卡）。
- 與分頁關係：標記為水平佔位，不改變行數；非首個分歧的直接原因，但屬渲染缺陷。

(d) 多欄節（`w:cols`）
- 實據：document.xml 有 4 個 `w:cols`，皆僅 `w:space="720"`、無 `w:num`，依 ECMA-376 預設單欄。
- 結論：排除。HC031 無多欄節。

(e) `keepNext`／`keepLines`／`widowControl`／`pageBreakBefore`
- 實據：document.xml 中 `keepNext`/`keepLines`/`pageBreakBefore` 各 0 次；`widowControl` 0 次顯式宣告（Word 預設 widow/orphan 控制開啟）。程式碼：`core/ooxml` 未解析 `widowControl`，`core/layout` 未消費 `keepNext`/`keepLines`（搜尋無相關消費）。
- 結論：HC031 無顯式分頁屬性；預設 widowControl 可能造成 LO 的 2 行寡行/孤行控制，我方未實作，屬潛在疊加因素（需測試）。

(f) 橫向節邊界的分頁時機
- 實據：4 個 sectPr（para、para(landscape)、para、body）；頁尺寸兩邊解讀一致（LO 頁 6 landscape 792×612、我方頁 5 landscape 792×612）。我方在索引 89 切換 landscape 並 `starts_section=true`。
- 結論：頁尺寸無誤讀；橫向頁位移一頁係前段累積分頁差造成，非獨立的節邊界時機缺陷。但節邊界換頁的收節順序仍值得單獨驗證。

額外發現（候選清單外）：首個分歧段落 17 為純文字段落，無表格/圖片/清單/欄/keep 屬性，顯示分頁差更可能來自**行高/段距/可用頁高計算**（我方 page 1 容納 54 行 vs LO page 1 的 49 行，我方每頁多排）。`core/layout/src/lib.rs` 目前以固定 `line_spacing=1.4` 且未見段落間距（space before/after）消費，可能是主因，需後續卡驗證。

## 4. 後續卡清單

1. **行高/段距/可用頁高校準**（高風險）
   - 預期改動檔案：`core/layout/src/lib.rs`（行高、段距、可用高度）、`core/ooxml/src/styles.rs`（space before/after 解析）、`core/ooxml/src/lib.rs`（段落屬性）。
   - 紅燈測試構想：以 HC031 建立 golden，斷言 pagebreaks 首個分歧段落 = 17 歸頁 1（修復後應與 LO 一致，歸頁 2）；`cargo test --workspace` 全綠。
   - 風險：高（影響全 corpus 分頁）。

2. **表格跨頁拆分（a）**（中風險）
   - 預期改動檔案：`core/layout/src/lib.rs`（表格佔帶與跨頁拆分）、`core/ooxml/src/table.rs`（row 高度）。
   - 紅燈測試構想：新增 golden `hc031_table_split`，斷言 100–300 與 400–600 分別落在不同頁；與 LO 對照。
   - 風險：中。

3. **清單符號漏發（c）**（中風險）
   - 預期改動檔案：`core/layout/src/lib.rs`（numPr 渲染）、`shells/native`/`shells/web`（SVG 發射）、`core/ooxml/src/styles.rs`（numPr 解析）。
   - 紅燈測試構想：SVG 文字元素包含 `I.`、`a.` 等標記；`scripts/diag_pagination.py` 的 SVG 符號數 > 0。
   - 風險：中（版面寬度變化可能影響換行）。

4. **浮動圖片錨點佔位（b）**（中風險）
   - 預期改動檔案：`core/layout/src/lib.rs`（`attach_floating_images`）、`core/ooxml/src/drawing.rs`。
   - 紅燈測試構想：HC031 golden 比對浮動圖頁面歸屬與 LO 一致。
   - 風險：中。

5. **widowControl 預設行為（e）**（低～中風險）
   - 預期改動檔案：`core/ooxml/src/styles.rs`、`core/layout/src/lib.rs`。
   - 紅燈測試構想：段落末 2 行不單獨留在次頁（與 LO 一致）。
   - 風險：低～中。

6. **橫向節邊界收節順序（f）**（低風險）
   - 預期改動檔案：`core/layout/src/lib.rs`（section 切換分頁）。
   - 紅燈測試構想：landscape 頁索引與 LO 一致（index5）。
   - 風險：低。

## 5. PM 覆核（合併前補記）

- **「首個分歧段落 17」的性質修正**：PM 用 J2 輪的 LO 原始擷取核對——LO p2 首行是 `o` / `You can also type a keyword to search online for the video that best fits your document.`，即段落 17 的**中段尾巴**。故 LO 實際是把段落 17 **跨頁切條**（前半留 p1、後半進 p2），我方則整段留 p1——不是「整段挪頁」。對齊表與 XML 片段仍有效，但後續卡（1/2/5）應把修復目標定義為「**跨頁切條策略＋每頁容量**（我方 p1 排 54 行 vs LO 49 行的行高/段距差）」，不是整段歸屬。
- **方法學弱點備註**：本腳本的 LO 段落→頁映射用「全文 find＋游標」，對 LO 注入的 endnote 記號（`i`）與清單符號（`o`/`•`）無彈性；後續若沿用請先對段落文字做 marker 正規化，或改用 J2 已上線的 content_key 口徑。
- 候選 (c)「我方漏發清單符號」由 PM 先前獨立實測交叉證實（我方 p0/p1 SVG 符號數＝0、LO 有 `o`/`•`/`I.`/`a.`），**確認為真缺陷**，排後續卡。

## 6. 驗收實跑（子 agent 自跑＋PM 覆核）

1. `python3 scripts/diag_pagination.py corpus/xopt/TestFiles_HC031-Complicated-Document.docx` → 成功，輸出對齊表與首個分歧段落 XML（索引 17），SVG 清單符號數 = 0。
2. 報告存在，(a)–(f) 逐項附實據/排除理由。
3. 候選 (c) 明確結論：我方漏發清單符號（SVG 符號數 0 vs LO 有 `I.`/`a.` 等）。
4. 後續卡清單含預期改動檔案、紅燈測試構想、風險等級。
5. `cargo test --workspace`：本卡未動碼，應仍全綠（請 PM 重跑確認）。
