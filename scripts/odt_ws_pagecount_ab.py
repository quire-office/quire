#!/usr/bin/env python3
"""ODF 正文格式化空白剝除（卡 t_ae9a9f22）：corpus-odt 全檔逐檔頁數 A/B。

用途：比對「正規化前（base binary）」與「正規化後（head binary）」的逐檔頁數，
逐檔列出 base/head 頁數差。正氣門：正規化後頁數**普遍下降或持平**（幽靈行消失
＝每頁容量回升）；任何上升都要單獨解釋。

兩支 binary 皆由本卡 worktree 編出：base 取本卡起點 commit（merge-base）乾淨
worktree，head 取修復後工作區（零回退鐵閘紀律，見 skill quire-redlight-conventions）。

口徑：`quire-cli odt-dump <file>`（全部頁 pretty 陣列）取陣列長度＝頁數；加密／
開檔失敗檔兩側同錯＝記 skipped，不算回退。平行 ThreadPoolExecutor(12)。

用法：
  scripts/odt_ws_pagecount_ab.py <base-binary> <head-binary> [--out out.json]
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


def dump_pages(binary, rel):
    """回傳 (頁數, 全頁文字項數) 或 (None, 錯誤訊息)。"""
    try:
        p = subprocess.run([binary, "odt-dump", os.path.join(ROOT, rel)],
                           capture_output=True, timeout=300)
    except subprocess.TimeoutExpired:
        return None, "timeout"
    if p.returncode != 0:
        return None, (p.stderr.decode("utf-8", "replace").strip() or "rc=%d" % p.returncode)
    try:
        data = json.loads(p.stdout.decode("utf-8", "replace"))
    except Exception as e:
        return None, "json: %s" % e
    if not isinstance(data, list):
        return None, "shape"
    items = sum(len(pg.get("items", [])) for pg in data)
    return len(data), items


def one(rel, base, head):
    bp, bi = dump_pages(base, rel)
    hp, hi = dump_pages(head, rel)
    return {"file": rel, "base_pages": bp, "head_pages": hp,
            "base_items": bi, "head_items": hi,
            "delta": (None if (bp is None or hp is None) else hp - bp)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("base")
    ap.add_argument("head")
    ap.add_argument("--out", default="")
    a = ap.parse_args()
    fl = files()
    with ThreadPoolExecutor(12) as ex:
        rows = list(ex.map(lambda r: one(r, a.base, a.head), fl))
    skipped = [r for r in rows if r["base_pages"] is None and r["head_pages"] is None]
    only_one = [r for r in rows if (r["base_pages"] is None) != (r["head_pages"] is None)]
    down = [r for r in rows if r["delta"] is not None and r["delta"] < 0]
    up = [r for r in rows if r["delta"] is not None and r["delta"] > 0]
    same = [r for r in rows if r["delta"] == 0]
    print("files=%d compared=%d down=%d same=%d UP=%d skipped_both_same_error=%d asymmetric=%d"
          % (len(rows), sum(1 for r in rows if r["delta"] is not None),
             len(down), len(same), len(up), len(skipped), len(only_one)))
    print("%-70s %6s %6s %6s" % ("file", "base", "head", "delta"))
    for r in rows:
        mark = "  <-- UP" if (r["delta"] or 0) > 0 else ("  down" if (r["delta"] or 0) < 0 else "")
        print("%-70s %6s %6s %6s%s" % (r["file"], r["base_pages"], r["head_pages"],
                                        r["delta"], mark))
    for r in skipped:
        print("SKIPPED(both same error): %s :: %s" % (r["file"], r["base_items"]))
    for r in only_one:
        print("ASYMMETRIC: %s base=%s/%s head=%s/%s" % (r["file"], r["base_pages"],
              r["base_items"], r["head_pages"], r["head_items"]))
    if a.out:
        with open(a.out, "w", encoding="utf-8") as fh:
            json.dump(rows, fh, ensure_ascii=False, indent=1)
    return 0 if not up and not only_one else 1


if __name__ == "__main__":
    sys.exit(main())
