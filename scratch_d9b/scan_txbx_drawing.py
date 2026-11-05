"""找 corpus 內「v:textbox 內嵌 w:drawing 點陣圖」＋宣告 inset 的檔案（D9b 紅燈候選）。

口徑：v:shape 片段內同時有 <v:textbox ...inset=...> 與 <w:drawing>（且 drawing 內有 a:blip）
→ 該圖是 textbox 的**內容**（LO 會依文字區原點＝框＋inset 排版），非 imagedata 填色。
"""
import zipfile
import re
import glob

files = sorted(glob.glob('corpus/**/*.doc*', recursive=True))
print(f"scanned: {len(files)}")
for p in files:
    try:
        z = zipfile.ZipFile(p)
    except Exception:
        continue
    for n in z.namelist():
        if not (n.startswith('word/') and n.endswith('.xml')):
            continue
        try:
            data = z.read(n).decode('utf-8', 'ignore')
        except Exception:
            continue
        if '<v:textbox' not in data or '<w:drawing' not in data:
            continue
        for m in re.finditer(r'<v:shape\b.*?(?=<v:shape\b|<w:p\b|$)', data, re.S):
            frag = m.group(0)
            mi = re.search(r'<v:textbox([^>]*)>', frag)
            if not mi or '<w:drawing' not in frag:
                continue
            if 'a:blip' not in frag and 'v:imagedata' not in frag:
                continue
            ins = re.search(r'inset="([^"]*)"', mi.group(1))
            sid = re.search(r'(?<!o:)id="([^"]*)"', frag)
            style = re.search(r'style="([^"]*)"', frag)
            ext = re.findall(r'<wp:extent cx="(\d+)" cy="(\d+)"/>', frag)
            print(f"{p} :: {n} :: id={sid.group(1) if sid else '?'} "
                  f"inset={ins.group(1) if ins else None!r} "
                  f"blip={'a:blip' in frag} img={'v:imagedata' in frag} "
                  f"ext={[(int(a)/12700, int(b)/12700) for a, b in ext]} "
                  f"style={style.group(1)[:80] if style else ''!r}")
