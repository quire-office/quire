#!/usr/bin/env python3
"""ODF-C3 紅燈計數基準腳本（獨立於 Rust 實作的第二雙眼）。

口徑（與 docs/odf-survey/survey.json 的 count 正則一致，避免解析器差異）：
  <text:p> 計數 = 正則 <text:p[\\s>/]（Start＋Empty 皆計）
  <text:h> 計數 = 正則 <text:h[\\s>/]
另記：XML 是否健全（iterparse 是否拋錯）、fo:font-size=\"..pt\" 顯式宣告清單、
加密條目presence。輸出 JSON 到 stdout。
"""
import io
import json
import re
import sys
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

ROOT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("corpus-odt")

RE_P = re.compile(rb"<text:p[\s>/]")
RE_H = re.compile(rb"<text:h[\s>/]")
RE_PT = re.compile(rb'fo:font-size="([^"]*pt)"')
RE_ANY_PT = re.compile(rb'fo:font-size="([^"]*)"')
RE_ENCRYPT = re.compile(rb"<draw:frame[^>]*style:background-image|encryption")


def xml_well_formed(raw: bytes) -> bool:
    try:
        for _ in ET.iterparse(io.BytesIO(raw), events=("start", "end")):
            pass
        return True
    except Exception:
        return False


def main():
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    rows = []
    total_p = total_h = 0
    for rec in manifest["files"]:
        rel = rec["file"]
        zf = zipfile.ZipFile(io.BytesIO((ROOT / rel).read_bytes()))
        names = zf.namelist()
        content = zf.read("content.xml") if "content.xml" in names else b""
        styles = zf.read("styles.xml") if "styles.xml" in names else b""
        n_p = len(RE_P.findall(content))
        n_h = len(RE_H.findall(content))
        total_p += n_p
        total_h += n_h
        rows.append(
            {
                "file": rel,
                "text_p": n_p,
                "text_h": n_h,
                "paragraphs": n_p + n_h,
                "has_text_h": b"<text:h" in content,
                "content_well_formed": xml_well_formed(content) if content else False,
                "pt_content": sorted({x.decode() for x in RE_PT.findall(content)}),
                "pt_styles": sorted({x.decode() for x in RE_PT.findall(styles)}),
                "fontsize_content_all": sorted({x.decode() for x in RE_ANY_PT.findall(content)}),
                "n_entries": len(names),
                "kind": rec.get("kind", ""),
            }
        )
    out = {
        "count": len(rows),
        "total_text_p": total_p,
        "total_text_h": total_h,
        "total_paragraphs": total_p + total_h,
        "files_with_headings": [r["file"] for r in rows if r["has_text_h"]],
        "files_not_well_formed": [r["file"] for r in rows if not r["content_well_formed"]],
        "files_with_pt": [
            {
                "file": r["file"],
                "kind": r["kind"],
                "pt_content": r["pt_content"],
                "pt_styles": r["pt_styles"],
            }
            for r in rows
            if r["pt_content"] or r["pt_styles"]
        ],
        "rows": rows,
    }
    print(json.dumps(out, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
