# F2 解剖報告：WordArt／DML preset 效果——目標檔實拆、語料普查與切片刻劃

工作樹：`wt/t_ab254070`（基線 main `f6f83bf`，本分支為 main 之祖先，落後 7 commit）
實作基線：`target/debug/quire-cli`（本分支編譯產物）
報告日期：2026-11-06

本報告每一條結論都由本卡實際跑過的指令支撐（指令與輸出附於各節），無一筆來自推測。

---

## 0. 結論先行（三句話）

1. **卡面前提不成立**：`corpus/ooxmlexport/artistic_effects.docx` 的 `word/document.xml` 內
   `prstTxWarp`／`wordArt`／`prstGeom`（非 rect）／`bevelT`／`softEdge`／`glow`／`effectLst` **出現次數全為 0**——本檔根本沒有 WordArt、也沒有任何形狀效果節點。卡面自附的普查指令在本檔回 0（實測見 §2）。
2. **「我方整幅空白」不成立**：本檔我方出貨 SVG 實測 **19,819 個非白像素**（150dpi），LO 側只有 **899 個彩色像素**；頁面上兩張內嵌 PNG 我方已正確發射。**圖軸 0.0 的真正成因是 EMF 向量圖（OLE 圖示）未光柵化**，與 WordArt／preset 無關。
3. **根因落在禁改面**：要讓本檔圖軸脫 0，必須（a）實作 EMF 向量記錄直出（`core/ooxml/src/emf_dib.rs` 的 K27 閘門，**本卡白名單未列**）或（b）放寬 J3 圖片軸門檻（度量面，**本卡禁區，且違反「不靠放寬度量湊燈」鐵條**）。二者皆非「在既有函式內追加分支」的切片規模。據卡面鐵律「根因若落在禁改面，停在卡面 comment 回報，不硬改」，本卡以解剖＋記帳收工，實作待 PM 重切。

---

## 1. 目標檔解剖（實拆 `word/document.xml`，7,204 bytes）

來源：`<hermes>/profiles/ow-render/cache/scratch/f2_drawing_dump.py`（zipfile 解包後逐區塊 dump）。

| 節點 | 實測計數 | 卡面假設 |
|---|---|---|
| `<w:drawing>` | 2 | — |
| `<wp:inline>` | 2（皆 inline，無 anchor） | — |
| `<pic:pic>`（DML 圖片） | 2 | — |
| `<a:prstGeom prst="rect">` | 2（**僅 rect**，於 `pic:spPr`） | 假設有多形狀 |
| `<a:prstTxWarp>` | **0** | 假設本檔有 WordArt |
| `<a:wordArt>` | **0** | 同上 |
| `<a:bevelT>`／`<a:softEdge>`／`<a:glow>`／`<a:reflection>`／`<a:effectLst>` | **0**（全部） | 假設本檔有形狀效果 |
| `<a:gradFill>` | **0** | 卡面必做 3 假設本檔有漸層 |
| `<a:solidFill>` | **0**（本檔 DML 填色只出自 VML/佔位路徑） | 卡面必做 3 |
| `<w:object>`（OLE） | 1 | 卡面未提及 |
| `<o:OLEObject DrawAspect="Icon">` | 1（`r:id="rId10"` → `word/embeddings/Microsoft_Word_Document.docx`） | 卡面未提及 |
| `<v:imagedata r:id="rId9">` | 1 → `word/media/image2.emf`（10,752 bytes） | 卡面未提及 |
| `<a:t>`（DML 文字） | **0** | — |
| `<w:t>`（正文文字） | **0**（本檔無任何正文文字） | 卡面必做 4 要求「文字必須發射」 |
| `<a14:imgEffect>` | 2（`artisticTexturizer`、`artisticPencilGrayscale`） | 卡面未提及 |

媒體Inventory：`media/image1.png`(823B)、`media/image2.emf`(10,752B)、`media/image3.png`(1,560B)、`media/hdphoto1.wdp`(1,432B)、`media/hdphoto2.wdp`(376B)、`embeddings/Microsoft_Word_Document.docx`(11,851B)。

