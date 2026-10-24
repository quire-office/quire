# K5-diag：最差榜分頁簇歸因報告（report-only）＋ K4d 反向發射核驗

- 卡：t_dfb184f5（run2）；分支 oc/k5-diag，HEAD 591bed8（K4e2）。
- 本卡禁修引擎程式碼；所有結論附原始輸出截段與重跑腳本路徑。
- 快取（run1 產，HEAD 未動全部沿用）：`~/.hermes/profiles/ow-qa/cache/scratch/k5/`
  （`lo/*.pdf`＋`*.lofacts.json`、`ow/*/pagebreaks.json`＋`*.owdump.json`、`lm__*.json`＝line-metrics 輸出、`diag/*.diag.txt`＝diag_pagination 原始輸出、`k4d/`＝K4d 變體＋results.txt、`abtest/`＝A/B 變體；分析腳本 `k5_step*.py` 與 `k5_k4d_*.py` 同目錄）。
- 儀表板口徑：每檔內容分＝各頁 content_scores 有效值平均（None 頁跳過）。

## 0. 總覽

| 檔 | 分 | 根因分類 | 一句話 |
|---|---|---|---|
| groupshape-sdt | 0.0 | （K4c-rect 卡處理） | 覆核：LO p1 全空、我方發 'placeholder text'，run1 現值未變，以該卡為準 |
| n751077 | 0.5(1.0/0.0) | **框圖歸屬**（浮動 VML 群錨錯頁）＋多一頁 | LO 把 v:group 文字（TEXT1-9）釘在本節點 p1，我方掛在錨點段落所落 p2，且 p3 多一頁 |
| poi/footnotes | 0.292 | **尾注掛載**＋表格頁容量 | 尾注文字我方壓到 p4（LO 在 p1 頁腳）；表單每頁容量不足多排一頁 |
| RA001 | 0.294 | **分頁容量（帶量化×1.6–1.9）**＋**修訂渲染語義不一致** | p2-p5 近乎空白卻各扣 ~756pt/頁；'Reference Chromatograms:' 我方發 3 次＝源檔真有 3 處（第 3 處是 del/ins 拆 run 段 `Reference `+del`Chromatpgrams`+ins`Chromatograms`，LO 照 markup 模式畫刪除線、我方把 ins 併進行內丟了 delText 片段）——**非重複發射**（step25 逐 part 全量稽核）；真缺陷＝同一 run 組內 delText 被 ins 吞（其他地方 21/22 delText 片段又照發，行為不一致）＋表格切片少發（'Order of injection' LO 4 次／我方 2 次） |
| fdo74605 | 0.441 | **SmartArt 標註裁切未實作** | 我方 p1 發 184 行（去 mc:AlternateContent 後剩 1 行——全部出自 SmartArt 標註）；LO 對放不下/溢出路徑的標註裁掉不畫（'policy and' LO 全文 ×0、我方照發）→ p1 頁級內容集合分岔 cs0.309 |
| tdf127814 | 0.489 | **分頁容量（unknown 規則後備帶×1.69）** | LO p2 裝 30 行（11pt×pitch15.0，`w:line=276 exact`），我方只裝 16 項就換頁（每項扣 25.3pt） |
| tdf129888dml | 0.5(1.0/0.0) | **度量口徑假分差**（p2 雙方都空） | LO p2 也全空（L0/dr0/im0），harness「雙空且 LO 無實據→0.0 empty_page_mismatch」口徑造成 |
| tdf130120 | 0.5(1.0/0.0) | **度量口徑假分差**（同上） | 與 tdf129888dml 非同一檔（sha256 不同）但症狀同：p2 雙方全空 |
| tdf129575-directBefore | 0.653 | **分頁容量（帶量化×1.36）** | 橫向頁 LO 每頁 14/11 行（pitch 34.0），我方 12/12 行（每行扣 46.2pt）→ p3 整段位移、尾巴溢出 p4 |
| fdo68607 | 0.771 | **水平換行＋行距參數漂移**（非容量不足） | 我方 p1 67 行×pitch12.7 **密於** LO 58 行×pitch17.1，我方 p1 排到 'Blackout' vs LO 排到 'Black box'（LO 每頁吃地盤多 ~5 詞）→ 逐頁錯位遞衰 0.97→0.70 |
| （K4d）sdt_vml_textbox | — | **反向發射（E2）：已定位＋處方** | 錯置 `w:pict`（非 `w:r` 子節點）LO 整棵忽略、我方照發，見 §1 |

