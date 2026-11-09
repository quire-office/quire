# E1-C 卡 B：LO 基準環境口徑定案（真 face 裝機態）＋ Trebuchet 垂直尺 1.3620→1.1611 重定錨

- 卡號：`t_ecb9aa10`（E1-C 卡 B，承接 `t_ab6c5319`）
- 定案資料檔：`docs/e1cb-lo-truth-canon.json`（結構化可程式校驗檔）
- 可重跑校驗腳本：`/tmp/agy-e1cb-scratch/verify_e1cb_canon.py`（預設讀實測快取秒級重算；帶 `--run-lo` 全量重跑雙平台 LO 轉換）

## 1. LO 基準環境口徑定案（實測回答與可程式判定規則）

### 1.1 主力取樣環境、字型配方與單環境收斂判準

本庫有兩張字型度量查表：`face_line_factor`（垂直行高 real-LH）與 `face_advance_scale`（水平字寬 advance）。兩表必須共用同一套 LibreOffice 真值基準環境，否則垂直與水平永遠只能對齊一半。經本卡跨平台實測，明定標準取樣口徑如下：

1. **主力基準二進位**：優先使用 Linux 本機 `~/.local/lo-linux/opt/libreoffice25.8/program/soffice`（版本 `LibreOffice 25.8.7.3`，build `30742500f2d3eb4366ac312fa33d3dcabdb3eba5`，與 K6／K7 校準報告同版本），Windows LO（`/mnt/c/Program Files/LibreOffice/program/soffice.exe`，實測版本為 `LibreOffice 24.8.0.3`，build `0bdf1299c94fe897b119f97f3c613e9dca6be583`）僅作為跨環境收斂覆核與 `render_lab` 驗收板對照。
2. **進程隔離與併發防護**：每次呼叫 LO 必帶任務專屬 `-env:UserInstallation=file:///tmp/loprof-e1cb-<unique>`、必帶 `--headless --norestore --nolockcheck --nodefault --convert-to pdf --outdir .`、一次只轉單一檔案；Windows `soffice.exe` 的工作目錄必須落在 `/mnt/c/` 本機磁碟路徑且 `--outdir` 給相對路徑 `.`。
3. **真 face 注入配方（零全域副作用）**：本庫既有慣例（`~/.local/share/fonts/k6cal/` 已裝 Calibri／Arial／Times New Roman／Cambria／PMingLiU）為「真 MS face 裝機 ⇒ LO 渲染真 MS face」。針對 `k6cal` 未涵蓋之 MS 字型（如 `Trebuchet MS`），嚴禁修改 `~/.local/share/fonts` 或系統字型目錄（避免污染其他卡已記帳之 LO 真值），一律以 per-process 環境變數 `FONTCONFIG_FILE=/tmp/e1cb-fontcfg/fonts.conf` 注入包含 `trebuc.ttf`、`trebucbd.ttf`、`trebucit.ttf`、`trebucbi.ttf` 四字重副本之私有目錄，並由產物 PDF 的 `span["font"]` 實證命中 `TrebuchetMS`／`TrebuchetMS-Bold`（不得為 `NotoSans*` 頂替態）。
4. **產物校驗與單環境收斂次數（N=2）**：每次轉換後必過四重產物校驗（`os.path.exists(pdf) and os.path.getsize(pdf) > 0 and doc.page_count > 0 and total_words > 0`）；定錨入庫時於獨立 profile 連續轉換 `N = 2` 次，計算逐頁行結構 `(y0, font, text)` 的 SHA-256 簽章 `sig`，實測 13 支基準檔 `sig_1 == sig_2` 全數 13/13 成立（確定性收斂）。若遇併發干擾導致產物缺失或簽章不一致，立即作廢重跑並記帳。

### 1.2 跨環境（Linux LO 25.8 注入真 face vs Windows LO 24.8）13 檔逐檔收斂表

對語料庫全部 9 支宣告 `Trebuchet MS` 之檔案與 `render_lab` 驗收板其餘 4 支基準檔（共 13 檔）進行三態實測，逐檔結果如下：

