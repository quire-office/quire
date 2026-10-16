# corpus 採集器（Phase0-1）

從公開授權來源蒐集真實世界 DOCX，去重+特徵標籤化後入 `corpus/`+`manifest.json`。
純 python stdlib；需 `gh`（GitHub API 列檔）與網路。重跑冪等（pool 有快取）。

## 來源與授權

| 目錄前綴 | 來源 | 性質 | 授權 |
|---|---|---|---|
| ooxmlimport/ ooxmlexport/ | LibreOffice/core sw/qa/extras | 真實用戶 bug 回報文件（tdf bugzilla，上游已匿名化） | MPL-2.0/GPL-3.0/LGPL-3.0 |
| poi/ | apache/poi test-data/document | 真實 Bugzilla 回報文件 | Apache-2.0 |
| xopt/ | Open-Xml-PowerTools (EricWhiteDev) | 功能导向範例文件（修訂/控件/域…） | MIT |
| phpword/ | PHPOffice/PHPWord | 範例/測試模板 | MIT |
| selfgen/ | scripts/corpus/gen_selfgen.py 自產 | CJK 繁/日/簡、阿拉伯 RTL、嵌入字型、altChunk 等補齊面向 | MIT OR Apache-2.0 |

每檔的來源 URL、原始路徑、授權、sha256、特徵標籤都寫進 `corpus/manifest.json`。

## 一鍵重跑

    bash scripts/corpus/collect.sh /tmp/corpus-work

流程：抓清單 → 平行下載 → 自產文件 → 探測特徵/去重 → 挑選 130 份入 corpus/ → 驗證閘門。
（`corpus/` 会被覆盖重建。）

## 各腳本

- `fetch_lo.py`：抓 LibreOffice QA docx（需同目錄 `lo_import.tsv`/`lo_export.tsv`，collect.sh 會產生）
- `fetch_extra.py`：抓 POI / Open-Xml-PowerTools / PHPWord
- `gen_selfgen.py`：手工 OOXML 自產 12 份（純 stdlib；含嵌入字型 ODTTF 混淆）
- `probe.py [pool]`：驗證 zip/word/document.xml，抽 38 項特徵標籤，sha256 去重 → `probe_results.json`
- `select.py [work] [out] [pool]`：關鍵特徵貪集合覆蓋 + 來源配額 → `corpus/` + `manifest.json`
- `check_xml.py`：zip 內全部 XML 必須 well-formed
- `../verify_corpus.py [corpus]`：閘門（≥100 檔 / json 可載入 / 實體一致 / zip 有效 / 覆蓋率統計）

## 特徵標籤（probe.py FEATURE_TAGS）

table merged_cells header_footer footnote endnote image tracked_revisions comments
numbering styles_defined theme sections columns hyperlink fields textbox equation
chart embedded_object content_control drawing legacy_vml smartart bookmark symbol
rtl east_asian superscript_subscript strike highlight paragraph_border_shading
dropcap doc_properties custom_xml altchunk glossary fonts_embedded websettings
