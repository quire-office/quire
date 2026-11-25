# -*- coding: utf-8 -*-
"""t_f073cd6c 真滑鼠 CDP 點擊矩陣＋真產線 Enter 探針（卡面必附考題 3）。

用戶實錄缺口：12pt/行距 1.4（pitch 16.8pt）檔在行**基線以上**（ascent 區）點擊時
`caret_at` 反查回**上一段段尾** ⇒ caret 錯段 ⇒ Enter 分錯段（「無法任意按 Enter
創建新段落」的直接成因）。本探針走**真產線全鏈**：真 Chrome（Playwright chromium）
＋ demo 頁（canvas mousedown → engine.caret_at_json 反查 → window.__ime.caret）＋
實際 pkg/ 產物（非靜態斷言）。

## 靶的錨點＝卡面口徑（caret_rect 的 y），並避開「列邊界 cp」的正向腿歧義
run 85 首跑把靶錨在 `Text.y + size_pt ± 4`（把 `Text.y` 同時當框頂與基線，兩把尺
混用）→ 21 例假紅。run 88 改以正向腿 `caret_rect` 為錨後**又**發現第二個探針自身的
坑：行首碼點 cp 同時是上一行的列尾插入點，`caret_rect(p, 行首cp)` 在邊界回**上一行**
的 y（正向腿對插入點的列歸屬以「列尾優先」）——拿它當本行錨，靶整組偏一行，又是
假紅。故本檔一律以 **cp＝行首＋1**（本行第一個字元，插入點唯一屬本行）問正向腿，
並按 rowY 去重。期望段/列以**文字對號**取得（段落文字＝該段各行文字串接 ⇒ 顯示
列表行序＝閱讀序，單調游標對號直接給該行首碼點與行尾碼點）。

燈（全部真滑鼠 click，取樣＝第 0 頁逐顯示列表 Text 項）：
  燈 A1（卡面考題2 同段面）：契約 y ±(該邊 pitch/2 − 1) → caret 段＝該行所屬段，
    零 None。`該邊 pitch`＝與**上／下各自相鄰顯示行**的契約 y 差（混級檔兩側 pitch
    不同：12pt 行距 16.8、24pt 行距 33.6；拿單側 pitch 打兩邊會自己越界假紅）。
  燈 A2（卡面考題2「跨中線才換段」）：契約 y ±(該邊 pitch/2 + 1) → caret 必落
    **相鄰顯示行**（para＝鄰行 para 且 cp ∈ 鄰行碼點區間）。
  燈 B（新建段落落點）：真滑鼠在缺陷面（契約 y 以上）點擊定 caret → 以該 caret 發
    **真產線** `apply_mutation_json(SplitParagraph)`（engine M1.3，編輯面 Enter 的
    正入口）→ 段數增一、新建段落＝點擊段被切開的後半（用戶實錄「新建段落落在
    para4/5 之間」修後必全等）。殼層 Enter 另有第二根因（段內字面 \n 不增段），
    由 `t_f073cd6c_shell_enter_evidence.py` 另檔取證，PM 裁決不併修。
  燈 C（記帳盞，非 gate）：像素墨跡真值稽核——逐行量 canvas 上該行字形的真實 y
    帶，統計「點到自己字形卻判給別段」點數並落 JSON。本盞不計成敗：本卡契約軸是
    `caret_rect` 的 y（＝顯示列表 `Text.y`），而出貨渲染器把字形**基線**畫在
    `Text.y + size_pt`（`shells/native/src/render_svg.rs:144`「讓 y 語義接近行頂」
    ＋ `shells/web/demo/main.js:410` 同式）⇒ 字形墨跡整體比契約軸低一個字級，
    中線切分點橫穿字形（12pt 實測：本行墨跡 [y+2.1, y+14]，中線 y+8.4）。座標
    語義屬另一條軸，如實取證交 PM 裁示，本卡不改判制、不造第二把尺。

wasm 側回塞臂：`U4M_LINEBOX_BUGGY` 是 cargo 態環境變數，wasm 產物讀不到 env，故
本檔遇 `U4M_LINEBOX_BUGGY=1` 一律硬 fail（回塞紅燈實錄由 cargo 態考卷
core/layout/tests/u4m_linebox_hit.rs 覆蓋），不假綠。

依賴：playwright（~/.local/chrome-control venv）。執行：
    cd shells/web && npm run build:wasm
    cd <repo 根> && ~/.local/chrome-control/bin/python \
        shells/web/tests/t_f073cd6c_click_matrix_live_probe.py
退出碼：0=全綠；1=有失敗；2=環境錯誤。
"""
import json
import os
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = ROOT / "fixtures" / "style_cascade.docx"  # 多頁混級（12/24pt）
BASE = os.environ.get("OPENWORD_BASE", "http://localhost:5199")
SHOT = ROOT / "build" / "u4m-click-matrix.png"
EVID = ROOT / "build" / "u4m-click-matrix.json"

