#!/usr/bin/env python3
"""K18-cal 探針：LO 文字框撐列模型校準（report-only，零引擎觸碰）。

子命令：
  anchors <docx>     抽 anchor 宣告（extent/posOffset/wrap/autofit/bodyIns/宿主行列）
  measure <docx>...  LO→PDF→pymupdf 實測（橫線 y 序列/豎線/框線/span），產 JSON
  variants           受控合成檔批次（autofit×pBdr×posV×dist×wrap 掃描）→ 列高鑑別
  mutate             對真實 WC048 單變因突變 → 回灌 LO 量列界

pt = PDF 點（pymupdf matrix=1；轉換 PDF 即 1pt=1/72in，毋需 DPI 換算）。
用法例：
  python3 scripts/k18_probe.py anchors corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx
  python3 scripts/k18_probe.py measure <docx>... --out ~/.hermes/cache/scratch/k18/lo
  python3 scripts/k18_probe.py variants --out ~/.hermes/cache/scratch/k18/var
  python3 scripts/k18_probe.py mutate --out ~/.hermes/cache/scratch/k18/mut
"""
import argparse
import json
import pathlib
import sys
import zipfile
import xml.etree.ElementTree as ET

REPO = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts"))

W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
WP = "{http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing}"
A = "{http://schemas.openxmlformats.org/drawingml/2006/main}"
WPS = "{http://schemas.microsoft.com/office/word/2010/wordprocessingShape}"
WPG = "{http://schemas.microsoft.com/office/word/2010/wordprocessingGroup}"
WPC = "{http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas}"

EMU_PT = 1 / 12700.0


def pt(v):
    try:
        return round(int(v) * EMU_PT, 2)
    except (TypeError, ValueError):
        return None


# ─────────────────────────────────────────────── anchors ──

def xfrm_info(el):
    for tag in (A + "xfrm", WPC + "xfrm"):
        x = el.find(tag)
        if x is not None:
            d = {}
            for k in ("off", "chOff"):
                e = x.find(A + k)
                if e is not None:
                    d[k] = [pt(e.get("x")), pt(e.get("y"))]
            for k in ("ext", "chExt"):
                e = x.find(A + k)
                if e is not None:
                    d[k] = [pt(e.get("cx")), pt(e.get("cy"))]
            return d
    return None


def bodypr_info(wsp):
    bp = wsp.find(WPS + "bodyPr")
    if bp is None:
        return None
    autofit = None
    for c in bp:
        tag = c.tag.split("}")[-1]
        if tag in ("spAutoFit", "normAutofit", "noAutofit"):
            autofit = tag
    return {"autofit": autofit,
            "lIns": pt(bp.get("lIns")), "tIns": pt(bp.get("tIns")),
            "rIns": pt(bp.get("rIns")), "bIns": pt(bp.get("bIns")),
            "anchor": bp.get("anchor")}


def txbx_paras(txbx):
    out = []
    for p in txbx.iter(W + "p"):
        ppr = p.find(W + "pPr")
        spacing = ppr.find(W + "spacing") if ppr is not None else None
        pbbdr = ppr.find(W + "pBdr") if ppr is not None else None
        info = {"lineRule": spacing.get(W + "lineRule") if spacing is not None else None,
                "line": spacing.get(W + "line") if spacing is not None else None,
                "before": spacing.get(W + "before") if spacing is not None else None,
                "after": spacing.get(W + "after") if spacing is not None else None,
                "pBdr": [e.tag.split("}")[-1] + ":sz" + str(e.get(W + "sz"))
                         + ":space" + str(e.get(W + "space"))
                         for e in pbbdr] if pbbdr is not None else None,
                "runs": []}
        for r in p.findall(W + "r"):
            t = "".join(t.text or "" for t in r.findall(W + "t"))
            rpr = r.find(W + "rPr")
            sz = (rpr.find(W + "sz").get(W + "val")
                  if rpr is not None and rpr.find(W + "sz") is not None else None)
            if t:
                info["runs"].append({"t": t, "sz_halfpt": sz})
        out.append(info)
    return out


