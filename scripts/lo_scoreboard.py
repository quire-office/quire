#!/usr/bin/env python3
"""lo_scoreboard —— LO 收斂追蹤儀表板（唯讀分析，不動引擎）。

用法：
  python3 scripts/lo_scoreboard.py --seed 20261022 --n 25 --out out/lo-scoreboard
  python3 scripts/lo_scoreboard.py --check [BASELINE] --out out/lo-scoreboard
"""
import argparse
import datetime
import json
import os
import random
import re
import shutil
import subprocess
import sys
import zipfile
import pathlib

REPO = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts"))
import render_lab as RL  # noqa: E402

PYTHON = sys.executable


def _abs(p):
    return pathlib.Path(p).resolve()


def sect_count(docx: pathlib.Path) -> int:
    try:
        z = zipfile.ZipFile(docx)
        data = z.read("word/document.xml").decode("utf-8", "ignore")
        return len(re.findall(r"<w:sectPr\b", data))
    except Exception:
        return 1


def layers_for(f: dict, sc: int) -> dict:
    tags = set(f.get("tags", []))
    kind = f.get("kind")
    source = f.get("source")
    return {
        "real": kind == "real-world",
        "selfgen": kind == "self-generated" or source == "quire",
        "table": "table" in tags,
        "image": ("image" in tags) or ("drawing" in tags),
        "endnote": "endnote" in tags,
        "multisection": sc > 1,
    }


LAYER_NAMES = ["real", "selfgen", "table", "image", "endnote", "multisection"]
LAYER_LABELS = {
    "real": "真實檔",
    "selfgen": "自製",
    "table": "含表格",
    "image": "含圖",
    "endnote": "含尾註",
    "multisection": "含多節",
}


def sample_files(manifest, corpus_dir, n, seed):
    rng = random.Random(seed)
    items = []
    for f in manifest["files"]:
        p = corpus_dir / f["file"]
        if not p.exists():
            continue
        sc = sect_count(p)
        items.append({
            "file": f,
            "path": p,
            "rel": str(p.relative_to(REPO)) if p.is_relative_to(REPO) else str(p),
            "layers": layers_for(f, sc),
            "sect_count": sc,
        })
    rng.shuffle(items)

    selected = []
    selected_ids = set()
    targets = {ln: 3 for ln in LAYER_NAMES}
    if n >= sum(targets.values()):
        for ln in LAYER_NAMES:
            need = targets[ln]
            for it in items:
                if need <= 0:
                    break
                if it["path"] in selected_ids:
                    continue
                if not it["layers"][ln]:
                    continue
                selected.append(it)
                selected_ids.add(it["path"])
                need -= 1
    remaining = [it for it in items if it["path"] not in selected_ids]
    rng.shuffle(remaining)
    for it in remaining:
        if len(selected) >= n:
            break
        selected.append(it)
        selected_ids.add(it["path"])
    rng.shuffle(selected)
    return selected[:n]


def _scored(scores):
    """只取有效頁分；None＝0/0 量測偽影，跳過不計（None-safe）。"""
    return [s for s in (scores or []) if s is not None]


def is_measurement_artifact(row):
    """整檔所有頁皆 None ⇒ 量測不可比（不是 0 分事故）。"""
    cs = row.get("content_scores")
    return bool(cs) and all(s is None for s in cs)


def file_score(row):
    cs = _scored(row.get("content_scores"))
    if cs:
        return sum(cs) / len(cs)
    if is_measurement_artifact(row):
        return None
    return 0.0


def layout_score(row):
    ls = _scored(row.get("layout_scores"))
    if ls:
        return sum(ls) / len(ls)
    if is_measurement_artifact(row):
        return None
    return 0.0


def artifact_evidence(row):
    ev = row.get("measurement_artifacts") or []
    if not ev:
        return "LO 圖形實據未記錄（舊版 scores）"
    parts = []
    for e in ev:
        parts.append(
            f"第 {e.get('page', 0) + 1} 頁 drawings={e.get('drawings', 0)}, "
            f"images={e.get('images', 0)}"
            + (f", 內文圖={e['unique_images']}" if "unique_images" in e else ""))
    return "; ".join(parts)


