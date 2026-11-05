"""二盞燈候選確認：CoverPage Text Box 465 與 n792778——是否「textbox 內嵌 w:drawing 圖」
   ＋宿主框自訂 inset（非預設）。並列我方 base-bin 目前發射座標。"""
import zipfile
import re
import subprocess
import xml.etree.ElementTree as ET

def local(tag):
    return tag.split('}')[-1]

def probe(rel, ids):
    z = zipfile.ZipFile(rel)
    raw = z.read('word/document.xml').decode('utf-8')
    root = ET.fromstring(raw)
    print(f"===== {rel}")
    for sh in root.iter():
        if local(sh.tag) not in ('shape', 'group'):
            continue
        sid = sh.get('id')
        if sid not in ids:
            continue
        print(f"--- {local(sh.tag)} id={sid!r}")
        print(f"    style={sh.get('style')!r}")
        for k, v in sh.attrib.items():
            if local(k) in ('coordorigin', 'coordsize', 'filled', 'stroked', 'strokeweight'):
                print(f"    {local(k)}={v!r}")
        for d in sh.iter():
            if local(d.tag) == 'textbox':
                print(f"    textbox inset={d.get('inset')!r} style={d.get('style')!r}")
            elif local(d.tag) == 'blip':
                print(f"    blip embed={dict((local(k), v) for k, v in d.attrib.items())}")
            elif local(d.tag) == 'imagedata':
                print(f"    imagedata {dict((local(k), (v[:30] if isinstance(v,str) else v)) for k, v in d.attrib.items())}")
            elif local(d.tag) == 'extent' and sh.get('id') in ids:
                print(f"    wp:extent {dict((local(k), v) for k, v in d.attrib.items())}")
    p = subprocess.run(['scratch_d9b/base-bin/quire-cli', 'dump', rel],
                       capture_output=True, text=True, timeout=240)
    import json
    try:
        d = json.loads(p.stdout)
    except Exception:
        print("   dump fail", p.stderr[:150]); return
    pages = d['pages'] if isinstance(d, dict) and 'pages' in d else d
    for pi, pg in enumerate(pages):
        items = pg['items'] if isinstance(pg, dict) and 'items' in pg else pg
        for it in items:
            k = list(it)[0]
            if k == 'Image':
                v = it[k]
                print(f"   OURS p{pi} Image x={v['x']:.2f} y={v['y']:.2f} w={v['w']:.2f} h={v['h']:.2f}")

probe('corpus/xopt/NewDocxDocuments_CoverPage.docx', {'Text Box 465'})
probe('corpus/ooxmlexport/n792778.docx', {'_x0000_s1125', '_x0000_s1127', '_x0000_s1130'})
