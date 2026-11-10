# E1-C 卡 C：拉丁上下文的全形標點 advance——按宣告族的覆寫臂（t_900f3b25）

真值口徑比照 `docs/e1cb-lo-truth-canon.md`（**真 MS face 裝機態**為基準）與
`docs/e1c-advance-anatomy.md` §2（定錨配方）。所有數值皆為實測，無一筆推估；
缺口以實測差值表述，不用「約／大約」。

---

## 0. 卡面前提逐條更正（先證再寫考卷）

卡面（本卡 body）與 `docs/e1c-advance-anatomy.md` §6 卡 C 段的前提，經四條實測後
**手段與真值口徑都要反轉**。逐條如下（PM 已於 comment 323／PM 值守 11-10 07:40 採納）：

| # | 卡面寫法 | 實測結果 | 處置 |
|---|---|---|---|
| 1 | 「修子集產生器／子集本身使這二個碼點的 advance 落到真值」 | 原始 `NotoSansCJKtc-Regular.otf` 與我方子集 `core/layout/assets/quire-notosanstc-subset.otf` **逐字全等**（’ … 皆 1.0000 em，scratch `probe_src_advance.py`）⇒ 不是子集產生器缺陷，是 CJK 字型的正當設計（標點佔 em 方塊）。改 otf ＝ 改字型設計。 | 手段反轉：不動 otf，改在量測層按宣告族覆寫 |
| 2 | 「LO NotoSans 真值 U+2019＝1.750pt（0.175 em）、U+2026＝7.910pt（0.791 em）」 | 那是 **LO 自帶 NotoSans 的 hmtx**＝卡 B 明文否決的「頂替態」口徑（scratch `probe_punct_truth.py` 直接讀該 otf hmtx 復現該數）。同一個 ’ 隨**宣告族**而變（scratch `probe_lo_face_punct.py` 定錨態 rawdict 逐 face 實渲）：Calibri 0.2495／Cambria 0.2212／TNR 0.3330／Arial 0.2222／NotoSerif 0.2500／**Trebuchet 0.3672 em**。**沒有單一的 0.175／0.791**。 | 真值口徑反轉：按 face 查表，頂替態數永不得入表（燈 5 鎖死） |
| 3 | 「同檔正常字距旁證：空格 0.224em、逗號 0.278em、`i` 0.275em ⇒ 1.0em 是異常值（不是字型設計）」 | 該旁證只在**拉丁上下文**成立。漢語段 LO 對 “ ” 實渲 **1.0 em**（scratch `probe_lo_rawdict.py`：源檔「插入”選項卡」Δ＝11.0pt@11pt＝恰 1.0 em，渲 WenQuanYiZenHei）。 | 覆寫範圍收窄：**只對非 CJK script run 生效**（燈 4 鎖死） |
| 4 | 「爆炸半徑：含 U+2019 41 檔、含 U+2026 10 檔、含任一 43 檔」 | 本 run 實掃（`c_fullwidth_cov.json`）：子集實測 ≥0.999 em 的碼點是 **11 個**（‘ ’ “ ” ‥ … ‧ · • ― ‐），非 2 個；拉丁上下文含任一者 **50 檔**。卡面只點二個碼點＝低估。 | 覆寫集＝實測 11 碼點；爆炸半徑以本 run 數為準 |
| 5 | 「本卡落地後 Team 段可行尺帶從 [0.9617,0.9801) 遷移至 [0.9750,0.9851)，現行 0.9744 落帶外 ⇒ 須重定 Trebuchet 臂值」 | 該遷移區間是**拿頂替態（0.175／0.791）推的**（本 run `verify_rescan.py` 對照組重現 [0.9750,0.9851) 且 0.9744 落帶外，證實其只對頂替態成立）。按卡 C **實際落地**的真 trebuc hmtx 臂重解：帶遷移至 **[0.9712, 0.9851)**，0.9744 **仍在帶內**（下緣餘 0.0095／上緣餘 0.0107）；B 軌逐行中位 0.9744 → 0.9748。 | 尺帶重掃已做，**臂值 0.9744 不變**（見 §4）；卡面「須重定臂值」不成立 |

