#!/usr/bin/env bash
# oc-pbb-fix BUGGY 回塞／還原腳本（持久版，勿只放 24h 快取）。
#
# 用法：
#   bash scripts/pbb_buggy_replay.sh buggy   # 把分頁旗標改回 parser 直讀，考卷必紅
#   bash scripts/pbb_buggy_replay.sh clean   # 從乾淨副本還原，考卷必綠
#
# 乾淨副本：scripts/.pbb_lib_clean.rs（buggy 執行前由 git HEAD 落地；
# 不依賴 git checkout，符合本卡禁 checkout 紀律）。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=16777216

FILE="core/engine-api/src/lib.rs"
CLEAN_COPY="scripts/.pbb_lib_clean.rs"

clean_line='        let grid_break_v: Vec<bool> = eff_all.iter().map(|p| p.page_break).collect();'
buggy_line='        let grid_break_v: Vec<bool> = doc.paragraphs.iter().map(|p| p.page_break).collect();'

case "${1:-}" in
  buggy)
    git show "HEAD:${FILE}" > "$CLEAN_COPY"
    python3 - "$FILE" "$clean_line" "$buggy_line" <<'PY'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
text = open(path, encoding="utf-8").read()
if old not in text:
    sys.exit("回塞失敗：找不到級聯合成行")
open(path, "w", encoding="utf-8").write(text.replace(old, new, 1))
print("已回塞：grid_break_v 改回 p.page_break 直讀")
PY
    ;;
  clean)
    if [[ -f "$CLEAN_COPY" ]]; then
      cp "$CLEAN_COPY" "$FILE"
    else
      git show "HEAD:${FILE}" > "$FILE"
    fi
    echo "已還原：grid_break_v 回到級聯投影"
    ;;
  *)
    echo "用法: $0 buggy|clean" >&2
    exit 2
    ;;
esac

cargo test -p quire-engine --test t_f81492_style_page_break
