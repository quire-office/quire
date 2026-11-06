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
| 3 | n792778 群組框（宣告 `inset="0,0,0,0"`） | 綠（我方實際發射的那張偏 LO 2.70pt；原燈斷言的第二張屬**群組子件漏發**，另掛盞，見 §3） |
| 3b | n792778 群組第二張圖 | `#[ignore]` 欠債（`v:group` 第二子件零發射，非 inset 路徑；見 §3） |
| 4 | tdf102466 頁首框（宣告 `inset="0in,0in,0in,0in"`） | 綠 |
| 5 | 契約鎖：`Some`／`None` 可區分 | 綠（fdo76591 兩框皆 `None`＋`content_pic=true`） |
| 6 | tdf48658 頁首框內圖 | `#[ignore]` 欠債（頁首 `push_hf_slot` 無浮動框分支，見 §5） |

`cargo test -p quire-engine --test d9b_txbx_inset_data_driven` 收工實測：
`5 passed; 0 failed; 2 ignored`（欠債盞不髒零回退閘門）。

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

## 6. 殘留待辦（供 PM 裁定／開卡）

1. 第 2 條：卡面數值前提被 §2 實測推翻——需裁定改用 §2 末「同源推導」寫法
   （解析層補 VML 絕對位移欄位）或另定「背景覆頁框」事實；**不得**照原卡面把
   `22.05` 換成 `pgMar.left`（會打紅三燈護欄）。
2. 燈 3：群組子件圖片漏發（採集／發射面，R35 同帶）→ 另開欠債卡。
3. 燈 6：頁首框內圖零發射（`push_hf_slot`）→ 另開欠債卡。
4. 第 3 條：字面量臂的根治屬框內段落流逐段推進（D8 家族）→ 另開欠債卡。
