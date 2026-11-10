# ODF-D1 紅燈紀錄（ODT 顯示列表發射層）

考卷：`core/odf/tests/odf_d1_display_list.rs`（四盞燈＋一條附加鎖）
實裝：`core/odf/src/display.rs`（＋`shells/native` 的 `odt-dump` 子命令）
契約解剖：`docs/odf-d1-anatomy.md`
互鎖：`fixtures/display_list_contract.json`（`scripts/odf_d1_contract_probe.py` 產出）
　　　`shells/native/tests/odf_d1_dump_contract.rs`（DOCX 側反查三盞）

## 一、紅燈方式：發射器佔位實作回塞（BUGGY 鑑別，比照 ODF-E1 紀律）

本卡屬「新增一層」而非「修既有錯值」——發射層從零建立，不存在「先寫錯再改對」的
自然紅燈。故紅燈用**回塞鑑別**拍：`emit_display_lists` 內建 `ODF_D1_BUGGY` 環境
變數分支（退化為佔位實作＝零頁零項，見 `display.rs` 檔頭註），設變數跑考卷＝
考卷必須拍紅；不設＝綠。這證明四盞燈真的在檢查東西，不是空轉斷言。

## 二、紅燈實測輸出（修復前口徑，`ODF_D1_BUGGY=1 cargo test -p quire-odf --test odf_d1_display_list`）

```
running 5 tests
test lamp1_target_file_emits_text_items_with_docx_field_names ... FAILED
test view_and_direct_emitter_agree_and_json_round_trips ... FAILED
test lamp3_page_membership_matches_pagination_boundaries ... FAILED
test lamp2_text_item_origins_inside_page_rect ... FAILED
test lamp4_emitter_is_pure_read_and_keeps_projection_totals ... ok

lamp1 panicked at core/odf/tests/odf_d1_display_list.rs:202:5:
目標檔至少要有 1 頁顯示列表

lamp2 panicked at core/odf/tests/odf_d1_display_list.rs:310:5:
燈 2 實測文字項數過少（0），斷言可能空轉

lamp3 panicked at core/odf/tests/odf_d1_display_list.rs:333:5:
assertion `left == right` failed: 發射頁數必須等於邊界表頁數
  left: 0
 right: 1

附加鎖 view_and_direct... panicked at :474:5:
assertion failed: view.page_count() >= 1

test result: FAILED. 1 passed; 4 failed; 0 ignored; 0 measured
```

鑑別正確性判讀（這是**期望**的紅燈形態，逐盞對帳）：
- 燈 1／燈 2／燈 3／附加鎖：全紅——它們斷言的是「有頁、有項、幾何與分頁器同格」，
  發射器空轉時必然紅（燈 1 甚至連基準檔對照都進不去，先死在存在性斷言）。
- 燈 4：**仍綠**——正確。燈 4 斷言的是「發射層是純讀取、不改投影計數基準」；
  佔位實作同樣不改任何欄位，純度斷言本該綠。這正是四盞燈分工的證明：若燈 4
  在空轉發射器下也紅，代表它混入了存在性斷言、失去零回退守門的獨立價值。
- 另一側防假綠（燈 4 抓「亂改」的那一側）另有反證：把發射器改成會碰 `projection`
  的寫入版會在燈 4 直接紅（燈 4 逐檔比較發射前後的
  `(paragraph_count, total_text_len, text_p_count, text_h_count, sizes_pt)` 五元組）。

## 三、開發期發現並修掉的真缺陷（非回塞，實測驅動）

1. **續排行 pen-x 未重置**：首版實作把同段跨行／跨頁的續排項接在上行行尾之後
   （`pen_x` 只按段落重置），燈 3 的「行首 x 貼版心左緣」斷言直接咬紅。修法＝
   **每行重置 pen-x**（發射以行為單位，與 DOCX 線 `emit_line_run_items` 同口徑）。
   紅燈實據：`{rel} p{page} 行 {i}：行首 x 未貼版心左緣`（開發期實抓，非推測）。
2. **基準檔與出貨產物脫節**：首版互鎖考卷拿單一 `sample.docx` 對基準檔全等，
   但 `family` 欄位只在文件宣告字型族時才發（serde `skip_serializing_if`），單檔
   交集口徑與探針腳本（全樣本聯集）脫節 → 燈 a 紅。修法＝互鎖考卷改用與腳本
   **同式**的口徑（`source_samples` 全檔聯集＋交集），並補 `files == srcs.len()`
   防「某檔整型變體退化」的偽綠。
3. **燈 4 檔數口徑寫錯**：首版斷言「48−3 mimetype 違例＝45」類推錯誤（實測
   可投影 48、加密 2、違例 3 仍可投影）。以探針實測歸類後改正為
   48 可投影＋2 加密＋3 違例（違例檔仍可投影，C3 既有事實）。

## 四、綠燈實測輸出（修復後）

`cargo test -p quire-odf`（含既有考卷零回退）：

```
odf_container.rs            6 passed; 0 failed
odf_d1_display_list.rs      5 passed; 0 failed   ← 本卡（4 燈＋附加鎖）
odf_e2a_generalref.rs       5 passed; 0 failed
odf_encrypted_gate.rs       1 passed; 0 failed   ← E1 六盞燈（單函數 suite）
odt_layout_projection.rs    7 passed; 0 failed   ← C3 七盞
（C4 page_geometry 9 盞＋lib 單元 2 盞＋doc 測試等同跑，全綠；總計 35 passed; 0 failed）
```

`cargo test -p quire-cli --test odf_d1_dump_contract`：`3 passed; 0 failed`
（燈 a 欄位聯集/交集全等、燈 b 四變體全等、燈 c `page`↔`dump` 同形）。

`cargo test -p quire-cli --test odt_render_encrypted`（E1 CLI 面對照組）與
`shells/native/tests/odt_render_page_geometry.rs`（C4 CLI 面）照舊綠——新子命令
`odt-dump` 未動 `odt-render` 一行程式碼（只共用 `page_geometry` 出口）。

## 五、殘帳（寫死在此，不偷偷擴張到 D1b 之外）

- 缺件採集：`fo:color`（字色）、`style:font-name`（族名）、斜體／底線／刪除線
  位元、段落框線 `fo:border` —— 全部 D1b。本切片發中性值（黑）／跳欄（None）／
  不發該項（框線），無一捏造。
- 發射項種類：`Rect`（分頁框／表格底圖）／`Rule`（框線）／`Image`（內嵌圖）——
  不發；頁框改由頁層 `width_pt`／`height_pt` 欄位承擔（`docs/odf-d1-anatomy.md` §4
  記帳，含不收 `Rect` 的三條理由）。
- 嵌入物件文字（E2e `object_paragraphs`）：無宿主頁內座標，本切片不消費。
- 註腳頁底預留（`footnote_reserve_pt`）：0（註腳未採集，分頁器傳 0 為老實值）。
- `Engine::from_odt_bytes`／wasm `new_odt`：**本卡未做**（取捨見解剖報告 §5：
  `OdtView` 為替代入口，wasm 走 `shells/web` 直依賴 quire-odf 或 Engine `odt`
  feature 兩條合法路線，擇一屬 ODF-D2 網站卡的決策）。
