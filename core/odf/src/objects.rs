//! ODF-E2e：嵌入物件 part（`Object*/content.xml`）取字。
//!
//! 缺陷（`docs/odf-survey/e2-flow-attribution.md` §2 D5／§3.4）：`content.xml`
//! 的 `draw:object`（`./Object N`）指向巢裝 part——圖表／數學公式／巢裝試算表
//! ——既有 walker 只讀宿主 `text:p`／`text:h`，part 內文字一個不讀，而 LO 會把
//! 其中**一部分**渲染進版面。
//!
//! ## 取字鐵律：LO 只渲染 part 文字的子集
//!
//! 把整個 part 的段落一股腦倒進文字流＝反向超發（報告 §3.4 實名點名的事故形態：
//! `strict` 的 Object 2 part 內文含 `;圖表標題序列1 1 1 2 2 3 3` 共 19 字，
//! LO 實際只發 `圖表標題`＋類別標籤 `1` `2` `3`，`;`（label-separator）與
//! `序列1`（序列名）LO 不渲染）。本模組按「LO 發射面」取字，六檔 part 全覆蓋
//! （LO 24.8.0.3 `--convert-to pdf`＋pymupdf 逐行＋150dpi 像素肉眼覆核，
//! 對照組明細見考卷 `tests/odf_e2e_object_parts.rs` 檔頭表）：
//!
//! 1. **圖表**（media `…opendocument.chart`）：
//!    * `chart:title` 內的 `text:p` 逐段發（實測：`strict`／`chartup` 標題
//!      `圖表標題`、`fdo76098` 標題 `Sales`）。
//!    * `chart:categories/@table:cell-range-address` 範圍內的**字串值**儲存格
//!      逐格發（LO 排成類別標籤：`strict` `1` `2` `3`、`solar` 九行星、
//!      `fdo76098` 四季）。**浮點值**儲存格不發——LO 依資料自動算軸刻度／日期
//!      （`41518`→`1-Sep-13`、Y 軸 `0.5` 小數刻度），那是 LO 端合成，
//!      `docs/odf-survey/README.md` §11 明文非我方目標。
//!    * 序列名（`chart:series/@chart:label-cell-address` 指向的儲存格）**僅在**
//!      有 `<chart:legend>` **且**圖表類別非 `chart:circle` 時發。三態實測對照：
//!      `strict` 無 legend ⇒ `序列1` 不發（卡面鐵律）；`chartup`/`solar` 有
//!      legend ⇒ 序列名發；`fdo76098` 圓餅的圖例項就是類別 ⇒ 序列名 `Sales`
//!      不重複發（LO PDF `Sales` 只出現一次＝標題）。
//!    * `chart:label-separator`（`; `）、數值欄、空白角落格一律不發。
//! 2. **數學公式**（media `…opendocument.formula`）：只取 `mi`/`mn`/`mo` 字形，
//!    按文件序串接成**一段**（LO 把它們排在同一基線區）。`annotation`
//!    （StarMath 原始碼，含 `{` `}` `^`）一律不發——LO 從不渲染 annotation
//!    （實測 `strict` Object 3 發 `A=πr2` 五字形、`hc031` Object 1 發 22 字形，
//!    兩者 annotation 皆零發射）。
//! 3. **巢裝試算表**（media `…opendocument.spreadsheet`）：只取**非空**
//!    `table:table-cell` 文字，逐格一段（`wordatt` Object 1 的 `это ooxml`／
//!    `this is ooxml`——LO 實測各排一行）；自閉合空值格與 `number-columns-repeated`
//!    撐出來的空格一律不發（`wordatt` 有 16383 個，照發即萬字超發）。
//!
//! ## 邊界（老實記帳，不虛構）
//!
//! * **只一層**：只解析宿主 `content.xml` 直接引用的 part，不遞迴打開 part 內
//!   再巢裝的 `draw:object`（語料 0 例；要遞迴得先有真值來源）。
//! * **以引用為準**：只認 `draw:object/@xlink:href`（LO 對未被引用的 Object
//!   目錄不渲染；`ObjectReplacements/` 是 `draw:image` 的替代圖，不是文字源）。
//! * **渲染軸不做**：物件文字的座標／字級由 LO 依圖表座標系計算，本切片不產生
//!   座標，故物件段不進 `paragraphs`（宿主段落流）也不進 `flatten()`（版面／
//!   斷頁口徑屬 E1／C4 工作面），單獨入帳 `OdfProjection::object_paragraphs`
//!   與 `object_text_len`（文字流對帳軸——本卡缺陷的正主；宿主
//!   `total_text_len` 零變動，C3 計數基準與歸因報告 §4 重驗清單因此不破）。
//! * **加密／缺檔 part 跳過**：manifest 宣告加密的物件 part 不強讀（語料 0 例）；
//!   引用了但 zip 內沒有 `content.xml`（`./Obj100` 這類 OLE 懸空引用）跳過。
//! * **回塞態** `ODF_E2E_BUGGY=1`：把每個被引用 part 的所有 `text:p`／`text:h`
//!   一股腦倒出來（本缺陷的相反極端），供考卷燈 7 證 absence 鎖的鑑別力。

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::OdfError;

