//! F1：圖表 part（`word/charts/chart*.xml`）的只讀解析（ECMA-376 Part 1 §21.2）。
//!
//! 背景（卡 t_f3acd271，PM 實查）：`grep -rn 'c:chart|chartSpace|ChartPart'
//! core/ooxml/src/*.rs` 零命中——圖表 part 從未被解析，遑論發射，
//! `corpus/ooxmlexport/chart-size.docx` 的圖軸分因此是 0.0（LO 渲染出
//! `0..4` 數值刻度與 `1/2/3` 類別標籤，我方一字未發）。本模組補「解析」這
//! 一刀：只讀取、不改寫、不存檔（圖表屬「引擎未要求變更的 part」，存檔時
//! 由 `ZipBundle` 原位元組搬運，無損鐵律不破）。
//!
//! 解剖依據見 `docs/f1-chart-anatomy.md`（實拆 chart-size／strict＋14 檔普查＋
//! LO 10 檔真值擬合）。四條本模組的硬口徑：
//!
//! 1. **cache 值優先**（卡面第二步驟）：`c:numCache`／`c:strCache` 是常態
//!    （語料 41 系列中 39 帶 cache），`c:numRef`／`c:strRef` 的 `c:f` 引用式
//!    一律**不重算**（外部工作表屬下一切片），只留原始字串供診斷。
//! 2. **空標題不虛構**：`c:title` 存在但無 `c:tx//a:t` ＝無標題文字。LO 對此
//!    自繪**本地化佔位字串**（繁中環境繪出「圖表標題」四字，而 chart-size
//!    全封包零 CJK，實證見解剖報告 §1 要點 B），那不是文件內容，我方不發
//!    （K7「表沒有實測值的族絕不猜數」同義）。
//! 3. **命名空間兩式皆認**：transitional（13/14 檔）與 strict
//!    （`corpus/ooxmlexport/strict.docx`）的 chart ns 不同，且 strict 把屬性值
//!    改寫成百分比字串（`lumMod val="65%"` vs `val="65000"`）——兩者都要吃得
//!    下，否則 strict 零命中。本模組一律以 `local_name()` 比對，前綴無關。
//! 4. **mc:Choice／mc:Fallback 雙份去重**：同一 `w:drawing`（同一 r:id、同一宿主
//!    段落）在兩支各出現一次（chart-size 實抓偏移 1332／7389），不去重就會
//!    雙份發射，反而新增假多字。
//!
//! 容錯口徑同 [`crate::ThemeColors`]／[`crate::Notes`]：缺 part／壞 XML＝停在已收
//! 部分（老實回報，不 panic、不虛構）。

use quick_xml::events::BytesStart;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::theme::ThemeColors;
use crate::zip_struct::ZipBundle;

/// transitional chart 命名空間（`.../drawingml/2006/chart`）。
const CHART_NS_TRANSITIONAL: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
/// strict chart 命名空間（`http://purl.oclc.org/ooxml/drawingml/chart`）。
const CHART_NS_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/chart";

/// 本模組關注的 local-name 白名單（路徑追蹤用；省比較成本、避免路徑無限增長）。
const WATCHED: &[&[u8]] = &[
    b"chartSpace",
    b"chart",
    b"plotArea",
    b"title",
    b"tx",
    b"rich",
    b"ser",
    b"dLbls",
    b"cat",
    b"val",
    b"catAx",
    b"valAx",
    b"dateAx",
    b"txPr",
    b"majorGridlines",
    b"spPr",
    b"solidFill",
    b"numCache",
    b"strCache",
    b"numRef",
    b"strRef",
    b"grouping",
    b"barChart",
    b"lineChart",
    b"areaChart",
    b"pieChart",
    b"scatterChart",
    b"scaling",
];

/// 一個 `c:ser`（系列）的讀取投影。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartSeries {
    /// `c:tx` 的系列名（`c:strRef/c:strCache` 的快取值或 `c:rich//a:t`；無＝None）。
    pub name: Option<String>,
    /// `c:cat` 的類別標籤：`c:strCache` 字串原值，或 `c:numCache` 的數值原字串
    /// （**不做 Excel 日期序列轉換**，解剖報告 §5-7）。
    pub cats: Vec<String>,
    /// `c:cat` 不存在時的點數——LO 改以點序號自動編號 `1..n`（實據 chart-size／
    /// strict：檔內無 `c:cat`，LO 仍繪出 `1`,`2`,`3`）。
    pub auto_cats: usize,
    /// `c:val/c:numCache` 的數值（解析失敗的點跳過，**不補 0**）。
    pub vals: Vec<f64>,
    /// `c:val/c:numRef/c:f` 原始字串（診斷用；**永不重算**，見模組頭口徑 1）。
    pub val_formula: Option<String>,
    /// `c:spPr/a:solidFill` 實色（0xAARRGGBB；無宣告＝None＝呼叫端退主題 accent1）。
    pub fill_argb: Option<u32>,
}