即：本檔是「**兩張帶藝術效果篩選（a14:imgEffect）的內嵌 PNG ＋ 一個 Word 文件 OLE 圖示（EMF 影像）**」，檔名 `artistic_effects` 指的是 **Word 的「藝術效果」圖片篩選**（`a14:imgEffect`，屬圖片濾鏡，ECMA 擴充 `a14` 命名空間），**不是** DrawingML preset 形狀／WordArt 效果。卡面把檔名讀成了能力面缺口。

---

## 2. LO 把它畫成什麼（真值，pymupdf 抽 LO 轉換 PDF）

來源：`/mnt/c/Users/me/AppData/Local/Temp/quire-render-lab/ooxmlexport__artistic_effects.pdf`
（由 `scripts/render_lab.py corpus/ooxmlexport/artistic_effects.docx` 現跑產生，2026-11-06 22:31）。

LO p0（595.3×841.9pt，**零可抽文字**，`get_text()==''`——與卡面 `measurement_artifact:LO有圖形無可抽文字` 一致）：

| LO 繪製物 | xref／尺寸 | 位置（pt） | 我方對應發射 |
|---|---|---|---|
| `media/image1.png` | xref 13，32×32 px | (284.85, 70.80)–(310.45, 96.40) | `<image>` (284.8, 78.1) 25.6×25.6 ✅ **x 逐位對齊，y 偏低 7.30pt** |
| `media/image2.emf`（OLE 圖示點陣） | xref 8，316×205 px | (259.90, 105.55)–(335.50, 154.75) | 灰底 `<rect fill="#f0f0f0">` (259.9, 111.9) 75.6×49.2 ＋ 4 條 `#cccccc` 0.75pt 佔位框線 —— **幾何尺寸完全吻合（75.6×49.2 ＝ `w:dxaOrig/dyaOrig` 1508×984 twips），但像素全無**，y 偏低 6.38pt |
| `media/image3.png` | xref 11，32×32 px | (285.60, 163.75)–(309.60, 187.75) | `<image>` (285.65, 169.93) 24.0×24.0 ✅ **x 對齊，y 偏低 6.18pt** |
| `get_drawings()` | 0 筆 | — | — |

我方 p0 實測（`quire-cli render` 出貨 SVG，同一 150dpi 光柵）：非白像素 **19,819**、sat≥25 彩色像素 **287**（全部來自 image1.png）。

**J3 三軸現況（本分支實跑）**：內容 `None`、版面 `None`、色 `1.0`、圖 `0.0`、真缺字 `0`。

---

## 3. 圖軸 0.0 的精確成因鏈（用 render_lab 自己的度量函式逐項算，不靠推測）

來源：`f2_img_axis_cause.py`（直接 `import render_lab` 呼叫 `RL._sat_hue`／`RL._label`／`RL.image_blobs`／`RL.score_image`／`RL.score_color`）。

J3 圖片軸門檻（`scripts/render_lab.py`）：`SAT_MIN=25`、`IMG_MIN_SIDE_PX=20`（≈3.4pt）、`IMG_MIN_PIXELS=400`。

| 側 | sat≥25 像素總數 | 連通域（未過濾） | 過三道門檻後 |
|---|---|---|---|
| LO | 899 | 525、258、66、26、24 px | **1 塊**（Word 圖示藍色部分，525px，11×12pt） |
| 我方 | 287 | 287 px（image1.png 綠色部分） | **0 塊**（287 < `IMG_MIN_PIXELS=400`） |

→ `score_image(LO, OW) = 0.0`（我方零候選框，缺圖計 0）。

因此 0.0 由**兩個獨立因素疊加**，且兩者都在 WordArt 之外：

