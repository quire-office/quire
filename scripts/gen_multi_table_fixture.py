#!/usr/bin/env python3
"""生成 fixtures/table_multi.docx（E 卡 t_f91e5773：多表定位器樣本）。

含：**三張 body 級根表**（root_slot 0/1/2）、巢狀表掛 T1 r0c2（巢狀步
`(row, cell, sub)` 語義樣本）、gridSpan 跨欄格、vMerge restart/continue
對（continue 格**無段落**——空 tc 畸形容錯樣本）。定位器考卷
（`table_locator_json`）以本檔為真實輸入——單表檔測不到「點哪張改哪張」。

結構（全部段落皆在表內、無表間段落＝扁平段落索引最簡可核；段落由主解析器
按文件序編號，本檔逐格註記预期段落數供考卷核帳）：

T1（4 列 × 3 網格欄）
  r0：A1「根表一 r0c0」｜A2「根表一 r0c1」｜A3「根表一 r0c2」＋**巢狀表**
      （巢狀表 1 列 2 欄：「巢狀N1」／「巢狀N2」）
  r1：B1（gridSpan=3）「根表一 r1c0」＋「B1 第二段」
  r2：V1（vMerge restart）「根表一 r2c0」｜C1「根表一 r2c1」｜C2「根表一 r2c2」
  r3：V2（vMerge continue，**空 tc 無段落**）｜C3「根表一 r3c1」｜C4「根表一 r3c2」
T2（1 列 × 2 欄）：「根表二 r0c0」／「根表二 r0c1」
T3（2 列 × 2 欄）：「根表三 r0c0」…「根表三 r1c1」

純 stdlib、STORE 壓縮、固定時間戳（可重複生成、位元組可重現，
gen_table_fixture.py 同款口徑）。
"""
import io
import struct
import zipfile
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "fixtures" / "table_multi.docx"

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'


def para(text):
    return f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>'


def tc(paras, w=3000, extra=""):
    """一個 w:tc：tcPr（寬＋可選附加屬性）＋段落序列（空序列＝空 tc 畸形樣本）。"""
    inner = "".join(para(t) for t in paras)
    return f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/>{extra}</w:tcPr>{inner}</w:tc>'


NESTED_TBL = (
    "<w:tbl>"
    '<w:tblPr><w:tblW w:w="2400" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr>'
    '<w:tblGrid><w:gridCol w:w="1200"/><w:gridCol w:w="1200"/></w:tblGrid>'
    "<w:tr>" + tc(["巢狀N1"], 1200) + tc(["巢狀N2"], 1200) + "</w:tr>"
    "</w:tbl>"
)

T1 = (
    "<w:tbl>"
    + '<w:tblPr><w:tblW w:w="9000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr>'
    + '<w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>'
    # r0：三格，末格內掛巢狀表（巢狀步 (0, 2, 0)）。
    + "<w:tr>"
    + tc(["根表一 r0c0"])
    + tc(["根表一 r0c1"])
    + '<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>'
    + para("根表一 r0c2")
    + NESTED_TBL
    + "</w:tc>"
    + "</w:tr>"
    # r1：單格跨 3 欄（gridSpan），兩段文字。
    + "<w:tr>"
    + '<w:tc><w:tcPr><w:tcW w:w="9000" w:type="dxa"/><w:gridSpan w:val="3"/></w:tcPr>'
    + para("根表一 r1c0")
    + para("B1 第二段")
    + "</w:tc>"
    + "</w:tr>"
    # r2：vMerge restart ＋ 兩格。
    + "<w:tr>"
    + tc(["根表一 r2c0"], 3000, '<w:vMerge w:val="restart"/>')
    + tc(["根表一 r2c1"])
    + tc(["根表一 r2c2"])
    + "</w:tr>"
    # r3：vMerge continue 的**空 tc**（無段落，OOXML 合法但零段落）＋ 兩格。
    + "<w:tr>"
    + '<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:vMerge w:val="continue"/></w:tcPr></w:tc>'
    + tc(["根表一 r3c1"])
    + tc(["根表一 r3c2"])
    + "</w:tr>"
    + "</w:tbl>"
)

T2 = (
    "<w:tbl>"
    '<w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr>'
    '<w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>'
    "<w:tr>" + tc(["根表二 r0c0"]) + tc(["根表二 r0c1"]) + "</w:tr>"
    "</w:tbl>"
)

T3 = (
    "<w:tbl>"
    '<w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr>'
    '<w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>'
    "<w:tr>" + tc(["根表三 r0c0"]) + tc(["根表三 r0c1"]) + "</w:tr>"
    "<w:tr>" + tc(["根表三 r1c0"]) + tc(["根表三 r1c1"]) + "</w:tr>"
    "</w:tbl>"
)

DOC = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f"<w:document {W}><w:body>"
    + T1
    + T2
    + T3
    + '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>'
    '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
    'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
    "</w:body></w:document>"
)

CT = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Override PartName="/word/document.xml" '
    'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
    "</Types>"
)

RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
    'Target="word/document.xml"/>'
    "</Relationships>"
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
        "<IHHHHHHIIIHHHHHII", 0x02014B50, 20, 20, 0, 0, 0, 0, crc, len(data), len(data),
        len(n), 0, 0, 0, 0, 0x20, offset) + n


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
    zf = zipfile.ZipFile(io.BytesIO(out))
    assert zf.testzip() is None, "zip CRC 驗證失敗"
    assert sorted(zf.namelist()) == sorted(n for n, _ in FILES)
    print(f"OK {OUT} ({len(out)} bytes)")


if __name__ == "__main__":
    main()
