//! ODF-E2d 紅燈考卷（先紅後綠）：styles.xml master-page 頁首／頁尾**段落內容**未投影。
//!
//! 缺陷（真值源 `docs/odf-survey/e2-flow-attribution.md` §2 D4／§3.4／§3.5，卡
//! t_b1c58929）：`project()` 讀 styles.xml 只為字級級聯，master-page 的
//! `style:header`／`style:footer` 段落內容一個不發，而 LO 每頁渲染。ODF-C4 明文
//! 「頁首頁尾內容不做（寫進報告記帳）」——本卡把該記帳缺口落地。
//!
//! ## 口徑（本卡定案，與基準腳本 `odf_e2d_hf_truth.py` 獨立互驗）
//!
//! * **宣告面**（`OdfProjection::hf_masters`）：styles.xml `office:master-styles`
//!   每筆 master-page 的六種槽位（header／-first／-left × footer／-first／-left）
//!   內 `text:p`／`text:h` 逐段投影，**含巢裝表格／文字框內段落**（LO 同渲染——
//!   FDO76312 頁首表格三格實測有渲染）。欄位節點取**快取值**
//!   （`text:page-number` 節點內文字面照收；LO 重算屬 README §11.1 合成軸）。
//! * **發射面**（`assign_hf_pages`）：逐頁指定生效主頁——§19.501
//!   「段落樣式宣告 `@style:master-page-name` ⇒ 該段落處換頁」為主頁鏈分段；
//!   頁序 0 有宣告 `-first` 槽即用 `-first`（**即使內容為空**，LO 實證：
//!   FDO76312／mce-nested 首頁頁首不渲染正頁首文字），否則用 plain；
//!   `-left` 鏡像頁：0 起頁序的奇數頁（＝邏輯偶數頁）取 `-left`、未宣告退
//!   plain（LO 真值 tdf123705：p0/p2 頁尾 plain `Rev…Page 1 of 3`、p1/p3 頁尾
//!   `-left` `…Page 2 of 2`、p1/p3 頁首 `-left` 空宣告⇒零發射）。全語料
//!   `style:mirror` 宣告 0 檔而 LO 仍按偶數頁吃 `-left`⇒不以 mirror 宣告為前提。
//!   宣告了卻該頁未消費的槽計帳 `skipped_left_slots`（空 `-left` 壓 plain 之形）。
//!   空文字段不發（LO 不渲染），槽位存在性由 `slot_count` 保留。
//! * **段序鐵律**：頁首段在該頁正文段**前**、頁尾段在該頁正文段**後**；
//!   宿主流 `paragraphs`／`flatten()`／`total_text_len` **零變動**（E2e 同款
//!   獨立入帳紀律——頁首尾文字不得污染宿主段落流與斷頁口徑）。
//!
//! ## 燈號
//!
//! * 燈 1 strict：頁首段逐字 `This is a header.`（raw 17 字／去空白 14 字），
//!   段序＝頁首段先於該頁正文段。
//! * 燈 2 tdf124637：主頁鏈五頁頁尾字面 `I` `II` `3` `2` `3`（`text:page-number`
//!   快取值）逐頁斷言＋段序（頁尾段在該頁正文段後）。
//! * 燈 3 兩類都要發：字面文字（lvlPicBulletId 頁尾 `\t\t`、tdf123705 頁首
//!   `Holland America Group…`）與欄位快取值（FDO76312 頁尾 `Page 1 of 2`＝
//!   `Page `＋page-number 快取 `1`＋`of `＋page-count 快取 `2`，tail 文字
//!   `of ` 不得漏收——E2 探針正是漏在 tail）。
//! * 燈 4 首頁 `-first` 規則：mce-nested 頁 0 頁首為空（宣告 `header-first`
//!   空段 ⇒ 不發正頁首 `Sample Doc 002`，LO 實測首頁無頁首文字）；同檔頁 1
//!   （若存在）才發 plain。FDO76312 同判。
//! * 燈 5 段序＋零污染全域鎖：全庫逐頁 hf 流中，頁首段恆先於該頁正文段、
//!   頁尾段恆後於該頁正文段；宿主流逐字逐檔與 `paragraphs` 完全一致。
//! * 燈 6 零回退：既有計數口徑逐位不動——全庫 `paragraph_count()` 總和
//!   19_935、`total_text_len` 語料和 591_774（＝合併態 main 實測，含上游 E2b／
//!   E2c 抑制發射 −2_128；分支 vs main 逐檔逐位相同 ⇒ 本卡貢獻嚴格為零）；
//!   hf 為獨立帳（`hf_masters`／`hf_switches`／`HfAssignment`）。
//! * 燈 7 全語料逐檔逐槽對齊**獨立第二雙眼**基準 `fixtures/e2d_hf_truth.json`
//!   （生成器 `odf_e2d_hf_truth.py`，ElementTree 實作，紅燈 commit 同入）。
//! * 燈 8（鑑別力回塞鎖）：`ODF_E2D_BUGGY=1`＝「主頁鏈不分段，全文件只用文件級
//!   主頁」（缺陷的相反極端之一）⇒ 燈 2 五頁頁尾必紅、燈 1 仍綠——證明分段
//!   鏈與 per-page 指派真的在被檢查，不是寫死期望值。

