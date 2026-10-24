//! 清單編號解析（`word/numbering.xml`；ECMA-376 §17.9）。
//!
//! 背景：LO 渲染出 `I.`／`a.`／`b.`／`•` 等清單符號，我方一個都沒發
//! （見 `docs/j3-hc031-pagination-diagnosis.md` (c)）。K2a 建忠實資料模型
//! （本模組）；K2b 由 engine 的 `lists.rs` 消費成發射投影的 marker run。
//! 本模組本身仍是純解析、**對版面／存檔路徑零影響**（投影另存，part 明文
//! 不動）。
//!
//! ## 資料模型三層（ECMA-376 §17.9）
//!
//! 1. `w:abstractNum`（§17.9.1）——多層定義本體（`w:lvl` × 9），與實例無關；
//! 2. `w:num`（§17.9.16）——實例：`w:abstractNumId` 指向抽象定義，另可帶
//!    `w:lvlOverride`（§17.9.6）逐層覆寫；
//! 3. 正文 `w:pPr/w:numPr`（§17.3.1.19）——段落引用（`w:ilvl` ＋ `w:numId`）。
//!
//! ## 口徑與老實邊界
//!
//! - **僅收 corpus 實見值**：[`NumberFormat`] 的值域來自 corpus 實掃（見
//!   模組末「值域出處」），未見值老實歸 `None`（同 H1 的 `NotePr::num_fmt`
//!   語義——不憑 ECMA 全集臆造）。
//! - **`w:lvlText` 原樣存**：佔位字串（`%1.`／`%1.%2`／項目符號字元）逐字
//!   保存，不做任何展開或正規化——展開是消費端（K3）的事。
//! - **`w:numStyleLink`／`w:styleLink` 只記錄不解析**：兩者指向 styles.xml
//!   的樣式 id（§17.9.21／§17.9.22），樣式鏈解析不在本卡範圍（老實記下
//!   字串，等消費端決定怎麼走）。
//! - **縮排單位**：`w:ind@left`／`@hanging` 原始單位是 twips，本模組換算成
//!   pt（÷20）存檔；`@firstLine`／`@right` 等其餘縮排欄**不收**（見未做清單）。
//! - **容錯**：缺 part／壞 XML ＝空彙總（不 panic）；`w:numId` 指向不存在的
//!   `w:abstractNum` ＝查不到有效層級（`None`），**不虛構**層級。
//! - **段落索引口徑**：本模組的 `para_index` 與 `crate::parse` 產出的
//!   `OoxmlDoc::paragraphs` **同序**（`w:p` Start 佔槽、自閉合 `<w:p/>` 不
//!   佔槽、`mc:Fallback` 子樹整棵吞沒）——口徑鐵律同 `drawing.rs` 的
//!   `InlineImage::para_index`。
//!
//! ## 值域出處（corpus 實掃，400 份 docx／154 檔含 numbering.xml）
//!
//! - `w:numFmt`：bullet(8492)／decimal(3738)／lowerLetter(1537)／
//!   lowerRoman(1531)／none(258)／upperLetter(54)／upperRoman(22)／
//!   decimalZero(10)／ordinal(5)／arabicAlpha(2)／custom(1)。
//! - `w:multiLevelType`：hybridMultilevel／multilevel／singleLevel。
//! - `w:lvlJc`：left／right／center。
//!
//! ## 未做清單（老實記下能力邊界，非漏實作）
//!
//! - **計數器語意不在解析層**：本模組只提供忠實素材；遞增／重啟／展開由
//!   engine 的清單發射層（K2b `lists.rs`）消費。
//! - **`w:lvl/w:rPr` 只收 `w:rFonts@ascii`**：字級（`w:sz`）與 hAnsi／cs
//!   等其餘字型欄不收——符號映射目前只吃 `@ascii`（K2b 實見值）。
//! - `w:lvl/w:pStyle`（層級↔段落樣式連結）、`w:isLgl`（數字轉字母）不收。
//! - `w:ind@firstLine`／`@right` 不收（只收 `left`／`hanging`）。
//! - `w:nsid`／`w:tmpl`／`w:tentative`／`w:tplc`（識別碼與試探旗標）不收。
//! - **樣式繼承而來的編號不追**：本模組只認段落**直接宣告**的 `w:pPr/w:numPr`；
//!   styles.xml 裡的 `w:style`（type=numbering）及其 `w:numPr` 屬樣式鏈解析
//!   （`styles.rs` 的管線），不在本卡範圍。
//! - `w:num` 的 `w:numIdMacAtCleanup`（舊版 Word 的相容旗標）不收。

use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::Align;

/// 編號 part 的慣例路徑（OPC 佈局：Word／LO 產出恆為此名）。
const NUMBERING_PART: &str = "word/numbering.xml";
/// 正文 part 的慣例路徑（段落引用 `w:pPr/w:numPr` 所在）。
const DOCUMENT_PART: &str = "word/document.xml";

/// 編號格式（`w:numFmt/@w:val`；ECMA-376 §17.9.3）。
///
/// **僅收 corpus 實見值**（見模組頭「值域出處」）；未見值於解析時老實歸
/// [`Option::None`]（同 H1 的 `NotePr::num_fmt` 語義——不憑 ECMA 全集臆造）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberFormat {
    /// `decimal`：阿拉伯數字（`1`／`2`／…）。
    Decimal,
    /// `decimalZero`：補零阿拉伯數字（`01`／`02`／…）。
    DecimalZero,
    /// `lowerLetter`：小寫英文字母（`a`／`b`／…）。
    LowerLetter,
    /// `upperLetter`：大寫英文字母（`A`／`B`／…）。
    UpperLetter,
    /// `lowerRoman`：小寫羅馬數字（`i`／`ii`／…）。
    LowerRoman,
    /// `upperRoman`：大寫羅馬數字（`I`／`II`／…）。
    UpperRoman,
    /// `bullet`：項目符號（符號字面由 `w:lvlText` 給，不做數字換算）。
    Bullet,
    /// `ordinal`：序數（`1st`／`2nd`／…；格式字串由 `w:numFmt/@w:format` 給）。
    Ordinal,
    /// `arabicAlpha`：阿拉伯數字＋字母（`1a`／`1b`／…）。
    ArabicAlpha,
    /// `none`：該層不顯示編號字面。
    None,
    /// `custom`：自訂格式（格式字串在 `w:numFmt/@w:format`，見
    /// [`Level::num_fmt_format`]）。
    Custom,
}

/// 抽象編號的多層級類型（`w:multiLevelType/@w:val`；ECMA-376 §17.9.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MultiLevelType {
    /// `singleLevel`：單層清單。
    SingleLevel,
    /// `multilevel`：多層清單。
    MultiLevel,
    /// `hybridMultilevel`：混合多層（Word 預設）。
    HybridMultilevel,
}

