//! K23 紅燈：表格 cell 可用排版寬必須扣 cell 內距，且內距級聯補齊「樣式級」。
//!
//! # 卡面假說 vs 實測根因（worker 實錘，PM 假說被否證）
//! 卡面假說「斷行器給第二欄的行寬＝欄全寬 425pt、未扣 `w:tcMar`／
//! `w:tblCellMar`」**不成立**：`layout_table` 的 `resolve_margins` 從 K4 起就在
//! 扣內距（`inner_w = w − left − right`），fdo68607 兩側 108 dxa 生效＝行寬
//! 414.2pt，全 8 頁右欄 316 條切片按 K7-cal 0.8834 尺推估最右 576.0pt ＜
//! 格右界 581.4pt、越界 0 條（與 `docs/k7-wrapwidth-diag.md` §1「缺陷 B：
//! cellMargins 漏扣假說＝**否證**」同口徑；Chrome 實渲我方 SVG 亦 0 條越界）。
//!
//! PM 看到的 ~626pt 是**渲染端假影**：格內文字發射端恆 `family: None`
//! （原始碼註解「格內選族待後續卡」）⇒ SVG 不寫 `font-family` ⇒ 渲染端選
//! 自身 wide fallback face，而斷行卻按 Calibri 0.8834 尺收斂——兩把尺不同
//! face，同一行被拉寬 13–16%、右緣最遠 665.6pt 越線（把 SVG 文字強制換成
//! wide face 重測即復現卡面描述的視覺）。
//!
//! # 同區域查出的兩個真缺陷（本檔鎖其修復）
//! - **A（視覺爆欄真因）**：格內文字「單樣式」與「多樣式」兩條發射路徑都不
//!   帶 `family`。修：`resolve_cell_line_style` 回傳族、兩路徑接上。
//!   燈 1b／1c 鎖 fdo68607 p0／p5 右欄文字項全帶 `font-family="Calibri"`。
//! - **B（內距級聯漏一樣式級）**：`w:tblCellMar` 只查 `w:tblPr` 直宣告＋ECMA
//!   預設，**表格樣式級從未進級聯**。全 corpus 152 有表檔掃出 3 檔受害
//!   （NumberedList／tdf103976 樣式宣告 86 dxa＝4.3pt、tdf115883 宣告 0 dxa），
//!   LO 25.8 真值內距 4.3pt／0pt，我方錯排預設 5.4pt（tdf115883 文字 x0
//!   41.4 → 修後 36.0，LO 36.2）。修：`table_style_cell_margins`（沿 basedOn、
//!   子贏父）＋ `TableInput::style_cell_margins`。
//!   級聯位階（ECMA-376 §17.4.40／§17.4.49＋LO 實測）：直接 `w:tcMar` ＞
//!   `w:tblPr/w:tblCellMar` ＞ **樣式級** ＞ ECMA 預設（左右 108 dxa＝5.4pt、
//!   上下 0）。燈 2／燈 3／燈 4／燈 4b 為合成檔逐值鎖。
//!
//! # 量測口徑
//! 推估尺一律取「發射項自身宣告 face」的查表值（`face_advance_scale` 同表：
//! Calibri／Carlito＝0.8834、未宣告＝1.0）——即斷行器實際用的那把尺。合成
//! fixture 的 run 只在 `docDefaults` 宣告 Calibri、run 級無 `w:rFonts`，故
//! 我方選族得 `None`、斷行按 1.0 尺（K7「零宣告＝1.0 零擾動」鐵律），測試
//! 亦按 1.0 尺推估，兩邊同尺才不會出現假紅。
//!
//! # 回塞鎖
//! - `K23_CELL_FAMILY_BUGGY=1` ⇒ 格內文字發射退回「恆不宣告族」舊態（燈 1b／
//!   燈 1c 必紅）。
//! - `K23_STYLE_CELL_MAR_BUGGY=1` ⇒ 樣式級整級不參與級聯（回退舊三級；燈 3／
//!   燈 4／燈 4b 必紅）。
//!
//! 測試進程內不設 env（避免平行污染），由外部執行：
//!
//! ```sh
//! K23_CELL_FAMILY_BUGGY=1 K23_STYLE_CELL_MAR_BUGGY=1 \
//!   cargo test -p quire-engine --test k23_cell_text_width
//! ```
//!
//! （實測見提交說明與 `docs/k23-cell-margin-report.md`）。

