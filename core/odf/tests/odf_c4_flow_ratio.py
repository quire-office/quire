#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ODF-C4 驗收閘門：odt_flow_lab ratio 重跑（本分支態）——卡面口徑。

口徑（逐字照 docs/odf-survey/e2-flow-attribution.md §開頭）：
  ratio = 我方 `odt-render` 的 total_text_len（含空白 chars）
        ÷ LO PDF `get_text()` 逐行 strip 後 join 的字數（**含空白**）
前一版 c4_flow_ratio.py 的分母誤用「去空白」口徑（E2 解剖的**對帳**口徑，
非卡面 ratio 口徑），median 因此虛高至 1.114；本腳本改回卡面口徑。

可比對檔＝corpus-odt/manifest.json 的 50 檔減 2 加密檔＝48。
LO PDF 真值＝c4_lo_pdf/index.json（LO 24.8 --convert-to pdf）。
"""
import json, pathlib, re, statistics, subprocess

REPO = pathlib.Path(__file__).resolve().parents[3]
SCR = pathlib.Path("/home/jerry/.hermes/profiles/ow-core/cache/scratch")
CLI = REPO / "target/debug/quire-cli"
WS = re.compile(r"\s+")

index = json.loads((SCR / "c4_lo_pdf/index.json").read_text(encoding="utf-8"))
manifest = json.loads((REPO / "corpus-odt/manifest.json").read_text(encoding="utf-8"))

rows, skipped = [], []
for rec in manifest["files"]:
    rel = "corpus-odt/" + rec["file"]
    ent = index.get(rel)
    if ent is None or not ent.get("pdf") or not pathlib.Path(ent["pdf"]).exists():
        skipped.append((rel, "無 LO PDF（加密檔）"))
        continue
    import pymupdf
    doc = pymupdf.open(ent["pdf"])
    lines = []
    for page in doc:
        for l in page.get_text().splitlines():
            n = WS.sub(" ", l).strip()
            if n:
                lines.append(n)
    pages = len(doc)
    doc.close()
    lo_chars = sum(len(l) for l in lines)  # 卡面口徑：含空白
    if lo_chars == 0:
        skipped.append((rel, "LO 抽出 0 字"))
        continue
    r = subprocess.run([str(CLI), "odt-render", str(REPO / rel)],
                      capture_output=True, text=True)
    if r.returncode != 0:
        skipped.append((rel, "odt-render 非零：" + r.stderr.strip()[:80]))
        continue
    j = json.loads(r.stdout)
    rows.append({"file": rec["file"], "pages": pages, "lo_chars": lo_chars,
                 "ow_total_text_len": j["total_text_len"],
                 "ratio": round(j["total_text_len"] / lo_chars, 3),
                 "page_source": j.get("page_source"),
                 "page_w_pt": round(j["page"]["w_pt"], 2),
                 "page_h_pt": round(j["page"]["h_pt"], 2)})

ratios = [r["ratio"] for r in rows]
print(f"可比對檔數: {len(rows)}（卡面預期 48）")
print(f"跳過: {skipped}")
print(f"median ratio: {statistics.median(ratios):.3f}")
print(f"mean   ratio: {statistics.mean(ratios):.3f}")
print(f"min/max ratio: {min(ratios):.3f} / {max(ratios):.3f}")
print("\n最低八席:")
for r in sorted(rows, key=lambda r: r["ratio"])[:8]:
    print(f"  {r['ratio']:.3f}  {r['file']}")
print("\n偏低三席追蹤（卡面基線 strict 0.570 / tdf124637 0.634 / tdf87533 0.791）:")
for w, base in (("strict", 0.570), ("tdf124637_sectionMargin", 0.634),
                ("tdf87533_bidi", 0.791)):
    hit = next((r for r in rows if w in r["file"]), None)
    if hit:
        d = hit["ratio"] - base
        print(f"  {w}: {hit['ratio']:.3f}（基線 {base:.3f}，Δ{d:+.3f}）")
    else:
        print(f"  {w}: N/A")
print("\n頁幾何生效原因分佈:")
src = {}
for r in rows:
    src[r["page_source"]] = src.get(r["page_source"], 0) + 1
for k, v in sorted(src.items()):
    print(f"  {k}: {v} 檔")
(SCR / "c4_flow_ratio2.json").write_text(json.dumps(rows, ensure_ascii=False, indent=1),
                                         encoding="utf-8")
print(f"\n寫入 {SCR/'c4_flow_ratio2.json'}")
