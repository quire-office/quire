# 走向拓撲錨定判準翻盤觸發集對照（bug65649 衍生，t_026e8c2e）

- 觸發集：混合走向 10 檔（docs/bug65649-landscape-diag.md §五）；成功比對 10 檔、LO 失敗 0 檔。
- 注記翻盤（新舊口徑注記不同）：**2 檔**。

## 總表

| 檔案 | LO 頁 | 我方頁 | 拓撲 LO | 拓撲我方 | 拓撲全等 | 舊 sectPr 筆數 | 新 sectPr 筆數 | 翻盤 |
|---|---:|---:|---|---|:-:|---:|---:|:-:|
| corpus/poi/test-data_document_bug65649.docx | 178 | 190 | PLPLPLPLP | PLPLPLPLP | 是 | 22 | 0 | **是** |
| corpus/xopt/TestFiles_HC031-Complicated-Document.docx | 9 | 9 | PLP | PLP | 是 | 0 | 0 | 否 |
| corpus/ooxmlexport/fdo78882.docx | 3 | 2 | PLP | PL | 否 | 0 | 0 | **是** |
| corpus/selfgen/ole_embedded_landscape_sections.docx | 2 | 2 | LP | LP | 是 | 0 | 0 | 否 |
| corpus/ooxmlexport/fdo74605.docx | 2 | 2 | LP | LP | 是 | 0 | 0 | 否 |
| corpus/ooxmlexport/fdo78886.docx | 2 | 2 | PL | PL | 是 | 0 | 0 | 否 |
| corpus/ooxmlexport/tdf120511_eatenSection.docx | 2 | 2 | PL | PL | 是 | 0 | 0 | 否 |
| corpus/ooxmlexport/tdf129575-directAfter.docx | 3 | 3 | PL | PL | 是 | 0 | 0 | 否 |
| corpus/ooxmlexport/tdf129575-directBefore.docx | 3 | 3 | PL | PL | 是 | 0 | 0 | 否 |
| corpus/ooxmlexport/tdf129575-styleAfter.docx | 3 | 3 | PL | PL | 是 | 0 | 0 | 否 |

## 逐檔前後注記對照

### corpus/poi/test-data_document_bug65649.docx

- 舊口徑注記（22 筆）：
  - 第 21 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 90 頁 LO 595.3×841.9 ≠ 我方 842.0×595.4（sectPr）
  - 第 91 頁 LO 595.3×841.9 ≠ 我方 842.0×595.4（sectPr）
  - 第 92 頁 LO 595.3×841.9 ≠ 我方 842.0×595.4（sectPr）
  - 第 95 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 96 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 97 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 98 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 99 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 123 頁 LO 595.3×841.9 ≠ 我方 841.9×595.3（sectPr）
  - 第 124 頁 LO 595.3×841.9 ≠ 我方 841.9×595.3（sectPr）
  - 第 125 頁 LO 595.3×841.9 ≠ 我方 841.9×595.3（sectPr）
  - 第 126 頁 LO 595.3×841.9 ≠ 我方 841.9×595.3（sectPr）
  - 第 127 頁 LO 595.3×841.9 ≠ 我方 841.9×595.3（sectPr）
  - 第 137 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 138 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 140 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 141 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 142 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 143 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 146 頁 LO 841.9×595.3 ≠ 我方 595.3×841.9（sectPr）
  - 第 178 頁 LO 595.3×841.9 ≠ 我方 841.9×595.3（sectPr）
- 新口徑注記（6 筆）：
  - 走向拓撲全等（9 段 PLPLPLPLP）；總頁數差 +12 頁＝分頁容量漂移（非 sectPr），尺寸注記改 run 錨定
  - 第 1 段（直向）容量漂移 +1 頁（LO 20 頁／我方 21 頁）
  - 第 2 段（橫向）容量漂移 +2 頁（LO 69 頁／我方 71 頁）
  - 第 3 段（直向）容量漂移 +2 頁（LO 5 頁／我方 7 頁）
  - 第 5 段（直向）容量漂移 +2 頁（LO 14 頁／我方 16 頁）
  - 第 8 段（橫向）容量漂移 +5 頁（LO 38 頁／我方 43 頁）

### corpus/xopt/TestFiles_HC031-Complicated-Document.docx

- 舊口徑：（無注記）
- 新口徑：（無注記）

### corpus/ooxmlexport/fdo78882.docx

- 舊口徑注記（1 筆）：
  - 第 3 頁我方無 pagebreak 記錄
- 新口徑注記（2 筆）：
  - 走向拓撲不一致：LO=PLP 我方=PL
  - 第 3 頁我方無 pagebreak 記錄

### corpus/selfgen/ole_embedded_landscape_sections.docx

- 舊口徑：（無注記）
- 新口徑：（無注記）

### corpus/ooxmlexport/fdo74605.docx

- 舊口徑：（無注記）
- 新口徑：（無注記）

### corpus/ooxmlexport/fdo78886.docx

- 舊口徑：（無注記）
- 新口徑：（無注記）

### corpus/ooxmlexport/tdf120511_eatenSection.docx

- 舊口徑：（無注記）
- 新口徑：（無注記）

### corpus/ooxmlexport/tdf129575-directAfter.docx

- 舊口徑：（無注記）
- 新口徑：（無注記）

### corpus/ooxmlexport/tdf129575-directBefore.docx

- 舊口徑：（無注記）
- 新口徑：（無注記）

### corpus/ooxmlexport/tdf129575-styleAfter.docx

- 舊口徑：（無注記）
- 新口徑：（無注記）
