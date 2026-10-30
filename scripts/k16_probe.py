#!/usr/bin/env python3
"""k16_probe —— K16-diag 量測腳本（唯讀歸因，不動引擎；report-only 卡 t_641a1261）。

三個子命令，逐一對應報告三節：
  ooxml   指定 docx：印出 sectPr／header-footer 引用／彩色宣告（w:color/w:fill/w:shd/
          drawing/pict）等 OOXML 側宣稱，供「LO 為何有此色」對账。
  color   LO PDF × 我方 SVG 逐色直方圖 diff：24 bin 面積占比逐欄對照，列出 deficit
          貢獻最大的 bin（呼應 J3 `color_hist`/`score_color`，含 SAT_MIN/COLOR_FLOOR）。
  missing token 對账 + 指定詞在 OOXML document.xml 的落點（父元素鏈），判真缺字/切片假影。
  wc048   WC048 殘差清單：我方 dump 顯示列表 vs LO PDF 文字行（y 座標，pt=px×72/DPI），
          扣除 K15（Textbox3 位置）/K12（填充色鏈）已認領項後的剩餘構成分解。

LO 真值一律 pymupdf 實測（lo_to_pdf + get_pixmap/get_text）；DPI=150（J3 口徑），
pt = px×72/150。跑完由 render_lab.kill_own_lo 清 LO 殭屍。
"""
import argparse
import json
import pathlib
import re
import sys
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent
sys.path.insert(0, str(HERE))
import render_lab as RL  # noqa: E402  (复用 lo_to_pdf/_pix_array/color_hist/…，零修改)
import pymupdf  # noqa: E402
import numpy as np  # noqa: E402

DPI = RL.J3_DPI  # 150：與 J3 三軸同口徑
HUE_NAMES = ["紅", "橙紅", "橙", "黃", "黃綠", "綠", "青綠", "青", "藍青", "藍", "紫藍", "紫紅"]


def px2pt(v: float) -> float:
    return v * 72.0 / DPI


# ── OOXML 宣稱掃描 ────────────────────────────────────────────────────────

def scan_docx(docx: pathlib.Path, word: str | None = None) -> None:
    z = zipfile.ZipFile(docx)
    names = z.namelist()
    print(f"# OOXML 掃描：{docx}")
    print(f"parts: {[n for n in names if n.startswith('word/') and n.endswith('.xml')]}")
    media = [n for n in names if n.startswith("word/media/")]
    print(f"media: {media}")
    doc = z.read("word/document.xml").decode("utf-8", "ignore")
    print(f"sectPr 數: {len(re.findall(r'<w:sectPr\b', doc))}")
    for m in re.finditer(r"<w:(headerReference|footerReference)\b[^>]*/>", doc):
        print("  ", m.group(0))
    print("  titlePg:", re.findall(r"<w:titlePg\b[^>]*/>", doc))
    try:
        st = z.read("word/settings.xml").decode("utf-8", "ignore")
        print("  evenAndOddHeaders:", "evenAndOddHeaders" in st)
    except KeyError:
        pass
    # 每 part 的彩色宣稱
    for n in names:
        if not (n.startswith("word/") and n.endswith(".xml")):
            continue
        d = z.read(n).decode("utf-8", "ignore")
        cols = sorted(set(re.findall(r'(?:w:color|w:fill)="([0-9A-Fa-f]{6})"', d))
                      | set(re.findall(r'w:fill="(theme[a-zA-Z]+)"', d)))
        nshd = len(re.findall(r"<w:shd\b", d))
        ndraw = len(re.findall(r"<w:drawing\b", d))
        npict = len(re.findall(r"<w:pict\b", d))
        if cols or nshd or ndraw or npict:
            print(f"  {n}: 色宣告={cols[:14]}{'…' if len(cols) > 14 else ''} "
                  f"shd={nshd} drawing={ndraw} pict={npict}")
    if word:
        locate_word(doc, word)


def locate_word(doc: str, word: str) -> None:
    """詞在 document.xml 的落點：字元偏移＋所在 <w:t> 的父元素鏈（節點棧回溯）。"""
    print(f"\n## 詞定位：{word!r}")
    for mo in re.finditer(re.escape(word), doc):
        i = mo.start()
        # 父元素鏈：向前掃標籤棧
        stack: list[str] = []
        for tm in re.finditer(r"<(/?)(w:[A-Za-z]+)[^>]*?(/)?>", doc[:i]):
            close, tag, selfclose = tm.groups()
            if selfclose:
                continue
            if close:
                if stack and stack[-1] == tag:
                    stack.pop()
            else:
                stack.append(tag)
        # 所在 w:t 起點與 w:r/w:p 邊界
        tstart = doc.rfind("<w:t", 0, i)
        tend = doc.find("</w:t>", i)
        ctx = doc[tstart:tend + 6] if 0 <= tstart < tend else "(不在任何 <w:t> 內)"
        # 所在 w:p 全文（截 500）
        pstart = doc.rfind("<w:p ", 0, i)
        if pstart < 0:
            pstart = doc.rfind("<w:p>", 0, i)
        pend = doc.find("</w:p>", i)
        ptxt = re.sub(r"<[^>]+>", "", doc[pstart:pend]) if 0 <= pstart < pend else "?"
        chain = " > ".join(stack[-9:])
        print(f"- byte {i}: 鏈 {chain}")
        print(f"  run: {ctx[:200]}")
        print(f"  段落文字: {ptxt[:240]}")


