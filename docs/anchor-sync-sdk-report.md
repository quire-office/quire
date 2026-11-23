# 錨點同步報告：docs/editor-sdk-design.md（K29e／U4 合併後行號漂移）

- 分支：`oc/anchor-sync-sdk`（基線 `main`）
- 任務：修正 K29e（`c71668c`／`6ac75f1`／`66460e4`）與 U4 合併造成的 48 條 STALE 錨點行號漂移。
- 方法：稽核器 v3（`scripts/ow_doc_anchor_audit.py`，本體未動）逐條列 STALE；對每條以 `git show HEAD:<檔>` ＋ `sed -n` 實讀源檔，取符號真實宣告行／欄位行／呼叫行，原位只改行號數字，區間寬度逐條保持 old＝new。
- 終態稽核：`AUDIT_SUMMARY ok=256 stale=0 missfile=0 phantom=0 proposed=0 range=0 total=256 ok_decl=125 ok_field=0 ok_callsite=0 ok_file=131`，exit code `0`。
- 白名單合規：全部 commit 只涉及 `docs/editor-sdk-design.md`（修改）與本報告（新建）；`core/`、`shells/`、`scripts/`、`tools/` 零改動。

## 逐條真值對照表（48 條）

證據行文字皆取自 `git show HEAD:core/engine-api/src/lib.rs`（除第 7 條 FIELD 條目外，同檔）。「宣告行」指區間首行；「收行」指區間末行。

