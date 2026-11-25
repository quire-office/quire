// t_f073cd6c 幾何口徑偵察（非考卷）：坐實 display list `Text.y`、`caret_rect.y_pt`
// 與 demo 渲染器基線的三者關係，作為「點擊靶 y 該錨在哪」的書面依據。
//
// 實測結論（本檔跑出的真值，非推測）：
//   Text.y === caret_rect(p,cp).y_pt（同值，同源自 line_bands）
//   demo/main.js:410 `baselineY = (t.y + t.size_pt) * scale` ⇒ 渲染態的字形**基線**
//   在 `t.y + size_pt`，`t.y` 是**字身框頂**（em box top），不是基線。
//   ⇒ 卡面「display list Text.y 同值（=基線）」是把框頂唸成基線；卡面考題 1/2 的
//   y 掃靶一律以 `caret_rect.y_pt` 為錨（= Text.y），本卡 caret_at 中線判制亦以
//   同一 `line_bands` 尺為軸 ⇒ 契約自洽，唯**渲染態**的字形墨跡比 `t.y` 低一個
//   字級（12pt 檔：墨跡實測 [y+2.4, y+19.9]），故「點到字形下半」依中線制屬下一行。
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const PKG = path.join(ROOT, "shells/web/pkg/quire_web.js");
const init = (await import(PKG)).default;
await init({
  module_or_path: new Uint8Array(
    fs.readFileSync(path.join(ROOT, "shells/web/pkg/quire_web_bg.wasm")),
  ),
});
const { WasmEngine } = await import(PKG);

const f = process.argv[2] || "fixtures/style_cascade.docx";
const e = new WasmEngine(fs.readFileSync(path.join(ROOT, f)));
const dl = JSON.parse(e.page_display_list_json(0));
const texts = dl.items.filter((i) => i.Text && i.Text.text.trim());
console.log(`### ${f}: pages=${e.page_count()} paras=${e.paragraph_count()} w_pt=${dl.width_pt}`);

// 以「文字對號」（needle）而非等距取樣找每段行首：顯示列表 Text 項的文字＝該行文字，
// 段落文字＝各行文字串接，故 indexOf 直接給出該行首碼點偏移。
const lines = [];
let offset = 0;
for (let p = 0; p < e.paragraph_count(); p++) {
  const txt = e.paragraph_text(p) || "";
  const chars = [...txt];
  let cur = 0;
  for (const it of texts) {
    const needle = it.Text.text;
    const idx = chars.slice(cur).join("").indexOf(needle);
    if (idx < 0) continue;
    const cp = cur + idx;
    const raw = e.caret_rect_json(p, cp);
    if (!raw) continue;
    const r = JSON.parse(raw);
    lines.push({ para: p, cp, y: r.y_pt, h: r.height_pt, page: r.page, size: it.Text.size_pt, text: needle.slice(0, 10), paraOffset: offset });
    cur = cp + needle.length;
  }
  offset += chars.length;
}
const p0 = lines.filter((l) => l.page === 0);
console.log(`\n第 0 頁以文字對號命中 ${p0.length} 行（顯示列表 Text 項 ${texts.length}）`);
for (const l of p0.slice(0, 12)) {
  console.log(`  para=${l.para} cp=${l.cp} caret_rect.y=${l.y} h=${l.h} size=${l.size} 「${l.text}」`);
}
console.log("\n相鄰行 caret_rect.y 差（=pitch）與中線：");
for (let i = 1; i < Math.min(p0.length, 8); i++) {
  const a = p0[i - 1], b = p0[i];
  const pitch = b.y - a.y;
  console.log(
    `  ${a.para}→${b.para}: y ${a.y} → ${b.y} pitch=${pitch.toFixed(2)} 中線=${((a.y + b.y) / 2).toFixed(2)}` +
      ` （渲染基線＝y+size：${(a.y + a.size).toFixed(1)} / ${(b.y + b.size).toFixed(1)}）`,
  );
}
