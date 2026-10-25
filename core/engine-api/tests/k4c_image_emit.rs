//! K4c-image：`DrawItem::Image` 變體＋內嵌圖片像素真發射（先紅後綠迴歸鎖）。
//!
//! 紅燈口徑（PM 實拆 Bug51170 header1.xml）：
//! - Bug51170 頁首 `wp:inline` 圖的 blip 候選對（`ve:AlternateContent`）：
//!   Choice `rId1→media/image1.pdf`（不可渲染）、Fallback `rId2→media/image2.png`
//!   （7998B 可渲染）。決策規則＝同一候選內「可渲染格式＞不可渲染格式」，
//!   皆可渲染取先見 ⇒ 必須發 `DrawItem::Image` 且像素為 PNG。
//! - n779834 頁首浮水印為 EMF（`v:imagedata r:id→media/image1.emf`）：
//!   EMF 本卡不解 ⇒ 走既有佔位框、**不得**發 Image。
//!
//! 斷言刻意走 **JSON 層**（`page_display_list_json`）而非直接 match 變體：
//! 紅燈階段（變體尚未存在）測試仍可編譯、老實失敗，不是編譯錯。

use quire_engine::Engine;

/// 從單頁顯示列表 JSON 抽出所有 Image 項（欄位＝mime/data_b64/x/y/w/h）。
fn image_items(json: &str) -> Vec<serde_json::Map<String, serde_json::Value>> {
    let v: serde_json::Value = serde_json::from_str(json).expect("顯示列表 JSON 可解析");
    v["items"]
        .as_array()
        .expect("items 陣列")
        .iter()
        .filter_map(|it| it.get("Image").and_then(|m| m.as_object()).cloned())
        .collect()
}

/// 佔位框 Rect 數（fill＝IMAGE_PLACEHOLDER_FILL＝0xFFF0F0F0）。
fn placeholder_rects(json: &str) -> usize {
    let v: serde_json::Value = serde_json::from_str(json).expect("顯示列表 JSON 可解析");
    v["items"]
        .as_array()
        .expect("items 陣列")
        .iter()
        .filter(|it| {
            it.get("Rect")
                .and_then(|r| r.get("fill_argb"))
                .and_then(|f| f.as_u64())
                == Some(0xFFF0_F0F0)
        })
        .count()
}

/// 紅燈主案：Bug51170 頁首圖必須以真像素發射。
/// Choice 指向 image1.pdf（本渲染器不認）→ 依 PM 決策規則退到 Fallback 的
/// image2.png（PNG magic 89 50 4E 47 0D 0A 1A 0A ⇒ base64 前綴 `iVBORw0KGgo`）。
#[test]
fn bug51170_header_image_emits_png_drawitem() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/poi/test-data_document_Bug51170.docx"
    ))
    .expect("讀 corpus Bug51170");
    let eng = Engine::from_docx_bytes(&bytes).expect("open");
    assert!(eng.page_count() >= 2, "Bug51170 應有多頁");
    // titlePg＋無 first 頁首：首頁依語義空白，default 頁首掛在頁 1..。
    let mut total_images = 0usize;
    for page in 1..eng.page_count() {
        let json = eng.page_display_list_json(page).expect("頁存在");
        let imgs = image_items(&json);
        assert!(
            !imgs.is_empty(),
            "第 {page} 頁頁首圖必須發 DrawItem::Image（像素真發射）：{json}"
        );
        let im = &imgs[0];
        let b64 = im["data_b64"].as_str().expect("data_b64 字串");
        assert!(
            b64.starts_with("iVBORw0KGgo"),
            "Bug51170 頁首圖像素必須解出 PNG magic（Fallback image2.png）；實際前綴：{}",
            &b64[..b64.len().min(24)]
        );
        assert_eq!(
            im["mime"].as_str(),
            Some("image/png"),
            "副檔名 png → image/png"
        );
        // 幾何＝extent 146×78pt（1854200×990600 EMU），且落在頁首區（正文之前）。
        let w = im["w"].as_f64().expect("w");
        let h = im["h"].as_f64().expect("h");
        assert!(
            (w - 146.0).abs() < 0.1 && (h - 78.0).abs() < 0.1,
            "extent 應為 146×78pt：{w}×{h}"
        );
        total_images += imgs.len();
    }
    assert!(total_images >= eng.page_count() - 1, "每頁頁首都該帶圖");
    // 頁首圖已有真像素 ⇒ 該處不再發灰底佔位框（同一張圖不雙發）。
    let json = eng.page_display_list_json(1).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let img_y = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|it| it.get("Image"))
        .filter_map(|im| im["y"].as_f64())
        .next()
        .expect("有 Image 項");
    let dup = v["items"].as_array().unwrap().iter().any(|it| {
        matches!(it.get("Rect"), Some(r)
                if r["fill_argb"].as_u64() == Some(0xFFF0_F0F0)
                    && (r["y"].as_f64().unwrap_or(-1.0) - img_y).abs() < 0.01)
    });
    assert!(
        !dup,
        "同位不得再發灰底佔位框（真像素取代佔位，不雙發）：{json}"
    );
}

/// EMF 退路案：n779834 頁首浮水印是 EMF（本卡不要求解 EMF）——
/// 必須走既有佔位框（灰底 Rect 仍在），且**不得**發任何 Image 項。
#[test]
fn emf_watermark_falls_back_to_placeholder() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/ooxmlexport/n779834.docx"
    ))
    .expect("讀 corpus n779834");
    let eng = Engine::from_docx_bytes(&bytes).expect("open");
    let mut saw_placeholder = false;
    for page in 0..eng.page_count() {
        let json = eng.page_display_list_json(page).expect("頁存在");
        assert!(
            image_items(&json).is_empty(),
            "EMF 不可渲染 ⇒ 第 {page} 頁不得發 Image 項（須退佔位框）"
        );
        saw_placeholder |= placeholder_rects(&json) > 0;
    }
    assert!(
        saw_placeholder,
        "n779834 至少一頁要維持 EMF 的灰底佔位框（誠實近似不退化）"
    );
}

/// 防假綠（buggy 回塞鎖）：正文流 inline 圖（fixtures/inline_image.docx 帶
/// 真實 67B PNG，rIdImg→media/image1.png）——修後應發 PNG Image 項，
/// 不再只有佔位框；尺寸＝extent 200×100pt、x＝版心左緣。
#[test]
fn inline_fixture_png_emitted_not_placeholder() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/inline_image.docx"
    ))
    .expect("讀 fixtures/inline_image.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("open");
    let json = eng.page_display_list_json(0).expect("頁存在");
    let imgs = image_items(&json);
    assert_eq!(imgs.len(), 1, "恰好一張真像素圖：{json}");
    let im = &imgs[0];
    let b64 = im["data_b64"].as_str().expect("data_b64");
    assert!(b64.starts_with("iVBORw0KGgo"), "PNG magic：{b64}");
    assert_eq!(im["mime"].as_str(), Some("image/png"));
    let (w, h) = (im["w"].as_f64().unwrap(), im["h"].as_f64().unwrap());
    assert!(
        (w - 200.0).abs() < 0.01 && (h - 100.0).abs() < 0.01,
        "extent 200×100pt：{w}×{h}"
    );
    // 佔位框退位：真像素取代灰底 Rect＋四 Rule。
    assert_eq!(placeholder_rects(&json), 0, "真像素处不再發佔位框");
}
