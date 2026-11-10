# D9b：滿版外框／頁腳圖座標硬編碼資料化——紅燈報告＋卡面前提勘誤

卡：`t_08b86cba`（返修 D9 合併 `cab2847` 的字面量落地）。分支基線 `9768949`，
半成品 `1fc10cf`，未 commit 微調收尾 `e1e894c`，merge main（F1/E2c 落地態）`1d13b6e`。

本檔只記**實測**與由此推出的結論；每條都附可重跑的探針（`scratch_d9b/`，本機不入库）。

---

## 1. 第 1 條（inset 資料化）：落地態與考卷燈號

規則（源檔資料驅動，非檔案常量）：發射像素若是宿主框 `w:txbxContent` 的內嵌
`w:drawing`（K27-B `txbx_drawing_extent` 形態），排版原點＝**框原點＋宿主框宣告的
`v:textbox/@inset`**；源檔未宣告時退 VML 格式預設 7.2/3.6pt
（`VML_TEXTBOX_DEFAULT_INSET`，ECMA/MS 預設 0.1in／0.05in，`core/ooxml/src/drawing.rs`
現行註解同口徑）。

資料路徑（inset 不再在排版層丟失）：
`core/ooxml/src/drawing.rs`（既有 `inset` 解析）→ `core/engine-api/src/table_map.rs`
`build_images`（搬 `txbx_inset` ＋ 由 `txbx_drawing_extent.is_some()` 立 `txbx_content_pic`）
→ `core/layout/src/lib.rs` `ImageBand`／`PageFloatingImage`（`#[serde(default)]` 向後相容）
→ `core/engine-api/src/lib.rs` 發射層（套 inset，取代 D9 的字面量 `(7.25, 3.60)`
與 fdo76591 專屬座標比對）。

考卷 `core/engine-api/tests/d9b_txbx_inset_data_driven.rs`（6 盞，1 盞掛 ignore）：

| 燈 | 物件 | 實測 |
| --- | --- | --- |
| 1 | fdo76591 `_x0000_s1039` 框內圖 | 綠（偏 LO 0.75pt；`D9B_INSET_BUGGY=1` 回塞態偏 7.95pt ⇒ 鑑別力成立） |
| 2 | fdo76591 頁腳 logo（D9 主證零回退） | 綠（頂緣 732.80pt ±2.0，且仍繪於 4pt 框底線之上；`D9B_INSET_BUGGY=1` 回塞態 729.20pt 必紅） |
| 3 | n792778 群組框（宣告 `inset="0,0,0,0"`） | 綠（欠債一收案後我方**兩張**皆發，首張偏 LO 0.05pt，見 §3b；收案前該燈吃的是宿主單發那張、偏 2.70pt） |
| 3b | n792778 群組第二張圖 | **本卡收案轉綠**（原 `#[ignore]` 欠債盞；`v:group` 子件框內圖逐框發射，見 §3b） |
| 4 | tdf102466 頁首框（宣告 `inset="0in,0in,0in,0in"`） | 綠 |
| 5 | 契約鎖：`Some`／`None` 可區分 | 綠（fdo76591 兩框皆 `None`＋`content_pic=true`） |
| 6 | tdf48658 頁首框內圖 | `#[ignore]` 欠債（頁首 `push_hf_slot` 無浮動框分支，見 §5） |

`cargo test -p quire-engine --test d9b_txbx_inset_data_driven` 收工實測（欠債一
`t_6aade0f3` 收案後）：`6 passed; 0 failed; 1 ignored`（欠債盞不髒零回退閘門；
原為 `5 passed; 0 failed; 2 ignored`，燈 3b 已解 `#[ignore]`）。

防調參護欄：`cargo test -p quire-engine --test d9_frame_overlap` 三燈全綠（merge main 後重跑實證）。

---

## 2. 卡面前提勘誤（第 2 條）：`22.05pt` 不是 `pgMar` 左邊距

卡面第 2 條要求「`22.05` 改為該 section 的 `pgMar` 左邊距」。實測**不成立**
（探針 `scratch_d9b/bleed_derive.py`／`vml_style_dump.py`，對 LO 真值框
`(22.05, 18.2, 567.0, 733.65)`——即 `lib.rs` 既有 R3 斷言鎖定的那框）：

