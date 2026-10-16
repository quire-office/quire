//! corpus 個資消毒工具（開源公開發布用）。
//!
//! stdin 吃一個 JSON：{"in": path, "out": path, "parts": {"path": "hex新內容"}}
//! 未列出的 zip entry 由引擎的 byte-level 無損機制原樣搬運——本工具的正確性
//! 直接站在 roundtrip --check 閘門的肩膀上（dogfooding）。
use std::io::Read;

#[derive(serde::Deserialize)]
struct Job {
    r#in: String,
    out: String,
    parts: std::collections::BTreeMap<String, String>,
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(2) {
        return Err("hex 長度非偶數".into());
    }
    let nib = |c: u8| -> Result<u8, String> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(format!("非法 hex 字元: {}", c as char)),
        }
    };
    let mut out = Vec::with_capacity(b.len() / 2);
    for pair in b.chunks(2) {
        out.push(nib(pair[0])? << 4 | nib(pair[1])?);
    }
    Ok(out)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    let job: Job = serde_json::from_str(&buf)?;
    let bytes = std::fs::read(&job.r#in)?;
    let mut doc = quire_ooxml::parse(&bytes)?;
    for (name, hexv) in &job.parts {
        doc.set_part(name, hex_decode(hexv)?);
    }
    let out = doc.save()?;
    std::fs::write(&job.out, &out)?;
    eprintln!("scrubbed {} parts -> {}", job.parts.len(), job.out);
    Ok(())
}
