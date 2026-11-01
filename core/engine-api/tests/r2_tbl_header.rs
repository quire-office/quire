//! R2 紅燈：`w:tblHeader` 跨頁表格表頭逐切片重發。
//!
//! 契約（卡面紅燈 1）：表頭列（`w:trPr/w:tblHeader`）在表格跨頁切片時，
//! 必須在**每個切片頁**重發——LO/Word §17.4.86 語義（自列 0 起連續的
//! tblHeader 段逐頁重貼；`w:val="0"` 取消重複）。
//!
//! 錨檔修正（run2 實測，PM 覆核依據）：卡面引 RA001 'Order of injection'
//! ×2→×4，但 RA001 **全檔 0 個 `w:tblHeader`**（document.xml grep=0），
//! 該串現況 ×4 由 L3-revisions 的 del/ins 採集修復達成（'Or'＋'der of
//! injection:' 拆 run ×2 ＋ delText 鏡像 ×2），與表格表頭重發無關——
//! RA001 無法承載本卡紅燈（R2_BUGGY 下 ×4 不動＝偽綠）。改錨
//! selfgen/patho_table_across_pages.docx：真 `w:tblHeader`（row 0
//! 「項目/數值/備註」）＋ 61 列跨多頁切片，紅燈直接量「表頭逐頁重發」。
//! RA001 留作 L3/頁數零回退守門（本卡不得動 p 計數、不得回退 delText）。
//!
//! 回塞鎖：`R2_BUGGY=1` 關閉逐切片重貼 → 紅燈 1 必須紅。
use quire_engine::Engine;
use quire_render_trait::DrawItem;

fn load(path: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + path;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open")
}

fn patho() -> Engine {
    load("selfgen/patho_table_across_pages.docx")
}

fn ra001() -> Engine {
    load("xopt/TestFiles_RA001-Tracked-Revisions-01.docx")
}

/// 回傳文字串出現的總次數與命中頁序（去重）。
fn pages_with(eng: &Engine, needle: &str) -> (usize, Vec<usize>) {
    let mut total = 0usize;
    let mut pages = Vec::new();
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p).expect("頁存在");
        let mut hit = 0usize;
        for it in &dl.items {
            if let DrawItem::Text { text, .. } = it {
                if text.contains(needle) {
                    hit += 1;
                }
            }
        }
        if hit > 0 {
            total += hit;
            pages.push(p);
        }
    }
    (total, pages)
}

/// 紅燈 1（卡面紅燈 1 的承載修正版）：表頭（`w:tblHeader` 列「項目／數值」）
/// 必須出現在**每個**切片頁。patho 檔 61 列表格跨 2 頁切片；修前（R2_BUGGY=1
/// 實測）表頭只在 p0 出現、p1 缺席 → 紅。修後逐頁重發 → 綠。
#[test]
fn r2_header_repeats_on_every_slice_page() {
    let eng = patho();
    let pages = eng.page_count();
    assert!(pages >= 2, "前置：patho 表須跨 ≥2 頁切片（實際 {pages}）");
    let (n, hdr_pages) = pages_with(&eng, "項目");
    assert_eq!(
        hdr_pages.len(),
        pages,
        "表頭「項目」必須在全部 {pages} 個切片頁重發（ECMA §17.4.86／LO 口徑）；\
         實際命中頁 {hdr_pages:?} 共 {n} 次（R2_BUGGY=1 下應只剩首切片頁→紅）"
    );
    // 表頭第二格也在每頁重發（不是只有第一格）。
    let (_, ps) = pages_with(&eng, "數值");
    assert_eq!(
        ps.len(),
        pages,
        "表頭「數值」格須逐切片頁重發：命中頁 {ps:?}（應全 {pages} 頁）"
    );
}

