#!/usr/bin/env python3
"""走向拓撲錨定（bug65649 判準修正，t_026e8c2e）的回歸鎖。

比照 render_lab 現況風格——零依賴、純函式合成案例：
  python3 scripts/tests/test_run_anchor_metrics.py          # 內建 runner，全綠 exit 0
  python3 -m pytest scripts/tests/test_run_anchor_metrics.py  # 有裝 pytest 時同樣可跑

保護對象（口徑變更的兩條邊界，缺一即為放水或誤殺）：
  A. 同拓撲＋頁數位移 ⇒ 假 sectPr 錯位必須消失、改報容量漂移（bug65649 指紋）；
  B. 拓撲不同／同拓撲段內尺寸全異 ⇒ 真缺陷照報，不得被 run 錨定掩蓋；
  C. 注記臂隔離：breaks 只被注記臂消費，注記口徑怎麼改，內容／版面分與
     first_divergence、measurement_artifacts 必須逐位不動。
render_lab.py --selftest 的 9a–9g 是同一判準的機制面臂；本檔是細部口徑鎖與
**bug65649 實指紋重放**（舊口徑 22 筆假錯位 ⇒ 新口徑 0 筆）。
"""

import importlib.util
import pathlib
import sys

SCRIPTS = pathlib.Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("render_lab", SCRIPTS / "render_lab.py")
rl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rl)

P = (595.3, 841.9)   # A4 直向（pt）
L = (841.9, 595.3)   # A4 橫向（pt）


def _lo_runs(spec):
    """spec=[(走向, 頁數), ...] ⇒ LO 逐頁 facts（只需 w/h；lines 空）。"""
    pages = []
    for (dim, n) in spec:
        w, h = dim
        pages.extend({"lines": [], "w_pt": w, "h_pt": h} for _ in range(n))
    return pages


def _ow_runs(spec):
    """spec=[(走向, 頁數), ...] ⇒ 我方 ow dict（pages 空行、breaks 帶尺寸）。"""
    pages, breaks = [], []
    for (dim, n) in spec:
        w, h = dim
        pages.extend({"lines": []} for _ in range(n))
        breaks.extend({"width_pt": w, "height_pt": h} for _ in range(n))
    return {"pages": pages, "breaks": breaks}


def _sectpr(notes):
    return [n for n in notes if "（sectPr）" in n]


def _sectpr_run(notes):
    return [n for n in notes if "（sectPr，run 錨定）" in n]


# ───────────────────────── A：同拓撲＋位移 ⇒ 假錯位消失 ─────────────────────────

def test_same_topology_drift_reports_capacity_not_sectpr():
    """bug65649 縮影：拓撲 P-L-P 全等、我方多 5 頁 ⇒ 零同位 sectPr、報容量漂移。"""
    lo = _lo_runs([(P, 20), (L, 69), (P, 5)])
    ow = _ow_runs([(P, 21), (L, 71), (P, 7)])
    row = rl.compare("run-anchor:drift", lo, ow)
    assert not _sectpr(row["notes"]), f"同位假錯位必須消失：{row['notes']}"
    assert not _sectpr_run(row["notes"])
    drift = [n for n in row["notes"] if "容量漂移 " in n]
    assert len(drift) == 3, f"三段頁數差皆非零，應 3 筆：{drift}"
    assert any("容量漂移 +1 頁" in n and "直向" in n for n in drift)
    assert any("容量漂移 +2 頁" in n and "橫向" in n for n in drift)
    assert any("容量漂移 +2 頁" in n and "直向" in n for n in drift)
    assert any("走向拓撲全等" in n and "+5 頁" in n for n in row["notes"])
    assert row["page_count_match"] is False


def test_same_topology_zero_drift_is_silent():
    """拓撲全等＋段內頁數也全等＋尺寸等價 ⇒ 注記臂完全靜默（不灌水）。
    斷言只篩注記臂自己的注記種類（雙空同位頁 artifact 屬 M2 既有臂，非本卡）。"""
    lo = [{"lines": [f"正文 {k}"], "w_pt": w, "h_pt": h}
          for k, (dim, n) in enumerate([(P, 3), (L, 2)])
          for (w, h) in [dim] * n]
    ow = _ow_runs([(P, 3), (L, 2)])
    row = rl.compare("run-anchor:silent", lo, ow)
    mine = [n for n in row["notes"]
            if "拓撲" in n or "容量漂移" in n or "sectPr" in n or "混尺" in n]
    assert mine == [], f"應零注記：{mine}"


