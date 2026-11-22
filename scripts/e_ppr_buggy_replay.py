"""E-1 同源返修（t_e32ac1cd）BUGGY 回塞鑑別腳本（支援 buggy / good 兩態）。

用法：
  python3 scripts/e_ppr_buggy_replay.py buggy   # 塞回同源缺陷，e_ppr_patch_merge 必紅
  python3 scripts/e_ppr_buggy_replay.py good    # 以字串置換完全逆轉，並與 HEAD 逐位元組對照

規範守衛（k29e／k29c 腳本同款紀律）：
- 嚴禁 git checkout / reset / clean / stash，走字串就地置換。
- 每處置換錨點必唯一（同點多處時宣告預期次數），失配即報警退出（不靜默跳過）。
- good 態末了以 `git diff` 對照 HEAD 驗證逐位元組一致。

瑕疵注入（還原本卡修正前的缺陷形態，字串置換、可完全逆轉）：
  P1 `patch_ppr`／`patch_numpr` 取既有宣告內文退回 `strip_leading_element`
     （對 `leading_element` 產物恆回空字串）→ 燈 1／2／3／4／5／6／6b／7／8／
     9／10／11 全數轉紅（既有段落宣告每次重組全數蒸發）。
  P2 段落側零意圖硬閂失效（全 None 補丁照樣重組）→ 燈 5／5b 另拍出「空 pPr
     被材料化」形態（k29d 拆分節判定的依賴項）。
  P3 numPr 改回全域字串掃描（不看頂層）→ 燈 6b 拍出「ChangeLevel 沿用
     pPrChange 快照內的錯號 numId」。
  P4 段落側偷吃 rPr 那份 rank 表（卡面點名的「鍵名重疊但序不同」陷阱，
     a＝patch_ppr 臂／b＝patch_numpr 臂）→ 燈 7／11 拍出宣告序退化。
  P5 ChangeLevel 零意圖硬閂退回「只護 RemoveNumbering」→ 燈 15 拍出無 numPr
     段落被合成 numId=0 寡人宣告（agy 第二意見 P3）。
"""

import subprocess
import sys

TARGET = "core/engine-api/src/mutation.rs"

# (標籤, 好版字串, BUGGY 版字串, 預期命中次數)——P1 的錨點在 patch_ppr 與
# patch_numpr 各一處，兩處都要塞回缺陷形態才是「同源兩臂」的完整回塞。
REVERTS = [
    (
        "P1_PPR_INNER_BUGGY",
        '        element_inner(ppr, "w:pPr")',
        '        strip_leading_element(ppr, "w:pPr")',
        2,
    ),
    (
        "P2_PPR_ZERO_INTENT_GATE_BUGGY",
        "    if patch == &ParaPropsPatch::default() {",
        "    if false && patch == &ParaPropsPatch::default() {",
        1,
    ),
    (
        "P3_NUMPR_TOPLEVEL_SCAN_BUGGY",
        '    let existing_numpr = top_level_element(&inner, "w:numPr");',
        '    let existing_numpr = if extract_num_id(&inner).is_some() {\n        Some(inner.clone())\n    } else {\n        None\n    };',
        1,
    ),
    # P4：段落側偷吃 rPr 那份 rank 表（卡面點名的「鍵名重疊但序不同」陷阱）——
    # 編譯過得、多數欄位看起來還算對，但 `rPr`／`jc` 在新式名查不中即退化隨前件，
    # 燈 7／11 據此拍紅（本卡開發過程真實踩過：併入器把裸名餵給 tag_local 同款失配）。
    (
        "P4a_PPR_RANK_TABLE_BUGGY",
        "&drop, ppr_child_rank)",
        "&drop, rpr_child_rank)",
        1,
    ),
    (
        "P4b_NUMPR_RANK_TABLE_BUGGY",
        '&["w:numPr"], ppr_child_rank)',
        '&["w:numPr"], rpr_child_rank)',
        1,
    ),
    (
        "P5_CHANGELEVEL_ZERO_INTENT_BUGGY",
        "    if existing_numpr.is_none() && !matches!(action, ListAction::AttachExisting { .. }) {",
        "    if existing_numpr.is_none() && matches!(action, ListAction::RemoveNumbering) {",
        1,
    ),
]


def toggle(old, new, label, expect=1):
    with open(TARGET, "r", encoding="utf-8") as fh:
        content = fh.read()
    n = content.count(old)
    if n == expect:
        with open(TARGET, "w", encoding="utf-8") as fh:
            fh.write(content.replace(old, new))
        print(f"[{label}] 成功置換 {n} 處")
    else:
        print(f"[{label}] 錨點命中 {n} 次、預期 {expect} 次，拒絕置換")
        sys.exit(1)


def run_tests():
    return subprocess.call(
        [
            "cargo",
            "test",
            "-p",
            "quire-engine",
            "--test",
            "e_ppr_patch_merge",
            "--",
            "--test-threads=1",
            "--nocapture",
        ]
    )


mode = sys.argv[1] if len(sys.argv) > 1 else ""
if mode == "buggy":
    for label, good, bad, expect in REVERTS:
        toggle(good, bad, label, expect)
    print("=== 正在執行紅燈測試（預期必紅）===")
    res = run_tests()
    print(f"BUGGY 態測試結束，exit_code={res}（預期非 0）")
    if res == 0:
        print("錯誤：BUGGY 態下考卷竟全綠，紅燈鑑別力失效", file=sys.stderr)
        sys.exit(1)
elif mode == "good":
    for label, good, bad, expect in REVERTS:
        toggle(bad, good, label.replace("BUGGY", "GOOD"), expect)
    dirty = subprocess.check_output(["git", "diff", "--name-only", "--", TARGET]).decode()
    if dirty.strip():
        print(f"警告：還原後 {dirty.strip()} 與 HEAD 不一致（置換未完全逆轉）", file=sys.stderr)
        subprocess.call(["git", "diff", "--", TARGET])
        sys.exit(1)
    print("還原態與 HEAD 逐位元組一致（乾淨副本驗證通過）")
    print("=== 正在執行正常測試（預期必綠）===")
    sys.exit(run_tests())
else:
    print("用法: e_ppr_buggy_replay.py buggy|good", file=sys.stderr)
    sys.exit(2)
