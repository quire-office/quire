"""掃 chart-size.docx 全封包的 CJK 字元出處（真缺字 圖表標題 的來源追查）。"""
import re
import sys
import zipfile

z = zipfile.ZipFile(sys.argv[1])
CJK = re.compile(r'[\u3000-\u9fff\uf900-\ufaff]')
for n in z.namelist():
    raw = z.read(n)
    try:
        txt = raw.decode('utf-8')
    except UnicodeDecodeError:
        txt = raw.decode('utf-8', 'replace')
    hits = sorted(set(CJK.findall(txt)))
    if hits:
        print('PART', n, '->', ''.join(hits))
        for m in CJK.finditer(txt):
            seg = txt[max(0, m.start() - 120):m.start() + 60].replace('\n', ' ')
            print('   @%d ...%s...' % (m.start(), seg))
            break