| 檔名（簡稱／路徑） | XML 宣告 Trebuchet | PDF 實染 Trebuchet | Linux 未注入（頁數／逐頁行數） | Linux 注入真 face（頁數／逐頁行數） | Windows LO（頁數／逐頁行數） | 頁數差（Lin注入 − Win） | 收斂判定與適用規則 |
|---|---|---|---|---|---|---|---|
| `cgs` (`corpus/ooxmlexport/cell-grid-span.docx`) | 是 | 是 | 2p `[33, 5]` | **2p `[36, 1]`** | **2p `[36, 1]`** | `0` | **完全收斂**（`RULE_1`：37 行文字／字型／y0≤0.05pt 逐行全等） |
| `n820504` (`corpus/ooxmlimport/n820504.docx`) | 是 | 是 | 1p `[6]` | **1p `[6]`** | **1p `[6]`** | `0` | **完全收斂**（`RULE_1`：注入後同染 `TrebuchetMS-Bold`） |
| `5DayTour` (`corpus/xopt/OpenXmlPowerToolsExamples_HtmlConverter01_5DayTourPlanTemplate.docx`) | 是 | 否 | 1p `[31]` | **1p `[31]`** | **1p `[31]`** | `0` | **完全收斂**（`RULE_1`） |
| `CoverPage` (`corpus/xopt/NewDocxDocuments_CoverPage.docx`) | 否 | 否 | 2p `[5, 0]` | **2p `[5, 0]`** | **2p `[5, 0]`** | `0` | **完全收斂**（`RULE_1`） |
| `WC048` (`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx`) | 否 | 否 | 1p `[12]` | **1p `[12]`** | **1p `[12]`** | `0` | **完全收斂**（`RULE_1`） |
| `btlr-textbox` (`corpus/ooxmlexport/btlr-textbox.docx`) | 否 | 否 | 1p `[2]` | **1p `[2]`** | **1p `[2]`** | `0` | **完全收斂**（`RULE_1`） |
| `RP001-01` (`corpus/xopt/TestFiles_RP_RP001-Tracked-Revisions-01-Accepted.docx`) | 是 | 否 | 5p `[1, 18, 17, 26, 4]` | **5p `[1, 18, 17, 26, 4]`** | **5p `[1, 18, 19, 25, 2]`** | `0` | **頁數收斂**（`RULE_1`：正文皆 `TimesNewRomanPSMT`，頁數同為 5 頁） |
| `RP001-02` (`corpus/xopt/TestFiles_RP_RP001-Tracked-Revisions-02-Accepted.docx`) | 是 | 否 | 5p `[1, 18, 17, 26, 4]` | **5p `[1, 18, 17, 26, 4]`** | **5p `[1, 18, 19, 25, 2]`** | `0` | **頁數收斂**（`RULE_1`：同上） |
| `fdo76591` (`corpus/ooxmlexport/fdo76591.docx`) | 否 | 否 | 1p `[28]` | **1p `[28]`** | **1p `[30]`** | `0` | **頁數收斂**（`RULE_1`：同為 1 頁；框內無宣告字型 fallback 差 2 行） |
| `LinkedTextBoxes` (`corpus/ooxmlexport/LinkedTextBoxes.docx`) | 是 | 否 | 1p `[72]` | **1p `[72]`** | **1p `[67]`** | `0` | **頁數收斂／行數歸因 `RULE_2`**（源檔宣告 `Candara`，補注入後兩邊皆 `1p [67]` 全等） |
| `HC061` (`corpus/xopt/TestFiles_HC061-Hyperlink-in-Field.docx`) | 是 | 否 | 2p `[34, 7]` | **2p `[34, 7]`** | **1p `[36]`** | `+1` | **不收斂→歸因 `RULE_2`**（源檔宣告 `Georgia`，補注入後兩邊皆 `1p [36]` 全等） |
| `RA001-01` (`corpus/xopt/TestFiles_RA001-Tracked-Revisions-01.docx`) | 是 | 否 | 7p `[1, 33, 18, 23, 15, 5, 0]` | **7p `[1, 33, 18, 23, 15, 5, 0]`** | **6p `[1, 34, 21, 24, 16, 3]`** | `+1` | **不收斂→歸因 `RULE_3`**（LO 25.8 vs 24.8 頁首空表列高差 35.05pt＋尾端刪除空段溢 p6 空頁） |
| `RA001-02` (`corpus/xopt/TestFiles_RA001-Tracked-Revisions-02.docx`) | 是 | 否 | 7p `[1, 33, 18, 23, 15, 5, 0]` | **7p `[1, 33, 18, 23, 15, 5, 0]`** | **6p `[1, 34, 21, 24, 16, 3]`** | `+1` | **不收斂→歸因 `RULE_3`**（同上） |

### 1.3 不收斂檔案之定量物理歸因與可程式判定規則（`decide_canonical_rule`）

本卡將跨環境分歧歸納為三條**純布林可程式判定規則**（實作於 `/tmp/agy-e1cb-scratch/verify_e1cb_canon.py::decide_canonical_rule`，輸入為 DOCX 靜態特徵 `feats`、Linux 注入態 `lin_fc` 與 Windows 態 `win`）：

