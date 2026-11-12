#!/usr/bin/env bash
# K29d BUGGY 回塞與還原腳本（支援 buggy 與 good 兩態）。
#
# 用法：
#   bash scripts/k29d_buggy_replay.sh buggy  # 塞入四處 BUGGY 瑕疵，k29d 考卷必紅
#   bash scripts/k29d_buggy_replay.sh good   # 還原正常實作，k29d 考卷必綠
#
# 規範守衛：
# - 嚴禁 git checkout / reset / clean / stash，走字串就地置換（參考 k29c 腳本）。
# - 每處替換皆列印成功旗標。
# - 還原後 git status --porcelain -uall 僅應留下本卡白名單變更。
#
# 四處瑕疵與對應紅燈（瑕疵編號 P2–P5 與腳本內 FLAG 旗標逐字對應；另有一處
# 「touched 自動退場失效」注入經試跑證明對本考卷不可觀測——whitelisted_changed
# 只記 data 實際不同的 entry，淨零變動態兩態同果——故不入腳本，避免虛報鑑別力；
# 該紀律由燈 3 undo byte_identical 與 k29a 紅燈 3 間接守衛）：
#   P2 插分節不新增宿主段   → 燈 4（para_sec base+1 口徑）紅。
#   P3 cp=0 回到「首 run 之前」注入 → 燈 2（頁尾插字被 fldChar 欄位代換污染）紅。
#   P4 未知節 id 退回 XmlPatchRejected → 燈 1（誠實拒絕型別）紅。
#   P5 rebuild_derived 不還原 bundle 基準 → 燈 3（undo 後 byte_identical）紅。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=16777216

TARGET_D="core/engine-api/src/mutation_k29d.rs"
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
    # P2：插分節符不新增宿主段（para_sec 回 base，燈 4 base+1 口徑紅）。
    toggle "$TARGET_D" \
      "        let mut out = doc.clone();
        out.replace_range(sp.start..sp.start, &new_para);" \
      "        let mut out = doc.clone();
        let _ = &new_para; // BUGGY_P2：不插入宿主段" \
      "FLAG_P2_BUGGY_HOST_PARA"
    # P3：cp=0 回到「首文字節點所在 run 之前」注入——fldChar 複雜欄段的新 run
    #      被欄位標記波及、代換成頁碼（燈 2 頁尾插字紅）。
    toggle "$TARGET_D" \
      "            let at = analysis
                .ppr_end_offset
                .unwrap_or(analysis.p_open_end_offset);
            out.replace_range(at..at, &inject_run);" \
      "            let at = analysis
                .text_nodes
                .iter()
                .find(|n| n.run_span.is_some())
                .and_then(|n| enclosing_run(&slice, n.node_span.start))
                .map(|(s, _)| s)
                .unwrap_or(analysis.p_open_end_offset); // BUGGY_P3：欄位污染注入點
            out.replace_range(at..at, &inject_run);" \
      "FLAG_P3_BUGGY_FIELD_POLLUTION_INJECT"
    # P4：未知節 id 退回 XmlPatchRejected（燈 1 誠實拒絕型別紅）。
    toggle "$TARGET_D" \
      "            if section_id != 0 {
                return Err(MutationError::SubDocNotFound {
                    target: unknown_section_target(section_id),
                });
            }" \
      "            if section_id != 0 {
                return Err(rejected(format!(\"BUGGY_P4 查無節 id={section_id}\")));
            }" \
      "FLAG_P4_BUGGY_REJECT_TYPE"
    # P5：派生重建後不把 bundle 換回開檔基準——中途存檔件的 deflate 流污染基準，
    #      undo 回基線後存檔仍非 byte_identical（燈 3 undo 全退紅）。
    toggle "$TARGET_L" \
      "        fresh.doc.bundle = self.doc.bundle.clone();" \
      "        // BUGGY_P5：bundle 基準被中途存檔件污染（undo byte_identical 假性失敗）" \
      "FLAG_P5_BUGGY_BUNDLE_BASELINE"
    echo "=== 正在執行紅燈測試（預期必紅）==="
    set +e
    cargo test -p quire-engine --test k29d_section_hf_notes
    TEST_RES=$?
    set -e
    echo "BUGGY 態測試結束，exit_code=${TEST_RES}（預期非 0）"
    if [ "$TEST_RES" -eq 0 ]; then
      echo "錯誤：BUGGY 態下考卷竟全綠，紅燈鑑別力失效" >&2
      exit 1
    fi
    ;;

  good)
    toggle "$TARGET_D" \
      "        let mut out = doc.clone();
        let _ = &new_para; // BUGGY_P2：不插入宿主段" \
      "        let mut out = doc.clone();
        out.replace_range(sp.start..sp.start, &new_para);" \
      "FLAG_P2_GOOD_HOST_PARA"
    toggle "$TARGET_D" \
      "            let at = analysis
                .text_nodes
                .iter()
                .find(|n| n.run_span.is_some())
                .and_then(|n| enclosing_run(&slice, n.node_span.start))
                .map(|(s, _)| s)
                .unwrap_or(analysis.p_open_end_offset); // BUGGY_P3：欄位污染注入點
            out.replace_range(at..at, &inject_run);" \
      "            let at = analysis
                .ppr_end_offset
                .unwrap_or(analysis.p_open_end_offset);
            out.replace_range(at..at, &inject_run);" \
      "FLAG_P3_GOOD_INJECT_POINT"
    toggle "$TARGET_D" \
      "            if section_id != 0 {
                return Err(rejected(format!(\"BUGGY_P4 查無節 id={section_id}\")));
            }" \
      "            if section_id != 0 {
                return Err(MutationError::SubDocNotFound {
                    target: unknown_section_target(section_id),
                });
            }" \
      "FLAG_P4_GOOD_REJECT_TYPE"
    toggle "$TARGET_L" \
      "        // BUGGY_P5：bundle 基準被中途存檔件污染（undo byte_identical 假性失敗）" \
      "        fresh.doc.bundle = self.doc.bundle.clone();" \
      "FLAG_P5_GOOD_BUNDLE_BASELINE"
    echo "=== 正在執行正常測試（預期必綠）==="
    cargo test -p quire-engine --test k29d_section_hf_notes
    echo "GOOD 態測試結束，exit_code=0"
    ;;

  *)
    echo "用法: $0 buggy|good" >&2
    exit 2
    ;;
esac
