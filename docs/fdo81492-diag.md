# fdo81492 歸因報告：最差榜第 1（內容分 0.060，第 1 頁即分歧）

- 卡：oc-f81492-diag（分支 oc/f81492）；report-only，白名單僅 `scripts/`＋`docs/`。
- 本版生成：2026-11-11。重跑入口：`/usr/bin/python3 scripts/f81492_axis_probe.py`（冪等；產物全落 /tmp）。
- 零引擎碼改動：本卡僅新增 `scripts/f81492_axis_probe.py` 與本報告；`core/`、`shells/`、`tools/`、`corpus/` 零觸碰。

## 0. 一句話結論

**0.060 的主軸是分頁／結構軸：我方把「References」整節（第 2 頁全部內容）擠上了第 1 頁。**

LO 在第 1 頁只放「work (McColl and Young 2005). This 」＋頁碼「1」，自第 2 頁起才排
References 全文；我方渲成單頁、pagebreaks.json 只有 page 0、無 page 1。原因是
`word/styles.xml` 的 Heading1 樣式宣告 `<w:pageBreakBefore/>`，而我方級聯層
（`core/ooxml/src/styles.rs`）**完全不解析樣式級 pageBreakBefore**，直接 pPr 的直讀路徑
（`core/engine-api/src/lib.rs:471`）自然收不到，layout 的顯式分頁臂只剩 parser 直讀這一路。
文字軸零缺字；色彩軸大體一致（1 個 run 色差）；圖片軸兩邊皆零。

## 1. 我方實測（cargo render＋dump，非引用）

```
find corpus -iname 'fdo81492*'
→ corpus/ooxmlexport/fdo81492.docx

render: corpus/ooxmlexport/fdo81492.docx → 1 頁 SVG ＋ /tmp/f81492-ow/pagebreaks.json（6 段落）

pagebreaks.json: [{"page": 0, "first_para": 0, "last_para": 5, "line_count": 11,
                   "continues_paragraph": false, "width_pt": 595.3, "height_pt": 841.9,
                   "starts_section": false}]
```

dump 顯示列表型別計數（第 1 頁，595.3×841.9pt）：

| 型別 | 數量 |
|---|---|
| Text | 22 |
| Rect | 0 |
| Image | 0 |
| Rule | 0 |

- 總頁數：1；第 2 頁不存在。
- 頁尾頁碼 "1" 於 (x=294.3, y=806.5, 12pt)。
- "References" 以 18pt、粗體、`#1f497d`、y=108.8 排在第 1 頁頂部區（LO 排在第 2 頁）。
- 兩個空字串 Text 項（y=265.2 / 292.8），內容為空，無實質墨跡。

## 2. LO 真值（Linux soffice 25.8 → PDF，pymupdf 實讀）

```
HOME=/tmp/loprof-f81492-diag .../soffice --headless -env:UserInstallation=file:///tmp/loprof-f81492-diag \
  --convert-to pdf --outdir /tmp/lof81492 corpus/ooxmlexport/fdo81492.docx
→ /tmp/lof81492/fdo81492.pdf（42087 bytes，驗檔存在、非零）
```

pytest 逐頁實測：

| 頁 | 尺寸 pt | 文字行數 | drawings | 線段 item | 實繪圖（get_images＋get_image_rects） |
|---|---|---|---|---|---|
| 0 | 595.30×841.89 | 2（`1`、`work (McColl and Young 2005). This `） | 0 | 0 | 0 |
| 1 | 595.30×841.89 | 9（`2`、`References`…兩條目全文） | 4 | 4（全為藍線 `(0,0,1)`、寬 0.7） | 0 |

- LO 全文 token：p0 7 個、p1 37 個（含頁尾 `1`／`2`）。
- LO p1 的 4 條藍色線段（x 起點 108.1，y≈142.3／156.1／169.9／223.3）＝兩條外部
  超連結的**底線向量**（Dairy URL 跨 3 行、CSIRO URL 跨 1 行）。span 色值亦證實：
  `References`＝`0x1f497d`、URL＝`0xff`、其餘黑。

## 3. 逐軸對照第 1 頁

### 3.1 文字軸——零缺字（分歧不是缺字）

raw token 集合差集（multiset）後，套「前段＋後段拼接命中」剔除（空白剝除後落對方全文即算切分粒度差）：

```
LO-only 拼接過濾後: {}
OW-only 拼接過濾後: {}
```

raw 差集全部是切分假影，例如我方把 hyperlink 切成 `http://`＋後段（LO 黏成單 token）、
LO 把句點黏進 URL token（`...ashx.`、`...tr16-05.pdf.`）。**全文 100% 命中、零缺字、零多字。**
差別僅在分頁：LO 的第 1 頁＝我方第 1 頁開頭一行＋頁碼，LO 第 2 頁＝我方第 1 頁其餘全部。

