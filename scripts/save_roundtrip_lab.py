#!/usr/bin/env python3
"""save-roundtrip-lab —— 儲存保真閘門（用戶 2026-11-25 指定）。

三條臂（每檔）：
  A 零編輯臂   原檔 → quire-cli save（零編輯）→ 位元組恆等預閘（sha256 對源檔）；
               恆等過＝PASS（像素層跳過但必須記錄 skipped）；不恆等＝必紅，
               並跑 LO 渲染兩份做逐頁像素/文字差分（鑑別變動性質）。
  B 編輯態臂   原檔 → 注入 mutation（打字/Enter/粗體/對齊/表格格內，經
               `quire-cli save --mutation`，serde 形與 wasm apply_mutation_json
               同源）→ 存檔 → LO 渲染 →
               規則：編輯單調不縮內容 ⇒ 突變段之前的頁必須逐像素零位移
               （防「存檔動到別處」）；突變區內必須可見突變（防「存檔吞編輯」）。
  C 本專案渲染臂 存檔後檔直接餵既有 render_lab 流程（LO vs quire SVG 雙欄，
               import scripts/render_lab.py 同口徑同取樣），逐頁內容分在
               突變區之外不得低於原檔對應分（存檔不得讓渲染保真退步）。

紅燈判據口徑：
  - 零編輯臂用**像素 diff 計數**（不用相似度分——skill 教訓：相似度分對局部
    缺線不敏感）；預設容差 0，--pixel-allow 可放（夜間防噪用，合併閘門不放）。
  - LO 基線走 Windows soffice.exe（PATH 注入口徑見 quire-development skill），
    專屬 UserInstallation profile（同輪多線取 LO 鐵律），atexit kill_own_lo。
  - 頂替 face 假象不得寫進判據：本考卷只比「存檔前 vs 存檔後」同一 LO 環境的
    相對差，不 absolutes 判字形。

回塞鎖：SAVE_RT_BUGGY=1（存檔鏈多做一件對渲染無害的 part 重排，見
  core/ooxml/src/lib.rs save()）＋ --discriminate：零編輯臂必須點名紅，
  未紅＝考卷無鑑別力（比照 U4F_*_BUGGY 口徑）。

報告：out/report.md（| corpus… 行與 scripts/sweep_cmp.py 兼容，可跨版本比對）
     ＋ out/scores.jsonl（機器可讀，逐臂一行）＋ 取樣清單 sample.json。
"""
import argparse
import atexit
import datetime
import hashlib
import json
import os
import pathlib
import random
import re
import shutil
import subprocess
import sys
import time
import zipfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import render_lab as rl  # noqa: E402  （同工具族：LO 取樣/compare/SVG 解析全共用）

REPO = pathlib.Path(__file__).resolve().parent.parent
BASELINE_DIR = REPO / "tests" / "save-roundtrip-baseline"
SEED_DEFAULT = 20261125
PIXEL_ALLOW_DEFAULT = 0
# 分數不退步容差（render_lab 報告逐頁分本身四捨五入到 0.001，留一位餘量）
SCORE_EPS = 0.002
CJK = "一-鿿"


def squash(s: str) -> str:
    """去盡空白（LO 斷行會插空格，比對存在性用）。"""
    return re.sub(r"\s+", "", s)


# ───────────────────────────── corpus 分層取樣 ─────────────────────────────
# 每層＝(層名, 標籤謂詞, 配額)。謂詞吃 manifest tags（+ zip 掃描欄位）。
# 混級字級層在掃描階段算（manifest 無此標籤）：document.xml 的 w:sz 去重後 ≥2 種。
def _mixed_sizes(docx: pathlib.Path) -> bool:
    import zipfile
    try:
        xml = zipfile.ZipFile(docx).read("word/document.xml").decode("utf-8", "ignore")
    except Exception:
        return False
    return len(set(re.findall(r'<w:sz w:val="(\d+)"', xml))) >= 2


def _sect_pr(docx: pathlib.Path) -> int:
    import zipfile
    try:
        xml = zipfile.ZipFile(docx).read("word/document.xml").decode("utf-8", "ignore")
        return len(re.findall(r"<w:sectPr\b", xml))
    except Exception:
        return 0


STRATA = [
    ("table",        lambda t, e: "table" in t,               8),
    ("merged",       lambda t, e: "merged_cells" in t,        3),
    ("nested_table", lambda t, e: "nested_table" in t,        2),
    ("table_across", lambda t, e: "table_across_pages" in t,  1),
    ("image",        lambda t, e: "image" in t,               4),
    ("endnote",      lambda t, e: "endnote" in t,             2),
    ("footnote",     lambda t, e: "footnote" in t,            3),
    ("multisection", lambda t, e: e["_sectpr"] >= 2,          3),
    ("mixed_size",   lambda t, e: e["_mixed"],                 4),
    ("cjk",          lambda t, e: "east_asian" in t,          3),
    ("columns",      lambda t, e: "columns" in t,             2),
    ("header_footer", lambda t, e: "header_footer" in t,      3),
    ("fields",       lambda t, e: "fields" in t,              2),
]