use quire_odf::header_footer::{hf_assignment, HfRole, MasterHfSlotKind};
use quire_odf::layout_glue::OdfLayoutParagraph;
use quire_odf::{project, OdfBundle};
use std::path::{Path, PathBuf};

fn repo_file(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn open(rel: &str) -> OdfBundle {
    let bytes = std::fs::read(repo_file(rel)).unwrap_or_else(|e| panic!("讀取 {rel} 失敗: {e}"));
    OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("開封 {rel} 失敗: {e}"))
}

/// 逐檔發射面指派（`hf_assignment` 自行取真實頁面幾何——逐主頁各取 own
/// page-layout，C4 鏈；考卷不代喂單一全域 params，免得把「多主頁多版面」吃掉）。
fn assign(
    rel: &str,
) -> (
    quire_odf::OdfProjection,
    quire_odf::header_footer::HfAssignment,
) {
    let bundle = open(rel);
    let proj = project(&bundle).expect("投影失敗");
    let hf = hf_assignment(&bundle, &proj).expect("頁首尾指派失敗");
    (proj, hf)
}

fn ns(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn texts(ps: &[OdfLayoutParagraph]) -> Vec<String> {
    ps.iter().map(|p| p.text.clone()).collect()
}

const STRICT: &str = "corpus-odt/lo_out/lo__ooxmlexport__strict.odt";
const TDF124637: &str = "corpus-odt/lo_out/lo__ooxmlexport__tdf124637_sectionMargin.odt";
const FDO76312: &str = "corpus-odt/lo_out/lo__ooxmlexport__FDO76312.odt";
const MCE: &str = "corpus-odt/lo_out/lo__ooxmlexport__mce-nested.odt";
const TDF123705: &str = "corpus-odt/lo_out/lo__ooxmlexport__tdf123705.odt";
const LVL_PIC: &str = "corpus-odt/lo_out/lo__ooxmlexport__lvlPicBulletId.odt";

// ---------------------------------------------------------------------------
// 燈 1：strict——頁首段逐字＋段序（頁首段在該頁正文段前）。
// ---------------------------------------------------------------------------

#[test]
fn lamp1_strict_header_paragraph_verbatim_and_order() {
    let (proj, hf) = assign(STRICT);
    // 宣告面：master-page Standard 有 header 槽一段，逐字面。
    let std = proj
        .hf_masters
        .iter()
        .find(|m| m.name == "Standard")
        .expect("strict 應有 master-page Standard");
    let h = std
        .slot(MasterHfSlotKind::Header)
        .expect("Standard 應有 style:header 槽");
    assert_eq!(
        texts(&h.paragraphs),
        vec!["This is a header."],
        "燈1 頁首段逐字"
    );
    assert_eq!(
        ns(&h.paragraphs[0].text).chars().count(),
        14,
        "燈1 去空白 14 字（E2 報告 §3.4 口徑）"
    );
    // 發射面：1 頁、頁首段在該頁正文段前。
    assert_eq!(hf.pages.len(), 1, "燈1 strict 我方分頁應為 1 頁");
    let p0 = &hf.pages[0];
    assert_eq!(p0.master_page.as_deref(), Some("Standard"), "燈1 生效主頁");
    assert_eq!(
        texts(&p0.header),
        vec!["This is a header."],
        "燈1 頁 0 發射頁首"
    );
    let flow = hf.flow(&proj);
    let idx_hf = flow
        .iter()
        .position(|(pg, role, t)| *pg == 0 && *role == HfRole::Header && t == "This is a header.")
        .expect("燈1 頁首段必須出現在 hf 流");
    let idx_body = flow
        .iter()
        .position(|(pg, role, t)| *pg == 0 && *role == HfRole::Body && t == "Hello world!")
        .expect("燈1 正文首段必須出現在 hf 流");
    assert!(idx_hf < idx_body, "燈1 段序：頁首段必須先於該頁正文段");
    // 該頁正文段集合＝宿主段落流的頁 0 切片（零污染）。真值：LO PDF 實測本檔
    // **1 頁**（rect 612×792 Letter），頁首 `This is a header.` 在 y=36.23、正文首段
    // `Hello world!` 在 y=72.23——故頁 0 正文切片＝宿主全部 6 段，逐字取自我方
    // `odt-render` 實測段落流（前 run 拍的 `["Hello world!", "", "", "", "",
    // "This is math:"]` 是臆測值：漏了 `This is a chart:`／`And this is a
    // smart-art:` 兩段、多了一個空段，與宿主流和 LO 版面都不符，已更正）。
    assert_eq!(
        hf.pages[0]
            .body_texts(&proj)
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Hello world!",
            "This is a chart:",
            "",
            "And this is a smart-art:",
            "This is math:",
            ""
        ],
        "燈1 頁 0 正文段逐字＝宿主段落流切片（LO 單頁 ⇒ 全流）"
    );
    // 無頁尾宣告 ⇒ 零發射。
    assert!(p0.footer.is_empty(), "燈1 strict 無頁尾宣告不得發射");
    // 宿主 total_text_len 零變動：65＝修復前實測（E2 報告 §3.4）。
    assert_eq!(proj.total_text_len, 65, "燈1 宿主文字流零污染");
}

