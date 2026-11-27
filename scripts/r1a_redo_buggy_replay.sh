#!/usr/bin/env bash
# R1-a（多步 Redo 自我擦除修復）BUGGY 回塞鑑別腳本——先紅後綠的機械證明隨貨。
#
# 用法：
#   bash scripts/r1a_redo_buggy_replay.sh buggy   # 回塞態：多步 redo 考卷必紅
#   bash scripts/r1a_redo_buggy_replay.sh good    # 正常態：考卷必綠
#
# 口徑（採 k21/g1/R7/K8/E-2/E-2b 同款 **env 回塞鎖** 慣例，非 k29e 的字串置換
# 慣例）：生產碼 mutation.rs 內建 `R1_REDO_BUGGY` 開關，兩處同口徑——
#   `apply_mutation_inner` 結尾：replay=true 時照樣 `redo_stack.clear()`
#     （修前鏡像：無條件 clear，重放第一筆就把整個 redo 棧清空）；
#   `redo()` 重放護欄：回塞態不保管剩餘棧（K29c／K29d 分流臂照清亦放行，
#     與修前「重放清棧」語義逐字等價）。
# 回塞＝置變數，零改檔、零還原風險，禁 checkout/reset 鐵律天然滿足。
#
# 鑑別力（buggy 態必紅的盞別，實測坐見）：
#   r1_redo_multistep：燈 1（多步重做鎖——第 2 次 redo 即 Ok(None)，修前形態）
#   ＝1 盞紅。燈 2（redo 後新操作斷鏈）兩態皆綠——新操作路徑（replay=false）
#   的 clear 不在回塞範圍（iron rule 1，搬走它才是缺陷）；燈 3（回塞鑑別盞）
#   兩態皆綠——它斷的是「缺陷在回塞態存在、正常態不存在」本身。
# good 態末了以 `git status` 對照 HEAD 驗證工作區零殘留（env 鎖不改檔，
# 髒 worktree 即另有未提交改動，報警不靜默）。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=16777216

run_pair() {
  local mode="$1" expect="$2" # expect: red|green
  local rc_all=0
  set +e
  cargo test -q -p quire-engine --test r1_redo_multistep >/tmp/r1a_replay.log 2>&1
  local rc=$?
  set -e
  if [ "$expect" = red ]; then
    if [ "$rc" -eq 0 ]; then
      echo "錯誤（$mode）：r1_redo_multistep 在 $mode 態竟全綠——鑑別力失效" >&2
      rc_all=1
    else
      echo "確認（$mode）：r1_redo_multistep 必紅（$(grep -c '^    r1a_' /tmp/r1a_replay.log || true) 盞失敗燈，log：/tmp/r1a_replay.log）"
    fi
  else
    if [ "$rc" -ne 0 ]; then
      echo "錯誤（$mode）：r1_redo_multistep 在 $mode 態竟紅——修復未落地或被污染" >&2
      tail -30 /tmp/r1a_replay.log
      rc_all=1
    else
      echo "確認（$mode）：r1_redo_multistep 必綠"
    fi
  fi
  return "$rc_all"
}

case "${1:-}" in
  buggy)
    export R1_REDO_BUGGY=1
    echo "=== 回塞態（R1_REDO_BUGGY=1）：多步 redo 考卷預期必紅 ==="
    run_pair buggy red
    echo "BUGGY 態回塞鑑別通過"
    ;;
  good)
    unset R1_REDO_BUGGY || true
    echo "=== 正常態：多步 redo 考卷預期必綠 ==="
    run_pair good green
    DIRTY="$(git status --porcelain -- core/engine-api/src/mutation.rs | tr -d '[:space:]')"
    if [ -n "$DIRTY" ]; then
      echo "警告：工作區對 HEAD 有殘留改動（$DIRTY）——本腳本不改檔，請先確認是否預期" >&2
      exit 1
    fi
    echo "GOOD 態通過（env 鎖零改檔，工作區乾淨）"
    ;;
  *)
    echo "用法: $0 buggy|good" >&2
    exit 2
    ;;
esac
