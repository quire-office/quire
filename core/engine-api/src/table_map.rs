//! canonical 表格樹 → layout 層 `TableInput`／`TableSlot`（Phase3-1b 映射層）。
//!
//! 分層：canonical 型別定義在 quire-ooxml、幾何演算法在 quire-layout，本模組
//! 只在 engine 層把前者餵成後者的輸入契約（與 sectPr 同款三段式）。換算
//! （twip→pt、pct→%）在此完成，layout 只吃 pt／百分比。
//!
//! 樣式框線：`w:tblStyle` 查 `word/styles.xml` 的 `w:style[@w:type='table']`
//! → `w:tblPr/w:tblBorders`，並沿 `w:basedOn` 鏈向上繼承（鏈環／斷鏈停住，
//! 容錯不 panic）。這是 v1 的最小查表——條件框線（firstRow/band1Horz 等）
//! 不實作，與 layout 層头注的取捨一致。
//!
//! 條件**底紋**（K8-shd）：`tblStylePr/w:tcPr/w:shd` 經 `w:tblLook` 生效閘
//! 逐屬性級聯消費（見 [`ShadingCtx`]），條件框線仍走 v1 最小查表不變。
//!
//! 條件**字色**（K22）：`tblStylePr/w:rPr/w:color` 與底色同軸、同閘、同
//! 優先序（[`cond_run_colors`]，資料流到 engine 級聯層 `cascade_paragraphs`
//! 的 run 著色烘焙；直接字色＞條件字色，ECMA-376 §17.4.75）。

use std::collections::HashMap;

use quire_layout::table::{
    CellBlock, CellInput, CellMarginInput, CellRun, EdgeSetInput, PenInput, RowInput, TableInput,
    WidthSpec,
};
use quire_layout::{LayoutParams, SectionSpec, TableSlot, TextMeasurer};
use quire_ooxml::{
    apply_shading_transform, Block, BodyTable, Borders, Margins, StyleTable, Table, TableCell,
    TblWidth, ThemeColors,
};
use quire_render_trait::{FLAG_BOLD, FLAG_ITALIC, FLAG_STRIKE, FLAG_UNDERLINE};

// K8-shd：條件底紋求值（`ShadingCtx`＝表格樣式表＋主題色表）。規則全部由
// fdo68607 LO 實測釘死（紅燈 k8_shd 檔頭四條鐵證＋LO PDF 逐頁 F/B 矩形計數）：
// 1. 生效閘＝`w:tblLook`（六旗標；缺件走 ECMA 預設 0x04A0——縱帶預設關）；
// 2. 優先序（低→高，ECMA-376 §17.4.75 順序表）：firstRow < lastRow <
//    firstCol < lastCol < band1Horz < band2Horz < band1Vert < band2Vert <
//    四角——後者覆蓋前者，逐屬性級聯非整條件覆蓋（fdo68607：firstColumn
//    粗體格仍 gets band1Horz 底色；`ShdDecision::Pass` 續條）；
// 3. 帶序＝排除特殊列（firstRow 列／lastRow 列）後按文件序 1-based：
//    奇數→band1Horz、偶數→band2Horz（縱帶同理按欄序）；跨頁不重置
//    （LO p1 首列 W、p2 首列 F，全域列序自然產生反相）；
// 4. 色值＝cache `@w:fill`（hex6）優先於 theme 推導（corpus 1007 tint 例
//    975 逐位元組命中、32 分歧例 LO 全讀 cache）；無 cache 才走
//    `themeFill` 槽色 × `themeFillTint`/`themeFillShade` 變換
//    （`apply_shading_transform`，floor 口徑）。

/// 條件優先序（低→高；ECMA-376 §17.4.75 順序表，四角最高）。
const COND_PRIORITY: [&str; 12] = [
    "firstRow",
    "lastRow",
    "firstCol",
    "lastCol",
    "band1Horz",
    "band2Horz",
    "band1Vert",
    "band2Vert",
    "neCell",
    "nwCell",
    "seCell",
    "swCell",
];

/// 一格命中的條件集合（布爾序＝`COND_PRIORITY` 下標；true＝該格此條件生效）。
#[derive(Clone, Copy, Default)]
struct CellConds([bool; 12]);

impl CellConds {
    /// 轉成 `cond_band_fill` 要的 (條件名, 生效) 陣列。
    fn gates(&self) -> [(&'static str, bool); 12] {
        let mut out = [("firstRow", false); 12];
        for (i, on) in self.0.iter().enumerate() {
            out[i] = (COND_PRIORITY[i], *on);
        }
        out
    }
}

/// 一表的逐列逐格條件閘（`table_input` 用；`w:tblLook` 缺件走 ECMA 預設）。
///
/// 欄序知識閘（不虛構）：無 `w:tblGrid`（corpus selfgen 檔如此）時欄位無從
/// 判定——firstCol／lastCol／縱帶整體關，只放行列級條件。
fn compute_cond_gates(t: &Table) -> Vec<Vec<CellConds>> {
    let l = t.props.look.clone().unwrap_or_default().effective();
    let n_rows = t.rows.len();
    let ncols = t.grid.len() as u32;
    let mut rows = Vec::with_capacity(n_rows);
    let mut band = 0u32; // 帶序計數（排除特殊列，見模組注釋規則 3）
    for (ri, row) in t.rows.iter().enumerate() {
        let first_row = ri == 0 && l.first_row;
        let last_row = ri + 1 == n_rows && l.last_row;
        let in_band = !first_row && !last_row;
        if in_band {
            band += 1;
        }
        let horz = if !l.h_band || !in_band {
            0
        } else if band % 2 == 1 {
            1
        } else {
            2
        };
        let mut col = 0u32;
        let cells = row
            .cells
            .iter()
            .map(|c| {
                let span = c.props.grid_span().max(1);
                let start = col;
                col += span;
                let mut a = [false; 12];
                a[0] = first_row;
                a[1] = last_row;
                a[2] = ncols > 0 && l.first_column && start == 0;
                a[3] = ncols > 0 && l.last_column && col >= ncols;
                a[4] = horz == 1;
                a[5] = horz == 2;
                // 縱帶序同理排除 firstCol／lastCol 特殊欄（與橫帶對稱口徑；
                // corpus 無縱帶活例可實證——fdo68607 noVBand=1，老實標明）。
                let v_ord = if l.v_band && ncols > 0 && !a[2] && !a[3] {
                    Some((start + 1) % 2)
                } else {
                    None
                };
                a[6] = v_ord == Some(1); // band1Vert（1-based 奇數欄）
                a[7] = v_ord == Some(0); // band2Vert
                a[8] = first_row && a[3]; // neCell
                a[9] = first_row && a[2]; // nwCell
                a[10] = last_row && a[3]; // seCell
                a[11] = last_row && a[2]; // swCell
                CellConds(a)
            })
            .collect();
        rows.push(cells);
    }
    rows
}

/// twip → pt。非法／缺＝None（容錯鐵律：不猜、不虚構）。
fn twip_pt(raw: Option<&str>) -> Option<f32> {
    raw.and_then(|s| s.trim().parse::<f32>().ok())
        .map(|v| v / 20.0)
}

// ---------------------------------------------------------------------------
// K8-shd：儲存格底色級聯（直接 tcPr shd ＞ tblStylePr 條件帶（經 tblLook 閘））
// ---------------------------------------------------------------------------

/// K8-shd：底色級聯上下文（表格樣式表＋主題色表）。純讀取派生資料——
/// 存檔路徑完全不經本層（round-trip 零影響）。`None`＝無級聯資訊
/// （頁首尾表格呼叫端傳 None＝維持舊「只認直接 fill」行為，零擾動）。
#[derive(Clone, Copy)]
pub struct ShadingCtx<'a> {
    pub styles: &'a StyleTable,
    pub theme: &'a ThemeColors,
}

/// `w:shd` 一棵宣告的判讀結果（逐屬性級聯用——PM 鐵證②：條件命中但
/// 未宣告 fill 時必須**續往下條條件**找，不得整條件覆蓋）。
enum ShdDecision {
    /// 決定底色（0xAARRGGBB，alpha 恆 FF）。
    Color(u32),
    /// 明確不著色（`@w:val="nil"`／`@w:fill="auto"`）：壓掉更低優先條條件。
    Nil,
    /// 本條無 fill 意見（續下一条／退回下層）。
    Pass,
}

fn parse_hex6(s: &str) -> Option<u32> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

/// 一棵 `w:shd`（直接 tcPr 或 tblStylePr 條件帶同口徑）→ 判讀。
///
/// 口徑（PM 四條鐵證）：
/// - `@w:val="nil"`＝明確不著色（Nil）；`@w:fill="auto"`／空字串＝明確
///   無底色（Nil，壓掉下層——與舊直接路徑「auto＝無填色」結果一致）。
/// - **cache `@w:fill` 優先於 theme 推導值**（corpus 32/1007 例生產者
///   cache 與 theme 現值分歧，LO 照讀 cache）。
/// - theme 推導＝`themeFill` 查 clrScheme ＋ `themeFillTint/Shade` 變換
///   （`apply_shading_transform`，floor 非 round）。查無主題色＝Pass
///   （不虚構）。
/// - `K8_SHD_BUGGY=1`：theme 推導**與條件帶全滅**（呼叫端另跳過條件迴圈），
///   只留 cache fill 舊路徑——紅燈 k8_shd 燈 1/2/3 必紅的機制分離鎖。
fn shd_decision(
    val: Option<&str>,
    fill: Option<&str>,
    theme_fill: Option<&str>,
    tint: Option<&str>,
    shade: Option<&str>,
    ctx: Option<&ShadingCtx>,
) -> ShdDecision {
    if val.is_some_and(|v| v.eq_ignore_ascii_case("nil")) {
        return ShdDecision::Nil;
    }
    if let Some(f) = fill.map(str::trim).filter(|f| !f.is_empty()) {
        if f.eq_ignore_ascii_case("auto") {
            return ShdDecision::Nil;
        }
        if let Some(rgb) = parse_hex6(f) {
            return ShdDecision::Color(0xFF00_0000 | rgb);
        }
        // fill 是非法字串（畸形檔）：不得因它放棄 theme 路徑，續往下。
    }
    let buggy = std::env::var_os("K8_SHD_BUGGY").is_some();
    if buggy {
        return ShdDecision::Pass;
    }
    if let (Some(ctx), Some(tok)) = (ctx, theme_fill.map(str::trim)) {
        if !tok.is_empty() && !tok.eq_ignore_ascii_case("none") {
            if let Some(base) = ctx.theme.resolve(tok) {
                return ShdDecision::Color(
                    apply_shading_transform(base, tint, shade).unwrap_or(base),
                );
            }
        }
    }
    ShdDecision::Pass
}

/// 條件帶查表結果（含 Nil 壓下層語義）。
fn cond_band_fill(style_id: &str, conds: &[(&str, bool)], ctx: &ShadingCtx) -> Option<u32> {
    // K8_SHD_BUGGY=1：條件帶全滅（紅燈 k8_shd 燈 1/2/3 的 BUGGY 鎖）。
    if std::env::var_os("K8_SHD_BUGGY").is_some() {
        return None;
    }
    for (cond, gate) in conds {
        if !gate {
            continue;
        }
        let Some(cs) = ctx.styles.table_style_cond_shading(style_id, cond) else {
            continue;
        };
        match shd_decision(
            cs.val.as_deref(),
            cs.fill.as_deref(),
            cs.theme_fill.as_deref(),
            cs.theme_fill_tint.as_deref(),
            cs.theme_fill_shade.as_deref(),
            Some(ctx),
        ) {
            ShdDecision::Color(c) => return Some(c),
            // 明確不著色＝本條件宣告壓掉更低優先條（逐屬性級聯）。
            ShdDecision::Nil => return None,
            // 該條件未宣告 fill：續下一條（PM 鐵證②實測：firstCol 粗體
            // 格仍 gets band1Horz 底色）。
            ShdDecision::Pass => continue,
        }
    }
    None
}

// ---------------------------------------------------------------------------
// K22：條件字色級聯（tblStylePr/w:rPr/w:color——與條件底色同軸、同閘、
// 同優先序走 [`COND_PRIORITY`]；fdo68607 firstRow 同時宣告 shd＋color，
// K8 只接了底色層，字色層自此補齊）。
// ---------------------------------------------------------------------------

