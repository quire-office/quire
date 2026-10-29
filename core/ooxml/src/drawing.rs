//! `w:drawing`（DrawingML）／`w:pict`（VML）圖片幾何投影——圖片佔位框（t_a1397522）。
//!
//! 定位（與 [`crate::table`] 同款「唯讀投影」，PM 補正 1）：本模組是純讀取
//! 投影，**不參與序列化**——無損寫回來源仍是 `parts` 明文，round-trip 零影響。
//! 只回答一個問題：這個 run 裡的圖形**多大、在哪、屬不屬於儲存格**，供
//! 版面層撑行帶＋顯示層發射灰底 Rect（點陣直出屬像素渲染卡，本卡不做）。
//!
//! 覆蓋（ECMA-376 §20.4 / VML）：
//! - `wp:inline`／`wp:anchor` 的**直下** `wp:extent/@cx、@cy`（EMU；÷12700
//!   ＝pt）。巢狀圖形（group／框中框）同名的 `wp:extent` 不得竄位：內層
//!   `w:drawing`／`w:pict` 整棵吞沒（Skip），外框只認自己的 extent。
//! - `wp:anchor` 的 `wp:positionH/positionV`：`@relativeFrom` 原字串＋子元素
//!   `wp:posOffset` 文本（EMU→pt，允許負值＝版心外偏移）。可否當頁面鎖點見
//!   [`InlineImage::anchor_page_pos`]。
//! - `wp:anchor/@allowOverlap` 記錄為 `overlap`（僅供老實回報）。
//! - `a:blip/@r:embed`：rId **記錄即可**（本卡不使用——像素渲染卡按它取圖）。
//! - VML `w:pict` 路徑：`v:shape/v:rect/v:oval/v:line/v:roundrect` 的
//!   `style="width:...;height:..."`（pt／cm／in／mm／px，無單位按 px）；
//!   `v:imagedata` 存在 → 圖片；`o:gfxdata`（OLE 圖示）存在而無 imagedata
//!   → 圖示佔位（POI WordWithAttachments 實據）。shape 不必包在 `w:pict` 內
//!   （Word OLE 常以裸 `v:shape` 掛在 `w:object` 下），兩種形態都吃；同一
//!   pict 內首個帶尺寸的 shape 為準（外框；group 子件不重複建框）。純文字
//!   框（無 imagedata／gfxdata）不是圖片：不發射——其文字已由段落流表達。
//! - `mc:Fallback` 子樹整棵吞沒：DrawingML（Choice）與 VML（Fallback）是同
//!   一內容的雙份表示，只取 Choice 一份，否則同一張圖算兩次。
//!
//! 段落錨點口徑（鐵律）：`para_index` ＝**最後一個先於本圖的 `w:p` Start**，
//! 計數器與 `parse_document_xml`／`table::read_body_tables`／`sectpr` 完全
//! 一致（Start 佔槽、自閉合不算、`mc:Fallback` 內不計；文字框巢狀段落照計）。
//! 本模組的 [`count_paras`] 與 `read_images` 共用同一狀態機——corpus 柵欄
//! 逐檔斷言「四解析器同口徑」。格內圖的錨點＝**格內**那個 `w:p`（圖形所在
//! 的最內層段落），engine 層據此對位注入 canonical 儲存格的
//! `CellBlock::Image`（表格切片負責帶高與繪製）；查無所屬儲存格＝老實丟棄
//! （格不畫，圖也不該在）。
//!
//! 佔位語義（本卡核心；執行在 layout/engine 層）：
//! - inline（含退化 anchor）：所屬段落撐 `ceil(h / line_h)` 帶（圖片是
//!   atomic block，不參與斷行）。
//! - anchor（浮動）：**不佔正文流**（ECMA-376：浮動圖形不排進文字流），只在
//!   鎖點發射佔位 Rect。鎖點＝`wp:positionH/V` 的 `posOffset`（缺任一律＝
//!   退化為 inline 佔帶——卡片鐵律：宁可多撑帶，不可讓含圖段落虛構空頁）。
//!   `relativeFrom` 決定原點系換算：page/pageMargin＝頁左上；column/margin
//!   ＝版心左上（單欄近似）；paragraph/line＝錨定段落帶左上（近似——LO 同款
//!   近似，寫實欄位/多欄偏移留像素渲染卡）。
//! - 非法／缺失 extent（缺屬性、非數值、<=0）→ 該圖不採納：不佔帶、不發射。
use serde::{Deserialize, Serialize};

/// EMU → pt（ECMA-376 §20.1：1 pt = 12700 EMU）。
pub const EMU_PER_PT: f64 = 12_700.0;

/// OOXML 整數值域上限（xsd:long）：屬性值寫到連 64 位整數都放不下
/// （如 1e39／4e21）必為垃圾——不採納（t_c1fb4c69 S1-4）。真正的病態危害
/// 在下游 `(h/line_h).ceil() as u32` 爆量→分頁迴圈 OOM，源头擋掉最乾淨。
const MAX_OOXML_LONG: f64 = i64::MAX as f64;

/// R5：serde 空值不外露（`txbx_start == 0` 等同舊資料）。
fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

fn is_zero_usize(v: &usize) -> bool {
    *v == 0
}

/// 圖片佔位投影：扁平段落序列第 `para_index` 段內的一個圖片/圖形框。
///
/// 只讀投影（段落以索引引用，與 `Block::Paragraph{index}` 同款）；佔位
/// 分支由 engine 層依 `in_cell` 與 [`InlineImage::anchor_page_pos`] 決定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineImage {
    /// 所屬段落＝最後一個先於本圖的 `w:p` Start（扁平 Start 口徑；正文前
    /// 出現圖＝0，容錯）。格內圖＝格內那個 `w:p`（圖形所在最內層段落）。
    pub para_index: usize,
    /// 寬（pt，已從 EMU／VML style 換算）。
    pub w_pt: f32,
    /// 高（pt）。
    pub h_pt: f32,
    /// `wp:anchor`（浮動）＝true；`wp:inline`／VML＝false。
    /// true 且 `anchor_page_pos()` 為 `Some` 時不撐行帶，只在鎖點發射 Rect。
    pub floating: bool,
    /// 位於儲存格內：不佔正文流，engine 層按 para_index 注入
    /// `CellBlock::Image`；查無所屬儲存格時老實丟棄。
    #[serde(default)]
    pub in_cell: bool,
    /// `wp:positionH` 之 `wp:posOffset`（EMU→pt）。
    #[serde(default)]
    pub pos_x_pt: Option<f32>,
    /// `wp:positionH/@relativeFrom` 原字串。
    #[serde(default)]
    pub pos_x_rel: Option<String>,
    /// `wp:positionH` 內 `<wp:align>` 對齊枚舉（ECMA-376 §20.4.2.8：與
    /// posOffset **互斥兩式**，OOXMLExport 封面圖族常用；t_1a79f1c5）。
    #[serde(default)]
    pub pos_x_align: Option<String>,
    /// `wp14:pctPosHOffset`（page 系 frame 寬度的百分比×100000；OOXMLExport
    /// 對位圖常用的第三式；t_1a79f1c5）。
    #[serde(default)]
    pub pos_x_pct: Option<f32>,
    /// `wp:positionV` 之 `wp:posOffset`（EMU→pt）。
    #[serde(default)]
    pub pos_y_pt: Option<f32>,
    /// `wp:positionV/@relativeFrom` 原字串。
    #[serde(default)]
    pub pos_y_rel: Option<String>,
    /// `wp:positionV` 內 `<wp:align>` 對齊枚舉（同上，V 軸）。
    #[serde(default)]
    pub pos_y_align: Option<String>,
    /// `wp14:pctPosVOffset`（page 系 frame 高度的百分比×100000；同上第三式）。
    #[serde(default)]
    pub pos_y_pct: Option<f32>,
    /// `wp:anchor/@allowOverlap`（None＝元素不存在／非 anchor）。
    #[serde(default)]
    pub overlap: Option<bool>,
    /// 文字框段落文字（子樹 `w:txbxContent` 各段，按框內段落序；K4e2）。
    /// 非空＝本候選是文字框：VML 純文字框據此採納佔位（舊實作只認
    /// imagedata/gfxdata，連框都不畫）；發射層把它們按框寬斷行畫進框內。
    /// serde default＋空值不外露＝無框圖檔的序列化位元組逐位不變。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub txbx_texts: Vec<String>,
    /// R5：本框文字在**宿主段落** `Paragraph::txbx_texts` 展平清單中的起始
    /// 索引（與 `txbx_runs` 同序；跨圖逐項展平由解析層蓋章）。發射層據此
    /// 取本框自帶的 run 組，取代舊「全域 txbx index 撞每圖一組」的錯位。
    /// serde default＋0 不外露＝無文字框檔案的序列化位元組逐位不變。
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub txbx_start: usize,
    /// R5：框內每個 `w:txbxContent` 子框的段落數（文件序）。只有 DrawingML
    /// 群組（`wpg:wgp` 內多個 `wps:wsp`）會有多項；LO 內容流把群組子框以
    /// **反向文件序**寫入 PDF（shapes-with-text 實據：Where 先於 rightArrow），
    /// 發射層據此對齊抽取序。單子框／VML 群組維持空＝舊行為零擾動。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub txbx_children: Vec<usize>,
    /// 框內文字對齊（例如 "center"；K10）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txbx_align: Option<String>,
    // ---- K4c：填充／描邊／虛線／框內幾何（全部 serde default；欄位不存在
    // ＝None＝舊行為，舊顯示列表 JSON 與序列化位元組零位移）----
    /// 填色（0xAARRGGBB）：`v:fill/@color` > `o:fillcolor`（VML）；
    /// DrawingML 取 `wps:spPr/a:solidFill/a:srgbClr`。不認識的颜色字串＝
    /// None（不冒充）；`auto`＝明確黑。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_argb: Option<u32>,
    /// `filled="f"` → Some(false)（不發填充，也**別把無填充畫成灰底**）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filled: Option<bool>,
    /// 描邊色（0xAARRGGBB）：`v:stroke/@color` > `o:strokecolor`；
    /// DrawingML 取 `wps:spPr/a:ln` 內 `a:solidFill/a:srgbClr`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_argb: Option<u32>,
    /// `stroked="f"` → Some(false)（不發框線）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroked: Option<bool>,
    /// 描邊線寬（pt）：`v:stroke/@weight`（無單位按 EMU）／
    /// `strokeweight`／DrawingML `a:ln/@w`（EMU）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width_pt: Option<f32>,
    /// 虛線樣式（SVG stroke-dasharray 語義）：`strokedashstyle`／
    /// `v:stroke/@dashstyle` 映射手冊見 [`vml_dashstyle_to_dash`]。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_dash: Option<String>,
    /// 框內矩形（相對佔位框原點：dx, dy, 寬, 高，pt）。`v:group` 內子件
    /// 依 coordorigin/coordsize 線性映射而來（PM 補正點：子件無單位數值
    /// 是組座標系單位，不是 px）。映射條件不足（缺 coordsize 等）＝None
    /// → 呼叫端退回整框（老實退化，不虛構位置）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txbx_rect: Option<[f32; 4]>,
    /// `v:textbox/@inset`（左,上,右,下 pt）——框內文邊界。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txbx_inset: Option<[f32; 4]>,
    /// `v:textbox style="layout-flow:vertical"`：框內文字直排（逐字堆疊）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub txbx_vertical: bool,
    /// 框內表格（`w:txbxContent` 內 `w:tbl`）逐格填色（行→列；None＝該格
    /// 無填色宣告）。n780563 三色列表實據：LO 發彩色條，我方至少要落地。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub txbx_table: Vec<Vec<Option<u32>>>,
    /// `a:blip/@r:embed`（VML 為 `v:imagedata/@r:id`）。本卡記錄不使用。
    #[serde(default)]
    pub embed_rid: Option<String>,
    /// 同一候選的**全部** blip rId（文档序；`embed_rid`＝首見）。K4c-image：
    /// `mc:AlternateContent`（含 `ve:` 變體）的 Choice 與 Fallback 各帶一個
    /// `a:blip r:embed`，幾何相同、像素不同（Bug51170：Choice→pdf 不可渲染、
    /// Fallback→png 可渲染）。解析層把 Fallback 的 rId 也記進本欄（不吞），
    /// 像素可用性優先序由 engine 層依副檔名決定——解析器不判格式。
    /// serde default＋空值不外露＝無此形態檔案的序列化位元組逐位不變。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embed_rids: Vec<String>,
    /// R3：VML 頁面釘定（`mso-position-*-relative:page|margin`＋`center`
    /// 的 `position:absolute` 群組）。true＝浮動圖的錨定頁取「所屬 section
    /// 起始頁」而非錨定段落所在頁（LO 頁面釘定語義，K5-diag §2.1 n751077）。
    /// 非頁面釘定的 VML 絕對定位（僅 margin-left/margin-top 偏移）維持
    /// 錨定段落頁（fdo76591 實據）。serde default 保持舊投影位元組不變。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub page_pinned: bool,
    /// R6：`wp:anchor/@relativeHeight`（z 序）。LO 對這些 `wps:wsp` 標註的
    /// 抽取序＝z 序（fdo74605 實據：LO p1 124 行依 relativeHeight 遞增），
    /// 發射層據此排序框內文字。inline／無宣告＝0（舊行為零擾動）。
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub rel_height: i64,
    /// R6：本候選是 DrawingML `wps:wsp` 標註（而非 VML `v:textbox`）。
    /// 只有此類候選套用「標註矩形∩宿主框」裁切與 z 序（K4c-image／VML
    /// 路徑一概不動）。serde default 保持舊投影位元組不變。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wps_annotation: bool,
}

impl InlineImage {
    /// 浮動鎖點（ECMA-376 `wp:positionH/V` 之 `wp:posOffset`）：兩軸齊備即
    /// 鎖點可用——浮動圖**不撐行帶**、只在鎖點發射 Rect（卡片佔位語義）。
    /// 缺任一軸＝None → 呼叫端退化為 inline 佔帶（宁可多撑帶，不虛構空頁）。
    /// `relativeFrom` 決定原點系（[`InlineImage::origin_of`] 的字串來源）。
    pub fn anchor_lock_pt(&self) -> Option<(f32, f32)> {
        match (self.pos_x_pt, self.pos_y_pt) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        }
    }
}

/// `wp:positionX/@relativeFrom` 原字串 → 原點系分類（ECMA-376 ST_RelFromH/V）。
/// page／pageMargin＝頁左上；column／margin＝版心左上（單欄近似）；
/// paragraph／line／character／未知＝錨定段落左上（近似——LO 同款近似，
/// 欄位/行內精確偏移留像素渲染卡）。
pub fn origin_of(relative_from: Option<&str>) -> &'static str {
    match relative_from {
        Some("page") | Some("pageMargin") => "page",
        // margin 枚舉家族（ECMA-376 ST_RelFromH/V 全六值，t_c1fb4c69 S2-8）：
        // 全部以版心為原點系，漏歸類會錯降為 paragraph 錨點。
        Some(
            "column" | "margin" | "topMargin" | "bottomMargin" | "leftMargin" | "rightMargin"
            | "insideMargin" | "outsideMargin",
        ) => "margin",
        _ => "paragraph",
    }
}

