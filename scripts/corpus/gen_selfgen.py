#!/usr/bin/env python3
"""自產多樣 DOCX（純 stdlib，手工 OOXML）——補 corpus 的 CJK/嵌入字型/首字大寫 等面向。

產生 pool/selfgen__*.docx；每個檔都是合法 zip+OOXML（Word/LibreOffice 可開）。
"""
from __future__ import annotations

import pathlib
import sys
import uuid
import zipfile
from datetime import datetime, timezone

HERE = pathlib.Path(__file__).parent
POOL = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else HERE) / "pool"
POOL.mkdir(exist_ok=True)

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
R = 'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
M = 'xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"'
V = 'xmlns:v="urn:schemas-microsoft-com:vml"'
MC = 'xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"'
O = 'xmlns:o="urn:schemas-microsoft-com:office:office"'
WP = 'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"'
A = 'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"'
PIC = 'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"'

NOW = "2026-10-14T09:00:00Z"


def rels(entries: list[tuple]) -> str:
    body = ""
    for e in entries:
        i, t, g = e[0], e[1], e[2]
        ext = ' TargetMode="External"' if len(e) > 3 and e[3] else ""
        body += (f'<Relationship Id="{i}" '
                 f'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{t}" Target="{g}"{ext}/>')
    return f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>'


def ct(extras: dict[str, str], defaults: dict[str, str] | None = None) -> str:
    d = {"rels": "application/vnd.openxmlformats-package.relationships+xml",
         "xml": "application/xml"}
    d.update(defaults or {})
    overrides = "".join(f'<Override PartName="{p}" ContentType="{c}"/>' for p, c in extras.items())
    defaults_s = "".join(f'<Default Extension="{e}" ContentType="{c}"/>' for e, c in d.items())
    return (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            f'<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{defaults_s}{overrides}</Types>')


CT_DOC = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
CT_STYLES = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"
CT_SETTINGS = "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"
CT_NUMBERING = "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"
CT_FONTS = "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"
CT_HEADER = "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"
CT_FOOTER = "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"
CT_FNOTES = "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"
CT_ENDNOTES = "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"
CT_COMMENTS = "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"
CT_GLOSSARY = "application/vnd.openxmlformats-officedocument.wordprocessingml.glossaryDocument+xml"
CT_CORE = "application/vnd.openxmlformats-package.core-properties+xml"
CT_APP = "application/vnd.openxmlformats-officedocument.extended-properties+xml"
CT_ODTTF = "application/vnd.openxmlformats-officedocument.obfuscatedFont"


def core_xml(title, subject, author="Quire Corpus Generator"):
    return (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            f'<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
            f'xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" '
            f'xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">'
            f'<dc:title>{title}</dc:title><dc:subject>{subject}</dc:subject><dc:creator>{author}</dc:creator>'
            f'<dcterms:created xsi:type="dcterms:W3CDTF">{NOW}</dcterms:created></cp:coreProperties>')


APP_XML = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
           '<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" '
           'xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">'
           '<Application>Quire SelfGen</Application></Properties>')

STYLES_MIN = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {W}>'
              '<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:eastAsia="PMingLiU" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>'
              '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
              '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/>'
              '<w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>'
              '<w:style w:type="character" w:styleId="Strong"><w:name w:val="Strong"/><w:rPr><w:b/></w:rPr></w:style>'
              '</w:styles>')


def sect(cols: int = 1, header_ids: str = "", extra: str = "") -> str:
    c = f'<w:cols w:num="{cols}"/>' if cols > 1 else '<w:cols w:num="1"/>'
    return f'<w:sectPr>{header_ids}{extra}{c}<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708"/></w:sectPr>'


def para(text: str = "", ppr: str = "", runs: str = "") -> str:
    runs = runs or (f'<w:r><w:t xml:space="preserve">{text}</w:t></w:r>' if text else "")
    return f'<w:p>{"<w:pPr>" + ppr + "</w:pPr>" if ppr else ""}{runs}</w:p>'


def document(body: str, sect_xml: str) -> str:
    return (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W} {R} {M} {MC} {V} {O}> <w:body>{body}{sect_xml}</w:body></w:document>')


def write_docx(name: str, parts: dict[str, str], binary: dict[str, bytes] | None = None):
    out = POOL / f"selfgen__{name}"
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for pn, content in parts.items():
            z.writestr(pn, content)
        for pn, data in (binary or {}).items():
            z.writestr(pn, data)
    print("gen", out.name)


def obfuscate_font(ttf: bytes, guid: uuid.UUID) -> bytes:
    # MS-OFFCRYPTO ODTTF 偽裝金鑰：Data1/2/3 小端、Data4 大端
    b = guid.bytes_le[:4] + guid.bytes_le[4:6] + guid.bytes_le[6:8] + guid.bytes[8:16]
    hdr = (0x00010000).to_bytes(4, "little") + b"ODTF" + (0x00010000).to_bytes(4, "little") + \
          (0).to_bytes(4, "little") + (0).to_bytes(8, "little") + (0).to_bytes(8, "little")
    first = ttf[:32]
    mask = b + b  # 16 bytes repeated twice = 32
    xord = bytes(x ^ m for x, m in zip(first, mask))
    return hdr + xord + ttf[32:]


def find_ttf() -> bytes:
    for p in ["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
              "/usr/share/fonts/dejavu/DejaVuSans.ttf",
              "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf"]:
        f = pathlib.Path(p)
        if f.exists():
            return f.read_bytes()
    raise SystemExit("no TTF found for embedded-font sample")


