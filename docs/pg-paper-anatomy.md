# PG-PAPER-1 引擎紙張覆寫解剖報告

## 1. SectionGeometry 消費點全鏈審查

經由對程式碼庫全面 grep 檢索與靜態分析，確認 `SectionGeometry` 的消費點、建立點以及 OOXML 解析路徑如下：

### 1.1 唯一來源點
- `core/ooxml/src/sectpr.rs:447`：`scan_sections` 函式是整套引擎中唯一解析 `<w:pgSz>` 與 `<w:pgMar>` 的地方。
- `core/engine-api/src/lib.rs:331-360`：`from_docx_bytes` 在此處呼叫 `read_sections_per_paragraph`，將每個 section 的幾何轉換為 `SectionSpec { id, geo: SectionGeometry { ... } }`。
- **結論**：整套引擎中完全沒有第二處繞過 sections 直讀 `pgSz` 或 `sectPr` 的漏網點。

### 1.2 消費點全鏈審查（覆寫是否全鏈生效）
從 `sections: Vec<SectionSpec>` 建立後，其下游消費鏈如下：

1. **基準參數 LayoutParams 推導（`core/engine-api/src/lib.rs:373-376`）**：
   - `params = LayoutParams::from_section(&sections.first().geo, ...)`
   - `page_w_pt`、`page_h_pt`、四邊邊界 `margins_pt` 完全由首個 section 的幾何更新。若注入覆寫，此處即刻生效。
2. **段落斷行可用寬（`core/engine-api/src/lib.rs:380-382`）**：
   - 逐段計算 `quire_layout::section_break_w_pt(&params, &sections, i)`。
   - 根據段落所屬 section 之幾何（`page_w_pt - margin_left_pt - margin_right_pt`）或分欄模型首欄寬決定。覆寫幾何後，所有段落斷行寬全面連動更新。
3. **表格槽幾何（`core/engine-api/src/lib.rs:624-638`）**：
   - 呼叫 `table_map::build_slots(..., &params, &sections, ...)`。
   - 表格寬度與列高計算以傳入的 `params` 與 `sections` 為準，覆寫完全生效。
4. **圖片行帶與錨定（`core/engine-api/src/lib.rs:639-646`）**：
   - 呼叫 `table_map::build_images(..., &params, &sections, ...)`。
   - 圖片佔位與分頁帶數依賴 `params` 與 `sections`，覆寫完全生效。
5. **分頁容量與版面構建（`core/engine-api/src/lib.rs:742-750`）**：
   - 傳入 `LayoutEngine::with_breaks_grid(&texts, params.clone(), sections.clone(), ...)`。
   - `quire_layout` 在分頁時以 `section_capacity(p, geo, page_index)` 計算可用高度（`geo.page_h_pt - geo.margin_top_pt - geo.margin_bottom_pt`），並產生每頁之 `Page { width_pt: geo.page_w_pt, height_pt: geo.page_h_pt, ... }`。
6. **頁首頁尾層與節幾何表（`core/engine-api/src/lib.rs:753-761`）**：
   - `build_hf_layer` 與 `sec_geo_all` 均遍歷 `sections` 收集 `sec_geo`。
   - 頁首發射座標 `y = header_dist_pt`，頁尾發射座標 `y = geo.page_h_pt - footer_dist_pt`，均以覆寫後之 `geo` 為基準。
7. **浮動圖定位與對齊（`core/engine-api/src/lib.rs:2179`）**：
   - 正文流圖片對齊 x 座標由 `self.sec_geo_all` 查取對應節幾何計算版心寬。
8. **分欄排版起點（`core/engine-api/src/lib.rs:2384`）**：
   - 各欄 x 偏移由 `self.sec_geo_all` 查取分欄幾何計算。
9. **註腳預留與發射幾何（`core/engine-api/src/lib.rs:1585`）**：
   - `page_infos` 查取 `sec_geo` 計算逐頁容量與版心寬。

**結論**：覆寫只要在 `sections` 建立處注入，整條管線（斷行、分頁、表格、圖片、頁首頁尾、浮圖、分欄、註腳）即百分之百全鏈生效，無任何旁路洩漏。

---

## 2. 構造期注入與快取隔離證明

### 2.1 構造期注入的無快取性
- 引擎在 `from_docx_bytes_with_paper` 構造期接收 `Option<&PaperSpec>`。
- 在 XML 解析出 sections 的第一時間點，即對 sections 完成紙張與邊距覆寫。
- 後續全部派生資料結構（`params`、`widths`、`slots`、`bands`、`LayoutEngine`、`sec_geo`、`sec_geo_all`）直接基於覆寫後的幾何建立。
- 構造過程中未曾以舊紙張尺寸執行任何排版，因此完全不存在「舊紙張的殘留排頁快取」。

