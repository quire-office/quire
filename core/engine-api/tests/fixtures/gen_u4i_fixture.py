#!/usr/bin/env python3
"""產生 U4i 紅燈考卷的合成最小檔（正文 caret 的 y／高度隨段落字級）。

紅燈規格（卡面 U4i 燈1–燈3）：
- P0：26pt 標題段（段內單一 run，sz=52 半磅）——驗證 caret 高度隨段級、
  y 與顯示列表 Text item 同尺。
- P1：12pt 正文段（多行繞排）——驗證大字號段之後的正文行 y 不漂移。
- P2：混級段（26pt run × 12pt run 同段）——段級取段內最大字級（斷行尺同尺）。
- P3：12pt 尾段——混級段之後的正文行 y 同尺鎖。
- P4：26pt 標題段（尾）——累積漂移偵測（若 caret 按全域 12pt 尺落 y，
  此段偏差最大）。

產生方式與本倉其他 fixture 產生器同款：zip + 五個 part，無外部依賴。
docDefaults 宣告字型（K6 校準尺生效路徑）＋ sz=24（12pt）文件基準。
"""
import os
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "u4i_mixed_size.docx")

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"""

ROOT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"""

DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"""

STYLES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:eastAsia="PMingLiU" w:hAnsi="Times New Roman"/><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"""


def para(runs_xml):
    return f"<w:p><w:r>{runs_xml}</w:r></w:p>"


def run(text, sz_halfpt=None):
    rpr = f'<w:rPr><w:sz w:val="{sz_halfpt}"/></w:rPr>' if sz_halfpt else ""
    return f"{rpr}<w:t>{text}</w:t>"


# 26pt 標題（sz 半磅制：52＝26pt）。
HEAD = "壹、壹級標題二十六點整行樣本"
# 12pt 正文多行段（12pt 全形字 39 字/行 × 4 行）。
BODY = "正文十二點樣本" * 26
# 混級段：26pt 開頭 + 12pt 續寫（段內最大字級 26pt＝斷行尺）。
MIX_HEAD = "混級段二十六點十字"
MIX_TAIL = "十二點續寫內容樣本" * 8
# 12pt 尾段。
TAIL = "尾段十二點收尾行"
# 26pt 尾標題（累積漂移偵測點）。
HEAD2 = "貳、貳級標題二十六點尾行"

DOCUMENT = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>'
    + para(run(HEAD, 52))
    + para(run(BODY))
    + f"<w:p><w:r>{run(MIX_HEAD, 52)}</w:r><w:r>{run(MIX_TAIL)}</w:r></w:p>"
    + para(run(TAIL))
    + para(run(HEAD2, 52))
    + "</w:body></w:document>"
)


def main():
    with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        z.writestr("_rels/.rels", ROOT_RELS)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/document.xml", DOCUMENT)
        z.writestr("word/styles.xml", STYLES)
    print(f"write {OUT} ({os.path.getsize(OUT)} bytes)")


if __name__ == "__main__":
    main()
