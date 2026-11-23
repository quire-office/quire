//! T4AA（t_4aa15a4e）紅燈考卷：表格格內 caret——點格內不落游標、無法直接打字定位。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 卡面缺口（用戶可見形態）
//! `caret_at`／`caret_rect` 對表格格內段一律回 None（正逆同為 None，屬 v1
//! 既定契約）。用戶後果：點擊表格儲存格內文字，游標不落、無法直接打字定位
//! （格內編輯僅存工具列 marker-grid 路徑可用）。U4f 探針 v2（f920bad）已把
//! CELL 樣本從死區判定拆出如實計帳——用戶檔頁1 CELL=4/32、頁3=4/39。
//!
//! # PM 定案（本卡不得自行改方向）
//! 1. 引擎側＝**納入表格段**（正逆同源路線，否決「外殼自算」）：格內行的
//!    帶→頁 y 映射走 `TablePlacement.band`＋`RowFrag.band_start`＋格內 block
//!    帶累加，x 走 `TableFrag.x_pt`＋`CellFrag.x_pt`＋行內 advance，座標逐層
//!    加法定律照 `push_frag_items` 發射口徑，嚴禁另造尺。
//! 2. cp↔格內段落錨定＝`table_locator_json` 的同一索引空間（正向
//!    `locate_table_for_para` 已有，本卡補反查 para→格內行幾何）。
//! 3. 正逆同源考卷：格內 cp 掃描 caret_rect→caret_at 互逆，斷點只準在一處。
//!
//! # 鑑別力（BUGGY 回塞鎖口徑，鎖在源碼熱路徑）
//! - `T4AA_CELL_CARET_BUGGY=1`：格內正逆兩腿整支退回修前「一律 None」
//!   （v1 契約態）⇒ 燈 1／燈 3／燈 5／燈 6 必紅；燈 2（點擊）在該態轉為
//!   「格內點擊必全滅」的缺口記帳斷言（比照 u4f 燈 3 的鎖態雙記法——同一盞
//!   燈在修前態記的是缺口，不是假綠）；燈 4（索引空間）與命中面正交，恆綠。
//! - `T4AA_COARSE=1`：粗帶尺對照（每行 1 帶），六盞恆綠＝零擾動守門。

use quire_layout::table::{CellBlock, CellInput, RowInput, TableInput, WidthSpec};
use quire_layout::{CaretPos, LayoutEngine, LayoutParams, ParaGrid, TableSlot, TextMeasurer};

/// 用戶檔實測帶尺（U4f 記帳：fs=11、line_spacing=0.062987015 ⇒ 0.69285715pt/帶）。
const FINE_FS: f32 = 11.0;
const FINE_LS: f32 = 0.062987015;
/// 粗尺對照（每行 1 帶＝K6 細化前形態）。
const COARSE_LS: f32 = 1.4;
/// 細尺態每行帶數（用戶檔實測 para_band）。
const FINE_BANDS: u32 = 23;

fn fine() -> bool {
    std::env::var_os("T4AA_COARSE").is_none()
}

fn bands_per_line() -> u32 {
    if fine() {
        FINE_BANDS
    } else {
        1
    }
}

fn params() -> LayoutParams {
    LayoutParams {
        page_w_pt: 612.0,
        page_h_pt: 792.0,
        margins_pt: [72.0; 4],
        font_size_pt: FINE_FS,
        line_spacing: if fine() { FINE_LS } else { COARSE_LS },
        footnote_reserve_pt: 0.0,
        footnote_reserves_pt: Vec::new(),
        endnote_reserves_pt: Vec::new(),
        header_pushes_pt: Vec::new(),
    }
}

/// 格內段文字（輸入與 canonical texts **共用同一產生器**，杜绝兩側漂移）。
fn cell_text(prefix: &str, i: usize) -> String {
    format!("{prefix}{i}內文測試樣本")
}

