//! E1-B 臨時探針（report-only，收案前刪除）：印出指定 part（document／header／
//! footer）各框的 txbx_texts／逐段綁定事實／求值結果，對照 LO 實繪真值。

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let part = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "word/document.xml".to_string());
    let bytes = std::fs::read(&path).expect("read");
    let doc = quire_ooxml::parse(&bytes).expect("parse");
    let xml = doc.parts.get(&part).unwrap_or_else(|| panic!("no {part}"));
    let imgs = quire_ooxml::read_images(xml);
    println!("== {part}: {} boxes", imgs.len());
    for (i, im) in imgs.iter().enumerate() {
        if false && im.txbx_texts.is_empty() && im.txbx_sdt_bindings.is_empty() {
            continue;
        }
        println!(
            "## box {i} para={} w={:.1} h={:.1} float={} wps={} box_binding={:?}",
            im.para_index, im.w_pt, im.h_pt, im.floating, im.wps_annotation, im.txbx_databinding
        );
        for (ti, t) in im.txbx_texts.iter().enumerate() {
            match im
                .txbx_sdt_bindings
                .get(ti)
                .as_ref()
                .and_then(|o| o.as_ref())
            {
                Some(b) => println!(
                    "   p{ti} LIT={t:?} alias={:?} plc={} xpath={} => RESOLVED={:?}",
                    b.alias,
                    b.placeholder,
                    b.xpath,
                    quire_ooxml::resolve_binding(&doc.parts, b)
                ),
                None => println!("   p{ti} LIT={t:?} (no binding)"),
            }
        }
    }
}
