//! R31：fdo76591 發射重覆＋VML logo 帶錯位紅燈考卷。
//!
//! 依據任務卡規範：
//! 1. dump JSON 斷言：body 每個可印文字串發射次數＝LO pymupdf 真值計數
//!    （「Scheduled Saturday Overtime」=1、「IMPORTANT NOTICE」=1，全篇掃描零重複）。
//! 2. logo 帶（DODGE/CHRYSLER/Jeep 三圖）bbox y 座標命中 LO pymupdf 真值 ±20pt、彼此不重疊。
//! 3. 班表段落帶與頂部塊 bbox 零重疊。
//!
//! 先紅後綠：考卷必須先在現行代碼上拍紅，記錄失敗輸出後再修綠。
//!
//! 現行代碼實拍紅燈紀錄（2026-11-03 實測輸出）：
//! ```text
//! running 3 tests
//! thread 'r31_lamp1_no_duplicate_emissions' panicked at core/engine-api/tests/r31_dup_emit.rs:53:5:
//! assertion `left == right` failed: 「Scheduled Saturday Overtime」發射次數應為 1，實得 2
//!   left: 2
//!  right: 1
//! thread 'r31_lamp2_logo_band_y_and_images_no_overlap' panicked at core/engine-api/tests/r31_dup_emit.rs:124:5:
//! logo 帶 y 座標 (18.20) 應落在 LO pymupdf 真值 (64.85) ±20pt 內
//! thread 'r31_lamp3_shift_schedule_no_overlap_with_top_block' panicked at core/engine-api/tests/r31_dup_emit.rs:217:5:
//! 班表段落帶起始 y (66.36) 必須大於等於頂部塊最大 y (93.20)，兩者不得重疊
//! test r31_lamp1_no_duplicate_emissions ... FAILED
//! test r31_lamp2_logo_band_y_and_images_no_overlap ... FAILED
//! test r31_lamp3_shift_schedule_no_overlap_with_top_block ... FAILED
//! failures:
//!     r31_lamp1_no_duplicate_emissions
//!     r31_lamp2_logo_band_y_and_images_no_overlap
//!     r31_lamp3_shift_schedule_no_overlap_with_top_block
//! test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
//! ```

use quire_engine::Engine;

fn read_corpus(rel: &str) -> Vec<u8> {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取 {}: {e}", p.display()))
}

/// 燈1：body 每個可印文字串發射次數＝LO pymupdf 真值計數。
///
/// 現行缺陷：
/// 「Scheduled Saturday Overtime」發射 2 次（LO 1 次）。
/// 「IMPORTANT NOTICE」發射 2 次（LO 1 次）。
/// 「PLANT HUMAN RESOURCES DEPARTMENT」發射 2 次（LO 1 次）。
#[test]
fn r31_lamp1_no_duplicate_emissions() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let json_str = eng.page_display_list_json(0).expect("首頁存在");
    let v: serde_json::Value = serde_json::from_str(&json_str).expect("JSON 解析成功");
    let items = v["items"].as_array().expect("items 陣列");

    let mut texts: Vec<String> = Vec::new();
    for it in items {
        if let Some(t) = it.get("Text").and_then(|m| m.get("text")).and_then(|s| s.as_str()) {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                texts.push(trimmed.to_string());
            }
        }
    }

    // 斷言關鍵標題字串在發射項中出現次數
    let count_occurrences = |needle: &str| -> usize {
        texts.iter().filter(|t| t.contains(needle)).count()
    };

    let overtime_count = count_occurrences("Scheduled Saturday Overtime");
    let notice_count = count_occurrences("IMPORTANT NOTICE");
    let hr_count = count_occurrences("PLANT HUMAN RESOURCES DEPARTMENT");

    assert_eq!(
        overtime_count, 1,
        "「Scheduled Saturday Overtime」發射次數應為 1，實得 {overtime_count}"
    );
    assert_eq!(
        notice_count, 1,
        "「IMPORTANT NOTICE」發射次數應為 1，實得 {notice_count}"
    );
    assert_eq!(
        hr_count, 1,
        "「PLANT HUMAN RESOURCES DEPARTMENT」發射次數應為 1，實得 {hr_count}"
    );

    let full_text: String = items
        .iter()
        .filter_map(|it| it.get("Text").and_then(|m| m.get("text")).and_then(|s| s.as_str()))
        .collect();
    let bel_count = full_text.match_indices("BELVIDERE ASSEMBLY PLANT").count();
    assert_eq!(
        bel_count, 1,
        "「BELVIDERE ASSEMBLY PLANT」整篇發射次數應為 1，實得 {bel_count}"
    );

    // 全篇掃描：重要完整詞句不得出現重複發射副本
    for unique_phrase in [
        "ELVIDERE",
        "World Class Manufacturing Starts Here",
        "SATURDAY APRIL 02, 2011",
        "11-016",
        "3-23-2011",
    ] {
        let c = count_occurrences(unique_phrase);
        assert_eq!(
            c, 1,
            "詞句「{unique_phrase}」發射次數應為 1，實得 {c}"
        );
    }
}

