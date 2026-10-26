// L1 探針：逐頁段落分佈＋多欄 section 歸屬（交付前刪除）
use quire_engine::Engine;
fn main() {
    let a = std::env::args().nth(1).unwrap();
    let bytes = std::fs::read(&a).unwrap();
    let e = Engine::from_docx_bytes(&bytes).expect("open");
    println!("pages={}", e.page_count());
    for p in 0..e.page_count() {
        let json = e.page_display_list_json(p).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let items = v["items"].as_array().unwrap();
        let texts: Vec<&str> = items
            .iter()
            .filter_map(|it| {
                it.get("Text")
                    .and_then(|t| t.get("text"))
                    .and_then(|s| s.as_str())
            })
            .collect();
        let xs: Vec<f64> = items
            .iter()
            .filter_map(|it| it.get("Text")?.get("x")?.as_f64())
            .collect();
        let minx = xs.iter().cloned().fold(f64::INFINITY, f64::min);
        let maxx = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        println!(
            "page {p}: {} texts, x∈[{minx:.1},{maxx:.1}], first={:?} last={:?}",
            texts.len(),
            texts.first().unwrap_or(&"").chars().take(24).collect::<String>(),
            texts.last().unwrap_or(&"").chars().take(24).collect::<String>(),
        );
    }
}
