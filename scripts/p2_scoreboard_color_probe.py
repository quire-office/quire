#!/usr/bin/env python3
"""p2_scoreboard_color_probe —— 為 P2-scoreboard-refresh 補「色分」軸（唯讀探針）。

lo_scoreboard.py 只吐內容/版面兩軸；本卡要「版面分與色分新值 vs 10/23 舊值」。
色分口徑 = render_lab.score_color（J3 色軸，150dpi，12 色相×明度 bin 的
Σmin/Σmax），函式直接 from render_lab import，**零引擎觸碰、零 render_lab.py 修改**。

對每檔：LO 端用本輪/舊輪 Windows scratch 裡的 LO PDF（轉檔時間戳過濾），
我方端用 out/lo-scoreboard/quire/<stem>/*.svg。每檔色分 = 各頁 score_color 平均
（與內容分「有效頁平均」同口徑；單頁光柵失敗該頁跳過）。

用法：
  python3 scripts/p2_scoreboard_color_probe.py \
      --scores out/lo-scoreboard/scores.jsonl \
      --pdf-dir /mnt/c/Users/me/AppData/Local/Temp/quire-render-lab \
      --pdf-since "2026-10-31 19:20" \
      --quire-dir out/lo-scoreboard/quire \
      --label new --out /tmp/color_new.json
"""
import argparse
import datetime
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import render_lab as RL  # noqa: E402
import pymupdf  # noqa: E402
import numpy as np  # noqa: E402


def page_arrays(pdf_path, svg_path, page_idx=0):
    d = pymupdf.open(pdf_path)
    try:
        lo_arr = RL._pix_array(d[page_idx]) if page_idx < d.page_count else None
    finally:
        d.close()
    sd = pymupdf.open(svg_path)
    try:
        ow_arr = RL._pix_array(sd[0])
    finally:
        sd.close()
    if lo_arr is None and ow_arr is None:
        return None, None
    if lo_arr is None:
        lo_arr = np.full_like(ow_arr, 255)
    if ow_arr is None:
        ow_arr = np.full_like(lo_arr, 255)
    return lo_arr, ow_arr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scores", required=True)
    ap.add_argument("--pdf-dir", required=True)
    ap.add_argument("--pdf-since", required=True, help="ISO 時間：只認此後生成的 PDF")
    ap.add_argument("--quire-dir", required=True)
    ap.add_argument("--label", default="run")
    ap.add_argument("--out", required=True)
    a = ap.parse_args()

    pdf_dir = pathlib.Path(a.pdf_dir)
    since = datetime.datetime.fromisoformat(a.pdf_since).timestamp()
    scores_path = pathlib.Path(a.scores)
    quire_dir = pathlib.Path(a.quire_dir)

    rows = [json.loads(l) for l in scores_path.read_text(encoding="utf-8").splitlines() if l.strip()]
    out = []
    for r in rows:
        rel = r["file"]
        stem = pathlib.Path(rel).stem
        parent = pathlib.Path(rel).parent.name
        pdf = pdf_dir / f"{parent}__{stem}.pdf"
        svg_dir = quire_dir / stem
        n_pages = max(int(r.get("lo_pages") or 0), int(r.get("ow_pages") or 0))
        per_page = []
        missing = []
        fresh = pdf.exists() and pdf.stat().st_mtime >= since
        for i in range(min(n_pages, RL.J3_MAX_PAGES)):
            svg = svg_dir / f"{stem}-p{i}.svg"
            if not fresh or not svg.exists():
                missing.append(i)
                continue
            try:
                lo_arr, ow_arr = page_arrays(pdf, svg, page_idx=i)
            except Exception:
                missing.append(i)
                continue
            if lo_arr is None:
                missing.append(i)
                continue
            per_page.append(round(RL.score_color(lo_arr, ow_arr), 3))
        out.append({
            "file": rel,
            "label": a.label,
            "pdf_fresh": fresh,
            "pages_scored": len(per_page),
            "pages_skipped": missing,
            "color_scores": per_page,
            "color_mean": (round(sum(per_page) / len(per_page), 3) if per_page else None),
        })
        print(f"{rel}: fresh={fresh} scored={len(per_page)}/{n_pages} "
              f"color={per_page} mean={out[-1]['color_mean']}", flush=True)

    pathlib.Path(a.out).write_text(json.dumps(out, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"→ {a.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
