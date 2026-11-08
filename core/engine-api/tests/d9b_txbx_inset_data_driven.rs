//! D9b：框內圖 `v:textbox/@inset` 資料化紅燈考卷（返修 D9 的字面量落地）。
//!
//! D9（合併 `cab2847`）把 fdo76591 頁腳 logo 的位移寫成字面量 `(7.25, 3.60)`，
//! 且以該檔專屬座標 `(225×45 於 (193.8, 729.2))` 做比對才觸發。本考卷鎖的是
//! **源檔資料驅動的同一條規則**：發射像素若是宿主框 `w:txbxContent` 的內嵌
//! `w:drawing`（K27-B 形態），其排版原點＝框原點＋宿主框 `v:textbox/@inset`
//! （源檔未宣告時退 VML 預設內縮 7.2/3.6——這是**格式預設**，不是檔案常量）。
//!
//! 燈 1 是本卡的先紅後綠主證：同一條規則把 D9 字面量臂**從來沒觸及**的第二個
//! 框 `_x0000_s1039` 也拉回 LO 真值（改前偏 7.95pt／3.55pt，改後 0.75pt／0.05pt）。
//!
//! 極性更正（卡面前提不可信，逐條實測後記）：
//! 卡面要求「以 corpus 內至少兩個帶自訂 `v:textbox/@inset` 的檔案對 LO bbox ±4pt」。
//! 全庫 grep（`scratch_d9b/scan_et.py`，400 檔）實況：
//!   * 帶**自訂非零** inset 的框（dkvert／mce-nested／fdo61343／tdf103975／
//!     WC048／CoverPage／HC031／groupshape-sdt）**全部是純文字框**，框內零圖片
//!     事實（LO pymupdf 實跑 images=0），對「框內圖 bbox」無從斷言；
//!   * 框內真有圖＋有 inset 宣告的檔案只有 `inset="0,0,0,0"` 一族
//!     （n792778／tdf102466／tdf48658……）。`0,0,0,0` 仍是**自訂宣告值**，
//!     語義與「未宣告→吃 VML 預設 7.2/3.6」不同（差 7.2pt），故用作
//!     「宣告值不得被預設值抹平」的第二、三盞燈（燈 3／燈 4）。
//!   * 我方唯一實際走 `PageFloatingImage` 的框內圖目前只有 fdo76591 兩框
//!     （全庫 DisplayList 差分實證：本改動只讓 fdo76591 一檔變動）。
//!     燈 3／燈 4 因此是**語義防調參鎖**（守住「宣告 0 ⇒ 不移位」與「新臂不得
//!     越界套到頁首／群組路徑」），非新臂本身之證明——新臂由燈 1 證明。
//!
//! 鑑別力（BUGGY 回塞鎖）：`D9B_INSET_BUGGY=1` ⇒ 發射層把框內圖原點退回框原點
//! （＝不吃 inset）。燈 1 必紅（實測偏 LO 7.95pt／3.55pt）；實跑兩種模式記進
//! `docs/d9b-inset-redlight.md`。

use quire_engine::{DrawItem, Engine};

fn read_corpus(rel: &str) -> Vec<u8> {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取 {}: {e}", p.display()))
}

/// 取顯示列表全部 Image 項（去掉像素位元組，只留幾何）。
fn images(dl: &quire_engine::PageDisplayList) -> Vec<(f32, f32, f32, f32)> {
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Image { x, y, w, h, .. } => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .collect()
}

/// 找與給定矩形最接近的一項（±容差由呼叫端把關）。
fn nearest(hay: &[(f32, f32, f32, f32)], want: (f32, f32, f32, f32)) -> (f32, usize) {
    hay.iter()
        .enumerate()
        .fold((f32::MAX, usize::MAX), |acc, (i, &(x, y, w, h))| {
            let d = (x - want.0)
                .abs()
                .max((y - want.1).abs())
                .max((w - want.2).abs())
                .max((h - want.3).abs());
            if d < acc.0 {
                (d, i)
            } else {
                acc
            }
        })
}