def test_rounding_delta_within_tolerance_is_not_sectpr():
    """twip→pt 0.1pt 級捨入差（LO 841.9×595.3 vs 我方 842.0×595.4）不判 sectPr
    ——觸發集實跑（bug65649 Run 1）抓到的誤報態，容差必須與舊臂一致（2.0pt）。"""
    lo = [{"lines": [f"橫頁 {i}"], "w_pt": 841.9, "h_pt": 595.3} for i in range(3)]
    ow = {"pages": [{"lines": [f"橫頁 {i}"]} for i in range(3)],
          "breaks": [{"width_pt": 842.0, "height_pt": 595.4}] * 3}
    row = rl.compare("run-anchor:rounding", lo, ow)
    assert not [n for n in row["notes"] if "sectPr" in n], f"{row['notes']}"


# ───────────────────────── bug65649 實指紋重放（22→0） ─────────────────────────

# docs/bug65649-landscape-diag.md §三 對照表：(走向, LO 頁數, 我方頁數)
BUG65649 = [(P, 20, 21), (L, 69, 71), (P, 5, 7), (L, 28, 28), (P, 14, 16),
            (L, 2, 2), (P, 1, 1), (L, 38, 43), (P, 1, 1)]


def _old_arm(lo_pages, brks):
    """修復前 compare() 的同位索引迴路逐字重現（容差 2.0）。"""
    notes = []
    for i, p in enumerate(lo_pages):
        if i >= len(brks):
            notes.append(f"第 {i + 1} 頁我方無 pagebreak 記錄")
            continue
        ow_w, ow_h = round(brks[i]["width_pt"], 1), round(brks[i]["height_pt"], 1)
        if abs(p["w_pt"] - ow_w) > 2.0 or abs(p["h_pt"] - ow_h) > 2.0:
            notes.append(
                f"第 {i + 1} 頁 LO {p['w_pt']}×{p['h_pt']} ≠ 我方 {ow_w}×{ow_h}（sectPr）"
            )
    return notes


def test_bug65649_replay_old_arm_22_false_notes_new_arm_zero():
    """按診斷報告 Runs 表重放 bug65649：舊口徑必得 22 筆假 sectPr（與基線
    worst-20261111/scores.jsonl 該檔注記數逐位相符），新口徑必得 0 筆＋
    容量漂移注記。這是「度量假分差被修掉、真缺陷不漏修」的指紋級鎖。
    """
    lo = _lo_runs([(d, n_lo) for (d, n_lo, _) in BUG65649])
    ow = _ow_runs([(d, n_ow) for (d, _, n_ow) in BUG65649])
    assert len(lo) == 178 and len(ow["breaks"]) == 190
    old = _old_arm(lo, ow["breaks"])
    assert len(_sectpr(old)) == 22, f"舊口徑指紋應 22 筆假錯位：{len(old)}"
    assert not [n for n in old if "我方無 pagebreak" in n]
    row = rl.compare("bug65649:replay", lo, ow)
    assert not _sectpr(row["notes"]), f"新口徑必零同位錯位：{row['notes'][:5]}"
    assert not _sectpr_run(row["notes"]), "各段宣告尺寸相同，不該有 run 錨定錯位"
    drift = [n for n in row["notes"] if "容量漂移 " in n]
    assert len(drift) == 5, f"+1/+2/+2/0/+2/0/0/+5/0 ⇒ 非零 5 段：{drift}"
    assert any("容量漂移 +5 頁" in n and "橫向" in n for n in drift)  # Run 7
    assert any("容量漂移 +1 頁" in n and "直向" in n for n in drift)  # Run 0


# ───────────────────────── B：真缺陷照報（防放水） ─────────────────────────

def test_different_topology_still_reports_same_index():
    """LO 有橫向段、我方全直向（真 section 缺陷）⇒ 照舊逐同位報＋標明拓撲差。"""
    lo = _lo_runs([(P, 3), (L, 2)])
    ow = _ow_runs([(P, 5)])
    row = rl.compare("run-anchor:topo-diff", lo, ow)
    assert any(n.startswith("走向拓撲不一致：LO=PL 我方=P") for n in row["notes"]), \
        f"拓撲差必寫明：{row['notes']}"
    sect = _sectpr(row["notes"])
    assert len(sect) == 2, f"LO 第 4、5 頁（橫向）vs 我方直向應 2 筆：{sect}"
    assert not any("容量漂移" in n for n in row["notes"])