| 檔案 | `pgSz` | `pgMar.left` | 版心寬 | fdo76591 實測框（LO 真值） |
| --- | --- | --- | --- | --- |
| fdo76591 | 612×792 | **36.00pt** | 540.00pt | `(22.05, 18.2, 567.0, 733.65)`：寬＝版心 1.05 倍，x−pgMar＝−13.95 |
| n751077 | 612×792 | **197.90pt** | 363.65pt | （該檔框幾何非本節標的；列此證明 `pgMar.left` 隨 section 劇變） |

三個事實：

1. fdo76591 的 `pgMar/@w:left` = 720 twips = **36.0pt**，不是 22.05pt。
   `22.05 = 36.00 + (−13.95)`，而 **−13.95pt 是 VML 自己宣告的**
   （`_x0000_s1026` 的 `style="position:absolute;left:0;margin-left:-13.95pt;
   margin-top:-17.8pt;width:567pt;height:733.65pt"`）——該常量的真正出處是
   **框的 VML 宣告**，不是 section 邊距。
2. 「框寬覆蓋版心」也不成立為判據：該框寬 567pt **超出**版心 540pt（1.05 倍），
   它是「蓋住整頁的负 z-index 背景框」（`z-index:-251659776`），語義是**覆頁**
   而非**貼版心**。
3. 若照卡面把 `22.05` 直接換成 `pgMar.left`，fdo76591 該臂由命中變不命中
   （22.05 ≠ 36.0），**三燈護欄 `d9_frame_overlap` 必紅**；n751077 這種大邊距
   section（左 197.9pt）更會被同條判定誤傷。

**可行的正確寫法**（資料推導，零檔案常量，且不觸碰三燈）：滿版判定改為
「框起點 ≈ `pgMar` 邊距＋框自身 VML 宣告偏移」的**同源推導**——即把 `-13.95`／
`-17.8` 這類偏移從解析層帶下來（`pos_x_pt`／`pos_y_pt` 已有欄位語義可承接），
判定式寫成 `框寬 ≥ 版心寬` 且 `|框起點 − (版心原點 ＋ 宣告偏移)| ≤ 容差`；
或按 `z-index < 0` ＋ `框面積 ≥ 版心面積` 立「背景覆頁框」事實。
兩條都要在解析層補欄位（VML `margin-left`/`margin-top` 的絕對位移目前未原樣搬到
排版層），屬**跨層改面**，且必須與第 1 條分開 commit 以便單獨回滾。
此項**未動**——卡面前置（數值前提）已被實測推翻，改法需 PM 裁定，不自行改寫卡面規格。

## 3. 燈 3（n792778）紅燈歸因：不是 inset，是群組子件只發出一張

實測（探針 `scratch_d9b/lamp3_diag.py`，新舊 binary 雙跑）：

* 我方 p0 只有 **1 張** Image `(43.20, 374.40, 130.90, 119.70)`；LO 有 **2 張**
  `[43.25,374.35,174.40,496.75]` 與 `[178.45,374.35,309.60,496.75]`（缺第二張 ⇒
  偏差 135.25pt 是「找不到」而非「位移錯」）。
* **base（merge-base `9768949`）與本枝輸出逐位相同** ⇒ 本卡的 inset 資料化對 n792778
  **零回退**；缺第二張是**既有缺口**，被新燈抓出來（正是先紅考卷的目的）。
* `PageFloatingImage` 層收到的是**群組外框** `(43.20, 374.40, 676.80, 122.40)`
  ＋`inset=Some([0,0,0,0])`＋`content_pic=true`；两张真圖屬 `v:group` 的兩個子件
  （`_x0000_s1127`／`_x0000_s1130`，皆無 `v:imagedata`，像素來自框內 `w:drawing`）。
  ⇒ 缺口在**群組子件的圖片發射**（一子件發、一子件漏），不是 inset 套用。
* 該面屬卡面白紙黑條的**嚴禁面**（VML 採集狀態機／`group_shapes` 欄位圖，R35
  `t_c088956f` 剛收案的同一帶）。本卡不越界修，改開欠債卡。

## 3b. 欠債一收案（t_6aade0f3）：純容器群組的子件框內圖逐框發射

