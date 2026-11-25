//! U4m（t_f073cd6c）紅燈考卷：`caret_at` 行框歸屬改**行距中線**——基線以上
//! 點擊不得錯判上一段（用戶「無法任意按 Enter 創建新段落」的直接成因）。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 用戶實測缺口（CDP 實錄，127.0.0.1:8791 示範檔，12pt／行距 1.4／pitch 16.8pt）
//! para5 首行基線 `y_pt=408`（display list `Text.y` 同值；上行 391.2、下行 424.8）：
//!   dy = -14 .. -2  → para 4 / cp 290（**上一段段尾**）
//!   dy =   0 .. +8  → para 5 / cp 24 （正確）
//! 亦即該行基線以上約 8.6pt（字級 ascent 全區，視覺上確實是那一行的字）點擊全部
//! 錯判成前一行。點擊錯判 → caret 錯段 → Enter 分錯段（矩陣實測：點 para5 段中
//! `grew=1`，但新建段落落在 para4/5 之間）。
//!
//! # 根因（core/layout/src/lib.rs `caret_at` 的 U4f 跨度判制）
//! 兩處疊加：
//!   1. **先 floor 再判帶**：`(y−margin_top)/line_h` 立刻 `.floor()`，把「基線以上
//!      8pt」與「基線以下 8pt」壓成同一個整數帶——粗帶尺（每行 1 帶，即用戶實錄
//!      的 12pt／行距 1.4 常態）下任何垂直判制都不可能發火。
//!   2. **跨度自基線起算**：發射口的行 y ＝ `margin_top ＋ band × line_h` 是**基線帶**
//!      （display list `Text.y` 同值、SVG 基線），反向腿的行跨度卻是
//!      `[起帶, 起帶＋bands_of(para))`——一路吞到下一行的基線。相鄰兩行的跨度邊界
//!      正好切在下一行的基線上 ⇒ 下一行的 ascent 區（視覺上就是那一行的字）被
//!      上一行吃掉。`caret_rect` 正向腿給的 y 恰在邊界上，U4i 的 `band_eps=1e-3`
//!      只救「恰邊界的浮點噪聲」，救不了「基線以上 2pt」這種真實點擊位置。
//!
//! # 修法（PM 定案；不造第二把尺）
//! 反向腿改用**小數帶商**比對**行距中線**切分的中介帶：一條行的命中區間
//!   lo ＝（同欄前行基線帶 ＋ 本行基線帶）／2，欄內首行退 `band − span/2`
//!   hi ＝（本行基線帶 ＋ 同欄後行基線帶）／2，欄內末行退 `band ＋ span`
//! 尺源仍是 `page.line_bands` ＋ `ParaGrid::bands_of`（正向腿同一來源）；命中面
//! **只增不減**（U4f 死區教訓：末行下緣沿用 `band＋span`＝修前覆蓋）；表格切片／
//! 流內圖的佔用帶區間一律夾住 lo/hi ⇒ 既有「表格帶必 None」考卷零擾動。
//! 整數帶 `band` 仍留給修前路徑（`U4F_CARET_AT_BUGGY` 回塞態與舊快照陣標退回）。
//!
//! # 既有考卷的歸屬口徑變更（唯一一處，PM 定案記帳）
//! `u4f_band_ruler` 燈 3(b) 舊斷言「行距帶（`b0+1 .. b0+span`）全部落回**上一行**」
//! ——那正是本卡修掉的缺陷（細帶尺 span=23 帶＝整拍行距，其中 `b0+12..b0+22` 是
//! 下一行字級 ascent 的字面位置，舊口徑把下一行的字搶給上一段）。本卡把該臂的
//! 歸屬期望改為「離哪條基線近就屬哪一行」（仍保留 U4f 的本意＝非死區斷言），
//! 並在該臂註明 t_f073cd6c。其餘 u4f 臂（燈 3(c) 正逆互逆、粗尺零擾動）零改動。
//!
//! # 鑑別力（BUGGY 回塞鎖口徑，比照 `U4F_CARET_AT_BUGGY`）
//! - `U4M_LINEBOX_BUGGY=1`：反查整支退回 U4f 修後的「小數部先丟、跨度起自基線帶」
//!   判制（修本卡前的逐位元組舊態）⇒ **燈 1／燈 2／燈 3／燈 6 必紅**（基線以上
//!   ascent 區成批改判上一行／上一段；四燈各自內建「BUGGY 態未紅＝靶失效」自證）。
//! - 燈 4（命中面不擴大）與燈 5（正向腿 y 護欄）在 BUGGY 態**恆綠**：兩燈護的是
//!   「不得吞表格帶／無主空帶」與「y 腿與發射同源」，與行框歸屬正交（記帳於各燈）。
//! - 雙地形：燈 1／2／3／6 同時跑「用戶實錄態」（12pt／1.4＝粗帶尺，1 帶＝1 行）
//!   與「細帶尺態」（比照 u4f：0.69285715pt／帶、23 帶／行）——用戶抱怨發生在粗尺態，
//!   只測細尺態會假綠。

