# G1：錨點圖 `positionV/H relativeFrom` 未消費——解剖報告

卡：t_76bbb908｜分支：`wt/t_76bbb908`｜署名：ADT109119
度量口徑：LO（Windows `soffice.exe` headless → PDF）用 pymupdf 抓「真繪製」
`get_image_rects()` 與 `get_drawings()` 框線，單位 pt。本報告只出現座標／計數，
不含任何用戶檔內文、內嵌圖片或 docProps 值。

## 1. 落點在哪一层（第一步實抓結論）

### 1.1 採集層（`core/ooxml/src/drawing.rs`）——無缺陷
`pos_x_rel/pos_y_rel/pos_x_pt/pos_y_pt/pos_x_align/layout_in_cell/rel_height`
全部如實採集（`G1_DEBUG=1 quire-cli info` 實抓用戶檔兩枚錨點圖）：

```
para=84 floating=true in_cell=false lic=true  xrel=column   yrel=paragraph
        xoff=53.35 yoff=11.81937 xalign=None  → anchor{400.1, 113.79, Page, Page}
para=87 floating=true in_cell=false lic=true  xrel=column   yrel=paragraph
        xoff=6.3   yoff=45.1     xalign=None  → anchor{353.05, 419.575, Page, Page}
```

座標值本身正確（extent 換算正確），**缺陷在下一层的「原點系」與「附掛頁」**。

### 1.2 真正落點＝兩处，缺一不可

| # | 落點 | 代碼位置 | 症狀 |
|---|---|---|---|
| A | **宿主列的頁內 y 是「靜態算的」** | `core/engine-api/src/table_map.rs:1053-1055`（`find_cell_for_para`）：`cell_y = margin_top + tblp_y + Σ(列高)` | 把「表格永遠從版心頂開始排」寫死。表格被前面內容推到頁中／跨頁時，算出的格頂與該列**實際**頁內位置無關 → 用戶檔 p1 圖片滯留 y=72.1（版心頂）被續列覆寫 |
| B | **附掛頁＝宿主段「流項起頭頁」** | `core/layout/src/lib.rs:1737-1747`（`attach_floating_images`）：`starts.partition_point(\|s\| *s <= (fi,0,0))` | 段落落在表格內時 `fi`＝該**表格項**，分頁點比的是表格起頭 → 宿主列被推到次頁時，圖片仍掛在表格起始頁。燈 1 要打的就是這個 |
| C | **V 軸原點漏了框線與上內距** | `resolved_anchor` K10/K15 分支只取「格頂」，未加 `top 框線寬＋pad_top_pt` | LO 真值差 3.7–4.75pt（見 §3 律） |
| D | **H 軸原點取格框左，未取欄內容盒** | 同上：`cell_x = margin_l + geom.0` | LO 真值＝格框路徑＋左內距，差 `half_border`（實測 1.0pt） |

`core/engine-api/src/lib.rs` 對 `pos_y_pt/pos_x_rel` 確無第二处消費點（PM 假設成立），
但它們已在 `table_map::resolved_anchor` 被換算進 `FloatingAnchor`——所以「零消費點」
的描述要修正為：**換算了偏移，沒換算原點（A/C/D）與附掛頁（B）**。

### 1.3 另有座標覆寫的層（PM 要的第 3 點）
- `build_images`（table_map.rs:1185）之後无人覆寫浮動圖座標；
  `attach_floating_images` 是**唯一**把 `FloatingAnchor` 折成頁絕對座標的地方
  （三個呼叫點：lib.rs:1669／2390／2457）。
- 發射層 `page_display_list` 只做「同原點配對」（K4c 像素池／K4e2 框文字），
  不改寫 x/y；**唯一**的例外是頁腳 CHRYSLER logo 的 +7.25/+3.60 特例
  （lib.rs:2220 附近，尺寸＋位置双重命中才生效）與 F1 圖表的框內內縮，
  兩者都不吃本卡路徑。→ 落點就是 A/B/C/D，不在 CellBlock／浮圖排序层。

## 2. 語料普查（`grep`/zipfile 實跑，非粗估）

- `relativeFrom="paragraph"`：語料 **83 檔**有 anchor；其中 V 軸＝paragraph 的
  anchor 共 **219 枚**（`lic=1` 213 枚／`lic=0 且宿主在格內` 4 枚／非格內 2 枚）。
  腳本：`g1_census_dyn.py`（本卡 scratch）。
- **本卡新路徑（宿主在格內＋`layoutInCell=0`＋V=paragraph）在既有語料只觸及
  1 檔 4 枚**（`ooxmlimport/line-rotation.docx`）。`lic=1` 的 213 枚走 K15/K17
  既有路徑——**本卡刻意不動它**（K15/K17 鎖與 WC048 零回退是硬約束）。
- 這決定實作紀律：新增「動態列錨點」只在 `!layout_in_cell && 宿主在格內`
  時啟用，`lic=1` 逐位元組走舊路徑。

## 3. LO 真值律（合成檔實抓，不靠猜）

