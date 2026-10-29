//! K8 紅燈：表格儲存格背景色 `w:shd` 放射（fdo68607 斑馬紋／表頭底紋全缺）。
//!
//! 缺陷（PM 像素級實證＋本卡 LO PNG 覆核）：fdo68607（單一大表，
//! `MediumShading1-Accent1` 樣式）LO p2 表格區 49.8% 像素＝D3DFEE
//! (211,223,238)（頁級 38.0%），我方同區**純白**（OW-p2 D3DFEE＝0/2103750）。
//!
//! 根因（本卡 corpus 實測修正卡面前提，逐項鐵證）：
//! 1. `document.xml` 直接 `w:shd`＝0——全部底色宣告在 styles.xml 的
//!    **`w:tblStylePr` 條件帶**（firstRow→4F81BD、band1Horz→D3DFEE＋
//!    themeFill accent1＋themeFillTint 3F）。corpus 400 檔：tblPr 級 shd
//!    0 檔、tblStylePr 條件底紋 25 條（firstRow 11／band1Vert 6／
//!    band1Horz 6／band2Vert 1／lastRow 1）。級聯必須補這一層。
//! 2. 帶的生效閘＝`w:tblLook`（fdo68607 firstRow=1、noVBand=1→縱帶全關、
//!    noHBand=0→橫帶開）；LO 像素剖面鐵證：evenHBand 行在 band1Vert 宣告
//!    欄仍白（縱帶被 noVBand 整體閘掉），且 firstColumn 粗體格仍gets
//!    band1Horz 底色（逐屬性級聯，非整條件覆蓋）。
//! 3. `themeFillTint` 變換**不是**卡面所述「百分比→HSL 亮度混合」——
//!    corpus 1007 個帶 tint 的 `w:shd` 實測：該式 0/1007 命中；
//!    `c' = 255 − (255−c)×(byte/255)`（floor）975/1007 命中（其餘 32＝
//!    生產者 cache 的 w:fill 與 theme 現值分歧，LO 照讀 cache 值＝本卡
//!    「cache 優先」口徑）。單元鎖：accent1(#4F81BD)+3F → D3DFEE 精確
//!    （三通道 211.5/223.8/238.7 floor 全中；round 會錯——211.5→212）。
//! 4. 卡面「用戶目視的黃色表頭族」corpus 查無：tblStylePr 條件底紋黃色
//!    0 條；黃 FFFF00 只在未使用的字元/段落樣式。表頭族紅燈改鎖實測值
//!    （fdo68607 firstRow 藍 4F81BD＋tdf129575 系 tblHeader 直宣告 E0E0E0）。
//!
//! 修正前（main tip 911c5dd）燈 1/2/3 全紅（我方無任何背景項）；燈 4
//! （零宣告守門）與燈 5（tdf129575 直宣告鎖）先綠——後者是既有直接路徑
//! 的零回歸鎖（卡面「tblHeader/tcPr shd 檔至少一檔斷言色值命中」由此檔
//! 承擔）。`K8_SHD_BUGGY=1` 回塞態：條件帶＋theme 後備底色全滅→燈 1/2/3
//! 必紅。測試進程內不設 env（避免平行污染），由外部執行
//! `K8_SHD_BUGGY=1 cargo test -p quire-engine --test k8_shd` 驗證。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load_corpus(name: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + name;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus")
}

/// 頁內填色 Rect（排除圖片佔位灰 `IMAGE_PLACEHOLDER_FILL`＝0xFFF0F0F0——
/// K4c 佔位框非背景）。
fn fill_rects(eng: &Engine, page: usize) -> Vec<(f32, f32, f32, f32, u32)> {
    let dl = eng.page_display_list(page).expect("頁存在");
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } if *fill_argb != 0xFFF0_F0F0 => Some((*x, *y, *w, *h, *fill_argb)),
            _ => None,
        })
        .collect()
}

/// 表格邊界框（全部表格 Rule 的併集）——背景覆蓋率的分母。
fn table_bbox(eng: &Engine, page: usize) -> Option<(f32, f32, f32, f32)> {
    let dl = eng.page_display_list(page).expect("頁存在");
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut any = false;
    for it in &dl.items {
        if let DrawItem::Rule {
            x1: bx1,
            y1: by1,
            x2,
            y2,
            ..
        } = it
        {
            for (px, py) in [(*bx1, *by1), (*x2, *y2)] {
                x0 = x0.min(px);
                y0 = y0.min(py);
                x1 = x1.max(px);
                y1 = y1.max(py);
                any = true;
            }
        }
    }
    any.then_some((x0, y0, x1, y1))
}

