#!/usr/bin/env python3
"""render-lab —— docx 跨引擎渲染比對器（Phase0-2 骨架）。

流程（每檔）：
  1. LibreOffice headless convert docx→PDF（基線引擎，LO 崩潰記 lo_fail 不中断）
  2. pymupdf 拆 PDF→每頁文字/幾何統計（LO 側 ground truth）
  3. quire-cli render 產我方每頁 SVG＋pagebreaks.json
  4. 逐頁比對：頁數、每頁首行/末行文字、行數 → 差異報告（markdown）

用法：
  python3 scripts/render_lab.py [--corpus corpus --n 10] [--out out/render-lab]
  # 或直接指定檔案：python3 scripts/render_lab.py file1.docx file2.docx

soffice 自動偵測：Linux soffice/libreoffice，或 WSL 下的 Windows soffice.exe。
"""

import argparse
import datetime
import difflib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import pathlib

try:
    import pymupdf
except ImportError:
    sys.exit("需要 pymupdf：python3 -m pip install --user pymupdf")

REPO = pathlib.Path(__file__).resolve().parent.parent
LINE_RE = re.compile(r"\s+")


def norm(s: str) -> str:
    return LINE_RE.sub(" ", s).strip()


def find_soffice() -> pathlib.Path:
    for name in ("soffice", "libreoffice"):
        p = shutil.which(name)
        if p:
            return pathlib.Path(p)
    # WSL：Windows 安裝位置
    for cand in (
        "/mnt/c/Program Files/LibreOffice/program/soffice.exe",
        "/mnt/c/Program Files (x86)/LibreOffice/program/soffice.exe",
    ):
        p = pathlib.Path(cand)
        if p.exists():
            return p
    sys.exit("找不到 LibreOffice（soffice）。請安裝 libreoffice-writer 或 Windows 版 LibreOffice。")


def win_scratch_dir() -> pathlib.Path:
    """Windows 本機暫存目錄（.exe 版 LO 走 \\wsl.localhost 讀寫易掛，改走 C: 本機盤）。"""
    env = os.environ.get("TEMP_WIN")
    cands: list[pathlib.Path] = []
    if env:
        cands.append(pathlib.Path(env))
    else:
        try:
            r = subprocess.run(["cmd.exe", "/c", "echo %TEMP%"], capture_output=True, timeout=15)
            w = r.stdout.decode("utf-8", errors="ignore").strip().strip('"') if r.returncode == 0 else ""
        except (OSError, subprocess.TimeoutExpired):
            w = ""
        if w:
            rr = subprocess.run(["wslpath", "-u", w], capture_output=True, text=True)
            if rr.returncode == 0:
                cands.append(pathlib.Path(rr.stdout.strip()))
        cands += sorted(pathlib.Path("/mnt/c/Users").glob("*/AppData/Local/Temp"))
    for base in cands:
        d = base / "quire-render-lab"
        try:
            d.mkdir(parents=True, exist_ok=True)
            probe = d / ".write-probe"
            probe.write_text("x")
            probe.unlink()
            return d
        except OSError:
            continue
    sys.exit(f"Windows 暫存目錄不可寫（試過：{[str(c) for c in cands]}）；可用 TEMP_WIN 指定")


def lo_scratch(soffice: pathlib.Path) -> tuple[pathlib.Path, str]:
    """回傳 (LO 讀寫用目錄, UserInstallation URI)。Windows exe → C: 本機暫存。"""
    if str(soffice).endswith(".exe"):
        d = win_scratch_dir()
        prof = d / "profile"
        prof.mkdir(exist_ok=True)
        r = subprocess.run(["wslpath", "-w", str(prof)], capture_output=True, text=True)
        win = r.stdout.strip().replace("\\", "/") if r.returncode == 0 else str(prof)
        return d, "file:///" + win.lstrip("/")
    d = pathlib.Path(tempfile.mkdtemp(prefix="quire-lo-"))
    return d, d.absolute().as_uri()