def wsp_summary(wsp):
    d = {}
    cnv = wsp.find(WPS + "cNvPr")
    if cnv is not None:
        d["name"], d["id"] = cnv.get("name"), cnv.get("id")
    spPr = wsp.find(WPS + "spPr")
    if spPr is not None:
        d["xfrm"] = xfrm_info(spPr)
    txbx = wsp.find(WPS + "txbx")
    if txbx is not None:
        d["paras"] = txbx_paras(txbx)
    d["bodyPr"] = bodypr_info(wsp)
    return d


def anchor_rec(d, host):
    anch = d.find(WP + "anchor")
    if anch is None:
        return None
    rec = dict(host)
    docpr = anch.find(WP + "docPr")
    if docpr is not None:
        rec["name"], rec["id"] = docpr.get("name"), docpr.get("id")
    ext = anch.find(WP + "extent")
    rec["extent_pt"] = [pt(ext.get("cx")), pt(ext.get("cy"))] if ext is not None else None
    for hh, key in (("positionH", "H"), ("positionV", "V")):
        pp = anch.find(WP + hh)
        if pp is not None:
            pv = pp.find(WP + "posOffset")
            pct = pp.find("{http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing}pctPosVOffset")
            rec["posOffset" + key] = pt(pv.text) if pv is not None else None
            rec["relativeFrom" + key] = pp.get("relativeFrom")
            if pct is not None:
                rec["pctPosV"] = pct.text
    for tag in ("wrapTopAndBottom", "wrapNone", "wrapSquare", "wrapAround",
                "wrapTight", "wrapThrough"):
        if anch.find(WP + tag) is not None:
            rec["wrap"] = tag
    rec["distT"], rec["distB"] = pt(anch.get("distT")), pt(anch.get("distB"))
    ee = anch.find(WP + "effectExtent")
    if ee is not None:
        rec["effectExtent"] = [pt(ee.get("t")), pt(ee.get("b"))]
    for tag in ("sizeRelH", "sizeRelV"):
        sr = anch.find("{http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing}" + tag)
        if sr is not None:
            pctel = sr[0] if len(sr) else None
            rec[tag] = (sr.get("relativeFrom"), pctel.text if pctel is not None else None)
    gd = anch.find(f"{A}graphic/{A}graphicData")
    if gd is not None:
        wsp, wgp = gd.find(WPS + "wsp"), gd.find(WPG + "wgp")
        if wgp is not None:
            rec["container"] = "group"
            spPr = wgp.find(WPG + "grpSpPr")
            rec["grp_xfrm"] = xfrm_info(spPr) if spPr is not None else None
            rec["children"] = [wsp_summary(c) for c in wgp.findall(WPS + "wsp")]
        elif wsp is not None:
            rec["container"] = "shape"
            rec.update(wsp_summary(wsp))
        else:
            rec["container"] = "unknown"
    return rec


def cmd_anchors(docx):
    z = zipfile.ZipFile(docx)
    root = ET.fromstring(z.read("word/document.xml"))
    raw = z.read("word/document.xml").decode("utf-8", "ignore")
    anchors = []

    def rec(el, host):
        if el.tag == W + "tbl":
            for ri, tr in enumerate(el.findall(W + "tr")):
                for ci, tc in enumerate(tr.findall(W + "tc")):
                    h = dict(host, table=host.get("table", 0) + 1, row=ri, cell=ci)
                    for ch in tc:
                        rec(ch, h)
            return
        if el.tag == W + "drawing":
            a = anchor_rec(el, host)
            if a:
                anchors.append(a)
            return
        for ch in el:
            rec(ch, host)

    for ch in root.find(W + "body"):
        rec(ch, {})
    # 表格宣告
    tables = []
    for t in root.iter(W + "tbl"):
        tinfo = {"rows": []}
        grid = t.find(W + "tblGrid")
        if grid is not None:
            # gridCol w 單位是 twips（1pt=20twip），非 EMU
            tinfo["cols_pt"] = [round(int(g.get(W + "w") or 0) / 20.0, 2)
                                for g in grid.findall(W + "gridCol")]
        for tr in t.findall(W + "tr"):
            rinfo = {}
            trPr = tr.find(W + "trPr")
            if trPr is not None:
                th = trPr.find(W + "trHeight")
                if th is not None:
                    rinfo["trHeight_twip"] = th.get(W + "val")
                    rinfo["hRule"] = th.get(W + "hRule")
            tinfo["rows"].append(rinfo)
        tables.append(tinfo)
    print(json.dumps({"file": str(docx),
                      "scan": {"spAutoFit": raw.count("<a:spAutoFit"),
                               "noAutofit": raw.count("<a:noAutofit"),
                               "mso_fit_shape_to_text": raw.count("mso-fit-shape-to-text")},
                      "tables": tables, "anchors": anchors},
                     ensure_ascii=False, indent=1))


