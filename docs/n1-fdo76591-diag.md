# N1-fdo76591：最差榜第 1 引擎回退歸因報告（report-only）

- 卡：t_7b43f12e（分支 oc/n1-fdo76591，基線 fb06767＝main 最新 merge）；本版生成：2026-10-25。
- 本卡零引擎觸碰（`core/`、`shells/`、`tools/`、`scripts/` 未改；bisect 全走 /tmp 唯讀 worktree）。
- 原始輸出快取：`~/.hermes/profiles/ow-qa/cache/scratch/n1_probe/`（`lo/fdo76591.pdf`、
  `ow_main/`、`ow_b6b3dc5/`、`ow_591bed8/`、`ow_b59a25e/`＝各 SHA render、`dump_*.json`＝
  dump、`z1–z7.py`/`lo_truth.py`/`lo_detail.py`/`kinds.py`＝分析腳本，全部可重跑）。

## 0. 一句話結論

**回退鎖死在單一 commit `591bed8`（K4e2 框內段不佔槽），K4c-image（ea92bdd）無罪排除。**
本檔全部可見文字住在唯一宿主段落內第三個 VML 文字框（整頁背板框）的 `txbxContent` 裡；
K4e2 把框內文字移出正文流後，三個框以 `InlineImage` **正文流佔帶**身份發射（VML
`position:absolute` 的 margin-left/margin-top 絕對定位完全沒被解析→不是浮動），
帶寬合計 6+3+44=53 帶＞每頁容量 42 帶，567×733.65pt 的大框（44 帶）被自己撐到第 2 頁，
它的 36 段框內文字跟著佔位框落 p1——p0 只剩零文字的佔位結構，p1 全內容，兩頁皆 0.0。

## 1. 現象復現（本卡實測，非引用）

### 1.1 LO 真值（Windows soffice，pymupdf 實讀）

```
soffice: /mnt/c/Program Files/LibreOffice/program/soffice.exe
pages: 1
-- LO p0: rect=Rect(0.0, 0.0, 612.0, 792.0) lines=30 drawings=12 images=3
   'BELVIDERE ASSEMBLY PLANT' / 'World Class Manufacturing Starts Here'
   / 'IMPORTANT NOTICE ' / 'Scheduled Saturday Overtime' / ... 末行 '3-23-2011'
```

LO 頁面對象細節（`lo_detail.py`）——三個框都以**絕對定位**畫在頁上，且三張圖都光柵化：

```
DRAW rect= [256.0, 590.5, 356.0, 690.7]   ← 框1：margin-left 220.05+36=256.05, margin-top 554.45+36=590.45
DRAW rect= [193.8, 729.2, 418.8, 774.2]   ← 框2：157.8+36, 693.2+36
DRAW rect= [22.1, 18.2, 589.0, 751.8]     ← 框3（背板，stroked）：-13.95+36, -17.8+36（w567/h733.65）
IMG xref 10 w490 h100 / IMG xref 54 w750 h122 / IMG xref 58 w166 h183   ← EMF×2＋PNG 全光柵化
```

⇒ LO 對 VML `position:absolute;margin-left/margin-top` 的換算＝**頁邊距原點＋偏移**；
負 z-index 大背板框的框內文字被 LO 當作正文流排出（1 頁 30 行）。

### 1.2 我方（fb06767 render＋dump）

```
render: → 2 頁 SVG ＋ pagebreaks.json（1 段落）
pagebreaks：p0 first_para=0 last_para=0 line_count=1；p1 line_count=0 continues_paragraph=true
kinds(dump)：page0 {'Rect':2,'Rule':8,'Text':3}   page1 {'Rect':1,'Rule':4,'Text':64}
p0 Rect：(36,36,99.95×100.2)、(36,136.8,225×45)      p1 Rect：(36,36,567×733.65)
p0 三個 <text> 全空字串；p1 64 run＝全部正文（BELVIDERE 在 p1，3-23-2011 亦在 p1）
<image> 計數＝0（p0/p1 皆 0）
```

與 PM 卡面實證逐位一致。行網格帶尺 `line_h=16.8pt`（12pt×1.4）：p0 兩佔位框 y 差
136.8−36=100.8=6×16.8 ⇒ 框1 帶數=⌈100.2/16.8⌉=6、框2=⌈45/16.8⌉=3、
框3=⌈733.65/16.8⌉=44；版心 720pt/16.8⇒容量 42 帶。

### 1.3 容器歸因（鑰匙，zipfile＋解析器雙實證）

