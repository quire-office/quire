# E2c 頁首直排框（header1.xml `wps:bodyPr vert="vert270"`）紅燈快照與實作紀錄

署名：ADT109119（worktree `wt/t_9603db22`，基線 `fdeaf5c`）

## 1. 紅燈復現與 LO 真值

`corpus/ooxmlexport/fdo78957.docx`（修前實測，`python3 scripts/render_lab.py`）：
內容 0.571／版面 0.0／真缺字 1（`Quire`），notes
`line_direction_mismatch:LO直排我方橫排`；我方出貨 SVG 只有兩個灰 Rect，
`TextBox 3` 框連框帶字完全不發（顯示列表僅 `test` 一條）。

LO 真值（pymupdf `dir`＋`bbox`，口徑同 `docs/e2-redlight.md` §2；數值由
`scripts/e2c_lo_truth.py` 走 render_lab 同一條 LO 轉換路徑實跑取得，可重跑覆核）：

```
page=0 text='Quire' dir=[0.0, -1.0] bbox=[561.6, 380.0, 577.1, 411.9] size=14.0
```

## 2. 根因三缺口（接手實測修正卡面假設；詳見卡 comment #224 後續）

解析端（`core/ooxml/src/drawing.rs` bodyPr 解析）沒斷——探針實測
`read_images(word/header1.xml)` 已回 `vert=Some("vert270")`。斷在三處：

- **A 發射路徑**：頁首發射層 `push_hf_slot` 只把 `part.images` 當 inline 圖發在
  版心左緣，`wp:anchor` 浮動框（`floating`）一概不理 → 框與框內文字零發射。
  新臂 `Engine::push_hf_vert_frames`（守門三條件同中：`floating`＋
  `wps_annotation`＋`@vert`＝vert270 族；VML `layout-flow:vertical` 屬鐵條禁區
  不進本臂，由 lamp3 鎖死）。
- **B 錨定/尺寸解析**：`InlineImage` 無 `wp14:sizeRelH/V` 欄位、`pos_x_pct`
  解析了但發射層從不換算。新增純事實欄位（`size_rel_*_pct`／`size_rel_*_rel`／
  `body_anchor`，serde default＋空值不外露＝舊投影序列化逐位不變），換算全在
  發射層 `hf_anchor_rect`（pct 覆寫 extent；`@relativeFrom` page＝整頁、
  margin＝版心；位置三式 pctPos→align→posOffset 輪替，解不出＝老實不發射）。
- **C 文字內容**：`w:txbxContent` 字面 `Bull on the Run` 包在 `<w:sdt>` 內帶
  `w:dataBinding w:xpath="/ns0:Properties[1]/ns0:Company[1]"`，LO 覆寫成
  `docProps/app.xml` 的 `<Company>Quire</Company>` 再排版。採集端補
  `txbx_databinding` 事實（**自閉合標籤走 on_empty 流**，只收 Start 會全漏——
  K17 同坑）；語意端 `hf_bound_text` 取 xpath 末段元素名查 docProps，
  查不到退回字面值（絕不虛構），且只覆寫**有字**段的顯示文字（空段不憑空冒字）。

## 3. 幾何定律（`docs/e2-vert-truth/` probe 反解，非單檔調參）

vert270＋`bodyPr/@anchor="t"`＋預設 inset（lIns/rIns=7.2、tIns/bIns=3.6）：

- 文字軸映射（probe 鐵證）：advance→ −y（上），next-para→ +x（右）。
- 進位軸（y）可排區 `[fy0+rIns, fy1−lIns]` 中點 395.9；LO 行 bbox 中點
  395.95 ⇒ 命中（吃框內段落 `w:jc="center"`）。
- 欄軸（x）anchor=t ⇒ x0=fx0+tIns=560.4（LO 561.6，Δ1.2）；col_w=E2 定律
  `sz*1.22`=17.08 ⇒ x1=577.5（LO 577.1，Δ0.4）。全部落在卡面 ±4pt 內。

我方發射實抓（`quire-cli page` 頁 0）：

```
Text{x:560.52, y:413.89, size:14.0, flags:16(FLAG_DIR_VERT270), text:"Quire"}
```

`LineFlow::Vertical270.compute_line_bbox` 命中 LO `[561.6,380.0,577.1,411.9]`
各邊 ≤4pt；出貨 SVG 帶 `rotate(-90 574.5 413.9)`（E2b 像素臂自動消費旗標，
本卡零觸碰 `shells/native/src/render_svg.rs`）。

