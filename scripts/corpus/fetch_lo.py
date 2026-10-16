#!/usr/bin/env python3
"""Parallel-download LibreOffice QA docx corpus into pool/ (stdlib only)."""
import concurrent.futures as cf
import hashlib
import pathlib
import sys
import urllib.request

BASE = {
    "ooxmlimport": "https://raw.githubusercontent.com/LibreOffice/core/master/sw/qa/extras/ooxmlimport/data/",
    "ooxmlexport": "https://raw.githubusercontent.com/LibreOffice/core/master/sw/qa/extras/ooxmlexport/data/",
}
POOL = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else pathlib.Path(__file__).parent) / "pool"
POOL.mkdir(exist_ok=True)
MAX_BYTES = 2 * 1024 * 1024


def load_jobs():
    jobs = []
    for src, tsv in (("ooxmlimport", "lo_import.tsv"), ("ooxmlexport", "lo_export.tsv")):
        for line in pathlib.Path(tsv).read_text().splitlines():
            name, size = line.split("\t")
            if int(size) <= MAX_BYTES:
                jobs.append((src, name, int(size)))
    return jobs


def fetch(job):
    src, name, size = job
    out = POOL / f"{src}__{name}"
    if out.exists() and out.stat().st_size > 0:
        return (src, name, "cached", out.stat().st_size)
    url = BASE[src] + name.replace(" ", "%20")
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "quire-corpus/0.1"})
        with urllib.request.urlopen(req, timeout=60) as r:
            data = r.read()
    except Exception as e:  # noqa: BLE001
        return (src, name, f"ERR {e}", 0)
    if not data.startswith(b"PK"):
        return (src, name, "NOT-ZIP", 0)
    out.write_bytes(data)
    return (src, name, "ok", len(data))


def main():
    jobs = load_jobs()
    print(f"{len(jobs)} jobs", flush=True)
    ok = err = 0
    with cf.ThreadPoolExecutor(max_workers=12) as ex:
        for src, name, status, size in ex.map(fetch, jobs):
            if status in ("ok", "cached"):
                ok += 1
            else:
                err += 1
                print(f"FAIL {src}/{name}: {status}", flush=True)
    print(f"done ok={ok} err={err}")
    sys.exit(0)


if __name__ == "__main__":
    main()
