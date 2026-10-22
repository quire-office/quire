//! 清單符號發射層（K2b；吃 K2a 的 [`quire_ooxml::Numbering`]）。
//!
//! 本模組只做「段落引用 → 合成 marker 文字」的純函式計算；真正插進發射
//! 投影的動作在 [`crate::cascade_paragraphs`]（marker run 一律插在級聯後
//! 的 eff run **之前**，存檔路徑完全不經投影）。
//!
//! ## 已做
//! - 計數器逐 `(numId, ilvl)` 推進；`start`／`startOverride` 起步；
//!   父層前進一律把更深層計數器清回起始值（標準多層語義）。
//! - `numRestart=eachSect`：同一 `numId` 跨節邊界重啟（節 id 由 engine
//!   的 `para_sec` 提供，來源＝既有 `scan_sections`）。
//! - `lvlRestart=0`：該層**永不因父層前進而重啟**（corpus 實見的唯一值）。
//! - `lvlText` 的 `%N` 展開：decimal／decimalZero／lower/upperLetter／
//!   lower/upperRoman／custom；`None` 按 decimal 兜底。
//! - 私用區符號映射（只收 corpus 實見值）：`rFonts@ascii="Symbol"`＋
//!   U+F0B7 → `•`；`Wingdings`＋U+F0A7 → `▪`、U+F0FC → `✓`。其餘私用區
//!   字元**原樣保留**（不猜）。
//! - `suff`：`nothing`＝直接接排、`space`＝一格；`tab`／缺＝一格（視覺
//!   兜底——tab 停止位屬後續卡）。
//!
//! ## 未做（老實邊界）
//! - `w:isLgl`（數字轉字母）不消費。
//! - `w:picBullet`（圖片項目符號）不解析。
//! - 樣式繼承編號（`styleLink`／`numStyleLink`）不追——只認段落直接
//!   `w:pPr/w:numPr`。
//! - 懸掛縮排只以「首行前置空白」近似（見 `ListMarker::indent_pt`）；
//!   續行左縮排需要版面層逐段可用寬支援，屬後續卡。

use std::collections::HashMap;

use quire_ooxml::{Level, NumberFormat, Numbering, NumberingRef};

/// 一段清單投影：[`crate::cascade_paragraphs`] 據此在 eff run 前插 marker。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListMarker {
    /// marker 本體＋後綴（不含前置縮排空白；空白由發射層按字級量測補）。
    pub text: String,
    /// 首行縮排量（pt；`ind_left − ind_hanging`，不低於 0）。
    pub indent_pt: f32,
}

