"""K25-A：`render_lab.compare()` 防放水臂（4c／4d）的**繪製口徑**回歸鎖。

# 為什麼需要這個檔（不是重複 render_lab --selftest 的 4d）
`render_lab.py --selftest` 的 4d fixture 是**手寫 dict**：它直接寫死
`{"images": 1, "image_xrefs": [6]}`，所以**改 `pdf_page_facts()` 的口徑時它不會紅**
——它驗的是 `compare()` 對「給定 images 值」的反應，驗不到「真實 PDF 的 images 值
怎麼算出來」。本檔補的就是這一層：用**真實 PDF**（pymupdf 合成、可直接開來畫）
跑 `pdf_page_facts()` → `compare()`，把「繪製 vs 宣告」兩條口徑的界線鎖死。

# 四條鎖（皆以實檔 PDF 為輸入，非手寫 dict）
- 鎖 1：LO 該頁**真繪製**的重複頁首圖＋我方空 ⇒ 仍必須 0.0＋`empty_page_mismatch`
  （燈 2 要求的新增變體；4d 語意不得因改口徑而稀釋）。
- 鎖 2：LO 該頁**只宣告未繪製**（共用 Resources obj）＋我方同位空頁 ⇒ None＋
  `measurement_artifact:雙空同位頁(LO真空)`（n751077 p2 型；燈 1 的機制面）。
- 鎖 3：同檔 p0（真繪製圖＋文字）在改口徑後**逐位不動**——繪製口徑只刪殭屍宣告，
  絕不動到真內容頁。
- 鎖 4：口徑欄位本身——`images`／`image_xrefs` 只計繪製者，`images_declared`／
  `image_xrefs_declared` 保留宣告值（A/B 覆核與報告可追溯）。

執行（與既有 tests/ 同風格、零依賴）：
  python3 scripts/tests/test_k25a_painted_ink.py
  python3 -m pytest scripts/tests/test_k25a_painted_ink.py
"""

import importlib.util
import pathlib
import tempfile

import numpy as np
import pymupdf

SCRIPTS = pathlib.Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("render_lab", SCRIPTS / "render_lab.py")
rl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rl)

HDR_RECT = pymupdf.Rect(17, 60, 84, 119)   # n751077 LO p0 圖片 rect（近似值即可）


def _logo():
    pix = pymupdf.Pixmap(pymupdf.csRGB, pymupdf.IRect(0, 0, 40, 35))
    pix.set_rect(pix.irect, (200, 40, 40))
    return pix


def _shared_resources_pdf(path: pathlib.Path, *, paint_second: bool) -> pathlib.Path:
    """2 頁 PDF：p0＝圖＋TEXT1 TEXT2；p1＝空（或也畫圖）。

    `paint_second=False`：**兩頁共用同一個 Resources 間接物件**、p1 內容流零 `Do`
    ⇒ p1 `get_images()` 回報同一 xref（宣告）但 `get_image_rects()==[]`（未繪製）
    ＝ n751077（LO obj 41）的精確重現。
    `paint_second=True`：p1 **真繪製**同一張圖＝逐頁重複頁首圖（4d 保護對象）。
    """
    doc = pymupdf.open()
    logo = _logo()
    p0 = doc.new_page(width=612, height=792)
    p0.insert_image(HDR_RECT, pixmap=logo)
    p0.insert_text((70, 150), "TEXT1 TEXT2", fontsize=14)
    p0_xref = doc[0].xref
    if paint_second:
        p1 = doc.new_page(width=612, height=792)
        p1.insert_image(HDR_RECT, pixmap=logo)
    else:
        p1 = doc.new_page(width=612, height=792)
        res = doc.xref_get_key(p0_xref, "Resources")
        assert res[0] == "xref", f"預期 p0 Resources 為間接物件，實得 {res}"
        doc.xref_set_key(p1.xref, "Resources", res[1])  # 共用＝殭屍宣告的来源
    doc.save(path)
    doc.close()
    return path


def _ow(two_pages=True):
    """我方事實（n751077 態：p0 有 TEXT1/TEXT2，p1 零字形的結構空頁）。"""
    pages = [{"lines": ["TEXT1", "TEXT2"], "images": []}]
    if two_pages:
        pages.append({"lines": [], "images": []})
    return {"pages": pages,
            "breaks": [{"width_pt": 612.0, "height_pt": 792.0}] * len(pages),
            "n_svg": len(pages)}


# ───────────────────────── 事實層：繪製 ≠ 宣告 ─────────────────────────

