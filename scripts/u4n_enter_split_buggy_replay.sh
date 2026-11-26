#!/usr/bin/env bash
# t_d900332c（U4N）殼層 Enter 分段回塞與還原腳本（支援 buggy 與 good 兩態）。
#
# 用法：
#   bash scripts/u4n_enter_split_buggy_replay.sh buggy  # 回塞態：考卷 E1/E2/E3/E4/E5/E5c/E7b 必紅
#   bash scripts/u4n_enter_split_buggy_replay.sh good   # 正常態：考卷全綠
#
# 回塞鎖口徑（JS 版 k21/g1，比照 U4F_*/U4D_*）：鎖內建在生產碼
# shells/web/demo/main.js 的 u4EnterBuggy()——正常瀏覽器本無 process，事件時求值
# 恆假（零行為擾動）；buggy 態由考卷經 Playwright addInitScript 注入
# window.process.env.U4N_ENTER_SPLIT_BUGGY 驅動，把 Enter／Shift+Enter 打回
# 修前缺陷態（commitText("\n") 段內字面換行、段數不增）。**零改檔、零 checkout**。
#
# 前置（一次性）：
#   cd shells/web && npm run build:wasm
#   cargo build -p quire-cli
#
# 規範守衛：
#   - 嚴禁 git checkout / reset / clean / stash。
#   - buggy 態考卷 exit 必須非 0（紅燈不紅＝回塞鎖無鑑別力）；
#     good 態 exit 必須 0；兩態跑完 git status 不得有本卡白名單外殘留。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
PY="${PYTHON:-$HOME/.local/chrome-control/bin/python}"
PROBE="shells/web/tests/t_shell_enter_paragraph_split.py"

if [ ! -x "$PY" ]; then
  echo "環境錯誤：找不到 playwright python（$PY）；可用 PYTHON=... 覆寫" >&2
  exit 2
fi

case "${1:-}" in
  buggy)
    echo "=== 回塞態（U4N_ENTER_SPLIT_BUGGY=1）：考卷預期必紅 ==="
    set +e
    U4N_ENTER_SPLIT_BUGGY=1 "$PY" "$PROBE"
    RES=$?
    set -e
    if [ "$RES" -eq 0 ]; then
      echo "BUGGY 態考卷竟然全綠＝回塞鎖無鑑別力（考卷失效），視為失敗"
      exit 1
    fi
    echo "BUGGY 態測試結束，exit_code=${RES}（預期非 0）✓"
    ;;

  good)
    echo "=== 正常態：考卷預期全綠 ==="
    "$PY" "$PROBE"
    echo "GOOD 態測試結束，exit_code=0"
    ;;

  *)
    echo "用法: $0 buggy|good" >&2
    exit 2
    ;;
esac

echo "=== 殘留檢查（git status --porcelain -uall，本卡白名單外不得有髒檔）==="
git status --porcelain -uall | grep -v -e "shells/web/demo/main.js" \
  -e "shells/web/tests/t_shell_enter_paragraph_split.py" \
  -e "scripts/u4n_enter_split_buggy_replay.sh" \
  -e "docs/word-ui-u4n" || echo "（零殘留）"
