#!/usr/bin/env python3
"""生成 fixtures/table_mixed.docx（Phase3-1a 表格 canonical model 自產樣本）。

含：合併列（gridSpan）／合併行（vMerge restart/continue）／巢狀表格／
邊框底紋（tblBorders/tcBorders/shd）／未知屬性（custom:* 自訂命名空間屬性、
w:tblLook 未讀屬性、w:cnfStyle/w:noWrap 未知元素）——兼驗「未知屬性
round-trip 不丟失」（byte-level 無損鐵律 + 驗收 6）。

純 stdlib、STORE 壓縮、固定時間戳（可重複生成、位元組可重現）。
"""
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "fixtures" / "table_mixed.docx"

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
CUS = 'xmlns:custom="urn:quire:unknown-table-attrs"'

CELL_BORDERS = ('<w:tcBorders>'
                '<w:top w:val="single" w:sz="12" w:space="0" w:color="8B0000"/>'
                '<w:left w:val="dashed" w:sz="6" w:space="1" w:color="FF0000"/>'
                '<w:bottom w:val="thick" w:sz="18" w:space="0" w:color="auto"/>'
                '<w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/>'
                '</w:tcBorders>')


def para(text):
    return f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>'


NESTED_TBL = (
    '<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/>'
    '<w:tblLayout w:type="fixed"/></w:tblPr>'
    '<w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/></w:tblGrid>'
    '<w:tr><w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr>'
    + para('內層A') +
    '</w:tc><w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr>'
    + para('內層B') +
    '</w:tc></w:tr></w:tbl>'
)

MAIN_TBL = (
    '<w:tbl>'
    '<w:tblPr>'
    '<w:tblStyle w:val="TableGrid"/>'
    '<w:tblW w:w="9000" w:type="dxa"/>'
    '<w:tblLayout w:type="fixed"/>'
    '<w:tblBorders>'
    '<w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/>'
    '<w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/>'
    '<w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/>'
    '<w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/>'
    '<w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/>'
    '<w:insideV w:val="dotted" w:sz="4" w:space="0" w:color="auto"/>'
    '</w:tblBorders>'
    '<w:tblLook w:val="04A0" w:firstRow="1" w:lastRow="0"/>'
    '<w:cnfStyle w:val="000000000000"/>'
    '<custom:tblNote w:val="quirk"/>'
    '<custom:legacy w:flag="on"/>'
    '</w:tblPr>'
    '<w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="3500"/><w:gridCol w:w="3500"/></w:tblGrid>'
    # 列 0：tblHeader + cantSplit；儲存格橫跨全欄（gridSpan=3）＋底紋
    '<w:tr>'
    '<w:trPr><w:cantSplit/><w:tblHeader/></w:trPr>'
    '<w:tc><w:tcPr><w:tcW w:w="9000" w:type="dxa"/><w:gridSpan w:val="3"/>'
    '<w:shd w:val="clear" w:color="auto" w:fill="D9E2F3"/></w:tcPr>'
    + para('財務摘要') +
    '</w:tc></w:tr>'
    # 列 1：三欄，第一欄置中（vAlign）＋自訂命名空間屬性（未知屬性）
    '<w:tr>'
    '<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/>' + CELL_BORDERS +
    '<w:shd w:val="solid" w:color="auto" w:fill="FDE9D9"/>'
    '<w:vAlign w:val="center"/><custom:cellRole w:kind="label"/></w:tcPr>'
    + para('項目') +
    '</w:tc>'
    '<w:tc><w:tcPr><w:tcW w:w="3500" w:type="dxa"/></w:tcPr>' + para('第一季') + '</w:tc>'
    '<w:tc><w:tcPr><w:tcW w:w="3500" w:type="dxa"/></w:tcPr>' + para('第二季') + '</w:tc>'
    '</w:tr>'
    # 列 2：vMerge restart（第 0 欄）、第二欄橫跨兩欄（gridSpan=2）含巢狀表格
    '<w:tr>'
    '<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/><w:vMerge w:val="restart"/></w:tcPr>'
    + para('備註') +
    '</w:tc>'
    '<w:tc><w:tcPr><w:tcW w:w="7000" w:type="dxa"/><w:gridSpan w:val="2"/>'
    '<w:shd w:val="clear" w:color="auto" w:fill="E2EFDA"/></w:tcPr>'
    + para('含附註') + NESTED_TBL +
    '</w:tc></w:tr>'
    # 列 3：vMerge continue（省略 w:val 的寫法，第 0 欄）；第二欄多段落
    '<w:tr>'
    '<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/><w:vMerge/></w:tcPr>'
    + para('') +
    '</w:tc>'
    '<w:tc><w:tcPr><w:tcW w:w="7000" w:type="dxa"/><w:gridSpan w:val="2"/></w:tcPr>'
    + para('續排段落一') + para('續排段落二') +
    '</w:tc></w:tr>'
    '</w:tbl>'
)

DOC = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f'<w:document {W} {CUS}><w:body>'
    + para('表格混合樣本（合併列／合併行／巢狀表格／邊框底紋／未知屬性）')
    + MAIN_TBL
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
    lh = struct.pack(
        "<IHHHHHIIIHH", 0x04034B50, 20, 0, 0, 0, 0, crc, len(data), len(data),
        len(n), 0)
    return lh + n + data, crc


def zip_central(name, data, crc, offset):
    n = name.encode()
    return struct.pack(
        "<IHHHHHHIIIHHHHHII", 0x02014B50, 20, 20, 0, 0, 0, 0, crc, len(data),
        len(data), len(n), 0, 0, 0, 0, 0x20, offset) + n


def main():
    locals_, centrals, off = [], [], 0
    for name, text in FILES:
        b = text.encode("utf-8")
        blob, crc = zip_local(name, b)
        locals_.append(blob)
        centrals.append(zip_central(name, b, crc, off))
        off += len(blob)
    local_blob = b"".join(locals_)
    central_blob = b"".join(centrals)
    eocd = struct.pack(
        "<IHHHHIIH", 0x06054B50, 0, 0, len(FILES), len(FILES),
        len(central_blob), len(local_blob), 0)
    out = local_blob + central_blob + eocd
    OUT.write_bytes(out)
    # 自證：python 標準庫 zipfile 必須視為合法 zip
    import io
    zf = __import__("zipfile").ZipFile(io.BytesIO(out))
    assert zf.testzip() is None, "zip CRC 驗證失敗"
    assert sorted(zf.namelist()) == sorted(n for n, _ in FILES)
    print(f"OK {OUT} ({len(out)} bytes)")


if __name__ == "__main__":
    main()
