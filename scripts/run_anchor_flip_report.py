#!/usr/bin/env python3
"""run_anchor_flip_report.py —— 判準翻盤觸發集對照（bug65649 歸因衍生，度量卡 t_026e8c2e）。

目的：在實跑全樣板前，按「判別臂翻盤」列出「新舊口徑注記不同的檔」清單，
逐檔給「舊口徑（同位索引逐頁比寬高）注記」vs「新口徑（走向拓撲錨定）注記」
前後對照表。觸發集＝docs/bug65649-landscape-diag.md §五混合走向 10 檔
（bug65649 必在列）＋ 呼叫端追加檔。

口徑來源（零複製漂移）：
- 舊口徑＝bug65649 修復前 compare() 的原迴路（本檔 `_old_arm_notes` 逐字重現，
  並用 worst-20261111 基線 bug65649 的 22 筆注記做實檔覆核）。
- 新口徑＝`render_lab.annotate_page_sizes()`（正式實裝，selftest 9a–9g 已鎖）。

LO PDF：優先吃快取 `%TEMP%/quire-render-lab/<parent>__<stem>.pdf`（卡面允許
「快取/重跑」），缺檔才重跑 soffice。我方側一律重跑 `render_lab.run_quire`
（本分支 HEAD 引擎，零 core/ 改動）。

用法：
  /usr/bin/python3 scripts/run_anchor_flip_report.py [--out out/qa-runanchor-flip]
產物：<out>/flip-report.md（逐檔前後注記對照表）＋<out>/flip.json（機器可讀）。
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts"))
import render_lab as RL  # noqa: E402

# 觸發集：docs/bug65649-landscape-diag.md §五 混合走向 10 檔
TRIGGER_SET = [
    "corpus/poi/test-data_document_bug65649.docx",
    "corpus/xopt/TestFiles_HC031-Complicated-Document.docx",
    "corpus/ooxmlexport/fdo78882.docx",
    "corpus/selfgen/ole_embedded_landscape_sections.docx",
    "corpus/ooxmlexport/fdo74605.docx",
    "corpus/ooxmlexport/fdo78886.docx",
    "corpus/ooxmlexport/tdf120511_eatenSection.docx",
    "corpus/ooxmlexport/tdf129575-directAfter.docx",
    "corpus/ooxmlexport/tdf129575-directBefore.docx",
    "corpus/ooxmlexport/tdf129575-styleAfter.docx",
]


def _old_arm_notes(lo_pages: list[dict], brks: list[dict]) -> list[str]:
    """修復前 compare() 的逐頁尺寸迴路逐字重現（行 584–593 口徑，容差 2.0）。"""
    notes: list[str] = []
    for i, p in enumerate(lo_pages):
        if i >= len(brks):
            notes.append(f"第 {i + 1} 頁我方無 pagebreak 記錄")
            continue
        ow_w, ow_h = round(brks[i]["width_pt"], 1), round(brks[i]["height_pt"], 1)
        if abs(p["w_pt"] - ow_w) > 2.0 or abs(p["h_pt"] - ow_h) > 2.0:
            notes.append(
                f"第 {i + 1} 頁 LO {p['w_pt']}×{p['h_pt']} ≠ 我方 {ow_w}×{ow_h}（sectPr）"
            )
    return notes


def _empty_row() -> dict:
    return {"notes": []}


def process(docx: pathlib.Path, out_dir: pathlib.Path, soffice, scratch,
            profile_uri: str) -> dict | None:
    rel = str(docx.relative_to(REPO)) if docx.is_relative_to(REPO) else str(docx)
    pdf = RL.lo_to_pdf(soffice, docx, scratch, profile_uri)
    if pdf is None:
        print(f"   ✗ LO 轉換失敗：{rel}")
        return {"file": rel, "status": "lo_fail"}
    lo = RL.pdf_page_facts(pdf)
    ow = RL.run_quire(docx, out_dir)
    brks = ow.get("breaks") or []
    old_notes = _old_arm_notes(lo, brks)
    row = _empty_row()
    RL.annotate_page_sizes(row, lo, brks)
    new_notes = row["notes"]
    old_sect = [n for n in old_notes if "（sectPr）" in n]
    new_sect = [n for n in new_notes if "sectPr" in n and "容量漂移" not in n]
    lo_runs = RL.orientation_runs([(p["w_pt"], p["h_pt"]) for p in lo])
    ow_runs = RL.orientation_runs([(round(b["width_pt"], 1),
                                    round(b["height_pt"], 1)) for b in brks])
    topo_lo = "".join(r["orient"] for r in lo_runs)
    topo_ow = "".join(r["orient"] for r in ow_runs)
    flipped = old_notes != new_notes
    rec = {
        "file": rel, "status": "ok",
        "lo_pages": len(lo), "ow_pages": len(brks),
        "topo_lo": topo_lo, "topo_ow": topo_ow,
        "topo_equal": bool(lo_runs) and bool(ow_runs) and topo_lo == topo_ow,
        "old_sectpr_n": len(old_sect), "new_sectpr_n": len(new_sect),
        "new_drift_notes": [n for n in new_notes if "容量漂移" in n],
        "flipped": flipped,
        "old_notes": old_notes, "new_notes": new_notes,
    }
    print(f"   LO {len(lo)} 頁／我方 {len(brks)} 頁；拓撲 LO={topo_lo} 我方={topo_ow}"
          f"｜舊 sectPr {len(old_sect)} 筆 → 新 {len(new_sect)} 筆"
          f"｜翻盤={'是' if flipped else '否'}")
    return rec


def write_report(recs: list[dict], out_md: pathlib.Path) -> None:
    flipped = [r for r in recs if r.get("flipped")]
    lines = [
        "# 走向拓撲錨定判準翻盤觸發集對照（bug65649 衍生，t_026e8c2e）",
        "",
        f"- 觸發集：混合走向 10 檔（docs/bug65649-landscape-diag.md §五）；"
        f"成功比對 {sum(1 for r in recs if r.get('status') == 'ok')} 檔、"
        f"LO 失敗 {sum(1 for r in recs if r.get('status') == 'lo_fail')} 檔。",
        f"- 注記翻盤（新舊口徑注記不同）：**{len(flipped)} 檔**。",
        "",
        "## 總表",
        "",
        "| 檔案 | LO 頁 | 我方頁 | 拓撲 LO | 拓撲我方 | 拓撲全等 | 舊 sectPr 筆數 "
        "| 新 sectPr 筆數 | 翻盤 |",
        "|---|---:|---:|---|---|:-:|---:|---:|:-:|",
    ]
    for r in recs:
        if r.get("status") != "ok":
            lines.append(f"| {r['file']} | — | — | — | — | — | — | — | lo_fail |")
            continue
        lines.append(
            f"| {r['file']} | {r['lo_pages']} | {r['ow_pages']} | {r['topo_lo']} "
            f"| {r['topo_ow']} | {'是' if r['topo_equal'] else '否'} "
            f"| {r['old_sectpr_n']} | {r['new_sectpr_n']} "
            f"| {'**是**' if r['flipped'] else '否'} |")
    lines += ["", "## 逐檔前後注記對照", ""]
    for r in recs:
        if r.get("status") != "ok":
            continue
        lines += [f"### {r['file']}", ""]
        if not r["old_notes"] and not r["new_notes"]:
            lines += ["- 舊口徑：（無注記）", "- 新口徑：（無注記）", ""]
            continue
        lines.append(f"- 舊口徑注記（{len(r['old_notes'])} 筆）：")
        lines += [f"  - {n}" for n in (r["old_notes"] or ["（無）"])]
        lines.append(f"- 新口徑注記（{len(r['new_notes'])} 筆）：")
        lines += [f"  - {n}" for n in (r["new_notes"] or ["（無）"])]
        lines.append("")
    out_md.write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=str(REPO / "out" / "qa-runanchor-flip"))
    ap.add_argument("--extra", nargs="*", default=[],
                    help="追加觸發集 docx（相對 repo）")
    a = ap.parse_args()
    out_dir = pathlib.Path(a.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    soffice = RL.find_soffice()
    scratch, profile_uri, needle = RL.lo_scratch(soffice)
    RL.kill_stale_lo()
    import atexit
    atexit.register(RL.kill_own_lo, needle)

    targets = [REPO / f for f in TRIGGER_SET + list(a.extra)]
    recs = []
    for docx in targets:
        rel = str(docx.relative_to(REPO)) if docx.is_relative_to(REPO) else str(docx)
        if not docx.exists():
            print(f"   ✗ 找不到 {rel}（記 missing）")
            recs.append({"file": rel, "status": "missing"})
            continue
        print(f"→ {rel}")
        recs.append(process(docx, out_dir, soffice, scratch, profile_uri))

    (out_dir / "flip.json").write_text(
        json.dumps(recs, ensure_ascii=False, indent=1), encoding="utf-8")
    write_report([r for r in recs if r], out_dir / "flip-report.md")
    print(f"\n對照表 → {out_dir / 'flip-report.md'}")
    print(f"機器可讀 → {out_dir / 'flip.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
