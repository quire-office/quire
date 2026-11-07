# F2 紅燈報告：`a:prstGeom/@prst` 未消費（非矩形形状全畫成外接矩形）

工作樹：`wt/t_ab254070`（基線 main `f6f83bf`）
考卷：`core/engine-api/tests/f2_wordart_projection.rs`（單一 suite，`F2_GEOM_BUGGY` 回塞鎖）
報告日期：2026-11-07

本報告每一條數字都由本卡實際跑過的指令支撑（指令見 §5），無一筆來自推測。

---

## 0. 卡面前提的實測更正（先講清楚，避免拿錯考卷打實作）

卡面把 `artistic_effects.docx` 的檔名（Word 的「藝術效果」圖片濾鏡 `a14:imgEffect`）
讀成了 DrawingML preset／WordArt 能力缺口。本卡接手後**逐檔實拆覆證**（不沿用前次
run 的自報，探針 `docs/f2-survey/f2v1_verify.py` 口徑），實測：

| 卡面假設 | `artistic_effects.docx` 實拆 |
|---|---|
| 有 `a:prstTxWarp`／`a:wordArt`（WordArt） | **0／0** |
| 有 `bevelT`／`softEdge`／`glow`／`effectLst` 效果節點 | **全 0** |
| `prstGeom` 有 rect/ellipse/triangle 等多形狀 | 僅 **2 個 rect**，且在 `pic:spPr`（圖片裁切框） |
| 有 `gradFill`／`solidFill` 待套填充 | **0／0** |
| 「我方整幅空白」（硬性合格線未過） | **不成立**：p0 現況發射 Image 2／Rect 1／Rule 4／Text 3 |
| 「LO 有圖形、我方必須有東西」 | **現況已成立**（燈1a 鎖之，防回退） |

圖軸 0.0 的真正成因是 **OLE 圖示的 EMF 向量記錄未光柵化**（`media/image2.emf` 記錄流
無 `EMR_STRETCHDIBITS`(type 81)，K27 閘門拒絕是正確行為），場域在 `emf_dib.rs`——
**不在本卡白名單**，已由解剖報告 §3／§6.3 記帳上報 PM，本卡不硬改。

## 1. 紅燈：真實缺口落在 `prstGeom/@prst` 從未被消費（燈1b）

前次解剖（§5.4）＋本卡普查復證：全語料本體 part `prstGeom` 590 次、**非 rect 195 次／約 25 檔**，
但 `@prst` 屬性在 `core/ooxml/src/*.rs` 實作路徑零消費（`b"prst"` 全庫 `grep` 於實作檔 0 命中），
所有非矩形形状一律退化為外接矩形（1 `Rect` ＋ 4 條軸對齊 `Rule`）。這是真實、可見、可量測的缺陷
（圓畫成方塊、星形畫成方塊、箭頭畫成方塊），比 WordArt 更貼近卡面原始意圖。

拍紅實測（`cargo test -p quire-engine --test f2_wordart_projection`，基線 `f6f83bf`）：

