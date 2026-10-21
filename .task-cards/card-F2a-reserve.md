# 任務卡 F2a：分頁器「頁底預留帶」機制（LayoutParams 參數化，預設 0＝零變化）

## 你的角色與紀律（違反＝任務任務失敗）
- 你在 git worktree `/home/jerry/projects/quire/.worktrees/oc-f2a`（branch `feat/footprint-reserve`）工作。先 `pwd` 確認。
- **嚴禁** `git commit` / `git push` / `git checkout` / `git reset` / `git stash`。改動留工作區，PM 逐行 review 後自行署名合併。
- 所有註釋／測試文案**繁體中文**（禁簡體；「稽核/台/了/干/吃/于/才/回/藉/游/岩/晒/峰/群」為 opencc 白名單）。
- 風格對齊 repo 慣例（`///` doc＋出處＋取捨）。
- 本卡是**機制卡**：只做「頁底可預留 N pt」的分頁參數化，預設 0 時渲染輸出必須**逐位元組零變化**。真正的註腳接線（引擎決定每頁預留多少）是下一張卡 F2b，**不歸你**。

## 專案背景
`quire`（Rust lossless DOCX 引擎；`core/layout` 版面層）。註腳要在頁底佔空間：Word 把正文區（text body）压缩給註腳讓位，分頁隨之前移。現況分頁容量由 `LayoutParams::per_page_lines()`（`core/layout/src/lib.rs` 約 117 行）決定：`(頁高 − top邊界 − bottom邊界).floor() / 帶高`。F1（git log 552d5be）已把註腳資料讀進 `OoxmlDoc.notes`，本卡給分頁器装上「頁底預留」旋鈕。

## 目標

### 1. `LayoutParams` 加預留欄（`core/layout/src/lib.rs`）
```rust
/// 頁底預留高（pt）：正文可用區底部要讓給頁底註腳區的空間。
/// 0＝無預留（現況行為，逐位元組零變化）；負值／NaN 一律視同 0（防呆）。
pub footnote_reserve_pt: f32,
```
- `Default`／`from_section` 構造路徑都給 0（不改變任何既有構造語義）。
- `per_page_lines()` 改為：`content_h = (頁高 − top − bottom − footnote_reserve_pt).max(line_h)` 再量化——**預留 0 時算式與舊版逐位元組等價**（注意 `max(line_h)` 兜底位置別動錯，容量恆 ≥1 帶的既有不变式要守住）。
- 預留超過可用高（病態值）：容量仍 ≥1（兜底順序寫進註釋）。
- `Clone/Copy/PartialEq` 等 derive 照舊可用；`line-metrics`（`tools/line-metrics`）等所有既有構造點補 0（編譯器會全點出來）。

### 2. 零消費
- `footnote_reserve_pt` 本卡**只有測試會給非零值**。grep 自查：除本卡測試外，倉庫裡不得有任何地方對它寫非 0。

## 驗收標準（PM 逐項重跑）
1. `cargo fmt --all --check` 綠
2. `cargo clippy --workspace --all-targets -- -D warnings` 綠
3. `cargo test --workspace --no-fail-fast` 全綠（既有測試零改動）
4. `cargo run -q -p quire-cli -- roundtrip corpus --check` → `399 byte-identical 399 失敗 0`
5. `cargo run -q -p quire-layout --bin goldens` 三檔零位移
6. `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine` 綠
7. **渲染零變化實證**：改碼前先存基準
   `cargo run -q -p quire-cli -- render corpus/xopt/TestFiles_HC031-Complicated-Document.docx --out /tmp/f2a-before`
   `cargo run -q -p quire-cli -- render corpus/poi/test-data_document_form_footnotes.docx --out /tmp/f2a-before-fn`
   改完 render 到 /tmp/f2a-after(-fn)，`diff -r` 兩對皆空。
8. **新增測試（≥4，先紅後綠）**，放 `core/layout` 既有 test 區：
   (a) 同一份長文件輸入，reserve=0 vs reserve=`2*line_h`：後者首頁 `line_count` 恰少 2（精確等式，不是 `<`）
   (b) reserve=0 時 `per_page_lines()` 與手算 `(content_h/line_h).floor()` 舊公式精確相等（零變化守門）
   (c) 病態：reserve=頁高、reserve=−50、reserve=NaN → 不 panic、容量恆 ≥1、NaN 視同 0 與 reserve=0 等值
   (d) 端到端：透過 `break_lines_at`／分頁主入口餵 reserve≠0 的 params，總頁數增加且首頁末行 top 不進預留區（寫清楚你測的是哪條路徑）

## 禁区
- 不碰 `core/ooxml/`、`core/engine-api/`、`core/render-trait/`、`shells/`（本卡純 layout＋構造點補 0 的連帶修改）。
- 不碰 `tools/line-metrics` 的**邏輯**（只准補構造點 `footnote_reserve_pt: 0`）。
- 不碰存檔路徑、`scripts/`、CI、corpus。

## 回報格式
- 改動檔案清單＋每檔一句話
- 驗收 1–7 實際命令與輸出摘要（未跑聲稱已跑＝失敗）
- 新增測試清單＋先紅後綠證據（紅時 panic 一行）
