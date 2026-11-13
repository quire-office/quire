#!/bin/sh
# F1 零回退鐵條實抓：SVG byte 級 A/B——正常態 vs F1_CHART_BUGGY=1 回塞態
# （＝本卡前的舊行為）。只有含圖表的檔案允許產生位元組差異。
# 用法：sh docs/f1-survey/svg_ab.sh
set -e
cd <repo>/.worktrees/t_f3acd271
# 清單來自 corpus_survey.py 實抓（docx 是 zip，grep 對 compressed stream 無效）
FILES=$(/usr/bin/python3 -c "
import json
rows = json.load(open('docs/f1-survey/chart_corpus.json'))
print('\n'.join(r['file'] for r in rows if r.get('chart_parts')))
")
mkdir -p /tmp/f1-ab/a /tmp/f1-ab/b
echo "== 含圖表語料（A/B 對象）=="
echo "$FILES"
for f in $FILES; do
  stem=$(basename "$f" .docx)
  cargo run -q -p quire-cli -- render "$f" --out "/tmp/f1-ab/a/$stem" >/dev/null
  F1_CHART_BUGGY=1 cargo run -q -p quire-cli -- render "$f" --out "/tmp/f1-ab/b/$stem" >/dev/null
done
echo "== byte diff（A=新態 B=回塞舊態）=="
for f in $FILES; do
  stem=$(basename "$f" .docx)
  if diff -rq "/tmp/f1-ab/a/$stem" "/tmp/f1-ab/b/$stem" >/dev/null 2>&1; then
    echo "SAME  $stem  ← 含圖表檔卻零差異＝發射未生效，可疑"
  else
    echo "DIFF  $stem"
  fi
done
echo "== 無圖表對照組（必逐位元組相同）=="
for f in corpus/ooxmlexport/fdo76591.docx corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx \
         corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell-Mod.docx corpus/ooxmlimport/n751077.docx \
         corpus/ooxmlexport/99_Fields.docx corpus/ooxmlexport/btlr-textbox.docx \
         corpus/ooxmlexport/cell-grid-span.docx corpus/xopt/NewDocxDocuments_CoverPage.docx; do
  [ -f "$f" ] || continue
  stem=$(basename "$f" .docx)
  cargo run -q -p quire-cli -- render "$f" --out "/tmp/f1-ab/a/$stem" >/dev/null
  F1_CHART_BUGGY=1 cargo run -q -p quire-cli -- render "$f" --out "/tmp/f1-ab/b/$stem" >/dev/null
  if diff -rq "/tmp/f1-ab/a/$stem" "/tmp/f1-ab/b/$stem" >/dev/null 2>&1; then
    echo "SAME  $stem  ← 正確（無圖表零擾動）"
  else
    echo "DIFF  $stem  ← 違規：無圖表檔被圖表發射污染"
  fi
done
