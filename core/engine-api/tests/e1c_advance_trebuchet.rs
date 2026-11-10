//! E1-C（含卡 B 垂直尺重定錨）：`Trebuchet MS` 水平 advance 尺（0.9744）與垂直 real-LH 尺（1.1611）入表鎖。
//!
//! # 為什麼有這張考卷
//!
//! `corpus/ooxmlexport/cell-grid-span.docx` 的 Team 段宣告 `Trebuchet MS`（段標 rPr ＋ 9 個 run 的 rPr 都宣告，`w:rFonts w:ascii/hAnsi`）。查表無臂時走 `_ => 1.0`，該段斷行與 LO 真值**逐行全等 0/9**；水平尺入表 0.9744 後升到 **6/9**（卡 C 標點覆寫臂＋卡 E 連動後現為 **9/9**，見下方卡 E 記帳）。垂直軸在 E1-A 曾按 Linux 未注入頂替態（NotoSans-Regular 1.3620）入表，導致 Team 段（繼承 docDefaults `w:line="276"`）行距推進量（pitch）實測 15.63pt、直接宣告 `w:line="240"` 段實測 13.60pt，較定錨真 face 真值（13.30pt／11.60pt）偏大 17.3%。本卡（E1-C 卡 B）依定案之 LO 真 face 裝機態口徑將 `face_line_factor("trebuchet ms")` 由 1.3620 重定錨為真值 **1.1611**，使垂直 pitch 與水平 advance 同源對齊真 `trebuc.ttf`。
//!
//! # LO 真值口徑（本卡定案，逐條見 `docs/e1cb-lo-truth-canon.md` 與 `docs/e1c-advance-anatomy.md` §2）
//!
//! 基準＝**真 face 裝機態**：Linux LO 25.8.7.3 用 private fontconfig 目錄 per-process 注入 Windows 裝機的 `trebuc*.ttf`（`FONTCONFIG_FILE` 注入，零全域副作用）後，span font 從頂替的 `NotoSans-Regular` 變成 `TrebuchetMS`／`TrebuchetMS-Bold`，Team 段 **9 行**、行距 pitch **13.30pt**（`w:line="240"` 段 **11.60pt**），且全篇 2 頁 `[36, 1]` 與 Windows LO（`soffice.exe`）**37 行文字、字型、y0（誤差 ≤0.05pt）逐行全等** ⇒ 兩環境完全收斂，作為定案基準。未注入時 Linux LO 頂替 NotoSans（hhea 1.3620）吃 10 行、pitch 15.65pt／13.65pt，屬裝機缺字假象而非真值。
//!
//! # 雙軸鑑別力（比照 `k6_auto_pitch.rs` 與 `k7_advance_scale.rs` 口徑：進程內不設 env，外部驗證）
//!
//! - 垂直軸回塞鎖：`K6_AUTO_PITCH_BUGGY=1 cargo test -p quire-engine --test e1c_advance_trebuchet` ⇒ 退回舊粗尺（Team 段 pitch 約 15.7pt、`w:line=240` 段約 13.65pt），燈 1 的垂直 pitch 斷言**必紅**（未重定錨前 1.3620 頂替口徑下實得 15.63pt／13.60pt 同樣必紅）。
//! - 水平軸回塞鎖：`K7_ADVANCE_SCALE_BUGGY=1 cargo test -p quire-engine --test e1c_advance_trebuchet` ⇒ 全表退 1.0，燈 2 的斷行匹配數退回 **0/9 必紅**。
//!
//! # 卡 E（t_0e390551）對本檔的翻案記帳（2026-11-10）
//!
//! 舊版寫「不斷言 9/9：殘餘 3 行差異（行 0/1/2 位移一格）」。卡 E 在**含卡 C 的基線**
//! （ef14bc9）上以 rawdict 真值口徑重跑斷行量測：該段已 **9/9 逐行全等**——PM 值守
//! 11-10 起手自證閘門的「連動根治」分支坐實，燈 2 門檻由 ≥6 tightening 至 **9/9**、
//! 穩定群由行 3..8 擴為全段行 0..8，防日後靜默退回。根因（實測，非推估）：行 0
//! `Innovation…` 的 U+2026 舊吃內嵌子集 1.0 em 方塊（0.9744 尺後 9.744pt@10pt）、
//! 真 trebuc.ttf hmtx＝7.344pt（覆寫臂取代不疊乘 ⇒ 行 0 淨縮 2.400pt）——`helped`
//! 由放不下翻成放得下，行 0/1/2「位移一格」的鏈式錯位整體消解。三段歸因：
//! base(279c1ee＝卡 D 態) 6/9 → head(ef14bc9＝含卡 C) 9/9 → 同碼只拔 Trebuchet
//! 標點臂 6/9（實據 docs/e1c-card-e-team-99.md）⇒ 唯一變量＝標點臂；斷行需求帶的
//! 逐字元瓜分未做（引擎端含 kerning/空格細項，pymupdf 裸 hmtx 模型復現不了 base
//! 態的「放不下 help」判定的完整算式），故不宣稱翻轉的臨界餘量數值。
//! 水平尺 0.9744 未動；對齊／tab stop／段寬三候選不構成殘錯位成因（拔臂態與
//! base 錯位逐行相同，幾何在兩態間未變，翻轉量全部由標點臂承接）。
//! 卡面舊歸因「源檔 3 處連續雙空格我方未折疊（LO 折疊；每處虛增 2.24pt）」早已被
//! pymupdf rawdict 逐字真值推翻——LO 對節點內雙空格渲染**兩個空格字形**（受控
//! sp2 gap＝12pt 佐證），不折疊；舊誤判源自 get_text("words") 抽詞把空白折成單字
//! 的抽取器假象。空白口徑定案見 `docs/e1cd-xml-space-policy.md`。燈 3 用「整段文字
//! 守恆」堵掉「靠丟字湊行數」的假綠。

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