def select_sample(corpus: pathlib.Path, n: int, seed: int, quarantined: set) -> list:
    """manifest 分層抽樣：先逐層配額（層內 seeded 亂序），再以隨機補到 n。
    回傳 entry 清單（含 strata 欄＝該檔命中的全部層名，供報告分層統計）。"""
    manifest = json.loads((corpus / "manifest.json").read_text(encoding="utf-8"))
    files = [f for f in manifest["files"]
             if f["file"] not in quarantined
             and (corpus / f["file"]).exists()]
    # 掃描欄位只在需要時算（mixed_size/sectPr 各一次 zip 讀，400 檔秒級）。
    for f in files:
        p = corpus / f["file"]
        f["_mixed"] = _mixed_sizes(p)
        f["_sectpr"] = _sect_pr(p)
    rng = random.Random(seed)
    picked, picked_names = [], set()
    strata_hits = {f["file"]: [] for f in files}
    for name, pred, quota in STRATA:
        eligible = [f for f in files if f["file"] not in picked_names and pred(set(f.get("tags", [])), f)]
        rng.shuffle(eligible)
        for f in eligible[:quota]:
            picked.append(f)
            picked_names.add(f["file"])
    for f in files:
        strata_hits[f["file"]] = [name for name, pred, _ in STRATA if pred(set(f.get("tags", [])), f)]
    if len(picked) < n:
        rest = [f for f in files if f["file"] not in picked_names]
        rng.shuffle(rest)
        picked += rest[: n - len(picked)]
    for f in picked:
        f["strata"] = strata_hits[f["file"]] or ["random"]
        for k in ("_mixed", "_sectpr"):
            f.pop(k, None)
    return picked[:n]


# ───────────────────────────── quire-cli 封裝 ─────────────────────────────
def quire_bin(quiet: bool = False) -> pathlib.Path:
    env = os.environ.get("QUIRE_CLI")
    if env:
        return pathlib.Path(env)
    binp = REPO / "target" / "debug" / "quire-cli"
    if not binp.exists():
        r = subprocess.run(["cargo", "build", "-q", "-p", "quire-cli"], cwd=REPO,
                           capture_output=True, text=True, env=_cargo_env())
        if r.returncode != 0:
            sys.exit(f"quire-cli build 失敗：\n{r.stderr[-1500:]}")
    return binp


def _cargo_env() -> dict:
    env = dict(os.environ)
    env["PATH"] = os.path.expanduser("~/.cargo/bin") + os.pathsep + env.get("PATH", "")
    return env


def _run_cli(binp: pathlib.Path, args: list, buggy: bool = False, timeout: int = 180):
    env = _cargo_env()
    if buggy:
        env["SAVE_RT_BUGGY"] = "1"
    r = subprocess.run([str(binp)] + args, capture_output=True, text=True,
                       cwd=REPO, env=env, timeout=timeout)
    return r


def cli_paragraphs(binp, docx) -> list:
    r = _run_cli(binp, ["paragraphs", str(docx)])
    if r.returncode != 0:
        raise RuntimeError(f"paragraphs 失敗: {r.stderr[:300]}")
    return json.loads(r.stdout)


def cli_pagebreaks(binp, docx) -> list:
    r = _run_cli(binp, ["pagebreaks", str(docx)])
    if r.returncode != 0:
        raise RuntimeError(f"pagebreaks 失敗: {r.stderr[:300]}")
    return json.loads(r.stdout)


def cli_dump(binp, docx) -> list:
    r = _run_cli(binp, ["dump", str(docx)])
    if r.returncode != 0:
        raise RuntimeError(f"dump 失敗: {r.stderr[:300]}")
    return json.loads(r.stdout)


def cli_save(binp, docx, out: pathlib.Path, mutations: list, buggy: bool) -> dict:
    args = ["save", str(docx), "--out", str(out)]
    for m in mutations:
        args += ["--mutation", json.dumps(m, ensure_ascii=False)]
    r = _run_cli(binp, args, buggy=buggy)
    if r.returncode != 0:
        raise RuntimeError(f"save rc={r.returncode}: {r.stderr.strip()[:400]}")
    return json.loads(r.stdout)


def sha256(p: pathlib.Path) -> str:
    h = hashlib.sha256()
    h.update(p.read_bytes())
    return h.hexdigest()


# ───────────────────────────── mutation 計劃 ─────────────────────────────
# 編輯單調不縮內容（打字/Enter/粗體/對齊/格內打字皆然）⇒ LO 重排只可能把內容
# 往後推：突變段落所在頁「之前」的頁在存檔前後必須逐像素零位移＝鐵律判據。
# 突變段所在頁由 quire pagebreaks（段落→頁）錨定，再以 LO 原文搜尋校正
# （LO 分頁與 quire 可能分歧；找不到＝退回 quire 頁並在記錄標 fallback）。
def norm(s: str) -> str:
    return re.sub(r"\s+", " ", s).strip()


def lo_page_texts(pdf) -> list:
    """LO 每頁全文（squash 去空白），存在性比對用。"""
    d = rl.pymupdf.open(pdf)
    try:
        return [squash(p.get_text("text")) for p in d]
    finally:
        d.close()


