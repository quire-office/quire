#!/usr/bin/env node
// 防再犯掃描鎖（HOTFIX-R1B）：庫碼不得無 cfg 呼叫 std 時鐘。
//
// 背景鐵證：CI run 46092813695 `wasm-size` job——R1-b 計時插樁在
// `core/engine-api` **庫碼**（會編進 wasm 產物）無條件呼叫
// `std::time::Instant::now()`，wasm32-unknown-unknown 上該呼叫直接 panic
// （`time not implemented on this platform` → wasm `unreachable`），
// `node scripts/wasm-smoke.mjs` 全紅。native 端全綠掩護了它，直到 wasm 才炸。
//
// 本鎖把教訓釘成閘門：掃 `core/**/src/**/*.rs`（排除 tests／examples／
// benches 目錄）中所有 `Instant::now()`／`SystemTime::now()`／`UNIX_EPOCH`
// 呼叫，每一處必須被下列**cfg 隔離模式**之一覆蓋，否則紅並列出檔:行：
//   1. `#[cfg(not(target_arch = "wasm32"))]` 護欄（函數／項目層，括號深追蹤）；
//   2. `#[cfg(test)]` 模組區（測試碼不進 wasm 產物）；
//   3. 註解（行註解／區塊註解，含 doc comment）中的提及不算呼叫。
// 注意：不提供行內放行註解——護欄要嘛在、要嘛紅，不留側門。
//
// 用法：
//   node scripts/wasm-clock-scan.mjs              正常掃描（庫碼乾淨⇒綠，exit 0）
//   node scripts/wasm-clock-scan.mjs --self-test  自測鎖的咬合力（合成樣本，
//                                                 鎖壞掉時 exit 1——防「假綠鎖」）
//   R1B_CLOCK_BUGGY=1 node scripts/wasm-clock-scan.mjs
//                                                 回塞鑑別：把 `#[cfg(not(target_arch
//                                                 = "wasm32"))]` 護欄從掃描視圖中
//                                                 移除（行號保留），掃描**必紅**；
//                                                 若掃不出紅＝鎖失去鑑別力，同樣
//                                                 exit 1 并报「鑑別失敗」。
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const CORE = path.join(ROOT, "core");

