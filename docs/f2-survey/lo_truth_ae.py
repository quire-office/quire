#!/usr/bin/env python3
# F2 LO 真值：artistic_effects.docx 的 LO PDF 逐頁事實（繪製圖形/圖片/文字/非白像素）
import sys, os, subprocess, glob
sys.path.insert(0, "/home/jerry/projects/quire/.worktrees/t_ab254070/scripts")
import pymupdf

PDF = "/mnt/c/Users/me/AppData/Local/Temp/quire-render-lab/ooxmlexport__artistic_effects.pdf"
if not os.path.exists(PDF):
    print("PDF 不存在，需先跑 render_lab：", PDF)
    sys.exit(1)

doc = pymupdf.open(PDF)
for pno, page in enumerate(doc):
    print(f"===== p{pno} rect={page.rect}")
    words = page.get_text("words")
    print("  words:", [w[4] for w in words][:40])
    print("  text repr:", repr(page.get_text())[:300])
    print("  images(宣告):", [(im[0], im[2], im[3]) for im in page.get_images(full=True)])
    for im in page.get_images(full=True):
        print("    xref", im[0], "rects=", page.get_image_rects(im[0]))
    d = page.get_drawings()
    print("  drawings:", len(d))
    for g in d[:12]:
        print("    type=", g.get("type"), "rect=", tuple(round(v, 1) for v in g["rect"]),
              "fill=", g.get("fill"), "color=", g.get("color"), "w=", g.get("width"),
              "items=", len(g.get("items", [])))
    pix = page.get_pixmap(dpi=150)
    n_color = 0
    samples = pix.samples
    for i in range(0, len(samples), pix.n):
        r, g, b = samples[i], samples[i + 1], samples[i + 2]
        if not (abs(r - g) < 12 and abs(g - b) < 12):
            n_color += 1
    print("  非白彩色像素(150dpi):", n_color)