## 1. 缺陷本體與實測數

`core/layout/assets/quire-notosanstc-subset.otf`（引擎 `include_bytes!` 內嵌）把
Noto Sans CJK TC 的整套 hmtx 照抄進來，於是**拉丁上下文**裡的 11 個全形標點也吃
1.0 em 方塊：

- cell-grid-span Team 段宣告 `Trebuchet MS`，段內 `iBall’s` 的 ’：我方 **10.000pt@10pt
  （1.000 em）** vs 定錨 LO 實渲 **3.672pt（0.3672 em，真 `trebuc.ttf` hmtx）**
  ⇒ 單字元虛增 **6.328pt@10pt**（卡面記 8.25pt 是拿頂替態 1.750pt 算的差值：
  10.000 − 1.750 ＝ 8.250，非定錨態差值）。
- 11 個碼點（子集實測 ≥0.999 em）：U+2018 U+2019 U+201C U+201D U+2025 U+2026
  U+2027 U+00B7 U+2022 U+2015 U+2010。真 face 缺字（如 Arial 的 U+2010）＝表內缺席
  ＝不覆寫（**無真值即不猜**，比照 K7 未命中＝1.0 守門）。

## 2. 修法：按宣告族的全形標點覆寫臂

新增 `core/layout/src/punct_advance.rs`：6 臂覆寫表（Calibri／Cambria／Times／
Arial／NotoSerif／Trebuchet），值源＝**真實 face 二進位的 hmtx 直讀**
（`calibri.ttf`／`cambria.ttc` Cambria 面／`times.ttf`／`arial.ttf`／`trebuc.ttf`／
LO 25.8 自帶 `NotoSerif-Regular.ttf`），並與 LO rawdict 物理渲染交叉驗證：
TNR ’ 0.331 vs 0.3330、Calibri ’ 0.249 vs 0.2495、NotoSerif ’ 0.250 vs 0.2500、
TNR … 1.000 vs 1.0000（PDF 0.05pt 量化內）。

臂的性質（與既有 `face_advance_scale` 的關係）：

- **取代而非疊乘**：LO 渲 ’ 用的就是該 face 自己的 hmtx，與水平尺（拉丁字母寬比）
  無涉 ⇒ 覆寫值直接取代 cluster 寬，`scale` 軸正交。
- **只作用非 CJK script run**：Han／Hangul／Hiragana／Katakana run 原樣走 em 方塊。
- **同臂同守門**：`punct_arm_for_face` 的鍵分組與 `face_advance_scale` 逐臂對齊
  （燈 2 雙臂對齊鎖；兩表走樣＝「拉丁窄了、標點沒窄」的半套尺，最難查）。
- **同一回塞鎖**：`K7_WRAP_WIDTH_BUGGY`／`K7_ADVANCE_SCALE_BUGGY` 同時退水平尺與
  標點臂（同一量測模型的兩軸，分開回塞會讓斷行尺與標點尺不同源）。

穿線路徑（量測＝發射同源）：`shaping.rs::shape_advances_with_punct` →
`TextMeasurer.punct_arm` → `kp.rs`（KP 斷行）／`lib.rs::ParaGrid.punct_arm`（段級）／
`table.rs::CellInput.text_punct_arm`（格內斷行，混排＝同格兩段不同臂 ⇒ 保守退回
不覆寫）／`engine-api::para_punct_arm`（選族口徑與 `para_advance_scale` 逐行同構）／
`emit_line_run_items` 的 `seg_w`（發射側同臂）。`tools/line-metrics`（PM 的
`build:wasm` 閘門路徑）直接共用 `quire_layout::punct_arm_for_face` ⇒ 工具與引擎不分叉。

`PUNCT_ARM_NONE`（＝0，未宣告族／未命中／回塞鎖）時既有行為**逐位元組不變**。

## 3. 考卷與鑑別力

- `core/layout/src/punct_advance.rs`：4 盞單元鎖（表值域、臂 id 穩定、缺字不覆寫）。
- `core/engine-api/tests/e1c_subset_punct.rs`：6 盞（燈 1 真檔 ’ 量寬 0.3672 em、
  燈 2 雙臂對齊、燈 3 回塞鎖、燈 4 漢語 1.0 em 保持、燈 5 頂替態永不入表、
  燈 6 文字守恆）。
