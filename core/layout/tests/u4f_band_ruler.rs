//! U4f（t_93937dc5）紅燈考卷：細帶尺下的表格跑版＋caret 全面死區。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 卡面雙缺陷（用戶實測檔 15 頁含 3 表；PM 探針 u4f_table_overlay_caret_probe.mjs 坐實）
//! 1. 跑版：表格列與正文段同 y 區交疊發射（用戶截圖佐證）。
//! 2. caret 死區：15/15 頁 `caret_at_json` 命中率 <8%（頁1/3 為 0/55 全死）
//!    → 外殼 pointerdown 拿不到 caret → 打字／退格／選取全失效。
//!
//! # 真實根因（讀碼＋引擎內部態實測坐實，非推測）
//! 本檔用戶檔經 K6 帶尺細化（`choose_grid_line_h`）後全域帶尺 = 0.69285715pt／帶
//! （fs=11、ls=0.062987015）。兩個消費端仍按「1 帶＝1 行／帶尺 ≥1pt」的舊假設：
//!
//! **根因 A（跑版）** `quire_layout::table::layout_table` 開頭 `let line_h =
//! line_h.max(1.0)`（table.rs:1075）把細帶尺無條件復辟成 1pt／帶，而 packer
//! `pack_run` 的 FlowItem::Table 分支用**全域**帶尺換算切片 y
//! （lib.rs:1539 `frag.y_pt = margin_top + push + place_band × line_h`）並按
//! `used`（表格帶數）計帳（lib.rs:1548）。同一個帶數、兩套單位：
//! 表格內部按 1.0pt／帶畫高、packer 按 0.693pt／帶計帳 ⇒ 表格高度被少計
//! 30.7%，表格下方的正文行從「表格還有 30.7% 高度沒畫完」的位置起排 ⇒ 交疊。
//! 實測（用戶檔頁1）：切片 y=72 高 345pt（真畫到 417），正文卻從帶 345
//! （y=311）起排 ⇒ 7 條正文行落在表格 y 區內。
//!
//! **根因 B（caret 死區）** `caret_at`（lib.rs:2764）以
//! `p.line_bands.iter().position(|&b| b == band)?` 的**完全相等**判定制反查：
//! 粗帶尺（每行 1 帶）時 line_bands 恆為連續 0..n，任何 y 都命中；細帶尺下
//! 每行佔 para_band 帶（本檔 23 帶／行），line_bands 成稀疏點集
//! （0,46,92,115,...），點擊 y 換算出的帶幾乎必然落在兩行之間 ⇒ 整頁 None。
//! 正向 `caret_rect`（lib.rs:2736）與反向 `caret_at` 用的是**同一** line_h
//! 與同一 `page.line_bands`（正逆映射同源，非嫌疑），斷點只在反向的完全相等判定制。
//!
//! # 鑑別力（BUGGY 回塞鎖口徑，鎖在源碼熱路徑）
//! - `U4F_TABLE_LINEH_BUGGY=1`：表格內部帶尺退回修前 `max(1.0)` 復辟
//!   ⇒ 燈 1／燈 2 必紅（繪製高/計帳高比值回到 1/帶尺倍、正文行落回表格 y 區）。
//! - `U4F_CARET_AT_BUGGY=1`：反查退回修前「起帶完全相等」判制
//!   ⇒ 燈 3 必紅（命中率掉回 ~66.7%，互逆臂成批 None）。
//! - `U4F_COARSE=1`：切換回粗帶尺（每行 1 帶）——三盞燈恆綠＝零擾動守門。

use quire_layout::table::{CellBlock, CellInput, RowInput, TableInput};
use quire_layout::{
    layout_paragraphs, paginate_tables, LayoutEngine, LayoutParams, ParaGrid, TableSlot,
    TextMeasurer,
};

/// 用戶檔實測帶尺（fs=11、line_spacing=0.062987015 ⇒ 0.69285715pt／帶）。
const FINE_FS: f32 = 11.0;
const FINE_LS: f32 = 0.062987015;
/// 粗尺對照（舊 K6 前形態：1 帶＝1 行）。
const COARSE_LS: f32 = 1.4;

fn fine() -> bool {
    std::env::var_os("U4F_COARSE").is_none()
}

