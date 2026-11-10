# K22：tblStylePr 條件字色級聯——表頭白字落地（fdo68607 header）

## 1. 缺陷與根因

`corpus/ooxmlexport/fdo68607.docx` 表頭列（Drama Vocabulary／Definition）：
LO 渲染＝藍底（#4F81BD）**白字**；我方（K22 前）渲染＝藍底**黑字**
（SVG 實測 text fill=#000000）。

根因：表格 style＝`MediumShading1-Accent1`，其
`w:tblStylePr w:type="firstRow"` 同時宣告

```xml
<w:rPr><w:b/><w:bCs/><w:color w:val="FFFFFF" w:themeColor="background1"/></w:rPr>
<w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="4F81BD" w:themeFill="accent1"/></w:tcPr>
```

K8-shd 級聯（`core/engine-api/src/table_map.rs`）只提取 `w:tcPr/w:shd`
（底色層）；同節點 run 級 `w:rPr/w:color`（字色層）在**解析層**就被丟掉
（舊 `parse_tbl_style_pr_shd` 只認 tcPr 直系 shd）。

## 2. 處方（實作面）

條件字色與條件底色**同軸**：同一張生效閘表（`compute_cond_gates`——
`w:tblLook` 六旗標、排除特殊列的 1-based 帶序、欄序知識閘）、同一個
ECMA-376 §17.4.75 優先序（`COND_PRIORITY`：firstRow < lastRow < firstCol
< lastCol < band1Horz < band2Horz < band1Vert < band2Vert < 四角），
逐條件獨立查（某條件未宣告字色＝續下一條，非整條件覆蓋——與 K8 底色
逐屬性級聯同款）。優先序末端：

```
直接 run 字色（w:rPr/w:color）＞ 級聯鏈（字元/段落樣式/docDefaults）
  ＞ tblStylePr 條件字色（本卡新增層）
```

資料流四段：

1. **解析**（`core/ooxml/src/styles.rs`）：`parse_tbl_style_pr_shd` 重構為
   `parse_tbl_style_pr`——同棵 `tblStylePr` 子樹共吃 `w:tcPr/w:shd` ＋
   **直系** `w:rPr`（只留色彩三態 color／color_set；`w:pPr/w:rPr` 是段落
   標記語意，不混收）。新欄 `OwnStyle::cond_color` ＋公開查表
   `table_style_cond_color(id, cond)`（basedOn 鏈）＋ `has_cond_color()`
   （開檔快路）。`w:color val="auto"` 維持三態（color_set 真、color
   None＝明確黑），由 engine 端落地 `0xFF000000`。
2. **查表**（`core/engine-api/src/table_map.rs::cond_run_colors`）：逐表
   （含巢狀，樣式 id 自帶＞繼承外層，ECMA §17.4.65）逐格算條件集合→
   `COND_PRIORITY` 序首個宣告字色者定案→烘焙到格內段落索引 Map。全檔
   無任何條件字色宣告＝整層跳過（corpus 378/400 檔零成本）。
3. **級聯烘焙**（`core/engine-api/src/lib.rs::cascade_paragraphs`）：
   `PitchCascade` 新增 `cond_color` 束欄；run 級聯鏈**完全沒宣告顏色**
   （`eff.color_set == false`）時才落 `Run::color = Some(條件色)`。
   直接格式／字元樣式／段落樣式／docDefaults 皆在上游、自然優先；超連結
   合成藍在 `with_hyperlink_default` 已把 color_set 置真，先於條件層贏
   （LO 口徑：連結色壓過表格條件色）。
4. **發射**：既有 run 著色鏈零改動（`resolve_cell_line_style` 讀
   `r.color`→DrawItem::Text.color_argb；`cell_run_spans` 併段鍵含
   color_argb 自然吸收新色）。

回塞鎖：`K22_COND_COLOR_BUGGY=1`＝查表層整滅（條件色回黑；直接色路徑
不動——機制分離）。`K8_SHD_BUGGY=1`（底色回塞）不受本卡影響。

## 3. 紅燈（`core/engine-api/tests/k22_conditional_color.rs`，6 test）

