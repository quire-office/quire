//! E1-C 卡 C：內嵌子集的「1.0 em 全形標點」在**拉丁上下文**裡吃方塊寬——跨檔水平
//! 度量缺陷的修復鎖（按宣告族的覆寫臂）。
//!
//! # 卡面前提逐條更正（先證再寫考卷，見 `docs/e1c-card-c-subset-punct.md`）
//!
//! 卡面：「我方內嵌子集 U+2019／U+2026 advance＝1.0 em，LO NotoSans 真值 0.175／0.791
//! ⇒ 修子集產生器／子集本身使這三個碼點的 advance 落到真值」（卡面原文逐字）。
//! **四條實測把手段與真值口徑都翻掉**：
//!
//! 1. **不是子集產生器的缺陷**：原始 `NotoSansCJKtc-Regular.otf` 與我方子集逐字全等
//!    （scratch `probe_src_advance.py`：’ … 皆 1.0000 em）。CJK 字型的標點本來就佔
//!    em 方塊——改 otf 等於改字型設計。
//! 2. **沒有單一的 LO 真值**：同一個 ’ 隨宣告族而變（scratch `probe_lo_face_punct.py`
//!    定錨態 rawdict 逐 face 實渲：Calibri 0.249／Cambria 0.249／TNR 0.331／Arial 0.222／
//!    NotoSerif 0.250／**Trebuchet 0.3672 em**）。⇒ 必須按 face 查表，與
//!    `face_advance_scale` 同臂同守門。
//! 3. **卡面的 0.175／0.791 是頂替態**：那是 LO **自帶 NotoSans** 的 hmtx
//!    （scratch `probe_punct_truth.py` 直接讀該 otf hmtx 復現該數），正是卡 B
//!    （`docs/e1cb-lo-truth-canon.md`）明文否決的口徑 ⇒ 不得入表（燈 5 鎖死）。
//! 4. **漢語上下文我方現態是對的**：LO 對漢語段的 “ ” 實渲 **1.0 em**（scratch
//!    `probe_lo_rawdict.py`：5DayTour「插入”選項卡」Δ=11.0pt@11pt＝恰 1.0 em，渲
//!    WenQuanYiZenHei）⇒ 覆寫**只能在非 CJK script run 生效**（燈 4 鎖死）。
//!
//! # 本檔鎖什麼
//!
//! - 燈 1：拉丁上下文（真檔 cell-grid-span Team 段，宣告 Trebuchet MS）的 ’ 在我方
//!   **量測層**吃 0.3672 em 而非 1.0 em——用「同行斷點是否把 ’ 容納進同一行」的
//!   幾何可觀測量鎖死，不是直接讀表（讀表在 `punct_advance.rs` 單元鎖）。
//! - 燈 2：**雙臂對齊鎖**——`punct_arm_for_face` 的分組鍵必須與 engine-api
//!   `face_advance_scale` 逐臂一致（兩表走樣 ⇒ 同一族「拉丁寬窄了、標點沒窄」＝
//!   半套尺，最難查的缺陷形態）。
//! - 燈 3：回塞鎖（鑑別力）——`K7_ADVANCE_SCALE_BUGGY=1` 時燈 1 的幾何退回舊態。
//! - 燈 4：漢語段標點**不得**被覆寫（1.0 em 方塊保持）。
//! - 燈 5：頂替態口徑（0.175／0.791）永不得入表。
//! - 燈 6：文字守恆（比照本庫防假綠雙斷言鐵律：斷行可以變，字不得增減）。
//!
//! ## 鑑別力口徑（外部執行，進程內不設 env）
//!
//! ```sh
//! K7_ADVANCE_SCALE_BUGGY=1 cargo test -p quire-engine --test e1c_subset_punct
//! ```
//!
//! 燈 1／燈 3 **必紅**（實跑結果記進 `docs/e1c-card-c-subset-punct.md`）。

use quire_engine::Engine;
use quire_layout::{punct_arm_for_face, punct_em, TextMeasurer};
use quire_render_trait::DrawItem;

const GRID_SPAN: &str = "ooxmlexport/cell-grid-span.docx";

