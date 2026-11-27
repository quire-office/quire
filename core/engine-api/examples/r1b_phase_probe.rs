//! 臨時：rebuild_derived 階段成本探針（本卡用，不進交付）。
use std::time::Instant;
use quire_engine::Engine;
use quire_ooxml::ZipBundle;

fn ms(t: &mut Instant) -> f64 {
    let v = t.elapsed().as_secs_f64() * 1000.0;
    *t = Instant::now();
    v
}

fn main() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/poi/test-data_document_bug65649.docx"
    );
    let bytes = std::fs::read(path).expect("read");
    let mut t = Instant::now();
    let bundle = ZipBundle::open(&bytes).expect("open");
    println!("zip open        : {:8.1} ms", ms(&mut t));
    let parts = bundle.decode_parts().expect("decode");
    println!("decode_parts    : {:8.1} ms", ms(&mut t));
    let doc = quire_ooxml::parse(&bytes).expect("parse");
    println!("full ooxml parse: {:8.1} ms", ms(&mut t));
    let engine = Engine::from_docx_bytes(&bytes).expect("engine");
    println!("engine full open: {:8.1} ms", ms(&mut t));
    println!("paragraphs={} pages={}", engine.paragraph_count(), engine.page_count());
    // parts 常駐量（=part_baselines 口徑：開檔時 parts==baselines）
    let mut sum = 0usize;
    for n in engine.part_names() {
        if let Some(b) = engine.part(&n) {
            sum += b.len();
        }
    }
    println!("parts 明文常駐總量: {:.1} MiB", sum as f64 / 1048576.0);
}
