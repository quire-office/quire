#!/usr/bin/env bash
# 分層鐵律：core 不得依賴 shells；shells/native 只准「直接」import engine。
# 用 cargo tree --depth 1 檢查直接依賴（engine 經erre 傳遞 core 是合法的）。
set -euo pipefail
cd "$(dirname "$0")/.."

CORES="quire-ooxml quire-layout quire-render-trait quire-crdt quire-engine"
for c in $CORES; do
  if cargo tree -p "$c" --depth 1 -e normal 2>/dev/null | grep -E "quire-(web|cli)"; then
    echo "✗ 分層違規：core crate $c 直接依賴了 shell"; exit 1
  fi
done
if cargo tree -p quire-cli --depth 1 -e normal 2>/dev/null | grep -E "quire-(ooxml|layout|render-trait|crdt)"; then
  echo "✗ 分層違規：quire-cli 直接 import 了 engine 以下的 core crate（只能經 quire-engine）"; exit 1
fi
if cargo tree -p quire-web --depth 1 -e normal 2>/dev/null | grep -E "quire-(ooxml|layout|render-trait|crdt)"; then
  echo "✗ 分層違規：quire-web 直接 import 了 engine 以下的 core crate"; exit 1
fi

# ---- Web-2 拆包鐵律：CRDT 只准活在協作分塊 ----
# 主 wasm 連「傳遞」依賴都不准出現 crdt（防 engine 預設帶上 collab feature）。
if cargo tree -p quire-web --target wasm32-unknown-unknown -e normal 2>/dev/null | grep -qE "quire-crdt"; then
  echo "✗ 拆包違規：quire-web（主 wasm）依賴了 quire-crdt——CRDT 不進首包"; exit 1
fi
# 協作分塊必須真的帶上 crdt（防 feature 斷線造成「假分塊」：分塊存在但沒內容）。
if ! cargo tree -p quire-web-collab --target wasm32-unknown-unknown -e normal 2>/dev/null | grep -qE "quire-crdt"; then
  echo "✗ 拆包違規：quire-web-collab 沒有依賴 quire-crdt——collab feature 斷線"; exit 1
fi
echo "✓ 分層檢查通過（core↛shells；shells→engine 唯一入口；CRDT 僅在協作分塊）"
