#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ODF-C4 頁面幾何計數基準腳本（C3 `odt_projection_baseline.py` 同款「第二雙眼」）。

獨立於 Rust 實作，直接從 ODT 的 styles.xml/content.xml 抓 `style:page-layout`
＋`style:master-page` 鏈，換算成 pt，並與 LO `--convert-to pdf` 的
pymupdf `page.rect` 真值（本腳本 `--truth` 讀 index→pdf 表）逐檔對照。

口徑（與 Rust `core/odf/src/page_geometry.rs` 實裝一致，見解剖報告）：
- 主頁鏈：文件首流程物件（頂層 text:p/text:h/table:table）@text:style-name →
  （沿 @style:parent-style-name 鏈）@style:master-page-name →
  缺省 @style:name="Standard"（無則 <office:master-styles> 首個）→
  @style:page-layout-name → styles.xml office:automatic-styles 的
  <style:page-layout>。
- 紙張：@style:paper-width/height 優先，@fo:page-width/height 後備（語料實態
  全走 fo: 軸——style: 軸 0 檔，解剖報告 §2）。
- 邊距：<style:page-margin> 子元素優先，fo:margin-* 後備（語料全走 fo:margin）。
- 單位：cm/mm/in/pt/pc；小數點（容錯本地化逗號）。

用法：
    python3 core/odf/tests/odf_c4_truth_baseline.py [--lo-index PATH] [--json OUT]
