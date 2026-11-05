"""普查圖表的「掛載語境」：inline/anchor、是否落在 wps:txbxContent 內、wp:extent、外部 r:id。"""
import re
import sys
import zipfile
from collections import OrderedDict

FILES = sys.argv[1:]

def ctx(raw, m):
    """回傳該 w:drawing 之前是否仍在某個 w:txbxContent 內（粗暴但可驗證的巢狀判斷）。"""
    head = raw[:m.start()]
    opens = len(re.findall(r'<w:txbxContent\b', head))
    closes = len(re.findall(r'</w:txbxContent>', head))
    return opens - closes

for path in FILES:
    z = zipfile.ZipFile(path)
    parts = [n for n in z.namelist() if re.match(r'word/(document|header\d*|footer\d*)\.xml$', n)]
    rels = {}
    for n in z.namelist():
        if n.endswith('.rels') and '/_rels/' in n:
            base = n.split('/_rels/')[-1]
            rels[base] = z.read(n).decode('utf-8', 'replace')
    print('###', path)
    for n in parts:
        raw = z.read(n).decode('utf-8', 'replace')
        if 'drawingml/2006/chart' not in raw:
            continue
        for m in re.finditer(r'<w:drawing\b.*?</w:drawing>', raw, re.S):
            s = m.group(0)
            if 'drawingml/2006/chart' not in s:
                continue
            kind = 'anchor' if '<wp:anchor' in s else ('inline' if '<wp:inline' in s else '?')
            ext = re.search(r'<wp:extent cx="(\d+)" cy="(\d+)"/>', s)
            rid = re.search(r'<c:chart[^>]*r:id="(rId\d+)"', s)
            docpr = re.search(r'<wp:docPr id="(\d+)" name="([^"]*)"', s)
            pos = re.findall(r'<wp:position([HV]) relativeFrom="([^"]*)">\s*<wp:posOffset>\s*(-?\d+)', s)
            pct = re.findall(r'<wp14:pctPos([HV])Offset>(\d+)', s)
            print('  %s depth_txbx=%d kind=%s extent=%s rid=%s docPr=%s pos=%s pct=%s part=%s'
                  % (n, ctx(raw, m), kind,
                     (ext.groups() if ext else None), (rid.group(1) if rid else None),
                     (docpr.groups() if docpr else None), pos, pct, n))
    # 該檔 chart part 的掛載 r:id → target
    for n in z.namelist():
        if n.endswith('.rels') and '/_rels/' in n and 'chart' in rels.get(n.split('/_rels/')[-1], ''):
            pass
    for base, r in rels.items():
        for m in re.finditer(r'<Relationship Id="(rId\d+)"[^>]*Type="[^"]*/chart"[^>]*Target="([^"]*)"', r):
            print('   REL', base, m.group(1), '->', m.group(2))
