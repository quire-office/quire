//! ODF-C3 紅燈考卷：ODT 文字流投影（text:p/text:h＋字級級聯）→ layout 首切片。
//!
//! 考卷口徑（紅燈 commit 先拍紅，數值與計數基準腳本
//! `core/odf/tests/odt_projection_baseline.py` 一起進 commit，不得事後補）：
//!
//! (a) 50 檔投影全跑：合法檔必須 `Ok`；3 個 mimetype 違例檔必須落在明確歸類
//!     （比照 C2 的 `MimetypeStatus`），禁止 catch-continue 假綠；段落總數
//!     （text:p＋text:h）逐檔與 XML 原始計數一致（見 `EXPECTED_COUNTS`，
//!     總數 19687＋248＝19935）。
//! (b) 樣式級聯硬斷言：兩個含顯式非 12pt 字級的語料檔（皆非 fdo 轉出樣本），
//!     寫死「該檔第 N 段字級 == XML 實際宣告值」的單段斷言。禁止命中率
//!     百分比斷言。
//! (c) `text:h` 入列斷言：含 `<text:h` 的語料檔，投影段落數 > 僅 text:p 計數，
//!     且 heading 文字出現在投影輸出。
//!
//! 計數口徑（與 `scripts/odf_survey.py`／survey.json 同）：`<text:p[\s>/]` 與
//! `<text:h[\s>/]` 正則計數（Start＋Empty 皆計），巢裝（表格格／文字框／
//! tracked-changes）一律入列。

use quire_odf::layout_glue::{
    layout_projection, page_break_positions, project, project_bytes, LayoutParams,
    DEFAULT_FONT_SIZE_PT,
};
use quire_odf::{MimetypeStatus, OdfBundle};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-odt")
}

fn manifest_files() -> Vec<String> {
    let p = corpus_root().join("manifest.json");
    let content = std::fs::read_to_string(&p).expect("讀取 manifest.json 失敗");
    let json: Value = serde_json::from_str(&content).expect("解析 manifest.json 失敗");
    let files = json["files"]
        .as_array()
        .expect("files 欄位應為陣列")
        .iter()
        .map(|f| f["file"].as_str().expect("file 欄位應為字串").to_string())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 50, "語料庫應恰有 50 檔");
    files
}

fn read_file(rel: &str) -> Vec<u8> {
    let path = corpus_root().join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("讀取檔案失敗 {rel}: {e}"))
}

