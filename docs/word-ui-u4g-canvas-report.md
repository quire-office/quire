# U4g 交付報告：web 殼 canvas 渲染器消費 `DrawItem::Text.underline` 繪形態幾何

- 任務：t_d0a46cec（ow-render）｜來源：U4f（t_7ad5a6f3）交付報告 §6.1 未做面
- 分支：`wt/t_d0a46cec`（基線＝main tip `5470fca`，`git merge-base --is-ancestor` 自證；先合併母卡 U4f `28a4080` 取 Ribbon 選單臂；四筆 commit `b7327d0`／`48aeafb`／`c81205c`／`8b6a0fc`；未 push）
- 收尾標：UNDERLINE_CANVAS_GEOM

## 1. 問題與落點

用戶現況：底線形態選單可見（U4f）、引擎發射形態欄（E-2）、檔契約鎖死（U4f 探針），
但 web 殼編輯器 canvas 上形態間**零像素差異**——`shells/web/editor/vendor/renderer.js`
只唸 `Text.flags & FLAG_UNDERLINE(4)` 兜底畫單條直線，未讀 `Text.underline` 欄。

## 2. 實作（vendor/renderer.js）

`resolveUnderlineGeom()` 三態口徑分派：

| 形態 | 幾何 | 真值源 |
|---|---|---|
| single | offsets [0.12]、width 0.07、`UNDERLINE_MIN_PX` 1 保底 | 在庫單線畫法原樣（SVG 端 `Single` 走 `text-decoration` 無可取樣倍率——零位移向後相容） |
| double | offsets [0.13, 0.25]、width 0.055 | `render_svg.rs::uline_geom()` 逐字 |
| thick | offsets [0.15]、width 0.13 | 同上 |
| dotted | offsets [0.15]、width 0.09、dash [1,2]×筆寬 | 同上 |
| dashed | offsets [0.15]、width 0.075、dash [4,2]×筆寬 | 同上 |

- `Some("off")`／明確 `"off"` → `null` 一筆不畫。
- `{"raw":…}`、未知字面、壞形 → 降級單線，**禁拋錯**（JS 口徑＝渲染輪完整回傳＋零例外）。
- 欄位未宣告＋旗標點亮 → 修前同一組單線倍率（serde default 向後相容零位移）。
- `dash` 段長乘筆寬（`v * penPx`）與 SVG 端 `d*sw` 同口徑。
- 回塞鎖：`process.env.U4G_UNDERLINE_BUGGY`（k21/g1/E-2/E-2b 同款 env 慣例；瀏覽器端本無
  `process`，探針經 CDP 注入 `window.process={env:{…}}` 驅動，零改檔零 checkout）。

## 3. 考卷（先紅後綠）

`shells/web/editor/tests/u4g_underline_canvas_probe.mjs`（真 Chrome CDP＋Canvas2D 像素指紋）：

- **A 層**（真 DOM 閉環）：Ribbon click→core→UI→adapter→引擎→渲染器→像素聚簇。grow×7 到
  26pt（12pt 筆寬 <2px 被 1px 保底吃掉，形態不可辨——老實記帳低字級不承諾鑑別力）。
  single 恰一簇／double 恰兩簇／thick 簇厚 ≥single×1.4／dotted・dashed 欄值到渲染器＋不畫實線／
  off 歸零逐像素等量／drain 回基準。
- **B 層**（合成顯示列表直繪，同一 renderer 入口）：乾淨白布零降部字串逐形態幾何斷言＋
  `flags&4 且 'off'` 與零底線態**逐像素全等**（引擎 Off 恆熄旗標，此鑑別態只能合成）＋
  Raw／未知字面／未宣告降級零例外。
- **C 層**（讀取端保形真檔）：node 造 `w:u w:val="wavy"` docx→真開檔→引擎保形發射
  `{raw:'wavy'}`→canvas 降級單線零例外（寫入端誠實拒 Raw，E-2b 燈 3 口徑）。
- **D 層**（同步鎖）：fetch renderer 源碼逐字比對四組常數＝SVG `uline_geom()` 取樣值，防兩端漂移。

實錄：改後 good 態 **23 盞全綠**；`U4G_BUGGY=1` 回塞態**恰 11 盞紅**（A/B 形態面全滅；
能正確宣稱層——基準/drain/single/降級/檔契約/同步鎖——保持綠＝鑑別力對上）。
隨貨 `scripts/u4g_underline_canvas_buggy_replay.sh buggy|good` 兩態 exit 0（逐盞點名＋
工作區零殘留檢查）。

視覺證據：`docs/word-ui-u4g-canvas-evidence.mosaic.{good,buggy}.png`——同一 renderer 入口
9 態拼圖。good＝double 兩條、thick 明顯加粗、dotted 點列、dashed 段列、off 零筆畫；
buggy＝全數折回單線且 off 殘留底線（修前缺陷如實入圖，兩張 md5 不同＝證據非重影）。

## 4. 考卷面同步重估（血緣記帳）

`scripts/u4b_selfcheck.mjs` 檢查 8 原鎖「三形態幾何全等＝渲染器未做面」，前提已被本卡消除，
按 U4f 同款「可收窄不可弱化」口徑反轉：形態欄逐形態流到顯示列表（輸入契約）＋渲染器源碼
分派鎖（五形態＋Off 不畫）＋原 x/y 全等斷言保留但語義改寫（形態不位移排版幾何＝正確行為）。
`engine-adapter.js` U4f 段「未做面」殘留註記改為 U4g 血緣記帳（純註解，零行為 diff）。

## 5. 驗證匯總

| 項 | 結果 |
|---|---|
| ow-gate（合併態，基線自證 `5470fca`） | 八閘 **fail=0**（fmt/clippy -D/test --workspace/layering/roundtrip fixtures＋corpus/wasm32 七 crate/goldens）；Rust 面檔自 `b7327d0` 起零觸碰，結果對最終樹有效 |
| u4g 探針 | good 23/23 綠；buggy 恰 11 紅（複跑定版） |
| u4b selfcheck（含反轉後檢查 8） | PASS |
| u3 selfcheck | PASS |
| 繁中純度 | staged diff 新增行＋四筆 commit message opencc s2t 逐字：洩漏 7 處全修（逐筆 PATCH 紀錄見 git 史與本檔提交史），殘留僅白名單「吃」 |
| 署名 | 四筆均 ADT109119（amend＋rebase reword 後最終掃描零簡體） |
| push | **未 push**（紀律：待 PM／用戶指示） |

## 6. 未做面／邊界記帳（誠實）

- `wavy` 繪製：引擎寫入端誠實拒、讀取端保形發射 `{raw:'wavy'}`——渲染器按卡面口徑**降級
  單線**，不發明波浪幾何（無 SVG 端真值可依，禁憑空自創）。words／wavy 等 Raw 形態的像素
  morphology 若要做，需引擎＋SVG 端先定義幾何，另卡。
- `renderer.js` 為 vendor 件（SYNC.md：來源 quire-www playground，雙站共繪製器防漂移）。本卡
  只改編輯器內副本；**上游 quire-www 反向同步屬 PM 調度面**，未擅自擴權。
- low-DPI／字級 ≤18pt 時 1px 保底吃掉形態差異——探針 grow 到 26pt 測並老實記帳，非缺陷是
  物理解析度下限。
- `core/` 零觸碰；與活體卡檔面零交集。
