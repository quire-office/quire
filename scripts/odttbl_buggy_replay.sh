#!/usr/bin/env bash
# AGY-ODTTBL BUGGY 回塞與鑑別腳本（支援 buggy 與 good 兩態）。
#
# 用法：
#   bash scripts/odttbl_buggy_replay.sh buggy  # 啟用 ODF_TABLE_BUGGY，考卷燈 1–4 必全紅
#   bash scripts/odttbl_buggy_replay.sh good   # 正常狀態，考卷燈 1–4 必全綠
#   bash scripts/odttbl_buggy_replay.sh        # 依序驗證兩態（buggy 必紅、good 必綠）

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=16777216

run_buggy() {
    echo "=== [BUGGY 態] 注入 ODF_TABLE_BUGGY=1（走訪器退回不攔表格邊界）==="
    set +e
    ODF_TABLE_BUGGY=1 cargo test -p quire-odf --test odf_table_flow
    local exit_code=$?
    set -e
    if [ "$exit_code" -ne 0 ]; then
        echo "[FLAG_ODTTBL_BUGGY_DISCRIMINATED] BUGGY 態如預期失敗（exit code: $exit_code），紅燈鑑別成功！"
        return 0
    else
        echo "[ERROR] BUGGY 態竟測試通過，無法鑑別回塞瑕疵！"
        return 1
    fi
}

run_good() {
    echo "=== [GOOD 態] 正常排版模型（二維表格流攔截與排版發射）==="
    unset ODF_TABLE_BUGGY || true
    cargo test -p quire-odf --test odf_table_flow
    echo "[FLAG_ODTTBL_GOOD_VERIFIED] GOOD 態全綠通過，正常功能驗證無誤！"
}

case "${1:-all}" in
    buggy)
        run_buggy
        ;;
    good)
        run_good
        ;;
    all)
        run_buggy
        echo ""
        run_good
        echo ""
        echo "=== AGY-ODTTBL 雙態驗證完成：buggy 必紅、good 必綠 ==="
        ;;
    *)
        echo "Usage: $0 [buggy|good|all]"
        exit 1
        ;;
esac
