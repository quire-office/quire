#!/usr/bin/env python3
"""J3 度量升級紅燈檔：色彩軸 / 圖片軸 / 真缺字對账 ＋ LO 殭屍收場路徑。

比照 render_lab 現況風格——零依賴、純函式合成案例：
  python3 scripts/tests/test_j3_metrics.py        # 內建 runner，全綠 exit 0
  python3 -m pytest scripts/tests/test_j3_metrics.py   # 有裝 pytest 時同樣可跑
（目前環境無 pytest，故 test_* 為無 fixture 的純函式，兩種跑法等價。）

先紅後綠：升級前 render_lab 沒有 score_color / score_image / token_audit，
下列三軸案例全部以 AttributeError 紅；殭屍收場案例鎖既有行為（升級後仍須綠）。
"""

import importlib.util
import pathlib
import subprocess
import sys
import tempfile
import textwrap
import traceback
from unittest import mock

import numpy as np

SCRIPTS = pathlib.Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("render_lab", SCRIPTS / "render_lab.py")
rl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rl)

LIGHT_BLUE = (0xD3, 0xDF, 0xEE)   # fdo68607 LO 側表格底紋色


def blank(h=400, w=300):
    return np.full((h, w, 3), 255, dtype=np.uint8)


def fill(img, x0, y0, x1, y1, rgb):
    img[y0:y1, x0:x1] = rgb
    return img


# ───────────────────────── 色彩軸 ─────────────────────────

def test_color_missing_shading_is_zero():
    """LO 有大片淡藍底紋、我方整頁白＝fdo68607 型 ⇒ ≈0。"""
    lo = fill(blank(), 0, 0, 300, 200, LIGHT_BLUE)
    s = rl.score_color(lo, blank())
    assert s < 0.05, f"色塊全缺應≈0，實得 {s}"


def test_color_identical_is_one_and_gray_ignored():
    lo = fill(blank(), 0, 0, 300, 200, LIGHT_BLUE)
    assert rl.score_color(lo, lo.copy()) > 0.99
    # 黑白灰（sat<25）不進直方圖：純灰字頁 vs 純白頁 ⇒ 兩邊都「無彩色」＝1.0
    gray = fill(blank(), 10, 10, 200, 40, (60, 60, 60))
    assert rl.score_color(gray, blank()) > 0.99


def test_color_wrong_hue_is_low():
    lo = fill(blank(), 0, 0, 300, 200, LIGHT_BLUE)
    ow = fill(blank(), 0, 0, 300, 200, (0xEE, 0xB0, 0xB0))   # 同面積換成淡紅
    assert rl.score_color(lo, ow) < 0.3


# ───────────────────────── 圖片軸 ─────────────────────────

def test_image_same_box_is_one():
    a = fill(blank(), 20, 20, 180, 120, (200, 40, 40))
    assert rl.score_image(a, a.copy()) > 0.99


def test_image_missing_and_misplaced_are_low():
    lo = fill(blank(), 20, 20, 180, 120, (200, 40, 40))
    assert rl.score_image(lo, blank()) < 0.05                       # 我方整張圖缺
    # Bug51170 型：位置/尺寸偏差（LO 大圖靠左、我方小圖置中）
    ow = fill(blank(), 130, 20, 250, 70, (200, 40, 40))
    assert rl.score_image(lo, ow) < 0.5
    # 兩邊都沒有彩色區塊 ⇒ 1.0（無從比較不扣分）
    assert rl.score_image(blank(), blank()) > 0.99


def test_image_boxes_bbox():
    a = fill(blank(), 20, 30, 180, 120, (200, 40, 40))
    boxes = rl.image_boxes(a)
    assert len(boxes) == 1
    x0, y0, x1, y1 = boxes[0]
    assert abs(x0 - 20) <= 6 and abs(y0 - 30) <= 6 and abs(x1 - 180) <= 6 and abs(y1 - 120) <= 6


# ───────────────────────── 真缺字 ─────────────────────────

def _missing(lo_text, ow_text):
    r = rl.token_audit(lo_text, ow_text)
    return {t.casefold() for t in r["true_missing"]}, r


