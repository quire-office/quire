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
import random
import re
import shutil
import subprocess
import sys
import tempfile
import time
import unicodedata
import pathlib
from collections import Counter

try:
    import pymupdf
except ImportError:
    sys.exit("需要 pymupdf：python3 -m pip install --user pymupdf")

try:  # J3 色彩/圖片軸需 numpy；scipy 缺席時圖片軸退回自實作 flood-fill。
    import numpy as np
except ImportError:
    sys.exit("需要 numpy：python3 -m pip install --user numpy")
try:
    from scipy import ndimage as _ndi
except ImportError:
    _ndi = None

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


# ───────────────────────── J3：色彩軸／圖片軸／真缺字對账 ─────────────────────────
# 既有 J2 內容/版面分只看文字行，完全看不到色塊與圖片（fdo68607 表格底紋整頁全缺
# 卻仍 0.93）。以下三軸為「函式級新增」，不動 compare() 既有欄位語意。

J3_DPI = 150
J3_MAX_PAGES = 6          # 與報告「首 6 頁」口徑一致
SAT_MIN = 25              # 飽和度（0..255）低於此視為黑白灰，不進色彩/圖片軸
COLOR_FLOOR = 0.0005      # 兩邊彩色覆蓋率都低於此＝視為「皆無彩色」（濾抗鋸齒雜訊）
IMG_MIN_SIDE_PX = 20      # 圖片連通域 bbox 最小邊長（150dpi 約 3.4mm）
IMG_MIN_PIXELS = 400      # 圖片連通域最少彩色像素


