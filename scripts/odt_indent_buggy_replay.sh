#!/usr/bin/env bash
# AGY-ODTIND BUGGY 回塞與還原腳本（支援 buggy 與 good 兩態）。
#
# 用法：
#   bash scripts/odt_indent_buggy_replay.sh buggy  # 塞入 BUGGY 瑕疵，紅燈 1–2 必紅
#   bash scripts/odt_indent_buggy_replay.sh good   # 還原正常實作，燈 1–4 必綠
#
# 規範守衛：
# - 嚴禁 git checkout / reset / clean / stash，走字串就地置換。
# - 每處替換皆列印成功旗標。
# - 還原後 git status --porcelain -uall 必須完全乾淨。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=16777216

TARGET="core/odf/src/display.rs"

case "${1:-}" in
  buggy)
    python3 - "$TARGET" <<'PY'
import sys

target = sys.argv[1]
with open(target, "r", encoding="utf-8") as f:
    content = f.read()

p_good = """                // [ODT_INDENT_BUGGY_POINT: LINE_BASE_X]
                let base_x = compute_line_x(
                    pg.margin_left_pt,
                    content_w,
                    line.width_pt,
                    line.char_offset,
                    props,
                );"""

p_buggy = """                // [ODT_INDENT_BUGGY_POINT: LINE_BASE_X]
                // BUGGY: 略過縮排與對齊計算，恆以版心左緣起排（燈 1、燈 2 驗證點）
                let base_x = pg.margin_left_pt;"""

if p_good in content:
    content = content.replace(p_good, p_buggy, 1)
    print("[FLAG_ODT_INDENT_BUGGY] 成功回塞：略過段落縮排與置中計算（紅燈 1、2 驗證點）")
else:
    print("[WARN] 未找到 P_GOOD，可能已是 BUGGY 態")

with open(target, "w", encoding="utf-8") as f:
    f.write(content)
PY
    echo "=== 正在執行紅燈測試（預期燈 1、燈 2 必紅）==="
    set +e
    cargo test -p quire-odf --test odf_paragraph_properties
    TEST_RES=$?
    set -e
    echo "BUGGY 態測試結束，exit_code=${TEST_RES}"
    ;;

  good)
    python3 - "$TARGET" <<'PY'
import sys

target = sys.argv[1]
with open(target, "r", encoding="utf-8") as f:
    content = f.read()

p_good = """                // [ODT_INDENT_BUGGY_POINT: LINE_BASE_X]
                let base_x = compute_line_x(
                    pg.margin_left_pt,
                    content_w,
                    line.width_pt,
                    line.char_offset,
                    props,
                );"""

p_buggy = """                // [ODT_INDENT_BUGGY_POINT: LINE_BASE_X]
                // BUGGY: 略過縮排與對齊計算，恆以版心左緣起排（燈 1、燈 2 驗證點）
                let base_x = pg.margin_left_pt;"""

if p_buggy in content:
    content = content.replace(p_buggy, p_good, 1)
    print("[FLAG_ODT_INDENT_GOOD] 成功還原：恢復段落縮排與置中計算")
else:
    print("[WARN] 未找到 P_BUGGY，可能已是 GOOD 態")

with open(target, "w", encoding="utf-8") as f:
    f.write(content)
PY
    echo "=== 正在執行正常測試（預期全綠）==="
    cargo test -p quire-odf --test odf_paragraph_properties
    echo "GOOD 態測試結束，exit_code=0"
    ;;

  *)
    echo "用法: $0 buggy|good" >&2
    exit 2
    ;;
esac