use quire_layout::table::{CellBlock, CellInput, RowInput, TableInput};
use quire_layout::{CaretPos, LayoutEngine, LayoutParams, ParaGrid, TableSlot, TextMeasurer};

fn buggy() -> bool {
    std::env::var_os("U4M_LINEBOX_BUGGY").is_some()
}

/// 版面地形（兩態同考）。
#[derive(Clone, Copy)]
struct Terrain {
    name: &'static str,
    fs: f32,
    ls: f32,
    /// 逐段每行帶數（`ParaGrid::bands`；1＝粗帶尺＝用戶實錄態）。
    bands: u32,
}

/// 用戶實錄態：12pt／行距 1.4 ⇒ 行距 16.8pt、帶尺 16.8pt（每行恰 1 帶）。
const USER: Terrain = Terrain {
    name: "用戶實錄態(12pt/1.4/粗帶尺)",
    fs: 12.0,
    ls: 1.4,
    bands: 1,
};
/// 細帶尺態（比照 u4f：0.69285715pt／帶、每行 23 帶）。
const FINE: Terrain = Terrain {
    name: "細帶尺態(0.693pt/帶/23帶每行)",
    fs: 11.0,
    ls: 0.062987015,
    bands: 23,
};
const TERRAINS: [Terrain; 2] = [USER, FINE];

/// 用戶實錄的字級 ascent（12pt 級約 8.6pt）：掃 y 的上緣取此值。
const ASCENT_PT: f32 = 8.0;

fn params_of(t: Terrain) -> LayoutParams {
    LayoutParams {
        page_w_pt: 612.0,
        page_h_pt: 792.0,
        margins_pt: [72.0; 4],
        font_size_pt: t.fs,
        line_spacing: t.ls,
        footnote_reserve_pt: 0.0,
        footnote_reserves_pt: Vec::new(),
        endnote_reserves_pt: Vec::new(),
        header_pushes_pt: Vec::new(),
    }
}

fn grid_of(t: Terrain, n: usize) -> ParaGrid {
    ParaGrid {
        size_pt: Vec::new(),
        bands: vec![t.bands; n],
        break_before: Vec::new(),
        before_bands: Vec::new(),
        after_bands: Vec::new(),
        advance_scale: Vec::new(),
        punct_arm: Vec::new(),
    }
}

fn engine_of(t: Terrain, texts: &[String]) -> LayoutEngine {
    let n = texts.len();
    LayoutEngine::with_breaks_grid(
        texts,
        params_of(t),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        grid_of(t, n),
    )
}

