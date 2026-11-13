"""K24 燈2 合成最小檔生成器（fixture 落地 core/engine-api/tests/fixtures/）。

形態＝shapes-with-text 真檔的縮影：一個 wp:inline＋wps:wsp，色走
`wps:style` fillRef idx=1 schemeClr accent1（theme accent1=FF0000 系合成值）。
另附第二個 wsp：spPr 直下 solidFill srgbClr=00FF00（優先序對照組）。
"""
import zipfile

CT = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
</Types>"""

RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""

DOCREL = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdT" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>
</Relationships>"""

NS = ('xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
      'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
      'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
      'xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"')

def anchor(i, inner):
    return (f'<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" '
            f'relativeHeight="{100+i}" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">'
            f'<wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>{914400*i}</wp:posOffset></wp:positionH>'
            f'<wp:positionV relativeFrom="paragraph"><wp:posOffset>{914400*i}</wp:posOffset></wp:positionV>'
            f'<wp:extent cx="914400" cy="914400"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/>'
            f'<wp:docPr id="{i+1}" name="Shape {i}"/><wp:cNvGraphicFramePr/>'
            f'<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">'
            f'{inner}</a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>')

WSP_REF = ('<wps:wsp><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>'
           '<a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom></wps:spPr>'
           '<wps:style><a:lnRef idx="2"><a:schemeClr val="accent1"/></a:lnRef>'
           '<a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef></wps:style>'
           '<wps:bodyPr/></wps:wsp>')

WSP_SOLID = ('<wps:wsp><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>'
             '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>'
             '<a:solidFill><a:srgbClr val="00FF00"/></a:solidFill></wps:spPr>'
             '<wps:style><a:fillRef idx="1"><a:schemeClr val="accent2"/></a:fillRef></wps:style>'
             '<wps:bodyPr/></wps:wsp>')

DOC = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
       f'<w:document {NS}><w:body><w:p>{anchor(0, WSP_REF)}{anchor(1, WSP_SOLID)}</w:p></w:body></w:document>')

THEME = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="K24">
<a:themeElements><a:clrScheme name="K24">
<a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
<a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
<a:dk2><a:srgbClr val="44546A"/></a:dk2>
<a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>
<a:accent1><a:srgbClr val="FF0000"/></a:accent1>
<a:accent2><a:srgbClr val="0000FF"/></a:accent2>
<a:accent3><a:srgbClr val="00B050"/></a:accent3>
<a:accent4><a:srgbClr val="7030A0"/></a:accent4>
<a:accent5><a:srgbClr val="FFFF00"/></a:accent5>
<a:accent6><a:srgbClr val="FF00FF"/></a:accent6>
<a:hlink><a:srgbClr val="0563C1"/></a:hlink>
<a:folHlink><a:srgbClr val="954F72"/></a:folHlink>
</a:clrScheme>
<a:fontScheme name="K24"><a:majorFont><a:latin typeface="Calibri Light"/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/></a:minorFont></a:fontScheme>
<a:fmtScheme name="K24"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst>
<a:lnStyleLst><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="25400"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="38100"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst>
<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>
<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst>
</a:fmtScheme></a:themeElements></a:theme>"""

out = "<repo>/.worktrees/t_02d9e0d8/core/engine-api/tests/fixtures/k24_minimal_style_fill.docx"
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("[Content_Types].xml", CT)
    z.writestr("_rels/.rels", RELS)
    z.writestr("word/_rels/document.xml.rels", DOCREL)
    z.writestr("word/document.xml", DOC)
    z.writestr("word/theme/theme1.xml", THEME)
print("wrote", out)
