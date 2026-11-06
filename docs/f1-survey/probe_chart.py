"""F1 解剖探針：實拆 chart-size.docx 的圖表掛載點與 chart part 結構（report-only，零引擎觸碰）。"""
import re
import sys
import zipfile
from collections import Counter

DOCX = sys.argv[1] if len(sys.argv) > 1 else 'corpus/ooxmlexport/chart-size.docx'


def pretty(raw):
    """只在標籤邊界插入換行與縮排，不改字一筆屬性，保持原始順序。"""
    out = []
    depth = 0
    for tok in re.findall(r'<[^>]+>|[^<]+', raw):
        if tok.startswith('</'):
            depth = max(0, depth - 1)
            continue
        if tok.startswith('<'):
            out.append('  ' * depth + tok)
            if not tok.endswith('/>'):
                depth += 1
        else:
            t = tok.strip()
            if t:
                out.append('  ' * depth + 'TEXT: ' + t)
    return '\n'.join(out)


z = zipfile.ZipFile(DOCX)
print('=== PART LIST ===')
for n in z.namelist():
    print(' ', n, z.getinfo(n).file_size)

doc = z.read('word/document.xml').decode('utf-8')
print('\n=== document.xml：w:drawing 掛載點 ===')
for m in re.finditer(r'<w:drawing\b.*?</w:drawing>', doc, re.S):
    print('OFFSET %d LEN %d' % (m.start(), len(m.group(0))))
    print(pretty(m.group(0)))
    print('---')

print('\n=== word/_rels/document.xml.rels ===')
print(pretty(z.read('word/_rels/document.xml.rels').decode('utf-8')))

print('\n=== [Content_Types].xml（chart 相關條目）===')
for m in re.finditer(r'<(?:Override|Default)[^>]*[Cc]hart[^>]*/>', z.read('[Content_Types].xml').decode('utf-8')):
    print(m.group(0))

for name in z.namelist():
    if re.search(r'charts/_rels/.*\.rels$', name):
        print('\n=== %s ===' % name)
        print(pretty(z.read(name).decode('utf-8')))

for name in z.namelist():
    if re.search(r'charts/chart\d+\.xml$', name):
        raw = z.read(name).decode('utf-8')
        print('\n=== %s（%d bytes）結構 ===' % (name, len(raw)))
        print(pretty(raw))
        print('--- 標籤計數 ---')
        for k, v in sorted(Counter(re.findall(r'<([A-Za-z0-9:]+)[ >/]', raw)).items(), key=lambda kv: -kv[1]):
            print('  %-30s %d' % (k, v))
