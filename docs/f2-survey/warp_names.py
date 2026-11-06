#!/usr/bin/env python3
# F2：全 corpus prstTxWarp/@prst 名稱分佈＋效果節點×檔案類型（warp 是否真變形）
import os, re, zipfile, collections, json

ROOT = "/home/jerry/projects/quire/corpus"
WARP = re.compile(r"<a:prstTxWarp[^>]*\bprst=\"([^\"]+)\"")
GEOM = re.compile(r"<a:prstGeom[^>]*\bprst=\"([^\"]+)\"")
BODY = re.compile(r"^word/(document|header\d*|footer\d*|footnotes|endnotes|comments)\.xml$")

warp_c = collections.Counter()
geom_c = collections.Counter()
warp_files = collections.defaultdict(set)
geom_files = collections.defaultdict(set)
eff_c = collections.Counter()
warp_nontriv_files = set()
n = 0
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
                    x = z.read(nm).decode("utf-8", "replace")
                    n += 1
                    for w in WARP.findall(x):
                        warp_c[w] += 1
                        warp_files[w].add(rel)
                        if w != "textNoShape":
                            warp_nontriv_files.add(rel)
                    for g in GEOM.findall(x):
                        geom_c[g] += 1
                        geom_files[g].add(rel)
                    for e in ("glow", "softEdge", "reflection", "bevelT", "bevelB",
                              "outerShdw", "innerShdw", "blur", "fillOverlay", "highlight"):
                        k = len(re.findall(r"<a:" + e + r"[\s/>]", x))
                        if k:
                            eff_c[e] += k
        except Exception as ex:
            print("SKIP", rel, ex)

out = {"body_parts_scanned": n,
       "warp": {k: {"count": v, "files": len(warp_files[k])} for k, v in warp_c.most_common()},
       "geom_top": {k: {"count": v, "files": len(geom_files[k])} for k, v in geom_c.most_common(20)},
       "effect_body": dict(eff_c),
       "warp_nontriv_files": sorted(warp_nontriv_files)}
with open("/home/jerry/.hermes/profiles/ow-render/cache/scratch/f2_warp_names.json", "w") as f:
    json.dump(out, f, ensure_ascii=False, indent=1)
print("本體 part 掃描數", n)
print("\n== prstTxWarp/@prst 分佈（名稱: 次數/檔案數）==")
for k, v in out["warp"].items():
    print(f"  {k:24s} {v['count']:4d}  {v['files']:3d} 檔")
print("\n== prstGeom/@prst Top20 ==")
for k, v in out["geom_top"].items():
    print(f"  {k:24s} {v['count']:4d}  {v['files']:3d} 檔")
print("\n== 本體效果節點 ==", out["effect_body"])
print("\n== 有『非 textNoShape』warp 的檔案 ==", len(warp_nontriv_files))
for x in sorted(warp_nontriv_files):
    print("   ", x)
