//! E1-C：`face_advance_scale("trebuchet ms")` 入表鎖（水平 advance 尺，正燈）。
//!
//! # 為什麼有這張考卷
//!
//! `corpus/ooxmlexport/cell-grid-span.docx` 的 Team 段宣告 `Trebuchet MS`（段標 rPr ＋
//! 9 個 run 的 rPr 都宣告，`w:rFonts w:ascii/hAnsi`）。查表無臂時走 `_ => 1.0`，
//! 該段斷行與 LO 真值**逐行全等 0/9**。本卡入表 0.9744 後升到 **6/9**，
//! 且入表前後**頁數零變動**（400 檔掃描 0 檔改變）——這是「尺被斷行吃掉」的行為級證據。
//!
//! # LO 真值口徑（本卡定錨，逐條見 `docs/e1c-advance-anatomy.md` §2）
//!
//! 基準＝**真 face 裝機態**：Linux LO 25.8 用 private fontconfig 目錄 per-process 注入
//! Windows 裝機的 `trebuc.ttf`（`FONTCONFIG_FILE` 注入，零全域副作用）後，
//! span font 從頂替的 `NotoSans-Regular` 變成 `TrebuchetMS`，該段 **9 行**
//! 且與 Windows LO（`scripts/render_lab.py::find_soffice()` 實測那支 `soffice.exe`）
//! **逐行全等** ⇒ 兩環境收斂，可作入表基準。
//! 未注入時 Linux LO 頂替 NotoSans ⇒ 該段 10 行，那是**裝機假象不是真值**——
//! 前一版 A 軌尺 1.0359（>1，t_8573d294 記帳）正是拿頂替態斷行當基準量的，方向相反，
//! 入表實跑斷點 0/10，即卡面所記「負結果」。
//!
//! 真值 9 行逐行由 `pymupdf` `get_text("dict")` 抽出（腳本：kanban scratch
//! `anchor_env.py`＋`emit_fixture.py`；原始 JSON `e1c_anchor.json`）。
//!
//! # 鑑別力（比照 `k7_advance_scale.rs` 口徑：進程內不設 env，外部驗證）
//!
//! 本檔斷言由 `face_advance_scale` 的 Trebuchet 臂驅動。外部執行
//! ```text
//! K7_ADVANCE_SCALE_BUGGY=1 cargo test -p quire-engine --test e1c_advance_trebuchet
//! ```
//! ⇒ 全表退 1.0（既有回塞鎖，比照 `R2_BUGGY`／`K6_AUTO_PITCH_BUGGY`）
//! ⇒ 燈 2 的匹配數退回 **0/9 必紅**，證明本檔真的在比斷行、不是恆真
//! （實跑紀錄見提交說明與 `docs/e1c-advance-anatomy.md` §4.2）。
//!
//! # 本檔**不**斷言的事（老實記帳）
//!
//! - 不斷言 9/9：殘餘 3 行差異（行 0/1/2 位移一格）根因＝源檔 3 處**連續雙空格**
//!   我方未折疊（LO 折疊；每處虛增 2.24pt，共 6.72pt，恰吃掉可行帶餘量）。
//!   那是水平度量軸的另一張卡（卡 D），與本臂正交。燈 3 用「整段文字守恆」堵掉
//!   「靠丟字湊行數」的假綠。
//! - 不斷言 p1 首末行：我方 p0 多吞 contact 段落（真值落 p1），需動 `pack_run`
//!   首行計帶，實測會把 RA001 由真值 6 頁改壞成 7 頁 ⇒ 見 `e1_pagination.rs`
//!   lamp1 p1 盞的 `#[ignore]` 字串。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

const GRID_SPAN: &str = "ooxmlexport/cell-grid-span.docx";

/// 定錨 LO 真值：Team 段 9 行（起 `Team, Passion`、迄 `country.`，排他於
/// 下一起始段 `For further media information, contact:`）。
const TRUTH_TEAM_LINES: [&str; 9] = [
    "Team, Passion, Innovation….the brand iBall stands by these three pillars of strength, which has helped",
    "write a success story, not just for the company alone, but for all associated with it. Launched in 2001",
    "with a single product category has today a gigantic range of over 300 products in its 26 product",
    "categories. It has also launched over 35 products with new technologies for the first time in India.",
    "iBall’s last major category launch was iBall Slide (Tablet PC), during September. iBall has already sold",
    "over 24 million products. The Company has a strong pan India presence with 24 branch offices across",
    "the country, with its products available in over 400 cities and towns. iBall products are serviced at its",
    "self-owned over 125 service centres spread across India. iBall is a well accepted brand in the corporate",
    "world and is fast becoming a household name throughout the country.",
];

/// 正規化：折疊任意空白（源檔有連續雙空格，LO 折疊）、全小寫、統一連結號。
fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .replace(['\u{2019}', '\u{2018}'], "'")
}

