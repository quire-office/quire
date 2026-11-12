#!/usr/bin/env bash
# OC-K29B BUGGY 回塞與還原腳本（支援 buggy 與 good 兩態）。
#
# 用法：
#   bash scripts/k29b_buggy_replay.sh buggy  # 塞入三處 BUGGY 瑕疵，紅燈 1–3 必紅
#   bash scripts/k29b_buggy_replay.sh good   # 還原正常實作，紅燈 1–3 必綠
#
# 規範守衛：
# - 嚴禁 git checkout / reset / clean / stash，走字串就地置換。
# - 每處替換皆列印成功旗標。
# - 還原後 git status --porcelain -uall 僅應留下本卡白名單變更。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=16777216

TARGET="core/engine-api/src/mutation.rs"

toggle() {
  local from="$1"
  local to="$2"
  local label="$3"
  python3 - "$TARGET" "$from" "$to" "$label" <<'PY'
import sys

target, old, new, label = sys.argv[1:5]
with open(target, "r", encoding="utf-8") as f:
    content = f.read()
if old in content:
    content = content.replace(old, new, 1)
    with open(target, "w", encoding="utf-8") as f:
        f.write(content)
    print(f"[{label}] 成功置換")
else:
    print(f"[{label}] 未找到目標字串（可能已在此態）")
PY
}

case "${1:-}" in
  buggy)
    toggle \
      "        let skip_structural_rebuild = false;" \
      "        let skip_structural_rebuild = true;" \
      "FLAG_P1_BUGGY_STRUCTURAL_REBUILD"
    toggle \
      "    let buggy_rpr = false;" \
      "    let buggy_rpr = true;" \
      "FLAG_P2_BUGGY_RPR_PATCH"
    toggle \
      "    let buggy_container_guard = false;" \
      "    let buggy_container_guard = true;" \
      "FLAG_P3_BUGGY_CONTAINER_GUARD"
    echo "=== 正在執行紅燈測試（預期必紅）==="
    set +e
    cargo test -p quire-engine --test k29b_structural_styles
    TEST_RES=$?
    set -e
    echo "BUGGY 態測試結束，exit_code=${TEST_RES}（預期非 0）"
    ;;

  good)
    toggle \
      "        let skip_structural_rebuild = true;" \
      "        let skip_structural_rebuild = false;" \
      "FLAG_P1_GOOD_STRUCTURAL_REBUILD"
    toggle \
      "    let buggy_rpr = true;" \
      "    let buggy_rpr = false;" \
      "FLAG_P2_GOOD_RPR_PATCH"
    toggle \
      "    let buggy_container_guard = true;" \
      "    let buggy_container_guard = false;" \
      "FLAG_P3_GOOD_CONTAINER_GUARD"
    echo "=== 正在執行正常測試（預期必綠）==="
    cargo test -p quire-engine --test k29b_structural_styles
    echo "GOOD 態測試結束，exit_code=0"
    ;;

  *)
    echo "用法: $0 buggy|good" >&2
    exit 2
    ;;
esac