/// 燈2：logo 帶（DODGE/CHRYSLER/Jeep 三圖）bbox y 座標命中 LO pymupdf 真值 ±20pt、彼此不重疊。
///
/// LO pymupdf 真值：
/// - logo 帶 (image3.png): y = 64.85 pt, h = 75.00 pt (範圍 [64.85, 139.85])
/// - World Class logo (image1.emf): y = 594.00 pt, h = 93.00 pt (範圍 [594.00, 687.00])
/// - Belvidere logo (image2.emf): y = 732.80 pt, h = 34.55 pt (範圍 [732.80, 767.35])
///
/// 現行缺陷：
/// logo 帶被錯誤發射在 y = 18.20 pt（誤差 46.65 pt，超出 ±20pt）。
#[test]
fn r31_lamp2_logo_band_y_and_images_no_overlap() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let json_str = eng.page_display_list_json(0).expect("首頁存在");
    let v: serde_json::Value = serde_json::from_str(&json_str).expect("JSON 解析成功");
    let items = v["items"].as_array().expect("items 陣列");

    struct ImgBox {
        y: f64,
        h: f64,
        w: f64,
    }

    let mut images: Vec<ImgBox> = Vec::new();
    for it in items {
        if let Some(im) = it.get("Image") {
            let y = im["y"].as_f64().expect("y");
            let h = im["h"].as_f64().expect("h");
            let w = im["w"].as_f64().expect("w");
            images.push(ImgBox { y, h, w });
        }
    }

    assert_eq!(images.len(), 3, "fdo76591 應發射 3 張點陣圖，實得 {}", images.len());

    // 尋找寬度約 495.75pt 的 logo 帶（image3.png）
    let logo_band = images
        .iter()
        .find(|im| (im.w - 495.75).abs() < 10.0 || (im.h - 75.0).abs() < 5.0)
        .expect("應包含 logo 帶圖片");

    let lo_logo_y = 64.85;
    assert!(
        (logo_band.y - lo_logo_y).abs() <= 20.0,
        "logo 帶 y 座標 ({:.2}) 應落在 LO pymupdf 真值 ({lo_logo_y}) ±20pt 內",
        logo_band.y
    );

    // 三圖彼此 y 區間不得重疊
    for i in 0..images.len() {
        for j in (i + 1)..images.len() {
            let a = &images[i];
            let b = &images[j];
            let a_top = a.y;
            let a_bot = a.y + a.h;
            let b_top = b.y;
            let b_bot = b.y + b.h;
            let overlap = (a_top < b_bot) && (b_top < a_bot);
            assert!(
                !overlap,
                "圖片 {i} (y: {:.2}..{:.2}) 與圖片 {j} (y: {:.2}..{:.2}) 不得重疊",
                a_top, a_bot, b_top, b_bot
            );
        }
    }
}

/// 燈3：班表段落帶與頂部塊 bbox 零重疊。
///
/// LO 版面配置：
/// - 頂部塊（標題文字＋logo 帶＋重要通知＋加班預告）：y 範圍約 26pt 至 195pt。
/// - 班表段落帶（1st Shift、2nd Shift 各班次時間）：y 範圍約 390pt 至 556pt。
/// - 兩者垂直區隔顯著，完全零重疊。
///
/// 現行缺陷：
/// 框內文字行高塌陷為 2.76pt，且頂部塊與班表全部被壓縮堆疊在頁首（y = 18..94），
/// 班表與頂部塊嚴重重疊。
#[test]
fn r31_lamp3_shift_schedule_no_overlap_with_top_block() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let json_str = eng.page_display_list_json(0).expect("首頁存在");
    let v: serde_json::Value = serde_json::from_str(&json_str).expect("JSON 解析成功");
    let items = v["items"].as_array().expect("items 陣列");

    let mut top_block_max_y: f64 = 0.0;
    let mut shift_schedule_min_y: f64 = f64::MAX;

    for it in items {
        if let Some(t) = it.get("Text") {
            let txt = t["text"].as_str().unwrap_or("");
            let y = t["y"].as_f64().expect("y");

            if txt.contains("BELVIDERE")
                || txt.contains("World Class")
                || txt.contains("IMPORTANT NOTICE")
                || txt.contains("Scheduled Saturday Overtime")
            {
                if y > top_block_max_y {
                    top_block_max_y = y;
                }
            }

            if txt.contains("1st Shift")
                || txt.contains("2nd Shift")
                || txt.contains("Start – 2:30")
                || txt.contains("12:00 a.m.")
            {
                if y < shift_schedule_min_y {
                    shift_schedule_min_y = y;
                }
            }
        } else if let Some(im) = it.get("Image") {
            // logo 帶屬於頂部塊
            let w = im["w"].as_f64().expect("w");
            let y = im["y"].as_f64().expect("y");
            let h = im["h"].as_f64().expect("h");
            if (w - 495.75).abs() < 10.0 {
                let bot = y + h;
                if bot > top_block_max_y {
                    top_block_max_y = bot;
                }
            }
        }
    }

    assert!(
        top_block_max_y > 0.0,
        "頂部塊應存在"
    );
    assert!(
        shift_schedule_min_y < f64::MAX,
        "班表段落帶應存在"
    );

    assert!(
        shift_schedule_min_y >= top_block_max_y,
        "班表段落帶起始 y ({shift_schedule_min_y:.2}) 必須大於等於頂部塊最大 y ({top_block_max_y:.2})，兩者不得重疊"
    );
}