/// 一個編號層級（`w:lvl`；ECMA-376 §17.9.6）。
///
/// 欄位語義逐條對應 XML；**未宣告者為 `None`**（老實：沒有就是沒有）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Level {
    /// `@w:ilvl`：層級（0–8；越界值照收，見 [`Numbering::effective_levels`]）。
    #[serde(default)]
    pub ilvl: u32,
    /// `w:start/@w:val`：該層起始編號（缺＝`None`；消費端用 1）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    /// `w:numFmt/@w:val`：編號格式。未見值＝`None`（同 H1 語義）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_fmt: Option<NumberFormat>,
    /// `w:numFmt/@w:format`：自訂格式字串（僅 `custom` 形態有意義；
    /// corpus 實見值形如 `α, β, γ, ...`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_fmt_format: Option<String>,
    /// `w:lvlText/@w:val`：**原樣存**的層級文字（佔位字串如 `%1.`／`%1.%2`，
    /// 項目符號則是字元本身）。不做展開——展開是消費端的事。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lvl_text: Option<String>,
    /// `w:lvlJc/@w:val`：層級文字對齊（複用既有 [`Align`] 枚舉）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jc: Option<Align>,
    /// `w:pPr/w:ind@w:left`：**pt**（原始 twips ÷ 20）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ind_left_pt: Option<f32>,
    /// `w:pPr/w:ind@w:hanging`：**pt**（原始 twips ÷ 20；懸掛縮排量）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ind_hanging_pt: Option<f32>,
    /// `w:rPr/w:numFmt/@w:val`：層級 run 屬性的格式值。**字級／字型
    /// （`w:sz`／`w:rFonts`）不收**——項目符號的符號字型（`Symbol`／`Wingdings`）
    /// 要等消費端決策（見模組末未做清單）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpr_num_fmt: Option<NumberFormat>,
    /// `w:rPr/w:rFonts/@w:ascii`：層級符號字型（K2b）。項目符號 `w:lvlText`
    /// 的私用區字元（U+F0B7 等）要靠它才翻得出「•」（Symbol／Wingdings）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpr_font_ascii: Option<String>,
    /// `w:numRestart/@w:val`：清單重啟時機（K2b 消費 `eachSect`＝節邊界重啟）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_restart: Option<String>,
    /// `w:lvlRestart/@w:val`：本層在第幾層前進時重啟（0＝永不重啟）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lvl_restart: Option<u32>,
    /// `w:suff/@w:val`：編號與文字間的後綴（`tab`／`space`／`nothing`；缺＝None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suff: Option<String>,
}

/// 抽象編號定義（`w:abstractNum`；ECMA-376 §17.9.1）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AbstractNum {
    /// `@w:abstractNumId`：抽象定義 id。
    #[serde(default)]
    pub abstract_num_id: u32,
    /// `w:multiLevelType/@w:val`：多層級類型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_level_type: Option<MultiLevelType>,
    /// 逐 `w:lvl`（`@w:ilvl` 0–8）。
    #[serde(default)]
    pub levels: Vec<Level>,
    /// `w:styleLink/@w:val`（§17.9.22）：指向 styles.xml 的樣式 id。
    /// **只記錄不解析**（樣式鏈解析不在本卡範圍）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_link: Option<String>,
    /// `w:numStyleLink/@w:val`（§17.9.21）：指向 styles.xml 的編號樣式 id。
    /// **只記錄不解析**。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_style_link: Option<String>,
}

/// 實例層的層級覆寫（`w:lvlOverride`；ECMA-376 §17.9.6）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LevelOverride {
    /// `@w:ilvl`：被覆寫的層級。
    #[serde(default)]
    pub ilvl: u32,
    /// `w:startOverride/@w:val`：該層起始編號覆寫（缺＝不覆寫）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_override: Option<i64>,
    /// `w:lvlOverride/w:lvl`：整層定義覆寫（缺＝只覆寫起始編號）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
}

/// 編號實例（`w:num`；ECMA-376 §17.9.16）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NumInstance {
    /// `@w:numId`：實例 id（正文 `w:numPr/w:numId@w:val` 指向它）。
    #[serde(default)]
    pub num_id: u32,
    /// `w:abstractNumId/@w:val`：指向的抽象定義 id。**病態檔可能缺**
    /// （老實＝`None`，該實例查不到有效層級）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abstract_num_id: Option<u32>,
    /// 逐 `w:lvlOverride`。
    #[serde(default)]
    pub overrides: Vec<LevelOverride>,
    /// `w:styleLink/@w:val`：**只記錄不解析**。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_link: Option<String>,
    /// `w:numStyleLink/@w:val`：**只記錄不解析**。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_style_link: Option<String>,
}

/// 一份文件的清單編號定義（`word/numbering.xml` 全量）。
///
/// 缺 part／壞 XML ＝空彙總（等同 [`Numbering::default`]）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Numbering {
    /// 全部 `w:abstractNum`（文件序）。
    #[serde(default)]
    pub abstract_nums: Vec<AbstractNum>,
    /// 全部 `w:num`（文件序）。
    #[serde(default)]
    pub nums: Vec<NumInstance>,
}

/// 段落對編號的引用（正文 `w:pPr/w:numPr`；ECMA-376 §17.3.1.19）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumberingRef {
    /// 段落索引——與 `OoxmlDoc::paragraphs` **同序**（`w:p` Start 佔槽、
    /// 自閉合 `<w:p/>` 不佔槽、`mc:Fallback` 子樹吞沒；口徑鐵律同
    /// `drawing.rs` 的 `InlineImage::para_index`）。
    pub para_index: usize,
    /// `w:ilvl/@w:val`：層級（缺＝0，ECMA 預設）。
    pub ilvl: u32,
    /// `w:numId/@w:val`：編號實例 id。**`0` 有意義**（ECMA：0＝取消編號），
    /// 照實收錄；查有效層級時回 `None`。
    pub num_id: u32,
}

/// 解析 `word/numbering.xml`（ECMA-376 §17.9）。
///
/// 容錯口徑同 [`crate::ThemeColors`]：壞 XML ＝停在已收部分（不 panic）；
/// 單筆缺關鍵屬性（`@w:abstractNumId`／`@w:numId`／`@w:ilvl`）者老實略過該筆。
///
/// `mc` 口徑同全 crate 一致：`mc:Choice` 透明、`mc:Fallback` 子樹整棵吞沒
/// （雙份表示只取一份——corpus 實例：`poi/test-data_document_Numbering.docx`
/// 的 `w:numFmt w:val="custom"` 就在 `mc:Choice` 裡，`mc:Fallback` 是
/// `decimal` 備援）。
pub fn parse_numbering(xml: &[u8]) -> Numbering {
    let text = String::from_utf8_lossy(xml).into_owned();
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut p = NumberingParser::default();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if p.fb > 0 {
                    // Fallback 子樹內：只維護巢狀深度與 AC 快照，其餘不看。
                    if name == b"Fallback" {
                        p.fb += 1;
                    } else if name == b"AlternateContent" {
                        p.ac_stack.push(p.fb);
                    }
                    p.open.push(name);
                    continue;
                }
                p.on_open(&name, &e);
            }
            Ok(Event::Empty(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if p.fb > 0 {
                    continue;
                }
                // 自閉合元素：開→取值→閉，一次走完（與成對寫法同效果）。
                p.on_open(&name, &e);
                p.on_close(&name);
                p.open.pop();
            }
            Ok(Event::End(e)) => {
                let name = e.local_name().as_ref().to_vec();
                // MC 標籤完結先行處理（即便在 Fallback 內）：錯置標籤
                // （AC 先於 Fallback 閉合）不致 fb 卡死而吞掉檔尾。
                if name == b"AlternateContent" {
                    p.fb = p.ac_stack.pop().unwrap_or(0);
                    p.open.pop();
                    continue;
                }
                if p.fb > 0 {
                    if name == b"Fallback" {
                        p.fb = p.fb.saturating_sub(1);
                    }
                    p.open.pop();
                    continue;
                }
                p.on_close(&name);
                p.open.pop();
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            // XML 畸形：老實返回已收集的部分（容錯口徑同 ThemeColors::parse）。
            Err(_) => break,
        }
    }
    p.out
}

