#!/usr/bin/env python3
"""K18-cal 外推驗證：先寫預測（公式），再跑 LO 實測，對答案。

公式（報告 §3，b 系列擬合）：
  grow = posV + eff_h + clash + ε（ε≈0.4）
  clash = (posV < 錨行行盒底 13.9 + distT) ？ 13.9 + distT ： 0
  eff_h = noAutofit→宣告 cy；spAutoFit→tIns+bIns+Σ(before+after+n_lines×pitch)
          + Σ_pBdr(sz/8+space)
  pitch = size×declared/240×LH_face（框內 declared 恆 240；Carlito LH=1.2207）

用法：PYTHONPATH=<repo>/scripts python3 scripts/k18_predict.py [--out DIR]
（盲測四案 pA–pD ＋ 碰撞崖判別 y1–y5。）
"""
import argparse
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import k18_probe as kp  # noqa: E402
import render_lab  # noqa: E402

LH = 1.2207  # Carlito（K6）
LINE_BOTTOM = 13.9  # 宿主列自然行盒高（b16 實測 14.0；13.9=夾界實證值）


def eff_h(kw):
    if kw.get("autofit") == "noAutofit":
        return kw["eh"]
    sz = kw.get("sz", 22) / 2.0
    pitch = sz * LH
    npar = kw.get("npar", 1)
    before = kw.get("before_pt", 0.0)
    pbdr = kw.get("pbdr")
    bd = 2 * ((pbdr[0] / 8.0) + pbdr[1]) if pbdr else 0.0
    return kw.get("tins", 3.6) + kw.get("bins", 3.6) + npar * pitch + (npar - 1) * before + bd


def predict(kw):
    posv, dist = kw["posv"], kw.get("dist", 7.2)
    clash = (LINE_BOTTOM + dist) if posv < LINE_BOTTOM + dist else 0.0
    return round(posv + eff_h(kw) + clash, 2)


BLIND = {  # 盲測四案（擬合後設計，未參與擬合）
    "pA_11pt_pbdr16_6_t5b10_posv15": dict(
        sz=22, italic=False, pbdr=(16, 6), tins=5.0, bins=10.0, posv=15.0,
        autofit="spAutoFit", wrap="topAndBottom", dist=7.2, ew=200, eh=999),
    "pB_2para_12it_pbdr_posv216": dict(
        sz=24, italic=True, pbdr=(24, 8), tins=3.6, bins=3.6, posv=21.6,
        autofit="spAutoFit", wrap="topAndBottom", dist=7.2, ew=273.6, eh=999,
        npar=2, before_pt=6.0),
    "pC_noautofit_eh40_posv30": dict(
        sz=22, italic=False, pbdr=(24, 8), autofit="noAutofit",
        wrap="topAndBottom", dist=7.2, posv=30.0, ew=273.6, eh=40,
        tins=3.6, bins=3.6),
    "pD_wrapSquare_11pt_nopbdr": dict(
        sz=22, italic=False, pbdr=None, autofit="spAutoFit",
        wrap="square", dist=7.2, posv=14.4, ew=185.9, eh=110.6,
        tins=3.6, bins=3.6),
}
PA = dict(BLIND["pA_11pt_pbdr16_6_t5b10_posv15"])
CLASH = {  # 碰撞崖判別（dist=0 界降 13.9）
    "y1_PA_dist0_posv15": {**PA, "dist": 0.0},
    "y2_PA_dist0_posv5": {**PA, "dist": 0.0, "posv": 5.0},
    "y3_noauto_posv10": {**PA, "autofit": "noAutofit", "eh": 40, "posv": 10},
    "y4_noauto_posv10_dist0": {**PA, "autofit": "noAutofit", "eh": 40,
                               "posv": 10, "dist": 0},
    "y5_noauto_posv216_dist0": {**PA, "autofit": "noAutofit", "eh": 40,
                                "posv": 21.6, "dist": 0},
}


