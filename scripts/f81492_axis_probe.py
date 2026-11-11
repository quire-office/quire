#!/usr/bin/env python3
"""f81492_axis_probe — fdo81492 最差榜第 1 逐軸歸因探針（report-only，唯讀，不動引擎）。

對應 docs/fdo81492-diag.md。流程：
  1) 我方 render（SVG＋pagebreaks.json）＋ dump（顯示列表 JSON）
  2) LO 真值：Linux soffice 轉 PDF（專屬 profile，一檔一轉），pymupdf 實讀
  3) 四軸對照（第 1 頁＋全文）：
       文字軸：LO 全文 token 集 vs 我方 dump token 集差集；
               前段＋後段拼接命中（空白剝除）＝假缺，剔除後再報
       色彩軸：非白像素色直方交集（110 dpi 光柵化，白＝>=250）
       圖片軸：實繪圖數（get_images(full=True)＋get_image_rects）
       結構軸：LO get_drawings 線段集合 vs 我方 dump Rect/Rule 項

冪等：產物全部落 /tmp（/tmp/f81492-ow、/tmp/lof81492、/tmp/loprof-f81492-diag），
重跑直接覆寫；不寫 repo、不寫 corpus。

需求：pymupdf 只在 /usr/bin/python3（本腳本即以該直譯器撰寫，勿用 3.12+ venv 跑）。
"""
import collections
import json
import pathlib
import re
import subprocess
import sys
import os

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent
DOCX = REPO / "corpus" / "ooxmlexport" / "fdo81492.docx"
OW_DIR = pathlib.Path("/tmp/f81492-ow")
LO_DIR = pathlib.Path("/tmp/lof81492")
LO_PDF = LO_DIR / "fdo81492.pdf"
LO_PROFILE = pathlib.Path("/tmp/loprof-f81492-diag")
SOFFICE = pathlib.Path.home() / ".local" / "lo-linux" / "opt" / "libreoffice25.8" / "program" / "soffice"
DPI = 110
WHITE_MIN = 250

try:
    import pymupdf
except ImportError:
    sys.exit("需要 pymupdf（只在 /usr/bin/python3）：/usr/bin/python3 scripts/f81492_axis_probe.py")


def run(cmd, **kw):
    print("+", " ".join(str(c) for c in cmd))
    return subprocess.run(cmd, check=True, **kw)