- `core/engine-api/tests/k23_cell_text_width.rs`：估測器同步吃臂（`est_right`／`meas`
  兩處，PM 覆核坐實）——該考卷的「推估右緣」原本用**不帶臂**的 `TextMeasurer`，
  窄化後低估實際佔位 ⇒ 屬**考卷估測器滯後**，非引擎回退。

### 鑑別力實跑（本 run 實跑，`gates.log` 第 8／9 節）

| 模式 | 命令 | 結果 |
|---|---|---|
| 正常態 | `cargo test -p quire-engine --test e1c_subset_punct` | 6 passed 0 failed |
| 正常態 | `cargo test -p quire-engine --test e1c_advance_trebuchet` | 3 passed 0 failed（燈 2 斷行——**卡 E 覆核更正**：本卡落地後實跑為 **9/9 逐行全等**，卡 C 記帳沿用卡 A 時期「6/9」舊測；≥6 綠態不輸出實得值故未察覺。燈 2 門檻已由卡 E 釘死 9/9） |
| 回塞（水平軸） | `K7_ADVANCE_SCALE_BUGGY=1 …--test e1c_subset_punct` | **2 FAILED**（燈 1 ’ 退回 10.0pt＝9.744pt 方塊態、燈 4 的「拉丁必須被覆寫」反向防假綠斷言同時紅）→ 鑑別力坐實 |
| 回塞（水平軸） | `K7_ADVANCE_SCALE_BUGGY=1 …--test e1c_advance_trebuchet` | **1 FAILED**（燈 2 斷行退回 0/9） |
| 回塞（幾何＋水平） | `K7_WRAP_WIDTH_BUGGY=1 …--test e1c_subset_punct` | **2 FAILED**（同上二盞） |
| 回塞（垂直軸） | `K6_AUTO_PITCH_BUGGY=1 …--test e1c_advance_trebuchet` | **1 FAILED**（燈 1 pitch） |

燈 4 的防假綠結構值得記帳：它同時斷「Han run 內 ’ 不得被覆寫」與「同 face 拉丁上下文
的 ” **必須**被覆寫」——後者讓「臂根本沒接上」無法冒充燈 4 的綠。實測回塞態下燈 4
確實紅（`47.069 vs 47.069`），證明該燈不是恆真。

## 4. 連動鐵律：重掃 Trebuchet 尺帶（卡面推估覆核＝不成立，臂值不動）

目標斷行＝定錨 Windows LO（render_lab 板）cell-grid-span Team 段 9 行，版心 468pt，
逐行需求帶取交集（`rescan_cardC.py`，實據 `c_rescan_cardC.json`）：

| 我方側 ’／… 口徑 | B 軌逐行比中位 | 可行尺帶 | 現行 0.9744 |
|---|---|---|---|
| 原態（1.0 em 方塊） | 0.9744 | [0.9617, 0.9801) | 在帶內（上下緣 0.0127／0.0057） |
| **卡 C 落地態（真 trebuc hmtx）** | **0.9748** | **[0.9712, 0.9851)** | **在帶內（0.0095／0.0107）** |
| 卡面頂替態口徑（0.175／0.791） | 0.9783 | [0.9750, 0.9851) | **帶外** |
| 旁證：真 trebuc 自身度量 | — | [0.9922, 1.0069)（≈1.0） | — |

判讀：卡面預期的 [0.9750, 0.9851) 只在頂替態下成立（本 run 對照組重現），而頂替態已被
卡 B 否決；按實際落地的真 face 臂，帶只把下緣由 0.9617 抬到 0.9712（’ 收窄 ⇒ 行變窄 ⇒
下緣上升），**0.9744 仍在帶內，故本臂不需重定值**。B 軌中位 0.9744→0.9748 的差 0.0004
小於取樣步長，不構成分檔變動；`e1c_advance_trebuchet` 燈 2 實跑——**卡 E（t_0e390551）
覆核更正**：本卡落地後該段已 **9/9 逐行全等**（本節舊文「仍 6/9」係沿用卡 A 時期舊測未重跑；
斷行鎖≥6 綠態不輸出實得值故未察覺，門檻已由卡 E 釘死 9/9，尺與臂走樣都會先紅）。
尺帶上緣 0.9851 由行 7 的自然寬決定（實測 475.09pt×s < 468 需 s < 0.9851）。

