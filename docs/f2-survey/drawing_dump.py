#!/usr/bin/env python3
# F2 解剖：artistic_effects.docx 四個 w:drawing 區塊原始 XML dump
import re, os

D = "<hermes>/profiles/ow-render/cache/scratch/f2ae3"
x = open(os.path.join(D, "word/document.xml"), encoding="utf-8").read()

print("== media ==")
for sub in ("media", "embeddings", "theme"):
    p = os.path.join(D, "word", sub)
    if os.path.isdir(p):
        for f in sorted(os.listdir(p)):
            print(f"  {sub}/{f}\t{os.path.getsize(os.path.join(p,f))}")

print("\n== rels ==")
print(open(os.path.join(D, "word/_rels/document.xml.rels"), encoding="utf-8").read())

print("\n== drawing 區塊原始 XML ==")
for i, m in enumerate(re.finditer(r"<w:drawing>.*?</w:drawing>", x, re.S)):
    print(f"\n######## drawing #{i} len={len(m.group(0))}")
    print(m.group(0))
