// ODF-E2 解剖探針：逐段輸出我方 ODT 文字流投影（text + 字級 + run 數），
// 供 python 側與 LO PDF 實際發射逐行對照。只讀公開 API，不改 src。
use quire_odf::{OdfBundle, project};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: odtflow <file.odt>");
        std::process::exit(2);
    }
    let bytes = match std::fs::read(&args[1]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("讀檔失敗: {e}");
            std::process::exit(1);
        }
    };
    let bundle = match OdfBundle::open(&bytes) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("開啟失敗: {e}");
            std::process::exit(1);
        }
    };
    let p = match project(&bundle) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("投影失敗: {e}");
            std::process::exit(1);
        }
    };
    let paras: Vec<serde_json::Value> = p
        .paragraphs
        .iter()
        .enumerate()
        .map(|(i, para)| {
            serde_json::json!({
                "i": i,
                "text": para.text,
                "len": para.text.chars().count(),
                "runs": para.runs.len(),
                "size": para.runs.first().map(|r| r.1).unwrap_or(0.0),
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({
            "paragraph_count": p.paragraph_count(),
            "total_text_len": p.total_text_len,
            "text_p_count": p.text_p_count,
            "text_h_count": p.text_h_count,
            "paragraphs": paras,
        })
    );
}
