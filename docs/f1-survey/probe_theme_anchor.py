"""查 theme1.xml accent1 與 document.xml txbxContent 段落 pPr 宣告（錨點律依據）。"""

import pathlib
import re
import sys
import zipfile

p = pathlib.Path(sys.argv[1])
z = zipfile.ZipFile(p)
theme = z.read('word/theme/theme1.xml').decode('utf-8', 'replace')
m = re.search(r'<a:clrScheme.*?</a:clrScheme>', theme, re.S)
if m:
    for slot in ('dk1', 'lt1', 'dk2', 'lt2', 'accent1', 'accent2', 'accent3'):
        sm = re.search(r'<a:%s>(.*?)</a:%s>' % (slot, slot), m.group(0), re.S)
        if sm:
            v = re.search(r'val="([0-9A-Fa-f]{6})"|lastClr="([0-9A-Fa-f]{6})"', sm.group(1))
            print(slot, v.group(1) or v.group(2) if v else '?')
doc = z.read('word/document.xml').decode('utf-8', 'replace')
i = doc.find('<w:txbxContent>')
seg = doc[i:i + 2600]
print('--- txbxContent 段 ---')
print(seg[:2500])
