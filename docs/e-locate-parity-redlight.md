# E 返修卡 t_c1dbc6d7：游標反查與 mutation 側口徑失配——紅燈報告

任務：t_c1dbc6d7（分支 `wt/e-locate-parity`）。來源 U4 合併後 agy（gemini-4-hydrogen-high）
獨立審查 t_f91e5773（合併點 db865bf）＋ PM 讀碼＋corpus 實測覆核。先紅後綠，每條紅燈
輸出逐字存證於本檔 §4（**由 merge-base `37ffbbb` 乾淨 worktree 編出的 base binary 實跑**，
非同支 binary 切換）。繁中註解、署名 ADT109119、未 push（照紀律等顏群學口令）。

## §0 結論速覽

| 項目 | 定性 | 狀態 |
| --- | --- | --- |
| S2-1 `w:sdt`／`w:ins`／`w:del` 透明包裝內巢狀表 → 反查路徑界內錯位、靜默改錯表 | 真 S2，corpus 13/400 命中 | 落地：M4 入口身分驗證（卡面定案 (a)） |
| S2-2 `cell_grid_span` 對整段 `<w:tcPr>` 找第一個 `w:val` | S2 定性，corpus 0/400（機率證據） | 落地：先定位 `<w:gridSpan` 標籤再取該標籤 `@w:val`，改吃 u32 |
| S3-1 反查面每下鑽一層 `steps.to_vec()`＋前置根表重算 `table_para_end` | 效能 | 落地：呼叫端持有 `scratch` 暫存 stack＋根表 `para_start` 單調性提前中斷 |
| S3-2 `col_cursor += span` 未 `saturating_add` | wasm32 debug panic＝瀏覽器 abort，零容忍 | 落地：`table_map.rs`／`mutation_k29c.rs` 全部 `col + span` 類加法改 `saturating_add` |
| S3-3 `TableHit::cell: Option<usize>` 配 `hit.cell?` 靜默誤判表外段 | 契約 | 落地：改 `cell: usize`（現行恆 Some），註解同步 |
| S3-4 `cell_path` 的 `row` 與 `cell_row` 恆等、docstring 漏列 | 文件 | 落地：欄位保留（刪屬 breaking）＋文件說明兩者語義分層 |
| S3-5 三層考卷零案例真正測到 `cell != grid_col_start`；巢狀表與 `SetCellProps` 從未直達 M4 | 覆蓋缺口（S2-2 漏網的真因） | 落地：`table_multi` T2 擴列補真樣本＋燈 4 巢狀 decoy 直通 M4＋`cell_path` 餵 `SetCellProps` |
| 無損紅線 | `roundtrip corpus --check` | PASS（見 §5） |
| 鑑別力 | BUGGY 期望門控＋雙 binary 紅燈存證 | 兩軌皆有（§4／§6） |

本卡**未動** `core/ooxml/src/table.rs`（parser 透明包裝規則）、**未動** `shells/web/editor/`、
**未動** `corpus/`。`shells/web/tests/e_wasm_table_map.rs` 燈 3 的期望常數隨 fixture 擴列同步
更正（見 §3，該檔屬考卷非編輯器）。

## §1 S2-1：透明包裝界內錯位——防線設計與殘差

### 根因（兩套口徑）
- canonical 側：`core/ooxml/src/table.rs` 的 `tbl_parent()` 與 `tr`／`tc` 的 `under()` 白名單
  **穿透** `Frame::Skip`／Opaque 透明包裝，把包裝節點**內**的表掛進 canonical 樹並計入
  `root_slot`／`row`／`cell`／`sub_table_idx`。
- raw 側：`core/engine-api/src/mutation_k29c.rs::locate_table` 走 `direct_children(text, …, "tbl"/"tr"/"tc")`
  ——**不穿透**任何包裝。
- 只要被包裝節點之後還有 ≥1 個未包裝的直接兄弟節點，反查回傳的座標就**錯位且仍落在界內**。
  `locate_table` 舊態只有 `Vec::get(idx)` 越界檢查，**零身分驗證** ⇒ 界內錯位不報錯，直接
  靜默改錯／刪錯下一張表、列或格（卡面點名的「M4 端有誠實拒絕兜底」不成立，本卡實測證實）。

