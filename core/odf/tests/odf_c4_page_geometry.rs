//! ODF-C4 紅燈考卷：ODT 頁面幾何鏈——`style:page-layout`＋`style:master-page`
//! 消費，odt-render 脫離 `LayoutParams::default()`（含分域自動樣式鐵律）。
//!
//! 考卷口徑（紅燈 commit 先拍紅進 `docs/odf-c4-redlight.md`，數值與計數基準
//! 腳本 `core/odf/tests/odf_c4_truth_baseline.py`＋LO PDF 真值一起進 commit，
//! 不得事後補）：
//!
//! - 燈 1：`sample3_page_sect.odt` 經 `page_geometry` 的頁寬高／邊距 =
//!   styles.xml `Mpm1` 換算值（±0.5pt）且 **≠** `LayoutParams::default()` 的
//!   假 A4（595×842）＋1 吋邊距（72pt）——鑑別力：證明真讀到。
//! - 燈 2：雙軸分歧探針（合成 fixture `c4_dual_axis.odt`，`style:` 軸與
//!   `fo:` 軸故意差 1cm）取 **`style:` 軸優先**；同檔 LO PDF 真值（566.957×
//!   793.701 = fo: 軸）同表列帳——LO 寫端只寫 fo: 軸、讀端按卡面政策讀 style:
//!   軸，兩者對「合成檔」本就不一致，如實記帳（解剖報告 §3）。
//! - 燈 3：無 page-layout 檔（合成 fixture `c4_no_page_layout.odt`：master-page
//!   指向不存在的版面名）回退 **LO 預設頁**，數值以 LO PDF 真值實測鎖死
//!   （595.304×841.890、文字左緣 56.80pt ⇒ 邊距 2.0cm 非 1 吋）。
//! - 燈 5：分域鐵律——content.xml 域與 styles.xml 域同名自動樣式（`P1`）宣告
//!   不同 master-page，解析必須各認各的域（content 域優先），不得混表。
//! - 燈 6：語料 50 檔＋3 樣本逐檔與基準腳本預期（=LO PDF 真值 ±0.5pt）一致。
//!
//! 燈 4（零回退）＝既有考卷（odt_layout_projection 7 盞＋odf_container 5 盞）
//! 全綠不動，由 `cargo test -p quire-odf` 整體閘門保證，不在本檔重複。

use quire_odf::page_geometry::{
    odf_length_to_pt, page_geometry, page_geometry_bytes, GeometrySource, LayoutParams,
    Orientation, StyleDomainPage, LO_DEFAULT_MARGIN_PT, LO_DEFAULT_PAGE_H_PT, LO_DEFAULT_PAGE_W_PT,
};
use quire_odf::OdfBundle;
use std::path::{Path, PathBuf};

const TOL: f32 = 0.5;

/// cm → pt：以 1in=2.54cm、1pt=1/72in 的精確定義獨立寫出（不 import 實作的
/// `CM_TO_PT`，避免斷言基準與實作同源）。
const CM: f32 = 72.0 / 2.54;

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() <= TOL
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-odt")
}

fn repo_file(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn geom_of(rel: &str) -> quire_odf::page_geometry::OdfPageGeometry {
    let bytes = std::fs::read(repo_file(rel)).unwrap_or_else(|e| panic!("讀取 {rel} 失敗: {e}"));
    let bundle = OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("開封 {rel} 失敗: {e}"));
    page_geometry(&bundle).unwrap_or_else(|e| panic!("頁面幾何解析 {rel} 失敗: {e}"))
}

// ---------------------------------------------------------------------------
// 燈 1：sample3_page_sect.odt 真讀到 Mpm1（21cm×29.7cm＋2.0cm 四邊），
// 且不同於 LayoutParams::default() 假值。
// ---------------------------------------------------------------------------

