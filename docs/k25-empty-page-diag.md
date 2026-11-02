# K25 診斷：n751077 p2「多出空頁」＝前提不成立（引擎已與 LO 同形；0.0 是度量偽影）

> 卡面號稱：LO 2 頁有內容、我方第 2 頁渲染為空頁（該頁內容分 0.000，
> `empty_page_mismatch`）。本卡先紅複測的直接量測推翻三處前提，逐條如下。
> 本檔零程式碼觸碰（只新增本 docs/ 檔），八閘 `GATE_SUMMARY fail=0`（髒樹態）。

## 0. 結論（三句話）

1. **頁數已一致**：我方 `quire-cli info` ＝ 2 頁，LO ＝ 2 頁；pagebreaks 逐頁
   尺寸同 612×792。K5-diag §2.1 那條「我方 3 頁」已不存在。
2. **第 2 頁兩邊都是物理性空白**：LO 該頁內容流只有 `/Artifact` 裁剪矩形，
   **零 BT/ET、零 Do**；72dpi 光柵**非白像素 0**。我方該頁同樣 0 非白像素。
   → 「LO 有內容、我方空」不成立；「每頁 text 數>0」這條燈在 LO 真值上
   本身就不可能成立（LO 該頁 `lines=[]`）。
3. **剩下的 0.0 是 `render_lab.py` 的度量缺陷**：`pdf_page_facts()` 把
   **宣告**在共用 `/Resources` 的圖片（未繪製）當成該頁內容，餓死了既有的
   4b「同位雙空＝度量假分差」臂。修正點在 `scripts/`，本卡白名單禁碰。

## 1. 實測記錄（可重跑，單位＝PDF pt，原點左上）

### 1.1 頁數與每頁事實

| 頁 | LO（pymupdf） | 我方（quire-cli / pagebreaks.json） |
|---|---|---|
| 0 | 6 行 `TEXT1`…`TEXT6TEXT7TEXT8TEXT9`；images(宣告)=1；**繪製** rect=[17.25, 59.65, 83.85, 118.5]；非白像素 7617 | 9 項顯示列表：1 Image(66.6×58.85 @17.2,59.7)＋TEXT1-9 全在；非白像素 13477 |
| 1 | `lines=[]`、`drawings=0`、`images(宣告)=1`、**繪製 rect=[]**、**非白像素 0** | 4 個零字形 Text（Title/Subtitle×2/BodyText 空段）；**非白像素 0** |

`pagebreaks.json`（我方）：p0 `first_para 0–1`、p1 `first_para 2–5, starts_section=true`——
正是 para-level `sectPr`（源檔 `w:sectPr`×2）照實開第 2 頁，LO 同形。

### 1.2 「宣告 ≠ 繪製」的鐵證（本卡核心）

LO 第 2 頁（索引 1）**完整**內容流，逐字元：

```
0.1 w
/Artifact BMC
q 0 0.028 611.971 791.971 re
W* n
EMC
Q 
```

- 零 `BT…ET`（無任何文字繪製）、零 `Do`（無任何圖片繪製）。
- 兩頁的 `/Resources` 是**同一個物件 41**（`d.xref_get_key(page,'Resources')`
  兩頁同回 `('xref','41 0 R')`）→ pymupdf `get_images()` 對第 2 頁也回報
  xref 6（`Im6`），但 `get_image_rects(6)` 回 `[]`＝該頁沒有把它畫出來。
- 光柵覆核：第 2 頁 72dpi 光柵非白像素 **0**（不靠任何圖庫語意的獨立覆核）。

### 1.3 餓死 4b 臂的機轉（逐行對照 `scripts/render_lab.py`）

`compare()` 雙空頁路徑（第 329–372 行）：

| 判別式 | n751077 p2 實值 | 結果 |
|---|---|---|
| `drawings or unique_images`（LO 有圖形⇒None） | 0 or 0（xref 6 在兩頁皆宣告⇒`xref_pages[6]==2`，非 unique） | 不進 |
| `i<len(lo) and i<len(ow) and images==0`（同位雙空⇒None，4b） | images **=1**（宣告值） | **不進←就是這裡漏** |
| else ⇒ `cs=ls=0.0`＋`empty_page_mismatch`（4b2/4d 防放水臂） | — | **命中（誤判）** |

把 `images`/`image_xrefs` 換成「繪製」口徑（`get_image_rects` 非空才算）後，
**不改 `compare()` 一行**，同檔同我方事實跑出的結果（實跑輸出）：

```json
{"lo_pages":2,"ow_pages":2,"page_count_match":true,
 "content_scores":[1.0,null],"layout_scores":[1.0,null],"first_divergence":null,
 "notes":["measurement_artifact:雙空同位頁(LO真空)",
          "源檔 w:sectPr×2（雙方照實開頁⇒真空為結構性）"],
 "measurement_artifacts":[{"page":1,"drawings":0,"images":0,
                           "reason":"both_empty_same_position"}]}
```

