#!/usr/bin/env python3
"""ODF-E1：合成「manifest 宣告加密」ODT 護欄 fixture（純 stdlib）。

動機：語料庫現成的加密樣本（corpus-odt/ext/…sha1_correct.odt／…sha1_wrong.odt）
演算法只有 `Blowfish CFB` 一種，無法證明錯誤訊息裡的演算法名是**從
manifest 解析出來的資料**、而不是寫死的字串。本腳本產出四檔合成封裝補齊
鑑別力面：

1. `encrypted_aes256.odt`：content.xml／meta.xml 宣告加密，演算法
   `AES256.GCM`＋`checksum-type="SHA1/1K"`。斷言點：錯誤訊息必須出現
   `AES256.GCM`（≠ Blowfish，證明演算法名來自資料而非字面量）。
2. `encrypted_no_algorithm.odt`：`<manifest:encryption-data>` 只有 checksum
   屬性、**無** `<manifest:algorithm>` 子元素。斷言點：仍必須報加密，演算法
   位置寫「未宣告」——不得憑空捏造演算法名（不虛構鐵律）。
3. `encrypted_meta_only.odt`：只有 `meta.xml` 宣告加密，content.xml 是明文。
   斷言點：投影必須照常 `Ok`（3 段），不得因為「封裝內有加密條目」就把整份
   文件判死（過度拒收同樣是錯）。
4. `encrypted_styles_only.odt`：只有 `styles.xml` 宣告加密，content.xml 是明文。
   斷言點：`project` 必須報加密（字級級聯依賴 styles.xml，密文會讓級聯靜默
   降級成 12pt 假值）；`extract_body`（不吃樣式）必須照常 `Ok`。兩軌極性相反，
   證明攔截是「按消費面」而非一刀切。

四檔的 `mimetype` 皆首條目＋METHOD_STORE（ODF 1.3 Part 2 §3.3 合規），
所以 `MimetypeStatus::Compliant`——本卡考題只考加密閘門，不混入 mimetype 變因。
"""
import zipfile
from pathlib import Path

FIXTURES = Path(__file__).resolve().parent.parent / "corpus-odt" / "fixtures"

MIMETYPE = b"application/vnd.oasis.opendocument.text"

CONTENT_XML = """<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
<office:body><office:text>
<text:h text:outline-level="1">加密閘護欄標題</text:h>
<text:p>第一段明文</text:p>
<text:p>第二段明文</text:p>
</office:text></office:body></office:document-content>
""".encode()

STYLES_XML = """<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
<office:styles>
<style:default-style style:family="paragraph"><style:text-properties fo:font-size="11pt"/></style:default-style>
</office:styles>
</office:document-styles>
""".encode()

META_XML = (
    b'<?xml version="1.0" encoding="UTF-8"?>'
    b'<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0">'
    b"<office:meta/></office:document-meta>"
)

ENC_AES = """  <manifest:encryption-data manifest:checksum-type="SHA1/1K" manifest:checksum="iUAr59Yk3JuyB+By8lzrIiSfQ5k=">
   <manifest:algorithm manifest:algorithm-name="AES256.GCM" manifest:initialisation-vector="dwVRjh5WIY0="/>
   <manifest:key-derivation manifest:key-derivation-name="PBKDF2" manifest:iteration-count="100000" manifest:salt="sMxs4AsPLY7Q2LoZp7spzA=="/>
  </manifest:encryption-data>
"""

ENC_NO_ALGO = """  <manifest:encryption-data manifest:checksum-type="SHA1/1K" manifest:checksum="0hhSRtdcwJDZbX5h85aerYIxRLI="/>
"""

HEAD = '<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0">\n'
ROOT = ' <manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/>\n'


def entry(path: str, enc: str = "") -> str:
    if not enc:
        return f' <manifest:file-entry manifest:full-path="{path}" manifest:media-type="text/xml"/>\n'
    return (
        f' <manifest:file-entry manifest:full-path="{path}" manifest:media-type="text/xml">\n'
        f"{enc}"
        " </manifest:file-entry>\n"
    )


def manifest(enc_content: str, enc_meta: str, enc_styles: str = "") -> bytes:
    body = (
        ROOT
        + entry("content.xml", enc_content)
        + entry("styles.xml", enc_styles)
        + entry("meta.xml", enc_meta)
        + entry("settings.xml")
    )
    return (HEAD + body + "</manifest:manifest>\n").encode()


def write_odt(name: str, manifest_xml: bytes) -> Path:
    FIXTURES.mkdir(parents=True, exist_ok=True)
    target = FIXTURES / name
    with zipfile.ZipFile(target, "w") as z:
        # mimetype 必須首條目＋STORE（ODF 1.3 §3.3）；全部 entry 固定
        # date_time／壓縮位元 ⇒ 腳本重跑 byte 級冪等（sha256 可入 manifest 登記）。
        zi = zipfile.ZipInfo("mimetype", (1980, 1, 1, 0, 0, 0))
        zi.compress_type = zipfile.ZIP_STORED
        z.writestr(zi, MIMETYPE)
        for part, data in (
            ("content.xml", CONTENT_XML),
            ("styles.xml", STYLES_XML),
            ("meta.xml", META_XML),
            ("settings.xml", META_XML),
            ("META-INF/manifest.xml", manifest_xml),
        ):
            zi = zipfile.ZipInfo(part, (1980, 1, 1, 0, 0, 0))
            zi.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zi, data)
    return target


def main() -> None:
    made = [
        write_odt("encrypted_aes256.odt", manifest(ENC_AES, ENC_AES)),
        write_odt("encrypted_no_algorithm.odt", manifest(ENC_NO_ALGO, "")),
        write_odt("encrypted_meta_only.odt", manifest("", ENC_AES)),
        write_odt("encrypted_styles_only.odt", manifest("", "", ENC_AES)),
    ]
    for p in made:
        print(f"寫出 {p.name}（{p.stat().st_size} bytes）→ {p.parent}")


if __name__ == "__main__":
    main()