1. **EMF 向量圖未光柵化**：LO 把 `media/image2.emf` 內 Word 圖示的**向量繪圖記錄**光柵化成 316×205 點陣（含 525px 的藍色塊）；我方 K27 閘門只抽 `EMR_STRETCHDIBITS`(type 81) 內嵌 DIB，本檔 EMF 記錄流為 `1,24,25,114,24,18,82,37,37,40,82,37,84`——**無 type 81**，唯一大記錄是 type **114 = `EMR_COMMENT`**（9,376B，`cbName=4/offData=0/cbBits=0`，實測 hex 見 `f2_emf_hex.py`）。閘門拒絕是**正確行為**：`core/ooxml/src/emf_dib.rs` 註解已明文「型別 114 是向量繪圖記錄 ⇒ 只貼 DIB 就是虛構，必須退佔位框」。要出這些像素，需要的是 **EMF 向量記錄直出器**（路徑／筆畫／畫筆狀態機），不是 preset 效果分支。
2. **PNG 側的彩色像素不足 400**：我方唯一 sat≥25 連通域（image1.png 綠字部分）287px，落在 `IMG_MIN_PIXELS=400` 之下被過濾。LO 同圖 258px 也被過濾——即 LO 那側靠的也是別塊。老實講：**即使把 §4 的 y 偏移修好，本檔圖軸仍為 0.0**（我方唯一候選塊仍過不了 400 門檻）。這是一條硬結論：**本卡的任何 WordArt／prstGeom 工作都不可能把 `artistic_effects` 的圖軸從 0 抬起來**。

色軸 `1.0` 則是度量地板效應（非「色對」）：LO 彩色覆蓋率 0.000413、我方 0.000132，兩側皆 `< COLOR_FLOOR=0.0005` → `score_color` 依規格回 1.0（「兩邊皆無彩色＝1.0」）。記帳，不改度量。

---

## 4. 順帶抓到的真實缺口（本檔，附 LO 真值，非本卡範圍但值得一張卡）

**內嵌圖片（`wp:inline`）垂直定位系統性偏低 6–7pt**：三項繪製物的 y 全部偏低，且偏移量高度一致——

| 繪製物 | LO y（頂） | 我方 y（頂） | Δy |
|---|---|---|---|
| image1.png | 70.80 | 78.10 | **+7.30pt** |
| OLE 圖示（EMF） | 105.55 | 111.93 | **+6.38pt** |
| image3.png | 163.75 | 169.93 | **+6.18pt** |

x 軸逐位對齊、寬高逐位對齊，唯 y 單向偏低，且與 E2b/E2c 直排無關（本檔無直排）。根因方向應在 `wp:inline` 的 baseline／行框互動（圖片基線對齐 vs 頂部對齊），屬 `core/engine-api` 發射層＋`core/ooxml` 的 extent/anchor 換算面。本卡未實作，僅記帳並附真值，供 PM 決定是否單開一張「inline 圖片 y 錨定」卡。

---

## 5. 語料普查（影響面依據，不準猜）

### 5.1 卡面自附指令實跑結果

卡面指令 `grep -rl 'prstTxWarp\|wordArt\|bevelT\|softEdge\|glow' corpus/ | wc -l` → **回傳 0**。

原因：語料是 zip 容器，XML 在 binary 檔內，`grep -r` 對 `.docx` 二進位檔不產生文字命中（grep 視為 binary file 不列名）。**照抄該指令會得到「語料零影響」的錯判**。本卡改用 zipfile 逐 part 解壓正則統計（`f2_census_coherent.py`／`f2_warp_names.py`／`f2_geom_files.py`），並把口徑寫明：

掃描範圍＝400 個 docx/docm/dotx、1,291 個「文件本體 part」（`word/document.xml`、`header*`、`footer*`、`footnotes`、`endnotes`、`comments`）。**刻意排除 `theme1.xml`／`charts/*`／`diagrams/*`**：theme 的 `effectStyleLst`/`fillStyleLst` 每個 docx 都自帶 3–4 筆 `outerShdw`/`bevelT`/`gradFill`，計入會把「有無效果」的真實影響面灌水到全語料（舊 scratch `f2_corpus_scan.json` 即此口徑，其「每檔 bevelT=1、outerShdw=3」全是 theme 樣板，不可作為影響面依據——本節即為該錯判的矯正）。

### 5.2 效果節點分佈（本體 part，兩種計數口徑並列）

