//! ODF 正文投影剝除 XML 格式化空白（卡 t_ae9a9f22）紅燈考卷。
//!
//! 一句話契約：`content.xml` 文字節點首尾的 XML 格式化換行／製表符（pretty-print
//! 縮排產物）不得進入段落文字、不得被斷行器當成硬換行。ODF 正文換行只認
//! `<text:line-break/>`、製表只認 `<text:tab/>`、空白只認 `<text:s/>`
//! （OASIS ODF 1.3 Part 3 §5.1.1）。
//!
//! 癥狀（用戶實測檔，政府公文樣式 ODT；本考卷一律用自造 fixture，零真實內文）：
//! 我方第 0 頁發射 48 個文字項中 36 個是空字串項（幽靈空行），該頁只排進源檔前
//! 8 段就撞 45 行上限斷頁。根因＝走查層把 pretty-print 的 `\n\t\t\t` 原樣 push
//! 進段落字串，斷行器把每個 `\n` 當成強制斷行。
//!
//! 口徑出處：`docs/odt-userdoc-layout-diag.md`（軸 2＋軸 4）；節點級裁剪比照
//! DOCX 線 `docs/e1cd-xml-space-policy.md` §2（只剝首尾、節點內連續空格不折疊
//! ——LO rawdict 實測不折疊，雙空格渲染兩個空格字形）。
//!
//! 六盞燈：
//! 1. 正規化純函數單元（首尾剝除／內部折單空格／不黏死）。
//! 2. 投影級：pretty-print 縮排不得進段落文字，節點間格式化空白＝恰一個空格。
//! 3. 斷行級（本卡核心紅燈）：同 fixture 該段行數 `== 1`（修前必紅）。
//! 4. 發射級：顯示列表不含正規化後長度為零的幽靈 `DrawItem::Text`，
//!    且真空段仍恰留 1 個空行（防砍過頭把真空段垂直間距一起刪掉）。
//! 5. 顯式元素不被誤傷：含 `<text:line-break/>` 的段落仍產 2 行。
//! 6. 回塞鑑別：`ODF_WS_BUGGY=1` 時燈 3／燈 4 必紅（證明兩盞燈真的在看正規化）。

use quire_odf::display::{emit_display_lists, DrawItem, OdtView};
use quire_odf::layout_glue::{layout_projection, normalize_xml_text, project_bytes, LayoutParams};

/// 最小合法 `content.xml` 殼（前綴宣告齊備；`walk_paragraphs` 按 `text:p`／
/// `text:span` 限定名比對，故前綴必須寫成 `text:`）。
fn content(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
 <office:body>
  <office:text>
{body}
  </office:text>
 </office:body>
</office:document-content>
"#
    )
}

fn params() -> LayoutParams {
    LayoutParams::default()
}

/// 本卡核心 fixture：段落被 pretty-print 縮排包圍，兩個 `text:span` 之間只有
/// 格式化空白（無 `<text:s/>`）。正規化後段文字必為 `甲 乙`（恰一空格、無換行）。
const SPACED_BODY: &str =
    "   <text:p>\n    <text:span>甲</text:span>\n    <text:span>乙</text:span>\n   </text:p>";

// ---------------------------------------------------------------------------
// 燈 1：正規化純函數單元（純字串進出，不必組 ODT 容器）
// ---------------------------------------------------------------------------

#[test]
fn lamp1_normalize_xml_text_strips_formatting_whitespace() {
    // 卡面錨點 1：首尾換行＋製表符＋空格一律剝除。
    assert_eq!(normalize_xml_text("\n\t  標題文字  \n  "), "標題文字");
    // 卡面錨點 2：節點內部含 \n／\t 的連續空白折成單個半形空格——不是換行、不黏死。
    assert_eq!(normalize_xml_text("第一行\n\t第二行"), "第一行 第二行");
    // 首尾只有**純空格**＝有意義空格（語料實態：LO 寫檔 `Prep 2 ` 這類段尾空格），
    // 不是 pretty-print 噪音 ⇒ 原樣保留，不得誤砍。
    assert_eq!(normalize_xml_text("Prep 2 "), "Prep 2 ");
    assert_eq!(normalize_xml_text(" Section"), " Section");
    // 節點內**純空格**連續段原樣保留（LO rawdict 實測不折疊雙空格，見
    // docs/e1cd-xml-space-policy.md §1——折疊即與真值背道）。
    assert_eq!(normalize_xml_text("AAAA  BBBB"), "AAAA  BBBB");
    // 混合：節點內**同一個**連續空白段只要含 \n／\r／\t ⇒ 整段折成恰一個半形空格
    // （卡面契約「內部連續空白折單空格」；純空格段不折疊見上一條，兩者不衝突）。
    assert_eq!(normalize_xml_text("A  \n\t  B"), "A B");
    // 整節點純空白 → 空字串（「節點間單空格」語義由 ParaCtx 的待發空白承載，燈 2 驗）。
    assert_eq!(normalize_xml_text("\n\t  \n"), "");
    assert_eq!(normalize_xml_text(""), "");
    // NBSP／類空格是**正文字元**，動到即吞字（語料實測存在）⇒ 一位不動。
    assert_eq!(normalize_xml_text("\u{00a0}X\u{2007}"), "\u{00a0}X\u{2007}");
    // 正規化冪等：二次正規化不再變動（防「折完又折」造成的二次位移）。
    let once = normalize_xml_text("  \n 甲 \t\n 乙  \n");
    assert_eq!(once.as_str(), "甲  乙");
    assert_eq!(normalize_xml_text(&once), once);
}

