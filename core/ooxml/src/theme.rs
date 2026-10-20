//! `word/theme/theme1.xml` 的 clrScheme 查表（DrawingML 主題色槽位）。
//!
//! 背景（ECMA-376 §17.3.2.25 `w:themeColor`）：`<w:color w:val="0000FF"
//! w:themeColor="hyperlink"/>` 有**兩個**顏色來源——`w:val` 只是**回退值**
//! （給不支援主題色的軟體），`w:themeColor` 指向 theme part 的
//! `a:clrScheme` 槽位；Word／LibreOffice 實際渲染時**以主題色為準**。
//! 只吃 `w:val` 會讓渲染色偏離 Word/LO（連結色偏藍正是此因）。
//!
//! 本模組只做「查表」：把 clrScheme 的 12 個槽位
//! （dk1/lt1/dk2/lt2/accent1..6/hlink/folHlink）讀成 0xAARRGGBB。
//! 純讀取：theme1.xml 仍是 passthrough part，**存檔路徑零觸碰**。
//!
//! 亮度變換（lumMod/lumOff/tint/shade/satMod）**本卡不做**——見模組末
//! 「未做清單」。因此帶 `w:themeShade`／`w:themeTint` 的 `w:color` 一律
//! **保留 Word 預算好的 `w:val`**（那正是變換後的色），不得拿「未變換的
//! 主題色」冒充（會渲染出 Word/LO 都沒有的色，比舊行為更差）。

use std::collections::BTreeMap;
use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};

/// theme part 的慣例路徑（OPC 關聯目標 `theme/theme1.xml` 的標準落點）。
const THEME_PART: &str = "word/theme/theme1.xml";

/// theme part 的慣例目錄（異名主題 part 如 theme2.xml 的落點）。
const THEME_DIR: &str = "word/theme/";

/// clrScheme 的 12 個槽位名（DrawingML §20.1.7.6 `a:clrScheme` 的子元素）。
const SLOTS: [&str; 12] = [
    "dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6",
    "hlink", "folHlink",
];

/// 主題色表：槽位名 → 0xAARRGGBB（不透明）。
///
/// 缺 theme part／壞 XML → **空表**（容錯：查不到就老實退回 `w:val` 回退值，
/// 不虛構主題色）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeColors {
    slots: HashMap<&'static str, u32>,
}

