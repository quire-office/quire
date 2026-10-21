//! 註腳／尾註編號消費層（H2／F3b 共用地基；純函式、零狀態）。
//!
//! 只做三件事（全部有實檔／ECMA 出處，不做臆造）：
//! 1. [`merge_note_pr`]：settings 層＋sectPr 層逐欄覆寫合併（§17.11.5 語義：
//!    節宣告的欄覆寫、未宣告沿用文件預設——**逐欄**，非整組替換）。
//! 2. [`label`]：把計數值按 `w:numFmt` 拍成發射字面（decimal／lowerRoman／
//!    chicago——只收 corpus 實見值；未見值回十進位，與無 numFmt 逐字相同）。
//! 3. [`assign`]：沿錨點引用序計數，按 `w:numRestart` 決定重啟時機，回
//!    「註 id → 字面」表（`number_label` 的來源；空字串＝十進位語義由
//!    `footnotes::note_number_text` 兜底）。
//!
//! ## 老實邊界（寫給未來的自己）
//! - `eachPage`（每頁重啟）：**corpus 零實檔**，且頁序要等分頁才知道——與
//!   「編號先於版面」的現行管線循環依賴。現行退回 `Continuous` 並在呼叫端
//!   註釋標明；真做要等分頁回饋（同 F3c 候選）。
//! - `chicago`（§17.11.18 值域內、corpus 1 檔實見）：Word 的符號序列
//!   `* † ‡ § # ** †† ‡‡ …`（第 7 條起重複符號加倍）。超出 2 輪加倍後
//!   回到首符號計數加權（Word 實務不超過 2 輪，corpus 無證據再外推）。
//! - 未見 numFmt 值（H1 解析已老實歸 None）⇒ 十進位（不虛構格式）。

use crate::footnotes::Anchor;
use std::collections::HashMap;

/// ECMA `w:numFmt` 的發射字面（只吃 H1 枚舉——未見值在解析層已是 None）。
///
/// `None`／`Decimal` ⇒ 十進位字面；與 F2b-2／F3 的舊行為逐字相同
/// （零變化守門的依據）。
pub fn label(fmt: Option<quire_ooxml::NoteNumberFormat>, n: usize) -> String {
    use quire_ooxml::NoteNumberFormat as F;
    match fmt {
        None | Some(F::Decimal) => n.to_string(),
        Some(F::LowerRoman) => roman(n),
        Some(F::Chicago) => chicago(n),
    }
}

