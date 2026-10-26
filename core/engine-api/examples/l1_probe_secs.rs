// L1 探針：節表逐段診斷（交付前刪除）
use quire_ooxml::read_sections_per_paragraph;
fn main() {
    let a = std::env::args().nth(1).unwrap();
    let bytes = std::fs::read(&a).unwrap();
    let doc = quire_ooxml::parse(&bytes).expect("parse");
    let xml = doc.parts.get("word/document.xml").expect("doc.xml");
    let n = doc.paragraphs.len();
    let secs = read_sections_per_paragraph(xml, n);
    println!("paras={n} secs={}", secs.len());
    let mut prev: Option<(u32, bool, f32)> = None;
    for (i, s) in secs.iter().enumerate() {
        let cur = (s.id, s.geo.continuous, s.geo.page_w_pt);
        if prev != Some(cur) {
            println!("para{i}: id={} cont={} w={:.1}", s.id, s.geo.continuous, s.geo.page_w_pt);
            prev = Some(cur);
        }
    }
}
