# 度量口徑變更：逐頁尺寸注記改「走向拓撲錨定」（bug65649 衍生，t_026e8c2e）

> 變更點僅 `scripts/render_lab.py` 的注記臂（`annotate_page_sizes`）；內容／版面
> 相似度、`first_divergence`、`measurement_artifacts`、頁數一致率**零改動**。
> `scripts/lo_scoreboard.py` 經 `RL.compare()` 吃新口徑，自動生效，本卡零觸碰。
> 背景診斷（三假說證偽、Runs 拓撲對照表）見 `docs/bug65649-landscape-diag.md`。

## 一、舊口徑為什麼錯

舊逐頁尺寸注記是**同位索引**比對：`lo_pages[i]` vs `brks[i]`，寬或高差 > 2.0 pt
就記「第 i+1 頁 LO x×y ≠ 我方 a×b（sectPr）」。這個口徑暗含「兩側第 i 頁是同一段
正文」的假定。一旦兩側**總頁數有位移**（分頁容量漂移，與 section 判定無關），
同位索引就錯位——bug65649 實測 LO 178 頁 vs 我方 190 頁、兩側走向拓撲 100% 全等
（`P-L-P-L-P-L-P-L-P` 九段），卻被灌進 **22 筆假 sectPr 錯位**（13 筆「LO 橫向我方
直向」＋9 筆反向，全部落在 Runs 邊界的相位差區間內），掩蓋真缺陷、污染最差榜判讀。

## 二、新口徑（兩段式）

`compare()` 收尾改調 `annotate_page_sizes(row, lo_pages, breaks)`：

1. **抽拓撲**：兩側逐頁尺寸序列各抽「連續走向段（orientation runs）」——相鄰同
   走向頁合併，得 P/L 交替序列（長寬同於 2.0 pt 容差內的方頁記 `S`）。走向由
   `page_orient` 判：寬−高 > 2.0 pt ⇒ `L`；高−寬 > 2.0 pt ⇒ `P`；否則 `S`。
2. **拓撲全等**（run 數與走向序列逐位相同）⇒ 尺寸注記改 **run 錨定**：
   - 第 k 段對第 k 段，段內比「去重尺寸集合」，等價判定的容差與舊臂同尺
     （2.0 pt；twip→pt 有 0.1 pt 級捨入差，LO `841.9×595.3` vs 我方
     `842.0×595.4` 是同一個 A4 橫向，**不是**幾何缺陷——觸發集實跑抓到此態）；
   - 兩側尺寸集合互相無容差內夥伴 ⇒ 記
     「第 k 段（直向／橫向）LO … ≠ 我方 …（sectPr，run 錨定）」——這是
     **真**幾何宣告差，照報；
   - 單側段內混雜超出容差的多種尺寸（同段多宣告）⇒ 另記
     「第 k 段（走向）LO／我方 段內混尺：…（同段多宣告，如實記錄）」；
   - 段內頁數差 ⇒ 記「第 k 段（走向）容量漂移 +n 頁（LO x 頁／我方 y 頁）」；
   - 兩側總頁數差 ⇒ 記一筆總注記「走向拓撲全等（n 段 …）；總頁數差 ±k 頁＝
     分頁容量漂移（非 sectPr），尺寸注記改 run 錨定」。
   - **不再逐同位索引報 sectPr 錯位。**
3. **拓撲不全等**（任一側無頁也走這條）⇒ **照舊**逐同位索引報（舊口徑原樣，
   含「第 i 頁我方無 pagebreak 記錄」），且首記「走向拓撲不一致：LO=… 我方=…」。
   這是真 section 缺陷的訊號，不可掩蓋（防放水臂，selftest 9c/9d 鎖死）。

## 三、baseline 解讀方式改變（儀表板判讀慣例）

- **日後 baseline 的「（sectPr）」注記只剩真拓撲／真幾何宣告缺陷**：
  - 「（sectPr）」＝拓撲不一致檔的同位錯位（真 section 缺陷訊號）；
  - 「（sectPr，run 錨定）」＝拓撲全等但該段宣告尺寸全異（真幾何差）；
  - 兩者是**值得開卡修**的物件。
- 「容量漂移 +n 頁」「走向拓撲全等…總頁數差…」是**分頁容量**注記（表格列高／
  折行量測漂移），不是幾何錯誤；收斂方向是排版容量（`core/layout`），不是
  section 判定。判讀最差榜時別再把它們當成 section 缺陷。
- **歷史 baseline（本次切換之前的 scores.jsonl）不可直接對照**：舊檔的 sectPr
  注記摻假錯位（bug65649 單檔 22 筆），新檔剝掉。跨口徑 A/B 只看分數欄位
  （內容／版面分數本口徑變更前後**逐位相同**，觸發集與 seed 樣板實跑已證）；
  注記欄位只能同口徑內比。下次重掃 baseline 起，注記欄位才開始累積新語義。
- 翻盤觸發集（哪些檔注記變了）以實跑為準，見 `docs/bug65649-flip-table.md`
  （`scripts/run_anchor_flip_report.py` 可重跑，產物另落
  `out/qa-runanchor-flip/flip-report.md`＋`flip.json`）——
  「分數沒動」與「注記全變」兩個極端宣稱都不實。
- 本次實跑結論（2026-11-11）：觸發集 10 檔中注記翻盤 **2 檔**——
  bug65649（22 筆假 sectPr → 0 筆＋6 筆容量漂移類注記）與 fdo78882
  （LO 3 頁 vs 我方 2 頁、拓撲 PLP≠PL ⇒ 新增「走向拓撲不一致」注記，
  屬真拓撲訊號浮出）；其餘 8 檔新舊注記逐位相同。seed=20261022 樣板
  25 檔不含翻盤檔（bug65649/fdo78882 均不在樣板內），該樣板分數與注記
  欄位前後逐位相同。

## 四、驗證鎖

- `python3 scripts/render_lab.py --selftest`：新臂 9a–9g（同拓撲＋頁數差 k ⇒
  零假 sectPr、報容量漂移；拓撲不同 ⇒ 照報；run 錨定不掩真幾何差；零 pagebreak
  退化態；orientation_runs 單元；0.1pt 捨入差不判錯）。
- `python3 scripts/tests/test_run_anchor_metrics.py`：獨立鎖——注記臂輸出形態、
  分數欄位隔離（只改 breaks 尺寸分數逐位不動）、bug65649 的 22 筆假注記
  在新口徑下全數收斂為容量漂移注記（按診斷報告 §三 Runs 表合成重放，
  不碰 LO/引擎；舊臂逐字重現版同檔對照，先證舊臂確實 reproduce 22 筆）。
- `bash scripts/check-layering.sh`：零 Rust 改動仍自證。
- 完整 seed 樣板（20261022、n=25）前後對照：scores 分數逐位相同、注記臂翻盤檔
  逐檔列出。產物落 `out/qa-runanchor-before|after/`，不覆寫 `out/lo-scoreboard`
  基線（雙目錄鐵律）。