// ---------------------------------------------------------------------------
// 燈 2：tdf124637——主頁鏈五頁頁尾字面 I II 3 2 3＋段序（頁尾段在正文段後）。
// ---------------------------------------------------------------------------

#[test]
fn lamp2_tdf124637_master_chain_footers_and_order() {
    let (proj, hf) = assign(TDF124637);
    assert_eq!(hf.pages.len(), 5, "燈2 §19.501 宣告即換頁 ⇒ 我方應分 5 頁");
    let per_page: Vec<Vec<String>> = hf.pages.iter().map(|p| texts(&p.footer)).collect();
    assert_eq!(
        per_page,
        vec![
            vec!["I".to_string()],
            vec!["II".to_string()],
            vec!["3".to_string()],
            vec!["2".to_string()],
            vec!["3".to_string()],
        ],
        "燈2 頁尾字面（text:page-number 快取值）逐頁"
    );
    let masters: Vec<Option<&str>> = hf.pages.iter().map(|p| p.master_page.as_deref()).collect();
    assert_eq!(
        masters,
        vec![
            Some("Standard"),
            Some("Converted1"),
            Some("Converted2"),
            Some("Converted3"),
            Some("Converted4")
        ],
        "燈2 逐頁生效主頁＝宣告即換頁鏈"
    );
    // 段序：每頁頁尾段必須在該頁最後正文段之後、下一頁任何段之前。
    let flow = hf.flow(&proj);
    for pg in 0..hf.pages.len() {
        let hf_idx = flow
            .iter()
            .rposition(|(p, role, _)| *p == pg && *role == HfRole::Footer)
            .unwrap_or_else(|| panic!("燈2 p{pg} 應有頁尾段"));
        let body_last = flow
            .iter()
            .rposition(|(p, role, _)| *p == pg && *role == HfRole::Body)
            .expect("燈2 每頁應有正文段");
        assert!(
            body_last < hf_idx,
            "燈2 p{pg} 段序：頁尾段必須後於該頁正文段"
        );
        let next_first = flow
            .iter()
            .position(|(p, _, _)| *p == pg + 1)
            .unwrap_or(flow.len());
        assert!(hf_idx < next_first, "燈2 p{pg} 頁尾段必須先於下一頁");
    }
}

