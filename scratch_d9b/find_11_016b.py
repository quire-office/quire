"""fdo76591：找 '11-016'/'3-23-2011' 的真實出處（可能被 run 拆分／由欄位生成）。
   同時抓我方發射層拿到的段落文字（經 Engine 級聯後）中匹配者。
"""
import zipfile
import re
import subprocess
import json

z = zipfile.ZipFile('corpus/ooxmlexport/fdo76591.docx')
raw = z.read('word/document.xml').decode('utf-8')

# 1) 把所有 w:t 串接後找（跨 run）
ts = re.findall(r'<w:t(?: [^>]*)?>([^<]*)</w:t>', raw)
joined = ''.join(ts)
for needle in ('11-016', '3-23-2011'):
    i = joined.find(needle)
    print(f"{needle!r} 在 w:t 串接文中的 offset: {i}")
    if i >= 0:
        print("   context:", joined[max(0, i - 60):i + 80].replace('\n', ' '))

# 2) 分段串接：找出哪些 w:p 含有（跨 run）
paras = re.findall(r'<w:p\b.*?</w:p>', raw, re.S)
for pi, para in enumerate(paras):
    ptext = ''.join(re.findall(r'<w:t(?: [^>]*)?>([^<]*)</w:t>', para))
    for needle in ('11-016', '3-23-2011'):
        if needle in ptext:
            # 宿主：往前數 v:shape
            offset = raw.find(para[:80])
            hs = [m for m in re.finditer(r'<v:shape[^>]*id="([^"]+)"', raw[:offset])]
            txc_open = len(re.findall(r'<w:txbxContent', raw[:offset]))
            txc_close = len(re.findall(r'</w:txbxContent>', raw[:offset]))
            print(f"\n### 段 {pi} 含 {needle!r}: text={ptext[:90]!r}")
            print(f"    宿主 shape={hs[-1].group(1) if hs else None} in_txbx={txc_open > txc_close}")
            ppr = re.search(r'<w:pPr>.*?</w:pPr>', para, re.S)
            print(f"    pPr={ppr.group(0)[:260] if ppr else None}")

# 3) 我方 engine dump：找頁 0 Text 項含 '11-016'/'3-23-2011' 者
p = subprocess.run(['.worktrees/t_08b86cba/scratch_d9b/base-bin/quire-cli', 'page',
                    'corpus/ooxmlexport/fdo76591.docx', '0'],
                   capture_output=True, text=True, timeout=180)
d = json.loads(p.stdout)
items = d['items'] if isinstance(d, dict) else d
for i, it in enumerate(items):
    k = list(it)[0]
    if k != 'Text':
        continue
    t = it[k].get('text', '')
    if '11-016' in t or '3-23-2011' in t or '11-' in t and '016' in t:
        print(f"我方 idx={i} Text {t!r} y={it[k].get('y')} x={it[k].get('x')} size={it[k].get('size_pt')}")
