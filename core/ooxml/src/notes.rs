//! 註腳／尾註解析（`word/footnotes.xml`、`word/endnotes.xml`；ECMA-376 §17.11）。
//!
//! 背景：註腳本體放在獨立 part（`w:footnote`／`w:endnote` 元素，各以 `w:id`
//! 與正文 `w:footnoteReference`／`w:endnoteReference` 對應）。render-lab 對
//! LO 基線時 LO 渲染了註腳、本引擎完全跳過，造成分頁位移——本卡先**讀進
//! 資料**，版面與發射（F2 卡）後續接手；本模組對顯示列表／測量**零消費**
//! （渲染輸出逐位元組零變化）。
//!
//! 容錯口徑同 [`crate::ThemeColors`]：缺 part／壞 XML ＝空彙總（老實不虛構），
//! 逐註取捨見 [`Notes::from_parts`] 與 [`parse_note_list`]。

use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::Paragraph;

/// 註腳 part 的慣例路徑（OPC 佈局：Word／LO 產出恆為此名）。
const FOOTNOTES_PART: &str = "word/footnotes.xml";
/// 尾註 part 的慣例路徑。
const ENDNOTES_PART: &str = "word/endnotes.xml";
/// settings part 的慣例路徑（`w:footnotePr`／`w:endnotePr` 所在；ECMA-376 §17.11）。
const SETTINGS_PART: &str = "word/settings.xml";

/// 註的型別（`w:footnote`／`w:endnote` 的 `@w:type`；ECMA-376 §17.11.10/§17.11.11）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NoteKind {
    /// 一般註（`@w:type` 缺省＝此型；正文的引用都指向它）。
    #[default]
    Regular,
    /// 分隔線註（`separator`，Word 慣例 id=-1，`<w:separator/>` 繪上緣線）。
    Separator,
    /// 續接分隔線註（`continuationSeparator`，Word 慣例 id=0）。
    ContinuationSeparator,
}

/// 引用端型別（正文 `w:footnoteReference`／`w:endnoteReference` 的對應；
/// ECMA-376 §17.11.2）。與 [`NoteKind`] 分開——separator 型不會被引用，
/// 引用標記只承載「腳註／尾註」兩分法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteKind2 {
    /// `w:footnoteReference` → `word/footnotes.xml`。
    Footnote,
    /// `w:endnoteReference` → `word/endnotes.xml`。
    Endnote,
}

/// 註腳／尾註的編號格式（`w:numFmt/@w:val`；ECMA-376 §17.11.17/§17.11.18）。
///
/// **僅收錄 corpus 實見值**（decimal／lowerRoman／chicago）；未見值於解析時
/// 老實歸 [`NotePr::num_fmt`] 的 None，不憑 ECMA 全集臆造（見
/// [`parse_num_fmt`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteNumberFormat {
    /// `decimal`（阿拉伯數字）。
    Decimal,
    /// `lowerRoman`（小寫羅馬數字）。
    LowerRoman,
    /// `chicago`（Chicago 註腳符號序列）。
    Chicago,
}

/// 註腳／尾註編號重啟時機（`w:numRestart/@w:val`；ECMA-376 §17.11.19）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteRestart {
    /// `continuous`：整份文件連續編號。
    Continuous,
    /// `eachPage`：每頁重新編號。
    EachPage,
    /// `eachSect`：每節重新編號。
    EachSect,
}

/// 註腳／尾註位置（`w:pos/@w:val`；ECMA-376 §17.11.14/§17.11.21）。
///
/// 值域分流：註腳為 `beneathText`／`pageBottom`；尾註為 `documentEnd`／
/// `pageEnd`／`sectEnd`。解析層不強制 kind（同一枚舉共用），由消費端依所在
/// 層（settings 或 sectPr）與 kind 判別；未見值老實歸 [`NotePr::pos`] 的 None。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotePos {
    /// `beneathText`：註緊接正文下方（註腳值域）。
    BeneathText,
    /// `pageBottom`：註在頁底（註腳值域）。
    PageBottom,
    /// `documentEnd`：尾註置於文件結尾（尾註值域）。
    DocumentEnd,
    /// `pageEnd`：尾註置於頁尾（尾註值域）。
    PageEnd,
    /// `sectEnd`：尾註置於節尾（尾註值域）。
    SectEnd,
}

