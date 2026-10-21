# 任務卡 H1：footnotePr／endnotePr 設定解析（純解析層，渲染零變化）

## 你的角色與紀律（違反＝任務失敗）
- 你在 git worktree `/home/jerry/projects/quire/.worktrees/oc-notepr`（branch `oc/notepr-parse`）工作。先 `pwd` 確認。
- **嚴禁** `git commit` / `git push` / `git checkout` / `git reset` / `git stash`。改動留工作區，PM 逐行 review 後自行合併。
- **嚴禁碰主 checkout**（`/home/jerry/projects/quire`）：所有 cargo 指令與 render 基準都在**你自己的 worktree 內**跑（render 輸出寫 `/tmp/h1-*`）。
- **純解析層**：本卡不讓任何版面／顯示列表／SVG／編號邏輯消費新欄位（消費是 F2c／F3 的事）。渲染輸出必須逐位元組零變化（PM 會做 A/B diff）。
- 所有註釋／測試文案**繁體中文**（禁簡體；「稽核/台/了/干/吃/于/才/回/藉/游/岩/晒/峰/群」為 opencc 白名單）。
- 風格對齊 repo 慣例：`///` doc comment 寫做什麼＋為什麼＋ECMA-376 出處；新欄位一律 `#[serde(default)]`（`Option` 欄加 `skip_serializing_if = "Option::is_none"`）。

## 專案背景
`quire` 是 Rust lossless DOCX 引擎（`core/ooxml` 解析層→`core/layout` 版面→`core/engine-api` 顯示列表）。存檔走 parts passthrough（`OoxmlDoc.parts` 明文原樣進出），解析增強對 roundtrip 零風險——別破壞它。
現成仿效對象：`core/ooxml/src/notes.rs`（F1 已落地的 `Notes::from_parts` 容錯口徑：缺 part／壞 XML＝空值不虛構）與 `theme.rs::ThemeColors::from_parts`。

## 實據（PM 已實掃 corpus 400 檔，照此起算，不要憑 ECMA 全集臆造需求）
- **settings.xml 層級** `w:footnotePr`／`w:endnotePr`：184 檔出現。內容絕大多數只有分隔線引用 `<w:footnote w:id="0"/>`／`<w:footnote w:id="-1"/>`（尾註同構 `w:endnote`）；少量真實設定：
  - `w:pos w:val="beneathText"`（4 檔）、`w:pos w:val="sectEnd"`（1 檔，endnotePr）
  - `w:numRestart w:val="eachSect"`（2 檔）、`w:numFmt w:val="decimal|lowerRoman|chicago"`（4 檔）
- **sectPr 層級**：9 檔實例（`OpenXmlPowerToolsExamples_DocumentBuilder04_Mars/Uranus/solar-system.docx`、`tests_PhpWordTests__files_documents_reader-comments/-styles.docx`、`test-data_document_WordWithAttachments.docx`、`tdf129659.docx`、`fdo77725.docx`、`tableCurrupt.docx`），值同上（pos／numRestart／numFmt）。
- ECMA-376 出處：§17.11.5 `w:footnotePr`、§17.11.17 `w:endnotePr`；子元素全集＝`w:footnote`/`w:endnote`（分隔線 id 引用）、`w:numFmt`（§17.11.18）、`w:numRestart`（§17.11.19）、`w:num`（起始值 §17.11.16）、`w:pos`（§17.11.14）。

## 目標

### 1. 新結構（放 `core/ooxml/src/notes.rs`，與 F1 同檔）
```rust
/// 註腳／尾註的編號與位置設定（ECMA-376 §17.11.5／§17.11.17）。
/// settings.xml 層＝文件預設；sectPr 層＝節覆寫（逐欄覆寫，非整組替換）。
pub struct NotePr {
    /// `w:numFmt/@w:val`（§17.11.18）。未見值老實歸 None（不臆造 decimal）。
    pub num_fmt: Option<NoteNumberFormat>,
    /// `w:numRestart/@w:val`（§17.11.19）：continuous|eachPage|eachSect。
    pub num_restart: Option<NoteRestart>,
    /// `w:num/@w:val`（§17.11.16）：起始編號；缺＝None（消費端用 1）。
    pub num_start: Option<i32>,
    /// `w:pos/@w:val`（§17.11.14）：beneathText|pageBottom（註腳）；
    /// documentEnd|pageEnd|sectEnd（尾註）。共用一枚舉以值域分流，註釋寫清。
    pub pos: Option<NotePos>,
    /// `w:footnote/@w:id`／`w:endnote/@w:id`：分隔線／續接線本體的註 id
    /// （Word 慣例 -1/0）。引用存在性記錄即可，本卡不解析其內容。
    pub ref_id: Option<i64>,
    pub separator_id: Option<i64>,
    pub continuation_separator_id: Option<i64>,
}
```
（型別名可依 repo 命名慣例微調，但欄位語義不得增刪；`w:footnote` 子元素三態——`w:type="separator"`／`"continuationSeparator"`／無 type＝參考線 ref——照 ECMA §17.11.6 分流。）

