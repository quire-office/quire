//! K29d 除錯腳本（交付前刪除）：印出各紅燈失敗點的實際狀態。
use quire_engine::{
    DocPos, Engine, HfBindAction, HfKind, HfSlotKind, Mutation, NoteKind2, NoteNumberFormat,
    NotePropsPatch, NoteRestart, PageOrientation, SectionColsPatch, SectionPropsPatch,
    SectionStartKind, SubDocTarget,
};

fn load(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    Engine::from_docx_bytes(&std::fs::read(&full).expect(rel)).expect("open")
}

fn tail(s: &str, n: usize) -> String {
    s.chars().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect()
}

fn seg_of(e: &Engine) -> String {
    let d = String::from_utf8_lossy(e.part("word/document.xml").unwrap()).to_string();
    match d.find("<w:sectPr") {
        Some(i) => d[i..].chars().take(500).collect(),
        None => "<NO SECTPR>".to_string(),
    }
}

#[test]
fn dbg_probe() {
    let seg = "<w:sectPr><w:pgSz w:w=\"11900\" w:h=\"16840\"/><w:pgMar w:top=\"1440\"/><w:cols w:space=\"425\"/><w:docGrid w:linePitch=\"360\"/></w:sectPr>";
    println!("TRACE_LOC_PGSZ={:?}", quire_engine::mutation::mutation_k29d_probe(seg, "w:pgSz"));
    println!("TRACE_LOC_COLS={:?}", quire_engine::mutation::mutation_k29d_probe(seg, "w:cols"));
    let mut e = load("fixtures/sample.docx");
    let r = e.apply_mutation(Mutation::SetSectionProperties {
        section_id: 0,
        patch: SectionPropsPatch {
            orientation: Some(PageOrientation::Landscape),
            ..Default::default()
        },
    });
    println!("PROBE1 ORIENT_RESULT={r:?}");
    println!("PROBE1 SEG={}", seg_of(&e));

    // 2) l1_two_col 單欄
    let mut e2 = load("fixtures/l1_two_col.docx");
    let r2 = e2.apply_mutation(Mutation::SetSectionProperties {
        section_id: 0,
        patch: SectionPropsPatch {
            cols: Some(SectionColsPatch { num: 1, space_pt: None }),
            ..Default::default()
        },
    });
    println!("PROBE2 ONECOL_RESULT={r2:?}");
    println!("PROBE2 SEG={}", seg_of(&e2));

    // 3) table_mixed titlePg on/off
    let mut e3 = load("fixtures/table_mixed.docx");
    let r3a = e3.apply_mutation(Mutation::ConfigureHeaderFooterSlot {
        section_id: 0,
        title_pg: Some(true),
        even_and_odd_headers: None,
        slot_bind: None,
    });
    println!("PROBE3 TITLEPG_ON={r3a:?} SEG={}", seg_of(&e3));
    let r3b = e3.apply_mutation(Mutation::ConfigureHeaderFooterSlot {
        section_id: 0,
        title_pg: Some(false),
        even_and_odd_headers: None,
        slot_bind: None,
    });
    println!("PROBE3 TITLEPG_OFF={r3b:?} SEG={}", seg_of(&e3));

    // 4) hf_table_header footer 路由＋頁首基準
    let e4 = load("fixtures/hf_table_header.docx");
    println!("PROBE4 HF_PAGES={} SECTPR={}", e4.page_count(), seg_of(&e4));

    // 5) form_footnotes InsertNote
    let mut e5 = load("corpus/poi/test-data_document_form_footnotes.docx");
    let host = (0..e5.paragraph_count())
        .find(|&i| e5.paragraph_text(i).is_some_and(|t| t.contains("Testdoc")))
        .unwrap_or(0);
    let r5 = e5.apply_mutation(Mutation::InsertNote {
        kind: NoteKind2::Footnote,
        pos: DocPos { para: host, cp: 0 },
        initial_text: String::new(),
    });
    println!("PROBE5 NOTE_INS={r5:?} host={host}");
    let nf = String::from_utf8_lossy(e5.part("word/footnotes.xml").unwrap()).to_string();
    println!("PROBE5 NOTES_TAIL={}", tail(&nf, 500));

    // 6) table_mixed InsertSectionBreak 段落數
    let mut e6 = load("fixtures/table_mixed.docx");
    println!("PROBE6 TM_PARA0={}", e6.paragraph_count());
    let r6 = e6.apply_mutation(Mutation::InsertSectionBreak {
        para: 0,
        kind: SectionStartKind::NextPage,
    });
    println!("PROBE6 BREAK_INS={r6:?}");
    println!(
        "PROBE6 TM_PARA1={} SECS={:?} SEG={}",
        e6.paragraph_count(),
        e6.para_sections(),
        seg_of(&e6)
    );

    // 7) SetNoteProperties
    let mut e7 = load("corpus/poi/test-data_document_form_footnotes.docx");
    let r7 = e7.apply_mutation(Mutation::SetNoteProperties {
        kind: NoteKind2::Footnote,
        patch: NotePropsPatch {
            num_fmt: Some(NoteNumberFormat::LowerRoman),
            num_restart: Some(NoteRestart::EachPage),
            num_start: Some(1),
            pos: None,
        },
    });
    println!("PROBE7 NOTEPROPS={r7:?}");
    let st = String::from_utf8_lossy(e7.part("word/settings.xml").unwrap()).to_string();
    let i = st.find("footnotePr").unwrap_or(0);
    println!("PROBE7 ST_FPR={}", st[i..(i + 300).min(st.len())].chars().collect::<String>());

    // 8) hf_table_header footer 綁定解析（footer1.xml 是 default slot？）
    let mut e8 = load("fixtures/hf_table_header.docx");
    let r8 = e8.apply_mutation(Mutation::InsertText {
        target: SubDocTarget::Footer { section_id: 0, slot: HfSlotKind::Default },
        pos: DocPos { para: 0, cp: 0 },
        text: "頁尾覆核".to_string(),
        style_override: None,
    });
    println!("PROBE8 FOOTER_INS={r8:?}");
    let r8b = e8.apply_mutation(Mutation::ConfigureHeaderFooterSlot {
        section_id: 0,
        title_pg: None,
        even_and_odd_headers: None,
        slot_bind: Some((HfKind::Footer, HfSlotKind::Default, HfBindAction::Unbind)),
    });
    println!("PROBE8 UNBIND={r8b:?} SEG={}", seg_of(&e8));
}

#[test]
fn dbg_para_count_vs_para_sec() {
    let mut e = open_t("fixtures/table_mixed.docx");
    settle(&mut e);
    println!("PROBE_BASE n_paras={} para_sec={:?}", e.paragraph_count(), e.para_sections());
    e.apply_mutation(Mutation::InsertSectionBreak { para: 5, kind: SectionStartKind::NextPage }).unwrap();
    settle(&mut e);
    println!("PROBE_BREAK n_paras={} para_sec={:?}", e.paragraph_count(), e.para_sections());
}