// ---------------------------------------------------------------------------
// 燈 3：兩類都要發——字面文字 與 欄位快取值（tail 文字不得漏收）。
// ---------------------------------------------------------------------------

#[test]
fn lamp3_literal_text_and_field_cached_values() {
    // 3a 字面文字：lvlPicBulletId 頁尾＝兩個 tab（源檔字面，非欄位）。
    let (_proj, hf) = assign(LVL_PIC);
    let footers: Vec<String> = hf.pages.iter().flat_map(|p| texts(&p.footer)).collect();
    assert!(
        footers.iter().any(|t| t == "\t\t"),
        "燈3a lvlPicBulletId 頁尾字面兩 tab 必須發射，實際 {footers:?}"
    );
    // 3b 字面文字：tdf123705 頁首 `Holland America Group Shorex Vendor Incident Form`。
    let (proj, _hf) = assign(TDF123705);
    let std = proj
        .hf_masters
        .iter()
        .find(|m| m.name == "Standard")
        .expect("tdf123705 應有 Standard");
    let h = std
        .slot(MasterHfSlotKind::Header)
        .expect("tdf123705 Standard 應有 header 槽");
    assert_eq!(
        texts(&h.paragraphs).first().map(String::as_str),
        Some("Holland America Group Shorex Vendor Incident Form"),
        "燈3b 頁首字面逐字（text:span 內文字須拼回）"
    );
    // 3c 欄位快取值＋tail：FDO76312 頁尾 `Page 1 of 2`——
    //     `Page `＋<text:page-number>1</>＋`of `（**tail**）＋<text:page-count>2</>。
    //     E2 探針的抽取正是漏掉 tail 少字（本檔 22→23 字級差），此處釘死。
    let (proj, hf) = assign(FDO76312);
    let std = proj
        .hf_masters
        .iter()
        .find(|m| m.name == "Standard")
        .expect("FDO76312 應有 Standard");
    let f = std
        .slot(MasterHfSlotKind::Footer)
        .expect("FDO76312 Standard 應有 footer 槽");
    assert_eq!(
        texts(&f.paragraphs).first().map(String::as_str),
        Some("Page 1 of 2"),
        "燈3c 欄位快取值＋tail 文字逐字"
    );
    // 頁 0 用 -first（空）⇒ 不發 plain 頁尾；頁尾文字不得出現在頁 0。
    assert!(
        hf.pages[0].footer.is_empty(),
        "燈3c 頁 0 有 header-first/footer-first 宣告 ⇒ 不得發 plain 頁尾"
    );
}

// ---------------------------------------------------------------------------
// 燈 4：首頁 -first 規則（LO 實證：首頁不吃 plain 頁首）。
// ---------------------------------------------------------------------------

#[test]
fn lamp4_first_page_uses_first_slot_even_when_empty() {
    // mce-nested：header-first 存在且為空段 ⇒ 頁 0 頁首零發射；
    // header(plain) 的 `Sample Doc 002` 只屬非首頁（E2 報告 §3.3 實測 LO 首頁無頁首）。
    let (proj, hf) = assign(MCE);
    let std = proj
        .hf_masters
        .iter()
        .find(|m| m.name == "Standard")
        .expect("mce-nested 應有 Standard");
    let hf_first = std
        .slot(MasterHfSlotKind::HeaderFirst)
        .expect("mce-nested Standard 應有 header-first 槽");
    assert!(
        hf_first.paragraphs.iter().all(|p| p.text.is_empty()),
        "燈4 header-first 應為空段宣告"
    );
    let plain = std
        .slot(MasterHfSlotKind::Header)
        .expect("mce-nested Standard 應有 header 槽");
    assert_eq!(
        texts(&plain.paragraphs).first().map(String::as_str),
        Some("Sample Doc 002"),
        "燈4 plain 頁首逐字"
    );
    assert!(
        hf.pages[0].header.is_empty(),
        "燈4 頁 0 頁首必須零發射（LO 首頁不吃 plain 頁首）"
    );
    for p in hf.pages.iter().skip(1) {
        assert_eq!(
            texts(&p.header).first().map(String::as_str),
            Some("Sample Doc 002"),
            "燈4 非首頁頁首必須發 plain 頁首"
        );
    }
    // absence 鎖：頁 0 流裡不得出現 `Sample Doc 002` 的頁首角色。
    let flow = hf.flow(&proj);
    assert!(
        !flow
            .iter()
            .any(|(pg, role, t)| *pg == 0 && *role == HfRole::Header && t.contains("Sample Doc")),
        "燈4 頁 0 頁首角色不得混入 plain 頁首文字"
    );
}

