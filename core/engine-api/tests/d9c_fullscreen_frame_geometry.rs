//! D9b 欠債三（t_da0682f0）：滿版外框幾何**資料化**紅燈考卷。
//!
//! 卡面白紙黑條的四條字面量臂（`core/engine-api/src/lib.rs` 三處 `22.05` 判定、
//! `emit_txbx_texts` 的退內縮 `[11.3, 4.0, 3.0, 0.0]` 與正文寬 `540.5`）都是
//! fdo76591 專屬數值，換檔即失效。PM 定案（卡面 comment 11-07 10:00）採
//! **同源推導**：滿版框判定＝「框寬 ≥ 版心寬 **且** |框起點 −（版心原點＋VML
//! 宣告偏移）| ≤ 容差」，容差沿用既有 1.5pt，零檔案常量。
//!
//! 本檔的實測根據（探針見 `docs/d9b-inset-redlight.md` §9；LO 25.8 Linux 側，
//! `-env:UserInstallation` 專屬 profile，pymupdf 抽 span `origin`）：
//!
//! * fdo76591 `pgMar/@w:left`＝720twips＝**36.0pt**，框 `_x0000_s1026` 宣告
//!   `margin-left:-13.95pt`／`margin-top:-17.8pt`／`width:567pt`／`strokeweight="4pt"`；
//!   版心寬＝612−36−36＝**540.0pt** ⇒ 框寬 567 ≥ 540 且
//!   |22.05 − (36.0 − 13.95)|＝**0.00** ⇒ 推導式命中（舊臂 `500.0`／`22.05` 是同一件事的
//!   兩個抄下來寫死的數）。
//! * **框內文字左緣的內縮是「框相對」而非「滿版專屬」**：反事實實測（探針
//!   `cf_inset_experiment.py`，各別改一軸）——框右移 10pt ⇒ 文字右移 10.00pt、
//!   `strokeweight` 4pt→8pt ⇒ 文字右移 **4.00pt**、`pgMar.left` 36→72pt ⇒ 文字右移
//!   **36.00pt**（框起點只動 36.0，故文字**不跟著框**走，而是錨定在邊距盒）；
//!   框寬 567→400pt（跌破「滿版」）⇒ 文字左緣**一動不動**（33.35→33.35）。
//!   ⇒ LO 的口徑是「文字左緣＝框起點 ＋ 描邊寬 ＋ VML 格式預設內縮 7.3pt」，
//!   既有 `[11.3, 4.0, 3.0, 0.0]` 的四個數分別是 `4.0`（宣告的 strokeweight）＋
//!   `7.3`（LO 框內建 margin，見 §9）等合成值，不得再寫字面量。
//! * n751077 `pgMar/@w:left`＝3958twips＝**197.90pt**（卡面 §2 第 3 條的誤傷例）：
//!   該檔群組框走 `mso-position-*-relative:page|margin` ＋ `center`（頁面釘定），
//!   推導式**無宣告偏移可用** ⇒ 不命中，與舊臂判定一致（舊臂 x≈17.25≠22.05 亦不命中）。
//!
//! 鑑別力（BUGGY 回塞鎖）：`D9C_FRAME_GEOM_BUGGY=1` ⇒ 發射層把「背景覆頁框」
//! 分支整個退回（不內收描邊、文字退回框原點）⇒ 燈 2／燈 3 必紅（實跑兩種模式
//! 記進 `docs/d9b-inset-redlight.md` §9）。

use quire_engine::{DrawItem, Engine};

fn read_corpus(rel: &str) -> Vec<u8> {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取 {}: {e}", p.display()))
}

/// fdo76591 滿版背景框的既有 LO 真值（本檔唯一的三個「真值」常數來源＝LO 實測，
/// 不是我方實作的回音）。
const LO_TEXT_X: f32 = 33.35;
const LO_FRAME_BOTTOM_RULE_Y: f32 = 749.85;
/// LO 真值：4pt 框線的中心線（`rect=[22.10, 749.85, ...]`），以及我方發射的口徑。
const LO_STROKE_W: f32 = 4.0;

