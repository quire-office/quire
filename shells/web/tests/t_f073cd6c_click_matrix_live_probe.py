# -*- coding: utf-8 -*-
"""t_f073cd6c 真滑鼠 CDP 點擊矩陣＋真產線 Enter 探針（卡面必附考題 3）。

用戶實錄缺口：12pt/行距 1.4（pitch 16.8pt）檔在行**行框頂以上**那一帶點擊時
`caret_at` 反查回**上一段段尾** ⇒ caret 錯段 ⇒ Enter 分錯段（「無法任意按 Enter
創建新段落」的直接成因）。本探針走**真產線全鏈**：真 Chrome（Playwright chromium）
＋ demo 頁（canvas mousedown → engine.caret_at_json 反查 → window.__ime.caret）＋
實際 pkg/ 產物（非靜態斷言）。

## 【PM 最終裁示 11-26 04:45：前提反轉 ⇒ 本檔判準改行框墨跡真值制】
發射 y／`caret_rect.y`／顯示列表 `Text.y` ＝**行框頂**（非基線）：三支出貨渲染器把
**字形基線**畫在 `y + size_pt`。run 85/88 落庫的「行距中線制」（切分＝兩行框頂中點）
依此作廢——中線橫穿字形（12pt 實測：本行墨跡 [y−1.92, y+15.46]，中線 y+8.4），PM 逐行
復算 `docs/word-ui-u4m-click-evidence.json` 得墨跡破口 38/38。故本檔：
  · 燈 A1 由「契約 y ±(pitch/2−1)」改為**實測墨跡取樣**（點到自己的字必得本段）；
  · 燈 A2 由「跨中線才換段」改為**跨 ink 邊界才換段**（切分點＝相鄰兩行墨跡相鄰邊
    的中點，用實測墨跡算，不吃被測碼）；
  · 燈 C 由「記帳盞（非 gate）」升格為 **gate**：引擎實測「點到自己墨跡卻判別段」
    必 0 點，並與規則 S（修前跨度制）／M（作廢的中線制）純幾何並跑對照。

## 靶的錨點＝卡面口徑（caret_rect 的 y），並避開「列邊界 cp」的正向腿歧義
run 85 首跑把靶錨在 `Text.y + size_pt ± 4`（把 `Text.y` 同時當框頂與基線，兩把尺
混用）→ 21 例假紅。run 88 改以正向腿 `caret_rect` 為錨後**又**發現第二個探針自身的
坑：行首碼點 cp 同時是上一行的列尾插入點，`caret_rect(p, 行首cp)` 在邊界回**上一行**
的 y（正向腿對插入點的列歸屬以「列尾優先」）——拿它當本行錨，靶整組偏一行，又是
假紅。故本檔一律以 **cp＝行首＋1**（本行第一個字元，插入點唯一屬本行）問正向腿，
並按 rowY 去重。期望段/列以**文字對號**取得（段落文字＝該段各行文字串接 ⇒ 顯示
列表行序＝閱讀序，單調游標對號直接給該行首碼點與行尾碼點）。

燈（全部真滑鼠 click，取樣＝第 0 頁逐顯示列表 Text 項）：
  燈 A1（Tripwire 主燈，非循環）：**實測像素墨跡取樣**——逐行量 canvas 上該行字形
    的真實 y 帶（JS_INK），在墨跡帶內取 6 點真滑鼠點擊 → caret 段必＝該行所屬段、
    零 None。判準是「點到自己的字必得本段」，取樣源＝像素，不吃被測反查碼。
  燈 A2（卡面考題2，PM 裁示改以 **ink 邊界**表述）：相鄰兩行的切分點＝兩行**模型
    墨跡相鄰邊**（`shaping::ink_em_ratios` 的 asc/em=1.16000／desc/em=0.28800，與
    引擎同源但本檔獨立算式）的中點，夾到不越下行行框頂。真滑鼠點擊切分點 ±0.6pt
    → 上側必落**本行**、下側必落**相鄰顯示行**（para＝鄰行 para 且 cp ∈ 鄰行碼點
    區間）＝「跨 ink 邊界才換段」。
  燈 B（新建段落落點）：真滑鼠在缺陷面（契約 y＝行框頂**以上**）點擊定 caret → 以
    該 caret 發**真產線** `apply_mutation_json(SplitParagraph)`（engine M1.3，編輯面
    Enter 的正入口）→ 段數增一、新建段落＝點擊段被切開的後半（用戶實錄「新建段落
    落在 para4/5 之間」修後必全等）。殼層 Enter 另有第二根因（段內字面 \n 不增段），
    由 `t_f073cd6c_shell_enter_evidence.py` 另檔取證，PM 裁決不併修。
  燈 C（**gate**，PM 裁示二「比修前跨度制更強」的直接證據）：像素墨跡真值稽核——
    逐行逐 pt 以 `caret_at_json` 反查自己的實測墨跡帶，統計「點到自己字形卻判給
    別段」點數，**必 0**；並在同一批實測墨跡上把規則 S（修前跨度制）與規則 M
    （run 85/88 已作廢的行距中線制）純幾何並跑對照，落 JSON 留證。

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
            # 一律逐邊取值；拿單側 pitch 打兩邊＝探針自己越界假紅）。
            usable = []
            for i, tg in enumerate(rows):
                up = (rows[i]["rowY"] - rows[i - 1]["rowY"]) if i > 0 else None
                dn = (rows[i + 1]["rowY"] - rows[i]["rowY"]) if i + 1 < len(rows) else None
                if up is None or dn is None:
                    continue
                tg["up_pitch"], tg["dn_pitch"] = up, dn
                usable.append(tg)
            rows = usable
            check("取樣：可用行（兩側相鄰行齊備） >= 12", len(rows) >= 12, f"實得 {len(rows)}")

            # ---- 先量像素墨跡（燈 A1/A2/C 的共同取樣源；JS_INK＝canvas 實態）----
            # 視窗刻意從 `rowY` 起算（不含行框頂以上）：12pt／pitch 16.8 態上行字形
            # 下緣實測在 rowY−2.8pt，往上取樣會把**上一行的字**混進本行墨跡＝假髒。
            # 行框頂以上的本行模型墨跡（上緣 rowY−0.16×size）另以 OVER 取樣覆蓋。
            for tg in rows:
                nxt = tg["rowY"] + tg["dn_pitch"]
                r = page.evaluate(JS_INK, [tg["x"], min(160.0, tg["size"] * 8),
                                           tg["rowY"], max(4.0, nxt - tg["rowY"] - 0.5)])
                if r.get("err") or not r["rows"]:
                    continue
                step = 1.0 / r["dpr"] / r["scale"]
                tg["ink_pt"] = [round(tg["rowY"] + r["rows"][0] * step, 2),
                                round(tg["rowY"] + (r["rows"][-1] + 1) * step, 2)]

            # 模型墨跡（引擎同源度量 `shaping::ink_em_ratios`；本檔獨立算式，非循環：
            # 判準式寫在考卷端，被測碼是引擎）。行框頂 y ＋ 上緣位移／下緣位移。
            ASC_EM, DESC_EM = 1.16000, 0.28800
            OVER = 0.16  # ＝ size − asc_em×size 的符號絕對值（上緣越出行框頂的比例）

            def model_ink(tg):
                s = tg["size"]
                return (tg["rowY"] + (s - ASC_EM * s), tg["rowY"] + (s + DESC_EM * s))

            def split_between(a, b):
                """相鄰兩行（a 在上）的墨跡切分點：兩行模型墨跡相鄰邊中點，夾到
                不越下行行框頂（＝引擎 `caret_at` 的紀律）。"""
                _, bot_a = model_ink(a)
                top_b, _ = model_ink(b)
                mid = (bot_a + top_b) / 2.0
                return min(max(mid, a["rowY"]), b["rowY"])

            # ---- 燈 A1（Tripwire 主燈）：真滑鼠點擊**實測像素墨跡**內 → 段必＝本行
            # 所屬段、cp 必落本行碼點區間、零死區（「點到自己的字必得本段」）----
            # 另取「行框頂以上」缺陷面兩點（用戶實錄的症狀帶）：判準不靠任何一把尺
            # 猜切分，而是鎖「不得野跳」——落點段必屬本行或緊貼的前一文字行，且若判
            # 給前行，該點必須在前一行**像素墨跡**內或距其下緣 ≤1.5pt（超出＝真的把
            # 本行的字判給別段；原缺陷的 8.6pt ascent 全區錯判遠超此容差，必紅）。
            mis_a1, dead_a1, nclick = [], [], 0
            record = []
            for i, tg in enumerate(rows):
                ys = []
                if tg.get("ink_pt"):
                    lo, hi = tg["ink_pt"]
                    ys += [("墨跡", lo + (hi - lo) * k / 5.0) for k in range(6)]
                else:
                    # 空白行（無像素墨跡）：退回模型墨跡（hhea，引擎同源尺）取樣。
                    t, b = model_ink(tg)
                    ys += [("模型墨跡", t + (b - t) * k / 5.0) for k in range(6)]
                # 用戶實錄缺陷面：行框頂以上（原判定「基線以上」的同一帶）。
                ys += [("框頂以上", tg["rowY"] - 1.0),
                       ("框頂以上", tg["rowY"] - OVER * tg["size"] / 2.0)]
                for tag, y_pt in ys:
                    page.mouse.click(box["x"] + (tg["x"] + tg["size"] * 0.5) * scale,
                                     box["y"] + y_pt * scale)
                    caret = page.evaluate("() => window.__ime.caret")
                    record.append({"i": i, "arm": "A1", "tag": tag,
                                   "para_own": tg["para"], "rowY": tg["rowY"],
                                   "size": tg["size"], "ink_pt": tg.get("ink_pt"),
                                   "cpStart": tg.get("cpStart"),
                                   "cpEnd": tg.get("cpEnd"),
                                   "click_y_pt": round(y_pt, 2), "caret": caret,
                                   "text": tg["text"]})
                    nclick += 1
                    if caret is None:
                        dead_a1.append((tg["para"], tag, round(y_pt, 2)))
                        continue
                    in_ink = bool(tg.get("ink_pt")) and (
                        tg["ink_pt"][0] <= y_pt <= tg["ink_pt"][1])
                    if in_ink:
                        # 硬契約：點到自己的字形 ⇒ 本段 + 本行碼點區間。
                        ok = (caret["para"] == tg["para"]
                              and (tg.get("cpStart") is None
                                   or tg["cpStart"] <= caret["cp"] <= tg["cpEnd"]))
                        if not ok:
                            mis_a1.append((tg["para"], caret["para"], tag,
                                           round(y_pt, 2), tg.get("cpStart"),
                                           tg.get("cpEnd"), caret["cp"]))
                    else:
                        # 缺陷面：只禁「野跳」（超出本行／前行一格，或判給前行卻離
                        # 前行像素墨跡過遠）。
                        prev = rows[i - 1] if i > 0 else None
                        far_ok = caret["para"] == tg["para"]
                        if not far_ok and prev is not None:
                            near_prev = caret["para"] == prev["para"]
                            pbot = prev["ink_pt"][1] if prev.get("ink_pt") else None
                            near_ok = pbot is None or y_pt <= pbot + 1.5
                            far_ok = near_prev and near_ok
                        if not far_ok:
                            mis_a1.append((tg["para"], caret["para"], tag,
                                           round(y_pt, 2), "野跳", None, caret["cp"]))
            check("燈 A1：真滑鼠點擊樣本 >= 150（混級多頁）", nclick >= 150,
                  f"實得 {nclick}")
            check("燈 A1：點到自己墨跡（像素真值）點擊段全等＋cp 落本行", not mis_a1,
                  f"錯判 {len(mis_a1)} 例：{mis_a1[:4]}")
            check("燈 A1：框頂以上／墨跡點擊零死區（caret_at 不返 None）", not dead_a1,
                  f"None {len(dead_a1)} 例：{dead_a1[:4]}")

            # ---- 燈 A2（卡面考題2，PM 裁示以 ink 邊界表述）：跨墨跡切分點才換段----
            # 判準不預設任何一把尺的切分位置，而是逐點掃兩行**實測像素墨跡**之間的
            # 空白帶（0.5pt 步），要求：
            #   (a) 零死區  (b) 落點不野跳（只屬本行或相鄰行）
            #   (c) 上端屬本行、下端屬相鄰行、全程恰一次換段（單調＝不回跳）
            #   (d) 換段點落在空白帶內（容 1.0pt）＝換段不穿任何一行的字
            # 空白帶不足 1pt（兩行字形相接）者無唯一切分 ⇒ 跳過記數。引擎實際切分由
            # hhea 模型墨跡決定，與實測字形盒可差 <1pt ⇒ 以 NOTE 記錄偏差、不 pin 死。
            bad_a2, ncross, skip_nogap, dev = [], 0, 0, []
            n_inner = n_cross = 0
            for i, tg in enumerate(rows):
                if i + 1 >= len(rows):
                    continue
                nb = rows[i + 1]
                if not (tg.get("ink_pt") and nb.get("ink_pt")):
                    skip_nogap += 1
                    continue
                lo, hi = tg["ink_pt"][1], nb["ink_pt"][0]
                if hi - lo < 1.0:
                    skip_nogap += 1  # 兩行像素墨跡相接＝無空白帶可掃（無唯一切分）
                    continue
                # 由上行墨跡下緣內 0.5pt 掃到下行墨跡上緣內 0.5pt（0.5pt 步）。
                seq = []
                s = 0
                while True:
                    y_pt = lo - 0.5 + s * 0.5
                    if y_pt > hi + 0.5:
                        break
                    s += 1
                    page.mouse.click(box["x"] + (tg["x"] + tg["size"] * 0.5) * scale,
                                     box["y"] + y_pt * scale)
                    caret = page.evaluate("() => window.__ime.caret")
                    ncross += 1
                    seq.append((round(y_pt, 2), caret))
                    record.append({"i": i, "arm": "A2", "tag": "空白帶掃描",
                                   "para_own": tg["para"], "nb_para": nb["para"],
                                   "rowY": tg["rowY"], "ink_own": tg["ink_pt"],
                                   "ink_nb": nb["ink_pt"], "click_y_pt": round(y_pt, 2),
                                   "caret": caret, "text": tg["text"]})
                owners = [(y, c["para"] if c else None) for (y, c) in seq]
                # (a) 空白帶零死區：任何一點不得 None。
                if any(p is None for (_, p) in owners):
                    bad_a2.append(("死區", i, [o for o in owners if o[1] is None][:3]))
                    continue
                # (b) 落點不得野跳：只許屬本行或相鄰行。
                foreign = [o for o in owners if o[1] not in (tg["para"], nb["para"])]
                if foreign:
                    bad_a2.append(("野跳", i, foreign[:3], tg["para"], nb["para"]))
                    continue
                seqp = [p for (_, p) in owners]
                trans = [y2 for (y, p), (y2, p2) in zip(owners, owners[1:]) if p != p2]
                if tg["para"] == nb["para"]:
                    n_inner += 1
                    # 段內兩行：整條空白帶都必回本段——段內的行邊界**不得**換段
                    # （原缺陷的「Enter 分錯段」正是邊界錯換；此處 0 換段才是正解）。
                    if trans:
                        bad_a2.append(("段內錯換段", i, trans[:4], tg["para"]))
                    continue
                # (c) 跨段兩行：上端屬本行、下端屬相鄰行、全程恰一次換段（單調）。
                n_cross += 1
                if seqp[0] != tg["para"] or seqp[-1] != nb["para"]:
                    bad_a2.append(("端點歸屬", i, seqp[0], seqp[-1], tg["para"], nb["para"]))
                    continue
                if len(trans) != 1:
                    bad_a2.append(("換段次數", i, len(trans), trans[:4]))
                    continue
                # (d) 換段點不穿字：必須落在兩行像素墨跡之間的空白（容 1.0pt）。
                tpt = trans[0]
                if not (lo - 1.0 <= tpt <= hi + 1.0):
                    bad_a2.append(("換段點穿字／越界", i, tpt, [lo, hi]))
                dev.append(abs(tpt - (lo + hi) / 2.0))
            check("燈 A2：空白帶掃描樣本 >= 150", ncross >= 150, f"實得 {ncross}")
            check("燈 A2：跨 ink 邊界才換段（零死區＋不野跳＋段內不換段＋跨段恰一次換段"
                  "＋換段點不穿字）",
                  not bad_a2, f"破口 {len(bad_a2)} 例：{bad_a2[:4]}")
            check("燈 A2：跨段配對樣本 >= 8（跨 ink 邊界斷言需足量地形）",
                  n_cross >= 8, f"實得 跨段 {n_cross}／段內 {n_inner}")
            note("燈 A2：配對計數（跨段＝恰一次換段斷言／段內＝零換段斷言）",
                 f"跨段 {n_cross}／段內 {n_inner}／跳過 {skip_nogap}")
            if dev:
                note("燈 A2：實測換段點與『兩行像素墨跡相鄰邊中點』偏差",
                     f"max={max(dev):.2f}pt／med={sorted(dev)[len(dev) // 2]:.2f}pt"
                     "（引擎以 hhea 模型墨跡定切分，與實測字形盒可差 <1pt；"
                     "換段點落在空白帶內＝不穿字，本卡契約成立）")

            # ---- 燈 B：真滑鼠點擊定 caret → 真產線 SplitParagraph → 新建段落＝點擊段 ----
            picked, bad_split, entered = set(), [], 0
            for i, tg in enumerate(rows):
                if tg["para"] in picked or tg["cpStart"] == 0:
                    continue  # 每段取首個非段首靶（段首分裂＝空頭段，另案語義）
                picked.add(tg["para"])
                # 缺陷面：契約 y（行框頂）以上、仍屬本行模型墨跡的那一帶（用戶實錄
                # ascent 態）——修前跨度制在此反查回上一段段尾。
                y_pt = tg["rowY"] - OVER * tg["size"] / 2.0
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

            # ---- 燈 C（**gate**）：像素墨跡真值稽核——點到自己字形必得本段，必 0 ----
            ink_report, ink_mis = [], 0
            for i, tg in enumerate(rows):
                if not tg.get("ink_pt"):
                    continue
                lo, hi = tg["ink_pt"]
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
                                   "size": tg["size"], "up_pitch": tg["up_pitch"],
                                   "dn_pitch": tg["dn_pitch"], "ink_pt": [lo, hi],
                                   "own_ink_pts": len(own_wrong), "sample": own_wrong[:6],
                                   "text": tg["text"]})
            # 歸因對照（PM 裁示二的決定性數字）：同一批實測墨跡帶上，把三套判制
            # **純幾何**各跑一遍，數「點到自己字形卻判給別行」的點數——
            #   規則 S（修前＝跨度起自行框頂，本 fixture 每行 span=1 帶）：[yᵢ, yᵢ₊₁)
            #   規則 M（run 85/88 已作廢＝行距中線切分）：[mid(yᵢ₋₁,yᵢ), mid(yᵢ,yᵢ₊₁))
            #   規則 I（本卡＝行框墨跡真值制）：切分點＝模型墨跡相鄰邊中點（夾不越
            #          下行行框頂），首行上界退 half-pitch、末行下界 y＋pitch
            # 三式皆由同一批 caret_rect 契約 y（＝顯示列表 Text.y＝行框頂）算出，
            # 不碰引擎，故與燈 C 的引擎實測同一基準可比。
            def rule_count(pick):
                n = 0
                for i, tg in enumerate(rows):
                    if not tg.get("ink_pt"):
                        continue
                    lo, hi = tg["ink_pt"]
                    a, b = pick(i, rows)
                    yy = lo
                    while yy <= hi:
                        if not (a <= yy < b):
                            n += 1
                        yy += 1.0
                return n

            def rule_S(i, r):
                return (r[i]["rowY"],
                        r[i + 1]["rowY"] if i + 1 < len(r) else r[i]["rowY"] + r[i]["dn_pitch"])

            def rule_M(i, r):
                top = ((r[i - 1]["rowY"] + r[i]["rowY"]) / 2.0 if i > 0
                       else r[i]["rowY"] - r[i]["up_pitch"] / 2.0)
                bot = ((r[i]["rowY"] + r[i + 1]["rowY"]) / 2.0 if i + 1 < len(r)
                       else r[i]["rowY"] + r[i]["dn_pitch"] / 2.0)
                return (top, bot)

            def rule_I(i, r):
                top = (split_between(r[i - 1], r[i]) if i > 0
                       else r[i]["rowY"] - r[i]["up_pitch"] / 2.0)
                bot = (split_between(r[i], r[i + 1]) if i + 1 < len(r)
                       else r[i]["rowY"] + r[i]["dn_pitch"])
                return (top, bot)

            span_cnt = rule_count(rule_S)
            mid_cnt = rule_count(rule_M)
            ink_rule_cnt = rule_count(rule_I)
            note("燈 C 歸因對照（同一批實測墨跡、純幾何三式並跑）",
                 f"規則 S（修前跨度制）錯判={span_cnt} 點／規則 M（作廢的行距中線制）"
                 f"錯判={mid_cnt} 點／規則 I（本卡墨跡真值制）錯判={ink_rule_cnt} 點"
                 f"／引擎實測錯判={ink_mis} 點")
            check("燈 C（gate）：引擎實測『點到自己墨跡卻判別段』必 0 點"
                  "（PM 裁示二：比修前跨度制更強）", ink_mis == 0,
                  f"錯判 {ink_mis} 點（契約軸 y＝顯示列表 Text.y＝行框頂；渲染器基線畫在"
                  f" y+size_pt）")
            check("燈 C（gate）：本卡規則 I 的純幾何對照同為 0 點（模型自證）",
                  ink_rule_cnt == 0, f"規則 I 錯判={ink_rule_cnt} 點")
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
