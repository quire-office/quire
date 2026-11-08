# ODF-D1 解剖報告：ODT 顯示列表發射層（`OdfProjection` → `PageDisplayList`）

考卷：`core/odf/tests/odf_d1_display_list.rs`（四盞燈＋一條附加鎖）
實裝：`core/odf/src/display.rs`
CLI：`quire-cli odt-dump <file.odt> [頁面 n]`（`shells/native/src/main.rs`）
契約基準檔：`fixtures/display_list_contract.json`（**只能**由
`scripts/odf_d1_contract_probe.py` 從 DOCX 線真實 `dump` 產物生成）
DOCX 側反查：`shells/native/tests/odf_d1_dump_contract.rs`（三盞）
紅燈紀錄：`docs/odf-d1-redlight.md`

---

## 一、卡面前置實測（逐條核對，PM 開檔可驗）

| 卡面宣稱 | 實測 | 判定 |
|---|---|---|
| `core/odf` 全庫 grep `display_list` 零命中 | `git grep -c 'display_list\|PageDisplayList' main -- core/odf` → 無輸出（main 線零命中） | 屬實 |
| ODT 線只有文字流投影 | `project()`→`OdfProjection`→`page_break_positions()`；`shells/native` 的 `odt_render` 只輸 count/len/breaks/page/page_source | 屬實 |
| `OdfLayoutParagraph` 帶 `font_size_pt()`／`bold()` | `layout_glue.rs:63,71`（逐 run `(CpRange, size_pt, bold)`，檔頭註明「粗體只供渲染層未來消費」） | 屬實 |
| `layout_projection` 已有逐段逐行幾何 | 走 `quire_layout::layout_paragraphs_grid`，回 `Vec<ParagraphLayout>`（`Line{text,width_pt,para_index,char_offset,col}`） | 屬實 |
| 分層鐵律：嚴禁引用 `quire_ooxml::Paragraph` | `layout_glue.rs` 檔頭第 4-5 行；本卡 `display.rs` 零 ooxml 型別 | 遵守 |

**卡面未提、實查改變解法的兩件事**（詳見 §3、§5）：
1. `PageDisplayList`／`DrawItem` 的**定義處不是 engine-api**，是
   `core/render-trait/src/lib.rs`（§89 起），engine-api 本身也只是 `pub use quire_render_trait::{...}`
   re-export。故卡面擔心的「型別在 engine-api 而 odf 不能依賴」根本不成立——
   本卡走**向下邊**（odf→render-trait，render-trait 零 core 依賴、只吃 serde），
   零新層級邊、零型別複製。
2. main 在本卡期間併入 **ODF-C4**（`page_geometry`：`style:page-layout`＋master-page
   鏈）。發射層因此**不**用 `LayoutParams::default()`（假 A4＋1 吋），改吃
   `page_geometry(...).to_layout_params()`，與 `odt-render` 同口徑。

## 二、DOCX 線 JSON 契約逐欄解剖（真相來源＝真實 `dump` 產物，非讀碼猜）

抽樣四檔（`fixtures/sample.docx`／`table_mixed`／`hf_table_header`／`inline_image`），
實測 7 頁／229 項，四變體齊備。頁層欄位：`page`／`width_pt`／`height_pt`／`items`。

座標系（render-trait 檔頭＋`shells/web/demo/main.js:335` 消費端雙向確認）：
原點＝頁左上角，單位 **pt**，y 向下為正；`Text.y`＝**行網格帶頂端**（渲染端自加
字級得基線：`baselineY = (t.y + t.size_pt) * scale`）；`Text.x`＝版心左緣＋行內位移。
z 序＝push 序（非文字組先、文字項後；`sort_display_items` 約定）。

| 契約欄位（變體.欄位） | 語意 | ODT 線有無對應資料 | 本切片處置 |
|---|---|---|---|
| `page`／`width_pt`／`height_pt` | 頁序與頁面尺寸（pt） | **有**（C4 幾何鏈） | 老實發：`pg.index`／`pg.width_pt`／`pg.height_pt` |
| `items` | 項陣列（externally-tagged enum，鍵＝變體名） | 有 | 只發 `Text` |
| `Text.x`／`Text.y` | 項原點 | **有**（`pg.margin_left_pt`＋行網格 `line_bands[i]×line_h_pt`） | 發（考卷燈 2/3 逐行對照） |
| `Text.text` | 項文字 | **有**（`OdfProjection.paragraphs[].text`，經 `Line`） | 發（逐 run 同質段） |
| `Text.size_pt` | 字級 | **有**（逐 run `fo:font-size` 級聯，C3） | 發 |
| `Text.flags` | 粗/斜/底/刪位元（1/2/4/8） | **粗體有**；斜體、底線、刪除線**未採集** | 只帶 `FLAG_BOLD`；其餘不發（＝0，D1b） |
| `Text.color_argb` | 0xAARRGGBB | **無**（`fo:color` 未採集；`StyleRecord` 只有 size/weight） | 中性值 `0xFF00_0000`（不透明黑，同 render-trait `default_opaque_black`）→ D1b |
| `Text.family` | 字型族名（`Option<String>`，`skip_serializing_if`） | **無**（`style:font-name` 未採集） | `None`＝serde 跳欄，渲染端用自身字型（Canvas2D 的 "Noto Sans TC"）。**不捏造族名** |
| `Rect.fill_argb`／`w`／`h` | 填色／佔位框 | 部分（表格底圖、佔位框） | **不發**（圖片／表格＝D1b；分頁框也**不**發實體 `Rect`，見 §4） |
| `Rule.*`（`x1,y1,x2,y2,width_pt,color_argb,dash`） | 筆畫（表格框線等） | 段落框線 `fo:border` **未採集**（`StyleRecord` 無 border 欄） | **不發**（卡面「若可得段落框線」實測**不可得**→ D1b） |
| `Image.*`（`mime,data_b64,alt,duotone`） | 內嵌圖片像素 | 有 zip 媒體但解碼／座標定位全缺 | **不發**（D1b） |

