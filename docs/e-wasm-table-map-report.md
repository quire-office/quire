# E 卡報告：wasm 綁定 table_count／tables_json＋游標反查（卡 t_f91e5773）

## 1. 交付摘要

| 契約項 | 狀態 |
| --- | --- |
| `shells/web/src/lib.rs` 綁 `table_count()`／`tables_json()` | ✅（另加 `tables_summary_json()`／`table_locator_json()`） |
| 引擎缺「char／paragraph offset → table index／cell path」查詢 → engine-api 補**純函數** | ✅ `table_map::locate_table_for_para`＋`table_summaries`（零改既有簽名） |
| wasm32 雙目標閘門 `ow-gate.sh` fail=0 | ✅（本報告 §5 實測輸出） |
| JS 端可用性單元驗證 | ✅ host 考卷八盞＋**實際 wasm 產物** Node smoke 九項全綠 |
| 嚴禁碰 `shells/web/editor/` | ✅（git diff 零命中，§6 自證） |

## 2. 新增介面（全部只讀、零業務邏輯透傳）

### 2.1 wasm 殼（`WasmEngine`，四條新方法）

- `table_count() -> number`：body 級表格張數。
- `tables_json() -> string`：canonical 表格樹 JSON（與 `Engine::tables_json` 逐字同形）。
- `tables_summary_json() -> string`：根表摘要陣列
  `{root_slot, table_count, rows, grid_cols, para_cells, para_start, para_end}`——
  列「表格清單」的輕量面，不必吃整棵樹（多表檔樹可達數 MB）。
- `table_locator_json(para) -> string | undefined`：段落 → 表格路徑反查。

### 2.2 反查回傳形制（`table_path` 欄位名與 `mutation::TablePath` 逐字對齊）

```json
{"para":5,
 "table_path":{"target":"Body","root_slot":1,"nested_steps":[[0,2,0]]},
 "cell_path":{"row":0,"cell":2,"cell_row":0,"grid_col_start":2,
              "grid_span":1,"grid_cols":3,"rows":2,"depth":1}}
```

- `JSON.parse` 後 `table_path` **原樣**可塞 `SetCellProps.table`／
  `DeleteTable.table` 等 M4 變體——Node smoke 燈 6 以真 Mutation 兌現
  （改 T1 的 `justify`，T0／T2 不受波及）。
- `nested_steps` 三元組＝（列, 格, 該格內**第幾張巢狀表**）；第 3 項口徑＝
  該格 `Block::Table` 序列下標，與 `mutation_k29c::locate_table` 的
  `direct_children(cell, "tbl")` 對齊（考卷 e2 以 decoy 子表鎖死）。
- `cell_path` 區分「tc 序」（`cell`）與「網格欄起點」（`grid_col_start`＋
  `grid_span`）——gridSpan／空 vMerge 格兩者不等（PM 補正 2 同款口徑）。
- **回 `undefined` 不是錯誤**：表外正文段與越界索引同回 None→undefined，
  外殼據此走正文編輯路徑（燈 4b／燈 6）。

### 2.3 引擎側純函數（`core/engine-api/src/table_map.rs`）

- `locate_table_for_para(tables, para) -> Option<TableHit>`：深度優先取最深
  命中（段落恰被一格擁有，最深＝唯一解）；先 `para_start` 區間粗篩再逐格掃。
- `table_summaries(tables) -> Vec<TableSummary>`。
- 零 `unsafe`、零 I/O、全程 `iter`——wasm32 可編譯由閘門 G7 守。

## 3. 考卷

### 3.1 引擎純函數考卷（`table_map.rs` tests，手刻樹四盞）

e1 根表命中＋gridSpan 起點累算／e2 巢狀最深命中＋sub_table_idx 對齊
（decoy 子表鎖序）／e3 多根表 root_slot 對號＋越界 None／e4 摘要計數。

