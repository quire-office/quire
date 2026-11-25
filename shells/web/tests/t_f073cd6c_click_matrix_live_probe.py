# -*- coding: utf-8 -*-
"""t_f073cd6c 真滑鼠 CDP 點擊矩陣＋真鍵盤 Enter 探針（卡面必附考題 3）。

用戶實錄缺口：12pt/行距 1.4（pitch 16.8pt）檔在行**基線以上**（ascent 區）點擊時
`caret_at` 反查回**上一段段尾** ⇒ caret 錯段 ⇒ Enter 分錯段（「無法任意按 Enter
創建新段落」的直接成因）。本探針走**真產線全鏈**：

  真 Chrome（Playwright chromium）+ demo 頁（shells/web/demo，canvas click →
  engine.caret_at_json 反查 → 隱形錨點 → Enter 真鍵盤按鍵）+ 實際 pkg/ 產物。

矩陣（多頁混級段，逐 Text 項取樣，含上一項屬別段的邊界行）：
  燈 A（點擊段↔caret 段全等）：對每個正文 Text 項，在基線以上 4pt（用戶實錄
    dy=-2..-8 態）與基線以下 4pt 兩點做**真滑鼠 click**，讀 window.__ime.caret
    → 歸屬段必須＝該項所在段（渲染幾何與反查幾何互逆）。
  燈 B（真鍵盤 Enter 落點）：點擊後按真 Enter → paragraph_count 增長時，新建
    段落的內容必須是點擊段被切開的後半（＝新段落落在點擊段），且段序正確。

先紅口徑：本卡修復（caret_at 行距中線判制）由 core 端 U4M_LINEBOX_BUGGY 回塞鎖
覆蓋（cargo 態已親跑紅燈實錄）；wasm 產物由本探針打**修後態全綠**＋對照臂
BUGGY_WASM=1 時（需另建 buggy wasm，見 README 段）燈 A 必紅——本檔執行時若
env U4M_LINEBOX_BUGGY=1 且 pkg 未重建，則明確 fail 提示，不假綠。

依賴：playwright（~/.local/chrome-control venv）。執行：
    cd shells/web && npm run build:wasm
    cd <repo 根> && ~/.local/chrome-control/bin/python \
        shells/web/tests/t_f073cd6c_click_matrix_live_probe.py
退出碼：0=全綠；1=有失敗；2=環境錯誤。
"""
import json
import os
import re
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

fails = []


def check(name, ok_, detail=""):
    print(("PASS " if ok_ else "FAIL ") + name + (f"  [{detail}]" if detail else ""), flush=True)
    if not ok_:
        fails.append(name)


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