缺件欄位總帳（**發射時填中性值**者僅 2 個，且都有防捏造紀律）：
`color_argb`→不透明黑（DOCX 線自身 `serde default` 即此值，零判別差異）、
`family`→`None`（跳欄＝渲染端預設，DOCX 線舊 JSON 同形）。其餘缺件一律**不發該項**，
不用中性值假裝有（假框／假圖＝度量假綠，render_lab 教訓）。

## 三、發射面覆蓋（本切片＝只發文字項，其余記帳 D1b）

發：`Text`（逐 run 同質段）。
不发（逐項寫死在 `display.rs` 檔頭＋考卷燈 1 的變體斷言，**不準偷偷擴張**）：
`Rect`（含分頁框）、`Rule`（含段落框線）、`Image`、頁首尾、註腳、表格、嵌入物件
（E2e 的 `object_paragraphs` 是**獨立帳**，本切片不消費——物件文字無宿主頁內座標，
發它必須先有錨點幾何，屬 D1b）。

**分頁框為何不發實體項**：卡面寫「文字項＋分頁框」。實查 `shells/web/demo/main.js`
的繪頁方式＝按 `paper_json`／頁 `width_pt`×`height_pt` 自繪頁面矩形，DOCX 線也
**從不**把紙邊發成 `Rect` 項（真實 dump 的 `Rect` 只來自表格底圖與圖片佔位）。
跟著發一個滿頁 `Rect` 會（a）與 DOCX 線發射口徑不一致、（b）蓋掉渲染端自己的
紙面樣式、（c）在 render_lab 內容軸憑空多出 1 項。故本卡「分頁框」的實作＝
**頁層 `width_pt`／`height_pt` 欄位**（渲染端據此画框），非 `Rect` 項。此為對卡面的
一处**主動收窄**，寫明在此供 PM 裁。

## 四、幾何同源性（發射器不得自烤座標）

- 斷行：`layout_projection`（ODF-C3 既有入口，逐段代表字級為斷行尺）——**不改**。
- 分頁：`quire_layout::paginate_per_section(&pls, &params, &[])`（無 section＝單一本
  幾何；`footnote_reserve`＝0，因註腳屬 D1b）。
- 行 y：`pg.margin_top_pt + pg.line_bands[i] × params.line_h_pt()`——用 `line_bands`
  而非陣標（表格後文字行的帶索引≠陣標；ODT 現況無表格，兩者在燈 3 斷言下等值，
  但口徑必須先對，否則 D1b 加表格時會錯位）。
- 項 x：行首＝`pg.margin_left_pt`（**每行重置**——續排行不接在上行行尾之後，這是
  與 DOCX 線 `emit_line_run_items` 同款口徑）；行內同質段以 `TextMeasurer::new(size).text_width`
  累積推進（同一內嵌字型尺＝雙殼一致，見 `shaping::EMBEDDED_FONT`）。
- 分頁斷點：發射器**不另算**——`page_break_positions`（全域尺）與 `breaks_from_pages`
  （分頁器實際頁）兩條路都在燈 3 裡與發射結果對照，不一致即咬紅。

## 五、分層取捨（卡面明定「報告要寫明取捨與替代方案後擇一」）

待解問題：發射層要用 `PageDisplayList` 型別，能不能依賴？

| 方案 | 依賴邊 | 評 |
|---|---|---|
| **A（擇此）** odf → `quire-render-trait` | 新邊，但 render-trait 是契約**定義處**（`core/render-trait`：零 core 依賴、只吃 serde；CI 已把它與 quire-odf 並列入 wasm32 雙目標閘門） | 向下的「契約型別」依賴，非跨線借模型。`layout_glue` 鐵律禁的是 **`quire_ooxml::Paragraph`（DOCX 內容模型）**，本邊不碰 ooxml 半字。零複製、零同步成本，兩線 JSON 由同一 `serde` derive 決定，**結構性不可能各烤一套** |
| B odf 內另定義同 schema 的 serde 結構 | 零新邊 | 兩套型別靠人工同步，正是卡面要防的「各烤一套欄位」；且 `DrawItem` 有 4 變體＋serde default 細節，複製即腐化。**拒** |
| C `PageDisplayList` 下沉到更底 crate | 移動既有型別 | 無需——它已经在最底層（只依賴 serde）。**多餘動作，拒** |
| D odf → `quire-engine`（engine-api） | **向上**邊 | 違反 `check-layering.sh` 精神（core 不得依賴 shell/engine；engine 在 `CORES` 清單內），且會把 ooxml 模型拖進 odf。**拒** |