/// 逐檔預期計數（text_p, text_h）——計數基準：
/// `core/odf/tests/odt_projection_baseline.py`（regex 口徑），並與
/// `docs/odf-survey/survey.json` 的 content.text_p/text_h 逐檔交叉核對一致
/// （紅燈 commit 同入，事後不得改）。
const EXPECTED_COUNTS: [(&str, usize, usize); 50] = [
    ("ext/lo_core_sw_qa_odfexport_WordTest_edit.odt", 13, 0),
    ("ext/lo_core_sw_qa_odfexport_embedded-pdf.odt", 1, 0),
    ("ext/lo_core_sw_qa_odfexport_sha1_correct.odt", 0, 0),
    ("ext/lo_core_sw_qa_odfexport_sha1_wrong.odt", 0, 0),
    ("ext/lo_core_sw_qa_odfexport_tdf140437.odt", 4, 0),
    ("ext/lo_core_sw_qa_odfexport_tdf152710.odt", 48, 1),
    ("ext/lo_core_sw_qa_odfexport_tdf77961.odt", 7, 0),
    (
        "ext/lo_core_sw_qa_odfimport_feature_table_merged-cells.odt",
        4,
        0,
    ),
    ("ext/lo_core_sw_qa_odfimport_ooo32780-1.odt", 32, 0),
    (
        "ext/oasis_opendocument_v1p3_os_part1_introduction.odt",
        177,
        7,
    ),
    ("lo_out/lo__ooxmlexport__FDO76312.odt", 25, 1),
    ("lo_out/lo__ooxmlexport__FDO78292.odt", 62, 2),
    ("lo_out/lo__ooxmlexport__fdo69616.odt", 4, 0),
    ("lo_out/lo__ooxmlexport__fdo76098.odt", 46, 0),
    ("lo_out/lo__ooxmlexport__fdo77887.odt", 181, 0),
    ("lo_out/lo__ooxmlexport__fdo78420.odt", 305, 0),
    ("lo_out/lo__ooxmlexport__lvlPicBulletId.odt", 16, 1),
    ("lo_out/lo__ooxmlexport__mce-nested.odt", 8, 0),
    ("lo_out/lo__ooxmlexport__strict.odt", 6, 0),
    (
        "lo_out/lo__ooxmlexport__table-row-data-displayed-twice.odt",
        57,
        0,
    ),
    ("lo_out/lo__ooxmlexport__tdf102466.odt", 431, 25),
    (
        "lo_out/lo__ooxmlexport__tdf104061_tableSectionColumns.odt",
        322,
        47,
    ),
    ("lo_out/lo__ooxmlexport__tdf105688.odt", 22, 0),
    ("lo_out/lo__ooxmlexport__tdf123054.odt", 97, 0),
    ("lo_out/lo__ooxmlexport__tdf123705.odt", 305, 0),
    ("lo_out/lo__ooxmlexport__tdf124637_sectionMargin.odt", 26, 4),
    ("lo_out/lo__ooxmlimport__tdf87533_bidi.odt", 6, 0),
    (
        "lo_out/lo__phpword__samples_resources_Sample_11_ReadWord2007.odt",
        28,
        4,
    ),
    (
        "lo_out/lo__phpword__tests_PhpWordTests__files_documents_reader-styles.odt",
        38,
        6,
    ),
    (
        "lo_out/lo__phpword__tests_PhpWordTests__files_documents_reader.odt",
        35,
        4,
    ),
    ("lo_out/lo__poi__test-data_document_Bug54849.odt", 45, 0),
    (
        "lo_out/lo__poi__test-data_document_WordWithAttachments.odt",
        61,
        0,
    ),
    ("lo_out/lo__poi__test-data_document_bug57031.odt", 1353, 0),
    ("lo_out/lo__poi__test-data_document_bug59058.odt", 935, 18),
    ("lo_out/lo__poi__test-data_document_bug65649.odt", 14161, 60),
    ("lo_out/lo__poi__test-data_document_checkboxes.odt", 16, 0),
    ("lo_out/lo__poi__test-data_document_drawing.odt", 214, 19),
    (
        "lo_out/lo__poi__test-data_document_form_footnotes.odt",
        211,
        9,
    ),
    ("lo_out/lo__selfgen__altchunk_customxml.odt", 1, 1),
    ("lo_out/lo__selfgen__dropcap_toc_fields.odt", 2, 2),
    ("lo_out/lo__selfgen__embedded_font_odttf.odt", 1, 0),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_ChartUpdater01_Chart-Embedded-Xlsx-08.odt",
        1,
        0,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.odt",
        13,
        5,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.odt",
        12,
        5,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Pluto.odt",
        12,
        6,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Saturn.odt",
        15,
        6,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_SolarOverview.odt",
        61,
        1,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Sun.odt",
        18,
        4,
    ),
    (
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_HtmlConverter01_Test-03.odt",
        108,
        0,
    ),
    (
        "lo_out/lo__xopt__TestFiles_HC031-Complicated-Document.odt",
        141,
        10,
    ),
];

/// mimetype 違例三檔（比照 C2 考卷 3：皆「非首條目」違例）。
const MIMETYPE_VIOLATIONS: [&str; 3] = [
    "ext/lo_core_sw_qa_odfexport_tdf140437.odt",
    "ext/lo_core_sw_qa_odfexport_tdf152710.odt",
    "ext/lo_core_sw_qa_odfexport_tdf77961.odt",
];