/// `numbering.xml` 的事件流狀態機（單一職責：把事件流收成 [`Numbering`]）。
#[derive(Default)]
struct NumberingParser {
    /// 產出。
    out: Numbering,
    /// 開啟中的元素 local-name 堆疊（棧頂＝當前元素；棧頂的下一層＝父層）。
    open: Vec<Vec<u8>>,
    /// `mc:Fallback` 深度（>0＝吞沒子樹）。
    fb: usize,
    /// `mc:AlternateContent` 巢狀快照（錯置標籤不致 fb 卡死）。
    ac_stack: Vec<usize>,
    /// 進行中的 `w:abstractNum`。
    cur_abstract: Option<AbstractNum>,
    /// 進行中的 `w:num`。
    cur_num: Option<NumInstance>,
    /// 進行中的 `w:lvl`（抽象層或覆寫層皆用它）。
    cur_lvl: Option<Level>,
    /// 進行中的 `w:lvlOverride`。
    cur_ovr: Option<LevelOverride>,
}

impl NumberingParser {
    /// 元素開啟：容器建檔（`w:abstractNum`／`w:num`／`w:lvl`／`w:lvlOverride`）
    /// ＋葉元素取值（依**父層**分流）。
    fn on_open(&mut self, name: &[u8], e: &quick_xml::events::BytesStart) {
        match name {
            b"abstractNum" => {
                self.cur_abstract = Some(AbstractNum {
                    abstract_num_id: attr_u32(e, "abstractNumId").unwrap_or(0),
                    ..Default::default()
                });
            }
            b"num" => {
                self.cur_num = Some(NumInstance {
                    num_id: attr_u32(e, "numId").unwrap_or(0),
                    ..Default::default()
                });
            }
            b"lvl" => {
                self.cur_lvl = Some(Level {
                    ilvl: attr_u32(e, "ilvl").unwrap_or(0),
                    ..Default::default()
                });
            }
            b"lvlOverride" => {
                self.cur_ovr = Some(LevelOverride {
                    ilvl: attr_u32(e, "ilvl").unwrap_or(0),
                    ..Default::default()
                });
            }
            _ => {}
        }
        // 葉元素取值：父層＝本元素開啟**前**的棧頂（此刻尚未入堆）。
        self.apply_leaf(name, e);
        self.open.push(name.to_vec());
    }

    /// 語意祖先鏈（近→遠）：**跳過 mc 包裝**（`mc:AlternateContent`／
    /// `mc:Choice` 透明——corpus 實例：`w:numFmt` 會被包在 `mc:Choice` 裡，
    /// 其語意父層仍是 `w:lvl`）。
    fn ancestors(&self) -> Vec<&[u8]> {
        self.open
            .iter()
            .rev()
            .map(|v| v.as_slice())
            .filter(|n| *n != b"Choice" && *n != b"AlternateContent")
            .collect()
    }

    /// 葉元素取值（父層分流；父＝語意父層、祖父＝語意祖父層）。
    fn apply_leaf(&mut self, name: &[u8], e: &quick_xml::events::BytesStart) {
        let anc = self.ancestors();
        let parent = anc.first().copied();
        let grand = anc.get(1).copied();
        match (parent, name) {
            // ---- w:lvl 的直接子元素（ECMA-376 §17.9.6）----
            (Some(b"lvl"), b"start") => {
                if let Some(l) = self.cur_lvl.as_mut() {
                    l.start = attr_i64(e, "val");
                }
            }
            (Some(b"lvl"), b"numFmt") => {
                if let Some(l) = self.cur_lvl.as_mut() {
                    l.num_fmt = attr_val(e, "val").as_deref().and_then(parse_number_format);
                    l.num_fmt_format = attr_val(e, "format");
                }
            }
            (Some(b"lvl"), b"lvlText") => {
                if let Some(l) = self.cur_lvl.as_mut() {
                    // 原樣存（佔位字串／符號字元都不加工）。
                    l.lvl_text = attr_val(e, "val");
                }
            }
            (Some(b"lvl"), b"lvlJc") => {
                if let Some(l) = self.cur_lvl.as_mut() {
                    l.jc = crate::styles::align_of(attr_val(e, "val").as_deref());
                }
            }
            (Some(b"lvl"), b"numRestart") => {
                if let Some(l) = self.cur_lvl.as_mut() {
                    l.num_restart = attr_val(e, "val");
                }
            }
            (Some(b"lvl"), b"lvlRestart") => {
                if let Some(l) = self.cur_lvl.as_mut() {
                    l.lvl_restart = attr_u32(e, "val");
                }
            }
            (Some(b"lvl"), b"suff") => {
                if let Some(l) = self.cur_lvl.as_mut() {
                    l.suff = attr_val(e, "val");
                }
            }
            // ---- w:lvl/w:pPr/w:ind（縮排；twips → pt）----
            (Some(b"pPr"), b"ind") if grand == Some(b"lvl") => {
                if let Some(l) = self.cur_lvl.as_mut() {
                    l.ind_left_pt = twips_to_pt(attr_val(e, "left"));
                    l.ind_hanging_pt = twips_to_pt(attr_val(e, "hanging"));
                }
            }
            // ---- w:lvl/w:rPr/w:numFmt（層級 run 屬性；字級／字型不收）----
            (Some(b"rPr"), b"numFmt") if grand == Some(b"lvl") => {
                if let Some(l) = self.cur_lvl.as_mut() {
                    l.rpr_num_fmt = attr_val(e, "val").as_deref().and_then(parse_number_format);
                }
            }
            // ---- w:lvl/w:rPr/w:rFonts（K2b：符號字型，只取 @w:ascii）----
            (Some(b"rPr"), b"rFonts") if grand == Some(b"lvl") => {
                if let Some(l) = self.cur_lvl.as_mut() {
                    l.rpr_font_ascii = attr_val(e, "ascii");
                }
            }
            // ---- w:abstractNum 的直接子元素（§17.9.1）----
            (Some(b"abstractNum"), b"multiLevelType") => {
                if let Some(a) = self.cur_abstract.as_mut() {
                    a.multi_level_type = attr_val(e, "val")
                        .as_deref()
                        .and_then(parse_multi_level_type);
                }
            }
            (Some(b"abstractNum"), b"styleLink") => {
                if let Some(a) = self.cur_abstract.as_mut() {
                    a.style_link = attr_val(e, "val");
                }
            }
            (Some(b"abstractNum"), b"numStyleLink") => {
                if let Some(a) = self.cur_abstract.as_mut() {
                    a.num_style_link = attr_val(e, "val");
                }
            }
            // ---- w:num 的直接子元素（§17.9.16）----
            (Some(b"num"), b"abstractNumId") => {
                if let Some(n) = self.cur_num.as_mut() {
                    n.abstract_num_id = attr_u32(e, "val");
                }
            }
            (Some(b"num"), b"styleLink") => {
                if let Some(n) = self.cur_num.as_mut() {
                    n.style_link = attr_val(e, "val");
                }
            }
            (Some(b"num"), b"numStyleLink") => {
                if let Some(n) = self.cur_num.as_mut() {
                    n.num_style_link = attr_val(e, "val");
                }
            }
            // ---- w:lvlOverride 的直接子元素（§17.9.6）----
            (Some(b"lvlOverride"), b"startOverride") => {
                if let Some(o) = self.cur_ovr.as_mut() {
                    o.start_override = attr_i64(e, "val");
                }
            }
            _ => {}
        }
    }

