// Quire web 殼啟動器（Web-2）：wasm 流式載入 + 首包/延遲分塊。
//
// 拆包契約：
//   1) 主 wasm（quire_web_bg.wasm）＝ 首包：解析/版面/顯示列表，**不含** CRDT。
//   2) 協作分塊（quire_web_collab_bg.wasm）＝ 延遲載入：只有使用者開啟
//      協作時才 dynamic import（見 enableCollab），CRDT 程式碼物理上不在首包。
//   3) 載入走 WebAssembly.instantiateStreaming（fetch Response 直接餵給 glue），
//      同時用 ReadableStream tee 回報下載進度——下載與編譯重疊，不等整包落地。
// 渲染器 = Canvas2D 顯示列表直繪；未來換 WebGPU。
// Web-1：真實 IME 編輯路徑已落地——隱形 contenteditable 錨點（#ime）＋
// caret 契約（caret_rect/caret_at 經引擎 shaping 度量），見下方「IME 錨點」段。
import init, { WasmEngine } from "../pkg/quire_web.js";

// 顯示寬（px）。pt→px 縮放按「此頁實際寬」（dl.width_pt，Phase0-2b 起逐頁如實
// 來自 sectPr）換算，不再假設全文件 A4 595。
const DISPLAY_W_PX = 850;
let scale = DISPLAY_W_PX / 595; // render() 按每頁寬重算
const MAIN_WASM_URL = new URL("../pkg/quire_web_bg.wasm", import.meta.url);
const canvas = document.getElementById("page");
const ctx = canvas.getContext("2d");
const statusEl = document.getElementById("status");
const imeEl = document.getElementById("ime");
const caretEl = document.getElementById("caret");
const compEl = document.getElementById("composited");
let engine = null;
let page = 0;

function fmtKB(n) {
  return `${(n / 1024).toFixed(1)} KB`;
}

// ---- 首包：流式載入（下載即編譯，進度可見） ----
async function loadMainChunk() {
  const t0 = performance.now();
  const res = await fetch(MAIN_WASM_URL);
  if (!res.ok) throw new Error(`wasm 下載失敗 ${res.status}`);
  const total = Number(res.headers.get("Content-Length")) || 0;
  let received = 0;
  const [a, b] = res.body.tee();
  const progress = (async () => {
    const reader = a.getReader();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      received += value.byteLength;
      const pct = total ? ` ${(received / total) * 100 | 0}%` : "";
      statusEl.textContent = `首包 wasm 下載中 ${fmtKB(received)}${total ? ` / ${fmtKB(total)}` : ""}${pct}`;
    }
  })();
  // 把「另一半」Response 交給 glue：Content-Type=application/wasm 時
  // wasm-bindgen 內部走 instantiateStreaming——邊下載邊編譯。
  const mod = new Response(b, { headers: res.headers });
  await init({ module_or_path: mod });
  await progress;
  const ms = Math.round(performance.now() - t0);
  statusEl.textContent =
    `首包就緒：${fmtKB(received)}（wasm 流式編譯，${ms}ms）；協作分塊未載入——開啟協作才會下載`;
}

// ---- 延遲分塊：協作（CRDT）----
let collab = null; // { glue, A, B, chunkKb, ms }
async function enableCollab() {
  if (collab) return collab;
  const t0 = performance.now();
  statusEl.textContent = "協作分塊下載中…";
  // 動態 import：vite 自動 code-split，主 bundle 不含此模組。
  const mod = await import("../../web-collab/pkg/quire_web_collab.js");
  const url = new URL(
    "../../web-collab/pkg/quire_web_collab_bg.wasm",
    import.meta.url,
  );
  const bytes = new Uint8Array(await (await fetch(url)).arrayBuffer());
  await mod.default({ module_or_path: bytes });
  collab = {
    glue: mod,
    A: new mod.WasmCollab(1),
    B: new mod.WasmCollab(2),
    chunkKb: fmtKB(bytes.byteLength),
    ms: Math.round(performance.now() - t0),
  };
  statusEl.textContent =
    `協作分塊就緒：${collab.chunkKb}（${collab.ms}ms，含 CRDT——首包沒有這段程式碼）`;
  document.getElementById("peers").hidden = false;
  return collab;
}

