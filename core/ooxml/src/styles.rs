//! `word/styles.xml` 的最小可用樣式級聯（ECMA-376 §17.7.2 子集；t_5dfc8ac7）。
//!
//! 定位：**級聯引擎的最小可行切片**，不是完整 Word 樣式引擎。套用序（後者
//! 覆蓋前者的「已宣告」屬性，未宣告＝不覆蓋）：
//!
//! ```text
//! docDefaults(w:rPrDefault ∪ w:pPrDefault) → style 的 w:basedOn 鏈（根先子後）
//!   → 段落直接格式 w:pPr（含其內 w:rPr）→ run 直接格式 w:rPr（含 w:rStyle 字元樣式）
//! ```
//!
//! 本卡支援面（其餘屬性見 [`StyleTable`] 頭注「未支援清單」）：
//! - `w:sz/@w:val`：半磅→pt（ECMA-376 §17.3.2.29）；
//! - `w:spacing/@w:line`＋`@w:lineRule`：`auto`→行距倍數（240＝單倍，
//!   §17.4.1.3）；`exact`/`atLeast`→固定 pt（twips→pt）；
//! - `w:b`／`w:i`／`w:u`／`w:strike`／`w:color`：值語義與 `lib.rs` 的
//!   `toggle_attr_on`／`parse_color_val` 逐字一致（none/false/0/off＝關閉）；
//! - `w:rFonts`（ECMA-376 §17.3.2.26）：字型族宣告（ascii/eastAsia/cs＋
//!   hint）與 theme 引用（asciiTheme/hAnsiTheme/eastAsiaTheme → fontScheme
//!   槽位；`cstheme` 不建模）；theme 引用在**解析當下**即查表落地成直接值，
//!   級聯系統裡不存在未解析的 theme 槽；
//! - `w:jc/@w:val`：解析並保留（發射層本卡不消費——顯示列表以版心左緣起排）。
//!
//! `w:pStyle` 引用的是**樣式 id**（`w:styleId`），不是 `w:name`：查表以 id 為
//! 鍵；段落未宣告 `w:pStyle` 時落 `w:default="1"` 的段落樣式（通常是 Normal，
//! ECMA-376 §17.7.2.3——Normal 即全文件段落基底）。查無該 id＝退回預設段落
//! 樣式，再查無＝只剩 docDefaults（畸形檔容錯，不虛構）。
//!
//! **樣式型別守衛**（`w:style/@w:type`，ECMA-376 §17.7.2.4）：`w:pStyle` 只能
//! 引用段落型樣式（§17.7.2.32）、`w:rStyle` 只能引用字元型樣式
//! （§17.7.2.29）。畸形檔（真實世界常見）把字元型 id 塞進 `w:pStyle` 時，
//! Word 依 §17.7.2.32 忽略該引用——本卡同口徑：型別不符者**不套用**（pStyle
//! 側落回預設段落樣式；rStyle 側整個跳過），且 `w:basedOn` 鏈（§17.7.2.3
//! 須同型）遇到跨型祖先即**斷鏈**（不往上撈）。不驗證型別會讓字元樣式的
//! rPr/pPr 併進段落級（或反之），污染字級/行距/對齊。
//!
//! `w:basedOn` 環／超鏈（畸形檔）：遞迴深度上限 [`MAX_CHAIN`] 層即停，老實
//! 停在已合成結果（與 `table_map::table_style_borders` 同款容錯策略）。合成
//! 在 [`StyleTable::parse`] 結束時一次性完成，之後全部查詢皆 `&self`——
//! 引擎層可在發射路徑直接使用。

use std::collections::HashMap;

use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

use crate::theme::{self, ThemeColors, ThemeFonts};

/// basedOn 鏈深度上限（超鏈＝畸形檔，老實停在已合成結果，見模組頭）。
const MAX_CHAIN: usize = 32;

/// `w:rFonts`（ECMA-376 §17.3.2.26）：字型族宣告。各欄 None＝未宣告（繼承）。
///
/// 三個字型族槽各自分立宣告、各自分立繼承——**逐欄**級聯，不是整包覆蓋
/// （下層只宣告 `w:ascii` 時，`w:eastAsia` 仍落到上層的值）。
///
/// `hint_east_asian` 是 `w:hint="eastAsia"` 的布林投影（ECMA-376
/// §17.3.2.26：`w:hint` 指示「歧義字元優先用哪一族的字型」）。此欄是純
/// bool（無法表達「本層明確宣告非 eastAsia」），故級聯採**或**語義：任一
/// 層宣告過 eastAsia 提示即為真。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFonts {
    /// `w:rFonts/@w:ascii`：西文（ASCII）字型族名。theme 引用
    /// （`@w:asciiTheme`／`@w:hAnsiTheme`）在解析當下即查表落地成此欄的
    /// 直接值——級聯系統裡不存在未解析的 theme 槽。
    #[serde(default)]
    pub ascii: Option<String>,
    /// `w:rFonts/@w:eastAsia`：東亞字型族名。theme 引用
    /// （`@w:eastAsiaTheme`）同 [`RunFonts::ascii`] 在解析當下落地。
    #[serde(default)]
    pub east_asia: Option<String>,
    /// `w:rFonts/@w:cs`：複雜文字（complex script）字型族名。
    #[serde(default)]
    pub cs: Option<String>,
    /// `w:rFonts/@w:hint` 是否為 `eastAsia`。
    #[serde(default)]
    pub hint_east_asian: bool,
    /// K6：theme 字型引用**斷鏈**（有 `w:*Theme` 但 theme part 缺席／槽位
    /// 解析不到，且無同元素直接值可回退）。LO 對這種檔案會落到自身替換
    /// 預設 Noto Serif；此旗標讓行距查表能走 1.3620 的 fallback 列。
    /// 純派生欄位，serde default 保舊快照相容。
    #[serde(default)]
    pub theme_unresolved: bool,
}

impl RunFonts {
    /// 逐欄級聯（ECMA-376 §17.3.2.26）：本層（較具體者）已宣告者贏，缺者
    /// 取上層。`hint_east_asian` 採或語義（見欄位注）。
    pub(crate) fn overlay(mut self, upper: &RunFonts) -> RunFonts {
        if self.ascii.is_none() {
            self.ascii = upper.ascii.clone();
        }
        if self.east_asia.is_none() {
            self.east_asia = upper.east_asia.clone();
        }
        if self.cs.is_none() {
            self.cs = upper.cs.clone();
        }
        self.hint_east_asian |= upper.hint_east_asian;
        self.theme_unresolved |= upper.theme_unresolved;
        self
    }
}

/// run 級有效樣式（級聯合成後；`None`＝該層完全未宣告，交下層／發射層兜底）。
/// serde derives：段落直接格式（`w:pPr/w:rPr`）以本型別存進文檔模型（lib.rs
/// `Paragraph::prun`）供 engine 級聯消費；`color_set` 帶 default 保舊 JSON 兼容。
///
/// 不 derive `Copy`：`fonts` 欄承載 `String` 字型族名（不可 Copy）。既有欄位
/// 全為 Copy 型，語義不變；改用 `Clone` 複製的呼叫點皆在本檔與 engine-api 內。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunStyle {
    /// 字級（pt）。半磅值÷2；缺＝None（呼叫端退回 12pt 後備）。
    pub size_pt: Option<f32>,
    /// 粗體。三態是級聯正確性的前提：`Some(false)`＝本層明確宣告關閉（可
    /// 熄滅上層的開啟，如 Word「Normal 粗體＋直接格式 b=0」）；`None`＝繼承。
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    /// 文字顏色（0xAARRGGBB）。`color_set` 區分「未宣告」（inherit）與
    /// 「宣告 auto/非法」（明確黑）——後者不得誤當前者繼承樣式色。
    pub color: Option<u32>,
    /// 本層是否宣告過 `w:color`（含 auto→`color=None` 的情形）。
    #[serde(default)]
    pub color_set: bool,
    /// 字型族宣告（`w:rFonts`；ECMA-376 §17.3.2.26）。`None`＝本層未宣告
    /// 任何字型（整包繼承上層）。逐欄繼承見 [`RunFonts`]。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fonts: Option<RunFonts>,
}

impl RunStyle {
    /// 超連結合成預設的**級聯末端兜底**（ECMA-376 §17.7.2：直接格式＞
    /// 字元樣式鏈（含 Hyperlink 樣式）＞段落樣式＞docDefaults）。解析期不再
    /// 把預設藍寫進 `Run::direct`——那會讓合成色冒充直接格式、越過用戶
    /// styles.xml 宣告的 Hyperlink 字元樣式。只有整條鏈都沒宣告顏色時，才在
    /// 此補上 Word Hyperlink 標準藍（`crate::HYPERLINK_DEFAULT_COLOR_ARGB`）；
    /// 底線同理：鏈上已宣告 `w:u`（含 `none`＝明確關閉）即不再合成。
    /// `hyperlink`＝本 run 位於 `w:hyperlink` 內（解析期 `Run::hyperlink_default`）。
    ///
    /// 呼叫端必須以「跑完 [`StyleTable::resolve_run`] 的最終結果」傳入，
    /// 否則 `color_set`／`underline` 未反映全鏈宣告，兜底會誤蓋樣式色。
    pub fn with_hyperlink_default(mut self, hyperlink: bool) -> RunStyle {
        if hyperlink {
            if !self.color_set {
                self.color = Some(crate::HYPERLINK_DEFAULT_COLOR_ARGB);
                self.color_set = true;
            }
            if self.underline.is_none() {
                self.underline = Some(true);
            }
        }
        self
    }

    /// 疊加上層：`upper` 只補本層未宣告的欄位——本層已宣告者贏。
    /// （級聯組裝自根往下：`child.overlay(parent)` 型別上即「子補父缺」。）
    pub(crate) fn overlay(mut self, upper: &RunStyle) -> RunStyle {
        macro_rules! fill {
            ($($f:ident),* $(,)?) => {
                $(if self.$f.is_none() { self.$f = upper.$f; })*
            };
        }
        fill!(size_pt, bold, italic, underline, strike);
        if !self.color_set {
            self.color = upper.color;
            self.color_set = upper.color_set;
        }
        // 字型族逐欄繼承（ECMA-376 §17.3.2.26）：本層未宣告的族別取上層，
        // 已宣告者贏——不是整包覆蓋（只宣告 ascii 時 eastAsia 仍繼承）。
        self.fonts = match (self.fonts.take(), upper.fonts.as_ref()) {
            (None, None) => None,
            (None, Some(u)) => Some(u.clone()),
            (Some(lower), None) => Some(lower),
            (Some(lower), Some(u)) => Some(lower.overlay(u)),
        };
        self
    }
}

/// 段落對齊（`w:jc`）——本卡解析並保留，發射層暫不消費（置中/靠右屬排版卡）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Align {
    Left,
    Center,
    Right,
    Both,
    Distribute,
}

