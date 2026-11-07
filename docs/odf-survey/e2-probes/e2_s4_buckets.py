"""S4 精確四分桶閉合：頁首 / 圖表 part 字面 / MathML part / LO 合成軸標籤。"""
import json, pathlib, re, zipfile
from collections import Counter
import xml.etree.ElementTree as ET
import pymupdf

SCRATCH = pathlib.Path("/home/jerry/.hermes/profiles/ow-qa/cache/scratch/e2_odt")
REPO = pathlib.Path("/home/jerry/projects/quire/.worktrees/t_72dc8bc8")
MATH = "{http://www.w3.org/1998/Math/MathML}"
WS = re.compile(r"\s+")
rel = "corpus-odt/lo_out/lo__ooxmlexport__strict.odt"
with zipfile.ZipFile(REPO / rel) as z:
    names = z.namelist()
    cx = z.read("content.xml").decode("utf-8", "replace")
    sx = z.read("styles.xml").decode("utf-8", "replace")
    parts = {n: z.read(n).decode("utf-8", "replace") for n in names
             if re.match(r"Object[^/]*/content\.xml$", n)}

hf = "".join(WS.sub("", re.sub(r"<[^>]+>", "", m.group(2))) for m in re.finditer(
    r"<style:(header|footer)(?:-first|-left)?[^>]*>(.*?)</style:\1(?:-first|-left)?>", sx, re.S))

chart, math = "", ""
for n, o in parts.items():
    for m in re.finditer(r"<text:(?:p|h)[^>]*>(.*?)</text:(?:p|h)>", o, re.S):
        chart += WS.sub("", re.sub(r"<[^>]+>", "", m.group(1)))
    try:
        root = ET.fromstring(o)
    except Exception:
        continue
    for e in root.iter():
        if e.tag.startswith(MATH) and e.tag.split("}")[-1] in ("mi", "mn", "mo") and e.text:
            math += e.text

# 我方可見投影（去 hidden 段；本檔無 hidden）
import subprocess
ODTFLOW = str(SCRATCH / "probe_target/release/odtflow")
rows = json.loads(subprocess.run([ODTFLOW, str(REPO / rel)], capture_output=True, text=True).stdout)["paragraphs"]
vis = WS.sub("", "".join(p["text"] for p in rows))

pdf = sorted((SCRATCH / "lo_pdf" / pathlib.Path(rel).name).glob("*.pdf"))[0]
d = pymupdf.open(pdf)
lo = WS.sub("", "".join(l for pg in d for l in pg.get_text().splitlines()))
d.close()
print("gap =", len(vis) - len(lo), " ours =", len(vis), " lo =", len(lo))
lo_only = Counter(lo) - Counter(vis)
ow_only = Counter(vis) - Counter(lo)
r_hf = Counter(hf) & lo_only
r_ch = Counter(chart) & (lo_only - r_hf)
r_ma = Counter(math) & (lo_only - r_hf - r_ch)
rest = lo_only - r_hf - r_ch - r_ma
print("hf_src", len(hf), "emit", sum(r_hf.values()))
print("chart_src", repr(chart), "emit", sum(r_ch.values()))
print("math_src", repr(math), "emit", sum(r_ma.values()))
print("rest(synth)", sum(rest.values()), "".join(f"{c}×{n}" if n > 1 else c for c, n in sorted(rest.items())))
print("ow_only", sum(ow_only.values()), dict(ow_only))
print("closure:", sum(r_hf.values()) + sum(r_ch.values()) + sum(r_ma.values()) + sum(rest.values()),
      "== gap", len(lo) - len(vis))
