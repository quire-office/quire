# F2 接手：我方顯示列表實抓——非矩形形状是否退化為外接 Rect
import json
import subprocess
import sys

CLI = "/home/jerry/projects/quire/.worktrees/t_ab254070/target/debug/quire-cli"
ROOT = __import__("os").path.dirname(__file__).rsplit("/docs",1)[0] + "/"

for rel in sys.argv[1:]:
    out = subprocess.run([CLI, "dump", ROOT + rel], capture_output=True, text=True).stdout
    d = json.loads(out)
    pages = d if isinstance(d, list) else d.get("pages", [])
    print("==", rel)
    for pg in pages:
        items = pg.get("items", [])
        kinds = {}
        for it in items:
            k = list(it.keys())[0]
            kinds[k] = kinds.get(k, 0) + 1
        print("  page", pg.get("page"), kinds)
        for it in items:
            k = list(it.keys())[0]
            if k in ("Rect", "Rule"):
                v = it[k]
                keep = {kk: (round(vv, 1) if isinstance(vv, float) else vv)
                        for kk, vv in v.items()
                        if kk in ("x", "y", "w", "h", "fill_argb",
                                  "x1", "y1", "x2", "y2", "color_argb")}
                print("    ", k, keep)
            elif k == "Text":
                v = it[k]
                print("     Text", round(v.get("x", 0), 1), round(v.get("y", 0), 1),
                      repr(v.get("text", ""))[:40])
