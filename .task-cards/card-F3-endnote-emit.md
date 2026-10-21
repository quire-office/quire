# 任務卡 F3：尾註（endnote）發射——文件末頁尾註區（只消費 F1 資料層＋既有條目排布）

## 你的角色與紀律（違反＝任務失敗）
- 你在 git worktree `/home/jerry/projects/quire/.worktrees/oc-f3`（branch `oc/endnote-emit`）工作。先 `pwd` 確認。
- **嚴禁** `git commit` / `git push` / `git checkout` / `git reset` / `git stash`。改動留工作區，PM 逐行 review 後自行合併。
- **嚴禁碰主 checkout**（`/home/jerry/projects/quire`）：所有 cargo 指令與 render 基準都在**你自己的 worktree 內**跑（render/dump 輸出寫 `/tmp/f3-*`）。
- 所有註釋／測試文案**繁體中文**（禁簡體；「稽核/台/了/干/吃/于/才/回/藉/游/岩/晒/峰/群」為 opencc 白名單）。
- 先讀 `core/engine-api/src/footnotes.rs`（F2b-2 已落地：`NoteEntry`／`EntryStyle`／`layout_entry`／`page_foot`／`collect_anchors`／`find_anchor_line`／`PageInfo`／`MarkStyle`）與 `core/engine-api/src/lib.rs` 的 `build_note_entries`／`compute_footnotes`／`footnote_items`／`page_display_list`。本卡是在這套機制**旁邊**加一條尾註支路，**不是重寫**它。

## 專案背景
`quire` 是 Rust lossless DOCX 引擎。F2b-2 已實裝頁底註腳（逐頁保留帶＋收斂）。本卡做尾註：ECMA-376 §17.11.10——尾註**不掛頁底**，全部排在**正文之後的專屬頁**（Word 預設 `endnotePr/pos=documentEnd`）。

## 幾何契約（PM 定案，照做；測試與實作共用）
1. **正文頁零位移**：尾註只在正文分頁完成後**追加頁**；正文每一頁的內容/分頁斷點與無尾註時**逐位元組相同**（這是本卡的第一鐵律，PM 會做 A/B diff）。
2. `Engine::page_count()` ＝ 正文頁數 ＋ 尾註頁數；`page_display_list(page)` 對 `page >= 正文頁數` 發射尾註頁。超出回 None（不可越界 panic）。
3. 尾註頁幾何＝**末節**的頁面幾何（`sec_geo` 取正文末頁的 `section_id`）。
4. 尾註首页頂部：分隔線（`Rule`，版心左緣起、寬＝版心×1/3，y＝margin_top）；條目區自 `margin_top + 1 帶 + SEP_GAP 量化帶` 起。後續頁不畫分隔線（Word 的 continuationSeparator 屬 F3b，老實留白，註釋寫明）。
5. 條目逐頁裝箱：容量＝`section_capacity`（末節、無頁底預留）減掉首頁分隔線佔帶；條目高度＝`ceil(行高×行數 / 帶尺)`（與 F2b-2 `layout_entry` 同尺）；條目**整條不拆分跨頁**（一條裝不下就整條去下一页——註腳跨頁切分屬 F2c，尾註同款延後）。
6. 編號＝正文引用序（1 起，**十進位**；`endnotePr/numFmt`（lowerRoman 等）的消費是 F3b，本卡不做——註釋老實標明）。記號（上標數字）發在正文錨點 run 行內終點，規則與 F2b-2 註腳記號完全相同（可直接重用 `find_anchor_line` 與上標常量）。
7. 尾註與註腳共存（HC031 就是）：兩條支路各自獨立跑；同一 run 不會同時有兩者（`note_ref` 單欄位），無競態。

## 目標
1. `footnotes.rs`：
   - `collect_anchors` 加參數或成對函式，能取 `NoteKind2::Endnote` 錨點（**不破壞既有簽名**——加新函式 `collect_endnote_anchors` 最乾淨，既有呼叫點零改動）。
   - 新純函式 `flow_items_to_pages(items: &[NoteItem], capacity_first: usize, capacity_rest: usize) -> Vec<Vec<usize>>`（條目索引按序裝箱；單條超過整頁容量時獨佔一頁，不 panic）。
   - 新結構 `EndLayout { pages: Vec<Vec<NoteItem>>, marks: Vec<Vec<AnchorMark>>, separator_top: bool }`（serde derive 同 `NoteLayout`）＋ `is_empty()`。
