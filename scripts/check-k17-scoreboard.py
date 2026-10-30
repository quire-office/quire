#!/usr/bin/env python3
"""K17 門檻驗證腳本：檢查 render_lab A/B 比對分數不退步。

口徑來源（PM 復活令 + 二輪覆核裁決，t_5a23ed09 卡面）：
- WC048 兩檔**內容分必須 strictly 大於**二輪基線（持平不算過）：
  - corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx      > 0.718
  - corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell-Mod.docx  > 0.723
- 觸發集其餘檔零回退（>= 基線）：
  - corpus/ooxmlexport/tdf124398_groupshapeChart.docx         >= 0.574

用法：先跑 render_lab（PM 親跑口徑 `scripts/render_lab.py`）產出 report.md，
再 `python3 scripts/check-k17-scoreboard.py <report.md|report_dir>`。
"""

import pathlib
import re
import sys

# (檔名 → (基線, 嚴格大于？))
BASELINES = {
    "corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx": (0.718, True),
    "corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell-Mod.docx": (0.723, True),
    "corpus/ooxmlexport/tdf124398_groupshapeChart.docx": (0.574, False),
}


def parse_report(report_path: pathlib.Path) -> dict[str, float]:
    scores: dict[str, float] = {}
    if not report_path.exists():
        print(f"錯誤：報告檔不存在：{report_path}", file=sys.stderr)
        return scores
    text = report_path.read_text(encoding="utf-8")
    for line in text.splitlines():
        if not line.startswith("|") or "---" in line or "檔案" in line:
            continue
        parts = [p.strip() for p in line.split("|")]
        # 表格格式: | 檔案 | LO 頁 | OW 頁 | 頁數 | 內容相似度（首 6 頁） | ...
        if len(parts) >= 6:
            f = parts[1]
            cs_str = parts[5]
            m = re.search(r"([0-9.]+)", cs_str)
            if m and f in BASELINES:
                scores[f] = float(m.group(1))
    return scores


def main():
    if len(sys.argv) < 2:
        report_dir = pathlib.Path.home() / ".hermes/cache/scratch/rl_branch"
    else:
        report_dir = pathlib.Path(sys.argv[1])
    report_file = report_dir / "report.md" if report_dir.is_dir() else report_dir

    print(f"讀取 render_lab 報告：{report_file}")
    scores = parse_report(report_file)
    if not scores:
        print("未在報告中解析到目標檔案的分數！", file=sys.stderr)
        sys.exit(1)

    all_passed = True
    print("\n=== K17 Scoreboard 門檻驗證 ===")
    for f, (baseline, strict) in BASELINES.items():
        actual = scores.get(f)
        op = ">" if strict else ">="
        if actual is None:
            print(f"✗ {f}: 未找到分數（基線 {op} {baseline}）")
            all_passed = False
        elif (strict and actual > baseline) or (not strict and actual >= baseline):
            print(f"✓ {f}: 實測 {actual} {op} 基線 {baseline}")
        else:
            diff = actual - baseline
            verdict = "持平不算過" if strict and actual == baseline else "退步"
            print(f"✗ {f}: 實測 {actual} 未達 {op} {baseline}（{verdict} {diff:+.3f}）")
            all_passed = False

    if not all_passed:
        print("\n驗證失敗：內容相似度未達標！", file=sys.stderr)
        sys.exit(1)
    print("\n所有檔案內容相似度均達標！")


if __name__ == "__main__":
    main()