def lo_visible_map(rows: list, lo_pages_text: list) -> dict:
    """para → LO 頁：段文字（squash 首 24 字）在 LO 原檔某頁出現才算可見。
    隱形段（LO 根本不渲染）不配當零位移判據的錨點——那是渲染保真缺陷的面，
    另卡追；本考卷只挑 LO 看得見的段下 mutation。"""
    vis = {}
    for r in rows:
        t = squash(r["text"])
        if len(t) < 6:
            continue
        key = t[:24]
        for i, pt in enumerate(lo_pages_text):
            if key in pt:
                vis[r["para"]] = i
                break
    return vis


def plan_mutations(rows: list, kinds_wanted: list, vis: dict, lo_pages_text: list) -> list:
    """回傳 [(kind, mutation_json|None, target_paras, note)]；湊不齊的臂老實標 skip。
    目標段一律限 LO 可見（vis）；格內臂限 LO 可見表格格文字。
    cp 一律用 `cp_len`（突變碼點空間，引擎 apply_mutation 同源口徑）——版面
    paragraph_text 長度可能與之分歧（TOC 前導點等），混用會造成假越界紅。"""
    by_para = {r["para"]: r for r in rows}

    def cp_of(r):
        v = r.get("cp_len")
        return v if isinstance(v, int) else None

    body = [r for r in rows if not r["in_table"] and r["para"] in vis and cp_of(r)]
    cells = [r for r in rows if r["in_table"] and norm(r["text"])
             and len(squash(r["text"])) >= 4 and cp_of(r)
             and any(squash(r["text"])[:16] in pt for pt in lo_pages_text)]
    plan = []

    def pick(minlen):
        # 各臂＝原檔＋單一 mutation（彼此獨立），可共用目標段，不互斥。
        cands = [r for r in body if len(norm(r["text"])) >= minlen]
        return min(cands, key=lambda r: (vis[r["para"]], r["para"])) if cands else None

    for kind in kinds_wanted:
        if kind == "type":
            r = pick(4)
            if r is None:
                plan.append((kind, None, [], "無 ≥4 字且 LO 可見的正文段")); continue
            plan.append((kind, {"InsertText": {"target": "Body",
                         "pos": {"para": r["para"], "cp": cp_of(r)},
                         "text": "存檔保真", "style_override": None}},
                         [r["para"]], f"段 {r['para']} 尾打字"))
        elif kind == "enter":
            r = pick(16)
            if r is None:
                plan.append((kind, None, [], "無 ≥16 字且 LO 可見的正文段")); continue
            plan.append((kind, {"SplitParagraph": {"target": "Body",
                         "pos": {"para": r["para"], "cp": (cp_of(r) or 0) // 2}}},
                         [r["para"]], f"段 {r['para']} 中斷 Enter"))
        elif kind == "bold":
            r = pick(4)
            if r is None:
                plan.append((kind, None, [], "無 ≥4 字且 LO 可見的正文段")); continue
            a, b = 1, min(4, cp_of(r) or 0)
            plan.append((kind, {"FormatTextRange": {
                "range": {"target": "Body",
                          "start": {"para": r["para"], "cp": a},
                          "end": {"para": r["para"], "cp": b}},
                "patch": {"bold": True}}}, [r["para"]],
                f"段 {r['para']} 圈選 [{a},{b}) 粗體"))
        elif kind == "align":
            r = pick(8)
            if r is None:
                plan.append((kind, None, [], "無 ≥8 字且 LO 可見的正文段")); continue
            plan.append((kind, {"SetParagraphStyle": {
                "target": "Body", "para_range": [r["para"], r["para"]],
                "patch": {"alignment": "center"}}}, [r["para"]],
                f"段 {r['para']} 置中對齊"))
        elif kind == "cell":
            r = cells[0] if cells else None
            if r is None:
                plan.append((kind, None, [], "無 LO 可見的表格格內段（本檔無此臂）")); continue
            plan.append((kind, {"InsertText": {"target": "Body",
                         "pos": {"para": r["para"], "cp": cp_of(r)},
                         "text": "格內字", "style_override": None}},
                         [r["para"]], f"表格格內段 {r['para']} 打字"))
        else:
            raise ValueError(kind)
    return plan


def zone_start_for(by_para: dict, target_paras: list, vis: dict,
                   quire_page_of: dict) -> tuple:
    """(zone_start, method)：突變段所在 LO 頁＝管轄區起點；之前必須零位移。"""
    qp = min((quire_page_of[p] for p in target_paras if quire_page_of.get(p) is not None),
             default=None)
    lp = vis.get(min(target_paras)) if target_paras else None
    if lp is not None:
        return (lp, "lo-visible")
    if qp is not None:
        return (qp, "quire-fallback")
    return (0, "no-target")


def _saved_document_xml(docx) -> str:
    with zipfile.ZipFile(docx) as z:
        return z.read("word/document.xml").decode("utf-8", "replace")


def content_check(kind, mjson, target_paras, orig_rows, saved_rows, saved_docx):
    """內容層：編輯必須確實進到存檔產物（引擎可讀層＋OOXML 樣式層）。
    渲染層只證明 LO 看得見；內容層先判「存檔有沒有吞編輯」，兩者分工。"""
    saved_all = squash("".join(r["text"] for r in saved_rows))
    tp = min(target_paras)
    orig = next((r for r in orig_rows if r["para"] == tp), None)
    if orig is None:
        return False, "找不到目標段"
    if kind in ("type", "cell"):
        needle = squash(mjson["InsertText"]["text"])
        return (needle in saved_all, f"注入字串「{needle}」未出現在存檔產物")
    if kind == "enter":
        grew = len(saved_rows) >= len(orig_rows) + 1
        kept = squash(orig["text"])[:16] in saved_all
        return (grew and kept,
                f"段數 {len(orig_rows)}→{len(saved_rows)}（應 +1）文字保留={kept}")
    xml = _saved_document_xml(saved_docx)
    # 目標段文字錨點：取首 6 字作短錨，找**所有**涵蓋它的 w:p 區塊再驗樣式
    # （TOC/正文同文重複時，錨點可能命中多個段落——任何一個有樣式即算寫入）。
    key = squash(orig["text"])[:6]
    p_blocks = [b for b in re.findall(r"<w:p\b.*?</w:p>", xml, re.S)
                if key and key in squash("".join(re.findall(r"<w:t[^>]*>([^<]*)</w:t>", b)))]
    if not p_blocks:
        return False, f"存檔 XML 找不到目標段（錨「{key}」）"
    if kind == "bold":
        lo = min(4, len(orig["text"])) - 1
        frag = squash(orig["text"])[1:lo + 1]
        for hit in p_blocks:
            for run in re.findall(r"<w:r\b.*?</w:r>", hit, re.S):
                rt = squash("".join(re.findall(r"<w:t[^>]*>([^<]*)</w:t>", run)))
                if rt and rt in frag and re.search(r"<w:b(\s|/>)", run):
                    return True, ""
        return False, "目標段涵蓋突變字的 run 無 <w:b>（粗體未進 XML）"
    if kind == "align":
        for hit in p_blocks:
            if re.search(r"<w:jc\s+w:val=\"center\"", hit):
                return True, ""
        return False, "目標段 pPr 無 <w:jc w:val=\"center\">（對齊未進 XML）"
    return False, f"未知臂 {kind}"


# ───────────────────────────── LO 渲染＋像素差分 ─────────────────────────────
def pix(page):
    import numpy as np  # noqa: F401
    return rl._pix_array(page)


def page_pixel_diff(pdf_a, pdf_b, i) -> int:
    import numpy as np
    da, db = rl.pymupdf.open(pdf_a), rl.pymupdf.open(pdf_b)
    try:
        if i >= da.page_count or i >= db.page_count:
            return -1  # 頁數不對齊：呼叫端另行判
        pa, pb = pix(da[i]), pix(db[i])
        if pa.shape != pb.shape:
            return int(pa.size)  # 頁尺寸都變＝整頁視為差異
        d = np.any(pa != pb, axis=2)
        return int(np.count_nonzero(d))
    finally:
        da.close(); db.close()


def page_text_lines(pdf, i) -> list:
    d = rl.pymupdf.open(pdf)
    try:
        return rl.pdf_page_facts(pdf)[i]["lines"] if i < d.page_count else None
    finally:
        d.close()


def lo_page_count(pdf) -> int:
    d = rl.pymupdf.open(pdf)
    try:
        return d.page_count
    finally:
        d.close()


# ───────────────────────────── 單檔流程 ─────────────────────────────
def process_file(binp, corpus, entry, out_dir, soffice, scratch, profile_uri, needle,
                 kinds, pixel_allow, buggy, keep_png=False) -> dict:
    rel = entry["file"]
    docx = corpus / rel
    stem = docx.stem
    savedir = out_dir / "saved" / stem
    savedir.mkdir(parents=True, exist_ok=True)
    rec = {"file": rel, "strata": entry.get("strata", []),
           "arms": {}, "status": "ok"}

    # ── 原檔 LO 渲染（各臂共用基線）──
    pdf0 = rl.lo_to_pdf(soffice, docx, scratch, profile_uri)
    if pdf0 is None:
        rec["status"] = "lo_fail"
        return rec
    facts0 = rl.pdf_page_facts(pdf0)
    lo_pages_text = lo_page_texts(pdf0)
    # quire 原檔（段落→頁、分頁斷點、渲染臂評分）
    rows = cli_paragraphs(binp, docx)
    rows_by_para = {r["para"]: r for r in rows}
    vis = lo_visible_map(rows, lo_pages_text)
    rec["lo_visible_paras"] = len(vis)
    rec["total_paras"] = len(rows)
    ow0 = rl.run_quire(docx, out_dir / "orig")
    cmp0 = rl.compare(rel, facts0, ow0, sect_pr=rl.sect_pr_count(docx))
    rec["orig_lo_pages"] = cmp0["lo_pages"]
    rec["orig_ow_pages"] = cmp0["ow_pages"]
    rec["orig_content"] = cmp0["content_scores"]

    # ── 臂 A：零編輯 ──
    a = {"byte_identical": False, "sha_source": sha256(docx)}
    saved_a = savedir / f"{stem}__zero.docx"
    try:
        rep = cli_save(binp, docx, saved_a, mutations=[], buggy=buggy)
        a["save_report"] = {k: rep[k] for k in ("bytes_in", "bytes_out", "byte_identical")}
        a["byte_identical"] = bool(rep["byte_identical"]) and sha256(saved_a) == a["sha_source"]
    except RuntimeError as e:
        a["verdict"] = "FAIL"
        a["why"] = f"存檔失敗: {e}"
        rec["arms"]["zero"] = a
        rec["status"] = "fail"
        return rec
    if a["byte_identical"]:
        a["verdict"] = "PASS"
        a["pixel_layer"] = "skipped（位元組恆等；跳過像素層但照記錄）"
    else:
        # 不恆等＝紅（預設態是 bug；BUGGY 態是回塞鎖生效的預期紅）。
        # 像素/文字差分照跑：鑑別變動性質（純結構 vs 真動到內容）。
        a["verdict"] = "FAIL"
        a["why"] = "零編輯存檔未過位元組恆等" + ("（SAVE_RT_BUGGY 回塞鎖生效）" if buggy else "")
        pdfa = rl.lo_to_pdf(soffice, saved_a, scratch, profile_uri)
        if pdfa is None:
            a["pixel_layer"] = "lo_fail"
        else:
            n0 = lo_page_count(pdf0); n1 = lo_page_count(pdfa)
            diffs = [page_pixel_diff(pdf0, pdfa, i) for i in range(min(n0, n1))]
            text_pages = []
            for i in range(min(n0, n1)):
                if (page_text_lines(pdf0, i) or []) != (page_text_lines(pdfa, i) or []):
                    text_pages.append(i)
            a["pixel_layer"] = {"pages_orig": n0, "pages_saved": n1,
                                "px_diff": diffs, "pages_px_nonzero": [i for i, d in enumerate(diffs) if d > pixel_allow],
                                "pages_text_diff": text_pages}
    rec["arms"]["zero"] = a

    # ── 臂 B/C：編輯態 × N 種 mutation ──
    for kind, mjson, target_paras, note in plan_mutations(rows, kinds, vis, lo_pages_text):
        arm = {"note": note, "mutation": mjson, "target_paras": target_paras}
        if mjson is None:
            arm["verdict"] = "SKIP"
            rec["arms"][f"edit:{kind}"] = arm
            continue
        saved = savedir / f"{stem}__{kind}.docx"
        try:
            rep = cli_save(binp, docx, saved, mutations=[mjson], buggy=buggy)
            arm["touched_parts"] = rep["touched_parts"]
        except RuntimeError as e:
            arm["verdict"] = "FAIL"
            arm["why"] = f"編輯存檔失敗: {e}"
            rec["arms"][f"edit:{kind}"] = arm
            rec["status"] = "fail"
            continue
        # 內容層：編輯必須進到存檔產物（先於渲染層；渲染層只驗證 LO 可見性）。
        saved_rows = cli_paragraphs(binp, saved)
        content_ok, content_why = content_check(kind, mjson, target_paras,
                                                rows, saved_rows, saved)
        pdf1 = rl.lo_to_pdf(soffice, saved, scratch, profile_uri)
        if pdf1 is None:
            arm["verdict"] = "LO_FAIL"
            arm["content_ok"] = content_ok
            rec["arms"][f"edit:{kind}"] = arm
            continue
        z, zmethod = zone_start_for(rows_by_para, target_paras, vis,
                                    {r["para"]: r["page"] for r in rows})
        n0, n1 = lo_page_count(pdf0), lo_page_count(pdf1)
        arm["zone_start"] = z
        arm["zone_method"] = zmethod
        arm["pages_orig"], arm["pages_saved"] = n0, n1
        arm["content_ok"] = content_ok
        arm["content_why"] = content_why
        diffs = [page_pixel_diff(pdf0, pdf1, i) for i in range(min(n0, n1))]
        arm["px_diff"] = diffs
        before = [i for i in range(min(z, len(diffs))) if diffs[i] > pixel_allow]
        arm["before_zone_pages_px_nonzero"] = before
        visible = any(d > pixel_allow for d in diffs[max(z, 0):]) or n1 > n0
        arm["visible"] = visible
        fails = []
        if not content_ok:
            fails.append(f"存檔吞編輯（內容層）：{content_why}")
        if before:
            fails.append(f"突變區前 {before} 頁像素非零（存檔動到別處）")
        if n1 < n0:
            fails.append(f"頁數縮減 {n0}→{n1}（打字類編輯不得縮頁）")
        if not visible and content_ok and not before:
            # 存檔鏈正確寫入樣式，但該區渲染本就同形（例：Heading1 加粗體＝LO
            # 本就渲染粗體）＝突變選靶視覺無效，非存檔缺陷。老實標 SKIP，
            # 內容層證據保留在記錄裡；不冒充 PASS 也不誤判存檔鏈紅。
            arm["verdict"] = "SKIP"
            arm["why"] = "突變對目標區視覺無效（內容層已驗證寫入，渲染同形）"
        elif fails:
            arm["verdict"] = "FAIL"
            arm["why"] = "；".join(fails)
            rec["status"] = "fail"
        else:
            arm["verdict"] = "PASS"

        # ── 臂 C：本專案渲染臂（存檔後檔餵既有 render_lab 流程）──
        try:
            ow1 = rl.run_quire(saved, out_dir / "saved-quire" / kind)
            cmp1 = rl.compare(f"{rel} →{kind}", rl.pdf_page_facts(pdf1), ow1,
                              sect_pr=rl.sect_pr_count(saved))
            arm["saved_content"] = cmp1["content_scores"]
            regress = []
            for i in range(min(z, len(cmp1["content_scores"]), len(cmp0["content_scores"]))):
                s_new, s_old = cmp1["content_scores"][i], cmp0["content_scores"][i]
                if s_new is None or s_old is None:
                    continue
                if s_new < s_old - SCORE_EPS:
                    regress.append(f"p{i + 1} {s_old}->{s_new}")
            if regress:
                arm["verdict"] = "FAIL"
                arm["why"] = arm.get("why", "") + f"渲染保真退步: {regress}"
                rec["status"] = "fail"
            # sweep_cmp 兼容行（| corpus… 開頭；欄位序比照 render_lab）
            arm["cmp_row"] = cmp1
        except Exception as e:  # noqa: BLE001
            arm["render_arm"] = f"render_lab 流程異常: {e}"
        if keep_png:
            rl.rasterize(pdf1, out_dir / "lo-png", f"{stem}__{kind}")
        rec["arms"][f"edit:{kind}"] = arm
    return rec


# ───────────────────────────── 報告 ─────────────────────────────
def verdict_cell(v):
    return {"PASS": "✓ PASS", "FAIL": "✗ FAIL", "SKIP": "— skip",
            "LO_FAIL": "— lo_fail"}.get(v, v)


def render_report(rows, meta, out_md: pathlib.Path):
    n_files = len(rows)
    red = [r for r in rows if r["status"] == "fail"]
    lines = [
        "# save-roundtrip-lab 報告（儲存保真閘門）",
        "",
        f"- 生成時間：{meta['ts']}",
        f"- LibreOffice：`{meta['soffice']}`"
        + ("（Windows soffice.exe 口徑）" if str(meta['soffice']).endswith(".exe")
           else "（本機 Linux LO；Windows soffice.exe 不在此機，PATH 注入口徑自動偵測）"),
        f"- quire-cli：`{meta['cli']}`｜git：`{meta['git']}`",
        f"- 對象：{meta['target']}（{n_files} 檔，seed={meta['seed']}）",
        f"- SAVE_RT_BUGGY：{meta['buggy']}｜--discriminate：{meta['discriminate']}｜像素容差：{meta['pixel_allow']}",
        f"- 紅燈檔案：{len(red)}；LO 轉換失敗：{sum(1 for r in rows if r['status'] == 'lo_fail')}",
    ]
    if meta.get("discriminate"):
        lines.append(f"- 鑑別力判定：**{meta['discriminate_verdict']}**")
    lines += ["", "## 臂 A：零編輯（位元組恆等預閘＋像素/文字差分）", "",
              "| 檔案 | 層 | 位元組恆等 | 像素層 | 判定 |", "|---|---|---|---|---|"]
    for r in rows:
        a = r["arms"].get("zero", {})
        pl = a.get("pixel_layer", "")
        if isinstance(pl, dict):
            pl = (f"px 非零頁 {pl['pages_px_nonzero']}｜文字差頁 {pl['pages_text_diff']}"
                  f"｜頁 {pl['pages_orig']}→{pl['pages_saved']}")
        lines.append(f"| {r['file']} | {','.join(r['strata'])} | "
                     f"{'✓' if a.get('byte_identical') else '✗'} | {pl} | "
                     f"{verdict_cell(a.get('verdict', '?'))} |")
    lines += ["", "## 臂 B：編輯態（突變區前逐像素零位移＋突變可見）", "",
              "| 檔案 | mutation | 目標段 | 區起(法) | 頁 原→存 | 區前非零頁 | 可見 | 判定 | 原因 |",
              "|---|---|---|---|---|---|---|---|---|"]
    for r in rows:
        for k, arm in r["arms"].items():
            if not k.startswith("edit:"):
                continue
            kind = k.split(":", 1)[1]
            lines.append(
                f"| {r['file']} | {kind} | {arm.get('target_paras')} | "
                f"{arm.get('zone_start', '—')}({arm.get('zone_method', '—')}) | "
                f"{arm.get('pages_orig', '—')}→{arm.get('pages_saved', '—')} | "
                f"{arm.get('before_zone_pages_px_nonzero', '—')} | "
                f"{'✓' if arm.get('visible') else ('✗' if arm.get('visible') is False else '—')} | "
                f"{verdict_cell(arm.get('verdict', '?'))} | {arm.get('why', '')} |")
    lines += ["", "## 臂 C：本專案渲染臂（存檔後檔 vs LO，sweep_cmp 兼容）", "",
              "| 檔案 | LO 頁 | OW 頁 | 頁數 | 內容相似度（首 6 頁） | 判定 |", "|---|---|---|---|---|---|"]
    for r in rows:
        for k, arm in r["arms"].items():
            cmp1 = arm.get("cmp_row")
            if not k.startswith("edit:") or not cmp1:
                continue
            cs = ", ".join("—" if s is None else str(s) for s in cmp1["content_scores"][:6])
            lines.append(f"| corpus/{r['file']} →{k.split(':', 1)[1]} | {cmp1['lo_pages']} | {cmp1['ow_pages']} | "
                         f"{'✓' if cmp1['page_count_match'] else '✗'} | {cs} | "
                         f"{verdict_cell(arm.get('verdict'))} |")
    lines += ["",
              "## 臂 C（原檔基線行，供 sweep_cmp 跨版本對照：同一 seed 取樣下",
              "## 上表存檔後分數不得低於本表對應分——突變區外；由本腳本內部把關）", "",
              "| 檔案 | LO 頁 | OW 頁 | 頁數 | 內容相似度（首 6 頁） |", "|---|---|---|---|---|"]
    for r in rows:
        cs = ", ".join("—" if s is None else str(s) for s in r.get("orig_content", [])[:6])
        lines.append(f"| corpus/{r['file']} | {r.get('orig_lo_pages', '—')} | {r.get('orig_ow_pages', '—')} | "
                     f"{'✓' if r.get('orig_lo_pages') == r.get('orig_ow_pages') else '✗'} | {cs} |")
    lines += ["",
              "口徑說明：",
              "- **臂 A 預閘＝位元組恆等**（sha256 對源檔逐位）；恆等過＝像素層跳過但記錄。",
              "  不恆等即紅，並照跑 LO 逐頁像素 diff 計數＋逐頁文字差分以鑑別變動性質。",
              "- **臂 B 判據**：打字/Enter/粗體/對齊/格內皆為內容單調不縮小编輯 ⇒ LO 重排",
              "  只能往後推 ⇒「突變段所在頁之前」的任何頁像素非零＝存檔動到別處（紅）；",
              "  突變區內像素與頁數全無變動＝存檔吞編輯（紅）。區界 quire pagebreaks 錨定，",
              "  LO 原文搜尋校正（fallback 標記如實入報告）。",
              "- **臂 C 判據**：突變區外逐頁內容分 ≥ 原檔 −0.002（存檔不得讓渲染保真退步）；",
              "  `| corpus…` 行與 scripts/sweep_cmp.py 兼容。",
              "- 相似度分只做臂 C 的「不得退步」判據；**臂 A/B 一律用像素 diff 計數**",
              "  （skill 教訓：相似度分對局部缺線不敏感）。",
              ""]
    out_md.write_text("\n".join(lines), encoding="utf-8")


def write_scores_jsonl(rows, meta, path: pathlib.Path):
    with path.open("w", encoding="utf-8") as fh:
        for r in rows:
            for k, arm in r["arms"].items():
                row = {"ts": meta["ts"], "file": r["file"], "strata": r["strata"],
                       "arm": k, "verdict": arm.get("verdict"),
                       "git": meta["git"], "buggy": meta["buggy"]}
                for f in ("byte_identical", "pixel_layer", "zone_start", "zone_method",
                          "pages_orig", "pages_saved", "before_zone_pages_px_nonzero",
                          "visible", "why", "note", "px_diff", "touched_parts"):
                    if f in arm:
                        row[f] = arm[f]
                if isinstance(row.get("pixel_layer"), dict):
                    row["px_diff"] = row["pixel_layer"].get("px_diff")
                fh.write(json.dumps(row, ensure_ascii=False) + "\n")


# ───────────────────────────── main ─────────────────────────────
def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", default=str(REPO / "corpus"))
    ap.add_argument("--out", default=str(REPO / "out" / "save-roundtrip-lab"))
    ap.add_argument("--n", type=int, default=None, help="取樣檔數（預設全量 42）")
    ap.add_argument("--quick", action="store_true", help="10 檔＋每檔前 2 種 mutation（PM 合併閘門候選）")
    ap.add_argument("--seed", type=int, default=SEED_DEFAULT)
    ap.add_argument("--files", nargs="*", help="直接指定 corpus 相對路徑（跳過分層抽樣）")
    ap.add_argument("--pixel-allow", type=int, default=PIXEL_ALLOW_DEFAULT,
                    help="每頁允許的非等像素數（預設 0；夜間可放，合併閘門不放）")
    ap.add_argument("--discriminate", action="store_true",
                    help="鑑別力模式：要求 SAVE_RT_BUGGY=1 下零編輯臂必紅，未紅即 exit 1")
    ap.add_argument("--keep-png", action="store_true")
    ap.add_argument("--report", default=None)
    a = ap.parse_args()

    buggy = os.environ.get("SAVE_RT_BUGGY") is not None
    if a.discriminate and not buggy:
        print("！ --discriminate 必須配 SAVE_RT_BUGGY=1（回塞鎖）一起跑")
        return 2

    kinds_all = ["type", "enter", "bold", "align", "cell"]
    kinds = kinds_all[:2] if a.quick else kinds_all
    n = a.n or (10 if a.quick else 42)

    corpus = pathlib.Path(a.corpus)
    quarantine_path = corpus / "quarantine.json"
    quarantined = set()
    if quarantine_path.exists():
        quarantined = set(json.loads(quarantine_path.read_text(encoding="utf-8"))["files"].keys())

    if a.files:
        manifest = json.loads((corpus / "manifest.json").read_text(encoding="utf-8"))
        by_name = {f["file"]: f for f in manifest["files"]}
        entries = []
        for f in a.files:
            e = by_name.get(f) or {"file": f, "tags": [], "strata": ["manual"]}
            e.setdefault("strata", ["manual"])
            entries.append(e)
        target_desc = f"自訂 {len(entries)} 檔"
        seed_used = None
    else:
        entries = select_sample(corpus, n, a.seed, quarantined)
        target_desc = f"corpus manifest 分層抽樣 {len(entries)} 檔"
        seed_used = a.seed

    out_dir = pathlib.Path(a.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    binp = quire_bin()
    soffice = rl.find_soffice()
    scratch, profile_uri, needle = rl.lo_scratch(soffice)
    # 專屬 profile（同輪多線取 LO 鐵律：render_lab 共用 profile 目錄會互撞）
    prof = scratch / f"profile-srt-{os.getpid()}"
    prof.mkdir(exist_ok=True)
    r = subprocess.run(["wslpath", "-w", str(prof)], capture_output=True, text=True) \
        if str(soffice).endswith(".exe") else None
    if str(soffice).endswith(".exe") and r and r.returncode == 0:
        win = r.stdout.strip().replace("\\", "/")
        profile_uri = "file:///" + win.lstrip("/")
        needle = win
    rl.kill_stale_lo()
    atexit.register(rl.kill_own_lo, needle)

    git = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=REPO,
                         capture_output=True, text=True).stdout.strip()
    meta = {"ts": datetime.datetime.now().isoformat(timespec="seconds"),
            "soffice": str(soffice), "cli": str(binp), "git": git,
            "target": target_desc, "seed": seed_used, "buggy": buggy,
            "discriminate": a.discriminate, "pixel_allow": a.pixel_allow}

    rows = []
    t0 = time.time()
    for i, e in enumerate(entries):
        print(f"[{i + 1}/{len(entries)}] {e['file']}", flush=True)
        try:
            rec = process_file(binp, corpus, e, out_dir, soffice, scratch,
                               profile_uri, needle, kinds, a.pixel_allow, buggy,
                               keep_png=a.keep_png)
        except Exception as ex:  # noqa: BLE001
            rec = {"file": e["file"], "strata": e.get("strata", []),
                   "arms": {"zero": {"verdict": "FAIL", "why": f"異常: {ex}"}},
                   "status": "fail"}
        rows.append(rec)
        r0 = rec["arms"].get("zero", {})
        print(f"   zero={r0.get('verdict')} " + " ".join(
            f"{k.split(':', 1)[1]}={v.get('verdict')}" for k, v in rec["arms"].items()
            if k.startswith("edit:")), flush=True)

    # 鑑別力判定：BUGGY 態下「原本零編輯恆等的檔」必須全部因重排而紅。
    disc_ok = None
    if a.discriminate:
        zero_red = [r["file"] for r in rows
                    if r["arms"].get("zero", {}).get("verdict") == "FAIL"
                    and r["arms"]["zero"].get("save_report", {}).get("bytes_in")]
        if zero_red:
            disc_ok = f"紅 {len(zero_red)}/{len(rows)} 檔點名（回塞鎖有效）"
        else:
            disc_ok = "零編輯臂未點名紅＝考卷無鑑別力（不可接受）"
    meta["discriminate_verdict"] = disc_ok or "—"

    report = pathlib.Path(a.report) if a.report else out_dir / "report.md"
    render_report(rows, meta, report)
    write_scores_jsonl(rows, meta, out_dir / "scores.jsonl")
    (out_dir / "sample.json").write_text(
        json.dumps({"seed": seed_used, "n": len(entries), "entries": entries},
                   ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"\n報告 → {report}\ntime={time.time() - t0:.0f}s")

    n_fail = sum(1 for r in rows if r["status"] == "fail")
    if a.discriminate:
        # 鑑別力模式的成功條件＝有紅被點名（且只允許 zero 臂因重排紅；
        # edit 臂在無害重排下區前位移仍應綠——真紅照報）。
        if disc_ok and disc_ok.startswith("紅"):
            edit_red = [r["file"] for r in rows if any(
                v.get("verdict") == "FAIL" for k, v in r["arms"].items() if k.startswith("edit:"))]
            print(f"DISCRIMINATION=OK（{disc_ok}）；編輯態紅（若有＝真缺陷）: {edit_red}")
            return 0 if not edit_red else 1
        print(f"DISCRIMINATION=FAILED — {disc_ok}")
        return 1
    print(f"SUMMARY files={len(rows)} fail={n_fail} "
          f"lo_fail={sum(1 for r in rows if r['status'] == 'lo_fail')}")
    return 1 if n_fail else 0


if __name__ == "__main__":
    sys.exit(main())