#[test]
fn lamp1_sample3_reads_page_layout_not_default() {
    let g = geom_of("docs/odf-design/sample3_page_sect.odt");
    // styles.xml：Mpm1 = 21cm × 29.7cm，fo:margin-* 四邊 2.0cm。
    assert!(
        approx(g.page_w_pt(), 21.0 * CM),
        "燈1 頁寬 {}",
        g.page_w_pt()
    );
    assert!(
        approx(g.page_h_pt(), 29.7 * CM),
        "燈1 頁高 {}",
        g.page_h_pt()
    );
    for (i, m) in g.margins_pt().iter().enumerate() {
        assert!(approx(*m, 2.0 * CM), "燈1 邊距[{i}] {m}");
    }
    assert_eq!(g.master_page_name.as_deref(), Some("Standard"));
    assert_eq!(g.page_layout_name.as_deref(), Some("Mpm1"));
    assert_eq!(g.source, GeometrySource::DefaultMasterStyles);
    // 鑑別力：≠ LayoutParams::default() 假 A4＋1 吋。
    let d = LayoutParams::default();
    assert!(
        (g.page_w_pt() - d.page_w_pt).abs() > 0.1,
        "燈1 鑑別力：頁寬仍是 default 假值"
    );
    assert!(
        g.margins_pt().iter().all(|m| (*m - 72.0).abs() > 1.0),
        "燈1 鑑別力：邊距仍是 default 1 吋假值 {:?}",
        g.margins_pt()
    );
    // LO PDF 真值（soffice --convert-to pdf＋pymupdf page.rect）=
    // 595.304 × 841.890——與換算值差 <0.5pt。
    assert!(
        approx(g.page_w_pt(), 595.304) && approx(g.page_h_pt(), 841.890),
        "燈1 對 LO 真值失焦 {}×{}",
        g.page_w_pt(),
        g.page_h_pt()
    );
    // 投影鏈消費端：to_layout_params 只動頁面幾何軸，字級/行距零回退。
    let p = g.to_layout_params();
    assert_eq!(p.font_size_pt, 12.0);
    assert_eq!(p.line_spacing, 1.4);
}

// ---------------------------------------------------------------------------
// 燈 2：雙軸分歧探針取 style: 軸優先。
// ---------------------------------------------------------------------------

#[test]
fn lamp2_dual_axis_prefers_style_axis() {
    // c4_dual_axis.odt：style:paper-width/height = 21cm×29cm；
    // fo:page-width/height = 20cm×28cm（故意差 1cm）。
    let g = geom_of("core/odf/tests/fixtures/c4_dual_axis.odt");
    let l = g.layout.as_ref().expect("燈2 應解到 Mpm1");
    assert!(l.size_from_style_axis, "燈2 取鏈旗標應為 style: 軸");
    assert!(
        approx(g.page_w_pt(), 21.0 * CM),
        "燈2 應取 style: 軸 21cm，實際 {}",
        g.page_w_pt()
    );
    assert!(
        approx(g.page_h_pt(), 29.0 * CM),
        "燈2 應取 style: 軸 29cm，實際 {}",
        g.page_h_pt()
    );
    assert!(!approx(g.page_w_pt(), 20.0 * CM), "燈2 不得取 fo: 軸寬");
    assert!(!approx(g.page_h_pt(), 28.0 * CM), "燈2 不得取 fo: 軸高");
    // fo: 軸後備路徑：把 style: 軸拿掉（合成 XML 直打純函數入口）。
    let content = br#"<?xml version="1.0"?><office:text xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"><text:p/></office:text>"#;
    let styles = br#"<?xml version="1.0"?>
<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
 xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
 <office:automatic-styles>
  <style:page-layout style:name="Mpm1">
   <style:page-layout-properties fo:page-width="20.00cm" fo:page-height="28.00cm"
    fo:margin-top="3.00cm" fo:margin-bottom="3.00cm"
    fo:margin-left="3.50cm" fo:margin-right="3.50cm"/>
  </style:page-layout>
 </office:automatic-styles>
 <office:master-styles><style:master-page style:name="Standard" style:page-layout-name="Mpm1"/></office:master-styles>
</office:document-styles>"#;
    let g2 = page_geometry_bytes(content, Some(styles)).expect("燈2 fo: 後備解析");
    assert!(
        approx(g2.page_w_pt(), 20.0 * CM),
        "燈2 fo: 後備應 20cm，實際 {}",
        g2.page_w_pt()
    );
    let l2 = g2.layout.as_ref().expect("燈2 fo 後備有版面");
    assert!(!l2.size_from_style_axis, "燈2 取鏈旗標應為 fo: 軸");
    // LO PDF 真值列帳（非斷言對象——LO 寫端/讀端口徑差，解剖報告 §3）：
    // c4_dual_axis.odt 經 LO 渲 fo: 軸 = 566.957 × 793.701。我方按卡面
    // 政策取 style: 軸 = 595.276 × 822.047；語料真實檔無雙軸分歧（style:
    // 軸 0 檔），故此分歧僅存在於合成探針，不影響燈 6 全語料對齊。
}

