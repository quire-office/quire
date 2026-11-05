import json
import pathlib
import sys

t = json.loads(pathlib.Path('docs/f1-survey/lo_chart_truth.json').read_text(encoding='utf-8'))
for name in sys.argv[1:]:
    v = t[name]
    if v == 'LO_FAIL':
        print('FAIL', name)
        continue
    print('###', name, 'pages', max(l['p'] for l in v['lines']) + 1)
    for l in v['lines']:
        print('  p%d %-22s dir=%s bbox=%s sz=%s f=%s c=%s' % (l['p'], l['t'][:22], l['dir'], l['bbox'], l['size'], l['font'], l['color']))
    print('  -- drawings --')
    for d in v['drawings'][:24]:
        print('   ', d['type'], 'fill', d['fill'], 'color', d['color'], 'w', d['width'], 'rect', d['rect'], 'n_items', len(d['items']))