/// 嵌入物件 part 的型別（先查 manifest 媒體型別，缺時嗅 part 內容）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    /// 圖表（`chart:chart`）。
    Chart,
    /// 數學公式（MathML `math:math`）。
    Formula,
    /// 巢裝試算表（`office:spreadsheet`）。
    Spreadsheet,
    /// 其他／不可判定——一律零發射（不虛構 LO 發射面）。
    Other,
}

/// 嵌入物件 part 的一條發射文字（獨立段，不混入宿主 `paragraphs`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdfObjectText {
    /// 來源 part 目錄名（`Object 2`／`Object 3`…）。
    pub part: String,
    /// part 型別。
    pub kind: ObjectKind,
    /// 發射文字（一段）。
    pub text: String,
}

const MEDIA_CHART: &str = "application/vnd.oasis.opendocument.chart";
const MEDIA_FORMULA: &str = "application/vnd.oasis.opendocument.formula";
const MEDIA_SPREADSHEET: &str = "application/vnd.oasis.opendocument.spreadsheet";

/// 回塞態旗標：`ODF_E2E_BUGGY=1` ⇒ 物件取字打回「整個 part 所有 `text:p`
/// 倒流」舊態（考卷燈 7 的鑑別力臂；正常態永不觸發）。
fn buggy_dump() -> bool {
    std::env::var_os("ODF_E2E_BUGGY").is_some()
}