/// 正規化：比對用（折疊任意空白、全小寫、統一連結號）。注意這是**內容比對容錯**，
/// 不代表 LO 折疊空白——LO 對節點內雙空格渲染兩個空格字形（rawdict 實測，
/// 見 docs/e1cd-xml-space-policy.md）；此處折疊只為免除斷行位置差異對文字守恆燈的干擾。
fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .replace(['\u{2019}', '\u{2018}'], "'")
}

/// 跨頁掃描全部文字行（依 0.5pt y-bucket 分組同行、x 排序拼字），回傳 `(y_pt, line_text)` 序列。
fn all_lines_with_y(eng: &Engine) -> Vec<(f32, String)> {
    let mut out: Vec<(f32, String)> = Vec::new();
    for p in 0..eng.page_count() {
        let dl = eng
            .page_display_list(p)
            .unwrap_or_else(|| panic!("第 {p} 頁顯示列表應存在"));
        let mut buckets: std::collections::BTreeMap<i64, Vec<(f32, f32, String)>> =
            std::collections::BTreeMap::new();
        for it in &dl.items {
            if let DrawItem::Text { x, y, text, .. } = it {
                if !text.trim().is_empty() {
                    buckets.entry((*y * 2.0).round() as i64).or_default().push((
                        *x,
                        *y,
                        text.clone(),
                    ));
                }
            }
        }
        for (_, mut runs) in buckets {
            runs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let y_pt = runs[0].1;
            let text = runs.into_iter().map(|(_, _, t)| t).collect::<String>();
            out.push((y_pt, text));
        }
    }
    out
}

/// 我方 Team 段斷行：取 `Team, Passion` 起、`For further` 前（排他），回傳 `(頁數, 段行 y 座標序列, 段行文字序列)`。
fn my_team_segment(eng: &Engine) -> (usize, Vec<f32>, Vec<String>) {
    let flat = all_lines_with_y(eng);
    let start = flat
        .iter()
        .position(|(_, t)| t.starts_with("Team, Passion"))
        .expect("Team 段起始行必須存在");
    let end = flat[start + 1..]
        .iter()
        .position(|(_, t)| t.starts_with("For further"))
        .map(|k| start + 1 + k)
        .unwrap_or(flat.len());
    let slice = &flat[start..end];
    (
        eng.page_count(),
        slice.iter().map(|(y, _)| *y).collect(),
        slice.iter().map(|(_, t)| t.clone()).collect(),
    )
}

