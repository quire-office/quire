"""s1026 完整屬性＋txbxContent 首段 pPr：找 insetpen／段落縮排等源檔事實（D9b 第 2 條根植）。"""
import zipfile
import xml.etree.ElementTree as ET

def local(tag):
    return tag.split('}')[-1]

z = zipfile.ZipFile('corpus/ooxmlexport/fdo76591.docx')
raw = z.read('word/document.xml').decode('utf-8')

# 1) s1026 的 v:shape 開始標籤全文
i = raw.find('_x0000_s1026')
start = raw.rfind('<v:shape', 0, i)
end = raw.find('>', start)
print("=== s1026 <v:shape ...> 全文 ===")
print(raw[start:end + 1])
# 相鄰的 o: 屬性群（含 insetpen）
tail = raw[end + 1:end + 400]
print("=== s1026 shape 之後 400 chars ===")
print(tail)
print()
# 2) txbxContent 第一段完整 XML
j = raw.find('_x0000_s1026')
k = raw.find('<w:txbxContent', j)
frag = raw[k:k + 1500]
print("=== s1026 txbxContent 前 1500 chars ===")
print(frag)
