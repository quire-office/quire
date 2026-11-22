//! E-1 同源返修紅燈考卷：`patch_ppr`／`patch_numpr` 欄位級合併——分次單欄段落
//! 補丁不得互相抹除既有段落宣告。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 缺陷（父卡 t_d69a5990 探針坐實，本卡重刻）
//! `patch_rpr`（E-1 已修）與 `patch_ppr`／`patch_numpr` 同源誤用
//! `strip_leading_element(ppr, "w:pPr")` 取「既有宣告內文」。該函式語義是
//! 「摘掉開頭元素、保留**其餘**」——對 `leading_element` 的產物（元素本身）
//! **恆回空字串**，於是用任何單欄段落補丁重組 `w:pPr` 時，既有 pStyle／numPr／
//! spacing／pageBreakBefore 全數蒸發。用戶可見形態：設對齊 → 段落樣式／清單
//! 編號／行距／分頁前全部消失（設行距、設分頁同理互抹）。
//!
//! # 斷言層口徑
//! - 寫入面：`word/document.xml` 明文逐字斷言（未觸碰宣告俱在／覆寫／移除形態、
//!   同族不重複、CT_PPr 宣告序），必要時另存檔重開（`save_docx` →
//!   `from_docx_bytes`）與 `roundtrip_check` 驗一次，防止只在記憶體態正確。
//! - 幾何面：`pageBreakBefore` 這類「用戶看得見」的宣告必須在版面上仍生效
//!   （受標段仍自新頁起頭），防止「XML 寫對、版面不認」。
//! - 無損紅線：全 `None` 補丁（零意圖）存檔 byte-identical——既有硬閂回歸鎖。
//! - `patch_numpr` 臂需帶 `word/numbering.xml`（`abstractNum`＋`num` 對得上
//!   numId）的 fixture，否則 `SetListNumbering` 會在 `InvalidNumberingReference`
//!   就被擋掉，拍不到合併面的紅燈。

use quire_engine::{diff_untouched, Engine, ListAction, Mutation, ParaPropsPatch, SubDocTarget};

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

fn para_props(engine: &mut Engine, para: usize, patch: ParaPropsPatch) {
    engine
        .apply_mutation(Mutation::SetParagraphStyle {
            target: SubDocTarget::Body,
            para_range: para..=para,
            patch,
        })
        .expect("套用 M2.2 SetParagraphStyle 失敗");
    settle(engine);
}

fn numbering(engine: &mut Engine, para: usize, action: ListAction) {
    engine
        .apply_mutation(Mutation::SetListNumbering {
            target: SubDocTarget::Body,
            para_range: para..=para,
            action,
        })
        .expect("套用 M3.1 SetListNumbering 失敗");
    settle(engine);
}

fn doc_xml(engine: &Engine) -> String {
    String::from_utf8(
        engine
            .part("word/document.xml")
            .expect("必須有 word/document.xml")
            .to_vec(),
    )
    .expect("document.xml 非 UTF-8")
}

/// 取出含 `needle` 的第一個 `<w:p>`…`</w:p>` 明文（受標段落真值）。
fn para_xml_containing(xml: &str, needle: &str) -> String {
    let at = xml
        .find(needle)
        .unwrap_or_else(|| panic!("document.xml 必須含文字片段 {needle:?}"));
    let start = xml[..at]
        .rfind("<w:p>")
        .or_else(|| xml[..at].rfind("<w:p "))
        .unwrap_or_else(|| panic!("文字片段 {needle:?} 之前必須有 <w:p> 開標籤"));
    let end = xml[at..]
        .find("</w:p>")
        .map(|rel| at + rel + "</w:p>".len())
        .unwrap_or_else(|| panic!("文字片段 {needle:?} 之後必須有 </w:p>"));
    xml[start..end].to_string()
}

/// 取出受標段落的 `<w:pPr>…</w:pPr>` 明文（不含段落本體）。
fn ppr_of_para(para_xml: &str) -> String {
    let start = para_xml
        .find("<w:pPr")
        .unwrap_or_else(|| panic!("段落必須帶 <w:pPr>，實測 = {para_xml}"));
    let end = para_xml
        .rfind("</w:pPr>")
        .map(|rel| rel + "</w:pPr>".len())
        .unwrap_or_else(|| panic!("pPr 必須有閉標籤，實測 = {para_xml}"));
    para_xml[start..end].to_string()
}

const STYLES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"/>
  <w:style w:type="paragraph" w:styleId="Heading1">
    <w:name w:val="heading 1"/>
    <w:pPr><w:keepNext/><w:spacing w:line="240" w:lineRule="auto"/></w:pPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading2">
    <w:name w:val="heading 2"/>
  </w:style>
</w:styles>"#;

