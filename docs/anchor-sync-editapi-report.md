# docs/edit-api-design.md 錨點同步報告（K29e 合併後行號漂移）

- 分支：`oc/anchor-sync-editapi`
- 起始基線：`main`（`db865bf`）
- 交付時 `main` 已前進至 `090ab1b`（opencc 純度補正，僅 core/scripts/shells 註解與檔頭文字）；已以 `git merge --no-edit main` 非破壞性整合，使 `main..HEAD` diff 僅剩白名單檔
- 白名單：僅 `docs/edit-api-design.md`；本報告為新建檔
- 稽核器：`scripts/ow_doc_anchor_audit.py`（v3，未修改本體）

## 稽核結果

起點（基線 main 文件）：

```
AUDIT_SUMMARY ok=353 stale=61 missfile=0 phantom=0 proposed=0 range=0 total=414 ok_decl=131 ok_field=8 ok_callsite=12 ok_file=202
```

終點（本分支 HEAD）：

```
AUDIT_SUMMARY ok=412 stale=2 missfile=0 phantom=0 proposed=0 range=0 total=414 ok_decl=171 ok_field=18 ok_callsite=21 ok_file=202
exit code = 1
```

殘留 2 條均為 v3 推論誤判，已於文末逐條列證，未為湊 exit 0 而改動。

本批共修正 61 條 STALE 錨點，另同步 1 條被 v3 降為 FILE 級、實際亦漂移的錨點（L128 第二錨點）。
逐行去數字後新舊行逐字相等：47/47 行；區間寬度總和 old=1217 → new=1141（未增大）。

## 逐條真值對照表

