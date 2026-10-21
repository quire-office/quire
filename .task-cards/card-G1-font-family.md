# 任務卡 G1：字型族選取管線——effective_fonts→顯示列表→SVG font-family

## 你的角色與紀律（違反＝任務失敗）
- 你在 git worktree `/home/jerry/projects/quire/.worktrees/oc-fonts`（branch `feat/font-family`）工作。先 `pwd` 確認。
- **嚴禁** `git commit` / `git push` / `git checkout` / `git reset` / `git stash`。改動留工作區，PM 逐行 review 後自行署名合併。
- 所有註釋／測試文案**繁體中文**（禁簡體；「稽核/台/了/干/吃/于/才/回/藉/游/岩/晒/峰/群」為 opencc 白名單）。
- 風格對齊 repo 慣例（`///` doc＋ECMA 出處；serde default 相容）。
- 本卡**允許**改變渲染輸出（這就是要的效果），但**不允許**改變任何**版面幾何**：分頁、行高、斷行、座標必須逐位元組不變（pagebreaks.json 零位移是鐵證）。

## 專案背景
`quire`（Rust lossless DOCX 引擎）。B1 卡（git log 0554a51）已落地：`Run::effective_fonts: Option<RunFonts>`（engine 開啟文件時烘焙，`ascii`/`east_asia`/`cs`/`hint_east_asian` 四欄，ECMA-376 §17.3.2.26），**只存不讀**。本卡是字型管線第二段：把這個宣告接到發射面。
現況：`core/render-trait/src/lib.rs` 的 `DrawItem::Text` **沒有字型欄位**（只有 size_pt/flags/color_argb）；SVG 輸出（`shells/native/src/render_svg.rs` 約 50 行 `<text>` 標籤）**完全不寫 font-family** → 瀏覽器全用預設字型，Word 標題的 Cambria/內文的 Calibri 等宣告全部丟失。
顯示列表 JSON 有舊版相容要求：**新欄位一律 `#[serde(default)]`**（對齊 `color_argb` 的做法）。

## 目標（三層各一刀，逐層乾淨）

### 1. `core/render-trait/src/lib.rs`：Text 加字型槽
`DrawItem::Text` 加：
```rust
/// 字型族名（w:rFonts 級聯選取結果；ECMA-376 §17.3.2.26）。
/// None＝文件未宣告（渲染端用自身預設），舊顯示列表 JSON 照解析。
#[serde(default, skip_serializing_if = "Option::is_none")]
pub family: Option<String>,
```
所有構造 `DrawItem::Text` 的點補 `family: None`（grep 全倉找齊）。

### 2. `core/engine-api/src/lib.rs`：選取規則＋發射
在發射顯示列表處（找 `push_frag_items`／構造 Text 的位置），每個文字項依**該文字所屬 run** 的 `effective_fonts` 選族：
- **選取規則（v1，寫進函式 doc）**：`hint_east_asian == true` 且 `east_asia` 有值 → `east_asia`；否則 `ascii` 有值 → `ascii`；否則 `east_asia` 有值且文字全為 CJK 碼點 → `east_asia`；皆無 → None（＝舊行為）。
- 表格格內文字（CellText）目前不帶 run 級字型（layout 層限制）→ 傳 None，函式註標明「格內選族待後續卡」。
- **選取只加欄位，不動任何座標/尺寸計算**。

### 3. `shells/native/src/render_svg.rs`：寫出 font-family
`Some(family)` 時在 `<text>` 加 `font-family="{escape(family)}"`（XML 跳脫：`&<>"'`；對齊既有 text 轉義函式，缺就補並測）。`None` 時完全不寫該屬性（輸出與舊版逐字節相同）。

## 鐵律與取捨
- **版面零變動**：pagebreaks.json 必須與改碼前逐位元組相同（這是「只加屬性不碰幾何」的證明，PM 會逐檔 diff）。
- `cs`（複雜文字）欄本卡**不消費**（引擎無 RTL 路徑），選取規則 doc 標明。
- `asciiTheme`/`eastAsiaTheme`（指向 theme fontScheme 的寫法）本卡**不做**——theme 查表已在 `theme.rs::ThemeFonts` 待接，另卡處理；本卡只吃直接名字。遇到只有 theme 引用的 run＝選不到 → None（老實退回，不虛構）。
- web shell（`shells/web`）：顯示列表 JSON 新增欄位向後相容，web 端不改（它的 canvas 字體堆疊另卡）；若 web 測試因欄位紅，只准加 serde 相容修，不准改 web 渲染邏輯。

## 驗收標準（PM 逐項重跑）
1. `cargo fmt --all --check` 綠
2. `cargo clippy --workspace --all-targets -- -D warnings` 綠
3. `cargo test --workspace --no-fail-fast` 全綠（既有測試**除了下面第 7 點明確允許的 SVG 斷言更新外**零改動）
4. `cargo run -q -p quire-cli -- roundtrip corpus --check` → `399 byte-identical 399 失敗 0`
5. `cargo run -q -p quire-layout --bin goldens` 三檔零位移
6. `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine` 綠
7. **版面零變動實證**：改碼前先跑三檔存基準
   `cargo run -q -p quire-cli -- render corpus/xopt/TestFiles_HC031-Complicated-Document.docx --out /tmp/g1-before-hc`
   `cargo run -q -p quire-cli -- render corpus/ooxmlimport/tdf170236.docx --out /tmp/g1-before-tdf`
   `cargo run -q -p quire-cli -- render corpus/poi/test-data_document_Bug54771b.docx --out /tmp/g1-before-bug`
   改完 render 到 /tmp/g1-after*：`pagebreaks.json` 三檔 `diff` **必空**；SVG 只准出現 `font-family` 屬性差異（PM 會 diff 抽查非 font-family 行零變化）。
8. **新增測試（≥5，先紅後綠）**：
   (a) 宣告 `w:rFonts w:ascii="Calibri"` 的文件 → 顯示列表 Text 項 `family=Some("Calibri")`
   (b) `w:hint="eastAsia"`＋eastAsia="標楷體" → family 選 east_asia
   (c) 無任何 rFonts 的文件（自己構造最小 docx 或找 corpus 現成）→ 顯示列表不帶 family 欄位（serde 跳過）＋SVG 無 font-family 字串
   (d) 舊顯示列表 JSON（無 family 欄）反序列化 OK＝None
   (e) family 含特殊字元（如 `"&<>"` 造 fake fixture 或直接單元測 SVG 轉義函式）→ XML 正確跳脫不炸結構
   (f)（鼓勵）pagebreaks 零位移防回歸測：HC031 render 前後 pagebreaks.json 字串等值

## 禁区
- 不碰 `core/layout/`（版面零變動鐵律；連「順手重構」都不行）。
- 不碰存檔路徑（writer/passthrough/zip_struct）。
- 不碰 `core/ooxml/` 的解析邏輯（B1 已交付，只消費 `effective_fonts`）。
- 不碰 `scripts/`、CI、corpus。

## 回報格式
- 改動檔案清單＋每檔一句話
- 驗收 1–7 實際命令與輸出摘要（未跑聲稱已跑＝失敗）
- 新增測試清單＋先紅後綠證據（紅時 panic 一行）
- 一句話自评：哪些檔案的 SVG 輸出實際出現 font-family（跑過才准回答）
