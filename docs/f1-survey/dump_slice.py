import pathlib
import sys
import zipfile

p = pathlib.Path(sys.argv[1])
part = sys.argv[2]
needles = sys.argv[3:] or ['<c:numCache>', '<c:title>', '<c:valAx>', '<c:catAx>', '<c:ser>', '<c:scaling>']
z = zipfile.ZipFile(p)
x = z.read(part).decode('utf-8', 'replace')
for n in needles:
    i = x.find(n)
    print('---', n, 'at', i)
    if i >= 0:
        print(repr(x[i:i + 700]))
