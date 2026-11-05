"""fdo76591：找出 '11-016'/'3-23-2011' 所在段落的完整 XML 與其所屬框（D9b 第 3 條根植）。"""
import zipfile
import re
import xml.etree.ElementTree as ET

def local(tag):
    return tag.split('}')[-1]

z = zipfile.ZipFile('corpus/ooxmlexport/fdo76591.docx')
raw = z.read('word/document.xml').decode('utf-8')

for needle in ('11-016', '3-23-2011'):
    i = raw.find(needle)
    if i < 0:
        print(f"{needle}: NOT FOUND")
        continue
    # 往前找最近的 <w:p，往後找 </w:p>
    ps = raw.rfind('<w:p ', 0, i)
    pe = raw.find('</w:p>', i)
    frag = raw[ps:pe + 6]
    print(f"===== {needle} 所在 <w:p>（len={len(frag)}）=====")
    print(frag[:1200])
    # 所屬宿主：往前找最近的 v:shape id
    hs = [m for m in re.finditer(r'<v:shape[^>]*id="([^"]+)"', raw[:i])]
    print(f"    最近宿主 shape: {hs[-1].group(1) if hs else None}")
    # 是否位於 v:txbxContent
    txc = [m for m in re.finditer(r'<w:txbxContent', raw[:i])]
    close = [m for m in re.finditer(r'</w:txbxContent>', raw[:i])]
    print(f"    txbxContent 開/閉 計數（前文）: {len(txc)}/{len(close)}")
    print()
