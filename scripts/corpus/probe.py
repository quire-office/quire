#!/usr/bin/env python3
"""quire corpus 採集器 —— 掃描 pool/、驗證 zip/docx、抽取特徵標籤、去重、挑選、輸出 corpus/ + manifest.json。

純 stdlib：zipfile + re + json。特徵 = 直接扫 OOXML part 的 XML 標記（不依賴第三方庫）。
"""
from __future__ import annotations

import concurrent.futures as cf
import hashlib
import json
import pathlib
import re
import sys
import zipfile

POOL = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path("pool")
OUT = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else pathlib.Path("corpus")

MAIN_NS = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"

# 特徵標籤定義：(tag, 說明)；偵測邏輯在 probe()
FEATURE_TAGS = [
    ("table", "表格 w:tbl"),
    ("merged_cells", "合併儲存格 gridSpan/vMerge"),
    ("header_footer", "頁首頁尾 header/footer part"),
    ("footnote", "註腳 footnotes.xml"),
    ("endnote", "尾註 endnotes.xml"),
    ("image", "內嵌圖片 media/*"),
    ("tracked_revisions", "修訂追蹤 w:ins/w:del/w:rPrChange"),
    ("comments", "批註 comments.xml"),
    ("numbering", "清單編號 numbering.xml"),
    ("styles_defined", "樣式表 styles.xml"),
    ("theme", "主題 theme*.xml"),
    ("sections", "分節 w:sectPr"),
    ("columns", "多欄 w:cols"),
    ("hyperlink", "超連結 w:hyperlink"),
    ("fields", "欄位 fldSimple/fldChar"),
    ("textbox", "文字框 textbox/txbxContent"),
    ("equation", "數學 m:oMath"),
    ("chart", "圖表 chart parts"),
    ("embedded_object", "嵌入式 OLE object/embedded*"),
    ("content_control", "內容控件 w:sdt"),
    ("drawing", "圖形繪製 w:drawing"),
    ("legacy_vml", "舊式 VML 圖形"),
    ("smartart", "SmartArt diagram"),
    ("bookmark", "書籤 w:bookmarkStart"),
    ("symbol", "符號 w:sym"),
    ("rtl", "RTL 文字 w:bidi"),
    ("east_asian", "CJK 文字（含中日韓碼位）"),
    ("superscript_subscript", "上標/下標 vertAlign"),
    ("strike", "刪除線 w:strike"),
    ("highlight", "螢光標 w:highlight"),
    ("paragraph_border_shading", "段落框線/底紋"),
    ("dropcap", "首字大寫"),
    ("doc_properties", "文件屬性 core/app"),
    ("custom_xml", "自訂 XML parts"),
    ("altchunk", "altChunk 外部內容"),
    ("glossary", "快速部件 glossary (dotx)"),
    ("fonts_embedded", "內嵌字型 fontTable+ftr*"),
    ("websettings", "webSettings part"),
]

CJK_RE = re.compile(r"[㐀-䶿一-鿿豈-﫿぀-ヿ가-힯]")


