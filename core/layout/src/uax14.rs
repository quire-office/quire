//! UAX#14 斷字規則層：把文字摺成「斷行單元（atom）」序列。
//!
//! 斷字機會取 `unicode-linebreak`（UAX#14 預設演算法的參考級實作：no_std、
//! 表由 LineBreak.txt 自動產生、零系統相依——wasm32 可編譯，符合雙殼鐵律）。
//! 本模組只做排版需要的前處理：
//!
//! - 每個 atom 是兩個相鄰斷字機會之間的不可切片段：LB13（閉標點黏前字）、
//!   LB14（開標點不離後字）、LB7/LB25（`3,000`、`50%`、`$100` 不拆）、
//!   NBSP/WJ glue、LB19（引號兩側不斷）全部由底層演算法決定，atom 內部
//!   因此「天然不可斷」。
//! - `forced`：斷點是 mandatory（`\n`、CR、CRLF、FF、NEL、LS、PS）。控制
//!   字元留在 atom 末尾（trim 區，不計寬），由斷行器在該 atom 後強制換行。
//! - `trim_from`：單元末尾不計寬區（空白＋控制字元）的起點——斷行器算行寬
//!   不計 `trim_from..end`，行尾不撐行。
//! - `hyphen`：斷點前一字元是連字號/軟連字號——斷此點按連字計價，讓 KP
//!   偏好其他斷點。
//!
//! 構造性質（kp.rs 的緊急切分與 property test 依賴）：
//! - atoms 首尾相接、完整覆蓋全文、每個 atom 非空；
//! - 可斷空白（SP/BA/Zs）只出現在 atom **尾端** trim 區——atom 主體
//!   （start..trim_from）內沒有可斷空白，故緊急切分永不切出空白開頭的行。
//!
//! 已知裁切（骨架期刻意不做；只影響斷點多寡，不影響正確性閘門）：
//! eastAsianWidth 對半形/CJK 標點的斷行抑制（`abc.` 後英文環境仍給斷點、
//! `——` 可拆）、LB21a 對 `--`/URL 的細化、LB25 全形數字細目。真實語料
//! 差異由 pagination golden 快照兜底。

use unicode_linebreak::{linebreaks, BreakOpportunity};

use crate::EOL_CONTROLS;

/// 斷行單元：源文字 `text[start..end]`，斷行器只准在 atom 邊界切行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atom {
    pub start: usize,
    pub end: usize,
    /// 行寬計算的不計寬起點（末尾空白＋控制字元區）。
    /// 恆有 `start <= trim_from <= end`。
    pub trim_from: usize,
    /// 此 atom 之後必須斷行（mandatory 控制字元在 atom 尾端、不計寬）。
    pub forced: bool,
    /// 此 atom 之後若斷行，屬「連字號斷行」（計價用）。
    pub hyphen: bool,
}