**拍紅實錄**：e2 首跑拍紅 `left: [(0,1,2)] right: [(0,1,1)]`——初版把
`sub_table_idx` 數成 blocks 全序列（段落也算），與 `locate_table` 的「只數
tbl」口徑錯位；修正後轉綠。此即「先考卷後實作」抓到的真 bug：若無 decoy
斷言，外殼拿路徑餵 M4 會在多表多巢狀檔錯位改到別張表。

### 3.2 殼層 host 考卷（`shells/web/tests/e_wasm_table_map.rs`，八盞）

燈1 計數真值（≠探針降級序假設值）／燈2 tables_json 逐字透傳＋para_start
單調／燈3 摘要逐欄對上 canonical／燈4 逐段反查**逐格手核**（fixture 檔頭
結構表：18 段→root_slot 0/1/2、巢狀步 [0,2,0]、空 vMerge 格的 tc 序≠網格
欄）／燈4b 無表檔恆 undefined 非錯誤／燈5 反查產物直通 M4 改對那張／
燈6 越界 usize::MAX 零 panic／燈7 table_mixed 單表零回退。

### 3.3 JS 端 smoke（`shells/web/tests/e_wasm_table_map_node_smoke.mjs`）

對 `pkg/` **實際出貨產物**打九項：glue 導出名／`number` 型別（非 BigInt 洩
漏）／JSON.parse 直吃／root_slot 對號／18 段全命中＋三元組經 glue 序列化
無損／越界→undefined 零 throw／路徑直通 M4 效果斷言／單表零回退。
入庫 `npm run test:tables`。ODT 既有 smoke（`test:odt`）重跑全綠。

## 4. fixture

`fixtures/table_multi.docx`（本卡新產）：三張 body 級根表（T0 四列含
gridSpan 跨欄格＋vMerge restart／continue 對＋**空 tc**、末格掛巢狀表；
T1 一列；T2 兩列），全 18 段皆在表內。產生器
`scripts/gen_multi_table_fixture.py`（純 stdlib、STORE、固定時間戳、可重現
——gen_table_fixture.py 同款口徑）。單表檔測不到「點哪張改哪張」，故必須
新產；`table_mixed.docx` 保留作零回退對照。

## 5. 閘門實測

`bash ~/.hermes/scripts/ow-gate.sh <worktree>`：八閘門（fmt／clippy -D
warnings／test --workspace／分層／fixtures roundtrip／corpus roundtrip／
wasm32 check×6／goldens）**GATE_SUMMARY fail=0**（輸出全文見卡面 completion
metadata）。首包 gzip 1,207 KB（預算 3 MB，未逼近）。

## 6. 邊界自證與殘留

- `git diff --name-only` 命中：`core/engine-api/src/{lib.rs,table_map.rs}`、
  `shells/web/src/lib.rs`、`shells/web/package.json`（僅加 `test:tables`
  script 一行）、`shells/web/tests/`（兩檔新考卷）、`fixtures/table_multi.docx`、
  `scripts/gen_multi_table_fixture.py`、本報告。**`shells/web/editor/` 零命中**。
- 降級序移除（`engine-adapter.js` 的探針考古換真索引）**不在本卡**——照卡面
  留給另卡，避免與 U4b／U4c 互踩。本卡交付後該卡的前置已清空：
  `tableCount()` 探針可直接換 `engine.table_count()`，`_resolveTablePath`
  可換 `table_locator_json(this.caret.para)`。
- 已知殘差（誠實記帳，代碼內同步註記）：`w:sdt`／`w:ins` 透明包裝**內**的
  巢狀表，canonical 掛進宿主格 blocks 而 `locate_table`（raw XML 直查）只看
  直接子節點——此類檔案 `sub_table_idx` 兩者序可能錯位。corpus 掃描未見
  「表內格子內 sdt 包巢狀表」形態；外殼 M4 端本有誠實拒絕兜底（改錯表會
  被 `locate_table` 的座標斷言擋下報錯，不靜默錯改）。
- 本卡只覆蓋正文（`target:"Body"`）——頁首尾／文字框子樹的表格反查屬
  SubDocTarget 完整路由的帳，不在契約內。
