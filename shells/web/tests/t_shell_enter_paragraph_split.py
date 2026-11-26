# -*- coding: utf-8 -*-
"""t_d900332c 殼層 Enter 分段真鍵盤考卷（先紅後綠，比照 t_f073cd6c 探針口徑）。

缺陷實錄（t_f073cd6c 取證入庫 `docs/word-ui-u4m-shell-enter-evidence.json`）：
    真鍵盤 Enter → beforeinput(insertParagraph) → commitText("\\n")
      → engine.insert_text_at_caret(para, cp, "\\n")   ← 段內字面換行
故段數永不增（用戶症狀「無法任意按 Enter 創建新段落」的真根因，在殼層非引擎）。
本檔打**真產線全鏈**：真 Chrome（Playwright chromium）＋ demo 殼 + 實際 pkg/ 產物
＋真鍵盤 Enter／Shift+Enter＋`quire-cli` 存檔回讀真值（不讀任何自報旗標）。

分派契約（本卡修復後態；兩種語意兩條路徑，不得混用）：
    Enter（insertParagraph）      → apply_mutation_json(SplitParagraph)   段數 +1
    Shift+Enter（insertLineBreak）→ apply_mutation_json(InsertBreak LineWrapping)
                                 段數不變（段內 `<w:br/>` 軟換行，Word 同語意）
    兩者皆**不得**再經 commitText／insert_text_at_caret("\\n")。

燈（單一斷言集——good 態必全綠；buggy 回塞態必紅，由
`scripts/u4n_enter_split_buggy_replay.sh` 逐盞點名，口徑比照 U4F_*/U4D_* 回塞鎖）：
  燈 E1（主燈）：真 Enter 於段中 → `paragraph_count()` 增一（修前紅燈點：47→47）。
  燈 E2：頭段＝前半、新建段落＝點擊段被切開的後半（引擎 `paragraph_text` 逐字對號）。
  燈 E3：Enter 後殼層 caret＝{para+1, cp:0}（引擎 caret_after 真值落地）。
  燈 E4（邊界）：Shift+Enter → 段數不變（軟換行不創段）＋走 InsertBreak(LineWrapping)
        ＋未走 SplitParagraph——軟換行與分段兩條路逕分立，PM 點名不得混用。
  燈 E5：呼叫鏈真值（包裹產線入口取證，比照 t_f073cd6c 口徑）：Enter 鏈中
        `apply_mutation_json(SplitParagraph)` 必被呼叫、`insert_text_at_caret("\\n")`
        必未被呼叫（字面路徑已收斂）。
  燈 E6：Enter 後 `undo_json()`（引擎正產線）→ 段數回減一，段索引／caret 無殘留。
  燈 E7（存檔重開，引擎回讀真值）：`save_docx_bytes()` 落盤 → `quire-cli paragraphs`
        讀回 → 段數＝瀏覽器引擎段數、逐段文字逐一全等（不讀自報旗標）。
        回塞態此燈亦紅＝字面 \\n 連存檔都活不下來（段內 \\n 存檔遭丟，第二層缺陷）。

拒跑防假綠：pkg/ 缺失、vite 不起、quire-cli 缺失 → 一律 exit 2（環境錯誤），
絕不降斷言。vite 一律**自起門牌**（預設 5211 strictPort）——沿用別處 5199 會測到
主倉／別棵 worktree 的舊 main.js（假綠源），故不重用既有埠。

回塞鎖 `U4N_ENTER_SPLIT_BUGGY`：JS 端本無 process，main.js 於**事件時**讀
`typeof process !== "undefined" && process.env.U4N_ENTER_SPLIT_BUGGY != null`
（正常瀏覽器恆假）；本檔 buggy 態經 `addInitScript` 注入 `window.process` 驅動。

執行：
    cd shells/web && npm run build:wasm
    cd <repo 根> && cargo build -p quire-cli
    ~/.local/chrome-control/bin/python shells/web/tests/t_shell_enter_paragraph_split.py
    # 回塞紅燈（預期 exit 1＋E1/E2/E3/E4/E5/E6/E7 紅）：
    U4N_ENTER_SPLIT_BUGGY=1 ~/.local/chrome-control/bin/python \\
        shells/web/tests/t_shell_enter_paragraph_split.py
退出碼：0=全綠；1=有失敗；2=環境錯誤。
"""
import base64
import json
import os
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = ROOT / "fixtures" / "style_cascade.docx"
PORT = os.environ.get("OPENWORD_U4N_PORT", "5211")
BASE = f"http://localhost:{PORT}"
CLI = ROOT / "target" / "debug" / "quire-cli"
BUGGY = os.environ.get("U4N_ENTER_SPLIT_BUGGY") == "1"
TAG = "buggy" if BUGGY else "good"
SHOT = ROOT / "build" / f"u4n-enter-split-{TAG}.png"
EVID = ROOT / "build" / f"u4n-enter-split-{TAG}.json"