**形態**（`word/document.xml` 實測，探針 `group_pretty.xml`）：`v:group` `_x0000_s1125`
（`margin-left:0;margin-top:331.2pt;width:676.8pt;height:122.4pt`，`coordorigin="864,4320"`
`coordsize="13536,2448"`）下掛兩個**巢狀子組** `_x0000_s1126`／`_x0000_s1129`，各自
`coordsize="2623,2448"`，子組內各一個 `v:shape`（`_x0000_s1127`／`_x0000_s1130`，
`type="#_x0000_t202"`、`filled="f" stroked="f"`、無 `v:imagedata`），像素來自子件
`v:textbox/@inset="0,0,0,0"` 框內的 `w:drawing`（兩張同 `a:blip r:embed="rId4"`）。

**既有兩條路徑都接不住**（這是缺口成因，不是 inset 套用錯）：

1. K25-B `vml_group_children` 雙閂都過不去——該臂只在「釘定群組（`page_pinned`）＋
   直系子件（`coord_stack.len()==1`）」開栓；本檔是 `margin-left/top` 定位（非釘定）
   ＋子件在**巢狀子組內**（幀棧高 2）。
2. 宿主候補的 `txbx_drawings`（R35）收得到兩張圖，但發射層的 `own_rids` 去重（宿主
   rid 與框內 blip 同源＝框已作為圖片發射）把它們**全部跳過**，宿主像素只有 rId4 一顆
   ⇒ 只剩一張發在框原點。

**架構決斷：新增 `VmlGroupPicture` 獨立投影，不改既有欄位圖。** `serde default`＋空
清單不外露 ⇒ 全庫 400 檔掃描只有本檔有此形態（探針 `scan_group_pics2.py`：`命中 1 檔`），
其餘檔案的投影序列化位元組逐位不變。

**發射尺＝子件框尺，不記框內 `wp:extent`——反事實實證（本 run 重跑，非引用前 run 說詞）**：
探針 `scratch_d9b/cf_extent_counterfactual.py` 把兩張框內 drawing 的 `wp:extent`
（原 `cx=1662430 cy=1520190` EMU＝130.90×119.70pt）改成 **48×36pt**（實替換 2 處），
Windows LO 25.8 重轉 PDF 後 pymupdf 實測：

```
p0 bbox=[43.25,374.35,174.40,496.75] w=131.15 h=122.40
p0 bbox=[178.45,374.35,309.60,496.75] w=131.15 h=122.40
```

與未改extent的LO真值逐位相同 ⇒ **LO 對這形態把圖塞進子件框、無視 drawing 自身 extent**。
故 `VmlGroupPicture` 若記框內 extent 就是誤導性欄位（發射層發的是框尺）。發射層
`take_image_pixel_at(..., frame_size_wins=true)` 據此**無視**池項 `txbx_drawing_extent`
（K27-B 覆寫），`false` 側既有口徑逐位不變（fdo76591 等框內圖仍吃自身 extent）。

**座標口徑**：排版原點＝群組外框原點＋子件映射矩形（巢狀 coord 幀逐層合成，`CoordFrame`
的 `map_*` 已是相對最外層組頂點）＋**子件自己**的 `v:textbox/@inset`（缺宣告退 VML 預設
7.2/3.6，K25-B 同款口徑，**不是**宿主框的 inset）。n792778 兩子件映射框
`[0,0,131.15,122.40]`／`[135.25,0,131.15,122.40]`＋宿主原點 `(43.20,374.40)`
⇒ 我方實發 `(43.20,374.40,131.15,122.40)`／`(178.45,374.40,131.15,122.40)`，
偏 LO 真值（`§3` 那兩張）分別為 **0.05pt（x/y 皆 ≤0.05）**／**0.05pt**。

**接管閂＝全數命中才接管**：任一子件座標非法或池裡無像素 ⇒ 整臂回 `false`＝宿主單發舊態
逐位不變（半套接管會同時留「少一張圖」與「灰底補位」兩類虛構墨跡，比舊態更糟）。接管
成功時宿主框自身不再作為圖片發射、也不發佔位框（LO 對純容器群組不繪框線）。宿主自身
有 `v:imagedata`／`o:gfxdata` 時採集層把清單清空＝舊行為零擾動。

