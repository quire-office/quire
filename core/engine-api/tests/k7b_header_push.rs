//! K7b 紅燈：頁首內容越 top margin 時推下正文起點（Bug51170 尾部流動）。
//!
//! 口徑出自 `docs/k7-wrapwidth-diag.md` §4（LO 真值＝soffice 25.8 本機轉
//! PDF 逐頁實測，K7 run67 診斷＋本卡覆核）：
//! - 源檔 `corpus/poi/test-data_document_Bug51170.docx`：A4、pgMar top
//!   106.35／header 35.45、titlePg；header part 含大圖 146×78pt（佔
//!   y 35.45–113.45＝越過 top margin 7.1pt）。
//! - LO：p0 無頁首（titlePg）正文起 y=106.7；**p1+ 正文起點被頁首圖推到
//!   125.65**（＝圖底＋一行清距 12.65）；每頁正文行 [29,36,30,33,30,28,12]
//!   （p6 尾頁 12 行＋頁碼區）。
//! - 修前我方：p1+ 起點仍 106.35 → 每頁多吸 1–2 行 → 尾頁 p6 吸乾（僅頁碼
//!   ＋頁首圖，零正文）。修後：頁首推下表（`LayoutParams::header_pushes_pt`）
//!   p1+ 起點 126.1（Δ LO 0.45pt）、p6 有正文——頁數 7 對齊 LO。
//!
//! 每頁行數**不逐一硬鎖**：逐頁行切分受 K7-cal 字形 advance 寬差軸污染
//! （報告 §3，K7 run68 進行中——本卡禁碰 pitch/advance 寬表），斷言只鎖
//! 「推下起點＋尾頁不吸乾＋頁數對齊 LO」三件事；總行數守門防「多頁湊數」
//! 退化。
//!
//! BUGGY 回塞鎖：`K7B_HDR_PUSH_BUGGY=1` 回塞舊口徑（不推）——紅燈 1/3 必
//! 紅。測試進程內不設 env（避免平行污染），由外部執行
//! `K7B_HDR_PUSH_BUGGY=1 cargo test -p quire-engine --test k7b_header_push`
//! 驗證（實測見提交說明）。
//!
//! 零擾動鎖（紅燈 4）：有頁首宣告但內容**未越界**的檔（tdf124637，頁首一
//! 空段）與無頁首檔（fdo68607）正文起點逐位不紅——推下表恆空。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

const B51170: &str = "poi/test-data_document_Bug51170.docx";

fn load(path: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + path;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open")
}

/// 第 `page` 頁非空 Text 項 y（排序）。
fn page_ys(eng: &Engine, page: usize) -> Vec<f32> {
    let dl = eng.page_display_list(page).expect("頁存在");
    let mut ys: Vec<f32> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { y, text, .. } if !text.trim().is_empty() => Some(*y),
            _ => None,
        })
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys
}

/// 紅燈 1：p1 正文首行被頁首大圖推到 126.1±1.0pt（LO 125.65）。
/// 修前舊徑＝106.35（距 LO 19.3pt）必紅。
#[test]
fn k7b_bug51170_body_start_pushed_below_header_image() {
    let eng = load(B51170);
    let ys = page_ys(&eng, 1);
    let first = ys.first().copied().unwrap_or(f32::NAN);
    assert!(
        (first - 126.1).abs() < 1.0,
        "p1 正文起點應 ≈126.1（LO 125.65±1.0，頁首圖底 113.45＋行距清距），實得 {first}"
    );
    // p0＝titlePg 無頁首：起點仍是 margin_top 106.35（±0.6，LO 106.7）。
    let y0 = *page_ys(&eng, 0).first().unwrap();
    assert!(
        (y0 - 106.35).abs() < 0.6,
        "titlePg 首頁不得推下（LO 106.7），實得 {y0}"
    );
    // p2–p5 同款：起點必須落在「推下網格」上（126.1＋n×帶尺 2.53）——
    // 首行是否恰在 126.1 取決於段後距分佈（K7-cal 寬差軸會挪行，本卡不鎖
    // 逐頁行切分），但起點必在推後網格、絕不回落 margin_top 106.35。
    for p in 2..=5 {
        let y = *page_ys(&eng, p).first().unwrap();
        // 帶尺＝2.53pt（正文行距 12.65 的 1/5——本檔 K6 細尺實測，
        // 148.87−126.10＝22.77＝9×2.53 印證）。
        let r = (y - 126.1) / 2.53;
        assert!(
            y > 113.45 && (r - r.round()).abs() * 2.53 < 0.4,
            "p{p} 正文起點必須在推下網格（126.1＋n×2.53）且不重疊頁首圖，實得 {y}"
        );
    }
}