impl ThemeColors {
    /// 解析一份 theme part（`word/theme/theme1.xml`）的 clrScheme。
    /// 純讀取；壞 XML＝就地停在已收部分（與 `styles.rs` 的容錯口徑一致）。
    pub fn parse(xml: &[u8]) -> ThemeColors {
        let mut out = ThemeColors::default();
        let mut reader = quick_xml::Reader::from_reader(xml);
        // 在 clrScheme 內？在其內才認槽位（theme part 還有 fontScheme/fmtScheme）。
        let mut in_scheme = false;
        // 目前所在的槽位（進入槽位元素＝Some，離開＝None）。
        let mut cur: Option<&'static str> = None;
        loop {
            match reader.read_event() {
                Err(_) | Ok(Event::Eof) => break,
                Ok(Event::Start(e)) => {
                    let name = e.local_name();
                    if name.as_ref() == b"clrScheme" {
                        in_scheme = true;
                        cur = None;
                        continue;
                    }
                    if !in_scheme {
                        continue;
                    }
                    if let Some(slot) = slot_of(name.as_ref()) {
                        cur = Some(slot);
                        continue;
                    }
                    // 槽位內的顏色元素：<a:srgbClr val="…"/> 或 <a:sysClr …/>。
                    if let (Some(slot), Some(c)) = (cur, color_of(&e)) {
                        out.insert(slot, c);
                    }
                }
                Ok(Event::Empty(e)) => {
                    if !in_scheme {
                        continue;
                    }
                    let name = e.local_name();
                    if slot_of(name.as_ref()).is_some() {
                        // 自閉合槽位（<a:dk1/>）＝無色，不動既有值。
                        cur = None;
                        continue;
                    }
                    if let (Some(slot), Some(c)) = (cur, color_of(&e)) {
                        out.insert(slot, c);
                    }
                }
                Ok(Event::End(e)) => {
                    let name = e.local_name();
                    if name.as_ref() == b"clrScheme" {
                        in_scheme = false;
                        cur = None;
                        continue;
                    }
                    if slot_of(name.as_ref()).is_some() {
                        cur = None;
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// 從整包 parts 找出 theme part 並解析（慣例路徑優先；異名 theme2.xml
    /// 等以 `word/theme/theme*.xml` 的字典序首見者代之）。
    ///
    /// 無 theme part → 空表（呼叫端的老實退回路徑不受影響）。
    pub fn from_parts(parts: &BTreeMap<String, Vec<u8>>) -> ThemeColors {
        let key = if parts.contains_key(THEME_PART) {
            Some(THEME_PART)
        } else {
            parts
                .keys()
                .find(|k| k.starts_with(THEME_DIR) && k.ends_with(".xml"))
                .map(String::as_str)
        };
        match key.and_then(|k| parts.get(k)) {
            Some(bytes) => ThemeColors::parse(bytes),
            None => ThemeColors::default(),
        }
    }

    /// 槽位色（0xAARRGGBB）；查無＝None。
    pub fn slot(&self, name: &str) -> Option<u32> {
        self.slots.get(name).copied()
    }

    /// 由 `w:themeColor`／`w:themeFill` 的屬性值查色：查到槽位 → Some(色)；
    /// 查無（無 theme part／查無該名／`none`）→ None（呼叫端退回 `w:val`）。
    pub fn resolve(&self, token: &str) -> Option<u32> {
        theme_slot_for(token).and_then(|s| self.slot(s))
    }

    /// 有無任何槽位色（空表＝文件無可用主題色）。
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// 寫入槽位色（**首見為準**：同槽位後見者不覆蓋——clrScheme 畸形重複時
    /// 以最先出現者為準，與 styles.xml 的「同 style 內重複宣告＝首見為準」
    /// 同口徑）。
    fn insert(&mut self, slot: &'static str, color: u32) {
        self.slots.entry(slot).or_insert(color);
    }
}

/// `w:themeColor`／`w:themeFill` 的屬性值 → clrScheme 槽位名。
///
/// 對照表（ECMA-376 §17.3.2.25 附註；等同 `w:clrSchemeMapping` 的**預設**
/// 對應：bg1→light1、t1→dark1、bg2→light2、t2→dark2、accent1..6→accent1..6、
/// hyperlink→hyperlink、followedHyperlink→followedHyperlink）：
///
/// | `w:themeColor` | clrScheme 槽位 |
/// |---|---|
/// | `text1`／`t1`／`dark1` | `dk1` |
/// | `background1`／`bg1`／`light1` | `lt1` |
/// | `text2`／`t2`／`dark2` | `dk2` |
/// | `background2`／`bg2`／`light2` | `lt2` |
/// | `accent1`..`accent6` | 同名槽位 |
/// | `hyperlink` | `hlink` |
/// | `followedHyperlink` | `folHlink` |
///
/// `none`（明示「無主題色」）與查無的名 → None。
pub fn theme_slot_for(token: &str) -> Option<&'static str> {
    match token {
        "text1" | "t1" | "dark1" => Some("dk1"),
        "background1" | "bg1" | "light1" => Some("lt1"),
        "text2" | "t2" | "dark2" => Some("dk2"),
        "background2" | "bg2" | "light2" => Some("lt2"),
        "accent1" => Some("accent1"),
        "accent2" => Some("accent2"),
        "accent3" => Some("accent3"),
        "accent4" => Some("accent4"),
        "accent5" => Some("accent5"),
        "accent6" => Some("accent6"),
        "hyperlink" => Some("hlink"),
        "followedHyperlink" => Some("folHlink"),
        _ => None,
    }
}

/// `w:color` 元素上**可直接使用**的主題色（ECMA-376 §17.3.2.25）。
///
/// - 有 `w:themeColor` 且能在 clrScheme 查到 → Some(主題色)；
/// - 帶 `w:themeShade`／`w:themeTint` → None（正確色是「變換後」的主題色，
///   本卡不實作變換——交給呼叫端保留 Word 預算好的 `w:val`）；
/// - 無 theme part／查無該名／`none` → None（呼叫端老實退回 `w:val`）。
pub fn theme_color_of(theme: &ThemeColors, e: &BytesStart) -> Option<u32> {
    if attr(e, "themeShade").is_some() || attr(e, "themeTint").is_some() {
        return None;
    }
    let token = attr(e, "themeColor")?;
    theme.resolve(&token)
}

/// `w:color` 元素的**有效顏色**：主題色優先於 `w:val` 回退值。
///
/// - 主題色可查 → Some(主題色)（**覆蓋** `w:val`——Word/LO 的渲染口徑）；
/// - 查不到 → `w:val` 的 6/8 位 hex（`auto`/非法 → None＝發射層的黑）。
pub fn resolve_color(theme: &ThemeColors, e: &BytesStart) -> Option<u32> {
    if let Some(t) = theme_color_of(theme, e) {
        return Some(t);
    }
    attr(e, "val").as_deref().and_then(crate::parse_color_val)
}

/// clrScheme 槽位名 → 槽位（查無＝None）。
fn slot_of(name: &[u8]) -> Option<&'static str> {
    let s = std::str::from_utf8(name).ok()?;
    SLOTS.iter().copied().find(|slot| *slot == s)
}

/// 槽位內的顏色元素 → 0xAARRGGBB：
/// - `<a:srgbClr val="RRGGBB"/>`：取 `val`；
/// - `<a:sysClr val="windowText" lastClr="000000"/>`：取 `lastClr`
///   （系統色的實際呈現值；`val` 是系統色名如 windowText，不可當 hex 用）。
fn color_of(e: &BytesStart) -> Option<u32> {
    let name = e.local_name();
    if name.as_ref() == b"srgbClr" {
        attr(e, "val").as_deref().and_then(crate::parse_color_val)
    } else if name.as_ref() == b"sysClr" {
        attr(e, "lastClr")
            .as_deref()
            .and_then(crate::parse_color_val)
            .or_else(|| {
                // lastClr 缺（畸形）→ 退回 val（部分檔案直接寫 hex）。
                attr(e, "val").as_deref().and_then(crate::parse_color_val)
            })
    } else {
        None
    }
}

/// 屬性取值（只認 local name——OOXML 屬性前綴 `w:` 不影響比對）。
fn attr(e: &BytesStart, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key.as_bytes() {
            a.unescape_value().ok().map(|v| v.to_string())
        } else {
            None
        }
    })
}

// ---- 未做清單（誠實 TODO，本卡刻意不實作） ----
//
// - **亮度變換**（lumMod/lumOff/tint/shade/satMod）：DrawingML 的
//   `<a:lumMod>/<a:lumOff>/<a:tint>/<a:shade>` 與 WordprocessingML 的
//   `w:themeShade`／`w:themeTint`。未實作前，帶變換的 `w:color` 一律保留
//   Word 預算好的 `w:val`（＝變換後的真實色），不用未變換色冒充。
// - **`w:clrSchemeMapping`**（settings.xml）：可把 bg1/t1/… 改指到別的槽位；
//   本卡只認預設對應（`theme_slot_for` 的表），非預設對應＝按預設近似。
// - **`w:themeFill`／`w:themeFillShade`／`w:themeFillTint`**（底紋用）：
//   槽位對應已備（`theme_slot_for` 同表），但底紋層（`table.rs` 的 `w:shd`）
//   尚未接上。
// - **合成預設連結色**（`RunStyle::with_hyperlink_default` 的 #0563C1）：
//   Word 的 Hyperlink 樣式色其實來自主題 hlink；本卡未改（維持既有行為）。
// - **theme part 的關聯解析**：本卡按慣例路徑 `word/theme/theme1.xml` 找（找不到
//   則取 `word/theme/theme*.xml` 的字典序首見者），**未**走
//   `word/_rels/document.xml.rels` 的 theme 關係——主題 part 放在 `word/theme/`
//   以外且檔名不同（如 `word/theme/theme9.xml` 之外的異名路徑）時會找不到 →
//   空表＝老實退回 `w:val`（不虛構主題色）。

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小 clrScheme：srgbClr ＋ sysClr（lastClr）各一，驗證 12 槽位解析。
    const THEME: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <a:themeElements>
    <a:clrScheme name="Office">
      <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
      <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
      <a:dk2><a:srgbClr val="1F497D"/></a:dk2>
      <a:lt2><a:srgbClr val="EEECE1"/></a:lt2>
      <a:accent1><a:srgbClr val="4F81BD"/></a:accent1>
      <a:accent2><a:srgbClr val="C0504D"/></a:accent2>
      <a:accent3><a:srgbClr val="9BBB59"/></a:accent3>
      <a:accent4><a:srgbClr val="8064A2"/></a:accent4>
      <a:accent5><a:srgbClr val="4BACC6"/></a:accent5>
      <a:accent6><a:srgbClr val="F79646"/></a:accent6>
      <a:hlink><a:srgbClr val="305090"/></a:hlink>
      <a:folHlink><a:srgbClr val="800080"/></a:folHlink>
    </a:clrScheme>
  </a:themeElements>
</a:theme>"#;

