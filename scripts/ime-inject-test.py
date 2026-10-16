# -*- coding: utf-8 -*-
"""Web-1 IME 隱形 contenteditable 錨點——程式化注入測試（任務閘門之一）。

做法：起（或沿用）vite demo server → 灌 fixtures/sample.docx 進 <input type=file>
→ 用 CDP `Input.imeSetComposition` / `Input.insertText` 注入合成事件。CDP 走的是
瀏覽器真實 IME 事件路徑（compositionstart/update/end + input:insertCompositionText），
與注音/倉頌候選確認同一條流，headless 也能驗證整條錨點→引擎鏈路。

依賴：playwright（`pip install playwright && playwright install chromium`）。
執行：
    python3 scripts/ime-inject-test.py            # 自起 vite server（--serve 預設）
    python3 scripts/ime-inject-test.py --no-serve # 沿用已運行的 http://localhost:5199
退出碼：0=全綠；1=有失敗；2=環境錯誤。
"""
import argparse
import json
import os
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent
DOCX = (ROOT / "fixtures" / "sample.docx").read_bytes()
BASE = os.environ.get("OPENWORD_BASE", "http://localhost:5199")
SHOT = os.environ.get("OPENWORD_SHOT", str(ROOT / "build" / "ime-inject.png"))

fails = []


def check(name, ok, detail=""):
    print(("PASS " if ok else "FAIL ") + name + (f"  [{detail}]" if detail else ""), flush=True)
    if not ok:
        fails.append(name)


def wait_server(url, timeout=40.0):
    t0 = time.time()
    while time.time() - t0 < timeout:
        try:
            with urllib.request.urlopen(url, timeout=1.5) as r:
                if r.status == 200:
                    return True
        except Exception:
            pass
        time.sleep(0.4)
    return False


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--no-serve", action="store_true", help="沿用外部已起的 demo server")
    args = ap.parse_args()

    server = None
    if not args.no_serve:
        if not wait_server(BASE, timeout=1.0):
            web = ROOT / "shells" / "web"
            server = subprocess.Popen(
                ["npx", "vite", "demo", "--config", "demo/vite.config.mjs"],
                cwd=web,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            if not wait_server(BASE):
                print("環境錯誤：vite demo server 起不來", file=sys.stderr)
                server.kill()
                return 2
    try:
        run_tests()
    finally:
        if server:
            server.terminate()
    print("RESULT:", "ALL PASS" if not fails else f"{len(fails)} FAIL: {fails}", flush=True)
    return 0 if not fails else 1


def run_tests():
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 1200, "height": 1000})
        page.goto(BASE, wait_until="networkidle")
        # 1) 灌文件進 <input type=file>（等同手動選檔）
        page.set_input_files(
            "#file",
            files={
                "name": "sample.docx",
                "mimeType": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "buffer": DOCX,
            },
        )
        page.wait_for_function("window.__ime && window.__ime.engine()", timeout=20000)
        para0 = page.evaluate("window.__ime.engine().paragraph_text(0)")
        check("開檔後段落 0 可讀", isinstance(para0, str) and len(para0) > 0, repr(para0[:20]))

        # 2) 焦點在隱形 contenteditable 錨點上
        check("隱形錨點持有焦點", page.evaluate("window.__ime.focused"))

        # 3) 注音式合成：ㄘㄜˋ → 候選「測」→ 確認「測試」（CDP 與真 IME 同一事件流）
        client = page.context.new_cdp_session(page)
        page.evaluate(
            """(() => {
              window.__ev = [];
              const ime = document.getElementById('ime');
              for (const t of ['compositionstart','compositionupdate','compositionend','beforeinput'])
                ime.addEventListener(t, e => window.__ev.push({t: e.type, it: e.inputType||null, data: e.data}));
            })()"""
        )
        client.send("Input.imeSetComposition", {"selectionStart": 0, "selectionEnd": 0, "text": "ㄘㄜˋ"})
        client.send("Input.imeSetComposition", {"selectionStart": 1, "selectionEnd": 1, "text": "測"})
        client.send("Input.insertText", {"text": "測試"})
        page.wait_for_timeout(200)
        kinds = [e["t"] for e in page.evaluate("window.__ev")]
        check(
            "合成事件流完整（start/update/end）",
            "compositionstart" in kinds and "compositionend" in kinds,
            json.dumps(kinds),
        )
        committed = page.evaluate("window.__ime.engine().paragraph_text(0)")
        check("注音合成結果進入引擎", committed.startswith("測試"), repr(committed[:12]))
        caret = page.evaluate("window.__ime.caret")
        check("caret 跟隨 commit 前移", caret["cp"] >= 2, json.dumps(caret))
        anchor = page.evaluate(
            "(() => { const e = document.getElementById('ime'); return [e.style.left, e.style.top]; })()"
        )
        check("隱形錨點跟隨 caret 定位", anchor != ["0px", "0px"], json.dumps(anchor))

        # 4) canvas 點擊 → caret 反查 → 倉頌式選字確認插入點擊落點
        page.click("#page", position={"x": 300, "y": 120})
        clicked = page.evaluate("window.__ime.caret")
        check("canvas 點擊反查 caret", clicked["para"] == 0, json.dumps(clicked))
        check("canvas 點擊後錨點仍持有焦點", page.evaluate("window.__ime.focused"))
        client.send("Input.insertText", {"text": "倉頌"})
        page.wait_for_timeout(150)
        after = page.evaluate("window.__ime.engine().paragraph_text(0)")
        check("點擊位 insertion 落點正確", "倉頌" in after, repr(after[:24]))

        # 5) emoji（surrogate pair 完整進引擎，不拆半）
        client.send("Input.insertText", {"text": "\U0001F600"})
        page.wait_for_timeout(150)
        emo = page.evaluate("window.__ime.engine().paragraph_text(0)")
        check("emoji 完整進引擎", "\U0001F600" in emo, repr(emo[-8:]))
        cps = page.evaluate("Array.from(window.__ime.engine().paragraph_text(0)).length")
        u16 = page.evaluate("window.__ime.engine().paragraph_text(0).length")
        check("碼點/UTF-16 長度一致（emoji 不拆半）", u16 >= cps, f"cp={cps} u16={u16}")

        # 6) 編輯反映到重繪（顯示列表含新文字＝真的走進引擎版面）
        dl_has = page.evaluate(
            """(() => {
              const j = JSON.parse(window.__ime.engine().page_display_list_json(0));
              return j.items.some(i => i.Text && i.Text.text.includes('測試'));
            })()"""
        )
        check("重繪顯示列表含 commit 文字", dl_has)

        Path(SHOT).parent.mkdir(parents=True, exist_ok=True)
        page.screenshot(path=SHOT)
        print(f"screenshot: {SHOT}", flush=True)
        browser.close()


if __name__ == "__main__":
    sys.exit(main())