/// 燈 1（資料面）：VML 宣告偏移＋該頁版心盒必須可取，滿版判定才能由資料推導。
///
/// 現行缺陷：排版層只帶最終 `x_pt`/`w_pt`，源檔宣告的 `margin-left:-13.95pt`
/// 與「該頁版心寬 540.0pt」在發射層都不可見，三處臂只能對 `22.05`/`500.0`/`540.5`
/// 做字面量比對。
#[test]
fn d9c_lamp1_declared_offset_and_content_box_reach_emission() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let cb = eng.page_content_box(0).expect("首頁版心盒存在");
    let (cx, cw) = (cb.x_pt, cb.w_pt);
    assert!(
        (cx - 36.0).abs() < 0.01,
        "fdo76591 版心 x 原點應由 pgMar/@w:left(720twips) 推導為 36.0pt，實得 {cx}"
    );
    assert!(
        (cw - 540.0).abs() < 0.01,
        "版心寬應由 pgSz−左右邊距推導為 540.0pt（非字面量 540.5），實得 {cw}"
    );

    let pics = eng.page_floating_images(0).expect("首頁浮動框存在");
    let frame = pics
        .iter()
        .find(|f| (f.w_pt - 567.0).abs() < 0.01 && (f.h_pt - 733.65).abs() < 0.01)
        .expect("必須存在 567×733.65 的滿版背景框（_x0000_s1026）");
    // 欄位名照 PM 定案（11-07 10:00 第 1 條）：走 `InlineImage::pos_x_pt`／
    // `pos_y_pt` 既有語義承接（VML margin 第 2 式的宣告偏移原值），不新增平行欄位。
    let dx = frame
        .pos_x_pt
        .expect("VML style 的 margin-left 宣告必須原樣搬到排版層（否則只能寫 22.05 字面量）");
    let dy = frame
        .pos_y_pt
        .expect("VML style 的 margin-top 宣告必須原樣搬到排版層");
    assert!(
        (dx + 13.95).abs() < 0.01,
        "margin-left 應原樣為 −13.95pt（帶負號宣告值），實得 {dx}"
    );
    assert!(
        (dy + 17.8).abs() < 0.01,
        "margin-top 應原樣為 −17.8pt，實得 {dy}"
    );

    // PM 定案的判定式，在考卷端獨立重算一遍（零檔案常量）。
    let off = (frame.x_pt - (cx + dx)).abs();
    assert!(
        frame.w_pt >= cw && off <= 1.5,
        "推導式必須命中（框寬 {:.2} ≥ 版心 {:.2}，起點偏離版心原點＋宣告偏移 {off:.2}pt）",
        frame.w_pt,
        cw
    );
}

/// 燈 2（行為面）：描邊內收與文字左緣都由**框自身宣告**推導，不認檔案。
///
/// 口徑（LO 反事實實測）：
/// * 4pt 框線中心線＝框邊＋半線寬（LO 底線 749.85＝18.2＋733.65−2.0）；
/// * 框內文字左緣＝框起點 ＋ 描邊寬 ＋ VML 格式預設內縮（LO 33.35＝22.05＋4.0＋7.3）。
///
/// 兩條都必須在**我方發射的同一組值**上自洽：框線寬取自我方發射的 Rule，
/// 故「改 strokeweight 而文字內縮不動」這類硬編碼會被本燈打紅。
#[test]
fn d9c_lamp2_insets_derive_from_declared_stroke() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    // 我方發射的 4pt 框線（底邊）。
    let bottom = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Rule {
                x1,
                y1,
                y2,
                width_pt,
                ..
            } if (*width_pt - LO_STROKE_W).abs() < 0.1 && (y1 - y2).abs() < 0.01 && *y1 > 700.0 => {
                Some((*x1, *y1, *width_pt))
            }
            _ => None,
        })
        .expect("必須存在 4pt 外框底邊 Rule");
    let (rule_x, rule_y, stroke) = bottom;
    assert!(
        (rule_y - LO_FRAME_BOTTOM_RULE_Y).abs() <= 1.0,
        "框底邊 y ({rule_y:.2}) 未依描邊向內收斂至 LO 真值 {LO_FRAME_BOTTOM_RULE_Y}"
    );

    // 框內正文左緣（LO 真值 33.35）。
    let text_x = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text { x, text, .. } if text.contains("UNLESS OTHERWISE NOTIFIED") => Some(*x),
            _ => None,
        })
        .expect("必須存在框內正文「UNLESS OTHERWISE NOTIFIED」文字項");
    assert!(
        (text_x - LO_TEXT_X).abs() <= 0.05,
        "正文左緣 ({text_x:.2}) 偏離 LO 真值 {LO_TEXT_X} 超過 0.05pt"
    );

    // 自洽律（本燈的真正新鎖）：文字左緣 −（框外緣）＝ 線寬 ＋ 框內建內縮。
    // 口徑勘誤（PM 11-10 發車令點名的「半線寬」待核對項）：底邊是**水平** Rule，
    // 描邊內收臂只收它的 y（top/bottom），其 x1＝框宣告左緣原值（22.05）——
    // 再減半線寬會二次內收（run1 考卷即犯此錯：13.30≠11.30 的 2.00 差值恰是
    // 被多減的一次 half_w）。反事實坐實（probe_y_cf／cf_inset_experiment）：
    // sw 4→8pt ⇒ 文字 +4.00pt＝**全線寬增量原值**（非半值）⇒ 線寬項係數 1。
    // 兩項都取自我方發射值 ⇒ 只要 stroke 改變而內縮仍掛字面量，本斷言即紅。
    let frame_left = rule_x;
    let gap = text_x - frame_left;
    let expected = stroke + 7.3;
    assert!(
        (gap - expected).abs() <= 0.05,
        "文字左緣與框外緣的間距 ({gap:.2}) 應＝宣告線寬＋框內建內縮＝{expected:.2}；\
         差值 {:.2} ⇒ 內縮仍掛字面量而非由框宣告推導",
        (gap - expected).abs()
    );
}