/// 燈 1（主證，先紅後綠）：fdo76591 第二個框內圖框 `_x0000_s1039`
/// （VML 宣告 `margin-left:220.05pt;margin-top:554.45pt;width:99.95pt;height:100.2pt`，
/// 換算頁絕對 (256.05, 590.45)）之**框內 w:drawing 內容圖**必按
/// 「框原點＋VML 預設內縮」發射。
///
/// LO pymupdf 真值（`docs/d9b-inset-redlight.md` §1）：
/// `bbox = [264.00, 594.00, 348.00, 687.00]`，w=84.00 h=93.00。
///
/// D9 態（`D9B_INSET_BUGGY=1`）：發射 (256.05, 590.45) ⇒ 偏 LO 7.95pt／3.55pt
/// （該框從不被 D9 的字面量臂觸及，故從來沒人修過）。
/// 本卡態：發射 (263.25, 594.05) ⇒ 偏 LO 0.75pt／0.05pt。
#[test]
fn d9b_lamp1_textbox_picture_follows_inset_from_source() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");
    let imgs = images(&dl);

    // LO 真值（實測，非推導）：LO pymupdf get_image_rects(xref=50)。
    let lo = (264.00f32, 594.00f32, 84.00f32, 93.00f32);
    let (dev, idx) = nearest(&imgs, lo);
    assert!(idx != usize::MAX, "顯示列表必須有 Image 項");
    let got = imgs[idx];
    assert!(
        dev <= 2.0,
        "框內圖 (_x0000_s1039) 未依源檔 inset 排版：實得 ({:.2},{:.2},{:.2},{:.2}) \
         偏 LO 真值 ({:.2},{:.2},{:.2},{:.2}) 達 {:.2}pt（容差 2.0pt）",
        got.0,
        got.1,
        got.2,
        got.3,
        lo.0,
        lo.1,
        lo.2,
        lo.3,
        dev
    );
}

/// 燈 2（零回退護欄）：D9 修好的頁腳 CHRYSLER logo 仍對 LO 真值
/// `y = 732.80pt`（±2.0pt），且仍繪於 4pt 外框底線**之上**。
/// 本燈證明「把字面量換成源檔資料」沒有把 D9 的三盞燈拖離 LO——
/// fdo76591 logo 框 `v:textbox` 未宣告 `@inset`，走的是**格式預設**臂。
#[test]
fn d9b_lamp2_footer_logo_still_on_frame_line() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");

    let (logo_idx, logo_y) = dl
        .items
        .iter()
        .enumerate()
        .find_map(|(i, it)| match it {
            DrawItem::Image { y, w, .. } if *w > 200.0 && *y > 700.0 => Some((i, *y)),
            _ => None,
        })
        .expect("必須存在 CHRYSLER logo Image");
    let (line_idx, _) = dl
        .items
        .iter()
        .enumerate()
        .find_map(|(i, it)| match it {
            DrawItem::Rule {
                y1, y2, width_pt, ..
            } if (*width_pt - 4.0).abs() < 0.1 && (y1 - y2).abs() < 0.01 && *y1 > 700.0 => {
                Some((i, *y1))
            }
            _ => None,
        })
        .expect("必須存在 4pt 外框底邊 Rule");
    assert!(
        logo_idx > line_idx,
        "logo (idx={logo_idx}) 又繪於外框底線 (idx={line_idx}) 之下——z 序回退"
    );
    assert!(
        (logo_y - 732.80).abs() <= 2.0,
        "logo 頂緣 ({logo_y:.2}pt) 偏離 LO 真值 732.80pt 超過 2.0pt"
    );
}

