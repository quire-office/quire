#!/usr/bin/env python3
"""P5-diag：EMF 域色軸地板定量診斷探針（report-only，唯讀）。

只讀 corpus／scripts/render_lab.py，不改任何 core/、shells/、既定 scripts、
corpus/、out/。所有暫存輸出預設落 /tmp，不污染工作區。

量測四件事：
  1. 兩個目標 docx 的 media EMF：檔名／大小／sha256 前 8／記錄型別前 5 名，
     以及 EMR_STRETCHDIBITS 內嵌 DIB 的寬高／bpp／壓縮方式。
  2. LO 側 ground truth：soffice 轉 PDF 後，PyMuPDF 逐頁數 drawings 彩色
     fill 面積、images 柵格與逐圖彩色覆蓋率、整頁彩色覆蓋率。
  3. 我方側：cargo render SVG 裡的灰色佔位 rect 座標／尺寸／色值，與 LO
     圖形位置對照；並模擬「路線 (b) 直讀 DIB」的色軸預測分數。
  4. 全語料 media EMF／WMF 市場規模（檔數／bytes／最大單檔／DIB 可讀數）。

用法：
  python3 scripts/p5_emf_probe.py
  python3 scripts/p5_emf_probe.py --out /tmp/p5-emf-probe --ours /tmp/p5-ours
  python3 scripts/p5_emf_probe.py --skip-lo --skip-render   # 只跑靜態掃描

輸出：終端摘要 ＋ <out>/p5_emf_probe.json。診斷結論另見
docs/p5-emf-diag.md（本探針不代寫結論）。
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import json
import os
import pathlib
import re
import shutil
import struct
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts"))
import render_lab as RL  # noqa: E402  —— 唯讀 import：沿用 LO 轉檔／色彩軸口徑

try:
    import numpy as np
    import pymupdf
except ImportError as e:  # pragma: no cover
    sys.exit(f"需要 numpy／pymupdf：{e}")

# EMF 記錄型別（MS-EMF §2.1.1；只列本診斷會遇到與判讀所需者）。
EMR_NAMES = {
    1: "HEADER", 2: "POLYBEZIER", 3: "POLYGON", 4: "POLYLINE",
    5: "POLYBEZIERTO", 6: "POLYLINETO", 7: "POLYPOLYLINE", 8: "POLYPOLYGON",
    9: "SETWINDOWEXTEX", 10: "SETWINDOWORGEX", 11: "SETVIEWPORTEXTEX",
    12: "SETVIEWPORTORGEX", 13: "SETBRUSHORGEX", 14: "EOF", 15: "SETPIXELV",
    16: "SETMAPPERFLAGS", 17: "SETMAPMODE", 18: "SETBKMODE", 19: "SETPOLYFILLMODE",
    20: "SETROP2", 21: "SETSTRETCHBLTMODE", 22: "SETTEXTALIGN",
    23: "SETCOLORADJUSTMENT", 24: "SETTEXTCOLOR", 25: "SETBKCOLOR",
    26: "OFFSETCLIPRGN", 27: "MOVETOEX", 28: "SETMETARGN", 29: "EXCLUDECLIPRECT",
    30: "INTERSECTCLIPRECT", 31: "SCALEVIEWPORTEXTEX", 32: "SCALEWINDOWEXTEX",
    33: "SAVEDC", 34: "RESTOREDC", 35: "SETWORLDTRANSFORM",
    36: "MODIFYWORLDTRANSFORM", 37: "SELECTOBJECT", 38: "CREATEPEN",
    39: "CREATEBRUSHINDIRECT", 40: "DELETEOBJECT", 41: "ANGLEARC", 42: "ELLIPSE",
    43: "RECTANGLE", 44: "ROUNDRECT", 45: "ARC", 46: "CHORD", 47: "PIE",
    48: "SELECTPALETTE", 49: "CREATEPALETTE", 50: "SETPALETTEENTRIES",
    51: "RESIZEPALETTE", 52: "REALIZEPALETTE", 53: "EXTFLOODFILL", 54: "LINETO",
    55: "ARCTO", 56: "POLYDRAW", 57: "SETARCDIRECTION", 58: "SETMITERLIMIT",
    59: "BEGINPATH", 60: "ENDPATH", 61: "CLOSEFIGURE", 62: "FILLPATH",
    63: "STROKEANDFILLPATH", 64: "STROKEPATH", 65: "FLATTENPATH",
    66: "WIDENPATH", 67: "SELECTCLIPPATH", 68: "ABORTPATH", 70: "COMMENT",
    71: "FILLRGN", 72: "FRAMERGN", 73: "INVERTRGN", 74: "PAINTRGN",
    75: "EXTSELECTCLIPRGN", 76: "BITBLT", 77: "STRETCHBLT", 78: "MASKBLT",
    79: "PLGBLT", 80: "SETDIBITSTODEVICE", 81: "STRETCHDIBITS",
    82: "EXTCREATEFONTINDIRECTW", 83: "EXTTEXTOUTA", 84: "EXTTEXTOUTW",
    85: "POLYBEZIER16", 86: "POLYGON16", 87: "POLYLINE16", 88: "POLYBEZIERTO16",
    89: "POLYLINETO16", 90: "POLYPOLYLINE16", 91: "POLYPOLYGON16",
    92: "POLYDRAW16", 93: "CREATEMONOBRUSH", 94: "CREATEDIBPATTERNBRUSHPT",
    95: "EXTCREATEPEN", 96: "POLYTEXTOUTA", 97: "POLYTEXTOUTW", 98: "SETICMMODE",
    99: "SETICMPROFILEA", 100: "SETICMPROFILEW",
}

TARGETS = [
    REPO / "corpus/ooxmlexport/fdo76591.docx",
    REPO / "corpus/ooxmlexport/n779834.docx",
]

SAT_MIN = 25  # 與 render_lab 色軸同口徑


# ───────────────────────── 通用小工具 ─────────────────────────

def emr_name(t: int) -> str:
    return EMR_NAMES.get(t, f"UNKNOWN_{t}")


def sha8(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()[:8]


def docx_media(docx: pathlib.Path) -> list[tuple[str, bytes]]:
    import zipfile
    z = zipfile.ZipFile(docx)
    return [(n, z.read(n)) for n in z.namelist() if "/media/" in n]


def parse_emf(data: bytes) -> dict:
    """切 EMR 記錄；回傳型別計數、前 5 名、STRETCHDIBITS 內嵌 DIB 摘要。"""
    records: list[tuple[int, int, int]] = []
    counts: collections.Counter[int] = collections.Counter()
    off = 0
    bad = False
    while off + 8 <= len(data):
        typ, size = struct.unpack_from("<II", data, off)
        counts[typ] += 1
        records.append((off, typ, size))
        if size < 8 or off + size > len(data):
            bad = True
            break
        off += size
    top5 = [
        {"type": t, "name": emr_name(t), "count": c}
        for t, c in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[:5]
    ]
    dibs = []
    for o, t, s in records:
        if t != 81 or s < 80:
            continue
        xd, yd, xs, ys, cxs, cys = struct.unpack_from("<6i", data, o + 24)
        off_bmi, cb_bmi, off_bits, cb_bits = struct.unpack_from("<4I", data, o + 48)
        usage, rop, cx_dest, cy_dest = struct.unpack_from("<4I", data, o + 64)
        bmi = data[o + off_bmi:o + off_bmi + cb_bmi]
        w = h = bpp = comp = None
        if len(bmi) >= 20:
            w, h, _planes, bpp, comp = struct.unpack_from("<iiHHI", bmi, 4)
        dibs.append({
            "record_offset": o,
            "record_size": s,
            "dest_xy": [xd, yd],
            "dest_wh": [cx_dest, cy_dest],
            "src_wh": [cxs, cys],
            "usage": usage,
            "rop": f"0x{rop:08x}",
            "bmi_bytes": cb_bmi,
            "bits_bytes": cb_bits,
            "dib_wh": [w, h],
            "dib_bpp": bpp,
            "dib_compression": comp,  # 0=BI_RGB
        })
    frame_pt = None
    if len(data) >= 40:
        rf = struct.unpack_from("<4i", data, 24)
        frame_pt = [
            round((rf[2] - rf[0]) / 100 * 72 / 25.4, 1),
            round((rf[3] - rf[1]) / 100 * 72 / 25.4, 1),
        ]
    return {
        "bytes": len(data),
        "sha8": sha8(data),
        "header_magic": data[:4].hex(),
        "record_count": len(records),
        "record_end": off,
        "record_well_formed": (not bad) and off == len(data),
        "top5": top5,
        "record_counts": {emr_name(t): c for t, c in sorted(counts.items())},
        "rcl_frame_pt": frame_pt,
        "stretchdibits": dibs,
    }


def decode_dib(emf: bytes, rec_off: int) -> tuple[int, int, int, "np.ndarray"] | None:
    """把 EMR_STRETCHDIBITS 的 BI_RGB DIB 解成 top-down RGB uint8。
    支援 16bpp（5-5-5）、24bpp、32bpp；其他（含 BI_PNG/BI_JPEG/位元遮罩）
    一律回 None（探針不臆測）。"""
    if len(emf) < rec_off + 80:
        return None
    off_bmi, cb_bmi, off_bits, cb_bits = struct.unpack_from("<4I", emf, rec_off + 48)
    bmi = emf[rec_off + off_bmi:rec_off + off_bmi + cb_bmi]
    if len(bmi) < 20:
        return None
    w, h, _planes, bpp, comp = struct.unpack_from("<iiHHI", bmi, 4)
    if comp != 0 or bpp not in (16, 24, 32) or w <= 0 or h == 0:
        return None
    raw = emf[rec_off + off_bits:rec_off + off_bits + cb_bits]
    stride = ((w * bpp + 31) // 32) * 4
    if len(raw) < stride * abs(h):
        return None
    top_down = h < 0
    h = abs(h)
    rows = []
    order = range(h) if top_down else range(h - 1, -1, -1)
    for y in order:
        row = raw[y * stride:y * stride + stride]
        if bpp == 16:
            v = np.frombuffer(row[:w * 2], dtype="<u2").astype(np.uint32)
            rgb = np.stack([
                ((v >> 10) & 31) * 255 // 31,
                ((v >> 5) & 31) * 255 // 31,
                (v & 31) * 255 // 31,
            ], axis=1)
        elif bpp == 24:
            rgb = np.frombuffer(row[:w * 3], dtype=np.uint8).reshape(w, 3)[:, ::-1]
        else:
            rgb = np.frombuffer(row[:w * 4], dtype=np.uint8).reshape(w, 4)[:, :3][:, ::-1]
        rows.append(rgb.astype(np.uint8))
    return w, h, bpp, np.stack(rows, axis=0)


def colored_frac(arr: "np.ndarray") -> float:
    sat, _hue, _val = RL._sat_hue(arr)
    return float((sat >= SAT_MIN).mean())


def paste_rgb(canvas: "np.ndarray", img: "np.ndarray", x: float, y: float,
              w: float, h: float) -> None:
    """最近鄰貼圖（僅供色軸模擬；不追求畫質）。"""
    H, W = canvas.shape[:2]
    xi, yi = int(round(x)), int(round(y))
    wi, hi = max(int(round(w)), 1), max(int(round(h)), 1)
    xs = (np.arange(wi) * img.shape[1] // wi).clip(0, img.shape[1] - 1)
    ys = (np.arange(hi) * img.shape[0] // hi).clip(0, img.shape[0] - 1)
    tile = img[ys][:, xs]
    x0, y0 = max(xi, 0), max(yi, 0)
    x1, y1 = min(xi + wi, W), min(yi + hi, H)
    if x1 > x0 and y1 > y0:
        canvas[y0:y1, x0:x1] = tile[y0 - yi:y1 - yi, x0 - xi:x1 - xi]


# ───────────────────────── LO 側 ─────────────────────────

def soffice_residual() -> int:
    """WSL 下數殘留 soffice.exe；非 WSL／無 tasklist 回 -1（未知）。"""
    if shutil.which("tasklist") is None:
        return -1
    try:
        r = subprocess.run(["tasklist"], capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired):
        return -1
    return sum(1 for line in r.stdout.splitlines() if "soffice" in line.lower())


def cleanup_soffice() -> int:
    """任務紀律：轉完確認殘留為 0；有殘留才 taskkill //F //IM soffice.exe。"""
    n = soffice_residual()
    if n > 0 and shutil.which("taskkill"):
        subprocess.run(["taskkill", "//F", "//IM", "soffice.exe"],
                       capture_output=True, timeout=60)
        n2 = soffice_residual()
        print(f"（收場：殘留 soffice {n} → taskkill 後 {n2}）", flush=True)
        return n2
    return n


