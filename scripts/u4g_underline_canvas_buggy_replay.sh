#!/usr/bin/env bash
# U4g（web 殼 canvas 渲染器消費 DrawItem::Text.underline 繪形態幾何）BUGGY 回塞
# 鑑別腳本——先紅後綠的機械證明隨貨。
#
# 用法：
#   bash scripts/u4g_underline_canvas_buggy_replay.sh buggy   # 回塞態：形態盞必紅
#   bash scripts/u4g_underline_canvas_buggy_replay.sh good    # 正常態：全考卷必綠
#
# 口徑（本卡採 k21/g1/R7/K8/E-2/E-2b 同款 **env 回塞鎖**慣例）：生產碼
# `shells/web/editor/vendor/renderer.js` 的 `underlineBuggy()` 讀
# `process.env.U4G_UNDERLINE_BUGGY`——置位時 `resolveUnderlineGeom()` 一律回
# 單線配方＝修前「只唸 FLAG_UNDERLINE、完全不讀 underline 欄」的舊行為（含
# Some(Off) 也照畫）。探針經 CDP 注入 `window.process={env:{...}}` 驅動本鎖
# （瀏覽器端本無 process，注入即回塞），零改檔、零還原風險，禁 checkout/reset
# 鐵律天然滿足。env 名稱 grep 現庫自訂（E2_/E2B_UNDERLINE_BUGGY 為既有鎖、不相犯）。
#
# 鑑別力（buggy 態必紅的盞別，實測坐見 11 盞）：
#   A-double／A-thick／A-dotted／A-dashed／A-off（真閉環像素形態面五盞）＋
#   B-double／B-thick／B-dotted／B-dashed／B-段數序／B-off（合成幾何六盞）——
#   證明斷言真在比形態幾何、不是比「有沒有底線」。
#   保持綠的盞（A0/A1/A4 基準與 drain、B-single、B-raw、B-未知字面、B-未宣告
#   兜底、C-真檔 Raw 降級、D-源碼同步鎖、零例外鎖）＝修前後行為相同的層，
#   改前即綠＝鑑別力對上（能正確宣稱的不誤報）。
#
# 前置：Chrome 需 --remote-debugging-port=9222（CDP_URL 可覆寫）；本腳本自管
# 靜態伺服（U4G_PORT，默認 8791，已在聽則複用）。
# good 態末了以 `git status` 對照 HEAD 驗證工作區零殘留（env 鎖不改檔，
# 髒 worktree 即另有未提交改動，報警不靜默）。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
MODE="${1:-}"
PORT="${U4G_PORT:-8791}"
CDP="${CDP_URL:-http://127.0.0.1:9222}"

case "$MODE" in buggy|good) ;; *)
  echo "usage: $0 buggy|good" >&2; exit 9;;
esac

# CDP 可用性先斷言（探針沒瀏覽器直接退 2，別混淆成鑑別紅）
if ! curl -s --max-time 3 "$CDP/json/version" >/dev/null; then
  echo "錯誤：CDP（$CDP）不可達——先起 Chrome --remote-debugging-port=9222" >&2
  exit 8
fi

# 靜態伺服：已在聽則複用，否則自起自收
SRV_PID=""
if ! curl -s --max-time 2 "http://127.0.0.1:$PORT/shells/web/editor/index.html" >/dev/null; then
  python3 -m http.server "$PORT" --bind 127.0.0.1 >/tmp/u4g_replay_http.log 2>&1 &
  SRV_PID=$!
  sleep 1
fi
cleanup() { [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null || true; }
trap cleanup EXIT

run_probe() {
  local expect="$1" # red|green
  export U4G_URL="http://127.0.0.1:$PORT/shells/web/editor/index.html"
  export CDP_URL="$CDP"
  if [ "$MODE" = buggy ]; then export U4G_BUGGY=1; else unset U4G_BUGGY || true; fi
  node shells/web/editor/tests/u4g_underline_canvas_probe.mjs >/tmp/u4g_replay.log 2>&1
  local rc=$?
  local reds
  reds=$(grep -c '^  ✗' /tmp/u4g_replay.log || true)
  if [ "$expect" = red ]; then
    if [ "$rc" -eq 0 ]; then
      echo "錯誤（$MODE）：考卷在 $MODE 態竟全綠——鑑別力失效" >&2; return 1
    fi
    # 點名必紅盞（缺任一＝斷言沒在比該形態幾何）
    local missing=0
    for lamp in 'A-double' 'A-thick' 'A-dotted' 'A-dashed' 'A-off' 'B-double' 'B-thick' 'B-dotted' 'B-dashed' 'B-段數序' 'B-off'; do
      if ! grep -q "✗ .*${lamp}" /tmp/u4g_replay.log; then
        echo "錯誤（$MODE）：形態盞「${lamp}」未翻紅——該形態斷言失效" >&2; missing=1
      fi
    done
    [ "$missing" -ne 0 ] && return 1
    echo "確認（$MODE）：考卷必紅（${reds} 盞紅，逐盞點名齊備；log：/tmp/u4g_replay.log）"
    return 0
  else
    if [ "$rc" -ne 0 ]; then
      echo "錯誤（$MODE）：考卷在 $MODE 態竟紅（${reds} 盞）——形態繪製未落地或被污染" >&2
      tail -40 /tmp/u4g_replay.log; return 1
    fi
    echo "確認（$MODE）：考卷全綠（$(grep -c '^  ✓' /tmp/u4g_replay.log || true) 盞綠；log：/tmp/u4g_replay.log）"
    return 0
  fi
}

echo "=== U4g canvas 形態幾何考卷（態=${MODE}）==="
run_probe "$([ "$MODE" = buggy ] && echo red || echo green)" || exit 1

if [ "$MODE" = good ]; then
  DIRTY="$(git status --porcelain -- shells/web/editor/vendor/renderer.js | tr -d '[:space:]')"
  if [ -n "$DIRTY" ]; then
    echo "警告：工作區對 HEAD 有 renderer.js 殘留改動（$DIRTY）——本腳本不改檔，請先確認是否預期" >&2
    exit 1
  fi
fi
echo "U4G_${MODE^^}_OK"