# ─────────────────────────────────────────────── measure ──

def measure_pdf(pdf):
    import pymupdf
    out = []
    d = pymupdf.open(pdf)
    for pi, page in enumerate(d):
        hlines, vlines = [], []
        for dr in page.get_drawings():
            col = dr.get("color")
            w = dr.get("width") or 0.0
            for it in dr["items"]:
                if it[0] == "l":
                    p1, p2 = it[1], it[2]
                    if abs(p1.y - p2.y) < 0.3:
                        hlines.append({"y": round(p1.y, 2),
                                       "x0": round(min(p1.x, p2.x), 2),
                                       "x1": round(max(p1.x, p2.x), 2),
                                       "w": round(w, 3),
                                       "color": [round(c, 3) for c in col] if col else None})
                    elif abs(p1.x - p2.x) < 0.3:
                        vlines.append({"x": round(p1.x, 2),
                                       "y0": round(min(p1.y, p2.y), 2),
                                       "y1": round(max(p1.y, p2.y), 2)})
        spans = []
        for b in page.get_text("dict")["blocks"]:
            for l in b.get("lines", []):
                for s in l.get("spans", []):
                    spans.append({"x": round(s["bbox"][0], 2), "y0": round(s["bbox"][1], 2),
                                  "y1": round(s["bbox"][3], 2), "size": round(s["size"], 2),
                                  "font": s["font"], "text": s["text"]})
        out.append({"page": pi, "hlines": hlines, "vlines": vlines, "spans": spans})
    d.close()
    return out


def dedup_triples(triples):
    out = []
    for t in sorted(triples, key=lambda t: t[0]):
        if not out or t[0] - out[-1][0] > 0.4:
            out.append(t)
    return out


def cmd_measure(docxs, outdir):
    import render_lab
    outdir = pathlib.Path(outdir)
    outdir.mkdir(parents=True, exist_ok=True)
    soffice = render_lab.find_soffice()
    scratch, prof, needle = render_lab.lo_scratch(soffice)
    render_lab.kill_stale_lo()
    import atexit
    atexit.register(render_lab.kill_own_lo, needle)
    result = {}
    for f in docxs:
        f = pathlib.Path(f)
        pdf = render_lab.lo_to_pdf(soffice, f, scratch, prof, timeout=180)
        if pdf is None:
            result[f.name] = {"error": "lo_fail"}
            continue
        result[f.name] = measure_pdf(pdf)
        (outdir / (f.stem + ".pdf")).write_bytes(pdf.read_bytes())
    (outdir / "measure.json").write_text(json.dumps(result, ensure_ascii=False, indent=1))
    print("wrote", outdir / "measure.json")


def tbl_lines_and_frame(pages, page=0):
    """(表格 0.5pt 橫線 y, 文字框 0.75~1.25pt 橫線 y) 去重排序。"""
    tbl, frame = [], []
    for h in pages[page]["hlines"]:
        (tbl if abs(h["w"] - 0.5) < 0.05 else frame).append((h["y"], h["x0"], h["x1"]))
    return [t[0] for t in dedup_triples(tbl)], [t[0] for t in dedup_triples(frame)]


# ──────────────────────────────────── 合成變體（variants） ──

EMU = 12700
NS = ('xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
      'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
      'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
      'xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"')
CT = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
      '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
      '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
      '<Default Extension="xml" ContentType="application/xml"/>'
      '<Override PartName="/word/document.xml" '
      'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
      '</Types>')
RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
        'Target="word/document.xml"/></Relationships>')
DOC_RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>')


