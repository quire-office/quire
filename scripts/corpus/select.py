#!/usr/bin/env python3
"""從 pool/ + probe_results.json 挑選 ~130 份 docx 入 corpus/，輸出 manifest.json。

策略：
1. 先對關鍵特徵做貪集合覆蓋（每輪挑新增特徵最多的檔），保證每個標籤都有檔案覆蓋。
2. 再按來源配額補齊真實世界多樣性（LO bug 文件為主體）。
3. 檔名 sanitize、去重（sha256）、限大小。
"""
from __future__ import annotations

import hashlib
import json
import pathlib
import re
import shutil
import sys

HERE = pathlib.Path(__file__).parent
# workdir：放 pool/ 與 probe_results.json 的目錄（預設 = 本腳本所在目錄）
WORK = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else HERE
POOL = WORK / "pool"
OUT = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else WORK / "corpus"
PROBE = json.loads((WORK / "probe_results.json").read_text())

TARGET = 130
MAX_SIZE = 600 * 1024

KEY_TAGS = [
    "table", "merged_cells", "header_footer", "footnote", "endnote", "image",
    "tracked_revisions", "comments", "numbering", "fields", "textbox", "equation",
    "chart", "embedded_object", "content_control", "columns", "hyperlink",
    "smartart", "rtl", "east_asian", "superscript_subscript", "strike",
    "highlight", "dropcap", "bookmark", "symbol", "sections", "drawing",
    "legacy_vml", "glossary", "custom_xml", "fonts_embedded", "altchunk",
]

SOURCES = {
    "ooxmlimport": {
        "repo": "LibreOffice/core",
        "url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/ooxmlimport/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；原始為 tdf bugzilla 匿名化真實用戶文件）",
        "kind": "real-world",
    },
    "ooxmlexport": {
        "repo": "LibreOffice/core",
        "url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/ooxmlexport/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；原始為 tdf bugzilla 匿名化真實用戶文件）",
        "kind": "real-world",
    },
    "poi": {
        "repo": "apache/poi",
        "url": "https://github.com/apache/poi/tree/trunk/test-data/document",
        "license": "Apache-2.0（Bugzilla 真實回報文件，POI 測試資料）",
        "kind": "real-world",
    },
    "xopt": {
        "repo": "EricWhiteDev/Open-Xml-PowerTools",
        "url": "https://github.com/EricWhiteDev/Open-Xml-PowerTools",
        "license": "MIT（Microsoft Open-Xml-PowerTools；功能导向範例文件）",
        "kind": "template",
    },
    "phpword": {
        "repo": "PHPOffice/PHPWord",
        "url": "https://github.com/PHPOffice/PHPWord",
        "license": "MIT（PHPWord 測試/範例文件）",
        "kind": "template",
    },
    "selfgen": {
        "repo": "quire",
        "url": "local://scripts/gen_selfgen.py",
        "license": "MIT OR Apache-2.0（本專案自產）",
        "kind": "self-generated",
    },
}

SAFE = re.compile(r"[^A-Za-z0-9._+-]")


def src_of(name: str) -> str:
    return name.split("__", 1)[0]


def orig_path(name: str) -> str:
    # ooxmlimport__foo.docx / poi__test-data__document__foo.docx
    src, rest = name.split("__", 1)
    return rest.replace("__", "/")


def main():
    global POOL
    POOL = pathlib.Path(sys.argv[3]) if len(sys.argv) > 3 else WORK / "pool"
    # candidates: valid, unique (not dup), size cap
    cands = []
    for name, r in PROBE.items():
        if "dup_of" in r or r["size"] > MAX_SIZE:
            continue
        src = src_of(name)
        if src not in SOURCES:
            continue
        if not (POOL / name).exists():
            continue
        cands.append((name, r))
    print(f"candidates after size/dup filter: {len(cands)}")

    chosen: list[tuple[str, dict]] = []
    picked_sha: set[str] = set()
    covered: set[str] = set()

    def take(name: str, r: dict, why: str):
        if r["sha256"] in picked_sha:
            return False
        picked_sha.add(r["sha256"])
        chosen.append((name, r))
        covered.update(set(r["tags"]) & set(KEY_TAGS))
        return True

    # pass 1: greedy set cover over KEY_TAGS (prefer smaller files first)
    missing = lambda: set(KEY_TAGS) - covered
    while missing():
        want = missing()
        best = None
        for name, r in cands:
            if r["sha256"] in picked_sha:
                continue
            gain = len(want & set(r["tags"]))
            if gain and (best is None or gain > best[0] or (gain == best[0] and r["size"] < best[2])):
                best = (gain, name, r["size"], r)
        if best is None:
            break
        take(best[1], best[3], f"cover +{best[0]}")
    print(f"after cover: {len(chosen)} files, missing tags: {sorted(missing())}")

    # pass 2: fill to TARGET with diverse real-world (LO) files, largest-feature-rich first
    def fill(pred, want_n):
        got = 0
        for name, r in sorted(cands, key=lambda x: (-len(x[1]["tags"]), x[1]["size"])):
            if got >= want_n or len(chosen) >= TARGET:
                break
            if r["sha256"] in picked_sha or not pred(name, r):
                continue
            take(name, r, "fill")
            got += 1

    fill(lambda n, r: src_of(n) == "selfgen", 12)
    fill(lambda n, r: src_of(n) == "ooxmlimport", 25)
    fill(lambda n, r: src_of(n) == "ooxmlexport", 35)
    fill(lambda n, r: src_of(n) == "poi", 20)
    fill(lambda n, r: src_of(n) == "xopt", 22)
    fill(lambda n, r: src_of(n) == "phpword", 10)
    fill(lambda n, r: True, TARGET - len(chosen))

    # materialize
    if OUT.exists():
        shutil.rmtree(OUT)
    entries = []
    for name, r in chosen:
        src = src_of(name)
        meta = SOURCES[src]
        subdir = OUT / src
        subdir.mkdir(parents=True, exist_ok=True)
        safe = SAFE.sub("_", orig_path(name))
        dst = subdir / safe
        shutil.copy2(POOL / name, dst)
        entries.append({
            "file": f"{src}/{safe}",
            "source": meta["repo"],
            "source_url": meta["url"],
            "orig_path": orig_path(name),
            "kind": meta["kind"],
            "license": meta["license"],
            "sha256": r["sha256"],
            "bytes": r["size"],
            "nparts": r["nparts"],
            "tags": r["tags"],
        })
    from collections import Counter
    cov = Counter()
    for e in entries:
        for t in e["tags"]:
            cov[t] += 1
    manifest = {
        "schema": "quire-corpus/1",
        "description": "Phase0-1 真實世界 DOCX corpus：真實 bug 回報文件（LibreOffice/POI，上游已匿名化）+ 功能导向公開範本（Open-Xml-PowerTools/PHPWord）+ 自產文件。",
        "count": len(entries),
        "files": entries,
        "feature_coverage": dict(sorted(cov.items())),
    }
    (OUT / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=1))
    print(f"corpus: {len(entries)} files -> {OUT}")
    for t, c in sorted(cov.items()):
        print(f"  {t:28s} {c}")


if __name__ == "__main__":
    main()
