# ODF-D2 紅燈紀錄（wasm 殼 ODT 入口：`WasmOdtView`）

卡號：t_89c4a196（PM 11-08 定案路線 1：`shells/web` 直依賴 `quire-odf`，**不走
`Engine` 邊**——理由見 `docs/odf-d1-anatomy.md` §5「engine-api 那條新邊：本卡不做」）

考卷：`shells/web/tests/odf_d2_wasm_view.rs`（host target 七盞）＋
`shells/web/tests/odf_d2_node_smoke.mjs`（實際 wasm 產物八面，`npm run test:odt`）
前置：ODF-D1（merged main `b4e833d`）的 `core/odf/src/display.rs::OdtView`
基準檔：`fixtures/display_list_contract.json`（零新增基準，沿用 D1 探針產物）

## 〇、卡面勘誤（實檔為準，先行記帳）

卡面考卷指定 `corpus-odt/lo_core_sw_qa_odfimport_WordTest_edit.odt`——**該檔不存在**。
`corpus-odt/manifest.json` 記的上游來源是 LibreOffice **odfexport** 目錄
（`sw/qa/extras/odfexport/data/WordTest_edit.odt`），倉庫實檔為
`corpus-odt/ext/lo_core_sw_qa_odfexport_WordTest_edit.odt`，且 ODF-D1 燈 1 用的正是
這檔。故本卡考卷照 D1 用 `ext/lo_core_sw_qa_odfexport_WordTest_edit.odt`
（6,496 bytes，D1 帳：13 段／402 碼點）。

## 一、紅燈方式：佔位空殼拍紅（比照 ODF-C4「佔位實作」紀律）

本卡屬「新增入口面」——`WasmOdtView` 從零建立，不存在「先寫錯再改對」的自然紅燈。
故紅燈用**佔位空殼**拍：考卷檔先入庫，`shells/web/src/lib.rs` 的 `WasmOdtView`
以佔位態入庫（`new` 一律 `Ok`、`page_count()` 一律 `0`、JSON 一律 `None`），
考卷因此拍**斷言紅**（非編譯紅）——證明七盞燈真的在檢查東西。

紅燈 commit：`c1813a1`（紅燈態考卷＋佔位殼＋`Cargo.toml` 依賴面；紅→綠 diff
因此只含實作本體，不混入 import/依賴噪音）。

## 二、紅燈實測輸出（`cargo test -p quire-web --test odf_d2_wasm_view`，佔位態）

```
running 7 tests
test lamp1_open_bytes_yields_at_least_one_page ... FAILED
test lamp2_page0_text_items_match_docx_contract_field_names ... FAILED
test lamp3_page_bounds_return_option_never_panic ... FAILED
test lamp4_shell_is_a_thin_passthrough_of_odtview ... FAILED
test lamp5_encrypted_and_corrupt_fail_loudly_not_silently ... FAILED
test lamp6_playground_read_surface_is_complete ... FAILED
test lamp7_docx_side_contract_unchanged ... ok

lamp1 panicked at shells/web/tests/odf_d2_wasm_view.rs:122:5:
目標檔經 wasm 殼至少要 1 頁，實際 0（靜默 0 頁＝ODF-E1 教訓禁止形態）

lamp2 panicked at shells/web/tests/odf_d2_wasm_view.rs:138:10:
第 0 頁顯示列表 JSON 應存在

lamp3 panicked at shells/web/tests/odf_d2_wasm_view.rs:175:5:
前提：目標檔至少 1 頁

lamp4 panicked at shells/web/tests/odf_d2_wasm_view.rs:206:5:
assertion `left == right` failed: 殼與直接面頁數必須一致
  left: 1
 right: 0

lamp5 panicked at shells/web/tests/odf_d2_wasm_view.rs:261:18:
加密檔必須報錯，竟構造成功（頁數 0）——靜默空視圖＝ODF-E1 教訓禁止形態

lamp6 panicked at shells/web/tests/odf_d2_wasm_view.rs:292:5:
assertion failed: view.page_count() >= 1

test result: FAILED. 1 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out
```

