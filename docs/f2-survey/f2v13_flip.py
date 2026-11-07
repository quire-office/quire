#!/usr/bin/env python3
"""F2 探針：connector／各 prst 的 xfrm flip 屬性＋LO 真值端點對照（實作前事實採集）。"""
import re
import zipfile

FILES = [
    "corpus/poi/test-data_document_shapes-with-text.docx",
    "corpus/ooxmlexport/tdf113183.docx",
    "corpus/ooxmlexport/tdf113258.docx",
    "corpus/ooxmlexport/fdo74605.docx",
]
EMU = 12700.0

def main():
    for f in FILES:
        z = zipfile.ZipFile(f)
        xml = z.read("word/document.xml").decode("utf8", "replace")
        print("=" * 20, f)
        for m in re.finditer(r'<wps:(wsp|cxnSp)\b', xml):
            seg = xml[m.start():m.start() + 2600]
            g = re.search(r'<a:prstGeom prst="([^"]+)"', seg)
            if not g:
                continue
            x = re.search(r'<a:xfrm([^>]*)>', seg)
            off = re.search(r'<a:off x="(-?\d+)" y="(-?\d+)"/>', seg)
            ext = re.search(r'<a:ext cx="(\d+)" cy="(\d+)"/>', seg)
            ph = re.search(r'<wp:posOffset>(-?\d+)</wp:posOffset>', xml[max(0, m.start() - 1600):m.start()])
            attrs = x.group(1) if x else ""
            o = [int(v) / EMU for v in off.groups()] if off else None
            e = [int(v) / EMU for v in ext.groups()] if ext else None
            print(g.group(1), "|", attrs.strip(), "| off_pt", o, "| ext_pt", e, "| posOff", (int(ph.group(1)) / EMU) if ph else None)

if __name__ == "__main__":
    main()
