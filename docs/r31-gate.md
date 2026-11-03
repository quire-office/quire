# R31 閘門自驗報告

## 任務卡：fdo76591 發射重複＋VML logo 帶錯位

### 1. `cargo fmt --all --check`
```
$ cargo fmt --all --check
（無輸出，通過）
```

### 2. `cargo clippy --workspace --all-targets -- -D warnings`
```
$ cargo clippy --workspace --all-targets -- -D warnings
    Checking quire-engine v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-r31/core/engine-api)
    Checking quire-cli v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-r31/shells/native)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s
（零警告，通過）
```

### 3. `cargo test --workspace`
```
$ cargo test --workspace
...
running 3 tests
test r31_lamp2_logo_band_y_and_images_no_overlap ... ok
test r31_lamp1_no_duplicate_emissions ... ok
test r31_lamp3_shift_schedule_no_overlap_with_top_block ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s

running 1 test
test k25b_pinned_group_children_stack_like_lo ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s

... 全倉庫單元測試與整合測試全數通過，0 failed。
```

### 4. `bash scripts/check-layering.sh`
```
$ bash scripts/check-layering.sh
✓ 分層檢查通過（core↛shells；shells→engine 唯一入口；CRDT 僅在協作分塊；xlsx 不進首包）
```

### 5. `cargo run -q -p quire-cli -- roundtrip corpus --check`
```
$ cargo run -q -p quire-cli -- roundtrip corpus --check
roundtrip --check: 399 檔（另有 1 檔隔離不計紅），byte-identical 399，失敗 0
```

### 6. `cargo check --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine`
```
$ cargo check --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine
    Checking quire-ooxml v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-r31/core/ooxml)
    Checking quire-engine v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-r31/core/engine-api)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.64s
```

### 7. `PATH="/mnt/c/Program Files/LibreOffice/program:$PATH" /usr/bin/python3 scripts/render_lab.py corpus/ooxmlexport/fdo76591.docx --out /tmp/r31-lab2`
```
$ PATH="/mnt/c/Program Files/LibreOffice/program:$PATH" /usr/bin/python3 scripts/render_lab.py corpus/ooxmlexport/fdo76591.docx --out /tmp/r31-lab2
→ corpus/ooxmlexport/fdo76591.docx
   LO 1 頁 / OW 1 頁，內容相似度 [1.0]（版面 [0.987]）
   色 [0.799] 圖 [0.667] 真缺字 0（斷行切片已排除 0）：[]

報告 → /tmp/r31-lab2/report.md
```
- 版面相似度：0.395 → 0.987（遠超目標值 ≥ 0.70）
- 內容相似度：1.0
- 真缺字：0
