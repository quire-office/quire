//! K29e 紅燈考卷：字元格式六欄（底線／刪除線／上下標／highlight／字型族）
//! 進 `RunDirectPatch` 的完整鏈閉環。
//!
//! 斷言層口徑（卡面鐵律）：字元格式類斷言**必讀渲染端 SVG 產物**
//! （`shells/native/src/render_svg.rs` 的 `page_to_svg`，與 `quire-cli render`
//! 同一支發射器）——底線/刪除線是 `<text>` 的 `text-decoration`、highlight
//! 是 `<rect>`、上下標是 baseline y 與 `font-size`；display list 斷言只當
//! 輔助證據（「斷言打在 page_display_list 而發射層未消費旗標照樣全綠」有前例）。
//!
//! LO 真值口徑（2026-11-13 Linux LO 25.8 探針，docs/k29e-redlight.md §2）：
//! - 上/下標字級 = 0.582×base（11pt→6.4pt 實測）；上標基線上移 0.35×base、
//!   下標下移 1.0pt@11pt（0.0909×base）。
//! - **行高不膨脹**：含 sup/sub 的行 auto 行距 pitch 與純正文行逐行全等
//!   （12.65pt）。卡面「上下標會改行高」前提經 LO 實測推翻——本卡照真值
//!   實作（位移只動基線 y，不動 layout 行高），更正逐條記 docs。
//!
//! 移除語意的兩條路徑（本卡寫進文件，不是第三態）：
//! - 移除類值（`Some(false)`／`UnderlineKind::None`／`StrikeKind::None`／
//!   `VertAlign::Baseline`）＝**顯式關閉**，寫出 `<w:x w:val="0|none"/>`
//!   形態，直接層贏過樣式級聯（Word 口徑）；
//! - `highlight: Some(alpha-0)` 例外：解析層既有口徑把 `w:highlight w:val="none"`
//!   老實讀成「未宣告」（styles.rs `apply_run_tag` highlight 臂註解），故移除
//!   直接層高亮後**樣式級高亮會重新顯形**——燈 6 照此實斷，不虛構第四態。

use quire_engine::{
    DocPos, DocRange, DrawItem, Engine, Mutation, RunDirectPatch, StrikeKind, SubDocTarget,
    UnderlineKind, VertAlign, FLAG_BOLD, FLAG_STRIKE, FLAG_UNDERLINE,
};

#[path = "../../../shells/native/src/render_svg.rs"]
mod render_svg;

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/k29e_char_format.docx"
    ))
    .expect("讀 fixtures/k29e_char_format.docx 失敗")
}

fn fixture_engine() -> Engine {
    let mut engine = Engine::from_docx_bytes(&fixture_bytes())
        .expect("開啟 fixtures/k29e_char_format.docx 失敗");
    settle(&mut engine);
    engine
}

fn svg_of(engine: &Engine, page: usize) -> String {
    let dl = engine
        .page_display_list(page)
        .expect("第 0 頁顯示列表必須存在");
    render_svg::page_to_svg(&dl)
}

fn fmt_para(engine: &mut Engine, para: usize, patch: RunDirectPatch) {
    engine
        .apply_mutation(Mutation::FormatTextRange {
            range: DocRange {
                target: SubDocTarget::Body,
                start: DocPos { para, cp: 0 },
                end: DocPos { para, cp: 10 },
            },
            patch,
        })
        .expect("套用 M2.1 FormatTextRange 失敗");
}

/// 抽出顯示列表裡第一條含 `needle` 的 Text 項（x, y, size_pt, flags, family）。
fn first_text_with(dl_items: &[DrawItem], needle: &str) -> (f32, f32, f32, u8, Option<String>) {
    dl_items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                flags,
                family,
                text,
                ..
            } if text.contains(needle) => Some((*x, *y, *size_pt, *flags, family.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("顯示列表必須含文字片段 {needle:?}"))
}

