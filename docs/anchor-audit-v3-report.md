# scripts/ow_doc_anchor_audit.py v3 擴充與稽核報告

## 一、交付摘要與 HEAD 歸零缺口說明

本工作依規格擴充 `scripts/ow_doc_anchor_audit.py` 至 v3，新增 `[FIELD]`（struct 欄位宣告）、`[CALLSITE]`（呼叫點 token 檢查）與 `[FILE]`（模組名降檔／非符號路徑區間檢查）三級判定，修復相鄰反引號路徑片段誤抓為符號名的缺陷，並實作 `--selftest`（含欄位錯行與呼叫點無 token 兩條謊言反證紅燈）。

在 `abb5927`（`t_e6f6d731` 剛合併時，K29e 尚未合入）驗證時，`docs/editor-sdk-design.md` 為 `stale=0`（exit 0），`docs/edit-api-design.md` 由 `stale=27` 降至 `stale=2`（僅餘 L250 與 L722 兩筆指往 `core/ooxml/src/table.rs:459` doc comment 而非實際欄位行 `460`／`core/ooxml/src/lib.rs:313`）。然而目前 `HEAD`（`66460e4`）在 `abb5927` 之後合入了 K29e（`c71668c`、`6ac75f1`、`66460e4`），於 `core/engine-api/src/lib.rs` 新增 L70-72（+3 行）與 L6202-6220（+11 行），導致 `core/engine-api/src/lib.rs` 行號實際漂移。依任務白名單禁碰 `docs/editor-sdk-design.md`，且依驗收鐵律嚴禁跳過真實 STALE 檢查造假湊綠，因此 `HEAD` 重跑雙設計書皆誠實維持 exit 1（見下方逐字摘要與第三節明細），待 PM 核准更新兩份設計書內 K29e 漂移行號後即可全數歸零。

---

## 二、基線對照與零回歸驗證

| 基準 / 版本 | 設計書 | v2 ok / stale | v3 ok / stale | v3 層級分佈 (decl / field / callsite / file) | 舊 ok 回歸數 | exit code |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `HEAD` (`66460e4`) | `docs/edit-api-design.md` | `331 / 83` | `377 / 37` | `154 / 8 / 13 / 202` | `0` | `1` |
| `HEAD` (`66460e4`) | `docs/editor-sdk-design.md` | `227 / 29` | `227 / 29` | `96 / 0 / 0 / 131` | `0` | `1` |
| `abb5927` (K29e 前) | `docs/edit-api-design.md` | `387 / 27` | `412 / 2` | `185 / 8 / 17 / 202` | `0` | `1` |
| `abb5927` (K29e 前) | `docs/editor-sdk-design.md` | `256 / 0` | `256 / 0` | `125 / 0 / 0 / 131` | `0` | `0` |

### 終態 `AUDIT_SUMMARY` 逐字輸出（`HEAD`）

```text
python3 scripts/ow_doc_anchor_audit.py HEAD docs/edit-api-design.md --repo .
AUDIT_SUMMARY ok=377 stale=37 missfile=0 phantom=0 proposed=0 range=0 total=414 ok_decl=154 ok_field=8 ok_callsite=13 ok_file=202
(exit code = 1)

python3 scripts/ow_doc_anchor_audit.py HEAD docs/editor-sdk-design.md --repo .
AUDIT_SUMMARY ok=227 stale=29 missfile=0 phantom=0 proposed=0 range=0 total=256 ok_decl=96 ok_field=0 ok_callsite=0 ok_file=131
(exit code = 1)
```

### `--selftest` 逐字輸出（含謊言反證紅燈）