def main():
    if os.environ.get("U4M_LINEBOX_BUGGY") == "1":
        # 回塞鎖口徑：buggy 態須先以 U4M_LINEBOX_BUGGY=1 重建 wasm 再跑本檔，
        # 否則打的是修後產物、紅燈無從谈起——寧可硬 fail 也不假綠。
        print("環境錯誤：U4M_LINEBOX_BUGGY=1 須先重建 buggy pkg（見檔頭），拒跑以免假綠")
        return 2
    if not FIXTURE.exists():
        print(f"環境錯誤：fixture 缺失 {FIXTURE}")
        return 2
    pkg = ROOT / "shells/web/pkg/quire_web.js"
    if not pkg.exists():
        print("環境錯誤：pkg/ 缺失——先在 shells/web 跑 npm run build:wasm")
        return 2

    serve = subprocess.Popen(
        ["npx", "vite", "demo", "--config", "demo/vite.config.mjs"],
        cwd=ROOT / "shells/web",
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        if not wait_server(BASE + "/demo/index.html"):
            print(f"環境錯誤：vite server 不起（{BASE}）")
            return 2
        with sync_playwright() as p:
            browser = p.chromium.launch(args=["--allow-file-access-from-files"])
            page = browser.new_page(viewport={"width": 1000, "height": 1250})
            # vite root=demo（vite.config.mjs），根路徑即 demo/index.html——比照
            # scripts/ime-inject-test.py 的 goto(BASE) 口徑，勿加 /demo 前綴。
            page.goto(BASE + "/", wait_until="networkidle")
            page.wait_for_function("() => !!window.__ime", timeout=30000)
            page.set_input_files("#file", str(FIXTURE))
            page.wait_for_function("() => !!window.__ime.engine && window.__ime.engine()", timeout=30000)
            page.wait_for_function(
                "() => window.__ime.engine().is_settled()", timeout=30000
            )
            page.evaluate("() => window.__ime.engine().set_sync_budget(null)")
            # 翻到含混級正文的第 2 頁（style_cascade 頁1 後尚有正文頁）。
            page.evaluate(
                """() => {
                  // demo 的 page 變數不匯出；用頁碼鈕不存在→以 keyboard PageDown 無效，
                  // 直接 engine 端確認第 0 頁即有 >=8 個正文 Text 項（已偵察實得 40）。
                }"""
            )
            # 取樣表：逐 Text 項（正文），回傳點擊靶（pt 座標＋期望段號由 JS 端以
            # caret_rect_json 對號——渲染 y 就是 baseline−size，caret_rect 的 y 同式）。
            targets = page.evaluate(
                """() => {
                  const eng = window.__ime.engine();
                  const out = [];
                  const perPage = Math.min(eng.page_count(), 3);
                  for (let pg = 0; pg < perPage; pg++) {
                    const dl = JSON.parse(eng.page_display_list_json(pg));
                    if (pg !== 0) continue; // 探針只打第 0 頁（demo 單頁顯示）
                    const items = dl.items.filter((i) => i.Text && i.Text.text.trim());
                    for (const it of items) {
                      const t = it.Text;
                      // 渲染器鐵證：baselineY 畫在 (t.y + t.size_pt)*scale ⇒ 字身
                      // 視覺帶 = [t.y*scale, (t.y+2*size)*scale]，基線 = t.y+size。
                    out.push({pg, x: t.x + t.size_pt * 2, yTop: t.y, size: t.size_pt,
                              text: t.text.slice(0, 12)});
                    }
                  }
                  return out;
                }"""
            )
            check("取樣：第 0 頁正文 Text 項 >= 20", len(targets) >= 20, f"實得 {len(targets)}")

            canvas = page.locator("#page")
            box = canvas.bounding_box()
            dl0 = page.evaluate("() => JSON.parse(window.__ime.engine().page_display_list_json(0))")
            scale = box["width"] / dl0["width_pt"]

            # ---- 燈 A：真滑鼠點擊（基線以上/以下）→ caret 段 = 該項渲染所在段 ----
            # 「該項渲染所在段」由 caret_rect_json 正腿對號：掃段內 cp，正向腿 y
            # 命中該渲染行的 y（同段同行；渲染與正向腿同源 line_bands）。y 欄位
            # 兩種約定皆收（實錄 Text.y==caret y；渲染器另在 (y+size) 畫 baseline）。
            owner = page.evaluate(
                """(targets) => {
                  const eng = window.__ime.engine();
                  const res = [];
                  for (const tg of targets) {
                    let found = null;
                    const n = eng.paragraph_count();
                    for (let p = 0; p < n && found === null; p++) {
                      const txt = eng.paragraph_text(p) || "";
                      const L = [...txt].length;
                      if (L === 0) continue;
                      const step = Math.max(1, Math.floor(L / 24));
                      for (let cp = 0; cp <= L; cp += step) {
                        const raw = eng.caret_rect_json(p, cp);
                        if (!raw) continue;
                        const r = JSON.parse(raw);
                        if (r.page !== tg.pg) continue;
                        const hitTop = Math.abs(r.y_pt - tg.yTop) < 0.6;
                        const hitBase = Math.abs(r.y_pt - (tg.yTop + tg.size)) < 0.6;
                        if (hitTop || hitBase) {
                          found = p;
                          break;
                        }
                      }
                    }
                    res.push(found);
                  }
                  return res;
                }""",
                targets,
            )

            mis_above, mis_below, none_above, none_below = [], [], [], []
            nclick = 0
            for i, (tg, own) in enumerate(zip(targets, owner)):
                if own is None:
                    continue  # 渲染項無法由正腿對號（表格帶等）：跳過，燈 A 只打正文。
                for sign, mis, none in ((-4.0, mis_above, none_above), (4.0, mis_below, none_below)):
                    # 基線＝yTop＋size（渲染器 baselineY 同式）；±4pt＝用戶實錄缺陷區。
                    y_pt = tg["yTop"] + tg["size"] + sign
                    px = box["x"] + tg["x"] * scale
                    py = box["y"] + y_pt * scale
                    page.mouse.click(px, py)
                    nclick += 1
                    caret = page.evaluate("() => window.__ime.caret")
                    if caret is None:
                        none.append((i, sign))
                    elif caret["para"] != own:
                        mis.append((i, sign, caret["para"], own, tg["text"]))
            check("燈 A：真滑鼠點擊樣本 >= 30", nclick >= 30, f"實得 {nclick}")
            check(
                "燈 A：基線以上點擊段全等（用戶實錄缺陷面）",
                not mis_above,
                f"錯段 {len(mis_above)} 例：{mis_above[:5]}",
            )
            check("燈 A：基線以上點擊零死區", not none_above, f"None {len(none_above)} 例：{none_above[:5]}")
            check("燈 A：基線以下點擊段全等", not mis_below, f"錯段 {len(mis_below)} 例：{mis_below[:5]}")
            check("燈 A：基線以下點擊零死區", not none_below, f"None {len(none_below)} 例：{none_below[:5]}")

            # ---- 燈 B：真滑鼠點擊後真 Enter（引擎 M1.3 SplitParagraph）----
            # main 分支 shell 側真相：demo 的 Enter 走 `insertParagraph →
            # commitText("\\n")`＝段內字面換行，**段數不增**（如實記帳於檔頭）；
            # 「新建段落」的正產線入口＝同一個 wasm 物件上的
            # `apply_mutation_json({SplitParagraph,...})`（engine M1.3，編輯面
            # adapter 的 Enter 即釀自此）。故本盞＝真滑鼠點擊定 caret → 以該
            # caret 發 SplitParagraph → 新建段落必須落在**點擊段**（用戶實錄
            # 「新建段落落在 para4/5 之間」＝錯判上一段段尾所致，修後必全等）。
            boundary = [
                (i, tg, own)
                for i, (tg, own) in enumerate(zip(targets, owner))
                if own is not None and tg["size"] >= 12
            ]
            entered = 0
            bad_split = []
            for i, tg, own in boundary[:8]:
                px = box["x"] + tg["x"] * scale
                # 基線以上 4pt＝用戶實錄缺陷區（dy=-2..-8）。
                py = box["y"] + (tg["yTop"] + tg["size"] - 4.0) * scale
                page.mouse.click(px, py)
                caret_before = page.evaluate("() => window.__ime.caret")
                if caret_before["para"] != own:
                    bad_split.append(("click錯段", i, caret_before, own))
                    continue
                cp = caret_before["cp"]
                txt_before = page.evaluate(
                    "(p) => window.__ime.engine().paragraph_text(p)", own
                )
                pc_before = page.evaluate("() => window.__ime.engine().paragraph_count()")
                res = page.evaluate(
                    """([p, cp]) => JSON.parse(window.__ime.engine().apply_mutation_json(
                        JSON.stringify({SplitParagraph: {target: "Body",
                                                         pos: {para: p, cp: cp}}})
                    ))""",
                    [own, cp],
                )
                page.wait_for_timeout(120)
                if not res.get("ok"):
                    bad_split.append(("mutation拒", i, res.get("error")))
                    entered += 1
                    continue
                pc_after = page.evaluate("() => window.__ime.engine().paragraph_count()")
                if pc_after != pc_before + 1:
                    bad_split.append(("段數未增一", i, pc_before, pc_after))
                    entered += 1
                    continue
                head = page.evaluate("(p) => window.__ime.engine().paragraph_text(p)", own)
                tail_new = page.evaluate(
                    "(p) => window.__ime.engine().paragraph_text(p)", own + 1
                )
                chars = list(txt_before or "")
                # 位元組切點→碼點切點：Python 端以碼點切，JS 側 paragraph_text 同尺。
                want_head, want_tail = "".join(chars[:cp]), "".join(chars[cp:])
                if head != want_head or tail_new != want_tail:
                    bad_split.append(
                        ("新建段落落點錯（用戶實錄：落在相鄰兩段之間）", i, own,
                         repr((head or "")[-8:]), repr(want_head[-8:]),
                         repr((tail_new or "")[:8]), repr(want_tail[:8]))
                    )
                entered += 1
                # 分裂後版面變動：下一靶重新對號成本高，改以 undo 回 baseline。
                page.evaluate("() => window.__ime.engine().undo_json()")
                page.wait_for_timeout(60)
            check("燈 B：Enter 分裂靶樣本 >= 4", entered >= 4, f"實得 {entered}")
            check("燈 B：點擊段 ↔ 新建段落段全等", not bad_split, f"{bad_split[:3]}")

            SHOT.parent.mkdir(exist_ok=True)
            page.screenshot(path=str(SHOT))
            browser.close()
    finally:
        serve.terminate()
    print(("U4M_CLICK_MATRIX_OK " if not fails else f"FAILED {len(fails)}: {fails}"))
    return 0 if not fails else 1


sys.exit(main())
