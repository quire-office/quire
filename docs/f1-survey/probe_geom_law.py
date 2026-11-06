"""F1 發射幾何定錨：從 LO 真值反推「圖表框 → 繪圖區 → 軸刻度」的相對幾何（多檔，禁單檔調參）。

判讀口徑同 fit_insets.py：
- 圖表外框＝白填色＋灰筆（0.851 級）的 rect。
- 繪圖區＝框內水平灰線（majorGridlines）的公共 x 跨度；0 線＝最低那條。
- 數值刻度＝框左側帶的文字（x2 <= 框左緣＋2）；類別標籤＝框下緣帶的文字。
"""
import json
import pathlib
import sys

GRAY = (0.8509804010391235,) * 3


def close(a, b, tol=0.02):
    if not a or not b or len(a) != len(b):
        return False
    return all(abs(x - y) <= tol for x, y in zip(a, b))


truth = json.loads(pathlib.Path('docs/f1-survey/lo_chart_truth.json').read_text(encoding='utf-8'))
names = sys.argv[1:] or [k for k, v in truth.items() if v != 'LO_FAIL']

for name in names:
    v = truth[name]
    draws = v.get('drawings', [])
    frames = [d for d in draws if d['type'] == 'fs' and d['rect']]
    best = None
    for fr in frames:
        grid = [
            d for d in draws
            if d is not fr and d['rect'] and close(d['color'], GRAY)
            and abs(d['rect'][1] - d['rect'][3]) < 0.5
            and d['rect'][0] >= fr['rect'][0] - 1 and d['rect'][2] <= fr['rect'][2] + 1
            and d['rect'][1] >= fr['rect'][1] and d['rect'][3] <= fr['rect'][3]
        ]
        if len(grid) >= 2 and (best is None or len(grid) > len(best[1])):
            best = (fr, grid)
    if best is None:
        print(f'{name:46} 無可判讀框')
        continue
    fr, grid = best
    fx1, fy1, fx2, fy2 = fr['rect']
    gx1 = min(d['rect'][0] for d in grid)
    gx2 = max(d['rect'][2] for d in grid)
    zero = max(d['rect'][1] for d in grid)          # 0 線（最低格線）
    top = min(d['rect'][1] for d in grid)           # 最高格線
    n_grid = len(grid)
    L = (gx1 - fx1) / (fx2 - fx1)
    R = (fx2 - gx2) / (fx2 - fx1)
    B = (fy2 - zero) / (fy2 - fy1)
    T = (fy2 - top) / (fy2 - fy1)
    step = (zero - top) / (n_grid - 1)
    # 刻度文字（左帶）與類別標籤（下帶）
    left = [l for l in v['lines'] if l['bbox'][2] <= fx1 + 3 and fx1 - 40 < l['bbox'][0] < fx1]
    bot = [l for l in v['lines'] if l['bbox'][1] >= fy2 - 3 and l['bbox'][3] <= fy2 + 22]
    print(f'{name:46} frame={(fx2-fx1):.2f}x{(fy2-fy1):.2f} grid={n_grid}')
    print(f'   繪圖區歸一 L={L:.3f} R={R:.3f} 零線={B:.3f} 頂線={T:.3f} 格線步距={step:.2f}pt')
    if left:
        ys = sorted((l['bbox'][1] + l['bbox'][3]) / 2 for l in left)
        print(f'   數值刻度 n={len(left)} 文字={[l["t"] for l in left]} 中心y={[f"{y:.1f}" for y in ys]}')
        print(f'   刻度右緣 x={max(l["bbox"][2] for l in left):.2f}（框左 {fx1:.2f}）'
              f' 首末中心距={ys[-1]-ys[0]:.2f} 字級={ {l["size"] for l in left} }')
    if bot:
        print(f'   類別標籤 n={len(bot)} 文字={[l["t"] for l in bot]}'
              f' 中心x={[f"{(l["bbox"][0]+l["bbox"][2])/2:.1f}" for l in bot]}'
              f' 頂y={min(l["bbox"][1] for l in bot):.2f}（零線 {zero:.2f}）字級={ {l["size"] for l in bot} }')