輸出：人類摘要＋（--json 時）逐檔 record（供紅燈報告引用）。
"""
from __future__ import annotations

import argparse, json, pathlib, re, sys, zipfile
import xml.etree.ElementTree as ET

REPO = pathlib.Path(__file__).resolve().parents[3]

UNIT = {"cm": 72.0 / 2.54, "mm": 72.0 / 25.4, "in": 72.0, "pt": 1.0, "pc": 12.0}
LEN_RE = re.compile(r"^([+-]?[0-9]+(?:\.[0-9]+)?)\s*(cm|mm|in|pt|pc)$")


def to_pt(v):
    if not v:
        return None
    m = LEN_RE.match(v.strip().replace(",", "."))
    if not m:
        return None
    x = float(m.group(1))
    return x * UNIT[m.group(2)] if x > 0 else None


def local(t):
    return t.rsplit("}", 1)[-1] if "}" in t else t


def parse(path: pathlib.Path):
    with zipfile.ZipFile(path) as z:
        names = z.namelist()
        styles = z.read("styles.xml") if "styles.xml" in names else b""
        content = z.read("content.xml") if "content.xml" in names else b""
    dom = {"layouts": {}, "masters": [], "first_master": None,
           "mp_content": {}, "mp_styles": {}, "parent": {}}
    if styles:
        root = ET.fromstring(styles)
        for el in root.iter():
            t = local(el.tag)
            a = {local(k): v for k, v in el.attrib.items()}
            if t == "page-layout":
                rec = {"name": a.get("name", ""), "w": None, "h": None,
                       "m": [None] * 4, "from_style_axis": False, "from_pm_el": False}
                for ch in el:
                    ct = local(ch.tag)
                    ca = {local(k): v for k, v in ch.attrib.items()}
                    if ct == "page-layout-properties":
                        sw, sh = to_pt(ca.get("paper-width")), to_pt(ca.get("paper-height"))
                        fw, fh = to_pt(ca.get("page-width")), to_pt(ca.get("page-height"))
                        w = sw if sw else fw
                        h = sh if sh else fh
                        rec["w"], rec["h"] = w, h
                        rec["from_style_axis"] = bool(sw)
                        for i, side in enumerate(("top", "right", "bottom", "left")):
                            rec["m"][i] = to_pt(ca.get(f"margin-{side}"))
                    elif ct == "page-margin":
                        rec["from_pm_el"] = True
                        for i, side in enumerate(("top", "right", "bottom", "left")):
                            v = to_pt(ca.get(f"margin-{side}") or ca.get(side))
                            if v is not None:
                                rec["m"][i] = v
                if rec["w"] and rec["h"] and rec["name"]:
                    dom["layouts"][rec["name"]] = rec
            elif t == "master-page":
                dom["masters"].append({"name": a.get("name", ""),
                                       "pl": a.get("page-layout-name") or None})
                if dom["first_master"] is None:
                    dom["first_master"] = a.get("name")
            elif t == "style":
                n = a.get("name")
                if n and a.get("master-page-name"):
                    dom["mp_styles"][n] = a["master-page-name"]
                if n and a.get("parent-style-name"):
                    dom["parent"].setdefault(n, a["parent-style-name"])
    # content.xml：automatic-styles 域的 master-page-name ＋ parent ＋ 首流程物件樣式
    root = ET.fromstring(content)
    depth_aut = 0
    first_style = ""
    # 用 iter 無法追蹤深度——手動走 tree
    def walk(el, nested):
        nonlocal first_style
        for ch in el:
            t = local(ch.tag)
            a = {local(k): v for k, v in ch.attrib.items()}
            if t == "automatic-styles":  # local() 已去前綴
                for s in ch:
                    sa = {local(k): v for k, v in s.attrib.items()}
                    n = sa.get("name")
                    if local(s.tag) == "style" and n:
                        if sa.get("master-page-name"):
                            dom["mp_content"][n] = sa["master-page-name"]
                        if sa.get("parent-style-name"):
                            dom["parent"].setdefault(n, sa["parent-style-name"])
                continue
            if t in ("p", "h", "table") and not nested and not first_style:  # local() 去前綴
                first_style = a.get("style-name") or ""
            walk(ch, nested or t in ("section", "table"))
    walk(root, False)

    def mp_of(style, table):
        cur, seen = style, set()
        while cur and cur not in seen and len(seen) < 8:
            seen.add(cur)
            if cur in table:
                return table[cur]
            cur = dom["parent"].get(cur, "")
        return None

    mp = mp_of(first_style, dom["mp_content"]) or mp_of(first_style, dom["mp_styles"])
    source = "para_master_page_name" if mp else "default_master_styles"
    if not mp:
        if any(m["name"] == "Standard" for m in dom["masters"]):
            mp = "Standard"
        elif dom["first_master"]:
            mp = dom["first_master"]
        elif dom["masters"]:
            mp = dom["masters"][0]["name"]
    layout = None
    if mp:
        m = next((x for x in dom["masters"] if x["name"] == mp), None)
        if m and m["pl"] and m["pl"] in dom["layouts"]:
            layout = dom["layouts"][m["pl"]]
        else:
            source = "master_page_without_layout" if m else "no_master_page"
    else:
        source = "no_master_page"
    LO_W, LO_H, LO_M = 21.001 * UNIT["cm"], 29.7 * UNIT["cm"], 2.0 * UNIT["cm"]
    w = layout["w"] if layout else LO_W
    h = layout["h"] if layout else LO_H
    mg = layout["m"] if (layout and all(v is not None for v in layout["m"])) else [LO_M] * 4
    return {"file": str(path.relative_to(REPO)), "first_style": first_style,
            "master_page": mp, "page_layout": layout["name"] if layout else None,
            "source": source,
            "page_w_pt": round(w, 4), "page_h_pt": round(h, 4),
            "margins_pt": [round(v, 4) for v in mg],
            "size_from_style_axis": bool(layout and layout["from_style_axis"]),
            "margins_from_pm_el": bool(layout and layout["from_pm_el"])}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", default=str(REPO / "corpus-odt"))
    ap.add_argument("--lo-index", default=None, help="c4_lo_pdf/index.json（LO PDF 路徑表）")
    ap.add_argument("--json", default=None)
    ap.add_argument("--files", default=None, help="逗號分隔的相對路徑覆蓋預設全語料")
    a = ap.parse_args()
    corpus = pathlib.Path(a.corpus)
    if a.files:
        files = [(REPO / f.strip()) for f in a.files.split(",")]
    else:
        manifest = json.loads((corpus / "manifest.json").read_text(encoding="utf-8"))
        files = [corpus / r["file"] for r in manifest["files"]]

    truth = {}
    if a.lo_index:
        idx = json.loads(pathlib.Path(a.lo_index).read_text(encoding="utf-8"))
        try:
            import pymupdf
        except ImportError:
            pymupdf = None
        for rel, rec in idx.items():
            if pymupdf and rec.get("pdf") and (REPO / rel).exists():
                d = pymupdf.open(rec["pdf"])
                p = d[0]
                truth[rel] = {"w": round(p.rect.width, 3), "h": round(p.rect.height, 3)}
                d.close()

    rows = []
    for p in files:
        rel = str(p.relative_to(REPO))
        try:
            r = parse(p)
        except Exception as e:
            rows.append({"file": rel, "error": str(e)[:120]})
            continue
        t = truth.get(rel)
        if t:
            r["lo_w_pt"], r["lo_h_pt"] = t["w"], t["h"]
            r["delta_w"] = round(r["page_w_pt"] - t["w"], 3)
            r["delta_h"] = round(r["page_h_pt"] - t["h"], 3)
        rows.append(r)
    bad = [r for r in rows if r.get("delta_w") is not None
           and (abs(r["delta_w"]) > 0.6 or abs(r["delta_h"]) > 0.6)]
    for r in rows:
        print(json.dumps(r, ensure_ascii=False))
    print(f"# {len(rows)} 檔；LO 頁尺寸對照偏差>0.6pt 者 {len(bad)} 檔", file=sys.stderr)
    for r in bad:
        print("DIVERGE", json.dumps(r, ensure_ascii=False), file=sys.stderr)
    if a.json:
        pathlib.Path(a.json).write_text(json.dumps(rows, ensure_ascii=False, indent=1),
                                        encoding="utf-8")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