# ── 逐色直方圖 diff ───────────────────────────────────────────────────────

def bin_labels() -> list[str]:
    return [f"{HUE_NAMES[h]}{'亮' if v else '暗'}" for h in range(12) for v in (0, 1)]


def page_pair_color_diff(lo_pdf: pathlib.Path, ow_svg: pathlib.Path, page: int) -> None:
    d = pymupdf.open(lo_pdf)
    lo_arr = RL._pix_array(d[page])
    d.close()
    s = pymupdf.open(str(ow_svg))
    ow_arr = RL._pix_array(s[0])
    s.close()
    a, b = RL.color_hist(lo_arr), RL.color_hist(ow_arr)
    sc = RL.score_color(lo_arr, ow_arr)
    tot = a.sum() + b.sum()
    print(f"## 逐色 diff（DPI={DPI}，bin=佔整頁像素比例）  score_color={sc:.3f}")
    print(f"彩色覆蓋率：LO={a.sum():.4f} 我方={b.sum():.4f}（COLOR_FLOOR={RL.COLOR_FLOOR}）")
    print("bin            LO       我方     min      max    |min-min|差")
    rows = sorted(range(24), key=lambda k: -(min(a[k], b[k])))
    labels = bin_labels()
    for k in rows:
        if a[k] < 1e-6 and b[k] < 1e-6:
            continue
        print(f"{labels[k]:<12}  {a[k]:.5f}  {b[k]:.5f}  {min(a[k], b[k]):.5f}  "
              f"{max(a[k], b[k]):.5f}  {a[k] - b[k]:+.5f}")
    inter, uni = np.minimum(a, b).sum(), np.maximum(a, b).sum()
    print(f"Σmin={inter:.5f} Σmax={uni:.5f} → 交集/並集={inter / uni:.4f}")
    # 缺口歸因：逐 bin 若把「我方該 bin 補到 LO 值」分數會到多少
    order = sorted(range(24), key=lambda k: -(max(a[k], b[k]) - min(a[k], b[k])))
    print("\n缺口貢獻排序（該 bin 若補齊，分數由 {0:.3f} 升至 …）".format(sc))
    for k in order[:6]:
        if max(a[k], b[k]) - min(a[k], b[k]) < 1e-5:
            continue
        c, dd = a.copy(), b.copy()
        if a[k] > b[k]:
            dd[k] = a[k]
        else:
            c[k] = b[k]
        up = np.minimum(c, dd).sum() / np.maximum(c, dd).sum()
        who = "我方缺" if a[k] > b[k] else "我方多發"
        print(f"  {labels[k]:<12} {who}  面積占頁 {abs(a[k] - b[k]):.5f}"
              f"（像素 {abs(a[k] - b[k]) * lo_arr.shape[0] * lo_arr.shape[1]:,.0f}px）"
              f" → 補齊後 {up:.3f}")
    # 連通域視角：哪一塊彩色造成的
    print("\n連通域（image_blobs：bbox px / pt / 填充密度）")
    for tag, arr in (("LO ", lo_arr), ("我方", ow_arr)):
        for (x0, y0, x1, y1), dens in RL.image_blobs(arr)[:8]:
            print(f"  {tag} ({px2pt(x0):.0f},{px2pt(y0):.0f})–({px2pt(x1):.0f},{px2pt(y1):.0f})pt"
                  f" 填充={dens:.2f} 面積占頁="
                  f"{(x1 - x0) * (y1 - y0) / (arr.shape[0] * arr.shape[1]):.4f}")


# ── 我方 dump 工具 ────────────────────────────────────────────────────────

def run_dump(docx: pathlib.Path) -> list[dict]:
    import subprocess
    r = subprocess.run(["cargo", "run", "-q", "-p", "quire-cli", "--", "dump", str(docx)],
                       capture_output=True, text=True, cwd=REPO, timeout=180)
    r.check_returncode()
    return json.loads(r.stdout)


