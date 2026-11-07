# F2 SVG 級零位元組 A/B：正常態 vs F2_GEOM_BUGGY=1（回塞態＝新分支整段關閉＝修前路徑）
# 卡面鐵條「無 WordArt／無效果檔案的 SVG 必須零位元組差異」的 SVG 層驗證
# （燈5 是顯示列表 JSON 層，本腳本是出貨 SVG 層的第二把鎖）。
import hashlib
import os
import subprocess
import sys

ROOT = os.path.dirname(__file__).rsplit("/docs", 1)[0] + "/"
CLI = ROOT + "target/debug/quire-cli"
FILES = [
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

def render_sha(rel, tag, env=None):
    stem = os.path.splitext(os.path.basename(rel))[0]
    outdir = f"/tmp/f2_ab_{tag}"
    subprocess.run(["rm", "-rf", outdir])
    os.makedirs(outdir, exist_ok=True)
    e = dict(os.environ)
    if env:
        e[env] = "1"
    subprocess.run([CLI, "render", ROOT + rel, "--out", outdir], capture_output=True, env=e)
    out = {}
    for f in sorted(os.listdir(outdir)):
        if f.startswith(stem) and f.endswith(".svg"):
            out[f] = hashlib.sha256(open(os.path.join(outdir, f), "rb").read()).hexdigest()[:16]
    return out

bad = 0
for rel in FILES:
    a = render_sha(rel, "normal")
    b = render_sha(rel, "buggy", "F2_GEOM_BUGGY")
    ok = a == b and a != {}
    if not ok:
        bad += 1
    print(("OK  " if ok else "DIFF"), rel, a if ok else f"{a} != {b}")
print("BAD =", bad)
sys.exit(1 if bad else 0)
