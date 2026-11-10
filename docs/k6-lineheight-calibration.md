# K6-cal — LO auto 行距模型校準報告（report-only）

> 卡：t_bd5f82ad（board `quire`）。工具鏈基準＝**LO 25.8.7.3**（Windows soffice.exe 主量測＋Linux
> `~/.local/lo-linux/opt/libreoffice25.8` 覆測，兩者同版本同 build hash 30742500，交叉驗證一致）。
> 手搓最小 docx（單段 11 行 `<w:br/>` 連線、零段距、1" 邊界 Letter）→ PDF → pymupdf 取行 y 差分眾數。
> 原始數據落盤 `docs/k6-lineheight-matrix.csv`（本卡唯一數據附件）；探索腳本在
> `~/.hermes/profiles/ow-qa/cache/scratch/k6-cal/`（gen_matrix.py／measure.py／step6d_real_linux.py 等）。
> PM 收尾註記：本報告由 PM 依 worker 兩 run 的實測數據（pitch_raw.csv 185 列）獨立重算覆核後代為成稿入庫
> （本卡 run1/run2 皆 3h 燒盡於 13.75 追兇；矩陣與擬合在兩 run 期間已完成，逐格殘差 PM 以 awk 重算覆核＝本報告數字）。

## 1. 核心結論（K6-impl 直接照抄的口徑）

**LO auto 行距（lineRule=auto）的 pitch：**

```
pitch_pt = size_pt × (declared_line / 240) × LH(effective_face)
```

- `size_pt` ＝該行內**最大 run 字級**（混級行取大）。
- `declared_line` ＝生效層的 `w:line`（級聯見 §4）；**零宣告 ≡ 240**（§3）。
- `LH(face) = (hhea.ascender + |hhea.descender| + hhea.lineGap) / unitsPerEm`，取
  **LO 替代後實際渲染 face** 的 hhea（不是宣告族的名義值）——這是全部矛盾的解鑰：
  LO 查的是「實際用字 face 的 real line-height」，宣告字體族只決定 substitute 到哪個 face。
- 混排行（拉丁＋CJK）＝ **max over runs（size×LH(run face)）**（§5）。
- **沒有 1.4／1.25／1.22／0.975 這類普適後備常數**——全部是缺「族→real-LH 表」的湊數。
  擬合殘差（99 個有宣告格，PM 重算）：mean |err| = **0.031pt**，max |err| = **0.102pt**
  （worst＝Liberation Serif 10pt w480；PDF y 座標 0.05pt 量化＋單行 half-pixel 抖動即可解釋）。
  卡面目標 ≤±0.3pt 大幅達成；0.975 補正無存在必要，K6-impl 必須讓它死亡。

### 家族因子表（3 位小數；fontTools 讀 hhea 與實測因子雙軌一致）

| 宣告族 | LO 實際渲染 face | LH（hhea real） | 實測因子 mean [min–max]（n=20） |
|---|---|---|---|
| Calibri | Carlito | **1.2207** | 1.2203 [1.2167–1.2273] |
| Cambria | Caladea | **1.1720** | 1.1713 [1.1667–1.1750] |
| Liberation Serif | LiberationSerif | **1.1499** | 1.1508 [1.1449–1.1565] |
| Times New Roman | LiberationSerif（替代） | **1.1499** | 1.1513 [1.1449–1.1600] |
| Arial | LiberationSans | **1.1499** | 1.1499 [1.1449–1.1545] |
| （theme 斷鏈 fallback） | Noto Serif | **1.3620** | 探針 t_themenone_11_notheme 15.0/11=1.3636 ✓ |
| （CJK：PMingLiU/KaitiTC/MSYaHei 宣告） | **WenQuanYi Zen Hei**（本基準環境） | **1.3496** | e_* 探針 14.8–14.9/11 ✓ |
| PMingLiU（真 face，僅供對照） | mingliu.ttc | 1.1992 | —（25.8 基準環境未用它渲染，見 §6） |

> 因子＝實測 pitch ÷ (size×declared/240)。spread ≤0.016 全在量化雜訊內＝**倍率線性成立、
> 每族一個 real-LH 因子成立**（候說一勝出；查表說無需）。
> 表值一律以「基準 LO＋本機字型環境」實測為準：**換 LO 版本或換字型環境，表就作廢**——
> 這正是為什麼因子必須實測、不能從 TTF metrics 推導後宣稱普適。

