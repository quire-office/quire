#!/usr/bin/env python3
"""F1 影響面普查：docx 為 zip，故須開封後再查 drawingml/2006/chart。
列出所有含圖表 part 的語料檔案＋圖型統計，寫入 docs/f1-survey/chart_corpus.json。
（解剖報告 §4 語料普查的實據來源，非推測。）"""
import json
import re
import zipfile
from collections import Counter
from pathlib import Path

ROOT = Path("/home/jerry/projects/quire/.worktrees/t_f3acd271")
CHART_URI = "drawingml/2006/chart"
KINDS = ["barChart", "lineChart", "pieChart", "areaChart", "scatterChart",
         "bubbleChart", "doughnutChart", "radarChart", "ofPieChart", "pie3DChart",
         "bar3DChart", "line3DChart", "area3DChart", "surfaceChart", "bubble3DChart"]

rows = []
for docx in sorted(ROOT.glob("corpus/**/*.docx")):
    try:
        zf = zipfile.ZipFile(docx)
    except Exception as exc:  # 破損檔如實記
        rows.append({"file": str(docx.relative_to(ROOT)), "error": str(exc)})
        continue
    names = zf.namelist()
    chart_parts = [n for n in names if n.startswith("word/charts/chart") and n.endswith(".xml")]
    if not chart_parts:
        # 掛載點也可能只存在於 document.xml 的 graphicData uri
        try:
            doc = zf.read("word/document.xml").decode("utf-8", "replace")
        except KeyError:
            doc = ""
        if CHART_URI not in doc:
            continue
    kinds = Counter()
    caches = Counter()
    refs = Counter()
    titles = 0
    for part in chart_parts:
        try:
            raw = zf.read(part).decode("utf-8", "replace")
        except KeyError:
            continue
        for k in KINDS:
            n = len(re.findall(r"<c:%s[ >]" % k, raw))
            if n:
                kinds[k] += n
        caches["numCache"] += len(re.findall(r"<c:numCache>", raw))
        caches["strCache"] += len(re.findall(r"<c:strCache>", raw))
        refs["numRef"] += len(re.findall(r"<c:numRef>", raw))
        refs["strRef"] += len(re.findall(r"<c:strRef>", raw))
        titles += len(re.findall(r"<c:title>", raw))
    rows.append({
        "file": str(docx.relative_to(ROOT)),
        "chart_parts": chart_parts,
        "kinds": dict(kinds),
        "cache": dict(caches),
        "ref": dict(refs),
        "title_nodes": titles,
    })

out = ROOT / "docs/f1-survey/chart_corpus.json"
out.write_text(json.dumps(rows, ensure_ascii=False, indent=1), encoding="utf-8")
print(f"含圖表檔案 {len(rows)} 檔 → {out}")
for r in rows:
    print(f"{r['file']}  parts={len(r.get('chart_parts', []))} "
          f"kinds={r.get('kinds')} cache={r.get('cache')} ref={r.get('ref')} "
          f"title={r.get('title_nodes')} {r.get('error', '')}")
