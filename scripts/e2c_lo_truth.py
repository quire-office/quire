#!/usr/bin/env python3
"""E2c LO 真值抽取：fdo78957 頁首直排框的 `Quire` 行（dir＋bbox）。

口徑同 `scripts/e2_frame_truth.py`／`docs/e2-redlight.md` §2：LibreOffice
headless 轉 PDF → pymupdf `get_text("dict")` 逐行讀 `line["dir"]`＋`line["bbox"]`。
數值固化於考卷 `core/engine-api/tests/e2c_header_vert.rs`（lamp2 LO 常數），
本腳本供回歸覆核重跑，不參與引擎路徑（report-only）。

用法：python3 scripts/e2c_lo_truth.py [corpus/ooxmlexport/fdo78957.docx]
"""
import pathlib
import subprocess
import sys
import tempfile

import pymupdf

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import render_lab as RL  # noqa: E402

TARGET = "Quire"


def main() -> int:
    docx = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "corpus/ooxmlexport/fdo78957.docx")
    soffice = RL.find_soffice()
    with tempfile.TemporaryDirectory() as td:
        subprocess.run(
            [str(soffice), "--headless", "--convert-to", "pdf", "--outdir", td, str(docx)],
            check=True, capture_output=True, timeout=300,
        )
        pdf = pathlib.Path(td) / (docx.stem + ".pdf")
        doc = pymupdf.open(pdf)
        hits = 0
        for page_idx, page in enumerate(doc):  # type: ignore[attr-defined]
            for b in page.get_text("dict").get("blocks", []):
                for l in b.get("lines", []):
                    text = "".join(s.get("text", "") for s in l.get("spans", []))
                    if TARGET not in text:
                        continue
                    hits += 1
                    print(
                        f"page={page_idx} text={text!r} dir={[round(v, 3) for v in l['dir']]} "
                        f"bbox={[round(v, 1) for v in l['bbox']]} "
                        f"size={round(l['spans'][0]['size'], 1)}"
                    )
        if hits != 1:
            print(f"FAIL：{TARGET} 行應恰 1 條，實得 {hits}", file=sys.stderr)
            return 1
    print("OK：LO 真值單條命中（lamp2 LO 常數 [561.6, 380.0, 577.1, 411.9] 同源）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