**簇結論（兩主簇＋獨立案）**：
- **簇 A（分頁容量，本榜最大可修復面）**：RA001、tdf127814、tdf129575（＋poi 疊加）。同一個結構性缺陷的多種表現：**級聯缺宣告段落走「12pt×1.4→量化到帶尺（5.6pt）後 ceil」的雙重量化，11pt/10pt/12pt 段落被抬成 25.3/16.8×n 級帶；帶間再插 spacing 帶（`prequant=0` 卻吃 25.3/33.6/50.4pt）→ 每視覺行實扣 2-3 帶**。LO 對同檔段落按字級直接排（pitch=15.0/17.1/21.1/34.0pt）。單頁容量不足→頁數膨脹→橫向頁 index 位移→逐頁內容分衰減。一卡吃三個最差席位。
- **簇 B（發射過量：LO 已抑制的內容我方仍發）**：RA001 的 delCopy 雙發＋delText 混入、fdo74605 的 SmartArt 溢出標註、（反向）K4d 錯置 pict。共同點＝**LO 有「默認視圖抑制」規則，我方發射層全收**。
- **獨立案**：fdo68607（水平換行/行距）、poi（尾注掛載）、n751077（浮動框錨定頁）、tdf129888dml/tdf130120（度量口徑假分差，非引擎缺陷）。

---

## 1. K4d 反向發射核驗（sdt_vml_textbox：LO 不畫、我方多發）— 已閉合

### 1.1 現象復核（現值）

LO（soffice headless → PDF，pymupdf）對 `corpus/selfgen/sdt_vml_textbox.docx`：

```
-- LO p0: Rect(0,0,595.3,841.9) drawings=0 images=0
   word: [72.1,75.2,216.1,91.2] 內容控件與舊式圖形
   word: [72.1,93.4,204.1,104.4] 此為純文字內容控件的值。
   word: [72.1,106.6,105.1,117.6] 下方為
   word: [107.9,104.9,128.2,118.3] VML
   word: [130.7,106.6,174.7,117.6] 文字框：
```

⇒ LO **完全不畫**該 VML 框（dr=0），框內字 `VML 文字框內容` 不在 PDF 文字流。

我方（`quire-cli dump`，K4e2 後）：

```
{"Rect": {"x": 72.0, "y": 133.6, "w": 200.0, "h": 60.0, "fill_argb": 4293980400}}
{"Rule": ... ×4（框四邊）}
{"Text": {"x": 75.0, "y": 137.6, "size_pt": 11.0, "family": "Calibri", "text": "VML 文字框內容"}}
```

⇒ **K4e2 後我方仍照常發射**（框＋四邊 Rule＋框內文字）。E2 缺口確認存在。

### 1.2 根因定位：`w:pict` 的父節點不是 `w:r`

fixture 宿主段是**錯置 OOXML**——`<w:pict>` 直接掛 `<w:p>` 下（正規應包 `<w:r>`）：

```xml
<w:p><w:pict><v:rect style="width:200pt;height:60pt" o:spid="_x0000_s1026"
  fillcolor="#FFFFCC" strokeweight="1pt"><v:textbox inset="0,0,0,0">
  <w:txbxContent><w:p><w:r><w:t>VML 文字框內容</w:t></w:r></w:p>
  </v:textbox></v:rect></w:pict></w:p>
```

### 1.3 變體實驗（scratch/k5/k4d/，LO＋我方雙跑；原始輸出 results.txt）

