"""fdo76591 頁腳 logo 框的 v:textbox/@inset 實值與框幾何（D9b 口徑根植）。"""
import zipfile
import re

p = 'corpus/ooxmlexport/fdo76591.docx'
z = zipfile.ZipFile(p)
for n in z.namelist():
    if not (n.startswith('word/') and n.endswith('.xml')):
        continue
    data = z.read(n).decode('utf-8', 'ignore')
    if 'imagedata' not in data:
        continue
    for m in re.finditer(r'<v:shape\b.*?(?=<v:shape\b|$)', data, re.S):
        frag = m.group(0)
        if 'imagedata' not in frag:
            continue
        sid = re.search(r'(?<!o:)id="([^"]*)"', frag)
        style = re.search(r'style="([^"]*)"', frag)
        inset = re.findall(r'<v:textbox([^>]*)>', frag)
        ext = re.findall(r'<wp:extent cx="(\d+)" cy="(\d+)"/>', frag)
        print(f"== {n} shape id={sid.group(1) if sid else '?'}")
        print(f"   style={style.group(1) if style else ''}")
        print(f"   textbox tags: {inset}")
        print(f"   wp:extent: {ext}")
