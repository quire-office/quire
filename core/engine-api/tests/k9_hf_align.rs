//! K9 紅燈：頁首圖置中＋頁碼 tab 停靠（Bug51170 肉眼缺陷兩條）。
//!
//! 缺陷與真值（PM 像素實測＋LO 實測）：
//! 1. Bug51170 p2 頁首插圖：LO bbox x 中樞 51.3%（LO 實測 mid_x=307.15pt / 595.30pt = 51.6%）；
//!    我方修前 27.4%（左齊 margin_left 89.85pt）。
//! 2. Bug51170 頁腳頁碼：LO 數字停靠右緣（LO 實測 x=518.90pt，右緣 524.47pt）；
//!    我方修前黏頁腳左端（x=89.85pt）。
//!    版心可用寬度 content_w = 434.65pt，右緣佔版心 98.7%（>= 0.9×版寬）。
//! 3. 零 jc／無 tabs 檔零擾動守門。
//! 4. BUGGY 回塞鎖：`K9_HF_ALIGN_BUGGY=1` 退回舊徑 ⇒ 紅燈必紅。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

const B51170: &str = "poi/test-data_document_Bug51170.docx";
const HF_FIXTURE: &[u8] = include_bytes!("../../../fixtures/hf_table_header.docx");

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 docx")
}

/// 紅燈 1：Bug51170 p2 頁首圖 bbox 中樞 |x - LO 51.3%| <= 2%。
/// 修前舊徑：圖片左齊 margin_left 89.85pt，中樞 162.85pt (27.4%) ⇒ 必紅。
#[test]
fn k9_bug51170_header_image_centered() {
    let eng = load_corpus(B51170);
    assert!(eng.page_count() >= 2, "Bug51170 應至少 2 頁");

    let page = 1; // p2 (0-based 1)
    let dl = eng.page_display_list(page).expect("p2 顯示列表存在");
    let page_w = dl.width_pt;

    let header_imgs: Vec<(f32, f32, f32, f32)> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Image { x, y, w, h, .. } if *y < 120.0 => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect();

    assert!(
        !header_imgs.is_empty(),
        "p2 頁首區（y < 120.0）必須有 Image"
    );

    let (x, _, w, _) = header_imgs[0];
    let mid_x = x + w / 2.0;
    let ratio = mid_x / page_w;

    assert!(
        (ratio - 0.513).abs() <= 0.02,
        "頁首圖 bbox 中樞比率 {ratio:.4}（mid_x={mid_x:.2}pt）偏離 LO 51.3% 超過 2%（修前舊徑 27.4% 必紅）"
    );
}