/// 註腳／尾註的編號與位置設定（ECMA-376 §17.11.5／§17.11.17）。
///
/// 兩個層級各帶一份：settings.xml 層＝文件預設；sectPr 層＝節覆寫（逐欄
/// 覆寫，非整組替換——未宣告的欄沿用上層，合併是消費端的事）。解析層只
/// 老實分層存，不做合併。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotePr {
    /// `w:numFmt/@w:val`（§17.11.17/§17.11.18）。未見值老實歸 None
    /// （不臆造 decimal）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_fmt: Option<NoteNumberFormat>,
    /// `w:numRestart/@w:val`（§17.11.19）：continuous|eachPage|eachSect。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_restart: Option<NoteRestart>,
    /// `w:numStart/@w:val`（§17.11.20；任務書寫作 `w:num`，兩者皆收）：
    /// 起始編號；缺＝None（消費端用 1）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_start: Option<i32>,
    /// `w:pos/@w:val`（§17.11.14/§17.11.21）：註腳 beneathText|pageBottom；
    /// 尾註 documentEnd|pageEnd|sectEnd。共用一枚舉以值域分流。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<NotePos>,
    /// `w:footnote/@w:id`／`w:endnote/@w:id` 中非特殊註的參考線 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<i64>,
    /// 分隔線註的 id（Word 慣例 -1；或 `w:type="separator"`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separator_id: Option<i64>,
    /// 續接分隔線註的 id（Word 慣例 0；或 `w:type="continuationSeparator"`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_separator_id: Option<i64>,
}

/// 一份文件的 settings.xml 層註設定（文件預設；ECMA-376 §17.11.5/§17.11.17）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteSettings {
    /// `w:footnotePr`（文件級腳註設定）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footnote: Option<NotePr>,
    /// `w:endnotePr`（文件級尾註設定）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endnote: Option<NotePr>,
}

/// 一則註的本體（ECMA-376 §17.11.3 `w:footnote`／§17.11.8 `w:endnote` 同構）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// `@w:id`。一般註腳 ≥1；Word 約定 separator=-1、continuationSeparator=0
    /// （`w:id` 屬 `ST_DecimalNumber` 整數域，i64 值域足夠；越界字面直接
    /// 解析失敗＝老實略註，見 [`parse_note_list`] 取捨）。
    #[serde(default)]
    pub id: i64,
    /// `@w:type`；缺省＝[`NoteKind::Regular`]。**取捨**：其餘型別
    /// （`continuation` 等）老實歸 Regular——corpus 未見該型；不憑名字
    /// 虛擬分發，等 corpus 出現實檔再依實據擴充。
    #[serde(default)]
    pub kind: NoteKind,
    /// 註文字段落（複用正文同款 `w:p` 全解析；引用來源＝本檔自身，
    /// 段落索引只在單一註內自洽）。段內 `w:footnoteRef`／`w:endnoteRef`
    /// 自動編號符與 `w:separator` 等**現階段不進 run 序**（落 catch-all
    /// 被忽略）——F2 發射符號時再定義其投影，本卡不虛構文字。
    #[serde(default)]
    pub paragraphs: Vec<Paragraph>,
}

impl Note {
    /// 註文字的合併純文字（各段落 run 串接；空註＝空字串）。
    pub fn text(&self) -> String {
        self.paragraphs
            .iter()
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect::<String>()
    }
}

/// 一份文件的全部註（兩個 part 各掃一份）。
///
/// 缺 part／壞 XML ＝空 [`Notes`]，等同 [`Notes::default`]；單一註解不出
/// 段落時**仍保留該註**（`paragraphs` 空）——老實：註的存在是事實，文字
/// 解析失敗是能力界線，不虛構也不整註丟棄。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Notes {
    /// `word/footnotes.xml` 的全部 `w:footnote`。
    #[serde(default)]
    pub footnotes: Vec<Note>,
    /// `word/endnotes.xml` 的全部 `w:endnote`。
    #[serde(default)]
    pub endnotes: Vec<Note>,
    /// settings.xml 層的文件級註設定（`w:footnotePr`／`w:endnotePr`；
    /// ECMA-376 §17.11.5/§17.11.17）。缺 settings part／元素＝None（容錯
    /// 口徑同本檔）。純解析層，版面／顯示列表零消費；serde default 保舊快照
    /// 相容。
    #[serde(default)]
    pub settings: NoteSettings,
}

