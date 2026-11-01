# P5-diag：EMF 域色軸地板定量診斷

- 基線：`main` @ `8271675`
- 性質：**report-only**。本卡只新增本報告與 `scripts/p5_emf_probe.py`，未改
  `core/`、`shells/`、`scripts/render_lab.py`、`scripts/lo_scoreboard.py`、
  `corpus/`、`out/`，亦未 commit／push。
- 對象：`corpus/ooxmlexport/fdo76591.docx`、`corpus/ooxmlexport/n779834.docx`
- 口徑：LO 側一律 `soffice --convert-to pdf` 後以 PyMuPDF 量測；我方側一律
  `quire-cli render` 的 SVG／`dump` 實據；色彩軸沿用 `render_lab.py`
  （150dpi、`SAT_MIN=25`、24-bin 色相×明度直方圖）。所有數字皆為本次實測。

---

## 0. 摘要

1. 兩個色軸地板的成因**不同**：
   - `fdo76591`：LO 的 1.386% 彩色覆蓋全部來自**三張點陣圖**（兩張 EMF 內嵌
     DIB＋一張 PNG），LO 的向量 fill 彩色面積為 0。EMF 本身幾乎只是
     「一張 DIB＋少量 state 記錄」。
   - `n779834`：LO 的 0.83% 彩色覆蓋全部來自**EMF 的 vector fill**（藍色浮水印
     路徑），唯一的點陣圖是灰階（彩色覆蓋 0.0）。本檔正文 0 字，LO PDF 也 0 字。
2. 全語料 400 檔中，真正會被渲染的 **media EMF 只有 16 parts／11 檔
   （2.75%）**，合計 562,908 B；其中**只有 3 parts／2 檔含
   `EMR_STRETCHDIBITS` 內嵌 DIB**，而這 3 parts 正好就是兩個目標檔。
3. 決策關鍵數字：路線 (b)「只抽 EMF 內嵌 DIB」估 **2–3 人日**，可讓
   `fdo76591` 色軸 **0.000 → 0.651～0.680**（若同時修 PNG 內嵌圖缺失可到
   **0.791**）；路線 (a)「完整 EMF 解碼器」估 **15–20 人日**，受益面也只有
   11 檔（2.75%），且對 `n779834` 的藍色 vector 才有機會修色。
4. `n779834` 的 DIB 是 24bpp 灰階，路線 (b) 對它的色軸仍然是 **0.000**；
   它需要路線 (a) 或路線 (c) 的豁免口徑。

---

## 1. 媒體部件與 EMF 記錄結構

### 1-1 目標檔 media parts

| 檔 | part | bytes | sha256 前 8 | 判讀 |
|---|---|---:|---|---|
| fdo76591 | `word/media/image1.emf` | 95,536 | `36406b96` | EMF，19 筆記錄，含 1 筆 `EMR_STRETCHDIBITS` |
| fdo76591 | `word/media/image2.emf` | 196,028 | `89101b12` | EMF，31 筆記錄，含 1 筆 `EMR_STRETCHDIBITS` |
| fdo76591 | `word/media/image3.png` | 6,091 | `e8924447` | 一般 PNG（非 EMF；見 §3-3 缺失說明） |
| n779834 | `word/media/image1.emf` | 111,368 | `efc02d13` | EMF，182 筆記錄，含 1 筆 `EMR_STRETCHDIBITS` |

三顆 EMF 的檔頭皆為 `01 00 00 00`（`iType=1, EMR_HEADER`），記錄鏈完整切到檔尾
（`record_well_formed=True`）。

### 1-2 EMF 記錄型別分佈（前 5 名）

| EMF | 記錄數 | 前 5 名（型別×次數） |
|---|---:|---|
| fdo `image1.emf` | 19 | `EMR_COMMENT×8`, `EMR_SAVEDC×2`, `EMR_RESTOREDC×2`, `EMR_SETICMMODE×2`, `EMR_HEADER×1`（第 5 名並列，取型別序最小） |
| fdo `image2.emf` | 31 | `EMR_COMMENT×8`, `EMR_SAVEDC×3`, `EMR_RESTOREDC×3`, `EMR_SELECTOBJECT×2`, `EMR_SETICMMODE×2` |
| n779834 `image1.emf` | 182 | `EMR_SETPOLYFILLMODE×22`, `EMR_MOVETOEX×21`, `EMR_BEGINPATH×16`, `EMR_ENDPATH×16`, `EMR_POLYBEZIERTO16×16` |

判讀：

