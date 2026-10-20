#!/usr/bin/env python3
"""產生 fixtures/xlsx_minimal.xlsx 與 fixtures/xlsx_macro.xlsm（X1 卡 round-trip fixtures）。

結構（最小但完整，全部 ZIP_STORED——zip_struct 雙方法皆支援，fixture 位元組
穩定可重現，byte-identical 斷言才有硬基準）：

xlsx_minimal.xlsx：
- [Content_Types].xml（Default rels/xml＋Override workbook/styles/sharedStrings/sheets）
- _rels/.rels（rId1 → xl/workbook.xml）
- xl/workbook.xml（2 sheets：Visible＋hidden；1 個 definedName）
- xl/_rels/workbook.xml.rels（rIdS1/S2 → worksheets/sheet1/2.xml；rIdSt/rIdSst）
- xl/worksheets/sheet1.xml（dimension B1:D9＋9 個 row；sheet2 空 sheetData）
- xl/styles.xml、xl/sharedStrings.xml（最小可辨識 part）

xlsx_macro.xlsm：
- macroEnabled 型別的 [Content_Types].xml（Default Extension="bin" ＋
  Override vbaProject ContentType）
- xl/vbaProject.bin（不透明二進位 part——round-trip 語義測試的核心：
  macro part 原樣搬運，我們從不解碼它）
- sheets 含 state="veryHidden"（LO tdf111974 形態）。
"""
import pathlib
import zipfile
import sys

CT = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
<Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"""

CT_MACRO = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="bin" ContentType="application/vnd.ms-office.vbaProject"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.ms-excel.sheet.macroEnabled.main+xml"/>
<Override PartName="/xl/vbaProject.bin" ContentType="application/vnd.ms-office.vbaProject"/>
<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
<Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"""

ROOT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"""

WB = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets>
<sheet name="資料" sheetId="1" state="visible" r:id="rIdS1"/>
<sheet name="隱藏頁" sheetId="2" state="hidden" r:id="rIdS2"/>
</sheets>
<definedNames><definedName name="_xlnm.Print_Area" localSheetId="0">資料!$B$2:$D$9</definedName></definedNames>
<calcPr calcId="124519"/>
</workbook>"""

WB_MACRO = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets>
<sheet name="OP PLAN" sheetId="1" r:id="rIdS1"/>
<sheet name="MACROONLY" sheetId="2" state="veryHidden" r:id=""/>
<sheet name="Roll Weeks" sheetId="5" state="hidden" r:id="rIdS2"/>
</sheets>
<definedNames>
<definedName name="Plant1">OP PLAN!$A$1</definedName>
<definedName name="_xlnm.Print_Area" localSheetId="0">OP PLAN!$A$1:$D$9</definedName>
</definedNames>
<calcPr calcId="124519"/>
</workbook>"""

WB_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdS1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rIdS2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>
<Relationship Id="rIdSt" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rIdSst" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
</Relationships>"""

SHEET1 = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<dimension ref="B1:D9"/>
<sheetData>
<row r="2"><c r="B2" t="s"><v>0</v></c><c r="C2"><v>10</v></c></row>
<row r="3"><c r="B3" t="s"><v>1</v></c><c r="C3"><v>20</v></c></row>
<row r="4"/>
<row r="5"/>
<row r="6"/>
<row r="7"/>
<row r="8"/>
<row r="9"/>
<row r="10"><c r="D10"><f>SUM(C2:C3)</f><v>30</v></c></row>
</sheetData>
</worksheet>"""

SHEET2 = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<dimension ref="A1"/>
<sheetData/>
</worksheet>"""

STYLES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>
</styleSheet>"""

SST = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="2" uniqueCount="2">
<si><t>名稱</t></si><si><t>金額</t></si>
</sst>"""

# 不透明 macro part：內容語義我們從不解析，只原樣搬運——用一段固定位元組
# 當代表（含 OLE 頭魔數，形態真實；byte-identical 斷言不依賴其內容）。
VBA = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1" + bytes(range(256)) * 4


def write(out: pathlib.Path, parts: list[tuple[str, bytes]]):
    with zipfile.ZipFile(out, "w", zipfile.ZIP_STORED) as z:
        for name, data in parts:
            # 固定 timestamp＋STORED：重新生成產生逐位元組相同的檔案，
            # fixtures 進 git 後 byte-identical 斷言的基準才穩定。
            zi = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            zi.compress_type = zipfile.ZIP_STORED
            z.writestr(zi, data)
    print(f"寫入 {out}（{out.stat().st_size} bytes）")


def main(fixtures: pathlib.Path):
    write(
        fixtures / "xlsx_minimal.xlsx",
        [
            ("[Content_Types].xml", CT.encode()),
            ("_rels/.rels", ROOT_RELS.encode()),
            ("xl/workbook.xml", WB.encode()),
            ("xl/_rels/workbook.xml.rels", WB_RELS.encode()),
            ("xl/worksheets/sheet1.xml", SHEET1.encode()),
            ("xl/worksheets/sheet2.xml", SHEET2.encode()),
            ("xl/styles.xml", STYLES.encode()),
            ("xl/sharedStrings.xml", SST.encode()),
        ],
    )
    write(
        fixtures / "xlsx_macro.xlsm",
        [
            ("[Content_Types].xml", CT_MACRO.encode()),
            ("_rels/.rels", ROOT_RELS.encode()),
            ("xl/workbook.xml", WB_MACRO.encode()),
            ("xl/_rels/workbook.xml.rels", WB_RELS.encode()),
            ("xl/worksheets/sheet1.xml", SHEET1.encode()),
            ("xl/worksheets/sheet2.xml", SHEET2.encode()),
            ("xl/styles.xml", STYLES.encode()),
            ("xl/sharedStrings.xml", SST.encode()),
            ("xl/vbaProject.bin", VBA),
            ("xl/custom/unknown-part.xml", b"<custom xmlns=\"urn:custom\"><thing/></custom>"),
        ],
    )


if __name__ == "__main__":
    root = pathlib.Path(__file__).resolve().parent.parent
    out_dir = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else root / "fixtures"
    out_dir.mkdir(parents=True, exist_ok=True)
    main(out_dir)
