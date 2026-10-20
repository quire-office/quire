#!/usr/bin/env python3
"""抓取 XLSX/XLSM corpus 來源到 pool（純 stdlib＋gh 列檔；風格鏡像 fetch_lo/fetch_extra）。

來源：
1. LibreOffice/core `sc/qa/`（MPL-2.0/GPL-3.0/LGPL-3.0，真實 bug 回報測試檔）
   —— 卡面寫 sc/qa/extras/ooxmlimport/data，實測該目錄不存在於 sc/ 下；
   實際 spreadsheet QA 檔位於 sc/qa/unit/data/xlsx/**、sc/qa/uitest/data/** 等，
   故對 sc/qa 整棵做 BFS tree 走查（頂层 recursive tree 被 GitHub 截斷）。
2. apache/poi `test-data/spreadsheet/`（Apache-2.0，Bugzilla 真實檔），
   只收 .xlsx/.xlsm（.xls=OLE2 非 OPC、.ods 非 OOXML，一律排除）。

用法：python3 fetch_xlsx.py [workdir]（預設 <本目錄>/xlsx-work）
workdir 內：pool/（快取）、lo_xlsx.tsv、poi_xlsx.tsv（清單快取，--refresh 重列）。
冪等：pool 有快取；清單有快取時不重複打 API。
"""
import concurrent.futures as cf
import json
import pathlib
import subprocess
import sys
import time
import urllib.parse
import urllib.request

WORK = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).parent / "xlsx-work"
REFRESH = "--refresh" in sys.argv
POOL = WORK / "pool"
POOL.mkdir(parents=True, exist_ok=True)
MAX_BYTES = 2 * 1024 * 1024
UA = {"User-Agent": "quire-corpus/0.1"}

SOURCES = {
    "lo": ("LibreOffice/core", "master"),
    "poi": ("apache/poi", "trunk"),
}


def gh_api(path: str) -> dict | list:
    out = subprocess.run(["gh", "api", path], capture_output=True, text=True,
                         check=True).stdout
    return json.loads(out)


def gh_ok():
    r = subprocess.run(["gh", "auth", "status"], capture_output=True)
    if r.returncode != 0:
        sys.exit("FAIL: 需要已登入的 gh CLI 來列 GitHub 檔案清單")


def list_lo() -> list[tuple[str, int]]:
    """BFS 走 LibreOffice/core 的 sc/qa 子樹，回傳 (path, size) 之 .xlsx/.xlsm。"""
    repo, ref = SOURCES["lo"]
    root = gh_api(f"repos/{repo}/git/trees/{ref}")["tree"]
    sha = next(e["sha"] for e in root if e["path"] == "sc")
    sha = next(e["sha"] for e in gh_api(f"repos/{repo}/git/trees/{sha}")["tree"] if e["path"] == "qa")
    stack = [(sha, "sc/qa")]
    hits: list[tuple[str, int]] = []
    while stack:
        s, pre = stack.pop()
        for e in gh_api(f"repos/{repo}/git/trees/{s}")["tree"]:
            p = f"{pre}/{e['path']}"
            if e["type"] == "tree":
                stack.append((e["sha"], p))
            elif e["type"] == "blob" and e["path"].lower().endswith((".xlsx", ".xlsm")):
                hits.append((p, e.get("size") or 0))
    return hits


def list_poi() -> list[tuple[str, int]]:
    """POI recursive tree（未截斷），只留 test-data/spreadsheet/ 的 .xlsx/.xlsm。"""
    repo, ref = SOURCES["poi"]
    tree = gh_api(f"repos/{repo}/git/trees/{ref}?recursive=1")
    if tree.get("truncated"):
        sys.exit("FAIL: POI recursive tree 被截斷，請改用 BFS 走查")
    hits = []
    for e in tree["tree"]:
        p = e["path"]
        if not p.startswith("test-data/spreadsheet/"):
            continue
        if not p.lower().endswith((".xlsx", ".xlsm")):
            continue  # .xls 是 OLE2 非 OPC、.ods 非 OOXML，一律排除
        if e.get("size"):
            hits.append((p, e["size"]))
    return hits


def load_lists() -> dict[str, list[tuple[str, int]]]:
    lists = {}
    for src, fn in (("lo", "lo_xlsx.tsv"), ("poi", "poi_xlsx.tsv")):
        fp = WORK / fn
        if fp.exists() and not REFRESH:
            lists[src] = [(l.split("\t")[0], int(l.split("\t")[1]))
                          for l in fp.read_text().splitlines() if l.strip()]
            print(f"{src}: 清單快取 {len(lists[src])} 檔（--refresh 可重列）")
        else:
            hits = (list_lo if src == "lo" else list_poi)()
            fp.write_text("".join(f"{p}\t{s}\n" for p, s in sorted(hits)))
            lists[src] = hits
            print(f"{src}: 列得 {len(hits)} 檔 .xlsx/.xlsm")
    return lists


def fetch(job):
    src, path, size = job
    out = POOL / f"{src}__{path.replace('/', '__')}"
    if out.exists() and out.stat().st_size > 0:
        return (path, "cached", 0)
    marker = out.with_name(out.name + ".notzip")
    if marker.exists():  # 上次判定非 ZIP（多為 OLE2 假檔名的 .xlsx）：不再重抓
        return (path, "notzip", 0)
    repo, ref = SOURCES[src]
    url = f"https://raw.githubusercontent.com/{repo}/{ref}/" + urllib.parse.quote(path)
    err = "?"
    for attempt in range(3):  # 重試：單檔失敗跳過＋回報計數
        try:
            req = urllib.request.Request(url, headers=UA)
            with urllib.request.urlopen(req, timeout=90) as r:
                data = r.read()
            if not data.startswith(b"PK"):
                # 副檔名 xlsx 實為 OLE2/其他格式（POI/LibreOffice fuzz 與 CVE 測試檔常見）：
                # 屬預期剔除而非錯誤，記 marker 使重跑冪等零錯誤。
                marker.write_text(f"magic={data[:8]!r}\n")
                return (path, "notzip", 0)
            out.write_bytes(data)
            return (path, "ok", len(data))
        except Exception as e:  # noqa: BLE001
            err = str(e)
            time.sleep(1.5 * (attempt + 1))
    return (path, f"ERR {err}", 0)


def main():
    gh_ok()
    lists = load_lists()
    jobs = [(src, p, s) for src, hits in lists.items() for p, s in hits if s <= MAX_BYTES]
    skipped_big = sum(1 for src, hits in lists.items() for _, s in hits if s > MAX_BYTES)
    print(f"{len(jobs)} jobs（跳過 >2MB {skipped_big} 檔）", flush=True)
    ok = err = notzip = 0
    with cf.ThreadPoolExecutor(max_workers=10) as ex:
        for path, status, _ in ex.map(fetch, jobs):
            if status in ("ok", "cached"):
                ok += 1
            elif status == "notzip":
                notzip += 1  # 副檔名 xlsx 實為 OLE2 等：預期剔除
                print(f"SKIP {path}: 非 ZIP（OLE2 假裝 xlsx）", flush=True)
            else:
                err += 1
                print(f"FAIL {path}: {status}", flush=True)
    print(f"done ok={ok} notzip={notzip} err={err}")
    sys.exit(0 if err == 0 else 1)


if __name__ == "__main__":
    main()