### 修法（卡面定案 (a)：反查路徑附帶可比對識別子）
`table_identity_error()`（`mutation_k29c.rs`）在每次 M4 落地前執行：

1. 以 `path` 在 canonical 樹定位目標表（找不到＝路徑與文件不符）。
2. 識別子＝該表子樹**首段（文件序最小段）的 canonical 索引**，經
   `mutation::find_paragraph_span`（**與主解析器同口徑**的既有原語）換算 raw 位元組區間。
3. 該區間必須**完整落在** raw 解析出的 `(start, end)` 內；不符 ⇒
   `InvalidTableOperation{detail:"路徑與文件不符（疑似 sdt/ins 透明包裝錯位）"}`，**絕不靜默執行**。
4. 目標表子樹**零段落**（無識別子可用）時退回**逐層張數配對**：raw 直屬 `w:tbl` 數必與該層
   canonical 表數相等。

收口點＝`Engine::locate_table_verified()`，全部 **10** 處 M4 `locate_table(text,&table)?` 呼叫點
改吃它（grep 坐實 10 處）。新增 `classify_locate_error()`：解析**失敗**時若 canonical／raw 根表
張數已分岔，改發點名透明包裝的拒絕，不再留「根表序 1 越界（共 1 張）」這種誘導外殼重選表的
假線索。

`TableHit` 增 `anchor_para`（＝命中段落 canonical 索引），`Engine::table_locator_json` 輸出成
JSON 的 `anchor_para` 欄，外殼可自行早早發現過期路徑。

### 誠實記帳的代價與殘差
- 本防線只**拒絕錯位**、**不修正**錯位：透明包裝檔內 raw 序與 canonical 序不一致的表，得到的是
  誠實拒絕而非「改對」。要「改對」需 raw 側改用與 parser 同一支穿透規則（卡面選項 (b)，動
  parser 熱檔，PM 未核准）。
- 逐層張數配對是「等式檢查」而非「一一對應檢查」：理論上存在「一處多算＋一處少算」互相抵銷的
  病態檔，此時配對通過而身分仍錯。corpus 未見此類檔，且識別子可用時走第 3 步強驗證、不受影響。
- 成本：每次 M4 呼叫多一次 `find_paragraph_span`（O(文件大小)）；只在 Mutation 路徑，不進顯示
  列表／roundtrip 熱路徑。

### 考卷自身兩處缺陷的更正（非引擎問題，但影響鑑別力）
1. 舊 lamp3 把「未包裝的 T_B 也必須成功」寫成期望——那是對未實作行為的臆測、非卡面斷言。
   實測：本 fixture canonical 2 張／raw 直屬 1 張，連裸表也無法由 canonical `root_slot 1` 對號
   ⇒ 正解是誠實拒絕＋全檔零變化。改寫為「零波及鎖」。防線「不得誤擋正常檔」的正向保證改由
   燈 4（未包裝多表檔逐張可改）承擔。
2. 舊 lamp6 拿**空集合**當零半寫基準，語料檔本來就可能自帶 center 表 ⇒ 把「檔案原樣」誤判成半寫。
   基準改為同 bytes 重開的 pristine 引擎。

## §2 S2-2：`cell_grid_span` 精確讀 `w:gridSpan/@w:val`

舊實作對**整段 `<w:tcPr>`** 呼叫 `attr_in_tag(raw, "w:val")`＝「本 tcPr 內第一個 `w:val`」。
ECMA-376 §17.4.103 的 `w:tcPr` 允許 `w:tcBorders`／`w:shd`／`w:vMerge`／`w:cnfStyle` 等自帶
`w:val` 的子女先於 `w:gridSpan` 宣告：

- `<w:shd w:val="clear">` ⇒ 拿到列舉字串，`parse()` 失敗 ⇒ 跨欄數錯算 **1**；
- `<w:vMerge w:val="3">`（畸形數字 val）⇒ 直接把合併序号當成跨欄數（錯算 **3**）。

