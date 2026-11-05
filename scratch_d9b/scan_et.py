"""以 ElementTree 嚴謹掃描：v:shape（或 v:group/v:rect）子樹內同時含
   (a) v:textbox 的 @inset 宣告 與 (b) 圖片事實（v:imagedata 或 a:blip）。
   並記錄該框的 style（position/margin/width/height）、是否 floating（wp:anchor）、
   以及圖是 imagedata（填色）還是 w:drawing 內容（textbox 內容圖）。
"""
import zipfile
import glob
import sys
import xml.etree.ElementTree as ET

NS = {
    'v': 'urn:schemas-microsoft-com:vml',
    'w': 'http://schemas.openxmlformats.org/wordprocessingml/2006/main',
    'wp': 'http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing',
    'a': 'http://schemas.openxmlformats.org/drawingml/2006/main',
    'wps': 'http://schemas.microsoft.com/office/word/2010/wordprocessingShape',
    'r': 'http://schemas.openxmlformats.org/officeDocument/2006/relationships',
}

def local(tag):
    return tag.split('}')[-1]

def find_all_local(el, name):
    return [e for e in el.iter() if local(e.tag) == name]

files = sorted(glob.glob('corpus/**/*.doc*', recursive=True))
rows = []
for p in files:
    try:
        z = zipfile.ZipFile(p)
    except Exception:
        continue
    for n in z.namelist():
        if not (n.startswith('word/') and n.endswith('.xml')):
            continue
        try:
            root = ET.fromstring(z.read(n))
        except Exception:
            continue
        # 每個 v:shape / v:rect / v:group 當候選宿主
        for sh in root.iter():
            if local(sh.tag) not in ('shape', 'rect', 'group', 'roundrect', 'oval'):
                continue
            tbs = [e for e in sh.iter() if local(e.tag) == 'textbox']
            if not tbs:
                continue
            insets = [tb.get('inset') for tb in tbs if tb.get('inset')]
            if not insets:
                continue
            has_imgdata = any(local(e.tag) == 'imagedata' for e in sh.iter())
            has_blip = any(local(e.tag) == 'blip' for e in sh.iter())
            has_drawing = any(local(e.tag) == 'drawing' for e in sh.iter())
            style = sh.get('style') or ''
            sid = sh.get('id') or sh.get('{urn:schemas-microsoft-com:office:office}shapeid')
            if not (has_imgdata or has_blip):
                continue
            rows.append((p, n, sid, insets[0], style[:110],
                         f"imgdata={has_imgdata} blip={has_blip} drawing={has_drawing}"))

print(f"hits={len(rows)}")
for r in rows:
    print(f"{r[0]} :: {r[1]} :: id={r[2]} inset={r[3]!r} {r[5]}\n    style={r[4]!r}")