/// 燈 3（防調參：宣告 0 ≠ 未宣告預設）：n792778 的浮動群組五框**自訂宣告**
/// `v:textbox/@inset="0,0,0,0"`。LO 依宣告把框內圖放在**框原點**（不移位）。
/// 若實作把「未宣告預設 7.2/3.6」套到已宣告 0 的框，本燈位移 7.2pt 必紅。
///
/// 本燈只斷言**我方實際發射的那張**（群組首子件）：實測 (43.20,374.40,130.90,119.70)
/// 對 LO 首張 (43.25,374.35,131.15,122.40) 偏 2.70pt ⇒ 宣告 0 確實「不移位」，
/// 未被 VML 預設 7.2pt 抹平（若抹平則 x 變 50.40、偏 LO 7.15pt > 容差 4.0pt 必紅——
/// 本燈對「宣告值被預設值抹平」這個錯法有鑑別力）。
///
/// 第二張（LO `bbox=[178.45,374.35,309.60,496.75]`）**不在此燈斷言**：我方只發一張，
/// 缺的是 `v:group` 第二子件（`_x0000_s1130`）的圖片發射——屬卡面白紙黑條嚴禁面
/// （VML 採集／`group_shapes` 欄位圖，R35 同帶），且**base 逐位相同**（非本卡回退，
/// 實證 `docs/d9b-inset-redlight.md` §3）⇒ 另掛 `d9b_lamp3b` 欠債盞。
#[test]
fn d9b_lamp3_declared_zero_inset_is_not_the_default() {
    let bytes = read_corpus("ooxmlexport/n792778.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 n792778 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");
    let imgs = images(&dl);
    assert!(!imgs.is_empty(), "n792778 首頁必須有群組圖");

    let lo = (43.25f32, 374.35f32, 131.15f32, 122.40f32);
    let (dev, idx) = nearest(&imgs, lo);
    assert!(idx != usize::MAX, "缺 LO 真值圖 {lo:?}；實得 {imgs:?}");
    assert!(
        dev <= 4.0,
        "宣告 inset=0 的群組圖未貼框原點：偏 LO {lo:?} 達 {dev:.2}pt（容差 4.0pt）\
         ——疑似把 VML 預設內縮 7.2pt 誤套到已宣告 0 的框",
    );
}

/// 燈 3b（本卡收案盞，原欠債盞）：n792778 群組**第二張**圖。
///
/// 原缺口（`docs/d9b-inset-redlight.md` §3）：`PageFloatingImage` 只收到群組外框
/// `(43.20,374.40,676.80,122.40)`＋`inset=Some([0,0,0,0])`；兩張真圖屬 `v:group`
/// 兩個子件（`_x0000_s1127`／`_x0000_s1130`，皆無 `v:imagedata`、像素來自框內
/// `w:drawing`），其中第二子件的圖未被發射。**非 inset 路徑缺陷**（首張已命中），
/// 且 merge-base binary 逐位相同＝既有缺口。
/// 收案（t_6aade0f3）：採集層 `VmlGroupPicture`（`core/ooxml/src/drawing.rs`）＋
/// 發射層 `emit_vml_group_pics`（`core/engine-api/src/lib.rs`）逐子件發射。
/// 鑑別力回塞鎖：`D9B_GROUP_PIC_BUGGY=1` 整臂關閉＝舊單發態，本燈必紅
/// （實跑記錄見 `docs/d9b-inset-redlight.md` §3b）。
#[test]
fn d9b_lamp3b_group_second_child_picture_emitted() {
    let bytes = read_corpus("ooxmlexport/n792778.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 n792778 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");
    let imgs = images(&dl);
    let lo = (178.45f32, 374.35f32, 131.15f32, 122.40f32);
    let (dev, idx) = nearest(&imgs, lo);
    assert!(idx != usize::MAX, "缺 LO 真值圖 {lo:?}；實得 {imgs:?}");
    assert!(
        dev <= 4.0,
        "群組第二張圖偏 LO 真值達 {dev:.2}pt（容差 4.0pt）"
    );
    // 守恆鎖：LO p0 實測恰好兩張（探針 scratch_d9b/lamp3_diag.py），我方不得
    // 多發第三張（接管臂若漏去重會在框原點重複宿主單發那張）。
    assert_eq!(
        imgs.len(),
        2,
        "n792778 p0 應恰好兩張圖（LO pymupdf 實測 2 張）；實得 {imgs:?}"
    );
}

