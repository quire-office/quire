#!/usr/bin/env python3
"""E2c 普查：全庫 docx 頁首／頁尾 part 的直排宣告（report-only，禁單檔調參實證）。

口徑：掃 `word/headerN.xml`／`word/footerN.xml` 兩類 part，命中兩式——
1. DrawingML `wps:bodyPr/@vert`（ST_TextVerticalType 全列：vert／vert270／
   vertStacked／eaVert／mongolianVert；**注意 vert270 含數字**，字元類漏
   `0-9` 會整列漏收——本腳本曾踩此坑，修正後全庫命中兩檔）；
2. VML `layout-flow:vertical`（n779627 鐵條族：LO 對拉丁文不觸發直排）。

輸出逐檔命中明細＋總數，考卷 lamp3（n779627 零 DIR 旗標）的第二檔實證來源。

用法：python3 scripts/e2c_survey_hf_vert.py [corpus]
"""
import pathlib
import re
import sys
import zipfile

VERT_TYPES = ("vert", "vert270", "vertStacked", "eaVert", "mongolianVert")


def main() -> int:
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "corpus")
    hits = []
    for p in sorted(root.rglob("*.docx")):
        try:
            with zipfile.ZipFile(p) as z:
                for name in z.namelist():
                    if not re.fullmatch(r"word/(header|footer)\d+\.xml", name):
                        continue
                    data = z.read(name).decode("utf-8", "replace")
                    found = []
                    for m in re.finditer(r'vert="([A-Za-z0-9]+)"', data):
                        if m.group(1) in VERT_TYPES:
                            found.append(f"bodyPr@vert={m.group(1)}")
                    if "layout-flow:vertical" in data:
                        found.append("vml layout-flow:vertical")
                    if found:
                        hits.append((str(p.relative_to(root)), name, sorted(set(found))))
        except Exception as e:  # noqa: BLE001
            print(f"SKIP {p}: {e}", file=sys.stderr)
    for f, n, h in hits:
        print(f"{f} :: {n} :: {', '.join(h)}")
    print(f"total hits: {len(hits)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