**燈號與零回退實測**（本 run 實跑）：

| 項 | 實測 |
| --- | --- |
| `d9b_lamp3b`（解 `#[ignore]` 後） | 綠，偏 LO 0.05pt（另加「p0 恰好 2 張」守恆鎖） |
| `D9B_GROUP_PIC_BUGGY=1` 回塞態 | 燈 3b **必紅**（實得偏 135.25pt＝舊單發態）⇒ 鑑別力成立 |
| `d9b` 全考卷 | `6 passed; 0 failed; 1 ignored`（剩燈 6 屬 §5 另卡） |
| `cargo test --workspace` | `failed_total=0`（95 支 test target 全 ok） |
| `d9_frame_overlap` 三燈 | 全綠 |
| FMT / CLIPPY | `cargo fmt --check` 乾淨／`cargo clippy --workspace --all-targets` 零警告 |
| render_lab fdo76591 | 內容 0.98／版面 1.0／色 0.8／圖 0.797——改前改後**逐位相同** |
| render_lab WC048 | 內容 1.0／版面 1.0／色 0.922——改前改後**逐位相同** |

A/B 基準手法：**同一支 binary**，改前態用 `D9B_GROUP_PIC_BUGGY=1`（新臂入口第一行即
`return false`，其後程式碼路徑與改前逐位相同），排除編譯器/環境漂移。**但該 env 版
只作鑑別力用**——零回退鐵閘走**兩支 binary**（merge-base 真身）對照，結果與探針
（`ab_bin_diff_par.py`／`list_skipped.py`）記於 §6。

## 4. 第 3 條（正文文字字面量臂）：`t.contains("11-016") || t.contains("3-23-2011")`

實測（探針 `scratch_d9b/find_11_016.py`／`find_2011.py`）：

* 兩個字面量都在 `word/document.xml` 的 `w:t` 串接文內（跨 run 合成一段
  `11-0163-23-2011`），宿主是 `_x0000_s1026`（滿版背景框）的 `w:txbxContent` 第 36 段。
* 該段**無** `w:jc`、無域代碼（全文 `fldSimple`／`instrText` 零筆）、無 `mso-*`
  排版宣告（styles.xml／settings.xml 的 mso 宣告數＝0）。⇒ **源檔沒有任何結構特徵
  可辨識「這是頁腳行」**：LO 把它貼到頁底，靠的是「滿版背景框＋框內段落流排到末段」
  的排版結果，不是該段的宣告。
* 我方既有發射實測 `y=732.63`（LO 真值 ~730.53 區間），偏差來自框內段落流的**累積推進**
  （框內 24 段逐段行距誤差疊加），不是「頁腳段沒錨定」。把字面量換成
  「最後一段 ⇒ 錨頁底」也會錯——該框最後一段之後還有空段，且同族檔案的頁腳不一定在末段。
* **結論（老實掛帳，不虛構已完成）**：本臂需的是框內段落流的**逐段推進校準**
  （D8 家族），不在本卡白名單可及範圍內。已在 `core/engine-api/src/lib.rs` 該臂留
  `// TODO(D9b)` 註記＋本節指引。該臂的**字面量本身仍未移除**（移除即回退），
  並不在本卡交付內。

## 5. 燈 6 欠債（`#[ignore]`）

tdf48658_transparentOLEheader 頁首框內 `w:drawing` 圖**零發射**：頁首發射路徑
`push_hf_slot` 無浮動框分支（同 E2c run166 抓到的家族）。解鎖條件：該路徑補框內圖
分支＋套宿主框 inset。

## 6. 零回退鐵閘實測（欠債一 `t_6aade0f3` 收案 run）

**merge-base binary 逐位對照（最強那條，卡面白紙黑條）**：不是 env 模擬，是**兩支
不同 binary**——base＝merge-base `b4e833d` 在**乾淨 worktree**（`base-wt`，獨立
`base-target`）編出的 `quire-cli`；head＝本卡 worktree 編出的 `quire-cli`。探針
`scratch_d9b/ab_bin_diff_par.py`（12 平行 worker，`dump` 全頁一次到位；比對時剝
`data_b64`、浮點圓整 4 位、含 `page`／`width_pt`／`height_pt` 維度）。

