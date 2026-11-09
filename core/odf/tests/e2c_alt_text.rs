//! ODF-E2c 考卷（先紅後綠）：draw:frame 內 svg:desc／svg:title 替代文字不得混入段落正文流。
//!
//! 缺陷真值源：`docs/odf-survey/e2-flow-attribution.md` §2 D3 與 §3.3（PM 以 LO 24.8.3.2 PDF 文字層逐檔對齊）。
//! 段落子樹內的圖形替代文字（svg:desc／svg:title）被 walker 以「p_stack 非空就收文字」收進段落；LO 端 alt-text 永不渲染。
//! 全語料 13/50 檔、未渲染多發 1055 字（上限值）。本檔只鎖投影段落流，逐段做 presence／absence 斷言，不以字數計數代替。
//!
//! 邊界鐵律：不得一律砍掉所有 svg:desc——`draw:text-box` 內的正文（含其巢裝 frame 之外的可見段落）必須原樣保留。
//! 因此本檔除紅燈案例外，另備 text-box／frame 內正文的 presence 鎖，防止過度砍傷。

use quire_odf::{project, OdfBundle};

fn corpus_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-odt")
}

fn open(rel: &str) -> OdfBundle {
    let path = corpus_root().join(rel);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("讀取失敗 {rel}: {e}"));
    OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("開啟失敗 {rel}: {e}"))
}

fn host_flow(rel: &str) -> Vec<String> {
    let p = project(&open(rel)).unwrap_or_else(|e| panic!("{rel} 投影失敗: {e}"));
    p.flatten().0
}

fn assert_absent(rel: &str, needle: &str, where_: &str) {
    let texts = host_flow(rel);
    assert!(
        !texts.iter().any(|t| t.contains(needle)),
        "{rel} 投影段落流仍含替代文字 {needle:?}（{where_}）"
    );
}

fn assert_present(rel: &str, needle: &str, where_: &str) {
    let texts = host_flow(rel);
    assert!(
        texts.iter().any(|t| t.contains(needle)),
        "{rel} 投影段落流遺失正文 {needle:?}（{where_}）"
    );
}

/// 燈 1（首紅例）：mce-nested 的 `Rectangle 366` 替代文字。
///
/// 真值出處：`corpus-odt/lo_out/lo__ooxmlexport__mce-nested.odt` 的 content.xml
/// offset 12694（`draw:custom-shape draw:name="Rectangle 366"` 內 `<svg:desc>Light vertical</svg:desc>`）。
/// 事件序：外層 `text:p` 仍開著 → `draw:custom-shape` → `svg:desc` → Text，故舊 walker 收進投影第 7 段。
/// LO 24.8.3.2 PDF 文字層無此字串（§3.3 閉合：+13 恰等於 gap）。
#[test]
fn lamp1_mce_nested_rect366_desc_absent() {
    let p = project(&open("lo_out/lo__ooxmlexport__mce-nested.odt")).expect("投影失敗");
    let texts = p.flatten().0;
    assert!(
        !texts.iter().any(|t| t.contains("Light vertical")),
        "mce-nested 投影仍含 Rectangle 366 的 svg:desc（content.xml offset 12694）"
    );
    assert_eq!(
        p.paragraph_count(),
        8,
        "mce-nested 段落數不得因跳過替代文字而變動"
    );
    assert!(
        texts.iter().any(|t| t == "Sample Doc 002"),
        "mce-nested 正文不得被誤砍"
    );
}

/// 燈 2：bug59058 全部 svg:desc 字串逐一字串 absence。
///
/// 真值出處：`corpus-odt/lo_out/lo__poi__test-data_document_bug59058.odt` 的 content.xml：
/// spacer.gif 於 offset 113120（重複 7 次）、tab1 於 offset 251104、tab2 於 offset 251976、
/// tab3 於 offset 277570、258981.fig.001 於 offset 295818、258981.fig.002 於 offset 296650、
/// tab4 於 offset 299533、tab5 於 offset 302911、fig3 於 offset 303885、
/// Line graph… 於 offset 442423、Skip Main Navigation 於 offset 448779、The Lancet Oncology 於 offset 449287。
/// 上述字面在 content.xml 全文只出現於 svg:desc，正文無同字，故皆可作 absence 鎖（最大單檔未渲染 459 字級）。
#[test]
fn lamp2_bug59058_desc_strings_absent() {
    const REL: &str = "lo_out/lo__poi__test-data_document_bug59058.odt";
    let descs = [
        "http://ww5.komen.org/images/popup/spacer.gif",
        "tab1",
        "tab2",
        "tab3",
        "258981.fig.001",
        "258981.fig.002",
        "tab4",
        "tab5",
        "fig3",
        "Line graph showing Number of New Cases and Deaths per 100,000 from 1975-2011",
        "Skip Main Navigation",
        "The Lancet Oncology",
    ];
    for d in descs {
        assert_absent(REL, d, "svg:desc 替代文字");
    }
}

