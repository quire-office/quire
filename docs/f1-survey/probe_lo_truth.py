"""LO 真值抽取（chart-size）：pymupdf 逐行 dir+bbox+size 口徑，同 e2_vertical_truth 做法。"""
import json
import sys
import pathlib

import pymupdf

pdf = pathlib.Path(sys.argv[1])
doc = pymupdf.open(pdf)
out = []
for pno, page in enumerate(doc):
    d = page.get_text('dict')
    for blk in d['blocks']:
        if blk.get('type') != 0:
            continue
        for ln in blk['lines']:
            txt = ''.join(sp['text'] for sp in ln['spans']).strip()
            if not txt:
                continue
            sp0 = ln['spans'][0]
            out.append({
                'page': pno,
                'text': txt,
                'dir': [round(ln['dir'][0], 3), round(ln['dir'][1], 3)],
                'bbox': [round(v, 2) for v in ln['bbox']],
                'size': round(sp0['size'], 2),
                'font': sp0.get('font'),
                'color': sp0.get('color'),
            })
print(json.dumps({'pdf': str(pdf), 'pages': doc.page_count, 'lines': out}, ensure_ascii=False, indent=1))
# 另外抓向量圖元（rect/line）以取得圖表外框與軸線幾何
geo = []
for pno, page in enumerate(doc):
    for dr in page.get_drawings():
        items = []
        for it in dr['items']:
            if it[0] == 're':
                items.append(['re', [round(v, 2) for v in it[1]]])
            elif it[0] == 'l':
                items.append(['l', [round(it[1].x, 2), round(it[1].y, 2), round(it[2].x, 2), round(it[2].y, 2)]])
        geo.append({'page': pno, 'type': dr['type'], 'fill': dr.get('fill'), 'color': dr.get('color'),
                    'width': dr.get('width'), 'rect': [round(v, 2) for v in dr['rect']], 'items': items[:6]})
print('=== DRAWINGS %d ===' % len(geo))
print(json.dumps(geo, ensure_ascii=False, indent=1))