def step_ours():
    OW_DIR.mkdir(parents=True, exist_ok=True)
    run(["cargo", "run", "-q", "-p", "quire-cli", "--", "render", str(DOCX), "--out", str(OW_DIR)], cwd=REPO)
    out = subprocess.run(
        ["cargo", "run", "-q", "-p", "quire-cli", "--", "dump", str(DOCX)],
        cwd=REPO, check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def step_lo():
    LO_DIR.mkdir(parents=True, exist_ok=True)
    LO_PDF.unlink(missing_ok=True)
    env = dict(os.environ, HOME=str(LO_PROFILE))
    run([str(SOFFICE), "--headless",
         f"-env:UserInstallation=file://{LO_PROFILE}",
         "--convert-to", "pdf", "--outdir", str(LO_DIR), str(DOCX)], env=env)
    if not LO_PDF.is_file() or LO_PDF.stat().st_size == 0:
        sys.exit(f"LO 轉檔失敗：{LO_PDF} 不存在或零大小")
    print(f"LO PDF ok: {LO_PDF} ({LO_PDF.stat().st_size} bytes)")


def page_text_lines(page):
    raw = page.get_text("rawdict")
    lines = []
    for b in raw.get("blocks", []):
        for l in b.get("lines", []):
            lines.append("".join(c.get("c", "") for s in l.get("spans", []) for c in s.get("chars", [])))
    return lines


def toks(s):
    return re.findall(r"\S+", s)


def axis_text(lo_doc, ow):
    lo_pages = ["\n".join(page_text_lines(p)) for p in lo_doc]
    ow_pages = [[i["Text"]["text"] for i in p["items"] if "Text" in i] for p in ow]
    lo_full = " ".join(lo_pages)
    ow_full = " ".join(" ".join(ts) for ts in ow_pages)
    lo_tok = collections.Counter(toks(lo_full))
    ow_tok = collections.Counter(toks(ow_full))
    lo_sq = lo_full.replace(" ", "")
    ow_sq = ow_full.replace(" ", "")
    only_lo_raw = lo_tok - ow_tok
    only_ow_raw = ow_tok - lo_tok
    # 「前段＋後段拼接命中」剔除：空白剝除後落在對方全文裡＝切分粒度差，非缺字
    only_lo = {t: c for t, c in only_lo_raw.items() if t.replace(" ", "") not in ow_sq}
    only_ow = {t: c for t, c in only_ow_raw.items() if t.replace(" ", "") not in lo_sq}
    print("\n== 文字軸 ==")
    for i, lp in enumerate(lo_pages):
        lt = collections.Counter(toks(lp))
        print(f"LO p{i}: 行數={len(page_text_lines(lo_doc[i]))} token={sum(lt.values())}")
    print(f"OW 頁數={len(ow)}，全文 token={sum(ow_tok.values())}")
    print("LO-only raw:", dict(only_lo_raw))
    print("OW-only raw:", dict(only_ow_raw))
    print("LO-only 拼接過濾後:", only_lo)
    print("OW-only 拼接過濾後:", only_ow)
    # 逐頁：LO p0 應為「我方 p0 的缺頁子集」之檢核
    lo_p0 = collections.Counter(toks(lo_pages[0]))
    print("LO p0 token:", dict(lo_p0))
    return {"lo_pages": lo_pages, "only_lo": only_lo, "only_ow": only_ow}


def nonwhite_hist(pix):
    hist = collections.Counter()
    s, n = pix.samples, pix.n
    for i in range(0, len(s), n):
        r, g, b = s[i], s[i + 1], s[i + 2]
        if r >= WHITE_MIN and g >= WHITE_MIN and b >= WHITE_MIN:
            continue
        hist[(r, g, b)] += 1
    return hist


def axis_color(lo_doc, ow_svg):
    print("\n== 色彩軸（非白像素色直方, dpi=%d）==" % DPI)
    ow_doc = pymupdf.open(str(ow_svg))
    ho = nonwhite_hist(ow_doc[0].get_pixmap(dpi=DPI))
    hues = []
    for i, p in enumerate(lo_doc):
        h = nonwhite_hist(p.get_pixmap(dpi=DPI))
        hues.append(h)
        inter = sum(min(ho[k], h[k]) for k in ho.keys() & h.keys())
        print(f"OW p0 vs LO p{i}: 非白 px ow={sum(ho.values())} lo={sum(h.values())} "
              f"intersect={inter} (vs min={inter / max(1, min(sum(ho.values()), sum(h.values()))):.3f})")
    union = collections.Counter()
    for h in hues:
        union.update(h)
    inter = sum(min(ho[k], union[k]) for k in ho.keys() & union.keys())
    print(f"OW p0 vs LO 全頁聯集: intersect={inter} (vs ow={inter / max(1, sum(ho.values())):.3f})")
    ow_doc.close()
    return {"ow_px": sum(ho.values()), "lo_union_px": sum(union.values()), "intersect_union": inter}


def axis_image(lo_doc, ow):
    print("\n== 圖片軸 ==")
    total = 0
    for i, p in enumerate(lo_doc):
        drawn = 0
        for img in p.get_images(full=True):
            drawn += len(p.get_image_rects(img[0]))
        print(f"LO p{i}: get_images(full)={len(p.get_images(full=True))} 實繪 rect 數={drawn}")
        total += drawn
    ow_imgs = sum(1 for p in ow for i in p["items"] if "Image" in i)
    print(f"OW dump Image 項={ow_imgs}")
    return {"lo_drawn": total, "ow_items": ow_imgs}


def axis_structure(lo_doc, ow):
    print("\n== 結構軸（線段集）==")
    lo_lines = []
    for i, p in enumerate(lo_doc):
        for d in p.get_drawings():
            for it in d.get("items", []):
                if it[0] == "l":
                    p0, p1 = it[1], it[2]
                    lo_lines.append((i, round(p0.x, 1), round(p0.y, 1), round(p1.x, 1), round(p1.y, 1),
                                     d.get("color"), d.get("width")))
    for ln in lo_lines:
        print("LO line:", ln)
    ow_rules = [(p["page"], k, i) for p in ow for i, it in enumerate(p["items"])
                for k in ("Rule", "Rect") if k in it]
    print(f"OW dump Rule/Rect 項={ow_rules}")
    return {"lo_lines": lo_lines, "ow_rule_rect": ow_rules}


def main():
    if not DOCX.is_file():
        sys.exit(f"找不到真實路徑: {DOCX}（先 find corpus -iname 'fdo81492*'）")
    print(f"DOCX = {DOCX}")
    ow = step_ours()
    ow_svg = OW_DIR / "fdo81492-p0.svg"
    pagebreaks = json.loads((OW_DIR / "pagebreaks.json").read_text())
    counts = [collections.Counter(next(iter(i)) for i in p["items"]) for p in ow]
    print("\n== 我方 ==")
    print("pagebreaks.json:", json.dumps(pagebreaks, ensure_ascii=False))
    for i, c in enumerate(counts):
        print(f"OW p{i}: 尺寸={ow[i]['width_pt']}×{ow[i]['height_pt']}pt 型別計數={dict(c)} 總項={sum(c.values())}")
    print("第 2 頁我方存在與否:", "存在" if len(ow) > 1 else "不存在")

    step_lo()
    lo_doc = pymupdf.open(str(LO_PDF))
    print("\n== LO ==")
    print("pages:", lo_doc.page_count)
    for i, p in enumerate(lo_doc):
        lines = page_text_lines(p)
        drawings = p.get_drawings()
        n_line_items = sum(1 for d in drawings for it in d.get("items", []) if it[0] == "l")
        drawn = 0
        for img in p.get_images(full=True):
            drawn += len(p.get_image_rects(img[0]))
        print(f"LO p{i}: 尺寸={p.rect.width:.2f}×{p.rect.height:.2f}pt 文字行數={len(lines)} "
              f"drawings={len(drawings)} 線段item={n_line_items} 實繪圖={drawn}")
        for ln in lines:
            print("   ", repr(ln))

    axis_text(lo_doc, ow)
    axis_color(lo_doc, ow_svg)
    axis_image(lo_doc, ow)
    axis_structure(lo_doc, ow)
    lo_doc.close()


if __name__ == "__main__":
    main()
