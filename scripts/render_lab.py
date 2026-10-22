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


def _bullet_slot(line: str) -> str:
    """整行若只是清單符號（LO 常吐私有区 \uf0b7／o／•／◦，我方吐 o／•／無），
    正規化成統一佔位 `[B]`——符號字型差異不該污染**內容**相似度（J2）。
    只處理「整行即符號」的情形，避免吃掉內文中的字母 o。"""
    t = line.strip()
    if t in ("\uf0b7", "\u2022", "\u25e6", "o", "O", "◦", "", "[B]"):
        return "[B]"
    return line


def content_key(lines: list[str]) -> str:
    """整頁內容鍵（J2）：正規化＋符號佔位＋casefold＋忽略換行位置。
    這是「內容是否一致」的口徑——換行/斷行位置差異不計入。"""
    toks = [t for l in lines if (t := _bullet_slot(norm(l))) not in ("", None)]
    key = " ".join(toks).casefold()
    # SequenceMatcher 對超長字串退化為 O(n²)：截斷保護（單頁 6000 字元遠夠）。
    return key[:6000]


def layout_key(lines: list[str]) -> str:
    """版面鍵：保留「前 8 行逐行拼接」的舊口徑（含換行位置）。"""
    return " | ".join(norm(l) for l in lines[:8])


def compare(name: str, lo: list[dict], ow: dict) -> dict:
    """逐頁比對 LO vs quire，回傳該檔的差異摘要。

    J2 校正：**內容相似度**（忽略換行、大小寫不敏感、符號佔位）與
    **版面相似度**（含換行位置）分成兩個獨立指標。舊版只有逐行序列比，
    把「同一句話換行位置不同」誤算成內容分歧（HC031 p1 實測兩邊文字
    幾乎相同卻得 0.007），會誤導所有 A/B 判讀。
    「首個分歧頁」判定改用 content_scores（<0.6）；換行差只進 notes。
    """
    lo_pages, ow_pages = lo, ow["pages"]
    row = {
        "file": name,
        "lo_pages": len(lo_pages),
        "ow_pages": len(ow_pages),
        "page_count_match": len(lo_pages) == len(ow_pages),
        "content_scores": [],   # 內容相似度 0..1（忽略換行；分歧判定用這個）
        "layout_scores": [],    # 版面相似度 0..1（含換行位置；僅供觀察）
        "first_divergence": None,
        "notes": [],
    }
    n = max(len(lo_pages), len(ow_pages))
    for i in range(n):
        a_lines = lo_pages[i]["lines"] if i < len(lo_pages) else []
        b_lines = ow_pages[i]["lines"] if i < len(ow_pages) else []
        ca, cb = content_key(a_lines), content_key(b_lines)
        la, lb = layout_key(a_lines), layout_key(b_lines)
        # autojunk=False：SequenceMatcher 預設對長字串把常見 2-char 塊當「垃圾」
        # 剔除，頁面文字一長就崩（HC031 p1 實測 legacy=0.008、關掉=0.655）。
        # 這是 0.007 假分歧的真正元兇，比換行/大小寫都是主因。
        cs = difflib.SequenceMatcher(None, ca, cb, autojunk=False).ratio() if (ca or cb) else 0.0
        ls = difflib.SequenceMatcher(None, la, lb, autojunk=False).ratio() if (la or lb) else 0.0
        row["content_scores"].append(round(cs, 3))
        row["layout_scores"].append(round(ls, 3))
        if cs < 0.6 and row["first_divergence"] is None:
            row["first_divergence"] = i
        # 內容近同但版面分數低 ⇒ 主要是換行位置差，記 notes 不記分歧。
        if cs >= 0.85 and ls < 0.5:
            row["notes"].append(f"第 {i + 1} 頁內容近同、僅換行位置不同")
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
        "| 檔案 | LO 頁 | OW 頁 | 頁數 | 內容相似度（首 6 頁） | 版面相似度（首 6 頁） | 首個內容分歧頁 | 備註 |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        if r.get("status") == "lo_fail":
            lines.append(f"| {r['file']} | — | — | — | — | — | — | LO 轉換失敗/逾時 |")
            continue
        cs = ", ".join(str(s) for s in r["content_scores"][:6])
        ls = ", ".join(str(s) for s in r["layout_scores"][:6])
        div = "—" if r["first_divergence"] is None else f"第 {r['first_divergence'] + 1} 頁"
        note = "; ".join(r["notes"]) or "—"
        lines.append(
            f"| {r['file']} | {r['lo_pages']} | {r['ow_pages']} | "
            f"{'✓' if r['page_count_match'] else '✗'} | {cs} | {ls} | {div} | {note} |")
    lines += [
        "",
        "說明（J2 校正）：**內容相似度**＝整頁全部行正規化後拼成單一詞序列、",
        "casefold、清單符號正規化成統一佔位後跑 SequenceMatcher——忽略換行位置，",
        "反映「內容是否一致」；**版面相似度**＝舊的逐行（前 8 行）序列比，含換行",
        "位置，反映斷行/換行差異。「首個內容分歧頁」以內容分 <0.6 判定；內容近同",
        "而版面分低只進備註（僅換行位置不同），不記分歧。",
        "本報告追蹤的是**頁數收斂速度**與**斷點位置**，作為排版演進的量化基線。",
        "",
    ]
    out_md.write_text("\n".join(lines), encoding="utf-8")


