# save-roundtrip-baseline（儲存保真閘門基線）

`scripts/save_roundtrip_lab.py` 的正式基線產物（用戶 2026-11-25 指定）。
三臂定義、判據與口徑見腳本檔頭註解；重跑：

```bash
# LO 基線：Windows soffice.exe 優先进 PATH（本機無則自動落 Linux LO）
export PATH="/mnt/c/Program Files/LibreOffice/program:$PATH"
python3 scripts/save_roundtrip_lab.py --out tests/save-roundtrip-baseline   # 全量 42 檔 nightly
python3 scripts/save_roundtrip_lab.py --quick --out out/srt-quick           # 10 檔合併閘門候選
SAVE_RT_BUGGY=1 python3 scripts/save_roundtrip_lab.py --discriminate \
    --files selfgen/cjk_japanese_ruby.docx --out /tmp/srt-disc              # 回塞鎖鑑別力
```

## 入庫／不入庫
- 入庫：`report.md`（人讀報告，sweep_cmp 兼容行）、`sample.json`（取樣清單＋seed，
  跨版本同 seed 重跑才可比）、`scores.jsonl`（機器讀逐臂記錄）。
- 不入庫：`saved/`（存檔臂 DOCX）、`orig/`、`saved-quire/`（SVG 渲染產物）——
  可由 corpus＋seed 完整重生成，體積 70 MB 級，落 `.gitignore`。

## 本基線跑況（2026-11-25）
- 環境：本機 LibreOffice 25.8（`~/.local/lo-linux`）＋`target/debug/quire-cli`。
  Windows `soffice.exe` 口徑在 Windows 執行機上重跑時自動生效。
- 臂 A（零編輯）：42/42 位元組恆等 PASS（roundtrip corpus 399/399 同步零倒退）。
- 臂 B/C（編輯態）：紅燈＝引擎突變層缺陷（SplitParagraph 對含 `w:hyperlink`
  段落插入第二個 `w:pPr` 產出 ill-formed XML 等），非存檔鏈倒退；逐檔原因見
  report.md「原因」欄。
- 回塞鎖：`SAVE_RT_BUGGY=1 --discriminate` 下零編輯臂必紅（zip part 重排），
  已實證 DISCRIMINATION=OK。
