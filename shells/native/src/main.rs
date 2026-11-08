//! quire-cli —— 分層鐵律：只准 import quire-engine。
//! 用途：CI 評測（開檔→分頁→顯示列表 JSON）、round-trip 閘門與未來 GUI 殼之前的開發頭。

mod render_svg;

use quire_engine::{dump_display_lists_json, xlsx_roundtrip_diff, Engine, XlsxDoc};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let usage = "用法:
      quire-cli info <file.docx>            頁數/段落數/保留 part 數
      quire-cli page <file.docx> <n>        輸出第 n 頁顯示列表 JSON
      quire-cli dump <file.docx>            輸出全部頁顯示列表 JSON
      quire-cli render <file.docx> --out <dir>
                headless 渲染：每頁 SVG（<stem>-p<頁碼>.svg）＋分頁斷點清單
                （pagebreaks.json：每頁段落分佈/跨頁續排），供 render-lab 跨引擎比對。
      quire-cli roundtrip <file|dir> [--out <path>] [--check]
                                     [--allow part1,part2]
                                     [--quarantine <quarantine.json>]
                開檔→不動→存檔；--check 做 zip 結構級對照（dir＝corpus 全掃）。
                非零離開碼＝有未觸碰部分失真。
                --quarantine：隔離清單 JSON 檔，頂層 files 物件對應
                「相對掃描根路徑（或檔名）→ 隔離原因」；
                清單內檔案跳過評測、不計紅，於彙報單列示。dir 掃描未指定時
                自動找 <dir>/quarantine.json。
      quire-cli roundtrip-xlsx <file|dir> [--check]
                XLSX/XLSM 容器級 round-trip：開檔→（X1 不改任何 part）→存回，
                --check 斷言 byte-identical（zip 結構級，不碰內容）。
                dir＝遞迴掃 *.xlsx 與 *.xlsm。非零離開碼＝有失真。
      quire-cli xlsx-info <file.xlsx>       workbook 最小模型 JSON（sheet 清單/
                definedNames 計數/rels 對照/dimension+row 計數）。
      quire-cli odt-render <file.odt>       ODT 文字流投影＋頁面幾何：讀 .odt →
                text:p/text:h＋字級級聯投影 → layout（頁寬高與四邊邊距取自
                styles.xml 的 style:page-layout 經 master-page 鏈換算；解不到
                者回退 LO 預設頁 A4 portrait＋四邊 2.0cm——非
                LayoutParams::default() 的假 A4＋1 吋）輸出固定 JSON 欄位：
                paragraph_count／total_text_len／page_breaks／page／page_source。";
    if args.len() < 3 {
        eprintln!("{usage}");
        return ExitCode::from(2);
    }
    let cmd = args[1].as_str();
    if cmd == "roundtrip" {
        return roundtrip(&args[2..], usage);
    }
    if cmd == "roundtrip-xlsx" {
        return roundtrip_xlsx(&args[2..], usage);
    }
    if cmd == "xlsx-info" {
        return xlsx_info(&args[2]);
    }
    if cmd == "odt-render" {
        return odt_render(&args[2]);
    }
    if cmd == "render" {
        return render(&args[2..], usage);
    }
    let path = &args[2];
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("讀檔失敗 {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let engine = match Engine::from_docx_bytes(&bytes) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("解析失敗: {e}");
            return ExitCode::from(1);
        }
    };
    match cmd {
        "info" => {
            println!("檔案: {path}");
            println!("頁數: {}", engine.page_count());
            println!("段落數: {}", engine.paragraph_count());
            println!("保留 zip part 數: {}", engine.preserved_part_count());
            if engine.footnote_rounds() > 0 {
                println!("頁底註腳: 收斂 {} 輪", engine.footnote_rounds());
            }
            if let Some(lines) = engine.page_lines(0) {
                println!("--- 第 0 頁預覽 ---");
                for l in lines.iter().take(6) {
                    println!("  {l}");
                }
            }
            ExitCode::SUCCESS
        }
        "page" => {
            let page: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            match engine.page_display_list_json(page) {
                Some(json) => {
                    println!("{json}");
                    ExitCode::SUCCESS
                }
                None => {
                    eprintln!("頁碼 {page} 不存在（共 {} 頁）", engine.page_count());
                    ExitCode::from(1)
                }
            }
        }
        "dump" => {
            println!("{}", dump_display_lists_json(&engine));
            ExitCode::SUCCESS
        }
        "footnotes" => {
            println!("{}", engine.footnotes_json());
            ExitCode::SUCCESS
        }
        other => {
            eprintln!(
                "未知指令: {other}
{usage}"
            );
            ExitCode::from(2)
        }
    }
}