/// 燈 1：頁數（2 頁）＋ Team 段行數（9 行）＋ 垂直尺 `face_line_factor("trebuchet ms") = 1.1611` 重定錨後的段內行距（pitch）對齊定錨 LO 真值。
/// Team 段（10pt，繼承 docDefaults `w:line="276"`＝1.15 倍）定錨 LO pitch 為 13.30pt（公式 10×1.15×1.1611＝13.35pt，與同檔 Calibri 11pt 13.43pt 共箱量化後為 13.53pt，落 K6 ±0.30pt 容差內）；直接宣告 `w:line="240"` 的 `Available at` 段（10pt）定錨 LO pitch 為 11.60pt（公式 10×1.0×1.1611＝11.61pt，量化後 11.60pt）。舊頂替口徑 1.3620（15.63pt／13.60pt）與 `K6_AUTO_PITCH_BUGGY=1`（12.70pt）皆必紅。
#[test]
fn lamp1_page_count_and_team_line_count_match_anchored_lo() {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + GRID_SPAN;
    let eng = Engine::from_docx_bytes(&std::fs::read(&full).expect("讀 corpus")).expect("open");
    let (pages, team_ys, seg) = my_team_segment(&eng);
    assert_eq!(
        pages, 2,
        "cell-grid-span：定錨 LO 真值 2 頁（Linux LO 定錨態／Windows 板皆 2 頁［36, 1］）"
    );
    assert_eq!(
        seg.len(),
        TRUTH_TEAM_LINES.len(),
        "Team 段須吃 9 行＝定錨 LO 真值（真 face 裝機態；未注入時頂替 Noto 會吃 10 行）。實得 {} 行：{:#?}",
        seg.len(),
        seg
    );
    let team_gaps: Vec<f32> = team_ys.windows(2).map(|w| w[1] - w[0]).collect();
    for (i, &g) in team_gaps.iter().enumerate() {
        assert!(
            (g - 13.30).abs() <= 0.30,
            "Team 段（10pt w:line=276）第 {i} 個行距 {g:.2}pt 偏離定錨真 face 真值 13.30±0.30pt（1.1611 重定錨口徑；若為 15.63pt 則是 1.3620 Noto 頂替舊口徑未更新，若為 12.70pt 則是 K6_AUTO_PITCH_BUGGY 回塞態）：gaps={team_gaps:?}"
        );
        assert!(
            (g - 15.63).abs() > 0.50,
            "Team 段舊頂替口徑 pitch 15.63pt（1.3620）必須死透，實得 {g:.2}pt"
        );
    }
    let flat = all_lines_with_y(&eng);
    let avail_idx = flat
        .iter()
        .position(|(_, t)| t.starts_with("Available at an attractive"))
        .expect("Available 段起始行必須存在");
    let avail_gap = flat[avail_idx + 1].0 - flat[avail_idx].0;
    assert!(
        (avail_gap - 11.60).abs() <= 0.15,
        "Available 段（10pt 直接宣告 w:line=240）行距 {avail_gap:.2}pt 偏離定錨真 face 真值 11.60±0.15pt（若為 13.60pt 則是 1.3620 頂替舊口徑）"
    );
}

/// 燈 2：**入表鎖**。我方逐行斷點對定錨真值**逐行全等 9/9**（卡 E 翻案後 tightening）。
/// 尺退 1.0（`K7_ADVANCE_SCALE_BUGGY=1`）時退回 0/9 ⇒ 本燈必紅，鑑別力在此。
///
/// 舊門檻是 `≥6`（卡 A/B 時期實測 6/9）。卡 C 的標點覆寫臂落地後真值已是 9/9，
/// `≥6` 變成「退回 3 行也不紅」的假防護——卡 C 記帳寫「燈 2 實跑仍 6/9（尺若走樣
/// 該燈先紅）」，其實走樣到 6/9 時該燈仍然綠。卡 E（t_0e390551）把門檻釘死為 9/9，
/// 並將穩定群由行 3..8 擴為全段，防日後靜默退回。
#[test]
fn lamp2_team_line_breaks_match_anchored_lo() {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + GRID_SPAN;
    let eng = Engine::from_docx_bytes(&std::fs::read(&full).expect("讀 corpus")).expect("open");
    let (_, _, seg) = my_team_segment(&eng);
    let matched = seg
        .iter()
        .zip(TRUTH_TEAM_LINES)
        .filter(|(mine, want)| norm(mine) == norm(want))
        .count();
    assert_eq!(
        matched,
        TRUTH_TEAM_LINES.len(),
        "cell-grid-span Team 段逐行斷點須與定錨 LO 真值**逐行全等 9/9**\
         （卡 E t_0e390551 實測：水平尺 0.9744 ＋ 卡 C 標點覆寫臂；`helped` 屬行 0，\
         行 0/1/2 不得再位移一格。未入表＝尺退 1.0 時匹配 0/9、拔 Trebuchet 標點臂\
         退回 6/9，兩者本燈皆紅）。實得 {matched}/{}：{:#?}",
        TRUTH_TEAM_LINES.len(),
        seg
    );
    // 全段行 0..8 是入表要鎖死的穩定群（卡 E 起）：尺或標點臂一走樣（含後續卡未重掃）先在這裡紅。
    for idx in 0..TRUTH_TEAM_LINES.len() {
        assert_eq!(
            norm(&seg[idx]),
            norm(TRUTH_TEAM_LINES[idx]),
            "Team 段第 {idx} 行斷點偏離定錨 LO 真值——水平尺走樣、被覆寫，或標點覆寫臂\
             （卡 C）失效。我方={:?} 真值={:?}",
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
    let (_, _, seg) = my_team_segment(&eng);
    let mine = norm(&seg.join(" "));
    let want = norm(&TRUTH_TEAM_LINES.join(" "));
    assert_eq!(
        mine, want,
        "Team 段整段文字（折疊空白後）須與定錨 LO 真值逐字元相同——\
         斷行可以不同，但不得丟字、不得多字"
    );
}
