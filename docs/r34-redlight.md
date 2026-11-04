# R34 紅燈：表內無填充框根殘留幽靈矩形與灰色佔位框線

測試檔：`core/engine-api/tests/r34_placeholder_fill.rs`（新增）＋
`core/engine-api/tests/r32_wc048_table.rs`（R34 重寫版）
標的：`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` p0
LO 二進位：`~/.local/lo-linux/opt/libreoffice25.8/program/soffice`
抽真值：pymupdf（`/usr/bin/python3`）

## PM 補正（撞面裁決）

與 D5 卡（t_ed8bbd01，agy 外部執行，正改 `core/ooxml/src/{lib,drawing,styles}.rs`
的 tab 採集與 run 樣式級聯）撞面：本卡**禁碰 `core/ooxml/`**。S1（框內 `w:tab`
停靠點定義誤轉偽造空格 → txbx 樣式配對全滅）與 `pending_txbx_drawing` 採集面
缺口全數移交 D5／後續卡。本卡只做：

1. `core/engine-api/src/lib.rs` `emit_image_shape` 填充／邊框段（幽靈 Rect＋
   佔位框線）。
2. `shells/native/src/render_svg.rs` alpha-0 Rect 跳過輸出。
3. r32 考卷重寫（修前必紅）＋ r34 新增紅燈；fdo76591 樣式燈寫進 r34 但標
   `#[ignore]`，待 D5 合併後由 PM 解鎖。

## 現行碼實拍（接手起點＝修前紅燈，基線 main@9d0407e 未動碼）

```text
$ ./target/debug/quire-cli page corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx 0
RECT0 0x0 160.6 93.6 273.6 110.5      ← Textbox2 根幽靈
RECT0 0x0 72.0 158.5 387.0 185.0      ← Textbox3 根幽靈
RULE 72.0 158.5 459.0 158.5           ← 0xFFCCCCCC 0.75pt 佔位框線 ×4
RULE 72.0 343.5 459.0 343.5
RULE 72.0 158.5 72.0 343.5
RULE 459.0 158.5 459.0 343.5
ccount CCCCCC-rules-in-box=4 alpha0-rects=2
```

S1 側（歸 D5，本卡只留證）：fdo76591 p0 `IMPORTANT NOTICE` sz=12 flags=0
fam=None、`      Scheduled Saturday Overtime`（6 個偽造前導空格）。

## LO 真值（pymupdf 實測）

| 項目 | LO 實測 (pt) |
|---|---|
| Textbox2 根（273.6×110.55） | 無底色、無框線；只有藍線 y99.2／132.9 |
| Textbox3 根（387×185） | 無底色、無框線；子件白框 (72.25,159.4)-(459.25,257.9)＋藍塊 (80.95,342.1)-(362.35,344.4) |
| n779834 EMF 浮水印 | 照舊灰底佔位＋淺灰四邊（誠實近似，不得誤殺） |

## 修法

- `emit_image_shape`（`core/engine-api/src/lib.rs`）：
  - `textbox_root_nofill`（表內文字框／群組根＋填色未宣告）成立時 `fill=None`
    ——不 push Rect（R32 的 alpha-0 幽靈口徑作廢；SVG 透明 rect 仍 hit-testable）。
  - 同條件＋`stroke_argb` 缺位＋`stroked != Some(true)`：群組子件發射**之後**、
    根框四邊之前 `return`——不發 `IMAGE_PLACEHOLDER_EDGE` 四條佔位 Rule。
  - 回塞鎖 `R34_PLACEHOLDER_GATE_BUGGY=1` 還原 R32 舊口徑（照 K12/K17 先例）。
- `render_svg.rs`：`(fill_argb >> 24) == 0` 的 Rect 一律不輸出（兜底閂，
  單元鎖測直接構造顯示列表）。
- k15／k17 探針換位：根框 Rect 消失後，`textbox3_origin` 踩同原點子件白框
  （387×98.5，dx/dy=0 逐位元組同值）、`textbox2_origin` 踩藍線反推（內縮
  10.925/5.6 由 LO 真值推得）。**斷言 LO 真值逐字不動**；BUGGY 回退態下子件
  跟根框整體位移，鑑別力等價（實測：K15 BUGGY 態白框飛 (208.3,21.0)、
  K17 BUGGY 態 y=106.5，與舊探針一致）。

## 修後實拍（R34 分支 68eb8aa＋合併 main 8202d13）

```text
$ cargo test -p quire-engine --test r32_wc048_table --test r34_placeholder_fill
test result: ok. 4 passed（r32）；3 passed 1 ignored（r34，fdo76591 燈待 D5）

$ R34_PLACEHOLDER_GATE_BUGGY=1 cargo test -p quire-engine --test r32_wc048_table --test r34_placeholder_fill
test textbox_roots_have_zero_ink ... FAILED        ← 修前必紅坐實
test test_text_total_matches_lo_truth ... FAILED   ← 幽靈吞噬斷言紅
test wc048_no_ghost_rect_and_no_placeholder_rules ... FAILED
```

## 收尾閘門

- `bash ~/.hermes/scripts/ow-gate.sh .`（合併 main 後的 merged tree）：**八閘
  PASS、GATE_SUMMARY fail=0**（fmt／clippy -D／workspace test／layering／
  roundtrip fixtures／roundtrip corpus 399／wasm32 check／goldens）。
- render_lab WC048 p0：內容 **1.0**（閘門 ≥0.688，R33 已把排序面收斂）、
  版面 1.0、色 0.922、真缺字 0。
- `ow-style-ab.py`（BASE=a6dadee vs 本分支 merged tree，全 399 檔）：
  `regressed_files=1 regressed_items=7`＝fdo76591 的 7 條，**與 R31/R32 審查
  時的基線回退逐條相同、零新增**（S1 歸 D5，解 ignore 由 PM 在 D5 合併後執行）。
- `quire-cli page` WC048：textbox 區 0 條 `0xFFCCCCCC`、全頁 0 個 alpha-0 Rect。