/// 逐段算出 marker（與 `paras` 同長；無 list 引用＝全 `None`）。
///
/// `para_sec[i]`＝段落 i 所屬 section id（缺位＝0）。純函式、零狀態洩漏。
pub(crate) fn compute_markers(
    numbering: &Numbering,
    refs: &[NumberingRef],
    para_sec: &[u32],
    n_paras: usize,
) -> Vec<Option<ListMarker>> {
    let mut out: Vec<Option<ListMarker>> = vec![None; n_paras];
    if refs.is_empty() {
        return out;
    }
    // numId → 有效層級表（一次解析、全檔重用）。
    let mut levels_cache: HashMap<u32, Option<Vec<Level>>> = HashMap::new();
    // (numId, ilvl) → 目前計數值。
    let mut counters: HashMap<(u32, u32), i64> = HashMap::new();
    // numId → 上一筆引用所在節（eachSect 重啟判定）。
    let mut last_sec: HashMap<u32, u32> = HashMap::new();

    for r in refs {
        if r.para_index >= n_paras {
            continue; // 越界引用老實跳過（不 panic）。
        }
        let levels = levels_cache
            .entry(r.num_id)
            .or_insert_with(|| numbering.effective_levels(r.num_id));
        let Some(levels) = levels.as_ref() else {
            continue; // numId 無有效定義（含 numId=0）——不虛構 marker。
        };
        let Some(level) = levels.iter().find(|l| l.ilvl == r.ilvl) else {
            continue;
        };

        // 節邊界重啟（numRestart=eachSect；本層或同實例任一層宣告即生效）。
        let each_sect = levels.iter().any(|l| {
            l.num_restart
                .as_deref()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("eachSect"))
        });
        let sec = para_sec.get(r.para_index).copied().unwrap_or(0);
        if each_sect {
            if let Some(prev) = last_sec.get(&r.num_id) {
                if *prev != sec {
                    counters.retain(|(num, _), _| *num != r.num_id);
                }
            }
        }
        last_sec.insert(r.num_id, sec);

        // 推進本層（首見＝start；start 缺＝1，ECMA 預設）。
        let start = level.start.unwrap_or(1);
        let c = counters
            .entry((r.num_id, r.ilvl))
            .or_insert_with(|| start.saturating_sub(1));
        *c = c.saturating_add(1);

        // 父層前進 ⇒ 重置所有子層（lvlRestart=0 的子層除外）。
        let deeper: Vec<u32> = counters
            .keys()
            .filter(|(num, lvl)| *num == r.num_id && *lvl > r.ilvl)
            .map(|(_, lvl)| *lvl)
            .collect();
        for d in deeper {
            let never = levels
                .iter()
                .find(|l| l.ilvl == d)
                .and_then(|l| l.lvl_restart)
                == Some(0);
            if !never {
                counters.remove(&(r.num_id, d));
            }
        }

        // 顯式「不顯示編號」的層不發 marker（計數器照樣推進）。
        if level.num_fmt == Some(NumberFormat::None) {
            continue;
        }
        let Some(raw) = level.lvl_text.as_deref() else {
            continue;
        };
        let expanded = expand_lvl_text(raw, levels, &counters, r.num_id);
        let body = map_private_use(&expanded, level.rpr_font_ascii.as_deref());
        if body.is_empty() {
            continue;
        }
        let suffix = match level
            .suff
            .as_deref()
            .map(|s| s.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("nothing") => "",
            // `tab` 的 tab 停止位屬後續卡；先以單格空白做視覺兜底。
            _ => " ",
        };
        let indent_pt =
            (level.ind_left_pt.unwrap_or(0.0) - level.ind_hanging_pt.unwrap_or(0.0)).max(0.0);
        out[r.para_index] = Some(ListMarker {
            text: format!("{body}{suffix}"),
            indent_pt,
        });
    }
    out
}

