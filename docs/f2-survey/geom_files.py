#!/usr/bin/env python3
# F2：找 corpus 內含非 rect prstGeom 的檔案清單（供零發射證據取樣）
import os, re, zipfile, collections, json, subprocess

ROOT = "<repo>/corpus"
GEOM = re.compile(r"<a:prstGeom[^>]*\bprst=\"([^\"]+)\"")
BODY = re.compile(r"^word/(document|header\d*|footer\d*|footnotes|endnotes|comments)\.xml$")
NONRECT = collections.defaultdict(list)
for dirpath, _d, files in os.walk(ROOT):
    for fn in sorted(files):
        if not fn.lower().endswith((".docx", ".docm", ".dotx")):
            continue
        p = os.path.join(dirpath, fn)
        rel = os.path.relpath(p, ROOT)
        try:
            with zipfile.ZipFile(p) as z:
                for nm in z.namelist():
                    if not nm.endswith(".xml") or not BODY.match(nm):
                        continue
                    for g in GEOM.findall(z.read(nm).decode("utf-8", "replace")):
                        if g != "rect":
                            NONRECT[g].append(rel)
        except Exception:
            pass
for g in sorted(NONRECT, key=lambda k: -len(set(NONRECT[k]))):
    uniq = sorted(set(NONRECT[g]))
    print(f"{g:22s} {len(uniq):3d} 檔: {uniq[:4]}")
