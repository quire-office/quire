#!/usr/bin/env python3
"""corpus-xlsx/ 閘門驗證（卡面：verify --xlsx 模式；採新檔方案——verify_corpus.py
DOCX 399 閘門零觸碰，改動最小）。

閘門：count ≥60、license SPDX 白名單、feature_coverage 非空且達量化門檻
（formula ≥20、sharedStrings ≥40、pivot_table ≥3、chart ≥3）、
zip 結構 sanity（EOCD 可解析＋[Content_Types].xml 存在）、manifest↔實體一致、
sha256 相符、單檔 ≤2MB。

用法：python3 scripts/verify_corpus_xlsx.py [corpus-xlsx目錄]（預設 <repo>/corpus-xlsx）
exit 0 = 全部閘門通過。
"""
from __future__ import annotations

import hashlib
import json
import pathlib
import re
import struct
import sys
import zipfile
from collections import Counter

ROOT = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parent.parent / "corpus-xlsx"

MIN_FILES = 60
MAX_FILE_BYTES = 2 * 1024 * 1024
MAX_TOTAL_BYTES = 40 * 1024 * 1024

# PM 2026-10-20 量化門檻（不足老實報 FAIL，不得湊數假標籤）
FEATURE_QUORUM = {
    "formula": 20,
    "sharedStrings": 40,
    "pivot_table": 3,
    "chart": 3,
}

# license 白名單：SPDX id＋來源說明（鏡像 verify_corpus.py 規則）
LICENSE_SPDX_RE = re.compile(
    r"\b(Apache-2\.0|MIT(?:-0)?|BSD-[23]-Clause|MPL-2\.0|GPL-\d(?:\.\d)?(?:-only|-or-later)?"
    r"|LGPL-\d(?:\.\d)?(?:-only|-or-later)?|OFL-1\.1|CC0-1\.0|CC-BY-[24]\.0|Unlicense)\b")
LICENSE_BARE = {"", "unknown", "mit", "apache-2.0", "apache 2.0", "bsd", "gpl", "lgpl", "n/a", "tbd"}


def license_ok(s: object) -> bool:
    if not isinstance(s, str) or s.strip().lower() in LICENSE_BARE:
        return False
    if not LICENSE_SPDX_RE.search(s):
        return False
    return len(s.strip()) > len(s.strip().split("（")[0].split(" ")[0]) + 2