錯算讓 `row_layout` 的 `col_start`／`span` 與反查面（`table_map.rs` 走 canonical
`CellProps::grid_span()`）分岔，餵 `InsertTableColumn`／`DeleteTableColumn`／`MergeTableCells`
就操作錯欄。

修法：先 `find_local_tag(raw, "gridSpan")` 定位該標籤（local name 精確比對，不認子字串），再在
**該開標籤**內搜 `w:val`，搜不到該標籤才回 1。**改吃 u32** 逐字對齊 canonical
（`core/ooxml/src/table.rs::CellProps::grid_span`：u32 解析、缺／非法／0＝1、不 panic）——用
u32 是為了讓 `w:gridSpan w:val="18446744073709551615"` 這類「usize 解得開、u32 解不開」的畸形值
兩側同判 1，否則又是新的同義兩制。

**鑑別手法**（不靠「不 panic 就綠」）：把污染格的 `grid_col_start` 當插入點發 `InsertTableColumn`。
口徑一致 ⇒ 插入點落在該格**內部** ⇒ 該格 `gridSpan` 2→3、該列**格數不變（2）**；口徑分岔 ⇒
該格被錯算成 span=1、插入點變成下一格起點 ⇒ **新增一格**（列格數 3、原格 gridSpan 停 2），或
基準錯致 `gridSpan` 跳到 **4**。三個數（tblGrid 欄數、列格數、該格 gridSpan）都是 `tables_json`
直接可讀的 canonical 事實＝使用者看得見的結果。

corpus 400 檔實測「`w:gridSpan` 之前出現其他 `w:val`」＝**0 命中** ⇒ 實際視覺人群暫零，屬機率證據；
規格正確性仍屬 S2，故以自製合成檔鎖住。

## §3 fixture 與產生器（全部純 stdlib 合成、零內嵌圖片、零個資，PII 掃描乾淨）

| fixture | 產生器 | 內容 |
| --- | --- | --- |
| `fixtures/sdt_wrapped_table.docx` | `scripts/gen_sdt_wrapped_table_fixture.py` | body 依序＝`w:sdtContent` 包一張表 T_A、接著裸表 T_B、表後尾段（確保 sdt 之後有未包裝兄弟） |
| `fixtures/table_gridspan_prefix.docx` | `scripts/gen_gridspan_prefix_fixture.py` | 3 網格欄；r0c0 前綴 `<w:shd w:val="clear"/>`＋`<w:gridSpan w:val="2"/>`；r1c0 前綴 `<w:vMerge w:val="3"/>`＋`<w:gridSpan w:val="2"/>` |
| `fixtures/table_nested_decoy.docx` | `scripts/gen_nested_decoy_fixture.py` | 宿主格內先 decoy 子表（`sub_table_idx=0`）再目標子表（`=1`）——驗 `sub_table_idx`≠0 直通 M4 |
| `fixtures/table_gridspan_overflow.docx` | 同上（同檔附第二表） | 畸形 `w:gridSpan`（`4294967295`／`18446744073709551615`）——wasm32 debug `attempt to add with overflow` 即瀏覽器 abort，零 panic 是硬要求（S3-2） |
| `fixtures/table_multi.docx`（擴列） | `scripts/gen_multi_table_fixture.py`（同步改，可重現） | T2 由「1 列 2 欄」擴為 3 網格欄：r0 三格（對照組）＋r1 為 `gridSpan=2` 跨前两欄＋一格 ⇒ **r1c1 的 tc 序＝1、網格起點＝2** |

`table_multi` 的 T2 擴列是 S3-5 的正解：舊形「1 列 2 欄、r0c1 兩者皆 1」使全套 fixture
**沒有任何一格** `cell != grid_col_start` 真正成立，三層考卷（engine／engine／wasm 殼）從未測到
兩者不等——S2-2 的 gridSpan 錯讀正是從這個洞漏網。擴列後 `e_gridspan_prefix.rs` 燈 7 斷言
「該檔必須含 `cell != grid_col_start` 案例」，產生器被改回舊形即報紅。

