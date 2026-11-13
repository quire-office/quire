#!/bin/sh
# F1 現況實抓：chart-size 我方發射清單逐項（定位口徑的判讀依據）。
set -e
cd <repo>/.worktrees/t_f3acd271
cargo run -q -p quire-cli -- dump corpus/ooxmlexport/chart-size.docx \
  | /usr/bin/python3 -c '
import sys, json
pages = json.load(sys.stdin)
for pg in pages:
    print("== page", pg["page"], pg["width_pt"], pg["height_pt"])
    for it in pg["items"]:
        k = list(it.keys())[0]
        v = it[k]
        if k == "Text":
            print("  Text  x=%.2f y=%.2f sz=%.2f c=%08x t=%r" % (v["x"], v["y"], v["size_pt"], v.get("color_argb", 0), v["text"]))
        elif k == "Rect":
            print("  Rect  x=%.2f y=%.2f w=%.2f h=%.2f fill=%08x" % (v["x"], v["y"], v["w"], v["h"], v.get("fill_argb", 0)))
        elif k == "Rule":
            print("  Rule  (%.2f,%.2f)-(%.2f,%.2f) w=%.2f" % (v["x1"], v["y1"], v["x2"], v["y2"], v.get("width_pt", 0)))
        else:
            print("  ", k, v if k != "Image" else {kk: (vv[:20] if isinstance(vv, str) else vv) for kk, vv in v.items()})
'
