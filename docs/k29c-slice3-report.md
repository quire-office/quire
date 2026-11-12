# K29c（Slice 3）收案報告：編輯 API 表格結構（M4）＋圖片媒體（M5）

執行者：opencode deepseek-v4.1-flash-free（外部執行者，基線 `c242ec6`）。
收尾：PM 值守接手——執行者於考卷全綠、八項閘門實跑通過後，因 opencode 免費額度耗盡
（`~/.local/share/opencode-ds3/opencode/log/opencode.log` 實據
`stream error … Free usage limit reached` @2026-11-12T02:57:40Z）靜默掛起三小時零進展，
PM 判死進程後接手覆核與署名提交。本報告由 PM 依執行者留存之實跑產物（`/tmp/k29c_*.log`）
與 PM 獨立重跑彙整。

## 交付物

- `core/engine-api/src/mutation.rs`：新增 M4/M5 資料型錄變體與公開型別
  （`TablePath`／`TablePropsPatch`／`RowPropsPatch`／`CellPropsPatch`／`VMergeState`／
  `InsertSide`／`FloatingAxisSpec`／`WrapModeSpec` 等），並在 `apply_mutation` 入口以
  `is_k29c_mutation` 分流至獨立實作檔（Slice 1/2 主幹零刪改，diff 為純新增）。
- `core/engine-api/src/mutation_k29c.rs`（新檔）：16 個 Mutation 的實作與四套段落索引
  （`para_sec`／`shape_boxes`／`image_pixels`／`tables` 段落索引）同步平移紀律。
- `core/engine-api/tests/k29c_table_media.rs`（新檔）：三盞紅燈考卷共 12 test。
- `scripts/k29c_buggy_replay.sh`（新檔）：buggy／good 兩態回塞鑑別，字串就地置換、
  不碰 git 歷史。

## 範圍實裝清單（M4＋M5，共 16 變體）

M4.1 `InsertTable`、M4.2 `DeleteTable`、M4.3 `InsertTableRow`／`DeleteTableRow`、
M4.4 `InsertTableColumn`／`DeleteTableColumn`、M4.5 `MergeTableCells`／`SplitTableCell`、
M4.6 `SetTableProps`／`SetRowProps`／`SetCellProps`、
M5.1 `InsertInlineImage`／`InsertFloatingImage`、M5.2 `ResizeOrMoveImage`、
M5.3 `ReplaceImageMedia`／`DeleteImage`。

## 先紅後綠實跑（執行者留存產物）

- 考卷拍紅紀錄：`test result: FAILED. 9 passed; 2 failed`（首輪部分實作態）與
  `test result: FAILED. 3 passed; 8 failed`／`4 passed; 8 failed`（BUGGY 注入態，
  `/tmp/k29c_buggy.log`、`/tmp/k29c_buggy_final.log`），八條必紅斷言含：
  插入表後行內圖宿主索引平移、列欄增刪段落索引同步、合併格 gridSpan/vMerge、
  跨頁表 w:tblHeader 重排後逐片重發、縮放免重開顯示列表＋配對鍵同步、
  media/rels 壓縮流零觸碰、相同位元組媒體去重複用 rId、強制 GC 不可逆。
- GREEN 態：`test result: ok. 12 passed; 0 failed`（`/tmp/k29c_good_final.log`）。
- BUGGY 回塞鑑別：`k29c_buggy_replay.sh buggy` → `4 passed; 8 failed`；
  `k29c_buggy_replay.sh good` → `12 passed; 0 failed`（兩態 exit code 均如預期）。

## 閘門

執行者實跑（基線 `c242ec6` 態，留存輸出）：clippy `-D warnings` exit 0、
`cargo test --workspace` 112 套全 ok 零 FAILED、roundtrip corpus --check
「399 檔 byte-identical 399，失敗 0」、goldens 三 jsonl 相符、分層檢查通過。
**執行者未跑完 wasm32 閘門即斃**（log 尾部無 wasm32 完成證據）。

PM 接手重跑（`~/.hermes/scripts/ow-gate.sh` 全八閘，基線態與 merge 到 main tip
`3fdba3a` 的合併態各一輪）：**GATE_SUMMARY fail=0**，含執行者漏跑的
wasm32-unknown-unknown 七 crate。考卷 `k29c_table_media` 12 test 於合併態另行實跑。

## 與設計書（docs/edit-api-design.md §6.4）的偏差

- 實作走獨立檔 `mutation_k29c.rs`＋入口 `is_k29c_mutation` 分流，設計書原案為同檔擴充——
  為避免 Slice 1/2 單段落外科手術主幹與表格/圖片家族糾纏，PM 认可此分檔。
- `SetTableProps`／`SetRowProps`／`SetCellProps` 以 Patch 結構體承載（非逐欄參數），
  與 `ParaPropsPatch` 慣例對齊。

## 不可逆邊界（誠實聲明）

- `ReplaceImageMedia` CoW 路徑可逆：原圖 `data_raw` 壓縮流位元組級還原，undo 後存檔零失真。
- `DeleteImage`／`ReplaceImageMedia` 攜 `gc_unreferenced_media: true` 對本會話新增之
  超預算媒體強制釋放時，回傳 `ReversibilityLevel::Irreversible { reason:
  MediaPayloadEvicted }` 並截斷對應 Undo 歷史（`mutation_k29c.rs`  irreversible 分支），
  屬設計書 §5.3.2 認可的不可逆降級。
- 其餘 14 變體為完全可逆（_inverse_ 走同一 XmlBytePatch／PartLifecycleInverse 紀律）。
