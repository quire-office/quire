# K16-diag — 三處殘留歸因：Bug51170 色分 0.66 ／ fdo68607 缺 `Regularity` ／ WC048 殘差清單

- 任務：t_641a1261（report-only；禁改 `core/`、`shells/`、`tools/`、`scripts/render_lab.py`，只準新增 `docs/` 報告＋`scripts/` 新檔）
- 基線：main = `38ba59c`（K8 merge 後）。分數出處 = 用戶既有掃描 `~/.hermes/cache/scratch/cmp-20261030/report.md`（LO=`/mnt/c/Program Files/LibreOffice/program/soffice.exe`，J3 三軸 150dpi）。本卡對三檔 LO PDF 全部**重轉重測**（`scripts/k16_probe.py lo2pdf`），Bug51170 p1 色分復現 **0.660**、WC048 色分復現 **0.072**、fdo68607 真缺字復現 `['Regularity']`，逐位吻合。
- 座標口徑：LO 側一律 pymupdf 實測；DPI=150（J3 同口徑），**pt = px×72/150**。LO 收場 `kill_own_lo`，交付前 `pgrep -c soffice` = 0（實測 0）。
- 量測腳本：`scripts/k16_probe.py`（新檔，唯讀）＋本報告文末「探針全錄」列出的 scratch 探針（探針引用 scratch 路徑，重跑前先跑 lo2pdf 重產 PDF）。

## TL;DR

1. **Bug51170 p1 起 0.66 不是色缺，是同一張頁首 logo 的編碼管徑差**。双方都畫同一張 logo（連通域 bbox (250,39)–(362,110)pt 全同、填充密度 0.30/0.32），AlternateContent 選支一致（LO SMask vs image2.png alpha **MAD=0.00**＝都走 Fallback PNG）。分數缺口 100% 集中在 logo **半透明環帶/邊緣像素**：LO 把 305×163 源 PNG **降採 146×78→JPEG(DCTDecode)＋SMask→光柵時升採 2.08×**，DCT 色振鈴造出我方沒有的暗紅/紫紅暗/黃綠暗小 bin；我方嵌原位元組、alpha 白合成使環帶偏亮（綠亮 0.00095 vs LO 0.00018）。管線模擬（降採→JPEG→升採）把分數帶推至 0.648–0.676，跨過 LO 真值 0.660——**判定：度量假影，非真缺陷**（整頁彩色覆蓋只差 8.8%，無色塊缺發；我方圖反而更高保真，MAD 4.74 vs LO 7.88）。
2. **fdo68607 缺 `Regularity` 是真缺字的假影——實際是軟性 `<w:br/>`（純換行）未實作**：該詞我方**有畫**（p5 SVG `<text>sound.Regularity in time…</text>`），但 LO 按 OOXML 語意在 `</w:t>` 前強制換行，LO 抽出成兩行 `…patterns of sound.`／`Regularity in time…`；我方 dump 拼成一整行 `sound.Regularity …`，token 化產出 `sound.regularity` 單 token，`token_audit` 對账時 `regularity` 在 LO 集有、我方集無，且拼接檢查只相鄰 2–3 段、救不回「黏在上一詞尾部」這型 → 誤報真缺字。**判定：度量假影（token 切分假影），非丟字**。修 `token_audit`（我方側對 token 內 `.`/大寫駝峰再切）或修引擎把 `<w:br/>` 落成硬換行（後者才是對 LO 的真收斂，corpus 尚有 24 檔、最多 133 個）。
3. **WC048 版面 0.858／色分 0.072 扣掉 K15（Textbox3 位置）、K12（填充色鏈）後**，色軸剩餘缺口按模擬排序：**(a) Rectangle 214 藍帶（accent1 實心，wpg 群組內第二子形狀）我方整體丟失 → 補回 0.072→0.271；(b) Textbox2 段落 pBdr 上下兩條 #4472C4 線未畫 → 再補 0.271→0.718**；(c) 白底框遮蓋順序與列高差（LO row1=206pt vs 我方 trHeight 98pt）為其餘 ≈0.2 零頭。版面軸 0.858 的殘差 100% 是可解釋的文字 y 位移＋display 序，零缺字。** merge 基線複核（§3.1）：(a)(b) 兩項在 merge main（K12 pBdr＋grpSp 子形狀落地）後已全部修復、色分 0.072→0.757——本卡原「另開功能卡」建議作廢，剩餘 0.18 缺口是線寬/抗鋸齒度量底噪＋LO 列高撐高行為，非結構缺項。

---

## 第 1 席：`corpus/poi/test-data_document_Bug51170.docx` p1–p6 色分固定 0.660（p0=1.0）

