"""語料普查：列出所有含 drawingml/2006/chart 的檔案、圖型統計、cache vs ref、系列數。"""
import re
import sys
import zipfile
from collections import Counter, OrderedDict

ROOTS = sys.argv[1:] or ['corpus', 'corpus-doc', 'corpus-odt', 'corpus-xlsx', 'fixtures']
CHART_URI = 'drawingml/2006/chart'
TYPES = ['barChart', 'lineChart', 'pieChart', 'areaChart', 'scatterChart', 'bubbleChart',
         'radarChart', 'doughnutChart', 'ofPieChart', 'stockChart', 'surfaceChart',
         'bar3DChart', 'line3DChart', 'pie3DChart', 'area3DChart', 'scatter3DChart',
         'surface3DChart', 'bubble3DChart', 'radarChart']

rows = []
import os
for root in ROOTS:
    if not os.path.isdir(root):
        continue
    for dirpath, _dirs, files in os.walk(root):
        for fn in sorted(files):
            p = os.path.join(dirpath, fn)
            if not fn.lower().endswith(('.docx', '.pptx', '.xlsx', '.odt', '.ods', '.odp')):
                continue
            try:
                z = zipfile.ZipFile(p)
            except Exception:
                continue
            names = z.namelist()
            chart_parts = [n for n in names if re.match(r'(word|ppt)/charts?/chart\d+\.xml$', n)
                           or re.match(r'ppt/charts/chart\d+\.xml$', n)]
            hit_ct = False
            for n in names:
                if n.endswith('.rels') or n == '[Content_Types].xml':
                    try:
                        if CHART_URI in z.read(n).decode('utf-8', 'replace'):
                            hit_ct = True
                            break
                    except Exception:
                        pass
            if not chart_parts and not hit_ct:
                continue
            kinds = Counter()
            sers = 0
            cache_val = cache_cat = ref_only = 0
            titles = 0
            cat_ax = val_ax = 0
            gridlines = 0
            ext = 0
            for n in chart_parts:
                raw = z.read(n).decode('utf-8', 'replace')
                for t in TYPES:
                    if '<c:%s' % t in raw:
                        kinds[t] += 1
                sers += raw.count('<c:ser>')
                cache_val += raw.count('<c:numCache>')
                cache_cat += raw.count('<c:strCache>')
                ref_only += raw.count('<c:numRef>') - raw.count('<c:numCache>')
                titles += raw.count('<c:title>')
                cat_ax += raw.count('<c:catAx>')
                val_ax += raw.count('<c:valAx>')
                gridlines += raw.count('<c:majorGridlines>')
                if '<c:externalData' in raw:
                    ext += 1
            rows.append(OrderedDict(
                path=p, charts=len(chart_parts), kinds=dict(kinds), ser=sers,
                numCache=cache_val, strCache=cache_cat, title=titles,
                catAx=cat_ax, valAx=val_ax, grid=gridlines, externalData=ext))

print('含圖表檔案數：%d' % len(rows))
for r in rows:
    print('%-58s charts=%d ser=%d numCache=%d strCache=%d title=%d catAx=%d valAx=%d grid=%d ext=%d kinds=%s'
          % (r['path'], r['charts'], r['ser'], r['numCache'], r['strCache'], r['title'],
             r['catAx'], r['valAx'], r['grid'], r['externalData'], r['kinds']))
