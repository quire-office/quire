//! K29a 紅燈測試 1–3：最小文字級閉環寫回、未觸碰 Part 零失真與 Undo 原位元組還原。
//!
//! 依據任務書 AGY-K29A 與 docs/edit-api-design.md §6.2 規範：
//! - 紅燈 1：文字編輯持久化閉環鎖（InsertText -> save -> reopen -> 樣式與文字留存）。
//! - 紅燈 2：未觸碰 part 零失真＋同 part 鄰段不擾動鎖（Surgical XML Patch 非全檔序列化）。
//! - 紅燈 3：Edit -> Undo -> Save 全檔 byte_identical 鎖（touched 自動退場）。

use quire_engine::{
    diff_untouched, BreakKind, DocPos, DocRange, DrawItem, Engine, Mutation, SubDocTarget,
    FLAG_BOLD,
};

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

fn load_corpus_bytes(rel: &str) -> Vec<u8> {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 corpus 失敗 {full}: {e}"))
}

/// 紅燈 1：文字編輯持久化閉環鎖。
/// 開 fixtures/style_cascade.docx，settle 後對第 0 段執行 InsertText，
/// save_docx() 後重開，斷言段落文字已更新且顯示列表文字仍帶有 FLAG_BOLD。
#[test]
fn k29a_lamp1_text_edit_persistence_closed_loop() {
    const FX: &[u8] = include_bytes!("../../../fixtures/style_cascade.docx");
    let mut engine = Engine::from_docx_bytes(FX).expect("開啟 fixtures/style_cascade.docx 失敗");
    settle(&mut engine);

    let op = Mutation::InsertText {
        target: SubDocTarget::Body,
        pos: DocPos { para: 0, cp: 8 },
        text: "（已存檔）".to_string(),
        style_override: None,
    };
    engine.apply_mutation(op).expect("套用 InsertText 失敗");

    let saved = engine.save_docx().expect("存檔失敗");
    let mut reopened = Engine::from_docx_bytes(&saved).expect("重新開檔失敗");
    settle(&mut reopened);

    assert_eq!(
        reopened.paragraph_text(0).as_deref(),
        Some("段落級自閉合粗體（已存檔）"),
        "重開後第 0 段文字必須持久化"
    );

    let p0_dl = reopened
        .page_display_list(0)
        .expect("第 0 頁顯示列表必須存在");
    let bold_item = p0_dl
        .items
        .into_iter()
        .find_map(|it| match it {
            DrawItem::Text { text, flags, .. } if text.contains("已存檔") => Some(flags),
            _ => None,
        })
        .expect("重開後新插入文字必須進入顯示列表");

    assert_ne!(
        bold_item & FLAG_BOLD,
        0,
        "重開後新插入文字仍必須保留段落樣式級聯之粗體旗標"
    );
}

