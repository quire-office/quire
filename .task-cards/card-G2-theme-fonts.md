# 任務卡 G2：rFonts theme 引用解析（asciiTheme/eastAsiaTheme → fontScheme 查表）

## 你的角色與紀律（違反＝任務失敗）
- 你在 git worktree `/home/jerry/projects/quire/.worktrees/oc-fonts2`（branch `feat/theme-font-refs`）工作。先 `pwd` 確認。
- **嚴禁** `git commit` / `git push` / `git checkout` / `git reset` / `git stash`。改動留工作區，PM 逐行 review 後自行署名合併。
- 所有註釋／測試文案**繁體中文**（禁簡體；「稽核/台/了/干/吃/于/才/回/藉/游/岩/晒/峰/群」為 opencc 白名單）。
- 風格對齊 repo 慣例（`///` doc＋ECMA 出處＋取捨；serde default＋skip 相容）。
- 本卡讓「只有 theme 引用的 run」也能選到字型族：**渲染輸出允許變**（多出 font-family 屬性是正確性修復），但**版面幾何不變**（pagebreaks.json 逐位元組零位移是鐵證——G1 已證明了這條路）。

## 專案背景與現成零件
`quire`（Rust lossless DOCX 引擎）。字型管線已兩段完成：
- **B1**（552d5be）：`RunFonts { ascii, east_asia, cs, hint_east_asian }` 解析＋逐欄級聯，engine 烘焙 `Run::effective_fonts`。
- **G1**（90c7877，讀 `git log`／`core/engine-api/src/lib.rs` 的 `select_text_family`）：顯示列表 `Text.family`＋SVG font-family。G1 刻意**不消費 theme 引用**：`asciiTheme`/`eastAsiaTheme` 的 run 選不到族、回 None——這卡把最後一段接上。
- **ThemeFonts**（`core/ooxml/src/theme.rs` 170 行起）已存在：`major_ascii/major_east_asia/minor_ascii/minor_east_asia` 四槽（DrawingML fontScheme），`ThemeFonts::from_parts(parts)` 現成。**本卡只做接線，不重寫查表**。
- corpus 規模（實測）：56 檔帶 `w:*Theme` 引用。HC031 是主力驗收檔：Heading 樣式 `<w:rFonts w:asciiTheme="majorHAnsi" w:eastAsiaTheme="majorEastAsia" .../>`，theme1.xml 的 majorFont latin="Calibri Light"、minorFont latin="Calibri"（ea typeface 皆空字串）。

## 目標

### 1. `RunFonts` 加 theme 槽（`core/ooxml`）
- 新增槽承載引用名（建議枚舉，禁止裸字串散落）：`ascii_theme`／`east_asia_theme`（`Option<FontSchemeSlot>`，serde default＋skip；`FontSchemeSlot { MajorAscii, MinorAscii, MajorEastAsia, MinorEastAsia }` 之類，命名自定但寫doc）。
- 解析映射（ECMA-376 §17.3.2.26 × DrawingML §20.1.8.22）：
  - `w:asciiTheme`/`w:hAnsiTheme`：`majorHAnsi`→MajorAscii、`minorHAnsi`→MinorAscii（HAnsi 槽對應 fontScheme 的 latin）。
  - `w:eastAsiaTheme`：`majorEastAsia`→MajorEastAsia、`minorEastAsia`→MinorEastAsia。
  - `w:cstheme`（Bidi 系）**不建模**——引擎無 cs 消費路徑（G1 同款取捨，欄注標明）。
  - 無法識別的值（如自佔位名）→ 老實 None，不虛構。
- **優先序鐵律**：ECMA §17.3.2.26 明文 theme 屬性與直接值同現時 **theme 取勝**——級聯照舊逐欄合成（下層直接值仍可覆蓋上層 theme 槽，因為級聯是逐欄「後層覆蓋前層」；**同一層內** theme 槽與直接值共存時，烘焙階段 theme 解析值取勝）。取捨全部寫進欄注。
- 級聯（`StyleTable`）逐欄語義不變：theme 槽各自獨立逐欄繼承（下層只宣告 asciiTheme 不會把上層的 east_asia 直接值抹掉——沿用 B1 的逐欄合成測試口徑）。