def _anchor_multi(*, npar=1, before_pt=0.0, **kw):
    E = kp.pt2emu
    rpr = ("<w:rPr>" + ("<w:i/><w:iCs/>" if kw["italic"] else "")
           + f'<w:sz w:val="{kw["sz"]}"/></w:rPr>')
    ppr = ""
    if kw["pbdr"]:
        s_, sp = kw["pbdr"]
        ppr += (f'<w:pBdr><w:top w:val="single" w:sz="{s_}" w:space="{sp}" w:color="4472C4"/>'
                f'<w:bottom w:val="single" w:sz="{s_}" w:space="{sp}" w:color="4472C4"/></w:pBdr>')
    ppr += '<w:spacing w:after="0"/>'
    one = f'<w:p><w:pPr>{ppr}</w:pPr><w:r>{rpr}<w:t>Txt</w:t></w:r></w:p>'
    if before_pt:
        b = f'<w:spacing w:before="{int(before_pt * 20)}" w:after="0"/>'
        inner = one + f'<w:p><w:pPr>{b}</w:pPr><w:r>{rpr}<w:t>Txt2</w:t></w:r></w:p>'
    else:
        inner = one * npar
    txbx = f"<w:txbxContent>{inner}</w:txbxContent>"
    af = {"spAutoFit": "<a:spAutoFit/>", "noAutofit": "<a:noAutofit/>"}[kw["autofit"]]
    wrapel = {"topAndBottom": "<wp:wrapTopAndBottom/>",
              "square": '<wp:wrapSquare wrapText="bothSides"/>',
              "none": "<wp:wrapNone/>"}[kw["wrap"]]
    return (f'<w:r><w:rPr><w:noProof/></w:rPr><w:drawing>'
            f'<wp:anchor distT="{E(kw["dist"])}" distB="{E(kw["dist"])}" distL="114300" distR="114300" '
            f'simplePos="0" relativeHeight="251658240" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">'
            f'<wp:simplePos x="0" y="0"/>'
            f'<wp:positionH relativeFrom="column"><wp:align>center</wp:align></wp:positionH>'
            f'<wp:positionV relativeFrom="paragraph"><wp:posOffset>{E(kw["posv"])}</wp:posOffset></wp:positionV>'
            f'<wp:extent cx="{E(kw["ew"])}" cy="{E(kw["eh"])}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>'
            f'{wrapel}<wp:docPr id="1" name="TB"/>'
            f'<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">'
            f'<a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">'
            f'<wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr>'
            f'<a:xfrm><a:off x="0" y="0"/><a:ext cx="{E(kw["ew"])}" cy="{E(kw["eh"])}"/></a:xfrm>'
            f'<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>'
            f'<a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill>'
            f'<a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></wps:spPr>'
            f'<wps:txbx>{txbx}</wps:txbx>'
            f'<wps:bodyPr wrap="square" lIns="{E(7.2)}" tIns="{E(kw["tins"])}" '
            f'rIns="{E(7.2)}" bIns="{E(kw["bins"])}" anchor="t">{af}</wps:bodyPr>'
            f'</wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>')


def _make_with_anchor(path, anchor):
    c = lambda t, a="": ('<w:tc><w:tcPr><w:tcW w:w="3005" w:type="dxa"/></w:tcPr>'  # noqa: E731
                        f'<w:p><w:pPr><w:spacing w:after="0"/></w:pPr>{a}'
                        f'<w:r><w:t>This is a test.</w:t></w:r></w:p></w:tc>')
    row = lambda a1, a2, a3: f"<w:tr>{a1}{a2}{a3}</w:tr>"  # noqa: E731
    rows = (row(c("x"), c("x", anchor), c("x")) + row(c("x"), c("x"), c("x"))
            + row(c("x"), c("x"), c("x")))
    body = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
            f'<w:document {kp.NS}><w:body>'
            '<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/>'
            '<w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
            '<w:left w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
            '<w:bottom w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
            '<w:right w:val="single" w:sz="4" w:space="0" w:color="000000"/>'
            '<w:insideH w:val="single" w:sz="4" w:space="0" w:color="000000"/></w:tblBorders></w:tblPr>'
            '<w:tblGrid><w:gridCol w:w="3005"/><w:gridCol w:w="3005"/>'
            '<w:gridCol w:w="3006"/></w:tblGrid>'
            + rows + '</w:tbl><w:p/></w:body></w:document>')
    import zipfile
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", kp.CT)
        z.writestr("_rels/.rels", kp.RELS)
        z.writestr("word/_rels/document.xml.rels", kp.DOC_RELS)
        z.writestr("word/document.xml", body)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=str(pathlib.Path.home()
                                         / ".hermes/cache/scratch/k18/pred"))
    a = ap.parse_args()
    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    soffice = render_lab.find_soffice()
    scratch, prof, needle = render_lab.lo_scratch(soffice)
    render_lab.kill_stale_lo()
    import atexit
    atexit.register(render_lab.kill_own_lo, needle)
    rows = []
    print(f"{'case':36s} {'pred':>8s} {'LO':>8s} {'err':>7s}")
    for name, kw in {**BLIND, **CLASH}.items():
        p = out / f"{name}.docx"
        kw = dict(kw)
        npar = kw.pop("npar", 1)
        before = kw.pop("before_pt", 0.0)
        _make_with_anchor(p, _anchor_multi(npar=npar, before_pt=before, **kw))
        pred_v = predict({**kw, "npar": npar, "before_pt": before})
        pdf = render_lab.lo_to_pdf(soffice, p, scratch, prof, timeout=180)
        if pdf is None:
            rows.append({"case": name, "pred": pred_v, "lo": None})
            continue
        pages = kp.measure_pdf(pdf)
        tbl, _ = kp.tbl_lines_and_frame(pages)
        act = round(tbl[1] - tbl[0], 2) if len(tbl) >= 2 else None
        err = None if act is None else round(act - pred_v, 2)
        rows.append({"case": name, "pred": pred_v, "lo": act, "err": err})
        print(f"{name:36s} {pred_v:8.2f} {str(act):>8s} {str(err):>7s}")
    (out / "prediction.json").write_text(json.dumps(rows, indent=1))


if __name__ == "__main__":
    main()