def lo_convert(docx: pathlib.Path, out: pathlib.Path) -> pathlib.Path | None:
    prog = "/mnt/c/Program Files/LibreOffice/program"
    if pathlib.Path(prog).exists():
        os.environ["PATH"] = prog + os.pathsep + os.environ.get("PATH", "")
    soffice = RL.find_soffice()
    RL.kill_stale_lo()
    scratch, profile, _tag = RL.lo_scratch(soffice)
    pdf = RL.lo_to_pdf(soffice, docx.resolve(), scratch, profile, timeout=180)
    cleanup_soffice()
    if pdf is None:
        return None
    out.mkdir(parents=True, exist_ok=True)
    dst = out / f"{docx.stem}.pdf"
    shutil.copyfile(pdf, dst)
    return dst


def analyze_pdf(pdf: pathlib.Path) -> dict:
    d = pymupdf.open(pdf)
    pages = []
    try:
        for i, p in enumerate(d):
            rect = p.rect
            page_area = rect.width * rect.height
            drawings = p.get_drawings()
            fills = [x for x in drawings if x.get("fill")]
            colored_fills = [
                x for x in fills
                if tuple(round(c, 4) for c in x["fill"]) not in ((0, 0, 0), (1, 1, 1))
            ]

            def area(r):
                return max(0.0, r.width) * max(0.0, r.height)

            with np.errstate(all="ignore"):
                arr = RL._pix_array(p)
            info = p.get_image_info()
            # get_image_info 在部分 PyMuPDF 版本不帶 xref，改由 get_images(full)
            # 以自然寬高對回 xref（同頁同尺寸多圖時取先見；本語料不衝突）。
            by_wh = {}
            for im0 in p.get_images(full=True):
                by_wh.setdefault((im0[2], im0[3]), im0[0])
            imgs = []
            for im in info:
                item = {
                    "bbox_pt": [round(v, 1) for v in im["bbox"]],
                    "nat_wh": [im.get("width"), im.get("height")],
                    "colorspace": im.get("colorspace"),
                    "bpc": im.get("bpc"),
                }
                xref = by_wh.get((im.get("width"), im.get("height")))
                if xref:
                    try:
                        ex = d.extract_image(xref)
                        pim = pymupdf.Pixmap(ex["image"])
                        a2 = np.frombuffer(pim.samples, dtype=np.uint8).reshape(
                            pim.height, pim.width, pim.n)
                        item["format"] = ex["ext"]
                        item["pixel_colored_frac"] = round(
                            colored_frac(a2[..., :3]) if pim.n >= 3 else 0.0, 4)
                    except Exception:
                        pass
                imgs.append(item)
            pages.append({
                "page": i,
                "w_pt": round(rect.width, 1),
                "h_pt": round(rect.height, 1),
                "text_chars": len(p.get_text()),
                "drawings": len(drawings),
                "fills": len(fills),
                "colored_fills": len(colored_fills),
                "fill_area_frac": round(sum(area(x["rect"]) for x in fills) / page_area, 4),
                "colored_fill_area_frac": round(
                    sum(area(x["rect"]) for x in colored_fills) / page_area, 4),
                "raster_colored_frac": round(colored_frac(arr), 4),
                "images": imgs,
            })
    finally:
        d.close()
    return {"pdf": str(pdf), "pages": pages}