/// 自宿主 `content.xml` 依文件序收集被引用的物件 part 目錄名。
///
/// 只認 `draw:object` 的 `xlink:href`（LO 對未被引用的 Object 目錄不渲染）。
/// 回傳已正規化的目錄名（`./Object 2` → `Object 2`），同檔重複引用只留首次。
pub fn object_refs(content_xml: &[u8]) -> Result<Vec<String>, OdfError> {
    let mut reader = Reader::from_reader(content_xml);
    let mut buf = Vec::new();
    let mut out: Vec<String> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if !is_draw_object(e.name().as_ref()) {
                    continue;
                }
                let Some(href) = attr_local(&e, b"href") else {
                    continue;
                };
                let dir = normalize_object_href(&href);
                if !dir.is_empty() && !out.iter().any(|d| d == &dir) {
                    out.push(dir);
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(OdfError::Xml(err.to_string())),
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

/// `xlink:href` → part 目錄名：去 `./` 前綴與尾斜線；`X/content.xml` 退到 `X`。
/// 非容器內引用（含 `://`、`../`、絕對路徑）回空字串（跳過）。
fn normalize_object_href(href: &str) -> String {
    let mut s = href.trim();
    if s.contains("://") || s.starts_with("../") || s.starts_with('/') {
        return String::new();
    }
    if let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }
    if let Some(rest) = s.strip_suffix("/content.xml") {
        s = rest;
    }
    while let Some(rest) = s.strip_suffix('/') {
        s = rest;
    }
    s.to_string()
}

#[inline]
fn is_draw_object(name: &[u8]) -> bool {
    name == b"draw:object" || name == b"object"
}

/// 讀一個物件 part 並回傳其發射文字（依型別走 LO 發射面規則）。
///
/// `part_dir`＝容器目錄名（`Object 2`）；`media`＝manifest 宣告的媒體型別
/// （缺＝`None`，改嗅 part 內容）。破損 part 的 XML 錯誤照實上報（不吞錯，
/// 口徑同 `project`）；型別不可判定者零發射（不虛構）。
pub fn object_texts(
    part_dir: &str,
    bytes: &[u8],
    media: Option<&str>,
) -> Result<Vec<OdfObjectText>, OdfError> {
    let kind = classify(media, bytes);
    let texts: Vec<String> = if buggy_dump() {
        // 回塞態：不分型別，把 part 的所有 text:p／text:h 一股腦倒出來。
        dump_all_paragraphs(bytes)?
    } else {
        match kind {
            ObjectKind::Chart => chart_face(bytes)?,
            ObjectKind::Formula => formula_face(bytes)?,
            ObjectKind::Spreadsheet => spreadsheet_face(bytes)?,
            ObjectKind::Other => Vec::new(),
        }
    };
    Ok(texts
        .into_iter()
        .filter(|t| !t.is_empty())
        .map(|text| OdfObjectText {
            part: part_dir.to_string(),
            kind,
            text,
        })
        .collect())
}

/// 型別判定：manifest 媒體型別優先，缺時嗅 part 內容特徵字面
/// （root 元素都是 `office:document-content`，故嗅內容而非 root）。
fn classify(media: Option<&str>, bytes: &[u8]) -> ObjectKind {
    match media {
        Some(m) if m == MEDIA_CHART => return ObjectKind::Chart,
        Some(m) if m == MEDIA_FORMULA => return ObjectKind::Formula,
        Some(m) if m == MEDIA_SPREADSHEET => return ObjectKind::Spreadsheet,
        _ => {}
    }
    let text = String::from_utf8_lossy(bytes);
    if text.contains("office:spreadsheet") {
        ObjectKind::Spreadsheet
    } else if text.contains("chart:chart") {
        ObjectKind::Chart
    } else if text.contains("MathML") {
        ObjectKind::Formula
    } else {
        ObjectKind::Other
    }
}

// ---------------------------------------------------------------------------
// 圖表 part：LO 發射面（標題＋字串類別＋條件性序列名）
// ---------------------------------------------------------------------------

/// `ChartMeta`：圖表 part 的發射面判定資訊（一趟走查收齊）。
struct ChartMeta {
    /// `chart:title` 內各 `text:p` 的文字（標題段）。
    titles: Vec<String>,
    has_legend: bool,
    is_circle: bool,
    /// `chart:categories` 範圍 `(col1, row1, col2, row2)`（只取首個）。
    categories: Option<(usize, usize, usize, usize)>,
    /// 各 `chart:series/@chart:label-cell-address` 解析出的 `(欄, 列)`。
    series_labels: Vec<(usize, usize)>,
}

/// 圖表 part 的發射文字（標題段→類別段→序列名段）。
fn chart_face(bytes: &[u8]) -> Result<Vec<String>, OdfError> {
    let meta = chart_meta(bytes)?;
    let mut out: Vec<String> = Vec::new();
    out.extend(meta.titles.into_iter().filter(|t| !t.is_empty()));

    if meta.categories.is_some() || !meta.series_labels.is_empty() {
        let table = chart_table(bytes)?;
        // 類別標籤：範圍內**字串值**儲存格（浮點值是 LO 合成刻度／日期的原料）。
        if let Some((c1, r1, c2, r2)) = meta.categories {
            for col in c1..=c2 {
                for row in r1..=r2 {
                    if let Some((text, numeric)) = table.get(&(col, row)) {
                        if !*numeric && !text.is_empty() {
                            out.push(text.clone());
                        }
                    }
                }
            }
        }
        // 序列名：有 legend 且非圓餅圖才發（實測三態——見模組文件第 1 條）。
        if meta.has_legend && !meta.is_circle {
            for (col, row) in meta.series_labels {
                if let Some((text, _)) = table.get(&(col, row)) {
                    if !text.is_empty() {
                        out.push(text.clone());
                    }
                }
            }
        }
    }
    Ok(out)
}

/// 走一遍圖表 part，收標題／legend／圖表類別／categories／series 標格位址。
fn chart_meta(bytes: &[u8]) -> Result<ChartMeta, OdfError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();

    let mut titles: Vec<String> = Vec::new();
    let mut has_legend = false;
    let mut is_circle = false;
    let mut categories: Option<(usize, usize, usize, usize)> = None;
    let mut series_labels: Vec<(usize, usize)> = Vec::new();

    // 巢裝深度：chart:title（其內 text:p 才收）與 text:p（取字上下文）。
    let mut depth_title = 0usize;
    let mut depth_para = 0usize;
    let mut para = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if name == b"chart:title" || name == b"title" {
                    depth_title += 1;
                } else if name == b"chart:chart" || name == b"chart" {
                    if let Some(cls) = attr_local(&e, b"class") {
                        is_circle |= cls.contains("circle");
                    }
                } else if name == b"chart:legend" || name == b"legend" {
                    has_legend = true;
                } else if name == b"chart:series" || name == b"series" {
                    if let Some(addr) = attr_local(&e, b"label-cell-address") {
                        if let Some(cell) = parse_cell_ref(&addr) {
                            series_labels.push(cell);
                        }
                    }
                } else if is_categories(name) && categories.is_none() {
                    if let Some(range) = attr_local(&e, b"cell-range-address") {
                        categories = parse_cell_range(&range);
                    }
                }
                if is_text_para(name) && depth_title > 0 {
                    depth_para += 1;
                    if depth_para == 1 {
                        para.clear();
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if name == b"chart:legend" || name == b"legend" {
                    has_legend = true;
                } else if name == b"chart:series" || name == b"series" {
                    if let Some(addr) = attr_local(&e, b"label-cell-address") {
                        if let Some(cell) = parse_cell_ref(&addr) {
                            series_labels.push(cell);
                        }
                    }
                } else if is_categories(name) && categories.is_none() {
                    if let Some(range) = attr_local(&e, b"cell-range-address") {
                        categories = parse_cell_range(&range);
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if depth_para > 0 {
                    para.push_str(&decode_text(&e)?);
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if depth_para > 0 {
                    para.push_str(&crate::body::resolve_general_ref(&e)?);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if is_text_para(name) && depth_title > 0 && depth_para > 0 {
                    depth_para -= 1;
                    if depth_para == 0 {
                        titles.push(std::mem::take(&mut para).trim().to_string());
                    }
                } else if name == b"chart:title" || name == b"title" {
                    depth_title = depth_title.saturating_sub(1);
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(OdfError::Xml(err.to_string())),
            _ => {}
        }
        buf.clear();
    }
    Ok(ChartMeta {
        titles,
        has_legend,
        is_circle,
        categories,
        series_labels,
    })
}

#[inline]
fn is_text_para(name: &[u8]) -> bool {
    name == b"text:p" || name == b"p" || name == b"text:h" || name == b"h"
}

#[inline]
fn is_categories(name: &[u8]) -> bool {
    name == b"chart:categories" || name == b"categories"
}

/// 圖表 local-table 一格的發射面判定：（文字, 是否數值型）。
type ChartCell = (String, bool);

/// 圖表儲存格表：`(欄, 列) → 格`（座標 1 起始）。
type ChartCells = HashMap<(usize, usize), ChartCell>;

/// 圖表 `table:table`（local-table）儲存格表：`(欄, 列) → (文字, 是否數值)`。
///
/// 座標口徑與 ODF 儲存格位址一致（`$A$2`＝欄 1 列 2，1 起始）：列號按
/// `table:table-header-rows`＋`table:table-rows` 的 `table:table-row` 文件序
/// 連續累計；欄號按列內儲存格的 `number-columns-repeated` 累計（角頭欄／列
/// 天然計入，實測與 `local-table.$B$1` 等宣告位址逐檔對齊）。
fn chart_table(bytes: &[u8]) -> Result<ChartCells, OdfError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();

    let mut cells: ChartCells = HashMap::new();
    let mut in_table = false;
    let mut row_index = 0usize;
    let mut col_index = 0usize;
    let mut in_cell = false;
    let mut cell_numeric = false;
    let mut cell_text = String::new();
    let mut cell_col = 0usize;
    let mut cell_row = 0usize;
    let mut depth_cell = 0usize;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if name == b"table:table" || name == b"table" {
                    in_table = true;
                    row_index = 0;
                    continue;
                }
                if !in_table {
                    continue;
                }
                if name == b"table:table-row" || name == b"table-row" {
                    row_index += 1;
                    col_index = 0;
                } else if is_calc_cell(name) {
                    let repeated = repeated_count(&e);
                    col_index += repeated;
                    cell_col = col_index;
                    cell_row = row_index;
                    let vt = attr_local(&e, b"value-type");
                    cell_numeric = matches!(
                        vt.as_deref(),
                        Some("float")
                            | Some("percentage")
                            | Some("currency")
                            | Some("date")
                            | Some("time")
                    );
                    cell_text.clear();
                    in_cell = true;
                    depth_cell = 1;
                } else if in_cell {
                    depth_cell += 1;
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if in_table && is_calc_cell(name) {
                    // 自閉合空值格：只佔欄位，不入表（空文字發射面不發）。
                    col_index += repeated_count(&e);
                }
            }
            Ok(Event::Text(e)) => {
                if in_cell {
                    cell_text.push_str(&decode_text(&e)?);
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if in_cell {
                    cell_text.push_str(&crate::body::resolve_general_ref(&e)?);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if name == b"table:table" || name == b"table" {
                    in_table = false;
                    continue;
                }
                if !in_table {
                    continue;
                }
                if is_calc_cell(name) {
                    depth_cell = depth_cell.saturating_sub(1);
                    if depth_cell == 0 && in_cell {
                        let text = cell_text.trim().to_string();
                        if !text.is_empty() {
                            cells.insert((cell_col, cell_row), (text, cell_numeric));
                        }
                        in_cell = false;
                    }
                } else if in_cell {
                    depth_cell = depth_cell.saturating_sub(1);
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(OdfError::Xml(err.to_string())),
            _ => {}
        }
        buf.clear();
    }
    Ok(cells)
}

/// `table:number-columns-repeated`（缺／非數＝1）。
fn repeated_count(e: &quick_xml::events::BytesStart<'_>) -> usize {
    attr_local(e, b"number-columns-repeated")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(1)
}

/// 解析單格位址 `local-table.$B$1`／`.$B$10` → `(欄, 列)`（1 起始）。
fn parse_cell_ref(addr: &str) -> Option<(usize, usize)> {
    let tail = addr.rsplit('.').next().filter(|s| !s.is_empty())?;
    let mut col = 0usize;
    let mut row = String::new();
    for ch in tail.chars() {
        if ch.is_ascii_alphabetic() {
            col = col * 26 + (ch.to_ascii_uppercase() as usize - 'A' as usize + 1);
        } else if ch.is_ascii_digit() {
            row.push(ch);
        }
    }
    if col == 0 || row.is_empty() {
        return None;
    }
    Some((col, row.parse::<usize>().ok()?))
}

/// 解析儲存格範圍 `local-table.$A$2:.$A$4` → `(col1, row1, col2, row2)`。
fn parse_cell_range(range: &str) -> Option<(usize, usize, usize, usize)> {
    let mut it = range.split(':');
    let (c1, r1) = parse_cell_ref(it.next()?)?;
    let (c2, r2) = match it.next() {
        Some(second) => parse_cell_ref(second)?,
        None => (c1, r1),
    };
    Some((c1.min(c2), r1.min(r2), c1.max(c2), r1.max(r2)))
}

// ---------------------------------------------------------------------------
// 公式 part：只取 MathML mi/mn/mo 字形（annotation 不發）
// ---------------------------------------------------------------------------

/// 公式 part 的發射文字：`mi`/`mn`/`mo` 字形按文件序串接成一段。
fn formula_face(bytes: &[u8]) -> Result<Vec<String>, OdfError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();
    let mut glyph_depth = 0usize;
    let mut out = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if is_math_glyph(e.name().as_ref()) {
                    glyph_depth += 1;
                }
            }
            Ok(Event::Text(e)) => {
                if glyph_depth > 0 {
                    out.push_str(&decode_text(&e)?);
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if glyph_depth > 0 {
                    out.push_str(&crate::body::resolve_general_ref(&e)?);
                }
            }
            Ok(Event::End(e)) => {
                if is_math_glyph(e.name().as_ref()) {
                    glyph_depth = glyph_depth.saturating_sub(1);
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(OdfError::Xml(err.to_string())),
            _ => {}
        }
        buf.clear();
    }
    let text = out.trim().to_string();
    Ok(if text.is_empty() {
        Vec::new()
    } else {
        vec![text]
    })
}

/// MathML 字形元素：`mi`（識別符）／`mn`（數）／`mo`（運算子）。
/// `mrow`／`msup`／`annotation` 等不算字形（其文字由子層字形自己報到；
/// annotation 的 StarMath 原始碼因此天然排除）。
#[inline]
fn is_math_glyph(name: &[u8]) -> bool {
    matches!(
        name,
        b"mi" | b"mn" | b"mo" | b"math:mi" | b"math:mn" | b"math:mo"
    )
}

// ---------------------------------------------------------------------------
// 巢裝試算表 part：只取非空儲存格（自閉合空格不發）
// ---------------------------------------------------------------------------

/// 巢裝試算表 part 的發射文字：非空 `table:table-cell` 逐格一段。
fn spreadsheet_face(bytes: &[u8]) -> Result<Vec<String>, OdfError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();
    let mut out: Vec<String> = Vec::new();

    let mut in_cell = false;
    let mut depth_cell = 0usize;
    let mut cell = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if is_calc_cell(name) {
                    in_cell = true;
                    depth_cell = 1;
                    cell.clear();
                } else if in_cell {
                    depth_cell += 1;
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name();
                // 自閉合格＝空值（covered／repeated 空格走這條），不發。
                let _ = name;
            }
            Ok(Event::Text(e)) => {
                if in_cell {
                    cell.push_str(&decode_text(&e)?);
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if in_cell {
                    cell.push_str(&crate::body::resolve_general_ref(&e)?);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if is_calc_cell(name) {
                    depth_cell = depth_cell.saturating_sub(1);
                    if depth_cell == 0 && in_cell {
                        let text = cell.trim().to_string();
                        if !text.is_empty() {
                            out.push(text);
                        }
                        in_cell = false;
                    }
                } else if in_cell {
                    depth_cell = depth_cell.saturating_sub(1);
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(OdfError::Xml(err.to_string())),
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

/// 計算類儲存格標籤（`covered-table-cell` 刻意排除——覆蓋格不發文字）。
#[inline]
fn is_calc_cell(name: &[u8]) -> bool {
    matches!(
        name,
        b"table:table-cell" | b"table-cell" | b"table:table-header-cell" | b"table-header-cell"
    )
}

/// 回塞態：part 內所有 `text:p`／`text:h` 的文字（不做任何發射面篩選）。
fn dump_all_paragraphs(bytes: &[u8]) -> Result<Vec<String>, OdfError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buf = Vec::new();
    let mut out: Vec<String> = Vec::new();
    let mut depth = 0usize;
    let mut cur = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if is_text_para(e.name().as_ref()) {
                    depth += 1;
                    if depth == 1 {
                        cur.clear();
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if depth > 0 {
                    cur.push_str(&decode_text(&e)?);
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if depth > 0 {
                    cur.push_str(&crate::body::resolve_general_ref(&e)?);
                }
            }
            Ok(Event::End(e)) => {
                if is_text_para(e.name().as_ref()) && depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        out.push(std::mem::take(&mut cur).trim().to_string());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(OdfError::Xml(err.to_string())),
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------

/// Text 事件解碼＋一般實體還原（口徑同 `body.rs`／`layout_glue.rs`）。
fn decode_text(e: &quick_xml::events::BytesText<'_>) -> Result<String, OdfError> {
    let decoded = e.decode().map_err(|e| OdfError::Xml(e.to_string()))?;
    quick_xml::escape::unescape(&decoded)
        .map(|u| u.into_owned())
        .map_err(|e| OdfError::Xml(e.to_string()))
}

/// 依**本地名**取屬性值（命名空間前綴不敏感，同既有 walker 口徑）。
fn attr_local(e: &quick_xml::events::BytesStart<'_>, local: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == local {
            return Some(String::from_utf8_lossy(&attr.value).into_owned());
        }
    }
    None
}
