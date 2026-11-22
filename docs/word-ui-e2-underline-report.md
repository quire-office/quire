# E-2 交付報告：underline 保形態——`w:u/@w:val` 解析枚舉＋發射多態

分支：`wt/t_b5a4d0bf`（基線 main，已 `git merge main` 取合併態；未 push）
署名：ADT109119。繁中註解，禁簡體禁 emoji。

---

## 1. 缺陷與根因（卡面 U4b §4／§8 PM 決策點 3）

`core/ooxml/src/styles.rs` 的 `RunStyle::underline` 原為 `Option<bool>`：布林只分
有無，`w:val="double"/"thick"/"dotted"/"dashed"` 等形態在解析當下一律折成
`Some(true)`，發射端（engine 烘焙 → `DrawItem::Text.flags` 的 `FLAG_UNDERLINE`
位元 → SVG `text-decoration="underline"`）只能畫單線。屬**保形態缺口**，非寫入
缺口（寫入端 `RunDirectPatch::underline` 早已是 K29e `UnderlineKind` 多態）。

## 2. 架構決策（run 1 選定、PM 令沿用；run 2 未另起爐竈）

- **單一真值源**：`Underline` 枚舉落 `core/render-trait/src/lib.rs`（發射契約層），
  變體 `Off / Single / Double / Thick / Dotted / Dashed / Raw(String)`。
  解析層級聯（`RunStyle::underline`）與發射層（`DrawItem::Text::underline`）
  共用同型 → 讀取端與繪製端零口徑漂移。
- **三態語義向 `bold`/`italic` 靠攏**（PM 補正 2）：外層 `Option` 表「未宣告」，
  `Underline::Off` 表明確關閉且**能熄滅樣式層宣告**（考卷燈 7 鎖死：樣式層
  `double` ＋ 直接格式 `w:u val="none"` → 有效值 off，旗標熄滅）。
- **依賴邊**：`core/ooxml`、`core/layout` 新增 `quire-render-trait` 依賴（兩處
  Cargo.toml 皆寫了 check-layering 合法性論證；render-trait 零 core 依賴，
  `scripts/check-layering.sh` 只禁 core→shells，該閘綠）。
- **serde 線契約**：`RunStyle.underline` 序列化為 `w:val` 小寫字面（`"double"`），
  `Raw` 走外部標籤 `{"raw":"wavy"}`；`DrawItem::Text.underline` 與所有派生欄
  一律 `default + skip_serializing_if = "Option::is_none"` → 未宣告態不外露，
  顯示列表 JSON 零位移（發射考卷燈 4 鎖死）。
- `dash` 在解析層歸一為 `Dashed`（Word 相容別名；corpus phpword 四檔實見）。
- 未支援形態（`wavy`／`words`／`dottedHeavy`…）走 `Raw` **保形**，不折損丟形。

## 3. 發射層實際落點（起手讀碼坐實，非推測）

`grep -rn underline core/layout core/render*` 結果：發射端不在 `core/layout`
（layout 只承載 `CellRun` 標注），真正的多態繪製落在**渲染殼** `shells/native/src/
render_svg.rs`（`page_to_svg`，與 `quire-cli render` 同一支發射器）。透傳鏈：

| 層 | 檔案 | 落點 |
| --- | --- | --- |
| 級聯烘焙 | `core/engine-api/src/lib.rs` `cascade_paragraphs` | `Run.effective_underline`（純派生欄；off→None；修訂插入線→None 交旗標兜底） |
| 同質段合併 | `core/engine-api/src/lib.rs` `LineSegment`／`emit_line_run_items` | 形態入**合併鍵**（防 double/single 併段吞形） |
| 表格 | `core/layout/src/table.rs` `CellRun` ＋ `core/engine-api/src/table_map.rs` `cell_run_spans` | 形態透傳＋入 span 合併鍵 |
| 註腳尾註 | `core/engine-api/src/footnotes.rs` `EntryStyle`／`NoteRun` | `underline_kind` 透傳 |
| 自由文字形／豎排 | `core/engine-api/src/lib.rs` `emit_txbx_texts` | 首 run 形態透傳（豎排不發幾何，見 §6 未做面） |
| 繪製 | `shells/native/src/render_svg.rs` `uline_geom`／`emit_underline_paths` | 幾何 `<path>` |
| ODF | `core/odf/src/display.rs` | `underline: None`（ODF 端形態未採集，同 `family` 紀律） |
| 行量測工具 | `tools/line-metrics/src/doc_inputs.rs` | 形態降有無旗標（純量測，bool 口徑與修前逐走相同） |

