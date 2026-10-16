import zipfile
import xml.dom.minidom
import pathlib
import sys

bad = 0
pat = sys.argv[1] if len(sys.argv) > 1 else "pool/selfgen__*.docx"
for p in sorted(pathlib.Path("pool").glob(pat.split("/")[-1])):
    try:
        zf = zipfile.ZipFile(p)
        for n in zf.namelist():
            if n.endswith((".xml", ".rels")):
                xml.dom.minidom.parseString(zf.read(n))
        print("OK ", p.name)
    except Exception as e:
        bad += 1
        print("BAD", p.name, e)
print("bad:", bad)
