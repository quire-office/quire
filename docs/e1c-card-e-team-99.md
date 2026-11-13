# E1-C 卡 E：cell-grid-span Team 段斷點 6/9→9/9＝卡 C 連動根治（實測記帳，零引擎改動）

- 測試檔：`corpus/ooxmlexport/cell-grid-span.docx`（Team 段宣告 Trebuchet MS）
- 分支：`wt/t_0e390551`，基線＝**含卡 C 修復的 ef14bc9**（PM 值守 11-10 裁定發車順序）
- 真值口徑：`docs/e1cb-lo-truth-canon.md`（Linux LO 25.8 主力＋per-process `FONTCONFIG_FILE`
  注入真 `trebuc*.ttf`，pymupdf **rawdict** 逐字量）；本 run 全部實跑，無一筆沿用
- 本卡**引擎零改動**（`git diff` 僅 test 檔門檻＋註解＋docs）——PM 自證閘門「連動收案」分支

## 0. 結論先行（PM 先讀這段）

**卡面殘留的「3 行斷點差一格（行 0/1/2 位移）」已被卡 C（t_900f3b25，標點覆寫臂）連動根治。**

1. **起手自證閘門（PM 寫死，本卡第一步）**：在含卡 C 的 ef14bc9 上重跑 Team 段斷行量測
   （rawdict 真值口徑）＝ **9/9 逐行全等**。判定分支走「連動收案」，不動引擎、不調參。
   雙口徑互證：對考卷常數 `TRUTH_TEAM_LINES` 9/9；對本 run **重新實轉**的定錨 LO PDF
   （rawdict 逐字重組，含空格字形）亦 9/9，且 LO 行 4 如實含兩處連續雙空格（LO 不折疊，
   與 `docs/e1cd-xml-space-policy.md` 一致）。
2. **三段歸因坐實根因＝標點覆寫臂，不是水平尺／對齊／段寬**（§2）：
   - base＝279c1ee（卡 D 態，無標點臂）：行 0 斷在 `…which has`，`helped` 掉進行 1 ⇒ **6/9**
     （行 0/1/2 各錯一位。卡 A 時代 docs 記「我方行 0 多吃 helped」與本 run base 實測方向
     相反，屬卡 A→卡 D 間節點緩衝裁剪的邊界位移，本卡不追（兩態皆 6/9、本卡切片只關
     「臂翻轉」）；此處以本 run 實測方向為準）
   - head＝ef14bc9（含卡 C）：**9/9**
   - 同碼拔 Trebuchet 標點臂（僅 `punct_arm_for_face` 的 trebuchet 臂改回
     `PUNCT_ARM_NONE`，水平尺 0.9744／垂直尺 1.1611／段寬 468pt／對齊全部原樣）：**退回 6/9**
   - ⇒ 唯一變量＝標點覆寫臂。候選方向 2（justification／tab stop）與 3（段寬／可行帶）
     無需再走：拔臂態與 base 態錯位逐行相同，幾何在兩態間未變。
3. **翻轉詞元坐實＝行 0 首個覆寫碼點 U+2026**（`Innovation…`）：內嵌子集全形 10.000pt@10pt
   → 真 `trebuc.ttf` hmtx 7.344pt（臂取代不疊乘 ⇒ 行 0 淨縮 **2.400pt**）。逐行覆寫碼點
   實掃（§2 表）：Team 段僅行 0（…×1）與行 4（’×1）含覆寫碼點；行 4 的 ’ 縮 6.328pt 不翻
   斷行（行 4 兩態斷點本就一致），**行 0 的 … 縮 2.400pt 是唯一翻轉源**。
   需求帶旁證（pymupdf 裸 hmtx，非引擎算式）：行 0 自然寬 477.05→461.50pt、
   「行 0＋write」503.25→488.11pt，版心 468pt——收窄把行 0 從「虛胖到裝不下真值行」
   拉回真 face 自然寬。**不對翻轉做逐字元瓜分宣稱**：引擎端含 kerning／空格細項，
   裸 hmtx 模型復現不了 base 態「helped 放不進」的完整算式（比照卡 C §5「不瓜分」紀律）。
