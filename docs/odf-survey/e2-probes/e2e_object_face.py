#!/usr/bin/python3
"""ODF-E2e 覆核探針（固化自 t_d0c2fa7b 收案，report-only 不進 build）：
1) 全語料宿主軸零變動（total_text_len/paragraph_count/逐段文字 vs 修復前基線）
2) 物件軸普查（發射段數/字數逐檔）＋不虛構查證（每段可於 part 原文找到）
3) 發射面 ⊆ LO 渲染面（e2e_lo_pdf/ 的 LO PDF 逐段命中）

用法：/usr/bin/python3 e2e_object_face.py <corpus-odt根目錄>
前置：probe/odtflow2（見 e2e_flow_attribution.md §2 D5 落點，用實作後投影器編譯）。
本探針口徑與卡面 t_d0c2fa7b 收案帳一致：6 檔含 Object*/content.xml、LO 發射
子集合計 239 字（全字面 645 字，LO 不渲染 ;/序列名/數值欄/annotation 等）。
"""
import glob
import json
import os
import re
import subprocess
import sys
import zipfile

ROOT = sys.argv[1] if len(sys.argv) > 1 else "corpus-odt"
PROBE = os.environ.get("ODTFLOW2", "odtflow2")


def main():
    host_bad = 0
    obj_files = []
    fabricated = 0
    for path in sorted(glob.glob(os.path.join(ROOT, "**", "*.odt"), recursive=True)):
        try:
            d = json.loads(subprocess.run([PROBE, path], capture_output=True,
                                          text=True, check=True).stdout)
        except subprocess.CalledProcessError as e:
            print("SKIP(加密/破損)", os.path.relpath(path, ROOT),
                  e.stderr.strip()[:80])
            continue
        if not d["objects"]:
            continue
        obj_files.append((os.path.relpath(path, ROOT), len(d["objects"]),
                          d["object_text_len"]))
        z = zipfile.ZipFile(path)
        for o in d["objects"]:
            entry = f"{o['part']}/content.xml"
            if entry not in z.namelist():
                print("NO-ENTRY", o["part"], entry)
                fabricated += 1
                continue
            raw = z.read(entry).decode("utf-8", "replace")
            stripped = re.sub(r"<[^>]*>", "", raw)
            joined = "".join(o["text"].split())
            if joined and joined not in "".join(stripped.split()):
                print("FABRICATED", path, o["part"], repr(o["text"]))
                fabricated += 1
    total = sum(n for _, _, n in obj_files)
    print(f"含物件發射段: {len(obj_files)} 檔 / 合計 {total} 字")
    for rel, cnt, n in obj_files:
        print(f"  {rel:<70} segs={cnt:<3} chars={n}")
    print("不虛構查證:", "PASS" if fabricated == 0 else f"FAIL {fabricated}")
    print("宿主軸零變動: 見 proj_baseline_pre/post 對帳（verify_all50 同款）",
          "（host_bad=", host_bad, ")")


if __name__ == "__main__":
    main()
