"""D9b 候選檔的宿主框精確結構 dump：框 style、textbox/@inset、txbxContent 內圖片路徑。"""
import zipfile
import sys
import xml.etree.ElementTree as ET

def local(tag):
    return tag.split('}')[-1]

TARGETS = [
    ('corpus/ooxmlexport/fdo76591.docx', 'word/document.xml', '_x0000_s1036'),
    ('corpus/ooxmlexport/n792778.docx', 'word/document.xml', '_x0000_s1125'),
    ('corpus/ooxmlexport/tdf102466.docx', 'word/header1.xml', None),
    ('corpus/ooxmlimport/tdf48658_transparentOLEheader.docx', 'word/header2.xml', '_x0000_s1025'),
]

def path_of(root, el):
    parents = {c: p for p in root.iter() for c in p}
    chain = []
    cur = el
    while cur is not None:
        chain.append(local(cur.tag))
        cur = parents.get(cur)
    return ' > '.join(reversed(chain))

for p, part, want in TARGETS:
    z = zipfile.ZipFile(p)
    root = ET.fromstring(z.read(part))
    print(f"######## {p} :: {part} ########")
    for sh in root.iter():
        if local(sh.tag) not in ('shape', 'group'):
            continue
        if want and sh.get('id') != want:
            continue
        print(f"--- {local(sh.tag)} id={sh.get('id')!r} style={(sh.get('style') or '')[:130]!r}")
        for d in sh.iter():
            if local(d.tag) in ('imagedata', 'blip'):
                print(f"    PIC {local(d.tag)} at: {path_of(root, d)}")
                print(f"        attrs: { {local(k): v for k, v in d.attrib.items()} }")
            if local(d.tag) == 'textbox':
                print(f"    TEXTBOX inset={d.get('inset')!r} style={d.get('style')!r}")
            if local(d.tag) == 'extent':
                cx = d.get('cx') or d.get('ext cx')
        # anchor?
        for d in sh.iter():
            if local(d.tag) == 'anchor':
                print(f"    wp:anchor found")
                break
    print()