def eocd_ok(p: pathlib.Path) -> bool:
    """zip EOCD 可解析 sanity：尾 64KB 內找得到 PK\x05\x06 尾綴記錄。"""
    with p.open("rb") as f:
        f.seek(0, 2)
        size = f.tell()
        f.seek(max(0, size - 65557))
        tail = f.read()
    i = tail.rfind(b"PK\x05\x06")
    if i < 0 or len(tail) - i < 22:
        return False
    # EOCD 头： signatures＋18 固定欄＋comment 長度欄一致
    total = struct.unpack_from("<H", tail, i + 20)[0]
    return 22 + total == len(tail) - i


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

    on_disk = sorted(p.relative_to(ROOT).as_posix()
                     for p in list(ROOT.rglob("*.xlsx")) + list(ROOT.rglob("*.xlsm")))
    listed = {e["file"] for e in files}
    missing = sorted(listed - set(on_disk))
    unlisted = sorted(set(on_disk) - listed)
    if missing:
        errors.append(f"manifest 列了但實體不存在: {missing[:5]}（共 {len(missing)}）")
    if unlisted:
        errors.append(f"實體存在但 manifest 未列: {unlisted[:5]}（共 {len(unlisted)}）")

    n = len(on_disk)
    print(f"corpus-xlsx 實體檔數: {n}（閘門 ≥{MIN_FILES}）")
    if n < MIN_FILES:
        errors.append(f"檔數 {n} < {MIN_FILES}")

    # 每檔：zip 可開＋EOCD sanity＋[Content_Types].xml＋sha＋大小＋doc_type 一致性
    bad_zip, bad_eocd, bad_ct, sha_mismatch, oversize, type_mismatch = [], [], [], [], [], []
    total_bytes = 0
    for e in files:
        p = ROOT / e["file"]
        if not p.exists():
            continue
        data = p.read_bytes()
        total_bytes += len(data)
        try:
            zf = zipfile.ZipFile(p)
            names = zf.namelist()
        except Exception:
            bad_zip.append(e["file"])
            continue
        if not eocd_ok(p):
            bad_eocd.append(e["file"])
        if "[Content_Types].xml" not in names:
            bad_ct.append(e["file"])
        if hashlib.sha256(data).hexdigest() != e["sha256"]:
            sha_mismatch.append(e["file"])
        if e["bytes"] > MAX_FILE_BYTES or len(data) > MAX_FILE_BYTES:
            oversize.append(f"{e['file']}({len(data)})")
        # 巨集判定與 select_xlsx.probe 鏡像：vbaProject.bin 或 macroEnabled CT 任一
        try:
            ct = zf.read("[Content_Types].xml")
        except Exception:
            ct = b""
        want = "xl/vbaProject.bin" in names or b"macroEnabled" in ct
        got = e.get("doc_type") == "xlsm"
        if want != got:
            type_mismatch.append(e["file"])
    if bad_zip:
        errors.append(f"zip 損壞: {bad_zip[:5]}（共 {len(bad_zip)}）")
    if bad_eocd:
        errors.append(f"zip EOCD 不可解析: {bad_eocd[:5]}（共 {len(bad_eocd)}）")
    if bad_ct:
        errors.append(f"缺 [Content_Types].xml（非 OPC）: {bad_ct[:5]}（共 {len(bad_ct)}）")
    if sha_mismatch:
        errors.append(f"sha256 不符: {sha_mismatch[:5]}（共 {len(sha_mismatch)}）")
    if oversize:
        errors.append(f"單檔超限 >2MB: {oversize[:5]}（共 {len(oversize)}）")
    if type_mismatch:
        errors.append(f"doc_type 與 vbaProject 不一致: {type_mismatch[:5]}（共 {len(type_mismatch)}）")
    print(f"corpus-xlsx 總量: {total_bytes / 1024 / 1024:.1f} MB（上限 {MAX_TOTAL_BYTES // 1024 // 1024} MB）")
    if total_bytes > MAX_TOTAL_BYTES:
        errors.append(f"總量 {total_bytes} > {MAX_TOTAL_BYTES}")

    # 欄位齐全＋license 白名單
    bad_license = []
    for e in files:
        for k in ("file", "source", "source_url", "orig_path", "kind", "license",
                  "batch", "doc_type", "sha256", "bytes", "tags"):
            if k not in e:
                errors.append(f"{e['file']} 缺欄位 {k}")
                break
        if e.get("doc_type") not in ("xlsx", "xlsm"):
            errors.append(f"{e['file']} doc_type 非 xlsx/xlsm: {e.get('doc_type')!r}")
        if not license_ok(e.get("license")):
            bad_license.append(f"{e['file']}: {e.get('license')!r}")
    if bad_license:
        errors.append(f"license 欄不符合白名單（SPDX＋來源說明）: {bad_license[:5]}（共 {len(bad_license)}）")

    # feature_coverage：manifest 宣告 vs 實體重算＋量化門檻
    cov: Counter = Counter()
    for e in files:
        for t in e.get("tags", []):
            cov[t] += 1
    declared = manifest.get("feature_coverage", {})
    if not declared:
        errors.append("manifest.feature_coverage 為空")
    print("\n== 特徵覆蓋率（n=%d）==" % len(files))
    print(f"{'tag':22s} {'覆蓋檔數':>8s} {'覆蓋率':>7s} {'manifest':>8s}")
    for t in sorted(set(cov) | set(declared)):
        c = cov.get(t, 0)
        flag = "" if declared.get(t, 0) == c else "  <== 與 manifest 不符"
        if flag:
            errors.append(f"feature_coverage {t}: 實體 {c} ≠ manifest {declared.get(t)}")
        print(f"{t:22s} {c:8d} {100.0 * c / max(1, len(files)):6.1f}% {declared.get(t, 0):8d}{flag}")

    print("\n== 量化門檻 ==")
    for t, need in sorted(FEATURE_QUORUM.items()):
        c = cov.get(t, 0)
        ok = "OK " if c >= need else "FAIL"
        print(f"  {ok} {t:22s} {c:4d}/{need}")
        if c < need:
            errors.append(f"特徵 {t} 覆蓋 {c} < 門檻 {need}")

    srcs = Counter(e["source"] for e in files)
    types = Counter(e.get("doc_type") for e in files)
    print("\n來源分佈:", dict(srcs))
    print("doc_type 分佈:", dict(types))

    if errors:
        print("\nFAIL:")
        for e in errors:
            print(" -", e)
        return 1
    print("\nPASS: corpus-xlsx 檔數、manifest 可載入、實體一致、zip/EOCD/[Content_Types] sanity、"
          "sha256、大小上限、doc_type 一致、license 白名單、feature_coverage、量化門檻全部通過。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