    /// 元素閉啟：把收好的狀態落進容器。
    fn on_close(&mut self, name: &[u8]) {
        // 此刻 `open` 棧頂＝正在閉合的元素本身，其下一層＝語意父層。
        let parent = if self.open.len() >= 2 {
            Some(self.open[self.open.len() - 2].as_slice())
        } else {
            None
        };
        match name {
            b"abstractNum" => {
                if let Some(a) = self.cur_abstract.take() {
                    self.out.abstract_nums.push(a);
                }
            }
            b"num" => {
                if let Some(n) = self.cur_num.take() {
                    self.out.nums.push(n);
                }
            }
            b"lvl" => {
                if let Some(l) = self.cur_lvl.take() {
                    match parent {
                        // 覆寫層內的 w:lvl → 進覆寫；抽象層內的 → 進抽象定義。
                        Some(b"lvlOverride") => {
                            if let Some(o) = self.cur_ovr.as_mut() {
                                o.level = Some(l);
                            }
                        }
                        _ => {
                            if let Some(a) = self.cur_abstract.as_mut() {
                                a.levels.push(l);
                            }
                        }
                    }
                }
            }
            b"lvlOverride" => {
                if let Some(o) = self.cur_ovr.take() {
                    if let Some(n) = self.cur_num.as_mut() {
                        n.overrides.push(o);
                    }
                }
            }
            _ => {}
        }
    }
}

