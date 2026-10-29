//! K7-cal 紅燈：水平字形 advance 寬表（face_advance_scale 查表落地鎖）。
//!
//! 口徑出自 `docs/k7-wrapwidth-diag.md` §3／§6 ＋ `docs/k7-advance-matrix.csv`
//! （run73 重產）：內嵌 Noto 子集 vs LO 實際渲染 face 的字形 advance 寬差
//! 是 fdo68607 換行殘差（491 行 vs LO 457）的唯一根因；K6 real-LH 的水平同構
//! ＝ `face_advance_scale` 查表。本檔鎖**查表轴的行為與守門**：
//!
//! 1. fdo68607 右欄總行 315±5（LO 實測 316；我方查表態 311）——advance 軸
//!    收斂的頁內鐵證（頁數 8 由 k7_wrap_width 嚴鎖，此處不重複）。
//! 2. Bug51170 全域正文行數 185±5（LO 185，同口徑 y-bucket 去重）。逐頁
//!    [29,32,29,32,29,27,13]±1 **暫不可達**：逐頁多吸 1–4 行的根因＝LO 正文
//!    底部預留頁尾帶（ph−bottom−footer 高；docx 手術探針 V0/V1/V3 鐵證：刪
//!    頁尾內容不改變預留＝非內容驅動），屬容量軸另卡（報告 §7），非水平
//!    advance 軸——本檔只鎖總量（水平尺退 1.0 時 p6 吸乾退化會咬）。
//! 3. 守門：CJK cluster 不乘尺（em 方塊鐵律，unit 級零位移）；零宣告／未命中
//!    族檔（sdt_vml_textbox／RA001／tdf127814）頁數逐位不動。
//! 4. BUGGY 雙態鎖：`K7_ADVANCE_SCALE_BUGGY=1` **只**回塞水平查表（幾何軸
//!    不動）→ 頁數燈必紅（回 9 頁）＋格線幾何燈保持綠（機制分離，與
//!    `K7_WRAP_WIDTH_BUGGY` 全回塞態互補）。測試進程內不設 env（避免平行
//!    污染），由外部執行
//!    `K7_ADVANCE_SCALE_BUGGY=1 cargo test -p quire-engine --test k7_advance_scale`
//!    驗證（實測見提交說明）。
//!
//! Arial 判別（run73，報告 §6.2）：A 軌 0.9950 被大寫樣本串（1.0656）抬高，
//! B 軌真實行 median 0.9627（n=188）Δ3.2%＞容差 2.1% → 表值以 B 軌定案。

use quire_engine::Engine;
use quire_layout::TextMeasurer;
use quire_render_trait::DrawItem;

fn load_corpus(name: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + name;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

/// 每頁 (y-bucket, 欄) 行集合：y 以 0.5pt 去重；`col`＝x>160.5（fdo68607
/// 右欄邊界，LO 右欄文字 x0=161.4／左欄 x≤156）。回傳逐頁 (右欄行, 左欄行)。
fn column_line_counts(eng: &Engine) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        let mut keys: std::collections::BTreeSet<(i64, bool)> = std::collections::BTreeSet::new();
        for it in &dl.items {
            if let DrawItem::Text { x, y, text, .. } = it {
                if !text.trim().is_empty() {
                    keys.insert(((*y * 2.0).round() as i64, *x > 160.5));
                }
            }
        }
        let right = keys.iter().filter(|(_, c)| *c).count();
        let left = keys.len() - right;
        out.push((right, left));
    }
    out
}

/// 一頁 uniq-y 行 y 列表（0.05pt 去重）。
fn page_uniq_ys(eng: &Engine, page: usize) -> Vec<f32> {
    let dl = eng.page_display_list(page).expect("頁存在");
    let mut ys: Vec<f32> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { y, text, .. } if !text.trim().is_empty() => Some(*y),
            _ => None,
        })
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    ys
}

/// 紅燈 1：fdo68607 右欄總行 315±5（LO y-bucket 口徑實測 316；我方查表態
/// 311——run73 逐頁 [40,42,42,42,42,42,42,19] 實測）。退 scale=1.0（BUGGY）
/// 右欄回 349 行（內嵌 Noto 偏寬 14.3% 早換行）——此燈必紅。
#[test]
fn fdo68607_right_column_total_lines_match_lo() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    let counts = column_line_counts(&eng);
    let right_total: usize = counts.iter().map(|(r, _)| r).sum();
    assert!(
        (310..=320).contains(&right_total),
        "右欄總行應 315±5（LO 316；BUGGY 退 1.0 回 349）——實得 {right_total}（逐頁 {counts:?}）"
    );
}

