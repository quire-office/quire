#!/usr/bin/env bash
# t_e93fb47b（SplitParagraph 跨切點容器配對）BUGGY 回塞鑑別腳本——先紅後綠的機械證明隨貨。
#
# 用法：
#   bash scripts/split_rev_buggy_replay.sh buggy   # 回塞態：考卷必紅（該紅盞別見下）
#   bash scripts/split_rev_buggy_replay.sh good    # 正常態：考卷全綠
#
# 口徑（採 E-2b／U4f 同款 **env 回塞鎖**慣例）：生產碼 `split_paragraph_xml`
# 內建 `SPLIT_REV_BUGGY` 開關——回塞＝跳過「切點容器配對補全」＋pPr 前綴退回
# 修前 `analysis.ppr_end_offset` 口徑（鏡像修前形態：只斷文字流、段尾未閉
# 容器原樣留著、雙份 pPr）。零改檔、零還原風險，禁 checkout/reset 鐵律天然
# 滿足。env 名稱 grep 現庫自訂，與既有 *_BUGGY 鎖不相犯。
#
# 鑑別力（buggy 態必紅的盞別，run 89 實測坐見）：
#   e_split_revision_containers：F1／F3／F4／F5（合成四形態，切點跨容器）／
#   C1～C4（corpus 四檔精確重現，C1 拍回卡面原文 `expected </w:ins>`）／
#   R1（雙份 pPr）／U1（undo 後存檔被病態段污染）＝10 盞紅。
#   F2（w:delText 守恆盞）不歸本鎖管（切點不跨容器＝配對補全本是恆等路徑），
#   回塞態仍綠＝鑑別力不混同，考卷檔頭已記帳。
# good 態末了以 `git status` 對照 HEAD 驗證工作區零殘留（env 鎖不改檔，
# 髒 worktree 即另有未提交改動，報警不靜默）。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=16777216

mode="${1:-}"
if [[ "$mode" != "buggy" && "$mode" != "good" ]]; then
  echo "用法: $0 buggy|good" >&2
  exit 2
fi

run_pair() {
  local expect="$1" # red|green
  local rc=0
  set +e
  cargo test -q -p quire-engine --test e_split_revision_containers >/tmp/split_rev_replay.log 2>&1
  rc=$?
  set -e
  local passed failed
  passed=$(grep -oE '[0-9]+ passed' /tmp/split_rev_replay.log | head -1 || echo "0 passed")
  failed=$(grep -oE '[0-9]+ failed' /tmp/split_rev_replay.log | head -1 || echo "0 failed")
  echo "  結果: ${passed}, ${failed} (rc=${rc})"
  if [[ "$expect" == "red" && "$rc" -eq 0 ]]; then
    echo "  ✗ 回塞態却全綠＝考卷無鑑別力" >&2
    return 1
  fi
  if [[ "$expect" == "green" && "$rc" -ne 0 ]]; then
    echo "  ✗ 正常態却紅（回歸！）" >&2
    tail -30 /tmp/split_rev_replay.log >&2
    return 1
  fi
  echo "  ✓ 符合預期（期望 ${expect}）"
  return 0
}

rc_all=0
if [[ "$mode" == "buggy" ]]; then
  echo "=== 回塞態（SPLIT_REV_BUGGY=1）：跨切點容器考卷預期必紅 ==="
  export SPLIT_REV_BUGGY=1
  run_pair red || rc_all=1
  unset SPLIT_REV_BUGGY
else
  echo "=== 正常態：跨切點容器考卷預期全綠 ==="
  run_pair green || rc_all=1
  if [[ -n "$(git status --porcelain)" ]]; then
    echo "  ！工作區有未提交改動（env 鎖不改檔，應為乾淨）：" >&2
    git status --short >&2
    rc_all=1
  fi
fi
exit "$rc_all"