/// `abstractNum`＋`num` 對得上 numId 的最小編號件（父卡探針缺的就是這件）。
const NUMBERING_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="7">
    <w:multiLevelType w:val="hybridMultilevel"/>
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/>
    </w:lvl>
    <w:lvl w:ilvl="1">
      <w:start w:val="1"/><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="%2)"/><w:lvlJc w:val="left"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="7"><w:abstractNumId w:val="7"/></w:num>
  <w:num w:numId="9"><w:abstractNumId w:val="7"/></w:num>
</w:numbering>"#;

fn body_wrap(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml">
  <w:body>
{body}
    <w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
  </w:body>
</w:document>"#
    )
}

/// 以明文段落序列組 docx（`styles.xml` 恆附；`with_numbering` 決定要不要
/// `word/numbering.xml`——`SetListNumbering` 的 numId 驗證需要它）。
fn docx_from_body(body: &str, with_numbering: bool) -> Vec<u8> {
    let mut zip = ZipTestHelper::new();
    zip.add(
        "[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
  <Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
</Types>"#,
    );
    zip.add(
        "_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    );
    zip.add(
        "word/_rels/document.xml.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
</Relationships>"#,
    );
    zip.add("word/styles.xml", STYLES_XML.as_bytes());
    if with_numbering {
        zip.add("word/numbering.xml", NUMBERING_XML.as_bytes());
    }
    zip.add("word/document.xml", body_wrap(body).as_bytes());
    zip.finish()
}

fn engine_from_body(body: &str, with_numbering: bool) -> Engine {
    let bytes = docx_from_body(body, with_numbering);
    let mut engine =
        Engine::from_docx_bytes(&bytes).expect("以明文 document.xml 組裝 docx 並開啟失敗");
    settle(&mut engine);
    engine
}

/// 基準段＋受標段（para 1，以「這是一份」開頭）＋尾段。
fn three_para(ppr_inner: &str) -> String {
    let ppr = if ppr_inner.is_empty() {
        String::new()
    } else {
        format!("<w:pPr>{ppr_inner}</w:pPr>")
    };
    format!(
        r#"    <w:p><w:r><w:t>基準段</w:t></w:r></w:p>
    <w:p>{ppr}<w:r><w:t>這是一份受標文字</w:t></w:r></w:p>
    <w:p><w:r><w:t>尾段</w:t></w:r></w:p>"#
    )
}

/// 父卡探針同款段落：pStyle＋numPr＋spacing＋pageBreakBefore 四宣告俱在。
fn rich_ppr() -> String {
    r#"<w:pStyle w:val="Heading1"/><w:numPr><w:ilvl w:val="1"/><w:numId w:val="7"/></w:numPr><w:spacing w:line="360" w:lineRule="auto"/><w:pageBreakBefore/>"#.to_string()
}

/// 燈 1（核心紅燈）：段落帶四宣告，只發**單欄** `alignment` → 四宣告必須全存活。
#[test]
fn eppr_lamp1_single_field_alignment_keeps_existing_declarations() {
    let mut engine = engine_from_body(&three_para(&rich_ppr()), true);

    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            alignment: Some("center".to_string()),
            ..Default::default()
        },
    );

    let para = para_xml_containing(&doc_xml(&engine), "這是一份");
    let ppr = ppr_of_para(&para);
    println!("=== 燈 1 實測 ===\n受標段落 XML = {para}\npPr = {ppr}");
    println!("  w:pStyle 存活 = {}", ppr.contains("w:pStyle"));
    println!("  w:numPr 存活 = {}", ppr.contains("w:numPr"));
    println!("  w:spacing 存活 = {}", ppr.contains("w:spacing"));
    println!(
        "  w:pageBreakBefore 存活 = {}",
        ppr.contains("w:pageBreakBefore")
    );
    println!("  w:jc 存活 = {}", ppr.contains("w:jc"));

    assert!(
        ppr.contains(r#"<w:pStyle w:val="Heading1"/>"#),
        "單欄 alignment 補丁（style_id=None）必須保留既有 `<w:pStyle w:val=\"Heading1\"/>`，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w:numId w:val="7"/>"#) && ppr.contains(r#"<w:ilvl w:val="1"/>"#),
        "單欄 alignment 補丁不得抹除既有 `<w:numPr>`（ilvl=1／numId=7），實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w:spacing w:line="360" w:lineRule="auto"/>"#),
        "單欄 alignment 補丁不得抹除既有 `<w:spacing w:line=\"360\">`，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains("<w:pageBreakBefore/>"),
        "單欄 alignment 補丁不得抹除既有 `<w:pageBreakBefore/>`，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w:jc w:val="center"/>"#),
        "單欄 alignment 補丁必須寫入 `<w:jc w:val=\"center\"/>`，實測 pPr = {ppr}"
    );
}

