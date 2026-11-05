"""D9b 根植三探：
A. n792778 群組 s1125 是否 floating＋其 txbx_inset 採集值（0,0,0,0 宣告能否落到 txbx_inset 欄位）
B. fdo76591 s1026 txbxContent 末段是否就是 '11-0163-23-2011'（決定第 3 條能否改「末段」結構判定）
C. tbrl-frame-vml（ooxmlexport）LO 是否渲染其框內圖（自訂 inset .72pt/7.2pt 的第二紅燈候選）
"""
import re
import zipfile
import subprocess
import os
import fitz

# --- B: s1026 txbxContent 段落清單
z = zipfile.ZipFile('corpus/ooxmlexport/fdo76591.docx')
raw = z.read('word/document.xml').decode('utf-8')
i = raw.find('_x0000_s1026')
tb = raw.find('<w:txbxContent', i)
tb_end = raw.find('</w:txbxContent>', tb)
body = raw[tb:tb_end]
paras = re.findall(r'<w:p\b.*?(?:</w:p>|/>)', body, re.S)
texts = []
for p_ in paras:
    t = ''.join(re.findall(r'<w:t(?: [^>]*)?>([^<]*)</w:t>', p_))
    texts.append(t)
print(f"B) s1026 txbxContent 段數={len(paras)}")
for k in range(max(0, len(texts) - 6), len(texts)):
    print(f"   末段[{k}]={texts[k]!r}")
hit = [k for k, t in enumerate(texts) if '11-016' in t]
print(f"   含 '11-016' 的段索引={hit}（末段索引={len(texts)-1}）")

# --- C: tbrl-frame-vml LO 真值
LO = os.path.expanduser("~/.local/lo-linux/opt/libreoffice25.8/program/soffice")
for rel in ('corpus/ooxmlexport/tbrl-frame-vml.docx', 'corpus/ooxmlimport/btlr-frame-vml.docx'):
    stem = os.path.basename(rel).replace('.docx', '')
    out = '/tmp/d9b_truth'
    os.makedirs(out, exist_ok=True)
    pdf = f"{out}/{stem}.pdf"
    if not os.path.exists(pdf):
        subprocess.run([LO, '--headless', '--norestore',
                        f'-env:UserInstallation=file:///tmp/d9b_prof_{stem}',
                        '--convert-to', 'pdf', '--outdir', out, rel],
                       capture_output=True, timeout=300)
    d = fitz.open(pdf)
    print(f"C) {rel} pages={len(d)}")
    for pno in range(len(d)):
        pg = d[pno]
        for img in pg.get_images(full=True):
            for r in pg.get_image_rects(img[0]):
                print(f"   p{pno} IMG [{r.x0:.2f},{r.y0:.2f},{r.x1:.2f},{r.y1:.2f}] "
                      f"w={r.width:.2f} h={r.height:.2f}")
        tb2 = pg.get_text('dict')
        for b in tb2['blocks'][:6]:
            if b['type'] == 0:
                for ln in b['lines'][:4]:
                    txt = ''.join(s['text'] for s in ln['spans'])
                    print(f"   p{pno} TXT {txt!r} dir={ln['dir']} bbox="
                          f"[{ln['bbox'][0]:.2f},{ln['bbox'][1]:.2f},{ln['bbox'][2]:.2f},{ln['bbox'][3]:.2f}]")
    d.close()