/// EMU 字串 → pt（非法／缺／<=0＝None，容錯鐵律：不猜、不虚構）。
/// 超出 OOXML 整數值域（[`MAX_OOXML_LONG`]）或換算後 `as f32` 溢出成 inf
/// （1e39）＝同樣 None——inf／爆量尺寸流入下游 bands 量化會把分頁打成
/// 無限迴圈 OOM（t_c1fb4c69 S1-4）。
pub fn emu_to_pt(raw: Option<&str>) -> Option<f32> {
    let v = raw?.trim().parse::<f64>().ok()?;
    if !v.is_finite() || v <= 0.0 || v > MAX_OOXML_LONG {
        return None;
    }
    let pt = (v / EMU_PER_PT) as f32;
    pt.is_finite().then_some(pt)
}

/// VML style 長度值 → pt。支援 pt、cm、in、mm、px；`%` 與非法值 → None
/// （頁寬百分比在本卡無所適從，老實不採納）。換算後 `as f32` 溢出成 inf
/// 同樣 None（同 [`emu_to_pt`] 的爆量防護，t_c1fb4c69 S1-4）。
fn vml_len_to_pt(num: f64, unit: &str) -> Option<f32> {
    if !num.is_finite() || num <= 0.0 || num > MAX_OOXML_LONG {
        return None;
    }
    let pt = match unit {
        "pt" => num,
        "cm" => num * 72.0 / 2.54,
        "in" => num * 72.0,
        "mm" => num * 72.0 / 25.4,
        "px" => num * 72.0 / 96.0,
        _ => return None,
    };
    let pt = pt as f32;
    (pt > 0.0).then_some(pt).filter(|p| p.is_finite())
}

/// `style="width:242.95pt;height:180.85pt;..."` → (寬pt, 高pt)。
/// 缺側／非法側＝該側 None（任缺 → 整圖不採納，呼叫端把關）。
/// 大小寫不敏感、空白容忍；其餘宣告（position/margin/z-index…）忽略。
pub fn vml_style_wh(style: Option<&str>) -> (Option<f32>, Option<f32>) {
    let Some(style) = style else {
        return (None, None);
    };
    let mut w = None;
    let mut h = None;
    for decl in style.split(';') {
        let Some((k, v)) = decl.split_once(':') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        if key != "width" && key != "height" {
            continue;
        }
        let v = v.trim();
        // 拆尾綴單位：數字前綴＋字母尾綴（空尾綴按 CSS 慣例視作 px）。
        let digits_end = v
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
            .unwrap_or(v.len());
        let (num_s, unit) = (
            &v[..digits_end],
            v[digits_end..].trim().to_ascii_lowercase(),
        );
        let Ok(num) = num_s.trim().parse::<f64>() else {
            continue;
        };
        let conv = vml_len_to_pt(num, if unit.is_empty() { "px" } else { &unit });
        if key == "width" {
            w = conv;
        } else {
            h = conv;
        }
    }
    (w, h)
}

/// R3 開關：VML `position:absolute` 絕對定位解析。關閉＝回到舊行為（VML
/// 框一律 inline 佔帶）——紅燈回塞驗證用，正式路徑恆 true。
const R3_VML_ABSOLUTE_ENABLED: bool = true;

/// R3 開關：VML 頁面釘定浮動框（`mso-position-*-relative:page|margin`＋
/// center）。關閉＝該類框退回舊 inline 佔帶（紅燈回塞驗證用；n751077
/// 3 頁重現），正式路徑恆 true。與 `core/layout` 的
/// `R3_PINNED_SECTION_START` 成對（一管採納、一管錨定頁）。
const R3_VML_PAGE_PINNED_ENABLED: bool = true;

/// VML style 偏移長度 → pt（**允許 0 與負值**；margin-left／margin-top 是
/// 頁邊距原點外的位移，可為負＝版心外）。支援 pt/cm/in/mm/px，無單位按
/// px（CSS 慣例；`margin-left:0` 在 n751077 實據即無單位）。非法／非有限
/// 值 → None（不虛構座標）。
fn vml_offset_to_pt(raw: &str) -> Option<f32> {
    let v = raw.trim();
    if v.is_empty() {
        return None;
    }
    let digits_end = v
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(v.len());
    let (num_s, unit) = (
        &v[..digits_end],
        v[digits_end..].trim().to_ascii_lowercase(),
    );
    let num = num_s.trim().parse::<f64>().ok()?;
    if !num.is_finite() || num.abs() > MAX_OOXML_LONG {
        return None;
    }
    let pt = match unit.as_str() {
        "" | "px" => num * 72.0 / 96.0,
        "pt" => num,
        "cm" => num * 72.0 / 2.54,
        "in" => num * 72.0,
        "mm" => num * 72.0 / 25.4,
        _ => return None,
    };
    (pt as f32).is_finite().then_some(pt as f32)
}

/// R3：VML shape/group 的 `position:absolute` 定位採集（首見為準）。
///
/// 兩式（依 LO 實測優先序）：
/// 1. `mso-position-{horizontal,vertical}:center|left|right|top|bottom` 搭配
///    `mso-position-*-relative:page|margin`——頁面釘定（n751077 v:group
///    實據）；記成 `pos_*_rel`＋`pos_*_align`，`page_pinned=true`。
/// 2. `margin-left`／`margin-top` 偏移——原點＝版心左上（LO 換算為頁邊距
///    原點＋偏移，fdo76591 三框實據）；記成 `pos_*_pt`＋rel="margin"。
///
/// 無 `position:absolute` 的 VML（K4a/K4c 既有語料）一概不動＝浮動旗標
/// 零變化。子件（組內 rect）不覆寫已由外層 group 定下的定位（首見為準）。
fn apply_vml_position(v: &mut Pending, e: &quick_xml::events::BytesStart) {
    if !R3_VML_ABSOLUTE_ENABLED {
        return;
    }
    // 定位一旦由外層 group 或前一同名候選定下即不再覆寫。
    if v.pos_x_pt.is_some()
        || v.pos_x_rel.is_some()
        || v.pos_x_align.is_some()
        || v.pos_y_pt.is_some()
        || v.pos_y_rel.is_some()
        || v.pos_y_align.is_some()
    {
        return;
    }
    let (_, _, kv) = vml_style_map(attr_local(e, b"style").as_deref());
    if !style_lookup(&kv, "position").is_some_and(|p| p.trim().eq_ignore_ascii_case("absolute")) {
        return;
    }
    let rel_of = |key: &str| style_lookup(&kv, key).map(|r| r.trim().to_ascii_lowercase());
    let page_or_margin =
        |r: Option<&String>| matches!(r.map(String::as_str), Some("page") | Some("margin"));
    let h_rel = rel_of("mso-position-horizontal-relative");
    let v_rel = rel_of("mso-position-vertical-relative");
    let h_align =
        style_lookup(&kv, "mso-position-horizontal").map(|a| a.trim().to_ascii_lowercase());
    let v_align = style_lookup(&kv, "mso-position-vertical").map(|a| a.trim().to_ascii_lowercase());
    let valid_h = |a: &str| matches!(a, "left" | "center" | "right" | "inside" | "outside");
    let valid_v = |a: &str| matches!(a, "top" | "center" | "bottom" | "inside" | "outside");
    // 頁面釘定判別（K5-diag §2.1）：水平與垂直都 relative page|margin 且
    // 皆為 center → 錨定頁取所屬 section 起始頁。回塞開關關閉時，此類框
    // 整支退回舊 inline 佔帶（不採納浮動），n751077 3 頁重現。
    let pinned = page_or_margin(h_rel.as_ref())
        && page_or_margin(v_rel.as_ref())
        && h_align.as_deref() == Some("center")
        && v_align.as_deref() == Some("center");
    if pinned && !R3_VML_PAGE_PINNED_ENABLED {
        return;
    }
    let mut positioned = false;
    // 第 1 式：頁面釘定對齊（relative 限 page/margin）。
    if page_or_margin(h_rel.as_ref()) {
        if let Some(a) = h_align.as_deref().filter(|a| valid_h(a)) {
            v.pos_x_rel = h_rel.clone();
            v.pos_x_align = Some(a.to_string());
            positioned = true;
        }
    }
    if page_or_margin(v_rel.as_ref()) {
        if let Some(a) = v_align.as_deref().filter(|a| valid_v(a)) {
            v.pos_y_rel = v_rel.clone();
            v.pos_y_align = Some(a.to_string());
            positioned = true;
        }
    }
    // 第 2 式：margin 偏移（第 1 式未定該軸時才退）。
    if v.pos_x_rel.is_none() && v.pos_x_align.is_none() {
        if let Some(x) = style_lookup(&kv, "margin-left").and_then(vml_offset_to_pt) {
            v.pos_x_pt = Some(x);
            v.pos_x_rel = Some("margin".to_string());
            positioned = true;
        }
    }
    if v.pos_y_rel.is_none() && v.pos_y_align.is_none() {
        if let Some(y) = style_lookup(&kv, "margin-top").and_then(vml_offset_to_pt) {
            v.pos_y_pt = Some(y);
            v.pos_y_rel = Some("margin".to_string());
            positioned = true;
        }
    }
    if positioned {
        v.floating = true;
    }
    if pinned {
        v.page_pinned = true;
    }
}

/// VML/OOXML 顏色字串 → 0xAARRGGBB。口徑（卡片契約 2）：
/// - `#RRGGBB`／`RRGGBB`／8 位 `AARRGGBB`（DrawingML `a:srgbClr/@val`）；
/// - 具名色 black/white/red/blue/green/gray/grey/yellow/purple/olive/
///   maroon/navy/teal/silver/lime；
/// - `auto` → 明確黑（VML 慣例：auto 即主題文字色，近似黑）；
/// - **不認識 → None**（老實不發色，不冒充）；
/// - 帶尾註的 Word 產出（`"white [3201]"`／`"#7f7f7f [1601]"`）只吃空格前
///   本體——尾註是主題索引註解，非颜色值的一部分。
///   名稱大小寫不敏感。
pub fn parse_vml_color(raw: Option<&str>) -> Option<u32> {
    let s = raw?.trim();
    let s = s.split([' ', '\t']).next().unwrap_or("");
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if lower == "auto" {
        return Some(0xFF00_0000);
    }
    let named = match lower.as_str() {
        "black" => 0xFF00_0000,
        "white" => 0xFFFF_FFFF,
        "red" => 0xFFFF_0000,
        "blue" => 0xFF00_00FF,
        "green" => 0xFF00_8000,
        "gray" | "grey" => 0xFF80_8080,
        "yellow" => 0xFFFF_FF00,
        "purple" => 0xFF80_0080,
        "olive" => 0xFF80_8000,
        "maroon" => 0xFF80_0000,
        "navy" => 0xFF00_0080,
        "teal" => 0xFF00_8080,
        "silver" => 0xFFC0_C0C0,
        "lime" => 0xFF00_FF00,
        _ => {
            // 十六進位：#RRGGBB／RRGGBB／AARRGGBB（ DrawingML 常用後者）。
            let hex = lower.strip_prefix('#').unwrap_or(&lower);
            if !(hex.len() == 6 || hex.len() == 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let v = u32::from_str_radix(hex, 16).ok()?;
            return Some(if hex.len() == 6 { 0xFF00_0000 | v } else { v });
        }
    };
    Some(named)
}

/// `strokedashstyle`／`v:stroke/@dashstyle` → SVG stroke-dasharray（卡片
/// 契約 4 映射手冊；VML 官方樣式 → dasharray，單位 pt，線寬呼叫端另給）：
/// - `solid`（含不認識值以外的實語義不明者）→ None（實線）；
/// - `dash` → `"6 3"`（畫 6pt 空 3pt）；
/// - `sysDash` → `"3 3"`（系統短橫線，LO 同款近似）；
/// - `dot`／`sysDot` → `"1 3"`（點：畫 1pt 空 3pt）；
/// - `dashDot` → `"6 3 1 3"`；
/// - `dashDotDot` → `"6 3 1 3 1 3"`；
/// - `longDash` → `"12 3"`；`longDashDot` → `"12 3 1 3"`；
/// - `longDashDotDot` → `"12 3 1 3 1 3"`；
/// - 三數值自訂式 `"sd,sd,sd"`（VML 允許 `1 2 3` 形态）→ 直接透傳。
///   不認識 → None（實線近似，不虛構花樣）。
pub fn vml_dashstyle_to_dash(raw: Option<&str>) -> Option<String> {
    let s = raw?.trim();
    if s.is_empty() {
        return None;
    }
    let named = match s.to_ascii_lowercase().as_str() {
        "solid" => return None,
        "dash" => "6 3",
        "sysdash" => "3 3",
        "dot" | "sysdot" => "1 3",
        "dashdot" => "6 3 1 3",
        "dashdotdot" => "6 3 1 3 1 3",
        "longdash" => "12 3",
        "longdashdot" => "12 3 1 3",
        "longdashdotdot" => "12 3 1 3 1 3",
        _ => {
            // 自訂式：純數字（可含小數）以空白／逗號分隔 → 原樣正規化成
            // 空白分隔（SVG 語義）。混雜非數字＝不認識 → None。
            let parts: Vec<&str> = s
                .split([',', ' ', '\t'])
                .filter(|p| !p.is_empty())
                .collect();
            if parts.is_empty() || !parts.iter().all(|p| p.parse::<f32>().is_ok()) {
                return None;
            }
            return Some(parts.join(" "));
        }
    };
    Some(named.to_string())
}

/// `style` 宣告串 → (寬pt, 高pt, 其餘鍵小寫表)。除 width/height 走
/// [`vml_style_wh`]（單位換算口徑共用）外，其餘鍵值原樣入表（呼叫端按需
/// 取 left/top/layout-flow 等）。值不 unescape（style 屬性由 quick-xml
/// 屬性迭代已 unescape）。
fn vml_style_map(style: Option<&str>) -> (Option<f32>, Option<f32>, Vec<(String, String)>) {
    let mut kv = Vec::new();
    if let Some(style) = style {
        for decl in style.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                kv.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
            }
        }
    }
    let (w, h) = vml_style_wh(style);
    (w, h, kv)
}

fn style_lookup<'a>(kv: &'a [(String, String)], key: &str) -> Option<&'a str> {
    kv.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// `inset="3.6pt,54pt,3.6pt,180pt"` → (左,上,右,下) pt。只吃 `pt` 顯式
/// 單位（VML inset 官方單位即 pt，無單位也按 pt）；任一值非法 → None
/// （呼叫端退回內縮 3pt 的舊近似——老實退化，不虛構邊界）。
fn parse_vml_inset(raw: Option<&str>) -> Option<[f32; 4]> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    let mut out = [0.0f32; 4];
    for (i, part) in raw.split(',').enumerate().take(4) {
        let part = part.trim();
        let digits_end = part
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .unwrap_or(part.len());
        let (num_s, unit) = (&part[..digits_end], part[digits_end..].trim());
        if !unit.is_empty() && !unit.eq_ignore_ascii_case("pt") {
            return None;
        }
        let v = num_s.trim().parse::<f32>().ok()?;
        if !v.is_finite() || v < 0.0 {
            return None;
        }
        out[i] = v;
    }
    Some(out)
}