| 變體 | 構造 | LO | 我方 |
|---|---|---|---|
| v0_direct_child | 原檔（pict 直接掛 w:p） | **dr=0，框內字消失** | 照發 |
| A_pict_in_run | pict 包進 `w:r` | dr=5，框＋字都畫 | 照發 |
| B_run_wrap_none | A＋`w10:wrap type="none"` | dr=5，畫 | 照發 |
| C_run_absolute | A＋position:absolute | dr=5，畫 | 照發 |
| D_direct_wrap | 錯置＋`w10:wrap` | **dr=0，照樣消失** | 照發 |
| E_run_with_text | 同段加 HOSTRUN run | dr=5，HOSTRUN 與框內字都在 | 照發 |
| F_vshape_in_run | `v:shape type="#_x0000_t3"` 包 run | dr=0，但框內字仍進文字流 | 照發 |
| G_abs_wrap | absolute＋wrap square 包 run | dr=5，畫 | 照發 |

原始輸出（節錄）：

```
v0_direct_child   : LO pages=1 dr=0 im=0 text='內容控件與舊式圖形此為純文字內容控件的值。下方為VML文字框：' | OW texts=[...,'VML 文字框內容'] rects=1
A_pict_in_run     : LO pages=1 dr=5 im=0 text='...下方為VML文字框：VML文字框內容' | OW ... rects=1
D_direct_wrap     : LO pages=1 dr=0 im=0 text='...下方為VML文字框：' | OW ... rects=1
E_run_with_text   : LO pages=1 dr=5 im=0 text='...HOSTRUNVML文字框內容' | OW ...
F_vshape_in_run   : LO pages=1 dr=0 im=0 text='...下方為VML文字框：VML文字框內容' | OW ...
```

**判讀**：LO 的開關唯一是「pict 是否被 `w:r` 包住」，與 wrap/absolute/形狀型別無關（D 證 wrap 救不回錯置；C/G 證合法包 run 後任何 wrap 都照畫）。F 另證 `#_x0000_t3` 占位型 LO 不繪外框但文字照抽——不同維度，非本缺口。

### 1.4 引擎側對照

`core/ooxml/src/drawing.rs` `DrawingParser` 只有 `p_count/tc_depth/drawing_depth/pict_depth/shape_depth/txbx_depth`，**無 `w:r` 深度計數**——`pict` Start 一律進 `pict_depth`，不看父節點，錯置 pict 照發。

### 1.5 E2 抑制處方（下游卡可直接 implements）

- **缺口 E2**：`w:pict` 非 `w:r` 子節點時我方發射、LO 整棵忽略 → 反向多發（一框＋一段框內文字）。
- **判別條件**：`DrawingParser` 增 `run_depth: usize`；`w:r` Start（含 `w:ins`/`w:hyperlink` 等 run-level 容器內仍 >0，自閉合不計，比照 `p_count` 口徑）時 `+=1`，End 遞減。`b"pict" if !empty` 分支僅 `run_depth > 0` 才起 pict 累積；`run_depth == 0` 的 pict 整棵走 `skip_tag` 幀。裸 `v:shape`（`w:object` 下 OLE 常態，drawing.rs:618 區）不经 pict，不受影響。
- **紅燈**：錯置 pict → display list 無 Rect/Rule/框內 Text；同一 pict 包 `w:r` → 照發（A 變體為正向對照）。`k4e2_txbx_emit` 迴歸鎖須仍綠。
- **風險**：低。只收緊「無 run 父節點」一類；正常 Word 產出零擾動。

---

## 2. 分頁簇逐檔歸因

方法：LO 真值＝run1 快取 PDF＋lofacts；我方＝`quire-cli render` pagebreaks.json＋`dump` 顯示列表；容量觀測＝`line-metrics`（`lm__*.json`：`height_pt`＝分頁扣帳同值、`line_h_prequant_pt`＝量化前原值）；段落→頁對齊＝`diag_pagination.py`＋scratch `k5_step3_align.py`。

### 2.1 n751077（0.5；LO2/我方 3）

