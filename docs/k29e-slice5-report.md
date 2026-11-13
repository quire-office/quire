# K29e（Slice 5）收案報告：編輯 API 字元格式補齊——底線／刪除線／上下標／highlight／字型族六欄進 RunDirectPatch

執行者：ow-core。基線 `74d22dda07e885e40763cf7165907d63cacdfd64`（main，K29d 已入庫）。
工作分支 `wt/t_4b55fc9d`，嚴禁 push，合併與 main 署名由 PM 執行。

## 交付物

- `core/engine-api/src/mutation.rs`：`RunDirectPatch`（宣告位 `pub struct RunDirectPatch`
  起 122 行）於既有五欄（bold／italic／color_rgb／size_pt／char_style）之外新增六欄：
  - `underline: Option<UnderlineKind>`（141 行；`UnderlineKind` 單欄承 single/double/
    dotted/dashed/none，`w:u/@w:val` 逐值對映，`as_u_val()` 單點維護）
  - `strike: Option<StrikeKind>`（145 行；單線 `w:strike`／雙線 `w:dstrike`／`None` 顯式關閉）
  - `vert_align: Option<quire_ooxml::VertAlign>`（151 行；baseline/superscript/subscript）
  - `highlight: Option<u32>`（163 行；命中 ECMA-376 十六命名色寫 `w:highlight/@w:val`
    與 Word 高亮 UI 互通，未命中回退 `w:shd` 底紋路徑——取捨註解在 `highlight_named_color`）
  - `font_family: Option<String>`（173 行；`w:rFonts` 的 ascii/hAnsi/cs/eastAsia 四欄同步
    寫入，同步策略註解在寫回處）
  全欄 `#[serde(default)]`，`None`＝不碰 XML；`Some(關閉值)`＝顯式移除宣告
  （寫 `w:val="0"`／`"none"`／`"baseline"` 形態，直接層贏過級聯）。
- `patch_rpr()` 落點：同檔 1495 行起。寫入紀律：先按同族 remove（`w:u`；
  `w:strike`＋`w:dstrike`；`w:vertAlign`；`w:highlight`＋`w:shd` 兩族一齊清；`w:rFonts`），
  再按 CT_RPr 官方順序（rStyle→rFonts→b→i→strike→…→u→…→vertAlign）重寫。
  全 `None` patch 硬閂（`patch == &RunDirectPatch::default()` 原字串返回）保證
  零注入、零位元組位移、不材料化空 `<w:rPr></w:rPr>`。
- `core/engine-api/src/lib.rs`：發射層下標臂（6210 行起，`is_super`／`is_sub`）——
  super/sub 共用 0.58× 字級縮尺；super 基線 −0.35×base（沿用既有常數零回退）、
  sub 基線 ＋base×(1.0/11.0)（=11pt 時 +1.0pt）。行高不膨脹（LO 真值，見下）。
  字體族宣告經 effective fonts 同時驅動發射 `font-family` 與排版度量（一體契約）。
  re-export `UnderlineKind`／`StrikeKind`／`VertAlign`。
- `core/engine-api/tests/k29e_char_format.rs`（新檔）：七盞紅燈考卷（斷言層口徑＝渲染端
  SVG 產物，display list 僅輔助）。
- `scripts/gen_k29e_fixture.py` ＋ `fixtures/k29e_char_format.docx`（zip STORE 位元組穩定）。
- `scripts/k29e_buggy_replay.sh`（新檔）：buggy／good 兩態回塞鑑別。
- `scripts/k29e_ab_zero_diff.py`（新檔）：全庫 A/B（base／head 雙 binary，全頁 dump 平行比對）。
- `docs/k29e-slice5-report.md`（本檔）。

## LO 真值探針（2026-11-13，Linux LO 25.8，pymupdf rawdict）

卡面前提「上下標會改行高」被實測推翻，照實回報：

- sup／sub 字級＝0.582×base（實作鎖 0.58，考卷容差 0.01）。
- sup 基線上移 −0.35×base；sub 基線下移 +1.0pt@11pt（=+0.0909×base）。
- 含 sup/sub 的行 auto 行距 pitch 恆 12.65pt——**不撐高**。故實作只動基線 y 與字級，
  layout 行高不動。卡面「上下標改行高」屬前提寫反，已記進
  `quire-redlight-conventions` skill 的 LO 真值節，請 PM 更正卡面措辭。

## 先紅後綠實跑

### 90 分鐘硬檢查點：紅燈已跑紅（lamp 3／4／7 紅）

