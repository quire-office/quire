//! ODF 雙走查器口徑對齊考卷（卡 t_4006056b）：`body.rs::extract_body_from_bytes`
//! 與 `layout_glue.rs::walk_paragraphs` 對同一 `content.xml` 必須產出**逐字元一致**
//! 的段落文字流——兩者同吃 `ws.rs` 的單一正規化實作＋同一 gap/pending 狀態機。
//!
//! 缺陷（agy 覆核＋PM 讀碼坐實，t_ae9a9f22 合併後仍存在的口徑分岔）：
//! 1. `body.rs` 的 `Event::Text` 直接 `push_str(&unescaped)`，零空白正規化 ⇒
//!    pretty-print 檔兩口徑每檔差可達數十至數百碼點（字數統計 vs 渲染對不上）。
//! 2. 兩走查器對 `Event::CData` 都落在 `_ => {}` 通配分支**靜默丟棄**——含
//!    CDATA 的 content.xml 內文直接消失，零報錯（corpus-odt 零命中，非急癥，
//!    屬正確性缺口）。
//!
//! 四盞燈：
//! 1. 口徑對等（A／B／C 三組 fixture）：段數、逐段文字、三個計數欄位全等，
//!    且**逐段釘死預期字串**（防「兩側同錯也過」）。修前必紅。
//! 2. CData 不丟：`<text:p><![CDATA[甲 <b> & 乙]]></text:p>` 兩軌都必須含
//!    `甲 <b> & 乙`（CData 內不解析實體，`&` 原樣保留）。修前必紅（整段消失）。
//! 3. NBSP 隔離同口徑：text 節點內、CData 內、NBSP 緊鄰硬換行邊界三種位置的
//!    `\u{00a0}` 兩側走查器都不動。
//! 4. 回塞鑑別：`ODF_BODY_WS_BUGGY=1`（body 側回到零正規化＋CData 丟棄舊路徑）
//!    時燈 1–3 的 body 側斷言必紅，證明本卷真的在看 body.rs 的對齊臂；正常態
//!    必綠。兩態實跑腳本：`scripts/odf_walker_parity_buggy_discrimination.sh`。
//!    本燈自己帶 env 判定（燈 4 在回塞態反過來斷言「兩軌必須不相等」）。
//!
//! 管轄邊界（不在本卷射程）：`text:section` 隱藏（E2b）與 draw 替代文字（E2c）
//! 只在 project 側抑制——body.rs 是「原始計數＋最小內文」口徑，刻意保留隱藏段
//! 文字以對齊 survey.json，不屬本卡的空白／CData 口徑分岔，本卷 fixture 不引入
//! 該形（引入即比錯東西）。
//!
//! fixture 合成方式（「嚴禁 corpus 新檔」鐵律下的零新增資產路徑）：開 corpus-odt
//! 現有檔作**封裝殼**（mimetype／manifest 齊備，`OdfBundle::open` 必然合規），經
//! `ZipBundle::serialize(changes)` 把 `content.xml` 條目整個換成測試內容；測試再
//! 從同一 bundle 讀出 `content.xml` 位元組餵 `project_bytes`，`extract_body()` 讀
//! 的也是同一條目——**兩走查器吃的是同一份位元組**，對等斷言才有效力。基殼檔未被
//! 修改，未新增任何檔案。

use std::collections::BTreeMap;

use quire_odf::OdfBundle;
use quire_ooxml::{Change, ZipBundle};

/// 封裝殼基殼（corpus-odt 現有檔，零新增資產）。
const SHELL_ODT: &str = "ext/lo_core_sw_qa_odfimport_ooo32780-1.odt";

/// NBSP：XML 空白四碼（空格／TAB／CR／LF）**之外**的正文字元，空白政策不得動它。
const NB: char = '\u{00a0}';

fn corpus_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-odt")
}

/// 以 corpus 基殼合成「`content.xml` 換成 `body_xml`」的封裝，回傳
/// （content.xml 位元組, 已開好的 bundle）。見檔頭說明：兩走查器讀同一份位元組。
fn synthesized(body_xml: &str) -> (Vec<u8>, OdfBundle) {
    let path = corpus_root().join(SHELL_ODT);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("讀取基殼失敗: {e}"));
    let zip = ZipBundle::open(&bytes).expect("基殼 zip 開啟失敗");
    let mut changes: BTreeMap<String, Change> = BTreeMap::new();
    changes.insert(
        "content.xml".to_string(),
        Change::Replace(body_xml.as_bytes().to_vec()),
    );
    let out = zip
        .serialize(&changes, &BTreeMap::new())
        .expect("合成封裝 serialize 失敗");
    let bundle = OdfBundle::open(&out).expect("合成封裝開啟失敗");
    let content = bundle
        .read_entry("content.xml")
        .expect("讀回合成 content.xml 失敗");
    assert_eq!(
        content,
        body_xml.as_bytes(),
        "合成後的 content.xml 必須逐位元組等於測試內容（否則兩軌吃的不是同一份）"
    );
    (content, bundle)
}