def test_declared_and_painted_differ_on_shared_resources():
    """前提鎖：共用 Resources 的 p1 必須「宣告 1、繪製 0」，兩欄位分開如實回報。"""
    with tempfile.TemporaryDirectory() as td:
        pdf = _shared_resources_pdf(pathlib.Path(td) / "decl.pdf", paint_second=False)
        facts = rl.pdf_page_facts(pdf)
        assert len(facts) == 2, f"頁數 {len(facts)}"
        p0, p1 = facts
        assert p0["images"] == 1 and p0["image_xrefs"], f"p0 應有真繪製圖：{p0}"
        assert p1["images"] == 0, f"p1 未繪製，繪製口徑應為 0，實得 {p1['images']}"
        assert p1["image_xrefs"] == [], f"p1 繪製清單應空，實得 {p1['image_xrefs']}"
        assert p1["images_declared"] == 1, (
            f"p1 宣告口徑仍應如實回報 1（殭屍宣告可見才查得動），實得 {p1['images_declared']}")
        assert p1["image_xrefs_declared"] == p0["image_xrefs"], (
            f"兩頁應宣告同一 xref（共用 Resources），實得 {p1['image_xrefs_declared']}"
            f" vs {p0['image_xrefs']}")
        # 殭屍宣告的鐵證：該頁光柵零墨跡（不靠任何圖庫語意的獨立覆核）。
        d = pymupdf.open(pdf)
        pm = d[1].get_pixmap()
        arr = np.frombuffer(pm.samples, dtype=np.uint8).reshape(pm.height, pm.width, pm.n)
        d.close()
        assert int((arr < 250).any(axis=2).sum()) == 0, "p1 應零墨跡（LO 真空同形）"


def test_painted_header_repeats_are_counted_on_every_page():
    """真繪製的逐頁重複頁首圖：兩頁都必須計入（這才是 4d 保護的對象）。"""
    with tempfile.TemporaryDirectory() as td:
        pdf = _shared_resources_pdf(pathlib.Path(td) / "paint.pdf", paint_second=True)
        facts = rl.pdf_page_facts(pdf)
        assert [f["images"] for f in facts] == [1, 1], (
            f"真繪製的重複頁首圖兩頁都該計入，實得 {[f['images'] for f in facts]}")
        assert facts[0]["image_xrefs"] == facts[1]["image_xrefs"], (
            "重複頁首圖應為同一 xref 出現在兩頁（xref_pages==2 ⇒ 非 unique）")


# ───────────────────────── 鎖 1：真繪製重複頁首圖仍 0.0 ─────────────────────────

def test_lamp2_painted_repeated_header_still_zero():
    """燈 2 的新增變體：LO 該頁**真繪製**重複頁首圖＋我方空 ⇒ 0.0＋empty_page_mismatch。

    這條是改口徑的核心風險點：若有人把「繪製口徑」實作寬鬆成「該檔有圖就算」，
    或乾脆把雙空頁一律判 None（放水），本鎖立刻紅。
    """
    with tempfile.TemporaryDirectory() as td:
        pdf = _shared_resources_pdf(pathlib.Path(td) / "hdr.pdf", paint_second=True)
        lo = rl.pdf_page_facts(pdf)
        row = rl.compare("k25a:painted-header", lo, _ow())
        assert row["content_scores"][1] == 0.0, (
            f"真繪製重複頁首圖頁必須仍 0.0，實得 {row['content_scores']}")
        assert row["content_scores"][0] == 1.0, f"p0 內容分應 1.0：{row['content_scores']}"
        assert row["first_divergence"] == 1, f"首個分歧頁應為索引 1：{row['first_divergence']}"
        assert "empty_page_mismatch" in row["notes"], f"應保留真事故標籤：{row['notes']}"
        assert not row["measurement_artifacts"], (
            f"真事故頁不得記 measurement_artifact（那是放水）：{row['measurement_artifacts']}")


# ───────────────────────── 鎖 2：殭屍宣告同位雙空 ⇒ None ─────────────────────────

def test_lamp1_shared_resources_void_page_is_none_not_zero():
    """n751077 p2 型：LO 該頁只宣告未繪製＋我方同位空 ⇒ None＋雙空同位頁 artifact。

    改口徑前本檔會紅（`images=1` 擋死 4b ⇒ 0.0＋empty_page_mismatch）。
    """
    with tempfile.TemporaryDirectory() as td:
        pdf = _shared_resources_pdf(pathlib.Path(td) / "void.pdf", paint_second=False)
        lo = rl.pdf_page_facts(pdf)
        row = rl.compare("k25a:void-p2", lo, _ow(), sect_pr=2)
        assert row["content_scores"] == [1.0, None], (
            f"應為 [1.0, None]（0.0 是度量偽影），實得 {row['content_scores']}")
        # 版面軸：p1 同為 None；p0 因「一行 vs 我方兩行」的換行位置差 <1.0（既有口徑，非本卡變更）
        assert row["layout_scores"][1] is None, f"版面同口徑應 None：{row['layout_scores']}"
        assert row["first_divergence"] is None, f"不記分歧：{row['first_divergence']}"
        assert "empty_page_mismatch" not in row["notes"], (
            f"偽影標籤必消失：{row['notes']}")
        assert any(n.startswith("measurement_artifact:雙空同位頁") for n in row["notes"]), (
            f"應帶雙空同位頁 artifact：{row['notes']}")
        assert any("w:sectPr" in n for n in row["notes"]), f"sectPr 實據應在 notes：{row['notes']}"
        arts = row["measurement_artifacts"]
        assert arts and arts[0]["page"] == 1 and arts[0]["reason"] == "both_empty_same_position", (
            f"artifact 實據應如實（page=1/reason）：{arts}")
        # 檔分：None 不入平均 ⇒ 0.500 → 1.000（lo_scoreboard.file_score 口徑）
        scored = [s for s in row["content_scores"] if s is not None]
        assert scored == [1.0], f"檔分應由 0.5 升為 1.0：{row['content_scores']}"


