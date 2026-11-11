# bug65649 橫向頁段歸因診斷報告

## 執行摘要

本診斷針對 `corpus/poi/test-data_document_bug65649.docx` 在 LibreOffice（LO）收斂儀表板中回報之「橫向頁段誤判（我方 144 橫向頁 vs LO 同位直向）」與「雙向錯位」現象進行全面代碼審查、XML 結構分析與跨引擎實測。

### 核心結論
1. **偽因確認，排除三項起手假說**：
   PM 觀察到的「雙向錯位」（LO 橫向我方直向：第 21、95–99、137、138、140–143、146 頁；LO 直向我方橫向：第 90–92、123–127、178 頁），**並非**排版引擎對 section 走向判定錯誤、**亦非** `w:sectPr` 溢出或讀錯宣告，而是**分頁容量差異導致的頁數漂移（LO 總頁數 178 頁 vs 我方 190 頁）在「同位索引比較」（第 i 頁 vs 第 i 頁）下產生的偽差**。
2. **走向拓撲 100% 全等**：
   LO 真值與我方排版之幾何走向連續區段（Runs）皆為嚴格相同的 9 個交替段落：
   `P -> L -> P -> L -> P -> L -> P -> L -> P`（Portrait 直向與 Landscape 橫向交替）。
3. **我方引擎內部幾何判定 0 誤差**：
   我方渲染之 190 頁中，每一頁之寬度、高度與走向，完全 100% 忠實對齊該頁內容所屬 section 之 `w:sectPr` 宣告（不一致頁數為 0）。

---

## 一、源檔 XML sectPr 直掃結果

經解包直掃 `corpus/poi/test-data_document_bug65649.docx` 之 `word/document.xml`，全檔共計包含 **15 個 `w:sectPr`**。

### 1. 宣告清單與所屬 Section
依文件順序掃描，15 個 section 之段落範圍、尺寸宣告、結束標記與走向判定如下表：

| SecID | 涵蓋段落範圍 (0-based) | 段數 | 頁面尺寸 twip (寬×高) | 頁面尺寸 pt (寬×高) | `w:orient` 屬性 | 結束 `w:type` | 走向判定 |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| Sec 0 | 0..23 | 24 | 11906×16838 | 595.3×841.9 | 無 (None) | nextPage (預設) | 直向 (Portrait) |
| Sec 1 | 24..389 | 366 | 11906×16838 | 595.3×841.9 | 無 (None) | nextPage (預設) | 直向 (Portrait) |
| Sec 2 | 390..427 | 38 | 16839×11907 | 841.95×595.35 | landscape | nextPage (預設) | 橫向 (Landscape) |
| Sec 3 | 428..2806 | 2379 | 16839×11907 | 841.95×595.35 | landscape | nextPage (預設) | 橫向 (Landscape) |
| Sec 4 | 2807..5769 | 2963 | 16839×11907 | 841.95×595.35 | landscape | nextPage (預設) | 橫向 (Landscape) |
| Sec 5 | 5770..11560 | 5791 | 16839×11907 | 841.95×595.35 | landscape | nextPage (預設) | 橫向 (Landscape) |
| Sec 6 | 11561..12242 | 682 | 11906×16838 | 595.3×841.9 | 無 (None) | nextPage (預設) | 直向 (Portrait) |
| Sec 7 | 12243..13646 | 1404 | 16838×11906 | 841.9×595.3 | landscape | nextPage (預設) | 橫向 (Landscape) |
| Sec 8 | 13647..13953 | 307 | 11906×16838 | 595.3×841.9 | 無 (None) | nextPage (預設) | 直向 (Portrait) |
| Sec 9 | 13954..13954 | 1 | 11906×16838 | 595.3×841.9 | 無 (None) | continuous | 直向 (Portrait) |
| Sec 10 | 13955..14159 | 205 | 11906×16838 | 595.3×841.9 | 無 (None) | nextPage (預設) | 直向 (Portrait) |
| Sec 11 | 14160..14323 | 164 | 16838×11906 | 841.9×595.3 | landscape | nextPage (預設) | 橫向 (Landscape) |
| Sec 12 | 14324..14369 | 46 | 11906×16838 | 595.3×841.9 | 無 (None) | nextPage (預設) | 直向 (Portrait) |
| Sec 13 | 14370..15932 | 1563 | 16838×11906 | 841.9×595.3 | landscape | nextPage (預設) | 橫向 (Landscape) |
| Sec 14 | 15933..15933 | 1 | 11906×16838 | 595.3×841.9 | 無 (None) | nextPage (預設) | 直向 (Portrait) |

