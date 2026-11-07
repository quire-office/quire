# F2 零回退核對：基準 f2_baseline_shapes.json（修前 sha/計數）vs 修後實抓
# 口徑同 f2v2_ourdump.py：quire-cli dump 逐頁計 DrawItem 型別 + SVG sha256 前16碼
import hashlib
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(__file__).rsplit("/docs", 1)[0] + "/"
CLI = ROOT + "target/debug/quire-cli"
BASE = json.load(open(ROOT + "docs/f2-survey/f2_baseline_shapes.json"))

# 卡面零回退點名檔（無非 rect prst 者必須零位元組差異）
GATE_FILES = [
    "corpus/ooxmlexport/artistic_effects.docx",
    "corpus/ooxmlexport/fdo76591.docx",
    "corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx",
    "corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell-Mod.docx",
    "corpus/ooxmlexport/btlr-textbox.docx",
    "corpus/ooxmlexport/99_Fields.docx",
    "corpus/ooxmlexport/cell-grid-span.docx",
    "corpus/xopt/NewDocxDocuments_CoverPage.docx",
    "corpus/ooxmlimport/n751077.docx",
    "corpus/ooxmlexport/fdo80897.docx",
]

def dump_kinds(rel):
    out = subprocess.run([CLI, "dump", ROOT + rel], capture_output=True, text=True).stdout
    d = json.loads(out)
    pages = d if isinstance(d, list) else d.get("pages", [])
    res = []
    for pg in pages:
        kinds = {}
        for it in pg.get("items", []):
            k = list(it.keys())[0]
            kinds[k] = kinds.get(k, 0) + 1
        res.append([pg.get("page"), kinds])
    return res

def svg_shas(rel, tag):
    stem = os.path.splitext(os.path.basename(rel))[0]
    outdir = f"/tmp/f2_svg_{tag}"
    os.makedirs(outdir, exist_ok=True)
    subprocess.run([CLI, "render", ROOT + rel, "--out", outdir], capture_output=True)
    shas = []
    for f in sorted(os.listdir(outdir)):
        if f.startswith(stem) and f.endswith(".svg"):
            shas.append([f, hashlib.sha256(open(os.path.join(outdir, f), "rb").read()).hexdigest()[:16]])
    return shas

bad = 0
for rel in GATE_FILES:
    kinds = dump_kinds(rel)
    # 基準檔在 baseline JSON 內者直接比計數
    stem = os.path.basename(rel)
    base = BASE.get(rel)
    status = "無基準(只報)"
    if base:
        same = base.get("pages") == kinds
        status = "基準一致" if same else "基準漂移!!"
        if not same:
            bad += 1
            print("DIFF", rel, "\n  base", base.get("pages"), "\n  now ", kinds)
    print(f"{rel:70s} {status:12s}", json.dumps(kinds, ensure_ascii=False))

print("=== SVG sha A/B vs 修前基準（f2_baseline_shapes.json svg 欄，逐檔判定）===")
# 基線檔分兩類：rect-only（必須零位元組差異）與含非 rect prst（本卡刻意改）。
NONRECT = {
    "corpus/ooxmlexport/fdo74605.docx",
    "corpus/ooxmlexport/tdf113258.docx",
    "corpus/ooxmlexport/tdf113183.docx",
    "corpus/poi/test-data_document_shapes-with-text.docx",
}
for rel, v in BASE.items():
    if not v.get("svg"):
        continue
    want = dict(v["svg"])
    got = dict(svg_shas(rel, "now"))
    same = want == got and bool(got)
    if rel in NONRECT:
        print(("刻意改變 OK" if not same else "意外未變 DIFF!!"), rel)
        if same:
            bad += 1
    else:
        print(("OK  " if same else "DIFF!!"), rel, "" if same else f"base={want} now={got}")
        if not same:
            bad += 1
print("BAD =", bad)
sys.exit(1 if bad else 0)