/// 紅燈 1：fdo68607 p2 顯示列表含 fill≈D3DFEE(211,223,238) 且面積覆蓋
/// 表格區 ≥40%（LO p2 頁級 38.0%＝表格併集框內約 50%；容差下限 40%）。
#[test]
fn fdo68607_p2_zebra_band_coverage() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    let (bx0, by0, bx1, by1) = table_bbox(&eng, 2).expect("p2 有表格框線");
    let bbox_area = (bx1 - bx0) * (by1 - by0);
    assert!(bbox_area > 1000.0, "p2 表格併集框過小：{bbox_area}");
    // D3DFEE：accent1(4F81BD)+tint 3F 的 LO/Word 實渲色（0xFFD3DFEE 精確）。
    let d3: u32 = 0xFFD3_DFEE;
    let area: f32 = fill_rects(&eng, 2)
        .into_iter()
        .filter(|(_, _, _, _, argb)| *argb == d3)
        .map(|(_, _, w, h, _)| w * h)
        .sum();
    let frac = area / bbox_area;
    assert!(
        frac >= 0.40,
        "fdo68607 p2 D3DFEE 背景應覆蓋表格區 ≥40%（LO 實渲 ≈50%），實得 {frac:.3}（面積 {area:.0}/{bbox_area:.0}）——斑馬紋全缺"
    );
}

/// 紅燈 2：fdo68607 p0 表頭行（firstRow 條件帶）＝accent1 實渲色。
/// LO p0 像素 y∈[130,211] 實測 (79,128,189)≈0xFF4F81BD（1 級色差＝LO
/// 色域轉換）；我方發射字面值 0xFF4F81BD，容差 2 級。
#[test]
fn fdo68607_p0_first_row_header_band() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    let want: u32 = 0xFF4F_81BD;
    let near = |c: u32| -> bool {
        let d = |s: u32| (((c >> s) & 0xFF) as i32 - ((want >> s) & 0xFF) as i32).abs();
        c >> 24 == 0xFF && d(0) <= 2 && d(8) <= 2 && d(16) <= 2
    };
    let rects = fill_rects(&eng, 0);
    let hits: Vec<_> = rects.iter().filter(|(_, _, _, _, c)| near(*c)).collect();
    assert!(
        !hits.is_empty(),
        "fdo68607 p0 應有表頭行 accent1(0xFF4F81BD) 背景 Rect（LO p0 y130-211 實渲藍帶；firstRow 條件帶全缺＝{rects:?}）"
    );
}

/// 紅燈 3：多頁斑馬紋連貫——p0 也必含 D3DFEE 帶（LO p0 頁級 32.6%；
/// 表頭藍只在首表首行，續頁無 w:tblHeader 不重複）。
#[test]
fn fdo68607_p0_band_present() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    let d3: u32 = 0xFFD3_DFEE;
    let n = fill_rects(&eng, 0)
        .into_iter()
        .filter(|(_, _, _, _, c)| *c == d3)
        .count();
    assert!(
        n >= 8,
        "fdo68607 p0 D3DFEE 背景條目應 ≥8（LO p0 頁級 32.6%），實得 {n}"
    );
    // 頁數零位移鎖（K7-cal 成果不得被 K8 動搖）
    assert_eq!(
        eng.page_count(),
        8,
        "fdo68607 頁數必須仍為 8＝LO（K8 純發射面，零幾何位移）"
    );
}

/// 零宣告守門：無任何 shd 宣告（含樣式條件）的表格檔，顯示列表零背景項
/// ——K8 不得虛構底色。1_page／TableWithAboveCaptions／fdo69644 三檔
/// （corpus 掃描：document.xml 與被引用表格樣式均無 shd、無圖片）。
#[test]
fn zero_shading_files_emit_no_background() {
    for name in [
        "ooxmlexport/1_page.docx",
        "ooxmlexport/TableWithAboveCaptions.docx",
        "ooxmlexport/fdo69644.docx",
    ] {
        let eng = load_corpus(name);
        for p in 0..eng.page_count() {
            let fills = fill_rects(&eng, p);
            assert!(
                fills.is_empty(),
                "{name} p{p} 零 shd 宣告檔不得發背景 Rect（零擾動守門），實得 {fills:?}"
            );
        }
    }
}

/// 表頭族零回歸鎖（既有直接 tcPr shd 路徑）：tdf129575-directBefore 的
/// tblHeader 行宣告 `w:fill="E0E0E0"`——發射必須命中（卡面「tblHeader/
/// tcPr shd 檔至少一檔斷言色值命中」；corpus 實測無黃色條件表頭，黃=
/// 未引用樣式殘影，本鎖改以實測灰表頭族承擔）。
#[test]
fn tbl_header_direct_shd_still_emitted() {
    let eng = load_corpus("ooxmlexport/tdf129575-directBefore.docx");
    let want: u32 = 0xFFE0_E0E0;
    let found = (0..eng.page_count()).any(|p| {
        fill_rects(&eng, p)
            .into_iter()
            .any(|(_, _, _, _, c)| c == want)
    });
    assert!(
        found,
        "tdf129575-directBefore 應發射表頭行 E0E0E0 背景（直接 shd 路徑零回歸）"
    );
}
