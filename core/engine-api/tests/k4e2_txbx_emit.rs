use quire_engine::Engine;

/// K4e2 紅燈：shapes-with-text 的文字框內容「A group of shapes」必須出現在
/// **同一頁**的顯示列表（正文流不再帶它——發射層要按 shape 佔位框畫回去）。
/// 修前：該文字由正文流段發射（但位置全錯、混進正文）；分離後未發射＝消失（紅）。
#[test]
fn shapes_with_text_txbx_text_emitted_at_shape() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/poi/test-data_document_shapes-with-text.docx"
    ))
    .expect("讀 corpus shapes-with-text");
    let eng = Engine::from_docx_bytes(&bytes).expect("open");
    let page = eng.page_count();
    let mut found: Vec<(usize, String)> = Vec::new();
    for p in 0..page {
        let dl = eng.page_display_list(p).expect("頁存在");
        for it in &dl.items {
            if let quire_render_trait::DrawItem::Text { text, x, y, .. } = it {
                if text.contains("A group of shapes") {
                    found.push((p, format!("({x:.1},{y:.1}) {text}")));
                }
            }
        }
    }
    assert!(
        !found.is_empty(),
        "框內文字「A group of shapes」既不在正文流也未按 shape 發射＝文字丟失"
    );
}
