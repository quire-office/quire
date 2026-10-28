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
import atexit
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
from collections import Counter

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


def lo_scratch(soffice: pathlib.Path) -> tuple[pathlib.Path, str, str]:
    """回傳 (LO 讀寫用目錄, UserInstallation URI, kill 判別用 path 字串)。
    Windows exe → C: 本機暫存；第三項是 soffice 命令列實際帶的路徑形式（供
    kill_own_lo 精準判別本輪自己的實例）。"""
    if str(soffice).endswith(".exe"):
        d = win_scratch_dir()
        prof = d / "profile"
        prof.mkdir(exist_ok=True)
        r = subprocess.run(["wslpath", "-w", str(prof)], capture_output=True, text=True)
        win = r.stdout.strip().replace("\\", "/") if r.returncode == 0 else str(prof)
        # soffice 命令列帶的是這個 Windows 形式路徑（正斜線版）＝判別錨點。
        return d, "file:///" + win.lstrip("/"), win
    d = pathlib.Path(tempfile.mkdtemp(prefix="quire-lo-"))
    return d, d.absolute().as_uri(), str(d)


def kill_stale_lo() -> None:
    """清掉上一輪殘留的 headless soffice（不然 Windows 版會委派給殭屍實例掛死）。
    只用温和 taskkill（無 /F）；使用者自己開的 LO 視窗不會被硬砍。"""
    if shutil.which("taskkill.exe"):
        subprocess.run(["taskkill.exe", "/IM", "soffice.bin.exe"], capture_output=True)
        subprocess.run(["taskkill.exe", "/IM", "soffice.exe"], capture_output=True)
        time.sleep(1.5)