- fdo 兩顆 EMF 幾乎沒有 vector 繪圖；主體就是 `EMR_STRETCHDIBITS`。`COMMENT`
  是 GDI+ 常見的嵌入備註（8 筆），`SAVEDC/RESTOREDC/SELECTOBJECT/SETICMMODE`
  是 state 管理。
- n779834 是典型的 GDI+ 向量浮水印：`BEGINPATH→ENDPATH→FILLPATH` 路徑迴圈
  16 次，靠 `SETWINDOWEXTEX`／`SETVIEWPORTEXTEX`／`MODIFYWORLDTRANSFORM`
  做座標變換。這正是「完整解碼器」成本高的來源。

### 1-3 `EMR_STRETCHDIBITS` 內嵌 DIB（路線 b 的可行性核心）

| EMF | DIB 寬高 | bpp | 壓縮 | dest 左上 | dest 寬高 | bits bytes | header rclFrame |
|---|---|---|---:|---|---:|---:|---|
| fdo `image1.emf` | 166×183 | 16 | `BI_RGB(0)` | (5167, 100) | 701×776 | 60,756 | 83.9×93.1 pt |
| fdo `image2.emf` | 750×122 | 16 | `BI_RGB(0)` | (0, 0) | 3512×571 | 183,000 | 210.7×34.2 pt |
| n779834 `image1.emf` | 282×125 | 24 | `BI_RGB(0)` | (0, −125) | 282×125 | 106,000 | 706.0×1039.1 pt |

三顆全部是**未壓縮 BI_RGB**，可直接解成 RGB，不需要 EMF vector 引擎：

- 16bpp 為 5-5-5（X1R5G5B5），需展開成 8-8-8；資料列為 bottom-up。
- 24bpp 為 BGR bottom-up。
- n779834 的 DIB 彩色覆蓋率實測 **0.0**（純灰階）；它的藍色來自 vector fill，
  不是這張 DIB。

---

## 2. LO 側 ground truth：LO 到底畫了什麼、佔多少面積

### 2-1 逐頁總量

| 檔 | 頁 | 頁尺寸 (pt) | 文字字元 | drawings | fills | 彩色 fills | fill 面積/頁 | 彩色 fill 面積/頁 | 整頁彩色覆蓋 | images |
|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| fdo76591 | p0 | 612.0×792.0 | 691 | 12 | 3 | **0** | 4.78% | **0.00%** | **1.386%** | 3 |
| n779834 | p0 | 595.3×841.9 | **0** | 17 | 17 | **4** | 2.37% | **1.87%** | **0.83%** | 1 |

### 2-2 LO 的點陣圖實據（PDF 內已被 LO 轉成 JPEG）

| 檔 | 來源推測 | bbox (pt) | 版面寬高 | 原像素 | 彩色覆蓋 |
|---|---|---|---:|---:|---:|---:|
| fdo76591 | `image3.png`（品牌 logo） | [57.7, 64.8, 553.5, 139.8] | 495.8×75.0 | 490×100 | **5.10%** |
| fdo76591 | `image2.emf` 的 DIB | [201.1, 732.8, 411.6, 767.3] | 210.5×34.5 | 750×122 | **0.30%** |
| fdo76591 | `image1.emf` 的 DIB | [264.0, 594.0, 348.0, 687.0] | 84.0×93.0 | 166×183 | **61.56%** |
| n779834 | `image1.emf` 的 DIB | [467.5, 60.1, 601.7, 119.6] | 134.2×59.5 | 282×125 | **0.00%** |

判讀：

- `fdo76591` 的 LO 彩色 100% 來自點陣圖：`image1.emf` 的 DIB 一張就貢獻
  61.56% 自身覆蓋；`image2.emf` 幾乎無彩；PNG logo 貢獻少量。LO 的 3 個
  fill 全是黑/白，彩色 fill 面積 0。
- `n779834` 的 LO 彩色 100% 來自 4 個**藍色 vector fill**（總面積 1.87%）；
  唯一點陣圖是灰階（彩色 0.00%）。換句話說，兩檔的色軸地板需要不同解方。

---

## 3. 我方側：灰色佔位框實據與位置對照

### 3-1 佔位框色值與座標

我方對不可渲染格式一律發 `Rect` 佔位，`dump` 的 `fill_argb` 實測為
`0xFFF0F0F0`（SVG `fill="#f0f0f0"`），外框 `Rule` 為
`#cccccc`（`0xFFCCCCCC`）——任務描述的「#ccc?」是**邊框**色，填色其實是
`#f0f0f0`。SVG 內**沒有任何 `<image>` 元素**，整頁彩色覆蓋實測 0.0000。