use quire_engine::Engine;
use quire_layout::{break_lines, TextMeasurer};
use quire_render_trait::DrawItem;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 corpus docx 失敗")
}

fn load_fixture(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 fixture docx 失敗")
}

const FDO68607: &str = "ooxmlexport/fdo68607.docx";
/// fdo68607 詞彙表第二欄（Definition）格右界：LO 25.8 實測格線中心 580.9pt
/// （`k7_wrap_width` 同口徑鎖 ±1.0pt）、我方幾何 581.4pt。
const RIGHT_CELL_BOUND: f32 = 581.4;
/// fdo68607 右欄判別界（`k7_advance_scale` 同口徑）：LO 右欄文字 x0=161.4、
/// 左欄 x≤156 ⇒ 取 160.5。
const RIGHT_COL_X0: f32 = 160.5;
/// Calibri → Carlito／真 Calibri 水平 advance 尺（`face_advance_scale` 查表值）。
const CAL_SCALE: f32 = 0.8834;
/// ECMA-376 預設 cell 左/右內距：108 dxa＝5.4pt。
const ECMA_SIDE_MARGIN: f32 = 5.4;
/// 樣式級宣告 86 dxa＝4.3pt（NumberedList／tdf103976 的 LO 實測內距）。
const STYLE_MAR_86: f32 = 4.3;

/// 發射項宣告 face 的水平尺（與斷行器 `para_advance_scale` 同表同口徑）。
fn advance_scale_of(family: Option<&str>) -> f32 {
    match family.map(|f| f.trim().to_ascii_lowercase()) {
        Some(ref f) if f == "calibri" || f == "carlito" => CAL_SCALE,
        _ => 1.0,
    }
}

/// E1-C 卡 C：發射項宣告 face 的**全形標點覆寫臂**（與斷行器 `para_punct_arm`
/// 同表同口徑）。本檔推估尺的宗旨是「斷行器實際用的那把尺」（見檔頭《量測
/// 口徑》），卡 C 落地後斷行器多帶一條標點臂 ⇒ 估測器不同步吃臂就會把
/// 「窄化後才塞得進去的行」用舊寬度量成越界（fdo68607 p1/p5 兩條含 ‘ 的行
/// 實測即如此，屬估測器滯後，非引擎回退：渲染端拿 `font-family="Calibri"`
/// 渲的 ’ 本來就是真 Calibri 的 0.2495 em）。
/// 回塞鎖同引擎（`K7_ADVANCE_SCALE_BUGGY=1`／`K7_WRAP_WIDTH_BUGGY=1` ⇒ 臂退 0）。
fn punct_arm_of(family: Option<&str>) -> u8 {
    let buggy = std::env::var_os("K7_ADVANCE_SCALE_BUGGY").is_some()
        || std::env::var_os("K7_WRAP_WIDTH_BUGGY").is_some();
    if buggy {
        return quire_layout::PUNCT_ARM_NONE;
    }
    family
        .map(quire_layout::punct_arm_for_face)
        .unwrap_or(quire_layout::PUNCT_ARM_NONE)
}

/// 一條文字項在「其宣告 face 尺」下的推估右緣。
fn est_right(x: f32, text: &str, size_pt: f32, family: Option<&str>) -> f32 {
    x + TextMeasurer::with_scale_and_punct(size_pt, advance_scale_of(family), punct_arm_of(family))
        .text_width(text)
}

fn meas(size_pt: f32, family: Option<&str>) -> TextMeasurer {
    TextMeasurer::with_scale_and_punct(size_pt, advance_scale_of(family), punct_arm_of(family))
}

/// 一頁中 x>`x_min` 且非空的所有文字項，依 (y, x) 排序回傳
/// `(x, y, text, size_pt, family)`。
fn texts_right_of(
    eng: &Engine,
    page: usize,
    x_min: f32,
    skip: &[&str],
) -> Vec<(f32, f32, String, f32, Option<String>)> {
    let dl = eng
        .page_display_list(page)
        .unwrap_or_else(|| panic!("第 {page} 頁應存在"));
    let mut v: Vec<(f32, f32, String, f32, Option<String>)> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                text,
                size_pt,
                family,
                ..
            } if *x > x_min && !text.trim().is_empty() && !skip.contains(&text.trim()) => {
                Some((*x, *y, text.clone(), *size_pt, family.clone()))
            }
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap()
            .then_with(|| a.0.partial_cmp(&b.0).unwrap())
    });
    v
}

