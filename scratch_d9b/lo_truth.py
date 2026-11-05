"""LO pymupdf images+bbox 真值抽取：對 D9b inset 候選檔逐檔輸出。

口徑（照 quire-redlight-conventions）：
  soffice --headless --norestore --convert-to pdf（LO 25.8，獨立 UserInstallation）
  pymupdf：page.get_images(full=True) + page.get_image_rects(xref) 取 bbox
  另取 page.get_drawings() 的矩形（框線幾何參考）
"""
import os
import subprocess
import sys
import zipfile
import xml.etree.ElementTree as ET

import fitz  # pymupdf

LO = os.path.expanduser("~/.local/lo-linux/opt/libreoffice25.8/program/soffice")
OUT = os.path.expanduser("/tmp/d9b_truth")
os.makedirs(OUT, exist_ok=True)

CANDS = [
    "corpus/ooxmlexport/fdo76591.docx",
    "corpus/ooxmlexport/n792778.docx",
    "corpus/ooxmlimport/tdf48658_transparentOLEheader.docx",
    "corpus/ooxmlexport/tdf102466.docx",
    "corpus/ooxmlexport/fdo61343.docx",
    "corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx",
    "corpus/xopt/NewDocxDocuments_CoverPage.docx",
    "corpus/xopt/TestFiles_HC031-Complicated-Document.docx",
    "corpus/ooxmlimport/groupshape-sdt.docx",
    "corpus/ooxmlexport/dkvert.docx",
    "corpus/ooxmlexport/mce-nested.docx",
    "corpus/ooxmlimport/n779627.docx",
    "corpus/ooxmlexport/fdo66688.docx",
    "corpus/ooxmlexport/fdo69636.docx",
    "corpus/ooxmlimport/btlr-frame-vml.docx",
    "corpus/ooxmlexport/fdo78432.docx",
    "corpus/ooxmlexport/fdo78651.docx",
    "corpus/ooxmlexport/fdo79008.docx",
    "corpus/ooxmlexport/fdo66929.docx",
    "corpus/ooxmlexport/fdo69616.docx",
    "corpus/ooxmlexport/LinkedTextBoxes.docx",
]

def convert(rel):
    stem = os.path.basename(rel).replace(".docx", "")
    pdf = os.path.join(OUT, stem + ".pdf")
    if os.path.exists(pdf):
        return pdf
    prof = f"file:///tmp/d9b_prof_{stem}"
    p = subprocess.run(
        [LO, "--headless", "--norestore", f"-env:UserInstallation={prof}",
         "--convert-to", "pdf", "--outdir", OUT, rel],
        capture_output=True, text=True, timeout=300)
    if not os.path.exists(pdf):
        print(f"  CONVERT FAIL {rel}: {p.stdout[:100]} {p.stderr[:100]}")
        return None
    return pdf

for rel in CANDS:
    stem = os.path.basename(rel).replace(".docx", "")
    pdf = convert(rel)
    if not pdf:
        continue
    doc = fitz.open(pdf)
    print(f"== {rel} pages={len(doc)}")
    for pno in range(min(len(doc), 3)):
        pg = doc[pno]
        for img in pg.get_images(full=True):
            xref = img[0]
            rects = pg.get_image_rects(xref)
            for r in rects:
                print(f"   p{pno} IMG xref={xref} bbox=[{r.x0:.2f},{r.y0:.2f},{r.x1:.2f},{r.y1:.2f}] "
                      f"w={r.width:.2f} h={r.height:.2f}")
    doc.close()
