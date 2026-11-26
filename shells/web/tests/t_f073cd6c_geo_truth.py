# -*- coding: utf-8 -*-
"""t_f073cd6c 幾何真值量測（非考卷，供 PM 裁示用）。

目的：本卡前提把 display list `Text.y`／`caret_rect.y_pt` 當成**基線**（ascent 區＝
y 以上），故行框歸屬改成「以相鄰 y 的中線切分」。但 demo 渲染器把字形畫在
`baselineY = (t.y + t.size_pt) * scale`（demo/main.js:410）——亦即渲染態的基線在
`t.y + size`，`t.y` 是**字身框頂**。兩套口徑差一個字級（12pt 檔差 12pt、24pt 差 24pt）。

本腳本不猜、只量：
  1. **墨跡真值**：在 canvas 上逐列取像素，找出該行字形實際佔據的 y 帶（像素→pt）。
  2. **反查歸屬真值**：對 y 逐 1pt 掃 `caret_at_json`，記錄換段臨界 y。
  3. 對帳：該行**墨跡**是否全程歸該行？（修後態若把字形下半判給下一行＝新缺陷）
輸出：逐行 (ink_top, ink_bottom, owner_lo, owner_hi) ＋ 換段臨界表＋截圖。
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
SHOT = ROOT / "build" / "u4m-geo-truth.png"


def server_up(url):
    try:
        with urllib.request.urlopen(url, timeout=3) as r:
            return r.status == 200
    except Exception:
        return False


def main():
    if not server_up(BASE + "/"):
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
        pg.wait_for_timeout(300)

        canvas = pg.locator("#page")
        box = canvas.bounding_box()
        dl0 = pg.evaluate("() => JSON.parse(window.__ime.engine().page_display_list_json(0))")
        scale = box["width"] / dl0["width_pt"]
        print(f"scale={scale:.4f} canvas w={box['width']} h={box['height']} dl w_pt={dl0['width_pt']}")

        # 取樣：第 0 頁正文 Text 項（含 12pt 與 24pt 兩級）
        items = pg.evaluate(
            """() => {
              const dl = JSON.parse(window.__ime.engine().page_display_list_json(0));
              return dl.items.filter(i => i.Text && i.Text.text.trim())
                .map(i => ({x:i.Text.x, y:i.Text.y, size:i.Text.size_pt, text:i.Text.text.slice(0,10)}));
            }"""
        )
        # 逐列像素：直接從 canvas 讀像素（device pixel→pt 由 scale 換算）
        rows = pg.evaluate(
            """([x0, wPx, y0Px, y1Px]) => {
              const c = document.getElementById('page');
              const ctx = c.getContext('2d');
              const dpr = c.width / c.getBoundingClientRect().width;
              const x = Math.round(x0 * dpr);
              const w = Math.max(1, Math.round(wPx * dpr));
              const yA = Math.round(y0Px * dpr), yB = Math.round(y1Px * dpr);
              const img = ctx.getImageData(x, yA, w, yB - yA).data;
              const ink = [];
              for (let r = 0; r < yB - yA; r++) {
                let n = 0;
                for (let c2 = 0; c2 < w; c2++) {
                  const o = (r * w + c2) * 4;
                  const a = img[o + 3];
                  if (a > 8) {
                    const lum = (img[o] + img[o + 1] + img[o + 2]) / 3;
                    if (lum < 140) n++;
                  }
                }
                ink.push(n);
              }
              return {ink, dpr, yA, yB};
            }""",
            [0, 0, 0, 1],
        )
        dpr = rows["dpr"]
        print(f"canvas dpr={dpr}")

        def ink_band(x_pt, y_top_pt, h_pt, w_pt):
            """回傳該視窗內有墨的像素列範圍（換算成 pt，原點＝canvas 頂）。"""
            args = [x_pt, w_pt / dpr * 1, (y_top_pt) * scale / dpr * dpr, (y_top_pt + h_pt) * scale]
            r = pg.evaluate(
                """([xPt, wPt, yTopPt, hPt]) => {
                  const c = document.getElementById('page');
                  const ctx = c.getContext('2d');
                  const rect = c.getBoundingClientRect();
                  const dpr = c.width / rect.width;
                  const scale = rect.width / window.__dlw;
                  const x = Math.round(xPt * scale * dpr);
                  const w = Math.max(1, Math.round(wPt * scale * dpr));
                  const yA = Math.round(yTopPt * scale * dpr);
                  const yB = Math.round((yTopPt + hPt) * scale * dpr);
                  if (x < 0 || yA < 0 || yB > c.height || x + w > c.width) return {err: 'range'};
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
                  return {rows, yA, yB, scale, dpr};
                }""",
                [x_pt, w_pt, y_top_pt, h_pt],
            )
            if r.get("err"):
                return None
            if not r["rows"]:
                return (None, None)
            lo = r["rows"][0] / dpr / r["scale"] + y_top_pt
            hi = (r["rows"][-1] + 1) / dpr / r["scale"] + y_top_pt
            return (round(lo, 2), round(hi, 2))

        pg.evaluate("() => { window.__dlw = JSON.parse(window.__ime.engine().page_display_list_json(0)).width_pt; }")

        picks = items[:8]
        print("\n=== 逐行：y(發射) / size / 墨跡帶 / caret_rect / 反查歸屬 ===")
        for j, it in enumerate(picks):
            y, size = it["y"], it["size"]
            # 乾淨視窗＝[本行框頂, 下一行框頂)：只含本行字形，不吃上下行墨跡。
            nxt = items[j + 1]["y"] if j + 1 < len(items) else y + size + 8.0
            ink = ink_band(it["x"], y, max(4.0, nxt - y - 0.5), min(160.0, size * 8))
            # 反查歸屬：逐 1pt 掃
            own = []
            for k in range(-8, int(size) + 13):
                yy = y + k
                raw = pg.evaluate(
                    "([x,y]) => window.__ime.engine().caret_at_json(0, x, y)",
                    [it["x"] + size * 0.5, yy],
                )
                par = json.loads(raw)["para"] if raw else None
                own.append((k, par))
            # 找該行應屬段：文字對號（段落文字＝各行文字串接 ⇒ indexOf 給該行首碼點）
            owner = pg.evaluate(
                """([needle]) => {
                  const eng = window.__ime.engine();
                  for (let pp = 0; pp < eng.paragraph_count(); pp++) {
                    if ((eng.paragraph_text(pp) || '').indexOf(needle) >= 0) return pp;
                  }
                  return null;
                }""",
                [it["text"]],
            )
            flip = [(k, o) for k, o in own if o != owner]
            ink_lo, ink_hi = ink if ink else (None, None)
            ink_mis = []
            if ink_lo is not None:
                for k, o in own:
                    yy = y + k
                    if ink_lo <= yy <= ink_hi and o != owner:
                        ink_mis.append((k, yy, o))
            print(
                f"\nText y={y} size={size} owner_para={owner} ink_pt=[{ink_lo},{ink_hi}] 「{it['text']}」"
            )
            print(f"  caret_at 掃描 owner≠{owner} 的 dy：{[(k, o) for k, o in flip][:14]}")
            print(f"  ★墨跡內判給別段（真滑鼠會點到字形卻錯段）：{ink_mis[:14]}")
            cr = pg.evaluate(
                """([yt]) => {
                  const eng = window.__ime.engine();
                  for (let pp = 0; pp < eng.paragraph_count(); pp++) {
                    const r = eng.caret_rect_json(pp, 0);
                    if (!r) continue;
                    const rr = JSON.parse(r);
                    if (rr.page === 0 && Math.abs(rr.y_pt - yt) < 0.6) return rr;
                  }
                  return null;
                }""",
                [y],
            )
            print(f"  caret_rect={cr}")
        pg.screenshot(path=str(SHOT))
        b.close()
    return 0


sys.exit(main())
