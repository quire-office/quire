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

// ---------------------------------------------------------------------------
// 事件流解析器（quick-xml 單趟；深度計數狀態機）
// ---------------------------------------------------------------------------

/// local name 是否為 VML 圖形元素（其 style 攜帶框尺寸）。
fn is_vml_shape(name: &[u8]) -> bool {
    matches!(name, b"shape" | b"rect" | b"oval" | b"line" | b"roundrect")
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
    /// 候選出自 `wp:anchor`（浮動）。
    floating: bool,
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
            in_cell,
            vml_image: false,
            vml_gfxdata: false,
            has_txbx: false,
            txbx: Vec::new(),
            txbx_cur: String::new(),
            txbx_in_t: false,
            txbx_para_depth: 0,
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
    /// `mc:AlternateContent` 巢狀快照（錯置標籤不致 fb 卡死，同主解析器）。
    ac_stack: Vec<usize>,
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
            graphic: None,
            vml: None,
            skip_tag: None,
            fb: 0,
            txbx_depth: 0,
            ac_stack: Vec::new(),
        }
    }

    fn cur_para(&self) -> usize {
        self.p_count.saturating_sub(1)
    }

    /// 完結落表：尺寸齊備才採納（extent 缺／非法＝老實不採納，不虛構幾何）。
    fn push_pending(&mut self, p: &Pending) {
        let (Some(w), Some(h)) = (p.w, p.h) else {
            return;
        };
        self.out.push(InlineImage {
            para_index: p.para,
            w_pt: w,
            h_pt: h,
            floating: p.floating,
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
                n if is_vml_shape(n) && !empty => {
                    self.shape_depth += 1;
                    if let Some(v) = self.vml.as_mut() {
                        if v.w.is_none() && v.h.is_none() {
                            let (w, h) = vml_style_wh(attr_local(e, b"style").as_deref());
                            v.w = w;
                            v.h = h;
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
                let mut p = Pending::new(self.cur_para(), self.tc_depth > 0);
                p.overlap = overlap;
                p.floating = floating;
                self.graphic = Some(Box::new(p));
            }
            b"pict" if !empty => {
                self.pict_depth += 1;
                if self.vml.is_none() {
                    self.vml = Some(Box::new(Pending::new(self.cur_para(), self.tc_depth > 0)));
                }
            }
            n if is_vml_shape(n) && !empty => {
                // 裸 VML shape（Word OLE 常態：w:object 下 v:shape 無 pict 包）。
                self.shape_depth += 1;
                if self.vml.is_none() {
                    let (w, h) = vml_style_wh(attr_local(e, b"style").as_deref());
                    let mut p = Pending::new(self.cur_para(), self.tc_depth > 0);
                    p.w = w;
                    p.h = h;
                    self.vml = Some(Box::new(p));
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
                    }
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
