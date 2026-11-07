"""G1 考卷合成檔生成器（全合成，零私密內容；署名 ADT109119）。

形態＝用戶實測檔（AI_.docx）的結構縮影（結構取自解剖報告，文字全佔位）：
- A4（11906×16838 twips），邊距 左/右 1800、上/下 1440 twips（=90/72pt）。
- 跨頁長表製造「續列」場景。
- 錨定表＝2 列 × 2 格，**首列為高墊列**（trHeight 9000 atLeast）→ 錨定列
  必然被推到次頁（燈 1 的判别場景：舊「表格流起始頁」口徑掛錯頁）。
- 錨定列每格一枚 `wp:anchor`（pic:pic + blip）：
    A：relativeHeight=2，positionH relativeFrom=column ＋ <wp:align>center</wp:align>
    B：relativeHeight=3，positionH relativeFrom=column ＋ posOffset=0
    兩者 positionV relativeFrom=paragraph posOffset=635 EMU（=0.05pt）、
    extent 2567305×1443990 EMU（=202.1×113.7pt）、wrapSquare wrapText="largest"、
    layoutInCell=0、behindDoc=0。
- 錨定表帶 2pt 實框（tblBorders sz=16）：考卷用橫線 y 錨定「列框頂」度量的基準。

LO 真值口徑（law probe 實測，docs/g1-anchor-relativity-anatomy.md）：
  圖頂 y ＝ 宿主列框頂線（中線）＋ 格上內距 ＋ posOffset（±0.05）
  圖左 x ＝ 格框左線 ＋ 格左內距 ＋ posOffset（align=center 時按**內容盒**置中）
"""
import struct
import sys
import zipfile
import zlib

OUT = sys.argv[1] if len(sys.argv) > 1 else "g1_anchor_paging.docx"

CT = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="png" ContentType="image/png"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"""

RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""

DOCREL = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdImgA" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/imageA.png"/>
<Relationship Id="rIdImgB" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/imageB.png"/>
</Relationships>"""

NS = (
    'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
    'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
    'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
    'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"'
)


def png(rgb):
    """4×3 純色 PNG（合成色塊，非照片）。"""
    w = h = 4
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))

    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw))
            + chunk(b"IEND", b""))


def anchor(relh, h_inner, rid):
    """一枚 wp:anchor（pic:pic）；h_inner＝positionH 內式（align 或 posOffset）。"""
    return (
        f'<w:r><w:drawing>'
        f'<wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" '
        f'relativeHeight="{relh}" behindDoc="0" locked="0" layoutInCell="0" allowOverlap="1">'
        f'<wp:simplePos x="0" y="0"/>'
        f'<wp:positionH relativeFrom="column">{h_inner}</wp:positionH>'
        f'<wp:positionV relativeFrom="paragraph"><wp:posOffset>635</wp:posOffset></wp:positionV>'
        f'<wp:extent cx="2567305" cy="1443990"/>'
        f'<wp:effectExtent l="0" t="0" r="0" b="0"/>'
        f'<wp:wrapSquare wrapText="largest"/>'
        f'<wp:docPr id="{relh}" name="Picture {relh}"/>'
        f'<wp:cNvGraphicFramePr/>'
        f'<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
        f'<pic:pic><pic:nvPicPr><pic:cNvPr id="{relh}" name="p{relh}.png"/><pic:cNvPicPr/></pic:nvPicPr>'
        f'<pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2567305" cy="1443990"/></a:xfrm>'
        f'<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>'
        f'</a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>'
    )


def p(text="", extra=""):
    t = f'<w:r><w:t>{text}</w:t></w:r>' if text else ""
    return f"<w:p>{t}{extra}</w:p>"


def cell(w, inner):
    return f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr>{inner}</w:tc>'


def row(cells, trpr=""):
    return f"<w:tr>{trpr}{''.join(cells)}</w:tr>"