fails = []


def check(name, ok_, detail=""):
    print(("PASS " if ok_ else "FAIL ") + name + (f"  [{detail}]" if detail else ""), flush=True)
    if not ok_:
        fails.append(name)


def note(name, detail=""):
    print("NOTE " + name + (f"  [{detail}]" if detail else ""), flush=True)


def wait_server(url, timeout=90.0):
    t0 = time.time()
    while time.time() - t0 < timeout:
        try:
            with urllib.request.urlopen(url, timeout=2) as r:
                if r.status == 200:
                    return True
        except Exception:
            time.sleep(0.4)
    return False


# 包裹產線入口取證（只記錄，不改行為；比照 t_f073cd6c_shell_enter_evidence.py）。
JS_SPY = """() => {
  window.__calls = [];
  const eng = window.__ime.engine();
  for (const m of ['insert_text_at_caret', 'apply_mutation_json',
                   'replace_paragraph_text']) {
    const f = eng[m].bind(eng);
    eng[m] = (...a) => {
      let arg = a.map(x => (typeof x === 'string' && x.length > 80 ? x.slice(0, 80) + '…' : x));
      if (m === 'apply_mutation_json') {
        try {
          const op = JSON.parse(a[0]);
          const k = Object.keys(op)[0];
          arg = [k, JSON.stringify(op[k]).slice(0, 120)];
        } catch (e) { /* 非 JSON：原樣記錄 */ }
      }
      window.__calls.push({m, a: arg});
      return f(...a);
    };
  }
}"""

# 取第 0 頁可編輯正文靶：非表格段、Text 項夠長（點得進段中）。
JS_TARGETS = """() => {
  const eng = window.__ime.engine();
  const dl = JSON.parse(eng.page_display_list_json(0));
  const items = dl.items.filter(i => i.Text && [...i.Text.text].length >= 10);
  const out = [];
  let cursor = 0;
  const paras = [];
  for (let p = 0; p < eng.paragraph_count(); p++) {
    paras.push([...(eng.paragraph_text(p) || '')].join(''));
  }
  for (const it of items) {
    const needle = it.Text.text;
    let para = -1, cpStart = 0, scan = 0;
    for (let p = cursor; p < paras.length; p++) {
      const idx = paras[p].indexOf(needle, scan);
      if (idx >= 0) { para = p; cpStart = idx; break; }
      scan = 0;
    }
    if (para < 0) continue;
    cursor = para;
    if (eng.table_locator_json(para)) continue;  // 表格段：M1.3/M1.5 只收正文 Body
    out.push({para, x: it.Text.x + it.Text.size_pt * 3, y: it.Text.y + it.Text.size_pt,
              size: it.Text.size_pt, text: needle.slice(0, 10)});
  }
  return {targets: out, w_pt: dl.width_pt};
}"""


def cli_paragraphs(path):
    """quire-cli 讀回真值（引擎回讀，非自報旗標）。失敗→環境錯誤 exit 2。"""
    r = subprocess.run([str(CLI), "paragraphs", str(path)],
                       capture_output=True, text=True, timeout=300)
    if r.returncode != 0:
        print(f"環境錯誤：quire-cli paragraphs 失敗 rc={r.returncode}: {r.stderr[:300]}")
        sys.exit(2)
    return json.loads(r.stdout)