# ───────────────────────── 我方側 ─────────────────────────

def quire_render(docx: pathlib.Path, out_root: pathlib.Path) -> pathlib.Path:
    render_dir = out_root / docx.stem
    r = subprocess.run(
        ["cargo", "run", "-q", "-p", "quire-cli", "--",
         "render", str(docx), "--out", str(render_dir)],
        capture_output=True, text=True, cwd=REPO, timeout=300)
    if r.returncode != 0:
        raise RuntimeError(f"quire render 失敗：{r.stderr.strip()[:300]}")
    return render_dir


def quire_dump(docx: pathlib.Path) -> list[dict]:
    r = subprocess.run(
        ["cargo", "run", "-q", "-p", "quire-cli", "--", "dump", str(docx)],
        capture_output=True, text=True, cwd=REPO, timeout=300)
    r.check_returncode()
    return json.loads(r.stdout)


def analyze_ours(render_dir: pathlib.Path, dump: list[dict]) -> dict:
    svgs = sorted(render_dir.glob("*-p*.svg"))
    pages = []
    for idx, svg in enumerate(svgs):
        text = svg.read_text(encoding="utf-8")
        rects = []
        for m in re.finditer(r"<rect\b([^>]*)/?>", text):
            attrs = dict(re.findall(r'(\w[\w:-]*)="([^"]*)"', m.group(1)))
            if attrs.get("fill", "").lower() == "#f0f0f0":
                rects.append({
                    "x": float(attrs.get("x", 0)),
                    "y": float(attrs.get("y", 0)),
                    "w": float(attrs.get("width", 0)),
                    "h": float(attrs.get("height", 0)),
                    "fill": attrs.get("fill"),
                })
        rules = []
        for m in re.finditer(r"<line\b([^>]*)/?>", text):
            attrs = dict(re.findall(r'(\w[\w:-]*)="([^"]*)"', m.group(1)))
            if attrs.get("stroke", "").lower() == "#cccccc":
                rules.append({
                    "x1": float(attrs.get("x1", 0)), "y1": float(attrs.get("y1", 0)),
                    "x2": float(attrs.get("x2", 0)), "y2": float(attrs.get("y2", 0)),
                    "stroke": attrs.get("stroke"),
                })
        pages.append({
            "page": idx,
            "svg": svg.name,
            "placeholder_rects": rects,
            "placeholder_border_rules": rules,
            "image_elements": len(re.findall(r"<image\b", text)),
            "text_elements": len(re.findall(r"<text\b", text)),
            "colors": sorted(set(re.findall(r'fill="(#[0-9a-fA-F]+)"', text))),
        })
    items = []
    for pg in dump:
        keep = []
        for it in pg["items"]:
            kind = next(iter(it))
            if kind in ("Rect", "Image", "Rule"):
                keep.append(it)
        items.append({"page": pg["page"], "non_text_items": keep})
    return {"render_dir": str(render_dir), "pages": pages, "dump": items}


