//! sectPr（section properties）頁面幾何解析 —— Phase0-2b。
//!
//! 分頁必須跟著檔案實際的頁面幾何走：`LayoutParams::default()` 只是**檔案沒有
//! 任何 sectPr 資訊時**的後備值，不是硬編碼。本模組由 `word/document.xml`
//! 逐 section 讀取 `pgSz`／`pgMar`／`type`（ECMA-376 §17.2 節），換算成 pt
//! （twip ÷ 20，ECMA-376 §17.18.106 長度單位），回傳**逐段落**的 section 清單。
//!
//! 段落對齊規則（ECMA-376 §17.2.3）：
//! - 段落 `w:pPr` 內的 `w:sectPr` ⇒ 該段落收束它前面的 section（該段屬此 section，
//!   其後的段落屬下一個 section）；
//! - `w:body` 結尾（任何 `w:p` 之外）的 `w:sectPr` ⇒ 收束剩餘段落（檔尾 section）。
//!
//! 實作以「`sectPr` 結束時已計入的段落數」為 section 涵蓋的上界：計數方式與
//! `parse_document_xml` 完全一致（只算 `w:p` 的 Start 事件，含表格內段落，
//! 不含自閉合 `w:p/>`），清單長度恆等於段落數，layout 層逐段直查。
//!
//! 每個 section 帶文件內序號 `id`：同尺寸的相鄰 section（HC031 末兩節皆直式
//! Letter）仍必須在邊界換頁，靠 id 辨識而非尺寸差異。`w:type` 讀 continuous
//! （同頁續排，不強制換頁）；oddPage/evenPage 以 nextPage 近似（頁碼奇偶補白
//! 屬後續卡）。
//!
//! 已知骨架近似：表格 Cell 內段落自帶 sectPr（畸形但存在）時，section 邊界
//! 推到該 Cell 所屬表格段落為止；section 起點的欄數（`w:cols`）未實作，
//! 多欄 section 走單欄。

use serde::{Deserialize, Serialize};

/// Word 省略 `w:pgSz` 時的頁面後備尺寸（twip）：A4 直式。
const DEF_PG_W: i64 = 11906;
const DEF_PG_H: i64 = 16838;
/// Word 省略 `w:pgMar` 各欄位時的預設邊界（twip）：上下 1 吋、左右 1.25 吋。
const DEF_MARGIN_TOP: i64 = 1440;
const DEF_MARGIN_BOTTOM: i64 = 1440;
const DEF_MARGIN_LEFT: i64 = 1800;
const DEF_MARGIN_RIGHT: i64 = 1800;

/// 一個 section 的頁面幾何（pt）。`landscape` 僅供老實回報，
/// 尺寸一律以 `page_w_pt`／`page_h_pt` 為準（Word/LO 皆直接信任 pgSz 的 w/h）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionGeometry {
    pub page_w_pt: f32,
    pub page_h_pt: f32,
    pub margin_top_pt: f32,
    pub margin_bottom_pt: f32,
    pub margin_left_pt: f32,
    pub margin_right_pt: f32,
    /// `w:pgSz/@w:orient="landscape"`。
    pub landscape: bool,
    /// `w:type/@w:val="continuous"`：同頁續排，不強制換頁。
    /// （oddPage/evenPage 以 nextPage 近似＝false。）
    pub continuous: bool,
}

/// 逐段落清單的一項：段落所屬 section 的識別與幾何。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionSpec {
    /// 文件內 section 序號（0 起）。同尺寸相鄰 section 靠 id 辨識邊界。
    pub id: u32,
    pub geo: SectionGeometry,
}

/// 一節的 pgSz/pgMar/type 累積值（twip）。
#[derive(Debug, Clone, Copy)]
struct Accum {
    w: i64,
    h: i64,
    top: i64,
    bottom: i64,
    left: i64,
    right: i64,
    landscape: bool,
    continuous: bool,
}