def run_pipeline(args):
    out_dir = pathlib.Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    corpus_dir = pathlib.Path(args.corpus)
    manifest = json.loads((corpus_dir / "manifest.json").read_text(encoding="utf-8"))

    targets = sample_files(manifest, corpus_dir, args.n, args.seed)
    print(f"抽樣 {len(targets)} 檔 (seed={args.seed})", flush=True)

    soffice = RL.find_soffice()
    scratch, profile_uri = RL.lo_scratch(soffice)
    RL.kill_stale_lo()

    rows = []
    lo_fail = 0
    for idx, it in enumerate(targets, 1):
        docx = it["path"]
        rel = it["rel"]
        print(f"[{idx}/{len(targets)}] → {rel}", flush=True)
        pdf = RL.lo_to_pdf(soffice, docx, scratch, profile_uri)
        if pdf is None:
            lo_fail += 1
            row = {"file": rel, "status": "lo_fail", "pages_lo": 0, "pages_ow": 0,
                   "page_count_match": False, "content_scores": [], "layout_scores": [],
                   "first_divergence": None, "notes": ["LO 轉換失敗/逾時"]}
            rows.append(row)
            print("   ✗ LO 轉換失敗", flush=True)
            continue
        try:
            lo = RL.pdf_page_facts(pdf)
            ow = RL.run_quire(docx, out_dir)
        except Exception as e:
            lo_fail += 1
            row = {"file": rel, "status": "lo_fail", "pages_lo": len(lo) if 'lo' in locals() else 0,
                   "pages_ow": 0, "page_count_match": False, "content_scores": [],
                   "layout_scores": [], "first_divergence": None,
                   "notes": [f"比對異常：{e}"]}
            rows.append(row)
            print(f"   ✗ 比對異常：{e}", flush=True)
            continue
        row = RL.compare(rel, lo, ow, sect_pr=RL.sect_pr_count(docx))
        row["status"] = "ok"
        row.setdefault("notes", row.get("notes", []))
        rows.append(row)
        print(f"   LO {row['lo_pages']} 頁 / OW {row['ow_pages']} 頁，"
              f"內容 {row['content_scores'][:4]}（版面 {row['layout_scores'][:4]}）", flush=True)

    # write scores.jsonl
    scores_path = out_dir / "scores.jsonl"
    with scores_path.open("w", encoding="utf-8") as fh:
        for r in rows:
            fh.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"分數 → {scores_path}", flush=True)

    # layer membership map
    file_layer_map = {it["rel"]: it["layers"] for it in targets}

    # baseline.md
    baseline_path = out_dir / "baseline.md"
    write_baseline(baseline_path, rows, args, lo_fail, file_layer_map, out_dir)
    print(f"基線 → {baseline_path}", flush=True)
    # 新值即後續 --check 的預設基線來源（write_baseline 已先對舊 prev 做 diff）。
    prev_path = out_dir / "scores.prev.jsonl"
    shutil.copyfile(scores_path, prev_path)
    print(f"預設基線 → {prev_path}", flush=True)
    return 0


def build_layer_rows(rows, file_layer_map):
    out = {}
    for ln in LAYER_NAMES:
        members = [r for r in rows if (file_layer_map.get(r["file"], {}).get(ln, False))]
        # 整檔量測不可比者不入分層平均（也不計入 n）。
        members = [r for r in members if file_score(r) is not None]
        if not members:
            out[ln] = {"n": 0, "content": 0.0, "layout": 0.0, "page_match_rate": 0.0}
            continue
        out[ln] = {
            "n": len(members),
            "content": sum(file_score(r) for r in members) / len(members),
            "layout": sum(layout_score(r) for r in members) / len(members),
            "page_match_rate": sum(1 for r in members if r.get("page_count_match")) / len(members),
        }
    return out


