"""逐 chart part 抓軸標量宣告（c:majorUnit／c:minorUnit／c:min／c:max／c:logBase）＋系列快取極值。

目的：確認 LO 刻度是否由宣告值決定（若是，律＝宣告優先，無需容量尺擬合）。
"""

import pathlib
import re
import sys
import zipfile

TAGS = ['majorUnit', 'minorUnit', 'min', 'max', 'logBase']

for f in sys.argv[1:]:
    z = zipfile.ZipFile(pathlib.Path(f))
    parts = [n for n in z.namelist() if re.match(r'word/(charts|charts/charts)/chart\d*\.xml$', n)]
    print('###', f)
    for p in sorted(parts):
        x = z.read(p).decode('utf-8', 'replace')
        decls = []
        for t in TAGS:
            for m in re.finditer(r'<c:%s(?:\s[^>]*)?>' % t, x):
                s = m.group(0)
                val = re.search(r'val="([^"]*)"', s)
                decls.append('%s=%s' % (t, val.group(1) if val else '?'))
        vals = [float(v) for v in re.findall(r'<c:v>([^<]*)</c:v>', x) if re.match(r'^-?[\d.]+([eE][-+]?\d+)?$', v)]
        print('  ', p, 'decls:', decls or '無', '| c:v n=%d min=%s max=%s' % (
            len(vals), min(vals) if vals else None, max(vals) if vals else None))
