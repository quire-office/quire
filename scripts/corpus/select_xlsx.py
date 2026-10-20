#!/usr/bin/env python3
"""從 xlsx-work/pool/ 挑選 ≥60 份真實世界 XLSX/XLSM 入 corpus-xlsx/，輸出 manifest.json。

策略（鏡像 select.py）：
1. 探測特徵標籤（formula/sharedStrings/pivot/chart/inlineStr/merged/numberFormat/
   dateSystem1904/definedNames/sheetProtection/before_compat）＋sha256 去重。
2. 两家去重：sha 相同留 poi/ 版，來源欄保留雙方出處註記（also_from）。
3. 關鍵特徵貪集合覆蓋保底，再按來源配額（lo/poi 各半）補齊多樣性。
4. 單檔 ≤2MB（fetch 已閘）、總量 ≤30MB；冪等可重跑（corpus-xlsx/ 覆蓋重建）。

用法：python3 select_xlsx.py [work] [out]（預設 xlsx-work/ 與 <repo>/corpus-xlsx）
"""
from __future__ import annotations

import concurrent.futures as cf
import hashlib
import json
import pathlib
import re
import shutil
import sys
import zipfile
from collections import Counter

HERE = pathlib.Path(__file__).parent
WORK = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "xlsx-work"
POOL = WORK / "pool"
OUT = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else HERE.parent.parent / "corpus-xlsx"

TARGET = 80          # 閘門 ≥60，留餘裕
MAX_TOTAL = 30 * 1024 * 1024
QUOTA = {"lo": 40, "poi": 40}

KEY_TAGS = [
    "formula", "sharedStrings", "pivot_table", "chart", "inlineStr", "merged",
    "numberFormat", "dateSystem1904", "definedNames", "sheetProtection",
    "before_compat", "xlsm", "drawing", "table_part", "conditional_format",
    "data_validation", "east_asian", "hyperlink", "doc_properties",
]

# 驗收量化門檻（PM 卡 2026-10-20）：tag → 最少覆蓋檔數
FEATURE_QUORUM = {
    "formula": 20,
    "sharedStrings": 40,
    "pivot_table": 3,
    "chart": 3,
}

SOURCES = {
    "lo": {
        "repo": "LibreOffice/core",
        "url": "https://github.com/LibreOffice/core/tree/master/sc/qa",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；sc/qa 為 tdf bugzilla 匿名化真實用戶文件）",
        "kind": "real-world",
    },
    "poi": {
        "repo": "apache/poi",
        "url": "https://github.com/apache/poi/tree/trunk/test-data/spreadsheet",
        "license": "Apache-2.0（Bugzilla 真實回報文件，POI 測試資料）",
        "kind": "real-world",
    },
}

SAFE = re.compile(r"[^A-Za-z0-9._+-]")
CJK_RE = re.compile(r"[㐀-䶿一-鿿豈-﫿぀-ヿ가-힯]")


def src_of(name: str) -> str:
    return name.split("__", 1)[0]


def orig_path(name: str) -> str:
    _, rest = name.split("__", 1)
    return rest.replace("__", "/")


def corpus_path_of(pool_name: str) -> str:
    src, rest = pool_name.split("__", 1)
    return f"{src}/{SAFE.sub('_', rest.replace('__', '/'))}"


