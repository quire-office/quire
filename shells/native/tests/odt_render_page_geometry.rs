//! ODF-C4 CLI 考卷：`quire-cli odt-render` 出口必須帶真實頁面幾何，不再是
//! `LayoutParams::default()` 的假 A4（595×842）＋1 吋邊距（72pt）。
//!
//! 口徑（ODF-C4 卡燈 1 的 CLI 面）：
//! 1. `sample3_page_sect.odt`：JSON `page.w_pt/h_pt` = styles.xml `Mpm1`
//!    （21cm×29.7cm）換算值（±0.5pt）＝ LO PDF 真值 595.304×841.890，
//!    且邊距四邊 56.69pt（2.0cm）≠ 72pt（1 吋假值）。
//! 2. `page_source` 欄位存在且為 `default_master_styles`（鑑別力：證明走了
//!    master-page 鏈而非回退）。
//! 3. 無 page-layout 探針檔：`page_source = master_page_without_layout`＋
//!    回退值仍是 LO 口徑 A4（595.304，非 default 的 595.0）——回退也得是
//!    「實測過的 LO 預設」，不是 `LayoutParams::default()`。

use std::process::Command;

fn render(rel: &str) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel);
    let out = Command::new(env!("CARGO_BIN_EXE_quire-cli"))
        .arg("odt-render")
        .arg(&path)
        .output()
        .expect("執行 quire-cli odt-render");
    assert!(
        out.status.success(),
        "{rel} 應成功，stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout 應為 JSON")
}

#[test]
fn odt_render_reports_real_page_geometry() {
    let j = render("docs/odf-design/sample3_page_sect.odt");
    let w = j["page"]["w_pt"].as_f64().expect("page.w_pt");
    let h = j["page"]["h_pt"].as_f64().expect("page.h_pt");
    let m: Vec<f64> = j["page"]["margins_pt"]
        .as_array()
        .expect("margins_pt 陣列")
        .iter()
        .map(|v| v.as_f64().expect("邊距應為數值"))
        .collect();
    assert_eq!(m.len(), 4);
    // Mpm1 = 21cm × 29.7cm，四邊 2.0cm（LO PDF 真值 595.304×841.890）。
    assert!((w - 595.304).abs() <= 0.5, "燈1-CLI 頁寬 {w}");
    assert!((h - 841.890).abs() <= 0.5, "燈1-CLI 頁高 {h}");
    for (i, v) in m.iter().enumerate() {
        assert!((v - 56.6929).abs() <= 0.5, "燈1-CLI 邊距[{i}] {v}");
        assert!(
            (v - 72.0).abs() > 1.0,
            "燈1-CLI 鑑別力：仍是 default 1 吋假值"
        );
    }
    assert!(
        (w - 595.0).abs() > 0.1,
        "燈1-CLI 鑑別力：頁寬仍是 default 假值 595"
    );
    assert_eq!(j["page_source"].as_str(), Some("default_master_styles"));
    // 零回退鐵閘：既有欄位仍在且語意不動。
    assert_eq!(j["paragraph_count"].as_u64(), Some(5));
    assert!(j["total_text_len"].as_u64().unwrap() > 0);
    assert!(j["page_breaks"].is_array());
}

#[test]
fn odt_render_fallback_is_lo_default_not_layout_default() {
    let j = render("core/odf/tests/fixtures/c4_no_page_layout.odt");
    assert_eq!(
        j["page_source"].as_str(),
        Some("master_page_without_layout")
    );
    let w = j["page"]["w_pt"].as_f64().expect("page.w_pt");
    let m: Vec<f64> = j["page"]["margins_pt"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    // LO 口徑 A4 = 21.001cm = 595.3031pt ≠ LayoutParams::default() 的 595.0。
    assert!((w - 595.3031).abs() <= 0.5, "燈3-CLI 回退頁寬 {w}");
    assert!(
        (w - 595.0).abs() > 0.1,
        "燈3-CLI 鑑別力：回退值仍是 default 595 假值"
    );
    for v in &m {
        assert!(
            (v - 56.6929).abs() <= 0.5,
            "燈3-CLI 回退邊距 {v}（LO 實測 56.80±0.5）"
        );
    }
}