### 1.1 現象與定位

p0 色分 1.0（雙方皆無彩色），p1 起六頁一模一樣的 0.660。逐頁分解（重跑實測）：

```
$ python3 scripts/k16_probe.py color <LO pdf> <我方 p1 svg> --page 1
## 逐色 diff（DPI=150，bin=佔整頁像素比例）  score_color=0.660
彩色覆蓋率：LO=0.00473 我方=0.00515（COLOR_FLOOR=0.0005）
bin            LO       我方
綠暗            0.00334  0.00359
綠亮            0.00018  0.00095   ← 我方多發（缺口貢獻第一）
黃綠暗          0.00061  0.00008   ← 我方缺（缺口貢獻第二）
紅暗            0.00009  0.00001
紫紅暗/黃綠亮/紅亮/橙紅亮 …      ← 全部是小面積環帶色
Σmin=0.00393 Σmax=0.00596 → 0.6598
連通域：LO (250,39)–(362,110)pt 填充=0.30 ／ 我方 (250,39)–(363,110)pt 填充=0.32
```

- 唯一彩色源 = 頁首 logo：`word/header1.xml` 內 `<w:drawing>` inline 圖 `descr="Cnig logo.pdf"`，`<wp:extent cx="1854200" cy="990600"/>`（=146×78pt）。`<ve:AlternateContent>`：**Choice(rId1=image1.pdf，Mac 向量)／Fallback(rId2=image2.png)**。
- `titlePg` 宣告存在但全檔只有 default header——p0 是標題頁不吃 header、p1 起吃：LO 與我方**都**遵守（LO p0 頂部帶彩色=0.00000、我方 p0 SVG `<image>` 數=0；p1 起双方都有 logo）。**p0→p1 跳變不是缺陷**，probe_p0 實測釘死。
- 六頁逐位元組同分（Σmin/Σmax 完全相同）：同一張 logo 逐頁重複、內文全黑白——固定 0.66 是單一系統性來源，非六個獨立缺陷。

### 1.2 根因：LO 對同一張 logo 走「降採＋JPEG＋升採」管線，我方嵌原位元組——邊緣環帶像素落進不同 bin

LO PDF 側（`probe_smask.py`）：

```
xref54 物件： /Width 146 /Height 78 /ColorSpace /DeviceRGB /Filter /DCTDecode /SMask 97 0 R
p1 image54 rect(pt)= Rect(234.15, 35.40, 380.15, 113.40)
```

我方側（`probe_logo.py`）：p1–p6 SVG 各嵌 **image2.png 原位元組**（sha 逐檔相同），p0 無圖。
**AlternateContent 選支双方一致＝Fallback(image2.png)**：LO 的 SMask vs image2.png alpha **MAD=0.00**、vs image1.pdf alpha MAD=11.78（`probe_alpha_br.py`，逐 pix 對拍釘死）——LO 丟棄了 `ve:Choice` 的 Mac 向量支，我方也嵌 Fallback PNG，選支不是缺陷。

真正差在**編碼管線**：源 `image2.png` 原生 305×163；LO 把它**降採到 146×78 再 JPEG(DCT) 內嵌**（LO 輸出管線按 72dpi 網格化圖片），pymupdf 150dpi 光柵時又**升採 2.08×**——環帶（0<α<255）像素經半採样＋DCT 色度振鈴，LO 頁出現我方沒有的**暗紅/紫紅暗/黃綠暗**小 bin（JPEG 對飽和綠邊緣的典型色振鈴），而 alpha 合成的白底使**我方**環帶偏亮（綠亮 0.00095 vs LO 0.00018）。透明區裸 RGB 本身不進畫面（SMask 遮掉），但 DCT 擴散讓其環帶鄰域色偏綠（`probe_hidden.py`：LO 透明區 SMask<8 仍有 48% 非黑像素，我方 PNG 透明區 100% 全黑）。

管線模擬（`probe_pipeline_sim.py`：把我方 p1 logo 區降採→JPEG→升採貼回，重算 `score_color`）：

```
down=2 q=75 → 0.676   down=2 q=85 → 0.674   down=2 q=60 → 0.648   down=2 q=95 → 0.651   （LO 真值 0.660）
```

模擬帶（0.648–0.676）跨過 LO 的 0.660——0.34 缺口可由 LO 的「降採＋JPEG＋升採」邊緣管線解釋，**無色塊級缺項**。MAD 對拍（`probe_logo5.py`）佐證我方畫的是高保真源圖：我方頁 logo 區 vs image2.png 源白合成 **4.74**（我方=源圖忠實）、LO 頁 logo 區 vs 源 **7.88**（LO 有損耗）——我方渲染比 LO 更乾淨，反而被 bin 級交集扣分。