// 把分塊回傳的操作落地到主引擎（JS 橋：跨 wasm 邊界只有 JSON）。
function applyLanded(opsJson) {
  for (const raw of opsJson) {
    const op = JSON.parse(raw);
    if (op.ReplaceRunText && engine) {
      const { para, text } = op.ReplaceRunText;
      // 骨架：run 級尚未穿透編輯面，先落地到段落級（主引擎契約）。
      engine.replace_paragraph_text(para, text);
    }
  }
  render();
}

// ---- demo UI ----
await loadMainChunk();

// 測試掛鉤（Playwright IME 注入測試讀取；也是除錯可見狀態）。
window.__ime = {
  get caret() { return { ...caret }; },
  get focused() { return document.activeElement === imeEl; },
  get composing() { return composing; },
  engine: () => engine,
};

document.getElementById("file").addEventListener("change", async (e) => {
  const buf = new Uint8Array(await e.target.files[0].arrayBuffer());
  engine = new WasmEngine(buf);
  page = 0;
  caret = { para: 0, cp: 0 };
  render();
  imeEl.focus({ preventScroll: true }); // 開檔後焦點給隱形錨點，可直接打字
});
document.getElementById("prev").onclick = () => { if (page > 0) { page--; render(); } };
document.getElementById("next").onclick = () => { if (engine && page + 1 < engine.page_count()) { page++; render(); } };
document.getElementById("collab").onclick = () => {
  enableCollab().catch((e) => (statusEl.textContent = `協作分塊載入失敗：${e}`));
};

// 模擬兩個協作者（A 提交 → 廣播 → B 合併後落地重繪主文件；反向同理）。
for (const side of ["A", "B"]) {
  document.getElementById(`send${side}`).onclick = async () => {
    const c = await enableCollab();
    const text = document.getElementById(`input${side}`).value;
    const para = Number(document.getElementById(`para${side}`).value) || 0;
    const local = side === "A" ? c.A : c.B;
    const remote = side === "A" ? c.B : c.A;
    const msg = local.commit_local(para, 0, text);
    const landed = remote.apply_remote(msg);
    applyLanded(landed.length ? landed : [JSON.stringify({ ReplaceRunText: { para, run: 0, text } })]);
    logPeer(side, para, text, msg, landed.length);
  };
}

function logPeer(side, para, text, msg, landedCount) {
  const el = document.getElementById("peerlog");
  const line = document.createElement("div");
  line.textContent = `${side} → p${para}「${text}」廣播 ${msg.slice(0, 72)}… 落地 ${landedCount} op`;
  el.prepend(line);
  while (el.childElementCount > 6) el.lastChild.remove();
}

// ---------------- ★Web-1：IME 隱形 contenteditable 錨點 ----------------
// 契約（canvas 是顯示端、contenteditable 是輸入端，兩端靠引擎 caret 換算）：
//   caret = { para, cp }，cp 以**碼點**計（emoji 等 surrogate pair 不拆半）。
//   定位：engine.caret_rect_json → 把隱形 #ime 移到該處，真實 IME 候選窗才跟隨。
//   輸入：#ime 收 compositionstart/update/end 與 beforeinput（含直接鍵入/貼上），
//         commit 經 engine.insert_text_at_caret 進引擎，再重繪。
//   點擊：canvas click → engine.caret_at_json 反查 caret（顯示→輸入反向契約）。
let caret = { para: 0, cp: 0 };
let composing = false;

// 碼點數（Array.from 按 code point 迭代，UTF-16 length 會把 emoji 算成 2）。
const cps = (s) => Array.from(s).length;

