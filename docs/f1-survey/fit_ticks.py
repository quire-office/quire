"""擬合分析：從 LO 真值（14 檔）反解軸刻度律——刻度值序列、步進、資料 min/max 關係、字級。
禁單檔調參：每條律必須在多數檔成立方採納。"""
import json
import pathlib
import re
import sys
import zipfile
from collections import Counter, defaultdict

truth = json.loads(pathlib.Path('docs/f1-survey/lo_chart_truth.json').read_text(encoding='utf-8'))


def chart_cache(path):
    z = zipfile.ZipFile(path)
    out = []
    for n in sorted(z.namelist()):
        if not re.search(r'charts?/chart\d*\.xml$', n):
            continue
        raw = z.read(n).decode('utf-8', 'replace')
        sers = []
        for m in re.finditer(r'<c:ser>.*?</c:ser>', raw, re.S):
            seg = m.group(0)
            vals = []
            vm = re.search(r'<c:val>.*?</c:val>', seg, re.S)
            if vm:
                vals = [v for v in re.findall(r'<c:v>([^<]*)</c:v>', vm.group(0))]
            cats = []
            cm = re.search(r'<c:cat>.*?</c:cat>', seg, re.S)
            if cm:
                cats = re.findall(r'<c:v>([^<]*)</c:v>', cm.group(0))
            sers.append({'val': vals, 'cat': cats})
        kinds = re.findall(r'<c:(\w*Chart)\b', raw)
        out.append({'part': n, 'kinds': kinds, 'sers': sers,
                    'barDir': (re.search(r'<c:barDir val="(\w+)"', raw) or [None, None])[1],
                    'grouping': (re.search(r'<c:grouping val="(\w+)"', raw) or [None, None])[1],
                    'title_text': re.findall(r'<a:t>([^<]*)</a:t>', (re.search(r'<c:title>.*?</c:title>', raw, re.S) or type('x', (), {'group': lambda s, *a: ''})()).group(0)),
                    'gapWidth': (re.search(r'<c:gapWidth val="(\d+)"', raw) or [None, None])[1]})
    return out


print('%-46s %-10s %s' % ('FILE', 'kind', 'LO 數值軸刻度序列（y 由上而下）/ 類別軸刻度 / 字級'))
law_rows = []
for name, v in truth.items():
    if v == 'LO_FAIL':
        print('FAIL', name)
        continue
    path = v['path']
    charts = chart_cache(path)
    lines = v['lines']
    # 數字刻度行（純數字、size<11）
    nums = [l for l in lines if re.fullmatch(r'-?\d+(\.\d+)?', l['t'])]
    print('=' * 100)
    print(name, [c['kinds'] for c in charts])
    for c in charts:
        for i, s in enumerate(c['sers']):
            print('   cache ser%d val=%s cat=%s' % (i, s['val'][:8], s['cat'][:8]))
    # 依 x 分組：數值軸刻度在同一 x、類別軸刻度在同一 y
    byx = defaultdict(list)
    for l in nums:
        byx[round(l['bbox'][0], 0)].append(l)
    for x, ls in sorted(byx.items(), key=lambda kv: min(q['bbox'][1] for q in kv[1])):
        seq = sorted(ls, key=lambda q: q['bbox'][1])
        print('   x≈%s 序列=%s sizes=%s' % (x, [q['t'] for q in seq], sorted(set(q['size'] for q in seq))))
    byy = defaultdict(list)
    for l in nums:
        byy[round(l['bbox'][1], 0)].append(l)
    multi = {y: sorted(ls, key=lambda q: q['bbox'][0]) for y, ls in byy.items() if len(ls) > 1}
    for y, ls in sorted(multi.items())[:6]:
        print('   y≈%s 序列=%s' % (y, [q['t'] for q in ls]))
