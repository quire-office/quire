# E2 直排文字（wps:bodyPr/@vert）紅燈快照與 LO 真值抽取口徑

署名：ADT109119

## 1. 任務背景與度量閘門補強成果
`corpus/ooxmlexport/btlr-textbox.docx` 的直排文字在修復前面臨「橫排發射＋度量偽影雙缺陷」：
LibreOffice 側實測文字行向量為 `dir=[0.0, -1.0]`（整行由下而上旋轉，兩段由左向右分欄）；我方先前將文字框內容橫排發射（`dir=[1.0, 0.0]`），兩段文字重疊在相同橫排基準線上，但舊版 `scripts/render_lab.py` 版面比對僅檢查行文字集合與行順序，未納入行向量與高寬比，導致橫發也能獲得內容 1.000／版面 1.000 之滿分偽影。

同卡已在 `scripts/render_lab.py` 中補強度量閘門：
1. `pdf_page_facts` 增補 pymupdf `line["dir"]` 與 `line["bbox"]` 抽取口徑。
2. `run_quire` 依據文字項目方向旗標（`FLAG_DIR_VERT270`、`FLAG_DIR_VERT`、`FLAG_DIR_EAVERT`）精確輸出行方向向量 `line_dirs`。
3. `compare` 加入行方向向量比對：若 LO 為垂直向量（`dir=[0, ±1]`）而我方為橫排向量（`dir=[1, 0]`），該頁版面分直接判失敗（`ls = 0.0`），且記號 `line_direction_mismatch:LO直排我方橫排`。
4. 補強後實測驗證：`render_lab.py --selftest` 8/8 全綠；`btlr-textbox.docx` 版面分數成功由 1.000 降為 0.000，確實驗出直排缺失；基準四檔（fdo76591、WC048、n751077、picture-with-schemecolor）分數完全無劣化。

## 2. LibreOffice pymupdf 真值抽取口徑
抽樣方法：使用 LibreOffice 產出之參考 PDF，透過 `pymupdf`（fitz）走訪頁面文字區塊之 `lines`，讀取每個 `line["dir"]` 與 `line["bbox"]`。
抽樣腳本：
```python
import fitz

def extract_line_facts(pdf_path):
    doc = fitz.open(pdf_path)
    facts = []
    for page_idx, page in enumerate(doc):
        d = page.get_text("dict")
        for b in d.get("blocks", []):
            for l in b.get("lines", []):
                text = "".join(s.get("text", "") for s in l.get("spans", []))
                facts.append((page_idx, text, l["dir"], [round(v, 2) for v in l["bbox"]]))
    return facts
```

抽樣定讞數值：
1. `btlr-textbox.docx`（vert270 族）：
   - `First paragraph.`: `dir=(0.0, -1.0)`, bbox=`[225.78, 154.23, 239.2, 224.9]`
   - `Second paragraph.`: `dir=(0.0, -1.0)`, bbox=`[239.23, 141.13, 252.65, 224.9]`
2. `fdo69636.docx`（vert270 族第二檔）：
   - `Vertical Text`: `dir=(0.0, -1.0)`, bbox=`[90.0, 72.57, 111.1, 170.55]`
3. `tbrl-textbox.docx`（vert 族）：
   - `AAA1.`: `dir=(0.0, 1.0)`, bbox=`[382.4, 78.7, 395.82, 106.12]`
   - `AAA2.`: `dir=(0.0, 1.0)`, bbox=`[356.95, 78.7, 370.37, 106.12]`
4. `tdf124754.docx`（vert 族第二檔）：
   - `This is a hyperlink: `: `dir=(0.0, 1.0)`, bbox=`[126.74, 83.85, 137.81, 163.85]`
5. `fdo74605.docx`（eaVert 族）：
   - DrawingML 標記 `<wps:bodyPr ... vert="eaVert"/>`

## 3. 紅燈考卷 `core/engine-api/tests/e2_vertical_text.rs` 執行快照
執行指令：`cargo test -p quire-engine --test e2_vertical_text`
執行結果（修復前）：4 失敗，1 通過。

```
running 5 tests
test e2_lamp1_vert_attr_parsing_and_classification ... FAILED
test e2_lamp5_vert_east_asian_cjk_stack_no_regression ... ok
test e2_lamp4_vert_emission_tbrl_and_tdf124754 ... FAILED
test e2_lamp2_vert270_emission_btlr_textbox ... FAILED
test e2_lamp3_vert270_second_file_fdo69636 ... FAILED

failures:

---- e2_lamp1_vert_attr_parsing_and_classification stdout ----
thread 'e2_lamp1_vert_attr_parsing_and_classification' panicked at core/engine-api/tests/e2_vertical_text.rs:26:5:
assertion `left == right` failed: btlr-textbox 應解析出 vert270 宣告
  left: None
 right: Some("vert270")

---- e2_lamp2_vert270_emission_btlr_textbox stdout ----
thread 'e2_lamp2_vert270_emission_btlr_textbox' panicked at core/engine-api/tests/e2_vertical_text.rs:123:5:
First paragraph. flags=0x0 未帶 FLAG_DIR_VERT270 方向旗標（仍為橫排發射）

---- e2_lamp3_vert270_second_file_fdo69636 stdout ----
thread 'e2_lamp3_vert270_second_file_fdo69636' panicked at core/engine-api/tests/e2_vertical_text.rs:181:8:
必須存在 Vertical Text 文字項

---- e2_lamp4_vert_emission_tbrl_and_tdf124754 stdout ----
thread 'e2_lamp4_vert_emission_tbrl_and_tdf124754' panicked at core/engine-api/tests/e2_vertical_text.rs:232:5:
AAA1. flags=0x0 未帶 FLAG_DIR_VERT 方向旗標（dir=[0, 1]）

failures:
    e2_lamp1_vert_attr_parsing_and_classification
    e2_lamp2_vert270_emission_btlr_textbox
    e2_lamp3_vert270_second_file_fdo69636
    e2_lamp4_vert_emission_tbrl_and_tdf124754

test result: FAILED. 1 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

## 4. 預期修復步驟
1. 考卷 1：在 `core/ooxml/src/drawing.rs` 中實裝 `wps:bodyPr/@vert` 屬性解析，向後相容落地至 `InlineImage.txbx_vert`，並完成 `vert`、`vert270`、`vertEastAsian*`（`eaVert`）三族分類。
2. 考卷 2 & 3：在 `core/engine-api/src/lib.rs` 的 `emit_txbx_texts` 中實裝 `vert270` 整行旋轉發射（`dir≈[0, -1]`），段落由左向右分欄，命中 LO bbox 真值 ±4pt。
3. 考卷 4：在 `emit_txbx_texts` 中實裝 `vert` 整行旋轉發射（`dir≈[0, 1]`），段落由右向左分欄，命中 LO bbox 真值 ±4pt。
4. 考卷 5：確認 `vertEastAsian*` 與 CJK 堆疊臂保持不變，回歸驗證通過。