探針檔＝`core/engine-api/tests/fixtures/g1_anchor_paging.docx`（生成器
`gen_g1_fixture.py`，全合成佔位文字＋4×4 純色 PNG，**零私密內容**）。
結構＝用戶檔縮影：A4／邊距 90,72／跨頁長表（續列場景）／錨定表 2 列×2 格
（首列 trHeight 9000 atLeast 把**錨定列推到次頁**，列 1 trHeight 2955）、
錨定列兩枚 `wp:anchor`：`column` 系一枚 `align=center`＋一枚 `posOffset=0`，
V 軸 `paragraph` posOffset=635 EMU（0.05pt）、extent 2567305×1443990 EMU、
`wrapSquare/largest`、`layoutInCell=0`、`behindDoc=0`、relH 2／3、
錨定表帶 2pt 實框（sz=16）＋cellMar 55（2.75pt）——框線與內距專為解座標律而設。

LO 實抓（p4，錨定列所在頁）：

```
真繪製 image rect : A (96.50,526.75)-(298.65,640.45)   B (304.10,526.75)-(506.25,640.45)
框線路徑          : 表左 V 線 x=93.8｜中 V 線 x=301.4｜表右 x=509.1
                   列界 H 線 y=73.0（表頂）／523.0（錨定列頂）／670.8（表底）
```

反解（框線路徑＝標稱邊＋半框線寬；標稱版心左＝90＋tblInd 2.75＝92.75，
標稱列頂＝72＋450＝522.0，框線寬 2.0pt）：

| 軸 | LO 實測 | 反解式 | 誤差 |
|---|---|---|---|
| A x | 96.50 | 標稱格左 92.75 ＋ ½左框 1.0 ＋ 左內距 2.75 ＋ 0 ＝ **96.50** | 0.00 |
| B x | 304.10 | 標稱格左 300.40 ＋ ½左框 1.0 ＋ 左內距 2.75 ＋ 0 ＝ **304.15** | 0.05 |
| A/B y | 526.75 | 標稱列頂 522.0 ＋ 上框寬 2.0 ＋ 上內距 2.75 ＋ posOffset 0.05 ＝ **526.80** | 0.05 |
| 寬 | 202.15 | extent 202.15（=2567305/12700） | 0.00 |
| A 置中 | 96.50 | 欄**內容盒**寬＝207.65−2×2.75＝202.15；(202.15−202.15)/2＝0 → 貼內容盒左緣 | 0.00 |

**律（本卡實作依據，皆為實測反解，非口算）**：

```
宿主列頂(頁內) ＝ 分頁完成後該列在該頁的實際頂（frag.y_pt ＋ 前置列高）
圖頂 y        ＝ 宿主列頂 ＋ top 框線寬 ＋ 格上內距 ＋ posOffset(V)
圖左 x        ＝ 表左(頁內) ＋ 表格本地格左 ＋ ½left 框線寬 ＋ 格左內距 ＋ posOffset(H)
align=center  ＝ 在「格寬 − 左內距 − 右內距」的內容盒內水平置中
附掛頁        ＝ 宿主列「實際落點頁」（列跨頁時＝列頂所在頁）
z 序          ＝ behindDoc=0 → 浮於文字層之上；層內依 relativeHeight 遞增
```

V 軸加**整條**框寬、H 軸加**半條**，是 LO 實測的非對稱（K15 在 WC048 的
0.6pt 框線下也自洽：該案原點踩「列界路徑 − ½框寬」，屬 `lic=1` 路徑，
本卡不動）。用戶檔 p0 末列「3.2.7 收在 y=750.7（有底線）」顯示同一族框線，
律的適用性一致；燈 2 容差 ±4pt 吸收框寬差（E1 欠債 A 的列高差另計）。

## 4. 現況差距向量（拍紅實測）

同一支合成檔，我方（`main` 同碼）實抓：

| 項目 | LO 真值 | 我方現況 | 差距 |
|---|---|---|---|
| 錨定列所在頁 | p4 | 圖片掛在宿主段流項起頭頁（≠p4） | **燈 1 紅** |
| 圖頂 y | 526.75 | 525.65（=72＋列高量化 453.6＋0.05，缺框寬與上內距） | −1.10（含列高量化 +2.6 的抵銷） |
| A x（center） | 96.50 | 95.50（置中基準誤用格寬 207.65，得偏移 2.75） | −1.00 |
| B x（posOffset=0） | 304.10 | 300.40（缺 ½框寬＋左內距） | −3.70 |
| 兩圖間距 | 207.60 | 204.90（燈 3 要求 x 軸各自正確） | −2.70 |

用戶檔（卡面真值，非本卡實抓）：p1 兩張 `<image>` 均在 y=72.1＝版心頂，
與 p1 開頭表格續列（y=72..265）重疊——即 §1.2-A「表格從版心頂起算」的直接後果。

## 5. 明確不做（記帳）

- `wrapSquare` 文字繞排（文字穿圖可接受）→ G1b。
- `behindDoc=1` 圖壓文字下：本卡只發 `behindDoc=0` 的浮層鍵；`behindDoc=1`
  維持現況（語料普查：用戶檔與合成檔皆 0）。
- `twoCellAnchor`（ Drawing 錨定第三式，語料未見文書流案例）。
- `rot`／`ext` effectExtent 邊框膨脹。
- E1 欠債 A（列高×LO 差 → 分頁落點差 4 列）：另卡 t_8573d294，不在本卡。
- `lic=1`（K15/K17 路徑）不改動——它的框線/內距同款缺口另計（記帳於此，
  因 K15/K17 已有 ±1pt 鎖在 0.6pt 框線檔上，動它需重校 213 枚 anchor）。