/// K22：逐段落查表——正文表 `tblStylePr` 條件字色的最終生效值
/// （段落索引 → 0xAARRGGBB）。Map 有鍵＝條件層宣告過字色（含 `auto`＝
/// 明確黑）；無鍵＝條件層無意見（級聯維持現行落點，零擾動）。
///
/// 層序語見呼叫端（`cascade_paragraphs` 的 K22 段）：本層＝表格條件層，
/// 只在 run 級聯鏈完全沒宣告顏色時落地（直接／字元／段落樣式在上）。
/// 頁首尾表不走本函數（K8 同款零擾動口徑）。`K22_COND_COLOR_BUGGY=1`＝
/// 整層關閉（回塞鎖：紅燈 k22 燈 1/2/3 必紅）。
pub fn cond_run_colors(tables: &[BodyTable], ctx: &ShadingCtx) -> HashMap<usize, u32> {
    let mut out = HashMap::new();
    // 快路：全表無任何條件字色宣告（corpus 378/400 檔）＝整層跳過，
    // 逐表 compute_cond_gates 的錢都不花（開檔零成本）。
    if !ctx.styles.has_cond_color() {
        return out;
    }
    if std::env::var_os("K22_COND_COLOR_BUGGY").is_some() {
        return out;
    }
    for bt in tables {
        collect_cond_color_table(&bt.table, None, ctx, &mut out);
    }
    out
}

/// 一表（含巢狀）逐格條件字色 → 格內段落烘焙。樣式 id：自帶＞繼承外層
/// （ECMA §17.4.65，同 `table_input` 巢狀口徑）；無任何樣式 id＝本表無
/// 條件層，但巢狀子表仍可能自帶——續遞迴，不虚構。
fn collect_cond_color_table(
    t: &quire_ooxml::Table,
    inherit: Option<&str>,
    ctx: &ShadingCtx,
    out: &mut HashMap<usize, u32>,
) {
    let Some(sid) = t.props.style.as_deref().or(inherit) else {
        for row in &t.rows {
            for c in &row.cells {
                for b in &c.blocks {
                    if let Block::Table(inner) = b {
                        collect_cond_color_table(inner, None, ctx, out);
                    }
                }
            }
        }
        return;
    };
    // 生效閘與帶序完全沿用 K8 的 compute_cond_gates（tblLook 閘、排除特殊
    // 列的 1-based 帶序、欄序知識閘）——字色與底色同軸的「同軸」就是這層。
    let gates = compute_cond_gates(t);
    for (ri, row) in t.rows.iter().enumerate() {
        for (ci, cell) in row.cells.iter().enumerate() {
            let cc = gates
                .get(ri)
                .and_then(|g| g.get(ci))
                .and_then(|cd| cond_cell_color(sid, &cd.gates(), ctx));
            for b in &cell.blocks {
                match b {
                    Block::Paragraph { index } => {
                        if let Some(c) = cc {
                            // 同段被多格覆蓋（畸形 vMerge）＝首見為準，與
                            // cond_shading 收錄口徑一致。
                            out.entry(*index).or_insert(c);
                        }
                    }
                    Block::Table(inner) => {
                        collect_cond_color_table(inner, Some(sid), ctx, out);
                    }
                }
            }
        }
    }
}

/// 一格命中的條件字色（`COND_PRIORITY` 序逐條查，首個宣告者定案——與
/// [`cond_band_fill`] 的底色查表同口徑；未宣告＝續下一条，逐屬性級聯非
/// 整條件覆蓋）。`w:color val="auto"`（color_set 真、color None）＝明確黑。
fn cond_cell_color(style_id: &str, conds: &[(&str, bool)], ctx: &ShadingCtx) -> Option<u32> {
    for (cond, gate) in conds {
        if !gate {
            continue;
        }
        if let Some(rs) = ctx.styles.table_style_cond_color(style_id, cond) {
            return Some(rs.color.unwrap_or(0xFF00_0000));
        }
    }
    None
}

/// `w:tblW`/`w:tcW` → `WidthSpec`。dxa→Pt；pct→Pct（pct 單位是 1/50%，
/// ECMA-376 §17.18.90）；auto/nil/缺＝None（由 layout 退回自然寬）。
/// K7c-3：`num<=0` 亦回 None——`<w:tblW w:w="0" w:type="dxa"/>` 是歷史檔
/// 把 auto 寫成 dxa 的變體（Word 自身也會產出），並非宣告零寬：誤判成
/// `Some(Pt(0))` 讓 is_some()=true 走壓版心心路徑、又失去 K7 自然寬均分
/// 資格。零/負寬在版面上無意義（寬 0 表不可見），依容錯鐵律退回自然寬。
fn width_spec(tw: &TblWidth) -> Option<WidthSpec> {
    let num = tw.num()? as f32;
    if num <= 0.0 {
        return None;
    }
    match tw.typ.as_deref() {
        Some("dxa") => Some(WidthSpec::Pt(num / 20.0)),
        Some("pct") => Some(WidthSpec::Pct(num / 50.0)),
        _ => None,
    }
}

/// 邊距單側：dxa→pt；pct/非法＝None（退回上層預設——v1 不做 pct 邊距）。
fn side_pt(tw: Option<&TblWidth>) -> Option<f32> {
    let tw = tw?;
    if matches!(tw.typ.as_deref(), Some("dxa") | None) {
        twip_pt(tw.w.as_deref())
    } else {
        None
    }
}

fn pen(b: Option<&quire_ooxml::Border>) -> Option<PenInput> {
    b.map(|b| PenInput {
        val: b.val.clone(),
        sz: b.sz.clone(),
        // color 原字串直送：auto/theme 名→黑、RRGGBB→RGB 的判定在 layout。
        color: b.color.clone(),
    })
}

fn edge_set(bs: &Borders) -> EdgeSetInput {
    EdgeSetInput {
        top: pen(bs.top.as_ref()),
        left: pen(bs.left.as_ref()),
        bottom: pen(bs.bottom.as_ref()),
        right: pen(bs.right.as_ref()),
        inside_h: pen(bs.inside_h.as_ref()),
        inside_v: pen(bs.inside_v.as_ref()),
    }
}

/// `w:tblCellMar`／`w:tcMar` → 四側 pt：新式 start/end 優先，舊式 left/right 後備。
fn cell_margins(m: &Margins) -> CellMarginInput {
    CellMarginInput {
        top: side_pt(m.top.as_ref()),
        left: side_pt(m.start.as_ref()).or_else(|| side_pt(m.left.as_ref())),
        bottom: side_pt(m.bottom.as_ref()),
        right: side_pt(m.end.as_ref()).or_else(|| side_pt(m.right.as_ref())),
    }
}

fn has_any_edges(e: &EdgeSetInput) -> bool {
    [
        e.top.as_ref(),
        e.left.as_ref(),
        e.bottom.as_ref(),
        e.right.as_ref(),
        e.inside_h.as_ref(),
        e.inside_v.as_ref(),
    ]
    .into_iter()
    .any(|p| p.is_some_and(|p| p.val.is_some()))
}

/// K23：`word/styles.xml` → styleId → **樣式級** `w:tblCellMar`（四側 pt，
/// 沿 basedOn 鏈合成、子贏父；與 `table_style_borders` 同款容錯：鏈長上限
/// 16＋拜訪集防環）。
///
/// 存在的理由（實測根因）：`w:tblCellMar` 不只寫在 `w:tblPr`，Word 的內建表
/// 格式（TableNormal／TableGrid／自訂樣式）普遍把內距宣告在**樣式**裡。舊
/// 級聯只看 `w:tblPr` 直宣告與 ECMA 預設，樣式級整級漏掉 ⇒ 宣告 0／86dxa
/// 的表被錯誤按 5.4pt/側排版（LO 25.8 ground truth：NumberedList／
/// tdf103976 格線左界 126.8→文字 x0=131.1＝4.3pt＝86dxa；tdf115883 樣式
/// 宣告 0 內距）。級聯位階（ECMA §17.4.40／§17.4.49＋LO 實測）：
/// 直接 `w:tcMar` ＞ `w:tblPr/w:tblCellMar` ＞ 本表（樣式級）＞ ECMA 預設。
pub fn table_style_cell_margins(styles_xml: &[u8]) -> HashMap<String, CellMarginInput> {
    let xml = String::from_utf8_lossy(styles_xml);
    // ---- 第一趟：逐 <w:style …> 記錄自帶四側與 basedOn ----
    let mut own: HashMap<String, (Option<CellMarginInput>, Option<String>)> = HashMap::new();
    let mut cursor = 0usize;
    while let Some(rel) = xml[cursor..].find("<w:style ") {
        let start = cursor + rel;
        let end = match xml[start..].find("</w:style>") {
            Some(e) => start + e + "</w:style>".len(),
            None => xml.len(),
        };
        cursor = end;
        let seg = &xml[start..end];
        if !seg.contains("w:type=\"table\"") {
            continue;
        }
        let Some(id) = attr(seg, "w:styleId") else {
            continue;
        };
        let based_on = seg
            .find("<w:basedOn ")
            .and_then(|i| attr(&seg[i..], "w:val"));
        let margins = find_tbl_cell_mar(seg).map(|m| cell_margins(&m));
        own.insert(id, (margins, based_on));
    }
    // ---- 第二趟：沿 basedOn 補缺位（子先、父補缺側 ⇒ 子宣告贏） ----
    let mut out: HashMap<String, CellMarginInput> = HashMap::new();
    for id in own.keys() {
        let mut acc: CellMarginInput = Default::default();
        let mut seen: Vec<&String> = Vec::new();
        let mut cur: Option<&String> = Some(id);
        let mut stack: Vec<Option<&CellMarginInput>> = Vec::new();
        for _ in 0..16 {
            let Some(key) = cur else { break };
            let Some((margins, based_on)) = own.get(key) else {
                break;
            };
            if seen.contains(&key) {
                break; // 鏈環：老實停在已合成結果
            }
            seen.push(key);
            stack.push(margins.as_ref());
            cur = based_on
                .as_ref()
                .and_then(|b| own.get_key_value(b))
                .map(|(k, _)| k);
        }
        // 子先寫、父只補缺側 ⇒ derived style wins（宣告 0 是有效宣告，
        // 必須壓過父樣式的 108dxa——NumberedList ProcedureTable 實測口徑）。
        for m in stack.iter().copied().flatten() {
            fill_missing_margins(&mut acc, m);
        }
        let any =
            acc.top.is_some() || acc.left.is_some() || acc.bottom.is_some() || acc.right.is_some();
        if any {
            out.insert(id.clone(), acc);
        }
    }
    out
}

/// 樣式級 `w:tblCellMar` → `Margins`（沿用 ooxml 側的方位結構，缺側＝None）。
/// 自閉合（`<w:tblCellMar/>`）或無閉合標籤（畸形）＝不認（回 None，老實不虛構）。
fn find_tbl_cell_mar(seg: &str) -> Option<Margins> {
    let i = seg.find("<w:tblCellMar")?;
    let tag_end = seg[i..].find('>').map(|e| i + e)?;
    if seg[tag_end.saturating_sub(1)..=tag_end] == *"/>" {
        return None; // 自閉合＝無任何側宣告
    }
    let close = seg[tag_end..].find("</w:tblCellMar>")? + tag_end;
    let region = &seg[tag_end + 1..close];
    let side = |tag: &str| -> Option<TblWidth> {
        let k = region.find(&format!("<w:{tag} "))?;
        let e = region[k..].find("/>")? + k;
        let w = attr(&region[k..=e], "w:w")?;
        Some(TblWidth {
            w: Some(w),
            typ: attr(&region[k..=e], "w:type"),
        })
    };
    let m = Margins {
        top: side("top"),
        start: side("start"),
        bottom: side("bottom"),
        end: side("end"),
        left: side("left"),
        right: side("right"),
        ..Default::default()
    };
    let any = m.top.is_some()
        || m.start.is_some()
        || m.bottom.is_some()
        || m.end.is_some()
        || m.left.is_some()
        || m.right.is_some();
    any.then_some(m)
}

/// 逐側補缺位（`None`＝該側未宣告，非宣告 0——0 是有效宣告必須保留）。
fn fill_missing_margins(dst: &mut CellMarginInput, src: &CellMarginInput) {
    if dst.top.is_none() {
        dst.top = src.top;
    }
    if dst.left.is_none() {
        dst.left = src.left;
    }
    if dst.bottom.is_none() {
        dst.bottom = src.bottom;
    }
    if dst.right.is_none() {
        dst.right = src.right;
    }
}

/// 格內圖片投影表（t_a1397522）：錨定段落 → 依文档序的尺寸序列。
/// 同一段落多張圖＝逐張注入（CellBlock::Image 在格內垂直堆疊、帶高累加
/// ——卡片語義；不合併，合併會虛構容量）。
fn cell_image_map(images: &[quire_ooxml::InlineImage]) -> HashMap<usize, Vec<(f32, f32)>> {
    let mut m: HashMap<usize, Vec<(f32, f32)>> = HashMap::new();
    let buggy = std::env::var_os("K10_CELL_ANCHOR_BUGGY").is_some();
    for im in images
        .iter()
        .filter(|im| im.in_cell && (buggy || !im.floating))
    {
        m.entry(im.para_index).or_default().push((im.w_pt, im.h_pt));
    }
    m
}

