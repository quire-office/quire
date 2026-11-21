#!/usr/bin/env python3
"""E 卡探針 v5：表格層底紋的**樣式路徑**（styles.xml table style 的 tblPr/w:shd）。

v4 已證 `document.xml` 直接 `tblPr/w:shd` LO 七變體全零渲染。
本探針封最後一軸：底紋若經 `w:tblStyle` 查表的表格樣式宣告，LO 是否渲染？
  Q1：tblStyle=MyTblShaded（styles.xml 該樣式 tblPr 含 shd 9900CC）
  Q2：同 Q1 但 document.xml 直接 tblPr 再疊 shd 00B050（樣式＋直接雙宣告）
  Q3：正向對照 cell(0,0)=9900CC
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


def para(text):
    return f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>' if text else '<w:p/>'


def pagebreak():
    return '<w:p><w:r><w:br w:type="page"/></w:r></w:p>'


def cell(text, shd_xml='', w='2500'):
    return (f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/>{shd_xml}'
            f'</w:tcPr>{para(text)}</w:tc>')


def table(tblpr, tag, cell00=''):
    rows = ''
    for r in range(3):
        c0 = cell(f'{tag}{r}0', cell00 if r == 0 else '')
        rows += '<w:tr>' + c0 + cell(f'{tag}{r}1') + '</w:tr>'
    return ('<w:tbl><w:tblPr>' + tblpr + '</w:tblPr>'
            '<w:tblGrid><w:gridCol w:w="2500"/><w:gridCol w:w="2500"/></w:tblGrid>'
            + rows + '</w:tbl>')


S = '<w:shd w:val="clear" w:color="auto" w:fill="{}"/>'
LAYOUT = '<w:tblLayout w:type="fixed"/>'
LOOK = '<w:tblLook w:val="04A0"/>'
VARIANTS = [
    ('Q1', f'<w:tblStyle w:val="MyTblShaded"/><w:tblW w:w="5000" w:type="dxa"/>'
           + BORDERS + LAYOUT + LOOK, ''),
    ('Q2', f'<w:tblStyle w:val="MyTblShaded"/><w:tblW w:w="5000" w:type="dxa"/>'
           + BORDERS + S.format('00B050') + LAYOUT + LOOK, ''),
    ('Q3', '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS + LAYOUT + LOOK,
     S.format('9900CC')),
]

body = ''
for tag, tblpr, c00 in VARIANTS:
    body += para(f'TABLE-{tag}') + table(tblpr, tag, c00) + pagebreak()
DOC = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
       f'<w:document {W}><w:body>' + body +
       '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>'
       '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
       'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
       '</w:body></w:document>')

STYLES = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f'<w:styles {W}>'
    '<w:style w:type="table" w:styleId="MyTblShaded">'
    '<w:name w:val="My Tbl Shaded"/>'
    '<w:tblPr><w:tblW w:w="0" w:type="auto"/>' + S.format('9900CC')
    + '</w:tblPr></w:style>'
    '</w:styles>'
)

CT = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
      '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
      '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
      '<Default Extension="xml" ContentType="application/xml"/>'
      '<Override PartName="/word/document.xml" '
      'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
      '<Override PartName="/word/styles.xml" '
      'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
      '</Types>')
RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" '
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
        'Target="word/document.xml"/></Relationships>')
DOC_RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId10" '
            'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" '
            'Target="styles.xml"/></Relationships>')


def main():
    out_dir = Path(os.environ.get('OUT_DIR', '.')).resolve()
    docx = out_dir / 'e_tblshd_probe5.docx'
    with zipfile.ZipFile(docx, 'w', zipfile.ZIP_DEFLATED) as z:
        z.writestr('[Content_Types].xml', CT)
        z.writestr('_rels/.rels', RELS)
        z.writestr('word/document.xml', DOC)
        z.writestr('word/_rels/document.xml.rels', DOC_RELS)
        z.writestr('word/styles.xml', STYLES)
    lo = Path(os.environ.get('SOFFICE',
               os.path.expanduser('~/.local/lo-linux/opt/libreoffice25.8/program/soffice')))
    prof = 'file://' + str(out_dir / ('loprof5-' + os.environ.get('TAG', 'x')))
    subprocess.run(
        [str(lo), f'-env:UserInstallation={prof}', '--headless', '--norestore',
         '--nolockcheck', '--nodefault', '--convert-to', 'pdf',
         '--outdir', str(out_dir), str(docx)],
        check=False, capture_output=True, timeout=240)
    pdf = out_dir / (docx.stem + '.pdf')
    assert pdf.exists(), 'LO 未產 PDF'
    import pymupdf
    doc = pymupdf.open(str(pdf))
    print('頁數:', len(doc))
    for pno, page in enumerate(doc):
        txt = page.get_text('text')
        tag = next((v[0] for v in VARIANTS if f'TABLE-{v[0]}' in txt), '?')
        fills = []
        for d in page.get_drawings():
            if d.get('fill') is not None:
                c = d['fill']
                rgb = tuple(round(v * 255) for v in c) if len(c) == 3 else c
                rr = d['rect']
                fills.append((rgb, (round(rr.x0, 1), round(rr.y0, 1),
                                    round(rr.x1, 1), round(rr.y1, 1))))
        print(f'p{pno} [{tag}] fills({len(fills)})={fills[:4]}')


if __name__ == '__main__':
    main()
