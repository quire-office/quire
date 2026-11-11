#!/usr/bin/env bash
# AGY-K29A BUGGY 回塞與還原腳本（支援 buggy 與 good 兩態）。
#
# 用法：
#   bash scripts/k29a_buggy_replay.sh buggy  # 塞入 BUGGY 瑕疵，紅燈 1–3 必紅
#   bash scripts/k29a_buggy_replay.sh good   # 還原正常實作，紅燈 1–3 必綠
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

TARGET="core/engine-api/src/mutation.rs"

case "${1:-}" in
  buggy)
    python3 - "$TARGET" <<'PY'
import sys

target = sys.argv[1]
with open(target, "r", encoding="utf-8") as f:
    content = f.read()

p1_good = """        // [K29A_BUGGY_POINT_1: PARTS_WRITEBACK]
        self.doc.parts.insert(part_name.to_string(), new_xml);"""

p1_buggy = """        // [K29A_BUGGY_POINT_1: PARTS_WRITEBACK]
        // BUGGY: 略過寫回 doc.parts
        let _ = &new_xml;"""

p2_good = """                // [K29A_BUGGY_POINT_2: UNDO_XML_RESTORE]
                *xml = restored_xml;"""

p2_buggy = """                // [K29A_BUGGY_POINT_2: UNDO_XML_RESTORE]
                // BUGGY: 略過 XML 反向還原
                let _ = &restored_xml;"""

if p1_good in content:
    content = content.replace(p1_good, p1_buggy, 1)
    print("[FLAG_P1_BUGGY] 成功回塞：略過 doc.parts 寫回（紅燈 1、2 驗證點）")
else:
    print("[WARN_P1] 未找到 P1_GOOD，可能已是 BUGGY 態")

if p2_good in content:
    content = content.replace(p2_good, p2_buggy, 1)
    print("[FLAG_P2_BUGGY] 成功回塞：略過 Undo XML 反向還原（紅燈 3 驗證點）")
else:
    print("[WARN_P2] 未找到 P2_GOOD，可能已是 BUGGY 態")

with open(target, "w", encoding="utf-8") as f:
    f.write(content)
PY
    echo "=== 正在執行紅燈測試（預期必紅）==="
    set +e
    cargo test -p quire-engine --test k29a_text_roundtrip
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

p1_good = """        // [K29A_BUGGY_POINT_1: PARTS_WRITEBACK]
        self.doc.parts.insert(part_name.to_string(), new_xml);"""

p1_buggy = """        // [K29A_BUGGY_POINT_1: PARTS_WRITEBACK]
        // BUGGY: 略過寫回 doc.parts
        let _ = &new_xml;"""

p2_good = """                // [K29A_BUGGY_POINT_2: UNDO_XML_RESTORE]
                *xml = restored_xml;"""

p2_buggy = """                // [K29A_BUGGY_POINT_2: UNDO_XML_RESTORE]
                // BUGGY: 略過 XML 反向還原
                let _ = &restored_xml;"""

if p1_buggy in content:
    content = content.replace(p1_buggy, p1_good, 1)
    print("[FLAG_P1_GOOD] 成功還原：恢復 doc.parts 寫回")
else:
    print("[WARN_P1] 未找到 P1_BUGGY，可能已是 GOOD 態")

if p2_buggy in content:
    content = content.replace(p2_buggy, p2_good, 1)
    print("[FLAG_P2_GOOD] 成功還原：恢復 Undo XML 反向還原")
else:
    print("[WARN_P2] 未找到 P2_BUGGY，可能已是 GOOD 態")

with open(target, "w", encoding="utf-8") as f:
    f.write(content)
PY
    echo "=== 正在執行正常測試（預期必綠）==="
    cargo test -p quire-engine --test k29a_text_roundtrip
    echo "GOOD 態測試結束，exit_code=0"
    ;;

  *)
    echo "用法: $0 buggy|good" >&2
    exit 2
    ;;
esac