// ---------------------------------------------------------------------------
// 燈 5：段序＋零污染全域鎖（全庫 48 檔）。
// ---------------------------------------------------------------------------

#[test]
fn lamp5_corpus_ordering_and_host_flow_untouched() {
    let mut checked_pages = 0usize;
    let mut checked_hf_items = 0usize;
    for (rel, bundle) in corpus_projectable() {
        let proj = project(&bundle).unwrap_or_else(|e| panic!("{rel} 投影失敗: {e}"));
        let hf = hf_assignment(&bundle, &proj).expect("{rel} 頁首尾指派失敗");
        let flow = hf.flow(&proj);
        // (a) 宿主流逐字＝paragraphs 原序（零污染：Body 角色逐段拼接等於 flatten）。
        let body_stream: Vec<String> = flow
            .iter()
            .filter(|(_, role, _)| *role == HfRole::Body)
            .map(|(_, _, t)| t.clone())
            .collect();
        let (host, _) = proj.flatten();
        assert_eq!(body_stream, host, "{rel}：hf 流的正文段必須逐字等於宿主流");
        // (b) 逐頁段序：Header* → Body* → Footer*（同頁內角色單調）。
        let mut rank = 0u8;
        let mut cur_page: Option<usize> = None;
        for (pg, role, _) in &flow {
            if cur_page != Some(*pg) {
                cur_page = Some(*pg);
                rank = 0;
            }
            let r = match role {
                HfRole::Header => 0u8,
                HfRole::Body => 1,
                HfRole::Footer => 2,
            };
            assert!(
                r >= rank,
                "{rel} p{pg}：段序違反（{role:?} 出現在更後面角色之後）"
            );
            rank = r;
            if *role != HfRole::Body {
                checked_hf_items += 1;
            }
        }
        checked_pages += hf.pages.len();
        // (c) 空文字段不發（LO 不渲染）。
        for p in &hf.pages {
            assert!(
                p.header.iter().all(|x| !x.text.is_empty())
                    && p.footer.iter().all(|x| !x.text.is_empty()),
                "{rel}：空文字頁首尾段不得發射"
            );
        }
    }
    assert!(checked_pages >= 60, "燈5 實測頁數過少（{checked_pages}）");
    assert!(
        checked_hf_items >= 40,
        "燈5 實測 hf 項過少（{checked_hf_items}），斷言可能空轉"
    );
}

// ---------------------------------------------------------------------------
// 燈 6：零回退——宿主計數口徑逐位不動（修復前實測釘）。
// ---------------------------------------------------------------------------