/// 正文行框幾何（**全部取自引擎自身印表**：`line_bands`／`Line::col`／`ParaGrid`）。
#[derive(Clone, Copy)]
struct Box0 {
    page: usize,
    /// 該頁 `lines` 的陣標。
    li: usize,
    para: usize,
    col: u8,
    /// 起帶＝**基線帶**（發射 y ＝ margin_top ＋ band × line_h）。
    band: u32,
    /// 該段每行帶數。
    span: u32,
}

/// 掃出全文件正文行框（每頁每行一條，順序＝引擎印表序）。
fn body_boxes(eng: &LayoutEngine, t: Terrain) -> Vec<Box0> {
    let mut out = Vec::new();
    for pg in eng.pages() {
        for (li, l) in pg.lines.iter().enumerate() {
            out.push(Box0 {
                page: pg.index,
                li,
                para: l.para_index,
                col: l.col,
                band: pg.line_bands.get(li).copied().unwrap_or(li as u32),
                span: t.bands,
            });
        }
    }
    out
}

/// 行框歸屬的**考卷端獨立實作**（卡面修法書面語義；吃引擎印出的 `line_bands`／
/// `Line::col`，不吃被測反查碼）——兩側不同調 ⇒ 本檔紅。
///
/// `bd` ＝ 點擊的**小數**帶商（`(y − margin_top)／帶尺`，未 floor）。
///   lo ＝（同欄前行基線帶 ＋ 本行基線帶）／2，欄內首行退 `band − span/2`
///   hi ＝（本行基線帶 ＋ 同欄後行基線帶）／2，欄內末行退 `band ＋ span`
fn slot_owner(boxes: &[Box0], page: usize, bd: f32) -> Option<usize> {
    let idx: Vec<usize> = (0..boxes.len())
        .filter(|&i| boxes[i].page == page)
        .collect();
    for (j, &i) in idx.iter().enumerate() {
        let prev = if j > 0 {
            // 同欄前行：向前找第一條同欄者（跨欄帶序各自重來，不互為鄰）。
            (0..j)
                .rev()
                .find(|&k| boxes[idx[k]].col == boxes[i].col)
                .map(|k| boxes[idx[k]].band as f32)
        } else {
            None
        };
        let next = (j + 1..idx.len())
            .find(|&k| boxes[idx[k]].col == boxes[i].col)
            .map(|k| boxes[idx[k]].band as f32);
        let b = boxes[i].band as f32;
        let span = boxes[i].span as f32;
        let lo = prev.map_or(b - span / 2.0, |p| (p + b) / 2.0);
        let hi = next.map_or(b + span, |n| (b + n) / 2.0);
        if bd >= lo && bd < hi {
            return Some(boxes[i].para);
        }
    }
    None
}

/// 用戶形態 fixture：n 段、每段多行（頁內必然出現「上一行屬別段」的靶邊界）。
fn multi_para_texts(n: usize, rep: usize) -> Vec<String> {
    (0..n)
        .map(|i| format!("第{i}段：{}內容", "中文文字內容文字".repeat(rep)))
        .collect()
}

/// 帶尺（pt／帶）。
fn line_h(t: Terrain) -> f32 {
    t.fs * t.ls
}

/// 靶：某頁某行的基線 y（pt）。
fn baseline_y(margin_top: f32, band: u32, lh: f32) -> f32 {
    margin_top + band as f32 * lh
}