def probe(p: pathlib.Path) -> dict | None:
    """探測一份 .xlsx/.xlsm：zip 合法性＋OPC 結構＋特徵標籤。非法回傳 None。"""
    try:
        data = p.read_bytes()
        zf = zipfile.ZipFile(p)
        names = set(zf.namelist())
    except Exception:  # noqa: BLE001
        return None
    if "[Content_Types].xml" not in names:  # 非 OPC 封裝
        return None
    tags: set[str] = set()
    try:
        ct = zf.read("[Content_Types].xml")
    except Exception:  # noqa: BLE001
        return None
    # 巨集檔判定：vbaProject.bin 或 macroEnabled content-type（兩條件任一，與 verify 鏡像）
    macro = "xl/vbaProject.bin" in names
    if not macro:
        try:
            macro = b"macroEnabled" in ct
        except Exception:  # noqa: BLE001
            macro = False
    if macro:
        tags.add("xlsm")
    wb = b""
    if "xl/workbook.xml" in names:
        try:
            wb = zf.read("xl/workbook.xml")
        except Exception:  # noqa: BLE001
            wb = b""
    if b"date1904=" in wb and (b'date1904="1"' in wb or b'date1904="true"' in wb):
        tags.add("dateSystem1904")
    if b"<definedNames" in wb:
        tags.add("definedNames")
    if not (b"mc:Ignorable" in wb or b"x14ac" in wb or b"mc:" in wb):
        tags.add("before_compat")  # 未宣告相容性擴充命名空間＝相容套件之前的舊檔
    if "xl/sharedStrings.xml" in names:
        tags.add("sharedStrings")
        try:  # CJK 文字多半存於 sharedStrings，只掃 sheet 會漏標 east_asian
            if CJK_RE.search(zf.read("xl/sharedStrings.xml").decode("utf-8", "ignore")):
                tags.add("east_asian")
        except Exception:  # noqa: BLE001
            pass
    sheets = sorted(n for n in names if re.match(r"xl/worksheets/sheet\d+\.xml$", n))
    for sn in sheets:
        try:
            s = zf.read(sn)
        except Exception:  # noqa: BLE001
            continue
        if b"<f>" in s or b"<f " in s:
            tags.add("formula")
        if b't="inlineStr"' in s:
            tags.add("inlineStr")
        if b"<mergeCell" in s:
            tags.add("merged")
        if b"<sheetProtection" in s:
            tags.add("sheetProtection")
        if b"<hyperlink" in s:
            tags.add("hyperlink")
        if b"<dataValidation" in s:
            tags.add("data_validation")
        if b"<conditionalFormatting" in s:
            tags.add("conditional_format")
        if b"<drawing" in s:
            tags.add("drawing")
        try:
            if CJK_RE.search(s.decode("utf-8", "ignore")):
                tags.add("east_asian")
        except Exception:  # noqa: BLE001
            pass
    if any(n.startswith("xl/pivotTables/") or n.startswith("xl/pivotCache/") for n in names):
        tags.add("pivot_table")
    if any(n.startswith("xl/charts/") or re.match(r"xl/drawings/chart\d+\.xml$", n) for n in names) \
            or b"/chart" in ct:
        tags.add("chart")
    if any(n.startswith("xl/tableParts/") for n in names):
        tags.add("table_part")
    if "xl/styles.xml" in names:
        try:
            st = zf.read("xl/styles.xml")
            if re.search(rb'<numFmt [^>]*formatCode="[^"]', st):
                tags.add("numberFormat")  # 自訂數字格式（非內建 General 系）
        except Exception:  # noqa: BLE001
            pass
    if "docProps/core.xml" in names or "docProps/app.xml" in names:
        tags.add("doc_properties")
    return {
        "sha256": hashlib.sha256(data).hexdigest(),
        "size": len(data),
        "nparts": len(names),
        "doc_type": "xlsm" if "xlsm" in tags else "xlsx",
        "tags": sorted(tags),
    }


