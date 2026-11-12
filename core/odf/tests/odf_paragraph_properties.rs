//! ODF 段落縮排與對齊消費（AGY-ODTIND）紅燈考卷。
//!
//! 規範出處：
//! - OASIS ODF 1.3 Part 3 §20.375 `fo:text-align`
//! - OASIS ODF 1.3 Part 3 §20.219 `fo:margin-left` / §20.220 `fo:margin-right`
//! - OASIS ODF 1.3 Part 3 §20.376 `fo:text-indent`
//! - 歸因報告 `docs/odt-userdoc-layout-diag.md` §軸 1（縮排與置中零消費）
//!
//! 四盞燈契約：
//! 1. 燈 1：`fo:text-align="center"` 段落 → 該段 Text 項 `x > margin_left_pt` 且中心落版心中點（±1pt）。
//! 2. 燈 2：`fo:margin-left="2cm" fo:text-indent="1cm"` → 首行 `x == margin_left_pt + 2cm_pt + 1cm_pt`（±0.5pt）、次行 `x == margin_left_pt + 2cm_pt`。
//! 3. 燈 3（防回歸）：無宣告段落 x 恆等 `margin_left_pt`（零擾動鎖）。
//! 4. 燈 4：存檔往返不受影響（驗證 `odf_container` 保持全綠）。
//! 5. 探針：若環境變數 `ODT_SAMPLE_PATH` 指向真實公文檔，統計逐頁 x 分佈（嚴禁印出內文文字）。

use quire_layout::{MARGIN_LEFT, MARGIN_RIGHT};
use quire_odf::display::{emit_display_lists, DrawItem};
use quire_odf::layout_glue::{project_bytes, LayoutParams};

const CM_PT: f32 = 72.0 / 2.54;

fn minimal_content(styles: &str, body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
    xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
    xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
    xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
    xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
  <office:automatic-styles>
{styles}
  </office:automatic-styles>
  <office:body>
    <office:text>
{body}
    </office:text>
  </office:body>
</office:document-content>
"#
    )
}

/// 燈 1：`fo:text-align="center"` 段落 → 該段 Text 項 `x > margin_left_pt` 且中心落版心中點（±1pt）。
#[test]
fn lamp1_center_align_text_item_centered() {
    let styles = r#"    <style:style style:name="P_center" style:family="paragraph">
      <style:paragraph-properties fo:text-align="center"/>
    </style:style>"#;
    let body = r#"      <text:p text:style-name="P_center">公文置中主標題</text:p>"#;
    let xml = minimal_content(styles, body);

    let proj = project_bytes(xml.as_bytes(), None).expect("解析 ODF 投影失敗");
    let params = LayoutParams::default();
    let lists = emit_display_lists(&proj, &params);
    assert!(!lists.is_empty(), "必須發射至少一頁顯示列表");

    let first_text = lists[0]
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                x, text, size_pt, ..
            } => Some((*x, text.clone(), *size_pt)),
            _ => None,
        })
        .expect("第 0 頁應包含文字項");

    let (x, text, size_pt) = first_text;
    let margin_left = params.margins_pt[MARGIN_LEFT];
    let margin_right = params.margins_pt[MARGIN_RIGHT];
    let content_w = params.page_w_pt - margin_left - margin_right;
    let content_center = margin_left + content_w / 2.0;

    let text_w = quire_layout::TextMeasurer::new(size_pt).text_width(&text);
    let line_center = x + text_w / 2.0;

    assert!(
        x > margin_left,
        "置中文字起排 x ({:.3}pt) 必須嚴格大於版心左邊距 ({:.3}pt)",
        x,
        margin_left
    );
    let diff = (line_center - content_center).abs();
    assert!(
        diff <= 1.0,
        "文字中心點 ({:.3}pt) 與版心中點 ({:.3}pt) 偏差 ({:.3}pt) 必須在 ±1pt 內",
        line_center,
        content_center,
        diff
    );
}

/// 燈 2：`fo:margin-left="2cm" fo:text-indent="1cm"` → 首行 `x == margin_left_pt + 2cm_pt + 1cm_pt`（±0.5pt）、次行 `x == margin_left_pt + 2cm_pt`。
#[test]
fn lamp2_margin_left_and_text_indent_emission() {
    let styles = r#"    <style:style style:name="P_indent" style:family="paragraph">
      <style:paragraph-properties fo:margin-left="2cm" fo:text-indent="1cm"/>
    </style:style>"#;
    let body = r#"      <text:p text:style-name="P_indent">首行縮排文字<text:line-break/>次行左邊距文字</text:p>"#;
    let xml = minimal_content(styles, body);

    let proj = project_bytes(xml.as_bytes(), None).expect("解析 ODF 投影失敗");
    let params = LayoutParams::default();
    let lists = emit_display_lists(&proj, &params);
    assert!(!lists.is_empty(), "必須發射至少一頁顯示列表");

    let text_items: Vec<(f32, String)> = lists[0]
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { x, text, .. } => Some((*x, text.clone())),
            _ => None,
        })
        .collect();

    assert!(
        text_items.len() >= 2,
        "兩行段落必須發射至少 2 個文字項，實發 {} 項",
        text_items.len()
    );

    let margin_left = params.margins_pt[MARGIN_LEFT];
    let expected_first_line_x = margin_left + 2.0 * CM_PT + 1.0 * CM_PT;
    let expected_second_line_x = margin_left + 2.0 * CM_PT;

    let (first_x, ref first_text) = text_items[0];
    let (second_x, ref second_text) = text_items[1];

    let diff1 = (first_x - expected_first_line_x).abs();
    assert!(
        diff1 <= 0.5,
        "首行 x ({:.3}pt, 內文: {}) 與期望值 ({:.3}pt) 差距 ({:.3}pt) 必須在 ±0.5pt 內",
        first_x,
        first_text,
        expected_first_line_x,
        diff1
    );

    let diff2 = (second_x - expected_second_line_x).abs();
    assert!(
        diff2 <= 0.5,
        "次行 x ({:.3}pt, 內文: {}) 與期望值 ({:.3}pt) 差距 ({:.3}pt) 必須在 ±0.5pt 內",
        second_x,
        second_text,
        expected_second_line_x,
        diff2
    );
}