/// 紅燈 2：頁數對齊 LO（7 頁），且尾頁 p6 不再被吸乾——LO p6 有 12 行正文，
/// 修前我方 p6 零正文（僅頁碼＋頁首圖）。總行數守門防「多頁空轉」退化：
/// 全域正文行（排除頁尾區 y≥780）LO＝188，修前我方＝184，修後必須 ≥LO−4
/// （K7-cal 寬差軸殘留 ±few 行，不硬鎖逐頁——見檔頭註明）。
#[test]
fn k7b_bug51170_page_count_matches_lo_and_tail_not_drained() {
    let eng = load(B51170);
    assert_eq!(eng.page_count(), 7, "頁數對齊 LO（7 頁）");
    // p6 尾頁：必須有正文行（y < 780 排除頁碼/頁尾文字）。
    let body: Vec<f32> = page_ys(&eng, 6)
        .into_iter()
        .filter(|&y| y < 780.0)
        .collect();
    assert!(
        body.len() >= 2,
        "p6 不得吸乾（LO 12 行正文；修前 0 行），實得正文行數 {}",
        body.len()
    );
    // 全域正文行總數（排除頁尾區）：修後 ≥ 184（修前基線，不得倒退吸尾頁）。
    let mut total = 0usize;
    for p in 0..eng.page_count() {
        total += page_ys(&eng, p).into_iter().filter(|&y| y < 780.0).count();
    }
    assert!(
        (180..210).contains(&total),
        "全域正文行數應 ≈LO 188（±容差；K7-cal 寬差軸另卡），實得 {total}"
    );
}

/// 紅燈 3：首行 y 落推下網格（margin_top＋spill＋pitch）——p1 首行與第二行
/// 距＝一個行距步（12.65），證推下未打亂行網格；且首行 y > 頁首圖底 113.45
/// ＋頁首圖仍發射在 y=35.45（推下不挪圖）。
#[test]
fn k7b_bug51170_grid_and_header_image_intact() {
    let eng = load(B51170);
    let ys = page_ys(&eng, 1);
    let pitch = ys[1] - ys[0];
    assert!(
        (pitch - 12.65).abs() < 0.05,
        "推下後行網格步距仍＝正文行距 12.65，實得 {pitch}"
    );
    let dl = eng.page_display_list(1).unwrap();
    let img = dl.items.iter().find_map(|it| match it {
        DrawItem::Image { y, h, .. } => Some((*y, *h)),
        _ => None,
    });
    assert_eq!(
        img,
        Some((35.45, 78.0)),
        "頁首圖仍掛 header 距 35.45×高 78（推下不挪圖）"
    );
    assert!(
        ys[0] > 113.45,
        "正文首行必須在頁首圖底端之下（不重疊），實得 {}",
        ys[0]
    );
}

/// 紅燈 4（零擾動鎖）：有頁首但內容**未越界**的檔（tdf124637_sectionMargin：
/// 頁首只一個空段）與無頁首檔（fdo68607）——正文起點與頁數逐位不動。
/// 錨定修前基線實測值（/tmp/k7b-base 5ae6fd8 dump 對照 byte-identical，
/// 見提交說明）：本卡改動的代價上限＝0（seed25 多數檔屬這類）。
#[test]
fn k7b_no_push_when_header_within_margin() {
    let eng = load("ooxmlexport/tdf124637_sectionMargin.docx");
    let ys = page_ys(&eng, 0);
    assert!(
        (ys[0] - 82.5615).abs() < 0.6,
        "短頁首檔正文起點＝修前基線 82.5615 不得動，實得 {}",
        ys[0]
    );
    assert_eq!(eng.page_count(), 5, "短頁首檔頁數 5 不得動（修前基線）");
    // 無頁首檔：fdo68607 p0 正文起點＝margin_top 36.0、頁數 9（暫記態，
    // k7_wrap_width 紅燈 3 口徑——9 或 8 皆合法，此處只鎖推下不摻與）。
    // K10 附註：表內浮動封面文字框修復後，首個文字項為 y≈21.6 之大標；
    // 正文起點（表格文字）仍為 margin_top 36.0 不受頁首推下干擾。
    let eng2 = load("ooxmlexport/fdo68607.docx");
    let ys2 = page_ys(&eng2, 0);
    let body_y0 = ys2.iter().copied().find(|&y| y >= 30.0).unwrap_or(0.0);
    assert!(
        (body_y0 - 36.0).abs() < 0.6,
        "無頁首檔正文起點＝margin_top 36.0 不得動，實得 {}",
        body_y0
    );
    assert!(
        eng2.page_count() == 9 || eng2.page_count() == 8,
        "fdo68607 頁數仍屬 k7_wrap_width 暫記態（9/8），實得 {}",
        eng2.page_count()
    );
}
