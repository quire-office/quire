"""擬合數值軸刻度律：從 14 檔 LO 真值反解（資料上界 → LO 軸上界/步進），禁單檔調參。"""
import json
import math
import pathlib
import re
import zipfile
from collections import defaultdict

truth = json.loads(pathlib.Path('docs/f1-survey/lo_chart_truth.json').read_text(encoding='utf-8'))


def charts(path):
    z = zipfile.ZipFile(path)
    out = []
    for n in sorted(z.namelist()):
        if not re.search(r'charts?/chart\d*\.xml$', n):
            continue
        raw = z.read(n).decode('utf-8', 'replace')
        kinds = re.findall(r'<c:(\w*Chart)\b', raw)
        grouping = (re.search(r'<c:grouping val="(\w+)"', raw) or [None, None])[1]
        sers = []
        for m in re.finditer(r'<c:ser>.*?</c:ser>', raw, re.S):
            seg = m.group(0)
            vm = re.search(r'<c:val>.*?</c:val>', seg, re.S)
            vals = [float(v) for v in re.findall(r'<c:v>([^<]*)</c:v>', vm.group(0))] if vm else []
            sers.append(vals)
        out.append({'part': n, 'kinds': kinds, 'grouping': grouping, 'sers': sers,
                    'numFmt': (re.search(r'<c:numFmt formatCode="([^"]*)"', raw) or [None, None])[1],
                    'maxMin': re.findall(r'<c:(max|min) val="([^"]*)"', raw)})
    return out


def lo_value_axis(v):
    """取「同一 x、多值、單調 y」那組＝數值軸刻度（口徑同 fit_ticks）。"""
    nums = [l for l in v['lines'] if re.fullmatch(r'-?[\d,]+(\.\d+)?', l['t']) and l['size'] < 11.5]
    byx = defaultdict(list)
    for l in nums:
        byx[round(l['bbox'][0])].append(l)
    best = None
    for x, ls in byx.items():
        if len(ls) < 2:
            continue
        seq = sorted(ls, key=lambda q: q['bbox'][1])
        vals = [float(q['t'].replace(',', '')) for q in seq]
        if best is None or len(vals) > len(best[0]):
            best = (vals, seq)
    return best


rows = []
for name, v in truth.items():
    if v == 'LO_FAIL':
        continue
    cs = charts(v['path'])
    ba = lo_value_axis(v)
    if not ba:
        continue
    vals, seq = ba
    step = None
    if len(vals) >= 2:
        diffs = {round(vals[i + 1] - vals[i], 6) for i in range(len(vals) - 1)}
        if len(diffs) == 1:
            step = diffs.pop()
    dmin = min((x for s in cs for sv in s['sers'] for x in sv), default=None)
    dmax = max((x for s in cs for sv in s['sers'] for x in sv), default=None)
    stacked = any(c['grouping'] in ('stacked', 'standard', 'percentStacked') for c in cs)
    sums = None
    if stacked:
        n = max((len(sv) for c in cs for sv in c['sers']), default=0)
        sums = [sum(c['sers'][i][k] for c in cs for _ in [0] if False) for k in range(0)]
        # 逐類別加總（同系列序）
        per_cat = defaultdict(float)
        for c in cs:
            for sv in c['sers']:
                pass
        # 簡化：取第一份 chart 的各 ser 同 idx 相加
        c0 = cs[0]
        lens = [len(s) for s in c0['sers']]
        if lens and len(set(lens)) == 1:
            sums = [sum(s[k] for s in c0['sers']) for k in range(lens[0])]
    rows.append({'name': name, 'kinds': [c['kinds'] for c in cs], 'grouping': [c['grouping'] for c in cs],
                 'dmin': dmin, 'dmax': dmax, 'stacked': stacked, 'sum_max': (max(sums) if sums else None),
                 'axis': vals, 'step': step, 'size': sorted({q['size'] for q in seq}),
                 'numFmt': [c['numFmt'] for c in cs]})

for r in rows:
    print('%-52s %s grp=%s' % (r['name'][:52], r['kinds'], r['grouping']))
    print('   data min=%s max=%s stacked=%s sum_max=%s' % (r['dmin'], r['dmax'], r['stacked'], r['sum_max']))
    print('   LO axis=%s step=%s size=%s fmt=%s' % (r['axis'], r['step'], r['size'], r['numFmt']))