- **燈1** fdo68607 p0：「Drama Vocabulary」／「Definition」發射
  color_argb＝0xFFFFFFFF（恰等匹配——標題「IGCSE Drama Glossary」同樣含
  "Drama"，contains 口徑會假紅，實測踩到後改恰等）。修前必紅（黑）。
- **燈1b** 零回退鎖：表頭藍底 4F81BD 仍在（K8 燈2 同尺）。
- **燈2** 合成最小檔 `fixtures/k22_cond_color.docx`（產生器
  `gen_k22_fixture.py` 入庫）：firstRow 紅字命中 H2；資料列第 2 欄維持
  預設黑（不外溢）；firstCol 藍字命中 R1C1／R2C1（不得只特化 firstRow）；
  band1Vert 黃／lastCol 白宣告被 `tblLook 04A0`（noVBand=1／lastColumn=0）
  閘死，全頁零黃零白字。
- **燈2b** 零回退鎖：合成檔 band1Horz 橫帶 D3DFEE 底色仍在（firstCol 只
  宣告字色不宣告 shd 時，底色續條下一條件——逐屬性級聯同軸驗證）。
- **燈3** 直接字色覆蓋鎖：H1（firstRow∩firstCol 交會格）run 直接
  color=00AA00 → 綠勝（紅=firstRow 誤勝、藍=firstCol 誤勝皆紅——三色
  互斥，優先序寫錯必紅其一）。
- **回塞鎖**：BUGGY 態燈1／燈2 條件色必回黑、H1 直接色仍綠。

單元鎖（`core/ooxml/src/styles.rs` tests）：直系 rPr 解析（auto 三態、
非色彩欄不收）、**rPr 排在 tcPr 之後**的順序鎖（舊實作首見 shd 即返會
漏收）、pPr/w:rPr 不混收、basedOn 鏈繼承。

## 4. 驗證

- `bash ~/.hermes/scripts/ow-gate.sh <ws>` 八閘全綠（見本卡 completion
  metadata 的 GATE_SUMMARY）。
- `cargo run -p quire-cli -- render corpus/ooxmlexport/fdo68607.docx`：
  p0 `fill="#ffffff">Drama`／`fill="#ffffff">Definition`（實測 SVG）。
- 回塞態 `K22_COND_COLOR_BUGGY=1 cargo test -p quire-engine --test
  k22_conditional_color`＝燈1／燈2 紅（4 pass 2 fail，紅的正是兩盞）。
- k8_shd 5/5 綠（底色級聯零回退）；roundtrip corpus 399/399
  byte-identical（純讀取派生資料，存檔路徑零觸碰）。

## 5. corpus 影響面（實測掃描，非推估）

`styles.xml` tblStylePr 直系 rPr 帶 `w:color` 宣告的檔案＝**22/400**
（firstRow 白字族 481 條、lastRow navy 族、swCell navy 192 條、
firstCol/lastCol FFFFFF 族、band 族 auto 等）。這 22 檔的格內文字顏色
向 LO 收斂（本卡目的）；其餘 378 檔 `has_cond_color()`＝false 整層跳過，
零位元組擾動。無一檔命中「shd 之後還有 rPr/pPr」的舊解析器洩漏態
（掃描 0 檔），讀盡重構零行為回退。

## 6. 邊界與老實聲明

- `w:tcPr/w:rPr`（Word 2013+ 儲存格級 run 屬性）不存在於本層契約——
  直接 tcPr 字色走 run 直接格式既有路徑，本卡未新增 tcPr 級直接色
  （corpus 無活例，不虛構）。
- 條件粗體／底線（同 rPr 的 b/i/u 欄）本卡不收（卡面只要求字色軸；
  fdo68607 表頭的 `<w:b/>` 條件粗體未落地，留給後續卡——紅燈面零影響：
  LO 表頭字重與我方現狀一致由直接格式承擔）。
- 頁首尾表格＝零擾動（K8 同款口徑：ctx 不進頁首尾路徑）。
- 巢狀表無自身 tblLook 時沿用 `compute_cond_gates` 的預設閘（ECMA
  0x04A0），與 K8 底色級聯同款近似。
