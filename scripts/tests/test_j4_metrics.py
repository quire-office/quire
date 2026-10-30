#!/usr/bin/env python3
"""J4 度量校準測試：四條已歸因偽影回歸鎖與防漂移驗證。

偽影 1：FILENAME 欄位渲染成 scratch 路徑 → 假缺字（2129393649）
偽影 2：圖軸口徑：LO 全矢量頁 vs 我方彩色 Region（WC048 案）
偽影 3：目錄點線黏連 token → 假缺字（fdo77129 How.....2）
偽影 4：Equation* OMML 缺字簇（真缺陷報告與交接規格，report-only）

執行方式：
  python3 scripts/tests/test_j4_metrics.py
"""

import importlib.util
import pathlib
import sys
import traceback
import numpy as np

SCRIPTS = pathlib.Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("render_lab", SCRIPTS / "render_lab.py")
rl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rl)


def blank(h=400, w=300):
    return np.full((h, w, 3), 255, dtype=np.uint8)


# ───────────────────────── 偽影 1：FILENAME scratch 路徑 ─────────────────────────

def test_artifact_1_filename_scratch_path_not_missing():
    """LO 實抽文字鐵證（corpus/ooxmlexport/2129393649.docx）：
    LO 頁首/頁尾將 OOXML <w:instrText> FILENAME \\p </w:instrText> 展開為本機臨時路徑：
    C:\\Users\\me\\AppData\\Local\\Temp\\quire-render-lab\\ooxmlexport__2129393649.docx
    token_audit 必須過濾該欄位產物，不得誤報為 true_missing。
    """
    lo_text = (
        "SOP Number: CP043.00\n"
        "Title: Procedure for Measuring Respiratory Rate\n"
        "CONTROLLED DOCUMENT-DO NOT DUPLICATE\n"
        "C:\\Users\\me\\AppData\\Local\\Temp\\quire-render-lab\\ooxmlexport__2129393649.docx\n"
        "I PURPOSE\n"
        "The purpose of measuring respiratory rate is to monitor the piglet."
    )
    ow_text = (
        "SOP Number: CP043.00\n"
        "Title: Procedure for Measuring Respiratory Rate\n"
        "CONTROLLED DOCUMENT-DO NOT DUPLICATE\n"
        "I PURPOSE\n"
        "The purpose of measuring respiratory rate is to monitor the piglet."
    )
    res = rl.token_audit(lo_text, ow_text)
    miss = res["true_missing"]
    assert miss == [], f"FILENAME 臨時路徑不應列入真缺字，實得: {miss}"
    field_artifacts = res.get("field_artifacts", [])
    assert any("2129393649" in a for a in field_artifacts), f"應標記 field_artifacts，實得: {field_artifacts}"


# ───────────────────────── 偽影 3：目錄點引導符黏連 ─────────────────────────

def test_artifact_3_toc_dot_leader_split_not_missing():
    """LO 實抽文字鐵證（corpus/ooxmlexport/fdo77129.docx p1）：
    LO 將目錄導引點與頁碼抽出為單一字串：
    How........................................................................................................................................................................2
    我方 dump 發射為 ['How', '.......2']。
    tokenize 必須在連續點/導引符處斷開，不得黏連誤報 true_missing。
    """
    lo_text = "How........................................................................................................................................................................2"
    ow_text = "How ...............................................................................................................................................2"
    res = rl.token_audit(lo_text, ow_text)
    miss = res["true_missing"]
    assert miss == [], f"目錄點線黏連詞不應列入真缺字，實得: {miss}"


# ───────────────────────── 偽影 2：圖軸口徑（WC048 案） ─────────────────────────

def test_artifact_2_wc048_vector_vs_image_calibration():
    """WC048 實測鐵證：
    LO 該頁零鑲嵌圖（全矢量 drawings=13, images=0）。
    我方 K12 修復後發射 3 個彩色矢量圖元（Rectangle 214 藍帶 Rect + 2 條 pBdr Rule），
    非 DrawItem::Image 鑲嵌圖。
    改前口徑：未排除矢量 Region，我方 Rect 跨過 400px 門檻被誤當 Image blob，致 1.0 -> 0.0。
    改後口徑：雙方皆無鑲嵌圖時圖分為 None（量測不可比/無圖可比）；
              1.0 必須只出現在「LO 有圖我方也有圖且匹配」的情形。
    """
    lo_img = blank()
    ow_img = blank()
    # 模擬我方 K12 發射的藍色向量 Rect (30x150 px, #4472C4, sat>=25, 4500px > 400px)
    ow_img[100:130, 50:200] = (0x44, 0x72, 0xC4)

    # (a) 雙方皆無鑲嵌圖（WC048 案：lo_rects=[], ow_rects=[]）⇒ 圖分應為 None，非 0.0 也非 1.0
    score_no_images = rl.score_image(lo_img, ow_img, lo_rects=[], ow_rects=[])
    assert score_no_images is None, f"雙方無鑲嵌圖時應為 None，實得: {score_no_images}"

    # (b) 雙方皆有鑲嵌圖且匹配時 ⇒ 圖分始得為 1.0
    matched_lo = blank()
    matched_lo[20:120, 20:180] = (200, 40, 40)
    matched_ow = blank()
    matched_ow[20:120, 20:180] = (200, 40, 40)
    rect = (20 * 72 / 150, 20 * 72 / 150, 180 * 72 / 150, 120 * 72 / 150)
    score_matched = rl.score_image(matched_lo, matched_ow, lo_rects=[rect], ow_rects=[rect])
    assert score_matched > 0.99, f"雙方有圖且匹配時應為 1.0，實得: {score_matched}"

    # (c) 向量 Rect 不應污染真正圖片比對
    # 我方即使另有向量 Rect（如 y=200..230 的藍色 Rect），只要圖片區域（y=20..120）匹配，圖片分仍應為 1.0
    ow_with_vector = matched_ow.copy()
    ow_with_vector[200:230, 50:200] = (0x44, 0x72, 0xC4)
    score_vector_masked = rl.score_image(matched_lo, ow_with_vector, lo_rects=[rect], ow_rects=[rect])
    assert score_vector_masked > 0.99, f"矢量 Rect 不應扣除圖分，實得: {score_vector_masked}"


def main() -> int:
    tests = [(n, f) for n, f in sorted(globals().items()) if n.startswith("test_") and callable(f)]
    fails = 0
    for name, fn in tests:
        try:
            fn()
            print(f"  PASS {name}")
        except Exception as e:
            fails += 1
            print(f"  FAIL {name}: {type(e).__name__}: {e}")
            if "-v" in sys.argv:
                traceback.print_exc()
    print("J4-TESTS", "ALL-PASS" if fails == 0 else f"{fails}-FAIL", f"({len(tests)} cases)")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
