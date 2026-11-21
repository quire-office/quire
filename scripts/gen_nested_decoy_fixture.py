#!/usr/bin/env python3
"""生成巢狀表 decoy／gridSpan 溢位兩張樣本檔（E 返修卡 t_c1dbc6d7 S3-5＋S3-2）。

產出兩檔（同檔頭紀律：純 stdlib、STORE、固定時間戳，位元組可重現）：

1. `fixtures/table_nested_decoy.docx`（S3-5：巢狀表直通 M4 的鑑別樣本）
   根表 T0（1 列 2 欄）
     c0：段「宿主格 H」＋**兩張**巢狀子表（decoy 口徑樣本——`sub_table_idx`
         必須選對那張）：
           S0＝**decoy**（tblW=1111，段「decoy 子表 D」）
           S1＝**目標**（tblW=2222，1 列 2 欄：段「目標子表 N0」／「目標子表 N1」）
     c1：段「根表 r0c1」
   既有 `table_multi.docx` 的巢狀格只有**一張**子表（sub_table_idx 恆 0），
   「子表序選錯」這條路徑從未被任何考卷打過；本檔補上。

2. `fixtures/table_gridspan_overflow.docx`（S3-2：wasm32 debug 溢位面）
   一張 1 列 2 欄表，c0 帶畸形 `w:gridSpan w:val="18446744073709551615"`
   （＝usize::MAX：`parse::<usize>()` **成功**）＋ c1 正常格。
   `table_map.rs` 的 `col_cursor += span` 與 `mutation_k29c.rs` 的 `col += span`
   ／`cell.col_start + cell.span` 皆未 `saturating_add` ⇒ debug 建構直接
   `attempt to add with overflow` **panic**（wasm 邊 panic＝瀏覽器 abort，
   零容忍）。canonical 側 `CellProps::grid_span()` 走 u32 解析、該值溢出
   u32 ⇒ 回 1（不 panic）——兩邊不同口徑正是本盞要釘死的形態。

自製合成檔：無真實內嵌圖片、無個資，可入 fixtures/（PM 定案）。
"""
import io
import struct
import zipfile
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'


def para(text):
    return f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>'


def tc(text, w, extra="", blocks=None):
    """一個 w:tc：tcPr（寬＋可選附加屬性）＋段落（或自訂 block 序列）。"""
    inner = blocks if blocks is not None else para(text)
    return f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/>{extra}</w:tcPr>{inner}</w:tc>'


def tbl(rows, w_total, ncols, tblw=None):
    grid = "".join('<w:gridCol w:w="3000"/>' for _ in range(ncols))
    return (
        "<w:tbl>"
        f'<w:tblPr><w:tblW w:w="{tblw or w_total}" w:type="dxa"/>'
        '<w:tblLayout w:type="fixed"/></w:tblPr>'
        f"<w:tblGrid>{grid}</w:tblGrid>"
        + "".join(f"<w:tr>{r}</w:tr>" for r in rows)
        + "</w:tbl>"
    )


def wrap_doc(body):
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f"<w:document {W}><w:body>"
        + body
        + '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>'
        '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
        'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
        "</w:body></w:document>"
    )


# ---------------------------------------------------------------------------
# 1) 巢狀 decoy 樣本
# ---------------------------------------------------------------------------
S_DECOY = tbl([tc("decoy 子表 D", 1111)], 1111, 1, tblw=1111)
S_TARGET = tbl(
    [tc("目標子表 N0", 1100) + tc("目標子表 N1", 1100)],
    2222,
    2,
    tblw=2222,
)
HOST_BLOCKS = para("宿主格 H") + S_DECOY + S_TARGET
NESTED_DECOY_DOC = wrap_doc(
    tbl(
        [
            # 一列兩格：c0＝宿主格（段落＋decoy 子表＋目標子表）、c1＝普通格。
            tc("", 6000, blocks=HOST_BLOCKS) + tc("根表 r0c1", 3000),
        ],
        9000,
        2,
    )
)

# ---------------------------------------------------------------------------
# 2) gridSpan 溢位樣本（兩條獨立攻擊面，各自可辨）
#
#    r0：兩格各 `w:gridSpan w:val="4294967295"`（＝u32::MAX，canonical
#        `CellProps::grid_span()` 解析**成功**）＋一格正常。
#        → `table_map.rs` 的 `col_cursor += span` 在 **wasm32（32 位 usize）**
#          下第二個累加即 `attempt to add with overflow`；64 位主機不溢位，
#          故本列只作「不 panic ＋ 幾何不炸壞」的邊界面。
#    r1：一格 `w:gridSpan w:val="18446744073709551615"`（＝usize::MAX，
#        mutation 側 `cell_grid_span` 的 `parse::<usize>()` **成功**、canonical
#        的 u32 解析失敗回 1——同語義兩制的極端形）＋一格正常。
#        → `mutation_k29c.rs::row_layout` 的 `col += span`：第一格把 col 推到
#          usize::MAX，第二格 `col += 1` 在 **debug 建構直接 panic**（64 位主機
#          也紅，非只在 wasm32）——本列是主機可觀察的紅燈載體。
# ---------------------------------------------------------------------------
OVERFLOW_DOC = wrap_doc(
    tbl(
        [
            # r0：u32::MAX ×2（wasm32 溢位面）
            tc("欄溢位 r0c0", 3000, '<w:gridSpan w:val="4294967295"/>')
            + tc("欄溢位 r0c1", 3000, '<w:gridSpan w:val="4294967295"/>')
            + tc("正常格 r0c2", 3000),
            # r1：usize::MAX（mutation 側 parse 成功 → debug panic 載體）
            tc("欄溢位 r1c0", 3000, '<w:gridSpan w:val="18446744073709551615"/>')
            + tc("正常格 r1c1", 3000),
        ],
        6000,
        3,
    )
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


def emit(out_path, document_xml):
    files = [
        ("[Content_Types].xml", CT),
        ("_rels/.rels", RELS),
        ("word/document.xml", document_xml),
    ]
    locals_, centrals, off = [], [], 0
    for name, text in files:
        b = text.encode("utf-8")
        blob, crc = zip_local(name, b)
        locals_.append(blob)
        centrals.append(zip_central(name, b, crc, off))
        off += len(blob)
    out = b"".join(locals_) + b"".join(centrals) + struct.pack(
        "<IHHHHIIH", 0x06054B50, 0, 0, len(files), len(files),
        len(b"".join(centrals)), len(b"".join(locals_)), 0)
    out_path.write_bytes(out)
    zf = zipfile.ZipFile(io.BytesIO(out))
    assert zf.testzip() is None, "zip CRC 驗證失敗"
    assert sorted(zf.namelist()) == sorted(n for n, _ in files)
    print(f"OK {out_path} ({len(out)} bytes)")


def main():
    emit(ROOT / "fixtures" / "table_nested_decoy.docx", NESTED_DECOY_DOC)
    emit(ROOT / "fixtures" / "table_gridspan_overflow.docx", OVERFLOW_DOC)


if __name__ == "__main__":
    main()