### 2. 橫向 Section 結束與後續直向 Section 宣告細節
1. **橫向 Section 總數**：共 **7 個**（Sec 2, 3, 4, 5, 7, 11, 13）。
2. **結束類型（`w:type`）**：
   所有 7 個橫向 section 之 XML 節點均**未宣告 `w:type`**，依 ECMA-376 規範預設值為 `nextPage`。無任何橫向 section 採用 `continuous`、`evenPage` 或 `oddPage` 結束。
3. **其後第一個直向 Section 宣告**：
   - Sec 2–5（橫向連段）後第一個直向 section 為 **Sec 6**（Child 361）：
     宣告 `w:pgSz w="11906" h="16838"`，無 orient（預設 portrait），未宣告 `w:type`（預設 `nextPage`）。
   - Sec 7（橫向）後第一個直向 section 為 **Sec 8**（Child 669）：
     宣告 `w:pgSz w="11906" h="16838"`，無 orient（預設 portrait），未宣告 `w:type`（預設 `nextPage`）。
   - Sec 11（橫向）後第一個直向 section 為 **Sec 12**（Child 740）：
     宣告 `w:pgSz w="11906" h="16838"`，無 orient（預設 portrait），未宣告 `w:type`（預設 `nextPage`）。
   - Sec 13（橫向）後第一個直向 section 為 **Sec 14**（Child 752，body 級收尾 sectPr）：
     宣告 `w:pgSz w="11906" h="16838"`，無 orient（預設 portrait），未宣告 `w:type`（預設 `nextPage`）。

---

## 二、引擎代碼審查與三假說證偽

### 1. 我方頁尺寸決定鏈實裝點
代碼鏈路由以下三層組成：
- **Section 採集層**：`core/ooxml/src/sectpr.rs`
  - `scan_sections`（行 382–601）：掃描 XML 事件流，以 `p_count` 累積 body 級段落槽位，遇 `sectPr` 記錄邊界 `boundaries.push((p_count, next_id, accum))`。
  - `read_sections_per_paragraph`（行 228–261）：依 `boundaries` 將段落槽位投影為逐段落的 `SectionSpec { id, geo }` 向量。
- **排版分頁層**：`core/layout/src/lib.rs`
  - `effective_section`（行 727–745）：由流項錨定段落 `anchor` 查得該段之 `SectionSpec`。
  - `pack_run`（行 1231–1580）：
    - 行 1443–1445：跨 section 換頁判定：
      `Some(prev) if prev.id != spec.id => { (!spec.geo.continuous || cap_hard || page_break_before) && !col_break_now }`
      當進入新 section 且非 continuous 時，強制 `start_new = true`。
    - 行 1478–1480：頁首項蓋章：`if st.page_spec.is_none() { st.page_spec = Some(spec); }`。
    - 行 1256、1264–1265：頁面收束蓋章：
      `let s = st.page_spec.unwrap_or_else(|| st.cur_spec.unwrap());`
      `width_pt: s.geo.page_w_pt, height_pt: s.geo.page_h_pt, section_id: s.id`。
  - `breaks_from_pages`（行 2148–2197）：
    - 行 2172：`let starts_section = i > 0 && pages[i - 1].section_id != pg.section_id;`。

