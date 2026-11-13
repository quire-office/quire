#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ODF-C4 效益面：page_breaks 軸對 LO 實際頁數的 A/B（main 假 A4 vs C4 真頁幾何）。

我方 page_breaks[i]＝第 i 個斷點前的段落索引，頁數＝len(page_breaks)+1。
口徑：逐檔比 LO PDF 頁數；計「|我方頁數 − LO 頁數| ≤ 1」的檔數（頁首頁尾未渲染，
容 1 頁誤差）。main 態用 <repo>/target/debug/quire-cli。
"""
import json, pathlib, subprocess, pymupdf

REPO = pathlib.Path(__file__).resolve().parents[3]
MAIN = pathlib.Path("<repo>/target/debug/quire-cli")
C4 = REPO / "target/debug/quire-cli"
idx = json.loads(pathlib.Path(
    "<hermes>/profiles/ow-core/cache/scratch/c4_lo_pdf/index.json").read_text())
manifest = json.loads((REPO / "corpus-odt/manifest.json").read_text())

rows = []
for rec in manifest["files"]:
    rel = "corpus-odt/" + rec["file"]
    ent = idx.get(rel)
    if not ent or not ent.get("pdf"):
        continue
    pages_lo = len(pymupdf.open(ent["pdf"]))
    o = {}
    for tag, cli in (("main", MAIN), ("c4", C4)):
        r = subprocess.run([str(cli), "odt-render", str(REPO / rel)],
                           capture_output=True, text=True)
        j = json.loads(r.stdout)
        o[tag] = len(j["page_breaks"]) + 1
    rows.append({"file": rec["file"], "lo": pages_lo, "main": o["main"], "c4": o["c4"],
                 "dm": abs(o["main"] - pages_lo), "dc": abs(o["c4"] - pages_lo)})

def score(key):
    ok = sum(1 for r in rows if r[key] <= 1)
    tot = sum(r[key] for r in rows)
    return ok, tot

om, tm = score("dm")
oc, tc = score("dc")
print(f"可比對檔數 {len(rows)}")
print(f"main（假 A4）：|頁數差|≤1 的檔數 {om}/{len(rows)}，總偏差 {tm}")
print(f"C4 （真頁幾何）：|頁數差|≤1 的檔數 {oc}/{len(rows)}，總偏差 {tc}")
imp = sorted(rows, key=lambda r: r["dm"] - r["dc"], reverse=True)
print("\n改善最多 8 席：")
for r in imp[:8]:
    print(f"  {r['dm']:>3} → {r['dc']:>3}  LO={r['lo']:<4} {r['file']}")
print("\n變差最多 5 席：")
for r in imp[-5:]:
    print(f"  {r['dm']:>3} → {r['dc']:>3}  LO={r['lo']:<4} {r['file']}")