/// 考卷 (a)：50 檔投影全跑——合法檔 `Ok`＋mimetype 明確歸類＋逐檔段落計數
/// （text:p＋text:h）與 XML 原始計數一致。禁止 catch-continue：違例三檔
/// 必須落在 `MimetypeStatus` 的「非首條目」變體，其餘 47 檔必須 Compliant。
#[test]
fn a_projection_over_corpus_50() {
    let files = manifest_files();
    let mut total_paragraphs = 0usize;
    let mut violation_hits = 0usize;

    for (rel, exp_p, exp_h) in EXPECTED_COUNTS {
        assert!(files.iter().any(|f| f == rel), "{rel} 應在 manifest 清單內");
        let bytes = read_file(rel);
        // 開啟失敗＝直接 panic（不吞錯、不 continue）。
        let bundle = OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("{rel} 開啟失敗: {e}"));

        // mimetype 明確歸類（比照 C2）：違例三檔非首條目、其餘合規。
        let status = bundle.mimetype_status().clone();
        if MIMETYPE_VIOLATIONS.contains(&rel) {
            assert!(
                status.is_not_first(),
                "{rel} 應歸類為非首條目違例，實際: {status:?}"
            );
            assert!(!status.is_compliant(), "{rel} 不得標記為合規");
            violation_hits += 1;
        } else {
            assert_eq!(
                status,
                MimetypeStatus::Compliant,
                "{rel} 應為合規檔案，實際: {status:?}"
            );
        }

        // 投影口徑（ODF-E1 起）：加密 content.xml／styles.xml 者必須落在
        // 明確 `Err(OdfError::Encrypted)`（老實報錯，不吞、不靜默零段）——
        // 判據與 project 的雙閘口徑完全對齊（content.xml∥styles.xml 任一加密即走錯誤軌）；
        // 其餘檔必須 Ok，且逐檔段落計數與 XML 原始計數一致。
        if bundle.manifest().is_encrypted("content.xml")
            || bundle.manifest().is_encrypted("styles.xml")
        {
            let err = project(&bundle)
                .err()
                .unwrap_or_else(|| panic!("{rel}：加密條目投影必須回 Err（ODF-E1）"));
            assert!(
                matches!(err, quire_odf::OdfError::Encrypted { .. }),
                "{rel}：加密條目投影必須落在 OdfError::Encrypted 明確變體，實際: {err:?}"
            );
            continue;
        }
        let proj = project(&bundle).unwrap_or_else(|e| panic!("{rel} 投影失敗: {e}"));
        assert_eq!(
            proj.text_p_count, exp_p,
            "{rel}：text:p 計數應與 XML 原始計數一致"
        );
        assert_eq!(
            proj.text_h_count, exp_h,
            "{rel}：text:h 計數應與 XML 原始計數一致"
        );
        assert_eq!(
            proj.paragraph_count(),
            exp_p + exp_h,
            "{rel}：段落總數（text:p＋text:h 全入列）應與 XML 原始計數一致"
        );
        assert_eq!(
            proj.paragraphs.len(),
            proj.paragraph_count(),
            "{rel}：段落清單長度應等於段落計數"
        );
        total_paragraphs += proj.paragraph_count();
    }

    assert_eq!(violation_hits, 3, "必須精確抓出三檔 mimetype 違例檔");
    assert_eq!(
        total_paragraphs, 19935,
        "50 檔段落總數（text:p 19687＋text:h 248）必須恰好等於 19935"
    );
}

/// 考卷 (b)-1：`ext/lo_core_sw_qa_odfexport_tdf152710.odt`（真實 Word/LO 檔，非
/// fdo 轉出）字級級聯硬斷言。XML 宣告值（PM 開檔可核對）：
/// - content.xml automatic-style `P5`（parent=`Heading_20_1`）宣告
///   `fo:font-size="10pt"`＋parent 鏈 `Heading_20_1` 宣告
///   `fo:font-weight="bold"`（styles.xml）。
/// - content.xml automatic-style `P3`（parent=`Standard`）宣告
///   `fo:font-size="10pt"`。
/// - styles.xml default-style(family=paragraph) 宣告 `fo:font-size="12pt"`。
///
/// 段落文件序（text:p＋text:h 混排）：[1] 是 `<text:h>`（style=P5，文字
/// 「Laisvos programos vaikų lavinimui」），[2] 是 `<text:p>`（style=P3）。
#[test]
fn b1_font_size_cascade_tdf152710() {
    let rel = "ext/lo_core_sw_qa_odfexport_tdf152710.odt";
    let bytes = read_file(rel);
    let bundle = OdfBundle::open(&bytes).expect("開檔");
    let proj = project(&bundle).expect("投影");
    assert!(
        proj.paragraph_count() >= 3,
        "{rel}：投影段落數應 ≥3（實測 {}）",
        proj.paragraph_count()
    );

    let heading = &proj.paragraphs[1];
    assert!(
        heading.text.starts_with("Laisvos programos"),
        "第 1 段應是 heading 文字，實際: {:?}",
        heading.text
    );
    assert_eq!(
        heading.font_size_pt(),
        10.0,
        "{rel} 第 1 段（text:h style=P5）字級必須是 content.xml 宣告的 10pt"
    );
    assert!(
        heading.bold(),
        "{rel} 第 1 段經 parent 鏈（Heading_20_1 fo:font-weight=bold）應記粗體"
    );

    let body_para = &proj.paragraphs[2];
    assert_eq!(
        body_para.font_size_pt(),
        10.0,
        "{rel} 第 2 段（text:p style=P3）字級必須是 content.xml 宣告的 10pt"
    );
    assert!(
        !body_para.bold(),
        "{rel} 第 2 段（P3 鏈無 bold 宣告）應為 normal"
    );
}