// ---------------------------------------------------------------------------
// 燈 0（前提自證）：兩種地形都必須真的-multi-line、且「基線以上 8pt」的點擊帶
// 落在**上一行的舊跨度**內（＝缺陷發生的地形）。地形失效直接紅，不給假綠。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp0_fixture_terrain_is_the_defect_ground() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(8, 8);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);
        let pg0: Vec<&Box0> = boxes.iter().filter(|b| b.page == 0).collect();
        assert!(
            pg0.len() > texts.len(),
            "{}：每段必須折多行（頁0 僅 {} 行／{} 段）",
            t.name,
            pg0.len(),
            texts.len()
        );
        assert!(
            pg0.windows(2).any(|w| w[0].para != w[1].para),
            "{}：靶缺失——頁0 無跨段行邊界",
            t.name
        );
        assert!(
            pg0.windows(2).any(|w| w[0].para == w[1].para),
            "{}：段內必須有續行（正逆互逆與同段掃描的地形）",
            t.name
        );
        // 缺陷地形：跨段邊界行的基線以上 8pt 落點，必須在上一行舊跨度內。
        let k = pg0
            .windows(2)
            .position(|w| w[0].para != w[1].para)
            .expect("前置斷言已保證")
            + 1;
        let (prev, cur) = (pg0[k - 1], pg0[k]);
        let click_band = cur.band as f32 - ASCENT_PT / lh;
        assert!(
            click_band >= prev.band as f32 && click_band < prev.band as f32 + prev.span as f32,
            "{}：基線以上 8pt（帶 {:.2}）未落在上一行舊跨度 [{}, {})——地形非缺陷態",
            t.name,
            click_band,
            prev.band,
            prev.band + prev.span
        );
        // 且該落點必須在上一行的**基線之上**（视觉上屬本行，不該給上一段）。
        assert!(
            click_band < cur.band as f32,
            "{}：點擊帶不在本行基線以上（用戶形態不成立）",
            t.name
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 1（用戶實錄形態，鑑別燈）：跨段邊界行的基線以上 −2/−4/−6/−8pt 點擊必落
// **本行所屬段**。BUGGY 態全批紅＝用戶「無法任意按 Enter 創建新段落」。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp1_click_above_baseline_stays_in_own_paragraph() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(8, 8);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);

        let mut total = 0usize;
        let mut bad = Vec::new();
        for (i, b) in boxes.iter().enumerate() {
            let Some(k) = i.checked_sub(1) else { continue };
            let prev = boxes[k];
            if prev.page != b.page || prev.col != b.col || prev.para == b.para {
                continue;
            }
            let pg = eng.pages().get(b.page).expect("頁必存在");
            // 半拍以內（同側）必屬本行：拍節比例取樣＋用戶實錄絕對偏移（僅取
            // 仍小於半拍者；細尺態半拍 7.97pt，-8pt 已合法跨中線＝屬上一行）。
            let pitch = b.span as f32 * lh;
            let mut dys: Vec<f32> = (1..=4).map(|k| -(pitch / 2.0) * (k as f32 / 5.0)).collect();
            dys.extend(
                [-2.0f32, -4.0, -6.0, -ASCENT_PT]
                    .into_iter()
                    .filter(|d| -d * 2.0 < pitch),
            );
            for dy in dys {
                let y = baseline_y(pg.margin_top_pt, b.band, lh) + dy;
                if y < pg.margin_top_pt {
                    continue;
                }
                total += 1;
                match eng.caret_at(b.page, pg.margin_left_pt + 100.0, y) {
                    Some(c) if c.para == b.para => {}
                    other => bad.push(format!(
                        "{} 頁{} 段{}(帶{}) dy={:.2} → {:?}",
                        t.name,
                        b.page,
                        b.para,
                        b.band,
                        dy,
                        other.map(|c| (c.para, c.cp))
                    )),
                }
            }
        }
        assert!(
            total >= 40,
            "{}：跨段邊界樣本不足（實得 {}）",
            t.name,
            total
        );
        if buggy() {
            assert!(
                !bad.is_empty(),
                "燈 1 在 U4M_LINEBOX_BUGGY 態未紅＝靶地形失效（回塞鎖失去鑑別力）"
            );
        }
        assert!(
            bad.is_empty(),
            "燈 1 基線以上點擊錯判 {}／{} 例（BUGGY 態點名：錯判上一段段尾）：\n{}",
            bad.len(),
            total,
            bad.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 2（卡面考題 1，鑑別燈）：逐行掃 y（baseline−8 .. baseline+8，step 2）
// 全程必還原該中介帶所屬段——修前態在 ascent 區成批紅＝鑑別證據。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp2_vertical_scan_restores_slot_owner() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(8, 8);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);
        assert!(!boxes.is_empty(), "{}：fixture 必須有正文行", t.name);

        let mut total = 0usize;
        let mut bad = Vec::new();
        for b in &boxes {
            let pg = eng.pages().get(b.page).expect("頁必存在");
            let x = pg.margin_left_pt + 100.0;
            let pitch = b.span as f32 * lh;
            let step = pitch / 8.0;
            let mut dy = -(pitch / 2.0) + step * 0.5;
            while dy <= ASCENT_PT + 0.001 {
                let y = baseline_y(pg.margin_top_pt, b.band, lh) + dy;
                if y >= pg.margin_top_pt {
                    total += 1;
                    let want = slot_owner(&boxes, b.page, (y - pg.margin_top_pt) / lh);
                    let got = eng.caret_at(b.page, x, y).map(|c| c.para);
                    if got != want {
                        bad.push(format!(
                            "{} 頁{} 行{} 段{} 帶{} dy={:.1}：反查 {:?} ≠ 行框語義 {:?}",
                            t.name, b.page, b.li, b.para, b.band, dy, got, want
                        ));
                    }
                }
                dy += step;
            }
        }
        assert!(total >= 100, "{}：掃 y 樣本不足（實得 {}）", t.name, total);
        if buggy() {
            assert!(
                !bad.is_empty(),
                "燈 2 在 U4M_LINEBOX_BUGGY 態未紅＝靶地形失效（回塞鎖失去鑑別力）"
            );
        }
        assert!(
            bad.is_empty(),
            "燈 2 逐行掃 y 破口 {}／{} 例：\n{}",
            bad.len(),
            total,
            bad.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 3（卡面考題 2，鑑別燈）：正逆互逆——`caret_rect(p,cp)` 的 y 上下
// ±(pitch/2−1) 仍回同段；跨中線（±(pitch/2+1)）才換段（且照樣命中中介帶語義）。
// pitch ＝ 相鄰行基線帶距 × 帶尺（細尺態＝span×帶尺＝整拍行距）。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp3_inverse_symmetry_around_pitch_midline() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(8, 8);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);

        let mut same_side = 0usize;
        let mut cross_side = 0usize;
        let mut bad = Vec::new();
        for (pi, tx) in texts.iter().enumerate() {
            let n = tx.chars().count();
            for cp in [0usize, n / 2, n.saturating_sub(1)] {
                let Some(rect) = eng.caret_rect(CaretPos { para: pi, cp }) else {
                    continue;
                };
                let pg = eng.pages().get(rect.page).expect("頁必存在");
                let bd = (rect.y_pt - pg.margin_top_pt) / lh;
                let Some(b) = boxes
                    .iter()
                    .find(|x| x.page == rect.page && (x.band as f32 - bd).abs() < 0.01)
                else {
                    continue;
                };
                // 本行與同欄鄰行的基線帶距＝拍節（pitch）；末行取 span（自身行距）。
                let same_col: Vec<&Box0> = boxes
                    .iter()
                    .filter(|x| x.page == b.page && x.col == b.col)
                    .collect();
                let j = same_col
                    .iter()
                    .position(|x| x.li == b.li)
                    .expect("本行必在同欄");
                let pitch_pt = if j + 1 < same_col.len() {
                    (same_col[j + 1].band - b.band) as f32 * lh
                } else if j > 0 {
                    (b.band - same_col[j - 1].band) as f32 * lh
                } else {
                    b.span as f32 * lh
                };
                let half = pitch_pt / 2.0;
                for sgn in [-1.0f32, 1.0] {
                    // (a) 半拍以內（離中線還差 1pt）：必回同段。
                    let y = rect.y_pt + sgn * (half - 1.0);
                    if y >= pg.margin_top_pt {
                        same_side += 1;
                        match eng.caret_at(rect.page, rect.x_pt, y) {
                            Some(c) if c.para == pi => {}
                            other => bad.push(format!(
                                "{} 同側：段{} cp{} y偏移 {:+}×(pitch/2−1)={:.2} → {:?}",
                                t.name,
                                pi,
                                cp,
                                sgn,
                                half,
                                other.map(|c| (c.para, c.cp))
                            )),
                        }
                    }
                    // (b) 跨中線（越過 pitch/2 再 1pt）：必換到中介帶語義的段，
                    //     且不得回 None（中線外一線仍是行框，不是空白）。
                    let y2 = rect.y_pt + sgn * (half + 1.0);
                    if y2 >= pg.margin_top_pt {
                        cross_side += 1;
                        let want = slot_owner(&boxes, rect.page, (y2 - pg.margin_top_pt) / lh);
                        let got = eng.caret_at(rect.page, rect.x_pt, y2).map(|c| c.para);
                        if got != want {
                            bad.push(format!(
                                "{} 跨側：段{} cp{} y偏移 {:+}×(pitch/2+1) → {:?} ≠ 行框語義 {:?}",
                                t.name, pi, cp, sgn, got, want
                            ));
                        }
                    }
                }
            }
        }
        assert!(
            same_side >= 32,
            "{}：同側樣本不足（實得 {}）",
            t.name,
            same_side
        );
        assert!(
            cross_side >= 32,
            "{}：跨側樣本不足（實得 {}）",
            t.name,
            cross_side
        );
        if buggy() {
            assert!(
                !bad.is_empty(),
                "燈 3 在 U4M_LINEBOX_BUGGY 態未紅＝靶地形失效（回塞鎖失去鑑別力）"
            );
        }
        assert!(
            bad.is_empty(),
            "燈 3 中線互逆破口 {} 例：\n{}",
            bad.len(),
            bad.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 4（卡面考題 4，命中面不擴大）：表格帶／空帶仍必回 None。
// 【記帳】BUGGY 態恆綠：本燈護「新歸屬制不得吞表格帶／無主空帶」，與修前的
// 跨度判制在此同向（護欄燈，非鑑別燈）。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp4_table_and_empty_bands_stay_none() {
    for t in TERRAINS {
        let p = params_of(t);
        let lh = line_h(t);
        // 5 列 × 40pt 列高（列高远大於格內文字行高）⇒ 表格帶群大於格內行帶群。
        let input = TableInput {
            grid_pt: vec![200.0],
            rows: (0..5)
                .map(|i| RowInput {
                    cells: vec![CellInput {
                        blocks: vec![CellBlock::Text(format!("儲存格{i}"))],
                        ..Default::default()
                    }],
                    height_pt: Some(40.0),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let mut texts = vec!["表格前的正文段。".repeat(4)];
        texts.extend((0..5).map(|i| format!("儲存格{i}")));
        texts.push("表格後的正文段。".to_string());
        let m = TextMeasurer::new(p.font_size_pt);
        let tl = quire_layout::table::layout_table(&input, p.content_w_pt(), lh, &m, 0);
        let slot = TableSlot {
            para_start: 1,
            para_end: 1 + 5,
            layout: tl,
        };
        let n = texts.len();
        let eng = LayoutEngine::with_breaks_grid(
            &texts,
            p.clone(),
            Vec::new(),
            vec![slot.clone()],
            Vec::new(),
            Vec::new(),
            grid_of(t, n),
        );
        let pg = eng.pages().first().expect("fixture 必有頁");
        let tp = pg.tables.first().expect("頁內必有表格切片");
        let rows_h: f32 = tp.frag.rows.iter().map(|r| r.height_pt).sum();
        let rows_bands = (rows_h / lh).ceil() as u32;
        assert!(
            rows_bands >= 3,
            "{}：表格必須真的吃掉一批帶（實得 {}）",
            t.name,
            rows_bands
        );

        // 格內段的正向腿 y（T4AA 已產品化）：落在格內行跨度者屬格內腿，本燈不
        // 觸碰；其餘表格帶是「無正文行」的帶群——正文反查一律不得命中。
        let cell_ys: Vec<f32> = (1..=5usize)
            .filter_map(|para| eng.caret_rect(CaretPos { para, cp: 0 }).map(|r| r.y_pt))
            .collect();
        assert_eq!(
            cell_ys.len(),
            5,
            "{}：格內段正向腿必全可得（T4AA 契約）",
            t.name
        );
        let mut probed = 0usize;
        let mut leak = Vec::new();
        for k in 0..rows_bands {
            let y = pg.margin_top_pt + (tp.band + k) as f32 * lh + lh * 0.25;
            if cell_ys.iter().any(|cy| (cy - y).abs() < FINE_AWAY) {
                continue; // 格內腿自己的命中面（T4AA 考卷把關）
            }
            probed += 1;
            if let Some(c) = eng.caret_at(pg.index, pg.margin_left_pt + 100.0, y) {
                leak.push((tp.band + k, c.para));
            }
        }
        assert!(probed >= 3, "{}：表格帶樣本不足（實得 {}）", t.name, probed);
        assert!(
            leak.is_empty(),
            "{}：燈 4(a) 正文反查吞掉表格帶 {} 例（命中面擴大＝搶走格內打字）：{:?}",
            t.name,
            leak.len(),
            leak.iter().take(8).collect::<Vec<_>>()
        );

        // (b) 空帶：末行下緣以遠的頁底空白仍 None（末行下界沿用修前口徑）。
        let one = vec!["只有這一頁這一段。".repeat(4)];
        let eng1 = engine_of(t, &one);
        let pg1 = eng1.pages().first().expect("首頁必存在");
        let last = *pg1.line_bands.last().expect("首頁必有行");
        let deep = pg1.margin_top_pt + (last as f32 + t.bands as f32 + 1.5) * lh;
        assert!(
            eng1.caret_at(pg1.index, pg1.margin_left_pt + 60.0, deep)
                .is_none(),
            "{}：燈 4(b) 末行下緣以遠的頁底空白必 None（深處 {:.1}pt）",
            t.name,
            deep
        );
    }
}

/// 燈 4 判定「格內行命中面」的容差（pt）：小於半拍，避免把表格帶誤當格內行。
const FINE_AWAY: f32 = 6.0;

// ---------------------------------------------------------------------------
// 燈 5（正向腿護欄）：`caret_rect` 的 y 仍恰為某條行的基線帶換算值（發射同尺），
// 且 caret_rect→caret_at(y＋0.35×帶尺) 互逆仍還原同段（u4f 燈 3(c) 同調）。
// 【記帳】BUGGY 態恆綠：本燈護 y 腿與發射同源，與行框歸屬正交。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp5_forward_leg_y_untouched() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(6, 8);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);
        let mut checked = 0usize;
        for (pi, tx) in texts.iter().enumerate() {
            let n = tx.chars().count();
            for cp in [0usize, n / 2] {
                let Some(rect) = eng.caret_rect(CaretPos { para: pi, cp }) else {
                    continue;
                };
                let mt = eng.pages()[rect.page].margin_top_pt;
                let bd = (rect.y_pt - mt) / lh;
                assert!(
                    boxes
                        .iter()
                        .any(|b| b.page == rect.page && (b.band as f32 - bd).abs() < 0.01),
                    "{}：段{} cp{} caret y {:.3} 非任何行的基線帶換算值（y 腿被動壞？）",
                    t.name,
                    pi,
                    cp,
                    rect.y_pt
                );
                checked += 1;
                let back = eng.caret_at(rect.page, rect.x_pt, rect.y_pt + lh * 0.35);
                assert_eq!(
                    back.map(|c| c.para),
                    Some(pi),
                    "{}：段{} cp{} caret_rect→caret_at(y＋0.35 帶) 互逆必還原同段",
                    t.name,
                    pi,
                    cp
                );
            }
        }
        assert!(checked >= 8, "{}：正向樣本不足（實得 {}）", t.name, checked);
    }
}

// ---------------------------------------------------------------------------
// 燈 6（用戶可見結果，鑑別燈）：點擊段 ↔ caret 段全等——用戶「按 Enter 分錯段」
// 的根因就是 caret 段 ≠ 點擊段。另做同源對帳：caret 的 cp 必落在點擊行自身的
// 碼點區間內（正逆兩腿同一份行文字，不是憑空落到別段的段尾）。
// 修前（BUGGY）態：caret 落上一段段尾 ⇒ 段號與 cp 雙雙錯 ⇒ 本燈紅。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp6_click_paragraph_equals_caret_paragraph() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(8, 8);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);

        let mut total = 0usize;
        let mut bad = Vec::new();
        for (i, b) in boxes.iter().enumerate() {
            let Some(k) = i.checked_sub(1) else { continue };
            if boxes[k].page != b.page || boxes[k].para == b.para || boxes[k].col != b.col {
                continue;
            }
            let pg = eng.pages().get(b.page).expect("頁必存在");
            // 半拍以內（同側）的基線以上偏移：拍節比例＋用戶實錄絕對偏移（僅取
            // 仍小於半拍者）。跨中線的偏移屬上一行的正確領土，不進本燈靶。
            let pitch = b.span as f32 * lh;
            let mut dys: Vec<f32> = (1..=3).map(|k| -(pitch / 2.0) * (k as f32 / 4.0)).collect();
            dys.extend([-ASCENT_PT, -4.0].into_iter().filter(|d| -d * 2.0 < pitch));
            for dy in dys {
                let y = baseline_y(pg.margin_top_pt, b.band, lh) + dy;
                if y < pg.margin_top_pt {
                    continue;
                }
                total += 1;
                let Some(c) = eng.caret_at(b.page, pg.margin_left_pt + 100.0, y) else {
                    bad.push(format!(
                        "{} 頁{} 段{} dy={:.2}：基線以上點擊反查 None（死區＝點擊無游標）",
                        t.name, b.page, b.para, dy
                    ));
                    continue;
                };
                if c.para != b.para {
                    bad.push(format!(
                        "{} 頁{} 帶{} dy={:.2}：caret 落段{}（點擊段 {}）⇒ Enter 把新段落切到兩段之間",
                        t.name, b.page, b.band, dy, c.para, b.para
                    ));
                    continue;
                }
                // 同源對帳：cp 必落在點擊行的碼點區間 [行起, 行起＋行長]（段內
                // 偏移由前行累計而来；錯判上一段段尾的舊態在此同樣露餡）。
                let line_start: usize = pg
                    .lines
                    .iter()
                    .take(b.li)
                    .filter(|l| l.para_index == b.para)
                    .map(|l| l.text.chars().count())
                    .sum();
                let line_len = pg.lines[b.li].text.chars().count();
                if c.cp < line_start || c.cp > line_start + line_len {
                    bad.push(format!(
                        "{} 頁{} 段{} dy={:.2}：cp {} 不在點擊行碼點區間 [{}, {}]",
                        t.name,
                        b.page,
                        b.para,
                        dy,
                        c.cp,
                        line_start,
                        line_start + line_len
                    ));
                }
            }
        }
        assert!(total >= 12, "{}：靶樣本不足（實得 {}）", t.name, total);
        if buggy() {
            assert!(
                !bad.is_empty(),
                "燈 6 在 U4M_LINEBOX_BUGGY 態未紅＝靶地形失效（回塞鎖失去鑑別力）"
            );
        }
        assert!(
            bad.is_empty(),
            "燈 6 點擊段 ↔ caret 段全等破口 {}／{} 例（BUGGY 態點名：Enter 分錯段）：\n{}",
            bad.len(),
            total,
            bad.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
        );
    }
}
