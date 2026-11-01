# K24：ellipse 直形實心填色丟失——`wps:style` 樣式矩陣落地實錄

任務卡：t_02d9e0d8（K24）。日期：2026-11-01。改動面：`core/ooxml/src/drawing.rs`、
`core/ooxml/src/theme.rs`、`core/engine-api/tests/k24_style_matrix.rs`（新）、
`core/engine-api/tests/fixtures/k24_minimal_style_fill.docx`（新）。零碰
scripts/、corpus/、out/。

## 1. 根因（實測，推翻卡面假說）

卡面假說「DrawingML 裸 `e:ellipse` 的 `a:solidFill` 未進渲染鏈」**不成立**：
`corpus/poi/test-data_document_shapes-with-text.docx` 的全部 11 個 `wps:wsp`
（ellipse／3×rect／triangle／rightArrow／straightConnector1／4×star5，含
2 個 `wpg:wgp` 群組的子形）的 `wps:spPr` **根本沒有 `a:solidFill`**——該檔
僅有的 4 個 solidFill 全在兩個文字框的 spPr（白底 `schemeClr lt1`，既有
行為正確）。

色源在 `wps:style` **樣式矩陣引用**（DrawingML style matrix reference）：

```xml
<wps:style>
  <a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="15000"/></a:schemeClr></a:lnRef>
  <a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef>
  <a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef>
  <a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef>
</wps:style>
```

`idx` 是 theme part `fillStyleLst`/`lnStyleLst` 的樣式索引（idx=0＝ECMA 明確
「不用」），顏色由 ref 子節點 `a:schemeClr`（可帶修飾器）給。theme1.xml
accent1=`156082`——與 P2 基線報告所記 LO 彩色覆蓋源 #156082 吻合。

**交叉驗證（Word 快取真值逐位吻合）**：同檔每個圖形的 `mc:Fallback` VML 表示
都帶 Word 預算快取欄 `fillcolor="#156082 [3204]" strokecolor="#030e13 [484]"`。
`030e13` ＝ accent1 `156082` 每通道 `floor(c×0.15)`（0x15→3、0x60→0x0E、
0x82→0x13）——證明 (a) fillRef→accent1、lnRef→accent1+shade15% 的折色路徑正確；
(b) **DrawingML 修飾器在 Word 的折算口徑是 8bit floor**（`theme.rs::
apply_shading_transform` 的 corpus 校準式），不是 K19 duotone 的線性域式
（後者有 LO 內嵌圖反推實據，域不同，勿混用——本卡 lnRef 折色實測命中
`#030e13` 逐位零誤差）。

**落點判定（三選一）**：ooxml 解析丟。全 repo grep `fillRef` 於改動前零命中
＝樣式矩陣解析鏈從未存在；layout／渲染 emit 下游本来就会消费 fill_argb
（K12 鏈就在那），上游沒送貨。

## 2. 修法（`core/ooxml/src/drawing.rs`）

新增 `StyleMatrix` 狀態機（宿主候補與群組子形各持一份），在
`apply_graphic_elem`（Start＋Empty 兩態共用入口）採集、End 完結折算：

- 門閂三層：`wsp_ctx`（`wps:wsp` 包圍內才認——圖表 `c:style` 子樹的
  `c:lnRef`/`c:fillRef` **同名不同物**，紅燈 `k24_style_outside_wsp_ignored`
  實鎖）＋ `style` 直下（ref）＋ ref 直下（顏色節點與修飾器）。
- `fillRef idx≥1`／`lnRef idx≥1` → 子節點折色落 `StyleMatrix.fill/stroke`
  （首見為準）；`idx=0`／缺＝不發色。
- 消費點（`push_pending`／`group_shapes`）：**既有優先序全缺才退矩陣折色**——
  `fill_argb = spPr solidFill > fillcolor attr > wps:style fillRef`（ECMA
  直接格式壓樣式矩陣）。既有 spPr solidFill 檔案一個位元組都不動（序列化
  零位移，roundtrip 399/399 實證）。
- 折色算式 `fold_style_node`：'s'＝8bit floor(c×k)（Word 快取實據）、
  't'＝朝白插值（同 `apply_shading_transform`）、'm'＝8bit Rec.601 luma
  飽和調製（同 K19 'm'）。
- `theme_slot_for` 補 DrawingML 槽位別名 `dk1/lt1/dk2/lt2`（ECMA ST_SystemColorVal
  同時收 `dk1` 與 `dark1` 兩式；DrawingML `wps:style` 慣用前者——實測本檔
  fontRef 用 `lt1`）。
- 回塞鎖：`K24_STYLE_BUGGY=1` → 採集整條閉嘴＝折色永不落地（照 K4D/K19
  「`with_theme()` 讀一次 env」先例，熱路徑零 env 查詢）。