def pt2emu(v):
    return int(round(v * EMU))


def _cell(text, anchor=""):
    return ('<w:tc><w:tcPr><w:tcW w:w="3005" w:type="dxa"/></w:tcPr>'
            f'<w:p><w:pPr><w:spacing w:after="0"/></w:pPr>{anchor}'
            f'<w:r><w:t>{text}</w:t></w:r></w:p></w:tc>')


def _anchor(*, sz, italic, pbdr, autofit, wrap, dist, posv, ew, eh,
            tins, bins, lins, rins, sizerel=None):
    rpr = "<w:rPr>" + ("<w:i/><w:iCs/>" if italic else "") + f'<w:sz w:val="{sz}"/></w:rPr>'
    ppr = ""
    if pbdr:
        s_, sp = pbdr
        ppr += (f'<w:pBdr><w:top w:val="single" w:sz="{s_}" w:space="{sp}" w:color="4472C4"/>'
                f'<w:bottom w:val="single" w:sz="{s_}" w:space="{sp}" w:color="4472C4"/></w:pBdr>')
    ppr += '<w:spacing w:after="0"/>'
    para = f'<w:p><w:pPr>{ppr}</w:pPr><w:r>{rpr}<w:t>Txt</w:t></w:r></w:p>'
    para = f"<w:txbxContent>{para}</w:txbxContent>"
    af = {"spAutoFit": "<a:spAutoFit/>", "noAutofit": "<a:noAutofit/>", "none": ""}[autofit]
    wrapel = {"topAndBottom": "<wp:wrapTopAndBottom/>",
              "square": '<wp:wrapSquare wrapText="bothSides"/>',
              "none": "<wp:wrapNone/>"}[wrap]
    sr = ""
    if sizerel:
        sr = (f'<wp14:sizeRelV relativeFrom="margin" '
              f'xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing">'
              f'<wp14:pctHeight>{sizerel}</wp14:pctHeight></wp14:sizeRelV>')
    return (f'<w:r><w:rPr><w:noProof/></w:rPr><w:drawing>'
            f'<wp:anchor distT="{pt2emu(dist)}" distB="{pt2emu(dist)}" distL="114300" distR="114300" '
            f'simplePos="0" relativeHeight="251658240" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">'
            f'<wp:simplePos x="0" y="0"/>'
            f'<wp:positionH relativeFrom="column"><wp:align>center</wp:align></wp:positionH>'
            f'<wp:positionV relativeFrom="paragraph"><wp:posOffset>{pt2emu(posv)}</wp:posOffset></wp:positionV>'
            f'<wp:extent cx="{pt2emu(ew)}" cy="{pt2emu(eh)}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>'
            f'{wrapel}<wp:docPr id="1" name="TB"/>'
            f'<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">'
            f'<a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">'
            f'<wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr>'
            f'<a:xfrm><a:off x="0" y="0"/><a:ext cx="{pt2emu(ew)}" cy="{pt2emu(eh)}"/></a:xfrm>'
            f'<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>'
            f'<a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill>'
            f'<a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></wps:spPr>'
            f'<wps:txbx>{para}</wps:txbx>'
            f'<wps:bodyPr wrap="square" lIns="{pt2emu(lins)}" tIns="{pt2emu(tins)}" '
            f'rIns="{pt2emu(rins)}" bIns="{pt2emu(bins)}" anchor="t">{af}</wps:bodyPr>'
            f'</wps:wsp></a:graphicData></a:graphic>{sr}</wp:anchor></w:drawing></w:r>')


def make_docx(path, *, anchor=True, **kw):
    a = _anchor(**kw) if anchor else ""
    row = lambda c1, c2, c3: f"<w:tr>{c1}{c2}{c3}</w:tr>"
    rows = (row(_cell("This is a test."), _cell("This is a test.", a), _cell("This is a test."))
            + row(_cell("This is a test."), _cell("This is a test."), _cell("This is a test."))
            + row(_cell("This is a test."), _cell("This is a test."), _cell("This is a test.")))
    body = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:document {NS}><w:body>'
            '<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/>'
            '<w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
            '<w:left w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
            '<w:bottom w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
            '<w:right w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
            '<w:insideH w:val="single" w:sz="4" w:space="0" w:color="000000"/></w:tblBorders></w:tblPr>'
            '<w:tblGrid><w:gridCol w:w="3005"/><w:gridCol w:w="3005"/><w:gridCol w:w="3006"/></w:tblGrid>'
            + rows + '</w:tbl><w:p/></w:body></w:document>')
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CT)
        z.writestr("_rels/.rels", RELS)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/document.xml", body)