```text
[PASS] 正測_DECL: rc=0 (expect 0), ok_decl=1 (expect 1)
    AUDIT_SUMMARY ok=1 stale=0 missfile=0 phantom=0 proposed=0 range=0 total=1 ok_decl=1 ok_field=0 ok_callsite=0 ok_file=0
[PASS] 正測_FIELD: rc=0 (expect 0), ok_field=1 (expect 1)
      OK        L1  core/demo/src/demo_mod.rs:2-2  ident=shape_boxes  [FIELD]
    AUDIT_SUMMARY ok=1 stale=0 missfile=0 phantom=0 proposed=0 range=0 total=1 ok_decl=0 ok_field=1 ok_callsite=0 ok_file=0
[PASS] 正測_CALLSITE: rc=0 (expect 0), ok_callsite=1 (expect 1)
      OK        L1  core/demo/src/demo_mod.rs:7-7  ident=helper_call  [CALLSITE]
    AUDIT_SUMMARY ok=1 stale=0 missfile=0 phantom=0 proposed=0 range=0 total=1 ok_decl=0 ok_field=0 ok_callsite=1 ok_file=0
[PASS] 正測_FILE模組名: rc=0 (expect 0), ok_file=1 (expect 1)
      OK        L1  core/demo/src/demo_mod.rs:1-4  [FILE]
    AUDIT_SUMMARY ok=1 stale=0 missfile=0 phantom=0 proposed=0 range=0 total=1 ok_decl=0 ok_field=0 ok_callsite=0 ok_file=1
[PASS] 謊言反證_FIELD錯行紅燈: rc=1 (expect 1), stale=1 (expect 1)
      STALE     L1  core/demo/src/demo_mod.rs:3-3  ident=shape_boxes  [FIELD] 實際欄位 core/demo/src/demo_mod.rs:2
    AUDIT_SUMMARY ok=0 stale=1 missfile=0 phantom=0 proposed=0 range=0 total=1 ok_decl=0 ok_field=0 ok_callsite=0 ok_file=0
[PASS] 謊言反證_CALLSITE無token紅燈: rc=1 (expect 1), stale=1 (expect 1)
      STALE     L1  core/demo/src/demo_mod.rs:8-8  ident=helper_call  [CALLSITE] 區間無呼叫 token  實際宣告 core/demo/src/demo_mod.rs:5
    AUDIT_SUMMARY ok=0 stale=1 missfile=0 phantom=0 proposed=0 range=0 total=1 ok_decl=0 ok_field=0 ok_callsite=0 ok_file=0
[PASS] 反證_FILE越界紅燈: rc=1 (expect 1), range=1 (expect 1)
      RANGE     L1  core/demo/src/demo_mod.rs:99-99  檔長 9 行（越界或反向）
    AUDIT_SUMMARY ok=0 stale=0 missfile=0 phantom=0 proposed=0 range=1 total=1 ok_decl=0 ok_field=0 ok_callsite=0 ok_file=0
SELFTEST_SUMMARY total=7 passed=7 failed=0
```

---

## 三、`abb5927` 殘留 27 條逐條歸屬與實讀證據

以下逐條列出 `abb5927` 基線下 `docs/edit-api-design.md` 的 27 筆 STALE 在 v3 的判定層級與實際讀碼證據行號：