def main():
    if not FIXTURE.exists():
        print(f"環境錯誤：fixture 缺失 {FIXTURE}")
        return 2
    if not (ROOT / "shells/web/pkg/quire_web.js").exists():
        print("環境錯誤：pkg/ 缺失——先在 shells/web 跑 npm run build:wasm")
        return 2
    if not CLI.exists():
        print(f"環境錯誤：quire-cli 缺失（{CLI}）——先 cargo build -p quire-cli")
        return 2

    # 自起 vite（strictPort）：不重用既有 5199——那可能是別棵 worktree 的舊 main.js。
    try:
        with urllib.request.urlopen(BASE + "/", timeout=1.5):
            print(f"環境錯誤：{BASE} 已被佔用——本檔堅持自起服務以免測到別處舊殼；"
                  f"請關掉佔用者或以 OPENWORD_U4N_PORT 換埠")
            return 2
    except Exception:
        pass
    serve = subprocess.Popen(
        ["node", "node_modules/vite/bin/vite.js", "demo", "--config",
         "demo/vite.config.mjs", "--port", PORT, "--strictPort"],
        cwd=ROOT / "shells/web",
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        start_new_session=True,  # 自成 process group：收尾連 vite 子程序一併帶走
    )

    def reap_server():
        # terminate() 只打直系子程序；vite 再生的子程序會成孤兒續佔埠
        # （本卡踩過：下次跑「已被佔用」假環境錯）。killpg 整團帶走。
        try:
            os.killpg(os.getpgid(serve.pid), 15)  # SIGTERM
        except Exception:
            serve.terminate()

    try:
        if not wait_server(BASE + "/"):
            print(f"環境錯誤：vite server 不起（{BASE}）")
            return 2
        with sync_playwright() as p:
            browser = p.chromium.launch(args=["--allow-file-access-from-files"])
            page = browser.new_page(viewport={"width": 1000, "height": 1300})
            pageerrors = []
            page.on("pageerror", lambda e: pageerrors.append(str(e)[:300]))
            if BUGGY:
                # 注入 window.process 驅動 JS 端回塞鎖（正常瀏覽器讀不到 env）。
                page.add_init_script(
                    "window.process = { env: { U4N_ENTER_SPLIT_BUGGY: '1' } };")
            page.goto(BASE + "/", wait_until="networkidle")
            page.wait_for_function("() => !!window.__ime", timeout=30000)
            page.set_input_files("#file", str(FIXTURE))
            page.wait_for_function("() => !!window.__ime.engine && window.__ime.engine()",
                                   timeout=30000)
            page.wait_for_function("() => window.__ime.engine().is_settled()", timeout=30000)
            page.evaluate("() => window.__ime.engine().set_sync_budget(null)")
            # 產線入口取證包裹：**必須在 engine 就緒後**裝（包的是 engine 實例；
            # 早於開檔裝＝包到 null，呼叫鏈恆空→斷言假紅）。
            page.evaluate(JS_SPY)
            page.wait_for_timeout(300)
            check("前置：鎖態與執行態一致（防測錯態）",
                  page.evaluate("() => typeof process !== 'undefined' && process.env"
                                " && process.env.U4N_ENTER_SPLIT_BUGGY != null") == BUGGY,
                  f"頁面鎖={'ON' if BUGGY else 'OFF'} 預期={'ON' if BUGGY else 'OFF'}")
            check("前置：頁面零未捕獲異常（回塞鎖自身不得拋錯）", not pageerrors,
                  str(pageerrors[:3]))

            box = page.locator("#page").bounding_box()
            got = page.evaluate(JS_TARGETS)
            scale = box["width"] / got["w_pt"]
            targets = got["targets"]
            check("前置：第 0 頁非表格靶 >= 4", len(targets) >= 4, f"實得 {len(targets)}")
            if not targets:
                print("環境錯誤：無可用靶（版面零 Text 項？）")
                return 2

            def click_target(t):
                page.mouse.click(box["x"] + t["x"] * scale, box["y"] + t["y"] * scale)
                page.wait_for_timeout(150)
                return page.evaluate("() => window.__ime.caret")

            def press_key(k):
                page.evaluate("() => { window.__calls = []; }")
                page.click("#ime")
                page.wait_for_timeout(80)
                page.keyboard.press(k)
                page.wait_for_timeout(300)
                return page.evaluate("() => window.__calls")

            def press_at(t, key="Enter"):
                """點擊靶 →（段首時真鍵 ArrowRight 一格）→ 真鍵盤 key。
                回傳（caret前, 段前數, 段後數, 呼叫鏈, caret後, 前文字）。"""
                click_target(t)
                before_caret = page.evaluate("() => window.__ime.caret")
                if key == "Enter" and before_caret["cp"] == 0:
                    # 段首 cp=0＝頭段空段、無前半文字可驗：真鍵 ArrowRight 一格
                    # （走殼層 keydown→caret 契約，非 JS 直改）再打。
                    page.click("#ime")
                    page.keyboard.press("ArrowRight")
                    page.wait_for_timeout(100)
                    before_caret = page.evaluate("() => window.__ime.caret")
                pc0 = page.evaluate("() => window.__ime.engine().paragraph_count()")
                t0 = page.evaluate("(p) => window.__ime.engine().paragraph_text(p)",
                                   before_caret["para"])
                calls = press_key(key)
                pc1 = page.evaluate("() => window.__ime.engine().paragraph_count()")
                after_caret = page.evaluate("() => window.__ime.caret")
                return before_caret, pc0, pc1, calls, after_caret, t0

            def mutation_of(calls, kind):
                return [c for c in calls
                        if c["m"] == "apply_mutation_json" and c["a"][0] == kind]

            # ---- 靶 1：真 Enter 分段（主燈 E1/E2/E3/E5）----
            t1 = next((t for t in targets if t["para"] >= 3), targets[0])
            caret1, pc0, pc1, calls1, caret_after1, txt1 = press_at(t1, "Enter")
            names1 = [c["m"] for c in calls1]
            cp1 = caret1["cp"]
            check("燈 E1（主燈）：真 Enter → paragraph_count 增一", pc1 == pc0 + 1,
                  f"{pc0} → {pc1}")
            if pc1 == pc0 + 1:
                head = page.evaluate("(p) => window.__ime.engine().paragraph_text(p)",
                                     caret1["para"])
                tail = page.evaluate("(p) => window.__ime.engine().paragraph_text(p)",
                                     caret1["para"] + 1)
                chars = list(txt1 or "")
                want_head, want_tail = "".join(chars[:cp1]), "".join(chars[cp1:])
                check("燈 E2：頭段＝前半＋新建段落＝被切開的後半（引擎 paragraph_text）",
                      head == want_head and tail == want_tail,
                      f"caret cp={cp1} head={head!r}|want={want_head!r} "
                      f"tail={tail!r}|want={want_tail!r}")
                check("燈 E3：Enter 後 caret＝{para+1, cp:0}（caret_after 落地）",
                      bool(caret_after1) and caret_after1["para"] == caret1["para"] + 1
                      and caret_after1["cp"] == 0,
                      f"before={caret1} after={caret_after1}")
            else:
                check("燈 E2：新建段落＝被切開的後半", False, "段數未增一，無法驗落點")
                check("燈 E3：Enter 後 caret＝{para+1, cp:0}", False, "段數未增一，無法驗落點")
                note("燈 E2/E3：回塞態段內文字", repr(
                    page.evaluate("(p) => window.__ime.engine().paragraph_text(p)",
                                  caret1["para"]))[:60])
            check("燈 E5a：apply_mutation_json(SplitParagraph) 被呼叫（正產線入口）",
                  len(mutation_of(calls1, "SplitParagraph")) == 1,
                  str([c["a"][0] for c in calls1]))
            check("燈 E5b：insert_text_at_caret('\\n') 未被呼叫（字面路徑已收斂）",
                  not [c for c in calls1 if c["m"] == "insert_text_at_caret"
                       and len(c["a"]) >= 3 and c["a"][2] == "\n"],
                  str(names1))

            # ---- 靶 2：Shift+Enter 軟換行邊界（燈 E4）----
            t2 = next((t for t in targets if t["para"] != t1["para"]
                       and t["para"] >= 3), targets[-1])
            _, s_pc0, s_pc1, s_calls, _, _ = press_at(t2, "Shift+Enter")
            brk = mutation_of(s_calls, "InsertBreak")
            check("燈 E4a：Shift+Enter 段數不變（軟換行不創段）", s_pc1 == s_pc0,
                  f"{s_pc0} → {s_pc1}")
            check("燈 E4b：Shift+Enter 走 InsertBreak(LineWrapping)、未走 SplitParagraph",
                  len(brk) == 1 and not mutation_of(s_calls, "SplitParagraph"),
                  str([c["a"][:2] for c in s_calls]))
            if len(brk) == 1:
                check("燈 E4c：InsertBreak kind＝LineWrapping（軟換行契約）",
                      "LineWrapping" in brk[0]["a"][1], brk[0]["a"][1])
            else:
                check("燈 E4c：InsertBreak kind＝LineWrapping（軟換行契約）", False,
                      "未呼叫 InsertBreak")

            # ---- 靶 3：第二次 Enter（連續分段）＋ Undo 殘留（燈 E6）----
            t3 = next((t for t in targets if t["para"] not in (t1["para"], t2["para"])
                       and t["para"] >= 3), targets[-1])
            _, u_pc0, u_pc1, u_calls, _, _ = press_at(t3, "Enter")
            check("燈 E5c：第二次真 Enter 段數再增一（可連續分段）", u_pc1 == u_pc0 + 1,
                  f"{u_pc0} → {u_pc1}")
            page.evaluate("() => window.__ime.engine().undo_json()")
            page.wait_for_timeout(250)
            u_pc2 = page.evaluate("() => window.__ime.engine().paragraph_count()")
            check("燈 E6：undo_json 後段數回減一（段索引／Undo 狀態無殘留）",
                  u_pc2 == u_pc0, f"undo 前基線 {u_pc0}／undo 後 {u_pc2}（redo 前 {u_pc1}）")

            # ---- 燈 E7：存檔重開（quire-cli 引擎回讀真值，不讀自報旗標）----
            b64 = page.evaluate(
                "() => { const b = window.__ime.engine().save_docx_bytes();"
                " let s = ''; const u = new Uint8Array(b);"
                " for (let i = 0; i < u.length; i += 8192)"
                "   s += String.fromCharCode.apply(null, u.subarray(i, i + 8192));"
                " return btoa(s); }")
            saved = ROOT / "build" / f"u4n-enter-split-saved-{TAG}.docx"
            saved.parent.mkdir(exist_ok=True)
            saved.write_bytes(base64.b64decode(b64))
            rows = cli_paragraphs(saved)
            live_pc = page.evaluate("() => window.__ime.engine().paragraph_count()")
            live_txt = [page.evaluate("(p) => window.__ime.engine().paragraph_text(p)", i)
                        for i in range(live_pc)]
            check("燈 E7a：存檔重開段數一致（quire-cli 回讀 vs 引擎段數）",
                  len(rows) == live_pc, f"cli={len(rows)} live={live_pc}")
            mism = []
            for i, row in enumerate(rows):
                got_t = row.get("text", "")
                want_t = live_txt[i] if i < len(live_txt) else None
                if got_t != want_t:
                    mism.append((i, (got_t or "")[:22], (want_t or "")[:22]))
            check("燈 E7b：存檔重開逐段文字全等（引擎回讀真值）", not mism,
                  f"不符 {len(mism)} 段：{mism[:3]}")
            if BUGGY and mism:
                note("燈 E7b：回塞態存檔丟失段內字面 \\n（第二層缺陷佐證）",
                     "段內 \\n 經 save_docx 後讀回消失——字面路徑連存檔都不活，"
                     "非僅「不增段」")

            page.screenshot(path=str(SHOT))
            EVID.write_text(json.dumps({
                "mode": TAG, "fixture": FIXTURE.name,
                "enter1": {"caret_before": caret1, "pc": [pc0, pc1],
                           "calls": calls1, "caret_after": caret_after1},
                "shift_enter": {"pc": [s_pc0, s_pc1], "calls": s_calls},
                "enter2": {"pc": [u_pc0, u_pc1], "calls": u_calls, "pc_after_undo": u_pc2},
                "save_readback": {"live_pc": live_pc, "cli_rows": len(rows),
                                  "mismatch": mism[:20], "saved": saved.name},
                "pageerrors": pageerrors,
            }, ensure_ascii=False, indent=2), encoding="utf-8")
            print(f"證據落盤：{EVID}／截圖：{SHOT}")
            browser.close()
    finally:
        reap_server()
    print(("U4N_ENTER_SPLIT_OK" if not fails else f"FAILED {len(fails)}: {fails}") +
          f"（態={TAG}）")
    return 0 if not fails else 1


sys.exit(main())