```
running 1 test
test f2_projection_suite ... FAILED

thread 'f2_projection_suite' panicked at core/engine-api/tests/f2_wordart_projection.rs:151:9:
tdf113183 star5 十邊輪廓（斜邊多數）必須發非軸對齊筆畫；實測 0（修前 0＝外接矩形退化）

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

燈1a（發射非空）在 suite 內先通過（未 panic 即綠），燈1b 第一條斷言即紅——本卡唯一硬性合格線
（不準發射空白）由燈1a 守住，真實缺口紅燈由燈1b 拍板。

判別式：**非軸對齊筆畫**。外接矩形退化態的四條邊恒 `x1==x2` 或 `y1==y2`；星形／三角形／橢圓／
連接線的邊是斜的（`x1≠x2 且 y1≠y2`）。修前三檔斜筆畫實測 **全 0**。

## 2. 我方退化態 vs LO 真值（逐檔實抓）

我方（`quire-cli dump`，`f2v2_ourdump.py`）：

| 檔 | prst | 我方 p0 發射 |
|---|---|---|
| `tdf113183` | star5、triangle | Rect 2／Rule 8／Text 1（Rule 全軸對齊，斜筆畫 **0**） |
| `tdf113258` | ellipse | Rect 1／Rule 4／Text 2（斜筆畫 **0**） |
| `shapes-with-text` | ellipse／star5×4／triangle／rightArrow／connector | Rect 11／Rule 44（斜筆畫 **0**） |
| `fdo74605` | roundRect／leftRightArrow／upArrow／connector | Rect 70／Rule 276（斜筆畫 **0**） |

LO 真值（pymupdf 抽 LO 轉換 PDF `get_drawings()`，逐邊端點，`f2v11_lo_lines.py`）：

| 檔／prst | LO 繪製物（端點實抓） |
|---|---|
| `tdf113183` triangle | 3 線段閉合：(549.0,558.5)→(579.7,492.5)→(610.5,558.5)→閉合 |
| `tdf113183` star5 | 10 頂點閉合折線：(59.3,299.0)(94.0,299.0)(104.8,267.8)(115.4,299.0)(150.1,299.0)(122.1,318.4)(132.8,349.5)(104.8,330.2)(76.7,349.5)(87.4,318.4) |
| `tdf113258` ellipse | 12 段三次貝塞爾（外接框 226.6×58.2） |
| `shapes-with-text` triangle | 3 線段：(347.2,265.5)→(395.2,187.5)→(443.3,265.5)→閉合 |
| `shapes-with-text` connector | 1 對角線：(92.2,266.2)→(272.8,236.3) |
| `shapes-with-text` ellipse | 12 段貝塞爾（外接框 146.3×69.0） |

LO 的星形頂點已逐點入 `core/ooxml/src/preset.rs` 的星形點表校準（非套公式猜數）；
三角形／連接線為直接幾何（頂點＝外接框角點），橢圓為 12 段貝塞爾折線近似。

## 3. 燈2（prst／warp 名稱逐字比對源檔）現況＝紅（佔位實作）

`shape_geom_facts()` 紅燈佔位實作恒回全 `None`（忠於「`@prst` 從未被讀取」的現況），
燈2 與燈3a 因此在佔位態即紅。綠燈 commit 由 `InlineImage.geom_prst`／`warp_prst` 欄位落地翻綠。

燈2 用 `ZipBundle::open(...).entry(...).decoded()` 在測試內**直接解源 zip** 抓 `@prst`，
不經引擎發射面（避免「比對自己產物」的循環論證）；雙向鎖（源檔每個非 rect 名稱都要在＋
我方不得虛構源檔沒有的名稱）。

## 4. 燈3（鑑別力）與燈4（零擾動）

燈3（`F2_GEOM_BUGGY=1`）與燈4（artistic_effects 兩態顯示列表逐位元組相同）在 suite 內
設／清 env 前後對照跑，**正常態段先取樣 JSON、回塞態段比對**——發射層讀 env 的時序污染
由「單一 suite＋集中讀取」規避（k25b／F1 紀律：env 回塞鎖拆多 test 平行跑會假紅，F1
里程碑四實測踩過，本卡直接採單一 suite）。綠燈後兩燈的證據補記於本報告 §6。

## 5. 證據與再跑指令

- 卡面前提覆證（目標檔＋代表檔節點實拆）：`/usr/bin/python3 docs/f2-survey/f2v1_verify.py`
- 我方退化態發射實抓：`/usr/bin/python3 docs/f2-survey/f2v2_ourdump.py <corpus 檔…>`
- 基線發射／SVG sha256 固化：`docs/f2-survey/f2_baseline_shapes.json`
- LO 真值（繪製物端點）：`/usr/bin/python3 docs/f2-survey/f2v11_lo_lines.py`
- 零回退點名檔 prst 普查：`/usr/bin/python3 docs/f2-survey/f2v10_gategoms.py`
- 考卷拍紅：`cargo test -p quire-engine --test f2_wordart_projection`（燈1b 紅）

## 6. 鐵律遵事（紅燈階段）

只在 worktree `wt/t_ab254070`；零 `git push`／checkout／reset／rebase／stash。
紅燈 commit 觸碰面＝`core/engine-api/tests/f2_wordart_projection.rs`（新增考卷）、
`core/engine-api/src/lib.rs`（僅新增 `shape_geom_facts()` 佔位讀取器，零發射路徑改動）、
`docs/f2-redlight.md`＋`docs/f2-survey/`（報告與探針）。**發射路徑零改動**＝四組零回退閘門
（roundtrip／fdo76591／WC048／btlr／99_Fields／cell-grid-span／CoverPage／n751077＋無效果檔
SVG 零位元組差異）在紅燈 commit 天然零位移。