/// 紅燈 2：Bug51170 頁碼項停靠右緣，x >= 0.9×版寬且對齊 LO 真值 518.90pt。
/// 修前舊徑：頁碼黏在頁腳文字左端（x=89.85pt）⇒ 必紅。
#[test]
fn k9_bug51170_footer_page_number_pinned_right() {
    let eng = load_corpus(B51170);
    assert!(eng.page_count() >= 2, "Bug51170 應至少 2 頁");

    let page = 1; // p2
    let dl = eng.page_display_list(page).expect("p2 顯示列表存在");
    let page_w = dl.width_pt;
    // Bug51170 pgMar: left=1797twip=89.85pt, right=1417twip=70.85pt => content_w = 434.65pt
    let margin_left = 89.85f32;
    let margin_right = 70.85f32;
    let content_w = page_w - margin_left - margin_right;

    // p2 頁碼為 "2"
    let page_num_item = dl.items.iter().find_map(|it| match it {
        DrawItem::Text { text, x, y, .. } if *y > 750.0 && text.trim() == "2" => {
            Some((*x, *y, text.clone()))
        }
        _ => None,
    });

    assert!(
        page_num_item.is_some(),
        "p2 頁腳區必須有頁碼 '2'：{:?}",
        dl.items
            .iter()
            .filter_map(|it| match it {
                DrawItem::Text { text, y, .. } if *y > 750.0 => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
    );

    let (x, y, _) = page_num_item.unwrap();

    // 斷言 1：x >= margin_left + 0.9 × 版心可用寬度（版寬）
    assert!(
        x >= margin_left + 0.9 * content_w,
        "p2 頁碼 x={x:.2}pt 未停靠右緣（應 >= margin_left + 0.9*content_w = {:.2}pt）",
        margin_left + 0.9 * content_w
    );

    // 斷言 2：對齊 LO 真值 518.90pt（±5.0pt 容差）
    assert!(
        (x - 518.90).abs() <= 5.0,
        "p2 頁碼 x={x:.2}pt 偏離 LO 真值 518.90pt 超過 5.0pt（y={y:.2}）"
    );
}

/// 綠燈守門：零 jc／無 tabs 檔零擾動。
/// fixtures/hf_table_header.docx 首頁 first 頁首文字與次頁 default 表格文字
/// 維持原樣從 margin_left 起排。
#[test]
fn k9_zero_drift_unaffected_files() {
    let eng = Engine::from_docx_bytes(HF_FIXTURE).expect("開啟 hf_table_header.docx");

    // p0 首頁文字 "首頁專用頁首" 在 margin_left (90.0pt，Letter 預設 1.25 吋)
    let p0 = eng.page_display_list(0).unwrap();
    let p0_text = p0
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text { text, x, y, .. } if text == "首頁專用頁首" => Some((*x, *y)),
            _ => None,
        })
        .expect("首頁專用頁首文字存在");
    assert!(
        (p0_text.0 - 90.0).abs() < 0.1,
        "首頁專用頁首 x 應維持 90.0pt，實得 {}",
        p0_text.0
    );

    // p1 第二頁表格文字 "機關名義：奎爾文書局" 在 margin_left 90pt + cell margin 5.4pt = 95.4pt
    let p1 = eng.page_display_list(1).unwrap();
    let p1_text = p1
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text { text, x, y, .. } if text == "機關名義：奎爾文書局" => {
                Some((*x, *y))
            }
            _ => None,
        })
        .expect("第二頁表格文字存在");
    assert!(
        (p1_text.0 - 95.4).abs() < 0.1,
        "第二頁表格文字 x 應維持 95.4pt，實得 {}",
        p1_text.0
    );
}

/// BUGGY 回塞鎖驗證：當 `K9_HF_ALIGN_BUGGY=1` 時，確認退回修前舊徑（左齊 margin_left）。
#[test]
fn k9_buggy_env_restores_red() {
    if std::env::var_os("K9_HF_ALIGN_BUGGY").is_none() {
        eprintln!("未設 K9_HF_ALIGN_BUGGY：跳過回塞檢驗（以 K9_HF_ALIGN_BUGGY=1 跑）。");
        return;
    }
    let eng = load_corpus(B51170);
    let page = 1;
    let dl = eng.page_display_list(page).expect("p2 顯示列表存在");
    let page_w = dl.width_pt;

    // (1) 圖片回塞舊徑：x ≈ 89.85pt, mid_x ≈ 162.85pt (27.4% 頁寬)
    let header_imgs: Vec<(f32, f32, f32, f32)> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Image { x, y, w, h, .. } if *y < 120.0 => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect();
    assert!(!header_imgs.is_empty());
    let (x, _, w, _) = header_imgs[0];
    let mid_x = x + w / 2.0;
    let ratio = mid_x / page_w;
    assert!(
        (ratio - 0.274).abs() < 0.02,
        "BUGGY 態頁首圖應回到左齊 27.4%，實得 {ratio:.4}"
    );

    // (2) 頁碼回塞舊徑：x ≈ 89.85pt（左緣）
    let page_num_x = dl.items.iter().find_map(|it| match it {
        DrawItem::Text { text, x, y, .. } if *y > 750.0 && text.trim() == "2" => Some(*x),
        _ => None,
    });
    assert_eq!(
        page_num_x.map(|x| (x * 100.0).round() / 100.0),
        Some(89.85),
        "BUGGY 態頁碼應回到左齊 89.85pt"
    );
}