    /// 12 槽位全解析（srgbClr 取 val、sysClr 取 lastClr）。
    #[test]
    fn parses_all_clr_scheme_slots() {
        let t = ThemeColors::parse(THEME);
        assert_eq!(t.slot("dk1"), Some(0xFF00_0000));
        assert_eq!(t.slot("lt1"), Some(0xFFFF_FFFF));
        assert_eq!(t.slot("dk2"), Some(0xFF1F_497D));
        assert_eq!(t.slot("lt2"), Some(0xFFEE_ECE1));
        assert_eq!(t.slot("accent1"), Some(0xFF4F_81BD));
        assert_eq!(t.slot("accent2"), Some(0xFFC0_504D));
        assert_eq!(t.slot("accent3"), Some(0xFF9B_BB59));
        assert_eq!(t.slot("accent4"), Some(0xFF80_64A2));
        assert_eq!(t.slot("accent5"), Some(0xFF4B_ACC6));
        assert_eq!(t.slot("accent6"), Some(0xFFF7_9646));
        assert_eq!(t.slot("hlink"), Some(0xFF30_5090));
        assert_eq!(t.slot("folHlink"), Some(0xFF80_0080));
    }

    /// sysClr 取 `lastClr`（`val` 是系統色名如 windowText，不是 hex）。
    #[test]
    fn sys_clr_uses_last_clr() {
        let t = ThemeColors::parse(THEME);
        assert_eq!(
            t.slot("dk1"),
            Some(0xFF00_0000),
            "sysClr 的 val=windowText 不可當 hex；取 lastClr=000000"
        );
        assert_eq!(t.slot("lt1"), Some(0xFFFF_FFFF));
    }