/// 紅燈 2：Bug51170 全域正文行數（y<780 排除頁尾區，逐頁 uniq-y）185±5
/// ＝LO 185（我方查表態 183）。**本燈非 BUGGY 回塞鎖**：本檔 Arial 0.9627
/// vs 1.0 的 advance 效應在 Bug51170 只有 ≈+3 行（回塞態 186 仍落帶內），
/// 抓不到——水平尺回塞的必紅鐵證在紅燈 1（fdo 右欄 311→349）＋
/// k7_wrap_width 頁數燈。本燈鎖「總量不漂」：advance/容量任一軸把總行數
/// 推出 LO±5 時先響（逐頁 [29,32,29,32,29,27,13]±1 暫不可達——逐頁多吸
/// 1–4 行根因＝LO 正文底界止於頁尾帶（行 top≤ph−bottom−footer_dist，
/// V0/V1/V3 docx 手術探針鐵證：刪頁尾內容不停止預留），屬容量軸另卡，
/// 報告 §7——非水平 advance 軸可達）。
#[test]
fn bug51170_body_lines_match_lo_total() {
    let eng = load_corpus("poi/test-data_document_Bug51170.docx");
    assert_eq!(eng.page_count(), 7, "頁數 7＝LO（K7b 同款前提）");
    let mut total = 0usize;
    for p in 0..eng.page_count() {
        total += page_uniq_ys(&eng, p)
            .into_iter()
            .filter(|&y| y < 780.0)
            .count();
    }
    assert!(
        (180..=190).contains(&total),
        "全域正文行應落 LO 185±5（我方現態 183；BUGGY 回塞態 186 亦落帶內＝\
         本燈非回塞鎖，見檔頭註明）——實得 {total}"
    );
    // 尾頁 p6 不得吸乾（容量軸退化直接反映在此）。
    let tail = page_uniq_ys(&eng, 6)
        .into_iter()
        .filter(|&y| y < 780.0)
        .count();
    assert!(tail >= 2, "p6 尾頁須有正文（修前吸乾＝0），實得 {tail}");
}

/// 紅燈 3（unit 守門）：CJK cluster 不乘尺——同一段純 CJK 文字，
/// `with_advance_scale(0.5)` 與 `new`（scale=1.0）行寬逐位元組全等（em 方塊
/// 鐵律：LO 替代 face 同為方塊，乘尺會縮出裂縫）。Latin 側同燈反向驗證：
/// scale=0.5 必須精確半寬（查表生效路徑）。
#[test]
fn cjk_clusters_are_scale_immutable() {
    let cjk = "台灣設計規範排版引擎校準字形寬度";
    let plain = TextMeasurer::new(14.0);
    let scaled = TextMeasurer::with_advance_scale(14.0, 0.5);
    assert_eq!(
        plain.text_width(cjk),
        scaled.text_width(cjk),
        "純 CJK 行寬必須對 advance 尺免疫（em 方塊不乘尺）"
    );
    let latin = "Associaao de direito privado";
    let half = scaled.text_width(latin);
    let full = plain.text_width(latin);
    assert!(
        full > 0.0 && (half - full * 0.5).abs() < 0.01,
        "Latin 行寬須與 scale 精確線性（查表生效路徑）：full={full} half={half}"
    );
    // 混排：CJK 部分不動＋Latin 部分乘尺 ⇒ 混排行寬縮幅介於 0 與全縮之間。
    let mixed = "關連 associao association";
    let m_full = plain.text_width(mixed);
    let m_half = scaled.text_width(mixed);
    assert!(
        m_half < m_full && m_half > m_full * 0.5,
        "混排行寬縮幅须介於全免與全縮之間：full={m_full} half={m_half}"
    );
}

/// 紅燈 4（零擾動守門臂）：無 footer 幾何無關檔＋查表未命中/零宣告代表檔
/// （sdt_vml_textbox 自製 1 頁／RA001 7 頁／tdf127814 2 頁——main tip db68a9e
/// 實測基線）頁數逐位不動。水平尺只作用在「宣告族命中查表」路徑，這些檔
/// 全走 1.0 後備＝零擾動；動表值把這些頁數翻走時此燈先響。
#[test]
fn zero_decl_and_miss_files_page_counts_untouched() {
    const LOCKS: [(&str, usize, &str); 3] = [
        ("selfgen/sdt_vml_textbox.docx", 1, "零宣告自製檔"),
        (
            "xopt/TestFiles_RA001-Tracked-Revisions-01.docx",
            7,
            "修訂檔（Arial 命中但表格為主）",
        ),
        ("ooxmlexport/tdf127814.docx", 2, "M1 帶尺錨檔"),
    ];
    for (name, baseline, arm) in LOCKS {
        let eng = load_corpus(name);
        let pages = eng.page_count();
        assert_eq!(
            pages, baseline,
            "{name}（{arm}）頁數 {pages}≠基線 {baseline}——advance 查表零擾動臂被破壞"
        );
    }
}
