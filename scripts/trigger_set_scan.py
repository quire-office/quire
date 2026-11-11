#!/usr/bin/env python3
"""trigger_set_scan.py — 掃描 corpus 中含橫向 section / 混合走向特徵的觸發集檔案。

用法：
    python3 scripts/trigger_set_scan.py ['w:orient'] ['w:sectPr']
"""

from __future__ import annotations

import glob
import os
import pathlib
import sys
import zipfile
import xml.etree.ElementTree as ET

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent


def scan_docx_file(path: pathlib.Path) -> dict | None:
    try:
        with zipfile.ZipFile(path) as z:
            if "word/document.xml" not in z.namelist():
                return None
            xml_data = z.read("word/document.xml")
    except Exception:
        return None

    if b"sectPr" not in xml_data:
        return None

    ns = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
    try:
        root = ET.fromstring(xml_data)
    except Exception:
        return None

    body = root.find("w:body", ns)
    if body is None:
        return None

    sections = []
    for elem in body.iter(f"{{{ns['w']}}}sectPr"):
        pgSz = elem.find("w:pgSz", ns)
        if pgSz is not None:
            w = int(pgSz.attrib.get(f"{{{ns['w']}}}w", 11906))
            h = int(pgSz.attrib.get(f"{{{ns['w']}}}h", 16838))
            orient = pgSz.attrib.get(f"{{{ns['w']}}}orient")
            is_land = (orient == "landscape") or (w > h)
        else:
            w, h, orient = 11906, 16838, None
            is_land = False
        sections.append({
            "w": w,
            "h": h,
            "orient": orient,
            "is_landscape": is_land,
        })

    if not sections:
        return None

    has_landscape = any(s["is_landscape"] for s in sections)
    has_portrait = any(not s["is_landscape"] for s in sections)
    is_mixed = has_landscape and has_portrait

    return {
        "path": str(path.relative_to(REPO_ROOT)),
        "sec_count": len(sections),
        "has_landscape": has_landscape,
        "is_mixed": is_mixed,
        "orient_seq": "".join("L" if s["is_landscape"] else "P" for s in sections),
    }


def main():
    corpus_dir = REPO_ROOT / "corpus"
    files = sorted(corpus_dir.rglob("*.docx"))
    print("=" * 80)
    print(f"Corpus 觸發集掃描（w:orient & w:sectPr）")
    print(f"掃描根目錄: {corpus_dir} (共 {len(files)} 個 docx)")
    print("=" * 80)

    all_results = []
    for f in files:
        res = scan_docx_file(f)
        if res:
            all_results.append(res)

    land_files = [r for r in all_results if r["has_landscape"]]
    mixed_files = [r for r in all_results if r["is_mixed"]]

    print(f"\n1. 全檔案 sectPr 概況：")
    print(f"  - 含有 sectPr 之檔案數: {len(all_results)}")
    print(f"  - 含有橫向宣告（w:orient=\"landscape\" 或 w>h）檔案數: {len(land_files)}")
    print(f"  - 含有「混合走向」（直向＋橫向並存）檔案數: {len(mixed_files)}")

    print(f"\n2. 混合走向觸發集清單（共 {len(mixed_files)} 檔）：")
    print(f"{'檔名':<60}{'Sec數':<8}{'走向序列'}")
    print("-" * 80)
    for r in mixed_files:
        print(f"{r['path']:<60}{r['sec_count']:<8}{r['orient_seq']}")

    print(f"\n3. 純橫向檔案清單（共 {len(land_files) - len(mixed_files)} 檔）：")
    for r in land_files:
        if not r["is_mixed"]:
            print(f"  {r['path']} (Sec數: {r['sec_count']}, 序列: {r['orient_seq']})")

    print("\n" + "=" * 80)


if __name__ == "__main__":
    main()