# ───────────────────────── 全語料掃描 ─────────────────────────

def corpus_scan(root: pathlib.Path) -> dict:
    import zipfile
    docx = sorted(root.rglob("*.docx"))
    media_emf = []
    thumb_emf = []
    media_wmf = []
    thumb_wmf = []
    for p in docx:
        try:
            z = zipfile.ZipFile(p)
        except Exception:
            continue
        for n in z.namelist():
            low = n.lower()
            if not low.endswith((".emf", ".wmf")):
                continue
            size = z.getinfo(n).file_size
            row = {"file": str(p.relative_to(REPO)), "part": n, "bytes": size}
            if low.endswith(".emf"):
                (thumb_emf if n.startswith("docProps/") else media_emf).append(row)
            else:
                (thumb_wmf if n.startswith("docProps/") else media_wmf).append(row)
    # media EMF 逐筆判 DIB 可讀性
    dib_ok = 0
    for row in media_emf:
        z = zipfile.ZipFile(REPO / row["file"])
        info = parse_emf(z.read(row["part"]))
        row["stretchdibits"] = len(info["stretchdibits"])
        row["rcl_frame_pt"] = info["rcl_frame_pt"]
        row["sha8"] = info["sha8"]
        if info["stretchdibits"]:
            dib_ok += 1
    media_emf.sort(key=lambda r: -r["bytes"])
    media_wmf.sort(key=lambda r: -r["bytes"])

    def summary(rows):
        files = {r["file"] for r in rows}
        return {
            "parts": len(rows),
            "files": len(files),
            "bytes": sum(r["bytes"] for r in rows),
            "max_bytes": max((r["bytes"] for r in rows), default=0),
            "file_frac": round(len(files) / len(docx), 4) if docx else 0.0,
        }

    union = {r["file"] for r in media_emf} | {r["file"] for r in media_wmf}
    return {
        "docx_total": len(docx),
        "media_emf": summary(media_emf),
        "thumb_emf": summary(thumb_emf),
        "media_wmf": summary(media_wmf),
        "thumb_wmf": summary(thumb_wmf),
        "media_emf_dib_readable_parts": dib_ok,
        "media_emf_dib_readable_files": len({
            r["file"] for r in media_emf if r["stretchdibits"]}),
        "media_emf_vector_only_parts": len(media_emf) - dib_ok,
        "media_emf_vector_only_files": len({
            r["file"] for r in media_emf if not r["stretchdibits"]}),
        "media_emf_or_wmf_union_files": len(union),
        "inventory_emf": media_emf,
        "inventory_wmf": media_wmf,
    }