// ---------------------------------------------------------------------------
// 燈 3：無 page-layout 檔回退 LO 預設頁（數值以 LO PDF 真值實測鎖死）。
// ---------------------------------------------------------------------------

#[test]
fn lamp3_missing_page_layout_falls_back_to_lo_default() {
    // c4_no_page_layout.odt：master-page Standard 指向 NoSuchLayout，
    // automatic-styles 域無任何 page-layout。
    let g = geom_of("core/odf/tests/fixtures/c4_no_page_layout.odt");
    assert!(g.layout.is_none(), "燈3 不得解到版面");
    assert_eq!(g.source, GeometrySource::MasterPageWithoutLayout);
    assert!(g.is_lo_default());
    // LO PDF 真值：頁 595.304 × 841.890；文字左緣 x0=56.80pt（pymupdf 量化
    // ±0.11）⇒ 邊距 2.0cm（56.6929pt），**不是** default 的 1 吋（72pt）。
    assert!(
        approx(g.page_w_pt(), 595.304),
        "燈3 回退頁寬 {}",
        g.page_w_pt()
    );
    assert!(
        approx(g.page_h_pt(), 841.890),
        "燈3 回退頁高 {}",
        g.page_h_pt()
    );
    assert!(
        approx(g.page_w_pt(), LO_DEFAULT_PAGE_W_PT) && approx(g.page_h_pt(), LO_DEFAULT_PAGE_H_PT),
        "燈3 常數與實測失焦"
    );
    for m in g.margins_pt() {
        // 同時咬死「我方常數」與「LO 實測左緣」：兩值差 0.107pt，須互落在
        // ±0.5 內（單邊斷言會放過常數漂移）。
        assert!(
            approx(m, LO_DEFAULT_MARGIN_PT) && approx(m, 56.80),
            "燈3 回退邊距 {m} 須同時 = LO_DEFAULT_MARGIN_PT 且落在 LO 實測 56.80pt ±0.5"
        );
        assert!(
            (m - 72.0).abs() > 1.0,
            "燈3 鑑別力：回退值不得是 default 1 吋"
        );
    }
    // 完全無 styles.xml 亦回退同值（探針：直接餵 None）。
    let g2 = page_geometry_bytes(
        br#"<?xml version="1.0"?><office:text xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"><text:p/></office:text>"#,
        None,
    )
    .expect("燈3 無 styles.xml 亦須 Ok");
    assert_eq!(g2.source, GeometrySource::NoMasterPage);
    assert!(approx(g2.page_w_pt(), LO_DEFAULT_PAGE_W_PT));
}

// ---------------------------------------------------------------------------
// 燈 5：分域鐵律（Part 3 §3.15.3）——content 域與 styles 域同名樣式 P1
// 各認各的 master-page，content 域優先。
// ---------------------------------------------------------------------------

#[test]
fn lamp5_style_domains_are_split() {
    let content = br#"<?xml version="1.0"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
 <office:automatic-styles>
  <style:style style:name="P1" style:family="paragraph" style:master-page-name="LandPage">
   <style:paragraph-properties/>
  </style:style>
 </office:automatic-styles>
 <office:body><office:text><text:p text:style-name="P1">x</text:p></office:text></office:body>
</office:document-content>"#;
    let styles = br#"<?xml version="1.0"?>
<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
 xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
 <office:automatic-styles>
  <style:page-layout style:name="LPm1">
   <style:page-layout-properties fo:page-width="29.7cm" fo:page-height="21.0cm"
    fo:margin-top="1cm" fo:margin-bottom="1cm" fo:margin-left="1cm" fo:margin-right="1cm"/>
  </style:page-layout>
  <style:style style:name="P1" style:family="paragraph" style:master-page-name="StdPage"/>
 </office:automatic-styles>
 <office:master-styles>
  <style:master-page style:name="StdPage" style:page-layout-name="MpmStd"/>
  <style:master-page style:name="LandPage" style:page-layout-name="LPm1"/>
 </office:master-styles>
</office:document-styles>"#;
    // content 域 P1 → LandPage（橫向 LPm1）；styles 域 P1 → StdPage（查無
    // MpmStd）。混表者必誤取 StdPage → 回退 LO 直向預設頁，被下面斷言打紅。
    let g = page_geometry_bytes(content, Some(styles)).expect("燈5 解析");
    assert_eq!(
        g.master_page_name.as_deref(),
        Some("LandPage"),
        "燈5 域優先序錯誤"
    );
    assert_eq!(g.source, GeometrySource::ParaMasterPageName);
    assert!(
        approx(g.page_w_pt(), 29.7 * CM),
        "燈5 橫向頁寬 {}",
        g.page_w_pt()
    );
    assert!(approx(g.page_h_pt(), 21.0 * CM));
    // 採集面：兩域各自入表＋多 master-page 記帳（本切片只取文件級預設）。
    let bytes = std::fs::read(repo_file("core/odf/tests/fixtures/c4_dual_axis.odt")).unwrap();
    let bundle = OdfBundle::open(&bytes).unwrap();
    let _ = &bundle;
    // unparsed_master_pages：燈5 合成檔有 2 個 master-page，取 1 記 1。
    assert_eq!(g.unparsed_master_pages, 1, "燈5 多 master-page 記帳");
    // Orientation 只採集不使用：LPm1 無宣告 orientation 屬性 → Unspecified。
    let l = g.layout.as_ref().unwrap();
    assert_eq!(l.print_orientation, Orientation::Unspecified);
    assert_eq!(l.fo_orientation, Orientation::Unspecified);
    // StyleDomainPage 是分域結構：type 層面與 layout_glue::StyleBook 無關聯
    // （編譯期即分離；這裡確認兩張 master-page 表獨立存在）。
    let dom = StyleDomainPage::default();
    assert!(dom.mp_by_content_style.is_empty() && dom.mp_by_styles_style.is_empty());
}