/// render 子命令：docx → 每頁 SVG ＋ pagebreaks.json（render-lab 骨架）。
fn render(rest: &[String], usage: &str) -> ExitCode {
    let mut out: Option<PathBuf> = None;
    let mut target: Option<String> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--out" => {
                i += 1;
                let Some(p) = rest.get(i) else {
                    eprintln!("--out 需要參數\n{usage}");
                    return ExitCode::from(2);
                };
                out = Some(PathBuf::from(p));
            }
            other if other.starts_with("--") => {
                eprintln!("未知選項: {other}\n{usage}");
                return ExitCode::from(2);
            }
            other => target = Some(other.to_string()),
        }
        i += 1;
    }
    let (Some(target), Some(out_dir)) = (target, out) else {
        eprintln!("需要 <file.docx> 與 --out <dir>\n{usage}");
        return ExitCode::from(2);
    };
    let src = PathBuf::from(&target);
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("doc")
        .to_string();
    let bytes = match std::fs::read(&src) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("讀檔失敗 {target}: {e}");
            return ExitCode::from(1);
        }
    };
    let engine = match Engine::from_docx_bytes(&bytes) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("解析失敗: {e}");
            return ExitCode::from(1);
        }
    };
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("建立輸出目錄失敗 {}: {e}", out_dir.display());
        return ExitCode::from(1);
    }
    let mut written = 0usize;
    for p in 0..engine.page_count() {
        let Some(dl) = engine.page_display_list(p) else {
            continue;
        };
        let svg = render_svg::page_to_svg(&dl);
        let f = out_dir.join(format!("{stem}-p{p}.svg"));
        if let Err(e) = std::fs::write(&f, svg) {
            eprintln!("寫檔失敗 {}: {e}", f.display());
            return ExitCode::from(1);
        }
        written += 1;
    }
    let pb = out_dir.join("pagebreaks.json");
    if let Err(e) = std::fs::write(&pb, engine.page_breaks_json()) {
        eprintln!("寫檔失敗 {}: {e}", pb.display());
        return ExitCode::from(1);
    }
    println!(
        "render: {} → {} 頁 SVG ＋ {}（{} 段落）",
        target,
        written,
        pb.display(),
        engine.paragraph_count()
    );
    ExitCode::SUCCESS
}

/// quarantine.json 的 schema：{"files": {"<路徑或檔名>": "<隔離原因>"}}
#[derive(serde::Deserialize)]
struct QuarantineFile {
    #[serde(default)]
    files: std::collections::BTreeMap<String, String>,
}