| # | 文件行 | 錨點 | v2 誤推 ident | v3 判定層級 | 實讀代碼證據行 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| 1 | L28 | `core/engine-api/src/lib.rs:2194-2205` | `engine` | `[FILE]` | 前鄰反引號為 sibling 錨點路徑 `core/engine-api/src/lib.rs:2706-2711`，目標區間實為 `caret_rect_json`（`abb5927` L2201；`HEAD` L2204） |
| 2 | L30 | `core/engine-api/src/lib.rs:2208-2252` | `engine` | `[FILE]` | 前鄰為 sibling 錨點路徑 `core/engine-api/src/lib.rs:1887-1902`，目標區間含 `caret_at_json`、`insert_text_at_caret` 與 `shape_boxes_vert_declared`（`abb5927` L2211-2251；`HEAD` L2214-2254） |
| 3 | L75 | `core/ooxml/src/zip_struct.rs:653-719` | `zip_struct` | `[FILE]` | 前鄰為 sibling 錨點路徑 `core/ooxml/src/zip_struct.rs:183-206` 且 `zip_struct` 為檔名模組名，目標為 `impl Planned`（`zip_struct.rs:653-719`） |
| 4 | L78 | `core/ooxml/src/zip_struct.rs:767-814` | `zip_struct` | `[FILE]` | 前鄰為 sibling 錨點路徑 `core/ooxml/src/zip_struct.rs:738-772` 且 `zip_struct` 為模組名，目標為 `fn build_central`（`zip_struct.rs:767-811`） |
| 5 | L79 | `core/ooxml/src/zip_struct.rs:487-498` | `zip_struct` | `[CALLSITE]` | 文件明文標記「呼叫點」，`zip_struct.rs:492` 含 `plan.push(Planned::fresh(` 呼叫 `fresh` token |
| 6 | L128 | `core/engine-api/src/lib.rs:2208-2252` | `engine` | `[FILE]` | 前鄰為 sibling 錨點路徑 `core/engine-api/src/lib.rs:2219-2234`，區間有效（`abb5927` 與 `HEAD` 均未越界） |
| 7 | L133 | `core/engine-api/src/lib.rs:623-624` | `engine` | `[FILE]` | 前鄰為 sibling 錨點路徑 `core/engine-api/src/lib.rs:378`，`lib.rs:623-624` 實為 `calibrated` 量化註解 |
| 8 | L133 | `core/engine-api/src/lists.rs:146-184` | `engine` | `[FILE]` | 前鄰為 sibling 錨點路徑 `core/engine-api/src/lib.rs:623-624`，`lists.rs:149-182` 實為 `expand_lvl_text` |
| 9 | L250 | `core/ooxml/src/table.rs:459-459` | `tables` | `STALE [FIELD]` | 對應 `OoxmlDoc::tables[..].para_start`，`table.rs:459` 為 `///` 註解行，實際欄位宣告 `pub para_start: usize` 位於 `core/ooxml/src/table.rs:460`（v3 嚴格拒將註解行誤判為欄位宣告） |
| 10 | L318 | `core/engine-api/src/lib.rs:618-626` | `engine` | `[FILE]` | 前鄰為 sibling 錨點路徑 `core/engine-api/src/lib.rs:336-344`，區間實為 `grid_bands_v` 量化迴圈 |
| 11 | L357 | `core/ooxml/src/drawing.rs:34-38` | `drawing` | `[FILE]` | 前鄰為 sibling 錨點路徑 `core/ooxml/src/drawing.rs:8` 且 `drawing` 為模組名，區間實為模組頭佔位語義註解 |
| 12 | L385 | `core/ooxml/src/sectpr.rs:51-51` | `sectpr` | `[FILE]` | 前鄰為 `core/ooxml/src/sectpr.rs:23` 且 `sectpr` 為模組名，`sectpr.rs:51` 為 `pgMar` 缺值註解 |
| 13 | L387 | `core/ooxml/src/sectpr.rs:54-54` | `sectpr` | `[FILE]` | 前鄰為 `core/ooxml/src/sectpr.rs:35` 且 `sectpr` 為模組名，`sectpr.rs:54` 為 `w:cols` 註解 |
| 14 | L389 | `core/ooxml/src/sectpr.rs:624-624` | `sectpr` | `[FILE]` | 前鄰為 `core/ooxml/src/sectpr.rs:75` 且 `sectpr` 為模組名，`sectpr.rs:624` 為 `_ => &mut slot.default` 分支 |
| 15 | L392 | `core/ooxml/src/sectpr.rs:160-163` | `notes` | `[FILE]` | 前鄰為 sibling 錨點路徑 `core/ooxml/src/notes.rs:48-127`，`sectpr.rs:160-163` 為 `note_footnote`／`note_endnote` 欄位 |
| 16 | L693 | `shells/native/src/main.rs:201-397` | `xml` | `[FILE]` | 前鄰為 CLI 指令字串 `quire-cli roundtrip ... --allow word/document.xml`，`main.rs:222-397` 為 `fn roundtrip` 主體 |
| 17 | L703 | `core/ooxml/src/diff.rs:110-110` | `diff` | `[FIELD]` | 前鄰為 `diff.per_entry`，`core/ooxml/src/diff.rs:110` 實為 `ZipDiff` 的 `pub per_entry: BTreeMap<String, EntryDiff>,` 欄位宣告行 |
| 18 | L713 | `core/engine-api/tests/k21_soft_br.rs:33-33` | `corpus` | `[FILE]` | 前鄰為 fixture 路徑 `corpus/ooxmlexport/fdo68607.docx`，`k21_soft_br.rs:33` 實為 `const FDO68607: &str = "ooxmlexport/fdo68607.docx";` |
| 19 | L722 | `core/ooxml/src/table.rs:459-459` | `tables` | `STALE [FIELD]` | 對應 `OoxmlDoc::tables`，`OoxmlDoc::tables` 實際欄位宣告位於 `core/ooxml/src/lib.rs:313`（`BodyTable::para_start` 位於 `core/ooxml/src/table.rs:460`），非 `table.rs:459` |
| 20 | L732 | `core/ooxml/src/styles.rs:1012-1020` | `MutationError` | `[FILE]` | 語境為 `（對齊 core/ooxml/src/styles.rs:1012-1020）` 跨 crate 對齊引用，`styles.rs:1012-1020` 為 `basedOn`／`link` 解析 |
| 21 | L750 | `core/engine-api/tests/r2_tbl_header.rs:27-27` | `corpus` | `[FILE]` | 前鄰為 `corpus/selfgen/patho_table_across_pages.docx`，`r2_tbl_header.rs:27` 為 `load("selfgen/patho_table_across_pages.docx")` |
| 22 | L764 | `core/engine-api/tests/r2_tbl_header.rs:27-27` | `corpus` | `[FILE]` | 同上，`r2_tbl_header.rs:27` |
| 23 | L781 | `core/engine-api/tests/k7b_header_push.rs:31-31` | `corpus` | `[FILE]` | 前鄰為 `corpus/poi/test-data_document_Bug51170.docx`，`k7b_header_push.rs:31` 為 `const B51170: &str = "poi/test-data_document_Bug51170.docx";` |
| 24 | L786 | `core/engine-api/tests/r7_footnote_anchor.rs:30-30` | `corpus` | `[FILE]` | 前鄰為 `corpus/poi/test-data_document_form_footnotes.docx`，`r7_footnote_anchor.rs:30` 為 `const FORM: &str = "poi/test-data_document_form_footnotes.docx";` |
| 25 | L791 | `core/engine-api/tests/k9_hf_align.rs:16-16` | `engine` | `[FILE]` | 前鄰為 sibling 錨點路徑 `core/engine-api/src/lib.rs:5267`，`k9_hf_align.rs:16` 為 `const HF_FIXTURE: &[u8]` |
| 26 | L792 | `core/engine-api/tests/k7b_header_push.rs:31-31` | `corpus` | `[FILE]` | 同 #23，`k7b_header_push.rs:31` |
| 27 | L793 | `core/engine-api/tests/r7_footnote_anchor.rs:30-30` | `corpus` | `[FILE]` | 同 #24，`r7_footnote_anchor.rs:30` |

