#!/usr/bin/env bash
# K29e BUGGY 回塞與還原腳本（支援 buggy 與 good 兩態）。
#
# 用法：
#   bash scripts/k29e_buggy_replay.sh buggy  # 塞回「舊五欄版」，k29e 考卷必紅
#   bash scripts/k29e_buggy_replay.sh good   # 從乾淨副本（HEAD 已提交態）還原，考卷必綠
#
# 規範守衛（k29c／k29d 腳本同款紀律）：
# - 嚴禁 git checkout / reset / clean / stash，走字串就地置換。
# - 每處置換錨點必唯一，失配即報警退出（不靜默跳過）。
# - good 態末了以 `git diff` 對照 HEAD（＝好版副本已提交態）驗證逐位元組一致，
#   不一致即紅（防置換未完全逆轉）。
#
# 瑕疵注入（主開關＝mutation.rs 的 `let buggy_k29e_fields = false;` 單行翻轉，
# 一處把六欄寫入路徑整個塞回「舊五欄版」；另雨處發射層退化）：
#   P1 六欄寫入路徑無效化（remove 不碰六族、add 不寫六欄、全 None 硬閂失效）
#      → 燈 1（六欄閉環）、燈 3（底線五形態＋雙線）、燈 5（highlight 雙路徑）、
#        燈 6（顯式關閉贏過級聯）、燈 2（空 rPr 材料化破 byte-identity）全紅。
#   P2a/P2b 發射層下標臂退化（不縮尺＋不位移）→ 燈 1、燈 4 紅（下標退回
#      全尺不位移＝本卡修掉的既有缺陷形態；證明燈 4 斷言真在比幾何）。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=16777216

TARGET_M="core/engine-api/src/mutation.rs"
TARGET_L="core/engine-api/src/lib.rs"

toggle() {
  local target="$1"
  local from="$2"
  local to="$3"
  local label="$4"
  python3 - "$target" "$from" "$to" "$label" <<'PY'
import sys

target, old, new, label = sys.argv[1:5]
with open(target, "r", encoding="utf-8") as f:
    content = f.read()
n = content.count(old)
if n == 1:
    content = content.replace(old, new, 1)
    with open(target, "w", encoding="utf-8") as f:
        f.write(content)
    print(f"[{label}] 成功置換")
elif n == 0:
    print(f"[{label}] 未找到目標字串（可能已在此態）")
else:
    print(f"[{label}] 錨點出現 {n} 次非唯一，拒絕置換")
    sys.exit(1)
PY
}

case "${1:-}" in
  buggy)
    # P1：主開關翻轉（remove/add 十五臂與全 None 硬閂全數退回舊五欄版）。
    toggle "$TARGET_M" \
      "let buggy_k29e_fields = false;" \
      "let buggy_k29e_fields = true;" \
      "P1_K29E_FIELDS_BUGGY"
    # P2：發射層下標臂退化（下標退回全尺不位移——本卡修掉的既有缺陷形態）。
    toggle "$TARGET_L" \
      "let seg_size = if is_super || is_sub {" \
      "let seg_size = if is_super { // BUGGY_P2a：下標不縮尺" \
      "P2a_SIZE_BUGGY"
    toggle "$TARGET_L" \
      "y + base_seg_size * (1.0 / 11.0)" \
      "y // BUGGY_P2b：下標不位移" \
      "P2b_Y_BUGGY"
    echo "=== 正在執行紅燈測試（預期必紅）==="
    set +e
    cargo test -p quire-engine --test k29e_char_format
    TEST_RES=$?
    set -e
    echo "BUGGY 態測試結束，exit_code=${TEST_RES}（預期非 0）"
    if [ "$TEST_RES" -eq 0 ]; then
      echo "錯誤：BUGGY 態下考卷竟全綠，紅燈鑑別力失效" >&2
      exit 1
    fi
    ;;

  good)
    toggle "$TARGET_M" \
      "let buggy_k29e_fields = true;" \
      "let buggy_k29e_fields = false;" \
      "P1_K29E_FIELDS_GOOD"
    toggle "$TARGET_L" \
      "let seg_size = if is_super { // BUGGY_P2a：下標不縮尺" \
      "let seg_size = if is_super || is_sub {" \
      "P2a_SIZE_GOOD"
    toggle "$TARGET_L" \
      "y // BUGGY_P2b：下標不位移" \
      "y + base_seg_size * (1.0 / 11.0)" \
      "P2b_Y_GOOD"
    # 乾淨副本對照：還原態必須與已提交態（HEAD＝好版副本）逐位元組一致。
    DIRTY="$(git diff --name-only -- "$TARGET_M" "$TARGET_L" | tr -d '[:space:]')"
    if [ -n "$DIRTY" ]; then
      echo "警告：還原後 $DIRTY 與 HEAD 不一致（置換未完全逆轉）" >&2
      git diff -- "$TARGET_M" "$TARGET_L" | head -40
      exit 1
    fi
    echo "還原態與 HEAD 逐位元組一致（乾淨副本驗證通過）"
    echo "=== 正在執行正常測試（預期必綠）==="
    cargo test -p quire-engine --test k29e_char_format
    echo "GOOD 態測試結束，exit_code=0"
    ;;

  *)
    echo "用法: $0 buggy|good" >&2
    exit 2
    ;;
esac
