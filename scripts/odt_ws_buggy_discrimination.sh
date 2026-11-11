#!/usr/bin/env bash
# t_ae9a9f22 回塞鑑別力實跑腳本（持久版，禁只放 scratch）。
#
# 正常態：六燈全綠；回塞態 ODF_WS_BUGGY=1（layout_glue.rs 文字節點走零正規化
# 舊路徑＝直接 push_str(&unescaped)）⇒ 燈 3（段行數 1→4）與燈 4（幽靈空項 0→2）
# 必紅，並連帶燈 2／燈 5 破（正規化臂真的在被檢查）。
# 乾淨副本＝git commit（分支 oc/odt-whitespace 尖端），本腳本只跑兩種模式取退出碼。
#
# 用法：bash scripts/odt_ws_buggy_discrimination.sh   （於 repo 根或 worktree 根執行）
set -u
export PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 RUST_MIN_STACK=16777216
LOGD="${1:-/tmp}"

echo "== 正常態（期望：6 passed / 0 failed）=="
cargo test -p quire-odf --test odf_xml_whitespace_policy > "$LOGD/odfws_lamps_clean.log" 2>&1
echo "CLEAN_RC=$?"
grep -E "^test (lamp|test result)" "$LOGD/odfws_lamps_clean.log" | tail -8

echo "== 回塞態 ODF_WS_BUGGY=1（期望：燈 3／燈 4 FAILED）=="
ODF_WS_BUGGY=1 cargo test -p quire-odf --test odf_xml_whitespace_policy > "$LOGD/odfws_lamps_buggy.log" 2>&1
echo "BUGGY_RC=$?"
grep -E "^test lamp" "$LOGD/odfws_lamps_buggy.log"

echo "== 鑑別判定 =="
if grep -q "lamp3_pretty_printed_paragraph_breaks_to_exactly_one_line ... FAILED" "$LOGD/odfws_lamps_buggy.log" \
   && grep -q "lamp4_emitter_drops_ghost_empty_items_and_keeps_true_empty_paragraph_line ... FAILED" "$LOGD/odfws_lamps_buggy.log"; then
    echo "DISCRIMINATION=OK（燈 3、燈 4 回塞態皆紅）"
else
    echo "DISCRIMINATION=FAIL（回塞態未拍紅燈 3／燈 4，見 $LOGD/odfws_lamps_buggy.log）"
    exit 1
fi