def _sat_hue(img: "np.ndarray"):
    """RGB uint8 (H,W,3) → (sat 0..255, hue 0..360, value 0..255)。"""
    a = img[..., :3].astype(np.int32)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    mx, mn = a.max(axis=2), a.min(axis=2)
    delta = (mx - mn).astype(np.float64)
    sat = np.where(mx > 0, (mx - mn) * 255 // np.maximum(mx, 1), 0)
    safe = np.where(delta == 0, 1.0, delta)
    h = np.where(mx == r, ((g - b) / safe) % 6,
                 np.where(mx == g, (b - r) / safe + 2, (r - g) / safe + 4)) * 60.0
    return sat, np.where(delta == 0, 0.0, h), mx


def color_hist(img: "np.ndarray") -> "np.ndarray":
    """非白彩色像素（sat≥SAT_MIN）的 24 bin 粗量化直方圖：12 色相 × 2 明度，
    每格＝佔**整頁像素**比例（保留面積資訊，不是只比色種）。黑白灰一律忽略。"""
    sat, hue, val = _sat_hue(img)
    m = sat >= SAT_MIN
    hist = np.zeros(24)
    if m.any():
        hb = (hue[m] // 30).astype(int) % 12
        vb = (val[m] >= 200).astype(int)
        hist = np.bincount(hb * 2 + vb, minlength=24).astype(np.float64)
    return hist / max(img.shape[0] * img.shape[1], 1)


def score_color(lo_img: "np.ndarray", ow_img: "np.ndarray") -> float:
    """色彩軸 0..1：兩頁彩色直方圖的加權交集 Σmin/Σmax。
    LO 有整頁底紋、我方無色塊 → ≈0；兩邊都沒彩色（純黑白灰頁）→ 1.0。"""
    a, b = color_hist(lo_img), color_hist(ow_img)
    if max(a.sum(), b.sum()) < COLOR_FLOOR:
        return 1.0
    return float(np.minimum(a, b).sum() / np.maximum(a, b).sum())


def _label(mask: "np.ndarray"):
    """連通域標記（8-鄰接）。優先 scipy.ndimage；缺席時自實作 BFS flood-fill。"""
    if _ndi is not None:
        return _ndi.label(mask, structure=np.ones((3, 3), dtype=int))
    labels = np.zeros(mask.shape, dtype=np.int32)
    n = 0
    h, w = mask.shape
    for y, x in zip(*np.nonzero(mask)):
        if labels[y, x]:
            continue
        n += 1
        stack = [(y, x)]
        labels[y, x] = n
        while stack:
            cy, cx = stack.pop()
            for ny in range(max(cy - 1, 0), min(cy + 2, h)):
                for nx in range(max(cx - 1, 0), min(cx + 2, w)):
                    if mask[ny, nx] and not labels[ny, nx]:
                        labels[ny, nx] = n
                        stack.append((ny, nx))
    return labels, n


def image_boxes(img: "np.ndarray") -> list[tuple[int, int, int, int]]:
    """彩色像素（sat≥SAT_MIN）連通域 → bbox (x0,y0,x1,y1)（像素，x1/y1 為開區間端點）。
    先做小幅膨脹把照片內被灰階/暗部切碎的彩色塊併成一塊，bbox 仍以原彩色像素為準。"""
    return [b for b, _ in image_blobs(img)]


def image_blobs(img: "np.ndarray") -> list[tuple[tuple[int, int, int, int], float]]:
    """同 image_boxes，另附填充密度（連通域彩色像素數 / bbox 面積，0..1）——
    區分「整塊實心底紋」與「只有彩色邊框（空心框）」：fdo68607 我方只畫了彩色表格線，
    bbox 與 LO 整頁底紋一模一樣，單看 bbox 會被放水。依面積由大到小。"""
    sat, _, _ = _sat_hue(img)
    mask = sat >= SAT_MIN
    if not mask.any():
        return []
    grown = mask
    if _ndi is not None:
        grown = _ndi.binary_dilation(mask, structure=np.ones((3, 3), dtype=bool), iterations=4)
    labels, n = _label(grown)
    if _ndi is not None:
        slices = _ndi.find_objects(labels)
    else:  # 無 scipy：以整頁範圍當 slice（慢但正確）
        slices = [(slice(0, labels.shape[0]), slice(0, labels.shape[1]))] * n
    boxes = []
    for k, sl in enumerate(slices, start=1):
        if sl is None:
            continue
        sub = (labels[sl] == k) & mask[sl]
        if sub.sum() < IMG_MIN_PIXELS:
            continue
        ys, xs = np.nonzero(sub)
        x0, x1 = sl[1].start + int(xs.min()), sl[1].start + int(xs.max()) + 1
        y0, y1 = sl[0].start + int(ys.min()), sl[0].start + int(ys.max()) + 1
        if x1 - x0 >= IMG_MIN_SIDE_PX and y1 - y0 >= IMG_MIN_SIDE_PX:
            boxes.append(((x0, y0, x1, y1), float(sub.sum()) / ((x1 - x0) * (y1 - y0))))
    return sorted(boxes, key=lambda bd: -(bd[0][2] - bd[0][0]) * (bd[0][3] - bd[0][1]))


def _norm_box(b, shape):
    h, w = shape[:2]
    return (b[0] / w, b[1] / h, b[2] / w, b[3] / h)


def _iou(a, b) -> float:
    iw = min(a[2], b[2]) - max(a[0], b[0])
    ih = min(a[3], b[3]) - max(a[1], b[1])
    if iw <= 0 or ih <= 0:
        return 0.0
    inter = iw * ih
    ua = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter
    return inter / ua if ua > 0 else 0.0


def score_image(lo_img: "np.ndarray", ow_img: "np.ndarray") -> float:
    """圖片軸 0..1：兩頁彩色連通域 bbox（以頁尺寸正規化）左右對照。
    配對＝貪心取 IoU 最大者（IoU=0 不配對＝缺圖/錯位）；配對分 = (0.6·IoU + 0.4·尺寸相似
    〔寬比×高比〕) × 填充密度相似（空心彩框 vs 實心底紋會被壓低）；整頁分以 bbox 面積加權，
    未配對者以 0 分計入。兩邊皆無彩色塊 → 1.0。"""
    ba, bb = image_blobs(lo_img), image_blobs(ow_img)
    la = [_norm_box(b, lo_img.shape) for b, _ in ba]
    lb = [_norm_box(b, ow_img.shape) for b, _ in bb]
    da, db = [d for _, d in ba], [d for _, d in bb]
    if not la and not lb:
        return 1.0
    if not la or not lb:
        return 0.0
    pairs = sorted(((_iou(x, y), i, j) for i, x in enumerate(la) for j, y in enumerate(lb)),
                   reverse=True)
    used_a, used_b, num, den = set(), set(), 0.0, 0.0
    area = lambda b: (b[2] - b[0]) * (b[3] - b[1])   # noqa: E731
    for iou, i, j in pairs:
        if iou <= 0 or i in used_a or j in used_b:
            continue
        used_a.add(i)
        used_b.add(j)
        a, b = la[i], lb[j]
        size = (min(a[2] - a[0], b[2] - b[0]) / max(a[2] - a[0], b[2] - b[0])) * \
               (min(a[3] - a[1], b[3] - b[1]) / max(a[3] - a[1], b[3] - b[1]))
        w = max(area(a), area(b))
        dens = min(da[i], db[j]) / max(da[i], db[j])
        num += w * (0.6 * iou + 0.4 * size) * dens
        den += w
    den += sum(area(b) for k, b in enumerate(la) if k not in used_a)
    den += sum(area(b) for k, b in enumerate(lb) if k not in used_b)
    return float(num / den) if den > 0 else 1.0


def _pix_array(page) -> "np.ndarray":
    pix = page.get_pixmap(matrix=pymupdf.Matrix(J3_DPI / 72, J3_DPI / 72), alpha=False)
    return np.frombuffer(pix.samples, dtype=np.uint8).reshape(pix.height, pix.width, pix.n)[..., :3].copy()


# ── token 對账（真缺字偵測）──────────────────────────────────────────────

_HYPHENS = "-\u2010\u2011\u2012\u2013\u2014"
_CJK_RE = "\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\uac00-\ud7af"
_TOK_RE = re.compile(f"[{_CJK_RE}]|[^\\s{_CJK_RE}]+")


def tokenize(text: str) -> list[tuple[str, str]]:
    """全文 → [(顯示形, 正規形)]。NFKC＋casefold；CJK 逐字成 token（換行位置無關）；
    其餘以空白切；去頭尾標點（連字號保留，供斷行拼接判定）；無任何字母數字者
    （項目符號 \\uf0b7、純標點）丟棄。"""
    out = []
    for raw in _TOK_RE.findall(unicodedata.normalize("NFKC", text)):
        t = raw
        while t and unicodedata.category(t[0])[0] in "PS" and t[0] not in _HYPHENS:
            t = t[1:]
        while t and unicodedata.category(t[-1])[0] in "PS" and t[-1] not in _HYPHENS:
            t = t[:-1]
        if t and any(c.isalnum() for c in t):
            out.append((t, t.casefold()))
    return out


def _key(n: str) -> str:
    return n.strip(_HYPHENS)


def _concats(norms: list[str]) -> set[str]:
    """相鄰 2～3 段拼接（保留連字號／去連字號兩形），供斷行切片判定。"""
    res = set()
    for i in range(len(norms)):
        for span in (2, 3):
            seg = norms[i:i + span]
            if len(seg) < span:
                break
            res.add(_key("".join(seg)))
            res.add(_key("".join(s.rstrip(_HYPHENS) for s in seg[:-1]) + seg[-1]))
    return res


def token_audit(lo_text: str, ow_text: str) -> dict:
    """LO 全文 token 集 vs 我方全文 token 集 → 真缺字對账。
    - true_missing：LO 有、我方無，且不是斷行切片（我方相鄰「前段＋後段」拼接命中 LO token，
      或反向 LO 被切我方連著，皆算切片、不算缺）
    - split_ok：判為斷行切片而排除的 LO token（供人覆核）
    以集合語意對账（同詞重複只算一次），順序依 LO 首次出現。"""
    lo, ow = tokenize(lo_text), tokenize(ow_text)
    ow_norms = [n for _, n in ow]
    ow_set = {n for n in ow_norms} | {_key(n) for n in ow_norms}
    ow_cat = _concats(ow_norms)
    lo_norms = [n for _, n in lo]
    # LO 相鄰切片：(i, i+span) 拼接命中我方 → 區段內各 token 皆算切片
    lo_split_idx = set()
    for i in range(len(lo_norms)):
        for span in (2, 3):
            seg = lo_norms[i:i + span]
            if len(seg) < span:
                break
            cands = {_key("".join(seg)),
                     _key("".join(s.rstrip(_HYPHENS) for s in seg[:-1]) + seg[-1])}
            if cands & ow_set:
                lo_split_idx.update(range(i, i + span))
    missing, split_ok, seen = [], [], set()
    for idx, (disp, n) in enumerate(lo):
        k = _key(n)
        if k in seen:
            continue
        seen.add(k)
        if n in ow_set or k in ow_set:
            continue
        if k in ow_cat or idx in lo_split_idx:
            split_ok.append(disp)
        else:
            missing.append(disp)
    return {"true_missing": missing, "split_ok": split_ok,
            "lo_tokens": len(seen), "ow_tokens": len(set(ow_norms))}


def j3_metrics(lo_pdf: pathlib.Path, out_dir: pathlib.Path, lo_pages: list[dict],
               ow: dict, max_pages: int = J3_MAX_PAGES) -> dict:
    """單檔 J3 三軸：逐頁色/圖分（LO PDF 頁 vs 我方同頁 SVG，皆 150dpi）＋全文真缺字。
    回傳 dict 供 row.update()；單頁光柵化失敗該頁記 None，不中斷整檔。"""
    color, image = [], []
    ow_pages = ow["pages"]
    n = min(max(len(lo_pages), len(ow_pages)), max_pages)
    d = pymupdf.open(lo_pdf)
    try:
        for i in range(n):
            try:
                lo_arr = _pix_array(d[i]) if i < d.page_count else None
                ow_arr = None
                if i < len(ow_pages) and ow_pages[i].get("svg"):
                    sd = pymupdf.open(out_dir / ow_pages[i]["svg"])
                    try:
                        ow_arr = _pix_array(sd[0])
                    finally:
                        sd.close()
                # 單邊無此頁 ⇒ 視為空白頁（LO 有色而我方無頁＝0；兩邊皆無色＝1）
                if lo_arr is None:
                    lo_arr = np.full_like(ow_arr, 255)
                if ow_arr is None:
                    ow_arr = np.full_like(lo_arr, 255)
                color.append(round(score_color(lo_arr, ow_arr), 3))
                image.append(round(score_image(lo_arr, ow_arr), 3))
            except Exception:  # noqa: BLE001 —— 單頁度量失敗不拖垮整檔
                color.append(None)
                image.append(None)
    finally:
        d.close()
    lo_text = "\n".join(l for pg in lo_pages for l in pg["lines"])
    ow_text = "\n".join(l for pg in ow_pages for l in pg["lines"])
    ta = token_audit(lo_text, ow_text)
    return {"color_scores": color, "image_scores": image,
            "true_missing": ta["true_missing"], "true_missing_n": len(ta["true_missing"]),
            "split_ok_n": len(ta["split_ok"])}


def render_report(rows: list[dict], meta: dict, out_md: pathlib.Path) -> None:
    ok = sum(1 for r in rows if r.get("status", "ok") == "ok" and r["page_count_match"])
    miss_total = sum(r.get("true_missing_n") or 0 for r in rows)
    miss_files = sum(1 for r in rows if r.get("true_missing_n"))
    lines = [
        "# render-lab 差異報告（LO 基線 vs quire）",
        "",
        f"- 生成時間：{meta['ts']}",
        f"- LibreOffice：`{meta['soffice']}`",
        f"- 對象：{meta['target']}（{len(rows)} 檔）",
        f"- 頁數一致：{ok}/{len(rows)}；LO 轉換失敗：{meta['lo_fail']}",
        f"- 真缺字 {miss_total}（{miss_files} 檔有；LO 有我方無、已排除斷行切片）",
        "",
        "| 檔案 | LO 頁 | OW 頁 | 頁數 | 內容相似度（首 6 頁） | 版面相似度（首 6 頁） | 首個內容分歧頁 | 備註 | 色相似度（首 6 頁） | 圖相似度（首 6 頁） | 真缺字 |",
        "|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        if r.get("status") == "lo_fail":
            lines.append(f"| {r['file']} | — | — | — | — | — | — | LO 轉換失敗/逾時 | — | — | — |")
            continue
        cs = ", ".join("—" if s is None else str(s) for s in r["content_scores"][:6])
        ls = ", ".join("—" if s is None else str(s) for s in r["layout_scores"][:6])
        div = "—" if r["first_divergence"] is None else f"第 {r['first_divergence'] + 1} 頁"
        note = "; ".join(r["notes"]) or "—"
        j3c = ", ".join("—" if s is None else str(s) for s in (r.get("color_scores") or [])[:6]) or "—"
        j3i = ", ".join("—" if s is None else str(s) for s in (r.get("image_scores") or [])[:6]) or "—"
        tm = r.get("true_missing")
        if tm is None:
            j3m = "—"
        else:
            shown = ", ".join(f"`{t}`" for t in tm[:6]) + ("…" if len(tm) > 6 else "")
            j3m = f"{len(tm)}" + (f"：{shown}" if tm else "")
        lines.append(
            f"| {r['file']} | {r['lo_pages']} | {r['ow_pages']} | "
            f"{'✓' if r['page_count_match'] else '✗'} | {cs} | {ls} | {div} | {note} | "
            f"{j3c} | {j3i} | {j3m} |")
    lines += [
        "",
        "說明（J2 校正）：**內容相似度**＝整頁全部行正規化後拼成單一詞序列、",
        "casefold、清單符號正規化成統一佔位後跑 SequenceMatcher——忽略換行位置，",
        "反映「內容是否一致」；**版面相似度**＝舊的逐行（前 8 行）序列比，含換行",
        "位置，反映斷行/換行差異。「首個內容分歧頁」以內容分 <0.6 判定；內容近同",
        "而版面分低只進備註（僅換行位置不同），不記分歧。",
        "本報告追蹤的是**頁數收斂速度**與**斷點位置**，作為排版演進的量化基線。",
        "",
        "說明（J3 新增三欄，文字分完全看不到色塊/圖片，故另立）：",
        "- **色相似度**＝LO PDF 頁 vs 我方同頁（皆 150dpi 光柵）取非白彩色像素（飽和度≥25，黑白灰忽略）",
        "  的 24 bin（12 色相×2 明度）面積直方圖，算加權交集 Σmin/Σmax；LO 有色塊我方全無≈0，",
        "  兩邊皆無彩色＝1.0。",
        "- **圖相似度**＝彩色像素連通域 bbox 左右對照（0.6·IoU＋0.4·尺寸相似，面積加權，缺圖/錯位計 0）。",
        "- **真缺字**＝LO 全文 token 集有、我方全文 token 集無，且非斷行切片（我方相鄰「前段＋後段」",
        "  拼接命中 LO token，或反向 LO 被切我方連著，不算缺）；CJK 逐字比；頁首「真缺字 n」為總計。",
        "- 三欄為舊欄位之外的增量；內容/版面/頁數欄位語意與計算零變動。",
        "",
    ]
    miss_detail = [(r["file"], r["true_missing"]) for r in rows if r.get("true_missing")]
    if miss_detail:
        lines += ["## 真缺字清單（逐檔）", ""]
        for f, toks in miss_detail:
            lines.append(f"- `{f}`（{len(toks)}）：" + ", ".join(f"`{t}`" for t in toks[:60])
                         + ("…" if len(toks) > 60 else ""))
        lines.append("")
    out_md.write_text("\n".join(lines), encoding="utf-8")


def _gray_page() -> "np.ndarray":
    g = np.full((400, 300, 3), 255, dtype=np.uint8)
    g[10:40, 10:200] = (60, 60, 60)
    return g


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

    # ── J3 三軸（色/圖/真缺字）合成案例 ──────────────────────────────
    def _blank():
        return np.full((400, 300, 3), 255, dtype=np.uint8)

    lo_c = _blank()
    lo_c[0:200, :] = (0xD3, 0xDF, 0xEE)          # fdo68607 LO 側表格底紋色
    sc = score_color(lo_c, _blank())
    check("5 色彩軸：LO 有底紋我方全白≈0（fdo68607 型）", sc < 0.05, f"color={sc:.3f}")
    check("5b 色彩軸：同圖＝1、純灰字頁 vs 白頁＝1（黑白灰不計）",
          score_color(lo_c, lo_c.copy()) > 0.99
          and score_color(_gray_page(), _blank()) > 0.99)

    lo_i = _blank()
    lo_i[20:120, 20:180] = (200, 40, 40)
    ow_i = _blank()
    ow_i[20:70, 130:250] = (200, 40, 40)           # Bug51170 型：尺寸小且位置偏
    si = score_image(lo_i, ow_i)
    check("6 圖片軸：同圖＝1、缺圖＝0、錯位縮小圖 <0.5",
          score_image(lo_i, lo_i.copy()) > 0.99 and score_image(lo_i, _blank()) < 0.05 and si < 0.5,
          f"錯位縮小={si:.3f}")

    ta = token_audit("Drama sharp-edged blade IGCSE glossary of terms",
                     "Drama sharp-\nedged blade of terms")
    miss = {t.casefold() for t in ta["true_missing"]}
    check("7 真缺字：IGCSE/glossary 真缺、sharp-edged 斷行切片不算缺",
          miss == {"igcse", "glossary"} and "sharp-edged" in {t.casefold() for t in ta["split_ok"]},
          f"true_missing={ta['true_missing']} split_ok={ta['split_ok']}")
    ta2 = token_audit("well-\nknown Result.", "Well-known result")
    check("7b 真缺字：LO 被切我方連著／標點大小寫差不算缺", ta2["true_missing"] == [],
          f"true_missing={ta2['true_missing']}")

    print("SELFTEST", "ALL-PASS" if fails == 0 else f"{fails}-FAIL")
    return 0 if fails == 0 else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="*", help="直接指定的 docx（預設取 corpus manifest 前 n 檔）")
    ap.add_argument("--selftest", action="store_true",
                    help="跑 J2 度量自測（不碰 LO/引擎，純函式驗證）後退出")
    ap.add_argument("--corpus", default=str(REPO / "corpus"))
    ap.add_argument("--n", type=int, default=10)
    ap.add_argument("--seed", type=int, default=None,
                    help="給定時自 corpus manifest 以此 seed 隨機抽 n 檔（預設取前 n 檔）")
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
        if a.seed is None:
            picked = manifest["files"][: a.n]
            target_desc = f"corpus manifest 前 {len(picked)} 檔"
        else:
            picked = random.Random(a.seed).sample(manifest["files"], a.n)
            target_desc = f"corpus manifest 隨機 {len(picked)} 檔（seed {a.seed}）"
        targets = [pathlib.Path(a.corpus) / f["file"] for f in picked]
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
        try:  # J3 三軸：失敗只記錄、不影響既有欄位
            r.update(j3_metrics(pdf, out_dir, lo, ow))
            print(f"   色 {r['color_scores']} 圖 {r['image_scores']} "
                  f"真缺字 {r['true_missing_n']}（斷行切片已排除 {r['split_ok_n']}）："
                  f"{r['true_missing'][:12]}")
        except Exception as e:  # noqa: BLE001
            print(f"   ✗ J3 度量異常（舊欄位不受影響）：{e}")

    meta = {"ts": datetime.datetime.now().isoformat(timespec="seconds"),
            "soffice": str(soffice), "target": target_desc, "lo_fail": lo_fail}
    report = pathlib.Path(a.report) if a.report else out_dir / "report.md"
    render_report(rows, meta, report)
    print(f"\n報告 → {report}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