```
scanned=400 skipped=1 files_with_diff=1
  DIFF/ITEMS ooxmlexport/n792778.docx:
    base=(0,0,('Image', (alt None, h 119.7,  mime image/png, w 130.9,  x 43.2, y 374.4)))
    head=(0,0,('Image', (alt None, h 122.4,  mime image/png, w 131.15, x 43.2, y 374.4)))
```

逐條判讀：

* `files_with_diff=1` ＝全庫 400 檔只有本卡標的檔 n792778 變動，**其餘 398 檔顯示
  列表逐位不動、頁數不動、頁面尺寸不動**（`DIFF/ITEMS` 後綴＝圖元層差異，非 `GEOM`
  頁面尺寸差異、非 `PAGES` 頁數差異）。
* `skipped=1` 是老實記帳：`poi/test-data_document_clusterfuzz-testcase-minimized-POIXWPFFuzzer-6733884933668864.docx`
  在 **base 與 head 兩側都**回 `zip 結構毀損: CRC-32 校驗不符`（探針
  `list_skipped.py` 實測兩側同錯）＝該檔是刻意造的毀損 fuzz 樣本，與本改動無關，
  不是回退。
* 首張圖 `130.90×119.70 → 131.15×122.40` 是**預期内且更準**的變動：base 那張走
  宿主單發臂、吃框內 drawing 自身 `wp:extent`（130.90×119.70，偏 LO 2.70/2.70pt）；
  head 走群組子件接管臂、吃**子件框尺**（LO 真值恰為 131.15×122.40，偏 0.05pt）。
  第二張圖由零發射轉為 `(178.45,374.40,131.15,122.40)`（偏 LO 0.05pt）——差分只在
  首個差異處截斷，故清單只列 idx 0。

**A/B 對照手法紀律**（本 run 踩過的坑，記給後續卡）：同一支 binary 用 `*_BUGGY=1`
env 開關做基線**只能證明新臂自身閂得住**，證明不了「編譯期其它位移」；要宣稱
「既有缺口、非本卡引入」需**兩支 binary**（merge-base 真身）。且 env 版基線在共享
checkout 髒態下不可信（見 §7）。

**render_lab 具名閘門**（Windows soffice.exe，同支 HEAD binary 兩跑：正常態 vs
`D9B_GROUP_PIC_BUGGY=1`）：fdo76591 內容 0.98／版面 1.0／色 0.8／圖 0.797、
WC048 內容 1.0／版面 1.0／色 0.922、n792778 色 1.0／圖 1.0——**三檔分數逐位相同**
（`diff` 分數行回 `SCORE_IDENTICAL`），跌幅 0.00 ≤ 卡面容差 0.02。
`render_lab.py --selftest` ALL-PASS。

**CI 同規閘門**（全部 worktree 實跑）：`cargo fmt --all --check` 乾淨；
`cargo clippy --workspace --all-targets -- -D warnings` RC=0；
`cargo test --workspace` failed=0（95 支 test target）；
`cargo check -p quire-crdt -p quire-engine -p quire-xlsx -p quire-odf
--target wasm32-unknown-unknown` RC=0；`d9_frame_overlap` 三燈全綠。

## 7. 工作區紀律事故與教訓（給 PM／後續卡）

本卡工作區是 scratch（`workspaces/t_6aade0f3`），但 run 233 把引擎改動直接寫進
**共享 checkout `~/projects/quire`**，造成兩項實質傷害（PM 11-09 00:57／03:20 兩則
值守備註抓出，worker 已於 01:04 留言承認）：

1. 污染以共享 checkout 為基線的閘門與 A/B（PM 的 `npm run build:wasm` 因此失敗）。
2. 主倉 `cargo build` 變紅：run 233 那 151 行是**半套**落地——帶了
   `Pending::new()` 的 `last_child_rect: None` 初始化，卻沒帶 `struct Pending` 的
   欄位宣告 ⇒ `error[E0560]`（`drawing.rs:1910`）。

