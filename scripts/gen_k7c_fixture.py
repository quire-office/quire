#!/usr/bin/env python3
"""產生 fixtures/k7c_table_xoff.docx：table_x_off 負偏移三盞紅燈（t_4203a0a5 K7c）。

版心：pgSz 11906 − pgMar 1440×2 ＝ 9026 twip ＝ 451.30pt；左緣頁座標 72pt。
五張單列表（垂直框線 x 全相異，便於逐燈斷言），每張前帶一段標題文字：

- T1 宣告 jc=center、無宣告寬（tblW auto）、自然寬 551.30pt 超版心：
  修正後置中負偏移→左右各越界 50.0pt：框線 22.0／573.30（幾何對稱鎖）。
  修正前 `.max(0.0)` 截 0→貼左緣（72／623.3 單邊右溢）＝紅燈。
- T2 宣告 jc=right、無宣告寬、自然寬 475.30pt 超版心：
  修正後右緣釘死版心右界 523.30、左越界 24.0pt：框線 48.0／523.30。
  口徑取捨（寫死於 table.rs 註解）：right＝右緣釘右界、越界量往左溢出，
  不再被 `.max(0.0)`／`.max(ind)` 鉗回貼左。
- T3 宣告寬表（tblW 8026 dxa＝401.30pt）＋負縮排 tblInd −360 dxa（−18pt）、
  預設左對齊：修正後 x＝72−18＝54.0／455.30。
  修正前 `.max(0.0)` 吞負縮排→72.0＝紅燈（合法負 tblInd 對齊正文邊界外移）。
- T4 `<w:tblW w:w="0" w:type="dxa"/>`（歷史 auto 變體）＋自然寬 511.30pt 超版心、
  未宣告 jc：修正後 width_spec 過濾 num≤0→None→走 K7 自然寬均分臂：
  框線 42.0／553.30。修正前 Some(Pt(0))→is_some 壓版心→貼左 72.0＝紅燈。
- T5 未宣告寬＋未宣告 jc＋tblInd 1 twip（0.05pt）超版心自然寬表：
  修正後容差比較（|ind|<0.1pt 視為無顯式縮排）→仍走均分臂 52.0／543.30。
  修正前 `== 0.0` 精確比較→整臂失效→0.05pt 階躍貼左＝紅燈。

BUGGY 回塞態（K7_WRAP_WIDTH_BUGGY=1）：resolve_columns 把所有自然寬表壓回
版心→ T1/T2（center/right 臂失去超版心前提、偏移歸 0 貼左）與 T4/T5
（K7 均分臂回塞貼左）四燈必紅；T3（宣告寬＋負縮排臂）不受 K7 開關影響
→恆綠（K7c 修正獨立性鎖）。

zip 一律 STORE＋固定內容：位元組可重現（fixture 鐵律）。
"""
import pathlib
import sys
import zipfile

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
R = 'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'

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

DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"""

BORDERS = ("<w:tblBorders>"
           '<w:top w:val="single" w:sz="8" w:space="0" w:color="000000"/>'
           '<w:left w:val="single" w:sz="8" w:space="0" w:color="000000"/>'
           '<w:bottom w:val="single" w:sz="8" w:space="0" w:color="000000"/>'
           '<w:right w:val="single" w:sz="8" w:space="0" w:color="000000"/>'
           "</w:tblBorders>")


def table(label: str, tblpr: str, col_tw: int) -> str:
    """燈標題段＋單列兩欄表（兩欄等寬 col_tw、四邊 single 框線）。

    tblPr 內依 ECMA-376 §17.4.72 宣告順序：tblW→jc→tblInd→tblBorders
    （歷史檔亂序也能解析，但 fixture 走正序以免無謂變數）。"""
    cell = (f'<w:tc><w:tcPr><w:tcW w:w="{col_tw}" w:type="dxa"/></w:tcPr>'
            f"<w:p><w:r><w:t>{label}格內</w:t></w:r></w:p></w:tc>")
    return (f"<w:p><w:r><w:t>{label}</w:t></w:r></w:p>"
            "<w:tbl><w:tblPr>" + tblpr + BORDERS + "</w:tblPr>"
            f'<w:tblGrid><w:gridCol w:w="{col_tw}"/><w:gridCol w:w="{col_tw}"/></w:tblGrid>'
            f"<w:tr>{cell}{cell}</w:tr></w:tbl>")


TABLES = [
    # T1：center＋無宣告寬＋超版心（5513×2＝11026 tw＝551.30pt）
    table("置中超寬", '<w:tblW w:w="0" w:type="auto"/><w:jc w:val="center"/>', 5513),
    # T2：right＋無宣告寬＋超版心（4753×2＝9506 tw＝475.30pt）
    table("右對齊超寬", '<w:tblW w:w="0" w:type="auto"/><w:jc w:val="right"/>', 4753),
    # T3：宣告寬 8026 dxa＋負 tblInd −360 tw（−18pt）（4013×2＝8026 tw＝401.30pt）
    table("負縮排宣告寬", '<w:tblW w:w="8026" w:type="dxa"/><w:tblInd w:w="-360" w:type="dxa"/>', 4013),
    # T4：tblW w=0 dxa（auto 變體誤判）＋超版心（5113×2＝10226 tw＝511.30pt）
    table("零宣告寬變體", '<w:tblW w:w="0" w:type="dxa"/>', 5113),
    # T5：1 twip 微縮排＋超版心（4913×2＝9826 tw＝491.30pt）
    table("微縮排均分", '<w:tblW w:w="0" w:type="auto"/><w:tblInd w:w="1" w:type="dxa"/>', 4913),
]

SECT = ("<w:sectPr>"
        '<w:pgSz w:w="11906" w:h="16838"/>'
        '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
        'w:header="708" w:footer="708" w:gutter="0"/>'
        "</w:sectPr>")


def body() -> str:
    inner = "".join(TABLES)
    return ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
            f"<w:document {W} {R}><w:body>{inner}{SECT}</w:body></w:document>")


def main(out: pathlib.Path):
    with zipfile.ZipFile(out, "w", zipfile.ZIP_STORED) as z:
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        z.writestr("_rels/.rels", ROOT_RELS)
        z.writestr("word/document.xml", body())
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
    print(f"寫入 {out}（{out.stat().st_size} bytes）")


if __name__ == "__main__":
    default = pathlib.Path(__file__).resolve().parent.parent / "fixtures" / "k7c_table_xoff.docx"
    main(pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else default)