### 1.3 判定與處方

- **判定：度量假影**（0.34 缺口全部來自單一 logo 的環帶/邊緣像素在兩條編碼管線下落進不同 bin；Σmin/Σmax 對稀疏彩色頁把小色種差放大成分數差）。整頁彩色覆蓋率仅差 8.8%（0.00473 vs 0.00515），連通域 bbox 一致。**非真缺陷，引擎側無需修**。
- **度量側改進選項**（`render_lab.py`，本卡未動）：(a) 色相 bin 粗併（12→6 色相或色族併欄）——本席若整頁併成單一覆蓋欄，分數=覆蓋率比 0.00473/0.00515≈**0.92**，即結構上限；(b) 或對 sat∈[25,64) 且單 bin 面積 <0.05% 頁的環帶色折併入最近鄰大 bin。
- **紅燈值建議**：色軸單頁 <0.35 才記真缺陷紅燈；0.35–0.80 且彩色連通域 bbox IoU>0.9＋填充密度比 >0.8（本席即此型）→ 標「圖片編碼管徑色差」註記不記紅燈，轉由圖軸（0.916）背書。

---

## 第 2 席：`corpus/ooxmlexport/fdo68607.docx` 真缺字 `['Regularity']`

### 2.1 證據鏈

```
$ python3 scripts/k16_probe.py missing corpus/ooxmlexport/fdo68607.docx <LO pdf> --words Regularity
true_missing=['Regularity']  split_ok=['shaped', 'sharp-', 'edged', 'effective', 'contexts']
- byte 182716: 鏈 w:document > w:body > w:tbl > w:tr > w:tc > w:p > w:r > w:t
  run: <w:t>Regularity in time or space of an action, process or feature.</w:t>
  我方 dump 含 'Regularity'？ True
```

- OOXML 落點：表格 cell 內同段落兩 run：`<w:t>Measured flow … patterns of sound.</w:t>` `<w:br/>` `<w:t>Regularity in time …</w:t>`——**`Regularity` 不在被跳過的結構內，我方有收**（我方 p5 SVG 實存 `<text x="161.8" y="208.8">sound.Regularity in time or space…</text>`）。
- LO PDF p5：該兩行是分離的 line（bbox y=292.8 與 309.9，同 x=161.4）——LO 遵守 `<w:br/>` 硬換行，抽出 `Regularity…` 獨立行。
- 我方 dump：同一段落輸出行 `sound.Regularity in time …`（`probe_reg.py`），`tokenize` 產出 `('sound.Regularity','sound.regularity')` 單 token。
- `token_audit` 對账：LO 集有 `regularity`、我方集只有 `sound.regularity`；`_concats` 拼接救回範圍＝我方**相鄰 2–3 token 串接**，而 `sound`＋`Regularity…` 在我方是**同一 token**、不存在可拼的相鄰對，反向 LO 切片檢查也命中不了（我方端沒有切片）→ 判入 `true_missing`。
- 全檔軟性 `<w:br/>`（無 type）共 8 個（`probe_final2.py`：總 19、帶 type 11）；corpus 掃描 **24 檔含軟 br**（最多 bug65649 133 個、60329 85 個）——同型誤報在掃描面上會繼續出現，只是本輪 seed 未抽中其他檔。

### 2.2 判定與處方

- **判定：度量假影（token 切分假影）＋引擎小缺陷（軟 br 未實作）**。字沒有丟、有畫、位置對；缺的是 `<w:br/>`（ECMA-376 §17.4.1 純換行）落行。
- **引擎側修復點**：`core/ooxml/src/lib.rs` `b"br" =>` 臂（lib.rs:744）現只 `record_page_break`（type="page"）；軟 br 需在 run 文字串內插 `\n`（或記錄換行偏移），讓 `core/layout` 折行時視作硬斷點。影響面＝上述 24 檔，對 LO 抽字序、行數、版面分都有正收益。
- **度量側修息**（`render_lab.py::token_audit`，非引擎）：對我方 token 補一輪「`[a-z.][A-Z]` 駝峰切＋`.` 後切」的再切分集，同入 `ow_set`——可擋住同型誤報，不掩蓋真缺字。
- **紅燈值建議**：真缺字词長 ≥6 且首字母大寫、且 LO 側該詞所在行 bbox 恰在前一行末無間隔者 → 先標 `split_ok(soft-br 嫌疑)`，與第 2 節修復脫鉤。

---

## 第 3 席：`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx`（版面 0.858／色 0.072）剩餘清單

