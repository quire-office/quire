#!/usr/bin/env python3
"""生成 core/engine-api/tests/fixtures/r35_txbx_state.docx（R35 紅燈考卷 selfgen 樣本）。

形態（卡面規格，逐盞燈對位）：
- 框1（卡 1／卡 3 誤殺）：VML `v:shape`（200x140pt，style 宣告）同時帶
  `v:imagedata r:id=rIdImgC`（外框自身圖）與 `v:textbox`；框內兩段——
  首段「首段」含 **兩張** 內嵌 `w:drawing`（rIdImgA 40x30pt、rIdImgB 30x20pt，
  同段多圖），次段「末段」。修前（R31 態）：單槽覆寫只發末張＋finalize
  整批誤殺清掉 C ⇒ 三張全滅或只剩一張。
- 框2（卡 4）：框內首段含一張**無 `wp:extent`** 的內嵌圖（rIdImgD）、次段為
  空段。修前：`ext.unwrap_or([0,0])` 刷 0x0 灰 Rect 吞掉空段步進。
- 框3（卡 5 狀態洩漏）：畸形閉合順序——`</w:txbxContent>` 先於 `</w:drawing>`
  閉合（blip 在 txbx 深度內採集、drawing 在深度外閉合），隨後第二個純文字
  pict（框4「乙框文字」）。修前：pending 殘留跨框，框4 誤發幽靈圖。

純 stdlib、STORE 壓縮、固定時間戳（可重複生成、位元組可重現）。
`roundtrip fixtures --check` 必須視其為無損閘門的一員。
"""
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "fixtures" / "r35_txbx_state.docx"

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
R = 'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
WP = 'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"'
A = 'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"'
PIC = 'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"'
V = 'xmlns:v="urn:schemas-microsoft-com:vml"'
O = 'xmlns:o="urn:schemas-microsoft-com:office:office"'


def chunk(t, d):
    return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)


def solid_png(w, h, rgb):
    row = b"\x00" + bytes(rgb) * w
    raw = row * h
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw))
            + chunk(b"IEND", b""))


# 三張辨色像素圖（A 紅 2x2、B 綠 3x2、C 藍 2x2）＋ D 灰 1x1（位元組互異，
# base64 前綴可判別池配對）。
PNG_A = solid_png(2, 2, (255, 0, 0))
PNG_B = solid_png(3, 2, (0, 200, 0))
PNG_C = solid_png(2, 2, (0, 0, 255))
PNG_D = solid_png(1, 1, (128, 128, 128))


def inline_drawing(rid, cx, cy, docpr):
    """框內段落流內嵌 w:drawing（wp:inline＋wp:extent＋pic blip r:embed）。"""
    ext = f'<wp:extent cx="{cx}" cy="{cy}"/>' if cx else ""
    return (
        "<w:r><w:rPr><w:noProof/></w:rPr><w:drawing>"
        f'<wp:inline distT="0" distB="0" distL="0" distR="0" {WP}>'
        + ext
        + f'<wp:docPr id="{docpr}" name="r35图{docpr}"/>'
        + f'<a:graphic {A}><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
        + f'<pic:pic {PIC}>'
        f'<pic:nvPicPr><pic:cNvPr id="{docpr}" name="{rid}.png"/><pic:cNvPicPr/></pic:nvPicPr>'
        "<pic:blipFill>"
        f'<a:blip {R} r:embed="{rid}"/>'
        "<a:stretch><a:fillRect/></a:stretch>"
        "</pic:blipFill>"
        "<pic:spPr>"
        '<a:xfrm><a:off x="0" y="0"/><a:ext cx="12700" cy="12700"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>'
        "</pic:spPr>"
        "</pic:pic>"
        "</a:graphicData></a:graphic>"
        "</wp:inline></w:drawing></w:r>"
    )