def test_same_topology_but_geo_mismatch_still_flagged():
    """拓撲全等（P-L）但橫向段我方宣告 900×600（同拓撲標籤下的真幾何差）
    ⇒ run 錨定照報「（sectPr，run 錨定）」，不能被「拓撲全等」分支放水。"""
    lo = _lo_runs([(P, 2), (L, 2)])
    ow = {"pages": [{"lines": []}] * 4,
          "breaks": [{"width_pt": P[0], "height_pt": P[1]}] * 2
          + [{"width_pt": 900.0, "height_pt": 600.0}] * 2}
    row = rl.compare("run-anchor:geo-diff", lo, ow)
    hit = _sectpr_run(row["notes"])
    assert len(hit) == 1 and "第 2 段" in hit[0], f"第 2 段真幾何差照報：{row['notes']}"


def test_missing_pagebreak_records_still_reported():
    """我方 brks 短於 LO 頁數（拓撲不一致態）：退化態照舊逐頁記缺 pagebreak，
    不被拓撲臂吞掉（此態 brks 只少一頁 ⇒ 僅最後一頁缺記錄，倒數第二頁是尺寸差）。"""
    lo = _lo_runs([(P, 4)])
    ow = _ow_runs([(P, 2), (L, 1)])   # 拓撲不同（P vs PL）
    row = rl.compare("run-anchor:no-brks", lo, ow)
    assert any(n.startswith("走向拓撲不一致") for n in row["notes"])
    assert any("第 4 頁我方無 pagebreak 記錄" in n for n in row["notes"]), \
        f"缺頁注記必留：{row['notes']}"
    assert len(_sectpr(row["notes"])) == 1, f"第 3 頁（我方橫向我方直）照報：{row['notes']}"


def test_empty_lo_pages_no_topology_note():
    """LO 零頁 ⇒ 不寫「走向拓撲不一致」（無中位可報），也不灌水。"""
    row = rl.compare("run-anchor:empty-lo", [], _ow_runs([(P, 3)]))
    assert not any("拓撲" in n for n in row["notes"]), f"{row['notes']}"


# ───────────────────────── C：注記臂隔離（分數零變動） ─────────────────────────

def test_breaks_only_feed_annotation_arm_scores_bitwise_identical():
    """同一 lo/pages、只把 breaks 從「全錯尺寸」換成「全對尺寸」：注記必須翻盤，
    但 content/layout 分、first_divergence、measurement_artifacts 必須逐位相同
    ——注記臂翻盤不得滲進分數（本卡紀律「前後 scores 分數逐位相同」的機制面鎖）。
    """
    spec_lo = [(P, 4), (L, 3)]
    spec_ow = [(P, 5), (L, 3)]     # 同拓撲 P-L、總數多 1 頁（位移態）
    lo = []
    for (dim, n) in spec_lo:
        w, h = dim
        lo.extend({"lines": [f"段落文字 {i} 內容填充以避開 0/0 臂"], "w_pt": w, "h_pt": h}
                  for i in range(n))
    pages = [{"lines": [f"段落文字 {i} 內容填充以避開 0/0 臂"]} for i in range(8)]
    bad = {"pages": pages, "breaks": [{"width_pt": L[0], "height_pt": L[1]}] * 8}
    good = {"pages": pages,
            "breaks": [{"width_pt": w, "height_pt": h}
                       for (dim, n) in spec_ow for (w, h) in [dim] * n]}
    r_bad = rl.compare("iso:bad", lo, bad)
    r_good = rl.compare("iso:good", lo, good)
    assert _sectpr(r_bad["notes"]) or _sectpr_run(r_bad["notes"]) or \
        any("拓撲不一致" in n for n in r_bad["notes"]), "壞尺寸態必有注記"
    assert not _sectpr(r_good["notes"]) and not _sectpr_run(r_good["notes"])
    assert r_good["content_scores"] == r_bad["content_scores"]
    assert r_good["layout_scores"] == r_bad["layout_scores"]
    assert r_good["first_divergence"] == r_bad["first_divergence"]
    assert r_good["measurement_artifacts"] == r_bad["measurement_artifacts"]
    assert r_good["page_count_match"] == r_bad["page_count_match"]


