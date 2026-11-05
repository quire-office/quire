"""n792778 s1125：直接子層（非子孫）的 v:textbox/@inset 是什麼？
   （決定宿主群組的 txbx_inset 是 Some(0,0,0,0) 還是 None。）
"""
import zipfile
import xml.etree.ElementTree as ET

def local(t):
    return t.split('}')[-1]

z = zipfile.ZipFile('corpus/ooxmlexport/n792778.docx')
root = ET.fromstring(z.read('word/document.xml'))
for sh in root.iter():
    if local(sh.tag) != 'group' or sh.get('id') != '_x0000_s1125':
        continue
    print("s1125 直接子層：")
    for c in sh:
        print(f"  <{local(c.tag)}> attrs={ {local(k): (v[:30] if isinstance(v,str) else v) for k,v in c.attrib.items()} }")
        for cc in c:
            print(f"      <{local(cc.tag)}> attrs={ {local(k): (v[:24] if isinstance(v,str) else v) for k,v in cc.attrib.items()} }")
            for ccc in cc:
                print(f"          <{local(ccc.tag)}>")