    /// sysClr 缺 lastClr（畸形）→ 退回 val（部分檔案直接寫 hex）。
    #[test]
    fn sys_clr_without_last_clr_falls_back_to_val() {
        let xml = br#"<a:theme xmlns:a="x"><a:themeElements><a:clrScheme>
            <a:dk1><a:sysClr val="123456"/></a:dk1></a:clrScheme></a:themeElements></a:theme>"#;
        let t = ThemeColors::parse(xml);
        assert_eq!(t.slot("dk1"), Some(0xFF12_3456));
    }

    /// 壞 XML／無 clrScheme → 空表（容錯：呼叫端退回 `w:val`）。
    #[test]
    fn missing_scheme_yields_empty_table() {
        assert!(ThemeColors::parse(b"").is_empty());
        assert!(ThemeColors::parse(b"<a:theme").is_empty());
        let no_scheme = br#"<a:theme xmlns:a="x"><a:themeElements/></a:theme>"#;
        assert!(ThemeColors::parse(no_scheme).is_empty());
    }

    /// 主題色名 → clrScheme 槽位（ECMA-376 §17.3.2.25 附註對照表）。
    #[test]
    fn theme_token_maps_to_slot() {
        assert_eq!(theme_slot_for("text1"), Some("dk1"));
        assert_eq!(theme_slot_for("bg1"), Some("lt1"));
        assert_eq!(theme_slot_for("text2"), Some("dk2"));
        assert_eq!(theme_slot_for("bg2"), Some("lt2"));
        assert_eq!(theme_slot_for("accent3"), Some("accent3"));
        assert_eq!(theme_slot_for("hyperlink"), Some("hlink"));
        assert_eq!(theme_slot_for("followedHyperlink"), Some("folHlink"));
        // 查無的名與「無主題色」→ None（呼叫端退回 w:val）。
        assert_eq!(theme_slot_for("accent9"), None);
        assert_eq!(theme_slot_for("none"), None);
        assert_eq!(theme_slot_for(""), None);
    }

    /// 依 token 查色：hyperlink → hlink 槽位色。
    #[test]
    fn resolve_by_token() {
        let t = ThemeColors::parse(THEME);
        assert_eq!(t.resolve("hyperlink"), Some(0xFF30_5090));
        assert_eq!(t.resolve("followedHyperlink"), Some(0xFF80_0080));
        assert_eq!(t.resolve("accent1"), Some(0xFF4F_81BD));
        assert_eq!(t.resolve("accent9"), None, "查無該名 → None（退回 w:val）");
    }
}