/// 最小合法 `content.xml` 殼（前綴宣告比照 `odf_xml_whitespace_policy`；
/// `walk_paragraphs` 按 `text:` 限定名比對，前綴不可省）。殼本身就自帶
/// pretty-print 縮排（`\n`＋空格），是口徑分岔的第一層來源。
fn content(inner: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
<office:body>
  <office:text>
{inner}
  </office:text>
</office:body>
</office:document-content>
"#
    )
}

/// 段間分隔符：取 `U+1F`（文字流永不出現的控制字元），串接後相等＝逐段逐字元相等。
const SEP: char = '\u{1f}';

/// 走查器一：`extract_body()` 段落文字串接。
fn body_stream(bundle: &OdfBundle) -> String {
    bundle
        .extract_body()
        .expect("extract_body 失敗")
        .paragraphs
        .iter()
        .map(|p| p.text.as_str())
        .collect::<Vec<_>>()
        .join(&SEP.to_string())
}

/// 走查器二：`project()` 宿主段落文字串接（只取 `paragraphs`；`object_paragraphs`
/// ＝draw:frame 替代文字獨立帳、`hf_masters`＝頁首尾獨立帳，body.rs 本就不產，
/// 納進來比的是計數口徑而非本卡的空白／CData 口徑；本卷 fixture 兩者皆零，
/// 燈 1 的計數斷言另外守著宿主面）。
fn proj_stream(content_xml: &[u8]) -> String {
    quire_odf::project_bytes(content_xml, None)
        .expect("project_bytes 失敗")
        .paragraphs
        .iter()
        .map(|p| p.text.as_str())
        .collect::<Vec<_>>()
        .join(&SEP.to_string())
}

/// 口徑對等的最強形式：段數、逐段文字、`total_text_len`／`text_p_count`／
/// `text_h_count` 全部一致，外加**逐段釘死預期值**（兩側同錯也過不了）。
fn assert_parity(tag: &str, content_xml: &[u8], bundle: &OdfBundle, expected: &[&str]) {
    let body = bundle
        .extract_body()
        .unwrap_or_else(|e| panic!("{tag}：extract_body 失敗: {e}"));
    let proj = quire_odf::project_bytes(content_xml, None)
        .unwrap_or_else(|e| panic!("{tag}：project_bytes 失敗: {e}"));
    let btexts: Vec<&str> = body.paragraphs.iter().map(|x| x.text.as_str()).collect();
    let ptexts: Vec<&str> = proj.paragraphs.iter().map(|x| x.text.as_str()).collect();

    assert_eq!(btexts, expected, "{tag}：body 口徑必須等於釘死的預期段落");
    assert_eq!(
        ptexts, expected,
        "{tag}：project 口徑必須等於釘死的預期段落"
    );
    assert_eq!(
        btexts, ptexts,
        "{tag}：兩走查器逐段逐字元必須一致（body {btexts:?} vs project {ptexts:?}）"
    );
    let joined: usize = expected.iter().map(|s| s.chars().count()).sum();
    assert_eq!(
        body.total_text_len, proj.total_text_len,
        "{tag}：total_text_len 必須一致"
    );
    assert_eq!(
        body.total_text_len, joined,
        "{tag}：total_text_len 必須等於釘死預期的段落碼點和"
    );
    assert_eq!(
        body.text_p_count, proj.text_p_count,
        "{tag}：text_p_count 必須一致"
    );
    assert_eq!(
        body.text_h_count, proj.text_h_count,
        "{tag}：text_h_count 必須一致"
    );
}

