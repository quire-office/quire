"""比對 chart-size 與 strict 兩檔 valAx 宣告（scaling/majorUnit/min/max），查 LO 刻度上界差異的檔內根因。"""
import zipfile
import re
import sys

for path in sys.argv[1:]:
    with zipfile.ZipFile(path) as z:
        names = [n for n in z.namelist() if re.match(r'word/charts/chart\d+\.xml$', n)]
        for n in names:
            xml = z.read(n).decode('utf-8')
            print('====', path, n, len(xml))
            for m in re.finditer(r'<c:valAx>.*?</c:valAx>', xml, re.S):
                seg = m.group(0)
                seg = re.sub(r'\s+', ' ', seg)
                print('  valAx:', seg[:1200])
            for m in re.finditer(r'<c:catAx>.*?</c:catAx>', xml, re.S):
                seg = re.sub(r'\s+', ' ', m.group(0))
                print('  catAx:', seg[:600])
