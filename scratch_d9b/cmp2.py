"""fdo76591：base vs new 的全部 Image/Rule 項（全精度，逐項對照 LO 真值）。"""
import json
import os
import subprocess

D = os.path.dirname(os.path.abspath(__file__))
BASE = os.path.join(D, "base-bin", "quire-cli")
NEW = os.path.join(os.path.dirname(D), "target", "debug", "quire-cli")
DOC = "corpus/ooxmlexport/fdo76591.docx"

def items(cli):
    p = subprocess.run([cli, "page", DOC, "0"], capture_output=True, text=True, timeout=240)
    d = json.loads(p.stdout)
    return d["items"] if isinstance(d, dict) else d

for tag, cli in (("base", BASE), ("new", NEW)):
    print(f"--- {tag} ---")
    for i, it in enumerate(items(cli)):
        k = list(it)[0]
        if k not in ("Image", "Rule"):
            continue
        v = {a: (round(x, 3) if isinstance(x, float) else x)
             for a, x in it[k].items() if a != "data_b64"}
        print(f"  [{i}] {k} {v}")
print()
print("LO pymupdf 真值（實測）：")
print("  IMG  xref=46 bbox=[201.05,732.80,411.60,767.35] w=210.55 h=34.55  ← CHRYSLER logo")
print("  IMG  xref=50 bbox=[264.00,594.00,348.00,687.00] w=84.00  h=93.00  ← 第二 logo 框")
print("  框底線 y=749.85（4pt 描邊中心線）")
