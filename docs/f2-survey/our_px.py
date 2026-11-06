#!/usr/bin/env python3
# F2：我方出貨 SVG 光柵化的彩色像素計數（對照 LO 992），定 圖軸 0.0 的成因
import pymupdf, sys, glob, os

for svg in sys.argv[1:]:
    doc = pymupdf.open(svg)
    page = doc[0]
    pix = page.get_pixmap(dpi=150)
    n_color = 0
    n_nonwhite = 0
    s = pix.samples
    for i in range(0, len(s), pix.n):
        r, g, b = s[i], s[i + 1], s[i + 2]
        if not (r > 245 and g > 245 and b > 245):
            n_nonwhite += 1
        if not (abs(r - g) < 12 and abs(g - b) < 12):
            n_color += 1
    print(os.path.basename(svg), "150dpi 非白像素:", n_nonwhite, "彩色像素(飽和度判定):", n_color)
    print("   SVG 內 <image> 數:", open(svg, encoding="utf-8").read().count("<image"))