隨動更正：`shells/web/tests/e_wasm_table_map.rs` 燈 3 對 T2（`rows[1]`）的期望常數由
`rows=1, grid_cols=2` 改為 `rows=2, grid_cols=3`（`tables_summary_json` 對 canonical 事實的直接反映，
非斷言放寬；改動前該燈在擴列 fixture 上實測紅 `left: Number(2) / right: 1`）。

## §4 修前紅燈存證（base binary＝merge-base `37ffbbb` 乾淨 worktree 編出，head binary＝本卡 worktree）

紀律依 `quire-redlight-conventions`：「既有缺口、非本卡引入」＋「其餘檔案零回退」需**兩支 binary**。
base 自 `git worktree add <scratch>/base-wt-292 37ffbbb`（獨立 `CARGO_TARGET_DIR`）編出，考卷與
fixture 由本卡原封複製（考卷在修前態可編譯、可執行＝真紅，非編譯錯）。

### S2-1（`e_sdt_wrapped_table`，base 態 4 failed／4 passed）

```
---- lamp2_located_wrapped_table_never_silently_mutates_the_wrong_table stdout ----
thread 'lamp2_...' panicked at core/engine-api/tests/e_sdt_wrapped_table.rs:318:13:
assertion `left == right` failed: 反查段 0（root_slot 0＝SDT 內表 T_A）餵 M4 後，被 centre 的根表集合必須恰為 [0]；實得 [1] ⇒ 界內錯位：反查說改 T_A、實際改到別張表（S2-1）
  left: [1]
 right: [0]

---- lamp3_no_path_in_a_wrapped_file_may_half_write stdout ----
panicked at core/engine-api/tests/e_sdt_wrapped_table.rs:404:23:
assertion `left == right` failed: root_slot 0 若成功，被改集合必須恰為 [0]（不得波及他表）
  left: [1]
 right: [0]

---- lamp6_corpus_wrapped_tables_never_silently_hit_the_wrong_table stdout ----
panicked at core/engine-api/tests/e_sdt_wrapped_table.rs:524:21:
assertion `left == right` failed: corpus/poi/test-data_document_Bug54849.docx root_slot 0 反查路徑餵 M4 後被改的根表集合必須恰為 [0]，實得 [1] ⇒ 界內錯位（S2-1 在語料檔上發作）
  left: [1]
 right: [0]

---- lamp7_buggy_gate_proves_the_attribution_assertion_is_live stdout ----
panicked at core/engine-api/tests/e_sdt_wrapped_table.rs:587:9:
綠態：歸屬斷言必須成立（被改集合 [1]，M4 結果 Ok(())）

test result: FAILED. 4 passed; 4 failed; 0 ignored; 0 measured; 0 finished in 0.30s
```

要點：`left: [1]`／`right: [0]`＝**界內錯位坐實**（反查宣稱 root_slot 0、實際改到 root_slot 1，無任何
錯誤訊息、`Ok(())` 回傳）；lamp6 證明同一缺陷在**真實語料檔**上發作，非合成檔專屬。

### S2-2＋S3-2（`e_gridspan_prefix`，base 態 4 failed／4 passed）

```
---- lamp3a_string_val_prefix_does_not_break_insert_column stdout ----
panicked at core/engine-api/tests/e_gridspan_prefix.rs:260:5:
assertion `left == right` failed: shd w:val="clear" 前綴（段 0）：在跨欄格內部插欄必須**加寬該格**而非新增一格；列格數 2→3 ⇒ mutation 側把 gridSpan 錯算（cell_grid_span 讀到前綴 w:val）
  left: 3
 right: 2

---- lamp3b_numeric_val_prefix_does_not_break_insert_column stdout ----
panicked at core/engine-api/tests/e_gridspan_prefix.rs:265:5:
assertion `left == right` failed: vMerge w:val="3" 前綴：該格 gridSpan 必須 2→3（錯算態會停在 2 或跳到 4）
  left: 4
 right: 3

---- lamp5_malformed_huge_gridspan_never_panics stdout ----
panicked at core/engine-api/src/mutation_k29c.rs:1631:63:
attempt to add with overflow

---- lamp6_buggy_gate_proves_lamp3_assertions_are_live stdout ----
panicked at core/engine-api/tests/e_gridspan_prefix.rs:408:9:
assertion `left == right` failed: 綠態：加寬斷言必須成立（實得列格數 3、gridSpan 2）
  left: (3, 2)
 right: (2, 3)

test result: FAILED. 4 passed; 4 failed; 0 ignored; 0 measured; 0 finished in 0.10s
```