/// roundtrip 子命令：單一檔案或 corpus 目錄（遞迴掃 *.docx）。
fn roundtrip(rest: &[String], usage: &str) -> ExitCode {
    let mut check = false;
    let mut out: Option<PathBuf> = None;
    let mut allow: Vec<String> = Vec::new();
    let mut quarantine_arg: Option<PathBuf> = None;
    let mut target: Option<String> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--check" => check = true,
            "--out" => {
                i += 1;
                let Some(p) = rest.get(i) else {
                    eprintln!("--out 需要參數\n{usage}");
                    return ExitCode::from(2);
                };
                out = Some(PathBuf::from(p));
            }
            "--quarantine" => {
                i += 1;
                let Some(p) = rest.get(i) else {
                    eprintln!("--quarantine 需要檔案路徑\n{usage}");
                    return ExitCode::from(2);
                };
                quarantine_arg = Some(PathBuf::from(p));
            }
            "--allow" => {
                i += 1;
                let Some(list) = rest.get(i) else {
                    eprintln!("--allow 需要逗號分隔的 part 清單\n{usage}");
                    return ExitCode::from(2);
                };
                allow = list
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            other if other.starts_with("--") => {
                eprintln!("未知選項: {other}\n{usage}");
                return ExitCode::from(2);
            }
            other => target = Some(other.to_string()),
        }
        i += 1;
    }
    let Some(target) = target else {
        eprintln!("需要 <file|dir>\n{usage}");
        return ExitCode::from(2);
    };
    let files = match collect_docx(&PathBuf::from(&target)) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("掃描 {target} 失敗: {e}");
            return ExitCode::from(1);
        }
    };
    if files.is_empty() {
        eprintln!("在 {target} 找不到 .docx（corpus 是空的？）");
        return ExitCode::from(1);
    }
    // 隔離清單：鍵＝檔名或相對掃描根的相對路徑；命中者跳過評測、不計紅。
    let target_dir = PathBuf::from(&target);
    let qpath = quarantine_arg.or_else(|| {
        target_dir
            .is_dir()
            .then(|| target_dir.join("quarantine.json"))
            .filter(|p| p.exists())
    });
    let quarantine: std::collections::BTreeMap<String, String> = match &qpath {
        Some(p) => match std::fs::read(p) {
            Ok(b) => serde_json::from_slice::<QuarantineFile>(b.as_slice())
                .map(|q| q.files)
                .unwrap_or_else(|e| {
                    eprintln!(
                        "quarantine JSON 解析失敗 {}: {e}（視為空清單）",
                        p.display()
                    );
                    Default::default()
                }),
            Err(_) => Default::default(),
        },
        None => Default::default(),
    };
    let mut quarantined: Vec<(String, String)> = Vec::new();
    let files: Vec<PathBuf> = files
        .into_iter()
        .filter(|f| {
            let rel = target_dir
                .is_dir()
                .then(|| f.strip_prefix(&target_dir).unwrap_or(f.as_path()))
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let fname = f
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let hit = quarantine
                .get(&rel)
                .or_else(|| quarantine.get(&fname))
                .cloned();
            match hit {
                Some(reason) => {
                    quarantined.push((if rel.is_empty() { fname } else { rel }, reason));
                    false
                }
                None => true,
            }
        })
        .collect();
    if !quarantined.is_empty() {
        println!("quarantine: {} 檔隔離（不計紅）：", quarantined.len());
        for (f, reason) in &quarantined {
            println!("  Q {f} — {reason}");
        }
    }

    let mut failed = 0usize;
    let mut identical = 0usize;
    for file in &files {
        let bytes = match std::fs::read(file) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("讀檔失敗 {}: {e}", file.display());
                failed += 1;
                continue;
            }
        };
        let engine = match Engine::from_docx_bytes(&bytes) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("解析失敗 {}: {e}", file.display());
                failed += 1;
                continue;
            }
        };
        let saved = match engine.save_docx() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("存檔失敗 {}: {e}", file.display());
                failed += 1;
                continue;
            }
        };
        if let Some(o) = &out {
            if let Err(e) = std::fs::write(o, &saved) {
                eprintln!("寫檔失敗 {}: {e}", o.display());
                return ExitCode::from(1);
            }
            println!("存檔 → {}", o.display());
        }
        if !check {
            continue;
        }
        match Engine::roundtrip_diff(&bytes, &saved, &allow) {
            Ok(d) if d.pass => {
                if d.byte_identical {
                    identical += 1;
                    println!(
                        "✓ {}（byte-identical，{} parts）",
                        file.display(),
                        engine.preserved_part_count()
                    );
                } else {
                    println!(
                        "✓ {}（未觸碰零失真，白名單變動 {:?}）",
                        file.display(),
                        d.whitelisted_changed
                    );
                }
            }
            Ok(d) => {
                failed += 1;
                eprintln!("✗ {} round-trip 失真:\n{}", file.display(), d.report());
            }
            Err(e) => {
                failed += 1;
                eprintln!("✗ {} 對照失敗: {e}", file.display());
            }
        }
    }
    if check {
        println!(
            "roundtrip --check: {} 檔（另有 {} 檔隔離不計紅），byte-identical {}，失敗 {}",
            files.len(),
            quarantined.len(),
            identical,
            failed
        );
    }
    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// 目標是檔案→單檔；是目錄→遞迴收集 *.docx（名稱排序，CI 可重現）。
fn collect_docx(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if path.is_dir() {
        collect_dir(path, &mut out)?;
        out.sort();
    } else if path.exists() {
        out.push(path.to_path_buf());
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "路徑不存在",
        ));
    }
    Ok(out)
}

fn collect_dir(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            collect_dir(&p, out)?;
        } else if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            n.to_ascii_lowercase().ends_with(".docx") && !n.starts_with('.') && !n.starts_with("~$")
        }) {
            out.push(p);
        }
    }
    Ok(())
}

// ---------------- XLSX（X1：容器級無損 round-trip 閘門） ----------------

