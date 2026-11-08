//! ODF-E2e 考卷（先紅後綠）：嵌入物件 part（`Object*/content.xml`）文字未投影。
//!
//! 缺陷（真值源 `docs/odf-survey/e2-flow-attribution.md` §2 D5／§3.4，卡
//! t_d0c2fa7b）：`content.xml` 的 `draw:object`（`./Object N`）指向的巢裝
//! part——圖表（`...opendocument.chart`）、數學公式（`...opendocument.formula`）、
//! 巢裝試算表（`...opendocument.spreadsheet`）——我方 walker 從不打開，LO 卻把
//! 其中一部分文字渲染進版面（全語料 5/50 檔、524 字）。本考卷鎖「LO 實際發射
//! 面」的**逐段文字**（presence 與 **absence** 成對），不以字數計數代替。
//!
//! ## 取字鐵律（本卡最重要一條；報告 §3.4 實測＋本卡逐檔像素複測）
//!
//! LO 只渲染 part 文字的**子集**。把整個 chart table 一股腦倒進段落流會讓本席
//! 從 −45 反向超發，故 presence 與 absence 必須同時斷言。六檔 part 的實測發射
//! 面（LO 24.8.0.3 `--convert-to pdf`＋pymupdf 逐行＋150dpi 像素肉眼覆核）：
//!
//! | part | LO 發（我方必須發） | LO 不發（我方必須不發） |
//! |---|---|---|
//! | `strict` Object 2 圖表 `chart:bar`，**無** legend | 標題 `圖表標題`＋類別 `1` `2` `3` | `; `（label-separator）、**序列名 `序列1`**、float 值 `1` `2` `3` |
//! | `strict` Object 3 公式（MathML） | `mi`/`mn`/`mo` 字形串 `A=πr2` | `<annotation>` StarMath 原始碼（`{` `}` `^`） |
//! | `chartup` Object 2 圖表 `chart:line`，有 legend | 標題 `圖表標題`＋序列名 `Series 1..3` | 類別 `41518..`（date serial float，LO 另排 `1-Sep-13`）、數值 `4.3` 等、`; ` |
//! | `fdo76098` Object 2 圖表 `chart:circle`，有 legend | 標題 `Sales`＋類別 `1st Qtr..4th Qtr`（圓餅圖例項就是類別——像素覆核圖例寫季度名） | 序列名 `Sales`（不另發第二次）、數值 `8.2` 等、`; ` |
//! | `solar` Object 2 圖表 `chart:line`，有 legend | 類別 `Mercury..Pluto`＋序列名 3 條（`Perihelion (Closest) (km)` 等大寫形，宿主表格是小寫 `closest`——可判別來源） | 數值 `57909175` 等、`; ` |
//! | `wordatt` Object 1 巢裝試算表 | 非空儲存格 `это ooxml`、`this is ooxml` | 16383 個 repeated 空格 |
//! | `hc031` Object 1 公式（MathML） | 22 個字形串 `(x+a)n=∑k=0n(nk)xkan−k` | `<annotation>`（`{left (x+a right )} …`） |
//!
//! 由此歸納的發射面規則（六 part 全覆蓋、逐檔可偽）：
//! 1. `chart:title` 段落文字必發。
//! 2. `chart:categories` 範圍內的**字串值**儲存格逐格必發（LO 排成類別標籤）；
//!    **浮點值**儲存格不發——LO 依資料自動算刻度／日期（`0.5`、`1-Sep-13`），
//!    那是 LO 合成，README §11 明文非我方目標。
//! 3. 序列名（`chart:series/@chart:label-cell-address`）**僅在**有
//!    `<chart:legend>` **且**圖表類別非 `chart:circle` 時發。三態對照組：
//!    `strict` 無 legend ⇒ `序列1` 不發（卡面鐵律）；`chartup`/`solar` 有 legend
//!    ⇒ 序列名發；`fdo76098` 圓餅圖例排類別 ⇒ 序列名 `Sales` 不重複發。
//! 4. `chart:label-separator`（`; `）、數值欄、空白角落格一律不發。
//! 5. MathML 只取 `mi`/`mn`/`mo` 字形（文件序串接成一段），
//!    `semantics/annotation` 一律不發。
//! 6. 巢裝試算表只取非空儲存格文字（跳過 covered／repeated 空格）。
//!
//! ## 口徑與燈號
//!
//! * `host_flow()`＝宿主段落流（`text:p`＋`text:h`，既有口徑，本卡不得變動）。
//! * `text_flow()`＝對接 LO 的完整文字流。紅燈態（修復前）＝宿主流（物件 part
//!   完全未讀）；綠燈態（修復後）＝宿主流＋物件發射段。**presence 鎖一律打
//!   `text_flow()`**，故修復前必紅、修復後必綠。
//! * absence／計數鎖：以**實測宿主基線計數**為釘（見 `HOST_*` 常數），修復前
//!   即綠、修復後**必須仍**綠——這是反超發臂（報告 §3.4 點名的反向超發事故形態），
//!   也是零誤傷臂（宿主正文本來就有的 `;`（solar 正文 1 個）不得被誤判成物件洩漏）。
//! * 燈 1／2／3／4：四檔圖表＋兩檔公式＋一檔試算表的 presence／absence 成對。
//! * 燈 5：宿主段不得被物件文字污染（逐段原樣、物件段獨立）。
//! * 燈 6：零誤傷——含物件者與無物件者的 `text:p`／`text:h`／`paragraph_count`
//!   必須與 ODF-C3 計數基準逐檔一致（物件段另立帳，不得擠掉宿主計數）。
//! * 燈 7（鑑別力回塞鎖）：`ODF_E2E_BUGGY=1` 把物件取字打回「整個 part 的
//!   `text:p` 一股腦倒進段落流」⇒ 圖表 presence 仍綠、absence／計數釘必紅，
//!   證明 absence 鎖精確命中取字鐵律，不是寫死期望值。該態同時**丟掉** MathML
//!   字形（MathML part 內無 `text:p`，倒 `text:p` 必然零發射）⇒ 燈 4c 在回塞態
//!   連 presence 也紅，正是「一股腦倒 text:p」連公式都保不住的實測寫照。

