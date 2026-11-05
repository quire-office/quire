"""fdo76591 _x0000_s1036（CHRYSLER logo 宿主框）與 _x0000_s1026（滿版外框）原始 XML 節點轉儲。"""
import zipfile
import xml.etree.ElementTree as ET

def local(tag):
    return tag.split('}')[-1]

z = zipfile.ZipFile('corpus/ooxmlexport/fdo76591.docx')
root = ET.fromstring(z.read('word/document.xml'))

def dump(el, depth=0, maxd=12):
    tag = el.tag.split('}')[-1]
    attrs = ' '.join(f"{k.split('}')[-1]}={v!r}" for k, v in el.attrib.items())
    txt = (el.text or '').strip()
    print('  ' * depth + f"<{tag}{' ' + attrs if attrs else ''}{'>' + txt if txt else ''}")
    if depth < maxd:
        for c in el:
            dump(c, depth + 1, maxd)

for sh in root.iter():
    if local(sh.tag) != 'shape':
        continue
    sid = sh.get('id')
    if sid not in ('_x0000_s1036', '_x0000_s1026', '_x0000_s1039'):
        continue
    print(f"######## {sid} ########")
    dump(sh, maxd=3)
    print()