BASE_RELS = lambda: [("_rels", "officeDocument", "word/document.xml"),
                     ("docProps", "core-properties", "../docProps/core.xml"),
                     ("docProps", "extended-properties", "../docProps/app.xml")]


def std_parts(body, sect_xml, *, styles=STYLES_MIN, settings="", numbering=None, extra_ct=None,
              extra_rels=None, headers=None, footers=None, footnotes=None, endnotes=None,
              fonttable=None, settings_ct=True):
    rel_entries = [("rId1", "styles", "styles.xml")]
    ct_extras = {"/word/document.xml": CT_DOC, "/word/styles.xml": CT_STYLES}
    n = 1
    if settings:
        n += 1
        rel_entries.append((f"rId{n}", "settings", "settings.xml"))
        ct_extras["/word/settings.xml"] = CT_SETTINGS
    if numbering:
        n += 1
        rel_entries.append((f"rId{n}", "numbering", "numbering.xml"))
        ct_extras["/word/numbering.xml"] = CT_NUMBERING
    if fonttable:
        n += 1
        rel_entries.append((f"rId{n}", "fontTable", "fontTable.xml"))
        ct_extras["/word/fontTable.xml"] = CT_FONTS
    header_rids: dict[str, str] = {}
    for i, (pn, xml) in enumerate(headers or [], start=1):
        n += 1
        rel_entries.append((f"rId{n}", "header", pn))
        header_rids[f"%%HDR{i}%%"] = f'r:id="rId{n}"'
        ct_extras[f"/word/{pn}"] = CT_HEADER
    footer_rids: dict[str, str] = {}
    for i, (pn, xml) in enumerate(footers or [], start=1):
        n += 1
        rel_entries.append((f"rId{n}", "footer", pn))
        footer_rids[f"%%FTR{i}%%"] = f'r:id="rId{n}"'
        ct_extras[f"/word/{pn}"] = CT_FOOTER
    if footnotes:
        n += 1
        rel_entries.append((f"rId{n}", "footnotes", "footnotes.xml"))
        ct_extras["/word/footnotes.xml"] = CT_FNOTES
    if endnotes:
        n += 1
        rel_entries.append((f"rId{n}", "endnotes", "endnotes.xml"))
        ct_extras["/word/endnotes.xml"] = CT_ENDNOTES
    for rid, typ, target, ctype, *rest in (extra_rels or []):
        n += 1
        ext = rest[0] if rest else False
        rel_entries.append((rid, typ, target, ext))
        if ctype:
            ct_extras[f"/word/{target}"] = ctype
    parts = {
        "[Content_Types].xml": ct(ct_extras, extra_ct),
        "_rels/.rels": rels([("rId1", "officeDocument", "word/document.xml"),
                             ("rId2", "core-properties", "docProps/core.xml"),
                             ("rId3", "extended-properties", "docProps/app.xml")]),
        "docProps/core.xml": core_xml("Quire corpus 自產文件", "測試語料"),
        "docProps/app.xml": APP_XML,
        "word/document.xml": document(body, sect_xml),
        "word/styles.xml": styles,
        "word/_rels/document.xml.rels": rels(rel_entries),
    }
    # resolve %%HDR1%%/%%FTR1%% placeholders with the rIds assigned above
    doc = parts["word/document.xml"]
    for k, v in header_rids.items():
        doc = doc.replace(k, v)
    for k, v in footer_rids.items():
        doc = doc.replace(k, v)
    parts["word/document.xml"] = doc
    if settings:
        parts["word/settings.xml"] = settings
    if numbering:
        parts["word/numbering.xml"] = numbering
    if fonttable:
        parts["word/fontTable.xml"] = fonttable
    for pn, xml in (headers or []):
        parts[f"word/{pn}"] = xml
    for pn, xml in (footers or []):
        parts[f"word/{pn}"] = xml
    if footnotes:
        parts["word/footnotes.xml"] = footnotes
    if endnotes:
        parts["word/endnotes.xml"] = endnotes
    return parts


FOOTNOTES_XML = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:footnotes {W}>'
                 '<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>'
                 '<w:footnote w:id="1"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> 註腳一：資料來源為內部統計報表。</w:t></w:r></w:p></w:footnote>'
                 '</w:footnotes>')
ENDNOTES_XML = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:endnotes {W}>'
                '<w:endnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:endnote>'
                '<w:endnote w:id="1"><w:p><w:r><w:endnoteRef/></w:r><w:r><w:t xml:space="preserve"> 尾註一：參考附件二之說明。</w:t></w:r></w:p></w:endnote>'
                '</w:endnotes>')


def hdr(pn, text, page_field=False):
    pf = '<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>' if page_field else ""
    return pn, (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {W} {R}><w:p>'
                f'<w:r><w:t xml:space="preserve">{text}</w:t></w:r>{pf}</w:p></w:hdr>')


def ftr(pn, text, page_field=True):
    pf = '<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>' if page_field else ""
    return pn, (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:ftr {W} {R}><w:p>'
                f'<w:r><w:t xml:space="preserve">{text}</w:t></w:r>{pf}</w:p></w:ftr>')


