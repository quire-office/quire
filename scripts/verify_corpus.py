#!/usr/bin/env python3
"""corpus/ 閘門驗證：≥400 檔、manifest 可被 json 載入、manifest↔實體檔一致、
特徵覆蓋率統計輸出、病態表格量化門檻、license 欄白名單檢查。

用法：python3 scripts/verify_corpus.py [corpus目錄]（預設 <repo>/corpus）
exit 0 = 全部閘門通過。
"""
from __future__ import annotations

import json
import pathlib
import re
import sys
import zipfile

ROOT = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parent.parent / "corpus"

MIN_FILES = 400
MAX_TOTAL_BYTES = 60 * 1024 * 1024
MAX_FILE_BYTES = 500 * 1024  # phase0 舊檔沿用 600KB 上限（零倒退豁免）
PHASE0_LEGACY_MAX = 600 * 1024

# PM 2026-10-16 病態表格量化門檻：tag → 最少覆蓋檔數
FEATURE_QUORUM = {
    "nested_table_3plus": 5,
    "table_across_pages": 3,
    "merged_interleave": 5,
    "tbl_layout_autofit": 3,
    "table_in_columns": 3,
    "table_cell_multi_para": 5,
    "table_cell_list": 5,
}

# license 白名單（PM 補正 4）：SPDX id 或「來源倉庫＋具體說明」字串。
# 禁 unknown/空/僅裸字（例如只寫 "MIT" 而無來源說明）。
LICENSE_SPDX_RE = re.compile(
    r"\b(Apache-2\.0|MIT(?:-0)?|BSD-[23]-Clause|MPL-2\.0|GPL-\d(?:\.\d)?(?:-only|-or-later)?"
    r"|LGPL-\d(?:\.\d)?(?:-only|-or-later)?|OFL-1\.1|CC0-1\.0|CC-BY-[24]\.0|Unlicense)\b")
LICENSE_BARE = {"", "unknown", "mit", "apache-2.0", "apache 2.0", "bsd", "gpl", "lgpl", "n/a", "tbd"}


def license_ok(s: object) -> bool:
    if not isinstance(s, str) or s.strip().lower() in LICENSE_BARE:
        return False
    if not LICENSE_SPDX_RE.search(s):
        return False
    # SPDX id 之外必須帶來源說明（長度 > 裸 id：含括號/倉庫名等具體資訊）
    return len(s.strip()) > len(s.strip().split("（")[0].split(" ")[0]) + 2


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

    # sha/size spot check + zip validity on every file + size caps
    bad_zip = []
    sha_mismatch = []
    oversize = []
    total_bytes = 0
    import hashlib
    for e in files:
        p = ROOT / e["file"]
        if not p.exists():
            continue
        data = p.read_bytes()
        total_bytes += len(data)
        try:
            zipfile.ZipFile(p).namelist()
        except Exception:
            bad_zip.append(e["file"])
        if hashlib.sha256(data).hexdigest() != e["sha256"]:
            sha_mismatch.append(e["file"])
        cap = PHASE0_LEGACY_MAX if e.get("batch") == "phase0" else MAX_FILE_BYTES
        if e["bytes"] > cap or len(data) > cap:
            oversize.append(f"{e['file']}({len(data)})")
    if bad_zip:
        errors.append(f"zip 損壞: {bad_zip[:5]}（共 {len(bad_zip)}）")
    if sha_mismatch:
        errors.append(f"sha256 不符: {sha_mismatch[:5]}（共 {len(sha_mismatch)}）")
    if oversize:
        errors.append(f"單檔超限: {oversize[:5]}（共 {len(oversize)}）")
    print(f"corpus 總量: {total_bytes / 1024 / 1024:.1f} MB（上限 {MAX_TOTAL_BYTES // 1024 // 1024} MB）")
    if total_bytes > MAX_TOTAL_BYTES:
        errors.append(f"corpus 總量 {total_bytes} > {MAX_TOTAL_BYTES}")

    # source/license fields present + license 白名單
    bad_license = []
    for e in files:
        for k in ("source", "source_url", "license", "kind", "tags", "sha256", "bytes", "batch"):
            if k not in e:
                errors.append(f"{e['file']} 缺欄位 {k}")
                break
        if not license_ok(e.get("license")):
            bad_license.append(f"{e['file']}: {e.get('license')!r}")
    if bad_license:
        errors.append(f"license 欄不符合白名單（SPDX＋來源說明）: {bad_license[:5]}（共 {len(bad_license)}）")

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

    # 病態表格量化門檻
    print("\n== 病態表格量化門檻 ==")
    for t, need in sorted(FEATURE_QUORUM.items()):
        c = cov.get(t, 0)
        ok = "OK " if c >= need else "FAIL"
        print(f"  {ok} {t:28s} {c:3d}/{need}")
        if c < need:
            errors.append(f"特徵 {t} 覆蓋 {c} < 門檻 {need}")

    kinds = Counter(e["kind"] for e in files)
    srcs = Counter(e["source"] for e in files)
    batches = Counter(e.get("batch", "?") for e in files)
    print("\n來源分佈:", dict(srcs))
    print("類型分佈:", dict(kinds))
    print("批次分佈:", dict(batches))

    if errors:
        print("\nFAIL:")
        for e in errors:
            print(" -", e)
        return 1
    print("\nPASS: corpus 檔數、manifest 可載入、實體一致、zip 有效、大小上限、"
          "license 白名單、病態表格量化門檻全部通過。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
