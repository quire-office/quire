# R8-diag — 最後兩席殘差歸因：patho_b p3（0.196）＋ RA001 p2（0.646）

- 任務：t_11f557ba（report-only；禁改 `core/`、`shells/`、`tools/`、`scripts/`，只準 commit 本報告）
- 現態基線：main = `7e04d5a`（R6 後），分數來源 `~/.hermes/cache/scratch/r6-sb/scores.jsonl`
  （seed 20261022、n=25）
- 全部實測均可復現：patho_b 三頁 content `[0.776, 0.694, 0.196]` 逐位吻合（step21 重算），
  RA001 六頁 `[1.000, 0.646, 0.977, 0.885, 0.997, 0.979]` 逐位吻合（step13 重算）。

## TL;DR

1. **patho_b p3=0.196 屬簇 A（分頁容量）家族，但機制層不同**：不是 M1-band 已修的
   「ceil 到粗帶」量化過扣，而是**未宣告行距的後備倍率 `1.4`**（`resolve_line_h_pt` 的
   `None` 分支）對上 LO 按字型 real metrics 排的 **13.75pt（11pt×1.25）**。每行多扣
   +12% → 每頁 45 行 vs LO 50 行 → 尾部 12 行全擠到 p3（LO p3 只有 2 行）。
   這正是 r4 報告點名的「LO auto 行距口徑屬 **K6 pitch 保真**權限」的首個滿額證據。
2. **RA001 p2=0.646 是度量假分歧，不是引擎缺陷**：LO PDF 內容流把 p2 的第二張浮動表
   （`w:tblpPr`）**整塊寫在頁尾之後**（流序 y 序列有一組 686.6→313.1 的大逆序），
   我方 display list 按視覺序發射。把 LO 行改按視覺序（y-cluster）重排，同一管線同口徑
   **0.646→0.966**，且 17 個視覺行逐項全配對成功（零消失、零多發）。處方在
   `scripts/render_lab.py`（LO 端行序），非 `core/`。
3. 與既有報告：K5-diag §2.3 對 RA001 的兩項歸因（`Reference Chromatograms` 發 3 次、
   `Order of injection` 我方少發）**現態均已治癒**（實測雙方均 2/2、4/4，step25），
   本席殘差需重新歸因如上；r4 §117「LO natural order＝內容流序非 bbox 序」與本席
   RA001 發現互相印證，無衝突。

---

## 第 1 席：`corpus/selfgen/patho_table_across_pages_b.docx` p3 = 0.196（版面 0.189）

### 1.1 現象

| 頁 | LO 行 | 我方行 | content | layout |
|---:|---:|---:|---:|---:|
| 1 | 50 | 45 | 0.776 | 0.839 |
| 2 | 50 | 45 | 0.694 | 0.197 |
| 3 | **2** | **12** | **0.196** | **0.189** |

總行數雙方皆 102（1 標題＋50 列×2 行＋1 尾段），**不缺任何內容**——純分頁點位移：
LO 50/50/2、我方 45/45/12。我方 p1 末列＝「續行 22」（y=757.3），LO p1 末列＝「續行 25」；
我方 p3 從「續行 45」開始（y=74.6…244.0，12 行＋尾段），LO p3 只剩「續行 50」＋「之後段落。」。

### 1.2 LO 真值

- pymupdf 實讀 p2/p3：pitch 均勻 **13.75pt**（p2 49 個間距全 13.75；step4）。
- span 實測字型/字級（step21）：`Calibri 11.0`×75 ＋ `PMingLiU 11.0`×75——
  源檔 `docDefaults` 宣告 `w:rFonts ascii=Calibri / eastAsia=PMingLiU、w:sz=22`（11pt），
  全檔**零行距宣告**（無 `w:spacing`、無 `w:trHeight`、無 `w:tblCellMar`、無 `w:tblHeader`）。