/// 燈 2：`None` 欄位＝繼承既有宣告——只發分頁欄時，pStyle／spacing／jc 不受波及。
#[test]
fn eppr_lamp2_none_fields_inherit_existing_declarations() {
    let inner = r#"<w:pStyle w:val="Heading2"/><w:spacing w:line="240" w:lineRule="auto"/><w:jc w:val="left"/>"#;
    let mut engine = engine_from_body(&three_para(inner), false);

    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            page_break_before: Some(true),
            ..Default::default()
        },
    );

    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 2 實測 ===\npPr = {ppr}");

    assert!(
        ppr.contains(r#"<w:pStyle w:val="Heading2"/>"#)
            && ppr.contains(r#"<w:spacing w:line="240" w:lineRule="auto"/>"#)
            && ppr.contains(r#"<w:jc w:val="left"/>"#),
        "page_break_before=Some(true) 時，未被點名的 pStyle／spacing／jc 必須逐字保留，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains("<w:pageBreakBefore/>"),
        "page_break_before=Some(true) 必須寫入 `<w:pageBreakBefore/>`，實測 pPr = {ppr}"
    );
}

/// 燈 3：`Some` 同族覆寫仍須精確——覆寫新值、舊同族宣告不殘留重複。
#[test]
fn eppr_lamp3_some_field_overwrites_same_family_without_dupes() {
    let inner = r#"<w:pStyle w:val="Heading1"/><w:jc w:val="left"/><w:spacing w:line="240" w:lineRule="auto"/>"#;
    let mut engine = engine_from_body(&three_para(inner), false);

    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            alignment: Some("center".to_string()),
            line_spacing: Some(1.5),
            ..Default::default()
        },
    );

    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 3 實測 ===\npPr = {ppr}");

    assert_eq!(
        ppr.matches("<w:jc").count(),
        1,
        "同族 w:jc 必須恰為 1 條（覆寫不得殘留舊宣告），實測 pPr = {ppr}"
    );
    assert_eq!(
        ppr.matches("<w:spacing").count(),
        1,
        "同族 w:spacing 必須恰為 1 條，實測 pPr = {ppr}"
    );
    assert_eq!(
        ppr.matches("<w:pStyle").count(),
        1,
        "w:pStyle 未被觸碰卻不該被重複材料化，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w:jc w:val="center"/>"#),
        "覆寫後的 w:jc 必須是新值 center，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"w:line="360""#),
        "line_spacing=Some(1.5) 必須覆寫成 w:line=\"360\"，實測 pPr = {ppr}"
    );
    assert!(
        !ppr.contains(r#"w:val="left""#) && !ppr.contains(r#"w:line="240""#),
        "同族舊宣告（jc=left／line=240）必須被移除，實測 pPr = {ppr}"
    );
}

/// 燈 4（分次單欄疊加）：四筆各動一欄的呼叫必須互相疊加、宣告序守 CT_PPr。
#[test]
fn eppr_lamp4_sequential_single_field_calls_accumulate() {
    let mut engine = engine_from_body(&three_para(""), false);

    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            style_id: Some("Heading1".to_string()),
            ..Default::default()
        },
    );
    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            page_break_before: Some(true),
            ..Default::default()
        },
    );
    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            line_spacing: Some(2.0),
            ..Default::default()
        },
    );
    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            alignment: Some("right".to_string()),
            ..Default::default()
        },
    );

    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 4 實測 ===\npPr = {ppr}");

    for needle in [
        r#"<w:pStyle w:val="Heading1"/>"#,
        "<w:pageBreakBefore/>",
        r#"<w:spacing w:line="480" w:lineRule="auto"/>"#,
        r#"<w:jc w:val="right"/>"#,
    ] {
        assert!(
            ppr.contains(needle),
            "四筆單欄呼叫後必須含 {needle}（後筆不得抹除前筆），實測 pPr = {ppr}"
        );
    }
    let pos = |n: &str| ppr.find(n).unwrap_or_else(|| panic!("pPr 應含 {n}：{ppr}"));
    assert!(
        pos(r#"<w:pStyle w:val="Heading1"/>"#) < pos("<w:pageBreakBefore/>")
            && pos("<w:pageBreakBefore/>") < pos(r#"<w:spacing w:line="480""#)
            && pos(r#"<w:spacing w:line="480""#) < pos(r#"<w:jc w:val="right"/>"#),
        "CT_PPr 宣告序必須為 pStyle → pageBreakBefore → spacing → jc，實測 pPr = {ppr}"
    );

    let saved = engine.save_docx().expect("存檔失敗");
    let reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
    let reopened_ppr = ppr_of_para(&para_xml_containing(
        std::str::from_utf8(reopened.part("word/document.xml").expect("document.xml"))
            .expect("UTF-8"),
        "這是一份",
    ));
    println!("=== 燈 4 存檔重開 ===\npPr = {reopened_ppr}");
    assert_eq!(
        reopened_ppr, ppr,
        "存檔重開後 pPr 必須逐字不變（未被重解析路徑重寫）"
    );
}

/// 燈 5：零意圖硬閂——全 `None` 補丁存檔 byte-identical、pPr 逐字原樣。
#[test]
fn eppr_lamp5_empty_patch_is_byte_identical() {
    let body = three_para(&rich_ppr());
    let bytes = docx_from_body(&body, true);
    let mut engine = Engine::from_docx_bytes(&bytes).expect("開啟失敗");
    settle(&mut engine);
    let before = engine.save_docx().expect("存檔失敗");
    let ppr_before = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));

    para_props(&mut engine, 1, ParaPropsPatch::default());

    let after = engine.save_docx().expect("存檔失敗");
    let ppr_after = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 5 實測 ===\n修前 pPr = {ppr_before}\n修後 pPr = {ppr_after}");

    assert_eq!(
        ppr_after, ppr_before,
        "全 None 補丁（零意圖）的 pPr 必須逐字元原樣（含元素間雜訊），不得重排／材料化空 pPr"
    );
    assert_eq!(
        before, after,
        "全 None 補丁（零意圖）必須零位元組位移——既有硬閂不得被段落欄位級合併波及"
    );
}

