#!/usr/bin/env python3
"""F2 探針：考卷四檔的 header/footer part 是否也有 prstGeom（決定事實池範圍）。"""
import re
import zipfile

FILES = [
    "corpus/ooxmlexport/tdf113183.docx",
    "corpus/ooxmlexport/tdf113258.docx",
    "corpus/ooxmlexport/fdo80897.docx",
    "corpus/poi/test-data_document_shapes-with-text.docx",
]

def main():
    for f in FILES:
        z = zipfile.ZipFile(f)
        print("=" * 16, f)
        for name in z.namelist():
            if not name.endswith(".xml"):
                continue
            data = z.read(name).decode("utf8", "replace")
            hits = re.findall(r'<a:prst(?:Geom|TxWarp) prst="([^"]+)"', data)
            if hits and "document.xml" not in name:
                print("  NON-BODY part", name, hits)
        doc = z.read("word/document.xml").decode("utf8", "replace")
        g = re.findall(r'<a:prstGeom prst="([^"]+)"', doc)
        w = re.findall(r'<a:prstTxWarp prst="([^"]+)"', doc)
        print("  body geom:", g)
        print("  body warp:", w)

if __name__ == "__main__":
    main()
