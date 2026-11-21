# E：tblPr 直接底紋發射——LO 真值覆核與考卷極性反記帳（t_f0cbc25d）

## 0. 結論速讀（卡面前提反轉）

卡面（U4 t_e1b68330 引擎探針轉述）把「`SetTableProps.shading_fill` 寫 XML
成功、顯示列表零變化」定性為**我方單邊失效**，契約 2 要求修發射層「tblPr
直接底紋發 Rect，範圍＝整表外框」。

**本卡 LO 25.8（Linux 25.8.7.3，`~/.local/lo-linux`）探針實測推翻此前提**：
LO 對 `w:tblPr` 直接 `w:shd` 底紋**七變體全數零渲染**——我方現行「零發射」
不是缺陷，而是**與 LO 真值一致**。照卡面修發射反而讓偏離 LO 的Rect 進入顯示
列表。考卷極性因此反轉：發射層**必須保持零發射**並鎖死（防「順手修」），
另用正向對照燈證明零發射不是底紋鏈全滅的假綠。

考卷：`core/engine-api/tests/e_tblpr_shd_parity.rs`（四盞燈，正態 4/4 綠；
`E_TBLPR_SHD_BUGGY=1` 燈 1／3 必紅＝鑑別力實證）。

## 1. LO 真值探針矩陣（探針 `scripts/e_probe_lo_tblpr_shd_v1..v5.py`，入庫可重跑）

判讀工具雙軌：`page.get_drawings()` fill 筆數（向量級）＋ 72dpi `pixmap.pixel`
像素掃描（防抽取器假象）。封頂色高飽和 `9900CC`（v4，排除近白色差誤判）與
`FAFBFE`（v1–v3，真檔 bug57031 同色）。

| 變體 | 構造 | LO 結果 |
| --- | --- | --- |
| v1 A | `tblPr` 直接 `shd fill=FAFBFE`，格全不宣告 | 零 fill（無底色drawing） |
| v1 B | 表層 `FAFBFE` ＋ 格(0,0) `FFFF00` | **只見黃色 cell Rect**，FAFBFE 零 |
| v1 C | `shd fill="auto"` | 零（與既有口徑一致） |
| v1 D | `shd val="nil" fill=FAFBFE` | 零 |
| v1 E | 無 shd 對照 | 零 |
| v3 O1 | bug57031 實際元素順序（tblBorders→shd→tblLayout） | 零 fill＋像素零命中 |
| v3 O2 | shd 緊跟 tblW（無 Borders 前置） | 同上 |
| v3 O3 | shd 在 tblLook 之後（schema 尾部違規） | 同上 |
| v3 S5 | `tblCellSpacing=100dxa`（5pt 縫隙）＋表層 shd | 縫隙**不露**表層色：零 fill、像素 FAFBFE±=0 |
| v3 S0 | `tblCellSpacing=0` 對照 | 同上 |
| v4 P1 | `val=clear fill=9900CC`（高飽和封頂） | `get_drawings` fills=0 |
| v4 P2 | `val=solid color=FF0000 fill=9900CC`（圖案底紋） | fills=0、像素高飽和零命中 |
| v4 P3 | autofit（無 tblLayout） | fills=0 |
| v4 P4 | 無 tblBorders | fills=0 |
| v4 P5 | 無 tblW（auto） | fills=0 |
| v4 P6 | **正向對照：cell(0,0)=9900CC** | **渲染：fills=2、像素 1363 筆精確 (153,0,204)** |
| v4 P7 | baseline 無底紋 | fills=0 |
| v5 Q1 | 底紋改經 `tblStyle`→styles.xml 樣式 `tblPr/w:shd` | fills=0（樣式路徑同不渲） |
| v5 Q2 | 樣式＋直接雙宣告 | fills=0 |
| v5 Q3 | 正向對照 cell=9900CC | 渲染 |

像素級覆核（`pixel_check.py`／`pixel_check_v4.py`）：v3 六頁、v4 七頁表區
（x∈[68,328]、y∈[78,150]）逐點掃描，非白像素僅框線抗鋸齒灰階群，探針色
命中數＝0（P6 對照頁除外）。

### 真檔 A/B（bug57031）

- XML 事實（`scripts/e_count_shd_57031.py`）：21 表中 8 表 `tblPr` 直接 `shd fill=FAFBFE`；
  12 格 `tcPr/w:shd fill=FAFBFE`；無 `tblStylePr`；無自閉合 tcPr/tblPr。
- LO 側（`scripts/lo_57031_pixels.py`，96dpi）：全檔 FAFBFE± 像素 80,743——集中於
  12 個 cell 層格子的幾何覆蓋。
- 我方側（`quire-cli dump`＋抽腳本）：顯示列表 FAFBFE Rect 恰 12 筆（全 cell 層），
  總面積 67,310 pt² ＝ 96dpi 換算 119,662 px——量級吻合 LO（LO 側格內文字
  像素蓋印＋邊框內縮使實渲面積略小，本卡不做逐格反事實分解，不宣稱瓜分）。