### 3.2 色彩軸——大致吻合，1 個 run 色差

110 dpi 光柵化、非白像素直方（白＝≥250）：

```
OW p0 非白 px=24904；LO p0=1598；LO p1=27515；LO 聯集=29113
ow∩lo0 = 1559（vs min 0.976）
ow∩lo1 = 18673（vs min 0.750）
ow∩(lo0∪lo1) = 19206（vs ow 0.771）
```

主色調完全吻合：黑 `#000000`、URL 藍 `#0000ff`、Heading1 藏青 `#1f497d`
（我方 SVG `fill` 與 LO span 色 `0x1f497d` 逐位一致）。兩個差異：
1. 我方內部錨點 hyperlink「McColl and Young 2005」（第 1 頁唯一同一行內超連結）
   畫 `#0563c1`（我方 hyperlink_default 兜底色），LO 畫黑（`0x0`）。單一 12pt run。
2. 頁面未白底覆蓋區因分頁錯位而錯位（我方整頁內容黏在第 1 頁 vs LO 幾乎空白的第 1 頁），
   直方交集被分頁差異壓低，非色彩缺陷。

### 3.3 圖片軸——兩邊皆零實繪圖

LO get_images(full=True) ＝ 0（兩頁 get_image_rects 均 0）；我方 dump Image 項＝0。本軸無訊號。

### 3.4 結構軸——我方缺底線向量（次要）＋分頁整體錯位（主要）

- LO 第 2 頁有 4 條 `Line`（外部超連結底線）；我方 dump 零 Rule/Rect 項，
  但 SVG 以 `text-decoration="underline"` 呈現底線，pymupdf 光柵化可見藍色底線像素
  （ow 非白直方含 `(0,0,255)` 1531 px 及抗鋸齒邊），**視覺等效**，不構成實質缺陷。
- 真正的結構差異＝分頁：我方 1 頁 vs LO 2 頁；我方 pagebreaks 只有 page 0，
  且我方頁首直接從「work (…). This 」接上「References」，LO 在此強制換頁。

### 3.5 主軸判定

**0.060 的主軸＝分頁／結構軸，根因是 pageBreakBefore 未生效**（內容整段被推去第 2 頁而 LO 留在第 1 頁，反之亦反之）。
第 1 頁即分歧（PM 已量）與本節實測逐位一致：我方第 1 頁含 LO 分屬兩頁的全部內容，
逐頁光柵對比在第 1 頁就全線崩潰；文字軸、圖片軸、色彩軸均不構成獨立失分。

## 4. 根因錨點

鐵證鏈（zipfile 雙實證）：

`corpus/ooxmlexport/fdo81492.docx` → `word/styles.xml`

```xml
<w:style w:type="paragraph" w:styleId="Heading1">
  …<w:pPr><w:keepNext/><w:pageBreakBefore/>…</w:pPr>…
</w:style>
```

`word/document.xml` 的「References」段：`<w:pStyle w:val="Heading1"/>`＋段首
`<w:lastRenderedPageBreak/>`（渲染提示，依 ECMA-376 §17.4.22 無強制斷頁語義，我方不理是正確的，
且 `core/ooxml/src/lib.rs:246-250` 註明鐵律）；直接 pPr **沒有** pageBreakBefore。
強制換頁唯一來源＝Heading1 樣式宣告。

我方代碼落點（全部為「樣式級 pageBreakBefore 不進級聯」的三層漏斗）：

1. `core/ooxml/src/styles.rs:613-611` `parse_ppr`（註解明寫「本卡只收 spacing(line)/jc/rPr，
   其餘段落屬性略過」）——樣式 `w:pPr` 內的 `pageBreakBefore` 走 `Event::Empty` 分支，
   落入 `_ => {}` 靜默吞掉；`parse_ppr_default`（`styles.rs:738`）同病。
2. `core/ooxml/src/styles.rs:232-243` `ParaStyle` 無 page_break 欄位；
   `resolve_cached` 合成處（`styles.rs:1236-1243`）段落欄位 only-or 鏈（line/spacing/align）
   不含分頁旗標；`effective_para`（`styles.rs:1280`）亦無從疊加；docDefaults 的
   pPrDefault 同樣無路。
3. `core/engine-api/src/lib.rs:471`
   `let grid_break_v: Vec<bool> = doc.paragraphs.iter().map(|p| p.page_break).collect();`
   ——網格分頁旗標**只讀 parser 直讀的** `p.page_break`（`Paragraph::page_break`，
   來自直接 pPr `w:pPr/w:pageBreakBefore`，見 `core/ooxml/src/lib.rs:277-283, 465-466`），
   樣式級訊號在此歸零。
4. layout 消費端（`core/layout/src/lib.rs:1398-1399, 1446-1452` 與
   `ParaGrid::break_before_of`，`lib.rs:802-804`）本身**支援** `break_before`，
   只是餵進來的向量從未含樣式級 true。

