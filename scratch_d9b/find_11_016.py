"""D9b 第 3 條根植：'11-016'/'3-23-2011' 在 fdo76591 哪個 part、哪個框、哪個段落。
   另：該段落的 pPr/jc/縮排事實（判斷能否資料化取代正文文字字面量）。
"""
import zipfile
import re
import xml.etree.ElementTree as ET

def local(tag):
    return tag.split('}')[-1]

z = zipfile.ZipFile('corpus/ooxmlexport/fdo76591.docx')
for n in z.namelist():
    if not n.endswith('.xml'):
        continue
    raw = z.read(n).decode('utf-8', 'ignore')
    for needle in ('11-016', '3-23-2011'):
        for m in re.finditer(re.escape(needle), raw):
            i = m.start()
            ps = raw.rfind('<w:p ', 0, i)
            if ps < 0:
                ps = raw.rfind('<w:p>', 0, i)
            pe = raw.find('</w:p>', i)
            frag = raw[ps:pe + 6]
            hs = [x for x in re.finditer(r'<v:shape[^>]*id="([^"]+)"', raw[:i])]
            gids = [x for x in re.finditer(r'<v:group[^>]*id="([^"]+)"', raw[:i])]
            txc_open = len(re.findall(r'<w:txbxContent', raw[:i]))
            txc_close = len(re.findall(r'</w:txbxContent>', raw[:i]))
            in_tb = raw[:i].rfind('<w:txbxContent') > raw[:i].rfind('</w:txbxContent>')
            hf = raw[:i].rfind('<w:hdr') > raw[:i].rfind('</w:hdr')
            ff = raw[:i].rfind('<w:ftr') > raw[:i].rfind('</w:ftr')
            print(f"### {n} :: {needle!r} (offset {i})")
            print(f"    in_txbxContent={in_tb}  in_hdr={hf} in_ftr={ff}")
            print(f"    最近 v:shape={hs[-1].group(1) if hs else None} 最近 v:group={gids[-1].group(1) if gids else None}")
            print(f"    段落 XML（{len(frag)}B）: {frag[:600]}")
            print()