### 2. 三個起手假說驗證與證偽
- **假說 (a) section 邊界判定錯（新 section 沒切換幾何來源，沿用上一個橫向 section）**：**證偽**。
  - 實測證據：在 bug65649 全檔 190 頁中，每一個 section 邊界（例如 Page 22、Page 93、Page 100、Page 128、Page 144、Page 146、Page 147、Page 190）皆準確觸發 `starts_section = true`。
  - 新頁之 `page_spec` 嚴格取自新 section 之幾何（直轉橫為 842.0×595.4，橫轉直為 595.3×841.9），無任何沿用上一 section 幾何之情事。
- **假說 (b) `w:type` 語義處理錯（continuous/evenPage 不強制開新頁導致宣告錨錯頁）**：**證偽**。
  - 實測證據：源檔中所有 7 個橫向 section 均為預設之 `nextPage`，全數強制開新頁。
  - 全檔唯一出現之 `continuous` 為 Sec 9（段落 13954），其前（Sec 8）與其後（Sec 10）均為直向 section，屬同走向之欄位設定，未干擾任何橫向段落邊界。
- **假說 (c) landscape 判定讀了錯的宣告（讀 theme/default 而非該 section 的 pgSz，或 orient 屬性與 w/h 哪個贏的口徑錯）**：**證偽**。
  - 實測證據：`sectpr.rs` 第 451 行 `a.landscape = attr_str(&e, b"orient").as_deref() == Some("landscape")` 且在 `into_geom()` 中以 `w > h` 雙重保險。
  - 我方引擎對 15 個 section 之走向判定與 XML 宣告 100% 一致。

---

## 三、真因定位：分頁容量差異與同位索引錯位指紋

### 1. 偽因排除與雙向交錯機制
儀表板回報的錯位頁面（共 22 頁）如下：
- **LO 橫向我方直向**（13 頁）：第 21、95、96、97、98、99、137、138、140、141、142、143、146 頁
- **LO 直向我方橫向**（9 頁）：第 90、91、92、123、124、125、126、127、178 頁

此種「雙向交錯」並非引擎判定混亂，而是**分頁容量差異所產生的典型相位差（Phase Shift）指紋**。

### 2. 連續走向區段（Runs）拓撲對照表
將 LO 真值 PDF 與我方 `pagebreaks.json` 按幾何走向連續段（Runs）對齊，兩側展現出**完全一致的 9 個 Runs 拓撲**：