/// 行距（`w:spacing/@w:line`＋`@w:lineRule` 的合成結果）。三態忠實保留
/// （ECMA-376 §17.4.1.3），「怎麼變成 pt」由排版端決定：
/// - `Auto(f)`：倍數（240＝單倍）——本引擎近似為「字級×倍數」（見 engine-api
///   `resolve_line_h_pt` 注），與既有 fallback（12pt×1.4）同口徑；
/// - `Exact(v)`：固定 v pt（twips 已換算），文字超出亦不撐高；
/// - `AtLeast(v)`：v 與該字級 fallback 行高取大（本引擎行高無內容自適應成分）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LineSpacing {
    Auto(f32),
    Exact(f32),
    AtLeast(f32),
}

impl Default for LineSpacing {
    fn default() -> Self {
        // 對齊既有硬編碼後備（`LayoutParams::default()` 的 1.4 倍行距）：
        // 文件毫無宣告時輸出逐位元組不變。
        LineSpacing::Auto(1.4)
    }
}

/// 段落級有效樣式（級聯合成的段落部分）。
///
/// 不 derive `Copy`：內含 [`RunStyle`]（其 `fonts` 欄承載 `String`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParaStyle {
    /// 段落基底 run 樣式：docDefaults∪樣式鏈∪段落 pPr/rPr 的合成結果
    /// （run 直接格式／字元樣式再疊於其上）。
    pub run: RunStyle,
    pub line: Option<LineSpacing>,
    /// 段前距（pt，`w:spacing/@w:before`；未宣告＝None——零變化守門的依據，
    /// 不得以 `Some(0)` 冒充）。本卡由 layout 量化成段前帶消費。
    pub spacing_before_pt: Option<f32>,
    /// 段後距（pt，`w:spacing/@w:after`；未宣告＝None，同上）。
    pub spacing_after_pt: Option<f32>,
    /// 對齊（本卡僅保留；未消費）。
    pub align: Option<Align>,
}

/// 一張 `w:style` 的自帶宣告（未沿鏈合成前）。
#[derive(Debug, Clone, Default)]
struct OwnStyle {
    /// 主 `w:rPr`（ECMA §17.7.2.3 style run properties）——段內文字的唯一
    /// 風格來源。與 `ppr_run` 分欄存放（t_16a13211 R3）：合成時固定
    /// `run.overlay(&ppr_run)`，優先序是語義、不是 XML 出現序。
    run: RunStyle,
    /// `w:pPr/w:rPr`（段落標記 run 屬性）：對段內 run 只補主 rPr 未宣告欄。
    ppr_run: RunStyle,
    line: Option<LineSpacing>,
    spacing_before_pt: Option<f32>,
    spacing_after_pt: Option<f32>,
    align: Option<Align>,
    based_on: Option<String>,
    /// `w:link`（字元樣式配對，UI/格式刷語義）：本卡只收不用。
    #[allow(dead_code)]
    link: Option<String>,
    kind: StyleKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum StyleKind {
    Paragraph,
    Character,
    /// 非段落/字元樣式（table/numbering）：本卡不收（`Other` 樣式不入表）。
    #[default]
    Other,
}

/// 已級聯合成的樣式表（開啟文件時一次性解析；純讀取派生資料——
/// 存檔路徑完全不經本層，styles.xml 照舊 passthrough）。
///
/// 未支援清單（ECMA-376 §17.7 廣度所限，本卡刻意不實作、非漏實作）：
/// `w:szCs`（複雜文字字級）、`w:ind`（縮排）、`w:spacing` 的 before/after
/// （段前後距——行帶模型無段落間距概念）、`w:kern`、`w:shd`（底紋）、
/// `w:vertAlign`（上下標）、`w:numPr`（編號）、`w:tabs`、`w:widowControl`、
/// `w:bidi`、`w:suppress*`、`w:smallCaps`、`w:tblStylePr`（條件表格樣式，
/// 另見 `table_map`）。
///
/// `w:rFonts`（字型族）已解析存檔（見 [`RunFonts`]），但**選族**（哪一族
/// 真的拿去排版）不在解析層——渲染層本卡零改動。
#[derive(Debug, Clone, Default)]
pub struct StyleTable {
    /// docDefaults 的 `w:rPrDefault/w:rPr`（全文件 run 基底）。
    run_default: RunStyle,
    /// docDefaults 的 `w:pPrDefault/w:pPr`（段落欄位）＋其內 `w:rPr`。
    para_default_line: Option<LineSpacing>,
    para_default_spacing_before_pt: Option<f32>,
    para_default_spacing_after_pt: Option<f32>,
    para_default_align: Option<Align>,
    para_default_run: RunStyle,
    /// styleId → 自帶宣告。
    styles: HashMap<String, OwnStyle>,
    /// K6：表格樣式（`w:type="table"`）的自帶段落宣告。原本整類丟棄；
    /// 校準報告 §3(2) 證實表格樣式的 `w:spacing/@w:line` 會壓過 docDefaults
    /// （fdo68607 的 240 vs 276），故單獨收表、查詢時沿 basedOn 鏈取第一個
    /// 已宣告值（不併進段落樣式表，避免型別污染）。
    table_styles: HashMap<String, OwnStyle>,
    /// styleId → 合成後（parse 結束一次性沿鏈合成；查詢期不可變）。
    resolved: HashMap<String, (RunStyle, ParaStyle)>,
    /// styleId → **只沿 basedOn 鏈**合成的 run 宣告（不含 docDefaults）。
    /// 字元樣式（`w:rStyle`）疊在段落級結果之上時必須用本表：ECMA-376
    /// §17.7.2 的解析序是「run 直接 → 字元樣式鏈 → 段落樣式鏈 → docDefaults」，
    /// 若把 docDefaults 先烘進字元樣式，它會越過段落樣式的明確宣告搶先命中。
    chain: HashMap<String, RunStyle>,
    /// `w:default="1"` 的段落樣式 id（通常 Normal）；段落未宣告 pStyle 時用。
    default_para: Option<String>,
    /// 有無任何宣告（空表＝呼叫端全走後備值）。
    empty: bool,
    /// resolve 遞迴拜訪鏈（環／超深偵測；parse 期一次性使用）。
    visiting: Vec<String>,
}

/// OOXML 整數值域上限（病態檔守衛）。判據與 `drawing.rs` 的 `MAX_OOXML_LONG`
/// 同源（該檔屬 t_c1fb4c69 熱區，本卡不動它，此處以同值常數鏡像）。
/// 超出＝畸形宣告，老實拒收（不猜、不把天文數字帶進斷行/分頁算術）。
/// `pub(crate)`：`lib.rs` 的 `w:tabs/w:tab/@w:pos` 數值守衛共用同一口徑
/// （t_16a13211 R2）。
pub(crate) const MAX_OOXML_LONG: f64 = i64::MAX as f64;

/// `w:sz/@w:val`（半磅）→ pt。非法/缺＝None（容錯鐵律：不猜）。
/// 畸形超大值（`parse::<f64>` 可吐出 inf／天文數字）一律拒收：舊實作只濾
/// `>0.0`，`"1e400"`→inf、`"4000000000000"`→2e12pt 都能進 size_pt，經
/// `resolve_line_h_pt`（只 clamp 下界）放大成段行高天文數，再在分頁佔帶
/// 算術餵飽 u32（t_5b391cfd S2）。
fn half_point_pt(raw: Option<&str>) -> Option<f32> {
    raw.and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0 && *v <= MAX_OOXML_LONG)
        .map(|v| (v / 2.0) as f32)
}

/// `w:spacing/@w:line`＋`@w:lineRule` → [`LineSpacing`]。line 非法＝None。
/// 超大/inf 值同 [`half_point_pt`] 一律拒收（t_5b391cfd S2）。
/// `pub(crate)`：document.xml 的直接格式 `w:pPr/w:spacing` 共用同一判據。
pub(crate) fn line_spacing(line: Option<&str>, rule: Option<&str>) -> Option<LineSpacing> {
    let v = line
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0 && *v <= MAX_OOXML_LONG)?;
    let rule = rule.unwrap_or("auto");
    Some(if rule.eq_ignore_ascii_case("exact") {
        // twips → pt。
        LineSpacing::Exact((v / 20.0) as f32)
    } else if rule.eq_ignore_ascii_case("atLeast") {
        LineSpacing::AtLeast((v / 20.0) as f32)
    } else {
        // auto：240 = 單倍。
        LineSpacing::Auto((v / 240.0) as f32)
    })
}

/// `w:spacing/@w:before`／`@w:after`（twips）→ pt。未宣告＝None；非法／
/// 負值／超大值同 [`half_point_pt`] 拒收（容錯鐵律：不猜）。**0 是合法
/// 宣告**（`Some(0.0)`），與未宣告的 `None` 區分——呼叫端據此判斷有無
/// 段落距宣告（零變化守門）。
pub(crate) fn space_pt(raw: Option<&str>) -> Option<f32> {
    raw.and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0 && *v <= MAX_OOXML_LONG)
        .map(|v| (v / 20.0) as f32)
}

pub(crate) fn align_of(val: Option<&str>) -> Option<Align> {
    match val?.trim() {
        "left" | "start" => Some(Align::Left),
        "center" => Some(Align::Center),
        "right" | "end" => Some(Align::Right),
        "both" => Some(Align::Both),
        "distribute" | "thaiDistribute" => Some(Align::Distribute),
        _ => None,
    }
}

/// 開關值（`w:b`/`w:i`/`w:u`/`w:strike`）：語義直接引用 `lib.rs::toggle_attr_on`
/// ——同一份語義只許有一份（無 w:val＝開啟；none/false/0/off＝關閉）。
fn toggle_on_attr(e: &quick_xml::events::BytesStart) -> bool {
    crate::toggle_attr_on(e)
}

/// 套用單個 run 屬性標籤（Start/Empty 共用）。未支援標籤靜默略過（模組頭清單）。
/// `pub(crate)`：document.xml 的直接格式 `w:pPr/w:rPr` 共用同一判據。
///
/// `theme`＝文件的主題色表（`w:themeColor` → theme1.xml 的 clrScheme 查表；
/// ECMA-376 §17.3.2.25——**主題色優先於 `w:val` 回退值**）。空表＝無 theme
/// part＝老實退回 `w:val`（既有行為）。
pub(crate) fn apply_run_tag(
    name: &[u8],
    e: &quick_xml::events::BytesStart,
    rs: &mut RunStyle,
    theme: &ThemeColors,
    tf: Option<&ThemeFonts>,
) {
    match name {
        b"b" => rs.bold = Some(toggle_on_attr(e)),
        b"i" => rs.italic = Some(toggle_on_attr(e)),
        b"u" => rs.underline = Some(toggle_on_attr(e)),
        b"strike" => rs.strike = Some(toggle_on_attr(e)),
        b"sz" => {
            if let Some(v) = half_point_pt(attr_val(e, "val").as_deref()) {
                rs.size_pt = Some(v);
            }
        }
        b"color" => {
            // 色彩宣告（ECMA-376 §17.3.2.25）：`w:themeColor` 指向 theme1.xml 的
            // clrScheme 槽位，**優先於** `w:val` 回退值（Word/LO 實際渲染口徑）；
            // 查不到（無 theme part／查無該名）→ 老實退回 `w:val`。
            // 帶 `w:themeShade`／`w:themeTint` 時正確色是「變換後」的主題色，
            // 本卡不實作變換 → 保留 Word 預算好的 `w:val`（見 theme 模組 TODO）。
            // 宣告 auto/非法 → 明確的「自動色」（黑）：color_set 置真但 color
            // 留空，不得當「未宣告」而誤繼承樣式色。
            let theme_color = theme::theme_color_of(theme, e);
            if let Some(t) = theme_color {
                rs.color_set = true;
                rs.color = Some(t);
            } else if let Some(v) = attr_val(e, "val") {
                rs.color_set = true;
                rs.color = crate::parse_color_val(&v);
            }
            // 只宣告了查不到的 `w:themeColor`（無 theme part／查無該名）→
            // 維持「未宣告」語義（繼承上層色），不虛構黑。
        }
        b"rFonts" => {
            // 字型族宣告（ECMA-376 §17.3.2.26）：只抓屬性（不讀文字）。
            // 四欄皆無宣告者（空元素）不動 rs——不得憑空物化一個空宣告。
            // 同 rPr 內重複宣告＝首見為準（與 styles.xml 樣式層同口徑）。
            if let Some(f) = rfonts_of(e, tf) {
                if rs.fonts.is_none() {
                    rs.fonts = Some(f);
                }
            }
        }
        _ => {}
    }
}

