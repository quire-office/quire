#!/usr/bin/env python3
# F2：圖軸 0.0 的精確成因——用 render_lab 自己的 image_blobs/score_image 逐頁算 LO/我方兩側
import sys, os, pathlib
sys.path.insert(0, "/home/jerry/projects/quire/.worktrees/t_ab254070/scripts")
import numpy as np, pymupdf
import render_lab as RL

PDF = "/mnt/c/Users/me/AppData/Local/Temp/quire-render-lab/ooxmlexport__artistic_effects.pdf"
SVG = "/home/jerry/.hermes/profiles/ow-render/cache/scratch/f2_cur/artistic_effects-p0.svg"
DPI = RL.J3_DPI


def raster(path):
    d = pymupdf.open(path)
    pix = d[0].get_pixmap(dpi=DPI)
    return np.frombuffer(pix.samples, dtype=np.uint8).reshape(pix.height, pix.width, pix.n)[..., :3]


lo = raster(PDF)
ow = raster(SVG)
for tag, img in (("LO", lo), ("OW", ow)):
    sat, hue, val = RL._sat_hue(img)
    print(f"-- {tag}: 尺寸 {img.shape}, sat>=25 像素 {int((sat>=25).sum())}")
    blobs = RL.image_blobs(img)
    print(f"   image_blobs（過 IMG_MIN_SIDE_PX={RL.IMG_MIN_SIDE_PX}/IMG_MIN_PIXELS={RL.IMG_MIN_PIXELS} 門檻後）: {len(blobs)}")
    for b, dens in blobs:
        print(f"     bbox(px)={b} 邊長={b[2]-b[0]}x{b[3]-b[1]}pt@{DPI}dpi 密度={dens:.3f}")
print("score_image(LO,OW) =", RL.score_image(lo, ow))
print("score_color(LO,OW) =", RL.score_color(lo, ow))
print("LO 色彩直方圖 nonzero bins:", int((RL.color_hist(lo) > 0).sum()),
      "和=", float(RL.color_hist(lo).sum()))
print("OW 色彩直方圖 nonzero bins:", int((RL.color_hist(ow) > 0).sum()),
      "和=", float(RL.color_hist(ow).sum()))
# 未過濾前的原始連通域
for tag, img in (("LO", lo), ("OW", ow)):
    sat, _, _ = RL._sat_hue(img)
    lab, n = RL._label(sat >= 25)
    import collections
    cnt = collections.Counter(lab.ravel())
    del cnt[0]
    print(f"{tag} 未過濾連通域 top10（像素數）:", sorted(cnt.values(), reverse=True)[:10])