全檔 `w:p` 40 個＝**body 級僅 1 個**（宿主段，`w:ind left=-1080 right=-1260`、3 個空 run），
其餘 39 個全在三個 `w:txbxContent` 內（38 個成對段＋1 個自閉合空段；
框1＝1、框2＝1、框3＝36 段完結，合併掛宿主段＝38，見 §1.4）：

| 框 | shape id | style（絕對定位） | z-index | txbx 段數 | 框內圖 |
|---|---|---|---|---|---|
| 1 | _x0000_s1039 | margin-left:220.05pt; margin-top:554.45pt; 99.95×100.2 | +251658752 | 1（空） | wp:inline rId6=image1.emf |
| 2 | _x0000_s1036 | margin-left:157.8pt; margin-top:693.2pt; 225×45 | +251657728 | 1（空） | wp:inline rId7=image2.emf |
| 3 | _x0000_s1026 | margin-left:-13.95pt; margin-top:-17.8pt; 567×733.65 | **−251659776（背板）** | **36（全文字）** | wp:inline rId9=image3.**png** |

**全部 99 個可見 `w:t` run（BELVIDERE…3-23-2011，32075B 框內文）都在框 3 的 txbxContent。**
零顯式分頁指令複驗：`w:br type=page`=0、`lastRenderedPageBreak`=0、`pageBreakBefore`=0、
`sectPr`=1、`w:tbl`=0；兩頁皆 612×792 無橫向頁。⇒ 我方第 2 頁純幾何/扣賬造成，坐實。

### 1.4 解析層鐵證（`quire-ooxml` examples probe，交付前已 rm）

```
== read_images candidates: 3
  #0 para=0 w=99.95 h=100.20 floating=false in_cell=false rid=None rids=[] txbx_texts=1
  #1 para=0 w=225.00 h=45.00 floating=false in_cell=false rid=None rids=[] txbx_texts=1
  #2 para=0 w=567.00 h=733.65 floating=false in_cell=false rid=None rids=[] txbx_texts=36
== paragraphs: 1
  para#0 runs=3 text="" txbx_texts=38 txbx_runs=38
      txbx[0] "" / txbx[1] "" / txbx[2] "BELVIDERE ASSEMBLY PLANT" / ...
```

三個要點全在這裡：
1. `floating=false`＋無 pos 座標 ⇒ VML `position:absolute;margin-left/margin-top`
   根本沒進解析（`drawing.rs` 只解析 DrawingML `wp:positionH/V` 的 posOffset，
   `vml_style_wh` 只取 w/h）→ `build_images` 走 `anchor=None` 佔帶支路。
2. `rid=None rids=[]` 而源檔明明有 blip（rId6/rId7/rId9）⇒ **`w:pict` 內嵌
   `w:drawing` 被 VML 分支整體吞沒**（`drawing.rs on_start` 的
   `if self.pict_depth > 0 || self.vml.is_some()` 分支 match 無 `b"drawing"` 臂，
   落入 `_ => {}` 後 `return`，內層 `wp:inline`/`a:blip` 一概不看）——
   rId9=image3.**png 可渲染格式被解析層丟 rid，K4c 像素池無從談起**（不是發射層漏發）。
3. 宿主段 `txbx_texts=38`＝三框文字**合併**掛同一段（1+1+36，框歸屬丟失），
   而 `shape_boxes` 池用的是 `read_images` 的**逐框** `txbx_texts`（36）——
   `emit_txbx_texts` 的 runs 卻取自 `paras.get(im.para).txbx_runs`（合併 38 清單）
   ⇒ **文字列（36，框3口徑）× run 列（38，合併口徑）錯位兩位**。
   實據（p1.svg）：`BELVIDERE ASSEMBLY PLANT` 發兩遍——line 8 整串 12pt 素體
   （text[0] 配到框1 的空段 run→無 size→12pt 基準）＋ lines 13–19
   `B`20pt/`ELVIDERE`14pt bold（text[2]="" 空段臂照 own_runs 把框3 真 run 又發一遍）；
   `IMPORTANT NOTICE` 同樣雙發（14pt bold＋16pt 白字）。64 run 裡的重複/錯樣即此。

## 2. 回退定位（唯讀 bisect，主 checkout 未碰）

| SHA | 內容 | render 結果 |
|---|---|---|
| `b6b3dc5`（K4a，K4e2 前） | 純文字框不採納、文字留正文流 | 2 頁、**39 段落**；p0 34 text（BELVIDERE 在 p0）、p1 18 text、零 Rect |
| `591bed8`（K4e2） | 框採納＋文字移入 txbx_texts | 2 頁、**1 段落**；p0 {Rect2,Rule8,Text3空}／p1 {Rect1,Rule4,Text64} |
| `b59a25e`（K5-diag merge，零引擎改動） | — | 與 591bed8 同 |
| `fb06767`（main） | K4c-image＋M2 口徑在其上 | **SVG 與 591bed8/b59a25e 逐字節相同（diff 空）** |

