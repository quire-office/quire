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
import html
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


def _painted_image_xrefs(p) -> tuple[list, list]:
    """該頁**實際繪製**的點陣圖（繪製口徑）。回傳 (繪製清單, 宣告清單)。

    `page.get_images()` 回的是「宣告在該頁 `/Resources`」的圖片——LO 常讓相鄰頁
    **共用同一個 Resources 間接物件**（n751077 兩頁同 obj 41），於是某頁會報出
    它根本沒畫的 xref（`get_image_rects(xref) == []`）。把宣告值當內容會誤傷
    `compare()` 的 4b「同位雙空（LO 真空）⇒ None」臂（判別式 `images == 0` 被
    殭屍宣告值擋死），真真空頁被判成 `empty_page_mismatch` 0.0（度量偽影）。

    因此內容口徑只計 `get_image_rects()` 非空者＝該頁真的有把它畫出來。
    「逐頁重複的頁首／頁尾圖」在兩頁都是**真繪製**，照樣進 `image_xrefs`，
    故 4d 防放水臂語意不受影響（`scripts/tests/test_k25a_painted_ink.py` 實檔鎖）。
    逐 entry 保留（不 dedupe）：維持 `xref_pages`／`unique_images` 的既有算式，
    本函式唯二的變更就是「未繪製的殭屍宣告不計」。
    """
    declared = [im[0] for im in p.get_images(full=True)]
    painted = []
    for x in declared:
        try:
            rects = p.get_image_rects(x)
        except Exception:  # noqa: BLE001 —— 保守方向：查不動就仍算該頁內容（不放水）
            painted.append(x)
            continue
        if rects:
            painted.append(x)
    return painted, declared