/// `w:rFonts` 元素 → 字型族宣告（ECMA-376 §17.3.2.26）。
///
/// - `w:ascii`／`w:eastAsia`／`w:cs` → 對應族別（空字串視為未宣告）；
/// - `w:asciiTheme`／`w:hAnsiTheme`／`w:eastAsiaTheme` → **在同一元素內**
///   經 fontScheme 查表落地成直接值：同元素同時有 `w:ascii` 與
///   `w:asciiTheme` 時主題取勝（`w:ascii` 是不支援主題字型軟體的回退值）；
///   查不到（無 theme part／未宣告／`@typeface` 空白）＝保留直接值；
///   兩者皆無＝該欄 None。`w:cstheme`（Bidi 系）不建模；
/// - `w:hint="eastAsia"` → `hint_east_asian` 為真（其餘值視為非 eastAsia）。
///
/// 四欄皆無宣告（空元素／無關屬性）→ `None`：呼叫端據此**不得**把
/// 「未見任何 rPr 宣告」的 `None` 語義破壞成 `Some(空宣告)`。
///
/// 重點：查表在**解析當下**完成，級聯系統裡不再存在未解析的 theme 槽。
/// 跨層就是普通逐欄級聯——下層直接值自然贏上層已落地的 theme 值。
pub(crate) fn rfonts_of(
    e: &quick_xml::events::BytesStart,
    tf: Option<&ThemeFonts>,
) -> Option<RunFonts> {
    let ascii = attr_val(e, "ascii").filter(|v| !v.trim().is_empty());
    let east_asia = attr_val(e, "eastAsia").filter(|v| !v.trim().is_empty());
    let cs = attr_val(e, "cs").filter(|v| !v.trim().is_empty());
    let hint = attr_val(e, "hint").is_some_and(|v| v.eq_ignore_ascii_case("eastAsia"));
    // theme 引用（ECMA-376 §17.3.2.26）：`w:asciiTheme` 與 `w:hAnsiTheme`
    // 指向同一個 HAnsi（latin）槽位——Word 通常只寫其中一個，兩者同現時
    // 以 `w:asciiTheme` 為準（先到先贏，不讓第二個覆蓋）。無法識別的值
    // → 老實 None，不虛構槽位。
    let ascii_theme = attr_val(e, "asciiTheme")
        .and_then(|v| theme::ascii_theme_slot(&v))
        .or_else(|| attr_val(e, "hAnsiTheme").and_then(|v| theme::ascii_theme_slot(&v)));
    let east_asia_theme =
        attr_val(e, "eastAsiaTheme").and_then(|v| theme::east_asia_theme_slot(&v));
    // 同層 theme 取勝：查表成功者覆蓋直接值；查不到則保留直接值
    // （回退語義）；查不到且直接值也沒有 → 該欄維持 None。
    let ascii_resolved = ascii_theme
        .and_then(|slot| tf.and_then(|t| t.resolve_slot(slot)))
        .map(str::to_string);
    let east_asia_resolved = east_asia_theme
        .and_then(|slot| tf.and_then(|t| t.resolve_slot(slot)))
        .map(str::to_string);
    // K6：theme 斷鏈＝有 theme 引用但查不到槽位，且該欄沒有直接值可回退。
    // 這正是 LO 落到 Noto Serif 的族群（報告 §1 的 fallback 列）。
    let theme_unresolved = (ascii_theme.is_some() && ascii_resolved.is_none() && ascii.is_none())
        || (east_asia_theme.is_some() && east_asia_resolved.is_none() && east_asia.is_none());
    let ascii = ascii_resolved.or(ascii);
    let east_asia = east_asia_resolved.or(east_asia);
    if ascii.is_none() && east_asia.is_none() && cs.is_none() && !hint && !theme_unresolved {
        return None;
    }
    Some(RunFonts {
        ascii,
        east_asia,
        cs,
        hint_east_asian: hint,
        theme_unresolved,
    })
}

/// 讀取 `w:rPr` 的**內容**（呼叫端已消耗 `<w:rPr>` Start；本函數消費至
/// `</w:rPr>`）。未支援標籤靜默略過（模組頭清單）。
fn parse_rpr_body(
    reader: &mut quick_xml::Reader<&[u8]>,
    theme: &ThemeColors,
    tf: Option<&ThemeFonts>,
) -> RunStyle {
    let mut rs = RunStyle::default();
    loop {
        match reader.read_event() {
            Err(_) | Ok(Event::Eof) => break,
            Ok(Event::End(e)) if e.local_name().as_ref() == b"rPr" => break,
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                apply_run_tag(e.local_name().as_ref(), &e, &mut rs, theme, tf);
            }
            _ => {}
        }
    }
    rs
}

