#!/usr/bin/env python3
"""corpus/ 閘門驗證：≥100 檔、manifest 可被 json 載入、manifest↔實體檔一致、特徵覆蓋率統計輸出。

用法：python3 scripts/verify_corpus.py [corpus目錄]（預設 <repo>/corpus）
exit 0 = 全部閘門通過。
"""
from __future__ import annotations

import json
import pathlib
import sys
import zipfile

ROOT = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parent.parent / "corpus"

MIN_FILES = 100


def main() -> int:
    errors: list[str] = []
    manifest_path = ROOT / "manifest.json"
    if not manifest_path.exists():
        print(f"FAIL: 找不到 {manifest_path}")
        return 1
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as e:
        print(f"FAIL: manifest.json 無法被 python json 載入: {e}")
        return 1
    files = manifest.get("files", [])
    print(f"manifest schema: {manifest.get('schema')}")
    print(f"manifest count 欄位: {manifest.get('count')}；files 列舉: {len(files)}")

    docx_on_disk = sorted(p.relative_to(ROOT).as_posix() for p in ROOT.rglob("*.docx"))
    listed = {e["file"] for e in files}
    missing = sorted(listed - set(docx_on_disk))
    unlisted = sorted(set(docx_on_disk) - listed)
    if missing:
        errors.append(f"manifest 列了但實體不存在: {missing[:5]}（共 {len(missing)}）")
    if unlisted:
        errors.append(f"實體存在但 manifest 未列: {unlisted[:5]}（共 {len(unlisted)}）")

    n = len(docx_on_disk)
    print(f"corpus/*.docx 實體檔數: {n}（閘門 ≥{MIN_FILES}）")
    if n < MIN_FILES:
        errors.append(f"檔數 {n} < {MIN_FILES}")

    # sha/size spot check + zip validity on every file
    bad_zip = []
    sha_mismatch = []
    for e in files:
        p = ROOT / e["file"]
        if not p.exists():
            continue
        data = p.read_bytes()
        try:
            zipfile.ZipFile(p).namelist()
        except Exception:
            bad_zip.append(e["file"])
        import hashlib
        if hashlib.sha256(data).hexdigest() != e["sha256"]:
            sha_mismatch.append(e["file"])
    if bad_zip:
        errors.append(f"zip 損壞: {bad_zip[:5]}（共 {len(bad_zip)}）")
    if sha_mismatch:
        errors.append(f"sha256 不符: {sha_mismatch[:5]}（共 {len(sha_mismatch)}）")

    # source/license fields present
    for e in files:
        for k in ("source", "source_url", "license", "kind", "tags", "sha256", "bytes"):
            if k not in e:
                errors.append(f"{e['file']} 缺欄位 {k}")
                break

    # feature coverage stats (recounted from disk-listed tags)
    from collections import Counter
    cov = Counter()
    for e in files:
        for t in e.get("tags", []):
            cov[t] += 1
    declared = manifest.get("feature_coverage", {})
    print("\n== 特徵覆蓋率（n=%d）==" % len(files))
    print(f"{'tag':28s} {'覆蓋檔數':>8s} {'覆蓋率':>7s} {'manifest':>8s}")
    for t in sorted(set(cov) | set(declared)):
        c = cov.get(t, 0)
        flag = "" if declared.get(t, 0) == c else "  <== 與 manifest 不符"
        print(f"{t:28s} {c:8d} {100.0 * c / max(1, len(files)):6.1f}% {declared.get(t, 0):8d}{flag}")

    kinds = Counter(e["kind"] for e in files)
    srcs = Counter(e["source"] for e in files)
    print("\n來源分佈:", dict(srcs))
    print("類型分佈:", dict(kinds))

    if errors:
        print("\nFAIL:")
        for e in errors:
            print(" -", e)
        return 1
    print("\nPASS: corpus 檔數、manifest 可載入、實體一致、zip 有效、特徵統計輸出全部通過。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