/// 燈 4（防調參第二檔）：tdf102466 頁首 `word/header1.xml` 的框**自訂宣告**
/// `v:textbox/@inset="0in,0in,0in,0in"`，框內 `w:drawing` 圖。
/// LO 真值（實測，第 2 頁）：`bbox=[85.05,35.35,208.75,63.80]` w=123.70 h=28.45。
/// 同燈 3 語義：宣告 0 ⇒ 不移位；新臂亦不得越界套進頁首發射路徑。
#[test]
fn d9b_lamp4_header_declared_zero_inset() {
    let bytes = read_corpus("ooxmlexport/tdf102466.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 tdf102466 失敗");
    let dl = eng.page_display_list(1).expect("第 2 頁存在");
    let imgs = images(&dl);
    assert!(!imgs.is_empty(), "tdf102466 p1 必須有頁首圖");
    let lo = (85.05f32, 35.35f32, 123.70f32, 28.45f32);
    let (dev, idx) = nearest(&imgs, lo);
    assert!(idx != usize::MAX, "缺 LO 真值圖 {lo:?}；實得 {imgs:?}");
    assert!(
        dev <= 4.0,
        "頁首宣告 inset=0 的框內圖偏 LO 真值達 {dev:.2}pt（容差 4.0pt）",
    );
}

/// 燈 5（契約鎖）：inset 事實必須**沿投影層送到發射層**，且「已宣告」與
/// 「未宣告」在資料層可區分（Some/None），不得在排版層抹平成同一個字面量。
/// fdo76591 兩框實測皆**未宣告** `@inset`（源檔實證見紅燈報告 §1）⇒ 期望
/// `txbx_inset == None` ＋ `txbx_content_pic == true`。
#[test]
fn d9b_lamp5_inset_fact_survives_to_emission_layer() {
    let bytes = read_corpus("ooxmlexport/fdo76591.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 fdo76591 失敗");
    let pg = eng.page_floating_images(0).expect("p0 存在");
    let pics: Vec<&quire_layout::PageFloatingImage> =
        pg.iter().filter(|f| f.txbx_content_pic).collect();
    assert_eq!(
        pics.len(),
        2,
        "fdo76591 应有两个「框内 w:drawing 内容图」浮动态；实得 {}",
        pics.len()
    );
    for f in pics {
        assert!(
            f.txbx_inset.is_none(),
            "源檔该框未宣告 v:textbox/@inset，資料層必須如实為 None（實得 {:?}）\
             ——Some/None 的區分是「宣告值 vs 格式預設」的唯一根據",
            f.txbx_inset
        );
    }
}

/// 欠債燈（老實掛帳，不髒零回退閘門）：tdf48658_transparentOLEheader 的頁首框
/// `word/header2.xml` 自訂宣告 `v:textbox/@inset="0,0,0,0"`、框內 `w:txbxContent`
/// 的表格中掛 `w:drawing` 圖；LO pymupdf 實測首頁該圖
/// `bbox=[61.90,52.55,80.65,66.05]`，我方首頁顯示列表**零 Image**（頁首發射路徑
/// 不認框內 w:drawing 內容圖——同 E2c run166 抓到的 `push_hf_slot` 缺口家族）。
/// 解鎖條件：頁首發射路徑補上框內圖分支（`core/engine-api/src/lib.rs`
/// `push_hf_slot`）＋套用宿主框 inset；見 `docs/d9b-inset-redlight.md` §4。
#[test]
#[ignore = "欠債：頁首框內 w:drawing 內容圖零發射（push_hf_slot 無浮動框分支）；見 docs/d9b-inset-redlight.md §4"]
fn d9b_lamp6_header_textbox_picture_emitted() {
    let bytes = read_corpus("ooxmlimport/tdf48658_transparentOLEheader.docx");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟 tdf48658 失敗");
    let dl = eng.page_display_list(0).expect("首頁存在");
    let imgs = images(&dl);
    let lo = (61.90f32, 52.55f32, 18.75f32, 13.50f32);
    let (dev, idx) = nearest(&imgs, lo);
    assert!(idx != usize::MAX, "頁首框內圖未發射；實得 {imgs:?}");
    assert!(
        dev <= 4.0,
        "頁首框內圖偏 LO 真值達 {dev:.2}pt（容差 4.0pt）"
    );
}
