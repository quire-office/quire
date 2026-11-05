"""逐檔 chart part：c:title 是否帶 c:tx 真文字、c:cat 存在與否、cache/ref 形態、axId 配對。"""
import re
import sys
import zipfile

FILES = sys.argv[1:]
for path in FILES:
    z = zipfile.ZipFile(path)
    print('###', path)
    for n in sorted(z.namelist()):
        if not re.search(r'charts?/chart\d*\.xml$', n):
            continue
        raw = z.read(n).decode('utf-8', 'replace')
        # c:title 內文（c:tx 之下的 a:t）
        titles = []
        for m in re.finditer(r'<c:title>.*?</c:title>', raw, re.S):
            seg = m.group(0)
            ts = re.findall(r'<a:t>([^<]*)</a:t>', seg)
            titles.append('|'.join(ts) if ts else '(EMPTY c:title 無 c:tx/a:t)')
        sers = []
        for m in re.finditer(r'<c:ser>.*?</c:ser>', raw, re.S):
            seg = m.group(0)
            tx = re.findall(r'<c:tx>.*?</c:tx>', seg, re.S)
            txv = []
            if tx:
                txv = re.findall(r'<a:t>([^<]*)</a:t>', tx[0]) or re.findall(r'<c:v>([^<]*)</c:v>', tx[0])
            cats = re.search(r'<c:cat>.*?</c:cat>', seg, re.S)
            cat_kind = None
            cat_vals = []
            if cats:
                cat_kind = 'strCache' if '<c:strCache>' in cats.group(0) else ('numCache' if '<c:numCache>' in cats.group(0) else 'ref-only')
                cat_vals = re.findall(r'<c:v>([^<]*)</c:v>', cats.group(0))
            vals = re.search(r'<c:val>.*?</c:val>', seg, re.S)
            val_kind = None
            val_vals = []
            if vals:
                val_kind = 'numCache' if '<c:numCache>' in vals.group(0) else 'ref-only'
                val_vals = re.findall(r'<c:v>([^<]*)</c:v>', vals.group(0))
            sers.append({'tx': '|'.join(txv), 'cat': cat_kind, 'catv': cat_vals[:6], 'val': val_kind, 'valv': val_vals[:6]})
        kinds = re.findall(r'<c:(\w*Chart)\b', raw)
        print(' ', n, 'kinds=', kinds)
        print('    title=', titles)
        for i, s in enumerate(sers):
            print('    ser[%d] %s' % (i, s))
        print('    catAx=%d valAx=%d dateAx=%d delete=%d grid=%d'
              % (raw.count('<c:catAx>'), raw.count('<c:valAx>'), raw.count('<c:dateAx>'),
                 raw.count('<c:delete val="1"/>'), raw.count('<c:majorGridlines>')))