use quire_odf::OdfBundle;

fn corpus_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-odt")
}

fn open(rel: &str) -> OdfBundle {
    let path = corpus_root().join(rel);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("讀取失敗 {rel}: {e}"));
    OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("開啟失敗 {rel}: {e}"))
}

const STRICT: &str = "lo_out/lo__ooxmlexport__strict.odt";
const CHARTUP: &str =
    "lo_out/lo__xopt__OpenXmlPowerToolsExamples_ChartUpdater01_Chart-Embedded-Xlsx-08.odt";
const FDO76098: &str = "lo_out/lo__ooxmlexport__fdo76098.odt";
const SOLAR: &str =
    "lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_SolarOverview.odt";
const WORDATT: &str = "lo_out/lo__poi__test-data_document_WordWithAttachments.odt";
const HC031: &str = "lo_out/lo__xopt__TestFiles_HC031-Complicated-Document.odt";

/// 宿主段落流（既有 `text:p`＋`text:h` 口徑；本卡不得變動）。
fn host_flow(rel: &str) -> Vec<String> {
    project_host(rel).flatten().0
}

fn project_host(rel: &str) -> quire_odf::OdfProjection {
    quire_odf::project(&open(rel)).unwrap_or_else(|e| panic!("{rel} 投影失敗: {e}"))
}

/// 對接 LO 的完整文字流。紅燈態＝宿主流（物件 part 未讀）；綠燈態＝宿主流＋
/// 物件發射段（見 `object_paragraphs`）。presence 鎖打這條軌。
fn text_flow(rel: &str) -> Vec<String> {
    let p = project_host(rel);
    let mut v = p.flatten().0;
    v.extend(p.object_paragraphs.iter().map(|o| o.text.clone()));
    v
}

fn contains(texts: &[String], needle: &str) -> bool {
    texts.iter().any(|t| t.contains(needle))
}

fn count_of(texts: &[String], needle: &str) -> usize {
    texts.iter().filter(|t| t.contains(needle)).count()
}

fn char_count(texts: &[String], c: char) -> usize {
    texts
        .iter()
        .map(|t| t.chars().filter(|&x| x == c).count())
        .sum()
}

// 實測宿主基線計數（`proj_baseline_pre.json`，修復前抓、修復後必須逐位不變——
// 反超發＋零誤傷雙臂）。solar 正文自帶 1 個 `;`（`…NASA's website);`），
// 故 absence 鎖釘在基線計數而非零，才不會把宿主正文誤判成物件洩漏。
const HOST_SEMI_STRICT: usize = 0;
const HOST_SEMI_CHARTUP: usize = 0;
const HOST_SEMI_FDO: usize = 0;
const HOST_SEMI_SOLAR: usize = 1;
const HOST_SALES_FDO: usize = 0;