`DrawItem` 無橢圓／星形圖元型別，且 render-trait／shells 不在本卡白名單——
填色以 **Rect 發射**（色軸按 24-bin 色直方圖計分，形状差屬渲染基元缺口，
另立新卡；LO 真值是橢圓／星形／箭頭輪廓）。

## 3. 紅燈全綠（先紅後綠實錄）

| 燈 | 規格 | 修前 | 修後 |
|---|---|---|---|
| 燈1 | shapes-with-text p0 發 #156082 實心 Rect（面積>0，ellipse 尺寸級 146.3×69.0±3pt） | 0 個＝紅 | 通過（SVG 實數 8 個 `fill="#156082"`） |
| 燈2 | 合成最小檔 fixtures/k24_minimal_style_fill.docx：ellipse 僅 fillRef accent1(FF0000)→發 FF0000 圖元；spPr solidFill 00FF00 壓 fillRef accent2(0000FF)→0000FF 不得出現 | 紅 | 通過 |
| 燈2b-d（ooxml 層） | fillRef→accent1＋lnRef shade→#030e13 逐位鎖；idx=0→None；群組子形同口徑；c:style 不收 | 紅 | 通過（5 新測） |
| 燈3 | K12 WC048 三藍圖元（2 Rule＋1 Rect `0xFF4472C4`）逐位零回退 | 綠 | 綠 |
| 回塞鎖 | `K24_STYLE_BUGGY=1` → 燈1／燈2 必紅 | — | 實測 2 紅 |

render_lab 單檔實測（LO 對齊，Windows soffice，150dpi）：
shapes-with-text p0 色分 **0.000 → 0.632**（BUGGY 回塞態回 0.000）；內容分
1.000、版面分 0.977 零回退。殘差 0.368 主因＝Rect 近似 vs LO 橢圓/星形/箭頭
真實輪廓（面積比）＋群組子形 z 序——屬渲染基元缺口，非填色鏈問題。

全語料掃描（`quire-cli dump` 逐檔 base main c87dcd7 vs 本支）：文字項 399 檔
逐位零差異；Rect 色分佈淨變化＝佔位灰 F0F0F0 −12 → 真色（白 lt1 +29、
accent1 4F81BD +16、5B9BD5 +13、156082 +8、lt2 E7E6E6 +2）——全為「灰佔位
改真色」方向，無反向案例。抽樣 A/B（render_lab 三檔）：fdo66688 色
**0.0→0.999**、dml-groupshape-paraadjust **0.0→0.988**、tdf131841 持平
（其 0.0 屬內容軸既有缺陷，非本卡面）。

## 4. 全閘門

fmt ✓／clippy -D warnings ✓／`cargo test --workspace` 全綠（engine-api 163、
ooxml 190、+3 新 engine 燈、+5 新 ooxml 燈）／goldens 三件零位移 ✓／
roundtrip corpus 399/399 byte-identical ✓／wasm32 check ✓／分層 ✓／
verify_corpus PASS ✓。零回退 A/B：WC048（色 0.947 同基線）、tdf153791
（色 0.0 同基線，fontRef 干擾檔不受本改動影響）；全語料文字項逐檔比對
base vs 本支＝零差異（見 completion metadata）。

## 5. 已知殘差（留口徑，不掩蓋）

1. **形状基元缺口**：橢圓/星形/三角形以 Rect 發射（render-trait 無 path 圖元；
   白名單外）。跟進卡：DrawItem 增 `Ellipse`/`Path` ＋ SVG 端發射 → 該檔色分
   0.632 還有上空間（LO 橢圓面積僅外切矩形的 ~78.5%）。
2. **群組 z 序**：群組子形與宿主框內文字的重疊序（LO 箭頭壓過 star5 之間），
   本卡不動既有發射序。
3. `effectRef`/`fontRef` 仍不消費（陰影/字型矩陣屬渲染面，非填色）。
4. `a:scrgbClr`（Office 2013 擴充色）不認識＝不虛構（本檔 rect 文字框的
   lnRef idx=0 走 `scrgbClr` 節點，恰 idx=0 不受影響）。

## 6. fixture 生成腳本

`core/engine-api/tests/fixtures/k24_minimal_style_fill.docx` 由本卡 scratch
腳本（zipfile 手搓，見 worktree 外 `gen_k24_fixture.py` 記錄於本檔附錄）生成；
形態＝shapes-with-text 縮影：inline＋wps:wsp×2（fillRef 路徑與 spPr 壓制
路徑對照），theme accent1=FF0000／accent2=0000FF。

附錄（生成腳本原文）：見 git blob `k24_minimal_style_fill.docx` 同卡 commit
說明；重建方法＝按 §2 消費點語意手搓 5 個 part（[Content_Types].xml、
_rels/.rels、word/_rels/document.xml.rels、word/document.xml、
word/theme/theme1.xml）。
