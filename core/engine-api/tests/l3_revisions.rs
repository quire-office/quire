use quire_engine::Engine;
use quire_render_trait::{DrawItem, FLAG_STRIKE, FLAG_UNDERLINE};

/// L3-revisions 紅燈（K5-diag §2.3 處方 (b)：默認視圖 del+ins 都發）：
/// RA001 的 `<w:del><w:r><w:delText>Chromatpgrams</w:delText></w:r></w:del>`
/// 必須出現在顯示列表且**帶刪除線**（LO 默認視圖照畫刪除線原文，實測 ×1；
/// 修前我方把 delText 整段吞掉 ×0——「同一 run 組內 delText 被 ins 吞」指紋）。
#[test]
fn ra001_deltext_emitted_with_strike() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/xopt/TestFiles_RA001-Tracked-Revisions-01.docx"
    ))
    .expect("讀 corpus RA001");
    let eng = Engine::from_docx_bytes(&bytes).expect("open");
    let pages = eng.page_count();
    let mut hits: Vec<(usize, u8)> = Vec::new();
    let mut chromatograms = 0usize;
    for p in 0..pages {
        let dl = eng.page_display_list(p).expect("頁存在");
        for it in &dl.items {
            if let DrawItem::Text { text, flags, .. } = it {
                if text.contains("Chromatpgrams") {
                    hits.push((p, *flags));
                }
                if text.contains("Chromatograms") {
                    chromatograms += 1;
                }
            }
        }
    }
    assert!(
        !hits.is_empty(),
        "delText 'Chromatpgrams' 未進顯示列表＝修訂刪除文字被吞（LO 默認視圖照畫）"
    );
    assert_eq!(hits.len(), 1, "'Chromatpgrams' 應恰發 1 次（源檔僅 1 處），實際 {hits:?}");
    let (_, flags) = hits[0];
    assert_ne!(flags & FLAG_STRIKE, 0, "刪除文字必須帶刪除線（FLAG_STRIKE），實際 flags={flags}");
    // ins 併入照發口徑不變（'Reference Chromatograms:' 源檔 3 處，LO 亦 3 次）。
    assert_eq!(chromatograms, 3, "'Chromatograms' 三處照發（含拆 run 段的 ins）");
}

/// L3-revisions：插入文字（`w:ins`／`w:moveTo` 內 `w:t`）默認視圖帶底線。
/// RA001 'Consider stage IV' 段整段在 `<w:ins>` 內（Word/LO 默認視圖插入加底線）。
#[test]
fn ra001_insertion_emitted_with_underline() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/xopt/TestFiles_RA001-Tracked-Revisions-01.docx"
    ))
    .expect("讀 corpus RA001");
    let eng = Engine::from_docx_bytes(&bytes).expect("open");
    let pages = eng.page_count();
    let mut found = None;
    for p in 0..pages {
        let dl = eng.page_display_list(p).expect("頁存在");
        for it in &dl.items {
            if let DrawItem::Text { text, flags, .. } = it {
                if text.contains("Consider stage IV") {
                    found = Some(*flags);
                }
            }
        }
    }
    let flags = found.expect("'Consider stage IV'（w:ins 插入段）必須在顯示列表");
    assert_ne!(flags & FLAG_UNDERLINE, 0, "插入文字默認視圖帶底線（FLAG_UNDERLINE），實際 flags={flags}");
}

/// 佔槽鐵律防呆：修訂分支只動 run 採集，不動 p 計數——RA001 段落數封頂
/// （231 段，K5-diag run 時基線；五台 p_count 一個不準碰）。
#[test]
fn ra001_paragraph_count_locked() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/xopt/TestFiles_RA001-Tracked-Revisions-01.docx"
    ))
    .expect("讀 corpus RA001");
    let doc = quire_ooxml::parse(&bytes).expect("parse");
    assert_eq!(doc.paragraphs.len(), 231, "RA001 正文段落數＝231（Start 口徑鐵律）");
}
