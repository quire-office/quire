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
/// `01 -   Full name  (as per passport; do not abbreviate or omit)` 中
/// `(as per passport…` 為斜體 run（且 sz=18）。整格單樣式發射＝單一 Text
/// 項；修後必須拆出「斜體段帶 ITALIC、非斜體段不带」的相鄰雙項，且兩段
/// 拼接為來源整段的連續子串（逐字元核）。
#[test]
fn l2_form_footnotes_italic_split() {
    let eng = load("poi/test-data_document_form_footnotes.docx");
    let para = "01 -   Full name  (as per passport; do not abbreviate or omit)";
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

/// 紅燈 2（fdo68607）：表頭格「IGCSE Drama Glossary」run 帶**字級差異**
/// （sz 28 vs 48 half-pt → 14pt vs 24pt）。修後同基線必須出現 size_pt 相異
/// 的相鄰 Text 項（且拼接不超出來源文字——以兩項皆含目標詞片段佐證）。
#[test]
fn l2_fdo68607_size_split() {
    let eng = load("ooxmlexport/fdo68607.docx");
    let mut found = false;
    let mut diag = String::new();
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        let ts = texts_of(&dl);
        let hits: Vec<&(f32, f32, f32, u8, String)> = ts
            .iter()
            .filter(|t| t.4.contains("Drama") || t.4.contains("Glossary") || t.4.contains("Acting"))
            .collect();
        for w in hits.windows(2) {
            diag.push_str(&format!("p{p} {:?} | {:?}\n", &w[0], &w[1]));
            if (w[0].0 - w[1].0).abs() < 0.01 && (w[0].2 - w[1].2).abs() > 0.5 {
                found = true;
            }
        }
    }
    assert!(
        found,
        "fdo68607：找不到同基線 size_pt 相異的相鄰 Text 項（整格單樣式未拆）。診断：\n{diag}"
    );
}