/// 解析正文 `word/document.xml` 的段落編號引用（`w:pPr/w:numPr`）。
///
/// 回傳依文件序的 [`NumberingRef`] 清單；無 `w:numPr` 的段落不出現在清單裡。
///
/// 索引口徑（口徑鐵律，與 `crate::parse` 的 `OoxmlDoc::paragraphs` 同序）：
/// - `w:p` **Start** 佔一個槽（自閉合 `<w:p/>` 不佔槽）；
/// - `mc:Fallback` 子樹整棵吞沒（`mc:Choice` 透明）；
/// - `w:numPr` 只認**段落直接子層**的 `w:pPr`（`w:pPrChange` 裡的修訂殘影
///   不收——同正文解析器只認 `w:pPr` 深度 1 的口徑）。
///
/// 取捨：`w:numPr` 缺 `w:numId` → 不收（沒有編號引用這件事）；缺 `w:ilvl`
/// → 視為 0（ECMA 預設）；`w:numId w:val="0"` → 照收（ECMA：0＝取消編號）。
pub fn parse_paragraph_numbering(document_xml: &[u8]) -> Vec<NumberingRef> {
    let text = String::from_utf8_lossy(document_xml).into_owned();
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut out: Vec<NumberingRef> = Vec::new();
    // 開啟中的元素 local-name 堆疊（棧頂＝當前元素）。
    let mut open: Vec<Vec<u8>> = Vec::new();
    // `mc:Fallback` 深度（>0＝吞沒子樹）。
    let mut fb = 0usize;
    // `mc:AlternateContent` 巢狀快照。
    let mut ac_stack: Vec<usize> = Vec::new();
    // 已開段落數（下一個段落索引＝此值）。
    let mut p_count = 0usize;
    // K4e2：`w:txbxContent` 文字框子樹深度：>0 時其內 `w:p` **不佔槽**（主
    // 解析器改掛外層段落的 `txbx_texts`）——口徑鐵律同 drawing.rs。
    let mut txbx_depth = 0usize;
    // 開啟中的段落索引堆疊（巢狀文字框段落時取最內層）。
    let mut para_stack: Vec<usize> = Vec::new();
    // 相對目前段落的 `w:pPr` 巢深（1＝段落直接子層；2＝w:pPrChange 內）。
    let mut ppr_depth = 0usize;
    // 進行中的 `w:numPr`：(ilvl, num_id)。
    let mut pending: Option<(Option<u32>, Option<u32>)> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if fb > 0 {
                    if name == b"Fallback" {
                        fb += 1;
                    } else if name == b"AlternateContent" {
                        ac_stack.push(fb);
                    }
                    open.push(name);
                    continue;
                }
                match name.as_slice() {
                    // MC 雙份表示：Choice 透明、Fallback 子樹整棵吞沒
                    // （口徑同正文解析器 `parse_document_xml`——雙份文字框
                    // 段落不得出現兩次）。
                    b"Fallback" => {
                        fb += 1;
                    }
                    b"AlternateContent" => {
                        ac_stack.push(fb);
                    }
                    // K4e2：文字框子樹深度（口徑同 p_count：Fallback 內不跟）。
                    b"txbxContent" => txbx_depth += 1,
                    b"p" => {
                        // Start 即預位：扁平索引＝Start 序（口徑鐵律）。
                        // K4e2：框內段落不佔槽——push 哨兵（End 照常配對 pop），
                        // 其 numPr 因索引哨兵而老實不收（主解析器框內不消費 pPr）。
                        if txbx_depth == 0 {
                            p_count += 1;
                            para_stack.push(p_count - 1);
                        } else {
                            para_stack.push(usize::MAX);
                        }
                        ppr_depth = 0;
                    }
                    b"pPr" => {
                        ppr_depth = if open.last().map(|v| v.as_slice()) == Some(b"p") {
                            1
                        } else {
                            ppr_depth + 1
                        };
                    }
                    b"numPr" => {
                        if ppr_depth == 1 && open.last().map(|v| v.as_slice()) == Some(b"pPr") {
                            pending = Some((None, None));
                        }
                    }
                    b"ilvl" => {
                        if let Some(p) = pending.as_mut() {
                            p.0 = attr_u32(&e, "val");
                        }
                    }
                    b"numId" => {
                        if let Some(p) = pending.as_mut() {
                            p.1 = attr_u32(&e, "val");
                        }
                    }
                    _ => {}
                }
                open.push(name);
            }
            Ok(Event::Empty(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if fb > 0 {
                    continue;
                }
                // 自閉合葉元素（`w:ilvl`／`w:numId` 的常態寫法）。
                match name.as_slice() {
                    b"ilvl" => {
                        if let Some(p) = pending.as_mut() {
                            p.0 = attr_u32(&e, "val");
                        }
                    }
                    b"numId" => {
                        if let Some(p) = pending.as_mut() {
                            p.1 = attr_u32(&e, "val");
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if name == b"AlternateContent" {
                    fb = ac_stack.pop().unwrap_or(0);
                    open.pop();
                    continue;
                }
                if fb > 0 {
                    if name == b"Fallback" {
                        fb = fb.saturating_sub(1);
                    }
                    open.pop();
                    continue;
                }
                match name.as_slice() {
                    // K4e2：文字框子樹完結（先於各分支，與計數點同深度口徑）。
                    b"txbxContent" => txbx_depth = txbx_depth.saturating_sub(1),
                    b"pPr" => {
                        ppr_depth = ppr_depth.saturating_sub(1);
                    }
                    b"numPr" => {
                        if let Some((ilvl, num_id)) = pending.take() {
                            // K4e2：框內段落（哨兵索引）的 numPr 老實不收。
                            if let (Some(num_id), Some(para)) = (num_id, para_stack.last().copied())
                            {
                                if para != usize::MAX {
                                    out.push(NumberingRef {
                                        para_index: para,
                                        ilvl: ilvl.unwrap_or(0),
                                        num_id,
                                    });
                                }
                            }
                        }
                    }
                    b"p" => {
                        para_stack.pop();
                        ppr_depth = 0;
                    }
                    _ => {}
                }
                open.pop();
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            // XML 畸形：老實返回已收集的部分（不 panic）。
            Err(_) => break,
        }
    }
    out
}

impl Numbering {
    /// 從整包 parts 解析編號定義（缺 `word/numbering.xml` ＝空彙總）。
    ///
    /// part 取法：**只用慣例路徑** `word/numbering.xml`（Word／LO 產出恆為
    /// 此名）；不走 `document.xml.rels` 關聯解析——異名落點老實略過＝空彙總
    /// （同 [`crate::notes`] 的註 part 取法）。
    pub fn from_parts(parts: &BTreeMap<String, Vec<u8>>) -> Numbering {
        parts
            .get(NUMBERING_PART)
            .map(|b| parse_numbering(b))
            .unwrap_or_default()
    }

    /// 依 id 查抽象定義（**同 id 多筆時首見為準**；查不到＝`None`）。
    pub fn abstract_num(&self, id: u32) -> Option<&AbstractNum> {
        self.abstract_nums.iter().find(|a| a.abstract_num_id == id)
    }

    /// 依 id 查編號實例（**同 id 多筆時首見為準**；查不到＝`None`）。
    pub fn num(&self, num_id: u32) -> Option<&NumInstance> {
        self.nums.iter().find(|n| n.num_id == num_id)
    }

    /// `numId` → **有效層級表**（num 層覆寫 abstract 後的合成結果；純函式）。
    ///
    /// 合成規則（ECMA-376 §17.9.6）：
    /// - 底本＝`w:num` 指向的 `w:abstractNum` 的各 `w:lvl`；
    /// - `w:lvlOverride` 帶整層 `w:lvl` → **整層取代**該層；
    /// - `w:lvlOverride` 只帶 `w:startOverride` → 沿用抽象層、僅覆寫起始編號；
    /// - 兩者並存 → 整層取代後再以 `w:startOverride` 覆寫起始編號。
    ///
    /// 回 `None` 的情形（老實，不虛構）：`numId` 不存在、該 `w:num` 缺
    /// `w:abstractNumId`、或指向的 `w:abstractNum` 不存在。覆寫若在抽象層
    /// 找不到對應層且自身無 `w:lvl`，該覆寫視為無效（不憑空造層）。
    pub fn effective_levels(&self, num_id: u32) -> Option<Vec<Level>> {
        let inst = self.num(num_id)?;
        let abs_id = inst.abstract_num_id?;
        let abs = self.abstract_num(abs_id)?;
        let mut levels: Vec<Level> = abs.levels.clone();
        for ovr in &inst.overrides {
            // 覆寫層：整層 `w:lvl` 取代該層；只有 `w:startOverride` 則沿用
            // 抽象層、僅覆寫起始編號。
            let mut lvl = match &ovr.level {
                Some(l) => l.clone(),
                None => match levels.iter().find(|l| l.ilvl == ovr.ilvl) {
                    Some(l) => l.clone(),
                    // 抽象層沒有對應層、覆寫也沒帶 `w:lvl` → 該覆寫無效
                    // （老實跳過，不憑空造層）。
                    None => continue,
                },
            };
            lvl.ilvl = ovr.ilvl;
            if let Some(s) = ovr.start_override {
                lvl.start = Some(s);
            }
            match levels.iter_mut().find(|l| l.ilvl == ovr.ilvl) {
                Some(slot) => *slot = lvl,
                None => levels.push(lvl),
            }
        }
        levels.sort_by_key(|l| l.ilvl);
        Some(levels)
    }
}

/// `w:numFmt/@w:val` → [`NumberFormat`]：**僅 corpus 實見值**；未見值 None。
fn parse_number_format(v: &str) -> Option<NumberFormat> {
    match v.trim() {
        "decimal" => Some(NumberFormat::Decimal),
        "decimalZero" => Some(NumberFormat::DecimalZero),
        "lowerLetter" => Some(NumberFormat::LowerLetter),
        "upperLetter" => Some(NumberFormat::UpperLetter),
        "lowerRoman" => Some(NumberFormat::LowerRoman),
        "upperRoman" => Some(NumberFormat::UpperRoman),
        "bullet" => Some(NumberFormat::Bullet),
        "ordinal" => Some(NumberFormat::Ordinal),
        "arabicAlpha" => Some(NumberFormat::ArabicAlpha),
        "none" => Some(NumberFormat::None),
        "custom" => Some(NumberFormat::Custom),
        _ => None,
    }
}

/// `w:multiLevelType/@w:val` → [`MultiLevelType`]：**僅 corpus 實見值**。
fn parse_multi_level_type(v: &str) -> Option<MultiLevelType> {
    match v.trim() {
        "singleLevel" => Some(MultiLevelType::SingleLevel),
        "multilevel" => Some(MultiLevelType::MultiLevel),
        "hybridMultilevel" => Some(MultiLevelType::HybridMultilevel),
        _ => None,
    }
}

/// twips → pt（`w:ind` 的原始單位是 twips；1pt ＝ 20twips）。
fn twips_to_pt(v: Option<String>) -> Option<f32> {
    let t = v?.trim().parse::<f64>().ok()?;
    if !t.is_finite() {
        return None;
    }
    Some((t / 20.0) as f32)
}

/// 屬性取整數（`@w:ilvl`／`@w:numId`／`@w:abstractNumId`）。
fn attr_u32(e: &quick_xml::events::BytesStart, key: &str) -> Option<u32> {
    crate::attr_val(e, key).and_then(|v| v.trim().parse::<u32>().ok())
}

/// 屬性取整數（`@w:start`／`@w:startOverride` 的 `w:val`）。
fn attr_i64(e: &quick_xml::events::BytesStart, key: &str) -> Option<i64> {
    crate::attr_val(e, key).and_then(|v| v.trim().parse::<i64>().ok())
}

/// 屬性取字串（`@w:val`／`@w:format`）。
fn attr_val(e: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
    crate::attr_val(e, key)
}

/// 從整包 parts 解析正文段落編號引用（缺 `word/document.xml` ＝空清單）。
pub fn paragraph_numbering_from_parts(parts: &BTreeMap<String, Vec<u8>>) -> Vec<NumberingRef> {
    parts
        .get(DOCUMENT_PART)
        .map(|b| parse_paragraph_numbering(b))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn corpus() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
    }

    fn parse_corpus(rel: &str) -> crate::OoxmlDoc {
        let bytes = std::fs::read(corpus().join(rel)).unwrap_or_else(|e| panic!("讀 {rel}: {e}"));
        crate::parse(&bytes).expect("解析 corpus 檔")
    }

    // ---------------- (a) HC031：抽象層級解析 ----------------

    /// HC031：`w:num@w:numId=1` → `w:abstractNumId=3` → 其 `w:lvl`（ilvl=0）
    /// ＝ decimal ＋ `lvlText="%1)"` ＋ `w:ind` left=360/hanging=360 twips
    ///（＝18pt／18pt）。
    #[test]
    fn hc031_numid1_abstract_lvl0_parsed() {
        let doc = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let numbering = Numbering::from_parts(&doc.parts);
        let inst = numbering.num(1).expect("numId=1 存在");
        assert_eq!(inst.abstract_num_id, Some(3), "numId=1 → abstractNumId=3");
        let abs = numbering.abstract_num(3).expect("abstractNumId=3 存在");
        let lvl0 = abs.levels.iter().find(|l| l.ilvl == 0).expect("lvl0");
        assert_eq!(lvl0.num_fmt, Some(NumberFormat::Decimal));
        assert_eq!(lvl0.lvl_text.as_deref(), Some("%1)"));
        assert_eq!(lvl0.jc, Some(Align::Left));
        assert_eq!(lvl0.ind_left_pt, Some(18.0), "原始 twips=360（÷20=18pt）");
        assert_eq!(
            lvl0.ind_hanging_pt,
            Some(18.0),
            "原始 twips=360（÷20=18pt）"
        );
    }

    /// HC031：`decimal` ＋ `lvlText="%1."` ＋ left=720/hanging=360 twips
    ///（＝36pt／18pt）這組值，實屬 **numId=4 → abstractNumId=0**（非 numId=1）。
    /// 兩組都鎖，避免消費端（K3）拿錯層。
    #[test]
    fn hc031_numid4_abstract0_lvl0_parsed() {
        let doc = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let numbering = Numbering::from_parts(&doc.parts);
        let inst = numbering.num(4).expect("numId=4 存在");
        assert_eq!(inst.abstract_num_id, Some(0));
        let abs = numbering.abstract_num(0).expect("abstractNumId=0 存在");
        let lvl0 = abs.levels.iter().find(|l| l.ilvl == 0).expect("lvl0");
        assert_eq!(lvl0.num_fmt, Some(NumberFormat::Decimal));
        assert_eq!(lvl0.lvl_text.as_deref(), Some("%1."));
        assert_eq!(lvl0.ind_left_pt, Some(36.0), "原始 twips=720（÷20=36pt）");
        assert_eq!(
            lvl0.ind_hanging_pt,
            Some(18.0),
            "原始 twips=360（÷20=18pt）"
        );
    }

    /// HC031 四個 numId 的格式分佈：numId=1→decimal、2→bullet、3→upperRoman、
    /// 4→decimal；bullet 層的 `w:lvlText` 是 U+F0B7（Symbol 字型私用區字元）
    /// ——**原樣存**、不轉成「•」。
    #[test]
    fn hc031_all_numids_formats_and_bullet_raw_text() {
        let doc = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let numbering = Numbering::from_parts(&doc.parts);
        let fmt_of = |num_id: u32| {
            let eff = numbering.effective_levels(num_id).expect("有效層級表");
            eff.iter().find(|l| l.ilvl == 0).and_then(|l| l.num_fmt)
        };
        assert_eq!(fmt_of(1), Some(NumberFormat::Decimal));
        assert_eq!(fmt_of(2), Some(NumberFormat::Bullet));
        assert_eq!(fmt_of(3), Some(NumberFormat::UpperRoman));
        assert_eq!(fmt_of(4), Some(NumberFormat::Decimal));
        let eff2 = numbering.effective_levels(2).expect("numId=2 有效層級表");
        let l0 = eff2.iter().find(|l| l.ilvl == 0).expect("lvl0");
        assert_eq!(
            l0.lvl_text.as_deref(),
            Some("\u{f0b7}"),
            "項目符號字元原樣存（不可換成 •）"
        );
        // numId=2 的 ilvl=1 是 Courier New 的 "o"（LO 渲染的 `o` 來源）
        let l1 = eff2.iter().find(|l| l.ilvl == 1).expect("lvl1");
        assert_eq!(l1.num_fmt, Some(NumberFormat::Bullet));
        assert_eq!(l1.lvl_text.as_deref(), Some("o"));
    }

    // ---------------- (b) 段落引用 ----------------

    /// HC031：正文 `w:pPr/w:numPr` 恰 30 筆；首筆為段落索引 18、ilvl=0、
    /// numId=1；索引口徑與 `OoxmlDoc::paragraphs` 同序（30 筆全落在
    /// ListParagraph 樣式的段上）。
    #[test]
    fn hc031_paragraph_numbering_refs() {
        let doc = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let refs = paragraph_numbering_from_parts(&doc.parts);
        assert_eq!(refs.len(), 30, "HC031 的 numPr 段落數");
        // K4e2：HC031 前段有 3 個文字框段落退出正文槽（掛 txbx_texts），
        // 清單段索引整體前移 2（18→16；兩框段在首個 numPr 段之前）。
        assert_eq!(
            refs[0],
            NumberingRef {
                para_index: 16,
                ilvl: 0,
                num_id: 1
            },
            "首筆段落引用"
        );
        // 索引對齊：每一筆都落在 ListParagraph 樣式的段落上（Word 的清單段
        // 一律套 ListParagraph）——索引錯位立刻現形。
        for r in &refs {
            let p = doc.paragraphs.get(r.para_index).unwrap_or_else(|| {
                panic!(
                    "段落索引 {} 越界（總數 {}）",
                    r.para_index,
                    doc.paragraphs.len()
                )
            });
            assert_eq!(
                p.style.as_deref(),
                Some("ListParagraph"),
                "段落 {} 的樣式應為 ListParagraph",
                r.para_index
            );
        }
        // 分佈：numId=1×10、2×5、3×10、4×5；ilvl 0/1/2 三級。
        let by_num = |n: u32| refs.iter().filter(|r| r.num_id == n).count();
        assert_eq!((by_num(1), by_num(2), by_num(3), by_num(4)), (10, 5, 10, 5));
        assert_eq!(refs.iter().filter(|r| r.ilvl == 2).count(), 1);
    }

    // ---------------- (c) 覆寫（lvlOverride／startOverride） ----------------

    /// POI 覆寫檔：`w:num@w:numId=22` → abstractNumId=9，帶 9 個
    /// `w:lvlOverride`；ilvl=0 的覆寫帶整層 `w:lvl`（upperLetter／`%1`），
    /// 與 abstract 的 decimal／`%1` 不同——合成後有效層級取覆寫值。
    #[test]
    fn level_override_replaces_abstract_level() {
        let doc = parse_corpus("poi/test-data_document_NumberingWOverrides.docx");
        let numbering = Numbering::from_parts(&doc.parts);
        let inst = numbering.num(22).expect("numId=22 存在");
        assert_eq!(inst.abstract_num_id, Some(9));
        assert_eq!(inst.overrides.len(), 9, "ilvl 0–8 各一");
        let ovr0 = inst
            .overrides
            .iter()
            .find(|o| o.ilvl == 0)
            .expect("ilvl0 覆寫");
        let ovr_lvl = ovr0.level.as_ref().expect("覆寫帶整層 w:lvl");
        assert_eq!(ovr_lvl.num_fmt, Some(NumberFormat::UpperLetter));
        assert_eq!(ovr_lvl.lvl_text.as_deref(), Some("%1"));
        // 合成：有效 lvl0 ＝ 覆寫層（不是 abstract 的 decimal）
        let eff = numbering.effective_levels(22).expect("有效層級表");
        let l0 = eff.iter().find(|l| l.ilvl == 0).expect("lvl0");
        assert_eq!(l0.num_fmt, Some(NumberFormat::UpperLetter));
        assert_eq!(l0.lvl_text.as_deref(), Some("%1"));
        // 沒有覆寫的層（如 ilvl=8 若無覆寫）沿用 abstract——本檔 9 層全覆寫，
        // 改驗「覆寫層之外，abstract 本身的層內容可解析」。
        let abs9 = numbering.abstract_num(9).expect("abstractNumId=9 存在");
        let a0 = abs9
            .levels
            .iter()
            .find(|l| l.ilvl == 0)
            .expect("abstract lvl0");
        assert_eq!(a0.num_fmt, Some(NumberFormat::Decimal), "abstract 底本未變");
    }

    /// RA001 修訂檔：`w:num@w:numId=30` 的 ilvl=0 覆寫帶
    /// `w:startOverride w:val="6"`——收錄生效（有效層級起始＝6）。
    #[test]
    fn start_override_recorded_and_applied() {
        let doc = parse_corpus("xopt/TestFiles_RA001-Tracked-Revisions-01.docx");
        let numbering = Numbering::from_parts(&doc.parts);
        let inst = numbering.num(30).expect("numId=30 存在");
        let ovr0 = inst
            .overrides
            .iter()
            .find(|o| o.ilvl == 0)
            .expect("ilvl0 覆寫");
        assert_eq!(ovr0.start_override, Some(6), "startOverride 收錄");
        assert!(ovr0.level.is_none(), "本檔覆寫只帶 startOverride");
        let eff = numbering.effective_levels(30).expect("有效層級表");
        let l0 = eff.iter().find(|l| l.ilvl == 0).expect("lvl0");
        assert_eq!(l0.start, Some(6));
        assert_eq!(
            l0.num_fmt,
            Some(NumberFormat::Decimal),
            "其餘欄位沿用 abstract"
        );
    }

    /// 合成語義（純函式）：`w:startOverride` 覆寫抽象層的起始編號——
    /// corpus 實檔的 startOverride 恰與 abstract 同值，故以合成 XML 驗語義。
    #[test]
    fn start_override_wins_over_abstract_start() {
        let xml = br#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:abstractNum w:abstractNumId="7">
            <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl>
          </w:abstractNum>
          <w:num w:numId="3"><w:abstractNumId w:val="7"/>
            <w:lvlOverride w:ilvl="0"><w:startOverride w:val="9"/></w:lvlOverride>
          </w:num>
        </w:numbering>"#;
        let numbering = parse_numbering(xml);
        let eff = numbering.effective_levels(3).expect("有效層級表");
        let l0 = eff.iter().find(|l| l.ilvl == 0).expect("lvl0");
        assert_eq!(l0.start, Some(9), "startOverride 蓋掉 abstract 的 1");
        assert_eq!(l0.lvl_text.as_deref(), Some("%1."), "其餘欄位沿用 abstract");
    }

    // ---------------- (d) 病態輸入 ----------------

    /// 病態輸入樣樣不炸：壞 XML 停在已收部分；`w:numId` 指向不存在的
    /// abstractNum ＝有效層級 `None`；`w:ilvl` 越界照收不炸。
    #[test]
    fn pathological_inputs_do_not_panic() {
        // 壞 XML（未閉合）→ 空彙總、不 panic。
        let broken = parse_numbering(b"<w:numbering><w:abstractNum w:abstractNumId=\"0\"><w:lvl");
        assert!(broken.abstract_nums.is_empty() || broken.abstract_nums[0].levels.is_empty());
        assert!(parse_numbering(b"not xml at all").abstract_nums.is_empty());
        // numId 指向不存在的 abstractNum → 有效層級 None（老實，不虛構）。
        let orphan = parse_numbering(
            br#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                 <w:num w:numId="5"><w:abstractNumId w:val="99"/></w:num>
               </w:numbering>"#,
        );
        assert_eq!(orphan.num(5).and_then(|n| n.abstract_num_id), Some(99));
        assert_eq!(orphan.effective_levels(5), None, "查不到抽象層 → None");
        assert_eq!(orphan.effective_levels(404), None, "numId 不存在 → None");
        // ilvl 越界（9 / 負值）→ 照收不炸，且合成表照樣可查。
        let oob = parse_numbering(
            br#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                 <w:abstractNum w:abstractNumId="0">
                   <w:lvl w:ilvl="9"><w:numFmt w:val="decimal"/></w:lvl>
                   <w:lvl w:ilvl="-1"><w:numFmt w:val="decimal"/></w:lvl>
                 </w:abstractNum>
                 <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
               </w:numbering>"#,
        );
        let eff = oob.effective_levels(1).expect("有效層級表");
        assert!(eff.iter().any(|l| l.ilvl == 9), "越界 ilvl 照收");
        // 段落引用：numPr 缺 numId → 不收；缺 ilvl → 視為 0。
        let refs = parse_paragraph_numbering(
            br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
                 <w:p><w:pPr><w:numPr><w:ilvl w:val="2"/></w:numPr></w:pPr></w:p>
                 <w:p><w:pPr><w:numPr><w:numId w:val="4"/></w:numPr></w:pPr></w:p>
                 <w:p><w:pPr><w:numPr><w:ilvl w:val="x"/><w:numId w:val="9"/></w:numPr></w:pPr></w:p>
               </w:body></w:document>"#,
        );
        assert_eq!(refs.len(), 2, "缺 numId 者不收、壞 ilvl 者不收");
        assert_eq!(
            refs[0],
            NumberingRef {
                para_index: 1,
                ilvl: 0,
                num_id: 4
            },
            "缺 ilvl → 視為 0；缺 numId 的第 0 段不收"
        );
        assert_eq!(refs[1].num_id, 9);
    }

    /// `mc:Fallback` 子樹內的段落編號不計（雙份表示去重口徑，同正文解析器）；
    /// 自閉合 `<w:p/>` 不佔段落索引（口徑鐵律）。
    #[test]
    fn paragraph_index_matches_engine_flat_convention() {
        let doc = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let refs = paragraph_numbering_from_parts(&doc.parts);
        // K4e2：文字框段落不再佔正文槽（131→128；3 個框內段掛 txbx_texts）。
        assert_eq!(doc.paragraphs.len(), 128, "HC031 扁平段落數（引擎口徑）");
        assert!(
            refs.iter().all(|r| r.para_index < 128),
            "段落索引須落在引擎段落序列內"
        );
        // 巢狀文字框內的段落不影響外層索引：正文第一個 numPr 段落是索引 16。
        assert_eq!(refs[0].para_index, 16);
    }

    // ---------------- (e) serde 向後相容 ----------------

    /// 新結構往返（純新增型別，未進任何既有 struct）；既有 `OoxmlDoc` 的
    /// 序列化輸出**零改動**（沒有新增欄位）——本卡對既有快照無影響。
    #[test]
    fn numbering_serde_roundtrip_and_existing_snapshot_untouched() {
        let doc = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let numbering = Numbering::from_parts(&doc.parts);
        let j = serde_json::to_string(&numbering).expect("序列化 Numbering");
        let back: Numbering = serde_json::from_str(&j).expect("反序列化 Numbering");
        assert_eq!(back, numbering, "新結構必須可往返");
        let refs = paragraph_numbering_from_parts(&doc.parts);
        let rj = serde_json::to_string(&refs).expect("序列化段落引用");
        let rback: Vec<NumberingRef> = serde_json::from_str(&rj).expect("反序列化段落引用");
        assert_eq!(rback, refs);
        // 既有 OoxmlDoc 快照：不得因本卡多出欄位。
        let v = serde_json::to_value(&doc).expect("序列化 OoxmlDoc");
        let obj = v.as_object().expect("obj");
        assert!(
            !obj.contains_key("numbering") && !obj.contains_key("numbering_refs"),
            "既有 struct 不得被塞新欄位：{:?}",
            obj.keys().collect::<Vec<_>>()
        );
    }

    // ---------------- (f) 無 numbering.xml 的檔 ----------------

    /// WC049（文字框在儲存格內）**沒有** `word/numbering.xml` → 空彙總、
    /// 不 panic、查不到有效層級；段落引用掃描照跑（該檔無 numPr → 空）。
    #[test]
    fn missing_numbering_part_yields_empty() {
        let doc = parse_corpus("xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx");
        assert!(
            !doc.parts.contains_key(NUMBERING_PART),
            "前置條件：本測試檔必須沒有 numbering part"
        );
        let numbering = Numbering::from_parts(&doc.parts);
        assert_eq!(numbering, Numbering::default());
        assert_eq!(numbering.effective_levels(1), None);
        assert!(paragraph_numbering_from_parts(&doc.parts).is_empty());
    }

    /// `mc:numFmt` 只取 `mc:Choice` 那份（`mc:Fallback` 子樹吞沒）——雙份
    /// 表示去重口徑同全 crate。corpus 實例：POI 檔的 `w:numFmt w:val="custom"`
    /// 在 `mc:Choice` 內、`mc:Fallback` 是 `decimal` 備援。
    #[test]
    fn mc_choice_wins_over_fallback_for_num_fmt() {
        let doc = parse_corpus("poi/test-data_document_Numbering.docx");
        let numbering = Numbering::from_parts(&doc.parts);
        let abs = numbering.abstract_num(3).expect("abstractNumId=3 存在");
        let lvl0 = abs.levels.iter().find(|l| l.ilvl == 0).expect("lvl0");
        assert_eq!(lvl0.num_fmt, Some(NumberFormat::Custom));
        assert_eq!(
            lvl0.num_fmt_format.as_deref(),
            Some("α, β, γ, ..."),
            "custom 的格式字串（@w:format）也要收"
        );
    }

    // ---------------- (g) 樣式連結（老實記錄、不解析） ----------------

    /// `w:numStyleLink`／`w:styleLink`（§17.9.21／§17.9.22）只記錄字串、
    /// 不解析樣式鏈；該檔的 abstractNum 全是 numStyleLink 指向同一樣式。
    #[test]
    fn style_links_recorded_but_not_resolved() {
        let doc = parse_corpus("xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-05.docx");
        let numbering = Numbering::from_parts(&doc.parts);
        assert!(!numbering.abstract_nums.is_empty());
        let abs0 = numbering.abstract_num(0).expect("abstractNumId=0 存在");
        assert_eq!(abs0.num_style_link.as_deref(), Some("Bulletedlist"));
        assert!(
            abs0.levels.is_empty(),
            "只有 numStyleLink 的抽象層沒有自己的 w:lvl"
        );
        assert!(
            numbering.nums.iter().all(|n| n.abstract_num_id.is_some()),
            "w:num 仍各自指向抽象定義"
        );
    }

    // ---------------- (h) K2b 補收欄位 ----------------

    /// 自製 numbering.xml：`w:rPr/w:rFonts@ascii`、`w:numRestart`、
    /// `w:lvlRestart`、`w:suff` 四欄全收（皆 Option，缺＝None）。
    #[test]
    fn k2b_level_extra_fields_parsed() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="7">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="bullet"/>
      <w:numRestart w:val="eachSect"/>
      <w:lvlRestart w:val="0"/>
      <w:suff w:val="nothing"/>
      <w:lvlText w:val="&#xF0B7;"/>
      <w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol"/></w:rPr>
    </w:lvl>
    <w:lvl w:ilvl="1">
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%2."/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="3"><w:abstractNumId w:val="7"/></w:num>
</w:numbering>"#;
        let n = parse_numbering(xml);
        let lv0 = n
            .effective_levels(3)
            .expect("有效層級")
            .into_iter()
            .find(|l| l.ilvl == 0)
            .expect("lvl0");
        assert_eq!(lv0.rpr_font_ascii.as_deref(), Some("Symbol"));
        assert_eq!(lv0.num_restart.as_deref(), Some("eachSect"));
        assert_eq!(lv0.lvl_restart, Some(0));
        assert_eq!(lv0.suff.as_deref(), Some("nothing"));
        let lv1 = n
            .effective_levels(3)
            .expect("有效層級")
            .into_iter()
            .find(|l| l.ilvl == 1)
            .expect("lvl1");
        assert_eq!(lv1.rpr_font_ascii, None, "未宣告＝None");
        assert_eq!(lv1.num_restart, None);
        assert_eq!(lv1.lvl_restart, None);
        assert_eq!(lv1.suff, None);
    }

    /// corpus 實檔：HC031 的 Symbol 字型、HC061 的 suff、RP052 的 lvlRestart=0
    /// 都必須原樣收進 [`Level`]。
    #[test]
    fn k2b_corpus_extra_fields_parsed() {
        let hc031 = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let n = Numbering::from_parts(&hc031.parts);
        let bullet = n
            .effective_levels(2)
            .expect("numId=2")
            .into_iter()
            .find(|l| l.ilvl == 0)
            .expect("lvl0");
        assert_eq!(bullet.rpr_font_ascii.as_deref(), Some("Symbol"));

        let hc061 = parse_corpus("xopt/TestFiles_HC061-Hyperlink-in-Field.docx");
        let n = Numbering::from_parts(&hc061.parts);
        let suffs: Vec<&str> = n
            .abstract_nums
            .iter()
            .flat_map(|a| a.levels.iter())
            .filter_map(|l| l.suff.as_deref())
            .collect();
        assert!(suffs.contains(&"space"), "HC061 實見 suff=space：{suffs:?}");
        assert!(
            suffs.contains(&"nothing"),
            "HC061 實見 suff=nothing：{suffs:?}"
        );

        let rp052 = parse_corpus("xopt/TestFiles_RP_RP052-Deleted-Para-Mark-Rejected.docx");
        let n = Numbering::from_parts(&rp052.parts);
        let restarts: Vec<u32> = n
            .abstract_nums
            .iter()
            .flat_map(|a| a.levels.iter())
            .filter_map(|l| l.lvl_restart)
            .collect();
        assert!(!restarts.is_empty(), "RP052 實見 lvlRestart");
        assert!(restarts.iter().all(|v| *v == 0), "本檔全為 0＝永不重啟");
    }
}
