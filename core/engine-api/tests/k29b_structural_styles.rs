//! K29b 紅燈測試 1–3：段落拆併索引平移、格式／清單最小 part 觸碰、跨容器與型別守衛。
//!
//! 依據任務書 OC-K29B 與 docs/edit-api-design.md §6.3 Slice 2：
//! - 紅燈 1：SplitParagraph／MergeParagraphWithNext 後行內圖仍按真像素發射。
//! - 紅燈 2：FormatTextRange／SetParagraphStyle／SetListNumbering 僅碰 document.xml；
//!   UpsertNamedStyle 僅碰 styles.xml。
//! - 紅燈 3：跨容器拆併與字元樣式誤用段落樣式 API 必須誠實拒絕，且失敗後存檔零改動。

use quire_engine::{
    diff_untouched, DocPos, DocRange, DrawItem, Engine, ListAction, Mutation, MutationError,
    ParaPropsPatch, RunDirectPatch, StyleType, SubDocTarget, FLAG_BOLD,
};

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

fn load_corpus_bytes(rel: &str) -> Vec<u8> {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 corpus 失敗 {full}: {e}"))
}

fn has_real_png_pixel(engine: &Engine, page: usize) -> bool {
    engine
        .page_display_list(page)
        .map(|dl| {
            dl.items.iter().any(|it| match it {
                DrawItem::Image { data_b64, .. } => data_b64.starts_with("iVBORw0KGgo"),
                _ => false,
            })
        })
        .unwrap_or(false)
}

/// 紅燈 1：拆併段跨解析器索引對齊鎖。
#[test]
fn k29b_lamp1_split_merge_keeps_inline_image_pixels() {
    const FX: &[u8] = include_bytes!("../../../fixtures/inline_image.docx");
    let mut engine = Engine::from_docx_bytes(FX).expect("開啟 fixtures/inline_image.docx 失敗");
    settle(&mut engine);

    let original_count = engine.paragraph_count();
    assert_eq!(original_count, 3, "inline_image.docx 應有三個扁平段落");
    assert!(
        has_real_png_pixel(&engine, 0),
        "開檔後第 0 頁必須已發射行內 PNG 真像素"
    );

    engine
        .apply_mutation(Mutation::SplitParagraph {
            target: SubDocTarget::Body,
            pos: DocPos { para: 0, cp: 5 },
        })
        .expect("套用 M1.3 SplitParagraph 失敗");
    settle(&mut engine);

    assert_eq!(
        engine.paragraph_count(),
        original_count + 1,
        "拆段後扁平段落數必須 +1"
    );
    assert!(
        has_real_png_pixel(&engine, 0),
        "拆段後原圖段平移，image_pixels 宿主索引必須同步 +1，不得退灰框"
    );

    // Undo 亦須回到拆段前狀態（索引平移可逆），Redo 再回到拆段後。
    engine.undo().expect("undo 拆段失敗");
    settle(&mut engine);
    assert_eq!(
        engine.paragraph_count(),
        original_count,
        "undo 拆段後段落數必須復原"
    );
    assert!(
        has_real_png_pixel(&engine, 0),
        "undo 拆段後行內圖仍必須為真像素"
    );
    engine.redo().expect("redo 拆段失敗");
    settle(&mut engine);
    assert_eq!(
        engine.paragraph_count(),
        original_count + 1,
        "redo 拆段後段落數必須回到 +1"
    );

    // 存檔重開後仍必須是真像素（索引平移不只存在記憶體態）。
    let saved = engine.save_docx().expect("拆段後存檔失敗");
    let mut reopened = Engine::from_docx_bytes(&saved).expect("拆段後重開失敗");
    settle(&mut reopened);
    assert_eq!(
        reopened.paragraph_count(),
        original_count + 1,
        "重開後段落數必須保留 +1"
    );
    assert!(
        has_real_png_pixel(&reopened, 0),
        "重開後行內圖仍必須為真像素 DrawItem::Image"
    );

    // 併回前段：段落數與圖片宿主位置完全復原。
    reopened
        .apply_mutation(Mutation::MergeParagraphWithNext {
            target: SubDocTarget::Body,
            para: 0,
        })
        .expect("套用 M1.4 MergeParagraphWithNext 失敗");
    settle(&mut reopened);
    assert_eq!(
        reopened.paragraph_count(),
        original_count,
        "併段後段落數必須復原"
    );
    assert!(
        has_real_png_pixel(&reopened, 0),
        "併段後行內圖仍必須為真像素"
    );
}

