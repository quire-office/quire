# corpus 採集器（Phase 0-1 建成的 130 → 擴充 ≥400）

從公開授權來源蒐集真實世界 DOCX，去重+特徵標籤化後入 `corpus/`+`manifest.json`。
純 python stdlib；需 `gh`（GitHub API 列檔）與網路。重跑冪等（pool 有快取；
phase0 既有 130 檔位元級沿用，不被未消毒 pool 版覆寫）。

## 來源與授權

| 目錄前綴 | 來源 | 性質 | 授權 |
|---|---|---|---|
| ooxmlimport/ ooxmlexport/ | LibreOffice/core sw/qa/extras | 真實用戶 bug 回報文件（tdf bugzilla，上游已匿名化） | MPL-2.0/GPL-3.0/LGPL-3.0 |
| poi/ | apache/poi test-data/document | 真實 Bugzilla 回報文件 | Apache-2.0 |
| xopt/ | Open-Xml-PowerTools (EricWhiteDev) | 功能導向範例文件（修訂/控件/域…） | MIT |
| phpword/ | PHPOffice/PHPWord | 範例/測試模板 | MIT |
| oxmlsdk/ | dotnet/Open-XML-SDK test assets | Microsoft 官方 Wordprocessing 測試文件 | MIT |
| docx4j/ | plutext/docx4j test resources + samples | 功能測試與範例（字體/巢狀/AlternateContent…） | Apache-2.0 |
| pydocx/ | python-openxml/python-docx | behave test files | MIT |
| selfgen/ | scripts/corpus/gen_selfgen.py 自產 | CJK 繁/日/簡、阿拉伯 RTL、嵌入字型、altChunk、病態表格補齊 | MIT OR Apache-2.0 |

每檔的來源 URL、原始路徑、授權（SPDX＋來源說明，verify 閘門檢查）、批次
（phase0/expand）、sha256、特徵標籤都寫進 `corpus/manifest.json`。

## 隔離清單（quarantine）

`corpus/quarantine.json`：`{"files": {"<相對路徑>": "<原因>"}}`。
`quire-cli roundtrip <dir> --check` 自動讀取同目錄 quarantine.json（或
`--quarantine <path>`），清單內檔案跳過評測、不計紅，彙報單列 `Q` 行。
parse smoke 同步跳過並列原因。紅燈修復後從清單移除；不得用隔離掩蓋倒退。

## 一鍵重跑

    bash scripts/corpus/collect.sh /tmp/corpus-work

流程：抓清單 → 平行下載 → 自產文件 → 探測特徵/去重 → 挑選 ≥400 份入 corpus/
（phase0 沿用＋配額補齊）→ PII 消毒 → 驗證閘門。（`corpus/` 會被覆蓋重建，
phase0 檔例外沿用消毒後位元級版本。）

## 各腳本

- `fetch_lo.py`：抓 LibreOffice QA docx（需同目錄 `lo_import.tsv`/`lo_export.tsv`，collect.sh 會產生）
- `fetch_extra.py`：抓 POI / Open-Xml-PowerTools / PHPWord / Open-XML-SDK / docx4j / python-docx
- `gen_selfgen.py`：手工 OOXML 自產 26 份（純 stdlib；嵌入字型 ODTTF 混淆＋14 份病態表格）
- `probe.py [pool]`：驗證 zip/word/document.xml，抽 46 項特徵標籤（含 8 項病態表格），sha256 去重 → `probe_results.json`
- `select.py [work] [out] [pool]`：phase0 沿用＋病態必收＋關鍵特徵貪集合覆蓋 + 來源配額（單檔 ≤500KB、總量 ≤60MB）→ `corpus/` + `manifest.json`
- `check_xml.py`：zip 內全部 XML 必須 well-formed
- `../scrub_corpus.py`：PII 消毒（creator/lastModifiedBy/company/email 遮罩），zip 損壞檔記錄不掩蓋
- `../verify_corpus.py [corpus]`：閘門（≥400 檔 / json 可載入 / 實體一致 / zip 有效 / 大小上限 / license 白名單 / 病態表格量化門檻 / 覆蓋率統計）

## 病態表格量化門檻（PM 2026-10-16，verify_corpus.py 檢查）

| 標籤 | 門檻 | 說明 |
|---|---|---|
| nested_table_3plus | ≥5 | 巢狀表格 ≥3 層 |
| table_across_pages | ≥3 | 單表估算高度 >1 頁（trHeight 總和 >12000 twips） |
| merged_interleave | ≥5 | vMerge×gridSpan 同格交錯或行列 span 總和≠grid 欄數 |
| tbl_layout_autofit | ≥3 | autofit（含未宣告）且欄寬極端（≥9000 或 ≤144 twips） |
| table_in_columns | ≥3 | section cols≥2 內含表格 |
| table_cell_multi_para / table_cell_list | ≥5 | 儲存格內 ≥3 段落 / 掛 numPr 清單 |

## 特徵標籤（probe.py FEATURE_TAGS，46 項）

table merged_cells header_footer footnote endnote image tracked_revisions comments
numbering styles_defined theme sections columns hyperlink fields textbox equation
chart embedded_object content_control drawing legacy_vml smartart bookmark symbol
rtl east_asian superscript_subscript strike highlight paragraph_border_shading
dropcap doc_properties custom_xml altchunk glossary fonts_embedded websettings
nested_table nested_table_3plus table_cell_multi_para table_cell_list
table_across_pages tbl_layout_autofit merged_interleave table_in_columns