/// 燈 1：六欄「設定→存檔→重解析→讀回」閉環（第 2 段零宣告 run）。
/// (a) 存檔件 XML 明文逐欄帶六個標籤；(b) 重開件級聯重解析後發射層（SVG）
/// 消費六欄——底線/刪除線進 text-decoration、下標進 font-size 與基線 y、
/// highlight 進底條 rect、字型族進 font-family。
#[test]
fn k29e_lamp1_six_fields_roundtrip_closed_loop() {
    let mut engine = fixture_engine();
    fmt_para(
        &mut engine,
        2,
        RunDirectPatch {
            underline: Some(UnderlineKind::Dotted),
            strike: Some(StrikeKind::Single),
            vert_align: Some(VertAlign::Subscript),
            highlight: Some(0xFFFF_FF00),
            font_family: Some("Arial".to_string()),
            ..Default::default()
        },
    );

    // (a) 存檔件 XML 明文：六欄標籤逐欄真實存在。
    let saved = engine.save_docx().expect("存檔失敗");
    let reopened_xml = {
        let reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
        String::from_utf8(
            reopened
                .part("word/document.xml")
                .expect("重開件必須有 document.xml")
                .to_vec(),
        )
        .expect("UTF-8")
    };
    for tag in [
        r#"<w:u w:val="dotted"/>"#,
        "<w:strike/>",
        r#"<w:vertAlign w:val="subscript"/>"#,
        r#"<w:highlight w:val="yellow"/>"#,
        r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:cs="Arial" w:eastAsia="Arial"/>"#,
    ] {
        assert!(
            reopened_xml.contains(tag),
            "存檔件必須含 {tag}；document.xml 前 800 字元：{}",
            &reopened_xml[..reopened_xml.len().min(800)]
        );
    }

    // (b) 重開件渲染端（SVG＋顯示列表）消費斷言——發射層真吃旗標。
    let mut reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
    settle(&mut reopened);
    let dl = reopened
        .page_display_list(0)
        .expect("第 0 頁顯示列表必須存在");
    let (tx, ty, t_size, t_flags, t_family) = first_text_with(&dl.items, "六欄目標段");
    assert_ne!(t_flags & FLAG_UNDERLINE, 0, "底線旗標必須進發射段 flags");
    assert_ne!(t_flags & FLAG_STRIKE, 0, "刪除線旗標必須進發射段 flags");
    assert_eq!(
        t_family.as_deref(),
        Some("Arial"),
        "字型族級聯重解析後必須烘進發射宣告（斷行尺與宣告同族契約）"
    );
    // 下標：字級 0.58×base＋基線較未編輯同段態下移（LO 真值口徑；既有
    // 缺陷＝下標全尺不位移——斷言打在未編輯基準的位移差，不跨段比 y）。
    let base_size = 11.0_f32;
    assert!(
        (t_size - base_size * 0.58).abs() < 0.01,
        "下標段字級必須 0.58×base（實得 {t_size}）"
    );
    let base_engine = fixture_engine();
    let base_dl = base_engine
        .page_display_list(0)
        .expect("基準第 0 頁必須存在");
    let (_, base_y, ..) = first_text_with(&base_dl.items, "六欄目標段");
    assert!(
        ty - base_y > 0.5,
        "下標基線必須較未編輯態下移（下標 y={ty} 對照基準 y={base_y}）"
    );

    let svg = svg_of(&reopened, 0);
    let text_tag = svg
        .lines()
        .find(|l| l.contains("<text ") && l.contains("六欄目標段"))
        .expect("SVG 必須含六欄目標段 <text>");
    assert!(
        text_tag.contains("text-decoration=\"underline line-through\""),
        "SVG 底線＋刪除線必須走 text-decoration（實得：{text_tag}）"
    );
    assert!(
        text_tag.contains("font-family=\"Arial\""),
        "SVG 必須宣告 Arial（實得：{text_tag}）"
    );
    // highlight 底條：發射 Rect、顏色＝黃、x 對齊文字起點。
    let rect = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } if *fill_argb == 0xFFFF_FF00 => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .expect("highlight 必須發射黃底 Rect（發射層底紋）");
    assert!(
        (rect.0 - tx).abs() < 0.01,
        "底條 x 必須對齊文字起點（rect {} vs text {tx}）",
        rect.0
    );
    assert!(
        rect.2 > 0.0 && rect.2 < 300.0,
        "底條寬必須匹配文字實際寬（>0 且不逾版面；實得 {}）",
        rect.2
    );
}

