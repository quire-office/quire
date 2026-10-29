//! K7c 紅燈：`table_x_off` 負偏移一致性（center 超大表／負縮排／w:w=0 誤判）。
//!
//! 三條規格返修（雙模型審查＋PM corpus 實測；corpus 400 檔命中數見各節）：
//! 1. jc=center 超大表負偏移被 `.max(0.0)` 截 0（貼左單邊右溢）——LO/Word
//!    口徑 center 必左右對稱越界；同規格與 K7 未宣告 jc 自然寬均分臂方向倒置
//!    （宣告 center 的貼左、沒宣告的反而置中）。corpus：jc=center 且無 tblW
//!    超版心＝0 檔（PM 全掃＋本卡覆核），無 LO 真值可鎖→先修內部一致性＋
//!    幾何對稱性（selfgen 自產燈）。
//! 2. `ind = indent_pt.unwrap_or(0.0).max(0.0)` 吞合法負 tblInd（Word 常見
//!    負縮排對齊正文邊界外移，corpus 命中 8 檔）；K7 臂守衛 `== 0.0` 精確
//!    比較在 1 twip（0.05pt）縮排下整臂失效、位移階躍——改容差比較。
//! 3. `<w:tblW w:w="0" w:type="dxa"/>`（歷史 auto 變體）→ `Some(Pt(0))` →
//!    is_some()=true 走壓版心心路徑又失去 K7 均分資格。corpus：tblW/tcW
//!    dxa≤0 皆 0 檔→零現案位移，width_spec 過濾 num≤0 回 None。
//!
//! 燈幾何（fixtures/k7c_table_xoff.docx，版心 451.30pt、左緣頁座標 72pt，
//! 生成腳本 scripts/gen_k7c_fixture.py 內有逐燈推導）：
//! - T1 center 無宣告寬 551.30pt 超版心：22.0／573.30（左右各越界 50.0pt）。
//! - T2 right 無宣告寬 475.30pt：48.0／523.30（右緣釘版心右界，往左溢出）。
//! - T3 宣告寬 401.30pt＋tblInd −18pt：54.0／455.30。
//! - T4 w:w=0 dxa（auto 變體）自然寬 511.30pt：42.0／553.30（均分臂）。
//! - T5 自然寬 491.30pt＋tblInd 1 twip（0.05pt）：52.0／543.30（容差仍均分）。
//!
//! 修正前（main tip db68a9e）五燈全紅：T1/T2/T4 貼左 72.0、T3 72.0、
//! T5 72.05（先紅後綠紀律：套 patch 前本檔必紅，套後必全綠）。
//! `K7_WRAP_WIDTH_BUGGY=1` 回塞態：T1/T2/T4/T5 依賴自然寬幾何→必紅；
//! T3（宣告寬＋負縮排臂）與 K7 回塞無關→恆綠（獨立性鎖：K7c 修正不被
//! K7 開關誤傷）。外部執行
//! `K7_WRAP_WIDTH_BUGGY=1 cargo test -p quire-engine --test k7c_table_xoff`。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn engine() -> Engine {
    let full = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/k7c_table_xoff.docx"
    );
    let bytes = std::fs::read(full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open fixture")
}

/// p0 垂直格線 x（去重排序，與 k7_wrap_width 紅燈同口徑）。
fn vertical_rule_xs(eng: &Engine) -> Vec<f32> {
    let dl = eng.page_display_list(0).expect("頁存在");
    let mut xs: Vec<f32> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Rule { x1, y1, x2, y2, .. }
                if (x1 - x2).abs() < 0.01 && (y1 - y2).abs() > 3.0 =>
            {
                Some(*x1)
            }
            _ => None,
        })
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    xs
}

/// 容差 1.0pt（比照 k7_wrap_width）；回塞態錯值皆 ≥10pt 級，燈必真紅。
fn has_rule(xs: &[f32], x: f32) -> bool {
    xs.iter().any(|v| (v - x).abs() < 1.0)
}

/// 全檔單頁（燈全在 p0；若分頁擴散即前提破壞，先咬紅）。
#[test]
fn k7c_fixture_single_page() {
    let eng = engine();
    assert_eq!(eng.page_count(), 1, "五張燈表都應落在第一頁");
}

/// 紅燈 T1：宣告 jc=center 的超版心表必左右對稱越界（LO/Word 口徑）。
/// 修正前 `.max(0.0)` 截 0→左框線 72.0（貼左、單邊右溢 100pt）＝紅。
#[test]
fn k7c_center_oversized_table_symmetric_overhang() {
    let xs = vertical_rule_xs(&engine());
    assert!(
        has_rule(&xs, 22.0) && has_rule(&xs, 573.3),
        "center 超寬表應左右各越界 50.0pt（框線 22.0/573.3），xs={xs:?}"
    );
}

/// 紅燈 T2：宣告 jc=right 的超版心表右緣釘版心右界、往左溢出。
/// 口徑取捨見 table.rs 註解（與 center 同「對稱/釘界」族，不再被 `.max(0.0)`
/// 鉗回貼左）。修正前框線 72.0/496.0（右界脫開 523.3）＝紅。
#[test]
fn k7c_right_oversized_table_pins_right_margin() {
    let xs = vertical_rule_xs(&engine());
    assert!(
        has_rule(&xs, 48.0) && has_rule(&xs, 523.3),
        "right 超寬表應右緣釘 523.3、左溢出至 48.0，xs={xs:?}"
    );
}

/// 紅燈 T3：合法負 tblInd（−18pt）宣告寬表——整表左移對齊正文邊界外側。
/// 修正前 `ind.max(0.0)` 吞負值→貼左 72.0＝紅（corpus 負 tblInd 命中 8 檔）。
#[test]
fn k7c_negative_indent_shifts_declared_width_table_left() {
    let xs = vertical_rule_xs(&engine());
    assert!(
        has_rule(&xs, 54.0) && has_rule(&xs, 455.3),
        "tblInd −18pt 應使 401.3pt 宣告寬表落 54.0/455.3，xs={xs:?}"
    );
}

/// 紅燈 T4：`w:w="0" w:type="dxa"`＝歷史 auto 變體，非宣告寬——
/// width_spec 過濾後走 K7 未宣告自然寬均分臂。
/// 修正前 `Some(Pt(0))`→is_some 壓版心→貼左 72.0/523.3＝紅。
#[test]
fn k7c_zero_dxa_tblw_is_auto_not_declared_width() {
    let xs = vertical_rule_xs(&engine());
    assert!(
        has_rule(&xs, 42.0) && has_rule(&xs, 553.3),
        "w:w=0 dxa 應視為 auto→自然寬 511.3pt 均分越界（42.0/553.3），xs={xs:?}"
    );
}

/// 紅燈 T5：1 twip（0.05pt）微縮排不該讓 K7 均分臂整臂失效——
/// 守衛容差比較（|ind|<0.1pt 視為無顯式縮排）後仍走均分。
/// 修正前 `== 0.0` 精確比較→臂跳過→x=0.05pt 貼左（位移從 −20.0pt 階躍
/// 到 +0.05pt，20pt 級不連續）＝紅。
#[test]
fn k7c_tiny_indent_within_tolerance_keeps_even_split() {
    let xs = vertical_rule_xs(&engine());
    assert!(
        has_rule(&xs, 52.0) && has_rule(&xs, 543.3),
        "tblInd 0.05pt（1 twip）超寬自然表仍應均分（52.0/543.3），xs={xs:?}"
    );
}
