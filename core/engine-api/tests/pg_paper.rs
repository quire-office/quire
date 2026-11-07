//! PG-PAPER-1 引擎紙張覆寫 API 考卷
//!
//! 燈 1：同一 docx 用 with_paper(Some(A3)) 後 paper_json 頁尺寸＝A3（±0.01pt），且分頁斷點序列 ≠ None 版。
//! 燈 2：邊距 Option 語意——給 top/bottom 覆寫、左右 None => 左右保留文件原值（逐欄斷言）。
//! 燈 3：零回退鐵閘——from_docx_bytes（不帶覆寫）對 golden 檔輸出 JSON byte-identical。
//! 燈 4：覆寫版 engine 接續編輯（replace_paragraph_text）後紙張仍是覆寫值（快取不回收）。
//! 燈 5：多 section 檔全部節套同一覆寫（fdo76591／cell-grid-span 實查單節，採用 tdf124637 多節實測）。

use quire_engine::{Engine, PaperSpec};

const CELL_GRID_SPAN: &[u8] = include_bytes!("../../../corpus/ooxmlexport/cell-grid-span.docx");
const HC031: &[u8] =
    include_bytes!("../../../corpus/xopt/TestFiles_HC031-Complicated-Document.docx");
const TDF124637: &[u8] = include_bytes!("../../../corpus/ooxmlexport/tdf124637_sectionMargin.docx");

// ISO 216 A3 點數尺寸：297mm × 420mm ≈ 841.89pt × 1190.55pt
const A3_W_PT: f32 = 841.89;
const A3_H_PT: f32 = 1190.55;

