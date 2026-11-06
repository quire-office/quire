#!/usr/bin/env python3
# F2：非 rect prstGeom 檔案的我方發射實況（是否退化為 rect／是否零發射）
import subprocess, json, sys, os, re, zipfile

WT = "/home/jerry/projects/quire/.worktrees/t_ab254070"
CLI = os.path.join(WT, "target/debug/quire-cli")
FILES = ["ooxmlexport/tdf113258.docx",              # ellipse
         "ooxmlimport/groupshape-fontname.docx",     # line
         "ooxmlexport/fdo74605.docx",                # roundRect + arrows + connector
         "ooxmlexport/tdf113183.docx",               # star5 + triangle
         "ooxmlimport/tdf120551.docx",               # roundRect + gradFill
         "ooxmlexport/TextEffects_Groupshapes.docx", # gradFill + warp
         "ooxmlexport/fdo80897.docx"]                # 唯一非 textNoShape warp（textTriangle）
GEOM = re.compile(r'<a:prstGeom[^>]*\bprst="([^"]+)"')
for rel in FILES:
    p = "/home/jerry/projects/quire/corpus/" + rel
    with zipfile.ZipFile(p) as z:
        geoms = []
        for nm in z.namelist():
            if nm.endswith(".xml") and nm.startswith("word/"):
                geoms += GEOM.findall(z.read(nm).decode("utf-8", "replace"))
    r = subprocess.run([CLI, "dump", p], capture_output=True, text=True, timeout=180)
    kinds = {}
    n_items = 0
    if r.returncode == 0:
        for pg in json.loads(r.stdout):
            for it in pg["items"]:
                k = list(it.keys())[0]
                kinds[k] = kinds.get(k, 0) + 1
                n_items += 1
    else:
        print("DUMP FAIL", rel, r.stderr[:200])
    print(f"{rel:46s} geom={sorted(set(geoms))}  我方 items={n_items} {kinds}")