/// `word/styles.xml` → styleId → 表格框線（沿 basedOn 鏈合成後的結果）。
///
/// 實作：先收錄每張樣式「自帶」的 tblBorders，再逐樣式沿 basedOn 向上補缺位
/// （缺位＝該方位未宣告，非宣告 none）。鏈長上限 16＋拜訪集防環。
pub fn table_style_borders(styles_xml: &[u8]) -> HashMap<String, EdgeSetInput> {
    let xml = String::from_utf8_lossy(styles_xml);
    // ---- 第一趟：逐 <w:style …>…</w:style> 記錄自帶框線與 basedOn ----
    let mut own: HashMap<String, (Option<EdgeSetInput>, Option<String>)> = HashMap::new();
    let mut cursor = 0usize;
    while let Some(rel) = xml[cursor..].find("<w:style ") {
        let start = cursor + rel;
        let end = match xml[start..].find("</w:style>") {
            Some(e) => start + e + "</w:style>".len(),
            None => xml.len(),
        };
        cursor = end;
        let seg = &xml[start..end];
        if !seg.contains("w:type=\"table\"") {
            continue;
        }
        let Some(id) = attr(seg, "w:styleId") else {
            continue;
        };
        let based_on = {
            let re_at = seg.find("<w:basedOn ");
            re_at.and_then(|i| attr(&seg[i..], "w:val"))
        };
        let borders = find_tbl_borders(seg).map(|bs| edge_set(&bs));
        own.insert(id, (borders, based_on));
    }
    // ---- 第二趟：沿 basedOn 補缺位 ----
    let mut out: HashMap<String, EdgeSetInput> = HashMap::new();
    for id in own.keys() {
        let mut acc: EdgeSetInput = Default::default();
        let mut seen: Vec<&String> = Vec::new();
        let mut cur: Option<&String> = Some(id);
        for _ in 0..16 {
            let Some(key) = cur else { break };
            let Some((borders, based_on)) = own.get(key) else {
                break;
            };
            if seen.contains(&key) {
                break; // 鏈環：老實停在已合成結果
            }
            seen.push(key);
            if let Some(b) = borders {
                fill_missing(&mut acc, b);
            }
            cur = based_on
                .as_ref()
                .and_then(|b| own.get_key_value(b))
                .map(|(k, _)| k);
        }
        if has_any_edges(&acc) {
            out.insert(id.clone(), acc);
        }
    }
    out
}

fn fill_missing(dst: &mut EdgeSetInput, src: &EdgeSetInput) {
    let e = [
        (&mut dst.top, &src.top),
        (&mut dst.left, &src.left),
        (&mut dst.bottom, &src.bottom),
        (&mut dst.right, &src.right),
        (&mut dst.inside_h, &src.inside_h),
        (&mut dst.inside_v, &src.inside_v),
    ];
    for (d, s) in e {
        if d.is_none() {
            *d = s.clone();
        }
    }
}

fn attr(seg: &str, name: &str) -> Option<String> {
    let pat = format!("{name}=\"");
    let i = seg.find(&pat)? + pat.len();
    let j = seg[i..].find('"')?;
    let v = &seg[i..i + j];
    (!v.is_empty()).then(|| v.to_string())
}

/// 在 style 段落內找 `w:tblBorders`（簡易字首掃描：tblBorders 只出現在
/// tblPr 內，一段 style 一份，取首份即可）。
fn find_tbl_borders(seg: &str) -> Option<Borders> {
    let i = seg.find("<w:tblBorders")?;
    let j = seg[i..]
        .find("</w:tblBorders>")
        .map(|j| i + j)
        .unwrap_or(seg.len());
    let region = &seg[i..j];
    let pick = |tag: &str| -> Option<quire_ooxml::Border> {
        let pat = format!("<w:{tag} ");
        let k = region.find(&pat)?;
        let e = region[k..]
            .find("/>")
            .map(|e| k + e)
            .unwrap_or(region.len());
        let one = &region[k..e];
        let b = quire_ooxml::Border {
            val: attr(one, "w:val"),
            sz: attr(one, "w:sz"),
            space: attr(one, "w:space"),
            color: attr(one, "w:color"),
            theme_color: attr(one, "w:themeColor"),
            theme_shade: attr(one, "w:themeShade"),
            theme_tint: attr(one, "w:themeTint"),
        };
        (!b.is_empty()).then_some(b)
    };
    let bs = Borders {
        top: pick("top"),
        left: pick("left").or_else(|| pick("start")),
        bottom: pick("bottom"),
        right: pick("right").or_else(|| pick("end")),
        inside_h: pick("insideH"),
        inside_v: pick("insideV"),
        tl2br: None,
        tr2bl: None,
    };
    (!bs.is_empty()).then_some(bs)
}

/// 段落級聯 run → 格內文字的 run 級樣式標注（L2-cellruns／TODO(CardA) 清償）。
///
/// 只為**確有多樣式**的段落產出標注：逐 run 累加碼點偏移、相鄰同鍵
/// `(flags, size_pt, color_argb)` 段先併段，併完仍 ≥2 段才入表——單樣式段
/// （含全段同鍵）整段不入表＝發射端走舊單樣式路徑，金樣與儀表板零擾動。
/// 鍵含新欄位（段合併鍵判別鐵律：size/color 三態入鍵，缺＝級聯未宣告）。
/// `flags` 以 render-trait 位元烘焙（layout 只透傳不解釋）；`color_argb`
/// 缺＝級聯未宣告（發射端退回預設黑，與 `run_style` 口徑一致）。
fn cell_run_spans(paras: &[quire_ooxml::Paragraph]) -> HashMap<usize, Vec<CellRun>> {
    if std::env::var("L2_BUGGY").is_ok() {
        return HashMap::new(); // 回塞：整體關閉標注（紅燈必須倒）
    }
    let mut out: HashMap<usize, Vec<CellRun>> = HashMap::new();
    for (i, p) in paras.iter().enumerate() {
        if p.runs.len() < 2 {
            continue;
        }
        let mut spans: Vec<CellRun> = Vec::new();
        let mut cp = 0usize;
        for r in &p.runs {
            let n = r.text.chars().count();
            if n == 0 {
                continue; // 零文字 run（tab／br-only）不佔碼點
            }
            let mut flags = 0u8;
            if r.bold {
                flags |= FLAG_BOLD;
            }
            if r.italic {
                flags |= FLAG_ITALIC;
            }
            if r.underline {
                flags |= FLAG_UNDERLINE;
            }
            if r.strike {
                flags |= FLAG_STRIKE;
            }
            let from = cp;
            cp += n;
            if let Some(last) = spans.last_mut() {
                // 相鄰同鍵併段（沿用正文 `emit_line_run_items` 合併鍵口徑：
                // 字型宣告不入鍵——選族屬後續卡，否則同款式 run 只因字型
                // 不同就被拆開，違反零版面變動契約）。
                if last.flags == flags && last.size_pt == r.size_pt && last.color_argb == r.color {
                    last.to = cp;
                    continue;
                }
            }
            spans.push(CellRun {
                from,
                to: cp,
                flags,
                size_pt: r.size_pt,
                color_argb: r.color,
            });
        }
        if spans.len() >= 2 {
            out.insert(i, spans);
        }
    }
    out
}