4. **連帶抓出一個假防護**：燈 2 門檻 `matched ≥ 6`＋穩定群行 3..8，在真值已是 9/9 的
   基線上＝「退回 3 行（行 0/1/2 再位移）也不紅」。卡 C 記帳「燈 2 實跑仍 6/9」是
   **沿用卡 A 時期舊測未重跑**（≥6 綠態不輸出實得值，錯值未暴露）。本卡把燈 2 釘死
   `assert_eq!(matched, 9)`＋穩定群擴為行 0..8：拔臂態必紅（6≠9）、尺退 1.0 必紅（0≠9），
   鑑別力反升。此為**考卷門檻校正**，非引擎行為改動。

## 1. 起手自證閘門實跑（PM 指令逐字執行）

```bash
# 工作樹＝ef14bc9（含卡 C），CARGO_TARGET_DIR 獨立
cargo test -p quire-engine --test e1c_advance_trebuchet
# → 3 passed; 0 failed（燈 1 頁數/行數/pitch、燈 2 斷行、燈 3 文字守恆）
```

scratch 側同口徑復算（`selfcheck_breaks.py`，y-bucket 0.5pt 聚行＝考卷同口徑）：

| binary | Team 段行數 | 對 LO 真值逐行全等 |
|---|---|---|
| ef14bc9 debug（含卡 C） | 9 | **9/9** |
| ef14bc9 release（同碼，opt 級別對照） | 9 | **9/9** |
| base-wt debug＝279c1ee（卡 D 態） | 9 | **6/9**（行 0/1/2 位移一格，`helped` 歸行 1） |
| ef14bc9＋拔 Trebuchet 標點臂（探針，已 revert） | 9 | **6/9**（錯位與 base 逐行相同） |

獨立 LO 復轉（`verify_vs_lo.py`：per-process `FONTCONFIG_FILE` 注入 `trebuc*.ttf`，
`fc-match "Trebuchet MS"` → `trebuc.ttf` 坐實；rawdict 逐字重組）：LO 2 頁、Team 段
9 行、span font `TrebuchetMS`、**我方對 LO rawdict 9/9**。LO 行 4 rawdict 如實含
`(Tablet PC),␣␣during` 連續雙空格——再次否證「LO 折疊」說（卡 D 覆核結論穩定復現）。

## 2. 翻轉機制的逐行實測（候選方向 1 的「先量」）

覆寫碼點掃描（真 trebuc hmtx vs 內嵌子集，@10pt，`probe_mechanism.py`）：

| 行 | 覆寫碼點 | 子集 | 真 face | 收窄 |
|---|---|---|---|---|
| 行 0 | U+2026 ×1（`Innovation…`） | 10.000pt | 7.344pt | **2.656pt**（×0.9744 尺＝淨 2.400pt） |
| 行 4 | U+2019 ×1（`iBall’s`） | 10.000pt | 3.672pt | 6.328pt（尺不疊乘臂 ⇒ 淨 6.328pt） |
| 行 1/2/3/5/6/7/8 | 無 | — | — | — |

貪婪需求帶（版心 468pt；natural＝行自然寬，with_next＝再加下一行首詞，>468 才正確斷）：

| 行 | natural 子集態 | natural trebuc 態 | with_next 子集態 | with_next trebuc 態 |
|---|---|---|---|---|
| 0 | 477.05 | 461.50 | 503.25（+write） | 488.11 |
| 1 | 464.08 | 457.73 | 486.93（+with） | 480.47 |
| 2 | 440.07 | 428.81 | 493.68（+categories.） | 481.74 |

判讀：行 0 在子集原態度下自然寬 477.05 **超出**版心 468——我方量寬虛胖，貪婪放進
`which has` 後已無空間給真值行 0 的收尾詞（錯位起點）；臂收窄後 461.50 ≤ 468，
行 0 按真 face 自然斷行收 `helped`，行 1/2 的位移一格隨鏈式消解。行 4 的 ’ 收窄
6.328pt 不跨斷行邊界（natural 477.51→459.31 遠低於翻轉點，兩態斷點一致）⇒
**翻轉源唯一＝行 0 U+2026**。

## 3. 候選方向逐條判定（卡面 1→2→3，先量後改）

