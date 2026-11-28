#!/usr/bin/env bash
# R1-a 續（replay 語意貫穿 K29c／K29d 分流臂）BUGGY 回塞鑑別腳本——先紅後綠的
# 機械證明隨貨（比照 scripts/r1a_redo_buggy_replay.sh 的 env 回塞鎖慣例）。
#
# 用法：
#   bash scripts/r1a2_replay_k29_buggy_replay.sh buggy        # 回塞態：考卷必紅
#   bash scripts/r1a2_replay_k29_buggy_replay.sh good         # 正常態：考卷必綠
#   bash scripts/r1a2_replay_k29_buggy_replay.sh shield-only  # 只撤護欄：仍必綠
#   bash scripts/r1a2_replay_k29_buggy_replay.sh mirror-only  # 只翻鏡像：仍必綠
#
# 三把鎖的口徑（生產碼 mutation.rs／mutation_k29c.rs／mutation_k29d.rs 內建）：
#   R1A_SHIELD_BUGGY=1      → `redo()` 呼叫端重放護欄失效（不保管剩餘棧），
#                             主幹重放臂照樣不誤清＝機制面判別軸。
#   R1A_UNTHREADED_BUGGY=1  → K29c／K29d 分流臂的 `commit_*` 退回「無條件
#                             redo_stack.clear()」的修前形態（鏡像）。
#   兩鎖併翻                 → 逐字復刻 R1-a 續修前世界：撤除兜底＋旗標未貫穿。
#
# 鑑別力（本腳本 buggy 態必紅的盞別）：
#   r1a_replay_k29_family：燈 1（K29c 跨家族重放鎖）、燈 2（K29d 跨家族重放鎖）、
#   燈 5（新操作斷鏈反向鎖）＝3 盞紅。燈 3 兩態皆綠（它斷的是「護欄撤除後考卷
#   仍為真」，正常態綠、回塞態由燈 1／2／5 承接紅）；燈 4 兩態皆綠（它自帶三態
#   對照，紅綠分岔在燈內完成）。
#
# 另兩態是本卡的「護欄／旗標分工」證明，不是缺陷態：
#   shield-only：護欄失效＋旗標已貫穿 ⇒ 全綠——使用者可見行為不依賴護欄兜底
#                （卡面任務 2「兜底角色可撤」的正向證據）。
#   mirror-only：分流臂未貫穿＋護欄在場 ⇒ 全綠——忠實記錄卡面「現況因護欄存在，
#                缺陷被兜住」的事實（紅燈必須打機制面的由來）。
#
# 回塞＝置變數，零改檔、零還原風險，禁 checkout/reset 鐵律天然滿足。
# good 態末了以 `git status` 對照 HEAD 驗證工作區零殘留（env 鎖不改檔）。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=16777216

TEST_TARGET="-p quire-engine --test r1a_replay_k29_family"
LOG=/tmp/r1a2_replay_k29.log

run_expect() {
  local mode="$1" expect="$2"   # expect: red|green
  set +e
  cargo test -q $TEST_TARGET >"$LOG" 2>&1
  local rc=$?
  set -e
  if [ "$expect" = red ]; then
    if [ "$rc" -eq 0 ]; then
      echo "錯誤（$mode）：r1a_replay_k29_family 在 $mode 態竟全綠——鑑別力失效" >&2
      return 1
    fi
    echo "確認（$mode）：r1a_replay_k29_family 必紅（$(grep -c '^    r1af_' "$LOG" || true) 盞失敗燈，log：$LOG）"
  else
    if [ "$rc" -ne 0 ]; then
      echo "錯誤（$mode）：r1a_replay_k29_family 在 $mode 態竟紅——修復未落地或被污染" >&2
      tail -30 "$LOG"
      return 1
    fi
    echo "確認（$mode）：r1a_replay_k29_family 必綠"
  fi
}

case "${1:-}" in
  buggy)
    export R1A_SHIELD_BUGGY=1
    export R1A_UNTHREADED_BUGGY=1
    echo "=== 回塞態（護欄撤除＋分流臂未貫穿）：跨家族重放鎖預期必紅 ==="
    run_expect buggy red
    echo "BUGGY 態回塞鑑別通過"
    ;;
  good)
    unset R1A_SHIELD_BUGGY || true
    unset R1A_UNTHREADED_BUGGY || true
    echo "=== 正常態：考卷預期必綠 ==="
    run_expect good green
    DIRTY="$(git status --porcelain -- core/engine-api/src/mutation.rs core/engine-api/src/mutation_k29c.rs core/engine-api/src/mutation_k29d.rs | tr -d '[:space:]')"
    if [ -n "$DIRTY" ]; then
      echo "警告：工作區對 HEAD 有殘留改動（$DIRTY）——本腳本不改檔，請先確認是否預期" >&2
      exit 1
    fi
    echo "GOOD 態通過（env 鎖零改檔，工作區乾淨）"
    ;;
  shield-only)
    export R1A_SHIELD_BUGGY=1
    unset R1A_UNTHREADED_BUGGY || true
    echo "=== 只撤護欄（旗標已貫穿）：考卷預期仍必綠＝兜底角色可撤的正向證據 ==="
    run_expect shield-only green
    ;;
  mirror-only)
    unset R1A_SHIELD_BUGGY || true
    export R1A_UNTHREADED_BUGGY=1
    echo "=== 只翻鏡像（護欄在場）：考卷預期仍必綠＝卡面「現況靠護欄過」實錄 ==="
    run_expect mirror-only green
    ;;
  *)
    echo "用法: $0 buggy|good|shield-only|mirror-only" >&2
    exit 2
    ;;
esac
