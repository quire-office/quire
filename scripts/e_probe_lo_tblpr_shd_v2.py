#!/usr/bin/env python3
"""E 卡 LO 真值探針 v2：tblPr 直接底紋的各種變體，LO 是否渲染。

每張表一頁（分頁符隔開）。變體：
  A：schema 正確順序（tblW, tblBorders, shd, tblLayout, tblLook）＋格不宣告
  F：A ＋ tblCellSpacing 100 dxa（5pt 縫隙，露出表層背景的可能性）
  G：A ＋ w:val="solid"（圖案底紋）
  H：tblPr 無 shd，改由 tblStyle（styles.xml）的 tblStylePr firstRow 帶底紋
  B：A ＋ 格(0,0) 明示 FFFF00（疊序：LO 是否把 cell 底紋蓋在表層之上）
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


def shd(fill, val='clear'):
    return f'<w:shd w:val="{val}" w:color="auto" w:fill="{fill}"/>'


def cell(text, shd_xml='', w='2500'):
    return (
        '<w:tc><w:tcPr>'
        f'<w:tcW w:w="{w}" w:type="dxa"/>'
        + shd_xml
        + f'</w:tcPr>{para(text)}</w:tc>'
    )


def table(tblpr_xml, tag, cell_fill00=''):
    rows = ''
    for r in range(2):
        cells = ''
        for c in range(2):
            fill = shd(cell_fill00) if (tag == 'B' and r == 0 and c == 0) else ''
            cells += cell(f'{tag}{r}{c}', fill)
        rows += f'<w:tr>{cells}</w:tr>'
    return (
        '<w:tbl><w:tblPr>'
        + tblpr_xml
        + '</w:tblPr>'
        '<w:tblGrid><w:gridCol w:w="2500"/><w:gridCol w:w="2500"/></w:tblGrid>'
        + rows
        + '</w:tbl>'
    )


# schema CT_TblPrBase 順序：tblStyle tblpPr tblOverlap bidiVisual minRowHeight
#   tblpX tblpY tblCellSpacing tblInd tblW jc tblCellMar tblBorders shd tblLayout tblLook
A_TBLPR = (
    '<w:tblW w:w="5000" w:type="dxa"/>'
    + BORDERS
    + shd('FAFBFE')
    + '<w:tblLayout w:type="fixed"/>'
    '<w:tblLook w:val="04A0"/>'
)
F_TBLPR = (
    '<w:tblCellSpacing w:w="100" w:type="dxa"/>'
    '<w:tblW w:w="5000" w:type="dxa"/>'
    + BORDERS
    + shd('FAFBFE')
    + '<w:tblLayout w:type="fixed"/>'
    '<w:tblLook w:val="04A0"/>'
)
G_TBLPR = (
    '<w:tblW w:w="5000" w:type="dxa"/>'
    + BORDERS
    + shd('FAFBFE', val='solid')
    + '<w:tblLayout w:type="fixed"/>'
    '<w:tblLook w:val="04A0"/>'
)
H_TBLPR = (
    '<w:tblStyle w:val="MyShaded"/>'
    '<w:tblW w:w="5000" w:type="dxa"/>'
    + BORDERS
    + '<w:tblLayout w:type="fixed"/>'
    '<w:tblLook w:val="04A0" w:firstRow="1" w:bandedRows="1"/>'
)

DOC = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f'<w:document {W}><w:body>'
    + para('TABLE-A') + table(A_TBLPR, 'A') + pagebreak()
    + para('TABLE-F') + table(F_TBLPR, 'F') + pagebreak()
    + para('TABLE-G') + table(G_TBLPR, 'G') + pagebreak()
    + para('TABLE-H') + table(H_TBLPR, 'H') + pagebreak()
    + para('TABLE-B') + table(A_TBLPR, 'B', cell_fill00='FFFF00') + pagebreak()
    + para('TABLE-Z') + table(
        '<w:tblW w:w="5000" w:type="dxa"/>' + BORDERS
        + '<w:tblLayout w:type="fixed"/><w:tblLook w:val="04A0"/>', 'Z')
    + '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>'
      '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
      'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
    '</w:body></w:document>'
)

STYLES = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f'<w:styles {W}>'
    '<w:style w:type="table" w:styleId="MyShaded">'
    '<w:name w:val="My Shaded"/>'
    '<w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr>'
    '<w:tblStylePr w:type="band1Horz"><w:tcPr>'
    '<w:shd w:val="clear" w:color="auto" w:fill="D9E2F3"/>'
    '</w:tcPr></w:tblStylePr>'
    '</w:style>'
    '</w:styles>'
)

CT = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Override PartName="/word/document.xml" '
    'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
    '<Override PartName="/word/styles.xml" '
    'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
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

DOC_RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId10" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" '
    'Target="styles.xml"/>'
    '</Relationships>'
)


def build(out_dir):
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    docx = out / 'e_tblshd_probe2.docx'
    with zipfile.ZipFile(docx, 'w', zipfile.ZIP_DEFLATED) as z:
        z.writestr('[Content_Types].xml', CT)
        z.writestr('_rels/.rels', RELS)
        z.writestr('word/document.xml', DOC)
        z.writestr('word/_rels/document.xml.rels', DOC_RELS)
        z.writestr('word/styles.xml', STYLES)
    return docx


def lo_convert(soffice, docx, out_dir, prof):
    subprocess.run(
        [str(soffice), f'-env:UserInstallation={prof}',
         '--headless', '--norestore', '--nolockcheck', '--nodefault',
         '--convert-to', 'pdf', '--outdir', str(out_dir), str(docx)],
        check=False, capture_output=True, timeout=240)
    pdf = Path(out_dir) / (docx.stem + '.pdf')
    return pdf if pdf.exists() else None


def main():
    out_dir = Path(os.environ.get('OUT_DIR', '.'))
    docx = build(out_dir)
    lo = Path(os.environ.get('SOFFICE',
               os.path.expanduser('~/.local/lo-linux/opt/libreoffice25.8/program/soffice')))
    prof = 'file://' + str((out_dir / ('loprof-' + os.environ.get('TAG', 'linux'))).resolve())
    pdf = lo_convert(lo, docx, out_dir, prof)
    if not pdf:
        print('LO 轉換失敗')
        sys.exit(1)
    import pymupdf
    doc = pymupdf.open(str(pdf))
    print('頁數:', len(doc))
    for pno, page in enumerate(doc):
        txt = page.get_text('text')
        tag = next((t for t in 'AFGHBZ' if f'TABLE-{t}' in txt), '?')
        x0 = y0 = 1e9
        x1 = y1 = -1e9
        n_stroke = 0
        fills = []
        for d in page.get_drawings():
            r = d['rect']
            if r.width > 30 and r.height > 8:
                x0, y0 = min(x0, r.x0), min(y0, r.y0)
                x1, y1 = max(x1, r.x1), max(y1, r.y1)
                n_stroke += 1
            if d.get('fill') is not None:
                c = d['fill']
                rgb = tuple(round(v * 255) for v in c) if len(c) == 3 else c
                fills.append((rgb, (round(r.x0, 1), round(r.y0, 1),
                                    round(r.x1, 1), round(r.y1, 1))))
        print(f'p{pno} [{tag}] stroke={n_stroke} fills={fills[:6]}')


if __name__ == '__main__':
    main()