#[derive(Debug, serde::Deserialize)]
struct PaperJson {
    page_w_pt: f32,
    page_h_pt: f32,
    margin_top_pt: f32,
    margin_bottom_pt: f32,
    margin_left_pt: f32,
    margin_right_pt: f32,
    paper_override_applied: bool,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct PageDisplayList {
    width_pt: f32,
    height_pt: f32,
    items: Vec<serde_json::Value>,
}

/// 燈 1：同一 docx 用 with_paper(Some(A3)) 後 paper_json 頁尺寸＝A3（±0.01pt），
/// 且分頁斷點序列 ≠ None 版（鑑別力：證明真重排）。
#[test]
fn lamp1_a3_paper_override_and_reflow_different() {
    let eng_default = Engine::from_docx_bytes(HC031).expect("解析預設紙張失敗");
    let default_pages = eng_default.page_count();

    let a3_spec = PaperSpec {
        page_w_pt: A3_W_PT,
        page_h_pt: A3_H_PT,
        margin_top_pt: None,
        margin_bottom_pt: None,
        margin_left_pt: None,
        margin_right_pt: None,
    };
    let eng_a3 =
        Engine::from_docx_bytes_with_paper(HC031, Some(&a3_spec)).expect("解析 A3 覆寫紙張失敗");

    // 斷言 paper_json 規格
    let info: PaperJson = serde_json::from_str(&eng_a3.paper_json()).expect("解析 paper_json 失敗");
    assert!(
        (info.page_w_pt - A3_W_PT).abs() < 0.01,
        "頁寬應為 A3 寬度 {A3_W_PT}，實得 {}",
        info.page_w_pt
    );
    assert!(
        (info.page_h_pt - A3_H_PT).abs() < 0.01,
        "頁高應為 A3 高度 {A3_H_PT}，實得 {}",
        info.page_h_pt
    );
    assert!(
        info.paper_override_applied,
        "paper_override_applied 必須為 true"
    );
    assert!(
        eng_a3.paper_override_applied(),
        "getter paper_override_applied() 必須為 true"
    );

    // 斷言真重排：A3 版面容量遠大於原始尺寸，總頁數與每頁顯示列表項數序列必定不同
    let a3_pages = eng_a3.page_count();
    assert_ne!(
        default_pages, a3_pages,
        "A3 覆寫後頁數應與原始頁數不同（原始 {default_pages} 頁，A3 {a3_pages} 頁）"
    );

    // 逐頁檢查第 0 頁的顯示列表寬高亦為 A3
    let dl_json = eng_a3.page_display_list_json(0).expect("首頁存在");
    let dl: PageDisplayList = serde_json::from_str(&dl_json).expect("解析 display list 失敗");
    assert!(
        (dl.width_pt - A3_W_PT).abs() < 0.01,
        "顯示列表寬應為 A3 寬度，實得 {}",
        dl.width_pt
    );
    assert!(
        (dl.height_pt - A3_H_PT).abs() < 0.01,
        "顯示列表高應為 A3 高度，實得 {}",
        dl.height_pt
    );
}

/// 燈 2：邊距 Option 語意——給 top/bottom 覆寫、左右 None => 左右保留文件原值（逐欄斷言）。
#[test]
fn lamp2_margin_option_semantics() {
    let eng_default = Engine::from_docx_bytes(CELL_GRID_SPAN).expect("解析原始失敗");
    let orig_info: PaperJson =
        serde_json::from_str(&eng_default.paper_json()).expect("解析原始 paper_json 失敗");
    assert!(!orig_info.paper_override_applied);
    assert!(!eng_default.paper_override_applied());

    // 覆寫上下邊距，左右維持 None
    let override_top = 108.0;
    let override_bottom = 144.0;
    let spec = PaperSpec {
        page_w_pt: orig_info.page_w_pt,
        page_h_pt: orig_info.page_h_pt,
        margin_top_pt: Some(override_top),
        margin_bottom_pt: Some(override_bottom),
        margin_left_pt: None,
        margin_right_pt: None,
    };

    let eng_override =
        Engine::from_docx_bytes_with_paper(CELL_GRID_SPAN, Some(&spec)).expect("覆寫邊距開檔失敗");
    let new_info: PaperJson =
        serde_json::from_str(&eng_override.paper_json()).expect("解析覆寫 paper_json 失敗");

    // 逐欄斷言
    assert!(
        (new_info.margin_top_pt - override_top).abs() < 0.01,
        "上邊距應被覆寫為 {override_top}，實得 {}",
        new_info.margin_top_pt
    );
    assert!(
        (new_info.margin_bottom_pt - override_bottom).abs() < 0.01,
        "下邊距應被覆寫為 {override_bottom}，實得 {}",
        new_info.margin_bottom_pt
    );
    assert!(
        (new_info.margin_left_pt - orig_info.margin_left_pt).abs() < 0.01,
        "左邊距應保留原值 {}，實得 {}",
        orig_info.margin_left_pt,
        new_info.margin_left_pt
    );
    assert!(
        (new_info.margin_right_pt - orig_info.margin_right_pt).abs() < 0.01,
        "右邊距應保留原值 {}，實得 {}",
        orig_info.margin_right_pt,
        new_info.margin_right_pt
    );
    assert!(new_info.paper_override_applied);
    assert!(eng_override.paper_override_applied());
}

/// 燈 3（零回退鐵閘）：from_docx_bytes（不帶覆寫）對 golden 檔輸出 JSON byte-identical。
#[test]
fn lamp3_zero_regression_golden_byte_identical() {
    let eng1 = Engine::from_docx_bytes(CELL_GRID_SPAN).expect("第一次開檔失敗");
    let eng2 =
        Engine::from_docx_bytes_with_paper(CELL_GRID_SPAN, None).expect("with_paper None 開檔失敗");

    assert!(!eng1.paper_override_applied());
    assert!(!eng2.paper_override_applied());
    assert_eq!(eng1.paper_json(), eng2.paper_json());
    assert_eq!(eng1.page_count(), eng2.page_count());

    for p in 0..eng1.page_count() {
        let dl1 = eng1.page_display_list_json(p).expect("eng1 頁存在");
        let dl2 = eng2.page_display_list_json(p).expect("eng2 頁存在");
        assert_eq!(dl1, dl2, "第 {p} 頁顯示列表 JSON 必須 100% byte-identical");
    }
}

/// 燈 4：覆寫版 engine 接續編輯（replace_paragraph_text）後紙張仍是覆寫值（快取不回收）。
#[test]
fn lamp4_override_persists_after_edit() {
    let spec = PaperSpec {
        page_w_pt: A3_W_PT,
        page_h_pt: A3_H_PT,
        margin_top_pt: Some(72.0),
        margin_bottom_pt: Some(72.0),
        margin_left_pt: Some(72.0),
        margin_right_pt: Some(72.0),
    };
    let mut eng =
        Engine::from_docx_bytes_with_paper(CELL_GRID_SPAN, Some(&spec)).expect("覆寫開檔失敗");

    // 驗證編輯前狀態
    assert!(eng.paper_override_applied());
    let info_before: PaperJson =
        serde_json::from_str(&eng.paper_json()).expect("解析編輯前 paper_json 失敗");
    assert!((info_before.page_w_pt - A3_W_PT).abs() < 0.01);

    // 執行段落文字替換
    let _ = eng.replace_paragraph_text(0, "這是編輯後的首段文字測試內容。");

    // 斷言編輯後紙張規格依然為覆寫值，快取不回收
    assert!(
        eng.paper_override_applied(),
        "編輯後 paper_override_applied 必須依然為 true"
    );
    let info_after: PaperJson =
        serde_json::from_str(&eng.paper_json()).expect("解析編輯後 paper_json 失敗");
    assert!(
        (info_after.page_w_pt - A3_W_PT).abs() < 0.01,
        "編輯後頁寬仍應為 A3 寬度，實得 {}",
        info_after.page_w_pt
    );
    assert!(
        (info_after.page_h_pt - A3_H_PT).abs() < 0.01,
        "編輯後頁高仍應為 A3 高度，實得 {}",
        info_after.page_h_pt
    );
    assert!(
        (info_after.margin_top_pt - 72.0).abs() < 0.01,
        "編輯後上邊距仍應為 72.0，實得 {}",
        info_after.margin_top_pt
    );

    // 檢查顯示列表寬高亦為 A3
    let dl_json = eng.page_display_list_json(0).expect("首頁存在");
    let dl: PageDisplayList = serde_json::from_str(&dl_json).expect("解析 display list 失敗");
    assert!((dl.width_pt - A3_W_PT).abs() < 0.01);
    assert!((dl.height_pt - A3_H_PT).abs() < 0.01);
}

/// 燈 5：多 section 檔全部節套同一覆寫。
#[test]
fn lamp5_multi_section_override_all_sections() {
    // 依解剖報告實測：tdf124637_sectionMargin.docx 含有 7 個節，為真實多 section 檔案
    let custom_w = 500.0;
    let custom_h = 750.0;
    let custom_top = 40.0;
    let spec = PaperSpec {
        page_w_pt: custom_w,
        page_h_pt: custom_h,
        margin_top_pt: Some(custom_top),
        margin_bottom_pt: None,
        margin_left_pt: None,
        margin_right_pt: None,
    };

    let eng = Engine::from_docx_bytes_with_paper(TDF124637, Some(&spec)).expect("多節覆寫開檔失敗");

    assert!(eng.paper_override_applied());
    let page_count = eng.page_count();
    assert!(page_count >= 2, "多節檔應有複數頁面，實得 {page_count}");

    // 檢查每一頁的顯示列表，寬高均必須為自訂紙張尺寸
    for p in 0..page_count {
        let dl_json = eng.page_display_list_json(p).expect("頁存在");
        let dl: PageDisplayList = serde_json::from_str(&dl_json).expect("解析 display list 失敗");
        assert!(
            (dl.width_pt - custom_w).abs() < 0.01,
            "第 {p} 頁寬應為覆寫值 {custom_w}，實得 {}",
            dl.width_pt
        );
        assert!(
            (dl.height_pt - custom_h).abs() < 0.01,
            "第 {p} 頁高應為覆寫值 {custom_h}，實得 {}",
            dl.height_pt
        );
    }
}
