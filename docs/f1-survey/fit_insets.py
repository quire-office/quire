"""從 LO 真值反推「圖表外框 → 繪圖區」相對內縮律（多檔擬合，禁單檔調參）。

口徑：
- 圖表外框＝LO 繪圖中「白填色＋灰筆（0.85 級）」且內部含 ≥2 條水平灰格線的 rect。
- 繪圖區＝框內水平灰格線的公共 x 跨度＋最低格線（值軸 0 線）；柱形只作參考。
- 內縮以 (框寬, 框高) 歸一化逐檔列出，另附「字級」原始宣告值對照，供人工判多數律。
"""

import json
import pathlib
import sys

GRAY = (0.8509804010391235, 0.8509804010391235, 0.8509804010391235)


def close(a, b, tol=0.02):
    if a is None or b is None or len(a) != len(b):
        return False
    return all(abs(x - y) <= tol for x, y in zip(a, b))


truth = json.loads(pathlib.Path('docs/f1-survey/lo_chart_truth.json').read_text(encoding='utf-8'))
names = sys.argv[1:]
if not names:
    names = [k for k, v in truth.items() if v != 'LO_FAIL']

for name in names:
    v = truth[name]
    draws = v['drawings']
    frames = [
        d for d in draws
        if d['type'] == 'fs' and close(d['fill'], (1.0, 1.0, 1.0)) and close(d['color'], GRAY) and d['rect']
    ]
    best = None
    for fr in frames:
        fx1, fy1, fx2, fy2 = fr['rect']
        grid = [
            d for d in draws
            if d is not fr and d['type'] == 's' and close(d['color'], GRAY) and d['rect']
            and abs(d['rect'][1] - d['rect'][3]) < 0.5
            and d['rect'][0] >= fx1 - 1 and d['rect'][2] <= fx2 + 1
            and fy1 - 1 <= d['rect'][1] <= fy2 + 1
        ]
        bars = [
            d for d in draws
            if d is not fr and d['type'] == 'f' and d['fill'] is not None and d['rect']
            and not close(d['fill'], (1, 1, 1)) and (d['rect'][2] - d['rect'][0]) > 2
            and d['rect'][0] >= fx1 - 1 and d['rect'][2] <= fx2 + 1
            and d['rect'][1] >= fy1 - 1 and d['rect'][3] <= fy2 + 1
        ]
        if len(grid) >= 2:
            score = len(grid) * 100 + len(bars)
            if best is None or score > best[0]:
                best = (score, fr, grid, bars)
    if best is None:
        print('%-46s 無可判讀框（水平灰格線 < 2）' % name[:46])
        continue
    _, fr, grid, bars = best
    fx1, fy1, fx2, fy2 = fr['rect']
    fw, fh = fx2 - fx1, fy2 - fy1
    gx1 = min(d['rect'][0] for d in grid)
    gx2 = max(d['rect'][2] for d in grid)
    gy_top = min(d['rect'][1] for d in grid)
    gy_bot = max(d['rect'][1] for d in grid)
    print('%-46s frame=%.2f x %.2f  grid=%d bars=%d' % (name[:46], fw, fh, len(grid), len(bars)))
    print('   內縮歸一 L=%.3f R=%.3f B(0線)=%.3f 格線頂=%.3f plotW=%.3f' % (
        (gx1 - fx1) / fw, (fx2 - gx2) / fw, (fy2 - gy_bot) / fh, (gy_top - fy1) / fh, (gx2 - gx1) / fw))
    # 數值軸刻度文字（x 小於格線左緣、落在框附近）
    ticks = [l for l in v['lines']
             if l['bbox'][2] <= gx1 + 6 and abs(l['bbox'][0] - fx1) < 40
             and fy1 - 10 <= l['bbox'][1] <= fy2 + 12
             and l['t'].strip().replace('.', '').replace('-', '').replace(',', '').replace('e', '').replace('+', '').isdigit()]
    if ticks:
        bb = [l['bbox'] for l in ticks]
        ys = sorted((l['bbox'][1] + l['bbox'][3]) / 2 for l in ticks)
        print('   刻度 n=%d x=%.1f..%.1f (歸一 %.3f..%.3f 對框左) 文字高=%.2f 步距=%.2f 字級=%s' % (
            len(ticks), min(b[0] for b in bb), max(b[2] for b in bb),
            (min(b[0] for b in bb) - fx1) / fw, (max(b[2] for b in bb) - fx1) / fw,
            (ticks[0]['bbox'][3] - ticks[0]['bbox'][1]),
            (ys[-1] - ys[0]) / max(1, len(ys) - 1),
            sorted({round(l['size'], 2) for l in ticks})))
    cats = [l for l in v['lines']
            if gy_bot - 2 <= l['bbox'][1] <= fy2 + 20 and l['bbox'][0] >= gx1 - 2 and l['bbox'][2] <= fx2 + 8]
    if cats:
        bb = [l['bbox'] for l in cats]
        print('   類別標籤 n=%d y_top=%.1f（對0線 +%.2fpt＝歸一 %.3f）字級=%s' % (
            len(cats), min(b[1] for b in bb), min(b[1] for b in bb) - gy_bot,
            (min(b[1] for b in bb) - gy_bot) / fh,
            sorted({round(l['size'], 2) for l in cats})))
    if bars:
        print('   柱底 y=%.1f（0線差 %.2f）柱寬=%s' % (
            max(d['rect'][3] for d in bars),
            max(d['rect'][3] for d in bars) - gy_bot,
            sorted({round(d['rect'][2] - d['rect'][0], 2) for d in bars})))