| 條目 | 舊行 | 新行 | git show 實讀證據行文字 |
| :--- | :--- | :--- | :--- |
| L21 `parse` | `core/engine-api/src/lib.rs:323` | `core/engine-api/src/lib.rs:326` | `let doc = parse(bytes)?;` |
| L22 `page_count` | `core/engine-api/src/lib.rs:2111-2113` | `core/engine-api/src/lib.rs:2114-2116` | `pub fn page_count(&self) -> usize {` |
| L23 `paragraph_count` | `core/engine-api/src/lib.rs:2121-2123` | `core/engine-api/src/lib.rs:2124-2126` | `pub fn paragraph_count(&self) -> usize {` |
| L24 `preserved_part_count` | `core/engine-api/src/lib.rs:2126-2128` | `core/engine-api/src/lib.rs:2129-2131` | `pub fn preserved_part_count(&self) -> usize {` |
| L25 `tables_json` | `core/engine-api/src/lib.rs:2132-2139` | `core/engine-api/src/lib.rs:2135-2142` | `pub fn table_count(&self) -> usize {` |
| L26 `paragraph_text` | `core/engine-api/src/lib.rs:2192-2194` | `core/engine-api/src/lib.rs:2252-2254` | `pub fn paragraph_text(&self, para: usize) -> Option<String> {` |
| L28 `page_breaks_json` | `core/engine-api/src/lib.rs:2706-2711` | `core/engine-api/src/lib.rs:2766-2771` | `pub fn page_breaks_json(&self) -> String {` |
| L28(2) `sort_display_items` | `core/engine-api/src/lib.rs:7545` | `core/engine-api/src/lib.rs:7616` | `fn sort_display_items(` |
| L29 `reflow_step_json` | `core/engine-api/src/lib.rs:2789-2805` | `core/engine-api/src/lib.rs:2849-2865` | `pub fn set_sync_budget(&mut self, pages: Option<usize>) {` |
| L30 `str` | `core/engine-api/src/lib.rs:1887-1902` | `core/engine-api/src/lib.rs:2279-2293` | `pub fn insert_text_at_caret(&mut self, para: usize, cp: usize, text: &str) -> Vec<String> {` |
| L31 `roundtrip_diff` | `core/engine-api/src/lib.rs:2871-2878` | `core/engine-api/src/lib.rs:2931-2938` | `pub fn roundtrip_diff(` |
| L32 `xlsx_roundtrip_diff` | `core/engine-api/src/lib.rs:2892-2958` | `core/engine-api/src/lib.rs:2953-3011` | `pub struct XlsxDoc {` |
| L33 `apply_remote` | `core/engine-api/src/lib.rs:2972-3009` | `core/engine-api/src/lib.rs:3033-3069` | `pub struct CollabSession {` |
| L45 `insert_text_at_caret` | `core/engine-api/src/lib.rs:2219-2234` | `core/engine-api/src/lib.rs:2279-2293` | `pub fn insert_text_at_caret(&mut self, para: usize, cp: usize, text: &str) -> Vec<String> {` |
| L45(2) `next` | `core/engine-api/src/lib.rs:2233` | `core/engine-api/src/lib.rs:2293` | `self.replace_paragraph_text(para, &next)` |
| L46 `replace_paragraph_text` | `core/engine-api/src/lib.rs:2208-2252` | `core/engine-api/src/lib.rs:2799-2843` | `pub fn replace_paragraph_text(&mut self, para: usize, text: &str) -> Vec<String> {` |
| L47 `text` | `core/engine-api/src/lib.rs:2602` | `core/engine-api/src/lib.rs:2800` | `let evs = self.layout.replace_paragraph_text(para, text);` |
| L48 `new` | `core/engine-api/src/lib.rs:2764` | `core/engine-api/src/lib.rs:2824` | `r.text = String::new();` |
| L49 `recompute_notes` | `core/engine-api/src/lib.rs:961` | `core/engine-api/src/lib.rs:964` | `self.recompute_notes();` |
| L50 `edit_syncs_paras_eff_styles_and_round_trip` | `core/engine-api/src/lib.rs:8920-8946` | `core/engine-api/src/lib.rs:9003-9029` | `fn edit_syncs_paras_eff_styles_and_round_trip() {` |
| L50(2) `replace_paragraph_text` | `core/engine-api/src/lib.rs:2739-2740` | `core/engine-api/src/lib.rs:9027-9028` | `let saved = engine.save_docx().expect("存檔");` |
| L53 `str` | `core/engine-api/src/lib.rs:2709-2712` | `core/engine-api/src/lib.rs:2914-2917` | `pub fn set_part(&mut self, name: &str, bytes: Vec<u8>) {` |
| L98 `parts` | `core/engine-api/src/lib.rs:3359` | `core/engine-api/src/lib.rs:351` | `let style_table = if styles_referenced(&doc.parts) {` |
| L108 `cascade_paragraphs` | `core/engine-api/src/lib.rs:3217-3509` | `core/engine-api/src/lib.rs:3560-3826` | `fn cascade_paragraphs(` |
| L123 `commit_local` | `core/engine-api/src/lib.rs:2989-2998` | `core/engine-api/src/lib.rs:3049-3058` | `pub fn commit_local(&mut self, para: usize, run: usize, text: &str) -> String {` |
| L123(2) `text` | `core/engine-api/src/lib.rs:2192` | `core/engine-api/src/lib.rs:2799` | `pub fn replace_paragraph_text(&mut self, para: usize, text: &str) -> Vec<String> {` |
| L128 `replace_paragraph_text` | `core/engine-api/src/lib.rs:2219-2234` | `core/engine-api/src/lib.rs:2279-2293` | `pub fn insert_text_at_caret(&mut self, para: usize, cp: usize, text: &str) -> Vec<String> {` |
| L128(2) `FILE 降級` | `core/engine-api/src/lib.rs:2208-2252` | `core/engine-api/src/lib.rs:2799-2843` | `pub fn replace_paragraph_text(&mut self, para: usize, text: &str) -> Vec<String> {` |
| L133 `cascade_paragraphs` | `core/engine-api/src/lib.rs:3217-3509` | `core/engine-api/src/lib.rs:3560-3826` | `fn cascade_paragraphs(` |
| L250 `para_start` | `core/ooxml/src/table.rs:459` | `core/ooxml/src/table.rs:460` | `pub para_start: usize,` |
| L250(2) `para` | `core/engine-api/src/lib.rs:233` | `core/engine-api/src/lib.rs:238` | `para: usize,` |
| L250(3) `para_sec` | `core/engine-api/src/lib.rs:94` | `core/engine-api/src/lib.rs:154` | `para_sec: Vec<u32>,` |
| L251 `recompute_notes` | `core/engine-api/src/lib.rs:969-974` | `core/engine-api/src/lib.rs:976-981` | `fn recompute_notes(&mut self) {` |
| L294 `cascade_paragraphs` | `core/engine-api/src/lib.rs:529-541` | `core/engine-api/src/lib.rs:542-554` | `let (eff_all, metrics, spacings, marker_chars) = cascade_paragraphs(` |
| L364 `page_display_list` | `core/engine-api/src/lib.rs:8601` | `core/engine-api/src/lib.rs:8617` | `let dl = engine.page_display_list(0).unwrap();` |
| L365 `w_pt` | `core/engine-api/src/lib.rs:233` | `core/engine-api/src/lib.rs:240` | `w_pt: f32,` |
| L395 `recompute_notes` | `core/engine-api/src/lib.rs:969-974` | `core/engine-api/src/lib.rs:976-981` | `fn recompute_notes(&mut self) {` |
| L458 `roundtrip_check` | `core/engine-api/src/lib.rs:2816-2823` | `core/engine-api/src/lib.rs:2876-2883` | `pub fn roundtrip_check(&self) -> Result<ZipDiff, EngineError> {` |
| L478 `CollabSession` | `core/engine-api/src/lib.rs:2945-2987` | `core/engine-api/src/lib.rs:3033-3069` | `pub struct CollabSession {` |
| L607 `touched` | `core/engine-api/src/lib.rs:94` | `core/engine-api/src/lib.rs:109` | `touched: BTreeMap<String, ()>,` |
| L607(2) `set_part` | `core/engine-api/src/lib.rs:2854` | `core/engine-api/src/lib.rs:2914` | `pub fn set_part(&mut self, name: &str, bytes: Vec<u8>) {` |
| L608 `roundtrip_check` | `core/engine-api/src/lib.rs:2816-2823` | `core/engine-api/src/lib.rs:2876-2883` | `pub fn roundtrip_check(&self) -> Result<ZipDiff, EngineError> {` |
| L654 `set_part` | `core/engine-api/src/lib.rs:2854-2857` | `core/engine-api/src/lib.rs:2914-2917` | `pub fn set_part(&mut self, name: &str, bytes: Vec<u8>) {` |
| L692 `insert_text_at_caret` | `core/engine-api/src/lib.rs:2219-2234` | `core/engine-api/src/lib.rs:2279-2293` | `pub fn insert_text_at_caret(&mut self, para: usize, cp: usize, text: &str) -> Vec<String> {` |
| L692(2) `replace_paragraph_text` | `core/engine-api/src/lib.rs:2208-2252` | `core/engine-api/src/lib.rs:2799-2843` | `pub fn replace_paragraph_text(&mut self, para: usize, text: &str) -> Vec<String> {` |
| L692(3) `edit_syncs_paras_eff_styles_and_round_trip` | `core/engine-api/src/lib.rs:8920-8946` | `core/engine-api/src/lib.rs:9003-9029` | `fn edit_syncs_paras_eff_styles_and_round_trip() {` |
| L693 `roundtrip_check` | `core/engine-api/src/lib.rs:2816-2823` | `core/engine-api/src/lib.rs:2876-2883` | `pub fn roundtrip_check(&self) -> Result<ZipDiff, EngineError> {` |
| L693(2) `roundtrip_diff` | `core/engine-api/src/lib.rs:2871-2878` | `core/engine-api/src/lib.rs:2931-2938` | `pub fn roundtrip_diff(` |
| L697 `save_docx` | `core/engine-api/src/lib.rs:2810` | `core/engine-api/src/lib.rs:2870` | `pub fn save_docx(&self) -> Result<Vec<u8>, EngineError> {` |
| L698 `FLAG_BOLD` | `core/engine-api/src/lib.rs:8561-8565` | `core/engine-api/src/lib.rs:8633-8633` | `assert_eq!(texts[0].1, FLAG_BOLD);` |
| L700 `roundtrip_check` | `core/engine-api/src/lib.rs:2816` | `core/engine-api/src/lib.rs:2876` | `pub fn roundtrip_check(&self) -> Result<ZipDiff, EngineError> {` |
| L722 `tables` | `core/ooxml/src/table.rs:459` | `core/ooxml/src/table.rs:460` | `pub para_start: usize,` |
| L722(2) `shape_boxes` | `core/engine-api/src/lib.rs:94` | `core/engine-api/src/lib.rs:157` | `shape_boxes: Vec<(usize, quire_ooxml::InlineImage)>,` |
| L722(3) `image_pixels` | `core/engine-api/src/lib.rs:94` | `core/engine-api/src/lib.rs:164` | `image_pixels: Vec<PixelSource>,` |
| L722(4) `para_sec` | `core/engine-api/src/lib.rs:94` | `core/engine-api/src/lib.rs:154` | `para_sec: Vec<u32>,` |
| L751 `tables_json` | `core/engine-api/src/lib.rs:2137-2139` | `core/engine-api/src/lib.rs:2140-2142` | `pub fn tables_json(&self) -> String {` |
| L755 `w_pt` | `core/engine-api/src/lib.rs:233` | `core/engine-api/src/lib.rs:240` | `w_pt: f32,` |
| L783 `recompute_notes` | `core/engine-api/src/lib.rs:969-974` | `core/engine-api/src/lib.rs:976-981` | `fn recompute_notes(&mut self) {` |
| L812 `touched` | `core/engine-api/src/lib.rs:94` | `core/engine-api/src/lib.rs:109` | `touched: BTreeMap<String, ()>,` |
| L813 `set_part` | `core/engine-api/src/lib.rs:2854-2857` | `core/engine-api/src/lib.rs:2914-2917` | `pub fn set_part(&mut self, name: &str, bytes: Vec<u8>) {` |
| L814 `insert_text_at_caret` | `core/engine-api/src/lib.rs:2219-2234` | `core/engine-api/src/lib.rs:2279-2293` | `pub fn insert_text_at_caret(&mut self, para: usize, cp: usize, text: &str) -> Vec<String> {` |
| L814(2) `replace_paragraph_text` | `core/engine-api/src/lib.rs:2208-2252` | `core/engine-api/src/lib.rs:2799-2843` | `pub fn replace_paragraph_text(&mut self, para: usize, text: &str) -> Vec<String> {` |