⇒ **回退引入點＝`591bed8`（K4e2），進 main 的載體 merge＝`b59a25e`；
`ea92bdd`（K4c-image）對本檔零增量（其前已壞）——嫌疑 2 的「發射側改動」部分證偽。**
（`git worktree add --detach /tmp/bisect-* <sha>`，用完已 rm。）

### 2.1 頭號嫌疑逐條裁決（卡面四大嫌疑）

1. **K4e2「框內段不佔槽」——證實（主因）**，但機理比卡面假設更精確：正文流不是
   「剩三個 pict 段」，而是**只剩 1 個宿主空段**（三 pict 同段）；文字不是「被推到 p2」，
   是**跟著 44 帶的佔位框被撐到 p2**（佔帶纔是推手，見嫌疑 3）。
2. **K4c-image 真像素發射——證偽（主因不在此）**；但拆出**伴生缺陷**：pict 內嵌
   drawing 的 blip rId 被解析層吞（§1.4 之 2），可渲染的 image3.png 從未進像素池。
   p0 佔位框仍發射＝屬實；`<image>`=0 的根因是**丟 rid**，不是 EMF 退佔位規則。
3. **帶制扣賬自撐一頁——證實（回退的直接推手）**：框3 h=733.65pt→44 帶＞容量 42，
   巨型圖豁免讓它獨佔 p1。M1-band 同族（量化扣賬 vs 絕對定位語義錯配）。
4. **假因排除**：橫向頁 index（兩頁皆 612×792）已排除；「整段挪頁」判據不適用
   （LO 僅 1 頁）。

### 2.2 歷史分數勘誤（與卡面/既往報告矛盾處，如實點名）

- 卡面「K5-diag 時期此檔 0.441、LO2/OW2 同頁數」**不確實**：`docs/k5-diag-pagination.md`
  §0 的 0.441 屬於 **fdo74605**（SmartArt 裁切），該表**不含 fdo76591**。
  本檔歷史分 **0.428**（`docs/lo-baseline-2026-10.md` 最差榜第 2，LO 2 頁/我方 2 頁）。
- `docs/m2-scorecard-recount-2026-10.md` 自身兩處口徑不一：§1 寫「0.843→0.000」、
  §4 寫「0.428→0.000」。以在庫兩份文檔為準：0.428 有據（lo-baseline），0.843 無出處，
  建議 PM 順手補正 M2 §1 措辭（非本卡職權）。
- LO 側 2→1 頁屬外部漂移（M2 已如實記錄），與本卡歸因無關；我方 p1 全空是我方側。

## 3. 根因鏈（完整因果，每環附證據位）

1. 【解析】VML `position:absolute;margin-left/margin-top` 不解析（`drawing.rs`
   `vml_style_wh` 只取 w/h；pos_x_pt/pos_y_pt 僅 DrawingML posOffset 路徑）→
   三框 `floating=false`、無鎖點（§1.4-1；LO 實證幾何 §1.1 對照我方 (36,36) 系佔帶位）。
2. 【版面】`build_images`（`table_map.rs:476`）`anchor=None` →
   `bands=⌈h/line_h⌉`：6+3+44=53 帶 > 每頁 42 帶；正文流僅宿主空段 1 行
   （K4e2 移走框內文字，`591bed8`）→ packer 在框3（44 帶）處換頁，巨型豁免落 p1。
3. 【發射】`emit_txbx_texts` 隨佔位框落 p1 發 36 段全部文字（p1.svg 64 run）；
   runs 取宿主段合併清單 38 → 兩位錯位 → 標題類內容雙發＋錯樣式（§1.4-3、p1.svg lines 8/13–27）。
4. 【像素】pict 內嵌 drawing 的 blip 被吞 → rid 全丟 → image3.png 不發（K4c 該型漏網）；
   EMF×2 本就退佔位（LO 光柵化了它們，屬既知近似，非本檔主分差）。

## 4. 檔/函數級修復處方（下游卡可直接 implement）

