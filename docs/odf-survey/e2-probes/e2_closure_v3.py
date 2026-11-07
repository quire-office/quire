#!/usr/bin/env python3
"""ODF-E2 最終閉合表（報告引用數字唯一來源）。

每席：gap = OW_ns − LO_ns；LO-only／我方-only 以字元多重集給出（先扣 LO 合成類：
控制字元、PUA 字形、TOC 點引導符），要求 LO-only ∪ 我方-only 逐字元可命名且等式成立。
"""
import json, pathlib, re, subprocess, zipfile
from collections import Counter
import xml.etree.ElementTree as ET
import pymupdf

SCRATCH = pathlib.Path("/home/jerry/.hermes/profiles/ow-qa/cache/scratch/e2_odt")
REPO = pathlib.Path("/home/jerry/projects/quire/.worktrees/t_72dc8bc8")
TXT = "{urn:oasis:names:tc:opendocument:xmlns:text:1.0}"
SVG = "{urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0}"
MATH = "{http://www.w3.org/1998/Math/MathML}"
WS = re.compile(r"\s+")
ODTFLOW = str(SCRATCH / "probe_target/release/odtflow")
SIX = {
    "S1": "corpus-odt/ext/lo_core_sw_qa_odfimport_ooo32780-1.odt",
    "S2": "corpus-odt/lo_out/lo__poi__test-data_document_form_footnotes.odt",
    "S3": "corpus-odt/lo_out/lo__ooxmlexport__mce-nested.odt",
    "S4": "corpus-odt/lo_out/lo__ooxmlexport__strict.odt",
    "S5": "corpus-odt/lo_out/lo__ooxmlexport__tdf124637_sectionMargin.odt",
    "S6": "corpus-odt/lo_out/lo__ooxmlimport__tdf87533_bidi.odt",
}

def groups_for(rel):
    """回傳源側具名文字組：頁首尾（styles.xml）、嵌入物件 part 文字、隱藏段（我方口徑）。"""
    with zipfile.ZipFile(REPO / rel) as z:
        names = z.namelist()
        cx = z.read("content.xml")
        sx = z.read("styles.xml").decode("utf-8", "replace") if "styles.xml" in names else ""
        objs = "".join(z.read(n).decode("utf-8", "replace") for n in names
                       if re.match(r"Object[^/]*/content\.xml$", n))
    hf = "".join(WS.sub("", re.sub(r"<[^>]+>", "", m.group(2))) for m in re.finditer(
        r"<style:(header|footer)(?:-first|-left)?[^>]*>(.*?)</style:\1(?:-first|-left)?>", sx, re.S))
    obj_txt = ""
    for m in re.finditer(r"<text:(?:p|h)[^>]*>(.*?)</text:(?:p|h)>", objs, re.S):
        obj_txt += WS.sub("", re.sub(r"<[^>]+>", "", m.group(1)))
    try:
        oroot = ET.fromstring(objs) if objs.strip() else None
    except Exception:
        oroot = None
    if oroot is not None:
        for tag in ("mi", "mn", "mo"):
            obj_txt += "".join((e.text or "") for e in oroot.iter(MATH + tag))
    root = ET.fromstring(cx)
    parent = {c: p for p in root.iter() for c in p}
    return hf, obj_txt, root, parent, cx.decode("utf-8", "replace")

rows = []
for k, rel in SIX.items():
    pdf = sorted((SCRATCH / "lo_pdf" / pathlib.Path(rel).name).glob("*.pdf"))[0]
    d = pymupdf.open(pdf)
    lo_all = WS.sub("", "".join(l for p_ in d for l in p_.get_text().splitlines()))
    d.close()
    ow = json.loads(subprocess.run([ODTFLOW, str(REPO / rel)], capture_output=True, text=True).stdout)["paragraphs"]
    ow_all = WS.sub("", "".join(p["text"] for p in ow))
    hf, obj_txt, root, parent, cx = groups_for(rel)
    # 我方口徑隱藏段
    flags = []
    for el in root.iter():
        if el.tag not in (TXT + "p", TXT + "h"):
            continue
        p, h = parent.get(el), False
        while p is not None:
            if p.tag == TXT + "section" and (p.get(TXT + "display") == "none"
                                             or p.get(TXT + "is-hidden") == "true"):
                h = True
                break
            p = parent.get(p)
        flags.append(h)
    H = sum(len(WS.sub("", p["text"])) for p, f in zip(ow, flags) if f)
    vis = WS.sub("", "".join(p["text"] for p, f in zip(ow, flags) if not f))
    # LO 合成類
    n_ctrl = sum(lo_all.count(chr(c)) for c in range(0, 32))
    n_pua = len(re.findall(r"[\ue000-\uf8ff]", lo_all))
    n_dots = max(0, lo_all.count(".") - ow_all.count(".")) \
        if re.search(r'style:leader-text="[^"]+"', cx) else 0
    keep = []
    dropped = Counter()
    nd = 0
    for c in lo_all:
        if ord(c) < 0x20:
            dropped["ctrl"] += 1
            continue
        if 0xE000 <= ord(c) <= 0xF8FF:
            dropped["pua"] += 1
            continue
        if c == "." and nd < n_dots:
            nd += 1
            dropped["dots"] += 1
            continue
        keep.append(c)
    lo_clean = "".join(keep)
    lo_only = Counter(lo_clean) - Counter(vis)
    ow_only = Counter(vis) - Counter(lo_clean)
    # 具名：LO-only 字元能歸給頁首尾/嵌入物件者先扣
    r_hf = Counter(hf) & lo_only
    r_obj = (Counter(obj_txt) & (lo_only - r_hf))
    rest = lo_only - r_hf - r_obj
    eq = H - sum(lo_only.values()) + sum(ow_only.values())
    row = dict(seat=k, rel=rel, gap=len(ow_all) - len(lo_all), H=H,
               ctrl=dropped["ctrl"], pua=dropped["pua"], dots=dropped["dots"],
               hf_emit=sum(r_hf.values()), obj_emit=sum(r_obj.values()),
               lo_only=dict(lo_only), lo_only_n=sum(lo_only.values()),
               ow_only_n=sum(ow_only.values()),
               rest_named="".join(f"{c}×{n}" if n > 1 else c for c, n in sorted(rest.items())),
               closure_eq=eq, hf_src=len(hf), obj_src=len(obj_txt))
    rows.append(row)
    print(json.dumps(row, ensure_ascii=False))
(SCRATCH / "closure_v3.json").write_text(json.dumps(rows, ensure_ascii=False, indent=1),
                                         encoding="utf-8")