/// fdo68607 右欄文字項。
fn right_col(eng: &Engine, page: usize) -> Vec<(f32, String, f32, Option<String>)> {
    texts_right_of(eng, page, RIGHT_COL_X0, &[])
        .into_iter()
        .map(|(x, _, t, s, f)| (x, t, s, f))
        .collect()
}

/// 合成 fixture 第二欄（被測欄，x>170）文字行。
fn cell_lines(eng: &Engine, page: usize) -> Vec<(f32, String, f32, Option<String>)> {
    texts_right_of(eng, page, 170.0, &["Alpha"])
        .into_iter()
        .map(|(x, _, t, s, f)| (x, t, s, f))
        .collect()
}

/// 該欄「最寬發射行」的實算寬——行寬判別的唯一可靠觀測量：只要它落在
/// `(buggy 行寬, fixed 行寬]` 窗內，buggy 態就必然多斷一行。
fn widest_line(lines: &[(f32, String, f32, Option<String>)]) -> f32 {
    lines
        .iter()
        .map(|(_, t, s, f)| meas(*s, f.as_deref()).text_width(t))
        .fold(0f32, f32::max)
}

// ================================================================ 燈 1
/// 燈 1a（真檔端到端鎖）：fdo68607 **全 8 頁**第二欄（Definition）每一條文字
/// 切片，按其宣告 face 的水平尺推估右緣，必 ≤ 格右界 581.4pt。
///
/// 這是「斷行寬＝可用排版寬（欄寬扣左右內距）」的可觀測鎖：行寬若誤用欄
/// 全寬 425pt（卡面假說），最長切片右緣會到 ~630pt；正確 414.2pt 時全部收斂
/// 在 581.4pt 內（實測全域最右 576.0pt）。
#[test]
fn k23_lamp1a_fdo68607_right_column_inside_cell_bound() {
    let eng = load_corpus(FDO68607);
    assert_eq!(
        eng.page_count(),
        8,
        "fdo68607 頁數必須維持 8 頁（本卡純排版層，不動分頁容量）"
    );

    let mut total = 0usize;
    let mut worst = 0f32;
    let mut viol: Vec<String> = Vec::new();
    for p in 0..eng.page_count() {
        for (x, text, size, fam) in right_col(&eng, p) {
            total += 1;
            let r = est_right(x, &text, size, fam.as_deref());
            worst = worst.max(r);
            if r > RIGHT_CELL_BOUND + 0.5 {
                viol.push(format!(
                    "p{p} x={x:.1} 右緣={r:.1} face={fam:?} {:?}",
                    &text[..text.len().min(44)]
                ));
            }
        }
    }
    assert!(
        total > 200,
        "第二欄切片數應與 LO 同量級（LO 實測 316），實得 {total}——判別界可能失效"
    );
    assert!(
        viol.is_empty(),
        "fdo68607 第二欄有 {} 條文字越過格右界 {RIGHT_CELL_BOUND}pt（全域最右 {worst:.1}pt）：\n{}",
        viol.len(),
        viol.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// 燈 1b（宣告尺一致性鎖，修 A）：fdo68607 p0 第二欄**全部**文字項必須宣告
/// `font-family="Calibri"`。斷行按 Calibri 尺收斂、發射卻不宣告 face 時，
/// 渲染端落 wide fallback face 會把同一行拉寬 13–16%（實測最右 665.6pt），
/// 正是卡面所見「表格右緣文字爆出去、貼到頁緣」。
#[test]
fn k23_lamp1b_fdo68607_cell_text_declares_family() {
    let eng = load_corpus(FDO68607);
    let items = right_col(&eng, 0);
    assert!(!items.is_empty(), "p0 第二欄應有文字項");

    let mut missing = 0usize;
    let mut other: Vec<String> = Vec::new();
    for (_, text, _, fam) in &items {
        match fam.as_deref() {
            None => missing += 1,
            Some("Calibri") => {}
            Some(f) => other.push(format!("{f:?} {:?}", &text[..text.len().min(28)])),
        }
    }
    assert_eq!(
        missing,
        0,
        "p0 第二欄有 {missing}/{} 條文字項未宣告 font-family（舊缺陷：格內選族恆 \
         None ⇒ 渲染端自行選 face，與斷行尺不同 face ⇒ 視覺爆欄）",
        items.len()
    );
    assert!(
        other.is_empty(),
        "p0 第二欄文字族應全為 Calibri（文件宣告 face），實得：{}",
        other
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    );
}

/// 燈 1c（p5 鎖）：K21 軟 br 已修該頁缺字，爆欄面必須同時不在——p5 第二欄
/// 逐條推估右緣 ≤ 581.4pt，且每條都帶 face 宣告。
#[test]
fn k23_lamp1c_fdo68607_p5_no_overrun() {
    let eng = load_corpus(FDO68607);
    let items = right_col(&eng, 5);
    assert!(!items.is_empty(), "p5 第二欄應有文字項");
    for (x, text, size, fam) in &items {
        let r = est_right(*x, text, *size, fam.as_deref());
        assert!(
            r <= RIGHT_CELL_BOUND + 0.5,
            "p5 第二欄切片越界：x={x:.1} 推估右緣={r:.1} > {RIGHT_CELL_BOUND}，\
             face={fam:?} 文字={:?}",
            &text[..text.len().min(48)]
        );
        assert!(
            fam.is_some(),
            "p5 第二欄切片未宣告 face（渲染端換 face 即視覺爆欄）：{:?}",
            &text[..text.len().min(48)]
        );
    }
}

// ================================================================ 燈 2
/// 燈 2（卡面燈 2 原口徑：ECMA 預設值由 `w:tblPr` 直宣告）：欄寬 4000 dxa＝
/// 200pt、`w:tblPr/w:tblCellMar` 左右各 108 dxa（預設值本身）⇒ 斷行行寬必＝
/// 200 − 5.4 − 5.4 ＝ **189.2pt**；回塞成欄全寬 200pt 時行數必變（少一行）。
///
/// 樣式級在本 fixture 不宣告內距 ⇒ 新增級聯對本檔零擾動（buggy/fixed 同值），
/// 判別軸纯粹是「有沒有扣左右內距」。
#[test]
fn k23_lamp2_ecma_default_margin_is_deducted() {
    let tok = "Compartmentalised floodlights set up so as to allow colour mixing between \
               the lights and to prevent spill on to the backdrop during a stage rehearsal \
               with careful rigging ";
    let m = TextMeasurer::with_advance_scale(12.0, 1.0);
    let at_inner = break_lines(tok, 200.0 - 2.0 * ECMA_SIDE_MARGIN, &m).len();
    let at_full = break_lines(tok, 200.0, &m).len();
    assert!(
        at_inner > at_full,
        "判別 token 必須「扣內距 189.2pt 比欄全寬 200pt 多斷一行」（回塞鎖地基）：\
         189.2→{at_inner} 行／200→{at_full} 行"
    );

    let eng = load_fixture("k23_cellmar_108.docx");
    let lines = cell_lines(&eng, 0);
    assert_eq!(
        lines.len(),
        at_inner,
        "k23_cellmar_108 第二欄行數必須＝行寬 189.2pt 下的 {at_inner} 行\
         （誤用欄全寬 200pt 只會得 {at_full} 行＝未扣內距）"
    );
    // 最寬發射行必須「放得進 189.2」且「離 200 的邊界夠遠」＝判別窗實錘。
    let widest = widest_line(&lines);
    assert!(
        widest <= 200.0 - 2.0 * ECMA_SIDE_MARGIN + 0.01,
        "最寬發射行 {widest:.2}pt 必須 ≤ 189.2pt（行寬上界），否則行寬未扣內距"
    );
    // 左內距生效：文字起點＝第二欄左界 172.0（版心 72＋首欄 100）＋ 5.4。
    assert!(
        (lines[0].0 - (172.0 + ECMA_SIDE_MARGIN)).abs() < 0.6,
        "108 dxa 左內距必須生效：x0 應 ≈177.4，實得 {:.1}",
        lines[0].0
    );
}

// ================================================================ 燈 3
/// 燈 3（直接 `w:tcMar` 覆寫鎖）：cell 自帶 `w:tcMar/w:left=1080` dxa（54pt）
/// 時必以 **cell** 為準，壓過樣式級宣告的 0 dxa。
///
/// 本 fixture：樣式級左右宣告 0 dxa、cell 只宣告 left=1080（right 不宣告）⇒
///   fixed：left＝54（cell 贏）、right＝0（樣式級補——有效宣告 0 ≠ 缺宣告）
///          → 行寬 200 − 54 − 0 ＝ **146.0**
///   buggy（樣式級缺席）：right 落 ECMA 預設 5.4 → 行寬 **140.6**
/// 判別 token 最寬行 144.10pt 落在 (140.6, 146.0] ⇒ buggy 多斷一行（9 vs 8）。
#[test]
fn k23_lamp3_direct_tcmar_wins() {
    let eng = load_fixture("k23_tcmar_override.docx");
    let lines = cell_lines(&eng, 0);
    assert!(!lines.is_empty(), "第二欄應有文字行");

    // (a) cell 級左內距贏樣式級：x0 ＝ 欄左界 172.0 ＋ 54.0。
    assert!(
        (lines[0].0 - (172.0 + 54.0)).abs() < 0.6,
        "cell 自帶 tcMar left=1080dxa 必須壓過樣式級 0dxa：x0 應 ≈226.0，實得 {:.1}",
        lines[0].0
    );

    // (b) 右側由樣式級補 0 ⇒ 行寬 146.0；buggy 落預設 5.4 ⇒ 140.6。
    let m = TextMeasurer::with_advance_scale(12.0, 1.0);
    let tok = "Compartmentalised floodlights set up so as to allow colour mixing between \
               the lights and to prevent spill on to the backdrop during a stage rehearsal \
               with careful rigging of the scenery and ";
    let at_style = break_lines(tok, 200.0 - 54.0 - 0.0, &m).len();
    let at_default = break_lines(tok, 200.0 - 54.0 - ECMA_SIDE_MARGIN, &m).len();
    assert!(
        at_style < at_default,
        "判別 token 需在行寬 146.0（樣式級補 0）與 140.6（預設 5.4）行數不同：\
         {at_style} vs {at_default}"
    );
    assert_eq!(
        lines.len(),
        at_style,
        "行寬必須＝200−54（cell）−0（樣式級有效宣告）＝146.0 → {at_style} 行；\
         buggy（右側落預設 5.4）會得 {at_default} 行"
    );

    // (c) 幾何鎖：cell 內文字不得越出欄右界（172＋200＝372）。
    let cell_right = 172.0 + 200.0;
    for (x, text, size, fam) in &lines {
        let r = est_right(*x, text, *size, fam.as_deref());
        assert!(
            r <= cell_right + 0.5,
            "cell 內文字越出欄右界：x={x:.1} 右緣={r:.1} > {cell_right:.1} {:?}",
            &text[..text.len().min(40)]
        );
    }
}

// ================================================================ 燈 4
/// 燈 4（樣式級內距鎖，修 B）：內距只宣告在 `w:tblStyle`（左右 86 dxa＝4.3pt）
/// 時必須進級聯。LO 25.8 ground truth：NumberedList／tdf103976 實測內距 4.3pt、
/// tdf115883 樣式宣告 0 dxa 實測 0pt——我方修前錯落 ECMA 預設 5.4pt
/// （tdf115883 文字 x0 錯排 41.4，修後 36.0 vs LO 36.2）。
///
/// 判別：行寬 fixed＝200−4.3×2＝**191.4**／buggy＝200−5.4×2＝**189.2**；
/// x0 fixed＝172＋4.3＝176.3／buggy＝177.4。token 最寬行 190.99pt 落窗內。
#[test]
fn k23_lamp4_table_style_cell_mar_participates() {
    let eng = load_fixture("k23_style_mar.docx");
    let lines = cell_lines(&eng, 0);
    assert!(!lines.is_empty(), "第二欄應有文字行");

    assert!(
        (lines[0].0 - (172.0 + STYLE_MAR_86)).abs() < 0.6,
        "樣式級 86dxa（4.3pt/側）左內距必須生效：x0 應 ≈176.3，實得 {:.1}\
         （buggy 態落 ECMA 預設會得 177.4）",
        lines[0].0
    );

    let m = TextMeasurer::with_advance_scale(12.0, 1.0);
    let tok = "Compartmentalised floodlights set up so as to allow colour mixing ";
    let at_style = break_lines(tok, 200.0 - 2.0 * STYLE_MAR_86, &m).len();
    let at_default = break_lines(tok, 200.0 - 2.0 * ECMA_SIDE_MARGIN, &m).len();
    assert!(
        at_style < at_default,
        "判別 token 需在行寬 191.4（樣式級 4.3/側）與 189.2（預設 5.4/側）行數不同：\
         {at_style} vs {at_default}"
    );
    assert_eq!(
        lines.len(),
        at_style,
        "行寬必須＝200−4.3×2＝191.4 → {at_style} 行；buggy（預設內距）會得 {at_default} 行"
    );
    let widest = widest_line(&lines);
    assert!(
        widest > 200.0 - 2.0 * ECMA_SIDE_MARGIN,
        "判別窗自我證明：最寬行 {widest:.2}pt 必須 **超過** buggy 行寬 189.2，\
         否則本燈在 buggy 態也會綠（假綠）"
    );
}

/// 燈 4b（逐側級聯＋「宣告 0 是有效宣告」鎖）：`w:tblPr/w:tblCellMar` 只直
/// 宣告 `left=0` 時，left 由直宣告贏（0 生效，非缺宣告）；right 由樣式級補
/// **0**，必須壓過 ECMA 預設 5.4。
///
/// fixed 行寬＝200−0−0＝**200.0**／buggy right 落預設 ⇒ **194.6**。
/// token 最寬行 198.90pt 落窗內（x0 兩態同為 172.0，判別靠行數）。
#[test]
fn k23_lamp4b_sidewise_cascade_and_zero_is_a_declaration() {
    let eng = load_fixture("k23_mar_per_side.docx");
    let lines = cell_lines(&eng, 0);
    assert!(!lines.is_empty(), "第二欄應有文字行");

    assert!(
        (lines[0].0 - 172.0).abs() < 0.6,
        "tblPr 直宣告 left=0 必須生效（0 是有效宣告）：x0 應 ≈172.0，實得 {:.1}",
        lines[0].0
    );

    let m = TextMeasurer::with_advance_scale(12.0, 1.0);
    let tok = "Compartmentalised floodlights set up so as to allow colour mixing between \
               the lights and to prevent spill on to the backdrop during a stage rehearsal \
               with careful rigging of the scenery and softbox diffusion ";
    let at_zero = break_lines(tok, 200.0, &m).len();
    let at_default = break_lines(tok, 200.0 - ECMA_SIDE_MARGIN, &m).len();
    assert!(
        at_zero < at_default,
        "判別 token 需在行寬 200.0（樣式級 right=0）與 194.6（預設 5.4）行數不同：\
         {at_zero} vs {at_default}"
    );
    assert_eq!(
        lines.len(),
        at_zero,
        "右內距必須＝0（樣式級有效宣告，非預設 5.4）→ 行寬 200.0、{at_zero} 行；\
         buggy 態會得 {at_default} 行"
    );
    let widest = widest_line(&lines);
    assert!(
        widest > 200.0 - ECMA_SIDE_MARGIN,
        "判別窗自我證明：最寬行 {widest:.2}pt 必須超過 buggy 行寬 194.6，否則假綠"
    );
}

// ================================================================ 燈 5
/// 燈 5（零擾動鎖）：表格未宣告任何樣式級內距時，新增級聯不得改變既有幾何
/// ——fdo68607 第二欄文字起點必須仍是 161.8pt（ECMA 預設 5.4pt/側生效），
/// 頁數 8、右欄切片數與行數零位移。防「樣式級查表誤命中／虛構宣告」。
#[test]
fn k23_lamp5_undeclared_style_margins_are_zero_perturbation() {
    let eng = load_corpus(FDO68607);
    let items = right_col(&eng, 0);
    let x0 = items.iter().map(|(x, ..)| *x).fold(f32::MAX, f32::min);
    assert!(
        (x0 - 161.8).abs() < 0.6,
        "fdo68607 第二欄文字起點必須維持 161.8（ECMA 預設內距、樣式級未宣告＝零擾動），\
         實得 {x0:.1}"
    );
    // p0 第二欄行數（y 去重）應維持 42±4（修前修後同值＝樣式級零擾動）。
    let dl = eng.page_display_list(0).unwrap();
    let mut ys: Vec<i64> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { x, y, text, .. } if *x > RIGHT_COL_X0 && !text.trim().is_empty() => {
                Some((y * 4.0).round() as i64)
            }
            _ => None,
        })
        .collect();
    ys.sort_unstable();
    ys.dedup();
    assert!(
        (40..=46).contains(&ys.len()),
        "fdo68607 p0 第二欄行數應維持 42±4（零擾動），實得 {}",
        ys.len()
    );
}