/// 單欄表格：每格一段（字串長度提供足夠 cp 樣本）；`line_ref_bands` 全域同步。
fn table_input(rows: usize, prefix: &str) -> TableInput {
    TableInput {
        grid_pt: vec![200.0],
        width: Some(WidthSpec::Pt(200.0)),
        rows: (0..rows)
            .map(|i| RowInput {
                cells: vec![CellInput {
                    blocks: vec![CellBlock::Text(cell_text(prefix, i))],
                    ..Default::default()
                }],
                ..Default::default()
            })
            .collect(),
        line_ref_bands: if fine() { FINE_BANDS } else { 1 },
        ..Default::default()
    }
}

fn grid_for(n: usize) -> ParaGrid {
    ParaGrid {
        size_pt: Vec::new(),
        bands: vec![bands_per_line(); n],
        break_before: Vec::new(),
        before_bands: Vec::new(),
        after_bands: Vec::new(),
        advance_scale: Vec::new(),
        punct_arm: Vec::new(),
    }
}

/// 一格兩段＋巢狀子表＋協定填充段的「難格」輸入（回傳輸入＋該格各 block 的
/// 預期文字，供 canonical 錨定逐條對帳）。
fn hard_cell_fixture() -> (TableInput, Vec<String>) {
    let inner = TableInput {
        grid_pt: vec![60.0],
        width: Some(WidthSpec::Pt(60.0)),
        rows: (0..2)
            .map(|i| RowInput {
                cells: vec![CellInput {
                    blocks: vec![CellBlock::Text(format!("內{i}"))],
                    ..Default::default()
                }],
                ..Default::default()
            })
            .collect(),
        line_ref_bands: if fine() { FINE_BANDS } else { 1 },
        ..Default::default()
    };
    // 文件序：表前段 → 子表兩段 → 協定填充空段（ECMA-376 §17.4.66）。
    let expect = vec![
        "表前段".to_string(),
        "內0".to_string(),
        "內1".to_string(),
        String::new(),
    ];
    let input = TableInput {
        grid_pt: vec![300.0],
        width: Some(WidthSpec::Pt(300.0)),
        rows: vec![RowInput {
            cells: vec![CellInput {
                blocks: vec![
                    CellBlock::Text("表前段".into()),
                    CellBlock::Table(Box::new(inner)),
                    CellBlock::Text(String::new()),
                ],
                ..Default::default()
            }],
            ..Default::default()
        }],
        line_ref_bands: if fine() { FINE_BANDS } else { 1 },
        ..Default::default()
    };
    (input, expect)
}

/// 收集頁內所有格內行：(頁, canonical 段, 行文字, 行首 cp, 帶跨度, x, y)。
/// 座標加法定律逐字照 `push_frag_items` 發射口徑（考卷獨立重推，與引擎實作
/// 不同源，才能真正鎖住「正逆同源」而不是自我印證）。
fn cell_lines(eng: &LayoutEngine) -> Vec<(usize, usize, String, usize, u32, f32, f32)> {
    let mut out = Vec::new();
    for pg in eng.pages() {
        for tp in &pg.tables {
            let slot = &eng.table_slots()[tp.slot];
            collect(
                &tp.frag,
                tp.frag.x_pt,
                tp.frag.y_pt,
                slot.para_start,
                pg.index,
                &mut out,
            );
        }
    }
    out
}

fn collect(
    frag: &quire_layout::table::TableFrag,
    base_x: f32,
    base_y: f32,
    para_base: usize,
    page: usize,
    out: &mut Vec<(usize, usize, String, usize, u32, f32, f32)>,
) {
    let mut ry = 0.0f32;
    for row in &frag.rows {
        for cell in &row.cells {
            for t in &cell.texts {
                out.push((
                    page,
                    para_base + t.text_ord.expect("本卡地基必帶 text_ord"),
                    t.text.clone(),
                    t.char_offset,
                    t.bands,
                    base_x + t.x_pt,
                    base_y + ry + t.y_pt,
                ));
            }
            for (yoff, nf) in &cell.nested {
                // 頂層與巢狀同律：子表切片頂＝base_y＋ry＋yoff、子表左緣＝
                // base_x＋nf.x_pt（push_frag_items 遞迴同式）。
                collect(
                    nf,
                    base_x + nf.x_pt,
                    base_y + ry + yoff,
                    para_base,
                    page,
                    out,
                );
            }
        }
        ry += row.height_pt;
    }
}

