//! ODF-E2d CLI 考卷：`quire-cli odt-render` 出口必須帶頁首尾**獨立帳**欄位，
//! 且宿主計數口徑逐位不動（零污染）。
//!
//! 口徑（本卡 CLI 面）：
//! 1. `strict.odt`：`hf_page_count`＝1（LO PDF 實測單頁）、`hf_paragraph_count`＝1、
//!    `hf_text_len`＝17（頁首 `This is a header.` raw 含空白）；同時既有
//!    `paragraph_count`＝6／`total_text_len`＝65 **逐位不動**（零污染鐵閘——頁首
//!    文字若混進宿主流，此兩欄會變 7／82）。
//! 2. `tdf124637_sectionMargin.odt`：§19.501 宣告即換頁 ⇒ `hf_chunks`＝5＝
//!    `hf_page_count`，且 `hf_text_len` 含五頁頁尾字面（`I`/`II`/`3`/`2`/`3`）——
//!    證明 CLI 出口吃的是**分段後**的發射面，不是宣告面單份。
//! 3. 無頁首尾宣告的探針檔：`hf_page_count`＝頁數、`hf_paragraph_count`＝0、
//!    `hf_text_len`＝0（零發射，不是缺欄位也不是報錯）。

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

fn u64_field(j: &serde_json::Value, key: &str) -> u64 {
    j[key]
        .as_u64()
        .unwrap_or_else(|| panic!("欄位 {key} 缺失或非整數：{j}"))
}

#[test]
fn odt_render_emits_header_paragraph_with_host_counts_untouched() {
    let j = render("corpus-odt/lo_out/lo__ooxmlexport__strict.odt");
    // 頁首尾獨立帳（LO PDF 實測：單頁、頁首一段 `This is a header.`）。
    assert_eq!(u64_field(&j, "hf_page_count"), 1, "燈1-CLI 頁數");
    assert_eq!(
        u64_field(&j, "hf_paragraph_count"),
        1,
        "燈1-CLI 頁首段數（宣告面單份，非每頁重複多記）"
    );
    assert_eq!(
        u64_field(&j, "hf_text_len"),
        17,
        "燈1-CLI 頁首文字碼點（`This is a header.` 含空白）"
    );
    // 零污染鐵閘：宿主兩欄逐位＝E2d 動工前實測值。頁首若混進宿主流會變 7／82。
    assert_eq!(u64_field(&j, "paragraph_count"), 6, "燈1-CLI 宿主段數回退");
    assert_eq!(
        u64_field(&j, "total_text_len"),
        65,
        "燈1-CLI 宿主文字流污染"
    );
    assert!(
        u64_field(&j, "hf_paragraph_count") < u64_field(&j, "paragraph_count") + 10,
        "燈1-CLI 兩帳應可區分（非同一計數器）"
    );
}

#[test]
fn odt_render_reports_segmented_master_chain_footers() {
    let j = render("corpus-odt/lo_out/lo__ooxmlexport__tdf124637_sectionMargin.odt");
    let chunks = u64_field(&j, "hf_chunks");
    let pages = u64_field(&j, "hf_page_count");
    assert_eq!(chunks, 5, "燈2-CLI §19.501 段落宣告主頁 ⇒ 五個分段");
    assert_eq!(pages, chunks, "燈2-CLI 分段即分頁（宣告即換頁）");
    // 五頁頁尾字面 I(1)+II(2)+3(1)+2(1)+3(1)＝6 碼點；獨立帳非零。
    assert_eq!(
        u64_field(&j, "hf_text_len"),
        6,
        "燈2-CLI 五頁頁尾欄位快取值碼點和",
    );
    // 頁首宣告存在但內容是空段（LO 不渲染空段 ⇒ 發射面濾除），故每頁只發
    // 頁尾一段＝5，不是宣告面的 10。
    assert_eq!(
        u64_field(&j, "hf_paragraph_count"),
        5,
        "燈2-CLI 發射段數＝五頁頁尾各一（空段濾除）"
    );
}

#[test]
fn odt_render_zero_header_footer_declaration_emits_zero() {
    // 探針檔無任何 master-page 頁首尾宣告 ⇒ 三欄全 0（零發射，非缺欄、非報錯）。
    let j = render("core/odf/tests/fixtures/c4_no_page_layout.odt");
    assert!(u64_field(&j, "hf_page_count") >= 1, "燈3-CLI 至少要分一頁");
    assert_eq!(
        u64_field(&j, "hf_paragraph_count"),
        0,
        "燈3-CLI 無宣告不得發射"
    );
    assert_eq!(u64_field(&j, "hf_text_len"), 0, "燈3-CLI 無宣告文字須為 0");
    assert_eq!(u64_field(&j, "hf_skipped_left_slots"), 0);
}