前提校核：LO 與我方文字**零缺字**（token 對账 0）；三個文字框文字双方都有。以下把 K15（Textbox3 位置）、K12（填充色鏈）認領項**排除在外**，只列「扣掉那兩項後還差什麼」，並給逐項模拟增量（`probe_wc_sim.py`，對 150dpi 光柵直接塗色重算 `score_color`）：

| # | 殘留項 | 歸屬 | LO 實據（pymupdf drawings，pt） | 我方現況（dump） | 模拟增量（色軸） |
|---|--------|------|-------------------------------|-----------------|------------------|
| R1 | **wpg:grpSp 群組第二子形狀 Rectangle 214 整塊丟失**：accent1 實心藍帶 | 新缺陷（非 K12/K15） | fill rect `[80.9,341.6,362.4,343.9]` fill=(0.267,0.447,0.769)=**#4472C4** | `#46 Rect` 只画了群組第一子形狀（Textbox3 白框 #F0F0F0）；第二子 `Rectangle 214`（`a:off y=9308386 chExt=3206750×9427131` 群組縮放座標）無任何輸出 | 0.072→**0.271**（+0.199，單項最大） |
| R2 | **段落 pBdr 未實作**：Textbox2 框內段落的上下 #4472C4 框線 | 新缺陷（非 K12/K15） | stroke line y=99.2／132.9（x 171.7–423.5）色 #4472C4；OOXML `<w:pBdr><w:top w:val="single" w:sz="24" w:space="8" w:color="4472C4"…/><w:bottom …/></w:pBdr>` | dump 零條對應 Rule（全檔 core 搜 `pBdr` 0 命中：`search_files b"pBdr"` = 0） | 0.271→**0.718**（+0.447） |
| R3 | Textbox2 文字未斜體 | 版面/字形軸零頭 | LO span `Calibri-Italic 12.0`（y=108.9–123.6） | dump `flags=2`（`render-trait` `FLAG_ITALIC=2`）**已帶斜體位**，LO 抽字寬差≈斜體寬差 | 0（色分無關；版面分 <0.01） |
| R4 | 表格列高：LO row1=206.1pt vs 我方 99.53pt（`trHeight=1961`=98.05pt 照宣告） | 待定（LO 撐高=行內物件 wrap 效應） | LO drawings 橫線 y=72.3／138.2／344.3／358.2；row0=66pt row1=206.1pt row2=13.9pt | 我方 Rule 網格 y=72／85.45／184.98／198.43（row0=13.45 row1=99.53 row2=13.45） | 色 0；版面分貢獻主要項（文字整體 y 序因此位移） |
| R5 | 白底框 z 序遮蓋：我方白框 #36 覆蓋區內暗像素 1962 vs LO 同窗 5356 | 屬 K15 域（不重覆歸因，僅存目） | LO 白框只蓋 `[72.2,86.9,251.9,107.4]` 小窗 | 我方白 Rect (54.2,86.4)185.9×110.6 大窗先發、蓋住表格橫線 | 色 ≈0（黑白灰不進色軸；屬版面/灰軸） |

**色軸 0.072 的完整算式**：LO 彩色覆蓋 0.00539 全數是 #4472C4 一系（青暗 0.00400＋青亮 0.00139）；我方只有 Textbox2 文字色＋两条 pBdr 漏畫後剩下的零星抗鋸齒（0.00039）。R1＋R2 補齊模擬到 **0.718**；其餘 0.28 缺口是 R1/R2 邊界像素的 bin 內位置差（青暗 vs 青亮取決於 LO 線條 y 圈點），已無結構性缺項——參考上界（把 LO 全部彩色像素直接蓋到我方頁）= **0.933**（非 1.0 即度量本身的位置敏感底噪）。

**不該由 K12/K15 處理的剩餘項清單**（本卡核心交付）：
1. **R1 wpg:grpSp 子形狀**：引擎目前只取群組第一子形狀（Textbox3 白框），第二子 Rectangle 214（藍帶）整塊丟。修復點建議：`core/ooxml` anchor→wpg 解析處（現只取 `wps:wsp` 首個；grpSpPr 的 chOff/chExt 子座標變換需一併落地，藍帶 local y=732.9pt/9427131×2349500→群組高換算後落 341.6pt，LO 實測即此值）。**這是 K12 填充色鏈的上游——色鏈修得再對，這塊形狀根本沒進 display list。**
2. **R2 `w:pBdr` 段落框線**：全 corpus 高頻特性，引擎零實作（grep 0 命中）。修復點：`core/ooxml` 段落直接格式收集（pPr 臂，lib.rs:756 旁）＋`core/layout` 行盒預留 space＋渲染發 Rule。
3. **R4 trHeight 對 LO 行內物件撐高**：我方照 `trHeight=1961` 排，LO 把 row1 撐到 206pt（內含錨定框 wrap 撐高，Word 行為）。此項與 K15 的 Textbox3 位置同根但**不同修復點**（K15 修框位置，本項是列高計算），若 K15 順手修列高請在卡內註明，避免雙改。

