//! K29d 紅燈考卷：M6 節屬性／分節／頁首頁尾／註腳尾註家族＋SubDocTarget 完整路由。
//!
//! 依據任務卡 K29d 與 `docs/edit-api-design.md` §6.5（紅燈定義逐條照抄）：
//! - 紅燈 1：節屬性寫回（雙欄動態重排）——顯示列表立即出現兩個欄起點（首欄 ≈72.0pt、
//!   次欄 ≈315.65pt，對齊 l1_columns 口徑）、存檔重開仍雙欄、最小觸碰面僅
//!   `word/document.xml`；反向改單欄後次欄起點消失。
//! - 紅燈 2：頁首頁尾獨立 part 寫回與推下重算——頁首插字後 `header_pushes_pt` 重算、
//!   正文首行同步下推（對齊 k7b_header_push.rs:57-60 口徑）；存檔僅碰對應
//!   `word/header*.xml`、正文 `zero_loss()`；`ConfigureHeaderFooterSlot` 三態
//!   （titlePg 僅碰 document.xml／evenAndOddHeaders 碰 settings.xml／新建 part 才碰
//!   `.rels` 與 `[Content_Types].xml`）最小觸碰面逐條斷言。
//! - 紅燈 3：註腳插刪與跨頁掛載——新註文掛引用首現頁頁腳區（y > 690.0pt，對齊
//!   r7_footnote_anchor.rs:78-83 口徑）、存檔僅碰 `word/document.xml`＋`word/footnotes.xml`、
//!   `undo()` 後存檔全檔 `byte_identical`；`SetNoteProperties` 經 `word/settings.xml`。
//! - 紅燈 4：分節符插刪與四套索引同步——`para_sec`／`sec_geo_all`／`shape_boxes`／
//!   `image_pixels`／表格段落索引與「存檔重開的乾淨投影」逐位相同（K29b/K29c 紀律）；
//!   受保護結構（根 sectPr／系統分隔線註）誠實拒絕。
//!
//! 分頁結果與 LO 對齊由既有 golden（k7b／r7 全檔守門）負責，本檔只斷言自家
//! 顯示列表與索引自洽（卡面明文「分頁結果頁數與 LO 對齊由考卷內斷言既有 golden
//! 不動負責」）。
//!
//! 本檔只依賴 engine 公開面（不 import 內部模組），fixture 由 repo 根相對讀取。

use quire_engine::{
    DocPos, DrawItem, Engine, HfBindAction, HfKind, HfSlotKind, Mutation, NoteKind2,
    NoteNumberFormat, NotePropsPatch, PageOrientation, SectionPropsPatch, SectionStartKind,
    SubDocTarget,
};

// ---------------------------------------------------------------------------
// 通用輔助
// ---------------------------------------------------------------------------

fn load_bytes(rel: &str) -> Vec<u8> {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 {full} 失敗: {e}"))
}

fn open(rel: &str) -> Engine {
    Engine::from_docx_bytes(&load_bytes(rel)).unwrap_or_else(|e| panic!("開啟 {rel} 失敗: {e}"))
}

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

/// 第 `page` 頁非空文字項的 x 起點集合（去重到 0.5pt，對齊 `l1_columns.rs` 口徑）。
fn text_xs(engine: &Engine, page: usize) -> Vec<f32> {
    let dl = engine.page_display_list(page).expect("頁存在");
    let mut xs: Vec<f32> = Vec::new();
    for it in &dl.items {
        if let DrawItem::Text { x, text, .. } = it {
            if text.trim().is_empty() {
                continue;
            }
            if !xs.iter().any(|o| (o - x).abs() < 0.5) {
                xs.push(*x);
            }
        }
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs
}

/// 第 `page` 頁有非空文字項的最小 y（正文首行）。
fn first_body_y(engine: &Engine, page: usize) -> f32 {
    let dl = engine.page_display_list(page).expect("頁存在");
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { y, text, .. } if !text.trim().is_empty() => Some(*y),
            _ => None,
        })
        .fold(f32::INFINITY, f32::min)
}

/// 第 `page` 頁含 `needle` 的文字項（顯示列表序）。
fn texts_with(engine: &Engine, page: usize, needle: &str) -> Vec<(f32, f32, String)> {
    let dl = engine.page_display_list(page).expect("頁存在");
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { x, y, text, .. } if !text.trim().is_empty() => {
                Some((*x, *y, text.clone()))
            }
            _ => None,
        })
        .filter(|(_, _, t)| t.contains(needle))
        .collect()
}

/// 第 `page` 頁含 `marker` 的正文行最小 y（燈 2 專用「正文首行」定義）。
/// 口徑說明：`first_body_y` 取全頁最小 y，在頁首含文字的檔會先撞上頁首項
/// （本引擎顯示列表不帶頁首／正文分帶標籤，k7b 金燈的檔頁首是圖才無此誤撞）。
/// 頁首增高推下鎖必須盯正文行本身，故以正文段落的標記字串過濾。
fn body_marker_min_y(engine: &Engine, page: usize, marker: &str) -> f32 {
    texts_with(engine, page, marker)
        .iter()
        .map(|(_, y, _)| *y)
        .fold(f32::INFINITY, f32::min)
}

fn find_para(engine: &Engine, needle: &str) -> Option<usize> {
    (0..engine.paragraph_count()).find(|&i| {
        engine
            .paragraph_text(i)
            .as_deref()
            .is_some_and(|t| t.contains(needle))
    })
}

/// 存檔重開（顯示列表與索引皆由存檔件重建，證明寫回真的落地）。
fn reopen(engine: &Engine) -> Engine {
    let saved = engine.save_docx().expect("存檔失敗");
    let mut e = Engine::from_docx_bytes(&saved).expect("重開失敗");
    settle(&mut e);
    e
}