/// 讀取 `w:pPr` 子樹（呼叫端已消耗 pPr 的 Start；其內 `w:rPr` 併入段落
/// run 基底回傳）。本卡只收 spacing(line)/jc/rPr，其餘段落屬性略過。
fn parse_ppr(
    reader: &mut quick_xml::Reader<&[u8]>,
    theme: &ThemeColors,
    tf: Option<&ThemeFonts>,
) -> (
    Option<LineSpacing>,
    Option<f32>,
    Option<f32>,
    Option<Align>,
    RunStyle,
) {
    let mut line = None;
    let mut before = None;
    let mut after = None;
    let mut align = None;
    let mut run = RunStyle::default();
    let mut in_rpr = false;
    loop {
        match reader.read_event() {
            Err(_) | Ok(Event::Eof) => break,
            Ok(Event::End(e)) if e.local_name().as_ref() == b"pPr" => break,
            Ok(Event::End(e)) => {
                // rPr 結束即出段內 run 區（其後的 spacing/jc 仍是段落欄位）。
                if e.local_name().as_ref() == b"rPr" {
                    in_rpr = false;
                }
            }
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if in_rpr {
                    apply_run_tag(&name, &e, &mut run, theme, tf);
                    continue;
                }
                match name.as_slice() {
                    b"spacing" => {
                        if line.is_none() {
                            line = line_spacing(
                                attr_val(&e, "line").as_deref(),
                                attr_val(&e, "lineRule").as_deref(),
                            );
                        }
                        if before.is_none() {
                            before = space_pt(attr_val(&e, "before").as_deref());
                        }
                        if after.is_none() {
                            after = space_pt(attr_val(&e, "after").as_deref());
                        }
                    }
                    b"jc" if align.is_none() => {
                        align = align_of(attr_val(&e, "val").as_deref());
                    }
                    b"rPr" => in_rpr = true,
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                // 自閉合標籤：`<w:rPr/>` 不進入 run 區（否則會吞掉其後的
                // spacing/jc——行距靜默丟失是本級聯層最壞的失敗模式）。
                let name = e.local_name().as_ref().to_vec();
                if in_rpr {
                    apply_run_tag(&name, &e, &mut run, theme, tf);
                    continue;
                }
                match name.as_slice() {
                    b"spacing" => {
                        if line.is_none() {
                            line = line_spacing(
                                attr_val(&e, "line").as_deref(),
                                attr_val(&e, "lineRule").as_deref(),
                            );
                        }
                        if before.is_none() {
                            before = space_pt(attr_val(&e, "before").as_deref());
                        }
                        if after.is_none() {
                            after = space_pt(attr_val(&e, "after").as_deref());
                        }
                    }
                    b"jc" if align.is_none() => {
                        align = align_of(attr_val(&e, "val").as_deref());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    (line, before, after, align, run)
}

/// 在容器內向前掃到 `w:rPr` 起點並收其內容（呼叫端已消耗容器 Start；遇到
/// 容器自身 End＝容器沒有 rPr，回 None）。
fn skip_to_rpr(
    reader: &mut quick_xml::Reader<&[u8]>,
    container_end: &[u8],
    theme: &ThemeColors,
    tf: Option<&ThemeFonts>,
) -> Option<RunStyle> {
    loop {
        match reader.read_event() {
            Err(_) | Ok(Event::Eof) => return None,
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if name == b"rPr" {
                    return Some(parse_rpr_body(reader, theme, tf));
                }
                if name.as_slice() == container_end {
                    return None;
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if name == b"rPr" || name.as_slice() == container_end {
                    return None;
                }
            }
            Ok(Event::Empty(e)) if e.local_name().as_ref() == container_end => {
                return None;
            }
            _ => {}
        }
    }
}

/// `w:pPrDefault` 子樹（其內可含 `w:pPr`；呼叫端已消耗 pPrDefault Start）。
fn parse_ppr_default(
    reader: &mut quick_xml::Reader<&[u8]>,
    theme: &ThemeColors,
    tf: Option<&ThemeFonts>,
) -> (
    Option<LineSpacing>,
    Option<f32>,
    Option<f32>,
    Option<Align>,
    RunStyle,
) {
    loop {
        match reader.read_event() {
            Err(_) | Ok(Event::Eof) => return Default::default(),
            Ok(Event::End(e)) if e.local_name().as_ref() == b"pPrDefault" => {
                return Default::default()
            }
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"pPr" => {
                return parse_ppr(reader, theme, tf)
            }
            _ => {}
        }
    }
}

fn attr_val(e: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key.as_bytes() {
            a.unescape_value().ok().map(|v| v.to_string())
        } else {
            None
        }
    })
}

impl StyleTable {
    /// 解析 `word/styles.xml`（**無主題色**：空主題表＝`w:themeColor` 一律
    /// 退回 `w:val` 回退值）。XML 錯誤＝就地停在已收部分（容錯：排版退回
    /// 後備值，不中斷開檔）。
    pub fn parse(xml: &[u8]) -> StyleTable {
        StyleTable::parse_with_fonts(xml, &ThemeColors::default(), None)
    }

    /// 解析 `word/styles.xml`，並帶入文件的主題色表（`w:themeColor` 查表用；
    /// ECMA-376 §17.3.2.25——**主題色優先於 `w:val` 回退值**）。
    ///
    /// 舊簽名保留為「無主題字型表」的 shim（`tf=None`）：theme 引用查不到
    /// 即保留直接值。需要 fontScheme 查表者請用
    /// [`StyleTable::parse_with_fonts`]。
    pub fn parse_with_theme(xml: &[u8], theme: &ThemeColors) -> StyleTable {
        StyleTable::parse_with_fonts(xml, theme, None)
    }

    /// 解析 `word/styles.xml`，帶入主題色表與主題字型表。
    ///
    /// `tf`＝文件 fontScheme：`w:rFonts` 的 theme 引用在**解析當下**查表
    /// 落地成直接值（ECMA-376 §17.3.2.26），級聯系統裡不再有 theme 槽。
    /// 無 theme part／查不到 → 保留直接值／該欄 None（老實回退）。
    pub fn parse_with_fonts(
        xml: &[u8],
        theme: &ThemeColors,
        tf: Option<&ThemeFonts>,
    ) -> StyleTable {
        let mut t = StyleTable {
            empty: true,
            ..Default::default()
        };
        let mut reader = quick_xml::Reader::from_reader(xml);
        let mut in_doc_defaults = false;
        let mut cur: Option<(String, OwnStyle)> = None;
        loop {
            match reader.read_event() {
                Err(_) | Ok(Event::Eof) => break,
                Ok(Event::Start(e)) => {
                    let name = e.local_name().as_ref().to_vec();
                    match name.as_slice() {
                        b"docDefaults" => in_doc_defaults = true,
                        b"rPrDefault" if in_doc_defaults => {
                            // rPrDefault 的子 rPr 直接收進 run_default（自閉合
                            // `<w:rPrDefault/>` 或無 rPr 時得空基底）。
                            if let Some(r) = skip_to_rpr(&mut reader, b"rPrDefault", theme, tf) {
                                t.run_default = r;
                            }
                            t.empty = false;
                        }
                        b"pPrDefault" if in_doc_defaults => {
                            let (l, b, af, a, r) = parse_ppr_default(&mut reader, theme, tf);
                            t.para_default_line = l;
                            t.para_default_spacing_before_pt = b;
                            t.para_default_spacing_after_pt = af;
                            t.para_default_align = a;
                            t.para_default_run = r;
                            t.empty = false;
                        }
                        b"style" => {
                            let id = attr_val(&e, "styleId").unwrap_or_default();
                            let kind = match attr_val(&e, "type").as_deref() {
                                Some("paragraph") => StyleKind::Paragraph,
                                Some("character") => StyleKind::Character,
                                _ => StyleKind::Other,
                            };
                            let is_default = attr_val(&e, "default")
                                .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
                            if is_default && kind == StyleKind::Paragraph && !id.is_empty() {
                                t.default_para = Some(id.clone());
                            }
                            cur = Some((
                                id,
                                OwnStyle {
                                    kind,
                                    ..Default::default()
                                },
                            ));
                        }
                        b"basedOn" => {
                            if let Some((_, own)) = cur.as_mut() {
                                own.based_on = attr_val(&e, "val");
                            }
                        }
                        b"link" => {
                            if let Some((_, own)) = cur.as_mut() {
                                own.link = attr_val(&e, "val");
                            }
                        }
                        b"rPr" => {
                            if let Some((_, own)) = cur.as_mut() {
                                // 主 rPr（style run properties）：段內文字的
                                // 唯一風格來源（t_16a13211 R3）。同 style 內
                                // 重複宣告＝首見為準。
                                own.run = std::mem::take(&mut own.run).overlay(&parse_rpr_body(
                                    &mut reader,
                                    theme,
                                    tf,
                                ));
                            }
                        }
                        b"pPr" => {
                            if let Some((_, own)) = cur.as_mut() {
                                let (l, b, af, a, r) = parse_ppr(&mut reader, theme, tf);
                                if own.line.is_none() {
                                    own.line = l;
                                }
                                if own.spacing_before_pt.is_none() {
                                    own.spacing_before_pt = b;
                                }
                                if own.spacing_after_pt.is_none() {
                                    own.spacing_after_pt = af;
                                }
                                if own.align.is_none() {
                                    own.align = a;
                                }
                                // 段落樣式的 pPr/rPr（段落標記 run 屬性）對段內
                                // 所有 run 生效（Word 段落標記/內容共用基底）；
                                // 但只補主 w:rPr 未宣告欄（ECMA §17.7.2.3）。
                                // 分欄存放（t_16a13211 R3）：標準 CT_Style 序
                                // pPr 在 rPr 之前，就地 overlay 會讓先解析的
                                // pPr/rPr 反客為主——合成式固定主 rPr 贏在
                                // resolve_cached。
                                own.ppr_run = std::mem::take(&mut own.ppr_run).overlay(&r);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(e)) => {
                    // style 內自閉合的 basedOn/link（少見但合法）。
                    if let Some((_, own)) = cur.as_mut() {
                        match e.local_name().as_ref() {
                            b"basedOn" => own.based_on = attr_val(&e, "val"),
                            b"link" => own.link = attr_val(&e, "val"),
                            _ => {}
                        }
                    }
                }
                Ok(Event::End(e)) => {
                    let name = e.local_name().as_ref().to_vec();
                    match name.as_slice() {
                        b"docDefaults" => in_doc_defaults = false,
                        b"style" => {
                            if let Some((id, own)) = cur.take() {
                                if !id.is_empty() {
                                    if own.kind == StyleKind::Other {
                                        t.table_styles.insert(id, own);
                                    } else {
                                        t.empty = false;
                                        t.styles.insert(id, own);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        // 畸形檔（`</w:style>` 前截斷）：半張樣式照「已收部分」入帳——
        // 比整張丟掉更接近 Word 的容錯行為（見函數頭）。
        if let Some((id, own)) = cur.take() {
            if !id.is_empty() {
                if own.kind == StyleKind::Other {
                    t.table_styles.insert(id, own);
                } else {
                    t.empty = false;
                    t.styles.insert(id, own);
                }
            }
        }
        t.resolve_all();
        t
    }

    /// 表內有無任何 style/docDefaults 宣告（空表＝呼叫端全走後備值）。
    pub fn is_empty(&self) -> bool {
        self.empty
    }

    /// styleId 的樣式型別（`w:style/@w:type`，ECMA-376 §17.7.2.4）。查無＝
    /// None（該 id 不在表內：非段落/字元型的樣式（table/numbering）不入表，
    /// 見 [`OwnStyle::kind`] 注）。
    ///
    /// 型別守衛的依據：`w:pStyle` 只能引用**段落型**樣式（§17.7.2.32），
    /// `w:rStyle` 只能引用**字元型**樣式（§17.7.2.29）；引用到別型＝畸形
    /// 檔，該引用不生效（Word 忽略之）。不驗證型別會讓字元樣式的 rPr/pPr
    /// 併進段落級（或反之），污染字級/行距/對齊。
    fn style_kind(&self, id: &str) -> Option<StyleKind> {
        self.styles.get(id).map(|s| s.kind)
    }

    /// docDefaults 自身的段前／段後距（pt；未宣告＝None）。K1 舊 gate 的
    /// 判定值；K4b 保留它，讓「docDefaults 有宣告」的檔案維持原行為，
    /// 只有「純靠預設段落樣式鏈」的檔案才走放寬後的新判定。
    pub fn doc_default_spacing(&self) -> (Option<f32>, Option<f32>) {
        (
            self.para_default_spacing_before_pt,
            self.para_default_spacing_after_pt,
        )
    }

    /// 文件級預設段前／段後距（pt；未宣告＝None）。K1 的「主開關」：
    /// 文件層級毫無段落距宣告時，呼叫端保持舊分頁路徑（零變化守門）。
    ///
    /// K4b：判定範圍從 docDefaults 放寬為 **docDefaults ∪ 預設段落樣式
    /// （Normal）的 basedOn 級聯鏈**。真實檔常把段距只寫在 Normal（例如
    /// Bug51170 的 `w:after="200"`＝10pt），docDefaults 完全空白；只看
    /// docDefaults 會把整個帶制關掉。`resolved` 已是「樣式鏈疊在 docDefaults
    /// 之上」的合成結果，故取其有效值即可（鏈走完、非法值已在解析期拒收）。
    /// 沒有任何 `w:default="1"` 段落樣式時退回慣例 id `Normal`（ECMA-376
    /// 的預設段落樣式；查無＝docDefaults，語義與舊版相同）。
    pub fn doc_spacing(&self) -> (Option<f32>, Option<f32>) {
        let default_chain = self
            .default_para
            .as_deref()
            .or(Some("Normal"))
            .and_then(|d| self.resolved.get(d));
        match default_chain {
            Some((_, p)) => (
                p.spacing_before_pt.or(self.para_default_spacing_before_pt),
                p.spacing_after_pt.or(self.para_default_spacing_after_pt),
            ),
            None => (
                self.para_default_spacing_before_pt,
                self.para_default_spacing_after_pt,
            ),
        }
    }

    /// 文件級行網格尺（呼叫端 fallback）：預設段落樣式（Normal，含其鏈與
    /// docDefaults）合成的（字級, 行距）。任一缺＝None（呼叫端自補 12pt／1.4）。
    pub fn doc_grid(&self) -> (Option<f32>, Option<LineSpacing>) {
        let base = self
            .default_para
            .as_deref()
            .and_then(|d| self.resolved.get(d))
            .cloned()
            .unwrap_or_else(|| self.doc_base());
        (base.0.size_pt, base.1.line)
    }

    /// docDefaults 合成基底（pPrDefault/rPr 優先於 rPrDefault——ECMA §17.7.2.5
    /// 層級：段落預設在全域預設之上）。
    fn doc_base(&self) -> (RunStyle, ParaStyle) {
        let run = self.para_default_run.clone().overlay(&self.run_default);
        (
            run.clone(),
            ParaStyle {
                run,
                line: self.para_default_line,
                spacing_before_pt: self.para_default_spacing_before_pt,
                spacing_after_pt: self.para_default_spacing_after_pt,
                align: self.para_default_align,
            },
        )
    }

    /// parse 結束一次性沿 basedOn 鏈合成全部樣式（查詢期全 `&self`）。
    /// id 排序走訪：畸形環形鏈的截斷結果必須確定（HashMap 隨機序會輸出飄移）。
    fn resolve_all(&mut self) {
        let mut ids: Vec<String> = self.styles.keys().cloned().collect();
        ids.sort();
        for id in ids {
            self.resolve_cached(&id);
        }
    }

    /// 取得 styleId 合成結果（遞迴先父後子＋快取；環／超深擋在 MAX_CHAIN）。
    /// 同步合成 `chain`（只沿 basedOn、不含 docDefaults 的 run 宣告，見欄位注）。
    fn resolve_cached(&mut self, id: &str) -> (RunStyle, ParaStyle) {
        if let Some(hit) = self.resolved.get(id) {
            return hit.clone();
        }
        let doc = self.doc_base();
        let Some(own) = self.styles.get(id).cloned() else {
            // 查無樣式：只剩 docDefaults 基底（畸形 pStyle/rStyle 引用容錯）。
            self.resolved.insert(id.to_string(), doc.clone());
            self.chain.insert(id.to_string(), RunStyle::default());
            return doc;
        };
        if self.visiting.len() >= MAX_CHAIN {
            // 超深／環：老實停在 doc 基底（見模組頭）。
            self.resolved.insert(id.to_string(), doc.clone());
            self.chain.insert(id.to_string(), RunStyle::default());
            return doc;
        }
        let (parent, parent_chain) = match own.based_on.as_deref().filter(|s| !s.is_empty()) {
            Some(b) => {
                // 自引用（basedOn 自己）直接停——避免無意義遞迴。
                // 跨型祖先（`w:basedOn` 須同型，ECMA-376 §17.7.2.3）：段落型
                // 樣式的祖先是字元型（畸形）＝斷鏈（不往上撈），字元樣式的
                // rPr/pPr 不得進段落級；反之（字元型的祖先是段落型）同樣斷鏈，
                // 段落樣式的 rPr 不得進字元級。查無該 id 同樣斷鏈（＝落
                // doc 基底，與舊實作同結果）。
                let same_kind = self.style_kind(b) == Some(own.kind);
                if !same_kind || b == id || self.visiting.iter().any(|v| v == b) {
                    (doc.clone(), RunStyle::default())
                } else {
                    self.visiting.push(id.to_string());
                    let p = self.resolve_cached(b);
                    // resolve_cached 必已填 chain（含查無／超深分支）。
                    let pc = self.chain.get(b).cloned().unwrap_or_default();
                    self.visiting.pop();
                    (p, pc)
                }
            }
            None => (doc.clone(), RunStyle::default()),
        };
        // 本樣式自帶疊在父合成結果上：先固定 `run.overlay(&ppr_run)`（主
        // rPr 贏段落標記 rPr，ECMA §17.7.2.3；t_16a13211 R3——優先序與 XML
        // 出現序無關），再補父缺；段落欄位缺位補父。
        let own_run = own.run.overlay(&own.ppr_run);
        // 純鏈版（不含 docDefaults）先算：chain 與 resolved 兩份合成各要一份
        // 輸入（`RunStyle` 不再 Copy，故先留一份副本）。
        let chain_run = own_run.clone().overlay(&parent_chain);
        let run = own_run.overlay(&parent.0);
        let para = ParaStyle {
            run: run.clone(),
            line: own.line.or(parent.1.line),
            spacing_before_pt: own.spacing_before_pt.or(parent.1.spacing_before_pt),
            spacing_after_pt: own.spacing_after_pt.or(parent.1.spacing_after_pt),
            align: own.align.or(parent.1.align),
        };
        let hit = (run, para);
        self.resolved.insert(id.to_string(), hit.clone());
        self.chain.insert(id.to_string(), chain_run);
        hit
    }

    /// 字元樣式（`w:rStyle` 引用）的 run 合成結果；查無＝None（呼叫端跳過）。
    /// 段落樣式 id 誤傳進來（畸形）＝**跳過**（None）：`w:rStyle` 只能引用
    /// 字元型樣式（ECMA-376 §17.7.2.29），段落樣式的 rPr 不得併進字元級。
    pub fn effective_char(&self, id: &str) -> Option<RunStyle> {
        if self.style_kind(id) != Some(StyleKind::Character) {
            return None;
        }
        self.resolved.get(id).map(|hit| hit.0.clone())
    }

    /// 字元樣式（`w:rStyle`）的**純鏈**宣告：只沿 basedOn 合成、不含
    /// docDefaults——engine 層把它疊在段落級結果之上時，docDefaults 必須
    /// 留在段落基底裡（ECMA-376 §17.7.2 層序；見 `chain` 欄位注）。
    ///
    /// 鏡像守衛：只認**字元型**樣式；`w:rStyle` 指向段落型（畸形）＝跳過
    /// （None）——否則段落樣式的 rPr 會在 run 級覆寫段內文字的字級/開關。
    pub fn char_chain(&self, id: &str) -> Option<RunStyle> {
        if self.style_kind(id) != Some(StyleKind::Character) {
            return None;
        }
        self.chain.get(id).cloned()
    }

    /// 段落級級聯：段落樣式（未宣告 pStyle＝落預設段落樣式 Normal，再落
    /// docDefaults）→ 疊段落直接格式（`inline`：直接 pPr 欄位＋其內 rPr，
    /// 由呼叫端解析後傳入）。回傳（段內 run 基底, 段落欄位合成結果）。
    ///
    /// 型別守衛：`w:pStyle` 只能引用**段落型**樣式（ECMA-376 §17.7.2.32）。
    /// 引用到字元型樣式（畸形檔，真實世界常見）＝該引用不生效（Word 忽略
    /// 之）——不得把字元樣式的 rPr/pPr 併進段落級（會污染字級/行距/對齊）。
    /// 忽略後的落點與「查無 id」同徑：預設段落樣式 → docDefaults。
    pub fn effective_para(
        &self,
        para_style_id: Option<&str>,
        inline: ParaStyle,
    ) -> (RunStyle, ParaStyle) {
        let (_, mut para) = match para_style_id.filter(|s| !s.is_empty()) {
            // 引用查無＝落預設段落樣式，再落 docDefaults（容錯不虛構）。
            Some(id) => {
                // 命中但型別不對（字元型塞進 pStyle）＝忽略該引用。
                let target = if self
                    .style_kind(id)
                    .is_some_and(|k| k != StyleKind::Paragraph)
                {
                    None
                } else {
                    Some(id)
                };
                target
                    .and_then(|t| self.resolved.get(t).cloned())
                    .or_else(|| {
                        self.default_para
                            .as_deref()
                            .and_then(|d| self.resolved.get(d).cloned())
                    })
                    .unwrap_or_else(|| self.doc_base())
            }
            None => match self.default_para.as_deref() {
                Some(dp) => self
                    .resolved
                    .get(dp)
                    .cloned()
                    .unwrap_or_else(|| self.doc_base()),
                None => self.doc_base(),
            },
        };
        // 段落直接格式（直接 pPr ∪ 其內 rPr）蓋過樣式宣告。
        para.run = inline.run.overlay(&para.run);
        para.line = inline.line.or(para.line);
        para.spacing_before_pt = inline.spacing_before_pt.or(para.spacing_before_pt);
        para.spacing_after_pt = inline.spacing_after_pt.or(para.spacing_after_pt);
        para.align = inline.align.or(para.align);
        let base = para.run.clone();
        (base, para)
    }

    /// K6：段落樣式鏈（只沿 basedOn，**不含 docDefaults**）的第一個行距宣告。
    /// 級聯優先序需把表格樣式插在「Normal 樣式」與「docDefaults」之間，
    /// 因此必須能分辨「這條 line 是樣式宣告還是 docDefaults 兜底」。
    pub fn para_chain_line(&self, id: Option<&str>) -> Option<LineSpacing> {
        let mut cur = match id.filter(|s| !s.is_empty()) {
            Some(s) if self.style_kind(s) == Some(StyleKind::Paragraph) => Some(s.to_string()),
            _ => self.default_para.clone(),
        };
        for _ in 0..MAX_CHAIN {
            let key = cur?;
            let own = self.styles.get(&key)?;
            if own.line.is_some() {
                return own.line;
            }
            cur = own
                .based_on
                .clone()
                .filter(|b| self.style_kind(b) == Some(StyleKind::Paragraph) && b != &key);
        }
        None
    }

    /// K6：段落樣式鏈（不含 docDefaults）的第一個段前／段後距宣告。
    pub fn para_chain_spacing(&self, id: Option<&str>) -> (Option<f32>, Option<f32>) {
        let mut cur = match id.filter(|s| !s.is_empty()) {
            Some(s) if self.style_kind(s) == Some(StyleKind::Paragraph) => Some(s.to_string()),
            _ => self.default_para.clone(),
        };
        for _ in 0..MAX_CHAIN {
            let Some(key) = cur else { break };
            let Some(own) = self.styles.get(&key) else {
                break;
            };
            if own.spacing_before_pt.is_some() || own.spacing_after_pt.is_some() {
                return (own.spacing_before_pt, own.spacing_after_pt);
            }
            cur = own
                .based_on
                .clone()
                .filter(|b| self.style_kind(b) == Some(StyleKind::Paragraph) && b != &key);
        }
        (None, None)
    }

    /// K6：表格樣式鏈（不含 docDefaults）的第一個段前／段後距宣告。
    pub fn table_style_spacing(&self, id: &str) -> (Option<f32>, Option<f32>) {
        let mut cur = Some(id.to_string());
        for _ in 0..MAX_CHAIN {
            let Some(key) = cur else { break };
            let Some(own) = self.table_styles.get(&key) else {
                break;
            };
            if own.spacing_before_pt.is_some() || own.spacing_after_pt.is_some() {
                return (own.spacing_before_pt, own.spacing_after_pt);
            }
            cur = own.based_on.clone().filter(|b| b != &key);
        }
        (None, None)
    }

    /// K6：表格樣式（`w:type="table"`）鏈的第一個行距宣告（不含 docDefaults）。
    /// 召喚端在「段落＞Normal 樣式＞表格樣式＞docDefaults」級聯中把它插在
    /// 第三層；查無 id／無宣告＝None（落到 docDefaults）。
    pub fn table_style_line(&self, id: &str) -> Option<LineSpacing> {
        let mut cur = Some(id.to_string());
        for _ in 0..MAX_CHAIN {
            let key = cur?;
            let own = self.table_styles.get(&key)?;
            if own.line.is_some() {
                return own.line;
            }
            cur = own.based_on.clone().filter(|b| b != &key);
        }
        None
    }

    /// K6：文件是否在任何級聯層宣告過字型族（docDefaults／樣式鏈／段落
    /// 直接格式由 engine 另檢）。無任何字型宣告的檔案維持舊 1.4 粗尺，
    /// 保 M1/R 系零宣告守門逐位元組不變。
    pub fn has_fonts(&self) -> bool {
        let all_run = |r: &RunStyle| r.fonts.is_some();
        self.run_default.fonts.is_some()
            || self.para_default_run.fonts.is_some()
            || self
                .styles
                .values()
                .any(|s| all_run(&s.run) || all_run(&s.ppr_run))
    }

    /// 級聯最外層：run 直接格式疊於（段落基底 ∪ 字元樣式）之上。
    pub fn resolve_run(base: RunStyle, direct: RunStyle) -> RunStyle {
        direct.overlay(&base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(xml: impl AsRef<str>) -> StyleTable {
        StyleTable::parse(xml.as_ref().as_bytes())
    }

    /// 最小主題 part：hlink＝305090（LO 實測的連結色）、accent1＝4F81BD。
    const THEME_HLINK: &[u8] =
        br#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
      <a:themeElements>
        <a:clrScheme name="Office">
          <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
          <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
          <a:accent1><a:srgbClr val="4F81BD"/></a:accent1>
          <a:hlink><a:srgbClr val="305090"/></a:hlink>
          <a:folHlink><a:srgbClr val="800080"/></a:folHlink>
        </a:clrScheme>
      </a:themeElements>
    </a:theme>"#;

    /// 主題色表（含 hlink＝305090）。
    fn theme() -> ThemeColors {
        ThemeColors::parse(THEME_HLINK)
    }

    /// 最小 fontScheme：major latin＝Calibri Light、minor latin＝Calibri。
    const THEME_FONTS: &[u8] =
        br#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
      <a:themeElements>
        <a:fontScheme name="Office">
          <a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/></a:majorFont>
          <a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/></a:minorFont>
        </a:fontScheme>
      </a:themeElements>
    </a:theme>"#;

    /// 主題字型表（major latin＝Calibri Light、minor latin＝Calibri）。
    fn theme_fonts() -> ThemeFonts {
        ThemeFonts::parse(THEME_FONTS)
    }

    const PREFIX: &str =
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#;
    const SUFFIX: &str = "</w:styles>";

    fn wrap(body: &str) -> String {
        format!("{PREFIX}{body}{SUFFIX}")
    }

    // ---------------- 主題色查表（w:themeColor → theme1.xml clrScheme） ----------------

    /// 主題色**優先於** `w:val` 回退值（ECMA-376 §17.3.2.25）：樣式層的
    /// `<w:color w:val="0000FF" w:themeColor="hyperlink"/>` 有效色＝主題 hlink
    /// ＝0x305090（修復前只吃 w:val＝純藍 0x0000FF，與 Word/LO 偏離）。
    #[test]
    fn theme_color_overrides_val_in_style() {
        let t = StyleTable::parse_with_theme(
            wrap(
                r#"<w:style w:type="character" w:styleId="Hyperlink">
                     <w:rPr><w:color w:val="0000FF" w:themeColor="hyperlink"/></w:rPr>
                   </w:style>"#,
            )
            .as_bytes(),
            &theme(),
        );
        let eff = t.effective_char("Hyperlink").expect("字元樣式必須可查");
        assert_eq!(
            eff.color,
            Some(0xFF30_5090),
            "主題色必須覆蓋 w:val 回退值（0x0000FF）"
        );
        assert!(eff.color_set, "宣告了顏色 → color_set 必真");
    }

    /// 回歸保護：**無 theme part** → 老實退回 `w:val`（不虛構主題色）。
    #[test]
    fn theme_absent_falls_back_to_val() {
        let t = StyleTable::parse_with_theme(
            wrap(
                r#"<w:style w:type="character" w:styleId="Hyperlink">
                     <w:rPr><w:color w:val="0000FF" w:themeColor="hyperlink"/></w:rPr>
                   </w:style>"#,
            )
            .as_bytes(),
            &ThemeColors::default(),
        );
        let eff = t.effective_char("Hyperlink").expect("字元樣式必須可查");
        assert_eq!(eff.color, Some(0xFF00_00FF), "無主題 → 退回 w:val");
        assert!(eff.color_set);
    }

    /// 容錯：`w:themeColor` 指向**查無**的名字（accent9）→ 退回 `w:val`。
    #[test]
    fn theme_unknown_name_falls_back_to_val() {
        let t = StyleTable::parse_with_theme(
            wrap(
                r#"<w:style w:type="character" w:styleId="Ghost">
                     <w:rPr><w:color w:val="123456" w:themeColor="accent9"/></w:rPr>
                   </w:style>"#,
            )
            .as_bytes(),
            &theme(),
        );
        let eff = t.effective_char("Ghost").expect("字元樣式必須可查");
        assert_eq!(eff.color, Some(0xFF12_3456), "查無該名 → 退回 w:val");
    }

    /// 誠實近似：帶 `w:themeShade`／`w:themeTint` 時，正確色是「變換後」的
    /// 主題色（lumMod/lumOff 變換本卡未實作）→ **保留 Word 預算好的
    /// `w:val`**，不得拿未變換的主題色冒充（會渲出 Word/LO 都沒有的色）。
    #[test]
    fn theme_shade_keeps_precomputed_val() {
        let t = StyleTable::parse_with_theme(
            wrap(
                r#"<w:style w:type="paragraph" w:styleId="Heading1">
                     <w:rPr><w:color w:val="365F91" w:themeColor="accent1" w:themeShade="BF"/></w:rPr>
                   </w:style>
                   <w:style w:type="paragraph" w:styleId="Heading2">
                     <w:rPr><w:color w:val="272727" w:themeColor="text1" w:themeTint="D8"/></w:rPr>
                   </w:style>"#,
            )
            .as_bytes(),
            &theme(),
        );
        let h1 = t.effective_para(Some("Heading1"), ParaStyle::default()).0;
        assert_eq!(
            h1.color,
            Some(0xFF36_5F91),
            "帶 themeShade → 保留 Word 預算色（非未變換的 accent1 4F81BD）"
        );
        let h2 = t.effective_para(Some("Heading2"), ParaStyle::default()).0;
        assert_eq!(
            h2.color,
            Some(0xFF27_2727),
            "帶 themeTint → 保留 Word 預算色（非未變換的 dk1 000000）"
        );
    }

    /// 主題色只在 `w:themeColor` 宣告時生效；無 themeColor 的樣式色不動。
    #[test]
    fn plain_val_untouched_by_theme() {
        let t = StyleTable::parse_with_theme(
            wrap(
                r#"<w:style w:type="character" w:styleId="Plain">
                     <w:rPr><w:color w:val="ABCDEF"/></w:rPr>
                   </w:style>"#,
            )
            .as_bytes(),
            &theme(),
        );
        let eff = t.effective_char("Plain").expect("字元樣式必須可查");
        assert_eq!(eff.color, Some(0xFFAB_CDEF));
    }

    #[test]
    fn empty_table_falls_back() {
        let t = table(wrap(""));
        assert!(t.is_empty());
        let (base, para) = t.effective_para(None, ParaStyle::default());
        assert_eq!(base, RunStyle::default());
        assert_eq!(para.line, None);
    }

    #[test]
    fn based_on_chain_merges_child_over_parent() {
        // basedOn 鏈合成：父宣告 sz/b，子宣告 i——子 inherit 父的 sz/b，i 只有子有。
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="Parent"><w:rPr><w:sz w:val="28"/><w:b/></w:rPr></w:style>
               <w:style w:type="paragraph" w:styleId="Child"><w:basedOn w:val="Parent"/><w:rPr><w:i/></w:rPr></w:style>"#,
        ));
        let (base, _) = t.effective_para(Some("Child"), ParaStyle::default());
        assert_eq!(base.size_pt, Some(14.0)); // 28 半磅 = 14pt。
        assert_eq!(base.bold, Some(true)); // 繼承自父。
        assert_eq!(base.italic, Some(true)); // 子自帶。
                                             // 父自身不變。
        let (pbase, _) = t.effective_para(Some("Parent"), ParaStyle::default());
        assert_eq!(pbase.italic, None);
    }

    #[test]
    fn direct_overrides_style_including_off_toggle() {
        // 直接格式優先：run 級 b=0 熄滅樣式級 b=1；sz 覆蓋；color auto=明確黑。
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="Big"><w:rPr><w:sz w:val="40"/><w:b/><w:color w:val="FF0000"/></w:rPr></w:style>"#,
        ));
        let inline = ParaStyle::default();
        let (base, _para) = t.effective_para(Some("Big"), inline);
        // run 直接格式：b=off + 無 sz（sz 繼承樣式）。
        let direct = RunStyle {
            bold: Some(false),
            ..Default::default()
        };
        let eff = StyleTable::resolve_run(base.clone(), direct);
        assert_eq!(eff.bold, Some(false)); // 直接格式熄滅樣式粗體。
        assert_eq!(eff.size_pt, Some(20.0)); // 未宣告→繼承樣式。
        assert_eq!(eff.color, Some(0xFFFF0000)); // 未宣告→繼承樣式色。
                                                 // 同段落改宣告 color auto → 明確黑（不得繼承 FF0000）。
        let direct_auto = RunStyle {
            color_set: true,
            color: None,
            ..Default::default()
        };
        let eff2 = StyleTable::resolve_run(base.clone(), direct_auto);
        assert_eq!(eff2.color, None);
        assert!(eff2.color_set);
    }

    #[test]
    fn missing_style_id_falls_back_to_default_para_then_docdefaults() {
        // pStyle 引用不存在的 id＝容錯退回預設段落樣式/docDefaults，不虛構。
        let t = table(wrap(
            r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>
               <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:rPr><w:i/></w:rPr></w:style>"#,
        ));
        let (base, _) = t.effective_para(Some("Ghost"), ParaStyle::default());
        // Ghost 查無→doc_base（無 Normal 鏈入）；sz 來自 docDefaults。
        assert_eq!(base.size_pt, Some(11.0));
        // 未宣告 pStyle 落 Normal。
        let (nbase, _) = t.effective_para(None, ParaStyle::default());
        assert_eq!(nbase.italic, Some(true));
        assert_eq!(nbase.size_pt, Some(11.0));
    }

    #[test]
    fn based_on_cycle_stops_without_hanging() {
        // 畸形檔：A↔B 互為 basedOn——第二遇環即停（老實停在已合成結果），
        // 不死迴圈；結果確定（排序走訪：A 先解，B 遇環停 doc 基底）。
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="A"><w:basedOn w:val="B"/><w:rPr><w:sz w:val="40"/></w:rPr></w:style>
               <w:style w:type="paragraph" w:styleId="B"><w:basedOn w:val="A"/><w:rPr><w:i/></w:rPr></w:style>"#,
        ));
        let (base, _) = t.effective_para(Some("A"), ParaStyle::default());
        assert_eq!(base.size_pt, Some(20.0)); // A 自帶。
        assert_eq!(base.italic, Some(true)); // B 截斷後仍沿鏈傳下（A←B←doc）。
                                             // 自引用（basedOn 自己）也照樣停在自身宣告。
        let t2 = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="S"><w:basedOn w:val="S"/><w:rPr><w:sz w:val="24"/></w:rPr></w:style>"#,
        ));
        let (base, _) = t2.effective_para(Some("S"), ParaStyle::default());
        assert_eq!(base.size_pt, Some(12.0));
        assert_eq!(base.bold, None);
    }

    #[test]
    fn line_spacing_rules() {
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="Exact20"><w:pPr><w:spacing w:line="400" w:lineRule="exact"/></w:pPr></w:style>
               <w:style w:type="paragraph" w:styleId="Double"><w:pPr><w:spacing w:line="480" w:lineRule="auto"/></w:pPr></w:style>
               <w:style w:type="paragraph" w:styleId="AtLeast"><w:pPr><w:spacing w:line="360" w:lineRule="atLeast"/></w:pPr></w:style>"#,
        ));
        let (_, p) = t.effective_para(Some("Exact20"), ParaStyle::default());
        assert_eq!(p.line, Some(LineSpacing::Exact(20.0))); // 400 twips = 20pt。
        let (_, p) = t.effective_para(Some("Double"), ParaStyle::default());
        assert_eq!(p.line, Some(LineSpacing::Auto(2.0))); // 480/240。
        let (_, p) = t.effective_para(Some("AtLeast"), ParaStyle::default());
        assert_eq!(p.line, Some(LineSpacing::AtLeast(18.0))); // 360/20。
    }

    #[test]
    fn doc_defaults_run_is_base_and_para_rpr_applies() {
        let t = table(wrap(
            r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="24"/></w:rPr></w:rPrDefault>
               <w:pPrDefault><w:pPr><w:spacing w:line="300" w:lineRule="auto"/><w:rPr><w:u/></w:rPr></w:pPr></w:pPrDefault></w:docDefaults>
               <w:style w:type="paragraph" w:default="1" w:styleId="Normal"/>
               <w:style w:type="character" w:styleId="Emph"><w:rPr><w:sz w:val="48"/></w:rPr></w:style>"#,
        ));
        // 段落基底：sz=12（docDefaults）＋u（pPrDefault/rPr）。
        let (base, para) = t.effective_para(None, ParaStyle::default());
        assert_eq!(base.size_pt, Some(12.0));
        assert_eq!(base.underline, Some(true));
        assert_eq!(para.line, Some(LineSpacing::Auto(1.25)));
        // 字元樣式疊加：sz 覆蓋、u 保留。
        let cs = t.effective_char("Emph").unwrap();
        let eff = StyleTable::resolve_run(base, cs);
        assert_eq!(eff.size_pt, Some(24.0));
        assert_eq!(eff.underline, Some(true));
    }

    #[test]
    fn paragraph_direct_ppr_rpr_and_jc_cascade() {
        // 段落直接 pPr/rPr（w:pPr/w:rPr）與 jc 保留。
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:rPr><w:sz w:val="20"/></w:rPr></w:style>"#,
        ));
        let inline = ParaStyle {
            run: RunStyle {
                italic: Some(true),
                ..Default::default()
            },
            line: None,
            spacing_before_pt: None,
            spacing_after_pt: None,
            align: Some(Align::Center),
        };
        let (base, para) = t.effective_para(None, inline);
        assert_eq!(base.size_pt, Some(10.0)); // 樣式級（段落直接 rPr 未宣告 sz）。
        assert_eq!(base.italic, Some(true)); // 段落直接 rPr。
        assert_eq!(para.align, Some(Align::Center)); // jc 解析保留。
    }

    #[test]
    fn style_main_rpr_beats_ppr_rpr_regardless_of_xml_order() {
        // ECMA-376 §17.7.2.3（t_16a13211 R3）：style 的主 `w:rPr`（style run
        // properties）對段內文字生效；`pPr/w:rPr` 是段落標記 run 屬性。段內
        // run 以主 rPr 為準。標準 CT_Style 序列 pPr 在 rPr **之前**——舊實作
        // 「先解析者贏」讓 Word 規範序反客為主（pPr/rPr 覆蓋主 rPr 同名欄）。
        // 修法＝OwnStyle 拆 run/ppr_run 兩欄，合成時固定主 rPr 優先。
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="S">
                 <w:pPr><w:rPr><w:sz w:val="20"/><w:i/></w:rPr></w:pPr>
                 <w:rPr><w:sz w:val="40"/></w:rPr>
               </w:style>"#,
        ));
        let (base, _) = t.effective_para(Some("S"), ParaStyle::default());
        assert_eq!(
            base.size_pt,
            Some(20.0),
            "主 rPr sz=40（半磅）贏 pPr/rPr sz=20"
        );
        assert_eq!(
            base.italic,
            Some(true),
            "pPr/rPr 未與主 rPr 衝突的欄位照常補缺"
        );
        // 出現序反轉（非規範序）不得翻轉結果——優先序是語義，不是 XML 順。
        let t2 = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="S">
                 <w:rPr><w:sz w:val="40"/></w:rPr>
                 <w:pPr><w:rPr><w:sz w:val="20"/><w:i/></w:rPr></w:pPr>
               </w:style>"#,
        ));
        let (base2, _) = t2.effective_para(Some("S"), ParaStyle::default());
        assert_eq!(base2.size_pt, Some(20.0));
        assert_eq!(base2.italic, Some(true));
        // basedOn 鏈同口徑：子樣式的 pPr/rPr 不得越過子主 rPr，更不得越過
        // 子主 rPr 搶父宣告（chain 合成走同一合成式）。
        let t3 = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="P"><w:rPr><w:b/></w:rPr></w:style>
               <w:style w:type="paragraph" w:styleId="C">
                 <w:basedOn w:val="P"/>
                 <w:pPr><w:rPr><w:sz w:val="20"/></w:rPr></w:pPr>
                 <w:rPr><w:sz w:val="40"/></w:rPr>
               </w:style>"#,
        ));
        let (base3, _) = t3.effective_para(Some("C"), ParaStyle::default());
        assert_eq!(base3.size_pt, Some(20.0), "子主 rPr 贏本子 pPr/rPr");
        assert_eq!(base3.bold, Some(true), "父宣告照常沿鏈補缺");
        // 字元樣式純鏈（chain）同口徑：pPr/rPr 只補缺、不蓋主 rPr。
        let t4 = table(wrap(
            r#"<w:style w:type="character" w:styleId="K">
                 <w:pPr><w:rPr><w:sz w:val="20"/></w:rPr></w:pPr>
                 <w:rPr><w:sz w:val="40"/></w:rPr>
               </w:style>"#,
        ));
        let chain = t4.char_chain("K").expect("字元樣式純鏈");
        assert_eq!(chain.size_pt, Some(20.0));
    }

    #[test]
    fn parse_survives_garbage_and_no_ns_prefix() {
        // 畸形 XML：就地停在已收部分，不 panic、不中斷。
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="A"><w:rPr><w:sz w:val="30"/>"#,
        ));
        let (base, _) = t.effective_para(Some("A"), ParaStyle::default());
        assert_eq!(base.size_pt, Some(15.0));
        // 無 w: 前綴（極少見，名稱以 local-name 匹配為準）也能收。
        let t2 = table(
            r#"<styles xmlns="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><style type="paragraph" styleId="Z"><rPr><b/></rPr></style></styles>"#,
        );
        let (base, _) = t2.effective_para(Some("Z"), ParaStyle::default());
        assert_eq!(base.bold, Some(true));
    }

    #[test]
    fn pstyle_pointing_to_character_style_is_ignored() {
        // 畸形檔（真實世界常見）：段落 `w:pStyle` 指向**字元型**樣式。
        // ECMA-376 §17.7.2.32：`w:pStyle` 只能引用段落型樣式，指向非段落
        // 型時該引用不生效（Word 忽略之）。舊實作不驗證 StyleKind，把字元
        // 樣式的 rPr/pPr 併進段落級——72 half-pt（＝36pt）的字級污染整段。
        let t = table(wrap(
            r#"<w:style w:type="character" w:styleId="X"><w:rPr><w:sz w:val="72"/></w:rPr></w:style>"#,
        ));
        let (base, _) = t.effective_para(Some("X"), ParaStyle::default());
        assert_ne!(
            base.size_pt,
            Some(36.0),
            "pStyle 指向字元型樣式＝忽略該樣式（不得套 72 half-pt＝36pt）"
        );
        assert_eq!(
            base.size_pt, None,
            "無任何合法來源＝不虛構字級（只剩 docDefaults 空基底）"
        );
        // 忽略後的落點與「查無 id」同徑：預設段落樣式仍生效（畸形引用
        // 不得連帶打掉整份文件的預設基底）。
        let t2 = table(wrap(
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:rPr><w:sz w:val="24"/></w:rPr></w:style>
               <w:style w:type="character" w:styleId="X"><w:rPr><w:sz w:val="72"/></w:rPr></w:style>"#,
        ));
        let (base2, _) = t2.effective_para(Some("X"), ParaStyle::default());
        assert_eq!(
            base2.size_pt,
            Some(12.0),
            "忽略畸形 pStyle 後落回預設段落樣式（24 半磅＝12pt）"
        );
    }

    #[test]
    fn paragraph_style_kind_still_applies_normally() {
        // 回歸保護：合法段落型樣式照常生效（型別守衛不得誤傷正常 pStyle）。
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="P"><w:rPr><w:sz w:val="40"/><w:b/></w:rPr></w:style>"#,
        ));
        let (base, _) = t.effective_para(Some("P"), ParaStyle::default());
        assert_eq!(base.size_pt, Some(20.0)); // 40 半磅 = 20pt。
        assert_eq!(base.bold, Some(true));
    }

    #[test]
    fn based_on_chain_to_character_ancestor_is_cut() {
        // 畸形 basedOn 鏈：段落樣式 Mid 的 basedOn 指向**字元型**樣式 Char。
        // 段落級級聯遇到字元型祖先即斷鏈（不往上撈）——只吃合法前綴
        // （Leaf 自身 ＋ Mid），Char 的字級不得滲入段落級。
        let t = table(wrap(
            r#"<w:style w:type="character" w:styleId="Char"><w:rPr><w:sz w:val="72"/></w:rPr></w:style>
               <w:style w:type="paragraph" w:styleId="Mid"><w:basedOn w:val="Char"/><w:rPr><w:b/></w:rPr></w:style>
               <w:style w:type="paragraph" w:styleId="Leaf"><w:basedOn w:val="Mid"/><w:rPr><w:i/></w:rPr></w:style>"#,
        ));
        let (base, _) = t.effective_para(Some("Leaf"), ParaStyle::default());
        assert_eq!(
            base.size_pt, None,
            "字元型祖先的 72 half-pt（＝36pt）不得滲入段落級"
        );
        assert_eq!(base.bold, Some(true), "合法前綴 Mid 的宣告照常生效");
        assert_eq!(base.italic, Some(true), "Leaf 自身宣告照常生效");
    }

    #[test]
    fn rstyle_pointing_to_paragraph_style_is_skipped() {
        // 鏡像守衛：`w:rStyle` 指向**段落型**樣式（畸形）＝跳過——段落樣式
        // 的 rPr 不得併進字元級（否則整段字級被段落樣式重寫）。
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="P"><w:rPr><w:sz w:val="72"/></w:rPr></w:style>"#,
        ));
        assert!(
            t.char_chain("P").is_none(),
            "rStyle 指向段落型樣式＝跳過字元級套用"
        );
        assert!(
            t.effective_char("P").is_none(),
            "同口徑：effective_char 鏡像守衛"
        );
        // 反向（守衛不得誤傷）：字元型 rStyle 照常生效。
        let t2 = table(wrap(
            r#"<w:style w:type="character" w:styleId="C"><w:rPr><w:sz w:val="28"/></w:rPr></w:style>"#,
        ));
        assert_eq!(t2.char_chain("C").unwrap().size_pt, Some(14.0));
    }

    #[test]
    fn toggle_values_none_zero_off_close_style_bold() {
        // b w:val="none"（ECMA-376 §17.3.1.1）＝明確關閉，覆蓋 docDefaults 的 b。
        let t = table(wrap(
            r#"<w:docDefaults><w:rPrDefault><w:rPr><w:b/></w:rPr></w:rPrDefault></w:docDefaults>
               <w:style w:type="paragraph" w:styleId="Plain"><w:rPr><w:b w:val="none"/></w:rPr></w:style>
               <w:style w:type="paragraph" w:styleId="Plain2"><w:rPr><w:b w:val="0"/></w:rPr></w:style>"#,
        ));
        let (base, _) = t.effective_para(Some("Plain"), ParaStyle::default());
        assert_eq!(base.bold, Some(false));
        let (base, _) = t.effective_para(Some("Plain2"), ParaStyle::default());
        assert_eq!(base.bold, Some(false));
    }

    // ---------------- 字型族宣告（w:rFonts；ECMA-376 §17.3.2.26） ----------------

    /// 級聯優先序：docDefaults 宣告 Arial → 段落樣式宣告 Times → 直接格式
    /// 宣告 Courier ＝ 有效 Courier（宣告即贏，逐層覆蓋）。
    #[test]
    fn rfonts_cascade_priority_direct_wins() {
        let t = table(wrap(
            r#"<w:docDefaults><w:rPrDefault><w:rPr>
                 <w:rFonts w:ascii="Arial" w:eastAsia="標楷體"/>
               </w:rPr></w:rPrDefault></w:docDefaults>
               <w:style w:type="paragraph" w:default="1" w:styleId="Normal">
                 <w:rPr><w:rFonts w:ascii="Times" w:eastAsia="新細明體"/></w:rPr>
               </w:style>"#,
        ));
        // 段落樣式鏈：樣式宣告贏 docDefaults。
        let (base, _) = t.effective_para(None, ParaStyle::default());
        let base_fonts = base.fonts.as_ref().expect("樣式鏈必須合成出字型族");
        assert_eq!(
            base_fonts.ascii.as_deref(),
            Some("Times"),
            "段落樣式贏 docDefaults"
        );
        assert_eq!(
            base_fonts.east_asia.as_deref(),
            Some("新細明體"),
            "east_asia 亦由段落樣式覆蓋"
        );
        // 直接格式宣告 Courier → 有效值 Courier。
        let direct = RunStyle {
            fonts: Some(RunFonts {
                ascii: Some("Courier".to_string()),
                east_asia: Some("微軟正黑體".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let eff = StyleTable::resolve_run(base, direct);
        let eff_fonts = eff.fonts.as_ref().expect("直接格式須帶字型族");
        assert_eq!(eff_fonts.ascii.as_deref(), Some("Courier"), "直接格式贏");
        assert_eq!(
            eff_fonts.east_asia.as_deref(),
            Some("微軟正黑體"),
            "east_asia 同樣由直接格式覆蓋"
        );
    }

    /// 逐欄繼承（不是整包覆蓋）：直接格式**只**宣告 `w:ascii` 時，
    /// `w:eastAsia` 落到上層的值（ECMA-376 §17.3.2.26 的族別各自獨立）。
    #[test]
    fn rfonts_inherits_per_field_not_whole_package() {
        let t = table(wrap(
            r#"<w:docDefaults><w:rPrDefault><w:rPr>
                 <w:rFonts w:ascii="Arial" w:eastAsia="標楷體"/>
               </w:rPr></w:rPrDefault></w:docDefaults>
               <w:style w:type="paragraph" w:default="1" w:styleId="Normal">
                 <w:rPr><w:rFonts w:ascii="Times"/></w:rPr>
               </w:style>"#,
        ));
        let (base, _) = t.effective_para(None, ParaStyle::default());
        let base_fonts = base.fonts.as_ref().expect("樣式鏈必須合成出字型族");
        assert_eq!(
            base_fonts.ascii.as_deref(),
            Some("Times"),
            "ascii 由段落樣式覆蓋"
        );
        assert_eq!(
            base_fonts.east_asia.as_deref(),
            Some("標楷體"),
            "east_asia 未宣告 → 逐欄繼承 docDefaults"
        );
        // 直接格式只宣告 ascii：east_asia 仍落到上層（不得被整包覆蓋成 None）。
        let direct = RunStyle {
            fonts: Some(RunFonts {
                ascii: Some("Courier".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let eff = StyleTable::resolve_run(base, direct);
        let eff_fonts = eff.fonts.as_ref().expect("直接格式須帶字型族");
        assert_eq!(
            eff_fonts.ascii.as_deref(),
            Some("Courier"),
            "ascii 由直接格式覆蓋"
        );
        assert_eq!(
            eff_fonts.east_asia.as_deref(),
            Some("標楷體"),
            "east_asia 未宣告 → 逐欄繼承上層（不是整包覆蓋）"
        );
    }

    /// theme 引用逐層解析（G2b）：上層 docDefaults 宣告 `eastAsia="標楷體"`
    /// ＋`asciiTheme="majorHAnsi"`、下層預設樣式宣告 `asciiTheme="minorHAnsi"`
    /// ——每個 `w:rFonts` 存入前就在同層查表落地，所以級聯結果是
    /// `ascii`＝minor 查表值（下層同層解析覆蓋上層）、`east_asia`＝標楷體
    /// （逐欄繼承不破）。級聯系統裡不存在未解析的 theme 槽。
    #[test]
    fn theme_refs_resolve_per_layer_before_cascade() {
        let t = StyleTable::parse_with_fonts(
            wrap(
                r#"<w:docDefaults><w:rPrDefault><w:rPr>
                     <w:rFonts w:eastAsia="標楷體" w:asciiTheme="majorHAnsi"/>
                   </w:rPr></w:rPrDefault></w:docDefaults>
                   <w:style w:type="paragraph" w:default="1" w:styleId="Normal">
                     <w:rPr><w:rFonts w:asciiTheme="minorHAnsi"/></w:rPr>
                   </w:style>"#,
            )
            .as_bytes(),
            &ThemeColors::default(),
            Some(&theme_fonts()),
        );
        let (base, _) = t.effective_para(None, ParaStyle::default());
        let fonts = base.fonts.as_ref().expect("樣式鏈必須合成出字型族");
        assert_eq!(
            fonts.ascii.as_deref(),
            Some("Calibri"),
            "下層 minorHAnsi 同層解析後必須覆蓋上層 majorHAnsi（不是 Calibri Light）"
        );
        assert_eq!(
            fonts.east_asia.as_deref(),
            Some("標楷體"),
            "東亞直接值必須逐欄繼承（下層只宣告西文 theme 不得抹掉）"
        );
    }

    /// 舊快照相容：`RunFonts` 的舊 JSON 照舊可讀。G2b 已刪除 theme 槽欄位，
    /// 舊快照若曾帶 `ascii_theme`／`east_asia_theme` 也只會被 serde 當未知
    /// 欄位忽略——不再有可往返的新欄位可測。
    #[test]
    fn run_fonts_old_json_still_reads() {
        let old = r#"{"ascii":"Arial","east_asia":null,"cs":null,"hint_east_asian":false}"#;
        let fonts: RunFonts = serde_json::from_str(old).expect("舊版 RunFonts JSON 必須可讀");
        assert_eq!(fonts.ascii.as_deref(), Some("Arial"));
        assert_eq!(fonts.east_asia, None);
    }

    /// 全鏈未宣告字型族 ＝ None（不虛構字型）。
    #[test]
    fn rfonts_absent_stays_none() {
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:rPr><w:b/></w:rPr></w:style>"#,
        ));
        let (base, _) = t.effective_para(None, ParaStyle::default());
        assert_eq!(base.fonts, None, "全鏈未宣告 → None");
        let eff = StyleTable::resolve_run(base, RunStyle::default());
        assert_eq!(eff.fonts, None, "直接格式未宣告 → 仍為 None");
    }

    /// (a) docDefaults 的 `w:before`／`w:after`（twips）解析成 pt；未宣告
    /// 必須是 `None`（不是 `Some(0)`）——零變化守門的依據。
    #[test]
    fn spacing_before_after_doc_defaults_twips_to_pt_and_none() {
        let t = table(wrap(
            r#"<w:docDefaults><w:pPrDefault><w:pPr>
                 <w:spacing w:before="160" w:after="240" w:line="259" w:lineRule="auto"/>
               </w:pPr></w:pPrDefault></w:docDefaults>"#,
        ));
        let (_, p) = t.effective_para(None, ParaStyle::default());
        assert_eq!(p.spacing_before_pt, Some(8.0), "160 twips = 8pt");
        assert_eq!(p.spacing_after_pt, Some(12.0), "240 twips = 12pt");

        let t2 = table(wrap(
            r#"<w:docDefaults><w:pPrDefault><w:pPr>
                 <w:spacing w:line="259" w:lineRule="auto"/>
               </w:pPr></w:pPrDefault></w:docDefaults>"#,
        ));
        let (_, p2) = t2.effective_para(None, ParaStyle::default());
        assert_eq!(p2.spacing_before_pt, None, "未宣告 = None，不是 Some(0)");
        assert_eq!(p2.spacing_after_pt, None, "未宣告 = None，不是 Some(0)");
    }

    /// (b) 樣式鏈繼承：basedOn 的 spacing 落地，子樣式只宣告一側時另一側
    /// 仍沿鏈取得（與 `line` 同語義）。
    #[test]
    fn spacing_before_after_inherit_along_based_on_chain() {
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:styleId="Parent">
                 <w:pPr><w:spacing w:before="200" w:after="160"/></w:pPr>
               </w:style>
               <w:style w:type="paragraph" w:styleId="Child">
                 <w:basedOn w:val="Parent"/>
                 <w:pPr><w:spacing w:after="40"/></w:pPr>
               </w:style>"#,
        ));
        let (_, child) = t.effective_para(Some("Child"), ParaStyle::default());
        assert_eq!(
            child.spacing_before_pt,
            Some(10.0),
            "before 沿鏈繼承 200 twips"
        );
        assert_eq!(
            child.spacing_after_pt,
            Some(2.0),
            "after 由子樣式覆寫 40 twips"
        );
    }

    /// (c) 段落 pPr 直接格式覆寫樣式鏈（inline 優先）。
    #[test]
    fn paragraph_direct_spacing_overrides_style_chain() {
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal">
                 <w:pPr><w:spacing w:before="200" w:after="200"/></w:pPr>
               </w:style>"#,
        ));
        let inline = ParaStyle {
            spacing_before_pt: Some(1.5),
            spacing_after_pt: Some(2.5),
            ..ParaStyle::default()
        };
        let (_, p) = t.effective_para(None, inline);
        assert_eq!(p.spacing_before_pt, Some(1.5), "直接格式覆寫樣式 before");
        assert_eq!(p.spacing_after_pt, Some(2.5), "直接格式覆寫樣式 after");
    }

    /// (d) 顯式 `w:before="0"` 必須是 `Some(0)`（有宣告），與未宣告的
    /// `None` 區分；零值本身不產生任何佔帶（由 layout 守門）。
    #[test]
    fn explicit_zero_spacing_is_some_zero() {
        let t = table(wrap(
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal">
                 <w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr>
               </w:style>"#,
        ));
        let (_, p) = t.effective_para(None, ParaStyle::default());
        assert_eq!(p.spacing_before_pt, Some(0.0));
        assert_eq!(p.spacing_after_pt, Some(0.0));
    }
}