impl Notes {
    /// 從整包 parts 解析兩個註 part（缺＝空；容錯口徑同
    /// [`crate::ThemeColors::from_parts`]）。
    ///
    /// 註 part 取法：**只用慣例路徑** `word/footnotes.xml`／
    /// `word/endnotes.xml`（Word／LO 產出恆為此名）；不走
    /// `document.xml.rels` 關聯解析——異名落點 → 老實略＝空彙總（見
    /// 模組末未做清單）。
    pub fn from_parts(parts: &BTreeMap<String, Vec<u8>>) -> Notes {
        // 註段落解析複用正文同款 `parse_document_xml`（w:themeColor 查表）
        // ——此處仿單次注入取一份主題色表（theme part 數 KB，重取成本
        // 可忽略），讓本函式簽名維持「一包 parts 進、彙總出」的形態。
        let theme = crate::ThemeColors::from_parts(parts);
        // settings.xml 層設定（缺 part／元素＝None；容錯口徑同本函式）。
        let settings = parts
            .get(SETTINGS_PART)
            .map(|b| parse_note_settings(b))
            .unwrap_or_default();
        Notes {
            footnotes: parts
                .get(FOOTNOTES_PART)
                .map(|b| parse_note_list(b, b"footnote", &theme))
                .unwrap_or_default(),
            endnotes: parts
                .get(ENDNOTES_PART)
                .map(|b| parse_note_list(b, b"endnote", &theme))
                .unwrap_or_default(),
            settings,
        }
    }

    /// 依 id 查一則腳註本體（**同 id 多註時首見為準**；查不到＝None，
    /// 不虛構預留位）。查詢不做 kind 過濾——separator(-1) 也查得到，
    /// 由呼叫端（F2）決定語義。
    pub fn footnote(&self, id: i64) -> Option<&Note> {
        self.footnotes.iter().find(|n| n.id == id)
    }

    /// 依 id 查尾註本體（同 [`Notes::footnote`] 口徑）。
    pub fn endnote(&self, id: i64) -> Option<&Note> {
        self.endnotes.iter().find(|n| n.id == id)
    }
}

/// 解析 settings.xml 的 `w:footnotePr`／`w:endnotePr`（文件級設定）。
///
/// 缺 `word/settings.xml` 或無對應元素 → 該欄 None（容錯口徑同
/// [`Notes::from_parts`]）。
pub fn parse_note_settings(settings_xml: &[u8]) -> NoteSettings {
    NoteSettings {
        footnote: parse_note_pr(settings_xml, b"footnotePr", b"footnote"),
        endnote: parse_note_pr(settings_xml, b"endnotePr", b"endnote"),
    }
}