// ================================================================ 回塞鎖
/// 回塞鎖 A（發射不宣告族）：`K23_CELL_FAMILY_BUGGY=1` ⇒ 格內文字退回「恆
/// `family: None`」舊態，燈 1b／燈 1c 必須紅——證明 fdo68607 的「爆欄」視覺
/// 修復由「斷行尺與發射宣告同 face」驅動。
#[test]
fn k23_buggy_lock_cell_family() {
    if std::env::var_os("K23_CELL_FAMILY_BUGGY").is_none() {
        eprintln!("未設 K23_CELL_FAMILY_BUGGY：跳過修 A 回塞檢驗（以 env=1 跑本檔驗證必紅）。");
        return;
    }
    let eng = load_corpus(FDO68607);
    let items = right_col(&eng, 0);
    let missing = items.iter().filter(|(_, _, _, f)| f.is_none()).count();
    // 修前實測：p0 右欄僅 1 條（表格標題，走正文發射路徑）帶族，其餘全 None
    // ⇒ BUGGY 態必須回到「幾乎全未宣告」（≥90%），而非全數（正文路徑不受本鎖影響）。
    assert!(
        missing * 10 >= items.len() * 9,
        "BUGGY 態格內文字應近乎全部退回未宣告族（舊行為），實得 {missing}/{}",
        items.len()
    );
}

