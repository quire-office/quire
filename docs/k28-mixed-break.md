# K28：中英混排表格爆字——幽靈網格欄稀釋格寬（根因報告）

用戶 11/01 線上 playground 實測截圖實錘（真檔 22.6KB，渲染 11 頁 vs LO 7 頁）：
`Phase 0 (單機驗證)` 鋸成 `Pha/se 0`、`Cloudflare Workers Paid ($5)` 鋸成
`Clo/udfl/are/Wor/ker/s`、`$10.00` 鋸成 `$10./00`、`資源規格與用途` 逐字直排。

## 根因（不在 UAX14／KP 層）

卡面假說 (a)「拉丁字內被誤標 allow」**不成立**：`break_atoms` 直接自證
（燈 3 後半）——`結束。`→`['結','束。']`、`$100` 整塊、`Phase`/`Cloudflare`
永不內切。`Clo/udfl` 是 KP **緊急切分**（`kp.rs`：atom 比行寬寬時「寧可溢出
不可丟字」逐字鋸）的下游症狀，上游是**表格欄寬算錯**。

上游兩個環節：

1. **解析器口徑**（`core/ooxml/src/table.rs`，本卡白名單外）：`w:tblGrid`
   內嵌的 `w:tblGridChange`（修訂歷史網格，OOXML 用於記錄欄結構變更前狀態）
   其內層 `w:gridCol` 被 frame-stack 解析**併入** `t.grid`——有效欄數＝直屬欄
   ＋歷史欄。用戶檔 §七表直屬 6×1560 dxa＋歷史 6×1560 dxa ⇒ `t.grid` 12 欄。
2. **欄寬分配**（`resolve_columns`，本卡修復點）：幽靈欄（有效欄數＞各列
   Σ`w:gridSpan` 最大值）參與 `w:tblW` 等比歸一 ⇒ 每欄 39pt（LO：raw
   1560 dxa＝**78.0pt**），內寬 39−18＝21pt（LO：60pt）。`Cloudflare`≈54pt、
   `$10.00`≈33pt 等 atom 全部比 21pt 行寬寬 ⇒ 緊急切分逐字鋸＋行數灌水
   （11 頁 vs 7 頁）。

## LO 25.8 ground truth（worker 2026-11-02 獨立實轉＋格線量測）

| 檔 | 併入後 grid | 列用 N | tblW | LO 實測 |
|---|---|---|---|---|
| k28-repro §七表（倉外真檔） | 12×1560 | 6 | 9360＝直屬和 | p4 七條豎線 72.4…540.4，節距 **78.0pt＝raw**（非歸一 39pt）；全文 **7 頁**（`/Count`＋pymupdf 雙驗） |
| `xopt/TestFiles_RA001-Tracked-Revisions-01` | 6×(3162,2112,3696)×2 | 3 | 8970＝直屬和≠併入和 17940 | **6 頁**；幽靈態 raw [158.1,105.6,184.8]pt 收斂（舊歸一 k=0.5 灌水 7 頁） |
| `xopt/TestFiles_RA001-Tracked-Revisions-02` | 同型 | 3 | 同型 | **6 頁**（同上） |
| `poi/bug65738`（無 GridChange） | 2 欄用 2／2 欄用 2 | ＝欄數 | — | **非幽靈態**，零擾動旁證 |
| `ooxmlexport/tdf104061` tbl2（無 GridChange） | 7 欄用 6 | 6 | 8052＝全欄和 | 舊路徑歸一恒等 ⇒ 截斷前後列寬逐位相同（display-list A/B diff=0），**非判別檔** |

判別軸：§七表（raw 78 vs 歸一 39，LO 取 raw）與 RA001 兩檔（raw 節距＋6 頁）
是「截斷＋跳過歸一」vs「截斷後仍歸一」的真判別檔。前稿引用的 bug65738
「4 欄用 3、LO raw 117.4pt」與 tdf104061「歸一口徑更遠」**實測不成立**（前者
實為 2 欄用 2 非幽靈表；後者舊路徑歸一恒等無從判别），worker 11/02 覆核後
已從本檔與程式碼註解剔除，改列零擾動旁證。PM 可對 `/mnt/c/Users/Public/k28/`
下 LO 實轉 PDF 覆核。

規則（與 LO 一致）：

1. **截斷**：`N = max(列 Σ gridSpan)`；`N < grid.len()` ⇒ 取前 N 欄（幽靈態）。
2. 幽靈態**跳過** tblW 等比歸一，保留 raw gridCol。
3. 非幽靈態（N＝欄數）路徑**逐位元組不變**（corpus 400 檔 display-list A/B：
   幽靈表 24 張／11 檔中，僅 RA001 兩檔分支翻轉；其餘含 fdo68607、
   tdf104061、HC006、HtmlConverter 全部 diff=0）；總寬＞版心壓回閘保留。

## 禁則（卡面燈 3 方向修正）

卡面文字「行尾不得是 `、。`」與 UAX14/LO 相反：`。、`（CL/NS）合法可行尾、
**禁行首**；`（「`（OP）合法可行首、**禁行尾**。真檔 buggy 態實錘 4 條違規
（行尾 `（`）正是緊急切分繞過 atom 禁則的產物——修欄寬後自然歸零（燈 3＋燈 4）。
燈 4 判行須先把同 y 的多樣式 run 片段併行（粗體「● Developer Pro（主力訂閱）」
＋同 y「：$15…」是**行中段**片段，逐項判會誤報行首禁則）。

## 交付

- 修復：`core/layout/src/table.rs::resolve_columns`（幽靈態截斷＋跳過歸一；
  `K28_GHOST_GRID_BUGGY=1` 回塞鎖，循 K21/K23 先例）。
- 回歸：`core/engine-api/tests/k28_cjk_latin_break.rs` 九燈（合成檔
  `k28_ghost_grid.docx` 由 `gen_k28_fixture.py` 產生，幾何軸逐部複製真檔
  §七表、文字中性化——用戶策略資產不入公開倉庫；真檔鎖走
  `K28_EVIDENCE_DOCX` 倉外 env）。
- 金樣對齊：`k7_advance_scale.rs` 零擾動臂＋`r2_tbl_header.rs` 頁數守門臂
  RA001 頁數 7→**6**（LO 真值收斂，逐值證據見各檔註解與本檔表格：
  k7b-lo 存證 PDF 實數 6＋worker 11/02 獨立重轉 6＋k5/r8-diag 同口徑；
  r2 舊註記「LO 7 頁」與其自證檔矛盾，屬歷史誤記非本卡引入）。
- 實測：綠燈 9/9（含真檔鎖：頁數 11→**7**、格線 78pt 節距、PM 19 條爆字清單
  歸零、禁則零違反）；回塞態燈 1a/1b/1c/1d/2/3＋燈 4 共 7 燈必紅；
  `fdo68607` 零擾動（8 頁、二欄 161.8pt）；corpus 400 檔頁數 A/B 僅 RA001
  兩檔變動（7→6＝LO）；render_lab RA001 LO 6/我方 6。
