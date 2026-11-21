#!/usr/bin/env python3
"""E 卡補充探針：`w:space` 單位查證（pt 還是 twip？）＋外框半線寬偏移量測。

問卷（docs/e-tblborder-redlight.md §5）：mutation 寫回的 `@w:space` 語義單位。
做法——同結構單格表，border 的 space 依序 0／12／240，看 LO 是否把框線／文字
基線推開、推開多少：
  - 若 space=12 讓 y 位移 ≈ 12pt ⇒ 單位＝pt
  - 若位移 ≈ 0.6pt（=12/20）⇒ 單位＝twip
  - 若零位移 ⇒ LO 不消費 table border space（本欄照實寫入、宣稱「存檔保真」）
另量外框：LO 頂線 path y 與我方中心線 y 的差＝外框是否外飄半線寬。

用法：/usr/bin/python3 scripts/e_tblborder_space_probe.py
"""
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

import pymupdf

ROOT = Path(__file__).resolve().parents[1]
SOFFICE = Path.home() / ".local/lo-linux/opt/libreoffice25.8/program/soffice"
W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'


def doc(space):
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<w:document {W}><w:body>'
        '<w:tbl><w:tblPr><w:tblW w:w="4320" w:type="dxa"/><w:tblLayout w:type="fixed"/>'
        '<w:tblBorders>'
        f'<w:top w:val="single" w:sz="8" w:space="{space}" w:color="FF0000"/>'
        '<w:left w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
        '<w:bottom w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
        '<w:right w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
        '<w:insideH w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
        '<w:insideV w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
        '</w:tblBorders></w:tblPr>'
        '<w:tblGrid><w:gridCol w:w="4320"/></w:tblGrid>'
        '<w:tr><w:tc><w:tcPr><w:tcW w:w="4320" w:type="dxa"/></w:tcPr>'
        '<w:p><w:r><w:t>space probe</w:t></w:r></w:p></w:tc></w:tr></w:tbl>'
        '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>'
        '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
        'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
        '</w:body></w:document>'
    )


CT = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Override PartName="/word/document.xml" '
    'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
    '</Types>'
)
RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
    'Target="word/document.xml"/></Relationships>'
)


def build(path, space):
    import io
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CT)
        z.writestr("_rels/.rels", RELS)
        z.writestr("word/document.xml", doc(space))
    Path(path).write_bytes(buf.getvalue())


def lines(pdf):
    d = pymupdf.open(pdf)
    out = []
    for pg in d:
        for dr in pg.get_drawings():
            for it in dr["items"]:
                if it[0] == "l":
                    out.append((round(it[1].x, 2), round(it[1].y, 2),
                                round(it[2].x, 2), round(it[2].y, 2),
                                round(dr.get("width") or 0, 3)))
    return out


def main():
    prof = tempfile.mkdtemp(prefix="lo-e-space-")
    out = tempfile.mkdtemp(prefix="e-space-")
    prev_y = None
    for space in (0, 12, 240):
        dx = Path(out) / f"s{space}.docx"
        build(dx, space)
        r = subprocess.run(
            [str(SOFFICE), "--headless", "--norestore", "--nolockcheck", "--nodefault",
             f"-env:UserInstallation=file://{prof}", "--convert-to", "pdf",
             "--outdir", out, str(dx)], capture_output=True, text=True, timeout=180)
        pdf = Path(out) / f"s{space}.pdf"
        assert pdf.exists(), f"LO 未產 PDF space={space}: rc={r.returncode} err={r.stderr[:300]}"
        ls = lines(pdf)
        ys = [l[1] for l in ls]
        txt = pymupdf.open(pdf)[0].get_text("dict")
        spans = [(round(s["bbox"][1], 2), s["text"])
                 for b in txt["blocks"] for l in b.get("lines", []) for s in l["spans"]]
        print(f"space={space}: lines={ls} text_top={spans}")
        prev_y = ys
    print("判讀：比較三組的 line y 與 text_top 位移量 ⇒ 單位 pt / twip / 不消費")


if __name__ == "__main__":
    sys.exit(main())