/// 我方 Team 段斷行：跨頁掃描 p0..p3，取 `Team, Passion` 起、`For further` 前（排他），
/// 依 y 分組同行、x 排序拼字（顯示列表序）。回傳 (頁數, 段行文字)。
fn my_team_segment(eng: &Engine) -> (usize, Vec<String>) {
    let mut pages: Vec<Vec<String>> = Vec::new();
    for p in 0..eng.page_count() {
        let dl = eng
            .page_display_list(p)
            .unwrap_or_else(|| panic!("第 {p} 頁顯示列表應存在"));
        let mut buckets: std::collections::BTreeMap<i64, Vec<(f32, String)>> =
            std::collections::BTreeMap::new();
        for it in &dl.items {
            if let DrawItem::Text { x, y, text, .. } = it {
                if !text.trim().is_empty() {
                    // 0.5pt y-bucket 去重（比照 k7_advance_scale.rs 口徑）。
                    buckets
                        .entry((*y * 2.0).round() as i64)
                        .or_default()
                        .push((*x, text.clone()));
                }
            }
        }
        let mut lines: Vec<String> = Vec::new();
        for (_, mut runs) in buckets {
            runs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            lines.push(runs.into_iter().map(|(_, t)| t).collect::<String>());
        }
        pages.push(lines);
    }
    let flat: Vec<String> = pages.into_iter().flatten().collect();
    let start = flat
        .iter()
        .position(|t| t.starts_with("Team, Passion"))
        .expect("Team 段起始行必須存在");
    let end = flat[start + 1..]
        .iter()
        .position(|t| t.starts_with("For further"))
        .map(|k| start + 1 + k)
        .unwrap_or(flat.len());
    (eng.page_count(), flat[start..end].to_vec())
}

/// 燈 1：頁數 ＋ 該段行數對定錨真值（入表前後都應為 2 頁／9 行——本卡實測
/// 尺入表不動頁數，行數則正是入表要守的目標）。
#[test]
fn lamp1_page_count_and_team_line_count_match_anchored_lo() {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + GRID_SPAN;
    let eng = Engine::from_docx_bytes(&std::fs::read(&full).expect("讀 corpus")).expect("open");
    let (pages, seg) = my_team_segment(&eng);
    assert_eq!(
        pages, 2,
        "cell-grid-span：定錨 LO 真值 2 頁（Linux LO 頂替態／定錨態／Windows 板三者皆 2 頁）"
    );
    assert_eq!(
        seg.len(),
        TRUTH_TEAM_LINES.len(),
        "Team 段須吃 9 行＝定錨 LO 真值（真 face 裝機態；未注入時頂替 Noto 會吃 10 行）。實得 {} 行：{:#?}",
        seg.len(),
        seg
    );
}

/// 燈 2：**入表鎖**。我方逐行斷點對定錨真值的匹配數 ≥6（實測 6/9）。
/// 尺退 1.0（`K7_ADVANCE_SCALE_BUGGY=1`）時退回 0/9 ⇒ 本燈必紅，鑑別力在此。
#[test]
fn lamp2_team_line_breaks_match_anchored_lo() {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + GRID_SPAN;
    let eng = Engine::from_docx_bytes(&std::fs::read(&full).expect("讀 corpus")).expect("open");
    let (_, seg) = my_team_segment(&eng);
    let matched = seg
        .iter()
        .zip(TRUTH_TEAM_LINES)
        .filter(|(mine, want)| norm(mine) == norm(want))
        .count();
    assert!(
        matched >= 6,
        "face_advance_scale(\"trebuchet ms\")＝0.9744 入表後，該段逐行斷點須匹配定錨 LO 真值 ≥6/9\
         （實測 6/9；未入表＝尺退 1.0 時匹配 0/9，本燈即鑑別力所在）。實得 {matched}/{}：{:#?}",
        TRUTH_TEAM_LINES.len(),
        seg
    );
    // 行 3..8（索引 3 起）是入表要鎖死的穩定群：尺一走樣（含卡 C 落地後未重掃）先在這裡紅。
    for idx in 3..TRUTH_TEAM_LINES.len() {
        assert_eq!(
            norm(&seg[idx]),
            norm(TRUTH_TEAM_LINES[idx]),
            "Team 段第 {idx} 行斷點偏離定錨 LO 真值——水平尺走樣或被覆寫。\
             我方={:?} 真值={:?}",
            seg[idx],
            TRUTH_TEAM_LINES[idx]
        );
    }
}

/// 燈 3：防假綠——整段文字守恆（折疊空白後逐字元比）。斷行匹配數可以靠丟字湊，
/// 文字守恆不行。段內所有詞都必須在，且不長出沒有的詞。
#[test]
fn lamp3_team_segment_text_is_conserved() {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + GRID_SPAN;
    let eng = Engine::from_docx_bytes(&std::fs::read(&full).expect("讀 corpus")).expect("open");
    let (_, seg) = my_team_segment(&eng);
    let mine = norm(&seg.join(" "));
    let want = norm(&TRUTH_TEAM_LINES.join(" "));
    assert_eq!(
        mine, want,
        "Team 段整段文字（折疊空白後）須與定錨 LO 真值逐字元相同——\
         斷行可以不同，但不得丟字、不得多字"
    );
}