| # | 條目（稽核器 STALE 行） | 舊行 | 新行 | 寬度 | git show 實讀證據行文字 |
|---|---|---|---|---|---|
| 1 | L84 `reflow_step_json` | 2800-2805 | 2860-2865 | 6→6 | 2860 `pub fn reflow_step_json(&mut self) -> String {`；2865 `    }` |
| 2 | L1943 `reflow_step_json` | 2800-2805 | 2860-2865 | 6→6 | 同上（同一符號第二次出現） |
| 3 | L85 `set_sync_budget` | 2789-2791 | 2849-2851 | 3→3 | 2849 `pub fn set_sync_budget(&mut self, pages: Option<usize>) {`；2850 `self.layout.set_sync_budget(pages);`；2851 `    }` |
| 4 | L1943 `set_sync_budget` | 2789-2791 | 2849-2851 | 3→3 | 同上（同一符號第二次出現） |
| 5 | L116 `from_xlsx_bytes` | 2902-2908 | 2962-2968 | 7→7 | 2962 `pub fn from_xlsx_bytes(bytes: &[u8]) -> Result<Self, EngineError> {`；2968 `    }` |
| 6 | L1945 `from_xlsx_bytes` | 2902-2908 | 2962-2968 | 7→7 | 同上（同一符號第二次出現） |
| 7 | L132 `EngineError` | 64-73 | 74-83 | 10→10 | 74 `pub enum EngineError {`；83 `}` |
| 8 | L1612 `EngineError` | 64-73 | 74-83 | 10→10 | 同上（同一符號第二次出現） |
| 9 | L1935 `EngineError` | 64-73 | 74-83 | 10→10 | 同上（同一符號第三次出現；該表列三處錨點同改） |
| 10 | L1281 `insert_text_at_caret` | 2219-2234 | 2279-2294 | 16→16 | 2279 `pub fn insert_text_at_caret(&mut self, para: usize, cp: usize, text: &str) -> Vec<String> {`；2294 `    }` |
| 11 | L1764 `insert_text_at_caret` | 2219-2234 | 2279-2294 | 16→16 | 同上（同一符號第二次出現） |
| 12 | L1939 `insert_text_at_caret` | 2219-2234 | 2279-2294 | 16→16 | 同上（同一符號第三次出現；同表列首欄錨點一併同改） |
| 13 | L1281 `replace_paragraph_text` | 2739-2783 | 2799-2843 | 45→45 | 2799 `pub fn replace_paragraph_text(&mut self, para: usize, text: &str) -> Vec<String> {`；2843 `    }` |
| 14 | L1942 `replace_paragraph_text` | 2739-2783 | 2799-2843 | 45→45 | 同上（同一符號第二次出現） |
| 15 | L1281 `parts`（FIELD） | 8955-8957 | 9026-9028 | 3→3 | 9026 `// round-trip：parts passthrough——發射投影 edits 不得污染存檔。`；9027 `let saved = engine.save_docx().expect("存檔");`；9028 `assert_eq!(saved.as_slice(), FX, "編輯發射投影後存檔仍須位元組級相同");`（同測試 `edit_syncs_paras_eff_styles_and_round_trip`，fn 宣告於 9003） |
| 16 | L1596 `save_docx` | 2810-2812 | 2870-2872 | 3→3 | 2870 `pub fn save_docx(&self) -> Result<Vec<u8>, EngineError> {`；2871 `Ok(self.doc.save()?)`；2872 `    }` |
| 17 | L1611 `save_docx` | 2810-2812 | 2870-2872 | 3→3 | 同上 |
| 18 | L1760 `save_docx` | 2810-2812 | 2870-2872 | 3→3 | 同上 |
| 19 | L1944 `save_docx` | 2810-2812 | 2870-2872 | 3→3 | 同上 |
| 20 | L1596 `save_xlsx` | 2911-2913 | 2971-2973 | 3→3 | 2971 `pub fn save_xlsx(&self) -> Result<Vec<u8>, EngineError> {`；2972 `Ok(self.bundle.save()?)`；2973 `    }` |
| 21 | L1611 `save_xlsx` | 2911-2913 | 2971-2973 | 3→3 | 同上 |
| 22 | L1760 `save_xlsx` | 2911-2913 | 2971-2973 | 3→3 | 同上 |
| 23 | L1945 `save_xlsx` | 2911-2913 | 2971-2973 | 3→3 | 同上 |
| 24 | L1611 `preserved_part_count` | 2126-2128 | 2129-2131 | 3→3 | 2129 `pub fn preserved_part_count(&self) -> usize {`；2130 `self.doc.parts.len()`；2131 `    }` |
| 25 | L1613 `preserved_part_count` | 2126-2128 | 2129-2131 | 3→3 | 同上 |
| 26 | L1938 `preserved_part_count` | 2126-2128 | 2129-2131 | 3→3 | 同上 |
| 27 | L1611 `roundtrip_diff` | 2871-2878 | 2931-2938 | 8→8 | 2931 `pub fn roundtrip_diff(`；2938 `    }` |
| 28 | L1944 `roundtrip_diff` | 2871-2878 | 2931-2938 | 8→8 | 同上 |
| 29 | L1611 `xlsx_roundtrip_diff` | 2946-2951 | 3006-3011 | 6→6 | 3006 `pub fn xlsx_roundtrip_diff(`；3011 `}` |
| 30 | L1945 `xlsx_roundtrip_diff` | 2946-2951 | 3006-3011 | 6→6 | 同上 |
| 31 | L1613 `page_count` | 2111-2112 | 2114-2115 | 2→2 | 2114 `pub fn page_count(&self) -> usize {`；2115 `self.layout.page_count() + self.end.pages.len()` |
| 32 | L1938 `page_count` | 2111-2112 | 2114-2115 | 2→2 | 同上 |
| 33 | L1613 `paragraph_count` | 2121-2123 | 2124-2126 | 3→3 | 2124 `pub fn paragraph_count(&self) -> usize {`；2125 `self.doc.paragraphs.len()`；2126 `    }` |
| 34 | L1938 `paragraph_count` | 2121-2123 | 2124-2126 | 3→3 | 同上 |
| 35 | L1613 `table_count` | 2132-2134 | 2135-2137 | 3→3 | 2135 `pub fn table_count(&self) -> usize {`；2136 `self.doc.tables.len()`；2137 `    }` |
| 36 | L1938 `table_count` | 2132-2134 | 2135-2137 | 3→3 | 同上 |
| 37 | L1938 `tables_json` | 2137-2139 | 2140-2142 | 3→3 | 2140 `pub fn tables_json(&self) -> String {`；2141 `serde_json::to_string(&self.doc.tables)...`；2142 `    }` |
| 38 | L1613 `is_settled` | 2794-2796 | 2854-2856 | 3→3 | 2854 `pub fn is_settled(&self) -> bool {`；2855 `self.layout.is_settled()`；2856 `    }` |
| 39 | L1943 `is_settled` | 2794-2796 | 2854-2856 | 3→3 | 同上 |
| 40 | L1613 `sheet_count` | 2933-2935 | 2993-2995 | 3→3 | 2993 `pub fn sheet_count(&self) -> usize {`；2994 `self.bundle.workbook_model().sheets.len()`；2995 `    }` |
| 41 | L1945 `sheet_count` | 2933-2935 | 2993-2995 | 3→3 | 同上 |
| 42 | L1941 `page_display_list_json` | 2714-2717 | 2774-2777 | 4→4 | 2774 `pub fn page_display_list_json(&self, page: usize) -> Option<String> {`；2777 `    }` |
| 43 | L1944 `roundtrip_check` | 2816-2823 | 2876-2883 | 8→8 | 2876 `pub fn roundtrip_check(&self) -> Result<ZipDiff, EngineError> {`；2883 `    }` |
| 44 | L1944 `set_part` | 2854-2857 | 2914-2917 | 4→4 | 2914 `pub fn set_part(&mut self, name: &str, bytes: Vec<u8>) {`；2917 `    }` |
| 45 | L1945 `XlsxDoc` | 2893-2941 | 2953-3001 | 49→49 | 2953 `pub struct XlsxDoc {`；3001 `}` |
| 46 | L1945 `workbook_model_json` | 2928-2930 | 2988-2990 | 3→3 | 2988 `pub fn workbook_model_json(&self) -> String {`；2990 `    }` |
| 47 | L1771 `CollabSession` | 2973-3009 | 3033-3069 | 37→37 | 3033 `pub struct CollabSession {`；3069 `}` |
| 48 | L1946 `CollabSession` | 2973-3009 | 3033-3069 | 37→37 | 同上 |