/// 全庫宿主段落總數基準（ODF-C3 口徑，odf_d1_display_list 同值釘）。
const CORPUS_PARAGRAPH_BASELINE: usize = 19_935;
/// 全庫宿主 `total_text_len` 基準＝**合併態 main（含 E2b／E2c）實測**（48 明文
/// 檔，`odt-render` 逐檔求和；PM 覆核 `probe/`＋逐檔對帳腳本）。
///
/// 沿革（三值皆有出處，禁臆測）：
/// * 65_569＝前 run 臆測值，實為 strict.odt 附近量級的**單檔**數，非全庫和。
/// * 593_902＝本卡動工前基線（CLI 全語料實測＝`docs/odf-e2a-redlight.md` §合計
///   表雙線鎖死），當時 main 尚未有 E2b／E2c。
/// * 591_774＝上游 `a0a2d82`（E2b 隱藏 section／E2c alt-text 抑制
///   正文發射）合法扣掉 2_128 字。PM 零污染鐵證：分支態 vs main **逐檔逐位元組
///   相同**（57 檔差異檔數＝0），故本卡對宿主計數的貢獻嚴格為零——此欄更新只
///   反映上游合法演進，不稀釋「E2d 不動宿主計數」鐵律（段落數 19_935 逐位未動
///   即 ODF-C3 契約面同證）。
/// * 591_699＝t_ae9a9f22（正文投影剝除 XML 格式化空白）合法扣掉 75 字，
///   PM 裁可（2026-11-11）。PM 獨立覆核（merge-base `aa9d04b` base binary vs
///   分支 HEAD `0b80261` 逐檔 A/B）：51 可比檔中 9 檔下降合計 −75、**零檔上升**，
///   逐檔差值與卡面 comment 列表一致；段落數 19_935（48 檔口徑）逐位未動，
///   即正規化只剝格式化空白、不動段落數，ODF-C3 契約面同證。
/// * **591_763＝現行值**：t_2a983570（跨節點黏連返修：文字節點首／尾被剝除的
///   格式化空白改記 pending_gap＋純空格節點不再折疊）合法回升 64 字，PM 裁可
///   （2026-11-12）。PM 獨立覆核（merge-base `7c2c0a9` base binary vs 合併 tip
///   `1a33f81` 逐檔 A/B，48 可比檔）：**8 檔上升合計 +64、零檔下降**（逐檔值：
///   form_footnotes +25／drawing +17／bug65649 +12／tdf104061 +4／tdf152710 +2／
///   tdf123705 +2／fdo78420 +1／checkboxes +1），與卡面 comment 八檔列表逐位一致；
///   段落數 19_935（48 檔口徑）逐位未動。方向正確——被剝的邊界空白回歸成 gap
///   空格、被誤折的純空格節點回歸第二個空格字形，兩者都是往 LO 真值靠攏。
const CORPUS_TOTAL_TEXT_BASELINE: usize = 591_763;

#[test]
fn lamp6_host_counting_zero_regression() {
    let mut paras = 0usize;
    let mut chars = 0usize;
    let mut files = 0usize;
    for (rel, bundle) in corpus_projectable() {
        let proj = project(&bundle).unwrap_or_else(|e| panic!("{rel} 投影失敗: {e}"));
        paras += proj.paragraph_count();
        chars += proj.total_text_len;
        files += 1;
    }
    assert_eq!(files, 48, "燈6 明文檔數應為 48");
    assert_eq!(paras, CORPUS_PARAGRAPH_BASELINE, "燈6 宿主段落總數回退");
    assert_eq!(chars, CORPUS_TOTAL_TEXT_BASELINE, "燈6 宿主字數回退");
}

// ---------------------------------------------------------------------------
// 燈 7：全語料逐檔逐槽對齊獨立第二雙眼基準（fixtures/e2d_hf_truth.json）。
// ---------------------------------------------------------------------------

/// 基準檔（生成器 `core/odf/tests/odf_e2d_hf_truth.py`，ElementTree 獨立實作）。
#[derive(serde::Deserialize)]
struct TruthFile {
    masters: Vec<TruthMaster>,
}
#[derive(serde::Deserialize)]
struct TruthMaster {
    name: String,
    slots: Vec<TruthSlot>,
}
#[derive(serde::Deserialize)]
struct TruthSlot {
    kind: String,
    paragraphs: Vec<TruthPara>,
}
#[derive(serde::Deserialize)]
struct TruthPara {
    text: String,
    size_pt: f32,
}

fn truth_census() -> std::collections::BTreeMap<String, TruthFile> {
    let p = repo_file("core/odf/tests/fixtures/e2d_hf_truth.json");
    let s =
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("基準檔缺失 {}: {e}", p.display()));
    serde_json::from_str(&s).expect("基準檔應為合法 JSON")
}

