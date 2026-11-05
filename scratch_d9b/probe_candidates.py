"""候選檔清單：我方 page dump 的 Image 項＋頁數（判定是否走浮動圖發射路徑）。"""
import json
import subprocess
import sys
import glob

CLI = "scratch_d9b/base-bin/quire-cli"
CANDS = [
    "corpus/ooxmlexport/fdo61343.docx",
    "corpus/ooxmlexport/dkvert.docx",
    "corpus/ooxmlimport/groupshape-sdt.docx",
    "corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx",
    "corpus/xopt/NewDocxDocuments_CoverPage.docx",
    "corpus/xopt/TestFiles_HC031-Complicated-Document.docx",
    "corpus/ooxmlexport/n792778.docx",
    "corpus/ooxmlimport/tdf48658_transparentOLEheader.docx",
    "corpus/ooxmlexport/tdf102466.docx",
    "corpus/ooxmlexport/mce-nested.docx",
    "corpus/ooxmlimport/n779627.docx",
    "corpus/ooxmlimport/btlr-frame-vml.docx",
    "corpus/ooxmlimport/tbrl-frame-vml.docx",
    "corpus/ooxmlexport/fdo66688.docx",
    "corpus/ooxmlexport/fdo69636.docx",
]
for rel in CANDS:
    try:
        p = subprocess.run([CLI, "dump", rel], capture_output=True, text=True, timeout=180)
    except Exception as e:
        print(f"{rel}: EXC {e}")
        continue
    if p.returncode != 0:
        print(f"{rel}: FAIL {p.stderr[:120]}")
        continue
    try:
        d = json.loads(p.stdout)
    except Exception:
        print(f"{rel}: JSON parse fail")
        continue
    pages = d["pages"] if "pages" in d else d
    if isinstance(d, dict) and "pages" in d:
        pages = d["pages"]
    elif isinstance(d, list):
        pages = d
    imgs = []
    npages = len(pages)
    for pi, pg in enumerate(pages):
        items = pg["items"] if isinstance(pg, dict) and "items" in pg else pg
        for it in items:
            k = list(it)[0]
            if k == "Image":
                v = it[k]
                imgs.append((pi, round(v["x"], 2), round(v["y"], 2),
                             round(v["w"], 2), round(v["h"], 2)))
    print(f"{rel}: pages={npages} images={len(imgs)}")
    for t in imgs[:8]:
        print(f"    p{t[0]} x={t[1]} y={t[2]} w={t[3]} h={t[4]}")