| Run | 走向 | 所屬 Sections | LO 頁碼範圍 (頁數) | Quire 頁碼範圍 (頁數) | 容量差 (Quire − LO) | 累積頁數漂移 | 同位索引比對 (第 i 頁 vs 第 i 頁) 表現 |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: | :--- |
| **Run 0** | **直向 (P)** | Sec 0..1 | P1..P20 (20 頁) | P1..P21 (21 頁) | **+1 頁** | +1 頁 | 第 21 頁：LO 已進入 Run 1(L)，Quire 仍在 Run 0(P) → **LO 橫向我方直向 (第 21 頁)** |
| **Run 1** | **橫向 (L)** | Sec 2..5 | P21..P89 (69 頁) | P22..P92 (71 頁) | **+2 頁** | +3 頁 | 第 90–92 頁：LO 已進入 Run 2(P)，Quire 仍在 Run 1(L) → **LO 直向我方橫向 (第 90–92 頁)** |
| **Run 2** | **直向 (P)** | Sec 6 | P90..P94 (5 頁) | P93..P99 (7 頁) | **+2 頁** | +5 頁 | 第 95–99 頁：LO 已進入 Run 3(L)，Quire 仍在 Run 2(P) → **LO 橫向我方直向 (第 95–99 頁)** |
| **Run 3** | **橫向 (L)** | Sec 7 | P95..P122 (28 頁) | P100..P127 (28 頁) | **0 頁** | +5 頁 | 第 123–127 頁：LO 已進入 Run 4(P)，Quire 仍在 Run 3(L) → **LO 直向我方橫向 (第 123–127 頁)** |
| **Run 4** | **直向 (P)** | Sec 8..10 | P123..P136 (14 頁) | P128..P143 (16 頁) | **+2 頁** | +7 頁 | 第 137, 138, 140–143 頁：LO 在 Run 5(L) 與 Run 7(L)，Quire 仍在 Run 4(P) → **LO 橫向我方直向 (第 137, 138, 140–143 頁)** |
| **Run 5** | **橫向 (L)** | Sec 11 | P137..P138 (2 頁) | P144..P145 (2 頁) | **0 頁** | +7 頁 | 頁數精準一致（皆為 2 頁） |
| **Run 6** | **直向 (P)** | Sec 12 | P139..P139 (1 頁) | P146..P146 (1 頁) | **0 頁** | +7 頁 | 第 146 頁：LO 在 Run 7(L)，Quire 落在 Run 6(P) → **LO 橫向我方直向 (第 146 頁)** |
| **Run 7** | **橫向 (L)** | Sec 13 | P140..P177 (38 頁) | P147..P189 (43 頁) | **+5 頁** | +12 頁 | 第 178 頁：LO 已進入 Run 8(P)，Quire 仍在 Run 7(L) → **LO 直向我方橫向 (第 178 頁)** |
| **Run 8** | **直向 (P)** | Sec 14 | P178..P178 (1 頁) | P190..P190 (1 頁) | **0 頁** | +12 頁 | 頁數精準一致（皆為 1 頁） |
| **合計** | - | - | **178 頁** | **190 頁** | **+12 頁** | - | - |

**結論驗證**：PM 列出的全部 22 個錯位頁面，100% 落在上述 Runs 邊界的相位差區間內。兩側引擎之橫向段總量（LO 137 橫向頁 vs 我方 144 橫向頁）差距僅 7 頁（來自 Run 1 多 2 頁、Run 7 多 5 頁之表格分頁差異），並非誤判。

---

## 四、根因錨點與修正口徑建議

### 1. 根因錨點（代碼定位）
本案之核心根因為**評測工具之同位比較假定**與**分頁容量差異**之耦合，具體錨點如下：

- **評測層同位索引比對假定**：
  - 檔名：`scripts/render_lab.py`
  - 函式：`compare()`
  - 行區間：行 584–593
  - 問題點：
    ```python
    brks = ow.get("breaks") or []
    for i, p in enumerate(lo_pages):
        ...
        ow_w, ow_h = round(brks[i]["width_pt"], 1), round(brks[i]["height_pt"], 1)
        if abs(p["w_pt"] - ow_w) > 2.0 or abs(p["h_pt"] - ow_h) > 2.0:
            row["notes"].append(
                f"第 {i + 1} 頁 LO {p['w_pt']}×{p['h_pt']} ≠ 我方 {ow_w}×{ow_h}（sectPr）"
            )
    ```
    當文件總頁數存在位移（如 178 vs 190）時，`lo_pages[i]` 與 `brks[i]` 已不再是同一段正文內容，直接按頁碼 `i` 比對寬高會把「換行/表格累積位移」誤報為「section 幾何不一致」。

- **分頁容量差異源頭（大表格行高與段落間距）**：
  - 檔名：`core/layout/src/table.rs`（表格切片與列高量化）與 `core/layout/src/kp.rs`（Knuth-Plass 折行量測）
  - 現象：
    - Run 0（正文契約）：LO 容納於 20 頁，我方在第 20 頁放不下末尾簽名，溢出 20 行至第 21 頁（使橫向段起點自 P21 延後至 P22）。
    - Run 1 與 Run 7（大估價表）：我方表格列切片或單元格 padding 相較 LO 略為寬鬆，分別多耗 2 頁與 5 頁。