/// 觸碰面斷言核心：`pass` 為真、`whitelisted_changed ∪ added` 恰為 `expect`
/// （排序比較；新建 part 在原檔無 entry，diff_zip_structure 把它記進 `added`
/// 而非 `whitelisted_changed`——兩者同屬「本次編輯造成的內容變動」，故併入），
/// 其餘全部 entry `zero_loss()`。
fn assert_touch_face(engine: &Engine, expect: &[&str], ctx: &str) {
    let diff = engine.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(
        diff.pass,
        "{ctx}：roundtrip 必須通過，違規項 {:?}",
        diff.violations
    );
    let mut got: Vec<String> = diff.whitelisted_changed.clone();
    got.extend(diff.added.iter().cloned());
    got.sort();
    let mut want: Vec<String> = expect.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(
        got, want,
        "{ctx}：whitelisted_changed∪added 必須恰為 {want:?}"
    );
    for (name, entry) in &diff.per_entry {
        if want.iter().any(|w| w == name) {
            continue;
        }
        assert!(
            entry.zero_loss(),
            "{ctx}：未觸碰 entry {name} 必須 zero_loss，摘要 {}",
            entry.summary()
        );
    }
}

/// 指名 entry 零失真斷言（最小觸碰面的反向鎖：指定 part 必須一位不動）。
fn assert_zero_loss(engine: &Engine, names: &[&str], ctx: &str) {
    let diff = engine.roundtrip_check().expect("roundtrip_check 失敗");
    for name in names {
        let entry = diff
            .per_entry
            .get(*name)
            .unwrap_or_else(|| panic!("{ctx}：diff 查無 entry {name}"));
        assert!(
            entry.zero_loss(),
            "{ctx}：entry {name} 必須 zero_loss，摘要 {}",
            entry.summary()
        );
    }
}

/// 四套段落索引＋段落數是否與「存檔重開的乾淨投影」逐位相同（K29b/K29c 紀律）。
fn assert_indices_in_sync(engine: &Engine, ctx: &str) {
    let live = engine.index_sync_json();
    let saved = engine.save_docx().expect("存檔失敗");
    let fresh =
        Engine::from_docx_bytes(&saved).unwrap_or_else(|e| panic!("{ctx}：存檔件重開失敗 {e}"));
    let clean = fresh.index_sync_json();
    assert_eq!(live, clean, "{ctx}：四套段落索引與乾淨重解析投影不一致");
}

/// `para_sections` 全表（段落 → 節 id）。
fn sec_ids(engine: &Engine) -> Vec<u32> {
    engine.para_sections().to_vec()
}

/// `word/footnotes.xml` 中最大的 `w:footnote/@w:id`（派發新 id 的觀察口）。
/// 注意 `w:id="` 長度是 6，值起點＝ pos+6（用 7 會吃掉值首字元導致恆失敗）。
fn max_footnote_id(xml: &[u8]) -> Option<i32> {
    let text = std::str::from_utf8(xml).ok()?;
    let mut max: Option<i32> = None;
    let mut cursor = 0usize;
    while let Some(rel) = text[cursor..].find("<w:footnote ") {
        let start = cursor + rel;
        let gt = text[start..].find('>')? + start;
        let tag = &text[start..=gt];
        if let Some(pos) = tag.find(r#"w:id=""#) {
            let rest = &tag[pos + 6..];
            if let Some(end) = rest.find('"') {
                if let Ok(v) = rest[..end].trim().parse::<i32>() {
                    max = Some(max.unwrap_or(i32::MIN).max(v));
                }
            }
        }
        cursor = gt + 1;
    }
    max
}

// ---------------------------------------------------------------------------
// 紅燈 1：節屬性與分欄動態重排鎖
// ---------------------------------------------------------------------------

#[test]
fn k29d_lamp1_section_columns_relayout_and_minimal_touch() {
    let mut e = open("fixtures/l1_two_col.docx");
    settle(&mut e);
    // 基準：雙欄檔 p0 兩個欄起點（l1_columns.rs:42-70 口徑）。
    let xs0 = text_xs(&e, 0);
    assert!(
        xs0.iter().any(|x| (x - 72.0).abs() < 0.6) && xs0.iter().any(|x| (x - 315.65).abs() < 0.6),
        "基準雙欄 p0 必須兩欄起點齊備：xs={xs0:?}"
    );

    // 反向先行：改單欄 ⇒ 次欄起點消失（卡面「反向：改單欄後次欄起點消失」）。
    e.apply_mutation(Mutation::SetSectionProperties {
        section_id: 0,
        patch: SectionPropsPatch {
            cols: Some(quire_engine::SectionColsPatch {
                num: 1,
                space_pt: None,
            }),
            ..Default::default()
        },
    })
    .expect("M6.1 SetSectionProperties（單欄）失敗");
    settle(&mut e);
    let xs1 = text_xs(&e, 0);
    assert!(
        xs1.iter().any(|x| (x - 72.0).abs() < 0.6),
        "單欄態版心左緣 72 仍必在：xs={xs1:?}"
    );
    assert!(
        !xs1.iter().any(|x| (x - 315.65).abs() < 1.0),
        "改單欄後次欄起點必須消失：xs={xs1:?}"
    );

    // 正向：改雙欄（num=2, space=36.0pt）＋插入長段文字 ⇒ 顯示列表立即重排。
    e.apply_mutation(Mutation::SetSectionProperties {
        section_id: 0,
        patch: SectionPropsPatch {
            cols: Some(quire_engine::SectionColsPatch {
                num: 2,
                space_pt: Some(36.0),
            }),
            ..Default::default()
        },
    })
    .expect("M6.1 SetSectionProperties（雙欄）失敗");
    settle(&mut e);
    let xs2 = text_xs(&e, 0);
    let first = xs2.iter().find(|x| (**x - 72.0).abs() < 0.6).copied();
    let second = xs2.iter().find(|x| (**x - 315.65).abs() < 0.6).copied();
    assert!(
        first.is_some() && second.is_some(),
        "改雙欄後 p0 必須立即出現兩個欄起點（首欄 ≈72／次欄 ≈315.65）：xs={xs2:?}"
    );
    let pitch = second.unwrap() - first.unwrap();
    assert!(
        (pitch - 243.65).abs() < 1.2,
        "欄距（次欄起點−首欄起點）＝欄寬＋space≈243.65pt，實得 {pitch}"
    );
    // 插入長段文字（動態重排：新內容同樣走雙欄斷行）。
    let last_para = e.paragraph_count() - 1;
    e.apply_mutation(Mutation::InsertText {
        target: SubDocTarget::Body,
        pos: DocPos {
            para: last_para,
            cp: 0,
        },
        text: "K29D 長段填充 ".repeat(80),
        style_override: None,
    })
    .expect("雙欄態 InsertText 失敗");
    settle(&mut e);
    let xs3 = text_xs(&e, 0);
    assert!(
        xs3.iter().any(|x| (x - 315.65).abs() < 0.6),
        "插入長文後 p0 仍必須保持雙欄：xs={xs3:?}"
    );
    assert_eq!(
        e.paragraph_count(),
        40,
        "l1_two_col.docx 段落數基準 40（InsertText 不改段數）"
    );

    // 存檔重開仍雙欄＋長文保留（寫回落地，非記憶體態幻覺）。
    let r = reopen(&e);
    let rx = text_xs(&r, 0);
    assert!(
        rx.iter().any(|x| (x - 72.0).abs() < 0.6) && rx.iter().any(|x| (x - 315.65).abs() < 0.6),
        "存檔重開後必須仍是雙欄：xs={rx:?}"
    );
    assert!(
        find_para(&r, "K29D 長段填充").is_some(),
        "存檔重開後長段文字必須保留"
    );

    // 最小觸碰面：僅 word/document.xml。
    assert_touch_face(&e, &["word/document.xml"], "紅燈 1 分欄動態重排");
    assert_indices_in_sync(&e, "紅燈 1 分欄動態重排");
}