def write_baseline(path, rows, args, lo_fail, file_layer_map, out_dir):
    layer_rows = build_layer_rows(rows, file_layer_map)
    scored_rows = [r for r in rows if file_score(r) is not None]
    artifacts = [r for r in rows if is_measurement_artifact(r)]
    overall_content = (sum(file_score(r) for r in scored_rows) / len(scored_rows)) if scored_rows else 0.0
    overall_layout = (sum(layout_score(r) for r in scored_rows) / len(scored_rows)) if scored_rows else 0.0
    pmr = (sum(1 for r in rows if r.get("page_count_match")) / len(rows)) if rows else 0.0
    page_weighted_content = None
    all_cs = [s for r in rows for s in _scored(r.get("content_scores"))]
    if all_cs:
        page_weighted_content = sum(all_cs) / len(all_cs)

    lines = []
    lines.append("# LO 收斂追蹤基線（lo_scoreboard）")
    lines.append("")
    lines.append(f"- 生成時間：{datetime.datetime.now().isoformat(timespec='seconds')}")
    lines.append(f"- seed：{args.seed}；n={len(rows)}；LO 轉換失敗：{lo_fail}")
    lines.append(f"- 指標：每檔內容分 = 該檔各頁 content_scores 有效值平均（None 頁跳過）；"
                 f"分層平均 = 層內檔案等權平均。")
    lines.append(f"- 量測不可比（整檔全 None）：{len(artifacts)} 檔，"
                 f"不入總平均／分層平均／最差榜。")
    lines.append(f"- 總平均（檔案等權）內容：{overall_content:.3f}；版面：{overall_layout:.3f}；頁數一致率：{pmr:.1%}")
    if page_weighted_content is not None:
        lines.append(f"- 總平均（頁等權）內容：{page_weighted_content:.3f}（{len(all_cs)} 頁）")
    lines.append("")
    lines.append("## 分層平均")
    lines.append("")
    lines.append("| 層 | n | 內容平均 | 版面平均 | 頁數一致率 |")
    lines.append("|---|---:|---:|---:|---:|")
    for ln in LAYER_NAMES:
        lr = layer_rows[ln]
        lines.append(f"| {LAYER_LABELS[ln]} | {lr['n']} | {lr['content']:.3f} | {lr['layout']:.3f} | {lr['page_match_rate']:.1%} |")
    lines.append("")

    # 量測不可比清單（整檔全 None）：獨立成表，不混進最差榜。
    lines.append("## 量測不可比清單（measurement_artifact）")
    lines.append("")
    if artifacts:
        lines.append("| 檔案 | 頁數（LO/OW） | LO 圖形實據 |")
        lines.append("|---|---|---|")
        for r in artifacts:
            lines.append(f"| {r['file']} | {r.get('lo_pages', '—')}/{r.get('ow_pages', '—')} | "
                         f"{artifact_evidence(r)} |")
    else:
        lines.append("（無）")
    lines.append("")

    # worst 10（排除整檔量測不可比者）
    worst = sorted(scored_rows, key=lambda r: (file_score(r), r.get("file", "")))[:10]
    lines.append("## 最差 10 檔")
    lines.append("")
    lines.append("| # | 檔案 | 內容分 | 首個內容分歧頁 | 備註 |")
    lines.append("|---:|---|---:|---|---|")
    for i, r in enumerate(worst, 1):
        div = "—" if r.get("first_divergence") is None else f"第 {r['first_divergence'] + 1} 頁"
        note = "; ".join(r.get("notes") or []) or "—"
        lines.append(f"| {i} | {r['file']} | {file_score(r):.3f} | {div} | {note} |")
    lines.append("")

    # diff vs prev
    prev_path = out_dir / "scores.prev.jsonl"
    if prev_path.exists():
        prev_rows = [json.loads(l) for l in prev_path.read_text(encoding="utf-8").splitlines() if l.strip()]
        prev_map = {r["file"]: r for r in prev_rows}
        deltas = []
        for r in rows:
            pr = prev_map.get(r["file"])
            if pr is None:
                continue
            cur_s, prev_s = file_score(r), file_score(pr)
            if cur_s is None or prev_s is None:
                continue  # 量測不可比（None）不参与 diff
            deltas.append((r["file"], cur_s - prev_s))
        improved = sorted([d for d in deltas if d[1] > 0.009], key=lambda x: -x[1])
        regressed = sorted([d for d in deltas if d[1] < -0.009], key=lambda x: x[1])
        lines.append("## 與上版基線 diff")
        lines.append("")
        lines.append(f"- 上版檔案數：{len(prev_rows)}；共同檔案：{len(deltas)}")
        lines.append(f"- 改善 {len(improved)} 檔、劣化 {len(regressed)} 檔")
        lines.append("")
        if improved:
            lines.append("### 改善榜（top 10）")
            for f, d in improved[:10]:
                lines.append(f"- {f}: {d:+.3f}")
            lines.append("")
        if regressed:
            lines.append("### 劣化榜（top 10）")
            for f, d in regressed[:10]:
                lines.append(f"- {f}: {d:+.3f}")
            lines.append("")
        # layer deltas
        prev_layer_map = {}
        # rebuild layer map for prev from file_layer_map (same)
        prev_layer_rows = build_layer_rows(prev_rows, file_layer_map)
        lines.append("### 分層平均變化")
        lines.append("")
        lines.append("| 層 | 上版 | 本版 | Δ |")
        lines.append("|---|---:|---:|---:|")
        for ln in LAYER_NAMES:
            a = prev_layer_rows[ln]["content"]
            b = layer_rows[ln]["content"]
            lines.append(f"| {LAYER_LABELS[ln]} | {a:.3f} | {b:.3f} | {b - a:+.3f} |")
        lines.append("")
    else:
        lines.append("## 與上版基線 diff")
        lines.append("")
        lines.append("（無 scores.prev.jsonl，跳過）")
        lines.append("")

    path.write_text("\n".join(lines), encoding="utf-8")