/// 考卷 (b)-2：`ext/oasis_opendocument_v1p3_os_part1_introduction.odt`
/// （OASIS 上游真實檔，非 fdo 轉出）字級級聯硬斷言。XML 宣告值：
/// - styles.xml 具名樣式 `Title`（parent=Normal）宣告 `fo:font-size="24pt"`
///   ＋`fo:font-weight="bold"`。
/// - styles.xml default-style(family=paragraph) 宣告 `fo:font-size="10pt"`
///   ＋`fo:font-weight="normal"`；content.xml automatic-style `P15`
///   （parent=Normal）無字級宣告 → 級聯落到 default-style 的 10pt。
///
/// 段落文件序：[0] `<text:p>` style=P15、[1] `<text:p>` style=Title
/// （「Open Document Format …」）。
#[test]
fn b2_font_size_cascade_oasis_intro() {
    let rel = "ext/oasis_opendocument_v1p3_os_part1_introduction.odt";
    let bytes = read_file(rel);
    let bundle = OdfBundle::open(&bytes).expect("開檔");
    let proj = project(&bundle).expect("投影");
    assert!(
        proj.paragraph_count() >= 2,
        "{rel}：投影段落數應 ≥2（實測 {}）",
        proj.paragraph_count()
    );

    let first = &proj.paragraphs[0];
    assert_eq!(
        first.font_size_pt(),
        10.0,
        "{rel} 第 0 段（style=P15 無宣告字級→default-style 10pt）級聯錯誤"
    );

    let title = &proj.paragraphs[1];
    assert!(
        title.text.starts_with("Open Document Format"),
        "第 1 段應是標題文字，實際: {:?}",
        title.text
    );
    assert_eq!(
        title.font_size_pt(),
        24.0,
        "{rel} 第 1 段（style=Title）字級必須是 styles.xml 宣告的 24pt"
    );
    assert!(
        title.bold(),
        "{rel} 第 1 段（Title fo:font-weight=bold）應記粗體"
    );
}

/// 考卷 (c)：`text:h` 入列斷言——同一語料檔的投影段落數必須**大於**僅 text:p
/// 計數（heading 也是一段，不另立計數），且 heading 文字出現在投影輸出。
#[test]
fn c_headings_enter_projection() {
    let rel = "ext/oasis_opendocument_v1p3_os_part1_introduction.odt";
    let bytes = read_file(rel);
    let bundle = OdfBundle::open(&bytes).expect("開檔");
    let proj = project(&bundle).expect("投影");

    assert_eq!(proj.text_p_count, 177, "{rel} text:p 基準");
    assert_eq!(proj.text_h_count, 7, "{rel} text:h 基準");
    assert!(
        proj.paragraph_count() > proj.text_p_count,
        "投影段落數（p+h={}) 必須嚴格大於僅 text:p 計數（{}）",
        proj.paragraph_count(),
        proj.text_p_count
    );
    assert_eq!(
        proj.paragraph_count(),
        proj.text_p_count + proj.text_h_count,
        "heading 必須入列成為段落，不另立計數"
    );
    let has_heading_text = proj
        .paragraphs
        .iter()
        .any(|p| p.text.contains("Normative References"));
    assert!(
        has_heading_text,
        "heading 文字（Normative References）必須出現在投影輸出"
    );
}

/// 附考卷：投影→layout 對接煙霧測試（A4 全域 default 參數）。對小中大型語料
/// 檔抽三檔：斷行輸出非空、分頁斷點單調遞增且嚴格小於段落總數。
#[test]
fn layout_integration_smoke() {
    for rel in [
        "ext/lo_core_sw_qa_odfexport_tdf152710.odt",
        "ext/oasis_opendocument_v1p3_os_part1_introduction.odt",
        "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.odt",
    ] {
        let bytes = read_file(rel);
        let bundle = OdfBundle::open(&bytes).expect("開檔");
        let proj = project(&bundle).expect("投影");
        let params = LayoutParams::default();

        let pls = layout_projection(&proj, &params);
        assert_eq!(
            pls.len(),
            proj.paragraph_count(),
            "{rel}：每段都必須有一筆斷行結果"
        );
        let total_lines: usize = pls.iter().map(|p| p.lines.len()).sum();
        assert!(total_lines > 0, "{rel}：有文字的檔案斷行輸出不得為空");

        let (texts, _) = proj.flatten();
        let breaks = page_break_positions(&texts, &params);
        for w in breaks.windows(2) {
            assert!(
                w[0] < w[1],
                "{rel}：分頁斷點必須嚴格單調遞增，實際: {breaks:?}"
            );
        }
        if let Some(&last) = breaks.last() {
            assert!(
                last < proj.paragraph_count(),
                "{rel}：分頁斷點必須小於段落總數，實際: {breaks:?}"
            );
        }
    }
}

