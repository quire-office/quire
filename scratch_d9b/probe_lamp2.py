"""第二盞燈候選深查：fdo78432 / mce-nested / dkvert / tdf103975——
   textbox 內是否 w:drawing 內容圖？宿主框 floating？我方目前發射？"""
import zipfile
import subprocess
import xml.etree.ElementTree as ET

def local(tag):
    return tag.split('}')[-1]

TARGETS = [
    ('corpus/ooxmlexport/fdo78432.docx', None),
    ('corpus/ooxmlexport/mce-nested.docx', None),
    ('corpus/ooxmlexport/dkvert.docx', None),
    ('corpus/ooxmlexport/tdf103975_notPageBreakD.docx', None),
]
for rel, _ in TARGETS:
    z = zipfile.ZipFile(rel)
    print(f"===== {rel}")
    for n in z.namelist():
        if not (n.startswith('word/') and n.endswith('.xml')):
            continue
        raw = z.read(n).decode('utf-8', 'ignore')
        if '<v:textbox' not in raw:
            continue
        try:
            root = ET.fromstring(raw)
        except Exception:
            continue
        for sh in root.iter():
            if local(sh.tag) not in ('shape', 'rect', 'roundrect', 'group', 'oval'):
                continue
            tbs = [d for d in sh.iter() if local(d.tag) == 'textbox']
            if not tbs:
                continue
            ins = next((t.get('inset') for t in tbs if t.get('inset')), None)
            blips = [d for d in sh.iter() if local(d.tag) == 'blip']
            imgs = [d for d in sh.iter() if local(d.tag) == 'imagedata']
            drawings = [d for d in sh.iter() if local(d.tag) == 'drawing']
            exts = [d for d in sh.iter() if local(d.tag) == 'extent' and d.get('cx')]
            print(f"  [{n}] {local(sh.tag)} id={sh.get('id')!r} inset={ins!r} "
                  f"blip={len(blips)} imagedata={len(imgs)} drawing={len(drawings)} "
                  f"ext={[(int(d.get('cx'))/12700, int(d.get('cy'))/12700) for d in exts]}")
            print(f"      style={(sh.get('style') or '')[:120]!r}")
    p = subprocess.run(['scratch_d9b/base-bin/quire-cli', 'dump', rel],
                       capture_output=True, text=True, timeout=240)
    import json
    try:
        d = json.loads(p.stdout)
        pages = d['pages'] if isinstance(d, dict) and 'pages' in d else d
        for pi, pg in enumerate(pages):
            items = pg['items'] if isinstance(pg, dict) and 'items' in pg else pg
            for it in items:
                k = list(it)[0]
                if k in ('Image', 'Rect'):
                    v = it[k]
                    print(f"   OURS p{pi} {k} x={v['x']:.2f} y={v['y']:.2f} "
                          f"w={v['w']:.2f} h={v['h']:.2f}")
    except Exception as e:
        print("   dump fail", e)