def test_compare_row_schema_unchanged():
    """欄位集合不得因注記臂改動而增減（scoreboard/報告渲染依賴此 schema）。"""
    row = rl.compare("schema", _lo_runs([(P, 1)]), _ow_runs([(P, 1)]))
    assert set(row) == {"file", "lo_pages", "ow_pages", "page_count_match",
                        "content_scores", "layout_scores", "first_divergence",
                        "notes", "measurement_artifacts"}, sorted(row)


# ───────────────────────── 單元：orientation_runs / page_orient ─────────────────────────

def test_orientation_runs_segmentation():
    runs = rl.orientation_runs([(595.3, 841.9), (595.3, 841.9), (841.9, 595.3),
                                (600.0, 600.0), (600.0, 600.0)])
    assert [(r["orient"], r["start"], r["n"]) for r in runs] == \
        [("P", 0, 2), ("L", 2, 1), ("S", 3, 2)]
    assert rl.orientation_runs([]) == []
    assert rl.page_orient(595.3, 841.9) == "P" and rl.page_orient(841.9, 595.3) == "L"
    assert rl.page_orient(600.0, 600.0) == "S"
    assert rl.page_orient(600.0, 601.5) == "S", "容差 2.0pt 內算方形"


def test_run_sizes_set_detects_mixed_within_run():
    """段內尺寸口徑三態（同段同走向）：
    ① 段內尺寸全在容差內 ⇒ 靜默；
    ② 兩側段內都混同一組尺寸（A4＋Letter）⇒ 混尺如實記錄×2、但不判 sectPr；
    ③ LO 混尺、我方只有 Letter ⇒ 混尺記錄＋無夥伴的 A4 照報 sectPr（run 錨定）。
    目的：「同段同走向」不得成為吞掉真尺寸差的藉口，也不得把等價尺寸報成錯位。"""
    def _pages(pairs):
        return ([{"lines": [f"直頁 {i}"], "w_pt": w, "h_pt": h}
                 for i, (w, h) in enumerate(pairs)],
                {"pages": [{"lines": [f"直頁 {i}"]} for i in range(len(pairs))],
                 "breaks": [{"width_pt": w, "height_pt": h} for (w, h) in pairs]})

    A4, LT = (595.3, 841.9), (612.0, 792.0)
    # ① 容差內靜默
    lo, ow = _pages([A4, (595.4, 842.0)])
    row = rl.compare("run-anchor:tol", lo, ow)
    mine = [n for n in row["notes"] if "拓撲" in n or "sectPr" in n or "混尺" in n]
    assert mine == [], f"容差內應靜默：{mine}"
    # ② 兩側同混尺 ⇒ 混尺記錄、零 sectPr
    lo, ow = _pages([A4, LT])
    row = rl.compare("run-anchor:mixed-both", lo, ow)
    assert len([n for n in row["notes"] if "段內混尺" in n]) == 2, \
        f"兩側各一筆混尺：{row['notes']}"
    assert not _sectpr_run(row["notes"]), f"尺寸集合等價不判錯位：{row['notes']}"
    # ③ 單側混尺 ⇒ 混尺記錄＋真尺寸差照報
    lo, _ = _pages([A4, LT])
    ow = {"pages": [{"lines": [f"直頁 {i}"]} for i in range(2)],
          "breaks": [{"width_pt": LT[0], "height_pt": LT[1]}] * 2}
    row2 = rl.compare("run-anchor:mixed-ow", lo, ow)
    assert len([n for n in row2["notes"] if "LO 段內混尺" in n]) == 1, \
        f"混尺如實記錄：{row2['notes']}"
    assert len(_sectpr_run(row2["notes"])) == 1, \
        f"無夥伴的 A4 照報：{row2['notes']}"


def main() -> int:
    tests = [(k, v) for k, v in sorted(globals().items())
             if k.startswith("test_") and callable(v)]
    fails = 0
    for name, fn in tests:
        try:
            fn()
            print(f"  PASS {name}")
        except AssertionError as e:
            fails += 1
            print(f"  FAIL {name}：{e}")
    print("RUN-ANCHOR", "ALL-PASS" if fails == 0 else f"{fails}-FAIL")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