### 2. 取得路徑（兩個層級都要）
- **settings.xml**：`Notes` 加欄位 `pub settings: NoteSettings`（`pub struct NoteSettings { pub footnote: Option<NotePr>, pub endnote: Option<NotePr> }`），`Notes::from_parts` 順手讀 `word/settings.xml`（缺＝None，容錯口徑同現行）。
- **sectPr**：`core/ooxml` 讀 section 的既有路徑（找 `read_sections_per_paragraph`／`read_sections_header_footer` 所在檔）加 `pub note_pr: Option<NotePr>`——**只在既有 SectionDef 級結構上掛欄位**，不動函式簽名回傳形状（serde default 相容）。若既有結構不便承載，寧可新增獨立函式 `read_sections_note_pr(xml) -> Vec<Option<NotePr>>`（與逐段 section 表同標），也不要改動既有簽名。

### 3. 零消費
`core/layout`、`core/engine-api`、`shells/` 嚴禁讀任何新欄位（grep 自查：`NotePr`／`num_fmt`／`num_restart`／`note_pr` 只在 `core/ooxml` 內出現；`settings` 欄位名太泛，自查用 `note_settings|NoteSettings`）。

## 驗收標準（PM 逐項重跑）
1. `cargo fmt --all --check` 綠
2. `cargo clippy --workspace --all-targets -- -D warnings` 綠
3. `cargo test --workspace --no-fail-fast` 全綠（既有測試零改動）
4. `cargo run -q -p quire-cli -- roundtrip corpus --check` → `399 檔 byte-identical 399 失敗 0`
5. `cargo run -q -p quire-layout --bin goldens` 三檔零位移
6. `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine` 綠
7. **渲染零變化實證（在你 worktree 內跑）**：改碼前
   `cargo run -q -p quire-cli -- render corpus/xopt/TestFiles_HC031-Complicated-Document.docx --out /tmp/h1-before`
   `cargo run -q -p quire-cli -- render corpus/ooxmlexport/fdo77725.docx --out /tmp/h1-before-f77725`
   改完輸出 `/tmp/h1-after(-f77725)`，`diff -r` 必空。
8. **新增測試（≥6，先紅後綠——先寫測試看紅再實作）**：
   (a) HC031 settings 層：`Notes::from_parts` 讀到 footnote/endnote 的 separator/continuation 引用 id（實檔值 -1/0）
   (b) `fdo77725.docx`：sectPr 的 `numRestart=eachSect` 落 `NoteRestart::EachSect`（實檔）
   (c) `reader-styles.docx`：endnotePr `numFmt=lowerRoman` 落 `NoteNumberFormat::LowerRoman`（實檔）
   (d) 病態輸入：`<w:numFmt w:val="還來沒見過的格式"/>`→老實 None 不 panic；`<w:footnotePr/>` 空組→Some(預設全 None)
   (e) serde 相容：不帶新欄位的舊 JSON 字串可反序列化；None 欄不外露
   (f) 逐欄覆寫語義：settings 有 `numFmt=decimal`＋sectPr 有 `pos=beneathText` 的自製 XML→兩層各自成組（合併是消費端的事，解析層老實分層存）

## 禁区
- 不碰 `core/layout/`、`core/engine-api/`、`shells/`（零消費鐵律）。
- 不碰存檔路徑（writer／passthrough／zip_struct）。
- 不碰 `scripts/`、CI、corpus、主 checkout。

## 回報格式
- 改動檔案清單＋每檔一句話
- 先紅後綠證據（紅燈原文貼上）
- 驗收 1–8 逐項實跑輸出
- 一句話自評（哪裡最可能有坑）
