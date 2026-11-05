# F3 文字框內圖片未渲染解剖報告（dml-picture-in-textframe.docx）

署名：ADT109119

## 一、源檔實拆與幾何座標體系解剖

### 1. 源檔結構解剖（`corpus/ooxmlexport/dml-picture-in-textframe.docx`）

該檔為標準 DrawingML 文字框內嵌 DML 圖片結構，頂層路徑為：
`<w:p>` → `<w:r>` → `<mc:AlternateContent>` → `<mc:Choice Requires="wps">` → `<w:drawing>` → `<wp:anchor>`。

文字框外框定義（`wps:wsp`）：
- 外層錨定：`<wp:anchor>`，`relativeHeight="251659264"`，`layoutInCell="1"`，`allowOverlap="1"`。
- 水平位置：`<wp:positionH relativeFrom="column"><wp:align>center</wp:align></wp:positionH>`，頁面水平居中。
- 垂直位置：`<wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>`，段落偏移 0 EMU。
- 外框尺寸：`<wp:extent cx="2388870" cy="2449830"/>`。以 1 pt = 12700 EMU 換算，寬度為 188.1 pt，高度為 192.9 pt。
- 形狀變換：`<wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2388870" cy="2449830"/></a:xfrm>...</wps:spPr>`，幾何尺寸與 `wp:extent` 完全對齊。
- 內邊距宣告：`<wps:bodyPr rot="0" vert="horz" wrap="square" lIns="91440" tIns="45720" rIns="91440" bIns="45720" anchor="t" anchorCtr="0"/>`。換算得左內縮 `lIns` = 7.2 pt、上內縮 `tIns` = 3.6 pt、右內縮 `rIns` = 7.2 pt、下內縮 `bIns` = 3.6 pt。可用排版內容區域寬度為 188.1 - 7.2 - 7.2 = 173.7 pt，高度為 192.9 - 3.6 - 3.6 = 185.7 pt。

文字框內嵌圖片定義（`wps:txbx` → `w:txbxContent`）：
- 內層段落：`<w:txbxContent><w:p><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0">`。
- 圖片尺寸宣告：`<wp:extent cx="2200275" cy="2200275"/>`。換算得寬度為 173.25 pt，高度為 173.25 pt。
- 圖片元素與資源引用：`<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rId5"/></pic:blipFill>...`。
- 關聯表查驗：`word/_rels/document.xml.rels` 中 `rId5` 指向 `media/image1.gif`。
- 圖片幾何變換：`<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2200275" cy="2200275"/></a:xfrm>...</pic:spPr>`。座標系為框內段落內容流之相對座標，原點 (0, 0) 隨文字流排版，其 `a:ext` 換算為 173.25 pt × 173.25 pt，與 `wp:extent` 完全一致。

### 2. 幾何位置與 bbox 對照（LO 真值 vs 框內範圍）

由 LibreOffice 產出之 PDF 實抓幾何真值（72 dpi pt 座標系）：
- 頁面尺寸：612.0 pt × 792.0 pt（標準 Letter 頁面）。
- 文字框矩形：`Rect(212.30, 70.80, 399.55, 251.15)`，寬 187.25 pt，高 180.35 pt。
- 圖片幾何 bbox：`Rect(219.55, 74.40, 392.80, 247.65)`，寬 173.25 pt，高 173.25 pt。
- 相對外框位移：左位移 219.55 - 212.30 = 7.25 pt（吻合 `lIns` 7.2 pt）；上位移 74.40 - 70.80 = 3.60 pt（吻合 `tIns` 3.6 pt）。
- 容納判定：圖片寬高 173.25 pt 落在框內可用寬高（173.7 pt × 185.7 pt）內部，圖片完整落在文字框邊界內，誤差小於 0.4 pt，符合 ±4pt 容許範圍。

## 二、我方現況對照與缺口定位

### 1. `quire-cli dump` 實抓輸出（main `10decfa`）