### 2.2 後續編輯操作（如 replace_paragraph_text）的持久性
- `Engine::replace_paragraph_text` 呼叫 `self.layout.replace_paragraph_text` 與 `self.recompute_notes`。
- `LayoutEngine` 內部持有的 `sections` 與 `params` 即為構造期注入的覆寫結構，不會重新讀取 XML 的原始 `pgSz`。
- `self.sec_geo` 與 `self.sec_geo_all` 在整個生命週期中均維持覆寫後幾何。
- 因此，接續編輯後的分頁與發射依然百分之百使用覆寫後的紙張大小與邊距，快取不會回收至檔案原始值。本性質將於考卷燈 4 中透過先紅後綠予以實測保證。

---

## 3. 全量重排耗時量測（UX 參考）

本卡使用 release 編譯之 `quire-cli info` 對 5 個不同規模與特徵的語料檔進行 5 次冷熱取樣量測，結果如下：

| 語料檔案 | 頁數 | 段落數 | 特徵 | 平均耗時 (ms) | 最短耗時 (ms) |
|---|---|---|---|---|---|
| `corpus/ooxmlexport/fdo76591.docx` | 1 | 1 | 單段極簡檔 | 14.64 | 13.89 |
| `corpus/ooxmlexport/cell-grid-span.docx` | 2 | 23 | 跨欄表格檔 | 3.42 | 3.30 |
| `corpus/xopt/TestFiles_HC031-Complicated-Document.docx` | 9 | 128 | 複雜長文、表格、註腳收斂 2 輪 | 32.96 | 31.44 |
| `corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_solar-system.docx` | 13 | 33 | 多節（12 sections）、大量頁面 | 4.99 | 4.87 |
| `corpus/xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-01.docx` | 1 | 51 | 典型格式化正文 | 5.02 | 4.95 |

### 3.1 網站卡（UI）UX 建議
1. **耗時評估**：在原生 release 環境下，一般 1-15 頁中小型文件開檔與全量排版耗時均小於 35ms；在 WebAssembly 環境下預估約為原生的 1.5 至 3 倍（約 5-100ms），反應速度極快。
2. **Loading 態建議**：
   - 對於一般文件（<50 頁），耗時遠低於 500ms 閾值，UI 可以近乎即時更新紙張。
   - 若遇到數百頁之超長文或上百表格之極端文件，Wasm 下可能突破 500ms。建議網站端（quire-www）在使用者點選切換紙張時，將按鈕置為 disabled 態並顯示微型加載指示（微 loading spinner），待新 `WasmEngine` 實例返回後即刻更新畫布。

---

## 4. 多 Section 檔案實況查驗

針對需求提及之候選檔案進行 XML 實體檢驗：
- `corpus/ooxmlexport/fdo76591.docx`：實際僅有 1 個 section（`<w:sectPr>`）。
- `corpus/ooxmlexport/cell-grid-span.docx`：實際僅有 1 個 section（`<w:sectPr>`）。
- 語料庫中真實的多 section 檔案代表：
  - `corpus/ooxmlexport/tdf124637_sectionMargin.docx`：7 個 sections。
  - `corpus/xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_solar-system.docx`：12 個 sections。
  - `corpus/xopt/TestFiles_HC031-Complicated-Document.docx`：4 個 sections。

在覆寫機制中，多 section 文件的全域語意為：所有節的 `page_w_pt` 與 `page_h_pt` 統一覆寫為新規格，指定的邊距統一覆寫，`None` 邊距各自保留該節原宣告。考卷燈 5 將針對真實多節文件驗證此行為。

---

## 5. PaperSpec 落點評估與決定

### 5.1 評選方案
- **方案 A（落於 `core/ooxml/src/sectpr.rs`）**：
  `PaperSpec` 與 `SectionGeometry`、`SectionSpec` 同族，置於同一模組，並於 `core/engine-api` 進行 `pub use quire_ooxml::PaperSpec;` 再匯出。
- **方案 B（落於 `core/engine-api/src/lib.rs`）**：
  直接在 `core/engine-api/src/lib.rs` 定義。

### 5.2 決定與理由
選定 **方案 B（落於 `core/engine-api/src/lib.rs`）**：
1. **嚴守檔案白名單邊界**：`core/ooxml/src/lib.rs` 未列於本卡之白名單中，若將 `PaperSpec` 定義於 `core/ooxml/src/sectpr.rs`，因 `sectpr` 為私有模組，需修改 `lib.rs` 進行 `pub use` 始能向外層透出，這將破壞白名單約束。
2. **型別直出與依賴簡潔**：`PaperSpec` 是構造期傳入 `Engine::from_docx_bytes_with_paper` 的公共輸入型別，直接定義於 `core/engine-api/src/lib.rs` 並頂層匯出，使 `shells/web`、`tests/pg_paper.rs` 與外部呼叫端皆能以標準的 `quire_engine::PaperSpec` 取用，介面直觀且無跨 crate 耦合。
3. **語意精確且無損**：`PaperSpec` 作為引擎層的構造期配置參數，在引擎內部於 sections 建立處映射至 `SectionGeometry`，職責分工清晰，符合高內聚低耦合原則。
