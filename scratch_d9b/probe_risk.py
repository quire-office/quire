"""實證三查（實作前風險鎖定）：
A. n751077 _x0000_s1027 群組的 filled/stroked 事實（滿版新判定會不會誤觸發描邊內縮臂）
B. tdf103573（PM 裁留 D9b 的「兩框同發 x=281.1」）：框宣告＋LO 真值＋我方目前發射
C. fdo76591 pgMar 換算：版心幾何＋s1026 覆蓋版心的相對宣告
"""
import zipfile
import re
import subprocess
import os
import xml.etree.ElementTree as ET
import fitz

def local(tag):
    return tag.split('}')[-1]

# A
z = zipfile.ZipFile('corpus/ooxmlimport/n751077.docx')
raw = z.read('word/document.xml').decode('utf-8')
i = raw.find('_x0000_s1027')
s = raw.rfind('<v:group', 0, i)
e = raw.find('>', s)
print("A) n751077 s1027 <v:group ...>:")
print(raw[s:e + 1])

# B
z2 = zipfile.ZipFile('corpus/ooxmlexport/tdf103573.docx')
raw2 = z2.read('word/document.xml').decode('utf-8')
for m in re.finditer(r'<v:shape[^>]*>|<v:group[^>]*>|<v:rect[^>]*>', raw2):
    frag = m.group(0)
    if 'position' in frag or 'margin' in frag or 'width' in frag:
        print("B) tdf103573 VML:", frag[:260])
# pgMar
for m in re.finditer(r'<w:pgMar[^/]*/>|<w:pgSz[^/]*/>', raw2):
    print("B) tdf103573:", m.group(0))
p = subprocess.run(['.worktrees/t_08b86cba/scratch_d9b/base-bin/quire-cli', 'page',
                    'corpus/ooxmlexport/tdf103573.docx', '0'],
                   capture_output=True, text=True, timeout=240)
import json
d = json.loads(p.stdout)
items = d['items'] if isinstance(d, dict) else d
for k_, it in enumerate(items):
    kk = list(it)[0]
    if kk in ('Rect', 'Rule', 'Image'):
        v = it[kk]
        print(f"B) OURS {kk} x={v.get('x')} y={v.get('y')} w={v.get('w')} h={v.get('h')}")
LO = os.path.expanduser("~/.local/lo-linux/opt/libreoffice25.8/program/soffice")
pdf = '/tmp/d9b_truth/tdf103573.pdf'
os.makedirs('/tmp/d9b_truth', exist_ok=True)
if not os.path.exists(pdf):
    subprocess.run([LO, '--headless', '--norestore',
                    '-env:UserInstallation=file:///tmp/d9b_prof_tdf103573',
                    '--convert-to', 'pdf', '--outdir', '/tmp/d9b_truth',
                    'corpus/ooxmlexport/tdf103573.docx'], capture_output=True, timeout=300)
doc = fitz.open(pdf)
pg = doc[0]
for dr in pg.get_drawings():
    r = dr['rect']
    if r.width > 200:
        print(f"B) LO drawing rect=[{r.x0:.2f},{r.y0:.2f},{r.x1:.2f},{r.y1:.2f}] "
              f"w={r.width:.2f} h={r.height:.2f} wpt={dr.get('width')}")
doc.close()
