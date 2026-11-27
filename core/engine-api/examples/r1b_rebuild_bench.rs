//! R1-b 基準：結構編輯／Undo 的「全檔重開」成本閘門（卡片門檻見 docs/r1b-perf-baseline.md）。
//!
//! 跑法（PM 驗收同口徑）：
//!   CARGO_PROFILE_RELEASE_DEBUG=1 cargo run --release -p quire-engine --example r1b_rebuild_bench
//! 鑑別開關：
//!   R1B_BUGGY=1 → rebuild_derived 走回「全檔 save＋from_docx_bytes 整包重開」舊路徑，
//!                  bench 必須明顯退回基線級別（證明考卷真的測到這條路徑）。
//! 可調：R1B_ROUNDS（取樣輪數，預設 12）。
//!
//! 與 p6_bench 的分工：p6_bench 監控 `LayoutEngine` 的打字增量路徑；本卷走
//! **真實 `Engine`**（quire-engine）路徑——`SetParagraphStyle`／`undo()` 等結構
//! 編輯觸發的 `rebuild_derived()` 完全不經 p6_bench，這是本卡存在的理由。
//!
//! 量測五個動作的 p50/p95（單位 ms）：
//!   (a) SetParagraphStyle —— structural=true ⇒ 每次必觸發 rebuild_derived()
//!   (b) undo()            —— 每次必觸發 rebuild_derived()
//!   (c) InsertText        —— structural=false ⇒ 增量路徑（打字延遲閘門）
//!   (d) FormatTextRange   —— structural=true ⇒ rebuild_derived()
//!   (e) 零編輯 save_docx() —— OoxmlDoc::save() 全檔 entry 比對成本
//!
//! workload：corpus/poi/test-data_document_bug65649.docx（PM 親測 44s 級的真实公文檔，
//! 橫向/直向混排多節）＋合成 500 頁含表格＋註腳檔（語料內無現成 500 頁含註腳檔，
//! 本卷用純 std 合成器在記憶體生成，實際頁數由開檔後 page_count() 老實回報）。
use std::time::Instant;

use quire_engine::{
    DocPos, DocRange, Engine, Mutation, ParaPropsPatch, RunDirectPatch, SubDocTarget,
};

// ---------------------------------------------------------------------------
// 通用工具（比照 p6_bench 的 stats/rss 輸出風格）
// ---------------------------------------------------------------------------

fn stats(times: &[f64]) -> (f64, f64, f64) {
    let mut v = times.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    let p95 = v[((v.len() as f64 * 0.95) as usize).min(v.len() - 1)];
    (mean, v[v.len() / 2], p95)
}

fn rss_mb() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest
                .trim()
                .trim_end_matches(" kB")
                .trim()
                .parse()
                .unwrap_or(0)
                / 1024;
        }
    }
    0
}

fn repo_rel(rel: &str) -> String {
    format!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../{}"), rel)
}

// ---------------------------------------------------------------------------
// 最小 docx 合成器（純 std、STORE、零內嵌資源——同 e3_hardening 考卷紀律）
// ---------------------------------------------------------------------------

struct ZipTestHelper {
    buf: Vec<u8>,
    centrals: Vec<Vec<u8>>,
    count: usize,
}