/// 紅燈 2（切片序語義）：數據列**不得**被重貼——「跨頁資料列 1」只出現在
/// 它所在的那一頁（重貼若把整表重發，此條紅）。
#[test]
fn r2_body_rows_not_repeated() {
    let eng = patho();
    let (n, ps) = pages_with(&eng, "跨頁資料列 11"); // row1: '跨頁資料列 1'＋'1'＋'備註 1'
    assert!(
        n >= 1 && ps.len() == 1,
        "數據列「跨頁資料列 1」只應在自身切片頁出現；實際 {n} 次／頁 {ps:?}"
    );
}

/// 綠燈守門（鐵律「本卡應不改頁數」）：patho 頁數封鎖在 M1-band 基線
/// 2 頁（重貼只加發射、不得把內容擠多一頁）。
#[test]
fn r2_page_count_locked() {
    let eng = patho();
    assert_eq!(
        eng.page_count(),
        2,
        "patho 頁數＝2（M1-band 基線；重貼不得改頁數）"
    );
}

/// 零擾動守門（綠燈）：數據列全檔各發一次——重貼只加表頭、不得虛增他項。
#[test]
fn r2_no_spurious_repeat() {
    let eng = patho();
    // 表頭「項目」發射次數恰＝頁數（每頁恰一次，不得同頁虛發多次）。
    let (n, _) = pages_with(&eng, "項目");
    assert_eq!(
        n,
        eng.page_count(),
        "表頭「項目」每切片頁恰發一次（實際 {n}）"
    );
    // 60 條資料列發射 48 次：每 5 列（5,10,…,60 共 12 列）首格是 vMerge
    // continue——ECMA 語義其內容渲染時不出現（goldens.rs 同款口徑），
    // 60−12＝48。重貼只加表頭，數據列不得再被重複。
    let (m, _) = pages_with(&eng, "跨頁資料列");
    assert_eq!(
        m, 48,
        "數據列全檔共發 48 次＝每非 merge 列恰一次（實際 {m}）"
    );
}

/// 守門（L3 鐵律＋卡面綠燈）：delText 'Chromatpgrams' 照發 ×1——本卡
/// 不得回退 L3-revisions 成果。頁數錨點 7→6（K28）：R2/K7b 期註記「LO
/// 真值 7 頁」與其**自證檔本身矛盾**——`scratch/k7b/lo/TestFiles_RA001-
/// Tracked-Revisions-01.pdf`（corpus 檔 md5 相同，K7b worker 實轉存證）
/// pymupdf 實數＝6 頁（K28 worker 2026-11-02 覆核，另獨立重轉亦 6）；
/// 本倉 k5-diag §2.3「LO6/我方 9」＋r8-diag「六頁」同口徑。K28 幽靈
/// 網格欄修復（`w:tblGridChange` 併入欄不再參與 tblW 歸一）使 RA001
/// 三表回 raw 節距，7→6 與 LO 收斂——向 LO 真值更新錨點（比照
/// af3bf34 K6 先例），非本卡改頁數。
#[test]
fn ra001_page_count_and_l3_locked() {
    let eng = ra001();
    assert_eq!(
        eng.page_count(),
        6,
        "RA001 頁數＝6（LO 真值：k7b-lo 自證 PDF 實數＋K28 重轉＋k5/r8-diag 三方一致；K28 幽靈欄修復後收斂）"
    );
    let (deltext, _) = pages_with(&eng, "Chromatpgrams");
    assert_eq!(deltext, 1, "L3 delText 'Chromatpgrams' ×1 不得回退");
    // 卡面口徑 'Order of injection' ×4 現況鎖定——注意這是 **L3 領土**
    // （'Or'＋'der of injection:' 拆 run ×2 ＋ delText 鏡像 ×2），
    // R2_BUGGY=1 下仍 ×4（實測）：該數非表頭重發貢獻，此條為 L3 成果
    // 防回退守門（本卡不得動 run 採集）。
    let (oi, _) = pages_with(&eng, "Order of injection");
    assert_eq!(oi, 4, "'Order of injection' ×4（L3 del/ins 口徑）不得回退");
}