/// canonical 表 → layout 輸入（遞迴巢狀表；樣式框線查 `styles`）。
/// `cell_imgs`（t_a1397522）：格內圖片投影（錨定段落 → 尺寸）——段落的
/// Block 之後追加 `CellBlock::Image`（文件序：文字在上、圖在下）。
/// `sizes`（t_5dfc8ac7）：段落索引 → 級聯有效字級（一格多段取最大）。
/// `run_spans`（L2-cellruns）：段落索引 → run 級樣式標注（只含確有多樣式
/// 的段落）；依格內**文字 block 出現序**平行落入 `CellInput::run_spans`。
/// `advs`（K7-cal）：段落索引 → 水平 advance 尺（一格多段取**最大**＝保守
/// 少塞字，同 `sizes` 取最大哲學；缺位＝不入格內欄＝舊斷行）。
#[allow(clippy::too_many_arguments)]
pub fn table_input(
    t: &Table,
    texts: &[String],
    styles: &HashMap<String, EdgeSetInput>,
    // K23：styleId → 樣式級 `w:tblCellMar`（四側 pt，沿 basedOn 合成）。
    style_margins: &HashMap<String, CellMarginInput>,
    cell_imgs: &HashMap<usize, Vec<(f32, f32)>>,
    sizes: &HashMap<usize, f32>,
    line_hs: &HashMap<usize, f32>,
    line_ref_bands: u32,
    run_spans: &HashMap<usize, Vec<CellRun>>,
    advs: &HashMap<usize, f32>,
    // K8-shd：條件底紋上下文（None＝頁首尾表格／診斷鏡像零擾動路徑；Some
    // ＝正文表消费 tblStylePr 條件層）。`style_inherit`＝巢狀表未自帶
    // `w:tblStyle` 時繼承的外層樣式 id（ECMA §17.4.65：子表無宣告沿父表）。
    cond: Option<&ShadingCtx<'_>>,
    style_inherit: Option<&str>,
    // K17：本表各列的錨定框撐列地板（row_idx→pt；巢狀表傳空表＝不撐，宿主
    // 列定位只認最內層直接格）。空表＝舊行為逐位元組不變。
    grow_floors: &HashMap<usize, f32>,
    // R32：wrapSquare 文字框對宿主段落的推下量（para_idx→pt）。併入該格
    // `margins.top`，讓首行文字落到框底之下（LO 口徑）。空表＝舊行為。
    wrap_tops: &HashMap<usize, f32>,
) -> TableInput {
    let p = &t.props;
    // K8-shd：本表（含巢狀）逐列逐格條件閘；巢狀表無自己的閘資料時不虛構
    // （`cond` 為 None 即整棵零擾動）。樣式 id：自帶 > 繼承外層。
    let style_id = p.style.as_deref().or(style_inherit);
    let gates: Option<Vec<Vec<CellConds>>> = match (cond, style_id) {
        (Some(_), Some(_)) => Some(compute_cond_gates(t)),
        _ => None,
    };
    TableInput {
        // M1-band：細尺份數逐層直送（巢狀表同尺；粗尺＝0/1＝舊行為）。
        line_ref_bands,
        grid_pt: t.grid.iter().map(|g| g.w_pt.unwrap_or(0.0)).collect(),
        width: p.width.as_ref().and_then(width_spec),
        fixed_layout: p.layout_fixed == Some(true),
        indent_pt: match p.indent_typ.as_deref() {
            Some("pct") => None, // v1：pct 縮排不換算（退回 0，不虚構）
            _ => twip_pt(p.indent_raw.as_deref()),
        },
        justify: p.justify.clone(),
        borders: p.borders.as_ref().map(edge_set),
        style_borders: p
            .style
            .as_deref()
            .and_then(|s| styles.get(s))
            .cloned()
            .filter(has_any_edges),
        cell_margins: p.cell_margins.as_ref().map(cell_margins),
        // K23：樣式級內距（查表口徑同 `style_borders`——只認本表自帶 tblStyle；
        // 巢狀表未自帶時不虛構繼承，本 corpus 無巢狀樣式內距案例）。
        style_cell_margins: p
            .style
            .as_deref()
            .and_then(|s| style_margins.get(s))
            .cloned()
            .filter(|m| {
                m.top.is_some() || m.left.is_some() || m.bottom.is_some() || m.right.is_some()
            }),
        rows: t
            .rows
            .iter()
            .enumerate()
            .map(|(ri, r)| RowInput {
                cant_split: r.props.cant_split,
                height_pt: twip_pt(r.props.height_raw.as_deref()),
                height_exact: r.props.height_rule.as_deref() == Some("exact"),
                header: r.props.header,
                // K17：錨定框撐列地板（空表＝None＝舊行為）。
                grow_floor_pt: grow_floors.get(&ri).copied(),
                cells: r
                    .cells
                    .iter()
                    .enumerate()
                    .map(|(ci, c)| {
                        // 格內 block **保持文件序**交錯映射（根因 1）：段落與
                        // 子表各自入獨立 Vec 會丟失拓撲序，子表後的尾部段落
                        // 會被拉回頂部——流式堆疊的前提就在這裡。
                        let mut blocks = Vec::new();
                        // run 標注（L2-cellruns）：與 blocks 中**文字 block
                        // 出現序**逐位對齊的平行欄位；缺位（無標注段落）
                        // 存空 vec，保持索引對齊。
                        let mut spans: Vec<Vec<CellRun>> = Vec::new();
                        // K6：與文字 block 出現序平行的宣告行高（細化檔才
                        // 填；見 `CellInput::text_line_h_pt`）。
                        let mut text_line_h: Vec<f32> = Vec::new();
                        // 一格多段的級聯字級＝段內最大（斷行量測保守尺：
                        // 大字級少塞字，不虛構塞得下的行）。
                        let mut cell_size: Option<f32> = None;
                        // K7-cal：格內水平尺（一格多段取最大＝保守少塞字）。
                        let mut cell_adv: Option<f32> = None;
                        // R32：本格 anchors 的 wrap 推下量（同格多段取最大）。
                        let mut wrap_top: Option<f32> = None;
                        for b in &c.blocks {
                            match b {
                                // 段落索引引用扁平序列：越界＝畸形檔，跳過（容錯）。
                                Block::Paragraph { index } => {
                                    if let Some(s) = texts.get(*index) {
                                        blocks.push(CellBlock::Text(s.clone()));
                                        spans.push(
                                            run_spans.get(index).cloned().unwrap_or_default(),
                                        );
                                        text_line_h
                                            .push(line_hs.get(index).copied().unwrap_or(0.0));
                                    }
                                    if let Some(sz) = sizes.get(index) {
                                        cell_size = Some(cell_size.map_or(*sz, |v| v.max(*sz)));
                                    }
                                    // K7-cal：格內水平尺＝有文字段的最大值
                                    // （呼叫端已濾掉空文字段；未命中族＝1.0，
                                    // 混排格保守走舊斷行，零誤窄）。
                                    if let Some(a) = advs.get(index) {
                                        cell_adv = Some(cell_adv.map_or(*a, |v: f32| v.max(*a)));
                                    }
                                    // R32：wrapSquare 文字框推下量（LO 首行落框底）。
                                    if let Some(off) = wrap_tops.get(index) {
                                        wrap_top = Some(wrap_top.map_or(*off, |v| v.max(*off)));
                                    }
                                    // 錨定在該段的格內圖（t_a1397522）：段落
                                    // 之後依序追加（文件序；查無尺寸＝老實跳過）。
                                    if let Some(imgs) = cell_imgs.get(index) {
                                        for &(w, h) in imgs {
                                            blocks.push(CellBlock::Image { w_pt: w, h_pt: h });
                                        }
                                    }
                                }
                                Block::Table(inner) => {
                                    blocks.push(CellBlock::Table(Box::new(table_input(
                                        inner,
                                        texts,
                                        styles,
                                        style_margins,
                                        cell_imgs,
                                        sizes,
                                        line_hs,
                                        line_ref_bands,
                                        run_spans,
                                        advs,
                                        // K8-shd：巢狀表沿用同一 ctx；樣式未自帶
                                        // 時繼承本表（子表常不重複宣告 tblStyle）。
                                        cond,
                                        style_id,
                                        // K17：巢狀表不撐（宿主列定位只認最內層
                                        // 直接格；撐外層列無 LO 校準依據）。
                                        &HashMap::new(),
                                        // R32：巢狀表段落無 wrap 推下事實。
                                        &HashMap::new(),
                                    ))));
                                }
                            }
                        }
                        // K8-shd：底色級聯（直接 `tcPr/w:shd` 判讀優先：Color
                        // 即定案、`nil`／`auto`＝明確無底色（壓掉條件帶）、
                        // Pass＝本條無意見續查條件帶；直接 shd 也支援 theme
                        // 推導——舊路徑只認 cache hex，此為能力補齊）。
                        let band = || match (cond, style_id, gates.as_ref()) {
                            (Some(ctx), Some(sid), Some(g)) => g
                                .get(ri)
                                .and_then(|row| row.get(ci))
                                .and_then(|cd| cond_band_fill(sid, &cd.gates(), ctx))
                                // `Color` 帶 alpha（0xFF…）；CellInput.fill 契約＝
                                // 6 位 RRGGBB（layout `parse_hex6` 恰 6 位才收）。
                                .map(|argb| format!("{:06X}", argb & 0xFF_FFFF)),
                            _ => None,
                        };
                        let fill: Option<String> = match c.props.shading.as_ref() {
                            Some(sh) => match shd_decision(
                                sh.val.as_deref(),
                                sh.fill.as_deref(),
                                sh.theme_fill.as_deref(),
                                sh.theme_fill_tint.as_deref(),
                                sh.theme_fill_shade.as_deref(),
                                cond,
                            ) {
                                ShdDecision::Color(a) => Some(format!("{:06X}", a & 0xFF_FFFF)),
                                ShdDecision::Nil => None,
                                ShdDecision::Pass => band(),
                            },
                            None => band(),
                        };
                        CellInput {
                            width: c.props.width.as_ref().and_then(width_spec),
                            grid_span: c.props.grid_span(),
                            vmerge: c.props.vmerge_restart(),
                            blocks,
                            borders: c.props.borders.as_ref().map(edge_set),
                            fill,
                            valign: c.props.valign.clone(),
                            margins: {
                                // R32：wrapSquare 文字框推下量併入格頂內距
                                // （宣告值更大時保留宣告；layout 端再量化帶）。
                                let mut margins = c.props.margins.as_ref().map(cell_margins);
                                if let Some(off) = wrap_top {
                                    let mut m = margins.unwrap_or_default();
                                    m.top = Some(m.top.unwrap_or(0.0).max(off));
                                    margins = Some(m);
                                }
                                margins
                            },
                            text_size_pt: cell_size,
                            text_advance_scale: cell_adv,
                            text_line_h_pt: text_line_h,
                            run_spans: spans,
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// 表格（含全部巢狀表）覆蓋的段落上界（不含）＝最大段索引＋1；全無段落＝para_start。
pub(crate) fn table_para_end(t: &Table, para_start: usize) -> usize {
    let mut end = para_start;
    t.walk(&mut |sub| {
        for row in &sub.rows {
            for cell in &row.cells {
                for b in &cell.blocks {
                    if let Block::Paragraph { index } = b {
                        end = end.max(index + 1);
                    }
                }
            }
        }
    });
    end
}

/// 逐頁 section 的可用寬（首 section 缺件時退回 params 基準值）。
fn avail_w_for(sections: &[SectionSpec], params: &LayoutParams, para: usize) -> f32 {
    match sections.get(para) {
        Some(s) => (s.geo.page_w_pt - s.geo.margin_left_pt - s.geo.margin_right_pt).max(1.0),
        None => params.content_w_pt(),
    }
}

/// `wp:positionH/V` 之 `<wp:align>` 白名單（ECMA-376 §20.4 ST_PosHAlign／
/// ST_PosVAlign；t_1a79f1c5）。t：left/center/right＋inside/outside；
/// b：top/center/bottom＋inside/outside。其餘枚舉（insideLeft/bar/...）
/// 保守退回退化鏈。
///
/// `inside`／`outside` 為裝訂雙面頁枚舉，真實落點取決於奇偶頁與裝訂方向；
/// 本層無頁碼奇偶資訊，故以 frame 邊界近似：inside≈left、outside≈right。
fn h_align_x(align: &str, frame_w: f32, w_pt: f32) -> Option<f32> {
    // 比對前轉小寫：第三方工具可能輸出 `<wp:align>Center</wp:align>`（S3），
    // 與 drawing 儲存處的正規化互為雙保險。
    match align.to_ascii_lowercase().as_str() {
        "left" | "inside" => Some(0.0),
        "center" => Some((frame_w - w_pt) / 2.0),
        "right" | "outside" => Some(frame_w - w_pt),
        _ => None,
    }
}
fn v_align_y(align: &str, frame_h: f32, h_pt: f32) -> Option<f32> {
    match align.to_ascii_lowercase().as_str() {
        // 同 h 軸：inside≈top、outside≈bottom 之近似（無奇偶頁資訊）。
        "top" | "inside" => Some(0.0),
        // `inner` 非 ST_PosVAlign 標準枚舉（來源可疑，保留相容），
        // 語意近似居中。
        "center" | "inner" => Some((frame_h - h_pt) / 2.0),
        "bottom" | "outside" => Some(frame_h - h_pt),
        _ => None,
    }
}

/// K10／K15／G1：查詢段落 `para_index` 在表格儲存格內的頁面原點、寬度與宿主列追蹤資訊。
/// `use_content_box` 為真（`!layout_in_cell && !G1_BUGGY`）時，H 軸原點取欄內容盒左緣
/// （`表左 + 格左 + ½左框線寬 + 左內距`）、寬取內容盒寬（`格寬 - 左內距 - 右內距`），
/// V 軸原點加上上框線寬與上內距（`列頂 + 上框線寬 + 上內距`）；為假時維持 K10/K15 格框原點。
pub fn find_cell_for_para(
    tables: &[BodyTable],
    slots: &[TableSlot],
    sections: &[SectionSpec],
    params: &LayoutParams,
    para_index: usize,
    use_content_box: bool,
    y_rel_to_row: bool,
) -> Option<((f32, f32, f32, f32), quire_layout::CellHostAnchor)> {
    for (slot_idx, (bt, slot)) in tables.iter().zip(slots.iter()).enumerate() {
        if para_index < slot.para_start || para_index >= slot.para_end {
            continue;
        }
        for (r_idx, row) in bt.table.rows.iter().enumerate() {
            for (c_idx, cell) in row.cells.iter().enumerate() {
                let found = cell.blocks.iter().any(|b| match b {
                    Block::Paragraph { index } => *index == para_index,
                    _ => false,
                });
                if found {
                    let cgeom = slot.layout.cell_content_geom(r_idx, c_idx)?;
                    let (margin_l, margin_t) = match sections.get(slot.para_start) {
                        Some(s) => (s.geo.margin_left_pt, s.geo.margin_top_pt),
                        None => (
                            params.margin(quire_layout::MARGIN_LEFT),
                            params.margin(quire_layout::MARGIN_TOP),
                        ),
                    };
                    let tblp_y = twip_pt(bt.table.props.tblp_y_raw.as_deref()).unwrap_or(0.0);
                    let static_row_top = margin_t + cgeom.cell_y_pt;
                    let table_left = margin_l + slot.layout.x_off;
                    let (cell_x, cell_y, cell_w) = if use_content_box {
                        (
                            margin_l
                                + cgeom.cell_x_pt
                                + 0.5 * cgeom.border_left_pt
                                + cgeom.pad_left_pt,
                            static_row_top + tblp_y + cgeom.border_top_pt + cgeom.pad_top_pt,
                            (cgeom.cell_w_pt - cgeom.pad_left_pt - cgeom.pad_right_pt).max(1.0),
                        )
                    } else {
                        (
                            margin_l + cgeom.cell_x_pt,
                            static_row_top + tblp_y,
                            cgeom.cell_w_pt,
                        )
                    };
                    let host = quire_layout::CellHostAnchor {
                        slot: slot_idx as u32,
                        row: r_idx as u32,
                        static_top_bits: static_row_top.to_bits(),
                        y_rel_to_row,
                    };
                    return Some(((cell_x, cell_y, cell_w, table_left), host));
                }
            }
        }
    }
    None
}

/// 浮動圖座標解析（t_1a79f1c5）：`wp:positionH/V` 的 posOffset 與 align 是
/// **互斥兩式**（ECMA-376 §20.4.2.8）——OOXMLExport 對整頁封面圖（wrapNone
/// ＋`<wp:align>center</wp:align>`）只寫 align 不寫 posOffset，舊口徑把缺
/// posOffset 一律退化 inline，792pt 封面圖撐爆行網格（1 頁变 3 頁）。
/// 現行規則：兩軸 posOffset 齊備 → 直接用；缺軸補 `<wp:align>` 換算
/// （frame 依 relativeFrom 分類取頁寬／版心寬／錨點段寬）；兩式皆無 →
/// None（呼叫端退化 inline，誠實近似宁多撑帶不虚構空頁）。
/// `page_w`／`page_h`＝頁寬高（page 系 frame）；`avail_w`＝版心寬（margin 系 frame）。
fn resolved_anchor(
    im: &quire_ooxml::InlineImage,
    page_w: f32,
    page_h: f32,
    avail_w: f32,
    avail_h: f32,
    anchor_para_w: Option<f32>,
    cell_info: Option<(f32, f32, f32, f32)>,
) -> Option<quire_layout::FloatingAnchor> {
    // 逐軸可解析才成錨：posOffset 或 `<wp:align>` 兩式任一即可（互斥但
    // 不必同時存在）；relativeFrom 缺＝段落系近似（origin_of 同款容錯）。
    if !im.floating {
        return None;
    }
    let x_origin = quire_ooxml::origin_of(im.pos_x_rel.as_deref());
    let y_origin = quire_ooxml::origin_of(im.pos_y_rel.as_deref());

    // K10：若身處表格儲存格內且非 page 原點系，以儲存格幾何為基準換算頁絕對原點。
    // K15：`layoutInCell=1` 壓制 `relativeFrom` 的 page 字面意義——宿主在儲存格內時
    // page 系軸同樣改以宿主儲存格為基準（LO 實測：WC048 Textbox3 page 系 V 軸＝
    // 第 2 列格頂＋posOffset 21.0pt）；無宿主儲存格（cell_info=None）才回退頁原點。
    let cell_rules = |origin: &str| origin != "page" || im.layout_in_cell;
    let (frame_w, x_base, x_is_page) =
        if let (Some((cx, _, cw, _)), true) = (cell_info, cell_rules(x_origin)) {
            (cw, cx, true)
        } else {
            let is_col_rel = im.pos_x_rel.as_deref() == Some("column");
            let fw = match x_origin {
                "page" => page_w,
                "column" | "margin" if is_col_rel => anchor_para_w.unwrap_or(avail_w),
                "column" | "margin" => avail_w,
                _ => anchor_para_w.unwrap_or(avail_w),
            };
            (fw, 0.0, false)
        };

    let (frame_h, y_base, y_is_page) =
        if let (Some((_, cy, _, _)), true) = (cell_info, cell_rules(y_origin)) {
            (avail_h.max(1.0), cy, true)
        } else {
            let fh = match y_origin {
                "page" => page_h,
                _ => avail_h.max(1.0),
            };
            (fh, 0.0, false)
        };

    // K15：pct 式的百分比基準恆為 `relativeFrom` 原宣告的 frame（page→頁寬高）；
    // layoutInCell 只換**原點**不換百分比基準（WC048 Textbox3：pctPosVOffset=2500
    // ＝頁高×2.5%＝21.05pt，LO 實測＝格頂＋21.0；誤用版心高會得 17.45）。
    // 非 page 原點系維持 K10 的 frame（零擾動）。
    let pct_frame_w = if x_origin == "page" { page_w } else { frame_w };
    let pct_frame_h = if y_origin == "page" { page_h } else { frame_h };

    // 逐軸三式平滑退檔：posOffset → `<wp:align>` → `<wp14:pctPos*Offset>`。
    // align 值不被識別（如 insideLeft／非法字串）只算該式落空，不可短路
    // 整個錨點——否則第三式 pct 永遠退不到（S2）。
    let x_offset = im
        .pos_x_pt
        .or_else(|| {
            im.pos_x_align
                .as_deref()
                .and_then(|a| h_align_x(a, frame_w, im.w_pt))
        })
        .or_else(|| im.pos_x_pct.map(|p| pct_frame_w * p / 100_000.0))?;
    let y_offset = im
        .pos_y_pt
        .or_else(|| {
            im.pos_y_align
                .as_deref()
                .and_then(|a| v_align_y(a, frame_h, im.h_pt))
        })
        .or_else(|| im.pos_y_pct.map(|p| pct_frame_h * p / 100_000.0))?;

    let mut x_pt = x_base + x_offset;
    let y_pt = y_base + y_offset;
    // K15：layoutInCell 的 align 式（center/right）算出的框不得越過**宿主表格左緣**——
    // LO 實測：Textbox3（右對齊、框寬 387 > 欄寬 150，算得負值）落在 72.25＝表左緣；
    // Textbox1（置中、185.9 > 150）同落 72.2。反例：Textbox2（中欄置中、273.6 > 150，
    // 左端 160.6 已在表內）LO 不夾到格左緣 222.25，故夾底基準是表左緣而非格左緣。
    // posOffset／pct 兩式是作者明示偏移，不夾（老實照排）。
    if let (true, true, Some((_, _, _, table_left))) =
        (im.layout_in_cell, im.pos_x_pt.is_none(), cell_info)
    {
        if x_is_page {
            x_pt = x_pt.max(table_left);
        }
    }

    Some(quire_layout::FloatingAnchor {
        x_pt,
        y_pt,
        x_frame: if x_is_page {
            quire_layout::OriginFrame::Page
        } else if im.pos_x_rel.as_deref() == Some("column") && anchor_para_w.is_some() {
            quire_layout::OriginFrame::Column
        } else {
            quire_layout::origin_frame(x_origin)
        },
        y_frame: if y_is_page {
            quire_layout::OriginFrame::Page
        } else {
            quire_layout::origin_frame(y_origin)
        },
    })
}

/// 正文流圖片錨點（t_a1397522）：InlineImage 投影 → layout `ImageBand` 序列。
/// 規則：
/// - 格內圖（in_cell）不進正文流（由 CellBlock::Image 負載，格內堆疊）；
/// - 浮動圖（anchor）且座標可解析（posOffset 或 align，t_1a79f1c5）→
///   **不佔帶**（bands=0），`anchor=Some(FloatingAnchor)`，各軸原點系由
///   relativeFrom 分類；
/// - 鎖點兩式皆缺 → 退化 inline（佔帶，誠實近似宁多撑帶不虚構空頁）；
/// - 帶高 ceil(h/line_h)（至少 1）；寬高老實原值（layout 端再鉗）。
///
/// align 換算的 frame 尺寸逐段查 `sections`（缺項退回 params 基準），
/// 與表格 `avail_w_for` 同口徑。輸出依 para 穩定排序
/// （build_flow_images 的單調游標契約）。
pub fn build_images(
    images: &[quire_ooxml::InlineImage],
    line_h: f32,
    params: &LayoutParams,
    sections: &[SectionSpec],
    tables: &[BodyTable],
    slots: &[TableSlot],
) -> Vec<quire_layout::ImageBand> {
    let buggy = std::env::var_os("K10_CELL_ANCHOR_BUGGY").is_some();
    // K15 BUGGY 鎖：忽略 layoutInCell 旗標（回到 K10 純 relativeFrom 原點系）。
    let k15_buggy = std::env::var_os("K15_CELL_ANCHOR_BUGGY").is_some();
    // G1 BUGGY 鎖：退回修前靜態表格列 y、表格流起頭頁附掛與無框線內距補償。
    let g1_buggy = std::env::var_os("G1_BUGGY").is_some();
    let mut out: Vec<quire_layout::ImageBand> = images
        .iter()
        .filter(|im| !im.in_cell || (!buggy && im.floating))
        .map(|im| {
            let im_owned;
            let im = if k15_buggy && im.layout_in_cell {
                im_owned = quire_ooxml::InlineImage {
                    layout_in_cell: false,
                    ..im.clone()
                };
                &im_owned
            } else {
                im
            };
            // 錨定段落的頁面幾何：page 系 frame 用頁寬高，margin 系用版心寬高。
            let (page_w, page_h, avail_w, avail_h) = match sections.get(im.para_index) {
                Some(s) => (
                    s.geo.page_w_pt,
                    s.geo.page_h_pt,
                    (s.geo.page_w_pt - s.geo.margin_left_pt - s.geo.margin_right_pt).max(1.0),
                    (s.geo.page_h_pt - s.geo.margin_top_pt - s.geo.margin_bottom_pt).max(1.0),
                ),
                None => (
                    params.page_w_pt,
                    params.page_h_pt,
                    params.content_w_pt(),
                    (params.page_h_pt
                        - params.margin(quire_layout::MARGIN_TOP)
                        - params.margin(quire_layout::MARGIN_BOTTOM))
                    .max(1.0),
                ),
            };
            let y_origin = quire_ooxml::origin_of(im.pos_y_rel.as_deref());
            let y_rel_to_row = y_origin != "page" || im.layout_in_cell;
            let use_content_box = !g1_buggy && !im.layout_in_cell;
            let cell_lookup = if !buggy {
                find_cell_for_para(
                    tables,
                    slots,
                    sections,
                    params,
                    im.para_index,
                    use_content_box,
                    y_rel_to_row,
                )
            } else {
                None
            };
            let cell_info = cell_lookup.map(|(info, _)| info);
            let cell_host = if !g1_buggy {
                cell_lookup.map(|(_, host)| host)
            } else {
                None
            };
            let para_col_w = (!g1_buggy)
                .then(|| quire_layout::section_break_w_pt(params, sections, im.para_index));
            let mut anchor =
                resolved_anchor(im, page_w, page_h, avail_w, avail_h, para_col_w, cell_info);
            if let (Some(a), Some(ch)) = (anchor.as_mut(), cell_host) {
                a.y_frame = quire_layout::OriginFrame::CellRow(ch);
            }
            if std::env::var_os("G1_DEBUG").is_some() {
                eprintln!(
                    "G1DBG para={} floating={} in_cell={} lic={} xrel={:?} yrel={:?} \
                     xoff={:?} yoff={:?} xalign={:?} cell={:?} anchor={:?}",
                    im.para_index,
                    im.floating,
                    im.in_cell,
                    im.layout_in_cell,
                    im.pos_x_rel,
                    im.pos_y_rel,
                    im.pos_x_pt,
                    im.pos_y_pt,
                    im.pos_x_align,
                    cell_info,
                    anchor
                );
            }
            let bands = if anchor.is_none() {
                // 帶量化鉗上限（t_c1fb4c69 S1-4）：病態 extent（源頭值域
                // 拒絕後仍可能漏進投影層）不得量化出天量帶數拖垮分頁。
                let b = (im.h_pt / line_h).ceil();
                if !b.is_finite() {
                    quire_layout::MAX_PAGE_BANDS
                } else {
                    ((b.max(1.0)) as u32).min(quire_layout::MAX_PAGE_BANDS)
                }
            } else {
                0
            };
            quire_layout::ImageBand {
                para: im.para_index,
                w_pt: im.w_pt,
                h_pt: im.h_pt,
                bands,
                anchor,
                page_pinned: im.page_pinned,
                // D9b：宿主框的 `v:textbox/@inset` 宣告與「像素是框內 w:drawing
                // 內容圖」的事實（K27-B extent 落地態）隨投影一路搬到發射層，
                // 發射層據此定位框內圖，不再對單一檔案座標做字面量比對。
                txbx_inset: im.txbx_inset,
                txbx_content_pic: im.txbx_drawing_extent.is_some(),
            }
        })
        .collect();
    out.sort_by_key(|b| b.para);
    out
}

/// 由 canonical 樹＋扁平文字建出排版完成的表格槽序列（engine 開啟文件時呼叫）。
/// 回傳依 para_start 排序（parser 保證 body 序單調），`build_flow` 契約直接成立。
/// `images`（t_a1397522）：全文圖片投影；格內圖注入對應 `CellBlock::Image`。
/// `paras`（t_5dfc8ac7）：級聯烘焙後的段落投影（run 帶 effective size）——
/// 逐格文字字級＝格內段落首 run 有效字級（首 run 無宣告＝無宣告）。
#[allow(clippy::too_many_arguments)]
pub fn build_slots(
    tables: &[BodyTable],
    texts: &[String],
    styles_xml: Option<&[u8]>,
    params: &LayoutParams,
    sections: &[SectionSpec],
    images: &[quire_ooxml::InlineImage],
    paras: &[quire_ooxml::Paragraph],
    line_hs: &[f32],
    line_ref_bands: u32,
    advance_scales: &[f32],
    // K8-shd：條件底紋 ctx（`None`＝零擾動：頁首尾表格／診斷鏡像／回塞鎖）。
    cond_ctx: Option<&ShadingCtx<'_>>,
    // K17：樣式表（撐列 pitch 的框內字族／字級退檔來源，取 docDefaults；
    // `None`＝無宣告檔走 12pt／Liberation 尺）。頁首尾表傳 None（images
    // 亦為空表，撐列不生效）。
    doc_styles: Option<&StyleTable>,
) -> Vec<TableSlot> {
    let styles = styles_xml.map(table_style_borders).unwrap_or_default();
    // K23：樣式級 cell 內距查表（與框線同款「沿 basedOn 補缺位」）。
    let style_margins = styles_xml.map(table_style_cell_margins).unwrap_or_default();
    let cell_imgs = cell_image_map(images);
    // 段落 → 級聯有效字級（首 run；None 不入表＝全域尺）。
    let mut sizes: HashMap<usize, f32> = HashMap::new();
    for (i, p) in paras.iter().enumerate() {
        if let Some(sz) = p.runs.first().and_then(|r| r.size_pt) {
            sizes.insert(i, sz);
        }
    }
    // K7-cal：段落索引 → 水平 advance 尺。**空文字段不參與**（空行不斷行、
    // 不 demanded 寬——ECMA 子表尾部填充空段不得把整格推回 1.0）；有文字段
    // 一律入表（未命中族＝1.0），格內取最大＝混排保守（最寬尺少塞字，
    // 未校準族與校準族混排的格整體走舊斷行，零誤窄）。
    let advs: HashMap<usize, f32> = advance_scales
        .iter()
        .enumerate()
        .filter(|(i, a)| a.is_finite() && texts.get(*i).is_some_and(|t| !t.trim().is_empty()))
        .map(|(i, a)| (i, *a))
        .collect();
    // L2-cellruns：格內文字的 run 級樣式標注（只含確有多樣式的段落；
    // 單樣式段整段不入表＝發射端走舊路徑，金樣零位移）。
    let run_spans = cell_run_spans(paras);
    // K6：段落索引 → 級聯量化前行高（僅細化檔消費；粗尺 lref=1 時表空，
    // 格內行高逐位元組不動）。
    let line_h_map: HashMap<usize, f32> = if line_ref_bands > 1 {
        line_hs
            .iter()
            .enumerate()
            .filter(|(_, h)| h.is_finite() && **h > 0.0)
            .map(|(i, h)| (i, *h))
            .collect()
    } else {
        HashMap::new()
    };
    let m = TextMeasurer::new(params.font_size_pt);
    let line_h = params.line_h_pt();
    let mut prev_end = 0usize; // 逐槽正規化游標：槽區間必不重疊且 ≤ n_paras
    let n_paras = texts.len();
    tables
        .iter()
        .map(|bt| {
            // K17：錨定框撐列地板（本表宿主列；回塞鎖在函數內整支關閉）。
            let mut wrap_tops: HashMap<usize, f32> = HashMap::new();
            let floors = k17_grow_floors(
                &bt.table,
                images,
                params,
                sections,
                doc_styles,
                line_hs,
                &mut wrap_tops,
            );
            let input = table_input(
                &bt.table,
                texts,
                &styles,
                &style_margins,
                &cell_imgs,
                &sizes,
                &line_h_map,
                line_ref_bands,
                &run_spans,
                &advs,
                // 正文表：頂層無繼承樣式（每表自帶 `w:tblStyle`；巢狀繼承
                // 在 table_input 內部遞迴處理）。
                cond_ctx,
                None,
                &floors,
                &wrap_tops,
            );
            let avail_w = avail_w_for(sections, params, bt.para_start);
            let layout = quire_layout::table::layout_table(&input, avail_w, line_h, &m, 0);
            let raw_end = table_para_end(&bt.table, bt.para_start);
            // 索引空間對齊（t_580c37ac 後）：三個解析器（parse_document_xml／
            // table／sectpr）共用單一計數口徑——非自閉合 `w:p` 的 Start 佔槽
            // （主解析器 Start 預位、End 回填），`mc:Fallback` 整棵不計。
            // `para_start` 與 `Block::Paragraph{index}` 出自同一計數器，錨點
            // 天然落在扁平索引空間，不再需要「可見段落最小索引」重錨（舊
            // workaround：txbxContent 使兩計數器分岔，WC048 實證；已隨口徑
            // 統一移除）。clamp 保留——純防衛，守 `build_flow` 的「槽區間
            // 不重疊且 ≤ n_paras」契約不被畸形輸入擊穿。
            let start = bt.para_start.max(prev_end).min(n_paras);
            let end = raw_end.max(start).min(n_paras).max(start);
            prev_end = end;
            TableSlot {
                para_start: start,
                para_end: end,
                layout,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// K17：錨定文字框撐列（grow = posV + eff_h）
// ---------------------------------------------------------------------------

/// 撐列模型的 ECMA 預設內邊距（`bodyPr` 缺 tIns/bIns＝45720EMU＝3.6pt；
/// lIns/rIns＝91440EMU＝7.2pt）。出处 docs/k18-lo-rowgrow-calibration.md §3
/// （WC048 實測口徑），非湊數常數。
const K17_INS_TB_PT: f32 = 3.6;
const K17_INS_LR_PT: f32 = 7.2;

/// 錨定框撐列事實（本卡本地判別，不進 InlineImage——避免污染解析投影）。
struct K17GrowFacts {
    autofit_sp: bool,
    tins: f32,
    bins: f32,
    lins: f32,
    rins: f32,
    size_pt: Option<f32>,
    face: Option<String>,
    spacing_sum_pt: f32,
    line_declared: Option<f32>,
}

/// K17 撐列地板計算：對本表各列收集「宿主段落在該列」的合格錨定框
/// （floating＋layoutInCell＋wrap≠none＋**有 DrawingML bodyPr 事實**或群組框），
/// 每框取 `grow = posV + eff_h`（規格 `docs/k18-lo-rowgrow-calibration.md` §6），
/// 同列多框取 **max**（LO 疊加未證，保守）。回傳 row_idx → 地板 pt。
///
/// `eff_h`（§6(4)(5)）：
/// - noAutofit／normAutofit／群組框 → 宣告 extent cy（h_pt）原值；
/// - spAutoFit → tIns＋bIns＋Σ_段[(before＋after)＋n_lines×pitch]＋
///   Σ_top/bottom 段邊框(sz/8＋space)；
///   pitch＝size×(line_declared/240)×LH_face（**框內 declared 恆 240 口徑，
///   勿吃 docDefaults**；LH 用 K6 face_line_factor）；
///   n_lines 用 TextMeasurer 依框內寬（w−lIns−rIns）實斷行（LO 同款換行）。
///
/// posV 三式退檔（比照 resolved_anchor，但只取 V 軸**列內偏移**）：
/// posOffset → pct×(page 系用頁高／其余用版心高)；align 式撐列基準未校準
/// ＝老實跳過（不虛構撐列）。回塞鎖 `K17_ANCHOR_ROWGROW_BUGGY=1` 整支關閉。
fn k17_grow_floors(
    table: &Table,
    images: &[quire_ooxml::InlineImage],
    params: &LayoutParams,
    sections: &[SectionSpec],
    styles: Option<&StyleTable>,
    // R32：段落索引 → 實際行高（發射端口徑；wrap 推移量化用）。
    para_line_hs: &[f32],
    wrap_tops: &mut HashMap<usize, f32>,
) -> HashMap<usize, f32> {
    let line_h = params.line_h_pt();
    let mut out: HashMap<usize, f32> = HashMap::new();
    if std::env::var_os("K17_ANCHOR_ROWGROW_BUGGY").is_some() {
        return out;
    }
    let doc_face = styles.and_then(|t| t.doc_face());
    let doc_size = styles.map_or(12.0, |t| t.doc_grid().0.unwrap_or(12.0));
    for im in images {
        // 資格（紅燈檔頭口徑）：浮動 anchor＋layoutInCell＋wrap≠none
        // （無宣告＝ECMA 預設 wrapSquare＝可撐）。
        if !im.floating || !im.layout_in_cell {
            continue;
        }
        if im.wrap_type.as_deref() == Some("none") {
            continue;
        }
        // 撐列對象＝文字框（有 bodyPr 事實）或群組框（只吃宣告 extent，
        // §6(5)）。純圖片錨點未校準＝不撐（老實）。
        let facts = match &im.textbox_grow {
            Some(g) => Some(K17GrowFacts {
                autofit_sp: g.autofit == "spAutoFit",
                tins: g.tins_pt.unwrap_or(K17_INS_TB_PT),
                bins: g.bins_pt.unwrap_or(K17_INS_TB_PT),
                lins: g.lins_pt.unwrap_or(K17_INS_LR_PT),
                rins: g.rins_pt.unwrap_or(K17_INS_LR_PT),
                size_pt: g.size_pt,
                face: g.face.clone(),
                spacing_sum_pt: g.spacing_sum_pt,
                line_declared: g.line_declared,
            }),
            None if !im.group_shapes.is_empty() => Some(K17GrowFacts {
                // 群組框＝比照 noAutofit（K18 §2 推翻「宣告高撐列」後唯一
                // 存活口徑：群組只吃宣告 extent cy）。
                autofit_sp: false,
                tins: 0.0,
                bins: 0.0,
                lins: 0.0,
                rins: 0.0,
                size_pt: None,
                face: None,
                spacing_sum_pt: 0.0,
                line_declared: None,
            }),
            None => None,
        };
        let Some(f) = facts else { continue };
        // 列內 V 偏移：posOffset 直用；pct 依原宣告 frame（page→頁高）。
        let y_origin = quire_ooxml::origin_of(im.pos_y_rel.as_deref());
        let (page_h, avail_h) = match sections.get(im.para_index) {
            Some(s) => (
                s.geo.page_h_pt,
                (s.geo.page_h_pt - s.geo.margin_top_pt - s.geo.margin_bottom_pt).max(1.0),
            ),
            None => (
                params.page_h_pt,
                (params.page_h_pt
                    - params.margin(quire_layout::MARGIN_TOP)
                    - params.margin(quire_layout::MARGIN_BOTTOM))
                .max(1.0),
            ),
        };
        let let_pos_v = match (im.pos_y_pt, im.pos_y_pct) {
            (Some(v), _) => v,
            (None, Some(p)) => {
                let frame = if y_origin == "page" { page_h } else { avail_h };
                frame * p / 100_000.0
            }
            // align 式（或全缺）撐列基準未校準＝老實不撐（不虛構列高）。
            _ => continue,
        };
        // eff_h。
        let eff_h = if !f.autofit_sp {
            im.h_pt
        } else {
            let size = f.size_pt.unwrap_or(doc_size);
            let lh_face = crate::face_line_factor(
                f.face
                    .as_deref()
                    .or(doc_face.as_deref())
                    .unwrap_or("liberation serif"),
            );
            let pitch = size * (f.line_declared.unwrap_or(240.0) / 240.0) * lh_face;
            let inner_w = (im.w_pt - f.lins - f.rins).max(1.0);
            // K20：斷行測量器必須帶 K7-cal 水平尺（`face_advance_scale`）。
            // 用 `new`（1.0 尺）会把 Calibri 框內文字寬放大 ~13%：WC049-Mod
            // 37 字被判 2 行（LO 真尺 1 行）→ 錨定列多撐一整行 13.4pt。
            // 族退檔鏈＝框內宣告 → doc_face → liberation serif（與 pitch 的
            // lh_face 同尺同鏈，行高/行寬一個模型）。BUGGY 總開關走
            // `k7_advance_enabled`（回塞鎖既有 env，與正文斷行同進退）。
            let adv_face = f
                .face
                .as_deref()
                .or(doc_face.as_deref())
                .unwrap_or("liberation serif");
            let adv = if crate::k7_advance_enabled() {
                crate::face_advance_scale(adv_face)
            } else {
                1.0
            };
            let m = quire_layout::TextMeasurer::with_advance_scale(size, adv);
            let mut h = f.tins + f.bins + f.spacing_sum_pt;
            // 段落數：txbx_texts（框內段文字，文档序）。空 vector＝無段文字
            // 的事實（不虛構段落行；K18 盲測 pA 單段口徑由 len=1 自然覆蓋）。
            for text in &im.txbx_texts {
                let n_lines = if text.trim().is_empty() {
                    1.0
                } else {
                    quire_layout::break_lines(text, inner_w, &m).len().max(1) as f32
                };
                h += n_lines * pitch;
            }
            // 段邊框（K12 txbx_borders 為真相：width_pt 已是 sz/8，space_pt
            // 另計）：top＋bottom 各一条（K18 §3 藍線項 2×(3+8)）。
            for b in &im.txbx_borders {
                if b.edge == "top" || b.edge == "bottom" {
                    h += b.width_pt + b.space_pt.unwrap_or(0.0);
                }
            }
            h
        };
        let grow = let_pos_v + eff_h;
        if !(grow.is_finite() && grow > 0.0) || grow > 2000.0 {
            continue; // 爆量防護（S1-4 同口徑）：病態值不撐列。
        }
        // 宿主列定位：錨定段落所屬**直接儲存格**所在列（巢狀表段落的宿主＝
        // 最內層格，遞迴下鑽）。
        let Some((row_idx, col_idx, host_cell)) = k17_host_cell(table, im.para_index) else {
            continue;
        };
        // R32：wrapSquare 文字框把宿主格首行文字推到框底之下（LO 口徑：
        // 起點＝ceil((posV+eff_h)/line_h)×line_h）。只認「框頂仍壓到首行帶」
        // （posV < 1.2×line_h）且「框寬壓滿宿主格寬」（≥85%，如 WC048 框寬 185.9pt
        // 壓過格寬 150.25pt）；若格寬充裕（如 WC049 格寬 450pt 框寬 185.9pt），
        // LO 走兩側環繞（wrapText="bothSides"），不推下首行。Textbox2 的
        // wrapTopAndBottom 與 Textbox3 群組 posV=21.05pt（首行上方、LO 不推）皆排除。
        let wrap_square = matches!(im.wrap_type.as_deref(), None | Some("square"));
        // R32：推移量化用的是宿主段的實際行高（`line_hs`，發射端口徑），
        // 非全域 params.line_h_pt（WC048 的 params 預設行高遠小於 cell 段
        // 實際行高，用錯口徑會把「框頂壓到首行」的資格判成不成立）。
        let cell_lh = para_line_hs
            .get(im.para_index)
            .copied()
            .filter(|v| *v > 0.0 && v.is_finite())
            .unwrap_or(line_h);
        let cell_w = host_cell
            .props
            .width
            .as_ref()
            .and_then(|tw| twip_pt(tw.w.as_deref()))
            .or_else(|| table.grid.get(col_idx).and_then(|gc| gc.w_pt));
        let covers_cell_w = cell_w.is_some_and(|w| im.w_pt >= w * 0.85);
        if wrap_square && covers_cell_w && let_pos_v < cell_lh * 1.2 {
            let off = ((let_pos_v + eff_h) / cell_lh).ceil() * cell_lh;
            if off.is_finite() && off > 0.0 && off < 2000.0 {
                let t = wrap_tops.entry(im.para_index).or_insert(0.0);
                *t = t.max(off);
            }
        }
        let e = out.entry(row_idx).or_insert(0.0);
        *e = e.max(grow);
    }
    out
}

/// 宿主儲存格定位：段落 index 落在本表（含巢狀）哪個直接儲存格 → (列號, 欄號, &Cell)。
/// 巢狀表內段落＝宿主在最內層表（其表高另有算法，撐**外層**列無據＝None）。
fn k17_host_cell(table: &Table, para_index: usize) -> Option<(usize, usize, &TableCell)> {
    for (ri, row) in table.rows.iter().enumerate() {
        for (ci, cell) in row.cells.iter().enumerate() {
            for b in &cell.blocks {
                match b {
                    Block::Paragraph { index } if *index == para_index => {
                        return Some((ri, ci, cell))
                    }
                    Block::Table(inner) => {
                        // 最內層命中＝回 None：撐外層列無 LO 校準依據。
                        let _ = inner;
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// E1（ECMA-376 §17.3.1.33）：消費正文段落 `w:spacing` 之 `w:beforeAutospacing` 與 `w:afterAutospacing`。
/// 當屬性為 `"1"` 或 `"true"` 時，以 HTML 自動段距 `280` twip（`14.0` pt）覆寫該段前／段後距，並對頁首首段上距及相鄰自動段距執行 HTML 邊距折疊。
pub fn apply_html_autospacing(doc_xml: Option<&[u8]>, spacings: &mut [(Option<f32>, Option<f32>)]) {
    let Some(raw) = doc_xml else {
        return;
    };
    let xml = String::from_utf8_lossy(raw);
    if !xml.contains("Autospacing=\"1\"") && !xml.contains("Autospacing=\"true\"") {
        return;
    }
    let mut fb_depth: usize = 0;
    let mut txbx_depth: usize = 0;
    let mut tbl_depth: usize = 0;
    let mut cur_para: Option<usize> = None;
    let mut flags: Vec<(bool, bool, bool)> = Vec::new();
    let mut rest = xml.as_ref();
    while let Some(lt) = rest.find('<') {
        let after_lt = &rest[lt + 1..];
        let Some(gt) = after_lt.find('>') else {
            break;
        };
        let seg = after_lt[..gt].trim();
        rest = &after_lt[gt + 1..];
        if seg.is_empty() || seg.starts_with('?') || seg.starts_with('!') {
            continue;
        }
        let is_end = seg.starts_with('/');
        let is_empty = seg.ends_with('/');
        let core = seg.trim_start_matches('/').trim_end_matches('/').trim();
        let tag_name = core.split_ascii_whitespace().next().unwrap_or("");
        let local = tag_name.rsplit(':').next().unwrap_or(tag_name);
        if local == "Fallback" {
            if is_end {
                fb_depth = fb_depth.saturating_sub(1);
            } else if !is_empty {
                fb_depth += 1;
            }
            continue;
        }
        if fb_depth > 0 {
            continue;
        }
        if local == "txbxContent" {
            if is_end {
                txbx_depth = txbx_depth.saturating_sub(1);
            } else if !is_empty {
                txbx_depth += 1;
            }
            continue;
        }
        if txbx_depth > 0 {
            continue;
        }
        match local {
            "tbl" => {
                if is_end {
                    tbl_depth = tbl_depth.saturating_sub(1);
                } else if !is_empty {
                    tbl_depth += 1;
                }
            }
            "p" => {
                if is_end {
                    cur_para = None;
                } else if !is_empty {
                    cur_para = Some(flags.len());
                    flags.push((false, false, tbl_depth > 0));
                }
            }
            "spacing" => {
                if let Some(pi) = cur_para {
                    if matches!(
                        attr(seg, "w:beforeAutospacing").as_deref(),
                        Some("1" | "true")
                    ) {
                        flags[pi].0 = true;
                    }
                    if matches!(
                        attr(seg, "w:afterAutospacing").as_deref(),
                        Some("1" | "true")
                    ) {
                        flags[pi].1 = true;
                    }
                }
            }
            _ => {}
        }
    }
    for (i, &(before_auto, after_auto, in_tbl)) in flags.iter().enumerate() {
        if in_tbl || i >= spacings.len() {
            continue;
        }
        if before_auto {
            let prev_after_auto = i > 0 && flags.get(i - 1).is_some_and(|f| !f.2 && f.1);
            spacings[i].0 = Some(if i == 0 || prev_after_auto { 0.0 } else { 14.0 });
        }
        if after_auto {
            spacings[i].1 = Some(14.0);
        }
    }
}

/// E1-B：把框內**段落級** `w:dataBinding` 綁定的**顯示文字**換成外部資料源真值。
///
/// 協定（ECMA-376 §17.5.2.10，實據見 `quire_ooxml::databind` 檔頭）：`w:sdtContent`
/// 的字面文字是 Word 存檔時寫入的**佔位**，LO 顯示時以 `w:xpath` 指向的資料源值
/// **覆寫**後才排版。覆寫極性由 LO 實測定案（`scratch_d9b/e1b_lo_truth_14.txt`，
/// 14 檔 LO 25.8 實跑，零反例）：
///   * 求值成功 ⇒ 覆寫，**`w:showingPlcHdr` 不豁免**（dkvert `[Company name]`
///     plc=true → LO 實繪 `Quire`）；
///   * 求值失敗（不認識的命名空間／part 缺字／值為空）⇒ 保留佔位原文
///     （dkvert `[Date]`、mce-nested `[Year]`／`[Pick the date]` LO 原樣保留）；
///   * 空段不覆寫（fdo78957 框內 sdt 外的空段，LO 不繪製，也絕不能被憑空覆寫）。
///
/// 取代 E1-2 的字面停損臂（`suppress_cover_page_author_placeholder`）：那臂只對
/// CoverPage 的 `Eric White` 做字面清空＝「少發」，本函式按真值覆寫＝「發對」。
/// 宿主段落 `txbx_texts`／`txbx_runs` 同切片一起換（`txbx_runs_for` 按文字切片
/// 配對取樣，兩側不同步會錯位退化成無樣式發射）；綁定段的佔位 run 是多個時
/// 收斂成單 run＋真值（LO 把整個控件內容繪成一條帶控件 rPr 的文字，佔位 run 的
/// 逐 run 樣式一併被取代）。
pub fn apply_txbx_databinding(
    parts: &std::collections::BTreeMap<String, Vec<u8>>,
    images: &mut [quire_ooxml::InlineImage],
    paras: &mut [quire_ooxml::Paragraph],
) {
    for im in images.iter_mut() {
        if im.txbx_sdt_bindings.is_empty() {
            continue;
        }
        let n = im.txbx_texts.len().min(im.txbx_sdt_bindings.len());
        for ti in 0..n {
            let Some(b) = im.txbx_sdt_bindings[ti].clone() else {
                continue;
            };
            let lit = im.txbx_texts[ti].clone();
            if lit.trim().is_empty() {
                continue; // 空段：LO 不繪製（不憑空覆寫）
            }
            let Some(value) = quire_ooxml::resolve_binding(parts, &b) else {
                continue; // 求值失敗：保留佔位原文（誠實退化）
            };
            if value.trim().is_empty() || value == lit {
                continue; // 同值＝零變化（絕大多數 bound 段在此早退）
            }
            im.txbx_texts[ti] = value.clone();
            // 宿主段落同步：本框切片＝[txbx_start, txbx_start+len)，逐位對照字面
            // （trim 比對；對不上＝游標錯位，老實不動，絕不亂改別框的字）。
            let Some(p) = paras.get_mut(im.para_index) else {
                continue;
            };
            let gi = im.txbx_start + ti;
            let Some(pt) = p.txbx_texts.get_mut(gi) else {
                continue;
            };
            if pt.trim() != lit.trim() {
                continue;
            }
            *pt = value.clone();
            if let Some(grp) = p.txbx_runs.get_mut(gi) {
                if !grp.is_empty() {
                    grp.truncate(1);
                    grp[0].text = value.clone();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 測試
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use quire_ooxml::{parse, InlineImage};

    /// 最小 InlineImage（只填測試有意義的欄位）。
    fn img(para: usize, h_pt: f32) -> InlineImage {
        InlineImage {
            para_index: para,
            w_pt: 100.0,
            h_pt,
            floating: false,
            in_cell: false,
            pos_x_pt: None,
            pos_x_rel: None,
            pos_x_align: None,
            pos_x_pct: None,
            pos_y_pt: None,
            pos_y_rel: None,
            pos_y_align: None,
            pos_y_pct: None,
            // K25-B：手刻最小候選＝非釘定群組，子件清單恆空（serde 不外露）。
            vml_group_children: Vec::new(),
            overlap: None,
            embed_rid: None,
            embed_rids: Vec::new(),
            txbx_texts: Vec::new(),
            txbx_start: 0,
            txbx_children: Vec::new(),
            // E1-B 新增欄位（PM 補件：手刻最小候選＝無 SDT 綁定）。
            txbx_sdt_bindings: Vec::new(),
            txbx_align: None,
            // E2c 新增欄位（全部老實缺省＝舊行為；本測試只吃幾何）。
            size_rel_w_pct: None,
            size_rel_w_rel: None,
            size_rel_h_pct: None,
            size_rel_h_rel: None,
            txbx_databinding: None,
            body_anchor: None,
            // K4c 新增欄位（全部老實缺省＝舊行為；本測試只吃幾何）。
            fill_argb: None,
            filled: None,
            stroke_argb: None,
            stroked: None,
            stroke_width_pt: None,
            stroke_dash: None,
            txbx_rect: None,
            txbx_inset: None,
            txbx_vertical: false,
            txbx_vert: None,
            txbx_table: Vec::new(),
            page_pinned: false,
            rel_height: 0,
            wps_annotation: false,
            layout_in_cell: false,
            // K17：撐列事實——本測只吃幾何，老實缺省＝不撐列。
            wrap_type: None,
            textbox_grow: None,
            group_shapes: Vec::new(),
            // R35：框內內嵌圖獨立欄位——本測只吃幾何，老實缺省。
            txbx_drawings: Vec::new(),
            txbx_borders: Vec::new(),
            duotone: None,
            txbx_drawing_extent: None,
            // F2 新增欄位（prst／warp／flip）——本測只吃幾何，老實缺省＝舊行為。
            geom_prst: None,
            warp_prst: None,
            flip_h: false,
            flip_v: false,
        }
    }

    /// K15：layoutInCell=1＋宿主儲存格 → page 系軸改以格為基準；無宿主退頁原點；
    /// 旗標關＝K10 舊行為（page 原點，格資訊被忽略）。
    #[test]
    fn layout_in_cell_overrides_page_origin() {
        let cell = Some((200.0, 300.0, 150.0, 72.0));
        let mut im = anchored_img();
        im.pos_x_pt = Some(10.0);
        im.layout_in_cell = true;
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, cell).expect("可解");
        assert_eq!((a.x_pt, a.y_pt), (210.0, 310.0), "page 系兩軸以格為基準");
        assert_eq!(a.x_frame, quire_layout::OriginFrame::Page);
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, None).expect("可解");
        assert_eq!((a.x_pt, a.y_pt), (10.0, 10.0), "無宿主格＝退頁原點");
        im.layout_in_cell = false;
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, cell).expect("可解");
        assert_eq!((a.x_pt, a.y_pt), (10.0, 10.0), "旗標關＝K10 舊行為");
    }

    /// K15：align 式夾表左緣（right：算得負值）；中欄置中左端在表內不夾；
    /// posOffset 明示負偏移不夾；pct 基準維持 page 高。
    #[test]
    fn layout_in_cell_align_clamps_to_table_left_only() {
        let mut im = anchored_img();
        im.layout_in_cell = true;
        im.w_pt = 387.0;
        im.pos_x_align = Some("right".into());
        // col0：格 x=72 w=150，table_left=72 → 72+150-387 < 72 → 夾 72。
        let c0 = Some((72.0, 100.0, 150.0, 72.0));
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, c0).expect("可解");
        assert_eq!(a.x_pt, 72.0);
        // 中欄 center：格 x=222 w=150、框寬 273.6 → 222+(150-273.6)/2=160.2 > 表左 72 → 不夾。
        im.w_pt = 273.6;
        im.pos_x_align = Some("center".into());
        let c1 = Some((222.0, 100.0, 150.0, 72.0));
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, c1).expect("可解");
        assert!((a.x_pt - 160.2).abs() < 0.01, "x={}", a.x_pt);
        // posOffset 明示負偏移＝作者意圖，不夾。
        im.pos_x_align = None;
        im.pos_x_pt = Some(-50.0);
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, c0).expect("可解");
        assert_eq!(a.x_pt, 22.0);
        // pct（page 系）：基準＝頁高 800×2.5%＝20（非版心高 700×2.5%）。
        im.pos_y_pt = None;
        im.pos_y_pct = Some(2500.0);
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, c0).expect("可解");
        assert_eq!(a.y_pt, 120.0);
    }

    /// 帶兩軸 page 原點的浮圖；y 已由 posOffset 鎖定，測試只折騰 x 軸。
    fn anchored_img() -> InlineImage {
        let mut im = img(0, 50.0);
        im.floating = true;
        im.pos_x_rel = Some("page".into());
        im.pos_y_rel = Some("page".into());
        im.pos_y_pt = Some(10.0);
        im
    }

    /// S2（t_1a79f1c5 後續）：align 值不被識別時只算該式落空，必須平滑退到
    /// 第三式 pct；舊 `?` 會讓整個錨點回 None（浮圖誤退化 inline 撐帶）。
    /// `inside` 已於本卡補入白名單，故用仍不識別的 `insideLeft` 觸發同一路徑。
    #[test]
    fn unrecognized_align_falls_back_to_pct() {
        let mut im = anchored_img();
        im.pos_x_align = Some("insideLeft".into());
        im.pos_x_pct = Some(50_000.0);
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, None)
            .expect("align 落空後應退到 pct，不得整體回 None");
        assert!((a.x_pt - 300.0).abs() < 1e-4, "pct 半頁 x={}", a.x_pt);
    }

    /// 回歸保護：`left` 仍走 align 算式得 0，不受退檔重構影響。
    #[test]
    fn left_align_computes_zero_x() {
        let mut im = anchored_img();
        im.pos_x_align = Some("left".into());
        let a = resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, None).expect("left 應可解");
        assert_eq!(a.x_pt, 0.0);
    }

    /// S3：第三方工具輸出首字母大寫 `<wp:align>Center</wp:align>` 仍須命中
    /// center 算式（drawing 儲存正規化＋此處比對轉小寫雙保險）。
    #[test]
    fn uppercase_center_align_matches_center_formula() {
        let mut im = anchored_img();
        im.pos_x_align = Some("Center".into());
        let a =
            resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, None).expect("Center 應可解");
        assert!((a.x_pt - 250.0).abs() < 1e-4, "置中 x={}", a.x_pt);
    }

    /// S2：ECMA-376 `inside`／`outside` 裝訂枚舉以 frame 邊界近似
    /// （inside≈left、outside≈right，無奇偶頁資訊）。
    #[test]
    fn inside_outside_align_approximate_frame_edges() {
        let mut im = anchored_img();
        im.pos_x_align = Some("inside".into());
        assert_eq!(
            resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, None)
                .expect("inside 應可解")
                .x_pt,
            0.0
        );
        im.pos_x_align = Some("outside".into());
        assert_eq!(
            resolved_anchor(&im, 600.0, 800.0, 500.0, 700.0, None, None)
                .expect("outside 應可解")
                .x_pt,
            500.0
        );
    }

    /// 回歸保護：mce-nested（Fallback 雙份＋align/pct 鎖點）仍收斂 1 頁。
    #[test]
    fn mce_nested_corpus_stays_one_page() {
        let bytes = corpus_bytes("ooxmlexport/mce-nested.docx");
        let engine = crate::Engine::from_docx_bytes(&bytes).expect("開啟 mce-nested.docx");
        assert_eq!(engine.page_count(), 1, "align/pct 三式退檔後仍應為 1 頁");
    }

    /// S1-4（t_c1fb4c69）engine 側同閘：正文流圖帶高 `(h/line_h).ceil() as u32`
    /// 遇病態 extent（f32::MAX→天量帶數／inf）必鉗到 quire_layout 的帶量化
    /// 上限——源頭 ooxml 值域拒絕後，這是投影層的第二道閘。
    #[test]
    fn build_images_clamps_giant_extent_bands() {
        let params = quire_layout::LayoutParams::default();
        let out = build_images(
            &[img(0, f32::MAX), img(1, f32::INFINITY)],
            16.8,
            &params,
            &[],
            &[],
            &[],
        );
        assert_eq!(out.len(), 2);
        for b in &out {
            assert!(
                b.bands <= quire_layout::MAX_PAGE_BANDS && b.bands >= 1,
                "巨 extent 帶數必鉗上限：{}",
                b.bands
            );
        }
    }

    fn corpus_bytes(rel: &str) -> Vec<u8> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        std::fs::read(root.join(rel)).unwrap_or_else(|e| panic!("讀 {rel}: {e}"))
    }

    /// R35 卡 7（VML 框內嵌圖誤觸 K17 撐列臂）邊界鎖。
    ///
    /// 誠實註記（實掃定案）：VML 候補 `layout_in_cell` 恆 false（Start 臂
    /// `Pending::new` 是唯一設值點，VML 路徑永不置真）⇒ corpus 實際到不了
    /// 本臂；本燈是**前瞻邊界鎖**——若日後 VML 獲得 in-cell 形態，挪用態
    /// （R31：內嵌圖塞 `group_shapes.stroke_dash`）必須仍被識別為撐列誤觸
    /// （紅態重現），獨立欄位態（R35：`txbx_drawings`）必須不撐列（綠態）。
    /// 與卡 6 同根：獨立欄位即解，本燈鎖住兩態分界。
    #[test]
    fn r35_embedded_drawings_do_not_grow_rows_via_group_shapes_arm() {
        use quire_ooxml::{Block, GridCol, InlineChildShape, Table, TableCell, TableRow};
        // 單列單格表，格內段落 index=1（宿主段落）。
        let table = Table {
            props: Default::default(),
            grid: vec![GridCol {
                w_pt: Some(200.0),
                raw: None,
            }],
            rows: vec![TableRow {
                props: Default::default(),
                cells: vec![TableCell {
                    props: Default::default(),
                    blocks: vec![Block::Paragraph { index: 1 }],
                }],
            }],
        };
        let params = quire_layout::LayoutParams::default();
        let mut wrap_tops = HashMap::new();
        let mut floors = |images: &[quire_ooxml::InlineImage]| {
            k17_grow_floors(&table, images, &params, &[], None, &[], &mut wrap_tops)
        };
        // 資格齊備的 VML 候選（floating＋layoutInCell——後者 corpus 恆 false，
        // 本測手工點燃以觸達 K17 臂）。
        let mut host = img(1, 60.0);
        host.floating = true;
        host.layout_in_cell = true;
        host.pos_y_pt = Some(0.0);
        // 綠態（R35 口徑）：內嵌圖在獨立欄位 ⇒ 群組臂不觸發 ⇒ 不撐列。
        let mut fixed = host.clone();
        fixed.txbx_drawings = vec![quire_ooxml::TxbxDrawing {
            para_ix: 0,
            rid: "rIdX".into(),
            extent_pt: [40.0, 30.0],
        }];
        assert!(
            floors(&[fixed]).is_empty(),
            "獨立欄位態不得誤判 wpg:wgp 群組硬撐列高（卡 7 根治面）"
        );
        // 紅態（R31 挪用形態重現）：內嵌圖塞進 group_shapes.stroke_dash ⇒
        // `None if !group_shapes.is_empty()` 臂照舊撐列（60pt）——證明本燈
        // 真有鑑別力，非恆綠假鎖。
        let mut misused = host.clone();
        misused.group_shapes = vec![InlineChildShape {
            dx_pt: 0.0,
            dy_pt: 0.0,
            w_pt: 40.0,
            h_pt: 30.0,
            fill_argb: None,
            filled: None,
            stroke_argb: None,
            stroked: None,
            stroke_width_pt: None,
            stroke_dash: Some("rIdX".into()),
            txbx_texts: Vec::new(),
            // F2：挪用欄位＝非群組形状，無 prst 事實。
            geom_prst: None,
            flip_h: false,
            flip_v: false,
        }];
        let f = floors(&[misused]);
        assert_eq!(
            f.get(&0).copied(),
            Some(60.0),
            "挪用態必須仍被群組臂撐列（鑑別力證據；R35 後此態自然不再產生）"
        );
    }

    /// 口徑統一回歸（t_580c37ac）：含巢狀 `w:p`（txbxContent）的 corpus 檔，
    /// `build_slots` 的槽區間必須直接合法（不重疊、≤ 段落數）——舊
    /// 「可見段落最小索引」重錨 workaround 已隨三解析器口徑統一移除，
    /// 錨點不再漂移。WC048／WC049／shape-in-floattable 為 txbx-in-cell
    /// 實據檔（舊註 WC048 實證）。
    #[test]
    fn txbx_corpus_slots_anchor_without_rework() {
        for rel in [
            "xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx",
            "xopt/TestFiles_WC_WC049-Text-Box-in-Cell.docx",
            "ooxmlexport/shape-in-floattable.docx",
            "xopt/TestFiles_HC031-Complicated-Document.docx",
        ] {
            let doc = parse(&corpus_bytes(rel)).expect("parse");
            let texts = doc.paragraph_texts();
            let params = quire_layout::LayoutParams::default();
            let styles = doc.parts.get("word/styles.xml").map(|s| s.as_slice());
            let slots = build_slots(
                &doc.tables,
                &texts,
                styles,
                &params,
                &[],
                &[],
                &doc.paragraphs,
                &[],
                1,
                &[],
                // K8-shd：本測只鎖槽錨點口徑——條件底紋傳 None＝零擾動路徑。
                None,
                // K17：images 恆空＝撐列不生效，doc_styles 老實傳 None。
                None,
            );
            assert!(!slots.is_empty(), "{rel}：標為 txbx-in-cell 檔應有表格槽");
            let mut prev_end = 0usize;
            for s in &slots {
                assert!(
                    s.para_start >= prev_end
                        && s.para_end <= texts.len()
                        && s.para_end >= s.para_start,
                    "{rel}：槽區間 [{}，{}) 違反 build_flow 契約（prev_end={prev_end}, n={})",
                    s.para_start,
                    s.para_end,
                    texts.len()
                );
                prev_end = s.para_end;
            }
            // 錨點即解析器計數（無重錨）：每個槽的 para_start 必須與首個
            // 可見段落索引一致或由空表段前錨點自然給出——以不重疊＋不越界
            // 為硬契約（上面已驗），此處再鎖「槽起點不晚於首可見段落＋1」
            // 防錨點向後漂移吞正文段。
            for (slot, bt) in slots.iter().zip(&doc.tables) {
                let mut min_vis: Option<usize> = None;
                bt.walk(&mut |sub| {
                    for row in &sub.rows {
                        for cell in &row.cells {
                            for b in &cell.blocks {
                                if let Block::Paragraph { index } = b {
                                    min_vis = Some(min_vis.map_or(*index, |v| v.min(*index)));
                                }
                            }
                        }
                    }
                });
                if let Some(v) = min_vis {
                    assert!(
                        slot.para_start <= v && v < slot.para_end.max(slot.para_start + 1),
                        "{rel}：槽 [{}, {}) 未涵蓋首可見段落 {v}（錨點漂移？）",
                        slot.para_start,
                        slot.para_end
                    );
                }
            }
        }
    }

    /// K7c-3：`w:w="0" w:type="dxa"`（歷史 auto 變體）與負值一律 None＝
    /// 退回自然寬——不再誤判 `Some(Pt(0))` 走壓版心路徑＋失去 K7 均分資格。
    #[test]
    fn k7c_nonpositive_dxa_width_is_none() {
        let zero = TblWidth {
            w: Some("0".into()),
            typ: Some("dxa".into()),
        };
        assert_eq!(width_spec(&zero), None, "w:w=0 dxa 應視同 auto→None");
        let neg = TblWidth {
            w: Some("-120".into()),
            typ: Some("dxa".into()),
        };
        assert_eq!(width_spec(&neg), None, "負寬無版面意義→None");
        let pct0 = TblWidth {
            w: Some("0".into()),
            typ: Some("pct".into()),
        };
        assert_eq!(width_spec(&pct0), None, "pct 0% 亦 None");
        // 回歸保護：正值不受影響。
        let ok = TblWidth {
            w: Some("200".into()),
            typ: Some("dxa".into()),
        };
        assert_eq!(width_spec(&ok), Some(WidthSpec::Pt(10.0)));
    }
}
