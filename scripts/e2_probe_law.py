#!/usr/bin/env python3
"""E2：LO 直排發射規律探針（合成 docx 網格，解出錨定與步長定律）。

座標系鐵證（pymupdf top-origin，已用純 SVG rotate 探針校驗）：
  vert270 dir=[0,-1] 起點(基線)=右上、往左上長；vert dir=[0,1] 起點=左下、往右下長。
本腳本對 (vert|vert270) × (anchor t|ctr|b) × (段數 1|2|3) × (框寬 100|200) 合成
docx，走 render_lab 同一條 LO 轉換路徑轉 PDF，抽每行 dir+bbox 與框矩形，
把「框→行原點」的定律從 LO 實測反解出來（不再靠 4 個樣本猜）。
"""
import json
import pathlib
import sys
import zipfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))
import pymupdf  # noqa: E402
from scripts.render_lab import find_soffice, lo_scratch, lo_to_pdf, kill_stale_lo  # noqa: E402

EMU_PT = 12700
TEMPLATE = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:wpc="http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas"
 xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
 xmlns:o="urn:schemas-microsoft-com:office:office"
 xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
 xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"
 xmlns:v="urn:schemas-microsoft-com:vml"
 xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing"
 xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
 xmlns:w10="urn:schemas-microsoft-com:office:word"
 xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
 xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"
 xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"
 xmlns:wpi="http://schemas.microsoft.com/office/word/2010/wordprocessingInk"
 xmlns:wne="http://schemas.microsoft.com/office/word/2006/wordml"
 xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
 mc:Ignorable="w14 wp14">
<w:body>
<w:p><w:r><w:drawing>
<wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="251658240"
 behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">
<wp:simplePos x="0" y="0"/>
<wp:positionH relativeFrom="page"><wp:posOffset>{px}</wp:posOffset></wp:positionH>
<wp:positionV relativeFrom="page"><wp:posOffset>{py}</wp:posOffset></wp:positionV>
<wp:extent cx="{cx}" cy="{cy}"/>
<wp:effectExtent l="0" t="0" r="0" b="0"/>
<wp:wrapNone/><wp:docPr id="1" name="Probe"/><wp:cNvGraphicFramePr/>
<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
<a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
<wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
<wps:cNvSpPr txBox="1"/>
<wps:spPr>
<a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>
<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/>
<a:ln><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>
</wps:spPr>
<wps:txbx>
<w:txbxContent>{paras}</w:txbxContent>
</wps:txbx>
<wps:bodyPr rot="0" vert="{vert}" wrap="square" lIns="0" tIns="0" rIns="0" bIns="0"
 anchor="{anchor}" anchorCtr="0"><a:noAutofit/></wps:bodyPr>
</wps:wsp>
</a:graphicData></a:graphic>
</wp:anchor></w:drawing></w:r></w:p>
<w:sectPr><pgSz xmlns="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
 w:w="12240" w:h="15840"/></w:sectPr>
</w:body></w:document>"""

CT = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"""

RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""


def make_docx(path: pathlib.Path, vert: str, anchor: str, n_paras: int, w_pt: float, h_pt: float) -> None:
    paras = "".join(
        f'<w:p><w:r><w:t>P{i + 1}.</w:t></w:r></w:p>' for i in range(n_paras)
    )
    doc = TEMPLATE.format(
        px=int(50 * EMU_PT), py=int(50 * EMU_PT),
        cx=int(w_pt * EMU_PT), cy=int(h_pt * EMU_PT),
        vert=vert, anchor=anchor, paras=paras,
    )
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CT)
        z.writestr("_rels/.rels", RELS)
        z.writestr("word/document.xml", doc)


def main() -> int:
    out_dir = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/e2_probe")
    out_dir.mkdir(parents=True, exist_ok=True)
    kill_stale_lo()
    soffice = find_soffice()
    scratch, s_prof, _ = lo_scratch(soffice)
    res = {}
    for vert in ("vert", "vert270"):
        for anchor in ("t", "ctr", "b"):
            for n_paras in (1, 2, 3):
                for w_pt in (100.0, 200.0):
                    h_pt = 300.0
                    name = f"{vert}_{anchor}_n{n_paras}_w{int(w_pt)}"
                    docx = out_dir / f"{name}.docx"
                    make_docx(docx, vert, anchor, n_paras, w_pt, h_pt)
                    pdf = lo_to_pdf(soffice, docx, scratch, s_prof)
                    if pdf is None:
                        res[name] = {"error": "lo_fail"}
                        continue
                    d = pymupdf.open(str(pdf))
                    pg = d[0]
                    lines = []
                    for blk in pg.get_text("dict").get("blocks", []):
                        for ln in blk.get("lines", []):
                            lines.append({
                                "dir": [round(float(v), 3) for v in ln["dir"]],
                                "bbox": [round(float(v), 1) for v in ln["bbox"]],
                                "text": "".join(s.get("text", "") for s in ln.get("spans", [])),
                                "size": round(float((ln["spans"] or [{}])[0].get("size", 0)), 2),
                            })
                    frames = [[round(float(v), 1) for v in (dr["rect"].x0, dr["rect"].y0,
                                                            dr["rect"].x1, dr["rect"].y1)]
                              for dr in pg.get_drawings()
                              if dr["rect"].width > 2 and dr["rect"].height > 2]
                    d.close()
                    res[name] = {"frame": frames, "lines": lines}
                    print(name, json.dumps(res[name], ensure_ascii=False), flush=True)
    (out_dir / "probe_truth.json").write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
