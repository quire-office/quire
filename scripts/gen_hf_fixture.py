#!/usr/bin/env python3
"""產生 fixtures/hf_table_header.docx：頁首為 w:tbl 公文表頭（t_b39f108b）。

結構（最小但完整）：
- [Content_Types].xml＋_rels/.rels＋word/document.xml（正文兩段、單 section、
  headerReference default→rIdHdr、footerReference default→rIdFtr、pgMar
  header=432/footer=720、titlePg 開＋first header→rIdFirst）
- word/_rels/document.xml.rels：rIdHdr→header1.xml、rIdFtr→footer1.xml、
  rIdFirst→header2.xml
- word/header1.xml：w:hdr 根＋一張單欄 w:tbl（框線 single）包「公文表頭」段
- word/header2.xml：w:hdr＋「首頁專用頁首」段（titlePg 驗證）
- word/footer1.xml：w:ftr＋PAGE 欄位（複雜欄 fldChar/instrText）＋「 / NUMPAGES
  簡單欄」——驗證頁碼代換

zip 一律 STORE（無壓縮）：zip_struct 雙方法皆支援，fixture 位元組穩定可重现。
"""
import zipfile, sys, pathlib

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
R = 'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
<Override PartName="/word/header2.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
<Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>
</Types>"""

ROOT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""

DOC_RELS = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdHdr" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
<Relationship Id="rIdFirst" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header2.xml"/>
<Relationship Id="rIdFtr" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>
</Relationships>"""

# 頁首表頭：單欄表、全框線 single，格內一段「公文表頭」。
HEADER_TBL = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr {W} {R}><w:p><w:r><w:t>公文表頭</w:t></w:r></w:p>
<w:tbl><w:tblPr><w:tblW w:w="9360" w:type="dxa"/>
<w:tblBorders><w:top w:val="single" w:sz="8" w:space="0" w:color="000000"/><w:left w:val="single" w:sz="8" w:space="0" w:color="000000"/><w:bottom w:val="single" w:sz="8" w:space="0" w:color="000000"/><w:right w:val="single" w:sz="8" w:space="0" w:color="000000"/></w:tblBorders>
</w:tblPr><w:tblGrid><w:gridCol w:w="9360"/></w:tblGrid>
<w:tr><w:tc><w:tcPr><w:tcW w:w="9360" w:type="dxa"/></w:tcPr>
<w:p><w:r><w:t>機關名義：奎爾文書局</w:t></w:r></w:p>
</w:tc></w:tr></w:tbl></w:hdr>"""

HEADER_FIRST = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr {W} {R}><w:p><w:r><w:t>首頁專用頁首</w:t></w:r></w:p></w:hdr>"""

# 頁尾：PAGE 用複雜欄（fldChar begin/instrText/separate/text/end），
# NUMPAGES 用簡單欄（fldSimple）。
FOOTER = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr {W} {R}><w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>過期快取</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>
<w:r><w:t xml:space="preserve"> / </w:t></w:r>
<w:fldSimple w:instr=" NUMPAGES "><w:r><w:t>過期總數</w:t></w:r></w:fldSimple>
</w:p></w:ftr>"""

BODY = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document {W} {R}><w:body>
<w:p><w:r><w:t>正文第一段</w:t></w:r></w:p>
{''.join(f'<w:p><w:r><w:t>正文填充第{i}行，湊滿第一頁以驗證第二頁走 default 表格頁首。</w:t></w:r></w:p>' for i in range(90))}
<w:sectPr>
<w:headerReference w:type="default" r:id="rIdHdr"/>
<w:headerReference w:type="first" r:id="rIdFirst"/>
<w:footerReference w:type="default" r:id="rIdFtr"/>
<w:pgSz w:w="11906" w:h="16838"/>
<w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800" w:header="432" w:footer="720" w:gutter="0"/>
<w:titlePg/>
</w:sectPr>
</w:body></w:document>"""

def main(out: pathlib.Path):
    with zipfile.ZipFile(out, "w", zipfile.ZIP_STORED) as z:
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        z.writestr("_rels/.rels", ROOT_RELS)
        z.writestr("word/document.xml", BODY)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/header1.xml", HEADER_TBL)
        z.writestr("word/header2.xml", HEADER_FIRST)
        z.writestr("word/footer1.xml", FOOTER)
    print(f"寫入 {out}（{out.stat().st_size} bytes）")

if __name__ == "__main__":
    out = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parent.parent / "fixtures" / "hf_table_header.docx"
    main(out)