/// roundtrip-xlsx 子命令：XLSX/XLSM 開檔→不動→存回，--check 斷言 byte-identical。
fn roundtrip_xlsx(rest: &[String], usage: &str) -> ExitCode {
    let mut check = false;
    let mut target: Option<String> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--check" => check = true,
            other if other.starts_with("--") => {
                eprintln!("未知選項: {other}\n{usage}");
                return ExitCode::from(2);
            }
            other => target = Some(other.to_string()),
        }
        i += 1;
    }
    let Some(target) = target else {
        eprintln!("需要 <file|dir>\n{usage}");
        return ExitCode::from(2);
    };
    let files = match collect_xlsx(&PathBuf::from(&target)) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("掃描 {target} 失敗: {e}");
            return ExitCode::from(1);
        }
    };
    if files.is_empty() {
        eprintln!("在 {target} 找不到 .xlsx/.xlsm（corpus-xlsx 是空的？）");
        return ExitCode::from(1);
    }
    let mut failed = 0usize;
    let mut identical = 0usize;
    for file in &files {
        let bytes = match std::fs::read(file) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("讀檔失敗 {}: {e}", file.display());
                failed += 1;
                continue;
            }
        };
        let doc = match XlsxDoc::from_xlsx_bytes(&bytes) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("開啟失敗 {}: {e}", file.display());
                failed += 1;
                continue;
            }
        };
        let saved = match doc.save_xlsx() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("存檔失敗 {}: {e}", file.display());
                failed += 1;
                continue;
            }
        };
        if !check {
            continue;
        }
        match xlsx_roundtrip_diff(&bytes, &saved) {
            Ok(d) if d.byte_identical && d.pass => {
                identical += 1;
                println!(
                    "✓ {}（byte-identical，{} parts）",
                    file.display(),
                    doc.part_count()
                );
            }
            Ok(d) => {
                failed += 1;
                eprintln!("✗ {} round-trip 失真:\n{}", file.display(), d.report());
            }
            Err(e) => {
                failed += 1;
                eprintln!("✗ {} 對照失敗: {e}", file.display());
            }
        }
    }
    if check {
        println!(
            "roundtrip-xlsx --check: {} 檔，byte-identical {}，失敗 {}",
            files.len(),
            identical,
            failed
        );
    }
    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// xlsx-info 子命令：workbook 最小模型 JSON 直出（除錯/斷言用）。
fn xlsx_info(path: &str) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("讀檔失敗 {path}: {e}");
            return ExitCode::from(1);
        }
    };
    match XlsxDoc::from_xlsx_bytes(&bytes) {
        Ok(doc) => {
            println!("{}", doc.workbook_model_json());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("開啟失敗 {path}: {e}");
            ExitCode::from(1)
        }
    }
}

/// odt-render 子命令（ODF-C3 首切片＋ODF-C4 頁面幾何）：讀 .odt → ODF 文字流
/// 投影 → layout（頁寬高與四邊邊距取自 styles.xml `style:page-layout` 經
/// `style:master-page` 鏈換算；解不到者回退 **LO 預設頁** A4 portrait＋四邊
/// 2.0cm，數值以語料 LO PDF 真值實測覆核，見 `docs/odf-c4-anatomy.md` §5）→
/// 固定 JSON `{"paragraph_count","total_text_len","page_breaks","page",
/// "page_source"}`。`page` 為實際生效的頁寬高與 `[上,右,下,左]` 邊距（pt），
/// `page_source` 為生效原因（`para_master_page_name`／`default_master_styles`
/// ／`master_page_without_layout`／`no_master_page`）——鑑別力面：區分「真讀到
/// page-layout」與「回退 LO 預設」。非零離開碼＝開檔／投影失敗（老實報錯，不吞）。
fn odt_render(path: &str) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("讀檔失敗 {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let bundle = match quire_odf::OdfBundle::open(&bytes) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("開啟失敗 {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let projection = match quire_odf::project(&bundle) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("投影失敗 {path}: {e}");
            return ExitCode::from(1);
        }
    };
    // ODF-C4：頁面幾何鏈取代 LayoutParams::default()（假 A4＋1 吋邊距）。
    // 加密等錯誤在 project 已被擋，這裡同口徑老實報錯。
    let geom = match quire_odf::page_geometry(&bundle) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("頁面幾何解析失敗 {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let params = geom.to_layout_params();
    let (texts, _) = projection.flatten();
    let page_breaks = quire_odf::page_break_positions(&texts, &params);
    let out = serde_json::json!({
        "paragraph_count": projection.paragraph_count(),
        "total_text_len": projection.total_text_len,
        "page_breaks": page_breaks,
        "page": {
            "w_pt": geom.page_w_pt(),
            "h_pt": geom.page_h_pt(),
            "margins_pt": geom.margins_pt(),
        },
        "page_source": geom.source.as_str(),
    });
    match serde_json::to_string(&out) {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("JSON 序列化失敗: {e}");
            ExitCode::from(1)
        }
    }
}

/// 目錄遞迴收集 *.xlsx＋*.xlsm（名稱排序，CI 可重現）；過濾規則同 collect_docx。
fn collect_xlsx(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if path.is_dir() {
        collect_xlsx_dir(path, &mut out)?;
        out.sort();
    } else if path.exists() {
        out.push(path.to_path_buf());
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "路徑不存在",
        ));
    }
    Ok(out)
}

fn collect_xlsx_dir(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            collect_xlsx_dir(&p, out)?;
        } else if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            let l = n.to_ascii_lowercase();
            (l.ends_with(".xlsx") || l.ends_with(".xlsm"))
                && !n.starts_with('.')
                && !n.starts_with("~$")
        }) {
            out.push(p);
        }
    }
    Ok(())
}
