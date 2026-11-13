# K29e 紅燈記錄：字元格式六欄（底線／刪除線／上下標／highlight／字型族）

執行者：ow-core（t_4b55fc9d）。考卷：`core/engine-api/tests/k29e_char_format.rs`。
收案報告（全閘門實跑與 A/B 歸因）：`docs/k29e-slice5-report.md`。

## §1 卡面前提更正

卡面寫「上下標會改行高，屬改變正文流的合法位移」——LO 25.8 探針實測推翻「撐高」半句：
含 sup/sub 的行 auto 行距 pitch 恆 12.65pt，不膨脹。位移只發生在 sub run 自身的
字級與基線 y（正文流行高不動）。goldens 三檔零位移與全庫 A/B 歸因（見 slice5 報告）
皆與此一致。請 PM 更正卡面措辭。

## §2 LO 真值（2026-11-13，Linux LO 25.8，pymupdf rawdict 探針）

探針檔：`scratch/gen_k29e_probe.py` 生成四段（11pt/16pt 兩組，含純基線段對照），
`soffice --headless --convert-to pdf` 後以 `k29e_lo_measure.py` 讀 rawdict span 逐字量。

| 量 | LO 實測 | 實作採用 |
|---|---|---|
| sup/sub 字級 | 0.582×base（11pt→6.4pt；16pt→9.3pt） | 0.58×base（考卷容差 0.01） |
| sup 基線位移 | 上移（bbox 頂縮、基線上移約 0.39×base 含字級縮尺效應） | −0.35×base（沿用既有上標常數，零回退） |
| sub 基線位移 | 下移（基線在下，+0.332×base 含縮尺後淨位移 +1.0pt@11pt） | +base×(1.0/11.0)（=11pt 時 +1.0pt） |
| 行高（auto） | pitch 恆 12.65pt（11pt 行，含 sup/sub 與否同值） | layout 行高不動 |

大字號（16pt）分支同測：pitch 仍不膨脹，位移比例線性於字級——故 sub 位移以
字級比例式實作（+1/11×base），非固定 1pt 常數。

## §3 紅燈實跑（90 分鐘硬檢查點）

初版考卷 lamp 3／4／7 紅，逐盞歸因（臨時探針 `k29e_probe_tmp.rs` dump 顯示列表＋XML，
用畢即刪）：

1. lamp 3（底線五形態＋雙線）：docDefaults 底線使 SVG `text-decoration` 合成
   `underline line-through`，精確子串斷言過脆 → 改鎖目標段 `<text>` 行含 `line-through`
   子串。修後綠。
2. lamp 4（上下標基線）：跨段比 y 無效（各段行 y 不同）→ 改同段「編輯態 vs 未編輯同段」
   基準差。修後綠，BUGGY 態（不縮尺不位移）必紅證明鑑別力。
3. lamp 7（字型族度量）：全 CJK 段 advance 皆方形，Latin advance 尺差不可觀察 →
   改鎖 real line-height 因子收斂（無宣告 15.4pt → Arial 12.6pt，後續段基線上移
   約 2.8pt），為「宣告族被度量層消費」的行為級事實斷言。修後綠。

BUGGY 回塞（`scripts/k29e_buggy_replay.sh buggy`）七盞全紅；good 態還原後
`git diff` 對照 HEAD 逐位元組一致＋15 passed。輸出全文見 slice5 報告。

## §4 給 PM 的下一步建議

- 卡面「上下標改行高」措辭更正（§1）。
- highlight 移除第四態（`w:highlight val="none"` 解析為未宣告）如需真三態，另開解析卡，
  本卡燈 6 照實斷既有口徑。
- 22 顆 `pending:K29e` 鈕轉 live 的資格判定屬誠實性卡：六欄 patch 欄位已逐欄真實存在
  （mutation.rs 141–173 行）＋發射閉環（考卷七燈），可據此逐鈕驗收。