## 殘留 STALE（稽核器誤判，未強改）

### L50 `core/engine-api/src/lib.rs:9027-9028` ident=replace_paragraph_text

文件原句：

> 單元測試 `edit_syncs_paras_eff_styles_and_round_trip`（`core/engine-api/src/lib.rs:8920-8946`）在 `core/engine-api/src/lib.rs:7947` 呼叫 `engine.replace_paragraph_text(0, "改後新文字")` 後，於 `core/engine-api/src/lib.rs:2739-2740` 直接斷言：`let saved = engine.save_docx().expect("存檔"); assert_eq!(saved.as_slice(), FX, "編輯發射投影後存檔仍須位元組級相同");`。

實讀證據（`git show HEAD:core/engine-api/src/lib.rs`）：

```
9010:        engine.replace_paragraph_text(0, "改後新文字");
9027:        let saved = engine.save_docx().expect("存檔");
9028:        assert_eq!(saved.as_slice(), FX, "編輯發射投影後存檔仍須位元組級相同");
```

判定理由：該錨點語意是「後方直接斷言」，真值即 9027-9028；v3 以錨點前最近 code span `engine.replace_paragraph_text(0, "改後新文字")` 推得 ident=replace_paragraph_text，因此要求在區間內看到該詞，屬 span 推論誤判。若改指 9010 雖可讓 v3 轉綠，卻與文件所述「直接斷言 let saved」不符，故不改。

