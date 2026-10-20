#!/usr/bin/env python3
"""產生 fixtures/theme_color.docx：主題色查表（w:themeColor → theme1.xml）端到端 fixture。

覆蓋四種形態（每條對應一個引擎層斷言——顯示列表的 color_argb）：
1. **字元樣式層**：`styles.xml` 的 Hyperlink 字元樣式宣告
   `<w:color w:val="0000FF" w:themeColor="hyperlink"/>`，run 只帶 rStyle 引用
   → 級聯有效色必須是**主題 hlink＝0x305090**（LO 實測連結色），非 w:val 純藍。
2. **run 直接格式層**：`<w:color w:val="0000FF" w:themeColor="hyperlink"/>` → 同樣 0x305090。
3. **查無該名**：`w:themeColor="accent9"` → 老實退回 `w:val`＝0x123456（容錯）。
4. **帶變換**：`w:themeColor="accent1" w:themeShade="BF"` → 正確色是「變換後」色
   （lumMod/lumOff 本卡未實作）→ 保留 Word 預算好的 `w:val`＝0x365F91
   （不得拿未變換的 accent1 0x4F81BD 冒充）。

zip 一律 STORE：zip_struct 雙方法皆支援，fixture 位元組穩定可重現。
"""
import pathlib
import sys
import zipfile

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/word/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
</Types>"""

ROOT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""

DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rIdTheme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>
</Relationships>"""

# 主題 part：hlink＝305090（LO 實測連結色）；dk1/lt1 用 sysClr（lastClr 路徑）；
# accent1＝4F81BD（帶 themeShade 的對照組）。
THEME = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme">
<a:themeElements>
<a:clrScheme name="Office">
<a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
<a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
<a:dk2><a:srgbClr val="1F497D"/></a:dk2>
<a:lt2><a:srgbClr val="EEECE1"/></a:lt2>
<a:accent1><a:srgbClr val="4F81BD"/></a:accent1>
<a:accent2><a:srgbClr val="C0504D"/></a:accent2>
<a:accent3><a:srgbClr val="9BBB59"/></a:accent3>
<a:accent4><a:srgbClr val="8064A2"/></a:accent4>
<a:accent5><a:srgbClr val="4BACC6"/></a:accent5>
<a:accent6><a:srgbClr val="F79646"/></a:accent6>
<a:hlink><a:srgbClr val="305090"/></a:hlink>
<a:folHlink><a:srgbClr val="800080"/></a:folHlink>
</a:clrScheme>
</a:themeElements>
</a:theme>"""

# docDefaults 12pt；Hyperlink 字元樣式帶主題色宣告（w:val 只是回退值）。
STYLES = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles {W}>
<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"/>
<w:style w:type="character" w:styleId="Hyperlink">
  <w:rPr><w:color w:val="0000FF" w:themeColor="hyperlink"/><w:u w:val="single"/></w:rPr>
</w:style>
</w:styles>"""

BODY = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document {W}><w:body>
<w:p><w:r><w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr><w:t>樣式層主題色</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:color w:val="0000FF" w:themeColor="hyperlink"/></w:rPr><w:t>直接格式主題色</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:color w:val="123456" w:themeColor="accent9"/></w:rPr><w:t>查無該名退回回退值</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:color w:val="365F91" w:themeColor="accent1" w:themeShade="BF"/></w:rPr><w:t>帶著色保留預算色</w:t></w:r></w:p>
<w:sectPr>
<w:pgSz w:w="11906" w:h="16838"/>
<w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800" w:header="720" w:footer="720" w:gutter="0"/>
</w:sectPr>
</w:body></w:document>"""


def main(out: pathlib.Path):
    with zipfile.ZipFile(out, "w", zipfile.ZIP_STORED) as z:
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        z.writestr("_rels/.rels", ROOT_RELS)
        z.writestr("word/document.xml", BODY)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/styles.xml", STYLES)
        z.writestr("word/theme/theme1.xml", THEME)
    print(f"寫入 {out}（{out.stat().st_size} bytes）")


if __name__ == "__main__":
    fixtures = pathlib.Path(__file__).resolve().parent.parent / "fixtures"
    main(pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else fixtures / "theme_color.docx")
