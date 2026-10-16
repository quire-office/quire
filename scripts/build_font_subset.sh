#!/usr/bin/env bash
# Phase0-4：由 Noto Sans CJK TC（OFL）產生 quire 內建字型子集。
#
# 產物：core/layout/assets/quire-notosanstc-subset.otf（進 git，引擎 include_bytes! 內嵌）
# 重產生：bash scripts/build_font_subset.sh [來源 otf 路徑]
#
# 涵蓋範圍＝「真實需求 ∪ 安全底」：
#   1) corpus/ 全部 docx 的 XML 字元（含屬性）＋ fixtures/ ＋ 所有 .rs 字串字面值
#      （測試用字元必須被涵蓋，否則斷行寬度走估值 fallback）
#   2) 常用 Unicode 區塊：ASCII、Latin-1、通用標點、CJK 標點/筆畫/假名/漢字區、
#      全形、康熙部首、FE 兼容標點——多留的 glyph 對體積影響小，但避免常見文字掉寬度。
set -euo pipefail
cd "$(dirname "$0")/.."

SRC="${1:-$HOME/.fonts/NotoSansCJKtc-Regular.otf}"
OUT="core/layout/assets/quire-notosanstc-subset.otf"
PY="${PY:-python3}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ---- 1. 掃出真實用到的字元 ----
python3 scripts/font_subset_chars.py "$WORK/chars.txt"

# ---- 2. pyftsubset ----
#   layout-features=* ：保留全部 OpenType 特性（liga/vert/halt…shaping 才真實）
#   保留 cmap/name 完整，讓 fontique/swash 能按家族名註冊與比對。
mkdir -p "$(dirname "$OUT")"
pyftsubset "$SRC" \
  --text-file="$WORK/chars.txt" \
  --unicodes='U+0020-007E,U+00A0-00FF,U+2000-206F,U+2E80-2FFF,U+3000-303F,U+FE10-FE1F,U+FF00-FF65,U+FFE0-FFE6,U+2190-21FF,U+2200-22FF,U+2460-24FF,U+25A0-27BF,U+3200-33FF' \
  --layout-features='*' \
  --no-hinting \
  --drop-tables+=DSIG,BASE \
  --output-file="$OUT"

# ---- 3. OFL 保留名稱（FRN）合規改名 ----
"$PY" scripts/font_subset_rename.py "$OUT"

ls -l "$OUT"
echo "✓ 子集已產生：$OUT"