要點：`shd` 態列格數 2→3（新增一格＝span 錯算 1）、`vMerge` 態 `gridSpan` 2→4（span 錯算 3）——
卡面預測的兩個錯算值逐一命中；lamp5 坐實 S3-2 在 debug 態是**真 panic**（wasm 邊＝瀏覽器 abort），
非理論風險。

### 修後綠態（head binary，本卡 worktree 實跑）

```
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s   # e_gridspan_prefix
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.46s   # e_sdt_wrapped_table
```

燈 6 語料面實測輸出（head 態，`--nocapture`）：
`燈 6 實測：7 檔／10 條反查路徑；套改 2 條、誠實拒絕 8 條`
⇒ 7 檔透明包裝語料全數落於准集（改對或誠實拒絕），零靜默錯改；其中 2 條本就不受包裝錯位影響、
照常成功＝**防線未誤擋正常檔**的語料面證據。

## §5 無損紅線與閘門

- `cargo run -q -p quire-cli -- roundtrip corpus --check`：PASS（400 檔 byte-identical／未觸碰零失真）。
- `cargo run -q -p quire-cli -- roundtrip fixtures --check`：PASS（含 4 張新 fixture）。
- `bash ~/.hermes/scripts/ow-gate.sh <worktree>`：八閘 fail=0（含 wasm32 `cargo check`、goldens、
  `check-layering.sh`、`cargo fmt --check`、`clippy -D warnings`、`cargo test --workspace`）。
- 本卡未新增探針腳本掛 CI（無 LO 真值面向：本卡是口徑一致性，非渲染視覺）。

## §6 鑑別力證明（BUGGY 期望門控，同支 binary 切換）

```
E_SDT_TABLE_BUGGY=1  → lamp7 FAILED:「BUGGY 態：期望被換成不可能值 ⇒ 此燈必須紅（實得 []）」
                       test result: FAILED. 7 passed; 1 failed
E_GRIDSPAN_BUGGY=1   → lamp6 FAILED:「BUGGY 態：期望換成不可能值 ⇒ 此燈必須紅（實得 2/3）」
                       test result: FAILED. 7 passed; 1 failed
```

兩軌皆「斷言真在比對 canonical 事實」的證據：BUGGY 態只換期望常數、引擎碼零動靜，該燈即紅。
注意 BUGGY 態的「實得」欄（`[]` 與 `2/3`）正是綠態的真值，與 §4 base 態紅燈的錯值（`[1]`、`3/2`）
互為對照。

## §7 給 PM 的下一步建議（不掛帳、不擴權）

1. **選項 (b) 若要推進**（透明包裝檔「改對」而非「誠實拒絕」）：需另卡核准動
   `core/ooxml/src/table.rs` 的 `tbl_parent()`／`under()` 穿透規則，讓 `locate_table` 與 parser
   吃同一支規則。影響面＝mutation 熱檔＋13/400 語料檔，建議以 `corpus` 全掃 A/B 為準入閘。
2. **JS 端消費端**：`table_locator_json` 新增的 `anchor_para` 欄目前是純輸出（`shells/web/editor/`
   本卡嚴禁觸碰、t_b9ae819b U4b 正在改）。U4b 收案後可另卡讓殼層在送 Mutation 前自行比對，
   省下往返；引擎側已收口，殼層漏比對不會造成靜默錯改。
3. **逐層張數配對的殘差**（§1 已記）：若要消滅「一處多算＋一處少算互相抵銷」的病態可能，
   同屬選項 (b) 範疇。
4. `table_multi.docx` 已擴 T2 列，`shells/web/tests/e_wasm_table_map.rs` 燈 3 期望已同步。後續若再
   擴該 fixture，該燈（以及 `para_cells`／`para_start`／`para_end` 斷言）是第一個要跟著改的地方。