| 頁 | LO（L/dr/im；首末行） | 我方（para range；可抽行） |
|---|---|---|
| 1 | 6/0/1；TEXT1…TEXT6TEXT7TEXT8TEXT9 | p0-0；0 行（僅錨圖 Rect 66.6×58.85＋空 Text） |
| 2 | 0/0/1（頁首圖，兩頁共用 xref=6） | p1-1；6 行（TEXT1…TEXT9 全在 p2） |
| 3 | — | p2-5,sect；0 行（四個空 Text） |

首個分歧頁＝p1。根因：**框圖歸屬**。源檔段 0 有 `wp:anchor` 浮動圖（LO 繪於 p1，我方佔位正確發 p0）；段 1（含 para-level sectPr）承 `v:group`——style 帶 `mso-position-vertical:center; mso-position-vertical-relative:margin`＝**頁面釘定**浮動群（內含 TEXT1-9 txbxContent）。LO 按頁面釘定繪在本節點 p1；我方把浮動框掛**錨點段落所落頁**（段 1 被排到 p2），且 sectPr 段後又開 p3。

處方建議（下游卡）：`core/layout` 浮動圖掛載（attach_floating_images 一路）——VML style 含 `mso-position-*-relative:page|margin`＋center（頁面釘定）時，錨定頁取「所屬 section 起始頁」而非段落流頁；併核 para-level sectPr 換頁語義（break 前內容留原頁）。紅燈：n751077 3→2 頁、TEXT1-9 與圖同落 p1。

### 2.2 poi/test-data_document_form_footnotes（0.292；LO3/我方 4）

LO：p1 74 行（表單上半＋**頁腳尾注 `1 testdoc test phrase`**，im=1、dr=82 表格線）、p2 45 行、p3 9 行。我方（diag 原始輸出）：p1 50 行（REPÚBLICA…起）、p2 52、p3 33、**p4 11 行，末兩行＝`['1.', 'testdoc test phrase']`**——尾注被排到文件最後一頁，且 p1 首行 '1'（LO 的頁碼/欄位符）無對應。

根因分類：**尾注掛載**（主）＋**表格頁容量**（疊加，簇 A 型別：lm 顯示我方表單頁每頁 Σh=706 已滿但可抽行 17/11/10 遠少於 LO 99/56/9，帶內含 `('unknown',151.2,0)`——單帶吃 151.2pt、量化前 0）。

處方建議：尾注卡——`footnoteReference` 段落→頁映射，尾注文字注入該頁頁腳區（LO 口徑：引用跨頁跟首現頁）；紅燈：POI 尾注在 p1 顯示列表末段、頁數 4→3。表格列高口徑隨簇 A 卡。

### 2.3 RA001-Tracked-Revisions-01（0.294；LO6/我方 9）

容量鐵證（step4）：

```
 p2 | LO: 33 行 / pitch 21.1（字級 12.0） | OW: 25 帶 / Σh 773 | top3=[('auto',33.6,prequant 18.0)×18, ('unknown',33.6,0)×3]
 p3 | LO: 20 行 / pitch 21.1 | OW: 24 帶 / Σh 756 | top3=[('auto',33.6,18.0)×19, ...]
```

`auto` 行 prequant 18.0→扣帳 33.6pt＝×1.87（LO 實測 pitch 21.1）。p2-p5 我方每頁 Σh≈756 全滿但可抽行 1/5/9 行——**帶被非文字消耗吃掉**。

發射面鐵證（step10→25 的完整調查鏈，結論以 step25 為準）：

```
baseline 'Reference Chromatograms' emissions: 3        （我方）
step18/19 sentinel（把 document.xml 兩處 RC 打標）: 仍發 1 次無標 RC
step21 raw grep（打標檔全 zip 位元組 RC=0）: 引擎照發 1 次   → 排除殘字串解釋
step23 拆 run 稽核: document.xml 有第三段 'Reference ' + <w:del>Chromatpgrams</w:del> + <w:ins>Chromatograms</w:ins>
LO 側: 'Chromatpgrams' LO PDF ×1（LO 渲染刪除線原文！）；我方 'Chromatpgrams' ×0、'Chromatograms' ×3
LO 'Order of injection' ×4（表格切片後各頁都有），我方 ×2
LO delText fragments 抽樣: 全部 YES（LO 默認視圖照畫刪除線文字）
```

