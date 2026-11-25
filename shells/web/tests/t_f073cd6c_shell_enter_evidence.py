# -*- coding: utf-8 -*-
"""t_f073cd6c 殼層 Enter 實態取證（PM 裁決 2：確認但不併修，本檔＝證據鏈產器）。

main 態 demo 殼的 Enter 路徑（shells/web/demo/main.js）：
    真鍵盤 Enter → contenteditable `beforeinput` (inputType=insertParagraph)
      → e.preventDefault() → commitText("\\n")
        → engine.insert_text_at_caret(para, cp, "\\n")   ← 段內插入字面換行
因此**段數不增**（用戶看到的「按 Enter 無法創建新段落」在殼層另有第二根因，
與本卡 caret_at 行框歸屬無關）。本檔用真瀏覽器＋真鍵盤把這條鏈打成實錄：
  燈 S1：真 Enter 前後 `paragraph_count()` 不變（段數不增）。
  燈 S2：被按的段內出現字面 `\\n`（＝段內換行，非新段）。
  燈 S3：呼叫鏈實錄＝`insert_text_at_caret` 被呼叫且參數為 ("\\n")，
          `apply_mutation_json`（SplitParagraph 正產線入口）**未被呼叫**。
"""
import json
import os
import sys
import urllib.request
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = ROOT / "fixtures" / "style_cascade.docx"
BASE = os.environ.get("OPENWORD_BASE", "http://localhost:5199")
EVID = ROOT / "build" / "u4m-shell-enter.json"

fails = []


def check(name, ok_, detail=""):
    print(("PASS " if ok_ else "FAIL ") + name + (f"  [{detail}]" if detail else ""), flush=True)
    if not ok_:
        fails.append(name)


def main():
    try:
        with urllib.request.urlopen(BASE + "/", timeout=3) as r:
            if r.status != 200:
                print("環境錯誤：vite 5199 非 200")
                return 2
    except Exception:
        print("環境錯誤：vite 5199 不起")
        return 2
    with sync_playwright() as p:
        b = p.chromium.launch(args=["--allow-file-access-from-files"])
        pg = b.new_page(viewport={"width": 1000, "height": 1250})
        pg.goto(BASE + "/", wait_until="networkidle")
        pg.wait_for_function("() => !!window.__ime", timeout=30000)
        pg.set_input_files("#file", str(FIXTURE))
        pg.wait_for_function("() => !!window.__ime.engine && window.__ime.engine()", timeout=30000)
        pg.wait_for_function("() => window.__ime.engine().is_settled()", timeout=30000)
        pg.evaluate("() => window.__ime.engine().set_sync_budget(null)")
        # 包住兩個產線入口取證呼叫鏈（只記錄，不改行為）。
        pg.evaluate(
            """() => {
              window.__calls = [];
              const eng = window.__ime.engine();
              for (const m of ['insert_text_at_caret', 'apply_mutation_json',
                               'replace_paragraph_text']) {
                const f = eng[m].bind(eng);
                eng[m] = (...a) => {
                  window.__calls.push({m, a: a.map(x => (typeof x === 'string' && x.length > 60 ? x.slice(0, 60) + '…' : x))});
                  return f(...a);
                };
              }
            }"""
        )
        box = pg.locator("#page").bounding_box()
        dl0 = pg.evaluate("() => JSON.parse(window.__ime.engine().page_display_list_json(0))")
        scale = box["width"] / dl0["width_pt"]
        tgt = pg.evaluate(
            """() => {
              const dl = JSON.parse(window.__ime.engine().page_display_list_json(0));
              const t = dl.items.filter(i => i.Text && i.Text.text.trim()
                                 && [...i.Text.text].length > 20)[0].Text;
              return {x: t.x + t.size_pt * 3, y: t.y + t.size_pt, size: t.size_pt};
            }"""
        )
        pg.mouse.click(box["x"] + tgt["x"] * scale, box["y"] + tgt["y"] * scale)
        caret = pg.evaluate("() => window.__ime.caret")
        pc0 = pg.evaluate("() => window.__ime.engine().paragraph_count()")
        t0 = pg.evaluate("(p) => window.__ime.engine().paragraph_text(p)", caret["para"])
        pg.evaluate("() => { window.__calls = []; }")
        # 真鍵盤 Enter（焦點在隱形錨點 #ime）。
        pg.click("#ime")
        pg.keyboard.press("Enter")
        pg.wait_for_timeout(200)
        pc1 = pg.evaluate("() => window.__ime.engine().paragraph_count()")
        t1 = pg.evaluate("(p) => window.__ime.engine().paragraph_text(p)", caret["para"])
        calls = pg.evaluate("() => window.__calls")
        ev = {
            "fixture": str(FIXTURE.relative_to(ROOT)),
            "caret": caret,
            "paragraph_count": {"before": pc0, "after": pc1},
            "para_text": {"before": t0, "after": t1},
            "call_chain": calls,
        }
        EVID.parent.mkdir(exist_ok=True)
        EVID.write_text(json.dumps(ev, ensure_ascii=False, indent=2), encoding="utf-8")
        print("呼叫鏈實錄：", json.dumps(calls, ensure_ascii=False))
        check("燈 S1：真 Enter 後段數不變（殼層不增段）", pc1 == pc0, f"{pc0} → {pc1}")
        check("燈 S2：段內出現真換行字元（段內換行而非新段）", "\n" in (t1 or ""),
              f"before={t0!r} after={t1!r}")
        names = [c["m"] for c in calls]
        check("燈 S3a：insert_text_at_caret 被呼叫", "insert_text_at_caret" in names, str(names))
        check("燈 S3b：apply_mutation_json(SplitParagraph) 未被呼叫",
              "apply_mutation_json" not in names, str(names))
        print(f"證據落盤：{EVID}")
        b.close()
    print("SHELL_ENTER_EVIDENCE_OK" if not fails else f"FAILED {fails}")
    return 0 if not fails else 1


sys.exit(main())