- 內容高 = 841.89 − 2×72（pgMar 1440tw）= 697.9pt → LO 每頁 `⌊697.9/13.75⌋` = 50 行。
- 13.75 = 11pt × **1.25**：LO 的「自動」行距按該行最大字型 ascent＋descent（＋lineGap）
  換算，此檔由 CJK 字型 PMingLiU 驅動 ≈1.25em。

### 1.3 我方 dump

- display list（step2/24）：p1/p2 各 45 個 Text y-行、p3 12 行＋尾段；行距 pitch 均勻
  **15.4pt** = 11pt × **1.4**。
- 引擎路徑（現讀碼實證，`core/engine-api/src/lib.rs`）：
  - `doc_grid()` 級聯出 `doc_size=11pt`（docDefaults `sz=22`）、`doc_line=None`（無 spacing 宣告）；
  - `resolve_line_h_pt(11.0, None)` → **`None => s * 1.4`**（lib.rs ≈1980–1989）＝ 15.4pt；
  - M1-band `refine_band_h`（lib.rs:236、1994–2013）因「有宣告且含表格」已細化帶尺
    （15.4/6＝2.5667pt/帶），**量化誤差＝0**——即 p3 殘差已與 ceil-to-band 無關，
    100% 由 1.4 倍率常數決定。
- 45 行 × 15.4 = 693.0pt ≤ 697.9，第 46 行 708.4 溢出——與 LO 的 50 行/頁差 5 行，
  兩頁累積 10 行 + 尾段 → p3 12 行 vs LO 2 行。

### 1.4 消失／位移／多發清單（p3）

| 類別 | 項目 | 座標差 |
|---|---|---|
| 位移 | 續行 45～欄二 50 共 15 個單元（LO 在 p2，我方在 p3） | 出現頁滯後一頁；我方 p3 內 y=74.6～228.6，LO 同內容 y 在 p2 頁尾段 |
| 多發（逐頁口徑） | 我方 p3 承載 16 個表格單元＋尾段，LO p3 僅 1 單元＋尾段——多出的 15 格 LO 早在 p2 已發完 | LO p3 words 僅 `續行 50`(y=74.2)、`之後段落。`(y=87.9) |
| 消失 | 無（102 行總數雙方一致） | — |

### 1.5 歸因（檔案×函式層級）

`core/engine-api/src/lib.rs::resolve_line_h_pt`（`None => s * 1.4` 分支）＋其呼叫端
（`doc_grid()`→基準行高→表格列高）。**不是** `core/layout` 的打包/ceil（M1-band 已細化、
本檔誤差 0），不是 R2 表頭重發（本檔 `tblHeader` 實掃=0，R2 紅燈鎖檔是無 `_b` 姊妹檔）。

### 1.6 與既有報告關係

- **簇 A（分頁容量）同簇、機制不同**：K5-diag §2.6 處方「帶尺細化 ≤2.8pt」已隨 M1-band
  入 main 且對本檔生效（ ceil 誤差歸零），但 K5 的根因敘述「雙重量化」只處理了 ceil 那層；
  本檔證明**殘留根因＝後備倍率 1.4 本身**（LO≈1.17–1.25，engine-api:2762 附近的 K6 註解
  亦記「LO auto 行距有效 pitch≈宣告×1.17」）。**非新缺陷類**，屬 K5 簇 A 的第二層。
- **K6 pitch 保真**：r4 §5-R1 已把「LO 對 auto 行距的字級口徑」劃歸 K6 權限並註記
  「需全量 A/B」；patho_b 是該口徑缺口迄今最乾淨的單因證據檔（其余變因全零宣告）。

### 1.7 若開修復卡該改什麼（處方建議）

1. `resolve_line_h_pt` 的 `None` 分支：由常數 1.4 改為**按實際字型 real metrics**
   （ascent＋descent＋lineGap）換算，與 `TextMeasurer` 同尺；無度量字型時保留 1.4 後備。
   影響面＝所有未宣告行距檔案（大），需 K6 式閘：只在度量可得時啟用。
