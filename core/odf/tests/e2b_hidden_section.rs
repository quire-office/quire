//! ODF-E2b 紅燈考卷：`text:section` 隱藏屬性（`display=none`／`is-hidden=true`／`condition`）段落不得發射。
//!
//! 真值出處：`docs/odf-survey/e2-flow-attribution.md` §2 D2／§3.1（LibreOffice 24.8 PDF 文字層逐檔對齊）。

use quire_odf::layout_glue::{project, project_bytes};
use quire_odf::OdfBundle;
use std::path::{Path, PathBuf};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-odt")
}

fn open_corpus(rel: &str) -> OdfBundle {
    let path = corpus_root().join(rel);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("讀取失敗 {rel}: {e}"));
    OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("開啟失敗 {rel}: {e}"))
}

/// `ooo32780-1.odt` 內 `Section1`／`Section2`／`Section5`／`Section3` 共用的 524 字元故事拷貝原文。
const STORY_COPY: &str = "He heard quiet steps behind him. That didn't bode well. Who could be following him this late at night and in this deadbeat part of town? And at this particular moment, just after he pulled off the big time and was making off with the greenbacks. Was there another crook who'd had the same idea, and was now watching him and waiting for a chance to grab the fruit of his labor? Or did the steps behind him mean that one of many law officers in town was on to him and just waiting to pounce and snap those cuffs on his wrists?";

/// `ooo32780-1.odt` 內 `Section6`（`content.xml` offset 8254）含尾註與註腳引用的第 3 份可見故事拷貝原文。
const SECTION6_STORY_COPY: &str = "viHe heard quiet steps behind4 him. That didn't bode well. Who could be following him this late at night and in this deadbeat part of town? And at this particular moment, just after he pulled off the big time and was making off with the greenbacks. Was there another crook who'd had the same idea, and was now watching him and waiting for a chance to grab the fruit of his labor? Or did the steps behind him mean that one of many law officers in town was on to him and just waiting to pounce and snap those cuffs on his wrists?";

/// 首紅例 `ext/lo_core_sw_qa_odfimport_ooo32780-1.odt`：驗證 `Section2`（offset 5005 `text:display="none"`）與 `Section5`（offset 6148 `text:is-hidden="true"`）不發射文字，且三份可見拷貝與其餘段落完整保留。
#[test]
fn ooo32780_1_hidden_sections_suppressed_and_visible_preserved() {
    let rel = "ext/lo_core_sw_qa_odfimport_ooo32780-1.odt";
    let bundle = open_corpus(rel);
    let proj = project(&bundle).expect("投影失敗");

    // 段落平鋪槽位總數維持 32（對齊 ODF-C3 原始計數不變量）。
    assert_eq!(proj.paragraph_count(), 32, "{rel} 段落槽位數應維持 32");

    // Absence 斷言 1：`content.xml` offset 4946..5025 之 `Section2` 帶 `text:display="none"`（offset 5005），對應第 7 段不得發射故事拷貝文字。
    assert_eq!(
        proj.paragraphs[7].text, "",
        "{rel} Section2（content.xml offset 5005 text:display=\"none\"）段落不得發射任何文字"
    );

    // Absence 斷言 2：`content.xml` offset 6062..6195 之 `Section5` 帶 `text:is-hidden="true"`（offset 6148），對應第 12 段不得發射故事拷貝文字。
    assert_eq!(
        proj.paragraphs[12].text, "",
        "{rel} Section5（content.xml offset 6148 text:is-hidden=\"true\"）段落不得發射任何文字"
    );

    // Absence 斷言 3：全檔故事拷貝在投影中僅能出現於 3 個可見區段（Section1、Section3、Section6），不得為修前的 5 份。
    let story_occurrences = proj
        .paragraphs
        .iter()
        .filter(|p| p.text.contains("He heard quiet steps behind"))
        .count();
    assert_eq!(
        story_occurrences, 3,
        "{rel} 故事拷貝在投影中應恰為 3 份可見拷貝（Section2 與 Section5 兩份隱藏拷貝不得出現）"
    );

    // Absence 斷言 4：總字元數應自修前 3082 扣減兩段 524 字為 2034；單份故事拷貝去空白為 422 字（E2a 補回每段 2 個單引號後由 420 增為 422，兩段共 844 字），全檔去空白字數自 2519 扣減 844 字後為 1675。
    assert_eq!(
        proj.total_text_len, 2034,
        "{rel} 投影總字元數應為 2034（扣除 Section2 與 Section5 各 524 字）"
    );
    let story_non_ws_len = STORY_COPY.chars().filter(|c| !c.is_whitespace()).count();
    assert_eq!(
        story_non_ws_len * 2,
        844,
        "兩段隱藏拷貝去空白字數合計應為 844 字（E2a 修前為 840 字，補回 4 個單引號後為 844 字）"
    );
    let non_ws_len: usize = proj
        .paragraphs
        .iter()
        .flat_map(|p| p.text.chars())
        .filter(|c| !c.is_whitespace())
        .count();
    assert_eq!(
        non_ws_len, 1675,
        "{rel} 去空白總字數應為 1675（修前 2519 扣除兩段隱藏拷貝 844 字）"
    );

    // Presence 斷言 1：可見拷貝一 `Section1`（`content.xml` offset 4058，無隱藏屬性）對應第 2 段必須完整保留。
    assert_eq!(
        proj.paragraphs[2].text, STORY_COPY,
        "{rel} Section1（content.xml offset 4058）可見拷貝一不得被誤砍"
    );

    // Presence 斷言 2：可見拷貝二 `Section3`（`content.xml` offset 7024，`text:protected="true"`）對應第 17 段必須完整保留。
    assert_eq!(
        proj.paragraphs[17].text, STORY_COPY,
        "{rel} Section3（content.xml offset 7024 text:protected=\"true\"）可見拷貝二不得被誤砍"
    );

    // Presence 斷言 3：可見拷貝三 `Section6`（`content.xml` offset 8254，無隱藏屬性）對應第 27 段及內部尾註（第 25 段）、註腳（第 26 段）必須完整保留。
    assert_eq!(
        proj.paragraphs[25].text, "Endnote (at the end of section – restart at 6)",
        "{rel} Section6 內嵌尾註段落不得被誤砍"
    );
    assert_eq!(
        proj.paragraphs[26].text, "Footnote (at the end of the text – restart at 4)",
        "{rel} Section6 內嵌註腳段落不得被誤砍"
    );
    assert_eq!(
        proj.paragraphs[27].text, SECTION6_STORY_COPY,
        "{rel} Section6（content.xml offset 8254）可見拷貝三不得被誤砍"
    );

    // Presence 斷言 4：可見連結區段 `Section4`（`content.xml` offset 7828）及全檔其餘所有非空段落一字不漏。
    let expected_other_non_empty: &[(usize, &str)] = &[
        (0, "Section Simple"),
        (5, "Section Hidden"),
        (
            6,
            "------------------ Beginn of hidden section ------------------",
        ),
        (
            8,
            "------------------ End of hidden section ------------------",
        ),
        (10, "Section Hidden IF (x==1) -> 1"),
        (
            11,
            "------------------ Beginn of hidden section ------------------",
        ),
        (
            13,
            "------------------ End of hidden section ------------------",
        ),
        (15, "Section Protected"),
        (19, "Section Linked"),
        (21, "I am a linked section"),
        (23, "Section Option"),
    ];
    for &(idx, expected_text) in expected_other_non_empty {
        assert_eq!(
            proj.paragraphs[idx].text, expected_text,
            "{rel} 第 {idx} 段可見文字不得被誤砍"
        );
    }
}