impl ZipTestHelper {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            centrals: Vec::new(),
            count: 0,
        }
    }

    fn add(&mut self, name: &str, data: &[u8]) {
        let offset = self.buf.len() as u64;
        let csize = data.len() as u64;
        let crc = crc32(data);

        self.buf.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        self.buf.extend_from_slice(&20u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0x1234u16.to_le_bytes());
        self.buf.extend_from_slice(&0x5678u16.to_le_bytes());
        self.buf.extend_from_slice(&crc.to_le_bytes());
        self.buf.extend_from_slice(&(csize as u32).to_le_bytes());
        self.buf.extend_from_slice(&(csize as u32).to_le_bytes());
        self.buf
            .extend_from_slice(&(name.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(name.as_bytes());
        self.buf.extend_from_slice(data);

        let mut c = Vec::new();
        c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        c.extend_from_slice(&0x031Eu16.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&[]); // 佔位（下方 central 再補齊欄位）
                                         // 上面這行是防呆：central 欄位一律重新完整寫出，避免半截記錄。
        c.extend_from_slice(&0x1234u16.to_le_bytes());
        c.extend_from_slice(&0x5678u16.to_le_bytes());
        c.extend_from_slice(&crc.to_le_bytes());
        c.extend_from_slice(&(csize as u32).to_le_bytes());
        c.extend_from_slice(&(csize as u32).to_le_bytes());
        c.extend_from_slice(&(name.len() as u16).to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&(offset as u32).to_le_bytes());
        c.extend_from_slice(name.as_bytes());
        self.centrals.push(c);
        self.count += 1;
    }

    fn finish(&self) -> Vec<u8> {
        let mut out = self.buf.clone();
        let cd_off = out.len() as u64;
        for c in &self.centrals {
            out.extend_from_slice(c);
        }
        let cd_size = out.len() as u64 - cd_off;

        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(self.count as u16).to_le_bytes());
        out.extend_from_slice(&(self.count as u16).to_le_bytes());
        out.extend_from_slice(&(cd_size as u32).to_le_bytes());
        out.extend_from_slice(&(cd_off as u32).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}

/// CRC-32（IEEE，zip 多項式）——純 std 實作（範例零额外依賴，dev-deps 進不了 examples）。
fn crc32(data: &[u8]) -> u32 {
    fn table() -> &'static [u32; 256] {
        static T: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
        T.get_or_init(|| {
            let mut t = [0u32; 256];
            for (i, v) in t.iter_mut().enumerate() {
                let mut c = i as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 {
                        0xEDB8_8320 ^ (c >> 1)
                    } else {
                        c >> 1
                    };
                }
                *v = c;
            }
            t
        })
    }
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = table()[(c ^ b as u32) as usize & 0xFF] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

// ---------------------------------------------------------------------------
// 合成 500 頁 workload：正文段落＋週期性表格＋週期性註腳引用（含表格＋註腳）
// ---------------------------------------------------------------------------

const CT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOC_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rIdNotes" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>
</Relationships>"#;

/// styles.xml 帶 docDefaults＋Normal／Heading1 宣告（級聯層有真值可吃，
/// 合成檔才走與真實檔同形的細尺校準路徑，基準數字不外推自零宣告捷徑）。
const STYLES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:eastAsia="標楷體"/><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults>
  <w:style w:type="paragraph" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:spacing w:line="240"/></w:pPr><w:rPr><w:sz w:val="32"/></w:rPr></w:style>
  <w:style w:type="character" w:styleId="FootnoteReference"><w:name w:val="footnote reference"/><w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>
  <w:style w:type="paragraph" w:styleId="FootnoteText"><w:name w:val="footnote text"/><w:rPr><w:sz w:val="20"/></w:rPr></w:style>
</w:styles>"#;

fn para_text(i: usize) -> String {
    match i % 7 {
        0 => format!("第 {i} 段：{}", "內容文字測試样本 mixed words。".repeat(18)),
        3 => format!("短段落 {i}。"),
        _ => format!("第 {i} 段：{}", "中文內容測試文字標點符號。".repeat(14)),
    }
}

/// 註腳本體（固定 60 條，正文引用循環使用——錨點數遠大於條目數是真實公文常態）。
fn footnotes_xml() -> String {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>
<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>"#,
    );
    for id in 1..=60 {
        s.push_str(&format!(
            r#"<w:footnote w:id="{id}"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr><w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t>註腳條目 {id}：這是註腳本體的文字內容，用於驗證註腳排布收斂路徑的成本。</w:t></w:r></w:p></w:footnote>"#
        ));
    }
    s.push_str("</w:footnotes>");
    s
}

fn synth_docx(n_paras: usize) -> Vec<u8> {
    let mut body = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
    );
    for i in 0..n_paras {
        // 每 47 段插一張 4 列表格（2 列），表格進 body 流。
        if i % 47 == 46 {
            body.push_str(r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="1440"/><w:gridCol w:w="1440"/><w:gridCol w:w="1440"/><w:gridCol w:w="1440"/></w:tblGrid>"#);
            for _row in 0..2 {
                body.push_str("<w:tr>");
                for c in 0..4 {
                    body.push_str(&format!(
                        r#"<w:tc><w:tcPr><w:tcW w:w="1440" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>表格格內文字 {c}。</w:t></w:r></w:p></w:tc>"#
                    ));
                }
                body.push_str("</w:tr>");
            }
            body.push_str("</w:tbl>");
        }
        // 每 9 段掛一個註腳引用（錨點密度推高 recompute_notes 壓力）。
        let note = if i % 9 == 4 {
            let id = 1 + (i / 9) % 60;
            format!(
                r#"<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="{id}"/></w:r>"#
            )
        } else {
            String::new()
        };
        body.push_str(&format!(
            r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r>{note}</w:p>"#,
            para_text(i)
        ));
    }
    // 末節宣告（A4 直向）。
    body.push_str(
        r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>"#,
    );
    body.push_str("</w:body></w:document>");

    let mut zip = ZipTestHelper::new();
    zip.add("[Content_Types].xml", CT_XML.as_bytes());
    zip.add("_rels/.rels", ROOT_RELS.as_bytes());
    zip.add("word/_rels/document.xml.rels", DOC_RELS.as_bytes());
    zip.add("word/styles.xml", STYLES_XML.as_bytes());
    zip.add("word/footnotes.xml", footnotes_xml().as_bytes());
    zip.add("word/document.xml", body.as_bytes());
    zip.finish()
}

