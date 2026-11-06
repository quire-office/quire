"""抽 lo_chart_truth.json 內 chart-size／strict 的軸刻度文字（左側帶與下方帶），
供 F1 發射幾何與刻度律定錨（非單檔猜數）。"""
import json, pathlib, sys

truth = json.loads(pathlib.Path('docs/f1-survey/lo_chart_truth.json').read_text(encoding='utf-8'))
for name in sys.argv[1:] or ['chart-size.docx', 'strict.docx']:
    v = truth.get(name)
    if not v or v == 'LO_FAIL':
        print(name, 'NO TRUTH'); continue
    draws = v.get('drawings', [])
    frames = [d for d in draws if d['type'] == 'fs' and d['rect']]
    fr = None
    for d in frames:
        if d['color'] and all(abs(c - 0.8509804010391235) < 0.02 for c in d['color']):
            fr = d['rect']; break
    print('===', name, 'frame', fr)
    if fr is None:
        # 退而求其次：取含最多水平灰線的 rect
        for d in frames:
            hs = [i for i in d.get('items', []) if i[0] == 'l' and abs(i[1][1] - i[1][3]) < 0.5]
            if len(hs) >= 2 and (fr is None or (fr[2]-fr[0]) * (fr[3]-fr[1]) < (d['rect'][2]-d['rect'][0]) * (d['rect'][3]-d['rect'][1])):
                fr = d['rect']
        print('  fallback frame', fr)
    fx1, fy1, fx2, fy2 = fr
    for L in v['lines']:
        x1, y1, x2, y2 = L['bbox']
        if L['size'] > 11.5:
            continue
        side = 'LEFT' if x2 < fx1 + 2 else ('BOTTOM' if y1 > fy2 - 2 else ('IN' if fx1 <= x1 and x2 <= fx2 and fy1 <= y1 else 'OUT'))
        print(f"  {side:6} t={L['t']!r:14} bbox=({x1:.2f},{y1:.2f},{x2:.2f},{y2:.2f}) size={L['size']}")
