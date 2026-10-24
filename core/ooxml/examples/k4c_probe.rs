use quire_ooxml::{parse, read_images};

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let doc = parse(&bytes).unwrap();
    let xml = doc.parts.get("word/document.xml").unwrap();
    println!("--- read_images ---");
    for im in read_images(xml) {
        println!(
            "para={} w={:.2} h={:.2} float={} cell={} txbx={:?} fill={:?} filled={:?} stroke={:?} stroked={:?} sw={:?} dash={:?} rect={:?} inset={:?} vert={} tbl={:?}",
            im.para_index,
            im.w_pt,
            im.h_pt,
            im.floating,
            im.in_cell,
            im.txbx_texts,
            im.fill_argb.map(|v| format!("{v:#010X}")),
            im.filled,
            im.stroke_argb.map(|v| format!("{v:#010X}")),
            im.stroked,
            im.stroke_width_pt,
            im.stroke_dash,
            im.txbx_rect,
            im.txbx_inset,
            im.txbx_vertical,
            im.txbx_table,
        );
    }
}
