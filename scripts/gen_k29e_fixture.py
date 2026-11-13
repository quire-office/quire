#!/usr/bin/env python3
"""產生 fixtures/k29e_char_format.docx：K29e 六欄字元格式考卷 fixture。

覆蓋形態（每條對應一盞紅燈，見 core/engine-api/tests/k29e_char_format.rs）：
1. **級聯回退段（燈 7）**：字元樣式 `K29eMark` 宣告 b/u/strike/highlight＋
   段落樣式 `K29ePara` 宣告vertAlign=superscript；內容 run 零 rPr＝全部格式
   來自樣式級聯。編輯把直接格式改寫成明確移除（u=none/strike=0/
   highlight=none/vertAlign=baseline）後，b 仍須從樣式級聯顯形（級聯鏈
   未被編輯破壞），而 u/strike/highlight/vertAlign 被直接層的顯式關閉熄滅。
2. **無 rPr run 段（燈 1）**：六欄 patch 對零宣告 run 的「設定→存檔→重解析
   →讀回」閉環。
3. **highlight/shd 雙路徑（燈 5）**：命名色走 w:highlight、非命名色走 w:shd
   底紋，兩者在解析層同落 highlight 欄。
4. **docDefaults 基準（燈 6）**：docDefaults 宣告粗體＋底線，級聯回退段不
   受直接層移除影響的對照段。

zip 一律 STORE：zip_struct 雙方法皆支援，fixture 位元組穩定可重現。
"""
import pathlib
import sys
import zipfile

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"""

ROOT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""

DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"""

# docDefaults 11pt＋粗體＋單條底線（燈 6 級聯基準：未被直接層移除的段
# 必須仍帶 b/u）。字元樣式 K29eMark 宣告四欄直接可窺的級聯格式；段落樣式
# K29ePara 疊 superscript（燈 7 驗 vertAlign 的級聯回退口徑）。
STYLES = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles {W}>
<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/><w:b/><w:u w:val="single"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"/>
<w:style w:type="character" w:styleId="K29eMark"><w:rPr><w:strike/><w:highlight w:val="yellow"/><w:u w:val="dotted"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="K29ePara"><w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>
</w:styles>"""

BODY = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document {W}><w:body>
<w:p><w:r><w:t>零宣告基準段</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="K29ePara"/></w:pPr><w:r><w:rPr><w:rStyle w:val="K29eMark"/></w:rPr><w:t>級聯回退段ABCDEFGHIJ</w:t></w:r></w:p>
<w:p><w:r><w:t>六欄目標段0123456789</w:t></w:r></w:p>
<w:p><w:r><w:t>版面填充段：把版面撐出可預測的兩行斷點，確保各段落在確定位置上。</w:t></w:r></w:p>
<w:p><w:r><w:t>版面填充乙段確保第一頁行群數量穩定不被編輯位移。</w:t></w:r></w:p>
</w:body></w:document>"""


def main(out: pathlib.Path):
    with zipfile.ZipFile(out, "w", zipfile.ZIP_STORED) as z:
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        z.writestr("_rels/.rels", ROOT_RELS)
        z.writestr("word/document.xml", BODY)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/styles.xml", STYLES)
    print(f"寫入 {out}（{out.stat().st_size} bytes）")


if __name__ == "__main__":
    fixtures = pathlib.Path(__file__).resolve().parent.parent / "fixtures"
    main(pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else fixtures / "k29e_char_format.docx")
