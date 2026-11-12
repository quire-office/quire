#!/usr/bin/env python3
"""ODF 表格排版合成測試語料產生器。

產製兩份合成 ODT 測試檔案至 core/odf/tests/fixtures/：
1. table_flow_3x5.odt：3 欄×5 列表格，含水平跨欄（number-columns-spanned="2"）與 covered-table-cell、底色與邊框，表前表後各有一正文段落。
2. table_flow_header_split.odt：跨頁表格，含 table:table-header-rows 表頭重複宣告，驗證 LO 25.8 跨頁切片與表頭重複真值。
"""

import argparse
import pathlib
import zipfile

MIMETYPE = "application/vnd.oasis.opendocument.text"

MANIFEST = """<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
 <manifest:file-entry manifest:full-path="/" manifest:version="1.3" manifest:media-type="application/vnd.oasis.opendocument.text"/>
 <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
 <manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
</manifest:manifest>
"""

NS_ATTRS = (
    'xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
    'xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" '
    'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
    'xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" '
    'xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"'
)

def build_odt(path: pathlib.Path, content_xml: str, styles_xml: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w") as z:
        zi = zipfile.ZipInfo("mimetype")
        zi.compress_type = zipfile.ZIP_STORED
        z.writestr(zi, MIMETYPE)
        z.writestr("META-INF/manifest.xml", MANIFEST)
        z.writestr("styles.xml", styles_xml)
        z.writestr("content.xml", content_xml)

def generate_table_3x5(out_dir: pathlib.Path) -> None:
    styles_xml = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {NS_ATTRS}>
 <office:styles/>
 <office:automatic-styles>
  <style:page-layout style:name="PL1">
   <style:page-layout-properties fo:page-width="21.0cm" fo:page-height="29.7cm" fo:margin-top="2.0cm" fo:margin-bottom="2.0cm" fo:margin-left="2.0cm" fo:margin-right="2.0cm"/>
  </style:page-layout>
 </office:automatic-styles>
 <office:master-styles>
  <style:master-page style:name="Standard" style:page-layout-name="PL1"/>
 </office:master-styles>
</office:document-styles>
"""

    content_xml = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {NS_ATTRS}>
 <office:automatic-styles>
  <style:style style:name="Tbl1" style:family="table">
   <style:table-properties style:width="12.0cm" fo:margin-left="1.0cm" table:align="left"/>
  </style:style>
  <style:style style:name="ColA" style:family="table-column">
   <style:table-column-properties style:column-width="3.0cm"/>
  </style:style>
  <style:style style:name="ColB" style:family="table-column">
   <style:table-column-properties style:column-width="4.0cm"/>
  </style:style>
  <style:style style:name="ColC" style:family="table-column">
   <style:table-column-properties style:column-width="5.0cm"/>
  </style:style>
  <style:style style:name="RowStyle" style:family="table-row">
   <style:table-row-properties style:min-row-height="0.8cm"/>
  </style:style>
  <style:style style:name="CellDefault" style:family="table-cell">
   <style:table-cell-properties fo:padding="0.15cm" fo:border="0.5pt solid #000000"/>
  </style:style>
  <style:style style:name="CellShaded" style:family="table-cell">
   <style:table-cell-properties fo:background-color="#d9e1f2" fo:padding="0.15cm" fo:border="0.5pt solid #000000"/>
  </style:style>
 </office:automatic-styles>
 <office:body>
  <office:text>
   <text:p>前置正文段落</text:p>
   <table:table table:name="Table1" table:style-name="Tbl1">
    <table:table-column table:style-name="ColA"/>
    <table:table-column table:style-name="ColB"/>
    <table:table-column table:style-name="ColC"/>
    <table:table-row table:style-name="RowStyle">
     <table:table-cell table:style-name="CellShaded" table:number-columns-spanned="2">
      <text:p>首列跨兩欄格</text:p>
     </table:table-cell>
     <table:covered-table-cell/>
     <table:table-cell table:style-name="CellDefault">
      <text:p>首列第三格</text:p>
     </table:table-cell>
    </table:table-row>
    <table:table-row table:style-name="RowStyle">
     <table:table-cell table:style-name="CellDefault"><text:p>次列第一格</text:p></table:table-cell>
     <table:table-cell table:style-name="CellDefault"><text:p>次列第二格</text:p></table:table-cell>
     <table:table-cell table:style-name="CellDefault"><text:p>次列第三格</text:p></table:table-cell>
    </table:table-row>
    <table:table-row table:style-name="RowStyle">
     <table:table-cell table:style-name="CellDefault"><text:p>三列第一格</text:p></table:table-cell>
     <table:table-cell table:style-name="CellDefault"><text:p>三列第二格</text:p></table:table-cell>
     <table:table-cell table:style-name="CellDefault"><text:p>三列第三格</text:p></table:table-cell>
    </table:table-row>
    <table:table-row table:style-name="RowStyle">
     <table:table-cell table:style-name="CellDefault"><text:p>四列第一格</text:p></table:table-cell>
     <table:table-cell table:style-name="CellDefault"><text:p>四列第二格</text:p></table:table-cell>
     <table:table-cell table:style-name="CellDefault"><text:p>四列第三格</text:p></table:table-cell>
    </table:table-row>
    <table:table-row table:style-name="RowStyle">
     <table:table-cell table:style-name="CellDefault"><text:p>五列第一格</text:p></table:table-cell>
     <table:table-cell table:style-name="CellDefault"><text:p>五列第二格</text:p></table:table-cell>
     <table:table-cell table:style-name="CellDefault"><text:p>五列第三格</text:p></table:table-cell>
    </table:table-row>
   </table:table>
   <text:p>後置正文段落</text:p>
  </office:text>
 </office:body>
</office:document-content>
"""
    build_odt(out_dir / "table_flow_3x5.odt", content_xml, styles_xml)

def generate_table_header_split(out_dir: pathlib.Path) -> None:
    # 頁高 14.0cm，上下邊距各 2.0cm，版心高 10.0cm。每列高 1.0cm，1 表頭列＋10 內容列＝11 列，保證跨為 2 頁且與 LO 25.8 真值收斂。
    styles_xml = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {NS_ATTRS}>
 <office:styles/>
 <office:automatic-styles>
  <style:page-layout style:name="PL_Split">
   <style:page-layout-properties fo:page-width="21.0cm" fo:page-height="14.0cm" fo:margin-top="2.0cm" fo:margin-bottom="2.0cm" fo:margin-left="2.0cm" fo:margin-right="2.0cm"/>
  </style:page-layout>
 </office:automatic-styles>
 <office:master-styles>
  <style:master-page style:name="Standard" style:page-layout-name="PL_Split"/>
 </office:master-styles>
</office:document-styles>
"""

    rows_xml = []
    for i in range(10):
        rows_xml.append(f"""    <table:table-row table:style-name="RowH1">
     <table:table-cell table:style-name="CellDefault"><text:p>資料行{i:02d}甲</text:p></table:table-cell>
     <table:table-cell table:style-name="CellDefault"><text:p>資料行{i:02d}乙</text:p></table:table-cell>
     <table:table-cell table:style-name="CellDefault"><text:p>資料行{i:02d}丙</text:p></table:table-cell>
    </table:table-row>""")
    content_rows = "\n".join(rows_xml)

    content_xml = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {NS_ATTRS}>
 <office:automatic-styles>
  <style:style style:name="TblSplit" style:family="table">
   <style:table-properties style:width="15.0cm" fo:margin-left="0.0cm" table:align="left"/>
  </style:style>
  <style:style style:name="ColSplit" style:family="table-column">
   <style:table-column-properties style:column-width="5.0cm"/>
  </style:style>
  <style:style style:name="RowH1" style:family="table-row">
   <style:table-row-properties style:min-row-height="1.0cm"/>
  </style:style>
  <style:style style:name="CellDefault" style:family="table-cell">
   <style:table-cell-properties fo:padding="0.10cm" fo:border="0.5pt solid #000000"/>
  </style:style>
  <style:style style:name="CellHeader" style:family="table-cell">
   <style:table-cell-properties fo:background-color="#c6d9f1" fo:padding="0.10cm" fo:border="1.0pt solid #000000"/>
  </style:style>
 </office:automatic-styles>
 <office:body>
  <office:text>
   <table:table table:name="TableSplit" table:style-name="TblSplit">
    <table:table-column table:style-name="ColSplit" table:number-columns-repeated="3"/>
    <table:table-header-rows>
     <table:table-row table:style-name="RowH1">
      <table:table-cell table:style-name="CellHeader"><text:p>表頭甲</text:p></table:table-cell>
      <table:table-cell table:style-name="CellHeader"><text:p>表頭乙</text:p></table:table-cell>
      <table:table-cell table:style-name="CellHeader"><text:p>表頭丙</text:p></table:table-cell>
     </table:table-row>
    </table:table-header-rows>
{content_rows}
   </table:table>
  </office:text>
 </office:body>
</office:document-content>
"""
    build_odt(out_dir / "table_flow_header_split.odt", content_xml, styles_xml)

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--outdir", default=str(pathlib.Path(__file__).parent / "fixtures"))
    args = ap.parse_args()
    out_dir = pathlib.Path(args.outdir)
    generate_table_3x5(out_dir)
    generate_table_header_split(out_dir)
    print(f"Generated fixtures in {out_dir}:")
    for f in ["table_flow_3x5.odt", "table_flow_header_split.odt"]:
        p = out_dir / f
        print(f"  {f}: {p.stat().st_size} bytes")

if __name__ == "__main__":
    main()