/// `c:valAx`／`c:catAx` 的軸宣告讀取投影。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartAxis {
    /// 元素是否存在（本切片只認第一組軸；多組軸的複合圖屬後續卡）。
    pub present: bool,
    /// `c:delete/@val`（true＝軸隱藏，刻度不發）。
    pub deleted: bool,
    /// `c:majorGridlines` 是否存在——決定格線要不要畫（卡面第二步驟 3）。
    pub major_gridlines: bool,
    /// `c:txPr//a:defRPr/@sz` ÷ 100（刻度字級；缺宣告＝None＝呼叫端退 9pt）。
    pub tick_size_pt: Option<f32>,
    /// `c:txPr//a:defRPr` 的實色（LO 刻度實測 0x595959＝tx1×lumMod65%＋lumOff35%）。
    pub tick_color: Option<u32>,
    /// `c:numFmt/@formatCode` 原字串（日期格式**不轉字串**，解剖報告 §5-7）。
    pub num_fmt: Option<String>,
    /// `c:scaling/c:orientation/@val` 原字串（`maxMin`＝反轉軸）。
    pub orientation: Option<String>,
}

/// 一份 chart part 的只讀投影（純讀取派生；存檔路徑完全不經此型別）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartDoc {
    /// 圖型（文件序去重後的 local-name：`barChart`／`lineChart`／`pieChart`…）。
    pub kinds: Vec<String>,
    /// `c:title/c:tx` 的文字（**無 `c:tx`＝None**；LO 本地化佔位不在此欄）。
    pub title: Option<String>,
    /// `c:title` 元素是否存在（即使無文字——LO 據此保留標題帶高度）。
    pub title_declared: bool,
    /// `c:autoTitleDeleted/@val`（true＝標題帶不保留）。
    pub auto_title_deleted: bool,
    /// 標題字級（`c:title/c:txPr//a:defRPr/@sz` ÷ 100）。
    pub title_size_pt: Option<f32>,
    /// 系列（文件序）。
    pub series: Vec<ChartSeries>,
    /// `c:grouping/@val` 是否堆疊式（`stacked`／`percentStacked`）——決定數值軸上界
    /// 採「逐類別加總」（LO 實據：SolarOverview／Chart-Cached-Data-04／05／tdf82824）。
    pub stacked: bool,
    /// `c:barDir/@val` 是否為 `col`（直柱）。
    pub bar_dir_col: bool,
    /// 是否為本切片可發幾何的平面圖型（bar／line／area）；pie／scatter 只發標題。
    pub plottable: bool,
    /// 數值軸。
    pub val_ax: ChartAxis,
    /// 類別軸。
    pub cat_ax: ChartAxis,
    /// 柱形填色預設（主題 accent1，0xAARRGGBB；無 theme part＝None）。
    pub accent1: Option<u32>,
    /// 格線色（`c:majorGridlines//a:ln/a:solidFill`；無＝None）。
    pub grid_color: Option<u32>,
    /// 軸線色（軸 `c:spPr/a:ln/a:solidFill`；無＝None）。
    pub axis_color: Option<u32>,
}

/// 圖表掛載點（`word/document.xml` 內 `a:graphicData/@uri`＝chart 之落點）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartMount {
    /// 宿主段落索引——**扁平 `w:p` Start 口徑**，與 `InlineImage::para_index` 同議
    /// （巢狀 `w:txbxContent` 段落不改寫外層槽位；corpus 柵欄鎖死的那條口徑）。
    pub para_index: usize,
    /// 在宿主 `w:txbxContent` 內的段落序（0 基；框外掛載＝0）。LO 逐段堆疊定位用。
    pub txbx_ordinal: usize,
    /// `wp:extent` 寬（pt，EMU÷12700）。
    pub w_pt: f32,
    /// `wp:extent` 高（pt）。
    pub h_pt: f32,
    /// 換算後的 chart part 路徑（`word/charts/chart1.xml`）；查不到＝該掛載丟棄。
    pub part: String,
    /// `wp:anchor`（浮動）＝true；`wp:inline`＝false。
    pub floating: bool,
    /// 掛載在 `w:txbxContent` 內（chart-size 實據：圖表掛在文字框第 2 段）。
    pub in_txbx: bool,
    /// `wp:positionV/@posOffset`（pt）與 `@relativeFrom`。
    pub pos_y_pt: Option<f32>,
    pub pos_y_rel: Option<String>,
    /// `wp:positionH/@posOffset`（pt）與 `@relativeFrom`。
    pub pos_x_pt: Option<f32>,
    pub pos_x_rel: Option<String>,
    /// 宿主 `wps:bodyPr/@lIns`／`@tIns`（pt；框內圖表定位內縮；缺宣告＝None）。
    pub host_l_ins_pt: Option<f32>,
    pub host_t_ins_pt: Option<f32>,
}

impl ChartDoc {
    /// 本切片可發幾何的圖型（bar／line／area 的 2D 平面臂；3D／radar／stock 不發）。
    fn plottable_kind(k: &str) -> bool {
        matches!(k, "barChart" | "lineChart" | "areaChart")
    }

    /// 全部系列的類別標籤（取第一個非空者；LO 同議——多系列共用類別軸標籤）。
    pub fn category_labels(&self) -> &[String] {
        self.series
            .iter()
            .find(|s| !s.cats.is_empty())
            .map(|s| s.cats.as_slice())
            .unwrap_or(&[])
    }

    /// 生效類別標籤：有 `c:cat` 用 cache 字串，無則以點序號自動編號（實據 chart-size）。
    pub fn effective_cats(&self) -> Vec<String> {
        let cached = self.category_labels();
        if !cached.is_empty() {
            return cached.to_vec();
        }
        let n = self.series.iter().map(|s| s.auto_cats).max().unwrap_or(0);
        (1..=n).map(|i| i.to_string()).collect()
    }

