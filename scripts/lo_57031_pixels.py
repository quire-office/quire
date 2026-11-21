"""真檔 A/B：bug57031 LO PDF 的 FAFBFE(250,251,254) 像素分佈（numpy 向量化）。

判讀：我方 FAFBFE Rect＝12 筆（全 cell 層，bug57031 XML 實測）。若 LO 渲染
表層底紋（8 張 tblPr 底紋表），其 FAFBFE 面積應遠大於 12 格。
"""
import os
import subprocess
import sys
from pathlib import Path

import numpy as np
import pymupdf

SRC = sys.argv[1]
OUT = Path(sys.argv[2]).resolve()
OUT.mkdir(parents=True, exist_ok=True)
pdf = OUT / (Path(SRC).stem + '.pdf')
if not pdf.exists():
    lo = os.path.expanduser(
        '~/.local/lo-linux/opt/libreoffice25.8/program/soffice')
    r = subprocess.run(
        [lo, '-env:UserInstallation=file:///tmp/e_tblshd_prof_57031',
         '--headless', '--norestore', '--nolockcheck', '--nodefault',
         '--convert-to', 'pdf', '--outdir', str(OUT), SRC],
        capture_output=True, timeout=600)
    print('LO rc=', r.returncode)
    assert pdf.exists(), 'LO 未產 PDF'

doc = pymupdf.open(str(pdf))
print('LO 頁數:', len(doc))
tot = 0
for pno, page in enumerate(doc):
    pix = page.get_pixmap(dpi=96, colorspace=pymupdf.csRGB)
    arr = np.frombuffer(pix.samples, dtype=np.uint8).reshape(pix.height, pix.width, pix.n)
    rgb = arr[:, :, :3].astype(np.int16)
    d = np.abs(rgb - np.array([250, 251, 254], dtype=np.int16))
    cnt = int(((d.max(axis=2)) <= 2).sum())
    if cnt:
        print(f'  p{pno}: FAFBFE±像素 = {cnt}')
    tot += cnt
print('FAFBFE± 像素總計 =', tot)