1. **`RULE_1_CONVERGED_ANCHORED_LINUX`（跨環境頁數收斂）**：
   - **可程式條件**：`lin_fc["pages"] == win["pages"]` 且不觸發 `RULE_2`。
   - **口徑決定**：直接採 **Linux LO 25.8 + per-process `FONTCONFIG_FILE` 真 face 注入態** 為基準真值。實測 `cell-grid-span.docx` 注入四字重 `trebuc*.ttf` 後，Linux LO 25.8 與 Windows LO 在 p0（36 行）與 p1（1 行 `E:redacted@example.invalid`）的文字、span 字型名（含 `TrebuchetMS` 與 `TrebuchetMS-Bold`）及 `y0`（最大差 `≤ 0.05pt`）**0 差異逐行全等**，排除 hinting、DPI 或欄寬取整造成分歧之假設。
2. **`RULE_2_UNINJECTED_MS_FACE_FALLBACK`（第三族 MS 字型未注入導致 Noto fallback 膨脹）**：
   - **可程式條件**：`(lin_fc["pages"] != win["pages"] or lin_fc["lines_per_page"] != win["lines_per_page"]) and any(f.startswith("Noto") for f in lin_fc["fonts"]) and not any(f.startswith("Noto") for f in win["fonts"])` 且源檔 `w:rFonts` 宣告之 MS 字型（如 `Georgia`、`Candara`）出現在 `win["fonts"]` 但未放入私有 fontconfig 目錄。
   - **實測歸因**：`HC061` 正文宣告 `Georgia`（真 face `LH = 1.1357`），Linux 未注入 `georgia*.ttf` 時被頂替為 `NotoSerif-Regular`／`NotoSerif-Italic`（`LH = 1.3620`），行距膨脹 20% 致 36 行溢為 2 頁 `[34, 7]`；`LinkedTextBoxes` 文字框宣告 `Candara`，Linux 未注入時頂替為 `NotoSans`（72 行 vs 67 行）。本卡以對照實驗將 `georgia*.ttf` 與 `candara*.ttf` 注入臨時 fontconfig 目錄後重跑 Linux LO 25.8，**`HC061` 立即由 `2p [34, 7]` 變為 `1p [36]`、`LinkedTextBoxes` 由 `1p [72]` 變為 `1p [67]`，與 Windows LO 100% 收斂**。
   - **口徑決定**：凡命中 `RULE_2` 之檔案，必須將源檔宣告且實際渲染之 MS 真 face 一併加入 per-process `FONTCONFIG_FILE` 後取 Linux LO 25.8 真值（等價於 Windows 真 face 態）。
3. **`RULE_3_HEADER_TRHEIGHT_VERSION_DRIFT`（頁首空表格列高跨 LO 版本差異＋尾端刪除空段空頁）**：
   - **可程式條件**：`feats["has_tracked_revisions"] and feats["has_empty_header_trheight"] and lin_fc["pages"] == win["pages"] + 1 and lin_fc["lines_per_page"][-1] == 0`。
   - **實測歸因（精確至 0.00pt）**：`RA001-01` 與 `RA001-02` 全文渲染字型為 `TimesNewRomanPSMT`（無任何 `TrebuchetMS` span），注入 `trebuc*.ttf` 前後完全不變。兩檔在第二節（Page 1..6）透過 `word/header3.xml` 掛載一個單列無字表格 `<w:tbl>`，宣告 `<w:trHeight w:val="1088"/>`（`1088 twip = 54.40pt`），加 `pgMar header="144"`（`7.20pt`）與尾隨空段（`13.80pt`），頁首總底界為 `7.20 + 54.40 + 13.80 = 75.40pt`。**Linux LO `25.8.7.3` 完整兌現空表列高 `54.40pt`**，將每頁正文頂界由 `pgMar top="720"`（`36.00pt`）推至 `75.40pt`；**Windows LO `24.8.0.3` 則將空表列塌縮為單行高 `19.35pt`**（在 `y0 = 8.21pt` 吐出零寬字元 `\u200d`），頁首底界僅 `7.20 + 19.35 + 13.80 = 40.35pt`。兩版本在 Page 1 首行 `:`（`p#19`）的起點分別為 `y0 = 247.26pt`（Linux）與 `y0 = 212.21pt`（Windows），**差值 `247.26 − 212.21 = +35.05pt` 恰等於 `54.40 − 19.35 = +35.05pt`**，而頁內正文逐行間距 `dy`（`20.70pt`、`36.55pt`、`41.40pt`）在兩平台 `0.00pt` 全等。因每頁頂界相差 `35.05pt`，5 頁累積差 `175.25pt`，使檔尾 3 個帶 `<w:pPr><w:rPr><w:del/></w:rPr></w:pPr>` 的刪除空段落（`p#226..228`）在 Linux LO 25.8 溢出至第 7 頁（Page 6：`0 words`、0 文字行，僅在 `x = 50.55pt, y0 = 75.40..130.60pt` 繪出 3 條 tracked-changes 頁邊修訂豎線）。
   - **口徑決定**：凡命中 `RULE_3` 之檔案，其末頁為 `0 words` 修訂邊線空頁，**有效正文頁數基準取非空文字頁數 `sum(1 for n in lin_fc["lines_per_page"] if n > 0) == win["pages"]`（即 6 頁，與 `render_lab` 驗收板及 `k7_advance_scale` 守門鎖一致）**。

