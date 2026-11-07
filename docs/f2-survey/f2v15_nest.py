#!/usr/bin/env python3
"""F2 探針：wsp/cxnSp 與宿主框的巢狀關係（決定 prst 該掛到哪一層候補）。"""
import re
import zipfile

FILES = [
    "corpus/poi/test-data_document_shapes-with-text.docx",
    "corpus/ooxmlexport/tdf113183.docx",
    "corpus/ooxmlexport/tdf113258.docx",
    "corpus/ooxmlexport/fdo80897.docx",
]

TAGS = re.compile(
    r"<(wp:anchor|wp:inline|wpg:wgp|wpg:group|wps:wsp|wps:cxnSp|wps:grpSp)"
    r"?\b|<(a:prstGeom [^>]*)>|</(wps:wsp|wps:cxnSp|wpg:wgp|wpg:group|wp:inline|wp:anchor)>"
)

def main():
    for f in FILES:
        z = zipfile.ZipFile(f)
        xml = z.read("word/document.xml").decode("utf8", "replace")
        print("=" * 20, f)
        depth = 0
        for m in TAGS.finditer(xml):
            open_t = m.group(1)
            geom = m.group(2)
            close_t = m.group(3)
            if geom:
                print("  " * depth, "GEOM", geom)
            elif open_t:
                print("  " * depth, "OPEN", open_t)
                depth += 1
            elif close_t:
                depth = max(0, depth - 1)
                print("  " * depth, "CLOSE", close_t)

if __name__ == "__main__":
    main()