/// `w:lvlText` 的 `%N` 佔位展開（N＝1-based 層級；越界層級按 decimal 兜底）。
fn expand_lvl_text(
    text: &str,
    levels: &[Level],
    counters: &HashMap<(u32, u32), i64>,
    num_id: u32,
) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
            if let Some(d) = chars[i + 1].to_digit(10) {
                if (1..=9).contains(&d) {
                    let idx = d - 1;
                    let lvl = levels.iter().find(|l| l.ilvl == idx);
                    let n = counters
                        .get(&(num_id, idx))
                        .copied()
                        .or_else(|| lvl.and_then(|l| l.start))
                        .unwrap_or(1);
                    match lvl {
                        Some(l) => out.push_str(&format_number(l, n)),
                        None => out.push_str(&n.to_string()),
                    }
                    i += 2;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// 按層級 `numFmt` 把計數值拍成字面（未見格式＝decimal 兜底）。
fn format_number(level: &Level, n: i64) -> String {
    match level.num_fmt {
        Some(NumberFormat::LowerRoman) => roman(n, false),
        Some(NumberFormat::UpperRoman) => roman(n, true),
        Some(NumberFormat::LowerLetter) => letter(n, false),
        Some(NumberFormat::UpperLetter) => letter(n, true),
        Some(NumberFormat::DecimalZero) => format!("{n:02}"),
        Some(NumberFormat::Custom) => level
            .num_fmt_format
            .clone()
            .unwrap_or_else(|| n.to_string()),
        // decimal／bullet／未見值／缺宣告 ⇒ 十進位（K2a 的 None 語義）。
        _ => n.to_string(),
    }
}

/// 小寫／大寫羅馬數字（1..=3999；越界回十進位兜底，不 panic 不胡編）。
fn roman(n: i64, upper: bool) -> String {
    const TAB: [(i64, &str); 13] = [
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
    let mut v = n;
    let mut out = String::new();
    for (val, sym) in TAB {
        while v >= val {
            out.push_str(sym);
            v -= val;
        }
    }
    if upper {
        out.to_ascii_uppercase()
    } else {
        out
    }
}

/// 字母編號（1→a、26→z、27→aa；Word 的 bijective base-26）。
fn letter(n: i64, upper: bool) -> String {
    if n <= 0 {
        return n.to_string();
    }
    let mut v = n as u32;
    let mut out = String::new();
    while v > 0 {
        let rem = ((v - 1) % 26) as u8;
        out.push((b'a' + rem) as char);
        v = (v - 1) / 26;
    }
    let s: String = out.chars().rev().collect();
    if upper {
        s.to_ascii_uppercase()
    } else {
        s
    }
}

/// 私用區符號映射（**只收 corpus 實見值**；其餘原樣保留）。
///
/// 未映射字元不是漏做，是「沒有字型證據就不猜」——U+F0B7 只在 Symbol
/// 字型下才是「•」，Wingdings 的 U+F0A7／U+F0FC 同理。
fn map_private_use(text: &str, font: Option<&str>) -> String {
    let font = font.unwrap_or("").trim();
    let is_symbol = font.eq_ignore_ascii_case("Symbol");
    let is_wingdings = font.eq_ignore_ascii_case("Wingdings");
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match u32::from(ch) {
            0xF0B7 if is_symbol => out.push('•'),
            0xF0A7 if is_wingdings => out.push('▪'),
            0xF0FC if is_wingdings => out.push('✓'),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use quire_ooxml::{parse_numbering, NumberingRef};

    fn numbering(lvls: &str) -> Numbering {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">{lvls}</w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
</w:numbering>"#
        );
        parse_numbering(xml.as_bytes())
    }

    fn r(para_index: usize, ilvl: u32) -> NumberingRef {
        NumberingRef {
            para_index,
            ilvl,
            num_id: 1,
        }
    }

    fn texts(markers: &[Option<ListMarker>]) -> Vec<Option<&str>> {
        markers
            .iter()
            .map(|m| m.as_ref().map(|x| x.text.as_str()))
            .collect()
    }

    /// (b) `%1.%2` 兩層展開：父層前進後子層沿用父層目前值。
    #[test]
    fn two_level_placeholder_expands() {
        let n = numbering(
            r#"<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl>
               <w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1.%2"/></w:lvl>"#,
        );
        let refs = [r(0, 0), r(1, 0), r(2, 1)];
        let m = compute_markers(&n, &refs, &[0, 0, 0], 3);
        assert_eq!(texts(&m), vec![Some("1. "), Some("2. "), Some("2.1 ")]);
    }

    /// (c) 父層前進 ⇒ 子層計數器重置。
    #[test]
    fn parent_advance_resets_children() {
        let n = numbering(
            r#"<w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl>
               <w:lvl w:ilvl="1"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1.%2."/></w:lvl>"#,
        );
        let refs = [r(0, 0), r(1, 1), r(2, 1), r(3, 0), r(4, 1)];
        let m = compute_markers(&n, &refs, &[0; 5], 5);
        assert_eq!(
            texts(&m),
            vec![
                Some("1. "),
                Some("1.1. "),
                Some("1.2. "),
                Some("2. "),
                Some("2.1. ")
            ]
        );
    }

    /// (d) `numRestart=eachSect`：同一 numId 跨節邊界重啟；`continuous` 不重啟。
    #[test]
    fn each_sect_restarts_counter() {
        let sect = |v: &str| {
            numbering(&format!(
                r#"<w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:numRestart w:val="{v}"/><w:lvlText w:val="%1."/></w:lvl>"#
            ))
        };
        let refs = [r(0, 0), r(1, 0), r(2, 0), r(3, 0)];
        let secs = [0, 0, 1, 1];
        let m = compute_markers(&sect("eachSect"), &refs, &secs, 4);
        assert_eq!(
            texts(&m),
            vec![Some("1. "), Some("2. "), Some("1. "), Some("2. ")]
        );
        let m = compute_markers(&sect("continuous"), &refs, &secs, 4);
        assert_eq!(
            texts(&m),
            vec![Some("1. "), Some("2. "), Some("3. "), Some("4. ")]
        );
    }

    /// `lvlRestart=0`：子層永不因父層前進而重啟（其餘層照常重置）。
    #[test]
    fn lvl_restart_zero_keeps_child_counter() {
        let n = numbering(
            r#"<w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl>
               <w:lvl w:ilvl="1"><w:numFmt w:val="decimal"/><w:lvlRestart w:val="0"/><w:lvlText w:val="%2."/></w:lvl>"#,
        );
        let refs = [r(0, 0), r(1, 1), r(2, 0), r(3, 1)];
        let m = compute_markers(&n, &refs, &[0; 4], 4);
        assert_eq!(
            texts(&m),
            vec![Some("1. "), Some("1. "), Some("2. "), Some("2. ")]
        );
    }

    /// (e) 私用區符號映射：Symbol F0B7→•、Wingdings F0A7→▪／F0FC→✓；
    /// 未映射字元原樣保留（不猜）。
    #[test]
    fn private_use_symbols_map_only_with_font_evidence() {
        let n = numbering(
            r#"<w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/><w:lvlText w:val="&#xF0B7;"/><w:rPr><w:rFonts w:ascii="Symbol"/></w:rPr></w:lvl>
               <w:lvl w:ilvl="1"><w:numFmt w:val="bullet"/><w:lvlText w:val="&#xF0A7;"/><w:rPr><w:rFonts w:ascii="Wingdings"/></w:rPr></w:lvl>
               <w:lvl w:ilvl="2"><w:numFmt w:val="bullet"/><w:lvlText w:val="&#xF0FC;"/><w:rPr><w:rFonts w:ascii="Wingdings"/></w:rPr></w:lvl>
               <w:lvl w:ilvl="3"><w:numFmt w:val="bullet"/><w:lvlText w:val="&#xF0B8;"/><w:rPr><w:rFonts w:ascii="Symbol"/></w:rPr></w:lvl>"#,
        );
        let refs = [r(0, 0), r(1, 1), r(2, 2), r(3, 3)];
        let m = compute_markers(&n, &refs, &[0; 4], 4);
        assert_eq!(
            texts(&m),
            vec![Some("• "), Some("▪ "), Some("✓ "), Some("\u{f0b8} "),],
            "未映射私用區字元必須原樣保留"
        );
    }

    /// (f) `suff=nothing` 直接接排；`space`／缺省隔一格。
    #[test]
    fn suffix_follows_suff() {
        let mk = |suff: &str| {
            numbering(&format!(
                r#"<w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/>{suff}<w:lvlText w:val="%1."/></w:lvl>"#
            ))
        };
        let refs = [r(0, 0)];
        assert_eq!(
            texts(&compute_markers(
                &mk(r#"<w:suff w:val="nothing"/>"#),
                &refs,
                &[0],
                1
            )),
            vec![Some("1.")],
            "suff=nothing 不得加空白"
        );
        assert_eq!(
            texts(&compute_markers(
                &mk(r#"<w:suff w:val="space"/>"#),
                &refs,
                &[0],
                1
            )),
            vec![Some("1. ")],
            "suff=space 隔一格"
        );
        assert_eq!(
            texts(&compute_markers(&mk(""), &refs, &[0], 1)),
            vec![Some("1. ")],
            "缺 suff＝單格視覺兜底"
        );
    }

    /// (g) 縮排量＝`ind_left − ind_hanging`（不低於 0）。
    #[test]
    fn marker_indent_is_left_minus_hanging() {
        let n = numbering(
            r#"<w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:pPr><w:ind w:left="1080" w:hanging="360"/></w:pPr></w:lvl>"#,
        );
        let m = compute_markers(&n, &[r(0, 0)], &[0], 1);
        assert_eq!(m[0].as_ref().unwrap().indent_pt, 36.0);
        let n = numbering(
            r#"<w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:pPr><w:ind w:left="360" w:hanging="720"/></w:pPr></w:lvl>"#,
        );
        let m = compute_markers(&n, &[r(0, 0)], &[0], 1);
        assert_eq!(m[0].as_ref().unwrap().indent_pt, 0.0, "不得為負");
    }

    /// 無 `numPr` 引用 ⇒ 零 marker（零變化守門的純函式面）。
    #[test]
    fn no_refs_means_no_markers() {
        let n = numbering(
            r#"<w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl>"#,
        );
        let m = compute_markers(&n, &[], &[0, 0, 0], 3);
        assert!(m.iter().all(Option::is_none));
    }
}