| 節點 | 僅 `a:` 前綴 | 任意前綴（含 `w10:`／無前綴） |
|---|---|---|
| `prstGeom` | 590 | 590 |
| `ln` | 493 | 493 |
| `solidFill` | 836 | 850 |
| `noFill` | 550 | 566 |
| `effectLst` | 98 | 98 |
| `prstTxWarp` | 100 | 100 |
| `gradFill` | 25 | 35 |
| `blipFill` | 7 | 167（差異出自 `pic:blipFill` 屬 `pic:` 前綴） |
| `custGeom` | 18 | 18 |
| `outerShdw` | 41 | 41 |
| `bevelT` | 10 | 18 |
| `glow` | 6 | 10 |
| `reflection` | 4 | 10 |
| `sp3d`／`scene3d` | 10／10 | 10／10 |
| `softEdge`／`bevelB`／`blur`／`innerShdw`／`fillOverlay`／`effectDag` | **0**（全部） | **0**（全部） |
| `wordArt` | **0** | **0** |
| `a14:imgEffect` | 0 | 2 |
| `OLEObject` | 0 | 28 |

### 5.3 WordArt warp 名稱分佈（本體 part，正則口徑 `prst=`，非 `val=`）

`a:prstTxWarp/@prst` 全語料只有兩種值：

| warp 名稱 | 次數 | 檔案數 | 是否真的變形 |
|---|---|---|---|
| `textNoShape` | **99** | 52 | **否**——`textNoShape`＝無變形的預設 warp，文字照直線基線排 |
| `textTriangle` | 1 | 1（`ooxmlexport/fdo80897.docx`） | 是（三角路徑排字） |

鐵證級結論：**全語料的 WordArt warp 有 99% 是 `textNoShape`（不產生任何視覺變形）**。卡面必做 2「至少做到文字照 warp 的 baseline 方向排」的真實需求面＝**1 個檔案**，而那 1 個檔案（fdo80897）我方已經發射出可見內容（實測：我方 p0 非白像素 7,290、彩色像素 2,553；LO 真值 p0 `drawings=16`，是紅色筆畫框線，見 `f2_lo_truth.out`）。

註：`a:wordArt`（ECMA-376 第三版的 `wps:wordArt` 包裝節點）在**全語料本體出現 0 次**——所有 warp 都直接掛在 `wps:bodyPr` 內。卡面以 `wordArt` 為消費目標，打的是語料中不存在的節點。

### 5.4 `prstGeom` 形狀分佈（本體 part）——這裡才是真實缺口

| `@prst` | 次數 | 檔案數 |
|---|---|---|
| rect | 395 | 155 |
| roundRect | 67 | 5 |
| straightConnector1 | 54 | 5 |
| line | 39 | 8 |
| ellipse | 8 | 8 |
| star5 | 8 | 5 |
| leftRightArrow | 6 | 1 |
| triangle | 4 | 4 |
| rtTriangle | 4 | 4 |
| upArrow | 3 | 1 |
| rightArrow | 1 | 1 |
| star10 | 1 | 1 |

非 rect 共 **195 次／約 25 檔**。代表檔：`ooxmlexport/fdo74605.docx`（roundRect＋leftRightArrow＋upArrow＋straightConnector1）、`ooxmlimport/groupshape-fontname.docx`（line）、`ooxmlexport/tdf113258.docx`（ellipse）、`ooxmlexport/tdf113183.docx`（star5＋triangle）、`ooxmlimport/tdf120551.docx`（roundRect＋line）、`poi/test-data_document_shapes-with-text.docx`（rightArrow）。

**實測我方現況**（`quire-cli dump`，`f2_shape_gap.py`）：上述檔案的 `DrawItem` 只有 `Rect`／`Rule`／`Text`／`Image`——即 **`prstGeom` 的 `@prst` 屬性從未被讀取**（全庫 `grep 'b"prst"'` 於 `core/ooxml/src/*.rs` 回 0 命中；`prstGeom` 只在 `drawing.rs` 的**測試字串**出現 3 次，實作路徑零消費），所有非矩形形狀一律退化成外接矩形 `Rect`。這是真實、可見、可量測的缺陷（圓被畫成方塊、箭頭被畫成方塊），比 WordArt 更接近卡面的原始意圖。

### 5.5 `gradFill`（卡面必做 3）