BASE = dict(sz=24, italic=True, pbdr=(24, 8), autofit="spAutoFit", wrap="topAndBottom",
            dist=7.2, posv=21.6, ew=273.6, eh=110.55, tins=3.6, bins=3.6, lins=7.2, rins=7.2)


def mk(**over):
    return {**BASE, **over}


VARIANTS = {
    "b0_replica": mk(),
    "b1_noautofit": mk(autofit="noAutofit"),
    "b2_h20": mk(autofit="noAutofit", eh=20),
    "b3_h40": mk(autofit="noAutofit", eh=40),
    "b4_h60": mk(autofit="noAutofit", eh=60),
    "b5_h80": mk(autofit="noAutofit", eh=80),
    "b6_h160": mk(autofit="noAutofit", eh=160),
    "b7_posv0": mk(autofit="noAutofit", posv=0),
    "b8_posv40": mk(autofit="noAutofit", posv=40),
    "b9_posv60": mk(autofit="noAutofit", posv=60),
    "b10_posv100": mk(autofit="noAutofit", posv=100),
    "b11_dist0": mk(autofit="noAutofit", dist=0),
    "b12_dist14": mk(autofit="noAutofit", dist=14.4),
    "b13_dist28": mk(autofit="noAutofit", dist=28.8),
    "b14_wrapSquare": mk(autofit="noAutofit", wrap="square"),
    "b15_wrapNone": mk(autofit="noAutofit", wrap="none"),
    "b16_nobox": mk(anchor=False),
    "b17_spAuto_h40": mk(eh=40),
    "b18_pbdr_space0": mk(pbdr=(24, 0)),
    "b19_nopbdr": mk(pbdr=None),
    "b20_sizerel20_noauto": mk(autofit="noAutofit", sizerel="20000"),
}


def cmd_variants(outdir):
    out = pathlib.Path(outdir)
    out.mkdir(parents=True, exist_ok=True)
    import render_lab
    soffice = render_lab.find_soffice()
    scratch, prof, needle = render_lab.lo_scratch(soffice)
    render_lab.kill_stale_lo()
    import atexit
    atexit.register(render_lab.kill_own_lo, needle)
    res = {}
    for name, kw in VARIANTS.items():
        p = out / f"{name}.docx"
        kw = dict(kw)
        anchor = kw.pop("anchor", True)
        make_docx(p, anchor=anchor, **kw)
        pdf = render_lab.lo_to_pdf(soffice, p, scratch, prof, timeout=180)
        if pdf is None:
            res[name] = {"error": "lo_fail"}
            continue
        pages = measure_pdf(pdf)
        tbl, frame = tbl_lines_and_frame(pages)
        res[name] = {"row0": round(tbl[1] - tbl[0], 2) if len(tbl) >= 2 else None,
                     "tbl": tbl, "frame": frame, "kw": kw}
        (out / f"{name}.pdf").write_bytes(pdf.read_bytes())
        print(f"{name:22s} row0={res[name]['row0']} tbl={tbl} frame={frame}")
    (out / "variants.json").write_text(json.dumps(res, ensure_ascii=False, indent=1))


# ─────────────────────────────────────────────── mutate ──

WC048 = REPO / "corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx"