/// 燈 3（防過度砍）：draw:text-box 內正文必須仍在。
///
/// 真值出處：`corpus-odt/lo_out/lo__poi__test-data_document_checkboxes.odt` 的 content.xml
/// offset 10477：`draw:frame > draw:text-box > text:p` 內的可見文字（LO 會渲染，非 alt-text）。
/// 同時鎖 fdo77887：其 text-box 內巢裝 frame 的 `logo1`（offset 47973）屬替代文字必須消失，
/// 但同一個 text-box 表格內的可見正文（offset 48278 起）必須保留。
#[test]
fn lamp3_text_box_body_preserved() {
    const CHECKBOXES: &str = "lo_out/lo__poi__test-data_document_checkboxes.odt";
    assert_present(
        CHECKBOXES,
        "Test a checkbox within a textbox:",
        "content.xml offset 10477 的 draw:text-box 正文",
    );
    assert_present(
        CHECKBOXES,
        "->",
        "content.xml offset 10477 的 draw:text-box 正文（text:s 後的箭頭）",
    );

    const FDO77887: &str = "lo_out/lo__ooxmlexport__fdo77887.odt";
    assert_absent(
        FDO77887,
        "logo1",
        "text-box 內巢裝 frame 的 svg:desc，offset 47973",
    );
    assert_present(
        FDO77887,
        "INFORME DE ASISTENCIA",
        "content.xml offset 48278 的 draw:text-box 表格正文",
    );
    assert_present(
        FDO77887,
        "ARGUMENTACIÓN JURÍDICA",
        "content.xml offset 48278 的 draw:text-box 表格正文",
    );
}

/// 燈 4（防過度砍）：draw:frame 內含可見正文的檔，正文必須仍在。
///
/// 真值出處：`corpus-odt/lo_out/lo__ooxmlexport__tdf105688.odt` 的 content.xml
/// offset 31244：`draw:frame > draw:text-box > table:table` 內的可見段落文字。
/// 該檔無 svg:desc；本鎖確保跳過替代文字時不會連 frame／text-box 正文一起砍掉。
#[test]
fn lamp4_frame_visible_body_preserved() {
    const REL: &str = "lo_out/lo__ooxmlexport__tdf105688.odt";
    assert_present(
        REL,
        "Xxxxxxx xxxx xxxxxxxx xxxxxxxxxxxxxx",
        "content.xml offset 31244 的 draw:frame 內 text-box 正文",
    );
}

/// 燈 5（容器邊界）：合成 XML 驗證「逐容器判定」而非一律砍。
///
/// 真值口徑：draw:text-box 直屬的 svg:desc 屬正文（LO 會渲染），必須保留；
/// draw:frame／draw:custom-shape 的 svg:desc／svg:title 屬替代文字，必須跳過。
/// 語料無 text-box 直屬 desc 的實例，故以 `project_bytes` 合成最小 content.xml 鎖住此邊界，
/// 防止後續把「跳過替代文字」簡化成無條件砍掉所有 svg:desc。
#[test]
fn lamp5_container_sensitive_alt_text() {
    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content>
<office:body><office:text>
<text:p>外層段落<draw:frame><draw:text-box><text:p>框內正文</text:p><svg:desc>框內說明</svg:desc></draw:text-box></draw:frame><draw:frame><svg:title>外框標題</svg:title><svg:desc>外框說明</svg:desc></draw:frame></text:p>
</office:text></office:body>
</office:document-content>"#;
    let p = quire_odf::project_bytes(XML.as_bytes(), None).expect("合成投影失敗");
    let texts = p.flatten().0;
    assert!(
        texts.iter().any(|t| t.contains("框內正文")),
        "draw:text-box 內正文被誤砍"
    );
    assert!(
        texts.iter().any(|t| t.contains("框內說明")),
        "draw:text-box 直屬 svg:desc 屬正文，不應跳過"
    );
    assert!(
        !texts.iter().any(|t| t.contains("外框說明")),
        "draw:frame 的 svg:desc 替代文字不應混入正文"
    );
    assert!(
        !texts.iter().any(|t| t.contains("外框標題")),
        "draw:frame 的 svg:title 替代文字不應混入正文"
    );
}