## 2. 矩陣（125 格設計，實測 124 格）

family(5) × size(10/11/12/14/20) × declared(240/276/360/480/NONE)，全格數據在
`docs/k6-lineheight-matrix.csv`（pitch_pt＝LO 實測眾數、pred_pt＝公式預測、err_pt＝差）。

- **實測 124/125**：誠實記帳——`m_LiberationSerif_sz10_w240`（連帶 `_wNONE`）在 measure 批次
  漏量（manifest 有列、pitch_raw 無值）。該族同格尚有 24 格，擬合不受影響。
- 跨字級×倍率因子恆定（上表 [min–max]），倍率線性逐點吻合（抽例）：
  - Calibri 14pt：240→17.1（預 17.09）；276→19.6（19.65）；360→25.6（25.63）；480→34.2（34.18）
  - Cambria 20pt：276→27.0（26.96）；Arial 10pt：480→23.0（23.00）
- 真檔對照（pitch_real.csv）：fdo68607 p0/p1/p2 眾數 **17.1**（40/42/41 行）；
  patho_b 三頁眾數 **14.85**（見 §6）。

## 3. 未解問題判定（卡面四題）

**(1) 276=1.15 倍率乘在哪一層？** → **乘 real line-height，不乘 em**。
每格 276 都落在 `size×1.15×LH_face`（±0.06pt 內），不是 `size×1.15×em`（後者對 Calibri 應 16.1 非 19.6）。
`w:after` 等段距在 `<w:br/>` 同段內完全不參與（v3_after200 實驗：加 w:after 段內 pitch 不動）。

**(2) docDefaults spacing vs 段落 spacing 優先序？** → 生效序（高→低）：
**段落直接 pPr ＞ Normal 樣式 pPr ＞ 表格樣式 pPr（套用儲存格段落）＞ docDefaults pPrDefault**。
鐵證（pitch_v/pitch_f）：
- v17（Normal 360 vs 表格樣式 240）→25.6＝Normal 贏；v18（直接 240 vs Normal/表格 360）→17.1＝直接贏。
- **styles.xml 整支必須被 `word/_rels/document.xml.rels` 引用才生效**：無 styles 關聯時
  docDefaults 全隊被 LO 忽略（y/w 批 17.1 vs r/s 批補 rels 後 19.6；f2_norels 同理）。
  quire 級聯實作若直接吃 styles.xml 而不管 rels，會在無 rels 檔上多出宣告（本缺陷人群小但真實）。

**(3) 零宣告（None）路徑 LO 因子？** → **零宣告 ≡ declared 240**：25 個 NONE 格與對應 240 格
**逐格全等（max|Δ|=0.00pt）**。即 LO 無宣告時走 `size × LH_face`，無任何「1.4 後備」。
quire 現行 None→×1.4（11pt→15.4）比 LO（Calibri 11pt→13.5）鬆 14%——K6-impl 要連根拔。

**(4) East Asian 混排取哪個 script？** → 取 **max（size×LH(face)）over runs**，即 CJK 行由
CJK face 決定（本環境替到 WQY Zen Hei 1.3496）。探針（pitch_raw `ea` 組，11pt）：
EA:Calibri→13.4（=11×1.2207）；EA:PMingLiU／EA:KaitiTC／純 CJK→14.8（=11×1.3496 眾數）；
純拉丁段宣告 EA:PMingLiU→13.5（EA face 不渲染就不參與）。**不是「取 Latin」也不是「取 EA」**，是取大。

## 4. fdo68607 矛盾完全破解（PM 覆核兩組「矛盾」數據的根因）

- 全文為一張大表，表格樣式 `MediumShading1-Accent1` 宣告 `line=240` → 依 §3(2) 表格樣式層
  **壓掉 docDefaults 的 276** → 生效 declared=240 → 14pt×1.2207×1.0 = **17.1**（眾數實測吻合）。