# ───────────────────────── 鎖 3：真內容頁逐位不動 ─────────────────────────

def test_lamp3_real_content_page_untouched_by_new_measure():
    """p0（真繪製圖＋文字）在繪製口徑下逐位不變：新口徑只刪殭屍宣告。"""
    with tempfile.TemporaryDirectory() as td:
        for paint in (False, True):
            pdf = _shared_resources_pdf(pathlib.Path(td) / f"c{paint}.pdf", paint_second=paint)
            facts = rl.pdf_page_facts(pdf)
            p0 = facts[0]
            assert p0["line_count"] == 1 and p0["lines"] == ["TEXT1 TEXT2"], (
                f"p0 文字事實不應變：{p0['lines']}")
            assert p0["images"] == 1 and p0["images_declared"] == 1, f"p0 圖應繪製：{p0}"
            assert p0["drawings"] == facts[0]["drawings"]
            assert p0["w_pt"] == 612.0 and p0["h_pt"] == 792.0, f"頁尺寸不應變：{p0}"
            # p0 詞序與我方同 ⇒ 內容分恒等 1.0（改口徑前後同值）
            row = rl.compare("k25a:content", facts, _ow())
            assert row["content_scores"][0] == 1.0, (
                f"paint_second={paint} p0 內容分應 1.0：{row['content_scores']}")


# ───────────────────────── 回塞鎖：BUGGY 態必紅 ─────────────────────────

def test_buggy_revert_must_turn_lamps_red():
    """把口徑還原成「宣告」（BUGGY 態）時鎖 2 必紅——證明本鎖有鑑別力、非假綠。"""
    buggy = []

    def buggy_facts(pdf):
        d = pymupdf.open(pdf)
        out = []
        for p in d:
            lines = [rl.norm(t) for t in str(p.get_text("text")).splitlines() if rl.norm(t)]
            imgs = p.get_images(full=True)
            out.append({"lines": lines, "line_count": len(lines), "words": 0,
                        "w_pt": round(p.rect.width, 1), "h_pt": round(p.rect.height, 1),
                        "drawings": len(p.get_drawings()),
                        "images": len(imgs), "image_xrefs": [im[0] for im in imgs]})
        d.close()
        return out

    with tempfile.TemporaryDirectory() as td:
        void = _shared_resources_pdf(pathlib.Path(td) / "b-void.pdf", paint_second=False)
        hdr = _shared_resources_pdf(pathlib.Path(td) / "b-hdr.pdf", paint_second=True)

        # BUGGY：殭屍宣告當內容 ⇒ 真空頁被誤判 0.0（就是本卡修的缺陷）
        row = rl.compare("buggy:void", buggy_facts(void), _ow())
        buggy.append(row["content_scores"] == [1.0, 0.0] and "empty_page_mismatch" in row["notes"])

        # BUGGY 態下鎖 1 仍必須綠（真繪製頁在兩條口徑下都該 0.0）——
        # 證明「鎖 2 紅」不是連帶誤傷，而是精確命中殭屍宣告那一類。
        row2 = rl.compare("buggy:hdr", buggy_facts(hdr), _ow())
        buggy.append(row2["content_scores"][1] == 0.0)

    assert buggy[0], "BUGGY（宣告口徑）下鎖 2 未紅 ⇒ 本卡紅燈沒有鑑別力"
    assert buggy[1], "BUGGY 下鎖 1 竟紅 ⇒ 兩條口徑未被區隔"


def _run():
    tests = [(n, f) for n, f in sorted(globals().items())
             if n.startswith("test_") and callable(f)]
    fails = 0
    for name, fn in tests:
        try:
            fn()
            print(f"  PASS {name}")
        except Exception as e:  # noqa: BLE001
            fails += 1
            print(f"  FAIL {name}: {type(e).__name__}: {e}")
    print("K25A PAINTED-INK", "ALL-PASS" if fails == 0 else f"{fails}-FAIL")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    raise SystemExit(_run())
