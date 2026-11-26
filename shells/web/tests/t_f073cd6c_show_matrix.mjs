// t_f073cd6c 證據檢視（非考卷）：把點擊矩陣 JSON 印成人讀表，核對逐行契約 y/pitch。
import fs from "node:fs";
const p = process.argv[2] || "build/u4m-click-matrix.json";
const d = JSON.parse(fs.readFileSync(p, "utf8"));
let prev = null;
for (const r of d.rows.slice(0, 44)) {
  const dy = prev === null ? null : +(r.rowY - prev).toFixed(2);
  console.log(
    `para=${String(r.para).padStart(2)} cp=${String(r.cp).padStart(3)} y=${String(r.rowY).padStart(9)} size=${r.size} dy=${String(dy).padStart(6)} 「${r.text}」`,
  );
  prev = r.rowY;
}
console.log(`\n--- clicks（前 12 筆）---`);
for (const c of d.clicks.slice(0, 12)) {
  console.log(JSON.stringify(c));
}