# EMU：1pt=12700。A=40x30pt、B=30x20pt。
EMU = 12700
DRAW_A = inline_drawing("rIdImgA", 40 * EMU, 30 * EMU, 1)
DRAW_B = inline_drawing("rIdImgB", 30 * EMU, 20 * EMU, 2)
DRAW_D = inline_drawing("rIdImgD", None, None, 4)  # 卡 4：無 wp:extent

BOX1 = (
    "<w:p><w:r><w:t>R35宿主段</w:t></w:r></w:p>"
    f'<w:p><w:r><w:pict {V} {O}>'
    '<v:shape type="#_x0000_t202" style="width:200pt;height:140pt" o:allowincell="f">'
    f'<v:imagedata {R} r:id="rIdImgC"/>'
    '<v:textbox><w:txbxContent>'
    f"<w:p><w:r><w:t>首段</w:t></w:r>{DRAW_A}{DRAW_B}</w:p>"
    "<w:p><w:r><w:t>末段</w:t></w:r></w:p>"
    "</w:txbxContent></v:textbox>"
    "</v:shape></w:pict></w:r></w:p>"
)

BOX2 = (
    f'<w:p><w:r><w:pict {V} {O}>'
    '<v:shape type="#_x0000_t202" style="width:150pt;height:90pt">'
    '<v:textbox><w:txbxContent>'
    f"<w:p><w:r><w:t>无图段</w:t></w:r>{DRAW_D}</w:p>"
    "<w:p></w:p>"
    "<w:p><w:r><w:t>步进步照</w:t></w:r></w:p>"
    "</w:txbxContent></v:textbox>"
    "</v:shape></w:pict></w:r></w:p>"
)

# 卡 2（svgBlip/r:link 形態，corpus 零實例⇒合成重現）：同張 drawing 先
# embed（rIdImgA）再 link-only blip——R31 單槽 `entry.0 = rid` 無條件覆寫
# 使 None 蓋掉已捕 A（A 全滅）；次段再放一張正常 B（extent 18x12，令
# finalize 誤殺臂在 BUGGY 態照樣觸發，B 走段落流倖存——紅燈鑑別力所在）。
DRAW5A = (
    "<w:r><w:rPr><w:noProof/></w:rPr><w:drawing>"
    f'<wp:inline distT="0" distB="0" distL="0" distR="0" {WP}>'
    f'<wp:extent cx="{25 * EMU}" cy="{15 * EMU}"/>'
    '<wp:docPr id="5" name="r35图5"/>'
    + f'<a:graphic {A}><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
    + f'<pic:pic {PIC}>'
    '<pic:nvPicPr><pic:cNvPr id="5" name="rIdImgA.png"/><pic:cNvPicPr/></pic:nvPicPr>'
    "<pic:blipFill>"
    f'<a:blip {R} r:embed="rIdImgA"/><a:blip {R} r:link="rIdZLink"/>'
    "<a:stretch><a:fillRect/></a:stretch>"
    "</pic:blipFill>"
    '<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="12700" cy="12700"/></a:xfrm>'
    '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>'
    "</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
)
DRAW5B = inline_drawing("rIdImgB", 18 * EMU, 12 * EMU, 6)  # B 第二尺寸＝18x12

BOX5 = (
    f'<w:p><w:r><w:pict {V} {O}>'
    '<v:shape type="#_x0000_t202" style="width:120pt;height:80pt">'
    '<v:textbox><w:txbxContent>'
    f"<w:p><w:r><w:t>link段</w:t></w:r>{DRAW5A}{DRAW5B}</w:p>"
    "</w:txbxContent></v:textbox>"
    "</v:shape></w:pict></w:r></w:p>"
)

