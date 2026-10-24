#!/usr/bin/env python3
"""diag_pagination — HC031 分頁斷點位移診斷（唯讀，不改排版引擎）。

用法：
    python3 scripts/diag_pagination.py corpus/xopt/TestFiles_HC031-Complicated-Document.docx
"""
import os, sys, subprocess, json, re, pathlib, time, shutil

# 環境自舉：系統 python3 無 pymupdf 時改用已知的 python3。
try:
    import pymupdf
except ImportError:
    _cand = "/home/jerry/.hermes/tools/python-3.14.7+20260901-linux-x64/bin/python3"
    if os.path.exists(_cand) and sys.executable != _cand:
        os.execv(_cand, [_cand] + sys.argv)
    sys.exit("需要 pymupdf")

REPO = pathlib.Path(__file__).resolve().parent.parent
PYTHON = sys.executable

def find_soffice():
    for name in ("soffice", "libreoffice"):
        p = shutil.which(name)
        if p:
            return pathlib.Path(p)
    for cand in (
        "/mnt/c/Program Files/LibreOffice/program/soffice.exe",
        "/mnt/c/Program Files (x86)/LibreOffice/program/soffice.exe",
    ):
        p = pathlib.Path(cand)
        if p.exists():
            return p
    sys.exit("找不到 LibreOffice")

def win_scratch_dir():
    cands = []
    try:
        r = subprocess.run(["cmd.exe", "/c", "echo %TEMP%"], capture_output=True, timeout=15)
        w = r.stdout.decode("utf-8", errors="ignore").strip().strip('"') if r.returncode == 0 else ""
    except Exception:
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
    sys.exit("Windows 暫存目錄不可寫")

def lo_to_pdf(soffice, docx, scratch):
    src = docx
    out_arg, src_arg = scratch, src
    if str(soffice).endswith(".exe"):
        src = scratch / f"{docx.parent.name}__{docx.name}"
        shutil.copyfile(docx, src)
        r = subprocess.run(["wslpath", "-w", str(scratch)], capture_output=True, text=True)
        out_arg = pathlib.Path(r.stdout.strip().replace("\\", "/"))
        r = subprocess.run(["wslpath", "-w", str(src)], capture_output=True, text=True)
        src_arg = pathlib.Path(r.stdout.strip().replace("\\", "/"))
    prof = scratch / "profile"
    prof.mkdir(exist_ok=True)
    r = subprocess.run(["wslpath", "-w", str(prof)], capture_output=True, text=True)
    profw = r.stdout.strip().replace("\\", "/")
    profile_uri = "file:///" + profw.lstrip("/")
    stale = scratch / (src.stem + ".pdf")
    if stale.exists():
        stale.unlink()
    # kill stale
    if shutil.which("taskkill.exe"):
        subprocess.run(["taskkill.exe", "/IM", "soffice.bin.exe"], capture_output=True)
        subprocess.run(["taskkill.exe", "/IM", "soffice.exe"], capture_output=True)
        time.sleep(1.5)
    cmd = [str(soffice), f"-env:UserInstallation={profile_uri}", "--headless",
           "--norestore", "--convert-to", "pdf", "--outdir", str(out_arg), str(src_arg)]
    subprocess.run(cmd, capture_output=True, timeout=180)
    return stale if stale.exists() else None

def norm(s):
    return re.sub(r"\s+", " ", s).strip()