/// A 組：pretty-print 縮排（段內首尾換行＋縮排）＋ `text:span` ＋實體引用
/// （`&#160;` 走 GeneralRef 字元參照臂、`&amp;` 走預定義具名臂）。
///
/// 預期口徑（t_ae9a9f22＋t_2a983570 已定案契約，逐段推導）：
/// * 段 1：`前段文字`＋（縮排剝除記 gap ⇒）恰一空格＋`A`＋`&nbsp;`解析值
///   `U+00A0`＋`&`（`&amp;`）＋`B`＋（span 尾縮排記 gap ⇒）恰一空格＋`後段文字`。
/// * 段 2：`甲`＋原樣空格＋`乙`＋原樣空格＋`丙`（純空格節點不折疊）。
const FIX_A_INNER: &str = r#"<text:p>
   前段文字
   <text:span>A&#160;&amp;B</text:span>
   後段文字
 </text:p>
 <text:p>甲 <text:span>乙</text:span> 丙</text:p>"#;
const FIX_A_EXPECTED: &[&str] = &[concat!("前段文字 A", "\u{00a0}", "&B 後段文字"), "甲 乙 丙"];

/// B 組：CData 不丟（卡面燈 2 逐字）＋ CData 左側節點間 gap。
/// 段 2 的 `</text:span>` 與 CDATA 之間有一個純縮排文字節點（`\n`）⇒ 恰一空格。
const FIX_B_INNER: &str = r#"<text:p><![CDATA[甲 <b> & 乙]]></text:p>
 <text:p>甲<text:span>乙</text:span>
<![CDATA[丙丁]]></text:p>"#;
const FIX_B_EXPECTED: &[&str] = &["甲 <b> & 乙", "甲乙 丙丁"];

/// C 組：NBSP 隔離——text 節點內部、CData 內部、NBSP 緊鄰顯式硬換行的邊界，
/// 三種位置的 `\u{00a0}` 皆為正文字元，空白政策與 gap 規則都不得動它。
const FIX_C_INNER: &str = concat!(
    r#"<text:p>"#,
    "\u{00a0}前\u{00a0}中\u{00a0}\u{00a0}後</text:p>\n", // 段首段尾皆 NBSP
    r#" <text:p><![CDATA[甲"#,
    "\u{00a0}",
    r#"乙]]></text:p>"#,
    "\n <text:p>行首",
    "\u{00a0}",
    r#"<text:line-break/><![CDATA["#,
    "\u{00a0}",
    r#"行尾]]></text:p>"#,
);
const FIX_C_EXPECTED: &[&str] = &[
    concat!("\u{00a0}前", "\u{00a0}", "中", "\u{00a0}", "\u{00a0}後"),
    concat!("甲", "\u{00a0}", "乙"),
    concat!("行首", "\u{00a0}", "\n", "\u{00a0}", "行尾"),
];

/// 燈 1：口徑對等——A／B／C 三組 fixture 的 `extract_body()` 與 `project()`
/// 逐段逐字元一致，且逐段等於釘死的預期值。
#[test]
fn lamp1_walker_streams_are_char_identical() {
    for (tag, inner, expected) in [
        ("A 縮排＋span＋實體", FIX_A_INNER, FIX_A_EXPECTED),
        ("B CData", FIX_B_INNER, FIX_B_EXPECTED),
        ("C NBSP", FIX_C_INNER, FIX_C_EXPECTED),
    ] {
        let (xml, bundle) = synthesized(&content(inner));
        assert_parity(tag, &xml, &bundle, expected);
    }
}

/// 燈 1 補強（卡面字面斷言）：段間串接後兩軌逐字元相等，並直接指認修前病徵
/// ——body 側殘留 pretty-print 縮排換行（`!contains('\\n')`）。
#[test]
fn lamp1b_concatenated_streams_equal_and_free_of_formatting_newlines() {
    let (xml, bundle) = synthesized(&content(FIX_A_INNER));
    let bs = body_stream(&bundle);
    let ps = proj_stream(&xml);
    assert_eq!(bs, ps, "A 組：兩走查器串接後必須逐字元相等");
    assert!(
        !bs.contains('\n'),
        "A 組：pretty-print 縮排不得殘留在 body 口徑（實際 {bs:?}）"
    );
    assert!(
        bs.contains('前') && bs.contains('後'),
        "A 組：正文不得被誤砍"
    );
    assert!(
        bs.contains("甲 乙 丙"),
        "A 組：span 兩側純空格須原樣保留為恰一空格（實際 {bs:?}）"
    );
}