function placeAnchor() {
  if (!engine) return;
  const raw = engine.caret_rect_json(caret.para, caret.cp);
  if (!raw) {
    caretEl.style.display = "none";
    imeEl.style.display = "none";
    return;
  }
  const r = JSON.parse(raw);
  // caret 不在目前顯示頁：隱藏但保留 focus 能力（翻頁後自動回來）。
  const visible = r.page === page;
  const x = r.x_pt * scale;
  const y = r.y_pt * scale;
  imeEl.style.display = "block";
  imeEl.style.left = `${x}px`;
  imeEl.style.top = `${y}px`;
  caretEl.style.display = visible ? "block" : "none";
  caretEl.style.left = `${x}px`;
  caretEl.style.top = `${y}px`;
  caretEl.style.height = `${r.height_pt * scale}px`;
}

// commit 一段文字進引擎（IME 確認、直接鍵入、貼上共用同一條路）。
function commitText(text) {
  if (!engine || !text) return;
  engine.insert_text_at_caret(caret.para, caret.cp, text);
  caret = { para: caret.para, cp: caret.cp + cps(text) };
  resetAnchor();
  render();
}

function resetAnchor() {
  imeEl.textContent = "";
  compEl.style.display = "none";
  composing = false;
}

imeEl.addEventListener("compositionstart", () => {
  composing = true;
  compEl.style.display = "block";
});
imeEl.addEventListener("compositionupdate", (e) => {
  // 合成中的候選文字疊畫在 caret 上（canvas 不會顯示 contenteditable 的內容）。
  const raw = engine && engine.caret_rect_json(caret.para, caret.cp);
  if (raw) {
    const r = JSON.parse(raw);
    compEl.style.left = `${r.x_pt * scale}px`;
    compEl.style.top = `${r.y_pt * scale}px`;
    compEl.style.font = `${r.height_pt * scale}px "Noto Sans TC", sans-serif`;
  }
  compEl.textContent = e.data || "";
});
imeEl.addEventListener("compositionend", (e) => {
  compEl.style.display = "none";
  composing = false;
  commitText(e.data || "");
});
// 非合成輸入（英数字直接鍵入、貼上、退格）：beforeinput 比 input 更早且帶 inputType。
imeEl.addEventListener("beforeinput", (e) => {
  if (composing || e.isComposing) return; // 合成中由 compositionend 處理
  switch (e.inputType) {
    case "insertText":
    case "insertFromPaste":
    case "insertLineBreak":
      e.preventDefault();
      commitText(e.data ?? "");
      break;
    case "insertParagraph":
      e.preventDefault();
      commitText("\n");
      break;
    case "deleteContentBackward":
      e.preventDefault();
      backspace();
      break;
    case "deleteContentForward":
      e.preventDefault();
      if (caret.cp < cps(engine.paragraph_text(caret.para) || "")) {
        caret = { para: caret.para, cp: caret.cp + 1 };
        placeAnchor();
      }
      break;
    default:
      e.preventDefault(); // 其餘一律不讓 contenteditable 自行改動
  }
});

function backspace() {
  const text = engine.paragraph_text(caret.para) || "";
  if (caret.cp === 0) {
    // 段首再退：合併到前段尾（骨架：只移 caret，實作段落合併屬後續卡）。
    if (caret.para > 0) {
      caret = { para: caret.para - 1, cp: cps(engine.paragraph_text(caret.para - 1) || "") };
      placeAnchor();
    }
    return;
  }
  const chars = Array.from(text);
  const next = chars.slice(0, caret.cp - 1).concat(chars.slice(caret.cp)).join("");
  engine.replace_paragraph_text(caret.para, next);
  caret = { para: caret.para, cp: caret.cp - 1 };
  render();
}

