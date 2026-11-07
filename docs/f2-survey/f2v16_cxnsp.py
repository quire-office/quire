#!/usr/bin/env python3
"""F2 探針：prstGeom 所屬宿主（top-level wsp vs wpg:wgp 群組子件）＋ wps:cxnSp 有無。"""
import re
import zipfile

FILES = [
    "corpus/poi/test-data_document_shapes-with-text.docx",
    "corpus/ooxmlexport/fdo74605.docx",
    "corpus/ooxmlexport/tdf113183.docx",
    "corpus/ooxmlexport/tdf113258.docx",
    "corpus/ooxmlexport/fdo80897.docx",
]

def main():
    for f in FILES:
        z = zipfile.ZipFile(f)
        xml = z.read("word/document.xml").decode("utf8", "replace")
        print("=" * 16, f)
        print("  <wps:cxnSp:", xml.count("<wps:cxnSp"), " <wps:wsp:", xml.count("<wps:wsp"),
              " <wpg:wgp:", xml.count("<wpg:wgp"))
        # 每個 prstGeom：往前找最近的 wsp / wgp 開標（wgp 開標在 wsp 開標之後 => 群組子件）
        for m in re.finditer(r'<a:prstGeom prst="([^"]+)"', xml):
            seg = xml[: m.start()]
            w = seg.rfind("<wps:wsp")
            g = seg.rfind("<wpg:wgp")
            c = seg.rfind("<wps:cxnSp")
            a = seg.rfind("<wp:anchor")
            i = seg.rfind("</wp:anchor")
            kind = "cxnSp" if c > max(w, g) else ("群組子件" if g > w else "頂層wsp")
            inside = a > i  # 目前仍在某 anchor 內
            print(f"  {m.group(1):>18} {kind:>8} anchor內={inside}")

if __name__ == "__main__":
    main()