/// 燈 5b：零 pPr 段落遇到零意圖補丁不得材料化空 `<w:pPr></w:pPr>`。
#[test]
fn eppr_lamp5b_empty_patch_does_not_materialise_empty_ppr() {
    let bytes = docx_from_body(&three_para(""), false);
    let mut engine = Engine::from_docx_bytes(&bytes).expect("開啟失敗");
    settle(&mut engine);
    let before = engine.save_docx().expect("存檔失敗");

    para_props(&mut engine, 1, ParaPropsPatch::default());

    let after = engine.save_docx().expect("存檔失敗");
    let para = para_xml_containing(&doc_xml(&engine), "這是一份");
    println!("=== 燈 5b 實測 ===\n受標段落 XML = {para}");
    assert!(
        !para.contains("<w:pPr>"),
        "零意圖補丁不得把無 pPr 的段落材料化成空 pPr，實測段落 = {para}"
    );
    assert_eq!(before, after, "零意圖補丁必須零位元組位移（存檔面）");
}

/// 燈 6：巢裝 `w:pPrChange`（內含第二顆 `w:pPr`）必須整塊存活、XML 平衡。
///
/// 父卡 E-1 對 `rPrChange` 已把 `leading_element` 改成深度計數收尾；本燈驗
/// pPr 側同樣吃得到，且 `element_inner` 尾錨 `rfind` 口徑在「巢裝同族」下
/// 不會把閉標籤錯認成內層那顆。
#[test]
fn eppr_lamp6_nested_ppr_change_survives_merge() {
    let inner = r#"<w:pStyle w:val="Heading1"/><w:spacing w:line="360" w:lineRule="auto"/><w:pPrChange w:id="12" w:author="稽核" w:date="2026-01-01T00:00:00Z"><w:pPr><w:pStyle w:val="Normal"/><w:jc w:val="both"/></w:pPr></w:pPrChange>"#;
    let mut engine = engine_from_body(&three_para(inner), false);

    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            alignment: Some("center".to_string()),
            ..Default::default()
        },
    );

    let para = para_xml_containing(&doc_xml(&engine), "這是一份");
    let ppr = ppr_of_para(&para);
    println!("=== 燈 6 實測 ===\n受標段落 XML = {para}\npPr = {ppr}");

    assert!(
        ppr.contains(r#"<w:pPrChange w:id="12" w:author="稽核" w:date="2026-01-01T00:00:00Z">"#),
        "巢裝 `<w:pPrChange>` 開標籤必須逐字存活，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w:jc w:val="both"/>"#),
        "`w:pPrChange` 內層的 `<w:jc w:val=\"both\"/>` 必須存活（巢裝內容不可被當成自身宣告掃掉），實測 pPr = {ppr}"
    );
    assert_eq!(
        ppr.matches("<w:pPrChange").count(),
        1,
        "w:pPrChange 必須恰為 1 條，實測 pPr = {ppr}"
    );
    assert_eq!(
        ppr.matches("<w:pPr>").count(),
        ppr.matches("</w:pPr>").count(),
        "pPr 開閉標籤必須平衡（巢裝截斷會吐出不平衡 XML），實測 pPr = {ppr}"
    );

    let diff = engine.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(
        diff.pass,
        "帶巢裝 pPrChange 的段落經欄位級合併後必須仍是合法文件：{:?}",
        diff.violations
    );
    let saved = engine.save_docx().expect("存檔失敗");
    let _ = Engine::from_docx_bytes(&saved).expect("合併後存檔重開失敗（ill-formed document）");
}

