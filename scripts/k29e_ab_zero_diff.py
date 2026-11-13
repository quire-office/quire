#!/usr/bin/env python3
"""K29e 編輯卡零回退證法：全庫 A/B 零 DIFF（base binary vs head binary）。

口徑（quire-redlight-conventions）：每檔一次 quire-cli dump（全頁 JSON），
ThreadPoolExecutor 平行；比對前剝 data_b64、浮點 round 4、含頁面尺寸欄。
PASS 條件＝DIFF 檔數恰為 0（編輯卡無合法觸發集）。
"""
import glob
import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

BASE = "/home/jerry/.hermes/profiles/ow-core/cache/scratch/k29e-base-target/debug/quire-cli"
HEAD = "/home/jerry/projects/quire/.worktrees/t_4b55fc9d/target/debug/quire-cli"
ROOT = "/home/jerry/projects/quire/.worktrees/t_4b55fc9d"

STRIP_KEYS = {"data_b64"}


def norm(obj):
    if isinstance(obj, dict):
        return {
            k: norm(v)
            for k, v in obj.items()
            if k not in STRIP_KEYS
        }
    if isinstance(obj, list):
        return [norm(v) for v in obj]
    if isinstance(obj, float):
        return round(obj, 4)
    return obj


def dump(binary, path):
    r = subprocess.run(
        [binary, "dump", path],
        capture_output=True,
        text=True,
        timeout=600,
        cwd=ROOT,
    )
    if r.returncode != 0:
        return ("ERR", r.stderr.strip().splitlines()[-1] if r.stderr else f"rc={r.returncode}")
    try:
        return ("OK", norm(json.loads(r.stdout)))
    except Exception as e:  # noqa: BLE001
        return ("ERR", f"json: {e}")


def compare(path):
    a = dump(BASE, path)
    b = dump(HEAD, path)
    if a[0] == "ERR" and b[0] == "ERR" and a[1] == b[1]:
        return ("SKIPPED_SAME_ERR", path, a[1])
    if a != b:
        return ("DIFF", path, None)
    return ("SAME", path, None)


def main():
    files = sorted(set(glob.glob("corpus/**/*.docx", recursive=True)))
    print(f"corpus docx 檔數：{len(files)}")
    diffs = []
    skipped = []
    errors = []
    with ThreadPoolExecutor(12) as ex:
        for status, path, detail in ex.map(compare, files):
            if status == "DIFF":
                diffs.append(path)
            elif status == "SKIPPED_SAME_ERR":
                skipped.append((path, detail))
    print(f"DIFF 檔數：{len(diffs)}")
    for p in diffs:
        print("  DIFF:", p)
    print(f"兩側同錯跳過：{len(skipped)}")
    for p, d in skipped:
        print("  SKIP:", p, "|", d)
    print("RESULT:", "PASS" if not diffs else "FAIL")


if __name__ == "__main__":
    main()
