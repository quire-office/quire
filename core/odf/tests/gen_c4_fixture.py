#!/usr/bin/env python3
"""ODF-C4 合成 fixture 產生器（core/odf/tests/fixtures/，隨考卷入庫）。

產兩檔（最小合法 ODT：mimetype 首條目＋STORE，ODF 1.3 Part 2 §3.3）：

1. c4_dual_axis.odt —— styles.xml 的 page-layout-properties 同時宣告
   style: 軸（style:paper-width/height，卡面口徑的「新軸」候選）與
   fo: 軸（fo:page-width/height），兩軸故意差 1cm，考燈 2 取 style: 軸優先。
   實查：ODF 1.3 os／1.4 schema 的 page-layout-properties 屬性表皆只列
   fo:page-width/height（無 style: 軸欄位），全語料 0 檔使用 style: 軸——本
   檔是純合成探針，不代表 LO 寫檔實態（解剖報告如實記帳）。
2. c4_no_page_layout.odt —— styles.xml 有 <office:master-styles>＋
   <style:master-page style:name="Standard"> 但 **page-layout-name 指向不
   存在的 page-layout** 且 <office:automatic-styles> 完全無
   <style:page-layout>：考燈 3 的「無 page-layout 檔回退 LO 預設頁」基準。
   LO 真值＝同檔 --convert-to pdf 實測（報告鎖值，非憑印象）。

用法：python3 gen_c4_fixture.py [--outdir DIR]
"""
import argparse, pathlib, zipfile

MIMETYPE = "application/vnd.oasis.opendocument.text"

MANIFEST = """<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
 <manifest:file-entry manifest:full-path="/" manifest:version="1.3" manifest:media-type="application/vnd.oasis.opendocument.text"/>
 <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
 <manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
</manifest:manifest>
"""

NS = (
    'xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
    'xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" '
    'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
    'xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"'
)

CONTENT = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {NS}>
 <office:body>
  <office:text>
   <text:p>C4 合成探針段落。</text:p>
  </office:text>
 </office:body>
</office:document-content>
"""

# 燈 2：雙軸分歧——style: 軸 21.00cm×29.00cm vs fo: 軸 20.00cm×28.00cm（差 1cm）。
# 邊距只寫 fo:（語料實態：全語料 70/70 舊式 fo:margin，0 檔 <style:page-margin>）。
STYLES_DUAL = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {NS}>
 <office:styles/>
 <office:automatic-styles>
  <style:page-layout style:name="Mpm1">
   <style:page-layout-properties style:paper-width="21.00cm" style:paper-height="29.00cm" fo:page-width="20.00cm" fo:page-height="28.00cm" style:print-orientation="portrait" fo:margin-top="3.00cm" fo:margin-bottom="3.00cm" fo:margin-left="3.50cm" fo:margin-right="3.50cm"/>
  </style:page-layout>
 </office:automatic-styles>
 <office:master-styles>
  <style:master-page style:name="Standard" style:page-layout-name="Mpm1"/>
 </office:master-styles>
</office:document-styles>
"""

# 燈 3：完全無 page-layout（master-page 引用不存在的版面名）。
STYLES_NOPL = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {NS}>
 <office:styles/>
 <office:automatic-styles/>
 <office:master-styles>
  <style:master-page style:name="Standard" style:page-layout-name="NoSuchLayout"/>
 </office:master-styles>
</office:document-styles>
"""


def build(path: pathlib.Path, styles: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w") as z:
        zi = zipfile.ZipInfo("mimetype")
        zi.compress_type = zipfile.ZIP_STORED
        z.writestr(zi, MIMETYPE)
        z.writestr("META-INF/manifest.xml", MANIFEST)
        z.writestr("styles.xml", styles)
        z.writestr("content.xml", CONTENT)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--outdir", default=str(pathlib.Path(__file__).parent / "fixtures"))
    a = ap.parse_args()
    out = pathlib.Path(a.outdir)
    build(out / "c4_dual_axis.odt", STYLES_DUAL)
    build(out / "c4_no_page_layout.odt", STYLES_NOPL)
    for p in (out / "c4_dual_axis.odt", out / "c4_no_page_layout.odt"):
        print("wrote", p, p.stat().st_size, "bytes")


if __name__ == "__main__":
    main()