def main() -> int:
    files = sorted(p for p in POOL.iterdir()
                   if p.suffix.lower() in (".xlsx", ".xlsm") and p.name.split("__", 1)[0] in SOURCES)
    print(f"pool files: {len(files)}")
    results: dict[str, dict] = {}
    with cf.ProcessPoolExecutor(max_workers=8) as ex:
        futs = {ex.submit(probe, p): p for p in files}
        for fut in cf.as_completed(futs):
            p = futs[fut]
            try:
                r = fut.result()
            except Exception:  # noqa: BLE001
                r = None
            if r:
                results[p.name] = r
    print(f"valid xlsx/xlsm: {len(results)}")

    # 去重：sha 相同留 poi/ 版（卡面鐵律），lo 版記 dup_of＋出處註記來源
    by_sha: dict[str, str] = {}
    for name in sorted(results, key=lambda n: (src_of(n) != "poi", n)):  # poi 優先登記
        r = results[name]
        if r["sha256"] in by_sha:
            r["dup_of"] = by_sha[r["sha256"]]
        else:
            by_sha[r["sha256"]] = name
    dupes = sum(1 for r in results.values() if "dup_of" in r)
    print(f"跨來源去重（留 poi 版）: {dupes}")

    cands = {n: r for n, r in results.items()
             if "dup_of" not in r and r["size"] <= 2 * 1024 * 1024}
    print(f"candidates: {len(cands)}")

    chosen: list[tuple[str, dict]] = []
    picked_sha: set[str] = set()
    picked_path: set[str] = set()
    covered: set[str] = set()

    def take(name: str, r: dict) -> bool:
        cpath = corpus_path_of(name)
        if r["sha256"] in picked_sha or cpath in picked_path:
            return False
        picked_sha.add(r["sha256"])
        picked_path.add(cpath)
        chosen.append((name, r))
        covered.update(set(r["tags"]) & set(KEY_TAGS))
        return True

    # pass 1: 關鍵特徵貪集合覆蓋（先小檔）
    missing = lambda: set(KEY_TAGS) - covered  # noqa: E731
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
        take(best[1], best[3])
    print(f"after cover: {len(chosen)} files, missing tags: {sorted(missing())}")

    # pass 2: 來源配額補齊（特徵多者优先，再小檔优先）
    per_src = Counter(src_of(n) for n, _ in chosen)

    def fill(src: str, want_n: int):
        got = 0
        order = sorted(((n, r) for n, r in cands.items() if src_of(n) == src),
                       key=lambda x: (-len(x[1]["tags"]), x[1]["size"]))
        for name, r in order:
            if got >= want_n or len(chosen) >= TARGET:
                break
            if r["sha256"] in picked_sha:
                continue
            if take(name, r):
                got += 1
                per_src[src] += 1

    for src, quota in QUOTA.items():
        fill(src, max(0, quota - per_src[src]))
    fill_any = sorted(cands.items(), key=lambda x: (-len(x[1]["tags"]), x[1]["size"]))
    for name, r in fill_any:
        if len(chosen) >= TARGET:
            break
        if r["sha256"] not in picked_sha:
            take(name, r)

    # 總量上限：超限先棄最大新檔（配額已達後才觸發）
    total = sum(r["size"] for _, r in chosen)
    if total > MAX_TOTAL:
        drop = sorted(range(len(chosen)), key=lambda i: -chosen[i][1]["size"])
        keep = set(range(len(chosen)))
        for i in drop:
            if total <= MAX_TOTAL:
                break
            total -= chosen[i][1]["size"]
            keep.discard(i)
        chosen = [chosen[i] for i in sorted(keep)]

    # materialize：覆蓋重建 corpus-xlsx/{lo,poi}/
    for src in SOURCES:
        d = OUT / src
        if d.exists():
            shutil.rmtree(d)
    entries = []
    for name, r in chosen:
        src = src_of(name)
        meta = SOURCES[src]
        cpath = corpus_path_of(name)
        dst = OUT / cpath
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(POOL / name, dst)
        entry = {
            "file": cpath,
            "source": meta["repo"],
            "source_url": meta["url"],
            "orig_path": orig_path(name),
            "kind": meta["kind"],
            "license": meta["license"],
            "batch": "xlsx-g1",
            "doc_type": r["doc_type"],
            "sha256": r["sha256"],
            "bytes": r["size"],
            "nparts": r["nparts"],
            "tags": r["tags"],
        }
        # 跨來源重複留 poi 版：來源欄保留雙方出處註記
        if src == "poi":
            twin = next((n for n, rr in results.items()
                         if rr.get("dup_of") == name), None)
            if twin:
                entry["also_from"] = f"{SOURCES[src_of(twin)]['repo']}:{orig_path(twin)}"
        entries.append(entry)

    cov: Counter = Counter()
    for e in entries:
        for t in e["tags"]:
            cov[t] += 1
    manifest = {
        "schema": "quire-corpus-xlsx/1",
        "description": "真實世界 XLSX/XLSM corpus：LibreOffice sc/qa（tdf bugzilla 匿名化真實文件）"
                       "＋ apache/poi test-data/spreadsheet（Bugzilla 真實回報文件）。"
                       "sha256 跨來源去重留 POI 版（also_from 記雙出處）。",
        "count": len(entries),
        "files": entries,
        "feature_coverage": dict(sorted(cov.items())),
    }
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=1))
    print(f"corpus-xlsx: {len(entries)} files（{total / 1024 / 1024:.1f} MB）-> {OUT}")
    print("來源分佈:", dict(Counter(src_of(n) for n, _ in chosen)))
    print("doc_type 分佈:", dict(Counter(e["doc_type"] for e in entries)))
    print("\n== 特徵覆蓋率 ==")
    for t in sorted(set(KEY_TAGS) | set(cov)):
        q = FEATURE_QUORUM.get(t)
        mark = f"（門檻 ≥{q}）" if q else ""
        print(f"  {t:20s} {cov.get(t, 0):5d}{mark}")
    fails = [t for t, q in FEATURE_QUORUM.items() if cov.get(t, 0) < q]
    if fails:
        print(f"WARN: 未達門檻（老實回報，不湊數假標籤）: {fails}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
