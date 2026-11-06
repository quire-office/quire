"""找 LO 真缺字（圖表標題／軸刻度）在封包內的出處：全 part 字串掃描。"""
import re
import sys
import zipfile

DOCX = sys.argv[1]
NEEDLE = sys.argv[2:] or ['圖', '表', '標', '題']
z = zipfile.ZipFile(DOCX)
for n in z.namelist():
    try:
        raw = z.read(n).decode('utf-8')
    except UnicodeDecodeError:
        raw = z.read(n).decode('utf-8', 'replace')
    hits = [ch for ch in NEEDLE if ch in raw]
    if hits:
        print('PART', n, 'HITS', ''.join(hits))
        for ch in hits:
            for m in re.finditer(re.escape(ch), raw):
                seg = raw[max(0, m.start() - 90):m.start() + 90].replace('\n', ' ')
                print('   %s @%d  ...%s...' % (ch, m.start(), seg))
                break