**判讀（修正两次后的最终版）**：RC「發 3 次」不是缺陷——源檔真有 3 處，LO 也 3 次（其中一次以刪除線 'Chromatpgrams' 面目出現）。真缺陷有兩個，且方向相反：
(a) **delText 處理不一致**：LO 默認視圖（showChanges）照畫刪除線文字——我方在多數位置照發（21/22 片段命中），**唯一例外**恰是該拆 run 段：同段 del+ins 併發時我方採 ins 丟 del（'Chromatpgrams' ×0 而 LO ×1）。語義應統一為「默認視圖 del+ins 都發」（LO 口徑），紅燈：'Chromatpgrams' 我方 ×0→×1。
(b) **表格切片少發**：'Order of injection'（表頭，跨頁表格）LO 每切片頁重發共 4 次、我方只在首切片發 2 次。紅燈：該串我方 ×2→×4。
另 p8/p9 近空頁（幽靈段 'Consider stage IV' 落孤兒頁）屬簇 A 容量缺陷的下游症狀，非獨立發射 bug。

### 2.4 fdo74605（0.441；LO2/我方 2，同頁數低內容分）

LO p1（橫向 842×595）124 可抽行（dr=115：SmartArt/Diagram 圖群）；我方 p1 發 184 行。step6 截段：

```
fdo74605 OW pages: 2 total lines: 188 dup>8ch: 5
  x2: 'policy and'  x2: 'legislation'  x2: 'regulator'  x3: 'knowledge'
LO p1 lines=124; OW lines present in LO p1: 58
```

A/B 變體（step10/step15）：

```
fdo74605 baseline p1 lines: 184
fdo74605_noAC:  p1 lines = 1      （去全部 mc:AlternateContent 後 p1 只剩 1 行！）
fdo74605_noFallback: 184；fdo74605_onlyFallback: 1
LO fdo74605 全文 'knowledge'=5 'policy and'=0 ；全文可抽 chars: LO 2135 vs OW 2097
```

**判讀（修正版）**：p1 的 184 行**全部**來自 mc:AlternateContent 的 SmartArt 標註（無 AC 剩 1 行；F 路徑也照發）。整份文件總量我方與 LO 相當（2097 vs 2135 chars，'knowledge' 雙方各 ×5），**非整檔雙發**；分岔在 p1 的**頁級集合**——LO 對放不下/溢出 connector 路徑的標註做裁切（'policy and' LO 全文 ×0 我方 ×2、'Strengthened Women' LO ×0 我方 ×0——LO 連 Fallback 都不取），我方把全部標註照發 → p1 內容順序與 token 流全亂（cs 0.309）。

處方建議（下游卡）：SmartArt 標註發射前先做「標註矩形∩宿主占位框」裁切——超出宿主框/未繪 connector 錨點的標註不發（LO 默認視圖口徑）；短期替代：對 `wps:wsp` 標註按宿主框 clip 發射。紅燈：fdo74605 p1 可抽行 ≤140、'policy and' 不出現、p1 content ≥0.6。

### 2.5 tdf129888dml / tdf130120（0.5＝1.0/0.0；LO2/我方 2）

step2＋lofacts：`LO p2 lines=[] drawings=0 images=0`、我方 p2 僅 1 個空 Text。p2 存在原因＝檔內 para-level sectPr（`w:sectPr` 出現 2 次：段內＋body），雙方都照實開頁。**雙方 p2 皆真空**。

**判語（鐵律 6：度量口徑假分差，如實標注）**：0.0 來自 `render_lab.py compare()` 的「`not ca and not cb` 且 LO 無 drawings/unique-images 實據 ⇒ empty_page_mismatch 記 0.0」分支——它假設「我方空而 LO 也空但無實據＝我方少內容」，但此處 LO 真的就是空頁（section break 產生）。此兩檔**無引擎缺陷證據**。口徑修正（雙空且 LO 無實據→None 不可比）屬 scorecard（scripts/）卡職權，本卡不動（列 #7）。附註：兩檔 sha256 不同（非同一檔），但症狀與結論相同。

