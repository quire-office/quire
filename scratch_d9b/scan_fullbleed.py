"""掃 corpus：以「各檔自身 pgMar」定義滿版框（覆蓋版心且跨滿版心左右），
   找出目前被 22.05 字面量漏判的檔案。

口徑（無單一檔案常量）：
  版心左 = pgMar/@left，版心右界 = pgSz/@w - pgMar/@right（twips→pt /20）
  滿版候選 = VML shape style position:absolute 且
             x ≤ 版心左 且 x+w ≥ 版心右 且 h ≥ (pgSz@h - top - bottom)
"""
import zipfile
import glob
import re
import xml.etree.ElementTree as ET

def local(tag):
    return tag.split('}')[-1]

def attr(el, name):
    for k, v in el.attrib.items():
        if k.split('}')[-1] == name:
            return v
    return None

def style_map(style):
    kv = {}
    for d in (style or '').split(';'):
        if ':' in d:
            k, v = d.split(':', 1)
            kv[k.strip().lower()] = v.strip()
    return kv

def pt(v):
    if v is None:
        return None
    v = v.strip()
    if v.endswith('pt'):
        return float(v[:-2])
    if v.endswith('mm'):
        return float(v[:-2]) * 72 / 25.4
    if v.endswith('cm'):
        return float(v[:-2]) * 72 / 2.54
    if v.endswith('in'):
        return float(v[:-2]) * 72
    if v.endswith('%'):
        return None
    try:
        return float(v)  # 無單位：VML 預設 pt
    except ValueError:
        return None

W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
files = sorted(glob.glob('corpus/**/*.doc*', recursive=True))
rows = []
for p in files:
    try:
        z = zipfile.ZipFile(p)
        root = ET.fromstring(z.read('word/document.xml'))
    except Exception:
        continue
    # 第一節 pgSz/pgMar（iter 是前序：sectPr 先於其子，必須在子層抓）
    pgw = pgh = ml = mr = mt = mb = None
    tw = lambda v: (pt(v) if v and not v.strip().isdigit() else (int(v) / 20 if v else None))
    for el in root.iter():
        if local(el.tag) == 'pgSz' and pgw is None:
            pgw = tw(attr(el, 'w'))
            pgh = tw(attr(el, 'h'))
        elif local(el.tag) == 'pgMar' and ml is None:
            ml = tw(attr(el, 'left'))
            mr = tw(attr(el, 'right'))
            mt = tw(attr(el, 'top'))
            mb = tw(attr(el, 'bottom'))
    if not pgw or ml is None:
        continue
    text_l, text_r, text_h = ml, pgw - mr, pgh - mt - mb
    for n in z.namelist():
        if not (n.startswith('word/') and n.endswith('.xml')):
            continue
        try:
            r2 = ET.fromstring(z.read(n))
        except Exception:
            continue
        for sh in r2.iter():
            if local(sh.tag) not in ('shape', 'rect', 'group', 'roundrect'):
                continue
            sm = style_map(sh.get('style'))
            if sm.get('position') != 'absolute':
                continue
            w = pt(sm.get('width'))
            h = pt(sm.get('height'))
            if not w or not h:
                continue
            x = pt(sm.get('left')) if sm.get('left') else pt(sm.get('margin-left'))
            y = pt(sm.get('top')) if sm.get('top') else pt(sm.get('margin-top'))
            if x is None or y is None:
                continue
            # relativeFrom（VML 以 mso-position-*-relative；缺省＝margin）
            hx = sm.get('mso-position-horizontal-relative', 'margin')
            vy = sm.get('mso-position-vertical-relative', 'margin')
            ax = x if hx in ('page', 'pageMargin') else ml + x
            ay = y if vy in ('page', 'pageMargin') else mt + y
            full = (ax <= text_l + 0.5) and (ax + w >= text_r - 0.5) and (h >= text_h - 2.0)
            if not full:
                continue
            tbinset = None
            for tb in sh.iter():
                if local(tb.tag) == 'textbox' and tb.get('inset'):
                    tbinset = tb.get('inset')
                    break
            stroke = sh.get('strokeweight')
            rows.append((p, n, sh.get('id'), f"pg={pgw:.0f}x{pgh:.0f} ml={ml} mr={mr} mt={mt} mb={mb}",
                         f"abs=({ax:.2f},{ay:.2f}) w={w} h={h}", f"inset={tbinset!r} stroke={stroke!r}"))

print(f"full-bleed candidates (pgMar-relative): {len(rows)}")
for r in rows:
    print(f"{r[0]} :: {r[1]} :: id={r[2]}\n    {r[3]}\n    {r[4]} {r[5]}")
