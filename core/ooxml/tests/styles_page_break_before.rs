//! 樣式級 `w:pageBreakBefore` 進級聯的單元考卷（oc-pbb-fix）。
//!
//! 三態語義：未宣告＝`None`；`w:val="false"|"0"|"off"`＝`Some(false)`；
//! 無 `w:val` 或 `w:val="true"|"1"|"on"`＝`Some(true)`。合成序：
//! 樣式 own `.or()` basedOn 父鏈，段落直接格式（inline pPr）再壓樣式級。

use quire_ooxml::{ParaStyle, StyleTable};

const PREFIX: &str =
    r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#;
const SUFFIX: &str = "</w:styles>";

fn wrap(body: &str) -> String {
    format!("{PREFIX}{body}{SUFFIX}")
}

/// 燈 1：樣式帶 `<w:pPr><w:pageBreakBefore/></w:pPr>`＋段落僅 `pStyle`
/// （無直接 pPr）⇒ 級聯結果 page_break＝true。
#[test]
fn style_page_break_before_flows_into_cascade() {
    let t = StyleTable::parse(
        wrap(
            r#"<w:style w:type="paragraph" w:styleId="Heading1">
                 <w:pPr><w:pageBreakBefore/></w:pPr>
               </w:style>"#,
        )
        .as_bytes(),
    );
    let (_, para) = t.effective_para(Some("Heading1"), ParaStyle::default());
    assert_eq!(
        para.page_break,
        Some(true),
        "樣式級 pageBreakBefore 必須經級聯流入段落旗標"
    );
}

/// 燈 2：樣式宣告 `<w:pageBreakBefore w:val="false"/>` ⇒ 明確 false
/// （壓過 basedOn 父樣式繼承的 true）。
#[test]
fn explicit_false_overrides_inherited_true() {
    let t = StyleTable::parse(
        wrap(
            r#"<w:style w:type="paragraph" w:styleId="Parent">
                 <w:pPr><w:pageBreakBefore/></w:pPr>
               </w:style>
               <w:style w:type="paragraph" w:styleId="Child">
                 <w:basedOn w:val="Parent"/>
                 <w:pPr><w:pageBreakBefore w:val="false"/></w:pPr>
               </w:style>"#,
        )
        .as_bytes(),
    );
    let (_, para) = t.effective_para(Some("Child"), ParaStyle::default());
    assert_eq!(
        para.page_break,
        Some(false),
        "w:val=false 必須是明確關，不可被父樣式的 true 蓋回"
    );
}

/// 燈 3：父樣式宣告 true、子樣式未宣告 ⇒ 繼承 true；段落直接 pPr 明確
/// false ⇒ 直接格式贏過樣式級。
#[test]
fn parent_true_inherited_and_inline_false_wins() {
    let t = StyleTable::parse(
        wrap(
            r#"<w:style w:type="paragraph" w:styleId="Parent">
                 <w:pPr><w:pageBreakBefore/></w:pPr>
               </w:style>
               <w:style w:type="paragraph" w:styleId="Child">
                 <w:basedOn w:val="Parent"/>
               </w:style>"#,
        )
        .as_bytes(),
    );
    let (_, inherited) = t.effective_para(Some("Child"), ParaStyle::default());
    assert_eq!(inherited.page_break, Some(true), "子樣式未宣告 ⇒ 繼承父 true");

    let inline = ParaStyle {
        page_break: Some(false),
        ..ParaStyle::default()
    };
    let (_, overridden) = t.effective_para(Some("Child"), inline);
    assert_eq!(
        overridden.page_break,
        Some(false),
        "段落直接 pPr 明確 false 必須贏過樣式級 true"
    );
}
