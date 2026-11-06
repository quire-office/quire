# F1 紅燈存檔（考卷先紅後綠的紅側證據）

署名：ADT109119。繁中註解，禁簡體禁 emoji。
實跑命令與實抓輸出照錄，不修飾、不推測。

---

## 1. 燈 1（解析層）紅燈側：`core/ooxml/tests/f1_chart_projection.rs`

紅燈態＝`core/ooxml/src/chart.rs` 尚未存在（main `5857114` 口徑：
`grep -rn 'c:chart\|chartSpace\|ChartPart' core/ooxml/src/*.rs` 零命中）。
該態下考卷連編譯都進不去（`quire_ooxml::read_charts_from_docx` 未定義），
即 PM 在卡面實查的「圖表 part 從未被解析」之直接證據：

```
error[E0432]: unresolved import `quire_ooxml::read_charts_from_docx`
```

解折器（commit b42b822＋40b7baf）落地後四盞全綠：

```
running 4 tests
test f1_lamp1a_chart_size_series_and_cache_values ... ok
test f1_lamp1b_chart_part_parses_under_strict_namespace ... ok
test f1_lamp1c_title_text_parsed_when_present ... ok
test f1_lamp1d_files_without_charts_return_empty ... ok
test result: ok. 4 passed; 0 failed
```

## 2. 燈 2／燈 3（發射層）紅燈側：`core/engine-api/tests/f1_chart_emission.rs`

實跑（解析層已就緒、發射層未動工的態，即「圖表被整段跳過」的臨界態）：

```
cargo test -p quire-engine --test f1_chart_emission -- --test-threads=1

test f1_lamp2_chart_size_axis_ticks_emitted ... FAILED
test f1_lamp2f_title_text_emitted ... FAILED
test f1_lamp3_buggy_toggle_and_no_chart_files_untouched ... FAILED

---- f1_lamp2_chart_size_axis_ticks_emitted stdout ----
燈 2a：數值軸刻度 0 未發射，實測文字清單 ["Body text.", "Before.", "", "After."]
---- f1_lamp2f_title_text_emitted stdout ----
燈 2f：fdo76098 的圖表標題 Sales 未出現在任何頁發射清單
---- f1_lamp3_buggy_toggle_and_no_chart_files_untouched stdout ----
燈 3 前置：正常態必須有數值刻度
test result: FAILED. 0 passed; 3 failed
```

關鍵實抓：我方 p0 發射清單只有四支正文文字（`Body text.`／`Before.`／空段／`After.`），
LO 在同一頁繪出的 `0,1,2,3,4`（數值軸）與 `1,2,3`（類別軸）**一字未發**——
與 `docs/f1-chart-anatomy.md` §1 要點 B 的 LO 真值表逐字對照成立。

我方當時幾何項全貌（`docs/f1-survey/dump_current.sh` 實抓）：

```
== page 0 612.0 792.0
  Rect  x=212.25 y=86.25 w=310.50 h=110.60 fill=ffffffff   ← 宿主文字框（既有發射）
  Rule  (212.25,86.25)-(522.75,86.25) w=0.75               ← 宿主框四邊
  Rule  (212.25,196.85)-(522.75,196.85) w=0.75
  Rule  (212.25,86.25)-(212.25,196.85) w=0.75
  Rule  (522.75,86.25)-(522.75,196.85) w=0.75
  Text  x=72.00  y=72.00  sz=11.00 t='Body text.'
  Text  x=215.25 y=90.25  sz=11.00 t='Before.'
  Text  x=215.25 y=92.67  sz=11.00 t=''
  Text  x=215.25 y=95.08  sz=11.00 t='After.'
```

圖表本體（LO：外框 `[219.85,112.75,389.35,214.15]`＋5 條水平灰格線＋5 支數值刻度
＋3 支類別刻度＋5 支柱形）在我方產物中**完全不存在**，故 `chart-size` 圖軸分 0.0。

## 3. 紅燈同時鎖住的兩條邊界（非本次要修的目標，避免順手改）

1. **宿主框內段落流的 y 精度**：LO `After.` 基線帶頂 223.83，我方發 95.08——
   既有 `emit_txbx_texts` 把圖表段收成 2.42pt 壓縮帶所致（框內段落流近似，
   屬 D9b／E2b 同族既有缺口，非本卡圖表解析缺口）。本卡只發圖表幾何與文字，
   不改框內段落流堆疊律；圖表自身的定位律見實作模組頭。
2. **LO 本地化佔位標題**：`圖表標題` 四字出自 LO 繁中介面（chart-size 全封包零
   CJK，解剖報告 §1 要點 B 實證），我方不虛構 → 本卡後該檔真缺字預期由 9 降到
   4（`圖`,`表`,`標`,`題`），非 0。此為設計結論，非未修完。
