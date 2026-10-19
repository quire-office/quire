#!/usr/bin/env python3
"""產生 fixtures/style_cascade.docx：樣式級聯反修驗證 fixture（t_5b391cfd）。

覆蓋形態（每條對應一個引擎層斷言）：
1. **pPr/rPr 自閉合 `<w:b/>`（S1）**：段首「段落級自閉合粗體」——其內容 run
   **完全不帶 rPr**（級聯只可能從段落 prun 取得粗體；舊解析靜默丟棄自閉合
   標籤 → display list flags 恆 0，修後 FLAG_BOLD 必須出現）。
2. **量化佔帶（測試缺口 (i)）**：run 級 `<w:sz w:val="48"/>`＝24pt 大字段，
   段行高 24×1.4＝33.6pt > 帶尺 16.8pt → bands＝2。斷言頁數＋行 y（band×行高）。
3. **leader×br-page（S3 案 a）**：末段同時宣告 `w:tabs/w:tab leader="dot"`
   ＋ run 內 `w:br w:type="page"` ＋ `w:tab`——page_breaks 非空，填充**不得**
   展開（段文字須與原始 run 串接逐字元等長，無點引字元）。
4. **編輯後發射同源（S4）**：pPr/rPr 自閉合 b 段供「編輯帶粗體段 → 新文字
   仍帶 FLAG_BOLD」斷言（paras_eff 不同步時舊 run 區間錯映、flags 歸 0）。

另含直接格式段（run rPr 粗體＋色）作發射樣式對照。zip 一律 STORE：
zip_struct 雙方法皆支援，fixture 位元組穩定可重現。
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
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"""

# docDefaults 12pt（帶尺＝12×1.4＝16.8pt，與全域後備同尺——零宣告段 bands=1）＋
# 顯式 Normal（default=1 段落樣式，無額外宣告＝不干擾級聯）。
STYLES = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles {W}>
<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"/>
<w:style w:type="paragraph" w:styleId="Heading1"><w:rPr><w:sz w:val="48"/></w:rPr></w:style>
</w:styles>"""

BODY = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document {W}><w:body>
<w:p><w:pPr><w:rPr><w:b/></w:rPr></w:pPr><w:r><w:t>段落級自閉合粗體</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:b/><w:color w:val="00FF00"/></w:rPr><w:t>直接格式粗體綠字</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>大字標題二十六磅</w:t></w:r></w:p>
{''.join(f'<w:p><w:r><w:t>級聯填充第{i}段：這一行的存在是為了把版面擠過第一頁，讓大字段落在可預測的頁與帶上落地。</w:t></w:r></w:p>' for i in range(40))}
<w:p><w:pPr><w:tabs><w:tab w:val="right" w:leader="dot" w:pos="8306"/></w:tabs></w:pPr><w:r><w:t>目錄條目前綴</w:t></w:r><w:r><w:tab/><w:t>12</w:t></w:r></w:p>
<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>強制換頁段從新頁起頭</w:t></w:r></w:p>
<w:p><w:pPr><w:tabs><w:tab w:val="right" w:leader="dot" w:pos="8306"/></w:tabs></w:pPr><w:r><w:t>分頁段前綴</w:t></w:r><w:r><w:tab/><w:t>頁碼</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p>
<w:p><w:r><w:t>分頁後的尾段</w:t></w:r></w:p>
<w:sectPr>
<w:pgSz w:w="11906" w:h="16838"/>
<w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800" w:header="720" w:footer="720" w:gutter="0"/>
</w:sectPr>
</w:body></w:document>"""

# 病態數值 fixture（t_5b391cfd S2）：畸形超大 w:sz／w:spacing——styles.rs 守衛
# 拒收（inf／逾 OOXML 整數值域），layout 端 saturating 柵欄兜底。斷言＝開啟
# 不 panic、頁數確定（兩次開啟一致）、全頁顯示列表可取、重排有限步定案。
PATHOLOGICAL = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document {W}><w:body>
<w:p><w:pPr><w:spacing w:line="999999999" w:lineRule="auto"/></w:pPr><w:r><w:t>超大行距段</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:sz w:val="4000000000000"/></w:rPr><w:t>超大字級段</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:sz w:val="1e400"/></w:rPr><w:t>無限字級段</w:t></w:r></w:p>
<w:p><w:r><w:t>病態檔基準段</w:t></w:r></w:p>
{''.join(f'<w:p><w:r><w:t>病態填充第{i}段確保分頁狀態機走完全部頁面而不繞回。</w:t></w:r></w:p>' for i in range(30))}
<w:sectPr>
<w:pgSz w:w="11906" w:h="16838"/>
<w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800" w:header="720" w:footer="720" w:gutter="0"/>
</w:sectPr>
</w:body></w:document>"""


def main(out: pathlib.Path):
    with zipfile.ZipFile(out, "w", zipfile.ZIP_STORED) as z:
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        z.writestr("_rels/.rels", ROOT_RELS)
        z.writestr("word/document.xml", BODY)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/styles.xml", STYLES)
    print(f"寫入 {out}（{out.stat().st_size} bytes）")
    path_out = out.parent / "style_pathological.docx"
    with zipfile.ZipFile(path_out, "w", zipfile.ZIP_STORED) as z:
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        z.writestr("_rels/.rels", ROOT_RELS)
        z.writestr("word/document.xml", PATHOLOGICAL)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/styles.xml", STYLES)
    print(f"寫入 {path_out}（{path_out.stat().st_size} bytes）")


if __name__ == "__main__":
    fixtures = pathlib.Path(__file__).resolve().parent.parent / "fixtures"
    main(pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else fixtures / "style_cascade.docx")