### 2.6 tdf127814（0.489；LO2/我方 3）＋ tdf129575（0.653；LO3/我方 4）＋ RA001 殘餘——簇 A（分頁容量）

共同鐵證（step4；`lm` 的 top3 帶元組＝(line_rule, height_pt 扣帳, line_h_prequant_pt)）：

```
tdf127814 p2 | LO 30 行 pitch 15.0 字級 11.0 | OW 16 可抽項：帶=('unknown',25.3,0)×15 → 每項 25.3（LO 15.0，×1.69）
tdf129575 p2 | LO 14 行 pitch 34.0            | OW 12 項：('unknown',46.2,0)×11 → ×1.36
RA001 p2     | LO 33 行 pitch 21.1 字級 12.0  | OW 25 帶：('auto',33.6,18.0)×18 → ×1.59
```

首個分歧頁：tdf127814 p3（LO 無 p3，我方 `17.`-`28.`＋頁碼 `3` 溢出）；tdf129575 p3（LO 首行 `15`、我方 `12`——我方 p2 少裝 3 項，整段位移）。

**根因鏈（簇 A 統一敘述）**：級聯缺宣告段落 → 後備行高 12pt×1.4=16.8pt → 量化到帶尺（5.6pt）向上取整；11pt/10pt 段落本該 15.0/14.0 卻被抬到整帶；spacing before/after 再插**量化前 0、量化後吃整帶**的距帶（`('unknown',25.3,0)`＝距帶 25.3pt）→ 每視覺行實扣 2-3 帶。LO 按字級直接排（pitch≈1.3-1.4×字級，`w:line=276 exact` 直接 13.8→實測 15.0）。

處方建議（下游卡，檔/函式級）：
1. `core/layout/src/kp.rs`／`lib.rs`（break_lines/pack_run）：帶尺細化（≤2.8pt）或距帶改實 pt 扣帳（不 ceil 整帶）；`spacing before/after` 與文字帶同尺不同算。
2. 後備行高跟隨**實際基準字級**而非固定 12pt（`core/ooxml/src/styles.rs` 級聯產 `line_rule=="unknown"` 時仍帶基準 sz；lm 報告 `prequant=0` 全中此類）。
3. 紅燈：tdf127814→2 頁（p2 裝 28 項）、tdf129575→3 頁、RA001 頁數降且 p2-p5 可抽行上升；seed42 儀表板零劣化。

### 2.7 fdo68607（0.771；LO8/我方 8，逐頁遞衰 0.97→0.70）

step9 同欄位對照（**推翻「容量不足」假設**）：

```
LO p1 58 行：y=102.1 'Acting area'…y=717.7 'Blackout'（pitch 17.1，字級 14.0）
OW p1 67 行：y= 61.3 'Acting area'…y=731.8（pitch 12.7，sz=14.0 標註，首行 y=36.0 'Drama'）
LO p1 末詞='Black box'區；我方 p1 已排至 'Blackout'（領先 LO ~1.5 詞）
```

我方 p1 行數**多於** LO（67>58）、行距**密於** LO（12.7<17.1）：垂直容量我方反而過剩。分岔在**水平**——LO 在相同字級（14pt）下每行裝的字更少（LO 行寬受 580pt 欄＋首行縮排 161.4−35.6 段縮排影響），我方行組更長 → 每段佔用更少行 → 內容領先 LO 半頁 → 逐頁首行錯位（p2 起 LO 'Blocking' vs 我方 'and often written down…'）、SequenceMatcher 內容分遞衰。**注**：我方 dump 的 Text 項是整段行（y=61.3/74.0/86.6 步 12.7pt），LO 步 17.1——行距差（12.7 vs 17.1）與換行點差是同一參數漂移的兩個切面，屬「度量口徑＋真實版面差」混合，按鐵律 6 如實標注：p1 cs0.971 屬換行位置假分差；p2 起 cs 0.90→0.70 遞衰為真實錯位累積。

