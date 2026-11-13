#!/usr/bin/env python3
# F2：單一口徑語料普查（同一片 part 集合同時算「a: 前綴」與「任意前綴」兩種計數）
import os, re, zipfile, collections, json

ROOT = "<repo>/corpus"
BODY = re.compile(r"^word/(document|header\d*|footer\d*|footnotes|endnotes|comments)\.xml$")
NAMES = ["prstTxWarp", "wordArt", "prstGeom", "custGeom", "solidFill", "gradFill", "pattFill",
         "blipFill", "noFill", "ln", "effectLst", "effectDag", "glow", "softEdge", "reflection",
         "bevelT", "bevelB", "blur", "innerShdw", "outerShdw", "fillOverlay", "sp3d", "scene3d",
         "imgEffect", "OLEObject"]

a_c = collections.Counter()      # 僅 a: 前綴
any_c = collections.Counter()    # 任意前綴（含無前綴）
files_a = collections.Counter()
nparts = nfiles = 0
for dirpath, _d, files in os.walk(ROOT):
    for fn in sorted(files):
        if not fn.lower().endswith((".docx", ".docm", ".dotx")):
            continue
        p = os.path.join(dirpath, fn)
        nfiles += 1
        try:
            with zipfile.ZipFile(p) as z:
                hit_this = False
                for nm in z.namelist():
                    if not nm.endswith(".xml") or not BODY.match(nm):
                        continue
                    x = z.read(nm).decode("utf-8", "replace")
                    nparts += 1
                    for t in NAMES:
                        na = len(re.findall(r"<a:" + t + r"[\s/>]", x))
                        nn = len(re.findall(r"<(?:[A-Za-z0-9_]+:)?" + t + r"[\s/>]", x))
                        a_c[t] += na
                        any_c[t] += nn
                        if na:
                            hit_this = True
                if hit_this:
                    files_a["any"] += 1
        except Exception:
            pass

out = {"docx_files": nfiles, "body_parts": nparts,
       "a_prefixed": dict(a_c), "any_prefixed": dict(any_c)}
print(json.dumps(out, ensure_ascii=False, indent=1))
with open("<hermes>/profiles/ow-render/cache/scratch/f2_census_coherent.json", "w") as f:
    json.dump(out, f, ensure_ascii=False, indent=1)