fn engine(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 corpus docx 失敗")
}

/// 與斷行器同口徑的測量器（臂隨 family；回塞鎖同引擎）。
fn meas(size_pt: f32, family: Option<&str>) -> TextMeasurer {
    let buggy = std::env::var_os("K7_ADVANCE_SCALE_BUGGY").is_some()
        || std::env::var_os("K7_WRAP_WIDTH_BUGGY").is_some();
    let arm = if buggy {
        quire_layout::PUNCT_ARM_NONE
    } else {
        family
            .map(punct_arm_for_face)
            .unwrap_or(quire_layout::PUNCT_ARM_NONE)
    };
    // Trebuchet 的水平尺（斷行器同值；本檔只需它不影響標點斷言——標點是「取代」
    // 而非疊乘，見 shaping.rs 註解）。
    let scale = match family.map(|f| f.trim().to_lowercase()).as_deref() {
        Some("trebuchet ms") => 0.9744,
        _ => 1.0,
    };
    TextMeasurer::with_scale_and_punct(size_pt, scale, arm)
}

/// 跨頁收集含某字元的文字項（`text` 非空且包含 `ch`）。
fn items_with(eng: &Engine, ch: char) -> Vec<(String, f32, Option<String>)> {
    let mut out = Vec::new();
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("顯示列表應存在");
        for it in &dl.items {
            if let DrawItem::Text {
                text,
                size_pt,
                family,
                ..
            } = it
            {
                if text.contains(ch) {
                    out.push((text.clone(), *size_pt, family.clone()));
                }
            }
        }
    }
    out
}

// ================================================================ 燈 1
/// 燈 1（真檔端到端）：cell-grid-span Team 段宣告 `Trebuchet MS`，段內含 `iBall’s`
/// （U+2019）。我方**量測層**必須把該 ’ 按真 trebuc hmtx 0.3672 em 計寬——若仍是
/// 1.0 em，含 ’ 的整行會被高估 6.33pt（@10pt），該行斷點隨之行寬走樣。
///
/// 可觀測量：把該行原文交給「與斷行器同尺」的測量器，行寬必須 ≤ Team 段的排版
/// 可用寬；且 ’ 的單字寬實得 0.3672×size（**取代**，不疊乘水平尺）。
#[test]
fn lamp1_apostrophe_in_latin_run_uses_face_hmtx() {
    let eng = engine(GRID_SPAN);
    let hits = items_with(&eng, '\u{2019}');
    assert!(
        !hits.is_empty(),
        "cell-grid-span 須含 U+2019（iBall’s）作為本燈標的——實得 0 條，考卷標的失效"
    );
    for (text, size, fam) in &hits {
        let fam = fam.as_deref().unwrap_or("Trebuchet MS");
        // (a) 單字元：’ 必須落在真 face hmtx（0.3672 em），且死透 1.0 em 舊態。
        let one = meas(*size, Some(fam)).text_width("\u{2019}");
        let want = 0.3672 * size;
        assert!(
            (one - want).abs() <= 0.05 * size.max(1.0),
            "’ 量寬 {one:.3}pt@{size}pt 偏離真 trebuc hmtx {want:.3}pt（0.3672 em）；\
             10.0pt@10pt＝子集方塊舊態未修：{fam:?} {text:?}"
        );
        assert!(
            (one - *size).abs() > 0.5 * size,
            "’ 仍吃 1.0 em 方塊（{one:.3}pt@{size}pt）＝覆寫臂未接上：{fam:?}"
        );
        // (b) 整行：含 ’ 的行按臂度量後不得比「方塊態」更寬（覆寫是收窄不是放大）。
        let w_arm = meas(*size, Some(fam)).text_width(text);
        let w_square =
            TextMeasurer::with_scale_and_punct(*size, 0.9744, quire_layout::PUNCT_ARM_NONE)
                .text_width(text);
        assert!(
            w_arm <= w_square + 0.01,
            "含 ’ 行寬 {w_arm:.3} 反而大於方塊態 {w_square:.3}＝覆寫方向錯：{text:?}"
        );
    }
}

