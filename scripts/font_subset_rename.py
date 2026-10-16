"""pyftsubset 產物的 OFL 改名：修改過的 OFL 字型不得使用保留名稱（FRN）。

設定 family/full name 為「Quire Noto Sans TC」，並在描述與授權說明保留
出處（modified from Noto Sans CJK TC, SIL OFL 1.1）。
"""
import sys

from fontTools.ttLib import TTFont

path = sys.argv[1]
FAMILY = "Quire Noto Sans TC"
DESC = (
    "Modified from Noto Sans CJK TC (Google, SIL OFL 1.1) for Quire: "
    "subset of characters required by the quire corpus."
)
LICENSE = (
    "This Font Software is licensed under the SIL Open Font License, "
    "Version 1.1. See assets/OFL.txt."
)

font = TTFont(path)
name = font["name"]
# 逐筆改寫既有 name record（保留其 platform/encoding/lang 組合）。
for rec in list(name.names):
    if rec.nameID in (1, 3, 4, 16):
        name.setName(FAMILY, rec.nameID, rec.platformID, rec.platEncID, rec.langID)
    elif rec.nameID == 10:
        name.setName(DESC, rec.nameID, rec.platformID, rec.platEncID, rec.langID)
    elif rec.nameID == 13:
        name.setName(LICENSE, rec.nameID, rec.platformID, rec.platEncID, rec.langID)
    elif rec.nameID == 14:
        name.setName("SIL Open Font License 1.1", rec.nameID, rec.platformID, rec.platEncID, rec.langID)
font.save(path)
print(f"renamed -> {FAMILY}", file=sys.stderr)