def pdf_page_facts(pdf: pathlib.Path) -> list[dict]:
    """PDF 每頁：文字行（幾何順序）、行方向向量、行數、頁尺寸（pt）、文字 span 數、圖形實據。

    `images`／`image_xrefs` ＝**繪製**口徑（見 `_painted_image_xrefs`）；
    `images_declared`／`image_xrefs_declared` 保留舊的**宣告**口徑供診斷與 A/B。
    E2 新增 `line_dirs` 與 `line_boxes`：捕獲 pymupdf 行方向向量與 bbox 形狀，防直排度量偽影。
    """
    d = pymupdf.open(pdf)
    pages = []
    for p in d:
        lines = []
        line_dirs = []
        line_boxes = []
        td = p.get_text("dict")
        for b in td.get("blocks", []):
            if "lines" in b:
                for l in b["lines"]:
                    lt = norm("".join(s.get("text", "") for s in l.get("spans", [])))
                    if lt:
                        lines.append(lt)
                        ldir = l.get("dir", (1.0, 0.0))
                        line_dirs.append((round(ldir[0], 2), round(ldir[1], 2)))
                        line_boxes.append(tuple(round(x, 1) for x in l.get("bbox", (0, 0, 0, 0))))
        if not lines:
            lines = [norm(t) for t in str(p.get_text("text")).splitlines() if norm(t)]
            line_dirs = [(1.0, 0.0)] * len(lines)
            line_boxes = [(0, 0, 0, 0)] * len(lines)
        words = len(p.get_text("words"))
        painted, declared = _painted_image_xrefs(p)
        pages.append({
            "lines": lines,
            "line_dirs": line_dirs,
            "line_boxes": line_boxes,
            "line_count": len(lines),
            "words": words,
            "w_pt": round(p.rect.width, 1),
            "h_pt": round(p.rect.height, 1),
            # 0/0 度量偽影判準：LO 該頁是否真的有非文字內容（向量圖／點陣圖）。
            # image_xrefs 供區分「內文圖」與「逐頁重複的頁首／頁尾圖」。
            "drawings": len(p.get_drawings()),
            "images": len(painted),
            "image_xrefs": painted,
            "images_declared": len(declared),
            "image_xrefs_declared": declared,
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


def _attr_num(attrs_str: str, name: str) -> float | None:
    """取 <text> 屬性上的純數值（缺屬性或不可解析 ⇒ None）。"""
    m = re.search(rf'\b{name}="(-?[\d.]+)"', attrs_str)
    if not m:
        return None
    try:
        return float(m.group(1))
    except ValueError:
        return None


def _svg_rotate_dir(attrs_str: str) -> tuple[float, float] | None:
    """從 transform 的 rotate 角推繪製方向；無旋轉（或只 0/180 度）⇒ None（不改判）。"""
    tr_m = re.search(r'transform=["\']([^"\']+)["\']', attrs_str)
    if not tr_m:
        return None
    rot_m = re.search(r"rotate\(\s*(-?\d+(?:\.\d+)?)\b", tr_m.group(1))
    if not rot_m:
        return None
    deg = float(rot_m.group(1)) % 360
    if abs(deg - 270) < 1.0:
        return (0.0, -1.0)
    if abs(deg - 90) < 1.0:
        return (0.0, 1.0)
    return None


def parse_svg_lines_and_dirs(svg_path: pathlib.Path) -> tuple[list[str], list[tuple[float, float]]]:
    """E2b 補強度量閘門：從實際出貨 SVG 的**真實幾何**解析文字行與行方向向量，防旗標與像素分家（ADT109119）。

    只准讀會參與繪製的幾何，禁讀任何自報屬性（本卡事故形態＝「旗標報直排、SVG 畫橫排」，
    自報屬性 `writing-mode`／`data-line-dir` 與旗標同屬我方宣稱，讀它們等於把破口從
    dump 搬到 SVG）：
    1. `<text>` 帶 `rotate(-90 …)`／`rotate(270 …)` ⇒ (0.0, -1.0)（btlr，由下而上）；
       帶 `rotate(90 …)` ⇒ (0.0, 1.0)（tbrl，由上而下）。
    2. **逐字直排定位**：連續單字元 `<text>` 同 x（|Δx|≤0.5）且 y 單調推進（步距 ≥0.3 字級）
       ⇒ 該欄為逐字堆疊的直排欄，方向依 y 指向取 (0.0, ±1.0)。
    3. 其餘（純 `<text>` 無旋轉、逐字橫向並排）⇒ (1.0, 0.0) 橫排。
    """
    if not svg_path.exists():
        return [], []
    content = svg_path.read_text(encoding="utf-8")
    items: list[tuple[str, tuple[float, float], float | None, float | None, float | None]] = []
    for m in re.finditer(r"<text\b([^>]*)>(.*?)</text>", content, re.DOTALL):
        attrs_str = m.group(1)
        text = norm(html.unescape(m.group(2)))
        if not text:
            continue
        direction = _svg_rotate_dir(attrs_str) or (1.0, 0.0)
        items.append((text, direction, _attr_num(attrs_str, "x"),
                      _attr_num(attrs_str, "y"), _attr_num(attrs_str, "font-size")))
    # 逐字直排偵測：找連續單字元、同 x、y 單調（同號）推進的欄位（真實像素定位，非宣稱）。
    run_start = 0
    while run_start < len(items):
        run_end = run_start
        run_sign = 0
        while run_end + 1 < len(items):
            a, b = items[run_end], items[run_end + 1]
            if len(a[0]) != 1 or len(b[0]) != 1:
                break
            if a[2] is None or b[2] is None or a[3] is None or b[3] is None:
                break
            if abs(a[2] - b[2]) > 0.5:  # x 不同 ⇒ 不同欄
                break
            step = b[3] - a[3]
            size = a[4] or b[4] or 1.0
            if abs(step) < 0.3 * size:  # y 沒推進 ⇒ 疊字而非直排
                break
            sign = 1 if step > 0 else -1
            if run_sign == 0:
                run_sign = sign
            elif sign != run_sign:  # 折返 ⇒ 不是一條單向直排欄
                break
            run_end += 1
        if run_end - run_start + 1 >= 3:  # 至少三字才構成逐字欄（兩字易與表格單字元欄誤判）
            first_y, last_y = items[run_start][3], items[run_end][3]
            vert = (0.0, 1.0) if last_y > first_y else (0.0, -1.0)
            for i in range(run_start, run_end + 1):
                txt, _d, x, y, sz = items[i]
                items[i] = (txt, vert, x, y, sz)
        run_start = run_end + 1
    return [i[0] for i in items], [i[1] for i in items]


def run_quire(docx: pathlib.Path, out_dir: pathlib.Path) -> dict:
    render_dir = out_dir / "quire" / docx.stem
    r = subprocess.run(
        ["cargo", "run", "-q", "-p", "quire-cli", "--",
         "render", str(docx), "--out", str(render_dir)],
        capture_output=True, text=True, cwd=REPO, timeout=180)
    if r.returncode != 0:
        raise RuntimeError(f"quire render 失敗: {r.stderr.strip()[:300]}")
    brks = json.loads((render_dir / "pagebreaks.json").read_text(encoding="utf-8"))
    # E2b：改從實際出貨產物 SVG 解析 lines 與 line_dirs，防旗標自報欺騙度量閘門。
    d = subprocess.run(
        ["cargo", "run", "-q", "-p", "quire-cli", "--", "dump", str(docx)],
        capture_output=True, text=True, cwd=REPO, timeout=180)
    d.check_returncode()
    pages = []
    for dl in json.loads(d.stdout):
        page_idx = dl["page"]
        svg_file = render_dir / f"{docx.stem}-p{page_idx}.svg"
        svg_lines, svg_dirs = parse_svg_lines_and_dirs(svg_file)
        if svg_file.exists():
            lines = svg_lines
            line_dirs = svg_dirs
        else:
            lines = []
            line_dirs = []
            for it in dl["items"]:
                if "Text" in it:
                    t = it["Text"]
                    text = norm(t.get("text", ""))
                    if text:
                        lines.append(text)
                        line_dirs.append((1.0, 0.0))
        images = [it["Image"] for it in dl["items"] if "Image" in it]
        pages.append({
            "lines": lines,
            "line_dirs": line_dirs,
            "line_count": len(lines),
            "svg": f"quire/{docx.stem}/{docx.stem}-p{page_idx}.svg",
            "images": images,
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


# ───────────────────── 走向拓撲錨定（bug65649 判準修正） ─────────────────────
# 舊口徑把 LO 第 i 頁與我方第 i 頁直接比寬高。當兩側**總頁數有位移**（分頁容量
# 漂移）時，同位索引已不是同一段正文——bug65649 實測 LO 178 頁 vs 我方 190 頁，
# 同位比對一次灌 22 筆假「sectPr 錯位」進 baseline（docs/bug65649-landscape-diag.md
# §三：該檔兩側走向拓撲其實 100% 全等，22 頁錯位全落在相位差區間內）。
# 新口徑兩段式：先抽兩側連續走向段（orientation runs）拓撲序列；
#   拓撲全等 ⇒ 尺寸注記改 run 錨定（第 k 段對第 k 段，段內頁數差記「容量漂移」），
#              不再逐同位索引報 sectPr 錯位；
#   拓撲不全等 ⇒ 照舊逐同位索引報（真 section 缺陷的訊號，不可掩蓋）＋寫明拓撲差。
# 注意：**只動注記臂**，內容/版面分數、first_divergence、measurement_artifacts
# 全部不受影響（test_run_anchor_metrics.py 有燈鎖死）。

SIZE_TOL = 2.0        # pt：沿用舊逐頁尺寸注記的容差，本卡不改尺
ORIENT_SQUARE_TOL = 2.0  # pt：長寬差小於此視為方形頁（無走向），記 "S"
ORIENT_LABEL = {"P": "直向", "L": "橫向", "S": "方形"}


def _pt1(v) -> float:
    """尺寸取一位小數（LO 側 pdf_page_facts 已 round，我方 breaks 未 round）。"""
    try:
        return round(float(v), 1)
    except (TypeError, ValueError):
        return -1.0


def page_orient(w_pt: float, h_pt: float) -> str:
    """單頁走向符號：P＝直向、L＝橫向、S＝方形（長寬同於容差內，無走向可言）。"""
    if w_pt - h_pt > ORIENT_SQUARE_TOL:
        return "L"
    if h_pt - w_pt > ORIENT_SQUARE_TOL:
        return "P"
    return "S"


def orientation_runs(dims: list[tuple[float, float]]) -> list[dict]:
    """逐頁尺寸序列 ⇒ 連續走向段（runs）拓撲序列。

    回傳 [{"orient": "P"|"L"|"S", "start": 0-based 首頁索引, "n": 段內頁數,
           "sizes": 段內去重尺寸 tuple 集合}...]，相鄰同走向頁合併成一段。
    空輸入回傳空清單（呼叫端據此判「不可比」，不進拓撲全等分支）。
    """
    runs: list[dict] = []
    for i, (w, h) in enumerate(dims):
        o = page_orient(w, h)
        if runs and runs[-1]["orient"] == o:
            runs[-1]["n"] += 1
            runs[-1]["sizes"].add((w, h))
        else:
            runs.append({"orient": o, "start": i, "n": 1, "sizes": {(w, h)}})
    for r in runs:
        r["sizes"] = frozenset(r["sizes"])
    return runs


def _fmt_sizes(sizes: frozenset) -> str:
    return "／".join(f"{w}×{h}" for w, h in sorted(sizes))


def _size_close(a: tuple[float, float], b: tuple[float, float]) -> bool:
    """兩組尺寸在 SIZE_TOL 內是否視為相同（twip→pt 有 0.1pt 級捨入差，
    LO 841.9×595.3 vs 我方 842.0×595.4 是同一個 A4 橫向，不是幾何缺陷）。"""
    return abs(a[0] - b[0]) <= SIZE_TOL and abs(a[1] - b[1]) <= SIZE_TOL


def _run_rep_size(sizes: frozenset) -> tuple[float, float]:
    """段代表尺寸：取段內最小者（段內多值時仍給單一代表，多值另行如實記錄）。"""
    return min(sizes)


def _run_spread(sizes: frozenset) -> bool:
    """段內是否混雜超出容差的多種尺寸（同段理應同宣告；混尺如實記錄）。"""
    rep = _run_rep_size(sizes)
    return any(not _size_close(rep, s) for s in sizes)


def _sizes_sym_cover(a: frozenset, b: frozenset) -> bool:
    """a 的每個尺寸在 b 有容差內夥伴、且 b 反向也成立 ⇒ 該段宣告尺寸等價。"""
    return (all(any(_size_close(x, y) for y in b) for x in a)
            and all(any(_size_close(x, y) for y in a) for x in b))


def _size_note_same_index(row: dict, lo_pages: list[dict], brks: list[dict]) -> None:
    """舊口徑（同位索引逐頁比寬高）——拓撲不全等時照舊走這條，不掩蓋真缺陷。"""
    for i, p in enumerate(lo_pages):
        if i >= len(brks):
            row["notes"].append(f"第 {i + 1} 頁我方無 pagebreak 記錄")
            continue
        ow_w, ow_h = _pt1(brks[i].get("width_pt")), _pt1(brks[i].get("height_pt"))
        if abs(p["w_pt"] - ow_w) > SIZE_TOL or abs(p["h_pt"] - ow_h) > SIZE_TOL:
            row["notes"].append(
                f"第 {i + 1} 頁 LO {p['w_pt']}×{p['h_pt']} ≠ 我方 {ow_w}×{ow_h}（sectPr）"
            )


def annotate_page_sizes(row: dict, lo_pages: list[dict], brks: list[dict]) -> None:
    """逐頁尺寸注記臂（Phase0-2b → 走向拓撲錨定版）。只寫 notes，不動任何分數欄位。"""
    lo_dims = [(_pt1(p.get("w_pt")), _pt1(p.get("h_pt"))) for p in lo_pages]
    ow_dims = [(_pt1(b.get("width_pt")), _pt1(b.get("height_pt"))) for b in brks]
    lo_runs = orientation_runs(lo_dims)
    ow_runs = orientation_runs(ow_dims)
    topo_lo = "".join(r["orient"] for r in lo_runs)
    topo_ow = "".join(r["orient"] for r in ow_runs)

    if lo_runs and ow_runs and topo_lo == topo_ow:
        # ── 拓撲全等：run 錨定比對，不再逐同位索引報 sectPr 錯位 ──
        n = len(lo_runs)
        drift_total = len(ow_dims) - len(lo_dims)
        if drift_total:
            _add_note(
                row,
                f"走向拓撲全等（{n} 段 {topo_lo}）；總頁數差 {drift_total:+d} 頁"
                f"＝分頁容量漂移（非 sectPr），尺寸注記改 run 錨定",
            )
        for k, (lr, rr) in enumerate(zip(lo_runs, ow_runs)):
            label = ORIENT_LABEL.get(lr["orient"], lr["orient"])
            for side, run in (("LO", lr), ("我方", rr)):
                if _run_spread(run["sizes"]):
                    _add_note(
                        row,
                        f"第 {k + 1} 段（{label}）{side} 段內混尺："
                        f"{_fmt_sizes(run['sizes'])}（同段多宣告，如實記錄）",
                    )
            if not _sizes_sym_cover(lr["sizes"], rr["sizes"]):
                _add_note(
                    row,
                    f"第 {k + 1} 段（{label}）LO {_fmt_sizes(lr['sizes'])} ≠ 我方 "
                    f"{_fmt_sizes(rr['sizes'])}（sectPr，run 錨定）",
                )
            d = rr["n"] - lr["n"]
            if d:
                _add_note(
                    row,
                    f"第 {k + 1} 段（{label}）容量漂移 {d:+d} 頁"
                    f"（LO {lr['n']} 頁／我方 {rr['n']} 頁）",
                )
        return

    # ── 拓撲不全等（任一側無頁亦走此路）：真 section 缺陷訊號，照舊逐同位索引報 ──
    if lo_runs and ow_runs:
        _add_note(row, f"走向拓撲不一致：LO={topo_lo} 我方={topo_ow}")
    _size_note_same_index(row, lo_pages, brks)


def _add_note(row: dict, note: str) -> None:
    if note not in row["notes"]:
        row["notes"].append(note)


def _synthetic_shared_resources_pdf(path: pathlib.Path, paint_second: bool) -> None:
    """合成 2 頁 PDF（自測 fixture 用；不碰 LO/引擎，只用 pymupdf 寫檔）。

    p0＝真繪製圖＋`TEXT1 TEXT2`。p1 依 `paint_second` 二選一：
    - False：把 p1 的 `/Resources` 指向 p0 **同一個間接物件**、內容流零 `Do`
      ⇒ p1「宣告 1、繪製 0」＝n751077（LO 兩頁同 obj 41）的精確重現。
    - True：p1 **真繪製**同一張圖＝逐頁重複的頁首圖（4d 防放水臂保護的對象）。
    """
    pix = pymupdf.Pixmap(pymupdf.csRGB, pymupdf.IRect(0, 0, 40, 35))
    pix.set_rect(pix.irect, (200, 40, 40))
    rect = pymupdf.Rect(17, 60, 84, 119)
    doc = pymupdf.open()
    p0 = doc.new_page(width=612, height=792)
    p0.insert_image(rect, pixmap=pix)
    p0.insert_text((70, 150), "TEXT1 TEXT2", fontsize=14)
    p0_xref = doc[0].xref
    doc.new_page(width=612, height=792)
    if paint_second:
        doc[1].insert_image(rect, pixmap=pix)
    else:
        res = doc.xref_get_key(p0_xref, "Resources")
        doc.xref_set_key(doc[1].xref, "Resources", res[1])
    doc.save(path)
    doc.close()


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
            # E2 行方向向量維度檢驗：LO 側若有直排行（dir≈[0, ±1]），而我方發射為橫排（[1, 0]），版面直接判失敗 (0.0)。
            lo_dirs = lo_pg.get("line_dirs", []) if lo_pg else []
            ow_dirs = ow_pages[i].get("line_dirs", []) if i < len(ow_pages) else []
            lo_has_vert = any(abs(d[1]) > 0.8 and abs(d[0]) < 0.2 for d in lo_dirs)
            ow_has_vert = any(abs(d[1]) > 0.8 and abs(d[0]) < 0.2 for d in ow_dirs)
            if lo_has_vert and not ow_has_vert:
                ls = 0.0
                note = "line_direction_mismatch:LO直排我方橫排"
                if note not in row["notes"]:
                    row["notes"].append(note)
                if row["first_divergence"] is None:
                    row["first_divergence"] = i
        row["content_scores"].append(None if cs is None else round(cs, 3))
        row["layout_scores"].append(None if ls is None else round(ls, 3))
        if cs is not None and cs < 0.6 and row["first_divergence"] is None:
            row["first_divergence"] = i
        # 內容近同但版面分數低 ⇒ 主要是換行位置差，記 notes 不記分歧。
        if cs is not None and ls is not None and cs >= 0.85 and ls < 0.5:
            row["notes"].append(f"第 {i + 1} 頁內容近同、僅換行位置不同")
    # 逐頁尺寸對照（Phase0-2b，bug65649 起改「走向拓撲錨定」）：我方每頁尺寸如實
    # 取自檔案 sectPr（pagebreaks.json 的 width_pt/height_pt），與 LO 逐頁比——只有
    # 真的不同才備註，不再拿固定 A4 當尺（舊備註「LO 頁尺寸 ≠ 我方固定 A4」自此
    # 退役）。兩側走向拓撲全等時改 run 錨定（同位位移的容量漂移不再誤報 sectPr），
    # 拓撲不一致才照舊逐同位索引報。詳見 annotate_page_sizes 上方說明。
    annotate_page_sizes(row, lo_pages, ow.get("breaks") or [])
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


def _mask_to_rects(arr: "np.ndarray", rects: list[tuple[float, float, float, float]], dpi: int = J3_DPI) -> "np.ndarray":
    """把非圖片 rect 區域（文字、向量 Rect、邊框 Rule）填白，只保留圖片區域。
    rects 為 pt 座標 (x0, y0, x1, y1)。"""
    if not rects:
        return np.full_like(arr, 255)
    masked = np.full_like(arr, 255)
    h, w = arr.shape[:2]
    scale = dpi / 72.0
    for x0, y0, x1, y1 in rects:
        px0, py0 = max(0, int(x0 * scale) - 2), max(0, int(y0 * scale) - 2)
        px1, py1 = min(w, int(x1 * scale) + 3), min(h, int(y1 * scale) + 3)
        masked[py0:py1, px0:px1] = arr[py0:py1, px0:px1]
    return masked


def score_image(lo_img: "np.ndarray", ow_img: "np.ndarray",
                lo_rects: list[tuple[float, float, float, float]] | None = None,
                ow_rects: list[tuple[float, float, float, float]] | None = None) -> float | None:
    """圖片軸 0..1（J4 校準）：
    若傳入 lo_rects 與 ow_rects（pt 座標）：
      - 雙方皆無鑲嵌圖（lo_rects=[] 且 ow_rects=[]，如 WC048 全矢量頁）⇒ 回傳 None（量測不可比/無圖可比）；
      - 一方有圖而另一方無圖 ⇒ 回傳 0.0（真缺圖或我方多圖）；
      - 雙方皆有圖 ⇒ 分別以 rects 遮罩過濾非圖片之矢量圖元/文字，比較真實圖片連通域。
    若未傳入 rects（相容 J3 合成測試純 numpy 呼叫）：
      - 兩邊皆無彩色塊 → 1.0；單邊無彩色塊 → 0.0。"""
    if lo_rects is not None or ow_rects is not None:
        lo_has = bool(lo_rects)
        ow_has = bool(ow_rects)
        if not lo_has and not ow_has:
            return None
        if not lo_has or not ow_has:
            return 0.0
        lo_img = _mask_to_rects(lo_img, lo_rects or [])
        ow_img = _mask_to_rects(ow_img, ow_rects or [])

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
_LEADER_RE = re.compile(r"\.{2,}|[\u00b7\u2022]{2,}")


def tokenize(text: str) -> list[tuple[str, str]]:
    """全文 → [(顯示形, 正規形)]。NFKC＋casefold；CJK 逐字成 token（換行位置無關）；
    其餘以空白切；連續點引導符（..、··）自邊界切開（J4 校準：免 How.....2 黏連誤報）；
    去頭尾標點（連字號保留，供斷行拼接判定）；無任何字母數字者
    （項目符號 \uf0b7、純標點）丟棄。"""
    out = []
    norm_text = unicodedata.normalize("NFKC", text)
    for raw in _TOK_RE.findall(norm_text):
        for part in _LEADER_RE.split(raw):
            t = part
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


def _is_field_artifact(token: str) -> bool:
    """LO 側欄位產物判定（如 FILENAME \\p 渲染成本機 scratch 路徑）。
    實例：C:\\Users\\me\\AppData\\Local\\Temp\\quire-render-lab\\ooxmlexport__2129393649.docx
    """
    t = token.replace("/", "\\")
    if re.match(r"^[a-zA-Z]:\\", t) or t.startswith("\\tmp\\") or "\\quire-render-lab\\" in t:
        if any(seg in t.lower() for seg in ("appdata", "temp", "quire-render-lab", "tmp")) and (
            t.lower().endswith(".docx") or t.lower().endswith(".pdf") or "quire-render-lab" in t.lower()
        ):
            return True
    return False


def token_audit(lo_text: str, ow_text: str) -> dict:
    """LO 全文 token 集 vs 我方全文 token 集 → 真缺字對账。
    - true_missing：LO 有、我方無，且不是斷行切片（我方相鄰「前段＋後段」拼接命中 LO token，
      或反向 LO 被切我方連著，皆算切片、不算缺）
    - split_ok：判為斷行切片而排除的 LO token（供人覆核）
    - field_artifacts：LO 展開本機 scratch 路徑等欄位產物而豁免的 token（J4 校準）
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
    missing, split_ok, field_artifacts, seen = [], [], [], set()
    for idx, (disp, n) in enumerate(lo):
        k = _key(n)
        if k in seen:
            continue
        seen.add(k)
        if n in ow_set or k in ow_set:
            continue
        if _is_field_artifact(disp):
            field_artifacts.append(disp)
        elif k in ow_cat or idx in lo_split_idx:
            split_ok.append(disp)
        else:
            missing.append(disp)
    return {"true_missing": missing, "split_ok": split_ok,
            "field_artifacts": field_artifacts,
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

                # J4 圖軸校準：提取 LO 鑲嵌圖 vs 我方 Image 項，矢量圖元不入圖軸
                lo_rects = []
                if i < d.page_count:
                    try:
                        lo_rects = [info["bbox"] for info in d[i].get_image_info() if "bbox" in info]
                    except Exception:
                        lo_rects = [d[i].rect] if len(d[i].get_images()) > 0 else []

                ow_rects = []
                if i < len(ow_pages):
                    imgs = ow_pages[i].get("images")
                    if imgs is not None:
                        ow_rects = [(im["x"], im["y"], im["x"] + im["w"], im["y"] + im["h"]) for im in imgs]
                    elif ow_pages[i].get("svg"):
                        svg_file = out_dir / ow_pages[i]["svg"]
                        if svg_file.exists():
                            for m in re.finditer(r'<image\b[^>]*\bx="([^"]+)"[^>]*\by="([^"]+)"[^>]*\bwidth="([^"]+)"[^>]*\bheight="([^"]+)"', svg_file.read_text(encoding="utf-8")):
                                try:
                                    ix, iy, iw, ih = float(m.group(1)), float(m.group(2)), float(m.group(3)), float(m.group(4))
                                    ow_rects.append((ix, iy, ix + iw, iy + ih))
                                except ValueError:
                                    pass

                si = score_image(lo_arr, ow_arr, lo_rects=lo_rects, ow_rects=ow_rects)
                image.append(None if si is None else round(si, 3))
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

    # (4d2)/(4d3) K25-A：4d fixture 是手寫 dict，改「繪製口徑」時它不會紅——故補
    # 兩條**真實 PDF**（合成，不碰 LO）的鎖：
    #   4d2＝LO 該頁**真繪製**的重複頁首圖＋我方空 ⇒ 仍必須 0.0（防放水臂不得稀釋）；
    #   4d3＝LO 該頁**只宣告未繪製**（兩頁共用 Resources obj，n751077 鐵證態）
    #        ＋我方同位空 ⇒ None＋雙空同位頁 artifact（燈1 的機制面）。
    # 細部欄位口徑另由 scripts/tests/test_k25a_painted_ink.py 逐條鎖。
    with tempfile.TemporaryDirectory() as _td:
        _tdp = pathlib.Path(_td)
        _hdr = _tdp / "k25a-hdr.pdf"
        _void = _tdp / "k25a-void.pdf"
        _synthetic_shared_resources_pdf(_hdr, paint_second=True)
        _synthetic_shared_resources_pdf(_void, paint_second=False)
        ow9 = {"pages": [{"lines": ["TEXT1", "TEXT2"]}, {"lines": []}],
               "breaks": [{"width_pt": 612.0, "height_pt": 792.0},
                          {"width_pt": 612.0, "height_pt": 792.0}]}
        f_hdr = pdf_page_facts(_hdr)
        r9 = compare("t9", f_hdr, ow9)
        check("4d2 真繪製重複頁首圖（實檔 PDF）仍 0.0（防放水臂必仍綠）",
              [pg["images"] for pg in f_hdr] == [1, 1]
              and r9["content_scores"] == [1.0, 0.0] and r9["first_divergence"] == 1
              and "empty_page_mismatch" in r9["notes"] and not r9["measurement_artifacts"],
              f"images={[pg['images'] for pg in f_hdr]} scores={r9['content_scores']}")
        f_void = pdf_page_facts(_void)
        r10 = compare("t10", f_void, ow9, sect_pr=2)
        check("4d3 只宣告未繪製的共用 Resources 頁（實檔 PDF）＝None 偽影",
              f_void[1]["images"] == 0 and f_void[1]["images_declared"] == 1
              and r10["content_scores"] == [1.0, None] and r10["first_divergence"] is None
              and "empty_page_mismatch" not in r10["notes"]
              and any(n.startswith("measurement_artifact:雙空同位頁") for n in r10["notes"])
              and r10["measurement_artifacts"]
              and r10["measurement_artifacts"][0]["reason"] == "both_empty_same_position",
              f"images={f_void[1]['images']}/{f_void[1]['images_declared']} "
              f"scores={r10['content_scores']}")

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

    # (6b) J4 校準：WC048 型——LO 全矢量頁（零鑲嵌圖）vs 我方彩色矢量 Region（Rect）
    #      雙方皆無鑲嵌圖時圖分記 None；1.0 僅限雙方皆有圖且匹配的情形
    lo_wc = _blank()
    ow_wc = _blank()
    ow_wc[100:130, 50:200] = (0x44, 0x72, 0xC4)  # K12 藍色向量 Rect
    swc = score_image(lo_wc, ow_wc, lo_rects=[], ow_rects=[])
    rect = (20 * 72 / 150, 20 * 72 / 150, 180 * 72 / 150, 120 * 72 / 150)
    s_matched = score_image(lo_i, lo_i.copy(), lo_rects=[rect], ow_rects=[rect])
    check("6b 圖軸口徑（WC048 型）：LO 全矢量頁 vs 我方彩色矢量 Region ⇒ None 偽影；1.0 僅限雙方皆有圖且匹配",
          swc is None and s_matched > 0.99,
          f"swc={swc} s_matched={s_matched}")

    ta = token_audit("Drama sharp-edged blade IGCSE glossary of terms",
                     "Drama sharp-\nedged blade of terms")
    miss = {t.casefold() for t in ta["true_missing"]}
    check("7 真缺字：IGCSE/glossary 真缺、sharp-edged 斷行切片不算缺",
          miss == {"igcse", "glossary"} and "sharp-edged" in {t.casefold() for t in ta["split_ok"]},
          f"true_missing={ta['true_missing']} split_ok={ta['split_ok']}")
    ta2 = token_audit("well-\nknown Result.", "Well-known result")
    check("7b 真缺字：LO 被切我方連著／標點大小寫差不算缺", ta2["true_missing"] == [],
          f"true_missing={ta2['true_missing']}")

    # (7c) J4 校準：2129393649 型——LO 展開 FILENAME \\p 本機 scratch 路徑豁免
    ta3 = token_audit(
        "CONTROLLED DOCUMENT\nC:\\Users\\me\\AppData\\Local\\Temp\\quire-render-lab\\ooxmlexport__2129393649.docx\nPURPOSE",
        "CONTROLLED DOCUMENT\nPURPOSE",
    )
    check("7c 真缺字：FILENAME 欄位本機 scratch 路徑不計缺（2129393649 型）",
          ta3["true_missing"] == [] and len(ta3.get("field_artifacts", [])) == 1,
          f"true_missing={ta3['true_missing']} field_artifacts={ta3.get('field_artifacts')}")

    # (7d) J4 校準：fdo77129 型——TOC 目錄引導點（連續 .／·）邊界切開
    ta4 = token_audit("How........................................................................................................................................................................2",
                      "How ...............................................................................................................................................2")
    check("7d 真缺字：TOC 點引導符黏連切開不計缺（fdo77129 型）",
          ta4["true_missing"] == [],
          f"true_missing={ta4['true_missing']}")

    # (8) E2 行方向維度自測：LO 側為直排（dir=[0, -1]），我方為橫排（[1, 0]）⇒ 版面分判 0.0 失敗；我方同為直排則通過。
    lo_v = {"lines": ["First paragraph.", "Second paragraph."],
            "line_dirs": [(0.0, -1.0), (0.0, -1.0)],
            "w_pt": 612.0, "h_pt": 792.0}
    ow_h = {"pages": [{"lines": ["First paragraph.", "Second paragraph."],
                       "line_dirs": [(1.0, 0.0), (1.0, 0.0)]}],
            "breaks": [{"width_pt": 612.0, "height_pt": 792.0}]}
    ow_v = {"pages": [{"lines": ["First paragraph.", "Second paragraph."],
                       "line_dirs": [(0.0, -1.0), (0.0, -1.0)]}],
            "breaks": [{"width_pt": 612.0, "height_pt": 792.0}]}
    rv_fail = compare("tv_fail", [lo_v], ow_h)
    rv_pass = compare("tv_pass", [lo_v], ow_v)
    check("8 行方向維度：LO 直排我方橫排判失敗，兩方皆直排通過",
          rv_fail["layout_scores"][0] == 0.0
          and any("line_direction_mismatch" in n for n in rv_fail["notes"])
          and rv_pass["layout_scores"][0] >= 0.99,
          f"fail_ls={rv_fail['layout_scores']} pass_ls={rv_pass['layout_scores']}")

    # (8b) E2b SVG 產物文字行方向抽取自測：出貨 SVG 旋轉解析（ADT109119）
    with tempfile.TemporaryDirectory() as td:
        tf = pathlib.Path(td) / "test.svg"
        tf.write_text(
            '<svg><text x="10" y="20">Horz</text>\n'
            '<text x="30" y="40" transform="rotate(-90 30 40)">Vert270</text>\n'
            '<text x="50" y="60" transform="rotate(90 50 60)">Vert90</text></svg>',
            encoding="utf-8"
        )
        s_lines, s_dirs = parse_svg_lines_and_dirs(tf)
        check("8b SVG 行方向抽取：橫排 (1,0)、rotate(-90) 為 (0,-1)、rotate(90) 為 (0,1)",
              s_lines == ["Horz", "Vert270", "Vert90"]
              and s_dirs == [(1.0, 0.0), (0.0, -1.0), (0.0, 1.0)],
              f"lines={s_lines} dirs={s_dirs}")

    # (8c) E2b 防自報鎖：自報直排屬性（writing-mode／data-line-dir）**不得**改變判定——
    # 像素是橫排（無旋轉、單行橫向）就必須回報 (1,0)，否則「旗標直排＋SVG 橫排」可繞過閘門（ADT109119）。
    with tempfile.TemporaryDirectory() as td:
        tf2 = pathlib.Path(td) / "cheat.svg"
        tf2.write_text(
            '<svg><text x="10" y="20" data-line-dir="ea-vert">Purely horizontal</text>\n'
            '<text x="10" y="34" writing-mode="vertical-rl">Also horizontal pixels</text></svg>',
            encoding="utf-8"
        )
        c_dirs = parse_svg_lines_and_dirs(tf2)[1]
        check("8c 防自報：純橫排像素掛直排自報屬性仍判 (1,0)（不得讀宣稱屬性）",
              c_dirs == [(1.0, 0.0), (1.0, 0.0)],
              f"dirs={c_dirs}")

    # (8d) E2b 逐字直排定位偵測：連續單字元同 x、y 單調推進 ⇒ 直排（真實像素定位，非宣稱）（ADT109119）。
    with tempfile.TemporaryDirectory() as td:
        tf3 = pathlib.Path(td) / "perchar.svg"
        tf3.write_text(
            '<svg>'
            + "".join(f'<text x="100" y="{20 + i * 12}" font-size="11">V</text>' for i in range(4))
            + "".join(f'<text x="{200 + i * 12}" y="30" font-size="11">h</text>' for i in range(4))
            + "</svg>",
            encoding="utf-8"
        )
        pc_dirs = parse_svg_lines_and_dirs(tf3)[1]
        check("8d 逐字直排定位：同 x 單向 y 推進判 (0,1)、同 y 橫向推進判 (1,0)",
              pc_dirs[:4] == [(0.0, 1.0)] * 4 and pc_dirs[4:] == [(1.0, 0.0)] * 4,
              f"dirs={pc_dirs}")

    # ── (9) bug65649 走向拓撲錨定：同拓撲＋頁數位移 ⇒ 不報 sectPr，報容量漂移；
    #      拓撲不同 ⇒ 照報（防放水臂，比照 4b2/4c/4d 慣例——改口徑只能修假錯位，
    #      不能把真 section 缺陷修沒）。分數欄位必須逐位不動（注記臂獨立）。
    P, L = (595.3, 841.9), (841.9, 595.3)   # A4 直／橫（pt）
    # (9a) 同拓撲 P-L-P、我方總頁數多 k：LO 20+69+5=94 頁，我方 21+71+7=99 頁
    #      （bug65649 前四 Runs 的縮影）。舊口徑在此必然誤報一橫一直假 sectPr 錯位。
    def _lo(dim, n, tag):
        w, h = dim
        return [{"lines": [f"{tag} 第 {i + 1} 頁正文填充"], "w_pt": w, "h_pt": h}
                for i in range(n)]

    lo9 = _lo(P, 20, "LOa") + _lo(L, 69, "LOb") + _lo(P, 5, "LOc")
    ow9 = {"pages": [{"lines": [f"OWx 第 {i + 1} 頁正文填充"]} for i in range(99)],
           "breaks": ([{"width_pt": w, "height_pt": h}
                       for (w, h) in [P] * 21 + [L] * 71 + [P] * 7])}
    r9 = compare("t9-run", lo9, ow9)
    sect9 = [n for n in r9["notes"] if "（sectPr）" in n]
    drift9 = [n for n in r9["notes"] if "容量漂移 " in n]   # 頁首總差註記無空格，不計入
    check("9a 同拓撲＋頁數位移＝零假 sectPr 錯位、改報容量漂移（bug65649 型）",
          not sect9
          and any("走向拓撲全等" in n and "+5 頁" in n for n in r9["notes"])
          and len(drift9) == 3  # 三段頁數差 21-20 / 71-69 / 7-5 全非零
          and any("容量漂移 +2 頁" in n and "橫向" in n for n in r9["notes"])
          and r9["page_count_match"] is False,
          f"sectPr={sect9} drift={drift9[:2]}")
    # (9b) 分數燈（本卡紀律「分數逐位相同」的機制面鎖）：同輸入只改 breaks 的尺寸
    #      餵成與 LO 逐頁相同——注記臂輸出必消失（只剩容量漂移類注記、零 sectPr），
    #      而 breaks 只被注記臂消費，故逐頁分與 first_divergence 必須逐位不動。
    ow9b = {"pages": ow9["pages"],
            "breaks": [{"width_pt": p["w_pt"], "height_pt": p["h_pt"]} for p in lo9]
                     + [{"width_pt": P[0], "height_pt": P[1]}
                        for _ in range(len(ow9["pages"]) - len(lo9))]}
    r9b = compare("t9-run-clean", lo9, ow9b)
    check("9b 注記臂隔離燈：只改 breaks 尺寸 ⇒ 零 sectPr 注記＋分數欄位逐位不動",
          not [n for n in r9b["notes"] if "（sectPr）" in n or "拓撲不一致" in n]
          and r9b["content_scores"] == r9["content_scores"]
          and r9b["layout_scores"] == r9["layout_scores"]
          and r9b["first_divergence"] == r9["first_divergence"]
          and r9b["measurement_artifacts"] == r9["measurement_artifacts"],
          f"notes={r9b['notes'][:3]}")
    # (9c) 防放水臂：拓撲不同（LO 有橫向段、我方全直向——真 section 缺陷）⇒
    #      照舊逐同位索引報 sectPr 錯位＋注記寫明拓撲不一致。
    lo10 = _lo(P, 3, "LOp") + _lo(L, 2, "LOl")
    ow10 = {"pages": [{"lines": [f"OWq 第 {i + 1} 頁正文填充"]} for i in range(5)],
            "breaks": [{"width_pt": w, "height_pt": h} for (w, h) in [P] * 5]}
    r10 = compare("t10-topo-diff", lo10, ow10)
    check("9c 防放水：拓撲不同仍照報 sectPr 錯位＋寫明走向拓撲不一致",
          any(n.startswith("走向拓撲不一致：LO=PL 我方=P") for n in r10["notes"])
          and len([n for n in r10["notes"] if "（sectPr）" in n]) == 2
          and not any("容量漂移" in n for n in r10["notes"]),
          f"notes={r10['notes']}")
    # (9d) run 錨定下的**真**幾何缺陷不被掩蓋：拓撲全等（P-L-P）但第 2 段 LO 橫向、
    #      我方該段同拓撲標籤下尺寸宣告不同（L 但 900×600）⇒ 仍報 sectPr（run 錨定）。
    lo11 = _lo(P, 2, "LOr") + _lo(L, 2, "LOs")
    ow11 = {"pages": [{"lines": [f"OWt 第 {i + 1} 頁正文填充"]} for i in range(4)],
            "breaks": [{"width_pt": P[0], "height_pt": P[1]},
                       {"width_pt": P[0], "height_pt": P[1]},
                       {"width_pt": 900.0, "height_pt": 600.0},
                       {"width_pt": 900.0, "height_pt": 600.0}]}
    r11 = compare("t11-run-real-geo", lo11, ow11)
    check("9d run 錨定不掩蓋真幾何差：同拓撲段內尺寸全異仍報 sectPr（run 錨定）",
          any("（sectPr，run 錨定）" in n and "第 2 段" in n for n in r11["notes"]),
          f"notes={r11['notes']}")
    # (9e) 我方無 pagebreak 記錄的退化態（brks 空）⇒ 照舊逐頁記「我方無 pagebreak」，
    #      不因拓撲比較路徑吞掉缺頁訊號。
    lo12 = _lo(P, 2, "LOu")
    r12 = compare("t12-no-brks", lo12, {"pages": [], "breaks": []})
    check("9e 零 pagebreak 退化態：缺頁注記照舊（不被拓撲臂吞掉）",
          [n for n in r12["notes"] if "我方無 pagebreak" in n]
          == ["第 1 頁我方無 pagebreak 記錄", "第 2 頁我方無 pagebreak 記錄"],
          f"notes={r12['notes']}")
    # (9f) orientation_runs 單元：混合序列分段正確＋方形頁記 S＋空輸入回空清單。
    runs_f = orientation_runs([(595.3, 841.9), (595.3, 841.9), (841.9, 595.3),
                               (600.0, 600.0), (600.0, 600.0)])
    check("9f orientation_runs 分段：P2-L1-S2、方頁記 S、空輸入回空清單",
          [(r["orient"], r["start"], r["n"]) for r in runs_f]
          == [("P", 0, 2), ("L", 2, 1), ("S", 3, 2)] and orientation_runs([]) == [],
          f"runs={[(r['orient'], r['start'], r['n']) for r in runs_f]}")

    # (9g) 容差鎖：twip→pt 的 0.1pt 級捨入差（LO 841.9×595.3 vs 我方 842.0×595.4）
    #      不得被 run 錨定判成 sectPr——bug65649 觸發集實跑抓到的誤報態。
    lo13 = _lo((841.9, 595.3), 3, "LOv")
    ow13 = {"pages": [{"lines": [f"OWw 第 {i + 1} 頁正文填充"]} for i in range(3)],
            "breaks": [{"width_pt": 842.0, "height_pt": 595.4}] * 3}
    r13 = compare("t13-rounding", lo13, ow13)
    check("9g 容差：0.1pt 級捨入差不判 sectPr（run 錨定同段尺寸等價）",
          not [n for n in r13["notes"] if "sectPr" in n],
          f"notes={r13['notes']}")

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