// ================================================================ 燈 2
/// 燈 2（雙臂對齊鎖）：標點臂的分組鍵必須與水平尺**逐臂一致**。engine-api 的
/// `face_advance_scale` 是 pub(crate) 不可跨 crate 直讀，故這裡鎖「同鍵集」的
/// 契約面：每臂至少一個鍵要解析到臂 id，且同一個 face 字串在兩套語義下不會
/// 一個有臂、一個無臂（用本檔硬編碼的 expected 鍵表做鏡像；鍵表改動時本燈先紅，
/// 強迫改表者同步兩側——比照 `tools/line-metrics` 的「同表同徑」記帳紀律）。
#[test]
fn lamp2_punct_arm_grouping_matches_advance_scale_groups() {
    // (face, 水平尺有臂?, 期望標點臂 id)
    const CASES: &[(&str, bool, u8)] = &[
        ("Calibri", true, 1),
        ("Carlito", true, 1),
        ("Cambria", true, 2),
        ("Caladea", true, 2),
        ("Times New Roman", true, 3),
        ("Times", true, 3),
        ("Liberation Serif", true, 3),
        ("Arial", true, 4),
        ("Liberation Sans", true, 4),
        ("Helvetica", true, 4),
        ("Noto Serif", true, 5),
        ("Trebuchet MS", true, 6),
        // 未宣告／CJK 族：兩軸一律零擾動。（「宋体」＝真實語料 `w:rFonts`／theme1.xml
        // 的簡體字面宣告，逐字引用＝資料，非註解用字；繁變體見 punct_advance 單元鎖。）
        ("宋体", false, 0),
        ("", false, 0),
        ("Unknown Font 42", false, 0),
    ];
    for (face, has_scale_arm, want) in CASES {
        let got = punct_arm_for_face(face);
        assert_eq!(
            got, *want,
            "`punct_arm_for_face({face:?})`＝{got} 期望 {want}——鍵集必須與 \
             face_advance_scale 分組逐臂對齊，否則出現「拉丁窄了、標點沒窄」半套尺"
        );
        assert_eq!(
            (got != 0),
            *has_scale_arm,
            "{face:?}：水平尺有臂＝{has_scale_arm} 但標點臂有無＝{} ⇒ 兩軸不同族，\
             兩表走樣（本燈存在的唯一理由）",
            got != 0
        );
    }
}

// ================================================================ 燈 3
/// 燈 3（鑑別力／回塞鎖）：`K7_ADVANCE_SCALE_BUGGY=1` 時標點臂必須**連帶退回**
/// 0（與水平尺同進退），且燈 1 的單字寬斷言在此態下必紅（用反向斷言把「回塞有效」
/// 鎖成 CI 可跑的綠燈）。
#[test]
fn lamp3_buggy_lock_reverts_both_axes_together() {
    let buggy = std::env::var_os("K7_ADVANCE_SCALE_BUGGY").is_some()
        || std::env::var_os("K7_WRAP_WIDTH_BUGGY").is_some();
    let one = meas(10.0, Some("Trebuchet MS")).text_width("\u{2019}");
    if buggy {
        assert!(
            (one - 10.0).abs() < 0.5,
            "回塞態 ’ 應退回子集方塊 10.0pt@10pt，實得 {one:.3} ⇒ 回塞鎖沒接上量測層"
        );
    } else {
        assert!(
            (one - 3.672).abs() < 0.5,
            "正常態 ’ 須為 3.672pt@10pt（真 trebuc hmtx），實得 {one:.3}"
        );
    }
    // 表層：臂 0 恆不覆寫（與 env 無關的守門）。
    assert!(punct_em(quire_layout::PUNCT_ARM_NONE, 0x2019).is_none());
}