初版考卷實跑 3 盞紅（lamp3 strike SVG 斷言、lamp4 跨段基線比對無效、lamp7 全 CJK
advance 尺差不可觀察）。以臨時探針（`k29e_probe_tmp.rs`，用畢即刪）dump 顯示列表與 XML
逐盞歸因後修正斷言口徑（段內同段基準、SVG `<text>` 行子串鎖、真行距因子收斂量），
非改方法論從零。

### 綠態（`cargo test -p quire-engine --test k29e_char_format`）

```
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
```

（七燈＋附錄 test 共 15 條；七盞燈名 `k29e_lamp1_six_fields_roundtrip_closed_loop` …
`k29e_lamp7_font_family_measure_and_emit_same_face` 全 ok。）

### BUGGY 回塞鑑別（`bash scripts/k29e_buggy_replay.sh buggy`）

主開關＝`patch_rpr` 內 `let buggy_k29e_fields = false;` 單行翻轉（六欄 remove 不碰六族、
add 不寫六欄、全 None 硬閂繞過）＋發射層下標臂退化（不縮尺＋不位移）：

```
[P1_K29E_FIELDS_BUGGY] 成功置換
[P2a_SIZE_BUGGY] 成功置換
[P2b_Y_BUGGY] 成功置換
test k29e_lamp1_six_fields_roundtrip_closed_loop ... FAILED
test k29e_lamp2_all_none_patch_keeps_part_byte_identical ... FAILED
test k29e_lamp3_underline_kinds_and_double_strike ... FAILED
test k29e_lamp4_super_sub_baseline_and_size ... FAILED
test k29e_lamp5_highlight_named_shd_and_width_match ... FAILED
test k29e_lamp6_removal_falls_back_to_style_cascade ... FAILED
test k29e_lamp7_font_family_measure_and_emit_same_face ... FAILED
test result: FAILED. 8 passed; 7 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

七盞全紅＝鑑別力完整（每盞都咬得住回塞）。

### 還原（`bash scripts/k29e_buggy_replay.sh good`）

```
[P1_K29E_FIELDS_GOOD] 成功置換
[P2a_SIZE_GOOD] 成功置換
[P2b_Y_GOOD] 成功置換
還原態與 HEAD 逐位元組一致（乾淨副本驗證通過）
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

好版副本＝HEAD（已提交態）；good 態末了以 `git diff` 對照 HEAD 自證逐位元組一致，
置換腳本與好版副本都在 `scripts/`（未只放 scratch）。

## 全閘門實測（每步落檔取自身退出碼）

| 閘門 | 命令 | 退出碼 | 實測輸出 |
|---|---|---|---|
| fmt | `cargo fmt --all --check` | 0 | 無輸出 |
| clippy -D | `cargo clippy --workspace --all-targets -- -D warnings` | 0 | Finished（零警告） |
| test | `cargo test --workspace` | 0 | passed 總和 **1008**、failed 0（基線 993 ＋本卡 15，只增不減） |
| roundtrip | `cargo run -q -p quire-cli -- roundtrip corpus --check` | 0 | `roundtrip --check: 399 檔（另有 1 檔隔離不計紅），byte-identical 399，失敗 0` |
| goldens | `cargo run -q -p quire-layout --bin goldens` | 0 | `✓ shaping.jsonl 相符（12 例）`／`✓ pagination.jsonl 相符（4 例）`／`✓ tables.jsonl 相符（5 例）`——三檔零位移 |
| wasm32 | `cargo check --target wasm32-unknown-unknown -p quire-odf -p quire-engine -p quire-web` | 0 | Finished |
| layering | `bash scripts/check-layering.sh` | 0 | `✓ 分層檢查通過（core↛shells；shells→engine 唯一入口；CRDT 僅在協作分塊；xlsx 不進首包）` |

基線 993 為實作前本 worktree 背景實測（`/tmp/k29e_baseline_test.log`，EXIT=0）。

## 全庫 A/B 零 DIFF（編輯卡證法）

方法：base binary（merge-base `74d22dda` 乾淨 source tree ＋獨立 CARGO_TARGET_DIR 編出）
vs head binary，`scripts/k29e_ab_zero_diff.py`，corpus 400 檔全頁 dump 平行（12 thread）
比對（剝 data_b64、浮點 round 4）。

```
corpus docx 檔數：400
DIFF 檔數：12
兩側同錯跳過：1（corpus/poi/...POIXWPFFuzzer-6733884933668864.docx，base/head 皆 CRC-32 毀損，非回退）
```

