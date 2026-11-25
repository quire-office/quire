# save-roundtrip-baseline（儲存保真閘門基線）

`scripts/save_roundtrip_lab.py` 的正式基線產物（用戶 2026-11-25 指定）。
三臂定義、判據與口徑見腳本檔頭註解；重跑：

```bash
# LO 基線：Windows soffice.exe 優先（本機查無 → 自動落 Linux LO 25.8，
# 正式口徑見 quire-development skill；報告檔頭自動標記實際用哪個）
export PATH="/mnt/c/Program Files/LibreOffice/program:$PATH"            # Windows 機
export PATH="$HOME/.local/lo-linux/opt/libreoffice25.8/program:$PATH"    # 本機
python3 scripts/save_roundtrip_lab.py --out tests/save-roundtrip-baseline   # 全量 42 檔 nightly
python3 scripts/save_roundtrip_lab.py --quick --out out/srt-quick           # 10 檔合併閘門候選
SAVE_RT_BUGGY=1 python3 scripts/save_roundtrip_lab.py --discriminate \
    --files selfgen/cjk_japanese_ruby.docx ooxmlexport/fdo78883.docx \
    poi/test-data_document_table_footnotes.docx xopt/TestFiles_RP_RP051-Arabic.docx \
    --out /tmp/srt-disc --report tests/save-roundtrip-baseline/discrimination.md  # 回塞鎖鑑別力
```

## 入庫／不入庫
- 入庫：`report.md`（人讀報告，`| corpus…` 行與 `scripts/sweep_cmp.py` 兼容，
  跨版本同 seed 重跑可逐頁比對）、`sample.json`（取樣清單＋seed=20261125，
  跨版本同 seed 重跑才可比）、`scores.jsonl`（機器讀逐臂記錄）、
  `discrimination.md`（回塞鎖鑑別力實錄）。
- 不入庫：`saved/`（存檔臂 DOCX）、`orig/`、`saved-quire/`（SVG 渲染產物）——
  可由 corpus＋seed 完整重生成，體積 70 MB 級，落 `.gitignore`。

## 判據（紅線，不得鬆）
- 臂 A（零編輯）：預閘＝**位元組恆等**（sha256 對源檔）；不恆等即紅。
  紅檔照跑 LO 逐頁**像素 diff 計數**（容差 0）＋逐頁文字差分鑑別性質——
  不用相似度分（skill 教訓：相似度分對局部缺線不敏感）。恆等過者像素層
  跳過但必須記錄（`skipped（位元組恆等…）`）。
- 臂 B（編輯態）：編輯單調不縮內容 ⇒ 突變段所在 LO 頁**之前**逐像素零位移
  （防「存檔動到別處」）＋突變區內可見（防「存檔吞編輯」）＋內容層
  flag 級差分（bold/align 以 `<w:b>` 淨增＋既有粗體 run 不被吞判；PM 裁決
  11-25 取代舊錨點字串判據；真寫入綠／未寫入紅／抹樣式紅三態實證）。
- 臂 C（本專案渲染）：存檔後檔餵既有 render_lab 流程，突變區外逐頁內容分
  ≥ 原檔 −0.002；`| corpus…` 行 sweep_cmp 兼容。
- 回塞鎖：`SAVE_RT_BUGGY=1`＝存檔鏈故意多做一件對渲染無害的 zip part
  重排（末位 entry 搬到前排）；`--discriminate` 要求零編輯臂必紅，
  未紅＝exit 1（比照 U4F_*_BUGGY 口徑）。

## 本基線跑況（2026-11-25 run 87）
- 環境：本機 LibreOffice 25.8（`~/.local/lo-linux/…/program/soffice`，
  PM 裁決 11-25：本機查無 Windows soffice.exe，接受 Linux LO 為本專案
  正式口徑；Windows 機重跑自動生效）＋`target/debug/quire-cli`。
- 臂 A（零編輯）：42/42 位元組恆等 PASS（roundtrip corpus 399/399 同步
  零倒退）。
- 臂 B/C（編輯態）：紅燈＝引擎突變層缺陷（SplitParagraph 對含
  `w:hyperlink`／`w:ins`／`w:fldSimple` 段落產出 ill-formed XML），
  **非存檔鏈倒退**，PM 裁決不歸本卡修——report.md「引擎突變層缺陷實錄」
  章節帶最小重現命令＋原檔目標段 XML＋跨切點容器開閉計數（返修卡線索）。
- 回塞鎖：`discrimination.md`＝紅 4/4 檔點名＋像素層零位移（重排對渲染
  無害的實據）＋編輯態臂零紅（重排不吞編輯）。
- 效能基線（run 87 修復後）：bug65649（12.5MB document.xml、15934 段、
  LO 180 頁）paragraphs 27.6s、零編輯存檔 20.9s、LO 轉換 9.4s——
  `paragraph_cp_lens` 單趟批次口徑修復前該檔在考卷 180s 探針必逾時。
