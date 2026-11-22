# U4f：底線形態 Ribbon 下拉轉 live — 交付報告

任務：t_7ad5a6f3（ow-web）｜來源：E-2b（t_5e62f3b5）交付報告 §6.1＋PM 中途兩則補正
分支：`wt/t_7ad5a6f3`（基線＝main tip `5470fca`，含 E-2b `ac2052e`；未 push）
尾標：UNDERLINE_STYLE_LIVE

## 1. 血緣（先講清楚：斷點不是外殼修的）

`format:underline-style` 的「誠實斷點」原記帳（U4b 時代：`styles.rs` 把 `w:u`
折成 `Option<bool>`、形態不進 `RunStyle`、發射層 `FLAG_UNDERLINE` 一態）由
**E-2（t_b5a4d0bf，docs/word-ui-e2-underline-report.md：`w:u/@w:val` 保形態進
RunStyle＋SVG 發幾何底線）＋E-2b（t_5e62f3b5，docs/word-ui-e2b-underline-shared-type-report.md：
共型寫入端真通＋Raw 誠實拒）**消除。本卡只做 UI 閉環收尾：標籤誠實化＋選單值域
接 canonical＋真瀏覽器閉環。引擎側零觸碰（diff 坐實）。

## 2. 改動清單（逐檔）

| 檔 | 改動 |
|---|---|
| `shells/web/editor/index.html` | L421 展開鈕：`pending:engine-para`→`live`、過期 title 改真實能力宣告（含 E-2/E-2b 血緣＋wavy/words 不入值域宣告）；新增 `#popover-underline-style` 選單骨架（4d 區塊）；頁尾圖例表常用列標籤說明同步 |
| `shells/web/editor/js/qword-core.js` | `format:underline-style` 臂：「誠實缺口、受理=false」→ canonical 六形態閘門＋`_notify('format:underline-style',{style})`；非法形態不受理且不沿用呼叫端 live 宣稱（`table:grid-select` 同款先例）；新增 `UNDERLINE_STYLE_CANONICAL` 導出＋`DEPENDENCY_MAP['pending:engine-para']` 描述更新（殘餘只剩 w:bidi） |
| `shells/web/editor/js/qword-ui.js` | import `UNDERLINE_KINDS`（值域單一來源）；subscribe 新增 `format:underline-style` 臂；`handleFormatUnderlineStyle`（adapter.formatUnderline 落地＋失敗日誌）；`initPopovers` 第 5 節：選單由 `UNDERLINE_KINDS` 動態填充六項（`data-underline-style`＋專屬 listener 帶 `{style}` payload＋stopPropagation，U4c 色塊同款先例） |
| `shells/web/editor/css/style.css` | `.qword-underline-menu`／`.qword-underline-item` 選單樣式（純文字零 emoji） |
| `shells/web/editor/js/engine-adapter.js` | **純註解**（卡 comment 390 逐檔申報）：L1754「誠實斷點（故不得掛 live）」段改寫為血緣記帳＋殘留註記（web canvas 渲染器未消費 `DrawItem::Text.underline`，另卡追）。零行為 diff |
| `shells/web/editor/tests/u4f_underline_style_live_probe.mjs` | 新增：真瀏覽器閉環探針（正式入庫版，U4c 同款口徑） |
| `docs/word-ui-u4f-underline-live-evidence.{json,png}` | 探針證據隨貨 |
| `scripts/u3_selfcheck.mjs` | 考卷重估（E-2 燈 13–15 同款口徑）：檢查 4(c) 由「禁止 live」反轉為「必須 live」（真能力掛 pending＝低報，與假 live 同罪），血緣＋閉環背書寫死註解 |
| `scripts/u4b_selfcheck.mjs` | 考卷重估：檔頭記帳、檢查 8（三形態 flags/幾何全等實測保留，結論改寫為「flags 只承載開關、形態在 Text.underline 欄；canvas 渲染器未做面」）、檢查 9（underline-style 轉 live＋wavy 不代收雙態）、檢查 10（HTML 斷言反轉） |
| `scripts/u4b_label_remap.mjs` | 映射表 `format:underline-style` 條目轉 live（含 U4f 血緣三查依據）——鈕面態維持「同一工具＋同一映射表可重放」不變量 |
| `docs/word-ui-k29e-remap-evidence.csv` | 補登 U4f 轉 live 血緣列（`git show main:index.html` 基準重放產出，非手改） |

白名單自證：`core/` 零觸碰；`engine-adapter.js` 僅註解（已申報）；與活體卡
t_93937dc5（ow-core，同為 U4f 家族缺陷修復）檔面互斥（其探針走 tests/ 新檔，
零現檔交集）。

## 3. 探針（先紅後綠實錄）

`shells/web/editor/tests/u4f_underline_style_live_probe.mjs`（CDP 真 Chrome，
U4c 同款骨架；`python3 -m http.server 8791` + `--remote-debugging-port=9222`）：

