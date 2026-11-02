#!/usr/bin/env python3
"""產生 K28 紅燈合成檔（幽靈網格欄：w:tblGrid 宣告欄數多於列實際使用數）。

結構逐部對齊用戶線上實測檔 §七預算表（PM 存證 k28-repro.docx，22.6KB）的
**幾何軸**，內容換成中性佔位文字（原檔屬用戶策略資產，不入公開倉庫）：
- `w:tblGrid` 12×1560 dxa（自然 18720 dxa），每列 6 格各 span=1（只用前
  6 欄＝9360 dxa＝468pt）
- `w:tblW w:w="9360"` dxa ＋ `w:tblLayout w:type="fixed"` ＋ `w:jc w:val="left"`
- 每格 `w:tcMar` top/bottom=120、left/right=180 dxa、`w:vAlign="top"`
- 框線全 single c4c7c5 sz=6（含 insideV——燈鎖用格線 x 位置）
- pgSz 12240×15840、pgMar 1440 四邊 ⇒ 版心 468×648pt

LO 25.8 ground truth（PM 轉存 k28-lo.pdf 實測格線）：欄寬＝raw gridCol 1560
dxa＝**78.0pt**，七條豎線 72.4/150.4/228.3/306.4/384.4/462.3/540.4；總寬
468pt＝tblW（前 6 欄和＝9360 dxa，歸一本應恒等）。我方修復前把 **12 欄全
寬**等比歸一到 9360 ⇒ 每欄 39pt ⇒ 內寬 39−18＝**21pt**（LO：78−18＝60pt）
⇒ `Worker`（≈36pt）/'$10.00'（≈33pt）等 atom 比行寬寬 ⇒ KP 緊急切分按
cluster 邊界逐字鋸開（實測 `'Alp'/'ha'`、`'$10.'/'00'`、CJK 逐字直排）——
正是用戶手機截圖所見（真檔 LO 7 頁 vs 我方 11 頁）。

產生方式與本倉其他 fixture 產生器同款：zip＋五個 part，無外部依賴。
"""
import os
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "k28_ghost_grid.docx")

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"""

ROOT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"""

DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"""

STYLES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:tblPr><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style></w:styles>"""

BORDERS = (
    "<w:tblBorders>"
    + "".join(
        f'<w:{edge} w:color="c4c7c5" w:space="0" w:sz="6" w:val="single"/>'
        for edge in ("top", "left", "bottom", "right", "insideH", "insideV")
    )
    + "</w:tblBorders>"
)

TCMAR = (
    "<w:tcMar>"
    '<w:top w:w="120.0" w:type="dxa"/>'
    '<w:left w:w="180.0" w:type="dxa"/>'
    '<w:bottom w:w="120.0" w:type="dxa"/>'
    '<w:right w:w="180.0" w:type="dxa"/>'
    "</w:tcMar>"
)

RPR = '<w:rPr><w:sz w:val="22"/></w:rPr>'


def cell(text):
    return (
        "<w:tc><w:tcPr>"
        f'{BORDERS}{TCMAR}<w:vAlign w:val="top"/></w:tcPr>'
        "<w:p><w:pPr>"
        f'<w:spacing w:after="120" w:before="120"/>{RPR}</w:pPr>'
        f'<w:r>{RPR}<w:t xml:space="preserve">{text}</w:t></w:r></w:p></w:tc>'
    )


def row(cells):
    return "<w:tr>" + "".join(cell(c) for c in cells) + "</w:tr>"


# 9 列 × 6 欄（幾何軸＝用戶實測檔；文字為中性佔位，策略資產不入庫）。判別軸：
# - 第 2 欄：長 Latin 詞（'Worker'≈36pt：buggy 21pt 行寬必鋸、fixed 60pt 完整）
#   ＋金額（LB25 數字串 '$10.00'≈33pt 同理被鋸）。
# - 第 1 欄：CJK 短詞（逐字直排判別軸）。
ROWS = [
    ["項目分類", "資源規格與用途", "Alpha 0 (單機驗證)", "Alpha 1 (集中營運)",
     "Alpha 2 (管線建置)", "Alpha 3 (分片啟動)"],
    ["邊緣層", "Edge Worker Paid ($5) + KV / DO 計量",
     "$0.00", "$5.00", "$10.00", "$15.00"],
    ["主機層", "Host Node (64GB RAM, NVMe)",
     "$45.00", "$45.00", "$45.00", "$90.00（雙機備援）"],
    ["批次層", "無頭瀏覽器沙盒運算（動態抓取）",
     "$0.00", "$10.00", "$25.00", "$40.00"],
    ["監控層", "Log Tracker 追蹤 + 基礎監控",
     "$0.00", "$0.00", "$12.00", "$20.00"],
    ["合規層", "登記、申報與諮詢費用",
     "$0.00", "$20.00", "$20.00", "$20.00"],
    ["網域層", "商業網域名稱 (.com)",
     "$1.20", "$1.20", "$1.20", "$1.20"],
    ["配額層", "進階方案：$15 美元 / 月，包含 3,000 次查詢（超出按量計費：$0.004 / 次）。",
     "$0.00", "$15.00", "$15.00", "$15.00"],
    ["每月合計", "", "$46.20", "$84.20", "$123.20", "$211.20"],
]

TABLE = (
    "<w:tbl><w:tblPr>"
    '<w:tblStyle w:val="TableNormal"/>'
    '<w:tblW w:w="9360.0" w:type="dxa"/>'
    '<w:jc w:val="left"/>'
    f"{BORDERS}"
    '<w:tblLayout w:type="fixed"/>'
    '<w:tblLook w:val="0600"/>'
    "</w:tblPr>"
    "<w:tblGrid>" + '<w:gridCol w:w="1560"/>' * 12 + "</w:tblGrid>"
    + "".join(row(r) for r in ROWS)
    + "</w:tbl>"
)

# 表後一段純 CJK 長句（燈 2：CJK 逐字可斷仍成立——多行、不溢出、禁則安全）。
CJK_PARA = (
    "<w:p><w:pPr><w:spacing w:after=\"120\"/></w:pPr>"
    f"<w:r>{RPR}<w:t xml:space=\"preserve\">成本模型收錄邊緣層、主機層、批次層、"
    "監控層、合規層、網域層與配額層等七類資源，金額一律以美元計價，並含隱性支出"
    "（例如維運人力、教育訓練與備援切換所衍生之固定成本）。各層資源於驗證通過後"
    "逐階段啟動，未通過者依停損原則果斷退場，避免過度工程拖垮整體進度。</w:t></w:r></w:p>"
)

# 表前一段：中英混排＋括號禁則形狀，提供正文斷行參照。
PRE_PARA = (
    "<w:p><w:pPr><w:spacing w:after=\"120\"/></w:pPr>"
    f'<w:r>{RPR}<w:t xml:space="preserve">成本模型（含隱性支出）：本表收錄邊緣層、'
    "主機與配額三類資源，金額以美元計（USD）。Edge Worker 與 Host Node 的組合"
    "在 Alpha 0 驗證通過後才啟動。</w:t></w:r></w:p>"
)

SECTPR = (
    "<w:sectPr>"
    '<w:pgSz w:h="15840" w:w="12240" w:orient="portrait"/>'
    '<w:pgMar w:bottom="1440" w:top="1440" w:left="1440" w:right="1440" '
    'w:header="0" w:footer="720"/>'
    "</w:sectPr>"
)

DOCUMENT = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
    f"<w:body>{PRE_PARA}{TABLE}{CJK_PARA}{SECTPR}</w:body></w:document>"
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
