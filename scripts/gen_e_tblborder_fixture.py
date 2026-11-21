#!/usr/bin/env python3
"""生成 fixtures/e_tblborder_probe.docx（E 卡：表格邊框 LO 真值探針樣本）。

設計（docs/e-tblborder-redlight.md §1）：
- 表 0（2 列×2 欄）：tblBorders 直接格式——top single sz=16 color=FF0000、
  left single sz=8 color=auto、bottom none、right none、insideH dashed
  sz=12 color=0000FF、insideV none。考四件事：sz→pt 換算（16→2pt、8→1pt、
  12→1.5pt）、none 不發射、inside 只畫內部（外框上下不畫）、虛線樣式。
- 表 1（1 列×2 欄）：tblBorders 全 none；格 (0,0) tcBorders top single
  sz=24 color=00B050（綠 3pt）、bottom none、left nil、right hidden。
  考 tcBorders 覆寫（宣告即贏）與 none/nil/hidden 三態皆不發射。

純 stdlib、STORE 壓縮、固定時間戳（可重複生成、位元組可重現）。
"""
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "fixtures" / "e_tblborder_probe.docx"

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'


def para(text):
    return f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>'


def cell(width_dxa, text, tcpr_extra=''):
    return (f'<w:tc><w:tcPr><w:tcW w:w="{width_dxa}" w:type="dxa"/>{tcpr_extra}'
            f'</w:tcPr>{para(text)}</w:tc>')


# 表 0：表格級 tblBorders 六方位全宣告（含 none 對照位）
TBL0 = (
    '<w:tbl>'
    '<w:tblPr>'
    '<w:tblW w:w="4320" w:type="dxa"/>'
    '<w:tblLayout w:type="fixed"/>'
    '<w:tblBorders>'
    '<w:top w:val="single" w:sz="16" w:space="0" w:color="FF0000"/>'
    '<w:left w:val="single" w:sz="8" w:space="0" w:color="auto"/>'
    '<w:bottom w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
    '<w:right w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
    '<w:insideH w:val="dashed" w:sz="12" w:space="0" w:color="0000FF"/>'
    '<w:insideV w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
    '</w:tblBorders>'
    '</w:tblPr>'
    '<w:tblGrid><w:gridCol w:w="2160"/><w:gridCol w:w="2160"/></w:tblGrid>'
    '<w:tr>'
    + cell(2160, '甲一') + cell(2160, '甲二') +
    '</w:tr>'
    '<w:tr>'
    + cell(2160, '甲三') + cell(2160, '甲四') +
    '</w:tr>'
    '</w:tbl>'
)

# 表 1：全 none 底＋單格 tcBorders（覆寫＋三不發射態）
TC_BORDER_00 = (
    '<w:tcBorders>'
    '<w:top w:val="single" w:sz="24" w:space="0" w:color="00B050"/>'
    '<w:bottom w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
    '<w:left w:val="nil" w:sz="0" w:space="0" w:color="auto"/>'
    '<w:right w:val="hidden" w:sz="0" w:space="0" w:color="auto"/>'
    '</w:tcBorders>'
)
TBL1 = (
    '<w:tbl>'
    '<w:tblPr>'
    '<w:tblW w:w="4320" w:type="dxa"/>'
    '<w:tblLayout w:type="fixed"/>'
    '<w:tblBorders>'
    '<w:top w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
    '<w:left w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
    '<w:bottom w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
    '<w:right w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
    '<w:insideH w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
    '<w:insideV w:val="none" w:sz="0" w:space="0" w:color="auto"/>'
    '</w:tblBorders>'
    '</w:tblPr>'
    '<w:tblGrid><w:gridCol w:w="2160"/><w:gridCol w:w="2160"/></w:tblGrid>'
    '<w:tr>'
    + cell(2160, '乙一', TC_BORDER_00) + cell(2160, '乙二') +
    '</w:tr>'
    '</w:tbl>'
)

DOC = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f'<w:document {W}><w:body>'
    + para('表格邊框探針：上表測表格級六方位，下表測儲存格級覆寫。')
    + TBL0
    + para('間隔段落')
    + TBL1
    + para('表格後段落')
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

FILES = [
    ("[Content_Types].xml", CT),
    ("_rels/.rels", RELS),
    ("word/document.xml", DOC),
]


def zip_local(name, data):
    crc = zlib.crc32(data) & 0xFFFFFFFF
    n = name.encode()
    head = struct.pack(
        '<IHHHHHIIIHH', 0x04034B50, 20, 0, 0, 0, 0,
        crc, len(data), len(data), len(n), 0)
    return head + n + data, crc, n


def zip_central(n, crc, offset, size):
    return struct.pack(
        '<IHHHHHHIIIHHHHHII', 0x02014B50, 20, 20, 0, 0, 0, 0, crc, size, size,
        len(n), 0, 0, 0, 0, 0x20, offset) + n


def main():
    locals_, centrals, off = [], [], 0
    for name, text in FILES:
        data = text.encode('utf-8')
        blob, crc, n = zip_local(name, data)
        locals_.append(blob)
        centrals.append(zip_central(n, crc, off, len(data)))
        off += len(blob)
    local_blob = b"".join(locals_)
    central_blob = b"".join(centrals)
    eocd = struct.pack(
        '<IHHHHIIH', 0x06054B50, 0, 0, len(FILES), len(FILES),
        len(central_blob), len(local_blob), 0)
    out = local_blob + central_blob + eocd
    OUT.write_bytes(out)
    import io
    zf = __import__("zipfile").ZipFile(io.BytesIO(out))
    assert zf.testzip() is None, "zip CRC 驗證失敗"
    assert sorted(zf.namelist()) == sorted(nm for nm, _ in FILES)
    print(f"OK {OUT} ({len(out)} bytes)")


if __name__ == '__main__':
    main()