fn engine_with_table(rows: usize, prefix: &str) -> (LayoutParams, Vec<String>) {
    let p = params();
    let texts: Vec<String> = (0..rows).map(|i| cell_text(prefix, i)).collect();
    (p, texts)
}

fn build_engine(texts: &[String], rows: usize, prefix: &str, p: &LayoutParams) -> LayoutEngine {
    let m = TextMeasurer::new(p.font_size_pt);
    let tl = quire_layout::table::layout_table(
        &table_input(rows, prefix),
        p.content_w_pt(),
        p.line_h_pt(),
        &m,
        0,
    );
    let slot = TableSlot {
        para_start: 0,
        para_end: rows,
        layout: tl,
    };
    LayoutEngine::with_breaks_grid(
        texts,
        p.clone(),
        Vec::new(),
        vec![slot],
        Vec::new(),
        Vec::new(),
        grid_for(texts.len()),
    )
}

// ---------------------------------------------------------------------------
// 燈 1（正向腿）：格內段 caret_rect 必可得，且 y 與發射口徑同尺同原點
// ---------------------------------------------------------------------------

#[test]
fn t4aa_cell_caret_rect_matches_fragment_geometry() {
    let (p, mut texts) = engine_with_table(6, "格");
    texts.push("表後正文段".to_string());
    let eng = build_engine(&texts, 6, "格", &p);

    // (a) 每個格內段逐 cp 掃 caret_rect：必可得、頁歸屬正確、y 與切片幾何同值。
    let lines = cell_lines(&eng);
    assert!(lines.len() >= 6, "格內行樣本不足（實得 {}）", lines.len());
    let mut probed = 0usize;
    let mut none_cnt = 0usize;
    for (pg, para, text, char_offset, _span, x0, y0) in &lines {
        for cp in 0..=text.chars().count() {
            probed += 1;
            let r = match eng.caret_rect(CaretPos { para: *para, cp }) {
                Some(r) => r,
                None => {
                    none_cnt += 1;
                    continue;
                }
            };
            assert_eq!(r.page, *pg, "格內 caret_rect 頁歸屬（段 {para}）");
            // 行首 cp 的 y 必須逐位元組等於發射口徑 y（同尺同原點，不另造尺）。
            if cp == *char_offset {
                assert!(
                    (r.y_pt - y0).abs() < 1e-3,
                    "格內 caret_rect.y 必等於切片幾何 y（段 {para}：{} vs {y0}）",
                    r.y_pt
                );
                assert!(
                    (r.x_pt - x0).abs() < 1e-3,
                    "段首 caret x 必等於切片 CellText.x_pt（段 {para}：{} vs {x0}）",
                    r.x_pt
                );
            }
            // 字級＝級聯宣告（無宣告＝全域尺）：height_pt 不得憑空換尺。
            assert!(
                (r.height_pt - p.font_size_pt).abs() < 1e-3,
                "height_pt＝級聯字級（段 {para}：{}）",
                r.height_pt
            );
        }
    }
    assert!(probed > 24, "cp 掃描樣本不足（實得 {probed}）");
    // 回塞鎖態（正向腿整支退回 None）必在此轉紅。
    let buggy = std::env::var_os("T4AA_CELL_CARET_BUGGY").is_some();
    if buggy {
        assert!(none_cnt > 0, "BUGGY 態格內正向腿必回 None");
    } else {
        assert_eq!(
            none_cnt, 0,
            "格內段 caret_rect 必全可得（缺口 {none_cnt} 條）"
        );
    }
    // 正文腿零擾動：表後正文段照舊可定位。
    let tail = eng
        .caret_rect(CaretPos {
            para: texts.len() - 1,
            cp: 0,
        })
        .expect("表後正文段 caret_rect 照舊可得");
    assert!(tail.y_pt > 0.0);
}