/// 回塞鎖 B（樣式級不參與級聯）：`K23_STYLE_CELL_MAR_BUGGY=1` ⇒ 燈 3／燈 4／
/// 燈 4b 必須退回「ECMA 預設 5.4pt/側」態（必紅），證明綠燈確由「樣式級進
/// 級聯」驅動，而非偶合常數或判別窗太寬。
#[test]
fn k23_buggy_regression_lock() {
    if std::env::var_os("K23_STYLE_CELL_MAR_BUGGY").is_none() {
        eprintln!("未設 K23_STYLE_CELL_MAR_BUGGY：跳過回塞檢驗（以 env=1 跑本檔驗證必紅）。");
        return;
    }
    let m = TextMeasurer::with_advance_scale(12.0, 1.0);

    // ---- 燈 4 態：左右都退回預設 5.4pt ----
    let eng = load_fixture("k23_style_mar.docx");
    let lines = cell_lines(&eng, 0);
    assert!(
        (lines[0].0 - (172.0 + ECMA_SIDE_MARGIN)).abs() < 0.6,
        "BUGGY 態樣式級不參與：x0 應退回 ECMA 預設 177.4，實得 {:.1}",
        lines[0].0
    );
    let tok4 = "Compartmentalised floodlights set up so as to allow colour mixing ";
    assert_eq!(
        lines.len(),
        break_lines(tok4, 200.0 - 2.0 * ECMA_SIDE_MARGIN, &m).len(),
        "BUGGY 態行寬必須退回 189.2（預設內距）"
    );

    // ---- 燈 4b 態：right 退回預設 5.4（left=0 直宣告仍生效）----
    let eng2 = load_fixture("k23_mar_per_side.docx");
    let lines2 = cell_lines(&eng2, 0);
    let tok2 = "Compartmentalised floodlights set up so as to allow colour mixing between \
                the lights and to prevent spill on to the backdrop during a stage rehearsal \
                with careful rigging of the scenery and softbox diffusion ";
    assert_eq!(
        lines2.len(),
        break_lines(tok2, 200.0 - ECMA_SIDE_MARGIN, &m).len(),
        "BUGGY 態 right 必須退回預設 5.4（行寬 194.6）"
    );

    // ---- 燈 3 態：cell 級 left=54 仍贏，right 退回預設 5.4 ----
    let eng3 = load_fixture("k23_tcmar_override.docx");
    let lines3 = cell_lines(&eng3, 0);
    assert!(
        (lines3[0].0 - (172.0 + 54.0)).abs() < 0.6,
        "BUGGY 態 cell 級 left=54 仍須生效（本鎖只關樣式級），實得 {:.1}",
        lines3[0].0
    );
    let tok3 = "Compartmentalised floodlights set up so as to allow colour mixing between \
                the lights and to prevent spill on to the backdrop during a stage rehearsal \
                with careful rigging of the scenery and ";
    assert_eq!(
        lines3.len(),
        break_lines(tok3, 200.0 - 54.0 - ECMA_SIDE_MARGIN, &m).len(),
        "BUGGY 態右內距必須退回預設 5.4（行寬 140.6）"
    );
}
