#!/usr/bin/env python3
"""Fetch additional real-world docx corpora: POI, Open-Xml-PowerTools, PHPWord,
python-docx, Microsoft Open-XML-SDK (官方 test files), docx4j test resources."""
import concurrent.futures as cf
import json
import pathlib
import subprocess
import sys
import urllib.parse
import urllib.request

POOL = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else pathlib.Path(__file__).parent) / "pool"
POOL.mkdir(exist_ok=True)
MAX_BYTES = 2 * 1024 * 1024

REPOS = [
    ("poi", "apache/poi", "trunk"),
    ("xopt", "EricWhiteDev/Open-Xml-PowerTools", "master"),
    ("phpword", "PHPOffice/PHPWord", "master"),
    ("oxmlsdk", "dotnet/Open-XML-SDK", "main"),
    ("docx4j", "plutext/docx4j", "VERSION_17_3_3"),
    ("pydocx", "python-openxml/python-docx", "master"),
]
# POI: only document/wordprocessing test-data paths
POI_PREFIXES = ("test-data/document/", "test-data/xwpf/", "poi-examples/src/main/")
# Open-XML-SDK: WordProcessing 相關 test assets（排除 spreadsheet/presentation 誤標）
OXML_PREFIXES = ("test/DocumentFormat.OpenXml.Tests.Assets/assets/",)
OXML_EXCLUDES = ("Spreadsheet", "Presentation", "V2FormatOpenXml", "validation")
# docx4j：test/sample resources（排除 docs/ 文件用附件與 target 產物）
DOCX4J_PREFIXES = ("docx4j-core-tests/src/test/", "docx4j-samples-docx4j/",
                   "docx4j-samples-docx-export-fo/", "docx4j-opencl/")
# python-docx：behave test files
PYDOCX_PREFIXES = ("features/steps/test_files/", "tests/")


def keep(src: str, path: str) -> bool:
    low = path.lower()
    if src == "poi":
        return path.startswith(POI_PREFIXES)
    if src == "oxmlsdk":
        return path.startswith(OXML_PREFIXES) and not any(
            x.lower() in low for x in OXML_EXCLUDES)
    if src == "docx4j":
        return path.startswith(DOCX4J_PREFIXES) and "/target/" not in path
    if src == "pydocx":
        return path.startswith(PYDOCX_PREFIXES)
    return True


def tree_jobs():
    jobs = []
    for src, repo, branch in REPOS:
        out = subprocess.run(
            ["gh", "api", f"repos/{repo}/git/trees/{branch}?recursive=1",
             "--jq", '.tree[]|select(.path|endswith(".docx"))|select(.size!=null and .size>0)|[.size,.path]|@json'],
            capture_output=True, text=True, check=True).stdout
        for line in out.splitlines():
            size, path = json.loads(line)
            if size > MAX_BYTES:
                continue
            if not keep(src, path):
                continue
            jobs.append((src, repo, branch, path, size))
    return jobs


def fetch(job):
    src, repo, branch, path, size = job
    safe = path.replace("/", "__")
    out = POOL / f"{src}__{safe}"
    if out.exists() and out.stat().st_size > 0:
        return (path, "cached")
    url = f"https://raw.githubusercontent.com/{repo}/{branch}/" + urllib.parse.quote(path)
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "quire-corpus/0.1"})
        with urllib.request.urlopen(req, timeout=90) as r:
            data = r.read()
    except Exception as e:  # noqa: BLE001
        return (path, f"ERR {e}")
    if not data.startswith(b"PK"):
        return (path, "NOT-ZIP")
    out.write_bytes(data)
    return (path, "ok")


def main():
    jobs = tree_jobs()
    print(f"{len(jobs)} jobs", flush=True)
    ok = err = 0
    with cf.ThreadPoolExecutor(max_workers=10) as ex:
        for path, status in ex.map(fetch, jobs):
            if status in ("ok", "cached"):
                ok += 1
            else:
                err += 1
                if err <= 20:
                    print(f"FAIL {path}: {status}", flush=True)
    print(f"done ok={ok} err={err}")


if __name__ == "__main__":
    main()