### L722 `core/ooxml/src/table.rs:460` ident=tables [FIELD]

文件原句：

> 核心實作：扁平段落數變動（$\Delta p \neq 0$）時對 `OoxmlDoc::tables`（`core/ooxml/src/table.rs:460`）、`Engine::shape_boxes`（`core/engine-api/src/lib.rs:157`）、`Engine::image_pixels`（`core/engine-api/src/lib.rs:164`）與 `Engine::para_sec`（`core/engine-api/src/lib.rs:154`）的索引同步平移。

實讀證據（`git show HEAD:core/ooxml/src/table.rs`）：

```
456: pub struct BodyTable {
460:     pub para_start: usize,
```

實讀證據（`git show HEAD:core/ooxml/src/lib.rs`）：

```
313:     pub tables: Vec<BodyTable>,
```

判定理由：該錨點路徑指向 `core/ooxml/src/table.rs`，真值 drift 是 `BodyTable::para_start` 從註解行 459 校正到欄位宣告行 460（與 L250 同一真值）。v3 以 span `OoxmlDoc::tables` 推得 FIELD ident=tables，但 `tables` 欄位實際宣告於 `core/ooxml/src/lib.rs:313`，`table.rs` 內不存在名為 `tables` 的欄位，任何 table.rs 行號都不可能命中；此為 span 推論與錨點路徑不一致之誤判。僅改行號的任務約束下無法修正路徑，故保留 460 並列證。

## Commit 批次

```
b900aee Merge branch 'main' into oc/anchor-sync-editapi
03ea4bb 文件(錨點同步): 新增 oc/anchor-sync-editapi 交付報告（61 條真值對照＋2 條誤判列證）
aac5651 文件(錨點同步): 7 條
0eb139e 文件(錨點同步): 7 條
463b5b8 文件(錨點同步): 10 條
ab6386d 文件(錨點同步): 4 條
710986f 文件(錨點同步): 5 條
b1c479a 文件(錨點同步): 6 條
f3171f4 文件(錨點同步): 6 條
74ff660 文件(錨點同步): 7 條
5134564 文件(錨點同步): 9 條
```

註：`03ea4bb` 之後的 `b900aee` 僅整合交付時已前進的 `main`；本報告內容於整合前後相同。
