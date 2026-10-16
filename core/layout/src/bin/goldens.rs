//! shaping/pagination golden 快照產生器／驗證器（Phase0-4/0-5 閘門）。
//!
//!   cargo run -p quire-layout --bin goldens            # 比對，差異即非零退出
//!   UPDATE_GOLDENS=1 cargo run -p quire-layout --bin goldens   # 重新生成
//!
//! 快照格式（每行 JSON）：{"id":..., "input":..., "max_w_pt":..., "lines":[{"text":..., "width_pt":...}]}
//! 快照捕捉的就是 **shaped advances 斷點**：斷點位移、字寬改變都會咬golden。
//! pagination.jsonl（Phase0-5）：{"id":..., "params":{...}, "breaks":[[page,first_para,
//! last_para,line_count,continues_paragraph],...]}——分頁斷點對段落歸屬的快照。
use std::path::Path;

use quire_layout::{break_lines, page_breaks, LayoutParams, Line, TextMeasurer};

const SIZE_PT: f32 = 12.0;

fn cases() -> Vec<(&'static str, &'static str, f32)> {
    // 中英混排斷點是核心；其餘為回歸面（標點/連字/缺字/RTL/全形）。
    vec![
        ("mixed_basic", "Quire 是一個開源的中文文書處理器，目標是取代 Word。", 200.0),
        ("mixed_narrow", "The 快速棕狐 jumps over the 懶惰的狗 while 測試 mixed CJK。", 120.0),
        ("mixed_wide", "在 Rust 的 ecosystem 裡，parley 與 swash 負責 text shaping；斷行演算法採用 Knuth-Plass，再配上 UAX#14 的 CJK/Latin 混排斷詞規則。", 451.0),
        ("cjk_punctuation", "他說：「這個方案——真的嗎？」我回答：「千真萬確！」", 140.0),
        ("latin_words", "Supercalifragilisticexpialidocious and simple words mixed together in a narrow box", 100.0),
        ("fullwidth_latin", "全形ＡＢＣ與半形ABC混排test", 90.0),
        ("ligature_fi", "Office efficient affiliation — finally fine stuff.", 150.0),
        ("numbers_units", "預算 3,000,000 元（約 USD 95,000.50）於 2026-10-14 核定。", 160.0),
        ("long_token", "超長無空白Tokenabcdefghij中文混排繼續的很長的一個字串結束", 80.0),
        ("cyrillic", "Привет мир 混排 тест", 110.0),
        ("empty", "", 200.0),
        ("single_char", "中", 200.0),
    ]
}

fn pagination_cases() -> Vec<(&'static str, Vec<String>, LayoutParams)> {
    // 分頁斷點：段落跨頁歸屬（KP 斷行使行數/續排點敏感於斷行演算法）。
    let p = LayoutParams::default();
    let long: Vec<String> = (0..25)
        .map(|i| format!("第 {i} 段：{}", "內文測試文字。".repeat(40)))
        .collect();
    let mixed: Vec<String> = std::iter::once("超長段落：".to_string() + &"字".repeat(3000))
        .chain((0..12).map(|i| format!("短段 {i}：中英 mixed 標點，！？「引號」")))
        .chain(std::iter::once(String::new()))
        .chain((0..6).map(|i| format!("尾段 {i}")))
        .collect();
    vec![
        ("uniform25", long, p),
        ("mixed_overflow", mixed, p),
        (
            "narrow_page",
            (0..15)
                .map(|i| format!("窄頁段落 {i}：{}", "文".repeat(300)))
                .collect(),
            LayoutParams {
                page_w_pt: 300.0,
                page_h_pt: 500.0,
                margins_pt: [36.0; 4],
                ..LayoutParams::default()
            },
        ),
        ("single_empty", vec![String::new()], p),
    ]
}

fn snap_break(id: &str, p: &LayoutParams, brks: &[quire_layout::PageBreak]) -> String {
    // 快照只收 uniform 邊界的文件（本黃金集全部四邊等寬）；margins_pt[0] 即其單一值。
    let mut s = format!(
        "{{\"id\":{},\"params\":{{\"page_w_pt\":{:.1},\"page_h_pt\":{:.1},\"margin_pt\":{:.1},\"font_size_pt\":{:.1},\"line_spacing\":{:.2}}},\"breaks\":[",
        json_str(id),
        p.page_w_pt,
        p.page_h_pt,
        p.margins_pt[0],
        p.font_size_pt,
        p.line_spacing
    );
    for (i, b) in brks.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "[{},{},{},{},{}]",
            b.page, b.first_para, b.last_para, b.line_count, b.continues_paragraph
        ));
    }
    s.push_str("]}");
    s
}

fn snap_line(id: &str, input: &str, max_w: f32, lines: &[Line]) -> String {
    let mut s = format!(
        "{{\"id\":{},\"input\":{},\"max_w_pt\":{:.3},\"lines\":[",
        json_str(id),
        json_str(input),
        max_w
    );
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"text\":{},\"width_pt\":{:.3}}}",
            json_str(&l.text),
            l.width_pt
        ));
    }
    s.push_str("]}");
    s
}

/// 不引依賴的 JSON 字串編碼（golden 需穩定、可 diff）。
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens");
    let m = TextMeasurer::new(SIZE_PT);
    let shaping: Vec<String> = cases()
        .into_iter()
        .map(|(id, input, max_w)| snap_line(id, input, max_w, &break_lines(input, max_w, &m)))
        .collect();
    let pagination: Vec<String> = pagination_cases()
        .into_iter()
        .map(|(id, texts, p)| snap_break(id, &p, &page_breaks(&texts, &p)))
        .collect();

    let update = std::env::var("UPDATE_GOLDENS").is_ok_and(|v| v != "0");
    let mut fails = 0usize;
    let snaps: [(&str, &[String]); 2] = [
        ("shaping.jsonl", &shaping),
        ("pagination.jsonl", &pagination),
    ];
    for (name, snapshot) in snaps {
        let path = root.join(name);
        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, format!("{}\n", snapshot.join("\n"))).unwrap();
            println!("✓ {name} 已更新（{} 例）", snapshot.len());
            continue;
        }
        let want = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("讀不到 {name}（先 UPDATE_GOLDENS=1 生成）：{e}"));
        for (got, want_line) in snapshot.iter().zip(want.lines()) {
            if got != want_line {
                fails += 1;
                eprintln!("✗ {name} 不符：\n  got  {got}\n  want {want_line}");
            }
        }
        if snapshot.len() != want.lines().count() {
            fails += 1;
            eprintln!(
                "✗ {name} 案例數不符：got {} want {}",
                snapshot.len(),
                want.lines().count()
            );
        }
        println!("✓ {name} 相符（{} 例）", snapshot.len());
    }
    if fails > 0 {
        panic!(
            "{fails} 個 golden 不符——若是刻意的演算法變更，UPDATE_GOLDENS=1 重生成並逐條審 diff"
        );
    }
}