    /// 數值軸的資料上界（LO 律 L3：clustered 取單點最大、堆疊取逐類別加總最大）。
    pub fn data_max(&self) -> Option<f64> {
        if self.stacked {
            let n = self
                .series
                .iter()
                .map(|s| s.vals.len())
                .max()
                .filter(|n| *n > 0)?;
            let mut best: Option<f64> = None;
            for i in 0..n {
                let sum: f64 = self
                    .series
                    .iter()
                    .filter_map(|s| s.vals.get(i).copied())
                    .sum();
                best = Some(best.map_or(sum, |b| b.max(sum)));
            }
            best
        } else {
            self.series
                .iter()
                .flat_map(|s| s.vals.iter().copied())
                .fold(None, |acc: Option<f64>, v| {
                    Some(acc.map_or(v, |a| a.max(v)))
                })
        }
    }

    /// 數值軸的資料下界（LO 律 L2：資料 min ≥ 0 時下界取 0，10/10 檔成立）。
    pub fn data_min(&self) -> Option<f64> {
        self.series
            .iter()
            .flat_map(|s| s.vals.iter().copied())
            .fold(None, |acc: Option<f64>, v| {
                Some(acc.map_or(v, |a| a.min(v)))
            })
            .map(|m| if m >= 0.0 { 0.0 } else { m })
    }
}

/// 由整份 docx 位元組取「掛載點＋已解析內容」成對清單（文档序、去重）。
///
/// 無圖表檔＝回空清單（發射層零擾動快路徑；round-trip 不受影響——本函數
/// **只讀 parts**，絕不寫回）。
pub fn read_charts_from_docx(bytes: &[u8], theme: &ThemeColors) -> Vec<(ChartMount, ChartDoc)> {
    let Ok(bundle) = ZipBundle::open(bytes) else {
        return Vec::new();
    };
    let Ok(parts) = bundle.decode_parts() else {
        return Vec::new();
    };
    read_charts_from_parts(&parts, theme)
}

/// [`read_charts_from_docx`] 的 parts 版（供已有 `OoxmlDoc::parts` 的呼叫端重用）。
pub fn read_charts_from_parts(
    parts: &BTreeMap<String, Vec<u8>>,
    theme: &ThemeColors,
) -> Vec<(ChartMount, ChartDoc)> {
    let Some(doc_xml) = parts.get("word/document.xml") else {
        return Vec::new();
    };
    let mut mounts = scan_chart_mounts(doc_xml);
    if mounts.is_empty() {
        return Vec::new();
    }
    // `c:chart/@r:id` → part 路徑（`word/_rels/document.xml.rels`）。
    let rels = crate::rels::read_rels(
        parts
            .get("word/_rels/document.xml.rels")
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
    );
    for m in mounts.iter_mut() {
        let rid = std::mem::take(&mut m.part);
        m.part = crate::rels::resolve_relationship(&rels, &rid)
            .filter(|p| parts.contains_key(p))
            .unwrap_or_default();
    }
    mounts.retain(|m| !m.part.is_empty());
    // 雙份去重（模組頭口徑 4）：同 part＋同宿主段落＋同尺寸＝同一圖表。
    mounts.sort_by(|a, b| {
        (
            a.para_index,
            a.part.as_str(),
            a.txbx_ordinal,
            a.w_pt.to_bits(),
            a.h_pt.to_bits(),
        )
            .cmp(&(
                b.para_index,
                b.part.as_str(),
                b.txbx_ordinal,
                b.w_pt.to_bits(),
                b.h_pt.to_bits(),
            ))
    });
    mounts.dedup_by(|a, b| {
        a.para_index == b.para_index
            && a.part == b.part
            && a.txbx_ordinal == b.txbx_ordinal
            && a.w_pt.to_bits() == b.w_pt.to_bits()
            && a.h_pt.to_bits() == b.h_pt.to_bits()
    });
    let mut cache: BTreeMap<String, ChartDoc> = BTreeMap::new();
    let mut out = Vec::with_capacity(mounts.len());
    for m in mounts {
        let doc = match cache.get(&m.part) {
            Some(d) => d.clone(),
            None => {
                let raw = parts.get(&m.part).map(|v| v.as_slice()).unwrap_or(&[]);
                let d = parse_chart(raw, theme);
                cache.insert(m.part.clone(), d.clone());
                d
            }
        };
        out.push((m, doc));
    }
    out
}

