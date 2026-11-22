#!/usr/bin/env bash
# E-2b（底線枚舉共型收尾）BUGGY 回塞鑑別腳本——先紅後綠的機械證明隨貨。
#
# 用法：
#   bash scripts/e2b_underline_buggy_replay.sh buggy   # 回塞態：共型考卷必紅
#   bash scripts/e2b_underline_buggy_replay.sh good    # 正常態：共型考卷必綠
#
# 口徑（本卡採 k21/g1/R7/K8/E-2 同款 **env 回塞鎖**慣例，非 k29e 的字串置換
# 慣例）：生產碼內建 `E2B_UNDERLINE_BUGGY` 開關，兩處同口徑——
#   render-trait `Underline::as_u_val`：Thick 折回 single（鏡像枚舉時代根本無
#     此變體）、Raw 折回 single（入口閘門同款放行＝「寫入端無從拒絕 Raw」的
#     修前語義）；
#   mutation.rs `reject_unwritable_underline`：Raw 放行。
# 回塞＝置變數，零改檔、零還原風險，禁 checkout/reset 鐵律天然滿足。env 名稱
# grep 現庫自訂（E2_UNDERLINE_BUGGY 為 E-2 既有鎖、不相犯；兩鎖可各自獨立翻）。
#
# 鑑別力（buggy 態必紅的盞別，實測坐見）：
#   e2b_underline_shared_type：燈1（as_u_val 值域：thick/raw 出口）／燈2
#   （Thick 端到端）／燈3（Raw 誠實拒）／燈6（canonical 全集閉環之 thick 值）
#   ＝4 盞紅——證明斷言真在比共型後的型別契約與寫出路徑，不是比「有没有底線」。
#   燈4／5／7（serde 遷移雙讀/canonical/不吞值）不歸 env 鎖管，回塞態仍綠
#   （線契約與寫出字面是兩層，鑑別力不混同）。
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
  cargo test -q -p quire-engine --test e2b_underline_shared_type >/tmp/e2b_replay.log 2>&1
  local rc=$?
  set -e
  if [ "$expect" = red ]; then
    if [ "$rc" -eq 0 ]; then
      echo "錯誤（$mode）：e2b_underline_shared_type 在 $mode 態竟全綠——鑑別力失效" >&2
      rc_all=1
    else
      echo "確認（$mode）：e2b_underline_shared_type 必紅（$(grep -c '^    e2b' /tmp/e2b_replay.log || true) 盞失敗燈，log：/tmp/e2b_replay.log）"
    fi
  else
    if [ "$rc" -ne 0 ]; then
      echo "錯誤（$mode）：e2b_underline_shared_type 在 $mode 態竟紅——共型未落地或被污染" >&2
      tail -30 /tmp/e2b_replay.log
      rc_all=1
    else
      echo "確認（$mode）：e2b_underline_shared_type 必綠"
    fi
  fi
  return "$rc_all"
}

case "${1:-}" in
  buggy)
    export E2B_UNDERLINE_BUGGY=1
    echo "=== 回塞態（E2B_UNDERLINE_BUGGY=1）：共型考卷預期必紅 ==="
    run_pair buggy red
    echo "BUGGY 態回塞鑑別通過"
    ;;
  good)
    unset E2B_UNDERLINE_BUGGY || true
    echo "=== 正常態：共型考卷預期必綠 ==="
    run_pair good green
    DIRTY="$(git status --porcelain -- core/render-trait/src/lib.rs core/engine-api/src/mutation.rs | tr -d '[:space:]')"
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