#[test]
fn lamp7_corpus_matches_independent_python_baseline() {
    let truth = truth_census();
    let mut checked_masters = 0usize;
    let mut checked_paras = 0usize;
    for (rel, bundle) in corpus_projectable() {
        let proj = project(&bundle).unwrap_or_else(|e| panic!("{rel} 投影失敗: {e}"));
        let key = rel.strip_prefix("corpus-odt/").unwrap_or(&rel).to_string();
        let tf = truth.get(&key).unwrap_or_else(|| panic!("基準缺檔 {key}"));
        assert_eq!(
            proj.hf_masters.len(),
            tf.masters.len(),
            "{rel}：master 筆數"
        );
        for (mine, want) in proj.hf_masters.iter().zip(&tf.masters) {
            assert_eq!(mine.name, want.name, "{rel}：master 名");
            assert_eq!(
                mine.slots.len(),
                want.slots.len(),
                "{rel} {}：槽數",
                mine.name
            );
            for (ms, ws) in mine.slots.iter().zip(&want.slots) {
                assert_eq!(ms.kind.as_str(), ws.kind, "{rel} {}：槽別", mine.name);
                assert_eq!(
                    ms.paragraphs.len(),
                    ws.paragraphs.len(),
                    "{rel} {} {}：段數",
                    mine.name,
                    ms.kind.as_str()
                );
                for (mp, wp) in ms.paragraphs.iter().zip(&ws.paragraphs) {
                    assert_eq!(
                        mp.text,
                        wp.text,
                        "{rel} {} {}：段文字",
                        mine.name,
                        ms.kind.as_str()
                    );
                    assert!(
                        (mp.font_size_pt() - wp.size_pt).abs() <= 0.01,
                        "{rel} {} {}：字級 {} vs {}",
                        mine.name,
                        ms.kind.as_str(),
                        mp.font_size_pt(),
                        wp.size_pt
                    );
                    checked_paras += 1;
                }
                checked_masters += 1;
            }
        }
    }
    assert!(
        checked_masters >= 80,
        "燈7 實測槽數過少（{checked_masters}）"
    );
    assert!(checked_paras >= 150, "燈7 實測段數過少（{checked_paras}）");
}

// ---------------------------------------------------------------------------
// 燈 8：回塞鑑別鎖——ODF_E2D_BUGGY=1（主頁鏈不分段，全文件只認文件級主頁）
// ⇒ 燈 2 形態必紅、燈 1 形態仍綠（證明分段鏈真的在被檢查）。
// ---------------------------------------------------------------------------

#[test]
fn lamp8_buggy_single_master_is_distinguishable() {
    let buggy = quire_odf::header_footer::buggy_single_master();
    let (_proj, hf) = assign(TDF124637);
    let footers: Vec<String> = hf.pages.iter().flat_map(|p| texts(&p.footer)).collect();
    if buggy {
        // 回塞態：不分段 ⇒ 五頁頁尾全同一主頁文字（燈 2 斷言必然成立不了）。
        assert!(
            footers.iter().all(|t| t == &footers[0]) && footers[0] == "I",
            "回塞態應退化為單一主頁頁尾，實際 {footers:?}"
        );
    } else {
        assert_eq!(footers, vec!["I", "II", "3", "2", "3"], "正常態燈2形態");
    }
    // 燈 1 形態兩態皆綠（單主頁檔不受分段影響）。
    let (_p2, hf2) = assign(STRICT);
    assert_eq!(
        hf2.pages[0]
            .header
            .first()
            .map(|p| p.text.as_str())
            .unwrap_or(""),
        "This is a header.",
        "燈1 形態在兩態都必須成立"
    );
}

// ---------------------------------------------------------------------------
// 共用：可投影語料（48 明文檔，加密 2 檔剔除）。
// ---------------------------------------------------------------------------

fn corpus_projectable() -> Vec<(String, OdfBundle)> {
    let manifest = repo_file("corpus-odt/manifest.json");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest).expect("manifest")).expect("JSON");
    let mut out = Vec::new();
    for rec in v["files"].as_array().expect("files 陣列") {
        let f = rec["file"].as_str().expect("file 欄位").to_string();
        let rel = format!("corpus-odt/{f}");
        let bundle = open(&rel);
        // 加密檔（manifest 宣告 encryption-data）由 project 閘門擋下，跳過。
        if project(&bundle).is_err() {
            continue;
        }
        out.push((rel, bundle));
    }
    out
}