def test_tokens_true_missing_vs_line_split():
    lo = "Drama sharp-edged blade IGCSE glossary of terms"
    ow = "Drama sharp-\nedged blade of terms"          # 我方斷行把 sharp-edged 切成兩段
    miss, r = _missing(lo, ow)
    assert miss == {"igcse", "glossary"}, miss         # 真丟字
    assert "sharp-edged" not in miss                   # 斷行切片不算缺
    assert r["split_ok"] and "sharp-edged" in {t.casefold() for t in r["split_ok"]}


def test_tokens_reverse_split_and_punct_and_case():
    # LO 把 token 切兩段、我方連著 ⇒ 不算缺；標點/大小寫差不算缺
    lo = "well-\nknown Result."
    ow = "Well-known result"
    miss, _ = _missing(lo, ow)
    assert miss == set(), miss


def test_tokens_cjk_char_level_and_bullets():
    lo = "\uf0b7 封面標題文字 IGCSE"
    ow = "封面標題\n文字"                               # CJK 換行位置不同不算缺；項目符號不算 token
    miss, _ = _missing(lo, ow)
    assert miss == {"igcse"}, miss


# ───────────────────────── 報告欄位 ─────────────────────────

def _base_row(**extra):
    row = {"file": "x.docx", "lo_pages": 1, "ow_pages": 1, "page_count_match": True,
           "content_scores": [1.0], "layout_scores": [1.0], "first_divergence": None,
           "notes": [], "measurement_artifacts": []}
    row.update(extra)
    return row


def test_report_has_j3_columns_and_header_total():
    row = _base_row(color_scores=[0.0], image_scores=[0.4],
                    true_missing=["IGCSE", "glossary"], true_missing_n=2, split_ok_n=1)
    with tempfile.TemporaryDirectory() as d:
        md = pathlib.Path(d) / "r.md"
        rl.render_report([row], {"ts": "t", "soffice": "s", "target": "x", "lo_fail": 0}, md)
        text = md.read_text(encoding="utf-8")
    assert "真缺字 2" in text.split("| 檔案")[0], "頁首欄需有「真缺字 n」"
    head = [l for l in text.splitlines() if l.startswith("| 檔案")][0]
    for col in ("色相似度", "圖相似度", "真缺字"):
        assert col in head, col
    # 既有欄位保留
    for col in ("LO 頁", "OW 頁", "內容相似度", "版面相似度", "首個內容分歧頁", "備註"):
        assert col in head, col
    assert "IGCSE" in text


def test_compare_old_fields_untouched():
    """J3 不得動 compare() 既有欄位語意：舊 key 全在、不新增 J3 key。"""
    lo = [{"lines": ["a b c"], "w_pt": 612.0, "h_pt": 792.0}]
    ow = {"pages": [{"lines": ["a b c"]}], "breaks": [{"width_pt": 612.0, "height_pt": 792.0}]}
    r = rl.compare("t", lo, ow)
    assert set(r) == {"file", "lo_pages", "ow_pages", "page_count_match", "content_scores",
                      "layout_scores", "first_divergence", "notes", "measurement_artifacts"}
    # 舊報告（無 J3 欄位的 row）仍可渲染
    with tempfile.TemporaryDirectory() as d:
        md = pathlib.Path(d) / "r.md"
        rl.render_report([r], {"ts": "t", "soffice": "s", "target": "x", "lo_fail": 0}, md)
        assert "| t |" in md.read_text(encoding="utf-8")


# ───────────────────────── LO 殭屍收場（必測） ─────────────────────────

def test_kill_own_lo_targets_only_own_profile():
    """kill_own_lo 的 PowerShell 篩選必須以本輪 profile 路徑（needle）為準，
    回傳殺掉個數；絕不可無差別 taskkill 使用者自己的 LO。"""
    seen = {}

    def fake_run(cmd, **kw):
        seen["cmd"] = cmd
        return subprocess.CompletedProcess(cmd, 0, stdout="3\n", stderr="")

    needle = "C:/Users/x/AppData/Local/Temp/quire-render-lab/profile"
    with mock.patch.object(rl.shutil, "which", return_value="powershell.exe"), \
            mock.patch.object(rl.subprocess, "run", side_effect=fake_run):
        n = rl.kill_own_lo(needle)
    assert n == 3
    ps = seen["cmd"][-1]
    assert needle in ps and "Stop-Process" in ps and "CommandLine" in ps


