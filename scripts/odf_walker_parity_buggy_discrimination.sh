#!/usr/bin/env bash
# 卡 t_4006056b 回塞鑑別力實跑腳本（持久版，禁只放 scratch）。
#
# 考卷 core/odf/tests/odf_walker_parity.rs（ODF 雙走查器口徑對齊）四盞燈的
# 鑑別面：
#   正常態：5 盞全綠（燈 1／燈 1b／燈 2／燈 3／燈 4）。
#   回塞態 A ODF_BODY_WS_BUGGY=1（body.rs 回到「零正規化＋CData 丟棄」舊路徑，
#     ＝卡前真實行為）⇒ 燈 1、燈 1b、燈 2、燈 3 必紅（body 側對齊臂真的在被
#     檢查），燈 4 綠（燈 4 在回塞態反斷言「兩軌必須不相等」）。
#   回塞態 B ODF_WS_BUGGY=1（layout_glue 文字節點走零正規化舊路徑＋CData 不入
#     文）⇒ 燈 1、燈 1b、燈 2、燈 3 同樣必紅（project 側分岔），燈 4 綠。
#   兩支旗標各自獨立＝各扳一側；同時扳（回塞態 C）時兩側同錯，燈 1／2／3 的
#   「逐段對等」臂會假綠，但燈 1 的「逐段釘死預期值」臂仍必須紅——證明本卷
#   不是只比兩軌互等（兩側同錯也過）的弱斷言。
#
# 乾淨副本＝git commit（分支 oc/walker-parity 尖端），本腳本只跑四種模式取退出碼。
# 用法：bash scripts/odf_walker_parity_buggy_discrimination.sh [日誌目錄]
set -u
export PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 RUST_MIN_STACK=16777216
LOGD="${1:-/tmp}"
T="--test odf_walker_parity"

must_red() { # $1=log $2=lamp
    grep -q "test $2 \.\.\. FAILED" "$1" || { echo "  缺紅: $2"; return 1; }
}

echo "== 正常態（期望：5 passed）=="
cargo test -p quire-odf $T > "$LOGD/owpar_clean.log" 2>&1
CLEAN_RC=$?
echo "CLEAN_RC=$CLEAN_RC"
grep -E "^test result" "$LOGD/owpar_clean.log"

echo "== 回塞態 A ODF_BODY_WS_BUGGY=1（期望：燈 1/1b/2/3 FAILED、燈 4 ok）=="
ODF_BODY_WS_BUGGY=1 cargo test -p quire-odf $T > "$LOGD/owpar_buggy_body.log" 2>&1
echo "BUGGY_BODY_RC=$?"
grep -E "^test lamp" "$LOGD/owpar_buggy_body.log"

echo "== 回塞態 B ODF_WS_BUGGY=1（期望：燈 1/1b/2/3 FAILED、燈 4 ok）=="
ODF_WS_BUGGY=1 cargo test -p quire-odf $T > "$LOGD/owpar_buggy_ws.log" 2>&1
echo "BUGGY_WS_RC=$?"
grep -E "^test lamp" "$LOGD/owpar_buggy_ws.log"

echo "== 回塞態 C 雙旗同扳（期望：燈 1 仍紅——釘值臂抓兩側同錯）=="
ODF_BODY_WS_BUGGY=1 ODF_WS_BUGGY=1 cargo test -p quire-odf $T > "$LOGD/owpar_buggy_both.log" 2>&1
echo "BUGGY_BOTH_RC=$?"
grep -E "^test lamp" "$LOGD/owpar_buggy_both.log"

echo "== 鑑別判定 =="
MISS=""
if [ "$CLEAN_RC" -ne 0 ]; then MISS="$MISS 正常態非全綠"; fi
for log in "$LOGD/owpar_buggy_body.log" "$LOGD/owpar_buggy_ws.log"; do
    for lamp in \
        lamp1_walker_streams_are_char_identical \
        lamp1b_concatenated_streams_equal_and_free_of_formatting_newlines \
        lamp2_cdata_is_not_silently_dropped \
        lamp3_nbsp_untouched_on_both_walkers; do
        must_red "$log" "$lamp" || MISS="$MISS $(basename "$log"):$lamp"
    done
done
# 燈 4 在三個回塞態都必須綠（它自己帶 env 分支：回塞態斷言兩軌分岔）。
for log in "$LOGD/owpar_buggy_body.log" "$LOGD/owpar_buggy_ws.log" "$LOGD/owpar_buggy_both.log"; do
    grep -q "test lamp4_buggy_flag_discriminates_the_body_normalization_arm \.\.\. ok" "$log" \
        || MISS="$MISS $(basename "$log"):lamp4 未綠"
done
# 回塞態 C：燈 1 必須仍紅（逐段釘死預期值臂，防「兩側同錯也過」）。
must_red "$LOGD/owpar_buggy_both.log" lamp1_walker_streams_are_char_identical \
    || MISS="$MISS 回塞態C:燈1 未紅（釘值臂失效）"

if [ -z "$MISS" ]; then
    echo "DISCRIMINATION=OK（正常態全綠；三回塞態鑑別齊備，見 $LOGD/owpar_*.log）"
else
    echo "DISCRIMINATION=FAIL（$MISS）"
    exit 1
fi