SVG 繪製配方（座標全來自 layout 的骨架級幾何直譯）：

- `double` → 兩條平行 `<path>`（SVG 1.1 `text-decoration` 對重複值渲染未定義，
  瀏覽器只畫一條，宣告式多值不可依賴）。
- `thick` → 單條 `<path>`，`stroke-width ≈ 字級×0.13`（≈單線 2.4 倍）。
- `dotted` → `<path>` ＋ `stroke-dasharray` 點級段長（<3pt，段長＝筆寬）。
- `dashed` → `<path>` ＋ `stroke-dasharray` 段級段長（≥3pt）。
- `single`／未宣告／`Raw(_)` → 舊 `text-decoration="underline"` 路徑**逐位元組
  保持**（零位移鐵律；`Raw`＝降級單線＋源碼 TODO，禁 panic，原字面保留在
  `Underline::Raw`，待繪製配方落地直接接上 `uline_geom`）。

## 4. 先紅後綠實錄

### 4a. 紅燈口徑（本 run 的機械證明＝BUGGY 回塞鎖）

run 1 的修前紅燈已由 PM 以 `refs/wip/t_b5a4d0bf-run1`（`a82599d`）保護，run 2
工作基準＝run 1 現態（改動已在樹上）。本 run 的**可重跑鑑別**採本庫既有
env 回塞鎖慣例（k21_soft_br／g1_anchor_relativity 同款，非 k29e 的字串置換），
env 名稱 grep 現庫自取：`E2_UNDERLINE_BUGGY`，兩處生產碼各內建開關——

- `styles.rs::underline_of`：置位時把判讀折回修前布林語義（任何有底線值→Single）；
- `render_svg.rs::uline_geom`：置位時恆回 `None`（全形態退回 `text-decoration`
  單線舊路徑）。

實測（`bash scripts/e2_underline_buggy_replay.sh buggy`）：

```
確認（buggy）：e2_underline_kind 必紅（5 盞失敗燈）
確認（buggy）：e2_underline_emit 必紅（4 盞失敗燈）
BUGGY 態回塞鑑別通過
```

解析層紅燈樣本（回塞態實得）：

```
---- e2_lamp1_double_val_preserves_kind ----
assertion `left == right` failed: <w:u w:val="double"/> 的形態必須保形為 "double"（修前布林折損為 true）
  left: String("single")
 right: String("double")
---- e2_lamp5_unsupported_kinds_kept_as_raw ----
assertion `left == right` failed: 未支援形態必須以 raw 保形（發射端降級單線＋TODO）
  left: String("single")
 right: Object {"raw": String("wavy")}
failures: e2_lamp1… e2_lamp2… e2_lamp3…（dash 歸一） e2_lamp5… e2_lamp7…（樣式層 double 保形）
test result: FAILED. 4 passed; 5 failed
```

### 4b. 修後轉綠（合併態實測）

```
$ cargo test -p quire-ooxml --test e2_underline_kind
test result: ok. 9 passed; 0 failed; 0 ignored

$ cargo test -p quire-engine --test e2_underline_emit
test result: ok. 14 passed; 0 failed; 0 ignored   （含 render_svg 單元測）

$ bash scripts/e2_underline_buggy_replay.sh good
確認（good）：e2_underline_kind 必綠
確認（good）：e2_underline_emit 必綠
GOOD 態通過（env 鎖零改檔，工作區乾淨）
```

### 4c. 既有考卷過期前提重估（可收窄不可弱化，逐條記帳）

`cargo test --workspace` 首跑出現 2 盞 **k29e_char_format** 紅燈，根因非缺陷，
而是既有斷言把「底線證據」寫死成 `text-decoration` 子串——E-2 後 `dotted`
改發幾何 `<path>`，底線仍在、畫得更對，卻因換画法假紅：

