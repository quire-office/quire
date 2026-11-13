"""LO 側「框」真值：get_drawings 的路徑 bbox（文字框邊框線）＋逐頁尺寸。

用途：直排欄位原點必須錨在 LO 實際繪製的框上（±4pt），不是錨在我方 Rect。
"""
import json
import sys
import pathlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))
import pymupdf  # noqa: E402
from scripts.render_lab import find_soffice, lo_scratch, lo_to_pdf, kill_stale_lo  # noqa: E402

DOCS = [
    "corpus/ooxmlexport/btlr-textbox.docx",
    "corpus/ooxmlexport/tbrl-textbox.docx",
    "corpus/ooxmlexport/fdo69636.docx",
    "corpus/ooxmlexport/tdf103573.docx",
    "corpus/ooxmlimport/btlr-frame-vml.docx",
    "corpus/ooxmlimport/tdf124754.docx",
    "corpus/ooxmlexport/tbrl-frame-vml.docx",
    "corpus/ooxmlimport/n779627.docx",
    "corpus/ooxmlimport/groupshape-sdt.docx",
]


def main() -> int:
    kill_stale_lo()
    soffice = find_soffice()
    scratch, s_prof, _ = lo_scratch(soffice)
    out = {}
    for docx in DOCS:
        pdf = lo_to_pdf(soffice, pathlib.Path(docx), scratch, s_prof)
        if pdf is None:
            out[docx] = {"error": "lo_fail"}
            continue
        d = pymupdf.open(str(pdf))
        recs = []
        for pi, page in enumerate(d):
            for dr in page.get_drawings():
                r = dr["rect"]
                if r.width < 2 or r.height < 2:
                    continue
                recs.append({"page": pi, "rect": [round(float(v), 1) for v in (r.x0, r.y0, r.x1, r.y1)],
                             "type": dr.get("type"), "items": len(dr.get("items", []))})
        d.close()
        out[docx] = recs
        print(docx, json.dumps(recs, ensure_ascii=False), flush=True)
    pathlib.Path("<hermes>/profiles/ow-render/cache/scratch/e2_lo_frames.json").write_text(
        json.dumps(out, ensure_ascii=False, indent=1), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