// ---------------------------------------------------------------------------
// 燈 1：strict——LO 只渲染子集的鐵律落點（無 legend ⇒ 序列名不發）
// ---------------------------------------------------------------------------

/// presence：圖表標題＋類別標籤＋公式字形必須入流。absence：`;`／序列名／
/// StarMath annotation 一律不得入流。
#[test]
fn lamp1_strict_chart_and_formula_face() {
    let f = text_flow(STRICT);

    assert!(
        contains(&f, "圖表標題"),
        "strict：圖表 part 標題「圖表標題」必須入流（LO 實測渲染，NotoSansTC-Thin \
         14pt，x=287.2 y=152.4）。實際流: {f:?}"
    );
    for cat in ["1", "2", "3"] {
        assert!(
            f.iter().any(|t| t == cat),
            "strict：類別標籤 {cat:?} 必須是獨立段（LO x 軸實測 9pt 三條）。\
             實際流: {f:?}"
        );
    }
    assert!(
        contains(&f, "A=πr2"),
        "strict：公式 part 的 `mi`/`mn`/`mo` 文件序字形串必須入流（LO 實測 A=π r²）。\
         實際流: {f:?}"
    );

    // absence（釘在宿主基線計數；修復前後都必須綠＝反超發臂）。
    assert_eq!(
        char_count(&f, ';'),
        HOST_SEMI_STRICT,
        "strict：`;`（chart label-separator）LO 永不渲染，不得由物件面加入"
    );
    assert_eq!(
        count_of(&f, "序列"),
        0,
        "strict：本圖表**無** `chart:legend` ⇒ 序列名 `序列1` LO 不渲染，\
         不得入流（卡面鐵律：一股腦倒 chart table 會從 −45 反向超發）"
    );
    for bad in ["{", "}", "^", "StarMath"] {
        assert_eq!(
            count_of(&f, bad),
            0,
            "strict：公式 `<annotation>`（StarMath 原始碼）LO 永不渲染，\
             {bad:?} 不得入流"
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 2：chartup——有 legend ⇒ 序列名發；date serial 類別不發
// ---------------------------------------------------------------------------

#[test]
fn lamp2_chartup_series_names_and_no_date_serial() {
    let f = text_flow(CHARTUP);

    assert!(
        contains(&f, "圖表標題"),
        "chartup：圖表標題必須入流（LO 實測渲染）。實際流: {f:?}"
    );
    for s in ["Series 1", "Series 2", "Series 3"] {
        assert!(
            contains(&f, s),
            "chartup：本圖表**有** `chart:legend` ⇒ 序列名 {s:?} 必須入流\
             （LO 圖例實測三條）。實際流: {f:?}"
        );
    }
    for bad in ["41518", "41519", "41520", "41521", "4.3", "2.4"] {
        assert_eq!(
            count_of(&f, bad),
            0,
            "chartup：{bad:?} 是 part 內的 date serial／浮點值，LO 不逐字渲染\
             （實測渲染 `1-Sep-13` 等合成日期），我方不得虛構"
        );
    }
    assert_eq!(
        char_count(&f, ';'),
        HOST_SEMI_CHARTUP,
        "chartup：`;`（label-separator）不得由物件面加入"
    );
}

// ---------------------------------------------------------------------------
// 燈 3：fdo76098——圓餅圖的圖例就是類別，序列名不重複發
// ---------------------------------------------------------------------------

#[test]
fn lamp3_fdo76098_pie_legend_is_categories() {
    let f = text_flow(FDO76098);

    assert!(
        contains(&f, "Sales"),
        "fdo76098：圖表標題 `Sales` 必須入流（LO 18pt 實測）。實際流: {f:?}"
    );
    // 序列名也是 `Sales`，LO 端只出現 1 次（像素覆核：圖例項是季度名）⇒
    // 我方發射面 `Sales` 恰好 1 段（基線宿主 0 段，全來自標題）。
    assert_eq!(
        count_of(&f, "Sales"),
        HOST_SALES_FDO + 1,
        "fdo76098：圓餅圖的圖例項就是類別（像素實測圖例寫 `1st Qtr`… 非序列名），\
         序列名 `Sales` 不得另發一次 ⇒ `Sales` 恰好 1 段"
    );
    for cat in ["1st Qtr", "2nd Qtr", "3rd Qtr", "4th Qtr"] {
        assert!(
            f.iter().any(|t| t == cat),
            "fdo76098：類別標籤 {cat:?} 必須是獨立段（LO 圖例實測）。實際流: {f:?}"
        );
    }
    for bad in ["8.2", "3.2", "1.4", "1.2"] {
        assert_eq!(
            count_of(&f, bad),
            0,
            "fdo76098：圓餅數值 {bad:?} LO 不渲染（像素覆核切片上零數字），不得入流"
        );
    }
    assert_eq!(
        char_count(&f, ';'),
        HOST_SEMI_FDO,
        "fdo76098：`;` 不得由物件面加入"
    );
}

// ---------------------------------------------------------------------------
// 燈 4：報告點名的最大席＋巢裝試算表＋第二份公式
// ---------------------------------------------------------------------------

/// `SolarOverview`（報告點名「最大席 368 字」）：類別 9 條＋序列名 3 條必發，
/// 數值一律不發。序列名用大寫 `Closest` 形——宿主表格是小寫 `closest`，
/// 字面差就是「這條來自 part 而非宿主」的判別器。
#[test]
fn lamp4a_solar_overview_chart_face() {
    let f = text_flow(SOLAR);

    for cat in [
        "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune", "Pluto",
    ] {
        assert!(
            contains(&f, cat),
            "solar：行星類別 {cat:?} 必須在流內（LO 圖表 x 軸實測九條）"
        );
    }
    assert!(
        contains(&f, "Perihelion (Closest) (km)"),
        "solar：有 legend ⇒ 序列名 `Perihelion (Closest) (km)`（大寫形）必須入流；\
         宿主表格只有小寫 `Perihelion (closest) (km)`，故本斷言只能由 part 滿足。\
         實際流: {f:?}"
    );
    assert!(
        contains(&f, "Average Distance (km)"),
        "solar：序列名 `Average Distance (km)` 必須入流（LO 圖例實測）"
    );
    assert!(
        contains(&f, "Aphelion (Farthest) (km)"),
        "solar：序列名 `Aphelion (Farthest) (km)` 必須入流（LO 圖例實測）"
    );
    for bad in ["57909175", "46000000", "69820000", "7375930000"] {
        assert_eq!(
            count_of(&f, bad),
            0,
            "solar：數值 {bad:?} LO 不逐字渲染，不得入流（超發即本席反向炸）"
        );
    }
    // absence 釘在宿主基線（solar 正文自帶 1 個 `;`）：物件面必須貢獻 0 個。
    assert_eq!(
        char_count(&f, ';'),
        HOST_SEMI_SOLAR,
        "solar：物件面不得加入任何 `;`（宿主正文已實測有 {HOST_SEMI_SOLAR} 個）"
    );
}

/// `WordWithAttachments` Object 1（巢裝試算表）：兩個非空儲存格必發。
#[test]
fn lamp4b_word_with_attachments_spreadsheet_cells() {
    let f = text_flow(WORDATT);

    assert!(
        f.iter().any(|t| t == "это ooxml"),
        "wordatt：巢裝試算表儲存格 `это ooxml` 必須是獨立段（LO 實測 y=229.2）。\
         實際流: {f:?}"
    );
    assert!(
        f.iter().any(|t| t == "this is ooxml"),
        "wordatt：巢裝試算表儲存格 `this is ooxml` 必須是獨立段（LO 實測 y=246.2）。\
         實際流: {f:?}"
    );
}

/// `HC031` Object 1（公式）：22 字形串必發；`<annotation>` 一律不發。
#[test]
fn lamp4c_hc031_formula_glyphs_without_annotation() {
    let f = text_flow(HC031);

    assert!(
        contains(&f, "(x+a)n=∑k=0n(nk)xkan−k"),
        "hc031：公式 `mi`/`mn`/`mo` 文件序字形串必須入流（LO page 4 實測 `(x+a)` \
         `n=∑` `k=0` …）。實際流: {f:?}"
    );
    for bad in ["{", "}", "StarMath", "sum from", "binom"] {
        assert_eq!(
            count_of(&f, bad),
            0,
            "hc031：`<annotation>`（`{{left (x+a right )}}`… StarMath 明文）\
             LO 不渲染，{bad:?} 不得入流"
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 5：宿主段落不得被物件文字污染
// ---------------------------------------------------------------------------

#[test]
fn lamp5_host_paragraphs_not_polluted() {
    let h = host_flow(STRICT);
    for host in [
        "Hello world!",
        "This is a chart:",
        "And this is a smart-art:",
        "This is math:",
    ] {
        assert!(
            h.iter().any(|t| t == host),
            "strict：宿主段 {host:?} 必須原樣存在（物件文字不得拼接進入）。實際流: {h:?}"
        );
        assert!(
            !h.iter().any(|t| t.starts_with(host) && t != host),
            "strict：宿主段 {host:?} 後面不得被接上物件文字（物件段必須獨立）"
        );
    }
    // 宿主流本身絕不得出現物件面字面（修復前後同口徑）。
    for bad in ["圖表標題", "序列", "A=π"] {
        assert_eq!(
            count_of(&h, bad),
            0,
            "strict：宿主段落流不得被物件文字污染（{bad:?}）"
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 6：零誤傷——宿主計數必須與 ODF-C3 基準逐檔一致（物件段另立帳）
// ---------------------------------------------------------------------------

/// `(檔, text:p, text:h)` 取自 `odt_layout_projection.rs` 的 `EXPECTED_COUNTS`
/// （ODF-C3 計數基準，事後不得改）：含物件者 6 檔＋無物件者 6 檔對照。
#[test]
fn lamp6_host_paragraph_counts_untouched() {
    const BASE: [(&str, usize, usize); 12] = [
        (STRICT, 6, 0),
        (CHARTUP, 1, 0),
        (FDO76098, 46, 0),
        (WORDATT, 61, 0),
        (HC031, 141, 10),
        (SOLAR, 61, 1),
        // 對照組（無 `draw:object`）
        ("lo_out/lo__ooxmlexport__mce-nested.odt", 8, 0),
        ("lo_out/lo__ooxmlimport__tdf87533_bidi.odt", 6, 0),
        ("ext/lo_core_sw_qa_odfimport_ooo32780-1.odt", 32, 0),
        ("lo_out/lo__ooxmlexport__tdf124637_sectionMargin.odt", 26, 4),
        ("lo_out/lo__selfgen__embedded_font_odttf.odt", 1, 0),
        ("lo_out/lo__ooxmlexport__fdo69616.odt", 4, 0),
    ];
    for (rel, p, h) in BASE {
        let proj = project_host(rel);
        assert_eq!(proj.text_p_count, p, "{rel}：text:p 計數不得被本卡變動");
        assert_eq!(proj.text_h_count, h, "{rel}：text:h 計數不得被本卡變動");
        assert_eq!(
            proj.paragraph_count(),
            p + h,
            "{rel}：宿主段落數必須等於 text:p＋text:h（物件段另立帳，\
             不得擠掉或重複計數宿主段）"
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 7：鑑別力回塞鎖（ODF_E2E_BUGGY=1）
// ---------------------------------------------------------------------------

/// 回塞態＝「整個 part 的所有 `text:p` 一股腦倒進段落流」（報告 §3.4 點名的
/// 反向超發事故形態）。此時 presence 臂仍綠、absence／計數釘必紅 ⇒ 證明
/// absence 鎖精確命中取字鐵律（不是寫死期望值）。
#[test]
fn lamp7_buggy_rollback_breaks_only_absence_locks() {
    let buggy = std::env::var_os("ODF_E2E_BUGGY").is_some();
    let f = text_flow(STRICT);
    let title = contains(&f, "圖表標題");
    let semi = char_count(&f, ';');
    let series = contains(&f, "序列");

    assert!(
        title,
        "presence 臂兩態都必須綠（回塞只動 absence 面）；buggy={buggy}，流: {f:?}"
    );
    if buggy {
        assert!(
            semi > 0 && series,
            "燈 7 鑑別力：ODF_E2E_BUGGY=1（一股腦倒 part text:p）態下 `;` 與序列名 \
             `序列1` 必須混入（`;`={semi}、序列={series}）——absence 鎖在此態若仍綠，\
             代表該鎖根本沒有鑑別力"
        );
    } else {
        assert_eq!(semi, HOST_SEMI_STRICT, "正常態：`;` 必須不在流內");
        assert!(!series, "正常態：無 legend 的序列名 `序列1` 必須不在流內");
    }
}
