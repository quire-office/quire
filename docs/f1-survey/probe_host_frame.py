"""F1 探針：chart-size 宿主文字框的框內段落流幾何（LO 真值 vs 我方發射口徑對照）。"""
import zipfile
import re
import sys

path = sys.argv[1] if len(sys.argv) > 1 else 'corpus/ooxmlexport/chart-size.docx'
needle = sys.argv[2] if len(sys.argv) > 2 else 'txbxContent'

with zipfile.ZipFile(path) as z:
    xml = z.read('word/document.xml').decode('utf-8')

i = xml.find(needle)
print('---- offset', i, '----')
print(xml[max(0, i - 300):i + 3000])