`quire-odf` 新增 `serde_json`（normal 依賴）：只为序列化輸出（`display_list_json`／
`dump_display_lists_json`）。卡面「不准用字串拼接造 JSON」照辦——輸出型別就是契約
型別，`serde_json::to_string(&PageDisplayList)`，考卷附加鎖做 round-trip
（`serde_json::from_str::<PageDisplayList>` 還原後與原結構 `assert_eq`），拼接無法過此鎖。

**隨車補的閘門**：`scripts/check-layering.sh` 的 `CORES` 清單原本**不含** `quire-odf`
（C2/C3 時期 odf 只依賴 ooxml＋layout，沒有需要守的新邊）。本卡為 odf 開了
odf→render-trait 這條新邊，若閘門不-cover 它，日後有人從 odf 伸手拿 ooxml 內容模型
或 shell  crate 就不會咬紅。故把 `quire-odf` 加入 `CORES`（只擋「core 依賴 shell」
方向），這是本卡唯一動到的既有閘門檔。

### engine-api 那條新邊：本卡**不做**，理由與替代方案

卡面 item 3 條件式要求「若引擎入口需先補 `Engine::from_odt_bytes`，本卡一併做」。
實查後判定**不需要新增 engine 邊**：

- `Engine` 的建構面是 docx 中心（`from_docx_bytes`／`from_docx_bytes_with_paper`），
  內部持有 `LayoutEngine`＋`paras_eff`（run 級可編輯投影）＋CRDT 失效事件流。ODT
  目前**唯讀投影**（無 run 級可編輯模型、無 rels、無 styles part 級回寫），硬塞進
  `Engine` 会得到一個「只能讀的 Engine」——半個契約、兩條極性，比不加更壞。
- 替代方案（已做）：`OdtView`（`core/odf/src/display.rs`）——開檔一次、投影一次、
  分頁一次，暴露 `page_count()`／`page_display_list_json(page)`／`dump_json()`／
  `paragraph_count()`／`total_text_len()`／`params()`。wasm 邊只需 `bytes`，零 fs。

ODF-D2（網站卡）接 `new_odt(bytes)` 的兩條合法路線，擇一即可（本卡兩者都不堵）：
1. **`shells/web` 直接加 `WasmOdtView`**（`#[wasm_bindgen]` 薄包 `OdtView`），
   `shells/web` 新增 `quire-odf` 直接依賴。可行性已實測：`check-layering.sh` 對
   `quire-web` 的禁列是 `quire-(ooxml|xlsx|layout|render-trait|crdt)`，**不含
   quire-odf**；CI 已有 `cargo check -p quire-odf --target wasm32-unknown-unknown`
   （本卡新增的 render-trait 邊同時在該閘門內）。JS 端繪圖碼零改（同 schema）。
2. **`Engine` 開 `odt` feature**（比照 `xlsx` 拆包紀律：預設不帶、主 wasm 首包不含），
   代價是必須先定義「唯讀 ODT 段」在 `Engine` 契約裡的地位（不可編輯時
   `replace_paragraph_text`／caret 面如何報錯），比本卡範圍大。
PM 若要求走 2，建議單開一張卡做「Engine 唯讀檔型契約」再並入，不要在 D2 內順手做。

## 六、附加鎖：CLI 面同形（`odt-dump` ↔ `page`/`dump`）

`shells/native/tests/odf_d1_dump_contract.rs`：燈 c 斷言 `page f.docx 0` 與
`dump f.docx[0]` 產物**逐字相同**，即 odt-dump 對齊的「同形」有單一定義；燈 a/b
從出貨 `dump` 反查契約基準檔（聯集全等＋必要欄位交集全等，四變體逐一），基準檔
若與出貨產物脫節立刻咬紅——這是防「ODT 對到一張過期基準圖」的第二雙眼。

## 七、驗收閘門對應

| 閘門 | 對應 |
|---|---|
| FMT／CLIPPY -D／TEST(workspace)／RT＋本卡考卷 | `bash ~/.hermes/scripts/ow-gate.sh <worktree>`（八閘） |
| `cargo test -p quire-odf` 全綠 | 6＋5＋5＋2＋9＋1＋7＝35 盞（含 C3 12 盞、E1 六盞燈、C4 九盞零回退） |
| wasm32 雙目標（CI） | `cargo check -p quire-odf --target wasm32-unknown-unknown` 已在 CI；新增邊只帶 serde |
| render_lab 零回退板六檔分數不動 | 本卡**未碰** `scripts/render_lab.py`／`core/engine-api`／`core/layout`／DOCX 發射層，DOCX 線 golden JSON 零位移（燈 4＋`goldens` 閘門雙重守） |