# 卡 5：畸形閉合——blip 在 txbx 深度內採集，`</w:drawing>` 在 txbxContent
# 之後才閉合（quick_xml 不驗證嵌套，照實餵）。
BOX3 = (
    f'<w:p><w:r><w:pict {V} {O}>'
    '<v:shape type="#_x0000_t202" style="width:120pt;height:60pt">'
    '<v:textbox><w:txbxContent>'
    f"<w:p><w:r><w:t>洩漏源</w:t></w:r><w:drawing>"
    f'<wp:inline distT="0" distB="0" distL="0" distR="0" {WP}>'
    f'<wp:extent cx="{20 * EMU}" cy="{10 * EMU}"/>'
    '<wp:docPr id="3" name="r35图3"/>'
    + f'<a:graphic {A}><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
    + f'<pic:pic {PIC}>'
    '<pic:nvPicPr><pic:cNvPr id="3" name="rIdImgA.png"/><pic:cNvPicPr/></pic:nvPicPr>'
    "<pic:blipFill>"
    f'<a:blip {R} r:embed="rIdImgA"/>'
    "<a:stretch><a:fillRect/></a:stretch>"
    "</pic:blipFill>"
    '<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="12700" cy="12700"/></a:xfrm>'
    '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>'
    "</pic:pic></a:graphicData></a:graphic></wp:inline>"
    "</w:txbxContent></w:drawing></w:r></w:p>"  # 畸形：txbxContent 先閉、drawing 後閉
    "</v:shape></w:pict></w:r></w:p>"
    '<w:p><w:r><w:pict %s %s>'
    '<v:shape type="#_x0000_t202" style="width:110pt;height:50pt">'
    '<v:textbox><w:txbxContent>'
    "<w:p><w:r><w:t>乙框文字</w:t></w:r></w:p>"
    "</w:txbxContent></v:textbox>"
    "</v:shape></w:pict></w:r></w:p>" % (V, O)
)

DOC = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f"<w:document {W} {R} {V} {O}><w:body>"
    + BOX1
    + BOX2
    + BOX3
    + BOX5
    + '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/>'
    '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
    'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
    "</w:body></w:document>"
)

CONTENT_TYPES = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="png" ContentType="image/png"/>'
    '<Override PartName="/word/document.xml" '
    'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
    "</Types>"
)

ROOT_RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
    'Target="word/document.xml"/></Relationships>'
)

DOC_RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rIdImgA" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" '
    'Target="media/imageA.png"/>'
    '<Relationship Id="rIdImgB" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" '
    'Target="media/imageB.png"/>'
    '<Relationship Id="rIdImgC" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" '
    'Target="media/imageC.png"/>'
    '<Relationship Id="rIdImgD" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" '
    'Target="media/imageD.png"/>'
    "</Relationships>"
)


def crc32(b):
    return zlib.crc32(b) & 0xFFFFFFFF


def build_zip(entries):
    buf = bytearray()
    centrals = []
    for name, data in entries:
        nb = name.encode()
        off = len(buf)
        crc = crc32(data)
        buf += struct.pack("<IHHHHHIIIHH", 0x04034B50, 20, 0, 0, 0x1234, 0x5678,
                           crc, len(data), len(data), len(nb), 0)
        buf += nb + data
        centrals.append(struct.pack("<IHHHHHHIIIHHHHHII", 0x02014B50, 20, 20, 0, 0,
                                    0x1234, 0x5678, crc, len(data), len(data),
                                    len(nb), 0, 0, 0, 0, 0, off) + nb)
    cd_off = len(buf)
    for c in centrals:
        buf += c
    cd_size = len(buf) - cd_off
    buf += struct.pack("<IHHHHIIH", 0x06054B50, 0, 0, len(entries), len(entries),
                       cd_size, cd_off, 0)
    return bytes(buf)


def main():
    entries = [
        ("[Content_Types].xml", CONTENT_TYPES.encode()),
        ("_rels/.rels", ROOT_RELS.encode()),
        ("word/document.xml", DOC.encode()),
        ("word/_rels/document.xml.rels", DOC_RELS.encode()),
        ("word/media/imageA.png", PNG_A),
        ("word/media/imageB.png", PNG_B),
        ("word/media/imageC.png", PNG_C),
        ("word/media/imageD.png", PNG_D),
    ]
    OUT.write_bytes(build_zip(entries))
    print(f"寫入 {OUT}（{OUT.stat().st_size} bytes）")


if __name__ == "__main__":
    main()
