# 錨點返修報告（t_e3a8202e）

日期：2026-11-12。分支 `wt/anchor-fix`。基線 commit `3cd1424`（稽核器 v2 落庫點），HEAD 為本次返修終態。

## 1. 方法摘要

- 錨點全量解析：行內 `` `path:x-y` `` 與 markdown `](path#Lx-Ly)` 兩式（`mkrows` 系解析，213 條 stale 與稽核器逐條對上，含同行多錨點的精確 span）。
- 定位基準採「作者視界」：沿目標檔 blob 歷史（由新而舊、依 blob hash 去重）找「宣稱區間內容＋該識別符當時為真」的最晚修訂，再以该区間實質內容在 HEAD 做唯一匹配定位；等寬平移，結束行不越該宣告的 brace 結尾。
- 三波落盤（各自 commit＋重跑稽核）：`1d42963` 已修 173 條、`242665b` 已修 2 條、第三波已修 10 條。合計 185 條。基線 stale 213 → 殘留 28。
- 紀律自證（每波均跑）：`git diff` 去數字後刪行集與加行集全等（103/103、2/2、10/10）。

## 2. 關鍵符號真值對照表（10 條）

「文件原宣稱」「改後」皆為該錨點在原文件行內的實值；實讀依據為 `git show HEAD:<檔>` 該行實際內容前 40 字（只抄程式碼）。

| 符號 | 檔 | 文件原宣稱 | 改後 | 實讀依據（該行實際內容前 40 字） |
|---|---|---|---|---|
| `page_count` | `core/engine-api/src/lib.rs` | `1823-1825`（doc L22） | `2072-2074` | L2072 `pub fn page_count(&self) -> usize {` |
| `paragraph_count` | `core/engine-api/src/lib.rs` | `1833-1835`（doc L23） | `2082-2084` | L2082 `pub fn paragraph_count(&self) -> usize {` |
| `Engine::from_docx_bytes`（本體呼叫點錨點） | `core/engine-api/src/lib.rs` | `213`（doc L21） | `319` | L319 `let doc = parse(bytes)?;`（宣告於 L306 `pub fn from_docx_bytes(bytes: &[u8])`） |
| `save_docx` 同列表列（稽核 ident `roundtrip_diff`） | `core/engine-api/src/lib.rs` | `2240-2270`（doc L31） | `2819-2826` | L2819 `pub fn roundtrip_diff(`（`save_docx` 宣告另在 L2771 `pub fn save_docx(&self)`） |
| `page_display_list` | `core/engine-api/src/lib.rs` | `1909-2151`（doc L109） | `2248-2490` | L2248 `pub fn page_display_list(&self, page: us`（宣告 brace 結尾 2663，未越界） |
| `Paragraph`（跨檔 ooxml） | `core/ooxml/src/lib.rs` | `272-278`（doc L104） | `242-248` | L242 `pub struct Paragraph {` |
| `compute_markers`（跨檔 lists） | `core/engine-api/src/lists.rs` | `146-184`（doc L318） | `45-83` | L45 `pub(crate) fn compute_markers(`（宣告 brace 結尾 146） |
| `build_images`（跨檔 table_map） | `core/engine-api/src/table_map.rs` | `334-436`（doc L365） | `1250-1352` | L1250 `pub fn build_images(`（宣告 brace 結尾 1399） |
| `table::count_flat_paras`（跨檔） | `core/ooxml/src/table.rs` | `1251-1253`（doc L105） | `1345-1347` | L1345 `pub(crate) fn count_flat_paras(xml: &[u8]` |
| `page_count`（md 格式錨點，跨檔） | `core/engine-api/src/lib.rs` | `#L2069-L2071`（sdk doc L1938） | `#L2072-L2074` | L2072 `pub fn page_count(&self) -> usize {` |

## 3. 終態稽核輸出（逐字）

```
AUDIT_SUMMARY ok=386 stale=28 missfile=0 phantom=0 proposed=0 range=0 total=414
AUDIT_SUMMARY ok=255 stale=0 missfile=0 phantom=0 proposed=1 range=0 total=256
```

（第一行 `docs/edit-api-design.md`，第二行 `docs/editor-sdk-design.md`。）

**未達標照實報**：`docs/editor-sdk-design.md` 已歸零（stale=0，exit 0）；`docs/edit-api-design.md` 剩 28 條 stale（exit 1）。剩餘全部屬「錨點緊鄰代碼片段點名的 ident 在目標檔內不存在任何同名宣告（含 struct 欄位、enum 變體）」——即該 ident 是檔名／路徑／散文 token，非該檔符號，依規則四維持原樣，不換相近符號充數。

## 4. 查無此物清單（edit-api-design.md，28 條，維持原樣）

判準：對目標檔逐行掃描 `pub|const|static|fn|struct|enum|trait|type|mod|macro_rules!` 宣告、結構欄位宣告（`name: T`）、列舉變體宣告（`Name {`／`Name(`／`Name,`），ident 逐字同名者為零。

