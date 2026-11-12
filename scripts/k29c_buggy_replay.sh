#!/usr/bin/env bash
# OC-K29C BUGGY 回塞與還原腳本（支援 buggy 與 good 兩態）。
#
# 用法：
#   bash scripts/k29c_buggy_replay.sh buggy  # 塞入四處 BUGGY 瑕疵，k29c 考卷必紅
#   bash scripts/k29c_buggy_replay.sh good   # 還原正常實作，k29c 考卷必綠
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

TARGET="core/engine-api/src/mutation_k29c.rs"

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
    # P1：commit_k29c 略過 rebuild_derived → 表格段落索引與像素池全不更新（紅燈 1）。
    toggle \
      "        self.rebuild_derived()
            .map_err(|e| MutationError::Engine(e.to_string()))?;
        while !self.is_settled() {" \
      "        // BUGGY_P1：略過派生層重建（段落索引與像素池不同步）
        while !self.is_settled() {" \
      "FLAG_P1_BUGGY_STRUCTURAL_REBUILD"
    # P2：縮放不寫回 wp:extent／a:ext → 顯示列表尺寸不變（紅燈 2）。
    toggle \
      "            if !t.closing && (t.local == \"extent\" || t.local == \"ext\") {" \
      "            if false && !t.closing && (t.local == \"extent\" || t.local == \"ext\") { // BUGGY_P2" \
      "FLAG_P2_BUGGY_RESIZE_EXTENT"
    # P3：關閉媒體內容雜湊去重 → 相同位元組仍新增重複 media part（紅燈 3 去重）。
    toggle \
      "            if name.starts_with(\"word/media/\") && bytes == media_bytes {" \
      "            if false && name.starts_with(\"word/media/\") && bytes == media_bytes { // BUGGY_P3" \
      "FLAG_P3_BUGGY_MEDIA_DEDUP"
    # P4：強制 GC 釋放位元組卻謊稱可逆、且不截斷 Undo 歷史（紅燈 3 不可逆鎖）。
    toggle \
      "                    irreversible = Some(ReversibilityLevel::Irreversible {
                        reason: IrreversibleReason::MediaPayloadEvicted {
                            part_name: media_part,
                            bytes_freed,
                        },
                    });" \
      "                    // BUGGY_P4：媒體已釋放卻謊稱可逆
                    irreversible = None;" \
      "FLAG_P4_BUGGY_GC_IRREVERSIBLE"
    echo "=== 正在執行紅燈測試（預期必紅）==="
    set +e
    cargo test -p quire-engine --test k29c_table_media
    TEST_RES=$?
    set -e
    echo "BUGGY 態測試結束，exit_code=${TEST_RES}（預期非 0）"
    if [ "$TEST_RES" -eq 0 ]; then
      echo "錯誤：BUGGY 態下考卷竟全綠，紅燈鑑別力失效" >&2
      exit 1
    fi
    ;;

  good)
    toggle \
      "        // BUGGY_P1：略過派生層重建（段落索引與像素池不同步）
        while !self.is_settled() {" \
      "        self.rebuild_derived()
            .map_err(|e| MutationError::Engine(e.to_string()))?;
        while !self.is_settled() {" \
      "FLAG_P1_GOOD_STRUCTURAL_REBUILD"
    toggle \
      "            if false && !t.closing && (t.local == \"extent\" || t.local == \"ext\") { // BUGGY_P2" \
      "            if !t.closing && (t.local == \"extent\" || t.local == \"ext\") {" \
      "FLAG_P2_GOOD_RESIZE_EXTENT"
    toggle \
      "            if false && name.starts_with(\"word/media/\") && bytes == media_bytes { // BUGGY_P3" \
      "            if name.starts_with(\"word/media/\") && bytes == media_bytes {" \
      "FLAG_P3_GOOD_MEDIA_DEDUP"
    toggle \
      "                    // BUGGY_P4：媒體已釋放卻謊稱可逆
                    irreversible = None;" \
      "                    irreversible = Some(ReversibilityLevel::Irreversible {
                        reason: IrreversibleReason::MediaPayloadEvicted {
                            part_name: media_part,
                            bytes_freed,
                        },
                    });" \
      "FLAG_P4_GOOD_GC_IRREVERSIBLE"
    echo "=== 正在執行正常測試（預期必綠）==="
    cargo test -p quire-engine --test k29c_table_media
    echo "GOOD 態測試結束，exit_code=0"
    ;;

  *)
    echo "用法: $0 buggy|good" >&2
    exit 2
    ;;
esac