# ───────────────────────── DIB 色軸模擬 ─────────────────────────

def simulate_route_b(docx: pathlib.Path, media: list[tuple[str, bytes]],
                     ours: dict, lo: dict) -> dict:
    """模擬路線 (b)：只把 EMF 內嵌 DIB 當圖片貼回，vector 不動。
    fdo 另計「EMF＋PNG」（PNG 是獨立文字框內嵌圖缺失，不算 EMF 功勞）。"""
    if not lo.get("pages"):
        return {}
    pdf = pymupdf.open(lo["pdf"])
    try:
        lo_arr = RL._pix_array(pdf[0])
    finally:
        pdf.close()
    scale = 150 / 72

    dibs = []
    png = None
    for name, data in media:
        low = name.lower()
        if low.endswith(".emf"):
            info = parse_emf(data)
            for rec in info["stretchdibits"]:
                dec = decode_dib(data, rec["record_offset"])
                if dec:
                    w, h, bpp, arr = dec
                    dibs.append({
                        "part": name, "wh": [w, h], "bpp": bpp,
                        "colored_frac": round(colored_frac(arr), 4),
                        "rcl_frame_pt": info["rcl_frame_pt"],
                        "arr": arr,
                    })
        elif low.endswith(".png"):
            pim = pymupdf.Pixmap(data)
            a = np.frombuffer(pim.samples, dtype=np.uint8).reshape(
                pim.height, pim.width, pim.n)[..., :3].copy()
            png = {"part": name, "wh": [pim.width, pim.height], "arr": a}

    page = lo["pages"][0]
    lo_imgs = page["images"]
    # 用自然寬高把 DIB 對到 LO PDF 的圖；PNG 亦然。
    def match_box(wh):
        for im in lo_imgs:
            if im.get("nat_wh") == wh:
                b = im["bbox_pt"]
                return [b[0], b[1], b[2] - b[0], b[3] - b[1]]
        return None

    ours_rects = ours["pages"][0]["placeholder_rects"] if ours.get("pages") else []
    ours_rects = sorted(ours_rects, key=lambda r: r["y"])

    def render_case(items):
        c = np.full_like(lo_arr, 255)
        for arr, (x, y, w, h) in items:
            paste_rgb(c, arr, x * scale, y * scale, w * scale, h * scale)
        return {
            "coverage": round(colored_frac(c), 4),
            "score_color": round(RL.score_color(lo_arr, c), 3),
        }

    cases = {"current_gray_placeholder": {
        "coverage": 0.0,
        "score_color": round(RL.score_color(lo_arr, np.full_like(lo_arr, 255)), 3),
    }}
    lo_boxes = []
    for d in dibs:
        box = match_box(d["wh"])
        lo_boxes.append(box)
    # (1) DIB 貼在我方佔位框
    items = []
    for d, box in zip(dibs, lo_boxes):
        if box is not None and ours_rects:
            r = ours_rects[min(len(items), len(ours_rects) - 1)]
            items.append((d["arr"], (r["x"], r["y"], r["w"], r["h"])))
        elif box is not None:
            items.append((d["arr"], tuple(box)))
    if items:
        cases["dib_only_at_our_rect"] = render_case(items)
    # (2) DIB 依 rclFrame 尺寸貼在我方錨點
    items2 = []
    for d, box in zip(dibs, lo_boxes):
        if box is None or not ours_rects:
            continue
        r = ours_rects[min(len(items2), len(ours_rects) - 1)]
        w, h = d.get("rcl_frame_pt") or [r["w"], r["h"]]
        items2.append((d["arr"], (r["x"], r["y"], w, h)))
    if items2:
        cases["dib_only_rclframe_at_our_anchor"] = render_case(items2)
    # (3) DIB 貼在 LO 實測框（幾何上限）
    items3 = [(d["arr"], tuple(box)) for d, box in zip(dibs, lo_boxes) if box]
    if items3:
        cases["dib_only_at_lo_box"] = render_case(items3)
    # (4) DIB＋PNG（PNG 貼 LO 框；fdo 專屬）
    if png and lo_boxes:
        box = match_box(png["wh"])
        if box:
            cases["dib_plus_png_at_lo_box"] = render_case(
                items3 + [(png["arr"], tuple(box))])
    return {
        "dibs": [{k: v for k, v in d.items() if k != "arr"} for d in dibs],
        "dib_lo_boxes_pt": lo_boxes,
        "sim_cases": cases,
    }


