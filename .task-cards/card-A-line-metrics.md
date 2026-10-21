# 任務卡 A：分頁器行高分解診斷工具（quire-layout）

## 你的角色與紀律（違反＝任務失敗）
- 你在 git worktree `/home/jerry/projects/quire/.worktrees/oc-paging-a`（branch `tool/line-metrics-report`）工作。
- **嚴禁** `git commit` / `git push` / `git checkout` / `git reset` / `git stash`。改動留在工作區，由 PM 逐行 review 後自行署名提交。
- **嚴禁**修改任何既有渲染/分頁行為（`core/layout/src/lib.rs` 的 `paginate`/`measure_lines` 等函式的**邏輯**）。本卡是**純觀測工具**加裝：goldens 必須零位移。
- 所有程式註釋與輸出訊息一律**繁體中文**（禁簡體；「稽核/台/了/干/吃/于/才/回/藉/游/岩/晒/峰/群」為 opencc 白名單字，可用）。
- 程式碼風格對齊本 repo 既有慣例：函式上方 `///` doc comment 寫「做什麼＋為什麼＋ECMA-376 出處（若適用）」；測試放同檔 `#[cfg(test)]`。

## 專案背景（你需要但不知道的）
`quire` 是 Rust 寫的 lossless DOCX 引擎。分頁器在 `core/layout/src/lib.rs`（`paginate`、行測量、section geometry）。
目前與 LibreOffice 基線做像素對比（`scripts/render_lab.py`），最大殺手是**分頁點分歧**：同一份文件我方 N 頁、LO N±1 頁，之後每頁像素對比全錯。要修分頁，先得知道**每一頁每一段的行高貢獻是怎麼湊出來的**——現在沒有這種觀測手段。

## 目標（交付物）
新增一個**獨立 bin target**（建議 `shells/native` 或 `core/layout` 的 `examples/`／`[[bin]]`，與既有 `goldens` bin 同質、同風格），命名 `line_metrics`：

```
cargo run -p <crate> --bin line_metrics -- <docx 路徑> [--out 輸出路徑]
```

輸出 JSON（stdout 或 --out 檔），結構：
- 頂層：`{ "file": ..., "page_count": N, "pages": [...] }`
- 每頁：`{ "page": i, "width_pt":, "height_pt":, "starts_section": bool, "lines": [...] }`
- 每行（line）：`{ "para_index":, "line_in_para":, "line_count_before":, "top_pt":, "height_pt":, "font_size_pt":, "line_rule": <"exact"|"atLeast"|"auto"|"unknown">, "spacing_before_pt":, "spacing_after_pt":, "kind": <"text"|"table"|"empty-para"|"page-break"|"unknown"> }`

## 實作要點
1. **只讀不改**：從既有分頁流程**旁路收集**（在構造 line 的地方 push 記錄），或從 `paginate` 已產出的 `Page` 結構重建——選改動最小、最不可能干擾渲染行為的做法。若 `Page`/line 結構目前沒帶 font_size/spacing 等欄位，**允許**在 line 結構上加欄位（純資料欄位，不改變佈局計算），但不得改既有欄位語義。
2. `height_pt`＝該行在分頁時實際消耗的推進高度（與分頁判定同一個數值，這是本工具的全部意義）。
3. `line_rule`／spacing 若來源是段落樣式級聯，報告「最終生效值」即可，不必標來源層級。
4. 對 **table 行**：報告表格所在區段每列的推进高度即可，kind=table（無需展開 cell 級）。
5. **容錯**：任何收集不到的小節填 `null`／`"unknown"`，**不得 panic**、不得因缺欄位拒絕輸出。
6. 加**至少兩個單元測試**（放 lib.rs 或 bin 旁，跟 repo 慣例）：(a) 兩段直排文字文件 → 行數/頁數與 `paginate` 直調結果一致；(b) 含一個手動換頁符的文件 → 跨頁行記錄的 `top_pt` 在頁界重置。

## 驗收標準（PM 會逐項跑）
1. `cargo fmt --all --check` 綠
2. `cargo clippy --workspace --all-targets -- -D warnings` 綠
3. `cargo test --workspace` 全綠（既有測試零改動、零新增紅）
4. `cargo run -q -p quire-layout --bin goldens` 輸出與現行 goldens **零位移**
5. `cargo run -q -p quire-cli -- roundtrip corpus --check` 仍 `399 檔 byte-identical 399`（本卡不碰存檔路徑，理應零影響——若紅即你改到不該改的）
6. `cargo run -p <crate> --bin line_metrics -- corpus/xopt/TestFiles_HC031-Complicated-Document.docx` 成功輸出合法 JSON，頁數與 `quire-cli render` 的 pagebreaks.json 一致（8 頁）
7. `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout` 綠（工具 bin 不得把非 wasm-safe 依賴拖進 lib）

## 禁区
- 不碰 `core/ooxml/`（解析層）、`core/engine-api/`（顯示列表層）。
- 不碰 `scripts/`、CI、Cargo.toml 的既有欄位（新增 `[[bin]]` 允許）。
- 不動 corpus 任何檔案。

## 回報格式（最後一則訊息）
- 改了哪些檔案（路徑清單）
- 上述 7 項驗收你實際跑過的命令與輸出摘要（未跑聲稱已跑＝失敗）
- 你對「HC031 我方比 LO 少一頁」的初步觀察（若 JSON 已能看出哪個段落行高可疑，指出 para_index）
