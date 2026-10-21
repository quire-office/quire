# 任務卡 F2b-1：分頁器「逐頁預留」消費＋迭代收斂機制（空預留＝零變化）

## 你的角色與紀律（違反＝任務失敗）
- 你在 git worktree `/home/jerry/projects/quire/.worktrees/oc-f2b1`（branch `feat/perpage-reserve`）工作。先 `pwd` 確認。
- **嚴禁** `git commit` / `git push` / `git checkout` / `git reset` / `git stash`。改動留工作區，PM 逐行 review。
- 所有註釋／測試文案**繁體中文**（禁簡體）。風格對齊 repo 慣例（`///` doc＋取捨）。
- 本卡是**機制卡**：`reserves` 為空／全 0 時渲染輸出必須**逐位元組零變化**。真正的「註腳算預留量＋發射」是下一張卡，不歸你。

## 專案背景與已就位的零件（先讀懂再動手）
`quire`（Rust lossless DOCX 引擎）。註腳管線已鋪三段：
- **F1**：`OoxmlDoc.notes`＋`Run::note_ref`（資料在）。
- **F2a**（git log fd00126）：`LayoutParams::footnote_reserve_pt`＋`per_page_lines()` 扣預留＋`section_capacity(p, geo)`——但這是**文件層級**單一旋鈕。
- Word 實際行為：**逐頁**預留——有註腳的頁才壓正文區。且存在雞生蛋問題：預留量取決於哪些段落的註腳落哪頁，而分頁又受預留影響。Word 用迭代收斂解，本卡做同款機制。

關鍵代碼路徑（先讀）：`core/layout/src/lib.rs` 的 `pack_run`（光標 `(fi, li, band)`＋`cur_used`，每步取 `section_capacity(p, &spec.geo)` 當容量）、分頁主入口（`paginate_flow`／`rebuild_full` 一帶，看簽名與呼叫鏈）。

## 目標

### 1. `LayoutParams` 加逐頁預留表
```rust
/// 逐頁頁底預留（pt，索引＝頁序）：超出表長的頁視同 0。
/// 空表＝無逐頁預留（現況零變化）。與 footnote_reserve_pt（文件層級
/// 基底）疊加取 max（兩者語義：基底給「全頁統預留」場景，逐頁表給
/// 「特定頁有註腳」場景；疊加口徑寫進欄注＋測死）。
pub footnote_reserves_pt: Vec<f32>,
```
- `Default` 給空表；`from_section` 不帶此欄（恆空，構造語義不動）；`PartialEq` derive 已可用。負值／NaN 元素視同 0（F2a 同款防呆，抽同款 helper）。

### 2. 容量計算接逐頁表
- `section_capacity` 加頁序參數（或新 helper）：該頁容量＝`per_page_lines()` 但底部預留取 `max(文件層級, reserves[頁序]?)`。
- `pack_run` 維護「當前頁序」游標（跨頁時 +1），逐頁表生效時每頁容量獨立。**預留全空時走舊單值路徑**（零變化構造性保證，F2a 同款口徑：不依賴 x-0.0==x，直接跳過逐頁查表分支）。

### 3. 迭代收斂入口（新 pub fn，不動既有入口簽名）
```rust
/// 以「預留→重排→再算預留」迭代到穩定的分頁入口（Word 同款收斂思路）。
/// reserve_fn：吃「本輪每頁的首/末段落錨點」回該頁預留（pt）。
/// 最多 3 輪；回傳最後一輪結果＋實際輪數。預留恆 0 時第 1 輪即收斂。
pub fn paginate_converged(..., reserve_fn: &dyn Fn(&PageAnchors) -> Vec<f32>) -> (Vec<PageBreak>, usize)
```
（參數型別依既有 `paginate_flow` 實際簽名設計，`PageAnchors` 至少含每頁 first_para/last_para；型別命名可微調但語義不變；3 輪上限＋「穩定即停」寫進 doc。）

### 4. 零消費
- 除本卡測試外，倉庫不得有任何地方給 `footnote_reserves_pt` 非空值／調 `paginate_converged`（grep 自查貼進回報）。

## 驗收標準（PM 逐項重跑）
1. `cargo fmt --all --check` 綠
2. `cargo clippy --workspace --all-targets -- -D warnings` 綠
3. `cargo test --workspace --no-fail-fast` 全綠（既有測試零改動）
4. `cargo run -q -p quire-cli -- roundtrip corpus --check` → `399 byte-identical 399 失敗 0`
5. `cargo run -q -p quire-layout --bin goldens` 三檔零位移
6. `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine` 綠
7. **渲染零變化實證**：PM 已備 `/tmp/f2b1-before*`（HC031＋POI footnotes 兩檔，**勿動**）；你 render 到 `/tmp/f2b1-after*`，`diff -r` 兩對皆空。
8. **新增測試（≥5，先紅後綠）**：
   (a) 兩頁長文，reserves=[0, 2*line_h, 0]：第 2 頁行數恰少 2、第 1/3 頁不受影響（精確等式）
   (b) reserves 空表 vs 不設：`page_breaks` 逐欄位相等（零變化守門）
   (c) 文件層級 10pt＋逐頁表 [0, 30pt]：第 2 頁取 30、其餘取 10（max 疊加口徑測死）
   (d) `paginate_converged`：reserve_fn 回「第 1 頁留 2 帶其餘 0」→ 收斂且回傳輪數 ≤3；reserve_fn 回全 0 → 恰 1 輪
   (e) 病態：reserves 元素為 NaN/−50/超大值 → 不 panic、兜底行為與 F2a 同款（容量恆 ≥1）
   (f)（鼓勵）reserves 長短於頁數：缺的視同 0、多的忽略——不 panic

## 禁区
- 不碰 `core/ooxml/`、`core/engine-api/`（消費端是下張卡）、`core/render-trait/`、`shells/`、存檔路徑、`scripts/`、corpus。
- `tools/line-metrics` 只准補構造點（`footnote_reserves_pt: Vec::new()`）。
- 不動既有 `paginate_flow`／`rebuild_full` 簽名（新入口另開）。

## 回報格式
- 改動檔案清單＋每檔一句話
- 驗收 1–7 實跑命令與輸出摘要（未跑聲稱已跑＝失敗）
- 新增測試清單＋先紅後綠證據（紅時 panic 原文）
- 零消費 grep 自查輸出