/// `v:group` 座標系快照（PM 補正點）：組內子件的 left/top/width/height
/// **無單位數值是該組 coordorigin/coordsize 座標系單位**，不是 px。
/// 每層組建 coord→extent 線性映射，巢狀組逐層合成（子組的 dx0/dy0/W/H
/// 由父幀先把子組自身的 coord 矩形映射成 pt 再入棧）：
/// `x_pt = dx0 + (coord_x − origin_x) / coordsize_w × w_pt`（y 同理）。
/// 缺 coordsize、子組自身 coord 矩形、或外框尺寸（pt）者——映射條件不足
/// → 該層壓入 None（毒化層），其子件老實不映射（呼叫端退回整框；
/// 不發射怪異座標）。
#[derive(Clone, Copy)]
struct CoordFrame {
    origin_x: f64,
    origin_y: f64,
    size_w: f64,
    size_h: f64,
    /// 組頂點相對**最外層佔位框原點**的偏移（pt，巢狀合成後）。
    dx0: f32,
    dy0: f32,
    /// 組 extent（pt）。
    w_pt: f32,
    h_pt: f32,
}

impl CoordFrame {
    /// coord 點 → pt（絕對，含本層偏移）。
    fn map_x(&self, cx: f64) -> f32 {
        self.dx0 + ((cx - self.origin_x) / self.size_w * self.w_pt as f64) as f32
    }
    fn map_y(&self, cy: f64) -> f32 {
        self.dy0 + ((cy - self.origin_y) / self.size_h * self.h_pt as f64) as f32
    }
    /// coord 尺寸 → pt（尺寸不含原點偏移與頂點偏移）。
    fn scale_x(&self, cw: f64) -> f32 {
        (cw / self.size_w * self.w_pt as f64) as f32
    }
    fn scale_y(&self, ch: f64) -> f32 {
        (ch / self.size_h * self.h_pt as f64) as f32
    }
}

/// `coordorigin="9211,-165"` → (x, y)；缺逗號側按 0；非法 → None。
fn parse_coord_pair(raw: &str) -> Option<(f64, f64)> {
    let mut it = raw.split(',');
    let x = it.next()?.trim().parse::<f64>().ok()?;
    let y = match it.next() {
        Some(s) => s.trim().parse::<f64>().ok()?,
        None => 0.0,
    };
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

/// 子件 style 的 left/top/width/height 四值 → 組座標系數值。VML 慣例缺
/// left/top 按 0；**只吃無單位純數值**（帶 pt/cm/… 字尾者是絕對慣例，不歸
/// 本映射管→ None，呼叫端退回整框）；缺寬或高 → None（映射條件不足）。
fn coord_style_rect(kv: &[(String, String)]) -> Option<[f64; 4]> {
    let mut out = [0.0f64; 4];
    for (i, key) in ["left", "top", "width", "height"].iter().enumerate() {
        let v = match style_lookup(kv, key) {
            Some(v) => v,
            None if i < 2 => continue,
            None => return None,
        };
        // 純數值（無單位）才是組座標系；帶 pt/cm/… 者交給 vml_len_to_pt 路徑。
        if v.chars()
            .any(|c| !(c.is_ascii_digit() || c == '.' || c == '-'))
        {
            return None;
        }
        let n = v.parse::<f64>().ok()?;
        if !n.is_finite() {
            return None;
        }
        out[i] = n;
    }
    Some(out)
}

/// VML 布林字串「假」值（filled="f"／stroked="false" 等）。
fn is_vml_falseish(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "f" | "false" | "0" | "no" | "off"
    )
}

/// style 值是否帶長度單位字尾（pt/cm/in/mm/px——數字前綴後仍有非數字殘留）。
/// 用於區分「絕對長度宣告」與「組座標系無單位數值」。
fn has_unit_suffix(v: &str) -> bool {
    let v = v.trim();
    match v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')) {
        Some(0) => false,
        Some(i) => {
            v[i..].trim().eq_ignore_ascii_case("pt")
                || v[i..].trim().eq_ignore_ascii_case("cm")
                || v[i..].trim().eq_ignore_ascii_case("in")
                || v[i..].trim().eq_ignore_ascii_case("mm")
                || v[i..].trim().eq_ignore_ascii_case("px")
        }
        None => false,
    }
}

/// shape 元素自身 Start → 填充／描邊宣告採集（首見為準）。優先序（卡片
/// 契約 2）：`v:fill/@color`／`v:stroke/@color` 子元素 > 本函數的 shape
/// 屬性（`fillcolor`／`strokecolor`／`strokeweight`／`strokedashstyle`）>
/// docDefaults（本卡未接級聯＝老實缺省不發色）。
fn apply_shape_style(v: &mut Pending, e: &quick_xml::events::BytesStart) {
    if let Some(filled) = attr_local(e, b"filled") {
        if is_vml_falseish(&filled) {
            v.filled_f = true;
        }
    }
    if let Some(stroked) = attr_local(e, b"stroked") {
        if is_vml_falseish(&stroked) {
            v.stroked_f = true;
        }
    }
    if v.fill_attr.is_none() {
        v.fill_attr = vml_attr_color(e, b"fillcolor");
    }
    if v.stroke_attr.is_none() {
        v.stroke_attr = vml_attr_color(e, b"strokecolor");
    }
    if v.stroke_w.is_none() {
        v.stroke_w = attr_weight_pt(attr_local(e, b"strokeweight").as_deref());
    }
    if v.dash.is_none() {
        v.dash = vml_dashstyle_to_dash(attr_local(e, b"strokedashstyle").as_deref());
    }
}