| 檔 | 佔位 rect（x, y, w, h，pt） | 來源（VML shape 宣告） |
|---|---|---|
| fdo76591 #1 | 256.0, 590.5, 99.9, 100.2 | `left:220.05pt; margin-top:554.45pt; width:99.95pt; height:100.2pt` |
| fdo76591 #2 | 193.8, 729.2, 225.0, 45.0 | `left:157.8pt; margin-top:693.2pt; width:225pt; height:45pt` |
| n779834 #1 | 70.9, 28.4, 593.0, 872.8 | 浮水印 VML shape `width:593pt; height:872.8pt` |

### 3-2 與 LO 圖形位置對照

| 圖 | LO bbox (x,y,w,h) | 我方佔位 (x,y,w,h) | 差值（我方−LO） | EMF rclFrame |
|---|---|---|---|---|
| fdo `image1.emf` | 264.0, 594.0, 84.0, 93.0 | 256.0, 590.5, 99.9, 100.2 | −8.0, −3.5, **+15.9, +7.2** | 83.9×93.1 |
| fdo `image2.emf` | 201.1, 732.8, 210.5, 34.5 | 193.8, 729.2, 225.0, 45.0 | −7.3, −3.6, **+14.5, +10.5** | 210.7×34.2 |
| fdo `image3.png` | 57.7, 64.8, 495.8, 75.0 | **無** | — | — |
| n779834 `image1.emf` | 467.5, 60.1, 134.2, 59.5 | 70.9, 28.4, 593.0, 872.8 | **−396.6, −31.7, +458.8, +813.3** | 706.0×1039.1 |

判讀：

- fdo 兩個佔位框位置大致對，但寬高大約多 7%～30%；原因是**我方用 VML
  shape box，LO 用 EMF header `rclFrame` 的自然尺寸**（84×93、210.7×34.2）。
  路線 (b) 若要用對尺寸，應以 `rclFrame`（或 STRETCHDIBITS dest 經 transform）
  為準，而不是 VML box。
- n779834 的佔位框是**滿版 593×872.8pt**（頁面 595.3×841.9pt，面積比 103%，
  右下已溢出頁面），來自浮水印 VML shape 宣告；LO 實際只畫 134.2×59.5pt。
  這是獨立的幾何問題：若走路線 (c) 的虛線＋文字，必須先修這個幾何，否則會得到
  一張幾乎滿版的虛線框。

### 3-3 相鄰但非 EMF 的發現：`image3.png` 在我方完全缺失

`fdo76591` 的 `rId9 → word/media/image3.png` 是品牌 logo，位於一個大型背景
VML 文字框內。我方 `dump` 沒有 `Image` 項，SVG 也沒有 `<image>`；LO 則正常
畫出（495.8×75pt，彩色覆蓋 5.10%）。這是**文字框內嵌圖投影缺失**，不是 EMF
問題，但會壓低 fdo 的可恢復色軸上限：只修 EMF DIB 最多到 0.680，連 PNG 一起修
才到 0.791。建議另開卡處理，不併入 EMF 決策。

---

## 4. 全語料 EMF 市場規模（400 檔 docx）

| 類別 | parts | 檔數 | 總 bytes | 最大單檔 | 佔全語料檔數 |
|---|---:|---:|---:|---:|---:|
| **media EMF**（真正參與渲染） | **16** | **11** | **562,908** | 196,028 | **2.75%** |
| media WMF | 20 | 12 | 317,581 | 245,030 | 3.00% |
| `docProps/thumbnail.emf`（文件縮圖，非內容） | 8 | 8 | 623,000 | 390,304 | 2.00% |
| `docProps/thumbnail.wmf` | 2 | 2 | 7,408 | 5,568 | 0.50% |
| media EMF ∪ media WMF | — | **21** | — | — | **5.25%** |

- media EMF 中含 `EMR_STRETCHDIBITS`：**3 parts／2 檔**。
- media EMF 中純 vector／text（無 DIB）：**13 parts／9 檔**。
- 若把 `docProps/thumbnail.emf` 也算進來，全語料 EMF 共 24 parts／1,185,908 B
  （1.13 MB）；但 thumbnail 不是頁面內容，寫解碼器的投資不應以它為分母。

### 4-1 media EMF 清冊（依 bytes 降序）

