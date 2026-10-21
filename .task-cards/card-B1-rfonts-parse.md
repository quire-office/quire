# 任務卡 B1：rFonts 解析＋theme fontScheme 查表（純解析層，零渲染行為變更）

## 你的角色與紀律（違反＝任務失敗）
- 你在 git worktree `/home/jerry/projects/quire/.worktrees/oc-rfonts`（branch `feat/rfonts-parse`）工作。先 `pwd` 確認。
- **嚴禁** `git commit` / `git push` / `git checkout` / `git reset` / `git stash`。改動留在工作區，PM 逐行 review 後自行署名提交。
- **本卡是純解析層**：解析並儲存字型宣告，**嚴禁**改任何測量／分頁／顯示列表／SVG 輸出邏輯。驗收要求渲染輸出逐位元組零變化——這是你能與其他任務並行的唯一前提。
- 所有程式註釋與測試文案一律**繁體中文**（禁簡體；「稽核/台/了/干/吃/于/才/回/藉/游/岩/晒/峰/群」為 opencc 白名單字）。
- 風格對齊 repo 慣例：`///` doc comment 寫「做什麼＋為什麼＋ECMA-376 出處」；新欄位一律 `#[serde(default)]` 保快照相容。

## 專案背景
`quire` 是 Rust lossless DOCX 引擎。`core/ooxml/` 是解析層（純讀取；存檔路徑 passthrough，styles.xml/document.xml 原樣進出，**這是你 byte-identical 399/399 零風險的來源——別破坏它**）。
現況：`RunStyle`（`core/ooxml/src/styles.rs` 約 51 行起）只有 size/bold/italic/underline/strike/color，**完全沒有字型族**；`StyleTable` 文件註釋（約 180 行起）把 `w:rFonts` 列為「未支援清單」第一項。`Run`（`core/ooxml/src/lib.rs` 約 57 行起）有 `direct: Option<RunStyle>` 直接格式快照與 `rstyle` 字元樣式引用。`core/ooxml/src/theme.rs` 已解析 clrScheme（`ThemeColors`），模組頭註釋明示 fontScheme 未做。

## 目標
### 1. `styles.rs`：解析 `w:rFonts`
新增（放 RunStyle 旁）：
```rust
/// `w:rFonts`（ECMA-376 §17.3.2.26）：字型族宣告。各欄 None＝未宣告（繼承）。
pub struct RunFonts { pub ascii: Option<String>, pub east_asia: Option<String>, pub cs: Option<String>, pub hint_east_asian: bool }
```
- `ascii`←`@w:ascii`、`east_asia`←`@w:eastAsia`、`cs`←`@w:cs`、`hint_east_asian`←`@w:hint=="eastAsia"`。
- 掛進 `RunStyle` 新欄位 `fonts: Option<RunFonts>`（`#[serde(default)]`）。
- 級聯語義與既有欄位一致：**宣告即贏**（下層 `Some` 覆蓋上層；全鏈 None＝None）。`StyleTable` 的 basedOn 鏈合成、`resolve_run`、字元樣式疊加三處都要帶上它（對齊 size/color 的既有級聯寫法，逐處比照）。
- `apply_run_tag` 加 `rFonts` 分支（只抓屬性，不讀文字）。

### 2. `theme.rs`：解析 `a:fontScheme`
新增 `ThemeFonts`（對齊 `ThemeColors` 的容錯口徑：壞 XML＝停在已收部分，缺 part＝空值全 None）：
- 抓 `a:fontScheme/a:majorFont` 與 `a:minorFont` 下的 `a:latin/@typeface` 與 `a:ea/@typeface` → 四欄 `major_ascii/major_east_asia/minor_ascii/minor_east_asia: Option<String>`。
- 注意 clrScheme 解析已有 `in_scheme` 旗標模式——fontScheme 在 theme1.xml 的同級節點，用同樣的「進入/離開節點才認」防串位。
- 暴露方式對齊 `ThemeColors`（呼叫端已有拿 theme part 的通道，沿用）。

### 3. `Run` 有效字型烘焙
仿 `effective_size`（`Run` 既有欄位，engine 開啟時烘焙）加 `effective_fonts: Option<RunFonts>`（`#[serde(default)]`）；engine 開啟文件處（`core/engine-api/src/lib.rs` 找 `effective_size` 烘焙點）一併烘焙。**只存不讀**——本卡不讓任何測量／渲染消費它。

## 序列化相容鐵律
- 所有新欄位 `#[serde(default)]`。**若 `RunStyle`/`Run`/相關結構出現在任何 goldens 或快照的序列化輸出**（先 grep `serde_json::to_*` 與 goldens bin 確認），新欄位必須加 `skip_serializing_if = "Option::is_none"`，否則 goldens 會因多出 `"fonts":null` 變紅。
- `hint_east_asian: bool` 也要 `#[serde(default)]`。

## 驗收標準（PM 會逐項重跑）
1. `cargo fmt --all --check` 綠
2. `cargo clippy --workspace --all-targets -- -D warnings` 綠
3. `cargo test --workspace` 全綠（既有測試零改動）
4. `cargo run -q -p quire-layout --bin goldens` **零位移**（三個 jsonl 全 ✓）
5. `cargo run -q -p quire-cli -- roundtrip corpus --check` → `399 檔 byte-identical 399 失敗 0`
6. `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-engine` 綠
7. **新增測試（≥4 個，先紅後綠：先寫測試看紅，再實作轉綠）**：
   (a) rFonts 直接格式解析：`<w:rFonts w:ascii="Arial" w:eastAsia="標楷體" w:hint="eastAsia"/>` → ascii/east_asia/hint 正確
   (b) 級聯優先序：docDefaults 宣告 Arial → 段落樣式宣告 Times → 直接格式宣告 Courier → 有效值 Courier；直接格式只宣告 ascii 時 east_asia 落到上層值（逐欄繼承，不是整包覆蓋）
   (c) fontScheme 解析：majorFont latin="Calibri Light" ea="MS 明朝" → 四欄正確；無 fontScheme 節點 → 全 None
   (d) 序列化相容：不含 fonts 欄位的舊版 JSON 字串能反序列化（None），且新欄位 None 時不出現於序列化輸出（若採 skip_serializing_if）
8. 渲染零變化實證：`cargo run -q -p quire-cli -- render corpus/ooxmlimport/tdf170236.docx --out /tmp/b1-a` 與 merge-base 版（`git stash` **禁用**；改用 `git worktree` 思路以外的方法：直接先跑一次存 /tmp/b1-before 再開始改碼）逐檔 diff 為空。

## 禁区
- 不碰 `core/layout/`（分頁/測量）、`core/render-trait/`、SVG 輸出、CRDT 協作分塊邏輯。
- 不碰存檔路徑（`core/ooxml/` 的 writer/passthrough——roundtrip 靠它）。
- 不碰 `scripts/`、CI、corpus。

## 回報格式（最後一則訊息）
- 改動檔案清單＋每檔一句話摘要
- 驗收 1–8 你實際跑過的命令與輸出摘要（未跑聲稱已跑＝失敗）
- 新增測試清單與先紅後綠證據（紅時的 panic 訊息一行）