/// 只掃掛載點（不做 rels 換算：`part` 欄暫存 `c:chart/@r:id` 原字串）。
///
/// 段落計數口徑必須與 `crate::drawing` 一致：`w:p` Start（非自閉合）且
/// `txbx_depth == 0` 才遞增；掛載的 `para_index` ＝「最後一個先於本圖的
/// `w:p`」之 0 基索引（正文前出現圖＝0，容錯口徑同 `InlineImage`）。
pub fn scan_chart_mounts(xml: &[u8]) -> Vec<ChartMount> {
    let text = String::from_utf8_lossy(xml).into_owned();
    let mut reader = Reader::from_str(&text);
    let mut p_count: usize = 0;
    let mut txbx_depth: usize = 0;
    // 每個進行中 `w:txbxContent` 的框內段落計數（巢狀框各一層）。
    let mut txbx_para: Vec<usize> = Vec::new();
    let mut drawing_depth: usize = 0;
    let mut cur: Option<PendingMount> = None;
    // 宿主 `wps:wsp` 堆疊：`wps:bodyPr`（lIns／tIns）在 DOM 順序上**排在
    // `wps:txbx` 之後**（chart-size 實抓：`<wps:txbx>…</wps:txbx>
    // <wps:bodyPr lIns="91440" tIns="45720" …/>`），掛載掃描當下還看不到內縮
    // ——改採「事後回填」：每個 wsp 記自己收到的 bodyPr 宣告與本框內產出的
    // 掛載索引，wsp 結束時一次回填（巢狀框各層自管，互不串框）。
    let mut wsp_stack: Vec<WspFrame> = Vec::new();
    let mut out: Vec<ChartMount> = Vec::new();
    loop {
        match reader.read_event() {
            Err(_) | Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = e.local_name();
                let n = name.as_ref();
                match n {
                    b"p" => {
                        if txbx_depth == 0 {
                            p_count += 1;
                        } else if let Some(c) = txbx_para.last_mut() {
                            *c += 1;
                        }
                    }
                    b"txbxContent" => {
                        txbx_depth += 1;
                        txbx_para.push(0);
                    }
                    b"drawing" => drawing_depth += 1,
                    b"inline" | b"anchor" if drawing_depth > 0 || txbx_depth > 0 => {
                        let mut pend = PendingMount::default();
                        pend.floating = n == b"anchor";
                        pend.para = p_count.saturating_sub(1);
                        pend.txbx_ordinal = txbx_para
                            .last()
                            .map(|c| c.saturating_sub(1))
                            .unwrap_or(0);
                        pend.in_txbx = txbx_depth > 0;
                        cur = Some(pend);
                    }
                    b"extent" if cur.is_some() => {
                        if let Some(p) = cur.as_mut() {
                            p.w_pt = emu_attr(&e, b"cx");
                            p.h_pt = emu_attr(&e, b"cy");
                        }
                    }
                    b"positionH" if cur.is_some() => {
                        if let Some(p) = cur.as_mut() {
                            p.pos_x_rel = attr_str(&e, b"relativeFrom");
                            p.last_pos_axis = Some(true);
                        }
                    }
                    b"positionV" if cur.is_some() => {
                        if let Some(p) = cur.as_mut() {
                            p.pos_y_rel = attr_str(&e, b"relativeFrom");
                            p.last_pos_axis = Some(false);
                        }
                    }
                    b"posOffset" if cur.is_some() => {
                        // `wp:positionH/V` 的子元素：文字節點才是偏移值（on Text 收）。
                        if let Some(p) = cur.as_mut() {
                            p.pending_pos = p.last_pos_axis.take();
                        }
                    }
                    b"graphicData" if cur.is_some() => {
                        if let Some(uri) = attr_str(&e, b"uri") {
                            if let Some(p) = cur.as_mut() {
                                p.is_chart = is_chart_uri(&uri);
                            }
                        }
                    }
                    b"chart" => {
                        // `c:chart/@r:id`（local-name＝id／embed，前綴无关）。
                        if let Some(p) = cur.as_mut() {
                            if p.is_chart && p.rid.is_none() {
                                p.rid = attr_str(&e, b"id").or_else(|| attr_str(&e, b"embed"));
                            }
                        }
                    }
                    b"wsp" => wsp_stack.push(WspFrame::default()),
                    b"bodyPr" if txbx_depth > 0 => {
                        if let Some(f) = wsp_stack.last_mut() {
                            f.l_ins = emu_attr_opt(&e, b"lIns");
                            f.t_ins = emu_attr_opt(&e, b"tIns");
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.local_name();
                let n = name.as_ref();
                match n {
                    b"extent" if cur.is_some() => {
                        if let Some(p) = cur.as_mut() {
                            p.w_pt = emu_attr(&e, b"cx");
                            p.h_pt = emu_attr(&e, b"cy");
                        }
                    }
                    b"chart" => {
                        if let Some(p) = cur.as_mut() {
                            if p.is_chart && p.rid.is_none() {
                                p.rid = attr_str(&e, b"id").or_else(|| attr_str(&e, b"embed"));
                            }
                        }
                    }
                    b"bodyPr" if txbx_depth > 0 => {
                        // chart-size 實據：`<wps:bodyPr … lIns="91440" tIns="45720" …/>`
                        // 是**自閉合**元素（Empty 事件），Start 臂收不到。
                        if let Some(f) = wsp_stack.last_mut() {
                            f.l_ins = emu_attr_opt(&e, b"lIns");
                            f.t_ins = emu_attr_opt(&e, b"tIns");
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(p) = cur.as_mut() {
                    if let Some(was_h) = p.pending_pos.take() {
                        if let Ok(s) = crate::qx::unescape_text(&t) {
                            if let Ok(v) = s.trim().parse::<f64>() {
                                let pt = (v / 12700.0) as f32;
                                if pt.is_finite() && pt > 0.0 {
                                    if was_h {
                                        p.pos_x_pt = Some(pt);
                                    } else {
                                        p.pos_y_pt = Some(pt);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name();
                let n = name.as_ref();
                match n {
                    b"txbxContent" => {
                        txbx_depth = txbx_depth.saturating_sub(1);
                        txbx_para.pop();
                    }
                    b"drawing" => drawing_depth = drawing_depth.saturating_sub(1),
                    b"inline" | b"anchor" => {
                        if let Some(pend) = cur.take() {
                            // 採納條件（老實不虛構）：有 r:id ＋ 有正面積。
                            if let Some(rid) = pend.rid {
                                if pend.w_pt > 0.0 && pend.h_pt > 0.0 {
                                    let idx = out.len();
                                    out.push(ChartMount {
                                        para_index: pend.para,
                                        txbx_ordinal: pend.txbx_ordinal,
                                        w_pt: pend.w_pt,
                                        h_pt: pend.h_pt,
                                        part: rid,
                                        floating: pend.floating,
                                        in_txbx: pend.in_txbx,
                                        pos_y_pt: pend.pos_y_pt,
                                        pos_y_rel: pend.pos_y_rel,
                                        pos_x_pt: pend.pos_x_pt,
                                        pos_x_rel: pend.pos_x_rel,
                                        host_l_ins_pt: None,
                                        host_t_ins_pt: None,
                                    });
                                    // 記在最內層**框外** wsp（圖表宿主是框外 wsp；
                                    // 巢狀框的掛載只回填給最外層那一個 frame）。
                                    if let Some(f) = wsp_stack.last_mut() {
                                        f.mounts.push(idx);
                                    }
                                }
                            }
                        }
                    }
                    b"wsp" => {
                        // wsp 結束＝用本框的 bodyPr 宣告回填本框產出的掛載。
                        if let Some(f) = wsp_stack.pop() {
                            for i in f.mounts {
                                if let Some(m) = out.get_mut(i) {
                                    m.host_l_ins_pt = f.l_ins;
                                    m.host_t_ins_pt = f.t_ins;
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
    out
}

/// 一個進行中 `wps:wsp` 的回填帳（內部型別）。
#[derive(Default)]
struct WspFrame {
    /// 本框 `wps:bodyPr/@lIns`（pt）。
    l_ins: Option<f32>,
    /// 本框 `wps:bodyPr/@tIns`（pt）。
    t_ins: Option<f32>,
    /// 本框產出的掛載索引（`out` 內）。
    mounts: Vec<usize>,
}

/// 掛載掃描的进行中候選（內部型別，不出口）。
#[derive(Default)]
struct PendingMount {
    para: usize,
    txbx_ordinal: usize,
    w_pt: f32,
    h_pt: f32,
    floating: bool,
    in_txbx: bool,
    is_chart: bool,
    rid: Option<String>,
    pos_x_pt: Option<f32>,
    pos_x_rel: Option<String>,
    pos_y_pt: Option<f32>,
    pos_y_rel: Option<String>,
    /// `posOffset` 文字節點歸屬（`Some(true)`＝H 軸、`Some(false)`＝V 軸）。
    pending_pos: Option<bool>,
    last_pos_axis: Option<bool>,
}

/// `a:graphicData/@uri` 是否指向 chart（兩式命名空間）。
fn is_chart_uri(uri: &str) -> bool {
    uri == CHART_NS_TRANSITIONAL || uri == CHART_NS_STRICT
}

/// EMU 屬性 → pt（`val="12700"`＝1pt）。非法／非正值＝0.0（呼叫端據此丟棄）。
fn emu_attr(e: &BytesStart, key: &[u8]) -> f32 {
    attr_str(e, key)
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|v| (v / 12700.0) as f32)
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(0.0)
}

/// EMU 屬性 → pt 的 Option 版：供 `wps:bodyPr/@lIns` 這類「宣告 0 有意義
/// （零內縮）」的屬性使用——缺屬性＝None（呼叫端退既有硬近），
/// 宣告 0＝Some(0.0)（老實吃宣告值，不退硬近）。
fn emu_attr_opt(e: &BytesStart, key: &[u8]) -> Option<f32> {
    attr_str(e, key)
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|v| (v / 12700.0) as f32)
        .filter(|v| v.is_finite() && *v >= 0.0)
}

/// 取屬性字串（local-name 比對，前綴无关——strict／transitional 共用）。
fn attr_str(e: &BytesStart, key: &[u8]) -> Option<String> {
    for a in e.attributes().flatten() {
        if a.key.local_name().as_ref() == key {
            return Some(String::from_utf8_lossy(&a.value).into_owned());
        }
    }
    None
}

/// `@val` 布林旗標（OOXML：`val="0"`／`"false"` 為假，缺屬性＝真）。
fn bool_attr(e: &BytesStart) -> bool {
    match attr_str(e, b"val") {
        Some(v) => !(v == "0" || v.eq_ignore_ascii_case("false")),
        None => true,
    }
}

/// strict 把百分比寫成 `"65%"`、transitional 寫成千分數 `65000`——
/// 統一換算成 0.0–1.0 比例。
fn parse_percent(raw: String) -> Option<f64> {
    let t = raw.trim();
    if let Some(p) = t.strip_suffix('%') {
        return p.trim().parse::<f64>().ok().map(|v| (v / 100.0).clamp(0.0, 1.0));
    }
    t.parse::<f64>().ok().map(|v| (v / 100_000.0).clamp(0.0, 1.0))
}

/// 文字收集目標（內部狀態機）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sink {
    None,
    Title,
    SerTx,
    CatPt,
    ValV { ser: usize },
    ValFormula,
    AxisSize { ax: AxisWhich },
    TitleSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AxisWhich {
    None,
    Cat,
    Val,
}

/// 解析一份 chart part 明文。壞 XML＝停在已收部分（容錯不 panic）。
pub fn parse_chart(xml: &[u8], theme: &ThemeColors) -> ChartDoc {
    let text = String::from_utf8_lossy(xml).into_owned();
    let mut reader = Reader::from_str(&text);
    let mut d = ChartDoc::default();
    let mut path: Vec<&'static [u8]> = Vec::new();
    let mut ser: isize = -1;
    let mut sink: Sink = Sink::None;
    let mut title_buf = String::new();
    let mut tx_buf = String::new();
    let mut cat_buf = String::new();
    // 巢狀深度計數器（End 配對遞減，畸形檔不致卡死）。
    let mut in_title = 0usize;
    let mut in_title_tx = 0usize;
    let mut in_ser_tx = 0usize;
    let mut in_cat = 0usize;
    let mut in_val = 0usize;
    let mut in_ser_sp = 0usize;
    let mut in_solid_fill = 0usize;
    let mut color = ColorAcc::default();
    loop {
        match reader.read_event() {
            Err(_) | Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = e.local_name();
                let n = name.as_ref();
                push_path(&mut path, n);
                match n {
                    b"chartSpace" | b"chart" | b"plotArea" | b"rich" | b"dateAx" | b"scaling" => {}
                    b"title" => {
                        in_title += 1;
                        d.title_declared = true;
                    }
                    b"tx" => {
                        if in_title > 0 {
                            in_title_tx += 1;
                        } else if ser >= 0 {
                            in_ser_tx += 1;
                        }
                    }
                    b"ser" => {
                        ser = d.series.len() as isize;
                        d.series.push(ChartSeries::default());
                    }
                    b"cat" => in_cat += 1,
                    b"val" => in_val += 1,
                    b"spPr" if ser >= 0 => in_ser_sp += 1,
                    b"barChart"
                    | b"lineChart"
                    | b"areaChart"
                    | b"pieChart"
                    | b"scatterChart"
                    | b"doughnutChart"
                    | b"radarChart"
                    | b"bubbleChart"
                    | b"area3DChart"
                    | b"bar3DChart"
                    | b"line3DChart"
                    | b"pie3DChart"
                    | b"ofPieChart"
                    | b"stockChart"
                    | b"surfaceChart" => {
                        let k = String::from_utf8_lossy(n).into_owned();
                        if !d.kinds.contains(&k) {
                            d.kinds.push(k);
                        }
                    }
                    b"barDir" => {
                        if let Some(v) = attr_str(&e, b"val") {
                            d.bar_dir_col = v == "col";
                        }
                    }
                    b"grouping" => {
                        if let Some(v) = attr_str(&e, b"val") {
                            d.stacked |= v == "stacked" || v == "percentStacked";
                        }
                    }
                    b"autoTitleDeleted" => d.auto_title_deleted = bool_attr(&e),
                    b"numFmt" => {
                        if let (Some(a), Some(v)) = (axis_slot_mut(&mut d, ax_of(&path)), attr_str(&e, b"formatCode")) {
                            a.num_fmt = Some(v);
                        }
                    }
                    b"orientation" => {
                        if let (Some(a), Some(v)) = (axis_slot_mut(&mut d, ax_of(&path)), attr_str(&e, b"val")) {
                            a.orientation = Some(v);
                        }
                    }
                    b"majorGridlines" => {
                        if let Some(a) = axis_slot_mut(&mut d, ax_of(&path)) {
                            a.major_gridlines = true;
                        }
                    }
                    b"delete" => {
                        if let Some(a) = axis_slot_mut(&mut d, ax_of(&path)) {
                            a.deleted = bool_attr(&e);
                        }
                    }
                    b"catAx" => d.cat_ax.present = true,
                    b"valAx" => d.val_ax.present = true,
                    b"numRef" | b"strRef" => {
                        // 引用式：只認「數值軸來源」那一支留 `c:f`（類別／系列名的
                        // 引用式連字串都不留，避免誤填 val_formula）。
                        if in_val > 0 && in_cat == 0 && in_ser_tx == 0 && in_title == 0 {
                            sink = Sink::ValFormula;
                        }
                    }
                    b"numCache" | b"strCache" => {
                        if in_cat > 0 {
                            // 文字由 `c:pt//c:v` 收（Sink::CatPt 在 `v` 臂設）。
                            sink = Sink::None;
                        } else if in_val > 0 && in_ser_tx == 0 {
                            sink = Sink::None;
                        }
                    }
                    b"txPr" => {
                        let ax = ax_of(&path);
                        if ax != AxisWhich::None {
                            sink = Sink::AxisSize { ax };
                        } else if in_title > 0 {
                            sink = Sink::TitleSize;
                        }
                    }
                    b"defRPr" => {
                        let sz = attr_str(&e, b"sz").and_then(|s| s.parse::<f64>().ok());
                        match sink {
                            Sink::AxisSize { ax } => {
                                if let (Some(a), Some(v)) = (axis_slot_mut(&mut d, ax), sz) {
                                    a.tick_size_pt = Some((v / 100.0) as f32);
                                }
                            }
                            Sink::TitleSize => {
                                if let Some(v) = sz {
                                    d.title_size_pt = Some((v / 100.0) as f32);
                                }
                            }
                            _ => {}
                        }
                        color = ColorAcc::default();
                    }
                    b"solidFill" => in_solid_fill += 1,
                    b"schemeClr" | b"srgbClr" if in_solid_fill > 0 => {
                        color = ColorAcc::default();
                        color.srgb = n == b"srgbClr";
                        color.token = attr_str(&e, b"val");
                    }
                    b"lumMod" | b"lumOff" if in_solid_fill > 0 => {
                        if let Some(v) = attr_str(&e, b"val").and_then(parse_percent) {
                            if n == b"lumMod" {
                                color.lum_mod = Some(v);
                            } else {
                                color.lum_off = Some(v);
                            }
                        }
                    }
                    b"t" => {
                        // `c:rich//a:t`（標題／系列名的 RichText 式）。
                        if in_title_tx > 0 {
                            sink = Sink::Title;
                        } else if in_ser_tx > 0 {
                            sink = Sink::SerTx;
                        }
                    }
                    b"v" => {
                        if in_ser_tx > 0 {
                            sink = Sink::SerTx;
                        } else if in_cat > 0 {
                            sink = Sink::CatPt;
                        } else if in_val > 0 {
                            sink = Sink::ValV {
                                ser: ser.max(0) as usize,
                            };
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.local_name();
                let n = name.as_ref();
                match n {
                    b"autoTitleDeleted" => d.auto_title_deleted = bool_attr(&e),
                    b"title" => d.title_declared = true,
                    b"delete" => {
                        if let Some(a) = axis_slot_mut(&mut d, ax_of(&path)) {
                            a.deleted = bool_attr(&e);
                        }
                    }
                    b"majorGridlines" => {
                        if let Some(a) = axis_slot_mut(&mut d, ax_of(&path)) {
                            a.major_gridlines = true;
                        }
                    }
                    b"barDir" => {
                        if let Some(v) = attr_str(&e, b"val") {
                            d.bar_dir_col = v == "col";
                        }
                    }
                    b"grouping" => {
                        if let Some(v) = attr_str(&e, b"val") {
                            d.stacked |= v == "stacked" || v == "percentStacked";
                        }
                    }
                    b"orientation" => {
                        if let (Some(a), Some(v)) =
                            (axis_slot_mut(&mut d, ax_of(&path)), attr_str(&e, b"val"))
                        {
                            a.orientation = Some(v);
                        }
                    }
                    b"numFmt" => {
                        if let (Some(a), Some(v)) =
                            (axis_slot_mut(&mut d, ax_of(&path)), attr_str(&e, b"formatCode"))
                        {
                            a.num_fmt = Some(v);
                        }
                    }
                    b"defRPr" => {
                        let sz = attr_str(&e, b"sz").and_then(|s| s.parse::<f64>().ok());
                        match sink {
                            Sink::AxisSize { ax } => {
                                if let (Some(a), Some(v)) = (axis_slot_mut(&mut d, ax), sz) {
                                    a.tick_size_pt = Some((v / 100.0) as f32);
                                }
                            }
                            Sink::TitleSize => {
                                if let Some(v) = sz {
                                    d.title_size_pt = Some((v / 100.0) as f32);
                                }
                            }
                            _ => {}
                        }
                    }
                    b"schemeClr" | b"srgbClr" if in_solid_fill > 0 => {
                        color = ColorAcc::default();
                        color.srgb = n == b"srgbClr";
                        color.token = attr_str(&e, b"val");
                    }
                    b"lumMod" | b"lumOff" if in_solid_fill > 0 => {
                        if let Some(v) = attr_str(&e, b"val").and_then(parse_percent) {
                            if n == b"lumMod" {
                                color.lum_mod = Some(v);
                            } else {
                                color.lum_off = Some(v);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                let Ok(s) = crate::qx::unescape_text(&t) else {
                    continue;
                };
                match sink {
                    Sink::Title => title_buf.push_str(&s),
                    Sink::SerTx => tx_buf.push_str(&s),
                    Sink::CatPt => cat_buf.push_str(&s),
                    Sink::ValV { ser: i } => {
                        if let Ok(v) = s.trim().parse::<f64>() {
                            if let Some(sv) = d.series.get_mut(i) {
                                sv.vals.push(v);
                            }
                        }
                    }
                    Sink::ValFormula => {
                        if let Some(i) = (ser >= 0).then_some(ser as usize) {
                            if let Some(sv) = d.series.get_mut(i) {
                                if sv.val_formula.is_none() {
                                    sv.val_formula = Some(s.trim().to_string());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name();
                let n = name.as_ref();
                match n {
                    b"t" | b"v" => {
                        if matches!(sink, Sink::CatPt) && !cat_buf.is_empty() {
                            if let Some(i) = (ser >= 0).then_some(ser as usize) {
                                if let Some(sv) = d.series.get_mut(i) {
                                    sv.cats.push(std::mem::take(&mut cat_buf));
                                }
                            }
                            cat_buf.clear();
                        }
                        sink = Sink::None;
                    }
                    b"solidFill" => {
                        in_solid_fill = in_solid_fill.saturating_sub(1);
                        if let Some(argb) = color.resolve(theme) {
                            if in_ser_sp > 0 {
                                // 系列本體實色（chart-size：accent1 直柱）。
                                if let Some(i) = (ser >= 0).then_some(ser as usize) {
                                    if let Some(sv) = d.series.get_mut(i) {
                                        if sv.fill_argb.is_none() {
                                            sv.fill_argb = Some(argb);
                                        }
                                    }
                                }
                            } else if matches!(sink, Sink::AxisSize { .. }) {
                                if let Some(a) = axis_slot_mut(&mut d, ax_of(&path)) {
                                    if a.tick_color.is_none() {
                                        a.tick_color = Some(argb);
                                    }
                                }
                            } else if path.iter().any(|x| *x == b"majorGridlines") {
                                if d.grid_color.is_none() {
                                    d.grid_color = Some(argb);
                                }
                            } else if in_title == 0 && d.axis_color.is_none() {
                                d.axis_color = Some(argb);
                            }
                        }
                        color = ColorAcc::default();
                    }
                    b"txPr" => sink = Sink::None,
                    b"title" => {
                        in_title = in_title.saturating_sub(1);
                        if in_title == 0 {
                            let t = title_buf.trim().to_string();
                            if !t.is_empty() && d.title.is_none() {
                                d.title = Some(t);
                            }
                            title_buf.clear();
                        }
                    }
                    b"tx" => {
                        if in_title_tx > 0 {
                            in_title_tx = in_title_tx.saturating_sub(1);
                        } else if in_ser_tx > 0 {
                            in_ser_tx = in_ser_tx.saturating_sub(1);
                            if in_ser_tx == 0 {
                                let t = tx_buf.trim().to_string();
                                if !t.is_empty() {
                                    if let Some(i) = (ser >= 0).then_some(ser as usize) {
                                        if let Some(sv) = d.series.get_mut(i) {
                                            if sv.name.is_none() {
                                                sv.name = Some(t);
                                            }
                                        }
                                    }
                                }
                                tx_buf.clear();
                            }
                        }
                    }
                    b"cat" => {
                        in_cat = in_cat.saturating_sub(1);
                        if in_cat == 0 && ser >= 0 {
                            if let Some(sv) = d.series.get_mut(ser as usize) {
                                if sv.cats.is_empty() {
                                    sv.auto_cats = sv.vals.len();
                                }
                            }
                        }
                    }
                    b"val" => in_val = in_val.saturating_sub(1),
                    b"ser" => {
                        if let Some(i) = (ser >= 0).then_some(ser as usize) {
                            if let Some(sv) = d.series.get_mut(i) {
                                // 無 `c:cat` 元素＝LO 自動編號（實據 chart-size／strict）。
                                if sv.cats.is_empty() {
                                    sv.auto_cats = sv.vals.len();
                                }
                            }
                        }
                        ser = -1;
                        in_ser_sp = 0;
                        in_ser_tx = 0;
                    }
                    b"spPr" => in_ser_sp = in_ser_sp.saturating_sub(1),
                    _ => {}
                }
                pop_path(&mut path, n);
            }
            _ => {}
        }
    }
    d.plottable = d
        .kinds
        .iter()
        .any(|k| ChartDoc::plottable_kind(k.as_str()));
    d.accent1 = theme.resolve("accent1");
    d
}

/// 目前路徑落在哪一支軸（`c:valAx`／`c:catAx`）。
fn ax_of(path: &[&'static [u8]]) -> AxisWhich {
    let has = |n: &[u8]| path.iter().any(|p| *p == n);
    if has(b"valAx") {
        AxisWhich::Val
    } else if has(b"catAx") {
        AxisWhich::Cat
    } else {
        AxisWhich::None
    }
}

fn axis_slot_mut<'a>(d: &'a mut ChartDoc, w: AxisWhich) -> Option<&'a mut ChartAxis> {
    match w {
        AxisWhich::Val => Some(&mut d.val_ax),
        AxisWhich::Cat => Some(&mut d.cat_ax),
        AxisWhich::None => None,
    }
}

/// `a:solidFill` 的色成分累積器（schemeClr／srgbClr＋lumMod／lumOff）。
#[derive(Default, Clone)]
struct ColorAcc {
    token: Option<String>,
    srgb: bool,
    lum_mod: Option<f64>,
    lum_off: Option<f64>,
}

impl ColorAcc {
    /// 解析成 0xAARRGGBB；主題查不到／無宣告＝None（老實不虛構）。
    ///
    /// lumMod／lumOff 對 LO 口徑（實證見 `docs/f1-chart-anatomy.md` §3b）：
    /// tx1 × lumMod 15% ＋ lumOff 85% ＝ 0xD9D9D9（LO 格線實測灰 0.851）；
    /// tx1 × lumMod 65% ＋ lumOff 35% ＝ 0x595959（LO 刻度實測 5855577）。
    fn resolve(&self, theme: &ThemeColors) -> Option<u32> {
        let base = if self.srgb {
            let rgb = self.token.as_deref().and_then(|t| u32::from_str_radix(t, 16).ok())?;
            0xFF00_0000 | rgb
        } else {
            theme.resolve(self.token.as_deref()?)?
        };
        let mut v = base;
        if let Some(m) = self.lum_mod {
            v = blend_channel(v, m, false);
        }
        if let Some(o) = self.lum_off {
            v = blend_channel(v, o, true);
        }
        Some(v)
    }
}

/// 單通道亮度混合：lumMod＝朝黑乘積、lumOff＝朝白加權（floor 口徑同
/// `crate::theme::apply_shading_transform`）。
fn blend_channel(argb: u32, k: f64, toward_white: bool) -> u32 {
    let mut out = argb & 0x00FF_FFFF;
    for shift in [16u32, 8, 0] {
        let c = (argb >> shift) & 0xFF;
        let v = if toward_white {
            255.0 - (255.0 - f64::from(c)) * (1.0 - k)
        } else {
            f64::from(c) * k
        };
        let v = v.floor().clamp(0.0, 255.0) as u32;
        out = (out & !(0xFF << shift)) | (v << shift);
    }
    (argb & 0xFF00_0000) | out
}

/// 路徑追蹤：只記白名單節點。
fn push_path(path: &mut Vec<&'static [u8]>, n: &[u8]) {
    if let Some(w) = WATCHED.iter().find(|w| *w == &n) {
        path.push(*w);
    }
}

fn pop_path(path: &mut Vec<&'static [u8]>, n: &[u8]) {
    if let Some(i) = path.iter().rposition(|x| *x == n) {
        path.remove(i);
    }
}