/// 與用戶檔同尺的版面參數（Letter 直式、邊界 72pt）。
fn params() -> LayoutParams {
    let ls = if fine() { FINE_LS } else { COARSE_LS };
    LayoutParams {
        page_w_pt: 612.0,
        page_h_pt: 792.0,
        margins_pt: [72.0; 4],
        font_size_pt: FINE_FS,
        line_spacing: ls,
        footnote_reserve_pt: 0.0,
        footnote_reserves_pt: Vec::new(),
        endnote_reserves_pt: Vec::new(),
        header_pushes_pt: Vec::new(),
    }
}

/// 單欄 n 列表格（每列宣告 trHeight=15.936pt＝用戶檔實測列高）。
fn table_input(rows: usize) -> TableInput {
    TableInput {
        grid_pt: vec![200.0],
        rows: (0..rows)
            .map(|i| RowInput {
                cells: vec![CellInput {
                    blocks: vec![CellBlock::Text(format!("儲存格{i}"))],
                    ..Default::default()
                }],
                height_pt: Some(15.936),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

/// 逐段每行帶數表：細尺下每行 23 帶（用戶檔實測 para_band），粗尺下恆 1。
fn grid_for(n: usize) -> ParaGrid {
    let bands = if fine() { 23 } else { 1 };
    ParaGrid {
        size_pt: Vec::new(),
        bands: vec![bands; n],
        break_before: Vec::new(),
        before_bands: Vec::new(),
        after_bands: Vec::new(),
        advance_scale: Vec::new(),
        punct_arm: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// 燈 1（根因 A／跑版）：切片自身高度必須等於 packer 計帳帶數×全域帶尺
// ---------------------------------------------------------------------------

#[test]
fn u4f_table_slice_height_matches_packer_band_charge() {
    let p = params();
    let line_h = p.line_h_pt();
    let m = TextMeasurer::new(p.font_size_pt);
    let tl = quire_layout::table::layout_table(&table_input(10), p.content_w_pt(), line_h, &m, 0);

    // 切片整高（繪製端實際畫出的 y 跨度）。
    let (frag, _next, used) = tl.fragment(0, 0, u32::MAX, false, 72.0);
    let drawn_h: f32 = frag.rows.iter().map(|r| r.height_pt).sum();
    // packer 計帳：place_band × 全域帶尺 → 表格之後正文起點位移＝used×line_h。
    let charged_h = used as f32 * line_h;

    // 強斷言：修前必紅（U4F_TABLE_LINEH_BUGGY=1 可重現紅態，見檔頭）。
    let ratio = drawn_h / charged_h;
    assert!(
        (ratio - 1.0).abs() < 0.02,
        "切片繪製高與 packer 計帳高必須同單位（比值 {ratio}；繪製 {drawn_h}pt vs 計帳 {charged_h}pt，used={used} 帶、帶尺 {line_h}pt）"
    );
}

// ---------------------------------------------------------------------------
// 燈 2（根因 A 的用戶可見形態）：表格之後的正文段不得落在表格 y 區內
// ---------------------------------------------------------------------------

#[test]
fn u4f_body_after_table_never_overlaps_table_y() {
    let p = params();
    let line_h = p.line_h_pt();
    let m = TextMeasurer::new(p.font_size_pt);
    let tl = quire_layout::table::layout_table(&table_input(10), p.content_w_pt(), line_h, &m, 0);
    // 10 列 × 15.936pt ＝ 159.36pt 表高（用戶檔頁1 實測 345pt／15 列同比例）。
    let mut texts = vec!["前段一行".to_string()];
    texts.extend((0..10).map(|i| format!("儲存格{i}")));
    texts.push("表格後的正文段".to_string());
    let tail_para = texts.len() - 1;
    let slot = TableSlot {
        para_start: 1,
        para_end: 1 + 10,
        layout: tl,
    };
    let pls = layout_paragraphs(&texts, &p);
    let (pages, _starts) = paginate_tables(&pls, &p, &[], &[slot]);
    assert_eq!(pages.len(), 1, "單頁fixture：表＋正文必須排進一頁");
    let pg = &pages[0];
    let tp = pg.tables.first().expect("頁內必須有表格切片");
    let table_h: f32 = tp.frag.rows.iter().map(|r| r.height_pt).sum();
    let table_top = tp.frag.y_pt;
    let table_bottom = table_top + table_h;

    // 表格後正文行的 y（發射口徑：margin_top＋帶×帶尺，engine-api 同式）。
    let mut overlapping = Vec::new();
    for (i, l) in pg.lines.iter().enumerate() {
        if l.para_index != tail_para {
            continue;
        }
        let band = pg.line_bands.get(i).copied().unwrap_or(i as u32) as f32;
        let y = pg.margin_top_pt + band * line_h;
        if y < table_bottom - 0.5 && y + line_h.max(1.0) > table_top {
            overlapping.push((i, y, table_top, table_bottom));
        }
    }
    // 強斷言：修前必紅（U4F_TABLE_LINEH_BUGGY=1 可重現紅態，見檔頭）。
    assert!(
        overlapping.is_empty(),
        "表格 y 區 [{table_top:.1},{table_bottom:.1}) 內不得有正文行，實得 {:?}",
        overlapping
    );
}

// ---------------------------------------------------------------------------
// 燈 3（根因 B／caret 死區）：細帶尺下逐頁反查必須命中
// ---------------------------------------------------------------------------

#[test]
fn u4f_caret_at_hits_every_emitted_line_on_fine_grid() {
    let p = params();
    let line_h = p.line_h_pt();
    let texts: Vec<String> = (0..24)
        .map(|i| format!("第{i}段：{}內容", "中文文字".repeat(6)))
        .collect();
    let grid = grid_for(texts.len());
    let eng = LayoutEngine::with_breaks_grid(
        &texts,
        p.clone(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        grid.clone(),
    );
    let pages = eng.pages();
    assert!(!pages.is_empty(), "必須有頁");

    // (a) 逐頁逐行：行自身 y 的中段點擊必須還原該行段落（正向 caret_rect 同點）。
    let mut total = 0usize;
    let mut dead = Vec::new();
    for pg in pages {
        for (i, _l) in pg.lines.iter().enumerate() {
            let band = pg.line_bands.get(i).copied().unwrap_or(i as u32) as f32;
            // 帶跨度中段＋行帶首兩處點擊（用戶點 anywhere 都要能定位）。
            for dy in [line_h * 0.5, 0.5, line_h * 8.0] {
                let y = pg.margin_top_pt + band * line_h + dy;
                total += 1;
                if eng.caret_at(pg.index, 200.0, y).is_none() {
                    dead.push((pg.index, i, band, dy));
                }
            }
        }
    }
    let hit_rate = if total == 0 {
        0.0
    } else {
        1.0 - dead.len() as f32 / total as f32
    };
    assert!(
        hit_rate > 0.7,
        "細帶尺下 caret 命中率必須 >70%，實得 {:.1}%（死區 {}/{} 例：{:?}）",
        hit_rate * 100.0,
        dead.len(),
        total,
        dead.iter().take(6).collect::<Vec<_>>()
    );

    // (b) 行距帶全命中（細尺態專用）：用戶檔的形態是「連續正文段、無段前後距」
    // ——兩行之間的每一帶都是行距，點那裡**不得是死區**（本臂的 U4f 本意）。
    // 粗尺態（每行 1 帶）無行距帶可掃 ⇒ 本臂跳過（零樣本不是零死區）。
    //
    // 【t_f073cd6c 歸屬口徑變更（PM 承擔改卷責任）：行框墨跡真值制】
    // 舊斷言「行距帶一律落回**上一行**」把 U4f 當時的判制（跨度 `[起帶, 起帶＋span)`
    // 起自行框頂）直接契約化，而那個判制正是本卡修的缺陷：span 帶＝整拍行距，其中
    // 靠下的帶是**下一行字形的字面位置**，舊口徑把下一行的字搶給上一段 ⇒ 用戶在下
    // 一行字上點擊卻拿到上一段段尾的 caret（按 Enter 分錯段）。
    // run 85/88 一度改以「兩行框頂之中線」切分，依 PM 11-26 04:45 前提反轉（`Text.y`
    // ＝行框頂，非基線）作廢：中線（12pt／pitch 16.8 態＝y＋8.4）橫穿字形，把每行
    // 字形下半中位 5.61pt 判給下一段（PM 逐行復算 38/38）。
    // 現行口徑＝**行框墨跡真值制**：相鄰兩行的切分點＝「上行墨跡下緣與下行墨跡上緣
    // 的中點」（夾到不越下行行框頂）。本臂以考卷端獨立算式求同一條切分邊界，期望：
    //     點擊帶 bd ＝ b0＋k＋0.5 ＜ split ⇒ 屬本行；bd ≥ split ⇒ 屬下一行。
    // 另加一條**對中線制的鑑別斷言**：落在「中線以上、墨跡切分以下」那一塊的帶，
    // 現行制必屬本行（中線制會判給下一行）——這是本臂對 PM 作廢理由的本地證據。
    // 保留兩條與歸屬口徑正交的硬斷言：非死區（U4f 本意）＋歸屬單調不跳行。
    let (asc_em, desc_em) = quire_layout::shaping::ink_em_ratios();
    // 本 fixture 全段同一字級（`ParaGrid::size_pt` 空＝退回 `params.font_size_pt`）。
    let size_pt = p.font_size_pt;
    // 墨跡上下緣相對行框頂的帶位移（與引擎 `shaping::ink_offsets_pt`／`caret_at`
    // 同一公式；尺源同一：`line_h`＝全域帶尺、`asc_em／desc_em`＝內嵌字型度量）。
    let ink_top_off = (size_pt - asc_em * size_pt) / line_h;
    let ink_bot_off = (size_pt + desc_em * size_pt) / line_h;
    let mut pitch_total = 0usize;
    let mut pitch_dead = Vec::new();
    let mut pitch_owner = Vec::new();
    let mut pitch_wild = Vec::new();
    let mut pitch_flip_bad = Vec::new();
    let mut pitch_flips = 0usize;
    // 鑑別計數（對中線制）：落在「中線～墨跡切分」之間、現行制判屬本行的樣本數。
    let mut ink_vs_midline = 0usize;
    if fine() {
        for pg in pages {
            let next_line_para =
                |i: usize| -> Option<usize> { pg.lines.get(i + 1).map(|l| l.para_index) };
            for (i, l) in pg.lines.iter().enumerate() {
                let b0 = match pg.line_bands.get(i) {
                    Some(b) => *b,
                    None => continue,
                };
                let span = grid.bands_of(l.para_index);
                // 切分點（帶）：與下一行（同欄）的墨跡相鄰邊中點；末行無下一行 ⇒
                // 沿用修前下界 `b0＋span`。
                let split = match next_line_para(i) {
                    Some(_) => {
                        let bot = b0 as f32 + ink_bot_off;
                        let dtop = (b0 + span) as f32 + ink_top_off;
                        let mid = (bot + dtop) / 2.0;
                        let hi = (b0 + span) as f32;
                        if mid >= hi {
                            hi
                        } else if mid <= b0 as f32 {
                            b0 as f32
                        } else {
                            mid
                        }
                    }
                    None => b0 as f32 + span as f32,
                };
                let midline = b0 as f32 + span as f32 / 2.0;
                for k in 1..span {
                    let bd = (b0 + k) as f32 + 0.5;
                    let y = pg.margin_top_pt + bd * line_h;
                    pitch_total += 1;
                    let Some(hit) = eng.caret_at(pg.index, 200.0, y) else {
                        pitch_dead.push((pg.index, i, b0, k));
                        continue;
                    };
                    // 歸屬口徑（墨跡真值制的書面版）。
                    let want = if bd < split {
                        l.para_index
                    } else {
                        match next_line_para(i) {
                            Some(pp) => pp,
                            None => l.para_index, // 末行無下一行：沿用本行
                        }
                    };
                    if hit.para != want {
                        pitch_owner.push((pg.index, i, b0, k, hit.para, want));
                    }
                    // 落點不野跳：歸屬段只能是本行段或下一行段（越界只走一格，
                    // 與「上一段段尾」這種跨段錯判劃清界線）。
                    if hit.para != l.para_index && Some(hit.para) != next_line_para(i) {
                        pitch_wild.push((pg.index, i, b0, k, hit.para));
                    }
                    // 鑑別計數（跨墨跡切分才換段）：本行段 ≠ 下一行段時必須真換段。
                    if bd >= split && next_line_para(i).is_some_and(|pp| pp != l.para_index) {
                        if Some(hit.para) != next_line_para(i) {
                            pitch_flip_bad.push((pg.index, i, b0, k, hit.para));
                        }
                        pitch_flips += 1;
                    }
                    // 對中線制的鑑別：中線以上、墨跡切分以下 ⇒ 現行制屬本行（中線
                    // 制在此判給下一行＝PM 復算 38/38 破口的取樣區）。
                    if bd >= midline && bd < split {
                        if hit.para != l.para_index {
                            pitch_owner.push((pg.index, i, b0, k, hit.para, l.para_index));
                        }
                        ink_vs_midline += 1;
                    }
                }
            }
        }
        assert!(pitch_total > 100, "行距樣本不足（實得 {pitch_total}）");
        assert!(
            pitch_flips > 10,
            "跨墨跡切分換段樣本不足（實得 {pitch_flips}）——本臂未真正掃過墨跡邊界"
        );
        assert!(
            ink_vs_midline > 10,
            "「中線～墨跡切分」取樣不足（實得 {ink_vs_midline}）——本臂未對中線制取樣"
        );
        assert!(
            pitch_dead.is_empty(),
            "行距帶不得有死區（字槽外的合法點擊也必落游標），死區 {:?} 例（共 {pitch_total}）",
            pitch_dead.iter().take(6).collect::<Vec<_>>()
        );
        assert!(
            pitch_owner.is_empty(),
            "行距帶歸屬須守行框墨跡真值制（bd＜切分點屬本行，否則屬下一行），錯位 {:?} 例（共 {pitch_total}）",
            pitch_owner.iter().take(6).collect::<Vec<_>>()
        );
        assert!(
            pitch_wild.is_empty(),
            "行距帶落點野跳（歸屬段超出本行／下一行兩格）：{:?} 例",
            pitch_wild.iter().take(6).collect::<Vec<_>>()
        );
        assert!(
            pitch_flip_bad.is_empty(),
            "跨墨跡切分未換段（下一行字形仍被上一段吞走＝用戶實錄缺陷）：{:?} 例",
            pitch_flip_bad.iter().take(6).collect::<Vec<_>>()
        );
    }

    // (c) 正逆互逆（卡面「正逆映射是否同源」的直接證據）：
    // caret_rect 給出的 y 反餵 caret_at 必須還原同段。
    let mut fwd = 0usize;
    let mut mismatch = Vec::new();
    for (pi, t) in texts.iter().enumerate() {
        let n = t.chars().count();
        for cp in [0usize, n / 2, n.saturating_sub(1)] {
            let Some(rect) = eng.caret_rect(quire_layout::CaretPos { para: pi, cp }) else {
                continue;
            };
            fwd += 1;
            let back = eng.caret_at(rect.page, rect.x_pt, rect.y_pt + line_h * 0.5);
            match back {
                Some(b) if b.para == pi => {}
                other => mismatch.push((pi, cp, rect.page, rect.y_pt, other)),
            }
        }
    }
    assert!(fwd > 20, "正向樣本不足（實得 {fwd}）");
    assert!(
        mismatch.is_empty(),
        "caret_rect→caret_at 互逆失敗 {}/{} 例：{:?}",
        mismatch.len(),
        fwd,
        mismatch.iter().take(6).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// 燈 4（零擾動守門）：粗帶尺（每行 1 帶）行為逐位元組不變
// ---------------------------------------------------------------------------

#[test]
fn u4f_coarse_grid_behaviour_unchanged() {
    let p = params();
    let line_h = p.line_h_pt();
    let texts: Vec<String> = (0..8)
        .map(|i| format!("第{i}段：{}內容", "中文文字".repeat(6)))
        .collect();
    let grid = grid_for(texts.len());
    let eng = LayoutEngine::with_breaks_grid(
        &texts,
        p.clone(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        grid.clone(),
    );
    let no_grid = LayoutEngine::with_breaks_grid(
        &texts,
        p.clone(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        ParaGrid::default(),
    );
    if fine() {
        // 細尺態：每行 23 帶 ⇒ 帶序必非連續（與粗尺不同形，證明本考真在細尺上跑）。
        let bands = &eng.pages()[0].line_bands;
        assert!(
            bands.len() < 2 && bands.get(1) == Some(&46) || bands.len() >= 2 && bands[1] != 1,
            "細尺前提失效：頁0 帶序 {bands:?}（應為 23 帶間隔）"
        );
    } else {
        // 粗尺態：grid.bands=1 與空 grid 必須逐位元組全等（零擾動閘門）。
        assert_eq!(
            eng.pages(),
            no_grid.pages(),
            "粗尺下 bands=1 必與全域尺全等"
        );
    }
    assert!(line_h > 0.0);
}