// ---------------------------------------------------------------------------
// 燈 6：語料 50 檔＋3 樣本逐檔對齊基準腳本（=LO PDF 真值 ±0.5pt）。
// 基準：core/odf/tests/odf_c4_truth_baseline.py（紅燈 commit 同入，事後不得改）。
// ---------------------------------------------------------------------------

/// (檔名相對於 corpus-odt/ 或 docs/odf-design/, 頁寬pt, 頁高pt, [上,右,下,左]邊距pt, 生效原因, 加密)
type ExpectedRow = (&'static str, f32, f32, [f32; 4], &'static str, bool);

const EXPECTED: [ExpectedRow; 51] = [
    (
        "ext/lo_core_sw_qa_odfexport_WordTest_edit.odt",
        612.0000,
        792.0000,
        [72.0000, 72.0000, 72.0000, 72.0000],
        "para_master_page_name",
        false,
    ),
    (
        "ext/lo_core_sw_qa_odfexport_embedded-pdf.odt",
        612.0000,
        792.0000,
        [56.6929, 56.6929, 56.6929, 56.6929],
        "default_master_styles",
        false,
    ),
    (
        "ext/lo_core_sw_qa_odfexport_tdf140437.odt",
        595.3032,
        841.8888,
        [56.6928, 56.6928, 56.6928, 56.6928],
        "default_master_styles",
        false,
    ),
    (
        "ext/lo_core_sw_qa_odfexport_tdf152710.odt",
        595.2472,
        841.8614,
        [23.3575, 40.2520, 31.0110, 33.3638],
        "default_master_styles",
        false,
    ),
    (
        "ext/lo_core_sw_qa_odfexport_tdf77961.odt",
        595.3039,
        841.8898,
        [56.6929, 56.6929, 56.6929, 56.6929],
        "default_master_styles",
        false,
    ),
    (
        "ext/lo_core_sw_qa_odfimport_feature_table_merged-cells.odt",
        595.3039,
        841.8898,
        [56.6929, 56.6929, 56.6929, 56.6929],
        "default_master_styles",
        false,
    ),
    (
        "ext/lo_core_sw_qa_odfimport_ooo32780-1.odt",
        595.2472,
        841.8614,
        [56.6929, 56.6929, 56.6929, 56.6929],
        "default_master_styles",
        false,
    ),
    (
        "ext/oasis_opendocument_v1p3_os_part1_introduction.odt",
        612.0000,
        792.0000,
        [72.0000, 89.8583, 36.0000, 89.8583],
        "para_master_page_name",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__FDO76312.odt",
        595.3039,
        841.8898,
        [35.4614, 36.0000, 20.1543, 36.0000],
        "para_master_page_name",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__FDO78292.odt",
        612.0000,
        792.0000,
        [72.0000, 72.0000, 36.0000, 72.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__fdo69616.odt",
        612.0000,
        792.0000,
        [36.0000, 72.0000, 36.0000, 72.0000],
        "para_master_page_name",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__fdo76098.odt",
        595.3039,
        841.8898,
        [72.0000, 89.8583, 72.0000, 89.8583],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__fdo77887.odt",
        612.0000,
        936.0000,
        [70.8945, 85.0394, 70.8945, 85.0394],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__fdo78420.odt",
        595.3039,
        841.8898,
        [56.6929, 56.6929, 56.6929, 56.6929],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__lvlPicBulletId.odt",
        612.0000,
        792.0000,
        [36.0000, 72.0000, 36.0000, 72.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__mce-nested.odt",
        612.0000,
        792.0000,
        [36.0000, 72.0000, 36.0000, 72.0000],
        "para_master_page_name",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__strict.odt",
        612.0000,
        792.0000,
        [36.0000, 72.0000, 72.0000, 72.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__table-row-data-displayed-twice.odt",
        612.0000,
        792.0000,
        [48.2457, 90.0000, 72.0000, 90.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__tdf102466.odt",
        595.3039,
        841.8898,
        [35.4047, 85.0394, 70.8378, 85.0394],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__tdf104061_tableSectionColumns.odt",
        841.8898,
        595.3039,
        [9.0425, 28.3465, 14.2016, 28.3465],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__tdf105688.odt",
        612.0000,
        792.0000,
        [56.6929, 56.6929, 56.6929, 56.6929],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__tdf123054.odt",
        595.3039,
        841.8898,
        [70.8378, 70.8378, 70.8378, 70.8378],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__tdf123705.odt",
        612.0000,
        792.0000,
        [9.3543, 18.0000, 9.3543, 18.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlexport__tdf124637_sectionMargin.odt",
        612.0000,
        792.0000,
        [36.0000, 70.8378, 36.0000, 70.8378],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__ooxmlimport__tdf87533_bidi.odt",
        595.3039,
        841.8898,
        [72.0000, 90.0000, 72.0000, 90.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__phpword__samples_resources_Sample_11_ReadWord2007.odt",
        595.3039,
        841.8898,
        [36.0000, 70.8945, 36.0000, 70.8945],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__phpword__tests_PhpWordTests__files_documents_reader-styles.odt",
        595.3039,
        841.8898,
        [36.0000, 70.8945, 36.0000, 70.8945],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__phpword__tests_PhpWordTests__files_documents_reader.odt",
        595.3039,
        841.8898,
        [36.0000, 70.8945, 36.0000, 70.8945],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__poi__test-data_document_Bug54849.odt",
        612.0000,
        792.0000,
        [36.0000, 72.0000, 36.0000, 72.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__poi__test-data_document_WordWithAttachments.odt",
        595.3039,
        841.8898,
        [35.4047, 42.4913, 56.6929, 85.0394],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__poi__test-data_document_bug57031.odt",
        612.0000,
        792.0000,
        [36.0000, 72.0000, 36.0000, 72.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__poi__test-data_document_bug59058.odt",
        612.0000,
        792.0000,
        [72.0000, 72.0000, 72.0000, 72.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__poi__test-data_document_bug65649.odt",
        595.3039,
        841.8898,
        [56.6929, 42.4913, 56.6929, 85.0394],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__poi__test-data_document_checkboxes.odt",
        612.0000,
        792.0000,
        [36.0000, 90.0000, 36.0000, 90.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__poi__test-data_document_drawing.odt",
        595.3039,
        841.8898,
        [56.6929, 42.4913, 56.6929, 85.0394],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__poi__test-data_document_form_footnotes.odt",
        612.0000,
        792.0000,
        [36.0000, 18.0000, 43.2000, 50.4000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__selfgen__altchunk_customxml.odt",
        595.3039,
        841.8898,
        [72.0000, 72.0000, 72.0000, 72.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__selfgen__dropcap_toc_fields.odt",
        595.3039,
        841.8898,
        [72.0000, 72.0000, 72.0000, 72.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__selfgen__embedded_font_odttf.odt",
        595.3039,
        841.8898,
        [72.0000, 72.0000, 72.0000, 72.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_ChartUpdater01_Chart-Embedded-Xlsx-08.odt",
        595.3039,
        841.8898,
        [72.0000, 72.0000, 72.0000, 72.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.odt",
        595.3039,
        841.8898,
        [72.0000, 90.0000, 72.0000, 90.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.odt",
        595.3039,
        841.8898,
        [72.0000, 90.0000, 72.0000, 90.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Pluto.odt",
        595.3039,
        841.8898,
        [72.0000, 90.0000, 72.0000, 90.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Saturn.odt",
        595.3039,
        841.8898,
        [72.0000, 90.0000, 72.0000, 90.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_SolarOverview.odt",
        612.0000,
        792.0000,
        [72.0000, 90.0000, 72.0000, 90.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Sun.odt",
        595.3039,
        841.8898,
        [72.0000, 90.0000, 72.0000, 90.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_HtmlConverter01_Test-03.odt",
        612.0000,
        792.0000,
        [43.2000, 54.0000, 36.0000, 54.0000],
        "default_master_styles",
        false,
    ),
    (
        "lo_out/lo__xopt__TestFiles_HC031-Complicated-Document.odt",
        612.0000,
        792.0000,
        [36.0000, 72.0000, 36.0000, 72.0000],
        "default_master_styles",
        false,
    ),
    (
        "sample3_page_sect.odt",
        595.3039,
        841.8898,
        [56.6929, 56.6929, 56.6929, 56.6929],
        "default_master_styles",
        false,
    ),
    (
        "sample1_styles_list.odt",
        595.3039,
        841.8898,
        [56.6929, 56.6929, 56.6929, 56.6929],
        "default_master_styles",
        false,
    ),
    (
        "sample2_table_frame.odt",
        595.3039,
        841.8898,
        [56.6929, 56.6929, 56.6929, 56.6929],
        "default_master_styles",
        false,
    ),
];

#[test]
fn lamp6_corpus_matches_truth_baseline() {
    let mut checked = 0usize;
    let mut fallback = 0usize;
    for (rel, w, h, m, src, encrypted) in EXPECTED {
        let path = if rel.starts_with("sample") {
            repo_file(&format!("docs/odf-design/{rel}"))
        } else {
            corpus_root().join(rel)
        };
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("讀取 {rel}: {e}"));
        let bundle = OdfBundle::open(&bytes).expect("開封");
        let g = match page_geometry(&bundle) {
            Err(e) => {
                // EXPECTED 51 列全為明文檔（加密 2 檔在基準生成期即 XML 解析
                // 失敗、不入表；其「必須報錯」由 odf_encrypted_gate 考卷守）。
                panic!("燈6 {rel}：明文檔不得報錯: {e}");
            }
            Ok(g) => {
                assert!(!encrypted, "燈6 {rel}：加密檔竟解析成功");
                g
            }
        };
        assert_eq!(g.source.as_str(), src, "燈6 {rel} 生效原因");
        assert!(
            approx(g.page_w_pt(), w),
            "燈6 {rel} 頁寬 {} ≠ {w}",
            g.page_w_pt()
        );
        assert!(
            approx(g.page_h_pt(), h),
            "燈6 {rel} 頁高 {} ≠ {h}",
            g.page_h_pt()
        );
        for (got, want) in g.margins_pt().iter().zip(m.iter()) {
            assert!(approx(*got, *want), "燈6 {rel} 邊距 {got} ≠ {want}");
        }
        checked += 1;
        if g.is_lo_default() {
            fallback += 1;
        }
    }
    assert_eq!(
        checked, 51,
        "燈6 應逐檔核對 51 明文檔（48 語料明文＋3 樣本；加密 2 檔在基準腳本生成期\n         即 XML 解析失敗、不入表，其「必須報錯」由 odf_encrypted_gate 考卷守）"
    );
    // 記帳：本切片回退 LO 預設頁的明文檔數（語料實態 0——50 檔全有 page-layout）。
    assert_eq!(fallback, 0, "燈6 語料回退檔數須與解剖報告一致（0）");
}

// ---------------------------------------------------------------------------
// 單位換算（映射表單元；解剖報告 §4）
// ---------------------------------------------------------------------------

#[test]
fn length_units_and_decimal_comma() {
    assert!(approx(odf_length_to_pt("21cm").unwrap(), 21.0 * CM));
    assert!(approx(odf_length_to_pt("210mm").unwrap(), 21.0 * CM));
    assert!(approx(odf_length_to_pt("8.5in").unwrap(), 612.0));
    assert!(approx(odf_length_to_pt("12pt").unwrap(), 12.0));
    assert!(approx(odf_length_to_pt("1pc").unwrap(), 12.0));
    assert!(approx(odf_length_to_pt("2,0cm").unwrap(), 2.0 * CM));
    assert_eq!(odf_length_to_pt("100%"), None, "百分比不做單位猜測");
    assert_eq!(odf_length_to_pt("21"), None, "無單位不入 pt");
    assert_eq!(odf_length_to_pt("0cm"), None, "非正數不入列（走後備鏈）");
    assert_eq!(odf_length_to_pt("-3cm"), None);
}
