# Web-1 spike：IME 隱形 contenteditable 錨點——驗證記錄與結論

任務：t_7807d954。門槛＝手動驗證記錄＋程式化注入測試（Playwright）。

## 1. 契約（本 spike 定案的顯示端↔輸入端介面）

canvas 只做顯示、收不到composition；真實 IME（注音/倉頌/emoji 面板）必須落在
可編輯元素上。架構採业界標準「隱形錨點」：

- **caret 契約**：`{para, cp}`，`cp` 以**碼點**（Unicode scalar）計——emoji
  （surrogate pair）不拆半；JS 端一律 `Array.from()` 換算，引擎端做碼點→位元組
  切口（UTF-8 安全切口在 Rust 側統一把關）。
- **定位（輸入→顯示）**：`WasmEngine.caret_rect_json(para, cp)` → CaretRect
  （page/x_pt/y_pt/height_pt，shaped advances 度量）。JS 把隱形 `#ime`
  （`contenteditable`，1px、opacity 0）移到該處＋疊畫 caret＋合成中候補疊字。
- **反查（顯示→輸入）**：canvas `mousedown` → `caret_at_json(page, x_pt, y_pt)`
  → caret；`preventDefault` 保住錨點焦點（放選點移給 canvas 會斷 IME 鏈——实测
  第一個 FAIL 就是这个，見 §4）。
- **commit**：`compositionend`（IME 確認）與 `beforeinput`（直接鍵入/貼上/退格）
  統一走 `insert_text_at_caret(para, cp, text)` 進引擎→重繪。
- 跨頁段落：`Line` 新增 `para_index`，caret_rect 能指出段落續排到第幾頁
  （golden 只比 text/width_pt，12 例照綠）。

## 2. 環境探查（影響驗證方式的兩個事實）

1. **CDP `Input.imeSetComposition` / `Input.insertText` 產生與真 IME 同規格的
   事件流**：`compositionstart → compositionupdate×n → (beforeinput:
   insertCompositionText, isComposing=true) → compositionend → (input:
   insertText)`，DOM 內容與 `e.data` 逐段一致——headless Chromium 亦可跑整條
   合成鏈路（探針腳本實測比對過）。這是注入測試能當主力閘門的根據。
2. 本 worktree 環境無圖形 IME 引擎（apt 裝 ibus/fcitx5 需 sudo、無密碼可給），
   故「人手按真注音/倉頌」一輪**本 run 未執行**，以 §5 步驟交人工補驗。

## 3. 程式化注入測試（閘門 A——已全綠）

`scripts/ime-inject-test.py`（Playwright＋CDP；自起 vite server）。實測輸出：

    PASS 開檔後段落 0 可讀  ['Quire 開源文書處理器樣本']
    PASS 隱形錨點持有焦點
    PASS 合成事件流完整（start/update/end）
    PASS 注音合成結果進入引擎  ['測試Quire 開']
    PASS caret 跟隨 commit 前移  [{"para":0,"cp":2}]
    PASS 隱形錨點跟隨 caret 定位  [["137.143px","102.857px"]]
    PASS canvas 點擊反查 caret  [{"para":0,"cp":15}]
    PASS canvas 點擊後錨點仍持有焦點
    PASS 點擊位 insertion 落點正確  ['測試Quire 開源文書倉頌處理器樣本']
    PASS emoji 完整進引擎  ['倉頌😀處理器樣本']
    PASS 碼點/UTF-16 長度一致（emoji 不拆半）  [cp=23 u16=24]
    PASS 重繪顯示列表含 commit 文字
    RESULT: ALL PASS

覆蓋面＝任務點名的三類輸入：注音合成（注音列→候補→確認）、倉頌式選字確認
（insertText 於點擊落點插入）、emoji（surrogate pair 完整性）。
執行：`cd shells/web && npm run test:ime`（CI wasm-size job 已掛同步步驟）。

截圖：`build/ime-inject.png`（開檔＋合成後狀態）。

## 4. 過程中抓到的真實坑（寫進契約）

1. **canvas 點擊必須 `preventDefault`**：否則瀏覽器把 focus 移給 canvas，隱形
   錨點失焦 → IME 斷鏈 → 注入的字掉在地上。注入測試老實紅了一輪才修。
2. **碼點≠UTF-16 index**：`String.length` 數 emoji 是 2；caret 以碼點計、JS 端
   `Array.from` 轉換、Rust 端碼點→位元組——三段各自守好，測試查 `u16>=cp`。
3. **空段／段尾 caret**：`caret_rect` 對 cp 越界鉗到段尾、找不到回 None 由呼叫端
   退回段首（引擎側單測覆盖不 panic）。

## 5. 手動驗證步驟（閘門 B——交人工，本 run 無人手/無 IME 引擎）

前置：WSLg 有 X0（實測存在）；Windows 端啟用「注音」與「倉頌」IME。

    cd shells/web && npm run demo   # http://localhost:5199
    # 用有視窗瀏覽器打開（WSLg 直接跑 Windows Chrome 亦可，重點是真 IME）：
    google-chrome http://localhost:5199   # 或手邊的 Chrome/Edge

逐項打勾（ Expected 均應成立）：

| # | 動作 | 預期 |
|---|------|------|
| M1 | 上傳 fixtures/sample.docx | 頁面渲染；狀態列顯示段數；caret 疊畫在第 1 段首 |
| M2 | 切微軟注音，打 `ce4`（ㄘㄜˋ）+空白 | 候補窗貼著 caret 出現；確認後「測試」進段首且重繪 |
| M3 | 切倉頌，輸入任一字碼後從候補窗選字確認 | 同 M2；插入點＝先前滑鼠點擊處 |
| M4 | Win+. 開 emoji 面板插入 😀 | emoji 完整進引擎（翻頁後仍在）；不動到旁字符 |
| M5 | 點擊段中/段尾/第二頁文字 | caret 跳到點擊處；繼續打字落點正確 |
| M6 | 退格跨 emoji | 一次退一格（半個 surrogate 不可殘留） |

若 M2/M3 候補窗位置明顯偏離 caret → 是 `caret_rect_json` 度量或 SCALE 疊畫問題，
回 §1 契約修，不動 IME 事件處理層。

## 6. 引擎/殼變更清單

- `core/layout`：`Line.para_index`、`CaretPos`/`CaretRect`、
  `LayoutEngine::caret_rect/caret_at`（shaped advances 互逆）、`paragraph_text`；
  新增 3 項單測（單調性/互逆容差 1 碼點/跨頁段尾）。
- `core/engine-api`：`caret_rect_json`、`caret_at_json`、`insert_text_at_caret`
  （含碼點→位元組安全切口）、2 項單測（emoji commit、JSON 往返）。
- `shells/web`：wasm-bindgen 對應四個透傳方法。
- `shells/web/demo`：隱形錨點 `#ime`＋caret/候補疊畫＋composition/beforeinput
  處理＋canvas 反查＋方向鍵；`window.__ime` 測試掛鉤。
- 閘門：`scripts/ime-inject-test.py`＋CI 步驟＋`npm run test:ime`。

## 7. 已知限制／後續卡（本 spike 刻意不做）

- 段首退格只移 caret，未做段落合併；無選區/Shift 擴充；無滑鼠拖曳選字。
- 合成中候補字是 JS 自繪疊字（`#composited`），非引擎度量排版。
- `#ime` opacity:0 在 Chromium 實測composition 正常；Safari/Firefox 候選窗行為
  未驗（跨瀏覽器驗證屬 Web-3）。
- 增量重排未做：每次 commit 全篇重排（骨架既有行為，P0-5 範圍）。