| doc 行 | 錨點 | ident | 理由 |
|---|---|---|---|
| 28 | `core/engine-api/src/lib.rs:2155-2166` | `engine` | 該檔無名 `engine` 之宣告（多為區域變數名，非宣告目標） |
| 30 | `core/engine-api/src/lib.rs:2169-2213` | `engine` | 同上 |
| 128 | `core/engine-api/src/lib.rs:2169-2213` | `engine` | 同上 |
| 133 | `core/engine-api/src/lib.rs:619-620` | `engine` | 同上 |
| 133 | `core/engine-api/src/lists.rs:146-184` | `engine` | lists.rs 內無 `engine` 宣告 |
| 318 | `core/engine-api/src/lib.rs:614-622` | `engine` | 同上 |
| 75 | `core/ooxml/src/zip_struct.rs:653-719` | `zip_struct` | 檔名當 ident；檔內無同名宣告 |
| 78 | `core/ooxml/src/zip_struct.rs:767-814` | `zip_struct` | 同上 |
| 79 | `core/ooxml/src/zip_struct.rs:487-498` | `zip_struct` | 同上 |
| 250 | `core/ooxml/src/table.rs:459-459` | `tables` | 檔內無 `tables` 欄位／宣告 |
| 722 | `core/ooxml/src/table.rs:459-459` | `tables` | 同上 |
| 313 | `core/ooxml/src/numbering.rs:547-547` | `numbering` | 檔名當 ident；檔內無同名宣告 |
| 357 | `core/ooxml/src/drawing.rs:34-38` | `drawing` | 同上 |
| 385 | `core/ooxml/src/sectpr.rs:51-51` | `sectpr` | 同上 |
| 387 | `core/ooxml/src/sectpr.rs:54-54` | `sectpr` | 同上 |
| 389 | `core/ooxml/src/sectpr.rs:624-624` | `sectpr` | 同上 |
| 392 | `core/ooxml/src/sectpr.rs:160-163` | `notes` | 檔內無 `notes` 宣告（`use crate::notes::NotePr` 為 import） |
| 693 | `shells/native/src/main.rs:201-397` | `xml` | ident 出自指令列散文（CLI 參數 `--allow word/document.xml`），非符號 |
| 703 | `core/ooxml/src/diff.rs:110-110` | `diff` | 檔內無名 `diff` 之宣告（`diff_zip_structure` 為近似符號，規則四禁止充數） |
| 732 | `core/ooxml/src/styles.rs:1012-1020` | `MutationError` | styles.rs 內無 `MutationError` 宣告（列舉定義在他檔） |
| 713 | `core/engine-api/tests/k21_soft_br.rs:33-33` | `corpus` | ident 為路徑前綴（corpus 檔名），非宣告 |
| 750 | `core/engine-api/tests/r2_tbl_header.rs:27-27` | `corpus` | 同上 |
| 764 | `core/engine-api/tests/r2_tbl_header.rs:27-27` | `corpus` | 同上 |
| 781 | `core/engine-api/tests/k7b_header_push.rs:31-31` | `corpus` | 同上 |
| 786 | `core/engine-api/tests/r7_footnote_anchor.rs:30-30` | `corpus` | 同上 |
| 792 | `core/engine-api/tests/k7b_header_push.rs:31-31` | `corpus` | 同上 |
| 793 | `core/engine-api/tests/r7_footnote_anchor.rs:30-30` | `corpus` | 同上 |
| 791 | `core/engine-api/tests/k9_hf_align.rs:16-16` | `engine` | 該測試檔內無 `engine` 宣告 |

備註：此 28 條的共性是**稽核器 ident 取自錨點前緊鄰代碼片段的最後 token**，而文件該處緊鄰片段是路徑字串或 CLI 指令散文，末 token 恰為檔名／路徑段——這是文件寫法與稽核器 ident 規則的錯配，不是行號漂移；改行號無法使其誠實通過，故照實掛賬。

## 5. 區間寬度自證

統計口徑：僅計稽核器認可之代碼錨點（路徑須為倉內真實檔案；雨檔合計 n=604 條，改前改後同數）。

- 改前（`3cd1424`）全部錨點區間寬度總和：**19485**
- 改後（HEAD）全部錨點區間寬度總和：**19160**（淨縮 325，零膨脹）

最寬三條（皆為改前既存寬錨點、本次未觸碰未擴大）：

1. `core/ooxml/src/lib.rs:569-1233`（w=665，edit-api doc L105）——`parse_document_xml_with_fonts` 所在的整段扁平解析區宣稱，原樣保留。
2. `core/engine-api/src/lib.rs:212-699`（w=488，sdk doc L132/L1761/L1937 多次引用）——`Engine` 建構區塊宣稱，原樣保留。
3. `core/engine-api/src/lib.rs:2248-2490`（w=243，edit-api doc L109，本波修）——`page_display_list` 宣告起點等寬平移；原寬 243，宣告真實 brace 結尾 2663，未越界、未擴大。

## 6. 交付盤點

- 已修並 commit：185 條（173＋2＋10），三波 commit 各自附純淨度自證。
- `docs/editor-sdk-design.md`：stale 0（達標）。
- `docs/edit-api-design.md`：stale 28（未達標，全數列於第 4 節查無此物清單，理由為 ident 非該檔符號）。
- 嚴禁事項遵循：未 push；未改任何散文／識別符／程式碼區塊；未新增刪除整行；無擴大區間（總寬度下降）。
