#!/usr/bin/env bash
# R1-b（根因 2：recompute_notes 收斂種子化）BUGGY 回塞鑑別腳本——先紅後綠的機械證明隨貨。
#
# 口徑（比照 scripts/e2b_underline_buggy_replay.sh）：生產碼內建
# `R1B_NOTE_SEED_BUGGY` 開關（core/engine-api/src/lib.rs）——
#   正常態：compute_footnotes/compute_header_push 的 `applied` 從現行
#           params.footnote_reserves_pt / header_pushes_pt 起步，且每輪
#           「同表跳過 update_params」；尾註檔另有定案快速徑。
#   回塞態：applied 刷回空表起步＋跳過定案快速徑＝舊行為（零變動也無條件
#           清斷行快取整篇重排）。
# 考卷 core/engine-api/tests/r1b_note_seed.rs 在進程內**不設 env**（紀律），
# 回塞態由本腳本在外部 export：燈 1（零變動編輯 relayout 增量必須 0）必紅。
set -u
cd "$(dirname "$0")/.."

echo "=== 正常態：r1b_note_seed 考卷預期全綠 ==="
if ! cargo test -p quire-engine --test r1b_note_seed 2>&1 | tail -3; then
    echo "✗ 正常態未全綠——先修正常態再談回塞鑑別"
    exit 1
fi

export R1B_NOTE_SEED_BUGGY=1
echo "=== 回塞態（R1B_NOTE_SEED_BUGGY=1）：燈 1 必紅 ==="
if cargo test -p quire-engine --test r1b_note_seed 2>&1 | grep -q "test result: FAILED"; then
    echo "✓ BUGGY 態回塞鑑別通過（燈 1 點名 relayout 增量 >0）"
    rc=0
else
    echo "✗ 回塞鑑別失敗：BUGGY 態仍全綠＝考卷無鑑別力（修的是別處？）"
    rc=1
fi
unset R1B_NOTE_SEED_BUGGY || true
exit $rc
