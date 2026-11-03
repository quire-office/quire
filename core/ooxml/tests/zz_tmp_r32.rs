use quire_ooxml::{parse, read_images};

#[test]
fn tmp_dump() {
    let full = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx"
    );
    let bytes = std::fs::read(full).unwrap();
    let doc = parse(&bytes).unwrap();
    let texts = doc.paragraph_texts();
    let xml = doc.parts.get("word/document.xml").unwrap();
    let imgs = read_images(xml);
    for im in &imgs {
        eprintln!(
            "IMG para={} w={} h={} posv={:?} wrap={:?} txbx={:?} group={} grow={:?}",
            im.para_index,
            im.w_pt,
            im.h_pt,
            im.pos_y_pt,
            im.wrap_type,
            im.txbx_texts,
            im.group_shapes.len(),
            im.textbox_grow
        );
    }
    for bt in &doc.tables {
        eprintln!("TABLE para_start={}", bt.para_start);
        for (ri, r) in bt.table.rows.iter().enumerate() {
            for (ci, c) in r.cells.iter().enumerate() {
                for b in &c.blocks {
                    if let quire_ooxml::Block::Paragraph { index } = b {
                        eprintln!("  r{ri} c{ci} -> P{index} {:?}", texts.get(*index));
                    }
                }
            }
        }
    }
}