**P1（必修，吃滿本檔分差）— `core/ooxml/src/drawing.rs`：VML 絕對定位入錨點。**
`on_start` 的 VML 分支（`is_vml_shape` 開標處，現調 `vml_style_wh` 取 w/h）擴為：
讀 `position:absolute` 旗標＋`margin-left/margin-top:<n>pt`（本檔實證格式；
`mso-position-*` 變體留註釋不擴）→ `pos_x_pt/pos_y_pt`＝偏移值、
`pos_x_rel/pos_y_rel="margin"`、`floating=true`（僅當 `position:absolute` 在場——
VML 內嵌圖無此旗標 → 零行為變化，K4a/K4c 語料零擾動）。
`table_map.rs::build_images`／`resolved_anchor`／`origin_of("margin")` 鏈路已就緒
（margin 系→版心左上，與 LO 實證原點一致 §1.1），**無需改**；`attach_floating_images`
浮動路徑已發 Rect＋`emit_txbx_texts`（`engine-api/lib.rs:1536` 浮動臂）——P1 落地上面
三框自動變浮動：零佔帶、絕對座標、文字落框。本檔預期：1 頁、背板框＋(22.05,18.2)、
框1/框2 於 (256.05,590.45)/(193.8,729.2)。

**P2（必修，正確性）— 逐框 run 列。** `Pending`/`InlineImage` 增
`txbx_runs: Vec<Vec<Run>>`（serde default，舊快照兼容；K4e2 已在 Pending 採
`txbx` 字串列，同點位補 run 列即可），`emit_txbx_texts` 改傳 `box_im.txbx_runs`
而非 `paras.get(para).txbx_runs`（`engine-api/lib.rs:1529/1565/2796` 三處呼叫）。
修掉雙發＋錯樣式；合併的宿主段 `txbx_texts` 保留（round-trip 無關，讀投影無害）。

**P3（應修，像素補齊）— pict 內嵌 drawing 的 rid 透傳。** `drawing.rs` VML 分支的
match 補 `b"drawing"`/`b"inline"`/`b"blip"` 窄口：**只** `note_rid`（仿 Fallback
破例臂 `lib.rs`/`drawing.rs:435` 的「rId 一律記、幾何/段落一概不動」紀律），
內層 extent/geometry 照舊不看。本檔 ⇒ #2 候選 rids=[rId9] → PNG 真像素發
`DrawItem::Image` 取代同位佔位框；EMF×2 照退佔位（誠實近似）。

**P4（掛賬，非本檔阻塞）**：LO 把背板框內文字按正文流全頁排出（30 行）＋EMF 光柵化——
我方按 P1+P2 後文字落框內（框寬 567−6 斷行，覆蓋整頁），內容集合對齊度足以翻正分；
「負 z-index 背板文字按正文流」的 LO 同款升格規則收益/風險不對稱，留 K5 §4 懸項。

## 5. 紅燈定義（修復卡驗收門檻，先紅後綠）

以 `corpus/ooxmlexport/fdo76591.docx` 為金樣，`cargo test`＋render_lab 雙層：

1. **解析斷言**（`drawing.rs` tests）：`read_images` 回 3 候選，逐框
   `floating==true`、`pos_x_rel/pos_y_rel=="margin"`、pos＝(220.05,554.45)/
   (157.8,693.2)/(−13.95,−17.8)、rid＝rId6/rId7/rId9（P3 後 #2 rids 含 rId9）。
2. **版面斷言**：`paginate` 後 `page_count()==1`；`Page.images` 空、
   `floating_images` 3 項（頁絕對座標 (256.05,590.45)/(193.8,729.2)/(22.05,18.2)±1pt）。
3. **發射斷言**：`page_display_list(1)==None`；p0 含 `DrawItem::Image`
   （image3.png base64）；`BELVIDERE` 只發一遍（20pt `B`＋14pt `ELVIDERE` 樣式，
   無雙發素體副本）。
4. **儀表板門檻**：`render_lab.compare`（LO 走 Windows soffice）該檔 content 分
   **≥ 0.43**（回退前 lo-baseline 0.428 之上加容差；P1+P2+P3 落地後預期顯著高於此），
   並把 fdo76591 加入 seed 看門名單防再漂。
5. **零擾動臂**：無 `position:absolute` 的 VML 圖（K4a/K4c 既有測試語料）
   `floating` 恆 false、逐位不變——P1 的門禁斷言。

---

### 附：本卡探針清單（可重跑；`core/ooxml/examples/n1_probe.rs` 已 rm）

- `z1/z4/z5/z6/z7.py`：zipfile 直掃（rels/txbx 歸屬/body 段數/blip 位置/段落樹深）。
- `lo_truth.py`＋`lo_detail.py`：LO 真值＋頁對象幾何（§1.1 截段來源）。
- `kinds.py`＋`dump_*.json`：各 SHA dump 的 DrawItem 計數。
- bisect worktree（已清理）：`git worktree add --detach /tmp/bisect-{k4e2pre,k4e2,k4cpre} <sha>`。