鑑別正確性判讀（**期望**的紅燈形態，逐盞對帳）：

- 燈 1／2／3／6：全紅——它們斷言「有頁、有項、JS 讀取面形制可用」，空殼必然紅。
  燈 3 死在**前提**斷言（先把「有頁」當前提），是燈 1 紅的連帶，非混用。
- 燈 4（薄殼透傳鎖）：紅在 `left: 1 / right: 0`——左邊是**直接面**
  （`OdtView::from_bundle` 走真實 D1 發射層，1 頁），右邊是殼（0 頁）。這條同時
  反證兩件事：核心發射層本來就是綠的（D1 已收案，本卡不動）、殼層確實漏了東西。
- 燈 5（老實報錯鎖）：紅在「加密檔竟構造成功（頁數 0）」——這正是 ODF-E1 教訓點名的
  禁止形態：加密檔被降格成**靜默空白視圖**，而不是明確報錯。佔位殼把這条假綠抓出來了。
- 燈 7（DOCX 零回退鎖）：**仍綠——正確**。它守的是 `WasmEngine`（DOCX 線）在本卡
  動過 `lib.rs` 後的欄位契約，佔位殼與它無關，本該綠。若它在空殼下也紅，代表它混入
  了 ODT 斷言、失去獨立守門價值（同 D1 燈 4 的分工邏輯）。

## 三、綠燈實測輸出（真實作後）

### 3.1 host 考卷（`cargo test -p quire-web`）

```
running 7 tests
test lamp5_encrypted_and_corrupt_fail_loudly_not_silently ... ok
test lamp2_page0_text_items_match_docx_contract_field_names ... ok
test lamp6_playground_read_surface_is_complete ... ok
test lamp1_open_bytes_yields_at_least_one_page ... ok
test lamp3_page_bounds_return_option_never_panic ... ok
test lamp4_shell_is_a_thin_passthrough_of_odtview ... ok
test lamp7_docx_side_contract_unchanged ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

燈 2 實測欄位名（逐欄對基準檔 `variant_required_keys.Text` 全等）：
`color_argb / flags / size_pt / text / x / y`（`family` 按 serde `skip_serializing_if`
跳欄，與 D1 發射層口徑一致——ODF `style:font-name` 屬 D1b 未採集）。
燈 4 逐頁比對：殼輸出與 `OdtView::from_bundle`（同 `page_geometry` params）**逐字相同**，
段落／碼點維持 D1 帳 13／402。

### 3.2 實際產物端（`npm run build:wasm` 後 `npm run test:odt`）

這條補的是 host 考卷**沒經過**的那一段：wasm-bindgen glue＋瀏覽器/Node 載入路徑。

```
✓ 實際產物 wasm 初始化（Node, module_or_path 手動餵位元組）
✓ 首包 glue 導出 WasmOdtView（ODT 面在首包，非分塊）
✓ ODT 開檔渲染：1 頁 / 13 段
✓ 第 0 頁可畫：23 顯示項（23 個 Text）
✓ 越界頁碼全回空且不 abort（含 2**53-1，共 1 頁界外三連發）
✓ 紙張狀態列可用：612×792pt（未覆寫）
✓ idle 續排迴圈零改可用（is_settled 恆真、reflow_step_json 零事件）
✓ 加密檔老實報錯：頁面幾何解析失敗: 加密不支援：條目 styles.xml 宣告加密
  （演算法 Blowfish CFB），本引擎不實作解密…
