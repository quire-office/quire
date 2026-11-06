"""A/B 逐檔顯示列表差分：base（改前）vs new（改後），找所有 DrawItem 層位的位移。
   用於 D9b 零回退自查：期望僅 fdo76591 logo（同值）、n792778／tdf102466（inset 資料化）
   變動，其餘檔案位元組級不動。
"""
import json
import os
import subprocess
import sys

BASE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "base-bin", "quire-cli")
NEW = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                   "target", "debug", "quire-cli")
CORPUS = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(
    os.path.abspath(__file__))))), "..", "corpus")
CORPUS = os.path.abspath(os.path.join(os.path.dirname(os.path.dirname(
    os.path.dirname(os.path.abspath(__file__)))), "corpus"))

def items_of(cli, rel, page):
    p = subprocess.run([cli, "page", rel, str(page)], capture_output=True, text=True, timeout=240)
    if p.returncode != 0:
        return None
    try:
        d = json.loads(p.stdout)
    except Exception:
        return None
    items = d["items"] if isinstance(d, dict) else d
    out = []
    for it in items:
        k = list(it)[0]
        v = it[k]
        v = {a: x for a, x in v.items() if a != "data_b64"}
        out.append((k, tuple(sorted((a, round(x, 4) if isinstance(x, float) else str(x))
                                    for a, x in v.items()))))
    return out

def n_pages(cli, rel):
    p = subprocess.run([cli, "info", rel], capture_output=True, text=True, timeout=240)
    if p.returncode != 0:
        return None
    for line in p.stdout.splitlines():
        if line.startswith("頁數:"):
            try:
                return int(line.split(":", 1)[1].strip())
            except ValueError:
                return None
    return None

def main():
    files = sys.argv[1:]
    if not files:
        manifest = json.load(open(os.path.join(CORPUS, "manifest.json")))
        files = [os.path.join(os.path.dirname(CORPUS), "corpus", f["file"])
                 for f in manifest["files"]]
    changed, scanned, pcount_diff = [], 0, []
    for rel in files:
        nb = n_pages(BASE, rel)
        nn = n_pages(NEW, rel)
        if nb is None or nn is None:
            continue
        if nb != nn:
            pcount_diff.append((rel, nb, nn))
            changed.append((rel, "PAGES", nb, nn))
            continue
        n = nb or 1
        diff = None
        for pg in range(n):
            b = items_of(BASE, rel, pg)
            m = items_of(NEW, rel, pg)
            if b is None or m is None:
                diff = ("PAGE_FAIL", pg)
                break
            if b != m:
                # 找出第一處差異
                idx = next((i for i in range(max(len(b), len(m)))
                            if i >= len(b) or i >= len(m) or b[i] != m[i]), None)
                diff = (pg, idx,
                        b[idx] if idx is not None and idx < len(b) else None,
                        m[idx] if idx is not None and idx < len(m) else None)
                break
        scanned += 1
        if diff:
            changed.append((rel, diff))
    print(f"scanned={scanned} files")
    print(f"files_with_diff={len(changed)}")
    for rel, d in changed[:40]:
        if d == "PAGES":
            print(f"  PAGES {rel}: base={d[1] if isinstance(d, tuple) else ''}")
        else:
            print(f"  DIFF {rel}: {d}")

if __name__ == "__main__":
    main()