/// 小寫羅馬數字（1..=3999；越界回十進位兜底，不 panic 不胡編）。
fn roman(n: usize) -> String {
    const TAB: [(i32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    if !(1..=3999).contains(&n) {
        return n.to_string();
    }
    let mut v = n as i32;
    let mut out = String::new();
    for (val, sym) in TAB {
        while v >= val {
            out.push_str(sym);
            v -= val;
        }
    }
    out
}

/// Chicago 註腳符號序列（Word 實務）：`* † ‡ § # ** †† ‡‡ §§ ## …`。
///
/// 第 1 輪：`* † ‡ § #`；第 k 輪（k≥2）＝第 1 輪第 i 個符號重複 k 次。
/// n ≥ 31（第 6 輪起）實務罕見：回十進位兜底（不臆造更長序列）。
fn chicago(n: usize) -> String {
    const MARKS: [&str; 5] = ["*", "\u{2020}", "\u{2021}", "\u{00a7}", "#"];
    if n == 0 {
        return String::new();
    }
    if n > 30 {
        return n.to_string();
    }
    let round = (n - 1) / MARKS.len(); // 0-based 輪次
    let mark = MARKS[(n - 1) % MARKS.len()];
    mark.repeat(round + 1)
}

/// 合併一欄：覆寫側有值才蓋（逐欄覆寫語義）。
fn merge_field<T>(base: Option<T>, over: Option<T>) -> Option<T> {
    over.or(base)
}

/// settings 層＋sectPr 層 `NotePr` 逐欄覆寫合併（§17.11.5 節覆寫語義）。
///
/// 兩側都缺 ⇒ `None`（＝不消費，走十進位 continuous 原路徑）。
pub fn merge_note_pr(
    settings: Option<quire_ooxml::NotePr>,
    section: Option<quire_ooxml::NotePr>,
) -> Option<quire_ooxml::NotePr> {
    match (settings, section) {
        (None, None) => None,
        (s @ Some(_), None) | (None, s @ Some(_)) => s,
        (Some(a), Some(b)) => Some(quire_ooxml::NotePr {
            num_fmt: merge_field(a.num_fmt, b.num_fmt),
            num_restart: merge_field(a.num_restart, b.num_restart),
            num_start: merge_field(a.num_start, b.num_start),
            pos: merge_field(a.pos, b.pos),
            ref_id: merge_field(a.ref_id, b.ref_id),
            separator_id: merge_field(a.separator_id, b.separator_id),
            continuation_separator_id: merge_field(
                a.continuation_separator_id,
                b.continuation_separator_id,
            ),
        }),
    }
}

/// 一個節的最终編號設定（合併後）；缺欄＝預設語義。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedNumbering {
    /// 編號格式（None＝十進位）。
    pub num_fmt: Option<quire_ooxml::NoteNumberFormat>,
    /// 重啟時機（`EachPage` 由 [`each_page_to_continuous`] 退回 Continuous；
    /// None＝continuous）。
    pub num_restart: Option<quire_ooxml::NoteRestart>,
    /// 起始值（僅在第一段生效；None＝1）。
    pub num_start: Option<i32>,
}

/// `eachPage` 的誠實退化：現行管線編號先於版面，頁序不可知 ⇒ 退回
/// continuous（呼叫端註釋標明；真做屬分頁回饋候選）。
pub fn each_page_to_continuous(
    r: Option<quire_ooxml::NoteRestart>,
) -> Option<quire_ooxml::NoteRestart> {
    match r {
        Some(quire_ooxml::NoteRestart::EachPage) => Some(quire_ooxml::NoteRestart::Continuous),
        other => other,
    }
}

/// 沿錨點序計數，回「註 id → 編號字面」。
///
/// - `resolve(section_id)` 回該節合併後的設定（呼叫端持有 settings/sectPr
///   兩層表；無任何宣告 ⇒ 回 `None` 欄的 `ResolvedNumbering` 即可）。
/// - 計數規則（測死）：依錨點**引用序**遞增；同一則註腳第二次被引用**不**
///   再計數（沿用已有 id 的字面——同頁多引用同則＝同字面）。
/// - `EachSect`：section id 改變 ⇒ 計數器重啟（從該節 `num_start` 或 1）；
///   `num_start` 只在整體第一段生效（Word 實務：numStart 文件級；節級
///   numStart 無 corpus 實據，不臆造第二語義）。
/// - `Continuous`（含 None／EachPage 退化）：一路遞增，`num_start` 定起點。
///
/// 回表中**未列**的 id＝無字面（呼叫端保持十進位 `number` 兜底——零變化）。
pub fn assign(
    anchors: &[Anchor],
    para_sec: &[u32],
    resolve: impl Fn(Option<u32>) -> ResolvedNumbering,
) -> HashMap<i64, String> {
    let mut out: HashMap<i64, String> = HashMap::new();
    let mut seen: Vec<i64> = Vec::new();
    let mut counter: i64 = -1; // 節域計數器（-1＝尚未起步）
    let mut overall: i64 = 0; // 全域首引用序（對齊 note_chain 的 number 兜底）
    let mut cur_sec: Option<u32> = None;
    let mut first = true;
    for a in anchors {
        if seen.contains(&a.note_id) {
            continue; // 同則再引用：字面沿用，不重計（兩側計數皆不動）。
        }
        seen.push(a.note_id);
        let sec = para_sec.get(a.para).copied();
        let r = resolve(sec);
        let restart = each_page_to_continuous(r.num_restart);
        let sect_changed = match (cur_sec, sec) {
            (Some(prev), Some(now)) => prev != now,
            _ => false,
        };
        if first || (restart == Some(quire_ooxml::NoteRestart::EachSect) && sect_changed) {
            counter = (r.num_start.unwrap_or(1).max(1)) as i64 - 1;
        }
        cur_sec = sec.or(cur_sec);
        overall += 1;
        counter += 1;
        let n = counter.max(1) as usize;
        let non_default_fmt = !matches!(
            r.num_fmt,
            None | Some(quire_ooxml::NoteNumberFormat::Decimal)
        );
        // 零變化守門：十進位且計數與全域序一致（無重啟、無 numStart≠1）
        // ⇒ **不寫** label——呼叫端十進位兜底逐字零變化（JSON/渲染皆然）。
        if non_default_fmt || (n as i64) != overall {
            out.insert(a.note_id, label(r.num_fmt, n));
        }
        first = false;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use quire_ooxml::{NoteNumberFormat as F, NotePr, NoteRestart as R};

    fn rn(fmt: Option<F>, restart: Option<R>, start: Option<i32>) -> ResolvedNumbering {
        ResolvedNumbering {
            num_fmt: fmt,
            num_restart: restart,
            num_start: start,
        }
    }
    fn anchor(id: i64, para: usize) -> Anchor {
        Anchor {
            note_id: id,
            para,
            run: 0,
            cp_before: 0,
            cp_len: 0,
        }
    }

    #[test]
    fn roman_table() {
        assert_eq!(roman(1), "i");
        assert_eq!(roman(4), "iv");
        assert_eq!(roman(9), "ix");
        assert_eq!(roman(14), "xiv");
        assert_eq!(roman(40), "xl");
        assert_eq!(roman(1999), "mcmxcix");
        assert_eq!(roman(0), "0", "越界回十進位兜底");
        assert_eq!(roman(4000), "4000");
    }

    #[test]
    fn chicago_sequence_is_word_order() {
        assert_eq!(chicago(1), "*");
        assert_eq!(chicago(2), "\u{2020}");
        assert_eq!(chicago(6), "**", "第二輪＝符號加倍");
        assert_eq!(chicago(10), "##");
        assert_eq!(chicago(11), "***");
        assert_eq!(chicago(31), "31", "31 起十進位兜底（不臆造）");
    }

    #[test]
    fn merge_is_per_field_override() {
        let base = NotePr {
            num_fmt: Some(F::Decimal),
            num_start: Some(7),
            ..Default::default()
        };
        let over = NotePr {
            num_restart: Some(R::EachSect),
            num_start: None,
            ..Default::default()
        };
        let m = merge_note_pr(Some(base), Some(over)).unwrap();
        assert_eq!(m.num_fmt, Some(F::Decimal), "未覆寫欄沿用");
        assert_eq!(m.num_restart, Some(R::EachSect), "覆寫欄生效");
        assert_eq!(m.num_start, Some(7), "覆寫側 None 不蓋掉 base 值");
        assert!(
            merge_note_pr(None, None).is_none(),
            "兩側都缺 ⇒ None（零變化）"
        );
    }

    #[test]
    fn continuous_counts_across_sections_and_skips_repeats() {
        let anchors = vec![anchor(1, 0), anchor(2, 1), anchor(1, 2), anchor(3, 3)];
        let para_sec = vec![0u32, 0, 1, 1];
        let m = assign(&anchors, &para_sec, |_| rn(Some(F::LowerRoman), None, None));
        assert_eq!(m[&1], "i");
        assert_eq!(m[&2], "ii");
        // 錨點序 1,2,1,3 ⇒ 首引用序 1,2,3（id=1 的重複引用**不**計數）
        // ⇒ id=3 拿 iii（若誤計數會得到 iv——本斷言正是測這個）。
        assert_eq!(m[&3], "iii");
    }

    #[test]
    fn each_sect_restarts_counter() {
        let anchors = vec![anchor(1, 0), anchor(2, 1), anchor(3, 2)];
        let para_sec = vec![0u32, 0, 1];
        let m = assign(&anchors, &para_sec, |sec| {
            rn(
                Some(F::LowerRoman),
                Some(R::EachSect),
                if sec == Some(0) { Some(1) } else { None },
            )
        });
        assert_eq!(m[&1], "i");
        assert_eq!(m[&2], "ii");
        assert_eq!(m[&3], "i", "第二節重啟 ⇒ iii 變 i");
    }

    #[test]
    fn num_start_applies_once_at_head() {
        let anchors = vec![anchor(1, 0), anchor(2, 1)];
        let para_sec = vec![0u32, 0];
        let m = assign(&anchors, &para_sec, |_| rn(Some(F::Chicago), None, Some(3)));
        assert_eq!(m[&1], "\u{2021}", "起點 3 ⇒ ‡");
        assert_eq!(m[&2], "\u{00a7}", "往後順延 4 ⇒ §");
    }

    #[test]
    fn decimal_never_writes_a_label() {
        // 零變化守門：無 numFmt／decimal ⇒ 表為空 ⇒ 呼叫端十進位兜底。
        let anchors = vec![anchor(1, 0), anchor(2, 1)];
        let m = assign(&anchors, &para_sec_all_zero(), |_| rn(None, None, None));
        assert!(m.is_empty());
        let m2 = assign(&anchors, &para_sec_all_zero(), |_| {
            rn(Some(F::Decimal), None, None)
        });
        assert!(m2.is_empty());
    }

    fn para_sec_all_zero() -> Vec<u32> {
        vec![0u32, 0]
    }

    #[test]
    fn each_page_degrades_to_continuous_honestly() {
        assert_eq!(
            each_page_to_continuous(Some(R::EachPage)),
            Some(R::Continuous),
            "eachPage 循環依賴 ⇒ 誠實退化（doc 已標明）"
        );
        assert_eq!(
            each_page_to_continuous(Some(R::EachSect)),
            Some(R::EachSect)
        );
        assert_eq!(each_page_to_continuous(None), None);
    }
}
