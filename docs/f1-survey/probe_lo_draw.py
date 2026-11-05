import pymupdf
d = pymupdf.open('/mnt/c/tmp/f1a/chart-size.pdf')
for i, dr in enumerate(d[0].get_drawings()):
    items = []
    for it in dr['items']:
        if it[0] == 're':
            items.append(('re', [round(v, 2) for v in it[1]]))
        elif it[0] == 'l':
            items.append(('l', [round(it[1].x, 2), round(it[1].y, 2), round(it[2].x, 2), round(it[2].y, 2)]))
        else:
            items.append((it[0],))
    print(i, dr['type'], 'fill', dr.get('fill'), 'color', dr.get('color'), 'w', dr.get('width'),
          'rect', [round(v, 2) for v in dr['rect']])
    for x in items[:8]:
        print('   ', x)