---

## 四、`HEAD` 無法歸零之實測根因（K29e 合入後之行號漂移）

於 `abb5927` 之後，K29e（`c71668c`／`6ac75f1`／`66460e4`）修改 `core/engine-api/src/lib.rs`：

1. `core/engine-api/src/lib.rs:70-72` 新增 `VertAlign` re-export 與註解（+3 行），使 `EngineError` 移至 L74、`Engine` 移至 L97（`para_sec` L154、`shape_boxes` L157、`image_pixels` L164、`touched` L109）、`PixelSource` 移至 L236、`page_count` 移至 L2114、`save_docx` 移至 L2813、`save_xlsx` 移至 L2914。
2. `core/engine-api/src/lib.rs:6202-6220` 擴充上下標發射臂（+11 行，累計 +14 行），使 `sort_display_items` 移至 L7559、`edit_syncs_paras_eff_styles_and_round_trip` 結尾斷言（原 `8955-8957`）移至 `8969-8971`。

上述行號漂移造成 `docs/editor-sdk-design.md` 產生 29 筆真實 STALE（全數指向 `core/engine-api/src/lib.rs` 已偏移行號），`docs/edit-api-design.md` 亦增加 35 筆真實 STALE（加上 `table.rs:459` 2 筆共 37 筆）。因 `docs/editor-sdk-design.md` 不在本任務白名單內且嚴禁放寬判定造假，建議於下一張文件同步卡一次更新兩份設計書中受 K29e 影響的 `lib.rs` 錨點行號及 `docs/edit-api-design.md` L250／L722 的 `table.rs:459` 錨點。