**DIFF＝12，非 0**——全部落在卡面預先豁免的「上下標屬改變正文流的合法位移」條款，
逐檔列 diff＋理由（無一檔出此集合；每檔的實際位移形態＝sub run 字級 11.0→6.38
（=0.58×）＋基線 +1.0pt，即本卡修掉的「下標全尺不位移」缺陷之修復面）：

| 檔 | subscript 宣告數 | 位移形態 |
|---|---|---|
| corpus/ooxmlexport/fdo77117.docx | 12 | sub 段 size 11.0→6.38、y +1.0 |
| corpus/ooxmlexport/fdo78420.docx | 7 | 同上 ×3 段（跨 3 頁）＋縮尺後行內 x 回填 |
| corpus/ooxmlexport/floatingtbl_with_formula.docx | 2 | 同上 ×2 段＋行內 x 回填 |
| corpus/ooxmlexport/tableCurrupt.docx | 6 | 同上 ×5 段＋行內 x 回填（items 同序同文，僅幾何欄位移） |
| corpus/ooxmlimport/tdf121804.docx | 3 | sub 段 size/y ×1 |
| corpus/phpword/samples_resources_Sample_11_ReadWord2007.docx | 1 | sub 段 size 12.0→6.96、y +1.09＋x 回填 |
| corpus/phpword/samples_resources_Sample_23_TemplateBlock.docx | 1 | 同上＋行內 Rect x 隨行內元素位移 |
| corpus/phpword/tests_PhpWordTests__files_documents_reader-styles.docx | 1 | sub 段 size 10.0→5.8、y +0.91 |
| corpus/phpword/tests_PhpWordTests__files_documents_reader.docx | 1 | 同 Sample_23 形態（同模板源檔） |
| corpus/phpword/tests_PhpWordTests__files_templates_clone-delete-block.docx | 1 | 同 Sample_23 形態 |
| corpus/poi/test-data_document_bug59058.docx | 8 | sub 段 size/y ×4＋x 回填 |
| corpus/selfgen/two_columns_inline_effects.docx | 1 | sub 段 size 11.0→6.38、y +1.0＋x 回填 |

歸因佐證（`/tmp/k29e_subset.txt`）：全庫宣告 `w:val="subscript"` 的檔共 14，DIFF 集合
⊆ 該集合成立且補集恰為 2 檔——`NumberedList.docx` 與 `tdf103976.docx` 的 subscript 宣告
只存在 `word/styles.xml` 的 `Subscript` 樣式本體、正文 `rStyle/Subscript` 引用 0 次，
故發射層無 sub run、零位移。**DIFF 檔集合＝實際發射 subscript 的檔集合，逐檔吻合，
無一檔可歸因於「編輯層代碼洩進未編輯路徑」**：六欄寫入路徑由 patch 明確指定才動
XML（A/B 全程無任何編輯呼叫，12 檔位移全部由發射層下標臂——既有解析旗標的消費修正——
產生，goldens 三檔零位移亦旁證 layout 未動）。superscript 檔零 DIFF（既有 sup 臂行為
原樣保留）。未編輯檔案不經 `patch_rpr`，該函數於 A/B 全程零呼叫。

## git diff --name-only main...HEAD

```
core/engine-api/src/lib.rs
core/engine-api/src/mutation.rs
core/engine-api/tests/k29e_char_format.rs
fixtures/k29e_char_format.docx
scripts/gen_k29e_fixture.py
scripts/k29e_ab_zero_diff.py
scripts/k29e_buggy_replay.sh
```

## 誠實聲明與殘留

- `shells/web/editor/` 零觸碰（禁區）；編輯器 selfcheck 對 `RunDirectPatch` 欄位數無計數
  斷言（`wordui_u1_selfcheck.mjs` 只數 pending 鈕、u3 只驗 color/size 語意），故無需改計數，
  照實點名。22 顆 `pending:K29e` 鈕未動——轉 live 屬誠實性卡與接線卡職責。
- highlight 移除的第四態不存在：解析層把 `w:highlight w:val="none"` 讀成未宣告，故移除
  直接層高亮後樣式級高亮重新顯形——燈 6 照實斷，不虛構語意。
- 上下標行高前提被 LO 探針推翻一事請 PM 更正卡面；本報告與 skill 已記真值。
- 四枚 commit 全在 `wt/t_4b55fc9d`，未 push；署名專案設定生效。