---

## 2. Trebuchet MS 垂直尺重定錨（1.3620 → 1.1611）

### 2.1 四重實測真值（獨立復測，與卡面 1.1611 完全吻合）

本卡依上述定案口徑重新量測 `Trebuchet MS` 的垂直 real line-height 因子 `LH`，四條獨立路徑全部收斂於 **`1.1611`**：

1. **TTF 二進位表直解（四字重同值）**：直接解析 `/mnt/c/Windows/Fonts/trebuc.ttf`、`trebucbd.ttf`、`trebucit.ttf`、`trebucbi.ttf`，四檔皆為 `unitsPerEm = 2048`、`hhea` `(ascender=1923, descender=-455, lineGap=0)` ⇒ `LH = (1923 + 455 + 0) / 2048 = 2378 / 2048 = 1.161133`；`OS/2` `(usWinAscent=1923, usWinDescent=455)` ⇒ `1.161133`。對照 Linux 未注入時 LO 頂替之 `NotoSans-Regular.ttf`：`upem = 1000`、`hhea` `(1069, -293, 0)` ⇒ `1.362000`（即舊表值 `1.3620` 的來源）。
2. **真檔 `cell-grid-span.docx` 無直接 `w:line` 段（繼承 `docDefaults w:line="276"`＝1.15 倍，10pt）**：定錨 LO（Linux 注入真 face 與 Windows LO 全等）於 Team 段（9 行，8 個間距，`y0 = 518.77..625.17pt`）實測逐行間距恆為 **`13.30pt`** ⇒ 反推 `LH = 13.30 / (10 × 276 / 240) = 1.1565`（公式預測 `10 × 1.15 × 1.161133 = 13.35pt`，差 `0.05pt`＝1 twip 量化內；舊頂替值 `1.3620` 為 `15.65pt`）。
3. **真檔 `cell-grid-span.docx` 直接宣告 `w:line="240"` 段（10pt）**：定錨 LO 於 `Available at...` 段（`y0 = 428.67 → 440.27 → 451.87pt`）及聯絡人段（`y0 = 679.77 → 691.37 → 702.97pt`）實測逐行間距恆為 **`11.60pt`** ⇒ 反推 `LH = 11.60 / (10 × 240 / 240) = 1.1600`（公式預測 `11.61pt`，差 `0.01pt`；舊頂替值 `1.3620` 為 `13.65pt`）。
4. **合成探針跨字級實測（零段距、`w:line="240"`、定錨 Linux LO 25.8 + 真 face）**：
   - `sz = 10pt`：11 個間距全為 `11.60pt` ⇒ `LH = 1.1600`（預測 `11.61pt`）
   - `sz = 11pt`：11 個間距全為 `12.75pt` ⇒ `LH = 1.1591`（預測 `12.77pt`）
   - `sz = 14pt`：11 個間距全為 `16.25pt` ⇒ `LH = 1.1607`（預測 `16.26pt`）
   - `sz = 20pt`：11 個間距全為 `23.25pt` ⇒ `LH = 1.1625`（預測 `23.22pt`）

### 2.2 落地效果與卡面前提更正（`e1_pagination` 燈 1 p1 首末行盞直接轉綠解 ignore）

將 `core/engine-api/src/lib.rs`（及觀測鏡像 `tools/line-metrics/src/doc_inputs.rs`）之 `face_line_factor("trebuchet ms")` 由 `1.3620` 改為 `1.1611`（**不動 `face_advance_scale`**）後：