## 4. 綠燈實測

1. `cargo test -p quire-engine --test e2c_header_vert`：4/4 綠（lamp1 發射存在、
   lamp2 dir 向量＋bbox ±4pt＋不重發、lamp3 n779627 零 DIR 旗標、lamp4 橫排
   `test` 項幾何逐位不變 (90.0, 844.1, 14.0)）。
2. `render_lab.py fdo78957.docx`：內容 **1.0**／版面 **1.0**／真缺字 **0**
   （由 0.0／1 轉綠；色 0.0 屬 J3 色軸既有缺口，非本卡範圍）。
3. 零回退：fdo76591 1.0/0.98（色 0.8 圖 0.721 真缺字 `1st`/`2nd`＝D 系列既有）、
   WC048 1.0/1.0、n751077 p1 0.88/0.867（p2 None）、99_Fields 1.0/1.0——
   與 `docs/d5-redlight.md`／`docs/e2b-redlight.md` §6.2 基線逐檔逐軸一致，
   無 >0.02 抖動。E2 五盞＋E2b 考卷全綠；`cargo test --workspace` 813 全綠；
   八閘 `GATE_SUMMARY fail=0`（worktree 重跑）。

## 5. 第二檔實證（禁單檔調參）

`scripts/e2c_survey_hf_vert.py` 全庫普查（頁首／頁尾 part 的 ST_TextVerticalType
全列＋VML `layout-flow:vertical`；**注意 `vert270` 含數字，字元類漏 `0-9`
會整列漏收**——本卡普查初版踩過，已修）全庫命中兩檔：

```
ooxmlexport/fdo78957.docx :: word/header1.xml :: bodyPr@vert=vert270, vml layout-flow:vertical
ooxmlimport/n779627.docx :: word/footer1.xml :: vml layout-flow:vertical
```

- `fdo78957`：本卡目標檔（DrawingML vert270 臂）。
- `n779627`：純 VML `layout-flow:vertical`，LO 實測全橫排（a–e 五行 dir=[1,0]、
  頁尾三框零文字）⇒ lamp3 鎖「新直排臂只認 DrawingML `@vert`，VML 臂不觸發」。

兩檔即全庫命中全集（含目標檔本身），直排臂發射面被語料窮盡。

## 6. 橫排 SVG byte 級零擾動

基線 `fdeaf5c`（不含本卡改動）vs 分支 HEAD 全庫 400 檔各自 render 成 SVG
逐檔 sha256 對照（口徑同 `docs/e2b-redlight.md` §6.3）：

```
CHANGED corpus/ooxmlexport/fdo78957.docx (pages 1->1)
total docs=400 changed=1 errors=0
```

**唯一變化＝目標檔 fdo78957**（本頁首直排框發射＋`rotate(-90)` 像素臂消費），
其餘 399 檔 SVG 逐位元組相同 ⇒ 橫排 byte 級不變鐵條成立。
另 fdo76591 真缺字 `1st`/`2nd` 經基線/分支雙側 dump 比對確認**基線同缺**
（既有缺口，非本卡回退）。

## 7. 範圍與殘留

- 觸碰面：`core/ooxml/src/drawing.rs`（bodyPr/@anchor、sizeRel 族、dataBinding
  事實採集）、`core/engine-api/src/lib.rs`（頁首發射層直排臂＋換算＋綁定值
  語意）、`core/engine-api/src/table_map.rs`（結構體新欄位測試補缺省）、
  `core/engine-api/tests/e2c_header_vert.rs`、`docs/`、`scripts/`（普查與真值
  腳本）。嚴禁區（render_svg.rs／VML 狀態機／odf／D9b 幾何／E1 分頁）零觸碰。
- 殘留（PM 裁定項，未硬改）：本臂只吃 vert270（vertStacked／eaVert／
  mongolianVert 族在頁首 part 全庫零命中，如實不發）；`hf_bound_text` 是
  docProps 末段元素名查表，非通用 XPath（fdo78957 唯一實據形體，查不到
  退回字面值）；橫排側 dataBinding 覆寫（run166 普查：全庫另有 28 檔帶
  dataBinding）屬另立新卡範疇，本卡未觸碰。