**紅燈值建議**：色軸 <0.30 且 LO 側彩色連通域 ≥1 塊而我方 0 塊 →「實心形狀丟失」紅燈（WC048 R1 即此型，LO blobs=[] 是因藍帶太細不成 blob，需以 drawings fill rect 覆核——建議把 LO `get_drawings()` fill 色面積直接入色軸對账，脫離連通域門檻）。

### 3.1 merge 基線複核（2026-10-30，PM 指示對齊）

本卡完成前 `git merge main`（91c2693：K12 pBdr＋wpg:grpSp 子形狀＋字距、K15 layoutInCell）進 worktree，零衝突、`cargo build -p quire-cli` 通過，並用 post-merge 渲染重測 WC048：

- **R1（Rectangle 214 藍帶）與 R2（pBdr 兩線）在 merge 後已双双落地**：dump 出現 `#4472C4` 三圖元——Rule y=99.2（pBdr top）、Rect fill 帶（藍帶）、Rule pBdr bottom，位置與 LO drawings 實測吻合；色軸 `score_color` 0.072 → **0.757**（`probe_postmerge.py`）。本報告 §3 表格按 cmp-20261030 基線歸因，兩項處方已由 K12 先行交貨，**本卡不另開功能卡**。
- 修正一處基線誤判：`git grep grpSp main -- core/ooxml/src/drawing.rs` 實存 grpSpPr/chOff 解析（drawing.rs:1106 起，註解「群組子形狀要按 grpSpPr off＋chOff 換算」）——先前 `search_files grpSp` 0 命中是搜尋面漏報（檔型 glob 限定所致），教訓：core 有無實作以 `git grep <main>` 為準。
- 剩餘缺口 0.757→0.933 上界的零頭歸因（`probe_blobs.py`/`probe_lstroke.py`）：**LO 的 pBdr/藍帶全走 stroke/fill 細線、`image_blobs`=[]，我方 pBdr 以 2.4pt Rule 實線入 blob（(164.2,97.4,423.4,109.9) fill=0.30）**——線寬與抗鋸齒路徑差使青暗/青亮 bin 配額不同；加 R4 列高差（LO row1 206pt vs 宣告 98pt）的 y 位移。屬度量位置敏感底噪＋LO 撐高行為，**無結構性缺項**，留給度量側（色軸改以 LO `get_drawings()` fill 面積對账）處理。
- Bug51170 post-merge 色分 0.660 逐位不變——§1 判定（度量假影）與 K12/K15 改動正交，如預期。

---

## 探針全錄（可重跑）

`scripts/k16_probe.py` 子命令（本卡唯一入庫新腳本）：
- `lo2pdf <docx…> --out <dir>`：LO 重轉 PDF（走 `render_lab.lo_to_pdf`＋atexit `kill_own_lo`）。
- `ooxml <docx> [--word W]`：sectPr/header/footer 引用、彩色宣告、詞定位＋父元素鏈。
- `color <lo.pdf> <ow.svg> --page N`：24 bin 逐色 diff＋缺口貢獻排序＋連通域 pt 座標。
- `missing <docx> <lo.pdf> --words W`：復現 `token_audit`＋詞在 document.xml 落點。
- `wc048 <docx> <lo.pdf>`：LO 視覺行（pt）vs 我方 display list 對照。
- `tbxdump <docx>`：anchor 段、trHeight、pBdr 清單。

scratch 探針（引用 `~/.hermes/cache/scratch/k16/`，重跑需先 `lo2pdf` 重產 PDF／`cmp-20261030` 我方 SVG）：`probe_pages`(逐頁色分解)、`probe_logo`(嵌入圖 sha 對账)、`probe_smask`(LO PDF 圖物件結構)、`probe_logo5`(logo 區 MAD 四方對拍)、`probe_branch`/`probe_alpha_br`(LO 選支＝Fallback，SMask MAD=0.00)、`probe_wc_sim`(WC048 模拟增量)、`probe_wcgeom`/`probe_wcET2`/`probe_wcgrp`(WC048 群組幾何)、`probe_final2`(軟 br 統計)、`probe_reg*`(Regularity 鏈)。

LO 殭屍清場實測：`pgrep -c soffice` → 0。
