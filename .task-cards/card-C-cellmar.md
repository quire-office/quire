# 任務卡 C：表格垂直 cell margin 生效（tblCellMar top/bottom）

## 你的角色與紀律（違反＝任務失敗）
- 你在 git worktree `/home/jerry/projects/quire/.worktrees/oc-cellmar`（branch `feat/cellmar-vertical`）工作。先 `pwd` 確認。
- **嚴禁** `git commit` / `git push` / `git checkout` / `git reset` / `git stash`。改動留在工作區，PM 逐行 review 後自行署名提交。
- 所有程式註釋與測試文案一律**繁體中文**（禁簡體；「稽核/台/了/干/吃/于/才/回/藉/游/岩/晒/峰/群」為 opencc 白名單字）。
- 風格對齊 repo 慣例（看 `core/layout/src/table.rs` 既有函式的 doc comment 密度與口吻）。

## 專案背景
`quire` 是 Rust lossless DOCX 引擎；`core/layout/src/table.rs` 是表格版面模組（帶化模型：行高以 `line_h` 為單位量化，`quant_bands(h_pt, line_h)` 換算帶數）。
**缺陷**：`resolve_margins`（約 1029 行）把 `w:tblCellMar`／`w:tcMar` 的 top/bottom 解析成 `SolvedMargins.top/bottom`，但**預設值都是 0.0**，且消費端（約 669 行 `pad_top_bands`／`pad_bottom_bands`）只把「宣告過非零值」的邊距量化進帶高。Word 的 `Table Normal` 樣式預設 cell margin 是**四邊各 0"**——所以預設 0 是對的；真正缺陷在**宣告了非零 top/bottom 的檔案**：量化到 `line_h` 整數帶（`quant_bands` ceil）會把 2pt 撐成 1 整帶（~13.8pt），行高膨脹；且頂部 padding 目前是否真的把文字內容下推（內容起點位移）需查證——LO/Word 是**精確 pt 堆疊**，不是整帶。

## 目標
把 cell 的 top/bottom margin 從「整帶量化」改為「精確 pt 偏移」，與 Word/LO 渲染口徑一致：
1. **內容偏移**：cell 文字區塊起點 y 精確下推 `margins.top`（減掉/加回對應帶數或帶內偏移——選與現有帶化幾何最不打架的實作，必要時在 CellGeom 加 `pad_top_pt`/`pad_bottom_pt` 精確欄位）。
2. **列高貢獻**：列總高 = 帶高總和 + top + bottom 精確值（若列高目前以純帶數表示，允許把剩餘精確高度帶在 RowGeom 上）。
3. **邊框繪製**：cell 邊框矩形必須跟著新內容盒（框線不內縮、居中發射是刚落地的鐵律——見 `git log -1`，別碰 `core/engine-api/src/lib.rs` 的框線代碼）。
4. 巢狀表（`CellBlock::Table`）的 padding 同樣生效（子表起點下推）。

## 實作要點
- `resolve_margins` 的預設值**保持** top/bottom=0（Word Table Normal 預設），只改消費端精度。
- 分頁跨列切分（列高超過剩餘頁高時的斷列）如受精確高度影響，測試必須斷言「斷列後頂部邊距只在首段出現」之類與 Word 一致的口徑；拿不準的邊界**老實保留現行行為並在程式註釋標 TODO**，不虛構。
- **goldens**：`scripts/goldens/tables.jsonl` 5 例是表格幾何快照。精確化後若既有 golden 的 top/bottom 都是 0/未宣告，輸出應**逐字元不變**；若某例宣告了非零 top/bottom 且你的修復改變了其高度——那是 golden 該更新（Word 才是基準），**但你必須先在回報中列出 diff 與理由**，不得靜默改 jsonl。

## 驗收標準（PM 會逐項重跑）
1. `cargo fmt --all --check` 綠
2. `cargo clippy --workspace --all-targets -- -D warnings` 綠
3. `cargo test --workspace` 全綠
4. `cargo run -q -p quire-cli -- roundtrip corpus --check` → `399 檔 byte-identical 399 失敗 0`（你不該碰存檔路徑）
5. `cargo run -q -p quire-layout --bin goldens` 輸出：shaping/pagination 兩檔必須 ✓ 不變；tables.jsonl 若變，附 diff 與逐例理由
6. `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout` 綠

## 新增測試（≥4 個，先紅後綠）
(a) `tblCellMar top=288`（0.2"=14.4pt）→ 首列文字 block 起點 y 精確下推 14.4pt（誤差 <0.01），列高含 top+bottom 精確值而非 ceil 帶數
(b) tcMar 覆蓋 tblCellMar（宣告即贏）：table top=288、cell top=144 → 該格下推 7.2pt
(c) top=0 或未宣告 → 幾何與修復前**逐位元組相同**（防回歸；直接對照修復前渲染輸出 diff）
(d) 巢狀表：外格 top=288 → 子表起點隨之下推
(e)（選做但鼓勵）垃圾值 top=9999pt → 不 panic、不天量帶數（鉗制口徑比照 S1-4 註釋）

## 禁区
- 不碰 `core/ooxml/`（解析層——top/bottom 已解析出来了，你只消費）。
- 不碰 `core/engine-api/src/lib.rs` 的框線發射代碼（19019ea 剛落地，動它=衝突）。
- 不碰正文分頁器（`core/layout/src/lib.rs` 的 `paginate`/行測量——另一任務在改）。
- 不碰 `scripts/`、CI、corpus。

## 回報格式（最後一則訊息）
- 改動檔案清單＋每檔一句話摘要
- 驗收 1–6 實際跑過的命令與輸出摘要
- tables.jsonl 是否變動＋diff＋理由
- 新增測試清單與先紅後綠證據（紅時 panic 訊息一行）