/// 解析一個 `w:footnotePr`／`w:endnotePr` 元素（ECMA-376 §17.11.5/§17.11.17）。
///
/// `pr_tag`＝`footnotePr`／`endnotePr`；`child_tag`＝`footnote`／`endnote`。
/// 掃到第一個 `pr_tag` 元素（settings.xml 層恆唯一）；`<w:footnotePr/>`
/// 空組 → `Some(NotePr::default())`；元素不存在 → None；壞 XML → 停在已收
/// 部分（同 [`crate::ThemeColors`] 容錯口徑）。
fn parse_note_pr(xml: &[u8], pr_tag: &[u8], child_tag: &[u8]) -> Option<NotePr> {
    let text = String::from_utf8_lossy(xml);
    let mut re = quick_xml::Reader::from_str(&text);
    loop {
        let ev = match re.read_event() {
            Ok(ev) => ev,
            Err(_) => return None,
        };
        match ev {
            Event::Eof => return None,
            Event::Empty(e) if e.local_name().as_ref() == pr_tag => {
                return Some(NotePr::default());
            }
            Event::Start(e) if e.local_name().as_ref() == pr_tag => {
                let mut pr = NotePr::default();
                loop {
                    match re.read_event() {
                        Ok(Event::End(end)) if end.local_name().as_ref() == pr_tag => break,
                        Ok(Event::Eof) => break,
                        Ok(Event::Start(c)) | Ok(Event::Empty(c)) => {
                            apply_note_pr_child(&mut pr, &c, child_tag);
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
                return Some(pr);
            }
            _ => {}
        }
    }
}

/// 套用一個 `w:footnotePr`／`w:endnotePr` 子元素到 [`NotePr`]（編號格式、
/// 重啟、起始值、位置、分隔線引用）。`pub(crate)`：sectpr 逐節掃描時重用
/// 同一套分流口徑，不另造第二台狀態機。
pub(crate) fn apply_note_pr_child(
    pr: &mut NotePr,
    e: &quick_xml::events::BytesStart,
    child_tag: &[u8],
) {
    match e.local_name().as_ref() {
        b"numFmt" => {
            pr.num_fmt = crate::attr_val(e, "val").as_deref().and_then(parse_num_fmt);
        }
        b"numRestart" => {
            pr.num_restart = crate::attr_val(e, "val")
                .as_deref()
                .and_then(parse_num_restart);
        }
        // ECMA-376 正名 `w:numStart`（§17.11.20）；任務書寫作 `w:num`，兩者皆收。
        b"num" | b"numStart" => {
            pr.num_start = crate::attr_val(e, "val")
                .as_deref()
                .and_then(|v| v.trim().parse::<i32>().ok());
        }
        b"pos" => {
            pr.pos = crate::attr_val(e, "val").as_deref().and_then(parse_pos);
        }
        b"footnote" | b"endnote" if e.local_name().as_ref() == child_tag => {
            if let Some(id) = crate::attr_val(e, "id")
                .as_deref()
                .and_then(|v| v.trim().parse::<i64>().ok())
            {
                apply_note_ref(pr, id, crate::attr_val(e, "type").as_deref());
            }
        }
        _ => {}
    }
}

/// `w:numFmt/@w:val` → [`NoteNumberFormat`]：僅 corpus 實見值；未見值 None。
fn parse_num_fmt(v: &str) -> Option<NoteNumberFormat> {
    match v.trim() {
        "decimal" => Some(NoteNumberFormat::Decimal),
        "lowerRoman" => Some(NoteNumberFormat::LowerRoman),
        "chicago" => Some(NoteNumberFormat::Chicago),
        _ => None,
    }
}

/// `w:numRestart/@w:val` → [`NoteRestart`]：未見值 None。
fn parse_num_restart(v: &str) -> Option<NoteRestart> {
    match v.trim() {
        "continuous" => Some(NoteRestart::Continuous),
        "eachPage" => Some(NoteRestart::EachPage),
        "eachSect" => Some(NoteRestart::EachSect),
        _ => None,
    }
}

/// `w:pos/@w:val` → [`NotePos`]：未見值 None。
fn parse_pos(v: &str) -> Option<NotePos> {
    match v.trim() {
        "beneathText" => Some(NotePos::BeneathText),
        "pageBottom" => Some(NotePos::PageBottom),
        "documentEnd" => Some(NotePos::DocumentEnd),
        "pageEnd" => Some(NotePos::PageEnd),
        "sectEnd" => Some(NotePos::SectEnd),
        _ => None,
    }
}

/// 記錄一個 `w:footnote`／`w:endnote` 引用（ECMA-376 §17.11.9）。
///
/// `w:type` 存在時優先分流（`separator`／`continuationSeparator`）；無
/// `w:type` 時依 Word 慣例：id=-1＝分隔線、id=0＝續接分隔線，其餘當參考線。
/// （ECMA §17.11.9 的 `w:footnote` 只有 `w:id`，此處的 type 為容錯分支。）
fn apply_note_ref(pr: &mut NotePr, id: i64, ty: Option<&str>) {
    match ty.map(str::trim) {
        Some("separator") => pr.separator_id = Some(id),
        Some("continuationSeparator") => pr.continuation_separator_id = Some(id),
        _ => match id {
            -1 => pr.separator_id = Some(id),
            0 => pr.continuation_separator_id = Some(id),
            _ => pr.ref_id = Some(id),
        },
    }
}

/// run 上的引用標記（ECMA-376 §17.11.2：正文 run 內 `w:footnoteReference`
/// ／`w:endnoteReference`）。語義同 [`crate::FieldKind`] 的標記做法：
/// 解析期標出「這個 run 是引用錨點」（Word 產出恆為專屬空文字 run），
/// 呈現層（F2）依此放註腳／尾註符號；本卡渲染／測量零消費，存檔路徑
/// 不經它（parts passthrough），serde default＋skip 保舊快照相容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRef {
    /// 腳註或尾註。
    pub kind: NoteKind2,
    /// `@w:id`（與 [`Note`] 的 id 同一命名空間：同一 kind 維度內唯一）。
    pub id: i64,
}

/// 正在掃描的一個註元素（[`parse_note_list`] 的內部狀態）。
///
/// `detail`＝`Some((id, kind, start))` 正常開注（start＝
/// `<w:footnote …>` 後一個位元組＝孩子的起點），`None`＝id 不可解的
/// 整註略過（依舊吞到配對 End 才回 Outside，日後同名再開不串味）；
/// `depth`＝同名巢深（按規範不巢套，開閉計數護欄只防畸形檔誤切 slice）。
struct OpenNote {
    detail: Option<(i64, NoteKind, usize)>,
    depth: usize,
}

/// 解析一個註 part：`w:footnotes`／`w:endnotes` 根下的同名 `w:footnote`
/// ／`w:endnote` 子元素，逐註一則 [`Note`]。
///
/// 段落解析**複用正文解析路徑** [`crate::parse_document_xml`]：先用
/// quick_xml（byte 級 Reader，位置＝原始位元組偏移）把每註**孩子的
/// 位元組範圍**（不含外包 `w:footnote` 標籤——留下的 fragment 仍良構，
/// 正文解析器可原樣吃）定位，再逐註重推正文解析器。同一套 `w:p` 佔槽
/// ／mc:Fallback／巢狀段落口徑，不另造第二台段落狀態機。
///
/// 取捨（病態輸入，樣樣不炸）：
/// - 進註元素時 `@w:id` 缺失或非整數 → 整註老實略過（無法被引用，卻
///   會干擾 id 查找語義）；
/// - 同名元素按規範不巢套，仍以開閉計數護欄，畸形檔不誤切 slice；
/// - XML 中途壞掉 → 停在已收部分（同 `ThemeColors::parse` 容錯口徑）；
/// - 單註 fragment 重解析失敗 → 該註照留、`paragraphs` 空（能力界線）。
fn parse_note_list(xml: &[u8], tag: &[u8], theme: &crate::ThemeColors) -> Vec<Note> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut out: Vec<Note> = Vec::new();
    // 進行中的註（None＝Outside：不在任何註元素內）。
    let mut open: Option<OpenNote> = None;
    // read_event 之前的位置＝上一事件結束點；End 事件到達時，註 fragment
    // 的右界就是它（**不含** `</w:footnote>` 本身——留下的 fragment 可
    // 原樣交給正文解析器）。
    let mut pos_before: usize;
    loop {
        pos_before = reader.buffer_position() as usize;
        let event = match reader.read_event() {
            Ok(ev) => ev,
            // 壞 XML → 停在已收部分（同 ThemeColors::parse 容錯口徑）。
            Err(_) => break,
        };
        match event {
            Event::Eof => break,
            Event::Start(e) if e.local_name().as_ref() == tag => match &mut open {
                None => {
                    let kind = kind_of(crate::attr_val(&e, "type").as_deref());
                    match crate::attr_val(&e, "id")
                        .as_deref()
                        .and_then(|v| v.trim().parse::<i64>().ok())
                    {
                        Some(id) => {
                            open = Some(OpenNote {
                                detail: Some((id, kind, pos_before)),
                                depth: 1,
                            });
                        }
                        None => {
                            open = Some(OpenNote {
                                detail: None,
                                depth: 1,
                            });
                        }
                    }
                }
                Some(inner) => inner.depth += 1,
            },
            Event::End(e) if e.local_name().as_ref() == tag => {
                if let Some(inner) = &mut open {
                    if inner.depth == 1 {
                        if let Some((id, kind, start)) = inner.detail.take() {
                            let frag = &xml[start..pos_before.min(xml.len())];
                            let paragraphs =
                                crate::parse_document_xml(frag, theme).unwrap_or_default();
                            out.push(Note {
                                id,
                                kind,
                                paragraphs,
                            });
                        }
                        // 細節 None＝整註略過：本就無事可收，歸位即可。
                        open = None;
                    } else {
                        inner.depth -= 1;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// `@w:type` → [`NoteKind`]：`separator`／`continuationSeparator` 對應；
/// 缺省與其餘值都歸 Regular（取捨見 [`Note::kind`] 欄注）。
fn kind_of(type_attr: Option<&str>) -> NoteKind {
    match type_attr.map(str::trim) {
        Some("separator") => NoteKind::Separator,
        Some("continuationSeparator") => NoteKind::ContinuationSeparator,
        _ => NoteKind::Regular,
    }
}

// ---- 未做清單（誠實 TODO，本卡刻意不實作） ----
//
// - **`document.xml.rels` 關聯解析**：註 part 只按慣例路徑找；不符
//   （罕見）→ 空彙總＝老實不虛構。
// - **`w:footnoteRef`／`w:endnoteRef`／`w:separator`（註內符號）**：
//   不進 run 序；F2 發射符號時定義投影。
// - **`w:footnotePr`／`w:endnotePr`**（settings/sectPr 上的編號格式、
//   重啟、位置設定）：H1 卡已收（見 [`NotePr`]／[`NoteSettings`]／
//   [`parse_note_settings`]；sectPr 層由 `sectpr::read_sections_note_pr`
//   讀），消費仍待 F2。
// - **`w:continuousBookmarks`／`w:bookmarkStart` 內嵌**：註內書籤
//   跑正文解析器自然落在 paragraphs；本卡不另行投影。

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

    // ---------------- (a/f) 實檔註本體 ----------------

    /// HC031：footnotes.xml 含 Separator(-1)／ContinuationSeparator(0)／
    /// Regular(1)，Regular 文字含「This is a footnote」；endnotes 同構。
    #[test]
    fn hc031_notes_from_parts_well() {
        let doc = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let notes = Notes::from_parts(&doc.parts);
        assert_eq!(notes.footnotes.len(), 3, "三則：-1/0/1");
        assert!(notes
            .footnotes
            .iter()
            .any(|n| n.id == -1 && n.kind == NoteKind::Separator));
        assert!(notes
            .footnotes
            .iter()
            .any(|n| n.id == 0 && n.kind == NoteKind::ContinuationSeparator));
        let f1 = notes.footnote(1).expect("Regular 腳註 id=1");
        assert_eq!(f1.kind, NoteKind::Regular);
        assert!(
            f1.text().contains("This is a footnote"),
            "腳註文字：{:?}",
            f1.text()
        );
        assert_eq!(notes.endnotes.len(), 3, "尾註同構三則");
        let e1 = notes.endnote(1).expect("尾註 id=1");
        assert_eq!(e1.kind, NoteKind::Regular);
        assert!(e1.text().contains("This is an endnote"));
    }

    /// POI 表單檔：separator id 從 0 起算、Regular(2)；正文恰 1 個
    /// Footnote(2) 引用。驗證非 -1/0/1 慣例佈局的檔也走同一口徑。
    #[test]
    fn poi_form_footnotes_well() {
        let doc = parse_corpus("poi/test-data_document_form_footnotes.docx");
        let notes = Notes::from_parts(&doc.parts);
        assert_eq!(
            notes.footnotes.len(),
            3,
            "separator(0)/contSep(1)/regular(2)"
        );
        assert!(notes
            .footnotes
            .iter()
            .any(|n| n.id == 0 && n.kind == NoteKind::Separator));
        assert!(notes
            .footnotes
            .iter()
            .any(|n| n.id == 1 && n.kind == NoteKind::ContinuationSeparator));
        let f2 = notes.footnote(2).expect("Regular 腳註 id=2");
        assert!(f2.text().contains("test phrase"), "註文字：{:?}", f2.text());
        let refs: Vec<NoteRef> = doc
            .paragraphs
            .iter()
            .flat_map(|p| p.runs.iter())
            .filter_map(|r| r.note_ref)
            .collect();
        assert_eq!(refs.len(), 1, "正文恰 1 個引用：{refs:?}");
        assert_eq!(
            refs[0],
            NoteRef {
                kind: NoteKind2::Footnote,
                id: 2
            }
        );
    }

    // ---------------- (b) 正文引用標記投影 ----------------

    /// HC031 正文：恰好 1 個 note_ref(Footnote,1) run 與 1 個
    /// note_ref(Endnote,1) run；純文字 run 不帶標記。
    #[test]
    fn hc031_document_note_refs_exactly_one_each() {
        let doc = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let refs: Vec<NoteRef> = doc
            .paragraphs
            .iter()
            .flat_map(|p| p.runs.iter())
            .filter_map(|r| r.note_ref)
            .collect();
        assert_eq!(refs.len(), 2, "全檔引用標記恰 2 個：{refs:?}");
        assert!(
            refs.contains(&NoteRef {
                kind: NoteKind2::Footnote,
                id: 1
            }),
            "須含 Footnote(1)：{refs:?}"
        );
        assert!(
            refs.contains(&NoteRef {
                kind: NoteKind2::Endnote,
                id: 1
            }),
            "須含 Endnote(1)：{refs:?}"
        );
        // 帶標記的 run 必是空文字（引用錨點；Word 產出恆為專屬 run）。
        for (i, p) in doc.paragraphs.iter().enumerate() {
            for r in &p.runs {
                if r.note_ref.is_some() {
                    assert_eq!(r.text, "", "第 {i} 段：引用 run 文字必空");
                }
            }
        }
    }

    // ---------------- (c) 缺 part 容錯 ----------------

    /// 無 footnotes.xml／endnotes.xml 的 corpus 檔 → 與 Notes::default()
    /// 等價、不 panic、footnote／endnote 查找得 None。
    #[test]
    fn missing_note_parts_yield_default_notes() {
        let doc = parse_corpus("ooxmlexport/date_field_in_shape.docx");
        assert!(
            !doc.parts.contains_key(FOOTNOTES_PART) && !doc.parts.contains_key(ENDNOTES_PART),
            "前置條件：本測試檔必須沒有註 part"
        );
        let notes = Notes::from_parts(&doc.parts);
        assert_eq!(notes, Notes::default());
        assert_eq!(notes.footnote(1), None);
        assert_eq!(notes.endnote(1), None);
        assert!(doc
            .paragraphs
            .iter()
            .flat_map(|p| p.runs.iter())
            .all(|r| r.note_ref.is_none()));
    }

    // ---------------- (d) serde 相容 ----------------

    /// 舊 JSON（不含 notes／note_ref）可反序列化；新欄位 None／空時
    /// 不出現於序列化輸出（Run 端 skip；OoxmlDoc 走 serde default）。
    #[test]
    fn notes_serde_backward_compatible() {
        // 舊版 Run 快照（無 note_ref 欄；與既有測試的舊快照形同）。
        let old = r#"{"text":"x","bold":false,"italic":false,"color":null,"underline":false,"strike":false,"field":null,"rstyle":null,"direct":null,"size_pt":null,"tab":false,"effective_fonts":null}"#;
        let run: crate::Run = serde_json::from_str(old).expect("舊版 Run JSON 必須能反序列化");
        assert_eq!(run.note_ref, None, "舊 JSON → note_ref=None");
        // 新欄位 None 時不得外露。
        let j = serde_json::to_string(&crate::Run::default()).expect("序列化 Run");
        assert!(
            !j.contains("note_ref"),
            "Run::note_ref None 時不得外露：{j}"
        );
        // 帶標記時可來回。
        let with_ref = crate::Run {
            note_ref: Some(NoteRef {
                kind: NoteKind2::Endnote,
                id: 7,
            }),
            ..Default::default()
        };
        let back: crate::Run =
            serde_json::from_str(&serde_json::to_string(&with_ref).unwrap()).expect("往返");
        assert_eq!(back.note_ref, with_ref.note_ref, "note_ref 必須可往返");
        // 舊版 OoxmlDoc 快照（缺 notes 欄）→ serde default 給空彙總。
        let sample =
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample.docx"))
                .expect("樣本檔");
        let doc = crate::parse(&sample).expect("解析樣本");
        let mut v = serde_json::to_value(&doc).expect("序列化 OoxmlDoc");
        v.as_object_mut().expect("obj").remove("notes");
        let old_doc: crate::OoxmlDoc =
            serde_json::from_value(v).expect("舊版 OoxmlDoc JSON 反序列化");
        assert_eq!(old_doc.notes, Notes::default(), "缺 notes 欄 → 空彙總");
    }

    // ---------------- (e) 病態輸入 ----------------

    /// 缺 id 的 `<w:footnoteReference/>`／`<w:endnoteReference/>`：老實
    /// 跳過該標記（run 無標記、文字完整），不 panic。**取捨**：缺 id
    /// 的引用無法對應任何註本體，虛構 id（或 0）都比「略過」更錯；
    /// 畸形檔以不炸為先。註 part 端同理：無 id 的 w:footnote 整註略過。
    #[test]
    fn missing_id_references_skipped_safely() {
        let xml = r#"<w:document xmlns:w="x"><w:body>
            <w:p>
              <w:r><w:footnoteReference/><w:t>a</w:t></w:r>
              <w:r><w:endnoteReference/><w:t>b</w:t></w:r>
              <w:r><w:footnoteReference w:id="abc"/><w:t>c</w:t></w:r>
            </w:p></w:body></w:document>"#;
        let paras = crate::parse_document_xml(xml.as_bytes(), &crate::ThemeColors::default())
            .expect("解析");
        assert_eq!(paras[0].runs[0].text, "a", "引用標記失敗不得吞文字");
        assert_eq!(paras[0].runs[1].text, "b");
        assert_eq!(paras[0].runs[2].text, "c");
        assert!(
            paras[0].runs.iter().all(|r| r.note_ref.is_none()),
            "缺 id／非整數 id → 全部無標記"
        );
        // 註 part 端：id 缺失的 w:footnote 整註略過（字面含中文 → 用 &str）。
        let no_id = r#"<w:footnotes xmlns:w="x">
            <w:footnote><w:p><w:r><w:t>無id</w:t></w:r></w:p></w:footnote>
            <w:footnote w:id="1"><w:p><w:r><w:t>有id</w:t></w:r></w:p></w:footnote>
            </w:footnotes>"#;
        let theme = crate::ThemeColors::default();
        let list = parse_note_list(no_id.as_bytes(), b"footnote", &theme);
        assert_eq!(list.len(), 1, "無 id 的註老實略過");
        assert_eq!(list[0].id, 1);
        // 空輸入／壞 XML → 空彙總（不炸）。
        assert!(parse_note_list(b"", b"footnote", &theme).is_empty());
        assert!(parse_note_list(
            b"<w:footnotes xmlns:w=\"x\"><w:footnote",
            b"footnote",
            &theme
        )
        .is_empty());
    }

    // ---------------- (H1) footnotePr／endnotePr 設定解析 ----------------

    /// (a) HC031 settings 層：`w:footnotePr`／`w:endnotePr` 的分隔線／續接
    /// 分隔線引用 id（Word 慣例 -1／0）落 `separator_id`／
    /// `continuation_separator_id`；兩層各自成組，本卡不合併。
    #[test]
    fn hc031_settings_note_pr_ids() {
        let doc = parse_corpus("xopt/TestFiles_HC031-Complicated-Document.docx");
        let notes = Notes::from_parts(&doc.parts);
        let f = notes
            .settings
            .footnote
            .expect("HC031 settings 有 footnotePr");
        assert_eq!(f.separator_id, Some(-1), "分隔線引用 id");
        assert_eq!(f.continuation_separator_id, Some(0), "續接分隔線引用 id");
        let e = notes.settings.endnote.expect("HC031 settings 有 endnotePr");
        assert_eq!(e.separator_id, Some(-1));
        assert_eq!(e.continuation_separator_id, Some(0));
    }

    /// (c) reader-styles.docx：`w:endnotePr` 的 `numFmt=lowerRoman` 落
    /// `NoteNumberFormat::LowerRoman`（settings 層）；`footnotePr` 為
    /// `decimal`。實檔值，不臆造。
    #[test]
    fn reader_styles_endnote_num_fmt() {
        let doc = parse_corpus("phpword/tests_PhpWordTests__files_documents_reader-styles.docx");
        let notes = Notes::from_parts(&doc.parts);
        let e = notes.settings.endnote.expect("reader-styles 有 endnotePr");
        assert_eq!(e.num_fmt, Some(NoteNumberFormat::LowerRoman));
        let f = notes
            .settings
            .footnote
            .expect("reader-styles 有 footnotePr");
        assert_eq!(f.num_fmt, Some(NoteNumberFormat::Decimal));
    }

    /// (d) 病態輸入：未見過的 `w:numFmt` 值 → 老實 None 不 panic；
    /// 空組 `<w:footnotePr/>` → Some(全 None)；壞 XML 不炸。
    #[test]
    fn note_pr_pathological_inputs() {
        let xml = r#"<w:settings xmlns:w="x"><w:footnotePr><w:numFmt w:val="還來沒見過的格式"/></w:footnotePr></w:settings>"#;
        let s = parse_note_settings(xml.as_bytes());
        let f = s.footnote.expect("footnotePr 存在");
        assert_eq!(f.num_fmt, None, "未見值不得臆造");
        let empty = br#"<w:settings xmlns:w="x"><w:footnotePr/></w:settings>"#;
        let s2 = parse_note_settings(empty);
        assert_eq!(
            s2.footnote,
            Some(NotePr::default()),
            "空組須為 Some(全 None)"
        );
        // 壞 XML（截斷）→ 不 panic（回 None 或已收部分）。
        let bad = br#"<w:settings xmlns:w="x"><w:footnotePr><w:numFmt w:val="decimal""#;
        let _ = parse_note_settings(bad);
    }

    /// (e) serde 相容：舊版 Notes JSON（無 `settings` 欄）可反序列化；
    /// `NotePr` 全 None 時不外露任何欄位；帶值時可往返。
    #[test]
    fn note_pr_serde_backward_compatible() {
        let old = r#"{"footnotes":[],"endnotes":[]}"#;
        let notes: Notes = serde_json::from_str(old).expect("舊版 Notes JSON 必須能反序列化");
        assert_eq!(
            notes.settings,
            NoteSettings::default(),
            "缺 settings → 全 None"
        );
        let j = serde_json::to_string(&NotePr::default()).expect("序列化 NotePr");
        assert_eq!(j, "{}", "NotePr 全 None 時須為空物件：{j}");
        let pr = NotePr {
            num_fmt: Some(NoteNumberFormat::LowerRoman),
            num_restart: Some(NoteRestart::EachSect),
            num_start: Some(3),
            pos: Some(NotePos::BeneathText),
            ref_id: Some(7),
            separator_id: Some(-1),
            continuation_separator_id: Some(0),
        };
        let back: NotePr =
            serde_json::from_str(&serde_json::to_string(&pr).unwrap()).expect("往返");
        assert_eq!(back, pr, "NotePr 必須可往返");
    }

    /// (f) 逐欄覆寫語意：settings 層 `numFmt=decimal` 與 sectPr 層
    /// `pos=beneathText` 各自成組——解析層老實分層存，合併是消費端的事。
    #[test]
    fn settings_and_section_layers_stay_separate() {
        let settings =
            br#"<w:settings xmlns:w="x"><w:footnotePr><w:numFmt w:val="decimal"/></w:footnotePr></w:settings>"#;
        let s = parse_note_settings(settings);
        assert_eq!(
            s.footnote.unwrap().num_fmt,
            Some(NoteNumberFormat::Decimal),
            "settings 層"
        );
        let doc_xml = br#"<w:document xmlns:w="x"><w:body>
            <w:p><w:r><w:t>A</w:t></w:r></w:p>
            <w:sectPr><w:footnotePr><w:pos w:val="beneathText"/></w:footnotePr></w:sectPr>
          </w:body></w:document>"#;
        let prs = crate::read_sections_note_pr(doc_xml);
        assert_eq!(prs.len(), 1, "單節");
        assert_eq!(
            prs[0].as_ref().unwrap().pos,
            Some(NotePos::BeneathText),
            "sectPr 層"
        );
    }
}