/// 附考卷：`project_bytes` 純函數入口＋缺省口徑——無樣式宣告的合成 content.xml
/// 一律 fallback 12.0pt；`text:p`／`text:h` 混排全入列且計數如實。
#[test]
fn project_bytes_falls_back_to_12pt() {
    let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
<office:body><office:text>
<text:h text:style-name="NoSuchStyle">標題一</text:h>
<text:p>正文一</text:p>
<text:p/>
</office:text></office:body></office:document-content>"#;
    let proj = project_bytes(content.as_bytes(), None).expect("合成 content.xml 投影必須 Ok");
    assert_eq!(proj.text_h_count, 1);
    assert_eq!(proj.text_p_count, 2);
    assert_eq!(proj.paragraph_count(), 3, "text:p＋text:h 一律入列");
    assert_eq!(proj.paragraphs[0].text, "標題一");
    assert_eq!(proj.paragraphs[1].text, "正文一");
    assert!(
        proj.paragraphs[2].text.is_empty(),
        "自閉合 text:p 是零文字段"
    );
    for p in &proj.paragraphs {
        assert_eq!(
            p.font_size_pt(),
            DEFAULT_FONT_SIZE_PT,
            "無任何樣式宣告時一律 fallback 12.0pt"
        );
        assert!(!p.bold(), "無字重宣告時一律 normal");
    }
    let (texts, sizes) = proj.flatten();
    assert_eq!(texts.len(), 3);
    assert_eq!(sizes, vec![12.0, 12.0, 12.0], "降維輸出（文字＋逐段字級）");
}

/// 附考卷：run 級級聯粒度——同段落內 `text:span` 換字級／換字重必須拆成
/// 多個 run，`(碼點區間, 字級pt, 粗體)` 逐片段如實；span 顯式宣告
/// `font-weight="normal"` 必須壓過段落級粗體（三態級聯：宣告 normal ≠ 未宣告）。
#[test]
fn run_level_cascade_splits_spans() {
    let styles = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0">
<office:styles>
<style:default-style style:family="paragraph"><style:text-properties fo:font-size="11pt"/></style:default-style>
<style:style style:name="Heading" style:family="paragraph"><style:text-properties fo:font-size="20pt" fo:font-weight="bold"/></style:style>
<style:style style:name="TSmall" style:family="text"><style:text-properties fo:font-size="8pt"/></style:style>
<style:style style:name="TNormal" style:family="text"><style:text-properties fo:font-weight="normal"/></style:style>
</office:styles>
</office:document-styles>"#;
    let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
<office:body><office:text>
<text:p text:style-name="Heading">AB<text:span text:style-name="TSmall">CD</text:span><text:span text:style-name="TNormal">EF</text:span>G</text:p>
</office:text></office:body></office:document-content>"#;
    let proj = project_bytes(content.as_bytes(), Some(styles.as_bytes())).expect("合成檔投影");
    assert_eq!(proj.paragraph_count(), 1);
    let p = &proj.paragraphs[0];
    assert_eq!(p.text, "ABCDEFG");
    assert_eq!(
        p.font_size_pt(),
        20.0,
        "段落代表字級＝首 run（Heading 20pt）"
    );
    assert!(p.bold(), "段落代表粗體＝首 run（Heading bold）");
    // run 邊界：AB(20,bold) CD(8,bold 沿承) EF(20,normal 顯式壓過) G(20,bold 回到段級)
    let runs: Vec<((usize, usize), f32, bool)> = p.runs.clone();
    assert_eq!(
        runs,
        vec![
            ((0, 2), 20.0, true),
            ((2, 4), 8.0, true),
            ((4, 6), 20.0, false),
            ((6, 7), 20.0, true),
        ],
        "run 級碼點區間＋字級＋粗體必須逐片段如實（相鄰同属性合併）"
    );
}