→ 該檔內容分 **0.500 → 1.000**（None 不入平均）、`empty_page_mismatch` 標籤消失、
頁數一致。這正是 K25 驗收欄要的「內容分由 0.5→上升、頁數一致」，而它**只能**
從度量層拿到，無法從 `core/layout` 拿到。

### 1.4 4d 防放水臂必須保持原樣（不可繞道＝本卡不自救的理由）

4d 的語意是「LO 該頁**繪製**了逐頁重複的頁首圖、我方無 ⇒ 我方少真內容，仍 0.0」。
n751077 p2 是「**宣告**了但**沒繪製**」——與 4d 保護的對象是兩件事，但共用同一個
`images` 欄位。若在 `pdf_page_facts()` 改成繪製口徑，4d 的合成案例必須同步證明
「真繪製的重複頁首圖仍 0.0」仍綠（現有 4d fixture 是手寫 dict，不會自動抓到回歸）。
⇒ 這是**度量合約變更**，需 PM 裁定，不該由一張白名單寫著 `core/layout` 的引擎卡順手做掉。

## 2. 真正的 n751077 引擎殘留（本卡白名單外，另立卡）

p0 的釘定 `v:group` 子件 y 座標全塌在框頂，與 LO 差最大 456pt（LO 依
`coordorigin=",1440"`／`coordsize="12239,12960"`／各子 rect 的 `top` 分佈）：

| 標籤 | LO 基線 y（top-origin＝792−PDF y） | 我方 SVG y | Δ |
|---|---:|---:|---:|
| TEXT1（16pt） | 144.45 | 169.4 | +25.0 |
| TEXT3（36pt） | 377.90 | 194.4 | −183.5 |
| TEXT4（36pt） | 419.35 | 195.7 | −223.7 |
| TEXT5（20pt） | 473.45 | 183.5 | −290.0 |
| TEXT6TEXT7TEXT8TEXT9（16pt） | 506.55 | 182.0 | −324.6 |
| TEXT2（48pt，x=394.5） | 660.35 | 203.9 | **−456.5** |

x 軸：LO TEXT1 x=70.9、TEXT2 x=394.5；我方全部 93.0。
度量側看不見：p0 內容/版面軸皆 1.0（詞序同）、色軸 1.0、圖軸 None——**現行
五軸對「同字重疊」完全失明**，這 456pt 位移是儀表板的盲區（非本卡範圍）。

## 3. 驗證鏈（本卡實際跑過的）

| 項 | 結果 |
|---|---|
| `bash ~/.hermes/scripts/ow-gate.sh .`（髒樹僅 docs/ 新增） | **八閘 PASS、GATE_SUMMARY fail=0**（fmt／clippy -D／workspace test／layering／roundtrip fixtures／roundtrip corpus 399／wasm check／goldens） |
| `render_lab.py --selftest` | **ALL-PASS**（含 4b／4b2／4c／4d 四臂原樣綠） |
| `render_lab.py corpus/ooxmlimport/n751077.docx`（單檔） | LO 2 頁 / OW 2 頁（頁數 ✓）、內容 `[1.0, 0.0]`、色 `[1.0, 1.0]`、圖 `[1.0, —]`、真缺字 0 |
| `quire-cli info n751077.docx` | 頁數 2、段落 6 |
| `cargo run -- page n751077.docx 1` | 顯示列表：4 個零字形 Text（零墨跡，與 LO 同） |
| 回歸鎖（pagination goldens） | `cargo run -p quire-layout --bin goldens` PASS——`uniform25`／`mixed_overflow`／`narrow_page`／`single_empty` 四例零變動（本卡未碰引擎，無回退面） |
| 新增引擎側鎖 `core/engine-api/tests/k25_empty_page_parity.rs` | **5/5 綠**（燈 A 頁數＝2／燈 B p0 六標籤全收＋圖片幾何 ∈ LO ±1pt／燈 C p1 零墨跡＝LO 真空同形／燈 D p1 `starts_section=true` 結構頁＋段落範圍 0–1 \| 2–5／自查鎖） |
| 鎖的鑑別力（變異探針，暫時檔跑完即刪） | **5/5 必紅**：期望頁數改 3→紅、要求 p1 有墨跡→紅、要求 p0 含 TEXT10→紅、圖片 x 期望 100（實測 17.25）→紅、`starts_section` 期望 false→紅。⇒ 非假綠 |
| `git status --porcelain -- scripts corpus out` | **空輸出**（禁區零觸碰）；本卡僅新增 2 檔（`docs/`＋`core/engine-api/tests/`，皆白名單內） |
| K7/K7-cal/K7b 收斂成果（fdo76591 等） | 零回退（未碰分頁/容量碼路徑） |

探針腳本（唯讀，import `render_lab` 不改檔）：scratch `k25_probe2/4/5/6/7.py`、
`k25_dump.py`、`k25_blast.py`。

## 4. 全庫衝擊面（實掃，非推估）