**收案態與那段半套改動的關係**：`last_child_rect` 屬 run 233 的舊設計（「記上一個
子件矩形再延後讀」），本卡收案態**整段淘汰**該欄——子件矩形改在
`begin_group_pic`→`map_child_shape` 的**同一個 Start** 內直接寫進 `cur_group_pic.rect`
（原因寫進欄位註解：同 pict 內後續兄弟子件會覆寫映射結果，延後讀取必然串位）。
故本卡 worktree 裡 `grep -n last_child_rect core/ooxml/src/drawing.rs` ＝ 0 命中，
**不存在「補欄位定義」這條路**，共享 checkout 的殘留態不可直接續做。

**處置**：依還原鐵律，worker 一個位元組都没動共享 checkout（run 238 起 blob hash
與 PM 保護的 `refs/wip/t_6aade0f3-run233-v2` 逐位相符），改動原樣保留、清算等用戶
點頭。本卡全部實作與編譯驗證落 `workspaces/t_6aade0f3/wt`（branch `wt/t_6aade0f3`），
merge-base 基線另用乾淨 worktree `base-wt`（獨立 target），全程零 push。
**教訓**：scratch 卡的引擎碼一律 worktree；要測「改前態」用乾淨 worktree 編 base
binary，別拿可能髒的共享 checkout 當中立基線。

## 8. 殘留待辦（供 PM 裁定／開卡）

1. 第 2 條：卡面數值前提被 §2 實測推翻——需裁定改用 §2 末「同源推導」寫法
   （解析層補 VML 絕對位移欄位）或另定「背景覆頁框」事實；**不得**照原卡面把
   `22.05` 換成 `pgMar.left`（會打紅三燈護欄）。
2. ~~燈 3：群組子件圖片漏發（採集／發射面，R35 同帶）→ 另開欠債卡。~~
   **已收案**：欠債一 `t_6aade0f3`（本檔 §3b），`d9b_lamp3b` 解 `#[ignore]` 轉 CI 常態綠。
3. 燈 6：頁首框內圖零發射（`push_hf_slot`）→ 另開欠債卡。
4. 第 3 條：字面量臂的根治屬框內段落流逐段推進（D8 家族）→ 另開欠債卡。

## 9. 欠債三收案（t_da0682f0）：滿版框幾何四軸全部改由宣告推導

四條字面量臂（`22.05`／`500.0`／`540.5`／退內縮 `[11.3,4.0,3.0,0.0]`）全數退役，
判定與內縮都從「該頁版心盒（pgSz−pgMar）＋框自身 VML 宣告」推導。

### 9.1 反事實實測矩陣（LO 25.8 Linux，pymupdf 重抽，各變體只改一軸）

探針：scratch `cf_inset_experiment.py`（造變體）＋`probe_y_cf.py`／`probe_center_cf.py`
／`probe_wrap_cf.py`（量測）。標的框＝`_x0000_s1026`（宣告
`margin-left:-13.95pt`／`margin-top:-17.8pt`／`width:567pt`／`strokeweight:4pt`）。

| 變體 | 框左緣 | 正文左緣 | 置中件中心 | 正文行寬 max | 結論 |
|---|---|---|---|---|---|
| base | 22.05 | 33.35 | 305.552／305.575 | 540.58 | 基準 |
| `margin-left:+10pt` | +10.00 | +10.00 | +10.00 | — | 框與內容同動 ⇒ 內容錨**框起點** |
| `pgMar/@w:left:36→72pt` | +36.00 | +36.00 | +36.00 | — | 框起點由版心原點＋宣告偏移合成 |
| `mso-width:4→8pt` | 0.00 | **+4.00** | **+0.00** | — | 線寬項係數 1（非半值）；行盒關於**框盒中心對稱** ⇒ 左右內縮同式 |
| `mso-width:567→400pt` | 0.00 | 0.00 | −24.45 | 540.58→**349.62** | 行寬**跟框走**，不被版心掐 |

兩條關鍵判讀：

1. **`sw=8pt` 置中件位移 +0.00** 而左緣文字動 +4.00pt ⇒ 內文行盒恆關於
   **宣告框盒中心**（`bx + bw/2`＝305.55）對稱 ⇒ 左右內縮必須**同式**
   `宣告線寬 ＋ LO 框內建內縮 7.3pt`。舊臂 `inset_r = 3.0`（fdo76591 抄來）把
   行盒中心拉到 303.35，置中件整體偏 LO 2.20pt。
