"""查 chart part 的 valAx 是否宣告 c:min／c:max／c:majorUnit／c:minorUnit／c:logBase
（決定 LO 刻度上界是否為宣告值而非自動尺度）。"""
import re
import sys
import zipfile

for path in sys.argv[1:]:
    z = zipfile.ZipFile(path)
    for n in z.namelist():
        if not re.match(r'word/charts/chart\d+\.xml$', n):
            continue
        xml = z.read(n).decode('utf-8', 'replace')
        print('===', path, n, len(xml))
        for tag in ('min', 'max', 'majorUnit', 'minorUnit', 'logBase', 'autoTitleDeleted',
                    'grouping', 'barDir', 'gapWidth', 'overlap', 'tickLblPos', 'delete'):
            for m in re.finditer(r'<c:%s[^>]*>' % tag, xml):
                print('   ', n, m.group(0))
        # 系列數與 numCache 點數
        print('    ser=', len(re.findall(r'<c:ser>', xml)),
              'numCache=', len(re.findall(r'<c:numCache>', xml)),
              'strCache=', len(re.findall(r'<c:strCache>', xml)),
              'cat=', len(re.findall(r'<c:cat>', xml)))
        for m in re.finditer(r'<c:v>([^<]*)</c:v>', xml):
            print('    v=', m.group(1), end='')
        print()