✓ 垃圾位元組落「開啟失敗」口徑
✓ DOCX 線零回退：2 頁 / 41 顯示項
✓ ODF-D2 wasm 端考卷全綠（實際產物＋playground 取數序列）
```

「JS 端零改」的證據就在這：腳本逐字重播 `quire-www/public/playground/playground.js`
的取數序列（`page_count` → `page_display_list_json(page)` → `JSON.parse` →
`width_pt`/`height_pt` 算縮放 → 逐項讀 `Text.{x,y,size_pt,flags,color_argb,text}`
→ `paper_json` 七鍵 → `is_settled`/`reflow_step_json`），對 ODT 直接跑得通，
繪圖碼一行不用動。站端唯一需要的是**入口分支**（odt 檔名→`new WasmOdtView`
取代 `new WasmEngine`）——那屬 quire-www 網站卡（卡面白名單已划走）。

### 3.3 真瀏覽器實測（vite dev server＋CDP `DOM.setFileInputFiles` 餵 file input）

3.2 證的是 glue＋Node；這一條證**瀏覽器裡真的畫出東西**（用戶原始訴求：
playground 上傳 ODT 能渲染）。灌 `corpus-odt/ext/lo_core_sw_qa_odfexport_WordTest_edit.odt`
進 `shells/web/demo`（vite，port 5199）：

```
狀態列      ：首包就緒：1830.9 KB（wasm 流式編譯，36ms）
pageinfo    ：第 1 / 1 頁（13 段）           ← 走的是 WasmOdtView
canvas      ：850×1100，**墨點 7897 個**（非白頁）
引擎實型別  ：WasmOdtView
caret API   ：typeof caret_rect_json === 'undefined'（唯讀，編輯面不存在＝契約正確）
replace API ：typeof replace_paragraph_text === 'undefined'
paper_json  ：{"margin_bottom_pt":72.0,"margin_left_pt":72.0,"margin_right_pt":72.0,
              "margin_top_pt":72.0,"page_h_pt":792.0,"page_w_pt":612.0,
              "paper_override_applied":false}