```json
[
  {
    "page": 0,
    "width_pt": 612.0,
    "height_pt": 792.0,
    "items": [
      {
        "Rect": {
          "x": 211.95001,
          "y": 70.85,
          "w": 188.1,
          "h": 192.9,
          "fill_argb": 4294967295
        }
      },
      {
        "Rule": {
          "x1": 211.95001,
          "y1": 70.85,
          "x2": 400.05002,
          "y2": 70.85,
          "width_pt": 0.75,
          "color_argb": 4278190080,
          "dash": null
        }
      },
      {
        "Rule": {
          "x1": 211.95001,
          "y1": 263.75,
          "x2": 400.05002,
          "y2": 263.75,
          "width_pt": 0.75,
          "color_argb": 4278190080,
          "dash": null
        }
      },
      {
        "Rule": {
          "x1": 211.95001,
          "y1": 70.85,
          "x2": 211.95001,
          "y2": 263.75,
          "width_pt": 0.75,
          "color_argb": 4278190080,
          "dash": null
        }
      },
      {
        "Rule": {
          "x1": 400.05002,
          "y1": 70.85,
          "x2": 400.05002,
          "y2": 263.75,
          "width_pt": 0.75,
          "color_argb": 4278190080,
          "dash": null
        }
      },
      {
        "Text": {
          "x": 70.85,
          "y": 70.85,
          "size_pt": 12.0,
          "flags": 0,
          "color_argb": 4278190080,
          "family": "Times New Roman",
          "text": ""
        }
      },
      {
        "Text": {
          "x": 214.95001,
          "y": 74.85,
          "size_pt": 12.0,
          "flags": 0,
          "color_argb": 4278190080,
          "family": "Times New Roman",
          "text": ""
        }
      }
    ]
  }
]
```

`render_lab` 實抓結果：
- 內容相似度：[None]，版面相似度：[None]，備註：`measurement_artifact:LO有圖形無可抽文字`。
- 色相似度：0.0，圖相似度：0.0。圖軸分實測為 0.0。

### 2. 缺口定位診斷

缺口精確落在「未採集」這一環。
1. 採集層硬性吞沒（`core/ooxml/src/drawing.rs` 第 2788 行）：解析器在 `self.graphic.is_some()`（外層 DrawingML 文字框進行中）時，遇到內層 `<w:drawing>` 即無條件執行 `self.skip_tag = Some((b"drawing", 0))`，將其當作竄位巢狀圖形全數吞沒，致使其內的 `wp:inline`、`wp:extent`、`a:graphic`、`pic:pic`、`a:blip` 等所有子標籤全部被直接跳過。
2. 採集狀態機單邊接線：R35 在 `core/ooxml/src/drawing.rs` 建立的 `TxbxFrame` 與 `txbx_drawings` 採集開關只掛在 `self.vml`（VML `v:textbox`）分支上；DrawingML `wps:wsp` 文字框路徑完全未曾接通 `open_slot`、`note_blip`、`note_extent` 與 `drain_txbx_frame`。
3. 發射層後果：外層文字框之 `InlineImage.txbx_drawings` 恆為空，發射層 `emit_txbx_texts` 在段落走訪時取得之 `candidates` 向量為空，無法取出任何像素，段落退化為空文字發射，整張圖片消失。

## 三、語料普查結果

以程式遍歷 `corpus/` 內全部 400 個 DOCX 檔案之解壓結果：
1. 含有 `w:txbxContent` 節點之檔案總數：115 檔。
2. 其中在 `w:txbxContent` 內部包含圖片元素（`pic:pic`、`a:blip` 或 `v:imagedata`）之檔案總數：9 檔。
   - 包含 DrawingML 格式 `wps:txbx` 且含圖片者共 2 檔：
     - `corpus/ooxmlexport/dml-picture-in-textframe.docx`
     - `corpus/ooxmlexport/fdo78474.docx`
   - 其餘 7 檔為 VML 格式 `v:textbox` 內嵌圖形，如 `fdo76591.docx`、`n792778.docx` 等。

## 四、明確不做事項界定

為守護變更最小化與各卡場邊界，本卡明確不做下列項目並留待對應卡場：
1. `a:blipFill` 的 `srcRect` 裁切解析（百分比子區塊取圖）。
2. 圖形進階效果（glow、shadow、softEdge 等，屬 F2 卡場）。
3. 圖表內部圖片（`c:chart` 結構）。
4. SVG 圖片（`svgBlip`）格式之 fallback 回退鏈。