本體 part `a:gradFill` 25 次／8 檔（任意前綴 35 次）。8 檔清單：`ooxmlexport/NoFillAttrInImagedata.docx`、`ooxmlexport/TextEffects_Groupshapes.docx`、`ooxmlexport/fdo73247.docx`、`ooxmlexport/fdo78658.docx`、`ooxmlimport/tdf120551.docx`、`poi/test-data_document_bib-chernigovka.netdo.ru_download_docs_17459.docx`、`xopt/NewDocxDocuments_CoverPage.docx`、`xopt/TestFiles_HC031-Complicated-Document.docx`。實作現況：`gradFill` 於 `core/ooxml/src/*.rs` 實作路徑零消費（僅 `theme.rs` 註解宣告「亮度變換本卡不做」）。`solidFill` 已完整消費（K12/K24 鏈），故卡面必做 3 的「solidFill 套到形狀填充與輪廓」**已存在**，非缺口。

### 5.6 OLE 圖示（`DrawAspect="Icon"`）

本體 6 檔：`ooxmlexport/artistic_effects.docx`、`ooxmlimport/tdf108545_embeddedDocxIcon.docx`、`phpword/tests_..._reader-styles.docx`、`phpword/tests_..._reader.docx`、`poi/test-data_document_WordWithAttachments.docx`、`poi/test-data_document_tika-3388.docx`。

`tdf108545_embeddedDocxIcon.docx` 實測（`f2_ole_truth.py` ＋ render_lab）：LO 真值 p0 有 1 張點陣圖 xref 8（318×209）於 (72.0, 94.45)–(148.5, 144.75)、文字 `Test.`；我方 SVG **零 `<image>`**，彩色像素 **0**，故該檔**圖軸同為 0.0**（版面/內容/色皆 1.0）。這才是「LO 有圖形、我方空白」的**真實形態**——根因同 §3：OLE 圖示的 EMF 影像未被光柵化，**依然與 WordArt／preset 無關**。

---

## 6. 劃界：本切片做什麼、不做什麼、已實測不支援

### 6.1 本卡（照字面）無法執行之處——逐條對卡面必做

| 卡面必做 | 卡面假設 | 實測 | 判定 |
|---|---|---|---|
| 1 `prstGeom` 常見形狀→SVG 輪廓 | 目標檔有 rect/ellipse/triangle 等 | 目標檔 `prstGeom` **只有 2 個 rect**，且它們在 `pic:spPr`（圖片裁切框，不該獨立發射） | **目標檔無法驗證本項**。缺口真實存在於**別檔**（§5.4，195 次／25 檔） |
| 2 `prstTxWarp` → 文字照 baseline 排 | 目標檔有 warp | 目標檔 `prstTxWarp` **0 次**；全語料 warp 99% 為 `textNoShape`（無變形），唯一真實變形檔是 fdo80897 且我方已有輸出 | **前提不存在**，本項在目標檔上無效 |
| 3 `solidFill`／`gradFill` 套填充與輪廓 | 目標檔有漸層 | 目標檔 `gradFill`/`solidFill` **0 次**；`solidFill` 消費鏈已存在；`gradFill` 零消費（真缺口，25 次／8 檔，**不在本卡目標檔**） | 半項為既有能力、半項為別檔缺口 |
| 4 文字必須發射 | 目標檔有 DML 文字 | 目標檔 `<a:t>` **0**、`<w:t>` **0**（**本檔無任何文字**）；LO 側同樣 `get_text()==''` | 「我方空白」不成立：雙側皆無文字，內容軸故判 `None`（度量語意 4/4b），非事故 |
| 硬性合格線「不準發射空白（LO 有東西、我方也必須有東西）」 | 我方整幅空白 | 我方出貨 SVG 非白像素 **19,819**（LO 899 彩色像素）——**合格線在現況已成立** | 卡面唯一硬線已被滿足 |

### 6.2 明確不做（寫進報告的「已實測不支援＋記帳」）

- **EMF 向量記錄直出**（`EMR_COMMENT`/type 114、polybezier、pens/brushes 狀態機）：本檔與 tdf108545 圖軸 0.0 的共同根因。規模＝新子系統，且 `core/ooxml/src/emf_dib.rs` **未列入本卡白名單**；K27 閘門現行行為（退佔位框）經實測是誠實的，不得放寬。
- **真路徑變形**（warp 逐字沿路徑變形）、`a:reflection`、`a:glow` 模糊濾鏡、`a:bevelT` 立體光照、3D scene、`a:softEdge` 濾鏡：卡面已列為不做，本卡亦未實作，語料影響面分別為 glow 10、reflection 10、bevelT 18、sp3d/scene3d 各 10、softEdge/blur **0**。
- **`a14:imgEffect` 圖片藝術篩選**（`artisticTexturizer`／`artisticPencilGrayscale`，本檔檔名由來）：語料僅 2 次／1 檔，屬 `a14` 擴充圖片濾鏡，非 DML preset，本卡未實作。
- **J3 度量放寬**（`IMG_MIN_PIXELS`/`SAT_MIN`/`COLOR_FLOOR`）：禁區，且違反鐵條，不碰。