`face_advance_scale("trebuchet ms")` 檔頭的 TODO 已改寫為本節實測記帳（不是刪掉）。

## 5. 零回退自證（雙 binary，base＝merge-base 279c1ee 乾淨 worktree）

`base-wt` 由 `git worktree add …/t_900f3b25/base-wt 279c1ee` ＋獨立 `CARGO_TARGET_DIR`
編出 release binary；head＝本卡 worktree release binary。

| 閘門 | 結果 |
|---|---|
| 樣式 A/B（`ow-style-ab.py`） | scanned=399/400，**regressed_files=0 regressed_items=0** |
| 400 檔頁數掃描（`blast_parallel.py`，12 平行） | manifest 400／有效 399（1 檔 skip＝POI fuzz CRC-32 毀損，**兩側同錯** rc=1 輸出逐字相同）⇒ **頁數改變 0 檔** |
| 全 corpus 字元多重集守恆（`multiset_check.py`） | **392 檔逐字元全等＋7 檔字元多重集守恆（僅行/頁邊界位移）＋1 skip＝399/400**，**multiset_diff＝0**（無吞字、無長字） |
| render_lab 觸發集六檔（`scripts/render_lab.py`，base vs head） | **兩份報告除生成時間戳外逐行相同**（`diff` 只剩時間戳行）：cell-grid-span／fdo76591／RA001-Tracked-Revisions-01／CoverPage／WC048-Text-Box-in-Cell／btlr-textbox |
| `cargo test --workspace` | **0 failed**（全線 `test result: ok`，無 FAILED 行） |
| `cargo fmt --all --check`／`clippy --all-targets -D warnings`／`check-layering.sh`／`roundtrip fixtures --check`（14 檔 byte-identical）／wasm32 四條 check | 全綠（`gates.log`） |

### 幾何差分 23 檔的可解釋性（逐條，非「差異少就算過」）

顯示列表雜湊差分 23 檔（頁數零變動）。查核（`explain_geom.py`／`explain_parts.py`）：
其中 21 檔的覆寫碼點在 `word/document.xml`、1 檔在 `word/numbering.xml`、1 檔在
`word/glossary/document.xml`——即**全部**落在「含覆寫碼點 ∩ 宣告覆寫臂族」交集內
（全 corpus 該交集 71 檔，其餘 48 檔因標點落在 CJK run、格內混排保守退回、或窄化後
斷點未跨越邊界而不變，皆屬預期）。

**2 檔需單獨交代（源檔全無字面標點，是清單符號）**：
`corpus/poi/test-data_document_delins.docx`、`corpus/ooxmlexport/table-row-data-displayed-twice.docx`。

- 現象：我方清單行首「`       • `」的 • 由**編號層合成**（源檔 `document.xml`／
  `numbering.xml` 查無 U+2022 字面，scratch `probe_entity_cp.py` 亦無實體形式），
  卡 C 把 U+2022 由 1.0 em 收窄 ⇒ 同行後續 item 的起點 x 由 **267.79pt 左移至 260.27pt**
  （實測 Δ＝**7.52pt**@12pt，`bullet_delta.py`；兩側該 item 的 text／size／family／y
  完全相同，只有 x 變）。旁證：子集 U+2022 advance 實測 12.000pt@12pt＝1.000 em，
  真 `times.ttf` hmtx 的 U+2022＝0.3501 em。**不對 Δ 做逐字元瓜分宣稱**（該段還含
  7 個空白與水平尺，未做反事實重排，故只記實測總差 7.52pt）。
