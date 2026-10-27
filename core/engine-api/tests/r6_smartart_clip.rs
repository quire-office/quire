//! R6 紅燈：fdo74605 SmartArt／Diagram 標註宿主框裁切（簇 B-2）。
//!
//! 來源：`docs/k5-diag-pagination.md` §2.4／§3 建議卡 #4。實據檔
//! `corpus/ooxmlexport/fdo74605.docx`（橫向 842×595；LO 2 頁／我方 2 頁）。
//! 缺陷：p1 的 `wps:wsp` 標註全部照發，抽取序與斷行都與 LO 分岔。
//! 本卡發射層（`core/engine-api`）對 `wps` 標註：
//! 1. 用 run 宣告字級（8pt）斷行，不再被文件基準 11pt 放大；
//! 2. 文字不越出宿主佔位框（標註矩形 ∩ 宿主框，越界行不發）；
//! 3. 依 `wp:anchor/@relativeHeight`（z 序）排抽取序（LO 內容流口徑）。
//!
//! 紅燈：
//! 1. p1 可抽行 ≤140（基線 158／報告 184）。
//! 2. 小寫 `policy and` 不出現（LO 全文 ×0；LO 的 `Policy and` 為大寫）。
//! 3. p1 內容相似度 ≥0.6（對 LO PDF 文字鍵；基線 0.379）。
//! 4. buggy 回塞鎖：`R6_CLIP_BUGGY=1` 關閉整組 R6 發射 → 1-3 必紅。
use quire_engine::Engine;
use quire_render_trait::DrawItem;

const DOC: &str = "ooxmlexport/fdo74605.docx";

/// LO p1 文字鍵（`render_lab.content_key` 口徑：正規化空白＋casefold＋
/// 逐行以單空格相接；PDF 由 LibreOffice headless 產生，124 行）。
const LO_P1_KEY: &str = r#"1 stronger gender knowledge and evidence policy and legislation review gender responsive health/edu services male leaders’ engagement violence against women is reduced, survivors of violence have access to support services and to justice women’s skills, knowledge and confidence women have expanded economic opportunities to earn income and accumulate economic assets women, and women’s interests, are increasingly represented, effective and visible in leadership positions at all levels of decision making women in the pacific participate fully, freely and safely in political, economic and social life legal, policy and regulatory frameworks legal, policy and regulatory frameworks income generation knowledge and skills policy, legislation and enforcement quality, affordable, integrated services prevention access to financial services australia’s areas of interest higher development outcomes policy engagement changes to social norms, values, practices advocacy/change agents/coalitions health and education increased economic opportunities reduce violence, expand services, and access to justice more supportive social norms for women’s agency increased women in decision making targeted communities in the pacific support women to have increased knowledge, skills and opportunities to enable them to participate in political, economic and social life pilots and impact evaluation local level leadership safe and productive markets financial literacy/ products services for survivors s justice -formal/non- formal group. community engagement mp and candidate support understandin g and knowledge how australia will work knowledge and management hub country led/ criteria driven support local initiative integrated approaches partnership with local actors sustained and long term diagram 1 strengthened women’s groups and coalitions long term institutional strengthening"#;

fn load() -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + DOC;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open")
}

fn norm_key(lines: &[String]) -> String {
    lines
        .iter()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn page_texts(eng: &Engine, page: usize) -> Vec<String> {
    let dl = eng.page_display_list(page).expect("頁存在");
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { text, .. } if !text.trim().is_empty() => Some(text.trim().to_string()),
            _ => None,
        })
        .collect()
}

/// 最長公共子序列長度（字元級；兩列滾動，O(n·m) 時間、O(m) 記憶體）。
fn lcs_len(a: &[char], b: &[char]) -> usize {
    let mut prev = vec![0u16; b.len() + 1];
    let mut cur = vec![0u16; b.len() + 1];
    for &ca in a {
        for (j, &cb) in b.iter().enumerate() {
            cur[j + 1] = if ca == cb {
                prev[j].saturating_add(1)
            } else {
                prev[j + 1].max(cur[j])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()] as usize
}

fn similarity(lo: &str, ow: &str) -> f64 {
    let a: Vec<char> = lo.chars().collect();
    let b: Vec<char> = ow.chars().collect();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let m = lcs_len(&a, &b) as f64;
    2.0 * m / (a.len() + b.len()) as f64
}

#[test]
fn r6_fdo74605_p1_meets_red_lights() {
    if std::env::var_os("R6_CLIP_BUGGY").is_some() {
        eprintln!("R6_CLIP_BUGGY 已設：本測試跳過綠燈斷言（由 buggy 回塞鎖驗紅）。");
        return;
    }
    let eng = load();
    assert_eq!(eng.page_count(), 2, "fdo74605 頁數必須維持 2（LO 同）");
    let lines = page_texts(&eng, 0);
    assert!(
        lines.len() <= 140,
        "fdo74605 p1 可抽行 {} > 140（基線 158／報告 184）",
        lines.len()
    );
    assert!(
        !lines.iter().any(|l| l.contains("policy and")),
        "fdo74605 p1 不得出現小寫 'policy and'（LO 全文 ×0）"
    );
    let ow_key = norm_key(&lines);
    let score = similarity(LO_P1_KEY, &ow_key);
    assert!(
        score >= 0.6,
        "fdo74605 p1 內容相似度 {score:.3} < 0.6（基線 0.379）"
    );
    eprintln!(
        "R6 綠燈：p1 可抽行 {}、policy and ×0、內容相似度 {score:.3}",
        lines.len()
    );
}

#[test]
fn r6_buggy_env_restores_red() {
    if std::env::var_os("R6_CLIP_BUGGY").is_none() {
        eprintln!("未設 R6_CLIP_BUGGY：本測試只驗回塞鎖（以 R6_CLIP_BUGGY=1 跑）。");
        return;
    }
    let eng = load();
    assert_eq!(eng.page_count(), 2, "回塞不得改頁數");
    let lines = page_texts(&eng, 0);
    let has_policy_and = lines.iter().any(|l| l.contains("policy and"));
    let score = similarity(LO_P1_KEY, &norm_key(&lines));
    eprintln!(
        "R6 回塞：p1 可抽行 {}（>140）、policy and ×{}、內容相似度 {score:.3}（<0.6）",
        lines.len(),
        lines.iter().filter(|l| l.contains("policy and")).count()
    );
    assert!(
        lines.len() > 140,
        "回塞後 p1 可抽行應 >140，實得 {}",
        lines.len()
    );
    assert!(has_policy_and, "回塞後小寫 'policy and' 應重現");
    assert!(score < 0.6, "回塞後內容相似度應 <0.6，實得 {score:.3}");
}