2. `lib.rs`（engine 層）：
   - `Engine` 加欄位 `end: EndLayout`、尾註條目/錨點映射（對稱於既有 `note_entries`/`note_anchors`/`note_of_anchor`/`mark_styles`，可另立 `end_note_entries` 等欄位）。
   - `build_note_entries` 順手收尾註錨點與條目（級聯取樣路徑完全同款——尾註段也是 `Paragraph`，一次 `cascade_paragraphs`）。
   - `page_count()` 尾註頁計入；`page_display_list`/`_json` 支援尾註頁（正文發射碼**原樣不動**，尾註頁走新分支）。
   - `caret_rect`/`caret_at`/插入路徑：只認正文頁（尾註頁回 None／不動——編輯面不觸尾註，註釋寫明取捨）。
   - `footnotes_json` 加尾註段（或另開 `endnotes_json`，選一個並寫進 doc comment）。
   - 存檔路徑**零接觸**（純讀取投影，roundtrip 鐵律）。
3. 無尾註檔 ⇒ 全鏈零擾動：`end` 空、`page_count` 不變、顯示列表逐位元組不變（構造性：空分支直接 return，同 F2b-2 快路徑口徑）。

## 驗收標準（PM 逐項重跑）
1. `cargo fmt --all --check` 綠
2. `cargo clippy --workspace --all-targets -- -D warnings` 綠
3. `cargo test --workspace --no-fail-fast` 全綠（既有測試零改動）
4. `cargo run -q -p quire-cli -- roundtrip corpus --check` → `399 檔 byte-identical 399 失敗 0`
5. `cargo run -q -p quire-layout --bin goldens` 三檔零位移
6. `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine` 綠
7. **零變化實證（在你 worktree 內跑）**：改碼前後各跑
   `cargo run -q -p quire-cli -- dump corpus/ooxmlexport/fdo77887.docx`、`cargo run -q -p quire-cli -- dump corpus/ooxmlexport/style-inheritance.docx`
   前後輸出 diff 必空（存 /tmp/f3-dump-old-*.json / f3-dump-new-*.json，PM 重跑覆核）。
   另：HC031 **正文頁**的 `page_breaks_json` 改前後必須相同（`cargo run -q -p quire-cli -- dump` 或 CLI 等價途徑，比對時只取正文頁段）。
8. **新增測試（≥7，先紅後綠——先寫測試看紅再實作）**，實檔：
   - `corpus/xopt/TestFiles_HC031-Complicated-Document.docx`（endnote id=1，文字「 This is an endnote.」）
   - `corpus/xopt/OpenXmlPowerToolsExamples_TextReplacer02_Test01.docx`（PM 備註：實際路徑以 `find corpus -name "TextReplacer02_Test01*"` 為準；文字「 This is the endnote.」）
   - `corpus/selfgen/arabic_rtl_notes.docx`（PM 備註：同樣 find 確認路徑；中文尾註「尾註一：參考附件二之說明。」，RTL——至少不 panic 且文字上頁）
   - `corpus/poi/test-data_document_endnotes.docx`（PM 備註：find 確認路徑）
   測試點：
   (a) HC031：末頁為尾註頁、含分隔線 Rule 與文字「This is an endnote」（前導空白照級聯拍平結果斷言，勿硬扣）
   (b) HC031：正文頁數改前後相同（斷言 `page_count - end 頁數` 穩定）＋正文錨點記號「1」在正文頁發射
   (c) 無尾註檔（SAMPLE）：`page_count()` 與 main 相同、`end.is_empty()`
   (d) 多條尾註容量測試：自製 engine（用 `Engine::from_docx_bytes(SAMPLE)`＋set_part 塞尾註 XML——仿既有 `build_theme_doc` 手法）灌 ≥40 條尾註 ⇒ 尾註頁數 ≥2 且每頁條目數 ≤ 容量
   (e) 一條超長尾註獨佔頁不 panic
   (f) `flow_items_to_pages` 純函式邊界：空輸入回空；單條裝不下獨佔一頁
   (g) RTL／中文尾註實檔：發射文字包含關鍵片段且不 panic

## 禁区
- 不碰 `core/layout/`、`core/ooxml/`、`core/render-trait/`（本卡全在 engine-api 層內）。
- 不碰存檔路徑（save_docx／parts／zip）。
- 不碰 `scripts/`、CI、corpus、主 checkout。
- 不動 `compute_footnotes` 的收斂迴圈本體（尾註不進收斂——它不擠正文）。

## 回報格式
- 改動檔案清單＋每檔一句話
- 先紅後綠證據（紅燈原文）
- 驗收 1–7 逐項實跑輸出＋驗收 8 測試清單與斷言要點
- 一句話自評（哪裡最可能有坑）