def parse_quire_paragraphs(docx):
    """解析 document.xml，回傳扁平段落文字列表（口徑對齊 quire：忽略 self-closing w:p）。"""
    import zipfile, xml.parsers.expat
    z = zipfile.ZipFile(docx)
    data = z.read("word/document.xml").decode("utf-8")
    data = re.sub(r"<w:p\b[^>]*/>", "", data)
    paras = []
    fb = [0]
    ac = []
    stack = []
    def lname(n): return n.split(":", 1)[1] if ":" in n else n
    def px(n): return n.split(":", 1)[0] if ":" in n else ""
    def start(name, attrs):
        ln = lname(name); pfx = px(name)
        if fb[0] > 0:
            if ln == "Fallback" and pfx == "mc":
                fb[0] += 1
            elif ln == "AlternateContent" and pfx == "mc":
                ac.append(fb[0])
            stack.append(("x", None))
            return
        if ln == "Fallback" and pfx == "mc":
            fb[0] += 1
            stack.append(("x", None))
            return
        if ln == "AlternateContent" and pfx == "mc":
            ac.append(fb[0])
            stack.append(("x", None))
            return
        if ln == "p" and pfx == "w":
            stack.append(("p", []))
            return
        stack.append(("x", None))
    def end(name):
        ln = lname(name); pfx = px(name)
        if not stack:
            return
        frame = stack.pop()
        if fb[0] > 0:
            if ln == "Fallback" and pfx == "mc":
                fb[0] = max(0, fb[0] - 1)
            elif ln == "AlternateContent" and pfx == "mc":
                if ac:
                    fb[0] = ac.pop()
            return
        if ln == "Fallback" and pfx == "mc":
            fb[0] = max(0, fb[0] - 1)
            return
        if ln == "AlternateContent" and pfx == "mc":
            if ac:
                fb[0] = ac.pop()
            return
        if frame and frame[0] == "p":
            paras.append("".join(frame[1]))
    def char(d):
        for i in range(len(stack) - 1, -1, -1):
            if stack[i] and stack[i][0] == "p":
                stack[i][1].append(d)
                return
    pr = xml.parsers.expat.ParserCreate()
    pr.StartElementHandler = start
    pr.EndElementHandler = end
    pr.CharacterDataHandler = char
    pr.Parse(data, True)
    return paras