/// 輔助函式：自 XML 明文提取 <w:body> 內所有頂層段落、表格與相容性容器切片。
fn extract_body_child_spans(xml: &[u8]) -> Vec<(usize, usize, Vec<u8>)> {
    let mut spans = Vec::new();
    let text = match std::str::from_utf8(xml) {
        Ok(t) => t,
        Err(_) => return spans,
    };
    let body_start_tag = match text.find("<w:body") {
        Some(pos) => pos,
        None => return spans,
    };
    let body_content_start = match text[body_start_tag..].find('>') {
        Some(offset) => body_start_tag + offset + 1,
        None => return spans,
    };
    let body_end_tag = match text[body_content_start..].find("</w:body>") {
        Some(offset) => body_content_start + offset,
        None => text.len(),
    };

    let slice = &text[body_content_start..body_end_tag];
    let mut cursor = 0;
    while cursor < slice.len() {
        let rel_lt = match slice[cursor..].find('<') {
            Some(i) => i,
            None => break,
        };
        let tag_start = cursor + rel_lt;
        let rem = &slice[tag_start..];
        let (tag_name, is_target) =
            if rem.starts_with("<w:p ") || rem.starts_with("<w:p>") || rem.starts_with("<w:p/>") {
                ("w:p", true)
            } else if rem.starts_with("<w:tbl ")
                || rem.starts_with("<w:tbl>")
                || rem.starts_with("<w:tbl/>")
            {
                ("w:tbl", true)
            } else if rem.starts_with("<mc:AlternateContent ")
                || rem.starts_with("<mc:AlternateContent>")
                || rem.starts_with("<mc:AlternateContent/>")
            {
                ("mc:AlternateContent", true)
            } else {
                ("", false)
            };

        if !is_target {
            let tag_end = match rem.find('>') {
                Some(i) => tag_start + i + 1,
                None => break,
            };
            cursor = tag_end;
            continue;
        }

        let first_gt = match rem.find('>') {
            Some(i) => i,
            None => break,
        };
        if rem[..first_gt].trim_end().ends_with('/') {
            let elem_end = tag_start + first_gt + 1;
            let abs_start = body_content_start + tag_start;
            let abs_end = body_content_start + elem_end;
            spans.push((abs_start, abs_end, xml[abs_start..abs_end].to_vec()));
            cursor = elem_end;
            continue;
        }

        let open_tag_prefix = format!("<{}", tag_name);
        let close_tag = format!("</{}>", tag_name);
        let mut depth = 0;
        let mut elem_cursor = tag_start;
        let mut found_end = None;

        while elem_cursor < slice.len() {
            let next_lt = match slice[elem_cursor..].find('<') {
                Some(i) => elem_cursor + i,
                None => break,
            };
            let tag_rem = &slice[next_lt..];
            let gt = match tag_rem.find('>') {
                Some(i) => next_lt + i,
                None => break,
            };
            let tag_inner = &slice[next_lt..=gt];
            let is_self_closing = tag_inner.trim_end().ends_with("/>");

            if tag_inner.starts_with(&open_tag_prefix) {
                let after_prefix = &tag_inner[open_tag_prefix.len()..];
                if (after_prefix.starts_with(' ')
                    || after_prefix.starts_with('>')
                    || after_prefix.starts_with('/'))
                    && !is_self_closing
                {
                    depth += 1;
                }
            } else if tag_inner == close_tag {
                depth -= 1;
                if depth == 0 {
                    found_end = Some(gt + 1);
                    break;
                }
            }
            elem_cursor = gt + 1;
        }

        if let Some(elem_end) = found_end {
            let abs_start = body_content_start + tag_start;
            let abs_end = body_content_start + elem_end;
            spans.push((abs_start, abs_end, xml[abs_start..abs_end].to_vec()));
            cursor = elem_end;
        } else {
            break;
        }
    }
    spans
}

