// 除錯用：跑官方 UAX#14 LineBreakTest.txt 一致性測試（暫時探针，驗完刪）。
use std::collections::HashMap;
use unicode_linebreak::{linebreaks, BreakOpportunity};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: lb_conformance <LineBreakTest.txt>");
    let src = std::fs::read_to_string(&path).unwrap();
    let mut total = 0usize;
    let mut fails = 0usize;
    let mut first: Vec<String> = Vec::new();
    for line in src.lines() {
        let Some((body, _)) = line.split_once('#') else {
            continue;
        };
        let body = body.trim();
        if body.is_empty() {
            continue;
        }
        // 解析 "0020 × 0041 ÷ ..."：×=不斷、÷=可斷（含 ×÷ 組合）。
        let mut cps: Vec<u32> = Vec::new();
        let mut expected: Vec<bool> = Vec::new(); // 每個 cp 之前是否可斷（首 cp 恆 false）
        let mut expect_break_next = false;
        for tok in body.split_whitespace() {
            match tok {
                "÷" => expect_break_next = true,
                "×" => expect_break_next = false,
                "÷×" => expect_break_next = true, // 成對標記：同點可斷
                "×÷" => expect_break_next = true,
                hex => {
                    let Ok(cp) = u32::from_str_radix(hex, 16) else {
                        continue;
                    };
                    cps.push(cp);
                    expected.push(expect_break_next);
                    expect_break_next = false;
                }
            }
        }
        if cps.is_empty() {
            continue;
        }
        let text: String = cps.iter().copied().filter_map(char::from_u32).collect();
        if text.chars().count() != cps.len() {
            continue; // 代理對等：本探针跳過
        }
        // 位元組位置 → 字元索引
        let mut byte_to_char: HashMap<usize, usize> = HashMap::new();
        for (ci, (b, _)) in text.char_indices().enumerate() {
            byte_to_char.insert(b, ci);
        }
        let mut got: Vec<usize> = Vec::new();
        for (p, o) in linebreaks(&text) {
            if matches!(o, BreakOpportunity::Allowed) && p < text.len() {
                got.push(byte_to_char[&p]);
            }
        }
        let want: Vec<usize> = (1..cps.len()).filter(|&k| expected[k]).collect();
        total += 1;
        if got != want {
            fails += 1;
            if first.len() < 12 {
                first.push(format!(
                    "{body}\n  want={want:?} got={got:?}\n  text={text:?}"
                ));
            }
        }
    }
    println!("cases={total} fails={fails}");
    for f in first {
        println!("----\n{f}");
    }
}