2. 紅燈建議（先落紅再修）：
   - patho_b lm `height_pt` 13.5–14.0（LO 13.75）；p1 可抽行 ≥50（現 45）；
   - p3 content ≥0.60（現 0.196）、頁數維持 3；
   - seed 儀表板全量 A/B 零劣化（對照 r4 run5 Saturn −0.161 教訓：純文字檔細化曾致倒退，
     故改倍率必須同跑 Saturn／Bug51170／reader-styles 防放風箏鎖）。

### 1.8 可重跑腳本

`~/.hermes/profiles/ow-qa/cache/scratch/r8/`：`step1_lo.py`（LO PDF＋事實）、
`step2_ow.py`（display list dump）、`step3_overview.py`（逐頁行數表）、
`step4_pitch.py`（LO/我方 pitch 直方圖）、`step20_styles.py`（樣式鏈零宣告實證）、
`step21_verify.py`（0.196 逐位復現＋LO 字型實據＋容量算術）、
`step24_dump_struct.py`（dump/lm 結構）、`step25_closure.py`（tblHeader=0 複核）。
產物：`patho_b.pdf`、`patho_b_dump.json`、`patho_b_lm.json`、`out/s*.txt`。

---

## 第 2 席：`corpus/xopt/TestFiles_RA001-Tracked-Revisions-01.docx` p2 = 0.646

### 2.1 現象

六頁 content `[1.000, 0.646, 0.977, 0.885, 0.997, 0.979]`；p2 獨卡。LO p2 抽 34 行、
我方 p2 發 63 個 Text 項（27 空項在 content_key 前已被 filter，step13/14：開關 [B] 與分數無關）。

### 2.2 LO 真值

LO p2 blocks **內容流序**（step15，`get_text("blocks")`）：

```
#0 y=8.2 | #1 150.1 | #2 212.2 | #3 248.8 | #4 290.2 |   ← 正文前半
#5 y=570.7 | #6 591.4 | #7 612.6 | ... #11 y=686.6 |     ← 第二張表（頁尾）
#12 y=313.1 | #13 334.3 | #14 355.4 | #15 376.6 | #16 y=387.1  ← 第一張表（頁中）晚發
```

相鄰 y 逆序對僅一組但跨度極大：**686.6 → 313.1**（整塊第一張表的 5 個 block 被排到
PDF 內容流最後）。源檔實據（step16）：三個 `System suitability solution` 表全帶
`<w:tblpPr w:vertAnchor="text" .../>`（浮動表），LO 依內部繪製次序寫流，不保證 y 單調。
按 y-cluster 重排後 LO p2 視覺行 17 行（step16/23），序內含兩張浮動表各自完整切片。

### 2.3 我方 dump ＋ 逐項對位（step13/23）

同管線同口徑的四開關分解：

| 頁 | 現況(stream) | LO 視覺序重排 | 去 [B] | 兩者全開 |
|---:|---:|---:|---:|---:|
| p1 | 1.000 | 1.000 | 1.000 | 1.000 |
| **p2** | **0.646** | **0.966** | 0.646 | **0.966** |
| p3 | 0.977 | 0.979 | 0.977 | 0.979 |
| p4 | 0.885 | 0.887 | 0.885 | 0.887 |
| p5 | 0.997 | 0.999 | 0.997 | 0.999 |
| p6 | 0.979 | 0.989 | 0.979 | 0.989 |

視覺序逐項配對（step23）：LO p2 全 17 視覺行**每一行**在我方都找到對應項——
Δy 介於 −36.2～＋8.3pt（無消失、無多發；「多發」清單僅是我方把表格 cell 拆成單元項、
與 LO 行聚類口徑不同的演繹偽影）。difflib opcode（step9）顯示 LO 端 176 字元的表格塊
整段出現在 key 尾部——正是流序反常的直接指紋。

### 2.4 歸因（檔案×函式層級）

- **主因（0.646→0.966 的全部差距）：度量面**——`scripts/render_lab.py:151`
  取 LO 行用 `get_text("text")` **內容流序**，我方 display list 是視覺序；
  RA001 p2 因浮動表恰好觸發 LO 流序大逆序而中獎。引擎發射無錯。
