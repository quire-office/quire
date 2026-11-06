#!/usr/bin/env python3
# F2：我方彩色像素落點 vs LO 彩色連通域（同一 150dpi 網格），定 EMF icon 渲染色差
import sys
sys.path.insert(0, "/home/jerry/projects/quire/.worktrees/t_ab254070/scripts")
import numpy as np, pymupdf, collections
import render_lab as RL

PDF = "/mnt/c/Users/me/AppData/Local/Temp/quire-render-lab/ooxmlexport__artistic_effects.pdf"
SVG = "/home/jerry/.hermes/profiles/ow-render/cache/scratch/f2_cur/artistic_effects-p0.svg"
S = 150.0 / 72.0

def blobs_pt(path):
    img = np.frombuffer(pymupdf.open(path)[0].get_pixmap(dpi=RL.J3_DPI).samples,
                        dtype=np.uint8).reshape(-1, pymupdf.open(path)[0].get_pixmap(dpi=RL.J3_DPI).width, 3)[0:1]
    d = pymupdf.open(path)
    pix = d[0].get_pixmap(dpi=RL.J3_DPI)
    img = np.frombuffer(pix.samples, dtype=np.uint8).reshape(pix.height, pix.width, pix.n)[..., :3]
    sat, hue, val = RL._sat_hue(img)
    lab, n = RL._label(sat >= 25)
    out = []
    for i in range(1, n + 1):
        ys, xs = np.nonzero(lab == i)
        if len(ys) < 5:
            continue
        out.append(dict(px=int(len(ys)),
                        bbox_pt=(round(xs.min()/S, 1), round(ys.min()/S, 1),
                                 round(xs.max()/S, 1), round(ys.max()/S, 1)),
                        mean_rgb=tuple(int(v) for v in img[ys, xs].mean(axis=0)),
                        mean_hue=round(float(hue[ys, xs].mean()), 1),
                        mean_sat=round(float(sat[ys, xs].mean()), 1)))
    return sorted(out, key=lambda b: -b["px"])

print("== LO 側連通域（>=5 px）==")
for b in blobs_pt(PDF):
    print("  ", b)
print("== 我方側連通域（>=5 px）==")
for b in blobs_pt(SVG):
    print("  ", b)