impl Accum {
    fn new() -> Self {
        Self {
            w: DEF_PG_W,
            h: DEF_PG_H,
            top: DEF_MARGIN_TOP,
            bottom: DEF_MARGIN_BOTTOM,
            left: DEF_MARGIN_LEFT,
            right: DEF_MARGIN_RIGHT,
            landscape: false,
            continuous: false,
        }
    }
    fn into_geom(self) -> SectionGeometry {
        SectionGeometry {
            page_w_pt: self.w as f32 / 20.0,
            page_h_pt: self.h as f32 / 20.0,
            margin_top_pt: self.top as f32 / 20.0,
            margin_bottom_pt: self.bottom as f32 / 20.0,
            // gutter（裝訂線）已於 pgMar 解析時併入 left（LTR 文件左側加寬）。
            margin_left_pt: self.left as f32 / 20.0,
            margin_right_pt: self.right as f32 / 20.0,
            landscape: self.landscape,
            continuous: self.continuous,
        }
    }
}

/// 由 `word/document.xml` 明文解析「逐段落」的 section（識別＋頁面幾何）。
///
/// 回傳清單長度 == `n_paragraphs`。檔案完全沒有 sectPr 時回傳空清單
/// （呼叫端退回 `LayoutParams::default()`——後備語義，非硬編碼斷言）。
pub fn read_sections_per_paragraph(xml: &[u8], n_paragraphs: usize) -> Vec<SectionSpec> {
    if n_paragraphs == 0 {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(xml);
    let mut re = quick_xml::Reader::from_str(&text);

    use quick_xml::events::Event;

    let mut p_count: usize = 0; // 與 parse_document_xml 同口徑：只算 w:p 的 Start（Fallback 內不計）
    let mut block_depth: i32 = 0; // w:p／w:tbl 巢狀深度（0＝body 級）
    let mut fb: usize = 0; // mc:Fallback 巢狀深度：>0 時其內 w:p 不入扁平口徑
    let mut ac_stack: Vec<usize> = Vec::new(); // AC 開標的 fb 快照（錯置標籤不致卡死）
    let mut in_sect = false;
    let mut accum: Option<Accum> = None;
    let mut pending: Option<SectionGeometry> = None; // 段落邊界收束、待所屬 w:p 結束時歸檔
                                                     // (涵蓋至第幾段, section 序號, 幾何)
    let mut boundaries: Vec<(usize, u32, SectionGeometry)> = Vec::new();
    let mut body_sect: Option<(u32, SectionGeometry)> = None;
    let mut next_id: u32 = 0; // section 文件序號：依收束出現順序編號

    // pgSz／pgMar／type 在真實文件一律自閉合（Event::Empty），必須與 Start
    // 同臂處理；Err＝尾部截斷，容忍並照常回報已解析的 section。
    while let Ok(ev) = re.read_event() {
        let ev_is_block_start = matches!(ev, Event::Start(_));
        match ev {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"Fallback" if ev_is_block_start => fb += 1,
                    b"AlternateContent" if ev_is_block_start => ac_stack.push(fb),
                    b"p" if ev_is_block_start => {
                        if fb == 0 {
                            p_count += 1;
                        }
                        block_depth += 1;
                    }
                    b"tbl" if ev_is_block_start => block_depth += 1,
                    b"sectPr" if fb == 0 => {
                        if ev_is_block_start {
                            in_sect = true;
                            accum = Some(Accum::new());
                        } else {
                            // <w:sectPr/> 無子元素：以預設值收束一個 section。
                            let g = Accum::new().into_geom();
                            if block_depth > 0 {
                                pending = Some(g);
                            } else {
                                body_sect = Some((next_id, g));
                                next_id += 1;
                            }
                        }
                    }
                    b"pgSz" if in_sect => {
                        if let Some(a) = accum.as_mut() {
                            a.w = attr_i64(&e, b"w", a.w);
                            a.h = attr_i64(&e, b"h", a.h);
                            a.landscape = attr_str(&e, b"orient").as_deref() == Some("landscape");
                        }
                    }
                    b"pgMar" if in_sect => {
                        if let Some(a) = accum.as_mut() {
                            let top = attr_i64(&e, b"top", a.top);
                            let bottom = attr_i64(&e, b"bottom", a.bottom);
                            let left = attr_i64(&e, b"left", a.left);
                            let right = attr_i64(&e, b"right", a.right);
                            let gutter = attr_i64(&e, b"gutter", 0);
                            a.top = top;
                            a.bottom = bottom;
                            a.left = left + gutter;
                            a.right = right;
                        }
                    }
                    b"type" if in_sect => {
                        if let Some(a) = accum.as_mut() {
                            a.continuous = attr_str(&e, b"val").as_deref() == Some("continuous");
                        }
                    }
                    _ => {}
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_vec();
                match name.as_slice() {
                    b"AlternateContent" => {
                        fb = ac_stack.pop().unwrap_or(0);
                    }
                    b"Fallback" => {
                        fb = fb.saturating_sub(1);
                    }
                    b"sectPr" => {
                        if in_sect {
                            in_sect = false;
                            if let Some(a) = accum.take() {
                                let g = a.into_geom();
                                if block_depth > 0 {
                                    pending = Some(g);
                                } else {
                                    body_sect = Some((next_id, g));
                                    next_id += 1;
                                }
                            }
                        }
                    }
                    b"p" | b"tbl" => {
                        block_depth -= 1;
                        if block_depth == 0 {
                            if let Some(g) = pending.take() {
                                // 此 body 級 block 收束一個 section：涵蓋到目前計入的所有段落。
                                boundaries.push((p_count, next_id, g));
                                next_id += 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if boundaries.is_empty() && body_sect.is_none() {
        return Vec::new(); // 無任何 section 資訊 → 呼叫端後備
    }

    // 展開成逐段落清單：邊界 section 各涵蓋到其邊界段為止，檔尾 section 吃其餘。
    let mut out = Vec::with_capacity(n_paragraphs);
    for (through, id, g) in &boundaries {
        while out.len() < (*through).min(n_paragraphs) {
            out.push(SectionSpec { id: *id, geo: *g });
        }
    }
    let tail = body_sect.or_else(|| boundaries.last().map(|(_, id, g)| (*id, *g)));
    if let Some((id, g)) = tail {
        while out.len() < n_paragraphs {
            out.push(SectionSpec { id, geo: g });
        }
    }
    out
}

fn attr_i64(e: &quick_xml::events::BytesStart, key: &[u8], def: i64) -> i64 {
    attr_str(e, key)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .map(|v| v as i64)
        .unwrap_or(def)
}

fn attr_str(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key {
            a.unescape_value().ok().map(|v| v.to_string())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xml(s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    /// 典型 Word 檔：單 section（body 級 sectPr，Letter 直式 12240×15840 twip）。
    #[test]
    fn letter_section_converts_twips_to_pt() {
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:p><w:r><w:t>B</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="12240" w:h="15840"/>
                 <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>
               </w:sectPr>
             </w:body></w:document>"#);
        let g = read_sections_per_paragraph(&x, 2);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0], g[1]);
        assert_eq!(g[0].id, 0);
        assert_eq!(g[0].geo.page_w_pt, 612.0, "12240 twip = 612 pt = Letter 寬");
        assert_eq!(g[0].geo.page_h_pt, 792.0, "15840 twip = 792 pt = Letter 高");
        assert_eq!(g[0].geo.margin_top_pt, 72.0);
        assert_eq!(g[0].geo.margin_left_pt, 72.0);
        assert!(!g[0].geo.landscape);
        assert!(!g[0].geo.continuous);
    }

    /// 三 section：直式→橫向→直式（HC031 形態：pPr 內 sectPr 收束 + body 級收尾；
    /// 首尾兩節同尺寸，靠 id 辨識邊界）。
    #[test]
    fn multi_section_landscape_boundary() {
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
               <w:p><w:pPr><w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr></w:pPr><w:r><w:t>第二段</w:t></w:r></w:p>
               <w:p><w:r><w:t>第三段</w:t></w:r></w:p>
               <w:p><w:pPr><w:sectPr><w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/></w:sectPr></w:pPr><w:r><w:t>第四段</w:t></w:r></w:p>
               <w:p><w:r><w:t>第五段</w:t></w:r></w:p>
               <w:p><w:r><w:t>第六段</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>
             </w:body></w:document>"#);
        let g = read_sections_per_paragraph(&x, 6);
        assert_eq!(g.len(), 6);
        // 段 1-2：Letter 直式；段 3-4：Letter 橫向；段 5-6：Letter 直式（body 級收尾）。
        assert_eq!(g[0].geo.page_w_pt, 612.0);
        assert_eq!(g[1].geo.page_w_pt, 612.0);
        assert_eq!(g[2].geo.page_w_pt, 792.0, "橫向段寬 792");
        assert_eq!(g[2].geo.page_h_pt, 612.0);
        assert!(g[2].geo.landscape);
        assert_eq!(g[3].geo.page_w_pt, 792.0, "橫向 section 涵蓋其邊界段");
        assert_eq!(g[4].geo.page_w_pt, 612.0);
        assert_eq!(g[5].geo.page_w_pt, 612.0);
        assert!(!g[5].geo.landscape);
        // section 序號：0,0,1,1,2,2——首尾同尺寸的邊界仍以 id 辨識。
        assert_eq!(
            g.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![0, 0, 1, 1, 2, 2]
        );
    }

    #[test]
    fn continuous_type_flagged() {
        // 邊界段（pPr 帶 sectPr 者）屬於該 sectPr 描述的 section；檔尾段落屬
        // body 級 section。`w:type=continuous` 標在 body 級 → 末節同頁續排。
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:p><w:pPr><w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:pPr></w:p>
               <w:p><w:r><w:t>C</w:t></w:r></w:p>
               <w:sectPr><w:type w:val="continuous"/><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
             </w:body></w:document>"#);
        let g = read_sections_per_paragraph(&x, 3);
        assert_eq!(g.iter().map(|s| s.id).collect::<Vec<_>>(), vec![0, 0, 1]);
        assert!(
            !g[0].geo.continuous,
            "首節（inline sectPr）未標 type → nextPage"
        );
        assert!(
            g[2].geo.continuous,
            "body 級 type=continuous → 末節同頁續排"
        );
    }

    #[test]
    fn no_sectpr_returns_empty_for_fallback() {
        let x = xml(
            r#"<w:document xmlns:w="x"><w:body><w:p><w:r><w:t>A</w:t></w:r></w:p></w:body></w:document>"#,
        );
        assert!(read_sections_per_paragraph(&x, 1).is_empty());
    }

    #[test]
    fn gutter_adds_to_left_margin() {
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="11906" w:h="16838"/>
                 <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:gutter="720"/>
               </w:sectPr>
             </w:body></w:document>"#);
        let g = read_sections_per_paragraph(&x, 1);
        assert_eq!(
            g[0].geo.margin_left_pt, 108.0,
            "left 1440 + gutter 720 = 2160 twip = 108 pt"
        );
        assert_eq!(g[0].geo.margin_right_pt, 72.0);
    }

    #[test]
    fn table_paragraphs_counted_like_parser() {
        // 表格 Cell 內段落也進段落清單（與 parse_document_xml 同口徑）；
        // body 級 sectPr 涵蓋全部段落。
        let x = xml(r#"<w:document xmlns:w="x"><w:body>
               <w:p><w:r><w:t>A</w:t></w:r></w:p>
               <w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
               <w:p><w:r><w:t>B</w:t></w:r></w:p>
               <w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
             </w:body></w:document>"#);
        let g = read_sections_per_paragraph(&x, 3);
        assert_eq!(g.len(), 3);
        assert_eq!(g[1].geo.page_w_pt, 595.3, "11906 twip = A4 寬 595.3 pt");
    }
}