### 6.3 建議的重切（給 PM 的三張卡，本卡不硬改）

1. **F2′：`a:prstGeom/@prst` 未消費——非矩形形狀全畫成外接矩形**。目標檔 `fdo74605.docx`（roundRect＋2 箭頭＋connector，1 檔集齊 4 種形狀，是最便宜的考卷）。影響面 195 次／25 檔（§5.4 清單）。白名單同本卡。這是本卡原始意圖裡**唯一真實且可驗證**的缺口。
2. **OLE 圖示 EMF 光柵化**（圖軸真缺口）：目標檔 `artistic_effects.docx` ＋ `tdf108545_embeddedDocxIcon.docx`，兩檔圖軸皆 0.0 且皆因此因。需要 EMF 向量直出，場域在 `emf_dib.rs`/EMF 採集面——**必須與 R35/F3 的圖片採集狀態機錯開**，並先決定「實作 EMF 直出」還是「改採集 LO/Word 預渲染點陣」路線。
3. **`wp:inline` 圖片 y 錨定偏低 6–7pt**（§4，三項繪製物一致偏移，LO 真值已附）。

### 6.4 排程風險（沿 PM 11-05 15:36 判斷，實測再確認）

`core/engine-api/src/lib.rs` 正被 F1（main 上 11-06 14:05–19:13 五個 commit：`7e23d53`→`fc3230b`）與 E2c WIP（`27c4194`）活躍修改；`core/ooxml/src/drawing.rs` 亦在 E2c/F3 改動清單內。本卡白名單三面全與重疊。任何上述重切卡都應排在 **F1 → E2c → D9b 之後**，否則三方解衝突風險高。本分支目前落後 main 7 commit。

---

## 7. 證據與再跑指令

- 目標檔解剖：`python3 f2_drawing_dump.py`（解包 `word/document.xml` 逐 `<w:drawing>` dump）
- LO 真值：`/usr/bin/python3 scripts/render_lab.py corpus/ooxmlexport/artistic_effects.docx --out <dir>` → `report.md`（色 1.0／圖 0.0／內容 None／版面 None／真缺字 0）＋ `f2_lo_truth_ae.py`（pymupdf：3 張點陣圖 rect、`get_drawings()==0`、`get_text()==''`）
- 我方發射：`./target/debug/quire-cli render corpus/ooxmlexport/artistic_effects.docx --out <dir>` ＋ `quire-cli dump`；像素計數 `f2_our_px.py`
- J3 逐項復算：`f2_img_axis_cause.py`（import `render_lab` 直呼 `_sat_hue`/`_label`/`image_blobs`/`score_image`/`score_color`）
- 語料普查：`f2_census_coherent.py`（雙口徑）、`f2_warp_names.py`（warp/geom 名稱分佈）、`f2_geom_files.py`（非 rect 形狀檔案清單）、`f2_shape_gap.py`（非 rect 檔我方發射實況）
- EMF 記錄流：`f2_emf_hex.py`（記錄型別清單 ＋ type 114 欄位實拆）
- 所有探針在 `<hermes>/profiles/ow-render/cache/scratch/`（scratch 24h 回收，關鍵數據已固化進本報告）

## 8. 鐵律遵事

只在 worktree `wt/t_ab254070`；零 `git push`／checkout／reset／rebase／stash；本報告為唯一新增檔案（`docs/`，白名單內）；引擎碼**零變更**（故 `roundtrip corpus --check`、fdo76591、WC048、btlr-textbox、99_Fields、cell-grid-span、CoverPage、n751077 與「無 WordArt 檔案 SVG 零位元組差異」四組閘門天然零位移——本卡未動任何發射路徑）。
