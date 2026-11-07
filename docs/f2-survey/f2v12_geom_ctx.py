#!/usr/bin/env python3
"""F2 幾何上下文探針：prstGeom／prstTxWarp 的掛點祖先鏈（決定解析層在哪收）。"""
import re
import sys
import zipfile

FILES = [
    "corpus/ooxmlexport/tdf113183.docx",
    "corpus/ooxmlexport/tdf113258.docx",
    "corpus/poi/test-data_document_shapes-with-text.docx",
    "corpus/ooxmlexport/fdo80897.docx",
    "corpus/ooxmlexport/fdo74605.docx",
]

def main():
    for f in FILES:
        z = zipfile.ZipFile(f)
        xml = z.read("word/document.xml").decode("utf8", errors="replace")
        print("=" * 24, f, len(xml))
        for m in re.finditer(r'<a:prstGeom prst="([^"]+)"', xml):
            s = max(0, m.start() - 400)
            tags = re.findall(r'<([a-zA-Z0-9:]+)[ >/]', xml[s:m.start()])
            print("geom", m.group(1), "tail:", tags[-6:])
        for m in re.finditer(r'<a:prstTxWarp prst="([^"]+)"', xml):
            s = max(0, m.start() - 300)
            tags = re.findall(r'<([a-zA-Z0-9:]+)[ >/]', xml[s:m.start()])
            print("warp", m.group(1), "tail:", tags[-5:])

if __name__ == "__main__":
    main()
