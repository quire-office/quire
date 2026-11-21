"""bug57031：tcPr/w:shd fill 色分佈＋tblPr/w:shd fill 色分佈（燈 3 期望值覆核）。"""
import zipfile, re, collections

x = zipfile.ZipFile('corpus/poi/test-data_document_bug57031.docx').read('word/document.xml').decode('utf8')
cell_c = collections.Counter()
for m in re.finditer(r'<w:tcPr\b[^>]*>(.*?)</w:tcPr>', x, re.S):
    for s in re.finditer(r'<w:shd[^>]*?w:fill="([^"]*)"[^>]*/?>', m.group(1)):
        cell_c[s.group(1).upper()] += 1
tbl_c = collections.Counter()
for m in re.finditer(r'<w:tblPr\b[^>]*>(.*?)</w:tblPr>', x, re.S):
    for s in re.finditer(r'<w:(?:tbl)?[sS]hd[^>]*?w:fill="([^"]*)"[^>]*/?>', m.group(1)):
        tbl_c[s.group(1).upper()] += 1
print('tcPr fill 分佈:', dict(cell_c), '總=', sum(cell_c.values()))
print('tblPr fill 分佈:', dict(tbl_c), '總=', sum(tbl_c.values()))
# 自閉合 tcPr 有無
print('自閉 tcPr:', len(re.findall(r'<w:tcPr[^>]*/>', x)))
print('自閉 tblPr:', len(re.findall(r'<w:tblPr[^>]*/>', x)))
# tblPr 區塊內是否含 tblStylePr（誤計風險）
print('tblPr 區塊含 tblStylePr 個數:', sum(1 for m in re.finditer(r'<w:tblPr\b[^>]*>(.*?)</w:tblPr>', x, re.S) if 'tblStylePr' in m.group(1)))