/// 燈 3（誤傷鎖）：大邊距 section（n751077，`pgMar.left`＝197.90pt）不得被同條
/// 判定命中——卡面 §2 第 3 條點名的誤傷場景。
///
/// 該檔群組框是**頁面釘定 center**（`mso-position-*-relative` ＋ `center`），
/// 推導式沒有「宣告偏移」可用 ⇒ 必須不命中；舊字面量臂對它也同樣不命中
/// （x≈17.25 ≠ 22.05），兩條判定必須一致，否則本卡就是把 fdo76591 的修法
/// 換成對另一檔的回退。
#[test]
fn d9c_lamp3_wide_margin_section_not_hit() {
    let bytes = read_corpus("ooxmlimport/n751077.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 n751077 失敗");
    let cb = eng.page_content_box(0).expect("首頁版心盒存在");
    let (cx, cw) = (cb.x_pt, cb.w_pt);
    assert!(
        (cx - 197.90).abs() < 0.05,
        "本燈的前提是這檔邊距劇變：pgMar/@w:left 應為 197.90pt（3958twips），實得 {cx}"
    );
    assert!(
        cw < 450.0,
        "大邊距 section 的版心寬應遠小於 fdo76591 的 540.0pt，實得 {cw}"
    );

    let pics = eng.page_floating_images(0).expect("首頁浮動框存在");
    assert!(!pics.is_empty(), "n751077 首頁應有浮動框（群組）");
    let ch = cb.h_pt;
    for f in pics {
        // 推導式＝實作同源：寬與高雙軸 ≥ 版心 ＋ 兩軸起點一致（容差 1.5pt）。
        let derived_hit = match (f.pos_x_pt, f.pos_y_pt) {
            (Some(dx), Some(dy)) => {
                f.w_pt >= cw
                    && f.h_pt >= ch
                    && (f.x_pt - (cx + dx)).abs() <= 1.5
                    && (f.y_pt - (cb.y_pt + dy)).abs() <= 1.5
            }
            _ => false,
        };
        let legacy_hit = f.w_pt >= 500.0 && (f.x_pt - 22.05).abs() <= 1.5;
        assert!(
            !legacy_hit,
            "前提：舊字面量臂對本檔框不命中（x={:.2}, w={:.2}）",
            f.x_pt,
            f.w_pt
        );
        assert!(
            derived_hit == legacy_hit,
            "推導式對 n751077 必須與舊臂同判定（命中即誤傷回退）：\
             框 x={:.2} w={:.2} 宣告偏移={:?}/{:?} → derived={derived_hit} legacy={legacy_hit}",
            f.x_pt,
            f.w_pt,
            f.pos_x_pt,
            f.pos_y_pt
        );
    }
}