// ================================================================ 燈 4
/// 燈 4（漢語上下文不可動）：CJK script run 內的標點**不得**被覆寫——LO 對漢語段
/// 的 “ ” 實渲 1.0 em（scratch rawdict 實測），動它＝回歸。
#[test]
fn lamp4_cjk_context_keeps_em_square() {
    // 「插入”選項卡」（源檔原貌逐字，簡體字面＝資料非註解）：” 落在 Han run 內
    // （swash script 判定），覆寫不生效。
    let s = "插入”选项卡";
    let w_arm = meas(11.0, Some("Calibri")).text_width(s);
    let w_square =
        TextMeasurer::with_scale_and_punct(11.0, 1.0, quire_layout::PUNCT_ARM_NONE).text_width(s);
    assert!(
        (w_arm - w_square).abs() < 0.01,
        "Han run 內的 ” 被覆寫了：臂態 {w_arm:.3} vs 方塊態 {w_square:.3}——\
         漢語標點 LO 實渲 1.0 em，覆寫必須限非 CJK run"
    );
    // 同 face 的純拉丁上下文則必須被覆寫（證明燈 4 不是「臂根本沒生效」的假綠）。
    let latin_arm = meas(11.0, Some("Calibri")).text_width("say\u{201d}this");
    let latin_square = TextMeasurer::with_scale_and_punct(11.0, 1.0, quire_layout::PUNCT_ARM_NONE)
        .text_width("say\u{201d}this");
    assert!(
        latin_arm < latin_square - 5.0,
        "拉丁上下文的 ” 未覆寫（{latin_arm:.3} vs {latin_square:.3}）⇒ 燈 4 的相等是假綠"
    );
}

// ================================================================ 燈 5
/// 燈 5（真值口徑鐵律）：卡面的 0.175／0.791 是 **LO 自帶 NotoSans 的 hmtx**＝卡 B
/// 已否決的頂替態，任何臂都不得出現該口徑（防止有人日後「照卡面修回去」）。
#[test]
fn lamp5_substitute_notosans_values_never_enter_the_table() {
    for arm in 1..=6u8 {
        assert_ne!(
            punct_em(arm, 0x2019),
            Some(0.175),
            "臂 {arm} 混入頂替態 NotoSans 口徑"
        );
        assert_ne!(
            punct_em(arm, 0x2026),
            Some(0.791),
            "臂 {arm} 混入頂替態 NotoSans 口徑"
        );
        // 且覆寫必須是「收窄」：不允許 >1.0 em（放大＝把方塊字撐更開＝必爆欄）。
        for cp in [0x2018u32, 0x2019, 0x201C, 0x201D, 0x2026, 0x00B7, 0x2022] {
            if let Some(em) = punct_em(arm, cp) {
                assert!(
                    em > 0.0 && em <= 1.0 + 1e-6,
                    "臂 {arm} U+{cp:04X} em={em} 超出 (0,1]"
                );
            }
        }
    }
}

// ================================================================ 燈 6
/// 燈 6（防假綠文字守恆）：卡 C 改變量寬 ⇒ 斷行會變；文字必須逐字守恆（不得因
/// 窄化而吞字或長字）。比照本庫《斷行／度量類考卷的防假綠雙斷言》鐵律。
#[test]
fn lamp6_corpus_text_is_conserved() {
    let eng = engine(GRID_SPAN);
    let mut lines: Vec<String> = Vec::new();
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("顯示列表應存在");
        let mut rows: std::collections::BTreeMap<i64, Vec<(f32, String)>> =
            std::collections::BTreeMap::new();
        for it in &dl.items {
            if let DrawItem::Text { x, y, text, .. } = it {
                if !text.trim().is_empty() {
                    rows.entry((*y * 2.0).round() as i64)
                        .or_default()
                        .push((*x, text.clone()));
                }
            }
        }
        for (_, mut runs) in rows {
            runs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            // 同 y-bucket 內**無分隔**拼接（比照 e1c_advance_trebuchet 段行聚合口徑）：
            // 被拆成多 run 的詞（"iBall"＋"’s"）拼起來才是原文。
            lines.push(runs.into_iter().map(|(_, t)| t).collect::<String>());
        }
    }
    let mine: String = lines
        .iter()
        .flat_map(|l| l.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ");
    for needle in [
        "iBall’s last major category launch",
        "Innovation….the brand",
        "iBall’s",
    ] {
        assert!(
            mine.contains(needle),
            "卡 C 落地後 Team 段文字不再守恆，找不到 {needle:?}（全篇摺疊後：{}…）",
            &mine[..mine.len().min(160)]
        );
    }
}