/// 紅燈 2：格式與清單編號最小 part 觸碰鎖。
#[test]
fn k29b_lamp2_format_and_numbering_touch_only_document_part() {
    let bytes = load_corpus_bytes("phpword/tests_PhpWordTests__files_documents_reader-styles.docx");
    let mut engine = Engine::from_docx_bytes(&bytes).expect("開啟 reader-styles.docx 失敗");
    settle(&mut engine);

    engine
        .apply_mutation(Mutation::FormatTextRange {
            range: DocRange {
                target: SubDocTarget::Body,
                start: DocPos { para: 0, cp: 0 },
                end: DocPos { para: 0, cp: 5 },
            },
            patch: RunDirectPatch {
                bold: Some(true),
                color_rgb: Some(0xFFFF_0000),
                ..Default::default()
            },
        })
        .expect("套用 M2.1 FormatTextRange 失敗");

    engine
        .apply_mutation(Mutation::SetParagraphStyle {
            target: SubDocTarget::Body,
            para_range: 0..=0,
            patch: ParaPropsPatch {
                page_break_before: Some(true),
                ..Default::default()
            },
        })
        .expect("套用 M2.2 SetParagraphStyle 失敗");

    engine
        .apply_mutation(Mutation::SetListNumbering {
            target: SubDocTarget::Body,
            para_range: 0..=0,
            action: ListAction::AttachExisting { num_id: 1, ilvl: 0 },
        })
        .expect("套用 M3.1 SetListNumbering 失敗");

    let diff = engine.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip_check 必須通過: {:?}", diff.violations);
    assert_eq!(
        diff.whitelisted_changed,
        vec!["word/document.xml".to_string()],
        "格式與清單操作只准觸碰 word/document.xml（styles.xml／numbering.xml 零觸碰）"
    );

    let saved = engine.save_docx().expect("存檔失敗");
    let mut reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
    settle(&mut reopened);

    let dl = reopened.page_display_list(0).expect("第 0 頁必須存在");
    let bold_red = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                text,
                flags,
                color_argb,
                y,
                ..
            } if text.contains("PHP") => Some((*flags, *color_argb, *y)),
            _ => None,
        })
        .expect("重開後第 0 頁必須有含 PHP 的文字項");
    assert_ne!(bold_red.0 & FLAG_BOLD, 0, "前 5 字必須帶 FLAG_BOLD");
    assert_eq!(bold_red.1, 0xFFFF_0000, "前 5 字顏色必須為 0xFFFF0000");

    let content_box = reopened.page_content_box(0).expect("第 0 頁版心盒");
    assert!(
        (bold_red.2 - content_box.y_pt).abs() < 0.5,
        "page_break_before 後該段必須自新頁 margin_top_pt 起頭（y={} vs margin_top={}）",
        bold_red.2,
        content_box.y_pt
    );
}

/// 紅燈 2 續：UpsertNamedStyle 僅觸碰 styles.xml，document.xml 零觸碰。
#[test]
fn k29b_lamp2_upsert_named_style_touches_only_styles_part() {
    let bytes = load_corpus_bytes("ooxmlexport/style-inheritance.docx");
    let mut engine = Engine::from_docx_bytes(&bytes).expect("開啟 style-inheritance.docx 失敗");
    settle(&mut engine);

    engine
        .apply_mutation(Mutation::UpsertNamedStyle {
            style_id: "Hyperlink".to_string(),
            style_type: StyleType::Character,
            based_on: None,
            para_patch: None,
            run_patch: Some(RunDirectPatch {
                color_rgb: Some(0xFF00_00FF),
                ..Default::default()
            }),
        })
        .expect("套用 M2.3 UpsertNamedStyle 失敗");

    let diff = engine.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip_check 必須通過: {:?}", diff.violations);
    assert_eq!(
        diff.whitelisted_changed,
        vec!["word/styles.xml".to_string()],
        "UpsertNamedStyle 只准觸碰 word/styles.xml（document.xml 零觸碰）"
    );
}

/// 紅燈 3：非法跨容器與樣式錯配誠實拒絕鎖。
#[test]
fn k29b_lamp3_cross_container_and_style_type_rejected_byte_identical() {
    const FX: &[u8] = include_bytes!("../../../fixtures/table_mixed.docx");
    let mut engine = Engine::from_docx_bytes(FX).expect("開啟 fixtures/table_mixed.docx 失敗");
    settle(&mut engine);

    let merge_err = engine
        .apply_mutation(Mutation::MergeParagraphWithNext {
            target: SubDocTarget::Body,
            para: 0,
        })
        .expect_err("表前段與格內段併段必須被拒絕");
    assert!(
        matches!(merge_err, MutationError::CrossContainerBoundary { .. }),
        "併段跨容器必須回 CrossContainerBoundary，實際: {merge_err:?}"
    );

    let delete_err = engine
        .apply_mutation(Mutation::DeleteRange {
            range: DocRange {
                target: SubDocTarget::Body,
                start: DocPos { para: 0, cp: 0 },
                end: DocPos { para: 1, cp: 0 },
            },
        })
        .expect_err("表前段到格內段的跨段刪除必須被拒絕");
    assert!(
        matches!(delete_err, MutationError::CrossContainerBoundary { .. }),
        "跨段刪除跨容器必須回 CrossContainerBoundary，實際: {delete_err:?}"
    );

    let style_err = engine
        .apply_mutation(Mutation::SetParagraphStyle {
            target: SubDocTarget::Body,
            para_range: 0..=0,
            patch: ParaPropsPatch {
                style_id: Some("Hyperlink".to_string()),
                ..Default::default()
            },
        })
        .expect_err("字元樣式 id 傳入段落樣式 API 必須被拒絕");
    assert!(
        matches!(style_err, MutationError::InvalidStyleReference { .. }),
        "樣式型別不符必須回 InvalidStyleReference，實際: {style_err:?}"
    );

    let saved = engine.save_docx().expect("失敗後存檔失敗");
    let diff = diff_untouched(FX, &saved).expect("diff_untouched 失敗");
    assert!(
        diff.byte_identical,
        "所有操作失敗後 save_docx() 必須與原檔 byte_identical"
    );
}
