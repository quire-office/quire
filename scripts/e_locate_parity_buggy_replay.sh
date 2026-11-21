#!/usr/bin/env bash
# t_c1dbc6d7（E 返修：游標反查口徑失配）BUGGY 回塞與鑑別力腳本。
#
# 用法：
#   bash scripts/e_locate_parity_buggy_replay.sh buggy   # BUGGY 期望門控態，兩盞必紅
#   bash scripts/e_locate_parity_buggy_replay.sh good    # 正常態，兩盞必綠
#   bash scripts/e_locate_parity_buggy_replay.sh base <commit>  # 於 pre-fix 態重現紅燈存證
#
# 卡面紀律（與 k29*_buggy_replay.sh 同族但更保守）：
# - 本卡缺陷在「反查路徑口徑 vs mutation 口徑失配」，注入點若動 src 會碰到鐵律禁改面
#   （core/ooxml/src/table.rs）與 mutation 熱檔，故**不走字串置換注入**，改用考卷內建
#   的 BUGGY 期望門控（E_SDT_TABLE_BUGGY／E_GRIDSPAN_BUGGY）：只換期望常數、引擎碼零動靜，
#   該燈即紅 ⇒ 證明斷言真在比對 canonical 事實。
# - base 態紅燈另以 `git worktree add`（唯讀檢出，絕不 checkout/reset 本工作區）建 pre-fix
#   乾淨樹，copy 考卷＋fixture 後跑：界內錯位（left:[1] vs right:[0]）與 overflow panic
#   在那邊重現，寫進 docs/e-locate-parity-redlight.md §4／§4b。
# - 嚴禁 git checkout / reset / clean / stash / push 動到本工作區。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0

TESTS=(--test e_sdt_wrapped_table --test e_gridspan_prefix)

run_expect_red() {
  local envname="$1" testname="$2"
  set +e
  env "$envname=1" cargo test -q -p quire-engine --test "$testname" 2>&1 | tail -5
  local rc=$?
  set -e
  if [ "$rc" -eq 0 ]; then
    echo "錯誤：$testname 在 $envname=1 下竟全綠 ⇒ 該盞鑑別力失效" >&2
    exit 1
  fi
  echo "[BUGGY] $testname（$envname=1）exit=$rc（預期非 0）✔"
}

case "${1:-}" in
  buggy)
    run_expect_red E_SDT_TABLE_BUGGY e_sdt_wrapped_table
    run_expect_red E_GRIDSPAN_BUGGY e_gridspan_prefix
    echo "BUGGY 鑑別雙盞均紅：斷言真在比 canonical 事實。"
    ;;

  good)
    cargo test -q -p quire-engine "${TESTS[@]}"
    echo "GOOD 態兩盞全綠（8/8 + 8/8）。"
    ;;

  base)
    BASE_REV="${2:?用法: $0 base <pre-fix commit>}"
    WT="$ROOT/../.buggy-base-$(basename "$BASE_REV")"
    if [ ! -d "$WT" ]; then
      git -C "$ROOT/.." worktree add "$WT" "$BASE_REV" >/dev/null
    fi
    # 考卷與 fixture 是本卡新增物：copy 進 pre-fix 樹，引擎碼維持 pre-fix（紅燈來源）。
    cp core/engine-api/tests/e_sdt_wrapped_table.rs \
       core/engine-api/tests/e_gridspan_prefix.rs "$WT/core/engine-api/tests/"
    cp fixtures/sdt_wrapped_table.docx fixtures/table_gridspan_prefix.docx \
       fixtures/table_gridspan_overflow.docx fixtures/table_nested_decoy.docx \
       fixtures/table_multi.docx "$WT/fixtures/"
    ( cd "$WT" && CARGO_TARGET_DIR="$ROOT/../.buggy-base-target" \
        cargo test -q -p quire-engine "${TESTS[@]}" 2>&1 | grep -E "FAILED|left:|overflow|test result" )
    echo "base 態紅燈如上方（界內錯位 left:[1]/right:[0] 與 overflow panic）；"
    echo "收尾：git worktree remove \"$WT\" --force（由執行者自行決定保留與否）。"
    ;;

  *)
    echo "用法: $0 buggy|good|base <commit>" >&2
    exit 2
    ;;
esac
