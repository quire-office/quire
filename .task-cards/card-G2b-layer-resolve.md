# 任務卡 G2b：theme 查表改「逐層解析」（修復 G2 跨層覆寫 bug）

## 你的角色與紀律（違反＝任務失敗）
- 你在 git worktree `/home/jerry/projects/quire/.worktrees/oc-fonts2`（branch `feat/theme-font-refs`）工作，工作區已有 G2 的未提交改動——**在其上修復**，不是重來。先 `pwd` 確認。
- **嚴禁** `git commit` / `git push` / `git checkout` / `git reset` / `git stash`。改動留工作區，PM 逐行 review。
- 所有註釋／測試文案**繁體中文**（禁簡體）。風格對齊 repo 慣例。

## PM 已定案的根因與修法（先讀懂再動手；設計已裁決，不要另闢蹊徑）
G2 現行實作：`RunFonts` 帶 `ascii_theme`／`east_asia_theme` 槽 → 級聯 → engine 烘焙點 `resolve_theme_fonts` 查表覆寫。**錯在查表時機**：HC031 實測 docDefaults 宣告 `asciiTheme="minorHAnsi"`（→Calibri），run 直接 rPr 宣告 `ascii="Wide Latin"`——級聯後基底帶著未解析的 minorHAnsi 槽，烘焙查表把 **Wide Latin 覆寫成 Calibri**（G1 本來的正確輸出被 retroactively 改壞；PM 已用 `unzip` 查證 Wide Latin/Times New Roman/Consolas 就在 run 層 rPr）。
**ECMA-376 §17.3.2.26 的 theme 取勝只適用於同一 `w:rFonts` 元素內**（`w:ascii` 是不支援主題字型軟體的回退值）；跨層就是普通逐欄級聯，下層直接值必須赢上層的 theme 引用。

**定案修法：查表移到「每層解析時」**——每個 `w:rFonts` 元素在被存入任何一層（docDefaults／樣式自帶／run 直接 rPr）的**當下**就經 fontScheme 查表，把 theme 引用落地成直接值；**級聯系統裡不再存在未解析的 theme 槽**。這樣「同層 theme 取勝／跨層直接值取勝」由既有逐欄級聯天然正確。

## 具體改動（機械式，但時機一點不能錯）
1. **`core/ooxml/src/styles.rs`**：
   - `RunFonts` **刪除** `ascii_theme`／`east_asia_theme` 兩欄（G2 加的；解析當下就落地，級聯不再需要槽）。serde 相容疑慮隨之消失。
   - `rfonts_of(e)` 改為 `rfonts_of(e, tf: Option<&ThemeFonts>)`（或等價）：元素**自身**同時有 `w:ascii` 與 `w:asciiTheme`→查表成功者覆蓋 `ascii`；查不到（無 theme part／未宣告／typeface 空白）→ 保留直接值（回退語義）。`eastAsia`／`eastAsiaTheme` 同理。查不到且直接值也沒有→該欄維持 None。`is_some()` 守門（空元素不物化）語義不變。
   - 所有**存入點**接上 `tf`：`apply_style_tag` 的 `b"rFonts"` 臂、`parse_rpr_body`，以及沿 `theme: &ThemeColors` 已串好的函式鏈**平行加傳** `tf: Option<&ThemeFonts>`（對照 ThemeColors 現成走法；呼叫點編譯器會全點出來）。
   - `StyleTable::parse_with_theme` 加 `tf` 參數（或新增 `parse_with_fonts`，舊簽名保留為傳 `None` 的 shim，既有測試調用點零改動）。
