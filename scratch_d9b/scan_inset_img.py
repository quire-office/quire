"""掃 corpus：找「浮動 VML/DML 框＋v:textbox/@inset 宣告＋框內圖片」的檔案（D9b 紅燈候選）。

判定口徑：
  A. word/*.xml 內同一 <v:shape> 子-tree 同時有 <v:textbox inset=...> 與 <v:imagedata>
  B. 或 <wps:txbx>/<w:txbxContent> 內含 <w:drawing>（DML 圖入框）且同 shape 有 VML textbox inset
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
        if 'imagedata' not in data and '<w:drawing' not in data:
            continue
        # 抓每個 v:shape（含巢狀最外層）片段：以 <v:shape ...> 到 </v:shape> 粗切（非嚴格 XML，夠用）
        for m in re.finditer(r'<v:shape\b.*?(?=<v:shape\b|</v:group>|$)', data, re.S):
            frag = m.group(0)
            mi = re.search(r'<v:textbox[^>]*inset="([^"]+)"', frag)
            if not mi:
                continue
            has_img = '<v:imagedata' in frag or '<w:drawing' in frag or 'v:imagedata' in frag
            style = re.search(r'style="([^"]*)"', frag)
            sid = re.search(r'(?<!o:)id="([^"]*)"', frag)
            if has_img:
                print(f"{p} :: {n} :: shape id={sid.group(1) if sid else '?'} "
                      f"inset={mi.group(1)!r} style={style.group(1)[:90] if style else ''!r}")