def kill_own_lo(needle: str) -> int:
    """收場用：**只殺本輪自己拉起的** soffice（命令列含本 scratch profile 路徑者）。

    背景（2026-10-28 事故）：`kill_stale_lo` 只在開跑前掃，跑完不清場；Windows 版
    soffice 轉換崩潰後會留下「Press Enter to continue」模態視窗，温和 taskkill 砍不動
    → 兩日累積 142 個 soffice.exe ＋ 71 個 soffice.bin，吃 RAM 連帶把 C 槽 pagefile／
    WSL swap.vhdx 撐爆（用戶 C 槽一度只剩 219GB）。

    判別用 **命令列帶本輪 profile 路徑**＝精準只殺自己的，使用者自己開的 LO 一律不碰。
    回傳殺掉的進程數（非 Windows 環境回 0）。
    """
    if not shutil.which("powershell.exe"):
        return 0
    try:
        # needle＝本 scratch 的 Windows 形式 profile 路徑（含 quire-render-lab 前綴，
        # 使用者自己開的 LO 命令列不會帶這種路徑）。反斜線/正斜線兩種寫法都納。
        alt = needle.replace("/", "\\\\")
        ps = (
            "$ErrorActionPreference='SilentlyContinue';"
            "$p = Get-CimInstance Win32_Process -Filter \"Name='soffice.exe' or Name='soffice.bin.exe'\" |"
            f" Where-Object {{ $_.CommandLine -like '*{needle}*' -or $_.CommandLine -like '*{alt}*' }};"
            "$n = ($p | Measure-Object).Count;"
            "$p | ForEach-Object { Stop-Process -Id $_.ProcessId -Force };"
            "$n"
        )
        r = subprocess.run(["powershell.exe", "-NoProfile", "-Command", ps],
                           capture_output=True, text=True, timeout=120)
        killed = int((r.stdout or "0").strip().splitlines()[-1])
    except Exception:
        return 0
    if killed:
        print(f"（收場：清掉本輪殘留 LibreOffice 進程 {killed} 個）", flush=True)
    return killed


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
    """PDF 每頁：文字行（幾何順序）、行數、頁尺寸（pt）、文字 span 數、圖形實據。"""
    d = pymupdf.open(pdf)
    pages = []
    for p in d:
        lines = [norm(t) for t in str(p.get_text("text")).splitlines() if norm(t)]
        words = len(p.get_text("words"))
        imgs = p.get_images(full=True)
        pages.append({
            "lines": lines,
            "line_count": len(lines),
            "words": words,
            "w_pt": round(p.rect.width, 1),
            "h_pt": round(p.rect.height, 1),
            # 0/0 度量偽影判準：LO 該頁是否真的有非文字內容（向量圖／點陣圖）。
            # image_xrefs 供區分「內文圖」與「逐頁重複的頁首／頁尾圖」。
            "drawings": len(p.get_drawings()),
            "images": len(imgs),
            "image_xrefs": [im[0] for im in imgs],
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


def sect_pr_count(docx: pathlib.Path) -> int | None:
    """源檔 `w:sectPr` 計數（真空頁 notes 實據，§2.5 方法）；讀不到回 None。"""
    try:
        import zipfile
        data = zipfile.ZipFile(docx).read("word/document.xml").decode("utf-8", "ignore")
        return len(re.findall(r"<w:sectPr\b", data))
    except Exception:
        return None


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


def compare(name: str, lo: list[dict], ow: dict, sect_pr: int | None = None) -> dict:
    """逐頁比對 LO vs quire，回傳該檔的差異摘要。

    J2 校正：**內容相似度**（忽略換行、大小寫不敏感、符號佔位）與
    **版面相似度**（含換行位置）分成兩個獨立指標。舊版只有逐行序列比，
    把「同一句話換行位置不同」誤算成內容分歧（HC031 p1 實測兩邊文字
    幾乎相同卻得 0.007），會誤導所有 A/B 判讀。
    「首個分歧頁」判定改用 content_scores（<0.6）；換行差只進 notes。

    M2 校正：同位雙空頁（LO 與我方都有該頁、LO 該頁文字/圖形/點陣圖全無）
    記 None（度量假分差）而非 0.0——判別用頁數對齊上下文；我方多出頁是空
    頁（LO 無此頁）仍 0.0 真事故。`sect_pr`＝源檔 `w:sectPr` 計數（可選，
    真空頁 notes 實據用，讓報告可覆核）。
    """
    lo_pages, ow_pages = lo, ow["pages"]
    row = {
        "file": name,
        "lo_pages": len(lo_pages),
        "ow_pages": len(ow_pages),
        "page_count_match": len(lo_pages) == len(ow_pages),
        "content_scores": [],   # 內容相似度 0..1（忽略換行；分歧判定用這個）；None＝量測不可比
        "layout_scores": [],    # 版面相似度 0..1（含換行位置；僅供觀察）；None＝量測不可比
        "first_divergence": None,
        "notes": [],
        "measurement_artifacts": [],  # 0/0 偽影頁的 LO 圖形實據（逐頁）
    }
    # 跨頁重複出現的圖片＝頁首／頁尾浮水印或裝飾圖（非該頁真正內容）。
    xref_pages = Counter(x for pg in lo_pages for x in (pg.get("image_xrefs") or []))
    n = max(len(lo_pages), len(ow_pages))
    for i in range(n):
        lo_pg = lo_pages[i] if i < len(lo_pages) else None
        a_lines = lo_pg["lines"] if lo_pg else []
        b_lines = ow_pages[i]["lines"] if i < len(ow_pages) else []
        ca, cb = content_key(a_lines), content_key(b_lines)
        la, lb = layout_key(a_lines), layout_key(b_lines)
        if not ca and not cb:
            drawings = (lo_pg or {}).get("drawings", 0)
            images = (lo_pg or {}).get("images", 0)
            # 只有「內文型」圖形才算偽影實據：向量 drawing，或不吃重複頁首/頁尾的點陣圖。
            unique_images = sum(1 for x in ((lo_pg or {}).get("image_xrefs") or [])
                                if xref_pages[x] == 1)
            if drawings or unique_images:
                # LO 該頁只剩圖形（VML pict／填色框／空矩形），雙方都抽不到文字：
                # 0/0 不是「全錯」，是量測不可比——記 None，不進平均、不算分歧。
                cs = ls = None
                note = "measurement_artifact:LO有圖形無可抽文字"
                if note not in row["notes"]:
                    row["notes"].append(note)
                row["measurement_artifacts"].append({
                    "page": i,
                    "drawings": drawings,
                    "images": images,
                    "unique_images": unique_images,
                })
            elif i < len(lo_pages) and i < len(ow_pages) and images == 0:
                # 同位雙空（LO 與我方都有這一頁）且 LO 該頁文字/圖形/點陣圖全無
                # ＝ LO 真空頁（para-level sectPr 照實開頁；tdf129888dml/tdf130120
                # K5-diag 實測 LO p2 lines=[] drawings=0 images=0）⇒ 0.0 是度量假
                # 分差，記 None 不進平均。sectPr 實據（源檔計數）寫進 notes 供覆核。
                cs = ls = None
                note = "measurement_artifact:雙空同位頁(LO真空)"
                if note not in row["notes"]:
                    row["notes"].append(note)
                if sect_pr is not None:
                    n2 = f"源檔 w:sectPr×{sect_pr}（雙方照實開頁⇒真空為結構性）"
                    if n2 not in row["notes"]:
                        row["notes"].append(n2)
                row["measurement_artifacts"].append({
                    "page": i,
                    "drawings": 0,
                    "images": 0,
                    "reason": "both_empty_same_position",
                })
            else:
                # 我方多出頁是空頁（LO 無此頁，n751077 p3 型），或 LO 該頁只剩重複
                # 頁首/頁尾圖（4d 防放水案）＝我方少了 LO 的真內容，仍記 0.0 真事故。
                cs = ls = 0.0
                if "empty_page_mismatch" not in row["notes"]:
                    row["notes"].append("empty_page_mismatch")
                if row["first_divergence"] is None:
                    row["first_divergence"] = i
        else:
            # autojunk=False：SequenceMatcher 預設對長字串把常見 2-char 塊當「垃圾」
            # 剔除，頁面文字一長就崩（HC031 p1 實測 legacy=0.008、關掉=0.655）。
            # 這是 0.007 假分歧的真正元兇，比換行/大小寫都是主因。
            # 單邊有字、另一邊空＝真事故（如 n751077 p2），ratio=0.0 照舊。
            cs = difflib.SequenceMatcher(None, ca, cb, autojunk=False).ratio() if (ca or cb) else 0.0
            ls = difflib.SequenceMatcher(None, la, lb, autojunk=False).ratio() if (la or lb) else 0.0
        row["content_scores"].append(None if cs is None else round(cs, 3))
        row["layout_scores"].append(None if ls is None else round(ls, 3))
        if cs is not None and cs < 0.6 and row["first_divergence"] is None:
            row["first_divergence"] = i
        # 內容近同但版面分數低 ⇒ 主要是換行位置差，記 notes 不記分歧。
        if cs is not None and ls is not None and cs >= 0.85 and ls < 0.5:
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
        cs = ", ".join("—" if s is None else str(s) for s in r["content_scores"][:6])
        ls = ", ".join("—" if s is None else str(s) for s in r["layout_scores"][:6])
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

    # (4) 雙空頁且 LO 有圖形 ⇒ 量測偽影：分數 None、不記分歧、留圖形實據。
    lo5 = {"lines": [], "drawings": 2, "images": 1, "w_pt": 612.0, "h_pt": 792.0}
    ow5 = {"pages": [{"lines": []}],
           "breaks": [{"width_pt": 612.0, "height_pt": 792.0}]}
    r5 = compare("t5", [lo5], ow5)
    check("4 雙空＋LO 有圖形＝None 偽影",
          r5["content_scores"][0] is None and r5["layout_scores"][0] is None
          and r5["first_divergence"] is None
          and "measurement_artifact:LO有圖形無可抽文字" in r5["notes"]
          and r5["measurement_artifacts"][0]["drawings"] == 2
          and r5["measurement_artifacts"][0]["images"] == 1,
          f"content={r5['content_scores'][0]} notes={r5['notes']}")

    # (4b) 同位雙空（LO 也有這一頁）且 LO 該頁無圖形實據 ⇒ 度量假分差
    #      （tdf129888dml/tdf130120 實測：para-level sectPr 雙方都照實開頁、
    #      LO p2 真空）⇒ None（measurement_artifact），不進平均、不記分歧。
    lo6a = [
        {"lines": ["PAGE ONE TEXT"], "drawings": 2, "images": 0, "image_xrefs": [],
         "w_pt": 612.0, "h_pt": 792.0},
        {"lines": [], "drawings": 0, "images": 0, "image_xrefs": [],
         "w_pt": 612.0, "h_pt": 792.0},
    ]
    ow6a = {"pages": [{"lines": ["PAGE ONE TEXT"]}, {"lines": []}],
            "breaks": [{"width_pt": 612.0, "height_pt": 792.0},
                       {"width_pt": 612.0, "height_pt": 792.0}]}
    r6a = compare("t6a", lo6a, ow6a)
    check("4b 同位雙空且 LO 無圖形＝None 度量假分差（tdf129888dml 型）",
          r6a["content_scores"] == [1.0, None]
          and r6a["layout_scores"] == [1.0, None]
          and r6a["first_divergence"] is None
          and any(n.startswith("measurement_artifact:雙空同位頁") for n in r6a["notes"])
          and r6a["measurement_artifacts"]
          and r6a["measurement_artifacts"][0].get("reason") == "both_empty_same_position",
          f"scores={r6a['content_scores']} notes={r6a['notes']}")

    # (4b2) 我方多出頁是空頁（LO 無此頁，n751077 p3 型真事故）⇒ 仍 0.0。
    #       判別＝頁數對齊上下文：LO 頁數 > index 存在＝同位頁；否則＝我方多頁。
    lo6b = {"lines": ["ONLY LO PAGE TEXT"], "drawings": 0, "images": 0,
            "image_xrefs": [], "w_pt": 612.0, "h_pt": 792.0}
    ow6b = {"pages": [{"lines": ["ONLY LO PAGE TEXT"]}, {"lines": []}],
            "breaks": [{"width_pt": 612.0, "height_pt": 792.0},
                       {"width_pt": 612.0, "height_pt": 792.0}]}
    r6b = compare("t6b", [lo6b], ow6b)
    check("4b2 我方多出空頁＝0.0 真事故（LO 無此頁）",
          r6b["content_scores"][1] == 0.0 and r6b["first_divergence"] == 1
          and any(n.startswith("empty_page_mismatch") for n in r6b["notes"])
          and not r6b["measurement_artifacts"],
          f"scores={r6b['content_scores']} div={r6b['first_divergence']} notes={r6b['notes']}")

    # (4c) 反向測試（n751077 p2 型）：LO 有文字、我方空 ⇒ 必須仍是 0.0，
    #      artifact 邏輯只能修雙空偽影，不能把真缺陷修沒。
    lo7 = {"lines": ["LO 這頁有真實文字內容", "第二行也是真實內容"],
           "drawings": 3, "images": 2, "w_pt": 612.0, "h_pt": 792.0}
    ow7 = {"pages": [{"lines": []}],
           "breaks": [{"width_pt": 612.0, "height_pt": 792.0}]}
    r7 = compare("t7", [lo7], ow7)
    check("4c LO 有字我方空＝0.0（防過度修正）",
          r7["content_scores"][0] == 0.0 and r7["first_divergence"] == 0
          and not r7["measurement_artifacts"],
          f"content={r7['content_scores'][0]} div={r7['first_divergence']}")

    # (4d) n751077 p2 型（逐頁重複的頁首圖，非內文圖）：LO 該頁無文字、只有
    #      重複圖片，我方空 ⇒ 仍必須是 0.0，不得因「有 image」而判偽影。
    lo8 = [
        {"lines": ["TEXT1", "TEXT2"], "drawings": 0, "images": 1,
         "image_xrefs": [6], "w_pt": 612.0, "h_pt": 792.0},
        {"lines": [], "drawings": 0, "images": 1,
         "image_xrefs": [6], "w_pt": 612.0, "h_pt": 792.0},
    ]
    ow8 = {"pages": [{"lines": ["TEXT1", "TEXT2"]}, {"lines": []}],
           "breaks": [{"width_pt": 612.0, "height_pt": 792.0},
                      {"width_pt": 612.0, "height_pt": 792.0}]}
    r8 = compare("t8", lo8, ow8)
    check("4d 重複頁首圖頁仍 0.0（n751077 p2 防偽影放水）",
          r8["content_scores"][1] == 0.0 and r8["content_scores"][0] == 1.0
          and r8["first_divergence"] == 1 and not r8["measurement_artifacts"],
          f"scores={r8['content_scores']} div={r8['first_divergence']}")

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
    scratch, profile_uri, needle = lo_scratch(soffice)
    kill_stale_lo()
    # 收場必殺：本輪自己拉起的 soffice 全部清掉（崩潰/異常也照清，atexit 語意）。
    atexit.register(kill_own_lo, needle)
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
        rows.append(compare(rel, lo, ow, sect_pr=sect_pr_count(docx)))
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