| 卡面候選 | 判定 | 證據 |
|---|---|---|
| 1. Trebuchet advance 尺字組寬差 | **不需要動尺**：分歧詞元＝U+2026（全形標點），根因是標點 advance 非拉丁字組寬；0.9744 在卡 C 重掃帶 [0.9712,0.9851) 內（卡 C §4），尺值不動 | §2 表＋卡 C 記帳 |
| 2. justification／tab stop 對段首幾何 | **排除**：拔臂態段首 x=72.000 與 head 態逐 item 相同（`probe_x_delta.py`），幾何未隨臂變；錯位態與 base 完全同構 | §0#2 |
| 3. 段寬／可行帶（sec_margin_w、內縮） | **排除**：同一支 468pt 段寬下 base 6/9、head 9/9、拔臂 6/9——段寬常量在三支 binary 間從未變動 | §1 表 |

## 4. 本卡改動清單（零引擎行為）

1. `core/engine-api/tests/e1c_advance_trebuchet.rs`：燈 2 門檻 `≥6`→`assert_eq 9/9`、
   穩定群行 3..8→行 0..8，檔頭記帳改寫（舊「不斷言 9/9」節更版為連動根治記帳）。
2. `core/engine-api/src/lib.rs`：`face_advance_scale` 檔頭 Trebuchet 臂註解——卡 C 寫
   「燈 2 實跑仍 6/9」更正為實測 9/9＋門檻釘死記帳（純註解，`git diff` 無行為行）。
3. `docs/e1c-card-c-subset-punct.md`：鑑別力表與 §4 的「燈 2 仍 6/9」兩處就地更正。
4. `docs/e1c-card-e-team-99.md`（本檔）。

## 5. 零回退與閘門（本 run 實跑）

| 閘門 | 結果 |
|---|---|
| `cargo test -p quire-engine --test e1c_advance_trebuchet`（最終態，9/9 新門檻） | 3 passed 0 failed |
| `K7_ADVANCE_SCALE_BUGGY=1`（尺退 1.0） | 燈 2 **必紅**（0/9≠9） |
| 拔臂探針（同碼，唯一變量臂） | 燈 2 **必紅**（6/9≠9）＝新門檻對「臂失效」的鑑別力 |
| `cargo test --workspace`（基線 ef14bc9 原態） | 925 passed 0 failed（`ws_test_pre.log`） |
| e1c_advance_trebuchet 三盞／d9_frame_overlap 三盞 | 全綠（同上 log） |
| 引擎 diff | 零（僅 test/註解/docs）⇒ A/B 與 400 檔掃描由卡 C 雙 binary 實證覆蓋，本卡不重複 |

## 6. 給 PM 的記帳

- 卡 D 卡面（279c1ee）「雙空格折疊＝6/9→9/9 臨門一腳」與本卡卡面「殘留 3 行」共用同一
  個已被 rawdict 否證的前提鏈：折疊說死於抽取器假象（卡 D 覆核），而真正的臨門一腳
  是卡 C 的行 0 … 收窄 2.400pt。本卡以實測把兩條線收斂。
- 燈 2 舊門檻 ≥6 的教訓（「綠態不輸出實得值 ⇒ 舊測錯值無法暴露」）已寫進考卷註解；
  日後類似「實測 X/N」記帳若未重跑，務必標明口徑基線，防止跨卡繼承錯值。
- `render_lab` 觸發集本卡未重跑：引擎零改動（`git diff --stat` 佐證），行為輸出與
  卡 C 收案態逐位元組同源的 binary（同一支 `lib.rs` 編譯產物，僅註解差異不進 codegen）。

## 7. 重現指令（本 run 全部實跑）

```bash
S=<hermes>/kanban/boards/quire/workspaces/t_0e390551
SCRATCH=$S /usr/bin/python3 $S/selfcheck_breaks.py     # 我方斷行 vs 考卷常數（--CLI 可換 binary）
SCRATCH=$S /usr/bin/python3 $S/verify_vs_lo.py          # 定錨 LO 重轉 + rawdict 9/9 復驗
SCRATCH=$S /usr/bin/python3 $S/probe_mechanism.py       # 逐行覆寫碼點 + 需求帶
SCRATCH=$S /usr/bin/python3 $S/probe_x_delta.py         # 段首/逐 item 幾何不隨臂變
```
