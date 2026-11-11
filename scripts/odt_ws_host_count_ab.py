#!/usr/bin/env python3
"""t_ae9a9f22：E2d 燈6 宿主計數（total_text_len）逐檔 A/B。

用途：本卡正規化合法扣掉 XML 格式化空白碼點 ⇒ `odf_e2d_header_footer` 燈 6 的
全庫常量基準位移移。逐檔列出 base/head `total_text_len` 差值（只准降不准升），
交 PM 裁基準更新；本卡不改既有考卷斷言數值。

用法：scripts/odt_ws_host_count_ab.py <base-binary> <head-binary>
口徑：`quire-cli odt-render <file>` 固定 JSON 的 paragraph_count／total_text_len
（與燈 6 同口徑：`project(&bundle)` 後求和；48 明文檔＝可投影檔）。
"""
import argparse, json, os, subprocess, sys
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CORPUS = os.path.join(ROOT, "corpus-odt")


def files():
    out = []
    for dirpath, _, names in os.walk(CORPUS):
        for n in sorted(names):
            if n.endswith(".odt"):
                out.append(os.path.relpath(os.path.join(dirpath, n), ROOT))
    return sorted(out)


def render(binary, rel):
    try:
        p = subprocess.run([binary, "odt-render", os.path.join(ROOT, rel)],
                           capture_output=True, timeout=300)
    except subprocess.TimeoutExpired:
        return None
    if p.returncode != 0:
        return None
    try:
        d = json.loads(p.stdout.decode("utf-8", "replace"))
    except Exception:
        return None
    return (d.get("paragraph_count"), d.get("total_text_len"))


def one(rel, base, head):
    b = render(base, rel)
    h = render(head, rel)
    if b is None and h is None:
        return {"file": rel, "skip": "both_unprojectable"}
    return {"file": rel, "base": b, "head": h,
            "delta": (None if (b is None or h is None) else h[1] - b[1])}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("base")
    ap.add_argument("head")
    ap.add_argument("--out", default="")
    a = ap.parse_args()
    with ThreadPoolExecutor(12) as ex:
        rows = list(ex.map(lambda r: one(r, a.base, a.head), files()))
    comp = [r for r in rows if r.get("delta") is not None]
    up = [r for r in comp if r["delta"] > 0]
    down = [r for r in comp if r["delta"] < 0]
    pb = sum(r["base"][0] for r in comp)
    ph = sum(r["head"][0] for r in comp)
    cb = sum(r["base"][1] for r in comp)
    ch = sum(r["head"][1] for r in comp)
    print("files_projectable=%d  paragraph_count base=%d head=%d (delta=%d)"
          % (len(comp), pb, ph, ph - pb))
    print("total_text_len      base=%d head=%d (delta=%d)  changed=%d up=%d down=%d"
          % (cb, ch, ch - cb, len(down) + len(up), len(up), len(down)))
    for r in down + up:
        print("  %-74s paras %5d->%-5d chars %8d->%-8d (%+d)"
              % (r["file"], r["base"][0], r["head"][0],
                 r["base"][1], r["head"][1], r["delta"]))
    for r in rows:
        if r.get("skip"):
            print("  SKIP %s" % r["file"])
    if a.out:
        with open(a.out, "w", encoding="utf-8") as fh:
            json.dump(rows, fh, ensure_ascii=False, indent=1)
    return 0 if not up else 1


if __name__ == "__main__":
    sys.exit(main())