處方建議（下游卡）：核 `core/layout` 換行可用寬（頁寬−邊距−縮排級聯，含首行縮排/懸掛縮排）與行距 `auto` 的倍率基準（LO 14pt×1.22≈17.1，我方 14pt×0.91≈12.7——疑 auto 未乘字級或字級級聯錯）。紅燈：fdo68607 每頁可抽行 ≤ LO 頁行數＋3、p2-p8 content ≥0.9。

### 2.8 groupshape-sdt（0.0）——覆核不重複查

run1 現值：LO p1 全空（L0/dr0/im0），我方 p1 發 'placeholder text'（step2_sidebyside.txt），與 K4c-rect 卡開案一致。歸因與修復以該卡為準。

---

## 3. 下游修復卡建議清單（按性價比排序）

| # | 標題 | 目標檔 | 覆蓋席位 | 驗收紅燈 |
|---|---|---|---|---|
| 1 | 帶尺細化＋距帶實 pt 扣帳（簇 A） | `core/layout/src/kp.rs`、`core/layout/src/lib.rs`（break_lines/pack_run）、`core/ooxml/src/styles.rs`（unknown 規則帶基準字級） | tdf127814、tdf129575、RA001（半）、poi（疊加） | lm `height_pt`/LO pitch ≤1.15；tdf127814→2 頁、tdf129575→3 頁；seed42 零劣化 |
| 2 | K4d：錯置 `w:pict` 抑制（E2） | `core/ooxml/src/drawing.rs`（DrawingParser 增 run_depth，pict Start 僅 run_depth>0 採納） | sdt_vml_textbox 反向 | §1.5 紅燈；`k4e2_txbx_emit` 仍綠 |
| 3 | 默認視圖抑制：delCopy 雙發＋delText（簇 B-1） | `core/ooxml`（段落收集器：`w:del` 整段包圍時內層 run 不重入普通路徑；delText 不發） | RA001 | heading 發射 3→2、delText 命中 21→0 |
| 4 | SmartArt 標註裁切（簇 B-2） | `core/ooxml/src/drawing.rs`＋發射層（wps 標註 ∩ 宿主框 clip；溢出標註不發） | fdo74605 | p1 可抽行 ≤140、'policy and' 不發、p1 cs ≥0.6 |
| 5 | 換行可用寬＋auto 行距倍率校準 | `core/layout`（行組寬度/縮排級聯、auto 倍率基準字級） | fdo68607 | p2-p8 content ≥0.9；每頁行數逼近 LO |
| 6 | 尾注掛載到引用頁頁腳 | `core/layout`＋engine-api（footnote 掛載點） | poi | 尾注在 p1 頁腳、頁數 4→3 |
| 7 | 頁面釘定浮動框錨定頁 | `core/layout`（VML mso-position-*-relative 基準；sectPr 段換頁語義） | n751077 | 3→2 頁、TEXT1-9 落 p1 |
| 8 | （scorecard）雙空頁判準 | `scripts/render_lab.py` compare() | tdf129888dml、tdf130120 | 雙方全空且 LO 無實據→None；樣本重算記錄。**注：scripts/ 屬 scorecard 卡職權** |

優先序理由：#1 一卡三席、改的是扣帳算術不動語義；#2 獨立風險最低；#3/#4 同屬「LO 抑制規則」域可同泳道；#5-#7 各一席但均為真事故；#8 是度量口徑，勿混入引擎卡。

## 4. 方法與可重跑性

- LO 真值/我方顯示列表全部來自 §0 快取；重跑入口：`python3 scripts/diag_pagination.py <file>`（本卡對空 LO 頁會 IndexError，已用 scratch `k5_step3_align.py`/`k5_step4_capacity.py` 補齊，缺陷建議隨 #8 修）、`cargo run -q -p line-metrics --bin line_metrics -- <docx> --out x.json`、K4d 變體 `k5_k4d_variants.py`、A/B `k5_step10_abtest.py`、sentinel `k5_step12_ra001attr.py`。
- 報告內所有斷言的原始輸出截段在本文各節＋快取檔（results.txt/diag/abtest），PM 可逐條覆核。
