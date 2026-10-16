#!/usr/bin/env bash
# 一鍵重建 corpus：抓公開來源 → 自產補齊 → 探測/去重 → 挑選 → 驗證。
# usage: bash scripts/corpus/collect.sh [workdir]
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
WORK="${1:-$(mktemp -d /tmp/quire-corpus-XXXX)}"
mkdir -p "$WORK"
cd "$WORK"

echo "== 1/6 清單 =="
gh api "repos/LibreOffice/core/contents/sw/qa/extras/ooxmlimport/data" --paginate \
  --jq '.[]|select(.name|endswith(".docx"))|[.name,.size]|@tsv' > lo_import.tsv
gh api "repos/LibreOffice/core/contents/sw/qa/extras/ooxmlexport/data" --paginate \
  --jq '.[]|select(.name|endswith(".docx"))|[.name,.size]|@tsv' > lo_export.tsv

echo "== 2/6 下載 =="
python3 "$HERE/fetch_lo.py" "$WORK"
python3 "$HERE/fetch_extra.py" "$WORK"

echo "== 3/6 自產 =="
python3 "$HERE/gen_selfgen.py" "$WORK" >/dev/null

echo "== 4/6 探測 =="
python3 "$HERE/probe.py" "$WORK/pool" >/dev/null
python3 "$HERE/check_xml.py" "selfgen__*.docx" >/dev/null

echo "== 5/6 挑選 -> $REPO/corpus =="
python3 "$HERE/select.py" "$WORK" "$REPO/corpus" "$WORK/pool"

echo "== 5.5/6 PII 消毒（引擎無損 set_part，quarantine 檔跳過）=="
python3 "$REPO/scripts/scrub_corpus.py"

echo "== 6/6 驗證 =="
python3 "$REPO/scripts/verify_corpus.py" "$REPO/corpus"
