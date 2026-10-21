# 任務卡 F1：footnotes.xml／endnotes.xml 解析＋引用標記（純解析層，渲染零變化）

## 你的角色與紀律（違反＝任務失敗）
- 你在 git worktree `/home/jerry/projects/quire/.worktrees/oc-f-notes`（branch `feat/notes-parse`）工作。先 `pwd` 確認。
- **嚴禁** `git commit` / `git push` / `git checkout` / `git reset` / `git stash`。改動留工作區，PM 逐行 review 後自行署名合併。
- **純解析層**：本卡不讓任何版面／顯示列表／SVG 消費新欄位。渲染輸出必須逐位元組零變化（PM 會做 400 檔 A/B diff）。
- 所有註釋／測試文案**繁體中文**（禁簡體；「稽核/台/了/干/吃/于/才/回/藉/游/岩/晒/峰/群」為 opencc 白名單）。
- 風格對齊 repo 慣例：`///` doc comment 寫做什麼＋為什麼＋ECMA-376 出處；新欄位一律 `#[serde(default)]`（`Option` 欄加 `skip_serializing_if = "Option::is_none"`）。

## 專案背景
`quire` 是 Rust lossless DOCX 引擎（`core/ooxml` 解析層→`core/layout` 版面→`core/engine-api` 顯示列表）。存檔走 parts passthrough（`OoxmlDoc.parts` 明文原樣進出），所以解析增強對 roundtrip 零風險——別破壞它。
**現況缺陷**：footnote/endnote 完全未解析。render-lab 對 LO 基線時 LO 渲染了註腳（HC031 p4 底部「1 This is a footnote.」）、我們整個跳過，造成分頁位移與相似度崩。後續卡 F2 做版面與發射，本卡先把資料讀進來。
**現成結構可仿**：`Run::field: Option<FieldKind>`（`core/ooxml/src/lib.rs` 約 74 行，欄位結果標記——引用標記與它同構）；`theme.rs` 的 `ThemeColors::from_parts(parts)`（從 `BTreeMap<String, Vec<u8>>` 找 part 再解析的容錯模式，抄它的「找不到＝空值不虛構」口徑）。

## 目標

### 1. 新模組 `core/ooxml/src/notes.rs`
```rust
/// 註腳／尾註本體（`word/footnotes.xml` 的 `w:footnote`、`word/endnotes.xml` 的 `w:endnote`；ECMA-376 §17.11）。
pub struct Note {
    pub id: i64,                    // w:id（一般註腳 ≥1）
    pub kind: NoteKind,             // Regular／Separator／ContinuationSeparator
    pub paragraphs: Vec<Paragraph>, // 註腳文字（用既有 Paragraph 型別全解析；引用來源＝本檔自身）
}
pub enum NoteKind { Regular, Separator, ContinuationSeparator }  // w:type 缺省＝Regular；"separator"／"continuationSeparator" 對應；其餘 type（continuation 等）→老實歸 Regular 並在欄位註標明取捨
pub struct Notes {  // 一檔的彙總
    pub footnotes: Vec<Note>,
    pub endnotes: Vec<Note>,
}
impl Notes { pub fn from_parts(parts: &BTreeMap<String, Vec<u8>>) -> Notes; }  // 缺 part／壞 XML＝空彙總（容錯口徑同 ThemeColors）
```
- 註腳段落解析直接複用本 crate 既有段落解析路徑（找 `parse` 內部怎麼解 `w:p`，抽或調同款函式；解不了的全欄位退化为空文字段落也要留 note 記錄——老實，不丟）。
- **分離式 id 查找**：`Notes::footnote(id)`／`endnote(id)`（`Option<&Note>`）。

### 2. 引用標記進 Run（`core/ooxml/src/lib.rs`）
- `Run` 加 `pub note_ref: Option<NoteRef>`（`#[serde(default, skip_serializing_if)]`）：
```rust
pub struct NoteRef { pub kind: NoteKind2, pub id: i64 }   // NoteKind2 = Footnote|Endnote
```
- 段落解析 `w:rPr` 之外的 run 內容處，遇 `<w:footnoteReference w:id="N"/>`／`<w:endnoteReference w:id="N"/>` 生成**一個只帶 note_ref 的 Run**（text 空），語義仿 `field` 標記。**同段落純文字 run 不受影響**。
- `OoxmlDoc` 加 `pub notes: Notes`（從 `parts` 一次性解析，仿 ThemeColors 取得路徑；serde default 保相容）。

### 3. 零消費
- `core/layout`、`core/engine-api` 的顯示列表／測量**嚴禁消費**以上任何新欄位（grep 自查：`note_ref`／`Notes` 只在 core/ooxml 內出現）。

## 驗收標準（PM 逐項重跑）
1. `cargo fmt --all --check` 綠
2. `cargo clippy --workspace --all-targets -- -D warnings` 綠
3. `cargo test --workspace --no-fail-fast` 全綠（既有測試零改動）
4. `cargo run -q -p quire-cli -- roundtrip corpus --check` → `399 檔 byte-identical 399 失敗 0`
5. `cargo run -q -p quire-layout --bin goldens` 三檔零位移
6. `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine` 綠
7. **渲染零變化實證**：改碼前先跑
   `cargo run -q -p quire-cli -- render corpus/xopt/TestFiles_HC031-Complicated-Document.docx --out /tmp/f1-before`
   `cargo run -q -p quire-cli -- render corpus/poi/test-data_document_form_footnotes.docx --out /tmp/f1-before-fn`
   改完同樣兩檔 render 到 /tmp/f1-after(-fn)，`diff -r` 必空。
8. **新增測試（≥5，先紅後綠——先寫測試看紅再實作）**，實檔用 `corpus/xopt/TestFiles_HC031-Complicated-Document.docx`（有 footnote id=1「 This is a footnote.」與 endnote id=1）與 `corpus/poi/test-data_document_form_footnotes.docx`：
   (a) HC031：`Notes::from_parts` 解出 footnotes 含 Separator(-1)/ContinuationSeparator(0)/Regular(1)，Regular 文字含「This is a footnote」；endnotes 同構
   (b) HC031：document 投影恰好 1 個 `note_ref(Footnote,1)` run 與 1 個 `note_ref(Endnote,1)` run
   (c) 無 footnotes.xml 的檔（任选 corpus 一個）→ `Notes::default()` 等價空、不 panic
   (d) serde 相容：不帶 `notes`／`note_ref` 的舊 JSON 字串可反序列化（得 None/空）；新欄位 None 時不出現於序列化輸出
   (e) 病態輸入：`<w:footnoteReference/>`（缺 id）→ 老實跳過該標記不 panic（在註釋說明取捨）

## 禁区
- 不碰 `core/layout/`、`core/engine-api/`、`core/render-trait/`（零消費鐵律）。
- 不碰存檔路徑（writer／passthrough／zip_struct）。
- 不碰 `scripts/`、CI、corpus。

## 回報格式
- 改動檔案清單＋每檔一句話
- 驗收 1–7 實際跑過的命令與輸出摘要（未跑聲稱已跑＝失敗）
- 新增測試清單＋先紅後綠證據（紅時 panic 一行）
