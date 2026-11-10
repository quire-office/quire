# E1-C 卡 D：`w:t`／`w:delText` 空白口徑定案（xml:space 節點級裁剪）

- 卡號：`t_6370fd10`（E1-C 卡 D，承接 `t_ab6c5319`）；殘留斷點差一格改立 `E1-C 卡 E` 追查。
- 真值環境口徑沿用 `docs/e1cb-lo-truth-canon.md`（E1-C 卡 B 定案）：Linux LO 25.8 主力、專屬 `-env:UserInstallation`、私有 fontconfig 注入真 face、pymupdf **rawdict** 逐字量測。
- 考卷：`core/ooxml/tests/e1cd_xml_space_node.rs`（4 盞，含 buggy 回塞鑑別力）。

## 1. 卡面舊歸因被真值推翻（這是本卡最重要的記帳）

卡面（承 `t_ab6c5319` 記帳）宣稱：cell-grid-span Team 段 6/9→9/9 的臨門一腳是「我方未折疊連續雙空格（LO 折疊；每處虛增 2.24pt）」。

PM 覆核以 pymupdf **rawdict** 逐字量 LO 真值 PDF（Linux LO 25.8、真 Trebuchet 注入態）：`w:t`「(Tablet PC),␣␣during September.␣␣iBall」的兩處連續雙空格，LO 各渲染**兩個空格字形**（`)` 後空格原點 398.56 與 401.65，`d` 起於 404.49），**LO 不折疊節點內連續空白**。

受控合成 fixture 佐證（Mono 10pt、單空格實距 6.0pt）：

| fixture | `w:t` 內容 | LO gap(A→B) | 判定 |
|---|---|---|---|
| sp1 | preserve「AAAA BBBB」 | 6.0pt | 單空格＝6pt |
| sp2 | preserve「AAAA␣␣BBBB」 | 12.0pt | 雙空格＝12pt，**不折疊** |
| sp3 | preserve「AAAA␣␣␣BBBB」 | 18.0pt | 三空格＝18pt，逐個計寬 |
| lead3 | preserve 行首三空格 | 行首＝空格@72.1 | preserve 頭尾保留 |
| lead3_nopres | 無 preserve 行首三空格 | 行首＝A@72.1 | **無 preserve 剝頭** |
| trail3_nopres | 無 preserve 行尾三空格 | 尾空白無字形 | **無 preserve 剝尾** |
| sp2_split | preserve 雙空格被實體切兩片 | 12.0pt | 跨片空白屬節點內，保留 |
| sp2_split_nopres | 無 preserve 同上 | 0.0pt（A 緊接 B） | 跨片頭尾空白整節點剝除 |

「LO 折疊」錯在量測工具：pymupdf `get_text("words")`／`get_text("text")` 抽詞時把任意連續空白折成單空格輸出，那是**抽取器行為不是渲染行為**。E1-C 卡 B 考卷註解與卡 D 卡面的「每處虛增 2.24pt」皆據此誤判，本檔正式校正；相關註解以「LO 不折疊（rawdict 實測）」為準。

## 2. 定案口徑（ECMA-376 §17.3.2.31＋上表實測）

1. 文字節點（`w:t`／`w:delText`）**整段緩衝、End 時一次裁剪**：未帶 `xml:space="preserve"` 時剝除節點**頭尾** XML 空白（僅空格、TAB、CR、LF——刻意不用 `char::is_whitespace`，NBSP 等正文字元不得動）；節點**內**連續空白一律保留、原樣計寬。
2. `xml:space="preserve"` 時節點頭尾空白原樣保留。
3. 裁剪以**整節點**為單位，不是逐 Text 分片：quick-xml 會在實體（如 `&amp;`）邊界把同一節點切成多個 Text 事件，逐片 trim 會把跨片頭尾空白誤判成節點內空白而丟字（`  A  &amp;  B  ` 無 preserve 正確得「A  &  B」，逐片 trim 退化「A&B」）。
4. 框內（`w:txbxContent`）文字節點與正文共用同一節點口徑與同一裁剪函數。
5. `cp_done`（br-page 偏移標尺）只累計**實際入文**碼點，裁剪掉的空白不計。
6. 畸形 XML 的孤立文字（未見 Start 的 Text 事件）照舊直接入 run，不吞字。

## 3. 實作位置與防偽綠

- 實作：`core/ooxml/src/lib.rs` 的 `is_xml_space`＋`flush_text_node`＋正文／框內兩組 `*_t_buf*` 狀態機（Start 設 preserve 旗標、Text 入緩衝、End 一次裁剪）。
- 鑑別力：燈 3 在「逐片 trim」舊口徑必紅（吞跨片空白）；燈 2 在「一律 preserve」口徑必紅（該剝不剝）。兩燈方向相反互封退化實作。回塞實測紀錄見提交說明。

## 4. 殘留（不在本卡）——**卡 E 覆核更正（t_0e390551，2026-11-10）**

~~cell-grid-span Team 段仍 6/9（真值 9 行，行 0／1／2 位移一格）。~~雙空格折疊說已死，
根因改查方向（advance 尺字組差／對齊與 tab stop／可行帶計算）立 **E1-C 卡 E** 追。
**卡 E 實測收案**：卡 C（宣告族標點覆寫臂）落地後該段已 **9/9 逐行全等**（rawdict 真值
口徑雙口徑互證）；翻轉源坐實＝行 0 `Innovation…` 的 U+2026 由 1.0 em 方塊收窄真 trebuc
hmtx 7.344pt（淨 2.400pt）使 `helped` 放進行 0，同碼拔臂 A/B 退回 6/9 唯一變量＝臂。
三候選中僅「標點 advance」一支成立，對齊／tab stop／段寬實測排除。全鏈記帳見
`docs/e1c-card-e-team-99.md`。