- **先紅（改碼前實跑）**：判定 **FAIL 16 項**——展開鈕 `req=pending:engine-para`、
  選單不存在（恰六項實得 0）、逐形態檔契約零命中、core 臂六形態直達全
  `handled=false`。能正確宣稱的層（wavy 誠實三層、值域禁 wavy/words、
  adapter kinds canonical）在改前即綠——鑑別力對上。
- **後綠（改碼後實跑，兩次）**：**全綠 PASS**。逐盞：
  - 展開鈕＋選單六項 `data-requirement=live`、選單恰六項、值域禁 wavy/words、
    值域源＝`adapter.UNDERLINE_KINDS` canonical 逐字對。
  - 逐形態（single/double/thick/dotted/dashed）：真 DOM click→core→UI→adapter→
    存檔件 `word/document.xml` **檔契約逐字對**（`<w:u w:val="thick"/>` 等，
    唯一命中零雜質）＋顯示列表 `FLAG_UNDERLINE` 點亮＋undo 逐位回前態。
  - off：寫 ECMA 檔契約正字 `<w:u w:val="none"/>`（serde 線契約 `'off'` 與檔契約
    `none` 兩套字面禁互竄——PM 補正點名項，探針按檔契約口徑斷言）＋形態零殘留＋
    flags 歸零＋double→off→double 拉回＋收尾 undo drain 回基準。
  - wavy 誠實三層：選單無此項、adapter 受理=false（引擎可寫形態錯誤訊息原字）、
    存檔件零位移；core 臂拒非法形態 `handled=false` 且降標籤 `pending:shell`。
  - core 臂 canonical 六形態直達指令（第二呼叫端）全 `handled＋live`。

### 驗證矩陣

| 項 | 結果 |
|---|---|
| u4f 瀏覽器閉環探針 | 改前 FAIL 16（先紅實錄）→ 改後 PASS（連跑兩次） |
| e2b_underline_shared_probe（回歸） | 20/20 E2B_PROBE_OK |
| u2/u3/u4/u4b/u4d selfcheck | 全 PASS（u3/u4b 為考卷重估後綠；u4b 檢查 10 鈕數 196 既有漂移 ✗ 為 baseline 215 造成，非本卡引入，`ok()` 不計入 errors） |
| 簡繁掃描 | staged diff 新增行 CJK 字集逐字審計：真洩漏 1（U+5355「单」→「單」，已修）；殘留 5 hit（域/建/提/收/改）繁簡同形誤報排除 |
| ow-gate.sh 八閘（合併態） | 見 §5 |
| 零回退 | `format:underline` 主鈕、`format:highlight` 等同排鈕零碰；`DEPENDENCY_MAP` 其餘鍵零動 |

## 4. 設計決策（PM 補給對齊）

1. **值域單一來源**：選單由 `engine-adapter.UNDERLINE_KINDS` 動態填充，core 臂
   本地聲明的 `UNDERLINE_STYLE_CANONICAL` 由探針逐字鎖死等值（qword-core.js 檔頭
   契約零 DOM/零 wasm 依賴，不得 import 拖 pkg 的 adapter）。wavy/words 不入
   任何一層（外殼不代收，引擎必拒）。
2. **閉環斷言檔契約**：探針只認存檔件 `w:u/@w:val` 逐形態逐字＋`FLAG_UNDERLINE`
   開關態，不斷言 UI class；canvas 形態繪製屬 vendor/renderer.js 未做面（E-2
   記帳延續，另卡），探針據此只鎖可宣稱的兩層，不低報也不誇大。
3. **wasm 重建**：`wasm-pack build --target web --out-dir pkg --release`（PM 地雷
   補給：`--target nodejs` 產 CommonJS 會炸 ESM import）。
4. **標籤雙向誠實**：假 live 與「真能力掛 pending」同罪——u3 紅線鎖反轉為
   「必須 live」，並保留 wavy 不代收的反向鎖防過度翻轉。

## 5. 合併態與全閘

基線自證：`git merge-base --is-ancestor main HEAD` 坐實本分支起點即 main tip
`5470fca`（PM 補正確認 E-2b 已入 main＝`ac2052e`；起手時 checkout `ac2052e` 後
merge main 補上 `5470fca` U4f 探針檔，零衝突）。ow-gate.sh 八閘合併態實錄：
見 commit 後 GATE_SUMMARY（fail=0 為收案條件）。

## 6. 未做面（老實記帳）

1. **web canvas 形態繪製**：`vendor/renderer.js` 仍只唸 `FLAG_UNDERLINE` 兜底單線，
   未消費 `DrawItem::Text.underline` 繪雙條/點/虛線幾何（E-2 未做面延續；用戶可見
   那層的形態差異目前只到存檔件＋旗標開關，屬引擎發射資料已備、渲染器未接）。
   → 另卡（ow-render 面）。
2. **選單即時打勾態**：目前六項無「現行形態」高亮（Word 選單有）；屬純外殼美化，
   不影響閉環。
3. **ODF 端形態採集**：E-2b 記帳另卡，零關聯本卡。