# ───────────────────────── 主流程 ─────────────────────────

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="/tmp/p5-emf-probe")
    ap.add_argument("--ours", default="/tmp/p5-ours")
    ap.add_argument("--skip-lo", action="store_true")
    ap.add_argument("--skip-render", action="store_true")
    a = ap.parse_args()
    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)

    result = {
        "baseline": "8271675",
        "targets": {},
        "corpus": corpus_scan(REPO / "corpus"),
    }

    for docx in TARGETS:
        key = docx.stem
        print(f"\n=== {docx.relative_to(REPO)} ===")
        media = docx_media(docx)
        parts = []
        for name, data in sorted(media, key=lambda kv: -len(kv[1])):
            ext = pathlib.Path(name).suffix.lower()
            entry = {"part": name, "bytes": len(data), "sha8": sha8(data)}
            if ext == ".emf":
                entry.update(parse_emf(data))
            parts.append(entry)
        entry = {"media_parts": parts}

        for p in parts:
            print(f"  {p['part']}  {p['bytes']} B  sha256:{p['sha8']}", end="")
            if "record_count" in p:
                print(f"  records={p['record_count']} well_formed={p['record_well_formed']}")
                print("     top5:", ", ".join(
                    f"{t['name']}×{t['count']}" for t in p["top5"]))
                for d in p["stretchdibits"]:
                    print(f"     STRETCHDIBITS DIB {d['dib_wh']} {d['dib_bpp']}bpp "
                          f"comp={d['dib_compression']} dest={d['dest_xy']} "
                          f"{d['dest_wh']} bits={d['bits_bytes']}")
                if not p["stretchdibits"]:
                    print("     （無 STRETCHDIBITS：純 vector／text EMF）")
            else:
                print()

        if not a.skip_lo:
            pdf = lo_convert(docx, out / "lo")
            if pdf:
                entry["lo"] = analyze_pdf(pdf)
                for pg in entry["lo"]["pages"]:
                    print(f"  LO p{pg['page']} {pg['w_pt']}×{pg['h_pt']}pt "
                          f"text={pg['text_chars']} drawings={pg['drawings']} "
                          f"fills={pg['fills']}(彩色{pg['colored_fills']}) "
                          f"fill={pg['fill_area_frac']:.4f} "
                          f"彩fill={pg['colored_fill_area_frac']:.4f} "
                          f"彩覆蓋={pg['raster_colored_frac']:.4f} "
                          f"images={len(pg['images'])}")
                    for im in pg["images"]:
                        print(f"     img {im['bbox_pt']} 原尺寸{im['nat_wh']} "
                              f"{im.get('format', '?')} "
                              f"彩={im.get('pixel_colored_frac')}")
            else:
                entry["lo"] = None
                print("  LO 轉檔失敗")

        if not a.skip_render:
            render_dir = quire_render(docx, pathlib.Path(a.ours))
            dump = quire_dump(docx)
            entry["ours"] = analyze_ours(render_dir, dump)
            for pg in entry["ours"]["pages"]:
                print(f"  OW p{pg['page']} 灰佔位 rect={pg['placeholder_rects']} "
                      f"<image>={pg['image_elements']} 色={pg['colors']}")
            if entry.get("lo"):
                entry["route_b_sim"] = simulate_route_b(docx, media, entry["ours"],
                                                       entry["lo"])
                sim = entry["route_b_sim"].get("sim_cases", {})
                for name, val in sim.items():
                    print(f"  模擬 {name}: 彩覆蓋={val['coverage']:.4f} "
                          f"色軸={val['score_color']:.3f}")
        result["targets"][key] = entry

    (out / "p5_emf_probe.json").write_text(
        json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")

    c = result["corpus"]
    print("\n=== 全語料掃描 ===")
    print(f"  docx {c['docx_total']} 檔")
    for k, label in (("media_emf", "media EMF"), ("media_wmf", "media WMF"),
                     ("thumb_emf", "docProps thumbnail EMF"),
                     ("thumb_wmf", "docProps thumbnail WMF")):
        s = c[k]
        print(f"  {label}: {s['parts']} parts / {s['files']} 檔 / "
              f"{s['bytes']} B / 最大 {s['max_bytes']} B / 佔全語料 {s['file_frac']:.2%}")
    print(f"  media EMF 含 STRETCHDIBITS：{c['media_emf_dib_readable_parts']} parts / "
          f"{c['media_emf_dib_readable_files']} 檔；純 vector/text："
          f"{c['media_emf_vector_only_parts']} parts / "
          f"{c['media_emf_vector_only_files']} 檔")
    print(f"  media EMF|WMF 聯集：{c['media_emf_or_wmf_union_files']} 檔")
    print(f"\nJSON → {out / 'p5_emf_probe.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