- **我方 `cell-grid-span.docx` 垂直行距與分頁切點同步收斂至定錨 LO 真值**：
  - `Available at...` 段（10pt，`w:line=240`）我方行距由 `13.60pt` 降至 **`11.60pt`**（與定錨 LO 真值 `11.60pt` 誤差 `0.00pt`）。
  - Team 段（10pt，`w:line=276`）我方行距由 `15.63pt` 降至 **`13.53pt`**（與同檔 Calibri 11pt `13.43pt` 共箱量化為 7 帶 × `1.9333pt`，落入 K6 `13.30 ± 0.30pt` 容差帶內）。
  - **更正前次報告（`docs/e1c-advance-anatomy.md` §0#4／§5#4）關於「需動 `pack_run` 首行計帶才能解 `e1_pagination` 燈 1 p1 首末行盞」之誤判**：前次誤把 E1 最初依 Linux 未注入頂替態（`2p [33, 5]`，p1 首行 `For further media information, contact:` 共 5 行）寫下的舊斷言當作定錨真值；實際上在定錨真 face 態（Linux 注入真 face 與 Windows LO 皆為 **`2p [36, 1]`**）中，`For further...`、`Claire George`、`M:09833043394`、`L: 022-32229646` 全部落在 p0（p0 末行為 `L: 022-32229646`，`y0 = 702.97pt`），**p1 僅有 `E:redacted@example.invalid` 唯一 1 行（`y0 = 72.02pt`）**。當本卡將垂直尺重定錨為 `1.1611` 後，我方 p0 末行恰為 `L: 022-32229646`（`y = 713.87pt`）、p1 恰為 `E:redacted@example.invalid` 1 行（`y = 72.00pt`），**無須觸碰 `pack_run`，p0/p1 切頁點即與定錨 LO 真值 100% 重合**，且 `render_lab` 上 `cell-grid-span` 的 p0 內容分由 `0.991` 升至 **`0.999`**、p1 版面分由 `0.510` 升至 **`1.000`**、p1 內容分由 `0.542` 升至 **`1.000`**。
  - 據此更正 `core/engine-api/tests/e1_pagination.rs` 的 `lamp1_cell_grid_span_second_page_first_last_line` 期望值對齊定錨真值 `[36, 1]`，並解除其 `#[ignore]` 轉為 CI 常態綠燈（`e1_pagination` 6 盞全綠，0 ignored）。

### 2.3 零回退與爆炸半徑實測

- **`render_lab` 驗收板六檔 A/B（base＝1.3620 vs new＝1.1611）**：
  - `cell-grid-span`：頁數 `2＝LO 2`，`layout_scores` `[0.531, 0.51] → [0.531, 1.0]`，`content_scores` `[0.991, 0.542] → [0.999, 1.0]`（正向大幅提升）。
  - `fdo76591`（1p，layout `[1.0]`，content `[0.98]`）、`RA001-01`（6p，content `[1.0, 0.663, 0.896, 0.927, 0.997, 0.979]`）、`CoverPage`（2p，`[1.0, None]`）、`WC048`（1p，`[1.0]`）、`btlr-textbox`（1p，`[1.0]`）：**全部逐位元組零變動（零回退）**。
- **corpus A/B 發射樣式掃描（`ow-style-ab.py`，`OW_BASE_CLI`＝main 基線）**：`scanned=399/400 regressed_files=0 regressed_items=0`。
- **400 檔頁數爆炸半徑掃描（`blast_advance.py`，base＝1.3620 vs new＝1.1611）**：`scanned=400 failed=0 頁數改變檔數=0`。

---

## 3. 可重跑指令（PM 覆核用）

```bash
# 1. 重算跨環境可程式判定規則並產生 docs/e1cb-lo-truth-canon.json（讀已驗證快取，秒級完成）：
/usr/bin/python3 /tmp/agy-e1cb-scratch/verify_e1cb_canon.py

# 2. 若需從頭重跑 Linux LO 25.8（未注入＋注入連取 2 次）與 Windows LO 轉換：
/usr/bin/python3 /tmp/agy-e1cb-scratch/verify_e1cb_canon.py --run-lo

# 3. 考卷正常態與雙軸 BUGGY 回塞鎖鑑別力驗證：
cargo test -p quire-engine --test e1c_advance_trebuchet
K6_AUTO_PITCH_BUGGY=1 cargo test -p quire-engine --test e1c_advance_trebuchet      # 燈 1 必紅
K7_ADVANCE_SCALE_BUGGY=1 cargo test -p quire-engine --test e1c_advance_trebuchet   # 燈 2 必紅
cargo test -p quire-engine --test e1_pagination                                    # 6 passed; 0 ignored
```