// ---------------------------------------------------------------------------
// 取樣腳本
// ---------------------------------------------------------------------------

/// 找一個「有文字的正文段落」（跳過表格格內段與空段）。
fn pick_para(engine: &Engine, want: f64) -> usize {
    let n = engine.paragraph_count();
    let mut i = (n as f64 * want) as usize;
    while i + 1 < n {
        if let Some(t) = engine.paragraph_text(i) {
            if t.chars().count() > 20 && !t.starts_with("表格格內文字") {
                return i;
            }
        }
        i += 1;
    }
    i
}

fn run_workload(label: &str, bytes: Vec<u8>, rounds: usize) -> (f64, f64) {
    // ---- 開檔（基準線外，只報數字不設閘門）----
    let t0 = Instant::now();
    let mut engine = match Engine::from_docx_bytes(&bytes) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("✗ {label} 開檔失敗：{err}");
            std::process::exit(1);
        }
    };
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let paras = engine.paragraph_count();
    let pages = engine.page_count();
    println!(
        "｜{label}：{} 段 / {} 頁｜開檔 {open_ms:.0}ms｜RSS {}MB",
        paras,
        pages,
        rss_mb()
    );

    let para_head = pick_para(&engine, 0.02);
    let para_mid = pick_para(&engine, 0.5);
    let para_tail = pick_para(&engine, 0.95).min(paras.saturating_sub(2));

    // ---- (e) 零編輯 save_docx() ----
    let mut t_save = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        let t = Instant::now();
        let saved = engine
            .save_docx()
            .unwrap_or_else(|e| panic!("{label} 零編輯存檔失敗：{e}"));
        t_save.push(t.elapsed().as_secs_f64() * 1000.0);
        // 零編輯存檔必須與原檔逐位元組相同（無損鐵律的即時哨，非計時對象）。
        assert_eq!(saved.len(), bytes.len(), "{label} 零編輯存檔位元組數偏移");
        assert!(
            saved == bytes,
            "{label} 零編輯存檔未 byte-identical（無損回退！）"
        );
    }

    // ---- (a) SetParagraphStyle（structural ⇒ rebuild_derived）----
    let mut t_style = Vec::with_capacity(rounds);
    for k in 0..rounds {
        let para = [para_head, para_mid, para_tail][k % 3];
        let patch = ParaPropsPatch {
            alignment: Some(String::from(if k % 2 == 0 { "center" } else { "left" })),
            ..Default::default()
        };
        let t = Instant::now();
        engine
            .apply_mutation(Mutation::SetParagraphStyle {
                target: SubDocTarget::Body,
                para_range: para..=para,
                patch,
            })
            .unwrap_or_else(|e| panic!("{label} SetParagraphStyle 失敗：{e}"));
        t_style.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    // ---- (b) undo()（每次必觸發 rebuild_derived）----
    let mut t_undo = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        let t = Instant::now();
        engine
            .undo()
            .unwrap_or_else(|e| panic!("{label} undo 失敗：{e}"));
        t_undo.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    // ---- (c) InsertText（增量打字路徑）----
    let mut t_insert = Vec::with_capacity(rounds);
    for k in 0..rounds {
        let para = [para_head, para_mid, para_tail][k % 3];
        let t = Instant::now();
        engine
            .apply_mutation(Mutation::InsertText {
                target: SubDocTarget::Body,
                pos: DocPos { para, cp: 5 },
                text: "字".to_string(),
                style_override: None,
            })
            .unwrap_or_else(|e| panic!("{label} InsertText 失敗：{e}"));
        t_insert.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    // ---- (d) FormatTextRange（structural ⇒ rebuild_derived）----
    let mut t_format = Vec::with_capacity(rounds);
    for k in 0..rounds {
        let para = [para_head, para_mid, para_tail][k % 3];
        let t = Instant::now();
        engine
            .apply_mutation(Mutation::FormatTextRange {
                range: DocRange {
                    target: SubDocTarget::Body,
                    start: DocPos { para, cp: 2 },
                    end: DocPos { para, cp: 12 },
                },
                patch: RunDirectPatch {
                    italic: Some(k % 2 == 0),
                    ..Default::default()
                },
            })
            .unwrap_or_else(|e| panic!("{label} FormatTextRange 失敗：{e}"));
        t_format.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    // ---- 收斂性自證：全部編輯 undo 乾淨後零編輯存檔必須回到 byte-identical ----
    while engine.can_undo() {
        engine.undo().expect("收尾 undo");
    }
    let saved = engine.save_docx().expect("收尾存檔");
    assert!(
        saved == bytes,
        "{label} 編輯→Undo 全淨後存檔未回到 byte-identical（K29 bundle 基準保真鐵律回退！）"
    );

    let (am, a50, a95) = stats(&t_style);
    let (bm, b50, b95) = stats(&t_undo);
    let (cm, c50, c95) = stats(&t_insert);
    let (dm, d50, d95) = stats(&t_format);
    let (em, e50, e95) = stats(&t_save);
    println!(
        "  (a) SetParagraphStyle ：mean {am:.2} p50 {a50:.2} p95 {a95:.2} ms（觸發 rebuild_derived）"
    );
    println!("  (b) undo()            ：mean {bm:.2} p50 {b50:.2} p95 {b95:.2} ms（觸發 rebuild_derived）");
    println!("  (c) InsertText        ：mean {cm:.2} p50 {c50:.2} p95 {c95:.2} ms（增量打字路徑）");
    println!("  (d) FormatTextRange   ：mean {dm:.2} p50 {d50:.2} p95 {d95:.2} ms（觸發 rebuild_derived）");
    println!("  (e) 零編輯 save_docx() ：mean {em:.2} p50 {e50:.2} p95 {e95:.2} ms");
    (pages as f64, a95)
}

fn main() {
    let rounds: usize = std::env::var("R1B_ROUNDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12);
    // workload 選擇（PM 驗收跑 both／預設 both；worker 分批時用 bug|synth 省時）。
    let which = std::env::var("R1B_WORKLOAD").unwrap_or_else(|_| "both".to_string());
    let buggy = std::env::var_os("R1B_BUGGY").is_some();
    println!(
        "R1-b rebuild_derived 效能基準｜profile=release(+debug info)｜rounds={rounds}｜模式＝{}",
        if buggy {
            "R1B_BUGGY=1（回塞：收斂空表起步＋打包全清快取）"
        } else {
            // 誠實標籤：rebuild_derived 目前仍整包重開（局部重解析未落地），
            // 加速來自 recompute_notes 種子化＋打包軸斷行快取保留。
            "正常態（種子化收斂＋打包快路徑；rebuild 仍整包）"
        }
    );

    // ---- workload 1：真實 180 頁公文檔（PM 親測 44s 級）----
    if which == "both" || which == "bug" {
        let bug_path = repo_rel("corpus/poi/test-data_document_bug65649.docx");
        let bug_bytes = match std::fs::read(&bug_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("✗ 讀 {bug_path}: {e}");
                std::process::exit(1);
            }
        };
        let (bug_pages, _) = run_workload("bug65649.docx（真實檔）", bug_bytes.clone(), rounds);
        assert!(bug_pages >= 150.0, "workload 頁數异常：{bug_pages}");
    }

    // ---- workload 2：合成 500 頁含表格＋註腳（指數擴充＋二分收斂，同 p6_bench 紀律）----
    if which == "both" || which == "synth" {
        let mut n = 4000usize;
        let mut bytes = synth_docx(n);
        loop {
            let e =
                Engine::from_docx_bytes(&bytes).unwrap_or_else(|e| panic!("合成檔開檔失敗：{e}"));
            if e.page_count() >= 500 {
                break;
            }
            n *= 2;
            if n > 200_000 {
                panic!("語料擴充失敗");
            }
            bytes = synth_docx(n);
        }
        let mut lo = n / 2;
        let mut hi = n;
        while lo + 50 < hi {
            let mid = (lo + hi) / 2;
            let e = Engine::from_docx_bytes(&synth_docx(mid)).expect("合成檔開檔失敗");
            if e.page_count() >= 500 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        let bytes = synth_docx(hi);
        println!(
            "｜合成 workload：{hi} 段（表格每 47 段一張／註腳每 9 段一錨）＝剛剛 ≥500 頁的最小規模"
        );
        let (synth_pages, _) = run_workload("合成 500 頁（表格＋註腳）", bytes, rounds);
        assert!(
            synth_pages >= 500.0,
            "合成 workload 未達 500 頁：{synth_pages}"
        );
    }

    println!(
        "★ 數字記帳：本卷只輸出 p50/p95 原始數字＋完整性斷言（零編輯 byte-identical／Undo 全淨 byte-identical）；門檻判定由 PM 依卡面（零編輯存檔<1.5s、InsertText p95<16ms、SetParagraphStyle／undo p95<50ms）對照 docs/r1b-perf-baseline.md 記帳。"
    );
}