/// 驗證 `text:display` 三種隱藏寫法與 `condition` 有無條件參數時的保守照發邊界。
#[test]
fn display_condition_variants_and_conservative_fallback() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
<office:body><office:text>
<text:section text:name="S1" text:display="none"><text:p>隱藏一_display_none</text:p></text:section>
<text:section text:name="S2" text:is-hidden="true"><text:p>隱藏二_is_hidden_true</text:p></text:section>
<text:section text:name="S3" text:display="condition"><text:p>隱藏三_無條件參數標準寫法</text:p></text:section>
<text:section text:name="S4" text:display="condition()"><text:p>隱藏四_無條件參數括號寫法</text:p></text:section>
<text:section text:name="S5" text:display="condition" text:condition="ooow:x==0"><text:p>可見五_帶可求值條件屬性保守照發</text:p></text:section>
<text:section text:name="S6" text:display="condition(ooow:x==0)"><text:p>可見六_括號帶可求值條件保守照發</text:p></text:section>
<text:section text:name="S7" text:display="true"><text:p>可見七_display_true</text:p></text:section>
</office:text></office:body></office:document-content>"#;

    let proj = project_bytes(xml.as_bytes(), None).expect("合成 XML 投影失敗");
    let texts: Vec<&str> = proj.paragraphs.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "",
            "",
            "",
            "",
            "可見五_帶可求值條件屬性保守照發",
            "可見六_括號帶可求值條件保守照發",
            "可見七_display_true",
        ],
        "無條件參數的 condition 應隱藏，帶可求值條件但未標 is-hidden=true 者應保守照發"
    );
}

/// 驗證嵌套 `text:section`（隱藏包可見、可見包隱藏）各自依自身屬性獨立判定。
#[test]
fn nested_sections_evaluated_independently() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
<office:body><office:text>
<text:section text:name="OuterHidden" text:display="none">
  <text:p>外層隱藏前段</text:p>
  <text:section text:name="InnerVisibleExplicit" text:display="true">
    <text:p>內層顯式可見段</text:p>
  </text:section>
  <text:section text:name="InnerVisibleDefault">
    <text:p>內層預設可見段</text:p>
  </text:section>
  <text:p>外層隱藏後段</text:p>
</text:section>
<text:section text:name="OuterVisible">
  <text:p>外層可見前段</text:p>
  <text:section text:name="InnerHidden" text:is-hidden="true">
    <text:p>內層隱藏段</text:p>
  </text:section>
  <text:p>外層可見後段</text:p>
</text:section>
</office:text></office:body></office:document-content>"#;

    let proj = project_bytes(xml.as_bytes(), None).expect("嵌套 section 投影失敗");
    let texts: Vec<&str> = proj.paragraphs.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "",
            "內層顯式可見段",
            "內層預設可見段",
            "",
            "外層可見前段",
            "",
            "外層可見後段",
        ],
        "嵌套 section 應各自獨立判定：隱藏的不發、可見的仍發，離開內層後恢復外層狀態"
    );
}
