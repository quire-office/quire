#!/usr/bin/env python3
"""從 pool/ + probe_results.json 挑選 ≥400 份 docx 入 corpus/，輸出 manifest.json。

策略：
1. 先對關鍵特徵做貪集合覆蓋（每輪挑新增特徵最多的檔），保證每個標籤都有檔案覆蓋。
2. 再按來源配額補齊真實世界多樣性（LO bug 文件為主體）。
3. 檔名 sanitize、去重（sha256）、限大小（單檔 ≤500KB，PM 上限 2026-10-16）。
4. 既有 corpus（批次 phase0）沿用：同路徑同 sha 者標 batch=phase0；
   因大小/去重被擠出的 phase0 檔記錄於 manifest.dropped_phase0（PM：超限降序棄舊）。
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

TARGET = 400
MAX_SIZE = 500 * 1024  # PM 2026-10-16：單檔 ≤500KB
MAX_TOTAL = 60 * 1024 * 1024  # corpus 總量 ≤60MB

KEY_TAGS = [
    "table", "merged_cells", "header_footer", "footnote", "endnote", "image",
    "tracked_revisions", "comments", "numbering", "fields", "textbox", "equation",
    "chart", "embedded_object", "content_control", "columns", "hyperlink",
    "smartart", "rtl", "east_asian", "superscript_subscript", "strike",
    "highlight", "dropcap", "bookmark", "symbol", "sections", "drawing",
    "legacy_vml", "glossary", "custom_xml", "fonts_embedded", "altchunk",
    # 病態表格（PM 量化門檻）
    "nested_table", "nested_table_3plus", "table_cell_multi_para", "table_cell_list",
    "table_across_pages", "tbl_layout_autofit", "merged_interleave", "table_in_columns",
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
        "license": "MIT（Microsoft Open-Xml-PowerTools；功能導向範例文件）",
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
        "license": "MIT OR Apache-2.0（本專案自產，代碼同本倉庫授權）",
        "kind": "self-generated",
    },
    "oxmlsdk": {
        "repo": "dotnet/Open-XML-SDK",
        "url": "https://github.com/dotnet/Open-XML-SDK/tree/main/test/DocumentFormat.OpenXml.Tests.Assets",
        "license": "MIT（dotnet/Open-XML-SDK 倉庫授權；Microsoft 官方 Wordprocessing test assets）",
        "kind": "test-asset",
    },
    "docx4j": {
        "repo": "plutext/docx4j",
        "url": "https://github.com/plutext/docx4j/tree/VERSION_17_3_3",
        "license": "Apache-2.0（plutext/docx4j 倉庫授權；test resources 與 sample docs）",
        "kind": "test-asset",
    },
    "pydocx": {
        "repo": "python-openxml/python-docx",
        "url": "https://github.com/python-openxml/python-docx/tree/master/features/steps/test_files",
        "license": "MIT（python-openxml/python-docx 倉庫授權；behave test files）",
        "kind": "test-asset",
    },
}

SAFE = re.compile(r"[^A-Za-z0-9._+-]")


def src_of(name: str) -> str:
    return name.split("__", 1)[0]


def orig_path(name: str) -> str:
    # ooxmlimport__foo.docx / poi__test-data__document__foo.docx
    src, rest = name.split("__", 1)
    return rest.replace("__", "/")


def pool_name_of(corpus_file: str, orig_path: str) -> str:
    src = corpus_file.split("/", 1)[0]
    return f"{src}__{orig_path.replace('/', '__')}"


def corpus_path_of(pool_name: str) -> str:
    src, rest = pool_name.split("__", 1)
    return f"{src}/{SAFE.sub('_', rest.replace('__', '/'))}"


def main():
    global POOL
    POOL = pathlib.Path(sys.argv[3]) if len(sys.argv) > 3 else WORK / "pool"
    sys.path.insert(0, str(HERE))
    from probe import probe as probe_file  # noqa: E402
    # 既有 corpus（phase0）清單：(corpus file → old entry)，並就地重新探測標籤
    # （phase0 檔案已過 PII 消毒＝sha 與 pool 原始檔不同，須位元級沿用既有檔，不得從 pool 覆寫）。
    old_manifest_p = OUT / "manifest.json"
    old_entries: dict[str, dict] = {}
    if old_manifest_p.exists():
        try:
            old = json.loads(old_manifest_p.read_text())
            old_entries = {e["file"]: e for e in old.get("files", [])}
        except Exception:
            old_entries = {}
    print(f"既有 corpus phase0 檔數: {len(old_entries)}")
    chosen: list[tuple[str, dict, str]] = []
    picked_sha: set[str] = set()
    picked_path: set[str] = set()
    covered: set[str] = set()

    def take(name: str, r: dict, batch: str) -> bool:
        cpath = corpus_path_of(name)
        if r["sha256"] in picked_sha or cpath in picked_path:
            return False
        picked_sha.add(r["sha256"])
        picked_path.add(cpath)
        chosen.append((name, r, batch))
        covered.update(set(r["tags"]) & set(KEY_TAGS))
        return True

    # pass 0a: phase0 全數沿用（位元級零倒退；消毒後 sha 進 manifest）
    dropped_phase0 = []
    phase0_reprobe: dict[str, dict] = {}  # corpus file → probe result（給 materialize）
    for f, e in sorted(old_entries.items()):
        if e.get("batch", "phase0") != "phase0":
            continue  # 上一輪 expand 檔：重新從 pool 挑選（冪等），不佔 phase0 配額
        p = OUT / f
        if not p.exists():
            dropped_phase0.append({"file": f, "reason": "既有 corpus 檔遺失"})
            continue
        import hashlib
        if hashlib.sha256(p.read_bytes()).hexdigest() != e["sha256"]:
            dropped_phase0.append({"file": f, "reason": "既有 corpus 檔 sha 與 manifest 不符"})
            continue
        r = probe_file(p)
        if r is None:
            dropped_phase0.append({"file": f, "reason": "既有 corpus 檔探測失敗"})
            continue
        # 沿用 manifest 的消毒後 sha/bytes；標籤用新探測（含病態表格標籤）
        r = {**r, "sha256": e["sha256"], "size": e["bytes"]}
        phase0_reprobe[f] = r
        name = pool_name_of(f, e.get("orig_path", f.split("/", 1)[1]))
        if not take(name, r, "phase0"):
            dropped_phase0.append({"file": f, "reason": "sha 與其他檔案重複"})
    if dropped_phase0:
        print(f"WARN: phase0 無法沿用 {len(dropped_phase0)} 檔：", dropped_phase0)

    # candidates: valid, unique (not dup), size cap（phase0 路徑一律排除——已沿用消毒版）
    cands = {}
    for name, r in PROBE.items():
        if "dup_of" in r:
            continue
        src = src_of(name)
        if src not in SOURCES:
            continue
        if not (POOL / name).exists():
            continue
        if corpus_path_of(name) in picked_path:
            continue
        if r["size"] > MAX_SIZE:
            continue
        cands[name] = r
    print(f"candidates after size/dup filter: {len(cands)}")

    # pass 0b: 自產病態表格必收（量化門檻靠它們）
    for name, r in sorted(cands.items()):
        if "patho_" in name:
            take(name, r, "expand")

    # pass 1: greedy set cover over KEY_TAGS (prefer smaller files first)
    missing = lambda: set(KEY_TAGS) - covered
    while missing():
        want = missing()
        best = None
        for name, r in cands.items():
            if r["sha256"] in picked_sha or corpus_path_of(name) in picked_path:
                continue
            gain = len(want & set(r["tags"]))
            if gain and (best is None or gain > best[0] or (gain == best[0] and r["size"] < best[2])):
                best = (gain, name, r["size"], r)
        if best is None:
            break
        take(best[1], best[3], "expand")
    print(f"after cover: {len(chosen)} files, missing tags: {sorted(missing())}")

    # pass 2: fill to TARGET with diverse real-world (LO) files, largest-feature-rich first
    def fill(pred, want_n):
        got = 0
        for name, r in sorted(cands.items(), key=lambda x: (-len(x[1]["tags"]), x[1]["size"])):
            if got >= want_n or len(chosen) >= TARGET:
                break
            if r["sha256"] in picked_sha or not pred(name, r):
                continue
            if take(name, r, "expand"):
                got += 1

    fill(lambda n, r: src_of(n) == "selfgen", 26)
    fill(lambda n, r: src_of(n) == "ooxmlimport", 45)
    fill(lambda n, r: src_of(n) == "ooxmlexport", 150)
    fill(lambda n, r: src_of(n) == "poi", 40)
    fill(lambda n, r: src_of(n) == "xopt", 30)
    fill(lambda n, r: src_of(n) == "phpword", 16)
    fill(lambda n, r: src_of(n) == "oxmlsdk", 40)
    fill(lambda n, r: src_of(n) == "docx4j", 30)
    fill(lambda n, r: src_of(n) == "pydocx", 14)
    fill(lambda n, r: True, TARGET - len(chosen))

    # 總量上限 60MB：超限降序棄新檔（phase0 保留，先保新特徵覆蓋已由 pass 0/1 保底）
    total = sum(r["size"] for _, r, _ in chosen)
    if total > MAX_TOTAL:
        trimmable = sorted((i for i, (_, r, b) in enumerate(chosen) if b == "expand"),
                           key=lambda i: -chosen[i][1]["size"])
        for i in trimmable:
            if total <= MAX_TOTAL:
                break
            name, r, _ = chosen[i]
            dropped_phase0.append({"file": corpus_path_of(name), "reason": f"總量 >60MB 裁剪（{r['size']} bytes）"})
            total -= r["size"]
        keep_idx = {i for i in range(len(chosen))
                    if not any(corpus_path_of(chosen[i][0]) == d["file"] for d in dropped_phase0)}
        chosen = [chosen[i] for i in sorted(keep_idx)]

    # materialize：phase0 檔（已 PII 消毒）先暫存，避免被 pool 未消毒版覆寫
    stash = OUT.parent / (OUT.name + ".stash")
    if stash.exists():
        shutil.rmtree(stash)
    if OUT.exists():
        shutil.copytree(OUT, stash)
        shutil.rmtree(OUT)
    entries = []
    for name, r, batch in chosen:
        src = src_of(name)
        meta = SOURCES[src]
        subdir = OUT / src
        subdir.mkdir(parents=True, exist_ok=True)
        safe = SAFE.sub("_", orig_path(name))
        dst = subdir / safe
        cpath = f"{src}/{safe}"
        entry = {
            "file": cpath,
            "source": meta["repo"],
            "source_url": meta["url"],
            "orig_path": orig_path(name),
            "kind": meta["kind"],
            "license": meta["license"],
            "batch": batch,
            "sha256": r["sha256"],
            "bytes": r["size"],
            "nparts": r["nparts"],
            "tags": r["tags"],
        }
        if batch == "phase0":
            old_e = old_entries.get(cpath, {})
            if old_e.get("scrubbed"):
                entry["scrubbed"] = True
            shutil.copy2(stash / cpath, dst)
        else:
            shutil.copy2(POOL / name, dst)
        entries.append(entry)
    if stash.exists():
        shutil.rmtree(stash)
    from collections import Counter
    cov = Counter()
    for e in entries:
        for t in e["tags"]:
            cov[t] += 1
    manifest = {
        "schema": "quire-corpus/1",
        "description": "真實世界 DOCX corpus：真實 bug 回報文件（LibreOffice/POI，上游已匿名化）+ 功能导向公開範本（Open-Xml-PowerTools/PHPWord）+ Microsoft OOXML 官方 test assets + docx4j/python-docx test resources + 自產文件（含病態表格）。",
        "count": len(entries),
        "files": entries,
        "feature_coverage": dict(sorted(cov.items())),
        "dropped_phase0": dropped_phase0,
    }
    (OUT / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=1))
    print(f"corpus: {len(entries)} files -> {OUT}（phase0 沿用 {sum(1 for _, _, b in chosen if b == 'phase0')}，expand {sum(1 for _, _, b in chosen if b == 'expand')}）")
    for t, c in sorted(cov.items()):
        print(f"  {t:28s} {c}")


if __name__ == "__main__":
    main()