/// 燈 2：CData 不丟——兩軌都必須逐字投影 `甲 <b> & 乙`（CData 內不解析實體：
/// `&` 原樣保留；若誤走 unescape 路徑必紅或報錯）。修前 CData 落 `_ => {}`
/// 靜默丟棄 ⇒ 段 1 為空字串 ⇒ 必紅。另釘 CData 左側 gap＝恰一空格。
#[test]
fn lamp2_cdata_is_not_silently_dropped() {
    let (xml, bundle) = synthesized(&content(FIX_B_INNER));
    let body = bundle.extract_body().expect("extract_body 失敗");
    let proj = quire_odf::project_bytes(&xml, None).expect("project_bytes 失敗");

    assert_eq!(
        body.paragraphs[0].text, "甲 <b> & 乙",
        "body 軌：CData 段落必須逐字入文（& 不得被實體化或丟棄）"
    );
    assert_eq!(
        proj.paragraphs[0].text, "甲 <b> & 乙",
        "project 軌：CData 段落必須逐字入文（& 不得被實體化或丟棄）"
    );
    assert_eq!(
        body.paragraphs[1].text, "甲乙 丙丁",
        "body 軌：CData 與相鄰節點間的格式化空白＝恰一個空格"
    );
    assert_eq!(
        proj.paragraphs[1].text, "甲乙 丙丁",
        "project 軌：CData 與相鄰節點間的格式化空白＝恰一個空格"
    );
}

/// 燈 3：NBSP 隔離同口徑——`\u{00a0}` 不是 XML 空白，text 節點內部（含段首段尾
/// 位置）、CData 內部、以及緊鄰顯式硬換行的邊界，兩側走查器都必須原樣保留。
#[test]
fn lamp3_nbsp_untouched_on_both_walkers() {
    let (xml, bundle) = synthesized(&content(FIX_C_INNER));
    let body = bundle.extract_body().expect("extract_body 失敗");
    let proj = quire_odf::project_bytes(&xml, None).expect("project_bytes 失敗");

    assert_eq!(
        body.paragraphs[0].text,
        format!("{NB}前{NB}中{NB}{NB}後"),
        "body 軌：text 節點內與段首段尾的 NBSP 不得動"
    );
    assert_eq!(
        proj.paragraphs[0].text,
        format!("{NB}前{NB}中{NB}{NB}後"),
        "project 軌：text 節點內與段首段尾的 NBSP 不得動"
    );
    assert_eq!(
        body.paragraphs[1].text,
        format!("甲{NB}乙"),
        "body 軌：CData 內 NBSP 不得動"
    );
    assert_eq!(
        proj.paragraphs[1].text,
        format!("甲{NB}乙"),
        "project 軌：CData 內 NBSP 不得動"
    );
    // gap 的「行首」判定只認 `\n`／`\t`（既有口徑，兩側同錯也是對齊）：NBSP
    // 緊鄰硬換行時不得被當成行首空白吞掉。
    assert_eq!(
        body.paragraphs[2].text,
        format!("行首{NB}\n{NB}行尾"),
        "body 軌：NBSP 緊鄰硬換行／CData 邊界不得被誤吞"
    );
    assert_eq!(
        proj.paragraphs[2].text,
        format!("行首{NB}\n{NB}行尾"),
        "project 軌：NBSP 緊鄰硬換行／CData 邊界不得被誤吞"
    );
}

/// 燈 4：回塞鑑別——單側回塞必拍出分岔。`ODF_BODY_WS_BUGGY=1`（body 側零正規化
/// ＋CData 丟棄）或 `ODF_WS_BUGGY=1`（project 側零正規化＋CData 不入文）任一
/// 單獨觸發時，兩軌**必須不相等**（相等＝本燈在看別的東西）；正常態必須相等
/// （不等＝對齊臂壞了，見燈 1）。雙臂同回塞＝兩側同錯（PM 定案「同錯也是對齊」），
/// 不在本燈管轄，直接跳過——該態由燈 1 的逐段釘值臂把關。兩態退出碼由持久腳本
/// `scripts/odf_walker_parity_buggy_discrimination.sh` 實跑。
#[test]
fn lamp4_buggy_flag_discriminates_the_body_normalization_arm() {
    let (xml, bundle) = synthesized(&content(FIX_A_INNER));
    let body_buggy = std::env::var_os("ODF_BODY_WS_BUGGY").is_some();
    let ws_buggy = std::env::var_os("ODF_WS_BUGGY").is_some();
    if body_buggy && ws_buggy {
        return;
    }
    let equal = body_stream(&bundle) == proj_stream(&xml);
    if body_buggy || ws_buggy {
        assert!(
            !equal,
            "單側回塞態：兩軌必須分岔（相等＝正規化臂不在被檢查，鑑別力失效）"
        );
    } else {
        assert!(equal, "正常態：兩軌必須逐字元相等（不等＝body 對齊臂壞了）");
    }
}