// ---------------------------------------------------------------------------
// 燈 2：投影級——pretty-print 縮排不得進段落文字，節點間空白＝恰一個空格
// ---------------------------------------------------------------------------

#[test]
fn lamp2_projection_drops_formatting_whitespace_and_keeps_one_gap_space() {
    let proj = project_bytes(content(SPACED_BODY).as_bytes(), None).expect("投影失敗");
    assert_eq!(
        proj.paragraph_count(),
        1,
        "pretty-print 縮排不得生出額外段落"
    );
    assert_eq!(
        proj.paragraphs[0].text, "甲 乙",
        "節點間格式化空白必須恰折成一個空格（不是零、也不是換行）"
    );
    assert!(
        !proj.paragraphs[0].text.contains('\n') && !proj.paragraphs[0].text.contains('\t'),
        "段落文字殘留硬換行／製表符：{:?}",
        proj.paragraphs[0].text
    );
    // 計數口徑：正規化後碼點數＝4（甲、空格、乙 共 3 碼點？實為 3）——釘死防膨脹。
    assert_eq!(
        proj.total_text_len,
        proj.paragraphs[0].text.chars().count(),
        "total_text_len 必須是正規化後的碼點和（幽靈空白不計）"
    );
    assert_eq!(proj.total_text_len, 3, "甲＋單空格＋乙＝3 碼點");
    // 段首／段尾縮排不得留幽靈空格（兩個 `text:p` 各自獨立成段）。
    let two = project_bytes(
        content("   <text:p>\n    <text:span>甲</text:span>\n   </text:p>\n   <text:p>\n    乙\n   </text:p>")
            .as_bytes(),
        None,
    )
    .expect("投影失敗");
    assert_eq!(two.paragraphs[0].text, "甲", "段首／段尾縮排不得留幽靈空格");
    assert_eq!(two.paragraphs[1].text, "乙", "段首／段尾縮排不得留幽靈空格");
}

// ---------------------------------------------------------------------------
// 燈 3：斷行級（本卡核心紅燈）——該段落行數 == 1
// ---------------------------------------------------------------------------

#[test]
fn lamp3_pretty_printed_paragraph_breaks_to_exactly_one_line() {
    let proj = project_bytes(content(SPACED_BODY).as_bytes(), None).expect("投影失敗");
    let pls = layout_projection(&proj, &params());
    assert_eq!(pls.len(), 1, "layout 段落數必須等於投影段落數");
    let lines: Vec<&str> = pls[0].lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        pls[0].lines.len(),
        1,
        "pretty-print 縮排曾被斷行器當成強制斷行（修前實測切出多行）；實得行集 {lines:?}"
    );
    assert_eq!(
        lines,
        vec!["甲 乙"],
        "唯一一行的文字必須是正規化後的段落全文"
    );
}

// ---------------------------------------------------------------------------
// 燈 4：發射級——幽靈空字串項不發，真空段仍恰留 1 個空行
// ---------------------------------------------------------------------------

/// 取某頁「正規化後長度為零」的文字項數（正規化＝折疊空白；判定口徑與發射層一致）。
fn empty_text_items(lists: &[quire_render_trait::PageDisplayList]) -> usize {
    lists
        .iter()
        .flat_map(|dl| dl.items.iter())
        .filter(|it| match it {
            DrawItem::Text { text, .. } => text.chars().all(|c| c.is_whitespace()),
            _ => false,
        })
        .count()
}

