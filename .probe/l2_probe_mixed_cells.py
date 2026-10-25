import zipfile, re, sys

def cells_with_styles(path):
    z = zipfile.ZipFile(path)
    xml = z.read("word/document.xml").decode("utf-8", "ignore")
    out = []
    for m in re.finditer(r"<w:tc\b.*?</w:tc>", xml, re.S):
        tc = m.group(0)
        runs = []
        for rm in re.finditer(r"<w:r\b.*?</w:r>", tc, re.S):
            r = rm.group(0)
            txt = "".join(re.findall(r"<w:t[^>]*>(.*?)</w:t>", r, re.S))
            if not txt:
                continue
            b = bool(re.search(r"<w:b/>|<w:b w:val=\"(1|true|on)\"", r))
            i = bool(re.search(r"<w:i/>|<w:i w:val=\"(1|true|on)\"", r))
            sz = re.search(r'<w:sz w:val="(\d+)"', r)
            col = re.search(r'<w:color w:val="([0-9A-Fa-f]{6})"', r)
            runs.append((txt, b, i, sz.group(1) if sz else None, col.group(1) if col else None))
        if len(runs) >= 2:
            styles = {(b, i, s, c) for (_, b, i, s, c) in runs}
            if len(styles) >= 2:
                out.append(runs)
    return out

for p in sys.argv[1:]:
    cs = cells_with_styles(p)
    print("===", p, "mixed cells:", len(cs))
    for c in cs[:8]:
        print("   ", [(t[:24], b, i, s, col) for (t, b, i, s, col) in c])