| 檔 | part | bytes | records | `EMR_STRETCHDIBITS` | rclFrame (pt) |
|---|---|---:|---:|---:|---|
| ooxmlexport/fdo76591.docx | image2.emf | 196,028 | 31 | 1（750×122 16bpp） | 210.7×34.2 |
| ooxmlexport/n779834.docx | image1.emf | 111,368 | 182 | 1（282×125 24bpp） | 706.0×1039.1 |
| ooxmlexport/fdo76591.docx | image1.emf | 95,536 | 19 | 1（166×183 16bpp） | 83.9×93.1 |
| poi/…VariousPictures.docx | image3.emf | 87,004 | vector | 0 | 191.5×133.5 |
| ooxmlexport/tdf122563.docx | image1.emf | 13,492 | vector/text | 0 | 256.0×64.0 |
| ooxmlexport/artistic_effects.docx | image2.emf | 10,752 | vector/text | 0 | 68.8×44.9 |
| poi/…tika-3388.docx | image1.emf | 10,240 | vector/text | 0 | 65.3×42.5 |
| poi/…VariousPictures.docx | image4.emf | 6,184 | vector | 0 | 168.3×101.9 |
| poi/…WordWithAttachments.docx | image3.emf | 5,396 | vector/text | 0 | 86.4×55.2 |
| ooxmlimport/…embeddedDocxIcon.docx | image1.emf | 5,276 | vector/text | 0 | 102.0×66.9 |
| phpword/…reader.docx | image2.emf | 5,224 | vector/text | 0 | 103.4×66.9 |
| poi/…WordWithAttachments.docx | image4.emf | 5,132 | vector | 0 | 372.9×142.0 |
| poi/…WordWithAttachments.docx | image5.emf | 3,392 | vector | 0 | 160.3×50.7 |
| poi/…EmbeddedDocument.docx | image1.emf | 2,972 | vector/text | 0 | 216.8×16.4 |
| ooxmlimport/ole-anchor.docx | image1.emf | 2,960 | vector/text | 0 | 42.7×13.2 |
| poi/…WordWithAttachments.docx | image2.emf | 1,952 | vector/text | 0 | 467.7×41.4 |

結論：**路線 (b) 覆蓋 3/16 parts，但這 3 parts 佔 media EMF bytes 的 71.6%**
（402,932／562,908），且正好是兩個色軸地板檔中唯一有點陣主體的 fdo。
路線 (a) 才能覆蓋剩下的 13 parts／9 檔，但那些多為小圖示、文字型 EMF 或
WordWithAttachments 的附加圖，對色軸影響小。

---

## 5. 三條路線：成本、受益、風險

### (a) 完整 EMF → DisplayList 解碼器

- **工時粗估：15–20 人日**（記錄解析 3–4、座標系／world transform／裁剪 4–5、
  pen/brush/palette/path（含 16-bit 變體）4–5、文字 `EXTTEXTOUTW`＋字型映射
  3–4、測試與語料回歸 2–3）。若只做本語料出現的子集可壓到 8–10 日，但那就
  不是「完整」，遇新檔仍會破。
- **受益檔數：11 檔 media EMF（2.75%）／16 parts**；若含 WMF 再 +12 檔
  （聯集 21 檔／5.25%），但 WMF 是另一格式，不在本卡範圍。對 `n779834` 的
  藍色 vector 浮水印，這是唯一能真正補回 1.87% 彩色 fill 的路線。
- **風險**：EMF 座標系（`rclFrame`／window／viewport／world transform）與裁剪
  很容易錯；字型度量與 LO 不會完全一致；ROP、16-bit path、palette、BI_BITFIELDS
  等分支多；畸形 EMF 有 panic／OOM 風險（需 fuzz）；語料只有 11 檔，容易過擬合；
  長期維護成本最高。

### (b) EMF 內嵌 DIB 直讀（`EMR_STRETCHDIBITS` 抽位圖當圖片渲染）

- **工時粗估：2–3 人日**（EMF 記錄掃描＋`BITMAPINFOHEADER` 解析 0.5–1、DIB
  16/24/32bpp→RGB 0.5–1、座標／縮放（`rclFrame` 或 dest×transform）0.5–1、
  測試與 fallback 0.5–1）。若再加 BI_PNG/BI_JPEG、BI_BITFIELDS、palette 可到
  4 日。
- **受益檔數：3 parts／2 檔**，但涵蓋 media EMF bytes 的 71.6%，且直接解掉
  `fdo76591` 的色軸地板。
- **實測預測**（以 `render_lab.score_color` 對 LO 頁模擬）：

