# LO 真值細部：tdf113183 的 triangle/star5 與 shapes-with-text 的 ellipse/triangle 繪製端點
import glob
import itertools

import pymupdf

SCRATCH = "/mnt/c/Users/me/AppData/Local/Temp/quire-render-lab/"

for t in ["ooxmlexport__tdf113183", "poi__test-data_document_shapes-with-text"]:
    pdfs = glob.glob(SCRATCH + t + ".pdf")
    if not pdfs:
        print("NO PDF", t)
        continue
    doc = pymupdf.open(pdfs[0])
    pg = doc[0]
    print("==", t)
    for d in pg.get_drawings():
        r = d["rect"]
        if r.width < 5:
            continue
        pts = []
        for it in d["items"]:
            if it[0] == "l":
                pts.append(("l", round(it[1].x, 1), round(it[1].y, 1), round(it[2].x, 1), round(it[2].y, 1)))
            elif it[0] == "c":
                pts.append(("c", round(it[1].x, 1), round(it[1].y, 1)))
            else:
                pts.append((it[0],))
        # 只列前 16 筆與筆數
        print("  type=%s rect=(%.1f,%.1f,%.1f,%.1f) n=%d" % (d["type"], r.x0, r.y0, r.x1, r.y1, len(pts)))
        for p in pts[:14]:
            print("     ", p)
    doc.close()
