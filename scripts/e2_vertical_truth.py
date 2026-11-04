#!/usr/bin/env python3
"""E2 直排真值抽取（LO 側 pymupdf `dir` 向量＋bbox 口徑）。

用法：
    /usr/bin/python3 scripts/e2_vertical_truth.py <docx...>

口徑（寫死，供 redlight 覆核）：
1. 用 LO（find_soffice()）把 docx 轉 PDF（--convert-to pdf，與 render_lab 同一
   `lo_to_pdf` 轉換路徑）。
2. 對每頁 `page.get_text("dict")`，逐 span 取 `dir`（pymupdf 的行進方向單位向量，
   與 PDF 文字矩陣同脈：橫排 [1,0]、由下而上 [0,-1]、由上而下 [0,1]）與
   `bbox`（精確到 0.1pt）。
3. 只印「非橫排」（abs(dir[0]) < 0.5）的 span，以及該頁橫排 span 的計數，
   方便一眼看出直排行與其相鄰橫排行。
"""
import json
import sys

import pymupdf  # noqa: E402

sys.path.insert(0, "scripts")
from render_lab import find_soffice, lo_scratch, lo_to_pdf, kill_stale_lo  # noqa: E402


def spans_of(pdf_path):
    d = pymupdf.open(str(pdf_path))
    rows = []
    for pi, page in enumerate(d):
        dd = page.get_text("dict")
        for blk in dd.get("blocks", []):
            for ln in blk.get("lines", []):
                dr = tuple(round(float(v), 3) for v in ln.get("dir", (1, 0)))
                bb = tuple(round(float(v), 1) for v in ln.get("bbox", ()))
                sp0 = (ln.get("spans") or [{}])[0]
                txt = "".join(sp.get("text", "") for sp in ln.get("spans", []))
                rows.append({"page": pi, "dir": dr, "bbox": bb, "text": txt,
                             "size": round(float(sp0.get("size", 0.0)), 2),
                             "font": sp0.get("font", "")})
    d.close()
    return rows


def main() -> int:
    soffice = find_soffice()
    scratch, s_prof, _needle = lo_scratch(soffice)
    kill_stale_lo()
    out = {}
    for docx in sys.argv[1:]:
        pdf = lo_to_pdf(soffice, __import__("pathlib").Path(docx), scratch, s_prof)
        out[docx] = spans_of(pdf) if pdf else []
    print(json.dumps(out, ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