## 第 15 條（FIELD 級）判定細節

- 舊錨點 `core/engine-api/src/lib.rs:8955-8957` 為行內路徑錨點，緊鄰前行代碼片段 `self.doc.parts`（間隔「（單元測試 」恰 6 字元），稽核器判為 FIELD 級 `ident=parts`；`git show HEAD` 實讀 8955-8957 為 leader 展開斷言（`(pb.1 - top_of(pb.0)).abs() < 0.01` 等），與「單元測試直接斷言編輯後 `save_docx()` 與原檔位元組相同」語意不符——真值測試已漂移。
- 真值測試為 `edit_syncs_paras_eff_styles_and_round_trip`（`#[test]` 於 9002、fn 於 9003），其存檔位元組相同斷言收尾三行為 9026-9028（含 `parts passthrough` 註解、`let saved = engine.save_docx()`、`assert_eq!(saved.as_slice(), FX, ...)`）。
- 新錨點 9026-9028 寬度 3 與舊區間相等；區間同時涵蓋該測試「parts 不動 ⇒ 存檔位元組相同」的註解與斷言本體，語意與原文宣稱一致。
- 附註：`parts` 欄位真實宣告在 `core/ooxml/src/lib.rs:321`（`pub parts: BTreeMap<String, Vec<u8>>`，實讀證據）與 `tools/scrub/src/main.rs:12`，皆在唯讀禁區；本條僅修正 engine-api 內測試斷言區間行號，未動上述檔案。

## 提交紀錄

| commit | 訊息 | 條數 |
|---|---|---|
| `f933675` | 文件(錨點同步): 10 條 | 10 |
| `a3b85d3` | 文件(錨點同步): 8 條 | 8 |
| `16d2ab0` | 文件(錨點同步): 9 條 | 9 |
| `1eefee3` | 文件(錨點同步): 7 條 | 7 |
| `30c8af9` | 文件(錨點同步): 10 條 | 10 |
| `b6c1dea` | 文件(錨點同步): 4 條 | 4 |

## 終態稽核輸出（逐字）

```
AUDIT_SUMMARY ok=256 stale=0 missfile=0 phantom=0 proposed=0 range=0 total=256 ok_decl=125 ok_field=0 ok_callsite=0 ok_file=131
```

exit code：`0`（綠燈：stale=0 且 missfile=0 且 phantom=0 且 range=0）。