is_settled  ：true      reflow_step_json：{"settled":true,"events":[]}
越界頁碼    ：page_display_list_json(99) → undefined（JS 端 falsy 分支接住，不 abort）
```

可見文字（畫面實際渲染）：`One / list firstline -1cm, textleft 2cm, no override /
Two / para overrides firstline +1cm / Three … Six` 全 13 段逐行落地，縮放按
612pt→850px 如實換算。

**DOCX 零回退（同頁改灌 `fixtures/sample.docx`）**：

```
引擎實型別：WasmEngine       pageinfo：第 1 / 2 頁（14 段）
canvas    ：墨點 81136 個    caret_rect_json：function（編輯面回來了）
insert_text_at_caret：function
```

即：入口分派依檔名選對型別，繪圖碼共用同一段，DOCX 線功能面（含 IME caret 契約）
完全不受影響。

### 3.4 加密檔口徑（老實報錯，非靜默）

`Err` 訊息原文（`OdfError::Encrypted` 顯示文案經殼層前綴轉出）：

```
頁面幾何解析失敗: 加密不支援：條目 styles.xml 宣告加密（演算法 Blowfish CFB），
本引擎不實作解密
```

注意落地點：本殼**先**走 `page_geometry()`（ODF-C4 幾何鏈，內部同樣有
`ensure_not_encrypted("styles.xml"/"content.xml")` 雙閘），故加密檔在幾何階段即被擋，
未達 `OdtView::from_bundle`→`project()` 那道閘。兩道閘同一變體同一措辭
（`OdfError::Encrypted`），口徑一致、訊息含條目路徑與演算法名（不捏造），
JS 端 `err.message` 直接可秀。

## 四、包體積實測（`scripts/check-wasm-size.mjs`，本卡增量）

| 產物 | 改前 raw | 改前 gzip | 改後 raw | 改後 gzip | gzip 增量 |
|---|---:|---:|---:|---:|---:|
| 首包 wasm `quire_web_bg.wasm` | 1737.9 KB | 975.7 KB | 1830.9 KB | **1011.6 KB** | **+35.9 KB** |
| 首包 glue `quire_web.js` | 15.8 KB | 3.9 KB | 21.4 KB | 5.3 KB | +1.4 KB |
| 協作分塊 wasm | 94.6 KB | 43.0 KB | 94.6 KB | 43.0 KB | 0（零影響） |

- 首包 gzip 1011.6 KB／預算 3072 KB → 佔 33%，餘量 2004 KB。
- 協作分塊逐位元組不變（`quire-odf` 只進首包，不碰 collab 邊）。
- 拆包符號雙向閘門綠：首包仍無 CRDT 符號、glue 仍無 `WasmCollab` 導出。
- 增量 35.9 KB gzip 的構成＝ODF 容器層＋投影＋發射層（`core/odf` 全量首次進
  wasm 產物）。**注意**：這與 `quire-odf` 的 zip 邊共用 `quire-ooxml::ZipBundle`，
  故 odf 鏈入首包實際上是**零新增 zip/deflate 程式碼**（DOCX 線早已帶在裡面上），
  代價只有 ODF 自己的 XML 投影與發射邏輯。
- `scripts/check-wasm-size.mjs` 本卡**未改**（無閘門註記需求：體積閘門與符號閘門
  皆照舊綠，首包預算不需要動）。

## 五、零回退證據

- DOCX 線：燈 7（欄位契約咬合基準檔）＋既有 `cargo test -p quire-web` 全綠＋
  `scripts/wasm-smoke.mjs` 全綠（首包開檔／編輯重排／協作合併／假拆包四面）。
- `check-layering.sh` 綠：`quire-web` 直接依賴新增 `quire-odf`（禁列
  `quire-(ooxml|xlsx|layout|render-trait|crdt)` 不含它，D1 已坐實），
  CRDT／xlsx 不得進首包的兩條傳遞依賴閘門照舊守死。
- `core/odf/**`／`core/engine-api/**`／`shells/native/**` 零改動（本卡 diff 只落
  `shells/web/**`＋本文件）。

## 六、設計取捨（寫死在此）

1. **錯誤型別用 `String` 而非 `JsError`**：`JsError::new` 是 wasm-bindgen import，
   非 wasm target 呼叫即 panic（實測：`cannot call wasm-bindgen imported functions
   on non-wasm targets`）。用 `String` 後，加密／垃圾位元組的**錯誤路徑考卷**能在
   host target 跑（燈 5），JS 端語義不變——wasm-bindgen 把 `Err(String)` 拋成
   `Error`，`err.message`＝原字串，playground.js 現行的 `err.message || String(err)`
   兩條口徑都吃得下（實測於 3.2）。
2. **零頁守門**：構造期若 `page_count()==0` 直接 `Err`。實測依據：空投影
   （`OdfProjection::default()`）經發射層仍給 **1 頁**（零文字項發空行佔位，
   與 DOCX 線 `emit_line_run_items` 零寬行同口徑）——故 0 頁只可能是上游退化，
   寧可報錯也不交白頁（ODF-E1 教訓的正面實作）。
3. **`paper_json` 補齊七鍵**：欄位名與 `Engine::paper_json` 逐欄同名，
   `paper_override_applied` 恆 `false`（ODT 本殼不接受紙張覆寫，發老實值而非假裝支援）。
   這是「JS 端零改」的必要條件——`updatePaperStatus`/`updatePaperUrl` 缺鍵會整段空白。
4. **`is_settled`／`reflow_step_json` 補齊**：ODT 無增量重排（開檔一次性分頁），
   恆真＋零事件。缺這兩條 JS 的 `render()` 會 `TypeError`（函數不存在）——不是
   「ODT 需要改 JS」，而是殼必須補齊 JS **既有的**通用契約面。
5. **不做編輯面**：`WasmOdtView` 不暴露 `replace_paragraph_text`／caret 諸方法。
   ODT 線目前無編輯契約（D1 解剖報告 §5），唯讀檔型硬塞編輯面＝假契約。
   playground 對 ODT 走唯讀顯示路徑（站端入口分支屬網站卡）。