### 2. engine 烘焙解析（`core/engine-api`）
- 烘焙 `effective_fonts` 時（找 B1 的烘焙點）：theme 槽經該文件 `ThemeFonts`（`ThemeFonts::from_parts`；`OoxmlDoc` 若未持有就取一份，成本 KB 級）查表：
  - 槽解析出非空字型名 → **取勝寫入對應 `ascii`/`east_asia` 值**（theme 優先於直接值；`effective_*` 是展示投影，覆寫合法——註釋寫明）。
  - typeface 空字串（HC031 的 ea 就是）→ 視同查不到，**不覆寫**（老實退回直接值/None）。
  - 無 theme part／查不到 → 退回直接值（=現況行為）。
- 存檔位元組級不變鐵律不破（effective_* 是展示欄，passthrough 不經它）。
- 版面零變動：不動任何尺寸／座標計算（G1 的 `font_declaration_does_not_change_page_breaks` 同款守門測要加一條 theme 版）。

### 3. 零外溢
- `shells/`、`core/layout`、`core/render-trait` 不改。`tools/line-metrics` 只在編譯器因 `RunFonts` 新欄位報錯時補**最小**構造修改（`..Default::default()` 之類），禁止順手重構。

## 驗收標準（PM 逐項重跑）
1. `cargo fmt --all --check` 綠
2. `cargo clippy --workspace --all-targets -- -D warnings` 綠
3. `cargo test --workspace --no-fail-fast` 全綠（既有測試零改動）
4. `cargo run -q -p quire-cli -- roundtrip corpus --check` → `399 byte-identical 399 失敗 0`
5. `cargo run -q -p quire-layout --bin goldens` 三檔零位移
6. `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine` 綠
7. **版面零變動＋輸出變化對照**：改碼前先存基準
   `cargo run -q -p quire-cli -- render corpus/xopt/TestFiles_HC031-Complicated-Document.docx --out /tmp/g2-before`
   `cargo run -q -p quire-cli -- render corpus/poi/test-data_document_form_footnotes.docx --out /tmp/g2-before-fn`
   改完 render 到 /tmp/g2-after(-fn)：`pagebreaks.json` 兩檔 diff **必空**；SVG diff 只准出現「新增 font-family 屬性」型別變化（PM 會剝掉 font-family 再 diff 驗證）。
8. **新增測試（≥5，先紅後綠）**：
   (a) HC031：Heading 文字項 family＝"Calibri Light"（majorHAnsi 查表實檔）；本文（minorHAnsi）＝"Calibri"——**實檔直測，不是自造 XML**
   (b) 自造 fixture：同層 `w:ascii="Arial" w:asciiTheme="minorHAnsi"` 且 theme minor=Calibri → 選 **Calibri**（theme 取勝）
   (c) theme typeface 空字串（HC031 eastAsia 現成）→ 不覆寫；直接值存在時選直接值
   (d) 級聯逐欄：樣式鏈上層 eastAsia 直接值＋下層 asciiTheme → 兩欄都在（B1 逐欄口徑延伸到 theme 槽）
   (e) 舊快照相容：不帶 theme 槽的 RunFonts JSON 反序列化 OK＋新欄位 None 不外露
   (f)（鼓勵）theme 版分頁零位移測（G1 (f) 同款：宣告 asciiTheme 前後 page_breaks_json 等值）

## 禁区
- 不重寫 `ThemeFonts`（現成零件，只接線）；不碰 `theme.rs` 既有解析邏輯（需要新槽查表接口就加最小 pub 輔助）。
- 不碰 `core/layout`、`core/render-trait`、`shells/`、存檔路徑、`scripts/`、CI、corpus。

## 回報格式
- 改動檔案清單＋每檔一句話
- 驗收 1–7 實際命令與輸出摘要（未跑聲稱已跑＝失敗）
- 新增測試清單＋先紅後綠證據（紅時 panic 一行）
- 一句話自評：HC031 渲染後 SVG 新增了哪些 font-family 值（跑過才准回答）