fails = []


def check(name, ok_, detail=""):
    print(("PASS " if ok_ else "FAIL ") + name + (f"  [{detail}]" if detail else ""), flush=True)
    if not ok_:
        fails.append(name)


def note(name, detail=""):
    print("NOTE " + name + (f"  [{detail}]" if detail else ""), flush=True)


def wait_server(url, timeout=40.0):
    t0 = time.time()
    while time.time() - t0 < timeout:
        try:
            with urllib.request.urlopen(url, timeout=2) as r:
                if r.status == 200:
                    return True
        except Exception:
            time.sleep(0.4)
    return False


# 取樣表：以**文字對號**把第 0 頁每個顯示列表 Text 項對到（段號, 該行首/尾碼點），
# 再以**行首＋1**（列內插入點，無邊界歧義）問正向腿取該行契約 y。
JS_ROWS = """() => {
  const eng = window.__ime.engine();
  const dl = JSON.parse(eng.page_display_list_json(0));
  const items = dl.items.filter(i => i.Text && i.Text.text.trim());
  const paras = [];
  for (let p = 0; p < eng.paragraph_count(); p++) {
    paras.push([...(eng.paragraph_text(p) || '')]);
  }
  const rows = [];
  let p = 0, cur = 0;
  for (const it of items) {
    const needle = it.Text.text;
    const nlen = [...needle].length;
    let hit = null;
    for (let look = 0; look < 3 && p + look < paras.length && !hit; look++) {
      const q = p + look;
      const from = look === 0 ? cur : 0;
      const idx = paras[q].slice(from).join('').indexOf(needle);
      if (idx < 0) continue;
      const cpStart = from + idx;
      const cpEnd = cpStart + nlen;                 // 該行末碼點（段落文字串接口徑）
      const cpAnchor = Math.min(cpStart + 1, cpEnd); // 列內插入點：無「上一行列尾」歧義
      const raw = eng.caret_rect_json(q, cpAnchor);
      if (!raw) continue;
      const r = JSON.parse(raw);
      if (r.page !== 0) continue;
      hit = {para: q, cpStart, cpEnd, rowY: r.y_pt, h: r.height_pt,
             size: it.Text.size_pt, x: it.Text.x, text: needle.slice(0, 12)};
      p = q; cur = cpEnd;
    }
    if (hit) rows.push(hit);
  }
  rows.sort((a, b) => a.rowY - b.rowY);
  const uniq = [];
  for (const r of rows) {
    if (uniq.length && Math.abs(uniq[uniq.length - 1].rowY - r.rowY) < 0.05) continue;
    uniq.push(r);
  }
  return {rows: uniq, w_pt: dl.width_pt};
}"""

# 像素真值：量 [yTop, yTop+h) 視窗內有墨的列，換算回 pt（dpr／scale 皆取自 canvas 實態）。
JS_INK = """([xPt, wPt, yTopPt, hPt]) => {
  const c = document.getElementById('page');
  const ctx = c.getContext('2d');
  const rect = c.getBoundingClientRect();
  const dpr = c.width / rect.width;
  const scale = rect.width / window.__dlw;
  const x = Math.round(xPt * scale * dpr);
  const w = Math.max(1, Math.round(wPt * scale * dpr));
  const yA = Math.max(0, Math.round(yTopPt * scale * dpr));
  const yB = Math.min(c.height, Math.round((yTopPt + hPt) * scale * dpr));
  if (x < 0 || x + w > c.width || yB - yA < 1) return {err: 'range'};
  const img = ctx.getImageData(x, yA, w, yB - yA).data;
  const rows = [];
  for (let r = 0; r < yB - yA; r++) {
    let n = 0;
    for (let c2 = 0; c2 < w; c2++) {
      const o = (r * w + c2) * 4;
      if (img[o + 3] > 8 && (img[o] + img[o + 1] + img[o + 2]) / 3 < 140) n++;
    }
    if (n > 0) rows.push(r);
  }
  return {rows, dpr, scale, yA};
}"""