結論：不是 layout 的錯、不是 direct-pPr 解析的錯；是**級聯層從未收樣式級 pageBreakBefore**，
屬已知未支援項目（styles.rs 模組頭「未支援清單」未列此項，屬靜默缺口，非刻意取捨）。

LO 則明顯消費了它（LO 把 References 推到第 2 頁；若 LO 也忽略，兩邊都會是單頁）。

## 5. 修復口徑建議（不實作，供 PM 決策）

1. `styles.rs`：`OwnStyle`／docDefaults 收 `page_break: Option<bool>`
   （三態：未宣告／true／false；`w:pageBreakBefore` 為 toggle，`w:val="false"/0/off`＝明確關）。
   `parse_ppr`／`parse_ppr_default` 解析之；`ParaStyle` 增 `page_break: Option<bool>`，
   合成沿用 `.or()` 鏈（own.or(parent)），inline 直接格式最高優先。
2. `engine-api/src/lib.rs:471`：`grid_break_v[i] = p.page_break || cascade_page_break(i)`
   （cascade 與既有 `effective_para` 呼叫點同一次計算，勿開第二趟）。
   `p.page_break` 為 bool、無顯式 false 三態——直接 `<w:pageBreakBefore w:val="false"/>`
   與未宣告同徑，可接受（低頻畸形），或在 ooxml 層把 `page_break` 升 Option<bool> 一併根治。
3. layout 零改動（消費端已備）。`w:lastRenderedPageBreak` 鐵律維持不動。
4. 考卷紅燈寫法（建議兩盞）：
   - 單元：styles 級聯——樣式帶 `<w:pPr><w:pageBreakBefore/></w:pPr>`＋段落僅 pStyle
     無直接 pPr → 斷言 effective page_break＝true；再加 `<w:pageBreakBefore w:val="false"/>`
     斷言 false；再加 Normal（不宣告）為父、子樣式未宣告 → 繼承父。
   - 整合紅燈：以本檔 `corpus/ooxmlexport/fdo81492.docx`（或同型最小 fixture：
     兩段落、第二段 pStyle 指向帶 pageBreakBefore 的樣式）render，斷言頁數＝2、
     pagebreaks.json 含 page 1、「References」為第 2 頁首項。現況＝1 頁 → 回紅，
     修復後轉綠。紅燈字樣建議：「樣式級 pageBreakBefore 未生效（t_f81492 紅證）」。
5. 回歸面提醒（PM 決策用）：樣式級 pageBreakBefore 屬常見 Word 樣式
   （Heading 1/2 預設常見），修復後受影響檔案**分頁會普遍收斂向 LO/Word**，
   原先靠單頁容身而誤過的金檔可能轉紅——建議先掃 corpus 統計受影響檔數再合入。

## 6. 殘留與限制（照實標）

- 色彩直方為 110 dpi 光柵化近似，且我方 SVG 由 pymupdf 內建 SVG 引擎光柵化
  （字型 fallback 與 LO 原生排版不等，像素值僅作交集近似，非品質判決）。
- 4 條 LO 底線向量 vs 我方 `text-decoration="underline"`：視覺等效已由藍色像素證實，
  未逐像素對齊線段座標（此非 0.060 來源）。
- 我方「McColl and Young 2005」一個內部錨點 run 畫 `#0563c1`、LO 畫黑：
  我方 hyperlink 兜底色（`HYPERLINK_DEFAULT_COLOR_ARGB`）與 LO 的 anchor 呈現不一致，
  屬獨立小缺陷，本卡不歸其因。
- 我方 dump 兩個空字串 Text 項（y=265.2／292.8）無墨跡，未影響評分判定。
- 頁尾頁碼「2」缺失是分頁錯位的衍生物（無第 2 頁則無 PAGE=2 可渲）。
- LO PDF 由 Linux soffice 25.8 產出（本卡指定路徑），未與 Windows soffice 交叉覆核；
  PM 若需 Windows 真值對帳，換 `--headless` 等價命令重跑 `scripts/f81492_axis_probe.py` 即可
  （探針內 soffice 路徑為常數，需 PM 改單行）。
- 未量化對 0.060 的逐像素貢獻拆解（分頁 ⇄ 0.060 的歸因是構造性的：
  首頁分歧頁＝第 1 頁、且內容 token 零缺、零多，故失分幾乎全落在整頁錯位的逐頁對比上）。

## 7. 重跑

```
/usr/bin/python3 scripts/f81492_axis_probe.py
```

依序：cargo render＋dump → Linux soffice 轉 PDF（/tmp/lof81492）→ pymupdf 逐頁
（尺寸／行數／線段／實繪圖）→ 文字／色彩／圖片／結構四軸印出。冪等，全落 /tmp，不寫 repo。