def kill_stale_lo() -> None:
    """清掉上一輪殘留的 headless soffice（不然 Windows 版會委派給殭屍實例掛死）。
    只用温和 taskkill（無 /F）；使用者自己開的 LO 視窗不會被硬砍。"""
    if shutil.which("taskkill.exe"):
        subprocess.run(["taskkill.exe", "/IM", "soffice.bin.exe"], capture_output=True)
        subprocess.run(["taskkill.exe", "/IM", "soffice.exe"], capture_output=True)
        time.sleep(1.5)


def lo_to_pdf(soffice: pathlib.Path, docx: pathlib.Path, scratch: pathlib.Path,
              profile_uri: str, timeout: int = 120) -> pathlib.Path | None:
    """單檔 LO 轉 PDF；崩潰/逾時回 None（由呼叫端記 lo_fail）。
    Windows exe 版：先把 docx 複製到 C: 本機暫存，再以 **Windows 路徑** 餵給 soffice.exe
    （它看不懂 /mnt/c/...；之前整個 corpus 全 fail 的根因）。"""
    src = docx
    out_arg, src_arg = scratch, src
    if str(soffice).endswith(".exe"):
        # corpus 各來源目錄可能同名：暫存檔名帶父目錄避免互相覆寫。
        src = scratch / f"{docx.parent.name}__{docx.name}"
        shutil.copyfile(docx, src)
        out_arg = pathlib.Path(_w2w(scratch))
        src_arg = pathlib.Path(_w2w(src))
    cmd = [str(soffice), f"-env:UserInstallation={profile_uri}",
           "--headless", "--norestore", "--convert-to", "pdf",
           "--outdir", str(out_arg), str(src_arg)]
    stale = scratch / (src.stem + ".pdf")
    if stale.exists():
        stale.unlink()  # 上一輪殘留 PDF 會被誤判成成功
    try:
        subprocess.run(cmd, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return None
    return stale if stale.exists() else None


def _w2w(linux_path: pathlib.Path) -> str:
    """/mnt/c/... → C:\\... （給 Windows 版 soffice 的引數）。"""
    r = subprocess.run(["wslpath", "-w", str(linux_path)], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"wslpath 轉換失敗（{linux_path}）：{r.stderr.strip()}")
    return r.stdout.strip()


def pdf_page_facts(pdf: pathlib.Path) -> list[dict]:
    """PDF 每頁：文字行（幾何順序）、行數、頁尺寸（pt）、文字 span 數。"""
    d = pymupdf.open(pdf)
    pages = []
    for p in d:
        lines = [norm(t) for t in str(p.get_text("text")).splitlines() if norm(t)]
        words = len(p.get_text("words"))
        pages.append({
            "lines": lines,
            "line_count": len(lines),
            "words": words,
            "w_pt": round(p.rect.width, 1),
            "h_pt": round(p.rect.height, 1),
        })
    d.close()
    return pages


def rasterize(pdf: pathlib.Path, png_dir: pathlib.Path, stem: str, max_pages: int = 4) -> int:
    """PDF 每頁 → PNG（150dpi，限前 max_pages 頁），供人眼跨引擎對照。"""
    png_dir.mkdir(parents=True, exist_ok=True)
    d = pymupdf.open(pdf)
    n = min(d.page_count, max_pages)
    for i in range(n):
        pix = d[i].get_pixmap(matrix=pymupdf.Matrix(150 / 72, 150 / 72))
        pix.save(png_dir / f"{stem}-p{i}.png")
    d.close()
    return n


def run_quire(docx: pathlib.Path, out_dir: pathlib.Path) -> dict:
    render_dir = out_dir / "quire" / docx.stem
    r = subprocess.run(
        ["cargo", "run", "-q", "-p", "quire-cli", "--",
         "render", str(docx), "--out", str(render_dir)],
        capture_output=True, text=True, cwd=REPO, timeout=180)
    if r.returncode != 0:
        raise RuntimeError(f"quire render 失敗: {r.stderr.strip()[:300]}")
    brks = json.loads((render_dir / "pagebreaks.json").read_text(encoding="utf-8"))
    # 我方每頁文字：由 dump 顯示列表取（與 SVG 同源）。
    d = subprocess.run(
        ["cargo", "run", "-q", "-p", "quire-cli", "--", "dump", str(docx)],
        capture_output=True, text=True, cwd=REPO, timeout=180)
    d.check_returncode()
    pages = []
    for dl in json.loads(d.stdout):
        lines = [norm(it["Text"]["text"]) for it in dl["items"] if "Text" in it]
        lines = [l for l in lines if l]
        pages.append({
            "lines": lines,
            "line_count": len(lines),
            "svg": f"quire/{docx.stem}/{docx.stem}-p{dl['page']}.svg",
        })
    return {"pages": pages, "breaks": brks, "n_svg": len(pages)}


def compare(name: str, lo: list[dict], ow: dict) -> dict:
    """逐頁比對 LO vs quire，回傳該檔的差異摘要。"""
    lo_pages, ow_pages = lo, ow["pages"]
    row = {
        "file": name,
        "lo_pages": len(lo_pages),
        "ow_pages": len(ow_pages),
        "page_count_match": len(lo_pages) == len(ow_pages),
        "page_scores": [],       # 逐頁相似度 0..1
        "first_divergence": None,
        "notes": [],
    }
    n = max(len(lo_pages), len(ow_pages))
    for i in range(n):
        a = " | ".join(lo_pages[i]["lines"][:8]) if i < len(lo_pages) else ""
        b = " | ".join(ow_pages[i]["lines"][:8]) if i < len(ow_pages) else ""
        score = difflib.SequenceMatcher(None, a, b).ratio() if (a or b) else 0.0
        row["page_scores"].append(round(score, 3))
        if score < 0.6 and row["first_divergence"] is None:
            row["first_divergence"] = i
    # 逐頁尺寸對照（Phase0-2b）：我方每頁尺寸如實取自檔案 sectPr（pagebreaks.json
    # 的 width_pt/height_pt），與 LO 逐頁比——只有真的不同才備註，不再拿固定
    # A4 當尺（舊備註「LO 頁尺寸 ≠ 我方固定 A4」自此退役）。
    brks = ow.get("breaks") or []
    for i, p in enumerate(lo_pages):
        if i >= len(brks):
            row["notes"].append(f"第 {i + 1} 頁我方無 pagebreak 記錄")
            continue
        ow_w, ow_h = round(brks[i]["width_pt"], 1), round(brks[i]["height_pt"], 1)
        if abs(p["w_pt"] - ow_w) > 2.0 or abs(p["h_pt"] - ow_h) > 2.0:
            row["notes"].append(
                f"第 {i + 1} 頁 LO {p['w_pt']}×{p['h_pt']} ≠ 我方 {ow_w}×{ow_h}（sectPr）"
            )
    return row


def render_report(rows: list[dict], meta: dict, out_md: pathlib.Path) -> None:
    ok = sum(1 for r in rows if r.get("status", "ok") == "ok" and r["page_count_match"])
    lines = [
        "# render-lab 差異報告（LO 基線 vs quire）",
        "",
        f"- 生成時間：{meta['ts']}",
        f"- LibreOffice：`{meta['soffice']}`",
        f"- 對象：{meta['target']}（{len(rows)} 檔）",
        f"- 頁數一致：{ok}/{len(rows)}；LO 轉換失敗：{meta['lo_fail']}",
        "",
        "| 檔案 | LO 頁 | OW 頁 | 頁數 | 逐頁相似度（首 6 頁） | 首個分歧頁 | 備註 |",
        "|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        if r.get("status") == "lo_fail":
            lines.append(f"| {r['file']} | — | — | — | — | — | LO 轉換失敗/逾時 |")
            continue
        scores = ", ".join(str(s) for s in r["page_scores"][:6])
        div = "—" if r["first_divergence"] is None else f"第 {r['first_divergence'] + 1} 頁"
        note = "; ".join(r["notes"]) or "—"
        lines.append(
            f"| {r['file']} | {r['lo_pages']} | {r['ow_pages']} | "
            f"{'✓' if r['page_count_match'] else '✗'} | {scores} | {div} | {note} |")
    lines += [
        "",
        "說明：相似度＝每頁前 8 行文字序列的 SequenceMatcher 值（0=完全不同，1=一致）。",
        "骨架階段我方無字型 shaping、無表格/頁首頁尾，分歧是預期；本報告追蹤的是",
        "**頁數收斂速度**與**斷點位置**，作為 Phase 0 排版演進的量化基線。",
        "",
    ]
    out_md.write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="*", help="直接指定的 docx（預設取 corpus manifest 前 n 檔）")
    ap.add_argument("--corpus", default=str(REPO / "corpus"))
    ap.add_argument("--n", type=int, default=10)
    ap.add_argument("--out", default=str(REPO / "out" / "render-lab"))
    ap.add_argument("--report", default=None, help="報告輸出路徑（預設 <out>/report.md）")
    a = ap.parse_args()

    out_dir = pathlib.Path(a.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    if a.files:
        targets = [pathlib.Path(f) for f in a.files]
        target_desc = f"自訂 {len(targets)} 檔"
    else:
        manifest = json.loads((pathlib.Path(a.corpus) / "manifest.json").read_text(encoding="utf-8"))
        targets = [pathlib.Path(a.corpus) / f["file"] for f in manifest["files"][: a.n]]
        target_desc = f"corpus manifest 前 {len(targets)} 檔"
    missing = [t for t in targets if not t.exists()]
    if missing:
        sys.exit(f"找不到檔案：{missing[:3]} ...")

    soffice = find_soffice()
    scratch, profile_uri = lo_scratch(soffice)
    kill_stale_lo()
    rows, lo_fail = [], 0
    for docx in targets:
        rel = str(docx.relative_to(REPO)) if docx.is_relative_to(REPO) else str(docx)
        print(f"→ {rel}", flush=True)
        pdf = lo_to_pdf(soffice, docx, scratch, profile_uri)
        if pdf is None:
            lo_fail += 1
            rows.append({"file": rel, "status": "lo_fail"})
            print(f"   ✗ LO 轉換失敗（記 lo_fail，繼續）")
            continue
        try:
            lo = pdf_page_facts(pdf)
            ow = run_quire(docx, out_dir)
            stem = docx.stem
            rasterize(pdf, out_dir / "lo-png", stem)
            for svg in sorted((out_dir / "quire" / stem).glob(f"{stem}-p*.svg"))[:4]:
                sd = pymupdf.open(svg)
                pix = sd[0].get_pixmap(matrix=pymupdf.Matrix(150 / 72, 150 / 72))
                (out_dir / "ow-png").mkdir(exist_ok=True)
                pix.save(out_dir / "ow-png" / (svg.stem + ".png"))
                sd.close()
        except Exception as e:
            print(f"   ✗ 比對異常：{e}")
            rows.append({"file": rel, "status": "lo_fail", "error": str(e)})
            lo_fail += 1
            continue
        rows.append(compare(rel, lo, ow))
        r = rows[-1]
        print(f"   LO {r['lo_pages']} 頁 / OW {r['ow_pages']} 頁，"
              f"逐頁相似度 {r['page_scores'][:4]}")

    meta = {"ts": datetime.datetime.now().isoformat(timespec="seconds"),
            "soffice": str(soffice), "target": target_desc, "lo_fail": lo_fail}
    report = pathlib.Path(a.report) if a.report else out_dir / "report.md"
    render_report(rows, meta, report)
    print(f"\n報告 → {report}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
