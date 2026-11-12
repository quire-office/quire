#!/usr/bin/env python3
"""t_ae9a9f22：A/B JSON 摘要（頁數/發射項變化逐檔）。"""
import json, sys
rows = json.load(open(sys.argv[1]))
comp = [r for r in rows if r["delta"] is not None]
changed = [r for r in comp if r["base_items"] != r["head_items"]]
print("compared=%d pages_up=%d pages_down=%d" % (
    len(comp), sum(1 for r in comp if r["delta"] > 0),
    sum(1 for r in comp if r["delta"] < 0)))
print("files_with_changed_item_counts=%d" % len(changed))
tb = sum(r["base_items"] for r in comp)
th = sum(r["head_items"] for r in comp)
print("items_total base=%d head=%d delta=%d" % (tb, th, th - tb))
for r in changed:
    print("  %-74s %6d -> %6d" % (r["file"], r["base_items"], r["head_items"]))
