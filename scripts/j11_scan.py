#!/usr/bin/env python3
"""scripts/j11_scan.py —— 面擴散掃描腳本（J11 專用，支援可重跑抽樣）。

功能：
  對 corpus 抽 n 檔（預設 n=30, seed=20261030）跑 render_lab 三軸度量（色分、圖分、真缺字），
  篩出色分<0.3 / 圖分<0.3 / 真缺字>0 的全部檔案，輸出結構化結果 JSON 與 Markdown 報告。
"""

import argparse
import atexit
import datetime
import json
import os
import pathlib
import random
import re
import sys
import pymupdf

# 優用 render_lab 現成函式
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import render_lab

REPO = pathlib.Path(__file__).resolve().parent.parent


def classify_anomaly(r: dict) -> dict:
    """針對色分<0.3、圖分<0.3、真缺字>0 進行初篩標記（真缺陷候選 vs 度量雜訊候選）。"""
    labels = []
    reasons = []

    c_scores = [s for s in (r.get("color_scores") or []) if s is not None]
    i_scores = [s for s in (r.get("image_scores") or []) if s is not None]
    min_c = min(c_scores) if c_scores else 1.0
    min_i = min(i_scores) if i_scores else 1.0
    missing = r.get("true_missing") or []

    # 1. 真缺字雜訊 vs 真缺陷判斷
    if missing:
        # 已知雜訊模式：
        # - 目錄點線黏 token：包含連續句點或點線，如 '....1', 'word....', '.....'
        # - 純符號或非語意拼裝：如 '_', '--', '\t'
        # - 特殊欄位碼／頁碼代碼：如 PAGE, NUMPAGES 等
        noise_tokens = []
        real_tokens = []
        for t in missing:
            if re.search(r"\.{2,}|…", t) or re.search(r"^\d+$", t):
                noise_tokens.append(t)
            elif len(t) <= 1:
                noise_tokens.append(t)
            else:
                real_tokens.append(t)

        if real_tokens:
            labels.append("真缺字:真缺陷候選")
            reasons.append(f"LO 有實體文字未發射（{len(real_tokens)} 個，例：{real_tokens[:3]}）")
        else:
            labels.append("真缺字:度量雜訊候選")
            reasons.append(f"缺字皆為點線/頁碼/單字符號等度量偽影（{len(noise_tokens)} 個，例：{noise_tokens[:3]}）")

    # 2. 色彩軸判斷
    if min_c < 0.3:
        labels.append("色彩軸:低分")
        reasons.append(f"色分最低 {min_c:.3f}")

    # 3. 圖片軸判斷
    if min_i < 0.3:
        labels.append("圖片軸:低分")
        reasons.append(f"圖分最低 {min_i:.3f}")

    return {
        "min_c": min_c,
        "min_i": min_i,
        "labels": labels,
        "reasons": reasons,
        "is_flagged": (min_c < 0.3 or min_i < 0.3 or len(missing) > 0),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="J11 色/圖/缺字三軸面擴散掃描器")
    ap.add_argument("--corpus", default=str(REPO / "corpus"), help="corpus 目錄")
    ap.add_argument("--n", type=int, default=30, help="抽樣檔數")
    ap.add_argument("--seed", type=int, default=20261030, help="隨機抽樣 seed")
    ap.add_argument("--out", default=str(REPO / "out" / "j11-scan"), help="渲染快取輸出目錄")
    ap.add_argument("--report", default=str(REPO / "docs" / "j11-scan-results.md"), help="Markdown 輸出路徑")
    ap.add_argument("--json-out", default=str(REPO / "docs" / "j11-scan-results.json"), help="JSON 輸出路徑")
    ap.add_argument("--from-json", default=None, help="從既有 JSON 產生 Markdown 報告（不重跑渲染）")
    args = ap.parse_args()

    if args.from_json:
        data = json.loads(pathlib.Path(args.from_json).read_text(encoding="utf-8"))
        meta = data["meta"]
        rows = data["rows"]
        flagged_rows = [r for r in rows if r.get("is_flagged")]
        soffice = "LibreOffice (from cached json)"
    else:
        corpus_dir = pathlib.Path(args.corpus)
        out_dir = pathlib.Path(args.out)
        out_dir.mkdir(parents=True, exist_ok=True)

        manifest_path = corpus_dir / "manifest.json"
        if not manifest_path.exists():
            sys.exit(f"找不到 manifest: {manifest_path}")
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

        rng = random.Random(args.seed)
        picked = rng.sample(manifest["files"], min(args.n, len(manifest["files"])))
        targets = [corpus_dir / f["file"] for f in picked]

        soffice = render_lab.find_soffice()
        scratch, profile_uri, needle = render_lab.lo_scratch(soffice)
        render_lab.kill_stale_lo()
        atexit.register(render_lab.kill_own_lo, needle)

        print(f"=== J11 擴散掃描開始：n={len(targets)}, seed={args.seed} ===")
        print(f"LO: {soffice}")
        print(f"輸出目錄: {out_dir}")

        rows = []
        lo_fail = 0

        for idx, docx in enumerate(targets, 1):
            rel = str(docx.relative_to(REPO)) if docx.is_relative_to(REPO) else str(docx)
            print(f"[{idx:02d}/{len(targets):02d}] → {rel}", flush=True)

            pdf = render_lab.lo_to_pdf(soffice, docx, scratch, profile_uri)
            if pdf is None:
                lo_fail += 1
                row = {"file": rel, "status": "lo_fail", "lo_pages": 0, "ow_pages": 0}
                rows.append(row)
                print("   ✗ LO 轉換失敗（記 lo_fail，繼續）")
                continue

            try:
                lo = render_lab.pdf_page_facts(pdf)
                ow = render_lab.run_quire(docx, out_dir)
                stem = docx.stem
                render_lab.rasterize(pdf, out_dir / "lo-png", stem)
                for svg in sorted((out_dir / "quire" / stem).glob(f"{stem}-p*.svg"))[:4]:
                    sd = pymupdf.open(svg)
                    pix = sd[0].get_pixmap(matrix=pymupdf.Matrix(150 / 72, 150 / 72))
                    (out_dir / "ow-png").mkdir(exist_ok=True)
                    pix.save(out_dir / "ow-png" / (svg.stem + ".png"))
                    sd.close()

                cmp_row = render_lab.compare(rel, lo, ow, sect_pr=render_lab.sect_pr_count(docx))
                j3 = render_lab.j3_metrics(pdf, out_dir, lo, ow)
                cmp_row.update(j3)
                cmp_row["status"] = "ok"
                diag = classify_anomaly(cmp_row)
                cmp_row.update(diag)
                rows.append(cmp_row)

                print(f"   LO {cmp_row['lo_pages']}p / OW {cmp_row['ow_pages']}p | "
                      f"色 {cmp_row.get('color_scores')} | 圖 {cmp_row.get('image_scores')} | "
                      f"缺字 {cmp_row.get('true_missing_n')} (切片 {cmp_row.get('split_ok_n')})")
                if diag["is_flagged"]:
                    print(f"   🚩 標記: {', '.join(diag['labels'])} — {'; '.join(diag['reasons'])}")

            except Exception as e:
                print(f"   ✗ 處理異常: {e}")
                row = {"file": rel, "status": "error", "error": str(e), "lo_pages": 0, "ow_pages": 0}
                rows.append(row)

        # 統計彙整
        flagged_rows = [r for r in rows if r.get("is_flagged")]
        print(f"\n=== 掃描完成：共 {len(rows)} 檔，旗標檔 {len(flagged_rows)} 檔，LO 失敗 {lo_fail} 檔 ===")

        # 寫入 JSON
        json_path = pathlib.Path(args.json_out)
        json_path.parent.mkdir(parents=True, exist_ok=True)
        meta = {
            "timestamp": datetime.datetime.now().isoformat(timespec="seconds"),
            "seed": args.seed,
            "n": len(targets),
            "lo_fail": lo_fail,
            "flagged_count": len(flagged_rows),
        }
        json_path.write_text(json.dumps({"meta": meta, "rows": rows}, ensure_ascii=False, indent=2), encoding="utf-8")
        print(f"JSON 已儲存至: {json_path}")

    # 寫入 Markdown 報告
    report_path = pathlib.Path(args.report)
    report_path.parent.mkdir(parents=True, exist_ok=True)

    n_targets = meta.get("n", len(rows))
    seed_val = meta.get("seed", args.seed)

    md_lines = [
        f"# J11 三軸面擴散掃描報告（n={n_targets}, seed={seed_val}）",
        "",
        f"- 執行時間：`{meta['timestamp']}`",
        f"- 抽樣條件：corpus manifest `n={n_targets}`, `seed={seed_val}`",
        f"- LibreOffice：`{soffice}`",
        f"- 總結：掃描 {len(rows)} 檔，**{len(flagged_rows)} 檔觸發旗標**（色分<0.3 或 圖分<0.3 或 真缺字>0）；LO 失敗 {meta.get('lo_fail', 0)} 檔",
        "",
        "## 1. 觸發旗標檔案彙總表",
        "",
        "| 序號 | 檔案 | LO/OW 頁 | 色分序列 | 圖分序列 | 缺字數 | 標籤分類 | 初步歸因研判 |",
        "|---|---|---|---|---|---|---|---|",
    ]

    def fmt_scores(scores):
        if not scores:
            return "—"
        return ", ".join("—" if s is None else f"{s:.2f}" for s in scores)

    for idx, r in enumerate(flagged_rows, 1):
        c_str = fmt_scores(r.get("color_scores"))
        i_str = fmt_scores(r.get("image_scores"))
        miss_n = r.get("true_missing_n", 0)
        labels = "<br>".join(r.get("labels", []))
        reasons = "; ".join(r.get("reasons", []))
        md_lines.append(
            f"| {idx} | `{r['file']}` | {r.get('lo_pages')}/{r.get('ow_pages')} | {c_str} | {i_str} | {miss_n} | {labels} | {reasons} |"
        )

    md_lines += [
        "",
        "## 2. 全 30 檔掃描完整清單",
        "",
        "| 序號 | 檔案 | 狀態 | 頁數 | 內容分 | 版面分 | 色分 | 圖分 | 真缺字 | 旗標 |",
        "|---|---|---|---|---|---|---|---|---|---|",
    ]

    for idx, r in enumerate(rows, 1):
        if r.get("status") == "lo_fail":
            md_lines.append(f"| {idx} | `{r['file']}` | LO失敗 | — | — | — | — | — | — | — |")
            continue
        c_str = fmt_scores(r.get("color_scores"))
        i_str = fmt_scores(r.get("image_scores"))
        cs = fmt_scores(r.get("content_scores"))
        ls = fmt_scores(r.get("layout_scores"))
        flag = "🚩" if r.get("is_flagged") else "✓"
        md_lines.append(
            f"| {idx} | `{r['file']}` | {r.get('status')} | {r.get('lo_pages')}/{r.get('ow_pages')} | {cs} | {ls} | {c_str} | {i_str} | {r.get('true_missing_n', 0)} | {flag} |"
        )

    report_path.write_text("\n".join(md_lines), encoding="utf-8")
    print(f"Markdown 報告已儲存至: {report_path}")

    # 收尾由 atexit 觸發 kill_own_lo
    return 0


if __name__ == "__main__":
    sys.exit(main())
