"""S5：LO-only 的 o×4 / 1×2 究竟出自哪些行——逐行對帳我方段落。"""
import json, pathlib, re, subprocess
from collections import Counter
import pymupdf

SCRATCH = pathlib.Path("<hermes>/profiles/ow-qa/cache/scratch/e2_odt")
REPO = pathlib.Path("<repo>/.worktrees/t_72dc8bc8")
WS = re.compile(r"\s+")
rel = "corpus-odt/lo_out/lo__ooxmlexport__tdf124637_sectionMargin.odt"
rows = json.loads(subprocess.run([str(SCRATCH / "probe_target/release/odtflow"), str(REPO / rel)],
                                 capture_output=True, text=True).stdout)["paragraphs"]
vis = WS.sub("", "".join(p["text"] for p in rows))
pdf = sorted((SCRATCH / "lo_pdf" / pathlib.Path(rel).name).glob("*.pdf"))[0]
d = pymupdf.open(pdf)
print("PAGES:", d.page_count)
for pno, pg in enumerate(d):
    for ln in pg.get_text().splitlines():
        s = WS.sub("", ln)
        if not s:
            continue
        only = Counter(s) - Counter(vis)
        if only:
            print(f"p{pno} LO-only {sum(only.values()):3d} {dict(only)} | {ln!r}")
d.close()