def main():
    if len(sys.argv) < 2:
        print("用法: python3 scripts/diag_pagination.py <file.docx>", file=sys.stderr)
        sys.exit(2)
    docx = pathlib.Path(sys.argv[1])
    if not docx.is_absolute():
        docx = (REPO / docx).resolve()
    if not docx.exists():
        sys.exit(f"找不到檔案: {docx}")

    soffice = find_soffice()
    scratch = win_scratch_dir() if str(soffice).endswith(".exe") else pathlib.Path("/tmp") / "quire-lo"
    scratch.mkdir(parents=True, exist_ok=True)

    # LO PDF
    pdf = lo_to_pdf(soffice, docx, scratch)
    if pdf is None:
        sys.exit("LO 轉換失敗")
    import pymupdf
    d = pymupdf.open(str(pdf))
    lo_texts = [p.get_text("text") for p in d]
    d.close()

    # quire render + dump
    render_dir = scratch / "quire" / docx.stem
    render_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["cargo", "run", "-q", "-p", "quire-cli", "--", "render", str(docx),
         "--out", str(render_dir)],
        cwd=REPO, check=True, capture_output=True, timeout=300)
    dump_proc = subprocess.run(
        ["cargo", "run", "-q", "-p", "quire-cli", "--", "dump", str(docx)],
        cwd=REPO, check=True, capture_output=True, text=True, timeout=300)
    # dump not strictly needed; pagebreaks is enough
    pagebreaks = json.loads((render_dir / "pagebreaks.json").read_text(encoding="utf-8"))

    # quire paragraphs
    qparas = parse_quire_paragraphs(docx)

    # map para -> quire page
    para_page_quire = {}
    for b in pagebreaks:
        for pi in range(b["first_para"], b["last_para"] + 1):
            para_page_quire[pi] = b["page"]

    # map para -> LO page via sequential normalized search
    concat = ""
    page_of_char = []
    for i, t in enumerate(lo_texts):
        nt = norm(t)
        concat += nt + " "
        page_of_char += [i] * (len(nt) + 1)
    cursor = 0
    para_page_lo = {}
    for pi, ptext in enumerate(qparas):
        pt = norm(ptext).lower()
        if pt == "":
            para_page_lo[pi] = None
            continue
        pos = concat.lower().find(pt, cursor)
        if pos == -1:
            pos = concat.lower().find(pt)
        if pos == -1:
            para_page_lo[pi] = None
        else:
            para_page_lo[pi] = page_of_char[pos] if pos < len(page_of_char) else None
            cursor = pos + len(pt)

    # first divergence
    first_div = None
    for pi in range(len(qparas)):
        lp = para_page_lo.get(pi)
        qp = para_page_quire.get(pi)
        if lp is not None and qp is not None and lp != qp:
            first_div = pi
            break

    # alignment table (first 6 pages)
    print("=" * 78)
    print(f"檔案: {docx}")
    print(f"LO 頁數: {len(lo_texts)}  /  quire 頁數: {len(pagebreaks)} (body pages; SVG 含尾註頁)")
    print("=" * 78)
    print(f"{'頁':>3}  {'quire first_para':>16}  {'quire last_para':>15}  {'LO~para (估)':>14}")
    for b in pagebreaks[:6]:
        # LO page same index if exists
        lo_idx = b["page"]
        if lo_idx >= len(lo_texts):
            lo_hint = "(LO 無此頁)"
        else:
            # 空 LO 頁（para-level sectPr 真空頁）splitlines() 為 []——直接 [0] 會
            # IndexError（§4 懸#8）；取首個非空行，全空則明確標記跳過。
            first = next((ln for ln in lo_texts[lo_idx].splitlines() if norm(ln)), "")
            lo_hint = first[:40] if first else "(LO 本頁為空——跳過)"
        print(f"{b['page']:>3}  {b['first_para']:>16}  {b['last_para']:>15}  {lo_hint}")

    print()
    print("逐段落歸屬對照（前 25 段）:")
    print(f"{'para':>5}  {'quire頁':>6}  {'LO頁':>5}  {'文字'}")
    for pi in range(min(25, len(qparas))):
        qp = para_page_quire.get(pi, "?")
        lp = para_page_lo.get(pi)
        lp_s = str(lp) if lp is not None else "-"
        mark = "  <-- 首個分歧" if pi == first_div else ""
        print(f"{pi:>5}  {qp:>6}  {lp_s:>5}  {qparas[pi][:46].replace(chr(10),' ')}{mark}")

    if first_div is not None:
        print()
        print(f"首個分歧段落 index = {first_div}")
        print(f"quire 頁: {para_page_quire.get(first_div)}  /  LO 頁: {para_page_lo.get(first_div)}")
        print("段落文字:", qparas[first_div][:120])

        # XML snippet: find w:p containing endnoteReference w:id="1" (this file's para 17)
        import zipfile
        z = zipfile.ZipFile(docx)
        data = z.read("word/document.xml").decode("utf-8")
        # fallback: search for the paragraph text in XML
        snippet = None
        anchor = data.find('<w:endnoteReference w:id="1"')
        if anchor != -1:
            s = data.rfind("<w:p ", 0, anchor)
            e = data.find("</w:p>", anchor) + len("</w:p>")
            snippet = data[s:e] if s != -1 else None
        else:
            m = re.search(r"<w:p\b[^>]*>.*?endnoteReference.*?</w:p>", data, re.S)
            snippet = m.group(0) if m else None
        print()
        print("=== 首個分歧段落 XML 片段 ===")
        print(snippet if snippet else "(無法擷取)")

    # candidate (c) list symbol count in SVG
    svg_dir = render_dir
    svg_markers = 0
    for svg in svg_dir.glob("*.svg"):
        s = svg.read_text(encoding="utf-8")
        for mm in re.findall(r"<text[^>]*>(.*?)</text>", s, re.S):
            if mm.strip() in ("I.", "II.", "a.", "b.", "c.", "o", "•", ""):
                svg_markers += 1
    print()
    print(f"候選 (c) SVG 清單符號數 = {svg_markers}")

if __name__ == "__main__":
    main()
