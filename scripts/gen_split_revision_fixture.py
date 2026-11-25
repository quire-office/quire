#!/usr/bin/env python3
"""產生 fixtures/split_revision_containers.docx：SplitParagraph 跨切點容器考卷 fixture。

對應考卷 core/engine-api/tests/e_split_revision_containers.rs（返修卡
t_e93fb47b）。四種形態各覆蓋一類「切點落在段內行內容器包裹的 run 內部」
的 ill-formed 形態（缺陷實錄見 tests/save-roundtrip-baseline/report.md
「引擎突變層缺陷實錄」）：

1. **w:ins 追蹤插入段（燈 F1）**：`<w:ins>` 包住含巢裝 `w:rPrChange`（rPr 內
   再裝 rPr——n830205 形態）的 run；切點落在該 run 文字內部。
2. **w:del 追蹤刪除段（燈 F2）**：`<w:del>` 包住 `w:delText` run；切點落在
   delText 內部（delText 也是文字流一部分，拆段不得吞）。
3. **w:hyperlink 多 run 段（燈 F3）**：`<w:hyperlink>` 內兩顆 run，切點落在
   第二顆 run 內部（WordWithAttachments／tdf154695 形態）。
4. **w:fldSimple 欄位段（燈 F4）**：`<w:fldSimple>` 包住欄位結果 run，前後
   各有欄位外正文 run；切點落在欄位內 run（Jupiter 形態）。

zip 一律 STORE：位元組穩定可重現（比照 gen_k29e_fixture.py 口徑）。
"""
import pathlib
import sys
import zipfile

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" \
xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"""

ROOT_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""

# 巢裝 rPrChange（n830205 形態）：rPr 內含 rPrChange，rPrChange 內再裝 rPr——
# 正是「首個 </w:rPr> 是內層閉標籤」的深度計數炸彈同族形態。
RPR_CHANGE = (
    '<w:rPr><w:b/><w:rPrChange w:id="1" w:author="QuireProbe" '
    'w:date="2025-11-26T00:00:00Z"><w:rPr><w:i/></w:rPr></w:rPrChange></w:rPr>'
)

BODY = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document {W}><w:body>
<w:p><w:r><w:t>基準段零容器ABCDEFGH</w:t></w:r></w:p>
<w:p><w:pPr><w:rPr><w:b/></w:rPr></w:pPr><w:r>{RPR_CHANGE}<w:t>欄位外前段XY</w:t></w:r><w:ins w:id="10" w:author="QuireProbe" w:date="2025-11-26T00:00:00Z"><w:r>{RPR_CHANGE}<w:t>插入文字ABCDEFGH</w:t></w:r></w:ins><w:r><w:t>欄位外後段ZW</w:t></w:r></w:p>
<w:p><w:r><w:t>刪除前段AB</w:t></w:r><w:del w:id="20" w:author="QuireProbe" w:date="2025-11-26T00:00:00Z"><w:r><w:delText xml:space="preserve">刪除文字CDEF</w:delText></w:r></w:del><w:r><w:t>刪除後段GH</w:t></w:r></w:p>
<w:p><w:r><w:t>連結前AB</w:t></w:r><w:hyperlink r:id="rIdHyper" w:history="1"><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>連結一</w:t></w:r><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t xml:space="preserve"> 連結二GHIJ</w:t></w:r></w:hyperlink><w:r><w:t>連結後KL</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">Figure </w:t></w:r><w:fldSimple w:instr=" SEQ Figure \\* ARABIC "><w:r><w:rPr><w:noProof/></w:rPr><w:t>12</w:t></w:r></w:fldSimple><w:r><w:t xml:space="preserve"> Jupiter</w:t></w:r></w:p>
<w:p><w:ins w:id="30" w:author="QuireProbe" w:date="2025-11-26T00:00:00Z"><w:hyperlink r:id="rIdHyper" w:history="1"><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>巢裝連結內文字ABCDEFGH</w:t></w:r></w:hyperlink></w:ins></w:p>
<w:p><w:r><w:t>版面填充段：把版面撐出可預測的斷點，確保各段落位置穩定不被編輯位移。</w:t></w:r></w:p>
<w:p><w:r><w:t>版面填充乙段確保第一頁行群數量穩定。</w:t></w:r></w:p>
</w:body></w:document>"""

DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdHyper" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.invalid/" TargetMode="External"/>
</Relationships>"""


def build(out: pathlib.Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    # 固定 date_time（ODF-E1 口徑）：zipfile 預設寫目前時間 ⇒ 每次重跑位元組
    # 不同＝「產生器可重現」宣稱不成立；STORE＋固定時間做到 byte 級冪等。
    fixed = (2026, 11, 26, 0, 0, 0)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_STORED) as z:
        for name, data in (
            ("[Content_Types].xml", CONTENT_TYPES),
            ("_rels/.rels", ROOT_RELS),
            ("word/document.xml", BODY),
            ("word/_rels/document.xml.rels", DOC_RELS),
        ):
            info = zipfile.ZipInfo(name, date_time=fixed)
            info.compress_type = zipfile.ZIP_STORED
            info.external_attr = 0o600 << 16
            z.writestr(info, data)


if __name__ == "__main__":
    fixtures = pathlib.Path(__file__).resolve().parent.parent / "fixtures"
    main_path = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else \
        fixtures / "split_revision_containers.docx"
    build(main_path)
    print(f"寫入 {main_path}")
