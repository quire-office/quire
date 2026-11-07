# 零回退風險盤點：具名閘門檔是否含非 rect 的 prstGeom（決定新發射會不會撞既有鎖）
import glob
import re
import zipfile
from collections import Counter

ROOT = __import__("os").path.dirname(__file__).rsplit("/docs",1)[0] + "/"
BODY = re.compile(r"word/(document|header\d*|footer\d*|footnotes|endnotes|comments)\.xml$")

# 卡面零回退鐵條點名的檔案
GATES = [
    "fdo76591", "WC048", "btlr", "99_Fields", "cell-grid-span", "CoverPage",
    "n751077", "artistic_effects", "fdo80897",
]
for g in GATES:
    hits = glob.glob(ROOT + "corpus/**/*" + g + "*", recursive=True)
    hits = [h for h in hits if h.endswith(".docx")]
    if not hits:
        print("MISSING", g)
        continue
    for h in hits:
        z = zipfile.ZipFile(h)
        txt = ""
        for n in z.namelist():
            if BODY.search(n):
                txt += z.read(n).decode("utf8", "ignore")
        geoms = Counter(re.findall(r"<a:prstGeom prst=\"([^\"]+)\"", txt))
        nonrect = {k: v for k, v in geoms.items() if k != "rect"}
        print(("%-40s" % h.replace(ROOT + "corpus/", "")), "geoms=", dict(geoms),
              "NONRECT=" , nonrect if nonrect else "無")