def selftest() -> int:
    """J2 自測（驗收 4）：內容/版面兩指標必須真正分離，且分歧判定看內容分。
    先紅後綠：舊實作只有一個逐行分，斷言 1/2 直接紅。"""
    fails = 0

    def check(name: str, cond: bool, detail: str = "") -> None:
        nonlocal fails
        print(f"  {'PASS' if cond else 'FAIL'} {name}" + (f"（{detail}）" if detail else ""))
        if not cond:
            fails += 1

    # (1) 僅換行位置不同、詞序相同 ⇒ content 恒等 1.0（完全換行無關），
    #     layout 因 "|" 斷點位置不同而 <1.0（換行可见）。兩指標分離的判準是
    #     「content 完全不受換行影響」，不是「layout 必須很低」。
    lo = {"lines": ["The quick brown fox jumps over", "the lazy dog and keeps running far",
                    "away from the farm every single morning today and every morning"],
          "w_pt": 612.0, "h_pt": 792.0}
    ow = {"pages": [{"lines": ["The quick brown fox", "jumps over the lazy dog and keeps",
                               "running far away from the farm every single morning today and every morning"]}],
          "breaks": [{"width_pt": 612.0, "height_pt": 792.0}]}
    r = compare("t1", [lo], ow)
    check("1 換行位置差不算內容分歧",
          r["content_scores"][0] >= 0.999 and r["layout_scores"][0] < 0.999,
          f"content={r['content_scores'][0]} layout={r['layout_scores'][0]}")
    check("1b 該頁不記 first_divergence", r["first_divergence"] is None)

    # (2) HEADING1 vs Heading1：大小寫不掉內容分。
    lo2 = {"lines": ["HEADING1", "Video provides a powerful way to help you prove your point."],
           "w_pt": 612.0, "h_pt": 792.0}
    ow2 = {"pages": [{"lines": ["Heading1", "Video provides a powerful way to help you prove",
                                "your point."]}],
           "breaks": [{"width_pt": 612.0, "height_pt": 792.0}]}
    r2 = compare("t2", [lo2], ow2)
    check("2 大小寫不敏感", r2["content_scores"][0] >= 0.99,
          f"content={r2['content_scores'][0]}")

    # (3) 真內容缺失（我方少一整段**互異**文字）⇒ content <0.6 且 first_divergence
    #     命中。注意：填充文字必須互異，否則 SequenceMatcher 對高重複字串會到处
    #     匹配、掩蓋缺失（實測同句×4 得 0.679 假高分）。
    segs = [
        "Alpha bravo charlie delta echo foxtrot golf hotel india juliett kilo lima mike. ",
        "November oscar papa quebec romeo sierra tango uniform victor whiskey xray yankee. ",
        "Zulu one two three four five six seven eight nine ten eleven twelve thirteen. ",
        "Fourteen fifteen sixteen seventeen eighteen nineteen twenty twentyone twentytwo. ",
    ]
    lo3 = {"lines": segs, "w_pt": 612.0, "h_pt": 792.0}
    ow3 = {"pages": [{"lines": segs[:1]}],
           "breaks": [{"width_pt": 612.0, "height_pt": 792.0}]}
    r3 = compare("t3", [lo3], ow3)
    check("3 真缺段（互異填充）內容分如實掉",
          r3["content_scores"][0] < 0.6 and r3["first_divergence"] == 0,
          f"content={r3['content_scores'][0]} div={r3['first_divergence']}")

    # (3b) 清單符號（LO \uf0b7/o vs 我方無符號）不污染內容分。
    lo4 = {"lines": ["\uf0b7", "To make your document look professionally produced Word",
                     "provides header footer and cover page designs."],
           "w_pt": 612.0, "h_pt": 792.0}
    ow4 = {"pages": [{"lines": ["To make your document look professionally produced Word",
                                "provides header footer and cover page designs."]}],
           "breaks": [{"width_pt": 612.0, "height_pt": 792.0}]}
    r4 = compare("t4", [lo4], ow4)
    check("3b 符號佔位不分歧", r4["content_scores"][0] >= 0.95,
          f"content={r4['content_scores'][0]}")

    print("SELFTEST", "ALL-PASS" if fails == 0 else f"{fails}-FAIL")
    return 0 if fails == 0 else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="*", help="直接指定的 docx（預設取 corpus manifest 前 n 檔）")
    ap.add_argument("--selftest", action="store_true",
                    help="跑 J2 度量自測（不碰 LO/引擎，純函式驗證）後退出")
    ap.add_argument("--corpus", default=str(REPO / "corpus"))
    ap.add_argument("--n", type=int, default=10)
    ap.add_argument("--out", default=str(REPO / "out" / "render-lab"))
    ap.add_argument("--report", default=None, help="報告輸出路徑（預設 <out>/report.md）")
    a = ap.parse_args()

    if a.selftest:
        return selftest()

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
              f"內容相似度 {r['content_scores'][:4]}（版面 {r['layout_scores'][:4]}）")

    meta = {"ts": datetime.datetime.now().isoformat(timespec="seconds"),
            "soffice": str(soffice), "target": target_desc, "lo_fail": lo_fail}
    report = pathlib.Path(a.report) if a.report else out_dir / "report.md"
    render_report(rows, meta, report)
    print(f"\n報告 → {report}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
