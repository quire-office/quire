#!/usr/bin/env python3
# F2：tdf108545（OLE icon 候選檔）LO 真值——確認是否「LO 真有圖形、我方零發射」
import sys, glob
sys.path.insert(0, "<repo>/.worktrees/t_ab254070/scripts")
import pymupdf

cands = glob.glob("/mnt/c/Users/me/AppData/Local/Temp/quire-render-lab/*tdf108545*.pdf")
print("PDF:", cands)
if not cands:
    sys.exit(1)
doc = pymupdf.open(cands[0])
for pno, page in enumerate(doc):
    print(f"-- p{pno} text={page.get_text()!r}")
    for im in page.get_images(full=True):
        print("   image xref", im[0], "w,h=", im[2], im[3], "rects=", page.get_image_rects(im[0]))
    d = page.get_drawings()
    print("   drawings:", len(d))
    for g in d[:6]:
        print("     ", g.get("type"), tuple(round(v, 1) for v in g["rect"]), "fill=", g.get("fill"))