2. **base 最長正文行實測 540.58pt 已越過版心寬 540.0pt** ⇒ LO 從未對框內正文施加
   「≤ section 版心寬」掐制；`w=400pt` 變體行寬縮到 349.62 再添一枝證據。
   舊 `540.5` 掐制的動機（「FOLLOWING」穿框被裁）其實是**行寬算到框緣外**
   （舊臂 553.7pt）造成的假症——對稱內縮後框內緣即上界，不需要外來常數。

### 9.2 落地後口徑（`core/engine-api/src/lib.rs`）

* 滿版判定 `is_page_covering_frame`：`框寬 ≥ 版心寬` **且** `框高 ≥ 版心高` **且**
  `|框起點 −（版心原點＋VML 宣告偏移）| ≤ 1.5pt`（容差沿用 D9 既有值）。高軸是
  PM 定案式的**保守加嚴**：全庫掃描（scratch `scan_predicate.py`，含頁首尾 part）
  顯示只比寬會放進 **31 框**（最誇張 `height:6.05pt` 的細線框），補高軸後全庫
  唯 1 框命中＝`_x0000_s1026`，命中集與舊字面量臂逐檔相同。
* 宣告偏移只在 VML 原點系＝`Margin` 時搬到 `pos_x_pt`/`pos_y_pt`（沿用
  `InlineImage` 既有語義，未新增平行欄位）⇒ n751077（`pgMar.left`＝197.90pt，
  群組走 `page`/`margin` 釘定＋`center`）天然不成立＝卡面 §2 第 3 條誤傷例守住。
* 退內縮：左右＝`stroke_w + 7.3`（對稱）、上＝`stroke_w`、下＝0.0；有宣告
  `v:textbox/@inset` 時宣告值照舊取勝（D9b `txbx_inset` 傳遞鏈）。
* 行寬：`bw − inset_l − inset_r`（與宣告 inset 同式）。
* 描邊內收：4 邊各收 `width_pt/2`，故框線中心線對齊 LO（底線 749.85）。
* z-index 未進主路（PM 定案第 3 條：僅准作輔助條件，本卡未使用）。
* 第 4 項頁腳字面量臂（`11-016`／`3-23-2011`）依 PM 定案第 4 條**保留不動**、
  改掛 `// TODO(D8)`（刪臂＝fdo76591 頁腳燈必紅）。

### 9.3 閘門與鑑別力實跑

* `d9c_fullscreen_frame_geometry` 3 燈綠；`D9C_FRAME_GEOM_BUGGY=1` ⇒ 燈 2 必紅
  （實跑：`框底邊 y (751.85) 未依描邊向內收斂至 LO 真值 749.85`，2 passed/1 failed）
  ⇒ 本卡非空轉。
* `d9_frame_overlap` 3 燈綠（零回退護欄）；`d8_bottom_flow` 4 燈綠（行寬軸改動不碰
  坏 D8 底流）。
* `ow-gate.sh` 八閘 `GATE_SUMMARY fail=0`；corpus 樣式 A/B
  `scanned=399/400 regressed_files=0 regressed_items=0`。
* render_lab（fdo76591／WC048／n751077）頁數 3/3 一致，fdo76591 圖相似度
  0.796→**0.799**（置中件往 LO 收斂），其餘欄位不動。
* 逐項 diff（`ab_fdo_diff.py`）：全庫 fdo76591 顯示列表只有 2 項位移
  （置中寬圖 x 55.725→57.675，中心 303.60→**305.550**＝LO 305.575，誤差 1.98→0.03）。

### 9.4 殘留（老實掛帳，非本卡可及）

* `IMPORTANT NOTICE` 黑塊：LO 中心 305.575，我方 313.81（base）→**315.76**（head）。
  該項走另一條發射臂、base 態已偏 +8.24pt；本改動使其再往右 1.95pt
  （與行寬同步移動）。誤差主體是該臂自身的固定偏移，屬另卡；本卡不掩蓋、
  不順手改（避免一次動兩套滿版語義）。
* 真缺字 `1st`／`2nd`（上標切片）屬 D8/文字面，非本卡範圍。
