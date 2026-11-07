# F2 探針：tdf120551（rect 系之外 prst）為何基線 SVG 未變——掛點歸因
import json
import re
import zipfile

ROOT = __import__("os").path.dirname(__file__).rsplit("/docs", 1)[0] + "/"
d = json.load(open(ROOT + "docs/f2-survey/f2_baseline_shapes.json"))
rel = "corpus/ooxmlimport/tdf120551.docx"
v = d[rel]
print("src_geom", v["src_geom"], "pages", v["pages"])
x = zipfile.ZipFile(ROOT + rel).read("word/document.xml").decode("utf8", "ignore")
for m in re.finditer(r"<a:prstGeom prst=\"([^\"]+)\"", x):
    seg = x[: m.start()]
    print(
        m.group(1),
        "anchor內=", seg.rfind("<wp:anchor") > seg.rfind("</wp:anchor"),
        "pict內=", seg.rfind("<w:pict") > seg.rfind("</w:pict"),
        "wsp內=", seg.rfind("<wps:wsp") > max(seg.rfind("</wps:wsp"), 0),
        "cxnSp內=", seg.rfind("<wps:cxnSp") > max(seg.rfind("</wps:cxnSp"), 0),
        "w:object內=", seg.rfind("<w:object") > seg.rfind("</w:object"),
    )