def cmd_mutate(outdir):
    out = pathlib.Path(outdir)
    out.mkdir(parents=True, exist_ok=True)
    zin = zipfile.ZipFile(WC048)

    def build(name, fn):
        xml = zin.read("word/document.xml").decode("utf-8")
        new = fn(xml)
        assert new != xml, f"{name}: 突變未命中"
        p = out / f"{name}.docx"
        with zipfile.ZipFile(p, "w", zipfile.ZIP_DEFLATED) as z:
            for it in zin.infolist():
                data = (new.encode("utf-8") if it.filename == "word/document.xml"
                        else zin.read(it.filename))
                z.writestr(it, data)
        return p

    def drop_autofit(marker):
        def fn(x):
            i = x.index(marker)
            j = x.index("<wps:bodyPr", i)
            k = x.index("</wps:bodyPr>", j)
            blk = x[j:k].replace("<a:spAutoFit/>", "")
            assert blk != x[j:k], marker
            return x[:j] + blk + x[k:]
        return fn

    def sub(old, new):
        def fn(x):
            assert old in x, old
            return x.replace(old, new, 1)
        return fn

    def bdrspace0(x):
        i = x.index(">Textbox2<")
        j = x.rindex("<w:pBdr>", 0, i)
        k = x.index("</w:pBdr>", j)
        return x[:j] + x[j:k].replace('w:space="8"', 'w:space="0"') + x[k:]

    def del_rect214(x):
        i = x.index('<wps:wsp><wps:cNvPr id="214"')
        j = x.index("</wps:wsp>", i) + len("</wps:wsp>")
        return x[:i] + x[j:]

    def grp_wrapnone(x):
        i = x.index('cx="4914900" cy="2349500"')
        j = x.index('<wp:wrapSquare wrapText="bothSides"/>', i)
        n = len('<wp:wrapSquare wrapText="bothSides"/>')
        return x[:j] + "<wp:wrapNone/>" + x[j + n:]

    MUTS = {
        "m1_tb2_noautofit": drop_autofit(">Textbox2<"),
        "m2_grp_cy100": sub('cx="4914900" cy="2349500"', 'cx="4914900" cy="1270000"'),
        "m3_del_rect214": del_rect214,
        "m5_tb2_bdrspace0": bdrspace0,
        "m6_tb2_posv0": sub("<wp:posOffset>274320</wp:posOffset>", "<wp:posOffset>0</wp:posOffset>"),
        "m8_grp_wrapnone": grp_wrapnone,
        "m9_grp_pct0": sub("<wp14:pctPosVOffset>2500</wp14:pctPosVOffset>",
                           "<wp14:pctPosVOffset>0</wp14:pctPosVOffset>"),
        "m10_tb1_noautofit": drop_autofit(">Textbox1<"),
    }
    built = {n: build(n, fn) for n, fn in MUTS.items() if _try(build, n, fn)}
    import render_lab
    soffice = render_lab.find_soffice()
    scratch, prof, needle = render_lab.lo_scratch(soffice)
    render_lab.kill_stale_lo()
    import atexit
    atexit.register(render_lab.kill_own_lo, needle)
    res = {}
    for n, p in built.items():
        pdf = render_lab.lo_to_pdf(soffice, p, scratch, prof, timeout=180)
        if pdf is None:
            print(f"{n:22s} LO_FAIL")
            continue
        pages = measure_pdf(pdf)
        tbl, frame = tbl_lines_and_frame(pages)
        res[n] = {"tbl": tbl, "frame": frame}
        (out / f"{n}.pdf").write_bytes(pdf.read_bytes())
        print(f"{n:22s} tbl={tbl} frame={frame}")
    (out / "mutate.json").write_text(json.dumps(res, ensure_ascii=False, indent=1))


def _try(build, n, fn):
    try:
        build(n, fn)
        return True
    except AssertionError as e:
        print("SKIP", n, e)
        return False


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    a1 = sub.add_parser("anchors")
    a1.add_argument("docx")
    a2 = sub.add_parser("measure")
    a2.add_argument("docx", nargs="+")
    a2.add_argument("--out", required=True)
    a3 = sub.add_parser("variants")
    a3.add_argument("--out", required=True)
    a4 = sub.add_parser("mutate")
    a4.add_argument("--out", required=True)
    a = ap.parse_args()
    if a.cmd == "anchors":
        cmd_anchors(a.docx)
    elif a.cmd == "measure":
        cmd_measure(a.docx, a.out)
    elif a.cmd == "variants":
        cmd_variants(a.out)
    elif a.cmd == "mutate":
        cmd_mutate(a.out)


if __name__ == "__main__":
    main()
