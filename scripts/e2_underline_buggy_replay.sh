#!/usr/bin/env bash
# E-2（underline 保形態）BUGGY 回塞鑑別腳本——先紅後綠的機械證明隨貨。
#
# 用法：
#   bash scripts/e2_underline_buggy_replay.sh buggy   # 回塞態：兩張考卷必紅
#   bash scripts/e2_underline_buggy_replay.sh good    # 正常態：兩張考卷必綠
#
# 口徑（本卡採 k21_soft_br／g1_anchor_relativity 的 **env 回塞鎖**慣例，
# 非 k29e 的字串置換慣例——理由：本卡缺陷點是「形態判讀函式折損」，两处
# 生產碼各自內建 `E2_UNDERLINE_BUGGY` 開關（styles.rs `underline_of` 折回
# 布林語義；render_svg.rs `uline_geom` 恆 None＝全形態退回 text-decoration
# 單線舊路徑），回塞＝置變數，零改檔、零還原風險，禁 checkout/reset 鐵律
# 天然滿足）。env 名稱 grep 現庫自取（兩處生產碼實讀坐實），非臆造。
#
# 鑑別力（buggy 態必紅的盞別，實測坐見）：
#   解析層 e2_underline_kind：燈1（double）／燈2（thick·dotted·dashed）／
#     燈3（dash 別名歸一）／燈5（wavy·words raw 保形）／燈7（樣式層 double
#     保形進級聯基底）＝5 盞紅。
#   發射層 e2_underline_emit：燈1（形態進顯示列表欄）／燈2（SVG 幾何底線）／
#     燈3（Raw 原字面透傳）／燈6（寫讀口徑閉環）＝4 盞紅。
#   合計 9 盞紅——證明斷言真在比形態／幾何，不是比「有無底線」。
# good 態末了以 `git diff` 對照 HEAD 驗證工作區零殘留（env 鎖不改檔，
# 髒OfWorktree 即另有未提交改動，報警不靜默）。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=16777216

run_pair() {
  local mode="$1" expect="$2" # expect: red|green
  local rc_all=0
  for t in "quire-ooxml e2_underline_kind" "quire-engine e2_underline_emit"; do
    set -- $t
    local pkg="$1" test="$2"
    set +e
    cargo test -q -p "$pkg" --test "$test" >/tmp/e2_replay_$test.log 2>&1
    local rc=$?
    set -e
    if [ "$expect" = red ]; then
      if [ "$rc" -eq 0 ]; then
        echo "錯誤（$mode）：$test 在 $mode 態竟全綠——鑑別力失效" >&2
        rc_all=1
      else
        echo "確認（$mode）：$test 必紅（$(grep -c '^    e2' /tmp/e2_replay_$test.log || true) 盞失敗燈，log：/tmp/e2_replay_$test.log）"
      fi
    else
      if [ "$rc" -ne 0 ]; then
        echo "錯誤（$mode）：$test 在 $mode 態竟紅——修復未落地或被污染" >&2
        tail -30 /tmp/e2_replay_$test.log
        rc_all=1
      else
        echo "確認（$mode）：$test 必綠"
      fi
    fi
  done
  return "$rc_all"
}

case "${1:-}" in
  buggy)
    export E2_UNDERLINE_BUGGY=1
    echo "=== 回塞態（E2_UNDERLINE_BUGGY=1）：兩張考卷預期必紅 ==="
    run_pair buggy red
    echo "BUGGY 態回塞鑑別通過"
    ;;
  good)
    unset E2_UNDERLINE_BUGGY || true
    echo "=== 正常態：兩張考卷預期必綠 ==="
    run_pair good green
    DIRTY="$(git status --porcelain -- core/ooxml/src/styles.rs shells/native/src/render_svg.rs | tr -d '[:space:]')"
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
