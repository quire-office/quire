#!/usr/bin/env python3
"""E 卡 LO 真值探針（v1，最早一版）：tblPr 直接底紋 LO 是否渲染。

四張表各佔一頁（分頁符隔開）：
  A：tblPr shd FAFBFE，格子全不宣告 → 卡面前提假設應整表區 FAFBFE（實測零渲染）
  B：tblPr shd FAFBFE＋格(0,0) shd FFFF00 → 格(0,0) 黃、其餘 FAFBFE（疊序假設）
  C：tblPr shd fill="auto" → 不應有任何底色
  D：tblPr shd val="nil" fill="FAFBFE" → nil 壓掉 fill，不應有底色

實測（LO 25.8 Linux）：四頁表區全數零 fill——卡面前提由此反轉，
續跑 v2–v5 封頂（見 docs/e-tblpr-shd-redlight.md）。
"""
import os
import subprocess
import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'


def para(text):
    return f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>' if text else '<w:p/>'


def pagebreak():
    return '<w:p><w:r><w:br w:type="page"/></w:r></w:p>'


BORDERS = (
    '<w:tblBorders>'
    '<w:top w:val="single" w:sz="8" w:space="0" w:color="000000"/>'
    '<w:left w:val="single" w:sz="8" w:space="0" w:color="000000"/>'
    '<w:bottom w:val="single" w:sz="8" w:space="0" w:color="000000"/>'
    '<w:right w:val="single" w:sz="8" w:space="0" w:color="000000"/>'
    '<w:insideH w:val="single" w:sz="8" w:space="0" w:color="000000"/>'
    '<w:insideV w:val="single" w:sz="8" w:space="0" w:color="000000"/>'
    '</w:tblBorders>'
)


def shd(fill, val='clear'):
    return f'<w:shd w:val="{val}" w:color="auto" w:fill="{fill}"/>'


def cell(text, shd_xml='', w='2500'):
    return (
        '<w:tc><w:tcPr>'
        f'<w:tcW w:w="{w}" w:type="dxa"/>'
        + shd_xml
        + f'</w:tcPr>{para(text)}</w:tc>'
    )


def table(shd_xml, tag):
    rows = ''
    for r in range(2):
        cells = ''
        for c in range(2):
            fill = ''
            if tag == 'B' and r == 0 and c == 0:
                fill = shd('FFFF00')
            cells += cell(f'{tag}{r}{c}', fill)
        rows += f'<w:tr>{cells}</w:tr>'
    return (
        '<w:tbl><w:tblPr>'
        '<w:tblW w:w="5000" w:type="dxa"/>'
        '<w:tblLayout w:type="fixed"/>'
        + BORDERS
        + shd_xml
        + '<w:tblLook w:val="04A0"/>'
        '</w:tblPr>'
        '<w:tblGrid><w:gridCol w:w="2500"/><w:gridCol w:w="2500"/></w:tblGrid>'
        + rows
        + '</w:tbl>'
    )


DOC = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f'<w:document {W}><w:body>'
    + para('TABLE-A')
    + table(shd('FAFBFE'), 'A')
    + pagebreak()
    + para('TABLE-B')
    + table(shd('FAFBFE'), 'B')
    + pagebreak()
    + para('TABLE-C')
    + table(shd('auto'), 'C')
    + pagebreak()
    + para('TABLE-D')
    + table(shd('FAFBFE', val='nil'), 'D')
    + pagebreak()
    + para('TABLE-E')
    + table('', 'E')
    + '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>'
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
    'Target="word/document.xml"/>'
    '</Relationships>'
)


def main():
    out = Path(os.environ.get('OUT_DIR', '.'))
    out.mkdir(parents=True, exist_ok=True)
    docx = out / 'e_tblshd_probe.docx'
    with zipfile.ZipFile(docx, 'w', zipfile.ZIP_DEFLATED) as z:
        z.writestr('[Content_Types].xml', CT)
        z.writestr('_rels/.rels', RELS)
        z.writestr('word/document.xml', DOC)

    lo = os.path.expanduser(
        '~/.local/lo-linux/opt/libreoffice25.8/program/soffice')
    prof = out / 'loprof'
    subprocess.run(
        [lo, '--headless', '--norestore', '--nolockcheck', '--nodefault',
         f'-env:UserInstallation=file://{prof}',
         '--convert-to', 'pdf', '--outdir', str(out), str(docx)],
        check=True, capture_output=True, timeout=300)
    pdf = out / 'e_tblshd_probe.pdf'
    assert pdf.exists(), 'LO 未產 PDF'

    import pymupdf
    doc = pymupdf.open(pdf)
    print('LO 頁數:', len(doc))
    # 每頁：抓表格 Rules bbox（黑色細線矩形群），取表內樣本點色值。
    for pno, page in enumerate(doc):
        txt = page.get_text('text')
        tag = next((t for t in 'ABCDE' if f'TABLE-{t}' in txt), '?')
        draws = page.get_drawings()
        rects = [d['rect'] for d in draws if d['rect'].width > 100]
        if not rects:
            print(f'p{pno} [{tag}] 無大矩形drawing')
            continue
        bx = max(rects, key=lambda r: r.width * r.height)
        # 表內取樣：四格中心
        pts = {
            'cell00': (bx.x0 + bx.width * 0.25, bx.y0 + bx.height * 0.25),
            'cell11': (bx.x0 + bx.width * 0.75, bx.y0 + bx.height * 0.75),
        }
        pix = page.get_pixmap(clip=bx, colorspace=pymupdf.csRGB, dpi=72)
        out_png = out / f'probe-p{pno}-{tag}.png'
        pix.save(str(out_png))
        samples = {}
        for name, (px, py) in pts.items():
            sx = int((px - bx.x0) * pix.width / bx.width)
            sy = int((py - bx.y0) * pix.height / bx.height)
            sx = min(max(sx, 2), pix.width - 3)
            sy = min(max(sy, 2), pix.height - 3)
            samples[name] = pix.pixel(sx, sy)
        print(f'p{pno} [{tag}] bbox={bx} 取樣={samples}')


if __name__ == '__main__':
    main()
