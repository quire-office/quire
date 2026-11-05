"""fdo76591：base vs new 全量項目對照（D9b 改動的逐項實測）。"""
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

b, m = items(BASE), items(NEW)
print(f"base items={len(b)} new items={len(m)}")
for i in range(max(len(b), len(m))):
    ib = b[i] if i < len(b) else None
    im = m[i] if i < len(m) else None
    if ib == im:
        continue
    kb = list(ib)[0] if ib else None
    km = list(im)[0] if im else None
    vb = {a: (round(x, 3) if isinstance(x, float) else x) for a, x in (ib[kb].items() if ib else []) if a != 'data_b64'}
    vm = {a: (round(x, 3) if isinstance(x, float) else x) for a, x in (im[km].items() if im else []) if a != 'data_b64'}
    print(f"[{i}] {kb}")
    print(f"   base: {vb}")
    print(f"   new : {vm}")
