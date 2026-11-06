"""F1：框外 wp:inline 圖表的 LO 水平定位律普查（禁單檔調參）。

口徑：LO drawings 中「白填色＋灰框」的 rect 起點 x，對照該檔版心左緣（多數
OOXML 語料 72pt）與置中位置 (page_w - 2*margin - w)/2 + margin。
"""

import json
import pathlib

GRAY = (0.8509804010391235,) * 3
truth = json.loads(pathlib.Path('docs/f1-survey/lo_chart_truth.json').read_text(encoding='utf-8'))

for name, v in truth.items():
    if v == 'LO_FAIL':
        continue
    frames = [
        d for d in v['drawings']
        if d['type'] == 'fs' and d['fill'] is not None and all(abs(c - 1.0) <= 0.02 for c in d['fill'])
        and d['color'] is not None and all(abs(c - g) <= 0.02 for c, g in zip(d['color'], GRAY)) and d['rect']
    ]
    for fr in frames:
        x1, y1, x2, y2 = fr['rect']
        w = x2 - x1
        # 假定 Letter 612pt、左右 margin 72pt（pagebreaks 口徑，普查用近似即可）。
        center_x = (612.0 - w) / 2.0
        print('%-46s frame x=%.2f w=%.2f | 左緣72 差=%.2f | 置中 差=%.2f' % (
            name[:46], x1, w, x1 - 72.0, x1 - center_x))