- `k29e_lamp1`（`Dotted` 寫入段）：改吃**雙路徑析取**（宣告式子串或緊接
  `<path>`）；`line-through`／`font-family` 斷言原樣保留。
- `k29e_lamp6`（顯式移除段）：斷言**更嚴**——宣告式子串與幾何 path 兩者皆須
  為無（只查子串會漏放行）；樣式級段不被連坐的斷言同步吃雙路徑。

修後 `cargo test -p quire-engine --test k29e_char_format` → 15/15 綠。

## 5. corpus 實掃（卡面任務 1；本 run 重掃，口徑並陳）

分母＝corpus 全 400 個 `.docx`；方式＝`unzip -p` 解明文後 grep `w:u w:val=`。

| 口徑 | 命中檔數 |
| --- | --- |
| 卡面口徑（只 `word/document.xml`） | **44 / 400** |
| 全 part 口徑（`word/*.xml`，含 styles／footnotes／endnotes） | **160 / 400** |

值分佈（全 `word/*.xml`）：`single`×1286、`none`×131、`thick`×5（FDO77715
styles.xml）、`dash`×5（phpword 四檔 document.xml）、`words`×1（fdo61343
styles.xml）、`double`×1（fdo61343 styles.xml）、`dotted`×1（xopt
SolarOverview document.xml）。僅掃 document.xml：`single`×909、`none`×8、
`dash`×5、`dotted`×1（thick／double／words 全在 styles.xml）。

**定性**：非 single 形態在 corpus **有真實命中**（7 檔帶非 single/none 值），
本卡屬實測缺口修復，非零命中的純規格正確性。`dash` 為 `dashed` 的 Word 相容
別名（phpword 產出），解析層歸一後發射端才認得——這是「同義兩制的規格正確性」
在 corpus 裡的實際壓力點。

## 6. 未做面（老實記帳，非藏缺）

1. **共型未做**：寫入端 `UnderlineKind` 定義在 `core/engine-api/src/mutation.rs`
   ——本卡白名單明令勿碰（E-1／活體卡 `t_c2bbcb38` territory），且它是 `pub`
   線契約（外殼已消費其 serde 形）。故卡面任務 4 改以**語義等價閉環**落地：
   發射考卷燈 6 逐值（single/double/dotted/dashed/none↔Off）走
   「寫入→存檔→重解析→發射」斷言形態一致，另斷未編輯件的
   `word/document.xml` 逐位元組保持。真正的共型需同時動 mutation.rs 與外殼
   序列化，屬另卡。
2. **`wavy` 等 ECMA 全集其餘形態只保形未繪製**：`Raw` 降級單線＋源碼 TODO
   （考卷燈 3 鎖「不炸＋照畫單線」），待繪製配方落地。
3. **豎排（vert／vert270／eaVert）不發幾何底線**：底線方向需隨行向旋轉，屬
   另卡範圍，維持修前 `text-decoration` 行為（源碼註解坐實）。
4. **ODF 端形態未採集**：`style:text-underline-style` 未讀，同 `family` 紀律
   走 `None`（旗標兜底單線），未虛構。
5. **外殼（shells/web/editor）未消費形態欄**：`DrawItem::Text.underline` 已上
   線但 `format:underline-style` 仍受 engine-adapter.js 既有誠實斷點約束——
   本卡白名單禁碰 `shells/web/editor/**`，轉 live 屬外殼卡。

## 7. 驗收

`bash ~/.hermes/scripts/ow-gate.sh <worktree>`（合併態）：**GATE_SUMMARY fail=0**

| 閘 | 結果 |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS |
| `bash scripts/check-layering.sh` | PASS |
| `cargo run -p quire-cli -- roundtrip fixtures --check` | PASS |
| `cargo run -p quire-cli -- roundtrip corpus --check` | PASS（399/399 byte-identical） |
| `cargo check --target wasm32-unknown-unknown`（7 crate） | PASS |
| `cargo run -p quire-layout --bin goldens` | PASS |

零回退佐證：讀取端形態欄純派生（`serde default + skip None`），corpus
roundtrip byte-identical 全綠；顯示列表 JSON 未宣告態零位移（燈 4）。
繁中：本卡改動面 866 詞經 opencc s2t 對照，白名單外洩漏歸零。

E2_DONE