def probe(path: pathlib.Path) -> dict | None:
    """回傳 {sha, size, parts, tags, nparts}；非法 docx 回 None。"""
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if not data.startswith(b"PK"):
        return None
    try:
        zf = zipfile.ZipFile(pathlib.BytesIO(data) if hasattr(pathlib, "BytesIO") else __import__("io").BytesIO(data))
    except Exception:
        return None
    try:
        names = zf.namelist()
    except Exception:
        return None
    name_set = set(names)
    has_main = "word/document.xml" in name_set
    if not has_main:
        return None
    tags: set[str] = set()
    try:
        doc = zf.read("word/document.xml")
    except Exception:
        return None
    low = doc

    def indoc(b: bytes) -> bool:
        return b in low

    if indoc(b"<w:tbl>"):
        tags.add("table")
    if indoc(b"gridSpan") or indoc(b"vMerge"):
        tags.add("merged_cells")
    if any(n.startswith("word/header") for n in names):
        tags.add("header_footer")
    if "word/footnotes.xml" in name_set and b"<w:footnote " not in (zf.read("word/footnotes.xml") if "word/footnotes.xml" in name_set else b""):
        pass
    if "word/footnotes.xml" in name_set:
        try:
            if re.search(rb"<w:footnote w:id=\"[1-9]", zf.read("word/footnotes.xml")):
                tags.add("footnote")
        except Exception:
            pass
    if "word/endnotes.xml" in name_set:
        try:
            if re.search(rb"<w:endnote w:id=\"[1-9]", zf.read("word/endnotes.xml")):
                tags.add("endnote")
        except Exception:
            pass
    if any(n.startswith("word/media/") for n in names):
        tags.add("image")
    if indoc(b"<w:ins ") or indoc(b"<w:del ") or indoc(b"<w:rPrChange") or indoc(b"<w:pPrChange") or indoc(b"<w:trPrChange") or indoc(b"<w:tcPrChange"):
        tags.add("tracked_revisions")
    if "word/comments.xml" in name_set:
        tags.add("comments")
    if "word/numbering.xml" in name_set and (indoc(b"<w:numPr>") or indoc(b"<w:numPr ")):
        tags.add("numbering")
    if "word/styles.xml" in name_set:
        tags.add("styles_defined")
    if any(re.search(r"word/theme/theme\d*\.xml", n) for n in names):
        tags.add("theme")
    if indoc(b"<w:sectPr") or indoc(b"<w:sectPr "):
        tags.add("sections")
    if re.search(rb"<w:cols [^>]*w:num=\"[2-9]", low) or b"<w:cols/>" not in low and re.search(rb"<w:cols w:", low):
        try:
            m = re.search(rb"<w:cols\b[^>]*w:num=\"(\d+)\"", low)
            if m and int(m.group(1)) >= 2:
                tags.add("columns")
        except Exception:
            pass
    if indoc(b"<w:hyperlink") or indoc(b"<w:hyperlink "):
        tags.add("hyperlink")
    if indoc(b"fldSimple") or indoc(b'w:fldChar') or indoc(b"<w:instrText"):
        tags.add("fields")
    if indoc(b"txbxContent") or indoc(b"<w:txbxContent>") or indoc(b"textbox"):
        tags.add("textbox")
    if indoc(b"<m:oMath") or indoc(b"<m:oMathPara"):
        tags.add("equation")
    if any("chart" in n and n.endswith(".xml") for n in names):
        tags.add("chart")
    if any(n.startswith("word/embeddings/") for n in names) or indoc(b"<o:OLEObject") or indoc(b"<w:object"):
        tags.add("embedded_object")
    if indoc(b"<w:sdt>") or indoc(b"<w:sdt ") or indoc(b"<w:sdtPr"):
        tags.add("content_control")
    if indoc(b"<w:drawing>") or indoc(b"<w:drawing "):
        tags.add("drawing")
    vml_bytes = bytearray(doc)
    for hn in names:
        if re.match(r"word/(header|footer)\d*\.xml$", hn):
            try:
                vml_bytes += zf.read(hn)
            except Exception:
                pass
    if b"<v:shape" in vml_bytes or b"<v:rect" in vml_bytes or b"<v:textbox" in vml_bytes or b"<v:group" in vml_bytes or b"<v:imagedata" in vml_bytes:
        tags.add("legacy_vml")
    if any("diagram" in n for n in names):
        tags.add("smartart")
    if indoc(b"<w:bookmarkStart") or indoc(b"<w:bookmarkStart "):
        tags.add("bookmark")
    if indoc(b"<w:sym "):
        tags.add("symbol")
    if indoc(b"<w:bidi") or indoc(b'w:bidi="1"') or indoc(b'w:bidi="true"'):
        tags.add("rtl")
    try:
        if CJK_RE.search(doc.decode("utf-8", "ignore")):
            tags.add("east_asian")
    except Exception:
        pass
    if indoc(b"superscript") or indoc(b"subscript"):
        tags.add("superscript_subscript")
    if indoc(b"<w:strike") or indoc(b'<w:strike/>'):
        tags.add("strike")
    if indoc(b"<w:highlight"):
        tags.add("highlight")
    if indoc(b"<w:pBdr") or indoc(b"<w:shd"):
        tags.add("paragraph_border_shading")
    if indoc(b"dropCap"):
        tags.add("dropcap")
    if "docProps/core.xml" in name_set or "docProps/app.xml" in name_set:
        tags.add("doc_properties")
    if any(n.startswith("customXml/") for n in names):
        tags.add("custom_xml")
    if indoc(b"<w:altChunk"):
        tags.add("altchunk")
    if any("glossary" in n for n in names):
        tags.add("glossary")
    if any(re.search(r"word/fonts/|ftr\d+\.odttf", n) for n in names) or indoc(b"<w:embedRegular"):
        tags.add("fonts_embedded")
    if "word/webSettings.xml" in name_set:
        tags.add("websettings")
    return {
        "sha256": hashlib.sha256(data).hexdigest(),
        "size": len(data),
        "nparts": len(names),
        "tags": sorted(tags),
    }


def main():
    files = sorted(p for p in POOL.glob("*.docx"))
    print(f"pool files: {len(files)}")
    results: dict[str, dict] = {}
    with cf.ProcessPoolExecutor(max_workers=8) as ex:
        futs = {ex.submit(probe, p): p for p in files}
        for fut in cf.as_completed(futs):
            p = futs[fut]
            try:
                r = fut.result()
            except Exception:
                r = None
            if r:
                results[p.name] = r
    # dedupe by sha
    by_sha: dict[str, str] = {}
    dupes = 0
    for name, r in sorted(results.items()):
        if r["sha256"] in by_sha:
            dupes += 1
            r["dup_of"] = by_sha[r["sha256"]]
        else:
            by_sha[r["sha256"]] = name
    print(f"valid docx: {len(results)} dupes: {dupes}")
    (POOL.parent / "probe_results.json").write_text(json.dumps(results, ensure_ascii=False))
    # coverage over unique files
    from collections import Counter
    cov = Counter()
    uniq = 0
    for name, r in results.items():
        if "dup_of" in r:
            continue
        uniq += 1
        for t in r["tags"]:
            cov[t] += 1
    print(f"unique: {uniq}")
    for tag, desc in FEATURE_TAGS:
        print(f"{tag:28s} {cov.get(tag, 0):5d}  {desc}")


if __name__ == "__main__":
    main()