| 情境 | 彩色覆蓋 | 色軸 |
|---|---:|---:|
| 現況（灰佔位） | 0.0000 | 0.000 |
| (b) DIB 貼我方佔位框 | 0.0123 | **0.651** |
| (b) DIB 依 `rclFrame` 尺寸貼我方錨點 | 0.0096 | **0.680** |
| (b) DIB 貼 LO 實測框（幾何上限） | 0.0096 | 0.680 |
| (b) DIB ＋修好 PNG 內嵌圖 | 0.0128 | **0.791** |
| (b) 套在 `n779834`（DIB 灰階） | 0.0000 | **0.000** |

  → `fdo76591` 可從 0.000 救到 0.65～0.79；`n779834` 完全不受益（需 (a) 或 (c)）。
- **風險**：16bpp 5-5-5 展開與 JPEG 重編碼有色偏；dest 到頁面的 transform／
  裁剪若做錯會位移；遇到沒有 `EMR_STRETCHDIBITS` 的 EMF 仍要 fallback 佔位；
  不是萬用 EMF（vector/text EMF 仍空白）；`n779834` 的 transform 較複雜
  （`SETWINDOWEXTEX 7176×10554`、`SETVIEWPORTEXTEX 2392×3518`、
  `MODIFYWORLDTRANSFORM`），DIB 定位要特別驗。

### (c) 維持灰佔位，改虛線框＋「EMF 無法渲染」文字，並在色軸加 EMF 豁免

- **工時粗估：0.5–1 人日**（佔位樣式 0.25、文字標記 0.25、`render_lab` 豁免口徑
  0.25、測試 0.25）。注意本卡禁改 `render_lab.py`／`out/`，這是後續卡工作。
- **受益檔數：11 檔全部**（誠實 UX）；但色軸分數本身不變（fdo 仍 0.000），
  只能靠豁免把 EMF 頁排除或降權，等於承認不渲染。
- **風險**：豁免口徑可能掩蓋真回歸，必須用「頁面含不可渲染 EMF」白名單而非
  整檔豁免；文字標記若進內容軸會污染 token（需排除或改用非文字標註）；
  `n779834` 佔位框是滿版 593×872.8pt，虛線＋文字會非常醜，需先修幾何（改用
  `rclFrame`／imagedata 尺寸或裁到頁內）；「美化」不解決色軸地板，只是把
  不可比說清楚。

### 成本效益對照

| 路線 | 工時 | 受益檔數 | fdo 色軸 | n779834 色軸 | 主要風險 |
|---|---:|---:|---:|---:|---|
| (a) 完整解碼器 | 15–20 日 | 11 檔（2.75%） | 可望接近 LO | 可望補回 vector 藍 | 座標／字型／fuzz／過擬合 |
| (b) DIB 直讀 | **2–3 日** | 2 檔（0.5%），但 71.6% bytes | **0.65–0.79** | 0.000 | 色偏／定位／非萬用 |
| (c) 佔位美化＋豁免 | 0.5–1 日 | 11 檔（UX） | 0.000（豁免） | 0.000（豁免） | 掩蓋回歸／滿版框很醜 |

---

## 6. 推薦

**推薦路線：(b) EMF 內嵌 DIB 直讀。**

一句話理由：media EMF 的 71.6% bytes 與 `fdo76591` 色軸地板的全部彩色來源都
集中在 3 筆 `EMR_STRETCHDIBITS`，2–3 人日就能把 fdo 色軸從 0.000 拉到
0.65～0.79，遠比 15–20 人日的完整解碼器划算；`n779834` 的 vector 藍色浮水印
再以路線 (c) 的誠實佔位＋精準豁免收尾。

---

## 附錄：可重現指令

```bash
# 一鍵跑完本報告所有數字（暫存落 /tmp，不污染工作區）
python3 scripts/p5_emf_probe.py --out /tmp/p5-emf-probe --ours /tmp/p5-ours

# 只跑靜態掃描（不轉 LO、不 render）
python3 scripts/p5_emf_probe.py --skip-lo --skip-render

# 產物
#   /tmp/p5-emf-probe/p5_emf_probe.json   全部量測值
#   /tmp/p5-emf-probe/lo/*.pdf            LO ground truth
#   /tmp/p5-ours/<stem>/*.svg             我方 render
```

LO 轉檔後探針會執行 `tasklist` 檢查 `soffice` 殘留，必要時以
`taskkill //F //IM soffice.exe` 清理；本次實測殘留為 0。
