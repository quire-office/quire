#!/usr/bin/env python3
"""生成 fixtures/inline_image.docx（t_a1397522 圖片幾何佔位框 selfgen 樣本）。

形態（卡片規格）：一段文字＋一張 100pt 高 inline 圖＋一段文字。
- `wp:inline`＋`wp:extent cx/cy`（EMU）：cx=2540000（200pt）、cy=1270000（100pt）。
- `a:blip r:embed` 指向 word/media/image1.png（1×1 像素，僅供合法性；本卡
  只做幾何佔位，點陣直出屬像素渲染卡）。
- `wp:anchor` 變體走引擎測試的 set_part 路徑（同檔型 XML 字串），不在本檔。

純 stdlib、STORE 壓縮、固定時間戳（可重複生成、位元組可重現）——
`roundtrip fixtures --check` 必須視其為無損閘門的一員。
"""
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "fixtures" / "inline_image.docx"

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
R = 'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
WP = 'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"'
A = 'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"'
PIC = 'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"'

# 1×1 透明像素（合法最小 PNG；rIdImg 的實際位元組，round-trip 不動它）。
PNG_1PX = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489"
    "0000000a49444154789c63000100000500010d0a2db40000000049454e44ae426082"
)

# 圖片段落：200×100pt inline 圖（extent 是佔位框的唯一依據）。
IMAGE_PARA = (
    "<w:p><w:r><w:drawing>"
    f'<wp:inline distT="0" distB="0" distL="0" distR="0" {WP}>'
    '<wp:extent cx="2540000" cy="1270000"/>'
    '<wp:docPr id="1" name="佔位圖"/>'
    f"<a:graphic {A}><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">"
    f'<pic:pic {PIC}>'
    "<pic:nvPicPr><pic:cNvPr id=\"1\" name=\"image1.png\"/><pic:cNvPicPr/></pic:nvPicPr>"
    "<pic:blipFill>"
    f'<a:blip {R} r:embed="rIdImg"/>'
    "<a:stretch><a:fillRect/></a:stretch>"
    "</pic:blipFill>"
    "<pic:spPr>"
    '<a:xfrm><a:off x="0" y="0"/><a:ext cx="2540000" cy="1270000"/></a:xfrm>'
    '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>'
    "</pic:spPr>"
    "</pic:pic>"
    "</a:graphicData></a:graphic>"
    "</wp:inline></w:drawing></w:r></w:p>"
)

DOC = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f"<w:document {W} {R}><w:body>"
    "<w:p><w:r><w:t>圖片前段：占位框登場之前的文字。</w:t></w:r></w:p>"
    + IMAGE_PARA
    + "<w:p><w:r><w:t>圖片後段：占位框之後的文字。</w:t></w:r></w:p>"
    '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/>'
    '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
    'w:header="708" w:footer="708"/></w:sectPr>'
    "</w:body></w:document>"
)

CT = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Default Extension="png" ContentType="image/png"/>'
    '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
    '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
    "</Types>"
)

RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
    "</Relationships>"
)

DOC_RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
    '<Relationship Id="rIdImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>'
    "</Relationships>"
)

STYLES = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f'<w:styles {W}><w:docDefaults><w:rPrDefault><w:rPr>'
    '<w:rFonts w:ascii="Calibri" w:eastAsia="PMingLiU" w:hAnsi="Calibri"/>'
    '<w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>'
    '<w:style w:type="paragraph" w:default="1" w:styleId="Normal">'
    '<w:name w:val="Normal"/></w:style></w:styles>'
)

FILES = [
    ("[Content_Types].xml", CT.encode()),
    ("_rels/.rels", RELS.encode()),
    ("word/document.xml", DOC.encode()),
    ("word/_rels/document.xml.rels", DOC_RELS.encode()),
    ("word/styles.xml", STYLES.encode()),
    ("word/media/image1.png", PNG_1PX),
]


def zip_local(name: str, data: bytes):
    crc = zlib.crc32(data) & 0xFFFFFFFF
    n = name.encode()
    lh = struct.pack(
        "<IHHHHHIIIHH", 0x04034B50, 20, 0, 0, 0, 0, crc, len(data), len(data),
        len(n), 0)
    return lh + n + data, crc


def zip_central(name: str, data: bytes, crc: int, offset: int) -> bytes:
    n = name.encode()
    return struct.pack(
        "<IHHHHHHIIIHHHHHII", 0x02014B50, 20, 20, 0, 0, 0, 0, crc, len(data),
        len(data), len(n), 0, 0, 0, 0, 0x20, offset) + n


def main():
    locals_, centrals, off = [], [], 0
    for name, data in FILES:
        blob, crc = zip_local(name, data)
        locals_.append(blob)
        centrals.append(zip_central(name, data, crc, off))
        off += len(blob)
    local_blob = b"".join(locals_)
    central_blob = b"".join(centrals)
    eocd = struct.pack(
        "<IHHHHIIH", 0x06054B50, 0, 0, len(FILES), len(FILES),
        len(central_blob), len(local_blob), 0)
    out = local_blob + central_blob + eocd
    OUT.write_bytes(out)
    import io
    zf = __import__("zipfile").ZipFile(io.BytesIO(out))
    assert zf.testzip() is None, "zip CRC 驗證失敗"
    assert sorted(zf.namelist()) == sorted(n for n, _ in FILES)
    print(f"OK {OUT} ({len(out)} bytes)")


if __name__ == "__main__":
    main()