# 錨定表 tblPr：tblW 5000 pct／jc left／tblInd 55 dxa／fixed／cellMar 55 四邊
# （=2.75pt，與實測檔同尺）＋ 2pt 實框（考卷度量基準）。
ANCHOR_TBLPR = (
    '<w:tblW w:w="5000" w:type="pct"/><w:jc w:val="left"/>'
    '<w:tblInd w:w="55" w:type="dxa"/><w:tblLayout w:type="fixed"/>'
    "<w:tblBorders>"
    '<w:top w:val="single" w:sz="16" w:space="0" w:color="000000"/>'
    '<w:left w:val="single" w:sz="16" w:space="0" w:color="000000"/>'
    '<w:bottom w:val="single" w:sz="16" w:space="0" w:color="000000"/>'
    '<w:right w:val="single" w:sz="16" w:space="0" w:color="000000"/>'
    '<w:insideH w:val="single" w:sz="16" w:space="0" w:color="000000"/>'
    '<w:insideV w:val="single" w:sz="16" w:space="0" w:color="000000"/>'
    "</w:tblBorders>"
    "<w:tblCellMar>"
    '<w:top w:w="55" w:type="dxa"/><w:left w:w="55" w:type="dxa"/>'
    '<w:bottom w:w="55" w:type="dxa"/><w:right w:w="55" w:type="dxa"/>'
    "</w:tblCellMar>"
)


def table(grid, rows, tblpr=None):
    g = "".join(f'<w:gridCol w:w="{w}"/>' for w in grid)
    tp = tblpr or '<w:tblW w:w="0" w:type="auto"/><w:tblLayout w:type="fixed"/>'
    return (
        f"<w:tbl><w:tblPr>{tp}</w:tblPr>"
        f'<w:tblGrid>{g}</w:tblGrid>{"".join(rows)}</w:tbl>'
    )


# ── 內容 ──────────────────────────────────────────────────────────────
body = [p("測試文件標題")]
for i in range(10):
    body.append(p(f"前言段落 {i}：" + "內容文字" * 12))

# 跨頁長表（3 欄 11 列）：製造 p1 開頭「表格續列」壓頂的場景。
grid1 = [704, 1983, 5609]
rows1 = [row([cell(w, p(f"明細{r}{c} " + "資料" * 10)) for c, w in enumerate(grid1)])
         for r in range(11)]
body.append(table(grid1, rows1))

for i in range(12):
    body.append(p(f"結語段落 {i}：" + "說明文字" * 14))
body.append(p("【合成錨定段效果】"))

# 錨定表：列 0＝高墊列（450pt atLeast）把錨定列推落次頁；列 1＝錨定列。
grid2 = [4153, 4153]
spacer = row([cell(4153, p("錨表首列墊高文字 " * 12)), cell(4153, p("墊高第二格 " * 12))],
             '<w:trPr><w:trHeight w:val="9000" w:hRule="atLeast"/></w:trPr>')
anchor_row = row(
    [
        cell(4153, p("", anchor(2, "<wp:align>center</wp:align>", "rIdImgA"))),
        cell(4153, p("", anchor(3, "<wp:posOffset>0</wp:posOffset>", "rIdImgB"))),
    ],
    '<w:trPr><w:trHeight w:val="2955" w:hRule="atLeast"/></w:trPr>',
)
body.append(table(grid2, [spacer, anchor_row], tblpr=ANCHOR_TBLPR))
for i in range(4):
    body.append(p(f"尾部段落 {i}"))

DOC = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
       f'<w:document {NS}><w:body>{"".join(body)}'
       f'<w:sectPr><w:pgSz w:w="11906" w:h="16838"/>'
       f'<w:pgMar w:left="1800" w:right="1800" w:gutter="0" w:header="0" '
       f'w:top="1440" w:footer="0" w:bottom="1440"/></w:sectPr></w:body></w:document>')

with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("[Content_Types].xml", CT)
    z.writestr("_rels/.rels", RELS)
    z.writestr("word/_rels/document.xml.rels", DOCREL)
    z.writestr("word/document.xml", DOC)
    z.writestr("word/media/imageA.png", png((30, 60, 160)))
    z.writestr("word/media/imageB.png", png((170, 40, 40)))
print("wrote", OUT)
