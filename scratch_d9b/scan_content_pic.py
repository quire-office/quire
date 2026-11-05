"""定論掃描：v:shape（宿主框）子樹中，圖片是否為 **v:textbox/w:txbxContent 內容圖**
   （路徑 shape > textbox > txbxContent > ... > (w:drawing w/blip | w:pict v:imagedata)），
   並記錄宿主框宣告的 v:textbox/@inset、框 style、是否 floating（wp:anchor）。

這是 D9b 第 1 條紅燈考卷的選檔依據：需要「框內圖＋自訂 inset 宣告」的實檔。
"""
import zipfile
import glob
import xml.etree.ElementTree as ET

def local(tag):
    return tag.split('}')[-1]

def attr(el, name):
    for k, v in el.attrib.items():
        if k.split('}')[-1] == name:
            return v
    return None

def has_content_picture(el):
    """回傳 (imagedata?, blip?)：圖片事實是否位於 txbxContent 之下。"""
    for tb in el.iter():
        if local(tb.tag) != 'textbox':
            continue
        for txc in tb:
            if local(txc.tag) != 'txbxContent':
                continue
            for d in txc.iter():
                if local(d.tag) == 'imagedata':
                    return True, True
                if local(d.tag) == 'blip':
                    return True, False
    return False, False

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
        for sh in root.iter():
            if local(sh.tag) not in ('shape', 'group', 'rect', 'roundrect', 'oval'):
                continue
            ins = None
            for tb in sh.iter():
                if local(tb.tag) == 'textbox' and tb.get('inset') is not None:
                    ins = tb.get('inset')
                    break
            content, is_blip = has_content_picture(sh)
            if not content:
                continue
            sid = sh.get('id')
            style = (sh.get('style') or '')
            rows.append((p, n, sid, ins, is_blip, style))

print(f"textbox-content-picture hosts: {len(rows)}")
for p, n, sid, ins, is_blip, style in rows:
    print(f"{p} :: {n} :: id={sid!r} inset={ins!r} blip={is_blip}\n    style={style[:120]!r}")
