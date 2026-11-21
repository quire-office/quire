#!/usr/bin/env python3
"""生成 fixtures/table_gridspan_prefix.docx（E 返修卡 t_c1dbc6d7 S2-2：gridSpan 錯讀樣本）。

# 這檔要拍到什麼（先紅後綠的紅燈載體）
`core/engine-api/src/mutation_k29c.rs::cell_grid_span` 對**整段 `<w:tcPr>`** 呼叫
`attr_in_tag(raw, "w:val")`——找的是「本段 tcPr 內第一個 w:val」，不是
「`w:gridSpan` 自己的 w:val」。ECMA-376 下 `w:tcPr` 內 `w:tcBorders`／`w:shd`／
`w:vMerge`／`w:cnfStyle` 皆可先於 `w:gridSpan` 宣告且自帶 `w:val`：
  - `w:shd w:val="clear"` ⇒ 字串 val → `parse::<usize>()` 失敗 → 跨欄數錯算 **1**；
  - `w:vMerge w:val="3"`（畸形數字 val，容錯路徑必須不 panic）⇒ 錯算 **3**。
同檔 `table_map.rs` 的游標反查走 `CellProps::grid_span()`（精確讀
`w:gridSpan/@w:val`）→ **同語義兩制**：`table_locator_json` 給的
`grid_col_start`／`grid_span` 與 mutation 側 `row_layout` 算出的 `col_start`／
`span` 失配 → 餵 `InsertTableColumn`／`DeleteTableColumn`／`MergeTableCells`
操作錯欄。

# 結構（一張表 2 列，兩個前綴變體各一列；斷言逐列手核）
T（1 根表，3 網格欄）
  r0：c0（tcPr **先** `w:shd w:val="clear"` 再 `w:gridSpan w:val="2"`，段 0）
      ＋c1（無跨欄，段 1）→ 正確網格：c0 col_start=0 span=2、c1 col_start=2 span=1
      現行 defect：c0 span 錯算 1 ⇒ c1 col_start 錯算 1
  r1：c0（tcPr **先** `w:vMerge w:val="3"` 再 `w:gridSpan w:val="2"`，段 2）
      ＋c1（無跨欄，段 3）→ 正確同上；現行 defect：c0 span 錯算 3 ⇒ c1 col_start 錯算 3

段落索引＝主解析器文件序：段 0,1 在 r0、段 2,3 在 r1（每格恰一段，逐格可直查）。

# 紀律
純 stdlib、STORE 壓縮、固定時間戳（gen_multi_table_fixture.py 同款口徑）。
自製合成檔：無真實內嵌圖片、無個資，可入 fixtures/（PM 定案）。
"""
import io
import struct
import zipfile
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "fixtures" / "table_gridspan_prefix.docx"

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'


def para(text):
    return f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>'


def tc(text, w, extra=""):
    return (
        f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/>{extra}</w:tcPr>'
        f"{para(text)}</w:tc>"
    )


# 兩種「其他 w:val 先於 w:gridSpan」的前綴（ECMA-376 皆合法宣告順序）。
SHD_PREFIX = '<w:shd w:val="clear" w:color="auto" w:fill="D9E2F3"/>'
VMERGE_NUMERIC = '<w:vMerge w:val="3"/>'       # 畸形數字 val（容錯必不 panic）
GRIDSPAN = '<w:gridSpan w:val="2"/>'

T = (
    "<w:tbl>"
    '<w:tblPr><w:tblW w:w="9000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr>'
    '<w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>'
    # r0：w:shd 字串 val 前綴 ＋ gridSpan=2 ＋ 同列下一格（網格起點必須是 2）。
    "<w:tr>"
    + tc("底紋前綴 r0c0 跨兩欄", 6000, SHD_PREFIX + GRIDSPAN)
    + tc("底紋前綴 r0c1", 3000)
    + "</w:tr>"
    # r1：w:vMerge 數字 val 前綴 ＋ gridSpan=2 ＋ 同列下一格。
    "<w:tr>"
    + tc("合併前綴 r1c0 跨兩欄", 6000, VMERGE_NUMERIC + GRIDSPAN)
    + tc("合併前綴 r1c1", 3000)
    + "</w:tr>"
    + "</w:tbl>"
)

DOC = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f"<w:document {W}><w:body>"
    + T
    + '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>'
    '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
    'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
    "</w:body></w:document>"
)

CT = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Override PartName="/word/document.xml" '
    'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
    "</Types>"
)

RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" '
    'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
    'Target="word/document.xml"/>'
    "</Relationships>"
)

FILES = [
    ("[Content_Types].xml", CT),
    ("_rels/.rels", RELS),
    ("word/document.xml", DOC),
]


def zip_local(name, data):
    crc = zlib.crc32(data) & 0xFFFFFFFF
    n = name.encode()
    lh = struct.pack(
        "<IHHHHHIIIHH", 0x04034B50, 20, 0, 0, 0, 0, crc, len(data), len(data),
        len(n), 0)
    return lh + n + data, crc


def zip_central(name, data, crc, offset):
    n = name.encode()
    return struct.pack(
        "<IHHHHHHIIIHHHHHII", 0x02014B50, 20, 20, 0, 0, 0, 0, crc, len(data), len(data),
        len(n), 0, 0, 0, 0, 0x20, offset) + n


def main():
    locals_, centrals, off = [], [], 0
    for name, text in FILES:
        b = text.encode("utf-8")
        blob, crc = zip_local(name, b)
        locals_.append(blob)
        centrals.append(zip_central(name, b, crc, off))
        off += len(blob)
    out = b"".join(locals_) + b"".join(centrals) + struct.pack(
        "<IHHHHIIH", 0x06054B50, 0, 0, len(FILES), len(FILES),
        len(b"".join(centrals)), len(b"".join(locals_)), 0)
    OUT.write_bytes(out)
    zf = zipfile.ZipFile(io.BytesIO(out))
    assert zf.testzip() is None, "zip CRC 驗證失敗"
    assert sorted(zf.namelist()) == sorted(n for n, _ in FILES)
    print(f"OK {OUT} ({len(out)} bytes)")


if __name__ == "__main__":
    main()
