"""fdo76591：找 imagedata 所在 part 與周邊 v:textbox / extent（D9b 口徑根植）。"""
import zipfile
import re

p = 'corpus/ooxmlexport/fdo76591.docx'
z = zipfile.ZipFile(p)
for n in z.namelist():
    if not n.endswith('.xml'):
        continue
    data = z.read(n).decode('utf-8', 'ignore')
    if 'imagedata' not in data and 'blip' not in data:
        continue
    print(f"== part {n}: imagedata={'imagedata' in data} blip={'blip' in data}")
    for m in re.finditer(r'<v:textbox([^>]*)>', data):
        print(f"   textbox attrs: {m.group(1)}")
    for m in re.finditer(r'<v:shape[^>]*>', data):
        s = m.group(0)
        sid = re.search(r'(?<!o:)id="([^"]*)"', s)
        style = re.search(r'style="([^"]*)"', s)
        print(f"   v:shape id={sid.group(1) if sid else '?'} style={style.group(1) if style else ''}")
    for m in re.finditer(r'<v:group[^>]*>', data):
        print(f"   v:group {m.group(0)[:200]}")
    for m in re.finditer(r'<wp:extent cx="(\d+)" cy="(\d+)"/>', data):
        print(f"   extent {m.group(1)}x{m.group(2)} = {int(m.group(1))/12700:.2f}x{int(m.group(2))/12700:.2f}pt")
