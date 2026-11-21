#!/usr/bin/env python3
"""E 卡 LO 真值探針 v3：順序變體＋縫隙量化。

假設待證：
  H1：LO 渲染 tblPr/w:shd 表層底紋（畫在 cell 底之下，正常表被 cell 白底蓋住
      →PDF 上不可見＝v1 黃頁是 v1 XML 順序錯誤造成的假象？）
  H2：tblCellSpacing>0 時表層底紋從縫隙露出（露出寬度＝spacing？）

變體：
  O1：shd 在 tblCellMar 之後（bug57031 實際順序，v1 實測「黃」）
  O2：shd 在 shd 前無 Borders（shd 緊跟 tblW）
  O3：shd 在 tblLayout 之後（schema 違規尾部）
  S5：正確順序＋tblCellSpacing 100dxa（5pt）→量縫隙
  S0：正確順序＋tblCellSpacing 0  →對照
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
SHD = '<w:shd w:val="clear" w:color="auto" w:fill="FAFBFE"/>'
CELLMAR = ('<w:tblCellMar><w:top w:w="15" w:type="dxa"/>'
           '<w:left w:w="15" w:type="dxa"/><w:bottom w:w="15" w:type="dxa"/>'
           '<w:right w:w="15" w:type="dxa"/></w:tblCellMar>')
LAYOUT = '<w:tblLayout w:type="fixed"/>'
LOOK = '<w:tblLook w:val="04A0"/>'


def para(text):
    return f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>' if text else '<w:p/>'


def pagebreak():
    return '<w:p><w:r><w:br w:type="page"/></w:r></w:p>'


def cell(text, w='2500'):
    return (f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr>{para(text)}</w:tc>')


def table(tblpr, tag):
    rows = ''
    for r in range(3):
        rows += '<w:tr>' + cell(f'{tag}{r}0') + cell(f'{tag}{r}1') + '</w:tr>'
    return ('<w:tbl><w:tblPr>' + tblpr + '</w:tblPr>'
            '<w:tblGrid><w:gridCol w:w="2500"/><w:gridCol w:w="2500"/></w:tblGrid>'
            + rows + '</w:tbl>')


VARIANTS = [
    # (tag, tblpr)
    ('O1', '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS + CELLMAR + SHD + LAYOUT + LOOK),
    ('O2', SHD + '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS + LAYOUT + LOOK),
    ('O3', '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS + LAYOUT + LOOK + SHD),
    ('S5', '<w:tblCellSpacing w:w="100" w:type="dxa"/>'
           '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS + SHD + LAYOUT + LOOK),
    ('S0', '<w:tblCellSpacing w:w="0" w:type="dxa"/>'
           '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS + SHD + LAYOUT + LOOK),
    ('N0', '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS + LAYOUT + LOOK),
]

body = ''
for tag, tblpr in VARIANTS:
    body += para(f'TABLE-{tag}') + table(tblpr, tag) + pagebreak()
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
    docx = out_dir / 'e_tblshd_probe3.docx'
    with zipfile.ZipFile(docx, 'w', zipfile.ZIP_DEFLATED) as z:
        z.writestr('[Content_Types].xml', CT)
        z.writestr('_rels/.rels', RELS)
        z.writestr('word/document.xml', DOC)
    lo = Path(os.environ.get('SOFFICE',
               os.path.expanduser('~/.local/lo-linux/opt/libreoffice25.8/program/soffice')))
    prof = 'file://' + str(out_dir / ('loprof3-' + os.environ.get('TAG', 'x')))
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
        strokes = []
        for d in page.get_drawings():
            r = d['rect']
            if d.get('fill') is not None:
                c = d['fill']
                rgb = tuple(round(v * 255) for v in c) if len(c) == 3 else c
                fills.append((rgb, (round(r.x0, 1), round(r.y0, 1),
                                    round(r.x1, 1), round(r.y1, 1))))
            else:
                strokes.append((round(r.x0, 1), round(r.y0, 1),
                                round(r.x1, 1), round(r.y1, 1)))
        print(f'p{pno} [{tag}] strokes={len(strokes)} fills({len(fills)})={fills[:8]}')
        if tag == 'S5' and strokes:
            xs = sorted({s[0] for s in strokes} | {s[2] for s in strokes})
            print('   S5 stroke x 集合:', xs)


if __name__ == '__main__':
    main()