- LO 真值側（Linux LO 25.8，定錨口徑）：該清單符號 LO 渲成 **OpenSymbol** 的
  `\uf0b7`（Symbol 字型清單符，非 U+2022），**size 10.00**（編號層自帶字級，正文
  同頁是 12.00）、寬 **4.11pt**（`opensym_em.py`／`bullet_size_fair.py`）。
  **以絕對 advance 對照（斷行吃的是絕對寬，em 不可跨字級直比）**：
  我方卡 C 後 U+2022 按真 `times.ttf` hmtx 0.3501 em × 12pt ＝ **4.201pt**
  vs LO **4.11pt** ⇒ 差 **0.091pt**（卡 C 前我方是 12.000pt，差 **7.890pt**）。
  ⇒ 卡 C 把這一類污染的誤差由 7.89pt 壓到 0.09pt，**LO 也把它當窄符點、不是 em 方塊**。
- 判準（不靠我方自身 dump）：把我方行序列對 LO 行序列做逐行全等計數
  （`bullet_verdict2.py`）：delins p0 LO 36 行／base 命中 **6**／head 命中 **11**
  ⇒ **head 更接近 LO**；table-row-data-displayed-twice p0/p1 命中 base 8/7 與 head 8/7
  ⇒ 持平。⇒ 這二檔是**朝真值收斂**，非回退。
- 殘留欠債（**不屬本卡切片，另立卡**）：LO 對編號層清單符渲染的是 Symbol／OpenSymbol
  窄符點，且吃**編號層自帶字級 10pt**；我方現發 U+2022 ＋ 正文宣告族 hmtx（TNR
  0.3501 em × 12pt），絕對寬已對到 0.091pt 內，但**字面值（U+2022 vs U+F0B7）、face
  （正文族 vs OpenSymbol）與字級來源（正文 12pt vs 編號層 10pt）三項皆不同**——
  正解是編號層符號的 face／char／size 映射（`w:numFmt val="bullet"` ＋ `w:sym
  w:font="Symbol" w:char="F0B7"` 的語義），不是再調 `punct_advance` 表（動它會把
  正文裡的真 U+2022 一起改壞）。

### 7 檔「僅行/頁邊界位移」的定性

`multiset_check.py` 標出 7 檔有序字元串不等但**字元多重集全等**（tdf104061／
Sample_11_ReadWord2007／reader-styles／reader／bug59058／FDO77715／2129393649）。
逐條 `explain_textdiff.py`：差異全部是**詞從上一行多塞一個/少塞一個**（貪婪斷行邊界
位移一格）與**段落跨頁點位移一行**，無一處字元增減。屬卡 C 收窄量寬的預期連動，
非文字遺漏。（`text_conservation.py` 初版把「同 y-bucket 內 run 拼接」的空白差也算成
增減，`multiset_check.py` 的去空白口徑才是正解——記帳防日後誤用。）

## 6. 給 PM 的下一步建議（修復卡，本卡不動）

1. **編號層清單符號 face／char 映射**（§5 殘留欠債）：`w:numFmt w:val="bullet"` ＋
   `w:sym w:font="Symbol" w:char="F0B7"` 應發 Symbol 系窄符點（LO 實渲 OpenSymbol
   0.411 em），非 U+2022 全形。標的檔：delins／table-row-data-displayed-twice／
   及全 corpus 宣告 bullet 編號的檔。**不要**動 `punct_advance.rs`。
2. **卡 E（t_0e390551）起手的自證閘門**：卡面「LO 折疊雙空格」已被 rawdict 推翻，
   且本卡已把 ’ 的 8.25pt 汙染項換成定錨態 6.328pt——Team 段殘留 3 行斷點差的歸因
   必須在含本卡的基線上重測，勿沿用舊尺帶調參。
3. `face_advance_scale` 檔頭 TODO 已改寫為實測記帳，不需再開卡追。

## 7. 本卡未做（誠實記帳）

- 未動 `face_advance_scale`／`face_line_factor` 數值（連動重掃結論＝臂值不動，§4）。
- 未動內嵌子集 otf 與 `scripts/build_font_subset.sh`（§0#1 反轉）。
- 頁首尾表（hf 路徑）不接標點臂，比照 K7-cal 水平尺在 hf 的零擾動口徑（`lib.rs`
  二處 `&[]` 註解記帳），屬後續卡。
- LO 頂替 face 的全庫 VCL 走查仍未做（承 `e1c-advance-anatomy.md` §5 欠債）。