- **殘差（0.966 距 1.0 的 0.034）：斷行邊界**——LO 把 `System suitability solution`
  排 1 行（y=355.4），我方拆 2 行（y=330.0/346.8，step5 OW23/OW26），屬 K6 量測尺
  （斷行可用寬×字級度量）缺口，同席副產品，不另立缺陷。
- pitch：我方 19.6pt vs LO 21.15pt（step8）——行高口徑差，與第 1 席同源（K6），
  只造成 y 位移、不造成分數崩塌。

### 2.5 與既有報告衝突點名（必列）

- **K5-diag §2.3**：「`Reference Chromatograms:` 我方發 3 次」與「`Order of injection`
  LO 4 次／我方 2 次（表格切片少發）」——**現態實測雙方均 2/2 與 4/4**（step25，逐頁計數
  LO={p2:1,p3:1}／OW 同），該兩項已由 L3 修訂語義＋R2/M1 治癒。若照 K5 舊結論去找
  p2=0.646 的引擎根因會撲空：本席殘差**不是發射缺失，是 LO 端流序度量假分歧**。
- **r4 §117**：「natural order＝LO 寫 PDF 的內容流序，非 bbox 排序」——印證本席機制，無衝突。
- **n1-fdo76591**：該案流序問題在**我方發射序**（框內文字插入點），與本席**LO 端流序**
  方向相反、宿主不同，不混簇。

### 2.6 若開修復卡該改什麼（處方建議）

1. `scripts/render_lab.py`：LO 端行序在 content/layout 對照前改**視覺序**（y-cluster，
   同 step13 `lo_lines_visual`），但**設觸發閘**以免把「我方順序錯」也洗白：
   僅當該頁 stream 序存在顯著逆序（相鄰逆序 span >50pt）時才重排，且報告欄記錄
   `stream_order_anomaly=true`。
2. 紅燈建議：
   - RA001 p2 content ≥0.95（現 0.646）；其餘頁分數變動 ≤±0.01；
   - 反向防灌水臂：構造「我方故意 y-sort 錯序」的 fixture，斷言觸發閘不洗白；
   - 自測擴充 M2 selftest（`render-9467208/report.md` 同口徑集），對拍 step13 表。
3. 引擎側**不需要**修復卡：RA001 p2 無消失／多發；斷行邊界（2.4 殘差）併入 K6。

### 2.7 可重跑腳本

同目錄：`step5_ra001_p2.py`（逐行對照＋opcode diff）、`step6_lo_p2_lines.py`、
`step10_order_experiment.py`／`step12_exact.py`／`step13_decomp.py`（0.646 復現＋視覺序
0.966 四開關分解）、`step15_stream_order.py`（流序逆序實據）、`step16_final_probes.py`
（tblpPr 浮動表實據）、`step23_align.py`（17 行逐項 Δy 配對）、`step25_closure.py`
（K5 舊結論覆核）。產物：`RA001.pdf`、`RA001_dump.json`、`out/s*.txt`。

---

## 判定總表（紀律第 3 點）

| 席位 | 判定 | 簇 | 修復面 |
|---|---|---|---|
| patho_b p3=0.196 | 已知簇第二層（K5 簇 A 殘留＝後備倍率；非 ceil、非表格切片、非 vMerge、非修訂交互） | 簇 A pitch/容量 → K6 pitch 保真管轄 | `core/engine-api` `resolve_line_h_pt` None 分支 |
| RA001 p2=0.646 | 度量假分歧（LO 浮動表流序≠視覺序）；引擎無錯 | 非缺陷簇；M2 判準修正類 | `scripts/render_lab.py` LO 行序＋觸發閘 |

*本檔為 t_11f557ba 唯一交付物；引擎零觸碰（core/shells/tools/scripts 未動），
探索期 90 分鐘 checkpoint 未觸發（首批復現數據於接手後 30 分鐘內重跑到手）。*