/// 紅燈 2：未觸碰 part 零失真＋同 part 鄰段不擾動鎖。
#[test]
fn k29a_lamp2_untouched_parts_and_neighbor_paragraphs_zero_loss() {
    let targets = [
        "ooxmlexport/style-inheritance.docx",
        "ooxmlexport/mce-nested.docx",
    ];

    for rel in targets {
        let original_bytes = load_corpus_bytes(rel);
        let mut engine = Engine::from_docx_bytes(&original_bytes)
            .unwrap_or_else(|e| panic!("開啟 {rel} 失敗: {e}"));
        settle(&mut engine);

        // 取出開檔時 document.xml 明文切片以備後續比對
        let orig_doc_xml = engine
            .part("word/document.xml")
            .expect("必須有 word/document.xml")
            .to_vec();
        let orig_spans = extract_body_child_spans(&orig_doc_xml);
        assert!(!orig_spans.is_empty(), "{rel} 頂層結構元素切片提取不可為空");

        // 執行單段 M1.1 (InsertText) 與單段 M1.2 (DeleteRange)
        engine
            .apply_mutation(Mutation::InsertText {
                target: SubDocTarget::Body,
                pos: DocPos { para: 0, cp: 0 },
                text: "SURGICAL_TEST_PREFIX_".to_string(),
                style_override: None,
            })
            .expect("套用 M1.1 失敗");

        engine
            .apply_mutation(Mutation::DeleteRange {
                range: DocRange {
                    target: SubDocTarget::Body,
                    start: DocPos { para: 0, cp: 0 },
                    end: DocPos { para: 0, cp: 9 }, // 刪除 "SURGICAL_" 前 9 碼
                },
            })
            .expect("套用 M1.2 失敗");

        let diff = engine.roundtrip_check().expect("roundtrip_check 失敗");

        // (a) roundtrip_check().pass == true 且 whitelisted_changed == ["word/document.xml"]
        assert!(
            diff.pass,
            "{rel} roundtrip_check pass 必須為真，違規項: {:?}",
            diff.violations
        );
        assert_eq!(
            diff.whitelisted_changed,
            vec!["word/document.xml".to_string()],
            "{rel} 變動 part 必須僅有 word/document.xml"
        );

        // (b) 其餘全部 entry 的 zero_loss() 全 true
        for (name, entry_diff) in &diff.per_entry {
            if name != "word/document.xml" {
                assert!(
                    entry_diff.zero_loss(),
                    "{rel} 之 entry {name} 必須符合 zero_loss，摘要: {}",
                    entry_diff.summary()
                );
            }
        }

        // (c) word/document.xml 明文內，被改 <w:p> 以外所有 <w:p>/<w:tbl>/<mc:AlternateContent>
        // 切片與編輯前逐位元組相等
        let saved_bytes = engine.save_docx().expect("存檔失敗");
        let reopened = Engine::from_docx_bytes(&saved_bytes).expect("重新開檔失敗");
        let after_doc_xml = reopened
            .part("word/document.xml")
            .expect("必須有 word/document.xml");
        let after_spans = extract_body_child_spans(after_doc_xml);

        assert_eq!(
            orig_spans.len(),
            after_spans.len(),
            "{rel} 頂層結構元素總數在單段編輯後必須保持不變"
        );

        // 第 0 個元素被編輯，其餘所有元素位元組必須 100% 相同
        for i in 1..orig_spans.len() {
            assert_eq!(
                orig_spans[i].2, after_spans[i].2,
                "{rel} 第 {i} 個非目標元素切片必須逐位元組完全相等（證明非全檔重序列化）"
            );
        }
    }
}

/// 紅燈 3：Edit -> Undo -> Save 全檔 byte_identical 鎖。
#[test]
fn k29a_lamp3_edit_undo_save_byte_identical() {
    const FX: &[u8] = include_bytes!("../../../fixtures/style_cascade.docx");
    let mut engine = Engine::from_docx_bytes(FX).expect("開啟 fixtures/style_cascade.docx 失敗");
    settle(&mut engine);

    // 1. M1.1 InsertText
    engine
        .apply_mutation(Mutation::InsertText {
            target: SubDocTarget::Body,
            pos: DocPos { para: 0, cp: 0 },
            text: "TEST_UNDO_".to_string(),
            style_override: None,
        })
        .expect("第 1 步 InsertText 失敗");

    // 2. M1.5 InsertBreak (Page)
    engine
        .apply_mutation(Mutation::InsertBreak {
            target: SubDocTarget::Body,
            pos: DocPos { para: 0, cp: 10 },
            kind: BreakKind::Page,
        })
        .expect("第 2 步 InsertBreak 失敗");

    // 3. 單段 M1.2 DeleteRange
    engine
        .apply_mutation(Mutation::DeleteRange {
            range: DocRange {
                target: SubDocTarget::Body,
                start: DocPos { para: 0, cp: 0 },
                end: DocPos { para: 0, cp: 5 },
            },
        })
        .expect("第 3 步 DeleteRange 失敗");

    // 連續三次 undo() 回初始態
    let u1 = engine.undo().expect("第 1 次 undo 失敗");
    assert!(u1.is_some(), "第 1 次 undo 必須產生結果");

    let u2 = engine.undo().expect("第 2 次 undo 失敗");
    assert!(u2.is_some(), "第 2 次 undo 必須產生結果");

    let u3 = engine.undo().expect("第 3 次 undo 失敗");
    assert!(u3.is_some(), "第 3 次 undo 必須產生結果");

    // 存檔並比對
    let saved = engine.save_docx().expect("存檔失敗");
    let diff = diff_untouched(FX, &saved).expect("diff_untouched 執行失敗");

    assert!(
        diff.byte_identical,
        "三次 undo 回初始態後存檔必須全檔 byte_identical（Deflate 流原位復原）"
    );
    assert!(diff.pass, "diff.pass 必須為真");
}
