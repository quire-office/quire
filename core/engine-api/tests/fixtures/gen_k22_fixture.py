#!/usr/bin/env python3
"""產生 K22 紅燈的合成最小檔（tblStylePr 條件**字色**級聯）。

紅燈規格（卡面 K22 燈2/燈3）：
- `firstRow` 條件：只宣告 `w:rPr/w:color val=FF0000`（白底紅字——條件字色
  若只作用在 firstRow 列、不外溢，資料列文字必須仍是預設黑）。
- `firstCol` 條件：**只宣告字色**（0000FF）不宣告底紋——縱軸條件也走同軸
  級聯鎖（實作不得只特化 firstRow）。
- `band1Horz` 條件：只宣告底紋 D3DFEE（無字色）——橫帶逐屬性級聯、字色
  續條不受影響。
- 表頭格「H1」（firstRow∩firstCol 交會格）的 run 帶**直接** `w:color=00AA00`
  ——直接字色 > 條件字色的優先序鎖（燈3：綠直接色勝紅條件色，且因
  firstRow 優先序高於 firstCol，若級聯誤走 firstCol 也會是藍——三色互斥，
  哪個落錯都抓得到）。
- `w:tblLook w:val="04A0"`＝ECMA 預設（firstRow 開／firstCol 開／橫帶開／
  縱帶預設關）——band1Vert/lastCol 的字色宣告因此永不生效（tblLook 閘鎖）。

產生方式與本倉其他 fixture 產生器同款：zip + 五個 part，無外部依賴。
"""
import os
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "k22_cond_color.docx")

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"""

ROOT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"""

DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"""

STYLES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:tblPr><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style><w:style w:type="table" w:styleId="K22Cond"><w:name w:val="K22 Cond"/><w:basedOn w:val="TableNormal"/><w:tblPr><w:tblStyleRowBandSize w:val="1"/><w:tblStyleColBandSize w:val="1"/></w:tblPr><w:tblStylePr w:type="firstRow"><w:rPr><w:color w:val="FF0000"/></w:rPr></w:tblStylePr><w:tblStylePr w:type="firstCol"><w:rPr><w:color w:val="0000FF"/></w:rPr></w:tblStylePr><w:tblStylePr w:type="band1Horz"><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="D3DFEE"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type="band1Vert"><w:rPr><w:color w:val="FFFF00"/></w:rPr></w:tblStylePr><w:tblStylePr w:type="lastCol"><w:rPr><w:color w:val="FFFFFF"/></w:rPr></w:tblStylePr></w:style></w:styles>"""


def cell(text, direct_color=None):
    rpr = f'<w:rPr><w:color w:val="{direct_color}"/></w:rPr>' if direct_color else ""
    return (
        '<w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr>'
        f"<w:p><w:r>{rpr}<w:t>{text}</w:t></w:r></w:p></w:tc>"
    )


def row(cells):
    return "<w:tr>" + "".join(cells) + "</w:tr>"


DOCUMENT = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>'
    '<w:tbl><w:tblPr><w:tblStyle w:val="K22Cond"/>'
    '<w:tblW w:w="0" w:type="auto"/>'
    '<w:tblLook w:val="04A0" w:firstRow="1" w:lastRow="0" w:firstColumn="1" '
    'w:lastColumn="0" w:noHBand="0" w:noVBand="1"/></w:tblPr>'
    '<w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="4000"/></w:tblGrid>'
    + row([cell("H1", "00AA00"), cell("H2")])
    + row([cell("R1C1"), cell("R1C2")])
    + row([cell("R2C1"), cell("R2C2")])
    + "</w:tbl>"
    "</w:body></w:document>"
)


def main():
    with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        z.writestr("_rels/.rels", ROOT_RELS)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/document.xml", DOCUMENT)
        z.writestr("word/styles.xml", STYLES)
    print(f"write {OUT} ({os.path.getsize(OUT)} bytes)")


if __name__ == "__main__":
    main()
