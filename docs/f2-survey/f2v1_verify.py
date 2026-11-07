# F2 語料普查：目標檔與代表檔 prst/warp/效果節點實拆（紅燈考卷真值源）
#
# 口徑：zipfile 逐 part 解壓，掃「本體 part」（word/document.xml、header*、footer*、
# footnotes、endnotes、comments）。刻意排除 theme1.xml／charts／diagrams——theme 的
# effectStyleLst 每個 docx 自帶 outerShdw/bevelT 樣板，計入會把影響面灌水（解剖報告 §5.1）。
import re
import zipfile

ROOT = "/home/jerry/projects/quire/.worktrees/t_ab254070/corpus/"
BODY = re.compile(r"word/(document|header\d*|footer\d*|footnotes|endnotes|comments)\.xml$")

TARGETS = [
    "ooxmlexport/artistic_effects.docx",
    "ooxmlexport/fdo74605.docx",
    "ooxmlimport/groupshape-fontname.docx",
    "ooxmlexport/tdf113183.docx",
    "ooxmlexport/tdf113258.docx",
    "ooxmlimport/tdf120551.docx",
    "poi/test-data_document_shapes-with-text.docx",
    "ooxmlexport/fdo80897.docx",
]

NODES = [
    "prstTxWarp", "wordArt", "prstGeom", "custGeom", "solidFill", "gradFill", "noFill",
    "effectLst", "glow", "softEdge", "reflection", "bevelT", "outerShdw", "sp3d", "scene3d",
]

for rel in TARGETS:
    z = zipfile.ZipFile(ROOT + rel)
    txt = ""
    for n in z.namelist():
        if BODY.search(n):
            txt += z.read(n).decode("utf8", "ignore")
    counts = {k: len(re.findall(r"<[A-Za-z0-9]+:%s[\s/>]" % k, txt)) for k in NODES}
    counts = {k: v for k, v in counts.items() if v}
    geoms = re.findall(r"<a:prstGeom[^>]*prst=\"([^\"]+)\"", txt)
    warps = re.findall(r"<a:prstTxWarp[^>]*prst=\"([^\"]+)\"", txt)
    texts = len(re.findall(r"<a:t[\s>]", txt))
    wtexts = len(re.findall(r"<w:t[\s>]", txt))
    print(rel)
    print("   nodes:", counts)
    print("   geom :", sorted(set(geoms)), "n=", len(geoms))
    print("   warp :", sorted(set(warps)), "n=", len(warps), " a:t=", texts, " w:t=", wtexts)
