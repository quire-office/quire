#!/bin/sh
# F1 發射對照：strict.docx 我方刻度 vs LO 真值（LO: 0,0.5,1,...,3.5 共 8 支）。
set -e
cd <repo>/.worktrees/t_f3acd271
cargo run -q -p quire-cli -- dump corpus/ooxmlexport/strict.docx \
  | /usr/bin/python3 -c '
import sys, json
pages = json.load(sys.stdin)
for pg in pages:
    ticks = [(round(v["x"], 1), round(v["y"], 1), v["text"])
             for it in pg["items"] for k, v in it.items()
             if k == "Text" and v["text"] in ("0", "0.5", "1", "1.5", "2", "2.5", "3", "3.5")]
    print("page", pg["page"], "刻度項：", ticks)
'
