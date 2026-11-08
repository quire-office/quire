# shells/web

wasm 殼（首包）＋ 協作延遲分塊（`../web-collab`）。

## 構建

    npm install
    npm run build:wasm    # 首包 pkg/（不含 CRDT）
    npm run build:collab  # 協作分塊 ../web-collab/pkg/（含 CRDT）
    npm run build:demo    # vite 打包 demo/dist（驗證 code-splitting）
    npm run size          # 包體積閘門（gzip 表＋假拆包符號檢查）
    npm run test:odt      # ODF-D2 考卷：對 pkg/ 實際產物打 ODT 路徑（免瀏覽器）
    npm run demo          # dev server → http://localhost:5199
    npm run test:ime      # IME 注入測試（Playwright+CDP；需 python3+playwright）

Node 端免瀏覽器驗證：`node ../../scripts/wasm-smoke.mjs`（DOCX＋協作）＋
`npm run test:odt`（ODT；見 `tests/odf_d2_node_smoke.mjs`）。

## ODT 入口（ODF-D2，卡 t_89c4a196）

首包另導出 `WasmOdtView`（唯讀）：`new WasmOdtView(bytes)` → `page_count()` /
`page_display_list_json(page)`——與 `WasmEngine` **同一個** `PageDisplayList`
serde 契約，JS 繪圖碼零改。demo 的檔位分派：`.odt`→`WasmOdtView`、其餘→
`WasmEngine`（`demo/main.js` 的 `isOdt`，繪圖碼共用；唯讀旗標只關 caret/編輯面）。
紅燈/綠燈全記錄：`docs/odf-d2-redlight.md`；host 端考卷：`cargo test -p quire-web`。
ODT 無編輯面（caret/replace 不存在＝契約正確形態），站端上傳分支屬 quire-www 網站卡。

## IME 編輯路徑（Web-1）

真實 IME（注音/倉頌/emoji）只認可編輯元素：demo 頁以**隱形 contenteditable
錨點**（`#ime`）承接 composition/beforeinput，經引擎 caret 契約
（`caret_rect_json`/`caret_at_json`/`insert_text_at_caret`，碼點計）定位與落地。
契約、驗證記錄與手動步驟：`demo/IME-ANCHOR-SPIKE.md`；
CI 以 `scripts/ime-inject-test.py`（CDP `Input.imeSetComposition`）閘死。

## 拆包契約（Web-2，CI 閘死）

- 首包 `quire_web_bg.wasm`：解析/版面/顯示列表。**CRDT 物理上不在其中**
  ——engine 的 `collab` feature 預設關閉，`scripts/check-layering.sh` 以
  cargo tree 傳遞依賴守衛，`scripts/check-wasm-size.mjs` 以二進位符號雙向查。
- 協作分塊 `quire_web_collab_bg.wasm`：只在使用者點「開啟協作」時由
  `demo/main.js` 動態 import（vite 自動拆 chunk；CI 查 dist 內 CRDT 只准在
  非預載 chunk）。
- 首包 gzip 預算 3 MB（CI 記錄每檔 raw/gzip 到 log 與 `build/wasm-size.json` artifact）。
- 載入路徑：main.js 用 `fetch` + `Response` 餵 glue → `instantiateStreaming`
  邊下載邊編譯，並 tee 出下載進度。

分層規則：本殼零業務邏輯；一切經 `quire-engine` 的 JSON 契約。
協作分塊與主殼不共享 wasm 記憶體——操作經 JSON 由 JS 橋接落地。