/// 燈 2：`None`＝保持原宣告——全 None patch 後 **document.xml part 逐位元組
/// 原樣**（寫入路徑紀律的硬鎖：patch 無任何明確指定時，連空 `<w:rPr/>` 都
/// 不得材料化、既有 rPr 不得重排；破鎖＝未編輯路徑被編輯層代碼洩入）。
#[test]
fn k29e_lamp2_all_none_patch_keeps_part_byte_identical() {
    let pristine = fixture_engine();
    let original_xml = pristine
        .part("word/document.xml")
        .expect("fixture 必須有 document.xml")
        .to_vec();

    // 零宣告 run 段與帶 rPr（rStyle）段兩處都做**整段全選**（cp 0..全長）：
    // 整段全選時選取後段為空、不產生 run 裂解（k29a 中段裂解紀律不在本燈
    // 範圍），全 None patch 的合法產物就是 part 逐位元組原樣。破鎖形態＝
    // 空 <w:rPr></w:rPr> 被材料化或既有 rPr 被重排。
    for (para, len) in [(2usize, 15usize), (1usize, 15usize)] {
        let mut engine = fixture_engine();
        engine
            .apply_mutation(Mutation::FormatTextRange {
                range: DocRange {
                    target: SubDocTarget::Body,
                    start: DocPos { para, cp: 0 },
                    end: DocPos { para, cp: len },
                },
                patch: RunDirectPatch::default(),
            })
            .expect("套用 M2.1 FormatTextRange 失敗");
        let saved = engine.save_docx().expect("存檔失敗");
        let reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
        let xml = reopened
            .part("word/document.xml")
            .expect("重開件必須有 document.xml")
            .to_vec();
        assert_eq!(
            xml, original_xml,
            "第 {para} 段全 None patch 後 document.xml 必須逐位元組原樣\
             （寫入路徑只在明確指定時動 XML）"
        );
    }
}

