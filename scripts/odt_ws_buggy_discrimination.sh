#!/usr/bin/env bash
# t_ae9a9f22 回塞鑑別力實跑腳本（持久版，禁只放 scratch）；t_2a983570 返修擴充。
#
# 正常態：十燈全綠；回塞態 ODF_WS_BUGGY=1（layout_glue.rs 文字節點走零正規化
# 舊路徑＝直接 push_str(&unescaped)）⇒ 燈 3（段行數 1→4）與燈 4（幽靈空項 0→2）
# 必紅（正規化臂真的在被檢查）；燈 7–10（跨節點黏連：首尾格式化空白 gap 記帳＋
# 純空格節點不折疊）在回塞態同樣必紅（gap 語義臂真的在被檢查）。
# 乾淨副本＝git commit（分支 oc/ws-gap 尖端），本腳本只跑兩種模式取退出碼。
#
# 用法：bash scripts/odt_ws_buggy_discrimination.sh   （於 repo 根或 worktree 根執行）
set -u
export PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 RUST_MIN_STACK=16777216
LOGD="${1:-/tmp}"

echo "== 正常態（期望：10 passed / 0 failed）=="
cargo test -p quire-odf --test odf_xml_whitespace_policy > "$LOGD/odfws_lamps_clean.log" 2>&1
echo "CLEAN_RC=$?"
grep -E "^test (lamp|test result)" "$LOGD/odfws_lamps_clean.log" | tail -12

echo "== 回塞態 ODF_WS_BUGGY=1（期望：燈 3／燈 4／燈 7–10 FAILED）=="
ODF_WS_BUGGY=1 cargo test -p quire-odf --test odf_xml_whitespace_policy > "$LOGD/odfws_lamps_buggy.log" 2>&1
echo "BUGGY_RC=$?"
grep -E "^test lamp" "$LOGD/odfws_lamps_buggy.log"

echo "== 鑑別判定 =="
MISS=""
for lamp in \
    lamp3_pretty_printed_paragraph_breaks_to_exactly_one_line \
    lamp4_emitter_drops_ghost_empty_items_and_keeps_true_empty_paragraph_line \
    lamp7_text_node_trailing_format_ws_earns_a_gap \
    lamp8_text_node_leading_format_ws_earns_a_gap \
    lamp9_pure_space_node_between_spans_is_not_folded \
    lamp10_entity_ref_boundary_not_glued; do
    grep -q "test ${lamp} \.\.\. FAILED" "$LOGD/odfws_lamps_buggy.log" || MISS="$MISS $lamp"
done
if [ -z "$MISS" ]; then
    echo "DISCRIMINATION=OK（燈 3、燈 4、燈 7–10 回塞態皆紅）"
else
    echo "DISCRIMINATION=FAIL（回塞態未拍紅：$MISS，見 $LOGD/odfws_lamps_buggy.log）"
    exit 1
fi
