#!/usr/bin/env python3
"""生成 fixtures/sdt_wrapped_table.docx（E 返修卡 t_c1dbc6d7 S2-1：透明包裝錯位樣本）。

# 這檔要拍到什麼（先紅後綠的紅燈載體）
`core/ooxml/src/table.rs` 的 `tbl_parent()` 與 `tr`／`tc` 的 `under()` 白名單
**穿透** `Frame::Skip`／`Opaque` 透明包裝（`w:sdt`／`w:sdtContent`／`w:ins`／
`w:del`／`w:customXml`…），把包裝節點**內**的表掛進 canonical 樹並計入
`root_slot`；而 `core/engine-api/src/mutation_k29c.rs::locate_table` 走
`direct_children(text, body, "tbl")`——**不穿透**任何包裝。兩套口徑不同 ⇒
只要被包裝節點之後還有 ≥1 個未包裝的直接兄弟節點，反查回傳的座標就
**錯位且仍落在界內**（`Vec::get` 越界檢查不會報錯），M4 靜默改錯表。

# 結構（刻意最簡：錯位來源只有一處，斷言零歧義）
body 內依序＝
  1. `<w:sdt><w:sdtContent>` 包一張表 **T_A**（2 列 2 欄，段 0..3）——canonical
     `root_slot == 0`（parser 穿透包裝），raw `direct_children` **看不到**它。
  2. 一張裸表 **T_B**（1 列 2 欄，段 4..5）——canonical `root_slot == 1`，
     raw `direct_children[0] == T_B`（錯位的受害者／被誤改者）。
  3. 尾段 P_tail（段 6）：確保表後仍有 body 級節點，零影響但讓檔尾合法。

`locate_table_json(T_A 內任一段)` 必須回 `root_slot == 0`（canonical 口徑）；
把該路徑原封不動餵 M4 時，raw 側解析出的就是 **T_B** —— 現行實作會靜默改到
T_B（紅燈），修後必須誠實拒絕且 **T_B 的 raw XML 零變化**。

# 紀律
純 stdlib、STORE 壓縮、固定時間戳（可重複生成、位元組可重現，
gen_multi_table_fixture.py 同款口徑）。自製合成檔：無真實內嵌圖片、無個資，
可入 fixtures/（PM 定案）。
"""
import io
import struct
import zipfile
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "fixtures" / "sdt_wrapped_table.docx"

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'


def para(text):
    return f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>'


def tc(text, w=3000, extra=""):
    return (
        f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/>{extra}</w:tcPr>'
        f"{para(text)}</w:tc>"
    )


def tbl(rows, w_total, ncols):
    grid = "".join('<w:gridCol w:w="3000"/>' for _ in range(ncols))
    return (
        "<w:tbl>"
        f'<w:tblPr><w:tblW w:w="{w_total}" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr>'
        f"<w:tblGrid>{grid}</w:tblGrid>"
        + "".join(f"<w:tr>{r}</w:tr>" for r in rows)
        + "</w:tbl>"
    )


# T_A：藏在 w:sdt 透明包裝**內**（canonical root_slot 0；raw 直屬子節點不可見）。
T_A = tbl(
    [
        tc("SDT內表 A r0c0") + tc("SDT內表 A r0c1"),
        tc("SDT內表 A r1c0") + tc("SDT內表 A r1c1"),
    ],
    6000,
    2,
)
SDT_A = (
    "<w:sdt>"
    "<w:sdtPr><w:id w:val=\"123456\"/><w:docPartObj>"
    '<w:docPartGallery w:val="Table"/><w:docPartUnique/>'
    "</w:docPartObj></w:sdtPr>"
    f"<w:sdtContent>{T_A}</w:sdtContent>"
    "</w:sdt>"
)

# T_B：緊接在 sdt 之後的**裸表**（canonical root_slot 1；raw direct_children[0]）。
# 這是錯位標的——現行實作把「改 T_A」的補丁寫到它身上。
T_B = tbl([tc("裸表 B r0c0") + tc("裸表 B r0c1")], 6000, 2)

DOC = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f"<w:document {W}><w:body>"
    + SDT_A
    + T_B
    + para("表後尾段")
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