def test_kill_own_lo_safe_without_powershell_or_on_error():
    with mock.patch.object(rl.shutil, "which", return_value=None):
        assert rl.kill_own_lo("anything") == 0
    with mock.patch.object(rl.shutil, "which", return_value="powershell.exe"), \
            mock.patch.object(rl.subprocess, "run", side_effect=OSError("boom")):
        assert rl.kill_own_lo("anything") == 0       # 收場清理本身絕不可再拋例外


def test_main_registers_kill_own_lo_before_first_lo_call():
    """殭屍收場呼叫點：main() 必須在第一次拉起 LO 之前就 atexit 註冊
    kill_own_lo(needle)（崩潰/異常照清）。"""
    order = []
    with tempfile.TemporaryDirectory() as d:
        docx = pathlib.Path(d) / "a.docx"
        docx.write_bytes(b"PK")
        out = pathlib.Path(d) / "out"
        needle = "NEEDLE-PROFILE"

        def fake_register(fn, *args, **kw):
            order.append(("register", fn, args))

        def fake_lo(*a, **kw):
            order.append(("lo",))
            return None                                # lo_fail，不中斷

        with mock.patch.object(rl, "find_soffice", return_value=pathlib.Path("/x/soffice")), \
                mock.patch.object(rl, "lo_scratch", return_value=(pathlib.Path(d), "file:///p", needle)), \
                mock.patch.object(rl, "kill_stale_lo"), \
                mock.patch.object(rl, "lo_to_pdf", side_effect=fake_lo), \
                mock.patch.object(rl.atexit, "register", side_effect=fake_register), \
                mock.patch.object(sys, "argv", ["render_lab.py", str(docx), "--out", str(out)]):
            rl.main()
    kinds = [o[0] for o in order]
    assert "register" in kinds and "lo" in kinds
    assert kinds.index("register") < kinds.index("lo"), order
    reg = next(o for o in order if o[0] == "register")
    assert reg[1] is rl.kill_own_lo and reg[2] == (needle,), reg


def test_atexit_cleanup_fires_even_when_pipeline_crashes():
    """真實 interpreter 收場：lo_to_pdf 炸例外 → 進程異常結束，kill_own_lo 仍被呼叫。"""
    code = textwrap.dedent(f"""
        import importlib.util, pathlib, sys, tempfile
        from unittest import mock
        spec = importlib.util.spec_from_file_location("render_lab", r"{SCRIPTS / 'render_lab.py'}")
        rl = importlib.util.module_from_spec(spec); spec.loader.exec_module(rl)
        d = pathlib.Path(tempfile.mkdtemp()); (d / "a.docx").write_bytes(b"PK")
        rl.find_soffice = lambda: pathlib.Path("/x/soffice")
        rl.lo_scratch = lambda s: (d, "file:///p", "ZOMBIE-NEEDLE")
        rl.kill_stale_lo = lambda: None
        def boom(*a, **k): raise RuntimeError("LO 崩潰")
        rl.lo_to_pdf = boom
        rl.kill_own_lo = lambda needle: print("CLEANUP-CALLED:" + needle, flush=True)
        sys.argv = ["render_lab.py", str(d / "a.docx"), "--out", str(d / "out")]
        rl.main()
    """)
    r = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=60)
    assert r.returncode != 0, "pipeline 應因 LO 崩潰而異常結束"
    assert "CLEANUP-CALLED:ZOMBIE-NEEDLE" in r.stdout, (r.stdout, r.stderr[-300:])


# ───────────────────────── runner ─────────────────────────

def main() -> int:
    tests = [(n, f) for n, f in sorted(globals().items()) if n.startswith("test_") and callable(f)]
    fails = 0
    for name, fn in tests:
        try:
            fn()
            print(f"  PASS {name}")
        except Exception as e:   # noqa: BLE001 —— 紅燈要看到完整原因
            fails += 1
            print(f"  FAIL {name}: {type(e).__name__}: {e}")
            if "-v" in sys.argv:
                traceback.print_exc()
    print("J3-TESTS", "ALL-PASS" if fails == 0 else f"{fails}-FAIL", f"({len(tests)} cases)")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
