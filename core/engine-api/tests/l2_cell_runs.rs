//! L2-cellruns 紅燈：表格格內文字多样式發射（清 TODO(CardA)）。
//!
//! 契約（卡面）：格內段落含 ≥2 種 run 樣式（b/i/sz/color 相異）時，發射層
//! 必須按 run 邊界拆多個 `DrawItem::Text`——相鄰項 flags/size 相異，且
//! 拼起來與整段文字**逐字元相等**（不丟字、不重複、不重排）。
use quire_engine::Engine;
use quire_render_trait::{DrawItem, FLAG_ITALIC};

fn load(path: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + path;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open")
}

/// 同頁文字項抽取：(y, x, size_pt, flags, text)。
fn texts_of(dl: &quire_render_trait::PageDisplayList) -> Vec<(f32, f32, f32, u8, String)> {
    let mut v: Vec<(f32, f32, f32, u8, String)> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { x, y, size_pt, flags, text, .. } => {
                Some((*y, *x, *size_pt, *flags, text.clone()))
            }
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.partial_cmp(&b.1).unwrap()));
    v
}

/// 紅燈 1（form_footnotes）：「01 -   Full name」格內整段
/// `01 -   Full name  (as per passport; do not abbreviate or omit any name)` 中
/// `(as per passport…` 為斜體 run（且 sz=18）。整格單樣式發射＝單一 Text
/// 項；修後必須拆出「斜體段帶 ITALIC、非斜體段不带」的相鄰雙項，且兩段
/// 拼接為來源整段的連續子串（逐字元核）。
#[test]
fn l2_form_footnotes_italic_split() {
    let eng = load("poi/test-data_document_form_footnotes.docx");
    let para = "01 -   Full name  (as per passport; do not abbreviate or omit any name)";
    let mut found: Option<(usize, String, String, u8, u8)> = None;
    'pages: for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        let ts = texts_of(&dl);
        // 依 y 分組掃相鄰對
        let mut i = 0;
        while i < ts.len() {
            let mut j = i + 1;
            while j < ts.len() && (ts[j].0 - ts[i].0).abs() < 0.01 {
                j += 1;
            }
            for w in ts[i..j].windows(2) {
                // 兩段都必須實文字（回塞防呆：舊路徑整行單項＋空項併發時
                // 空項會被誤判成「第二段」，偽綠）。
                if w[0].4.is_empty() || w[1].4.is_empty() {
                    continue;
                }
                let joined = w[0].4.clone() + &w[1].4;
                let flags_differ = w[0].3 != w[1].3;
                if flags_differ && !joined.is_empty() && para.contains(&joined) {
                    // 斜體必須只染在其中一段（OR 染整行＝兩段都帶ITALIC＝不合格）
                    let a_it = w[0].3 & FLAG_ITALIC != 0;
                    let b_it = w[1].3 & FLAG_ITALIC != 0;
                    if a_it != b_it {
                        found = Some((p, w[0].4.clone(), w[1].4.clone(), w[0].3, w[1].3));
                        break 'pages;
                    }
                }
            }
            i = j;
        }
    }
    let Some((p, a, b, fa, fb)) = found else {
        panic!(
            "form_footnotes：找不到相鄰 flags 相異（且斜體只染一段）並拼接為整段連續子串的 Text 項。\n\
             整段來源＝{para:?}"
        );
    };
    println!("p{p} split: {a:?}({fa}) | {b:?}({fb})");
}

/// 紅燈 2（bug65649）：格內段落「Огнетушащая … концентрация: 3,42」的
/// `[\xa0].` 片段 run 帶**字級差異**（sz 24 vs 28 half-pt → 12pt vs 14pt）。
/// 修後同基線必須出現 size_pt 相異的相鄰 Text 項，且兩項拼接為來源段落的
/// 連續子串（逐字元核——不丟字、不重複、不重排）。
///
/// 取捨注記（卡面「form_footnotes 或 fdo68607」的實測修正）：fdo68607 表頭
/// 格的 sz48「IGCSE Drama Glossary」實為格內**文字框**內容
/// （`mc:AlternateContent`→`w:pict`→`v:textbox`，走 K4e2 `emit_txbx_texts`
/// 路徑）——卡面鐵律「格內文字框不在本卡範圍、別碰」，該檔格內主線段落
/// 只有單 run「Acting area」，無格內多樣式可拆，故字級紅燈改錨 bug65649
/// （實測：真·格內段落雙字級）。fdo68607 仍留儀表板 A/B 觀察。
#[test]
fn l2_bug65649_size_split() {
    let eng = load("poi/test-data_document_bug65649.docx");
    // 來源段落（run 級 sz 24 vs 28 half-pt → 12pt vs 14pt；\xa0 run 為大字級）。
    let para = "Огнетушащая (рабочая) концентрация: 3,42\u{a0}%.";
    let mut found = false;
    let mut diag = String::new();
    'pages: for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        let ts = texts_of(&dl);
        for w in ts.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            // 兩段都必須實文字（回塞防呆：舊路徑未拆時整段為單項，相鄰
            // 他格項不得冒充第二段）。
            if a.4.is_empty() || b.4.is_empty() {
                continue;
            }
            // 嚴格錨定：相鄰兩項拼接必須是**本來源段落的連續子串**且含
            // 錨字（不丟字、不重複、不重排），同基線、字級相異。
            let joined = a.4.clone() + &b.4;
            let same_line = (a.0 - b.0).abs() < 0.01 && b.1 > a.1;
            let anchored = joined.contains("\u{a0}%") && para.contains(&joined);
            diag.push_str(&format!("p{p} {a:?} | {b:?}\n"));
            if same_line && anchored && (a.2 - b.2).abs() > 0.5 {
                found = true;
                break 'pages;
            }
        }
    }
    assert!(
        found,
        "bug65649：找不到同基線 size_pt 相異、拼接為來源段落連續子串的相鄰 Text 項（整格單樣式未拆）。診断：\n{diag}"
    );
}
