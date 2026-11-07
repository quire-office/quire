# PG-PAPER-1 引擎紙張覆寫紅燈記錄

## 1. 考卷與測試命令
考卷檔案：`core/engine-api/tests/pg_paper.rs`
執行命令：
```bash
cargo test -p quire-engine --test pg_paper
```

## 2. 修前真實紅燈輸出
修前程式碼尚未實作 `PaperSpec` 資料結構、`Engine::from_docx_bytes_with_paper`、`Engine::paper_override_applied` 與 `Engine::paper_json`。真實編譯輸出如下：

```text
error[E0432]: unresolved import `quire_engine::PaperSpec`
 --> core/engine-api/tests/pg_paper.rs:9:28
  |
9 | use quire_engine::{Engine, PaperSpec};
  |                            ^^^^^^^^^ no `PaperSpec` in the root

error[E0599]: no associated function or constant named `from_docx_bytes_with_paper` found for struct `Engine` in the current scope
   --> core/engine-api/tests/pg_paper.rs:55:26
    |
 55 |     let eng_a3 = Engine::from_docx_bytes_with_paper(HC031, Some(&a3_spec))
    |                          ^^^^^^^^^^^^^^^^^^^^^^^^^^ associated function or constant not found in `Engine`

error[E0599]: no method named `paper_json` found for struct `Engine` in the current scope
   --> core/engine-api/tests/pg_paper.rs:107:43
    |
107 | ...(&eng_default.paper_json()).expect("解析原始 paper_json 失敗");
    |                  ^^^^^^^^^^ method not found in `Engine`

error[E0599]: no method named `paper_override_applied` found for struct `Engine` in the current scope
   --> core/engine-api/tests/pg_paper.rs:109:26
    |
109 |     assert!(!eng_default.paper_override_applied());
    |                          ^^^^^^^^^^^^^^^^^^^^^^ method not found in `Engine`

error[E0599]: no associated function or constant named `from_docx_bytes_with_paper` found for struct `Engine` in the current scope
   --> core/engine-api/tests/pg_paper.rs:123:32
    |
123 | ...erride = Engine::from_docx_bytes_with_paper(CELL_GRID_SPAN, Some(&sp...
    |                     ^^^^^^^^^^^^^^^^^^^^^^^^^^ associated function or constant not found in `Engine`

error[E0599]: no associated function or constant named `from_docx_bytes_with_paper` found for struct `Engine` in the current scope
   --> core/engine-api/tests/pg_paper.rs:159:24
    |
159 | ...t eng2 = Engine::from_docx_bytes_with_paper(CELL_GRID_SPAN, None).ex...
    |                     ^^^^^^^^^^^^^^^^^^^^^^^^^^ associated function or constant not found in `Engine`

error[E0599]: no method named `paper_override_applied` found for struct `Engine` in the current scope
   --> core/engine-api/tests/pg_paper.rs:161:19
    |
161 |     assert!(!eng1.paper_override_applied());
    |                   ^^^^^^^^^^^^^^^^^^^^^^ method not found in `Engine`

error[E0599]: no method named `paper_json` found for struct `Engine` in the current scope
   --> core/engine-api/tests/pg_paper.rs:163:21
    |
163 |     assert_eq!(eng1.paper_json(), eng2.paper_json());
    |                     ^^^^^^^^^^ method not found in `Engine`

error[E0599]: no associated function or constant named `from_docx_bytes_with_paper` found for struct `Engine` in the current scope
   --> core/engine-api/tests/pg_paper.rs:187:27
    |
187 | ...ut eng = Engine::from_docx_bytes_with_paper(CELL_GRID_SPAN, Some(&sp...
    |                     ^^^^^^^^^^^^^^^^^^^^^^^^^^ associated function or constant not found in `Engine`

error[E0599]: no associated function or constant named `from_docx_bytes_with_paper` found for struct `Engine` in the current scope
   --> core/engine-api/tests/pg_paper.rs:245:23
    |
245 |     let eng = Engine::from_docx_bytes_with_paper(TDF124637, Some(&spec))
    |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^ associated function or constant not found in `Engine`

Some errors have detailed explanations: E0432, E0599.
error: could not compile `quire-engine` (test "pg_paper") due to 10 previous errors
```

## 3. 紅燈原因與鑑別力分析
1. **燈 1 至燈 5 介面未開闢**：修前引擎缺乏構造期紙張覆寫入口 `from_docx_bytes_with_paper`，無法將自訂紙張尺寸（如 A3）注入 sections 建立流程，亦無法獲取生效之幾何資訊與覆寫旗標。
2. **鑑別力驗證**：
   - 燈 1 鎖定 A3 覆寫後尺寸與真重排斷點相異；
   - 燈 2 鎖定邊距 Option 覆寫與原邊距保留語意；
   - 燈 3 鎖定不帶覆寫時與既有引擎行為 100% byte-identical；
   - 燈 4 鎖定段落編輯後覆寫紙張持久化（快取不回滾）；
   - 燈 5 鎖定多 section 文件的全域覆寫語意。
所有項目均以紅燈坐實，等待綠燈實作解除。