- 判讀：LO 渲出的 FAFBFE 全部可由 cell 層 12 格解釋；8 張表層底紋表無一
  呈現超出 cell 覆蓋的表層色塊（v1 S5 縫隙實驗另證「被蓋住露色」假說不成立
  ——5pt 縫隙下表層仍零渲染）。

### 環境記帳（誠實標註）

本機僅 Linux LO 25.8.7.3 一套（`/mnt/c` 無 Windows LibreOffice 安裝，
`soffice.exe` 不存在）——跨環境分歧軸（skill「定錨」紀律）本卡**老實缺測**。
K29e 上下標真值同以 Linux LO 25.8 記帳，本卡與之同版同環境，可比。

## 2. 代碼面事實（實讀定位，非卡面行號）

- ooxml 投影：`core/ooxml/src/table.rs:809`（`tblShd`|`shd` under `tblPr` →
  `TableProps.shading`，含 `read_shd` 全欄位）——**投影存在，XML 無損面齊備**。
- 發射鏈消費面：`core/engine-api/src/table_map.rs::table_input`（`fill` 只從
  `c.props.shading`（cell 層）與 `cond_band_fill`（tblStylePr 條件帶）取；
  `TableProps.shading` 全檔零引用）→ `quire_layout::table::CellInput.fill` →
  `CellFrag.fill_argb` → `core/engine-api/src/lib.rs::push_frag_items`（逐格
  Rect）。`TableFrag` 層無 fill 欄位、`TableInput` 無表層 fill 欄位——表層
  底紋在結構上無發射路徑（**非 bug，是與 LO 一致的正確形態**）。
- 寫入面（K29c）：`core/engine-api/src/mutation_k29c.rs::table_props_xml`
  把 `shading_fill` 寫成 `<w:shd w:val="clear" w:color="auto" w:fill="…"/>`
  進 `tblPr`——XML 真寫入、roundtrip 無損，本卡零觸碰。

## 3. 考卷四盞燈（`e_tblpr_shd_parity.rs`）

| 燈 | 斷言 | BUGGY 態（`E_TBLPR_SHD_BUGGY=1`） |
| --- | --- | --- |
| 1 單邊失效鎖 | `SetTableProps.shading_fill`→XML 含 `<w:shd …/>` 且落在 tblPr 內；顯示列表逐項簽名**逐位不動**；探針色 Rect＝0；存檔重開 shd 仍在（無損） | 期望轉為「應進顯示列表」→必紅 |
| 2 cell 正向對照 | 同表 `SetCellProps.shading_fill` 恰多一筆 Rect（零發射≠底紋鏈全滅） | 無 BUGGY 態（錨點燈） |
| 3 真檔奇偶校驗 | bug57031：FAFBFE Rect 總數＝XML 掃描 cell 層有效宣告（12），**不含**表層 8 表貢獻（發射偷跑態為 20） | 期望改 20→必紅（實測 `left:12 right:20`） |
| 4 投影零消費＋roundtrip | ooxml parse 成功、part 明文保留 tblPr 底紋；bug57031 開檔→存檔 byte-identical；Rect 數＝cell 層數 | — |

燈 3 期望值由考卷內 XML 掃描**獨立推導**（非寫死常數），語料再生成也不假綠。

實跑（本 worktree，2026-11-21）：

```
$ cargo test -p quire-engine --test e_tblpr_shd_parity
test result: ok. 4 passed; 0 failed

$ E_TBLPR_SHD_BUGGY=1 cargo test -p quire-engine --test e_tblpr_shd_parity
test result: FAILED. 2 passed; 2 failed
  ├ lamp1: BUGGY 態「顯示列表應變」未變 → 紅
  └ lamp3: BUGGY 態期望 20 實得 12 → 紅
```

## 4. 給 PM 的下一步建議

1. **`table:shading` 鈕維持降標，不轉 live**（原契約 4 的前提已死）：引擎
   寫入 XML 是真實的（供 Word 側看），但 LO/我方一致零渲染，掛 live 會向
   用戶宣稱不存在的可見效果。若要給用戶「整表底色」能力，正確路徑是 UI 層
   逐格發 `SetCellProps`（LO 真值支援、發射鏈現成）——屬外殼車決策。
2. U4 報告 §1.2／§5.2 的「表格層底紋可見性缺口」記帳應由本檔取代：缺口
   不存在，屬 LO 真值。若 PM 要回填 U4 報告，引本檔 §1 探針矩陣。
3. 若未來接 Word 相容渲染（非 LO 真值板）需表層底紋，先重跑本檔探針矩陣
   另立真值板，勿直接在本發射層加臂。

## 5. 白名單與紀律自證

- 觸碰：`core/engine-api/tests/e_tblpr_shd_parity.rs`（新增考卷）＋本 docs。
  發射層檔案**零修改**（實讀定位見 §2——結論是無需修）。不碰 `shells/`。
- 未做（老實列）：Windows LO 跨環境覆核（本機無安裝）；render_lab 全板
  A/B（本卡零發射改動，零回退面＝全閘 gate 覆蓋，未另跑 corpus 差分）。
- 無 push／checkout／reset／rebase／stash。