def load_scores(path):
    rows = []
    if not path.exists():
        return rows
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def file_layers_for_path(rel, corpus_dir):
    # find corpus file
    p = None
    # rel like corpus/...
    cand = REPO / rel
    if cand.exists():
        p = cand
    else:
        cand2 = corpus_dir / rel
        if cand2.exists():
            p = cand2
        else:
            # maybe basename search
            for root in (corpus_dir,):
                for q in root.rglob(pathlib.Path(rel).name):
                    p = q
                    break
    if p is None or not p.exists():
        return {ln: False for ln in LAYER_NAMES}
    # find manifest entry
    manifest = json.loads((corpus_dir / "manifest.json").read_text(encoding="utf-8"))
    entry = None
    for f in manifest["files"]:
        if f["file"] == rel or str(corpus_dir / f["file"]) == str(p):
            entry = f
            break
    if entry is None:
        # try by relative path from corpus
        try:
            relc = str(p.relative_to(corpus_dir))
            for f in manifest["files"]:
                if f["file"] == relc:
                    entry = f
                    break
        except Exception:
            pass
    if entry is None:
        return {ln: False for ln in LAYER_NAMES}
    sc = sect_count(p)
    return layers_for(entry, sc)


def check_mode(args):
    out_dir = pathlib.Path(args.out)
    scores_path = pathlib.Path(args.scores) if getattr(args, "scores", None) else out_dir / "scores.jsonl"
    baseline_path = pathlib.Path(args.check) if args.check else None
    if baseline_path is None:
        baseline_path = out_dir / "scores.prev.jsonl"
    cur = load_scores(scores_path)
    base = load_scores(baseline_path)
    if not cur:
        print(f"找不到/空 scores：{scores_path}")
        return 2
    if not base:
        print(f"找不到/空 baseline：{baseline_path}")
        return 2

    corpus_dir = pathlib.Path(args.corpus)

    def layer_map_for(rows):
        m = {}
        for r in rows:
            m[r["file"]] = file_layers_for_path(r["file"], corpus_dir)
        return m

    cur_lm = layer_map_for(cur)
    base_lm = layer_map_for(base)
    cur_lr = build_layer_rows(cur, cur_lm)
    base_lr = build_layer_rows(base, base_lm)

    fails = []
    print("== 分層平均檢查 ==")
    for ln in LAYER_NAMES:
        if base_lr[ln]["n"] == 0:
            continue
        if cur_lr[ln]["n"] == 0:
            fails.append(f"{LAYER_LABELS[ln]}：本版 n=0")
            print(f"{LAYER_LABELS[ln]}: base n={base_lr[ln]['n']} cur n=0 -> FAIL")
            continue
        delta = cur_lr[ln]["content"] - base_lr[ln]["content"]
        status = "OK" if delta >= -0.02 else "FAIL"
        print(f"{LAYER_LABELS[ln]}: base={base_lr[ln]['content']:.3f} cur={cur_lr[ln]['content']:.3f} Δ={delta:+.3f} {status}")
        if delta < -0.02:
            fails.append(f"{LAYER_LABELS[ln]} 分層平均降 {abs(delta):.3f} > 0.02")

    # individual regression（None＝量測不可比，兩邊任一 None 即跳過；None-safe）
    base_map = {r["file"]: r for r in base}
    regressed = []
    for r in cur:
        br = base_map.get(r["file"])
        if br is None:
            continue
        cur_s, base_s = file_score(r), file_score(br)
        if cur_s is None or base_s is None:
            continue
        if cur_s < base_s - 1e-12:
            regressed.append((r, br, cur_s, base_s))
            fails.append(f"{r['file']} 個別內容分劣化 {base_s:.3f} → {cur_s:.3f}")
    # new worst files（量測不可比檔不入榜）
    def worst_set(rows, k=10):
        scored = [r for r in rows if file_score(r) is not None]
        return set(r["file"] for r in sorted(scored, key=lambda x: (file_score(x), x.get("file", "")))[:k])
    base_worst = worst_set(base, 10)
    cur_worst = worst_set(cur, 10)
    new_worst = cur_worst - base_worst
    if new_worst:
        for f in new_worst:
            fails.append(f"最差榜新增惡化檔：{f}")
    print("")
    print(f"個別劣化檔數：{len(regressed)}")
    print(f"最差榜新增檔：{len(new_worst)}")
    if fails:
        print("\n--check 失敗：")
        for f in fails:
            print(f"  - {f}")
        return 1
    print("\n--check 通過")
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="*", help="直接指定 docx（不使用）")
    ap.add_argument("--check", nargs="?", const="", default=None,
                    help="檢查模式：可選基線路徑（預設 <out>/scores.prev.jsonl）")
    ap.add_argument("--corpus", default=str(REPO / "corpus"))
    ap.add_argument("--n", type=int, default=25)
    ap.add_argument("--seed", type=int, default=20261022)
    ap.add_argument("--out", default=str(REPO / "out" / "lo-scoreboard"))
    ap.add_argument("--scores", default=None, help="scores.jsonl 路徑（check 用）")
    a = ap.parse_args()

    if a.check is not None:
        # if --check given with empty const, use default baseline
        if a.check == "":
            a.check = None
        return check_mode(a)

    return run_pipeline(a)


if __name__ == "__main__":
    sys.exit(main())