def main():
    if os.environ.get("U4M_LINEBOX_BUGGY") == "1":
        print("環境錯誤：wasm 態讀不到 env，回塞臂須另建 buggy pkg；本檔拒跑以免假綠")
        return 2
    if not FIXTURE.exists():
        print(f"環境錯誤：fixture 缺失 {FIXTURE}")
        return 2
    pkg = ROOT / "shells/web/pkg/quire_web.js"
    if not pkg.exists():
        print("環境錯誤：pkg/ 缺失——先在 shells/web 跑 npm run build:wasm")
        return 2

    own_serve = None
    if not wait_server(BASE + "/", timeout=2.0):
        own_serve = subprocess.Popen(
            ["npx", "vite", "demo", "--config", "demo/vite.config.mjs"],
            cwd=ROOT / "shells/web",
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if not wait_server(BASE + "/", timeout=90.0):
            print(f"環境錯誤：vite server 不起（{BASE}）")
            own_serve.terminate()
            return 2
    try:
        with sync_playwright() as p:
            browser = p.chromium.launch(args=["--allow-file-access-from-files"])
            page = browser.new_page(viewport={"width": 1000, "height": 1300})
            # vite root=demo（vite.config.mjs），根路徑即 demo/index.html——比照
            # scripts/ime-inject-test.py 的 goto(BASE) 口徑，勿加 /demo 前綴。
            page.goto(BASE + "/", wait_until="networkidle")
            page.wait_for_function("() => !!window.__ime", timeout=30000)
            page.set_input_files("#file", str(FIXTURE))
            page.wait_for_function("() => !!window.__ime.engine && window.__ime.engine()", timeout=30000)
            page.wait_for_function("() => window.__ime.engine().is_settled()", timeout=30000)
            page.evaluate("() => window.__ime.engine().set_sync_budget(null)")
            page.wait_for_timeout(300)
            page.evaluate(
                "() => { window.__dlw = JSON.parse(window.__ime.engine().page_display_list_json(0)).width_pt; }"
            )

            got = page.evaluate(JS_ROWS)
            rows = got["rows"]
            check("取樣：第 0 頁文字對號（去重）>= 15 行", len(rows) >= 15, f"實得 {len(rows)}")
            canvas = page.locator("#page")
            box = canvas.bounding_box()
            scale = box["width"] / got["w_pt"]

            # 逐行兩側 pitch＝與**上／下各自相鄰顯示行**的契約 y 差（混級時兩側不同，
            # A1/A2 一律逐邊取值；拿單側 pitch 打兩邊＝探針自己越界假紅）。
            usable = []
            for i, tg in enumerate(rows):
                up = (rows[i]["rowY"] - rows[i - 1]["rowY"]) / 2.0 if i > 0 else None
                dn = (rows[i + 1]["rowY"] - rows[i]["rowY"]) / 2.0 if i + 1 < len(rows) else None
                if up is None or dn is None:
                    continue
                tg["up_half"], tg["dn_half"] = up, dn
                usable.append(tg)
            rows = usable
            check("取樣：可用行（兩側相鄰行齊備） >= 12", len(rows) >= 12, f"實得 {len(rows)}")

            # ---- 燈 A1：真滑鼠點擊契約 y ±(該邊 pitch/2 − 1) → 同段、零死區 ----
            mis_a1, dead_a1, nclick = [], [], 0
            record = []
            for i, tg in enumerate(rows):
                for tag, dy in (("上側", -(tg["up_half"] - 1.0)), ("下側", tg["dn_half"] - 1.0)):
                    y_pt = tg["rowY"] + dy
                    page.mouse.click(box["x"] + (tg["x"] + tg["size"] * 0.5) * scale,
                                     box["y"] + y_pt * scale)
                    nclick += 1
                    caret = page.evaluate("() => window.__ime.caret")
                    record.append({"i": i, "arm": "A1", "tag": tag, "para_own": tg["para"],
                                   "cpStart": tg["cpStart"], "cpEnd": tg["cpEnd"],
                                   "rowY": tg["rowY"], "up_half": tg["up_half"],
                                   "dn_half": tg["dn_half"], "click_y_pt": round(y_pt, 2),
                                   "caret": caret, "text": tg["text"]})
                    if caret is None:
                        dead_a1.append((i, tag, round(y_pt, 2), tg["text"]))
                    elif caret["para"] != tg["para"]:
                        mis_a1.append((i, tag, round(y_pt, 2), caret["para"], tg["para"], tg["text"]))
            check("燈 A1：真滑鼠點擊樣本 >= 24", nclick >= 24, f"實得 {nclick}")
            check("燈 A1：契約 y ±(該邊 pitch/2−1) 點擊段全等（卡面考題2 同段面）", not mis_a1,
                  f"錯段 {len(mis_a1)}/{nclick} 例：{mis_a1[:4]}")
            check("燈 A1：契約 y ±(該邊 pitch/2−1) 零死區", not dead_a1,
                  f"None {len(dead_a1)} 例：{dead_a1[:4]}")

            # ---- 燈 A2：跨中線（該邊 pitch/2 + 1）→ 必落相鄰顯示行（「跨中線才換段」）----
            bad_a2, ncross = [], 0
            for i, tg in enumerate(rows):
                arms = []
                if i + 1 < len(rows):
                    arms.append(("下跨", i + 1, tg["rowY"] + tg["dn_half"] + 1.0))
                if i > 0:
                    arms.append(("上跨", i - 1, tg["rowY"] - tg["up_half"] - 1.0))
                for tag, j, y_pt in arms:
                    nb = rows[j]
                    page.mouse.click(box["x"] + (nb["x"] + nb["size"] * 0.5) * scale,
                                     box["y"] + y_pt * scale)
                    ncross += 1
                    caret = page.evaluate("() => window.__ime.caret")
                    record.append({"i": i, "arm": "A2", "tag": tag, "para_own": tg["para"],
                                   "rowY": tg["rowY"], "click_y_pt": round(y_pt, 2),
                                   "caret": caret, "nb_para": nb["para"],
                                   "nb_cpStart": nb["cpStart"], "nb_cpEnd": nb["cpEnd"],
                                   "nb_text": nb["text"]})
                    if caret is None:
                        bad_a2.append((tag, "死區", round(y_pt, 2), tg["text"]))
                    elif not (caret["para"] == nb["para"]
                              and nb["cpStart"] <= caret["cp"] <= nb["cpEnd"]):
                        bad_a2.append((tag, "未落鄰行", round(y_pt, 2), caret,
                                       (nb["para"], nb["cpStart"], nb["cpEnd"])))
            check("燈 A2：跨中線樣本 >= 16", ncross >= 16, f"實得 {ncross}")
            check("燈 A2：跨中線必落相鄰行（卡面考題2「跨中線才換段」）", not bad_a2,
                  f"例：{bad_a2[:4]}")

            # ---- 燈 B：真滑鼠點擊定 caret → 真產線 SplitParagraph → 新建段落＝點擊段 ----
            picked, bad_split, entered = set(), [], 0
            for i, tg in enumerate(rows):
                if tg["para"] in picked or tg["cpStart"] == 0:
                    continue  # 每段取首個非段首靶（段首分裂＝空頭段，另案語義）
                picked.add(tg["para"])
                # 缺陷面：契約 y 以上（用戶實錄 ascent 態）——修前此處反查回上一段段尾。
                y_pt = tg["rowY"] - (tg["up_half"] - 1.0)
                page.mouse.click(box["x"] + (tg["x"] + tg["size"] * 0.5) * scale,
                                 box["y"] + y_pt * scale)
                before = page.evaluate("() => window.__ime.caret")
                if before["para"] != tg["para"]:
                    bad_split.append(("點擊即錯段", i, before, tg["para"], tg["text"]))
                    continue
                cp = before["cp"]
                txt = page.evaluate("(p) => window.__ime.engine().paragraph_text(p)", tg["para"])
                pc0 = page.evaluate("() => window.__ime.engine().paragraph_count()")
                res = page.evaluate(
                    """([p, cp]) => JSON.parse(window.__ime.engine().apply_mutation_json(
                        JSON.stringify({SplitParagraph: {target: "Body",
                                                         pos: {para: p, cp: cp}}})
                    ))""",
                    [tg["para"], cp],
                )
                page.wait_for_timeout(120)
                if not res.get("ok"):
                    bad_split.append(("mutation拒", i, res.get("error")))
                    entered += 1
                    continue
                pc1 = page.evaluate("() => window.__ime.engine().paragraph_count()")
                if pc1 != pc0 + 1:
                    bad_split.append(("段數未增一", i, pc0, pc1))
                    entered += 1
                    continue
                head = page.evaluate("(p) => window.__ime.engine().paragraph_text(p)", tg["para"])
                tail = page.evaluate("(p) => window.__ime.engine().paragraph_text(p)", tg["para"] + 1)
                chars = list(txt or "")
                want_head, want_tail = "".join(chars[:cp]), "".join(chars[cp:])
                if head != want_head or tail != want_tail:
                    bad_split.append(("新建段落落點錯（用戶實錄：落在相鄰兩段之間）",
                                      i, tg["para"], repr((head or "")[-8:]), repr(want_head[-8:]),
                                      repr((tail or "")[:8]), repr(want_tail[:8])))
                entered += 1
                page.evaluate("() => window.__ime.engine().undo_json()")  # 回 baseline 再打下一靶
                page.wait_for_timeout(80)
            check("燈 B：真產線分裂靶樣本 >= 6（多段混級）", entered >= 6, f"實得 {entered}")
            check("燈 B：點擊段 ↔ 新建段落段全等", not bad_split, f"{bad_split[:3]}")

            # ---- 燈 C（記帳盞，非 gate）：像素墨跡真值稽核 ----
            ink_report, ink_mis = [], 0
            for i, tg in enumerate(rows):
                nxt = tg["rowY"] + 2 * tg["dn_half"]
                r = page.evaluate(JS_INK, [tg["x"], min(160.0, tg["size"] * 8),
                                           tg["rowY"], max(4.0, nxt - tg["rowY"] - 0.5)])
                if r.get("err") or not r["rows"]:
                    continue
                step = 1.0 / r["dpr"] / r["scale"]
                lo = round(tg["rowY"] + r["rows"][0] * step, 2)
                hi = round(tg["rowY"] + (r["rows"][-1] + 1) * step, 2)
                own_wrong = []
                yy = lo
                while yy <= hi:
                    raw = page.evaluate("([x,y]) => window.__ime.engine().caret_at_json(0, x, y)",
                                        [tg["x"] + tg["size"] * 0.5, yy])
                    par = json.loads(raw)["para"] if raw else None
                    if par != tg["para"]:
                        own_wrong.append((round(yy, 2), par))
                    yy += 1.0
                ink_mis += len(own_wrong)
                ink_report.append({"i": i, "para": tg["para"], "rowY": tg["rowY"],
                                   "size": tg["size"], "up_half": tg["up_half"],
                                   "dn_half": tg["dn_half"], "ink_pt": [lo, hi],
                                   "own_ink_pts": len(own_wrong), "sample": own_wrong[:6],
                                   "text": tg["text"]})
            note("燈 C（記帳）：像素墨跡真值——本行字形墨跡內判給別段的點數合計",
                 f"{ink_mis}（契約軸 y＝顯示列表 Text.y；渲染器基線畫在 y+size_pt，"
                 f"故中線切分點橫穿字形——座標語義軸，PM 裁示範圍）")
            for rec in ink_report[:6]:
                note("  墨跡實錄", f"para{rec['para']} y={rec['rowY']} size={rec['size']} "
                                   f"ink={rec['ink_pt']} 錯判點={rec['own_ink_pts']} 「{rec['text']}」")

            SHOT.parent.mkdir(exist_ok=True)
            page.screenshot(path=str(SHOT))
            EVID.write_text(json.dumps({"fixture": FIXTURE.name, "rows": rows,
                                        "clicks": record, "ink_truth": ink_report,
                                        "ink_misattributed_points": ink_mis},
                                       ensure_ascii=False, indent=2), encoding="utf-8")
            print(f"證據落盤：{EVID}／截圖：{SHOT}")
            browser.close()
    finally:
        if own_serve is not None:
            own_serve.terminate()
    print("U4M_CLICK_MATRIX_OK " if not fails else f"FAILED {len(fails)}: {fails}")
    return 0 if not fails else 1


sys.exit(main())
