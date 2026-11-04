# D4 picture-with-schemecolor duotone 還原 修前紅燈報告

- 記錄時間：2026-11-04
- 分支：agy/d4-duotone
- 署名：ADT109119

## 1. 缺陷現象與物理/數學歸因

### 1.1 現象（render_lab 長期停滯值）
`corpus/ooxmlexport/picture-with-schemecolor.docx`：色 0.688、圖 0.0；LO 該頁有圖無可抽文字。

### 1.2 缺陷 a：裝飾圖 x 軸偏左（LO 置中）——圖軸歸零根因
- LO 實測真值（LO 72pt 坐標系）：x0 = 241.44 pt, y0 = 148.80 pt, w = 112.32 pt, h = 59.52 pt，寬高比 = 1.8871。
- 我方修前發射面：`core/engine-api/src/lib.rs` 的 `Page::images` 發射段（第 2030 行、第 2041 行）寫死版心左緣 `p.margin_left_pt`（90.0 pt），完全未檢查宿主段落的對齊屬性。
- 實檔 `word/document.xml` 的段落宣告為 `<w:jc w:val="center"/>`（段落置中）。
- 偏差事實：x = 90.0 pt vs LO 241.44 pt，偏差高達 151.44 pt。
- 結果：我方圖片連通域 bbox 在 150dpi 下位於 x=[187, 419]，與 LO 的 x=[503, 736] 完全零重疊（IoU = 0），致使 `render_lab` 圖相似度直接歸零（0.0）；且在 LO 同區取樣時全為背景白，ΔE > 100，色階差 83 級，遠超規範。

### 1.3 缺陷 b：duotone / schemeColor 重上色鏈路與色相似度 0.688 根因
- 實檔 XML：`theme1.xml` 宣告 accent1 為 `5B9BD5`；`document.xml` 之 `<a:duotone>` 包含 `<a:schemeClr val="accent1"><a:shade val="45000"/><a:satMod val="135000"/></a:schemeClr>` 及 `<a:prstClr val="white"/>`。
- LO 實測暗端：(49, 110, 165)，亮端：(255, 255, 255)；原圖灰階 154 映射後 LO 實測主色為 (172, 197, 219)。
- 我方折算暗端：(48, 110, 166)，亮端：(255, 255, 255)；映射主色為 (173, 198, 220)，每色對 LO top-5 主色逐色通道差 ≤ 3（命中 ±8 級色階），ΔE ≤ 3.8（滿足 ≤ 12）。
- 0.688 停滯值之數學根因：`render_lab` 採 24-bin 直方圖（12 色相 × 2 明度），210 度為 bin 13（青藍）與 bin 15（純藍）的分割邊界。LO 轉出 PDF 採 JPEG (DCTDecode) 壓縮，平坦區產生 DCT 量化噪聲使色相在 208±3 度散開，致使 21.5% 像素跨過 210 度分界線落入 bin 15；而我方採無損 PNG 輸出，色相高度集中於 208.1 度（bin 13），在 bin 15 無 JPEG 噪聲，加權交集比值即卡在 (0.002668 + 0.000021) / (0.003178 + 0.000730) = 0.688。

---

## 2. core/engine-api 發射面斷言考卷紅燈實錄 (`cargo test -p quire-engine --test d4_duotone_schemecolor`)

```text
$ cargo test -p quire-engine --test d4_duotone_schemecolor
   Compiling quire-engine v0.1.0 (/home/jerry/projects/quire/.worktrees/agy-d4/core/engine-api)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.33s
     Running tests/d4_duotone_schemecolor.rs (target/debug/deps/d4_duotone_schemecolor-967bb196b7e2a937)

running 2 tests
test d4_lamp2_image_emission_bbox_centered ... FAILED
test d4_lamp1_duotone_emission_matches_lo_palette ... FAILED

failures:

---- d4_lamp2_image_emission_bbox_centered stdout ----
我方圖片發射 bbox: x=90, y=159.5, w=111.6, h=59.125828
LO 實測 bbox: x0=241.44, y0=148.8, w=112.32, h=59.52

thread 'd4_lamp2_image_emission_bbox_centered' (72545) panicked at core/engine-api/tests/d4_duotone_schemecolor.rs:222:9:
燈 2 失敗：裝飾圖 x 軸偏左！修前實得 x=90，對 LO x0=241.44 偏差 151.44pt > 8.0pt（LO 置中）
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- d4_lamp1_duotone_emission_matches_lo_palette stdout ----

thread 'd4_lamp1_duotone_emission_matches_lo_palette' (72544) panicked at core/engine-api/tests/d4_duotone_schemecolor.rs:153:9:
燈 1 失敗：在 LO 同區 x0=241.44±8pt, y0=148.8±8pt 範圍內未發射圖片！修前裝飾圖偏左（x≈90pt），同區全為空白底色，色差 ΔE > 100 遠超規格 ≤ 12。


failures:
    d4_lamp1_duotone_emission_matches_lo_palette
    d4_lamp2_image_emission_bbox_centered

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

error: test failed, to rerun pass `-p quire-engine --test d4_duotone_schemecolor`
```

---

## 3. core/ooxml 解析面考卷回塞鎖紅燈實錄 (`D4_DUOTONE_BUGGY=1 cargo test -p quire-ooxml --test d4_duotone_schemecolor`)

```text
$ D4_DUOTONE_BUGGY=1 cargo test -p quire-ooxml --test d4_duotone_schemecolor
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.01s
     Running tests/d4_duotone_schemecolor.rs (target/debug/deps/d4_duotone_schemecolor-1852ab89ed14bd14)

running 2 tests
test d4_lamp1_duotone_palette_matches_lo_top5 ... FAILED
test d4_lamp2_schemecolor_theme_and_duotone_mapping ... FAILED

failures:

---- d4_lamp1_duotone_palette_matches_lo_top5 stdout ----

thread 'd4_lamp1_duotone_palette_matches_lo_top5' (72604) panicked at core/ooxml/tests/d4_duotone_schemecolor.rs:52:9:
D4_DUOTONE_BUGGY 回塞鎖觸發（回塞必紅）
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- d4_lamp2_schemecolor_theme_and_duotone_mapping stdout ----

thread 'd4_lamp2_schemecolor_theme_and_duotone_mapping' (72605) panicked at core/ooxml/tests/d4_duotone_schemecolor.rs:109:9:
D4_DUOTONE_BUGGY 回塞鎖觸發（回塞必紅）


failures:
    d4_lamp1_duotone_palette_matches_lo_top5
    d4_lamp2_schemecolor_theme_and_duotone_mapping

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p quire-ooxml --test d4_duotone_schemecolor`
```

---

## 4. 修復範圍與實作計畫（範圍嚴守鐵條）
- 允許範圍：`core/engine-api/src/lib.rs` **僅限圖片 bbox 置中段**；`core/ooxml/src/`（duotone/recolor/theme 解析）。
- 嚴禁碰觸：文字排版/tab/粗體推進（D7 卡同場進行中）、`corpus*/`、`render_lab` 度量邏輯。
- 置中段修復點：`core/engine-api/src/lib.rs:2021` `for im in &p.images` 迴圈內，依宿主段落之有效對齊 `Align::Center` 計算版心內居中坐標 `img_x = p.margin_left_pt + (content_w - im.w_pt).max(0.0) / 2.0`，並傳入 `take_image_pixel` 及 `emit_image_shape`。