#[test]
fn k29d_lamp1_orientation_margins_and_writeback_without_sectpr() {
    // sample.docx 全檔無任何 sectPr：SetSectionProperties 必須以開檔生效幾何
    // 合成 body 級 `<w:sectPr>`（設計書 §2.3.6），且合成本身零視覺位移。
    let mut e = open("fixtures/sample.docx");
    settle(&mut e);
    let y0 = first_body_y(&e, 0);

    e.apply_mutation(Mutation::SetSectionProperties {
        section_id: 0,
        patch: SectionPropsPatch {
            orientation: Some(PageOrientation::Landscape),
            ..Default::default()
        },
    })
    .expect("M6.1 橫向失敗");
    settle(&mut e);
    let doc = String::from_utf8(e.part("word/document.xml").expect("document.xml").to_vec())
        .expect("UTF-8");
    let pgsz = doc
        .split("<w:pgSz")
        .nth(1)
        .and_then(|s| s.split('>').next())
        .unwrap_or_default()
        .to_string();
    // PM 裁決（11-12）：sample.docx 無 sectPr，生效幾何＝LayoutParams::default() 整數
    // A4 595×842pt（core/layout/src/lib.rs:352）⇒ 橫向正確值 16840/11900（Word 真 A4
    // 的 16838/11906 屬 twip 精度另卡議題，本卡嚴禁改引擎預設湊小數）。
    assert!(
        pgsz.contains(r#"w:w="16840""#) && pgsz.contains(r#"w:h="11900""#),
        "橫向必須寫回 pgSz w/h 互換（twip＝pt×20），實得 {pgsz}"
    );
    assert!(
        pgsz.contains(r#"w:orient="landscape""#),
        "橫向必須帶 w:orient=\"landscape\"，實得 {pgsz}"
    );

    // 邊距改寫＋索引不動：上下 72pt、左右 90pt（twip 1440／1800）。
    e.apply_mutation(Mutation::SetSectionProperties {
        section_id: 0,
        patch: SectionPropsPatch {
            margin_top_pt: Some(72.0),
            margin_bottom_pt: Some(72.0),
            margin_left_pt: Some(90.0),
            margin_right_pt: Some(90.0),
            ..Default::default()
        },
    })
    .expect("M6.1 邊距失敗");
    settle(&mut e);
    let doc = String::from_utf8(e.part("word/document.xml").expect("document.xml").to_vec())
        .expect("UTF-8");
    let pgmar = doc
        .split("<w:pgMar")
        .nth(1)
        .and_then(|s| s.split('>').next())
        .unwrap_or_default()
        .to_string();
    assert!(
        pgmar.contains(r#"w:top="1440""#) && pgmar.contains(r#"w:left="1800""#),
        "邊距必須寫回 twip（top=1440／left=1800），實得 {pgmar}"
    );
    assert_eq!(e.paragraph_count(), 14, "sample.docx 段落數基準 14");
    assert_eq!(sec_ids(&e).len(), 14, "para_sec 必須與段落數同長");
    let y1 = first_body_y(&e, 0);
    assert!(
        (y1 - y0).abs() < 0.6,
        "top 邊距未變（72pt）時正文首行 y 不得位移：{y0} → {y1}"
    );
    assert_touch_face(&e, &["word/document.xml"], "紅燈 1 橫向與邊距");
    assert_indices_in_sync(&e, "紅燈 1 橫向與邊距");

    // 存檔重開：橫向＋邊距全部保留。
    let r = reopen(&e);
    let doc = String::from_utf8(r.part("word/document.xml").expect("document.xml").to_vec())
        .expect("UTF-8");
    assert!(
        doc.contains(r#"w:orient="landscape""#),
        "重開後 orient 保留"
    );
    assert!(doc.contains(r#"w:left="1800""#), "重開後邊距保留");
}

#[test]
fn k29d_lamp1_section_type_and_honest_rejects() {
    let mut e = open("fixtures/sample.docx");
    settle(&mut e);

    // 節起點型別：continuous 寫回 `<w:type w:val="continuous"/>`。
    e.apply_mutation(Mutation::SetSectionProperties {
        section_id: 0,
        patch: SectionPropsPatch {
            section_type: Some(SectionStartKind::Continuous),
            ..Default::default()
        },
    })
    .expect("M6.1 節起點型別失敗");
    settle(&mut e);
    let doc = String::from_utf8(e.part("word/document.xml").expect("document.xml").to_vec())
        .expect("UTF-8");
    assert!(
        doc.contains(r#"<w:type w:val="continuous"/>"#),
        "continuous 必須寫回 <w:type w:val=\"continuous\"/>"
    );
    assert_touch_face(&e, &["word/document.xml"], "紅燈 1 節起點型別");

    // 誠實拒絕：邊距把版心吃掉 ⇒ XmlPatchRejected（設計書 §2.3.6-3）。
    let mut e2 = open("fixtures/sample.docx");
    settle(&mut e2);
    let err = e2
        .apply_mutation(Mutation::SetSectionProperties {
            section_id: 0,
            patch: SectionPropsPatch {
                margin_left_pt: Some(500.0),
                margin_right_pt: Some(500.0),
                ..Default::default()
            },
        })
        .expect_err("版心寬 <= 0 必須誠實拒絕");
    assert!(
        matches!(err, quire_engine::MutationError::XmlPatchRejected { .. }),
        "版心歸零的拒絕型別必須是 XmlPatchRejected，實得 {err:?}"
    );
    assert!(!e2.can_undo(), "被拒絕的操作不得留下 Undo 記錄（零副作用）");

    // 誠實拒絕：不存在的節 id。
    let mut e3 = open("fixtures/sample.docx");
    settle(&mut e3);
    let err = e3
        .apply_mutation(Mutation::SetSectionProperties {
            section_id: 99,
            patch: SectionPropsPatch {
                cols: Some(quire_engine::SectionColsPatch {
                    num: 2,
                    space_pt: None,
                }),
                ..Default::default()
            },
        })
        .expect_err("不存在的節 id 必須誠實拒絕");
    assert!(
        matches!(err, quire_engine::MutationError::SubDocNotFound { .. }),
        "未知節 id 的拒絕型別必須是 SubDocNotFound，實得 {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 紅燈 2：頁首／頁尾獨立 Part 寫回與推下重算鎖
// ---------------------------------------------------------------------------

#[test]
fn k29d_lamp2_header_text_routes_to_header_part_only() {
    let mut e = open("fixtures/hf_table_header.docx");
    settle(&mut e);
    // PM 裁決（11-12）：金燈 k9 的 >=2 是寬鬆基準；本卡要鎖「編輯零擾動」，故釘死
    // 實測基線 3 頁（編輯前 3、頁首路由編輯後仍 3），嚴禁改引擎頁數湊 2。
    assert_eq!(
        e.page_count(),
        3,
        "hf_table_header.docx 頁數基準 3（本引擎實測基線）"
    );
    assert!(
        texts_with(&e, 1, "頁首覆核字串").is_empty(),
        "基準態 p1 不得已含新頁首字串"
    );
    let doc_before = e.part("word/document.xml").expect("document.xml").to_vec();
    let hdr_before = e.part("word/header1.xml").expect("header1.xml").to_vec();
    let first_before = body_marker_min_y(&e, 1, "正文填充");

    // 對 SubDocTarget::Header（default slot → word/header1.xml）插入長文字。
    e.apply_mutation(Mutation::InsertText {
        target: SubDocTarget::Header {
            section_id: 0,
            slot: HfSlotKind::Default,
        },
        pos: DocPos { para: 0, cp: 0 },
        text: "頁首覆核字串".repeat(90),
        style_override: None,
    })
    .expect("頁首 InsertText 失敗");
    settle(&mut e);

    // (a) 頁首增高 ⇒ header_pushes_pt 重算、正文首行 y 同步下推（k7b 口徑）。
    // 「正文首行」以正文標記字串過濾（見 body_marker_min_y 注：全頁最小 y 會先
    // 撞上頁首文字項，非正文行）。
    assert_eq!(
        e.page_count(),
        3,
        "頁首路由後頁數必須零擾動（編輯前後恆 3）"
    );
    let after = body_marker_min_y(&e, 1, "正文填充");
    assert!(
        after > first_before + 10.0,
        "頁首增高後 p1 正文首行必須隨之推下：修前 {first_before} → 修後 {after}"
    );
    // (b) 新文字發射在頁首區（發射層真的讀到改後的 header part），且全在正文首行之上。
    let hits = texts_with(&e, 1, "頁首覆核字串");
    assert!(!hits.is_empty(), "頁首新文字必須發射在 p1 頁首區");
    assert!(
        hits.iter().all(|(_, y, _)| *y < after - 0.5),
        "頁首文字必須全部位於推下後的正文首行之上（y < {after}），實得 {:?}",
        hits.iter().map(|(_, y, _)| *y).collect::<Vec<_>>()
    );

    // (c) 最小觸碰面：僅 word/header1.xml，正文 word/document.xml 零失真。
    assert_touch_face(&e, &["word/header1.xml"], "紅燈 2 頁首插字");
    assert_zero_loss(&e, &["word/document.xml"], "紅燈 2 頁首插字");
    assert_eq!(
        e.part("word/document.xml"),
        Some(doc_before.as_slice()),
        "頁首編輯不得動 memory 中的正文明文"
    );
    assert_ne!(
        e.part("word/header1.xml"),
        Some(hdr_before.as_slice()),
        "header1.xml 必須真的被改寫"
    );

    // (d) 存檔重開：字串保留、推下仍生效（逐位一致）。
    let r = reopen(&e);
    assert!(
        !texts_with(&r, 1, "頁首覆核字串").is_empty(),
        "重開後頁首文字保留"
    );
    let ra = body_marker_min_y(&r, 1, "正文填充");
    assert!(
        (ra - after).abs() < 0.01,
        "重開後正文首行 y 必須與編輯態一致：{after} vs {ra}"
    );
    assert_indices_in_sync(&e, "紅燈 2 頁首插字");
}

#[test]
fn k29d_lamp2_footer_and_note_body_slices_reuse_m1() {
    // 頁尾子樹（footer1.xml）與註腳本體（footnotes.xml）走同一 SubDoc 寫回路由。
    let mut e = open("fixtures/hf_table_header.docx");
    settle(&mut e);
    e.apply_mutation(Mutation::InsertText {
        target: SubDocTarget::Footer {
            section_id: 0,
            slot: HfSlotKind::Default,
        },
        pos: DocPos { para: 0, cp: 0 },
        text: "頁尾覆核".to_string(),
        style_override: None,
    })
    .expect("頁尾 InsertText 失敗");
    settle(&mut e);
    assert!(
        !texts_with(&e, 1, "頁尾覆核").is_empty(),
        "頁尾新文字必須發射在 p1 頁尾區"
    );
    assert_touch_face(&e, &["word/footer1.xml"], "紅燈 2 頁尾插字");

    let mut f = open("corpus/poi/test-data_document_form_footnotes.docx");
    settle(&mut f);
    f.apply_mutation(Mutation::InsertText {
        target: SubDocTarget::Footnote { note_id: 2 },
        pos: DocPos { para: 0, cp: 6 },
        text: "改寫".to_string(),
        style_override: None,
    })
    .expect("註腳本體 InsertText 失敗");
    settle(&mut f);
    assert!(
        !texts_with(&f, 0, "改寫").is_empty(),
        "註腳本體改寫必須立刻反映在 p0 頁腳區"
    );
    assert_touch_face(
        &f,
        &["word/footnotes.xml"],
        "紅燈 2 註腳本體插字（正文零失真）",
    );
    assert_indices_in_sync(&f, "紅燈 2 註腳本體插字");
}

#[test]
fn k29d_lamp2_configure_slots_minimal_touch() {
    // (i) 啟 titlePg（table_mixed 無 titlePg）：僅碰 word/document.xml。
    let mut e = open("fixtures/table_mixed.docx");
    settle(&mut e);
    e.apply_mutation(Mutation::ConfigureHeaderFooterSlot {
        section_id: 0,
        title_pg: Some(true),
        even_and_odd_headers: None,
        slot_bind: None,
    })
    .expect("M6.3 啟 titlePg 失敗");
    settle(&mut e);
    let doc = String::from_utf8(e.part("word/document.xml").expect("document.xml").to_vec())
        .expect("UTF-8");
    assert!(
        doc.contains("<w:titlePg") && !doc.contains(r#"<w:titlePg w:val="0""#),
        "titlePg 必須落地為 <w:titlePg/>"
    );
    assert_touch_face(&e, &["word/document.xml"], "紅燈 2 titlePg");
    // 反向：停 titlePg 同樣只碰 document.xml。
    e.apply_mutation(Mutation::ConfigureHeaderFooterSlot {
        section_id: 0,
        title_pg: Some(false),
        even_and_odd_headers: None,
        slot_bind: None,
    })
    .expect("M6.3 停 titlePg 失敗");
    settle(&mut e);
    // 反向：停 titlePg 同樣只碰 document.xml。PM 裁決（11-12）第 3 項查語意結論：
    // on→off 淨零變動 ⇒ Slice 1 主幹「touched 自動退場」紀律（k29a 紅燈 3 同口徑）
    // 使該 part 退出白名單、存檔回到原檔——停態 whitelisted_changed 正解是空集合，
    // 原斷言把「碰過」與「最終內容不同」混為一談，屬考卷口徑錯誤，在此修正；
    // 「off 真的移除旗標」由上方 !doc.contains 斷言獨立鎖住，不靠觸碰面背書。
    let doc = String::from_utf8(e.part("word/document.xml").expect("document.xml").to_vec())
        .expect("UTF-8");
    assert!(
        !doc.contains("<w:titlePg"),
        "停 titlePg 必須移除 <w:titlePg/>"
    );
    assert_touch_face(&e, &[], "紅燈 2 停 titlePg（淨零變動退場）");
    let diff_off = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(
        diff_off.byte_identical,
        "on→off 淨零變動存檔必須 byte_identical，殘留 {:?}",
        diff_off.whitelisted_changed
    );

    // (ii) 啟 evenAndOddHeaders：僅碰 word/settings.xml（正文零失真）。
    let mut e2 = open("corpus/poi/test-data_document_Bug51170.docx");
    settle(&mut e2);
    e2.apply_mutation(Mutation::ConfigureHeaderFooterSlot {
        section_id: 0,
        title_pg: None,
        even_and_odd_headers: Some(true),
        slot_bind: None,
    })
    .expect("M6.3 啟奇偶頁失敗");
    settle(&mut e2);
    let settings = String::from_utf8(e2.part("word/settings.xml").expect("settings.xml").to_vec())
        .expect("UTF-8");
    assert!(
        settings.contains("<w:evenAndOddHeaders"),
        "evenAndOddHeaders 必須落地 settings.xml"
    );
    assert_touch_face(&e2, &["word/settings.xml"], "紅燈 2 evenAndOddHeaders");
    assert_zero_loss(&e2, &["word/document.xml"], "紅燈 2 evenAndOddHeaders");

    // (iii) 新建頁首 part：才碰 [Content_Types].xml／document.xml／.rels／新 part。
    let mut e3 = open("corpus/poi/test-data_document_Bug51170.docx");
    settle(&mut e3);
    e3.apply_mutation(Mutation::ConfigureHeaderFooterSlot {
        section_id: 0,
        title_pg: None,
        even_and_odd_headers: None,
        slot_bind: Some((HfKind::Header, HfSlotKind::Even, HfBindAction::Create)),
    })
    .expect("M6.3 新建 even 頁首失敗");
    settle(&mut e3);
    let mut names: Vec<String> = e3
        .part_names()
        .into_iter()
        .filter(|n| n.starts_with("word/header"))
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "word/header1.xml".to_string(),
            "word/header2.xml".to_string()
        ],
        "新建 even 頁首必須落為最小空號 word/header2.xml（既有 header1.xml 之後）"
    );
    let doc3 = String::from_utf8(e3.part("word/document.xml").expect("document.xml").to_vec())
        .expect("UTF-8");
    assert!(
        doc3.contains(r#"w:type="even""#),
        "新建 even slot 必須在 sectPr 綁 <w:headerReference w:type=\"even\" .../>"
    );
    let rels3 = String::from_utf8(
        e3.part("word/_rels/document.xml.rels")
            .expect("document.xml.rels")
            .to_vec(),
    )
    .expect("UTF-8");
    assert!(
        rels3.contains("header2.xml"),
        "新 part 必須註冊進 word/_rels/document.xml.rels"
    );
    let ct3 = String::from_utf8(
        e3.part("[Content_Types].xml")
            .expect("[Content_Types].xml")
            .to_vec(),
    )
    .expect("UTF-8");
    assert!(
        ct3.contains("/word/header2.xml"),
        "新建 part 必須註冊進 [Content_Types].xml 的 Override"
    );
    assert_touch_face(
        &e3,
        &[
            "[Content_Types].xml",
            "word/_rels/document.xml.rels",
            "word/document.xml",
            "word/header2.xml",
        ],
        "紅燈 2 新建頁首 part",
    );
    assert_zero_loss(&e3, &["word/settings.xml"], "紅燈 2 新建頁首 part");

    // (iv) 新建後再對該 slot 插字：觸碰面不再擴大（仍四 entry，內容落地 header2.xml）。
    e3.apply_mutation(Mutation::InsertText {
        target: SubDocTarget::Header {
            section_id: 0,
            slot: HfSlotKind::Even,
        },
        pos: DocPos { para: 0, cp: 0 },
        text: "偶數頁首".to_string(),
        style_override: None,
    })
    .expect("新建 even 頁首插字失敗");
    settle(&mut e3);
    assert_touch_face(
        &e3,
        &[
            "[Content_Types].xml",
            "word/_rels/document.xml.rels",
            "word/document.xml",
            "word/header2.xml",
        ],
        "紅燈 2 新建 even 頁首後插字",
    );
    let r3 = reopen(&e3);
    let h3 = String::from_utf8(
        r3.part("word/header2.xml")
            .expect("header2.xml 存檔保留")
            .to_vec(),
    )
    .expect("UTF-8");
    assert!(h3.contains("偶數頁首"), "新建 part 的內容必須存檔保留");

    // (v) 解除綁定：僅碰 document.xml（part 本體留存不刪——誠實邊界）。
    let mut e4 = open("fixtures/hf_table_header.docx");
    settle(&mut e4);
    e4.apply_mutation(Mutation::ConfigureHeaderFooterSlot {
        section_id: 0,
        title_pg: None,
        even_and_odd_headers: None,
        slot_bind: Some((HfKind::Footer, HfSlotKind::Default, HfBindAction::Unbind)),
    })
    .expect("M6.3 解除頁尾綁定失敗");
    settle(&mut e4);
    let doc4 = String::from_utf8(e4.part("word/document.xml").expect("document.xml").to_vec())
        .expect("UTF-8");
    assert!(
        !doc4.contains("footerReference"),
        "default 頁尾綁定必須從 sectPr 移除（headerReference 不受波及）"
    );
    assert!(
        doc4.contains(r#"w:type="default""#),
        "解除頁尾綁定不得誤刪頁首 default 綁定"
    );
    assert_touch_face(&e4, &["word/document.xml"], "紅燈 2 解除綁定");
    let r4 = reopen(&e4);
    assert!(
        texts_with(&r4, 1, "過期快取").is_empty(),
        "解除綁定後頁尾不得再發射（footer part 仍在包內但不掛載）"
    );
}

// ---------------------------------------------------------------------------
// 紅燈 3：註腳／尾註插刪與跨頁掛載鎖
// ---------------------------------------------------------------------------

#[test]
fn k29d_lamp3_insert_note_anchors_to_footer_and_undoes_byte_identical() {
    let rel = "corpus/poi/test-data_document_form_footnotes.docx";
    let mut e = open(rel);
    settle(&mut e);
    let host = find_para(&e, "Testdoc").unwrap_or(0);
    let base_pages = e.page_count();
    assert_eq!(base_pages, 3, "form_footnotes.docx 頁數基準 3（r7 口徑）");

    // (1) InsertNote：正文錨點＋footnotes.xml 新註體（id 由引擎派發）。
    e.apply_mutation(Mutation::InsertNote {
        kind: NoteKind2::Footnote,
        pos: DocPos { para: host, cp: 0 },
        initial_text: String::new(),
    })
    .expect("M6.4 InsertNote 失敗");
    settle(&mut e);
    let new_id = e
        .part("word/footnotes.xml")
        .and_then(max_footnote_id)
        .expect("新註腳 id 必須存在");
    assert!(
        new_id >= 2,
        "新註腳 id 必須避開系統分隔線註（本檔 0/1），實得 {new_id}"
    );

    // (2) 以 SubDocTarget::Footnote 編註文。
    e.apply_mutation(Mutation::InsertText {
        target: SubDocTarget::Footnote { note_id: new_id },
        pos: DocPos { para: 0, cp: 0 },
        text: "K29d 新註腳本文".to_string(),
        style_override: None,
    })
    .expect("註腳本體 InsertText 失敗");
    settle(&mut e);

    // (3) 新註文出現在引用首現頁的頁腳區（r7_footnote_anchor.rs:78-83 口徑）。
    let hits = texts_with(&e, 0, "K29d 新註腳本文");
    assert!(
        !hits.is_empty(),
        "新註文必須出現在 p0（引用首現頁）顯示列表"
    );
    for (_, y, t) in &hits {
        assert!(
            *y > 690.0,
            "註文 {t:?} y={y} 必須落在 p0 頁腳區（應 >690，對齊 r7 口徑）"
        );
    }
    assert_eq!(
        e.page_count(),
        base_pages,
        "插註腳不得新增頁（自家顯示列表自洽）"
    );

    // (4) 存檔後僅 document.xml＋footnotes.xml 列白名單，其餘 zero_loss。
    assert_touch_face(
        &e,
        &["word/document.xml", "word/footnotes.xml"],
        "紅燈 3 插註腳＋編註文",
    );
    assert_zero_loss(
        &e,
        &["word/endnotes.xml", "word/settings.xml", "word/styles.xml"],
        "紅燈 3 插註腳＋編註文",
    );
    assert_indices_in_sync(&e, "紅燈 3 插註腳");

    // (5) undo() ×2 後存檔全檔 byte_identical（touched 自動退場）。
    e.undo().expect("undo 註文失敗");
    settle(&mut e);
    e.undo().expect("undo 插註腳失敗");
    settle(&mut e);
    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(
        diff.byte_identical,
        "undo 回初始態後存檔必須全檔 byte_identical，白名單殘留 {:?}",
        diff.whitelisted_changed
    );
    assert!(e.can_redo(), "undo 後必須可 redo");
}

#[test]
fn k29d_lamp3_delete_note_and_protected_structure() {
    let rel = "corpus/poi/test-data_document_form_footnotes.docx";
    let mut e = open(rel);
    settle(&mut e);
    assert!(
        !texts_with(&e, 0, "test phrase").is_empty(),
        "基準註文必須掛在 p0 頁腳區（r7 口徑）"
    );

    // 刪既有註腳 id=2：正文錨點 run 與註體同拆，僅碰 document.xml＋footnotes.xml。
    e.apply_mutation(Mutation::DeleteNote {
        kind: NoteKind2::Footnote,
        note_id: 2,
    })
    .expect("M6.4 DeleteNote 失敗");
    settle(&mut e);
    assert!(
        texts_with(&e, 0, "test phrase").is_empty(),
        "刪註後註文必須從顯示列表消失"
    );
    assert_touch_face(
        &e,
        &["word/document.xml", "word/footnotes.xml"],
        "紅燈 3 刪註腳",
    );
    assert_zero_loss(
        &e,
        &["word/endnotes.xml", "word/settings.xml"],
        "紅燈 3 刪註腳",
    );
    assert_indices_in_sync(&e, "紅燈 3 刪註腳");
    let r = reopen(&e);
    assert!(
        texts_with(&r, 0, "test phrase").is_empty(),
        "重開後註文仍不得出現（刪除真的寫回）"
    );

    // 誠實拒絕：系統分隔線註（separator/continuationSeparator）不得刪。
    let mut p = open(rel);
    settle(&mut p);
    for id in [0i32, 1i32] {
        let err = p
            .apply_mutation(Mutation::DeleteNote {
                kind: NoteKind2::Footnote,
                note_id: id,
            })
            .expect_err("系統分隔線註必須誠實拒絕刪除");
        assert!(
            matches!(err, quire_engine::MutationError::ProtectedStructure { .. }),
            "id={id} 的拒絕型別必須是 ProtectedStructure，實得 {err:?}"
        );
    }
    assert!(!p.can_undo(), "被拒絕的刪註不得留下 Undo 記錄");
    assert_touch_face(&p, &[], "紅燈 3 拒刪分隔線註");
}

#[test]
fn k29d_lamp3_note_properties_go_through_settings_only() {
    let rel = "corpus/poi/test-data_document_form_footnotes.docx";
    let mut e = open(rel);
    settle(&mut e);
    let doc_before = e.part("word/document.xml").expect("document.xml").to_vec();

    // 改編號格式／重啟規則／起始值：僅碰 word/settings.xml。
    e.apply_mutation(Mutation::SetNoteProperties {
        kind: NoteKind2::Footnote,
        patch: NotePropsPatch {
            num_fmt: Some(NoteNumberFormat::LowerRoman),
            num_restart: Some(quire_engine::NoteRestart::EachPage),
            num_start: Some(1),
            pos: None,
        },
    })
    .expect("M6.4 SetNoteProperties 失敗");
    settle(&mut e);
    let settings = String::from_utf8(e.part("word/settings.xml").expect("settings.xml").to_vec())
        .expect("UTF-8");
    assert!(
        settings.contains(r#"w:numFmt w:val="lowerRoman""#),
        "numFmt 必須寫入 settings.xml 的 w:footnotePr"
    );
    assert!(
        settings.contains(r#"w:numRestart w:val="eachPage""#),
        "numRestart 必須寫入 settings.xml 的 w:footnotePr"
    );
    assert!(
        settings.contains(r#"w:numStart w:val="1""#),
        "numStart 必須寫入 settings.xml 的 w:footnotePr"
    );
    assert_touch_face(&e, &["word/settings.xml"], "紅燈 3 註腳編號設定");
    assert_eq!(
        e.part("word/document.xml"),
        Some(doc_before.as_slice()),
        "改註腳設定不得動正文明文"
    );

    // 尾註側同路徑（w:endnotePr），且與 footnotePr 互不侵蝕。
    e.apply_mutation(Mutation::SetNoteProperties {
        kind: NoteKind2::Endnote,
        patch: NotePropsPatch {
            num_fmt: Some(NoteNumberFormat::Decimal),
            num_restart: None,
            num_start: None,
            pos: None,
        },
    })
    .expect("M6.4 SetNoteProperties（尾註）失敗");
    settle(&mut e);
    let settings = String::from_utf8(e.part("word/settings.xml").expect("settings.xml").to_vec())
        .expect("UTF-8");
    let end_block = settings
        .split("<w:endnotePr")
        .nth(1)
        .and_then(|s| s.split("</w:endnotePr>").next())
        .unwrap_or_default()
        .to_string();
    assert!(
        end_block.contains(r#"w:numFmt w:val="decimal""#),
        "尾註 numFmt 必須落在 w:endnotePr 內，實得 {end_block}"
    );
    let foot_block = settings
        .split("<w:footnotePr")
        .nth(1)
        .and_then(|s| s.split("</w:footnotePr>").next())
        .unwrap_or_default()
        .to_string();
    assert!(
        foot_block.contains(r#"w:numFmt w:val="lowerRoman""#),
        "改尾註設定不得覆寫腳註設定，實得 {foot_block}"
    );
    assert_touch_face(&e, &["word/settings.xml"], "紅燈 3 尾註編號設定");

    // 存檔重開：設定保留（寫回落地）。
    let r = reopen(&e);
    let rs = String::from_utf8(r.part("word/settings.xml").expect("settings.xml").to_vec())
        .expect("UTF-8");
    assert!(
        rs.contains(r#"w:numFmt w:val="lowerRoman""#),
        "重開後設定保留"
    );
}

// ---------------------------------------------------------------------------
// 紅燈 4：分節符插刪與四套索引同步鎖
// ---------------------------------------------------------------------------

#[test]
fn k29d_lamp4_insert_section_break_keeps_four_index_sets_in_sync() {
    let rel = "fixtures/table_mixed.docx";
    let mut e = open(rel);
    settle(&mut e);
    let base = e.paragraph_count();
    assert_eq!(base, 13, "table_mixed.docx 扁平段落數基準 13");
    let ids_before = sec_ids(&e);
    assert!(
        ids_before.iter().all(|id| *id == ids_before[0]),
        "基準態單節：para_sec 全同值，實得 {ids_before:?}"
    );
    let pages_before = e.page_count();

    // 於 body 級第 0 段插段內 <w:pPr><w:sectPr>（ECMA-376 §17.2.3：前節收束於段內 sectPr）。
    e.apply_mutation(Mutation::InsertSectionBreak {
        para: 0,
        kind: SectionStartKind::NextPage,
    })
    .expect("M6.2 InsertSectionBreak 失敗");
    settle(&mut e);

    // 段內 sectPr 落地。
    let doc = String::from_utf8(e.part("word/document.xml").expect("document.xml").to_vec())
        .expect("UTF-8");
    let host_p = doc
        .split("<w:p>")
        .nth(1)
        .unwrap_or_default()
        .chars()
        .take(400)
        .collect::<String>();
    assert!(
        host_p.contains("<w:pPr>") && host_p.contains("<w:sectPr"),
        "分節符必須落在目標段的 <w:pPr><w:sectPr>，實得 {host_p}"
    );

    // 節數 1→2：para_sec 出現第 1 節 id；段落數 base+1（PM 裁決 11-12：插分節符
    // 採「新增宿主页」口徑——python-docx add_section() 生態慣例即新增承載段，
    // 掛既有段會讓無辜段落悄然變成節收束點，編輯 API 非破壞性優先。斷言口徑
    // 同步改為「插入後 para_sec 長度＝插入後段落數＝base+1」，實作零讓步）。
    let ids = sec_ids(&e);
    assert_eq!(
        ids.len(),
        base + 1,
        "para_sec 長度必須恆等於插入後段落數（base+1）"
    );
    assert_eq!(
        e.paragraph_count(),
        base + 1,
        "宿主段插入後段落數必須 base+1"
    );
    assert_eq!(ids[0], 0, "分節符所在宿主段仍屬第 0 節");
    assert!(
        ids.contains(&1),
        "分節後其後段落必須歸第 1 節，實得 {ids:?}"
    );
    assert!(
        e.page_count() > pages_before,
        "nextPage 分節後頁數必須增加（顯示列表自洽：{pages_before} → {}）",
        e.page_count()
    );
    assert_eq!(
        find_para(&e, "表格後段落"),
        Some(13),
        "宿主段插在第 0 段前：其後段落索引整體 +1（12→13），除此之外零額外位移"
    );
    assert_touch_face(&e, &["word/document.xml"], "紅燈 4 插分節符");
    assert_indices_in_sync(&e, "紅燈 4 插分節符");

    // 存檔重開：節邊界逐位保留。
    let r = reopen(&e);
    assert_eq!(sec_ids(&r), ids, "存檔重開後 para_sec 必須與編輯態逐位相同");

    // 刪除分節符：回單節、索引回穩。
    e.apply_mutation(Mutation::DeleteSectionBreak { section_id: 0 })
        .expect("M6.2 DeleteSectionBreak 失敗");
    settle(&mut e);
    let ids2 = sec_ids(&e);
    assert_eq!(ids2.len(), base, "刪分節後 para_sec 長度仍＝段落數");
    assert!(
        ids2.iter().all(|id| *id == 0),
        "刪分節後必須回單節 id 0，實得 {ids2:?}"
    );
    assert_eq!(find_para(&e, "表格後段落"), Some(12), "段落索引不位移");
    assert_touch_face(&e, &["word/document.xml"], "紅燈 4 刪分節符");
    assert_indices_in_sync(&e, "紅燈 4 刪分節符");

    // undo 全退（插分節→刪分節→undo×2）：全檔 byte_identical。
    e.undo().expect("undo 刪分節失敗");
    settle(&mut e);
    e.undo().expect("undo 插分節失敗");
    settle(&mut e);
    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(
        diff.byte_identical,
        "undo 回初始態後存檔必須全檔 byte_identical，殘留 {:?}",
        diff.whitelisted_changed
    );
}

#[test]
fn k29d_lamp4_delete_root_sectpr_is_refused() {
    // table_mixed.docx 只有一個 body 級根 sectPr：不準刪（設計書 §2.3.6-2）。
    let mut e = open("fixtures/table_mixed.docx");
    settle(&mut e);
    let err = e
        .apply_mutation(Mutation::DeleteSectionBreak { section_id: 0 })
        .expect_err("根級 sectPr 的刪除必須誠實拒絕");
    assert!(
        matches!(err, quire_engine::MutationError::ProtectedStructure { .. }),
        "根 sectPr 拒絕型別必須是 ProtectedStructure，實得 {err:?}"
    );
    assert!(!e.can_undo(), "被拒絕的分節刪除不得留下 Undo 記錄");
}
