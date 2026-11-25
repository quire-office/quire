// t_f073cd6c 探針前置偵察（非考卷，只摸清顯示列表欄位與段落形態）：
// 對實際 pkg/ 產物問「顯示列表 Text 項有哪些欄位、哪些 fixture 適合真滑鼠點擊矩陣」。
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const PKG = path.join(ROOT, "shells/web/pkg/quire_web.js");
const init = (await import(PKG)).default;
const { WasmEngine } = await import(PKG);
await init({
  module_or_path: new Uint8Array(fs.readFileSync(path.join(ROOT, "shells/web/pkg/quire_web_bg.wasm"))),
});

for (const f of process.argv.slice(2)) {
  const p = path.join(ROOT, f);
  if (!fs.existsSync(p)) {
    console.log(`跳過（不存在）：${f}`);
    continue;
  }
  const e = new WasmEngine(fs.readFileSync(p));
  const dl = JSON.parse(e.page_display_list_json(0));
  const texts = dl.items.filter((i) => i.Text);
  const sizes = [...new Set(texts.map((t) => t.Text.size_pt ?? t.Text.sizePt))];
  console.log(
    `\n### ${f}: pages=${e.page_count()} paras=${e.paragraph_count()} w_pt=${dl.width_pt} h_pt=${dl.height_pt} Text項=${texts.length} 字級=${sizes.join(",")}`,
  );
  if (texts.length) console.log("  Text 欄位樣本：", JSON.stringify(texts[0]).slice(0, 260));
  // 逐段行數＋字級分佈（挑多段混級多頁者）。
  const per = [];
  for (let pg = 0; pg < e.page_count(); pg++) {
    const d = JSON.parse(e.page_display_list_json(pg));
    per.push(d.items.filter((i) => i.Text).length);
  }
  console.log("  逐頁 Text 項數：", per.join(","));
}
