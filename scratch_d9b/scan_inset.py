"""掃描 corpus/ 內宣告 v:textbox/@inset 的檔案（D9b 紅燈考卷候選）。"""
import zipfile
import re
import glob
import sys

pattern = re.compile(r'<v:textbox([^>]*)>')
hits = {}
files = sorted(glob.glob('corpus/**/*.doc*', recursive=True))
print(f"scanned docx/doc candidates: {len(files)}", file=sys.stderr)
for p in files:
    try:
        z = zipfile.ZipFile(p)
    except Exception:
        continue
    names = [n for n in z.namelist()
             if (n.startswith('word/') or n.startswith('ppt/')) and n.endswith('.xml')]
    found = []
    for n in names:
        try:
            data = z.read(n).decode('utf-8', 'ignore')
        except Exception:
            continue
        for m in pattern.finditer(data):
            attrs = m.group(1)
            im = re.search(r'inset="([^"]+)"', attrs)
            if im:
                found.append((n, im.group(1)))
    if found:
        hits[p] = found

print(f"files with declared v:textbox/@inset: {len(hits)}")
for p, f in sorted(hits.items()):
    vals = sorted(set(v for _, v in f))
    parts = sorted(set(n for n, _ in f))
    print(f"{p}  count={len(f)} parts={parts} values={vals[:8]}")