/// 燈 3：底線五形態與刪除線雙線——存檔件 XML 逐值對應；渲染端設定刪除線
/// 後 SVG 必發 line-through（發射層真消費，XML 口徑是欄位契約讀回證據）。
#[test]
fn k29e_lamp3_underline_kinds_and_double_strike() {
    let cases: [(UnderlineKind, &str); 5] = [
        (UnderlineKind::Single, r#"<w:u w:val="single"/>"#),
        (UnderlineKind::Double, r#"<w:u w:val="double"/>"#),
        (UnderlineKind::Dotted, r#"<w:u w:val="dotted"/>"#),
        (UnderlineKind::Dashed, r#"<w:u w:val="dashed"/>"#),
        (UnderlineKind::None, r#"<w:u w:val="none"/>"#),
    ];
    for (kind, tag) in cases {
        let mut engine = fixture_engine();
        fmt_para(
            &mut engine,
            2,
            RunDirectPatch {
                underline: Some(kind),
                ..Default::default()
            },
        );
        let saved = engine.save_docx().expect("存檔失敗");
        let reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
        let xml = String::from_utf8(
            reopened
                .part("word/document.xml")
                .expect("document.xml")
                .to_vec(),
        )
        .expect("UTF-8");
        assert!(xml.contains(tag), "底線 {kind:?} 必須寫出 {tag}");
    }
    // 刪除線雙線：strike＋dstrike 成對；關閉顯式 val=0。
    let mut engine = fixture_engine();
    fmt_para(
        &mut engine,
        2,
        RunDirectPatch {
            strike: Some(StrikeKind::Double),
            ..Default::default()
        },
    );
    let saved = engine.save_docx().expect("存檔失敗");
    let reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
    let xml = String::from_utf8(
        reopened
            .part("word/document.xml")
            .expect("document.xml")
            .to_vec(),
    )
    .expect("UTF-8");
    assert!(
        xml.contains("<w:strike/><w:dstrike/>"),
        "雙線刪除線必須成對寫 strike＋dstrike"
    );

    let mut engine = fixture_engine();
    fmt_para(
        &mut engine,
        2,
        RunDirectPatch {
            strike: Some(StrikeKind::None),
            ..Default::default()
        },
    );
    let saved = engine.save_docx().expect("存檔失敗");
    let reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
    let xml = String::from_utf8(
        reopened
            .part("word/document.xml")
            .expect("document.xml")
            .to_vec(),
    )
    .expect("UTF-8");
    assert!(
        xml.contains(r#"<w:strike w:val="0"/>"#),
        "移除刪除線必須顯式 val=0"
    );

    // 渲染端：設定單線刪除線後 SVG 該段必帶 line-through 裝飾（基準態本段
    // 已有 docDefaults 底線，SVG 合成 `underline line-through`——只鎖
    // line-through 子串，兩形態通吃）。
    let mut engine = fixture_engine();
    fmt_para(
        &mut engine,
        2,
        RunDirectPatch {
            strike: Some(StrikeKind::Single),
            ..Default::default()
        },
    );
    settle(&mut engine);
    let svg = svg_of(&engine, 0);
    let tag = svg
        .lines()
        .find(|l| l.contains("<text ") && l.contains("六欄目標段01234"))
        .expect("SVG 必須含該段選取段");
    assert!(
        tag.contains("line-through"),
        "設定刪除線後 SVG 該段必須帶 line-through（實得 {tag}）"
    );
}

/// 燈 4：上下標對基線 y 與字級的影響（SVG 產物斷言，LO 真值常數）。
/// 上標：size=0.58×base、基線上移；下標：同字級縮放、基线下移。
/// 兩態的**行 pitch 不變**（後續段基線 y 不受上下標撐高——LO 真值坐實）。
#[test]
fn k29e_lamp4_super_sub_baseline_and_size() {
    for (va, expect_shift_up) in [
        (VertAlign::Superscript, true),
        (VertAlign::Subscript, false),
    ] {
        let mut engine = fixture_engine();
        fmt_para(
            &mut engine,
            2,
            RunDirectPatch {
                vert_align: Some(va),
                ..Default::default()
            },
        );
        settle(&mut engine);
        let dl = engine.page_display_list(0).expect("第 0 頁必須存在");
        let (_, ty, t_size, ..) = first_text_with(&dl.items, "六欄目標段");
        // 基準態＝未編輯同檔：同段選取段基線 y（該段首行行頂）。
        let base_engine = fixture_engine();
        let base_dl = base_engine.page_display_list(0).expect("第 0 頁必須存在");
        let (_, base_y, plain_size, ..) = first_text_with(&base_dl.items, "六欄目標段");
        assert!(
            (t_size - plain_size * 0.58).abs() < 0.01,
            "{va:?} 段字級必須 0.58×base（{t_size} vs {plain_size}）"
        );
        let shift = ty - base_y;
        if expect_shift_up {
            assert!(
                (shift + plain_size * 0.35).abs() < 0.6,
                "上標基線必須上移 ≈0.35×base（shift={shift}，實得 y={ty} vs 基準 {base_y}）"
            );
        } else {
            assert!(
                (shift - plain_size / 11.0).abs() < 0.6,
                "下標基線必須下移 ≈1.0pt@11pt（shift={shift}，實得 y={ty} vs 基準 {base_y}）"
            );
        }
        // 行高零膨脹：後續段基線 y 不因上下標移動。
        let (_, fill_y, ..) = first_text_with(&base_dl.items, "版面填充段");
        let (_, shifted_fill_y, ..) = first_text_with(&dl.items, "版面填充段");
        assert!(
            (fill_y - shifted_fill_y).abs() < 0.01,
            "上下標不得撐高行距：後續段基線 {shifted_fill_y} vs 基準 {fill_y}"
        );
        // SVG 產物同口徑：font-size 縮放進 <text>。
        let svg = svg_of(&engine, 0);
        let text_tag = svg
            .lines()
            .find(|l| l.contains("<text ") && l.contains("六欄目標段"))
            .expect("SVG 必須含六欄目標段");
        let want = format!("font-size=\"{:.1}\"", plain_size * 0.58);
        assert!(
            text_tag.contains(&want),
            "SVG {va:?} font-size 必須縮放（找 {want}，實得 {text_tag}）"
        );
    }
}

/// 燈 5：highlight 雙路徑（命名色→w:highlight、非命名色→w:shd）＋底條
/// 寬度匹配文字實際寬（歷史高發缺陷區：底條必須吃**實測**段寬）。
#[test]
fn k29e_lamp5_highlight_named_shd_and_width_match() {
    // 命名色路徑。
    let mut engine = fixture_engine();
    fmt_para(
        &mut engine,
        2,
        RunDirectPatch {
            highlight: Some(0xFF00_8000),
            ..Default::default()
        },
    );
    let saved = engine.save_docx().expect("存檔失敗");
    let reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
    let xml = String::from_utf8(
        reopened
            .part("word/document.xml")
            .expect("document.xml")
            .to_vec(),
    )
    .expect("UTF-8");
    assert!(
        xml.contains(r#"<w:highlight w:val="darkgreen"/>"#),
        "命名色 darkgreen 必須走 w:highlight 枚舉"
    );

    // 非命名色路徑（w:shd 底紋）＋存檔重開後底條 Rect 實測寬。
    let mut engine = fixture_engine();
    fmt_para(
        &mut engine,
        2,
        RunDirectPatch {
            highlight: Some(0xFFAB_CD12),
            ..Default::default()
        },
    );
    let bytes = engine.save_docx().expect("存檔失敗");
    {
        let probe = Engine::from_docx_bytes(&bytes).expect("重開失敗");
        let xml = String::from_utf8(
            probe
                .part("word/document.xml")
                .expect("document.xml")
                .to_vec(),
        )
        .expect("UTF-8");
        assert!(
            xml.contains(r#"<w:shd w:val="clear" w:color="auto" w:fill="ABCD12"/>"#),
            "非命名色必須走 w:shd 底紋路徑"
        );
    }
    let mut reopened = Engine::from_docx_bytes(&bytes).expect("重開失敗");
    settle(&mut reopened);
    let dl = reopened.page_display_list(0).expect("第 0 頁必須存在");
    let (tx, _, _, _, _) = first_text_with(&dl.items, "六欄目標段");
    let rect = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } if *fill_argb == 0xFFAB_CD12 => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .expect("非命名色底紋重開後必須發射同色 Rect");
    assert!(
        (rect.0 - tx).abs() < 0.01,
        "底條 x 必須對齊文字起點（{} vs {tx}）",
        rect.0
    );
    // 「六欄目標段01234」＝5 全形＋5 數字@11pt：合理帶 [50,160]pt；
    // 固定寬（缺段寬匹配）與缺寬都會咬這條。
    assert!(
        rect.2 > 50.0 && rect.2 < 160.0,
        "底條寬必須匹配十碼文字實際寬（實得 {}）",
        rect.2
    );
    // SVG 產物口徑：底條落 <rect>。
    let svg = svg_of(&reopened, 0);
    assert!(
        svg.contains("fill=\"#abcd12\""),
        "SVG 必須發底條 rect（實得片段：{}）",
        &svg[..svg.len().min(400)]
    );
}

/// 燈 6：移除直接格式回退級聯（卡面必須考卷條目）。
/// 第 2 段級聯基底＝docDefaults（b＋u single）。施加「移除類值」四欄
/// （bold 不碰）後：
/// - underline 顯式關閉贏過 docDefaults → 旗標熄滅；
/// - bold 未被觸碰 → 仍從 docDefaults 顯形（級聯鏈零破壞）；
/// - 第 1 段（級聯回退段，格式全來自字元樣式＋段落樣式）不受連坐。
#[test]
fn k29e_lamp6_removal_falls_back_to_style_cascade() {
    // 基準態 A：第 2 段帶級聯底線（docDefaults u single）。
    let base = fixture_engine();
    let dl = base.page_display_list(0).expect("第 0 頁必須存在");
    let (_, _, _, base_flags, _) = first_text_with(&dl.items, "六欄目標段");
    assert_ne!(
        base_flags & FLAG_UNDERLINE,
        0,
        "基準態第 2 段必須自 docDefaults 級聯取得底線"
    );
    // 基準態 B：第 1 段級聯格式全在（strike 旗標＋黃底＋上標縮級）。
    let (_, _, k_size, k_flags, _) = first_text_with(&dl.items, "級聯回退段");
    assert_ne!(k_flags & FLAG_STRIKE, 0, "基準態字元樣式刪除線必須在");
    assert!(
        dl.items.iter().any(|it| matches!(
            it,
            DrawItem::Rect { fill_argb, .. } if *fill_argb == 0xFFFF_FF00
        )),
        "基準態字元樣式黃底必須在"
    );
    assert!(
        (k_size - 11.0 * 0.58).abs() < 0.01,
        "基準態段落樣式上標必須縮級（實得 {k_size}）"
    );

    // 移除態：第 2 段四欄顯式關閉（bold 不碰）。
    let mut engine = fixture_engine();
    fmt_para(
        &mut engine,
        2,
        RunDirectPatch {
            underline: Some(UnderlineKind::None),
            strike: Some(StrikeKind::None),
            vert_align: Some(VertAlign::Baseline),
            highlight: Some(0x00FF_FF00),
            ..Default::default()
        },
    );
    settle(&mut engine);
    let dl = engine.page_display_list(0).expect("第 0 頁必須存在");
    let (flags, bold_visible) = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text { text, flags, .. } if text.contains("六欄目標段") => {
                Some((*flags, (*flags & FLAG_BOLD) != 0))
            }
            _ => None,
        })
        .expect("第 2 段必須在顯示列表");
    assert_eq!(
        flags & FLAG_UNDERLINE,
        0,
        "顯式移除底線後旗標必須熄滅（直接層顯式關閉贏過 docDefaults）"
    );
    assert_eq!(flags & FLAG_STRIKE, 0, "顯式移除刪除線後旗標必須熄滅");
    assert!(
        bold_visible,
        "bold 未被觸碰：必須仍從 docDefaults 級聯顯形（級聯鏈零破壞）"
    );
    // 第 1 段（樣式級）不受連坐：strike／黃底／上標全在。
    let (_, _, k_size, k_flags, _) = first_text_with(&dl.items, "級聯回退段");
    assert_ne!(k_flags & FLAG_STRIKE, 0, "第 1 段字元樣式刪除線不得被連坐");
    assert!(
        (k_size - 11.0 * 0.58).abs() < 0.01,
        "第 1 段上標縮級不受第 2 段編輯連坐"
    );

    // 渲染端口徑：SVG 該段不再帶 underline 裝飾（其他段不受累）。
    let svg = svg_of(&engine, 0);
    let tag = svg
        .lines()
        .find(|l| l.contains("<text ") && l.contains("六欄目標段"))
        .expect("SVG 必須含該段");
    assert!(
        !tag.contains("underline"),
        "移除後 SVG 該段不得再帶底線裝飾（實得 {tag}）"
    );
    let k_tag = svg
        .lines()
        .find(|l| l.contains("<text ") && l.contains("級聯回退段"))
        .expect("SVG 必須含第 1 段");
    assert!(
        k_tag.contains("underline"),
        "第 1 段（樣式級底線）不得被連坐（實得 {k_tag}）"
    );
}

/// 燈 7：字型族斷行尺一體契約——存檔重開件級聯烘出宣告族，發射 `<text>`
/// 帶 `font-family`，且**排版度量同吃該族**：宣告 Arial 後該段行的 real
/// line-height 因子（`run_line_factor` 按宣告 face 查表）與水平 advance 尺
/// （`para_advance_scale`）同由 effective_fonts 驅動——實測段落行距由無宣告
/// 態 15.4pt（預設 face 因子）收斂為 Arial 態 ≈12.6pt，後續段基線上移。
/// 宣告了就用對應的尺，否則渲染端選預設 face 把行拉寬真越欄（K23 教訓）。
#[test]
fn k29e_lamp7_font_family_measure_and_emit_same_face() {
    let mut engine = fixture_engine();
    fmt_para(
        &mut engine,
        3,
        RunDirectPatch {
            font_family: Some("Arial".to_string()),
            ..Default::default()
        },
    );
    let bytes = engine.save_docx().expect("存檔失敗");
    let mut reopened = Engine::from_docx_bytes(&bytes).expect("重開失敗");
    settle(&mut reopened);

    let dl = reopened.page_display_list(0).expect("第 0 頁必須存在");
    let (ax, _, _, _, family) = first_text_with(&dl.items, "版面填充段");
    assert_eq!(
        family.as_deref(),
        Some("Arial"),
        "重開件級聯必須烘出 Arial（effective_fonts 消費 w:rFonts/@w:ascii）"
    );
    // 排版消費實證：無宣告態「版面填充乙段」基線 y=133.6（行距 15.4
    // ＝預設 face 因子×11pt）；Arial 態行距按 Arial face 因子收斂（實測
    // 12.6pt）→ 乙段基線上移 ≈2.8pt。尺表查無該 face 時退回預設因子，
    // 位移歸零＝本斷言紅（誠實：斷言只鎖「宣告族被度量層消費」事實）。
    let base = fixture_engine();
    let base_dl = base.page_display_list(0).expect("基準第 0 頁必須存在");
    let (_, base_next_y, ..) = first_text_with(&base_dl.items, "版面填充乙段");
    let (_, arial_next_y, ..) = first_text_with(&dl.items, "版面填充乙段");
    assert!(
        base_next_y - arial_next_y > 1.0,
        "Arial 宣告必須進排版度量：乙段基線應上移（Arial {arial_next_y} vs 基準 {base_next_y}）"
    );
    let svg = svg_of(&reopened, 0);
    let tag = svg
        .lines()
        .find(|l| l.contains("<text ") && l.contains("版面填充段"))
        .expect("SVG 必須含該段");
    assert!(
        tag.contains("font-family=\"Arial\""),
        "SVG 宣告族必須與斷行尺同族（實得 {tag}）"
    );
    let _ = ax;
}