2. **`core/ooxml/src/lib.rs`**：`parse()` 內部已有 `ThemeColors::from_parts(&parts)`——同處取 `ThemeFonts::from_parts(&parts)` 傳入 `parse_document_xml`／`parse_header_footer_part`（平行加傳，同 ThemeColors 走法）；run 直接 rPr 的 `rfonts_of` 呼叫點（約 413/608/731 行附近）接上 `tf`。`FontSchemeSlot` 若不再需要 pub re-export 就收回（`ascii_theme_slot` 等映射留在 theme.rs 內部用）。
3. **`core/engine-api/src/lib.rs`**：**撤除** G2 的 `resolve_theme_fonts` 與其調用、`theme_fonts` 傳參鏈（`cascade_paragraphs`／`build_hf_layer` 回復原簽名）——查表已在 ooxml 解析層完成，engine 不再需要 `ThemeFonts`。`select_text_family` 零改動（G1 原樣）。
4. **既有測試調適**：G2 寫的 `theme_slots_inherit_per_field_in_cascade`（斷言 `ascii_theme` 槽存在）語義已過時——改寫成新架構的等價斷言：上層 docDefaults `eastAsia="標楷體"`＋`asciiTheme="majorHAnsi"`、下層樣式 `asciiTheme="minorHAnsi"` → 結果 `ascii`＝minor 查表值（下層**同層解析**覆蓋上層）、`east_asia`＝標楷體（逐欄繼承不破）。`run_fonts_theme_refs_are_backward_compatible` 隨欄位刪除簡化為「RunFonts 舊 JSON 照舊可讀」（若已無新欄可測就刪該測並說明）。
5. **新增回歸測（先紅後綠——動工前先跑給 PM 看紅）**，engine 層：
   (a) **HC031 直接值守土**（本次 bug 的正主）：`family_on_pages(engine, "Video provides a powerful way to help you prove your point.")` == "Wide Latin"；"You can also type a keyword..." == "Consolas"；p3 數學行 "x+an=k=0nnkxkan-k" == "Cambria Math"。改動前必紅（現被覆寫成 Calibri）。
   (b) **同層 theme 取勝**（G2 (b) 測保留）：同元素 `w:ascii="Arial" w:asciiTheme="minorHAnsi"`＋theme minor=Calibri → Calibri。
   (c) **跨層直接值取勝**（新語義鎖）：docDefaults `asciiTheme="majorHAnsi"`（→Calibri Light），run 直接 `w:ascii="Arial"` → **Arial**（不是 Calibri Light——本次 bug 的一般化鎖鏈）。
   (d) **theme 引用仍生效**（G2 (a) 測保留照跑）：HC031 無直接 rPr 的本文文字 → Calibri；text box run → Calibri Light。

## 驗收標準（PM 逐項重跑＋獨立剝除驗證）
1. `cargo fmt --all --check` 綠
2. `cargo clippy --workspace --all-targets -- -D warnings` 綠
3. `cargo test --workspace --no-fail-fast` 全綠（除第 4 點明列的 G2 調適測外，既有測試零改動）
4. `cargo run -q -p quire-cli -- roundtrip corpus --check` → `399 byte-identical 399 失敗 0`
5. `cargo run -q -p quire-layout --bin goldens` 三檔零位移
6. `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine` 綠
7. **版面零變動＋值級對照**（PM 已存好 /tmp/g2-before*，勿動）：render HC031 與 POI footnotes 檔到 /tmp/g2b-after*；`pagebreaks.json` diff 必空；**PM 專用驗法**：before 與 after **兩側都**剝除 font-family 屬性後 diff 必空（只准新增/改值 font-family），且 p0 必須同時含 `Wide Latin`／`Times New Roman`／`Consolas`（直接值回魂）**與** `Calibri`／`Calibri Light`（theme 生效）。
8. 回報：紅燈證據（(a)(c) 改前 panic 原文）＋HC031 SVG font-family 值分布（uniq -c 實跑輸出）。

## 禁区
- 不動 `select_text_family`（G1）、`ThemeFonts` 解析（theme.rs 既有邏輯）、`core/layout`、`core/render-trait`、`shells/`、存檔路徑、`scripts/`、corpus。
- 不新增 serde 欄位（本卡是**減欄位**）。

## 回報格式
- 改動檔案清單＋每檔一句話（engine 撤了哪些 G2 加的東西要列清楚）
- 驗收 1–8 實跑命令與輸出摘要
- (a)(c) 紅燈證據原文