// 呼叫點模式：路徑寫法不敏感（std::time::Instant::now 或 import 後 Instant::now）。
const CALL_RE =
  /\b(?:Instant|SystemTime)\s*::\s*now\s*\(|\bUNIX_EPOCH\b/g;
// 護欄模式：cfg 屬性行（容忍空白變體）。
const GUARD_NATIVE_RE = /#\[cfg\(\s*not\(\s*target_arch\s*=\s*"wasm32"\s*\)\s*\)\s*\]/;
const GUARD_TEST_RE = /#\[cfg\(\s*test\s*\)\s*\]/;

// ---- 註解遮罩：把註解字元替換成空格（保留行號與欄位數），字串內容保留 ----
// 簡化假設：Rust 原始字串 r"..." 與字串內未成對括弧不影響本掃描（只找固定模式）。
function maskComments(src) {
  const out = src.split("\n").map((l) => l.split(""));
  let inBlock = 0; // /* */ 深度（Rust 區塊註解可巢狀）
  for (let i = 0; i < out.length; i++) {
    const line = out[i];
    let inStr = false; // 簡化：單行字串態
    let j = 0;
    while (j < line.length) {
      const two = line[j] + (line[j + 1] ?? "");
      if (inBlock > 0) {
        if (two === "/*") {
          inBlock++;
          line[j] = line[j + 1] = " ";
          j += 2;
          continue;
        }
        if (two === "*/") {
          inBlock--;
          line[j] = line[j + 1] = " ";
          j += 2;
          continue;
        }
        line[j] = " ";
        j += 1;
        continue;
      }
      if (inStr) {
        if (line[j] === "\\") j += 2;
        else {
          if (line[j] === '"') inStr = false;
          j += 1;
        }
        continue;
      }
      if (two === "//") {
        for (let k = j; k < line.length; k++) line[k] = " ";
        break;
      }
      if (line[j] === '"') {
        inStr = true;
        j += 1;
        continue;
      }
      if (line[j] === "/" && line[j + 1] === "*") {
        inBlock = 1;
        line[j] = line[j + 1] = " ";
        j += 2;
        continue;
      }
      j += 1;
    }
  }
  return out.map((l) => l.join("")).join("\n");
}

// ---- cfg 區域追蹤：回傳「受保護行集合」（1-based），括號深追蹤項目體 ----
// 保護區 = cfg 屬性行之後的第一個項目（fn／mod／struct／impl／const／static／
// trait／type／使用者條目），從其起始行起到括號平衡歸零為止。
function guardedLines(masked, guardRe) {
  const lines = masked.split("\n");
  const guarded = new Set();
  for (let i = 0; i < lines.length; i++) {
    if (!guardRe.test(lines[i])) continue;
    // 屬性簇：連續的 `#...`／`//`／空行向後找項目起始行。
    let s = i + 1;
    while (
      s < lines.length &&
      (/^\s*(#|\/\/|\/\*|\*)/.test(lines[s]) || lines[s].trim() === "")
    ) {
      s++;
    }
    if (s >= lines.length) {
      guarded.add(i + 1);
      continue;
    }
    // 從項目起始行追蹤括號平衡；無括號的條目（如 use、單行 const）以 `;` 收尾。
    let depth = 0;
    let started = false;
    let ended = false;
    for (let k = s; k < lines.length && !ended; k++) {
      guarded.add(k + 1);
      const code = lines[k];
      for (const ch of code) {
        if (ch === "{") {
          depth++;
          started = true;
        } else if (ch === "}") {
          depth--;
          if (started && depth <= 0) ended = true;
        }
      }
      if (!started && code.includes(";")) ended = true;
      if (!started && k > s && /^\s*\]/.test(code)) ended = true; // derive(...) 等屬性簇防禦
    }
  }
  return guarded;
}

// ---- 檔案枚舉：core/**/src/**/*.rs，排除 tests／examples／benches 目錄 ----
function* walk(dir) {
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, ent.name);
    if (ent.isDirectory()) {
      if (["target", "node_modules", "tests", "examples", "benches"].includes(ent.name))
        continue;
      yield* walk(p);
    } else if (ent.name.endsWith(".rs")) {
      yield p;
    }
  }
}

function listLibraryFiles() {
  if (!fs.existsSync(CORE)) return [];
  return [...walk(CORE)]
    .filter((p) => p.includes(`${path.sep}src${path.sep}`))
    .sort();
}

// ---- 掃描主體：回傳違規清單 [{file, line, code}] ----
function scanBuffer(src, displayPath) {
  const masked = maskComments(src);
  const nativeGuarded = guardedLines(masked, GUARD_NATIVE_RE);
  const testGuarded = guardedLines(masked, GUARD_TEST_RE);
  const violations = [];
  const lines = masked.split("\n");
  const rawLines = src.split("\n");
  for (let i = 0; i < lines.length; i++) {
    CALL_RE.lastIndex = 0;
    if (!CALL_RE.test(lines[i])) continue;
    if (nativeGuarded.has(i + 1) || testGuarded.has(i + 1)) continue;
    violations.push({ file: displayPath, line: i + 1, code: rawLines[i].trim() });
  }
  return violations;
}

function scanRepo({ stripNativeGuards }) {
  const all = [];
  for (const f of listLibraryFiles()) {
    let src = fs.readFileSync(f, "utf8");
    if (stripNativeGuards) {
      // 回塞鑑別：把 cfg 護欄行整行遮罩掉（**保留行號**），模擬「有人拆護欄」的
      // 復辟態——掃描視圖裡 `r1b_now_native` 的 Instant::now() 自此無cover。
      src = src
        .split("\n")
        .map((l) => (GUARD_NATIVE_RE.test(l) ? "" : l))
        .join("\n");
    }
    all.push(...scanBuffer(src, path.relative(ROOT, f)));
  }
  return all;
}

