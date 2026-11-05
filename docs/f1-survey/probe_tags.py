import re
import sys
import zipfile

raw = zipfile.ZipFile(sys.argv[1]).read(sys.argv[2] if len(sys.argv) > 2 else 'word/document.xml').decode('utf-8')
# 逐標籤輸出（附偏移），便於比對巢狀結構與掛載次數
pos = 0
for tok in re.findall(r'<[^>]+>|[^<]+', raw):
    if tok.strip():
        t = tok.strip()
        if t.startswith('<'):
            print('%6d  %s' % (pos, t[:150]))
        else:
            print('%6d      TEXT %r' % (pos, t[:60]))
    pos += len(tok)