// ---------------------------------------------------------------------------
// 燈 2（反向腿）：點擊格內必落游標——行頂／帶中段／帶尾三處都要命中
// ---------------------------------------------------------------------------

#[test]
fn t4aa_pointerdown_in_cell_lands_caret() {
    let (p, mut texts) = engine_with_table(6, "格");
    texts.push("表後正文段".to_string());
    let eng = build_engine(&texts, 6, "格", &p);
    let line_h = p.line_h_pt();
    let lines = cell_lines(&eng);
    let buggy = std::env::var_os("T4AA_CELL_CARET_BUGGY").is_some();

    // 用戶形態：點擊格內文字（x＝行首＋1pt；y 三處＝行頂／帶中段／帶尾）。
    let mut total = 0usize;
    let mut miss = Vec::new();
    for (pg, para, _t, _co, _span, x0, y0) in &lines {
        for dy in [FINE_FS * 0.3, line_h * 0.5, line_h * 0.9] {
            total += 1;
            match eng.caret_at(*pg, x0 + 1.0, y0 + dy) {
                Some(c) if !buggy => {
                    // 落點語義（PM 定案第 2 條）：與正文同款——同段、cp 落在
                    // 該行區間內（點擊 x＝行首＋1pt，必落行首附近）。
                    assert_eq!(c.para, *para, "點擊格內必落該格段");
                    assert!(c.cp <= 2, "行首附近的點擊 cp 必靠近行首（實得 {}）", c.cp);
                }
                _ => miss.push((*pg, *para, dy)),
            }
        }
    }
    assert!(total >= 18, "點擊樣本不足（實得 {total}）");
    if buggy {
        assert_eq!(miss.len(), total, "BUGGY 態格內反查必全滅（v1 契約態）");
    } else {
        assert!(
            miss.is_empty(),
            "格內點擊必命中（死區 {}/{}：{:?}）",
            miss.len(),
            total,
            miss.iter().take(6).collect::<Vec<_>>()
        );
    }

    // 格外正當空白不落 caret（命中面不扩大到無文字區）。
    if !buggy {
        let (pg, y0) = (lines[0].0, lines[0].6);
        assert!(
            eng.caret_at(pg, 12.0, y0 + line_h * 0.5).is_none(),
            "版心左緣以遠（任何格外）點擊必 None"
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 3（正逆同源）：格內 cp 掃描 caret_rect→caret_at 互逆，斷點只準在一處
// ---------------------------------------------------------------------------

#[test]
fn t4aa_cell_caret_roundtrip_is_inverse() {
    let (p, mut texts) = engine_with_table(6, "格");
    texts.push("表後正文段".to_string());
    let eng = build_engine(&texts, 6, "格", &p);
    let line_h = p.line_h_pt();

    // 互逆：caret_rect(para,cp) 的點反查必還原同段同 cp（比照 U4f 燈 3 口徑）。
    let mut total = 0usize;
    let mut bad = Vec::new();
    for (para, para_text) in texts.iter().take(6).enumerate() {
        let n = para_text.chars().count();
        for cp in 0..=n {
            let fwd = match eng.caret_rect(CaretPos { para, cp }) {
                Some(r) => r,
                None => {
                    bad.push((para, cp, "rect=None".to_string()));
                    continue;
                }
            };
            for dy in [0.5f32, line_h * 0.5, line_h * 0.9] {
                total += 1;
                let back = eng.caret_at(fwd.page, fwd.x_pt, fwd.y_pt + dy);
                match back {
                    Some(c) if c.para == para && c.cp == cp => {}
                    other => bad.push((
                        para,
                        cp,
                        format!("dy={dy:.2} back={:?}", other.map(|c| (c.para, c.cp))),
                    )),
                }
            }
        }
    }
    assert!(total > 60, "互逆樣本不足（實得 {total}）");
    assert!(
        bad.is_empty(),
        "caret_rect→caret_at 互逆失敗 {}/{} 例：{:?}",
        bad.len(),
        total,
        bad.iter().take(8).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// 燈 4（canonical 錨定）：text_ord 與 retext_walk／table_locator 同一索引空間
// ---------------------------------------------------------------------------

#[test]
fn t4aa_text_ord_anchors_canonical_para_across_nest_and_filler() {
    let p = params();
    let m = TextMeasurer::new(p.font_size_pt);
    let (input, expect) = hard_cell_fixture();
    // canonical 段：表格子樹段落依文件序佔用 para_start..（含協定填充空段），
    // 之後才是表後正文段——與 table_map／retext_walk 的計數口徑一致。
    let mut texts = expect.clone();
    texts.push("表後正文段".to_string());
    let tl = quire_layout::table::layout_table(&input, p.content_w_pt(), p.line_h_pt(), &m, 0);
    let slot = TableSlot {
        para_start: 0,
        para_end: expect.len(),
        layout: tl,
    };
    let eng = LayoutEngine::with_breaks_grid(
        &texts,
        p.clone(),
        Vec::new(),
        vec![slot],
        Vec::new(),
        Vec::new(),
        grid_for(texts.len()),
    );

    // 頂層格內行：只含本格外層 block（巢狀子表行另行核），本 fixture＝表前段一行。
    let mut seen = 0usize;
    for (_pg, para, text, _co, _span, _x, _y) in cell_lines(&eng) {
        let canonical = eng.paragraph_text(para).expect("canonical 段必存在");
        assert_eq!(
            canonical, text,
            "格內行文字必等於 canonical 段文字（段 {para}：行 {text:?} vs 段 {canonical:?}）"
        );
        seen += 1;
    }
    assert!(seen >= 1, "頂層格內行樣本不足（實得 {seen}）");

    // 巢狀子表行：text_ord 已換算成外層樹序（內0／內1＝canonical 段 1／2）。
    let mut nested_seen = 0usize;
    for pg in eng.pages() {
        for tp in &pg.tables {
            let slot = &eng.table_slots()[tp.slot];
            for row in &tp.frag.rows {
                for cell in &row.cells {
                    for (_yoff, nf) in &cell.nested {
                        for nrow in &nf.rows {
                            for ncell in &nrow.cells {
                                for t in &ncell.texts {
                                    let ord = t.text_ord.expect("巢狀切片必帶換算後的 text_ord");
                                    let canonical = eng
                                        .paragraph_text(slot.para_start + ord)
                                        .expect("巢狀段必在 canonical 索引空間");
                                    assert_eq!(
                                        canonical, t.text,
                                        "巢狀格內行 canonical 錨定（ord {ord}）"
                                    );
                                    nested_seen += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(nested_seen >= 2, "巢狀樣本不足（實得 {nested_seen}）");

    // 協定填充段（ECMA-376 §17.4.66 尾部空 w:p）：真實段落，無行可定位——
    // caret_rect 誠實回 None（不憑空落游標），但它的 canonical 索引照佔位：
    // 表後正文段索引仍接在填充段之後（錨定不漂移）。
    let filler = expect.len() - 1; // 表前段＋子表2段 → 填充段索引＝3
    assert_eq!(texts[filler], "", "fixture：末 block 必為協定填充空段");
    assert!(
        eng.caret_rect(CaretPos {
            para: filler,
            cp: 0
        })
        .is_none(),
        "協定填充段無行可定位，caret_rect 必誠實 None"
    );
    assert!(
        eng.caret_rect(CaretPos {
            para: texts.len() - 1,
            cp: 0
        })
        .is_some(),
        "表後正文段照舊可定位（填充段不得吃掉正文錨定）"
    );
}

// ---------------------------------------------------------------------------
// 燈 5（跨頁切片續排）：列切片拆到第二頁的格內行照樣可定位＋互逆
// ---------------------------------------------------------------------------

#[test]
fn t4aa_cell_caret_survives_row_slice_across_pages() {
    // 60 列細尺表＝必然跨頁切片（每列 23 帶＝15.9pt，60 列 ≈ 956pt＞版心高）。
    let (p, texts) = engine_with_table(60, "列");
    let eng = build_engine(&texts, 60, "列", &p);
    let line_h = p.line_h_pt();
    assert!(
        eng.pages().len() >= 2,
        "60 列細尺表必跨頁（實得 {} 頁）",
        eng.pages().len()
    );

    // 跨頁的格內行：各頁切片各自的行，正逆兩腿都必可得且互逆（走 LayoutEngine
    // 真管線，考卷絕不自搓第二套尺——那會把「同源」考成自我印證）。
    let mut total = 0usize;
    let mut bad = Vec::new();
    let mut pages_with_cell_lines = 0usize;
    for pg in eng.pages() {
        let mut seen_here: Vec<(usize, usize)> = Vec::new();
        for tp in &pg.tables {
            let para_base = eng.table_slots()[tp.slot].para_start;
            fn walk(
                frag: &quire_layout::table::TableFrag,
                cb: &mut dyn FnMut(&quire_layout::table::CellText),
            ) {
                for row in &frag.rows {
                    for cell in &row.cells {
                        for t in &cell.texts {
                            cb(t);
                        }
                        for (_, nf) in &cell.nested {
                            walk(nf, cb);
                        }
                    }
                }
            }
            walk(&tp.frag, &mut |t| {
                if let Some(ord) = t.text_ord {
                    seen_here.push((para_base + ord, t.text.chars().count()));
                }
            });
        }
        if !seen_here.is_empty() {
            pages_with_cell_lines += 1;
        }
        for (para, n) in seen_here {
            for cp in 0..=n {
                total += 1;
                let fwd = match eng.caret_rect(CaretPos { para, cp }) {
                    Some(r) => r,
                    None => {
                        bad.push((pg.index, para, cp, "rect=None".to_string()));
                        continue;
                    }
                };
                let back = eng.caret_at(fwd.page, fwd.x_pt, fwd.y_pt + line_h * 0.5);
                match back {
                    Some(c) if c.para == para && c.cp == cp => {}
                    o => bad.push((
                        pg.index,
                        para,
                        cp,
                        format!("{:?}", o.map(|c| (c.para, c.cp))),
                    )),
                }
            }
        }
    }
    assert!(
        pages_with_cell_lines >= 2,
        "跨頁態必有多頁含格內行（實得 {pages_with_cell_lines} 頁）"
    );
    assert!(total > 24, "跨頁 cp 樣本不足（實得 {total}）");
    assert!(
        bad.is_empty(),
        "跨頁切片格內正逆互逆失敗 {}/{} 例：{:?}",
        bad.len(),
        total,
        bad.iter().take(8).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// 燈 6（打字連動）：格內打字後切片重建，caret 正逆兩腿跟隨新文字（不殘留舊尺）
// ---------------------------------------------------------------------------

#[test]
fn t4aa_cell_caret_follows_retext_rebuild() {
    let (p, mut texts) = engine_with_table(6, "格");
    texts.push("表後正文段".to_string());
    let mut eng = build_engine(&texts, 6, "格", &p);
    let line_h = p.line_h_pt();
    // 打字進格內段（t_c2bbcb38 口徑：以建槽快照重播 layout_table，槽重建）。
    let new_text = "格0改寫後的長文字內容測試";
    eng.replace_paragraph_text(0, new_text);
    assert_eq!(eng.paragraph_text(0), Some(new_text));

    // 重建後：新文字的末 cp 仍可得（行數變了，舊快照的跨度不得殘留）。
    let fwd = eng
        .caret_rect(CaretPos {
            para: 0,
            cp: new_text.chars().count(),
        })
        .expect("打字後格內段末 cp 必可得");
    let back = eng
        .caret_at(fwd.page, fwd.x_pt, fwd.y_pt + line_h * 0.5)
        .expect("打字後反查必命中");
    assert_eq!(
        (back.para, back.cp),
        (0, new_text.chars().count()),
        "打字後正逆互逆（新文字長度）"
    );
}
