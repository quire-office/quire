"""LO 真值批量抽取（圖表語料）：走 render_lab 已驗證的 soffice.exe 口徑（Windows 路徑＋獨立 profile）。"""
import json
import os
import pathlib
import subprocess
import sys
import tempfile

import pymupdf

SOFFICE = '/mnt/c/Program Files/LibreOffice/program/soffice.exe'
REPO = pathlib.Path('.').resolve()


def w2w(lp):
    r = subprocess.run(['wslpath', '-w', str(lp)], capture_output=True, text=True)
    return r.stdout.strip()


def convert(docx, scratch, profile_uri):
    src = scratch / (docx.parent.name + '__' + docx.name)
    subprocess.run(['cp', str(docx), str(src)], check=True)
    out_arg = w2w(scratch)
    src_arg = w2w(src)
    stale = scratch / (src.stem + '.pdf')
    if stale.exists():
        stale.unlink()
    subprocess.run([SOFFICE, '-env:UserInstallation=' + profile_uri, '--headless', '--norestore',
                    '--convert-to', 'pdf', '--outdir', out_arg, src_arg],
                   capture_output=True, timeout=180)
    return stale if stale.exists() else None


def lines_of(pdf):
    d = pymupdf.open(pdf)
    out = []
    for pno, page in enumerate(d):
        for blk in page.get_text('dict')['blocks']:
            if blk.get('type') != 0:
                continue
            for ln in blk['lines']:
                txt = ''.join(sp['text'] for sp in ln['spans']).strip()
                if not txt:
                    continue
                sp0 = ln['spans'][0]
                out.append({'p': pno, 't': txt,
                            'dir': [round(ln['dir'][0], 2), round(ln['dir'][1], 2)],
                            'bbox': [round(v, 2) for v in ln['bbox']],
                            'size': round(sp0['size'], 2), 'font': sp0.get('font'),
                            'color': sp0.get('color')})
        d2 = pymupdf.open(pdf)
    return out


def drawings_of(pdf):
    d = pymupdf.open(pdf)
    out = []
    for pno, page in enumerate(d):
        for dr in page.get_drawings():
            items = []
            for it in dr['items']:
                if it[0] == 're':
                    items.append(['re', [round(v, 2) for v in it[1]]])
                elif it[0] == 'l':
                    items.append(['l', [round(it[1].x, 2), round(it[1].y, 2),
                                        round(it[2].x, 2), round(it[2].y, 2)]])
            out.append({'p': pno, 'type': dr['type'], 'fill': dr.get('fill'),
                        'color': dr.get('color'), 'width': dr.get('width'),
                        'rect': [round(v, 2) for v in dr['rect']], 'items': items})
    return out


def main():
    files = [l.strip() for l in pathlib.Path(sys.argv[1]).read_text().splitlines() if l.strip()]
    out_root = pathlib.Path(sys.argv[2])
    out_root.mkdir(parents=True, exist_ok=True)
    scratch = pathlib.Path('/mnt/c/tmp/f1lo')
    scratch.mkdir(parents=True, exist_ok=True)
    profile = 'file:///C:/tmp/f1lo/prof'
    res = {}
    for f in files:
        p = (REPO / f)
        pdf = convert(p, scratch, profile)
        if pdf is None:
            res[p.name] = 'LO_FAIL'
            print('LO_FAIL', f, flush=True)
            continue
        res[p.name] = {'path': f, 'lines': lines_of(pdf), 'drawings': drawings_of(pdf)}
        print('OK', p.name, len(res[p.name]['lines']), 'lines', flush=True)
    (out_root / 'lo_chart_truth.json').write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding='utf-8')
    print('DONE')


main()
