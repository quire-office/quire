#!/usr/bin/env python3
"""ODF-D1 探針（非考卷）：從 DOCX 線真實 `dump` 輸出抽取顯示列表 JSON 契約鍵集合。

用途：產出 `fixtures/display_list_contract.json`——ODT 發射層（`core/odf/src/display.rs`）
的欄位名對照基準（燈 1 schema 斷言的 DOCX 側真相來源）。**基準檔只准由本腳本從
DOCX 線真實產物生成，不准手寫**（手寫＝兩線各烤一套欄位的破口）；DOCX 側另有
`shells/native/tests/odf_d1_dump_contract.rs` 反查本檔未漂移，兩側互鎖。

口徑：
- `top_level_keys`／`variant_keys`＝逐變體鍵名**聯集**（涵蓋全樣本出現過的欄位）。
- `variant_required_keys`＝逐變體鍵名**交集**（每個樣本項都發的欄位＝必要欄位；
  ODT 切片至少必須發齊這些，否則視為漏欄位）。
- `family` 在 DOCX 側只在文件宣告字型族時才發（serde `skip_serializing_if`），
  故落在聯集不落交集——ODT 側發 None（跳欄）是合法子集，非漏欄。
"""
import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CLI = REPO / "target" / "debug" / "quire-cli"
OUT = REPO / "fixtures" / "display_list_contract.json"

# 覆蓋四種變體（Text/Rect/Rule/Image）的 DOCX 側樣本檔。
SAMPLES = [
    "fixtures/sample.docx",
    "fixtures/table_mixed.docx",
    "fixtures/hf_table_header.docx",
    "fixtures/inline_image.docx",
    "corpus/ooxmlexport/k4c_image_emit.docx",
]

TOP: set[str] = set()
UNION: dict[str, set[str]] = {}
INTERSECT: dict[str, set[str]] = {}
PAGES = 0
ITEMS = 0


def walk(path: str) -> None:
    global PAGES, ITEMS
    r = subprocess.run([str(CLI), "dump", path], cwd=REPO, capture_output=True, text=True)
    if r.returncode != 0:
        print(f"跳過 {path}: {r.stderr.strip()[:120]}", file=sys.stderr)
        return
    for dl in json.loads(r.stdout):
        PAGES += 1
        TOP.update(dl.keys())
        for it in dl["items"]:
            ITEMS += 1
            for variant, fields in it.items():
                keys = set(fields.keys())
                UNION.setdefault(variant, set()).update(keys)
                INTERSECT[variant] = keys if variant not in INTERSECT else INTERSECT[variant] & keys


def main() -> int:
    if not CLI.exists():
        print(f"先 build：{CLI} 不存在（cargo build -p quire-cli）", file=sys.stderr)
        return 1
    seen = []
    for s in SAMPLES:
        if (REPO / s).exists():
            walk(s)
            seen.append(s)
        else:
            print(f"樣本檔不存在：{s}", file=sys.stderr)
    if not seen:
        print("無樣本可抽，拒絕產出空契約檔", file=sys.stderr)
        return 1
    out = {
        "source_samples": seen,
        "top_level_keys": sorted(TOP),
        "variant_keys": {k: sorted(v) for k, v in sorted(UNION.items())},
        "variant_required_keys": {k: sorted(v) for k, v in sorted(INTERSECT.items())},
        "pages": PAGES,
        "items": ITEMS,
    }
    OUT.write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"✓ 寫入 {OUT}（{PAGES} 頁／{ITEMS} 項／變體 {sorted(UNION)}）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
