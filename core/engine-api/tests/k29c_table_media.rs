//! K29c 紅燈測試：M4 表格家族與 M5 圖片家族。
//!
//! 依據任務書 OC-K29C 與 docs/edit-api-design.md §6.4／§5.3.2：
//! - 紅燈 1：表格增刪列欄、合併與跨頁表頭重排鎖，段落索引四套同步平移。
//! - 紅燈 2：圖片縮放移動零媒體觸碰，且顯示列表配對鍵 `(para, w_pt)` 當下同步。
//! - 紅燈 3：新圖雜湊去重、CoW Undo 零失真、強制 GC 不可逆鎖。
//!
//! 本檔只依賴 engine 公開面（不 import 內部模組），fixture 由 repo 根相對讀取。

use quire_engine::{
    DocPos, DrawItem, Engine, InsertSide, Mutation, ReversibilityLevel, SubDocTarget, TablePath,
};

/// 一張與 fixture 既有 `word/media/image1.png` 不同的合法 1×1 PNG（紅色像素）。
const NEW_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xf7, 0x03, 0x41, 0x43, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

fn load_bytes(rel: &str) -> Vec<u8> {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 {full} 失敗: {e}"))
}

fn open(rel: &str) -> Engine {
    Engine::from_docx_bytes(&load_bytes(rel)).unwrap_or_else(|e| panic!("開啟 {rel} 失敗: {e}"))
}