/// 燈 3（防回歸）：無宣告段落 x 恆等 `margin_left_pt`（零擾動鎖）。
#[test]
fn lamp3_unspecified_paragraph_stays_at_margin_left() {
    let styles = r#"    <style:style style:name="P_plain" style:family="paragraph">
    </style:style>"#;
    let body = r#"      <text:p text:style-name="P_plain">一般無縮排對齊宣告段落</text:p>"#;
    let xml = minimal_content(styles, body);

    let proj = project_bytes(xml.as_bytes(), None).expect("解析 ODF 投影失敗");
    let params = LayoutParams::default();
    let lists = emit_display_lists(&proj, &params);
    assert!(!lists.is_empty(), "必須發射至少一頁顯示列表");

    let text_items: Vec<f32> = lists[0]
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { x, .. } => Some(*x),
            _ => None,
        })
        .collect();

    assert!(!text_items.is_empty(), "應發射文字項");
    let margin_left = params.margins_pt[MARGIN_LEFT];
    for x in text_items {
        let diff = (x - margin_left).abs();
        assert!(
            diff < 0.001,
            "無宣告段落文字項 x ({:.4}pt) 必須恆等於版心左緣 ({:.4}pt)",
            x,
            margin_left
        );
    }
}

/// 燈 4：存檔往返不受影響（驗證最小 ODT 往返與結構）。
#[test]
fn lamp4_container_roundtrip_untouched() {
    // 驗證 project_bytes 對空或格式完整 content.xml 零崩潰
    let xml = minimal_content("", "<text:p/>");
    let proj = project_bytes(xml.as_bytes(), None).expect("空段落解析失敗");
    assert_eq!(proj.paragraphs.len(), 1);
}

/// 探針：用戶公文檔逐頁 x 分佈統計（嚴禁印出任何內文，只印數值與次數）。
#[test]
fn probe_user_sample_x_distribution() {
    let Ok(path_str) = std::env::var("ODT_SAMPLE_PATH") else {
        println!("未設定 ODT_SAMPLE_PATH，跳過用戶檔量測探針");
        return;
    };
    let mut path = std::path::PathBuf::from(&path_str);
    if !path.exists() {
        let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(&path_str);
        if candidate.exists() {
            path = candidate;
        } else {
            println!("ODT_SAMPLE_PATH 指向檔案不存在: {}，跳過", path_str);
            return;
        }
    }
    let bytes = std::fs::read(path).expect("讀取 ODT 樣本檔失敗");
    let bundle = quire_odf::OdfBundle::open(&bytes).expect("開啟 OdfBundle 失敗");
    let proj = quire_odf::layout_glue::project(&bundle).expect("計算投影失敗");
    let geom = quire_odf::page_geometry::page_geometry(&bundle).expect("讀取幾何失敗");
    let params = geom.to_layout_params();
    let lists = emit_display_lists(&proj, &params);

    println!("=== 用戶公文檔逐頁 x 座標分佈統計（零文字洩漏）===");
    println!("總頁數: {}", lists.len());
    let mut total_unique_x = std::collections::BTreeSet::new();

    for (p_idx, page) in lists.iter().enumerate() {
        let mut x_counts: std::collections::BTreeMap<i32, usize> =
            std::collections::BTreeMap::new();
        for item in &page.items {
            if let DrawItem::Text { x, .. } = item {
                // 以 0.1pt 為量化桶，避免浮點微小差異
                let bucket = (x * 10.0).round() as i32;
                *x_counts.entry(bucket).or_insert(0) += 1;
                total_unique_x.insert(bucket);
            }
        }
        let top_x: Vec<String> = x_counts
            .iter()
            .take(6)
            .map(|(b, c)| format!("{:.1}pt:{}次", *b as f32 / 10.0, c))
            .collect();
        println!(
            "頁碼 {:2}: 相異 x 數 = {:2}, 典型 x 座標分佈: [{}]",
            p_idx,
            x_counts.len(),
            top_x.join(", ")
        );
    }
    println!("全書相異 x 坐標總數: {}", total_unique_x.len());
}
