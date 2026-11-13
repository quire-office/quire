#!/usr/bin/env python3
"""E 卡 LO 真值探針：fixtures/e_tblborder_probe.docx → PDF 線段逐條抽取。

口徑（quire-redlight-conventions）：
- soffice Linux 版 + 獨立 UserInstallation（避免誤落 CWD）。
- pymupdf get_drawings()：stroke 線的 width（pt）與 rect（pt，PDF y 軸向下）。
- 輸出逐條 JSON：path type / width / rect / items 首尾座標。

用法：/usr/bin/python3 scripts/e_tblborder_lo_probe.py
"""
import json
import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

import pymupdf

ROOT = Path(__file__).resolve().parents[1]
FIX = ROOT / "fixtures" / "e_tblborder_probe.docx"
SOFFICE = Path.home() / ".local/lo-linux/opt/libreoffice25.8/program/soffice"


def ensure_fixture():
    if not FIX.exists():
        subprocess.run([sys.executable, str(ROOT / "scripts/gen_e_tblborder_fixture.py")],
                       check=True)
    zf = zipfile.ZipFile(FIX)
    assert zf.testzip() is None


def to_pdf(docx: Path, outdir: Path) -> Path:
    prof = tempfile.mkdtemp(prefix="lo-e-tbl-")
    r = subprocess.run(
        [str(SOFFICE), "--headless", "--norestore", "--nolockcheck", "--nodefault",
         f"-env:UserInstallation=file://{prof}",
         "--convert-to", "pdf", "--outdir", str(outdir), str(docx)],
        capture_output=True, text=True, timeout=180)
    pdf = outdir / (docx.stem + ".pdf")
    assert pdf.exists(), f"LO 未產 PDF: rc={r.returncode} out={r.stdout} err={r.stderr}"
    return pdf


def main():
    ensure_fixture()
    outdir = Path(tempfile.mkdtemp(prefix="e-tbl-pdf-"))
    pdf = to_pdf(FIX, outdir)
    doc = pymupdf.open(pdf)
    print(f"pages={len(doc)}")
    for pno, page in enumerate(doc):
        print(f"--- page {pno} size={page.rect}")
        for d in page.get_drawings():
            color = d.get("color")
            w = d.get("width")
            for item in d["items"]:
                kind = item[0]
                if kind == "l":
                    p1, p2 = item[1], item[2]
                    rec = {"t": "line", "x1": round(p1.x, 2), "y1": round(p1.y, 2),
                           "x2": round(p2.x, 2), "y2": round(p2.y, 2),
                           "w": None if w is None else round(w, 3),
                           "rgb": None if color is None else [round(c, 3) for c in color],
                           "dashes": d.get("dashes")}
                    print(json.dumps(rec, ensure_ascii=False))
                elif kind == "re":
                    r = item[1]
                    rec = {"t": "rect", "x0": round(r.x0, 2), "y0": round(r.y0, 2),
                           "x1": round(r.x1, 2), "y1": round(r.y1, 2),
                           "w": None if w is None else round(w, 3),
                           "rgb": None if color is None else [round(c, 3) for c in color],
                           "dashes": d.get("dashes")}
                    print(json.dumps(rec, ensure_ascii=False))
                else:
                    rec = {"t": kind, "w": None if w is None else round(w, 3),
                           "rgb": None if color is None else [round(c, 3) for c in color]}
                    print(json.dumps(rec, ensure_ascii=False))


if __name__ == "__main__":
    main()