// ---- 自測模式：鎖自己的咬合力考卷（防「假綠鎖」）----
function selfTest() {
  const cases = [
    { name: "裸呼叫必紅", src: "fn f() {\n    let t = std::time::Instant::now();\n}\n", want: 1 },
    {
      name: "native 護欄內必綠",
      src: '#[cfg(not(target_arch = "wasm32"))]\nfn f() {\n    let t = std::time::Instant::now();\n}\n',
      want: 0,
    },
    {
      name: "cfg(test) 模組內必綠",
      src: '#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        let t = std::time::Instant::now();\n    }\n}\nfn g() {}\n',
      want: 0,
    },
    {
      name: "doc comment 提及必綠",
      src: "/// 使用 `std::time::Instant::now()` 計時。\nfn f() {}\n",
      want: 0,
    },
    { name: "SystemTime 裸呼叫必紅", src: "fn f() {\n    let t = std::time::SystemTime::now();\n}\n", want: 1 },
    { name: "UNIX_EPOCH 裸引用必紅", src: "fn f() {\n    let t = std::time::UNIX_EPOCH;\n}\n", want: 1 },
    {
      name: "護欄巢狀多行簽名仍覆蓋",
      src:
        '#[cfg(not(target_arch = "wasm32"))]\n#[inline]\nfn f(\n    a: u32,\n) -> u32 {\n    let _t = std::time::Instant::now();\n    a\n}\n',
      want: 0,
    },
    {
      name: "護欄區域後的裸呼叫必紅（邊界不外溢）",
      src:
        '#[cfg(not(target_arch = "wasm32"))]\nfn g() {}\nfn f() {\n    let _t = Instant::now();\n}\n',
      want: 1,
    },
  ];
  let failed = 0;
  for (const c of cases) {
    const got = scanBuffer(c.src, "<self-test>").length;
    if (got === c.want) {
      console.log(`✓ self-test ${c.name}（違規 ${got}/${c.want}）`);
    } else {
      failed++;
      console.error(`✗ self-test ${c.name}：期望違規 ${c.want} 筆，實得 ${got} 筆——掃描鎖本身壞了`);
    }
  }
  if (failed > 0) {
    console.error(`✗ wasm-clock-scan 自測失敗：${failed} 件——鎖不可信，CI 必須紅`);
    process.exit(1);
  }
  console.log(`✓ wasm-clock-scan 自測全綠（${cases.length} 件咬合力考卷）`);
}

// ---- 入口 ----
const argv = process.argv.slice(2);
if (argv.includes("--self-test")) {
  selfTest();
  process.exit(0);
}

const buggy = process.env.R1B_CLOCK_BUGGY === "1";
if (buggy) {
  console.log("★ 回塞鑑別態（R1B_CLOCK_BUGGY=1）：掃描視圖已移除 wasm32 護欄——本態必紅。");
  const v = scanRepo({ stripNativeGuards: true });
  if (v.length === 0) {
    console.error(
      "✗ 鑑別失敗：拆掉護欄後掃描仍全綠——掃描鎖失去咬合力（防再犯鎖形同虛設），exit 1。",
    );
    process.exit(1);
  }
  console.error(`✗ 回塞態如預期紅：${v.length} 處裸時鐘呼叫失去護欄覆蓋——`);
  for (const x of v) console.error(`  ${x.file}:${x.line}  ${x.code}`);
  console.error("  （正常態護欄在時這些呼叫全部由 r1b_now() 抽象吸收，掃描綠。）");
  process.exit(1);
}

const violations = scanRepo({ stripNativeGuards: false });
if (violations.length > 0) {
  console.error(
    `✗ wasm 庫碼含無 cfg 隔離的時鐘呼叫（${violations.length} 處）——` +
      `wasm32-unknown-unknown 上 Instant::now()／SystemTime::now() 直接 panic（unreachable）。`,
  );
  for (const x of violations) console.error(`  ${x.file}:${x.line}  ${x.code}`);
  console.error(
    "  修法：改經 core/engine-api/src/lib.rs 的 r1b_now() 抽象，或為該項目加\n" +
      '        #[cfg(not(target_arch = "wasm32"))] 護欄（掃描鎖認得護欄與 cfg(test) 區）。',
  );
  process.exit(1);
}
console.log("✓ wasm-clock-scan：core 庫碼零無護欄時鐘呼叫（wasm 產物安全）");