/// `v:fill`／`v:stroke`／`v:textbox` 子元素 → 樣式採集（Start 與 Empty 共用；
/// 首見為準）。`v:fill/@color` 是填充色主源，`v:stroke` 帶 color/weight/
/// dashstyle，`v:textbox` 帶 inset 與 style 內的 layout-flow。
fn apply_vml_deco(v: &mut Pending, name: &[u8], e: &quick_xml::events::BytesStart) {
    match name {
        b"fill" => {
            if v.fill.is_none() {
                v.fill = vml_attr_color(e, b"color");
            }
        }
        b"stroke" => {
            if v.stroke.is_none() {
                v.stroke = vml_attr_color(e, b"color");
            }
            if v.stroke_w.is_none() {
                v.stroke_w = attr_weight_pt(attr_local(e, b"weight").as_deref());
            }
            if v.dash.is_none() {
                v.dash = vml_dashstyle_to_dash(attr_local(e, b"dashstyle").as_deref());
            }
        }
        b"textbox" => {
            if v.txbx_inset.is_none() {
                v.txbx_inset = parse_vml_inset(attr_local(e, b"inset").as_deref());
            }
            if !v.txbx_vertical {
                let (_, _, kv) = vml_style_map(attr_local(e, b"style").as_deref());
                if let Some(flow) = style_lookup(&kv, "layout-flow") {
                    if flow.eq_ignore_ascii_case("vertical") {
                        v.txbx_vertical = true;
                    }
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// 事件流解析器（quick-xml 單趟；深度計數狀態機）
// ---------------------------------------------------------------------------

/// local name 是否為 VML 圖形元素（其 style 攜帶框尺寸）。
fn is_vml_shape(name: &[u8]) -> bool {
    // K4c：`v:group` 入列——組是「可遞進的容器形」（pict 直接子孫常態即
    // group 包 shape，groupshape-sdt 實據），座標經 coordorigin/coordsize
    // 映射（見 [`CoordFrame`]），尺寸以組自身 style（pt 慣例）為準。
    matches!(
        name,
        b"shape" | b"rect" | b"oval" | b"line" | b"roundrect" | b"group"
    )
}

/// K4d：吞沒幀閉合標籤查表（`is_vml_shape` 同族）——run 外裸 shape 整棵
/// 跳過時 `skip_tag` 需要與 Start 配對的靜態閉合名。非 shape 名 → None。
fn vml_shape_close(name: &[u8]) -> Option<&'static [u8]> {
    Some(match name {
        b"shape" => b"shape",
        b"rect" => b"rect",
        b"oval" => b"oval",
        b"line" => b"line",
        b"roundrect" => b"roundrect",
        b"group" => b"group",
        _ => return None,
    })
}

/// K4c：填色／描邊／框線寬的字串→色值共用口徑（VML 與 DrawingML 落點同
/// 一函數；首見為準由呼叫端把關）。
fn vml_attr_color(attrs: &quick_xml::events::BytesStart, key: &[u8]) -> Option<u32> {
    parse_vml_color(attr_local(attrs, key).as_deref())
}

/// `weight`／`strokeweight`／`a:ln/@w` 字串 → pt。帶單位走 vml_len_to_pt
/// （strokeweight="1.5pt"），純數值按 EMU（v:stroke/@weight="9360"、
/// a:ln/@w="9525"——兩者皆 EMU 慣例，9525EMU=0.75pt 吻合 DrawingML 預設）。
fn attr_weight_pt(raw: Option<&str>) -> Option<f32> {
    let s = raw?.trim();
    if s.is_empty() {
        return None;
    }
    let digits_end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(s.len());
    let (num_s, unit) = (&s[..digits_end], s[digits_end..].trim());
    let num = num_s.parse::<f64>().ok()?;
    if unit.is_empty() {
        // 純數值＝EMU。
        return emu_to_pt(Some(num_s));
    }
    vml_len_to_pt(num, &unit.to_ascii_lowercase())
}

/// 進行中的圖片累積（DrawingML 候選與 VML shape 共用；完結時尺寸齊備才落表）。
struct Pending {
    /// 錨定段落（建立時的 `w:p` Start 序——即圖形所在的最內層段落）。
    para: usize,
    w: Option<f32>,
    h: Option<f32>,
    pos_x_pt: Option<f32>,
    pos_x_rel: Option<String>,
    pos_x_align: Option<String>,
    pos_x_pct: Option<f32>,
    pos_y_pt: Option<f32>,
    pos_y_rel: Option<String>,
    pos_y_align: Option<String>,
    pos_y_pct: Option<f32>,
    /// posOffset 文本捕獲旗標（Some(true)＝寫 pos_y、Some(false)＝寫 pos_x、
    /// None＝不捕獲）。**只在 `wp:posOffset` 元素啟閉期間開啟**（t_c1fb4c69
    /// S1-5）：舊實作在 positionH/V 開標就設 capture，`<wp:align>` 對齊或 pct
    /// 偏移（無 posOffset）時 capture 永不重置，其後 txbxContent 的數字文字
    /// 會被誤當 EMU 填入鎖點座標。
    capture: Option<bool>,
    /// 進行中的 positionH/V 軸向（true=V）：其內 posOffset 開標時據此決定
    /// 捕獲寫入哪一軸（positionH/V 閉合清空）。
    pos_axis: Option<bool>,
    /// 進行中的 `<wp:align>` 軸向（true=V）：其文字直存對齊枚舉字串
    /// （t_1a79f1c5；與 capture 分離——align 文字非數值，不得走 EMU 解析）。
    align_axis: Option<bool>,
    /// 進行中的 `<wp14:pctPos[HV]Offset>` 軸向（true=V）：其文字直存
    /// 百分比×100000 整數（t_1a79f1c5；OOXMLExport 位圖第三式）。
    pct_axis: Option<bool>,
    overlap: Option<bool>,
    rid: Option<String>,
    /// K4c-image：本候選見過的全部 blip rId（文档序去重；`rid`＝首見）。
    /// Choice 走常規路徑收、Fallback 子樹破例只收 rId（幾何/段落一概不動）。
    rids: Vec<String>,
    /// 產出目標是儲存格（建立時 tc 深度 > 0）而非正文段落。
    in_cell: bool,
    /// 候選出自 `wp:anchor`（浮動）或 VML `position:absolute`（R3）。
    floating: bool,
    /// R3：VML 頁面釘定（mso-position 兩軸 page|margin＋center）——浮動
    /// 圖錨定頁改取所屬 section 起始頁。
    page_pinned: bool,
    /// R6：`wp:anchor/@relativeHeight`（z 序；DrawingML 標註抽取序）。
    rel_height: i64,
    /// R6：本候選是 DrawingML `wps:wsp` 標註（套用宿主框裁切與 z 序）。
    wps_annotation: bool,
    /// VML：v:imagedata 出現過。
    vml_image: bool,
    /// VML：o:gfxdata（OLE 圖示）出現過。
    vml_gfxdata: bool,
    /// 子樹內見過 `w:txbxContent`（文字框）。VML 純文字框靠此採納佔位
    /// （K4e2：其文字不再走正文流，改掛宿主段落 `txbx_texts`，發射層按此
    /// Shape 落點繪製）；DrawingML 側同標記但採納條件本就看 extent，不動。
    has_txbx: bool,
    /// 框內段落文字（`w:txbxContent` 各段完結時推入）。
    txbx: Vec<String>,
    /// 進行中的框內段落文字。
    txbx_cur: String,
    /// 框內 `w:t` 文字區間旗標（Start/End 配對）。
    txbx_in_t: bool,
    /// 框內 `w:p` 開閉深度（完結時 flush txbx_cur）。
    txbx_para_depth: usize,
    /// R5：框內每個 `w:txbxContent` 子框的段落數（DrawingML 群組子框序用）。
    txbx_children: Vec<usize>,
    /// 框內文字對齊（例如 "center"；K10）。
    txbx_align: Option<String>,
    // ---- K4c：填色／描邊／框內幾何累積（首見為準，與 w/h 同款口徑）----
    /// `v:fill/@color`（主）與 DrawingML `spPr/solidFill/srgbClr`。
    fill: Option<u32>,
    /// shape 自身 `fillcolor`／`o:fillcolor`（退居其次：`v:fill/@color` >
    /// 本欄 > docDefaults，卡片契約 2 優先序）。
    fill_attr: Option<u32>,
    filled_f: bool,
    /// `v:stroke/@color`（主）。
    stroke: Option<u32>,
    /// shape 自身 `strokecolor`／`o:strokecolor`（退居其次）。
    stroke_attr: Option<u32>,
    stroked_f: bool,
    stroke_w: Option<f32>,
    dash: Option<String>,
    /// 最近一個可映射 shape 的組內矩形（pt，相對外框原點）。
    mapped_rect: Option<[f32; 4]>,
    txbx_rect: Option<[f32; 4]>,
    txbx_inset: Option<[f32; 4]>,
    txbx_vertical: bool,
    /// 進行中的一列填色（框內表格；None＝該格無填色宣告）。
    tbl_row: Vec<Option<u32>>,
    tbl_rows: Vec<Vec<Option<u32>>>,
    /// 進行中 `w:tc` 的填色（`w:shd/@w:fill` 首見為準；`w:tc` 完結時
    /// push 進 `tbl_row` 後歸零）。
    cell_fill: Option<u32>,
    /// 框內 `w:tc` 開閉深度（>0 時 w:shd/@w:fill 歸本格）。
    tc_in: usize,
    /// 最外層 `v:group` 自身 style（帶單位宣告）撐出的 extent（pt）：
    /// pict 直接子孫是組時，候補 w/h 缺 → 以此補（組即佔位框實據）。
    w_from_group: Option<f32>,
    h_from_group: Option<f32>,
    /// K4c：v:group coord→extent 映射幀棧（None 項＝映射條件不足的毒化層，
    /// 其子件老實不映射）。随 pict／裸 shape 建候補時清空。
    coord_stack: Vec<Option<CoordFrame>>,
    // ---- DrawingML wps:spPr 樣式上下文（僅 graphic 候補用）----
    /// `wps:spPr` 深度（>0 時 solidFill/srgbClr 才歸本候選的填充）。
    sp_ctx: usize,
    /// `a:ln` 深度（>0 時 solidFill 歸**描邊**而非填充）。
    ln_ctx: usize,
    /// `a:solidFill` 深度（其內 srgbClr 才落色）。
    fill_ctx: usize,
    /// `a:extLst`／`a:ext` 深度（>0 時 solidFill/srgbClr **一概不收**：
    /// 擴充清單如 `a14:hiddenLine` 非真實填充／描邊，mce-nested 實據）。
    ext_ctx: usize,
}

impl Pending {
    /// K4c-image：blip／imagedata rId 記錄——首見進 `rid`（既有語義逐位不
    /// 變），全部進 `rids`（文档序去重）。`AlternateContent` 的 Choice 與
    /// Fallback 各帶一份 blip（幾何同、像素異）時兩條都留，engine 層依
    /// 副檔名可渲染性擇優。
    fn note_rid(&mut self, rid: Option<String>) {
        let Some(rid) = rid else { return };
        if self.rid.is_none() {
            self.rid = Some(rid.clone());
        }
        if !self.rids.contains(&rid) {
            self.rids.push(rid);
        }
    }

    fn new(para: usize, in_cell: bool) -> Self {
        Self {
            para,
            w: None,
            h: None,
            pos_x_pt: None,
            pos_x_rel: None,
            pos_x_align: None,
            pos_x_pct: None,
            pos_y_pt: None,
            pos_y_rel: None,
            pos_y_align: None,
            pos_y_pct: None,
            capture: None,
            pos_axis: None,
            align_axis: None,
            pct_axis: None,
            overlap: None,
            rid: None,
            rids: Vec::new(),
            floating: false,
            page_pinned: false,
            rel_height: 0,
            wps_annotation: false,
            in_cell,
            vml_image: false,
            vml_gfxdata: false,
            has_txbx: false,
            txbx: Vec::new(),
            txbx_cur: String::new(),
            txbx_in_t: false,
            txbx_para_depth: 0,
            txbx_children: Vec::new(),
            txbx_align: None,
            fill: None,
            fill_attr: None,
            filled_f: false,
            stroke: None,
            stroke_attr: None,
            stroked_f: false,
            stroke_w: None,
            dash: None,
            mapped_rect: None,
            txbx_rect: None,
            txbx_inset: None,
            txbx_vertical: false,
            tbl_row: Vec::new(),
            tbl_rows: Vec::new(),
            cell_fill: None,
            tc_in: 0,
            w_from_group: None,
            h_from_group: None,
            coord_stack: Vec::new(),
            sp_ctx: 0,
            ln_ctx: 0,
            fill_ctx: 0,
            ext_ctx: 0,
        }
    }
}

struct DrawingParser {
    out: Vec<InlineImage>,
    /// 全局 `w:p` Start 計數——與 `parse_document_xml` 逐位元組同口徑
    /// （Start 佔槽、自閉合不算、Fallback 內不計）。
    p_count: usize,
    // ---- 深度計數（良構 XML 開閉必配；與 table.rs 幀棧等價但更薄）----
    tc_depth: usize,
    drawing_depth: usize,
    pict_depth: usize,
    shape_depth: usize,
    /// K4d（E2）：進行中 `w:r` 包圍深度——LO 對 VML 圖形的繪製開關**唯一**是
    /// 「`w:pict` 是否為 `w:r` 子節點」（k5-diag §1.3 變體實驗：錯置 pict＋
    /// wrap/absolute 照樣消失；合法包 run 後任何 wrap 都照畫）。`run_depth
    /// == 0` 的 pict／裸 shape 整棵跳過（框、Rule、框內文字發射路徑同受此閂）。
    /// 自閉合 `w:r` 不計（比照 `p_count` 口徑）；`w:ins`／`w:hyperlink` 包 run
    /// （修訂圖／連結圖的 Word 正規產出）天然涵蓋——計的是祖先 run，非父節點。
    run_depth: usize,
    /// 進行中的 DrawingML 候選（與 inline/anchor 閉合配對）。
    graphic: Option<Box<Pending>>,
    /// 進行中的 VML 累積（pict 或裸 shape）。
    vml: Option<Box<Pending>>,
    /// 吞沒子樹（Graphic 內的巢狀 drawing/pict）：只跟自己的閉合標籤。
    skip_tag: Option<(&'static [u8], usize)>,
    /// `mc:Fallback` 深度（parser 級）：>0 時圖片內容一概不看（雙份去重）。
    fb: usize,
    /// K4e2：`w:txbxContent` 文字框子樹深度。>0 時其內 `w:p` **不入扁平口徑**
    /// （主解析器改掛外層段落的 `txbx_texts`）——四解析器同口徑鐵律的改寫點。
    txbx_depth: usize,
    /// R5：宿主段落 → 已完結候選累積的框內段落數（含不受理候選）。發射層的
    /// `txbx_texts` 是跨圖展平的，本游標在解析層逐框蓋章 `txbx_start`，讓
    /// 「第 N 框」對到自己的 run 組，不再錯採前框樣式。
    txbx_seen: Vec<usize>,
    /// `mc:AlternateContent` 巢狀快照（錯置標籤不致 fb 卡死，同主解析器）。
    ac_stack: Vec<usize>,
    /// K4d 防過抑回塞鎖（照 L2-cellruns `L2_BUGGY` 先例）：`K4D_BUGGY=1` 時
    /// 閂退化為「run 與非 run 都採納」——紅燈 1（錯置 pict 消失）必須轉紅。
    /// 只在 `new()` 讀一次，熱路徑零 env 查詢。
    buggy: bool,
    /// K10 表內浮動錨點回塞鎖：`K10_CELL_ANCHOR_BUGGY=1` 時回退舊 in_cell 判定。
    cell_anchor_buggy: bool,
}

impl DrawingParser {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            p_count: 0,
            tc_depth: 0,
            drawing_depth: 0,
            pict_depth: 0,
            shape_depth: 0,
            run_depth: 0,
            graphic: None,
            vml: None,
            skip_tag: None,
            fb: 0,
            txbx_depth: 0,
            txbx_seen: Vec::new(),
            ac_stack: Vec::new(),
            buggy: std::env::var("K4D_BUGGY").is_ok(),
            cell_anchor_buggy: std::env::var("K10_CELL_ANCHOR_BUGGY").is_ok(),
        }
    }

    fn cur_para(&self) -> usize {
        self.p_count.saturating_sub(1)
    }

    /// 完結落表：尺寸齊備才採納（extent 缺／非法＝老實不採納，不虛構幾何）。
    fn push_pending(&mut self, p: &Pending) {
        // R5：先在宿主段落的展平清單裡蓋章本框起點，再推進游標——不受理
        // 候選（缺 extent）也照樣計數，才與主解析器 `attach_txbx` 的批次
        // 累積序對齊（否則後續框的 run 會整體前移）。
        if self.txbx_seen.len() <= p.para {
            self.txbx_seen.resize(p.para + 1, 0);
        }
        let txbx_start = self.txbx_seen[p.para];
        self.txbx_seen[p.para] = txbx_start + p.txbx.len();
        let (Some(w), Some(h)) = (p.w, p.h) else {
            return;
        };
        self.out.push(InlineImage {
            para_index: p.para,
            w_pt: w,
            h_pt: h,
            floating: p.floating,
            page_pinned: p.page_pinned,
            rel_height: p.rel_height,
            wps_annotation: p.wps_annotation,
            in_cell: p.in_cell,
            pos_x_pt: p.pos_x_pt,
            pos_x_rel: p.pos_x_rel.clone(),
            pos_x_align: p.pos_x_align.clone(),
            pos_x_pct: p.pos_x_pct,
            pos_y_pt: p.pos_y_pt,
            pos_y_rel: p.pos_y_rel.clone(),
            pos_y_align: p.pos_y_align.clone(),
            pos_y_pct: p.pos_y_pct,
            overlap: p.overlap,
            txbx_texts: p.txbx.clone(),
            txbx_start,
            txbx_children: p.txbx_children.clone(),
            txbx_align: p.txbx_align.clone(),
            // 卡片契約 2 優先序：v:fill/@color（p.fill）> shape 屬性
            // fillcolor/o:fillcolor（p.fill_attr）> docDefaults（未接＝不發）。
            fill_argb: p.fill.or(p.fill_attr),
            filled: if p.filled_f { Some(false) } else { None },
            stroke_argb: p.stroke.or(p.stroke_attr),
            stroked: if p.stroked_f { Some(false) } else { None },
            stroke_width_pt: p.stroke_w,
            stroke_dash: p.dash.clone(),
            txbx_rect: p.txbx_rect.or(p.mapped_rect),
            txbx_inset: p.txbx_inset,
            txbx_vertical: p.txbx_vertical,
            txbx_table: p.tbl_rows.clone(),
            embed_rid: p.rid.clone(),
            embed_rids: p.rids.clone(),
        });
    }

    /// VML 累積完結：有圖片（imagedata 或 OLE 圖示 gfxdata）**或文字框**
    /// （K4e2：txbxContent——其文字不再走正文流，無框連字都不剩，必須撐位）
    /// 才採納；純裝飾 shape（無圖無框）仍不撐位。
    fn finalize_vml(&mut self) {
        let Some(v) = self.vml.take() else {
            return;
        };
        if v.vml_image || v.vml_gfxdata || v.has_txbx {
            self.push_pending(&v);
        }
    }
}

// ---- K4c：組映射兩函數為自由函數（只吃 v 與事件屬性、不碰解析器狀態）：
// 呼叫端在 `if let Some(v) = self.vml.as_mut()` 內，若再呼叫 &mut self 方法
// 會雙重借入 self（E0499），自由函數邊界最乾淨。----

/// K4c：`v:group` Start → 壓入 coord→extent 映射幀（[`CoordFrame`]）。
/// 巢狀組：子組自身 style 的 left/top/width/height（**無單位＝父組
/// coordorigin/coordsize 座標系單位**，PM 補正點）經父幀映射成 pt 後
/// 入棧；最外層組（coord_stack 空＝pict 直接子孫）以自身 style 的
/// pt 尺寸為 extent（VML px 慣例只適用 pict 直接子孫，本組有 pt 宣告）。
/// 缺 coordsize、子組 coord 矩形非法、或外框尺寸（pt）缺 → 壓 None
/// 毒化層，其子件老實不映射（呼叫端退回整框，不發射怪異座標）。
fn push_group_frame(v: &mut Pending, e: &quick_xml::events::BytesStart) {
    let style = attr_local(e, b"style");
    let frame: Option<CoordFrame> = (|| {
        let (size_w, size_h) = parse_coord_pair(attr_local(e, b"coordsize")?.trim())?;
        if size_w <= 0.0 || size_h <= 0.0 {
            return None;
        }
        let (origin_x, origin_y) = attr_local(e, b"coordorigin")
            .as_deref()
            .and_then(parse_coord_pair)
            .unwrap_or((0.0, 0.0));
        let (_, _, kv) = vml_style_map(style.as_deref());
        let (dx0, dy0, w_pt, h_pt) = match v.coord_stack.last().copied().flatten() {
            // 巢狀組：coord 矩形經父幀線性映射（映射不起 → 毒化）。
            Some(parent) => {
                let r = coord_style_rect(&kv)?;
                (
                    parent.map_x(r[0]),
                    parent.map_y(r[1]),
                    parent.scale_x(r[2]),
                    parent.scale_y(r[3]),
                )
            }
            // 最外層組：自身 style 即 extent。**只吃帶單位宣告**的
            // width/height（pt/cm/…）——無單位純數值在組語境是 coord
            // 單位而非 px，pict 直接子孫的 px 慣例不適用於組，缺父幀
            // 無從映射 → 老實毒化（不虛構 extent）。
            None => {
                let wv = style_lookup(&kv, "width").filter(|v| has_unit_suffix(v))?;
                let hv = style_lookup(&kv, "height").filter(|v| has_unit_suffix(v))?;
                let (w, h) = (
                    vml_style_wh(Some(&format!("width:{wv}"))).0?,
                    vml_style_wh(Some(&format!("height:{hv}"))).1?,
                );
                // 外層組即佔位框：把 extent 留給候補（呼叫端在 w/h 缺時採用）。
                v.w_from_group = Some(w);
                v.h_from_group = Some(h);
                (0.0, 0.0, w, h)
            }
        };
        if !(w_pt.is_finite() && h_pt.is_finite()) || w_pt <= 0.0 || h_pt <= 0.0 {
            return None;
        }
        Some(CoordFrame {
            origin_x,
            origin_y,
            size_w,
            size_h,
            dx0,
            dy0,
            w_pt,
            h_pt,
        })
    })();
    v.coord_stack.push(frame);
}

/// K4c：組內子件 Start → 若頂幀可用且子件 style 為純 coord 數值，
/// 線性映射出組內矩形（pt，相對佔位框原點）；首見為準。映射條件不足
/// （毒化層／帶單位值／缺尺寸）→ 不動 `mapped_rect`（None＝呼叫端
/// 退回整框，老實退化）。
fn map_child_shape(v: &mut Pending, e: &quick_xml::events::BytesStart) {
    if v.mapped_rect.is_some() {
        return;
    }
    let Some(frame) = v.coord_stack.last().copied().flatten() else {
        return;
    };
    let (_, _, kv) = vml_style_map(attr_local(e, b"style").as_deref());
    let Some(r) = coord_style_rect(&kv) else {
        return;
    };
    let rect = [
        frame.map_x(r[0]),
        frame.map_y(r[1]),
        frame.scale_x(r[2]),
        frame.scale_y(r[3]),
    ];
    if rect.iter().all(|x| x.is_finite()) && rect[2] > 0.0 && rect[3] > 0.0 {
        v.mapped_rect = Some(rect);
    }
}

impl DrawingParser {
    fn on_start(&mut self, name: &[u8], empty: bool, e: &quick_xml::events::BytesStart) {
        // ---- mc 雙份表示（先於一切：AC 透明、Fallback 吞沒——lib.rs 同構）----
        if self.fb > 0 {
            // Fallback 子樹內：只維護巢狀深度與 AC 快照＋K4c-image 破例——
            // blip/imagedata 的 rId 一律記進進行中候選的 `rids`（像素備選；
            // 幾何／段落／採納旗標一概不動，雙份去重語義不受影響）。
            if name == b"Fallback" && !empty {
                self.fb += 1;
            } else if name == b"AlternateContent" && !empty {
                self.ac_stack.push(self.fb);
            }
            let rid = match name {
                b"blip" => attr_local(e, b"embed"),
                b"imagedata" => attr_local(e, b"id"),
                _ => None,
            };
            if let Some(rid) = rid {
                if let Some(g) = self.graphic.as_mut() {
                    g.note_rid(Some(rid));
                } else if let Some(v) = self.vml.as_mut() {
                    v.note_rid(Some(rid));
                }
            }
            return;
        }
        match name {
            b"Fallback" if !empty => {
                self.fb += 1;
                return;
            }
            b"AlternateContent" if !empty => {
                // AC 透明：Choice 照常解析（圖形只此一份）。
                self.ac_stack.push(self.fb);
                return;
            }
            _ => {}
        }

        // ---- K4e2：txbxContent 深度追蹤（先於一切分支；此處 fb 必為 0）----
        if name == b"txbxContent" && !empty {
            if self.txbx_depth == 0 && self.skip_tag.is_none() {
                // R5：新子框起點——只記 DrawingML 候選（`wpg:wgp` 群組子框）；
                // VML 群組維持空（LO 對 VML 群組的抽取序未見反向實據，不動）。
                if let Some(g) = self.graphic.as_mut() {
                    g.txbx_children.push(0);
                }
            }
            self.txbx_depth += 1;
        }

        // ---- 吞沒幀（Graphic 內的巢狀 drawing/pict）：段落照計（口徑），
        // 雙份維護照常，其餘標記不看 ----
        if let Some((tag, depth)) = self.skip_tag {
            if name == b"p" && !empty && self.txbx_depth == 0 {
                self.p_count += 1;
            }
            if name == tag && !empty {
                self.skip_tag = Some((tag, depth + 1));
            }
            return;
        }

        // ---- K4d：`w:r` 包圍計數（E2 閂輸入；先於 VML/DrawingML 各分支）。
        // 自閉合 `w:r`（Empty 事件）不計，比照 `p_count` 口徑；`w:ins`／
        // `w:hyperlink` 等 run-level 容器不特判——計的是祖先 run，非父節點。----
        if name == b"r" && !empty {
            self.run_depth += 1;
        }

        // ---- 段落計數（三解析器同口徑；自閉合 w:p 不算；K4e2：框內段落不計）----
        if name == b"p" {
            if self.txbx_depth > 0 {
                // 框內段落：不計 p_count，宿主候補段落深度＋1（End 配對 flush）。
                if !empty {
                    if let Some(v) = self.vml.as_mut() {
                        v.txbx_para_depth += 1;
                    }
                    if let Some(g) = self.graphic.as_mut() {
                        g.txbx_para_depth += 1;
                    }
                }
            } else if !empty {
                self.p_count += 1;
            }
            return;
        }

        // K10：文字框內對齊（例如 <w:jc w:val="center"/>）。
        if self.txbx_depth > 0 && name == b"jc" {
            let val = attr_local(e, b"val");
            if let Some(v) = self.vml.as_mut() {
                if v.txbx_align.is_none() {
                    v.txbx_align = val.clone();
                }
            }
            if let Some(g) = self.graphic.as_mut() {
                if g.txbx_align.is_none() {
                    g.txbx_align = val;
                }
            }
        }

        // ---- VML 累積（pict 內或裸 shape）----
        if self.pict_depth > 0 || self.vml.is_some() {
            // 文字框 w:txbxContent 段落：K4e2 起**不佔正文槽**（主解析器改掛
            // `Paragraph::txbx_texts`）；外層 pict/shape 的 w:p 仍照計。
            if name == b"p" && !empty && self.txbx_depth == 0 {
                self.p_count += 1;
            }
            if name == b"txbxContent" && !empty {
                if let Some(v) = self.vml.as_mut() {
                    v.has_txbx = true;
                }
            }
            // 框內文字採集（K4e2）：p 開段、t 開文字區間。
            if self.txbx_depth > 0 {
                if let Some(v) = self.vml.as_mut() {
                    match name {
                        b"p" if !empty => v.txbx_para_depth += 1,
                        b"t" => v.txbx_in_t = true,
                        _ => {}
                    }
                }
            }
            match name {
                b"imagedata" => {
                    if let Some(v) = self.vml.as_mut() {
                        v.vml_image = true;
                        // K4c-image：經 note_rid 統一記錄（首見進 rid＋全部進
                        // rids）；語義與舊「首見才設 rid」逐位相同。
                        v.note_rid(attr_local(e, b"id"));
                    }
                }
                b"gfxdata" => {
                    if let Some(v) = self.vml.as_mut() {
                        v.vml_gfxdata = true;
                    }
                }
                b"pict" if !empty => {
                    // pict 內嵌 pict（文字框內再放圖）：深度計數；標記續寫
                    // 同一累積（首見尺寸為準＝外框，LO 同款近似）。
                    self.pict_depth += 1;
                }
                b"fill" | b"stroke" | b"textbox" => {
                    if let Some(v) = self.vml.as_mut() {
                        apply_vml_deco(v, name, e);
                    }
                }
                b"tbl" | b"tr" => {}
                b"tc" if !empty => {
                    if let Some(v) = self.vml.as_mut() {
                        if self.txbx_depth > 0 {
                            v.tc_in += 1;
                        }
                    }
                }
                b"shd" => {
                    // 框內表格格填色（n780563 三色列實據）：w:tc 內的
                    // w:shd/@w:fill，首見為準。
                    if self.txbx_depth > 0 {
                        if let Some(v) = self.vml.as_mut() {
                            if v.tc_in > 0 && v.cell_fill.is_none() {
                                v.cell_fill = parse_vml_color(attr_local(e, b"fill").as_deref());
                            }
                        }
                    }
                }
                n if is_vml_shape(n) && !empty => {
                    self.shape_depth += 1;
                    if let Some(v) = self.vml.as_mut() {
                        if n == b"group" {
                            // K4c：組遞進——座標系幀入棧；外層組（首見且
                            // 帶單位宣告）以自身 style 撐框尺寸。
                            push_group_frame(v, e);
                            if v.w.is_none() && v.h.is_none() {
                                if let (Some(w), Some(h)) = (v.w_from_group, v.h_from_group) {
                                    v.w = Some(w);
                                    v.h = Some(h);
                                }
                            }
                        } else {
                            if v.w.is_none() && v.h.is_none() {
                                let (w, h) = vml_style_wh(attr_local(e, b"style").as_deref());
                                v.w = w;
                                v.h = h;
                            }
                            map_child_shape(v, e);
                            apply_shape_style(v, e);
                        }
                        // R3：VML 絕對定位只認最外層（group 自身或無組包裝的
                        // 頂層 shape）；組內子件的 position:absolute 是組座標
                        // 系宣告，不得取代外框鎖點（n751077 子 rect 實據）。
                        if n == b"group" || v.coord_stack.is_empty() {
                            apply_vml_position(v, e);
                        }
                    }
                }
                _ => {}
            }
            return;
        }

        // ---- DrawingML 候選進行中：只收 extent／position／blip；
        // 巢狀 drawing/pict 整棵吞沒（內層 extent 不得竄位外框）----
        if self.graphic.is_some() {
            // 群組圖形的 wps:txbx/w:txbxContent 段落：K4e2 起**不佔正文槽**
            // （主解析器改掛 `Paragraph::txbx_texts`）；外層段落仍照計。
            if name == b"p" && !empty && self.txbx_depth == 0 {
                self.p_count += 1;
            }
            let mut p = self.graphic.take().expect("剛檢查過");
            if name == b"txbxContent" && !empty {
                p.has_txbx = true;
            }
            // R6：DrawingML `wps:wsp` 標註——只有此類候選吃宿主框裁切／z 序。
            if name == b"wsp" {
                p.wps_annotation = true;
            }
            // K4c：wps:spPr 樣式上下文深度（填充／描邊歸屬判別用）。
            match name {
                b"spPr" => p.sp_ctx += 1,
                // 擴充清單標記：`a:extLst` 與其子 `a:ext/@uri`（帶 uri 者
                // 才是擴充清單節點；xfrm 內的幾何 `a:ext cx/cy` 同名不同物）。
                b"extLst" => p.ext_ctx += 1,
                b"ext" if attr_local(e, b"uri").is_some() => p.ext_ctx += 1,
                b"ln" => {
                    p.ln_ctx += 1;
                    // a:ln/@w（EMU）＝描邊線寬（首見為準；擴充清單內的
                    // hiddenLine 不計——那不是真的可見描邊）。
                    if p.stroke_w.is_none() && p.ext_ctx == 0 {
                        p.stroke_w = emu_to_pt(attr_local(e, b"w").as_deref());
                    }
                }
                b"solidFill" => p.fill_ctx += 1,
                b"srgbClr" if p.fill_ctx > 0 && p.ext_ctx == 0 => {
                    // 卡片契約 2 優先序（DrawingML 路徑）：`a:ln` 內的
                    // solidFill 歸**描邊**，否則（spPr 直下）歸填充。首見為準。
                    let argb = parse_vml_color(attr_local(e, b"val").as_deref());
                    if let Some(c) = argb {
                        if p.ln_ctx > 0 {
                            if p.stroke.is_none() {
                                p.stroke = Some(c);
                            }
                        } else if p.sp_ctx > 0 && p.fill.is_none() {
                            p.fill = Some(c);
                        }
                    }
                }
                _ => {}
            }
            // 框內文字採集（K4e2；群組圖形 wps:txbx 路徑）。
            if self.txbx_depth > 0 {
                match name {
                    b"p" if !empty => p.txbx_para_depth += 1,
                    b"t" => p.txbx_in_t = true,
                    _ => {}
                }
            }
            match name {
                b"extent" => {
                    if p.w.is_none() {
                        p.w = emu_to_pt(attr_local(e, b"cx").as_deref());
                    }
                    if p.h.is_none() {
                        p.h = emu_to_pt(attr_local(e, b"cy").as_deref());
                    }
                }
                b"positionH" | b"positionV" => {
                    let vertical = name == b"positionV";
                    let rel = attr_local(e, b"relativeFrom");
                    if vertical {
                        p.pos_y_rel = rel;
                    } else {
                        p.pos_x_rel = rel;
                    }
                    // 軸向記帳但**不開啟捕獲**（t_c1fb4c69 S1-5）：capture 只
                    // 屬 posOffset 元素本體；align/pct 形態整棵都不該吞文字。
                    p.pos_axis = Some(vertical);
                }
                b"posOffset" => {
                    // 真正的 EMU 偏移文本容器：開標才啟捕獲（S1-5 修法）。
                    p.capture = p.pos_axis;
                }
                b"align" => {
                    // 對齊枚舉文本容器（t_1a79f1c5）：positionH/V 開標過
                    // （pos_axis 有值）才認——直存字串走 on_text 的
                    // align_axis 分支，不走 EMU 數值解析。
                    p.align_axis = p.pos_axis;
                }
                b"pctPosHOffset" => {
                    // 百分比偏移文本容器（OOXMLExport 位圖第三式，t_1a79f1c5）。
                    p.pct_axis = Some(false);
                }
                b"pctPosVOffset" => {
                    p.pct_axis = Some(true);
                }
                b"blip" => {
                    // K4c-image：經 note_rid 統一記錄（首見進 rid＋全部進
                    // rids）；rid 語義與舊「首見才設」逐位相同。
                    p.note_rid(attr_local(e, b"embed"));
                }
                b"drawing" | b"pict" if !empty => {
                    let close: &'static [u8] = if name == b"drawing" {
                        b"drawing"
                    } else {
                        b"pict"
                    };
                    self.skip_tag = Some((close, 0));
                }
                _ => {}
            }
            self.graphic = Some(p);
            return;
        }

        // ---- 常規結構 ----
        match name {
            b"drawing" if !empty => {
                self.drawing_depth += 1;
            }
            b"inline" | b"anchor" if !empty && self.drawing_depth > 0 => {
                let floating = name == b"anchor";
                // allowOverlap 是直上布林屬性（corpus 實形 `allowOverlap="1"`）。
                let overlap = if floating {
                    attr_local(e, b"allowOverlap").map(|v| {
                        !matches!(
                            v.trim().to_ascii_lowercase().as_str(),
                            "none" | "false" | "0" | "off"
                        )
                    })
                } else {
                    None
                };
                // K10：只有非浮動的 inline 圖形才視為占用格內流的 in_cell；
                // 浮動 anchor 即令錨在儲存格段落，仍走頁面浮動層定位管線。
                let in_cell = if self.cell_anchor_buggy {
                    self.tc_depth > 0
                } else {
                    self.tc_depth > 0 && !floating
                };
                let mut p = Pending::new(self.cur_para(), in_cell);
                p.overlap = overlap;
                p.floating = floating;
                // R6：z 序（LO 抽取序）——anchor 的 relativeHeight；inline 無。
                if floating {
                    p.rel_height = attr_local(e, b"relativeHeight")
                        .and_then(|v| v.trim().parse::<i64>().ok())
                        .unwrap_or(0);
                }
                self.graphic = Some(Box::new(p));
            }
            b"pict" if !empty => {
                // K4d（E2）：錯置 pict（非 `w:r` 子孫）整棵吞沒——LO 唯一繪製
                // 開關＝run 包圍（§1.5 C/G：合法包 run 後任何 wrap 都照畫，
                // 故閂只認 run 包圍，不順手加 wrap/absolute 條件）。
                if self.run_depth > 0 || self.buggy {
                    self.pict_depth += 1;
                    if self.vml.is_none() {
                        self.vml = Some(Box::new(Pending::new(self.cur_para(), self.tc_depth > 0)));
                    }
                } else {
                    self.skip_tag = Some((b"pict", 0));
                }
            }
            n if is_vml_shape(n) && !empty => {
                // 裸 VML shape（Word OLE 常態：w:object 下 v:shape 無 pict 包）。
                // K4d：同閂——run 外裸 shape 照樣吞沒（corpus 全數在 run 內，
                // 零擾動；處方依 k5-diag §1.5 卡面，勿自行放寬）。
                let Some(close) = vml_shape_close(n) else {
                    return;
                };
                if self.run_depth > 0 || self.buggy {
                    self.shape_depth += 1;
                    if self.vml.is_none() {
                        let (w, h) = vml_style_wh(attr_local(e, b"style").as_deref());
                        let mut p = Pending::new(self.cur_para(), self.tc_depth > 0);
                        p.w = w;
                        p.h = h;
                        // R3：裸 VML shape 的 position:absolute 同樣入鎖點。
                        apply_vml_position(&mut p, e);
                        self.vml = Some(Box::new(p));
                    }
                } else {
                    self.skip_tag = Some((close, 0));
                }
            }
            b"tc" if !empty => self.tc_depth += 1,
            _ => {}
        }
    }

    /// Empty 事件：extent／blip／imagedata 幾乎都自閉合，與 Start 分支拆開
    /// 處理，避免「收了 Start 漏了 Empty」的漏口。
    fn on_empty(&mut self, name: &[u8], e: &quick_xml::events::BytesStart) {
        // 自閉合 Fallback／AC：不改 fb（無子樹可吞）。
        if matches!(name, b"Fallback" | b"AlternateContent") {
            return;
        }
        if self.fb > 0 {
            // K4c-image 破例（同 Start 分支）：自閉合 blip／imagedata 才是
            // **常態**（Bug51170 的 `<a:blip r:embed="rId2"/>` 就是自閉合），
            // 只記 rId，其餘一概不動。
            let rid = match name {
                b"blip" => attr_local(e, b"embed"),
                b"imagedata" => attr_local(e, b"id"),
                _ => None,
            };
            if let Some(rid) = rid {
                if let Some(g) = self.graphic.as_mut() {
                    g.note_rid(Some(rid));
                } else if let Some(v) = self.vml.as_mut() {
                    v.note_rid(Some(rid));
                }
            }
            return;
        }
        if self.skip_tag.is_some() {
            return; // 自閉合 w:p 於主解析器同樣不計
        }

        // K10：文字框內對齊（例如 <w:jc w:val="center"/>）。
        if self.txbx_depth > 0 && name == b"jc" {
            let val = attr_local(e, b"val");
            if let Some(v) = self.vml.as_mut() {
                if v.txbx_align.is_none() {
                    v.txbx_align = val.clone();
                }
            }
            if let Some(g) = self.graphic.as_mut() {
                if g.txbx_align.is_none() {
                    g.txbx_align = val;
                }
            }
        }

        if self.pict_depth > 0 || self.vml.is_some() {
            match name {
                b"imagedata" => {
                    if let Some(v) = self.vml.as_mut() {
                        v.vml_image = true;
                        // K4c-image：同 Start 分支，經 note_rid 統一記錄。
                        v.note_rid(attr_local(e, b"id"));
                    }
                }
                b"gfxdata" => {
                    if let Some(v) = self.vml.as_mut() {
                        v.vml_gfxdata = true;
                    }
                }
                // K4c：v:fill／v:stroke／v:textbox 常為自閉合標籤（tdf152200
                // 實據 `<v:fill .../>`）——Empty 事件與 Start 共用採集口徑，
                // 否則自閉合的填色／描邊／inset 全數漏收。
                b"fill" | b"stroke" | b"textbox" => {
                    if let Some(v) = self.vml.as_mut() {
                        apply_vml_deco(v, name, e);
                    }
                }
                b"shd" if self.txbx_depth > 0 => {
                    // 框內表格格填色（n780563 三色列實據）：Empty 形態同警。
                    if let Some(v) = self.vml.as_mut() {
                        if v.tc_in > 0 && v.cell_fill.is_none() {
                            v.cell_fill = parse_vml_color(attr_local(e, b"fill").as_deref());
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        if let Some(p) = self.graphic.as_mut() {
            match name {
                b"extent" => {
                    if p.w.is_none() {
                        p.w = emu_to_pt(attr_local(e, b"cx").as_deref());
                    }
                    if p.h.is_none() {
                        p.h = emu_to_pt(attr_local(e, b"cy").as_deref());
                    }
                }
                b"blip" => {
                    // K4c-image：同 Start 分支，經 note_rid 統一記錄。
                    p.note_rid(attr_local(e, b"embed"));
                }
                // K4c：`a:srgbClr` 幾乎總以自閉合形態出現（tdf152200 實據
                // `<a:srgbClr val="000000"/>`）——Empty 事件同樣要落色，
                // 歸屬判別與 Start 臂同口徑（ln 內→描邊、spPr 直下→填充）。
                b"srgbClr" if p.fill_ctx > 0 && p.ext_ctx == 0 => {
                    if let Some(c) = parse_vml_color(attr_local(e, b"val").as_deref()) {
                        if p.ln_ctx > 0 {
                            if p.stroke.is_none() {
                                p.stroke = Some(c);
                            }
                        } else if p.sp_ctx > 0 && p.fill.is_none() {
                            p.fill = Some(c);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn on_text(&mut self, text: &str) {
        if self.fb > 0 || self.skip_tag.is_some() {
            return;
        }
        // K4e2：框內文字採集（t 區間內；只進宿主候補的 txbx，不碰捕獲態）。
        if self.txbx_depth > 0 {
            if let Some(v) = self.vml.as_mut() {
                if v.txbx_in_t {
                    v.txbx_cur.push_str(text);
                }
            }
            if let Some(g) = self.graphic.as_mut() {
                if g.txbx_in_t {
                    g.txbx_cur.push_str(text);
                }
            }
            return;
        }
        if let Some(p) = self.graphic.as_mut() {
            if let Some(vertical) = p.align_axis {
                // `<wp:align>` 對齊枚舉：直存字串（trim 後），不解析數值
                //（t_1a79f1c5；OOXMLExport 封面圖族 positionH/V 只有 align）。
                let v = text.trim().to_ascii_lowercase();
                if !v.is_empty() {
                    if vertical {
                        if p.pos_y_align.is_none() {
                            p.pos_y_align = Some(v);
                        }
                    } else if p.pos_x_align.is_none() {
                        p.pos_x_align = Some(v);
                    }
                }
            }
            if let Some(vertical) = p.pct_axis {
                // `<wp14:pctPos[HV]Offset>`：百分比×100000 整數直存（換算
                // 成 pt 由 engine 端依 frame 尺寸做，t_1a79f1c5）。
                let parsed = text
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|x| x.is_finite() && x.abs() <= MAX_OOXML_LONG)
                    .map(|x| x as f32)
                    .filter(|v| v.is_finite());
                if let Some(val) = parsed {
                    if vertical {
                        if p.pos_y_pct.is_none() {
                            p.pos_y_pct = Some(val);
                        }
                    } else if p.pos_x_pct.is_none() {
                        p.pos_x_pct = Some(val);
                    }
                }
            }
            if let Some(vertical) = p.capture {
                // posOffset 合法值含負數（版心外偏移）：不套 emu_to_pt 的正數
                // 限制，但守衛同 emu_to_pt 口徑補齊**鏡像後**那層（t_16a13211
                // R1）：舊實作只查 f64 域 finite，4.3e43÷12700 在 f64 有限、
                // `as f32` 溢出成 inf 照收——inf 座標流入顯示幾何。
                let parsed = text
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|x| x.is_finite() && x.abs() <= MAX_OOXML_LONG)
                    .map(|x| (x / EMU_PER_PT) as f32)
                    .filter(|v| v.is_finite());
                if let Some(val) = parsed {
                    if vertical {
                        if p.pos_y_pt.is_none() {
                            p.pos_y_pt = Some(val);
                        }
                    } else if p.pos_x_pt.is_none() {
                        p.pos_x_pt = Some(val);
                    }
                }
            }
        }
    }

    fn on_end(&mut self, name: &[u8]) {
        // ---- MC 閉合先行（錯置標籤不致卡死，同主解析器策略）----
        if name == b"AlternateContent" {
            self.fb = self.ac_stack.pop().unwrap_or(0);
            return;
        }
        if name == b"Fallback" {
            self.fb = self.fb.saturating_sub(1);
            return;
        }
        if self.fb > 0 {
            return;
        }

        // ---- K4e2：txbxContent 完結（先於各分支，與計數點同深度口徑）----
        if name == b"txbxContent" {
            self.txbx_depth = self.txbx_depth.saturating_sub(1);
        }
        // ---- K4e2：框內 p／t 完結——文字 flush 進宿主候補的 txbx ----
        if self.txbx_depth > 0 && matches!(name, b"p" | b"t") {
            let td = self.txbx_depth;
            if let Some(v) = self.vml.as_mut() {
                if name == b"t" {
                    v.txbx_in_t = false;
                } else {
                    v.txbx_para_depth = v.txbx_para_depth.saturating_sub(1);
                    if v.txbx_para_depth == 0 {
                        v.txbx.push(std::mem::take(&mut v.txbx_cur));
                    }
                }
            }
            if let Some(g) = self.graphic.as_mut() {
                if name == b"t" {
                    g.txbx_in_t = false;
                } else {
                    g.txbx_para_depth = g.txbx_para_depth.saturating_sub(1);
                    if g.txbx_para_depth == 0 {
                        g.txbx.push(std::mem::take(&mut g.txbx_cur));
                        // R5：本子框（頂層 txbxContent）每 flush 一段就累加；
                        // 巢狀框（depth>1）不計入外層子框統計。
                        if td == 1 {
                            if let Some(c) = g.txbx_children.last_mut() {
                                *c += 1;
                            }
                        }
                    }
                }
            }
        }

        // ---- K4c：框內表格填色 flush（tc 完結→本列；tr 完結→本表）。----
        if self.txbx_depth > 0 && matches!(name, b"tc" | b"tr" | b"tbl") {
            if let Some(v) = self.vml.as_mut() {
                match name {
                    b"tc" => {
                        if v.tc_in > 0 {
                            v.tc_in -= 1;
                            v.tbl_row.push(v.cell_fill.take());
                        }
                    }
                    b"tr" if !v.tbl_row.is_empty() => {
                        v.tbl_rows.push(std::mem::take(&mut v.tbl_row));
                    }
                    _ => {}
                }
            }
        }

        // ---- 吞沒幀 ----
        if let Some((tag, depth)) = self.skip_tag {
            if name == tag {
                if depth == 0 {
                    self.skip_tag = None;
                } else {
                    self.skip_tag = Some((tag, depth - 1));
                }
            }
            return;
        }

        // ---- K4d：`w:r` 包圍計數閉合（與 Start 配對；fb／吞沒幀內 Start 未
        // 計、此處同不減——兩側早退點同構，深度必平衡）。----
        if name == b"r" {
            self.run_depth = self.run_depth.saturating_sub(1);
        }

        // ---- 儲存格邊界（in_cell 旗標在候選建立時已定格，閉合只管深度）----
        if name == b"tc" {
            self.tc_depth = self.tc_depth.saturating_sub(1);
        }

        // ---- VML 完結 ----
        if self.pict_depth > 0 || self.shape_depth > 0 {
            if name == b"pict" {
                self.pict_depth = self.pict_depth.saturating_sub(1);
                if self.pict_depth == 0 && self.shape_depth == 0 {
                    self.finalize_vml();
                }
                return;
            }
            if is_vml_shape(name) {
                self.shape_depth = self.shape_depth.saturating_sub(1);
                // pict 尚未閉合時不提前完結（外層 pict End 統一收尾）。
                if self.pict_depth == 0 && self.shape_depth == 0 {
                    self.finalize_vml();
                }
                return;
            }
        }

        // ---- DrawingML：欄位捕獲狀態收尾（t_c1fb4c69 S1-5）：posOffset 閉合
        // 必清 capture（舊實作永不重置＝狀態洩漏的主因）；positionH/V 閉合
        // 清軸向。graphic 完結路徑（inline/anchor End）在下方另行 take。----
        if self.graphic.is_some() {
            match name {
                b"posOffset" => {
                    if let Some(p) = self.graphic.as_mut() {
                        p.capture = None;
                    }
                    return;
                }
                b"align" => {
                    if let Some(p) = self.graphic.as_mut() {
                        p.align_axis = None;
                    }
                    return;
                }
                b"pctPosHOffset" | b"pctPosVOffset" => {
                    if let Some(p) = self.graphic.as_mut() {
                        p.pct_axis = None;
                    }
                    return;
                }
                b"positionH" | b"positionV" => {
                    if let Some(p) = self.graphic.as_mut() {
                        p.pos_axis = None;
                        p.capture = None;
                        p.align_axis = None;
                        p.pct_axis = None;
                    }
                    return;
                }
                // K4c：spPr 樣式上下文閉合出棧（與 Start 的 += 配對）。
                b"spPr" => {
                    if let Some(p) = self.graphic.as_mut() {
                        p.sp_ctx = p.sp_ctx.saturating_sub(1);
                    }
                    return;
                }
                b"extLst" | b"ext" => {
                    // End 不辨屬性（閉合標籤無 attrs）：Ext/Start 的 uri 判別
                    // 已保證計數配對（同一對開閉都算或都不算——xfrm 幾何 ext
                    // 開標不計，閉標 saturating_sub 於 0 亦不改其義）。
                    if let Some(p) = self.graphic.as_mut() {
                        p.ext_ctx = p.ext_ctx.saturating_sub(1);
                    }
                    return;
                }
                b"ln" => {
                    if let Some(p) = self.graphic.as_mut() {
                        p.ln_ctx = p.ln_ctx.saturating_sub(1);
                    }
                    return;
                }
                b"solidFill" => {
                    if let Some(p) = self.graphic.as_mut() {
                        p.fill_ctx = p.fill_ctx.saturating_sub(1);
                    }
                    return;
                }
                _ => {}
            }
        }

        // ---- DrawingML 完結 ----
        if self.graphic.is_some() && matches!(name, b"inline" | b"anchor") {
            if let Some(p) = self.graphic.take() {
                self.push_pending(&p);
            }
            return;
        }
        if name == b"drawing" {
            self.drawing_depth = self.drawing_depth.saturating_sub(1);
        }
    }
}

/// 屬性查值：local-name 配對（`r:embed`／`wp:cx` 皆不假設前綴綁定）。
fn attr_local(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key {
            a.unescape_value().ok().map(|v| v.into_owned())
        } else {
            None
        }
    })
}

/// 事件流單趟餵入（read_images／count_paras 共用，口徑必然一致）。
fn run_parser(xml: &[u8]) -> DrawingParser {
    use quick_xml::events::Event;
    let text = String::from_utf8_lossy(xml).into_owned();
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut p = DrawingParser::new();
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                p.on_start(&name, false, &e);
            }
            Ok(Event::Empty(e)) => {
                let name = e.local_name().as_ref().to_vec();
                p.on_empty(&name, &e);
            }
            Ok(Event::Text(t)) => {
                if let Ok(s) = t.unescape() {
                    p.on_text(&s);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name().as_ref().to_vec();
                p.on_end(&name);
            }
            Err(_) => break, // XML 畸形：老實返回已收集的幾何（容錯不 panic）
            _ => {}
        }
    }
    p
}

/// 解析 document.xml 全文的圖片幾何（純函數；engine／測試入口）。
///
/// 輸出依文档序；`para_index` 用扁平 Start 口徑（與 `parse_document_xml`
/// 逐檔一致，corpus 柵欄鎖死）。
pub fn read_images(xml: &[u8]) -> Vec<InlineImage> {
    run_parser(xml).out
}

/// 同口徑段落計數器（corpus 柵欄用）：與 `read_images` 同一狀態機走完
/// `w:p` Start 計數——測試據此與 `parse_document_xml`／`count_flat_paras`
/// 逐檔交叉驗證（四解析器同口徑契約）。
pub fn count_paras(xml: &[u8]) -> usize {
    run_parser(xml).p_count
}

#[cfg(test)]
mod tests {
    use super::*;

    const W_NS: &str =
        r##"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""##;

    fn doc(body: &str) -> String {
        format!(r#"<w:document {W_NS}><w:body>{body}</w:body></w:document>"#)
    }

    #[test]
    fn emu_conversion_and_guards() {
        assert_eq!(emu_to_pt(Some("12700")), Some(1.0));
        assert_eq!(emu_to_pt(Some("914400")), Some(72.0));
        assert_eq!(emu_to_pt(Some("1398905")).unwrap().round(), 110.0);
        assert_eq!(emu_to_pt(Some("0")), None);
        assert_eq!(emu_to_pt(Some("-5")), None);
        assert_eq!(emu_to_pt(Some("abc")), None);
        assert_eq!(emu_to_pt(None), None);
        // 畸形巨大值（t_c1fb4c69 S1-4）：f64 有限但 ÷12700 後 as f32 溢出成 inf
        // → 必須 None（不採納），否則 inf 高度下游 bands 爆量 OOM。
        assert_eq!(emu_to_pt(Some("4000000000000000000000")), None);
        assert_eq!(emu_to_pt(Some("1e39")), None);
        assert_eq!(emu_to_pt(Some("1e400")), None); // f64 即 inf
                                                    // 4000 億 EMU ≈ 3.1e7pt：f32 表得住（有限）→ 老實保留（爆量由 bands 上限兜底）。
        assert!(emu_to_pt(Some("400000000000")).unwrap().is_finite());
    }

    #[test]
    fn vml_style_units() {
        assert_eq!(
            vml_style_wh(Some("width:100pt;height:50pt")),
            (Some(100.0), Some(50.0))
        );
        assert_eq!(
            vml_style_wh(Some("width:2.5cm;height:1in"))
                .0
                .unwrap()
                .round(),
            71.0
        );
        let (w, h) = vml_style_wh(Some("width:96px;height:48px"));
        assert_eq!((w.unwrap().round(), h.unwrap().round()), (72.0, 36.0));
        // 無單位按 px；其餘宣告忽略；非法側＝該側 None。
        assert_eq!(
            vml_style_wh(Some("width:192;height:96")),
            (Some(144.0), Some(72.0))
        );
        assert_eq!(
            vml_style_wh(Some("position:absolute;width:50pt")),
            (Some(50.0), None)
        );
        assert_eq!(vml_style_wh(Some("width:50%;height:50%")), (None, None));
        assert_eq!(vml_style_wh(None), (None, None));
        // 畸形巨大值（t_c1fb4c69 S1-4）：f64 有限、換算後 as f32 溢出成 inf →
        // 整側 None（字面長數字不走科學記號——本解析器不認 e 記號）。
        assert_eq!(
            vml_style_wh(Some(
                "width:9999999999999999999999999999999999999999999;height:12pt"
            ))
            .0,
            None
        );
        assert_eq!(vml_style_wh(Some("width:1e39pt;height:12pt")).0, None); // 非法單位
                                                                            // 4e9in＝2.88e11pt：f32 表得住（有限）→ 老實保留，爆量由 bands 上限兜底。
        assert!(vml_style_wh(Some("width:4000000000in;height:12pt"))
            .0
            .unwrap()
            .is_finite());
    }

    #[test]
    fn inline_image_extents_and_anchor_band() {
        let xml = doc(
            r#"<w:p><w:r><w:drawing><wp:inline xmlns:wp="x"><wp:extent cx="1270000" cy="1270000"/>
               <a:graphic xmlns:a="y"><a:graphicData><pic:pic xmlns:pic="z">
               <a:blip xmlns:r="r" r:embed="rId5"/>
               </pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        let im = &imgs[0];
        assert_eq!(im.para_index, 0);
        assert_eq!((im.w_pt, im.h_pt), (100.0, 100.0));
        assert!(!im.floating);
        assert!(!im.in_cell);
        assert_eq!(im.embed_rid.as_deref(), Some("rId5"));
    }

    #[test]
    fn anchor_with_lockpoints_drops_bands() {
        let xml = doc(
            r#"<w:p/><w:p><w:r><w:drawing><wp:anchor allowOverlap="1"><wp:positionH relativeFrom="page">
               <wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page">
               <wp:posOffset>-127000</wp:posOffset></wp:positionV><wp:extent cx="635000" cy="635000"/>
               </wp:anchor></w:drawing></w:r></w:p>"#,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        let im = &imgs[0];
        assert!(im.floating);
        assert_eq!(im.overlap, Some(true));
        assert_eq!(im.anchor_lock_pt(), Some((72.0, -10.0)));
        assert_eq!(origin_of(im.pos_x_rel.as_deref()), "page");
        assert_eq!(origin_of(im.pos_y_rel.as_deref()), "page");
    }

    #[test]
    fn align_only_position_never_captures_textbox_digits() {
        // capture 狀態洩漏（t_c1fb4c69 S1-5）：positionH/V 用 <wp:align>（無
        // posOffset）時，on_start 設了 capture 而 on_end 永不重置——之後
        // w:txbxContent 的文字「123」被當成 EMU 填入 pos_x/pos_y（123/12700
        // ≈0.0097pt 的幽靈鎖點）。修法＝只在 posOffset 元素啟閉捕獲。
        let xml = doc(
            r#"<w:p><w:r><w:drawing><wp:anchor><wp:positionH relativeFrom="column">
               <wp:align>center</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph">
               <wp:align>top</wp:align></wp:positionV><wp:extent cx="127000" cy="127000"/>
               <wps:txbx xmlns:wps="w"><w:txbxContent><w:p><w:r><w:t>123</w:t></w:r></w:p></w:txbxContent></wps:txbx>
               </wp:anchor></w:drawing></w:r></w:p> "#,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert_eq!(
            imgs[0].pos_x_pt, None,
            "align 對齊不是 posOffset：文字數字不得當成 EMU 鎖點"
        );
        assert_eq!(imgs[0].pos_y_pt, None, "positionV 同口徑");
        assert_eq!(
            imgs[0].anchor_lock_pt(),
            None,
            "鎖點不可用→呼叫端退化 inline"
        );
    }

    #[test]
    fn posoffset_capture_still_works_after_leak_fix() {
        // 過度修正防護（t_c1fb4c69 S1-5 綠燈伴測）：真正的 posOffset 兩軸
        // 照常捕獲（10pt／2pt），修 leak 不得把正路徑一起修壞。
        let xml = doc(
            r#"<w:p><w:r><w:drawing><wp:anchor><wp:positionH relativeFrom="column">
               <wp:posOffset>127000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page">
               <wp:posOffset>25400</wp:posOffset></wp:positionV><wp:extent cx="127000" cy="127000"/>
               </wp:anchor></w:drawing></w:r></w:p>"#,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs[0].anchor_lock_pt(), Some((10.0, 2.0)));
    }

    #[test]
    fn posoffset_beyond_f32_range_rejected() {
        // 病態檔（t_16a13211 R1）：4.3e43 EMU ÷12700 在 f64 有限（≈3.4e39），
        // 但 `as f32` 溢出成 inf——舊守衛只查 f64 域，inf 直接進 pos_x_pt／
        // pos_y_pt 流入顯示幾何。修法＝f64 值域＋f32 鏡像後雙重守衛（同
        // emu_to_pt 口徑，t_c1fb4c69 守衛族補齊本臂）。
        let xml = doc(
            r#"<w:p><w:r><w:drawing><wp:anchor><wp:positionH relativeFrom="column">
               <wp:posOffset>4.3e43</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page">
               <wp:posOffset>-4.3e43</wp:posOffset></wp:positionV><wp:extent cx="127000" cy="127000"/>
               </wp:anchor></w:drawing></w:r></w:p>"#,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert_eq!(
            imgs[0].pos_x_pt, None,
            "f32 鏡像溢出＝拒收，inf 不得流入下游幾何"
        );
        assert_eq!(imgs[0].pos_y_pt, None, "負號側同口徑");
        assert_eq!(
            imgs[0].anchor_lock_pt(),
            None,
            "鎖點不可用→呼叫端退化 inline"
        );
        // 過度修正防護：合法負偏移（版心外）照樣採納——本臂不得套 emu_to_pt
        // 的正數限制，只補鏡像後 finite 那層。
        let xml2 = doc(
            r#"<w:p><w:r><w:drawing><wp:anchor><wp:positionH relativeFrom="column">
               <wp:posOffset>-127000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page">
               <wp:posOffset>25400</wp:posOffset></wp:positionV><wp:extent cx="127000" cy="127000"/>
               </wp:anchor></w:drawing></w:r></w:p>"#,
        );
        let imgs2 = read_images(xml2.as_bytes());
        assert_eq!(imgs2[0].anchor_lock_pt(), Some((-10.0, 2.0)));
    }

    #[test]
    fn anchor_missing_posoffset_degrades_to_inline() {
        // 卡片鐵律：posOffset 缺任一軸＝鎖點不可用 → 退化 inline（呼叫端佔帶）。
        let xml = doc(
            r#"<w:p><w:r><w:drawing><wp:anchor><wp:positionH relativeFrom="page">
               <wp:posOffset>12700</wp:posOffset></wp:positionH><wp:extent cx="127000" cy="127000"/>
               </wp:anchor></w:drawing></w:r></w:p>"#,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert!(imgs[0].floating);
        assert_eq!(imgs[0].anchor_lock_pt(), None, "缺 positionV＝鎖點不可用");
    }

    #[test]
    fn origin_frame_classification() {
        assert_eq!(origin_of(Some("page")), "page");
        assert_eq!(origin_of(Some("pageMargin")), "page");
        assert_eq!(origin_of(Some("column")), "margin");
        assert_eq!(origin_of(Some("margin")), "margin");
        assert_eq!(origin_of(Some("paragraph")), "paragraph");
        assert_eq!(origin_of(Some("line")), "paragraph");
        assert_eq!(origin_of(None), "paragraph");
        // margin 枚舉家族（t_c1fb4c69 S2-8，ECMA-376 ST_RelFromH/V）：六個
        // *Margin 值同屬「版心」原點系，漏歸類會錯降為 paragraph 原點。
        for k in [
            "topMargin",
            "bottomMargin",
            "leftMargin",
            "rightMargin",
            "insideMargin",
            "outsideMargin",
        ] {
            assert_eq!(origin_of(Some(k)), "margin", "{k} 應歸 margin 原點系");
        }
    }

    #[test]
    fn missing_or_invalid_extent_not_emitted() {
        let xml = doc(
            r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="junk" cy="1270000"/></wp:inline></w:drawing></w:r></w:p>"#,
        );
        assert!(read_images(xml.as_bytes()).is_empty());
        let xml = doc(
            r#"<w:p><w:r><w:drawing><wp:inline><wp:effectExtent/></wp:inline></w:drawing></w:r></w:p>"#,
        );
        assert!(read_images(xml.as_bytes()).is_empty());
    }

    #[test]
    fn nested_extent_does_not_steal_outer_frame() {
        // 巢狀 drawing（group 內容）的 extent 整棵吞沒：外框只認自己的。
        let xml = doc(
            r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="1270000" cy="1270000"/>
               <w:drawing><wp:inline><wp:extent cx="12700" cy="12700"/></wp:inline></w:drawing>
               </wp:inline></w:drawing></w:r></w:p>"#,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert_eq!((imgs[0].w_pt, imgs[0].h_pt), (100.0, 100.0));
    }

    #[test]
    fn fallback_swallowed_double_representation_counts_once() {
        let xml = doc(r#"<w:p><w:r><mc:AlternateContent xmlns:mc="mc">
               <mc:Choice xmlns:wp="wp"><w:drawing><wp:inline><wp:extent cx="1270000" cy="1270000"/></wp:inline></w:drawing></mc:Choice>
               <mc:Fallback><w:pict><v:shape xmlns:v="v" style="width:100pt;height:100pt"><v:imagedata xmlns:r="r" r:id="rId9"/></v:shape></w:pict></mc:Fallback>
               </mc:AlternateContent></w:r></w:p>"#);
        let imgs = read_images(xml.as_bytes());
        assert_eq!(
            imgs.len(),
            1,
            "DrawingML 與 VML 是雙份表示，只取 Choice 一份"
        );
        assert_eq!((imgs[0].w_pt, imgs[0].h_pt), (100.0, 100.0));
    }

    #[test]
    fn nested_fallback_alternatecontent_still_counts_once() {
        // 巢狀 Fallback（t_1a79f1c5 卡片測試項）：Fallback 內再嵌一層
        // AlternateContent（Choice+Fallback 各帶 extent）→ 整棵吞沒，仍 1 份。
        let xml = doc(r#"<w:p><w:r><mc:AlternateContent xmlns:mc="mc">
               <mc:Choice xmlns:wp="wp"><w:drawing><wp:inline><wp:extent cx="1270000" cy="1270000"/></wp:inline></w:drawing></mc:Choice>
               <mc:Fallback><mc:AlternateContent>
                 <mc:Choice><w:drawing><wp:inline><wp:extent cx="2540000" cy="2540000"/></wp:inline></w:drawing></mc:Choice>
                 <mc:Fallback><w:pict><v:shape xmlns:v="v" style="width:200pt;height:200pt"><v:imagedata xmlns:r="r" r:id="rId9"/></v:shape></w:pict></mc:Fallback>
               </mc:AlternateContent></mc:Fallback>
               </mc:AlternateContent></w:r></w:p>"#);
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1, "巢狀 Fallback 內的 drawing 不得計入");
        assert_eq!((imgs[0].w_pt, imgs[0].h_pt), (100.0, 100.0));
    }

    #[test]
    fn align_and_pct_positions_are_captured() {
        // 第三式捕獲（t_1a79f1c5）：OOXMLExport 封面圖族的 positionH/V 只有
        // `<wp:align>` 或 `<wp14:pctPos*Offset>`，兩者都要進投影欄位——
        // engine 端據此解算鎖點，不再盲退 inline。
        let xml = doc(r#"<w:p><w:r><w:drawing><wp:anchor>
               <wp:positionH relativeFrom="page"><wp:align>right</wp:align></wp:positionH>
               <wp:positionV relativeFrom="page"><wp14:pctPosVOffset xmlns:wp14="wp14">25000</wp14:pctPosVOffset></wp:positionV>
               <wp:extent cx="1270000" cy="640080"/></wp:anchor></w:drawing></w:r></w:p>"#);
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert_eq!(imgs[0].pos_x_align.as_deref(), Some("right"));
        assert_eq!(imgs[0].pos_y_pct, Some(25000.0));
        assert_eq!(imgs[0].pos_x_pt, None, "align 不是 posOffset");
        assert_eq!(imgs[0].pos_y_pt, None, "pct 不是 posOffset");
    }

    #[test]
    fn vml_image_via_pict() {
        let xml = doc(
            r##"<w:p><w:r><w:pict><v:shape xmlns:v="v" style="position:absolute;width:242.95pt;height:180.85pt"><v:imagedata xmlns:r="r" r:id="rId3"/></v:shape></w:pict></w:r></w:p>"##,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert_eq!(imgs[0].w_pt.round(), 243.0);
        assert_eq!(imgs[0].h_pt.round(), 181.0);
        assert_eq!(imgs[0].embed_rid.as_deref(), Some("rId3"));
    }

    #[test]
    fn vml_absolute_margin_offsets_become_floating() {
        // R3 席1（fdo76591 實據）：position:absolute＋margin-left/margin-top
        // → margin 原點系鎖點（LO 換算＝頁邊距原點＋偏移），浮動不佔帶。
        let xml = doc(
            r##"<w:p><w:r><w:pict><v:shape xmlns:v="v" style="position:absolute;left:0;margin-left:220.05pt;margin-top:554.45pt;width:99.95pt;height:100.2pt;z-index:251658752"><v:textbox><w:txbxContent><w:p><w:r><w:t>框</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>"##,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert!(imgs[0].floating, "position:absolute 必須採納為浮動");
        assert_eq!(imgs[0].pos_x_rel.as_deref(), Some("margin"));
        assert_eq!(imgs[0].pos_y_rel.as_deref(), Some("margin"));
        assert!((imgs[0].pos_x_pt.unwrap() - 220.05).abs() < 1e-3);
        assert!((imgs[0].pos_y_pt.unwrap() - 554.45).abs() < 1e-3);
        assert!(!imgs[0].page_pinned, "margin 偏移不是頁面釘定");
    }

    #[test]
    fn vml_negative_margin_offset_allowed() {
        // 負偏移＝版心外（fdo76591 背板框 -13.95／-17.8 實據）。
        let xml = doc(
            r##"<w:p><w:r><w:pict><v:shape xmlns:v="v" style="position:absolute;margin-left:-13.95pt;margin-top:-17.8pt;width:567pt;height:733.65pt"><v:textbox><w:txbxContent><w:p/></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>"##,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert!((imgs[0].pos_x_pt.unwrap() + 13.95).abs() < 1e-3);
        assert!((imgs[0].pos_y_pt.unwrap() + 17.8).abs() < 1e-3);
    }

    #[test]
    fn vml_position_absolute_pinned_center_group() {
        // R3 席2（n751077 實據）：mso-position 兩軸 page|margin＋center →
        // 頁面釘定（浮動＋page_pinned），中心對齊由 engine 依 frame 換算。
        let xml = doc(
            r##"<w:p><w:r><w:pict><v:group xmlns:v="v" style="position:absolute;margin-left:0;margin-top:0;width:611.95pt;height:9in;mso-position-horizontal:center;mso-position-horizontal-relative:page;mso-position-vertical:center;mso-position-vertical-relative:margin" coordorigin=",1440" coordsize="12239,12960"><v:rect style="left:1800;top:1440;width:8638;height:916"><v:textbox><w:txbxContent><w:p><w:r><w:t>TEXT1</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></v:group></w:pict></w:r></w:p>"##,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert!(
            (imgs[0].w_pt - 611.95).abs() < 0.1,
            "group w={}",
            imgs[0].w_pt
        );
        assert!(
            (imgs[0].h_pt - 648.0).abs() < 0.1,
            "group h={}",
            imgs[0].h_pt
        );
        assert!(imgs[0].floating, "頁面釘定群組必須浮動");
        assert!(imgs[0].page_pinned, "兩軸 page|margin＋center＝頁面釘定");
        assert_eq!(imgs[0].pos_x_rel.as_deref(), Some("page"));
        assert_eq!(imgs[0].pos_y_rel.as_deref(), Some("margin"));
        assert_eq!(imgs[0].pos_x_align.as_deref(), Some("center"));
        assert_eq!(imgs[0].pos_y_align.as_deref(), Some("center"));
        assert_eq!(imgs[0].pos_x_pt, None, "center 不得被 margin-left:0 蓋掉");
    }

    #[test]
    fn vml_without_absolute_stays_inline() {
        // 零擾動守門：無 position:absolute 的 VML 圖（K4a/K4c 既有語料）
        // floating 恆 false、不帶鎖點——R3 的採納門禁。
        let xml = doc(
            r##"<w:p><w:r><w:pict><v:shape xmlns:v="v" style="width:242.95pt;height:180.85pt;margin-left:10pt"><v:imagedata xmlns:r="r" r:id="rId3"/></v:shape></w:pict></w:r></w:p>"##,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert!(!imgs[0].floating, "無 position:absolute 不得變浮動");
        assert_eq!(imgs[0].pos_x_pt, None);
        assert!(!imgs[0].page_pinned);
    }

    #[test]
    fn vml_bare_shape_without_pict() {
        // Word OLE：w:object 下裸 v:shape（無 w:pict 包）；o:gfxdata 附件圖示。
        let xml = doc(
            r##"<w:p><w:r><w:object><v:shape xmlns:v="v" type="#_x0000_t75" style="width:50pt;height:40pt"><v:imagedata xmlns:r="r" r:id="rId4"/></v:shape></w:object></w:r></w:p>"##,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert_eq!((imgs[0].w_pt, imgs[0].h_pt), (50.0, 40.0));
    }

    #[test]
    fn vml_textbox_without_imagedata_not_emitted() {
        let xml = doc(
            r##"<w:p><w:r><w:pict><v:shape xmlns:v="v" type="#_x0000_t202" style="width:120pt;height:60pt"><v:textbox>w:txbxContent 文字</v:textbox></v:shape></w:pict></w:r></w:p>"##,
        );
        assert!(read_images(xml.as_bytes()).is_empty());
    }

    // ---- K4d（E2）：錯置 w:pict 抑制——LO 繪製開關唯一＝`w:r` 包圍 ----

    /// 紅燈 1（parser 層）：`w:pict` 直接掛 `w:p`（錯置 OOXML，實據
    /// sdt_vml_textbox）→ 整棵不採納（框、框內文字發射路徑同消失）。
    #[test]
    fn misplaced_pict_direct_under_para_not_emitted() {
        let xml = doc(
            r##"<w:p><w:pict><v:rect xmlns:v="v" style="width:200pt;height:60pt" fillcolor="#FFFFCC"><v:textbox><w:txbxContent><w:p><w:r><w:t>VML 文字框內容</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:p>"##,
        );
        let imgs = read_images(xml.as_bytes());
        assert!(
            imgs.is_empty(),
            "錯置 pict（非 w:r 子孫）LO 整棵忽略不畫，我方不得多發：{imgs:?}"
        );
        // 宿主段仍照常佔槽（抑制的是圖形，不是段落計數口徑）。
        assert_eq!(count_paras(xml.as_bytes()), 1);
    }

    /// 正向對照（變體 A）：同一 pict 包進 `w:r` → 照常採納（閂只認 run 包圍）。
    #[test]
    fn same_pict_inside_run_still_emitted() {
        let xml = doc(
            r##"<w:p><w:r><w:pict><v:rect xmlns:v="v" style="width:200pt;height:60pt" fillcolor="#FFFFCC"><v:textbox><w:txbxContent><w:p><w:r><w:t>VML 文字框內容</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:r></w:p>"##,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(
            imgs.len(),
            1,
            "合法包 run 的 pict 照發（K4c-rect 語義不動）"
        );
        assert_eq!((imgs[0].w_pt, imgs[0].h_pt), (200.0, 60.0));
        assert_eq!(imgs[0].fill_argb, Some(0xFF_FF_FF_CC));
        assert_eq!(imgs[0].txbx_texts, vec!["VML 文字框內容".to_string()]);
    }

    /// §1.5 C/G 鐵證的 parser 側對照：合法包 run 後，任何 wrap／absolute 形態
    /// 都照樣採納——閂**不得**順手加 wrap/absolute 條件。
    #[test]
    fn wrapped_or_absolute_pict_in_run_unaffected() {
        let wrap = doc(
            r##"<w:p><w:r><w:pict><v:rect xmlns:v="v" style="width:200pt;height:60pt"><w10:wrap xmlns:w10="w" type="none"/><v:textbox><w:txbxContent><w:p><w:r><w:t>x</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:r></w:p>"##,
        );
        assert_eq!(
            read_images(wrap.as_bytes()).len(),
            1,
            "wrap=none 包 run 照發（LO §1.3 B：dr=5）"
        );
        let abs = doc(
            r##"<w:p><w:r><w:pict><v:rect xmlns:v="v" style="position:absolute;width:200pt;height:60pt"><v:textbox><w:txbxContent><w:p><w:r><w:t>x</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:r></w:p>"##,
        );
        assert_eq!(
            read_images(abs.as_bytes()).len(),
            1,
            "position:absolute 包 run 照發（LO §1.3 C：dr=5）"
        );
    }

    /// 錯置 pict 加 wrap（變體 D）：LO 照樣消失——閂不認 wrap 救錯置。
    #[test]
    fn misplaced_pict_with_wrap_still_suppressed() {
        let xml = doc(
            r##"<w:p><w:pict><v:rect xmlns:v="v" style="width:200pt;height:60pt"><w10:wrap xmlns:w10="w" type="none"/><v:textbox><w:txbxContent><w:p><w:r><w:t>x</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:p>"##,
        );
        assert!(
            read_images(xml.as_bytes()).is_empty(),
            "錯置＋wrap 照樣抑制（§1.3 D 變體：LO dr=0）"
        );
    }

    /// run 外裸 `v:shape`（無 pict 包）同閂：錯置裸 shape 不採納；包 run 的
    /// OLE 常態（`w:object` 下裸 shape）零擾動。
    #[test]
    fn bare_shape_outside_run_suppressed_inside_run_ok() {
        let bad = doc(
            r##"<w:p><v:shape xmlns:v="v" style="width:50pt;height:40pt"><v:imagedata xmlns:r="r" r:id="rId4"/></v:shape></w:p>"##,
        );
        assert!(
            read_images(bad.as_bytes()).is_empty(),
            "run 外裸 shape＝無 LO 繪製依據，不採納"
        );
        let ok = doc(
            r##"<w:p><w:r><w:object><v:shape xmlns:v="v" type="#_x0000_t75" style="width:50pt;height:40pt"><v:imagedata xmlns:r="r" r:id="rId4"/></v:shape></w:object></w:r></w:p>"##,
        );
        assert_eq!(
            read_images(ok.as_bytes()).len(),
            1,
            "OLE 裸 shape 常態（run 內）零擾動"
        );
    }

    /// 巢狀容器：`w:ins` 包 run（修訂圖的 Word 正規產出）——計的是祖先 run，
    /// 非直接父節點，照發。
    #[test]
    fn pict_under_ins_in_run_emitted() {
        let xml = doc(
            r##"<w:p><w:ins w:id="1" w:author="a"><w:r><w:pict><v:rect xmlns:v="v" style="width:30pt;height:20pt"><v:textbox><w:txbxContent><w:p><w:r><w:t>t</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:r></w:ins></w:p>"##,
        );
        assert_eq!(
            read_images(xml.as_bytes()).len(),
            1,
            "w:ins>w:r>pict：run 包圍成立照發"
        );
    }

    /// 同段先有正常 run 後接錯置 pict：pict 仍在 run 外（`</w:r>` 已收）→ 抑制。
    #[test]
    fn pict_after_closed_run_suppressed() {
        let xml = doc(
            r##"<w:p><w:r><w:t>HOSTRUN</w:t></w:r><w:pict><v:rect xmlns:v="v" style="width:200pt;height:60pt"><v:textbox><w:txbxContent><w:p><w:r><w:t>x</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:p>"##,
        );
        assert!(
            read_images(xml.as_bytes()).is_empty(),
            "變體 E 口徑：同段有文字 run 但 pict 不在 run 內＝照樣抑制"
        );
    }

    /// 防過抑回塞鎖（紅燈 2）：buggy 態（`K4D_BUGGY=1` 或欄位直設）把閂退化為
    /// 「run 與非 run 都採納」時，錯置臂必須重新採納（紅燈 1 轉紅）。
    /// 本測試不設行程 env（避免汙染平行執行緒）：直設欄位自證 buggy 路徑。
    #[test]
    fn buggy_lock_adopts_misplaced_pict() {
        use quick_xml::events::Event;
        let xml = r#"<w:body><w:p><w:pict><v:rect xmlns:v="v" style="width:200pt;height:60pt"><v:textbox><w:txbxContent><w:p><w:r><w:t>x</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:p></w:body>"#;
        let mut p = DrawingParser::new();
        p.buggy = true;
        let mut reader = quick_xml::Reader::from_str(xml);
        loop {
            match reader.read_event() {
                Ok(Event::Eof) | Err(_) => break,
                Ok(Event::Start(e)) => {
                    let n = e.local_name().as_ref().to_vec();
                    p.on_start(&n, false, &e);
                }
                Ok(Event::Empty(e)) => {
                    let n = e.local_name().as_ref().to_vec();
                    p.on_empty(&n, &e);
                }
                Ok(Event::Text(t)) => {
                    if let Ok(s) = t.unescape() {
                        p.on_text(&s);
                    }
                }
                Ok(Event::End(e)) => {
                    let n = e.local_name().as_ref().to_vec();
                    p.on_end(&n);
                }
                _ => {}
            }
        }
        assert_eq!(
            p.out.len(),
            1,
            "buggy 態必須重新採納錯置 pict（否則紅燈 2 假綠）"
        );
        // 同款 XML 於修態（run_depth==0）＝空（與紅燈 1 互鎖）。
        let mut fixed = DrawingParser::new();
        let mut reader = quick_xml::Reader::from_str(xml);
        loop {
            match reader.read_event() {
                Ok(Event::Eof) | Err(_) => break,
                Ok(Event::Start(e)) => {
                    let n = e.local_name().as_ref().to_vec();
                    fixed.on_start(&n, false, &e);
                }
                Ok(Event::Empty(e)) => {
                    let n = e.local_name().as_ref().to_vec();
                    fixed.on_empty(&n, &e);
                }
                Ok(Event::Text(t)) => {
                    if let Ok(s) = t.unescape() {
                        fixed.on_text(&s);
                    }
                }
                Ok(Event::End(e)) => {
                    let n = e.local_name().as_ref().to_vec();
                    fixed.on_end(&n);
                }
                _ => {}
            }
        }
        assert!(fixed.out.is_empty(), "修態同款 XML 必須抑制");
    }

    #[test]
    fn in_cell_image_flagged_and_anchored_to_cell_paragraph() {
        // 表格：para0 表前、格內 para1 帶圖、para2 表後。格內圖錨點＝para1。
        // （`<w:p/>` 自閉合不佔槽——四解析器同口徑鐵律，見 t_580c37ac。）
        let xml = doc(
            r#"<w:p></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:drawing><wp:inline xmlns:wp="x"><wp:extent cx="254000" cy="254000"/></wp:inline></w:drawing></w:r></w:p></w:tc></w:tr></w:tbl><w:p></w:p>"#,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs.len(), 1);
        assert!(imgs[0].in_cell);
        assert_eq!(imgs[0].para_index, 1, "格內圖錨點＝格內段落");
        assert_eq!(count_paras(xml.as_bytes()), 3);
    }

    #[test]
    fn para_counting_matches_flat_start_convention() {
        // 巢狀 w:p（txbxContent）**不計**（K4e2：掛宿主段落 txbx_texts）、
        // 自閉合不算、Fallback 內不計。
        let xml = doc(
            r##"<w:p/><w:p><w:r><w:pict><v:shape xmlns:v="v" style="width:9pt;height:9pt"><v:imagedata xmlns:r="r" r:id="rId1"/><v:textbox><w:txbxContent><w:p></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p><w:p><mc:AlternateContent xmlns:mc="mc"><mc:Fallback><w:p></w:p><w:p></w:p></mc:Fallback></mc:AlternateContent></w:p>"##,
        );
        // w:p(自閉合不算) + w:p + txbx w:p(不計) + w:p = Start 計數 2。
        assert_eq!(count_paras(xml.as_bytes()), 2);
    }

    #[test]
    fn anchor_para_index_tracks_flat_count() {
        let xml = doc(
            r#"<w:p></w:p><w:p></w:p><w:p><w:r><w:drawing><wp:inline><wp:extent cx="127000" cy="127000"/></wp:inline></w:drawing></w:r></w:p>"#,
        );
        let imgs = read_images(xml.as_bytes());
        assert_eq!(imgs[0].para_index, 2);
    }
}