- 拆解探針：f1（表格樣式改 360）→23.4 段內混排；f3（砍表格樣式）→19.6＝docDefaults 276 露出。
- 於是 PM 舊測「自製 Calibri auto 240＝17.05」與「fdo 17.1÷14÷1.15＝1.062」不再矛盾：
  前者本來就是 declared=240；後者的 1.15 根本沒生效（被表格樣式擋）。
  「1.22×0.975」「1.062」「16.1 vs 17.05 之爭」全部是同一個級聯錯位的投影。

## 5. patho_b 13.75 懸案 → LO 版本漂移（重要旗標，R8 歸因需覆核）

- R8-diag 採信 `patho_b` LO pitch **13.75**（＝11×1.25），出處 PDF 的 producer 是 **LO 24.8.0.3**
  （舊 Debian port，內嵌真 PMingLiU）。
- 本卡在同一檔以**基準 LO 25.8.7.3**（Linux lo-run-real258 ＋ Windows soffice.exe）實測：
  pitch 眾數 **14.85**（PMingLiU→**WQY Zen Hei 1.3496** 替代，11×1.3496=14.846 ✓ 逐頁全中）。
- **13.75 在 25.8 不可重現**（全環境字體掃描僅 Sitka 1.2502 巧合、非替代路徑）。1.25 是 24.8
  工具鏈的產物，不是 LO 語義。
- **旗標**：quire 儀表板基線是 LO 25.8.7.3 → patho_b 的 LO 真值應為 14.85 口徑
  （每頁行數 697.9/14.85≈47，非 R8 依 13.75 推的 50）。R8 對 patho_b p3 殘差的「容量差」定量
  需以 14.85 重算；K6-impl 的 red 錨點一律以本卡 25.8 實測為準。

## 6. 給 K6-impl（t_43ac64aa）的實作規格＋紅燈值

1. **資料結構**：`family→factor` 查表（上表 §1，鍵＝宣告族小寫，值＝LH）＋`None→240` 歸一化。
   查不到族時走 **fontTools 讀不到就退回 1.1499（Liberation 系）＋TODO 標註**，
   不得使用 1.4/1.22/0.975 任何舊魔術。-theme 斷鏈 fallback 列（Noto Serif 1.3620）要入表。
2. **渲染 face 替代表**：表鍵是「宣告族」，值錨「LO 替代後 face」——本環境
   Calibri→Carlito、Cambria→Caladea、Times/Arial→Liberation 系、PMingLiU/KaitiTC→WQY（1.3496）。
3. **混排行高**：`max over runs (run_size × LH(run face))`。
4. **紅燈值（全部由本表產生，可直接斷言）**：
   - `fixtures/k6_auto_pitch.docx`（Liberation Serif 14pt，段落宣告 240/276/360/480）
     → 段內 pitch **16.1 / 18.5 / 24.2 / 32.2 ±0.6pt**（公式 16.10/18.51/24.15/32.20；LO 實測同）。
   - `fdo68607`：pitch **17.1**（表格樣式 declared=240 生效，§4）→ 頁數 **8**（LO）。
   - `patho_b`：pitch **14.85**（11pt 零宣告×WQY 1.3496）——**不是 13.75**。
   - 零宣告拉丁檔（Calibri 11pt auto）：LO 13.5；quire 舊徑 15.4（×1.4）必紅。
   - buggy 回塞鎖（K6_AUTO_PITCH_BUGGY=1）依既有開關回舊徑，紅燈 1 必紅。
5. 級聯實作補一條：styles.xml 未經 `document.xml.rels` 引用則整支不參与（§3(2)）。

## 7. 可重跑路徑

- 產生：`gen_matrix.py`（矩陣＋探針）→ 量測：`measure.py`／`step6d_real_linux.py`（眾數）→
  彙整：`analyze.py`；數據：`pitch_raw.csv`（185 列全量）＋ `pitch_real.csv`（真檔）。
- 覆核腳本（PM 收尾用，awk 一行的那個口徑）：見本報告 §1 殘差數字，任何值守可照 §1 公式重算覆核。
- 字型 LH 覆核：fontTools `hhea`（ascender+|descender|+lineGap)/unitsPerEm，face 對照見 §1 表。

*（本卡 report-only，零引擎觸碰；矩陣數據檔 `docs/k6-lineheight-matrix.csv` 隨本報告同 commit 入庫。）*