// 顯示端 → 輸入端：點 canvas 換算 caret，並把焦點交給隱形錨點。
// preventDefault：不放 browser 把選點移到 canvas（那會讓錨點失焦、IME 斷鏈）。
canvas.addEventListener("mousedown", (e) => {
  if (!engine) return;
  e.preventDefault();
  const rect = canvas.getBoundingClientRect();
  const xPt = ((e.clientX - rect.left) * (canvas.width / rect.width)) / scale;
  const yPt = ((e.clientY - rect.top) * (canvas.height / rect.height)) / scale;
  const raw = engine.caret_at_json(page, xPt, yPt);
  if (!raw) return;
  caret = JSON.parse(raw);
  imeEl.focus({ preventScroll: true });
  placeAnchor();
});

// 方向鍵：由 caret 契約自行推進（focus 在隱形錨點上，不靠原生選取）。
imeEl.addEventListener("keydown", (e) => {
  if (!engine) return;
  const text = engine.paragraph_text(caret.para) || "";
  const total = cps(text);
  const move = (d) => {
    e.preventDefault();
    caret = { para: caret.para, cp: Math.max(0, Math.min(total, caret.cp + d)) };
    placeAnchor();
  };
  if (e.key === "ArrowLeft") move(-1);
  else if (e.key === "ArrowRight") move(1);
  else if (e.key === "Home") { caret = { para: caret.para, cp: 0 }; e.preventDefault(); placeAnchor(); }
  else if (e.key === "End") { caret = { para: caret.para, cp: total }; e.preventDefault(); placeAnchor(); }
});

// ---- Phase0-6：背景重排排程（軟分頁尾段在瀏覽器空檔續排） ----
// 契約：engine.reflow_step_json() → {"settled":bool,"events":[...]}；
// settled=false 就繼續排。每步是一頁粒度的有界工作（預算 shell 側決定），
// 主執行緒空檔才跑 → 打字路徑永不被尾段重排堵住（輸入延遲閘門的另一半）。
let reflowScheduled = false;
function scheduleIdleReflow() {
  if (reflowScheduled || !engine || engine.is_settled()) return;
  reflowScheduled = true;
  const idle =
    window.requestIdleCallback || ((fn) => setTimeout(() => fn({ timeRemaining: () => 4 }), 4));
  idle(() => {
    reflowScheduled = false;
    if (!engine) return;
    try {
      engine.reflow_step_json();
    } catch (e) {
      console.error("reflow_step_json 解析失敗", e);
      return;
    }
    render(); // 頁數/錨點要跟著更新；未定案則 render() 再排下一輪
  });
}

function render() {
  if (!engine) return;
  // Phase0-6 背景重排排程（shell 側）：預設全速模式 is_settled() 恆真，這行
  // 零成本；一旦 shell 開了 set_sync_budget(Some(n))（長檔打字保 8ms 輸入延遲），
  // 未定案尾段就在瀏覽器空檔（requestIdleCallback）逐步補排，補完重繪當前頁。
  if (!engine.is_settled()) scheduleIdleReflow();
  const dl = JSON.parse(engine.page_display_list_json(page));
  // 逐頁跟隨實際頁面幾何（sectPr）：縮放、畫布尺寸都以 dl.width_pt/height_pt 為準。
  scale = DISPLAY_W_PX / (dl.width_pt || 595);
  canvas.width = Math.round(dl.width_pt * scale);
  canvas.height = Math.round(dl.height_pt * scale);
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = "#000";
  for (const item of dl.items) {
    if (!item.Text) continue;
    const t = item.Text;
    ctx.font = `${t.size_pt * scale}px "Noto Sans TC", "Microsoft JhengHei", sans-serif`;
    ctx.fillText(t.text, t.x * scale, (t.y + t.size_pt) * scale);
  }
  document.getElementById("pageinfo").textContent = `第 ${page + 1} / ${engine.page_count()} 頁（${engine.paragraph_count()} 段）`;
  placeAnchor(); // 重繪後 caret/錨點位置必須跟著重算（編輯→版面變動）
}
