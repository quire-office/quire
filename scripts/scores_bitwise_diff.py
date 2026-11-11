#!/usr/bin/env python3
"""score 前後對照燈（t_026e8c2e 度量卡紀律：注記臂口徑變更 ⇒ 分數逐位相同）。

用法：
  /usr/bin/python3 scripts/scores_bitwise_diff.py A.jsonl B.jsonl
比對兩份 scores.jsonl 的**分數欄位**（lo_pages/ow_pages/page_count_match/
content_scores/layout_scores/first_divergence/status），逐檔逐位；注記欄位
（notes/measurement_artifacts）屬本卡刻意翻盤的口徑面，單獨統計差異檔數，
不計入分數燈。分數有任何一欄不同 ⇒ exit 1（＝動到不該動的）。
"""
import json
import pathlib
import sys

SCORE_KEYS = ("lo_pages", "ow_pages", "page_count_match", "content_scores",
              "layout_scores", "first_divergence", "status")


def load(p: pathlib.Path) -> dict[str, dict]:
    rows = {}
    for line in p.read_text(encoding="utf-8").splitlines():
        if line.strip():
            r = json.loads(line)
            rows[r["file"]] = r
    return rows


def main() -> int:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    a, b = load(pathlib.Path(sys.argv[1])), load(pathlib.Path(sys.argv[2]))
    only_a = sorted(set(a) - set(b))
    only_b = sorted(set(b) - set(a))
    if only_a or only_b:
        print(f"✗ 檔案集合不同：僅 A {only_a[:3]}／僅 B {only_b[:3]}")
        return 1
    score_diff, note_diff = [], []
    for f in sorted(a):
        ra, rb = a[f], b[f]
        bad = [k for k in SCORE_KEYS if ra.get(k) != rb.get(k)]
        if bad:
            score_diff.append((f, bad))
        if ra.get("notes") != rb.get("notes"):
            note_diff.append(f)
    print(f"共同檔案 {len(a)} 檔")
    if score_diff:
        print(f"✗ 分數欄位不同 {len(score_diff)} 檔（本卡零改動紅燈）：")
        for f, keys in score_diff[:10]:
            print(f"   - {f}: {keys}")
    else:
        print("✓ 分數欄位逐位相同（content/layout/first_divergence/頁數/頁數一致/status）")
    print(f"注記欄位翻盤檔數：{len(note_diff)}")
    for f in note_diff:
        print(f"   ~ {f}")
    return 1 if score_diff else 0


if __name__ == "__main__":
    sys.exit(main())