def tc(paras: list[str], w: int = 2000, *, gridspan: int = 0, vmerge: str = "",
       wtype: str = "dxa", numpr: bool = False, inner: str = "") -> str:
    """一個儲存格：paras 逐段輸出；numpr=True 每段掛清單；inner 追加巢狀表格 XML。"""
    tcpr = f'<w:tcW w:w="{w}" w:type="{wtype}"/>'
    if gridspan:
        tcpr += f'<w:gridSpan w:val="{gridspan}"/>'
    if vmerge == "restart":
        tcpr += '<w:vMerge w:val="restart"/>'
    elif vmerge == "cont":
        tcpr += "<w:vMerge/>"
    ppr = "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>" if numpr else ""
    body = "".join(para(t, ppr) for t in paras) + inner
    return f"<w:tc><w:tcPr>{tcpr}</w:tcPr>{body}</w:tc>"


def tbl(*, cols: int, rows: str, tblpr_extra: str = "", grid_cols: str = "") -> str:
    """cols＝grid 欄數（gridCol 等寬 w=9000//cols）；rows＝預先組好的 w:tr XML 串接。"""
    g = grid_cols or "".join("<w:gridCol w:w=\"{}\"/>".format(9000 // cols) for _ in range(cols))
    return ("<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/>"
            + tblpr_extra
            + "</w:tblPr><w:tblGrid>" + g + "</w:tblGrid>" + rows + "</w:tbl>")


def main():
    # 1) 繁體中文商業報告：表格(合併)＋頁首頁尾＋頁碼欄位＋樣式＋編號清單
    tbl = ('<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="0" w:type="auto"/><w:tblBorders>'
           '<w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/></w:tblBorders></w:tblPr>'
           '<w:tr><w:trPr><w:tblHeader/></w:trPr>'
           '<w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>項目</w:t></w:r></w:p></w:tc>'
           '<w:tc><w:tcPr><w:tcW w:w="2500" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>第一季</w:t></w:r></w:p></w:tc>'
           '<w:tc><w:tcPr><w:tcW w:w="2500" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>第二季</w:t></w:r></w:p></w:tc></w:tr>'
           '<w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>營收（新臺幣千元）</w:t></w:r></w:p></w:tc>'
           '<w:tc><w:tcPr><w:tcW w:w="5000" w:type="dxa"/><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>12,345</w:t></w:r></w:p></w:tc></w:tr>'
           '<w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>成本</w:t></w:r></w:p></w:tc>'
           '<w:tc><w:tcPr><w:tcW w:w="2500" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>7,890</w:t></w:r></w:p></w:tc>'
           '<w:tc><w:tcPr><w:tcW w:w="2500" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>8,012</w:t></w:r></w:p></w:tc></w:tr>'
           '<w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>備註</w:t></w:r></w:p></w:tc>'
           '<w:tc><w:tcPr><w:tcW w:w="5000" w:type="dxa"/><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>資料為試算</w:t></w:r></w:p></w:tc></w:tr>'
           '<w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>'
           '<w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p><w:r><w:t>（續上欄）</w:t></w:r></w:p></w:tc></w:tr>'
           '</w:tbl>')
    numbering = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering {W}>'
                 '<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum>'
                 '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>')
    body = (para("第一季營運報告", "Heading1" and '<w:pStyle w:val="Heading1"/>')
            + para("本季整體表現符合預期，以下為重點摘要。")
            + para("营收成長來自海外訂單。" and "營收成長來自海外訂單。", '<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>')
            + para("毛利率因規模效應提升。", '<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>')
            + para("財務摘要表如下：")
            + tbl
            + para("（本文件為 Quire 語料庫自產文件，無商業意涵。）"))
    sect_xml = ('<w:sectPr><w:headerReference %%HDR1%% r:type="default" w:id="0"/><w:footerReference %%FTR1%% r:type="default" w:id="1"/>'
                '<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708"/></w:sectPr>')
    parts = std_parts(body, sect_xml, numbering=numbering,
                      headers=[hdr("header1.xml", "開源文書處理器 — 智慧財產局內部測試")],
                      footers=[ftr("footer1.xml", "第 ")])
    write_docx("cjk_traditional_report.docx", parts)

    # 2) 修訂追蹤：插入/刪除/格式變更/移動
    body = (para("修訂追蹤測試文件", '<w:pStyle w:val="Heading1"/>')
            + para("", "", '<w:ins w:id="1" w:author="王小明" w:date="2026-10-01T10:00:00Z"><w:r><w:t>這段文字是新增的。</w:t></w:r></w:ins>'
                   '<w:del w:id="2" w:author="李小明" w:date="2026-10-01T10:05:00Z"><w:r><w:delText xml:space="preserve">這段文字被刪除了。</w:delText></w:r></w:del>')
            + para("", "", '<w:r><w:rPr><w:rPrChange w:id="3" w:author="王小明" w:date="2026-10-01T10:10:00Z"><w:rPr><w:i/></w:rPr></w:rPrChange></w:rPr><w:t>這段文字變更了格式。</w:t></w:r>')
            + para("移動來源（原始位置）", '<w:pPrChange w:id="4"><w:pPr/></w:pPrChange>')
            + para("", "", '<w:moveFrom w:id="5" w:author="王小明" w:date="2026-10-01T10:20:00Z"><w:r><w:t>被移動的段落</w:t></w:r></w:moveFrom>')
            + para("", "", '<w:moveTo w:id="6" w:author="王小明" w:date="2026-10-01T10:20:00Z"><w:r><w:t>移動目的地</w:t></w:r></w:moveTo>'))
    parts = std_parts(body, sect())
    write_docx("tracked_revisions_mixed.docx", parts)

    # 3) 嵌入字型（odttf obfuscated DejaVuSans）
    guid = uuid.UUID("3F2504E0-4F89-11D3-9A0C-0305E82C3301")
    ttf = find_ttf()
    fonttable = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:fonts {W} {R}>'
                 f'<w:font w:name="DejaVuSans (Embedded)"><w:embedRegular r:id="rIdF1" w:fontKey="{{{guid}}}" w:type="regular"/></w:font></w:fonts>')
    settings = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings {W}>'
                '<w:embedTrueTypeFonts/><w:defaultTabStop w:val="720"/></w:settings>')
    body = para("本文件內嵌 DejaVu Sans 字型以測試 embedded font 解析路徑。", "")
    parts = std_parts(body, sect(), settings=settings, fonttable=fonttable,
                      extra_rels=[("rIdF1", "font", "fonts/odtt1.odttf", CT_ODTTF)])
    write_docx("embedded_font_odttf.docx", parts, binary={"word/fonts/odtt1.odttf": obfuscate_font(ttf, guid)})

    # 4) 首字大寫（drop cap）＋欄位（目錄）
    body = (para("目錄", '<w:pStyle w:val="Heading1"/>')
            + '<w:p><w:r><w:fldChar w:fldCharType="begin" w:dirty="true"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \\o "1-3" \\h \\z \\u </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>（按 F9 更新目錄）</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>'
            + para("第一章 總則", '<w:pStyle w:val="Heading1"/>')
            + para("", "", '<w:r><w:rPr><w:framePr w:wrap="around" w:vAlign="text" w:dropCap="drop" w:lines="3"/></w:rPr><w:t>開</w:t></w:r>'
                   '<w:r><w:t>源文書處理器的目標是以單一 Rust 核心同時支撐桌面與瀏覽器兩個殼層。本章說明整體架構與分層原則。</w:t></w:r>'))
    parts = std_parts(body, sect())
    write_docx("dropcap_toc_fields.docx", parts)

    # 5) 多欄＋首頁不同頁首＋符號＋上標/下標＋刪除線＋螢光標
    body = (para("雙欄排版與行內效果測試", '<w:pStyle w:val="Heading1"/>')
            + para("", "", '<w:r><w:rPr><w:strike/><w:highlight w:val="yellow"/></w:rPr><w:t>刪除線加螢光</w:t></w:r>'
                   '<w:r><w:t xml:space="preserve">、化学式 H</w:t></w:r>'
                   '<w:r><w:rPr><w:vertAlign w:val="subscript"/></w:rPr><w:t>2</w:t></w:r>'
                   '<w:r><w:t>O 與 E=mc</w:t></w:r>'
                   '<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t>2</w:t></w:r>'
                   '<w:r><w:t xml:space="preserve">，符號 </w:t></w:r>'
                   '<w:r><w:sym w:font="Cambria Math" w:char="F020"/></w:r>')
            + para("第二欄文字：分頁引擎必須在欄高限制內斷行，CJK 支援任意字斷行，Latin 以空白與連字號優先。")
            + para("第三段：驗證多欄文件的分頁與欄平衡行為是否正確。"))
    sect_xml = ('<w:sectPr><w:titlePg/><w:headerReference %%HDR1%% r:type="first" w:id="0"/>'
                '<w:cols w:num="2"/><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708"/></w:sectPr>')
    parts = std_parts(body, sect_xml, headers=[hdr("header1.xml", "首頁頁首（與內文不同）")])
    write_docx("two_columns_inline_effects.docx", parts)

    # 6) 阿拉伯語 RTL＋註腳＋尾註
    body = (para("اختبار مستند عربي", '<w:pStyle w:val="Heading1"/><w:bidi/>')
            + para("هذا نص تجريبي لاتجاه من اليمين إلى اليسار مع مرجع.", '<w:bidi/>',
                   '<w:r><w:t>هذا نص تجريبي لاتجاه من اليمين إلى اليسار مع مرجع</w:t></w:r>'
                   '<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteReference w:id="1"/></w:r>'
                   '<w:r><w:t>.</w:t></w:r>')
            + para("ملاحظة أخيرة مع حاشية سفلية.", '<w:bidi/>',
                   '<w:r><w:t>ملاحظة أخيرة مع حاشية سفلية</w:t></w:r>'
                   '<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:endnoteReference w:id="1"/></w:r>'
                   '<w:r><w:t>.</w:t></w:r>'))
    parts = std_parts(body, sect(), footnotes=FOOTNOTES_XML, endnotes=ENDNOTES_XML)
    write_docx("arabic_rtl_notes.docx", parts)

    # 7) 內容控件＋舊式 VML 圖形＋文字框
    body = (para("內容控件與舊式圖形", '<w:pStyle w:val="Heading1"/>')
            + '<w:sdt><w:sdtPr><w:alias w:val="標題控件"/><w:tag w:val="title"/></w:sdtPr><w:sdtContent>'
            + para("此為純文字內容控件的值。") + '</w:sdtContent></w:sdt>'
            + '<w:p><w:r><w:t>下方為 VML 文字框：</w:t></w:r></w:p>'
            + '<w:p><w:pict><v:rect style="width:200pt;height:60pt" o:spid="_x0000_s1026" fillcolor="#FFFFCC" strokeweight="1pt">'
              '<v:textbox inset="0,0,0,0"><w:txbxContent>' + para("VML 文字框內容") + '</w:txbxContent></v:textbox></v:rect></w:pict></w:p>')
    parts = std_parts(body, sect())
    write_docx("sdt_vml_textbox.docx", parts)

    # 8) 日文（ルビ/振假名）＋簡體中文對照＋符號
    body = (para("日本語文書テスト", '<w:pStyle w:val="Heading1"/>')
            + '<w:p><w:r><w:ruby><w:rubyPr/><w:rt><w:r><w:rPr><w:sz w:val="12"/></w:rPr><w:t>にほん</w:t></w:r></w:rt><w:rb><w:r><w:t>日本</w:t></w:r></w:rb></w:ruby></w:r>'
              '<w:r><w:t>語の原稿です。</w:t></w:r></w:p>'
              + para("简体中文对照段落：本段落用于测试简体中文语料解析。")
              + para("混排：開源、開源、开源、日本語、한국어。"))
    parts = std_parts(body, sect())
    write_docx("cjk_japanese_ruby.docx", parts)

    # 9) 超連結＋書籤＋批註＋快速部件(glossary part)
    body = (para("連結、書籤與批註測試", '<w:pStyle w:val="Heading1"/>')
            + '<w:p><w:bookmarkStart w:id="0" w:name="目標位置"/><w:hyperlink r:id="rIdLink"><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>官網連結</w:t></w:r></w:hyperlink>'
              '<w:r><w:t>　</w:t></w:r><w:hyperlink w:anchor="目標位置"><w:r><w:t>檔內跳轉</w:t></w:r></w:hyperlink><w:bookmarkEnd w:id="0"/></w:p>'
            + para("下面這段會被批註。", "", '<w:r><w:t>下面這段會被批註。</w:t></w:r>'
                   '<w:commentRangeStart w:id="1"/><w:r><w:t>被批註的文字。</w:t></w:r><w:commentRangeEnd w:id="1"/>'
                   '<w:r><w:rPr><w:rStyle w:val="Strong"/></w:rPr><w:commentReference w:id="1"/></w:r>'))
    comments = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:comments {W}>'
                '<w:comment w:id="1" w:author="審稿人" w:date="2026-10-02T08:00:00Z" w:initials="S">'
                '<w:p><w:r><w:t>這段數據需要更新。</w:t></w:r></w:p></w:comment></w:comments>')
    glossary = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:glossaryDocument {W}>'
                '<w:docParts><w:docPart><w:docPartPr><w:name w:val="標準條款"/><w:category><w:name w:val="條款"/></w:category>'
                '<w:types><w:type w:val="paragraph"/></w:types></w:docPartPr><w:docPartBody>'
                + para("標準條款：本契約雙方應本誠信原則履行。") + '</w:docPartBody></w:docPart></w:docParts></w:glossaryDocument>')
    parts = std_parts(body, sect(),
                      extra_rels=[("rIdLink", "hyperlink", "https://quire.example.org/", None, True),
                                  ("rIdCom", "comments", "comments.xml", CT_COMMENTS),
                                  ("rIdGlo", "glossaryDocument", "glossary/document.xml", CT_GLOSSARY)])
    parts["word/comments.xml"] = comments
    parts["word/glossary/document.xml"] = glossary
    write_docx("hyperlink_bookmark_comments_glossary.docx", parts)

    # 10) altChunk（外部 HTML 片段）＋自訂 XML
    body = (para("altChunk 外部內容測試", '<w:pStyle w:val="Heading1"/>')
            + '<w:altChunk r:id="chunk1"/>'
            + para("altChunk 之後的段落。"))
    custom = ('<?xml version="1.0" encoding="UTF-8"?><metadata><field name="docno">OW-2026-001</field></metadata>')
    parts = std_parts(body, sect(), extra_ct={"html": "text/html"},
                      extra_rels=[("rIdChunk", "altChunk", "altchunk1.html", None)])
    parts["word/altchunk1.html"] = "<html><body><p>altChunk 注入的 HTML 段落。</p></body></html>"
    parts["customXml/item1.xml"] = custom
    write_docx("altchunk_customxml.docx", parts)

    # 11) 數學方程式（Office Math）＋段落框線底紋
    body = (para("方程式測試", '<w:pStyle w:val="Heading1"/>')
            + '<m:oMathPara><m:oMath><m:sSup><m:e><mi>E</mi></m:e><m:sup><mi>2</mi></m:sup></m:sSup><mo>=</mo><m:sSup><m:e><mi>m</mi></m:e><m:sup><mi>2</mi></m:sup></m:sSup><m:cFunction><mi>f</mi></m:cFunction><mo>(</mo><mi>x</mi><mo>)</mo></m:oMath></m:oMathPara>'
            + para("带边框底纹的段落。（此段刻意使用简体中文，测试 CJK 解析）", '<w:pBdr><w:top w:val="double" w:sz="6"/><w:bottom w:val="single" w:sz="4"/></w:pBdr><w:shd w:val="clear" w:fill="E6F2FF"/>'))
    parts = std_parts(body, sect())
    write_docx("equation_paraborder.docx", parts)

    # 12) 嵌入 OLE（xlsx）＋欄位（STYLEREF/PAGE）＋分節多節
    body = (para("第一節", '<w:pStyle w:val="Heading1"/>')
            + '<w:p><w:r><w:object><v:shape id="ole1" style="width:200pt;height:100pt"><v:imagedata r:id="rIdImg" o:title=""/></v:shape><o:OLEObject Type="Embed" ProgID="Excel.Sheet.12" ShapeID="ole1" DrawAspect="Content" ObjectID="_1234567890" r:id="rIdOle"/></w:object></w:r></w:p>'
            + '<w:p><w:pPr><w:sectPr><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:pPr></w:p>'
            + para("第二節（橫向）", '<w:pStyle w:val="Heading1"/>')
            + '<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> STYLEREF "Heading 1" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t xml:space="preserve"> — 頁 </w:t></w:r>'
              '<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>')
    png = bytes.fromhex("89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000a49444154789c63000100000500010d0a2db40000000049454e44ae426082")
    import io
    xlsx_buf = io.BytesIO()
    with zipfile.ZipFile(xlsx_buf, "w", zipfile.ZIP_DEFLATED) as xz:
        xz.writestr("[Content_Types].xml",
                    '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
                    '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>')
        xz.writestr("_rels/.rels",
                    '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>')
        xz.writestr("xl/workbook.xml",
                    '<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheets><sheet name="S1" sheetId="1"/></sheets></workbook>')
    parts = std_parts(body, sect(cols=1),
                      extra_rels=[("rIdOle", "oleObject", "embeddings/ole1.xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                                  ("rIdImg", "image", "media/image1.png", "image/png")],
                      extra_ct={"png": "image/png", "xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"})
    write_docx("ole_embedded_landscape_sections.docx", parts,
               binary={"word/embeddings/ole1.xlsx": xlsx_buf.getvalue(), "word/media/image1.png": png})


NUMBERING_LIST = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering {W}>'
                  '<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="•"/><w:pPr><w:ind w:left="360" w:hanging="180"/></w:pPr></w:lvl></w:abstractNum>'
                  '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>')


def gen_pathological_tables():
    """PM 病態表格量化門檻補齊（2026-10-16）：
    nested_table_3plus / table_cell_multi_para / table_cell_list /
    table_across_pages / tbl_layout_autofit / merged_interleave / table_in_columns。
    """
    # P1 深巢狀表格（3 層：外→中→內）＋ cell 多段落
    inner = tbl(cols=2, rows="".join(
        "<w:tr>" + tc([f"內格 R{r}C1"], 1500) + tc([f"內格 R{r}C2"], 1500) + "</w:tr>"
        for r in range(3)))
    mid = tbl(cols=2, rows="".join(
        "<w:tr><w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>"
        + para(f"中層第 {r} 列") + inner + "</w:tc>" + tc([f"中欄 {r}"], 3000) + "</w:tr>"
        for r in range(2)))
    outer = tbl(cols=2, rows=(
        "<w:tr><w:tc><w:tcPr><w:tcW w:w=\"4500\" w:type=\"dxa\"/></w:tcPr>"
        + para("外層 A：巢狀深度 3 起點") + mid + "</w:tc>"
        + tc(["外層 B", "第二段", "第三段"], 4500) + "</w:tr>"))
    body = (para("深巢狀表格（外→中→內三層）＋儲存格多段落", '<w:pStyle w:val="Heading1"/>')
            + para("以下表格嵌套三層 w:tbl，驗證遞迴解析與無損回寫。") + outer
            + para("巢狀表格之後的段落。"))
    write_docx("patho_nested_table_3deep.docx", std_parts(body, sect()))

    # P1b 深巢狀表格變體（3 層反方向：內層兩張並排）＋ cell 清單
    innerA = tbl(cols=2, rows="".join(
        "<w:tr>" + tc([f"α{r}"], 1200, numpr=True) + tc([f"β{r}"], 1200) + "</w:tr>"
        for r in range(2)))
    innerB = tbl(cols=1, rows="".join(
        "<w:tr>" + tc([f"γ{r}", "第二段", "第三段"], 1800) + "</w:tr>" for r in range(2)))
    mid2 = tbl(cols=2, rows=(
        "<w:tr><w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>"
        + para("中層左：並排 A") + innerA + "</w:tc>"
        "<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>"
        + para("中層右：並排 B") + innerB + "</w:tc></w:tr>"))
    outer2 = tbl(cols=1, rows=(
        "<w:tr><w:tc><w:tcPr><w:tcW w:w=\"9000\" w:type=\"dxa\"/></w:tcPr>"
        + para("外層單欄") + mid2 + "</w:tc></w:tr>"))
    body = (para("深巢狀＋儲存格內清單", '<w:pStyle w:val="Heading1"/>')
            + para("每格都掛 numPr 清單且巢狀三層。") + outer2)
    write_docx("patho_nested_table_lists.docx", std_parts(body, sect(), numbering=NUMBERING_LIST))

    # P1c 3 層巢狀（表格內表格內表格，深度計數器不同路徑）
    deep3 = tbl(cols=3, rows="".join(
        "<w:tr>" + "".join(tc([f"L3({r},{c})"], 900) for c in range(3)) + "</w:tr>"
        for r in range(2)))
    deep2 = tbl(cols=2, rows="".join(
        "<w:tr>" + f"<w:tc><w:tcPr><w:tcW w:w=\"2400\" w:type=\"dxa\"/></w:tcPr>{deep3}</w:tc>"
        + tc([f"L2 r{r}"], 2400) + "</w:tr>" for r in range(2)))
    deep1 = tbl(cols=2, rows=(
        f"<w:tr><w:tc><w:tcPr><w:tcW w:w=\"4500\" w:type=\"dxa\"/></w:tcPr>{deep2}</w:tc>"
        + tc(["L1 對照格", "第二", "第三"], 4500) + "</w:tr>"))
    body = (para("三層巢狀變體 C", '<w:pStyle w:val="Heading1"/>') + deep1)
    write_docx("patho_nested_table_deep3c.docx", std_parts(body, sect()))

    # P1d 4 層巢狀（超過 3 層門檻的保險樣本）
    l4 = tbl(cols=1, rows="".join("<w:tr>" + tc([f"L4-{r}"], 700) + "</w:tr>" for r in range(2)))
    l3 = tbl(cols=1, rows="".join(
        f"<w:tr><w:tc><w:tcPr><w:tcW w:w=\"1400\" w:type=\"dxa\"/></w:tcPr>{l4}</w:tc></w:tr>"
        for r in range(2)))
    l2 = tbl(cols=1, rows=(
        f"<w:tr><w:tc><w:tcPr><w:tcW w:w=\"2800\" w:type=\"dxa\"/></w:tcPr>{l3}</w:tc></w:tr>"))
    l1 = tbl(cols=2, rows=(
        f"<w:tr><w:tc><w:tcPr><w:tcW w:w=\"4500\" w:type=\"dxa\"/></w:tcPr>{l2}</w:tc>"
        + tc(["四層測試對照", "段落2", "段落3"], 4500) + "</w:tr>"))
    body = (para("四層巢狀表格", '<w:pStyle w:val="Heading1"/>') + l1)
    write_docx("patho_nested_table_4deep.docx", std_parts(body, sect()))

    # P2 跨頁長表：60 列 trHeight=340 → 估算 20400 twips > 一頁
    rows = "<w:tr><w:trPr><w:tblHeader/></w:trPr>" + tc(["項目"], 3000, vmerge="restart") + tc(["數值"], 3000) + tc(["備註"], 3000) + "</w:tr>"
    rows += "".join(
        "<w:tr><w:trPr><w:trHeight w:val=\"340\"/></w:trPr>"
        + tc([f"跨頁資料列 {r}"], 3000, vmerge="cont" if r % 5 == 0 else "")
        + tc([str(r * 17 % 97)], 3000) + tc([f"備註 {r}"], 3000) + "</w:tr>"
        for r in range(1, 61))
    longtbl = tbl(cols=3, rows=rows,
                  tblpr_extra='<w:tblLayout w:type="fixed"/><w:tblBorders><w:top w:val="single" w:sz="4"/></w:tblBorders>')
    body = (para("跨頁長表（60 列）", '<w:pStyle w:val="Heading1"/>')
            + para("本表總高度超過一頁，驗證跨頁續排與 tblHeader 重複列。")
            + longtbl + para("長表之後的段落。"))
    write_docx("patho_table_across_pages.docx", std_parts(body, sect()))

    # P2b 跨頁長表變體（無 trHeight、50 列預設高 + 多段落格加高）
    rows = "".join(
        "<w:tr>" + tc([f"欄一 {r}", f"續行 {r}"], 3000) + tc([f"欄二 {r}"], 3000) + "</w:tr>"
        for r in range(1, 51))
    body = (para("跨頁長表變體（無列高宣告）", '<w:pStyle w:val="Heading1"/>')
            + tbl(cols=2, rows=rows) + para("之後段落。"))
    write_docx("patho_table_across_pages_b.docx", std_parts(body, sect()))

    # P3 合併交錯：gridSpan×vMerge 同格重疊＋行內 span 總和≠grid 欄數（錯位病態）
    rows = (
        # 4 欄 grid；第一行 1+1+2（gridSpan 2）正常
        "<w:tr>" + tc(["交錯表頭A"], 2250, vmerge="restart") + tc(["B"], 2250)
        + tc(["C 跨兩欄", "第二", "第三"], 4500, gridspan=2) + "</w:tr>"
        # 第二行：vMerge cont＋gridSpan 3
        "<w:tr>" + tc([""], 2250, vmerge="cont") + tc(["續行", "多段一", "多段二"], 6750, gridspan=3) + "</w:tr>"
        # 第三行：span 總和 2+1＝3 ≠ grid 4（錯位）
        "<w:tr>" + tc(["錯位列", "第二"], 4500, gridspan=2) + tc(["單格"], 2250) + "</w:tr>"
        # 第四行：正常補齊
        "<w:tr>" + "".join(tc([f"D{c}"], 2250) for c in range(4)) + "</w:tr>")
    body = (para("合併交錯：gridSpan×vMerge 重疊與欄位錯位", '<w:pStyle w:val="Heading1"/>')
            + para("同一格同時帶 gridSpan 與 vMerge，且第三行 span 總和不等於 grid 欄數。")
            + tbl(cols=4, rows=rows))
    write_docx("patho_merged_interleave.docx", std_parts(body, sect()))

    # P3b 合併交錯變體：棋盤式 vMerge restart/cont × gridSpan 交錯
    rows = (
        "<w:tr>" + tc(["棋盤1", "段落2"], 3000, vmerge="restart", gridspan=2) + tc(["棋盤2"], 3000) + "</w:tr>"
        + "<w:tr>" + tc([""], 3000, vmerge="cont", gridspan=2) + tc(["棋盤3"], 3000, vmerge="restart") + "</w:tr>"
        + "<w:tr>" + tc(["棋盤4"], 3000, gridspan=2) + tc([""], 3000, vmerge="cont") + "</w:tr>")
    body = (para("棋盤式合併交錯", '<w:pStyle w:val="Heading1"/>') + tbl(cols=3, rows=rows))
    write_docx("patho_merged_interleave_b.docx", std_parts(body, sect()))

    # P4 autofit 極端欄寬：tblLayout=autofit＋單欄 9603 twips（超出 A4 正文寬）
    rows = ("<w:tr>" + tc(["極寬欄"], 9603, wtype="auto") + tc(["極窄欄"], 96, wtype="auto") + "</w:tr>"
            + "".join("<w:tr>" + tc([f"寬 {r}"], 9603, wtype="auto") + tc([f"{r}"], 96, wtype="auto") + "</w:tr>"
                      for r in range(1, 9)))
    body = (para("tblLayout=autofit 極端欄寬", '<w:pStyle w:val="Heading1"/>')
            + para("autofit 且欄寬宣告跨越三個數量級（9603 vs 96 twips）。")
            + tbl(cols=2, rows=rows, tblpr_extra='<w:tblLayout w:type="autofit"/>',
                  grid_cols='<w:gridCol w:w="9603"/><w:gridCol w:w="96"/>'))
    write_docx("patho_autofit_extreme_widths.docx", std_parts(body, sect()))

    # P4b autofit 未宣告 layout（預設 autofit）＋ w:type=auto＋極端值
    rows = ("<w:tr>" + tc(["auto 型寬欄"], 9026, wtype="auto") + tc(["auto 窄欄", "二", "三"], 113, wtype="auto") + "</w:tr>")
    body = (para("未宣告 tblLayout 的 autofit 極端欄寬", '<w:pStyle w:val="Heading1"/>')
            + tbl(cols=2, rows=rows, grid_cols='<w:gridCol w:w="9026"/><w:gridCol w:w="113"/>'))
    write_docx("patho_autofit_implicit.docx", std_parts(body, sect()))

    # P5 表格內多段落＋清單
    rows = (
        "<w:tr>" + tc(["欄位"], 3000) + tc(["說明"], 6000) + "</w:tr>"
        + "<w:tr>" + tc(["清單格"], 3000)
        + "<w:tc><w:tcPr><w:tcW w:w=\"6000\" w:type=\"dxa\"/></w:tcPr>"
        + para("段落一", '<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>')
        + para("段落二", '<w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>')
        + para("段落三（非清單）")
        + para("段落四（非清單）") + "</w:tc></w:tr>"
        + "<w:tr>" + tc(["多段無清單", "第二段", "第三段", "第四段", "第五段"], 3000)
        + tc(["第二格", "第二段", "第三段"], 6000) + "</w:tr>")
    body = (para("儲存格內多段落與清單", '<w:pStyle w:val="Heading1"/>')
            + para("單格 5 段、清單與普通段落混排。") + tbl(cols=2, rows=rows))
    write_docx("patho_table_cell_lists.docx", std_parts(body, sect(), numbering=NUMBERING_LIST))

    # P6 欄（cols=2）內表格
    inner_tbl = tbl(cols=2, rows=(
        "<w:tr>" + tc(["欄內表頭A"], 2000) + tc(["欄內表頭B"], 2000) + "</w:tr>"
        + "".join("<w:tr>" + tc([f"資料 {r}"], 2000) + tc([f"{r * 3}"], 2000) + "</w:tr>" for r in range(1, 6))))
    body = (para("雙欄版面中的表格", '<w:pStyle w:val="Heading1"/>')
            + para("第一段落在左欄。")
            + inner_tbl
            + para("表格之後繼續雙欄文字，驗證欄平衡與表內斷欄。")
            + para("第三段。"))
    sect_xml = ('<w:sectPr><w:cols w:num="2" w:space="720"/><w:pgSz w:w="11906" w:h="16838"/>'
                '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708"/></w:sectPr>')
    write_docx("patho_table_in_columns.docx", std_parts(body, sect_xml))

    # P6b 欄（cols=3）內表格＋清單
    body = (para("三欄版面中的表格", '<w:pStyle w:val="Heading1"/>')
            + para("導言。")
            + tbl(cols=1, rows="".join(
                "<w:tr>" + tc([f"三欄內表格列 {r}", "第二段"], 1500, numpr=(r % 2 == 0)) + "</w:tr>"
                for r in range(1, 8)))
            + para("結語。"))
    sect_xml = ('<w:sectPr><w:cols w:num="3" w:space="360"/><w:pgSz w:w="11906" w:h="16838"/>'
                '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708"/></w:sectPr>')
    write_docx("patho_table_in_columns_3col.docx", std_parts(body, sect_xml, numbering=NUMBERING_LIST))

    # P7 綜合病態大雜燴：巢狀＋跨頁＋交錯＋autofit 同檔
    nested_in_cell = tbl(cols=2, rows="".join(
        "<w:tr>" + tc([f"內 {r}"], 1000) + tc([f"內 {r}B"], 1000) + "</w:tr>" for r in range(2)))
    rows = "".join(
        "<w:tr><w:trPr><w:trHeight w:val=\"300\"/></w:trPr>"
        + (f"<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/><w:vMerge w:val=\"restart\"/></w:tcPr>"
           + para(f"合併錨列 {r}") + nested_in_cell + "</w:tc>" if r == 1 else
           tc([""], 3000, vmerge="cont" if r in (2, 3) else ""))
        + tc([f"欄二 {r}"], 3000)
        + tc([f"欄三 {r}", "第二段"], 3000) + "</w:tr>" for r in range(1, 46))
    body = (para("綜合病態表格", '<w:pStyle w:val="Heading1"/>')
            + para("巢狀表格＋vMerge＋autofit＋跨頁 45 列。")
            + tbl(cols=3, rows=rows, tblpr_extra='<w:tblLayout w:type="autofit"/>'))
    write_docx("patho_table_kitchen_sink.docx", std_parts(body, sect()))


if __name__ == "__main__":
    main()
    gen_pathological_tables()