/// 燈 6b：巢裝 `w:pPrChange` ＋ 清單動作同樣不得截斷（`patch_numpr` 側）。
#[test]
fn eppr_lamp6b_nested_ppr_change_survives_numbering_merge() {
    let inner = r#"<w:pStyle w:val="Heading1"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="9"/></w:numPr><w:pPrChange w:id="7" w:author="稽核" w:date="2026-01-02T00:00:00Z"><w:pPr><w:spacing w:line="240" w:lineRule="auto"/></w:pPr></w:pPrChange>"#;
    let mut engine = engine_from_body(&three_para(inner), true);

    numbering(&mut engine, 1, ListAction::ChangeLevel { ilvl: 1 });

    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 6b 實測 ===\npPr = {ppr}");

    assert!(
        ppr.contains(r#"<w:pPrChange w:id="7" w:author="稽核" w:date="2026-01-02T00:00:00Z">"#)
            && ppr.contains(r#"<w:spacing w:line="240" w:lineRule="auto"/>"#),
        "清單動作的重組不得截斷巢裝 `<w:pPrChange>`，實測 pPr = {ppr}"
    );
    assert_eq!(
        ppr.matches("<w:pPr>").count(),
        ppr.matches("</w:pPr>").count(),
        "pPr 開閉標籤必須平衡，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w:ilvl w:val="1"/>"#) && ppr.contains(r#"<w:numId w:val="9"/>"#),
        "ChangeLevel 必須改 ilvl=1 並沿用既有 numId=9，實測 pPr = {ppr}"
    );
}

/// 燈 7（`patch_numpr` 臂核心紅燈）：設清單編號不得抹除既有 pStyle／spacing。
#[test]
fn eppr_lamp7_attach_numbering_keeps_existing_declarations() {
    let inner = r#"<w:pStyle w:val="Heading1"/><w:spacing w:line="360" w:lineRule="auto"/><w:jc w:val="center"/>"#;
    let mut engine = engine_from_body(&three_para(inner), true);

    numbering(
        &mut engine,
        1,
        ListAction::AttachExisting { num_id: 7, ilvl: 1 },
    );

    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 7 實測 ===\npPr = {ppr}");

    assert!(
        ppr.contains(r#"<w:numPr><w:ilvl w:val="1"/><w:numId w:val="7"/></w:numPr>"#),
        "AttachExisting 必須寫入 numPr(ilvl=1,numId=7)，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w:pStyle w:val="Heading1"/>"#),
        "設清單編號（style_id 未被點名）必須保留既有 pStyle，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w:spacing w:line="360" w:lineRule="auto"/>"#),
        "設清單編號不得抹除既有 spacing，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w:jc w:val="center"/>"#),
        "設清單編號不得抹除既有 jc，實測 pPr = {ppr}"
    );
    // CT_PPr 序：pStyle(0) → numPr(6) → spacing(21) → jc(26)。
    let pos = |n: &str| ppr.find(n).unwrap_or_else(|| panic!("pPr 應含 {n}：{ppr}"));
    assert!(
        pos(r#"<w:pStyle w:val="Heading1"/>"#) < pos("<w:numPr>")
            && pos("<w:numPr>") < pos(r#"<w:spacing w:line="360""#)
            && pos(r#"<w:spacing w:line="360""#) < pos(r#"<w:jc w:val="center"/>"#),
        "CT_PPr 宣告序必須為 pStyle → numPr → spacing → jc，實測 pPr = {ppr}"
    );
}

/// 燈 8：`RemoveNumbering` 只摘 numPr，其餘段落宣告（含既有 pStyle／spacing／jc）
/// 逐字存活；且對「本來就沒有 numPr」的段落必須零位元組位移。
#[test]
fn eppr_lamp8_remove_numbering_touches_only_numpr() {
    let inner = r#"<w:pStyle w:val="Heading1"/><w:numPr><w:ilvl w:val="1"/><w:numId w:val="7"/></w:numPr><w:spacing w:line="360" w:lineRule="auto"/><w:jc w:val="both"/>"#;
    let mut engine = engine_from_body(&three_para(inner), true);

    numbering(&mut engine, 1, ListAction::RemoveNumbering);

    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 8 實測 ===\npPr = {ppr}");

    assert!(
        !ppr.contains("<w:numPr>"),
        "RemoveNumbering 必須移除 numPr，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w:pStyle w:val="Heading1"/>"#)
            && ppr.contains(r#"<w:spacing w:line="360" w:lineRule="auto"/>"#)
            && ppr.contains(r#"<w:jc w:val="both"/>"#),
        "RemoveNumbering 只准摘 numPr，pStyle／spacing／jc 必須逐字保留，實測 pPr = {ppr}"
    );

    // 無 numPr 的段落發 RemoveNumbering：合法產物是零位元組位移。
    let plain_inner = r#"<w:pStyle w:val="Heading2"/><w:jc w:val="right"/>"#;
    let bytes = docx_from_body(&three_para(plain_inner), true);
    let mut plain = Engine::from_docx_bytes(&bytes).expect("開啟失敗");
    settle(&mut plain);
    let before = plain.save_docx().expect("存檔失敗");
    numbering(&mut plain, 1, ListAction::RemoveNumbering);
    let after = plain.save_docx().expect("存檔失敗");
    let plain_ppr = ppr_of_para(&para_xml_containing(&doc_xml(&plain), "這是一份"));
    println!("=== 燈 8 無 numPr 態 ===\npPr = {plain_ppr}");
    assert_eq!(plain_ppr, format!("<w:pPr>{plain_inner}</w:pPr>"));
    assert_eq!(
        before, after,
        "對無 numPr 的段落發 RemoveNumbering 必須零位元組位移（空 pPr 不得被材料化／重排）"
    );
}

/// 燈 9：未知擴展元素（`w15:` 系）必須隨前件保留、不被丟棄，也不得提調到
/// `w:pStyle` 之前（首位宣告契約）。
#[test]
fn eppr_lamp9_unknown_extension_element_follows_predecessor() {
    let inner = r#"<w:pStyle w:val="Heading1"/><w15:foo w:val="bar"/><w:spacing w:line="360" w:lineRule="auto"/>"#;
    let mut engine = engine_from_body(&three_para(inner), false);

    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            alignment: Some("center".to_string()),
            ..Default::default()
        },
    );

    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 9 實測 ===\npPr = {ppr}");

    assert!(
        ppr.contains(r#"<w15:foo w:val="bar"/>"#),
        "未知擴展宣告 `<w15:foo/>` 必須逐字存活（未知元素不可被誤當成垃圾掃掉），實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w:pStyle w:val="Heading1"/>"#)
            && ppr.contains(r#"<w:spacing w:line="360" w:lineRule="auto"/>"#),
        "未知元素兩側的既有宣告必須俱在，實測 pPr = {ppr}"
    );
    let pos = |n: &str| ppr.find(n).unwrap_or_else(|| panic!("pPr 應含 {n}：{ppr}"));
    assert!(
        pos(r#"<w:pStyle w:val="Heading1"/>"#) < pos(r#"<w15:foo"#),
        "未知擴展元素不得被提調到 w:pStyle 之前，實測 pPr = {ppr}"
    );
    let diff = engine.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "合併後仍是合法文件：{:?}", diff.violations);
}

/// 燈 10（幾何面）：`pageBreakBefore` 經單欄對齊補丁後，版面仍須承認——受標段
/// 仍自新頁版心上緣起頭（XML 寫對但版面不認＝假綠）。
#[test]
fn eppr_lamp10_page_break_before_still_lands_after_alignment_patch() {
    let body = r#"    <w:p><w:r><w:t>基準段</w:t></w:r></w:p>
    <w:p><w:r><w:t>這是一份受標文字</w:t></w:r></w:p>
    <w:p><w:r><w:t>尾段</w:t></w:r></w:p>"#;
    let mut engine = engine_from_body(body, false);

    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            page_break_before: Some(true),
            ..Default::default()
        },
    );
    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            line_spacing: Some(1.5),
            ..Default::default()
        },
    );
    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            alignment: Some("center".to_string()),
            ..Default::default()
        },
    );

    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    let dl = engine.page_display_list(1).expect("第 1 頁必須存在");
    let y = dl
        .items
        .iter()
        .find_map(|it| match it {
            quire_engine::DrawItem::Text { text, y, .. } if text.contains("這是一份") => {
                Some(*y)
            }
            _ => None,
        })
        .expect("第 1 頁必須含受標文字（pageBreakBefore 存活）");
    let box1 = engine.page_content_box(1).expect("第 1 頁版心盒");
    println!(
        "=== 燈 10 實測 ===\npPr = {ppr}\ny = {y} vs margin_top = {}",
        box1.y_pt
    );

    assert!(
        ppr.contains("<w:pageBreakBefore/>") && ppr.contains(r#"<w:jc w:val="center"/>"#),
        "pageBreakBefore 與 jc 必須相安無事地共存，實測 pPr = {ppr}"
    );
    assert!(
        (y - box1.y_pt).abs() < 0.5,
        "分頁前宣告必須仍在版面上生效（受標段自第 1 頁版心上緣起頭），實測 y={y} margin_top={}",
        box1.y_pt
    );
}

/// 燈 11：段落標記 `w:rPr`（pPr 內的那顆）與 `sectPr` 屬 pPr 尾段——單欄補丁
/// 不得把它們擠掉或寫到它們前面。
#[test]
fn eppr_lamp11_paragraph_mark_rpr_and_sectpr_keep_tail_position() {
    let inner = r#"<w:pStyle w:val="Heading1"/><w:spacing w:line="360" w:lineRule="auto"/><w:rPr><w:b/><w:sz w:val="28"/></w:rPr>"#;
    let mut engine = engine_from_body(&three_para(inner), false);

    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            alignment: Some("center".to_string()),
            ..Default::default()
        },
    );

    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 11 實測 ===\npPr = {ppr}");

    assert!(
        ppr.contains("<w:rPr><w:b/><w:sz w:val=\"28\"/></w:rPr>"),
        "段落標記 rPr 必須逐字存活，實測 pPr = {ppr}"
    );
    let pos = |n: &str| ppr.find(n).unwrap_or_else(|| panic!("pPr 應含 {n}：{ppr}"));
    assert!(
        pos(r#"<w:jc w:val="center"/>"#) < pos("<w:rPr>"),
        "CT_PPr 序：jc(26) 必須在 rPr(33) 之前，實測 pPr = {ppr}"
    );
}

/// 燈 13（agy 第二意見 P1）：被覆寫剔除的宣告仍定義其後未知元素的「隨前件」
/// 鄰域——未知元素不可因前件被 drop 而斷鏈往前漂。
#[test]
fn eppr_lamp13_unknown_element_follows_dropped_predecessor() {
    // 原序：pStyle → spacing(360) → w15:foo → tabs。發 line_spacing=Some(2.0)：
    // 舊 spacing 被同族剔除，新 spacing(480) 由補丁寫回。w15:foo 的前件是那顆
    // **被剔除的** spacing——隨前件紀律要求它留在 spacing 鄰域（tabs 之後），
    // 不可斷鏈退回 pStyle 之後（agy 實抓：drop 的 continue 發生在 last_rank
    // 更新之前，未知元素因此漂到文首）。
    let inner = r#"<w:pStyle w:val="Heading1"/><w:spacing w:line="360" w:lineRule="auto"/><w15:foo w:val="bar"/><w:tabs><w:tab w:val="left" w:pos="720"/></w:tabs>"#;
    let mut engine = engine_from_body(&three_para(inner), false);

    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            line_spacing: Some(2.0),
            ..Default::default()
        },
    );

    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 13 實測 ===\npPr = {ppr}");

    assert!(
        ppr.contains(r#"<w:spacing w:line="480" w:lineRule="auto"/>"#)
            && ppr.contains(r#"<w15:foo w:val="bar"/>"#)
            && ppr.contains(r#"<w:tab w:val="left" w:pos="720"/>"#),
        "新 spacing(480)／w15:foo／tabs 必須俱在，實測 pPr = {ppr}"
    );
    let pos = |n: &str| ppr.find(n).unwrap_or_else(|| panic!("pPr 應含 {n}：{ppr}"));
    // 斷鏈的病態形態＝w15:foo 退回 pStyle 之後（前件被 drop 後 last_rank 未更新）
    // ——它必須留在 spacing 鄰域（tabs 之後），且與新 spacing 相鄰（同 rank 組，
    // 兩者的先後由「補丁新宣告在前」紀律決定，不在本燈斷言範圍）。
    assert!(
        pos(r#"<w:pStyle w:val="Heading1"/>"#) < pos("<w:tabs>")
            && pos("<w:tabs>") < pos("<w15:foo")
            && pos("<w:tabs>") < pos(r#"<w:spacing w:line="480""#),
        "w15:foo 必須留在被覆寫前件（spacing）的鄰域、不得斷鏈漂回 tabs 之前，實測 pPr = {ppr}"
    );
    let (a, b) = (pos("<w15:foo"), pos(r#"<w:spacing w:line="480""#));
    let gap = if a < b {
        ppr[a..b].find("<w:tabs>").is_none() && ppr[a..b].find("pStyle").is_none()
    } else {
        ppr[b..a].find("<w:tabs>").is_none() && ppr[b..a].find("pStyle").is_none()
    };
    assert!(
        gap,
        "w15:foo 必須與 spacing 相鄰（其間不得插入其他已知宣告），實測 pPr = {ppr}"
    );
}

/// 燈 14（agy 第二意見 P2）：`pStyle` 覆寫後必須仍居 pPr 首位——同 rank 並存時
/// 補丁新宣告要贏過「隨前件漂移到 rank 0」的既有未知元素（CT_PPrBase 首位契約）。
#[test]
fn eppr_lamp14_replaced_pstyle_stays_first_ahead_of_inherited_unknown() {
    let inner = r#"<w:pStyle w:val="Heading1"/><w15:foo w:val="bar"/>"#;
    let mut engine = engine_from_body(&three_para(inner), false);

    para_props(
        &mut engine,
        1,
        ParaPropsPatch {
            style_id: Some("Heading2".to_string()),
            ..Default::default()
        },
    );

    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 14 實測 ===\npPr = {ppr}");

    assert!(
        ppr.starts_with(r#"<w:pPr><w:pStyle w:val="Heading2"/>"#),
        "w:pStyle 是 CT_PPrBase 首位宣告：替換後必須緊接 pPr 開標籤，未知元素不得靠隨前件 rank 0 抢位，實測 pPr = {ppr}"
    );
    assert!(
        ppr.contains(r#"<w15:foo w:val="bar"/>"#),
        "w15:foo 必須存活，實測 pPr = {ppr}"
    );
}

/// 燈 15（agy 第二意見 P3）：`ChangeLevel` 對**無 numPr** 的段落是零意圖——
/// 不得合成 `numId="0"` 的寡人宣告、不得材料化 pPr（同 RemoveNumbering 紀律）。
#[test]
fn eppr_lamp15_change_level_without_numpr_is_zero_intent() {
    let inner = r#"<w:pStyle w:val="Heading1"/><w:jc w:val="left"/>"#;
    let bytes = docx_from_body(&three_para(inner), true);
    let mut engine = Engine::from_docx_bytes(&bytes).expect("開啟失敗");
    settle(&mut engine);
    let before = engine.save_docx().expect("存檔失敗");

    numbering(&mut engine, 1, ListAction::ChangeLevel { ilvl: 2 });

    let after = engine.save_docx().expect("存檔失敗");
    let ppr = ppr_of_para(&para_xml_containing(&doc_xml(&engine), "這是一份"));
    println!("=== 燈 15 實測 ===\npPr = {ppr}");

    assert_eq!(
        ppr,
        format!("<w:pPr>{inner}</w:pPr>"),
        "無 numPr 段落發 ChangeLevel 必須零位元組變動（不可合成 numId=0 寡人宣告）"
    );
    assert_eq!(
        before, after,
        "零意圖 ChangeLevel 存檔面亦須 byte-identical"
    );
}

// ── ZIP 組裝工具（與 e1_run_patch_merge.rs 同款口徑：store 模式＋最小中央目錄）──

struct ZipTestHelper {
    buf: Vec<u8>,
    centrals: Vec<Vec<u8>>,
    count: usize,
}

impl ZipTestHelper {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            centrals: Vec::new(),
            count: 0,
        }
    }

    fn add(&mut self, name: &str, data: &[u8]) {
        let offset = self.buf.len() as u64;
        let csize = data.len() as u64;
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let crc = crc.sum();

        self.buf.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        self.buf.extend_from_slice(&20u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0x1234u16.to_le_bytes());
        self.buf.extend_from_slice(&0x5678u16.to_le_bytes());
        self.buf.extend_from_slice(&crc.to_le_bytes());
        self.buf.extend_from_slice(&(csize as u32).to_le_bytes());
        self.buf.extend_from_slice(&(csize as u32).to_le_bytes());
        self.buf
            .extend_from_slice(&(name.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(name.as_bytes());
        self.buf.extend_from_slice(data);

        let mut c = Vec::new();
        c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        c.extend_from_slice(&0x031Eu16.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0x1234u16.to_le_bytes());
        c.extend_from_slice(&0x5678u16.to_le_bytes());
        c.extend_from_slice(&crc.to_le_bytes());
        c.extend_from_slice(&(csize as u32).to_le_bytes());
        c.extend_from_slice(&(csize as u32).to_le_bytes());
        c.extend_from_slice(&(name.len() as u16).to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&(offset as u32).to_le_bytes());
        c.extend_from_slice(name.as_bytes());
        self.centrals.push(c);
        self.count += 1;
    }

    fn finish(&self) -> Vec<u8> {
        let mut out = self.buf.clone();
        let cd_off = out.len() as u64;
        for c in &self.centrals {
            out.extend_from_slice(c);
        }
        let cd_size = out.len() as u64 - cd_off;

        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(self.count as u16).to_le_bytes());
        out.extend_from_slice(&(self.count as u16).to_le_bytes());
        out.extend_from_slice(&(cd_size as u32).to_le_bytes());
        out.extend_from_slice(&(cd_off as u32).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}

/// 燈 12：`SetParagraphStyle` 失敗路徑的零改動回歸鎖（存檔 byte_identical）——
/// 本卡改的是同一條重組路徑，守衛不得被稀釋。
#[test]
fn eppr_lamp12_rejected_style_leaves_document_untouched() {
    let body = three_para(&rich_ppr());
    let bytes = docx_from_body(&body, true);
    let mut engine = Engine::from_docx_bytes(&bytes).expect("開啟失敗");
    settle(&mut engine);

    let err = engine
        .apply_mutation(Mutation::SetParagraphStyle {
            target: SubDocTarget::Body,
            para_range: 1..=1,
            patch: ParaPropsPatch {
                style_id: Some("NoSuchStyle".to_string()),
                ..Default::default()
            },
        })
        .expect_err("不存在的樣式 id 必須被拒絕");
    assert!(
        matches!(
            err,
            quire_engine::MutationError::InvalidStyleReference { .. }
        ),
        "未知樣式 id 必須回 InvalidStyleReference，實測 = {err:?}"
    );

    let saved = engine.save_docx().expect("存檔失敗");
    let diff = diff_untouched(&bytes, &saved).expect("diff_untouched 失敗");
    assert!(
        diff.byte_identical,
        "失敗後 save_docx() 必須與原檔 byte_identical（守衛未因欄位級合併而失效）"
    );
}
