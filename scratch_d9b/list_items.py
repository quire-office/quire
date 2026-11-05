"""列出 fdo76591 頁 0 顯示列表的 Image/Rule 項（D9b 基線實測）。"""
import json
import subprocess
import sys

cli = sys.argv[1] if len(sys.argv) > 1 else "scratch_d9b/base-bin/quire-cli"
doc = sys.argv[2] if len(sys.argv) > 2 else "../../corpus/ooxmlexport/fdo76591.docx"
page = sys.argv[3] if len(sys.argv) > 3 else "0"

p = subprocess.run([cli, "page", doc, page], capture_output=True, text=True, timeout=120)
if p.returncode != 0:
    print("FAIL", p.stderr[:500])
    sys.exit(1)
d = json.loads(p.stdout)
items = d["items"] if isinstance(d, dict) else d
for i, it in enumerate(items):
    k = list(it)[0]
    if k in ("Image", "Rule", "Rect"):
        v = it[k]
        vv = {}
        for a, x in v.items():
            if isinstance(x, float):
                vv[a] = round(x, 2)
            elif isinstance(x, str) and len(x) > 24:
                vv[a] = x[:20] + "…"
            else:
                vv[a] = x
        print(i, k, vv)