/// 空白類（LB18 的 WS：空格、tab，以及 Zs——Zs 多為 BA 類，斷點在其後，
/// 自然附著前單元並在 trim 區消化）。NBSP 是 NonBreakingGlue，**不是**空白。
#[inline]
pub(crate) fn is_ws(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

/// 行尾不計寬字元：空白＋mandatory 控制字元（斷行標記，不顯示不計寬）。
#[inline]
pub(crate) fn is_ignorable(c: char) -> bool {
    is_ws(c) || EOL_CONTROLS.contains(&c)
}

/// 把文字摺成 atom 序列（見模組頭的文件）。
pub fn break_atoms(text: &str) -> Vec<Atom> {
    if text.is_empty() {
        return Vec::new();
    }
    // (位元組斷點, 是否 mandatory)。unicode-linebreak 給 EOF 的斷點也是
    // Mandatory（「總要至少有一行」的技術性標記），降回普通段尾。
    let mut splits: Vec<(usize, bool)> = linebreaks(text)
        .map(|(p, o)| {
            (
                p,
                matches!(o, BreakOpportunity::Mandatory) && p < text.len(),
            )
        })
        .collect();
    if splits.last().is_none_or(|(p, _)| *p != text.len()) {
        splits.push((text.len(), false));
    }

    let mut atoms: Vec<Atom> = Vec::with_capacity(splits.len());
    let mut start = 0usize;
    for &(p, forced) in &splits {
        if p <= start {
            continue; // 防禦：斷點嚴格遞增，理論不發生重複
        }
        atoms.push(Atom {
            start,
            end: p,
            trim_from: trim_trailing_ignorable(text, start, p),
            forced,
            hyphen: matches!(
                text[start..p].chars().next_back(),
                Some('-') | Some('\u{ad}') // 連字號 / 軟連字號（SHY）
            ),
        });
        start = p;
    }
    atoms
}

/// `text[a..b]` 去掉末尾連續不計寬字元後的不計寬起點。
fn trim_trailing_ignorable(text: &str, a: usize, b: usize) -> usize {
    let mut t = b;
    while let Some(c) = text[a..t].chars().next_back() {
        if is_ignorable(c) {
            t -= c.len_utf8();
        } else {
            break;
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atoms_str(text: &str) -> Vec<String> {
        break_atoms(text)
            .into_iter()
            .map(|a| text[a.start..a.end].to_string())
            .collect()
    }

    #[test]
    fn latin_words_carry_trailing_space() {
        assert_eq!(atoms_str("hello world"), vec!["hello ", "world"]);
        assert_eq!(atoms_str("end "), vec!["end "]);
    }

    #[test]
    fn cjk_breaks_between_every_pair() {
        assert_eq!(atoms_str("中文測試"), vec!["中", "文", "測", "試"]);
    }

    #[test]
    fn punctuation_gluing_follows_uax14() {
        // LB13：閉標點永不許落斷點右側；LB14：開標點不離後字；
        // LB19：引號兩側不斷（“hi” 整塊）。
        assert_eq!(atoms_str("結束。"), vec!["結", "束。"]);
        assert_eq!(
            atoms_str("引用「開頭」結尾"),
            vec!["引", "用", "「開", "頭」", "結", "尾"]
        );
        assert_eq!(atoms_str("真的嗎？好"), vec!["真", "的", "嗎？", "好"]);
        assert_eq!(
            atoms_str("he said “hi” ok"),
            vec!["he ", "said ", "“hi” ", "ok"]
        );
    }

    #[test]
    fn mandatory_breaks_marked_and_control_stays_in_atom() {
        let a = break_atoms("a\nb");
        assert_eq!(a.len(), 2);
        assert!(a[0].forced);
        assert_eq!(&"a\nb"[a[0].start..a[0].end], "a\n");
        assert_eq!(a[0].trim_from, 1, "控制字元不計寬：{a:?}");
        assert_eq!(&"a\nb"[a[1].start..a[1].end], "b");
        for t in [
            "a\u{2028}b",
            "a\u{2029}b",
            "a\r\nb",
            "a\rb",
            "a\u{85}b",
            "a\u{0c}b",
        ] {
            let bs = break_atoms(t);
            assert!(bs.iter().any(|x| x.forced), "{t:?} 應含 mandatory 斷點");
        }
        // 連續強制斷點：中間是僅含控制字元的 atom（trim 後主體為空）。
        let a = break_atoms("a\n\nb");
        assert_eq!(a.len(), 3);
        assert_eq!(&"a\n\nb"[a[1].start..a[1].end], "\n");
        assert_eq!(a[1].start, a[1].trim_from, "純控制 atom 主體應為空");
    }

    #[test]
    fn nbsp_never_breaks_and_fullwidth_space_glues() {
        assert_eq!(atoms_str("word\u{a0}nbsp"), vec!["word\u{a0}nbsp"]);
        // 全形空格（BA 類，斷點在其後）：附著左單元且在 trim 區。
        assert_eq!(atoms_str("中\u{3000}文"), vec!["中\u{3000}", "文"]);
        let a = break_atoms("end\u{3000}next");
        assert_eq!(a[0].trim_from, 3, "全形空格不計寬：{a:?}");
    }

    #[test]
    fn numeric_runs_stay_together() {
        // LB25：數字串、貨幣、百分號不拆。
        assert_eq!(atoms_str("3,000 元"), vec!["3,000 ", "元"]);
        assert_eq!(atoms_str("50% off"), vec!["50% ", "off"]);
        assert_eq!(atoms_str("$100 ok"), vec!["$100 ", "ok"]);
    }

    #[test]
    fn combining_marks_and_zwj_never_split() {
        // e+U+0301 與 ZWJ 序列永不落 atom 頭（LB6）。
        for t in ["café", "café", "a\u{200d}b"] {
            let atoms = break_atoms(t);
            for a in &atoms {
                let s = &t[a.start..a.end];
                assert!(
                    !s.starts_with('\u{301}') && !s.starts_with('\u{200d}'),
                    "{t:?} 的組合字元被斷到單元頭：{a:?}"
                );
            }
        }
    }

    #[test]
    fn hyphen_flag() {
        let a = break_atoms("he-llo wo-rld");
        assert!(
            a.iter().filter(|x| x.hyphen).count() >= 2,
            "連字號後斷點應標 hyphen：{a:?}"
        );
    }

    #[test]
    fn atoms_tile_text_and_keep_ws_only_at_tails() {
        for t in [
            "中文 English 混排 test，含標點！",
            "a\nb\r\nc\u{2028}d",
            "多   空   格",
            "",
            "   ",
            "  中",
            "USD 95,000.50（約 3,000,000 元）於 2026-10-14 核定。",
            "引用「開頭」結尾（括號）《书名号》與『』",
            "a\u{a0}b c\u{a0}d",
            "中\u{3000}全形空格\u{3000}文",
            "a \n b",
        ] {
            let atoms = break_atoms(t);
            let mut pos = 0usize;
            for a in &atoms {
                assert_eq!(a.start, pos, "atoms 必須首尾相接：{t:?} @ {a:?}");
                assert!(
                    a.start < a.end && a.start <= a.trim_from && a.trim_from <= a.end,
                    "空 atom 或 trim_from 越界：{a:?}"
                );
                // 構造性質：atom 主體內沒有可斷空白。
                assert!(
                    !t[a.start..a.trim_from].chars().any(is_ws),
                    "可斷空白混入 atom 主體：{t:?} @ {a:?}"
                );
                pos = a.end;
            }
            assert_eq!(pos, t.len(), "atoms 必須覆蓋全文：{t:?}");
        }
    }
}