### 2. 修正口徑建議（供後續修復卡直接執行）

1. **評測工具口徑修正（`scripts/render_lab.py`）**：
   - 在比對頁面尺寸前，先提取兩側之「連續走向區段拓撲（Orientation Runs）」。
   - 若兩側 Runs 序列拓撲全等（如均為 P-L-P-L-P-L-P-L-P），則尺寸對照應採**區段錨定對齊（Run-Anchored Alignment）**或在報告中明確標註為「分頁容量位移引致之同位尺寸差」，避免誤列為「sectPr 幾何錯誤」。
2. **分頁容量收斂方向（非幾何錯誤，屬容量微調）**：
   - 檢視 `corpus/poi/test-data_document_bug65649.docx` 之俄文字型量測尺度與 `contextualSpacing`、`autoSpaceDE`、`autoSpaceDN` 預設值，使 Run 0 能在 20 頁內收斂。
   - 檢視 `core/layout/src/table.rs` 之表格列最小高度與單元格垂直邊距計算，收斂 Run 1 (+2 頁) 與 Run 7 (+5 頁) 之表格頁數。

---

## 五、受影響觸發集掃描（Corpus 普查）

使用新增之可重跑探針 `scripts/trigger_set_scan.py` 普查整個 `corpus/` 目錄（共 400 個 docx 檔案），獲得以下分佈：

### 1. 普查統計數據
- 含有 `w:sectPr` 之檔案：**399 檔**
- 含有橫向宣告（`w:orient="landscape"` 或 w > h）之檔案：**28 檔**
- 含有「混合走向」（直向與橫向並存，易受頁數漂移產生同位錯位者）：**10 檔**
- 純橫向檔案（全檔單一橫向 section 或連續橫向）：**18 檔**

### 2. 混合走向觸發集清單（共 10 檔）
以下 10 檔若出現分頁容量差異，均可能在 `render_lab.py` 的同位比對中產生類似的假性尺寸錯位回報：

| 檔案路徑 | Section 總數 | 走向序列 | 備註 |
| :--- | :---: | :--- | :--- |
| `corpus/poi/test-data_document_bug65649.docx` | 15 | `PPLLLLPLPPPLPLP` | 本案目標檔，交替 9 Runs |
| `corpus/xopt/TestFiles_HC031-Complicated-Document.docx` | 4 | `PLPP` | 複雜版面測試檔 |
| `corpus/ooxmlexport/fdo78882.docx` | 3 | `PLP` | 雙向切換 |
| `corpus/selfgen/ole_embedded_landscape_sections.docx` | 2 | `LP` | 橫轉直 |
| `corpus/ooxmlexport/fdo74605.docx` | 2 | `LP` | 橫轉直 |
| `corpus/ooxmlexport/fdo78886.docx` | 2 | `PL` | 直轉橫 |
| `corpus/ooxmlexport/tdf120511_eatenSection.docx` | 2 | `PL` | 直轉橫 |
| `corpus/ooxmlexport/tdf129575-directAfter.docx` | 2 | `PL` | 直轉橫 |
| `corpus/ooxmlexport/tdf129575-directBefore.docx` | 2 | `PL` | 直轉橫 |
| `corpus/ooxmlexport/tdf129575-styleAfter.docx` | 2 | `PL` | 直轉橫 |

---

## 六、探針工具說明

本任務交付兩支位於 `scripts/` 的冪等、唯讀探針工具：
1. `scripts/bug65649_landscape_probe.py`：
   - 專門診斷 bug65649 之 section 宣告、Quire 內部頁面寬高對照、LO PDF pymupdf 真值對照及 Runs 拓撲對照。
   - 支援 `--table` 參數印出 190 頁逐頁對照表。
2. `scripts/trigger_set_scan.py`：
   - 專門普查 corpus 中含 `w:orient` 與多 section 之觸發集清單，分類純橫向與混合走向檔案。