/// 該頁真 PNG 像素的 `DrawItem::Image`（寬, 高）清單。
fn real_pngs(engine: &Engine, page: usize) -> Vec<(f32, f32)> {
    engine
        .page_display_list(page)
        .map(|dl| {
            dl.items
                .iter()
                .filter_map(|it| match it {
                    DrawItem::Image { w, h, data_b64, .. }
                        if data_b64.starts_with("iVBORw0KGgo") =>
                    {
                        Some((*w, *h))
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 該頁是否出現灰底佔位框 `0xFFF0F0F0`（真像素退化即命中）。
fn has_gray_placeholder(engine: &Engine, page: usize) -> bool {
    engine
        .page_display_list(page)
        .map(|dl| {
            dl.items.iter().any(
                |it| matches!(it, DrawItem::Rect { fill_argb, .. } if *fill_argb == 0xFFF0_F0F0),
            )
        })
        .unwrap_or(false)
}

fn find_para(engine: &Engine, needle: &str) -> Option<usize> {
    (0..engine.paragraph_count()).find(|&i| {
        engine
            .paragraph_text(i)
            .as_deref()
            .is_some_and(|t| t.contains(needle))
    })
}

fn pages_with(engine: &Engine, needle: &str) -> Vec<usize> {
    let mut pages = Vec::new();
    for p in 0..engine.page_count() {
        let hit = engine
            .page_display_list(p)
            .map(|dl| {
                dl.items.iter().any(|it| match it {
                    DrawItem::Text { text, .. } => text.contains(needle),
                    _ => false,
                })
            })
            .unwrap_or(false);
        if hit {
            pages.push(p);
        }
    }
    pages
}

fn root_table(engine: &Engine) -> serde_json::Value {
    let v: serde_json::Value =
        serde_json::from_str(&engine.tables_json()).expect("tables_json 不是合法 JSON");
    v.get(0).cloned().expect("必須有第一張表")
}

fn grid_len(table: &serde_json::Value) -> usize {
    table["table"]["grid"].as_array().unwrap().len()
}

fn rows_len(table: &serde_json::Value) -> usize {
    table["table"]["rows"].as_array().unwrap().len()
}

// ---------------------------------------------------------------------------
// 紅燈 1：表格結構與段落索引同步
// ---------------------------------------------------------------------------

/// 插入列後：格內空段增、表後段落索引平移、`para_sec` 同步、存檔重開一致。
#[test]
fn k29c_lamp1_insert_row_keeps_paragraph_index_in_sync() {
    let mut e = open("fixtures/table_mixed.docx");
    settle(&mut e);
    let base = e.paragraph_count();
    assert_eq!(base, 13, "table_mixed.docx 扁平段落數基準");
    assert_eq!(find_para(&e, "表格後段落"), Some(12), "表格後段落基準索引");
    assert_eq!(e.para_sections().len(), base, "para_sec 與段落數同長");

    e.apply_mutation(Mutation::InsertTableRow {
        table: TablePath::body(0),
        at_row: 1,
        position: InsertSide::After,
    })
    .expect("M4.3 InsertTableRow 失敗");
    settle(&mut e);

    assert_eq!(e.paragraph_count(), base + 3, "插入 3 格列 → 段落數 +3");
    assert_eq!(
        e.para_sections().len(),
        e.paragraph_count(),
        "Δp≠0 後 para_sec 必須同步平移"
    );
    assert_eq!(
        find_para(&e, "表格後段落"),
        Some(15),
        "表後段落索引必須 +3 不錯位"
    );
    let t = root_table(&e);
    assert_eq!(rows_len(&t), 5, "根表列數 4→5");
    assert_eq!(grid_len(&t), 3, "插入列不得動網格欄數");
    assert_eq!(t["para_start"].as_u64(), Some(1), "表格起點段落索引不變");

    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    assert_eq!(
        diff.whitelisted_changed,
        vec!["word/document.xml".to_string()],
        "插入列只准觸碰 word/document.xml"
    );

    let saved = e.save_docx().expect("存檔失敗");
    let mut r = Engine::from_docx_bytes(&saved).expect("重開失敗");
    settle(&mut r);
    assert_eq!(r.paragraph_count(), base + 3, "重開後段落數保留");
    assert_eq!(
        find_para(&r, "表格後段落"),
        Some(15),
        "重開後表後段落索引仍正確"
    );
    assert_eq!(rows_len(&root_table(&r)), 5, "重開後列數保留");
}

/// 刪除網格欄：`gridSpan` 降階、`vMerge restart` 保留、段落數同步減。
#[test]
fn k29c_lamp1_delete_column_updates_gridspan_and_preserves_vmerge() {
    let mut e = open("fixtures/table_mixed.docx");
    settle(&mut e);
    let base = e.paragraph_count();

    e.apply_mutation(Mutation::DeleteTableColumn {
        table: TablePath::body(0),
        grid_col: 2,
    })
    .expect("M4.4 DeleteTableColumn 失敗");
    settle(&mut e);

    assert_eq!(e.paragraph_count(), base - 1, "刪除單格欄 → 段落數 -1");
    assert_eq!(
        e.para_sections().len(),
        e.paragraph_count(),
        "para_sec 同步"
    );
    assert!(
        find_para(&e, "第二季").is_none(),
        "被刪欄的格內段落必須消失"
    );
    assert_eq!(find_para(&e, "表格後段落"), Some(11), "表後段落索引 -1");

    let t = root_table(&e);
    assert_eq!(grid_len(&t), 2, "網格 3→2 欄");
    assert_eq!(
        t["table"]["rows"][0]["cells"][0]["props"]["grid_span_raw"].as_str(),
        Some("2"),
        "跨三欄表頭格 gridSpan 3→2"
    );
    assert_eq!(
        t["table"]["rows"][2]["cells"][0]["props"]["vmerge"].as_str(),
        Some("restart"),
        "vMerge restart 必須保留"
    );

    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    assert_eq!(
        diff.whitelisted_changed,
        vec!["word/document.xml".to_string()]
    );

    let saved = e.save_docx().expect("存檔失敗");
    let mut r = Engine::from_docx_bytes(&saved).expect("重開失敗");
    settle(&mut r);
    assert_eq!(grid_len(&root_table(&r)), 2, "重開後網格欄數保留");
    assert_eq!(
        root_table(&r)["table"]["rows"][0]["cells"][0]["props"]["grid_span_raw"].as_str(),
        Some("2"),
        "重開後 gridSpan 保留"
    );
}

/// 合併列 1 的兩個橫向格：`gridSpan` 反映、內容不遺失。
#[test]
fn k29c_lamp1_merge_cells_sets_gridspan_without_losing_paragraphs() {
    let mut e = open("fixtures/table_mixed.docx");
    settle(&mut e);
    let base = e.paragraph_count();

    e.apply_mutation(Mutation::MergeTableCells {
        table: TablePath::body(0),
        top_row: 1,
        bottom_row: 1,
        left_grid_col: 1,
        right_grid_col: 2,
    })
    .expect("M4.5 MergeTableCells 失敗");
    settle(&mut e);

    assert_eq!(
        e.paragraph_count(),
        base,
        "橫向合併保留兩格內容段落，段落總數不變"
    );
    let t = root_table(&e);
    assert_eq!(
        t["table"]["rows"][1]["cells"].as_array().unwrap().len(),
        2,
        "列 1 由 3 格併成 2 格"
    );
    assert_eq!(
        t["table"]["rows"][1]["cells"][1]["props"]["grid_span_raw"].as_str(),
        Some("2"),
        "合併後第二格（起點 第一季）gridSpan=2"
    );
    assert!(find_para(&e, "第一季").is_some(), "第一季內容保留");
    assert!(find_para(&e, "第二季").is_some(), "第二季內容併入保留");

    let saved = e.save_docx().expect("存檔失敗");
    let mut r = Engine::from_docx_bytes(&saved).expect("重開失敗");
    settle(&mut r);
    assert_eq!(
        root_table(&r)["table"]["rows"][1]["cells"][1]["props"]["grid_span_raw"].as_str(),
        Some("2"),
        "重開後 gridSpan 保留"
    );
}

/// 在既有表格前插入表格使 Δp≠0：行內圖宿主索引平移後仍發真像素。
#[test]
fn k29c_lamp1_insert_table_shifts_inline_image_host_index() {
    let mut e = open("fixtures/inline_image.docx");
    settle(&mut e);
    assert_eq!(e.paragraph_count(), 3, "inline_image.docx 三個扁平段落");
    assert_eq!(real_pngs(&e, 0).len(), 1, "開檔必須有真 PNG 像素");
    assert_eq!(find_para(&e, "圖片後段"), Some(2));

    e.apply_mutation(Mutation::InsertTable {
        target: SubDocTarget::Body,
        after_para: Some(0),
        rows: 1,
        cols: 1,
        col_widths_pt: Some(vec![100.0]),
    })
    .expect("M4.1 InsertTable 失敗");
    settle(&mut e);

    assert_eq!(e.paragraph_count(), 4, "1×1 表含 1 段 → 段落數 +1");
    assert_eq!(find_para(&e, "圖片後段"), Some(3), "圖後段索引 +1");
    let pngs = real_pngs(&e, 0);
    assert_eq!(
        pngs.len(),
        1,
        "插表後行內圖仍須發射真像素（image_pixels 宿主索引已同步平移）"
    );
    assert!(
        (pngs[0].0 - 200.0).abs() < 0.01 && (pngs[0].1 - 100.0).abs() < 0.01,
        "圖尺寸不變，實際 {:?}",
        pngs[0]
    );
    assert!(
        !has_gray_placeholder(&e, 0),
        "行內圖不得退化為 0xFFF0F0F0 佔位框"
    );
    assert_eq!(
        root_table(&e)["para_start"].as_u64(),
        Some(1),
        "新表首段落在扁平序列索引 1"
    );

    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    assert_eq!(
        diff.whitelisted_changed,
        vec!["word/document.xml".to_string()]
    );
}

/// 跨頁表插入資料列後，`w:tblHeader` 表頭仍逐切片頁重發。
#[test]
fn k29c_lamp1_tblheader_repeats_after_row_reorder() {
    let mut e = open("corpus/selfgen/patho_table_across_pages.docx");
    settle(&mut e);
    let pages_before = e.page_count();
    assert!(pages_before >= 2, "前置：patho 表跨 ≥2 頁");
    assert_eq!(
        pages_with(&e, "項目").len(),
        pages_before,
        "基準：表頭逐頁重發"
    );

    e.apply_mutation(Mutation::InsertTableRow {
        table: TablePath::body(0),
        at_row: 1,
        position: InsertSide::Before,
    })
    .expect("M4.3 InsertTableRow（表頭下）失敗");
    settle(&mut e);

    let pages_after = e.page_count();
    assert!(pages_after >= 2, "重排後表仍須跨 ≥2 頁");
    let hdr = pages_with(&e, "項目");
    assert_eq!(
        hdr.len(),
        pages_after,
        "表頭重排後仍須逐切片頁重發；命中頁 {hdr:?}／共 {pages_after} 頁"
    );
    let num = pages_with(&e, "數值");
    assert_eq!(
        num.len(),
        pages_after,
        "表頭第二格亦須逐切片頁重發；命中頁 {num:?}"
    );

    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    assert_eq!(
        diff.whitelisted_changed,
        vec!["word/document.xml".to_string()]
    );
}

// ---------------------------------------------------------------------------
// 紅燈 2：縮放移動零媒體觸碰＋配對鍵當下同步
// ---------------------------------------------------------------------------

/// 縮放後不必存檔重開，顯示列表與 `(para, w_pt)` 像素配對當下同步。
#[test]
fn k29c_lamp2_resize_updates_display_and_pixel_pairing_without_reopen() {
    let mut e = open("fixtures/inline_image.docx");
    settle(&mut e);
    let base = real_pngs(&e, 0);
    assert_eq!(base.len(), 1, "基準一張行內圖");
    assert!(
        (base[0].0 - 200.0).abs() < 0.01 && (base[0].1 - 100.0).abs() < 0.01,
        "基準圖 200×100pt，實際 {:?}",
        base[0]
    );
    assert!(!has_gray_placeholder(&e, 0));

    e.apply_mutation(Mutation::ResizeOrMoveImage {
        target: SubDocTarget::Body,
        host_para: 1,
        drawing_index: 0,
        new_width_pt: Some(250.0),
        new_height_pt: Some(120.0),
        new_pos_h: None,
        new_pos_v: None,
        new_wrap_mode: None,
    })
    .expect("M5.2 ResizeOrMoveImage 失敗");
    settle(&mut e);

    let pngs = real_pngs(&e, 0);
    assert_eq!(pngs.len(), 1, "縮放不得使真像素消失");
    assert!(
        (pngs[0].0 - 250.0).abs() < 0.01 && (pngs[0].1 - 120.0).abs() < 0.01,
        "不需重開，DrawItem::Image 當下必須 250×120，實際 {:?}",
        pngs[0]
    );
    assert!(
        !has_gray_placeholder(&e, 0),
        "配對鍵 (para, w_pt) 未同步會退化為 0xFFF0F0F0 佔位框"
    );

    // 再縮回卡面指定尺寸 200×100，配對鍵需再次同步。
    e.apply_mutation(Mutation::ResizeOrMoveImage {
        target: SubDocTarget::Body,
        host_para: 1,
        drawing_index: 0,
        new_width_pt: Some(200.0),
        new_height_pt: Some(100.0),
        new_pos_h: None,
        new_pos_v: None,
        new_wrap_mode: None,
    })
    .expect("M5.2 ResizeOrMoveImage 回復尺寸失敗");
    settle(&mut e);
    let pngs = real_pngs(&e, 0);
    assert!(
        (pngs[0].0 - 200.0).abs() < 0.01 && (pngs[0].1 - 100.0).abs() < 0.01,
        "當下必須回到 200×100，實際 {:?}",
        pngs[0]
    );
    assert!(!has_gray_placeholder(&e, 0), "不得退化為佔位框");
}

/// 縮放只改目標 XML part；媒體與 `.rels` 壓縮流零失真。
#[test]
fn k29c_lamp2_resize_keeps_media_and_rels_byte_lossless() {
    let mut e = open("fixtures/inline_image.docx");
    settle(&mut e);

    e.apply_mutation(Mutation::ResizeOrMoveImage {
        target: SubDocTarget::Body,
        host_para: 1,
        drawing_index: 0,
        new_width_pt: Some(250.0),
        new_height_pt: Some(120.0),
        new_pos_h: None,
        new_pos_v: None,
        new_wrap_mode: None,
    })
    .expect("M5.2 ResizeOrMoveImage 失敗");
    settle(&mut e);

    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    assert_eq!(
        diff.whitelisted_changed,
        vec!["word/document.xml".to_string()],
        "縮放只准觸碰 word/document.xml"
    );
    let media = diff
        .per_entry
        .get("word/media/image1.png")
        .expect("media entry 必須存在");
    assert!(
        media.zero_loss(),
        "word/media/*.png 壓縮流必須一字未動: {}",
        media.summary()
    );
    let rels = diff
        .per_entry
        .get("word/_rels/document.xml.rels")
        .expect("rels entry 必須存在");
    assert!(
        rels.zero_loss(),
        "word/_rels/document.xml.rels 必須零失真: {}",
        rels.summary()
    );
}

// ---------------------------------------------------------------------------
// 紅燈 3：媒體去重、CoW Undo、強制 GC 不可逆
// ---------------------------------------------------------------------------

/// 插入位元組完全相同的圖：複用既有 rId，不新增 media part、不改 `.rels`。
#[test]
fn k29c_lamp3_insert_identical_media_reuses_existing_part() {
    let mut e = open("fixtures/inline_image.docx");
    settle(&mut e);
    let png = e
        .part("word/media/image1.png")
        .expect("既有 image1.png")
        .to_vec();
    let rels_before = e
        .part("word/_rels/document.xml.rels")
        .expect("既有 rels")
        .to_vec();

    e.apply_mutation(Mutation::InsertInlineImage {
        target: SubDocTarget::Body,
        pos: DocPos { para: 0, cp: 0 },
        media_bytes: png,
        mime_type: "image/png".to_string(),
        width_pt: 50.0,
        height_pt: 50.0,
        alt_text: Some("複用既有圖".to_string()),
    })
    .expect("M5.1 InsertInlineImage（相同位元組）失敗");
    settle(&mut e);

    let doc = String::from_utf8_lossy(e.part("word/document.xml").unwrap()).to_string();
    assert_eq!(
        doc.matches(r#"r:embed="rIdImg""#).count(),
        2,
        "新舊兩張圖都必須複用既有 rIdImg"
    );
    assert_eq!(
        e.part("word/_rels/document.xml.rels").unwrap(),
        rels_before.as_slice(),
        "複用既有圖不得改寫 .rels"
    );

    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    assert!(
        diff.added.is_empty(),
        "位元組相同不得新增 media part，實際 {:?}",
        diff.added
    );
    assert_eq!(
        diff.whitelisted_changed,
        vec!["word/document.xml".to_string()]
    );
}

/// 插入全新 PNG：新增 media part 與 `.rels` 關係；已有 png Default 時 Content_Types 零增量。
#[test]
fn k29c_lamp3_insert_new_png_adds_media_and_relationship() {
    let mut e = open("fixtures/inline_image.docx");
    settle(&mut e);

    e.apply_mutation(Mutation::InsertInlineImage {
        target: SubDocTarget::Body,
        pos: DocPos { para: 0, cp: 0 },
        media_bytes: NEW_PNG.to_vec(),
        mime_type: "image/png".to_string(),
        width_pt: 40.0,
        height_pt: 40.0,
        alt_text: None,
    })
    .expect("M5.1 InsertInlineImage（全新圖）失敗");
    settle(&mut e);

    let rels_after = String::from_utf8_lossy(e.part("word/_rels/document.xml.rels").unwrap());
    assert!(
        rels_after.contains("media/image2.png"),
        "全新圖必須寫入 media/image2.png 關係: {rels_after}"
    );
    assert_eq!(
        rels_after.matches("relationships/image").count(),
        2,
        "圖形關係數 1→2"
    );

    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    assert!(
        diff.added.iter().any(|n| n == "word/media/image2.png"),
        "必須新增 media part，實際 added={:?}",
        diff.added
    );
    assert!(
        diff.whitelisted_changed
            .contains(&"word/document.xml".to_string()),
        "document.xml 必須在白名單"
    );
    assert!(
        diff.whitelisted_changed
            .contains(&"word/_rels/document.xml.rels".to_string()),
        ".rels 必須在白名單"
    );
    let ct = diff
        .per_entry
        .get("[Content_Types].xml")
        .expect("Content_Types entry");
    assert!(
        ct.zero_loss(),
        "已有 png Default 時 Content_Types 必須零增量: {}",
        ct.summary()
    );
}

/// CoW 替換 → Undo → 存檔：原圖壓縮流 100% 復原，新圖 part 被生命週期逆動作移除。
#[test]
fn k29c_lamp3_cow_replace_undo_restores_original_media_bytes() {
    let mut e = open("fixtures/inline_image.docx");
    settle(&mut e);
    let orig_media = e.part("word/media/image1.png").unwrap().to_vec();
    let orig_doc = e.part("word/document.xml").unwrap().to_vec();

    let out = e
        .apply_mutation(Mutation::ReplaceImageMedia {
            target: SubDocTarget::Body,
            host_para: 1,
            drawing_index: 0,
            new_media_bytes: NEW_PNG.to_vec(),
            new_mime_type: "image/png".to_string(),
        })
        .expect("M5.3 ReplaceImageMedia 失敗");
    assert!(
        matches!(out.reversibility, ReversibilityLevel::StrictByteReversible),
        "CoW 替換必須宣告嚴格可逆，實際 {:?}",
        out.reversibility
    );
    assert!(e.can_undo(), "CoW 替換必須可 Undo");
    assert!(
        e.part("word/media/image1.png").is_some(),
        "CoW 不得刪除原圖"
    );

    e.undo().expect("undo 替換失敗");
    settle(&mut e);

    assert_eq!(
        e.part("word/media/image1.png").unwrap(),
        orig_media.as_slice(),
        "原圖 media 位元組必須零失真"
    );
    assert_eq!(
        e.part("word/document.xml").unwrap(),
        orig_doc.as_slice(),
        "undo 後 document.xml 明文必須復原"
    );
    assert!(
        e.part("word/media/image2.png").is_none(),
        "undo 必須移除本次會話新增的 media part"
    );

    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    // 若 Undo 已讓整檔逐位元組復原，`roundtrip_check` 會提前回 byte_identical 而
    // 不填 per_entry；兩種情形都必須證明原圖壓縮流未被動過。
    let media_data_identical = diff
        .per_entry
        .get("word/media/image1.png")
        .map(|m| m.data_identical)
        .unwrap_or(diff.byte_identical);
    assert!(
        media_data_identical,
        "原圖壓縮流 data_raw 必須逐位元組復原（byte_identical={}）",
        diff.byte_identical
    );

    let saved = e.save_docx().expect("存檔失敗");
    let mut r = Engine::from_docx_bytes(&saved).expect("重開失敗");
    settle(&mut r);
    assert_eq!(r.paragraph_count(), 3, "重開後段落數不變");
    assert_eq!(real_pngs(&r, 0).len(), 1, "重開後原圖仍為真像素");
}

/// 在沒有 `word/_rels/document.xml.rels` 與 png Default 的檔插圖：自動補齊，
/// Undo 時新建 part 必須整份移除、Content_Types 還原。
#[test]
fn k29c_lamp3_insert_into_doc_without_rels_is_fully_reversible() {
    let mut e = open("fixtures/table_mixed.docx");
    settle(&mut e);
    assert!(
        e.part("word/_rels/document.xml.rels").is_none(),
        "前置：table_mixed.docx 沒有 document rels"
    );
    let ct_before = e.part("[Content_Types].xml").unwrap().to_vec();
    assert!(
        !String::from_utf8_lossy(&ct_before).contains(r#"Extension="png""#),
        "前置：Content_Types 尚無 png Default"
    );

    let out = e
        .apply_mutation(Mutation::InsertInlineImage {
            target: SubDocTarget::Body,
            pos: DocPos { para: 0, cp: 0 },
            media_bytes: NEW_PNG.to_vec(),
            mime_type: "image/png".to_string(),
            width_pt: 40.0,
            height_pt: 40.0,
            alt_text: None,
        })
        .expect("無 rels 檔插入圖片失敗");
    assert!(
        matches!(out.reversibility, ReversibilityLevel::StrictByteReversible),
        "插入圖片必須嚴格可逆"
    );
    settle(&mut e);

    assert!(
        e.part("word/_rels/document.xml.rels").is_some(),
        "必須自動新建 document rels"
    );
    let ct_after = String::from_utf8_lossy(e.part("[Content_Types].xml").unwrap()).to_string();
    assert!(
        ct_after.contains(r#"Extension="png""#),
        "必須補上 png Default 最小增量: {ct_after}"
    );
    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    assert!(
        diff.added.iter().any(|n| n == "word/media/image1.png"),
        "必須新增 media part，實際 added={:?}",
        diff.added
    );
    assert!(
        diff.added
            .iter()
            .any(|n| n == "word/_rels/document.xml.rels"),
        "必須新增 rels part，實際 added={:?}",
        diff.added
    );

    e.undo().expect("undo 插圖失敗");
    settle(&mut e);
    assert!(
        e.part("word/media/image1.png").is_none(),
        "undo 必須移除本次新增媒體"
    );
    assert!(
        e.part("word/_rels/document.xml.rels").is_none(),
        "undo 必須移除本次新建 rels（不得留空 part）"
    );
    assert_eq!(
        e.part("[Content_Types].xml").unwrap(),
        ct_before.as_slice(),
        "undo 必須還原 Content_Types"
    );
    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    assert!(
        diff.added.is_empty(),
        "undo 後不得殘留新增 part，實際 {:?}",
        diff.added
    );
}

/// 對本會話新增圖強制 GC：回報不可逆並截斷 Undo 歷史，既有圖不受牽連。
#[test]
fn k29c_lamp3_forced_gc_evicts_new_media_and_is_irreversible() {
    let mut e = open("fixtures/inline_image.docx");
    settle(&mut e);

    e.apply_mutation(Mutation::InsertInlineImage {
        target: SubDocTarget::Body,
        pos: DocPos { para: 0, cp: 0 },
        media_bytes: NEW_PNG.to_vec(),
        mime_type: "image/png".to_string(),
        width_pt: 40.0,
        height_pt: 40.0,
        alt_text: None,
    })
    .expect("插入本次會話新圖失敗");
    settle(&mut e);
    assert!(
        e.part("word/media/image2.png").is_some(),
        "前置：本會話已新增 image2.png"
    );
    assert!(e.can_undo(), "前置：插入後仍有 Undo 歷史");

    let out = e
        .apply_mutation(Mutation::DeleteImage {
            target: SubDocTarget::Body,
            host_para: 0,
            drawing_index: 0,
            gc_unreferenced_media: true,
        })
        .expect("M5.3 DeleteImage（強制 GC）失敗");

    match &out.reversibility {
        ReversibilityLevel::Irreversible { reason } => {
            let s = format!("{reason:?}");
            assert!(
                s.contains("MediaPayloadEvicted"),
                "不可逆理由必須指出媒體已釋放，實際 {s}"
            );
        }
        other => panic!("強制 GC 必須回報不可逆，實際 {other:?}"),
    }
    assert!(
        !e.can_undo(),
        "不可逆操作必須截斷 Undo 歷史（can_undo=false）"
    );
    assert!(
        e.part("word/media/image2.png").is_none(),
        "本會話新增圖的媒體位元組必須被強制釋放"
    );
    assert!(
        e.part("word/media/image1.png").is_some(),
        "開檔既有圖不得被回收"
    );

    let saved = e.save_docx().expect("存檔失敗");
    let mut r = Engine::from_docx_bytes(&saved).expect("重開失敗");
    settle(&mut r);
    assert_eq!(r.paragraph_count(), 3, "重開後段落數不變");
    assert_eq!(
        real_pngs(&r, 0).len(),
        1,
        "刪除本次新增圖後只剩原始行內圖真像素"
    );
}