#[test]
fn lamp4_emitter_drops_ghost_empty_items_and_keeps_true_empty_paragraph_line() {
    // (a) 純 pretty-print 段落：該頁不得有任何零長度／純空白文字項。
    let proj = project_bytes(content(SPACED_BODY).as_bytes(), None).expect("投影失敗");
    let lists = emit_display_lists(&proj, &params());
    assert!(!lists.is_empty(), "至少要有一頁");
    assert_eq!(
        empty_text_items(&lists),
        0,
        "幽靈空行（正規化後長度為零）不得發射 DrawItem::Text；實發 {:?}",
        lists
            .iter()
            .flat_map(|d| d.items.iter())
            .collect::<Vec<_>>()
    );

    // (b) 真空段：恰留 1 個空行維持垂直間距（防砍過頭）。
    let proj_empty = project_bytes(content("   <text:p/>").as_bytes(), None).expect("投影失敗");
    assert_eq!(
        proj_empty.paragraphs[0].text, "",
        "自閉合 text:p 是零文字段"
    );
    let lists_empty = emit_display_lists(&proj_empty, &params());
    assert_eq!(
        empty_text_items(&lists_empty),
        1,
        "真空段必須恰發 1 個空文字項（維持垂直間距）"
    );

    // (c) 混排：pretty-print 段＋真空段 ⇒ 空項數仍恰為 1（幽靈零發射、真空段不誤砍）。
    let mixed = project_bytes(
        content(&format!("{SPACED_BODY}\n   <text:p>\n   </text:p>")).as_bytes(),
        None,
    )
    .expect("投影失敗");
    assert_eq!(mixed.paragraph_count(), 2, "pretty-print 不得生幽靈段落");
    assert_eq!(
        mixed.paragraphs[1].text, "",
        "真空段仍為空字串（正規化不誤砍真空段）"
    );
    let lists_mixed = emit_display_lists(&mixed, &params());
    assert_eq!(
        empty_text_items(&lists_mixed),
        1,
        "混排態：真空段 1 個空行、幽靈項 0 個；實發 {:?}",
        lists_mixed
            .iter()
            .flat_map(|d| d.items.iter())
            .collect::<Vec<_>>()
    );

    // (d) OdtView 快取面與直發面同源（正規化改動不得讓兩側分岔）。
    let view_lists = OdtView::from_projection(mixed.clone(), &params());
    for i in 0..lists_mixed.len() {
        assert_eq!(
            view_lists.page_display_list_json(i),
            quire_odf::display::display_list_json(&lists_mixed, i),
            "OdtView 與直發面第 {i} 頁必須逐字相同"
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 5：顯式元素不被誤傷——<text:line-break/> 仍產 2 行
// ---------------------------------------------------------------------------

#[test]
fn lamp5_explicit_line_break_still_produces_two_lines() {
    // 顯式換行前後都包圍 pretty-print 縮排：正規化只剝格式化空白，真換行必須留住。
    let body = "   <text:p>\n    上行\n    <text:line-break/>\n    下行\n   </text:p>";
    let proj = project_bytes(content(body).as_bytes(), None).expect("投影失敗");
    assert_eq!(
        proj.paragraphs[0].text, "上行\n下行",
        "顯式 text:line-break 必須保留為 \\n，其周圍縮排剝除"
    );
    let pls = layout_projection(&proj, &params());
    let lines: Vec<&str> = pls[0].lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        pls[0].lines.len(),
        2,
        "顯式換行段落必須產 2 行；實得 {lines:?}"
    );
    assert_eq!(lines, vec!["上行", "下行"], "兩行文字逐字對照");

    // 顯式 text:s（text:c 計數）與 text:tab 同樣不受正規化影響。
    let explicit = project_bytes(
        content("   <text:p>\n    A<text:s/>B<text:s text:c=\"3\"/>C<text:tab/>\n   </text:p>")
            .as_bytes(),
        None,
    )
    .expect("投影失敗");
    assert_eq!(
        explicit.paragraphs[0].text, "A B   C\t",
        "text:s／text:s@c=3／text:tab 的顯式空白計數必須原樣生效"
    );

    // 顯式換行後的縮排是**該行行首**噪音：不得刷成行首多一個空格。
    let after_break = project_bytes(
        content("   <text:p>上行<text:line-break/>\n      下行</text:p>").as_bytes(),
        None,
    )
    .expect("投影失敗");
    assert_eq!(
        after_break.paragraphs[0].text, "上行\n下行",
        "硬換行之後的 pretty-print 縮排不得變成行首空格"
    );
}

// ---------------------------------------------------------------------------
// 燈 6：回塞鑑別鎖——ODF_WS_BUGGY=1 把正規化呼叫退化為直接 push_str
// ---------------------------------------------------------------------------

#[test]
fn lamp6_buggy_reinjection_is_distinguishable() {
    if std::env::var("ODF_WS_BUGGY").is_err() {
        // 正常態：燈 3／燈 4 的判定條件必須成立（否則本燈只是空轉）。
        let proj = project_bytes(content(SPACED_BODY).as_bytes(), None).expect("投影失敗");
        let pls = layout_projection(&proj, &params());
        let lists = emit_display_lists(&proj, &params());
        assert_eq!(pls[0].lines.len(), 1, "正常態：正規化後該段恰 1 行");
        assert_eq!(empty_text_items(&lists), 0, "正常態：零幽靈空項");
        return;
    }
    // 回塞態：正規化呼叫被改回直接 push_str ⇒ 燈 3（行數）與燈 4（幽靈項）必紅。
    let proj = project_bytes(content(SPACED_BODY).as_bytes(), None).expect("投影失敗");
    let pls = layout_projection(&proj, &params());
    let lines: Vec<&str> = pls[0].lines.iter().map(|l| l.text.as_str()).collect();
    assert_ne!(
        pls[0].lines.len(),
        1,
        "BUGGY 態若仍只 1 行 ⇒ 燈 3 沒有鑑別力（段文字={:?} 行集={lines:?}",
        proj.paragraphs[0].text
    );
    let lists = emit_display_lists(&proj, &params());
    assert_ne!(
        empty_text_items(&lists),
        0,
        "BUGGY 態若零幽靈項 ⇒ 燈 4 沒有鑑別力"
    );
}

use quire_render_trait;