`out/lo-scoreboard/scores.jsonl`＋`scores.prev.jsonl` 帶 `empty_page_mismatch`
標籤的檔案＝**僅 n751077 一檔（兩份皆同）**。⇒ 度量若改繪製口徑，本輪已知
受影響面就這一個檔案、一個頁面；但 4d/4c 語意必須另補合成案例鎖。

## 5. 下游（本卡只點名，不執行）

1. **度量卡（PM 裁定）**：`scripts/render_lab.py::pdf_page_facts()` 圖片軸改用
   「繪製」口徑（`page.get_image_rects(xref)` 非空才算該頁內容），使 n751077 p2
   落入既有 4b「同位雙空＝None」臂；紅燈先行＝n751077 p2 由 0.0→None（檔分
   0.5→1.0）＋4d/4c 必仍綠（4d fixture 需新增「LO 真繪製的重複頁首圖」變體）。
   白名單：`scripts/render_lab.py`、`scripts/tests/`、`docs/`。
2. **幾何卡（ow-render）**：n751077 釘定 `v:group` 子件 coord→extent 映射＋框內
   `v-text-anchor` 基線（§2 表為 LO 真值，容差建議 ±2pt）；紅燈＝TEXT2 基線
   y∈660.4±2、x∈394.5±2（現況 203.9/93.0 必紅）。白名單：`core/ooxml/src/drawing.rs`、
   `core/engine-api/src/lib.rs`（textbox 一路）、`core/engine-api/tests/`。
3. **度量盲區（順帶記帳）**：五軸對「同頁同字重疊」（詞序同、位置全錯）給 1.0——
   建議版面軸加 y 直方圖項，否則 §2 這類誤位永遠不出現於最差榜。

## 6. K25-B 收案（2026-11-03，ow-render）：逐框排版落地，§2 表全數命中 ±2pt

### 6.1 根因（實錘）
發射層只認「宿主框」一份 `txbx_rect`（K4c 映射後的最後一個子件矩形），三個子
`v:rect` 的 `top`（1440/11160/2294 coord 單位）在映射後被丟棄，框內 12 段文字全
掛宿主框頂（y≈153–166）。解析層 WIP（7ebde51）已把子件矩形＋coord_top 帶進
`InlineImage.vml_group_children`，本卡補的是發射層逐框排版。

### 6.2 模型（LO 六組資料集擬合，最差殘差 0.202pt；scratch `model_final.py`）
- **組框映射**：`CoordFrame` 線性映射 coord→extent（既有 `map`），宿主框按
  `mso-position-*-center/relative:page|margin` 釘頁心；子件 y ＝組頂＋top 映射。
- **框內堆疊**：每子件独立起點＝框頂＋邊距；逐段 pitch＝`size × face_line_factor`
  （K6 auto-pitch 同口徑）、基線 ALP＝`face_baseline_ratio × line_factor × size`
  （hhea 實測 asc/(asc+|desc|)，缺 face 退 Liberation Serif 實測值，零虛構）。
- **空段**：中段的空段吃 docDefaults 字級；**只有最後一段**空段繼承最後有字段
  字級（LO quirk，V5 實測 4.6pt 漂移的元凶）。
- **右/居中對齊位移**：全大寫串的量寬走 K7 `A_upper` 軌（`face_advance_scale_caps`，
  **取代**混合樣本折中尺、不疊乘）。TEXT2 48pt：LO advance＝541.10−394.5＝146.60
  ＝Liberation Serif 實測 146.62；折中尺 133.98 會把整串推向邊界外側 12.6pt。

### 6.3 驗收實據（显示列表 top-origin 基線，`cargo run -q -p quire-cli -- dump`）
| 標籤 | LO (x, y) | 我方 (x, y) | Δx | Δy |
|---|---|---|---:|---:|
| TEXT1（16） | (70.90, 144.45) | (70.90, 144.33) | −0.00 | −0.12 |
| TEXT2（48） | (394.50, 660.35) | (395.47, 659.94) | +0.97 | −0.41 |
| TEXT3（36） | (70.90, 377.90) | (70.90, 377.75) | −0.00 | −0.15 |
| TEXT4（36） | (70.90, 419.35) | (70.90, 419.15) | −0.00 | −0.20 |
| TEXT5（20） | (70.90, 473.45) | (70.90, 473.34) | −0.00 | −0.11 |
| TEXT6-9（16） | (70.90, 506.55) | (70.90, 506.43) | −0.00 | −0.12 |

### 6.4 閘門與零擾動
- 紅燈鎖 `core/engine-api/tests/k25b_group_child_stack.rs`（先紅後綠；燈3 回塞
  `K25_B_CHILD_STACK_BUGGY=1` 必紅鑑別力）。
- `k25_empty_page_parity` 5 綠零回退；全 `cargo test -p quire-engine` 37 binaries
  287 綠；ow-gate 八閘 `GATE_SUMMARY fail=0`。
- 全庫 A/B（400 docx，同 binary BUGGY 開關前後 dump 逐項比對）：擾動恰落
  `corpus/ooxmlimport/n751077.docx` 一檔（6 項）——其餘 399 檔逐位不變。
