#!/usr/bin/env python3
"""E 卡 LO 真值探針 v4：高飽和色封頂「LO 不渲染 tblPr 直接底紋」結論。

v3 用 FAFBFE（近白）；若担心近白被抽樣誤判，v4 全用高飽和色。
  P1：val=clear fill=9900CC，cell 無宣告
  P2：val=solid fill=9900CC color=FF0000（圖案底紋）
  P3：autofit（無 tblLayout）＋ clear 9900CC
  P4：無 tblBorders ＋ clear 9900CC
  P5：無 tblW（auto）＋ clear 9900CC
  P6：cell(0,0)=9900CC 正向對照（LO 必畫）
  P7：baseline 無底紋
"""
import os
import subprocess
import sys
import zipfile
from pathlib import Path

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'

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
SHD_C = '<w:shd w:val="clear" w:color="auto" w:fill="9900CC"/>'
SHD_S = '<w:shd w:val="solid" w:color="FF0000" w:fill="9900CC"/>'


def para(text):
    return f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>' if text else '<w:p/>'


def pagebreak():
    return '<w:p><w:r><w:br w:type="page"/></w:r></w:p>'


def cell(text, shd_xml='', w='2500'):
    return (f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/>{shd_xml}'
            f'</w:tcPr>{para(text)}</w:tc>')


def table(tblpr, tag, cell_fill=None):
    rows = ''
    for r in range(3):
        c0 = cell(f'{tag}{r}0', cell_fill() if (cell_fill and r == 0) else '')
        rows += '<w:tr>' + c0 + cell(f'{tag}{r}1') + '</w:tr>'
    return ('<w:tbl><w:tblPr>' + tblpr + '</w:tblPr>'
            '<w:tblGrid><w:gridCol w:w="2500"/><w:gridCol w:w="2500"/></w:tblGrid>'
            + rows + '</w:tbl>')


VARIANTS = [
    ('P1', '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS + SHD_C
           + '<w:tblLayout w:type="fixed"/><w:tblLook w:val="04A0"/>', None),
    ('P2', '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS + SHD_S
           + '<w:tblLayout w:type="fixed"/><w:tblLook w:val="04A0"/>', None),
    ('P3', '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS + SHD_C
           + '<w:tblLook w:val="04A0"/>', None),
    ('P4', '<w:tblW w:w="5000" w:type="dxa"/>' + SHD_C
           + '<w:tblLayout w:type="fixed"/><w:tblLook w:val="04A0"/>', None),
    ('P5', BORDERS + SHD_C
           + '<w:tblLayout w:type="fixed"/><w:tblLook w:val="04A0"/>', None),
    ('P6', '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS
           + '<w:tblLayout w:type="fixed"/><w:tblLook w:val="04A0"/>',
           lambda: SHD_C),
    ('P7', '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS
           + '<w:tblLayout w:type="fixed"/><w:tblLook w:val="04A0"/>', None),
]

body = ''
for tag, tblpr, cf in VARIANTS:
    body += para(f'TABLE-{tag}') + table(tblpr, tag, cf) + pagebreak()
DOC = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
       f'<w:document {W}><w:body>' + body +
       '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>'
       '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
       'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
       '</w:body></w:document>')

CT = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
      '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
      '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
      '<Default Extension="xml" ContentType="application/xml"/>'
      '<Override PartName="/word/document.xml" '
      'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
      '</Types>')
RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" '
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
        'Target="word/document.xml"/></Relationships>')


def main():
    out_dir = Path(os.environ.get('OUT_DIR', '.')).resolve()
    docx = out_dir / 'e_tblshd_probe4.docx'
    with zipfile.ZipFile(docx, 'w', zipfile.ZIP_DEFLATED) as z:
        z.writestr('[Content_Types].xml', CT)
        z.writestr('_rels/.rels', RELS)
        z.writestr('word/document.xml', DOC)
    lo = Path(os.environ.get('SOFFICE',
               os.path.expanduser('~/.local/lo-linux/opt/libreoffice25.8/program/soffice')))
    prof = 'file://' + str(out_dir / ('loprof4-' + os.environ.get('TAG', 'x')))
    subprocess.run(
        [str(lo), f'-env:UserInstallation={prof}', '--headless', '--norestore',
         '--nolockcheck', '--nodefault', '--convert-to', 'pdf',
         '--outdir', str(out_dir), str(docx)],
        check=False, capture_output=True, timeout=240)
    pdf = out_dir / (docx.stem + '.pdf')
    assert pdf.exists(), 'LO 未產 PDF'
    import pymupdf
    doc = pymupdf.open(str(pdf))
    print('LO:', str(lo), '頁數:', len(doc))
    want = (0x99, 0x00, 0xCC)
    for pno, page in enumerate(doc):
        txt = page.get_text('text')
        tag = next((v[0] for v in VARIANTS if f'TABLE-{v[0]}' in txt), '?')
        fills = []
        for d in page.get_drawings():
            if d.get('fill') is not None:
                c = d['fill']
                rgb = tuple(round(v * 255) for v in c) if len(c) == 3 else c
                r = d['rect']
                fills.append((rgb, (round(r.x0, 1), round(r.y0, 1),
                                    round(r.x1, 1), round(r.y1, 1))))
        n99 = sum(1 for f in fills if f[0] == want)
        print(f'p{pno} [{tag}] fills({len(fills)}) 9900CC筆數={n99} {fills[:4]}')


if __name__ == '__main__':
    main()