# ── 主 ────────────────────────────────────────────────────────────────────

def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p1 = sub.add_parser("ooxml")
    p1.add_argument("docx")
    p1.add_argument("--word", default=None)
    p2 = sub.add_parser("color")
    p2.add_argument("lo_pdf")
    p2.add_argument("ow_svg")
    p2.add_argument("--page", type=int, default=1)
    p3 = sub.add_parser("missing")
    p3.add_argument("docx")
    p3.add_argument("lo_pdf")
    p3.add_argument("--words", default="")
    p4 = sub.add_parser("wc048")
    p4.add_argument("docx")
    p4.add_argument("lo_pdf")
    p5 = sub.add_parser("lo2pdf")
    p5.add_argument("docx", nargs="+")
    p5.add_argument("--out", required=True)
    p6 = sub.add_parser("tbxdump")
    p6.add_argument("docx")
    a = ap.parse_args()

    if a.cmd == "tbxdump":
        d = zipfile.ZipFile(a.docx).read("word/document.xml").decode("utf-8", "ignore")
        for m in re.finditer(r"<w:p\b[^>]*>", d):
            seg = d[m.start():d.find("</w:p>", m.start())]
            if "<w:anchor" in seg:
                t = "".join(re.findall(r"<w:t[^>]*>([^<]*)</w:t>", seg))
                name = (re.findall(r'<wp:docPr[^>]*name="([^"]*)"', seg)
                        + re.findall(r'v:shape id="([^"]*)"', seg))
                print(f"anchor段 byte{m.start()} 文字={t!r} shapes={name[:2]}")
        print("tblpPr:", re.findall(r"<w:tblpPr[^>]*/>", d))
        print("trHeight:", re.findall(r"<w:trHeight[^>]*/>", d))
        for m in re.finditer(r"<w:pBdr>.*?</w:pBdr>", d, re.S):
            print("pBdr:", re.sub(r"\s+", " ", m.group(0))[:260])
    elif a.cmd == "lo2pdf":
        soffice = RL.find_soffice()
        out = pathlib.Path(a.out)
        out.mkdir(parents=True, exist_ok=True)
        scratch, profile_uri, needle = RL.lo_scratch(soffice)
        RL.kill_stale_lo()
        import atexit
        atexit.register(RL.kill_own_lo, needle)
        for f in a.docx:
            f = pathlib.Path(f)
            pdf = RL.lo_to_pdf(soffice, f, scratch, profile_uri)
            if pdf is None:
                print(f"✗ LO 轉換失敗：{f}")
                continue
            dest = out / (f.stem + ".pdf")
            import shutil
            shutil.copyfile(pdf, dest)
            print(f"✓ {f.name} → {dest}")
    elif a.cmd == "ooxml":
        scan_docx(pathlib.Path(a.docx), a.word)
    elif a.cmd == "color":
        page_pair_color_diff(pathlib.Path(a.lo_pdf), pathlib.Path(a.ow_svg), a.page)
    elif a.cmd == "missing":
        d = pymupdf.open(a.lo_pdf)
        lo_text = "\n".join(p.get_text("text") for p in d)
        d.close()
        dump = run_dump(pathlib.Path(a.docx))
        ow_text = "\n".join(str(it["Text"]["text"])
                            for dl in dump for it in dl["items"] if "Text" in it)
        ta = RL.token_audit(lo_text, ow_text)
        print(f"true_missing={ta['true_missing']}  split_ok={ta['split_ok']}")
        for w in [w for w in a.words.split(",") if w]:
            doc = zipfile.ZipFile(a.docx).read("word/document.xml").decode("utf-8", "ignore")
            locate_word(doc, w)
            print(f"  我方 dump 含 {w!r}？ {w.casefold() in ow_text.casefold()}")
    elif a.cmd == "wc048":
        d = pymupdf.open(a.lo_pdf)
        pg = d[0]
        print(f"# WC048 LO p0 尺寸 {pg.rect.width:.1f}×{pg.rect.height:.1f}pt")
        words = pg.get_text("words")  # x0,y0,x1,y1,word
        rows = sorted(words, key=lambda w: (round(float(w[1]), 1), w[0]))
        print("LO 行（y=pt，依視覺序）：")
        last = None
        for w in rows:
            y = round(float(w[1]), 1)
            if y != last:
                print(f"\n  y={y:6.1f} x={px2pt(0):5.1f}", end="  ")
                last = y
            print(f"[{w[4]}]", end="")
        print()
        d.close()
        print("\n我方 dump 顯示列表：")
        for dl in run_dump(pathlib.Path(a.docx)):
            for it in dl["items"]:
                if "Text" in it:
                    print(f"  p{dl['page']} {RL.norm(it['Text']['text'])[:120]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
