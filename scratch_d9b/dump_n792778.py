"""n792778：VML 群組結構＋坐標系事實（判定 5 個 inset=0 框是否走 PageFloatingImage）。"""
import zipfile
import xml.etree.ElementTree as ET

def local(tag):
    return tag.split('}')[-1]

z = zipfile.ZipFile('corpus/ooxmlexport/n792778.docx')
raw = z.read('word/document.xml').decode('utf-8')
root = ET.fromstring(raw)
for sh in root.iter():
    if local(sh.tag) not in ('shape', 'group'):
        continue
    sid = sh.get('id')
    if sid not in ('_x0000_s1125', '_x0000_s1126', '_x0000_s1127',
                   '_x0000_s1129', '_x0000_s1130'):
        continue
    print(f"--- {local(sh.tag)} id={sid}")
    print(f"    style={sh.get('style')!r}")
    for k in ('coordorigin', 'coordsize', 'filled', 'stroked', 'strokeweight', 'o:gfxdata'):
        for kk, vv in sh.attrib.items():
            if local(kk) == k:
                print(f"    {k}={vv!r}")
    # 子層 imagedata / blip
    for d in sh.iter():
        if local(d.tag) == 'imagedata':
            print(f"    imagedata { {local(k): (v[:40] if isinstance(v,str) else v) for k,v in d.attrib.items()} }")
        elif local(d.tag) == 'blip':
            print(f"    blip { {local(k): v for k,v in d.attrib.items()} }")
        elif local(d.tag) == 'extent':
            print(f"    wp:extent { {local(k): v for k,v in d.attrib.items()} }")
        elif local(d.tag) == 'textbox':
            print(f"    textbox inset={d.get('inset')!r} style={d.get('style')!r}")
