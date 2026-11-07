// E1-B 臨時探針（收案前刪除）：跑「求值覆寫」預演，印出每檔每段覆寫前後對照。
// 目的：在動發射層之前，先用 LO 實測真值（scratch_d9b/e1b_lo_truth_14.txt）逐檔核對
// 覆寫極性，並抓出「LO 保留佔位但我方會覆寫」的反例。report-only。
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    while let Some(path) = args.next() {
        let part = args
            .next()
            .unwrap_or_else(|| "word/document.xml".to_string());
        let bytes = std::fs::read(&path).expect("read");
        let doc = quire_ooxml::parse(&bytes).expect("parse");
        let xml = match doc.parts.get(&part) {
            Some(x) => x.clone(),
            None => {
                println!("## {path} :: 無 {part}，跳過");
                continue;
            }
        };
        let imgs = quire_ooxml::read_images(&xml);
        for (bi, im) in imgs.iter().enumerate() {
            for (ti, t) in im.txbx_texts.iter().enumerate() {
                let b = match im
                    .txbx_sdt_bindings
                    .get(ti)
                    .as_ref()
                    .and_then(|o| o.as_ref())
                {
                    Some(b) => b,
                    None => continue,
                };
                let resolved = quire_ooxml::resolve_binding(&doc.parts, b);
                let act = if t.trim().is_empty() {
                    "跳過（空段）"
                } else {
                    match &resolved {
                        Some(v) if v.trim() != t.trim() => "覆寫",
                        Some(_) => "同值（零變化）",
                        None => "保留佔位",
                    }
                };
                println!(
                    "{path} [{part}] box{bi} p{ti} LIT={t:?} plc={} => {resolved:?} 【{act}】",
                    b.placeholder
                );
            }
        }
        let _: &BTreeMap<String, Vec<u8>> = &doc.parts;
    }
}
